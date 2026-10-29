//! A [`truck`](https://github.com/ricosjp/truck)-backed implementation of
//! [`cad_contract::kernel::Kernel`].
//!
//! This adapter is the proof that the contract describes a real kernel: every capability it
//! advertises is backed by an operation exercised in the tests below, and every operation
//! validates its input before it reaches truck.
//!
//! # What this is not
//!
//! This is **not** a replacement for SolidCraft's `crates/kernel` (94 803 LOC of fillets, chamfers,
//! healing, sheet metal, STEP/IGES and history tracking). It is the small, shared subset — extrude,
//! the three booleans, tessellation — that a second application can use without adopting the whole
//! parametric stack, expressed in the vocabulary both apps can agree on.
//!
//! # Panics
//!
//! A panic inside truck would otherwise take the application down. Every call into truck runs
//! under [`guarded`], which turns a panic into [`KernelError::Internal`]; this mirrors the
//! `guard` SolidCraft already ships. No method in this crate panics on caller input.
//!
//! # Thread safety
//!
//! `TruckKernel` is `Send` but not `Sync`: truck's topology handles are reference-counted and its
//! boolean operations are not documented as reentrant. One kernel instance must be used from one
//! thread at a time (see `docs/security-review.md`, R-7).
//!
//! # Cancellation and deadlines
//!
//! [`KernelError::Cancelled`] and [`KernelError::Timeout`] existed in the contract but nothing set
//! them, so a UI could not abandon a long boolean and an agent runtime could not enforce a budget.
//! Every operation that returns a [`Result`] now has a `_with_budget` twin taking a [`Budget`]; the
//! [`Kernel`] methods themselves delegate with [`Budget::unlimited`], so no existing call site
//! changes behaviour.
//!
//! The checks are **cooperative checkpoints**, not preemption. truck is a synchronous C-style
//! geometry library: once `truck_shapeops::or` is entered, control does not come back until it
//! returns. A checkpoint therefore runs at every boundary this crate owns — before an operation,
//! between boolean attempts, around the conservation measurement, and inside the weld and
//! orientation loops, which are the only long loops written here. A single pathological boolean
//! deep inside truck cannot be interrupted; that is stated rather than implied, because a caller
//! that believes otherwise would cancel and wait forever.

use std::collections::{BTreeMap, HashMap, VecDeque};
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::{Duration, Instant};

pub mod corpus;

use cad_contract::capability::{Capability, CapabilitySet, KernelInfo};
use cad_contract::error::{KernelError, Result};
use cad_contract::kernel::{BooleanOp, Kernel, SolidId, validate_profile};
use cad_contract::mesh::Mesh;
use cad_contract::tol::DEFAULT_LINEAR_MM;
use truck_meshalgo::prelude::*;
use truck_modeling::{Point3, Rad, Solid, Vector3, Wire, builder};

/// The smallest tolerance truck's boolean and tessellation code accepts.
///
/// truck asserts `tol > TOLERANCE` (1e-4 in its own units); rather than let that assertion become a
/// caught panic, callers get an `Invalid` error with the bound in the message.
pub const MIN_TOLERANCE_MM: f64 = 1e-4;

/// How far, in millimetres, the boolean retry moves the tool to break a coplanar-face coincidence.
///
/// All three axes get a different, incommensurate offset: two boxes that share a y-extent stay
/// coplanar in y even after a z-only nudge, which is why the first version of this retry did not
/// fix a union of boxes offset along x.
///
/// Measured: a total offset of 1e-6 mm is too small to change truck's face classification and still
/// fails; 1e-5 mm and above succeed. ~1e-4 mm per axis is well below any manufacturing tolerance and
/// changes the result's volume by at most `nudge x tool surface area` (a few thousandths of a mm³
/// for a small tool).
pub const COPLANAR_NUDGE_MM: [f64; 3] = [7.0e-5, 9.0e-5, 1.1e-4];

/// The default tolerance passed to truck's boolean operations, in millimetres.
///
/// This is deliberately **not** the tessellation tolerance. truck's boolean classification is
/// sensitive to it: a value that is large relative to the geometry (0.05 mm on a 10 mm feature)
/// makes face classification fail and the operation return no result. SolidCraft uses
/// scale-relative values of the same magnitude (`size * 1e-5`); 1e-3 mm is a safe default for
/// millimetre-scale CAD parts and [`TruckKernel::with_boolean_tolerance`] overrides it.
pub const DEFAULT_BOOLEAN_TOLERANCE_MM: f64 = 1e-3;

/// A cancellation flag shared between the caller and a running operation.
///
/// Cheap to clone: every clone observes the same flag, so a UI thread can cancel an operation that
/// a worker thread is inside. Dropping the token does **not** cancel anything — cancellation is an
/// explicit act, because a token that cancelled on drop would make every temporary budget lethal.
///
/// A token created by [`CancellationToken::unlimited`] is inert: cancelling it does nothing and
/// `is_cancelled` is always `false`. That is what the plain [`Kernel`] methods pass, so the
/// delegated path allocates nothing and can never be cancelled behind the caller's back.
#[derive(Debug, Clone, Default)]
pub struct CancellationToken {
    flag: Option<Arc<AtomicBool>>,
}

impl CancellationToken {
    /// A real, cancellable token.
    pub fn new() -> Self {
        CancellationToken {
            flag: Some(Arc::new(AtomicBool::new(false))),
        }
    }

    /// An inert token that can never be cancelled.
    pub const fn unlimited() -> Self {
        CancellationToken { flag: None }
    }

    /// Request cancellation. Idempotent, and safe to call from any thread.
    pub fn cancel(&self) {
        if let Some(flag) = &self.flag {
            flag.store(true, Ordering::SeqCst);
        }
    }

    /// Clear the flag so the token can be reused for a new operation.
    pub fn reset(&self) {
        if let Some(flag) = &self.flag {
            flag.store(false, Ordering::SeqCst);
        }
    }

    /// Has cancellation been requested?
    pub fn is_cancelled(&self) -> bool {
        self.flag
            .as_ref()
            .is_some_and(|flag| flag.load(Ordering::SeqCst))
    }
}

/// An absolute point in time after which an operation must stop.
///
/// Absolute rather than a per-call duration so that one budget can be applied to a whole command
/// made of many kernel calls: the deadline does not restart when the command starts its next
/// boolean.
#[derive(Debug, Clone, Copy)]
pub struct Deadline {
    started: Instant,
    limit: Option<Duration>,
}

impl Default for Deadline {
    fn default() -> Self {
        Self::none()
    }
}

impl Deadline {
    /// No deadline: the operation may run as long as it needs.
    pub fn none() -> Self {
        // The start instant is irrelevant without a limit: `remaining` short-circuits to `None`.
        Deadline {
            started: Instant::now(),
            limit: None,
        }
    }

    /// A deadline `timeout` from now.
    pub fn after(timeout: Duration) -> Self {
        Deadline {
            started: Instant::now(),
            limit: Some(timeout),
        }
    }

    /// A deadline at an absolute instant, for callers that already know when the command began.
    pub fn until(deadline: Instant) -> Self {
        let started = Instant::now();
        Deadline {
            started,
            limit: Some(deadline.saturating_duration_since(started)),
        }
    }

    /// Time left, or `None` when there is no deadline.
    pub fn remaining(&self) -> Option<Duration> {
        self.limit
            .map(|limit| limit.saturating_sub(self.started.elapsed()))
    }

    /// How long has run since the deadline was taken.
    pub fn elapsed(&self) -> Duration {
        self.started.elapsed()
    }

    /// Has the deadline passed? A budget with no deadline never has.
    pub fn is_expired(&self) -> bool {
        self.remaining().is_some_and(|left| left.is_zero())
    }
}

/// What an operation must respect: a cancellation flag and a deadline.
///
/// A [`Budget::unlimited`] budget is what the [`Kernel`] trait methods pass, so wrapping an
/// existing call is free of behaviour change. Cancellation is checked before timeout, so a caller
/// that cancels *and* overruns is told the operation was cancelled — that is the answer the UI
/// needs, because the user did that deliberately.
#[derive(Debug, Clone, Default)]
pub struct Budget {
    cancel: Option<CancellationToken>,
    deadline: Deadline,
}

impl Budget {
    /// No cancellation, no deadline.
    pub fn unlimited() -> Self {
        Budget {
            cancel: None,
            deadline: Deadline::none(),
        }
    }

    /// A budget that only enforces a deadline.
    pub fn with_timeout(timeout: Duration) -> Self {
        Budget {
            cancel: None,
            deadline: Deadline::after(timeout),
        }
    }

    /// A budget that only enforces a cancellation flag.
    pub fn with_cancel_token(token: CancellationToken) -> Self {
        Budget {
            cancel: Some(token),
            deadline: Deadline::none(),
        }
    }

    /// A budget with both a flag and a deadline.
    pub fn new(token: CancellationToken, deadline: Deadline) -> Self {
        Budget {
            cancel: Some(token),
            deadline,
        }
    }

    /// The flag in use, or an inert one, so a caller can hand a clone to another thread.
    pub fn token(&self) -> CancellationToken {
        self.cancel.clone().unwrap_or_default()
    }

    /// The deadline in use.
    pub fn deadline(&self) -> Deadline {
        self.deadline
    }

    /// Has the caller cancelled?
    pub fn is_cancelled(&self) -> bool {
        self.cancel
            .as_ref()
            .is_some_and(|token| token.is_cancelled())
    }

    /// The next checkpoint: `Ok(())`, [`KernelError::Cancelled`], or [`KernelError::Timeout`].
    pub fn check(&self) -> Result<()> {
        if self.is_cancelled() {
            return Err(KernelError::Cancelled);
        }
        if self.deadline.is_expired() {
            return Err(KernelError::Timeout(self.deadline.elapsed()));
        }
        Ok(())
    }
}

/// A geometry kernel backed by truck's B-rep modeling and shape operations.
#[derive(Debug)]
pub struct TruckKernel {
    solids: HashMap<u64, Solid>,
    next: u64,
    boolean_tol: f64,
    coplanar_retries: u64,
    // Call counters only. Atomics rather than `Cell` because `Kernel::tessellate` takes `&self`, and
    // `Relaxed` is enough: these are diagnostics, never a synchronisation point.
    extrude_calls: AtomicU64,
    boolean_calls: AtomicU64,
    tessellate_calls: AtomicU64,
}

impl Default for TruckKernel {
    fn default() -> Self {
        Self::new()
    }
}

impl TruckKernel {
    /// A kernel with no solids.
    pub fn new() -> Self {
        TruckKernel {
            solids: HashMap::new(),
            next: 1,
            boolean_tol: DEFAULT_BOOLEAN_TOLERANCE_MM,
            coplanar_retries: 0,
            extrude_calls: AtomicU64::new(0),
            boolean_calls: AtomicU64::new(0),
            tessellate_calls: AtomicU64::new(0),
        }
    }

    /// How many `extrude` operations this instance has run.
    ///
    /// Counted so a cache can be shown to skip work rather than merely *claim* to: a rebuild that
    /// reuses a cached prefix must not increment this. The counter is diagnostic state, not model
    /// state, and is never persisted.
    pub fn extrude_calls(&self) -> u64 {
        self.extrude_calls.load(Ordering::Relaxed)
    }

    /// How many `boolean` operations this instance has run.
    pub fn boolean_calls(&self) -> u64 {
        self.boolean_calls.load(Ordering::Relaxed)
    }

    /// How many `tessellate` operations this instance has run.
    pub fn tessellate_calls(&self) -> u64 {
        self.tessellate_calls.load(Ordering::Relaxed)
    }

    /// A kernel whose boolean operations use `tol` mm.
    ///
    /// Smaller values separate closer geometry but cost more work; values below 1e-6 are clamped.
    pub fn with_boolean_tolerance(mut self, tol: f64) -> Self {
        if tol.is_finite() && tol > 0.0 {
            self.boolean_tol = tol.max(1e-6);
        }
        self
    }

    /// The boolean tolerance in use, in millimetres.
    pub fn boolean_tolerance(&self) -> f64 {
        self.boolean_tol
    }

    /// How many boolean operations needed the coplanar-face retry.
    ///
    /// Observable on purpose: a test can assert that the retry path is exercised, and a benchmark
    /// can report how often real geometry needs it instead of leaving it as folklore.
    pub fn coplanar_retries(&self) -> u64 {
        self.coplanar_retries
    }

    /// Number of live solids (for leak tests and diagnostics).
    pub fn live_solids(&self) -> usize {
        self.solids.len()
    }

    /// [`Kernel::extrude`] with a caller-supplied [`Budget`].
    ///
    /// Checkpoints: once before the profile reaches truck, once after. A profile that takes long
    /// enough to matter is a profile with many points, and the point loop is truck's, not ours.
    pub fn extrude_with_budget(
        &mut self,
        profile: &[[f64; 2]],
        height_mm: f64,
        budget: &Budget,
    ) -> Result<SolidId> {
        self.require(Capability::Extrude)?;
        budget.check()?;
        self.extrude_calls.fetch_add(1, Ordering::Relaxed);
        validate_profile(profile)?;
        if !height_mm.is_finite() || height_mm == 0.0 {
            return Err(KernelError::Invalid(
                "extrude height must be finite and non-zero".into(),
            ));
        }

        let solid = guarded("extrude", || {
            let vertices: Vec<_> = profile
                .iter()
                .map(|p| builder::vertex(Point3::new(p[0], p[1], 0.0)))
                .collect();
            let edges = (0..vertices.len())
                .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % vertices.len()]))
                .collect::<Vec<_>>();
            let wire: Wire = edges.into_iter().collect();
            let face = builder::try_attach_plane(&[wire]).map_err(|e| {
                KernelError::Invalid(format!("profile is not a planar region: {e}"))
            })?;
            Ok(builder::tsweep(&face, Vector3::new(0.0, 0.0, height_mm)))
        })?;
        budget.check()?;

        Ok(self.insert(solid))
    }

    /// [`Kernel::translate`] with a caller-supplied [`Budget`].
    ///
    /// A translation is a deep copy with every vertex moved, so it is cheap and exact: it cannot
    /// change a volume, and it is the only way to place a tool so that it *overshoots* the body it
    /// cuts (see [`Kernel::translate`]).
    pub fn translate_with_budget(
        &mut self,
        solid: SolidId,
        delta_mm: [f64; 3],
        budget: &Budget,
    ) -> Result<SolidId> {
        self.require(Capability::Transform)?;
        budget.check()?;
        if !delta_mm.iter().all(|d| d.is_finite()) {
            return Err(KernelError::Invalid(
                "translation must be finite in every axis".into(),
            ));
        }
        let moved = guarded("translate", || {
            let shift = Vector3::new(delta_mm[0], delta_mm[1], delta_mm[2]);
            Ok(builder::translated(self.lookup(solid)?, shift))
        })?;
        budget.check()?;
        Ok(self.insert(moved))
    }

    /// [`Kernel::revolve`] with a caller-supplied [`Budget`].
    ///
    /// The profile is built into a planar face exactly as [`Kernel::extrude`] builds it, then swept
    /// a full turn about an in-plane axis. Two inputs are validated here rather than left to truck:
    ///
    /// * the angle must be a **full turn**. A partial sweep of a face is an open shell, and truck's
    ///   return type cannot tell the two apart, so accepting 90° would hand the caller something
    ///   that is not a solid while claiming it is;
    /// * the profile must lie on **one side** of the axis. A profile that crosses it self-intersects
    ///   when revolved, which is not a solid of revolution and has no meaningful volume.
    pub fn revolve_with_budget(
        &mut self,
        profile: &[[f64; 2]],
        axis_origin_mm: [f64; 2],
        axis_direction_mm: [f64; 2],
        angle_rad: f64,
        budget: &Budget,
    ) -> Result<SolidId> {
        self.require(Capability::Revolve)?;
        budget.check()?;
        validate_profile(profile)?;

        let [ox, oy] = axis_origin_mm;
        let [dx, dy] = axis_direction_mm;
        if !ox.is_finite() || !oy.is_finite() || !dx.is_finite() || !dy.is_finite() {
            return Err(KernelError::Invalid(
                "the revolve axis must be finite in every component".into(),
            ));
        }
        let length = dx.mul_add(dx, dy * dy).sqrt();
        if length < 1e-12 {
            return Err(KernelError::Invalid(
                "the revolve axis direction must be non-zero".into(),
            ));
        }
        if !angle_rad.is_finite() || (angle_rad - std::f64::consts::TAU).abs() > 1e-9 {
            return Err(KernelError::Invalid(format!(
                "revolve supports a full {:.6} rad turn only, got {angle_rad} rad; a partial \
                 sweep produces an open shell, not a solid",
                std::f64::consts::TAU
            )));
        }
        // Cross product with the axis direction: its sign says which side of the axis a point is
        // on. Every point must agree, or the swept profile would intersect itself.
        let mut side = 0i8;
        for point in profile {
            let cross = dx * (point[1] - oy) - dy * (point[0] - ox);
            if cross.abs() < 1e-9 {
                continue;
            }
            let sign = if cross > 0.0 { 1 } else { -1 };
            if side == 0 {
                side = sign;
            } else if side != sign {
                return Err(KernelError::Invalid(
                    "the profile crosses the revolve axis; a solid of revolution must lie on one \
                     side of it"
                        .into(),
                ));
            }
        }
        if side == 0 {
            return Err(KernelError::Invalid(
                "the profile lies on the revolve axis and encloses no volume when swept".into(),
            ));
        }

        let solid = guarded("revolve", || {
            let vertices: Vec<_> = profile
                .iter()
                .map(|p| builder::vertex(Point3::new(p[0], p[1], 0.0)))
                .collect();
            let edges = (0..vertices.len())
                .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % vertices.len()]))
                .collect::<Vec<_>>();
            let wire: Wire = edges.into_iter().collect();
            let face = builder::try_attach_plane(&[wire]).map_err(|e| {
                KernelError::Invalid(format!("profile is not a planar region: {e}"))
            })?;
            Ok(builder::rsweep(
                &face,
                Point3::new(ox, oy, 0.0),
                Vector3::new(dx / length, dy / length, 0.0),
                Rad(angle_rad),
            ))
        })?;
        budget.check()?;
        Ok(self.insert(solid))
    }

    /// [`Kernel::boolean`] with a caller-supplied [`Budget`].
    ///
    /// Checkpoints: before either operand is copied, between the first attempt and the coplanar
    /// retry, and around each conservation measurement — the measurements tessellate both operands
    /// and the result, which is the longest stretch this crate owns.
    pub fn boolean_with_budget(
        &mut self,
        op: BooleanOp,
        a: SolidId,
        b: SolidId,
        budget: &Budget,
    ) -> Result<SolidId> {
        self.require(op.capability())?;
        budget.check()?;
        self.boolean_calls.fetch_add(1, Ordering::Relaxed);
        let tolerance = self.boolean_tol;

        // Deep copies: truck's operations consume relationships between the operands' topology
        // objects, and the caller's handles must stay valid (the contract says neither input is
        // consumed).
        // A *deep* copy (`builder::clone`) of the operand's topology graph, not a shallow `clone` of
        // its `Arc`s. truck's boolean splits and rewires the topology it is given; feeding it a
        // shallow copy corrupts the caller's solid and makes the *next* operation in a chain fail.
        let left = guarded("boolean operand", || Ok(builder::clone(self.lookup(a)?)))?;
        let right = guarded("boolean operand", || Ok(builder::clone(self.lookup(b)?)))?;

        // One boolean attempt with the tool moved by `offset`. A difference is `a AND NOT b`.
        let attempt = |offset: [f64; 3]| -> Option<Solid> {
            let shift = Vector3::new(offset[0], offset[1], offset[2]);
            let tool = if offset == [0.0; 3] {
                builder::clone(&right)
            } else {
                builder::translated(&right, shift)
            };
            let combined = match op {
                BooleanOp::Union => truck_shapeops::or(&left, &tool, tolerance),
                BooleanOp::Intersection => truck_shapeops::and(&left, &tool, tolerance),
                BooleanOp::Difference => {
                    let mut tool = tool;
                    tool.not();
                    truck_shapeops::and(&left, &tool, tolerance)
                }
            };
            // Move the result back so it is reported in the caller's coordinates.
            combined.map(|solid| {
                if offset == [0.0; 3] {
                    solid
                } else {
                    builder::translated(&solid, -shift)
                }
            })
        };

        let (result, retried) = guarded(op.label(), || match attempt([0.0; 3]) {
            Some(solid) => Ok((solid, false)),
            None => {
                // Measured limitation of truck's boolean: operands with **coplanar, overlapping
                // faces** (for example a hole exactly as tall as the plate it passes through) return
                // no result, while piercing, contained and partially overlapping pairs succeed.
                // Moving only the tool by a fraction of a micrometre removes the coincidence; the
                // result is moved back, so the geometric change is bounded by COPLANAR_NUDGE_MM
                // (~1e-4 mm per axis) times the tool's surface area.
                budget.check()?;
                match attempt(COPLANAR_NUDGE_MM) {
                    Some(solid) => Ok((solid, true)),
                    None => Err(KernelError::Failed(format!(
                        "{} produced no result; the operands may be tangent, coincident, or nearly degenerate",
                        op.label()
                    ))),
                }
            }
        })?;

        if retried {
            self.coplanar_retries = self.coplanar_retries.saturating_add(1);
        }

        // Refuse a result that violates conservation of volume, rather than return a wrong solid.
        // A union can only grow and an intersection can only shrink; violating that means the
        // kernel produced malformed topology, which is exactly what a silently accepted boolean
        // would hide. (SolidCraft's `boolean_guard` enforces the same rule for all three ops.)
        let measure_tol = 0.05;
        let before_measure = solid_measure(&left, measure_tol, budget)?;
        let tool_measure = solid_measure(&right, measure_tol, budget)?;
        let after_measure = solid_measure(&result, measure_tol, budget)?;
        let before = before_measure.map_or(f64::NAN, |measure| measure.0);
        let tool = tool_measure.map_or(f64::NAN, |measure| measure.0);
        let after = after_measure.map_or(f64::NAN, |measure| measure.0);
        let slack = 1e-3 * before.max(tool).max(1.0);
        let violated = match op {
            BooleanOp::Union => after < before.max(tool) - slack,
            BooleanOp::Intersection => after > before.min(tool) + slack,
            BooleanOp::Difference => after > before + slack,
        };
        if violated {
            return Err(KernelError::Failed(format!(
                "{} produced a result that fails volume conservation \
                 ({} mm³ and {} mm³ in, {} mm³ out); refusing to return it",
                op.label(),
                before,
                tool,
                after
            )));
        }

        // An empty result is an error, not a body. Subtracting a solid from itself removes
        // everything, and there is no mesh that represents nothing — so the coplanar retry reports
        // a shell as thick as its own nudge instead. Measured: cutting a 10 x 10 x 5 plate with its
        // own profile returns 0.008 mm³, which would otherwise be handed to the application as a
        // successfully built feature. A remainder no larger than the nudge could have manufactured
        // by itself is nothing, and saying so is the only answer a caller can act on.
        if op == BooleanOp::Difference
            && after.is_finite()
            && let Some((_, tool_area)) = tool_measure
        {
            let nudge = COPLANAR_NUDGE_MM
                .iter()
                .fold(0.0f64, |largest, offset| largest.max(offset.abs()));
            // Twice the nudge times the tool's surface: the size of the shell the retry can leave.
            if after <= 2.0 * nudge * tool_area {
                return Err(KernelError::Failed(format!(
                    "{} removed the whole solid: {after} mm³ of {before} mm³ is left, which is no \
                     larger than the {nudge:.1e} mm the coplanar retry has to move the tool; refusing \
                     to return a sliver as a body",
                    op.label()
                )));
            }
        }

        Ok(self.insert(result))
    }

    /// [`Kernel::tessellate`] with a caller-supplied [`Budget`].
    ///
    /// This is where cancellation earns its keep: tessellation walks every face, and the weld and
    /// orientation passes afterwards are quadratic-ish over triangle count. Both loops checkpoint
    /// internally, so a large mesh stops within a bounded slice of work rather than at the end.
    pub fn tessellate_with_budget(
        &self,
        solid: SolidId,
        chord_tol_mm: f64,
        budget: &Budget,
    ) -> Result<Mesh> {
        self.require(Capability::Tessellate)?;
        budget.check()?;
        self.tessellate_calls.fetch_add(1, Ordering::Relaxed);
        let tol = Self::tolerance(chord_tol_mm)?;
        let solid = self.lookup(solid)?;
        guarded("tessellate", || mesh_of(solid, tol, budget))
    }

    /// [`Kernel::volume`] with a caller-supplied [`Budget`].
    pub fn volume_with_budget(&self, solid: SolidId, budget: &Budget) -> Result<f64> {
        let mesh = self.tessellate_with_budget(solid, DEFAULT_LINEAR_MM, budget)?;
        Ok(mesh.signed_volume().abs())
    }

    /// [`Kernel::bounding_box`] with a caller-supplied [`Budget`].
    pub fn bounding_box_with_budget(
        &self,
        solid: SolidId,
        budget: &Budget,
    ) -> Result<cad_contract::mesh::Aabb3> {
        let mesh = self.tessellate_with_budget(solid, DEFAULT_LINEAR_MM, budget)?;
        mesh.bounding_box()
            .ok_or_else(|| KernelError::Failed("solid produced no geometry".into()))
    }

    fn lookup(&self, id: SolidId) -> Result<&Solid> {
        self.solids
            .get(&id.get())
            .ok_or(KernelError::UnknownSolid(id))
    }

    fn insert(&mut self, solid: Solid) -> SolidId {
        let id = SolidId::from_raw(self.next);
        self.next = self.next.saturating_add(1);
        self.solids.insert(id.get(), solid);
        id
    }

    /// Validate a tessellation tolerance and return it clamped to the kernel's usable range.
    fn tolerance(tol: f64) -> Result<f64> {
        if !tol.is_finite() || tol <= 0.0 {
            return Err(KernelError::Invalid(format!(
                "tolerance {tol} must be finite and positive"
            )));
        }
        if tol < MIN_TOLERANCE_MM {
            return Err(KernelError::Invalid(format!(
                "tolerance {tol} mm is below the kernel minimum of {MIN_TOLERANCE_MM} mm"
            )));
        }
        Ok(tol)
    }
}

/// Run `f`, converting a panic into [`KernelError::Internal`].
///
/// `what` names the operation so the error is actionable. The document is unchanged either way:
/// the operation's result is only inserted into the kernel after it returns `Ok`.
fn guarded<T>(what: &str, f: impl FnOnce() -> Result<T>) -> Result<T> {
    match catch_unwind(AssertUnwindSafe(f)) {
        Ok(result) => result,
        Err(payload) => Err(KernelError::Internal(format!(
            "{what}: {}",
            panic_message(&payload)
        ))),
    }
}

/// Best-effort text from a panic payload.
fn panic_message(payload: &(dyn std::any::Any + Send)) -> String {
    if let Some(s) = payload.downcast_ref::<&str>() {
        (*s).to_string()
    } else if let Some(s) = payload.downcast_ref::<String>() {
        s.clone()
    } else {
        "non-string panic payload".to_string()
    }
}

impl Kernel for TruckKernel {
    fn info(&self) -> KernelInfo {
        // Only capabilities with a tested operation behind them. STEP/IGES, fillets, sweeps and
        // WASM are deliberately absent: this adapter does not implement them.
        KernelInfo::new(
            "truck",
            "0.6",
            CapabilitySet::from([
                Capability::Extrude,
                // Revolve is advertised because it is measured: a rectangle sharing the axis gives
                // the analytic cylinder and one offset from it the analytic annulus (see the tests
                // below). Only a full turn is supported, and the adapter says so on any other angle.
                Capability::Revolve,
                Capability::BooleanUnion,
                // Difference is advertised because it is *measured* correct: the raw result's
                // volume is wrong only because truck winds the walls of a cavity inward, and this
                // adapter's weld + orientation repair corrects exactly that (a 100 x 100 x 10 plate
                // with a 40 x 40 through-cut measures 84 000.18 mm³ against an analytic 84 000).
                // Two limitations are real and are stated rather than hidden: operands with
                // coincident faces still need the coplanar retry, whose nudge leaves a
                // COPLANAR_NUDGE_MM membrane on the shared face, and a tessellation of that
                // membrane can carry hairline sliver edges. Build a through-cut with a tool that
                // *overshoots* the body (see `Kernel::translate`), which avoids borrowed faces
                // entirely and is exact.
                Capability::BooleanDifference,
                Capability::BooleanIntersection,
                Capability::Transform,
                Capability::Tessellate,
            ]),
        )
    }

    fn extrude(&mut self, profile: &[[f64; 2]], height_mm: f64) -> Result<SolidId> {
        // Delegation with an unlimited budget: the trait path is unchanged behaviour.
        self.extrude_with_budget(profile, height_mm, &Budget::unlimited())
    }

    fn revolve(
        &mut self,
        profile: &[[f64; 2]],
        axis_origin_mm: [f64; 2],
        axis_direction_mm: [f64; 2],
        angle_rad: f64,
    ) -> Result<SolidId> {
        self.revolve_with_budget(
            profile,
            axis_origin_mm,
            axis_direction_mm,
            angle_rad,
            &Budget::unlimited(),
        )
    }

    fn boolean(&mut self, op: BooleanOp, a: SolidId, b: SolidId) -> Result<SolidId> {
        self.boolean_with_budget(op, a, b, &Budget::unlimited())
    }

    fn translate(&mut self, solid: SolidId, delta_mm: [f64; 3]) -> Result<SolidId> {
        self.translate_with_budget(solid, delta_mm, &Budget::unlimited())
    }

    fn tessellate(&self, solid: SolidId, chord_tol_mm: f64) -> Result<Mesh> {
        self.tessellate_with_budget(solid, chord_tol_mm, &Budget::unlimited())
    }

    fn release(&mut self, solid: SolidId) {
        // Deliberately not budgeted: releasing is a map removal, and cleanup is the one operation
        // that must still work when a deadline has expired or the user has cancelled everything.
        self.solids.remove(&solid.get());
    }

    fn contains(&self, solid: SolidId) -> Option<bool> {
        Some(self.solids.contains_key(&solid.get()))
    }

    fn volume(&self, solid: SolidId) -> Result<f64> {
        self.volume_with_budget(solid, &Budget::unlimited())
    }

    fn bounding_box(&self, solid: SolidId) -> Result<cad_contract::mesh::Aabb3> {
        self.bounding_box_with_budget(solid, &Budget::unlimited())
    }
}

/// Triangulate every face patch of `solid` into one mesh.
///
/// truck tessellates each face in the face's own parameter space and emits per-face vertices. Two
/// properties therefore have to be established here before the mesh can be trusted as a watertight
/// solid boundary:
///
/// 1. **Welding.** Per-face vertices mean that two faces meeting at an edge share no vertex index,
///    so the mesh looks like a set of open patches even though the geometry is closed.
/// 2. **Consistent outward winding.** Measured on a 10x10x5 box: the cap at z = 0 came back wound
///    inward while every other face was outward — 8 open edges after welding. The signed volume
///    still read exactly 500 mm³ because a face lying in the z = 0 plane contributes nothing to the
///    tetrahedral sum, which is why volume alone is not a winding check.
///
/// A mesh handed to a renderer, an exporter or another kernel must be a closed, consistently
/// oriented manifold, so the repair happens here rather than in every consumer.
fn mesh_of(solid: &Solid, tol: f64, budget: &Budget) -> Result<Mesh> {
    let mut raw = Mesh::default();
    for shell in solid.boundaries() {
        // A checkpoint per shell: a multi-shell result (a boolean output, say) can stop between
        // shells even though the shell's own triangulation is truck's.
        budget.check()?;
        let meshed = shell.robust_triangulation(tol);
        for face in meshed.face_iter() {
            // Per face: this is the natural unit of work in tessellation.
            budget.check()?;
            let Some(polygon) = face.surface() else {
                continue;
            };
            let base = u32::try_from(raw.positions.len())
                .map_err(|_| KernelError::TooLarge("mesh exceeds u32 vertex indices".into()))?;
            for p in polygon.positions() {
                if !p.x.is_finite() || !p.y.is_finite() || !p.z.is_finite() {
                    return Err(KernelError::Failed(
                        "kernel produced a non-finite vertex".into(),
                    ));
                }
                raw.positions.push([p.x, p.y, p.z]);
            }
            for tri in polygon.tri_faces() {
                // `tri_faces()` yields `StandardVertex`, whose `pos` is the position index.
                let index = |v: &StandardVertex| -> Result<u32> {
                    u32::try_from(v.pos)
                        .ok()
                        .and_then(|i| i.checked_add(base))
                        .ok_or_else(|| KernelError::TooLarge("mesh index overflow".into()))
                };
                raw.triangles
                    .push([index(&tri[0])?, index(&tri[1])?, index(&tri[2])?]);
            }
        }
    }

    let welded = weld_by_position(&raw, WELD_QUANTUM_MM, budget)?;
    let oriented = orient_outward(welded, budget)?;
    oriented
        .validate()
        .map_err(|e| KernelError::Failed(format!("kernel produced an invalid mesh: {e}")))?;
    Ok(oriented)
}

/// Vertices between cancellation checkpoints while welding. 4 096 is ~0.2 ms of work on this
/// machine — fine enough that a cancelled export stops promptly, coarse enough that the check is
/// not the bottleneck.
const WELD_CHECKPOINT_STRIDE: usize = 4096;

/// Vertices closer than this are the same point. One nanometre is four orders of magnitude below
/// truck's own 1e-4 mm tolerance, so distinct geometry cannot be merged accidentally.
const WELD_QUANTUM_MM: f64 = 1e-6;

/// Merge coincident vertices so edges can be matched across faces.
///
/// The lookup is keyed by quantised position, so the loop below is the longest linear pass in the
/// tessellation path; it checkpoints every [`WELD_CHECKPOINT_STRIDE`] vertices.
fn weld_by_position(mesh: &Mesh, quantum: f64, budget: &Budget) -> Result<Mesh> {
    let mut lookup: BTreeMap<(i64, i64, i64), u32> = BTreeMap::new();
    let mut positions: Vec<[f64; 3]> = Vec::with_capacity(mesh.positions.len());
    let mut remap: Vec<u32> = Vec::with_capacity(mesh.positions.len());

    for (index, point) in mesh.positions.iter().enumerate() {
        if index % WELD_CHECKPOINT_STRIDE == 0 {
            budget.check()?;
        }
        let key = (
            (point[0] / quantum).round() as i64,
            (point[1] / quantum).round() as i64,
            (point[2] / quantum).round() as i64,
        );
        let next = u32::try_from(positions.len())
            .map_err(|_| KernelError::TooLarge("mesh exceeds u32 vertex indices".into()))?;
        let index = *lookup.entry(key).or_insert(next);
        if index == next {
            positions.push(*point);
        }
        remap.push(index);
    }

    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(mesh.triangles.len());
    for tri in &mesh.triangles {
        let mapped = [
            remap[tri[0] as usize],
            remap[tri[1] as usize],
            remap[tri[2] as usize],
        ];
        // A triangle whose vertices welded together has no area; dropping it cannot open the
        // surface further, because it carried no surface.
        if mapped[0] != mapped[1] && mapped[1] != mapped[2] && mapped[2] != mapped[0] {
            triangles.push(mapped);
        }
    }

    Ok(Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    })
}

/// Make the winding consistent across shared edges, then give every shell the sign its nesting
/// requires: outer shells outward, cavities inward.
///
/// The second half is not a refinement, it is the fix for the defect that made every difference
/// measurement wrong. A cut whose tool is fully enclosed leaves the tool's own surface as an inner
/// shell; truck returns that shell wound **outward**, so the enclosed volume was `outer + cavity`
/// (216 000 mm³ for a 200 000 mm³ plate with a 16 000 mm³ void) instead of `outer - cavity`. Flipping
/// the *whole* mesh cannot repair that, because the outer shell is already right. What is needed is
/// the number of shells that contain each shell, and that is measured here rather than assumed: a
/// point taken from every shell is tested against every other shell, so a cavity is handled by the
/// same code as an ordinary body and a shell nested three deep comes out right too.
fn orient_outward(mut mesh: Mesh, budget: &Budget) -> Result<Mesh> {
    budget.check()?;
    let mut adjacency: BTreeMap<(u32, u32), Vec<usize>> = BTreeMap::new();
    for (index, tri) in mesh.triangles.iter().enumerate() {
        if index % WELD_CHECKPOINT_STRIDE == 0 {
            budget.check()?;
        }
        for (a, b) in directed_edges(tri) {
            adjacency.entry(undirected(a, b)).or_default().push(index);
        }
    }

    // Pass 1: consistent winding within each connected component, and which component that is.
    let mut visited = vec![false; mesh.triangles.len()];
    let mut component = vec![0usize; mesh.triangles.len()];
    let mut components = 0usize;
    let mut queue: VecDeque<usize> = VecDeque::new();
    for seed in 0..mesh.triangles.len() {
        if visited[seed] {
            continue;
        }
        visited[seed] = true;
        component[seed] = components;
        queue.push_back(seed);
        while let Some(current) = queue.pop_front() {
            for (a, b) in directed_edges(&mesh.triangles[current]) {
                let Some(neighbours) = adjacency.get(&undirected(a, b)) else {
                    continue;
                };
                for &other in neighbours {
                    if other == current || visited[other] {
                        continue;
                    }
                    // Two consistently oriented neighbours traverse their shared edge in opposite
                    // directions; the same direction means this one is flipped.
                    if has_directed_edge(&mesh.triangles[other], a, b) {
                        mesh.triangles[other].swap(1, 2);
                    }
                    visited[other] = true;
                    component[other] = components;
                    queue.push_back(other);
                }
            }
        }
        components = components.saturating_add(1);
    }

    // Pass 2: nesting depth per component, then the sign that depth requires.
    let mut signed = vec![0.0f64; components];
    for (index, tri) in mesh.triangles.iter().enumerate() {
        signed[component[index]] += triangle_six_volume(&mesh, tri);
    }
    let mut points: Vec<Option<[f64; 3]>> = vec![None; components];
    for (index, tri) in mesh.triangles.iter().enumerate() {
        if points[component[index]].is_some() {
            continue;
        }
        if let Some(position) = mesh.positions.get(tri[0] as usize) {
            points[component[index]] = Some(position);
        }
    }

    // Group the triangles once so each component can be tested as a closed shell.
    let mut shells: Vec<Vec<[u32; 3]>> = vec![Vec::new(); components];
    for (index, tri) in mesh.triangles.iter().enumerate() {
        shells[component[index]].push(*tri);
    }

    let mut flip = vec![false; components];
    for index in 0..components {
        budget.check()?;
        // Signed volume divided by six is positive for an outward shell, so the sign of `signed`
        // already answers "is this wound outward".
        let outward = signed[index] > 0.0;
        let mut depth = 0usize;
        if components > 1 {
            let Some(point) = points[index] else {
                continue;
            };
            for (other, shell) in shells.iter().enumerate() {
                if other == index || shell.is_empty() {
                    continue;
                }
                if point_inside(&mesh.positions, shell, point) {
                    depth = depth.saturating_add(1);
                }
            }
        }
        // Even depth: a body, which must be wound outward. Odd depth: a cavity, which must not.
        flip[index] = outward != depth.is_multiple_of(2);
    }
    for (index, tri) in mesh.triangles.iter_mut().enumerate() {
        if flip[component[index]] {
            tri.swap(1, 2);
        }
    }

    // A closed surface's signed volume is positive exactly when the solid it bounds is outward and
    // its cavities are not. Keep the whole-mesh correction as a fallback, so a shell whose nesting
    // could not be measured still comes back with a positive total rather than an inside-out solid.
    if mesh.signed_volume() < 0.0 {
        for tri in &mut mesh.triangles {
            tri.swap(1, 2);
        }
    }
    Ok(mesh)
}

/// Six times the signed volume contributed by one triangle, in the divergence-theorem sum.
fn triangle_six_volume(mesh: &Mesh, tri: &[u32; 3]) -> f64 {
    let (Some(a), Some(b), Some(c)) = (
        mesh.positions.get(tri[0] as usize),
        mesh.positions.get(tri[1] as usize),
        mesh.positions.get(tri[2] as usize),
    ) else {
        return 0.0;
    };
    a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
        + a[2] * (b[0] * c[1] - b[1] * c[0])
}

/// Is `point` inside the closed shell made of `triangles`?
///
/// Ray-parity: a ray cast from the point crosses the surface an odd number of times exactly when the
/// point is enclosed. The direction deliberately has no axis alignment and no small integer ratio, so
/// a ray from a shell's vertex cannot run along a face or through an edge of an axis-aligned shell —
/// the case that makes naive parity wrong on exactly the geometry CAD produces.
fn point_inside(positions: &[[f64; 3]], triangles: &[[u32; 3]], point: [f64; 3]) -> bool {
    const DIRECTION: [f64; 3] = [0.5731, 0.2713, 0.7739];
    let mut crossings = 0usize;
    for tri in triangles {
        let (Some(a), Some(b), Some(c)) = (
            positions.get(tri[0] as usize),
            positions.get(tri[1] as usize),
            positions.get(tri[2] as usize),
        ) else {
            continue;
        };
        if ray_crosses_triangle(*a, *b, *c, point, DIRECTION) {
            crossings = crossings.saturating_add(1);
        }
    }
    crossings % 2 == 1
}

/// Does the ray from `origin` along `direction` cross the triangle, strictly ahead of the origin?
///
/// Möller–Trumbore. A hit at the origin itself (`distance <= 0`) is excluded: the probe point sits on
/// its own shell, and counting that shell's own faces would invert every answer.
fn ray_crosses_triangle(
    a: [f64; 3],
    b: [f64; 3],
    c: [f64; 3],
    origin: [f64; 3],
    direction: [f64; 3],
) -> bool {
    let edge1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let edge2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let pvec = [
        direction[1] * edge2[2] - direction[2] * edge2[1],
        direction[2] * edge2[0] - direction[0] * edge2[2],
        direction[0] * edge2[1] - direction[1] * edge2[0],
    ];
    let det = edge1[0] * pvec[0] + edge1[1] * pvec[1] + edge1[2] * pvec[2];
    if det.abs() < 1e-12 {
        return false;
    }
    let inv = 1.0 / det;
    let tvec = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
    let u = (tvec[0] * pvec[0] + tvec[1] * pvec[1] + tvec[2] * pvec[2]) * inv;
    if !(0.0..=1.0).contains(&u) {
        return false;
    }
    let qvec = [
        tvec[1] * edge1[2] - tvec[2] * edge1[1],
        tvec[2] * edge1[0] - tvec[0] * edge1[2],
        tvec[0] * edge1[1] - tvec[1] * edge1[0],
    ];
    let v = (direction[0] * qvec[0] + direction[1] * qvec[1] + direction[2] * qvec[2]) * inv;
    if v < 0.0 || u + v > 1.0 {
        return false;
    }
    let distance = (edge2[0] * qvec[0] + edge2[1] * qvec[1] + edge2[2] * qvec[2]) * inv;
    distance > 1e-9
}

fn directed_edges(tri: &[u32; 3]) -> [(u32, u32); 3] {
    [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])]
}

fn undirected(a: u32, b: u32) -> (u32, u32) {
    if a <= b { (a, b) } else { (b, a) }
}

fn has_directed_edge(tri: &[u32; 3], a: u32, b: u32) -> bool {
    directed_edges(tri).contains(&(a, b))
}

/// Enclosed volume and surface area of a solid, measured from its tessellation.
///
/// `Ok(None)` means the solid could not be measured but the operation may continue: the conservation
/// guard tolerates an unmeasurable operand by skipping its comparison. The two outcomes this will not
/// swallow are cancellation and timeout, because those are the caller's decision and must stop the
/// operation, not be treated as "unmeasurable".
fn solid_measure(solid: &Solid, tol: f64, budget: &Budget) -> Result<Option<(f64, f64)>> {
    match mesh_of(solid, tol, budget) {
        Ok(mesh) => Ok(Some((mesh.signed_volume().abs(), mesh.surface_area()))),
        Err(e @ (KernelError::Cancelled | KernelError::Timeout(_))) => Err(e),
        Err(_) => Ok(None),
    }
}

#[cfg(test)]
mod tests {
    use super::{Budget, CancellationToken, MIN_TOLERANCE_MM, TruckKernel};
    use cad_contract::capability::Capability;
    use cad_contract::error::KernelError;
    use cad_contract::kernel::{BooleanOp, Kernel, SolidId};
    use cad_contract::mesh::Mesh;
    use std::time::Duration;

    /// Tests return `Result` so that a failed kernel call **fails the test**.
    ///
    /// An earlier version of this module used `let Ok(x) = call() else { return }`, which turned
    /// every kernel failure into a pass; that is how a broken boolean went unnoticed. Returning
    /// `Result` makes the failure visible without `unwrap`/`expect` (both denied by the workspace
    /// lints).
    type TestResult = Result<(), KernelError>;

    fn square(size: f64) -> Vec<[f64; 2]> {
        vec![[0.0, 0.0], [size, 0.0], [size, size], [0.0, size]]
    }

    fn square_at(size: f64, x: f64, y: f64) -> Vec<[f64; 2]> {
        vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
    }

    fn volume(kernel: &TruckKernel, id: cad_contract::kernel::SolidId) -> Result<f64, KernelError> {
        kernel.volume(id)
    }

    /// Assert that `actual` is within `tolerance` of `expected`, as a test failure.
    fn close(actual: f64, expected: f64, tolerance: f64, what: &str) -> TestResult {
        if (actual - expected).abs() <= tolerance {
            Ok(())
        } else {
            Err(KernelError::Failed(format!(
                "{what}: expected {expected}, got {actual}"
            )))
        }
    }

    #[test]
    fn advertises_exactly_what_it_implements() -> TestResult {
        let kernel = TruckKernel::new();
        let info = kernel.info();
        assert_eq!(info.name, "truck");
        assert!(info.supports(Capability::Extrude));
        assert!(info.supports(Capability::Tessellate));
        assert!(info.supports(Capability::BooleanUnion));
        assert!(info.supports(Capability::BooleanIntersection));
        // Difference is claimed because the repaired result is measured correct, not because truck's
        // raw output is: see `difference_is_the_analytic_cut` below.
        assert!(info.supports(Capability::BooleanDifference));
        assert!(info.supports(Capability::Transform));
        // Revolve is claimed because it is measured: see the cylinder and annulus tests below.
        assert!(info.supports(Capability::Revolve));
        // Not implemented, so not advertised.
        assert!(!info.supports(Capability::Fillet));
        assert!(!info.supports(Capability::Sweep));
        assert!(!info.supports(Capability::StepExport));
        assert!(!info.supports(Capability::Wasm));
        Ok(())
    }

    /// The counters exist so a cache can be *shown* to skip work instead of claiming it.
    ///
    /// They are diagnostics, not model state: incrementing them must not change an answer, and the
    /// assertions below are about the count alone.
    #[test]
    fn operations_are_counted() -> TestResult {
        let mut kernel = TruckKernel::new();
        assert_eq!(kernel.extrude_calls(), 0);
        assert_eq!(kernel.boolean_calls(), 0);
        assert_eq!(kernel.tessellate_calls(), 0);

        let plate = kernel.extrude(&square(10.0), 5.0)?;
        assert_eq!(kernel.extrude_calls(), 1);
        // A taller tool overlapping the plate in plan, so the union has no fully coincident solid.
        let tool = kernel.extrude(&square_at(4.0, 3.0, 3.0), 8.0)?;
        assert_eq!(kernel.extrude_calls(), 2);

        let joined = kernel.boolean(BooleanOp::Union, plate, tool)?;
        assert_eq!(kernel.boolean_calls(), 1);
        kernel.tessellate(joined, 0.05)?;
        assert_eq!(kernel.tessellate_calls(), 1);
        Ok(())
    }

    /// A session cache asks this before adopting a handle another kernel instance issued.
    #[test]
    fn contains_answers_for_issued_and_released_handles() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        assert_eq!(kernel.contains(id), Some(true));
        assert_eq!(kernel.contains(SolidId::from_raw(999)), Some(false));
        kernel.release(id);
        assert_eq!(kernel.contains(id), Some(false));
        Ok(())
    }

    /// A rectangle that shares the revolve axis becomes the analytic cylinder.
    #[test]
    fn revolve_on_the_axis_is_a_cylinder() -> TestResult {
        let mut kernel = TruckKernel::new();
        // x 0..4, y 0..3, revolved about the y axis through the origin: radius 4, height 3.
        let profile = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]];
        let id = kernel.revolve(&profile, [0.0, 0.0], [0.0, 1.0], std::f64::consts::TAU)?;
        // A tessellated cylinder is an inscribed prism, so the measurement converges to the
        // analytic value as the chord tolerance falls: at the adapter's finest accepted tolerance
        // the polygon-area error is ~2e-5 relative, well inside the 0.05 mm³ bound below.
        let mesh = kernel.tessellate(id, MIN_TOLERANCE_MM)?;
        let volume = mesh.signed_volume().abs();
        let analytic = std::f64::consts::PI * 16.0 * 3.0;
        close(volume, analytic, 0.05, "cylinder volume")?;

        let bb = kernel.bounding_box(id)?;
        close(bb.min[1], 0.0, 1e-6, "cylinder bottom")?;
        close(bb.max[1], 3.0, 1e-6, "cylinder top")?;
        close(bb.min[0], -4.0, 1e-6, "cylinder min x")?;
        close(bb.max[0], 4.0, 1e-6, "cylinder max x")?;
        // A full turn is watertight.
        assert_eq!(mesh.open_edges(), 0, "a cylinder must be closed");
        Ok(())
    }

    /// A rectangle offset from the axis becomes the analytic annulus, with a real hole.
    #[test]
    fn revolve_offset_from_the_axis_is_an_annulus() -> TestResult {
        let mut kernel = TruckKernel::new();
        // x 2..4, y 0..3 about the y axis: outer radius 4, inner 2, height 3.
        let profile = vec![[2.0, 0.0], [4.0, 0.0], [4.0, 3.0], [2.0, 3.0]];
        let id = kernel.revolve(&profile, [0.0, 0.0], [0.0, 1.0], std::f64::consts::TAU)?;
        let mesh = kernel.tessellate(id, MIN_TOLERANCE_MM)?;
        let volume = mesh.signed_volume().abs();
        let analytic = std::f64::consts::PI * (16.0 - 4.0) * 3.0;
        close(volume, analytic, 0.05, "annulus volume")?;
        assert_eq!(mesh.open_edges(), 0, "an annulus must be closed");
        Ok(())
    }

    /// A partial turn would be an open shell, so it is refused instead of returned as a solid.
    #[test]
    fn a_partial_revolve_is_refused() -> TestResult {
        let mut kernel = TruckKernel::new();
        let profile = vec![[0.0, 0.0], [4.0, 0.0], [4.0, 3.0], [0.0, 3.0]];
        match kernel.revolve(
            &profile,
            [0.0, 0.0],
            [0.0, 1.0],
            std::f64::consts::FRAC_PI_2,
        ) {
            Err(KernelError::Invalid(message)) => {
                assert!(message.contains("full"), "{message}");
                Ok(())
            }
            other => Err(KernelError::Failed(format!(
                "a partial revolve must be refused, got {other:?}"
            ))),
        }
    }

    /// A profile that crosses the axis has no solid of revolution.
    #[test]
    fn a_profile_crossing_the_axis_is_refused() -> TestResult {
        let mut kernel = TruckKernel::new();
        let profile = vec![[-2.0, 0.0], [4.0, 0.0], [4.0, 3.0], [-2.0, 3.0]];
        match kernel.revolve(&profile, [0.0, 0.0], [0.0, 1.0], std::f64::consts::TAU) {
            Err(KernelError::Invalid(message)) => {
                assert!(message.contains("crosses"), "{message}");
                Ok(())
            }
            other => Err(KernelError::Failed(format!(
                "a crossing profile must be refused, got {other:?}"
            ))),
        }
    }

    #[test]
    fn extruded_box_has_the_exact_analytic_volume() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        // 10 x 10 x 5 = 500 mm³.
        close(volume(&kernel, id)?, 500.0, 1e-6, "box volume")
    }

    #[test]
    fn extruded_box_has_the_expected_bounding_box() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        let bb = kernel.bounding_box(id)?;
        close(bb.min[0], 0.0, 1e-9, "min x")?;
        close(bb.min[2], 0.0, 1e-9, "min z")?;
        close(bb.max[0], 10.0, 1e-9, "max x")?;
        close(bb.max[2], 5.0, 1e-9, "max z")?;
        Ok(())
    }

    /// The box fixture, used by the orientation checks below.
    fn tessellated_box(size: f64, height: f64) -> Result<Mesh, KernelError> {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(size), height)?;
        kernel.tessellate(id, 0.01)
    }

    #[test]
    fn tessellation_is_a_closed_outward_manifold() -> TestResult {
        // Regression for two measured defects in the raw tessellation: per-face vertices (every
        // edge looked open) and one inward-wound cap whose error the signed volume could not
        // expose, because that cap lay in the z = 0 plane.
        let mesh = tessellated_box(10.0, 5.0)?;
        assert_eq!(
            mesh.open_edges(),
            0,
            "a welded solid boundary has no open edges"
        );
        assert!(mesh.is_closed());
        assert!(
            (mesh.signed_volume() - 500.0).abs() < 1e-6,
            "volume was {}",
            mesh.signed_volume()
        );
        assert_eq!(mesh.triangle_count(), 12);
        assert_eq!(mesh.vertex_count(), 8);

        // For a convex box every face normal must point away from the centre.
        let centre = [5.0, 5.0, 2.5];
        for tri in &mesh.triangles {
            let a = mesh.positions[tri[0] as usize];
            let b = mesh.positions[tri[1] as usize];
            let c = mesh.positions[tri[2] as usize];
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let normal = [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ];
            let mid = [
                (a[0] + b[0] + c[0]) / 3.0,
                (a[1] + b[1] + c[1]) / 3.0,
                (a[2] + b[2] + c[2]) / 3.0,
            ];
            let outward = normal[0] * (mid[0] - centre[0])
                + normal[1] * (mid[1] - centre[1])
                + normal[2] * (mid[2] - centre[2]);
            assert!(
                outward > 0.0,
                "a face is wound inward: dot product {outward}"
            );
        }
        Ok(())
    }

    #[test]
    fn tessellation_is_valid_and_closed() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        let mesh = kernel.tessellate(id, 0.05)?;
        mesh.validate()
            .map_err(|e| KernelError::Failed(e.to_string()))?;
        assert!(
            mesh.triangle_count() >= 12,
            "a box needs at least 12 triangles, got {}",
            mesh.triangle_count()
        );
        assert!(
            mesh.signed_volume() > 0.0,
            "a closed outward mesh has positive signed volume"
        );
        close(mesh.signed_volume(), 500.0, 1e-6, "tessellated box volume")
    }

    /// A through-cut built from an overshooting tool is the analytic difference.
    ///
    /// This is the product path: the tool is extruded longer than the body and translated so that
    /// it straddles it, which leaves no coincident faces for truck to mis-classify and therefore
    /// needs no nudge. Measured: 840 mm³ and 224 mm² exactly.
    #[test]
    fn difference_is_the_analytic_cut() -> TestResult {
        let mut kernel = TruckKernel::new();
        let plate = kernel.extrude(&square(10.0), 10.0)?;
        // 4 x 4 x 14, then moved down 2: it spans z -2..12 through a plate spanning z 0..10.
        let tool = kernel.extrude(&square_at(4.0, 3.0, 3.0), 14.0)?;
        let tool = kernel.translate(tool, [0.0, 0.0, -2.0])?;
        let cut = kernel.boolean(BooleanOp::Difference, plate, tool)?;
        // 1000 mm³ plate minus a 4 x 4 x 10 through-hole (160 mm³) = 840 mm³.
        close(volume(&kernel, cut)?, 840.0, 1e-2, "through-cut volume")?;

        // The plate's own faces must not move: a translation of the *tool* only.
        let bb = kernel.bounding_box(cut)?;
        close(bb.min[2], 0.0, 1e-9, "cut keeps the plate's bottom")?;
        close(bb.max[2], 10.0, 1e-9, "cut keeps the plate's top")?;

        // A through-hole leaves one shell and a watertight boundary.
        let mesh = kernel.tessellate(cut, 0.01)?;
        assert_eq!(mesh.open_edges(), 0, "a through-cut must be watertight");
        Ok(())
    }

    /// A cut whose tool shares a face with the body is still bounded and honest.
    ///
    /// Truck cannot do this without a nudge, and the nudge leaves a `COPLANAR_NUDGE_MM` membrane on
    /// the shared face, so the volume is 1.1e-4 mm x (hole area) below the analytic answer rather
    /// than exact. The assertion states that bound instead of pretending the result is exact; the
    /// application avoids the case entirely by overshooting the tool.
    #[test]
    fn a_coplanar_cut_is_within_the_nudge_bound() -> TestResult {
        let mut kernel = TruckKernel::new();
        let plate = kernel.extrude(&square(10.0), 10.0)?;
        let tool = kernel.extrude(&square_at(4.0, 3.0, 3.0), 10.0)?;
        let cut = kernel.boolean(BooleanOp::Difference, plate, tool)?;
        // Analytic 840; the membrane leaves 0.11 mm³ x 16 mm² = 0.0018 mm³ unremoved.
        let measured = volume(&kernel, cut)?;
        assert!(
            (measured - 840.0).abs() <= 0.05,
            "coplanar cut volume was {measured}, analytic 840"
        );
        // A cut may never add material.
        assert!(measured <= 840.0 + 0.05, "a cut cannot gain volume");
        Ok(())
    }

    /// A difference that removes everything is an error, not a sliver.
    ///
    /// There is no mesh for "nothing", so the coplanar retry reports a shell as thick as its own
    /// nudge — measured 0.008 mm³ for a plate cut by its own profile. Handing that to the document
    /// would mark the feature `built` and show a 0.008 mm³ body, which is a wrong answer presented
    /// as a successful one. The message says what happened instead.
    #[test]
    fn a_difference_that_removes_everything_is_refused() -> TestResult {
        let mut kernel = TruckKernel::new();
        let plate = kernel.extrude(&square(10.0), 5.0)?;
        let same = kernel.extrude(&square(10.0), 5.0)?;
        match kernel.boolean(BooleanOp::Difference, plate, same) {
            Err(KernelError::Failed(message)) => {
                assert!(
                    message.contains("removed the whole solid"),
                    "unexpected message: {message}"
                );
            }
            other => {
                return Err(KernelError::Failed(format!(
                    "an empty difference must be refused, got {other:?}"
                )));
            }
        }
        // The operands survive the refusal.
        close(
            volume(&kernel, plate)?,
            500.0,
            1e-6,
            "operand after a refusal",
        )
    }

    /// Translation moves a solid exactly and leaves its volume untouched.
    #[test]
    fn translate_is_exact_and_does_not_change_volume() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        let moved = kernel.translate(id, [7.0, -3.0, 2.5])?;
        let bb = kernel.bounding_box(moved)?;
        close(bb.min[0], 7.0, 1e-9, "translated min x")?;
        close(bb.min[1], -3.0, 1e-9, "translated min y")?;
        close(bb.min[2], 2.5, 1e-9, "translated min z")?;
        close(bb.max[2], 7.5, 1e-9, "translated max z")?;
        close(volume(&kernel, moved)?, 500.0, 1e-6, "translated volume")?;
        // The source is not consumed.
        close(
            volume(&kernel, id)?,
            500.0,
            1e-6,
            "source volume after translate",
        )?;

        // Bad input is rejected, and an unknown handle is not silently translated.
        assert!(matches!(
            kernel.translate(id, [f64::NAN, 0.0, 0.0]),
            Err(KernelError::Invalid(_))
        ));
        let foreign = cad_contract::kernel::SolidId::from_raw(9_999);
        assert_eq!(
            kernel.translate(foreign, [1.0, 0.0, 0.0]),
            Err(KernelError::UnknownSolid(foreign))
        );
        Ok(())
    }

    #[test]
    fn intersection_is_exactly_the_overlap() -> TestResult {
        let mut kernel = TruckKernel::new();
        let a = kernel.extrude(&square(10.0), 10.0)?;
        let b = kernel.extrude(&square_at(4.0, 8.0, 8.0), 10.0)?;
        let common = kernel.boolean(BooleanOp::Intersection, a, b)?;
        // Overlap is 2 x 2 x 10 = 40 mm³.
        close(volume(&kernel, common)?, 40.0, 0.5, "intersection volume")
    }

    #[test]
    fn union_counts_the_overlap_once() -> TestResult {
        let mut kernel = TruckKernel::new();
        let a = kernel.extrude(&square(10.0), 10.0)?;
        let b = kernel.extrude(&square_at(4.0, 8.0, 8.0), 10.0)?;
        let joined = kernel.boolean(BooleanOp::Union, a, b)?;
        // 1000 + 160 - 40 = 1120 mm³.
        close(volume(&kernel, joined)?, 1120.0, 1.0, "union volume")
    }

    #[test]
    fn operands_survive_a_boolean() -> TestResult {
        let mut kernel = TruckKernel::new();
        let a = kernel.extrude(&square(10.0), 10.0)?;
        let b = kernel.extrude(&square_at(4.0, 8.0, 8.0), 10.0)?;
        kernel.boolean(BooleanOp::Union, a, b)?;
        // The contract says neither input is consumed.
        close(volume(&kernel, a)?, 1000.0, 1e-6, "operand a after boolean")?;
        close(volume(&kernel, b)?, 160.0, 1e-6, "operand b after boolean")
    }

    #[test]
    fn release_is_idempotent_and_invalidates_the_handle() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        assert_eq!(kernel.live_solids(), 1);
        kernel.release(id);
        assert_eq!(kernel.live_solids(), 0);
        kernel.release(id); // must not panic
        assert_eq!(
            kernel.tessellate(id, 0.05),
            Err(KernelError::UnknownSolid(id))
        );
        Ok(())
    }

    #[test]
    fn bad_input_is_reported_as_invalid_not_as_a_kernel_crash() -> TestResult {
        let mut kernel = TruckKernel::new();
        assert!(matches!(
            kernel.extrude(&[[0.0, 0.0], [1.0, 1.0]], 5.0),
            Err(KernelError::Invalid(_))
        ));
        assert!(matches!(
            kernel.extrude(&square(10.0), 0.0),
            Err(KernelError::Invalid(_))
        ));
        assert!(matches!(
            kernel.extrude(&square(10.0), f64::NAN),
            Err(KernelError::Invalid(_))
        ));
        // Collinear points enclose no area, so this is invalid input, not a kernel failure.
        assert!(matches!(
            kernel.extrude(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]], 5.0),
            Err(KernelError::Invalid(_))
        ));
        Ok(())
    }

    #[test]
    fn tessellation_tolerance_is_bounded() -> TestResult {
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        assert!(matches!(
            kernel.tessellate(id, 0.0),
            Err(KernelError::Invalid(_))
        ));
        assert!(matches!(
            kernel.tessellate(id, f64::NAN),
            Err(KernelError::Invalid(_))
        ));
        assert!(matches!(
            kernel.tessellate(id, MIN_TOLERANCE_MM / 10.0),
            Err(KernelError::Invalid(_))
        ));
        kernel.tessellate(id, MIN_TOLERANCE_MM)?;
        Ok(())
    }

    #[test]
    fn handles_from_another_kernel_are_rejected() -> TestResult {
        let mut first = TruckKernel::new();
        let second = TruckKernel::new();
        let id = first.extrude(&square(10.0), 5.0)?;
        // `second` never issued this handle.
        assert_eq!(
            second.tessellate(id, 0.05),
            Err(KernelError::UnknownSolid(id))
        );
        first.tessellate(id, 0.05)?;
        Ok(())
    }

    #[test]
    fn repeated_booleans_agree_in_volume() -> TestResult {
        // Volume must be stable across identical runs even where truck's face ordering is not.
        let mut kernel = TruckKernel::new();
        let mut volumes = Vec::new();
        for _ in 0..3 {
            let a = kernel.extrude(&square(10.0), 10.0)?;
            let b = kernel.extrude(&square_at(4.0, 8.0, 8.0), 10.0)?;
            let joined = kernel.boolean(BooleanOp::Union, a, b)?;
            volumes.push(volume(&kernel, joined)?);
        }
        let first = volumes.first().copied().unwrap_or(f64::NAN);
        for v in &volumes {
            close(*v, first, 1e-6, "volume across identical runs")?;
        }
        Ok(())
    }

    // --- cancellation and deadlines ------------------------------------------------------

    /// A deadline that has already passed must stop the call before it does any work, and the
    /// instance must be undamaged afterwards.
    #[test]
    fn an_expired_deadline_returns_timeout_and_leaves_the_kernel_usable() -> TestResult {
        let mut kernel = TruckKernel::new();
        let expired = Budget::with_timeout(Duration::ZERO);

        let extrude = kernel.extrude_with_budget(&square(10.0), 5.0, &expired);
        assert!(
            matches!(&extrude, Err(KernelError::Timeout(_))),
            "expected a timeout, got {extrude:?}"
        );
        assert_eq!(
            extrude.as_ref().err().map(KernelError::kind),
            Some(cad_contract::error::ErrorKind::Timeout),
            "the UI must be able to branch on the kind"
        );
        assert_eq!(kernel.live_solids(), 0, "no half-built solid may be kept");

        // The same instance, with no budget, must still produce exact geometry.
        let id = kernel.extrude(&square(10.0), 5.0)?;
        close(volume(&kernel, id)?, 500.0, 1e-6, "volume after a timeout")
    }

    #[test]
    fn every_budgeted_entry_point_enforces_the_deadline() -> TestResult {
        let mut kernel = TruckKernel::new();
        let expired = Budget::with_timeout(Duration::ZERO);
        let solid = {
            // Built with an unlimited budget, so the fixtures below exist to be refused.
            let mut fresh = TruckKernel::new();
            fresh.extrude(&square(10.0), 5.0)?
        };
        // `solid` was issued by another instance, so `kernel` would report `UnknownSolid` for it;
        // the budget check runs first, so an expired deadline is reported regardless of the
        // handle's validity — a UI that has run out of time must not wait on a lookup either.
        assert!(matches!(
            kernel.boolean_with_budget(BooleanOp::Union, solid, solid, &expired),
            Err(KernelError::Timeout(_))
        ));
        assert!(matches!(
            kernel.tessellate_with_budget(solid, 0.05, &expired),
            Err(KernelError::Timeout(_))
        ));
        Ok(())
    }

    /// A token cancelled before the call stops it, and `reset` makes the same token usable again —
    /// a UI that cancels one command must be able to run the next one with the same token.
    #[test]
    fn a_cancelled_token_stops_the_operation_and_can_be_reset() -> TestResult {
        let token = CancellationToken::new();
        token.cancel();
        let budget = Budget::with_cancel_token(token.clone());
        let mut kernel = TruckKernel::new();

        let extrude = kernel.extrude_with_budget(&square(10.0), 5.0, &budget);
        assert!(
            matches!(&extrude, Err(KernelError::Cancelled)),
            "expected cancellation, got {extrude:?}"
        );
        assert_eq!(kernel.live_solids(), 0);

        token.reset();
        let id = kernel.extrude_with_budget(&square(10.0), 5.0, &budget)?;
        close(
            volume(&kernel, id)?,
            500.0,
            1e-6,
            "volume after resetting the token",
        )
    }

    /// Cancellation observed **mid-operation** by another thread, followed by a successful
    /// operation on the very same instance.
    ///
    /// The workload is unbounded on purpose: it loops until the token fires (with a safety cap that
    /// would fail the test rather than hang it), so the assertion is never racing the clock. This is
    /// the acceptance test for `task.adapter.cancellation`.
    #[test]
    fn cancelling_a_running_operation_is_observed_and_the_instance_survives() -> TestResult {
        let token = CancellationToken::new();
        let budget = Budget::with_cancel_token(token.clone());
        let mut kernel = TruckKernel::new();

        let worker = std::thread::spawn(move || {
            let mut iterations = 0u32;
            // Handles created by *completed* calls in the current chain. A cancelled operation
            // leaves these behind — correctly so: they are results the caller already owns — so
            // the test releases them explicitly instead of pretending they never existed.
            let mut pending: Vec<cad_contract::kernel::SolidId> = Vec::new();
            let outcome: TestResult = loop {
                if iterations >= 2_000 {
                    break Err(KernelError::Internal(
                        "the workload finished before cancellation was observed".into(),
                    ));
                }
                iterations = iterations.saturating_add(1);
                let step = (|| -> TestResult {
                    let a = kernel.extrude_with_budget(&square(10.0), 10.0, &budget)?;
                    pending.push(a);
                    let b = kernel.extrude_with_budget(&square_at(4.0, 8.0, 8.0), 10.0, &budget)?;
                    pending.push(b);
                    let joined = kernel.boolean_with_budget(BooleanOp::Union, a, b, &budget)?;
                    pending.push(joined);
                    kernel.release(joined);
                    kernel.release(a);
                    kernel.release(b);
                    pending.clear();
                    Ok(())
                })();
                if let Err(error) = step {
                    break Err(error);
                }
            };
            (kernel, outcome, iterations, pending)
        });

        // Long enough that the loop is unquestionably running, far shorter than 2 000 unions.
        std::thread::sleep(Duration::from_millis(30));
        token.cancel();

        let (mut kernel, outcome, iterations, pending) = worker
            .join()
            .map_err(|_| KernelError::Internal("the kernel worker panicked".into()))?;
        assert!(
            matches!(&outcome, Err(KernelError::Cancelled)),
            "expected Cancelled after {iterations} iteration(s), got {outcome:?}"
        );

        // State after the cancellation: only results of calls that *completed* exist, the aborted
        // boolean inserted nothing, and releasing the leftovers empties the kernel exactly.
        assert_eq!(
            kernel.live_solids(),
            pending.len(),
            "only the completed calls of the interrupted chain can be live"
        );
        for id in &pending {
            kernel.release(*id);
        }
        assert_eq!(kernel.live_solids(), 0, "a cancelled chain leaks nothing");

        // And the same instance keeps working.
        let id = kernel.extrude(&square(10.0), 5.0)?;
        close(
            volume(&kernel, id)?,
            500.0,
            1e-6,
            "volume after a mid-flight cancellation",
        )
    }

    /// The plain trait path must be impossible to interrupt: it is the delegated, unlimited budget.
    #[test]
    fn the_trait_path_never_reports_cancellation() -> TestResult {
        let token = CancellationToken::new();
        token.cancel(); // no effect: the trait methods do not take this token
        let mut kernel = TruckKernel::new();
        let id = kernel.extrude(&square(10.0), 5.0)?;
        close(volume(&kernel, id)?, 500.0, 1e-6, "unbudgeted volume")
    }
}

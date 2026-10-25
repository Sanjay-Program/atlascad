//! The geometry-ops agent: the first concrete [`Agent`](cad_agent_api::agent::Agent) in this
//! workspace.
//!
//! The agent runtime existed before this crate, but nothing exercised it end to end: there was a
//! trait, a registry, a schema checker and a permission model, and no agent that a caller could
//! actually run. This crate supplies one — a small, honest geometry service that exposes the
//! contract kernel's operations to an agent loop.
//!
//! # What it does
//!
//! One JSON object in, one JSON object out. The `op` field selects the operation:
//!
//! | `op` | required fields | result |
//! |---|---|---|
//! | `extrude` | `profile`, `height_mm` | a solid handle |
//! | `extrude_from_workspace` | `profile_file`, `height_mm` | a solid handle |
//! | `boolean` | `operation`, `a`, `b` | a solid handle |
//! | `tessellate` | `solid` | triangle/vertex counts, volume, area, bounding box |
//! | `volume` | `solid` | volume in mm³ |
//! | `bounding_box` | `solid` | min and max corner |
//! | `release` | `solid` | — |
//! | `capabilities` | — | the kernel's name, version and advertised capabilities |
//!
//! Solid handles are the kernel's own session handles, valid for this agent's lifetime. They are
//! **not** persistent: they must never be written into a project file (see
//! [`cad_contract::kernel::SolidId`]).
//!
//! # What it deliberately does not do
//!
//! * No filesystem write. The agent reads at most one JSON profile file, and only when the host
//!   granted [`Permission::ReadWorkspace`].
//! * No network, no shell, no secrets.
//! * No document, no undo/redo, no parametric history. A solid handle is not a feature tree.
//! * No geometry in the reply. The reply carries *summaries* — counts and measurements — so the
//!   output budget is bounded regardless of how complex the solid is.
//!
//! # Permissions
//!
//! The agent asks for exactly one capability, [`Permission::ReadWorkspace`], and uses it for one
//! operation. Every other operation is pure computation over the agent's own in-memory kernel and
//! needs no grant. If the host does not grant `read_workspace`, the agent still activates and still
//! answers every other operation — only `extrude_from_workspace` is refused, with
//! [`AgentError::PermissionDenied`].
//!
//! # Cancellation
//!
//! The caller owns the [`CancellationToken`]: it is handed to the constructor, cloned into every
//! call's budget, and shared with the caller. Cancelling it stops an in-flight kernel operation at
//! its next checkpoint and the call returns [`AgentError::Cancelled`]. Because the token is shared,
//! the caller must [`reset`](CancellationToken::reset) it before the next call — the same rule the
//! kernel adapter documents for a UI that cancels one command and then runs the next.
//!
//! The call's deadline comes from the runtime: [`AgentContext::deadline_ms`] is an absolute
//! instant, and the agent turns it into the kernel's own deadline. An expired deadline produces
//! [`AgentError::Timeout`].

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::path::{Component, Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use cad_agent_api::AgentContext;
use cad_agent_api::agent::Agent;
use cad_agent_api::contract::{AGENT_CONTRACT_VERSION, AgentContract, AgentId, AgentMeta, Budgets};
use cad_agent_api::error::AgentError;
use cad_agent_api::permission::{Permission, PermissionSet};
use cad_contract::error::KernelError;
use cad_contract::kernel::{BooleanOp, Kernel, SolidId};
use cad_contract::tol::DEFAULT_LINEAR_MM;
use cad_kernel_truck::{Budget, CancellationToken, Deadline, TruckKernel};
use serde_json::{Value, json};

/// The stable id of this agent.
///
/// The `agent.` prefix matches the id the engineering manifest uses for this entry, so the
/// manifest and the running agent are named by one string rather than two that must be kept in
/// step by hand.
pub const AGENT_ID: &str = "agent.cad.geometry-ops";

/// The largest profile file the agent will read, in bytes.
///
/// A profile is a list of `[x, y]` pairs; a megabyte is already several tens of thousands of
/// points, far past anything a human would draw and past the point where the kernel's own cost
/// stops being dominated by the tessellation.
pub const PROFILE_FILE_MAX_BYTES: u64 = 1 << 20;

/// The largest number of points a profile may contain.
pub const MAX_PROFILE_POINTS: usize = 4096;

/// The largest extrusion height the agent accepts, in millimetres.
///
/// 100 m of material is far beyond any part a user would model, and the bound stops a hostile
/// profile from asking the kernel for a solid that exhausts memory.
pub const MAX_HEIGHT_MM: f64 = 100_000.0;

/// The smallest extrusion height the agent accepts, in millimetres.
///
/// A zero or negative height has no volume and would ask the kernel for a degenerate solid.
pub const MIN_HEIGHT_MM: f64 = 0.001;

/// The largest chord tolerance the agent accepts, in millimetres.
///
/// A tolerance larger than the geometry produces a meaningless mesh rather than an error, so it
/// is refused rather than accepted and ignored.
pub const MAX_CHORD_TOL_MM: f64 = 10.0;

/// The smallest chord tolerance the agent accepts, in millimetres.
///
/// The kernel's own minimum is 1e-4 mm; asking for less is refused before the kernel sees it.
pub const MIN_CHORD_TOL_MM: f64 = 0.0001;

/// The geometry-ops agent.
///
/// Holds one kernel for its lifetime, so solid handles stay valid across calls. The kernel is
/// behind a [`Mutex`] because [`Agent::run`] takes `&self` while the kernel needs `&mut self`, and
/// because truck's topology handles are not reentrant: one kernel, one thread at a time.
pub struct GeometryAgent {
    contract: AgentContract,
    kernel: Mutex<TruckKernel>,
    cancel: CancellationToken,
    /// The confined directory profile files may be read from, if the host configured one.
    workspace: Option<PathBuf>,
}

impl GeometryAgent {
    /// An agent with no workspace: every operation except `extrude_from_workspace` works, and that
    /// one is refused because there is nowhere to read from.
    pub fn new(cancel: CancellationToken) -> Result<Self, AgentError> {
        let contract = AgentContract {
            api_version: AGENT_CONTRACT_VERSION,
            meta: AgentMeta {
                id: AgentId::new(AGENT_ID)?,
                version: env!("CARGO_PKG_VERSION").to_string(),
                name: "Geometry operations".to_string(),
                purpose: "Extrude, combine, tessellate and measure solids through the shared \
                          kernel contract, with lifecycle, schema, permission and budget \
                          enforcement by the agent runtime."
                    .to_string(),
            },
            // Exactly one capability, used by exactly one operation.
            permissions: PermissionSet::new().with(Permission::ReadWorkspace),
            input_schema: input_schema(),
            output_schema: output_schema(),
            budgets: default_budgets(),
        };
        contract.validate()?;
        Ok(GeometryAgent {
            contract,
            kernel: Mutex::new(TruckKernel::new()),
            cancel,
            workspace: None,
        })
    }

    /// Allow this agent to read profile files from `workspace`.
    ///
    /// The workspace is canonicalised once, here, so that a later comparison of a file's real path
    /// against it cannot be defeated by a symlink planted after construction.
    pub fn with_workspace(mut self, workspace: impl AsRef<Path>) -> Result<Self, AgentError> {
        let root = workspace.as_ref();
        let canonical = root.canonicalize().map_err(|error| {
            AgentError::InvalidContract(format!(
                "workspace {} is not usable: {error}",
                root.display()
            ))
        })?;
        if !canonical.is_dir() {
            return Err(AgentError::InvalidContract(format!(
                "workspace {} is not a directory",
                canonical.display()
            )));
        }
        self.workspace = Some(canonical);
        Ok(self)
    }

    /// Replace the contract's budgets.
    ///
    /// A host that runs this agent in a tighter environment than the defaults — a shared build
    /// server, a browser tab, a per-request slot — needs a smaller call count or a shorter
    /// deadline than a workstation does. The contract is re-validated, so an impossible budget
    /// (a zero call count, a 128 MiB output cap) is refused here rather than at the first call.
    pub fn with_budgets(mut self, budgets: Budgets) -> Result<Self, AgentError> {
        budgets.validate()?;
        self.contract.budgets = budgets;
        self.contract.validate()?;
        Ok(self)
    }

    /// The agent's declared contract.
    pub fn contract(&self) -> &AgentContract {
        &self.contract
    }

    /// The cancellation token the caller supplied, so a host can cancel from another thread.
    pub fn cancel_token(&self) -> CancellationToken {
        self.cancel.clone()
    }

    /// The kernel's advertised capabilities, for a caller that wants to know what is on offer.
    pub fn capabilities(&self) -> Value {
        let kernel = self.lock();
        capabilities_of(&kernel)
    }

    /// Take the kernel lock, recovering from poisoning.
    ///
    /// A panic inside the kernel is caught by the adapter itself and turned into an error, so the
    /// lock is only poisoned if the *agent* panics while holding it. Recovering the guard rather
    /// than propagating the poison keeps the agent usable and lets the runtime's own quarantine
    /// logic decide what happens next.
    fn lock(&self) -> std::sync::MutexGuard<'_, TruckKernel> {
        match self.kernel.lock() {
            Ok(guard) => guard,
            Err(poisoned) => poisoned.into_inner(),
        }
    }

    /// The budget for one call: the runtime's deadline plus the caller's cancellation token.
    fn budget(&self, context: &AgentContext) -> Budget {
        let remaining_ms = context.deadline_ms().saturating_sub(now_ms());
        Budget::new(
            self.cancel.clone(),
            Deadline::after(Duration::from_millis(remaining_ms)),
        )
    }

    /// Read one JSON profile file from the confined workspace.
    ///
    /// The name must be a single path component: separators, `..` and absolute paths are refused
    /// before any filesystem call, and the resolved file must still be inside the workspace after
    /// symlinks are followed.
    fn read_workspace_profile(&self, name: &str) -> Result<Vec<[f64; 2]>, AgentError> {
        let root = self.workspace.as_ref().ok_or_else(|| {
            AgentError::InvalidInput("this agent has no workspace to read profiles from".into())
        })?;
        let path = confined_path(root, name)?;
        // Canonicalise the *file*, not just its parent: a symlink inside the workspace pointing at
        // /etc/passwd resolves to a path outside the workspace and is refused here.
        let real = path.canonicalize().map_err(|error| {
            AgentError::InvalidInput(format!("cannot open profile {name:?}: {error}"))
        })?;
        if !real.starts_with(root) {
            return Err(AgentError::PermissionDenied {
                agent: self.contract.meta.id.clone(),
                permission: Permission::ReadWorkspace.label().to_string(),
            });
        }
        let metadata = std::fs::metadata(&real).map_err(|error| {
            AgentError::InvalidInput(format!("cannot stat profile {name:?}: {error}"))
        })?;
        if metadata.len() > PROFILE_FILE_MAX_BYTES {
            return Err(AgentError::BudgetExhausted(format!(
                "profile {name:?} is {} bytes; the limit is {PROFILE_FILE_MAX_BYTES}",
                metadata.len()
            )));
        }
        let bytes = std::fs::read(&real).map_err(|error| {
            AgentError::InvalidInput(format!("cannot read profile {name:?}: {error}"))
        })?;
        let text = String::from_utf8(bytes).map_err(|error| {
            AgentError::InvalidInput(format!("profile {name:?} is not valid UTF-8: {error}"))
        })?;
        parse_profile(&text)
    }

    /// Translate a kernel error into the agent error taxonomy.
    ///
    /// Cancellation and timeout keep their identity: they are the caller's decisions, not the
    /// agent's failures. Invalid input stays invalid input. Everything else is a failure of the
    /// operation itself.
    fn map_kernel_error(&self, error: KernelError) -> AgentError {
        match error {
            KernelError::Cancelled => AgentError::Cancelled,
            KernelError::Timeout(elapsed) => AgentError::Timeout(elapsed),
            KernelError::Invalid(message) => AgentError::InvalidInput(message),
            KernelError::Unsupported { capability } => AgentError::InvalidInput(format!(
                "{} is not supported by this kernel",
                capability.label()
            )),
            KernelError::UnknownSolid(handle) => {
                AgentError::InvalidInput(format!("unknown solid handle {handle:?}"))
            }
            KernelError::TooLarge(message) => AgentError::InvalidInput(message),
            other => AgentError::Failed {
                agent: self.contract.meta.id.clone(),
                message: other.to_string(),
            },
        }
    }
}

impl Agent for GeometryAgent {
    fn contract(&self) -> &AgentContract {
        &self.contract
    }

    fn run(&self, context: &AgentContext, input: Value) -> Result<Value, AgentError> {
        let op = input
            .get("op")
            .and_then(Value::as_str)
            .ok_or_else(|| AgentError::InvalidInput("input has no \"op\" string".into()))?
            .to_string();
        let budget = self.budget(context);
        let mut kernel = self.lock();

        match op.as_str() {
            // The kernel is already locked here, so the reply is built from the guard rather than
            // through `capabilities`, which would lock a second time and deadlock.
            "capabilities" => Ok(capabilities_of(&kernel)),
            "extrude" => {
                let profile = profile_from(&input)?;
                let height = height_from(&input)?;
                let solid = kernel
                    .extrude_with_budget(&profile, height, &budget)
                    .map_err(|error| self.map_kernel_error(error))?;
                Ok(solid_reply(&op, solid))
            }
            "extrude_from_workspace" => {
                // Capability-based access control, enforced by the agent against the grant the
                // runtime computed (contract ∩ owner decision).
                if !context.allows(Permission::ReadWorkspace) {
                    return Err(AgentError::PermissionDenied {
                        agent: self.contract.meta.id.clone(),
                        permission: Permission::ReadWorkspace.label().to_string(),
                    });
                }
                let name = string_from(&input, "profile_file")?;
                let height = height_from(&input)?;
                // The budget is checked before the read, not after: a call that has already run out
                // of time should not spend any of it on the filesystem.
                budget
                    .check()
                    .map_err(|error| self.map_kernel_error(error))?;
                let profile = self.read_workspace_profile(&name)?;
                let solid = kernel
                    .extrude_with_budget(&profile, height, &budget)
                    .map_err(|error| self.map_kernel_error(error))?;
                Ok(solid_reply(&op, solid))
            }
            "boolean" => {
                let operation = boolean_from(&input)?;
                let a = solid_from(&input, "a")?;
                let b = solid_from(&input, "b")?;
                let solid = kernel
                    .boolean_with_budget(operation, a, b, &budget)
                    .map_err(|error| self.map_kernel_error(error))?;
                Ok(solid_reply(&op, solid))
            }
            "tessellate" => {
                let solid = solid_from(&input, "solid")?;
                let tolerance = tolerance_from(&input)?.unwrap_or(DEFAULT_LINEAR_MM);
                let mesh = kernel
                    .tessellate_with_budget(solid, tolerance, &budget)
                    .map_err(|error| self.map_kernel_error(error))?;
                let box3 = mesh.bounding_box();
                Ok(json!({
                    "op": op,
                    "ok": true,
                    "triangles": mesh.triangle_count(),
                    "vertices": mesh.vertex_count(),
                    "volume_mm3": mesh.signed_volume().abs(),
                    "area_mm2": mesh.surface_area(),
                    "open_edges": mesh.open_edges(),
                    "min": box3.map(|b| b.min),
                    "max": box3.map(|b| b.max),
                }))
            }
            "volume" => {
                let solid = solid_from(&input, "solid")?;
                let volume = kernel
                    .volume_with_budget(solid, &budget)
                    .map_err(|error| self.map_kernel_error(error))?;
                Ok(json!({"op": op, "ok": true, "volume_mm3": volume}))
            }
            "bounding_box" => {
                let solid = solid_from(&input, "solid")?;
                let box3 = kernel
                    .bounding_box_with_budget(solid, &budget)
                    .map_err(|error| self.map_kernel_error(error))?;
                Ok(json!({"op": op, "ok": true, "min": box3.min, "max": box3.max}))
            }
            "release" => {
                let solid = solid_from(&input, "solid")?;
                kernel.release(solid);
                Ok(json!({"op": op, "ok": true}))
            }
            other => Err(AgentError::InvalidInput(format!(
                "unknown op {other:?}; expected one of {}",
                OPERATIONS.join(", ")
            ))),
        }
    }
}

/// The operations this agent understands, for the error message and the schema's `enum`.
pub const OPERATIONS: &[&str] = &[
    "extrude",
    "extrude_from_workspace",
    "boolean",
    "tessellate",
    "volume",
    "bounding_box",
    "release",
    "capabilities",
];

/// The default budgets: a long-lived agent with many calls, a small reply and half a minute of
/// wall clock per call.
fn default_budgets() -> Budgets {
    Budgets {
        max_calls: 10_000,
        // The reply is a summary, never geometry: a kilobyte is already generous.
        max_output_bytes: 8 << 10,
        max_wall_time_ms: 30_000,
    }
}

/// The declared input schema.
///
/// Every bound the agent relies on is declared here, so the runtime refuses a hostile input
/// before the agent sees it: a profile of at most [`MAX_PROFILE_POINTS`] points, a height of at
/// most [`MAX_HEIGHT_MM`] mm, a tolerance of at most [`MAX_CHORD_TOL_MM`] mm, and a file name of
/// at most 128 characters.
fn input_schema() -> Value {
    json!({
        "type": "object",
        "required": ["op"],
        "properties": {
            "op": {
                "type": "string",
                "enum": OPERATIONS,
                "description": "The operation to perform."
            },
            "profile": {
                "type": "array",
                "minItems": 3,
                "maxItems": MAX_PROFILE_POINTS,
                "description": "A closed planar profile as [x, y] pairs, in millimetres.",
                "items": {
                    "type": "array",
                    "minItems": 2,
                    "maxItems": 2,
                    "items": {"type": "number"}
                }
            },
            "profile_file": {
                "type": "string",
                "minLength": 1,
                "maxLength": 128,
                "description": "A file name, relative to the agent's workspace."
            },
            "height_mm": {
                "type": "number",
                "minimum": MIN_HEIGHT_MM,
                "maximum": MAX_HEIGHT_MM,
                "description": "The extrusion height in millimetres."
            },
            "operation": {
                "type": "string",
                "enum": ["union", "intersection", "difference"]
            },
            "a": {"type": "integer", "minimum": 1},
            "b": {"type": "integer", "minimum": 1},
            "solid": {"type": "integer", "minimum": 1},
            "chord_tol_mm": {
                "type": "number",
                "minimum": MIN_CHORD_TOL_MM,
                "maximum": MAX_CHORD_TOL_MM
            }
        }
    })
}

/// The declared output schema.
///
/// A reply carries measurements, never geometry, so the size of a reply does not depend on the
/// size of the solid.
fn output_schema() -> Value {
    json!({
        "type": "object",
        "required": ["op", "ok"],
        "properties": {
            "op": {"type": "string"},
            "ok": {"type": "boolean"},
            "solid": {"type": "integer", "minimum": 1},
            "triangles": {"type": "integer", "minimum": 0},
            "vertices": {"type": "integer", "minimum": 0},
            "open_edges": {"type": "integer", "minimum": 0},
            "volume_mm3": {"type": "number"},
            "area_mm2": {"type": "number"},
            "min": {
                "type": "array",
                "minItems": 3,
                "maxItems": 3,
                "items": {"type": "number"}
            },
            "max": {
                "type": "array",
                "minItems": 3,
                "maxItems": 3,
                "items": {"type": "number"}
            }
        }
    })
}

/// The reply for an operation that produced a solid.
fn solid_reply(op: &str, solid: SolidId) -> Value {
    json!({"op": op, "ok": true, "solid": solid.get()})
}

/// The kernel's advertised capabilities.
///
/// Takes the kernel rather than locking it, so the caller decides where the lock is taken. A
/// second lock on the same `Mutex` inside one call would deadlock, which is exactly what the
/// `capabilities` operation would have done if it had locked for itself.
fn capabilities_of(kernel: &TruckKernel) -> Value {
    let info = kernel.info();
    json!({
        "op": "capabilities",
        "ok": true,
        "kernel": info.name,
        "kernel_version": info.version,
        "capabilities": info.capabilities.iter().map(|c| c.label()).collect::<Vec<_>>(),
    })
}

/// The profile from the request body.
fn profile_from(input: &Value) -> Result<Vec<[f64; 2]>, AgentError> {
    let points = input
        .get("profile")
        .and_then(Value::as_array)
        .ok_or_else(|| AgentError::InvalidInput("input has no \"profile\" array".into()))?;
    points_to_profile(points)
}

/// Validate a list of `[x, y]` pairs and convert it to a profile.
fn points_to_profile(points: &[Value]) -> Result<Vec<[f64; 2]>, AgentError> {
    if points.len() < 3 {
        return Err(AgentError::InvalidInput(format!(
            "a profile needs at least 3 points, got {}",
            points.len()
        )));
    }
    if points.len() > MAX_PROFILE_POINTS {
        return Err(AgentError::InvalidInput(format!(
            "a profile may have at most {MAX_PROFILE_POINTS} points, got {}",
            points.len()
        )));
    }
    let mut profile = Vec::with_capacity(points.len());
    for (index, point) in points.iter().enumerate() {
        let pair = point.as_array().ok_or_else(|| {
            AgentError::InvalidInput(format!("profile point {index} is not an array"))
        })?;
        if pair.len() != 2 {
            return Err(AgentError::InvalidInput(format!(
                "profile point {index} has {} coordinates, expected 2",
                pair.len()
            )));
        }
        let x = pair[0].as_f64().ok_or_else(|| {
            AgentError::InvalidInput(format!("profile point {index} has a non-numeric x"))
        })?;
        let y = pair[1].as_f64().ok_or_else(|| {
            AgentError::InvalidInput(format!("profile point {index} has a non-numeric y"))
        })?;
        if !x.is_finite() || !y.is_finite() {
            return Err(AgentError::InvalidInput(format!(
                "profile point {index} is not finite"
            )));
        }
        profile.push([x, y]);
    }
    Ok(profile)
}

/// A required number from the request body.
fn number_from(input: &Value, field: &str) -> Result<f64, AgentError> {
    let value = input
        .get(field)
        .and_then(Value::as_f64)
        .ok_or_else(|| AgentError::InvalidInput(format!("input has no numeric {field:?}")))?;
    if !value.is_finite() {
        return Err(AgentError::InvalidInput(format!("{field} must be finite")));
    }
    Ok(value)
}

/// An optional number from the request body.
fn optional_number_from(input: &Value, field: &str) -> Result<Option<f64>, AgentError> {
    match input.get(field) {
        None | Some(Value::Null) => Ok(None),
        Some(value) => {
            let number = value.as_f64().ok_or_else(|| {
                AgentError::InvalidInput(format!("input has a non-numeric {field:?}"))
            })?;
            if !number.is_finite() {
                return Err(AgentError::InvalidInput(format!("{field} must be finite")));
            }
            Ok(Some(number))
        }
    }
}

/// The extrusion height, bounded the way the declared schema bounds it.
///
/// The runtime enforces the schema before the agent runs, so this check is redundant *through the
/// runtime*. It is not redundant: the agent is a library, and a caller that uses it directly gets
/// the same bound. The two checks are deliberately identical rather than merely similar, so the
/// contract and the behaviour cannot drift apart.
fn height_from(input: &Value) -> Result<f64, AgentError> {
    let height = number_from(input, "height_mm")?;
    if height < MIN_HEIGHT_MM {
        return Err(AgentError::InvalidInput(format!(
            "height_mm must be at least {MIN_HEIGHT_MM}, got {height}"
        )));
    }
    if height > MAX_HEIGHT_MM {
        return Err(AgentError::InvalidInput(format!(
            "height_mm must be at most {MAX_HEIGHT_MM}, got {height}"
        )));
    }
    Ok(height)
}

/// The chord tolerance, bounded the way the declared schema bounds it. `None` means "not
/// supplied", and the kernel's own default applies.
fn tolerance_from(input: &Value) -> Result<Option<f64>, AgentError> {
    let Some(tolerance) = optional_number_from(input, "chord_tol_mm")? else {
        return Ok(None);
    };
    if tolerance < MIN_CHORD_TOL_MM {
        return Err(AgentError::InvalidInput(format!(
            "chord_tol_mm must be at least {MIN_CHORD_TOL_MM}, got {tolerance}"
        )));
    }
    if tolerance > MAX_CHORD_TOL_MM {
        return Err(AgentError::InvalidInput(format!(
            "chord_tol_mm must be at most {MAX_CHORD_TOL_MM}, got {tolerance}"
        )));
    }
    Ok(Some(tolerance))
}

/// A required string from the request body.
fn string_from(input: &Value, field: &str) -> Result<String, AgentError> {
    input
        .get(field)
        .and_then(Value::as_str)
        .map(str::to_string)
        .ok_or_else(|| AgentError::InvalidInput(format!("input has no string {field:?}")))
}

/// A required solid handle from the request body.
fn solid_from(input: &Value, field: &str) -> Result<SolidId, AgentError> {
    let raw = input
        .get(field)
        .and_then(Value::as_u64)
        .ok_or_else(|| AgentError::InvalidInput(format!("input has no integer {field:?}")))?;
    if raw == 0 {
        return Err(AgentError::InvalidInput(format!(
            "{field} must be a handle this agent issued; 0 is reserved for \"none\""
        )));
    }
    Ok(SolidId::from_raw(raw))
}

/// The boolean operation from the request body.
fn boolean_from(input: &Value) -> Result<BooleanOp, AgentError> {
    match string_from(input, "operation")?.as_str() {
        "union" => Ok(BooleanOp::Union),
        "intersection" => Ok(BooleanOp::Intersection),
        "difference" => Ok(BooleanOp::Difference),
        other => Err(AgentError::InvalidInput(format!(
            "unknown boolean operation {other:?}"
        ))),
    }
}

/// Parse a profile file: a JSON array of `[x, y]` pairs.
fn parse_profile(text: &str) -> Result<Vec<[f64; 2]>, AgentError> {
    let value: Value = serde_json::from_str(text)
        .map_err(|error| AgentError::InvalidInput(format!("profile is not valid JSON: {error}")))?;
    let points = value.as_array().ok_or_else(|| {
        AgentError::InvalidInput("a profile file must contain an array of [x, y] pairs".into())
    })?;
    points_to_profile(points)
}

/// Join `name` to `root`, refusing anything that is not a single, ordinary file name.
///
/// The check is on *path components*, not on the characters of the string: `..` is a parent
/// directory and is refused, while `a..b.json` is an ordinary file name and is allowed.
fn confined_path(root: &Path, name: &str) -> Result<PathBuf, AgentError> {
    let candidate = Path::new(name);
    let mut components = candidate.components();
    let Some(Component::Normal(component)) = components.next() else {
        return Err(AgentError::InvalidInput(format!(
            "{name:?} is not a file name inside the workspace"
        )));
    };
    if components.next().is_some() {
        return Err(AgentError::InvalidInput(format!(
            "{name:?} is not a file name inside the workspace"
        )));
    }
    let text = component
        .to_str()
        .ok_or_else(|| AgentError::InvalidInput("the file name is not valid UTF-8".to_string()))?;
    // A backslash is a separator on Windows and a legal character on Unix; refusing it keeps one
    // workspace rule for both platforms.
    if text.contains('\\') || text.chars().any(char::is_control) {
        return Err(AgentError::InvalidInput(format!(
            "{name:?} is not a file name inside the workspace"
        )));
    }
    Ok(root.join(text))
}

/// Milliseconds since the Unix epoch.
fn now_ms() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::{
        AGENT_ID, AgentContext, CancellationToken, GeometryAgent, MAX_CHORD_TOL_MM, MAX_HEIGHT_MM,
        MAX_PROFILE_POINTS, MIN_CHORD_TOL_MM, MIN_HEIGHT_MM, PermissionSet, confined_path,
        parse_profile,
    };
    use cad_agent_api::agent::Agent;
    use cad_agent_api::error::AgentError;
    use cad_agent_api::permission::Permission;
    use serde_json::json;

    type TestResult = Result<(), AgentError>;

    /// A context with a deadline one minute out and the given permissions.
    fn context(permissions: PermissionSet) -> AgentContext {
        let deadline = now_ms() + 60_000;
        AgentContext::new(permissions, deadline, 8 << 10)
    }

    fn now_ms() -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    fn square(size: f64) -> serde_json::Value {
        json!([[0.0, 0.0], [size, 0.0], [size, size], [0.0, size]])
    }

    #[test]
    fn the_contract_is_valid_and_declares_one_permission() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        agent.contract().validate()?;
        assert_eq!(agent.contract().meta.id.as_str(), AGENT_ID);
        assert!(
            agent
                .contract()
                .permissions
                .contains(Permission::ReadWorkspace)
        );
        assert!(!agent.contract().permissions.contains(Permission::Shell));
        assert!(!agent.contract().permissions.contains(Permission::Network));
        assert!(!agent.contract().permissions.contains(Permission::Secrets));
        Ok(())
    }

    #[test]
    fn extruding_a_square_reports_the_analytic_volume() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        let reply = agent.run(
            &ctx,
            json!({"op": "extrude", "profile": square(10.0), "height_mm": 5.0}),
        )?;
        assert_eq!(reply["ok"], json!(true));
        let solid = reply["solid"].as_u64().unwrap_or(0);
        assert!(solid > 0, "a handle was issued");

        let measured = agent.run(&ctx, json!({"op": "volume", "solid": solid}))?;
        let volume = measured["volume_mm3"].as_f64().unwrap_or(f64::NAN);
        assert!(
            (volume - 500.0).abs() < 1e-6,
            "expected 500 mm³, got {volume}"
        );
        Ok(())
    }

    /// A handle this agent never issued must be refused, not silently accepted.
    #[test]
    fn an_unknown_handle_is_refused() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        let error = match agent.run(&ctx, json!({"op": "volume", "solid": 9999})) {
            Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
            Err(error) => error,
        };
        assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");
        Ok(())
    }

    /// The declared schema is not decoration: the agent refuses what it declares out of bounds.
    #[test]
    fn a_profile_with_too_few_points_is_refused() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        let error = match agent.run(
            &ctx,
            json!({"op": "extrude", "profile": [[0.0, 0.0], [1.0, 0.0]], "height_mm": 5.0}),
        ) {
            Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
            Err(error) => error,
        };
        assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");
        Ok(())
    }

    #[test]
    fn a_non_finite_height_is_refused() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        let error = match agent.run(
            &ctx,
            json!({"op": "extrude", "profile": square(10.0), "height_mm": "tall"}),
        ) {
            Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
            Err(error) => error,
        };
        assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");
        Ok(())
    }

    /// The kernel's boolean difference is not advertised, and the agent says so rather than
    /// pretending it worked.
    #[test]
    fn an_unsupported_boolean_is_reported_as_invalid_input() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        let a = agent.run(
            &ctx,
            json!({"op": "extrude", "profile": square(10.0), "height_mm": 10.0}),
        )?;
        let b = agent.run(
            &ctx,
            json!({"op": "extrude", "profile": square(10.0), "height_mm": 10.0}),
        )?;
        let error = match agent.run(
            &ctx,
            json!({
                "op": "boolean",
                "operation": "difference",
                "a": a["solid"],
                "b": b["solid"]
            }),
        ) {
            Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
            Err(error) => error,
        };
        assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");
        Ok(())
    }

    /// The workspace op needs the grant; without it the agent refuses even though the runtime let
    /// the agent activate.
    #[test]
    fn reading_a_profile_needs_the_read_workspace_grant() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let without = context(PermissionSet::new());
        let error = match agent.run(
            &without,
            json!({"op": "extrude_from_workspace", "profile_file": "p.json", "height_mm": 5.0}),
        ) {
            Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
            Err(error) => error,
        };
        assert!(
            matches!(error, AgentError::PermissionDenied { .. }),
            "{error}"
        );
        Ok(())
    }

    /// Path traversal is refused before any filesystem call.
    #[test]
    fn a_traversing_file_name_is_refused() -> TestResult {
        let root = std::path::Path::new("/tmp");
        for name in [
            "../etc/passwd",
            "/etc/passwd",
            "sub/dir.json",
            "..",
            ".",
            "a\\b.json",
        ] {
            assert!(
                confined_path(root, name).is_err(),
                "{name:?} must not resolve to a path"
            );
        }
        // An ordinary name with dots in it is fine.
        let ok = confined_path(root, "bracket.v2.json");
        assert!(ok.is_ok(), "an ordinary name must be accepted");
        Ok(())
    }

    #[test]
    fn a_profile_file_must_be_an_array_of_pairs() -> TestResult {
        assert!(parse_profile("[[0,0],[1,0],[1,1]]").is_ok());
        assert!(parse_profile("{\"points\": []}").is_err());
        assert!(parse_profile("[[0,0],[1,0]]").is_err());
        assert!(parse_profile("[[0,0],[1,0],[1]]").is_err());
        assert!(parse_profile("not json").is_err());
        Ok(())
    }

    #[test]
    fn the_profile_point_cap_is_enforced() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        let mut points = Vec::new();
        for index in 0..=MAX_PROFILE_POINTS {
            points.push(json!([index as f64, 0.0]));
        }
        let error = match agent.run(
            &ctx,
            json!({"op": "extrude", "profile": points, "height_mm": 5.0}),
        ) {
            Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
            Err(error) => error,
        };
        assert!(matches!(error, AgentError::InvalidInput(_)), "{error}");
        Ok(())
    }

    /// The agent enforces its own bounds, so a caller that uses it directly — bypassing the
    /// runtime — gets the same limits as one that goes through it.
    #[test]
    fn the_agent_enforces_its_own_bounds_without_the_runtime() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let ctx = context(PermissionSet::new());
        for height in [0.0, -1.0, 1.0e9] {
            let error = match agent.run(
                &ctx,
                json!({"op": "extrude", "profile": square(10.0), "height_mm": height}),
            ) {
                Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
                Err(error) => error,
            };
            assert!(
                matches!(error, AgentError::InvalidInput(_)),
                "{height}: {error}"
            );
        }
        for tolerance in [0.0, 100.0] {
            let error = match agent.run(
                &ctx,
                json!({"op": "tessellate", "solid": 1, "chord_tol_mm": tolerance}),
            ) {
                Ok(_) => return Err(AgentError::InvalidInput("should have failed".into())),
                Err(error) => error,
            };
            assert!(
                matches!(error, AgentError::InvalidInput(_)),
                "{tolerance}: {error}"
            );
        }
        Ok(())
    }

    /// The schema and the agent's own checks must agree, or the contract would promise bounds the
    /// agent does not enforce.
    #[test]
    fn the_schema_bounds_match_the_agents_own_bounds() -> TestResult {
        let agent = GeometryAgent::new(CancellationToken::new())?;
        let schema = agent.contract().input_schema.clone();
        let height = schema["properties"]["height_mm"].clone();
        assert_eq!(height["minimum"], json!(MIN_HEIGHT_MM));
        assert_eq!(height["maximum"], json!(MAX_HEIGHT_MM));
        let tolerance = schema["properties"]["chord_tol_mm"].clone();
        assert_eq!(tolerance["minimum"], json!(MIN_CHORD_TOL_MM));
        assert_eq!(tolerance["maximum"], json!(MAX_CHORD_TOL_MM));
        let profile = schema["properties"]["profile"].clone();
        assert_eq!(profile["maxItems"], json!(MAX_PROFILE_POINTS));
        Ok(())
    }
}

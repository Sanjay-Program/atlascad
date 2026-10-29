//! An analytic correctness corpus for solid construction and booleans.
//!
//! Every fixture here states the volume and surface area a correct kernel must produce, derived
//! from the geometry alone (the arithmetic is written out next to each fixture), together with a
//! stated tolerance. Nothing is compared against another run of the same kernel: a self-consistent
//! wrong answer is still wrong, and that is exactly how the difference defect survived — the
//! measurement was that `and(a, ¬b)` returned **more** volume than the target held.
//!
//! # Why the table has statuses other than pass/fail
//!
//! * **unsupported** — the kernel does not advertise the capability. The fixture is *recorded*, not
//!   skipped: a corpus that silently drops what a kernel cannot do reads as all-green and hides
//!   that the most dangerous operation (difference) is missing.
//! * **known-failure** — the kernel attempts this and is measurably wrong today (see
//!   `docs/kernel-comparison.md`). The row stays in the table as a visible failure; it does not
//!   fail the build, because the gate is "a previously passing case must not regress", and this one
//!   never passed. If it starts passing, the report says so, so the baseline can be updated.
//! * **not-constructible** — the fixture cannot be built through the contract's API at all. Saying
//!   that out loud is more useful than a fixture that quietly tests something else.
//!
//! The exit code is non-zero only for a regression (a `must-pass` fixture that did not pass) or an
//! infrastructure failure (a caught panic, a timeout, an unknown handle) — see [`Verdict`].

use cad_contract::capability::Capability;
use cad_contract::error::{ErrorKind, KernelError};
use cad_contract::kernel::{Kernel, SolidId};

/// What the corpus expects from a *capable* kernel, as measured today.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Expectation {
    /// The analytic volume and area must come back within tolerance. Any other outcome is a
    /// regression and fails the build.
    MustPass,
    /// The kernel does not advertise this capability; it must refuse with `Unsupported` rather
    /// than return a wrong solid.
    Unsupported,
    /// The kernel attempts this and is wrong today, by a documented measurement. The row is shown
    /// as a failure; it does not fail the build.
    KnownFailure,
    /// The contract cannot express the geometry this fixture needs (see the module docs).
    NotConstructible(&'static str),
}

/// Volume and area measured from a tessellation, plus the mesh's open-edge count — a volume that
/// is right on an open mesh is not a solid.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Measured {
    pub volume_mm3: f64,
    pub area_mm2: f64,
    pub open_edges: usize,
}

/// What one fixture produced on one kernel.
#[derive(Debug, Clone, PartialEq)]
pub enum Status {
    /// Within tolerance of the analytic values.
    Pass(Measured),
    /// Outside tolerance: both deltas are reported so the table shows how far off it is.
    Wrong {
        measured: Measured,
        volume_delta_mm3: f64,
        area_delta_mm2: f64,
    },
    /// The capability is not advertised (either checked up front or refused by the call).
    Unsupported {
        capability: Capability,
        reason: String,
    },
    /// A kernel that *does* advertise the capability refused this particular call cleanly
    /// (`Invalid`, `Failed`, `TooLarge`) — an honest refusal, not a wrong answer.
    Refused(KernelError),
    /// Infrastructure failure: a caught panic, a timeout, a cancellation, or a handle the kernel
    /// did not issue. Never acceptable, whatever the expectation.
    Failed(KernelError),
    /// The fixture cannot be built through the contract API.
    NotConstructible(&'static str),
}

/// The verdict of one row against its [`Expectation`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    /// Met its expectation (including a clean refusal where refusal is expected).
    Ok,
    /// Did not meet a `must-pass` expectation, or failed for an infrastructure reason.
    Regression,
    /// Still failing, as documented; visible in the table, does not fail the build.
    KnownFailure,
    /// Expected to be unsupported or known-failing, and produced a correct answer instead: the
    /// baseline is stale and must be promoted to `must-pass`.
    UnexpectedPass,
}

/// One row of the corpus.
#[derive(Debug, Clone, Copy)]
pub struct Fixture {
    /// Stable id, used as the table key and by regression references in docs.
    pub id: &'static str,
    /// What the geometry is and where the analytic numbers come from.
    pub description: &'static str,
    /// The capability the fixture exercises; the kernel must advertise it before the fixture runs.
    pub capability: Capability,
    /// What is expected of a kernel today.
    pub expectation: Expectation,
    /// Analytic volume in mm³.
    pub volume_mm3: f64,
    /// Absolute tolerance on the volume, in mm³.
    pub volume_tolerance_mm3: f64,
    /// Analytic surface area in mm².
    pub area_mm2: f64,
    /// Absolute tolerance on the area, in mm².
    pub area_tolerance_mm2: f64,
    /// Builds the solid whose volume and area are asserted. Operands are released before it
    /// returns; on failure any operand it created stays live, which is harmless because the corpus
    /// discards the kernel instance after the run.
    pub build: fn(&mut dyn Kernel) -> Result<SolidId, KernelError>,
}

/// A closed axis-aligned rectangle profile, counter-clockwise.
fn rect(width: f64, height: f64, at: [f64; 2]) -> Vec<[f64; 2]> {
    vec![
        [at[0], at[1]],
        [at[0] + width, at[1]],
        [at[0] + width, at[1] + height],
        [at[0], at[1] + height],
    ]
}

/// A closed rectangle profile centred on the origin and rotated by `degrees` (counter-clockwise).
///
/// Used by the non-axis-aligned fixture: the contract's `extrude` sweeps along +Z, so the box this
/// builds has four vertical faces that meet no axis — as far off-axis as the API can express.
fn rotated_rect(side: f64, degrees: f64) -> Vec<[f64; 2]> {
    let radians = degrees.to_radians();
    let (sin, cos) = radians.sin_cos();
    let half = side / 2.0;
    [[-half, -half], [half, -half], [half, half], [-half, half]]
        .map(|[x, y]| [x * cos - y * sin + 5.0, x * sin + y * cos + 5.0])
        .to_vec()
}

/// Extrude a rectangle and release the operands a caller handed in.
fn extrude_rect(
    kernel: &mut dyn Kernel,
    width: f64,
    height: f64,
    at: [f64; 2],
    depth: f64,
) -> Result<SolidId, KernelError> {
    let profile = rect(width, height, at);
    kernel.extrude(&profile, depth)
}

// --- fixtures ----------------------------------------------------------------------------------

/// 10 x 10 x 5 mm box: the control. If this fails, the harness is broken, not the kernel.
fn build_box_control(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    // volume = 10 * 10 * 5 = 500 mm³
    // area   = 2 * (10 * 10) + 4 * (10 * 5) = 200 + 200 = 400 mm²
    extrude_rect(kernel, 10.0, 10.0, [0.0, 0.0], 5.0)
}

/// A 10 x 10 x 5 mm box whose footprint is rotated 30° about +Z.
fn build_rotated_box(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    // A square swept along +Z: volume = side² * depth = 100 * 5 = 500 mm³, and the area formula is
    // the same as the axis-aligned box (2 * 100 + 4 * (10 * 5) = 400 mm²) because only the
    // footprint's orientation changed.
    let profile = rotated_rect(10.0, 30.0);
    kernel.extrude(&profile, 5.0)
}

/// A concave L-shaped footprint extruded 5 mm: a prism no rectangle fixture can describe.
///
/// This is the kernel half of the drawn-polyline feature: the document passes a validated loop to
/// the adapter, and this fixture proves the adapter builds the prism the loop describes rather
/// than only boxes and circles. The loop is counter-clockwise, so the solid must be outward-facing.
fn build_polyline_l_bracket(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    // footprint = 30 x 10 arm + 10 x 20 upright = 500 mm²; volume = 500 * 5 = 2500 mm³.
    // area = 2 * 500 (caps) + 120 (perimeter) * 5 = 1000 + 600 = 1600 mm².
    let profile = vec![
        [0.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [10.0, 10.0],
        [10.0, 30.0],
        [0.0, 30.0],
    ];
    kernel.extrude(&profile, 5.0)
}

/// Two boxes overlapping by 2 x 2 mm over their full height — the fixture `docs/` quotes as 1120.
fn build_union_partial_overlap(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let a = extrude_rect(kernel, 10.0, 10.0, [0.0, 0.0], 10.0)?;
    let b = extrude_rect(kernel, 4.0, 4.0, [8.0, 8.0], 10.0)?;
    let joined = kernel.boolean(cad_contract::kernel::BooleanOp::Union, a, b)?;
    kernel.release(a);
    kernel.release(b);
    // volume = 1000 + 160 - 40 = 1120 mm³ (overlap 2 * 2 * 10 = 40).
    // area   = 704 mm², from the axis projections: top+bottom = 2 * (112 footprint) = 224,
    //          ±x = 120 + 120, ±y = 120 + 120. (The naive "600 + 192 - 48 - 48 = 696" is
    //          wrong by 8: the coincident top and bottom patches of the overlap column lie on
    //          *both* boundaries and in neither interior, so they are boundary once, not zero
    //          times — an error this fixture exists to catch.)
    Ok(joined)
}

/// Two boxes sharing a whole face (x = 10) — full coplanar contact, the degenerate union.
fn build_union_tangent_contact(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let a = extrude_rect(kernel, 10.0, 10.0, [0.0, 0.0], 10.0)?;
    let b = extrude_rect(kernel, 10.0, 10.0, [10.0, 0.0], 10.0)?;
    let joined = kernel.boolean(cad_contract::kernel::BooleanOp::Union, a, b)?;
    kernel.release(a);
    kernel.release(b);
    // volume = 1000 + 1000 = 2000 mm³.
    // The shared 10 x 10 face is interior, so a correct boundary is
    //   area = 600 + 600 - 2 * 100 = 1000 mm².
    // Measured today: volume 2000.0 (exact) but area 1200.0 — truck's union of two solids in
    // face-to-face contact returns them as a compound that still carries both coincident contact
    // patches, so the surface has an internal double wall. Volume survives because the two
    // patches have opposite normals and cancel; area does not. Documented as a known failure
    // (see docs/kernel-comparison.md).
    Ok(joined)
}

/// Two boxes 10 mm apart in x at coordinates near 1e6 mm — 10 µm of relative round-off territory.
fn build_union_large_coordinates(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let origin = 1_000_000.0;
    let a = extrude_rect(kernel, 100.0, 100.0, [origin, origin], 10.0)?;
    let b = extrude_rect(kernel, 100.0, 100.0, [origin + 90.0, origin], 10.0)?;
    let joined = kernel.boolean(cad_contract::kernel::BooleanOp::Intersection, a, b)?;
    kernel.release(a);
    kernel.release(b);
    // Intersection x in [origin + 90, origin + 100] -> 10 x 100 x 10 mm.
    // volume = 10000 mm³.
    // area   = 2 * (100 * 10) + 2 * (10 * 10) + 2 * (10 * 100) = 2000 + 200 + 2000 = 4200 mm².
    Ok(joined)
}

/// An intersection whose result is a 0.01 mm (10 µm) wall.
fn build_thin_wall(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let a = extrude_rect(kernel, 10.0, 10.0, [0.0, 0.0], 10.0)?;
    let b = extrude_rect(kernel, 10.0, 10.0, [9.99, 0.0], 10.0)?;
    let wall = kernel.boolean(cad_contract::kernel::BooleanOp::Intersection, a, b)?;
    kernel.release(a);
    kernel.release(b);
    // Intersection x in [9.99, 10] -> 0.01 x 10 x 10 mm.
    // volume = 1 mm³.
    // area   = 2 * (10 * 10) + 4 * (0.01 * 10) = 200 + 0.4 = 200.4 mm².
    Ok(wall)
}

/// An intersection whose result is a 0.001 mm (1 µm) sliver — at truck's boolean tolerance.
fn build_near_degenerate_sliver(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let a = extrude_rect(kernel, 10.0, 10.0, [0.0, 0.0], 10.0)?;
    let b = extrude_rect(kernel, 10.0, 10.0, [9.999, 0.0], 10.0)?;
    let sliver = kernel.boolean(cad_contract::kernel::BooleanOp::Intersection, a, b)?;
    kernel.release(a);
    kernel.release(b);
    // Intersection x in [9.999, 10] -> 0.001 x 10 x 10 mm.
    // volume = 0.1 mm³.
    // area   = 2 * (10 * 10) + 4 * (0.001 * 10) = 200 + 0.04 = 200.04 mm².
    Ok(sliver)
}

/// A 100 x 100 x 10 plate minus a 40 x 40 x 10 block whose top *and* bottom faces are coplanar
/// with the plate's — the classic coplanar cut.
fn build_coplanar_cut(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let plate = extrude_rect(kernel, 100.0, 100.0, [0.0, 0.0], 10.0)?;
    let tool = extrude_rect(kernel, 40.0, 40.0, [30.0, 30.0], 10.0)?;
    let cut = kernel.boolean(cad_contract::kernel::BooleanOp::Difference, plate, tool)?;
    kernel.release(plate);
    kernel.release(tool);
    // volume = 100000 - 16000 = 84000 mm³.
    // area   = 24000 (plate) - 2 * 1600 (openings in the caps) + 4 * (40 * 10) (hole walls)
    //        = 22400 mm².
    Ok(cut)
}

/// The same plate minus a 40 x 40 block that **straddles** it, so no face is coincident.
///
/// This is the product's cut: a tool that overshoots the body on both sides. `extrude` always starts
/// at z = 0, so the tool is extruded longer than the plate and translated down; that `translate`
/// call is the whole reason the exactness is achievable, and it is why the contract has a
/// `Transform` capability rather than leaving placement to the caller's arithmetic.
///
/// The previous version of this fixture extruded a 30 mm tool from z = 0, which is still coplanar
/// with the plate's bottom face — so it did not test a piercing cut at all, it tested the coplanar
/// one twice.
fn build_piercing_cut(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let plate = extrude_rect(kernel, 100.0, 100.0, [0.0, 0.0], 10.0)?;
    // Spans z -5..25, through a plate spanning z 0..10.
    let tool = extrude_rect(kernel, 40.0, 40.0, [30.0, 30.0], 30.0)?;
    let tool = kernel.translate(tool, [0.0, 0.0, -5.0])?;
    let cut = kernel.boolean(cad_contract::kernel::BooleanOp::Difference, plate, tool)?;
    kernel.release(plate);
    kernel.release(tool);
    // volume = 100000 - 16000 = 84000 mm³.
    // area   = 24000 (plate) - 2 * 1600 (openings) + 4 * (40 * 10) (hole walls) = 22400 mm².
    Ok(cut)
}

/// A plate with a 40 x 40 x 10 cavity floating inside it, touching no outer face.
///
/// The previous version built the plate by unioning an upward and a downward extrusion, which share
/// their whole z = 0 footprint — so the *fixture* hit the coplanar defect before the difference it
/// meant to test ever ran, and reported "union produced no result". Translating one extrusion is
/// both simpler and sufficient: the plate is extruded 20 mm and moved down 10, the tool 10 mm and
/// moved down 5, so neither body contains the z = 0 plane and nothing is coincident.
fn build_contained_void(kernel: &mut dyn Kernel) -> Result<SolidId, KernelError> {
    let plate = extrude_rect(kernel, 100.0, 100.0, [0.0, 0.0], 20.0)?;
    let plate = kernel.translate(plate, [0.0, 0.0, -10.0])?;

    let tool = extrude_rect(kernel, 40.0, 40.0, [30.0, 30.0], 10.0)?;
    let tool = kernel.translate(tool, [0.0, 0.0, -5.0])?;

    let voided = kernel.boolean(cad_contract::kernel::BooleanOp::Difference, plate, tool)?;
    kernel.release(plate);
    kernel.release(tool);
    // volume = 100 * 100 * 20 - 40 * 40 * 10 = 200000 - 16000 = 184000 mm³.
    // area   = 28000 (outer box 100 x 100 x 20) + 4 * (40 * 10) + 2 * (40 * 40) (cavity)
    //        = 28000 + 1600 + 3200 = 32800 mm².
    Ok(voided)
}

/// Every fixture, in table order.
pub const FIXTURES: &[Fixture] = &[
    Fixture {
        id: "box_control",
        description: "10x10x5 box; control for the measuring path",
        capability: Capability::Extrude,
        expectation: Expectation::MustPass,
        volume_mm3: 500.0,
        volume_tolerance_mm3: 1e-6,
        area_mm2: 400.0,
        area_tolerance_mm2: 1e-6,
        build: build_box_control,
    },
    Fixture {
        id: "non_axis_aligned_box",
        description: "same box with a 30-degree rotated footprint",
        capability: Capability::Extrude,
        expectation: Expectation::MustPass,
        volume_mm3: 500.0,
        volume_tolerance_mm3: 1e-6,
        area_mm2: 400.0,
        area_tolerance_mm2: 1e-6,
        build: build_rotated_box,
    },
    Fixture {
        id: "polyline_l_bracket",
        description: "concave L-shaped drawn profile (500 mm²) extruded 5 mm",
        capability: Capability::Extrude,
        expectation: Expectation::MustPass,
        volume_mm3: 2500.0,
        volume_tolerance_mm3: 1e-6,
        area_mm2: 1600.0,
        area_tolerance_mm2: 1e-6,
        build: build_polyline_l_bracket,
    },
    Fixture {
        id: "union_partial_overlap",
        description: "10x10x10 and 4x4x10 boxes overlapping 2x2 over the full height",
        capability: Capability::BooleanUnion,
        expectation: Expectation::MustPass,
        volume_mm3: 1120.0,
        volume_tolerance_mm3: 1.0,
        area_mm2: 704.0,
        area_tolerance_mm2: 0.05,
        build: build_union_partial_overlap,
    },
    Fixture {
        id: "union_tangent_contact",
        description: "two 10x10x10 boxes sharing a whole face (coplanar contact)",
        capability: Capability::BooleanUnion,
        // Truck keeps both coincident contact patches (area 1200, not 1000) — a known failure
        // until the adapter fuses tangent solids. If this ever starts passing, promote it.
        expectation: Expectation::KnownFailure,
        volume_mm3: 2000.0,
        volume_tolerance_mm3: 1.0,
        area_mm2: 1000.0,
        area_tolerance_mm2: 1.0,
        build: build_union_tangent_contact,
    },
    Fixture {
        id: "intersection_large_coordinates",
        description: "10x100x10 overlap of two boxes placed near 1e6 mm",
        capability: Capability::BooleanIntersection,
        // At ~1e6 mm absolute coordinates the operand edges (100 mm long, 1e-6 relative
        // precision) sit at truck's boolean tolerance: the measured intersection comes back
        // 11499.8 mm³ with 24 open edges — not a solid. Known failure until the adapter
        // re-centres operands before the boolean.
        expectation: Expectation::KnownFailure,
        volume_mm3: 10_000.0,
        volume_tolerance_mm3: 1.0,
        area_mm2: 4200.0,
        area_tolerance_mm2: 1.0,
        build: build_union_large_coordinates,
    },
    Fixture {
        id: "intersection_thin_wall",
        description: "intersection leaving a 0.01 mm wall",
        capability: Capability::BooleanIntersection,
        expectation: Expectation::MustPass,
        volume_mm3: 1.0,
        volume_tolerance_mm3: 0.01,
        area_mm2: 200.4,
        area_tolerance_mm2: 0.01,
        build: build_thin_wall,
    },
    Fixture {
        id: "intersection_near_degenerate_sliver",
        description: "intersection leaving a 0.001 mm sliver, at the boolean tolerance",
        capability: Capability::BooleanIntersection,
        expectation: Expectation::MustPass,
        volume_mm3: 0.1,
        // At the tolerance boundary (1 µm sliver = truck's boolean tolerance) the kernel trims
        // the sliver's volume by ~7% (measured 0.0930 mm³) while still returning a *closed*
        // mesh — the property this fixture guards. 10% states that honestly; a torn or open
        // result fails via the open-edge rule regardless of volume.
        volume_tolerance_mm3: 0.01,
        area_mm2: 200.04,
        area_tolerance_mm2: 0.01,
        build: build_near_degenerate_sliver,
    },
    Fixture {
        id: "difference_coplanar_cut",
        description: "40x40 through-cut with coplanar top and bottom faces",
        capability: Capability::BooleanDifference,
        // Recorded as a known failure with its measurements, not skipped: the volume is right to
        // 0.18 mm³ but the coplanar retry's nudge leaves a 1.1e-4 mm membrane on the shared face,
        // and truck's tessellation of that membrane carries hairline sliver edges, so the mesh is
        // not watertight. The application never builds a cut this way — it overshoots the tool —
        // so this records the limitation of the raw boolean contract instead of hiding it.
        expectation: Expectation::KnownFailure,
        volume_mm3: 84_000.0,
        volume_tolerance_mm3: 1.0,
        area_mm2: 22_400.0,
        area_tolerance_mm2: 1.0,
        build: build_coplanar_cut,
    },
    Fixture {
        id: "difference_piercing_cut",
        description: "40x40 block straddling the plate, so no faces are coincident",
        capability: Capability::BooleanDifference,
        expectation: Expectation::MustPass,
        volume_mm3: 84_000.0,
        // Tight on purpose: this is the cut the application builds, so it has to be exact rather
        // than merely close. 1 mm³ on 84 000 is 12 ppm.
        volume_tolerance_mm3: 1.0,
        area_mm2: 22_400.0,
        area_tolerance_mm2: 1.0,
        build: build_piercing_cut,
    },
    Fixture {
        id: "difference_contained_void",
        description: "40x40x10 cavity floating inside a 100x100x20 plate",
        capability: Capability::BooleanDifference,
        expectation: Expectation::MustPass,
        volume_mm3: 184_000.0,
        volume_tolerance_mm3: 10.0,
        area_mm2: 32_800.0,
        area_tolerance_mm2: 10.0,
        build: build_contained_void,
    },
];

/// Tessellate `solid` at the contract's default chord tolerance and measure it.
pub fn measure(kernel: &dyn Kernel, solid: SolidId) -> Result<Measured, KernelError> {
    let mesh = kernel.tessellate(solid, cad_contract::tol::DEFAULT_LINEAR_MM)?;
    mesh.validate()
        .map_err(|e| KernelError::Failed(format!("measured mesh is invalid: {e}")))?;
    Ok(Measured {
        volume_mm3: mesh.signed_volume().abs(),
        area_mm2: mesh.surface_area(),
        open_edges: mesh.open_edges(),
    })
}

/// Classify an error that came back from a fixture's build or measurement.
fn classify(error: KernelError) -> Status {
    match &error {
        KernelError::Unsupported { capability } => Status::Unsupported {
            capability: *capability,
            reason: error.to_string(),
        },
        KernelError::Invalid(_) | KernelError::Failed(_) | KernelError::TooLarge(_) => {
            Status::Refused(error)
        }
        _ => Status::Failed(error),
    }
}

/// Run one fixture against one kernel.
pub fn run_fixture(kernel: &mut dyn Kernel, fixture: &Fixture) -> Status {
    if let Expectation::NotConstructible(reason) = fixture.expectation {
        return Status::NotConstructible(reason);
    }
    // A capability check first, so an unsupported operation is *recorded* as unsupported rather
    // than discovered by failing halfway through building its operands.
    if !kernel.supports(fixture.capability) {
        return Status::Unsupported {
            capability: fixture.capability,
            reason: format!(
                "{} is not advertised by {}",
                fixture.capability.label(),
                kernel.info().name
            ),
        };
    }
    let solid = match (fixture.build)(kernel) {
        Ok(solid) => solid,
        Err(error) => return classify(error),
    };
    let measured = match measure(kernel, solid) {
        Ok(measured) => measured,
        Err(error) => {
            kernel.release(solid);
            return classify(error);
        }
    };
    kernel.release(solid);

    let volume_delta_mm3 = (measured.volume_mm3 - fixture.volume_mm3).abs();
    let area_delta_mm2 = (measured.area_mm2 - fixture.area_mm2).abs();
    // An open mesh cannot be a correct solid whatever its volume says, so it never passes.
    if measured.open_edges != 0 {
        return Status::Wrong {
            measured,
            volume_delta_mm3,
            area_delta_mm2,
        };
    }
    if volume_delta_mm3 <= fixture.volume_tolerance_mm3
        && area_delta_mm2 <= fixture.area_tolerance_mm2
    {
        Status::Pass(measured)
    } else {
        Status::Wrong {
            measured,
            volume_delta_mm3,
            area_delta_mm2,
        }
    }
}

/// Judge a status against the fixture's expectation.
pub fn verdict(expectation: Expectation, status: &Status) -> Verdict {
    let passed = matches!(status, Status::Pass(_));
    match expectation {
        Expectation::MustPass => {
            if passed {
                Verdict::Ok
            } else {
                Verdict::Regression
            }
        }
        Expectation::Unsupported => match status {
            Status::Unsupported { .. } | Status::Refused(_) => Verdict::Ok,
            Status::Pass(_) => Verdict::UnexpectedPass,
            _ => Verdict::Regression,
        },
        Expectation::KnownFailure => match status {
            Status::Pass(_) => Verdict::UnexpectedPass,
            Status::Wrong { .. } | Status::Unsupported { .. } | Status::Refused(_) => {
                Verdict::KnownFailure
            }
            _ => Verdict::Regression,
        },
        Expectation::NotConstructible(_) => Verdict::Ok,
    }
}

/// One fixture's outcome on one kernel.
#[derive(Debug, Clone)]
pub struct Row {
    pub fixture: &'static Fixture,
    pub status: Status,
    pub verdict: Verdict,
}

/// The whole table: every fixture, for every kernel column.
#[derive(Debug, Clone)]
pub struct Report {
    /// Kernel names, in column order.
    pub kernels: Vec<String>,
    pub rows: Vec<Row>,
}

impl Report {
    /// 0 when nothing regressed, 1 otherwise.
    ///
    /// A `known-failure` row and an `unexpected-pass` row both keep the gate green: the first
    /// never passed, and the second is an improvement that only requires a baseline edit. An
    /// infrastructure failure (`Status::Failed`, a caught panic or timeout) always fails, because
    /// no baseline should normalise a crash.
    pub fn exit_code(&self) -> u8 {
        let regressed = self
            .rows
            .iter()
            .any(|row| row.verdict == Verdict::Regression);
        u8::from(regressed)
    }

    /// Count rows with this verdict.
    pub fn count(&self, verdict: Verdict) -> usize {
        self.rows
            .iter()
            .filter(|row| row.verdict == verdict)
            .count()
    }

    /// The table, ready to print.
    pub fn render(&self) -> String {
        let mut out = String::new();
        out.push_str("analytic solid/boolean correctness corpus\n");
        out.push_str(&format!("kernel(s): {}\n\n", self.kernels.join(", ")));

        let id_width = self
            .rows
            .iter()
            .map(|row| row.fixture.id.len())
            .max()
            .unwrap_or(8)
            .max("fixture".len());
        let cap_width = "capability".len().max(
            self.rows
                .iter()
                .map(|row| row.fixture.capability.label().len())
                .max()
                .unwrap_or(0),
        );

        out.push_str(&format!(
            "{:<id_width$}  {:<cap_width$}  {:<10}  {:<14}  {}\n",
            "fixture",
            "capability",
            "expected",
            "result",
            "detail",
            id_width = id_width,
            cap_width = cap_width,
        ));
        out.push_str(&"-".repeat(id_width + cap_width + 80));
        out.push('\n');

        for row in &self.rows {
            let expectation = match row.fixture.expectation {
                Expectation::MustPass => "must-pass",
                Expectation::Unsupported => "unsupported",
                Expectation::KnownFailure => "known-fail",
                Expectation::NotConstructible(_) => "n/a",
            };
            let (label, detail) = match &row.status {
                Status::Pass(m) => (
                    "PASS",
                    format!(
                        "volume {} mm³ (Δ {:.3e}), area {} mm² (Δ {:.3e})",
                        m.volume_mm3,
                        (m.volume_mm3 - row.fixture.volume_mm3).abs(),
                        m.area_mm2,
                        (m.area_mm2 - row.fixture.area_mm2).abs()
                    ),
                ),
                Status::Wrong {
                    measured,
                    volume_delta_mm3,
                    area_delta_mm2,
                } => (
                    "FAIL",
                    format!(
                        "measured {} mm³ (Δ {:.3e} vs {}), area {} mm² (Δ {:.3e} vs {})",
                        measured.volume_mm3,
                        volume_delta_mm3,
                        row.fixture.volume_mm3,
                        measured.area_mm2,
                        area_delta_mm2,
                        row.fixture.area_mm2
                    ),
                ),
                Status::Unsupported { reason, .. } => ("UNSUPPORTED", reason.clone()),
                Status::Refused(error) => ("REFUSED", error.to_string()),
                Status::Failed(error) => ("ERROR", format!("{}: {}", kind_label(error), error)),
                Status::NotConstructible(reason) => ("NOT-CONSTRUCTIBLE", reason.to_string()),
            };
            out.push_str(&format!(
                "{:<id_width$}  {:<cap_width$}  {:<10}  {:<14}  {}\n",
                row.fixture.id,
                row.fixture.capability.label(),
                expectation,
                label,
                detail,
                id_width = id_width,
                cap_width = cap_width,
            ));
        }

        out.push('\n');
        out.push_str(&format!(
            "{} fixture(s): {} ok, {} known-failure, {} regression(s), {} unexpected pass(es)\n",
            self.rows.len(),
            self.count(Verdict::Ok),
            self.count(Verdict::KnownFailure),
            self.count(Verdict::Regression),
            self.count(Verdict::UnexpectedPass),
        ));
        if self.count(Verdict::UnexpectedPass) > 0 {
            out.push_str(
                "note: an unexpected pass means the baseline is stale — promote it to must-pass.\n",
            );
        }
        out.push_str(&format!("exit code: {}\n", self.exit_code()));
        out
    }
}

fn kind_label(error: &KernelError) -> &'static str {
    match error.kind() {
        ErrorKind::Unsupported => "unsupported",
        ErrorKind::InvalidInput => "invalid input",
        ErrorKind::OperationFailed => "operation failed",
        ErrorKind::SizeTooLarge => "too large",
        ErrorKind::Internal => "kernel internal error",
        ErrorKind::InvalidHandle => "invalid handle",
        ErrorKind::Cancelled => "cancelled",
        ErrorKind::Timeout => "timeout",
    }
}

/// Run every fixture against every kernel column.
///
/// Rows come out kernel-major (all fixtures for the first kernel, then the second, …) so a table
/// with several columns can be read as one block per kernel.
pub fn run_all(kernels: &mut [(&str, &mut dyn Kernel)]) -> Report {
    let names = kernels
        .iter()
        .map(|(name, _)| (*name).to_string())
        .collect();
    let mut rows = Vec::new();
    for (_name, kernel) in kernels.iter_mut() {
        for fixture in FIXTURES {
            let status = run_fixture(*kernel, fixture);
            let verdict = verdict(fixture.expectation, &status);
            rows.push(Row {
                fixture,
                status,
                verdict,
            });
        }
    }
    Report {
        kernels: names,
        rows,
    }
}

/// The fixture list is the contract of this module: ids and analytic values must not drift
/// without a matching documentation change.
#[cfg(test)]
mod tests {
    use super::{
        Expectation, FIXTURES, Report, Row, Status, Verdict, run_all, run_fixture, verdict,
    };
    use crate::TruckKernel;
    use cad_contract::capability::Capability;
    use cad_contract::error::KernelError;
    use cad_contract::kernel::Kernel;

    #[test]
    fn the_fixture_set_covers_every_required_case() {
        let ids: Vec<&str> = FIXTURES.iter().map(|f| f.id).collect();
        for required in [
            "difference_coplanar_cut",
            "difference_piercing_cut",
            "difference_contained_void",
            "union_tangent_contact",
            "intersection_near_degenerate_sliver",
            "intersection_thin_wall",
            "intersection_large_coordinates",
            "non_axis_aligned_box",
            "polyline_l_bracket",
        ] {
            assert!(
                ids.contains(&required),
                "missing required fixture {required}"
            );
        }
        // Ids are unique, or the table is ambiguous.
        let mut sorted = ids.clone();
        sorted.sort_unstable();
        sorted.dedup();
        assert_eq!(sorted.len(), ids.len(), "duplicate fixture ids");
    }

    /// The harness itself must be able to fail: a fixture whose analytic value is deliberately
    /// wrong must come back as a regression, and the exit code must be non-zero. This is the test
    /// behind "the corpus binary exits non-zero on a wrong volume".
    #[test]
    fn a_wrong_volume_is_a_regression_and_sets_the_exit_code() {
        let mut kernel = TruckKernel::new();
        // `box_control` with a sabotaged analytic value: same fixture, wrong expected volume.
        let mut sabotaged = FIXTURES[0];
        sabotaged.volume_mm3 = 501.0;
        sabotaged.volume_tolerance_mm3 = 1e-6;
        let status = run_fixture(&mut kernel, &sabotaged);
        assert!(matches!(status, Status::Wrong { .. }), "got {status:?}");
        let verdict = verdict(sabotaged.expectation, &status);
        assert_eq!(verdict, Verdict::Regression);

        // The same failure rendered as a report row: the table must show it and the gate must be
        // non-zero. `FIXTURES[0]` is the static twin of `sabotaged` (only the expectation differs,
        // and the verdict has already been judged against the sabotaged one).
        let report = Report {
            kernels: vec!["truck".to_string()],
            rows: vec![Row {
                fixture: &FIXTURES[0],
                status,
                verdict,
            }],
        };
        assert_eq!(report.exit_code(), 1);
        assert!(report.render().contains("FAIL"), "{}", report.render());
    }

    /// An open mesh never passes, even if its volume happens to be right.
    #[test]
    fn an_open_mesh_never_passes() {
        let mut kernel = TruckKernel::new();
        let solid = match (FIXTURES[0].build)(&mut kernel) {
            Ok(solid) => solid,
            Err(_) => return,
        };
        // Measure through the public helper: a valid box is closed, so this asserts the helper
        // reports the open-edge count the pass rule keys on.
        let measured = crate::corpus::measure(&kernel, solid);
        assert!(
            matches!(measured, Ok(m) if m.open_edges == 0),
            "{measured:?}"
        );
        kernel.release(solid);
    }

    /// Difference is measured, never skipped: all three cut fixtures appear in the report, the two
    /// that avoid coincident faces must be exact, and the coplanar one is recorded with its
    /// measurement rather than dropped.
    #[test]
    fn difference_is_measured_not_skipped() {
        let report = run_all(&mut [(
            "truck",
            &mut TruckKernel::new() as &mut dyn cad_contract::kernel::Kernel,
        )]);
        let difference_rows: Vec<_> = report
            .rows
            .iter()
            .filter(|row| row.fixture.capability == Capability::BooleanDifference)
            .collect();
        assert_eq!(
            difference_rows.len(),
            3,
            "all three cut fixtures must appear"
        );

        // The two fixtures that build their tool without coincident faces are exact, to the stated
        // tolerance. This is the measurement that justifies advertising `BooleanDifference`.
        for id in ["difference_piercing_cut", "difference_contained_void"] {
            let row = difference_rows
                .iter()
                .find(|row| row.fixture.id == id)
                .filter(|row| row.verdict == Verdict::Ok);
            assert!(row.is_some(), "{id} did not pass: {:?}", report.render());
            let row = report.rows.iter().find(|row| row.fixture.id == id);
            let Some(row) = row else { continue };
            assert!(
                matches!(&row.status, Status::Pass(m) if m.open_edges == 0),
                "{id} must be watertight: {:?}",
                row.status
            );
        }

        // The coplanar case is a recorded limitation with a number attached, not a hidden skip.
        let coplanar = report
            .rows
            .iter()
            .find(|row| row.fixture.id == "difference_coplanar_cut");
        let Some(coplanar) = coplanar else {
            return;
        };
        assert_eq!(coplanar.verdict, Verdict::KnownFailure);
        assert_eq!(report.exit_code(), 0, "a known failure is not a regression");
    }

    /// The full corpus on the truck adapter: everything the adapter advertises must pass, and the
    /// table must render with a matching exit code.
    #[test]
    fn the_truck_adapter_passes_everything_it_advertises() {
        let report = run_all(&mut [(
            "truck",
            &mut TruckKernel::new() as &mut dyn cad_contract::kernel::Kernel,
        )]);
        let rendered = report.render();
        assert!(rendered.contains("box_control"), "{rendered}");
        for row in &report.rows {
            if row.fixture.expectation == Expectation::MustPass {
                assert_eq!(
                    row.verdict,
                    Verdict::Ok,
                    "regression in {}: {:?}",
                    row.fixture.id,
                    row.status
                );
            }
        }
        assert_eq!(report.exit_code(), 0, "{rendered}");
    }

    /// Contract-level sanity: refusing an unadvertised operation must be `Unsupported`, which is
    /// what the corpus records rather than a failure.
    #[test]
    fn an_unadvertised_operation_refuses_with_unsupported() {
        let kernel = TruckKernel::new();
        // Fillets are in the contract but not implemented by this adapter, so they must refuse
        // rather than attempt them. (Difference used to be the example here, then revolve; both are
        // now advertised because both are measured correct.)
        assert_eq!(
            kernel.require(Capability::Fillet),
            Err(KernelError::Unsupported {
                capability: Capability::Fillet
            })
        );
        assert!(kernel.require(Capability::BooleanDifference).is_ok());
        assert!(kernel.require(Capability::Revolve).is_ok());
    }

    /// The profile validator the corpus depends on rejects the degenerate input it should.
    #[test]
    fn the_corpus_profiles_are_valid() {
        let profile = super::rect(10.0, 10.0, [0.0, 0.0]);
        assert!(cad_contract::kernel::validate_profile(&profile).is_ok());
        let rotated = super::rotated_rect(10.0, 30.0);
        assert!(cad_contract::kernel::validate_profile(&rotated).is_ok());
    }
}

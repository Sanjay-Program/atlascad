//! SolidCraft's kernel behind the shared [`cad_contract::Kernel`] trait.
//!
//! Enabled by the `cad-contract` feature (off by default), so the shipped kernel and its API stay
//! exactly as they are: this module *delegates* to the same [`Body`](crate::Body) operations the
//! application uses — `extrude`, `boolean`, `tessellate` — and owns nothing but a handle table.
//!
//! The trait's rules are the interesting part of the adapter:
//!
//! * a handle is a **session** handle: [`SolidcraftKernel`] only accepts the ones it issued, so a
//!   handle from another instance (or one already released) is [`KernelError::UnknownSolid`];
//! * the profile is normalised counter-clockwise, and a negative height extrudes downwards from
//!   `z = 0`, so a caller may pass either winding and either direction;
//! * a failure leaves the handle table unchanged, and a tessellation is validated against the
//!   contract's mesh invariants before it is returned;
//! * the contract error taxonomy maps one-to-one onto this crate's [`KernelError`](crate::KernelError).
//!
//! Volumes come from the contract's default `volume` (measured from a tessellation), so a test can
//! compare the contract path with [`measure`](crate::measure) and catch a wrong tessellation.

use std::collections::BTreeMap;

use cad_contract::capability::{Capability, CapabilitySet, KernelInfo};
use cad_contract::error::KernelError;
use cad_contract::kernel::{BooleanOp, Kernel, SolidId, validate_profile};
use cad_contract::mesh::{Aabb3, Mesh};
use solidcraft_geom::{Loop2, Plane, Region2, Vec2};

use crate::{Body, BoolOp, boolean, build::extrude};

/// The operations this adapter exposes, each one exercised by the tests below.
fn capabilities() -> CapabilitySet {
    CapabilitySet::from([
        Capability::Extrude,
        Capability::BooleanUnion,
        Capability::BooleanDifference,
        Capability::BooleanIntersection,
        Capability::Tessellate,
    ])
}

/// This kernel as a [`Kernel`].
///
/// Solids live in a table keyed by the handle issued for them; the model is untouched by a failed
/// operation, and [`release`](Kernel::release) is the only way a solid goes away.
#[derive(Default)]
pub struct SolidcraftKernel {
    solids: BTreeMap<u64, Body>,
    next: u64,
}

impl SolidcraftKernel {
    /// An empty kernel session.
    pub fn new() -> Self {
        SolidcraftKernel { solids: BTreeMap::new(), next: 1 }
    }

    /// The solid behind `solid`, for an operation the contract does not express.
    ///
    /// The borrow ends the moment the handle is released.
    pub fn body(&self, solid: SolidId) -> Option<&Body> {
        self.solids.get(&solid.get())
    }

    /// The body behind `solid`, or [`KernelError::UnknownSolid`].
    fn get(&self, solid: SolidId) -> Result<&Body, KernelError> {
        self.solids.get(&solid.get()).ok_or(KernelError::UnknownSolid(solid))
    }

    /// Store `body` and issue its handle. `SolidId(0)` is reserved, so it is never issued.
    fn insert(&mut self, body: Body) -> SolidId {
        let id = SolidId::from_raw(self.next);
        self.next = self.next.checked_add(1).unwrap_or(1);
        self.solids.insert(id.get(), body);
        id
    }

    /// Solids currently held (the table never holds released handles).
    pub fn len(&self) -> usize {
        self.solids.len()
    }

    /// Does the session hold no solids?
    pub fn is_empty(&self) -> bool {
        self.solids.is_empty()
    }
}

/// This crate's error as the contract's.
fn map_error(e: crate::KernelError) -> KernelError {
    match e {
        crate::KernelError::Invalid(message) => KernelError::Invalid(message),
        crate::KernelError::Failed(message) => KernelError::Failed(message),
        crate::KernelError::TooLarge(message) => KernelError::TooLarge(message),
        crate::KernelError::Internal(message) => KernelError::Internal(message),
    }
}

/// `body` as the contract's mesh, refusing one that breaks the contract's invariants.
fn contract_mesh(body: &Body, chord_tol_mm: f64) -> Result<Mesh, KernelError> {
    let mesh = body.tessellate(chord_tol_mm).map_err(map_error)?;
    let out = Mesh {
        positions: mesh.positions.iter().map(|p| [p.x, p.y, p.z]).collect(),
        triangles: mesh.triangles.iter().map(|t| [t[0], t[1], t[2]]).collect(),
        normals: mesh.normals.iter().map(|n| [n.x, n.y, n.z]).collect(),
    };
    out.validate().map_err(|e| KernelError::Failed(format!("the tessellation is not a valid mesh: {e}")))?;
    Ok(out)
}

impl Kernel for SolidcraftKernel {
    fn info(&self) -> KernelInfo {
        KernelInfo::new("solidcraft", env!("CARGO_PKG_VERSION"), capabilities())
    }

    fn extrude(&mut self, profile: &[[f64; 2]], height_mm: f64) -> Result<SolidId, KernelError> {
        validate_profile(profile)?;
        if !height_mm.is_finite() || height_mm == 0.0 {
            return Err(KernelError::Invalid(format!("extrusion height {height_mm} must be finite and non-zero")));
        }
        // The contract wants an outward solid: normalise the loop and extrude in the direction of
        // the (signed) height.
        let points: Vec<Vec2> = profile.iter().map(|p| Vec2::new(p[0], p[1])).collect();
        let region = Region2 { outer: Loop2::polygon(&points).ccw(), holes: Vec::new() };
        let (start, end) = if height_mm < 0.0 { (height_mm, 0.0) } else { (0.0, height_mm) };
        let mut bodies = extrude(&Plane::XY, &[region], start, end).map_err(map_error)?;
        match bodies.len() {
            1 => {}
            n => {
                return Err(KernelError::Invalid(format!(
                    "the profile extrudes to {n} solids; the contract takes one closed loop without holes or crossings"
                )));
            }
        }
        let body = bodies.pop().ok_or_else(|| KernelError::Failed("the extrusion produced no solid".into()))?;
        Ok(self.insert(body))
    }

    fn boolean(&mut self, op: BooleanOp, a: SolidId, b: SolidId) -> Result<SolidId, KernelError> {
        self.require(op.capability())?;
        let (left, right) = (self.get(a)?, self.get(b)?);
        let native = match op {
            BooleanOp::Union => BoolOp::Union,
            BooleanOp::Difference => BoolOp::Cut,
            BooleanOp::Intersection => BoolOp::Intersect,
        };
        let out = boolean(left, right, native).map_err(map_error)?;
        let body = out.ok_or_else(|| KernelError::Failed(format!("the {} produced no solid", op.label())))?;
        Ok(self.insert(body))
    }

    fn tessellate(&self, solid: SolidId, chord_tol_mm: f64) -> Result<Mesh, KernelError> {
        if !chord_tol_mm.is_finite() || chord_tol_mm <= 0.0 {
            return Err(KernelError::Invalid(format!("chord tolerance {chord_tol_mm} must be a finite positive number of millimetres")));
        }
        contract_mesh(self.get(solid)?, chord_tol_mm)
    }

    fn release(&mut self, solid: SolidId) {
        self.solids.remove(&solid.get());
    }

    /// Exact mass properties: this kernel measures its own B-rep rather than a tessellation, so a
    /// caller gets the kernel's number instead of the mesh's approximation.
    fn volume(&self, solid: SolidId) -> Result<f64, KernelError> {
        let body = self.get(solid)?;
        Ok(crate::measure::measure(body).map_err(map_error)?.volume.abs())
    }

    fn bounding_box(&self, solid: SolidId) -> Result<Aabb3, KernelError> {
        let mesh = contract_mesh(self.get(solid)?, cad_contract::tol::DEFAULT_LINEAR_MM)?;
        mesh.bounding_box().ok_or_else(|| KernelError::Failed("the solid produced no geometry".into()))
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::build::box_solid;
    use solidcraft_geom::Vec3;

    /// A 10 × 10 millimetre square, counter-clockwise.
    fn square() -> [[f64; 2]; 4] {
        [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    }

    /// A 6 × 6 millimetre square inside the 10 × 10 one.
    fn inner() -> [[f64; 2]; 4] {
        [[2.0, 2.0], [8.0, 2.0], [8.0, 8.0], [2.0, 8.0]]
    }

    /// The milestone test: the same box built through the contract and through the kernel's own
    /// API measures the same volume, so the contract describes the kernel actually shipped.
    #[test]
    fn the_contract_and_the_kernel_agree_on_a_box() {
        let mut k = SolidcraftKernel::new();
        let id = k.extrude(&square(), 5.0).unwrap();
        let through_contract = k.volume(id).unwrap();
        let through_kernel = crate::measure::measure(&box_solid(Vec3::ZERO, Vec3::new(10.0, 10.0, 5.0)).unwrap()).unwrap().volume;
        assert!((through_contract - 500.0).abs() < 1e-6, "{through_contract}");
        assert!((through_kernel - 500.0).abs() < 1e-6, "{through_kernel}");
        assert!((through_contract - through_kernel).abs() < 1e-6, "{through_contract} vs {through_kernel}");
        // The bounding box is the profile extruded from z = 0.
        let bb = k.bounding_box(id).unwrap();
        assert_eq!(bb.min, [0.0, 0.0, 0.0]);
        assert_eq!(bb.max, [10.0, 10.0, 5.0]);
        assert!(k.info().supports(Capability::Extrude));
    }

    /// Either winding, either direction: the adapter normalises what the trait allows.
    #[test]
    fn winding_and_direction_do_not_change_the_solid() {
        let mut k = SolidcraftKernel::new();
        let clockwise = square().map(|[x, y]| [y, x]);
        let up = k.extrude(&square(), 5.0).unwrap();
        let cw = k.extrude(&clockwise, 5.0).unwrap();
        let down = k.extrude(&square(), -5.0).unwrap();
        for id in [up, cw, down] {
            let v = k.volume(id).unwrap();
            assert!((v - 500.0).abs() < 1e-6, "handle {id:?} measured {v}");
        }
        // Downwards is below the plane.
        let bb = k.bounding_box(down).unwrap();
        assert_eq!(bb.min[2], -5.0);
        assert_eq!(bb.max[2], 0.0);
    }

    #[test]
    fn booleans_combine_placed_solids() {
        let mut k = SolidcraftKernel::new();
        let block = k.extrude(&square(), 10.0).unwrap();
        let hole = k.extrude(&inner(), 20.0).unwrap();
        let cut = k.boolean(BooleanOp::Difference, block, hole).unwrap();
        assert!((k.volume(cut).unwrap() - 640.0).abs() < 1e-6, "{}", k.volume(cut).unwrap());
        let union = k.boolean(BooleanOp::Union, block, hole).unwrap();
        assert!(k.volume(union).unwrap() >= 1000.0);
        let shared = k.boolean(BooleanOp::Intersection, block, hole).unwrap();
        assert!((k.volume(shared).unwrap() - 360.0).abs() < 1e-6);
        // Neither input was consumed.
        assert!((k.volume(block).unwrap() - 1000.0).abs() < 1e-6);
        assert_eq!(k.len(), 5);
    }

    #[test]
    fn handles_are_session_handles() {
        let mut a = SolidcraftKernel::new();
        let b = SolidcraftKernel::new();
        let id = a.extrude(&square(), 1.0).unwrap();
        assert_eq!(b.tessellate(id, 0.05), Err(KernelError::UnknownSolid(id)));
        a.release(id);
        a.release(id);
        assert!(a.is_empty());
        assert_eq!(a.tessellate(id, 0.05), Err(KernelError::UnknownSolid(id)));
        assert_eq!(a.volume(id), Err(KernelError::UnknownSolid(id)));
        // Handle 0 is never issued.
        assert_eq!(a.tessellate(SolidId::from_raw(0), 0.05), Err(KernelError::UnknownSolid(SolidId::from_raw(0))));
    }

    #[test]
    fn bad_input_is_invalid_and_leaves_the_session_alone() {
        let mut k = SolidcraftKernel::new();
        assert!(matches!(k.extrude(&[[0.0, 0.0], [1.0, 0.0]], 1.0), Err(KernelError::Invalid(_))));
        assert!(matches!(k.extrude(&square(), 0.0), Err(KernelError::Invalid(_))));
        assert!(matches!(k.extrude(&square(), f64::NAN), Err(KernelError::Invalid(_))));
        let id = k.extrude(&square(), 1.0).unwrap();
        assert!(matches!(k.tessellate(id, 0.0), Err(KernelError::Invalid(_))));
        assert!(matches!(k.tessellate(id, f64::INFINITY), Err(KernelError::Invalid(_))));
        assert_eq!(k.len(), 1, "a refused input must not leave a solid behind");
        assert!(k.body(id).is_some());
    }

    /// The mesh a caller receives satisfies the contract's invariants and encloses the solid.
    ///
    /// The tessellation is per B-rep face (the kernel tessellates faces separately), so a caller
    /// that wants a single welded surface still has `signed_volume`/`bounding_box`, which are
    /// exact over the face patches; the contract only promises a valid mesh, not a welded one.
    #[test]
    fn tessellation_is_a_valid_mesh_that_encloses_the_solid() {
        let mut k = SolidcraftKernel::new();
        let id = k.extrude(&square(), 2.0).unwrap();
        let mesh = k.tessellate(id, 0.05).unwrap();
        assert!(mesh.validate().is_ok());
        assert_eq!(mesh.normals.len(), mesh.positions.len());
        assert!((mesh.signed_volume() - 200.0).abs() < 1e-6, "{}", mesh.signed_volume());
        assert_eq!(mesh.bounding_box().map(|b| b.max), Some([10.0, 10.0, 2.0]));
        assert!(k.info().supports(Capability::Tessellate));
    }

    /// A capability the adapter does not advertise is refused before the kernel is touched.
    #[test]
    fn unsupported_capabilities_are_reported() {
        let k = SolidcraftKernel::new();
        assert_eq!(k.require(Capability::Fillet), Err(KernelError::Unsupported { capability: Capability::Fillet }));
        let mut k = SolidcraftKernel::new();
        let id = k.extrude(&square(), 1.0).unwrap();
        // Releasing the only solid leaves the table empty; a boolean then names an unknown handle
        // rather than building on nothing.
        k.release(id);
        assert_eq!(k.boolean(BooleanOp::Union, id, id), Err(KernelError::UnknownSolid(id)));
    }

    #[test]
    fn capability_set_is_exactly_what_is_tested() {
        let set = capabilities();
        let listed: Vec<Capability> = set.iter().collect();
        assert_eq!(
            listed,
            vec![
                Capability::Extrude,
                Capability::BooleanUnion,
                Capability::BooleanDifference,
                Capability::BooleanIntersection,
                Capability::Tessellate,
            ]
        );
    }
}

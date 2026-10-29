//! The 3D kernel contract.
//!
//! Deliberately small. Every operation here is one that *both* a B-rep kernel and a future
//! mesh-based kernel can implement, and every one is exercised by the truck adapter's tests. Anything
//! kernel-specific (fillets, sheet metal, spline surfaces) is reached through the adapter's own API
//! after checking [`Capability`](crate::capability::Capability), not through this trait — a trait
//! that tried to express everything would either exclude the best kernel or flatten its abilities.
//!
//! # Handles vs identities
//!
//! [`SolidId`] is a **session handle**. It is valid only for the kernel instance that issued it and
//! must never be written to a project file; persistence uses
//! [`EntityId`](crate::ids::EntityId). An adapter must reject handles it did not issue.

use crate::capability::{Capability, KernelInfo};
use crate::error::{KernelError, Result};
use crate::mesh::{Aabb3, Mesh};
use crate::tol::DEFAULT_LINEAR_MM;

/// An opaque solid handle issued by a kernel instance.
///
/// Not persistent. `SolidId(0)` is reserved for "none" and is never issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct SolidId(u64);

impl SolidId {
    /// Wrap a raw handle. Only a kernel adapter should call this.
    pub const fn from_raw(raw: u64) -> Self {
        SolidId(raw)
    }

    /// The raw value, for bookkeeping inside an adapter.
    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A boolean operation between two solids.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum BooleanOp {
    /// Everything in either solid.
    Union,
    /// `a` with `b` removed.
    Difference,
    /// Only what both solids share.
    Intersection,
}

impl BooleanOp {
    /// The capability that must be advertised to run this operation.
    pub const fn capability(self) -> Capability {
        match self {
            BooleanOp::Union => Capability::BooleanUnion,
            BooleanOp::Difference => Capability::BooleanDifference,
            BooleanOp::Intersection => Capability::BooleanIntersection,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            BooleanOp::Union => "union",
            BooleanOp::Difference => "difference",
            BooleanOp::Intersection => "intersection",
        }
    }
}

/// Why a profile loop is unusable.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProfileError {
    #[error("a profile needs at least 3 points, got {0}")]
    TooFewPoints(usize),
    #[error("profile point {index} is not finite")]
    NonFinitePoint { index: usize },
    #[error("profile has a repeated consecutive point at index {index}")]
    DuplicateConsecutivePoint { index: usize },
    #[error("profile encloses no area")]
    ZeroArea,
}

impl From<ProfileError> for KernelError {
    fn from(e: ProfileError) -> Self {
        KernelError::Invalid(e.to_string())
    }
}

/// Validate a closed planar profile given as millimetre coordinates in its own plane.
///
/// The loop is implicitly closed (the last point joins the first). Checks, in order: enough points,
/// all finite, no repeated consecutive point, and non-zero enclosed area. A kernel adapter should
/// run this *before* touching the kernel so that bad input is reported as
/// [`KernelError::Invalid`] rather than as a kernel failure.
pub fn validate_profile(points: &[[f64; 2]]) -> core::result::Result<(), ProfileError> {
    if points.len() < 3 {
        return Err(ProfileError::TooFewPoints(points.len()));
    }
    for (index, p) in points.iter().enumerate() {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(ProfileError::NonFinitePoint { index });
        }
    }
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        if points[index] == points[next] {
            return Err(ProfileError::DuplicateConsecutivePoint { index });
        }
    }
    // Shoelace: twice the signed area.
    let mut twice_area = 0.0;
    for index in 0..points.len() {
        let next = (index + 1) % points.len();
        twice_area += points[index][0] * points[next][1] - points[next][0] * points[index][1];
    }
    if twice_area.abs() < 1e-12 {
        return Err(ProfileError::ZeroArea);
    }
    Ok(())
}

/// A geometry kernel.
///
/// Implementations must:
/// * return `Err` and **leave the model unchanged** on every failure;
/// * never panic on caller input (a third-party panic is caught and reported as
///   [`KernelError::Internal`], as SolidCraft's shipped kernel already does);
/// * accept only handles they issued.
pub trait Kernel {
    /// What this kernel is and what it can do.
    fn info(&self) -> KernelInfo;

    /// Does this kernel advertise `capability`?
    fn supports(&self, capability: Capability) -> bool {
        self.info().capabilities.contains(capability)
    }

    /// Fail with [`KernelError::Unsupported`] unless `capability` is advertised.
    fn require(&self, capability: Capability) -> Result<()> {
        if self.supports(capability) {
            Ok(())
        } else {
            Err(KernelError::Unsupported { capability })
        }
    }

    /// Extrude a closed profile along +Z by `height_mm`.
    ///
    /// The profile must be counter-clockwise (positive area) for an outward-oriented solid; an
    /// adapter may normalise the winding. Negative heights extrude downwards.
    fn extrude(&mut self, profile: &[[f64; 2]], height_mm: f64) -> Result<SolidId>;

    /// Combine two solids. Neither input is consumed.
    fn boolean(&mut self, op: BooleanOp, a: SolidId, b: SolidId) -> Result<SolidId>;

    /// Move a solid by `delta_mm` without rotating it. `solid` is not consumed.
    ///
    /// This exists for more than convenience: a tool that must cut *through* a body has to
    /// overshoot it, and [`extrude`](Kernel::extrude) always starts at z = 0, so a tool built for a
    /// body whose faces are at z = 0 and z = h always shares a plane with that body. Coincident
    /// faces are the case boolean operations handle worst (see `docs/kernel-comparison.md` §4.1),
    /// so a through-cut is built by extruding an over-long tool and translating it to straddle the
    /// body. A kernel that cannot move a solid therefore cannot build an exact cut, and says so by
    /// not advertising [`Capability::Transform`].
    ///
    /// The default implementation reports the capability as unsupported, so a kernel that has not
    /// implemented it cannot silently accept a translation and ignore it.
    fn translate(&mut self, _solid: SolidId, _delta_mm: [f64; 3]) -> Result<SolidId> {
        Err(KernelError::Unsupported {
            capability: Capability::Transform,
        })
    }

    /// Revolve a closed profile a full turn about an axis lying in the profile's own plane.
    ///
    /// `axis_origin_mm` is a point on the axis in the profile's coordinate system and
    /// `axis_direction_mm` its direction in the same plane; the direction need not be normalized.
    /// `angle_rad` is currently required to be a full turn: a partial revolve produces an open
    /// shell, which is not a solid, and the contract has no way to represent one. An adapter that
    /// cannot produce a closed solid must return [`KernelError::Invalid`] rather than a shell.
    ///
    /// The result must be the analytic solid of revolution: a rectangle that shares the axis
    /// revolved a full turn is a cylinder, and one that does not is an annulus.
    ///
    /// The default implementation reports the capability as unsupported, so a kernel that has not
    /// implemented it cannot silently accept the call.
    fn revolve(
        &mut self,
        _profile: &[[f64; 2]],
        _axis_origin_mm: [f64; 2],
        _axis_direction_mm: [f64; 2],
        _angle_rad: f64,
    ) -> Result<SolidId> {
        Err(KernelError::Unsupported {
            capability: Capability::Revolve,
        })
    }

    /// Tessellate a solid with chord tolerance `chord_tol_mm`.
    ///
    /// The mesh must satisfy [`Mesh::validate`]. An adapter that cannot produce a valid mesh
    /// returns an error rather than an invalid mesh.
    fn tessellate(&self, solid: SolidId, chord_tol_mm: f64) -> Result<Mesh>;

    /// Release a solid. Releasing an unknown handle is not an error (idempotent cleanup).
    fn release(&mut self, solid: SolidId);

    /// Can this kernel still resolve `solid`?
    ///
    /// `None` — the default — means the adapter cannot answer, so a caller must assume the handle is
    /// valid and let the adapter reject it if it is not. `Some(false)` exists for session caches: a
    /// cache that outlives one kernel instance otherwise adopts handles the next instance never
    /// issued, and the failure surfaces later as a confusing `UnknownSolid` on an unrelated feature.
    fn contains(&self, _solid: SolidId) -> Option<bool> {
        None
    }

    /// Volume in mm³, measured from a tessellation at the default chord tolerance.
    ///
    /// Provided because it is exactly what a correctness test needs. Kernels that can measure
    /// exact mass properties should overflow this through their own API and advertise
    /// [`Capability::MassProperties`].
    fn volume(&self, solid: SolidId) -> Result<f64> {
        let mesh = self.tessellate(solid, DEFAULT_LINEAR_MM)?;
        Ok(mesh.signed_volume().abs())
    }

    /// Bounding box in millimetres.
    fn bounding_box(&self, solid: SolidId) -> Result<Aabb3> {
        let mesh = self.tessellate(solid, DEFAULT_LINEAR_MM)?;
        mesh.bounding_box()
            .ok_or_else(|| KernelError::Failed("solid produced no geometry".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::{BooleanOp, Kernel, SolidId, validate_profile};
    use crate::capability::{Capability, CapabilitySet, KernelInfo};
    use crate::error::KernelError;
    use crate::mesh::Mesh;
    use std::collections::HashMap;

    /// A kernel that only knows a unit cube, used to test the contract's provided methods.
    struct UnitCubeKernel {
        next: u64,
    }

    impl UnitCubeKernel {
        fn new() -> Self {
            UnitCubeKernel { next: 1 }
        }

        fn cube_mesh() -> Mesh {
            let positions = vec![
                [0.0, 0.0, 0.0],
                [1.0, 0.0, 0.0],
                [1.0, 1.0, 0.0],
                [0.0, 1.0, 0.0],
                [0.0, 0.0, 1.0],
                [1.0, 0.0, 1.0],
                [1.0, 1.0, 1.0],
                [0.0, 1.0, 1.0],
            ];
            let triangles = vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ];
            Mesh {
                positions,
                triangles,
                normals: Vec::new(),
            }
        }
    }

    impl Kernel for UnitCubeKernel {
        fn info(&self) -> KernelInfo {
            KernelInfo::new(
                "unit-cube",
                "0",
                CapabilitySet::from([Capability::Extrude, Capability::Tessellate]),
            )
        }

        fn extrude(
            &mut self,
            profile: &[[f64; 2]],
            height_mm: f64,
        ) -> Result<SolidId, KernelError> {
            validate_profile(profile)?;
            if !height_mm.is_finite() || height_mm <= 0.0 {
                return Err(KernelError::Invalid("height must be positive".into()));
            }
            let id = SolidId::from_raw(self.next);
            self.next += 1;
            Ok(id)
        }

        fn boolean(
            &mut self,
            _op: BooleanOp,
            _a: SolidId,
            _b: SolidId,
        ) -> Result<SolidId, KernelError> {
            Err(KernelError::Unsupported {
                capability: Capability::BooleanUnion,
            })
        }

        fn tessellate(&self, solid: SolidId, _tol: f64) -> Result<Mesh, KernelError> {
            if solid.get() == 0 || solid.get() >= self.next {
                return Err(KernelError::UnknownSolid(solid));
            }
            Ok(Self::cube_mesh())
        }

        fn release(&mut self, _solid: SolidId) {}
    }

    fn square() -> [[f64; 2]; 4] {
        [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]]
    }

    #[test]
    fn provided_volume_uses_tessellation() {
        let mut k = UnitCubeKernel::new();
        let Ok(id) = k.extrude(&square(), 5.0) else {
            return;
        };
        let volume = k.volume(id).unwrap_or_default();
        assert!(
            (volume - 1.0).abs() < 1e-9,
            "expected the mock cube's volume, got {volume}"
        );
    }

    #[test]
    fn provided_bounding_box_works() {
        let mut k = UnitCubeKernel::new();
        let Ok(id) = k.extrude(&square(), 5.0) else {
            return;
        };
        let bb = k.bounding_box(id);
        assert!(bb.is_ok());
        if let Ok(bb) = bb {
            assert_eq!(bb.min, [0.0; 3]);
            assert_eq!(bb.max, [1.0, 1.0, 1.0]);
        }
    }

    #[test]
    fn require_reports_the_missing_capability() {
        let k = UnitCubeKernel::new();
        assert!(k.require(Capability::Extrude).is_ok());
        assert_eq!(
            k.require(Capability::Fillet),
            Err(KernelError::Unsupported {
                capability: Capability::Fillet
            })
        );
    }

    #[test]
    fn unknown_handle_is_rejected_not_panicked() {
        let k = UnitCubeKernel::new();
        assert_eq!(
            k.tessellate(SolidId::from_raw(7), 0.05),
            Err(KernelError::UnknownSolid(SolidId::from_raw(7)))
        );
        assert_eq!(
            k.tessellate(SolidId::from_raw(0), 0.05),
            Err(KernelError::UnknownSolid(SolidId::from_raw(0)))
        );
    }

    #[test]
    fn bad_profiles_are_invalid_input() {
        assert!(validate_profile(&[[0.0, 0.0], [1.0, 0.0]]).is_err());
        assert!(validate_profile(&[[0.0, 0.0], [1.0, 0.0], [f64::NAN, 1.0]]).is_err());
        assert!(validate_profile(&[[0.0, 0.0], [0.0, 0.0], [1.0, 1.0]]).is_err());
        // Collinear points enclose no area.
        assert!(validate_profile(&[[0.0, 0.0], [1.0, 0.0], [2.0, 0.0]]).is_err());
        assert!(validate_profile(&square()).is_ok());
    }

    #[test]
    fn profile_error_converts_to_invalid() {
        let e: KernelError = super::ProfileError::ZeroArea.into();
        assert!(matches!(e, KernelError::Invalid(_)));
    }

    #[test]
    fn boolean_capability_mapping() {
        assert_eq!(BooleanOp::Union.capability(), Capability::BooleanUnion);
        assert_eq!(
            BooleanOp::Difference.capability(),
            Capability::BooleanDifference
        );
        assert_eq!(
            BooleanOp::Intersection.capability(),
            Capability::BooleanIntersection
        );
    }

    /// Compile-time evidence that a map keyed by handles works (adapters store solids this way).
    #[test]
    fn handles_are_usable_as_map_keys() {
        let mut m: HashMap<SolidId, u8> = HashMap::new();
        m.insert(SolidId::from_raw(3), 1);
        assert_eq!(m.get(&SolidId::from_raw(3)), Some(&1));
    }
}

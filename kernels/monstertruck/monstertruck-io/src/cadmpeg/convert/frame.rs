//! Placements: turning the intermediate representation's axis-and-reference
//! triples into the matrices monstertruck's analytic geometry is posed by.
//!
//! Every analytic carrier in `cadmpeg-ir` is described the STEP way -- an origin,
//! an axis, and a zero-azimuth reference direction. monstertruck poses the same
//! shapes with a [`Matrix4`] on a [`Processor`], or with three points on a
//! [`Plane`]. This module is the only place that bridges the two, so the
//! handedness argument below is made once.
//!
//! [`Processor`]: monstertruck_geometry::prelude::Processor
//! [`Plane`]: monstertruck_modeling::Plane

use cadmpeg_ir::math::{Point3 as IrPoint3, Vector3 as IrVector3};
use cadmpeg_ir::transform::Transform as IrTransform;
use monstertruck_modeling::{InnerSpace, Matrix4, Point3, Vector3};

use super::Context;
use crate::Result;

/// Coordinates as monstertruck spells them.
///
/// cadmpeg normalises lengths to millimetres on decode, so there is no unit
/// factor to apply here -- see the module note on [`crate::cadmpeg`].
pub(super) fn point(value: &IrPoint3) -> Point3 { Point3::new(value.x, value.y, value.z) }

/// A direction as monstertruck spells it, not yet normalised.
pub(super) fn vector(value: &IrVector3) -> Vector3 { Vector3::new(value.x, value.y, value.z) }

/// A row-major intermediate-representation transform as a column-major
/// [`Matrix4`].
///
/// `Transform::rows[i][j]` is row `i`, column `j`; [`Matrix4::from_cols`] wants
/// columns. Getting this backwards transposes every placement, which for a rigid
/// motion inverts the rotation and leaves the translation in the wrong row -- so
/// it is not the kind of mistake that shows up as a small error.
pub(super) fn matrix(value: &IrTransform) -> Matrix4 {
    let rows = &value.rows;
    let column = |index: usize| {
        monstertruck_modeling::Vector4::new(
            rows[0][index],
            rows[1][index],
            rows[2][index],
            rows[3][index],
        )
    };
    Matrix4::from_cols(column(0), column(1), column(2), column(3))
}

/// A right-handed orthonormal placement: where a carrier sits, which way its
/// axis points, and where its parameter zero is.
#[derive(Debug, Clone, Copy)]
pub(super) struct Frame {
    /// The carrier's origin or centre.
    pub(super) origin: Point3,
    /// Unit axis.
    pub(super) axis: Vector3,
    /// Unit zero-azimuth direction, perpendicular to [`Frame::axis`].
    pub(super) reference: Vector3,
}

impl Frame {
    /// The third axis, completing a right-handed triple.
    pub(super) fn co_reference(&self) -> Vector3 { self.axis.cross(self.reference) }

    /// The matrix that maps the canonical frame -- x to `reference`, y to
    /// `axis x reference`, z to `axis`, origin to `origin` -- into place, with
    /// `scale` applied to the two radial columns.
    ///
    /// Scaling the radial columns is what turns a UNIT circle into a circle of a
    /// given radius without touching the entity, which is how monstertruck's
    /// exact conics are built: the radius lives in the placement, and the entity
    /// stays canonical and exactly representable.
    pub(super) fn placement(&self, scale: f64) -> Matrix4 {
        let radial = self.reference * scale;
        Matrix4::from_cols(
            radial.extend(0.0),
            (self.axis.cross(radial)).extend(0.0),
            self.axis.extend(0.0),
            self.origin.to_homogeneous(),
        )
    }
}

/// Build a frame, refusing rather than guessing when the source directions do
/// not define one.
///
/// Two things are checked, and both are real in exported files:
///
/// * a zero-length axis or reference, which no amount of normalising fixes;
/// * a reference direction PARALLEL to the axis, which leaves the azimuth
///   undetermined.
///
/// The reference direction is projected perpendicular to the axis rather than
/// used as given. Exporters are entitled to emit a reference that is only
/// approximately perpendicular, and Gram-Schmidt against the axis is what STEP's
/// own `axis2_placement_3d` semantics prescribe.
pub(super) fn frame(
    origin: &IrPoint3,
    axis: &IrVector3,
    reference: &IrVector3,
    what: &str,
    context: &Context<'_>,
) -> Result<Frame> {
    let axis = vector(axis);
    let reference = vector(reference);
    if !axis.magnitude2().is_finite() || axis.magnitude2() <= 0.0 {
        return Err(context.malformed(format!("{what} has a zero-length or non-finite axis")));
    }
    let axis = axis.normalize();
    let radial = reference - axis * axis.dot(reference);
    if !radial.magnitude2().is_finite() || radial.magnitude2() <= 0.0 {
        return Err(context.malformed(format!(
            "{what} has a reference direction parallel to its axis, so its zero azimuth is \
             undetermined"
        )));
    }
    Ok(Frame {
        origin: point(origin),
        axis,
        reference: radial.normalize(),
    })
}

/// A frame for a carrier described by a normal and an in-plane direction, which
/// is how the intermediate representation spells a plane.
pub(super) fn plane_frame(
    origin: &IrPoint3,
    normal: &IrVector3,
    u_axis: &IrVector3,
    context: &Context<'_>,
) -> Result<Frame> {
    frame(origin, normal, u_axis, "plane", context)
}

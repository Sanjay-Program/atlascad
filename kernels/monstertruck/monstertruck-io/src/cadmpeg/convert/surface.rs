//! Surface carriers: intermediate representation to [`Surface`].
//!
//! # Analytic carriers stay analytic
//!
//! Which variant an analytic surface lands on is a decision with consequences,
//! not a formality. monstertruck keeps spheres and tori as their analytic types
//! rather than as their (exact) rational nets so that the CLOSED-FORM parameter
//! division and the analytic `search_parameter` survive the import -- spec 012
//! U1.2. Routing them onto a net instead is exact in the forward direction and
//! wrong in the inverse one, which is the direction that places face trims.
//!
//! The routing here is the one
//! `monstertruck-io/src/step/load/step_geometry/geom_impls/to_modeling.rs` makes
//! for STEP, restated for a different input vocabulary. It is NOT a second
//! opinion about which surfaces are exactly representable.
//!
//! # How the degenerate cases are refused
//!
//! The STEP path guards its analytic arms with predicates that restate the
//! builder's own admissibility rules -- a spindle torus must not reach the
//! analytic variant, because on one `search_parameter` is wrong over ~29% of the
//! domain. Restating a predicate is exactly the thing that drifts, and this
//! module cannot even share the STEP one: `iges` does not imply the `step`
//! feature.
//!
//! So this module does not restate it. It builds the analytic carrier, then ASKS
//! THE BUILDER by probing [`TryIntoHomogeneousBsplineSurface`]. A carrier the
//! builder refuses to net is a carrier whose analytic form is not admissible, and
//! the probe is the predicate rather than a copy of it. One throwaway net per
//! analytic surface is the price, and it cannot drift.

use cadmpeg_ir::geometry::{NurbsSurface as IrNurbsSurface, SurfaceGeometry};
use monstertruck_geometry::prelude::{
    BsplineSurface, KnotVector, NurbsSurface, Processor, RevolutionSurface, Sphere, Torus,
    TryIntoHomogeneousBsplineSurface,
};
use monstertruck_modeling::{
    Curve, EuclideanSpace, Line, Plane, Point3, Surface, Transformed, Vector4,
};

use super::{Context, frame};
use crate::Result;

/// Convert one surface carrier.
pub(super) fn convert(geometry: &SurfaceGeometry, context: &Context<'_>) -> Result<Surface> {
    match geometry {
        SurfaceGeometry::Plane {
            origin,
            normal,
            u_axis,
        } => {
            let frame = frame::plane_frame(origin, normal, u_axis, context)?;
            // Three points, not a normal: `Plane` stores its own u and v axes, and
            // taking v as `axis x u` keeps the plane's normal equal to the source
            // normal. Swapping the two would flip every face on it.
            Ok(Surface::Plane(Plane::new(
                frame.origin,
                frame.origin + frame.reference,
                frame.origin + frame.co_reference(),
            )))
        }

        SurfaceGeometry::Cylinder {
            origin,
            axis,
            ref_direction,
            radius,
        } => {
            let frame = frame::frame(origin, axis, ref_direction, "cylinder", context)?;
            let radius = finite_radius(*radius, "cylinder", context)?;
            // A cylinder is a line at distance `radius` from the axis, revolved.
            // The profile is UNIT length: the surface is unbounded along its axis
            // and the profile's own range is incidental, which is exactly what
            // STEP's `CYLINDRICAL_SURFACE` also produces. Consumers that need the
            // real extent pass the face's trim rectangle to
            // `try_into_homogeneous_bspline_surface_over`.
            let base = frame.origin + frame.reference * radius;
            revolution(Line(base, base + frame.axis), frame)
        }

        SurfaceGeometry::Cone {
            origin,
            axis,
            ref_direction,
            radius,
            ratio,
            half_angle,
        } => {
            // An ELLIPTICAL cone is not a surface of revolution, so it has no
            // `RevolutionSurface` form and no other exact one either. Refused by
            // name rather than silently converted into the circular cone that
            // `ratio == 1` would have meant.
            if (ratio - 1.0).abs() > f64::EPSILON.sqrt() {
                return Err(crate::Error::UnsupportedSurfaceKind {
                    format: context.format,
                    kind: "elliptical cone",
                });
            }
            let frame = frame::frame(origin, axis, ref_direction, "cone", context)?;
            let radius = finite_radius(*radius, "cone", context)?;
            let taper = half_angle.tan();
            if !taper.is_finite() {
                return Err(context.malformed(format!(
                    "cone half-angle {half_angle} radians has no finite taper, so its profile is \
                     parallel to its own axis"
                )));
            }
            let base = frame.origin + frame.reference * radius;
            let apex_ward = frame.axis + frame.reference * taper;
            revolution(Line(base, base + apex_ward), frame)
        }

        SurfaceGeometry::Sphere { center, radius, .. } => {
            // `axis` and `ref_direction` are deliberately dropped. They fix the
            // sphere's parameterization, not its point set, and a sphere's point
            // set is invariant under rotation about its own centre. Nothing
            // downstream reads the intermediate representation's parameter-space
            // curves -- `CompressedFace` has nowhere to store them and every
            // consumer re-derives (u, v) by projecting the 3D boundary -- so any
            // valid parameterization of the right point set is the right answer.
            // Keeping the source frame instead would cost a `Processor` transform
            // on every sphere and buy nothing.
            let radius = finite_radius(*radius, "sphere", context)?;
            let sphere = Processor::new(Sphere::new(frame::point(center), radius));
            admissible(&sphere, "sphere", context)?;
            Ok(Surface::SphericalSurface(sphere))
        }

        SurfaceGeometry::Torus {
            center,
            axis,
            ref_direction,
            major_radius,
            minor_radius,
        } => {
            // Unlike a sphere, a torus's point set DOES depend on its axis, so
            // the frame is carried in the `Processor`'s transform and the entity
            // stays canonical.
            let frame = frame::frame(center, axis, ref_direction, "torus", context)?;
            let major = finite_radius(*major_radius, "torus major radius", context)?;
            let minor = finite_radius(*minor_radius, "torus minor radius", context)?;
            // `Torus::new` PANICS on a non-positive radius. `finite_radius` has
            // already refused those, so this call cannot reach the panic -- which
            // is the point of checking here rather than trusting the file.
            let torus = Processor::with_transform(
                Torus::new(Point3::origin(), major, minor),
                frame.placement(1.0),
            );
            admissible(&torus, "torus", context)?;
            Ok(Surface::ToroidalSurface(torus))
        }

        SurfaceGeometry::Nurbs(surface) => nurbs(surface, context),

        SurfaceGeometry::Transformed { basis, transform } => {
            let mut surface = convert(basis, context)?;
            if !transform.is_affine() {
                return Err(context
                    .malformed("a transformed surface carries a non-affine placement".to_owned()));
            }
            surface.transform_by(frame::matrix(transform));
            Ok(surface)
        }

        // Refused by name, every one of them. A `Procedural` carrier names a
        // construction this crate would have to replay; `Polygonal` is an
        // approximation with a chordal bound, and admitting it as a spline patch
        // would present a tessellation as exact geometry; `Unknown` is bytes the
        // decoder itself could not classify.
        SurfaceGeometry::Procedural { .. } => Err(unsupported("procedural", context)),
        SurfaceGeometry::Polygonal { .. } => Err(unsupported("source-native polygonal", context)),
        SurfaceGeometry::Unknown { .. } => Err(unsupported("unclassified native", context)),
        // Matched EXHAUSTIVELY, with no catch-all. `SurfaceGeometry` is not
        // `#[non_exhaustive]`, so a cadmpeg release that adds a carrier breaks
        // this build -- which is what we want. A catch-all would turn that
        // release into a silent runtime refusal for a surface someone then has to
        // discover is unsupported, instead of a decision made when the dependency
        // moves.
    }
}

/// Revolve a profile line about a frame's axis.
fn revolution(profile: Line<Point3>, frame: frame::Frame) -> Result<Surface> {
    Ok(Surface::RevolutionSurface(Processor::new(
        RevolutionSurface::by_revolution(Curve::Line(profile), frame.origin, frame.axis),
    )))
}

/// A radius that can actually be built with.
fn finite_radius(radius: f64, what: &str, context: &Context<'_>) -> Result<f64> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(context.malformed(format!("{what} has radius {radius}")));
    }
    Ok(radius)
}

/// Ask the net builder whether this analytic carrier is admissible.
///
/// See the module note: the builder's refusal IS the predicate, so probing it
/// cannot drift from it the way a restated guard can.
fn admissible<T: TryIntoHomogeneousBsplineSurface>(
    surface: &T,
    what: &'static str,
    context: &Context<'_>,
) -> Result<()> {
    match surface.try_into_homogeneous_bspline_surface() {
        Some(_) => Ok(()),
        None => Err(crate::Error::UnsupportedSurfaceKind {
            format: context.format,
            kind: match what {
                "torus" => "degenerate torus",
                other => other,
            },
        }),
    }
}

fn unsupported(kind: &'static str, context: &Context<'_>) -> crate::Error {
    crate::Error::UnsupportedSurfaceKind {
        format: context.format,
        kind,
    }
}

/// A free-form surface, rational or not.
///
/// Non-rational nets become [`Surface::BsplineSurface`] rather than a
/// [`Surface::NurbsSurface`] with every weight at one: the polynomial form has no
/// weight division in its evaluation, and the boolean kernel's exact-patch
/// machinery distinguishes the two.
fn nurbs(surface: &IrNurbsSurface, context: &Context<'_>) -> Result<Surface> {
    let (rows, columns) = (surface.u_count as usize, surface.v_count as usize);
    if surface.u_periodic || surface.v_periodic {
        // A periodic knot vector is not a clamped one, and wrapping it correctly
        // means reconstructing control points the source did not send. Refused
        // rather than built with a seam in the wrong place.
        return Err(crate::Error::UnsupportedSurfaceKind {
            format: context.format,
            kind: "periodic NURBS",
        });
    }
    expect_length(
        surface.control_points.len(),
        rows * columns,
        "NURBS surface control points",
        context,
    )?;
    expect_length(
        surface.u_knots.len(),
        rows + surface.u_degree as usize + 1,
        "NURBS surface u knots",
        context,
    )?;
    expect_length(
        surface.v_knots.len(),
        columns + surface.v_degree as usize + 1,
        "NURBS surface v knots",
        context,
    )?;
    let knots = (
        KnotVector::from(surface.u_knots.clone()),
        KnotVector::from(surface.v_knots.clone()),
    );
    // Control points arrive u-major: index `i * v_count + j` is pole (i, j),
    // which is the row-of-v layout `BsplineSurface` wants.
    let rows_of = |row: usize| &surface.control_points[row * columns..(row + 1) * columns];
    match &surface.weights {
        None => {
            let net = (0..rows)
                .map(|row| rows_of(row).iter().map(frame::point).collect())
                .collect();
            Ok(Surface::BsplineSurface(built(
                BsplineSurface::try_new(knots, net),
                context,
            )?))
        }
        Some(weights) => {
            expect_length(
                weights.len(),
                rows * columns,
                "NURBS surface weights",
                context,
            )?;
            let net = (0..rows)
                .map(|row| {
                    rows_of(row)
                        .iter()
                        .zip(&weights[row * columns..(row + 1) * columns])
                        .map(|(point, weight)| homogeneous(point, *weight))
                        .collect()
                })
                .collect();
            Ok(Surface::NurbsSurface(NurbsSurface::new(built(
                BsplineSurface::try_new(knots, net),
                context,
            )?)))
        }
    }
}

/// A rational control point in homogeneous coordinates.
pub(super) fn homogeneous(point: &cadmpeg_ir::math::Point3, weight: f64) -> Vector4 {
    Vector4::new(point.x * weight, point.y * weight, point.z * weight, weight)
}

/// Report a count mismatch as malformed geometry rather than panicking in the
/// slice arithmetic below it.
pub(super) fn expect_length(
    actual: usize,
    expected: usize,
    what: &str,
    context: &Context<'_>,
) -> Result<()> {
    if actual != expected {
        return Err(context.malformed(format!(
            "{what}: expected {expected} values, the file carried {actual}"
        )));
    }
    Ok(())
}

/// Turn a geometry-crate construction failure into a typed import failure.
///
/// `BsplineSurface::new` panics where `try_new` reports; on file input the
/// reporting one is the only defensible choice.
fn built<T>(
    result: std::result::Result<T, monstertruck_geometry::errors::Error>,
    context: &Context<'_>,
) -> Result<T> {
    result.map_err(|error| context.malformed(error.to_string()))
}

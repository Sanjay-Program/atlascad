//! Curve carriers: intermediate representation to [`Curve`], trimmed to an edge.
//!
//! # A curve is converted FOR an edge, never on its own
//!
//! monstertruck's [`CompressedEdge`] holds a curve whose own bounded range runs
//! from the edge's start vertex to its end vertex: `Edge::try_new` checks the
//! curve's endpoints against the vertices it is given. The intermediate
//! representation instead stores an UNBOUNDED carrier plus the edge's parameter
//! interval on it -- a full circle and `[t0, t1]`, an infinite line and two
//! signed distances.
//!
//! So the conversion needs both halves, and the trim is not an afterthought. This
//! module takes the edge's interval with the carrier, and every arm below returns
//! a curve already restricted and already oriented start-to-end. Converting the
//! carrier first and trimming afterwards would need a second parameterization
//! argument at every call site, and would be wrong for exactly the carriers whose
//! parameterization is not preserved by the conversion.
//!
//! # What the interval means
//!
//! Per the representation's own contract, conic parameters are ANGLES from the
//! reference direction and line parameters are SIGNED DISTANCES along the unit
//! direction. They are not normalised to `[0, 1]`, and the start vertex lies at
//! `t0`. A converter that assumed a normalised interval would produce arcs of the
//! right shape in the wrong place.
//!
//! [`CompressedEdge`]: monstertruck_topology::compress::CompressedEdge

use cadmpeg_ir::geometry::{CurveGeometry, NurbsCurve as IrNurbsCurve};
use monstertruck_geometry::prelude::{
    BoundedCurve, BsplineCurve, Cut, KnotVector, NurbsCurve, Processor, TrimmedCurve, UnitCircle,
};
use monstertruck_modeling::{Curve, Line, Point3, SquareMatrix, Transform as _, Transformed};

use super::surface::{expect_length, homogeneous};
use super::{Context, frame};
use crate::{Error, Result};

/// The parameter interval an edge occupies on its carrier, when the file says.
pub(super) type Interval = Option<[f64; 2]>;

/// Convert one curve carrier, restricted to `interval` and oriented so that the
/// result runs from the edge's start vertex to its end vertex.
///
/// `endpoints` are the edge's own vertex positions. They are the fallback for
/// carriers whose restriction is determined by its endpoints -- a line -- and they
/// are never used to REPLACE a stated interval.
pub(super) fn convert(
    geometry: &CurveGeometry,
    interval: Interval,
    endpoints: (Point3, Point3),
    context: &Context<'_>,
) -> Result<Curve> {
    match geometry {
        CurveGeometry::Line { origin, direction } => {
            let origin = frame::point(origin);
            let direction = frame::vector(direction);
            match interval {
                // The stated interval wins: a line's parameter is a signed
                // distance, so this reproduces the source's own endpoints
                // exactly, including the case where they sit a tolerance away
                // from the vertex points.
                Some([start, end]) => Ok(Curve::Line(Line(
                    origin + direction * start,
                    origin + direction * end,
                ))),
                // With no interval the vertices ARE the trim, exactly: the
                // segment between two points on a line is that line's
                // restriction, with nothing inferred.
                None => Ok(Curve::Line(Line(endpoints.0, endpoints.1))),
            }
        }

        CurveGeometry::Circle {
            center,
            axis,
            ref_direction,
            radius,
        } => {
            let frame = frame::frame(center, axis, ref_direction, "circle", context)?;
            if !radius.is_finite() || *radius <= 0.0 {
                return Err(context.malformed(format!("circle has radius {radius}")));
            }
            // An arc needs its angles. Without them the carrier is the whole
            // circle, and which of the two arcs between the vertices the edge
            // meant is not recoverable -- so this refuses instead of picking one.
            let Some([start, end]) = interval else {
                return Err(context.malformed(
                    "a circular edge carries no parameter range, so which arc between its \
                     vertices it spans is not recoverable"
                        .to_owned(),
                ));
            };
            // The radius rides in the placement and the entity stays a UNIT
            // circle, which is the construction `monstertruck-modeling`'s own
            // `circle_arc` uses. `TrimmedCurve`'s range is the angle sweep, so a
            // reversed interval gives the arc traversed the other way and the
            // endpoints still land on the right vertices.
            let arc = Processor::with_transform(
                TrimmedCurve::new(UnitCircle::<Point3>::new(), (start, end)),
                frame.placement(*radius),
            );
            // Exact: a circular arc IS a rational quadratic, so this is a change
            // of representation and not an approximation.
            rational(&arc, context)
        }

        CurveGeometry::Nurbs(curve) => nurbs(curve, interval, context),

        CurveGeometry::Degenerate { .. } => Err(unsupported("degenerate", context)),

        CurveGeometry::Transformed { basis, transform } => {
            if !transform.is_affine() {
                return Err(context
                    .malformed("a transformed curve carries a non-affine placement".to_owned()));
            }
            let matrix = frame::matrix(transform);
            // The endpoints are in MODEL space and the basis carrier is not, so
            // the fallback endpoints have to be pulled back before the basis is
            // converted, or a line with no stated interval would be built from
            // points that are not on it. An affine map is invertible or it is not
            // a placement.
            let inverse = matrix.invert().ok_or_else(|| {
                context.malformed("a transformed curve carries a singular placement".to_owned())
            })?;
            let pulled = (
                inverse.transform_point(endpoints.0),
                inverse.transform_point(endpoints.1),
            );
            let mut curve = convert(basis, interval, pulled, context)?;
            curve.transform_by(matrix);
            Ok(curve)
        }

        // Ellipses, parabolas and hyperbolas are exactly representable as
        // rational curves in principle, but this crate's geometry has no builder
        // for them -- only the circle has one -- so admitting them would mean
        // writing that builder here, unverified, and calling the result exact.
        // Refused by name until the builder exists next to the circle's.
        CurveGeometry::Ellipse { .. } => Err(unsupported("elliptical", context)),
        CurveGeometry::Parabola { .. } => Err(unsupported("parabolic", context)),
        CurveGeometry::Hyperbola { .. } => Err(unsupported("hyperbolic", context)),
        // A composite carrier is several curves in one edge; monstertruck's
        // `CompressedEdge` holds exactly one, so admitting it needs the edge
        // SPLIT, which is a topology change and not a geometry conversion.
        CurveGeometry::Composite { .. } => Err(unsupported("composite", context)),
        CurveGeometry::Procedural { .. } => Err(unsupported("procedural", context)),
        // An approximation with a chordal bound. Admitting it as a spline would
        // present a polyline as the exact edge.
        CurveGeometry::Polyline { .. } => Err(unsupported("source-native polyline", context)),
        CurveGeometry::Unknown { .. } => Err(unsupported("unclassified native", context)),
        // Exhaustive on purpose, with no catch-all: see the note in
        // [`super::surface::convert`].
    }
}

fn unsupported(kind: &'static str, context: &Context<'_>) -> Error {
    Error::UnsupportedCurveKind {
        format: context.format,
        kind,
    }
}

/// A carrier whose exact form is rational, as a [`Curve::NurbsCurve`].
fn rational<T>(curve: &T, context: &Context<'_>) -> Result<Curve>
where T: monstertruck_geometry::prelude::TryIntoHomogeneousBsplineCurve {
    curve
        .try_into_homogeneous_bspline_curve()
        .map(|net| Curve::NurbsCurve(NurbsCurve::new(net)))
        .ok_or_else(|| {
            context.malformed(
                "an exactly-representable curve could not be built, which means its own numbers \
                 are degenerate"
                    .to_owned(),
            )
        })
}

/// A free-form curve, restricted to the edge's knot interval.
fn nurbs(curve: &IrNurbsCurve, interval: Interval, context: &Context<'_>) -> Result<Curve> {
    let count = curve.control_points.len();
    if curve.periodic {
        // Same reasoning as the periodic surface: wrapping the knot vector means
        // reconstructing control points the file did not send.
        return Err(unsupported("periodic NURBS", context));
    }
    expect_length(
        curve.knots.len(),
        count + curve.degree as usize + 1,
        "NURBS curve knots",
        context,
    )?;
    let knots = KnotVector::from(curve.knots.clone());
    let mut converted = match &curve.weights {
        None => Curve::BsplineCurve(built(
            BsplineCurve::try_new(
                knots,
                curve.control_points.iter().map(frame::point).collect(),
            ),
            context,
        )?),
        Some(weights) => {
            expect_length(weights.len(), count, "NURBS curve weights", context)?;
            Curve::NurbsCurve(NurbsCurve::new(built(
                BsplineCurve::try_new(
                    knots,
                    curve
                        .control_points
                        .iter()
                        .zip(weights)
                        .map(|(point, weight)| homogeneous(point, *weight))
                        .collect(),
                ),
                context,
            )?))
        }
    };
    // A free-form carrier's parameterization IS preserved by the conversion --
    // the knot vector carries the source's own knot values -- so the edge's
    // interval is a knot-span restriction and `Cut` performs it exactly.
    if let Some([start, end]) = interval {
        restrict(&mut converted, start, end, context)?;
    }
    Ok(converted)
}

/// Restrict a curve to `[start, end]` on its own parameterization, reversing it
/// when the edge runs backwards along the carrier.
///
/// `Cut` splits in place: after `cut(t)` the receiver covers `[a, t]` and the
/// returned curve covers `[t, b]`. Two cuts therefore isolate the middle span,
/// and the order matters -- cutting at the far end first would discard the span
/// the second cut needs.
fn restrict(curve: &mut Curve, start: f64, end: f64, context: &Context<'_>) -> Result<()> {
    let (low, high) = (start.min(end), start.max(end));
    let (own_low, own_high) = curve.range_tuple();
    let slack = monstertruck_modeling::TOLERANCE.max((own_high - own_low).abs() * 1.0e-9);
    if !low.is_finite() || !high.is_finite() {
        return Err(context.malformed(format!(
            "an edge carries the parameter range [{start}, {end}]"
        )));
    }
    if low < own_low - slack || high > own_high + slack {
        // Extrapolating a spline past its knot span is not the same curve, so a
        // range that leaves the carrier is a defect in the file, not a rounding
        // question.
        return Err(context.malformed(format!(
            "an edge spans [{low}, {high}] on a curve whose own range is [{own_low}, {own_high}]"
        )));
    }
    // Clamp INTO the carrier's range before cutting: a range that is inside it to
    // within `slack` is the whole carrier, and asking `Cut` for a parameter a few
    // ulps outside is not defined.
    let (low, high) = (low.max(own_low), high.min(own_high));
    if high - own_low < slack || own_high - low < slack {
        return Err(context.malformed(format!(
            "an edge spans the empty range [{low}, {high}] on its curve"
        )));
    }
    if own_high - high > slack {
        let _discarded = curve.cut(high);
    }
    if low - own_low > slack {
        *curve = curve.cut(low);
    }
    if end < start {
        // The edge runs against the carrier's parameter direction. Inverting here
        // -- rather than leaving it to the coedge's sense -- keeps the invariant
        // this module promises: the returned curve runs start vertex to end
        // vertex, so `Edge::try_new` can check it.
        monstertruck_modeling::Invertible::invert(curve);
    }
    Ok(())
}

/// Turn a geometry-crate construction failure into a typed import failure.
fn built<T>(
    result: std::result::Result<T, monstertruck_geometry::errors::Error>,
    context: &Context<'_>,
) -> Result<T> {
    result.map_err(|error| context.malformed(error.to_string()))
}

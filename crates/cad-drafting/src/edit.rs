//! Geometric transformation, editing, and inquiry operations on 2D drafting entities.
//!
//! Provides production CAD operations:
//! - Offset (parallel line, concentric circle/arc, offset rectangle, mitered polyline)
//! - Fillet (tangent arc between two lines with radius R, or R=0 corner intersection/trim)
//! - Chamfer (beveled corner between two lines with distances d1 and d2)
//! - Transform (translate, rotate by angle, scale from base point, mirror across axis)
//! - Explode (decompose rectangles and polylines into individual lines)
//! - Inquiries (distance/angle, polygon area/perimeter via shoelace formula, entity metrics)

use crate::{DraftError, EntityKind};
use serde::{Deserialize, Serialize};

/// Result of an inquiry measuring distance and angle between two points.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DistanceInquiry {
    pub distance: f64,
    pub delta_x: f64,
    pub delta_y: f64,
    pub angle_deg: f64,
}

/// Result of an inquiry measuring enclosed polygon area and perimeter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AreaInquiry {
    pub area: f64,
    pub perimeter: f64,
    pub centroid: [f64; 2],
}

/// Result of an inquiry inspecting detailed geometric properties of an entity.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EntityProperties {
    pub kind: String,
    pub length: f64,
    pub area: Option<f64>,
    pub bounds: [f64; 4],
}

/// Structural and mechanical engineering cross-section properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SectionProperties {
    pub area: f64,
    pub perimeter: f64,
    pub centroid: [f64; 2],
    pub i_xx: f64,
    pub i_yy: f64,
    pub i_xy: f64,
    pub j_polar: f64,
    pub i_1: f64,
    pub i_2: f64,
    pub principal_angle_deg: f64,
    pub r_x: f64,
    pub r_y: f64,
    pub s_x: f64,
    pub s_y: f64,
}

/// Computes distance and angle in degrees between two 2D points.
pub fn measure_distance(p1: [f64; 2], p2: [f64; 2]) -> Result<DistanceInquiry, DraftError> {
    if !p1[0].is_finite() || !p1[1].is_finite() || !p2[0].is_finite() || !p2[1].is_finite() {
        return Err(DraftError::Invalid(
            "coordinates must be finite numbers".into(),
        ));
    }
    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let dist = (dx * dx + dy * dy).sqrt();
    let angle_deg = dy.atan2(dx).to_degrees().rem_euclid(360.0);
    Ok(DistanceInquiry {
        distance: dist,
        delta_x: dx,
        delta_y: dy,
        angle_deg,
    })
}

/// Computes area and perimeter of a polygon using the shoelace formula.
pub fn measure_polygon_area(points: &[[f64; 2]]) -> Result<AreaInquiry, DraftError> {
    if points.len() < 3 {
        return Err(DraftError::Invalid(
            "an area measurement requires at least 3 points to form a polygon".into(),
        ));
    }
    for p in points {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(DraftError::Invalid(
                "coordinates must be finite numbers".into(),
            ));
        }
    }
    let n = points.len();
    let mut double_area = 0.0;
    let mut perimeter = 0.0;
    let mut cx_sum = 0.0;
    let mut cy_sum = 0.0;

    for i in 0..n {
        let j = (i + 1) % n;
        let x0 = points[i][0];
        let y0 = points[i][1];
        let x1 = points[j][0];
        let y1 = points[j][1];

        let cross = x0 * y1 - x1 * y0;
        double_area += cross;
        cx_sum += (x0 + x1) * cross;
        cy_sum += (y0 + y1) * cross;

        let dx = x1 - x0;
        let dy = y1 - y0;
        perimeter += (dx * dx + dy * dy).sqrt();
    }

    let area = (double_area.abs()) * 0.5;
    let centroid = if double_area.abs() > 1e-12 {
        let factor = 1.0 / (3.0 * double_area);
        [cx_sum * factor, cy_sum * factor]
    } else {
        let sum_x: f64 = points.iter().map(|p| p[0]).sum();
        let sum_y: f64 = points.iter().map(|p| p[1]).sum();
        [sum_x / n as f64, sum_y / n as f64]
    };

    Ok(AreaInquiry {
        area,
        perimeter,
        centroid,
    })
}

/// Translates an entity by displacement `(dx, dy)`.
pub fn translate_entity(kind: &EntityKind, dx: f64, dy: f64) -> Result<EntityKind, DraftError> {
    if !dx.is_finite() || !dy.is_finite() {
        return Err(DraftError::Invalid(
            "displacement values must be finite".into(),
        ));
    }
    let shift = |p: &[f64; 2]| [p[0] + dx, p[1] + dy];
    let res = match kind {
        EntityKind::Point { at } => EntityKind::Point { at: shift(at) },
        EntityKind::Line { start, end } => EntityKind::Line {
            start: shift(start),
            end: shift(end),
        },
        EntityKind::Circle { center, radius } => EntityKind::Circle {
            center: shift(center),
            radius: *radius,
        },
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => EntityKind::Arc {
            center: shift(center),
            radius: *radius,
            start_degrees: *start_degrees,
            end_degrees: *end_degrees,
        },
        EntityKind::Polyline { points, closed } => EntityKind::Polyline {
            points: points.iter().map(shift).collect(),
            closed: *closed,
        },
        EntityKind::Text { at, height, value } => EntityKind::Text {
            at: shift(at),
            height: *height,
            value: value.clone(),
        },
        EntityKind::Dimension {
            start,
            end,
            offset,
            text,
        } => EntityKind::Dimension {
            start: shift(start),
            end: shift(end),
            offset: *offset,
            text: text.clone(),
        },
        EntityKind::Rectangle {
            origin,
            width,
            height,
        } => EntityKind::Rectangle {
            origin: shift(origin),
            width: *width,
            height: *height,
        },
        EntityKind::Ellipse {
            center,
            radius_x,
            radius_y,
        } => EntityKind::Ellipse {
            center: shift(center),
            radius_x: *radius_x,
            radius_y: *radius_y,
        },
        EntityKind::Spline { points, closed } => EntityKind::Spline {
            points: points.iter().map(shift).collect(),
            closed: *closed,
        },
    };
    Ok(res)
}

/// Rotates an entity around `center` by `angle_rad` radians counter-clockwise.
pub fn rotate_entity(
    kind: &EntityKind,
    center: [f64; 2],
    angle_rad: f64,
) -> Result<EntityKind, DraftError> {
    if !center[0].is_finite() || !center[1].is_finite() || !angle_rad.is_finite() {
        return Err(DraftError::Invalid(
            "rotation parameters must be finite".into(),
        ));
    }
    let cos_a = angle_rad.cos();
    let sin_a = angle_rad.sin();
    let rot = |p: &[f64; 2]| {
        let x = p[0] - center[0];
        let y = p[1] - center[1];
        [
            center[0] + x * cos_a - y * sin_a,
            center[1] + x * sin_a + y * cos_a,
        ]
    };
    let angle_deg = angle_rad.to_degrees();

    let res = match kind {
        EntityKind::Point { at } => EntityKind::Point { at: rot(at) },
        EntityKind::Line { start, end } => EntityKind::Line {
            start: rot(start),
            end: rot(end),
        },
        EntityKind::Circle { center: c, radius } => EntityKind::Circle {
            center: rot(c),
            radius: *radius,
        },
        EntityKind::Arc {
            center: c,
            radius,
            start_degrees,
            end_degrees,
        } => EntityKind::Arc {
            center: rot(c),
            radius: *radius,
            start_degrees: (start_degrees + angle_deg).rem_euclid(360.0),
            end_degrees: (end_degrees + angle_deg).rem_euclid(360.0),
        },
        EntityKind::Polyline { points, closed } => EntityKind::Polyline {
            points: points.iter().map(rot).collect(),
            closed: *closed,
        },
        EntityKind::Text { at, height, value } => EntityKind::Text {
            at: rot(at),
            height: *height,
            value: value.clone(),
        },
        EntityKind::Dimension {
            start,
            end,
            offset,
            text,
        } => EntityKind::Dimension {
            start: rot(start),
            end: rot(end),
            offset: *offset,
            text: text.clone(),
        },
        EntityKind::Rectangle {
            origin,
            width,
            height,
        } => {
            // General rotation transforms a rectangle into an oriented 4-point closed Polyline
            let p0 = *origin;
            let p1 = [origin[0] + width, origin[1]];
            let p2 = [origin[0] + width, origin[1] + height];
            let p3 = [origin[0], origin[1] + height];
            EntityKind::Polyline {
                points: vec![rot(&p0), rot(&p1), rot(&p2), rot(&p3)],
                closed: true,
            }
        }
        EntityKind::Ellipse {
            center: c,
            radius_x,
            radius_y,
        } => {
            // Ellipse aligned to XY: if rotated by multiples of 90 degrees, keep ellipse representation
            let deg_norm = angle_deg.rem_euclid(360.0);
            if (deg_norm - 90.0).abs() < 1e-6 || (deg_norm - 270.0).abs() < 1e-6 {
                EntityKind::Ellipse {
                    center: rot(c),
                    radius_x: *radius_y,
                    radius_y: *radius_x,
                }
            } else if (deg_norm - 0.0).abs() < 1e-6 || (deg_norm - 180.0).abs() < 1e-6 {
                EntityKind::Ellipse {
                    center: rot(c),
                    radius_x: *radius_x,
                    radius_y: *radius_y,
                }
            } else {
                // Approximate general rotated ellipse with 32 points closed polyline
                let mut points = Vec::with_capacity(32);
                for i in 0..32 {
                    let theta = (i as f64) * std::f64::consts::TAU / 32.0;
                    let local_x = radius_x * theta.cos();
                    let local_y = radius_y * theta.sin();
                    let rotated_x = local_x * cos_a - local_y * sin_a;
                    let rotated_y = local_x * sin_a + local_y * cos_a;
                    let new_center = rot(c);
                    points.push([new_center[0] + rotated_x, new_center[1] + rotated_y]);
                }
                EntityKind::Polyline {
                    points,
                    closed: true,
                }
            }
        }
        EntityKind::Spline { points, closed } => EntityKind::Spline {
            points: points.iter().map(rot).collect(),
            closed: *closed,
        },
    };
    Ok(res)
}

/// Scales an entity around `base` by `factor` > 0.
pub fn scale_entity(
    kind: &EntityKind,
    base: [f64; 2],
    factor: f64,
) -> Result<EntityKind, DraftError> {
    if !base[0].is_finite() || !base[1].is_finite() || !factor.is_finite() || factor <= 0.0 {
        return Err(DraftError::Invalid(
            "scale factor must be a finite positive number".into(),
        ));
    }
    let sc = |p: &[f64; 2]| {
        [
            base[0] + factor * (p[0] - base[0]),
            base[1] + factor * (p[1] - base[1]),
        ]
    };

    let res = match kind {
        EntityKind::Point { at } => EntityKind::Point { at: sc(at) },
        EntityKind::Line { start, end } => EntityKind::Line {
            start: sc(start),
            end: sc(end),
        },
        EntityKind::Circle { center, radius } => EntityKind::Circle {
            center: sc(center),
            radius: radius * factor,
        },
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => EntityKind::Arc {
            center: sc(center),
            radius: radius * factor,
            start_degrees: *start_degrees,
            end_degrees: *end_degrees,
        },
        EntityKind::Polyline { points, closed } => EntityKind::Polyline {
            points: points.iter().map(sc).collect(),
            closed: *closed,
        },
        EntityKind::Text { at, height, value } => EntityKind::Text {
            at: sc(at),
            height: height * factor,
            value: value.clone(),
        },
        EntityKind::Dimension {
            start,
            end,
            offset,
            text,
        } => EntityKind::Dimension {
            start: sc(start),
            end: sc(end),
            offset: offset * factor,
            text: text.clone(),
        },
        EntityKind::Rectangle {
            origin,
            width,
            height,
        } => EntityKind::Rectangle {
            origin: sc(origin),
            width: width * factor,
            height: height * factor,
        },
        EntityKind::Ellipse {
            center,
            radius_x,
            radius_y,
        } => EntityKind::Ellipse {
            center: sc(center),
            radius_x: radius_x * factor,
            radius_y: radius_y * factor,
        },
        EntityKind::Spline { points, closed } => EntityKind::Spline {
            points: points.iter().map(sc).collect(),
            closed: *closed,
        },
    };
    Ok(res)
}

/// Mirrors an entity across an axis line defined by two points `p1` and `p2`.
pub fn mirror_entity(
    kind: &EntityKind,
    p1: [f64; 2],
    p2: [f64; 2],
) -> Result<EntityKind, DraftError> {
    if !p1[0].is_finite() || !p1[1].is_finite() || !p2[0].is_finite() || !p2[1].is_finite() {
        return Err(DraftError::Invalid("axis points must be finite".into()));
    }
    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let len = (dx * dx + dy * dy).sqrt();
    if len < 1e-9 {
        return Err(DraftError::Invalid(
            "mirror axis requires two distinct points".into(),
        ));
    }
    // Unit normal to mirror axis
    let nx = -dy / len;
    let ny = dx / len;

    let mir = |p: &[f64; 2]| {
        let vx = p[0] - p1[0];
        let vy = p[1] - p1[1];
        let d = vx * nx + vy * ny;
        [p[0] - 2.0 * d * nx, p[1] - 2.0 * d * ny]
    };

    let res = match kind {
        EntityKind::Point { at } => EntityKind::Point { at: mir(at) },
        EntityKind::Line { start, end } => EntityKind::Line {
            start: mir(start),
            end: mir(end),
        },
        EntityKind::Circle { center, radius } => EntityKind::Circle {
            center: mir(center),
            radius: *radius,
        },
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => {
            let new_center = mir(center);
            let s_rad = start_degrees.to_radians();
            let e_rad = end_degrees.to_radians();
            let p_start = [
                center[0] + radius * s_rad.cos(),
                center[1] + radius * s_rad.sin(),
            ];
            let p_end = [
                center[0] + radius * e_rad.cos(),
                center[1] + radius * e_rad.sin(),
            ];
            let m_start = mir(&p_start);
            let m_end = mir(&p_end);
            // In reflection, orientation reverses: swap start and end to maintain CCW arc
            let new_start_deg = (m_end[1] - new_center[1])
                .atan2(m_end[0] - new_center[0])
                .to_degrees()
                .rem_euclid(360.0);
            let new_end_deg = (m_start[1] - new_center[1])
                .atan2(m_start[0] - new_center[0])
                .to_degrees()
                .rem_euclid(360.0);
            EntityKind::Arc {
                center: new_center,
                radius: *radius,
                start_degrees: new_start_deg,
                end_degrees: new_end_deg,
            }
        }
        EntityKind::Polyline { points, closed } => {
            let mut mirrored_points: Vec<[f64; 2]> = points.iter().map(mir).collect();
            // Reverse ordering to preserve natural counter-clockwise / clockwise winding
            mirrored_points.reverse();
            EntityKind::Polyline {
                points: mirrored_points,
                closed: *closed,
            }
        }
        EntityKind::Text { at, height, value } => EntityKind::Text {
            at: mir(at),
            height: *height,
            value: value.clone(),
        },
        EntityKind::Dimension {
            start,
            end,
            offset,
            text,
        } => EntityKind::Dimension {
            start: mir(start),
            end: mir(end),
            offset: -offset,
            text: text.clone(),
        },
        EntityKind::Rectangle {
            origin,
            width,
            height,
        } => {
            let p0 = *origin;
            let p1 = [origin[0] + width, origin[1]];
            let p2 = [origin[0] + width, origin[1] + height];
            let p3 = [origin[0], origin[1] + height];
            EntityKind::Polyline {
                points: vec![mir(&p0), mir(&p3), mir(&p2), mir(&p1)],
                closed: true,
            }
        }
        EntityKind::Ellipse {
            center,
            radius_x,
            radius_y,
        } => {
            let axis_angle = dy.atan2(dx).to_degrees().rem_euclid(180.0);
            if axis_angle.abs() < 1e-6 || (axis_angle - 90.0).abs() < 1e-6 {
                EntityKind::Ellipse {
                    center: mir(center),
                    radius_x: *radius_x,
                    radius_y: *radius_y,
                }
            } else {
                let mut points = Vec::with_capacity(32);
                for i in 0..32 {
                    let theta = (i as f64) * std::f64::consts::TAU / 32.0;
                    let p = [
                        center[0] + radius_x * theta.cos(),
                        center[1] + radius_y * theta.sin(),
                    ];
                    points.push(mir(&p));
                }
                points.reverse();
                EntityKind::Polyline {
                    points,
                    closed: true,
                }
            }
        }
        EntityKind::Spline { points, closed } => {
            let mut mirrored: Vec<[f64; 2]> = points.iter().map(mir).collect();
            mirrored.reverse();
            EntityKind::Spline {
                points: mirrored,
                closed: *closed,
            }
        }
    };
    Ok(res)
}

/// Offsets an entity by distance `distance`.
///
/// For lines: positive distance offsets along the left normal; negative along right.
/// For circles/arcs: positive expands radius; negative contracts radius.
/// For rectangles: positive expands width/height; negative contracts.
/// For polylines: offsets each segment and miters adjacent vertices.
pub fn offset_entity(
    kind: &EntityKind,
    distance: f64,
    side: f64,
) -> Result<EntityKind, DraftError> {
    if !distance.is_finite() || distance == 0.0 {
        return Err(DraftError::Invalid(
            "offset distance must be a non-zero finite number".into(),
        ));
    }
    let signed_d = if side < 0.0 {
        -distance.abs()
    } else {
        distance.abs()
    };

    match kind {
        EntityKind::Line { start, end } => {
            let dx = end[0] - start[0];
            let dy = end[1] - start[1];
            let len = (dx * dx + dy * dy).sqrt();
            if len < 1e-9 {
                return Err(DraftError::Invalid("line is too short to offset".into()));
            }
            let nx = -dy / len;
            let ny = dx / len;
            let offset_vec = [nx * signed_d, ny * signed_d];
            Ok(EntityKind::Line {
                start: [start[0] + offset_vec[0], start[1] + offset_vec[1]],
                end: [end[0] + offset_vec[0], end[1] + offset_vec[1]],
            })
        }
        EntityKind::Circle { center, radius } => {
            let new_radius = radius + signed_d;
            if new_radius <= 0.0 {
                return Err(DraftError::Invalid(format!(
                    "offset distance {distance} reduces circle radius to non-positive"
                )));
            }
            Ok(EntityKind::Circle {
                center: *center,
                radius: new_radius,
            })
        }
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => {
            let new_radius = radius + signed_d;
            if new_radius <= 0.0 {
                return Err(DraftError::Invalid(format!(
                    "offset distance {distance} reduces arc radius to non-positive"
                )));
            }
            Ok(EntityKind::Arc {
                center: *center,
                radius: new_radius,
                start_degrees: *start_degrees,
                end_degrees: *end_degrees,
            })
        }
        EntityKind::Rectangle {
            origin,
            width,
            height,
        } => {
            let new_width = width + 2.0 * signed_d;
            let new_height = height + 2.0 * signed_d;
            if new_width <= 0.0 || new_height <= 0.0 {
                return Err(DraftError::Invalid(format!(
                    "offset distance {distance} reduces rectangle dimensions to non-positive"
                )));
            }
            Ok(EntityKind::Rectangle {
                origin: [origin[0] - signed_d, origin[1] - signed_d],
                width: new_width,
                height: new_height,
            })
        }
        EntityKind::Polyline { points, closed } => {
            let n = points.len();
            if n < 2 {
                return Err(DraftError::Invalid(
                    "polyline must have at least 2 points".into(),
                ));
            }
            let mut offset_segments = Vec::with_capacity(if *closed { n } else { n - 1 });
            let count = if *closed { n } else { n - 1 };

            for i in 0..count {
                let j = (i + 1) % n;
                let p0 = points[i];
                let p1 = points[j];
                let dx = p1[0] - p0[0];
                let dy = p1[1] - p0[1];
                let len = (dx * dx + dy * dy).sqrt();
                if len < 1e-9 {
                    continue;
                }
                let nx = -dy / len;
                let ny = dx / len;
                let s_off = [p0[0] + nx * signed_d, p0[1] + ny * signed_d];
                let e_off = [p1[0] + nx * signed_d, p1[1] + ny * signed_d];
                offset_segments.push((s_off, e_off, [dx / len, dy / len]));
            }

            if offset_segments.is_empty() {
                return Err(DraftError::Invalid(
                    "polyline has degenerate segments".into(),
                ));
            }

            let mut new_points = Vec::with_capacity(n);
            let num_seg = offset_segments.len();

            if *closed {
                for i in 0..num_seg {
                    let prev_idx = if i == 0 { num_seg - 1 } else { i - 1 };
                    let seg_prev = &offset_segments[prev_idx];
                    let seg_curr = &offset_segments[i];
                    if let Some(pt) =
                        line_intersection(seg_prev.0, seg_prev.1, seg_curr.0, seg_curr.1)
                    {
                        new_points.push(pt);
                    } else {
                        new_points.push(seg_curr.0);
                    }
                }
            } else {
                new_points.push(offset_segments[0].0);
                for i in 0..num_seg - 1 {
                    let seg_curr = &offset_segments[i];
                    let seg_next = &offset_segments[i + 1];
                    if let Some(pt) =
                        line_intersection(seg_curr.0, seg_curr.1, seg_next.0, seg_next.1)
                    {
                        new_points.push(pt);
                    } else {
                        new_points.push(seg_curr.1);
                    }
                }
                new_points.push(offset_segments[num_seg - 1].1);
            }

            Ok(EntityKind::Polyline {
                points: new_points,
                closed: *closed,
            })
        }
        EntityKind::Ellipse { .. }
        | EntityKind::Spline { .. }
        | EntityKind::Point { .. }
        | EntityKind::Text { .. }
        | EntityKind::Dimension { .. } => Err(DraftError::Invalid(format!(
            "offset is not supported on {}",
            kind.label()
        ))),
    }
}

/// Explodes compound entities into fundamental Line entities.
///
/// A Rectangle decomposes into 4 Lines.
/// A Polyline decomposes into `N - 1` (open) or `N` (closed) Lines.
pub fn explode_entity(kind: &EntityKind) -> Result<Vec<EntityKind>, DraftError> {
    match kind {
        EntityKind::Rectangle {
            origin,
            width,
            height,
        } => {
            let p0 = *origin;
            let p1 = [origin[0] + width, origin[1]];
            let p2 = [origin[0] + width, origin[1] + height];
            let p3 = [origin[0], origin[1] + height];
            Ok(vec![
                EntityKind::Line { start: p0, end: p1 },
                EntityKind::Line { start: p1, end: p2 },
                EntityKind::Line { start: p2, end: p3 },
                EntityKind::Line { start: p3, end: p0 },
            ])
        }
        EntityKind::Polyline { points, closed } => {
            if points.len() < 2 {
                return Err(DraftError::Invalid(
                    "polyline needs at least 2 points to explode".into(),
                ));
            }
            let n = points.len();
            let count = if *closed { n } else { n - 1 };
            let mut lines = Vec::with_capacity(count);
            for i in 0..count {
                let j = (i + 1) % n;
                lines.push(EntityKind::Line {
                    start: points[i],
                    end: points[j],
                });
            }
            Ok(lines)
        }
        _ => Err(DraftError::Invalid(format!(
            "{} is already a primitive entity and cannot be exploded",
            kind.label()
        ))),
    }
}

/// Fillets two lines with radius `radius`.
///
/// If `radius == 0.0`: trims or extends both lines so they meet precisely at their intersection point.
/// If `radius > 0.0`: trims both lines to tangent points and inserts a tangent `EntityKind::Arc`.
///
/// Returns `(trimmed_line1, trimmed_line2, optional_fillet_arc)`.
pub fn fillet_lines(
    l1: &EntityKind,
    l2: &EntityKind,
    radius: f64,
) -> Result<(EntityKind, EntityKind, Option<EntityKind>), DraftError> {
    let (s1, e1) = match l1 {
        EntityKind::Line { start, end } => (*start, *end),
        _ => return Err(DraftError::Invalid("fillet requires line entities".into())),
    };
    let (s2, e2) = match l2 {
        EntityKind::Line { start, end } => (*start, *end),
        _ => return Err(DraftError::Invalid("fillet requires line entities".into())),
    };

    if !radius.is_finite() || radius < 0.0 {
        return Err(DraftError::Invalid(
            "fillet radius must be finite and non-negative".into(),
        ));
    }

    let p_int = match line_intersection(s1, e1, s2, e2) {
        Some(p) => p,
        None => {
            return Err(DraftError::Invalid(
                "lines are parallel and cannot be filleted".into(),
            ));
        }
    };

    // Find the outer endpoint for each line (the endpoint farthest from intersection point)
    let d_s1 = dist_sq(s1, p_int);
    let d_e1 = dist_sq(e1, p_int);
    let (outer1, orig_s1) = if d_s1 > d_e1 { (s1, true) } else { (e1, false) };

    let d_s2 = dist_sq(s2, p_int);
    let d_e2 = dist_sq(e2, p_int);
    let (outer2, orig_s2) = if d_s2 > d_e2 { (s2, true) } else { (e2, false) };

    let len1 = (outer1[0] - p_int[0]).hypot(outer1[1] - p_int[1]);
    let len2 = (outer2[0] - p_int[0]).hypot(outer2[1] - p_int[1]);
    if len1 < 1e-6 || len2 < 1e-6 {
        return Err(DraftError::Invalid("lines are too short to fillet".into()));
    }

    let u1 = [(outer1[0] - p_int[0]) / len1, (outer1[1] - p_int[1]) / len1];
    let u2 = [(outer2[0] - p_int[0]) / len2, (outer2[1] - p_int[1]) / len2];

    let dot = u1[0] * u2[0] + u1[1] * u2[1];
    if dot.abs() > 0.999999 {
        return Err(DraftError::Invalid(
            "lines are collinear; fillet requires intersecting lines".into(),
        ));
    }

    if radius == 0.0 {
        let new_l1 = if orig_s1 {
            EntityKind::Line {
                start: outer1,
                end: p_int,
            }
        } else {
            EntityKind::Line {
                start: p_int,
                end: outer1,
            }
        };
        let new_l2 = if orig_s2 {
            EntityKind::Line {
                start: outer2,
                end: p_int,
            }
        } else {
            EntityKind::Line {
                start: p_int,
                end: outer2,
            }
        };
        return Ok((new_l1, new_l2, None));
    }

    // Angle theta between direction vectors u1 and u2:
    let half_theta = (dot.clamp(-1.0, 1.0).acos()) * 0.5;
    let tan_half = half_theta.tan();
    let sin_half = half_theta.sin();
    if tan_half < 1e-6 || sin_half < 1e-6 {
        return Err(DraftError::Invalid(
            "lines meet at an angle too small to fillet".into(),
        ));
    }

    let t = radius / tan_half;
    if t > len1 || t > len2 {
        return Err(DraftError::Invalid(format!(
            "fillet radius {radius} is too large for the line segments (required tangent distance {t:.2})"
        )));
    }

    let t1 = [p_int[0] + t * u1[0], p_int[1] + t * u1[1]];
    let t2 = [p_int[0] + t * u2[0], p_int[1] + t * u2[1]];

    // Bisector vector
    let bx = u1[0] + u2[0];
    let by = u1[1] + u2[1];
    let blen = (bx * bx + by * by).sqrt();
    if blen < 1e-9 {
        return Err(DraftError::Invalid(
            "degenerate bisector in fillet calculation".into(),
        ));
    }
    let ub = [bx / blen, by / blen];
    let d_center = radius / sin_half;
    let center = [p_int[0] + d_center * ub[0], p_int[1] + d_center * ub[1]];

    // Arc angles from center to t1 and t2
    let ang1 = (t1[1] - center[1])
        .atan2(t1[0] - center[0])
        .to_degrees()
        .rem_euclid(360.0);
    let ang2 = (t2[1] - center[1])
        .atan2(t2[0] - center[0])
        .to_degrees()
        .rem_euclid(360.0);

    // Center to corner point bisector angle (midpoint of fillet arc curves towards corner p_int)
    let mid_ang = (p_int[1] - center[1])
        .atan2(p_int[0] - center[0])
        .to_degrees()
        .rem_euclid(360.0);

    // Determine whether arc from ang1 to ang2 or ang2 to ang1 contains mid_ang
    let diff12 = (ang2 - ang1).rem_euclid(360.0);
    let diff1m = (mid_ang - ang1).rem_euclid(360.0);

    let (start_deg, end_deg) = if diff1m < diff12 {
        (ang1, ang2)
    } else {
        (ang2, ang1)
    };

    let fillet_arc = EntityKind::Arc {
        center,
        radius,
        start_degrees: start_deg,
        end_degrees: end_deg,
    };

    let new_l1 = if orig_s1 {
        EntityKind::Line {
            start: outer1,
            end: t1,
        }
    } else {
        EntityKind::Line {
            start: t1,
            end: outer1,
        }
    };
    let new_l2 = if orig_s2 {
        EntityKind::Line {
            start: outer2,
            end: t2,
        }
    } else {
        EntityKind::Line {
            start: t2,
            end: outer2,
        }
    };

    Ok((new_l1, new_l2, Some(fillet_arc)))
}

/// Chamfers two lines with cutback distances `d1` and `d2`.
///
/// Trims both lines and creates a bevel `EntityKind::Line` connecting cutback points.
/// Returns `(trimmed_line1, trimmed_line2, bevel_line)`.
pub fn chamfer_lines(
    l1: &EntityKind,
    l2: &EntityKind,
    d1: f64,
    d2: f64,
) -> Result<(EntityKind, EntityKind, EntityKind), DraftError> {
    let (s1, e1) = match l1 {
        EntityKind::Line { start, end } => (*start, *end),
        _ => return Err(DraftError::Invalid("chamfer requires line entities".into())),
    };
    let (s2, e2) = match l2 {
        EntityKind::Line { start, end } => (*start, *end),
        _ => return Err(DraftError::Invalid("chamfer requires line entities".into())),
    };

    if !d1.is_finite() || !d2.is_finite() || d1 <= 0.0 || d2 <= 0.0 {
        return Err(DraftError::Invalid(
            "chamfer distances must be positive finite numbers".into(),
        ));
    }

    let p_int = match line_intersection(s1, e1, s2, e2) {
        Some(p) => p,
        None => {
            return Err(DraftError::Invalid(
                "lines are parallel and cannot be chamfered".into(),
            ));
        }
    };

    let d_s1 = dist_sq(s1, p_int);
    let d_e1 = dist_sq(e1, p_int);
    let (outer1, orig_s1) = if d_s1 > d_e1 { (s1, true) } else { (e1, false) };

    let d_s2 = dist_sq(s2, p_int);
    let d_e2 = dist_sq(e2, p_int);
    let (outer2, orig_s2) = if d_s2 > d_e2 { (s2, true) } else { (e2, false) };

    let len1 = (outer1[0] - p_int[0]).hypot(outer1[1] - p_int[1]);
    let len2 = (outer2[0] - p_int[0]).hypot(outer2[1] - p_int[1]);

    if d1 > len1 || d2 > len2 {
        return Err(DraftError::Invalid(format!(
            "chamfer distances ({d1}, {d2}) are larger than available line lengths ({len1:.2}, {len2:.2})"
        )));
    }

    let u1 = [(outer1[0] - p_int[0]) / len1, (outer1[1] - p_int[1]) / len1];
    let u2 = [(outer2[0] - p_int[0]) / len2, (outer2[1] - p_int[1]) / len2];

    let k1 = [p_int[0] + d1 * u1[0], p_int[1] + d1 * u1[1]];
    let k2 = [p_int[0] + d2 * u2[0], p_int[1] + d2 * u2[1]];

    let new_l1 = if orig_s1 {
        EntityKind::Line {
            start: outer1,
            end: k1,
        }
    } else {
        EntityKind::Line {
            start: k1,
            end: outer1,
        }
    };
    let new_l2 = if orig_s2 {
        EntityKind::Line {
            start: outer2,
            end: k2,
        }
    } else {
        EntityKind::Line {
            start: k2,
            end: outer2,
        }
    };
    let bevel = EntityKind::Line { start: k1, end: k2 };

    Ok((new_l1, new_l2, bevel))
}

/// Helper calculating infinite intersection point of two 2D lines.
pub fn line_intersection(
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
    p4: [f64; 2],
) -> Option<[f64; 2]> {
    let v1x = p2[0] - p1[0];
    let v1y = p2[1] - p1[1];
    let v2x = p4[0] - p3[0];
    let v2y = p4[1] - p3[1];

    let denom = v1x * v2y - v1y * v2x;
    if denom.abs() < 1e-9 {
        return None;
    }

    let dpx = p3[0] - p1[0];
    let dpy = p3[1] - p1[1];
    let t = (dpx * v2y - dpy * v2x) / denom;

    Some([p1[0] + t * v1x, p1[1] + t * v1y])
}

fn dist_sq(a: [f64; 2], b: [f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    dx * dx + dy * dy
}

/// Generates crosshatch line segments inside a closed polygon boundary.
///
/// Implements standard CAD hatch patterns:
/// - "ANSI31": parallel lines at `angle_deg` (default 45°) with `spacing` distance.
/// - "NET": crosshatch grid with lines at `angle_deg` and `angle_deg + 90°`.
///
/// Uses scanline ray-casting algorithm to compute exact intersection segments within the polygon.
pub fn generate_hatch_lines(
    points: &[[f64; 2]],
    pattern: &str,
    angle_deg: f64,
    spacing: f64,
) -> Result<Vec<[f64; 4]>, DraftError> {
    if points.len() < 3 {
        return Err(DraftError::Invalid(
            "hatch requires at least 3 points to form a closed boundary".into(),
        ));
    }
    if !spacing.is_finite() || spacing <= 0.0 {
        return Err(DraftError::Invalid(
            "hatch spacing must be a positive finite number".into(),
        ));
    }
    for p in points {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(DraftError::Invalid(
                "coordinates must be finite numbers".into(),
            ));
        }
    }

    let mut lines = Vec::new();
    let is_net = pattern.eq_ignore_ascii_case("NET");

    let generate_pass = |ang: f64| -> Vec<[f64; 4]> {
        let rad = ang.to_radians();
        let cos_a = rad.cos();
        let sin_a = rad.sin();

        // Rotate polygon points by -ang
        let rot_pts: Vec<[f64; 2]> = points
            .iter()
            .map(|p| [p[0] * cos_a + p[1] * sin_a, -p[0] * sin_a + p[1] * cos_a])
            .collect();

        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in &rot_pts {
            if p[1] < min_y {
                min_y = p[1];
            }
            if p[1] > max_y {
                max_y = p[1];
            }
        }

        let k_start = (min_y / spacing).ceil() as i64;
        let k_end = (max_y / spacing).floor() as i64;

        let n = rot_pts.len();
        let mut pass_lines = Vec::new();

        for k in k_start..=k_end {
            let y_scan = (k as f64) * spacing;
            let mut xs = Vec::new();

            for i in 0..n {
                let j = (i + 1) % n;
                let p1 = rot_pts[i];
                let p2 = rot_pts[j];

                if (p1[1] <= y_scan && y_scan < p2[1]) || (p2[1] <= y_scan && y_scan < p1[1]) {
                    let dy = p2[1] - p1[1];
                    if dy.abs() > 1e-9 {
                        let t = (y_scan - p1[1]) / dy;
                        let x = p1[0] + t * (p2[0] - p1[0]);
                        xs.push(x);
                    }
                }
            }

            xs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

            for chunk in xs.chunks_exact(2) {
                let x1 = chunk[0];
                let x2 = chunk[1];
                if (x2 - x1).abs() > 1e-6 {
                    // Rotate back by +ang
                    let orig_x1 = x1 * cos_a - y_scan * sin_a;
                    let orig_y1 = x1 * sin_a + y_scan * cos_a;
                    let orig_x2 = x2 * cos_a - y_scan * sin_a;
                    let orig_y2 = x2 * sin_a + y_scan * cos_a;
                    pass_lines.push([orig_x1, orig_y1, orig_x2, orig_y2]);
                }
            }
        }

        pass_lines
    };

    lines.extend(generate_pass(angle_deg));
    if is_net {
        lines.extend(generate_pass(angle_deg + 90.0));
    }

    Ok(lines)
}

/// Constructs a circle whose diameter is defined by two endpoints `p1` and `p2`.
pub fn circle_from_2_points(p1: [f64; 2], p2: [f64; 2]) -> Result<EntityKind, DraftError> {
    if !p1[0].is_finite() || !p1[1].is_finite() || !p2[0].is_finite() || !p2[1].is_finite() {
        return Err(DraftError::Invalid("coordinates must be finite".into()));
    }
    let dx = p2[0] - p1[0];
    let dy = p2[1] - p1[1];
    let dist = (dx * dx + dy * dy).sqrt();
    if dist < 1e-9 {
        return Err(DraftError::Invalid(
            "points must be distinct to define a circle diameter".into(),
        ));
    }
    let center = [(p1[0] + p2[0]) * 0.5, (p1[1] + p2[1]) * 0.5];
    let radius = dist * 0.5;
    Ok(EntityKind::Circle { center, radius })
}

/// Computes center and radius of a circumcircle defined by three points.
pub fn circumcircle_center_radius(
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
) -> Result<([f64; 2], f64), DraftError> {
    for p in &[p1, p2, p3] {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(DraftError::Invalid("coordinates must be finite".into()));
        }
    }
    let d = 2.0 * (p1[0] * (p2[1] - p3[1]) + p2[0] * (p3[1] - p1[1]) + p3[0] * (p1[1] - p2[1]));
    if d.abs() < 1e-9 {
        return Err(DraftError::Invalid(
            "the 3 points are collinear or coincident; cannot define a circle or arc".into(),
        ));
    }
    let p1_sq = p1[0] * p1[0] + p1[1] * p1[1];
    let p2_sq = p2[0] * p2[0] + p2[1] * p2[1];
    let p3_sq = p3[0] * p3[0] + p3[1] * p3[1];

    let ux = (p1_sq * (p2[1] - p3[1]) + p2_sq * (p3[1] - p1[1]) + p3_sq * (p1[1] - p2[1])) / d;
    let uy = (p1_sq * (p3[0] - p2[0]) + p2_sq * (p1[0] - p3[0]) + p3_sq * (p2[0] - p1[0])) / d;

    let r_sq = (p1[0] - ux) * (p1[0] - ux) + (p1[1] - uy) * (p1[1] - uy);
    let radius = r_sq.sqrt();
    if radius < 1e-9 || !radius.is_finite() {
        return Err(DraftError::Invalid("invalid circumcircle radius".into()));
    }
    Ok(([ux, uy], radius))
}

/// Constructs a circumcircle passing through three distinct, non-collinear points.
pub fn circle_from_3_points(
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
) -> Result<EntityKind, DraftError> {
    let (center, radius) = circumcircle_center_radius(p1, p2, p3)?;
    Ok(EntityKind::Circle { center, radius })
}

/// Constructs an arc starting at `p1`, passing through intermediate point `p2`, and ending at `p3`.
pub fn arc_from_3_points(
    p1: [f64; 2],
    p2: [f64; 2],
    p3: [f64; 2],
) -> Result<EntityKind, DraftError> {
    let (center, radius) = circumcircle_center_radius(p1, p2, p3)?;
    let theta1 = (p1[1] - center[1])
        .atan2(p1[0] - center[0])
        .to_degrees()
        .rem_euclid(360.0);
    let theta2 = (p2[1] - center[1])
        .atan2(p2[0] - center[0])
        .to_degrees()
        .rem_euclid(360.0);
    let theta3 = (p3[1] - center[1])
        .atan2(p3[0] - center[0])
        .to_degrees()
        .rem_euclid(360.0);

    let d_theta2 = (theta2 - theta1).rem_euclid(360.0);
    let d_theta3 = (theta3 - theta1).rem_euclid(360.0);

    if d_theta2 < 1e-6 || d_theta3 < 1e-6 || (d_theta3 - d_theta2).abs() < 1e-6 {
        return Err(DraftError::Invalid(
            "the 3 points must be distinct and non-overlapping".into(),
        ));
    }

    // In DXF / 2D drafting, arcs sweep counter-clockwise from start_degrees to end_degrees.
    let (start_degrees, end_degrees) = if d_theta2 < d_theta3 {
        // Traversal p1 -> p2 -> p3 is counter-clockwise
        (theta1, theta3)
    } else {
        // Traversal p1 -> p2 -> p3 is clockwise; in CCW representation, sweeps from p3 to p1
        (theta3, theta1)
    };

    Ok(EntityKind::Arc {
        center,
        radius,
        start_degrees,
        end_degrees,
    })
}

/// Constructs a regular polygon with `sides` vertices.
///
/// If `inscribed` is true, vertices lie on a circle of `radius`.
/// If `inscribed` is false (circumscribed), the polygon's sides are tangent to a circle of `radius`.
pub fn regular_polygon(
    center: [f64; 2],
    radius: f64,
    sides: usize,
    inscribed: bool,
    start_angle_deg: f64,
) -> Result<EntityKind, DraftError> {
    if !center[0].is_finite()
        || !center[1].is_finite()
        || !radius.is_finite()
        || !start_angle_deg.is_finite()
    {
        return Err(DraftError::Invalid(
            "polygon parameters must be finite".into(),
        ));
    }
    if radius <= 1e-9 {
        return Err(DraftError::Invalid(
            "polygon radius must be greater than zero".into(),
        ));
    }
    if sides < 3 {
        return Err(DraftError::Invalid(
            "a polygon requires at least 3 sides".into(),
        ));
    }

    let r_vert = if inscribed {
        radius
    } else {
        let half_angle = std::f64::consts::PI / (sides as f64);
        let cos_val = half_angle.cos();
        if cos_val.abs() < 1e-9 {
            return Err(DraftError::Invalid("invalid polygon geometry".into()));
        }
        radius / cos_val
    };

    let start_rad = start_angle_deg.to_radians();
    let step = 2.0 * std::f64::consts::PI / (sides as f64);
    let mut points = Vec::with_capacity(sides);

    for i in 0..sides {
        let ang = start_rad + (i as f64) * step;
        let x = center[0] + r_vert * ang.cos();
        let y = center[1] + r_vert * ang.sin();
        points.push([x, y]);
    }

    Ok(EntityKind::Polyline {
        points,
        closed: true,
    })
}

/// Combines a list of line segments and open polylines into a single connected Polyline.
///
/// Chaining automatically connects touching endpoints within `tolerance`.
/// If the first and last vertices coincide, the resulting polyline is marked `closed: true`.
pub fn join_entities(entities: &[EntityKind], tolerance: f64) -> Result<EntityKind, DraftError> {
    if entities.is_empty() {
        return Err(DraftError::Invalid("no entities provided to join".into()));
    }
    if tolerance <= 0.0 || !tolerance.is_finite() {
        return Err(DraftError::Invalid(
            "join tolerance must be positive finite".into(),
        ));
    }

    // Convert each entity to a sequence of vertices [v0, v1, ...]
    let mut chains: Vec<Vec<[f64; 2]>> = Vec::new();
    for ent in entities {
        match ent {
            EntityKind::Line { start, end } => {
                chains.push(vec![*start, *end]);
            }
            EntityKind::Polyline { points, closed } => {
                if *closed {
                    return Err(DraftError::Invalid(
                        "cannot join an already closed polyline; select open lines or polylines"
                            .into(),
                    ));
                }
                if points.len() < 2 {
                    return Err(DraftError::Invalid(
                        "polyline must have at least 2 points".into(),
                    ));
                }
                chains.push(points.clone());
            }
            EntityKind::Rectangle {
                origin,
                width,
                height,
            } => {
                let [x, y] = origin;
                chains.push(vec![
                    *origin,
                    [x + width, *y],
                    [x + width, y + height],
                    [*x, y + height],
                    *origin,
                ]);
            }
            _ => {
                return Err(DraftError::Invalid(
                    "JOIN only supports Line, Polyline, and Rectangle entities".into(),
                ));
            }
        }
    }

    if chains.len() == 1 {
        let pts = chains.remove(0);
        let closed = pts.len() >= 3 && dist_sq(pts[0], pts[pts.len() - 1]).sqrt() <= tolerance;
        let mut final_pts = pts;
        if closed {
            final_pts.pop();
        }
        return Ok(EntityKind::Polyline {
            points: final_pts,
            closed,
        });
    }

    let tol_sq = tolerance * tolerance;
    let mut current = chains.remove(0);

    while !chains.is_empty() {
        let head = current[0];
        let tail = current[current.len() - 1];
        let mut found_idx = None;
        let mut match_type = 0; // 1: tail to seg.start, 2: tail to seg.end, 3: head to seg.end, 4: head to seg.start

        for (i, seg) in chains.iter().enumerate() {
            let seg_start = seg[0];
            let seg_end = seg[seg.len() - 1];

            if dist_sq(tail, seg_start) <= tol_sq {
                found_idx = Some(i);
                match_type = 1;
                break;
            } else if dist_sq(tail, seg_end) <= tol_sq {
                found_idx = Some(i);
                match_type = 2;
                break;
            } else if dist_sq(head, seg_end) <= tol_sq {
                found_idx = Some(i);
                match_type = 3;
                break;
            } else if dist_sq(head, seg_start) <= tol_sq {
                found_idx = Some(i);
                match_type = 4;
                break;
            }
        }

        if let Some(idx) = found_idx {
            let seg = chains.remove(idx);
            match match_type {
                1 => {
                    // Append seg[1..] to current
                    current.extend_from_slice(&seg[1..]);
                }
                2 => {
                    // Append reversed seg[..seg.len()-1] to current
                    for pt in seg[..seg.len() - 1].iter().rev() {
                        current.push(*pt);
                    }
                }
                3 => {
                    // Prepend seg[..seg.len()-1] to current
                    let mut new_chain = seg;
                    new_chain.pop(); // remove duplicate endpoint
                    new_chain.extend(current);
                    current = new_chain;
                }
                4 => {
                    // Prepend reversed seg[1..] to current
                    let mut new_chain: Vec<[f64; 2]> = seg[1..].iter().rev().copied().collect();
                    new_chain.extend(current);
                    current = new_chain;
                }
                _ => {}
            }
        } else {
            return Err(DraftError::Invalid(
                "selected entities do not form a single contiguous path; gaps detected".into(),
            ));
        }
    }

    // Check if the joined path is closed
    let is_closed = current.len() >= 3 && dist_sq(current[0], current[current.len() - 1]) <= tol_sq;
    if is_closed {
        current.pop(); // remove redundant closure point
    }

    Ok(EntityKind::Polyline {
        points: current,
        closed: is_closed,
    })
}

/// Creates a 2D rectangular array of copies of `entities`.
pub fn rectangular_array_entities(
    entities: &[EntityKind],
    rows: usize,
    cols: usize,
    row_spacing: f64,
    col_spacing: f64,
) -> Result<Vec<EntityKind>, DraftError> {
    if rows == 0 || cols == 0 {
        return Err(DraftError::Invalid(
            "rows and columns must be at least 1".into(),
        ));
    }
    if !row_spacing.is_finite() || !col_spacing.is_finite() {
        return Err(DraftError::Invalid("array spacing must be finite".into()));
    }
    let mut copies = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            if r == 0 && c == 0 {
                continue; // Original entity is at (0, 0)
            }
            let dx = (c as f64) * col_spacing;
            let dy = (r as f64) * row_spacing;
            for ent in entities {
                copies.push(translate_entity(ent, dx, dy)?);
            }
        }
    }
    Ok(copies)
}

/// Creates a polar (circular) array of copies of `entities` distributed around `center`.
pub fn polar_array_entities(
    entities: &[EntityKind],
    count: usize,
    center: [f64; 2],
    fill_angle_deg: f64,
    rotate_items: bool,
) -> Result<Vec<EntityKind>, DraftError> {
    if count < 2 {
        return Err(DraftError::Invalid(
            "polar array count must be at least 2".into(),
        ));
    }
    if !fill_angle_deg.is_finite() || fill_angle_deg.abs() < 1e-6 {
        return Err(DraftError::Invalid(
            "fill angle must be a non-zero finite number".into(),
        ));
    }
    if !center[0].is_finite() || !center[1].is_finite() {
        return Err(DraftError::Invalid(
            "center coordinates must be finite".into(),
        ));
    }

    let is_full_circle = (fill_angle_deg.abs() - 360.0).abs() < 1e-6;
    let step = if is_full_circle {
        fill_angle_deg / (count as f64)
    } else {
        fill_angle_deg / ((count - 1) as f64)
    };

    let mut copies = Vec::new();
    for i in 1..count {
        let angle = (i as f64) * step;
        if rotate_items {
            for ent in entities {
                copies.push(rotate_entity(ent, center, angle)?);
            }
        } else {
            let rad = angle.to_radians();
            let cos_a = rad.cos();
            let sin_a = rad.sin();
            for ent in entities {
                let anchor = match ent {
                    EntityKind::Point { at } => *at,
                    EntityKind::Line { start, end } => {
                        [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5]
                    }
                    EntityKind::Circle { center: c, .. } => *c,
                    EntityKind::Arc { center: c, .. } => *c,
                    EntityKind::Polyline { points, .. } => {
                        let sx: f64 = points.iter().map(|p| p[0]).sum();
                        let sy: f64 = points.iter().map(|p| p[1]).sum();
                        [sx / points.len() as f64, sy / points.len() as f64]
                    }
                    EntityKind::Rectangle {
                        origin,
                        width,
                        height,
                    } => [origin[0] + width * 0.5, origin[1] + height * 0.5],
                    EntityKind::Ellipse { center: c, .. } => *c,
                    EntityKind::Text { at, .. } => *at,
                    EntityKind::Dimension { start, end, .. } => {
                        [(start[0] + end[0]) * 0.5, (start[1] + end[1]) * 0.5]
                    }
                    EntityKind::Spline { points, .. } => {
                        let sx: f64 = points.iter().map(|p| p[0]).sum();
                        let sy: f64 = points.iter().map(|p| p[1]).sum();
                        [sx / points.len() as f64, sy / points.len() as f64]
                    }
                };
                let rel_x = anchor[0] - center[0];
                let rel_y = anchor[1] - center[1];
                let rot_x = center[0] + rel_x * cos_a - rel_y * sin_a;
                let rot_y = center[1] + rel_x * sin_a + rel_y * cos_a;
                let dx = rot_x - anchor[0];
                let dy = rot_y - anchor[1];
                copies.push(translate_entity(ent, dx, dy)?);
            }
        }
    }
    Ok(copies)
}

/// Computes comprehensive cross-sectional properties for structural and mechanical engineering:
/// Area, centroid, moments of inertia (Ixx, Iyy, Ixy, J), principal axes (I1, I2, theta_p),
/// radii of gyration (rx, ry), and elastic section moduli (Sx, Sy).
pub fn analyze_cross_section(points: &[[f64; 2]]) -> Result<SectionProperties, DraftError> {
    if points.len() < 3 {
        return Err(DraftError::Invalid(
            "cross section requires at least 3 points to form a polygon".into(),
        ));
    }
    for p in points {
        if !p[0].is_finite() || !p[1].is_finite() {
            return Err(DraftError::Invalid(
                "coordinates must be finite numbers".into(),
            ));
        }
    }

    let n = points.len();
    let mut double_area = 0.0;
    let mut perimeter = 0.0;
    let mut cx_sum = 0.0;
    let mut cy_sum = 0.0;
    let mut ixx_0 = 0.0;
    let mut iyy_0 = 0.0;
    let mut ixy_0 = 0.0;

    for i in 0..n {
        let j = (i + 1) % n;
        let x0 = points[i][0];
        let y0 = points[i][1];
        let x1 = points[j][0];
        let y1 = points[j][1];

        let cross = x0 * y1 - x1 * y0;
        double_area += cross;
        cx_sum += (x0 + x1) * cross;
        cy_sum += (y0 + y1) * cross;

        // Second moments about origin
        ixx_0 += (y0 * y0 + y0 * y1 + y1 * y1) * cross;
        iyy_0 += (x0 * x0 + x0 * x1 + x1 * x1) * cross;
        ixy_0 += (x0 * y1 + 2.0 * x0 * y0 + 2.0 * x1 * y1 + x1 * y0) * cross;

        let dx = x1 - x0;
        let dy = y1 - y0;
        perimeter += (dx * dx + dy * dy).sqrt();
    }

    let area = (double_area.abs()) * 0.5;
    if area < 1e-12 {
        return Err(DraftError::Invalid(
            "polygon has zero or negligible area".into(),
        ));
    }

    // Centroid
    let factor = 1.0 / (3.0 * double_area);
    let centroid = [cx_sum * factor, cy_sum * factor];

    // Normalize origin second moments
    ixx_0 = ixx_0.abs() / 12.0;
    iyy_0 = iyy_0.abs() / 12.0;
    ixy_0 /= 24.0;
    if double_area < 0.0 {
        ixy_0 = -ixy_0;
    }

    // Parallel Axis Theorem: transfer to centroid
    let cx = centroid[0];
    let cy = centroid[1];
    let i_xx = (ixx_0 - area * cy * cy).max(0.0);
    let i_yy = (iyy_0 - area * cx * cx).max(0.0);
    let i_xy = ixy_0 - area * cx * cy;
    let j_polar = i_xx + i_yy;

    // Principal moments of inertia
    let i_avg = (i_xx + i_yy) * 0.5;
    let diff = (i_xx - i_yy) * 0.5;
    let rad = (diff * diff + i_xy * i_xy).sqrt();
    let i_1 = i_avg + rad;
    let i_2 = (i_avg - rad).max(0.0);
    let principal_angle_deg = 0.5 * (-2.0 * i_xy).atan2(i_xx - i_yy).to_degrees();

    // Radii of gyration
    let r_x = (i_xx / area).sqrt();
    let r_y = (i_yy / area).sqrt();

    // Section moduli Sx, Sy
    let mut min_x = f64::INFINITY;
    let mut max_x = f64::NEG_INFINITY;
    let mut min_y = f64::INFINITY;
    let mut max_y = f64::NEG_INFINITY;
    for p in points {
        if p[0] < min_x {
            min_x = p[0];
        }
        if p[0] > max_x {
            max_x = p[0];
        }
        if p[1] < min_y {
            min_y = p[1];
        }
        if p[1] > max_y {
            max_y = p[1];
        }
    }
    let c_y = (max_y - cy).abs().max((min_y - cy).abs());
    let c_x = (max_x - cx).abs().max((min_x - cx).abs());

    let s_x = if c_y > 1e-9 { i_xx / c_y } else { 0.0 };
    let s_y = if c_x > 1e-9 { i_yy / c_x } else { 0.0 };

    Ok(SectionProperties {
        area,
        perimeter,
        centroid,
        i_xx,
        i_yy,
        i_xy,
        j_polar,
        i_1,
        i_2,
        principal_angle_deg,
        r_x,
        r_y,
        s_x,
        s_y,
    })
}

/// Computes analytical cross-sectional properties for a circular section.
pub fn analyze_circle_section(
    center: [f64; 2],
    radius: f64,
) -> Result<SectionProperties, DraftError> {
    if !center[0].is_finite() || !center[1].is_finite() || !radius.is_finite() || radius <= 1e-9 {
        return Err(DraftError::Invalid(
            "valid center and positive radius required".into(),
        ));
    }
    let pi = std::f64::consts::PI;
    let area = pi * radius * radius;
    let perimeter = 2.0 * pi * radius;
    let i_val = (pi * radius.powi(4)) / 4.0;
    let j_polar = (pi * radius.powi(4)) / 2.0;
    let s_val = (pi * radius.powi(3)) / 4.0;
    let r_val = radius * 0.5;

    Ok(SectionProperties {
        area,
        perimeter,
        centroid: center,
        i_xx: i_val,
        i_yy: i_val,
        i_xy: 0.0,
        j_polar,
        i_1: i_val,
        i_2: i_val,
        principal_angle_deg: 0.0,
        r_x: r_val,
        r_y: r_val,
        s_x: s_val,
        s_y: s_val,
    })
}

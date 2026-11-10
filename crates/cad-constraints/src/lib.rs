//! 2D Parametric Geometric & Dimensional Constraint Engine.
//!
//! Provides pure Rust zero-unsafe constraint modeling and solving:
//! - Geometry: Points, Lines, Circles
//! - Constraints: Coincident, Concentric, Horizontal, Vertical, Parallel, Perpendicular,
//!   Tangent, EqualLength, EqualRadius, Distance, PointLineDistance, Angle, FixPoint,
//!   PointOnLine, PointOnCircle, FixLength, FixRadius
//! - Damped least squares / Levenberg-Marquardt solver with weighted minimum-norm updates
//! - Degrees-of-Freedom (DOF) analysis and over-constrained / conflicting diagnostics.

#![forbid(unsafe_code)]
#![deny(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented,
    clippy::todo,
    clippy::unreachable
)]

pub mod solver;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Strongly-typed identifier for points.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PointId(pub usize);

/// Strongly-typed identifier for lines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct LineId(pub usize);

/// Strongly-typed identifier for circles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct CircleId(pub usize);

/// Strongly-typed identifier for constraints.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct ConstraintId(pub usize);

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Point {
    pub id: PointId,
    pub x: f64,
    pub y: f64,
    pub fixed: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Line {
    pub id: LineId,
    pub p1: PointId,
    pub p2: PointId,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Circle {
    pub id: CircleId,
    pub center: PointId,
    pub radius: f64,
    pub fixed_radius: bool,
}

/// Geometric and dimensional constraint specifications.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ConstraintKind {
    /// Two points coincide: $p_1 = p_2$.
    Coincident(PointId, PointId),
    /// Point lies on line segment or its extension.
    PointOnLine(PointId, LineId),
    /// Point lies on circle circumference.
    PointOnCircle(PointId, CircleId),
    /// Line segment is horizontal: $y_1 = y_2$.
    Horizontal(LineId),
    /// Line segment is vertical: $x_1 = x_2$.
    Vertical(LineId),
    /// Two lines are parallel: $(p_2 - p_1) \times (q_2 - q_1) = 0$.
    Parallel(LineId, LineId),
    /// Two lines are perpendicular: $(p_2 - p_1) \cdot (q_2 - q_1) = 0$.
    Perpendicular(LineId, LineId),
    /// Line is tangent to circle: perpendicular distance from center to line equals radius.
    Tangent(LineId, CircleId),
    /// Two circles are tangent: distance between centers equals sum of radii.
    TangentCircles(CircleId, CircleId),
    /// Two circles share center: $c_1 = c_2$.
    Concentric(CircleId, CircleId),
    /// Two lines have equal length.
    EqualLength(LineId, LineId),
    /// Two circles have equal radius.
    EqualRadius(CircleId, CircleId),
    /// Distance between two points equals `distance`.
    Distance(PointId, PointId, f64),
    /// Distance between a point and a line equals `distance`.
    PointLineDistance(PointId, LineId, f64),
    /// Angle between two lines in radians equals `angle_rad`.
    Angle(LineId, LineId, f64),
    /// Lock point position to exact coordinates `[x, y]`.
    FixPoint(PointId, [f64; 2]),
    /// Lock line length to exact value.
    FixLength(LineId, f64),
    /// Lock circle radius to exact value.
    FixRadius(CircleId, f64),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Constraint {
    pub id: ConstraintId,
    pub kind: ConstraintKind,
}

/// Degrees of freedom status of a sketch system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DofStatus {
    Unconstrained,
    Under,
    Fully,
    Over,
    Inconsistent,
}

/// Degrees of freedom and constraint diagnostics report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DofReport {
    pub total_dof: usize,
    pub rank: usize,
    pub free_parameters: usize,
    pub status: DofStatus,
    pub redundant_constraints: Vec<ConstraintId>,
    pub conflicting_constraints: Vec<ConstraintId>,
}

/// Solver configuration options.
#[derive(Debug, Clone)]
pub struct SolveOptions {
    pub max_iterations: usize,
    pub tolerance: f64,
}

impl Default for SolveOptions {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-8,
        }
    }
}

/// Result report from a solve attempt.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolveReport {
    pub iterations: usize,
    pub final_residual: f64,
    pub converged: bool,
}

#[derive(Debug, Error, PartialEq, Clone)]
pub enum ConstraintError {
    #[error("point #{0:?} not found")]
    PointNotFound(PointId),
    #[error("line #{0:?} not found")]
    LineNotFound(LineId),
    #[error("circle #{0:?} not found")]
    CircleNotFound(CircleId),
    #[error("constraint #{0:?} not found")]
    ConstraintNotFound(ConstraintId),
    #[error("system did not converge (residual = {final_residual:e})")]
    DidNotConverge { final_residual: f64 },
    #[error("constraints conflict: {0:?}")]
    Conflict(Vec<ConstraintId>),
    #[error("system is over-constrained with redundant constraints: {0:?}")]
    OverConstrained(Vec<ConstraintId>),
    #[error("invalid parameter: {0}")]
    Invalid(String),
}

/// 2D Parametric Sketch Constraint System.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SketchSystem {
    points: Vec<Point>,
    lines: Vec<Line>,
    circles: Vec<Circle>,
    constraints: Vec<Constraint>,
    next_point_id: usize,
    next_line_id: usize,
    next_circle_id: usize,
    next_constraint_id: usize,
}

impl SketchSystem {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn points(&self) -> &[Point] {
        &self.points
    }

    pub fn lines(&self) -> &[Line] {
        &self.lines
    }

    pub fn circles(&self) -> &[Circle] {
        &self.circles
    }

    pub fn constraints(&self) -> &[Constraint] {
        &self.constraints
    }

    pub fn point(&self, id: PointId) -> Option<&Point> {
        self.points.iter().find(|p| p.id == id)
    }

    pub fn line(&self, id: LineId) -> Option<&Line> {
        self.lines.iter().find(|l| l.id == id)
    }

    pub fn circle(&self, id: CircleId) -> Option<&Circle> {
        self.circles.iter().find(|c| c.id == id)
    }

    pub fn constraint(&self, id: ConstraintId) -> Option<&Constraint> {
        self.constraints.iter().find(|c| c.id == id)
    }

    pub fn add_point(&mut self, x: f64, y: f64) -> PointId {
        let id = PointId(self.next_point_id);
        self.next_point_id += 1;
        self.points.push(Point {
            id,
            x,
            y,
            fixed: false,
        });
        id
    }

    pub fn add_line(&mut self, p1: PointId, p2: PointId) -> Result<LineId, ConstraintError> {
        if self.point(p1).is_none() {
            return Err(ConstraintError::PointNotFound(p1));
        }
        if self.point(p2).is_none() {
            return Err(ConstraintError::PointNotFound(p2));
        }
        if p1 == p2 {
            return Err(ConstraintError::Invalid(
                "line endpoints must be distinct".into(),
            ));
        }
        let id = LineId(self.next_line_id);
        self.next_line_id += 1;
        self.lines.push(Line { id, p1, p2 });
        Ok(id)
    }

    pub fn add_circle(
        &mut self,
        center: PointId,
        radius: f64,
    ) -> Result<CircleId, ConstraintError> {
        if self.point(center).is_none() {
            return Err(ConstraintError::PointNotFound(center));
        }
        if radius <= 0.0 || !radius.is_finite() {
            return Err(ConstraintError::Invalid(
                "circle radius must be positive and finite".into(),
            ));
        }
        let id = CircleId(self.next_circle_id);
        self.next_circle_id += 1;
        self.circles.push(Circle {
            id,
            center,
            radius,
            fixed_radius: false,
        });
        Ok(id)
    }

    pub fn add_constraint(
        &mut self,
        kind: ConstraintKind,
    ) -> Result<ConstraintId, ConstraintError> {
        self.validate_constraint(&kind)?;
        let id = ConstraintId(self.next_constraint_id);
        self.next_constraint_id += 1;
        self.constraints.push(Constraint { id, kind });
        Ok(id)
    }

    pub fn remove_constraint(&mut self, id: ConstraintId) -> Result<(), ConstraintError> {
        let pos = self
            .constraints
            .iter()
            .position(|c| c.id == id)
            .ok_or(ConstraintError::ConstraintNotFound(id))?;
        self.constraints.remove(pos);
        Ok(())
    }

    fn validate_constraint(&self, kind: &ConstraintKind) -> Result<(), ConstraintError> {
        match kind {
            ConstraintKind::Coincident(p1, p2) | ConstraintKind::Distance(p1, p2, _) => {
                if self.point(*p1).is_none() {
                    return Err(ConstraintError::PointNotFound(*p1));
                }
                if self.point(*p2).is_none() {
                    return Err(ConstraintError::PointNotFound(*p2));
                }
            }
            ConstraintKind::PointOnLine(p, l) | ConstraintKind::PointLineDistance(p, l, _) => {
                if self.point(*p).is_none() {
                    return Err(ConstraintError::PointNotFound(*p));
                }
                if self.line(*l).is_none() {
                    return Err(ConstraintError::LineNotFound(*l));
                }
            }
            ConstraintKind::PointOnCircle(p, c) => {
                if self.point(*p).is_none() {
                    return Err(ConstraintError::PointNotFound(*p));
                }
                if self.circle(*c).is_none() {
                    return Err(ConstraintError::CircleNotFound(*c));
                }
            }
            ConstraintKind::Horizontal(l)
            | ConstraintKind::Vertical(l)
            | ConstraintKind::FixLength(l, _) => {
                if self.line(*l).is_none() {
                    return Err(ConstraintError::LineNotFound(*l));
                }
            }
            ConstraintKind::Parallel(l1, l2)
            | ConstraintKind::Perpendicular(l1, l2)
            | ConstraintKind::EqualLength(l1, l2)
            | ConstraintKind::Angle(l1, l2, _) => {
                if self.line(*l1).is_none() {
                    return Err(ConstraintError::LineNotFound(*l1));
                }
                if self.line(*l2).is_none() {
                    return Err(ConstraintError::LineNotFound(*l2));
                }
            }
            ConstraintKind::Tangent(l, c) => {
                if self.line(*l).is_none() {
                    return Err(ConstraintError::LineNotFound(*l));
                }
                if self.circle(*c).is_none() {
                    return Err(ConstraintError::CircleNotFound(*c));
                }
            }
            ConstraintKind::TangentCircles(c1, c2)
            | ConstraintKind::Concentric(c1, c2)
            | ConstraintKind::EqualRadius(c1, c2) => {
                if self.circle(*c1).is_none() {
                    return Err(ConstraintError::CircleNotFound(*c1));
                }
                if self.circle(*c2).is_none() {
                    return Err(ConstraintError::CircleNotFound(*c2));
                }
            }
            ConstraintKind::FixPoint(p, _) => {
                if self.point(*p).is_none() {
                    return Err(ConstraintError::PointNotFound(*p));
                }
            }
            ConstraintKind::FixRadius(c, _) => {
                if self.circle(*c).is_none() {
                    return Err(ConstraintError::CircleNotFound(*c));
                }
            }
        }
        Ok(())
    }

    /// Extract flat parameter vector:
    /// Each point contributes 2 parameters (x, y) at index `2 * i`, `2 * i + 1`.
    /// Each circle contributes 1 radius parameter at index `2 * num_points + j`.
    fn extract_parameters(&self) -> (Vec<f64>, Vec<usize>, Vec<f64>) {
        let num_pts = self.points.len();
        let num_circles = self.circles.len();
        let mut x = Vec::with_capacity(2 * num_pts + num_circles);
        let mut free = Vec::new();
        let mut weights = Vec::new();

        for (i, p) in self.points.iter().enumerate() {
            x.push(p.x);
            x.push(p.y);
            if !p.fixed {
                free.push(2 * i);
                free.push(2 * i + 1);
            }
            weights.push(1.0);
            weights.push(1.0);
        }

        for (j, c) in self.circles.iter().enumerate() {
            x.push(c.radius);
            let idx = 2 * num_pts + j;
            if !c.fixed_radius {
                free.push(idx);
            }
            weights.push(1.0);
        }

        (x, free, weights)
    }

    /// Update entity parameters from flat parameter vector.
    fn write_back_parameters(&mut self, x: &[f64]) {
        let num_pts = self.points.len();
        for (i, p) in self.points.iter_mut().enumerate() {
            if let (Some(&px), Some(&py)) = (x.get(2 * i), x.get(2 * i + 1)) {
                p.x = px;
                p.y = py;
            }
        }
        for (j, c) in self.circles.iter_mut().enumerate() {
            if let Some(&r) = x.get(2 * num_pts + j) {
                c.radius = r.abs();
            }
        }
    }

    /// Helper to look up point coordinates from parameter vector.
    fn get_point_xy(&self, p_id: PointId, x: &[f64]) -> Option<[f64; 2]> {
        let idx = self.points.iter().position(|p| p.id == p_id)?;
        let px = *x.get(2 * idx)?;
        let py = *x.get(2 * idx + 1)?;
        Some([px, py])
    }

    /// Helper to look up circle radius from parameter vector.
    fn get_circle_r(&self, c_id: CircleId, x: &[f64]) -> Option<f64> {
        let idx = self.circles.iter().position(|c| c.id == c_id)?;
        let num_pts = self.points.len();
        Some(*x.get(2 * num_pts + idx)?)
    }

    /// Evaluates residuals for active constraints.
    pub fn evaluate_residuals(
        &self,
        x: &[f64],
        active_constraints: &[Constraint],
        out: &mut Vec<f64>,
    ) {
        for c in active_constraints {
            match &c.kind {
                ConstraintKind::Coincident(p1, p2) => {
                    if let (Some(a), Some(b)) =
                        (self.get_point_xy(*p1, x), self.get_point_xy(*p2, x))
                    {
                        out.push(a[0] - b[0]);
                        out.push(a[1] - b[1]);
                    }
                }
                ConstraintKind::PointOnLine(p, line_id) => {
                    if let Some(line) = self.line(*line_id)
                        && let (Some(pt), Some(a), Some(b)) = (
                            self.get_point_xy(*p, x),
                            self.get_point_xy(line.p1, x),
                            self.get_point_xy(line.p2, x),
                        )
                    {
                        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                        let len = dx.hypot(dy).max(1e-9);
                        let dist = ((pt[0] - a[0]) * dy - (pt[1] - a[1]) * dx) / len;
                        out.push(dist);
                    }
                }
                ConstraintKind::PointOnCircle(p, c_id) => {
                    if let Some(c) = self.circle(*c_id)
                        && let (Some(pt), Some(center), Some(r)) = (
                            self.get_point_xy(*p, x),
                            self.get_point_xy(c.center, x),
                            self.get_circle_r(*c_id, x),
                        )
                    {
                        let d = (pt[0] - center[0]).hypot(pt[1] - center[1]);
                        out.push(d - r.abs());
                    }
                }
                ConstraintKind::Horizontal(l_id) => {
                    if let Some(line) = self.line(*l_id)
                        && let (Some(a), Some(b)) =
                            (self.get_point_xy(line.p1, x), self.get_point_xy(line.p2, x))
                    {
                        out.push(b[1] - a[1]);
                    }
                }
                ConstraintKind::Vertical(l_id) => {
                    if let Some(line) = self.line(*l_id)
                        && let (Some(a), Some(b)) =
                            (self.get_point_xy(line.p1, x), self.get_point_xy(line.p2, x))
                    {
                        out.push(b[0] - a[0]);
                    }
                }
                ConstraintKind::Parallel(l1_id, l2_id) => {
                    if let (Some(l1), Some(l2)) = (self.line(*l1_id), self.line(*l2_id))
                        && let (Some(a1), Some(b1), Some(a2), Some(b2)) = (
                            self.get_point_xy(l1.p1, x),
                            self.get_point_xy(l1.p2, x),
                            self.get_point_xy(l2.p1, x),
                            self.get_point_xy(l2.p2, x),
                        )
                    {
                        let (dx1, dy1) = (b1[0] - a1[0], b1[1] - a1[1]);
                        let (dx2, dy2) = (b2[0] - a2[0], b2[1] - a2[1]);
                        let len1 = dx1.hypot(dy1).max(1e-9);
                        let len2 = dx2.hypot(dy2).max(1e-9);
                        let cross = (dx1 * dy2 - dy1 * dx2) / (len1 * len2);
                        out.push(cross);
                    }
                }
                ConstraintKind::Perpendicular(l1_id, l2_id) => {
                    if let (Some(l1), Some(l2)) = (self.line(*l1_id), self.line(*l2_id))
                        && let (Some(a1), Some(b1), Some(a2), Some(b2)) = (
                            self.get_point_xy(l1.p1, x),
                            self.get_point_xy(l1.p2, x),
                            self.get_point_xy(l2.p1, x),
                            self.get_point_xy(l2.p2, x),
                        )
                    {
                        let (dx1, dy1) = (b1[0] - a1[0], b1[1] - a1[1]);
                        let (dx2, dy2) = (b2[0] - a2[0], b2[1] - a2[1]);
                        let len1 = dx1.hypot(dy1).max(1e-9);
                        let len2 = dx2.hypot(dy2).max(1e-9);
                        let dot = (dx1 * dx2 + dy1 * dy2) / (len1 * len2);
                        out.push(dot);
                    }
                }
                ConstraintKind::Tangent(l_id, c_id) => {
                    if let (Some(line), Some(circ)) = (self.line(*l_id), self.circle(*c_id))
                        && let (Some(a), Some(b), Some(c), Some(r)) = (
                            self.get_point_xy(line.p1, x),
                            self.get_point_xy(line.p2, x),
                            self.get_point_xy(circ.center, x),
                            self.get_circle_r(*c_id, x),
                        )
                    {
                        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                        let len = dx.hypot(dy).max(1e-9);
                        let dist = ((c[0] - a[0]) * dy - (c[1] - a[1]) * dx).abs() / len;
                        out.push(dist - r.abs());
                    }
                }
                ConstraintKind::TangentCircles(c1_id, c2_id) => {
                    if let (Some(c1), Some(c2)) = (self.circle(*c1_id), self.circle(*c2_id))
                        && let (Some(center1), Some(center2), Some(r1), Some(r2)) = (
                            self.get_point_xy(c1.center, x),
                            self.get_point_xy(c2.center, x),
                            self.get_circle_r(*c1_id, x),
                            self.get_circle_r(*c2_id, x),
                        )
                    {
                        let d = (center2[0] - center1[0]).hypot(center2[1] - center1[1]);
                        out.push(d - (r1.abs() + r2.abs()));
                    }
                }
                ConstraintKind::Concentric(c1_id, c2_id) => {
                    if let (Some(c1), Some(c2)) = (self.circle(*c1_id), self.circle(*c2_id))
                        && let (Some(p1), Some(p2)) = (
                            self.get_point_xy(c1.center, x),
                            self.get_point_xy(c2.center, x),
                        )
                    {
                        out.push(p1[0] - p2[0]);
                        out.push(p1[1] - p2[1]);
                    }
                }
                ConstraintKind::EqualLength(l1_id, l2_id) => {
                    if let (Some(l1), Some(l2)) = (self.line(*l1_id), self.line(*l2_id))
                        && let (Some(a1), Some(b1), Some(a2), Some(b2)) = (
                            self.get_point_xy(l1.p1, x),
                            self.get_point_xy(l1.p2, x),
                            self.get_point_xy(l2.p1, x),
                            self.get_point_xy(l2.p2, x),
                        )
                    {
                        let len1 = (b1[0] - a1[0]).hypot(b1[1] - a1[1]);
                        let len2 = (b2[0] - a2[0]).hypot(b2[1] - a2[1]);
                        out.push(len1 - len2);
                    }
                }
                ConstraintKind::EqualRadius(c1_id, c2_id) => {
                    if let (Some(r1), Some(r2)) =
                        (self.get_circle_r(*c1_id, x), self.get_circle_r(*c2_id, x))
                    {
                        out.push(r1.abs() - r2.abs());
                    }
                }
                ConstraintKind::Distance(p1, p2, target_dist) => {
                    if let (Some(a), Some(b)) =
                        (self.get_point_xy(*p1, x), self.get_point_xy(*p2, x))
                    {
                        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
                        out.push(d - *target_dist);
                    }
                }
                ConstraintKind::PointLineDistance(p, l_id, target_dist) => {
                    if let Some(line) = self.line(*l_id)
                        && let (Some(pt), Some(a), Some(b)) = (
                            self.get_point_xy(*p, x),
                            self.get_point_xy(line.p1, x),
                            self.get_point_xy(line.p2, x),
                        )
                    {
                        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                        let len = dx.hypot(dy).max(1e-9);
                        let dist = ((pt[0] - a[0]) * dy - (pt[1] - a[1]) * dx).abs() / len;
                        out.push(dist - *target_dist);
                    }
                }
                ConstraintKind::Angle(l1_id, l2_id, target_angle_rad) => {
                    if let (Some(l1), Some(l2)) = (self.line(*l1_id), self.line(*l2_id))
                        && let (Some(a1), Some(b1), Some(a2), Some(b2)) = (
                            self.get_point_xy(l1.p1, x),
                            self.get_point_xy(l1.p2, x),
                            self.get_point_xy(l2.p1, x),
                            self.get_point_xy(l2.p2, x),
                        )
                    {
                        let (dx1, dy1) = (b1[0] - a1[0], b1[1] - a1[1]);
                        let (dx2, dy2) = (b2[0] - a2[0], b2[1] - a2[1]);
                        let len1 = dx1.hypot(dy1).max(1e-9);
                        let len2 = dx2.hypot(dy2).max(1e-9);
                        let dot = (dx1 * dx2 + dy1 * dy2) / (len1 * len2);
                        let cross = (dx1 * dy2 - dy1 * dx2) / (len1 * len2);
                        let current_angle = cross.atan2(dot);
                        let diff =
                            (current_angle - *target_angle_rad).rem_euclid(std::f64::consts::TAU);
                        let norm_diff = if diff > std::f64::consts::PI {
                            diff - std::f64::consts::TAU
                        } else {
                            diff
                        };
                        out.push(norm_diff);
                    }
                }
                ConstraintKind::FixPoint(p, target_xy) => {
                    if let Some(pt) = self.get_point_xy(*p, x) {
                        out.push(pt[0] - target_xy[0]);
                        out.push(pt[1] - target_xy[1]);
                    }
                }
                ConstraintKind::FixLength(l_id, target_len) => {
                    if let Some(line) = self.line(*l_id)
                        && let (Some(a), Some(b)) =
                            (self.get_point_xy(line.p1, x), self.get_point_xy(line.p2, x))
                    {
                        let d = (b[0] - a[0]).hypot(b[1] - a[1]);
                        out.push(d - *target_len);
                    }
                }
                ConstraintKind::FixRadius(c_id, target_r) => {
                    if let Some(r) = self.get_circle_r(*c_id, x) {
                        out.push(r.abs() - *target_r);
                    }
                }
            }
        }
    }

    /// Solves the constraint system with default options.
    pub fn solve(&mut self) -> Result<SolveReport, ConstraintError> {
        self.solve_with_options(&SolveOptions::default())
    }

    /// Solves the constraint system with configurable options.
    pub fn solve_with_options(
        &mut self,
        options: &SolveOptions,
    ) -> Result<SolveReport, ConstraintError> {
        let (x0, free, weights) = self.extract_parameters();
        let constraints = self.constraints.clone();

        let f = |x: &[f64], out: &mut Vec<f64>| {
            self.evaluate_residuals(x, &constraints, out);
        };

        let outcome = solver::solve(
            &f,
            &x0,
            &free,
            &weights,
            options.tolerance,
            options.max_iterations,
        );

        if !outcome.converged {
            // Locate conflicting constraints
            let mut conflicting = Vec::new();
            let mut r = Vec::new();
            f(&outcome.x, &mut r);
            if !r.is_empty() {
                for c in &constraints {
                    let mut r_c = Vec::new();
                    self.evaluate_residuals(&outcome.x, std::slice::from_ref(c), &mut r_c);
                    if solver::max_abs(&r_c) > options.tolerance {
                        conflicting.push(c.id);
                    }
                }
            }
            if conflicting.is_empty() {
                conflicting = constraints.iter().map(|c| c.id).collect();
            }
            return Err(ConstraintError::Conflict(conflicting));
        }

        self.write_back_parameters(&outcome.x);
        Ok(SolveReport {
            iterations: outcome.iterations,
            final_residual: outcome.final_residual,
            converged: outcome.converged,
        })
    }

    /// Performs degrees-of-freedom and redundancy analysis.
    pub fn dof_analysis(&self) -> DofReport {
        let (x, free, _) = self.extract_parameters();
        let nf = free.len();
        if nf == 0 || self.constraints.is_empty() {
            let status = if self.constraints.is_empty() {
                DofStatus::Unconstrained
            } else {
                DofStatus::Fully
            };
            return DofReport {
                total_dof: nf,
                rank: 0,
                free_parameters: nf,
                status,
                redundant_constraints: Vec::new(),
                conflicting_constraints: Vec::new(),
            };
        }

        let f_all = |vals: &[f64], out: &mut Vec<f64>| {
            self.evaluate_residuals(vals, &self.constraints, out);
        };

        let mut r = Vec::new();
        f_all(&x, &mut r);
        let total_rows = r.len();

        let jac_all = solver::jacobian(&f_all, &x, &free, total_rows);
        let total_rank = solver::rank(&jac_all, total_rows, nf);

        // Detect redundant constraints by incremental rank evaluation
        let mut redundant = Vec::new();
        let mut cur_constraints: Vec<Constraint> = Vec::new();
        let mut cur_rank = 0;

        for c in &self.constraints {
            cur_constraints.push(c.clone());
            let f_cur = |vals: &[f64], out: &mut Vec<f64>| {
                self.evaluate_residuals(vals, &cur_constraints, out);
            };
            let mut cur_r = Vec::new();
            f_cur(&x, &mut cur_r);
            let rows = cur_r.len();
            let jac = solver::jacobian(&f_cur, &x, &free, rows);
            let new_rank = solver::rank(&jac, rows, nf);
            if new_rank == cur_rank {
                redundant.push(c.id);
            } else {
                cur_rank = new_rank;
            }
        }

        let dof = nf.saturating_sub(total_rank);
        let status = if !redundant.is_empty() {
            DofStatus::Over
        } else if dof == 0 {
            DofStatus::Fully
        } else if total_rank == 0 {
            DofStatus::Unconstrained
        } else {
            DofStatus::Under
        };

        DofReport {
            total_dof: dof,
            rank: total_rank,
            free_parameters: nf,
            status,
            redundant_constraints: redundant,
            conflicting_constraints: Vec::new(),
        }
    }
}

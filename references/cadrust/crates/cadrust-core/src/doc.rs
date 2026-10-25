//! Document schema (.cadrust.json). Every feature references earlier features by id.
//! A feature that creates a body registers it under its own id; a feature that
//! modifies a body replaces it in place, so later features keep using the same id.

use crate::expr::Expr;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Document {
    #[serde(default = "default_version")]
    pub version: u32,
    /// Design name shown in the web app's document tab; the CLI derives it from the file name.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(default = "default_units")]
    pub units: String,
    #[serde(default)]
    pub parameters: BTreeMap<String, Expr>,
    /// Ids of features that are skipped during evaluation.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub suppressed: Vec<String>,
    #[serde(default)]
    pub timeline: Vec<Feature>,
}

fn default_version() -> u32 {
    1
}
fn default_units() -> String {
    "mm".into()
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum Plane {
    #[default]
    XY,
    XZ,
    YZ,
}

impl Plane {
    /// (u axis, v axis, normal) with normal = u x v.
    pub fn basis(self) -> [cadrum::DVec3; 3] {
        use cadrum::DVec3 as V;
        match self {
            Plane::XY => [V::X, V::Y, V::Z],
            Plane::XZ => [V::X, V::Z, -V::Y],
            Plane::YZ => [V::Y, V::Z, V::X],
        }
    }
}

/// Where a sketch lives: a named world plane, or a planar face of a body
/// (nearest planar face to `point`, as the body exists at that point in the timeline).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum PlaneRef {
    Named(Plane),
    Face { face: FaceRef },
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FaceRef {
    pub body: String,
    pub point: [f64; 3],
    /// Persistent face name; resolved before the point when present.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl Default for PlaneRef {
    fn default() -> Self {
        PlaneRef::Named(Plane::XY)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum SketchEntity {
    /// Axis-aligned rectangle from corner (x, y) with size (w, h).
    Rect { x: Expr, y: Expr, w: Expr, h: Expr },
    Circle { cx: Expr, cy: Expr, r: Expr },
    Polygon { points: Vec<[Expr; 2]> },
    Line { x0: Expr, y0: Expr, x1: Expr, y1: Expr },
    /// Three-point arc: start, a point on the arc, end.
    Arc { x0: Expr, y0: Expr, xm: Expr, ym: Expr, x1: Expr, y1: Expr },
}

/// A point of an entity: start/end of a Line or Arc, center of a Circle.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PointRef {
    pub e: usize,
    #[serde(default = "default_point")]
    pub p: String,
}

fn default_point() -> String {
    "start".into()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Constraint {
    Horizontal { e: usize },
    Vertical { e: usize },
    Coincident { a: PointRef, b: PointRef },
    Distance { a: PointRef, b: PointRef, value: Expr },
    Radius { e: usize, value: Expr },
    FixPoint { a: PointRef, x: Expr, y: Expr },
    Equal { a: usize, b: usize },
    Parallel { a: usize, b: usize },
    Perpendicular { a: usize, b: usize },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
pub enum BoolOp {
    #[default]
    NewBody,
    Join,
    Cut,
    Intersect,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum NamedSelect {
    #[default]
    All,
    Top,
    Bottom,
    Vertical,
}

/// Edge/face selector: a named geometric rule, or explicit picks matched by
/// nearest edge midpoint (a geometric stand-in for persistent naming).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(untagged)]
pub enum EdgeSelect {
    Named(NamedSelect),
    /// Picked edges: persistent names (tried first) with the pick points as fallback.
    Pick {
        pick: Vec<[f64; 3]>,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        names: Option<Vec<String>>,
    },
}

impl Default for EdgeSelect {
    fn default() -> Self {
        EdgeSelect::Named(NamedSelect::All)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum Feature {
    Sketch {
        id: String,
        #[serde(default)]
        plane: PlaneRef,
        #[serde(default)]
        offset: Expr,
        entities: Vec<SketchEntity>,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        constraints: Vec<Constraint>,
    },
    Extrude {
        id: String,
        sketch: String,
        distance: Expr,
        /// Second side (opposite the sketch normal); `symmetric` uses `distance` on both sides.
        #[serde(default, skip_serializing_if = "Expr::is_zero")]
        distance2: Expr,
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        symmetric: bool,
        /// Draft angle of the side faces in degrees; positive flares outward.
        #[serde(default, skip_serializing_if = "Expr::is_zero")]
        taper: Expr,
        /// Extend through the whole target body (Join / Cut / Intersect only).
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        through_all: bool,
        #[serde(default)]
        op: BoolOp,
        #[serde(default)]
        target: Option<String>,
    },
    Revolve {
        id: String,
        sketch: String,
        /// Axis in sketch coordinates.
        axis_start: [Expr; 2],
        axis_end: [Expr; 2],
        #[serde(default = "full_turn")]
        angle: Expr,
        #[serde(default)]
        op: BoolOp,
        #[serde(default)]
        target: Option<String>,
    },
    Box {
        id: String,
        size: [Expr; 3],
        #[serde(default)]
        at: [Expr; 3],
        #[serde(default)]
        op: BoolOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    Cylinder {
        id: String,
        radius: Expr,
        height: Expr,
        #[serde(default)]
        at: [Expr; 3],
        #[serde(default)]
        op: BoolOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    Sphere {
        id: String,
        radius: Expr,
        #[serde(default)]
        at: [Expr; 3],
        #[serde(default)]
        op: BoolOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// Torus around a Z axis through `at`; `major` is the ring radius, `minor` the tube radius.
    Torus {
        id: String,
        major: Expr,
        minor: Expr,
        #[serde(default)]
        at: [Expr; 3],
        #[serde(default)]
        op: BoolOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    /// Uniform scale about `center`.
    Scale {
        id: String,
        body: String,
        factor: Expr,
        #[serde(default)]
        center: [Expr; 3],
    },
    /// Split a body by a base plane with offset: the part on the plane's normal side keeps
    /// the body id, the other part becomes a new body under this feature's id.
    Split {
        id: String,
        body: String,
        #[serde(default)]
        plane: Plane,
        #[serde(default)]
        offset: Expr,
    },
    /// Push / pull: offset the picked faces of a body along their normals (positive outward).
    OffsetFace {
        id: String,
        body: String,
        faces: Vec<FaceRef>,
        distance: Expr,
    },
    /// Remove a body from the model.
    Delete {
        id: String,
        body: String,
    },
    /// Body imported from a STEP or BRep file, stored as BRep text so the document stays self-contained.
    Import {
        id: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        source: Option<String>,
        data: String,
        #[serde(default)]
        at: [Expr; 3],
        #[serde(default)]
        op: BoolOp,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        target: Option<String>,
    },
    Fillet {
        id: String,
        body: String,
        radius: Expr,
        #[serde(default)]
        edges: EdgeSelect,
    },
    Chamfer {
        id: String,
        body: String,
        distance: Expr,
        #[serde(default)]
        edges: EdgeSelect,
    },
    Shell {
        id: String,
        body: String,
        thickness: Expr,
        #[serde(default)]
        open: NamedSelect,
    },
    Combine {
        id: String,
        target: String,
        tool: String,
        op: BoolOp,
        /// Keep the tool body instead of consuming it.
        #[serde(default, skip_serializing_if = "std::ops::Not::not")]
        keep_tool: bool,
    },
    Move {
        id: String,
        body: String,
        #[serde(default)]
        translate: [Expr; 3],
        /// Rotation about Z through the origin, degrees.
        #[serde(default)]
        rotate_z: Expr,
    },
    Mirror {
        id: String,
        body: String,
        #[serde(default)]
        plane: Plane,
        #[serde(default)]
        offset: Expr,
        /// Join the mirrored copy to the body; otherwise it becomes a new body under this id.
        #[serde(default = "yes")]
        join: bool,
    },
    Pattern {
        id: String,
        body: String,
        /// Copies along X and Y, including the original.
        count: [Expr; 2],
        step: [Expr; 2],
        /// Fuse all copies into this body (the source body is consumed).
        #[serde(default, skip_serializing_if = "Option::is_none")]
        join: Option<String>,
    },
    /// Copies rotated about a Z axis through `center`, spread over `angle` degrees.
    CircularPattern {
        id: String,
        body: String,
        count: Expr,
        #[serde(default = "full_turn")]
        angle: Expr,
        #[serde(default)]
        center: [Expr; 2],
        #[serde(default, skip_serializing_if = "Option::is_none")]
        join: Option<String>,
    },
    /// Drilled hole along -Z starting at `at` (the surface point).
    Hole {
        id: String,
        body: String,
        at: [Expr; 3],
        diameter: Expr,
        depth: Expr,
        #[serde(default)]
        kind: HoleKind,
        /// Counterbore diameter / depth, or countersink diameter / angle (deg).
        #[serde(default)]
        head_diameter: Expr,
        #[serde(default)]
        head_depth: Expr,
        #[serde(default = "ninety")]
        head_angle: Expr,
    },
    /// Sweep a profile sketch along a path sketch (its first chain or loop).
    Sweep {
        id: String,
        sketch: String,
        path: String,
        #[serde(default)]
        orient: SweepOrient,
        #[serde(default)]
        op: BoolOp,
        #[serde(default)]
        target: Option<String>,
    },
    /// Body colour, written to glTF and STEP.
    Appearance {
        id: String,
        body: String,
        color: String,
    },
    /// Loft through the outer loops of two or more sketches.
    Loft {
        id: String,
        sketches: Vec<String>,
        #[serde(default)]
        ruled: bool,
        #[serde(default)]
        op: BoolOp,
        #[serde(default)]
        target: Option<String>,
    },
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum HoleKind {
    #[default]
    Simple,
    Counterbore,
    Countersink,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Default)]
#[serde(rename_all = "lowercase")]
pub enum SweepOrient {
    /// Profile follows the path (Frenet frame).
    #[default]
    Frenet,
    /// Profile keeps its orientation.
    Fixed,
}

fn ninety() -> Expr {
    Expr::Num(90.0)
}

fn full_turn() -> Expr {
    Expr::Num(360.0)
}
fn yes() -> bool {
    true
}

impl Feature {
    pub fn id(&self) -> &str {
        match self {
            Feature::Sketch { id, .. }
            | Feature::Extrude { id, .. }
            | Feature::Revolve { id, .. }
            | Feature::Box { id, .. }
            | Feature::Cylinder { id, .. }
            | Feature::Fillet { id, .. }
            | Feature::Chamfer { id, .. }
            | Feature::Shell { id, .. }
            | Feature::Combine { id, .. }
            | Feature::Move { id, .. }
            | Feature::Mirror { id, .. }
            | Feature::Pattern { id, .. }
            | Feature::CircularPattern { id, .. }
            | Feature::Hole { id, .. }
            | Feature::Sweep { id, .. }
            | Feature::Appearance { id, .. }
            | Feature::Sphere { id, .. }
            | Feature::Torus { id, .. }
            | Feature::Scale { id, .. }
            | Feature::Split { id, .. }
            | Feature::OffsetFace { id, .. }
            | Feature::Delete { id, .. }
            | Feature::Import { id, .. }
            | Feature::Loft { id, .. } => id,
        }
    }
    pub fn kind(&self) -> &'static str {
        match self {
            Feature::Sketch { .. } => "Sketch",
            Feature::Extrude { .. } => "Extrude",
            Feature::Revolve { .. } => "Revolve",
            Feature::Box { .. } => "Box",
            Feature::Cylinder { .. } => "Cylinder",
            Feature::Fillet { .. } => "Fillet",
            Feature::Chamfer { .. } => "Chamfer",
            Feature::Shell { .. } => "Shell",
            Feature::Combine { .. } => "Combine",
            Feature::Move { .. } => "Move",
            Feature::Mirror { .. } => "Mirror",
            Feature::Pattern { .. } => "Pattern",
            Feature::CircularPattern { .. } => "CircularPattern",
            Feature::Hole { .. } => "Hole",
            Feature::Sweep { .. } => "Sweep",
            Feature::Appearance { .. } => "Appearance",
            Feature::Sphere { .. } => "Sphere",
            Feature::Torus { .. } => "Torus",
            Feature::Scale { .. } => "Scale",
            Feature::Split { .. } => "Split",
            Feature::OffsetFace { .. } => "OffsetFace",
            Feature::Delete { .. } => "Delete",
            Feature::Import { .. } => "Import",
            Feature::Loft { .. } => "Loft",
        }
    }
}

impl Document {
    pub fn from_json(s: &str) -> crate::Result<Self> {
        Ok(serde_json::from_str(s)?)
    }
    pub fn to_json(&self) -> crate::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}

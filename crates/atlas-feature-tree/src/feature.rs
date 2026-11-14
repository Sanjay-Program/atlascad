//! Parametric 3D solid and surface feature definitions.

use serde::{Deserialize, Serialize};

/// Solid boolean combination method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BooleanOp {
    Join,
    Cut,
    Intersect,
    NewBody,
}

/// Parametric feature operations.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum FeatureOperation {
    /// Linear sketch extrusion along plane normal.
    Extrude {
        profile_id: String,
        depth_mm: f64,
        boolean_op: BooleanOp,
    },
    /// Revolving sketch profile around a 3D axis.
    Revolve {
        profile_id: String,
        angle_deg: f64,
        axis_origin: [f64; 3],
        axis_direction: [f64; 3],
    },
    /// Rounding sharp edges with continuous tangent arc.
    Fillet {
        edge_ids: Vec<String>,
        radius_mm: f64,
    },
    /// Beveling sharp edges.
    Chamfer {
        edge_ids: Vec<String>,
        distance_mm: f64,
    },
    /// Drilled or tapped cylindrical hole.
    Hole {
        center: [f64; 3],
        diameter_mm: f64,
        depth_mm: f64,
        through_all: bool,
    },
    /// Rectangular / linear array duplication.
    LinearPattern {
        feature_id: String,
        count: u32,
        spacing_mm: f64,
        direction: [f64; 3],
    },
    /// Symmetry across a planar face or datum.
    Mirror {
        feature_ids: Vec<String>,
        plane_normal: [f64; 3],
        plane_point: [f64; 3],
    },
}

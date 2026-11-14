//! Directed dependency edges between engineering nodes.

use atlas_core::identifiers::NodeId;
use serde::{Deserialize, Serialize};

/// Reason for a dependency relationship.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DependencyReason {
    /// Target uses source as geometric input.
    GeometricBoundary,
    /// Target discretizes or meshes the source.
    SpatialDiscretization,
    /// Target extracts orthographic or section views from source.
    ProjectionView,
    /// Target calculates toolpaths over source surface.
    ToolpathSurface,
    /// Target summarizes physical or BOM properties from source.
    PropertyAggregation,
}

/// A directed edge in the DAG (Source -> Target). Target depends on Source.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyEdge {
    pub source: NodeId,
    pub target: NodeId,
    pub reason: DependencyReason,
}

impl DependencyEdge {
    pub fn new(source: NodeId, target: NodeId, reason: DependencyReason) -> Self {
        Self {
            source,
            target,
            reason,
        }
    }
}

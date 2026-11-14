//! Dependency graph node representations and state tracking.

use atlas_core::identifiers::NodeId;
use serde::{Deserialize, Serialize};

/// Functional classification of engineering nodes in the dependency graph.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum NodeKind {
    /// Parametric or B-Rep CAD geometry.
    Geometry,
    /// 2D/3D surface or volume mesh.
    Mesh,
    /// Finite element analysis structural or thermal simulation.
    FeaSimulation,
    /// Computational fluid dynamics or aerodynamics solver.
    CfdSimulation,
    /// 2D engineering drawing, projection, or layout.
    Drawing,
    /// Subtractive or additive manufacturing toolpath.
    CamToolpath,
    /// Building Information Modeling coordination or IFC model.
    BimCoordination,
    /// Engineering compliance, calculation, or BOM report.
    EngineeringReport,
}

/// Operational state of a node in the graph.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NodeState {
    /// Up-to-date; results match current upstream inputs.
    Current,
    /// Outdated due to upstream changes; requires recomputation.
    Stale { reason: String },
    /// Currently computing in a worker thread or subprocess.
    Computing,
    /// Failed during recomputation.
    Failed { error: String },
}

/// A discrete node in the engineering dependency DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependencyNode {
    pub id: NodeId,
    pub name: String,
    pub kind: NodeKind,
    pub state: NodeState,
    pub revision: u64,
    pub provenance_hash: String,
}

impl DependencyNode {
    /// Construct a new node.
    pub fn new(id: NodeId, name: impl Into<String>, kind: NodeKind) -> Self {
        Self {
            id,
            name: name.into(),
            kind,
            state: NodeState::Current,
            revision: 1,
            provenance_hash: "0000000000000000".to_string(),
        }
    }

    /// Mark the node as stale.
    pub fn mark_stale(&mut self, reason: impl Into<String>) {
        self.state = NodeState::Stale {
            reason: reason.into(),
        };
    }

    /// Mark the node as current with a new provenance hash and revision.
    pub fn mark_current(&mut self, new_hash: impl Into<String>) {
        self.state = NodeState::Current;
        self.revision = self.revision.saturating_add(1);
        self.provenance_hash = new_hash.into();
    }
}

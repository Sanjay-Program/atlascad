//! # atlas-dependency-graph
//!
//! Engineering Directed Acyclic Graph (DAG) coordinating CAD geometry,
//! meshes, simulations, technical drawings, CAM toolpaths, and reports.
//!
//! Provides automated downstream invalidation, cycle detection, provenance
//! hashing, and topological recomputation scheduling.

pub mod cycle_detection;
pub mod edge;
pub mod graph;
pub mod invalidation;
pub mod node;
pub mod provenance;
pub mod scheduler;

pub mod prelude {
    pub use crate::edge::{DependencyEdge, DependencyReason};
    pub use crate::graph::DependencyGraph;
    pub use crate::node::{DependencyNode, NodeKind, NodeState};
    pub use crate::provenance::ProvenanceRecord;
}

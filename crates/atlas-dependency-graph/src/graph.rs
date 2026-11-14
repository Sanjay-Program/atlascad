//! Engineering Directed Acyclic Dependency Graph (DAG).

use crate::cycle_detection::CycleDetector;
use crate::edge::{DependencyEdge, DependencyReason};
use crate::invalidation::InvalidationEngine;
use crate::node::{DependencyNode, NodeState};
use crate::scheduler::DependencyScheduler;
use atlas_core::error::CoreError;
use atlas_core::identifiers::NodeId;
use std::collections::{HashMap, HashSet};

/// Complete dependency graph coordinating all engineering artifacts.
#[derive(Debug, Default, Clone)]
pub struct DependencyGraph {
    nodes: HashMap<NodeId, DependencyNode>,
    /// Outgoing edges: Source -> `Vec<Target>` (Target depends on Source)
    downstream: HashMap<NodeId, Vec<NodeId>>,
    /// Incoming edges: Target -> `Vec<Source>` (Target depends on Source)
    upstream: HashMap<NodeId, Vec<NodeId>>,
    edges: Vec<DependencyEdge>,
}

impl DependencyGraph {
    /// Create a new empty engineering dependency graph.
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
            downstream: HashMap::new(),
            upstream: HashMap::new(),
            edges: Vec::new(),
        }
    }

    /// Register a new node in the graph.
    pub fn add_node(&mut self, node: DependencyNode) {
        self.nodes.insert(node.id.clone(), node);
    }

    /// Add a dependency relationship: `target` depends on `source`.
    pub fn add_dependency(
        &mut self,
        source: NodeId,
        target: NodeId,
        reason: DependencyReason,
    ) -> Result<(), CoreError> {
        if !self.nodes.contains_key(&source) {
            return Err(CoreError::NotFound(format!("Source node '{}'", source)));
        }
        if !self.nodes.contains_key(&target) {
            return Err(CoreError::NotFound(format!("Target node '{}'", target)));
        }

        // Verify DAG acyclicity before insertion
        CycleDetector::would_create_cycle(&self.downstream, &source, &target)?;

        self.downstream
            .entry(source.clone())
            .or_default()
            .push(target.clone());
        self.upstream
            .entry(target.clone())
            .or_default()
            .push(source.clone());
        self.edges
            .push(DependencyEdge::new(source, target, reason));

        Ok(())
    }

    /// Invalidate a modified node and automatically cascade stale state to all downstream dependents.
    pub fn invalidate(&mut self, root: &NodeId, reason: &str) -> Vec<NodeId> {
        InvalidationEngine::invalidate_downstream(&mut self.nodes, &self.downstream, root, reason)
    }

    /// Plan the recomputation order for all currently stale nodes.
    pub fn plan_recomputation(&self) -> Result<Vec<NodeId>, CoreError> {
        let stale: HashSet<NodeId> = self
            .nodes
            .iter()
            .filter(|(_, n)| matches!(n.state, NodeState::Stale { .. }))
            .map(|(id, _)| id.clone())
            .collect();

        DependencyScheduler::plan_recompute(&stale, &self.upstream)
    }

    /// Retrieve a node by ID.
    pub fn get_node(&self, id: &NodeId) -> Option<&DependencyNode> {
        self.nodes.get(id)
    }

    /// Retrieve a mutable reference to a node by ID.
    pub fn get_node_mut(&mut self, id: &NodeId) -> Option<&mut DependencyNode> {
        self.nodes.get_mut(id)
    }

    /// Count total registered nodes.
    pub fn node_count(&self) -> usize {
        self.nodes.len()
    }

    /// Count total dependency edges.
    pub fn edge_count(&self) -> usize {
        self.edges.len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::NodeKind;

    #[test]
    fn test_full_cad_engineering_dependency_workflow() {
        let mut graph = DependencyGraph::new();

        let geom = NodeId::new("cad-geom");
        let mesh = NodeId::new("fem-mesh");
        let fea = NodeId::new("fea-solve");
        let cfd = NodeId::new("cfd-solve");
        let dwg = NodeId::new("drawing-sheet");
        let cam = NodeId::new("cnc-toolpath");

        graph.add_node(DependencyNode::new(geom.clone(), "Solid CAD Model", NodeKind::Geometry));
        graph.add_node(DependencyNode::new(mesh.clone(), "FEM Tetrahedral Mesh", NodeKind::Mesh));
        graph.add_node(DependencyNode::new(fea.clone(), "CalculiX Structural FEA", NodeKind::FeaSimulation));
        graph.add_node(DependencyNode::new(cfd.clone(), "OpenFOAM Aerodynamics", NodeKind::CfdSimulation));
        graph.add_node(DependencyNode::new(dwg.clone(), "4-View Orthographic Drawing", NodeKind::Drawing));
        graph.add_node(DependencyNode::new(cam.clone(), "3-Axis CNC Toolpath", NodeKind::CamToolpath));

        assert!(graph.add_dependency(geom.clone(), mesh.clone(), DependencyReason::SpatialDiscretization).is_ok());
        assert!(graph.add_dependency(mesh.clone(), fea.clone(), DependencyReason::PropertyAggregation).is_ok());
        assert!(graph.add_dependency(mesh.clone(), cfd.clone(), DependencyReason::PropertyAggregation).is_ok());
        assert!(graph.add_dependency(geom.clone(), dwg.clone(), DependencyReason::ProjectionView).is_ok());
        assert!(graph.add_dependency(geom.clone(), cam.clone(), DependencyReason::ToolpathSurface).is_ok());

        assert_eq!(graph.node_count(), 6);
        assert_eq!(graph.edge_count(), 5);

        // When CAD geometry changes, cascade invalidation
        let affected = graph.invalidate(&geom, "Pocket depth updated");
        assert_eq!(affected.len(), 6);

        // Compute recompute plan
        let plan = graph.plan_recomputation();
        assert!(plan.is_ok());
        if let Ok(order) = plan {
            assert_eq!(order.len(), 6);
            assert_eq!(order[0], geom);
        }
    }
}

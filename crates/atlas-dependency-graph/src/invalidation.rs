//! Automated downstream invalidation cascades.

use crate::node::DependencyNode;
use atlas_core::identifiers::NodeId;
use std::collections::{HashMap, HashSet};

/// Handles downstream stale-marking when an upstream artifact changes.
pub struct InvalidationEngine;

impl InvalidationEngine {
    /// Invalidate a root node and cascade down all dependent nodes in the DAG.
    ///
    /// Returns the list of affected NodeIds in topological cascade order.
    pub fn invalidate_downstream(
        nodes: &mut HashMap<NodeId, DependencyNode>,
        downstream_adj: &HashMap<NodeId, Vec<NodeId>>,
        root: &NodeId,
        reason: &str,
    ) -> Vec<NodeId> {
        let mut affected = Vec::new();
        let mut visited = HashSet::new();
        let mut queue = Vec::new();

        if let Some(node) = nodes.get_mut(root) {
            node.mark_stale(reason);
            affected.push(root.clone());
            visited.insert(root.clone());
        }

        if let Some(children) = downstream_adj.get(root) {
            for child in children {
                queue.push(child.clone());
            }
        }

        while let Some(current) = queue.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }

            if let Some(node) = nodes.get_mut(&current) {
                node.mark_stale(format!(
                    "Upstream dependency '{}' changed: {}",
                    root, reason
                ));
                affected.push(current.clone());
            }

            if let Some(children) = downstream_adj.get(&current) {
                for child in children {
                    if !visited.contains(child) {
                        queue.push(child.clone());
                    }
                }
            }
        }

        affected
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::node::{NodeKind, NodeState};

    #[test]
    fn test_cascade_invalidation() {
        let mut nodes = HashMap::new();
        let geom = NodeId::new("cad-geom");
        let mesh = NodeId::new("fem-mesh");
        let fea = NodeId::new("fea-solve");

        nodes.insert(
            geom.clone(),
            DependencyNode::new(geom.clone(), "Geometry", NodeKind::Geometry),
        );
        nodes.insert(
            mesh.clone(),
            DependencyNode::new(mesh.clone(), "Mesh", NodeKind::Mesh),
        );
        nodes.insert(
            fea.clone(),
            DependencyNode::new(fea.clone(), "FEA", NodeKind::FeaSimulation),
        );

        let mut downstream = HashMap::new();
        downstream.insert(geom.clone(), vec![mesh.clone()]);
        downstream.insert(mesh.clone(), vec![fea.clone()]);

        let affected = InvalidationEngine::invalidate_downstream(
            &mut nodes,
            &downstream,
            &geom,
            "Extrude depth modified from 10 to 15mm",
        );

        assert_eq!(affected.len(), 3);
        if let Some(n) = nodes.get(&geom) {
            assert!(matches!(n.state, NodeState::Stale { .. }));
        }
        if let Some(n) = nodes.get(&mesh) {
            assert!(matches!(n.state, NodeState::Stale { .. }));
        }
        if let Some(n) = nodes.get(&fea) {
            assert!(matches!(n.state, NodeState::Stale { .. }));
        }
    }
}

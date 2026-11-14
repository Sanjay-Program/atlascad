//! Topological scheduler for incremental recomputation.

use atlas_core::error::CoreError;
use atlas_core::identifiers::NodeId;
use std::collections::{HashMap, HashSet, VecDeque};

/// Scheduler computing safe parallel or sequential execution batches.
pub struct DependencyScheduler;

impl DependencyScheduler {
    /// Compute a topological recomputation order for all stale nodes.
    pub fn plan_recompute(
        stale_nodes: &HashSet<NodeId>,
        incoming_adj: &HashMap<NodeId, Vec<NodeId>>,
    ) -> Result<Vec<NodeId>, CoreError> {
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        let mut adj: HashMap<NodeId, Vec<NodeId>> = HashMap::new();

        for node in stale_nodes {
            in_degree.insert(node.clone(), 0);
            adj.insert(node.clone(), Vec::new());
        }

        // Calculate in-degrees restricted to the stale subgraph
        for target in stale_nodes {
            if let Some(sources) = incoming_adj.get(target) {
                for source in sources {
                    if stale_nodes.contains(source) {
                        adj.entry(source.clone()).or_default().push(target.clone());
                        let count = in_degree.entry(target.clone()).or_insert(0);
                        *count = count.saturating_add(1);
                    }
                }
            }
        }

        let mut queue: VecDeque<NodeId> = in_degree
            .iter()
            .filter(|(_, deg)| **deg == 0)
            .map(|(id, _)| id.clone())
            .collect();

        let mut order = Vec::new();

        while let Some(u) = queue.pop_front() {
            order.push(u.clone());

            if let Some(neighbors) = adj.get(&u) {
                for v in neighbors {
                    if let Some(deg) = in_degree.get_mut(v) {
                        *deg = deg.saturating_sub(1);
                        if *deg == 0 {
                            queue.push_back(v.clone());
                        }
                    }
                }
            }
        }

        if order.len() != stale_nodes.len() {
            return Err(CoreError::ValidationFailed(
                "Cycle detected in stale recomputation subgraph".to_string(),
            ));
        }

        Ok(order)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_topological_scheduler() {
        let a = NodeId::new("Geometry");
        let b = NodeId::new("Mesh");
        let c = NodeId::new("FEA");

        let mut stale = HashSet::new();
        stale.insert(a.clone());
        stale.insert(b.clone());
        stale.insert(c.clone());

        let mut incoming = HashMap::new();
        incoming.insert(b.clone(), vec![a.clone()]);
        incoming.insert(c.clone(), vec![b.clone()]);

        let plan = DependencyScheduler::plan_recompute(&stale, &incoming);
        assert!(plan.is_ok());
        if let Ok(order) = plan {
            assert_eq!(order, vec![a, b, c]);
        }
    }
}

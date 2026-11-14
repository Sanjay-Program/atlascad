//! Cycle detection using Depth-First Search for DAG integrity.

use atlas_core::error::CoreError;
use atlas_core::identifiers::NodeId;
use std::collections::{HashMap, HashSet};

/// Detects cycles in a directed graph.
pub struct CycleDetector;

impl CycleDetector {
    /// Verify that adding an edge (source -> target) will not introduce a cycle.
    pub fn would_create_cycle(
        adj: &HashMap<NodeId, Vec<NodeId>>,
        source: &NodeId,
        target: &NodeId,
    ) -> Result<(), CoreError> {
        if source == target {
            return Err(CoreError::ValidationFailed(format!(
                "Self-referential dependency on node '{}'",
                source
            )));
        }

        // A cycle occurs if there is already a path from target to source
        let mut visited = HashSet::new();
        let mut stack = vec![target.clone()];

        while let Some(current) = stack.pop() {
            if current == *source {
                return Err(CoreError::ValidationFailed(format!(
                    "Dependency cycle detected between '{}' and '{}'",
                    source, target
                )));
            }

            if !visited.insert(current.clone()) {
                continue;
            }

            if let Some(neighbors) = adj.get(&current) {
                for neighbor in neighbors {
                    if !visited.contains(neighbor) {
                        stack.push(neighbor.clone());
                    }
                }
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cycle_detection() {
        let mut adj = HashMap::new();
        let a = NodeId::new("A");
        let b = NodeId::new("B");
        let c = NodeId::new("C");

        adj.insert(a.clone(), vec![b.clone()]);
        adj.insert(b.clone(), vec![c.clone()]);

        // Adding C -> A would create A -> B -> C -> A cycle
        assert!(CycleDetector::would_create_cycle(&adj, &c, &a).is_err());

        // Adding A -> C is fine (transitive DAG edge)
        assert!(CycleDetector::would_create_cycle(&adj, &a, &c).is_ok());
    }
}

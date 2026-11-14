//! Sequential feature timeline and interactive rollback bar.

use crate::feature_node::FeatureNode;
use atlas_core::error::CoreError;

/// Sequential feature tree with rollback bar cursor.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct FeatureHistory {
    features: Vec<FeatureNode>,
    /// Index in `features` where the rollback bar is placed.
    /// Operations after this index are rolled back (suppressed from evaluation).
    rollback_index: Option<usize>,
}

impl FeatureHistory {
    pub fn new() -> Self {
        Self {
            features: Vec::new(),
            rollback_index: None,
        }
    }

    /// Add a feature to the end of the history.
    pub fn append(&mut self, feature: FeatureNode) {
        self.features.push(feature);
    }

    /// Move rollback bar to a specific feature index.
    pub fn set_rollback(&mut self, index: Option<usize>) -> Result<(), CoreError> {
        if let Some(idx) = index.filter(|&i| i > self.features.len()) {
            return Err(CoreError::NotFound(format!(
                "Rollback index {} out of bounds",
                idx
            )));
        }
        self.rollback_index = index;
        Ok(())
    }

    /// Features currently active (before or at the rollback bar).
    pub fn active_features(&self) -> &[FeatureNode] {
        let limit = self.rollback_index.unwrap_or(self.features.len());
        &self.features[..limit]
    }

    /// Total feature count.
    pub fn len(&self) -> usize {
        self.features.len()
    }

    /// Is empty.
    pub fn is_empty(&self) -> bool {
        self.features.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::{BooleanOp, FeatureOperation};

    #[test]
    fn test_rollback_bar() {
        let mut tree = FeatureHistory::new();
        tree.append(FeatureNode::new(
            "f1",
            "Extrude Base",
            FeatureOperation::Extrude {
                profile_id: "sk1".to_string(),
                depth_mm: 20.0,
                boolean_op: BooleanOp::NewBody,
            },
        ));
        tree.append(FeatureNode::new(
            "f2",
            "Cut Pocket",
            FeatureOperation::Extrude {
                profile_id: "sk2".to_string(),
                depth_mm: 5.0,
                boolean_op: BooleanOp::Cut,
            },
        ));

        assert_eq!(tree.active_features().len(), 2);

        // Roll back before f2
        assert!(tree.set_rollback(Some(1)).is_ok());
        assert_eq!(tree.active_features().len(), 1);
        assert_eq!(tree.active_features()[0].id, "f1");

        // Roll forward to end
        assert!(tree.set_rollback(None).is_ok());
        assert_eq!(tree.active_features().len(), 2);
    }
}

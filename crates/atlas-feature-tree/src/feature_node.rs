//! Feature graph node with state and dependency tracking.

use crate::feature::FeatureOperation;
use serde::{Deserialize, Serialize};

/// Operational state of a single parametric feature.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeatureState {
    Valid,
    NeedsRecompute,
    Suppressed,
    Error(String),
}

/// A node in the parametric feature timeline.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FeatureNode {
    pub id: String,
    pub name: String,
    pub operation: FeatureOperation,
    pub state: FeatureState,
    pub dependencies: Vec<String>,
}

impl FeatureNode {
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        operation: FeatureOperation,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            operation,
            state: FeatureState::NeedsRecompute,
            dependencies: Vec::new(),
        }
    }

    pub fn with_dependency(mut self, dep_id: impl Into<String>) -> Self {
        self.dependencies.push(dep_id.into());
        self
    }
}

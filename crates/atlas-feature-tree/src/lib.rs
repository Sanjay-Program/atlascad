//! # atlas-feature-tree
//!
//! Parametric modeling feature tree, history timeline, rollback bar cursor,
//! parameter variables, validation, and incremental recompute.

pub mod feature;
pub mod feature_node;
pub mod history;
pub mod parameters;
pub mod recompute;
pub mod validation;

pub mod prelude {
    pub use crate::feature::{BooleanOp, FeatureOperation};
    pub use crate::feature_node::{FeatureNode, FeatureState};
    pub use crate::history::FeatureHistory;
    pub use crate::parameters::Parameter;
    pub use crate::recompute::RecomputeEngine;
    pub use crate::validation::FeatureValidator;
}

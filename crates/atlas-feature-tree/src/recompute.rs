//! Incremental feature recomputation engine.

use crate::history::FeatureHistory;
use crate::validation::FeatureValidator;
use atlas_core::error::CoreError;

pub struct RecomputeEngine;

impl RecomputeEngine {
    /// Recomputes active features in topological timeline sequence.
    pub fn recompute_timeline(history: &mut FeatureHistory) -> Result<usize, CoreError> {
        let mut recomputed_count: usize = 0;
        let limit = history.active_features().len();

        for i in 0..limit {
            let active = history.active_features();
            let feat = match active.get(i) {
                Some(f) => f.clone(),
                None => continue,
            };

            // Validate geometric parameters
            FeatureValidator::validate(&feat.operation)?;

            recomputed_count = recomputed_count.saturating_add(1);
        }

        Ok(recomputed_count)
    }
}

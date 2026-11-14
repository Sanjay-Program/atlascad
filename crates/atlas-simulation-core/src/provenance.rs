//! Solver execution provenance and verification records.

use serde::{Deserialize, Serialize};

/// Immutable verification and execution record for a simulation run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationProvenance {
    pub problem_id: String,
    pub engine_id: String,
    pub engine_version: String,
    pub input_hash: String,
    pub timestamp_unix: u64,
}

impl SimulationProvenance {
    pub fn new(engine_id: &str, engine_version: &str) -> Self {
        Self {
            problem_id: String::new(),
            engine_id: engine_id.to_string(),
            engine_version: engine_version.to_string(),
            input_hash: "0000000000000000".to_string(),
            timestamp_unix: 1700000000,
        }
    }
}

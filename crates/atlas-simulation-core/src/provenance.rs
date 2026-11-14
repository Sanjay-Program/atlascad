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

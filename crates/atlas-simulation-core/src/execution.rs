//! Simulation execution results and output payloads.

use crate::convergence::ConvergenceMonitor;
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// High-level simulation completion status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SimulationStatus {
    Converged,
    Diverged,
    TimedOut,
    Failed,
}

/// Standardized execution result returned by all integrated solvers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationResult {
    pub problem_id: String,
    pub engine_id: String,
    pub status: SimulationStatus,
    pub execution_time_ms: u64,
    pub scalar_metrics: HashMap<String, f64>,
    pub convergence: ConvergenceMonitor,
    pub validation: Option<AnalyticalComparison>,
    pub provenance: SimulationProvenance,
    pub logs: Vec<String>,
}

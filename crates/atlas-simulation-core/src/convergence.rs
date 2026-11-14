//! Convergence monitors and residual telemetry.

use serde::{Deserialize, Serialize};

/// Residual telemetry for a single solver iteration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IterationTelemetry {
    pub iteration: u32,
    pub residual_norm: f64,
    pub elapsed_ms: u64,
}

/// Convergence state tracker for iterative numerical solvers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ConvergenceMonitor {
    pub is_converged: bool,
    pub total_iterations: u32,
    pub final_residual: f64,
    pub history: Vec<IterationTelemetry>,
}

impl Default for ConvergenceMonitor {
    fn default() -> Self {
        Self {
            is_converged: false,
            total_iterations: 0,
            final_residual: 1.0,
            history: Vec::new(),
        }
    }
}

impl ConvergenceMonitor {
    pub fn record(&mut self, iteration: u32, residual: f64, elapsed_ms: u64, tolerance: f64) {
        self.total_iterations = iteration;
        self.final_residual = residual;
        self.is_converged = residual <= tolerance;
        self.history.push(IterationTelemetry {
            iteration,
            residual_norm: residual,
            elapsed_ms,
        });
    }
}

//! Numerical solver controls and convergence settings.

use serde::{Deserialize, Serialize};

/// Numerical convergence settings and iteration limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolverSettings {
    /// Maximum non-linear or iterative solver iterations.
    pub max_iterations: u32,
    /// Target residual convergence tolerance (e.g. 1e-6).
    pub tolerance: f64,
    /// Time step size in seconds for transient solvers.
    pub time_step_seconds: Option<f64>,
    /// Total duration in seconds for transient simulations.
    pub total_duration_seconds: Option<f64>,
    /// Number of concurrent CPU worker threads.
    pub thread_count: usize,
    /// Whether GPU acceleration is requested if available.
    pub use_gpu: bool,
}

impl Default for SolverSettings {
    fn default() -> Self {
        Self {
            max_iterations: 100,
            tolerance: 1e-6,
            time_step_seconds: None,
            total_duration_seconds: None,
            thread_count: 4,
            use_gpu: false,
        }
    }
}

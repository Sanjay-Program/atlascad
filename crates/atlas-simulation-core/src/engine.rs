//! Engine adapter traits for physical simulations.

use crate::execution::SimulationResult;
use crate::problem::SimulationProblem;
use atlas_core::error::CoreError;

/// Core trait implemented by multidisciplinary solver adapters.
pub trait SimulationEngineAdapter: Send + Sync {
    /// Return the unique identifier of the engine (e.g. "calculix", "openfoam").
    fn engine_id(&self) -> &str;

    /// Check if this adapter can solve the given physics problem.
    fn supports_problem(&self, problem: &SimulationProblem) -> bool;

    /// Execute the simulation problem and return standard results.
    fn execute(&self, problem: &SimulationProblem) -> Result<SimulationResult, CoreError>;
}

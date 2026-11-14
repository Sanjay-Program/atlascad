//! # atlas-simulation-core
//!
//! Standardized engineering simulation problem definitions, boundary conditions,
//! materials, solver settings, convergence telemetry, and analytical validation.

pub mod boundary_conditions;
pub mod convergence;
pub mod engine;
pub mod execution;
pub mod materials;
pub mod model;
pub mod problem;
pub mod provenance;
pub mod solver_settings;
pub mod validation;

pub mod prelude {
    pub use crate::boundary_conditions::BoundaryCondition;
    pub use crate::convergence::{ConvergenceMonitor, IterationTelemetry};
    pub use crate::engine::SimulationEngineAdapter;
    pub use crate::execution::{SimulationResult, SimulationStatus};
    pub use crate::materials::SimulationMaterial;
    pub use crate::problem::{AnalysisType, SimulationProblem};
    pub use crate::solver_settings::SolverSettings;
    pub use crate::validation::{AnalyticalComparison, ValidationStatus};
}

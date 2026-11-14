//! # atlas-simulation-core
//!
//! Standardized engineering simulation problem definitions, boundary conditions,
//! materials, solver settings, convergence telemetry, and analytical validation.

pub mod aerodynamics;
pub mod boundary_conditions;
pub mod cfd;
pub mod chemical;
pub mod convergence;
pub mod engine;
pub mod execution;
pub mod generative;
pub mod geotechnical;
pub mod materials;
pub mod model;
pub mod molecular;
pub mod power_flow;
pub mod problem;
pub mod provenance;
pub mod robotics;
pub mod solver_settings;
pub mod validation;

pub mod prelude {
    pub use crate::aerodynamics::{
        NormalShockProblem, WingAeroProblem, solve_normal_shock, solve_wing_aerodynamics,
    };
    pub use crate::boundary_conditions::BoundaryCondition;
    pub use crate::cfd::{FluidProperties, PipeFlowProblem, solve_pipe_flow};
    pub use crate::chemical::{ChemicalComponent, FlashProblem, solve_rachford_rice_flash};
    pub use crate::convergence::{ConvergenceMonitor, IterationTelemetry};
    pub use crate::engine::SimulationEngineAdapter;
    pub use crate::execution::{SimulationResult, SimulationStatus};
    pub use crate::generative::{TopologyOptProblem, solve_topology_optimization};
    pub use crate::geotechnical::{
        RetainingWallProblem, SoilProperties, solve_retaining_wall_pressures,
    };
    pub use crate::materials::SimulationMaterial;
    pub use crate::molecular::{
        Atom, LennardJonesParam, MolecularDynamicsProblem, run_molecular_dynamics,
    };
    pub use crate::power_flow::{BusType, PowerFlowProblem, TransmissionLine, solve_power_flow};
    pub use crate::problem::{AnalysisType, SimulationProblem};
    pub use crate::robotics::{DhLink, RobotKinematicsProblem, solve_robot_kinematics};
    pub use crate::solver_settings::SolverSettings;
    pub use crate::validation::{AnalyticalComparison, ValidationStatus};
}

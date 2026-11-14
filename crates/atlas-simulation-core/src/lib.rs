//! # atlas-simulation-core
//!
//! Standardized engineering simulation problem definitions, boundary conditions,
//! materials, solver settings, convergence telemetry, and analytical validation.

pub mod acoustics;
pub mod aerodynamics;
pub mod aeroelasticity;
pub mod biomedical;
pub mod boundary_conditions;
pub mod building_physics;
pub mod cavitation;
pub mod cfd;
pub mod chemical;
pub mod convergence;
pub mod digital_twin;
pub mod electromagnetics;
pub mod engine;
pub mod execution;
pub mod fsi;
pub mod generative;
pub mod geotechnical;
pub mod marine;
pub mod materials;
pub mod model;
pub mod molecular;
pub mod power_flow;
pub mod problem;
pub mod provenance;
pub mod rainflow;
pub mod robotics;
pub mod solar;
pub mod solver_settings;
pub mod structural;
pub mod thermal_stress;
pub mod validation;

pub mod prelude {
    pub use crate::acoustics::{AcousticsProblem, solve_acoustics};
    pub use crate::aerodynamics::{
        NormalShockProblem, WingAeroProblem, solve_normal_shock, solve_wing_aerodynamics,
    };
    pub use crate::aeroelasticity::{AeroelasticProblem, solve_aeroelastic_viv};
    pub use crate::biomedical::{
        HemodynamicsRequest, HemodynamicsResult, OrthopedicBoneRequest, OrthopedicBoneResult,
        solve_hemodynamics, solve_orthopedic_bone,
    };
    pub use crate::boundary_conditions::BoundaryCondition;
    pub use crate::building_physics::{
        BuildingAssemblyProblem, BuildingLayer, solve_building_u_value,
    };
    pub use crate::cavitation::{CavitationProblem, solve_cavitation_npsh};
    pub use crate::cfd::{FluidProperties, PipeFlowProblem, solve_pipe_flow};
    pub use crate::chemical::{ChemicalComponent, FlashProblem, solve_rachford_rice_flash};
    pub use crate::convergence::{ConvergenceMonitor, IterationTelemetry};
    pub use crate::digital_twin::{
        DigitalTwinProblem, StrainRosetteTelemetry, StressCycleBlock, solve_digital_twin,
    };
    pub use crate::electromagnetics::{MicrostripProblem, solve_microstrip_line};
    pub use crate::engine::SimulationEngineAdapter;
    pub use crate::execution::{SimulationResult, SimulationStatus};
    pub use crate::fsi::{FsiProblem, solve_fsi_coupled};
    pub use crate::generative::{
        GenerativeMesh, GenerativeSolution, TopologyOptProblem, filter_sensitivities,
        reconstruct_generative_mesh, solve_topology_optimization,
    };
    pub use crate::geotechnical::{
        RetainingWallProblem, SoilProperties, solve_retaining_wall_pressures,
    };
    pub use crate::marine::{
        AiryWaveRequest, AiryWaveResult, MarineHydrostaticsRequest, MarineHydrostaticsResult,
        solve_airy_wave, solve_hydrostatics,
    };
    pub use crate::materials::SimulationMaterial;
    pub use crate::molecular::{
        Atom, LennardJonesParam, MolecularDynamicsProblem, run_molecular_dynamics,
    };
    pub use crate::power_flow::{
        BusType, ComplexNumber, PowerFlowProblem, TransmissionLine, build_bus_admittance_matrix,
        solve_power_flow, y_bus_frobenius_norm,
    };
    pub use crate::problem::{AnalysisType, SimulationProblem};
    pub use crate::rainflow::{
        RainflowCycle, RainflowProblem, count_rainflow_cycles, solve_rainflow_fatigue,
    };
    pub use crate::robotics::{
        DhLink, IkResult, QuinticTrajectoryProblem, RobotKinematicsProblem,
        compute_forward_transforms, compute_geometric_jacobian, compute_manipulability,
        solve_dls_inverse_kinematics, solve_quintic_trajectory, solve_robot_kinematics,
    };
    pub use crate::solar::{SolarPvProblem, solve_solar_pv};
    pub use crate::solver_settings::SolverSettings;
    pub use crate::structural::{
        BucklingProblem, ColumnEndCondition, CompositeFailureProblem, CompositeLaminateProblem,
        FatigueProblem, LaminaStrengthLimits, OrthotropicLamina, solve_column_buckling,
        solve_composite_failure, solve_composite_laminate, solve_fatigue_safety,
    };
    pub use crate::thermal_stress::{
        BimetalStripProblem, ThermalStressProblem, solve_bimetal_deflection, solve_thermal_stress,
    };
    pub use crate::validation::{AnalyticalComparison, ValidationStatus};
}

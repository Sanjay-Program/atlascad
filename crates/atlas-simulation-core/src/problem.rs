//! Simulation problem definitions.

use crate::boundary_conditions::BoundaryCondition;
use crate::materials::SimulationMaterial;
use crate::solver_settings::SolverSettings;
use serde::{Deserialize, Serialize};

/// Specific engineering discipline and physics type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnalysisType {
    /// Linear static finite element analysis.
    LinearStaticFea,
    /// Modal dynamic natural frequencies and mode shapes.
    ModalVibration,
    /// Steady-state thermal conduction.
    SteadyThermal,
    /// Transient thermal conduction.
    TransientThermal,
    /// Incompressible Navier-Stokes CFD.
    IncompressibleCfd,
    /// Compressible external aerodynamics.
    CompressibleAero,
    /// Balanced 3-phase AC load flow.
    PowerLoadFlow,
    /// SPICE analog/digital circuit simulation.
    SpiceCircuit,
    /// Multi-joint rigid/flexible contact dynamics.
    RoboticsDynamics,
    /// Pressurized pipe network distribution hydraulics.
    WaterNetworkHydraulics,
}

/// A complete, self-contained simulation study definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SimulationProblem {
    pub problem_id: String,
    pub name: String,
    pub analysis_type: AnalysisType,
    pub target_engine: Option<String>,
    pub material: SimulationMaterial,
    pub boundary_conditions: Vec<BoundaryCondition>,
    pub settings: SolverSettings,
}

impl SimulationProblem {
    /// Construct a standard cantilever beam static FEA problem.
    pub fn standard_cantilever_fea(id: impl Into<String>) -> Self {
        Self {
            problem_id: id.into(),
            name: "Cantilever Beam Static FEA".to_string(),
            analysis_type: AnalysisType::LinearStaticFea,
            target_engine: Some("calculix".to_string()),
            material: SimulationMaterial::structural_steel(),
            boundary_conditions: vec![
                BoundaryCondition::FixedSupport {
                    surface_id: "face_fixed_x0".to_string(),
                },
                BoundaryCondition::ForceLoad {
                    node_id: "tip_node".to_string(),
                    force_n: [0.0, 0.0, -1000.0],
                },
            ],
            settings: SolverSettings::default(),
        }
    }
}

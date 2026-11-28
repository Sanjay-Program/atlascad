//! Standardized engineering engine integration contract and runtime registry.
//!
//! Provides the typed integration boundaries across CAD, CAE, CAM, ECAD, BIM,
//! and scientific computing solvers.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::Path;

/// The unique identifier of an engineering engine.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct EngineId(pub String);

impl EngineId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// The multidisciplinary domain of an engineering engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineDiscipline {
    Drafting,
    Mechanical,
    Assembly,
    Simulation,
    Cfd,
    Bim,
    Civil,
    Power,
    Electronics,
    Chemical,
    Robotics,
    Aerospace,
    Manufacturing,
    Environmental,
}

impl EngineDiscipline {
    pub const fn label(self) -> &'static str {
        match self {
            Self::Drafting => "Drafting & 2D CAD",
            Self::Mechanical => "Mechanical CAD",
            Self::Assembly => "Assemblies & Mechanisms",
            Self::Simulation => "Structural FEA & Multiphysics",
            Self::Cfd => "Fluid Dynamics & Aerodynamics",
            Self::Bim => "Architecture & OpenBIM",
            Self::Civil => "Civil & Infrastructure",
            Self::Power => "Electrical Power Systems",
            Self::Electronics => "Electronics & ECAD",
            Self::Chemical => "Chemical & Process Engineering",
            Self::Robotics => "Robotics & Kinematics",
            Self::Aerospace => "Aerospace & Flight Dynamics",
            Self::Manufacturing => "Manufacturing & CNC CAM",
            Self::Environmental => "Geoscience & Water Hydraulics",
        }
    }
}

/// Legal license classification and required architectural isolation tier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseTier {
    /// Permissive (MIT, Apache-2.0, BSD, Public Domain). Safe for in-process linkage.
    TierAPermissive,
    /// Weak Copyleft (LGPL). Permitted via dynamic C ABI linking.
    TierBWeakCopyleft,
    /// Strong Copyleft (GPL-2.0, GPL-3.0). Mandatory process isolation via IPC/CLI.
    TierCStrongCopyleft,
    /// Network Copyleft (AGPL-3.0). Must run as standalone isolated network service.
    TierDNetworkCopyleft,
    /// Specialized open source or proprietary license.
    TierESpecialized,
}

/// Execution model and runtime containment boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionModel {
    InProcess,
    CAbi,
    ProcessIsolated,
    RemoteWorker,
}

/// Supported functional engineering capabilities.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineCapability {
    CadBrep,
    MeshGeneration,
    FeaStatic,
    FeaModal,
    FeaThermal,
    CfdIncompressible,
    Aerodynamics,
    BimIfc,
    SpiceSimulation,
    PcbDrc,
    PowerFlow,
    ChemicalKinetics,
    ProcessFlowsheet,
    MolecularConformers,
    AircraftGeometry,
    FlightDynamics,
    RobotKinematics,
    CncToolpaths,
    WaterHydraulics,
    UrbanRunoff,
    GroundwaterFlow,
}

/// Descriptive metadata and health status of an integrated engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineInfo {
    pub id: EngineId,
    pub name: String,
    pub discipline: EngineDiscipline,
    pub version: String,
    pub license: String,
    pub license_tier: LicenseTier,
    pub execution_model: ExecutionModel,
    pub capabilities: Vec<EngineCapability>,
    pub available: bool,
    pub binary_path: Option<String>,
    pub description: String,
}

/// Job execution request delivered to an engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobRequest {
    pub job_id: String,
    pub engine_id: EngineId,
    pub job_type: Option<String>,
    pub model_revision: u64,
    pub parameters: serde_json::Value,
    pub timeout_seconds: u64,
}

/// Job output and provenance record returned by an engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineJobResult {
    pub job_id: String,
    pub engine_id: EngineId,
    pub success: bool,
    pub elapsed_ms: u64,
    pub output: serde_json::Value,
    pub error_message: Option<String>,
    pub warnings: Vec<String>,
    pub provenance_hash: String,
}

/// Diagnostic health status report for an engine.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EngineHealthReport {
    pub engine_id: EngineId,
    pub healthy: bool,
    pub message: String,
    pub detected_version: Option<String>,
    pub ping_ms: u64,
}

/// Runtime registry managing built-in and external engineering engines.
#[derive(Debug, Clone)]
pub struct EngineRegistry {
    engines: BTreeMap<EngineId, EngineInfo>,
}

impl Default for EngineRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl EngineRegistry {
    /// Initialize the standard AtlasCAD engineering engine catalog.
    pub fn new() -> Self {
        let mut reg = Self {
            engines: BTreeMap::new(),
        };
        reg.register_builtin_engines();
        reg.register_adapted_engines();
        reg
    }

    fn register_builtin_engines(&mut self) {
        self.register(EngineInfo {
            id: EngineId::new("truck"),
            name: "truck B-Rep CAD Kernel".to_string(),
            discipline: EngineDiscipline::Mechanical,
            version: "0.6.0 (Patched Shapeops)".to_string(),
            license: "Apache-2.0".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::InProcess,
            capabilities: vec![EngineCapability::CadBrep],
            available: true,
            binary_path: None,
            description: "Pure-Rust boundary-representation solid geometric modeling kernel."
                .to_string(),
        });

        self.register(EngineInfo {
            id: EngineId::new("cad-cae"),
            name: "AtlasCAD Native Multiphysics CAE".to_string(),
            discipline: EngineDiscipline::Simulation,
            version: "0.1.0".to_string(),
            license: "AtlasCAD Native (Permissive)".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::InProcess,
            capabilities: vec![
                EngineCapability::FeaStatic,
                EngineCapability::FeaModal,
                EngineCapability::FeaThermal,
            ],
            available: true,
            binary_path: None,
            description: "Built-in finite element solver for linear static stress, natural frequency modal vibration, and thermal conduction.".to_string(),
        });

        self.register(EngineInfo {
            id: EngineId::new("cad-cam"),
            name: "AtlasCAD 3-Axis CNC CAM".to_string(),
            discipline: EngineDiscipline::Manufacturing,
            version: "0.1.0".to_string(),
            license: "AtlasCAD Native (Permissive)".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::InProcess,
            capabilities: vec![EngineCapability::CncToolpaths],
            available: true,
            binary_path: None,
            description: "Built-in 3-axis CNC milling toolpath generation, Taylor tool-life feeds & speeds, and G-code postprocessing.".to_string(),
        });
    }

    fn register_adapted_engines(&mut self) {
        // Open CASCADE
        self.register(EngineInfo {
            id: EngineId::new("occt"),
            name: "Open CASCADE Technology (OCCT)".to_string(),
            discipline: EngineDiscipline::Mechanical,
            version: "8.0.1".to_string(),
            license: "LGPL-2.1 with Exception".to_string(),
            license_tier: LicenseTier::TierBWeakCopyleft,
            execution_model: ExecutionModel::CAbi,
            capabilities: vec![EngineCapability::CadBrep],
            available: true,
            // A C++ library reached through its Python bindings: the adapter imports it, so there
            // is no executable or shared object at a fixed path.
            binary_path: None,
            description: "Industry-standard B-Rep surface modeling, STEP AP242, and IGES data exchange engine.".to_string(),
        });

        // Gmsh
        self.register(EngineInfo {
            id: EngineId::new("gmsh"),
            name: "Gmsh Mesh Generator".to_string(),
            discipline: EngineDiscipline::Simulation,
            version: "4.13.1".to_string(),
            license: "GPL-2.0+".to_string(),
            license_tier: LicenseTier::TierCStrongCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::MeshGeneration],
            available: true,
            binary_path: Some("/usr/bin/gmsh".to_string()),
            description: "High-performance automated 2D and 3D finite element mesh generator."
                .to_string(),
        });

        // CalculiX
        self.register(EngineInfo {
            id: EngineId::new("calculix"),
            name: "CalculiX (`ccx`) Non-Linear FEA".to_string(),
            discipline: EngineDiscipline::Simulation,
            version: "2.21".to_string(),
            license: "GPL-2.0".to_string(),
            license_tier: LicenseTier::TierCStrongCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![
                EngineCapability::FeaStatic,
                EngineCapability::FeaModal,
                EngineCapability::FeaThermal,
            ],
            available: true,
            binary_path: Some("/usr/bin/ccx".to_string()),
            description: "Implicit non-linear finite element structural and thermal solver."
                .to_string(),
        });

        // OpenFOAM
        self.register(EngineInfo {
            id: EngineId::new("openfoam"),
            name: "OpenFOAM CFD Solver".to_string(),
            discipline: EngineDiscipline::Cfd,
            version: "v11".to_string(),
            license: "GPL-3.0".to_string(),
            license_tier: LicenseTier::TierCStrongCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::CfdIncompressible],
            available: true,
            binary_path: Some("/opt/openfoam11/bin/simpleFoam".to_string()),
            description:
                "Computational fluid dynamics, turbulence modeling, and multiphase flow simulation."
                    .to_string(),
        });

        // SU2
        self.register(EngineInfo {
            id: EngineId::new("su2"),
            name: "SU2 Aerodynamics & Flow".to_string(),
            discipline: EngineDiscipline::Aerospace,
            version: "8.0.1".to_string(),
            license: "LGPL-2.1".to_string(),
            license_tier: LicenseTier::TierBWeakCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::Aerodynamics],
            available: true,
            binary_path: Some("/usr/bin/SU2_CFD".to_string()),
            description: "Compressible aerodynamics, aircraft lift/drag calculation, and aerodynamic shape optimization.".to_string(),
        });

        // IfcOpenShell
        self.register(EngineInfo {
            id: EngineId::new("ifcopenshell"),
            name: "IfcOpenShell BIM Engine".to_string(),
            discipline: EngineDiscipline::Bim,
            version: "0.7.0".to_string(),
            license: "LGPL-3.0".to_string(),
            license_tier: LicenseTier::TierBWeakCopyleft,
            execution_model: ExecutionModel::CAbi,
            capabilities: vec![EngineCapability::BimIfc],
            available: true,
            // A Python/C++ toolkit: the adapter imports it in a separate interpreter.
            binary_path: None,
            description:
                "OpenBIM parser, geometry extractor, and serializer for IFC2x3 and IFC4 schemas."
                    .to_string(),
        });

        // OpenSees
        self.register(EngineInfo {
            id: EngineId::new("opensees"),
            name: "OpenSees Structural Framework".to_string(),
            discipline: EngineDiscipline::Civil,
            version: "3.8.0.0".to_string(),
            license: "BSD-style (Permissive)".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::FeaStatic],
            available: true,
            // The OpenSeesPy library adapter runs in a separate interpreter; there is no binary.
            binary_path: None,
            description: "Earthquake engineering, non-linear structural frame analysis, and seismic response simulation.".to_string(),
        });

        // EnergyPlus
        self.register(EngineInfo {
            id: EngineId::new("energyplus"),
            name: "EnergyPlus Building Energy Engine".to_string(),
            discipline: EngineDiscipline::Bim,
            version: "24.1.0".to_string(),
            license: "BSD-3-Clause".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::FeaThermal],
            available: true,
            binary_path: Some("/usr/bin/energyplus".to_string()),
            description:
                "Whole-building annual energy simulation, thermal comfort modeling, and HVAC loads."
                    .to_string(),
        });

        // ngspice
        self.register(EngineInfo {
            id: EngineId::new("ngspice"),
            name: "ngspice Circuit Simulator".to_string(),
            discipline: EngineDiscipline::Electronics,
            version: "43".to_string(),
            license: "BSD-3-Clause / LGPL".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::CAbi,
            capabilities: vec![EngineCapability::SpiceSimulation],
            available: true,
            binary_path: Some("/usr/bin/ngspice".to_string()),
            description: "Mixed-signal analog, digital, and RF SPICE electronic circuit simulator."
                .to_string(),
        });

        // KiCad CLI
        self.register(EngineInfo {
            id: EngineId::new("kicad"),
            name: "KiCad PCB Engine".to_string(),
            discipline: EngineDiscipline::Electronics,
            version: "8.0.2".to_string(),
            license: "GPL-3.0+".to_string(),
            license_tier: LicenseTier::TierCStrongCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::PcbDrc],
            available: true,
            binary_path: Some("/usr/bin/kicad-cli".to_string()),
            description:
                "Schematic parity validation, PCB design rule checks (DRC), and Gerber export."
                    .to_string(),
        });

        // pandapower
        self.register(EngineInfo {
            id: EngineId::new("pandapower"),
            name: "pandapower Electric Grid Engine".to_string(),
            discipline: EngineDiscipline::Power,
            version: "3.2.1".to_string(),
            license: "BSD-3-Clause".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::PowerFlow],
            available: true,
            // A Python library: the adapter runs it in a separate interpreter, so there is no
            // executable at a fixed path.
            binary_path: None,
            description: "Balanced 3-phase AC power flow, short-circuit, and state estimation for transmission & distribution grids.".to_string(),
        });

        // OpenDSS
        self.register(EngineInfo {
            id: EngineId::new("opendss"),
            name: "OpenDSS Distribution Grid Solver".to_string(),
            discipline: EngineDiscipline::Power,
            version: "9.6.1".to_string(),
            license: "BSD-3-Clause".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::CAbi,
            capabilities: vec![EngineCapability::PowerFlow],
            available: true,
            binary_path: Some("/usr/lib/libdss_capi.so".to_string()),
            description: "High-speed electrical power distribution system simulation, harmonics, and DER integration.".to_string(),
        });

        // Cantera
        self.register(EngineInfo {
            id: EngineId::new("cantera"),
            name: "Cantera Chemical Kinetics".to_string(),
            discipline: EngineDiscipline::Chemical,
            version: "3.2.0".to_string(),
            license: "BSD-3-Clause".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            // A Python library, driven by atlas-engine-manager::cantera in a separate interpreter.
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::ChemicalKinetics],
            available: true,
            binary_path: None,
            description:
                "Chemical kinetics, thermodynamic equilibrium, and 1D laminar combustion flames."
                    .to_string(),
        });

        // DWSIM
        self.register(EngineInfo {
            id: EngineId::new("dwsim"),
            name: "DWSIM Chemical Process Flowsheet".to_string(),
            discipline: EngineDiscipline::Chemical,
            version: "8.6.0".to_string(),
            license: "GPL-3.0".to_string(),
            license_tier: LicenseTier::TierCStrongCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::ProcessFlowsheet],
            available: true,
            binary_path: Some("/usr/bin/dwsim".to_string()),
            description: "Steady-state chemical process flowsheet simulation, distillation columns, and heat exchanger networks.".to_string(),
        });

        // RDKit
        self.register(EngineInfo {
            id: EngineId::new("rdkit"),
            name: "RDKit Cheminformatics & Molecular Modeling".to_string(),
            discipline: EngineDiscipline::Chemical,
            version: "2024.03.1".to_string(),
            license: "BSD-3-Clause".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::CAbi,
            capabilities: vec![EngineCapability::MolecularConformers],
            available: true,
            binary_path: None,
            description: "2D chemical structure parsing, 3D conformer optimization, and molecular descriptor calculation.".to_string(),
        });

        // OpenVSP
        self.register(EngineInfo {
            id: EngineId::new("openvsp"),
            name: "OpenVSP Parametric Aircraft Geometry".to_string(),
            discipline: EngineDiscipline::Aerospace,
            version: "3.38.0".to_string(),
            license: "NOSA 1.3".to_string(),
            license_tier: LicenseTier::TierESpecialized,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::AircraftGeometry],
            available: true,
            binary_path: Some("/usr/bin/vsp".to_string()),
            description: "Parametric aircraft geometric modeling, fuselage/wing generation, and vortex lattice aerodynamic evaluation.".to_string(),
        });

        // JSBSim
        self.register(EngineInfo {
            id: EngineId::new("jsbsim"),
            name: "JSBSim Flight Dynamics Model".to_string(),
            discipline: EngineDiscipline::Aerospace,
            version: "1.2.0".to_string(),
            license: "LGPL-2.1".to_string(),
            license_tier: LicenseTier::TierBWeakCopyleft,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::FlightDynamics],
            available: true,
            binary_path: Some("/usr/bin/JSBSim".to_string()),
            description: "6-DOF non-linear flight dynamics, aerodynamic control surfaces, and atmospheric trajectory simulation.".to_string(),
        });

        // MuJoCo
        self.register(EngineInfo {
            id: EngineId::new("mujoco"),
            name: "MuJoCo Articulated Dynamics".to_string(),
            discipline: EngineDiscipline::Robotics,
            version: "3.1.5".to_string(),
            license: "Apache-2.0".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            // MuJoCo is a Python library, not an AtlasCAD in-process integrator.
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::RobotKinematics],
            available: true,
            binary_path: None,
            description: "Multi-joint contact physics, forward/inverse robot dynamics, and kinematic simulation.".to_string(),
        });

        // EPANET
        self.register(EngineInfo {
            id: EngineId::new("epanet"),
            name: "EPANET Pipe Network Hydraulics".to_string(),
            discipline: EngineDiscipline::Environmental,
            version: "2.2.0".to_string(),
            license: "Public Domain / CC0".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            // EPANET is an external library/toolkit, not an AtlasCAD in-process solver.
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::WaterHydraulics],
            available: true,
            binary_path: None,
            description: "Pressurized drinking water distribution pipe networks, hydraulic head loss, and water quality age modeling.".to_string(),
        });

        // EPA SWMM
        self.register(EngineInfo {
            id: EngineId::new("swmm"),
            name: "EPA SWMM Urban Stormwater Hydraulics".to_string(),
            discipline: EngineDiscipline::Environmental,
            version: "5.2.4".to_string(),
            license: "Public Domain / CC0".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            // EPA SWMM is an external library/toolkit, not an AtlasCAD in-process solver.
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::UrbanRunoff],
            available: true,
            binary_path: None,
            description: "Urban rainfall runoff, sewer network drainage, retention basins, and flood hydrograph simulation.".to_string(),
        });

        // MODFLOW
        self.register(EngineInfo {
            id: EngineId::new("modflow"),
            name: "MODFLOW 6 Groundwater Flow".to_string(),
            discipline: EngineDiscipline::Environmental,
            version: "6.8.1".to_string(),
            license: "Public Domain (USGS)".to_string(),
            license_tier: LicenseTier::TierAPermissive,
            execution_model: ExecutionModel::ProcessIsolated,
            capabilities: vec![EngineCapability::GroundwaterFlow],
            available: true,
            binary_path: None,
            description: "3D finite-difference groundwater flow, aquifer depletion, and subsurface hydraulic head modeling.".to_string(),
        });
    }

    /// Register an engine description in the catalog.
    pub fn register(&mut self, info: EngineInfo) {
        self.engines.insert(info.id.clone(), info);
    }

    /// Retrieve all registered engineering engines.
    pub fn list(&self) -> Vec<EngineInfo> {
        self.engines.values().cloned().collect()
    }

    /// Retrieve a specific engine by ID.
    pub fn get(&self, id: &EngineId) -> Option<&EngineInfo> {
        self.engines.get(id)
    }

    /// Test the execution health and operational readiness of an engine.
    pub fn test_health(&self, id: &EngineId) -> EngineHealthReport {
        let engine = match self.engines.get(id) {
            Some(e) => e,
            None => {
                return EngineHealthReport {
                    engine_id: id.clone(),
                    healthy: false,
                    message: "Engine ID is not registered in AtlasCAD registry".to_string(),
                    detected_version: None,
                    ping_ms: 0,
                };
            }
        };

        match engine.execution_model {
            ExecutionModel::InProcess => EngineHealthReport {
                engine_id: id.clone(),
                healthy: true,
                message: format!(
                    "Engine '{}' ready in-process (AtlasCAD runtime)",
                    engine.name
                ),
                detected_version: Some(engine.version.clone()),
                ping_ms: 1,
            },
            ExecutionModel::CAbi | ExecutionModel::ProcessIsolated => {
                let bin_path = engine.binary_path.as_deref().unwrap_or("");
                let exists = !bin_path.is_empty() && Path::new(bin_path).exists();
                EngineHealthReport {
                    engine_id: id.clone(),
                    healthy: false,
                    message: format!(
                        "No execution adapter is implemented for '{}'; a binary at '{}' is {} and no job can run",
                        engine.name,
                        bin_path,
                        if exists { "present" } else { "absent" }
                    ),
                    detected_version: None,
                    ping_ms: 0,
                }
            }
            ExecutionModel::RemoteWorker => EngineHealthReport {
                engine_id: id.clone(),
                healthy: false,
                message: format!(
                    "Remote worker for '{}' is declared in metadata only; no endpoint was probed",
                    engine.name
                ),
                detected_version: None,
                ping_ms: 0,
            },
        }
    }

    /// Execute an engineering computation job through the appropriate engine adapter.
    pub fn execute_job(&self, req: &EngineJobRequest) -> EngineJobResult {
        let start = std::time::Instant::now();
        let engine = match self.engines.get(&req.engine_id) {
            Some(e) => e,
            None => {
                return EngineJobResult {
                    job_id: req.job_id.clone(),
                    engine_id: req.engine_id.clone(),
                    success: false,
                    elapsed_ms: 0,
                    output: serde_json::Value::Null,
                    error_message: Some(format!("Unknown engine ID: '{}'", req.engine_id.as_str())),
                    warnings: Vec::new(),
                    provenance_hash: "0000000000000000".to_string(),
                };
            }
        };

        let elapsed = start.elapsed().as_millis() as u64;
        EngineJobResult {
            job_id: req.job_id.clone(),
            engine_id: req.engine_id.clone(),
            success: false,
            elapsed_ms: elapsed.max(1),
            output: serde_json::Value::Null,
            error_message: Some(format!(
                "no execution adapter is installed for `{}`; no solver was run and no result is reported",
                engine.name
            )),
            warnings: vec![
                "synthetic engine output was removed: a result is only reported when a real process produced it"
                    .to_string(),
            ],
            provenance_hash: "0000000000000000".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Only AtlasCAD's own kernels may claim in-process execution. An external engine marked
    /// in-process used to make `test_health` report it healthy while no adapter could run it.
    #[test]
    fn only_atlascad_own_engines_execute_in_process() {
        let registry = EngineRegistry::new();
        let mut in_process: Vec<String> = registry
            .list()
            .into_iter()
            .filter(|engine| engine.execution_model == ExecutionModel::InProcess)
            .map(|engine| engine.id.as_str().to_string())
            .collect();
        in_process.sort();
        assert_eq!(in_process, vec!["cad-cae", "cad-cam", "truck"]);
    }

    /// Registration is not availability: a library engine with no imported module is unhealthy.
    #[test]
    fn a_library_engine_without_an_adapter_is_not_healthy() {
        let registry = EngineRegistry::new();
        for id in [
            "cantera",
            "pandapower",
            "opensees",
            "mujoco",
            "epanet",
            "swmm",
        ] {
            let report = registry.test_health(&EngineId::new(id));
            assert!(!report.healthy, "{id} must not report healthy");
            assert!(
                report.message.contains("No execution adapter"),
                "{id}: {}",
                report.message
            );
        }
    }
}

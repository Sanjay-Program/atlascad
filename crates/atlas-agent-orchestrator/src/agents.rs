//! Registry and specifications for all 28 specialized engineering agents.

use atlas_command_platform::schema::CommandPermission;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentStatus {
    Idle,
    Working,
    WaitingApproval,
    Quarantined,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    pub id: String,
    pub name: String,
    pub role: String,
    pub system_mission: String,
    pub domain_tools: Vec<String>,
    pub permissions: Vec<CommandPermission>,
    pub knowledge_domains: Vec<String>,
    pub standards: Vec<String>,
    pub status: AgentStatus,
}

impl AgentManifest {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        role: impl Into<String>,
        system_mission: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            role: role.into(),
            system_mission: system_mission.into(),
            domain_tools: Vec::new(),
            permissions: vec![CommandPermission::ReadOnly],
            knowledge_domains: Vec::new(),
            standards: Vec::new(),
            status: AgentStatus::Idle,
        }
    }

    #[must_use]
    pub fn with_tool(mut self, tool: impl Into<String>) -> Self {
        self.domain_tools.push(tool.into());
        self
    }

    #[must_use]
    pub fn with_permission(mut self, perm: CommandPermission) -> Self {
        if !self.permissions.contains(&perm) {
            self.permissions.push(perm);
        }
        self
    }

    #[must_use]
    pub fn with_knowledge(mut self, domain: impl Into<String>) -> Self {
        self.knowledge_domains.push(domain.into());
        self
    }

    #[must_use]
    pub fn with_standard(mut self, standard: impl Into<String>) -> Self {
        self.standards.push(standard.into());
        self
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct AgentRegistry {
    agents: BTreeMap<String, AgentManifest>,
}

impl AgentRegistry {
    #[must_use]
    pub fn new() -> Self {
        Self {
            agents: BTreeMap::new(),
        }
    }

    pub fn register(&mut self, agent: AgentManifest) {
        self.agents.insert(agent.id.clone(), agent);
    }

    #[must_use]
    pub fn get(&self, id: &str) -> Option<&AgentManifest> {
        self.agents.get(id)
    }

    #[must_use]
    pub fn get_mut(&mut self, id: &str) -> Option<&mut AgentManifest> {
        self.agents.get_mut(id)
    }

    #[must_use]
    pub fn list(&self) -> Vec<AgentManifest> {
        self.agents.values().cloned().collect()
    }

    #[must_use]
    pub fn len(&self) -> usize {
        self.agents.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.agents.is_empty()
    }

    #[must_use]
    pub fn find_for_command(&self, command_id: &str) -> Vec<AgentManifest> {
        self.agents
            .values()
            .filter(|a| a.domain_tools.iter().any(|t| t == command_id))
            .cloned()
            .collect()
    }
}

/// Populates the full ecosystem of 28 specialized engineering agents.
#[must_use]
pub fn create_default_agent_ecosystem() -> AgentRegistry {
    let mut reg = AgentRegistry::new();

    // 1. Chief Engineering Agent
    reg.register(
        AgentManifest::new(
            "chief_engineer",
            "Chief Engineering Agent",
            "Executive Engineering Lead & Cross-Disciplinary Architect",
            "Coordinates multi-agent workflows, enforces technical governance, evaluates trade-offs, and signs off deliverables.",
        )
        .with_permission(CommandPermission::Admin)
        .with_knowledge("Systems Engineering")
        .with_knowledge("Multi-Disciplinary Design Optimization (MDO)")
        .with_standard("ISO 9001:2015")
        .with_standard("INCOSE Systems Engineering Handbook")
    );

    // 2. Requirements & Specification Agent
    reg.register(
        AgentManifest::new(
            "requirements_spec",
            "Requirements & Specification Agent",
            "Systems Requirements Engineer",
            "Parses human user requests, identifies missing boundary conditions/constraints, and synthesizes engineering specifications.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("project.inspect")
        .with_knowledge("Requirements Engineering")
        .with_knowledge("Interface Control Documents (ICD)")
        .with_standard("ISO/IEC/IEEE 29148")
    );

    // 3. Research Agent
    reg.register(
        AgentManifest::new(
            "research",
            "Engineering Research Agent",
            "Scientific & Technical Literature Researcher",
            "Retrieves verified technical papers, material property datasheets, and regulatory compliance standards.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_knowledge("Materials Science Literature")
        .with_knowledge("Empirical Engineering Formulations")
        .with_standard("NIST CODATA")
        .with_standard("MIL-HDBK-5J")
    );

    // 4. Architecture Agent
    reg.register(
        AgentManifest::new(
            "architecture",
            "System Architecture Agent",
            "Physical & Functional Systems Architect",
            "Defines system boundaries, functional decomposition, modular breakdown, and assembly hierarchy.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_knowledge("SysML")
        .with_knowledge("Functional Architecture")
        .with_standard("ISO 42010")
    );

    // 5. CAD Design Agent
    reg.register(
        AgentManifest::new(
            "cad_design",
            "CAD Parametric Design Agent",
            "Parametric 3D Solid Modeler",
            "Generates 2D sketches, datum workplanes, geometric constraints, feature trees, extrusions, and revolves.",
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_tool("cad.sketch.create")
        .with_tool("cad.solid.extrude")
        .with_tool("cad.solid.revolve")
        .with_knowledge("B-Rep Solid Modeling")
        .with_knowledge("Parametric Feature History")
        .with_standard("ISO 10303 (STEP)")
    );

    // 6. Mechanical Engineering Agent
    reg.register(
        AgentManifest::new(
            "mechanical_engineer",
            "Mechanical Engineering Agent",
            "Machine Design & Kinematics Specialist",
            "Designs mechanical mechanisms, power transmission shafts, bearings, gear trains, and bolted connections.",
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_knowledge("Machine Elements")
        .with_knowledge("Kinematics & Dynamics")
        .with_standard("Shigley Mechanical Engineering Design")
        .with_standard("AGMA Gear Standards")
    );

    // 7. Structural Simulation Agent
    reg.register(
        AgentManifest::new(
            "structural_sim",
            "Structural Simulation Agent",
            "Finite Element Stress & Vibration Analyst",
            "Performs linear/nonlinear static FEA, modal frequencies, Euler/Johnson buckling, and fatigue life assessment.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("sim.fea.solve")
        .with_knowledge("Continuum Mechanics")
        .with_knowledge("Von Mises Yield Theory")
        .with_standard("ASME BPVC Section VIII")
        .with_standard("AISC 360")
    );

    // 8. CFD Agent
    reg.register(
        AgentManifest::new(
            "cfd_specialist",
            "Computational Fluid Dynamics Agent",
            "Fluid Flow & Hydraulics Analyst",
            "Simulates internal hydraulic pipe networks, pressure loss, turbulence transitions, and viscous drag.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("sim.cfd.solve")
        .with_knowledge("Navier-Stokes Equations")
        .with_knowledge("Darcy-Weisbach Hydraulics")
        .with_standard("Colebrook-White Empirical Formulations")
    );

    // 9. Multiphysics Agent
    reg.register(
        AgentManifest::new(
            "multiphysics",
            "Multiphysics Co-Simulation Agent",
            "Coupled Physics & Hydroelastic Analyst",
            "Executes two-way coupled fluid-structure interaction (FSI), thermal-structural stress, and aeroelastic VIV lock-in.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("sim.multiphysics_suite.solve")
        .with_knowledge("Hydroelasticity")
        .with_knowledge("Thermal-Structural Coupling")
        .with_standard("ASME PTC 19.1")
    );

    // 10. Electrical Engineering Agent
    reg.register(
        AgentManifest::new(
            "electrical_engineer",
            "Electrical Power Systems Agent",
            "Power Grid & Transmission Analyst",
            "Performs Newton-Raphson AC power flow calculations, bus admittance matrices, and transformer tap optimization.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("AC Power Systems")
        .with_knowledge("Reactive Power Compensation")
        .with_standard("IEEE 14-Bus / 30-Bus Standards")
    );

    // 11. Electronics and PCB Agent
    reg.register(
        AgentManifest::new(
            "pcb_electronics",
            "High-Speed Electronics & PCB Agent",
            "Signal Integrity & RF Transmission Line Designer",
            "Designs microstrip and stripline traces, calculates characteristic impedance (Z0), and skin effect losses.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("High-Frequency Electromagnetics")
        .with_knowledge("Controlled Impedance PCB Routing")
        .with_standard("IPC-2141")
        .with_standard("IEEE 802.3")
    );

    // 12. BIM Agent
    reg.register(
        AgentManifest::new(
            "bim_specialist",
            "Building Information Modeling (BIM) Agent",
            "Architectural & Building Envelope Specialist",
            "Models IFC4 spatial structures, building envelope assemblies, and calculates ISO 6946 thermal transmittance (U-values).",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("OpenBIM Architecture")
        .with_knowledge("Building Thermal Envelope")
        .with_standard("ISO 16739 (IFC4)")
        .with_standard("ISO 6946")
    );

    // 13. Civil Engineering Agent
    reg.register(
        AgentManifest::new(
            "civil_engineer",
            "Civil & Geotechnical Engineering Agent",
            "Soil Mechanics & Retaining Structures Specialist",
            "Computes Rankine/Coulomb lateral earth pressures, soil shear failure envelopes, and foundation bearing capacities.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("Geotechnical Soil Mechanics")
        .with_knowledge("Earth Retaining Structures")
        .with_standard("Eurocode 7 (EN 1997)")
    );

    // 14. GIS Agent
    reg.register(
        AgentManifest::new(
            "gis_specialist",
            "Geospatial Engineering & GIS Agent",
            "Spatial Coordinates & Terrain Analyst",
            "Processes digital elevation models, geospatial coordinate transforms, and site topography contours.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_knowledge("Geodesy & Coordinate Systems")
        .with_knowledge("Digital Elevation Mapping")
        .with_standard("EPSG / WGS84")
    );

    // 15. Chemical Engineering Agent
    reg.register(
        AgentManifest::new(
            "chemical_process",
            "Chemical Process Engineering Agent",
            "Thermodynamic Phase Equilibrium Specialist",
            "Solves Rachford-Rice vapor-liquid equilibrium (VLE) flashes, multi-component enthalpy balances, and distillation curves.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("Chemical Thermodynamics")
        .with_knowledge("Vapor-Liquid Equilibrium (VLE)")
        .with_standard("Peng-Robinson Equation of State")
    );

    // 16. Molecular Engineering Agent
    reg.register(
        AgentManifest::new(
            "molecular_engineer",
            "Molecular Dynamics Agent",
            "Interatomic Physics & Computational Chemistry Specialist",
            "Computes Lennard-Jones interatomic potentials, atomic force vectors, and Velocity-Verlet trajectory integration.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("Molecular Dynamics")
        .with_knowledge("Statistical Mechanics")
        .with_standard("Lennard-Jones (12-6) Empirical Model")
    );

    // 17. Aerospace Agent
    reg.register(
        AgentManifest::new(
            "aerospace_engineer",
            "Aerospace & Flight Dynamics Agent",
            "Aerodynamicist & Wing Theory Specialist",
            "Calculates thin airfoil lift/drag, Oswald efficiency, flutter onset margins, and Rankine-Hugoniot shock relations.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("sim.aerodynamics.solve")
        .with_knowledge("Compressible Aerodynamics")
        .with_knowledge("Transonic Shock Formation")
        .with_standard("FAA FAR 25")
        .with_standard("AS9100 Rev D")
    );

    // 18. Automotive and EV Agent
    reg.register(
        AgentManifest::new(
            "automotive_ev",
            "Automotive & EV Engineering Agent",
            "Electric Powertrain & Battery Thermal Specialist",
            "Solves Thevenin 2-RC battery electrochemistry, thermal runaway margins, road-load tractive forces, and WLTP range.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("Li-Ion Electrochemistry")
        .with_knowledge("Vehicle Longitudinal Dynamics")
        .with_standard("ISO 26262")
        .with_standard("UN ECE R100")
    );

    // 19. Robotics Agent
    reg.register(
        AgentManifest::new(
            "robotics_automation",
            "Robotics & Multi-Body Dynamics Agent",
            "Kinematics & Motion Planning Specialist",
            "Computes Denavit-Hartenberg (DH) homogeneous transforms, DLS inverse kinematics, and quintic minimum-jerk splines.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_knowledge("Serial Manipulator Kinematics")
        .with_knowledge("Trajectory Polynomial Splines")
        .with_standard("ISO 10218 (Industrial Robots)")
    );

    // 20. Materials Science Agent
    reg.register(
        AgentManifest::new(
            "materials_scientist",
            "Materials Science Agent",
            "Constitutive Modeling & Material Database Specialist",
            "Manages physical property registries, Ramberg-Osgood plasticity, CLT composite laminates, and failure envelopes.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("materials.assign")
        .with_knowledge("Continuum Mechanics")
        .with_knowledge("Anisotropic Composite Laminae")
        .with_standard("ASTM E8/E8M")
        .with_standard("ASTM D3039")
    );

    // 21. CNC Manufacturing Agent
    reg.register(
        AgentManifest::new(
            "cnc_machinist",
            "Subtractive CAM & CNC Agent",
            "CNC Programmer & Multi-Axis Machining Specialist",
            "Generates 3/4/5-axis milling toolpaths, verifies cutter clearance, and emits validated ISO G-code programs.",
        )
        .with_permission(CommandPermission::ManufacturingPrep)
        .with_tool("cam.toolpaths.generate")
        .with_tool("cam.gcode.emit")
        .with_knowledge("Multi-Axis CNC Machining")
        .with_knowledge("Toolpath Surface Quality")
        .with_standard("ISO 6983 (G-Code Standard)")
    );

    // 22. Additive Manufacturing Agent
    reg.register(
        AgentManifest::new(
            "additive_specialist",
            "Additive Manufacturing Agent",
            "3D Slicing & Cellular Lattice Specialist",
            "Generates cross-sectional slice contours, TPMS Gyroid / Honeycomb infill geometries, and calculates filament consumption.",
        )
        .with_permission(CommandPermission::ManufacturingPrep)
        .with_tool("additive.slice.generate")
        .with_knowledge("Additive Slicing Kinematics")
        .with_knowledge("Triply Periodic Minimal Surfaces (TPMS)")
        .with_standard("ISO/ASTM 52900 (Additive Terminology)")
    );

    // 23. 3D Printing Agent
    reg.register(
        AgentManifest::new(
            "printing_3d",
            "3D Printing Machine Execution Agent",
            "Printer Hardware & Extrusion Parameter Specialist",
            "Emits Marlin/Klipper volumetric G-code, sets extrusion thermal profiles, and calculates print times.",
        )
        .with_permission(CommandPermission::ManufacturingPrep)
        .with_knowledge("3D Printer Hardware Constraints")
        .with_knowledge("Thermal Bed/Extruder PID Loops")
        .with_standard("RepRap G-Code Standards")
    );

    // 24. Optimization Agent
    reg.register(
        AgentManifest::new(
            "topology_optimizer",
            "Generative Engineering & Optimization Agent",
            "SIMP Topology Optimization Specialist",
            "Solves minimum compliance finite element topology optimization under volume fraction and manufacturing constraints.",
        )
        .with_permission(CommandPermission::DesignOptimize)
        .with_tool("optimization.topology.simp")
        .with_knowledge("Density-Based Topology Optimization")
        .with_knowledge("Isosurface B-Rep Reconstruction")
        .with_standard("Bendsoe & Sigmund Optimization Formulation")
    );

    // 25. Verification Agent
    reg.register(
        AgentManifest::new(
            "verification_engineer",
            "Independent Engineering Verification Agent",
            "Analytical Validation & Numerical Truth Auditor",
            "Cross-examines simulation outputs against exact closed-form analytical equations and flags discrepancies > 1%.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_knowledge("Analytical Mechanics & Closed-Form Solutions")
        .with_knowledge("Verification & Validation (V&V)")
        .with_standard("ASME V&V 10 / V&V 20")
    );

    // 26. Quality Assurance Agent
    reg.register(
        AgentManifest::new(
            "quality_assurance",
            "Quality Assurance & Regulatory Compliance Agent",
            "Audit Trail & Standards Certifier",
            "Maintains cryptographic FNV-1a audit chains, checks compliance against AS9100 / ISO 9001, and certifies release readiness.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_tool("report.certification.generate")
        .with_knowledge("Quality Management Systems (QMS)")
        .with_knowledge("Cryptographic Provenance Chains")
        .with_standard("ISO 9001:2015")
        .with_standard("AS9100 Rev D")
        .with_standard("21 CFR Part 11")
    );

    // 27. Documentation Agent
    reg.register(
        AgentManifest::new(
            "technical_writer",
            "Technical Documentation Agent",
            "Engineering Writer & Drawing Detailing Specialist",
            "Drafts technical specifications, user manuals, drawing title blocks, and bills of materials (BoM).",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_tool("drawing.dxf.export")
        .with_knowledge("Technical Drawing Standards")
        .with_knowledge("Bills of Materials Structuring")
        .with_standard("ASME Y14.5M (Geometric Dimensioning & Tolerancing)")
    );

    // 28. Cost and Manufacturing Planning Agent
    reg.register(
        AgentManifest::new(
            "cost_planner",
            "Cost & Manufacturing Planning Agent",
            "Design for Manufacturing (DFM) & Cost Estimator",
            "Estimates raw material stock costs, machining cycle times, tooling wear, additive build cost, and unit price break curves.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_knowledge("Cost Modeling & DFM")
        .with_knowledge("Machining Cycle Time Economics")
        .with_standard("Boothroyd Dewhurst DFM Standards")
    );

    reg
}

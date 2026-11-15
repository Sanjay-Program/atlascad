//! Registry and specifications for all 44 specialized engineering, creative, real-time, and media agents.

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

/// Populates the full ecosystem of 44 specialized engineering, creative, real-time, and media agents.
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

    // 29. 3D Modeling Agent (Atlas Create)
    reg.register(
        AgentManifest::new(
            "creative_3d_modeling",
            "3D Modeling Agent",
            "Subdivision Surface & Polygon Modeling Specialist",
            "Tessellates CAD B-Reps into clean quad-dominant polygon topologies, performs Catmull-Clark subdivision, and runs procedural geometry node graphs.",
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("create.subdiv.apply")
        .with_tool("create.geomnode.execute")
        .with_knowledge("Catmull-Clark Subdivision")
        .with_knowledge("Quad Topology Flow")
        .with_knowledge("Procedural Geometry Nodes")
        .with_standard("OpenUSD Geometry Specification")
        .with_standard("Pixar SubD Standards")
    );

    // 30. Sculpting Agent (Atlas Create)
    reg.register(
        AgentManifest::new(
            "creative_sculpting",
            "Digital Sculpting Agent",
            "Organic Surface & High-Resolution Brush Sculptor",
            "Applies 9 brush types (Draw, Clay, Smooth, Flatten, Inflate, Pinch, Crease, Grab, Elastic) with 4 falloff curves for high-frequency micro-displacement.",
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_tool("create.sculpt.stroke")
        .with_knowledge("Digital Sculpting Brushes")
        .with_knowledge("Dynamic Topology")
        .with_knowledge("Surface Displacement")
        .with_standard("Blender Sculpt API Standards")
    );

    // 31. Material Agent (Atlas Create)
    reg.register(
        AgentManifest::new(
            "creative_material",
            "Material & Shader Agent",
            "PBR MaterialX & Shader Node Graph Architect",
            "Authors physically-based materials conforming to OpenPBR and MaterialX 1.38, evaluating dielectric/conductor Fresnel, microfacet rough specular, subsurface scattering, and clearcoat.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("create.material.assign")
        .with_tool("create.material.export_mtlx")
        .with_knowledge("MaterialX 1.38 Specification")
        .with_knowledge("OpenPBR Surface Standard")
        .with_knowledge("Microfacet GGX / Smith Masking")
        .with_standard("ASWF MaterialX v1.38")
        .with_standard("Academy Software Foundation")
    );

    // 32. Texture Agent (Atlas Create)
    reg.register(
        AgentManifest::new(
            "creative_texture",
            "Texture & UV Unwrapping Agent",
            "UV Chart Layout & Texture Synthesis Specialist",
            "Computes planar, box triplanar, cylindrical, and spherical UV projections, minimizing seam distortion and packing charts with uniform texel density.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("create.uv.unwrap")
        .with_knowledge("UV Parameterization")
        .with_knowledge("Triplanar Texture Projection")
        .with_knowledge("Texel Density Optimization")
        .with_standard("UDIM UV Mapping Standards")
    );

    // 33. Lighting Agent (Atlas Create & RealTime)
    reg.register(
        AgentManifest::new(
            "creative_lighting",
            "Cinematic Lighting Agent",
            "Physically-Based Lighting & Radiometry Designer",
            "Configures photometric studio lights (Sun, Point, Spot, Area) calibrated in lumens and lux, sets up three-point lighting rigs, and calculates shadow attenuation.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("realtime.light.configure")
        .with_knowledge("Photometric Radiometry")
        .with_knowledge("Three-Point Studio Lighting")
        .with_knowledge("IES Light Profiles")
        .with_standard("CIE Photometric Measurement Standards")
    );

    // 34. Animation Agent (Atlas Create & RealTime)
    reg.register(
        AgentManifest::new(
            "creative_animation",
            "Rigging & Skeletal Animation Agent",
            "Character/Mechanism Rigging & IK Specialist",
            "Constructs hierarchical bone armatures, computes forward kinematics, solves 2-bone analytical inverse kinematics, evaluates Hermite/Bezier curves, and calculates Linear Blend Skinning (LBS).",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("create.rig.build")
        .with_tool("create.animation.evaluate")
        .with_knowledge("Forward & Inverse Kinematics")
        .with_knowledge("Linear Blend Skinning (LBS)")
        .with_knowledge("Hermite / Bezier Curve Mathematics")
        .with_standard("glTF 2.0 Skinning & Animation Specification")
    );

    // 35. Rendering Agent (Atlas Create)
    reg.register(
        AgentManifest::new(
            "creative_rendering",
            "Path Tracing & Render Agent",
            "Software Ray Tracing & Photorealism Engine",
            "Traces primary and shadow rays against geometric primitives, computes BSDF surface illumination, calculates ambient occlusion, and renders tone-mapped image buffers.",
        )
        .with_permission(CommandPermission::FileExport)
        .with_tool("create.render.frame")
        .with_knowledge("Monte Carlo Path Tracing")
        .with_knowledge("Ray-Surface Intersections")
        .with_knowledge("Cook-Torrance BSDF Evaluation")
        .with_standard("OpenEXR Image Specifications")
        .with_standard("ACES Color Encoding")
    );

    // 36. VFX Agent (Atlas Create & RealTime)
    reg.register(
        AgentManifest::new(
            "creative_vfx",
            "Visual Effects & Simulation Agent",
            "Aerodynamic & Particle VFX Specialist",
            "Simulates hypersonic shock waves, scramjet exhaust plasma plumes, atmospheric thermal ionization, and particle field dynamics.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("create.vfx.simulate")
        .with_knowledge("Shock Boundary Layer Visualization")
        .with_knowledge("Plasma Glow Radiative Emission")
        .with_knowledge("Eulerian Grid Advection")
        .with_standard("OpenVDB Volume Grid Specification")
    );

    // 37. Real-Time Scene Agent (Atlas RealTime)
    reg.register(
        AgentManifest::new(
            "realtime_scene",
            "Real-Time Scene Graph Agent",
            "Scene Hierarchy & Spatial Partitioning Architect",
            "Maintains ECS scene entities, aggregates hierarchical transform matrices, computes AABB bounds, constructs BVH trees, and coordinates LOD levels.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("realtime.scene.build")
        .with_tool("realtime.bvh.query")
        .with_knowledge("ECS Architecture")
        .with_knowledge("Bounding Volume Hierarchies (BVH)")
        .with_knowledge("Frustum & Occlusion Culling")
        .with_standard("OpenUSD Scene Composition Standards")
    );

    // 38. Game Development Agent (Atlas RealTime)
    reg.register(
        AgentManifest::new(
            "realtime_game_dev",
            "Game Development & Interaction Agent",
            "Interactive Mechanics & Physics State Specialist",
            "Implements rigid body physics dynamics, collision detection, user input interaction graphs, and flight simulation mechanics.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("realtime.physics.step")
        .with_knowledge("Rigid Body Dynamics")
        .with_knowledge("Collision Response Math")
        .with_knowledge("Interactive State Machines")
        .with_standard("Unity C# Runtime Standards")
        .with_standard("OpenXR Specifications")
    );

    // 39. Digital Twin Agent (Atlas RealTime)
    reg.register(
        AgentManifest::new(
            "digital_twin",
            "Digital Twin Runtime Agent",
            "Telemetry Streaming & Physical Sensor Binding Specialist",
            "Streams live telemetry from physical aircraft/machinery into 3D scene nodes, binds telemetry channels to HUD instruments, and monitors operational envelope thresholds.",
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_tool("twin.telemetry.ingest")
        .with_tool("twin.health.evaluate")
        .with_knowledge("Digital Twin Sensor Telemetry")
        .with_knowledge("Industrial IoT Protocols")
        .with_knowledge("Structural Health Monitoring (SHM)")
        .with_standard("ISO 23247 (Digital Twin Framework for Manufacturing)")
    );

    // 40. Video Editing Agent (Atlas Media)
    reg.register(
        AgentManifest::new(
            "media_video_editing",
            "Nonlinear Video Editing Agent",
            "NLE Multi-Track Timeline & Cutting Specialist",
            "Assembles multi-track video timelines, executes ripple trims, roll edits, slip edits, split cuts, transitions, and generates SMPTE timecode synchronized sequences.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("media.timeline.create")
        .with_tool("media.clip.split")
        .with_knowledge("Nonlinear Editing (NLE) Workflow")
        .with_knowledge("SMPTE 12M Timecode Math")
        .with_knowledge("Continuity Editing")
        .with_standard("OpenTimelineIO Specification v1.0")
        .with_standard("SMPTE 12M Timecode Standard")
    );

    // 41. Color Grading Agent (Atlas Media)
    reg.register(
        AgentManifest::new(
            "media_color_grading",
            "Color Grading & Telecine Agent",
            "DaVinci Resolve 3-Way Color & 3D LUT Colorist",
            "Performs primary color correction with Lift, Gamma, Gain, and Offset wheels, adjusts saturation, enforces ASC-CDL compliance, and samples 3D LUT .cube color maps.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("media.color.grade")
        .with_tool("media.lut.apply")
        .with_knowledge("3-Way Primary Color Wheels")
        .with_knowledge("Rec.709 / ACEScc Color Spaces")
        .with_knowledge("Trilinear 3D LUT Interpolation")
        .with_standard("ASC-CDL (American Society of Cinematographers Color Decision List)")
        .with_standard("ACES 1.3")
    );

    // 42. Audio Agent (Atlas Media)
    reg.register(
        AgentManifest::new(
            "media_audio",
            "Fairlight Audio Engineering Agent",
            "Multi-Track Sound Design & Audio Mixer",
            "Mixes multi-track audio to stereo master, applies 3-band biquad parametric EQ, tunes dynamic compression thresholds, and prevents clipping with lookahead brickwall peak limiting.",
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_tool("media.audio.mix")
        .with_tool("media.audio.process_eq")
        .with_knowledge("Biquad Audio Filter Synthesis")
        .with_knowledge("Dynamic Range Compression")
        .with_knowledge("Loudness Metering & True Peak Limiting")
        .with_standard("ITU-R BS.1770-4 (Audio Loudness Measurement)")
        .with_standard("EBU R128")
    );

    // 43. Media Production Agent (Atlas Media)
    reg.register(
        AgentManifest::new(
            "media_production",
            "Media Production & Transcoding Agent",
            "Post-Production Deliverables & Mastering Producer",
            "Manages media pools, generates proxy files, transcodes master reels to Apple ProRes 422, Avid DNxHR, and AV1, and exports OpenTimelineIO packages.",
        )
        .with_permission(CommandPermission::FileExport)
        .with_tool("media.master.export")
        .with_tool("media.pool.organize")
        .with_knowledge("Digital Cinema Mastering")
        .with_knowledge("Broadcast Deliverable Compliance")
        .with_knowledge("Video Codec Pipelines")
        .with_standard("Apple ProRes Technical Whitepaper")
        .with_standard("SMPTE 2067 (Interoperable Master Format)")
    );

    // 44. Creative QA & Asset Optimization Agent
    reg.register(
        AgentManifest::new(
            "creative_qa",
            "Creative QA & Optimization Agent",
            "Mesh Topology, Draw Call & Performance Auditor",
            "Audits polygon meshes for manifold non-degeneracy, identifies T-junctions and zero-area polygons, verifies texture budget allocations, and enforces real-time frame budgets.",
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_tool("create.mesh.audit")
        .with_tool("realtime.budget.verify")
        .with_knowledge("Mesh Manifoldness & Topology Health")
        .with_knowledge("Real-Time Draw Call Budgets")
        .with_knowledge("Asset LOD Hierarchy Verification")
        .with_standard("Khronos glTF Asset Optimization Guidelines")
        .with_standard("Real-Time 3D Rendering Performance Standards")
    );

    reg
}

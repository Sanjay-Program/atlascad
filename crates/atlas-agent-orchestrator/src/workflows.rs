//! Reusable engineering workflow templates for autonomous multi-agent execution.

use crate::error::OrchestratorError;
use crate::task_graph::{EngineeringTask, TaskGraph};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowTemplateInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: String,
    pub required_agents: Vec<String>,
    pub task_count: usize,
}

pub struct WorkflowRegistry;

impl WorkflowRegistry {
    #[must_use]
    pub fn list_templates() -> Vec<WorkflowTemplateInfo> {
        vec![
            WorkflowTemplateInfo {
                id: "concept_to_cad".into(),
                name: "Product Concept to 3D Parametric CAD".into(),
                description: "Translates functional envelope into 2D sketches, datum workplanes, and 3D B-Rep solid features.".into(),
                category: "CAD".into(),
                required_agents: vec!["requirements_spec".into(), "cad_design".into(), "mechanical_engineer".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "cad_to_fea".into(),
                name: "CAD to Finite Element Stress Analysis".into(),
                description: "Discretizes 3D solid geometry into elements, applies boundary constraints, and solves linear static stress & modal modes.".into(),
                category: "Simulation".into(),
                required_agents: vec!["cad_design".into(), "materials_scientist".into(), "structural_sim".into(), "verification_engineer".into()],
                task_count: 5,
            },
            WorkflowTemplateInfo {
                id: "cad_to_cfd".into(),
                name: "CAD to Computational Fluid Dynamics".into(),
                description: "Extracts fluid domain, models boundary layer turbulence, and computes hydraulic head loss & pressure distribution.".into(),
                category: "Simulation".into(),
                required_agents: vec!["cad_design".into(), "cfd_specialist".into(), "verification_engineer".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "cad_to_optimization".into(),
                name: "CAD to Generative SIMP Topology Optimization".into(),
                description: "Optimizes structural mass distribution under load cases, minimizes compliance, and reconstructs 3D boundary geometry.".into(),
                category: "Optimization".into(),
                required_agents: vec!["structural_sim".into(), "topology_optimizer".into(), "cad_design".into()],
                task_count: 5,
            },
            WorkflowTemplateInfo {
                id: "cad_to_additive".into(),
                name: "CAD to Additive Slicing & 3D Printing".into(),
                description: "Performs planar triangle-mesh slicing, generates TPMS Gyroid infill, and exports machine G-code.".into(),
                category: "Manufacturing".into(),
                required_agents: vec!["cad_design".into(), "additive_specialist".into(), "printing_3d".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "cad_to_cnc".into(),
                name: "CAD to 5-Axis CNC Machining".into(),
                description: "Plans multi-axis facing, contouring, and rotary toolpaths with tool-holder collision detection and ISO G-code.".into(),
                category: "Manufacturing".into(),
                required_agents: vec!["cad_design".into(), "cnc_machinist".into(), "cost_planner".into()],
                task_count: 5,
            },
            WorkflowTemplateInfo {
                id: "bim_to_energy".into(),
                name: "BIM Architectural to Building Energy Simulation".into(),
                description: "Parses IFC4 spatial elements, evaluates envelope U-values according to ISO 6946, and computes annual thermal heating/cooling loads.".into(),
                category: "Architecture & BIM".into(),
                required_agents: vec!["architecture".into(), "bim_specialist".into(), "verification_engineer".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "circuit_to_spice".into(),
                name: "Circuit Schematic to SPICE Simulation".into(),
                description: "Constructs analog circuit netlist, performs AC frequency sweep, and calculates filter cutoff and phase margins.".into(),
                category: "Electronics".into(),
                required_agents: vec!["electrical_engineer".into(), "pcb_electronics".into()],
                task_count: 3,
            },
            WorkflowTemplateInfo {
                id: "pcb_to_si".into(),
                name: "High-Speed PCB to Controlled Impedance & Signal Integrity".into(),
                description: "Models microstrip/stripline cross sections via IPC-2141, calculates propagation delay, and checks transmission line length.".into(),
                category: "Electronics".into(),
                required_agents: vec!["pcb_electronics".into(), "materials_scientist".into()],
                task_count: 3,
            },
            WorkflowTemplateInfo {
                id: "robot_to_kinematics".into(),
                name: "Robotic Manipulator to 6-DOF Trajectory Planning".into(),
                description: "Formulates Denavit-Hartenberg frames, solves damped least-squares inverse kinematics, and plans quintic minimum-jerk splines.".into(),
                category: "Robotics".into(),
                required_agents: vec!["robotics_automation".into(), "mechanical_engineer".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "ev_to_battery_thermal".into(),
                name: "EV Powertrain to Battery Electrochemistry & Thermal Runaway".into(),
                description: "Models 2-RC Thevenin battery pack dynamics, evaluates Arrhenius thermal runaway onset, and estimates WLTP cruising range.".into(),
                category: "Automotive".into(),
                required_agents: vec!["automotive_ev".into(), "multiphysics".into(), "verification_engineer".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "req_to_tech_docs".into(),
                name: "Requirements to Certified Technical Documentation".into(),
                description: "Extracts CAD drawing sheets, DXF CAD layers, bills of materials, and compiles an AS9100 Rev D audit dossier.".into(),
                category: "Documentation".into(),
                required_agents: vec!["technical_writer".into(), "quality_assurance".into(), "cost_planner".into()],
                task_count: 4,
            },
            WorkflowTemplateInfo {
                id: "cad_to_product_commercial".into(),
                name: "CAD to Cinematic Product Commercial".into(),
                description: "Tessellates CAD solid into subdivision mesh, configures MaterialX PBR shaders, arranges 3-point studio lighting, path-traces 4K frames, and assembles multi-track video with color grading.".into(),
                category: "Creative & Media".into(),
                required_agents: vec![
                    "creative_3d_modeling".into(),
                    "creative_material".into(),
                    "creative_lighting".into(),
                    "creative_rendering".into(),
                    "media_video_editing".into(),
                    "media_color_grading".into(),
                    "media_audio".into(),
                ],
                task_count: 7,
            },
            WorkflowTemplateInfo {
                id: "cad_to_interactive_twin".into(),
                name: "CAD to Interactive Digital Twin Runtime".into(),
                description: "Generates multi-resolution LODs, builds real-time scene graph with BVH collision, binds physical IoT telemetry streams, and sets up interactive 3D HUD telemetry instruments.".into(),
                category: "Digital Twin & RealTime".into(),
                required_agents: vec![
                    "realtime_scene".into(),
                    "realtime_game_dev".into(),
                    "digital_twin".into(),
                    "creative_qa".into(),
                ],
                task_count: 5,
            },
            WorkflowTemplateInfo {
                id: "engineering_to_vfx_master".into(),
                name: "Aerodynamic Simulation to VFX Master Cut".into(),
                description: "Takes Mach 10 hypersonic shock wave and aerothermal temperature data, renders plasma plume volume VFX, mixes Fairlight supersonic audio, and exports master ProRes reel.".into(),
                category: "Creative & Media".into(),
                required_agents: vec![
                    "cfd_specialist".into(),
                    "creative_vfx".into(),
                    "creative_rendering".into(),
                    "media_video_editing".into(),
                    "media_audio".into(),
                    "media_production".into(),
                ],
                task_count: 6,
            },
            WorkflowTemplateInfo {
                id: "hypersonic_full_vehicle_master".into(),
                name: "12,000 km/h Hypersonic Aircraft Master Autonomous Workflow".into(),
                description: "Closed-loop autonomous generative engineering for Mach 11 waverider: synthesizes B-Rep geometry, solves plasma aerothermodynamics, supercritical cryo-cooling, scramjet propulsion, 270V avionics, 5-axis CNC toolpaths, and AS9100 compliance.".into(),
                category: "Hypersonic & Aerospace".into(),
                required_agents: vec![
                    "chief_engineer".into(),
                    "requirements_spec".into(),
                    "cad_design".into(),
                    "cfd_specialist".into(),
                    "structural_sim".into(),
                    "thermal_engineer".into(),
                    "chemical_engineer".into(),
                    "electrical_engineer".into(),
                    "manufacturing_engineer".into(),
                    "digital_twin".into(),
                    "media_production".into(),
                    "certification_officer".into(),
                ],
                task_count: 11,
            },
        ]
    }

    pub fn instantiate_template(template_id: &str) -> Result<TaskGraph, OrchestratorError> {
        let mut graph = TaskGraph::new();

        match template_id {
            "concept_to_cad" => {
                graph.add_task(EngineeringTask::new(
                    "t1_spec",
                    "Review Specifications",
                    "requirements_spec",
                    "project.inspect",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_sketch",
                        "Create Base Datum & Sketch",
                        "cad_design",
                        "cad.sketch.create",
                    )
                    .with_dependency("t1_spec"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_extrude",
                        "Extrude 3D Solid Geometry",
                        "cad_design",
                        "cad.solid.extrude",
                    )
                    .with_dependency("t2_sketch"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_material",
                        "Assign Material",
                        "materials_scientist",
                        "materials.assign",
                    )
                    .with_dependency("t3_extrude"),
                )?;
            }
            "cad_to_fea" => {
                graph.add_task(EngineeringTask::new(
                    "t1_mat",
                    "Verify Material Assignment",
                    "materials_scientist",
                    "materials.assign",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_mesh",
                        "Generate Volumetric Mesh",
                        "cad_design",
                        "mesh.generate",
                    )
                    .with_dependency("t1_mat"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_solve",
                        "Solve Linear Static FEA",
                        "structural_sim",
                        "sim.fea.solve",
                    )
                    .with_dependency("t2_mesh"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_audit",
                        "Validate Stress & FoS",
                        "verification_engineer",
                        "report.certification.generate",
                    )
                    .with_dependency("t3_solve"),
                )?;
            }
            "cad_to_cfd" => {
                graph.add_task(EngineeringTask::new(
                    "t1_mesh",
                    "Discretize Flow Channel",
                    "cfd_specialist",
                    "mesh.generate",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_solve",
                        "Solve Internal Hydraulics",
                        "cfd_specialist",
                        "sim.cfd.solve",
                    )
                    .with_dependency("t1_mesh"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_verify",
                        "Assess Darcy-Weisbach Match",
                        "verification_engineer",
                        "report.certification.generate",
                    )
                    .with_dependency("t2_solve"),
                )?;
            }
            "cad_to_optimization" => {
                graph.add_task(EngineeringTask::new(
                    "t1_mesh",
                    "Discretize Design Space",
                    "cad_design",
                    "mesh.generate",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_opt",
                        "Execute SIMP Optimization",
                        "topology_optimizer",
                        "optimization.topology.simp",
                    )
                    .with_dependency("t1_mesh"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_reconstruct",
                        "Reconstruct Watertight Solid",
                        "cad_design",
                        "cad.solid.extrude",
                    )
                    .with_dependency("t2_opt"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_validate",
                        "Re-run FEA on Optimized Body",
                        "structural_sim",
                        "sim.fea.solve",
                    )
                    .with_dependency("t3_reconstruct"),
                )?;
            }
            "cad_to_additive" => {
                graph.add_task(EngineeringTask::new(
                    "t1_slice",
                    "Compute Slice Contours",
                    "additive_specialist",
                    "additive.slice.generate",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_infill",
                        "Generate Gyroid Infill Lattice",
                        "additive_specialist",
                        "additive.slice.generate",
                    )
                    .with_dependency("t1_slice"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_gcode",
                        "Format Machine G-Code",
                        "printing_3d",
                        "cam.gcode.emit",
                    )
                    .with_dependency("t2_infill"),
                )?;
            }
            "cad_to_cnc" => {
                graph.add_task(EngineeringTask::new(
                    "t1_plan",
                    "Plan 5-Axis Milling Strategy",
                    "cnc_machinist",
                    "cam.toolpaths.generate",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_collision",
                        "Verify Fixture Clearance",
                        "cnc_machinist",
                        "cam.toolpaths.generate",
                    )
                    .with_dependency("t1_plan"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_gcode",
                        "Emit CNC Program G-Code",
                        "cnc_machinist",
                        "cam.gcode.emit",
                    )
                    .with_dependency("t2_collision"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_cost",
                        "Estimate Machining Cycle Cost",
                        "cost_planner",
                        "report.certification.generate",
                    )
                    .with_dependency("t3_gcode"),
                )?;
            }
            "bim_to_energy" => {
                graph.add_task(EngineeringTask::new(
                    "t1_inspect",
                    "Inspect Spatial Hierarchy",
                    "bim_specialist",
                    "project.inspect",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_uval",
                        "Compute ISO 6946 U-Values",
                        "bim_specialist",
                        "sim.multiphysics_suite.solve",
                    )
                    .with_dependency("t1_inspect"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_report",
                        "Compile Building Energy Dossier",
                        "technical_writer",
                        "report.certification.generate",
                    )
                    .with_dependency("t2_uval"),
                )?;
            }
            "circuit_to_spice" => {
                graph.add_task(EngineeringTask::new(
                    "t1_netlist",
                    "Generate Circuit Netlist",
                    "electrical_engineer",
                    "project.inspect",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_solve",
                        "Execute AC Filter Response",
                        "pcb_electronics",
                        "sim.multiphysics_suite.solve",
                    )
                    .with_dependency("t1_netlist"),
                )?;
            }
            "pcb_to_si" => {
                graph.add_task(EngineeringTask::new(
                    "t1_props",
                    "Retrieve Substrate Dielectric",
                    "materials_scientist",
                    "materials.assign",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_si",
                        "Solve IPC-2141 Impedance",
                        "pcb_electronics",
                        "sim.multiphysics_suite.solve",
                    )
                    .with_dependency("t1_props"),
                )?;
            }
            "robot_to_kinematics" => {
                graph.add_task(EngineeringTask::new(
                    "t1_dh",
                    "Compute DH Transformation",
                    "robotics_automation",
                    "sim.multiphysics_suite.solve",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_traj",
                        "Generate Minimum-Jerk Spline",
                        "robotics_automation",
                        "sim.multiphysics_suite.solve",
                    )
                    .with_dependency("t1_dh"),
                )?;
            }
            "ev_to_battery_thermal" => {
                graph.add_task(EngineeringTask::new(
                    "t1_ecm",
                    "Solve 2-RC Battery Thevenin",
                    "automotive_ev",
                    "sim.multiphysics_suite.solve",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_thermal",
                        "Evaluate Runaway Margin",
                        "multiphysics",
                        "sim.multiphysics_suite.solve",
                    )
                    .with_dependency("t1_ecm"),
                )?;
            }
            "req_to_tech_docs" => {
                graph.add_task(EngineeringTask::new(
                    "t1_dxf",
                    "Export 2D Drawing Sheet",
                    "technical_writer",
                    "drawing.dxf.export",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_audit",
                        "Compile AS9100 Dossier",
                        "quality_assurance",
                        "report.certification.generate",
                    )
                    .with_dependency("t1_dxf"),
                )?;
            }
            "cad_to_product_commercial" => {
                graph.add_task(EngineeringTask::new(
                    "t1_subdiv",
                    "Catmull-Clark Mesh Subdivision",
                    "creative_3d_modeling",
                    "create.subdiv.apply",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_material",
                        "Assign PBR MaterialX Shaders",
                        "creative_material",
                        "create.material.assign",
                    )
                    .with_dependency("t1_subdiv"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_lighting",
                        "Rig Cinematic Studio Lights",
                        "creative_lighting",
                        "realtime.light.configure",
                    )
                    .with_dependency("t2_material"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_render",
                        "Path Trace 4K Keyframe Frames",
                        "creative_rendering",
                        "create.render.frame",
                    )
                    .with_dependency("t3_lighting"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t5_edit",
                        "Assemble Multi-Track NLE Timeline",
                        "media_video_editing",
                        "media.timeline.create",
                    )
                    .with_dependency("t4_render"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t6_grade",
                        "Apply 3-Way Lift/Gamma/Gain Grade",
                        "media_color_grading",
                        "media.color.grade",
                    )
                    .with_dependency("t5_edit"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t7_audio",
                        "Fairlight Multi-Channel Mix & Master",
                        "media_audio",
                        "media.audio.mix",
                    )
                    .with_dependency("t6_grade"),
                )?;
            }
            "cad_to_interactive_twin" => {
                graph.add_task(EngineeringTask::new(
                    "t1_scene",
                    "Build ECS Real-Time Scene Graph",
                    "realtime_scene",
                    "realtime.scene.build",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_bvh",
                        "Construct Spatial BVH Hierarchy",
                        "realtime_scene",
                        "realtime.bvh.query",
                    )
                    .with_dependency("t1_scene"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_physics",
                        "Configure Rigid Body Kinematics",
                        "realtime_game_dev",
                        "realtime.physics.step",
                    )
                    .with_dependency("t2_bvh"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_telemetry",
                        "Ingest Live IoT Sensor Channels",
                        "digital_twin",
                        "twin.telemetry.ingest",
                    )
                    .with_dependency("t3_physics"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t5_qa",
                        "Verify Real-Time Draw Call Budget",
                        "creative_qa",
                        "realtime.budget.verify",
                    )
                    .with_dependency("t4_telemetry"),
                )?;
            }
            "engineering_to_vfx_master" => {
                graph.add_task(EngineeringTask::new(
                    "t1_cfd",
                    "Extract Aerodynamic Pressure & Shock Front",
                    "cfd_specialist",
                    "sim.cfd.solve",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_vfx",
                        "Simulate Hypersonic Plasma Plume VFX",
                        "creative_vfx",
                        "create.vfx.simulate",
                    )
                    .with_dependency("t1_cfd"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_render",
                        "Ray Trace Aerothermal Visuals",
                        "creative_rendering",
                        "create.render.frame",
                    )
                    .with_dependency("t2_vfx"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_timeline",
                        "Cut Sequence on NLE Timeline",
                        "media_video_editing",
                        "media.timeline.create",
                    )
                    .with_dependency("t3_render"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t5_sound",
                        "Design Supersonic Jet Engine Audio",
                        "media_audio",
                        "media.audio.mix",
                    )
                    .with_dependency("t4_timeline"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t6_master",
                        "Export ProRes 422 Master Package",
                        "media_production",
                        "media.master.export",
                    )
                    .with_dependency("t5_sound"),
                )?;
            }
            "hypersonic_full_vehicle_master" => {
                graph.add_task(EngineeringTask::new(
                    "t1_mission_req",
                    "Synthesize Mach 11 Flight Envelope & Mission Requirements",
                    "requirements_spec",
                    "project.inspect",
                ))?;
                graph.add_task(
                    EngineeringTask::new(
                        "t2_waverider_cad",
                        "Synthesize 38m Waverider B-Rep Parametric CAD Geometry",
                        "cad_design",
                        "cad.solid.extrude",
                    )
                    .with_dependency("t1_mission_req"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t3_aero_plasma",
                        "Solve Nonequilibrium Aerothermodynamics & Plasma Sheath",
                        "cfd_specialist",
                        "sim.hypersonic.plasma",
                    )
                    .with_dependency("t2_waverider_cad"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t4_scramjet_cycle",
                        "Solve Multistage Oblique Shock Scramjet Propulsion",
                        "chemical_engineer",
                        "sim.hypersonic.scramjet",
                    )
                    .with_dependency("t2_waverider_cad"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t5_regen_cooling",
                        "Solve Supercritical LH2 Active Regenerative Cooling",
                        "thermal_engineer",
                        "sim.hypersonic.thermal",
                    )
                    .with_dependency("t3_aero_plasma")
                    .with_dependency("t4_scramjet_cycle"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t6_uhtc_structural",
                        "Verify UHTC / Gamma-TiAl Thermomechanical Stress & Creep",
                        "structural_sim",
                        "sim.hypersonic.creep",
                    )
                    .with_dependency("t3_aero_plasma")
                    .with_dependency("t5_regen_cooling"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t7_avionics_qins",
                        "Size 270V DC Bus & Cold-Atom Quantum Navigation",
                        "electrical_engineer",
                        "sim.hypersonic.solve",
                    )
                    .with_dependency("t1_mission_req"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t8_mfg_toolpaths",
                        "Generate 5-Axis CNC & SLM Toolpaths for Combustor Injectors",
                        "manufacturing_engineer",
                        "cam.toolpaths.generate",
                    )
                    .with_dependency("t6_uhtc_structural"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t9_digital_twin",
                        "Bind 6-DOF Telemetry to Unity Engine Digital Twin",
                        "digital_twin",
                        "export.unity.engine",
                    )
                    .with_dependency("t3_aero_plasma")
                    .with_dependency("t7_avionics_qins"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t10_media_master",
                        "Assemble DaVinci Resolve Master Presentation Reel",
                        "media_production",
                        "export.davinci.resolve",
                    )
                    .with_dependency("t9_digital_twin"),
                )?;
                graph.add_task(
                    EngineeringTask::new(
                        "t11_as9100_cert",
                        "Emit Cryptographic AS9100 / FAA Certification Dossier",
                        "certification_officer",
                        "eco.submit",
                    )
                    .with_dependency("t6_uhtc_structural")
                    .with_dependency("t8_mfg_toolpaths"),
                )?;
            }
            _ => {
                return Err(OrchestratorError::Internal(format!(
                    "Unknown template ID: '{template_id}'"
                )));
            }
        }

        Ok(graph)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_creative_workflow_templates() {
        let templates = WorkflowRegistry::list_templates();
        assert!(templates.iter().any(|t| t.id == "cad_to_product_commercial"));
        assert!(templates.iter().any(|t| t.id == "cad_to_interactive_twin"));
        assert!(templates.iter().any(|t| t.id == "engineering_to_vfx_master"));

        let comm_graph = WorkflowRegistry::instantiate_template("cad_to_product_commercial");
        assert!(comm_graph.is_ok());
        if let Ok(g) = comm_graph {
            assert_eq!(g.tasks.len(), 7);
        }

        let twin_graph = WorkflowRegistry::instantiate_template("cad_to_interactive_twin");
        assert!(twin_graph.is_ok());
        if let Ok(g) = twin_graph {
            assert_eq!(g.tasks.len(), 5);
        }

        let hyper_graph = WorkflowRegistry::instantiate_template("hypersonic_full_vehicle_master");
        assert!(hyper_graph.is_ok());
        if let Ok(g) = hyper_graph {
            assert_eq!(g.tasks.len(), 11);
            assert!(g.validate_no_cycles().is_ok());
        }
    }
}

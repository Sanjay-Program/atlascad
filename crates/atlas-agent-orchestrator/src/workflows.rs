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
            _ => {
                return Err(OrchestratorError::Internal(format!(
                    "Unknown template ID: '{template_id}'"
                )));
            }
        }

        Ok(graph)
    }
}

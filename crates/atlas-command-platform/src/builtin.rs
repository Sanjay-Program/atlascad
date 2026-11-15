//! Standard built-in engineering commands for AtlasCAD.

use crate::error::CommandError;
use crate::registry::CommandRegistry;
use crate::schema::{
    ArgumentType, CommandArgument, CommandCategory, CommandPermission, CommandSchema,
    ValidationRule,
};

pub fn register_builtin_commands(registry: &mut CommandRegistry) -> Result<(), CommandError> {
    // --- 1. Project & Document Commands ---
    registry.register(
        CommandSchema::new(
            "project.create",
            "Create New Project",
            "Initializes a new AtlasCAD project with unit system and coordinate frame.",
            CommandCategory::Project,
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_argument(CommandArgument {
            name: "project_name".into(),
            arg_type: ArgumentType::String,
            description: "Unique name of the project".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        })
        .with_argument(CommandArgument {
            name: "unit_system".into(),
            arg_type: ArgumentType::String,
            description: "Base physical unit system (si or imperial)".into(),
            required: false,
            default_value: Some(serde_json::json!("si")),
            validation_rules: vec![ValidationRule::AllowedValues(vec![
                "si".into(),
                "imperial".into(),
            ])],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "project.inspect",
            "Inspect Project Manifest",
            "Retrieves project topology, active units, and revision metadata.",
            CommandCategory::Project,
        )
        .with_permission(CommandPermission::ReadOnly),
    )?;

    // --- 2. CAD Modeling & Geometry Commands ---
    registry.register(
        CommandSchema::new(
            "cad.sketch.create",
            "Create 2D Sketch Profile",
            "Constructs a 2D sketch profile on a specified datum plane.",
            CommandCategory::Sketch,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "plane".into(),
            arg_type: ArgumentType::String,
            description: "Datum workplane (xy, xz, yz, or custom elevation)".into(),
            required: true,
            default_value: Some(serde_json::json!("xy")),
            validation_rules: vec![ValidationRule::NonEmpty],
        })
        .with_argument(CommandArgument {
            name: "profile_kind".into(),
            arg_type: ArgumentType::String,
            description: "Geometric primitive kind (rectangle, circle, regular_polygon)".into(),
            required: true,
            default_value: Some(serde_json::json!("rectangle")),
            validation_rules: vec![ValidationRule::AllowedValues(vec![
                "rectangle".into(),
                "circle".into(),
                "regular_polygon".into(),
            ])],
        })
        .with_argument(CommandArgument {
            name: "origin_z".into(),
            arg_type: ArgumentType::Number,
            description: "Elevation Z coordinate in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(0.0)),
            validation_rules: Vec::new(),
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "cad.solid.extrude",
            "Extrude Sketch to 3D Solid",
            "Extrudes a closed 2D profile along normal to produce a B-Rep solid.",
            CommandCategory::Features,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "sketch_id".into(),
            arg_type: ArgumentType::String,
            description: "Identifier of the source sketch".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        })
        .with_argument(CommandArgument {
            name: "depth_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Linear extrusion depth in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(10.0)),
            validation_rules: vec![ValidationRule::Min(0.001)],
        })
        .with_argument(CommandArgument {
            name: "operation".into(),
            arg_type: ArgumentType::String,
            description: "Boolean operation (new_body, join, cut, intersect)".into(),
            required: false,
            default_value: Some(serde_json::json!("join")),
            validation_rules: vec![ValidationRule::AllowedValues(vec![
                "new_body".into(),
                "join".into(),
                "cut".into(),
                "intersect".into(),
            ])],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "cad.solid.revolve",
            "Revolve Profile Around Axis",
            "Sweeps a 2D profile circularly around an axis to generate a solid of revolution.",
            CommandCategory::Features,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "sketch_id".into(),
            arg_type: ArgumentType::String,
            description: "Identifier of the source sketch".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        })
        .with_argument(CommandArgument {
            name: "angle_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Revolution sweep angle in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(360.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(360.0)],
        }),
    )?;

    // --- 3. Material & Mesh Commands ---
    registry.register(
        CommandSchema::new(
            "materials.assign",
            "Assign Engineering Material",
            "Applies certified physical material properties to a solid or part.",
            CommandCategory::Materials,
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_argument(CommandArgument {
            name: "material_id".into(),
            arg_type: ArgumentType::String,
            description:
                "Material registry key (e.g. steel.1018, aluminum.6061-t6, titanium.ti-6al-4v)"
                    .into(),
            required: true,
            default_value: Some(serde_json::json!("steel.1018")),
            validation_rules: vec![ValidationRule::NonEmpty],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "mesh.generate",
            "Generate FEA/CFD Discretization Mesh",
            "Constructs volumetric tetrahedral or hexahedral voxel mesh from solid geometry.",
            CommandCategory::Mesh,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "voxel_size_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Characteristic mesh element edge size in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(2.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100.0)],
        }),
    )?;

    // --- 4. Simulation & CAE Solvers ---
    registry.register(
        CommandSchema::new(
            "sim.fea.solve",
            "Linear Static Stress FEA",
            "Solves boundary value elasticity Ku = F for displacement, strain, and von Mises stress.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "material".into(),
            arg_type: ArgumentType::String,
            description: "Material identifier".into(),
            required: false,
            default_value: Some(serde_json::json!("steel.1018")),
            validation_rules: Vec::new(),
        })
        .with_argument(CommandArgument {
            name: "voxel_size_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Element discretization resolution".into(),
            required: false,
            default_value: Some(serde_json::json!(2.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerodynamics.solve",
            "Subsonic & Supersonic Aerodynamics",
            "Calculates CL, CD, L/D, and Rankine-Hugoniot shock jumps across flow regimes.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "airspeed_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Free-stream airspeed in m/s".into(),
            required: true,
            default_value: Some(serde_json::json!(68.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "angle_of_attack_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Wing angle of attack in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(4.5)),
            validation_rules: Vec::new(),
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.cfd.solve",
            "Internal CFD & Hydraulic Network",
            "Solves Darcy-Weisbach head loss and Colebrook-White friction factor.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "velocity_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Inflow fluid velocity in m/s".into(),
            required: true,
            default_value: Some(serde_json::json!(1.5)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.multiphysics_suite.solve",
            "Master 27-Domain Multiphysics Suite",
            "Executes complete multi-disciplinary verification across aerospace, nuclear, automotive & mining standards.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    // --- 5. Generative Design & Optimization ---
    registry.register(
        CommandSchema::new(
            "optimization.topology.simp",
            "SIMP Topology Optimization",
            "Minimizes structural compliance under material volume fraction constraints.",
            CommandCategory::Optimization,
        )
        .with_permission(CommandPermission::DesignOptimize)
        .with_argument(CommandArgument {
            name: "volume_fraction".into(),
            arg_type: ArgumentType::Number,
            description: "Target material volume fraction (0.1 to 0.9)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.35)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(0.9)],
        })
        .with_argument(CommandArgument {
            name: "iterations".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of OC/MMA density filter iterations".into(),
            required: false,
            default_value: Some(serde_json::json!(12)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        }),
    )?;

    // --- 6. CAM & CNC Machining ---
    registry.register(
        CommandSchema::new(
            "cam.toolpaths.generate",
            "Generate CNC Milling Toolpaths",
            "Plans 3-axis, 4-axis rotary, and 5-axis simultaneous cutting trajectories.",
            CommandCategory::Cnc,
        )
        .with_permission(CommandPermission::ManufacturingPrep)
        .with_argument(CommandArgument {
            name: "operation".into(),
            arg_type: ArgumentType::String,
            description: "Machining strategy (facing, contour, pocket, multiaxis5)".into(),
            required: true,
            default_value: Some(serde_json::json!("facing")),
            validation_rules: vec![ValidationRule::AllowedValues(vec![
                "facing".into(),
                "contour".into(),
                "pocket".into(),
                "rotary4".into(),
                "multiaxis5".into(),
            ])],
        })
        .with_argument(CommandArgument {
            name: "tool_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "End mill tool diameter in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(6.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "cam.gcode.emit",
            "Emit CNC Machine G-Code",
            "Formats ISO standard toolpath G-code for CNC machine controller.",
            CommandCategory::Cnc,
        )
        .with_permission(CommandPermission::FileExport)
        .with_approval(true), // G-code execution approval gate
    )?;

    // --- 7. Additive Manufacturing & 3D Printing ---
    registry.register(
        CommandSchema::new(
            "additive.slice.generate",
            "Slice 3D Model for Additive Manufacturing",
            "Computes planar cross sections and generates TPMS Gyroid or Honeycomb infills.",
            CommandCategory::Additive,
        )
        .with_permission(CommandPermission::ManufacturingPrep)
        .with_argument(CommandArgument {
            name: "layer_height_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Vertical slice resolution in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(0.2)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(1.0)],
        })
        .with_argument(CommandArgument {
            name: "infill_pattern".into(),
            arg_type: ArgumentType::String,
            description: "Infill geometry (rectilinear, grid, honeycomb, gyroid)".into(),
            required: false,
            default_value: Some(serde_json::json!("gyroid")),
            validation_rules: vec![ValidationRule::AllowedValues(vec![
                "rectilinear".into(),
                "grid".into(),
                "honeycomb".into(),
                "gyroid".into(),
            ])],
        }),
    )?;

    // --- 8. 2D Drafting & Export ---
    registry.register(
        CommandSchema::new(
            "drawing.dxf.export",
            "Export 2D Drawing to DXF",
            "Generates AutoCAD R12/2000 compliant DXF with layers, entities, and title block.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::FileExport),
    )?;

    registry.register(
        CommandSchema::new(
            "report.certification.generate",
            "Generate AS9100 / ISO 9001 Certification Dossier",
            "Produces cryptographically signed engineering compliance report with 3-tier approvals.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::ReadOnly),
    )?;

    Ok(())
}

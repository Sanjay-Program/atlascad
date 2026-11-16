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
            "sim.hypersonic_plane.solve",
            "12,000 km/h Hypersonic Multi-Physics Aircraft Simulation",
            "Coupled aerothermodynamics, Scramjet chemical propulsion, UHTC thermal stress, 270V DC avionics, and 6-DOF trajectory.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "target_speed_km_h".into(),
            arg_type: ArgumentType::Number,
            description: "Target airspeed in km/h (default 12,000 km/h = Mach ~11)".into(),
            required: false,
            default_value: Some(serde_json::json!(12000.0)),
            validation_rules: vec![ValidationRule::Min(100.0)],
        })
        .with_argument(CommandArgument {
            name: "cruise_altitude_m".into(),
            arg_type: ArgumentType::Number,
            description: "Operational cruise altitude in meters (default 32,000 m)".into(),
            required: false,
            default_value: Some(serde_json::json!(32000.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(60000.0)],
        })
        .with_argument(CommandArgument {
            name: "angle_of_attack_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Waverider angle of attack in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(2.5)),
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

    // --- 9. Post-Production & Digital Twin Engines ---
    registry.register(
        CommandSchema::new(
            "export.davinci.resolve",
            "Export DaVinci Resolve Timeline & VFX Bundle",
            "Generates FCPXML multi-track timeline, CMX 3600 EDL, 3D aerothermal LUT (.cube), and Pixar OpenUSD (.usda) scene.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::FileExport)
        .with_argument(CommandArgument {
            name: "timebase_fps".into(),
            arg_type: ArgumentType::Number,
            description: "Timeline frame rate (e.g. 24.0, 25.0, 29.97, 30.0, 60.0)".into(),
            required: false,
            default_value: Some(serde_json::json!(25.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(240.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "export.unity.engine",
            "Export Unity 3D Engine & Digital Twin Package",
            "Generates Unity Package (com.atlascad.unity), glTF 2.0 PBR scene model, and 6-DOF telemetry flight controllers.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::FileExport)
        .with_argument(CommandArgument {
            name: "include_hud".into(),
            arg_type: ArgumentType::Boolean,
            description: "Include real-time flight telemetry glass-cockpit HUD controller".into(),
            required: false,
            default_value: Some(serde_json::json!(true)),
            validation_rules: vec![],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "export.step.cad",
            "Export ISO 10303-21 STEP CAD Model",
            "Generates standardized ISO 10303-21 Part 21 (AP203/AP214) exchange file for solid B-Rep geometry.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::FileExport),
    )?;

    registry.register(
        CommandSchema::new(
            "export.threemf.additive",
            "Export ASTM/ISO 52915 3MF Additive Model",
            "Generates standardized 3D Manufacturing Format (3MF) XML model with mesh vertices and triangles.",
            CommandCategory::Additive,
        )
        .with_permission(CommandPermission::FileExport),
    )?;

    // --- 10. System & Deployment Lifecycle ---
    registry.register(
        CommandSchema::new(
            "system.deploy.status",
            "Query System Deployment & Runtime Status",
            "Inspects production container orchestrator, host telemetry, engine availability, and active session states.",
            CommandCategory::System,
        )
        .with_permission(CommandPermission::ReadOnly),
    )?;

    // --- 11. Atlas Create, RealTime & Media Creative Ecosystem ---
    registry.register(
        CommandSchema::new(
            "create.subdiv.apply",
            "Apply Catmull-Clark Subdivision Surface",
            "Performs iterative Catmull-Clark subdivision smoothing on a polygon mesh.",
            CommandCategory::Mesh,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "mesh_id".into(),
            arg_type: ArgumentType::String,
            description: "Target polygon mesh identifier".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        })
        .with_argument(CommandArgument {
            name: "levels".into(),
            arg_type: ArgumentType::Integer,
            description: "Subdivision recursion depth (1-4)".into(),
            required: false,
            default_value: Some(serde_json::json!(1)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(4.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "create.sculpt.stroke",
            "Execute Digital Sculpting Brush Stroke",
            "Deforms mesh vertices using sculpting brush physics (Draw, Clay, Smooth, Flatten, Inflate).",
            CommandCategory::Geometry,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "brush_type".into(),
            arg_type: ArgumentType::String,
            description: "Sculpting brush algorithm (draw, clay, smooth, flatten, inflate)".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "create.material.assign",
            "Assign PBR MaterialX Shader",
            "Binds a physically-based material with metallic, roughness, and subsurface properties.",
            CommandCategory::Materials,
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_argument(CommandArgument {
            name: "material_name".into(),
            arg_type: ArgumentType::String,
            description: "Name of PBR material definition".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "realtime.scene.build",
            "Build Real-Time ECS Scene Graph",
            "Assembles 3D hierarchy, generates BVH spatial indices, and binds telemetry sensors.",
            CommandCategory::Assemblies,
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_argument(CommandArgument {
            name: "scene_name".into(),
            arg_type: ArgumentType::String,
            description: "Name of the real-time scene".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "media.timeline.create",
            "Create NLE Multi-Track Timeline",
            "Instantiates video and audio timeline tracks with SMPTE timecode support.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::ProjectEdit)
        .with_argument(CommandArgument {
            name: "title".into(),
            arg_type: ArgumentType::String,
            description: "Title of timeline project".into(),
            required: true,
            default_value: None,
            validation_rules: vec![ValidationRule::NonEmpty],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "media.render.export",
            "Export Timeline Package via OpenTimelineIO",
            "Renders timeline EDL/OTIO and exports master media deliverables.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::FileExport),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.plasma",
            "Nonequilibrium Plasma Sheath & RF Blackout",
            "Computes electron number density, plasma cutoff frequency, wave attenuation across L/X/Ka bands, and MHD magnetic window.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "flight_mach".into(),
            arg_type: ArgumentType::Number,
            description: "Flight Mach number (e.g. 11.0)".into(),
            required: false,
            default_value: Some(serde_json::json!(11.0)),
            validation_rules: vec![ValidationRule::Min(5.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.thermal",
            "Active Regenerative Cooling Thermal Network",
            "Solves supercritical LH2 microchannel convective heat transfer, Dittus-Boelter HTC, and substrate wall thermal safety margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.scramjet",
            "Scramjet Multistage Inlet, Isolator & Combustor",
            "Solves 3-ramp oblique shock wave compression, isolator unstart margin (Waltrup-Billig), and supersonic Rayleigh combustion.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.cryo",
            "Cryogenic Fuel Feed & Turbopump Hydraulics",
            "Evaluates 60-layer MLI boil-off rates, expander-cycle turbopump head rise, and inducer cavitation safety margins.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.bom",
            "Detailed 15-System Real-World Aircraft BOM",
            "Outputs the complete hierarchical engineering BOM with 48 line items, materials, masses, and CG coordinates.",
            CommandCategory::Reporting,
        )
        .with_permission(CommandPermission::ReadOnly),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.mhd_bypass",
            "Magnetohydrodynamic (MHD) Energy Bypass Generator",
            "Solves boundary layer ionization electrical extraction, Faraday generator power, and downstream SERN nozzle thrust augmentation.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.slush_h2",
            "Slush Hydrogen (13.8 K Triple Point) Fuel & Cryogenics",
            "Evaluates density boost, 15% tank volume reduction, latent heat cooling margin, and zero-boiloff hold time.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.creep",
            "High-Temperature Viscoplastic Creep & Larson-Miller Rupture",
            "Evaluates steady-state Norton-Bailey creep strain rate and predicts 1000-hour mission life for UHTC ceramics.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.flutter",
            "Hypersonic Aeroelastic Panel Flutter Stability",
            "Evaluates Dowell third-order piston theory aerodynamic panel flutter and high-temperature thermal stiffness degradation.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.6dof",
            "Hypersonic 6-DOF Bank-to-Turn Maneuvering Dynamics",
            "Models spherical Earth orbital relief, bank-to-turn roll steering, normal g-load envelope, and asymmetric cross-track aeroheating.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.hypersonic.hts_magnet",
            "REBCO High-Temperature Superconducting Magnet",
            "Models 20 K slush LH2 cooled superconducting magnetic coil, critical current margin, magnetic pressure, and cryo heat leakage.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    // --- 10. Multi-Disciplinary Engineering & Science Commands ---
    registry.register(
        CommandSchema::new(
            "sim.civil.frame_buckling",
            "Euler & Johnson Column Buckling Analysis",
            "Evaluates critical buckling load, effective slenderness ratio, and AISC 360 safety factor for structural frames.",
            CommandCategory::Bim,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Unsupported column length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(4.5)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.civil.retaining_wall",
            "Rankine & Coulomb Lateral Earth Pressure",
            "Computes active and passive earth thrust, overturning stability, and sliding resistance for retaining structures.",
            CommandCategory::Bim,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "wall_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Retaining wall height in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(4.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.power.ac_flow",
            "Newton-Raphson AC Power Flow Solver",
            "Solves bus admittance matrix, voltage magnitudes/angles, and active/reactive transmission line losses.",
            CommandCategory::Electrical,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.chemical.vle_flash",
            "Rachford-Rice Multi-Component VLE Flash",
            "Evaluates isothermal vapor-liquid equilibrium split, phase compositions, and Peng-Robinson fugacities.",
            CommandCategory::Chemical,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.chemical.acid_base",
            "Analytical Acid-Base Equilibria & Titration Solver",
            "Computes rigorous pH titration curves, equivalence points, Henderson-Hasselbalch buffer capacities, and neutralization enthalpy.",
            CommandCategory::Chemical,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "analyte_id".into(),
            arg_type: ArgumentType::String,
            description: "Chemical species identifier for analyte (e.g. hcl, acetic_acid, h2so4, nh3)".into(),
            required: false,
            default_value: Some(serde_json::json!("acetic_acid")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "analyte_concentration_m".into(),
            arg_type: ArgumentType::Number,
            description: "Analyte molar concentration in mol/L (M)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.10)),
            validation_rules: vec![ValidationRule::Min(0.0001)],
        })
        .with_argument(CommandArgument {
            name: "analyte_volume_ml".into(),
            arg_type: ArgumentType::Number,
            description: "Analyte initial volume in mL".into(),
            required: false,
            default_value: Some(serde_json::json!(25.0)),
            validation_rules: vec![ValidationRule::Min(0.001)],
        })
        .with_argument(CommandArgument {
            name: "titrant_id".into(),
            arg_type: ArgumentType::String,
            description: "Chemical species identifier for titrant (e.g. naoh, koh, hcl)".into(),
            required: false,
            default_value: Some(serde_json::json!("naoh")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "titrant_concentration_m".into(),
            arg_type: ArgumentType::Number,
            description: "Titrant molar concentration in mol/L (M)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.10)),
            validation_rules: vec![ValidationRule::Min(0.0001)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.chemical.species_catalog",
            "Chemical Acid-Base & Solvent Species Catalog",
            "Returns standardized thermodynamic parameters, Ka/Kb values, valence, and molar mass for chemical engineering species.",
            CommandCategory::Chemical,
        )
        .with_permission(CommandPermission::ReadOnly),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.robotics.kinematics",
            "6-DOF Manipulator Forward & Inverse Kinematics",
            "Evaluates Denavit-Hartenberg homogeneous transforms, Damped Least Squares inverse kinematics, and Yoshikawa manipulability.",
            CommandCategory::Robotics,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.env.plume_dispersion",
            "Pasquill-Gifford Gaussian Atmospheric Plume",
            "Calculates downwind and crosswind pollutant concentrations and peak ground-level impact distance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.marine.hydrostatics",
            "Marine Vessel Hydrostatics & Wave Dispersion",
            "Solves vessel displacement, metacentric height (GM), righting arm GZ, and linear Airy surface gravity wave kinematics.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.acoustics.stc",
            "Acoustic Sound Transmission Class & Attenuation",
            "Evaluates transmission loss across ISO 1/3 octave bands, mass law limits, and ASTM E413 STC rating.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun),
    )?;

    // --- 13. Sheet Metal Bending & Flat Pattern Unfolding ---
    registry.register(
        CommandSchema::new(
            "cad.sheet_metal.unfold",
            "Sheet Metal Bend Allowance & Flat Pattern Unfolding",
            "Calculates neutral axis K-factor, bend allowance (BA), outside setback (OSSB), bend deduction (BD), flat blank dimensions, and press-brake forming tonnage.",
            CommandCategory::Features,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "material".into(),
            arg_type: ArgumentType::String,
            description: "Sheet metal material (carbon_steel, stainless_steel, aluminum_5052, copper, brass)".into(),
            required: false,
            default_value: Some(serde_json::json!("carbon_steel")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Sheet plate thickness in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(2.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "flange_lengths_mm".into(),
            arg_type: ArgumentType::Array,
            description: "List of flange lengths along bend sequence in mm".into(),
            required: true,
            default_value: Some(serde_json::json!([50.0, 50.0])),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "bend_angles_deg".into(),
            arg_type: ArgumentType::Array,
            description: "List of bend angles in degrees (e.g. [90.0])".into(),
            required: false,
            default_value: Some(serde_json::json!([90.0])),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "inner_radii_mm".into(),
            arg_type: ArgumentType::Array,
            description: "List of inner bend radii in mm (e.g. [2.0])".into(),
            required: false,
            default_value: Some(serde_json::json!([2.0])),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "die_opening_width_mm".into(),
            arg_type: ArgumentType::Number,
            description: "V-die opening width for press-brake forming tonnage calculation in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(16.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 14. Standard Fasteners & Bolted Joint Analysis ---
    registry.register(
        CommandSchema::new(
            "cad.hardware.fastener",
            "ISO Metric Bolt Sizing & Preload Torque Analysis",
            "Computes tensile stress area, proof load, clamping preload, tightening torque, and joint separation safety factor for ISO standard fasteners.",
            CommandCategory::Assemblies,
        )
        .with_permission(CommandPermission::ReadOnly)
        .with_argument(CommandArgument {
            name: "thread_size".into(),
            arg_type: ArgumentType::String,
            description: "ISO metric thread size (e.g. M3, M4, M5, M6, M8, M10, M12, M16, M20, M24, M30, M36)".into(),
            required: true,
            default_value: Some(serde_json::json!("M10")),
            validation_rules: vec![ValidationRule::NonEmpty],
        })
        .with_argument(CommandArgument {
            name: "bolt_class".into(),
            arg_type: ArgumentType::String,
            description: "ISO 898-1 bolt property class (4.8, 8.8, 10.9, 12.9, A2-70)".into(),
            required: false,
            default_value: Some(serde_json::json!("8.8")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "clamped_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Grip length / clamped joint thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(40.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_tension_load_kn".into(),
            arg_type: ArgumentType::Number,
            description: "External tensile working load applied to joint in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_shear_load_kn".into(),
            arg_type: ArgumentType::Number,
            description: "External transverse shear load applied to joint in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(5.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "torque_coefficient_k".into(),
            arg_type: ArgumentType::Number,
            description: "Nut factor / friction torque coefficient K (0.15 lubricated, 0.20 dry zinc-plated)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.18)),
            validation_rules: vec![ValidationRule::Min(0.05)],
        }),
    )?;

    // --- 15. Chemical Reaction Kinetics & Continuous Reactor Sizing ---
    registry.register(
        CommandSchema::new(
            "sim.chemical.kinetics",
            "Chemical Reaction Kinetics & CSTR/PFR Reactor Design",
            "Evaluates Arrhenius rate constants, reaction order concentration profiles, half-life, Damköhler number, and CSTR/PFR reactor volumes.",
            CommandCategory::Chemical,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "reaction_name".into(),
            arg_type: ArgumentType::String,
            description: "Descriptive chemical reaction equation or identifier".into(),
            required: false,
            default_value: Some(serde_json::json!("A -> Products")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "order".into(),
            arg_type: ArgumentType::String,
            description: "Reaction order (first_order, second_order, reversible_first_order)".into(),
            required: false,
            default_value: Some(serde_json::json!("first_order")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "initial_concentration_m".into(),
            arg_type: ArgumentType::Number,
            description: "Initial reactant concentration in mol/L (M)".into(),
            required: true,
            default_value: Some(serde_json::json!(1.0)),
            validation_rules: vec![ValidationRule::Min(0.0001)],
        })
        .with_argument(CommandArgument {
            name: "rate_constant_k".into(),
            arg_type: ArgumentType::Number,
            description: "Reaction rate constant k at operating temperature".into(),
            required: false,
            default_value: Some(serde_json::json!(0.05)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "temperature_k".into(),
            arg_type: ArgumentType::Number,
            description: "Reaction operating temperature in Kelvin".into(),
            required: false,
            default_value: Some(serde_json::json!(298.15)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "volumetric_flow_m3_s".into(),
            arg_type: ArgumentType::Number,
            description: "Continuous feed volumetric flow rate in m³/s for reactor volume sizing".into(),
            required: false,
            default_value: Some(serde_json::json!(0.005)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "target_conversion".into(),
            arg_type: ArgumentType::Number,
            description: "Target fractional conversion X (0.0 to 0.999)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.80)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(0.999)],
        }),
    )?;

    // --- 16. Multi-Technology Additive Manufacturing Process Profiles ---
    registry.register(
        CommandSchema::new(
            "additive.process_profile.solve",
            "Multi-Technology Additive Process Physics & Layer Profiling",
            "Solves Jacobs working curve exposure for SLA/DLP photopolymers, Volumetric Energy Density (VED) for SLM metal powder bed fusion, and layer-by-layer cycle times.",
            CommandCategory::Additive,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "technology".into(),
            arg_type: ArgumentType::String,
            description: "Additive technology (fdm_fff, sla_dlp_msla, slm_metal_pbf, sls_polymer)".into(),
            required: true,
            default_value: Some(serde_json::json!("sla_dlp_msla")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "part_height_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Overall part Z height in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "part_volume_cm3".into(),
            arg_type: ArgumentType::Number,
            description: "Solid part volume in cm³".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "layer_height_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Slicing layer thickness in mm (e.g. 0.050 mm for SLA, 0.040 mm for SLM, 0.200 mm for FDM)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.050)),
            validation_rules: vec![ValidationRule::Min(0.005)],
        }),
    )?;

    // --- 17. Outdoor Environmental Noise (ISO 9613-2) ---
    registry.register(
        CommandSchema::new(
            "sim.acoustics.outdoor_noise",
            "ISO 9613-2 Outdoor Environmental Noise Propagation",
            "Calculates geometrical divergence, atmospheric absorption, ground reflection/absorption, and Maekawa barrier diffraction attenuation.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "source_name".into(),
            arg_type: ArgumentType::String,
            description: "Name/description of exterior sound source".into(),
            required: false,
            default_value: Some(serde_json::json!("Industrial Rooftop Chiller")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "sound_power_lwa".into(),
            arg_type: ArgumentType::Number,
            description: "Total A-weighted sound power level LwA in dBA".into(),
            required: true,
            default_value: Some(serde_json::json!(102.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "distance_m".into(),
            arg_type: ArgumentType::Number,
            description: "Direct propagation distance from source to receiver in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "source_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Source elevation height in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(2.5)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "receiver_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Receiver elevation height in meters (standard observer = 1.5m)".into(),
            required: false,
            default_value: Some(serde_json::json!(1.5)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "barrier_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Acoustic barrier screen height in meters (optional)".into(),
            required: false,
            default_value: Some(serde_json::json!(3.5)),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "barrier_distance_m".into(),
            arg_type: ArgumentType::Number,
            description: "Distance from source to acoustic barrier in meters (optional)".into(),
            required: false,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "ground_factor".into(),
            arg_type: ArgumentType::Number,
            description: "ISO 9613-2 ground porosity factor G (0.0 hard, 1.0 porous vegetation)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.70)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(1.0)],
        })
        .with_argument(CommandArgument {
            name: "regulatory_limit_dba".into(),
            arg_type: ArgumentType::Number,
            description: "Target regulatory permissible noise threshold in dBA".into(),
            required: false,
            default_value: Some(serde_json::json!(50.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        }),
    )?;

    // --- 18. Mechanical Helical Springs (DIN EN 13906-1) ---
    registry.register(
        CommandSchema::new(
            "cad.hardware.spring",
            "DIN EN 13906-1 Helical Compression Spring Design & Verification",
            "Calculates spring rate k, Wahl stress factor kw, solid height Ls, solid stress tau_n, surge natural frequency f0, and free-standing buckling limit.",
            CommandCategory::Geometry,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "wire_diameter_d_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Spring wire diameter d in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(4.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "mean_coil_diameter_d_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Mean coil diameter D in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(32.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "free_length_l0_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Uncompressed free length L0 in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "active_coils_na".into(),
            arg_type: ArgumentType::Number,
            description: "Number of active coils na".into(),
            required: true,
            default_value: Some(serde_json::json!(7.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_force_f2_n".into(),
            arg_type: ArgumentType::Number,
            description: "Maximum operating working load F2 in Newtons".into(),
            required: true,
            default_value: Some(serde_json::json!(650.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "material".into(),
            arg_type: ArgumentType::String,
            description: "Spring steel material alloy (e.g. MusicWire, OilTempered, Stainless302, InconelX750, ChromeSilicon)".into(),
            required: false,
            default_value: Some(serde_json::json!("MusicWire")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "end_type".into(),
            arg_type: ArgumentType::String,
            description: "Spring end coil configuration (SquaredAndGround, Closed, Open)".into(),
            required: false,
            default_value: Some(serde_json::json!("SquaredAndGround")),
            validation_rules: vec![],
        }),
    )?;

    // --- 19. Involute Spur & Helical Gears (ISO 6336 / AGMA 2001-D04) ---
    registry.register(
        CommandSchema::new(
            "cad.hardware.gear_pair",
            "ISO 6336 / AGMA 2001-D04 Involute Gear Pair Design & Rating",
            "Synthesizes transverse module, standard pitch/root/tip diameters, contact ratio, pitch line velocity, Lewis bending stress, and AGMA contact pitting safety factor.",
            CommandCategory::Geometry,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "gear_type".into(),
            arg_type: ArgumentType::String,
            description: "Gear kinematic arrangement (Spur or Helical)".into(),
            required: false,
            default_value: Some(serde_json::json!("Helical")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "normal_module_mn".into(),
            arg_type: ArgumentType::Number,
            description: "Normal module mn in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(3.5)),
            validation_rules: vec![ValidationRule::Min(0.2)],
        })
        .with_argument(CommandArgument {
            name: "pinion_teeth_z1".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of teeth on driving pinion z1".into(),
            required: true,
            default_value: Some(serde_json::json!(19)),
            validation_rules: vec![ValidationRule::Min(8.0)],
        })
        .with_argument(CommandArgument {
            name: "gear_teeth_z2".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of teeth on driven gear wheel z2".into(),
            required: true,
            default_value: Some(serde_json::json!(57)),
            validation_rules: vec![ValidationRule::Min(8.0)],
        })
        .with_argument(CommandArgument {
            name: "face_width_b_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Active tooth face width b in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(40.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "helix_angle_beta_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Helix angle beta in degrees (0.0 for spur)".into(),
            required: false,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(45.0)],
        })
        .with_argument(CommandArgument {
            name: "pinion_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Pinion rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(3000.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "transmitted_power_kw".into(),
            arg_type: ArgumentType::Number,
            description: "Transmitted mechanical power in kW".into(),
            required: true,
            default_value: Some(serde_json::json!(35.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        }),
    )?;

    // --- 20. Advanced CNC Machining Physics (DIN 6584 / Kienzle Force Model) ---
    registry.register(
        CommandSchema::new(
            "cam.physics.cutting_force",
            "DIN 6584 Kienzle Cutting Force & CNC Spindle Physics",
            "Computes specific cutting force kc, cutting force Fc, cutting power Pc, required motor power Pm, material removal rate MRR, and theoretical surface finish Ra.",
            CommandCategory::Cnc,
        )
        .with_permission(CommandPermission::ManufacturingPrep)
        .with_argument(CommandArgument {
            name: "operation".into(),
            arg_type: ArgumentType::String,
            description: "Machining kinematic operation (Turning, EndMilling, FaceMilling, Drilling)".into(),
            required: false,
            default_value: Some(serde_json::json!("FaceMilling")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "workpiece".into(),
            arg_type: ArgumentType::String,
            description: "Workpiece material grade (LowCarbonSteel, AlloySteel4140, AusteniticStainless304, Aluminum6061T6, TitaniumTi6Al4V, CastIronGG25)".into(),
            required: false,
            default_value: Some(serde_json::json!("AlloySteel4140")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "cutting_speed_vc_m_per_min".into(),
            arg_type: ArgumentType::Number,
            description: "Cutting linear speed Vc in m/min".into(),
            required: true,
            default_value: Some(serde_json::json!(180.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "feed_f_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Feed per tooth or feed per revolution f in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(0.15)),
            validation_rules: vec![ValidationRule::Min(0.001)],
        })
        .with_argument(CommandArgument {
            name: "depth_of_cut_ap_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Axial depth of cut ap in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(3.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "radial_width_ae_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Radial engagement width ae in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(40.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "tool_diameter_dc_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Cutting cutter diameter Dc in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(50.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "number_of_teeth_z".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of cutter insert flutes z".into(),
            required: false,
            default_value: Some(serde_json::json!(4)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 21. Industrial Electrical Short-Circuit & Voltage Drop (IEC 60909) ---
    registry.register(
        CommandSchema::new(
            "sim.electrical.short_circuit",
            "IEC 60909 3-Phase Symmetrical Short-Circuit & Voltage Drop",
            "Computes transformer and network impedances, initial symmetrical short-circuit current Ik'', peak dynamic current ip, breaking MVA, and cable run voltage drop.",
            CommandCategory::Electrical,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_voltage_un_v".into(),
            arg_type: ArgumentType::Number,
            description: "Secondary nominal line-to-line AC voltage in Volts (e.g. 400V)".into(),
            required: true,
            default_value: Some(serde_json::json!(400.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "transformer_rated_power_sn_kva".into(),
            arg_type: ArgumentType::Number,
            description: "Distribution transformer rated apparent capacity in kVA (e.g. 1000 kVA)".into(),
            required: true,
            default_value: Some(serde_json::json!(1000.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "transformer_impedance_voltage_uk_percent".into(),
            arg_type: ArgumentType::Number,
            description: "Transformer percent short-circuit impedance voltage uk% (e.g. 6.0%)".into(),
            required: true,
            default_value: Some(serde_json::json!(6.0)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(20.0)],
        })
        .with_argument(CommandArgument {
            name: "feeder_length_l_m".into(),
            arg_type: ArgumentType::Number,
            description: "Feeder cable run route length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(65.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "conductor_cross_section_q_mm2".into(),
            arg_type: ArgumentType::Number,
            description: "Phase conductor cross-sectional area in mm2 (e.g. 150 mm2)".into(),
            required: true,
            default_value: Some(serde_json::json!(150.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "conductor_material".into(),
            arg_type: ArgumentType::String,
            description: "Conductor metallurgy (Copper or Aluminum)".into(),
            required: false,
            default_value: Some(serde_json::json!("Copper")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "operating_current_ib_a".into(),
            arg_type: ArgumentType::Number,
            description: "Normal operating design load current Ib in Amperes".into(),
            required: false,
            default_value: Some(serde_json::json!(220.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        }),
    )?;

    // --- 22. Architectural Daylighting & CIE Sky Luminescence (EN 17037) ---
    registry.register(
        CommandSchema::new(
            "sim.building.daylight_factor",
            "CIE Sky Luminescence & EN 17037 Daylight Factor Analysis",
            "Calculates CIE standard overcast sky daylight factors (Sky Component SC, ERC, IRC), working desk daylight factor DF%, and room average daylight factor ADF%.",
            CommandCategory::Bim,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "sky_type".into(),
            arg_type: ArgumentType::String,
            description: "CIE standard sky condition (CieOvercast, CieUniform, CiePartlyCloudy, CieClear)".into(),
            required: false,
            default_value: Some(serde_json::json!("CieOvercast")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "room_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Room interior depth/length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(6.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "room_width_m".into(),
            arg_type: ArgumentType::Number,
            description: "Room interior width along fenestration facade in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(5.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "room_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Room finished floor-to-ceiling clear height in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(3.0)),
            validation_rules: vec![ValidationRule::Min(1.5)],
        })
        .with_argument(CommandArgument {
            name: "window_width_m".into(),
            arg_type: ArgumentType::Number,
            description: "Total architectural window glazed width in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(3.2)),
            validation_rules: vec![ValidationRule::Min(0.2)],
        })
        .with_argument(CommandArgument {
            name: "window_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Architectural window glazed height in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.0)),
            validation_rules: vec![ValidationRule::Min(0.2)],
        })
        .with_argument(CommandArgument {
            name: "desk_distance_from_window_m".into(),
            arg_type: ArgumentType::Number,
            description: "Primary occupant desk distance from window plane in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(2.5)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        }),
    )?;

    // --- 23. Transmission Shaft Sizing & DIN 6885-1 Parallel Key ---
    registry.register(
        CommandSchema::new(
            "cad.hardware.shaft_key",
            "DIN 6885-1 & ASME B106.1M Transmission Shaft & Key Sizing",
            "Calculates shaft diameter under combined torsion and bending, torsional angular deflection, and DIN 6885-1 parallel key shear stress and hub contact pressure.",
            CommandCategory::Geometry,
        )
        .with_permission(CommandPermission::GeometryCreate)
        .with_argument(CommandArgument {
            name: "transmitted_power_kw".into(),
            arg_type: ArgumentType::Number,
            description: "Transmitted mechanical power in kW".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "rotational_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Shaft rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(1450.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "bending_moment_nm".into(),
            arg_type: ArgumentType::Number,
            description: "Peak resultant bending moment in N·m".into(),
            required: false,
            default_value: Some(serde_json::json!(350.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "selected_shaft_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal selected shaft diameter in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(5.0)],
        }),
    )?;

    // --- 24. ASME BPVC Section VIII Div. 1 Pressure Vessel ---
    registry.register(
        CommandSchema::new(
            "sim.pressure_vessel.asme",
            "ASME BPVC Section VIII Division 1 Pressure Vessel & Formed Heads",
            "Calculates minimum wall thickness for cylindrical shells and 2:1 ellipsoidal / torispherical heads, vessel MAWP, and hydrostatic proof test pressure.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "design_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Internal design gauge pressure in bar (e.g. 20.0 bar)".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "inside_diameter_d_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Inside diameter of vessel in mm (e.g. 1200 mm)".into(),
            required: true,
            default_value: Some(serde_json::json!(1200.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "tangent_length_l_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Cylindrical shell tangent-to-tangent length in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(3000.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "corrosion_allowance_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Specified corrosion allowance in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(2.5)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        }),
    )?;

    // --- 25. ASHRAE Psychrometrics & Moist Air Thermodynamics ---
    registry.register(
        CommandSchema::new(
            "sim.thermal.psychrometrics",
            "ASHRAE Psychrometric State & Moist Air Thermodynamics",
            "Calculates saturation and vapor pressures, humidity ratio W, specific enthalpy h, dew point Tdp, wet bulb Twb, specific volume, and density.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "dry_bulb_temp_c".into(),
            arg_type: ArgumentType::Number,
            description: "Dry-bulb ambient temperature in °C".into(),
            required: true,
            default_value: Some(serde_json::json!(24.0)),
            validation_rules: vec![ValidationRule::Min(-50.0), ValidationRule::Max(85.0)],
        })
        .with_argument(CommandArgument {
            name: "relative_humidity".into(),
            arg_type: ArgumentType::Number,
            description: "Relative humidity (0.0 to 1.0 or 0.0 to 100.0%)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.50)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(100.0)],
        }),
    )?;

    // --- 26. Stormwater Runoff Hydrology & Manning Pipe Sizing ---
    registry.register(
        CommandSchema::new(
            "sim.civil.hydrology",
            "Rational Stormwater Runoff & Manning Culvert Pipe Sizing",
            "Calculates watershed Kirpich time of concentration, IDF rainfall intensity, peak runoff Q_peak, and Manning minimum required storm sewer diameter.",
            CommandCategory::Bim,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "catchment_area_ha".into(),
            arg_type: ArgumentType::Number,
            description: "Catchment drainage area in hectares (1 ha = 10,000 m2)".into(),
            required: true,
            default_value: Some(serde_json::json!(8.5)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "flow_path_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Longest overland/channel drainage path length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(450.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "terrain_slope".into(),
            arg_type: ArgumentType::Number,
            description: "Average terrain slope in m/m (e.g. 0.02 for 2%)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.020)),
            validation_rules: vec![ValidationRule::Min(0.0001)],
        })
        .with_argument(CommandArgument {
            name: "pipe_slope".into(),
            arg_type: ArgumentType::Number,
            description: "Storm sewer pipe longitudinal gradient in m/m (e.g. 0.008 for 0.8%)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.008)),
            validation_rules: vec![ValidationRule::Min(0.0001)],
        }),
    )?;

    // --- 27. Linear Elastic Fracture Mechanics & Paris Crack Growth ---
    registry.register(
        CommandSchema::new(
            "sim.structural.fracture",
            "LEFM Stress Intensity K_I & Paris Fatigue Crack Growth",
            "Calculates Mode I stress intensity factor K_I, fracture toughness margin K_IC, critical fast-fracture flaw size a_c, and Paris fatigue cycle life N_f.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "max_stress_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "Maximum cyclic tensile stress in MPa".into(),
            required: true,
            default_value: Some(serde_json::json!(180.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "min_stress_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "Minimum cyclic tensile stress in MPa".into(),
            required: false,
            default_value: Some(serde_json::json!(18.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_crack_depth_a0_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Initial detected flaw depth a_0 in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(1.5)),
            validation_rules: vec![ValidationRule::Min(0.05)],
        })
        .with_argument(CommandArgument {
            name: "plate_width_w_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Structural section width W in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 28. Rotor Dynamics & Critical Speeds (API 617) ---
    registry.register(
        CommandSchema::new(
            "sim.rotordynamics.api617",
            "Lateral Rotor Dynamics, Critical Speeds & Campbell Split",
            "Calculates rigid and flexible-bearing lateral critical speeds, gyroscopic Campbell whirl splitting, unbalance vibration response, and API 617 separation margins.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "operating_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Operating rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(12000.0)),
            validation_rules: vec![ValidationRule::Min(100.0)],
        })
        .with_argument(CommandArgument {
            name: "shaft_span_m".into(),
            arg_type: ArgumentType::Number,
            description: "Bearing-to-bearing shaft span in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1.20)),
            validation_rules: vec![ValidationRule::Min(0.05)],
        })
        .with_argument(CommandArgument {
            name: "shaft_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Shaft outer diameter in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(0.080)),
            validation_rules: vec![ValidationRule::Min(0.005)],
        })
        .with_argument(CommandArgument {
            name: "disc_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Center disc / impeller mass in kg".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        }),
    )?;

    // --- 29. Welding Metallurgy & Cooling Rates (AWS D1.1 / ISO 15614) ---
    registry.register(
        CommandSchema::new(
            "cam.welding.metallurgy",
            "Welding Metallurgy, Net Heat Input & t8/5 Cooling Time",
            "Calculates net arc heat input, 2D/3D Rosenthal cooling rates (t8/5), IIW/Ito-Bessyo Carbon Equivalents, preheat temperatures, and peak HAZ hardness.",
            CommandCategory::Cnc,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "arc_voltage_v".into(),
            arg_type: ArgumentType::Number,
            description: "Arc voltage in Volts".into(),
            required: true,
            default_value: Some(serde_json::json!(28.0)),
            validation_rules: vec![ValidationRule::Min(5.0)],
        })
        .with_argument(CommandArgument {
            name: "welding_current_a".into(),
            arg_type: ArgumentType::Number,
            description: "Welding current in Amperes".into(),
            required: true,
            default_value: Some(serde_json::json!(240.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "travel_speed_mm_s".into(),
            arg_type: ArgumentType::Number,
            description: "Torch travel speed in mm/s".into(),
            required: true,
            default_value: Some(serde_json::json!(4.5)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "plate_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Base plate thickness in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(25.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 30. Binary Distillation Column Design (McCabe-Thiele) ---
    registry.register(
        CommandSchema::new(
            "sim.chemical.distillation",
            "Binary Distillation Column Design & McCabe-Thiele Stepping",
            "Calculates Fenske minimum theoretical stages, Underwood minimum reflux R_min, McCabe-Thiele stage stepping, optimal feed tray, and condenser/reboiler duty.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "feed_rate_kmol_hr".into(),
            arg_type: ArgumentType::Number,
            description: "Feed molar rate in kmol/hr".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "feed_mole_fraction_zf".into(),
            arg_type: ArgumentType::Number,
            description: "Feed light key mole fraction (0.0 to 1.0)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.45)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(0.99)],
        })
        .with_argument(CommandArgument {
            name: "distillate_mole_fraction_xd".into(),
            arg_type: ArgumentType::Number,
            description: "Distillate product mole fraction (0.0 to 1.0)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.95)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(0.999)],
        })
        .with_argument(CommandArgument {
            name: "bottoms_mole_fraction_xb".into(),
            arg_type: ArgumentType::Number,
            description: "Bottoms product mole fraction (0.0 to 1.0)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.05)),
            validation_rules: vec![ValidationRule::Min(0.001), ValidationRule::Max(0.99)],
        })
        .with_argument(CommandArgument {
            name: "relative_volatility_alpha".into(),
            arg_type: ArgumentType::Number,
            description: "Relative volatility alpha (alpha = K_light / K_heavy)".into(),
            required: true,
            default_value: Some(serde_json::json!(2.45)),
            validation_rules: vec![ValidationRule::Min(1.05)],
        }),
    )?;

    // --- 31. PCB Trace Ampacity & Thermal Vias (IPC-2152) ---
    registry.register(
        CommandSchema::new(
            "sim.ecad.pcb_thermal",
            "PCB Trace Current Ampacity & Thermal Via Arrays (IPC-2152)",
            "Calculates allowable continuous current for temperature rise, copper trace Joule heating, and thermal via conduction matrix.",
            CommandCategory::Electronics,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "trace_width_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Trace width in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(2.50)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "trace_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Trace length in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(50.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "current_a".into(),
            arg_type: ArgumentType::Number,
            description: "Continuous current in Amperes".into(),
            required: true,
            default_value: Some(serde_json::json!(5.5)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "allowable_temp_rise_c".into(),
            arg_type: ArgumentType::Number,
            description: "Allowable temperature rise in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 32. Structural Connections (Eurocode 3 / AISC 360) ---
    registry.register(
        CommandSchema::new(
            "sim.structural.connections",
            "Structural Steel Bolted & Welded Connections (EC3 / AISC 360)",
            "Calculates bolt shear resistance, plate bearing, bolt tension, combined interaction, and fillet weld directional throat stresses.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "applied_shear_force_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Design applied shear force V_Ed in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(180.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_tension_force_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Design applied tension force N_Ed in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "weld_throat_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Fillet weld throat thickness a in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(6.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "weld_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Total fillet weld effective length in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(220.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_weld_force_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Design applied force on fillet weld in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(150.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        }),
    )?;

    // --- 33. Centrifugal Pumps & Affinity Laws (ISO 9906) ---
    registry.register(
        CommandSchema::new(
            "sim.hydraulic.pump",
            "Centrifugal Pump Performance, Affinity Laws & Operating Point (ISO 9906)",
            "Calculates head-flow operating point, speed/diameter affinity scaling, brake horsepower, hydraulic efficiency, and NPSH cavitation margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal pump speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(2950.0)),
            validation_rules: vec![ValidationRule::Min(100.0)],
        })
        .with_argument(CommandArgument {
            name: "nominal_impeller_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal impeller diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(250.0)),
            validation_rules: vec![ValidationRule::Min(20.0)],
        })
        .with_argument(CommandArgument {
            name: "shutoff_head_m".into(),
            arg_type: ArgumentType::Number,
            description: "Shutoff head H0 in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "system_static_head_m".into(),
            arg_type: ArgumentType::Number,
            description: "System static elevation head in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(25.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "npsh_available_m".into(),
            arg_type: ArgumentType::Number,
            description: "Net Positive Suction Head Available (NPSHA) in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(6.5)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        }),
    )?;

    // --- 34. Shell-and-Tube Heat Exchangers (TEMA / Kern's Method) ---
    registry.register(
        CommandSchema::new(
            "sim.thermal.heat_exchanger_rating",
            "Shell-and-Tube Heat Exchanger Rating & Kern Method (TEMA)",
            "Calculates LMTD, F-correction factor, Kern shell-side Nu/ho, tube-side Dittus-Boelter hi, overall clean/fouled U, surface area margin, and shell/tube pressure drops.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "hot_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Hot stream mass flowrate in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(18.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "cold_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Cold stream mass flowrate in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(24.66)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "shell_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Shell internal diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(600.0)),
            validation_rules: vec![ValidationRule::Min(100.0)],
        })
        .with_argument(CommandArgument {
            name: "tube_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Total number of heat transfer tubes".into(),
            required: true,
            default_value: Some(serde_json::json!(380)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "tube_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal tube length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(4.5)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        }),
    )?;

    // --- 35. Geotechnical Bearing Capacity & Settlement (Meyerhof / EC7) ---
    registry.register(
        CommandSchema::new(
            "sim.civil.geotechnical_bearing",
            "Shallow Foundation Bearing Capacity & Elastic Settlement (Meyerhof / EC7)",
            "Calculates ultimate bearing capacity q_ult, allowable capacity q_all, factor of safety, and elastic immediate settlement under column loading.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "foundation_width_b_m".into(),
            arg_type: ArgumentType::Number,
            description: "Footing width B in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.20)),
            validation_rules: vec![ValidationRule::Min(0.2)],
        })
        .with_argument(CommandArgument {
            name: "foundation_length_l_m".into(),
            arg_type: ArgumentType::Number,
            description: "Footing length L in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.20)),
            validation_rules: vec![ValidationRule::Min(0.2)],
        })
        .with_argument(CommandArgument {
            name: "embedment_depth_df_m".into(),
            arg_type: ArgumentType::Number,
            description: "Embedment depth D_f in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1.50)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "column_axial_load_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Applied column vertical load in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(1250.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "friction_angle_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Effective soil internal friction angle phi' in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(32.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(50.0)],
        }),
    )?;

    // --- 36. Multi-Stage Gas Compression & Intercooling (ASME PTC 10) ---
    registry.register(
        CommandSchema::new(
            "sim.thermal.gas_compression",
            "Multi-Stage Gas Compression & Intercooling Thermodynamics (ASME PTC 10)",
            "Calculates stage pressure ratios, polytropic head, discharge temperatures, intercooling heat duties, total brake power, and energy savings.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Gas mass flowrate in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(4.5)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "inlet_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Overall inlet suction pressure in bar".into(),
            required: true,
            default_value: Some(serde_json::json!(1.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "target_outlet_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Target final discharge pressure in bar".into(),
            required: true,
            default_value: Some(serde_json::json!(30.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "number_of_stages".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of compression stages".into(),
            required: true,
            default_value: Some(serde_json::json!(3)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "polytropic_efficiency".into(),
            arg_type: ArgumentType::Number,
            description: "Stage polytropic efficiency (0.5 to 0.95)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.82)),
            validation_rules: vec![ValidationRule::Min(0.50), ValidationRule::Max(0.98)],
        }),
    )?;

    // --- 37. Structural Timber Fire Resistance (Eurocode 5 EN 1995-1-2) ---
    registry.register(
        CommandSchema::new(
            "sim.structural.timber_fire",
            "Structural Timber Fire Resistance & Charring Kinetics (Eurocode 5)",
            "Calculates ISO 834 fire temperatures, 1D/effective charring depth, reduced residual cross-section, accidental fire design bending moment, and R30/R60/R90 verification.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "initial_breadth_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Initial timber breadth in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(200.0)),
            validation_rules: vec![ValidationRule::Min(20.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_depth_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Initial timber depth in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(500.0)),
            validation_rules: vec![ValidationRule::Min(20.0)],
        })
        .with_argument(CommandArgument {
            name: "fire_duration_min".into(),
            arg_type: ArgumentType::Number,
            description: "Fire exposure duration in minutes (e.g. 30, 60, 90)".into(),
            required: true,
            default_value: Some(serde_json::json!(60.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "ambient_moment_ed_kn_m".into(),
            arg_type: ArgumentType::Number,
            description: "Cold ULS design bending moment in kNm".into(),
            required: true,
            default_value: Some(serde_json::json!(140.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "fire_load_ratio_eta_fi".into(),
            arg_type: ArgumentType::Number,
            description: "Accidental fire load combination factor eta_fi".into(),
            required: false,
            default_value: Some(serde_json::json!(0.60)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1.0)],
        }),
    )?;

    // --- 38. Marine Propeller Hydrodynamics (Wageningen B-Series / ITTC) ---
    registry.register(
        CommandSchema::new(
            "sim.marine.propeller",
            "Marine Propeller Open-Water Hydrodynamics & Keller Cavitation (ITTC)",
            "Calculates Wageningen B-series advance coefficient J, KT, KQ, open-water efficiency eta_0, thrust, delivered power, and Keller cavitation margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Propeller outer diameter in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(6.80)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "pitch_ratio_p_d".into(),
            arg_type: ArgumentType::Number,
            description: "Propeller pitch-to-diameter ratio P/D".into(),
            required: true,
            default_value: Some(serde_json::json!(1.05)),
            validation_rules: vec![ValidationRule::Min(0.4), ValidationRule::Max(2.0)],
        })
        .with_argument(CommandArgument {
            name: "rotational_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Propeller rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(108.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "advance_speed_knots".into(),
            arg_type: ArgumentType::Number,
            description: "Ship advance speed through water in knots".into(),
            required: true,
            default_value: Some(serde_json::json!(17.5)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 39. Automotive Braking & Disc Rotor Thermal Dynamics (ECE R13H / SAE J292) ---
    registry.register(
        CommandSchema::new(
            "sim.automotive.braking",
            "Automotive Braking Dynamics & Disc Rotor Thermal Dissipation (ECE R13H)",
            "Calculates dynamic longitudinal axle load transfer, brake torque distribution, stopping distance, kinetic energy dissipation, and rotor adiabatic temperature rise.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "vehicle_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Total vehicle mass in kg".into(),
            required: true,
            default_value: Some(serde_json::json!(1850.0)),
            validation_rules: vec![ValidationRule::Min(200.0)],
        })
        .with_argument(CommandArgument {
            name: "wheelbase_m".into(),
            arg_type: ArgumentType::Number,
            description: "Wheelbase in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.85)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_speed_kph".into(),
            arg_type: ArgumentType::Number,
            description: "Initial vehicle speed in km/h".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "road_friction_coeff".into(),
            arg_type: ArgumentType::Number,
            description: "Tire-road peak friction coefficient mu".into(),
            required: false,
            default_value: Some(serde_json::json!(0.92)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1.5)],
        }),
    )?;

    // --- 40. Reinforced Concrete Flexure & Crack Width (Eurocode 2 EN 1992-1-1) ---
    registry.register(
        CommandSchema::new(
            "sim.civil.reinforced_concrete",
            "Reinforced Concrete Beam Flexural ULS Capacity & SLS Crack Width (EC2)",
            "Calculates ultimate moment resistance M_Rd, neutral axis depth, ductility ratio xi, steel service stress, and characteristic crack width w_k.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "breadth_b_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Beam breadth b in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(300.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "depth_h_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Beam overall depth h in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(600.0)),
            validation_rules: vec![ValidationRule::Min(100.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_moment_ed_kn_m".into(),
            arg_type: ArgumentType::Number,
            description: "Applied design bending moment M_Ed in kNm".into(),
            required: true,
            default_value: Some(serde_json::json!(240.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "rebar_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of main tensile rebars".into(),
            required: true,
            default_value: Some(serde_json::json!(4)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "rebar_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Tensile rebar diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(6.0)],
        }),
    )?;

    // --- 41. HVAC Airflow, Duct Pressure Loss & Fan Power (ASHRAE / SMACNA) ---
    registry.register(
        CommandSchema::new(
            "sim.hvac.duct_airflow",
            "HVAC Airflow Duct Hydraulics, Pressure Drop & Fan Power (ASHRAE)",
            "Calculates Huebscher equivalent hydraulic diameter, Colebrook-White friction factors, dynamic fitting losses, total external static pressure (ESP), and fan brake power.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "airflow_m3_h".into(),
            arg_type: ArgumentType::Number,
            description: "Volumetric airflow rate in m³/h".into(),
            required: true,
            default_value: Some(serde_json::json!(5400.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "duct_width_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Rectangular duct width in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(600.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "duct_height_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Rectangular duct height in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(350.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        })
        .with_argument(CommandArgument {
            name: "duct_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Straight duct total run length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    // --- 42. Aero Gas Turbine Open Brayton Cycle & Thrust (NASA SP-290 / ISO 2314) ---
    registry.register(
        CommandSchema::new(
            "sim.aerospace.gas_turbine",
            "Aero Gas Turbine Open Brayton Cycle & Propulsion Performance (NASA SP-290)",
            "Calculates aircraft gas turbine thermodynamic state stations (0-2-3-4-5-9), fuel-air ratio, nozzle jet velocity, net thrust, TSFC, and thermal / propulsive efficiencies.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "flight_mach".into(),
            arg_type: ArgumentType::Number,
            description: "Flight Mach number (e.g. 0.82 cruise)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.82)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(4.0)],
        })
        .with_argument(CommandArgument {
            name: "altitude_m".into(),
            arg_type: ArgumentType::Number,
            description: "Flight altitude in meters (e.g. 10668m / 35,000 ft)".into(),
            required: true,
            default_value: Some(serde_json::json!(10668.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(30000.0)],
        })
        .with_argument(CommandArgument {
            name: "core_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Core air mass flowrate in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "compressor_pressure_ratio".into(),
            arg_type: ArgumentType::Number,
            description: "Compressor overall pressure ratio OPR (Pt3/Pt2)".into(),
            required: true,
            default_value: Some(serde_json::json!(24.0)),
            validation_rules: vec![ValidationRule::Min(1.5), ValidationRule::Max(70.0)],
        })
        .with_argument(CommandArgument {
            name: "turbine_inlet_temp_k".into(),
            arg_type: ArgumentType::Number,
            description: "Turbine Inlet Temperature TIT in Kelvin".into(),
            required: true,
            default_value: Some(serde_json::json!(1650.0)),
            validation_rules: vec![ValidationRule::Min(600.0), ValidationRule::Max(2400.0)],
        }),
    )?;

    // --- 43. Hydraulic Cylinder & Actuator Mechanics (ISO 6020-2 / NFPA T3.6.7 R2) ---
    registry.register(
        CommandSchema::new(
            "sim.hydraulic.cylinder",
            "Hydraulic Actuator Sizing, Barrel Lamé Stresses & Rod Buckling (ISO 6020-2)",
            "Calculates hydraulic cylinder push/pull forces, thick-walled barrel hoop and von Mises stresses, Euler/Rankine-Gordon rod column buckling, and cushion deceleration peak pressures.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "bore_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Cylinder bore inner diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "rod_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Piston rod diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(56.0)),
            validation_rules: vec![ValidationRule::Min(8.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "stroke_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Actuator stroke length in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(600.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(10000.0)],
        })
        .with_argument(CommandArgument {
            name: "working_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Hydraulic system working pressure in bar".into(),
            required: true,
            default_value: Some(serde_json::json!(210.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(700.0)],
        })
        .with_argument(CommandArgument {
            name: "barrel_wall_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Cylinder barrel wall thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(2.0)],
        }),
    )?;

    // --- 44. Belt & Pulley Power Transmission (ISO 5296 / RMA IP-20) ---
    registry.register(
        CommandSchema::new(
            "sim.mechanical.belt_drive",
            "Belt & Pulley Transmission Kinematics, Tensioning & Fatigue Life (ISO 5296)",
            "Calculates belt linear speed, pulley wrap angles, Euler-Eytelwein friction drive tensioning, centrifugal tension, parallel belt count, shaft loads, and fatigue hours.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "power_kw".into(),
            arg_type: ArgumentType::Number,
            description: "Transmitted mechanical power in kW".into(),
            required: true,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "driver_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Driving pulley speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(1450.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "driver_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Driving pulley pitch diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(160.0)),
            validation_rules: vec![ValidationRule::Min(20.0)],
        })
        .with_argument(CommandArgument {
            name: "driven_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Driven pulley pitch diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(315.0)),
            validation_rules: vec![ValidationRule::Min(20.0)],
        })
        .with_argument(CommandArgument {
            name: "center_distance_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Shaft center distance in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(600.0)),
            validation_rules: vec![ValidationRule::Min(50.0)],
        }),
    )?;

    // --- 45. Soil Slope Stability Analysis (Bishop's Method of Slices / Eurocode 7) ---
    registry.register(
        CommandSchema::new(
            "sim.geotechnical.slope_stability",
            "Soil Slope Stability Analysis via Bishop's Method of Slices (Eurocode 7 / USACE)",
            "Calculates circular slip surface limit equilibrium, slice discretization, Bishop and Fellenius Factors of Safety (FoS), pore water pressure ratio (ru), and pseudo-static seismic inertia (kh).",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "slope_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Vertical slope height in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(10.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "slope_angle_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Slope angle from horizontal in degrees".into(),
            required: true,
            default_value: Some(serde_json::json!(32.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(85.0)],
        })
        .with_argument(CommandArgument {
            name: "soil_unit_weight_kn_m3".into(),
            arg_type: ArgumentType::Number,
            description: "Soil unit weight in kN/m3".into(),
            required: false,
            default_value: Some(serde_json::json!(19.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(30.0)],
        })
        .with_argument(CommandArgument {
            name: "effective_cohesion_kpa".into(),
            arg_type: ArgumentType::Number,
            description: "Effective soil cohesion c' in kPa".into(),
            required: false,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(0.0)],
        })
        .with_argument(CommandArgument {
            name: "effective_friction_angle_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Effective soil friction angle phi' in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(27.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(55.0)],
        }),
    )?;

    // --- 46. Three-Phase Induction Motor Electromechanics & Torque-Speed (IEEE 112 / NEMA MG-1) ---
    registry.register(
        CommandSchema::new(
            "sim.electrical.induction_motor",
            "Three-Phase Induction Motor Electromechanics & Torque-Speed (IEEE 112 / NEMA MG-1)",
            "Solves per-phase equivalent circuit, Thevenin transformation, breakdown torque, starting current and torque, power factor, and electromechanical efficiency.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "rated_power_kw".into(),
            arg_type: ArgumentType::Number,
            description: "Rated shaft output power in kW".into(),
            required: true,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(0.25), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_voltage_ll_v".into(),
            arg_type: ArgumentType::Number,
            description: "Line-to-line RMS voltage in Volts (e.g. 400V)".into(),
            required: true,
            default_value: Some(serde_json::json!(400.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(15000.0)],
        })
        .with_argument(CommandArgument {
            name: "frequency_hz".into(),
            arg_type: ArgumentType::Number,
            description: "Supply grid frequency in Hz (50 or 60)".into(),
            required: true,
            default_value: Some(serde_json::json!(50.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(400.0)],
        })
        .with_argument(CommandArgument {
            name: "pole_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Magnetic pole count (2, 4, 6, 8)".into(),
            required: true,
            default_value: Some(serde_json::json!(4)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(16.0)],
        }),
    )?;

    // --- 47. Solar Thermal Collector & Heat Pipe Efficiency (ISO 9806 / EN 12975) ---
    registry.register(
        CommandSchema::new(
            "sim.thermal.solar_collector",
            "Solar Thermal Collector & Heat Pipe Efficiency Rating (ISO 9806 / EN 12975)",
            "Evaluates flat-plate and evacuated-tube solar collector efficiency curves, fluid temperature gain, useful thermal power gain, and stagnation temperatures.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "aperture_area_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Net optical aperture area in m2".into(),
            required: true,
            default_value: Some(serde_json::json!(2.30)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "solar_irradiance_w_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Incident plane-of-array solar irradiance in W/m2".into(),
            required: true,
            default_value: Some(serde_json::json!(850.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "ambient_temp_c".into(),
            arg_type: ArgumentType::Number,
            description: "Ambient air temperature in degrees Celsius".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(-40.0), ValidationRule::Max(60.0)],
        })
        .with_argument(CommandArgument {
            name: "fluid_inlet_temp_c".into(),
            arg_type: ArgumentType::Number,
            description: "Fluid inlet temperature in degrees Celsius".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(-20.0), ValidationRule::Max(150.0)],
        })
        .with_argument(CommandArgument {
            name: "fluid_flow_rate_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Fluid mass flowrate in kg/s".into(),
            required: false,
            default_value: Some(serde_json::json!(0.045)),
            validation_rules: vec![ValidationRule::Min(0.001)],
        }),
    )?;

    // --- 48. Hydrodynamic Journal Bearings (ISO 7902 / DIN 31652) ---
    registry.register(
        CommandSchema::new(
            "sim.tribology.journal_bearing",
            "Hydrodynamic Fluid-Film Journal Bearing Analysis (ISO 7902 / DIN 31652)",
            "Evaluates fluid-film lubrication, Sommerfeld number, eccentricity ratio, minimum oil film thickness, friction torque, power loss, and oil temperature rise.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "journal_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Journal shaft diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "bearing_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Bearing sleeve length in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(70.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "radial_clearance_um".into(),
            arg_type: ArgumentType::Number,
            description: "Radial clearance in micrometers (um)".into(),
            required: true,
            default_value: Some(serde_json::json!(65.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "rotational_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Shaft rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(3000.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "radial_load_n".into(),
            arg_type: ArgumentType::Number,
            description: "Radial load supported by bearing in Newtons".into(),
            required: true,
            default_value: Some(serde_json::json!(12500.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        }),
    )?;

    // --- 49. Wind Turbine Aerodynamics & BEM Power Curve (IEC 61400-1) ---
    registry.register(
        CommandSchema::new(
            "sim.renewable.wind_turbine",
            "Wind Turbine Aerodynamics, BEM Power Curve & AEP (IEC 61400-1)",
            "Evaluates horizontal-axis wind turbine swept area, tip speed ratio, Betz limit performance, pitch-regulated power curve, aerodynamic thrust, and Weibull Annual Energy Production (AEP).",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "rotor_radius_m".into(),
            arg_type: ArgumentType::Number,
            description: "Rotor blade radius in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(63.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(150.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_power_mw".into(),
            arg_type: ArgumentType::Number,
            description: "Rated generator capacity in Megawatts (MW)".into(),
            required: true,
            default_value: Some(serde_json::json!(4.0)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(25.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_wind_speed_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Rated wind speed in m/s".into(),
            required: true,
            default_value: Some(serde_json::json!(11.5)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(25.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_wind_speed_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Operating wind speed for instantaneous performance in m/s".into(),
            required: false,
            default_value: Some(serde_json::json!(9.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(50.0)],
        }),
    )?;

    // --- 50. Steam Rankine Cycle & Condenser Thermodynamics (IAPWS-IF97 / ASME PTC 6) ---
    registry.register(
        CommandSchema::new(
            "sim.thermal.steam_rankine",
            "Steam Rankine Power Cycle & Condenser Thermodynamics (IAPWS-IF97)",
            "Solves superheated steam turbine expansion, wet exhaust quality, boiler heat input, net work, thermal efficiency, gross MW output, and condenser cooling duty.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "steam_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Main steam mass flow rate in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        })
        .with_argument(CommandArgument {
            name: "boiler_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Boiler operating pressure in bar".into(),
            required: true,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(300.0)],
        })
        .with_argument(CommandArgument {
            name: "superheater_temp_c".into(),
            arg_type: ArgumentType::Number,
            description: "Superheater throttle temperature in degrees Celsius".into(),
            required: true,
            default_value: Some(serde_json::json!(540.0)),
            validation_rules: vec![ValidationRule::Min(150.0), ValidationRule::Max(700.0)],
        })
        .with_argument(CommandArgument {
            name: "condenser_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Condenser back-pressure in bar".into(),
            required: false,
            default_value: Some(serde_json::json!(0.08)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(1.0)],
        }),
    )?;

    // --- 51. Deep Foundation Bored Pile Load Capacity & Settlement (Eurocode 7 / API RP 2A) ---
    registry.register(
        CommandSchema::new(
            "sim.civil.pile_foundation",
            "Deep Foundation Bored Pile Bearing Capacity & Settlement (Eurocode 7)",
            "Evaluates stratified shaft skin friction (alpha & beta methods), base end-bearing, Eurocode 7 design compressive resistance (DA1/DA2), and elastic working settlement.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "pile_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Bored pile circular diameter in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1.0)),
            validation_rules: vec![ValidationRule::Min(0.3), ValidationRule::Max(4.0)],
        })
        .with_argument(CommandArgument {
            name: "pile_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Embedded pile length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_axial_load_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Applied design axial working load in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(3200.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        }),
    )?;

    // --- 52. Gear Tooth Pitting & Bending Fatigue Life (ISO 6336 Method B) ---
    registry.register(
        CommandSchema::new(
            "sim.mechanical.gear_fatigue",
            "Involute Gear Pitting & Tooth Root Bending Fatigue Rating (ISO 6336)",
            "Evaluates Hertzian surface contact stress (sigma_H), pitting safety factors (S_H), root fillet bending stress (sigma_F), and tooth breakage fatigue safety factors (S_F).",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "transmitted_power_kw".into(),
            arg_type: ArgumentType::Number,
            description: "Transmitted mechanical power in kW".into(),
            required: true,
            default_value: Some(serde_json::json!(55.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "pinion_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Pinion rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(1450.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "pinion_teeth_z1".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of teeth on pinion".into(),
            required: true,
            default_value: Some(serde_json::json!(21)),
            validation_rules: vec![ValidationRule::Min(8.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "gear_teeth_z2".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of teeth on wheel gear".into(),
            required: true,
            default_value: Some(serde_json::json!(63)),
            validation_rules: vec![ValidationRule::Min(8.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "normal_module_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Normal module in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(4.0)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "face_width_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Gear tooth face width in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(50.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.battery_thermal",
            "Battery Pack Liquid Cooling & Thermal Management (IEC 62660 / SAE J2464)",
            "Evaluates lithium-ion cell heat generation, liquid cooling plate convective dissipation, coolant temperature rise across channels, and inter-cell thermal gradients.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "cell_count_series".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of cells in series".into(),
            required: true,
            default_value: Some(serde_json::json!(96)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "cell_count_parallel".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of cell strings in parallel".into(),
            required: true,
            default_value: Some(serde_json::json!(4)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_current_a".into(),
            arg_type: ArgumentType::Number,
            description: "Total pack continuous discharge current in A".into(),
            required: true,
            default_value: Some(serde_json::json!(200.0)),
            validation_rules: vec![ValidationRule::Min(0.1)],
        })
        .with_argument(CommandArgument {
            name: "coolant_flow_rate_l_min".into(),
            arg_type: ArgumentType::Number,
            description: "Liquid coolant volume flow rate in L/min".into(),
            required: true,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.rocket_nozzle",
            "Rocket Engine De Laval Nozzle Supersonic Gas Dynamics (NASA SP-8039)",
            "Evaluates isentropic compressible expansion, throat critical parameters, exit Mach number, sea-level and vacuum specific impulses, and Summerfield flow separation.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "chamber_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Stagnation chamber pressure in bar".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "chamber_temperature_k".into(),
            arg_type: ArgumentType::Number,
            description: "Combustion chamber temperature in Kelvin".into(),
            required: true,
            default_value: Some(serde_json::json!(3500.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "throat_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Nozzle throat diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "expansion_ratio_eps".into(),
            arg_type: ArgumentType::Number,
            description: "Nozzle area expansion ratio Ae/At".into(),
            required: true,
            default_value: Some(serde_json::json!(16.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(300.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.chemical.cstr_reactor",
            "CSTR Chemical Reactor Kinetics & Thermal Multiplicity (Fogler / DIERS)",
            "Evaluates Arrhenius reaction kinetics, Damköhler number, Van Heerden heat generation vs removal curves, and thermal runaway ignition stability margins.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "reactor_volume_m3".into(),
            arg_type: ArgumentType::Number,
            description: "Liquid reactor volume in m³".into(),
            required: true,
            default_value: Some(serde_json::json!(2.50)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "feed_volumetric_flow_m3_h".into(),
            arg_type: ArgumentType::Number,
            description: "Feed volumetric flow rate in m³/h".into(),
            required: true,
            default_value: Some(serde_json::json!(5.0)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "feed_reactant_concentration_mol_m3".into(),
            arg_type: ArgumentType::Number,
            description: "Reactant feed concentration in mol/m³".into(),
            required: true,
            default_value: Some(serde_json::json!(2000.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.civil.channel_hydraulics",
            "Open Channel Flow Manning Hydraulics & Hydraulic Jump (Chow / USBR)",
            "Evaluates uniform open channel flow, normal and critical depths, Froude number regime, sequent conjugate depths, and turbulent energy dissipation across hydraulic jumps.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "bottom_width_m".into(),
            arg_type: ArgumentType::Number,
            description: "Channel bed bottom width in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(6.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "side_slope_z".into(),
            arg_type: ArgumentType::Number,
            description: "Side slope ratio z:1 (horizontal to vertical)".into(),
            required: true,
            default_value: Some(serde_json::json!(1.5)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "discharge_rate_m3_s".into(),
            arg_type: ArgumentType::Number,
            description: "Design volumetric discharge rate in m³/s".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(0.01)],
        })
        .with_argument(CommandArgument {
            name: "bed_slope_s0".into(),
            arg_type: ArgumentType::Number,
            description: "Longitudinal bed slope S0 (m/m)".into(),
            required: true,
            default_value: Some(serde_json::json!(0.0015)),
            validation_rules: vec![ValidationRule::Min(0.00001), ValidationRule::Max(0.5)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.electrical.transmission_line",
            "Overhead Transmission Line Catenary Sag & Tension (IEEE 738 / IEC 60826)",
            "Evaluates conductor catenary profile, change-of-state equation under thermal and storm loading, maximum hot sag, and mechanical factors of safety against breaking load.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "span_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Horizontal span length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(350.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "conductor_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Conductor outer diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(28.14)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_tensile_strength_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Rated tensile breaking strength in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(140.0)),
            validation_rules: vec![ValidationRule::Min(1.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_horizontal_tension_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Initial baseline tension at reference temp in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(28.0)),
            validation_rules: vec![ValidationRule::Min(0.5)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.piping.flexibility",
            "Piping Flexibility & Thermal Expansion Stress Analysis (ASME B31.3 / B31.1)",
            "Evaluates thermal displacement stress range, expansion U-loop sizing, flexibility factor, and anchor reaction loads for high-temperature process piping systems.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_pipe_size_inches".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal pipe size in inches (e.g. 8.0)".into(),
            required: false,
            default_value: Some(serde_json::json!(8.0)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(96.0)],
        })
        .with_argument(CommandArgument {
            name: "total_line_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Total anchor-to-anchor run length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(60.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "design_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Design operating temperature in degrees Celsius".into(),
            required: true,
            default_value: Some(serde_json::json!(260.0)),
            validation_rules: vec![ValidationRule::Min(-273.15), ValidationRule::Max(1200.0)],
        })
        .with_argument(CommandArgument {
            name: "expansion_loop_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Height of expansion U-loop in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(7.5)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.automotive.quarter_car",
            "Automotive Suspension Kinematics & Quarter-Car Ride Dynamics (ISO 2631 / Milliken)",
            "Evaluates two-degree-of-freedom quarter-car ride comfort, sprung and unsprung natural frequencies, RMS chassis acceleration, suspension stroke, and road roughness ISO PSD response.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "sprung_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Sprung quarter-car chassis mass in kg".into(),
            required: true,
            default_value: Some(serde_json::json!(400.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "unsprung_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Unsprung wheel and hub assembly mass in kg".into(),
            required: true,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "suspension_stiffness_n_m".into(),
            arg_type: ArgumentType::Number,
            description: "Suspension spring stiffness in N/m".into(),
            required: true,
            default_value: Some(serde_json::json!(30000.0)),
            validation_rules: vec![ValidationRule::Min(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "suspension_damping_n_s_m".into(),
            arg_type: ArgumentType::Number,
            description: "Suspension damper damping coefficient in N·s/m".into(),
            required: true,
            default_value: Some(serde_json::json!(1800.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "vehicle_speed_km_h".into(),
            arg_type: ArgumentType::Number,
            description: "Vehicle forward speed in km/h".into(),
            required: false,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.electrical.power_transformer",
            "Power Transformer Electromagnetics & Thermal Rating (IEC 60076 / IEEE C57)",
            "Evaluates transformer magnetic flux density, core and copper losses, percentage impedance, voltage regulation at rated power factor, winding hot-spot temperature, and relative rate of thermal aging.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "rated_power_mva".into(),
            arg_type: ArgumentType::Number,
            description: "Rated apparent power in MVA".into(),
            required: true,
            default_value: Some(serde_json::json!(40.0)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "primary_voltage_kv".into(),
            arg_type: ArgumentType::Number,
            description: "Primary line-to-line voltage in kV".into(),
            required: true,
            default_value: Some(serde_json::json!(132.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "secondary_voltage_kv".into(),
            arg_type: ArgumentType::Number,
            description: "Secondary line-to-line voltage in kV".into(),
            required: true,
            default_value: Some(serde_json::json!(33.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "percent_impedance".into(),
            arg_type: ArgumentType::Number,
            description: "Percentage short-circuit impedance (%Z)".into(),
            required: false,
            default_value: Some(serde_json::json!(10.5)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(50.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.thermal.refrigeration_cycle",
            "Vapor Compression Refrigeration Cycle & Heat Pump COP (ASHRAE / AHRI 550/590)",
            "Evaluates thermodynamic vapor-compression cycle states, evaporating/condensing pressures, COP in cooling and heating, compressor shaft power, and discharge temperature.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "refrigerant".into(),
            arg_type: ArgumentType::String,
            description: "Refrigerant fluid type (R410A, R134a, R32, R290)".into(),
            required: false,
            default_value: Some(serde_json::json!("R410A")),
            validation_rules: vec![],
        })
        .with_argument(CommandArgument {
            name: "design_cooling_capacity_kw".into(),
            arg_type: ArgumentType::Number,
            description: "Evaporator cooling capacity in kW".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100000.0)],
        })
        .with_argument(CommandArgument {
            name: "evaporating_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Evaporating saturation temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(5.0)),
            validation_rules: vec![ValidationRule::Min(-100.0), ValidationRule::Max(30.0)],
        })
        .with_argument(CommandArgument {
            name: "condensing_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Condensing saturation temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(120.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.marine.ship_stability",
            "Ship Hydrostatic Intact Stability & GZ Righting Arm (IMO Resolution MSC.267(85))",
            "Evaluates vessel hydrostatic displacement, transverse metacentric height GM0, righting arm GZ curve up to 60 deg, and IMO intact stability criteria compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "length_bp_m".into(),
            arg_type: ArgumentType::Number,
            description: "Ship length between perpendiculars in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "beam_m".into(),
            arg_type: ArgumentType::Number,
            description: "Molded breadth/beam in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "draft_m".into(),
            arg_type: ArgumentType::Number,
            description: "Design waterline draft in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(7.0)),
            validation_rules: vec![ValidationRule::Min(0.2), ValidationRule::Max(35.0)],
        })
        .with_argument(CommandArgument {
            name: "vertical_center_of_gravity_kg_m".into(),
            arg_type: ArgumentType::Number,
            description: "Vertical center of gravity above keel (KG) in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(7.2)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(50.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.manufacturing.injection_molding",
            "Plastic Injection Molding Process Physics & Clamp Tonnage (ISO 294 / SPE)",
            "Evaluates polymer melt cavity filling pressure, Fourier 1D transient cooling time, part cycle time, and machine clamping force requirement.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_wall_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal part wall thickness in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.5)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "projected_area_cm2".into(),
            arg_type: ArgumentType::Number,
            description: "Total projected cavity area in cm²".into(),
            required: true,
            default_value: Some(serde_json::json!(180.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "cavity_volume_cm3".into(),
            arg_type: ArgumentType::Number,
            description: "Part cavity volume in cm³".into(),
            required: true,
            default_value: Some(serde_json::json!(112.5)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100000.0)],
        })
        .with_argument(CommandArgument {
            name: "max_flow_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Maximum flow length from gate in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(175.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(5000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.civil.seismic_analysis",
            "Structural Seismic Response Spectrum & Base Shear (ASCE 7-22 / Eurocode 8)",
            "Evaluates building equivalent lateral force (ELF) procedure, design response spectrum, fundamental period, base shear Vb, story shear distribution, and roof drift.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "building_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Total building height above ground grade in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(36.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "story_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Total number of stories above base".into(),
            required: true,
            default_value: Some(serde_json::json!(10)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "total_seismic_weight_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Effective total building seismic weight in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(58000.0)),
            validation_rules: vec![ValidationRule::Min(10.0)],
        })
        .with_argument(CommandArgument {
            name: "short_period_spectral_accel_ss_g".into(),
            arg_type: ArgumentType::Number,
            description: "Mapped short-period spectral acceleration Ss (g)".into(),
            required: true,
            default_value: Some(serde_json::json!(1.20)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(5.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.orbital_mechanics",
            "Spacecraft Orbital Mechanics & Hohmann Transfer (Curtis / Vallado)",
            "Evaluates circular orbit velocities, semi-major axis, two-impulse Hohmann transfer delta-V budget, time of flight, Tsiolkovsky propellant mass, and J2 nodal precession.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "initial_altitude_km".into(),
            arg_type: ArgumentType::Number,
            description: "Initial parking orbit altitude above Earth in km".into(),
            required: true,
            default_value: Some(serde_json::json!(400.0)),
            validation_rules: vec![ValidationRule::Min(120.0), ValidationRule::Max(200000.0)],
        })
        .with_argument(CommandArgument {
            name: "target_altitude_km".into(),
            arg_type: ArgumentType::Number,
            description: "Target orbit altitude above Earth in km".into(),
            required: true,
            default_value: Some(serde_json::json!(35786.0)),
            validation_rules: vec![ValidationRule::Min(120.0), ValidationRule::Max(500000.0)],
        })
        .with_argument(CommandArgument {
            name: "spacecraft_wet_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Initial wet spacecraft mass in kilograms".into(),
            required: true,
            default_value: Some(serde_json::json!(3500.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500000.0)],
        })
        .with_argument(CommandArgument {
            name: "propulsion_isp_s".into(),
            arg_type: ArgumentType::Number,
            description: "Rocket engine effective specific impulse Isp (seconds)".into(),
            required: true,
            default_value: Some(serde_json::json!(318.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(5000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.electronics.dc_dc_converter",
            "Synchronous DC-DC Buck Converter Power Stage & Efficiency (IEEE / Erickson)",
            "Evaluates SMPS pulse-width modulation duty cycle, inductor ripple current, output voltage ripple, MOSFET conduction and switching losses, and overall conversion efficiency.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "input_voltage_v".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal DC input supply voltage in Volts".into(),
            required: true,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "output_voltage_v".into(),
            arg_type: ArgumentType::Number,
            description: "Regulated DC output load voltage in Volts".into(),
            required: true,
            default_value: Some(serde_json::json!(3.3)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_output_current_a".into(),
            arg_type: ArgumentType::Number,
            description: "Continuous DC output load current in Amperes".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "switching_frequency_hz".into(),
            arg_type: ArgumentType::Number,
            description: "PWM switching frequency in Hertz".into(),
            required: false,
            default_value: Some(serde_json::json!(500000.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(10000000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.science.fusion_tokamak",
            "Nuclear Fusion Tokamak Plasma Equilibrium & Confinement (Wesson / ITER)",
            "Evaluates D-T fusion reaction thermal power, edge safety factor q95, Troyon beta limit, Greenwald density limit, IPB98(y,2) H-mode energy confinement time, and fusion gain factor Q.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "major_radius_r0_m".into(),
            arg_type: ArgumentType::Number,
            description: "Plasma toroidal major radius in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(6.20)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "minor_radius_a_m".into(),
            arg_type: ArgumentType::Number,
            description: "Plasma poloidal minor radius in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.00)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(20.0)],
        })
        .with_argument(CommandArgument {
            name: "toroidal_magnetic_field_t".into(),
            arg_type: ArgumentType::Number,
            description: "On-axis toroidal magnetic field in Tesla".into(),
            required: true,
            default_value: Some(serde_json::json!(5.30)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(30.0)],
        })
        .with_argument(CommandArgument {
            name: "plasma_current_ma".into(),
            arg_type: ArgumentType::Number,
            description: "Total toroidal plasma current in MegaAmperes".into(),
            required: true,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.manufacturing.wire_edm",
            "Wire Electrical Discharge Machining Process Physics (ISO 230 / CIRP)",
            "Evaluates single-pulse discharge energy, Snoeys electro-thermal removal rate, linear cutting speed, kerf width, Ra surface roughness, recast layer, and wire deflection.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "workpiece_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Workpiece thickness along wire axis in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(50.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "wire_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Wire electrode nominal diameter in millimeters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.25)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(1.0)],
        })
        .with_argument(CommandArgument {
            name: "open_circuit_voltage_v".into(),
            arg_type: ArgumentType::Number,
            description: "Open-circuit discharge pulse voltage in Volts".into(),
            required: false,
            default_value: Some(serde_json::json!(95.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "discharge_current_a".into(),
            arg_type: ArgumentType::Number,
            description: "Average discharge pulse current in Amperes".into(),
            required: false,
            default_value: Some(serde_json::json!(16.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.renewable.wind_blade_aeroelastic",
            "Wind Turbine Blade Aerodynamic Bending & Flutter (IEC 61400-1 / Theodorsen)",
            "Evaluates centrifugal Southwell frequency stiffening, coupled flap-torsion flutter velocity, aerodynamic divergence, and 50-year extreme gust tip deflection.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "blade_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Blade structural length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_rotor_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Rated rotor rotational speed in RPM".into(),
            required: true,
            default_value: Some(serde_json::json!(10.5)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "extreme_gust_speed_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "50-year extreme gust speed in m/s (IEC 61400-1 DLC 6.1)".into(),
            required: false,
            default_value: Some(serde_json::json!(52.5)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(120.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.marine.subsea_pipeline",
            "Subsea Offshore Pipeline Wall Thickness & Hydrostatic Collapse (DNV-ST-F101)",
            "Evaluates hydrostatic seawater pressure at depth, elastic and plastic collapse, Timoshenko ovality interaction, burst yield pressure, and propagating buckling arrestors.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "outside_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Pipe nominal outside diameter in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(508.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(2500.0)],
        })
        .with_argument(CommandArgument {
            name: "nominal_wall_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal steel wall thickness in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(25.4)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "water_depth_m".into(),
            arg_type: ArgumentType::Number,
            description: "Seabed installation water depth in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1500.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(11000.0)],
        })
        .with_argument(CommandArgument {
            name: "internal_design_pressure_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "Internal design operating pressure in MegaPascals".into(),
            required: false,
            default_value: Some(serde_json::json!(22.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(200.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.combustor_aerothermal",
            "Gas Turbine Combustor Aerothermal Performance & NOx (Lefebvre / ICAO)",
            "Evaluates combustion efficiency, Rayleigh momentum and liner friction pressure loss, turbine inlet temperature, pattern factor, and thermal NOx/CO emissions indices.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "inlet_total_pressure_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "Combustor inlet total pressure P3 in MegaPascals".into(),
            required: true,
            default_value: Some(serde_json::json!(2.40)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "inlet_total_temperature_k".into(),
            arg_type: ArgumentType::Number,
            description: "Combustor inlet total temperature T3 in Kelvin".into(),
            required: true,
            default_value: Some(serde_json::json!(780.0)),
            validation_rules: vec![ValidationRule::Min(200.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "air_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Air mass flow rate through combustor in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(38.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "fuel_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Fuel mass flow rate in kg/s".into(),
            required: true,
            default_value: Some(serde_json::json!(0.76)),
            validation_rules: vec![ValidationRule::Min(0.001), ValidationRule::Max(50.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.bess_grid",
            "Battery Energy Storage System Container & Grid RTE (IEC 62933 / IEEE 2030.2.1)",
            "Evaluates electrochemical cell DC efficiency, PCS inverter losses, medium-voltage transformer efficiency, chiller/HVAC auxiliary loads, and AC-to-AC Round-Trip Efficiency.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_capacity_mwh".into(),
            arg_type: ArgumentType::Number,
            description: "Installed DC nameplate energy storage capacity in MegaWatt-hours".into(),
            required: true,
            default_value: Some(serde_json::json!(10.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_power_mw".into(),
            arg_type: ArgumentType::Number,
            description: "Rated continuous active power at Point of Interconnection in MegaWatts".into(),
            required: true,
            default_value: Some(serde_json::json!(2.50)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "c_rate".into(),
            arg_type: ArgumentType::Number,
            description: "Continuous operational charge/discharge C-rate".into(),
            required: false,
            default_value: Some(serde_json::json!(0.25)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(5.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.geotechnical.soil_consolidation",
            "Soil Consolidation & Foundation Settlement (Terzaghi / ASTM D2435 / Eurocode 7)",
            "Evaluates primary consolidation settlement, OCR, time-rate of consolidation via Fourier series dimensionless time factor Tv, pore water dissipation, and secondary creep.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "layer_thickness_m".into(),
            arg_type: ArgumentType::Number,
            description: "Clay layer initial thickness in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(6.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_effective_stress_kpa".into(),
            arg_type: ArgumentType::Number,
            description: "Initial vertical effective overburden stress in kPa".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "applied_stress_increment_kpa".into(),
            arg_type: ArgumentType::Number,
            description: "Applied net foundation stress increment in kPa".into(),
            required: true,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(5000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.hydraulic_turbine",
            "Hydraulic Turbine Pelton & Francis Energy Conversion (IEC 60193 / Euler)",
            "Evaluates gross and net head, water jet kinematics, runner speed ratio phi, Euler specific work, shaft power, hydraulic efficiency, Thoma cavitation factor, and generator output.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "net_head_m".into(),
            arg_type: ArgumentType::Number,
            description: "Net available hydraulic head in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(450.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(2500.0)],
        })
        .with_argument(CommandArgument {
            name: "flow_rate_m3_s".into(),
            arg_type: ArgumentType::Number,
            description: "Volumetric water flow rate in m³/s".into(),
            required: true,
            default_value: Some(serde_json::json!(3.20)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "runner_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Runner pitch diameter in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1.80)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(20.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.biomedical.stent_expansion",
            "Cardiovascular Stent Biomechanics & Radial Expansion (ASTM F3067 / FDA)",
            "Evaluates balloon expansion mechanics, crown plastic hinge moments, elastic recoil, longitudinal foreshortening, Radial Resistive Force (RRF), and strut fracture ductility margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_expanded_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Target expanded inner diameter in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(3.00)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(25.0)],
        })
        .with_argument(CommandArgument {
            name: "balloon_pressure_atm".into(),
            arg_type: ArgumentType::Number,
            description: "Inflation balloon pressure in atmospheres".into(),
            required: true,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(40.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.electronics.microwave_waveguide",
            "Microwave Rectangular Waveguide & Cavity Resonator (IEEE Std 148 / Pozar)",
            "Evaluates TE10 cutoff frequency, guide wavelength, phase and group velocities, wave impedance, skin depth, conductor attenuation, insertion loss, and TE101 cavity unloaded Q.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "broad_dimension_a_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Waveguide broad internal dimension 'a' in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(22.86)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "narrow_dimension_b_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Waveguide narrow internal dimension 'b' in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(10.16)),
            validation_rules: vec![ValidationRule::Min(0.2), ValidationRule::Max(250.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_frequency_ghz".into(),
            arg_type: ArgumentType::Number,
            description: "Carrier frequency in GigaHertz".into(),
            required: true,
            default_value: Some(serde_json::json!(10.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.electrical.gis_dielectric",
            "Gas-Insulated Switchgear High-Voltage Dielectric Breakdown (IEC 62271-203)",
            "Evaluates coaxial cylinder electric field stress, geometric field utilization factor, SF6/Eco-gas Paschen breakdown scaling, roughness enhancement, and LIWV impulse margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "rated_voltage_kv_rms".into(),
            arg_type: ArgumentType::Number,
            description: "Rated line-to-line RMS operating voltage in kV".into(),
            required: true,
            default_value: Some(serde_json::json!(420.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "conductor_radius_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Conductor outer radius in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(60.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(300.0)],
        })
        .with_argument(CommandArgument {
            name: "enclosure_radius_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Grounded enclosure inner radius in millimeters".into(),
            required: true,
            default_value: Some(serde_json::json!(165.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(800.0)],
        })
        .with_argument(CommandArgument {
            name: "gas_pressure_abs_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Gas absolute filling pressure in bar".into(),
            required: false,
            default_value: Some(serde_json::json!(5.50)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(20.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.pem_fuel_cell",
            "Proton Exchange Membrane Fuel Cell Polarization (Larminie / Barbir)",
            "Evaluates Nernst thermodynamic potential, Tafel activation overpotential, Springer Nafion membrane ohmic losses, concentration polarization, stack power, and hydrogen fuel consumption.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "cell_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of series electrochemical cells in stack".into(),
            required: true,
            default_value: Some(serde_json::json!(360)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "active_cell_area_cm2".into(),
            arg_type: ArgumentType::Number,
            description: "Active geometric MEA cell area in cm²".into(),
            required: true,
            default_value: Some(serde_json::json!(280.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Stack operating temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(120.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_current_density_a_cm2".into(),
            arg_type: ArgumentType::Number,
            description: "Operating current density in A/cm²".into(),
            required: false,
            default_value: Some(serde_json::json!(1.20)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(5.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.science.superconducting_magnet",
            "Superconducting Solenoid Magnet Field & Quench Protection (Wilson / CERN)",
            "Evaluates finite solenoid Biot-Savart field, stored magnetic energy, field/temperature-dependent critical current margin, Lorentz hoop stress, and quench dump resistor sizing with adiabatic hot-spot temperature.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "inner_radius_m".into(),
            arg_type: ArgumentType::Number,
            description: "Inner bore radius in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(0.35)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(5.0)],
        })
        .with_argument(CommandArgument {
            name: "outer_radius_m".into(),
            arg_type: ArgumentType::Number,
            description: "Outer winding radius in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(0.48)),
            validation_rules: vec![ValidationRule::Min(0.06), ValidationRule::Max(6.0)],
        })
        .with_argument(CommandArgument {
            name: "coil_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Axial coil length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1.25)),
            validation_rules: vec![ValidationRule::Min(0.10), ValidationRule::Max(20.0)],
        })
        .with_argument(CommandArgument {
            name: "turns_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Total winding turns count".into(),
            required: true,
            default_value: Some(serde_json::json!(2600)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_current_a".into(),
            arg_type: ArgumentType::Number,
            description: "Operating transport current in Amperes".into(),
            required: false,
            default_value: Some(serde_json::json!(1400.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(50000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.reentry_aerothermodynamics",
            "Hypersonic Reentry Aerothermodynamics & Stagnation Heat Flux (Sutton-Graves / Fay-Riddell)",
            "Evaluates hypersonic bow shock standoff distance, stagnation pressure and enthalpy, convective and shock-layer radiative heat flux, radiative equilibrium wall temperature, and TPS ablation thickness.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "freestream_velocity_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Freestream entry velocity in m/s".into(),
            required: true,
            default_value: Some(serde_json::json!(7400.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(15000.0)],
        })
        .with_argument(CommandArgument {
            name: "altitude_km".into(),
            arg_type: ArgumentType::Number,
            description: "Entry altitude in kilometers".into(),
            required: true,
            default_value: Some(serde_json::json!(62.0)),
            validation_rules: vec![ValidationRule::Min(15.0), ValidationRule::Max(110.0)],
        })
        .with_argument(CommandArgument {
            name: "nose_radius_m".into(),
            arg_type: ArgumentType::Number,
            description: "Nose cap spherical radius in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1.15)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "vehicle_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Vehicle entry mass in kilograms".into(),
            required: false,
            default_value: Some(serde_json::json!(5200.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(100000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.manufacturing.lpbf_meltpool",
            "Laser Powder Bed Fusion Additive Manufacturing Melt Pool (King / Rosenthal)",
            "Evaluates moving Gaussian laser thermals, Volumetric Energy Density (VED), King normalized enthalpy, melt pool dimensions, cooling rate, and conduction vs keyhole defect regimes.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "laser_power_w".into(),
            arg_type: ArgumentType::Number,
            description: "Laser beam optical power in Watts".into(),
            required: true,
            default_value: Some(serde_json::json!(250.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "scan_speed_mm_s".into(),
            arg_type: ArgumentType::Number,
            description: "Laser scanning travel speed in mm/s".into(),
            required: true,
            default_value: Some(serde_json::json!(950.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "beam_diameter_um".into(),
            arg_type: ArgumentType::Number,
            description: "Laser beam spot diameter in micrometers".into(),
            required: false,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "layer_thickness_um".into(),
            arg_type: ArgumentType::Number,
            description: "Powder layer thickness in micrometers".into(),
            required: false,
            default_value: Some(serde_json::json!(40.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(200.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.acoustics.phononic_bandgap",
            "Acoustic Metamaterial & Phononic Crystal Bandgap Mechanics (Mead / Brillouin)",
            "Evaluates 1D periodic elastodynamic transfer matrix dispersion, Floquet-Bloch stopbands, Bragg bandgap frequency bounds, evanescent spatial attenuation, and transmission loss (TL) in dB.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "unit_cell_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of periodic unit cells in metamaterial stack".into(),
            required: true,
            default_value: Some(serde_json::json!(8)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "layer_a_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Thickness of Layer A in millimeters".into(),
            required: false,
            default_value: Some(serde_json::json!(24.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "layer_b_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Thickness of Layer B in millimeters".into(),
            required: false,
            default_value: Some(serde_json::json!(16.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "excitation_frequency_hz".into(),
            arg_type: ArgumentType::Number,
            description: "Operating excitation frequency in Hertz".into(),
            required: false,
            default_value: Some(serde_json::json!(24000.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(500000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.rail.wheel_rail_contact",
            "Wheel-Rail Contact Mechanics & Track Hunting (Kalker / Hertz / Nadal)",
            "Evaluates 3D Hertzian contact patch, peak normal pressure, Kalker linear longitudinal/lateral creep coefficients, Klingel hunting frequency, and Nadal derailment safety quotient.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "wheel_vertical_load_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Wheel normal vertical load in kN".into(),
            required: true,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "wheel_radius_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Wheel nominal rolling radius in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(460.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "rail_crown_radius_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Rail crown transverse radius in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(300.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "train_speed_km_h".into(),
            arg_type: ArgumentType::Number,
            description: "Train forward travel speed in km/h".into(),
            required: false,
            default_value: Some(serde_json::json!(200.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(600.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.mining.mine_ventilation",
            "Subsurface Mine Ventilation Aerodynamics & Fan Duty (Atkinson / MSHA)",
            "Evaluates Atkinson airway friction resistance, shock head losses, natural ventilation buoyancy (NVE), required fan total pressure, motor electrical power, and methane gas dilution compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "airway_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Airway segment length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(1400.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(20000.0)],
        })
        .with_argument(CommandArgument {
            name: "airway_area_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Airway cross-sectional area in m²".into(),
            required: true,
            default_value: Some(serde_json::json!(20.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "airway_perimeter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Airway wetted perimeter in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(18.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "airflow_rate_m3_s".into(),
            arg_type: ArgumentType::Number,
            description: "Required volumetric airflow rate in m³/s".into(),
            required: true,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(1000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.telecom.reflector_antenna",
            "Parabolic Reflector Antenna Electromagnetics & Gain (Silver / ITU-R S.580)",
            "Evaluates parabolic dish geometry, Ruze surface tolerance efficiency, aperture illumination and spillover efficiencies, bore-sight gain in dBi, beamwidths, and ITU-R S.580 envelope compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "dish_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Parabolic aperture diameter in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(2.40)),
            validation_rules: vec![ValidationRule::Min(0.20), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "focal_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Reflector focal length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(0.96)),
            validation_rules: vec![ValidationRule::Min(0.10), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "operating_frequency_ghz".into(),
            arg_type: ArgumentType::Number,
            description: "Microwave frequency in GigaHertz".into(),
            required: true,
            default_value: Some(serde_json::json!(14.25)),
            validation_rules: vec![ValidationRule::Min(0.10), ValidationRule::Max(300.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.semiconductor.gaa_fet",
            "Gate-All-Around (GAA) Nanosheet Transistor Electrostatics (Taur / BSIM-CMG)",
            "Evaluates 3D natural electrostatic scaling length, Drain-Induced Barrier Lowering (DIBL), subthreshold swing against thermal limit, threshold voltage, quasi-ballistic drive current, and Ion/Ioff ratio.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nanosheet_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of vertically stacked nanosheets".into(),
            required: true,
            default_value: Some(serde_json::json!(3)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "gate_length_nm".into(),
            arg_type: ArgumentType::Number,
            description: "Physical gate length Lg in nanometers".into(),
            required: true,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(3.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "nanosheet_width_nm".into(),
            arg_type: ArgumentType::Number,
            description: "Individual nanosheet width in nanometers".into(),
            required: true,
            default_value: Some(serde_json::json!(30.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "nanosheet_thickness_nm".into(),
            arg_type: ArgumentType::Number,
            description: "Individual nanosheet thickness in nanometers".into(),
            required: true,
            default_value: Some(serde_json::json!(5.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "equivalent_oxide_thickness_nm".into(),
            arg_type: ArgumentType::Number,
            description: "Equivalent oxide thickness EOT in nanometers".into(),
            required: false,
            default_value: Some(serde_json::json!(0.75)),
            validation_rules: vec![ValidationRule::Min(0.30), ValidationRule::Max(5.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.agri.irrigation_hydraulics",
            "Precision Agricultural Irrigation Lateral Hydraulics (Christiansen / ISO 9261)",
            "Evaluates multi-outlet friction reduction factors, head loss distribution, pressure and flow variation, Christiansen Uniformity Coefficient (CU), and Keller-Bliesner Emission Uniformity (EU).",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "pipe_inside_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Lateral pipe inside diameter in mm".into(),
            required: true,
            default_value: Some(serde_json::json!(16.0)),
            validation_rules: vec![ValidationRule::Min(4.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "lateral_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Total lateral length in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "emitter_spacing_m".into(),
            arg_type: ArgumentType::Number,
            description: "Distance between consecutive emitters in meters".into(),
            required: true,
            default_value: Some(serde_json::json!(0.40)),
            validation_rules: vec![ValidationRule::Min(0.10), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "nominal_discharge_l_h".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal emitter flow rate in L/h".into(),
            required: true,
            default_value: Some(serde_json::json!(1.60)),
            validation_rules: vec![ValidationRule::Min(0.20), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "inlet_pressure_head_m".into(),
            arg_type: ArgumentType::Number,
            description: "Lateral inlet pressure head in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(11.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.environmental.wastewater_treatment",
            "Activated Sludge Aeration & Secondary Clarifier Sedimentation (ASCE / WEF / Takacs)",
            "Evaluates biological basin volume, HRT, F/M ratio, daily biomass production, SOTR clean water oxygen demand, blower power, and clarifier SOR/SLR solids flux.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "influent_flow_m3_day".into(),
            arg_type: ArgumentType::Number,
            description: "Daily wastewater influent flow rate in m³/day".into(),
            required: false,
            default_value: Some(serde_json::json!(20000.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(1000000.0)],
        })
        .with_argument(CommandArgument {
            name: "influent_bod_mg_l".into(),
            arg_type: ArgumentType::Number,
            description: "Influent 5-day Biochemical Oxygen Demand in mg/L".into(),
            required: false,
            default_value: Some(serde_json::json!(220.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "target_effluent_bod_mg_l".into(),
            arg_type: ArgumentType::Number,
            description: "Target discharge effluent BOD5 in mg/L".into(),
            required: false,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "mlss_mg_l".into(),
            arg_type: ArgumentType::Number,
            description: "Aeration basin Mixed Liquor Suspended Solids in mg/L".into(),
            required: false,
            default_value: Some(serde_json::json!(3000.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(10000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.automotive.ev_powertrain",
            "EV Longitudinal Dynamics, Traction Envelope & Regenerative Braking (SAE J2452 / UNECE R13-H)",
            "Calculates aerodynamic and rolling road load forces, wheel and battery cruising power, driving range, 0-100 km/h acceleration, top speed, and regen energy recovery.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "vehicle_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Total vehicle mass in kilograms".into(),
            required: false,
            default_value: Some(serde_json::json!(1850.0)),
            validation_rules: vec![ValidationRule::Min(500.0), ValidationRule::Max(10000.0)],
        })
        .with_argument(CommandArgument {
            name: "motor_peak_torque_nm".into(),
            arg_type: ArgumentType::Number,
            description: "Traction electric motor peak torque in Nm".into(),
            required: false,
            default_value: Some(serde_json::json!(350.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "battery_capacity_kwh".into(),
            arg_type: ArgumentType::Number,
            description: "Traction battery usable energy capacity in kWh".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(300.0)],
        })
        .with_argument(CommandArgument {
            name: "cruising_speed_km_h".into(),
            arg_type: ArgumentType::Number,
            description: "Steady cruising evaluation speed in km/h".into(),
            required: false,
            default_value: Some(serde_json::json!(100.0)),
            validation_rules: vec![ValidationRule::Min(30.0), ValidationRule::Max(300.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.civil.seismic_base_isolation",
            "Lead-Rubber Bearing (LRB) Seismic Base Isolation Mechanics (ASCE 7-22 / FEMA P-751)",
            "Evaluates characteristic yield force, post-yield stiffness, effective period, equivalent viscous damping ratio, design displacement, and structural base shear attenuation.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "superstructure_mass_tonnes".into(),
            arg_type: ArgumentType::Number,
            description: "Superstructure seismic mass in metric tonnes".into(),
            required: false,
            default_value: Some(serde_json::json!(4500.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(100000.0)],
        })
        .with_argument(CommandArgument {
            name: "bearing_count".into(),
            arg_type: ArgumentType::Integer,
            description: "Total number of supporting LRBs".into(),
            required: false,
            default_value: Some(serde_json::json!(16)),
            validation_rules: vec![ValidationRule::Min(4.0), ValidationRule::Max(256.0)],
        })
        .with_argument(CommandArgument {
            name: "bearing_outer_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Bearing outer diameter in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(800.0)),
            validation_rules: vec![ValidationRule::Min(300.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "lead_core_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Lead plug core diameter in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(160.0)),
            validation_rules: vec![ValidationRule::Min(30.0), ValidationRule::Max(500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.defense.terminal_ballistics",
            "Terminal Ballistics & Armor Hydrodynamic Penetration (Tate-Alekseevskii / STANAG 4569)",
            "Solves modified Bernoulli hydrodynamic long-rod penetration, interface velocity, erosion rate, depth of penetration, ballistic limit V50, and NATO STANAG protection level.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "projectile_length_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Kinetic penetrator active core length in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(90.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "impact_velocity_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Projectile impact velocity in m/s".into(),
            required: false,
            default_value: Some(serde_json::json!(1450.0)),
            validation_rules: vec![ValidationRule::Min(300.0), ValidationRule::Max(3000.0)],
        })
        .with_argument(CommandArgument {
            name: "obliquity_angle_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Impact NATO obliquity angle from plate normal in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(30.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(80.0)],
        })
        .with_argument(CommandArgument {
            name: "armor_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Armor physical plate thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.packaging.solder_thermal_fatigue",
            "BGA Solder Joint Thermal Fatigue & Creep Lifetime (Engelmaier / IPC-9701)",
            "Evaluates DNP thermal shear strain, Engelmaier creep ductility exponent, Coffin-Manson low-cycle fatigue life, Weibull characteristic life, and IPC/AEC qualification compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "package_width_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Package substrate width in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(35.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(150.0)],
        })
        .with_argument(CommandArgument {
            name: "solder_joint_height_um".into(),
            arg_type: ArgumentType::Number,
            description: "Solder joint standoff height in micrometers".into(),
            required: false,
            default_value: Some(serde_json::json!(400.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "cycle_min_temp_c".into(),
            arg_type: ArgumentType::Number,
            description: "Thermal cycle minimum temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(-40.0)),
            validation_rules: vec![ValidationRule::Min(-65.0), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "cycle_max_temp_c".into(),
            arg_type: ArgumentType::Number,
            description: "Thermal cycle maximum temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(125.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(200.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.marine.ship_slamming",
            "Ship Bow & Bottom Hydrodynamic Slamming Impact (Wagner / Chuang / DNV-RU-SHIP)",
            "Evaluates localized peak Wagner slamming pressure, space-averaged pulse, total panel impact force, dynamic bending stress, and DNV-RU-SHIP minimum shell plating thickness.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "deadrise_angle_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Section deadrise angle beta in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(18.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(85.0)],
        })
        .with_argument(CommandArgument {
            name: "relative_impact_velocity_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Relative normal water-entry impact velocity in m/s".into(),
            required: false,
            default_value: Some(serde_json::json!(7.5)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(30.0)],
        })
        .with_argument(CommandArgument {
            name: "frame_spacing_m".into(),
            arg_type: ArgumentType::Number,
            description: "Transverse stiffener / frame spacing in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.75)),
            validation_rules: vec![ValidationRule::Min(0.2), ValidationRule::Max(2.5)],
        })
        .with_argument(CommandArgument {
            name: "plate_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Longitudinal primary girder / floor spacing in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(2.40)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(6.0)],
        })
        .with_argument(CommandArgument {
            name: "plate_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Existing plate thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(16.0)),
            validation_rules: vec![ValidationRule::Min(4.0), ValidationRule::Max(60.0)],
        })
        .with_argument(CommandArgument {
            name: "steel_yield_strength_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "Hull structural steel yield strength in MPa (e.g. 355 for NV-36)".into(),
            required: false,
            default_value: Some(serde_json::json!(355.0)),
            validation_rules: vec![ValidationRule::Min(150.0), ValidationRule::Max(800.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.automotive.crash_deceleration",
            "Vehicle Frontal Crash Deceleration & Occupant Severity (ECE R94 / FMVSS 208 / HIC15)",
            "Simulates dynamic structural crush distance, haversine deceleration pulse, seatbelt restraint tension, chest deflection, HIC15, and Prasad-Mertz AIS 4+ severe brain injury probability.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "vehicle_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Vehicle total test mass in kg".into(),
            required: false,
            default_value: Some(serde_json::json!(1650.0)),
            validation_rules: vec![ValidationRule::Min(400.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "impact_velocity_km_h".into(),
            arg_type: ArgumentType::Number,
            description: "Initial collision velocity in km/h (e.g. 64 km/h for Euro NCAP 40% offset)".into(),
            required: false,
            default_value: Some(serde_json::json!(64.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(140.0)],
        })
        .with_argument(CommandArgument {
            name: "crumple_zone_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Maximum available crumple crush space in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.72)),
            validation_rules: vec![ValidationRule::Min(0.2), ValidationRule::Max(2.0)],
        })
        .with_argument(CommandArgument {
            name: "crash_box_mean_force_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Front crash rail mean plastic collapse force in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(310.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "structural_energy_efficiency".into(),
            arg_type: ArgumentType::Number,
            description: "Structural energy absorption efficiency factor (0.50 - 0.90)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.75)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(0.95)],
        })
        .with_argument(CommandArgument {
            name: "occupant_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Occupant dummy mass in kg (50th percentile male ~ 78 kg)".into(),
            required: false,
            default_value: Some(serde_json::json!(78.0)),
            validation_rules: vec![ValidationRule::Min(30.0), ValidationRule::Max(150.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.nuclear.fuel_pin",
            "Nuclear Reactor Fuel Pin Radial Conduction & DNB Margin (Fourier / Dittus-Boelter / Tong W-3)",
            "Evaluates UO2 fuel centerline temperature, helium gap conductance, Zircaloy cladding profile, Dittus-Boelter convection, Tong W-3 Critical Heat Flux, and US NRC DNBR thermal safety margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "linear_heat_rate_kw_per_m".into(),
            arg_type: ArgumentType::Number,
            description: "Peak linear heat generation rate q' in kW/m (typ. 15.0 - 45.0)".into(),
            required: false,
            default_value: Some(serde_json::json!(32.0)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(60.0)],
        })
        .with_argument(CommandArgument {
            name: "pellet_radius_mm".into(),
            arg_type: ArgumentType::Number,
            description: "UO2 fuel pellet radius in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(4.10)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(8.0)],
        })
        .with_argument(CommandArgument {
            name: "gap_thickness_um".into(),
            arg_type: ArgumentType::Number,
            description: "Helium gas radial gap thickness in micrometers".into(),
            required: false,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(250.0)],
        })
        .with_argument(CommandArgument {
            name: "clad_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Zircaloy cladding wall thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(0.60)),
            validation_rules: vec![ValidationRule::Min(0.2), ValidationRule::Max(1.5)],
        })
        .with_argument(CommandArgument {
            name: "coolant_pressure_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "Reactor primary system pressure in MPa (typ. 15.5 for PWR)".into(),
            required: false,
            default_value: Some(serde_json::json!(15.50)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(22.0)],
        })
        .with_argument(CommandArgument {
            name: "bulk_coolant_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Bulk subcooled coolant temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(305.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(340.0)],
        })
        .with_argument(CommandArgument {
            name: "coolant_mass_flux_kg_m2_s".into(),
            arg_type: ArgumentType::Number,
            description: "Core coolant mass flux G in kg/(m²·s)".into(),
            required: false,
            default_value: Some(serde_json::json!(3500.0)),
            validation_rules: vec![ValidationRule::Min(800.0), ValidationRule::Max(6000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.chemical.adsorption_psa",
            "Pressure Swing Adsorption (PSA) Gas Separation & Breakthrough Dynamics (Langmuir / LDF / Thomas)",
            "Calculates Langmuir multicomponent adsorption equilibrium, Glueckauf LDF mass transfer, Thomas breakthrough curve, Mass Transfer Zone length, product purity, and gas recovery.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "bed_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Adsorption column bed inner diameter in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.60)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(4.0)],
        })
        .with_argument(CommandArgument {
            name: "bed_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Packed adsorbent bed length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(2.20)),
            validation_rules: vec![ValidationRule::Min(0.3), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "adsorption_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Adsorption cycle pressure in bar absolute".into(),
            required: false,
            default_value: Some(serde_json::json!(5.50)),
            validation_rules: vec![ValidationRule::Min(1.5), ValidationRule::Max(40.0)],
        })
        .with_argument(CommandArgument {
            name: "desorption_pressure_bar".into(),
            arg_type: ArgumentType::Number,
            description: "Desorption/regeneration pressure in bar absolute".into(),
            required: false,
            default_value: Some(serde_json::json!(1.10)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(5.0)],
        })
        .with_argument(CommandArgument {
            name: "feed_flow_rate_nm3_h".into(),
            arg_type: ArgumentType::Number,
            description: "Feed gas flow rate at STP in Nm³/h".into(),
            required: false,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "adsorbent_bulk_density_kg_m3".into(),
            arg_type: ArgumentType::Number,
            description: "Adsorbent packed bulk density in kg/m³".into(),
            required: false,
            default_value: Some(serde_json::json!(700.0)),
            validation_rules: vec![ValidationRule::Min(300.0), ValidationRule::Max(1500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.rotorcraft_aerodynamics",
            "Rotorcraft Helicopter Hover & Forward Flight Aerodynamics (Glauert / Leishman / Prouty)",
            "Computes blade element momentum inflow, hover induced & profile power, Figure of Merit, Glauert forward flight power bucket, tip Mach number, and stall margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "gross_weight_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Rotorcraft gross takeoff weight in kg".into(),
            required: false,
            default_value: Some(serde_json::json!(4300.0)),
            validation_rules: vec![ValidationRule::Min(300.0), ValidationRule::Max(30000.0)],
        })
        .with_argument(CommandArgument {
            name: "rotor_radius_m".into(),
            arg_type: ArgumentType::Number,
            description: "Main rotor radius R in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(6.50)),
            validation_rules: vec![ValidationRule::Min(1.5), ValidationRule::Max(16.0)],
        })
        .with_argument(CommandArgument {
            name: "number_of_blades".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of main rotor blades".into(),
            required: false,
            default_value: Some(serde_json::json!(4)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(8.0)],
        })
        .with_argument(CommandArgument {
            name: "blade_chord_m".into(),
            arg_type: ArgumentType::Number,
            description: "Mean blade chord in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.38)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(1.0)],
        })
        .with_argument(CommandArgument {
            name: "rotor_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Rotor rotational speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(345.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(800.0)],
        })
        .with_argument(CommandArgument {
            name: "forward_speed_knots".into(),
            arg_type: ArgumentType::Number,
            description: "Forward flight airspeed in knots (0.0 for hover)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(250.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.thermal.heat_exchanger_delaware",
            "Shell-and-Tube Heat Exchanger Rating & Pressure Drop (Bell-Delaware / TEMA)",
            "Evaluates shell-side crossflow velocity, Bell correction factors (Jc, Jl, Jb, Js), LMTD F-factor, overall U-value, required area overdesign, and shell pressure drop.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "shell_inside_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Shell inside diameter in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.60)),
            validation_rules: vec![ValidationRule::Min(0.15), ValidationRule::Max(2.5)],
        })
        .with_argument(CommandArgument {
            name: "number_of_tubes".into(),
            arg_type: ArgumentType::Integer,
            description: "Total tube count in bundle".into(),
            required: false,
            default_value: Some(serde_json::json!(360)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(10000.0)],
        })
        .with_argument(CommandArgument {
            name: "tube_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Tube effective length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(4.80)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(12.0)],
        })
        .with_argument(CommandArgument {
            name: "baffle_spacing_m".into(),
            arg_type: ArgumentType::Number,
            description: "Central baffle spacing in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.30)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(1.5)],
        })
        .with_argument(CommandArgument {
            name: "hot_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Hot fluid mass flow rate in kg/s".into(),
            required: false,
            default_value: Some(serde_json::json!(18.5)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(500.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.geotechnical.drilled_shaft",
            "Drilled Shaft & Bored Pile Axial Resistance & Settlement (FHWA GEC-010 / AASHTO LRFD)",
            "Calculates O'Neill-Reese alpha/beta side shear, base resistance, AASHTO LRFD factored geotechnical and structural capacities, and elastic service settlement.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "shaft_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Shaft diameter in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(1.20)),
            validation_rules: vec![ValidationRule::Min(0.40), ValidationRule::Max(4.0)],
        })
        .with_argument(CommandArgument {
            name: "embedded_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Total embedded shaft length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(18.0)),
            validation_rules: vec![ValidationRule::Min(3.0), ValidationRule::Max(70.0)],
        })
        .with_argument(CommandArgument {
            name: "factored_load_demand_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Factored design axial compressive load in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(3200.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "service_load_demand_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Service unfactored axial compressive load in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(2200.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(35000.0)],
        })
        .with_argument(CommandArgument {
            name: "undrained_shear_strength_kpa".into(),
            arg_type: ArgumentType::Number,
            description: "Undrained shear strength in kPa for cohesive soils".into(),
            required: false,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(600.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.biomedical.heart_valve",
            "Prosthetic Heart Valve Hemodynamics & Effective Orifice Area (ISO 5840 / Gorlin Formula)",
            "Computes Gorlin hydraulic Effective Orifice Area (EOA), indexed iEOA, Patient-Prosthesis Mismatch (PPM) severity, Bernoulli peak gradient, regurgitant fraction, and ISO 5840 safety compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "nominal_size_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Nominal tissue annulus size in mm (e.g. 23 mm)".into(),
            required: false,
            default_value: Some(serde_json::json!(23.0)),
            validation_rules: vec![ValidationRule::Min(17.0), ValidationRule::Max(33.0)],
        })
        .with_argument(CommandArgument {
            name: "heart_rate_bpm".into(),
            arg_type: ArgumentType::Number,
            description: "Heart rate in beats per minute".into(),
            required: false,
            default_value: Some(serde_json::json!(70.0)),
            validation_rules: vec![ValidationRule::Min(40.0), ValidationRule::Max(180.0)],
        })
        .with_argument(CommandArgument {
            name: "stroke_volume_ml".into(),
            arg_type: ArgumentType::Number,
            description: "Stroke volume in mL per beat".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(25.0), ValidationRule::Max(150.0)],
        })
        .with_argument(CommandArgument {
            name: "patient_bsa_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Patient body surface area BSA in m²".into(),
            required: false,
            default_value: Some(serde_json::json!(1.85)),
            validation_rules: vec![ValidationRule::Min(0.8), ValidationRule::Max(3.0)],
        })
        .with_argument(CommandArgument {
            name: "mean_pressure_gradient_mmhg".into(),
            arg_type: ArgumentType::Number,
            description: "Mean systolic transvalvular gradient in mmHg".into(),
            required: false,
            default_value: Some(serde_json::json!(10.5)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(40.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.centrifugal_compressor",
            "Centrifugal Compressor Stage Aerodynamics & Surge Margin (Stanitz / Eckardt / Dixon)",
            "Evaluates Stanitz slip factor, Euler work, total-to-total pressure ratio, Eckardt de-Haller diffusion, aerodynamic surge margin, and impeller disk centrifugal hoop stress.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "mass_flow_rate_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Air mass flow rate in kg/s".into(),
            required: false,
            default_value: Some(serde_json::json!(3.80)),
            validation_rules: vec![ValidationRule::Min(0.2), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "rotational_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Impeller shaft speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(38000.0)),
            validation_rules: vec![ValidationRule::Min(5000.0), ValidationRule::Max(120000.0)],
        })
        .with_argument(CommandArgument {
            name: "impeller_tip_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Impeller tip diameter D2 in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.285)),
            validation_rules: vec![ValidationRule::Min(0.08), ValidationRule::Max(1.0)],
        })
        .with_argument(CommandArgument {
            name: "number_of_blades".into(),
            arg_type: ArgumentType::Integer,
            description: "Number of full and splitter blades".into(),
            required: false,
            default_value: Some(serde_json::json!(18)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(32.0)],
        })
        .with_argument(CommandArgument {
            name: "blade_backsweep_deg".into(),
            arg_type: ArgumentType::Number,
            description: "Blade backsweep angle from radial in degrees".into(),
            required: false,
            default_value: Some(serde_json::json!(35.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(50.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.rail.pantograph",
            "High-Speed Rail Pantograph-Catenary Interaction (EN 50367 / EN 50119 / TSI)",
            "Calculates mean contact force Fm, contact force standard deviation, Doppler speed ratio alpha, arcing loss percentage NQ, and dynamic contact wire uplift.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "train_speed_km_h".into(),
            arg_type: ArgumentType::Number,
            description: "Train operating speed in km/h (e.g. 300 km/h)".into(),
            required: false,
            default_value: Some(serde_json::json!(300.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(450.0)],
        })
        .with_argument(CommandArgument {
            name: "static_contact_force_n".into(),
            arg_type: ArgumentType::Number,
            description: "Static contact force in Newtons".into(),
            required: false,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(40.0), ValidationRule::Max(200.0)],
        })
        .with_argument(CommandArgument {
            name: "contact_wire_tension_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Contact wire tension in kN".into(),
            required: false,
            default_value: Some(serde_json::json!(22.0)),
            validation_rules: vec![ValidationRule::Min(8.0), ValidationRule::Max(40.0)],
        })
        .with_argument(CommandArgument {
            name: "contact_wire_mass_kg_per_m".into(),
            arg_type: ArgumentType::Number,
            description: "Contact wire mass density in kg/m".into(),
            required: false,
            default_value: Some(serde_json::json!(1.07)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(2.5)],
        })
        .with_argument(CommandArgument {
            name: "catenary_span_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Catenary span length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(55.0)),
            validation_rules: vec![ValidationRule::Min(30.0), ValidationRule::Max(85.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.geotechnical.tbm_tunneling",
            "Tunnel Boring Machine (TBM) Earth Pressure Balance (EPB) Mechanics & Face Stability (ITA / Herzog / Eirich)",
            "Calculates tunnel face support pressure, cutterhead advance thrust, cutting and muck mixing torque, drive power, and specific excavation energy.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "tunnel_outer_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Tunnel outer diameter / cutterhead bore diameter in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(6.60)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(18.0)],
        })
        .with_argument(CommandArgument {
            name: "tunnel_depth_to_axis_m".into(),
            arg_type: ArgumentType::Number,
            description: "Depth from surface to tunnel axis in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(22.0)),
            validation_rules: vec![ValidationRule::Min(3.0), ValidationRule::Max(150.0)],
        })
        .with_argument(CommandArgument {
            name: "water_table_depth_m".into(),
            arg_type: ArgumentType::Number,
            description: "Depth to groundwater table in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(4.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "cutterhead_rotational_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Cutterhead rotational speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(2.5)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "penetration_rate_mm_per_rev".into(),
            arg_type: ArgumentType::Number,
            description: "Cutterhead penetration per revolution in mm/rev".into(),
            required: false,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(35.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.structural.prestressed_concrete",
            "Post-Tensioned Prestressed Concrete Girder Loss & Flexural Stress (ACI 318-19 / AASHTO LRFD / PTI)",
            "Calculates parabolic tendon friction and wobble losses, anchorage wedge seating draw-in, AASHTO refined time-dependent creep/shrinkage/relaxation losses, and extreme fiber service stresses.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "span_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Girder clear span length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(32.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(120.0)],
        })
        .with_argument(CommandArgument {
            name: "concrete_f_c_mpa".into(),
            arg_type: ArgumentType::Number,
            description: "28-day concrete compressive strength f'c in MPa".into(),
            required: false,
            default_value: Some(serde_json::json!(45.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(120.0)],
        })
        .with_argument(CommandArgument {
            name: "tendon_steel_area_mm2".into(),
            arg_type: ArgumentType::Number,
            description: "Total cross-sectional area of prestressing steel in mm²".into(),
            required: false,
            default_value: Some(serde_json::json!(2800.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(20000.0)],
        })
        .with_argument(CommandArgument {
            name: "midspan_eccentricity_m".into(),
            arg_type: ArgumentType::Number,
            description: "Tendon drape eccentricity at midspan in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.48)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(3.0)],
        })
        .with_argument(CommandArgument {
            name: "service_moment_midspan_kn_m".into(),
            arg_type: ArgumentType::Number,
            description: "Service load bending moment at midspan in kN·m".into(),
            required: false,
            default_value: Some(serde_json::json!(2150.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(50000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.spacecraft_jitter",
            "Spacecraft Reaction Wheel Micro-Vibration & Payload Pointing Jitter (NASA-HDBK-7005 / ECSS-E-HB-32-26A)",
            "Calculates reaction wheel static/dynamic unbalance forces/torques, bearing kinematic pass frequencies (BPFO/BPFI), structural modal amplification, and payload line-of-sight pointing jitter.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "wheel_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Reaction wheel spin speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(3600.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(15000.0)],
        })
        .with_argument(CommandArgument {
            name: "static_imbalance_g_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Rotor static mass unbalance in g·mm".into(),
            required: false,
            default_value: Some(serde_json::json!(1.80)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "dynamic_imbalance_g_mm2".into(),
            arg_type: ArgumentType::Number,
            description: "Rotor dynamic rocking unbalance in g·mm²".into(),
            required: false,
            default_value: Some(serde_json::json!(35.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "structural_resonance_freq_hz".into(),
            arg_type: ArgumentType::Number,
            description: "Spacecraft structural bus resonance frequency in Hz".into(),
            required: false,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "satellite_inertia_kg_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Spacecraft moment of inertia about optical axis in kg·m²".into(),
            required: false,
            default_value: Some(serde_json::json!(320.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(50000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.automotive.radar_lidar_sensor",
            "Autonomous Vehicle 77 GHz Radar & ToF LiDAR Perception in Adverse Weather (ITU-R P.838 / Kruse / ISO 26262)",
            "Calculates 77 GHz FMCW radar range equation with rain attenuation, ToF pulsed LiDAR optical link budget with Kruse atmospheric aerosol extinction, and sensor fusion detection margins.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "target_range_m".into(),
            arg_type: ArgumentType::Number,
            description: "Target distance in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(400.0)],
        })
        .with_argument(CommandArgument {
            name: "target_rcs_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Target Radar Cross Section in m²".into(),
            required: false,
            default_value: Some(serde_json::json!(5.0)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "target_optical_reflectivity".into(),
            arg_type: ArgumentType::Number,
            description: "Target optical Lambertian reflectivity (0.0 to 1.0)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.18)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(1.0)],
        })
        .with_argument(CommandArgument {
            name: "radar_tx_power_w".into(),
            arg_type: ArgumentType::Number,
            description: "Radar RF transmit power in Watts".into(),
            required: false,
            default_value: Some(serde_json::json!(0.10)),
            validation_rules: vec![ValidationRule::Min(0.001), ValidationRule::Max(5.0)],
        })
        .with_argument(CommandArgument {
            name: "lidar_pulse_power_w".into(),
            arg_type: ArgumentType::Number,
            description: "LiDAR laser pulse peak optical power in Watts".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(2000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.marine.auv_propulsion",
            "Autonomous Underwater Vehicle (AUV) Hydrodynamic Resistance & Mission Battery Endurance (ITTC-78 / Hoerner)",
            "Calculates ITTC-78 friction, Gertler-Myring streamlined hull form factor, total hydrodynamic drag, propeller thrust/QPC power, and battery mission endurance/range.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "cruising_speed_knots".into(),
            arg_type: ArgumentType::Number,
            description: "AUV cruising speed in knots".into(),
            required: false,
            default_value: Some(serde_json::json!(3.50)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(15.0)],
        })
        .with_argument(CommandArgument {
            name: "vehicle_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "AUV hull length overall in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(4.80)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(25.0)],
        })
        .with_argument(CommandArgument {
            name: "vehicle_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "AUV hull body outer diameter in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(0.533)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(3.0)],
        })
        .with_argument(CommandArgument {
            name: "battery_capacity_kwh".into(),
            arg_type: ArgumentType::Number,
            description: "Onboard Lithium-ion battery capacity in kWh".into(),
            required: false,
            default_value: Some(serde_json::json!(24.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "hotel_power_draw_w".into(),
            arg_type: ArgumentType::Number,
            description: "Electrical hotel load (compute, sensors, INS) in Watts".into(),
            required: false,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(5000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.hydro_governor",
            "Hydroelectric Turbine Governor, Penstock Water Hammer Surge & Wicket Gate Dynamics (Allievi / IEEE Std 1207)",
            "Calculates elastic water hammer wave speed, Allievi transient head rise, mechanical/water starting times, full load rejection runaway overspeed, and IEEE 1207 isolated governor tuning parameters.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "rated_net_head_m".into(),
            arg_type: ArgumentType::Number,
            description: "Rated net water head in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(145.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_discharge_m3_s".into(),
            arg_type: ArgumentType::Number,
            description: "Rated volumetric discharge flow rate in m³/s".into(),
            required: false,
            default_value: Some(serde_json::json!(24.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "rated_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Rated synchronous rotational speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(500.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(3600.0)],
        })
        .with_argument(CommandArgument {
            name: "penstock_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Penstock conduit total length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(420.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "wicket_gate_closure_time_s".into(),
            arg_type: ArgumentType::Number,
            description: "Full emergency wicket gate closure duration in seconds".into(),
            required: false,
            default_value: Some(serde_json::json!(6.50)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(60.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.semiconductor.cmp_planarization",
            "Semiconductor Chemical-Mechanical Planarization (CMP) Removal Rate, Dishing & Erosion (SEMI / Preston)",
            "Calculates Preston/Runnels material removal rate, Stribeck lubrication hydrodynamic regime, pad conditioning decay, and pattern-dependent dielectric dishing/erosion.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "wafer_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Wafer diameter in millimeters (e.g. 300 mm)".into(),
            required: false,
            default_value: Some(serde_json::json!(300.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(450.0)],
        })
        .with_argument(CommandArgument {
            name: "down_force_pressure_kpa".into(),
            arg_type: ArgumentType::Number,
            description: "Applied carrier down-force polishing pressure in kPa".into(),
            required: false,
            default_value: Some(serde_json::json!(21.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "relative_velocity_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Relative platen-to-wafer linear velocity in m/s".into(),
            required: false,
            default_value: Some(serde_json::json!(1.25)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "polish_time_seconds".into(),
            arg_type: ArgumentType::Number,
            description: "CMP polishing duration in seconds".into(),
            required: false,
            default_value: Some(serde_json::json!(60.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(600.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.transportation.pavement_mepdg",
            "Flexible Asphalt Pavement Structural Response, Fatigue & Rutting (AASHTO MEPDG / Asphalt Institute MS-1)",
            "Calculates multi-layer Odemark equivalent thickness, asphalt tensile bottom strain, subgrade compressive strain, and AI cumulative fatigue cracking and subgrade rutting life.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "asphalt_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Asphalt concrete surface layer thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(150.0)),
            validation_rules: vec![ValidationRule::Min(25.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "base_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Crushed aggregate base course thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(200.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(600.0)],
        })
        .with_argument(CommandArgument {
            name: "subbase_thickness_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Granular subbase course thickness in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(250.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(800.0)],
        })
        .with_argument(CommandArgument {
            name: "design_traffic_millions_esal".into(),
            arg_type: ArgumentType::Number,
            description: "Design service traffic in millions of 80 kN ESALs".into(),
            required: false,
            default_value: Some(serde_json::json!(8.50)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(200.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.biomedical.scaffold_degradation",
            "Biomaterial Tissue Engineering Scaffold Porosity, Permeability & Degradation (Darcy / Kozeny-Carman / Pitt / ISO 13781)",
            "Calculates cellular interconnected porosity, Darcy Kozeny-Carman nutrient fluid permeability, Gibson-Ashby porous modulus, and hydrolytic degradation kinetics.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "scaffold_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Cylindrical scaffold outer diameter in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(10.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "scaffold_height_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Cylindrical scaffold height in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "target_porosity_pct".into(),
            arg_type: ArgumentType::Number,
            description: "Target open interconnected cellular porosity percentage (%)".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(30.0), ValidationRule::Max(95.0)],
        })
        .with_argument(CommandArgument {
            name: "degradation_evaluation_weeks".into(),
            arg_type: ArgumentType::Number,
            description: "In vitro degradation duration in weeks".into(),
            required: false,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(156.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.satellite_orbital_decay",
            "Satellite Orbital Decay & Atmospheric Re-entry Lifetime (King-Hele / Jacchia-Roberts / NASA DAS / ISO 27852)",
            "Calculates solar flux thermospheric density, King-Hele semi-major axis altitude decay rate, total re-entry lifetime, and IADC 25-year / FCC 5-year space debris disposal compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "satellite_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Satellite wet mass at end of mission in kg".into(),
            required: false,
            default_value: Some(serde_json::json!(12.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "drag_cross_section_area_m2".into(),
            arg_type: ArgumentType::Number,
            description: "Effective ram aerodynamic cross-sectional drag area in m²".into(),
            required: false,
            default_value: Some(serde_json::json!(0.045)),
            validation_rules: vec![ValidationRule::Min(0.001), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "drag_coefficient_cd".into(),
            arg_type: ArgumentType::Number,
            description: "Free molecular aerodynamic drag coefficient C_D (standard ~2.2)".into(),
            required: false,
            default_value: Some(serde_json::json!(2.20)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(4.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_perigee_altitude_km".into(),
            arg_type: ArgumentType::Number,
            description: "Initial orbit perigee altitude in km".into(),
            required: false,
            default_value: Some(serde_json::json!(420.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "initial_apogee_altitude_km".into(),
            arg_type: ArgumentType::Number,
            description: "Initial orbit apogee altitude in km".into(),
            required: false,
            default_value: Some(serde_json::json!(440.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(2000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.mining.bucket_wheel_excavator",
            "Bucket Wheel Excavator (BWE) & Continuous Surface Mining Digging Mechanics (ISO 5048 / Krause)",
            "Calculates hourly theoretical excavation volume, cutting lip tangential digging force, wheel shaft drive torque/power, specific cutting energy, and ISO 5048 machine tipping stability.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "wheel_diameter_m".into(),
            arg_type: ArgumentType::Number,
            description: "Bucket wheel diameter in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(12.50)),
            validation_rules: vec![ValidationRule::Min(2.0), ValidationRule::Max(30.0)],
        })
        .with_argument(CommandArgument {
            name: "bucket_nominal_volume_m3".into(),
            arg_type: ArgumentType::Number,
            description: "Individual bucket nominal capacity in m³".into(),
            required: false,
            default_value: Some(serde_json::json!(1.80)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(10.0)],
        })
        .with_argument(CommandArgument {
            name: "wheel_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Bucket wheel rotational speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(4.80)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(20.0)],
        })
        .with_argument(CommandArgument {
            name: "lineal_cutting_resistance_kn_m".into(),
            arg_type: ArgumentType::Number,
            description: "Geological formation lineal digging resistance in kN/m".into(),
            required: false,
            default_value: Some(serde_json::json!(110.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(1000.0)],
        })
        .with_argument(CommandArgument {
            name: "boom_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Excavator bucket wheel boom outreach length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(42.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(150.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.process.multiphase_pipe_flow",
            "Two-Phase Gas-Liquid Hydrocarbon Pipe Flow Assurance (Lockhart-Martinelli / Beggs-Brill / API 14E)",
            "Calculates Lockhart-Martinelli parameter, Chisholm two-phase frictional pressure gradient, in-situ liquid holdup, flow regime, and API RP 14E erosional velocity thresholds.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "pipe_length_m".into(),
            arg_type: ArgumentType::Number,
            description: "Pipeline total length in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(8500.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(200000.0)],
        })
        .with_argument(CommandArgument {
            name: "pipe_inner_diameter_mm".into(),
            arg_type: ArgumentType::Number,
            description: "Pipe internal diameter in mm".into(),
            required: false,
            default_value: Some(serde_json::json!(254.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(1500.0)],
        })
        .with_argument(CommandArgument {
            name: "liquid_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Liquid oil/water mass flow rate in kg/s".into(),
            required: false,
            default_value: Some(serde_json::json!(35.0)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "gas_mass_flow_kg_s".into(),
            arg_type: ArgumentType::Number,
            description: "Gas vapor mass flow rate in kg/s".into(),
            required: false,
            default_value: Some(serde_json::json!(4.50)),
            validation_rules: vec![ValidationRule::Min(0.01), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "elevation_change_m".into(),
            arg_type: ArgumentType::Number,
            description: "Pipeline net elevation vertical rise in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(-2000.0), ValidationRule::Max(2000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.geothermal_reservoir",
            "Geothermal Reservoir Doublet Well Heat Extraction & Thermal Breakthrough (Gringarten / Bodvarsson)",
            "Calculates steady-state doublet hydrodynamic pressure drop, circulation pumping power, Gringarten thermal breakthrough arrival time, 30-year wellhead temperature decline, and thermal COP.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "reservoir_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Undisturbed in-situ reservoir formation temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(160.0)),
            validation_rules: vec![ValidationRule::Min(40.0), ValidationRule::Max(350.0)],
        })
        .with_argument(CommandArgument {
            name: "reinjection_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Cooled brine reinjection temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(65.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(120.0)],
        })
        .with_argument(CommandArgument {
            name: "brine_flow_rate_l_s".into(),
            arg_type: ArgumentType::Number,
            description: "Production and reinjection brine volumetric flow rate in L/s".into(),
            required: false,
            default_value: Some(serde_json::json!(75.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "well_spacing_distance_m".into(),
            arg_type: ArgumentType::Number,
            description: "Horizontal separation distance between doublet wells in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(1200.0)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "reservoir_permeability_md".into(),
            arg_type: ArgumentType::Number,
            description: "Reservoir formation intrinsic permeability in milliDarcys (mD)".into(),
            required: false,
            default_value: Some(serde_json::json!(450.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(10000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.process.spray_drying_kinetics",
            "Spray Drying Atomization, Droplet Evaporation & Powder Moisture (Masters / Ranz-Marshall / ASABE)",
            "Calculates rotary atomizer tip speed, Sauter Mean Diameter D32, D2-law droplet evaporation drying time, process air heater thermal duty, and Gordon-Taylor powder glass transition stickiness margin.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "feed_rate_kg_h".into(),
            arg_type: ArgumentType::Number,
            description: "Liquid slurry feed mass flow rate in kg/h".into(),
            required: false,
            default_value: Some(serde_json::json!(2500.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "feed_solids_fraction".into(),
            arg_type: ArgumentType::Number,
            description: "Feed slurry total solids mass fraction (e.g. 0.48 = 48 wt%)".into(),
            required: false,
            default_value: Some(serde_json::json!(0.48)),
            validation_rules: vec![ValidationRule::Min(0.05), ValidationRule::Max(0.75)],
        })
        .with_argument(CommandArgument {
            name: "atomizer_speed_rpm".into(),
            arg_type: ArgumentType::Number,
            description: "Rotary atomizer wheel rotational speed in RPM".into(),
            required: false,
            default_value: Some(serde_json::json!(18000.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "inlet_air_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Hot drying process air inlet temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(200.0)),
            validation_rules: vec![ValidationRule::Min(80.0), ValidationRule::Max(400.0)],
        })
        .with_argument(CommandArgument {
            name: "outlet_air_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Exhaust process air outlet temperature in °C".into(),
            required: false,
            default_value: Some(serde_json::json!(88.0)),
            validation_rules: vec![ValidationRule::Min(40.0), ValidationRule::Max(150.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.thermoelectric_rtg",
            "Radioisotope Thermoelectric Generator (RTG) & Solid-State Conversion (Ioffe / NASA SP-8000)",
            "Calculates Pu-238 alpha decay heat, thermoelectric figure of merit ZT, Seebeck open-circuit voltage, matched load electrical power, and 15-year deep space mission power degradation profile.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "initial_thermal_power_w_th".into(),
            arg_type: ArgumentType::Number,
            description: "Beginning-of-Life initial thermal heat source in Watts-thermal".into(),
            required: false,
            default_value: Some(serde_json::json!(4400.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(20000.0)],
        })
        .with_argument(CommandArgument {
            name: "hot_junction_temp_k".into(),
            arg_type: ArgumentType::Number,
            description: "Hot junction operating temperature in Kelvin".into(),
            required: false,
            default_value: Some(serde_json::json!(1273.15)),
            validation_rules: vec![ValidationRule::Min(300.0), ValidationRule::Max(1800.0)],
        })
        .with_argument(CommandArgument {
            name: "cold_junction_temp_k".into(),
            arg_type: ArgumentType::Number,
            description: "Cold junction radiator rejection temperature in Kelvin".into(),
            required: false,
            default_value: Some(serde_json::json!(566.15)),
            validation_rules: vec![ValidationRule::Min(100.0), ValidationRule::Max(800.0)],
        })
        .with_argument(CommandArgument {
            name: "couple_count".into(),
            arg_type: ArgumentType::Number,
            description: "Total number of series-connected unicouples".into(),
            required: false,
            default_value: Some(serde_json::json!(572.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(5000.0)],
        })
        .with_argument(CommandArgument {
            name: "mission_elapsed_years".into(),
            arg_type: ArgumentType::Number,
            description: "Elapsed operational deep-space mission time in years".into(),
            required: false,
            default_value: Some(serde_json::json!(15.0)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(100.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.nuclear.containment_loca",
            "Nuclear Reactor Containment Post-LOCA Pressure & Temperature Dynamics (10 CFR 50 App A / Tagami / Uchida)",
            "Calculates post-LOCA mass and energy discharge, Dalton air-steam partial pressures, Tagami and Uchida passive heat sink condensation coefficients, and active spray depressurization margins.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "containment_free_volume_m3".into(),
            arg_type: ArgumentType::Number,
            description: "Net internal free air volume of containment in cubic meters".into(),
            required: false,
            default_value: Some(serde_json::json!(65000.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(150000.0)],
        })
        .with_argument(CommandArgument {
            name: "design_pressure_bar_a".into(),
            arg_type: ArgumentType::Number,
            description: "Containment structural design internal pressure in bar absolute".into(),
            required: false,
            default_value: Some(serde_json::json!(4.80)),
            validation_rules: vec![ValidationRule::Min(1.5), ValidationRule::Max(12.0)],
        })
        .with_argument(CommandArgument {
            name: "blowdown_enthalpy_gj".into(),
            arg_type: ArgumentType::Number,
            description: "Total primary coolant blowdown enthalpy discharged in GigaJoules".into(),
            required: false,
            default_value: Some(serde_json::json!(380.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "spray_flow_rate_l_s".into(),
            arg_type: ArgumentType::Number,
            description: "Containment spray cooling active delivery rate in L/s".into(),
            required: false,
            default_value: Some(serde_json::json!(650.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(3000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.environmental.atmospheric_dispersion",
            "Atmospheric Pollutant Gaussian Plume Dispersion (EPA AERMOD / Pasquill-Gifford / Briggs)",
            "Calculates thermal buoyancy and momentum plume rise, Pasquill-Gifford dispersion coefficients, ground-level centerline concentrations, and US EPA NAAQS ambient limit compliance.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "emission_rate_g_s".into(),
            arg_type: ArgumentType::Number,
            description: "Pollutant mass emission rate in grams per second".into(),
            required: false,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "stack_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Physical chimney stack height in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(350.0)],
        })
        .with_argument(CommandArgument {
            name: "stack_gas_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Flue gas exit temperature in degrees Celsius".into(),
            required: false,
            default_value: Some(serde_json::json!(140.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(500.0)],
        })
        .with_argument(CommandArgument {
            name: "wind_speed_at_stack_height_m_s".into(),
            arg_type: ArgumentType::Number,
            description: "Mean wind speed at stack release height in m/s".into(),
            required: false,
            default_value: Some(serde_json::json!(5.50)),
            validation_rules: vec![ValidationRule::Min(0.5), ValidationRule::Max(40.0)],
        })
        .with_argument(CommandArgument {
            name: "receptor_downwind_distance_m".into(),
            arg_type: ArgumentType::Number,
            description: "Receptor evaluation downwind distance in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(2500.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(100000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.aerospace.rocket_ascent_trajectory",
            "Multistage Rocket Ascent Trajectory & Orbital Injection (Tsiolkovsky / NASA SP-8010)",
            "Calculates multistage Tsiolkovsky staging delta-V, gravity/aerodynamic/steering losses, Max Q dynamic pressure, Earth rotation assist, and circular LEO injection velocity.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "gross_liftoff_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Total launch vehicle gross liftoff mass (GLOW) in kg".into(),
            required: false,
            default_value: Some(serde_json::json!(549054.0)),
            validation_rules: vec![ValidationRule::Min(1000.0), ValidationRule::Max(4000000.0)],
        })
        .with_argument(CommandArgument {
            name: "payload_mass_kg".into(),
            arg_type: ArgumentType::Number,
            description: "Orbital payload mass in kilograms".into(),
            required: false,
            default_value: Some(serde_json::json!(15600.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(150000.0)],
        })
        .with_argument(CommandArgument {
            name: "stage1_thrust_sl_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Stage 1 sea-level total thrust in kiloNewtons".into(),
            required: false,
            default_value: Some(serde_json::json!(7607.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(50000.0)],
        })
        .with_argument(CommandArgument {
            name: "stage2_thrust_vac_kn".into(),
            arg_type: ArgumentType::Number,
            description: "Stage 2 vacuum thrust in kiloNewtons".into(),
            required: false,
            default_value: Some(serde_json::json!(981.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(10000.0)],
        })
        .with_argument(CommandArgument {
            name: "target_orbit_altitude_km".into(),
            arg_type: ArgumentType::Number,
            description: "Target circular LEO injection altitude in kilometers".into(),
            required: false,
            default_value: Some(serde_json::json!(300.0)),
            validation_rules: vec![ValidationRule::Min(150.0), ValidationRule::Max(2000.0)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.civil.gravity_dam_stability",
            "Concrete Gravity Dam Overturning, Sliding & Seepage Uplift Stability (USACE EM 1110-2-2200 / FERC)",
            "Calculates hydrostatic water thrust, USACE trapezoidal foundation drainage seepage uplift, Westergaard seismic hydrodynamic forces, shear-friction sliding SFF, and middle-third Kern bearing stresses.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "dam_height_m".into(),
            arg_type: ArgumentType::Number,
            description: "Concrete gravity dam structural height in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(85.0)),
            validation_rules: vec![ValidationRule::Min(10.0), ValidationRule::Max(300.0)],
        })
        .with_argument(CommandArgument {
            name: "crest_width_m".into(),
            arg_type: ArgumentType::Number,
            description: "Dam crest roadway width in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(8.50)),
            validation_rules: vec![ValidationRule::Min(3.0), ValidationRule::Max(40.0)],
        })
        .with_argument(CommandArgument {
            name: "base_width_m".into(),
            arg_type: ArgumentType::Number,
            description: "Dam foundation base footprint width in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(68.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(250.0)],
        })
        .with_argument(CommandArgument {
            name: "upstream_water_depth_m".into(),
            arg_type: ArgumentType::Number,
            description: "Upstream reservoir operating water depth in meters".into(),
            required: false,
            default_value: Some(serde_json::json!(80.0)),
            validation_rules: vec![ValidationRule::Min(5.0), ValidationRule::Max(300.0)],
        })
        .with_argument(CommandArgument {
            name: "horizontal_seismic_coef_kh".into(),
            arg_type: ArgumentType::Number,
            description: "Pseudostatic horizontal peak ground acceleration coefficient k_h in g's".into(),
            required: false,
            default_value: Some(serde_json::json!(0.05)),
            validation_rules: vec![ValidationRule::Min(0.0), ValidationRule::Max(0.50)],
        }),
    )?;

    registry.register(
        CommandSchema::new(
            "sim.energy.electrolyzer_hydrogen",
            "Water Electrolyzer Hydrogen Production & Cell Polarization (Faraday / Butler-Volmer / ISO 22734)",
            "Calculates Nernst reversible potential, thermoneutral voltage, anode/cathode Butler-Volmer activation overpotentials, membrane protonic ohmic losses, Faraday efficiency, and specific energy consumption.",
            CommandCategory::Simulation,
        )
        .with_permission(CommandPermission::SimulationRun)
        .with_argument(CommandArgument {
            name: "cell_temperature_c".into(),
            arg_type: ArgumentType::Number,
            description: "Cell operating temperature in degrees Celsius".into(),
            required: false,
            default_value: Some(serde_json::json!(70.0)),
            validation_rules: vec![ValidationRule::Min(20.0), ValidationRule::Max(100.0)],
        })
        .with_argument(CommandArgument {
            name: "hydrogen_operating_pressure_bar_a".into(),
            arg_type: ArgumentType::Number,
            description: "Hydrogen delivery operating pressure in bar absolute".into(),
            required: false,
            default_value: Some(serde_json::json!(30.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(50.0)],
        })
        .with_argument(CommandArgument {
            name: "current_density_a_per_cm2".into(),
            arg_type: ArgumentType::Number,
            description: "Operating current density in A/cm²".into(),
            required: false,
            default_value: Some(serde_json::json!(1.80)),
            validation_rules: vec![ValidationRule::Min(0.1), ValidationRule::Max(4.0)],
        })
        .with_argument(CommandArgument {
            name: "active_cell_area_cm2".into(),
            arg_type: ArgumentType::Number,
            description: "Active cell geometric membrane area in cm²".into(),
            required: false,
            default_value: Some(serde_json::json!(600.0)),
            validation_rules: vec![ValidationRule::Min(50.0), ValidationRule::Max(2000.0)],
        })
        .with_argument(CommandArgument {
            name: "cell_count".into(),
            arg_type: ArgumentType::Number,
            description: "Number of series-connected cells in the stack".into(),
            required: false,
            default_value: Some(serde_json::json!(120.0)),
            validation_rules: vec![ValidationRule::Min(1.0), ValidationRule::Max(500.0)],
        }),
    )?;

    Ok(())
}

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

    Ok(())
}

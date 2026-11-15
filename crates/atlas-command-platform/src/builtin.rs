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

    Ok(())
}

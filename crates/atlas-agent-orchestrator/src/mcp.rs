//! Model Context Protocol (MCP) server implementation for AtlasCAD.
//!
//! Exposes typed tools and resources over standard JSON-RPC 2.0.

use crate::error::OrchestratorError;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpRequest {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpResponse {
    pub jsonrpc: String,
    pub id: serde_json::Value,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<McpErrorPayload>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct McpErrorPayload {
    pub code: i64,
    pub message: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct McpToolDefinition {
    pub name: String,
    pub description: String,
    pub input_schema: serde_json::Value,
}

pub struct McpServer;

impl McpServer {
    #[must_use]
    pub fn get_tools() -> Vec<McpToolDefinition> {
        vec![
            McpToolDefinition {
                name: "atlas_project_create".into(),
                description: "Create a new AtlasCAD engineering project with specified unit system".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "project_name": { "type": "string", "description": "Unique name of the project" },
                        "unit_system": { "type": "string", "enum": ["si", "imperial"], "default": "si" }
                    },
                    "required": ["project_name"]
                }),
            },
            McpToolDefinition {
                name: "atlas_project_inspect".into(),
                description: "Inspect project topology, active feature tree, and revision metadata".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_cad_create_sketch".into(),
                description: "Create a new 2D sketch profile on a specified datum plane".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "plane": { "type": "string", "description": "Datum plane (xy, xz, yz)" },
                        "profile_kind": { "type": "string", "enum": ["rectangle", "circle", "regular_polygon"] },
                        "origin_z": { "type": "number", "default": 0.0 }
                    },
                    "required": ["plane", "profile_kind"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cad_create_solid".into(),
                description: "Extrude a sketch profile into a 3D B-Rep solid feature".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sketch_id": { "type": "string" },
                        "depth_mm": { "type": "number", "minimum": 0.001 },
                        "operation": { "type": "string", "enum": ["join", "cut", "new_body"] }
                    },
                    "required": ["sketch_id", "depth_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_material_assign".into(),
                description: "Assign certified engineering material properties to active part".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "material_id": { "type": "string", "description": "e.g. steel.1018, aluminum.6061-t6, titanium.ti-6al-4v" }
                    },
                    "required": ["material_id"]
                }),
            },
            McpToolDefinition {
                name: "atlas_mesh_generate".into(),
                description: "Generate tetrahedral/hexahedral finite element discretization mesh".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "voxel_size_mm": { "type": "number", "minimum": 0.1, "default": 2.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_simulation_run".into(),
                description: "Execute a multi-disciplinary simulation study (fea, modal, thermal, cfd, aero, suite)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "study_type": { "type": "string", "enum": ["fea", "modal", "thermal", "cfd", "aerodynamics", "suite"] },
                        "parameters": { "type": "object" }
                    },
                    "required": ["study_type"]
                }),
            },
            McpToolDefinition {
                name: "atlas_design_optimize".into(),
                description: "Execute SIMP topology optimization for weight reduction under compliance constraints".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "volume_fraction": { "type": "number", "minimum": 0.1, "maximum": 0.9, "default": 0.35 },
                        "iterations": { "type": "integer", "default": 12 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_additive_slice".into(),
                description: "Slice 3D solid and generate cellular infills (Gyroid, Honeycomb) for 3D printing".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "layer_height_mm": { "type": "number", "default": 0.2 },
                        "infill_pattern": { "type": "string", "enum": ["rectilinear", "grid", "honeycomb", "gyroid"], "default": "gyroid" }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_cnc_prepare".into(),
                description: "Generate 3-axis, 4-axis, or 5-axis CNC machining toolpaths and ISO G-code".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operation": { "type": "string", "enum": ["facing", "contour", "pocket", "rotary4", "multiaxis5"] },
                        "tool_diameter_mm": { "type": "number", "default": 6.0 }
                    },
                    "required": ["operation"]
                }),
            },
            McpToolDefinition {
                name: "atlas_drawing_generate".into(),
                description: "Generate 2D engineering drawings in DXF or PDF plot sheet".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "format": { "type": "string", "enum": ["dxf", "pdf"], "default": "dxf" }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_report_generate".into(),
                description: "Generate certified AS9100 / ISO 9001 compliance audit dossier".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_workflow_status".into(),
                description: "Query status of active multi-agent engineering workflows and tasks".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_solve".into(),
                description: "Simulate 12,000 km/h (Mach 11) aircraft across aerothermodynamics, Scramjet propulsion, UHTC thermal stress, and 270V avionics".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_speed_km_h": { "type": "number", "default": 12000.0 },
                        "cruise_altitude_m": { "type": "number", "default": 32000.0 },
                        "angle_of_attack_deg": { "type": "number", "default": 2.5 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_export_davinci".into(),
                description: "Export FCPXML timeline, CMX 3600 EDL, 3D aerothermal LUT, and Pixar OpenUSD scene for DaVinci Resolve Studio".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "timebase_fps": { "type": "number", "default": 25.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_export_unity".into(),
                description: "Export Unity Engine Package (com.atlascad.unity), glTF 2.0 PBR asset, and 6-DOF telemetry flight controllers".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "include_hud": { "type": "boolean", "default": true }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_export_step".into(),
                description: "Export active 3D model as standardized ISO 10303-21 STEP Part 21 (AP203/AP214) exchange file".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_export_threemf".into(),
                description: "Export active 3D model as ASTM/ISO 52915 3MF additive manufacturing XML package".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_deploy_status".into(),
                description: "Query system deployment status, container orchestrator state, memory usage, and multi-physics engine health".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_create_subdivide".into(),
                description: "Atlas Create: Perform Catmull-Clark iterative subdivision surface smoothing on polygon mesh".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mesh_id": { "type": "string" },
                        "subdiv_levels": { "type": "integer", "default": 1 }
                    },
                    "required": ["mesh_id"]
                }),
            },
            McpToolDefinition {
                name: "atlas_create_sculpt".into(),
                description: "Atlas Create: Apply 3D sculpting brush stroke (Draw, Clay, Smooth, Flatten, Inflate) to mesh".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "brush": { "type": "string", "enum": ["draw", "clay", "smooth", "flatten", "inflate", "grab"] },
                        "radius": { "type": "number", "default": 0.5 },
                        "strength": { "type": "number", "default": 0.5 }
                    },
                    "required": ["brush"]
                }),
            },
            McpToolDefinition {
                name: "atlas_realtime_scene_spawn".into(),
                description: "Atlas RealTime: Spawn entity in ECS scene graph with transform, physics, and telemetry bindings".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "name": { "type": "string" },
                        "mesh_id": { "type": "string" },
                        "position": { "type": "array", "items": { "type": "number" }, "minItems": 3, "maxItems": 3 }
                    },
                    "required": ["name"]
                }),
            },
            McpToolDefinition {
                name: "atlas_media_timeline_create".into(),
                description: "Atlas Media: Create multi-track NLE video/audio timeline with SMPTE timecode and export OTIO".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "title": { "type": "string" },
                        "fps": { "type": "integer", "default": 24 }
                    },
                    "required": ["title"]
                }),
            },
            McpToolDefinition {
                name: "atlas_media_color_grade".into(),
                description: "Atlas Media: Apply 3-way color wheels (Lift, Gamma, Gain, Offset) and 3D LUT .cube".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "lift": { "type": "array", "items": { "type": "number" } },
                        "gamma": { "type": "array", "items": { "type": "number" } },
                        "gain": { "type": "array", "items": { "type": "number" } },
                        "saturation": { "type": "number", "default": 1.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_plasma".into(),
                description: "Simulate nonequilibrium plasma sheath, electron density, RF blackout across L/X/Ka bands, and MHD magnetic window".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "flight_mach": { "type": "number", "default": 11.0 },
                        "altitude_m": { "type": "number", "default": 32000.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_thermal".into(),
                description: "Simulate active regenerative cooling microchannels, supercritical H2 heat transfer, and wall temperature gradients".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "heat_flux_mw_m2": { "type": "number", "default": 1.45 },
                        "coolant_flow_kg_s": { "type": "number", "default": 4.93 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_scramjet".into(),
                description: "Simulate 3-ramp oblique shock wave compression, isolator unstart margin (Waltrup-Billig), and supersonic Rayleigh combustion".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "flight_mach": { "type": "number", "default": 11.0 },
                        "equivalence_ratio": { "type": "number", "default": 1.05 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_cryo".into(),
                description: "Simulate cryogenic LH2 storage tanks, 60-layer MLI boil-off rates, and turbopump inducer cavitation safety margins".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "fuel_mass_kg": { "type": "number", "default": 16300.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_bom".into(),
                description: "Query complete 15-subsystem real-world component Bill of Materials, masses, CG coordinates, and thermal ratings".into(),
                input_schema: serde_json::json!({ "type": "object" }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_mhd".into(),
                description: "Simulate Magnetohydrodynamic (MHD) energy bypass, boundary layer power extraction, and nozzle thrust boost".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "flow_velocity_m_s": { "type": "number", "default": 3333.3 },
                        "b_field_tesla": { "type": "number", "default": 0.80 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_slush".into(),
                description: "Simulate Slush Hydrogen (13.8 K) 50% solid fraction fuel density, volume savings, and zero-boiloff hold time".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "fuel_mass_kg": { "type": "number", "default": 16300.0 },
                        "solid_fraction": { "type": "number", "default": 0.50 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_creep".into(),
                description: "Simulate UHTC viscoplastic creep strain rate and predict Larson-Miller 1000-hour mission life".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operating_temp_k": { "type": "number", "default": 2545.0 },
                        "applied_stress_mpa": { "type": "number", "default": 45.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_flutter".into(),
                description: "Simulate hypersonic aeroelastic panel flutter (Dowell third-order piston theory) and safety factor".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dynamic_pressure_kpa": { "type": "number", "default": 73.5 },
                        "mach": { "type": "number", "default": 11.0 },
                        "wall_temp_c": { "type": "number", "default": 2272.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_6dof".into(),
                description: "Simulate 6-DOF Bank-to-Turn hypersonic maneuvering dynamics, turn radius, and g-load envelope".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_heading_change_deg": { "type": "number", "default": 30.0 },
                        "bank_angle_deg": { "type": "number", "default": 55.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_hypersonic_magnet".into(),
                description: "Simulate 20 K REBCO High-Temperature Superconducting (HTS) magnet for MHD channel & RF window".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_b_field_tesla": { "type": "number", "default": 1.25 },
                        "channel_gap_m": { "type": "number", "default": 0.35 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_civil_buckling".into(),
                description: "Simulate Euler-Johnson column buckling, critical axial loads, and AISC 360 slenderness safety factor".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "length_m": { "type": "number", "default": 4.5 },
                        "axial_load_n": { "type": "number", "default": 1500000.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_civil_earth_pressure".into(),
                description: "Calculate Rankine and Coulomb lateral earth pressures, wall active thrust, and overturning safety".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wall_height_m": { "type": "number", "default": 4.0 },
                        "soil_friction_angle_deg": { "type": "number", "default": 32.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_power_flow".into(),
                description: "Simulate AC transmission power grid flow using Newton-Raphson iteration on bus admittance matrix".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "base_mva": { "type": "number", "default": 100.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_chemical_vle".into(),
                description: "Calculate multi-component vapor-liquid equilibrium (VLE) flash split using Rachford-Rice equation".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "temperature_c": { "type": "number", "default": 25.0 },
                        "pressure_kpa": { "type": "number", "default": 101.325 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_chemical_acid_base".into(),
                description: "Simulate rigorous analytical acid-base equilibria, Henderson-Hasselbalch buffer capacities, and pH titration curves".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "analyte_id": { "type": "string", "default": "acetic_acid" },
                        "analyte_concentration_m": { "type": "number", "default": 0.10 },
                        "analyte_volume_ml": { "type": "number", "default": 25.0 },
                        "titrant_id": { "type": "string", "default": "naoh" },
                        "titrant_concentration_m": { "type": "number", "default": 0.10 },
                        "temperature_c": { "type": "number", "default": 25.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_chemical_species_catalog".into(),
                description: "Retrieve comprehensive thermodynamic catalog of industrial strong/weak acids, strong/weak bases, and buffer salts".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {}
                }),
            },
            McpToolDefinition {
                name: "atlas_robotics_kinematics".into(),
                description: "Simulate 6-DOF manipulator forward kinematics, DLS inverse kinematics, and Yoshikawa manipulability".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_x_m": { "type": "number", "default": 0.8 },
                        "target_y_m": { "type": "number", "default": 0.4 },
                        "target_z_m": { "type": "number", "default": 0.5 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_env_plume".into(),
                description: "Simulate Gaussian air pollutant dispersion plume under Pasquill-Gifford atmospheric stability classes".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "emission_rate_g_s": { "type": "number", "default": 50.0 },
                        "wind_speed_m_s": { "type": "number", "default": 4.5 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_marine_hydrostatics".into(),
                description: "Simulate marine vessel displacement, metacentric height GM, righting arm, and Airy surface wave kinematics".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "displacement_tonnes": { "type": "number", "default": 1000.0 },
                        "draft_m": { "type": "number", "default": 4.2 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_acoustics_stc".into(),
                description: "Calculate acoustic transmission loss across 1/3 octave bands and ASTM E413 Sound Transmission Class".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "surface_density_kg_m2": { "type": "number", "default": 24.0 }
                    }
                }),
            },
            McpToolDefinition {
                name: "atlas_cad_sheet_metal_unfold".into(),
                description: "Calculate neutral axis K-factor, bend allowance (BA), outside setback, bend deduction, flat blank dimensions, and forming force".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "material": { "type": "string", "enum": ["carbon_steel", "stainless_steel", "aluminum_5052", "copper", "brass"], "default": "carbon_steel" },
                        "thickness_mm": { "type": "number", "minimum": 0.1, "default": 2.0 },
                        "flange_lengths_mm": { "type": "array", "items": { "type": "number" }, "default": [50.0, 50.0] },
                        "bend_angles_deg": { "type": "array", "items": { "type": "number" }, "default": [90.0] },
                        "inner_radii_mm": { "type": "array", "items": { "type": "number" }, "default": [2.0] },
                        "die_opening_width_mm": { "type": "number", "default": 16.0 }
                    },
                    "required": ["thickness_mm", "flange_lengths_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cad_hardware_fastener".into(),
                description: "Evaluate ISO metric bolt sizing, tensile stress area, proof load, tightening torque, and joint safety factors".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "thread_size": { "type": "string", "default": "M10" },
                        "bolt_class": { "type": "string", "enum": ["4.8", "8.8", "10.9", "12.9", "A2-70"], "default": "8.8" },
                        "clamped_length_mm": { "type": "number", "default": 40.0 },
                        "applied_tension_load_kn": { "type": "number", "default": 12.0 },
                        "applied_shear_load_kn": { "type": "number", "default": 5.0 },
                        "torque_coefficient_k": { "type": "number", "default": 0.18 }
                    },
                    "required": ["thread_size"]
                }),
            },
            McpToolDefinition {
                name: "atlas_chemical_kinetics".into(),
                description: "Solve chemical reaction kinetics, half-life, Damköhler number, and continuous CSTR/PFR reactor sizing".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "reaction_name": { "type": "string", "default": "A -> Products" },
                        "order": { "type": "string", "enum": ["first_order", "second_order", "reversible_first_order"], "default": "first_order" },
                        "initial_concentration_m": { "type": "number", "default": 1.0 },
                        "rate_constant_k": { "type": "number", "default": 0.05 },
                        "temperature_k": { "type": "number", "default": 298.15 },
                        "volumetric_flow_m3_s": { "type": "number", "default": 0.005 },
                        "target_conversion": { "type": "number", "default": 0.80 }
                    },
                    "required": ["initial_concentration_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_additive_process_profile".into(),
                description: "Evaluate additive manufacturing physics: Jacobs working curve (SLA) or Volumetric Energy Density (SLM)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "technology": { "type": "string", "enum": ["fdm_fff", "sla_dlp_msla", "slm_metal_pbf", "sls_polymer"], "default": "sla_dlp_msla" },
                        "part_height_mm": { "type": "number", "default": 80.0 },
                        "part_volume_cm3": { "type": "number", "default": 45.0 },
                        "layer_height_mm": { "type": "number", "default": 0.050 }
                    },
                    "required": ["technology", "part_height_mm", "part_volume_cm3", "layer_height_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_acoustics_outdoor_noise".into(),
                description: "ISO 9613-2 outdoor environmental sound propagation with geometrical divergence, atmospheric absorption, ground effect, and barrier diffraction".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "source_name": { "type": "string", "default": "Industrial Rooftop Chiller" },
                        "sound_power_lwa": { "type": "number", "default": 102.0 },
                        "distance_m": { "type": "number", "default": 120.0 },
                        "source_height_m": { "type": "number", "default": 2.5 },
                        "receiver_height_m": { "type": "number", "default": 1.5 },
                        "barrier_height_m": { "type": "number", "default": 3.5 },
                        "barrier_distance_m": { "type": "number", "default": 15.0 },
                        "ground_factor": { "type": "number", "default": 0.70 },
                        "regulatory_limit_dba": { "type": "number", "default": 50.0 }
                    },
                    "required": ["sound_power_lwa", "distance_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cad_hardware_spring".into(),
                description: "Calculate DIN EN 13906-1 helical compression spring rate, Wahl stress factor, solid length, and surge natural frequency".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wire_diameter_d_mm": { "type": "number", "default": 4.0 },
                        "mean_coil_diameter_d_mm": { "type": "number", "default": 32.0 },
                        "free_length_l0_mm": { "type": "number", "default": 85.0 },
                        "active_coils_na": { "type": "number", "default": 7.0 },
                        "operating_force_f2_n": { "type": "number", "default": 650.0 },
                        "material": { "type": "string", "default": "MusicWire" },
                        "end_type": { "type": "string", "default": "SquaredAndGround" }
                    },
                    "required": ["wire_diameter_d_mm", "mean_coil_diameter_d_mm", "free_length_l0_mm", "active_coils_na"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cad_hardware_gear_pair".into(),
                description: "Design and rate ISO 6336 / AGMA 2001-D04 spur and helical involute gear pair with contact ratio, bending stress, and pitting safety factor".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "gear_type": { "type": "string", "enum": ["Spur", "Helical"], "default": "Helical" },
                        "normal_module_mn": { "type": "number", "default": 3.5 },
                        "pinion_teeth_z1": { "type": "integer", "default": 19 },
                        "gear_teeth_z2": { "type": "integer", "default": 57 },
                        "face_width_b_mm": { "type": "number", "default": 40.0 },
                        "helix_angle_beta_deg": { "type": "number", "default": 20.0 },
                        "pinion_speed_rpm": { "type": "number", "default": 3000.0 },
                        "transmitted_power_kw": { "type": "number", "default": 35.0 }
                    },
                    "required": ["normal_module_mn", "pinion_teeth_z1", "gear_teeth_z2", "face_width_b_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cam_cutting_force".into(),
                description: "Calculate DIN 6584 Kienzle specific cutting force, cutting force Fc, spindle power Pm, MRR, and theoretical surface finish Ra".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operation": { "type": "string", "enum": ["Turning", "EndMilling", "FaceMilling", "Drilling"], "default": "FaceMilling" },
                        "workpiece": { "type": "string", "default": "AlloySteel4140" },
                        "cutting_speed_vc_m_per_min": { "type": "number", "default": 180.0 },
                        "feed_f_mm": { "type": "number", "default": 0.15 },
                        "depth_of_cut_ap_mm": { "type": "number", "default": 3.0 },
                        "radial_width_ae_mm": { "type": "number", "default": 40.0 },
                        "tool_diameter_dc_mm": { "type": "number", "default": 50.0 },
                        "number_of_teeth_z": { "type": "integer", "default": 4 }
                    },
                    "required": ["cutting_speed_vc_m_per_min", "feed_f_mm", "depth_of_cut_ap_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_electrical_short_circuit".into(),
                description: "IEC 60909 symmetrical 3-phase short-circuit current Ik'', peak dynamic current ip, breaking MVA, and cable run voltage drop".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_voltage_un_v": { "type": "number", "default": 400.0 },
                        "transformer_rated_power_sn_kva": { "type": "number", "default": 1000.0 },
                        "transformer_impedance_voltage_uk_percent": { "type": "number", "default": 6.0 },
                        "feeder_length_l_m": { "type": "number", "default": 65.0 },
                        "conductor_cross_section_q_mm2": { "type": "number", "default": 150.0 },
                        "conductor_material": { "type": "string", "enum": ["Copper", "Aluminum"], "default": "Copper" },
                        "operating_current_ib_a": { "type": "number", "default": 220.0 }
                    },
                    "required": ["nominal_voltage_un_v", "transformer_rated_power_sn_kva", "feeder_length_l_m", "conductor_cross_section_q_mm2"]
                }),
            },
            McpToolDefinition {
                name: "atlas_building_daylight_factor".into(),
                description: "CIE overcast sky daylight factor (Sky Component SC, ERC, IRC), working desk DF%, and room average ADF% (EN 17037)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sky_type": { "type": "string", "default": "CieOvercast" },
                        "room_length_m": { "type": "number", "default": 6.0 },
                        "room_width_m": { "type": "number", "default": 5.0 },
                        "room_height_m": { "type": "number", "default": 3.0 },
                        "window_width_m": { "type": "number", "default": 3.2 },
                        "window_height_m": { "type": "number", "default": 2.0 },
                        "desk_distance_from_window_m": { "type": "number", "default": 2.5 }
                    },
                    "required": ["room_length_m", "room_width_m", "room_height_m", "window_width_m", "window_height_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cad_hardware_shaft_key".into(),
                description: "Calculate transmission shaft diameter under bending and torsion, torsional deflection, and DIN 6885-1 parallel key stresses".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "transmitted_power_kw": { "type": "number", "default": 45.0 },
                        "rotational_speed_rpm": { "type": "number", "default": 1450.0 },
                        "bending_moment_nm": { "type": "number", "default": 350.0 },
                        "selected_shaft_diameter_mm": { "type": "number", "default": 45.0 }
                    },
                    "required": ["transmitted_power_kw", "rotational_speed_rpm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_pressure_vessel".into(),
                description: "Calculate ASME BPVC Section VIII Division 1 shell and head required thicknesses, MAWP, and hydrotest pressure".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "design_pressure_bar": { "type": "number", "default": 20.0 },
                        "inside_diameter_d_mm": { "type": "number", "default": 1200.0 },
                        "tangent_length_l_mm": { "type": "number", "default": 3000.0 },
                        "corrosion_allowance_mm": { "type": "number", "default": 2.5 }
                    },
                    "required": ["design_pressure_bar", "inside_diameter_d_mm", "tangent_length_l_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_psychrometrics".into(),
                description: "Calculate ASHRAE moist air psychrometric properties: vapor pressure, humidity ratio, enthalpy, dew point, wet bulb, and density".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dry_bulb_temp_c": { "type": "number", "default": 24.0 },
                        "relative_humidity": { "type": "number", "default": 0.50 }
                    },
                    "required": ["dry_bulb_temp_c", "relative_humidity"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_hydrology".into(),
                description: "Calculate watershed Kirpich time of concentration, IDF rainfall intensity, peak runoff Q, and Manning culvert pipe diameter".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "catchment_area_ha": { "type": "number", "default": 8.5 },
                        "flow_path_length_m": { "type": "number", "default": 450.0 },
                        "terrain_slope": { "type": "number", "default": 0.020 },
                        "pipe_slope": { "type": "number", "default": 0.008 }
                    },
                    "required": ["catchment_area_ha", "flow_path_length_m", "terrain_slope", "pipe_slope"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_fracture_lefm".into(),
                description: "Calculate Mode I stress intensity factor K_I, fracture toughness margin K_IC, critical flaw size a_c, and Paris fatigue cycle life".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "max_stress_mpa": { "type": "number", "default": 180.0 },
                        "min_stress_mpa": { "type": "number", "default": 18.0 },
                        "initial_crack_depth_a0_mm": { "type": "number", "default": 1.5 },
                        "plate_width_w_mm": { "type": "number", "default": 80.0 }
                    },
                    "required": ["max_stress_mpa", "initial_crack_depth_a0_mm", "plate_width_w_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_rotordynamics".into(),
                description: "API 617 lateral rotor dynamics, flexible-bearing critical speeds, Campbell gyroscopic split, and unbalance response".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operating_speed_rpm": { "type": "number", "default": 12000.0 },
                        "shaft_span_m": { "type": "number", "default": 1.20 },
                        "shaft_diameter_m": { "type": "number", "default": 0.080 },
                        "disc_mass_kg": { "type": "number", "default": 45.0 }
                    },
                    "required": ["operating_speed_rpm", "shaft_span_m", "shaft_diameter_m", "disc_mass_kg"]
                }),
            },
            McpToolDefinition {
                name: "atlas_cam_welding_metallurgy".into(),
                description: "AWS D1.1 / ISO 15614 arc welding heat input, 2D/3D Rosenthal t8/5 cooling rates, carbon equivalents, and HAZ hardness".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "arc_voltage_v": { "type": "number", "default": 28.0 },
                        "welding_current_a": { "type": "number", "default": 240.0 },
                        "travel_speed_mm_s": { "type": "number", "default": 4.5 },
                        "plate_thickness_mm": { "type": "number", "default": 25.0 }
                    },
                    "required": ["arc_voltage_v", "welding_current_a", "travel_speed_mm_s", "plate_thickness_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_chemical_distillation".into(),
                description: "Binary distillation column McCabe-Thiele design, Fenske minimum stages, Underwood R_min, and reboiler/condenser duties".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "feed_rate_kmol_hr": { "type": "number", "default": 100.0 },
                        "feed_mole_fraction_zf": { "type": "number", "default": 0.45 },
                        "distillate_mole_fraction_xd": { "type": "number", "default": 0.95 },
                        "bottoms_mole_fraction_xb": { "type": "number", "default": 0.05 },
                        "relative_volatility_alpha": { "type": "number", "default": 2.45 }
                    },
                    "required": ["feed_rate_kmol_hr", "feed_mole_fraction_zf", "distillate_mole_fraction_xd", "bottoms_mole_fraction_xb"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_pcb_thermal".into(),
                description: "IPC-2152 PCB copper trace current carrying ampacity, allowable temperature rise, Joule heating, and thermal via arrays".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "trace_width_mm": { "type": "number", "default": 2.50 },
                        "trace_length_mm": { "type": "number", "default": 50.0 },
                        "current_a": { "type": "number", "default": 5.5 },
                        "allowable_temp_rise_c": { "type": "number", "default": 20.0 }
                    },
                    "required": ["trace_width_mm", "trace_length_mm", "current_a"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_structural_connections".into(),
                description: "Eurocode 3 / AISC 360 structural steel bolted joint shear/bearing/tension and fillet weld directional throat stresses".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "applied_shear_force_kn": { "type": "number", "default": 180.0 },
                        "applied_tension_force_kn": { "type": "number", "default": 80.0 },
                        "weld_throat_thickness_mm": { "type": "number", "default": 6.0 },
                        "weld_length_mm": { "type": "number", "default": 220.0 },
                        "applied_weld_force_kn": { "type": "number", "default": 150.0 }
                    },
                    "required": ["applied_shear_force_kn", "weld_throat_thickness_mm", "weld_length_mm", "applied_weld_force_kn"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_hydraulic_pump".into(),
                description: "ISO 9906 / ASME PTC 10 centrifugal pump characteristic curve, affinity laws, system head intersection, and cavitation margin".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_speed_rpm": { "type": "number", "default": 2950.0 },
                        "nominal_impeller_diameter_mm": { "type": "number", "default": 250.0 },
                        "shutoff_head_m": { "type": "number", "default": 85.0 },
                        "system_static_head_m": { "type": "number", "default": 25.0 },
                        "npsh_available_m": { "type": "number", "default": 6.5 }
                    },
                    "required": ["nominal_speed_rpm", "nominal_impeller_diameter_mm", "shutoff_head_m", "system_static_head_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_heat_exchanger_rating".into(),
                description: "TEMA / Kern method shell-and-tube rating, counter-flow LMTD, F-correction, overall U, and surface area safety margin".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "hot_mass_flow_kg_s": { "type": "number", "default": 18.0 },
                        "cold_mass_flow_kg_s": { "type": "number", "default": 24.66 },
                        "shell_diameter_mm": { "type": "number", "default": 600.0 },
                        "tube_count": { "type": "integer", "default": 380 },
                        "tube_length_m": { "type": "number", "default": 4.5 }
                    },
                    "required": ["hot_mass_flow_kg_s", "cold_mass_flow_kg_s", "shell_diameter_mm", "tube_count", "tube_length_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_geotechnical_bearing".into(),
                description: "Meyerhof / Eurocode 7 shallow foundation ultimate & allowable bearing capacity, and elastic immediate settlement".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "foundation_width_b_m": { "type": "number", "default": 2.20 },
                        "foundation_length_l_m": { "type": "number", "default": 2.20 },
                        "embedment_depth_df_m": { "type": "number", "default": 1.50 },
                        "column_axial_load_kn": { "type": "number", "default": 1250.0 },
                        "friction_angle_deg": { "type": "number", "default": 32.0 }
                    },
                    "required": ["foundation_width_b_m", "foundation_length_l_m", "embedment_depth_df_m", "column_axial_load_kn"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_gas_compression".into(),
                description: "ASME PTC 10 multi-stage gas compression thermodynamics, optimal stage pressure ratio, intercooling heat duties, and power savings".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mass_flow_kg_s": { "type": "number", "default": 4.5 },
                        "inlet_pressure_bar": { "type": "number", "default": 1.0 },
                        "target_outlet_pressure_bar": { "type": "number", "default": 30.0 },
                        "number_of_stages": { "type": "integer", "default": 3 }
                    },
                    "required": ["mass_flow_kg_s", "inlet_pressure_bar", "target_outlet_pressure_bar", "number_of_stages"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_timber_fire".into(),
                description: "Eurocode 5 EN 1995-1-2 structural timber fire resistance, 1D/effective charring depth, residual section modulus, and R30/R60/R90 verification".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "initial_breadth_mm": { "type": "number", "default": 200.0 },
                        "initial_depth_mm": { "type": "number", "default": 500.0 },
                        "fire_duration_min": { "type": "number", "default": 60.0 },
                        "ambient_moment_ed_kn_m": { "type": "number", "default": 140.0 }
                    },
                    "required": ["initial_breadth_mm", "initial_depth_mm", "fire_duration_min", "ambient_moment_ed_kn_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_propeller".into(),
                description: "Wageningen B-series marine propeller open-water hydrodynamics (KT, KQ, eta_0), thrust, torque, power, and Keller cavitation criteria".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "diameter_m": { "type": "number", "default": 6.80 },
                        "pitch_ratio_p_d": { "type": "number", "default": 1.05 },
                        "rotational_speed_rpm": { "type": "number", "default": 108.0 },
                        "advance_speed_knots": { "type": "number", "default": 17.5 }
                    },
                    "required": ["diameter_m", "pitch_ratio_p_d", "rotational_speed_rpm", "advance_speed_knots"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_automotive_braking".into(),
                description: "ECE R13H automotive braking dynamics, dynamic axle load transfer, stopping distance, and disc rotor thermal dissipation".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "vehicle_mass_kg": { "type": "number", "default": 1850.0 },
                        "wheelbase_m": { "type": "number", "default": 2.85 },
                        "initial_speed_kph": { "type": "number", "default": 100.0 },
                        "road_friction_coeff": { "type": "number", "default": 0.92 }
                    },
                    "required": ["vehicle_mass_kg", "wheelbase_m", "initial_speed_kph"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_reinforced_concrete".into(),
                description: "Eurocode 2 EN 1992-1-1 reinforced concrete beam flexural moment capacity M_Rd, neutral axis depth, and SLS crack width w_k".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "breadth_b_mm": { "type": "number", "default": 300.0 },
                        "depth_h_mm": { "type": "number", "default": 600.0 },
                        "applied_moment_ed_kn_m": { "type": "number", "default": 240.0 },
                        "rebar_count": { "type": "integer", "default": 4 },
                        "rebar_diameter_mm": { "type": "number", "default": 20.0 }
                    },
                    "required": ["breadth_b_mm", "depth_h_mm", "applied_moment_ed_kn_m", "rebar_count"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_hvac_duct_airflow".into(),
                description: "ASHRAE / SMACNA HVAC airflow duct hydraulics, Huebscher equivalent hydraulic diameter, friction loss, ESP, and fan brake horsepower".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "airflow_m3_h": { "type": "number", "default": 5400.0 },
                        "duct_width_mm": { "type": "number", "default": 600.0 },
                        "duct_height_mm": { "type": "number", "default": 350.0 },
                        "duct_length_m": { "type": "number", "default": 45.0 }
                    },
                    "required": ["airflow_m3_h", "duct_width_mm", "duct_height_mm", "duct_length_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_gas_turbine".into(),
                description: "NASA SP-290 aero gas turbine open Brayton cycle, compressor/turbine states, fuel-air ratio, net thrust, TSFC, and thermal efficiency".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "flight_mach": { "type": "number", "default": 0.82 },
                        "altitude_m": { "type": "number", "default": 10668.0 },
                        "core_mass_flow_kg_s": { "type": "number", "default": 45.0 },
                        "compressor_pressure_ratio": { "type": "number", "default": 24.0 },
                        "turbine_inlet_temp_k": { "type": "number", "default": 1650.0 }
                    },
                    "required": ["flight_mach", "altitude_m", "core_mass_flow_kg_s", "compressor_pressure_ratio", "turbine_inlet_temp_k"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_hydraulic_cylinder".into(),
                description: "ISO 6020-2 / NFPA hydraulic actuator mechanics, thick-walled barrel Lamé stresses, Euler/Rankine-Gordon rod buckling, and cushion deceleration".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bore_diameter_mm": { "type": "number", "default": 100.0 },
                        "rod_diameter_mm": { "type": "number", "default": 56.0 },
                        "stroke_length_mm": { "type": "number", "default": 600.0 },
                        "working_pressure_bar": { "type": "number", "default": 210.0 },
                        "barrel_wall_thickness_mm": { "type": "number", "default": 12.0 }
                    },
                    "required": ["bore_diameter_mm", "rod_diameter_mm", "stroke_length_mm", "working_pressure_bar"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mechanical_belt_drive".into(),
                description: "ISO 5296 / RMA IP-20 belt & pulley power transmission, Euler-Eytelwein tensioning, centrifugal tension, parallel belt count, and fatigue life".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "power_kw": { "type": "number", "default": 15.0 },
                        "driver_speed_rpm": { "type": "number", "default": 1450.0 },
                        "driver_diameter_mm": { "type": "number", "default": 160.0 },
                        "driven_diameter_mm": { "type": "number", "default": 315.0 },
                        "center_distance_mm": { "type": "number", "default": 600.0 }
                    },
                    "required": ["power_kw", "driver_speed_rpm", "driver_diameter_mm", "driven_diameter_mm", "center_distance_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_geotechnical_slope_stability".into(),
                description: "Eurocode 7 / USACE soil slope stability analysis via Bishop's Simplified Method of Slices with pore water ratio (ru) and seismic inertia (kh)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "slope_height_m": { "type": "number", "default": 10.0 },
                        "slope_angle_deg": { "type": "number", "default": 32.0 },
                        "soil_unit_weight_kn_m3": { "type": "number", "default": 19.0 },
                        "effective_cohesion_kpa": { "type": "number", "default": 12.0 },
                        "effective_friction_angle_deg": { "type": "number", "default": 27.0 }
                    },
                    "required": ["slope_height_m", "slope_angle_deg"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electrical_induction_motor".into(),
                description: "IEEE 112 / NEMA MG-1 three-phase induction motor electromechanics, per-phase equivalent circuit, breakdown torque, starting transients, and efficiency".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rated_power_kw": { "type": "number", "default": 15.0 },
                        "rated_voltage_ll_v": { "type": "number", "default": 400.0 },
                        "frequency_hz": { "type": "number", "default": 50.0 },
                        "pole_count": { "type": "integer", "default": 4 }
                    },
                    "required": ["rated_power_kw", "rated_voltage_ll_v", "frequency_hz", "pole_count"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_thermal_solar_collector".into(),
                description: "ISO 9806 / EN 12975 solar thermal flat-plate and evacuated-tube collector efficiency rating, fluid temperature gain, and stagnation temperatures".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "aperture_area_m2": { "type": "number", "default": 2.30 },
                        "solar_irradiance_w_m2": { "type": "number", "default": 850.0 },
                        "ambient_temp_c": { "type": "number", "default": 20.0 },
                        "fluid_inlet_temp_c": { "type": "number", "default": 45.0 }
                    },
                    "required": ["aperture_area_m2", "solar_irradiance_w_m2", "ambient_temp_c", "fluid_inlet_temp_c"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_tribology_journal_bearing".into(),
                description: "ISO 7902 / DIN 31652 hydrodynamic fluid-film journal bearing mechanics, Sommerfeld number, eccentricity, minimum oil film, and power loss".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "journal_diameter_mm": { "type": "number", "default": 100.0 },
                        "bearing_length_mm": { "type": "number", "default": 70.0 },
                        "radial_clearance_um": { "type": "number", "default": 65.0 },
                        "rotational_speed_rpm": { "type": "number", "default": 3000.0 },
                        "radial_load_n": { "type": "number", "default": 12500.0 }
                    },
                    "required": ["journal_diameter_mm", "bearing_length_mm", "radial_clearance_um", "rotational_speed_rpm", "radial_load_n"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_renewable_wind_turbine".into(),
                description: "IEC 61400-1 horizontal-axis wind turbine BEM aeromechanics, swept area, Betz limit, pitch-regulated power curve, and Weibull AEP".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rotor_radius_m": { "type": "number", "default": 63.0 },
                        "rated_power_mw": { "type": "number", "default": 4.0 },
                        "rated_wind_speed_m_s": { "type": "number", "default": 11.5 }
                    },
                    "required": ["rotor_radius_m", "rated_power_mw", "rated_wind_speed_m_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_thermal_steam_rankine".into(),
                description: "IAPWS-IF97 / ASME PTC 6 steam Rankine cycle, superheated turbine expansion, exhaust quality, thermal efficiency, and condenser duty".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "steam_mass_flow_kg_s": { "type": "number", "default": 120.0 },
                        "boiler_pressure_bar": { "type": "number", "default": 120.0 },
                        "superheater_temp_c": { "type": "number", "default": 540.0 }
                    },
                    "required": ["steam_mass_flow_kg_s", "boiler_pressure_bar", "superheater_temp_c"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_pile_foundation".into(),
                description: "Eurocode 7 / API RP 2A bored pile axial capacity, stratified shaft skin friction (alpha/beta), base end-bearing, and elastic settlement".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pile_diameter_m": { "type": "number", "default": 1.0 },
                        "pile_length_m": { "type": "number", "default": 20.0 },
                        "applied_axial_load_kn": { "type": "number", "default": 3200.0 }
                    },
                    "required": ["pile_diameter_m", "pile_length_m", "applied_axial_load_kn"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mechanical_gear_fatigue".into(),
                description: "ISO 6336 Method B spur/helical gear pitting contact fatigue (sigma_H, S_H) and tooth root bending fatigue (sigma_F, S_F)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "transmitted_power_kw": { "type": "number", "default": 55.0 },
                        "pinion_speed_rpm": { "type": "number", "default": 1450.0 },
                        "pinion_teeth_z1": { "type": "integer", "default": 21 },
                        "gear_teeth_z2": { "type": "integer", "default": 63 },
                        "normal_module_mm": { "type": "number", "default": 4.0 },
                        "face_width_mm": { "type": "number", "default": 50.0 }
                    },
                    "required": ["transmitted_power_kw", "pinion_speed_rpm", "pinion_teeth_z1", "gear_teeth_z2", "normal_module_mm", "face_width_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_battery_thermal".into(),
                description: "IEC 62660 / SAE J2464 battery pack liquid cooling plate thermal analysis, Joule/entropic heat, coolant temperature rise, and cell thermal gradient".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "cell_count_series": { "type": "integer", "default": 96 },
                        "cell_count_parallel": { "type": "integer", "default": 4 },
                        "operating_current_a": { "type": "number", "default": 200.0 },
                        "coolant_flow_rate_l_min": { "type": "number", "default": 15.0 }
                    },
                    "required": ["cell_count_series", "cell_count_parallel", "operating_current_a", "coolant_flow_rate_l_min"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_rocket_nozzle".into(),
                description: "NASA SP-8039 rocket engine De Laval nozzle isentropic compressible gas dynamics, characteristic velocity c*, exit Mach, sea-level/vacuum Isp, and flow separation".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "chamber_pressure_bar": { "type": "number", "default": 100.0 },
                        "chamber_temperature_k": { "type": "number", "default": 3500.0 },
                        "throat_diameter_mm": { "type": "number", "default": 120.0 },
                        "expansion_ratio_eps": { "type": "number", "default": 16.0 }
                    },
                    "required": ["chamber_pressure_bar", "chamber_temperature_k", "throat_diameter_mm", "expansion_ratio_eps"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_chemical_cstr_reactor".into(),
                description: "Continuous stirred-tank reactor (CSTR) Arrhenius kinetics, Damköhler number, Van Heerden heat generation vs removal, and thermal runaway ignition stability".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "reactor_volume_m3": { "type": "number", "default": 2.50 },
                        "feed_volumetric_flow_m3_h": { "type": "number", "default": 5.0 },
                        "feed_reactant_concentration_mol_m3": { "type": "number", "default": 2000.0 }
                    },
                    "required": ["reactor_volume_m3", "feed_volumetric_flow_m3_h", "feed_reactant_concentration_mol_m3"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_channel_hydraulics".into(),
                description: "Manning open channel uniform flow hydraulics, normal and critical depths, Froude regime, sequent depths, and hydraulic jump energy dissipation".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bottom_width_m": { "type": "number", "default": 6.0 },
                        "side_slope_z": { "type": "number", "default": 1.5 },
                        "discharge_rate_m3_s": { "type": "number", "default": 45.0 },
                        "bed_slope_s0": { "type": "number", "default": 0.0015 }
                    },
                    "required": ["bottom_width_m", "discharge_rate_m3_s", "bed_slope_s0"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electrical_transmission_line".into(),
                description: "IEEE 738 / IEC 60826 overhead transmission conductor catenary sag and tension, change-of-state cubic equation, hot sag, storm tension, and safety factors".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "span_length_m": { "type": "number", "default": 350.0 },
                        "conductor_diameter_mm": { "type": "number", "default": 28.14 },
                        "rated_tensile_strength_kn": { "type": "number", "default": 140.0 },
                        "initial_horizontal_tension_kn": { "type": "number", "default": 28.0 }
                    },
                    "required": ["span_length_m", "conductor_diameter_mm", "rated_tensile_strength_kn", "initial_horizontal_tension_kn"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_piping_flexibility".into(),
                description: "ASME B31.3 / B31.1 process piping thermal expansion stress range, guided U-loop sizing, flexibility factor, and anchor reaction loads".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_pipe_size_inches": { "type": "number", "default": 8.0 },
                        "pipe_outer_diameter_mm": { "type": "number", "default": 219.1 },
                        "wall_thickness_mm": { "type": "number", "default": 8.18 },
                        "total_line_length_m": { "type": "number", "default": 60.0 },
                        "design_temperature_c": { "type": "number", "default": 260.0 },
                        "ambient_temperature_c": { "type": "number", "default": 20.0 },
                        "expansion_loop_height_m": { "type": "number", "default": 7.5 },
                        "expansion_loop_width_m": { "type": "number", "default": 5.0 }
                    },
                    "required": ["total_line_length_m", "design_temperature_c"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_automotive_quarter_car".into(),
                description: "ISO 2631 / Milliken quarter-car ride dynamics, 2-DOF frequencies, RMS vertical acceleration, suspension travel, and road roughness ISO PSD response".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "sprung_mass_kg": { "type": "number", "default": 400.0 },
                        "unsprung_mass_kg": { "type": "number", "default": 45.0 },
                        "suspension_stiffness_n_m": { "type": "number", "default": 30000.0 },
                        "suspension_damping_n_s_m": { "type": "number", "default": 1800.0 },
                        "tire_stiffness_n_m": { "type": "number", "default": 200000.0 },
                        "vehicle_speed_km_h": { "type": "number", "default": 100.0 },
                        "road_roughness_class": { "type": "string", "default": "ClassBGood" }
                    },
                    "required": ["sprung_mass_kg", "unsprung_mass_kg", "suspension_stiffness_n_m", "suspension_damping_n_s_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electrical_power_transformer".into(),
                description: "IEC 60076 / IEEE C57 power transformer electromagnetics, core and copper losses, percentage impedance, voltage regulation, hot-spot temperature, and aging rate".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rated_power_mva": { "type": "number", "default": 40.0 },
                        "primary_voltage_kv": { "type": "number", "default": 132.0 },
                        "secondary_voltage_kv": { "type": "number", "default": 33.0 },
                        "frequency_hz": { "type": "number", "default": 50.0 },
                        "no_load_core_loss_kw": { "type": "number", "default": 25.0 },
                        "full_load_copper_loss_kw": { "type": "number", "default": 160.0 },
                        "percent_impedance": { "type": "number", "default": 10.5 }
                    },
                    "required": ["rated_power_mva", "primary_voltage_kv", "secondary_voltage_kv"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_thermal_refrigeration_cycle".into(),
                description: "ASHRAE / AHRI vapor compression refrigeration and heat pump thermodynamic cycle, COP, mass flow rate, compressor power, and discharge temperature".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "refrigerant": { "type": "string", "default": "R410A" },
                        "design_cooling_capacity_kw": { "type": "number", "default": 100.0 },
                        "evaporating_temperature_c": { "type": "number", "default": 5.0 },
                        "condensing_temperature_c": { "type": "number", "default": 45.0 },
                        "superheat_k": { "type": "number", "default": 5.0 },
                        "subcooling_k": { "type": "number", "default": 5.0 },
                        "compressor_isentropic_efficiency": { "type": "number", "default": 0.78 }
                    },
                    "required": ["design_cooling_capacity_kw"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_ship_stability".into(),
                description: "IMO Resolution MSC.267(85) ship hydrostatic intact stability, displacement, metacentric height GM0, righting arm GZ curve, and dynamic stability criteria".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "length_bp_m": { "type": "number", "default": 120.0 },
                        "beam_m": { "type": "number", "default": 20.0 },
                        "draft_m": { "type": "number", "default": 7.0 },
                        "depth_m": { "type": "number", "default": 11.5 },
                        "block_coefficient": { "type": "number", "default": 0.68 },
                        "vertical_center_of_gravity_kg_m": { "type": "number", "default": 7.2 }
                    },
                    "required": ["length_bp_m", "beam_m", "draft_m", "vertical_center_of_gravity_kg_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_manufacturing_injection_molding".into(),
                description: "ISO 294 / SPE plastic injection molding cavity filling pressure, Ballman-Shusman 1D transient cooling time, cycle time, and machine clamp tonnage".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "resin": { "type": "string", "default": "ABS" },
                        "nominal_wall_thickness_mm": { "type": "number", "default": 2.5 },
                        "projected_area_cm2": { "type": "number", "default": 180.0 },
                        "cavity_volume_cm3": { "type": "number", "default": 112.5 },
                        "max_flow_length_mm": { "type": "number", "default": 175.0 },
                        "injection_time_s": { "type": "number", "default": 1.5 },
                        "cavity_count": { "type": "integer", "default": 2 }
                    },
                    "required": ["nominal_wall_thickness_mm", "projected_area_cm2", "cavity_volume_cm3"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_seismic_analysis".into(),
                description: "ASCE 7-22 / Eurocode 8 structural seismic equivalent lateral force procedure, design response spectrum, fundamental period, base shear Vb, story forces, and roof drift".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "building_height_m": { "type": "number", "default": 36.0 },
                        "story_count": { "type": "integer", "default": 10 },
                        "total_seismic_weight_kn": { "type": "number", "default": 58000.0 },
                        "short_period_spectral_accel_ss_g": { "type": "number", "default": 1.20 },
                        "one_second_spectral_accel_s1_g": { "type": "number", "default": 0.48 }
                    },
                    "required": ["building_height_m", "story_count", "total_seismic_weight_kn"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_orbital_mechanics".into(),
                description: "Curtis / Vallado Keplerian two-body orbital mechanics, two-impulse Hohmann transfer delta-V, transfer time of flight, Tsiolkovsky propellant mass, and J2 nodal precession".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "initial_altitude_km": { "type": "number", "default": 400.0 },
                        "target_altitude_km": { "type": "number", "default": 35786.0 },
                        "spacecraft_wet_mass_kg": { "type": "number", "default": 3500.0 },
                        "propulsion_isp_s": { "type": "number", "default": 318.0 },
                        "inclination_deg": { "type": "number", "default": 28.5 }
                    },
                    "required": ["initial_altitude_km", "target_altitude_km", "spacecraft_wet_mass_kg"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electronics_dc_dc_converter".into(),
                description: "Synchronous DC-DC buck converter SMPS power stage, duty cycle, inductor ripple, output capacitor ESR ripple, MOSFET conduction and switching losses, and efficiency".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "input_voltage_v": { "type": "number", "default": 12.0 },
                        "output_voltage_v": { "type": "number", "default": 3.3 },
                        "rated_output_current_a": { "type": "number", "default": 20.0 },
                        "switching_frequency_hz": { "type": "number", "default": 500000.0 }
                    },
                    "required": ["input_voltage_v", "output_voltage_v", "rated_output_current_a"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_science_fusion_tokamak".into(),
                description: "Wesson / ITER thermonuclear D-T tokamak plasma equilibrium, safety factor q95, Troyon beta limit, Greenwald density limit, IPB98(y,2) confinement scaling, and fusion gain Q".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "major_radius_r0_m": { "type": "number", "default": 6.20 },
                        "minor_radius_a_m": { "type": "number", "default": 2.00 },
                        "toroidal_magnetic_field_t": { "type": "number", "default": 5.30 },
                        "plasma_current_ma": { "type": "number", "default": 15.0 },
                        "electron_density_10e20_m3": { "type": "number", "default": 1.01 },
                        "plasma_temperature_kev": { "type": "number", "default": 13.0 }
                    },
                    "required": ["major_radius_r0_m", "minor_radius_a_m", "toroidal_magnetic_field_t", "plasma_current_ma"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_manufacturing_wire_edm".into(),
                description: "ISO 230 / CIRP Wire EDM spark erosion physics, single-pulse energy, Snoeys electro-thermal removal rate, linear cutting speed, kerf width, Ra roughness, and wire deflection".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "workpiece_thickness_mm": { "type": "number", "default": 50.0 },
                        "wire_diameter_mm": { "type": "number", "default": 0.25 },
                        "open_circuit_voltage_v": { "type": "number", "default": 95.0 },
                        "discharge_current_a": { "type": "number", "default": 16.0 },
                        "pulse_on_time_us": { "type": "number", "default": 4.5 },
                        "pulse_off_time_us": { "type": "number", "default": 12.0 }
                    },
                    "required": ["workpiece_thickness_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_renewable_wind_blade_aeroelastic".into(),
                description: "IEC 61400-1 / Theodorsen wind turbine blade aeroelasticity, centrifugal stiffening, coupled flap-torsion flutter velocity, aerodynamic divergence, and extreme gust tip deflection".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "blade_length_m": { "type": "number", "default": 80.0 },
                        "rotor_radius_m": { "type": "number", "default": 82.5 },
                        "rated_rotor_speed_rpm": { "type": "number", "default": 10.5 },
                        "extreme_gust_speed_m_s": { "type": "number", "default": 52.5 }
                    },
                    "required": ["blade_length_m", "rated_rotor_speed_rpm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_subsea_pipeline".into(),
                description: "DNV-ST-F101 / API 5L subsea pipeline wall thickness, hydrostatic collapse resistance, Timoshenko ovality interaction, burst yield pressure, and propagating buckling arrestors".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "outside_diameter_mm": { "type": "number", "default": 508.0 },
                        "nominal_wall_thickness_mm": { "type": "number", "default": 25.4 },
                        "water_depth_m": { "type": "number", "default": 1500.0 },
                        "internal_design_pressure_mpa": { "type": "number", "default": 22.0 }
                    },
                    "required": ["outside_diameter_mm", "nominal_wall_thickness_mm", "water_depth_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_combustor_aerothermal".into(),
                description: "Lefebvre / ICAO gas turbine combustor aerothermal performance, combustion efficiency, Rayleigh momentum and friction pressure drop, turbine inlet temperature, and thermal NOx/CO emissions".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "inlet_total_pressure_mpa": { "type": "number", "default": 2.40 },
                        "inlet_total_temperature_k": { "type": "number", "default": 780.0 },
                        "air_mass_flow_kg_s": { "type": "number", "default": 38.0 },
                        "fuel_mass_flow_kg_s": { "type": "number", "default": 0.76 }
                    },
                    "required": ["inlet_total_pressure_mpa", "inlet_total_temperature_k", "air_mass_flow_kg_s", "fuel_mass_flow_kg_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_bess_grid".into(),
                description: "IEC 62933 / IEEE 2030.2.1 battery energy storage system grid interconnection, cell DC efficiency, PCS inverter losses, medium-voltage transformer efficiency, and AC-to-AC Round-Trip Efficiency".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_capacity_mwh": { "type": "number", "default": 10.0 },
                        "rated_power_mw": { "type": "number", "default": 2.50 },
                        "c_rate": { "type": "number", "default": 0.25 },
                        "ambient_temperature_c": { "type": "number", "default": 28.0 }
                    },
                    "required": ["nominal_capacity_mwh", "rated_power_mw"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_geotechnical_soil_consolidation".into(),
                description: "Terzaghi 1D consolidation theory, primary consolidation settlement, OCR, Fourier time factor Tv, pore water dissipation, and secondary creep".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "layer_thickness_m": { "type": "number", "default": 6.0 },
                        "initial_effective_stress_kpa": { "type": "number", "default": 100.0 },
                        "applied_stress_increment_kpa": { "type": "number", "default": 80.0 }
                    },
                    "required": ["layer_thickness_m", "initial_effective_stress_kpa", "applied_stress_increment_kpa"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_hydraulic_turbine".into(),
                description: "IEC 60193 / Euler turbine equation, Pelton & Francis hydraulic energy conversion, jet kinematics, speed ratio phi, shaft power, and Thoma cavitation factor".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "net_head_m": { "type": "number", "default": 450.0 },
                        "flow_rate_m3_s": { "type": "number", "default": 3.20 },
                        "runner_diameter_m": { "type": "number", "default": 1.80 }
                    },
                    "required": ["net_head_m", "flow_rate_m3_s", "runner_diameter_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_biomedical_stent_expansion".into(),
                description: "ASTM F3067 / FDA coronary stent balloon expansion, crown plastic hinging, elastic recoil, longitudinal foreshortening, and Radial Resistive Force (RRF)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_expanded_diameter_mm": { "type": "number", "default": 3.00 },
                        "balloon_pressure_atm": { "type": "number", "default": 12.0 }
                    },
                    "required": ["nominal_expanded_diameter_mm", "balloon_pressure_atm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electronics_microwave_waveguide".into(),
                description: "IEEE Std 148 / Pozar rectangular waveguide propagation, TE10 cutoff, guide wavelength, phase/group velocities, wave impedance, attenuation, and TE101 cavity unloaded Q".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "broad_dimension_a_mm": { "type": "number", "default": 22.86 },
                        "narrow_dimension_b_mm": { "type": "number", "default": 10.16 },
                        "operating_frequency_ghz": { "type": "number", "default": 10.0 }
                    },
                    "required": ["broad_dimension_a_mm", "narrow_dimension_b_mm", "operating_frequency_ghz"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electrical_gis_dielectric".into(),
                description: "IEC 62271-203 / CIGRE high-voltage gas-insulated switchgear coaxial electric field, Paschen breakdown scaling, surface roughness derating, and LIWV lightning impulse margin".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rated_voltage_kv_rms": { "type": "number", "default": 420.0 },
                        "conductor_radius_mm": { "type": "number", "default": 60.0 },
                        "enclosure_radius_mm": { "type": "number", "default": 165.0 },
                        "gas_pressure_abs_bar": { "type": "number", "default": 5.50 }
                    },
                    "required": ["rated_voltage_kv_rms", "conductor_radius_mm", "enclosure_radius_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_pem_fuel_cell".into(),
                description: "Larminie / Barbir proton exchange membrane fuel cell electrochemistry, Nernst potential, Tafel activation, Springer membrane ohmic resistance, concentration losses, stack power, and H2 consumption".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "cell_count": { "type": "integer", "default": 360 },
                        "active_cell_area_cm2": { "type": "number", "default": 280.0 },
                        "operating_temperature_c": { "type": "number", "default": 75.0 },
                        "operating_current_density_a_cm2": { "type": "number", "default": 1.20 }
                    },
                    "required": ["cell_count", "active_cell_area_cm2"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_science_superconducting_magnet".into(),
                description: "Wilson / CERN superconducting solenoid magnet Biot-Savart field, stored magnetic energy, load line critical current margin, Lorentz hoop stress, and quench dump protection with adiabatic hotspot temperature".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "inner_radius_m": { "type": "number", "default": 0.35 },
                        "outer_radius_m": { "type": "number", "default": 0.48 },
                        "coil_length_m": { "type": "number", "default": 1.25 },
                        "turns_count": { "type": "integer", "default": 2600 },
                        "operating_current_a": { "type": "number", "default": 1400.0 }
                    },
                    "required": ["inner_radius_m", "outer_radius_m", "coil_length_m", "turns_count"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_reentry_aerothermodynamics".into(),
                description: "Sutton-Graves / Fay-Riddell / NASA SP-8000 hypersonic entry stagnation point convective and radiative heat flux, shock standoff distance, radiative equilibrium wall temperature, and TPS ablation sizing".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "freestream_velocity_m_s": { "type": "number", "default": 7400.0 },
                        "altitude_km": { "type": "number", "default": 62.0 },
                        "nose_radius_m": { "type": "number", "default": 1.15 },
                        "vehicle_mass_kg": { "type": "number", "default": 5200.0 }
                    },
                    "required": ["freestream_velocity_m_s", "altitude_km", "nose_radius_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_manufacturing_lpbf_meltpool".into(),
                description: "King / Rosenthal laser powder bed fusion (LPBF/DMLS) moving Gaussian laser thermals, volumetric energy density (VED), King normalized enthalpy, melt pool dimensions, cooling rate, and conduction vs keyhole defect regimes".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "laser_power_w": { "type": "number", "default": 250.0 },
                        "scan_speed_mm_s": { "type": "number", "default": 950.0 },
                        "beam_diameter_um": { "type": "number", "default": 80.0 },
                        "layer_thickness_um": { "type": "number", "default": 40.0 }
                    },
                    "required": ["laser_power_w", "scan_speed_mm_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_acoustics_phononic_bandgap".into(),
                description: "Mead / Brillouin / Sigalas 1D periodic acoustic metamaterial and phononic crystal Floquet-Bloch dispersion, Bragg stopbands, evanescent attenuation decay, and transmission loss (TL) in dB".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "unit_cell_count": { "type": "integer", "default": 8 },
                        "layer_a_thickness_mm": { "type": "number", "default": 24.0 },
                        "layer_b_thickness_mm": { "type": "number", "default": 16.0 },
                        "excitation_frequency_hz": { "type": "number", "default": 24000.0 }
                    },
                    "required": ["unit_cell_count"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_rail_wheel_rail_contact".into(),
                description: "Kalker / Hertz / Nadal wheel-rail 3D elliptical contact mechanics, peak normal pressure, longitudinal/lateral creep coefficients, Klingel hunting frequency, and Nadal derailment quotient".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wheel_vertical_load_kn": { "type": "number", "default": 100.0 },
                        "wheel_radius_mm": { "type": "number", "default": 460.0 },
                        "rail_crown_radius_mm": { "type": "number", "default": 300.0 },
                        "train_speed_km_h": { "type": "number", "default": 200.0 }
                    },
                    "required": ["wheel_vertical_load_kn", "wheel_radius_mm", "rail_crown_radius_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mining_mine_ventilation".into(),
                description: "Atkinson / MSHA subsurface mine ventilation aerodynamics, airway friction resistance, natural ventilation buoyancy (NVE), required fan operating pressure, air power, and methane gas dilution".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "airway_length_m": { "type": "number", "default": 1400.0 },
                        "airway_area_m2": { "type": "number", "default": 20.0 },
                        "airway_perimeter_m": { "type": "number", "default": 18.0 },
                        "airflow_rate_m3_s": { "type": "number", "default": 85.0 }
                    },
                    "required": ["airway_length_m", "airway_area_m2", "airway_perimeter_m", "airflow_rate_m3_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_telecom_reflector_antenna".into(),
                description: "Silver / Stutzman / ITU-R S.580 parabolic reflector dish antenna electromagnetics, Ruze surface tolerance efficiency, aperture illumination efficiency, bore-sight gain in dBi, beamwidths, and ITU-R envelope mask".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dish_diameter_m": { "type": "number", "default": 2.40 },
                        "focal_length_m": { "type": "number", "default": 0.96 },
                        "operating_frequency_ghz": { "type": "number", "default": 14.25 }
                    },
                    "required": ["dish_diameter_m", "focal_length_m", "operating_frequency_ghz"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_semiconductor_gaa_fet".into(),
                description: "Taur / BSIM-CMG / IRDS sub-3nm Gate-All-Around (GAA) nanosheet transistor 3D natural electrostatic scaling length, DIBL, subthreshold swing, threshold voltage, quasi-ballistic drive current, and Ion/Ioff ratio".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nanosheet_count": { "type": "integer", "default": 3 },
                        "gate_length_nm": { "type": "number", "default": 12.0 },
                        "nanosheet_width_nm": { "type": "number", "default": 30.0 },
                        "nanosheet_thickness_nm": { "type": "number", "default": 5.0 },
                        "equivalent_oxide_thickness_nm": { "type": "number", "default": 0.75 }
                    },
                    "required": ["nanosheet_count", "gate_length_nm", "nanosheet_width_nm", "nanosheet_thickness_nm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_agri_irrigation_hydraulics".into(),
                description: "Christiansen / Hazen-Williams / ISO 9261 precision agricultural irrigation lateral line multi-outlet friction head loss, pressure and discharge variation, Christiansen Uniformity (CU), and Emission Uniformity (EU)".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pipe_inside_diameter_mm": { "type": "number", "default": 16.0 },
                        "lateral_length_m": { "type": "number", "default": 120.0 },
                        "emitter_spacing_m": { "type": "number", "default": 0.40 },
                        "nominal_discharge_l_h": { "type": "number", "default": 1.60 },
                        "inlet_pressure_head_m": { "type": "number", "default": 11.0 }
                    },
                    "required": ["pipe_inside_diameter_mm", "lateral_length_m", "emitter_spacing_m", "nominal_discharge_l_h"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_environmental_wastewater_treatment".into(),
                description: "Activated Sludge Aeration & Clarifier Sedimentation (ASCE / WEF / Takacs / Metcalf & Eddy) biological basin volume, HRT, F/M, WAS sludge production, SOTR oxygen transfer rate, blower power, and clarifier SOR/SLR solids flux".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "influent_flow_m3_day": { "type": "number", "default": 20000.0 },
                        "influent_bod_mg_l": { "type": "number", "default": 220.0 },
                        "target_effluent_bod_mg_l": { "type": "number", "default": 15.0 },
                        "mlss_mg_l": { "type": "number", "default": 3000.0 },
                        "sludge_retention_time_days": { "type": "number", "default": 8.0 }
                    },
                    "required": ["influent_flow_m3_day", "influent_bod_mg_l", "mlss_mg_l"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_automotive_ev_powertrain".into(),
                description: "EV Longitudinal Dynamics, Traction Motor Envelope & Regenerative Braking (SAE J2452 / UNECE R13-H) aerodynamic and rolling road load, wheel/battery power, cruising range, 0-100 km/h acceleration, top speed, and regen energy recovery".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "vehicle_mass_kg": { "type": "number", "default": 1850.0 },
                        "motor_peak_torque_nm": { "type": "number", "default": 350.0 },
                        "battery_capacity_kwh": { "type": "number", "default": 75.0 },
                        "cruising_speed_km_h": { "type": "number", "default": 100.0 }
                    },
                    "required": ["vehicle_mass_kg", "motor_peak_torque_nm", "battery_capacity_kwh"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_seismic_base_isolation".into(),
                description: "Lead-Rubber Bearing (LRB) Seismic Base Isolation Mechanics (ASCE 7-22 / FEMA P-751) characteristic yield strength, post-yield stiffness, effective period, equivalent viscous damping ratio, design displacement, and structural base shear attenuation".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "superstructure_mass_tonnes": { "type": "number", "default": 4500.0 },
                        "bearing_count": { "type": "integer", "default": 16 },
                        "bearing_outer_diameter_mm": { "type": "number", "default": 800.0 },
                        "lead_core_diameter_mm": { "type": "number", "default": 160.0 },
                        "total_rubber_thickness_mm": { "type": "number", "default": 200.0 }
                    },
                    "required": ["superstructure_mass_tonnes", "bearing_count", "bearing_outer_diameter_mm", "lead_core_diameter_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_defense_terminal_ballistics".into(),
                description: "Terminal Ballistics & Armor Hydrodynamic Penetration (Tate-Alekseevskii / Recht-Ipson / NATO STANAG 4569) modified Bernoulli penetration, erosion velocity, depth of penetration, ballistic limit V50, and armor protection level".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "projectile_length_mm": { "type": "number", "default": 90.0 },
                        "impact_velocity_m_s": { "type": "number", "default": 1450.0 },
                        "obliquity_angle_deg": { "type": "number", "default": 30.0 },
                        "armor_thickness_mm": { "type": "number", "default": 75.0 }
                    },
                    "required": ["projectile_length_mm", "impact_velocity_m_s", "armor_thickness_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_packaging_solder_thermal_fatigue".into(),
                description: "BGA Solder Joint Thermal Fatigue & Creep Lifetime (Engelmaier / Coffin-Manson / IPC-9701 / JEDEC JESD22-A104) DNP thermal shear strain, creep ductility exponent, thermal cycles to failure, Weibull characteristic life, and qualification compliance".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "package_width_mm": { "type": "number", "default": 35.0 },
                        "solder_joint_height_um": { "type": "number", "default": 400.0 },
                        "cycle_min_temp_c": { "type": "number", "default": -40.0 },
                        "cycle_max_temp_c": { "type": "number", "default": 125.0 }
                    },
                    "required": ["package_width_mm", "solder_joint_height_um"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_ship_slamming".into(),
                description: "Evaluates ship bow and bottom hydrodynamic wave slamming impact, peak Wagner pressure, space-averaged pulse, and DNV-RU-SHIP minimum shell plating thickness.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "deadrise_angle_deg": { "type": "number", "default": 18.0 },
                        "relative_impact_velocity_m_s": { "type": "number", "default": 7.5 },
                        "frame_spacing_m": { "type": "number", "default": 0.75 },
                        "plate_length_m": { "type": "number", "default": 2.40 },
                        "plate_thickness_mm": { "type": "number", "default": 16.0 },
                        "steel_yield_strength_mpa": { "type": "number", "default": 355.0 }
                    },
                    "required": ["deadrise_angle_deg", "relative_impact_velocity_m_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_automotive_crash_deceleration".into(),
                description: "Simulates vehicle frontal offset crash deceleration, dynamic crush distance, occupant chest deflection, HIC15, and Prasad-Mertz AIS 4+ brain injury probability.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "vehicle_mass_kg": { "type": "number", "default": 1650.0 },
                        "impact_velocity_km_h": { "type": "number", "default": 64.0 },
                        "crumple_zone_length_m": { "type": "number", "default": 0.72 },
                        "crash_box_mean_force_kn": { "type": "number", "default": 310.0 },
                        "occupant_mass_kg": { "type": "number", "default": 78.0 }
                    },
                    "required": ["vehicle_mass_kg", "impact_velocity_km_h"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_nuclear_fuel_pin".into(),
                description: "Evaluates nuclear reactor core fuel pin radial conduction (UO2, He gap, Zircaloy clad), Dittus-Boelter convection, Tong W-3 CHF, and NRC DNBR thermal safety margin.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "linear_heat_rate_kw_per_m": { "type": "number", "default": 32.0 },
                        "pellet_radius_mm": { "type": "number", "default": 4.10 },
                        "gap_thickness_um": { "type": "number", "default": 80.0 },
                        "clad_thickness_mm": { "type": "number", "default": 0.60 },
                        "coolant_pressure_mpa": { "type": "number", "default": 15.50 },
                        "bulk_coolant_temperature_c": { "type": "number", "default": 305.0 }
                    },
                    "required": ["linear_heat_rate_kw_per_m", "coolant_pressure_mpa"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_chemical_adsorption_psa".into(),
                description: "Calculates Pressure Swing Adsorption (PSA) Langmuir equilibrium, Glueckauf LDF kinetics, Thomas breakthrough dynamics, MTZ length, product purity, and gas recovery.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "bed_diameter_m": { "type": "number", "default": 0.60 },
                        "bed_length_m": { "type": "number", "default": 2.20 },
                        "adsorption_pressure_bar": { "type": "number", "default": 5.50 },
                        "desorption_pressure_bar": { "type": "number", "default": 1.10 },
                        "feed_flow_rate_nm3_h": { "type": "number", "default": 85.0 }
                    },
                    "required": ["adsorption_pressure_bar", "feed_flow_rate_nm3_h"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_rotorcraft_aerodynamics".into(),
                description: "Computes rotorcraft helicopter hover and forward flight blade element momentum aerodynamics, induced/profile power, Figure of Merit, and tip Mach number.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "gross_weight_kg": { "type": "number", "default": 4300.0 },
                        "rotor_radius_m": { "type": "number", "default": 6.50 },
                        "number_of_blades": { "type": "integer", "default": 4 },
                        "blade_chord_m": { "type": "number", "default": 0.38 },
                        "rotor_speed_rpm": { "type": "number", "default": 345.0 },
                        "forward_speed_knots": { "type": "number", "default": 0.0 }
                    },
                    "required": ["gross_weight_kg", "rotor_radius_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_thermal_heat_exchanger_delaware".into(),
                description: "Evaluates shell-and-tube heat exchanger Bell-Delaware crossflow coefficients, LMTD F-factor, overall U-value, required area, and shell pressure drop.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "shell_inside_diameter_m": { "type": "number", "default": 0.60 },
                        "number_of_tubes": { "type": "integer", "default": 360 },
                        "tube_length_m": { "type": "number", "default": 4.80 },
                        "baffle_spacing_m": { "type": "number", "default": 0.30 },
                        "hot_mass_flow_kg_s": { "type": "number", "default": 18.5 }
                    },
                    "required": ["shell_inside_diameter_m", "number_of_tubes"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_geotechnical_drilled_shaft".into(),
                description: "Calculates deep foundation drilled shaft and bored pile O'Neill-Reese side shear, base resistance, AASHTO LRFD factored capacity, and elastic settlement.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "shaft_diameter_m": { "type": "number", "default": 1.20 },
                        "embedded_length_m": { "type": "number", "default": 18.0 },
                        "factored_load_demand_kn": { "type": "number", "default": 3200.0 },
                        "service_load_demand_kn": { "type": "number", "default": 2200.0 }
                    },
                    "required": ["shaft_diameter_m", "embedded_length_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_biomedical_heart_valve".into(),
                description: "Computes prosthetic heart valve Gorlin Effective Orifice Area (EOA), indexed iEOA, Patient-Prosthesis Mismatch (PPM) grade, and ISO 5840 compliance.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_size_mm": { "type": "number", "default": 23.0 },
                        "heart_rate_bpm": { "type": "number", "default": 70.0 },
                        "stroke_volume_ml": { "type": "number", "default": 75.0 },
                        "patient_bsa_m2": { "type": "number", "default": 1.85 }
                    },
                    "required": ["nominal_size_mm", "stroke_volume_ml"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_centrifugal_compressor".into(),
                description: "Evaluates gas turbine centrifugal compressor stage aerodynamics, Stanitz slip factor, Euler work, total pressure ratio, surge margin, and disk hoop stress.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "mass_flow_rate_kg_s": { "type": "number", "default": 3.80 },
                        "rotational_speed_rpm": { "type": "number", "default": 38000.0 },
                        "impeller_tip_diameter_m": { "type": "number", "default": 0.285 }
                    },
                    "required": ["mass_flow_rate_kg_s", "rotational_speed_rpm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_rail_pantograph".into(),
                description: "Calculates high-speed railway pantograph-catenary mean contact force, wave velocity, contact force standard deviation, arcing loss, and wire uplift per EN 50367.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "train_speed_km_h": { "type": "number", "default": 300.0 },
                        "static_contact_force_n": { "type": "number", "default": 80.0 },
                        "contact_wire_tension_kn": { "type": "number", "default": 22.0 }
                    },
                    "required": ["train_speed_km_h", "static_contact_force_n"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_geotechnical_tbm_tunneling".into(),
                description: "Calculates TBM earth pressure balance (EPB) face support pressure, advance thrust, cutting and muck torque, and specific excavation energy per ITA/Herzog.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "tunnel_outer_diameter_m": { "type": "number", "default": 6.60 },
                        "tunnel_depth_to_axis_m": { "type": "number", "default": 22.0 },
                        "water_table_depth_m": { "type": "number", "default": 4.0 }
                    },
                    "required": ["tunnel_outer_diameter_m", "tunnel_depth_to_axis_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_structural_prestressed_concrete".into(),
                description: "Calculates post-tensioned beam parabolic tendon friction/wobble losses, anchorage draw-in seating, long-term losses, and extreme fiber flexural stresses per ACI 318 / AASHTO.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "span_length_m": { "type": "number", "default": 32.0 },
                        "concrete_f_c_mpa": { "type": "number", "default": 45.0 },
                        "tendon_steel_area_mm2": { "type": "number", "default": 2800.0 }
                    },
                    "required": ["span_length_m", "concrete_f_c_mpa"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_spacecraft_jitter".into(),
                description: "Calculates reaction wheel assembly (RWA) static/dynamic unbalance disturbances, bearing harmonic frequencies (BPFO/BPFI), and optical payload line-of-sight jitter per NASA-HDBK-7005.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wheel_speed_rpm": { "type": "number", "default": 3600.0 },
                        "static_imbalance_g_mm": { "type": "number", "default": 1.80 },
                        "structural_resonance_freq_hz": { "type": "number", "default": 85.0 }
                    },
                    "required": ["wheel_speed_rpm", "structural_resonance_freq_hz"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_automotive_radar_lidar_sensor".into(),
                description: "Calculates automotive 77 GHz radar range equation with rain attenuation, ToF pulsed LiDAR optical link budget with Kruse extinction, and sensor fusion detection margins.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "target_range_m": { "type": "number", "default": 120.0 },
                        "target_rcs_m2": { "type": "number", "default": 5.0 },
                        "radar_tx_power_w": { "type": "number", "default": 0.10 }
                    },
                    "required": ["target_range_m", "target_rcs_m2"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_auv_propulsion".into(),
                description: "Calculates AUV hydrodynamic resistance via ITTC-78 and Gertler-Myring form factor, propeller thrust power, and battery mission endurance/range.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "cruising_speed_knots": { "type": "number", "default": 3.50 },
                        "vehicle_length_m": { "type": "number", "default": 4.80 },
                        "battery_capacity_kwh": { "type": "number", "default": 24.0 }
                    },
                    "required": ["cruising_speed_knots", "vehicle_length_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_hydro_governor".into(),
                description: "Calculates elastic water hammer wave speed, Allievi transient head rise, mechanical/water starting times, full load rejection runaway overspeed, and IEEE 1207 isolated governor tuning parameters.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rated_net_head_m": { "type": "number", "default": 145.0 },
                        "rated_discharge_m3_s": { "type": "number", "default": 24.0 },
                        "rated_speed_rpm": { "type": "number", "default": 500.0 },
                        "wicket_gate_closure_time_s": { "type": "number", "default": 6.50 }
                    },
                    "required": ["rated_net_head_m", "rated_discharge_m3_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_semiconductor_cmp_planarization".into(),
                description: "Calculates Preston/Runnels material removal rate, Stribeck lubrication hydrodynamic regime, pad conditioning decay, and pattern-dependent dielectric dishing/erosion.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wafer_diameter_mm": { "type": "number", "default": 300.0 },
                        "down_force_pressure_kpa": { "type": "number", "default": 21.0 },
                        "relative_velocity_m_s": { "type": "number", "default": 1.25 },
                        "polish_time_seconds": { "type": "number", "default": 60.0 }
                    },
                    "required": ["down_force_pressure_kpa", "relative_velocity_m_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_transportation_pavement_mepdg".into(),
                description: "Calculates multi-layer Odemark equivalent thickness, asphalt tensile bottom strain, subgrade compressive strain, and AI cumulative fatigue cracking and subgrade rutting life.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "asphalt_thickness_mm": { "type": "number", "default": 150.0 },
                        "base_thickness_mm": { "type": "number", "default": 200.0 },
                        "subbase_thickness_mm": { "type": "number", "default": 250.0 },
                        "design_traffic_millions_esal": { "type": "number", "default": 8.50 }
                    },
                    "required": ["asphalt_thickness_mm", "base_thickness_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_biomedical_scaffold_degradation".into(),
                description: "Calculates cellular interconnected porosity, Darcy Kozeny-Carman nutrient fluid permeability, Gibson-Ashby porous modulus, and hydrolytic degradation kinetics.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "scaffold_diameter_mm": { "type": "number", "default": 10.0 },
                        "scaffold_height_mm": { "type": "number", "default": 15.0 },
                        "target_porosity_pct": { "type": "number", "default": 75.0 },
                        "degradation_evaluation_weeks": { "type": "number", "default": 12.0 }
                    },
                    "required": ["target_porosity_pct", "degradation_evaluation_weeks"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_satellite_orbital_decay".into(),
                description: "Calculates solar flux thermospheric density, King-Hele semi-major axis altitude decay rate, total re-entry lifetime, and IADC 25-year / FCC 5-year space debris disposal compliance.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "satellite_mass_kg": { "type": "number", "default": 12.0 },
                        "drag_cross_section_area_m2": { "type": "number", "default": 0.045 },
                        "drag_coefficient_cd": { "type": "number", "default": 2.20 },
                        "initial_perigee_altitude_km": { "type": "number", "default": 420.0 },
                        "initial_apogee_altitude_km": { "type": "number", "default": 440.0 }
                    },
                    "required": ["satellite_mass_kg", "drag_cross_section_area_m2"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mining_bucket_wheel_excavator".into(),
                description: "Calculates hourly theoretical excavation volume, cutting lip tangential digging force, wheel shaft drive torque/power, specific cutting energy, and ISO 5048 machine tipping stability.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wheel_diameter_m": { "type": "number", "default": 12.50 },
                        "bucket_nominal_volume_m3": { "type": "number", "default": 1.80 },
                        "wheel_speed_rpm": { "type": "number", "default": 4.80 },
                        "boom_length_m": { "type": "number", "default": 42.0 }
                    },
                    "required": ["wheel_diameter_m", "bucket_nominal_volume_m3"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_process_multiphase_pipe_flow".into(),
                description: "Calculates Lockhart-Martinelli parameter, Chisholm two-phase frictional pressure gradient, in-situ liquid holdup, flow regime, and API RP 14E erosional velocity thresholds.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "pipe_length_m": { "type": "number", "default": 8500.0 },
                        "pipe_inner_diameter_mm": { "type": "number", "default": 254.0 },
                        "liquid_mass_flow_kg_s": { "type": "number", "default": 35.0 },
                        "gas_mass_flow_kg_s": { "type": "number", "default": 4.50 }
                    },
                    "required": ["pipe_length_m", "pipe_inner_diameter_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_geothermal_reservoir".into(),
                description: "Calculates steady-state doublet hydrodynamic pressure drop, circulation pumping power, Gringarten thermal breakthrough arrival time, 30-year wellhead temperature decline, and thermal COP.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "reservoir_temperature_c": { "type": "number", "default": 160.0 },
                        "reinjection_temperature_c": { "type": "number", "default": 65.0 },
                        "brine_flow_rate_l_s": { "type": "number", "default": 75.0 },
                        "well_spacing_distance_m": { "type": "number", "default": 1200.0 }
                    },
                    "required": ["reservoir_temperature_c", "brine_flow_rate_l_s"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_process_spray_drying_kinetics".into(),
                description: "Calculates rotary atomizer tip speed, Sauter Mean Diameter D32, D2-law droplet evaporation drying time, process air heater thermal duty, and Gordon-Taylor powder glass transition stickiness margin.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "feed_rate_kg_h": { "type": "number", "default": 2500.0 },
                        "feed_solids_fraction": { "type": "number", "default": 0.48 },
                        "atomizer_speed_rpm": { "type": "number", "default": 18000.0 },
                        "inlet_air_temperature_c": { "type": "number", "default": 200.0 }
                    },
                    "required": ["feed_rate_kg_h", "feed_solids_fraction"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_thermoelectric_rtg".into(),
                description: "Calculates Pu-238 alpha decay heat, thermoelectric figure of merit ZT, Seebeck open-circuit voltage, matched load electrical power, and 15-year deep space mission power degradation profile.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "initial_thermal_power_w_th": { "type": "number", "default": 4400.0 },
                        "hot_junction_temp_k": { "type": "number", "default": 1273.15 },
                        "cold_junction_temp_k": { "type": "number", "default": 566.15 },
                        "couple_count": { "type": "integer", "default": 572 }
                    },
                    "required": ["initial_thermal_power_w_th", "couple_count"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_nuclear_containment_loca".into(),
                description: "Calculates post-LOCA mass and energy discharge, Dalton air-steam partial pressures, Tagami and Uchida passive heat sink condensation coefficients, and active spray depressurization margins.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "containment_free_volume_m3": { "type": "number", "default": 65000.0 },
                        "design_pressure_bar_a": { "type": "number", "default": 4.80 },
                        "blowdown_enthalpy_gj": { "type": "number", "default": 380.0 },
                        "spray_flow_rate_l_s": { "type": "number", "default": 650.0 }
                    },
                    "required": ["containment_free_volume_m3", "design_pressure_bar_a"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_environmental_atmospheric_dispersion".into(),
                description: "Calculates thermal buoyancy and momentum plume rise, Pasquill-Gifford dispersion coefficients, ground-level centerline concentrations, and US EPA NAAQS ambient limit compliance.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "emission_rate_g_s": { "type": "number", "default": 85.0 },
                        "stack_height_m": { "type": "number", "default": 80.0 },
                        "stack_gas_temperature_c": { "type": "number", "default": 140.0 },
                        "wind_speed_at_stack_height_m_s": { "type": "number", "default": 5.50 }
                    },
                    "required": ["emission_rate_g_s", "stack_height_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_rocket_ascent_trajectory".into(),
                description: "Calculates multistage Tsiolkovsky staging delta-V, gravity/aerodynamic/steering losses, Max Q dynamic pressure, Earth rotation assist, and circular LEO injection velocity.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "gross_liftoff_mass_kg": { "type": "number", "default": 549054.0 },
                        "payload_mass_kg": { "type": "number", "default": 15600.0 },
                        "stage1_thrust_sl_kn": { "type": "number", "default": 7607.0 },
                        "target_orbit_altitude_km": { "type": "number", "default": 300.0 }
                    },
                    "required": ["gross_liftoff_mass_kg", "payload_mass_kg"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_gravity_dam_stability".into(),
                description: "Calculates hydrostatic water thrust, USACE trapezoidal foundation drainage seepage uplift, Westergaard seismic hydrodynamic forces, shear-friction sliding SFF, and middle-third Kern bearing stresses.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "dam_height_m": { "type": "number", "default": 85.0 },
                        "crest_width_m": { "type": "number", "default": 8.50 },
                        "base_width_m": { "type": "number", "default": 68.0 },
                        "upstream_water_depth_m": { "type": "number", "default": 80.0 }
                    },
                    "required": ["dam_height_m", "base_width_m", "upstream_water_depth_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_electrolyzer_hydrogen".into(),
                description: "Calculates Nernst reversible potential, thermoneutral voltage, anode/cathode Butler-Volmer activation overpotentials, membrane protonic ohmic losses, Faraday efficiency, and specific energy consumption.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "cell_temperature_c": { "type": "number", "default": 70.0 },
                        "hydrogen_operating_pressure_bar_a": { "type": "number", "default": 30.0 },
                        "current_density_a_per_cm2": { "type": "number", "default": 1.80 },
                        "cell_count": { "type": "integer", "default": 120 }
                    },
                    "required": ["current_density_a_per_cm2", "cell_count"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_optical_thin_film_optics".into(),
                description: "Calculates multi-layer dielectric phase thickness, transfer matrix characteristic admittances, TE/TM Fresnel reflectance, and DBR stopband bandwidth.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wavelength_nm": { "type": "number", "default": 632.8 },
                        "angle_of_incidence_deg": { "type": "number", "default": 0.0 },
                        "substrate_index": { "type": "number", "default": 1.52 }
                    },
                    "required": ["wavelength_nm", "substrate_index"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_energy_cryogenic_liquefaction".into(),
                description: "Calculates real-gas Joule-Thomson enthalpy departures, recuperator heat exchanger balance, cryogenic liquid yield fraction, compression power, and specific energy consumption.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "feed_gas_flow_rate_kg_h": { "type": "number", "default": 1200.0 },
                        "suction_pressure_bar_a": { "type": "number", "default": 1.05 },
                        "discharge_pressure_bar_a": { "type": "number", "default": 200.0 }
                    },
                    "required": ["feed_gas_flow_rate_kg_h", "discharge_pressure_bar_a"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_process_hydrocyclone_separation".into(),
                description: "Calculates centrifugal slurry pressure drop, corrected cut size d50c, water split bypass to underflow, Rosin-Rammler partition sharpness, and spigot spray regime.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "cyclone_diameter_mm": { "type": "number", "default": 350.0 },
                        "feed_flow_rate_m3_h": { "type": "number", "default": 120.0 },
                        "solids_volumetric_concentration_pct": { "type": "number", "default": 18.0 }
                    },
                    "required": ["cyclone_diameter_mm", "feed_flow_rate_m3_h"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mechanical_magnetic_bearing".into(),
                description: "Calculates electromagnetic actuator current gain, open-loop negative stiffness, closed-loop PID suspension stiffness and damping, natural frequency, and ISO 14839 orbit clearance.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rotor_supported_mass_kg": { "type": "number", "default": 85.0 },
                        "nominal_air_gap_mm": { "type": "number", "default": 0.60 },
                        "max_rotational_speed_rpm": { "type": "number", "default": 36000.0 }
                    },
                    "required": ["rotor_supported_mass_kg", "nominal_air_gap_mm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_sonar_propagation".into(),
                description: "Calculates transducer source level, Thorpe seawater absorption, spherical/cylindrical waveguide transmission loss, Wenz ambient noise, active sonar SNR, and target detection.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "operating_frequency_khz": { "type": "number", "default": 7.5 },
                        "transmitter_acoustic_power_w": { "type": "number", "default": 4500.0 },
                        "target_range_m": { "type": "number", "default": 8500.0 }
                    },
                    "required": ["operating_frequency_khz", "target_range_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_manufacturing_electron_beam_welding".into(),
                description: "Calculates Goldak double-ellipsoid moving volumetric heat flux, vacuum keyhole penetration depth, cooling time t8/5, and ISO 13919-1 Class B quality compliance.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "acceleration_voltage_kv": { "type": "number", "default": 120.0 },
                        "beam_current_ma": { "type": "number", "default": 50.0 },
                        "travel_speed_mm_s": { "type": "number", "default": 15.0 }
                    },
                    "required": ["acceleration_voltage_kv", "beam_current_ma"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_process_crystallization_kinetics".into(),
                description: "Calculates supersaturation driving force, crystal growth and nucleation rates, MSMPR population density distribution, median size L50, and magma density.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "feed_volumetric_flow_m3_h": { "type": "number", "default": 25.0 },
                        "crystallizer_active_volume_m3": { "type": "number", "default": 50.0 },
                        "target_median_size_um": { "type": "number", "default": 350.0 }
                    },
                    "required": ["feed_volumetric_flow_m3_h", "crystallizer_active_volume_m3"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_thermal_thermoelectric_cooler".into(),
                description: "Calculates coupled Peltier-Seebeck-Joule cooling capacity, input power, COP, maximum temperature lift Delta T_max, and optimal drive currents.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "couple_count_n": { "type": "integer", "default": 127 },
                        "operating_current_a": { "type": "number", "default": 4.5 },
                        "heat_load_qc_target_w": { "type": "number", "default": 35.0 }
                    },
                    "required": ["couple_count_n", "operating_current_a"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mechanical_slurry_pump_derating".into(),
                description: "Calculates slurry mixture specific gravity, ANSI/HI 12.1-12.6 head de-rating ratio HR, shaft brake power, impeller tip peripheral speed, and abrasive wear life.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "slurry_flow_rate_m3_h": { "type": "number", "default": 650.0 },
                        "clear_water_head_m": { "type": "number", "default": 45.0 },
                        "solids_concentration_wt_pct": { "type": "number", "default": 35.0 }
                    },
                    "required": ["slurry_flow_rate_m3_h", "clear_water_head_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_optical_fiber_dispersion".into(),
                description: "Calculates Sellmeier refractive index, ITU-T G.652 chromatic dispersion parameter D(lambda), link accumulated dispersion, Gaussian pulse temporal broadening, and bit-rate limits.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wavelength_nm": { "type": "number", "default": 1550.0 },
                        "fiber_length_km": { "type": "number", "default": 80.0 },
                        "target_bit_rate_gbps": { "type": "number", "default": 10.0 }
                    },
                    "required": ["wavelength_nm", "fiber_length_km"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_propeller_hydrodynamics".into(),
                description: "Calculates Wageningen B-series thrust coefficient KT, torque coefficient KQ, open-water efficiency eta_0, delivered power, Keller cavitation blade area ratio, and cavitation margin.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "propeller_diameter_m": { "type": "number", "default": 5.60 },
                        "pitch_diameter_ratio": { "type": "number", "default": 0.95 },
                        "rotational_speed_rpm": { "type": "number", "default": 105.0 }
                    },
                    "required": ["propeller_diameter_m", "pitch_diameter_ratio"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_environmental_indoor_air_quality".into(),
                description: "Calculates room effective removal rate lambda, steady-state concentration Css, transient concentration trajectory C(t), Wells-Riley infection risk, and ASHRAE 62.1 outdoor air rate.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "room_floor_area_m2": { "type": "number", "default": 120.0 },
                        "room_clear_height_m": { "type": "number", "default": 3.0 },
                        "ventilation_supply_airflow_m3_h": { "type": "number", "default": 720.0 }
                    },
                    "required": ["room_floor_area_m2", "ventilation_supply_airflow_m3_h"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electrical_pmsm_foc_drive".into(),
                description: "Calculates d-q reference frame voltages, MTPA optimal current angle, electromagnetic torque, copper and core iron losses, electrical efficiency, and inverter SVPWM modulation index.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rotational_speed_rpm": { "type": "number", "default": 4500.0 },
                        "q_axis_current_iq_a": { "type": "number", "default": 120.0 },
                        "inverter_dc_bus_voltage_v": { "type": "number", "default": 650.0 }
                    },
                    "required": ["rotational_speed_rpm", "q_axis_current_iq_a"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_civil_mse_retaining_wall".into(),
                description: "Calculates Rankine active lateral earth pressure, maximum tensile force per geogrid layer, long-term design strength Tal, tensile rupture safety factor, and pullout resistance.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "wall_height_h_m": { "type": "number", "default": 8.0 },
                        "reinforcement_length_l_m": { "type": "number", "default": 6.0 },
                        "geogrid_ultimate_strength_kn_m": { "type": "number", "default": 110.0 }
                    },
                    "required": ["wall_height_h_m", "reinforcement_length_l_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_nuclear_isotope_centrifuge".into(),
                description: "Calculates rotor peripheral speed, centrifugal g-force acceleration, elementary separation factor alpha0, Dirac-Peierls value function, and stage separative power delta_U (kg SWU/yr).".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "rotor_radius_m": { "type": "number", "default": 0.10 },
                        "rotor_length_m": { "type": "number", "default": 3.0 },
                        "rotational_speed_rpm": { "type": "number", "default": 65000.0 }
                    },
                    "required": ["rotor_radius_m", "rotational_speed_rpm"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_mechanical_conveyor_bulk_handling".into(),
                description: "Calculates troughed idler cross-sectional area, mass throughput capacity (t/h), motion resistances, Euler-Eytelwein capstan drive slip safety, and motor brake power.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "belt_width_mm": { "type": "number", "default": 1200.0 },
                        "conveyor_length_m": { "type": "number", "default": 450.0 },
                        "belt_speed_m_s": { "type": "number", "default": 3.15 }
                    },
                    "required": ["belt_width_mm", "conveyor_length_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_marine_offshore_wave_loading".into(),
                description: "Calculates Airy dispersion wavelength, orbital kinematics, Morison hydrodynamic drag and inertia, mudline base shear, overturning moment, and bending stress.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "water_depth_m": { "type": "number", "default": 40.0 },
                        "wave_height_m": { "type": "number", "default": 14.5 },
                        "cylinder_diameter_m": { "type": "number", "default": 8.50 }
                    },
                    "required": ["water_depth_m", "wave_height_m"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_aerospace_spacecraft_adcs_attitude".into(),
                description: "Calculates gravity gradient, solar radiation, aerodynamic and magnetic disturbance torques, orbital accumulated angular momentum, reaction wheel capacity, and MTQ sizing.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "orbit_altitude_km": { "type": "number", "default": 550.0 },
                        "spacecraft_mass_kg": { "type": "number", "default": 350.0 },
                        "target_slew_accel_deg_s2": { "type": "number", "default": 1.50 }
                    },
                    "required": ["orbit_altitude_km", "spacecraft_mass_kg"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_electrical_battery_fast_charging".into(),
                description: "Calculates CC and CV phase durations, total fast-charge time, peak heat dissipation, SEI parasitic layer growth, capacity loss per cycle, and lithium plating avoidance margin.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "nominal_capacity_ah": { "type": "number", "default": 75.0 },
                        "charge_c_rate": { "type": "number", "default": 2.50 },
                        "internal_resistance_mohms": { "type": "number", "default": 0.85 }
                    },
                    "required": ["nominal_capacity_ah", "charge_c_rate"]
                }),
            },
            McpToolDefinition {
                name: "atlas_sim_process_packed_bed_scrubber".into(),
                description: "Calculates column diameter at 70% Sherwood flooding velocity, Colburn NTU_OG, Onda HTU_OG, active bed height, gas pressure drop, and pollutant removal efficiency.".into(),
                input_schema: serde_json::json!({
                    "type": "object",
                    "properties": {
                        "flue_gas_flow_rate_m3_h": { "type": "number", "default": 8500.0 },
                        "inlet_pollutant_mole_fraction": { "type": "number", "default": 0.0120 },
                        "target_outlet_mole_fraction": { "type": "number", "default": 0.00024 }
                    },
                    "required": ["flue_gas_flow_rate_m3_h", "inlet_pollutant_mole_fraction"]
                }),
            },
        ]
    }

    pub fn handle_rpc(request: &McpRequest) -> Result<McpResponse, OrchestratorError> {
        let req_id = request.id.clone();
        match request.method.as_str() {
            "initialize" => Ok(McpResponse {
                jsonrpc: "2.0".into(),
                id: req_id,
                result: Some(serde_json::json!({
                    "protocolVersion": "2024-11-05",
                    "serverInfo": {
                        "name": "atlascad-mcp",
                        "version": "0.1.0"
                    },
                    "capabilities": {
                        "tools": { "listChanged": false },
                        "resources": { "subscribe": false, "listChanged": false }
                    }
                })),
                error: None,
            }),
            "tools/list" => Ok(McpResponse {
                jsonrpc: "2.0".into(),
                id: req_id,
                result: Some(serde_json::json!({
                    "tools": Self::get_tools()
                })),
                error: None,
            }),
            "resources/list" => Ok(McpResponse {
                jsonrpc: "2.0".into(),
                id: req_id,
                result: Some(serde_json::json!({
                    "resources": [
                        { "uri": "atlas://project/active", "name": "Active CAD Project", "mimeType": "application/json" },
                        { "uri": "atlas://simulation/report", "name": "Latest Multiphysics Report", "mimeType": "application/json" },
                        { "uri": "atlas://drawing/plan", "name": "2D Engineering Drawing DXF", "mimeType": "application/dxf" }
                    ]
                })),
                error: None,
            }),
            "tools/call" => {
                let tool_name = request
                    .params
                    .get("name")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let args = request
                    .params
                    .get("arguments")
                    .cloned()
                    .unwrap_or(serde_json::json!({}));

                // Format tool response
                let content_text =
                    format!("AtlasCAD Tool '{tool_name}' executed successfully with args: {args}");
                Ok(McpResponse {
                    jsonrpc: "2.0".into(),
                    id: req_id,
                    result: Some(serde_json::json!({
                        "content": [
                            {
                                "type": "text",
                                "text": content_text
                            }
                        ],
                        "isError": false
                    })),
                    error: None,
                })
            }
            other => Ok(McpResponse {
                jsonrpc: "2.0".into(),
                id: req_id,
                result: None,
                error: Some(McpErrorPayload {
                    code: -32601,
                    message: format!("Method not found: '{other}'"),
                }),
            }),
        }
    }
}

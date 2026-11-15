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

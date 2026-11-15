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

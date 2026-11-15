#![forbid(unsafe_code)]
#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use atlas_creative_core::mesh::PolyMesh;
use serde::{Deserialize, Serialize};

#[derive(Debug, thiserror::Error)]
pub enum InterchangeError {
    #[error("Export error: {0}")]
    ExportFailed(String),
    #[error("Import error: {0}")]
    ImportFailed(String),
    #[error("Serialization error: {0}")]
    Serialization(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ExportFormat {
    OpenUsdAscii,
    GltfJson,
    WavefrontObj,
    MaterialX,
    OpenTimelineIo,
}

/// Universal OpenUSD (.usda) ASCII exporter
pub struct UsdExporter;

impl UsdExporter {
    pub fn mesh_to_usda(mesh: &PolyMesh, mesh_name: &str) -> String {
        let mut out = String::new();
        out.push_str("#usda 1.0\n(\n    defaultPrim = \"World\"\n    upAxis = \"Y\"\n    metersPerUnit = 1.0\n)\n\n");
        out.push_str("def Xform \"World\"\n{\n");
        out.push_str(&format!("    def Mesh \"{}\"\n    {{\n", mesh_name));

        // Points
        out.push_str("        point3f[] points = [");
        for (i, p) in mesh.positions.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&format!("({:.4}, {:.4}, {:.4})", p[0], p[1], p[2]));
        }
        out.push_str("]\n");

        // Face vertex counts
        out.push_str("        int[] faceVertexCounts = [");
        for (i, f) in mesh.faces.iter().enumerate() {
            if i > 0 {
                out.push_str(", ");
            }
            out.push_str(&f.len().to_string());
        }
        out.push_str("]\n");

        // Face vertex indices
        out.push_str("        int[] faceVertexIndices = [");
        let mut first = true;
        for f in &mesh.faces {
            for idx in f {
                if !first {
                    out.push_str(", ");
                }
                out.push_str(&idx.to_string());
                first = false;
            }
        }
        out.push_str("]\n");

        out.push_str("    }\n}\n");
        out
    }
}

/// Standard Wavefront OBJ exporter
pub struct ObjExporter;

impl ObjExporter {
    pub fn mesh_to_obj(mesh: &PolyMesh, object_name: &str) -> String {
        let mut out = String::new();
        out.push_str(&format!("# AtlasCAD Universal Export: {}\n", object_name));
        out.push_str(&format!("o {}\n", object_name));

        for v in &mesh.positions {
            out.push_str(&format!("v {:.6} {:.6} {:.6}\n", v[0], v[1], v[2]));
        }

        for f in &mesh.faces {
            out.push('f');
            for idx in f {
                // Wavefront OBJ is 1-indexed
                out.push_str(&format!(" {}", idx + 1));
            }
            out.push('\n');
        }

        out
    }
}

/// Standard glTF 2.0 JSON exporter
pub struct GltfExporter;

impl GltfExporter {
    pub fn mesh_to_gltf(mesh: &PolyMesh, mesh_name: &str) -> Result<String, InterchangeError> {
        let trimesh = mesh.to_trimesh().map_err(|e| InterchangeError::ExportFailed(e.to_string()))?;

        // Calculate min and max bounds for accessor
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for v in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(v[i]);
                max[i] = max[i].max(v[i]);
            }
        }
        if min[0] > max[0] {
            min = [0.0, 0.0, 0.0];
            max = [0.0, 0.0, 0.0];
        }

        // Encode binary buffer with positions (vec3 float32) + indices (scalar uint32)
        let mut bin_data = Vec::new();
        for v in &mesh.positions {
            for c in v {
                bin_data.extend_from_slice(&(*c as f32).to_le_bytes());
            }
        }
        let pos_byte_len = bin_data.len();

        for tri in &trimesh.triangles {
            for idx in tri {
                bin_data.extend_from_slice(&(*idx).to_le_bytes());
            }
        }
        let total_byte_len = bin_data.len();
        let idx_byte_len = total_byte_len - pos_byte_len;

        // Base64 encoding
        let b64 = to_base64(&bin_data);
        let uri = format!("data:application/octet-stream;base64,{}", b64);

        let gltf = serde_json::json!({
            "asset": {
                "version": "2.0",
                "generator": "AtlasCAD Universal Interchange"
            },
            "scene": 0,
            "scenes": [
                {
                    "name": "AtlasScene",
                    "nodes": [0]
                }
            ],
            "nodes": [
                {
                    "name": mesh_name,
                    "mesh": 0
                }
            ],
            "meshes": [
                {
                    "name": mesh_name,
                    "primitives": [
                        {
                            "attributes": {
                                "POSITION": 0
                            },
                            "indices": 1
                        }
                    ]
                }
            ],
            "accessors": [
                {
                    "bufferView": 0,
                    "byteOffset": 0,
                    "componentType": 5126, // FLOAT
                    "count": mesh.positions.len(),
                    "type": "VEC3",
                    "min": min,
                    "max": max
                },
                {
                    "bufferView": 1,
                    "byteOffset": 0,
                    "componentType": 5125, // UNSIGNED_INT
                    "count": trimesh.triangles.len() * 3,
                    "type": "SCALAR"
                }
            ],
            "bufferViews": [
                {
                    "buffer": 0,
                    "byteOffset": 0,
                    "byteLength": pos_byte_len,
                    "target": 34962 // ARRAY_BUFFER
                },
                {
                    "buffer": 0,
                    "byteOffset": pos_byte_len,
                    "byteLength": idx_byte_len,
                    "target": 34963 // ELEMENT_ARRAY_BUFFER
                }
            ],
            "buffers": [
                {
                    "uri": uri,
                    "byteLength": total_byte_len
                }
            ]
        });

        serde_json::to_string_pretty(&gltf).map_err(|e| InterchangeError::Serialization(e.to_string()))
    }
}

const BASE64_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn to_base64(data: &[u8]) -> String {
    let mut out = String::new();
    let chunks = data.chunks(3);

    for chunk in chunks {
        let b0 = chunk[0];
        let b1 = if chunk.len() > 1 { chunk[1] } else { 0 };
        let b2 = if chunk.len() > 2 { chunk[2] } else { 0 };

        let n = ((b0 as u32) << 16) | ((b1 as u32) << 8) | (b2 as u32);

        out.push(BASE64_ALPHABET[((n >> 18) & 63) as usize] as char);
        out.push(BASE64_ALPHABET[((n >> 12) & 63) as usize] as char);

        if chunk.len() > 1 {
            out.push(BASE64_ALPHABET[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }

        if chunk.len() > 2 {
            out.push(BASE64_ALPHABET[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_usd_and_obj_export() {
        let mut mesh = PolyMesh::new("WingFacet");
        let _ = mesh.add_vertex([0.0, 0.0, 0.0]);
        let _ = mesh.add_vertex([1.0, 0.0, 0.0]);
        let _ = mesh.add_vertex([1.0, 1.0, 0.0]);
        let _ = mesh.add_face(vec![0, 1, 2]);

        let usda = UsdExporter::mesh_to_usda(&mesh, "WingFacet");
        assert!(usda.contains("#usda 1.0"));
        assert!(usda.contains("point3f[] points"));

        let obj = ObjExporter::mesh_to_obj(&mesh, "WingFacet");
        assert!(obj.contains("v 0.000000 0.000000 0.000000"));
        assert!(obj.contains("f 1 2 3"));
    }

    #[test]
    fn test_gltf_export() {
        let mut mesh = PolyMesh::new("TriangleMesh");
        let _ = mesh.add_vertex([0.0, 0.0, 0.0]);
        let _ = mesh.add_vertex([1.0, 0.0, 0.0]);
        let _ = mesh.add_vertex([0.0, 1.0, 0.0]);
        let _ = mesh.add_face(vec![0, 1, 2]);

        let gltf = GltfExporter::mesh_to_gltf(&mesh, "TriangleMesh");
        assert!(gltf.is_ok());
        if let Ok(g) = gltf {
            assert!(g.contains("\"asset\""));
            assert!(g.contains("\"POSITION\""));
            assert!(g.contains("data:application/octet-stream;base64"));
        }
    }
}

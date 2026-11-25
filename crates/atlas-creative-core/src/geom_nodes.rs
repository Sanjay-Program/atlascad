use crate::CreativeError;
use crate::mesh::PolyMesh;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GeometryNode {
    Transform {
        translation: [f64; 3],
        scale: [f64; 3],
    },
    Subdivide {
        levels: u32,
    },
    NoiseDisplace {
        amplitude: f64,
        frequency: f64,
    },
    ExtrudeFaces {
        distance: f64,
    },
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct GeometryGraph {
    pub name: String,
    pub nodes: Vec<GeometryNode>,
}

impl GeometryGraph {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            nodes: Vec::new(),
        }
    }

    pub fn add_node(&mut self, node: GeometryNode) -> &mut Self {
        self.nodes.push(node);
        self
    }

    pub fn execute(&self, input: &PolyMesh) -> Result<PolyMesh, CreativeError> {
        let mut mesh = input.clone();
        for node in &self.nodes {
            match node {
                GeometryNode::Transform { translation, scale } => {
                    for pos in &mut mesh.positions {
                        pos[0] = pos[0] * scale[0] + translation[0];
                        pos[1] = pos[1] * scale[1] + translation[1];
                        pos[2] = pos[2] * scale[2] + translation[2];
                    }
                    mesh.recompute_normals();
                }
                GeometryNode::Subdivide { levels } => {
                    mesh = mesh.subdivide_catmull_clark(*levels)?;
                }
                GeometryNode::NoiseDisplace {
                    amplitude,
                    frequency,
                } => {
                    for (pos, norm) in mesh.positions.iter_mut().zip(&mesh.normals) {
                        let val = ((pos[0] * frequency).sin()
                            + (pos[1] * frequency).cos()
                            + (pos[2] * frequency).sin())
                            * amplitude;
                        pos[0] += norm[0] * val;
                        pos[1] += norm[1] * val;
                        pos[2] += norm[2] * val;
                    }
                    mesh.recompute_normals();
                }
                GeometryNode::ExtrudeFaces { distance } => {
                    let mut new_positions = mesh.positions.clone();
                    let mut new_faces = Vec::new();

                    for face in &mesh.faces {
                        if face.len() < 3 {
                            continue;
                        }
                        // Compute face normal
                        let mut sum_n = [0.0, 0.0, 0.0];
                        for &v_idx in face {
                            if let Some(norm) = mesh.normals.get(v_idx as usize) {
                                sum_n[0] += norm[0];
                                sum_n[1] += norm[1];
                                sum_n[2] += norm[2];
                            }
                        }
                        let len = (sum_n[0] * sum_n[0] + sum_n[1] * sum_n[1] + sum_n[2] * sum_n[2])
                            .sqrt()
                            .max(1e-6);
                        let fnorm = [sum_n[0] / len, sum_n[1] / len, sum_n[2] / len];

                        let mut extruded_v_indices = Vec::with_capacity(face.len());
                        for &v_idx in face {
                            let p = mesh.positions[v_idx as usize];
                            let new_p = [
                                p[0] + fnorm[0] * distance,
                                p[1] + fnorm[1] * distance,
                                p[2] + fnorm[2] * distance,
                            ];
                            let new_v_idx = u32::try_from(new_positions.len()).map_err(|_| {
                                CreativeError::GeometryNode("too many vertices".into())
                            })?;
                            new_positions.push(new_p);
                            extruded_v_indices.push(new_v_idx);
                        }

                        // Top cap
                        new_faces.push(extruded_v_indices.clone());

                        // Side quads
                        let f_len = face.len();
                        for i in 0..f_len {
                            let b1 = face[i];
                            let b2 = face[(i + 1) % f_len];
                            let t2 = extruded_v_indices[(i + 1) % f_len];
                            let t1 = extruded_v_indices[i];
                            new_faces.push(vec![b1, b2, t2, t1]);
                        }
                    }

                    mesh.positions = new_positions;
                    mesh.faces = new_faces;
                    mesh.recompute_normals();
                }
            }
        }
        Ok(mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_geometry_nodes_pipeline() {
        let cube = PolyMesh::cube(1.0);
        let mut graph = GeometryGraph::new("ProceduralColumn");
        graph
            .add_node(GeometryNode::Transform {
                translation: [0.0, 0.0, 1.0],
                scale: [1.0, 1.0, 2.0],
            })
            .add_node(GeometryNode::Subdivide { levels: 1 });

        let result = graph.execute(&cube);
        assert!(result.is_ok());
        if let Ok(res) = result {
            assert_eq!(res.face_count(), 24);
            assert!(res.positions[0][2] > 0.0);
        }
    }
}

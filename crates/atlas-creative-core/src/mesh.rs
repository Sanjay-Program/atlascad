use crate::CreativeError;
use cad_contract::mesh::Mesh;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PolyMesh {
    pub name: String,
    pub positions: Vec<[f64; 3]>,
    pub normals: Vec<[f64; 3]>,
    pub uvs: Vec<[f64; 2]>,
    pub faces: Vec<Vec<u32>>,
}

impl Default for PolyMesh {
    fn default() -> Self {
        Self::new("Untitled_Mesh")
    }
}

impl PolyMesh {
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            positions: Vec::new(),
            normals: Vec::new(),
            uvs: Vec::new(),
            faces: Vec::new(),
        }
    }

    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    pub fn face_count(&self) -> usize {
        self.faces.len()
    }

    pub fn add_vertex(&mut self, pos: [f64; 3]) -> u32 {
        let idx = self.positions.len() as u32;
        self.positions.push(pos);
        self.normals.push([0.0, 1.0, 0.0]);
        idx
    }

    pub fn add_face(&mut self, face: Vec<u32>) -> usize {
        let idx = self.faces.len();
        self.faces.push(face);
        idx
    }

    pub fn from_trimesh(mesh: &Mesh, name: impl Into<String>) -> Self {
        let faces = mesh
            .triangles
            .iter()
            .map(|t| vec![t[0], t[1], t[2]])
            .collect();
        let mut poly = Self {
            name: name.into(),
            positions: mesh.positions.clone(),
            normals: mesh.normals.clone(),
            uvs: Vec::new(),
            faces,
        };
        if poly.normals.is_empty() {
            poly.recompute_normals();
        }
        poly
    }

    pub fn to_trimesh(&self) -> Result<Mesh, CreativeError> {
        let mut triangles = Vec::new();
        for face in &self.faces {
            if face.len() < 3 {
                continue;
            }
            // Fan triangulation for quads and n-gons
            for i in 1..(face.len() - 1) {
                let a = *face
                    .first()
                    .ok_or_else(|| CreativeError::Mesh("missing face index".into()))?;
                let b = *face
                    .get(i)
                    .ok_or_else(|| CreativeError::Mesh("missing face index".into()))?;
                let c = *face
                    .get(i + 1)
                    .ok_or_else(|| CreativeError::Mesh("missing face index".into()))?;
                triangles.push([a, b, c]);
            }
        }
        let mesh = Mesh {
            positions: self.positions.clone(),
            triangles,
            normals: self.normals.clone(),
        };
        mesh.validate()
            .map_err(|e| CreativeError::Mesh(e.to_string()))?;
        Ok(mesh)
    }

    pub fn recompute_normals(&mut self) {
        let mut normals = vec![[0.0, 0.0, 0.0]; self.positions.len()];
        for face in &self.faces {
            if face.len() < 3 {
                continue;
            }
            for i in 1..(face.len() - 1) {
                let (Some(&ai), Some(&bi), Some(&ci)) =
                    (face.first(), face.get(i), face.get(i + 1))
                else {
                    continue;
                };
                let (Some(&a), Some(&b), Some(&c)) = (
                    self.positions.get(ai as usize),
                    self.positions.get(bi as usize),
                    self.positions.get(ci as usize),
                ) else {
                    continue;
                };
                let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let n = [
                    u[1] * v[2] - u[2] * v[1],
                    u[2] * v[0] - u[0] * v[2],
                    u[0] * v[1] - u[1] * v[0],
                ];
                for &idx in &[ai, bi, ci] {
                    if let Some(slot) = normals.get_mut(idx as usize) {
                        slot[0] += n[0];
                        slot[1] += n[1];
                        slot[2] += n[2];
                    }
                }
            }
        }
        for n in &mut normals {
            let len = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
            if len > 1e-12 {
                n[0] /= len;
                n[1] /= len;
                n[2] /= len;
            } else {
                *n = [0.0, 0.0, 1.0];
            }
        }
        self.normals = normals;
    }

    pub fn subdivide_catmull_clark(&self, iterations: u32) -> Result<PolyMesh, CreativeError> {
        let mut current = self.clone();
        for _ in 0..iterations {
            current = current.subdivide_once()?;
        }
        Ok(current)
    }

    fn subdivide_once(&self) -> Result<PolyMesh, CreativeError> {
        // 1. Compute face points (centroids)
        let mut face_points: Vec<[f64; 3]> = Vec::with_capacity(self.faces.len());
        for face in &self.faces {
            if face.is_empty() {
                return Err(CreativeError::Mesh("face has no vertices".into()));
            }
            let mut sum = [0.0, 0.0, 0.0];
            for &idx in face {
                let p = self.positions.get(idx as usize).ok_or_else(|| {
                    CreativeError::Mesh(format!("vertex index {idx} out of range"))
                })?;
                sum[0] += p[0];
                sum[1] += p[1];
                sum[2] += p[2];
            }
            let n = face.len() as f64;
            face_points.push([sum[0] / n, sum[1] / n, sum[2] / n]);
        }

        // 2. Identify unique edges and associated faces
        let mut edge_faces: HashMap<(u32, u32), Vec<usize>> = HashMap::new();
        for (f_idx, face) in self.faces.iter().enumerate() {
            let len = face.len();
            for i in 0..len {
                let a = face[i];
                let b = face[(i + 1) % len];
                let key = if a < b { (a, b) } else { (b, a) };
                edge_faces.entry(key).or_default().push(f_idx);
            }
        }

        // 3. Compute edge points
        let mut edge_to_point_idx: HashMap<(u32, u32), u32> = HashMap::new();
        let mut new_positions = self.positions.clone();
        let orig_vert_count = self.positions.len();

        for (&(a, b), f_list) in &edge_faces {
            let p_a = *self
                .positions
                .get(a as usize)
                .ok_or_else(|| CreativeError::Mesh("edge vertex a out of bounds".into()))?;
            let p_b = *self
                .positions
                .get(b as usize)
                .ok_or_else(|| CreativeError::Mesh("edge vertex b out of bounds".into()))?;

            let edge_pt = if f_list.len() == 2 {
                let fp1 = face_points[f_list[0]];
                let fp2 = face_points[f_list[1]];
                [
                    (p_a[0] + p_b[0] + fp1[0] + fp2[0]) / 4.0,
                    (p_a[1] + p_b[1] + fp1[1] + fp2[1]) / 4.0,
                    (p_a[2] + p_b[2] + fp1[2] + fp2[2]) / 4.0,
                ]
            } else {
                [
                    (p_a[0] + p_b[0]) / 2.0,
                    (p_a[1] + p_b[1]) / 2.0,
                    (p_a[2] + p_b[2]) / 2.0,
                ]
            };

            let pt_idx = u32::try_from(new_positions.len()).map_err(|_| {
                CreativeError::Mesh("subdivision exceeded vertex index limit".into())
            })?;
            edge_to_point_idx.insert((a, b), pt_idx);
            new_positions.push(edge_pt);
        }

        // Append face points
        let face_point_base = u32::try_from(new_positions.len())
            .map_err(|_| CreativeError::Mesh("vertex index overflow".into()))?;
        new_positions.extend_from_slice(&face_points);

        // 4. Update original vertices
        for (v_idx, pos) in new_positions.iter_mut().enumerate().take(orig_vert_count) {
            let mut adj_faces = Vec::new();
            let mut adj_edges = Vec::new();

            for (&(a, b), f_list) in &edge_faces {
                if a == v_idx as u32 || b == v_idx as u32 {
                    let other = if a == v_idx as u32 { b } else { a };
                    adj_edges.push(other);
                    for &f in f_list {
                        if !adj_faces.contains(&f) {
                            adj_faces.push(f);
                        }
                    }
                }
            }

            let n = adj_edges.len() as f64;
            if n >= 3.0 && adj_faces.len() == adj_edges.len() {
                let mut f_sum = [0.0, 0.0, 0.0];
                for &f in &adj_faces {
                    let fp = face_points[f];
                    f_sum[0] += fp[0];
                    f_sum[1] += fp[1];
                    f_sum[2] += fp[2];
                }
                let f_avg = [f_sum[0] / n, f_sum[1] / n, f_sum[2] / n];

                let mut r_sum = [0.0, 0.0, 0.0];
                let v_pos = self.positions[v_idx];
                for &other in &adj_edges {
                    let op = self.positions[other as usize];
                    r_sum[0] += (v_pos[0] + op[0]) / 2.0;
                    r_sum[1] += (v_pos[1] + op[1]) / 2.0;
                    r_sum[2] += (v_pos[2] + op[2]) / 2.0;
                }
                let r_avg = [r_sum[0] / n, r_sum[1] / n, r_sum[2] / n];

                *pos = [
                    (f_avg[0] + 2.0 * r_avg[0] + (n - 3.0) * v_pos[0]) / n,
                    (f_avg[1] + 2.0 * r_avg[1] + (n - 3.0) * v_pos[1]) / n,
                    (f_avg[2] + 2.0 * r_avg[2] + (n - 3.0) * v_pos[2]) / n,
                ];
            }
        }

        // 5. Build new quad faces
        let mut new_faces = Vec::new();
        for (f_idx, face) in self.faces.iter().enumerate() {
            let face_pt_idx = face_point_base + (f_idx as u32);
            let len = face.len();
            for i in 0..len {
                let v = face[i];
                let v_next = face[(i + 1) % len];
                let v_prev = face[(i + len - 1) % len];

                let e_next_key = if v < v_next { (v, v_next) } else { (v_next, v) };
                let e_prev_key = if v_prev < v { (v_prev, v) } else { (v, v_prev) };

                let edge_pt_next = *edge_to_point_idx
                    .get(&e_next_key)
                    .ok_or_else(|| CreativeError::Mesh("missing edge point".into()))?;
                let edge_pt_prev = *edge_to_point_idx
                    .get(&e_prev_key)
                    .ok_or_else(|| CreativeError::Mesh("missing edge point".into()))?;

                new_faces.push(vec![v, edge_pt_next, face_pt_idx, edge_pt_prev]);
            }
        }

        let mut result = PolyMesh {
            name: format!("{}_subdiv", self.name),
            positions: new_positions,
            normals: Vec::new(),
            uvs: Vec::new(),
            faces: new_faces,
        };
        result.recompute_normals();
        Ok(result)
    }

    pub fn cube(size: f64) -> Self {
        let h = size / 2.0;
        let positions = vec![
            [-h, -h, -h],
            [h, -h, -h],
            [h, h, -h],
            [-h, h, -h],
            [-h, -h, h],
            [h, -h, h],
            [h, h, h],
            [-h, h, h],
        ];
        let faces = vec![
            vec![0, 3, 2, 1], // Bottom
            vec![4, 5, 6, 7], // Top
            vec![0, 1, 5, 4], // Front
            vec![2, 3, 7, 6], // Back
            vec![0, 4, 7, 3], // Left
            vec![1, 2, 6, 5], // Right
        ];
        let mut m = Self {
            name: "Cube".into(),
            positions,
            normals: Vec::new(),
            uvs: Vec::new(),
            faces,
        };
        m.recompute_normals();
        m
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cube_subdivision_catmull_clark() {
        let cube = PolyMesh::cube(2.0);
        assert_eq!(cube.vertex_count(), 8);
        assert_eq!(cube.face_count(), 6);

        let subdiv1 = cube.subdivide_catmull_clark(1);
        assert!(subdiv1.is_ok());
        if let Ok(s1) = subdiv1 {
            assert_eq!(s1.face_count(), 24); // 6 faces * 4 quads = 24 quads
            assert_eq!(s1.vertex_count(), 26); // 8 verts + 12 edge pts + 6 face pts = 26

            let trimesh = s1.to_trimesh();
            assert!(trimesh.is_ok());
            if let Ok(tm) = trimesh {
                assert_eq!(tm.triangle_count(), 48); // 24 quads * 2 triangles = 48
            }
        }
    }
}

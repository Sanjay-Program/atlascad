//! Triangle-mesh interchange.
//!
//! [`Mesh`] is the only geometry type that crosses the contract boundary in bulk. It uses plain
//! `f64` arrays rather than a kernel or UI vector type, so a renderer, a CLI and a kernel adapter
//! can exchange tessellations without depending on each other.
//!
//! The invariants that matter are checkable, so [`Mesh::validate`] checks them instead of trusting
//! the producer:
//!
//! * every coordinate is finite (a `NaN` vertex poisons every downstream GPU buffer);
//! * every triangle index is in range;
//! * at least one triangle exists.
//!
//! Degenerate triangles are *counted* rather than rejected: a B-rep tessellator legitimately emits
//! slivers at tangencies, and refusing them would reject correct models.

use serde::{Deserialize, Serialize};

/// An axis-aligned bounding box in millimetres.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct Aabb3 {
    pub min: [f64; 3],
    pub max: [f64; 3],
}

impl Aabb3 {
    /// An empty box (min above max) that [`Aabb3::union`] can grow from.
    pub const EMPTY: Aabb3 = Aabb3 {
        min: [f64::INFINITY; 3],
        max: [f64::NEG_INFINITY; 3],
    };

    /// A box containing the two corner points.
    pub fn new(a: [f64; 3], b: [f64; 3]) -> Self {
        Aabb3 {
            min: [a[0].min(b[0]), a[1].min(b[1]), a[2].min(b[2])],
            max: [a[0].max(b[0]), a[1].max(b[1]), a[2].max(b[2])],
        }
    }

    /// Is this a real box (finite, and min not above max)?
    pub fn is_valid(&self) -> bool {
        self.min
            .iter()
            .chain(self.max.iter())
            .all(|v| v.is_finite())
            && (0..3).all(|i| self.min[i] <= self.max[i])
    }

    /// Grow to include `other`.
    pub fn union(self, other: Aabb3) -> Self {
        Aabb3 {
            min: [0, 1, 2].map(|i| self.min[i].min(other.min[i])),
            max: [0, 1, 2].map(|i| self.max[i].max(other.max[i])),
        }
    }

    /// Include a point.
    pub fn extend(&mut self, p: [f64; 3]) {
        for ((min, max), v) in self.min.iter_mut().zip(self.max.iter_mut()).zip(p) {
            *min = (*min).min(v);
            *max = (*max).max(v);
        }
    }

    /// Side lengths.
    pub fn size(&self) -> [f64; 3] {
        [0, 1, 2].map(|i| self.max[i] - self.min[i])
    }

    /// Centre point.
    pub fn center(&self) -> [f64; 3] {
        [0, 1, 2].map(|i| (self.min[i] + self.max[i]) * 0.5)
    }
}

/// Why a mesh failed validation.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum MeshError {
    #[error("mesh has no triangles")]
    Empty,
    #[error("vertex {index} has a non-finite coordinate")]
    NonFiniteVertex { index: usize },
    #[error("triangle {triangle} uses vertex {index}, but the mesh has {vertices} vertices")]
    IndexOutOfRange {
        triangle: usize,
        index: usize,
        vertices: usize,
    },
    #[error("mesh has {normals} normals for {vertices} vertices")]
    NormalCountMismatch { normals: usize, vertices: usize },
}

/// A triangle mesh with optional per-vertex normals.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct Mesh {
    /// Vertex positions in millimetres.
    pub positions: Vec<[f64; 3]>,
    /// Triangle vertex indices, counter-clockwise seen from outside.
    pub triangles: Vec<[u32; 3]>,
    /// Per-vertex normals. Empty when the producer does not supply them.
    pub normals: Vec<[f64; 3]>,
}

impl Mesh {
    /// Number of triangles.
    pub fn triangle_count(&self) -> usize {
        self.triangles.len()
    }

    /// Number of vertices.
    pub fn vertex_count(&self) -> usize {
        self.positions.len()
    }

    /// Check the invariants a downstream consumer relies on.
    pub fn validate(&self) -> Result<(), MeshError> {
        if self.triangles.is_empty() {
            return Err(MeshError::Empty);
        }
        for (index, p) in self.positions.iter().enumerate() {
            if !p.iter().all(|v| v.is_finite()) {
                return Err(MeshError::NonFiniteVertex { index });
            }
        }
        if !self.normals.is_empty() && self.normals.len() != self.positions.len() {
            return Err(MeshError::NormalCountMismatch {
                normals: self.normals.len(),
                vertices: self.positions.len(),
            });
        }
        let vertices = self.positions.len();
        for (triangle, tri) in self.triangles.iter().enumerate() {
            for &index in tri {
                if index as usize >= vertices {
                    return Err(MeshError::IndexOutOfRange {
                        triangle,
                        index: index as usize,
                        vertices,
                    });
                }
            }
        }
        Ok(())
    }

    /// Number of triangles with (near) zero area.
    ///
    /// Slivers are legal; this is a quality metric, not an error.
    pub fn degenerate_triangles(&self, area_epsilon: f64) -> usize {
        self.triangles
            .iter()
            .filter(|tri| {
                let [a, b, c] =
                    tri.map(|i| self.positions.get(i as usize).copied().unwrap_or([0.0; 3]));
                let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                let cross = [
                    ab[1] * ac[2] - ab[2] * ac[1],
                    ab[2] * ac[0] - ab[0] * ac[2],
                    ab[0] * ac[1] - ab[1] * ac[0],
                ];
                let area =
                    0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
                area <= area_epsilon
            })
            .count()
    }

    /// Signed volume via the divergence theorem, in mm³.
    ///
    /// Positive for a closed, outward-oriented mesh. This is the cheap correctness oracle used by
    /// the benchmarks: for a box of known size the exact answer is known, so a wrong tessellation
    /// or a wrong boolean shows up as a wrong number.
    pub fn signed_volume(&self) -> f64 {
        let mut six_v = 0.0;
        for tri in &self.triangles {
            let (Some(a), Some(b), Some(c)) = (
                self.positions.get(tri[0] as usize),
                self.positions.get(tri[1] as usize),
                self.positions.get(tri[2] as usize),
            ) else {
                continue;
            };
            six_v += a[0] * (b[1] * c[2] - b[2] * c[1]) - a[1] * (b[0] * c[2] - b[2] * c[0])
                + a[2] * (b[0] * c[1] - b[1] * c[0]);
        }
        six_v / 6.0
    }

    /// Number of triangle edges that are not shared by exactly two triangles.
    ///
    /// A closed, manifold surface has zero: every edge is used twice, once in each direction. A
    /// non-zero count means the mesh is a patch (or was triangulated per face without stitching), so
    /// its signed volume is **not** a reliable measurement of an enclosed solid. This is the check
    /// that makes [`Mesh::signed_volume`] trustworthy.
    pub fn open_edges(&self) -> usize {
        let mut edges: std::collections::HashMap<(u32, u32), i32> =
            std::collections::HashMap::new();
        for tri in &self.triangles {
            for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
                *edges.entry((a, b)).or_insert(0) += 1;
                *edges.entry((b, a)).or_insert(0) -= 1;
            }
        }
        // An edge used once in each direction cancels; anything left is not part of a closed
        // two-manifold.
        edges.values().filter(|count| **count != 0).count()
    }

    /// Is the mesh a closed two-manifold (so its signed volume is meaningful)?
    pub fn is_closed(&self) -> bool {
        !self.triangles.is_empty() && self.open_edges() == 0
    }

    /// Total surface area, in mm².
    pub fn surface_area(&self) -> f64 {
        let mut total = 0.0;
        for tri in &self.triangles {
            let (Some(a), Some(b), Some(c)) = (
                self.positions.get(tri[0] as usize),
                self.positions.get(tri[1] as usize),
                self.positions.get(tri[2] as usize),
            ) else {
                continue;
            };
            let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            let cross = [
                ab[1] * ac[2] - ab[2] * ac[1],
                ab[2] * ac[0] - ab[0] * ac[2],
                ab[0] * ac[1] - ab[1] * ac[0],
            ];
            total += 0.5 * (cross[0] * cross[0] + cross[1] * cross[1] + cross[2] * cross[2]).sqrt();
        }
        total
    }

    /// Bounding box, or `None` for an empty mesh.
    pub fn bounding_box(&self) -> Option<Aabb3> {
        let mut bb = Aabb3::EMPTY;
        for p in &self.positions {
            bb.extend(*p);
        }
        bb.is_valid().then_some(bb)
    }
}

#[cfg(test)]
mod tests {
    use super::{Aabb3, Mesh, MeshError};

    /// A unit cube (1 mm³) as 12 triangles, outward-oriented.
    fn unit_cube() -> Mesh {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2], // bottom (z = 0), outward -z
            [4, 5, 6],
            [4, 6, 7], // top (z = 1), outward +z
            [0, 1, 5],
            [0, 5, 4], // y = 0
            [1, 2, 6],
            [1, 6, 5], // x = 1
            [2, 3, 7],
            [2, 7, 6], // y = 1
            [3, 0, 4],
            [3, 4, 7], // x = 0
        ];
        Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        }
    }

    #[test]
    fn valid_cube_passes_validation() {
        assert_eq!(unit_cube().validate(), Ok(()));
    }

    #[test]
    fn cube_volume_is_one_and_area_is_six() {
        let mesh = unit_cube();
        assert!((mesh.signed_volume() - 1.0).abs() < 1e-12);
        assert!((mesh.surface_area() - 6.0).abs() < 1e-12);
    }

    #[test]
    fn inward_winding_gives_negative_volume() {
        let mut mesh = unit_cube();
        for tri in &mut mesh.triangles {
            tri.swap(1, 2);
        }
        assert!((mesh.signed_volume() + 1.0).abs() < 1e-12);
    }

    #[test]
    fn empty_mesh_is_rejected() {
        assert_eq!(Mesh::default().validate(), Err(MeshError::Empty));
    }

    #[test]
    fn nan_vertex_is_rejected_with_its_index() {
        let mut mesh = unit_cube();
        mesh.positions[3] = [f64::NAN, 0.0, 0.0];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::NonFiniteVertex { index: 3 })
        );
    }

    #[test]
    fn out_of_range_index_is_rejected() {
        let mut mesh = unit_cube();
        mesh.triangles[0] = [0, 1, 99];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::IndexOutOfRange {
                triangle: 0,
                index: 99,
                vertices: 8
            })
        );
    }

    #[test]
    fn normal_count_mismatch_is_rejected() {
        let mut mesh = unit_cube();
        mesh.normals = vec![[0.0, 0.0, 1.0]; 3];
        assert_eq!(
            mesh.validate(),
            Err(MeshError::NormalCountMismatch {
                normals: 3,
                vertices: 8
            })
        );
    }

    #[test]
    fn bounding_box_of_cube() {
        let bb = unit_cube().bounding_box();
        assert_eq!(
            bb,
            Some(Aabb3 {
                min: [0.0; 3],
                max: [1.0; 3]
            })
        );
        if let Some(bb) = bb {
            assert_eq!(bb.size(), [1.0; 3]);
            assert_eq!(bb.center(), [0.5; 3]);
        }
    }

    #[test]
    fn empty_and_union_bounds() {
        assert!(!Aabb3::EMPTY.is_valid());
        let a = Aabb3::new([0.0; 3], [1.0; 3]);
        let b = Aabb3::new([2.0, -1.0, 0.0], [3.0, 0.0, 1.0]);
        let u = a.union(b);
        assert_eq!(u.min, [0.0, -1.0, 0.0]);
        assert_eq!(u.max, [3.0, 1.0, 1.0]);
    }

    #[test]
    fn cube_is_closed_but_an_open_patch_is_not() {
        let cube = unit_cube();
        assert_eq!(cube.open_edges(), 0);
        assert!(cube.is_closed());
        // Drop one triangle: its three edges become unpaired.
        let mut open = unit_cube();
        open.triangles.pop();
        assert!(!open.is_closed());
        assert!(open.open_edges() > 0);
    }

    #[test]
    fn empty_mesh_is_not_closed() {
        assert!(!Mesh::default().is_closed());
    }

    #[test]
    fn degenerate_triangles_are_counted_not_rejected() {
        let mut mesh = unit_cube();
        // Add a zero-area sliver using an existing vertex three times.
        mesh.triangles.push([0, 0, 0]);
        assert_eq!(mesh.validate(), Ok(()));
        assert_eq!(mesh.degenerate_triangles(1e-12), 1);
    }

    #[test]
    fn json_round_trip() {
        let mesh = unit_cube();
        let json = serde_json::to_string(&mesh).unwrap_or_default();
        assert_eq!(serde_json::from_str::<Mesh>(&json).ok(), Some(mesh));
    }
}

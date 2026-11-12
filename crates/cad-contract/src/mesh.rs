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

    /// Exact closed-form mass properties via the divergence theorem.
    ///
    /// Computes volume, surface area, center of gravity (centroid), inertia tensor about origin and
    /// centroid, principal moments and axes (via Jacobi eigenvalue decomposition), and radii of gyration.
    /// Returns `None` if the mesh is empty or degenerates to zero volume.
    pub fn mass_properties(&self) -> Option<MassProperties> {
        if self.triangles.is_empty() || self.positions.is_empty() {
            return None;
        }

        let mut total_det = 0.0;
        let mut c_acc = [0.0; 3];
        let mut t_acc = [[0.0; 3]; 3];

        for tri in &self.triangles {
            let (Some(a), Some(b), Some(c)) = (
                self.positions.get(tri[0] as usize),
                self.positions.get(tri[1] as usize),
                self.positions.get(tri[2] as usize),
            ) else {
                continue;
            };

            let cross = [
                b[1] * c[2] - b[2] * c[1],
                b[2] * c[0] - b[0] * c[2],
                b[0] * c[1] - b[1] * c[0],
            ];
            let det = a[0] * cross[0] + a[1] * cross[1] + a[2] * cross[2];
            total_det += det;

            for k in 0..3 {
                c_acc[k] += det * (a[k] + b[k] + c[k]);
            }

            for i in 0..3 {
                for j in i..3 {
                    let term = a[i] * a[j]
                        + b[i] * b[j]
                        + c[i] * c[j]
                        + (a[i] + b[i] + c[i]) * (a[j] + b[j] + c[j]);
                    let val = (det / 120.0) * term;
                    t_acc[i][j] += val;
                    if i != j {
                        t_acc[j][i] += val;
                    }
                }
            }
        }

        if total_det.abs() < 1e-12 {
            return None;
        }

        let sign = if total_det < 0.0 { -1.0 } else { 1.0 };
        let volume_mm3 = total_det * sign / 6.0;
        let centroid_mm = [
            c_acc[0] / (4.0 * total_det),
            c_acc[1] / (4.0 * total_det),
            c_acc[2] / (4.0 * total_det),
        ];

        let mut t_origin = [[0.0; 3]; 3];
        let mut t_cent = [[0.0; 3]; 3];
        for i in 0..3 {
            for j in 0..3 {
                let to = t_acc[i][j] * sign;
                t_origin[i][j] = to;
                t_cent[i][j] = to - volume_mm3 * centroid_mm[i] * centroid_mm[j];
            }
        }

        let make_inertia_tensor = |t: [[f64; 3]; 3]| -> [[f64; 3]; 3] {
            [
                [t[1][1] + t[2][2], -t[0][1], -t[0][2]],
                [-t[1][0], t[0][0] + t[2][2], -t[1][2]],
                [-t[2][0], -t[2][1], t[0][0] + t[1][1]],
            ]
        };

        let inertia_tensor_origin = make_inertia_tensor(t_origin);
        let inertia_tensor_centroid = make_inertia_tensor(t_cent);

        let (mut principal_moments, principal_axes) = jacobi_symmetric_3x3(inertia_tensor_centroid);
        for m in &mut principal_moments {
            if *m < 0.0 {
                *m = 0.0;
            }
        }

        let calc_radii = |tensor: [[f64; 3]; 3]| -> [f64; 3] {
            [
                (tensor[0][0].max(0.0) / volume_mm3).sqrt(),
                (tensor[1][1].max(0.0) / volume_mm3).sqrt(),
                (tensor[2][2].max(0.0) / volume_mm3).sqrt(),
            ]
        };

        let radii_of_gyration_origin = calc_radii(inertia_tensor_origin);
        let radii_of_gyration_centroid = calc_radii(inertia_tensor_centroid);
        let radii_of_gyration_principal = [
            (principal_moments[0] / volume_mm3).sqrt(),
            (principal_moments[1] / volume_mm3).sqrt(),
            (principal_moments[2] / volume_mm3).sqrt(),
        ];

        Some(MassProperties {
            volume_mm3,
            surface_area_mm2: self.surface_area(),
            centroid_mm,
            inertia_tensor_centroid,
            inertia_tensor_origin,
            principal_moments,
            principal_axes,
            radii_of_gyration_centroid,
            radii_of_gyration_origin,
            radii_of_gyration_principal,
        })
    }
}

/// Mass properties and inertial characteristics of a 3D geometry.
///
/// Geometric volume properties are in millimetre units (mm, mm³, mm⁵).
/// Physical properties scaled with material density are available via [`MassProperties::with_density`].
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MassProperties {
    /// Total enclosed volume in mm³.
    pub volume_mm3: f64,
    /// Total external surface area in mm².
    pub surface_area_mm2: f64,
    /// Center of gravity / centroid `[x, y, z]` in mm.
    pub centroid_mm: [f64; 3],
    /// Inertia tensor relative to centroid `[[Ixx, -Ixy, -Ixz], [-Iyx, Iyy, -Iyz], [-Izx, -Izy, Izz]]` in mm⁵.
    pub inertia_tensor_centroid: [[f64; 3]; 3],
    /// Inertia tensor relative to origin in mm⁵.
    pub inertia_tensor_origin: [[f64; 3]; 3],
    /// Principal moments of inertia `[I1, I2, I3]` in mm⁵ sorted in ascending order.
    pub principal_moments: [f64; 3],
    /// Normalized principal axes of inertia `[v1, v2, v3]`, each `[x, y, z]`.
    pub principal_axes: [[f64; 3]; 3],
    /// Radii of gyration relative to centroid `[kx, ky, kz]` in mm.
    pub radii_of_gyration_centroid: [f64; 3],
    /// Radii of gyration relative to origin `[kx, ky, kz]` in mm.
    pub radii_of_gyration_origin: [f64; 3],
    /// Radii of gyration relative to principal axes `[k1, k2, k3]` in mm.
    pub radii_of_gyration_principal: [f64; 3],
}

impl MassProperties {
    /// Scale mass properties by material density (given in kg/m³).
    ///
    /// Since 1 m³ = 10⁹ mm³, mass (kg) = volume (mm³) * density (kg/m³) * 10⁻⁹.
    /// Mass moment of inertia (kg·mm²) = geometric moment (mm⁵) * density (kg/m³) * 10⁻⁹.
    pub fn with_density(&self, density_kg_m3: f64) -> PhysicalMassReport {
        let density = if density_kg_m3.is_finite() && density_kg_m3 > 0.0 {
            density_kg_m3
        } else {
            0.0
        };
        let scale = density * 1e-9;
        let mass_kg = (self.volume_mm3 * scale).max(0.0);
        let mass_g = mass_kg * 1000.0;

        let scale_tensor = |m: [[f64; 3]; 3]| -> [[f64; 3]; 3] {
            [
                [m[0][0] * scale, m[0][1] * scale, m[0][2] * scale],
                [m[1][0] * scale, m[1][1] * scale, m[1][2] * scale],
                [m[2][0] * scale, m[2][1] * scale, m[2][2] * scale],
            ]
        };

        let principal_mass_moments = [
            (self.principal_moments[0] * scale).max(0.0),
            (self.principal_moments[1] * scale).max(0.0),
            (self.principal_moments[2] * scale).max(0.0),
        ];

        PhysicalMassReport {
            density_kg_m3: density,
            mass_kg,
            mass_g,
            volume_mm3: self.volume_mm3,
            surface_area_mm2: self.surface_area_mm2,
            centroid_mm: self.centroid_mm,
            mass_inertia_centroid: scale_tensor(self.inertia_tensor_centroid),
            mass_inertia_origin: scale_tensor(self.inertia_tensor_origin),
            principal_mass_moments,
            principal_axes: self.principal_axes,
            radii_of_gyration_centroid: self.radii_of_gyration_centroid,
            radii_of_gyration_origin: self.radii_of_gyration_origin,
            radii_of_gyration_principal: self.radii_of_gyration_principal,
        }
    }
}

/// Physical mass properties scaled by material density.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PhysicalMassReport {
    /// Material density in kg/m³.
    pub density_kg_m3: f64,
    /// Total mass in kilograms (kg).
    pub mass_kg: f64,
    /// Total mass in grams (g).
    pub mass_g: f64,
    /// Volume in mm³.
    pub volume_mm3: f64,
    /// Surface area in mm².
    pub surface_area_mm2: f64,
    /// Centroid in mm.
    pub centroid_mm: [f64; 3],
    /// Mass moments of inertia about centroid in kg·mm².
    pub mass_inertia_centroid: [[f64; 3]; 3],
    /// Mass moments of inertia about origin in kg·mm².
    pub mass_inertia_origin: [[f64; 3]; 3],
    /// Principal mass moments of inertia `[I1, I2, I3]` in kg·mm².
    pub principal_mass_moments: [f64; 3],
    /// Normalized principal axes of inertia.
    pub principal_axes: [[f64; 3]; 3],
    /// Radii of gyration about centroid in mm.
    pub radii_of_gyration_centroid: [f64; 3],
    /// Radii of gyration about origin in mm.
    pub radii_of_gyration_origin: [f64; 3],
    /// Radii of gyration about principal axes in mm.
    pub radii_of_gyration_principal: [f64; 3],
}

/// Jacobi eigenvalue algorithm for 3x3 real symmetric matrices.
///
/// Returns sorted eigenvalues in ascending order and corresponding orthonormal eigenvectors.
pub fn jacobi_symmetric_3x3(mut a: [[f64; 3]; 3]) -> ([f64; 3], [[f64; 3]; 3]) {
    let mut v = [[1.0, 0.0, 0.0], [0.0, 1.0, 0.0], [0.0, 0.0, 1.0]];

    for _sweep in 0..50 {
        let off_diag_sq = a[0][1] * a[0][1] + a[0][2] * a[0][2] + a[1][2] * a[1][2];
        if off_diag_sq < 1e-24 {
            break;
        }

        for p in 0..2 {
            for q in (p + 1)..3 {
                let apq = a[p][q];
                if apq.abs() < 1e-15 {
                    continue;
                }
                let app = a[p][p];
                let aqq = a[q][q];
                let tau = (aqq - app) / (2.0 * apq);
                let t = if tau >= 0.0 {
                    1.0 / (tau + (1.0 + tau * tau).sqrt())
                } else {
                    -1.0 / (-tau + (1.0 + tau * tau).sqrt())
                };
                let c = 1.0 / (1.0 + t * t).sqrt();
                let s = t * c;
                let h = t * apq;

                a[p][p] -= h;
                a[q][q] += h;
                a[p][q] = 0.0;
                a[q][p] = 0.0;

                for r in 0..3 {
                    if r != p && r != q {
                        let arp = a[r][p];
                        let arq = a[r][q];
                        a[r][p] = c * arp - s * arq;
                        a[p][r] = a[r][p];
                        a[r][q] = s * arp + c * arq;
                        a[q][r] = a[r][q];
                    }
                    let vrp = v[r][p];
                    let vrq = v[r][q];
                    v[r][p] = c * vrp - s * vrq;
                    v[r][q] = s * vrp + c * vrq;
                }
            }
        }
    }

    let mut ev = [a[0][0], a[1][1], a[2][2]];
    let mut axes = [
        [v[0][0], v[1][0], v[2][0]],
        [v[0][1], v[1][1], v[2][1]],
        [v[0][2], v[1][2], v[2][2]],
    ];

    if ev[0] > ev[1] {
        ev.swap(0, 1);
        axes.swap(0, 1);
    }
    if ev[1] > ev[2] {
        ev.swap(1, 2);
        axes.swap(1, 2);
    }
    if ev[0] > ev[1] {
        ev.swap(0, 1);
        axes.swap(0, 1);
    }

    // Ensure right-handed coordinate frame: axes[0] x axes[1] . axes[2] > 0
    let cross = [
        axes[0][1] * axes[1][2] - axes[0][2] * axes[1][1],
        axes[0][2] * axes[1][0] - axes[0][0] * axes[1][2],
        axes[0][0] * axes[1][1] - axes[0][1] * axes[1][0],
    ];
    let dot = cross[0] * axes[2][0] + cross[1] * axes[2][1] + cross[2] * axes[2][2];
    if dot < 0.0 {
        axes[2] = [-axes[2][0], -axes[2][1], -axes[2][2]];
    }

    (ev, axes)
}

#[cfg(test)]
mod tests {
    use super::{Aabb3, Mesh, MeshError, jacobi_symmetric_3x3};

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

    #[test]
    fn jacobi_3x3_known_matrix() {
        let m = [[2.0, 1.0, 0.0], [1.0, 2.0, 0.0], [0.0, 0.0, 5.0]];
        let (ev, axes) = jacobi_symmetric_3x3(m);
        assert!((ev[0] - 1.0).abs() < 1e-10);
        assert!((ev[1] - 3.0).abs() < 1e-10);
        assert!((ev[2] - 5.0).abs() < 1e-10);

        // Verify M * v = lambda * v for each eigenpair
        for i in 0..3 {
            let lam = ev[i];
            let v = [axes[i][0], axes[i][1], axes[i][2]];
            let mv = [
                m[0][0] * v[0] + m[0][1] * v[1] + m[0][2] * v[2],
                m[1][0] * v[0] + m[1][1] * v[1] + m[1][2] * v[2],
                m[2][0] * v[0] + m[2][1] * v[1] + m[2][2] * v[2],
            ];
            for j in 0..3 {
                assert!((mv[j] - lam * v[j]).abs() < 1e-9);
            }
        }
    }

    #[test]
    fn mass_properties_of_unit_cube() {
        let cube = unit_cube();
        let Some(props) = cube.mass_properties() else {
            assert_eq!("cube.mass_properties", "returned Some");
            return;
        };
        assert!((props.volume_mm3 - 1.0).abs() < 1e-12);
        assert!((props.surface_area_mm2 - 6.0).abs() < 1e-12);
        assert!((props.centroid_mm[0] - 0.5).abs() < 1e-12);
        assert!((props.centroid_mm[1] - 0.5).abs() < 1e-12);
        assert!((props.centroid_mm[2] - 0.5).abs() < 1e-12);

        // Diagonal moments about centroid for unit cube are 1/6
        let expected_i = 1.0 / 6.0;
        assert!((props.inertia_tensor_centroid[0][0] - expected_i).abs() < 1e-10);
        assert!((props.inertia_tensor_centroid[1][1] - expected_i).abs() < 1e-10);
        assert!((props.inertia_tensor_centroid[2][2] - expected_i).abs() < 1e-10);

        // Off-diagonals about centroid must be near zero
        assert!(props.inertia_tensor_centroid[0][1].abs() < 1e-10);
        assert!(props.inertia_tensor_centroid[0][2].abs() < 1e-10);
        assert!(props.inertia_tensor_centroid[1][2].abs() < 1e-10);

        // Principal moments
        for i in 0..3 {
            assert!((props.principal_moments[i] - expected_i).abs() < 1e-10);
            assert!((props.radii_of_gyration_centroid[i] - (expected_i).sqrt()).abs() < 1e-10);
        }
    }

    #[test]
    fn mass_properties_of_rectangular_prism() {
        // Box of 10 x 20 x 30 mm
        let (w, d, h) = (10.0, 20.0, 30.0);
        let positions = vec![
            [0.0, 0.0, 0.0],
            [w, 0.0, 0.0],
            [w, d, 0.0],
            [0.0, d, 0.0],
            [0.0, 0.0, h],
            [w, 0.0, h],
            [w, d, h],
            [0.0, d, h],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2], // bottom (z = 0)
            [4, 5, 6],
            [4, 6, 7], // top (z = h)
            [0, 1, 5],
            [0, 5, 4], // y = 0
            [1, 2, 6],
            [1, 6, 5], // x = w
            [2, 3, 7],
            [2, 7, 6], // y = d
            [3, 0, 4],
            [3, 4, 7], // x = 0
        ];
        let prism = Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        };

        let Some(props) = prism.mass_properties() else {
            assert_eq!("prism.mass_properties", "returned Some");
            return;
        };
        let expected_vol = w * d * h; // 6000
        assert!((props.volume_mm3 - expected_vol).abs() < 1e-9);
        assert!((props.centroid_mm[0] - w * 0.5).abs() < 1e-9);
        assert!((props.centroid_mm[1] - d * 0.5).abs() < 1e-9);
        assert!((props.centroid_mm[2] - h * 0.5).abs() < 1e-9);

        let exp_ixx = expected_vol * (d * d + h * h) / 12.0; // 6000 * 1300 / 12 = 650000
        let exp_iyy = expected_vol * (w * w + h * h) / 12.0; // 6000 * 1000 / 12 = 500000
        let exp_izz = expected_vol * (w * w + d * d) / 12.0; // 6000 * 500 / 12 = 250000

        assert!((props.inertia_tensor_centroid[0][0] - exp_ixx).abs() < 1e-6);
        assert!((props.inertia_tensor_centroid[1][1] - exp_iyy).abs() < 1e-6);
        assert!((props.inertia_tensor_centroid[2][2] - exp_izz).abs() < 1e-6);

        // Principal moments sorted ascending: 250000, 500000, 650000
        assert!((props.principal_moments[0] - exp_izz).abs() < 1e-6);
        assert!((props.principal_moments[1] - exp_iyy).abs() < 1e-6);
        assert!((props.principal_moments[2] - exp_ixx).abs() < 1e-6);

        // Density scale: Structural steel 7850 kg/m³
        let physical = props.with_density(7850.0);
        let exp_mass_kg = expected_vol * 7850.0 * 1e-9;
        assert!((physical.mass_kg - exp_mass_kg).abs() < 1e-9);
        assert!((physical.mass_g - exp_mass_kg * 1000.0).abs() < 1e-6);
        assert!((physical.mass_inertia_centroid[0][0] - exp_ixx * 7850.0 * 1e-9).abs() < 1e-6);
    }
}

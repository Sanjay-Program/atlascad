//! Voxel solid meshing from a closed triangle mesh.
//!
//! The platform already produces a watertight tessellation from the kernel, so the smallest honest
//! path to a volumetric mesh is to classify a regular grid of voxel centres inside the solid. That
//! keeps the analysis inside one crate, gives an exactly representable mesh for axis-aligned tests,
//! and makes the geometric error a stated, measurable quantity (voxel volume versus the surface
//! mesh's signed volume) instead of an invisible one.
//!
//! What this is not: a boundary-fitted or curved-element mesher. A curved or sloped face is
//! approximated by whole voxels, so a stress at a re-entrant corner or on a fillet is a mesh
//! approximation, and the study says so.

use crate::CaeError;
use cad_contract::mesh::Mesh;

/// Largest grid this crate will classify. The check happens before the inside/outside mask is
/// allocated, so an accidentally tiny voxel size cannot exhaust memory.
pub(crate) const MAX_GRID_CELLS: usize = 4_000_000;

/// A regular grid of cubes, with the cells whose centre lies inside the solid marked.
pub(crate) struct VoxelGrid {
    pub origin: [f64; 3],
    pub cell: f64,
    pub counts: [usize; 3],
    pub inside: Vec<bool>,
}

impl VoxelGrid {
    /// Classify a grid of `cell`-sized voxels covering the mesh's bounding box.
    ///
    /// The grid origin sits at the bounding box minimum, so a model whose extents are an exact
    /// multiple of `cell` produces an exact voxel fill with no partial cells.
    pub fn build(mesh: &Mesh, cell: f64) -> Result<Self, CaeError> {
        let bounds = mesh
            .bounding_box()
            .ok_or_else(|| CaeError::Invalid("the mesh has no usable bounding box".into()))?;
        let size = bounds.size();
        if !cell.is_finite() || cell <= 0.0 {
            return Err(CaeError::Invalid(format!(
                "voxel size {cell} is not a positive length"
            )));
        }
        let counts = [0, 1, 2].map(|axis| {
            let ratio = size[axis] / cell;
            let nearest = ratio.round();
            let cells = if (ratio - nearest).abs() < 1e-9 {
                nearest
            } else {
                ratio.ceil()
            };
            (cells.max(1.0) as usize).min(MAX_GRID_CELLS)
        });
        let cells = counts[0]
            .checked_mul(counts[1])
            .and_then(|value| value.checked_mul(counts[2]))
            .ok_or_else(|| CaeError::Invalid("voxel grid dimensions overflow".into()))?;
        if cells > MAX_GRID_CELLS {
            return Err(CaeError::Invalid(format!(
                "a {cell} mm voxel needs {cells} cells, over the {MAX_GRID_CELLS}-cell limit; \
                 use a larger voxel size"
            )));
        }

        let triangles: Vec<[[f64; 3]; 3]> = mesh
            .triangles
            .iter()
            .filter_map(|tri| {
                let a = *mesh.positions.get(tri[0] as usize)?;
                let b = *mesh.positions.get(tri[1] as usize)?;
                let c = *mesh.positions.get(tri[2] as usize)?;
                Some([a, b, c])
            })
            .collect();
        if triangles.is_empty() {
            return Err(CaeError::Invalid("the mesh has no usable triangles".into()));
        }

        let mut inside = vec![false; cells];
        // A ray along +x is jittered in y and z so it does not run exactly through a shared edge or
        // vertex, where the parity test is ill-conditioned. The offset is far smaller than the
        // distance from a voxel centre to the surface.
        let jitter_y = cell * 1.0e-4;
        let jitter_z = cell * 2.0e-4;
        let mut crossings = 0_usize;
        for k in 0..counts[2] {
            for j in 0..counts[1] {
                for i in 0..counts[0] {
                    let centre = [
                        bounds.min[0] + (i as f64 + 0.5) * cell,
                        bounds.min[1] + (j as f64 + 0.5) * cell,
                        bounds.min[2] + (k as f64 + 0.5) * cell,
                    ];
                    let origin = [centre[0], centre[1] + jitter_y, centre[2] + jitter_z];
                    let mut hits = 0_usize;
                    for triangle in &triangles {
                        if ray_hits(origin, *triangle) {
                            hits += 1;
                        }
                    }
                    if hits % 2 == 1 {
                        inside[i + counts[0] * (j + counts[1] * k)] = true;
                        crossings += 1;
                    }
                }
            }
        }
        if crossings == 0 {
            return Err(CaeError::Invalid(
                "no voxel centre fell inside the solid; the voxel size is probably larger than \
                 the model"
                    .into(),
            ));
        }
        Ok(VoxelGrid {
            origin: bounds.min,
            cell,
            counts,
            inside,
        })
    }

    /// Number of solid voxels, which is the number of hexahedral elements.
    pub fn element_count(&self) -> usize {
        self.inside.iter().filter(|inside| **inside).count()
    }

    /// Node counts per axis for the structured grid.
    pub fn node_counts(&self) -> [usize; 3] {
        [self.counts[0] + 1, self.counts[1] + 1, self.counts[2] + 1]
    }
}

/// Does a ray from `origin` in +x cross this triangle ahead of its origin?
///
/// Möller–Trumbore, with the barycentric bounds inclusive so an edge hit is counted.
fn ray_hits(origin: [f64; 3], triangle: [[f64; 3]; 3]) -> bool {
    let [a, b, c] = triangle;
    let e1 = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let e2 = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    // The ray direction is +x, so the determinant is the x component of e1 x e2.
    let p = [0.0, -e2[2], e2[1]];
    let det = e1[0] * p[0] + e1[1] * p[1] + e1[2] * p[2];
    if det.abs() < 1.0e-12 {
        return false;
    }
    let inv = 1.0 / det;
    let t = [origin[0] - a[0], origin[1] - a[1], origin[2] - a[2]];
    let u = (t[0] * p[0] + t[1] * p[1] + t[2] * p[2]) * inv;
    if !(-1.0e-12..=1.0 + 1.0e-12).contains(&u) {
        return false;
    }
    let q = [
        t[1] * e1[2] - t[2] * e1[1],
        t[2] * e1[0] - t[0] * e1[2],
        t[0] * e1[1] - t[1] * e1[0],
    ];
    let v = q[0] * inv;
    if v < -1.0e-12 || u + v > 1.0 + 1.0e-12 {
        return false;
    }
    let distance = (e2[0] * q[0] + e2[1] * q[1] + e2[2] * q[2]) * inv;
    distance > 0.0
}

#[cfg(test)]
mod tests {
    use super::{VoxelGrid, ray_hits};

    fn cube() -> cad_contract::mesh::Mesh {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [2.0, 0.0, 0.0],
            [2.0, 2.0, 0.0],
            [0.0, 2.0, 0.0],
            [0.0, 0.0, 2.0],
            [2.0, 0.0, 2.0],
            [2.0, 2.0, 2.0],
            [0.0, 2.0, 2.0],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        cad_contract::mesh::Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        }
    }

    #[test]
    fn an_aligned_cube_fills_its_grid_exactly() -> Result<(), Box<dyn std::error::Error>> {
        let grid = VoxelGrid::build(&cube(), 1.0)?;
        assert_eq!(grid.counts, [2, 2, 2]);
        assert_eq!(grid.element_count(), 8);
        assert_eq!(grid.node_counts(), [3, 3, 3]);
        Ok(())
    }

    #[test]
    fn a_voxel_larger_than_the_model_is_refused() {
        assert!(VoxelGrid::build(&cube(), 10.0).is_err());
    }

    #[test]
    fn a_non_positive_voxel_size_is_refused() {
        assert!(VoxelGrid::build(&cube(), 0.0).is_err());
        assert!(VoxelGrid::build(&cube(), -1.0).is_err());
    }

    #[test]
    fn a_ray_through_the_middle_hits_the_far_face_only() {
        // A triangle in the plane x = 4, spanning y and z, crossed by a +x ray.
        let triangle = [[4.0, 0.0, 0.0], [4.0, 2.0, 0.0], [4.0, 0.0, 2.0]];
        assert!(ray_hits([1.0, 0.5, 0.5], triangle));
        // From beyond the plane the crossing is behind the origin.
        assert!(!ray_hits([5.0, 0.5, 0.5], triangle));
        // Outside the triangle's y-z footprint.
        assert!(!ray_hits([1.0, 1.5, 1.5], triangle));
        // A triangle that contains the ray direction is parallel to it and not crossed.
        let parallel = [[0.0, 0.0, 4.0], [1.0, 0.0, 4.0], [0.0, 0.0, 6.0]];
        assert!(!ray_hits([1.0, 1.0, 1.0], parallel));
    }
}

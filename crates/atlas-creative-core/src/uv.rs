use crate::mesh::PolyMesh;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum UvProjection {
    PlanarZ,
    PlanarY,
    PlanarX,
    BoxTriplanar,
    Cylindrical,
    Spherical,
}

pub fn unwrap_uvs(mesh: &mut PolyMesh, projection: UvProjection) {
    let mut uvs = Vec::with_capacity(mesh.positions.len());

    let (mut min_x, mut max_x) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut min_y, mut max_y) = (f64::INFINITY, f64::NEG_INFINITY);
    let (mut min_z, mut max_z) = (f64::INFINITY, f64::NEG_INFINITY);

    for p in &mesh.positions {
        min_x = min_x.min(p[0]);
        max_x = max_x.max(p[0]);
        min_y = min_y.min(p[1]);
        max_y = max_y.max(p[1]);
        min_z = min_z.min(p[2]);
        max_z = max_z.max(p[2]);
    }

    let dx = (max_x - min_x).max(1e-6);
    let dy = (max_y - min_y).max(1e-6);
    let dz = (max_z - min_z).max(1e-6);

    for p in &mesh.positions {
        let (u, v) = match projection {
            UvProjection::PlanarZ => ((p[0] - min_x) / dx, (p[1] - min_y) / dy),
            UvProjection::PlanarY => ((p[0] - min_x) / dx, (p[2] - min_z) / dz),
            UvProjection::PlanarX => ((p[1] - min_y) / dy, (p[2] - min_z) / dz),
            UvProjection::BoxTriplanar => {
                let abs_x = p[0].abs();
                let abs_y = p[1].abs();
                let abs_z = p[2].abs();
                if abs_z >= abs_x && abs_z >= abs_y {
                    ((p[0] - min_x) / dx, (p[1] - min_y) / dy)
                } else if abs_y >= abs_x && abs_y >= abs_z {
                    ((p[0] - min_x) / dx, (p[2] - min_z) / dz)
                } else {
                    ((p[1] - min_y) / dy, (p[2] - min_z) / dz)
                }
            }
            UvProjection::Cylindrical => {
                let angle = p[1].atan2(p[0]); // -pi to pi
                let u = (angle + std::f64::consts::PI) / (2.0 * std::f64::consts::PI);
                let v = (p[2] - min_z) / dz;
                (u, v)
            }
            UvProjection::Spherical => {
                let r = (p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt().max(1e-6);
                let phi = p[1].atan2(p[0]); // longitude
                let theta = (p[2] / r).clamp(-1.0, 1.0).acos(); // latitude
                let u = (phi + std::f64::consts::PI) / (2.0 * std::f64::consts::PI);
                let v = theta / std::f64::consts::PI;
                (u, v)
            }
        };
        uvs.push([u.clamp(0.0, 1.0), v.clamp(0.0, 1.0)]);
    }

    mesh.uvs = uvs;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_uv_box_unwrap() {
        let mut cube = PolyMesh::cube(2.0);
        unwrap_uvs(&mut cube, UvProjection::BoxTriplanar);
        assert_eq!(cube.uvs.len(), cube.positions.len());
        for uv in &cube.uvs {
            assert!(uv[0] >= 0.0 && uv[0] <= 1.0);
            assert!(uv[1] >= 0.0 && uv[1] <= 1.0);
        }
    }
}

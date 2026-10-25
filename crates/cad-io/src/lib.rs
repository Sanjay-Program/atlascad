//! Mesh interchange shared by the headless CLI and the desktop application.
//!
//! Only binary STL is implemented today. It is the one format the truck adapter can write without
//! an additional geometry dependency, and it is what every downstream slicer and viewer reads.
//!
//! # What STL does not carry
//!
//! Binary STL has no unit field, no body or assembly structure, no colours, and no feature history.
//! A reader that needs units must take them from the [`cad_contract::document::DocumentHeader`]
//! stored next to the file. This crate never guesses a unit from the magnitude of the coordinates.
//!
//! # Untrusted input
//!
//! [`read_stl_binary`] validates the declared triangle count against the file length before it
//! allocates, so a 84-byte file that claims four billion triangles is rejected instead of reserving
//! hundreds of gigabytes.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cad_contract::mesh::{Mesh, MeshError};
use std::path::Path;

/// The 80-byte binary STL header.
pub const STL_HEADER_BYTES: usize = 80;
/// The size of one binary STL facet: normal, three vertices, attribute word.
pub const STL_TRIANGLE_BYTES: usize = 50;
/// Header plus the triangle-count word.
pub const STL_PREFIX_BYTES: usize = STL_HEADER_BYTES + 4;

/// Why a mesh could not be written or read as STL.
#[derive(Debug, thiserror::Error)]
pub enum StlError {
    #[error("mesh: {0}")]
    Mesh(#[from] MeshError),
    #[error("I/O at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid STL: {0}")]
    Invalid(String),
}

/// Serialise a mesh as a binary STL.
///
/// The mesh is validated first, so an out-of-range index cannot become a truncated file. Binary STL
/// stores 32-bit floats; the conversion is explicit and lossy by definition, not by accident.
pub fn stl_bytes(mesh: &Mesh) -> Result<Vec<u8>, StlError> {
    mesh.validate()?;
    let triangle_count = u32::try_from(mesh.triangles.len()).map_err(|_| {
        StlError::Invalid("mesh has more triangles than binary STL can hold".into())
    })?;

    let mut out = Vec::with_capacity(STL_PREFIX_BYTES + STL_TRIANGLE_BYTES * mesh.triangles.len());
    let label = b"Atlas CAD binary STL - millimetre coordinates";
    let mut header = [0u8; STL_HEADER_BYTES];
    let copy = label.len().min(STL_HEADER_BYTES);
    header[..copy].copy_from_slice(&label[..copy]);
    out.extend_from_slice(&header);
    out.extend_from_slice(&triangle_count.to_le_bytes());

    for triangle in &mesh.triangles {
        let [ai, bi, ci] = *triangle;
        let a = vertex(mesh, ai)?;
        let b = vertex(mesh, bi)?;
        let c = vertex(mesh, ci)?;
        let normal = normal_of(a, b, c);
        for value in normal
            .iter()
            .chain(a.iter())
            .chain(b.iter())
            .chain(c.iter())
        {
            out.extend_from_slice(&(*value as f32).to_le_bytes());
        }
        out.extend_from_slice(&0u16.to_le_bytes());
    }
    Ok(out)
}

/// Write a mesh as a binary STL and return the number of bytes written.
pub fn write_stl_binary(mesh: &Mesh, path: &Path) -> Result<usize, StlError> {
    let bytes = stl_bytes(mesh)?;
    std::fs::write(path, &bytes).map_err(|source| StlError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(bytes.len())
}

/// Read a binary STL back into a mesh.
///
/// Every triangle gets its own three vertices, which is how STL stores facets. A consumer that needs
/// shared vertices must weld them; [`Mesh::open_edges`] is only meaningful after welding.
pub fn read_stl_binary(bytes: &[u8]) -> Result<Mesh, StlError> {
    if bytes.len() < STL_PREFIX_BYTES {
        return Err(StlError::Invalid(
            "file is too short to be a binary STL".into(),
        ));
    }
    let mut count_bytes = [0u8; 4];
    count_bytes.copy_from_slice(&bytes[STL_HEADER_BYTES..STL_PREFIX_BYTES]);
    let count = u32::from_le_bytes(count_bytes) as usize;
    let expected = count
        .checked_mul(STL_TRIANGLE_BYTES)
        .and_then(|size| size.checked_add(STL_PREFIX_BYTES))
        .ok_or_else(|| {
            StlError::Invalid(format!(
                "binary STL declares an impossible {count} triangles"
            ))
        })?;
    if bytes.len() != expected {
        return Err(StlError::Invalid(format!(
            "binary STL declares {count} triangles ({expected} bytes) but the file is {} bytes",
            bytes.len()
        )));
    }

    let mut positions: Vec<[f64; 3]> = Vec::with_capacity(count.saturating_mul(3));
    let mut triangles: Vec<[u32; 3]> = Vec::with_capacity(count);
    for index in 0..count {
        let base = STL_PREFIX_BYTES + index * STL_TRIANGLE_BYTES + 12; // skip the stored normal
        let mut corners = [[0.0f64; 3]; 3];
        for (corner, slot) in corners.iter_mut().enumerate() {
            for (axis, component) in slot.iter_mut().enumerate() {
                let start = base + corner * 12 + axis * 4;
                let mut raw = [0u8; 4];
                raw.copy_from_slice(&bytes[start..start + 4]);
                *component = f32::from_le_bytes(raw) as f64;
            }
        }
        let first = u32::try_from(positions.len())
            .map_err(|_| StlError::Invalid("STL has too many vertices for a u32 mesh".into()))?;
        positions.extend_from_slice(&corners);
        triangles.push([first, first + 1, first + 2]);
    }

    let mesh = Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    };
    mesh.validate()?;
    Ok(mesh)
}

fn vertex(mesh: &Mesh, index: u32) -> Result<[f64; 3], StlError> {
    mesh.positions.get(index as usize).copied().ok_or_else(|| {
        StlError::Invalid(format!(
            "triangle references vertex {index} but the mesh has {} vertices",
            mesh.positions.len()
        ))
    })
}

fn normal_of(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> [f64; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let length = (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt();
    if length > 0.0 && length.is_finite() {
        [n[0] / length, n[1] / length, n[2] / length]
    } else {
        [0.0, 0.0, 0.0]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn cube() -> Mesh {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 5.0],
            [10.0, 0.0, 5.0],
            [10.0, 10.0, 5.0],
            [0.0, 10.0, 5.0],
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
        Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        }
    }

    #[test]
    fn a_written_cube_is_exactly_fifty_bytes_per_triangle() -> TestResult {
        let bytes = stl_bytes(&cube())?;
        assert_eq!(bytes.len(), STL_PREFIX_BYTES + 12 * STL_TRIANGLE_BYTES);
        let declared = u32::from_le_bytes([bytes[80], bytes[81], bytes[82], bytes[83]]);
        assert_eq!(declared, 12);
        Ok(())
    }

    #[test]
    fn stl_round_trip_preserves_volume_and_triangle_count() -> TestResult {
        let original = cube();
        let bytes = stl_bytes(&original)?;
        let read = read_stl_binary(&bytes)?;
        assert_eq!(read.triangle_count(), original.triangle_count());
        assert!(
            (read.signed_volume().abs() - 500.0).abs() < 1e-4,
            "volume was {}",
            read.signed_volume()
        );
        Ok(())
    }

    #[test]
    fn a_truncated_file_is_refused() -> TestResult {
        let mut bytes = stl_bytes(&cube())?;
        bytes.truncate(bytes.len() - 1);
        assert!(matches!(read_stl_binary(&bytes), Err(StlError::Invalid(_))));
        Ok(())
    }

    #[test]
    fn an_impossible_triangle_count_is_refused_before_allocating() -> TestResult {
        let mut bytes = vec![0u8; STL_PREFIX_BYTES];
        bytes[80..84].copy_from_slice(&u32::MAX.to_le_bytes());
        assert!(matches!(
            read_stl_binary(&bytes),
            Err(StlError::Invalid(message)) if message.contains("declares")
        ));
        Ok(())
    }

    #[test]
    fn a_mesh_with_an_out_of_range_index_is_not_written() {
        let mut mesh = cube();
        mesh.triangles[0] = [0, 1, 99];
        assert!(matches!(stl_bytes(&mesh), Err(StlError::Mesh(_))));
    }

    #[test]
    fn an_empty_file_is_refused() {
        assert!(read_stl_binary(&[]).is_err());
    }
}

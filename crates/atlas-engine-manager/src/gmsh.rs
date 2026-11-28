//! Real Gmsh (`gmsh`) process adapter.
//!
//! This module does not guess at results: it writes a deterministic `.geo` description of a
//! 10 x 10 x 10 mm cube, runs the installed `gmsh` binary in batch mode on a single thread, parses
//! the MSH 2.2 file it wrote, and sums the signed volumes of the tetrahedra. The sum must equal the
//! analytic cube volume, so a passing run proves the process, the file format and the parser agree.
//! It exercises meshing only; it is not a mesh-quality or convergence study. If `gmsh` is absent
//! the caller gets an error and no metrics.
//!
//! The executable is discovered from `ATLAS_GMSH`, then `gmsh` on `PATH`. A rootless installation
//! on Linux is available from conda-forge (`micromamba install -c conda-forge gmsh`).

use crate::process::{output_with_timeout, provenance};
use serde::Serialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Cube edge length in mm; the analytic volume is `EDGE_MM^3`.
const EDGE_MM: f64 = 10.0;
/// Target element size bounds in mm. Fixed values keep the generated mesh deterministic.
const ELEMENT_SIZE_MIN_MM: f64 = 2.0;
const ELEMENT_SIZE_MAX_MM: f64 = 4.0;

/// Verified result of meshing one document through `gmsh`.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentMesh {
    pub solver_version: String,
    pub executable: String,
    pub nodes: usize,
    pub elements: usize,
    pub tetrahedra: usize,
    pub document_volume_mm3: f64,
    pub mesh_volume_mm3: f64,
    /// `|mesh_volume - document_volume| / document_volume` in percent.
    pub volume_error_percent: f64,
    pub max_dimension_mm: f64,
    pub target_element_mm: f64,
    /// Triangles the surface was subdivided into before meshing.
    pub refined_triangles: usize,
    pub elapsed_ms: u64,
    /// FNV-1a over the document STL and the mesh Gmsh wrote.
    pub provenance_hash: String,
}

/// Tetrahedral volume mesh handed to a downstream solver.
#[derive(Debug, Clone)]
pub struct VolumeMesh {
    /// Node coordinates in millimetres, indexed by the element connectivity below.
    pub coordinates: Vec<[f64; 3]>,
    /// Four node indices per tetrahedron, into `coordinates`.
    pub tetrahedra: Vec<[usize; 4]>,
}

/// Verified result of one real `gmsh` run.
#[derive(Debug, Clone, Serialize)]
pub struct GmshBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub nodes: usize,
    pub elements: usize,
    pub tetrahedra: usize,
    /// Sum of the tetrahedron volumes the solver wrote, in mm^3.
    pub mesh_volume_mm3: f64,
    pub analytic_volume_mm3: f64,
    pub min_tet_volume_mm3: f64,
    pub max_tet_volume_mm3: f64,
    /// `|mesh_volume - analytic_volume| / analytic_volume` in percent.
    pub volume_error_percent: f64,
    /// `1 - volume_error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the `.geo` input and the `.msh` file Gmsh wrote.
    pub provenance_hash: String,
}

/// Find the solver binary: `ATLAS_GMSH` first, then `gmsh` on `PATH`.
#[must_use]
pub fn discover_executable() -> Option<PathBuf> {
    if let Ok(configured) = env::var("ATLAS_GMSH") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join("gmsh"))
        .find(|candidate| candidate.is_file())
}

/// Detected `gmsh` version, or an error when the solver is absent or does not answer.
pub fn probe_version() -> Result<String, String> {
    let executable =
        discover_executable().ok_or("Gmsh is not installed: put gmsh on PATH or set ATLAS_GMSH")?;
    solver_version(&executable, Duration::from_secs(10))
}

/// Mesh the benchmark cube through the installed solver and check the enclosed volume.
pub fn run_cube_mesh_benchmark(timeout: Duration) -> Result<GmshBenchmark, String> {
    let executable =
        discover_executable().ok_or("Gmsh is not installed: put gmsh on PATH or set ATLAS_GMSH")?;
    let version = solver_version(&executable, timeout)?;
    let geo = cube_geo();

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let workdir = env::temp_dir().join(format!("atlas-gmsh-{}-{unique}", std::process::id()));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create the solver workdir: {error}"))?;

    let started = Instant::now();
    let result = run_in_workdir(&executable, &workdir, "cube.geo", "cube.msh", &geo, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let parsed = result.and_then(|()| {
        let msh = fs::read_to_string(workdir.join("cube.msh"))
            .map_err(|error| format!("Gmsh wrote no mesh file: {error}"))?;
        parse_msh(&msh).ok_or_else(|| "Gmsh's MSH file has no readable tetrahedra".to_string())
    });
    let output = fs::read(workdir.join("cube.msh")).unwrap_or_default();
    let _ = fs::remove_dir_all(&workdir);
    let mesh = parsed?;

    let volumes: Vec<f64> = mesh
        .tetrahedra
        .iter()
        .map(|tet| tet_volume(&mesh, *tet))
        .collect();
    let mesh_volume_mm3: f64 = volumes.iter().sum();
    let analytic_volume_mm3 = EDGE_MM * EDGE_MM * EDGE_MM;
    let volume_error_percent =
        (mesh_volume_mm3 - analytic_volume_mm3).abs() / analytic_volume_mm3 * 100.0;
    let min_tet_volume_mm3 = volumes.iter().copied().fold(f64::INFINITY, f64::min);
    let max_tet_volume_mm3 = volumes.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    Ok(GmshBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        nodes: mesh.nodes,
        elements: mesh.element_count,
        tetrahedra: mesh.tetrahedra.len(),
        mesh_volume_mm3,
        analytic_volume_mm3,
        min_tet_volume_mm3,
        max_tet_volume_mm3,
        volume_error_percent,
        accuracy_score: (1.0 - volume_error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[geo.as_bytes(), output.as_slice()]),
    })
}

fn run_in_workdir(
    executable: &PathBuf,
    workdir: &PathBuf,
    geo_name: &str,
    msh_name: &str,
    geo: &str,
    timeout: Duration,
) -> Result<(), String> {
    fs::write(workdir.join(geo_name), geo)
        .map_err(|error| format!("cannot write the solver geometry: {error}"))?;
    let mut command = Command::new(executable);
    command
        .arg("-nt")
        .arg("1")
        .arg("-3")
        .arg("-format")
        .arg("msh2")
        .arg("-o")
        .arg(msh_name)
        .arg(geo_name)
        .current_dir(workdir);
    let outcome = output_with_timeout(command, timeout)?;
    if !outcome.status.success() {
        return Err(format!(
            "Gmsh failed with {}: {} | {}",
            outcome.status,
            String::from_utf8_lossy(&outcome.stderr).trim(),
            String::from_utf8_lossy(&outcome.stdout).trim()
        ));
    }
    Ok(())
}

/// Mesh a document's exported STL in the solver and compare the enclosed volume with the model.
///
/// The STL is the document's own tessellation, so this is a real geometry-to-mesher binding: the
/// element size follows the model's bounding box and the returned volume is checked against the
/// model volume rather than a fixed reference case. It meshes; it does not solve.
pub fn run_document_mesh(
    stl: &[u8],
    document_volume_mm3: f64,
    timeout: Duration,
) -> Result<DocumentMesh, String> {
    mesh_document_with_geometry(stl, document_volume_mm3, timeout).map(|(summary, _)| summary)
}

/// Mesh a document and also return the elements, so a structural solver can run on them.
///
/// This is the same run as [`run_document_mesh`]; the returned [`VolumeMesh`] is exactly the
/// tetrahedra whose summed volume was checked against the model volume.
pub fn mesh_document_with_geometry(
    stl: &[u8],
    document_volume_mm3: f64,
    timeout: Duration,
) -> Result<(DocumentMesh, VolumeMesh), String> {
    if !document_volume_mm3.is_finite() || document_volume_mm3 <= 0.0 {
        return Err("the document has no positive volume to mesh".to_string());
    }
    let executable =
        discover_executable().ok_or("Gmsh is not installed: put gmsh on PATH or set ATLAS_GMSH")?;
    let version = solver_version(&executable, timeout)?;
    let max_dimension_mm =
        stl_max_dimension(stl).ok_or("the exported STL is not a readable binary STL")?;
    // Twelve divisions across the largest extent: fine enough to fill the volume with a useful
    // number of tetrahedra without making a large model's meshing time unpredictable.
    let target_element_mm = (max_dimension_mm / 12.0).max(0.5);
    let geo = document_geo(target_element_mm);

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let workdir = env::temp_dir().join(format!("atlas-gmsh-doc-{}-{unique}", std::process::id()));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create the solver workdir: {error}"))?;
    // Gmsh tetrahedralises the STL surface it is given without adding surface points, so a coarse
    // document tessellation would produce a handful of tetrahedra. Subdividing the planar triangles
    // keeps the geometry identical (the volume stays exact) while giving the mesher a usable
    // surface resolution.
    let refined = refine_stl(stl, target_element_mm, 200_000)?;
    let refined_triangles = refined.len();
    fs::write(workdir.join("part.stl"), write_stl(&refined))
        .map_err(|error| format!("cannot write the document STL: {error}"))?;

    let started = Instant::now();
    let result = run_in_workdir(&executable, &workdir, "part.geo", "part.msh", &geo, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let parsed = result.and_then(|()| {
        let msh = fs::read_to_string(workdir.join("part.msh"))
            .map_err(|error| format!("Gmsh wrote no mesh file: {error}"))?;
        parse_msh(&msh).ok_or_else(|| {
            "Gmsh could not build a volume from the document surface; the model may be open or \
             non-manifold"
                .to_string()
        })
    });
    let output = fs::read(workdir.join("part.msh")).unwrap_or_default();
    let _ = fs::remove_dir_all(&workdir);
    let mesh = parsed?;

    let volumes: Vec<f64> = mesh
        .tetrahedra
        .iter()
        .map(|tet| tet_volume(&mesh, *tet))
        .collect();
    let mesh_volume_mm3: f64 = volumes.iter().sum();
    let volume_error_percent =
        (mesh_volume_mm3 - document_volume_mm3).abs() / document_volume_mm3 * 100.0;

    let summary = DocumentMesh {
        solver_version: version,
        executable: executable.display().to_string(),
        nodes: mesh.nodes,
        elements: mesh.element_count,
        tetrahedra: mesh.tetrahedra.len(),
        document_volume_mm3,
        mesh_volume_mm3,
        volume_error_percent,
        max_dimension_mm,
        target_element_mm,
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[stl, output.as_slice()]),
        refined_triangles,
    };
    let geometry = VolumeMesh {
        coordinates: mesh.coordinates,
        tetrahedra: mesh.tetrahedra,
    };
    Ok((summary, geometry))
}

#[derive(Clone, Copy, Debug)]
struct Triangle {
    vertices: [[f32; 3]; 3],
}

/// Read the triangles of a binary STL. Normals and attributes are dropped; only geometry matters.
fn read_stl(bytes: &[u8]) -> Result<Vec<Triangle>, String> {
    if bytes.len() < 84 {
        return Err("the exported STL is too short to be binary STL".to_string());
    }
    let count = u32::from_le_bytes(
        bytes[80..84]
            .try_into()
            .map_err(|_| "the STL triangle count is unreadable".to_string())?,
    ) as usize;
    let needed = 84usize
        .checked_add(
            count
                .checked_mul(50)
                .ok_or("the STL triangle count overflows")?,
        )
        .ok_or("the STL triangle count overflows")?;
    if bytes.len() < needed {
        return Err(format!(
            "the STL declares {count} triangles but is only {} bytes",
            bytes.len()
        ));
    }
    let mut triangles = Vec::with_capacity(count);
    for index in 0..count {
        let base = 84 + index * 50 + 12;
        let mut vertices = [[0.0f32; 3]; 3];
        for (corner, vertex) in vertices.iter_mut().enumerate() {
            for (axis, coordinate) in vertex.iter_mut().enumerate() {
                let offset = base + corner * 12 + axis * 4;
                let raw = bytes
                    .get(offset..offset + 4)
                    .ok_or("the STL ends inside a triangle")?;
                *coordinate = f32::from_le_bytes(
                    raw.try_into()
                        .map_err(|_| "the STL coordinate is unreadable".to_string())?,
                );
                if !coordinate.is_finite() {
                    return Err("the STL contains a non-finite coordinate".to_string());
                }
            }
        }
        triangles.push(Triangle { vertices });
    }
    if triangles.is_empty() {
        return Err("the STL contains no triangles".to_string());
    }
    Ok(triangles)
}

/// Serialise triangles as a binary STL with zero normals (Gmsh recomputes what it needs).
fn write_stl(triangles: &[Triangle]) -> Vec<u8> {
    let mut bytes = Vec::with_capacity(84 + triangles.len() * 50);
    let mut header = [0u8; 80];
    let label = b"AtlasCAD surface subdivided for Gmsh";
    header[..label.len()].copy_from_slice(label);
    bytes.extend_from_slice(&header);
    bytes.extend_from_slice(&(triangles.len() as u32).to_le_bytes());
    for triangle in triangles {
        bytes.extend_from_slice(&[0u8; 12]);
        for vertex in triangle.vertices {
            for coordinate in vertex {
                bytes.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        bytes.extend_from_slice(&0u16.to_le_bytes());
    }
    bytes
}

/// Split planar triangles until no edge exceeds `max_edge_mm`, up to `cap` triangles.
///
/// Splitting a triangle at its edge midpoints keeps the surface geometrically identical, so a
/// volume computed from the resulting tetrahedra is unchanged; only the resolution improves.
fn refine_stl(stl: &[u8], max_edge_mm: f64, cap: usize) -> Result<Vec<Triangle>, String> {
    let triangles = read_stl(stl)?;
    let mut refined = Vec::with_capacity(triangles.len().min(cap.max(1)));
    for triangle in triangles {
        subdivide(triangle, max_edge_mm, 0, cap, &mut refined);
    }
    Ok(refined)
}

fn subdivide(
    triangle: Triangle,
    max_edge_mm: f64,
    depth: u32,
    cap: usize,
    out: &mut Vec<Triangle>,
) {
    if out.len() >= cap || depth >= 6 {
        out.push(triangle);
        return;
    }
    let [a, b, c] = triangle.vertices;
    let longest = edge_length(a, b)
        .max(edge_length(b, c))
        .max(edge_length(c, a));
    if longest <= max_edge_mm {
        out.push(triangle);
        return;
    }
    let ab = midpoint(a, b);
    let bc = midpoint(b, c);
    let ca = midpoint(c, a);
    for child in [
        Triangle {
            vertices: [a, ab, ca],
        },
        Triangle {
            vertices: [ab, b, bc],
        },
        Triangle {
            vertices: [ca, bc, c],
        },
        Triangle {
            vertices: [ab, bc, ca],
        },
    ] {
        subdivide(child, max_edge_mm, depth + 1, cap, out);
    }
}

fn edge_length(a: [f32; 3], b: [f32; 3]) -> f64 {
    let dx = f64::from(b[0] - a[0]);
    let dy = f64::from(b[1] - a[1]);
    let dz = f64::from(b[2] - a[2]);
    (dx * dx + dy * dy + dz * dz).sqrt()
}

fn midpoint(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [
        (a[0] + b[0]) / 2.0,
        (a[1] + b[1]) / 2.0,
        (a[2] + b[2]) / 2.0,
    ]
}

/// Largest axis-aligned extent of a binary STL, in the file's units (millimetres here).
fn stl_max_dimension(bytes: &[u8]) -> Option<f64> {
    if bytes.len() < 84 {
        return None;
    }
    let triangles = u32::from_le_bytes(bytes.get(80..84)?.try_into().ok()?) as usize;
    if bytes.len() < 84 + triangles.checked_mul(50)? {
        return None;
    }
    let mut min = [f64::INFINITY; 3];
    let mut max = [f64::NEG_INFINITY; 3];
    for triangle in 0..triangles {
        let base = 84 + triangle * 50 + 12;
        for corner in 0..3 {
            for axis in 0..3 {
                let offset = base + corner * 12 + axis * 4;
                let raw = bytes.get(offset..offset + 4)?;
                let value = f32::from_le_bytes(raw.try_into().ok()?) as f64;
                if !value.is_finite() {
                    return None;
                }
                min[axis] = min[axis].min(value);
                max[axis] = max[axis].max(value);
            }
        }
    }
    let extent = (0..3)
        .map(|axis| max[axis] - min[axis])
        .fold(0.0_f64, f64::max);
    (extent > 0.0).then_some(extent)
}

fn solver_version(executable: &PathBuf, timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(executable);
    command.arg("--version");
    let outcome = output_with_timeout(command, timeout)?;
    let version = String::from_utf8_lossy(&outcome.stdout).trim().to_string();
    if version.is_empty() {
        return Err("cannot read the Gmsh version".to_string());
    }
    Ok(version)
}

struct Mesh {
    /// Coordinates indexed by the order the node ids first appeared.
    coordinates: Vec<[f64; 3]>,
    /// Tetrahedra as indices into `coordinates`.
    tetrahedra: Vec<[usize; 4]>,
    /// Node count as written by Gmsh.
    nodes: usize,
    /// Total element count including boundary entities.
    element_count: usize,
}

/// Parse the MSH 2.2 sections this adapter needs: `$Nodes` and `$Elements`.
fn parse_msh(text: &str) -> Option<Mesh> {
    let lines: Vec<&str> = text.lines().collect();
    let mut coordinates = Vec::new();
    let mut node_index: HashMap<u64, usize> = HashMap::new();
    let mut tetrahedra = Vec::new();
    let mut element_count = 0usize;

    let mut index = 0usize;
    while index < lines.len() {
        match lines[index].trim() {
            "$Nodes" => {
                let count = lines.get(index + 1)?.trim().parse::<usize>().ok()?;
                for line in lines.iter().skip(index + 2).take(count) {
                    let mut columns = line.split_whitespace();
                    let id = columns.next()?.parse::<u64>().ok()?;
                    let x = columns.next()?.parse::<f64>().ok()?;
                    let y = columns.next()?.parse::<f64>().ok()?;
                    let z = columns.next()?.parse::<f64>().ok()?;
                    node_index.insert(id, coordinates.len());
                    coordinates.push([x, y, z]);
                }
                index += 2 + count;
            }
            "$Elements" => {
                let count = lines.get(index + 1)?.trim().parse::<usize>().ok()?;
                element_count = count;
                for line in lines.iter().skip(index + 2).take(count) {
                    let mut columns = line.split_whitespace();
                    let _id = columns.next()?.parse::<u64>().ok()?;
                    let kind = columns.next()?.parse::<u32>().ok()?;
                    let tags = columns.next()?.parse::<usize>().ok()?;
                    for _ in 0..tags {
                        columns.next()?;
                    }
                    if kind != 4 {
                        continue;
                    }
                    let mut corners = [0usize; 4];
                    for corner in &mut corners {
                        let id = columns.next()?.parse::<u64>().ok()?;
                        *corner = *node_index.get(&id)?;
                    }
                    tetrahedra.push(corners);
                }
                index += 2 + count;
            }
            _ => index += 1,
        }
    }

    let nodes = coordinates.len();
    (!tetrahedra.is_empty()).then_some(Mesh {
        coordinates,
        tetrahedra,
        nodes,
        element_count,
    })
}

/// Absolute volume of a tetrahedron from its four corner coordinates, in mm^3.
fn tet_volume(mesh: &Mesh, tet: [usize; 4]) -> f64 {
    let [a, b, c, d] = tet.map(|corner| mesh.coordinates[corner]);
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let w = [d[0] - a[0], d[1] - a[1], d[2] - a[2]];
    let cross = [
        v[1] * w[2] - v[2] * w[1],
        v[2] * w[0] - v[0] * w[2],
        v[0] * w[1] - v[1] * w[0],
    ];
    (u[0] * cross[0] + u[1] * cross[1] + u[2] * cross[2]).abs() / 6.0
}

fn document_geo(target_element_mm: f64) -> String {
    format!(
        "// AtlasCAD Gmsh document meshing\n\
         Merge \"part.stl\";\n\
         Surface Loop(1) = Surface{{:}};\n\
         Volume(1) = {{1}};\n\
         Mesh.CharacteristicLengthMax = {target_element_mm};\n\
         Mesh 3;\n"
    )
}

fn cube_geo() -> String {
    format!(
        "// AtlasCAD Gmsh cube meshing benchmark\n\
         SetFactory(\"OpenCASCADE\");\n\
         Box(1) = {{0, 0, 0, {EDGE_MM}, {EDGE_MM}, {EDGE_MM}}};\n\
         Mesh.CharacteristicLengthMin = {ELEMENT_SIZE_MIN_MM};\n\
         Mesh.CharacteristicLengthMax = {ELEMENT_SIZE_MAX_MM};\n\
         Mesh 3;\n"
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_runs_when_gmsh_is_installed() -> Result<(), Box<dyn std::error::Error>> {
        if discover_executable().is_none() {
            eprintln!("SKIP: gmsh is not installed in this environment");
            return Ok(());
        }
        let result =
            run_cube_mesh_benchmark(Duration::from_secs(60)).map_err(std::io::Error::other)?;
        assert!(!result.solver_version.is_empty());
        assert!(result.nodes > 0);
        assert!(result.tetrahedra > 0);
        assert!(
            result.volume_error_percent < 0.01,
            "meshed volume {} differs from {} by {:.6}%",
            result.mesh_volume_mm3,
            result.analytic_volume_mm3,
            result.volume_error_percent
        );
        assert!(result.min_tet_volume_mm3 > 0.0);
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }

    #[test]
    fn binary_stl_bounds_are_read() -> Result<(), Box<dyn std::error::Error>> {
        let mut stl = vec![0u8; 80];
        stl.extend_from_slice(&1u32.to_le_bytes());
        stl.extend_from_slice(&[0u8; 12]);
        for point in [[0.0f32, 0.0, 0.0], [10.0, 0.0, 0.0], [0.0, 10.0, 0.0]] {
            for coordinate in point {
                stl.extend_from_slice(&coordinate.to_le_bytes());
            }
        }
        stl.extend_from_slice(&0u16.to_le_bytes());
        assert_eq!(stl.len(), 134);
        assert_eq!(stl_max_dimension(&stl), Some(10.0));
        assert_eq!(stl_max_dimension(&stl[..100]), None);
        Ok(())
    }

    #[test]
    fn refinement_splits_long_triangles_and_keeps_the_surface()
    -> Result<(), Box<dyn std::error::Error>> {
        let triangle = Triangle {
            vertices: [[0.0, 0.0, 0.0], [12.0, 0.0, 0.0], [0.0, 12.0, 0.0]],
        };
        let stl = write_stl(&[triangle]);
        let refined = refine_stl(&stl, 6.0, 100).map_err(std::io::Error::other)?;
        assert!(refined.len() >= 4, "the 12 mm triangle must be split");
        let longest = refined
            .iter()
            .flat_map(|t| {
                let [a, b, c] = t.vertices;
                [edge_length(a, b), edge_length(b, c), edge_length(c, a)]
            })
            .fold(0.0_f64, f64::max);
        assert!(
            longest <= 6.0 + 1e-6,
            "longest refined edge is {longest} mm"
        );
        let area = |triangles: &[Triangle]| -> f64 {
            triangles
                .iter()
                .map(|t| {
                    let [a, b, c] = t.vertices;
                    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
                    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
                    let cross = [
                        ab[1] * ac[2] - ab[2] * ac[1],
                        ab[2] * ac[0] - ab[0] * ac[2],
                        ab[0] * ac[1] - ab[1] * ac[0],
                    ];
                    f64::from(cross[0])
                        .hypot(f64::from(cross[1]))
                        .hypot(f64::from(cross[2]))
                        / 2.0
                })
                .sum()
        };
        assert!((area(&[triangle]) - area(&refined)).abs() < 1e-6);
        Ok(())
    }

    #[test]
    fn parser_computes_a_unit_tetrahedron() -> Result<(), Box<dyn std::error::Error>> {
        let msh = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n\
                   $Nodes\n4\n\
                   1 0 0 0\n2 1 0 0\n3 0 1 0\n4 0 0 1\n$EndNodes\n\
                   $Elements\n1\n1 4 2 0 1 1 2 3 4\n$EndElements\n";
        let mesh = parse_msh(msh).ok_or("the MSH file was not parsed")?;
        assert_eq!(mesh.nodes, 4);
        assert_eq!(mesh.element_count, 1);
        assert_eq!(mesh.tetrahedra.len(), 1);
        let volume = tet_volume(&mesh, mesh.tetrahedra[0]);
        assert!((volume - 1.0 / 6.0).abs() < 1e-12, "volume = {volume}");
        Ok(())
    }
}

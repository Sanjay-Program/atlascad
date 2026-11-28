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
    let result = run_in_workdir(&executable, &workdir, &geo, timeout);
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
    geo: &str,
    timeout: Duration,
) -> Result<(), String> {
    fs::write(workdir.join("cube.geo"), geo)
        .map_err(|error| format!("cannot write the solver geometry: {error}"))?;
    let mut command = Command::new(executable);
    command
        .arg("-nt")
        .arg("1")
        .arg("-3")
        .arg("-format")
        .arg("msh2")
        .arg("-o")
        .arg("cube.msh")
        .arg("cube.geo")
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

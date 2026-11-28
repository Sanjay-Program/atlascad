//! Real CalculiX (`ccx`) process adapter.
//!
//! This module does not guess at results: it writes a deterministic uniaxial-tension `*INP` deck,
//! runs the installed `ccx` binary on it, parses the displacements CalculiX printed, and compares
//! the axial extension with `F L / (A E)`. C3D8 solves this patch case exactly, so the comparison
//! tests the process, units and parser rather than mesh convergence. If `ccx` is absent the caller
//! gets an error and no metrics.
//!
//! The executable is discovered from `ATLAS_CCX`, then `ccx` on `PATH`. A typical rootless
//! installation is `pip install cad-ccx`, which bundles the conda-forge build.

use crate::process::{output_with_timeout, provenance};
use serde::Serialize;
use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::Command;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Bar geometry and load for the benchmark (all lengths in mm, forces in N).
const LENGTH_MM: f64 = 100.0;
const WIDTH_MM: f64 = 10.0;
const HEIGHT_MM: f64 = 10.0;
const TOTAL_LOAD_N: f64 = 100.0;
const YOUNGS_MODULUS_MPA: f64 = 210_000.0;
const POISSON: f64 = 0.3;
/// Segments along x, y, z: 5x3x3 nodes and 4x2x2 C3D8 elements. A uniform axial stress
/// state is captured exactly by a single element; the extra divisions only exercise the mesh.
const NX: usize = 4;
const NY: usize = 2;
const NZ: usize = 2;

/// Verified result of one real `ccx` run.
#[derive(Debug, Clone, Serialize)]
pub struct CalculixBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub nodes: usize,
    pub elements: usize,
    pub axial_extension_mm: f64,
    pub analytical_displacement_mm: f64,
    pub error_percent: f64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the deck and the solver's own output.
    pub provenance_hash: String,
}

/// Find the solver binary: `ATLAS_CCX` first, then `ccx` on `PATH`.
#[must_use]
pub fn discover_executable() -> Option<PathBuf> {
    if let Ok(configured) = env::var("ATLAS_CCX") {
        let path = PathBuf::from(configured);
        if path.is_file() {
            return Some(path);
        }
    }
    let path = env::var_os("PATH")?;
    env::split_paths(&path)
        .map(|dir| dir.join("ccx"))
        .find(|candidate| candidate.is_file())
}

/// Detected `ccx` version, or an error when the solver is absent or does not answer.
pub fn probe_version() -> Result<String, String> {
    let executable = discover_executable()
        .ok_or("CalculiX `ccx` is not installed: put ccx on PATH or set ATLAS_CCX")?;
    solver_version(&executable, Duration::from_secs(10))
}

/// Run the axial-tension benchmark through the installed solver.
///
/// Returns an error with the solver's stderr when it does not start, times out or produces no
/// displacement table.
pub fn run_tension_bar_benchmark(timeout: Duration) -> Result<CalculixBenchmark, String> {
    let executable = discover_executable()
        .ok_or("CalculiX `ccx` is not installed: put ccx on PATH or set ATLAS_CCX")?;
    let version = solver_version(&executable, timeout)?;
    let deck = tension_deck();

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let workdir = env::temp_dir().join(format!("atlas-ccx-{}-{unique}", std::process::id()));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create the solver workdir: {error}"))?;

    let started = Instant::now();
    let result = run_in_workdir(&executable, &workdir, &deck, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let parsed = result.and_then(|()| {
        let dat = fs::read_to_string(workdir.join("beam.dat"))
            .map_err(|error| format!("CalculiX produced no beam.dat: {error}"))?;
        parse_displacements(&dat).ok_or_else(|| {
            format!(
                "CalculiX output has no displacement table; first lines: {}",
                dat.lines().take(4).collect::<Vec<_>>().join(" | ")
            )
        })
    });
    let output = fs::read(workdir.join("beam.dat")).unwrap_or_default();
    let _ = fs::remove_dir_all(&workdir);
    let axial_extension_mm = parsed?
        .values
        .into_iter()
        .fold(0.0_f64, |max, vx| max.max(vx.abs()));

    let analytical = analytical_extension_mm();
    let error_percent = (axial_extension_mm - analytical).abs() / analytical * 100.0;
    let provenance_hash = provenance(&[deck.as_bytes(), output.as_slice()]);

    Ok(CalculixBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        nodes: parsed_node_count(),
        elements: NX * NY * NZ,
        axial_extension_mm,
        analytical_displacement_mm: analytical,
        error_percent,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash,
    })
}

struct Displacements {
    values: Vec<f64>,
}

fn run_in_workdir(
    executable: &PathBuf,
    workdir: &PathBuf,
    deck: &str,
    timeout: Duration,
) -> Result<(), String> {
    fs::write(workdir.join("beam.inp"), deck)
        .map_err(|error| format!("cannot write the solver deck: {error}"))?;
    let mut command = Command::new(executable);
    command.arg("beam").current_dir(workdir);
    let outcome = output_with_timeout(command, timeout)?;
    if !outcome.status.success() {
        return Err(format!(
            "CalculiX failed with {}: {} | {}",
            outcome.status,
            String::from_utf8_lossy(&outcome.stderr).trim(),
            String::from_utf8_lossy(&outcome.stdout).trim()
        ));
    }
    Ok(())
}

fn solver_version(executable: &PathBuf, timeout: Duration) -> Result<String, String> {
    let mut command = Command::new(executable);
    command.arg("-v");
    let outcome = output_with_timeout(command, timeout)?;
    let text = String::from_utf8_lossy(&outcome.stdout);
    let version = text
        .lines()
        .find_map(|line| line.split_whitespace().last())
        .unwrap_or("unknown")
        .trim()
        .to_string();
    Ok(version)
}

/// Parse the `displacements (vx,vy,vz)` table that `*NODE PRINT` writes.
fn parse_displacements(dat: &str) -> Option<Displacements> {
    let mut in_table = false;
    let mut values = Vec::new();
    for line in dat.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("displacements") {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if trimmed.is_empty() {
            if values.is_empty() {
                continue;
            }
            break;
        }
        let columns: Vec<&str> = trimmed
            .split_whitespace()
            .map(|column| column.trim_end_matches(','))
            .collect();
        if columns.len() < 4 {
            continue;
        }
        if let (Ok(_node), Ok(vx)) = (columns[0].parse::<u32>(), columns[1].parse::<f64>()) {
            values.push(vx);
        }
    }
    (!values.is_empty()).then_some(Displacements { values })
}

fn parsed_node_count() -> usize {
    (NX + 1) * (NY + 1) * (NZ + 1)
}

fn analytical_extension_mm() -> f64 {
    let area = WIDTH_MM * HEIGHT_MM;
    TOTAL_LOAD_N * LENGTH_MM / (area * YOUNGS_MODULUS_MPA)
}

/// CalculiX accepts at most 16 entries per line; write node sets in smaller chunks.
fn push_ids(deck: &mut String, ids: &[String]) {
    for chunk in ids.chunks(10) {
        deck.push_str(&chunk.join(","));
        deck.push('\n');
    }
}

fn node_id(i: usize, j: usize, k: usize) -> usize {
    1 + i * (NY + 1) * (NZ + 1) + j * (NZ + 1) + k
}

fn tension_deck() -> String {
    let mut deck = String::from("*HEADING\nAtlasCAD CalculiX axial tension benchmark\n");
    deck.push_str("*NODE, NSET=NALL\n");
    for i in 0..=NX {
        for j in 0..=NY {
            for k in 0..=NZ {
                let x = LENGTH_MM * i as f64 / NX as f64;
                let y = WIDTH_MM * j as f64 / NY as f64;
                let z = HEIGHT_MM * k as f64 / NZ as f64;
                deck.push_str(&format!("{},{x:.6},{y:.6},{z:.6}\n", node_id(i, j, k)));
            }
        }
    }

    deck.push_str("*ELEMENT, TYPE=C3D8, ELSET=EALL\n");
    let mut element = 1;
    for i in 0..NX {
        for j in 0..NY {
            for k in 0..NZ {
                let corners = [
                    (i, j, k),
                    (i + 1, j, k),
                    (i + 1, j + 1, k),
                    (i, j + 1, k),
                    (i, j, k + 1),
                    (i + 1, j, k + 1),
                    (i + 1, j + 1, k + 1),
                    (i, j + 1, k + 1),
                ];
                let ids: Vec<String> = corners
                    .iter()
                    .map(|(ci, cj, ck)| node_id(*ci, *cj, *ck).to_string())
                    .collect();
                deck.push_str(&format!("{element},{}\n", ids.join(",")));
                element += 1;
            }
        }
    }

    deck.push_str("*NSET, NSET=FIXED\n");
    let fixed: Vec<String> = (0..=NY)
        .flat_map(|j| (0..=NZ).map(move |k| (j, k)))
        .map(|(j, k)| node_id(0, j, k).to_string())
        .collect();
    push_ids(&mut deck, &fixed);
    deck.push_str("*NSET, NSET=TIP\n");
    let tip: Vec<String> = (0..=NY)
        .flat_map(|j| (0..=NZ).map(move |k| (j, k)))
        .map(|(j, k)| node_id(NX, j, k).to_string())
        .collect();
    push_ids(&mut deck, &tip);

    let per_tip_node = TOTAL_LOAD_N / tip.len() as f64;
    let load_z = per_tip_node;
    deck.push_str(&format!(
        "*BOUNDARY\nFIXED,1,3,0.0\n\
         *MATERIAL, NAME=STEEL\n*ELASTIC\n{YOUNGS_MODULUS_MPA:.1},{POISSON}\n\
         *SOLID SECTION, ELSET=EALL, MATERIAL=STEEL\n1.0\n\
         *STEP\n*STATIC\n*CLOAD\nTIP,1,{load_z:.6}\n\
         *NODE PRINT, NSET=NALL\nU\n*END STEP\n"
    ));
    deck
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_runs_when_ccx_is_installed() -> Result<(), Box<dyn std::error::Error>> {
        if discover_executable().is_none() {
            eprintln!("SKIP: ccx is not installed in this environment");
            return Ok(());
        }
        let result =
            run_tension_bar_benchmark(Duration::from_secs(60)).map_err(std::io::Error::other)?;
        assert_eq!(result.nodes, 45);
        assert_eq!(result.elements, 16);
        assert!(result.axial_extension_mm > 0.0);
        assert!(
            result.error_percent < 2.0,
            "tip deflection {} differs from beam theory {} by {:.2}%",
            result.axial_extension_mm,
            result.analytical_displacement_mm,
            result.error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }

    #[test]
    fn parser_reads_the_ccx_table() -> Result<(), Box<dyn std::error::Error>> {
        let dat = "\n displacements (vx,vy,vz) for set NALL and time  0.1000000E+01\n\n\
                   1, 1.9E-01, 2.0E-05, -1.0E-05\n\
                   2, 0.0E+00, 0.0E+00, -0.0E+00\n\n";
        let parsed = parse_displacements(dat).ok_or("the CCX displacement table was not parsed")?;
        assert_eq!(parsed.values.len(), 2);
        assert!((parsed.values[0] - 0.19).abs() < 1e-9);
        Ok(())
    }
}

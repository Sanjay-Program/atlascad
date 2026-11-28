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
use std::collections::HashMap;
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

/// Upper bound on the tetrahedra this adapter will submit in one deck.
///
/// Well beyond any document the studio meshes at its default element size, but it stops a
/// pathological model from generating a deck the solver would grind on for minutes.
const MAX_SOLVE_TETS: usize = 400_000;

/// Verified result of solving a document's own tetrahedral mesh.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentSolve {
    pub solver_version: String,
    pub executable: String,
    pub nodes: usize,
    pub elements: usize,
    /// Pressure applied normal to the highest face, in N/mm^2 (MPa).
    pub pressure_mpa: f64,
    /// Area of the boundary faces lying on the highest plane, from the solved mesh itself.
    pub loaded_area_mm2: f64,
    /// `pressure_mpa * loaded_area_mm2`.
    pub applied_force_n: f64,
    pub max_displacement_mm: f64,
    /// Largest `|uz|`: the quantity comparable with `P L / E` on a prismatic body. The vector
    /// maximum above also contains Poisson contraction, so it is reported separately.
    pub max_axial_displacement_mm: f64,
    pub mean_loaded_displacement_mm: f64,
    /// Magnitude of the summed vertical reaction at the clamped face.
    pub reaction_force_n: f64,
    /// `|reaction - applied| / applied` in percent: the solved equilibrium residual.
    pub equilibrium_error_percent: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the deck and the solver's own output.
    pub provenance_hash: String,
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

/// Solve a document's tetrahedral volume mesh under a uniform pressure on its highest face.
///
/// The mesh is the one Gmsh produced for the document, so this runs on the model's own geometry
/// rather than a reference part. The clamp is the lowest plane of the mesh and the load is the
/// consistent nodal force of `pressure_mpa` over the boundary triangles lying on the highest
/// plane, so the total applied force is `pressure * area` exactly. Linear tets represent a
/// uniform stress state exactly, which is why the prismatic case reproduces `P L / E`; on a
/// general shape the reported numbers are what the solver computed, not a closed-form guess.
///
/// Equilibrium is checked against the solver's own reactions: the summed vertical reaction at
/// the clamp must balance the applied force, and the residual is returned.
pub fn run_document_solve(
    coordinates: &[[f64; 3]],
    tetrahedra: &[[usize; 4]],
    pressure_mpa: f64,
    timeout: Duration,
) -> Result<DocumentSolve, String> {
    if !pressure_mpa.is_finite() || pressure_mpa <= 0.0 {
        return Err("the pressure must be a positive number of N/mm^2".to_string());
    }
    if tetrahedra.is_empty() {
        return Err("the mesh has no tetrahedra to solve".to_string());
    }
    if tetrahedra.len() > MAX_SOLVE_TETS {
        return Err(format!(
            "the mesh has {} tetrahedra, above the {} this adapter will submit",
            tetrahedra.len(),
            MAX_SOLVE_TETS
        ));
    }
    if coordinates.is_empty() {
        return Err("the mesh has no nodes to solve".to_string());
    }
    let executable = discover_executable()
        .ok_or("CalculiX `ccx` is not installed: put ccx on PATH or set ATLAS_CCX")?;
    let version = solver_version(&executable, timeout)?;

    let patch = classify_document_nodes(coordinates, tetrahedra)?;
    let applied_force_n = pressure_mpa * patch.loaded_area_mm2;
    if applied_force_n <= 0.0 {
        return Err("the loaded face has no area".to_string());
    }
    let deck = document_deck(
        coordinates,
        tetrahedra,
        &patch.loaded,
        &patch.fixed,
        pressure_mpa,
    );

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let workdir = env::temp_dir().join(format!("atlas-ccx-doc-{}-{unique}", std::process::id()));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create the solver workdir: {error}"))?;

    let started = Instant::now();
    let result = run_in_workdir_named(&executable, &workdir, "part", &deck, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let parsed = result.and_then(|()| {
        let dat = fs::read_to_string(workdir.join("part.dat"))
            .map_err(|error| format!("CalculiX produced no part.dat: {error}"))?;
        Ok(dat)
    });
    let dat = parsed?;
    let output = dat.as_bytes();
    let _ = fs::remove_dir_all(&workdir);

    let displacements = parse_nodal_table(&dat, "displacements")
        .ok_or("CalculiX printed no displacement table for the document mesh")?;
    let reactions = parse_nodal_table(&dat, "forces")
        .ok_or("CalculiX printed no reaction-force table for the clamped face")?;

    let mut max_displacement_mm = 0.0_f64;
    let mut max_axial_displacement_mm = 0.0_f64;
    for (_, [ux, uy, uz]) in &displacements {
        max_displacement_mm = max_displacement_mm.max(ux.hypot(*uy).hypot(*uz));
        max_axial_displacement_mm = max_axial_displacement_mm.max(uz.abs());
    }
    let loaded_ids: std::collections::HashSet<u32> = patch.loaded.keys().copied().collect();
    let mut loaded_sum = 0.0_f64;
    let mut loaded_count = 0usize;
    for (node, [_, _, uz]) in &displacements {
        if loaded_ids.contains(node) {
            loaded_sum += uz.abs();
            loaded_count += 1;
        }
    }
    let mean_loaded_displacement_mm = if loaded_count == 0 {
        0.0
    } else {
        loaded_sum / loaded_count as f64
    };
    let reaction_force_n = reactions
        .iter()
        .map(|(_, [_, _, fz])| fz)
        .sum::<f64>()
        .abs();
    let equilibrium_error_percent =
        (reaction_force_n - applied_force_n).abs() / applied_force_n * 100.0;

    Ok(DocumentSolve {
        solver_version: version,
        executable: executable.display().to_string(),
        nodes: coordinates.len(),
        elements: tetrahedra.len(),
        pressure_mpa,
        loaded_area_mm2: patch.loaded_area_mm2,
        applied_force_n,
        max_displacement_mm,
        max_axial_displacement_mm,
        mean_loaded_displacement_mm,
        reaction_force_n,
        equilibrium_error_percent,
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[deck.as_bytes(), output]),
    })
}

/// Split the mesh into clamped nodes, loaded nodes with their consistent load weights, and the
/// loaded area.
///
/// Boundary faces are the triangles used by exactly one tetrahedron. The loaded face is the set of
/// those lying on the mesh's highest plane; a face contributes a third of its area to each of its
/// nodes, which is the consistent nodal load for a uniform pressure on a linear triangle.
/// The clamp and the loaded patch derived from a document's tetrahedral mesh.
struct DocumentPatch {
    /// Loaded node id (1-based) to its consistent load weight in mm².
    loaded: HashMap<u32, f64>,
    /// Clamped node ids (1-based).
    fixed: Vec<u32>,
    loaded_area_mm2: f64,
}

fn classify_document_nodes(
    coordinates: &[[f64; 3]],
    tetrahedra: &[[usize; 4]],
) -> Result<DocumentPatch, String> {
    let max_z = coordinates
        .iter()
        .map(|c| c[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let min_z = coordinates
        .iter()
        .map(|c| c[2])
        .fold(f64::INFINITY, f64::min);
    let extent = (max_z - min_z).abs().max(1.0);
    let tolerance = extent * 1e-6;

    let mut counts: HashMap<[usize; 3], usize> = HashMap::new();
    for tet in tetrahedra {
        for face in [[0usize, 1, 2], [0, 1, 3], [0, 2, 3], [1, 2, 3]] {
            let mut key = [tet[face[0]], tet[face[1]], tet[face[2]]];
            key.sort_unstable();
            *counts.entry(key).or_insert(0) += 1;
        }
    }

    let mut weights: HashMap<u32, f64> = HashMap::new();
    let mut loaded_area_mm2 = 0.0_f64;
    for (face, count) in &counts {
        if *count != 1 {
            continue;
        }
        let corners = face.map(|index| coordinates[index]);
        if !corners.iter().all(|c| (c[2] - max_z).abs() <= tolerance) {
            continue;
        }
        let area = triangle_area(corners);
        if area <= 0.0 {
            continue;
        }
        loaded_area_mm2 += area;
        for index in face {
            *weights.entry((*index + 1) as u32).or_insert(0.0) += area / 3.0;
        }
    }

    let fixed: Vec<u32> = coordinates
        .iter()
        .enumerate()
        .filter(|(_, c)| (c[2] - min_z).abs() <= tolerance)
        .map(|(index, _)| (index + 1) as u32)
        .collect();

    if weights.is_empty() {
        return Err(
            "no boundary face lies on the highest plane, so there is nothing to load".to_string(),
        );
    }
    if fixed.is_empty() {
        return Err("no node lies on the lowest plane, so the model cannot be clamped".to_string());
    }
    if fixed.len() == coordinates.len() {
        return Err("the model is a flat sheet: every node lies on the clamped plane".to_string());
    }
    if (max_z - min_z).abs() <= tolerance {
        return Err("the model has no height to compress".to_string());
    }
    Ok(DocumentPatch {
        loaded: weights,
        fixed,
        loaded_area_mm2,
    })
}

fn triangle_area(corners: [[f64; 3]; 3]) -> f64 {
    let [a, b, c] = corners;
    let ab = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let ac = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let cross = [
        ab[1] * ac[2] - ab[2] * ac[1],
        ab[2] * ac[0] - ab[0] * ac[2],
        ab[0] * ac[1] - ab[1] * ac[0],
    ];
    cross[0].hypot(cross[1]).hypot(cross[2]) / 2.0
}

fn document_deck(
    coordinates: &[[f64; 3]],
    tetrahedra: &[[usize; 4]],
    loaded: &HashMap<u32, f64>,
    fixed: &[u32],
    pressure_mpa: f64,
) -> String {
    let mut deck = String::from("*HEADING\nAtlasCAD CalculiX document solve\n*NODE, NSET=NALL\n");
    for (index, [x, y, z]) in coordinates.iter().enumerate() {
        deck.push_str(&format!("{},{x:.9},{y:.9},{z:.9}\n", index + 1));
    }
    deck.push_str("*ELEMENT, TYPE=C3D4, ELSET=EALL\n");
    for (index, tet) in tetrahedra.iter().enumerate() {
        deck.push_str(&format!(
            "{},{},{},{},{}\n",
            index + 1,
            tet[0] + 1,
            tet[1] + 1,
            tet[2] + 1,
            tet[3] + 1
        ));
    }

    let fixed_ids: Vec<String> = fixed.iter().map(ToString::to_string).collect();
    deck.push_str("*NSET, NSET=FIXED\n");
    push_ids(&mut deck, &fixed_ids);
    let mut loaded_ids: Vec<u32> = loaded.keys().copied().collect();
    loaded_ids.sort_unstable();
    let loaded_strings: Vec<String> = loaded_ids.iter().map(ToString::to_string).collect();
    deck.push_str("*NSET, NSET=LOADED\n");
    push_ids(&mut deck, &loaded_strings);

    deck.push_str(&format!(
        "*BOUNDARY\nFIXED,1,3,0.0\n\
         *MATERIAL, NAME=STEEL\n*ELASTIC\n{YOUNGS_MODULUS_MPA:.1},{POISSON}\n\
         *SOLID SECTION, ELSET=EALL, MATERIAL=STEEL\n1.0\n\
         *STEP\n*STATIC\n*CLOAD\n"
    ));
    for node in loaded_ids {
        let force = pressure_mpa * loaded[&node];
        deck.push_str(&format!("{node},3,{:.9}\n", -force));
    }
    deck.push_str("*NODE PRINT, NSET=LOADED\nU\n*NODE PRINT, NSET=FIXED\nRF\n*END STEP\n");
    deck
}

fn run_in_workdir_named(
    executable: &PathBuf,
    workdir: &PathBuf,
    stem: &str,
    deck: &str,
    timeout: Duration,
) -> Result<(), String> {
    fs::write(workdir.join(format!("{stem}.inp")), deck)
        .map_err(|error| format!("cannot write the solver deck: {error}"))?;
    let mut command = Command::new(executable);
    command.arg(stem).current_dir(workdir);
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

/// Verified result of a steady heat-conduction solve on a document's mesh.
#[derive(Debug, Clone, Serialize)]
pub struct DocumentThermal {
    pub solver_version: String,
    pub executable: String,
    pub nodes: usize,
    pub elements: usize,
    /// Temperature imposed on the lowest face, in °C.
    pub cold_face_c: f64,
    /// Temperature imposed on the highest face, in °C.
    pub hot_face_c: f64,
    pub loaded_area_mm2: f64,
    /// Heat conducted into the cold face, from the solver's own reaction flux, in W.
    pub conducted_heat_w: f64,
    /// `k A ΔT / L` for the prismatic case, in W.
    pub prismatic_reference_heat_w: f64,
    /// `|conducted - reference| / reference` in percent.
    pub reference_error_percent: f64,
    pub min_temperature_c: f64,
    pub max_temperature_c: f64,
    /// Largest deviation of the solved field from the linear through-thickness profile, as a
    /// percentage of the applied difference. Zero for a prismatic body; a shape effect otherwise.
    pub linear_profile_residual_percent: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the deck and the solver's own output.
    pub provenance_hash: String,
}

/// Thermal conductivity of the benchmark material in W/(mm·K): 50 W/(m·K) steel.
const THERMAL_CONDUCTIVITY_W_PER_MM_K: f64 = 0.05;

/// Solve steady heat conduction through a document's tetrahedral mesh.
///
/// The lowest face is held at 0 °C and the highest at `delta_c`, with the sides insulated. The
/// conducted heat is the solver's reaction flux at the cold face, and it is compared with
/// `k A ΔT / L` for the prismatic case. The through-thickness profile is also checked, which linear
/// tetrahedra reproduce exactly for a body of constant cross-section.
pub fn run_document_thermal(
    coordinates: &[[f64; 3]],
    tetrahedra: &[[usize; 4]],
    delta_c: f64,
    timeout: Duration,
) -> Result<DocumentThermal, String> {
    if !delta_c.is_finite() || delta_c <= 0.0 {
        return Err("the temperature difference must be positive".to_string());
    }
    if tetrahedra.is_empty() {
        return Err("the mesh has no tetrahedra to solve".to_string());
    }
    if tetrahedra.len() > MAX_SOLVE_TETS {
        return Err(format!(
            "the mesh has {} tetrahedra, above the {} this adapter will submit",
            tetrahedra.len(),
            MAX_SOLVE_TETS
        ));
    }
    let executable = discover_executable()
        .ok_or("CalculiX `ccx` is not installed: put ccx on PATH or set ATLAS_CCX")?;
    let version = solver_version(&executable, timeout)?;
    let patch = classify_document_nodes(coordinates, tetrahedra)?;

    let min_z = coordinates
        .iter()
        .map(|c| c[2])
        .fold(f64::INFINITY, f64::min);
    let max_z = coordinates
        .iter()
        .map(|c| c[2])
        .fold(f64::NEG_INFINITY, f64::max);
    let height = max_z - min_z;
    if height <= 0.0 {
        return Err("the model has no height to conduct through".to_string());
    }
    let hot: Vec<u32> = {
        let mut ids: Vec<u32> = patch.loaded.keys().copied().collect();
        ids.sort_unstable();
        ids
    };
    let deck = thermal_deck(coordinates, tetrahedra, &hot, &patch.fixed, delta_c);

    let unique = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_nanos());
    let workdir = env::temp_dir().join(format!("atlas-ccx-heat-{}-{unique}", std::process::id()));
    fs::create_dir_all(&workdir)
        .map_err(|error| format!("cannot create the solver workdir: {error}"))?;
    let started = Instant::now();
    let result = run_in_workdir_named(&executable, &workdir, "heat", &deck, timeout);
    let elapsed_ms = started.elapsed().as_millis() as u64;
    let dat = result.and_then(|()| {
        fs::read_to_string(workdir.join("heat.dat"))
            .map_err(|error| format!("CalculiX produced no heat.dat: {error}"))
    })?;
    let output = dat.as_bytes().to_vec();
    let _ = fs::remove_dir_all(&workdir);

    let temperatures = parse_nodal_scalar(&dat, "temperatures")
        .ok_or("CalculiX printed no temperature table for the document mesh")?;
    let reactions = parse_nodal_scalar(&dat, "heat generation")
        .ok_or("CalculiX printed no reaction flux for the cold face")?;

    let min_temperature_c = temperatures
        .iter()
        .map(|(_, t)| *t)
        .fold(f64::INFINITY, f64::min);
    let max_temperature_c = temperatures
        .iter()
        .map(|(_, t)| *t)
        .fold(f64::NEG_INFINITY, f64::max);
    let mut linear_residual = 0.0_f64;
    for (node, temperature) in &temperatures {
        let z = coordinates[*node as usize - 1][2];
        let expected = delta_c * (z - min_z) / height;
        linear_residual = linear_residual.max((temperature - expected).abs());
    }
    let conducted_heat_w = reactions.iter().map(|(_, flux)| flux).sum::<f64>().abs();
    let prismatic_reference_heat_w =
        THERMAL_CONDUCTIVITY_W_PER_MM_K * patch.loaded_area_mm2 * delta_c / height;
    let reference_error_percent = if prismatic_reference_heat_w > 0.0 {
        (conducted_heat_w - prismatic_reference_heat_w).abs() / prismatic_reference_heat_w * 100.0
    } else {
        0.0
    };

    Ok(DocumentThermal {
        solver_version: version,
        executable: executable.display().to_string(),
        nodes: coordinates.len(),
        elements: tetrahedra.len(),
        cold_face_c: 0.0,
        hot_face_c: delta_c,
        loaded_area_mm2: patch.loaded_area_mm2,
        conducted_heat_w,
        prismatic_reference_heat_w,
        reference_error_percent,
        min_temperature_c,
        max_temperature_c,
        linear_profile_residual_percent: linear_residual / delta_c * 100.0,
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[deck.as_bytes(), output.as_slice()]),
    })
}

fn thermal_deck(
    coordinates: &[[f64; 3]],
    tetrahedra: &[[usize; 4]],
    hot: &[u32],
    cold: &[u32],
    delta_c: f64,
) -> String {
    let mut deck =
        String::from("*HEADING\nAtlasCAD CalculiX document thermal solve\n*NODE, NSET=NALL\n");
    for (index, [x, y, z]) in coordinates.iter().enumerate() {
        deck.push_str(&format!("{},{x:.9},{y:.9},{z:.9}\n", index + 1));
    }
    deck.push_str("*ELEMENT, TYPE=C3D4, ELSET=EALL\n");
    for (index, tet) in tetrahedra.iter().enumerate() {
        deck.push_str(&format!(
            "{},{},{},{},{}\n",
            index + 1,
            tet[0] + 1,
            tet[1] + 1,
            tet[2] + 1,
            tet[3] + 1
        ));
    }
    let cold_ids: Vec<String> = cold.iter().map(ToString::to_string).collect();
    deck.push_str("*NSET, NSET=COLD\n");
    push_ids(&mut deck, &cold_ids);
    let hot_ids: Vec<String> = hot.iter().map(ToString::to_string).collect();
    deck.push_str("*NSET, NSET=HOT\n");
    push_ids(&mut deck, &hot_ids);
    deck.push_str(&format!(
        "*MATERIAL, NAME=STEEL\n*CONDUCTIVITY\n{THERMAL_CONDUCTIVITY_W_PER_MM_K}\n\
         *SOLID SECTION, ELSET=EALL, MATERIAL=STEEL\n\
         *STEP\n*HEAT TRANSFER, STEADY STATE\n*BOUNDARY\nCOLD,11,11,0.0\nHOT,11,11,{delta_c}\n\
         *NODE PRINT, NSET=COLD\nRFL\n*NODE PRINT, NSET=NALL\nNT\n*END STEP\n"
    ));
    deck
}

/// Parse a one-value-per-node `*NODE PRINT` table such as `temperatures` or `heat generation`.
fn parse_nodal_scalar(dat: &str, header: &str) -> Option<Vec<(u32, f64)>> {
    let mut in_table = false;
    let mut rows = Vec::new();
    for line in dat.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(header) {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if trimmed.is_empty() {
            if rows.is_empty() {
                continue;
            }
            break;
        }
        let columns: Vec<&str> = trimmed
            .split_whitespace()
            .map(|column| column.trim_end_matches(','))
            .collect();
        if columns.len() < 2 {
            continue;
        }
        if let (Ok(node), Ok(value)) = (columns[0].parse::<u32>(), columns[1].parse::<f64>()) {
            rows.push((node, value));
        }
    }
    (!rows.is_empty()).then_some(rows)
}

/// Parse one `(fx,fy,fz)`-shaped `*NODE PRINT` table: `displacements` or `forces`.
fn parse_nodal_table(dat: &str, header: &str) -> Option<Vec<(u32, [f64; 3])>> {
    let mut in_table = false;
    let mut rows = Vec::new();
    for line in dat.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with(header) {
            in_table = true;
            continue;
        }
        if !in_table {
            continue;
        }
        if trimmed.is_empty() {
            if rows.is_empty() {
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
        let tree = (
            columns[0].parse::<u32>(),
            columns[1].parse::<f64>(),
            columns[2].parse::<f64>(),
        );
        if let (Ok(node), Ok(first), Ok(second)) = tree {
            let third = columns[3].parse::<f64>().ok()?;
            rows.push((node, [first, second, third]));
        }
    }
    (!rows.is_empty()).then_some(rows)
}

/// Parse the `displacements (vx,vy,vz)` table that `*NODE PRINT` writes.
fn parse_displacements(dat: &str) -> Option<Displacements> {
    let rows = parse_nodal_table(dat, "displacements")?;
    Some(Displacements {
        values: rows.into_iter().map(|(_, [vx, _, _])| vx).collect(),
    })
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

    #[test]
    fn reaction_table_is_parsed_alongside_displacements() -> Result<(), Box<dyn std::error::Error>>
    {
        let dat = "\n displacements (vx,vy,vz) for set LOADED and time  0.1000000E+01\n\n\
                   5, 1.0E-05, 0.0E+00, -4.7E-05\n\n\
                   \n forces (fx,fy,fz) for set FIXED and time  0.1000000E+01\n\n\
                   1, 0.0E+00, 0.0E+00, -2.0E+02\n\
                   2, 0.0E+00, 0.0E+00, 2.0E+02\n\n";
        let forces = parse_nodal_table(dat, "forces").ok_or("the reaction table was not parsed")?;
        assert_eq!(forces.len(), 2);
        assert_eq!(forces[0].0, 1);
        let total: f64 = forces.iter().map(|(_, [_, _, fz])| fz).sum();
        assert!(total.abs() < 1e-12, "reactions must cancel: {total}");
        // The displacement table in the same file still parses through the shared reader.
        let displacements = parse_nodal_table(dat, "displacements").ok_or("displacements")?;
        assert_eq!(displacements[0].1[2], -4.7e-5);
        Ok(())
    }

    /// The lexical face classification is what decides clamp and load, so check it on a cube whose
    /// six-tet decomposition has a known top area.
    #[test]
    fn cube_mesh_top_face_becomes_the_loaded_patch() -> Result<(), Box<dyn std::error::Error>> {
        let coordinates = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [1.0, 1.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
            [1.0, 0.0, 1.0],
            [1.0, 1.0, 1.0],
            [0.0, 1.0, 1.0],
        ];
        let tetrahedra = vec![
            [0, 1, 2, 6],
            [0, 2, 3, 6],
            [0, 3, 7, 6],
            [0, 7, 4, 6],
            [0, 4, 5, 6],
            [0, 5, 1, 6],
        ];
        let patch =
            classify_document_nodes(&coordinates, &tetrahedra).map_err(std::io::Error::other)?;
        assert!(
            (patch.loaded_area_mm2 - 1.0).abs() < 1e-12,
            "top face area was {}, expected 1.0",
            patch.loaded_area_mm2
        );
        let mut loaded_ids: Vec<u32> = patch.loaded.keys().copied().collect();
        loaded_ids.sort_unstable();
        assert_eq!(loaded_ids, vec![5, 6, 7, 8]);
        let weight_total: f64 = patch.loaded.values().sum();
        assert!((weight_total - 1.0).abs() < 1e-12);
        assert_eq!(patch.fixed, vec![1, 2, 3, 4]);

        let deck = document_deck(&coordinates, &tetrahedra, &patch.loaded, &patch.fixed, 2.0);
        assert!(deck.contains("*ELEMENT, TYPE=C3D4, ELSET=EALL"));
        assert!(deck.contains("*BOUNDARY\nFIXED,1,3,0.0"));
        assert!(deck.contains("*NODE PRINT, NSET=FIXED\nRF"));
        // The corner of both top triangles carries a third of each: 2.0 * 1/3 N downwards.
        assert!(deck.contains("5,3,-0.666666667"), "{deck}");
        Ok(())
    }

    #[test]
    fn thermal_tables_are_parsed() -> Result<(), Box<dyn std::error::Error>> {
        let dat = "\n heat generation for set COLD and time  0.1000000E+01\n\n\
                   1 -1.25E+02\n2 -1.25E+02\n\n\
                   \n temperatures for set NALL and time  0.1000000E+01\n\n\
                   1 0.0\n2 1.0E+02\n\n";
        let flux = parse_nodal_scalar(dat, "heat generation").ok_or("flux")?;
        assert_eq!(flux.len(), 2);
        let total: f64 = flux.iter().map(|(_, v)| v).sum();
        assert!((total + 250.0).abs() < 1e-12, "{total}");
        let temperatures = parse_nodal_scalar(dat, "temperatures").ok_or("temperatures")?;
        assert_eq!(temperatures[1], (2, 100.0));
        Ok(())
    }

    #[test]
    fn prismatic_heat_reference_is_k_a_delta_over_length() {
        // 50 W/(m·K) over 400 mm² and 100 K across 10 mm is 200 W.
        let reference = THERMAL_CONDUCTIVITY_W_PER_MM_K * 400.0 * 100.0 / 10.0;
        assert!((reference - 200.0).abs() < 1e-12);
    }

    #[test]
    fn a_flat_or_inverted_mesh_is_refused() -> Result<(), Box<dyn std::error::Error>> {
        let flat = vec![
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [1.0, 1.0, 0.0],
        ];
        let error = classify_document_nodes(&flat, &[[0, 1, 2, 3]])
            .err()
            .ok_or("a flat mesh must be refused")?;
        assert!(
            error.contains("nothing to load")
                || error.contains("no height")
                || error.contains("flat sheet"),
            "{error}"
        );
        Ok(())
    }
}

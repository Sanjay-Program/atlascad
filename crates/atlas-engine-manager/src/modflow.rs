//! Real MODFLOW 6 benchmark adapter, driven through FloPy.
//!
//! MODFLOW 6 is a public-domain finite-difference groundwater solver distributed as a Fortran
//! executable; FloPy writes its input files and reads its heads and cell-by-cell budget. This
//! adapter builds a ten-cell, single-layer confined aquifer with a constant head at each end, runs
//! the `mf6` executable, and compares the simulated heads with the closed-form solution of the
//! same one-dimensional problem: with no sources the head is linear between the two fixed nodes,
//! and the discharge is Darcy's law `Q = K A Δh / L`.
//!
//! The constant-head cells are cell centres, so the distance between the fixed nodes is
//! `(ncol − 1) · Δx`, not the model length. Both references are computed here, not read back from
//! MODFLOW, so the heads and the reported boundary inflow are checked independently.
//!
//! The interpreter is discovered by [`crate::python`]; `flopy` must be importable from it and
//! `mf6` must be executable.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Aquifer and stress of the test case: ten 100 m cells, 10 m thick and 100 m wide, K = 10 m/day,
/// the head falling from 100 m to 90 m.
const COLUMNS: usize = 10;
const CELL_LENGTH_M: f64 = 100.0;
const ROW_WIDTH_M: f64 = 100.0;
const THICKNESS_M: f64 = 10.0;
const CONDUCTIVITY_M_DAY: f64 = 10.0;
const HEAD_LEFT_M: f64 = 100.0;
const HEAD_RIGHT_M: f64 = 90.0;

/// Verified result of one real MODFLOW 6 run.
#[derive(Debug, Clone, Serialize)]
pub struct ModflowBenchmark {
    /// FloPy version that wrote and read the simulation.
    pub solver_version: String,
    /// Python interpreter used.
    pub executable: String,
    /// The `mf6` executable FloPy ran.
    pub engine_executable: String,
    pub columns: u64,
    pub cell_length_m: f64,
    pub thickness_m: f64,
    pub row_width_m: f64,
    pub hydraulic_conductivity_m_day: f64,
    /// Head in each cell, from the solver's head file, in metres.
    pub heads_m: Vec<f64>,
    /// `h₀ + (h_n − h₀) · i / (n − 1)`, in metres.
    pub analytic_heads_m: Vec<f64>,
    /// Largest `|simulated − analytic|`, in metres.
    pub max_head_error_m: f64,
    /// That error as a percentage of the 10 m head drop.
    pub error_percent: f64,
    /// Boundary inflow MODFLOW reported in its cell-by-cell budget, in m³/day.
    pub inflow_m3_day: f64,
    /// `K · b · w · Δh / L`, in m³/day.
    pub darcy_flow_m3_day: f64,
    /// Difference between the simulated inflow and Darcy's law, in percent.
    pub flow_error_percent: f64,
    /// Whether `mf6` reported normal termination.
    pub converged: bool,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the FloPy version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("flopy", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected FloPy version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("flopy", Duration::from_secs(60))
}

/// Run the confined-aquifer model and compare the heads and the inflow with the closed form.
pub fn run_confined_aquifer_benchmark(timeout: Duration) -> Result<ModflowBenchmark, String> {
    let (executable, version) = available().ok_or(
        "flopy is not importable: set ATLAS_PYTHON to an interpreter with flopy installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let converged = python::boolean(&result, "converged")?;
    if !converged {
        return Err("MODFLOW 6 did not report normal termination".to_string());
    }
    let heads_m: Vec<f64> = result
        .get("heads_m")
        .and_then(serde_json::Value::as_array)
        .map(|values| values.iter().filter_map(serde_json::Value::as_f64).collect())
        .ok_or("MODFLOW returned no heads")?;
    if heads_m.len() != COLUMNS {
        return Err(format!(
            "MODFLOW returned {} heads for a {COLUMNS}-cell model",
            heads_m.len()
        ));
    }
    let analytic_heads_m = analytic_heads_m();
    let max_head_error_m = heads_m
        .iter()
        .zip(&analytic_heads_m)
        .map(|(simulated, analytic)| (simulated - analytic).abs())
        .fold(0.0_f64, f64::max);
    // The head drop across the aquifer is the scale that makes the error meaningful.
    let error_percent = max_head_error_m / (HEAD_LEFT_M - HEAD_RIGHT_M) * 100.0;

    let inflow_m3_day = python::number(&result, "inflow_m3_day")?;
    let darcy_flow_m3_day = darcy_flow_m3_day();
    let flow_error_percent = (inflow_m3_day - darcy_flow_m3_day).abs() / darcy_flow_m3_day * 100.0;
    let engine_executable = result
        .get("engine_executable")
        .and_then(serde_json::Value::as_str)
        .ok_or("MODFLOW reported no executable path")?
        .to_string();

    Ok(ModflowBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        engine_executable,
        columns: COLUMNS as u64,
        cell_length_m: CELL_LENGTH_M,
        thickness_m: THICKNESS_M,
        row_width_m: ROW_WIDTH_M,
        hydraulic_conductivity_m_day: CONDUCTIVITY_M_DAY,
        heads_m,
        analytic_heads_m,
        max_head_error_m,
        error_percent,
        inflow_m3_day,
        darcy_flow_m3_day,
        flow_error_percent,
        converged,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

/// Distance between the two constant-head nodes, which sit at cell centres.
fn node_spacing_m() -> f64 {
    (COLUMNS - 1) as f64 * CELL_LENGTH_M
}

/// Linear head between the two fixed nodes of a source-free one-dimensional aquifer.
fn analytic_heads_m() -> Vec<f64> {
    (0..COLUMNS)
        .map(|index| {
            HEAD_LEFT_M
                + (HEAD_RIGHT_M - HEAD_LEFT_M) * index as f64 / (COLUMNS - 1) as f64
        })
        .collect()
}

/// `K · b · w · Δh / L` through the aquifer cross-section, in m³/day.
fn darcy_flow_m3_day() -> f64 {
    CONDUCTIVITY_M_DAY * THICKNESS_M * ROW_WIDTH_M * (HEAD_LEFT_M - HEAD_RIGHT_M) / node_spacing_m()
}

fn benchmark_script() -> &'static str {
    r#"import json
import os
import shutil
import sys
import tempfile

import flopy

COLUMNS = 10
CELL_LENGTH = 100.0
ROW_WIDTH = 100.0
THICKNESS = 10.0
CONDUCTIVITY = 10.0
HEAD_LEFT = 100.0
HEAD_RIGHT = 90.0


def find_mf6():
    candidates = [
        os.environ.get("ATLAS_MF6"),
        os.environ.get("MF6_EXE"),
        os.path.join(os.path.dirname(sys.executable), "mf6"),
        "mf6",
    ]
    for candidate in candidates:
        if candidate:
            found = shutil.which(candidate)
            if found:
                return found
    raise SystemExit("mf6 is not executable: set ATLAS_MF6 or install MODFLOW 6")


engine = find_mf6()
with tempfile.TemporaryDirectory() as workspace:
    simulation = flopy.mf6.MFSimulation(
        sim_name="atlas", sim_ws=workspace, exe_name=engine
    )
    flopy.mf6.ModflowTdis(
        simulation, time_units="DAYS", nper=1, perioddata=[(1.0, 1, 1.0)]
    )
    flopy.mf6.ModflowIms(
        simulation, complexity="SIMPLE", outer_dvclose=1e-12, inner_dvclose=1e-14
    )
    model = flopy.mf6.ModflowGwf(simulation, modelname="atlas", save_flows=True)
    flopy.mf6.ModflowGwfdis(
        model,
        nlay=1,
        nrow=1,
        ncol=COLUMNS,
        delr=CELL_LENGTH,
        delc=ROW_WIDTH,
        top=THICKNESS,
        botm=0.0,
    )
    flopy.mf6.ModflowGwfic(model, strt=0.5 * (HEAD_LEFT + HEAD_RIGHT))
    flopy.mf6.ModflowGwfnpf(model, icelltype=1, k=CONDUCTIVITY, save_flows=True)
    flopy.mf6.ModflowGwfchd(
        model,
        stress_period_data=[
            ((0, 0, 0), HEAD_LEFT),
            ((0, 0, COLUMNS - 1), HEAD_RIGHT),
        ],
    )
    flopy.mf6.ModflowGwfoc(
        model,
        budget_filerecord="atlas.cbc",
        head_filerecord="atlas.hds",
        saverecord=[("HEAD", "ALL"), ("BUDGET", "ALL")],
    )
    simulation.write_simulation(silent=True)
    converged, _report = simulation.run_simulation(silent=True)
    heads = model.output.head().get_data()[0, 0, :].tolist()
    budget = model.output.budget().get_data(text="CHD")[0]
    inflow = float(sum(-record["q"] for record in budget if record["q"] < 0.0))

print(json.dumps({
    "converged": bool(converged),
    "heads_m": heads,
    "inflow_m3_day": inflow,
    "engine_executable": engine,
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_closed_form_is_the_linear_profile_and_darcys_law() {
        let heads = analytic_heads_m();
        assert_eq!(heads.len(), COLUMNS);
        assert!((heads[0] - 100.0).abs() < 1e-12);
        assert!((heads[COLUMNS - 1] - 90.0).abs() < 1e-12);
        // Uneven spacing would make the mid-cell head something other than the mean.
        assert!((heads[COLUMNS / 2] - 95.0).abs() > 0.0);
        assert!((heads[COLUMNS / 2 - 1] + heads[COLUMNS / 2] - 190.0).abs() < 1e-12);
        assert!((darcy_flow_m3_day() - 111.111_111_111_111_11).abs() < 1e-9);
    }

    #[test]
    fn benchmark_runs_when_flopy_and_mf6_are_installed() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: flopy is not importable in this environment");
            return Ok(());
        }
        let result = match run_confined_aquifer_benchmark(Duration::from_secs(180)) {
            Ok(result) => result,
            Err(message) if message.contains("not executable") => {
                eprintln!("SKIP: mf6 is not installed in this environment: {message}");
                return Ok(());
            }
            Err(message) => return Err(std::io::Error::other(message).into()),
        };
        assert!(result.converged);
        assert!(
            result.error_percent < 1e-6,
            "MODFLOW heads miss the linear profile by {:.3e}%",
            result.error_percent
        );
        assert!(
            result.flow_error_percent < 1e-6,
            "MODFLOW inflow {} m³/day differs from Darcy's law {} m³/day by {:.3e}%",
            result.inflow_m3_day,
            result.darcy_flow_m3_day,
            result.flow_error_percent
        );
        assert!(result.engine_executable.contains("mf6"));
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

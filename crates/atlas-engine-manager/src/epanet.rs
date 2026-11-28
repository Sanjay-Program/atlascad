//! Real EPANET benchmark adapter, driven through WNTR.
//!
//! EPANET is a hydraulic network solver exposed through the WNTR Python package, so this adapter
//! builds a one-pipe network in a separate interpreter, runs the EPANET engine and compares the
//! junction head with the Hazen-Williams head loss for the same demand. The reference is computed
//! here with EPANET's published SI coefficient (10.667, exponents 1.852 and 4.871), not read back
//! from EPANET, so the check is independent of the solver's own bookkeeping.
//!
//! The interpreter is discovered by [`crate::python`]; `wntr` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Reservoir head, pipe geometry and junction demand of the test network.
const RESERVOIR_HEAD_M: f64 = 100.0;
const PIPE_LENGTH_M: f64 = 1_000.0;
const PIPE_DIAMETER_M: f64 = 0.3;
const HAZEN_WILLIAMS_C: f64 = 100.0;
const DEMAND_M3_S: f64 = 0.05;
/// EPANET's SI Hazen-Williams coefficient and exponents.
const HW_COEFFICIENT: f64 = 10.667;
const HW_FLOW_EXPONENT: f64 = 1.852;
const HW_DIAMETER_EXPONENT: f64 = 4.871;

/// Verified result of one real EPANET run.
#[derive(Debug, Clone, Serialize)]
pub struct EpanetBenchmark {
    pub solver_version: String,
    pub executable: String,
    /// Junction head EPANET computed, in metres.
    pub junction_head_m: f64,
    /// `100 − h_f` with `h_f` from Hazen-Williams, in metres.
    pub analytic_head_m: f64,
    pub head_loss_m: f64,
    /// Pipe flow the engine reported, in m³/s.
    pub flow_m3_s: f64,
    pub error_percent: f64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the WNTR version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("wntr", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected WNTR version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("wntr", Duration::from_secs(60))
}

/// Solve the one-pipe network and compare the junction head with Hazen-Williams.
pub fn run_pipe_network_benchmark(timeout: Duration) -> Result<EpanetBenchmark, String> {
    let (executable, version) = available().ok_or(
        "wntr is not importable: set ATLAS_PYTHON to an interpreter with wntr and EPANET installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let junction_head_m = python::number(&result, "junction_head_m")?;
    let flow_m3_s = python::number(&result, "flow_m3_s")?;
    if (flow_m3_s - DEMAND_M3_S).abs() / DEMAND_M3_S > 1e-6 {
        return Err(format!(
            "EPANET reported {flow_m3_s} m³/s through the pipe, expected the {DEMAND_M3_S} m³/s demand"
        ));
    }
    let head_loss_m = analytic_head_loss_m();
    let analytic_head_m = RESERVOIR_HEAD_M - head_loss_m;
    let error_percent = (junction_head_m - analytic_head_m).abs() / analytic_head_m * 100.0;

    Ok(EpanetBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        junction_head_m,
        analytic_head_m,
        head_loss_m,
        flow_m3_s,
        error_percent,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

/// Hazen-Williams head loss for the test network, in metres.
fn analytic_head_loss_m() -> f64 {
    HW_COEFFICIENT * PIPE_LENGTH_M * DEMAND_M3_S.powf(HW_FLOW_EXPONENT)
        / (HAZEN_WILLIAMS_C.powf(HW_FLOW_EXPONENT) * PIPE_DIAMETER_M.powf(HW_DIAMETER_EXPONENT))
}

fn benchmark_script() -> &'static str {
    r#"import json
import os
import tempfile
import wntr

# EPANET writes temp.inp/temp.rpt/temp.bin into the process directory, so run it in a scratch
# directory that is removed afterwards instead of leaving files behind in the caller's tree.
with tempfile.TemporaryDirectory() as workdir:
    os.chdir(workdir)
    network = wntr.network.WaterNetworkModel()
    network.add_reservoir("R1", base_head=100.0)
    network.add_junction("J1", base_demand=0.05, elevation=0.0)
    network.add_pipe("P1", "R1", "J1", length=1000.0, diameter=0.3, roughness=100.0)
    results = wntr.sim.EpanetSimulator(network).run_sim()
    head = float(results.node["head"]["J1"].iloc[0])
    flow = float(results.link["flowrate"]["P1"].iloc[0])
print(json.dumps({"junction_head_m": head, "flow_m3_s": flow}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_loss_is_the_hazen_williams_head() {
        // h_f = 10.667 · 1000 · 0.05^1.852 / (100^1.852 · 0.3^4.871) ≈ 2.8939 m.
        let loss = analytic_head_loss_m();
        assert!((loss - 2.893_857_3).abs() < 1e-4, "{loss}");
    }

    #[test]
    fn benchmark_runs_when_wntr_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: wntr is not importable in this environment");
            return Ok(());
        }
        let result =
            run_pipe_network_benchmark(Duration::from_secs(120)).map_err(std::io::Error::other)?;
        assert!(
            result.error_percent < 1e-3,
            "junction head {} m differs from Hazen-Williams {} m by {:.3e}%",
            result.junction_head_m,
            result.analytic_head_m,
            result.error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

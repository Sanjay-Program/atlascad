//! Real pandapower benchmark adapter.
//!
//! Pandapower is a Python library, so this adapter runs a two-bus test network in a separate
//! interpreter and compares the voltage it computes with the exact solution of the constant-power
//! load equations. It does not invent a power flow: the measured number is what pandapower's
//! Newton solver wrote, and the reference is the quadratic below solved independently here.
//!
//! The interpreter is discovered by [`crate::python`]; `pandapower` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Sending-end voltage of the test network, in per unit.
const SLACK_VM_PU: f64 = 1.0;
/// Series line impedance in per unit on the 1 MVA / 1 kV base.
const LINE_R_PU: f64 = 0.1;
const LINE_X_PU: f64 = 0.1;
/// Constant-power load at the receiving bus, in per unit.
const LOAD_P_PU: f64 = 1.0;

/// Verified result of one real pandapower power flow.
#[derive(Debug, Clone, Serialize)]
pub struct PandapowerBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub converged: bool,
    pub iterations: u64,
    /// Receiving-bus voltage magnitude the solver wrote, in per unit.
    pub measured_vm_pu: f64,
    /// The high-voltage root of `|V|² = Re(V) − R P`, `Im(V) = −X P` for the same network.
    pub analytic_vm_pu: f64,
    pub error_percent: f64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    /// Active losses on the line, in MW.
    pub line_loss_mw: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the pandapower version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("pandapower", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected pandapower version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("pandapower", Duration::from_secs(60))
}

/// Solve the two-bus network and check the voltage against the closed-form root.
pub fn run_power_flow_benchmark(timeout: Duration) -> Result<PandapowerBenchmark, String> {
    let (executable, version) = available().ok_or(
        "pandapower is not importable: set ATLAS_PYTHON to an interpreter with pandapower installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let converged = python::boolean(&result, "converged")?;
    if !converged {
        return Err("pandapower reported that the power flow did not converge".to_string());
    }
    let measured_vm_pu = python::number(&result, "vm_pu_bus1")?;
    let iterations = python::number(&result, "iterations")? as u64;
    let line_loss_mw = python::number(&result, "line_loss_mw")?;
    let analytic_vm_pu = analytic_receiving_voltage();
    let error_percent = (measured_vm_pu - analytic_vm_pu).abs() / analytic_vm_pu * 100.0;

    Ok(PandapowerBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        converged,
        iterations,
        measured_vm_pu,
        analytic_vm_pu,
        error_percent,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        line_loss_mw,
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

/// Voltage magnitude at the receiving bus, solved exactly.
///
/// With a slack bus at 1 pu and a constant-power load `P` behind `R + jX`, the receiving voltage
/// `V` satisfies `|V|² = Re(V) − R P` and `Im(V) = −X P`. The larger root is the operating point a
/// Newton power flow converges to from a flat start.
fn analytic_receiving_voltage() -> f64 {
    let slack = SLACK_VM_PU;
    let imaginary = -LINE_X_PU * LOAD_P_PU / slack;
    let discriminant = slack * slack - 4.0 * (imaginary * imaginary + LINE_R_PU * LOAD_P_PU);
    if discriminant < 0.0 {
        return f64::NAN;
    }
    let real = (slack + discriminant.sqrt()) / 2.0;
    real.hypot(imaginary)
}

fn benchmark_script() -> &'static str {
    r#"import json
import pandapower as pp

net = pp.create_empty_network(sn_mva=1.0)
bus0 = pp.create_bus(net, vn_kv=1.0)
bus1 = pp.create_bus(net, vn_kv=1.0)
pp.create_ext_grid(net, bus0, vm_pu=1.0)
pp.create_line_from_parameters(
    net, bus0, bus1, length_km=1.0, r_ohm_per_km=0.1, x_ohm_per_km=0.1,
    c_nf_per_km=0.0, max_i_ka=10.0,
)
pp.create_load(net, bus1, p_mw=1.0, q_mvar=0.0)
pp.runpp(
    net, algorithm="nr", init="flat", tolerance_mva=1e-12, max_iteration=50, numba=False,
)
print(json.dumps({
    "converged": bool(net.converged),
    "iterations": int(net._ppc["iterations"]),
    "vm_pu_bus1": float(net.res_bus.vm_pu.values[1]),
    "line_loss_mw": float(net.res_line.pl_mw.values[0]),
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_voltage_matches_the_quadratic_root() {
        // |V| = |0.87418 - 0.1j| for this network; the solver must reproduce it.
        let analytic = analytic_receiving_voltage();
        assert!((analytic - 0.879_866_9).abs() < 1e-6, "{analytic}");
    }

    #[test]
    fn benchmark_runs_when_pandapower_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: pandapower is not importable in this environment");
            return Ok(());
        }
        let result =
            run_power_flow_benchmark(Duration::from_secs(120)).map_err(std::io::Error::other)?;
        assert!(result.converged);
        assert!(result.iterations <= 10);
        assert!(
            result.error_percent < 1e-6,
            "power flow {} pu differs from the exact root {} pu by {:e}%",
            result.measured_vm_pu,
            result.analytic_vm_pu,
            result.error_percent
        );
        assert!(result.line_loss_mw > 0.0);
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

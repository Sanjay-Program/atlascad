//! Real MuJoCo benchmark adapter.
//!
//! MuJoCo is a Python/C++ rigid-body simulator, so this adapter drops a point mass from a known
//! height in a separate interpreter and compares the simulated height after one second with
//! `z₀ − ½ g t²`. The deviation is the integrator's own truncation error, which is why the
//! timestep is reported with the result instead of being hidden.
//!
//! The interpreter is discovered by [`crate::python`]; `mujoco` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

const DROP_HEIGHT_M: f64 = 10.0;
const GRAVITY_M_S2: f64 = 9.81;
const DURATION_S: f64 = 1.0;
const TIMESTEP_S: f64 = 0.000_01;
const STEPS: u64 = 100_000;

/// Verified result of one real MuJoCo run.
#[derive(Debug, Clone, Serialize)]
pub struct MujocoBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub steps: u64,
    pub timestep_s: f64,
    /// Height the simulator reported after `steps` steps, in metres.
    pub final_height_m: f64,
    /// `z₀ − ½ g t²` in metres.
    pub analytic_height_m: f64,
    pub error_percent: f64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the MuJoCo version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("mujoco", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected MuJoCo version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("mujoco", Duration::from_secs(60))
}

/// Integrate the free-fall case and compare with the closed form.
pub fn run_free_fall_benchmark(timeout: Duration) -> Result<MujocoBenchmark, String> {
    let (executable, version) = available().ok_or(
        "mujoco is not importable: set ATLAS_PYTHON to an interpreter with mujoco installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let final_height_m = python::number(&result, "height_m")?;
    let steps = python::number(&result, "steps")? as u64;
    let timestep_s = python::number(&result, "timestep_s")?;
    if steps != STEPS {
        return Err(format!(
            "the simulator reported {steps} steps, expected the {STEPS} in the script"
        ));
    }
    if (timestep_s - TIMESTEP_S).abs() > 1e-12 {
        return Err(format!(
            "the simulator used a {timestep_s} s timestep, expected {TIMESTEP_S} s"
        ));
    }
    let analytic_height_m = DROP_HEIGHT_M - 0.5 * GRAVITY_M_S2 * DURATION_S.powi(2);
    let error_percent = (final_height_m - analytic_height_m).abs() / analytic_height_m * 100.0;

    Ok(MujocoBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        steps,
        timestep_s,
        final_height_m,
        analytic_height_m,
        error_percent,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

fn benchmark_script() -> &'static str {
    r#"import json
import mujoco

xml = """<mujoco>
  <option timestep="0.00001" gravity="0 0 -9.81"/>
  <worldbody>
    <body name="ball" pos="0 0 10">
      <freejoint/>
      <geom size="0.1" mass="1"/>
    </body>
  </worldbody>
</mujoco>"""
model = mujoco.MjModel.from_xml_string(xml)
data = mujoco.MjData(model)
steps = 100000
for _ in range(steps):
    mujoco.mj_step(model, data)
print(json.dumps({
    "height_m": float(data.qpos[2]),
    "steps": steps,
    "timestep_s": float(model.opt.timestep),
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_height_is_the_drop_formula() {
        let analytic = DROP_HEIGHT_M - 0.5 * GRAVITY_M_S2 * DURATION_S.powi(2);
        assert!((analytic - 5.095).abs() < 1e-12);
        assert!((STEPS as f64 * TIMESTEP_S - DURATION_S).abs() < 1e-12);
    }

    #[test]
    fn benchmark_runs_when_mujoco_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: mujoco is not importable in this environment");
            return Ok(());
        }
        let result =
            run_free_fall_benchmark(Duration::from_secs(120)).map_err(std::io::Error::other)?;
        assert_eq!(result.steps, STEPS);
        assert!(
            result.error_percent < 1e-2,
            "simulated height {} m differs from {} m by {:.3e}%",
            result.final_height_m,
            result.analytic_height_m,
            result.error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

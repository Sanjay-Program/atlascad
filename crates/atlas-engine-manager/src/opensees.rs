//! Real OpenSeesPy benchmark adapter.
//!
//! OpenSeesPy is a Python library, so this adapter builds a cantilever in a separate interpreter,
//! runs its static analysis and compares the tip displacement with `P L³ / (3 E I)`. The beam
//! elements carry that solution exactly for a tip point load, so a passing run checks the model
//! build, the analysis path and the parser rather than mesh convergence.
//!
//! The interpreter is discovered by [`crate::python`]; `openseespy` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Cantilever geometry and load: 1000 mm long, 10 x 10 mm section, 210 GPa steel, 1 N tip load.
const LENGTH_MM: f64 = 1_000.0;
const YOUNGS_MODULUS_MPA: f64 = 210_000.0;
const WIDTH_MM: f64 = 10.0;
const HEIGHT_MM: f64 = 10.0;
const TIP_LOAD_N: f64 = 1.0;
const ELEMENTS: usize = 10;

/// Verified result of one real OpenSees static analysis.
#[derive(Debug, Clone, Serialize)]
pub struct OpenseesBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub elements: u64,
    /// Absolute tip displacement the solver wrote, in mm.
    pub tip_displacement_mm: f64,
    /// `P L³ / (3 E I)` for the same cantilever, in mm.
    pub analytic_displacement_mm: f64,
    pub error_percent: f64,
    /// `1 - error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the OpenSeesPy version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("openseespy", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected OpenSeesPy version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("openseespy", Duration::from_secs(60))
}

/// Solve the cantilever and check the tip displacement against the closed form.
pub fn run_cantilever_benchmark(timeout: Duration) -> Result<OpenseesBenchmark, String> {
    let (executable, version) = available().ok_or(
        "openseespy is not importable: set ATLAS_PYTHON to an interpreter with openseespy installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let code = python::number(&result, "code")?;
    if code != 0.0 {
        return Err(format!(
            "OpenSees returned analysis code {code} instead of 0"
        ));
    }
    let tip_displacement_mm = python::number(&result, "tip_displacement_mm")?.abs();
    let elements = python::number(&result, "elements")? as u64;
    if elements != ELEMENTS as u64 {
        return Err(format!(
            "OpenSees reported {elements} elements, expected the {ELEMENTS} in the script"
        ));
    }
    let analytic_displacement_mm = analytic_tip_displacement_mm();
    let error_percent =
        (tip_displacement_mm - analytic_displacement_mm).abs() / analytic_displacement_mm * 100.0;

    Ok(OpenseesBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        elements,
        tip_displacement_mm,
        analytic_displacement_mm,
        error_percent,
        accuracy_score: (1.0 - error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

fn analytic_tip_displacement_mm() -> f64 {
    let second_moment = WIDTH_MM * HEIGHT_MM.powi(3) / 12.0;
    TIP_LOAD_N * LENGTH_MM.powi(3) / (3.0 * YOUNGS_MODULUS_MPA * second_moment)
}

fn benchmark_script() -> &'static str {
    r#"import importlib.metadata as metadata
import json
import openseespy.opensees as ops

length, modulus, width, height, load, elements = 1000.0, 210000.0, 10.0, 10.0, 1.0, 10
second_moment = width * height ** 3 / 12.0
area = width * height

ops.wipe()
ops.model("basic", "-ndm", 2, "-ndf", 3)
ops.node(1, 0.0, 0.0)
ops.fix(1, 1, 1, 1)
for i in range(1, elements + 1):
    ops.node(i + 1, length * i / elements, 0.0)
ops.geomTransf("Linear", 1)
for i in range(1, elements + 1):
    ops.element("elasticBeamColumn", i, i, i + 1, area, modulus, second_moment, 1)
ops.timeSeries("Linear", 1)
ops.pattern("Plain", 1, 1)
ops.load(elements + 1, 0.0, -load, 0.0)
ops.system("BandGeneral")
ops.numberer("RCM")
ops.constraints("Plain")
ops.integrator("LoadControl", 1.0)
ops.analysis("Static")
code = ops.analyze(1)

try:
    version = metadata.version("openseespy")
except metadata.PackageNotFoundError:
    version = "unknown"

print(json.dumps({
    "code": int(code),
    "elements": elements,
    "tip_displacement_mm": float(ops.nodeDisp(elements + 1, 2)),
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn analytic_displacement_is_the_beam_formula() {
        // 1 N at 1000 mm on a 10 x 10 mm, 210 GPa cantilever: 1.9048 mm.
        let analytic = analytic_tip_displacement_mm();
        assert!(
            (analytic - 1.904_761_904_761_904_7).abs() < 1e-9,
            "{analytic}"
        );
    }

    #[test]
    fn benchmark_runs_when_opensees_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: openseespy is not importable in this environment");
            return Ok(());
        }
        let result =
            run_cantilever_benchmark(Duration::from_secs(120)).map_err(std::io::Error::other)?;
        assert_eq!(result.elements, ELEMENTS as u64);
        assert!(
            result.error_percent < 0.01,
            "tip displacement {} mm differs from P L³/(3 E I) = {} mm by {:e}%",
            result.tip_displacement_mm,
            result.analytic_displacement_mm,
            result.error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

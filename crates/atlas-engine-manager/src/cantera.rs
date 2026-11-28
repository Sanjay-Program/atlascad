//! Real Cantera benchmark adapter.
//!
//! Cantera is a Python/C++ thermochemistry library, so this adapter runs a short script in a
//! separate interpreter, reads the JSON it prints, and checks the numbers against closed-form or
//! standard references: Mayer's relation, the ideal-gas law, and the standard enthalpy of
//! combustion of methane. The adiabatic flame temperature is reported as the value the library
//! computed; no accuracy claim is attached to it.
//!
//! The interpreter is discovered by [`crate::python`]; `cantera` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Standard lower heating value of methane at 298.15 K with gaseous water, in kJ/mol.
const METHANE_LHV_KJ_PER_MOL: f64 = 802.3;

/// Verified result of one real Cantera run.
#[derive(Debug, Clone, Serialize)]
pub struct CanteraBenchmark {
    pub solver_version: String,
    pub executable: String,
    /// Lower heating value computed from the mechanism's species enthalpies, in kJ/mol CH4.
    pub lhv_kj_per_mol: f64,
    /// Standard reference the value above is compared with, in kJ/mol.
    pub lhv_reference_kj_per_mol: f64,
    pub lhv_error_percent: f64,
    /// Adiabatic (constant-pressure) flame temperature of stoichiometric methane/air, in K.
    pub flame_temperature_k: f64,
    /// `|cp − cv − R| / R`; zero to machine precision for an ideal gas.
    pub mayer_residual_relative: f64,
    /// `|ρ − pM/(RT)| / ρ`; zero to machine precision for an ideal gas.
    pub density_residual_relative: f64,
    /// `1 - lhv_error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the Cantera version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("cantera", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected Cantera version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("cantera", Duration::from_secs(60))
}

/// Compute the methane energy balance and equilibrium flame temperature with Cantera.
pub fn run_thermochemistry_benchmark(timeout: Duration) -> Result<CanteraBenchmark, String> {
    let (executable, version) = available().ok_or(
        "cantera is not importable: set ATLAS_PYTHON to an interpreter with cantera installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let lhv_kj_per_mol = python::number(&result, "lhv_kj_per_mol")?;
    let flame_temperature_k = python::number(&result, "flame_temperature_k")?;
    let mayer_residual_relative = python::number(&result, "mayer_residual_relative")?;
    let density_residual_relative = python::number(&result, "density_residual_relative")?;
    let lhv_error_percent =
        (lhv_kj_per_mol - METHANE_LHV_KJ_PER_MOL).abs() / METHANE_LHV_KJ_PER_MOL * 100.0;

    Ok(CanteraBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        lhv_kj_per_mol,
        lhv_reference_kj_per_mol: METHANE_LHV_KJ_PER_MOL,
        lhv_error_percent,
        flame_temperature_k,
        mayer_residual_relative,
        density_residual_relative,
        accuracy_score: (1.0 - lhv_error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

fn benchmark_script() -> &'static str {
    r#"import json
import cantera as ct

gas = ct.Solution("gri30.yaml")
T0 = 298.15

def h(name):
    return gas.species(name).thermo.h(T0)

# CH4 + 2 O2 -> CO2 + 2 H2O(g), per mole of methane.
lhv = (h("CH4") + 2 * h("O2") - h("CO2") - 2 * h("H2O")) / 1e6

gas.TPX = T0, ct.one_atm, "CH4:1, O2:2, N2:7.52"
gas.equilibrate("HP")
flame = gas.T

gas.TPX = T0, ct.one_atm, "CH4:1, O2:2, N2:7.52"
specific_gas_constant = ct.gas_constant / gas.mean_molecular_weight
mayer = abs(gas.cp_mass - gas.cv_mass - specific_gas_constant) / specific_gas_constant
ideal_density = ct.one_atm * gas.mean_molecular_weight / (ct.gas_constant * T0)
density = abs(gas.density - ideal_density) / gas.density

print(json.dumps({
    "version": ct.__version__,
    "lhv_kj_per_mol": lhv,
    "flame_temperature_k": flame,
    "mayer_residual_relative": mayer,
    "density_residual_relative": density,
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn benchmark_runs_when_cantera_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: cantera is not importable in this environment");
            return Ok(());
        }
        let result = run_thermochemistry_benchmark(Duration::from_secs(120))
            .map_err(std::io::Error::other)?;
        assert!(
            result.lhv_error_percent < 0.2,
            "mechanism LHV {} kJ/mol differs from the standard {} kJ/mol by {:.4}%",
            result.lhv_kj_per_mol,
            result.lhv_reference_kj_per_mol,
            result.lhv_error_percent
        );
        assert!(
            (2000.0..2600.0).contains(&result.flame_temperature_k),
            "flame temperature {} K is not physical for methane/air",
            result.flame_temperature_k
        );
        assert!(result.mayer_residual_relative < 1e-9);
        assert!(result.density_residual_relative < 1e-9);
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

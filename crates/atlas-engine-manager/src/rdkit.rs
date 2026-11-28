//! Real RDKit benchmark adapter.
//!
//! RDKit is a Python chemistry library, so this adapter builds benzene from its SMILES in a
//! separate interpreter and reads back the molecular formula and masses. The reference values are
//! independent of RDKit: the formula is derived from the SMILES, the average mass from standard
//! atomic weights (C 12.011, H 1.008) and the monoisotopic mass from the exact masses of ¹²C and ¹H.
//! It checks the chemistry database, not an RDKit-internal round trip.
//!
//! The interpreter is discovered by [`crate::python`]; `rdkit` must be importable from it.

use crate::process::provenance;
use crate::python;
use serde::Serialize;
use std::path::PathBuf;
use std::time::{Duration, Instant};

/// Benzene: C6H6, the simplest aromatic ring.
const SMILES: &str = "c1ccccc1";
const REFERENCE_FORMULA: &str = "C6H6";
/// Standard atomic weights: 6 × 12.011 + 6 × 1.008 g/mol.
const REFERENCE_AVERAGE_MASS: f64 = 6.0 * 12.011 + 6.0 * 1.008;
/// Monoisotopic mass of ¹²C₆¹H₆ in daltons (6 × 12.000000 + 6 × 1.0078250319).
const REFERENCE_EXACT_MASS: f64 = 6.0 * 12.0 + 6.0 * 1.007_825_031_9;

/// Verified result of one real RDKit run.
#[derive(Debug, Clone, Serialize)]
pub struct RdkitBenchmark {
    pub solver_version: String,
    pub executable: String,
    pub smiles: String,
    pub formula: String,
    pub heavy_atoms: u64,
    /// Monoisotopic mass RDKit computed, in daltons.
    pub exact_mass: f64,
    pub reference_exact_mass: f64,
    pub exact_mass_error_percent: f64,
    /// Average molecular weight RDKit computed, in g/mol.
    pub average_mass: f64,
    pub reference_average_mass: f64,
    pub average_mass_error_percent: f64,
    /// `1 - exact_mass_error_percent / 100`, clamped to `[0, 1]`; not a certification.
    pub accuracy_score: f64,
    pub elapsed_ms: u64,
    /// FNV-1a over the script and the solver's stdout.
    pub provenance_hash: String,
}

/// The interpreter and the RDKit version, when the module is importable.
#[must_use]
pub fn available() -> Option<(PathBuf, String)> {
    let python = python::discover_python()?;
    let version = python::module_version("rdkit", Duration::from_secs(60)).ok()?;
    Some((python, version))
}

/// Detected RDKit version, or an error when it cannot be imported.
pub fn probe_version() -> Result<String, String> {
    python::module_version("rdkit", Duration::from_secs(60))
}

/// Compute benzene's formula and masses with RDKit and compare them with the references.
pub fn run_molecular_benchmark(timeout: Duration) -> Result<RdkitBenchmark, String> {
    let (executable, version) = available().ok_or(
        "rdkit is not importable: set ATLAS_PYTHON to an interpreter with rdkit installed",
    )?;
    let started = Instant::now();
    let (result, stdout) = python::run_json(benchmark_script(), timeout)?;
    let elapsed_ms = started.elapsed().as_millis() as u64;

    let formula = result
        .get("formula")
        .and_then(serde_json::Value::as_str)
        .ok_or("the solver output has no formula")?
        .to_string();
    if formula != REFERENCE_FORMULA {
        return Err(format!(
            "RDKit reported {formula} for benzene, expected {REFERENCE_FORMULA}"
        ));
    }
    let exact_mass = python::number(&result, "exact_mass")?;
    let average_mass = python::number(&result, "average_mass")?;
    let heavy_atoms = python::number(&result, "heavy_atoms")? as u64;
    let exact_mass_error_percent =
        (exact_mass - REFERENCE_EXACT_MASS).abs() / REFERENCE_EXACT_MASS * 100.0;
    let average_mass_error_percent =
        (average_mass - REFERENCE_AVERAGE_MASS).abs() / REFERENCE_AVERAGE_MASS * 100.0;

    Ok(RdkitBenchmark {
        solver_version: version,
        executable: executable.display().to_string(),
        smiles: SMILES.to_string(),
        formula,
        heavy_atoms,
        exact_mass,
        reference_exact_mass: REFERENCE_EXACT_MASS,
        exact_mass_error_percent,
        average_mass,
        reference_average_mass: REFERENCE_AVERAGE_MASS,
        average_mass_error_percent,
        accuracy_score: (1.0 - exact_mass_error_percent / 100.0).clamp(0.0, 1.0),
        elapsed_ms: elapsed_ms.max(1),
        provenance_hash: provenance(&[benchmark_script().as_bytes(), stdout.as_bytes()]),
    })
}

fn benchmark_script() -> &'static str {
    r#"import json
from rdkit import Chem
from rdkit.Chem import Descriptors, rdMolDescriptors

molecule = Chem.MolFromSmiles("c1ccccc1")
print(json.dumps({
    "formula": rdMolDescriptors.CalcMolFormula(molecule),
    "exact_mass": float(Descriptors.ExactMolWt(molecule)),
    "average_mass": float(Descriptors.MolWt(molecule)),
    "heavy_atoms": int(molecule.GetNumHeavyAtoms()),
    "atoms": int(molecule.GetNumAtoms()),
}))
"#
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn references_are_the_standard_values() {
        // 6 × 12.011 + 6 × 1.008 = 78.114 g/mol; C6H6 monoisotopic mass 78.04695 Da.
        assert!((REFERENCE_AVERAGE_MASS - 78.114).abs() < 1e-9);
        assert!((REFERENCE_EXACT_MASS - 78.046_950_191_4).abs() < 1e-6);
    }

    #[test]
    fn benchmark_runs_when_rdkit_is_importable() -> Result<(), Box<dyn std::error::Error>> {
        if available().is_none() {
            eprintln!("SKIP: rdkit is not importable in this environment");
            return Ok(());
        }
        let result =
            run_molecular_benchmark(Duration::from_secs(120)).map_err(std::io::Error::other)?;
        assert_eq!(result.formula, REFERENCE_FORMULA);
        assert_eq!(result.heavy_atoms, 6);
        assert!(
            result.exact_mass_error_percent < 1e-6,
            "exact mass {} differs from {} by {:.2e}%",
            result.exact_mass,
            result.reference_exact_mass,
            result.exact_mass_error_percent
        );
        assert!(
            result.average_mass_error_percent < 0.01,
            "average mass {} differs from {} by {:.4}%",
            result.average_mass,
            result.reference_average_mass,
            result.average_mass_error_percent
        );
        assert!(result.provenance_hash.starts_with("fnv1a64-"));
        Ok(())
    }
}

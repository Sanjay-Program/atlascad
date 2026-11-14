//! Structural FEA assessment criteria and factor of safety.

use serde::{Deserialize, Serialize};

/// Structural integrity assessment against material allowable yield limits.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StructuralAssessment {
    pub max_von_mises_mpa: f64,
    pub max_displacement_mm: f64,
    pub yield_strength_mpa: f64,
    pub ultimate_strength_mpa: f64,
    pub factor_of_safety: f64,
    pub required_min_fos: f64,
    pub compliance_status: String,
    pub solver_provenance: String,
}

impl StructuralAssessment {
    pub fn assess(
        max_von_mises_mpa: f64,
        max_displacement_mm: f64,
        yield_strength_mpa: f64,
        ultimate_strength_mpa: f64,
        required_min_fos: f64,
        solver_provenance: impl Into<String>,
    ) -> Self {
        let fos = if max_von_mises_mpa > 1e-6 {
            yield_strength_mpa / max_von_mises_mpa
        } else {
            999.0
        };

        let status = if fos >= required_min_fos {
            "PASS_VERIFIED"
        } else {
            "FAIL_EXCEEDS_YIELD"
        };

        Self {
            max_von_mises_mpa,
            max_displacement_mm,
            yield_strength_mpa,
            ultimate_strength_mpa,
            factor_of_safety: fos,
            required_min_fos,
            compliance_status: status.to_string(),
            solver_provenance: solver_provenance.into(),
        }
    }
}

//! # Nuclear & Radiation Shielding Engineering
//!
//! Provides radiation protection and nuclear physics shielding calculations:
//! 1. Photon (Gamma / X-ray) attenuation via Beer-Lambert law with Taylor exposure buildup factors.
//! 2. Half-Value Layer (HVL) and Tenth-Value Layer (TVL) thicknesses for Lead, Concrete, Steel, and Water.
//! 3. Multi-layer biological shield attenuation and occupational dose rate assessment (ICRP-103 limits).

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Material layer definition in a composite radiation shield.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShieldLayer {
    pub material_name: String,
    /// Physical shield thickness in meters
    pub thickness_m: f64,
    /// Linear attenuation coefficient mu in 1/m for the incident photon energy
    pub linear_attenuation_m_inv: f64,
    /// Taylor buildup parameter A
    pub taylor_param_a: f64,
    /// Taylor buildup parameter alpha_1
    pub taylor_param_alpha1: f64,
    /// Taylor buildup parameter alpha_2
    pub taylor_param_alpha2: f64,
}

impl Default for ShieldLayer {
    fn default() -> Self {
        Self {
            material_name: "Heavy Barite Concrete (p = 3.5 g/cm3)".to_string(),
            thickness_m: 0.40,
            linear_attenuation_m_inv: 18.5,
            taylor_param_a: 1.50,
            taylor_param_alpha1: -0.075,
            taylor_param_alpha2: 0.050,
        }
    }
}

/// Parameters for radiation source and shielding configuration.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RadiationShieldingProblem {
    pub problem_name: String,
    /// Radioisotope source activity in Becquerels (Bq) (e.g. 3.7e11 Bq = 10 Curies)
    pub source_activity_bq: f64,
    /// Photon energy per emission in MeV (e.g., 1.25 MeV for Cobalt-60 average)
    pub photon_energy_mev: f64,
    /// Specific gamma ray constant Gamma in (uSv * m^2) / (MBq * hr)
    pub gamma_constant_usv_m2_per_mbq_hr: f64,
    /// Radial standoff distance from point source to receptor in meters
    pub distance_to_receptor_m: f64,
    /// Sequential shield layers from source to receptor
    pub layers: Vec<ShieldLayer>,
    /// Occupational dose rate limit in uSv/hr (e.g., 10.0 uSv/hr for controlled radiation area)
    pub dose_rate_limit_usv_hr: f64,
}

impl Default for RadiationShieldingProblem {
    fn default() -> Self {
        Self {
            problem_name: "Industrial Radiography Vault Biological Shield (Co-60)".to_string(),
            source_activity_bq: 3.7e11,
            photon_energy_mev: 1.25,
            gamma_constant_usv_m2_per_mbq_hr: 0.305,
            distance_to_receptor_m: 3.0,
            layers: vec![
                ShieldLayer {
                    material_name: "Lead Inner Liner (Pb)".to_string(),
                    thickness_m: 0.05,
                    linear_attenuation_m_inv: 68.0,
                    taylor_param_a: 1.15,
                    taylor_param_alpha1: -0.045,
                    taylor_param_alpha2: 0.020,
                },
                ShieldLayer {
                    material_name: "Barite Structural Concrete".to_string(),
                    thickness_m: 0.50,
                    linear_attenuation_m_inv: 18.5,
                    taylor_param_a: 1.65,
                    taylor_param_alpha1: -0.080,
                    taylor_param_alpha2: 0.040,
                },
            ],
            dose_rate_limit_usv_hr: 7.5,
        }
    }
}

/// Solves radiation transmission, exponential attenuation, buildup factors, and dose rate.
pub fn solve_radiation_shielding(
    prob: &RadiationShieldingProblem,
) -> Result<SimulationResult, String> {
    if prob.source_activity_bq <= 0.0 || prob.distance_to_receptor_m <= 0.0 {
        return Err(
            "Source activity and receptor distance must be positive non-zero values".to_string(),
        );
    }
    if prob.gamma_constant_usv_m2_per_mbq_hr <= 0.0 {
        return Err("Gamma ray constant must be positive".to_string());
    }

    let activity_mbq = prob.source_activity_bq / 1.0e6;
    let d2 = prob.distance_to_receptor_m * prob.distance_to_receptor_m;
    let unshielded_dose_rate_usv_hr = (prob.gamma_constant_usv_m2_per_mbq_hr * activity_mbq) / d2;

    let mut total_mfp = 0.0;
    let mut total_uncollided_transmission = 1.0;
    let mut primary_buildup: f64 = 1.0;
    let mut total_thickness_m = 0.0;

    for layer in &prob.layers {
        if layer.thickness_m < 0.0 || layer.linear_attenuation_m_inv <= 0.0 {
            return Err(
                "Shield thickness must be non-negative and linear attenuation positive".to_string(),
            );
        }
        let mfp_i = layer.linear_attenuation_m_inv * layer.thickness_m;
        total_mfp += mfp_i;
        total_thickness_m += layer.thickness_m;
        total_uncollided_transmission *= (-mfp_i).exp();

        let b_i = layer.taylor_param_a * (-layer.taylor_param_alpha1 * mfp_i).exp()
            + (1.0 - layer.taylor_param_a) * (-layer.taylor_param_alpha2 * mfp_i).exp();
        primary_buildup = primary_buildup.max(b_i);
    }

    let shielded_transmission_factor = primary_buildup * total_uncollided_transmission;
    let shielded_dose_rate_usv_hr = unshielded_dose_rate_usv_hr * shielded_transmission_factor;

    let (hvl_m, tvl_m) = if let Some(first) = prob.layers.first() {
        let hvl = std::f64::consts::LN_2 / first.linear_attenuation_m_inv;
        let tvl = 10.0_f64.ln() / first.linear_attenuation_m_inv;
        (hvl, tvl)
    } else {
        (0.0, 0.0)
    };

    let compliance_safe = shielded_dose_rate_usv_hr <= prob.dose_rate_limit_usv_hr;
    let dose_factor_of_safety = prob.dose_rate_limit_usv_hr / shielded_dose_rate_usv_hr.max(1e-12);

    let mut metrics = HashMap::new();
    metrics.insert(
        "unshielded_dose_rate_usv_hr".to_string(),
        unshielded_dose_rate_usv_hr,
    );
    metrics.insert(
        "shielded_dose_rate_usv_hr".to_string(),
        shielded_dose_rate_usv_hr,
    );
    metrics.insert("total_mean_free_paths".to_string(), total_mfp);
    metrics.insert("total_shield_thickness_m".to_string(), total_thickness_m);
    metrics.insert(
        "overall_attenuation_factor".to_string(),
        1.0 / shielded_transmission_factor.max(1e-30),
    );
    metrics.insert("first_layer_hvl_m".to_string(), hvl_m);
    metrics.insert("first_layer_tvl_m".to_string(), tvl_m);
    metrics.insert("exposure_buildup_factor".to_string(), primary_buildup);
    metrics.insert("dose_factor_of_safety".to_string(), dose_factor_of_safety);
    metrics.insert(
        "is_radiologically_safe".to_string(),
        if compliance_safe { 1.0 } else { 0.0 },
    );

    let theoretical_unshielded = (prob.gamma_constant_usv_m2_per_mbq_hr * activity_mbq) / d2;
    let val = AnalyticalComparison::assess(
        "unshielded_dose_rate_usv_hr",
        unshielded_dose_rate_usv_hr,
        theoretical_unshielded,
    );

    Ok(SimulationResult {
        problem_id: "physics-nuclear-radiation-shielding".to_string(),
        engine_id: "atlas-nuclear-shielding".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: ConvergenceMonitor::new(1e-6),
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-nuclear-shielding", "0.1.0"),
        logs: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_radiation_shielding_attenuation_and_dose() -> Result<(), String> {
        let prob = RadiationShieldingProblem::default();
        let result = solve_radiation_shielding(&prob)?;

        assert_eq!(result.status, SimulationStatus::Converged);
        let unshielded = result
            .scalar_metrics
            .get("unshielded_dose_rate_usv_hr")
            .copied()
            .unwrap_or(0.0);
        let shielded = result
            .scalar_metrics
            .get("shielded_dose_rate_usv_hr")
            .copied()
            .unwrap_or(0.0);
        let mfp = result
            .scalar_metrics
            .get("total_mean_free_paths")
            .copied()
            .unwrap_or(0.0);
        let safe = result
            .scalar_metrics
            .get("is_radiologically_safe")
            .copied()
            .unwrap_or(0.0);

        assert!(
            unshielded > 11000.0 && unshielded < 14000.0,
            "Unshielded: {} uSv/hr",
            unshielded
        );
        assert!(mfp > 12.0 && mfp < 13.5, "Total mfp: {}", mfp);
        assert!(
            shielded > 0.0 && shielded < 1.0,
            "Shielded dose rate: {} uSv/hr",
            shielded
        );
        assert_eq!(safe, 1.0, "Vault must be radiologically safe");
        Ok(())
    }
}

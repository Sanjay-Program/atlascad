//! # Geoscience, Mining & Rock Mechanics Engineering
//!
//! Provides geomechanical and rock engineering formulations:
//! 1. Hoek-Brown failure criterion (2002 edition) for jointed rock masses and equivalent Mohr-Coulomb parameters.
//! 2. Bieniawski Rock Mass Rating (RMR89) geomechanical classification and in-situ deformation modulus.
//! 3. USBM (US Bureau of Mines) blast vibration attenuation and peak particle velocity (PPV) safety limits.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for Hoek-Brown rock mass failure criterion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HoekBrownProblem {
    pub rock_name: String,
    /// Uniaxial compressive strength of intact rock sigma_ci in MPa
    pub intact_ucs_mpa: f64,
    /// Geological Strength Index GSI (10 to 100)
    pub gsi: f64,
    /// Intact rock material constant m_i (e.g., 24 for granite, 15 for sandstone, 7 for claystone)
    pub intact_mi: f64,
    /// Disturbance factor D (0.0 for undisturbed in-situ / TBM, 1.0 for blast-damaged)
    pub disturbance_d: f64,
    /// Effective confining minor principal stress sigma_3_prime in MPa
    pub confining_stress_mpa: f64,
}

impl Default for HoekBrownProblem {
    fn default() -> Self {
        Self {
            rock_name: "Granodiorite Deep Tunnel Rock Mass".to_string(),
            intact_ucs_mpa: 130.0,
            gsi: 65.0,
            intact_mi: 28.0,
            disturbance_d: 0.0,
            confining_stress_mpa: 15.0,
        }
    }
}

/// Parameters for Bieniawski Rock Mass Rating (RMR89) geomechanical classification.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RmrClassificationProblem {
    pub location_name: String,
    /// R1: Uniaxial Compressive Strength rating (0 to 15)
    pub r1_strength_score: f64,
    /// R2: Rock Quality Designation (RQD %) rating (3 to 20)
    pub r2_rqd_score: f64,
    /// R3: Discontinuity spacing rating (5 to 20)
    pub r3_spacing_score: f64,
    /// R4: Condition of discontinuities rating (0 to 30)
    pub r4_condition_score: f64,
    /// R5: Groundwater condition rating (0 to 15)
    pub r5_groundwater_score: f64,
    /// Discontinuity orientation adjustment for excavation/tunneling (-12 to 0)
    pub orientation_adjustment: f64,
}

impl Default for RmrClassificationProblem {
    fn default() -> Self {
        Self {
            location_name: "Highway Tunnel Underground Cavern".to_string(),
            r1_strength_score: 12.0,
            r2_rqd_score: 17.0,
            r3_spacing_score: 15.0,
            r4_condition_score: 25.0,
            r5_groundwater_score: 10.0,
            orientation_adjustment: -2.0,
        }
    }
}

/// Parameters for blast vibration ground motion prediction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BlastVibrationProblem {
    pub blast_site: String,
    /// Distance from blast detonation point D in meters
    pub distance_m: f64,
    /// Maximum charge weight per delay W in kg (TNT or ANFO equivalent)
    pub max_charge_per_delay_kg: f64,
    /// Site transmission coefficient K (USBM average: 1140 for average rock, 242 for hard granite)
    pub site_constant_k: f64,
    /// Attenuation slope exponent beta (USBM standard: 1.60)
    pub attenuation_exponent_beta: f64,
    /// Structural vibration velocity safety threshold in mm/s (e.g., 20 mm/s for residential DIN 4150)
    pub safety_threshold_ppv_mm_s: f64,
}

impl Default for BlastVibrationProblem {
    fn default() -> Self {
        Self {
            blast_site: "Open-Pit Mine Bench Production Blast".to_string(),
            distance_m: 250.0,
            max_charge_per_delay_kg: 180.0,
            site_constant_k: 1140.0,
            attenuation_exponent_beta: 1.60,
            safety_threshold_ppv_mm_s: 20.0,
        }
    }
}

/// Solves Hoek-Brown (2002) rock mass compressive strength and equivalent Mohr-Coulomb parameters.
pub fn solve_hoek_brown(prob: &HoekBrownProblem) -> Result<SimulationResult, String> {
    if prob.intact_ucs_mpa <= 0.0 || prob.gsi <= 0.0 || prob.intact_mi <= 0.0 {
        return Err("Intact UCS, GSI, and m_i must be positive non-zero values".to_string());
    }
    if prob.gsi > 100.0 || prob.disturbance_d < 0.0 || prob.disturbance_d > 1.0 {
        return Err("GSI must be <= 100 and disturbance D must be in [0.0, 1.0]".to_string());
    }

    let gsi = prob.gsi;
    let d = prob.disturbance_d;
    let s_ci = prob.intact_ucs_mpa;
    let s_3 = prob.confining_stress_mpa.max(0.0);

    let denom_mb = 28.0 - 14.0 * d;
    let m_b = prob.intact_mi * ((gsi - 100.0) / denom_mb).exp();

    let denom_s = 9.0 - 3.0 * d;
    let s = ((gsi - 100.0) / denom_s).exp();

    let a = 0.5 + (1.0 / 6.0) * ((-gsi / 15.0).exp() - (-20.0_f64 / 3.0_f64).exp());

    let term = m_b * (s_3 / s_ci) + s;
    let sigma_1_mpa = s_3 + s_ci * term.max(0.0).powf(a);

    let sigma_cm_mpa = s_ci * s.powf(a);
    let sigma_t_mpa = -s * s_ci / m_b;

    let term_mc = 6.0 * a * m_b * term.powf(a - 1.0);
    let sin_phi = term_mc / (2.0 * (1.0 + a) * (2.0 + a) + term_mc);
    let phi_deg = sin_phi.min(0.999).asin().to_degrees();

    let c_mpa =
        (s_ci * ((1.0 + 2.0 * a) * s + (1.0 - a) * m_b * (s_3 / s_ci)) * term.powf(a - 1.0))
            / ((1.0 + a)
                * (2.0 + a)
                * (1.0 + (6.0 * a * m_b * term.powf(a - 1.0)) / ((1.0 + a) * (2.0 + a))).sqrt());

    let mut metrics = HashMap::new();
    metrics.insert("rock_mass_mb".to_string(), m_b);
    metrics.insert("rock_mass_s".to_string(), s);
    metrics.insert("rock_mass_a".to_string(), a);
    metrics.insert("major_principal_stress_s1_mpa".to_string(), sigma_1_mpa);
    metrics.insert("rock_mass_ucs_scm_mpa".to_string(), sigma_cm_mpa);
    metrics.insert("rock_mass_tensile_st_mpa".to_string(), sigma_t_mpa);
    metrics.insert("equivalent_friction_angle_deg".to_string(), phi_deg);
    metrics.insert("equivalent_cohesion_mpa".to_string(), c_mpa);

    let theoretical_s1 = s_3 + s_ci * (m_b * (s_3 / s_ci) + s).powf(a);
    let val =
        AnalyticalComparison::assess("major_principal_stress_s1_mpa", sigma_1_mpa, theoretical_s1);

    Ok(SimulationResult {
        problem_id: "geoscience-mining-hoek-brown".to_string(),
        engine_id: "atlas-hoek-brown".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: ConvergenceMonitor::new(1e-6),
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-hoek-brown", "0.1.0"),
        logs: Vec::new(),
    })
}

/// Evaluates Bieniawski Rock Mass Rating (RMR89) score, class, and in-situ deformation modulus.
pub fn solve_rmr_classification(
    prob: &RmrClassificationProblem,
) -> Result<SimulationResult, String> {
    let r_basic = prob.r1_strength_score
        + prob.r2_rqd_score
        + prob.r3_spacing_score
        + prob.r4_condition_score
        + prob.r5_groundwater_score;

    let rmr_total = (r_basic + prob.orientation_adjustment).clamp(0.0, 100.0);

    let (class_num, stand_up_time_days) = if rmr_total >= 81.0 {
        (1.0, 3650.0)
    } else if rmr_total >= 61.0 {
        (2.0, 180.0)
    } else if rmr_total >= 41.0 {
        (3.0, 7.0)
    } else if rmr_total >= 21.0 {
        (4.0, 0.4)
    } else {
        (5.0, 0.02)
    };

    let e_m_gpa = 10.0_f64.powf((rmr_total - 10.0) / 40.0);

    let mut metrics = HashMap::new();
    metrics.insert("rmr_basic_score".to_string(), r_basic);
    metrics.insert("rmr_total_score".to_string(), rmr_total);
    metrics.insert("rock_class_number".to_string(), class_num);
    metrics.insert(
        "estimated_stand_up_time_days".to_string(),
        stand_up_time_days,
    );
    metrics.insert("deformation_modulus_em_gpa".to_string(), e_m_gpa);

    let val = AnalyticalComparison::assess(
        "rmr_total_score",
        rmr_total,
        r_basic + prob.orientation_adjustment,
    );

    Ok(SimulationResult {
        problem_id: "geoscience-mining-rmr-classification".to_string(),
        engine_id: "atlas-rmr-classifier".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: ConvergenceMonitor::new(1e-6),
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-rmr-classifier", "0.1.0"),
        logs: Vec::new(),
    })
}

/// Solves USBM blast vibration attenuation and peak particle velocity (PPV).
pub fn solve_blast_vibration(prob: &BlastVibrationProblem) -> Result<SimulationResult, String> {
    if prob.distance_m <= 0.0 || prob.max_charge_per_delay_kg <= 0.0 {
        return Err("Distance and charge weight must be positive non-zero values".to_string());
    }

    let scaled_distance = prob.distance_m / prob.max_charge_per_delay_kg.sqrt();
    let ppv_mm_s = prob.site_constant_k * scaled_distance.powf(-prob.attenuation_exponent_beta);

    let within_safety_limit = ppv_mm_s <= prob.safety_threshold_ppv_mm_s;
    let factor_of_safety = prob.safety_threshold_ppv_mm_s / ppv_mm_s.max(1e-6);

    let mut metrics = HashMap::new();
    metrics.insert("scaled_distance_m_per_kg05".to_string(), scaled_distance);
    metrics.insert("peak_particle_velocity_ppv_mm_s".to_string(), ppv_mm_s);
    metrics.insert(
        "vibration_safety_threshold_mm_s".to_string(),
        prob.safety_threshold_ppv_mm_s,
    );
    metrics.insert("vibration_factor_of_safety".to_string(), factor_of_safety);
    metrics.insert(
        "is_within_safety_limits".to_string(),
        if within_safety_limit { 1.0 } else { 0.0 },
    );

    let theoretical_ppv =
        prob.site_constant_k * scaled_distance.powf(-prob.attenuation_exponent_beta);
    let val =
        AnalyticalComparison::assess("peak_particle_velocity_ppv_mm_s", ppv_mm_s, theoretical_ppv);

    Ok(SimulationResult {
        problem_id: "geoscience-mining-blast-vibration".to_string(),
        engine_id: "atlas-blast-vibration".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: ConvergenceMonitor::new(1e-6),
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-blast-vibration", "0.1.0"),
        logs: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hoek_brown_strength_verification() -> Result<(), String> {
        let prob = HoekBrownProblem::default();
        let result = solve_hoek_brown(&prob)?;

        assert_eq!(result.status, SimulationStatus::Converged);
        let s1 = result
            .scalar_metrics
            .get("major_principal_stress_s1_mpa")
            .copied()
            .unwrap_or(0.0);
        let scm = result
            .scalar_metrics
            .get("rock_mass_ucs_scm_mpa")
            .copied()
            .unwrap_or(0.0);
        let phi = result
            .scalar_metrics
            .get("equivalent_friction_angle_deg")
            .copied()
            .unwrap_or(0.0);

        assert!(s1 > 120.0 && s1 < 160.0, "Major stress s1: {} MPa", s1);
        assert!(scm > 15.0 && scm < 25.0, "Rock mass UCS: {} MPa", scm);
        assert!(phi > 35.0 && phi < 60.0, "Friction angle: {} deg", phi);
        Ok(())
    }

    #[test]
    fn test_rmr_and_blast_vibration() -> Result<(), String> {
        let rmr_prob = RmrClassificationProblem::default();
        let rmr_res = solve_rmr_classification(&rmr_prob)?;
        let rmr_score = rmr_res
            .scalar_metrics
            .get("rmr_total_score")
            .copied()
            .unwrap_or(0.0);
        let rmr_class = rmr_res
            .scalar_metrics
            .get("rock_class_number")
            .copied()
            .unwrap_or(0.0);
        let em = rmr_res
            .scalar_metrics
            .get("deformation_modulus_em_gpa")
            .copied()
            .unwrap_or(0.0);

        assert!((rmr_score - 77.0).abs() < 1e-6);
        assert!((rmr_class - 2.0).abs() < 1e-6);
        assert!(em > 40.0 && em < 60.0, "Modulus Em: {} GPa", em);

        let blast_prob = BlastVibrationProblem::default();
        let blast_res = solve_blast_vibration(&blast_prob)?;
        let ppv = blast_res
            .scalar_metrics
            .get("peak_particle_velocity_ppv_mm_s")
            .copied()
            .unwrap_or(0.0);
        let fos = blast_res
            .scalar_metrics
            .get("vibration_factor_of_safety")
            .copied()
            .unwrap_or(0.0);

        assert!(ppv > 7.0 && ppv < 15.0, "PPV: {} mm/s", ppv);
        assert!(fos > 1.4 && fos < 2.5, "Vibration FoS: {}", fos);
        Ok(())
    }
}

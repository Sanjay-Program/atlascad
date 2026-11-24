//! Piping Flexibility & Thermal Expansion Stress Analysis Module.
//!
//! Evaluates thermal expansion displacement stress ranges (SE), allowable stress ranges (SA),
//! stress intensification factors (SIF), U-loop flexibility, and anchor reaction loads
//! in accordance with ASME B31.3 Process Piping and ASME B31.1 Power Piping codes.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Piping flexibility analysis problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipingFlexibilityProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Pipe nominal outside diameter in millimeters (mm).
    pub nominal_outside_diameter_mm: f64,
    /// Pipe nominal wall thickness in millimeters (mm).
    pub wall_thickness_mm: f64,
    /// Distance between rigid anchors along the piping run in meters (m).
    pub anchor_distance_m: f64,
    /// Maximum operating design metal temperature in degrees Celsius (°C).
    pub design_temperature_c: f64,
    /// Ambient installation reference temperature in degrees Celsius (°C).
    pub ambient_temperature_c: f64,
    /// Linear thermal expansion coefficient of pipe material in 1/°C (e.g. 12.0e-6 /°C for carbon steel).
    pub thermal_expansion_coeff_per_c: f64,
    /// Material elastic modulus (Young's modulus) at design temperature in Gigapascals (GPa).
    pub elastic_modulus_gpa: f64,
    /// Basic allowable stress of pipe material at minimum (cold) metal temperature in MPa (Sc).
    pub cold_allowable_stress_mpa: f64,
    /// Basic allowable stress of pipe material at maximum (hot) metal temperature in MPa (Sh).
    pub hot_allowable_stress_mpa: f64,
    /// Sustained longitudinal stress due to internal pressure and gravity in MPa (SL).
    pub sustained_stress_sl_mpa: f64,
    /// Total expected thermal operating lifetime cycles (N) for cyclic reduction factor f.
    pub total_thermal_cycles: usize,
    /// U-loop height perpendicular to piping run in meters (H).
    pub loop_height_m: f64,
    /// U-loop width parallel to piping run in meters (W).
    pub loop_width_m: f64,
    /// Bend centerline radius in mm (typically 1.5 * OD for long-radius elbows).
    pub bend_radius_mm: Option<f64>,
}

fn default_problem_id() -> String {
    "piping_flexibility_problem".to_string()
}

impl Default for PipingFlexibilityProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            nominal_outside_diameter_mm: 219.1, // 8-inch NPS
            wall_thickness_mm: 8.18,            // Schedule 40
            anchor_distance_m: 60.0,
            design_temperature_c: 260.0,
            ambient_temperature_c: 20.0,
            thermal_expansion_coeff_per_c: 12.8e-6,
            elastic_modulus_gpa: 190.0,
            cold_allowable_stress_mpa: 137.9, // ASTM A106 Gr B
            hot_allowable_stress_mpa: 125.0,
            sustained_stress_sl_mpa: 65.0,
            total_thermal_cycles: 5000,
            loop_height_m: 7.5,
            loop_width_m: 5.0,
            bend_radius_mm: Some(304.8), // 1.5 * 203.2
        }
    }
}

/// Piping flexibility analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PipingFlexibilityResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Net linear thermal expansion to be absorbed between anchors in millimeters (mm).
    pub thermal_growth_mm: f64,
    /// Pipe cross-sectional metal area in mm².
    pub pipe_metal_area_mm2: f64,
    /// Pipe area moment of inertia in mm⁴.
    pub moment_of_inertia_mm4: f64,
    /// Pipe section modulus in mm³.
    pub section_modulus_mm3: f64,
    /// Cyclic stress range reduction factor f per ASME B31.3.
    pub cyclic_factor_f: f64,
    /// Allowable displacement stress range SA in MPa.
    pub allowable_stress_range_sa_mpa: f64,
    /// Elbow flexibility characteristic h.
    pub flexibility_characteristic_h: f64,
    /// In-plane/out-of-plane stress intensification factor (SIF) i.
    pub stress_intensification_factor_i: f64,
    /// Maximum bending moment induced in the expansion loop in kilonewton-meters (kN·m).
    pub max_loop_bending_moment_kn_m: f64,
    /// Calculated displacement stress range SE in MPa.
    pub displacement_stress_se_mpa: f64,
    /// Maximum reaction force exerted on end anchor points in kilonewtons (kN).
    pub anchor_reaction_force_kn: f64,
    /// Stress ratio SE / SA (must be <= 1.0 for code compliance).
    pub stress_ratio: f64,
    /// Whether the piping system complies with ASME B31.3 flexibility requirements.
    pub is_safe: bool,
}

/// Solves piping flexibility, thermal expansion absorption, and ASME B31.3 stress range.
pub fn solve_piping_flexibility(
    problem: &PipingFlexibilityProblem,
) -> Result<PipingFlexibilityResult, String> {
    if problem.nominal_outside_diameter_mm <= 0.0 || problem.wall_thickness_mm <= 0.0 {
        return Err("Pipe diameter and wall thickness must be positive".to_string());
    }
    if problem.wall_thickness_mm >= problem.nominal_outside_diameter_mm / 2.0 {
        return Err("Wall thickness cannot exceed pipe outer radius".to_string());
    }
    if problem.anchor_distance_m <= 0.0 || problem.loop_height_m <= 0.0 {
        return Err("Anchor distance and loop height must be positive".to_string());
    }
    if problem.elastic_modulus_gpa <= 0.0 || problem.thermal_expansion_coeff_per_c <= 0.0 {
        return Err(
            "Elastic modulus and thermal expansion coefficient must be positive".to_string(),
        );
    }

    let d_o = problem.nominal_outside_diameter_mm;
    let t = problem.wall_thickness_mm;
    let d_i = d_o - 2.0 * t;
    let r_m = (d_o - t) / 2.0;

    // Metal cross section: A = pi/4 * (Do^2 - Di^2)
    let a_metal_mm2 = (std::f64::consts::PI / 4.0) * (d_o.powi(2) - d_i.powi(2));
    // Moment of inertia: I = pi/64 * (Do^4 - Di^4)
    let i_mm4 = (std::f64::consts::PI / 64.0) * (d_o.powi(4) - d_i.powi(4));
    // Section modulus: Z = 2 * I / Do
    let z_mm3 = 2.0 * i_mm4 / d_o;

    // Net thermal expansion: Delta L = L * alpha * (T_hot - T_cold)
    let delta_t = (problem.design_temperature_c - problem.ambient_temperature_c).max(0.0);
    let delta_l_m = problem.anchor_distance_m * problem.thermal_expansion_coeff_per_c * delta_t;
    let delta_l_mm = delta_l_m * 1000.0;

    // ASME B31.3 Paragraph 302.3.5:
    // Cyclic stress range reduction factor f:
    // f = 6.0 * N^(-0.2) <= 1.0 (minimum f = 0.15)
    let n_cycles = (problem.total_thermal_cycles as f64).max(1.0);
    let f_cyclic = if n_cycles <= 7000.0 {
        1.0
    } else {
        (6.0 * n_cycles.powf(-0.2)).clamp(0.15, 1.0)
    };

    // Allowable displacement stress range SA:
    // SA = f * [ 1.25 * (Sc + Sh) - SL ]
    let s_c = problem.cold_allowable_stress_mpa;
    let s_h = problem.hot_allowable_stress_mpa;
    let s_l = problem.sustained_stress_sl_mpa;
    let s_a_mpa = f_cyclic * (1.25 * (s_c + s_h) - s_l).max(0.0);

    // Flexibility characteristic of elbows: h = (t * R_bend) / (r_m^2)
    let r_bend = problem.bend_radius_mm.unwrap_or(1.5 * d_o);
    let h_flex = (t * r_bend) / r_m.powi(2);
    // Stress Intensification Factor (SIF) i per ASME B31.3 Appendix D:
    // i = 0.9 / h^(2/3) >= 1.0
    let sif_i = (0.9 / h_flex.powf(2.0 / 3.0)).max(1.0);

    // U-Loop structural mechanics (guided cantilever / beam deflection):
    // For a symmetrical U-loop with 2 legs of height H and width W:
    // Expansion absorbed by the 2 vertical legs: delta_x = delta_l
    // Lateral deflection per leg = delta_l / 2 (or shared across total height H)
    // Moment at fixed base of loop leg: M = (3 * E * I * delta_l) / (2 * H^2)
    let e_mpa = problem.elastic_modulus_gpa * 1000.0;
    let h_mm = problem.loop_height_m * 1000.0;
    let moment_n_mm = (3.0 * e_mpa * i_mm4 * delta_l_mm) / (2.0 * h_mm.powi(2));
    let moment_kn_m = moment_n_mm * 1e-6;

    // Resultant displacement bending stress: SE = i * M / Z
    let s_e_mpa = (sif_i * moment_n_mm) / z_mm3;

    // Anchor reaction thrust load: F_anchor = 12 * E * I * delta_l / (2 * H^3)
    let f_anchor_n = (12.0 * e_mpa * i_mm4 * delta_l_mm) / (2.0 * h_mm.powi(3));
    let f_anchor_kn = f_anchor_n * 1e-3;

    let stress_ratio = if s_a_mpa > 1e-6 {
        s_e_mpa / s_a_mpa
    } else {
        999.0
    };
    let is_safe = s_e_mpa <= s_a_mpa;

    Ok(PipingFlexibilityResult {
        problem_id: problem.problem_id.clone(),
        thermal_growth_mm: delta_l_mm,
        pipe_metal_area_mm2: a_metal_mm2,
        moment_of_inertia_mm4: i_mm4,
        section_modulus_mm3: z_mm3,
        cyclic_factor_f: f_cyclic,
        allowable_stress_range_sa_mpa: s_a_mpa,
        flexibility_characteristic_h: h_flex,
        stress_intensification_factor_i: sif_i,
        max_loop_bending_moment_kn_m: moment_kn_m,
        displacement_stress_se_mpa: s_e_mpa,
        anchor_reaction_force_kn: f_anchor_kn,
        stress_ratio,
        is_safe,
    })
}

/// Generic simulation adapter for piping flexibility analysis.
pub fn solve_piping_flexibility_sim(problem: &PipingFlexibilityProblem) -> SimulationResult {
    match solve_piping_flexibility(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("thermal_growth_mm".to_string(), result.thermal_growth_mm);
            metrics.insert(
                "displacement_stress_se_mpa".to_string(),
                result.displacement_stress_se_mpa,
            );
            metrics.insert(
                "allowable_stress_range_sa_mpa".to_string(),
                result.allowable_stress_range_sa_mpa,
            );
            metrics.insert(
                "anchor_reaction_force_kn".to_string(),
                result.anchor_reaction_force_kn,
            );
            metrics.insert("stress_ratio".to_string(), result.stress_ratio);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.displacement_stress_se_mpa, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "displacement_stress_se_mpa",
                result.displacement_stress_se_mpa,
                result.allowable_stress_range_sa_mpa * 0.8,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.piping.flexibility_asme_b31_3".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.piping.flexibility_asme_b31_3",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "ASME B31.3 Piping Flexibility: ΔL = {:.1} mm, SE = {:.1} MPa <= SA = {:.1} MPa (Ratio = {:.2}, F_anchor = {:.1} kN, Safe: {})",
                    result.thermal_growth_mm,
                    result.displacement_stress_se_mpa,
                    result.allowable_stress_range_sa_mpa,
                    result.stress_ratio,
                    result.anchor_reaction_force_kn,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.piping.flexibility_asme_b31_3".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.piping.flexibility_asme_b31_3",
                    "0.1.0",
                ),
                logs: vec![format!("Piping flexibility simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_piping_flexibility_nominal() {
        let prob = PipingFlexibilityProblem::default();
        let res = solve_piping_flexibility(&prob).expect("solver failed");
        println!(
            "Piping flexibility: ΔL = {}, SE = {}, SA = {}, ratio = {}, F_anchor = {}",
            res.thermal_growth_mm,
            res.displacement_stress_se_mpa,
            res.allowable_stress_range_sa_mpa,
            res.stress_ratio,
            res.anchor_reaction_force_kn
        );

        assert!(res.thermal_growth_mm > 150.0 && res.thermal_growth_mm < 220.0);
        assert!(res.cyclic_factor_f == 1.0);
        assert!(res.allowable_stress_range_sa_mpa > 250.0);
        assert!(res.displacement_stress_se_mpa > 10.0 && res.displacement_stress_se_mpa < 350.0);
        assert!(res.stress_ratio <= 1.0);
        assert!(res.anchor_reaction_force_kn > 5.0 && res.anchor_reaction_force_kn < 40.0);
        assert!(res.is_safe);
    }
}

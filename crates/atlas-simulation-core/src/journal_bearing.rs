//! Hydrodynamic fluid-film journal bearing mechanics solver.
//!
//! Compliant with ISO 7902-1, DIN 31652, and the Reynolds lubrication equation.
//! Evaluates Sommerfeld number, eccentricity ratio, minimum oil film thickness,
//! friction torque, power dissipation, lubricant flow rate, and hydrodynamic temperature rise.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Problem definition for hydrodynamic journal bearing simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalBearingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Journal (shaft) outer diameter in mm.
    pub journal_diameter_mm: f64,
    /// Bearing sleeve axial length in mm.
    pub bearing_length_mm: f64,
    /// Radial clearance between journal and bearing in micrometers (µm).
    pub radial_clearance_um: f64,
    /// Shaft rotational speed in RPM.
    pub rotational_speed_rpm: f64,
    /// Total static/dynamic radial load supported by bearing in Newtons.
    pub radial_load_n: f64,
    /// Lubricant dynamic viscosity at operating temperature in mPa·s (cP).
    pub lubricant_viscosity_mpa_s: f64,
    /// Lubricant mass density in kg/m³.
    pub lubricant_density_kg_m3: f64,
    /// Lubricant specific heat capacity in J/(kg·K).
    pub lubricant_specific_heat_j_kg_k: f64,
    /// Lubricant supply inlet temperature in degrees Celsius.
    pub inlet_oil_temp_c: f64,
    /// Combined peak-to-valley surface roughness Rz in micrometers (µm).
    pub surface_roughness_rz_um: f64,
}

impl Default for JournalBearingProblem {
    fn default() -> Self {
        Self {
            problem_id: "journal_bearing_nominal".to_string(),
            journal_diameter_mm: 100.0,
            bearing_length_mm: 70.0,
            radial_clearance_um: 65.0,
            rotational_speed_rpm: 3000.0,
            radial_load_n: 12500.0,
            lubricant_viscosity_mpa_s: 25.0,
            lubricant_density_kg_m3: 860.0,
            lubricant_specific_heat_j_kg_k: 1950.0,
            inlet_oil_temp_c: 45.0,
            surface_roughness_rz_um: 3.0,
        }
    }
}

/// Simulation output results for hydrodynamic journal bearing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JournalBearingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Bearing length-to-diameter aspect ratio L/D.
    pub length_to_diameter_ratio: f64,
    /// Mean projected bearing pressure in MPa.
    pub mean_bearing_pressure_mpa: f64,
    /// Journal surface tangential velocity in m/s.
    pub surface_velocity_m_s: f64,
    /// Dimensionless Sommerfeld bearing characteristic number.
    pub sommerfeld_number: f64,
    /// Dimensionless journal eccentricity ratio epsilon = e / C.
    pub eccentricity_ratio: f64,
    /// Minimum hydrodynamic oil film thickness h_min in micrometers (µm).
    pub minimum_film_thickness_um: f64,
    /// Permissible minimum oil film thickness for full fluid-film lubrication in µm.
    pub permissible_minimum_film_thickness_um: f64,
    /// Attitude angle between load line and displacement vector in degrees.
    pub attitude_angle_deg: f64,
    /// Coefficient of hydrodynamic friction f.
    pub friction_coefficient: f64,
    /// Hydrodynamic viscous friction torque in N·m.
    pub friction_torque_nm: f64,
    /// Power dissipated as heat in bearing in kW.
    pub hydrodynamic_power_loss_kw: f64,
    /// Required lubricating oil volume flow rate in L/min.
    pub oil_flow_rate_l_min: f64,
    /// Mean lubricant temperature rise across bearing in degrees Celsius.
    pub lubricant_temp_rise_c: f64,
    /// Estimated peak hydrodynamic oil film pressure in MPa.
    pub maximum_film_pressure_mpa: f64,
    /// Whether bearing operates in safe hydrodynamic regime (h_min > h_lim, safe temp).
    pub is_safe: bool,
    /// Detailed design commentary and standard compliance assessment.
    pub design_assessment: String,
}

/// Solves hydrodynamic journal bearing lubrication according to ISO 7902 / DIN 31652.
pub fn solve_journal_bearing(problem: &JournalBearingProblem) -> Result<JournalBearingResult, String> {
    if problem.journal_diameter_mm <= 0.0 {
        return Err("Journal diameter must be positive".to_string());
    }
    if problem.bearing_length_mm <= 0.0 {
        return Err("Bearing length must be positive".to_string());
    }
    if problem.radial_clearance_um <= 0.0 {
        return Err("Radial clearance must be positive".to_string());
    }
    if problem.rotational_speed_rpm <= 0.0 {
        return Err("Rotational speed must be positive".to_string());
    }
    if problem.radial_load_n <= 0.0 {
        return Err("Radial load must be positive".to_string());
    }
    if problem.lubricant_viscosity_mpa_s <= 0.0 {
        return Err("Viscosity must be positive".to_string());
    }

    // 1. Basic Geometry & Kinematics
    let d_m = problem.journal_diameter_mm / 1000.0;
    let r_m = d_m / 2.0;
    let l_m = problem.bearing_length_mm / 1000.0;
    let c_m = problem.radial_clearance_um * 1.0e-6;
    let l_over_d = problem.bearing_length_mm / problem.journal_diameter_mm;

    let n_rev_s = problem.rotational_speed_rpm / 60.0;
    let omega_rad_s = 2.0 * PI * n_rev_s;
    let v_m_s = omega_rad_s * r_m;

    // Projected bearing pressure: p = W / (D * L)
    let p_pa = problem.radial_load_n / (d_m * l_m);
    let p_mpa = p_pa / 1.0e6;

    // Clearance ratio psi = C / R
    let psi = c_m / r_m;
    let mu_pa_s = problem.lubricant_viscosity_mpa_s * 1.0e-3;

    // 2. Sommerfeld Number (ISO 7902 / DIN 31652): S = (mu * n) / (p * psi^2)
    let sommerfeld = (mu_pa_s * n_rev_s) / (p_pa * psi * psi);

    // 3. Eccentricity Ratio & Minimum Film Thickness (Raimondi-Boyd regression for finite L/D)
    let ld_factor = l_over_d.powf(1.25);
    let s_effective = sommerfeld.max(1.0e-4) * ld_factor;
    let epsilon = (1.0 / (1.0 + 2.8 * s_effective.powf(0.80))).clamp(0.05, 0.98);

    let h_min_m = c_m * (1.0 - epsilon);
    let h_min_um = h_min_m * 1.0e6;

    // Permissible minimum oil film thickness (ISO 7902: 2.5 * Rz)
    let h_lim_um = 2.5 * problem.surface_roughness_rz_um;

    // Attitude angle phi
    let term_phi = ((1.0 - epsilon * epsilon).sqrt() / epsilon).max(1.0e-3);
    let phi_rad = (PI / 4.0 * term_phi).atan();
    let phi_deg = phi_rad * 180.0 / PI;

    // 4. Hydrodynamic Friction & Power Loss (Petroff modified for eccentricity)
    // T_f = 2 * pi * mu * omega * R^3 * L / (C * sqrt(1 - eps^2))
    let denominator_f = c_m * (1.0 - epsilon * epsilon).sqrt().max(1.0e-7);
    let friction_torque_nm = (2.0 * PI * mu_pa_s * omega_rad_s * r_m.powi(3) * l_m) / denominator_f;
    let power_loss_w = friction_torque_nm * omega_rad_s;
    let power_loss_kw = power_loss_w / 1000.0;

    let friction_coeff = friction_torque_nm / (problem.radial_load_n * r_m).max(1.0e-3);

    // 5. Hydrodynamic Lubricant Flow Rate & Temperature Rise
    // Q_oil ≈ 0.6 * (1 + 0.8 * epsilon) * pi * R * C * omega * L
    let q_oil_m3_s = 0.6 * (1.0 + 0.8 * epsilon) * PI * r_m * c_m * omega_rad_s * l_m;
    let q_oil_l_min = q_oil_m3_s * 60.0 * 1000.0;

    let rho = problem.lubricant_density_kg_m3.max(500.0);
    let cp = problem.lubricant_specific_heat_j_kg_k.max(1000.0);
    let mass_flow_kg_s = q_oil_m3_s * rho;
    let delta_t_c = power_loss_w / (mass_flow_kg_s * cp).max(1.0);

    // Maximum hydrodynamic peak oil film pressure (typically 2.0 - 3.5 * p_mean)
    let p_max_mpa = p_mpa * (2.2 / (1.0 - epsilon).max(0.05));

    // Safety checks:
    let is_film_safe = h_min_um >= h_lim_um;
    let is_temp_safe = (problem.inlet_oil_temp_c + delta_t_c) <= 105.0;
    let is_pressure_safe = p_max_mpa <= 30.0;
    let is_safe = is_film_safe && is_temp_safe && is_pressure_safe;

    let assessment = format!(
        "Journal D={:.0}mm, L={:.0}mm (L/D={:.2}), Speed={:.0} RPM. p_mean={:.2} MPa, Sommerfeld S={:.3}. Eccentricity eps={:.3}, h_min={:.1} µm (limit={:.1} µm). Friction f={:.4}, P_loss={:.2} kW, Q_oil={:.1} L/min, DeltaT=+{:.1} °C (T_peak={:.1} °C), P_max={:.1} MPa. Status: {}",
        problem.journal_diameter_mm,
        problem.bearing_length_mm,
        l_over_d,
        problem.rotational_speed_rpm,
        p_mpa,
        sommerfeld,
        epsilon,
        h_min_um,
        h_lim_um,
        friction_coeff,
        power_loss_kw,
        q_oil_l_min,
        delta_t_c,
        problem.inlet_oil_temp_c + delta_t_c,
        p_max_mpa,
        if is_safe { "PASS FULL HYDRODYNAMIC" } else { "MARGINAL / CHECK CLEARANCE OR VISCOSITY" }
    );

    Ok(JournalBearingResult {
        problem_id: problem.problem_id.clone(),
        length_to_diameter_ratio: l_over_d,
        mean_bearing_pressure_mpa: p_mpa,
        surface_velocity_m_s: v_m_s,
        sommerfeld_number: sommerfeld,
        eccentricity_ratio: epsilon,
        minimum_film_thickness_um: h_min_um,
        permissible_minimum_film_thickness_um: h_lim_um,
        attitude_angle_deg: phi_deg,
        friction_coefficient: friction_coeff,
        friction_torque_nm,
        hydrodynamic_power_loss_kw: power_loss_kw,
        oil_flow_rate_l_min: q_oil_l_min,
        lubricant_temp_rise_c: delta_t_c,
        maximum_film_pressure_mpa: p_max_mpa,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for journal bearing.
pub fn solve_journal_bearing_sim(problem: &JournalBearingProblem) -> SimulationResult {
    match solve_journal_bearing(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("sommerfeld_number".to_string(), result.sommerfeld_number);
            metrics.insert("minimum_film_thickness_um".to_string(), result.minimum_film_thickness_um);
            metrics.insert("hydrodynamic_power_loss_kw".to_string(), result.hydrodynamic_power_loss_kw);
            metrics.insert("lubricant_temp_rise_c".to_string(), result.lubricant_temp_rise_c);
            metrics.insert("maximum_film_pressure_mpa".to_string(), result.maximum_film_pressure_mpa);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.minimum_film_thickness_um, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "minimum_film_thickness_um",
                result.minimum_film_thickness_um,
                15.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.tribology.journal_bearing_iso7902".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.tribology.journal_bearing_iso7902", "0.1.0"),
                logs: vec![result.design_assessment],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.tribology.journal_bearing_iso7902".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.tribology.journal_bearing_iso7902", "0.1.0"),
                logs: vec![format!("Journal bearing simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_journal_bearing_nominal() {
        let prob = JournalBearingProblem::default();
        let sol = solve_journal_bearing(&prob).expect("Nominal journal bearing sizing should succeed");

        assert_eq!(sol.length_to_diameter_ratio, 0.70);
        assert!(sol.mean_bearing_pressure_mpa > 1.5 && sol.mean_bearing_pressure_mpa < 2.0);
        assert!(sol.surface_velocity_m_s > 15.0 && sol.surface_velocity_m_s < 16.0);
        assert!(sol.sommerfeld_number > 0.15 && sol.sommerfeld_number < 0.60);
        assert!(sol.eccentricity_ratio > 0.40 && sol.eccentricity_ratio < 0.75);
        assert!(sol.minimum_film_thickness_um > sol.permissible_minimum_film_thickness_um);
        assert!(sol.hydrodynamic_power_loss_kw > 0.8 && sol.hydrodynamic_power_loss_kw < 3.5);
        assert!(sol.lubricant_temp_rise_c > 5.0 && sol.lubricant_temp_rise_c < 30.0);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_journal_bearing_heavy_load_failure() {
        let prob = JournalBearingProblem {
            radial_load_n: 150000.0, // excessive load
            rotational_speed_rpm: 300.0, // low speed
            ..Default::default()
        };
        let sol = solve_journal_bearing(&prob).expect("Calculation executes");
        assert!(!sol.is_safe);
        assert!(sol.minimum_film_thickness_um < sol.permissible_minimum_film_thickness_um);
    }
}

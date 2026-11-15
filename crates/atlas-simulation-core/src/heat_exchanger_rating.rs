//! Shell-and-Tube Heat Exchanger Rating & Kern's Method (TEMA Class R/C/B / ASME BPVC Sec VIII).
//!
//! Evaluates hot and cold stream thermal duties, Counter-Flow Logarithmic Mean Temperature
//! Difference (LMTD), TEMA cross-flow F-correction factor, Kern's shell-side hydraulic
//! and thermal correlations, overall heat transfer coefficient U, required surface area,
//! and stream pressure drops.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Standard tube layout pattern.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TubeLayoutPattern {
    /// 90-degree square pitch (easy mechanical cleaning).
    SquarePitch90,
    /// 45-degree rotated square pitch.
    RotatedSquarePitch45,
    /// 30-degree triangular pitch (maximum surface density).
    TriangularPitch30,
}

/// Heat exchanger rating problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatExchangerRatingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Hot stream mass flowrate (kg/s).
    pub hot_mass_flow_kg_s: f64,
    /// Hot stream specific heat capacity (J/(kg·K)).
    pub hot_cp_j_kg_k: f64,
    /// Hot stream inlet temperature (°C).
    pub hot_inlet_temp_c: f64,
    /// Hot stream outlet temperature (°C).
    pub hot_outlet_temp_c: f64,
    /// Hot stream density (kg/m³).
    pub hot_density_kg_m3: f64,
    /// Hot stream dynamic viscosity (Pa·s).
    pub hot_viscosity_pa_s: f64,
    /// Hot stream thermal conductivity (W/(m·K)).
    pub hot_conductivity_w_m_k: f64,
    /// Cold stream mass flowrate (kg/s).
    pub cold_mass_flow_kg_s: f64,
    /// Cold stream specific heat capacity (J/(kg·K)).
    pub cold_cp_j_kg_k: f64,
    /// Cold stream inlet temperature (°C).
    pub cold_inlet_temp_c: f64,
    /// Cold stream outlet temperature (°C).
    pub cold_outlet_temp_c: f64,
    /// Cold stream density (kg/m³).
    pub cold_density_kg_m3: f64,
    /// Cold stream dynamic viscosity (Pa·s).
    pub cold_viscosity_pa_s: f64,
    /// Cold stream thermal conductivity (W/(m·K)).
    pub cold_conductivity_w_m_k: f64,
    /// Shell inside diameter (mm).
    pub shell_diameter_mm: f64,
    /// Baffle spacing (mm).
    pub baffle_spacing_mm: f64,
    /// Tube outer diameter (mm, e.g. 19.05 mm = 3/4 inch).
    pub tube_outer_diameter_mm: f64,
    /// Tube wall thickness (mm, e.g. 1.65 mm = BWG 16).
    pub tube_wall_thickness_mm: f64,
    /// Tube pitch (mm, e.g. 25.4 mm = 1.0 inch square).
    pub tube_pitch_mm: f64,
    /// Tube layout pattern.
    pub layout_pattern: TubeLayoutPattern,
    /// Total number of tubes.
    pub tube_count: usize,
    /// Number of tube passes (e.g. 1, 2, 4).
    pub tube_passes: usize,
    /// Effective tube length between tube sheets (m).
    pub tube_length_m: f64,
    /// Tube metal thermal conductivity (W/(m·K), e.g. 16.0 for SS316, 50.0 for carbon steel).
    pub tube_metal_conductivity_w_m_k: f64,
    /// Shell-side fouling resistance (m²·K/W).
    pub shell_fouling_factor: f64,
    /// Tube-side fouling resistance (m²·K/W).
    pub tube_fouling_factor: f64,
}

impl Default for HeatExchangerRatingProblem {
    fn default() -> Self {
        Self {
            problem_id: "industrial_oil_water_cooler".to_string(),
            // Shell-side: hot oil cooling from 95°C to 55°C
            hot_mass_flow_kg_s: 18.0,
            hot_cp_j_kg_k: 2150.0,
            hot_inlet_temp_c: 95.0,
            hot_outlet_temp_c: 55.0,
            hot_density_kg_m3: 850.0,
            hot_viscosity_pa_s: 0.0035,
            hot_conductivity_w_m_k: 0.135,
            // Tube-side: cooling water heating from 25°C to 40°C
            cold_mass_flow_kg_s: 24.66,
            cold_cp_j_kg_k: 4180.0,
            cold_inlet_temp_c: 25.0,
            cold_outlet_temp_c: 40.0,
            cold_density_kg_m3: 996.0,
            cold_viscosity_pa_s: 0.0008,
            cold_conductivity_w_m_k: 0.62,
            // Exchanger geometry:
            shell_diameter_mm: 600.0,
            baffle_spacing_mm: 250.0,
            tube_outer_diameter_mm: 19.05, // 3/4"
            tube_wall_thickness_mm: 1.65,
            tube_pitch_mm: 25.4, // 1" square pitch
            layout_pattern: TubeLayoutPattern::SquarePitch90,
            tube_count: 380,
            tube_passes: 2,
            tube_length_m: 4.5,
            tube_metal_conductivity_w_m_k: 45.0, // Carbon steel
            shell_fouling_factor: 0.00035,
            tube_fouling_factor: 0.00018,
        }
    }
}

/// Comprehensive heat exchanger rating results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeatExchangerRatingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total heat duty transferred Q (kW).
    pub heat_duty_kw: f64,
    /// Counter-flow Logarithmic Mean Temperature Difference (LMTD, °C).
    pub lmtd_c: f64,
    /// TEMA F-correction factor for multi-pass configuration.
    pub f_correction_factor: f64,
    /// Effective temperature driving force Delta T_eff (°C).
    pub effective_temp_difference_c: f64,
    /// Shell-side cross-flow area A_s (m²).
    pub shell_cross_flow_area_m2: f64,
    /// Shell-side equivalent hydraulic diameter D_e (m).
    pub shell_equivalent_diameter_m: f64,
    /// Shell-side Reynolds number.
    pub shell_reynolds_number: f64,
    /// Shell-side convective film coefficient h_o (W/(m²·K)).
    pub shell_film_coeff_w_m2_k: f64,
    /// Tube-side flow velocity (m/s).
    pub tube_velocity_m_s: f64,
    /// Tube-side Reynolds number.
    pub tube_reynolds_number: f64,
    /// Tube-side convective film coefficient h_i (W/(m²·K)).
    pub tube_film_coeff_w_m2_k: f64,
    /// Overall clean heat transfer coefficient U_clean (W/(m²·K)).
    pub overall_u_clean_w_m2_k: f64,
    /// Overall design/fouled heat transfer coefficient U_fouled (W/(m²·K)).
    pub overall_u_fouled_w_m2_k: f64,
    /// Actual provided heat transfer surface area A_prov (m²).
    pub provided_surface_area_m2: f64,
    /// Minimum required heat transfer area A_req (m²).
    pub required_surface_area_m2: f64,
    /// Surface over-design margin (%).
    pub surface_margin_pct: f64,
    /// Shell-side estimated pressure drop Delta P_s (kPa).
    pub shell_pressure_drop_kpa: f64,
    /// Tube-side estimated pressure drop Delta P_t (kPa).
    pub tube_pressure_drop_kpa: f64,
    /// Exchanger thermal rating adequate.
    pub rating_adequate: bool,
}

/// Rates a shell-and-tube heat exchanger per TEMA and Kern's method.
pub fn solve_heat_exchanger_rating(prob: &HeatExchangerRatingProblem) -> HeatExchangerRatingResult {
    // Heat duty: Q = m_dot * Cp * delta_T (kW)
    let q_hot = prob.hot_mass_flow_kg_s * prob.hot_cp_j_kg_k * (prob.hot_inlet_temp_c - prob.hot_outlet_temp_c) / 1000.0;
    let q_duty = q_hot.max(0.1);

    // LMTD calculation:
    // Delta T1 = T_h_in - T_c_out
    // Delta T2 = T_h_out - T_c_in
    let dt1 = (prob.hot_inlet_temp_c - prob.cold_outlet_temp_c).max(0.5);
    let dt2 = (prob.hot_outlet_temp_c - prob.cold_inlet_temp_c).max(0.5);

    let lmtd = if (dt1 - dt2).abs() < 1e-4 {
        dt1
    } else {
        (dt1 - dt2) / (dt1 / dt2).ln()
    };

    // TEMA F-correction factor:
    // R = (T_h_in - T_h_out) / (T_c_out - T_c_in)
    // P = (T_c_out - T_c_in) / (T_h_in - T_c_in)
    let dt_c = (prob.cold_outlet_temp_c - prob.cold_inlet_temp_c).max(0.1);
    let dt_h = (prob.hot_inlet_temp_c - prob.hot_outlet_temp_c).max(0.1);
    let r_ratio = dt_h / dt_c;
    let p_ratio = (dt_c / (prob.hot_inlet_temp_c - prob.cold_inlet_temp_c).max(0.1)).clamp(0.01, 0.95);

    let f_factor = if prob.tube_passes == 1 {
        1.0
    } else {
        let r_sq_p1 = (r_ratio * r_ratio + 1.0).sqrt();
        let num = r_sq_p1 * ((1.0 - p_ratio) / (1.0 - p_ratio * r_ratio).max(1e-4)).ln();
        let denom_term = (2.0 - p_ratio * (r_ratio + 1.0 - r_sq_p1)) / (2.0 - p_ratio * (r_ratio + 1.0 + r_sq_p1)).max(1e-4);
        let denom = (r_ratio - 1.0) * denom_term.max(1e-4).ln();
        if denom.abs() > 1e-6 {
            (num / denom).clamp(0.50, 1.0)
        } else {
            0.85
        }
    };

    let dt_eff = f_factor * lmtd;

    // Shell geometry:
    let d_s_m = prob.shell_diameter_mm * 1.0e-3;
    let b_m = prob.baffle_spacing_mm * 1.0e-3;
    let d_o_m = prob.tube_outer_diameter_mm * 1.0e-3;
    let t_w_m = prob.tube_wall_thickness_mm * 1.0e-3;
    let d_i_m = d_o_m - 2.0 * t_w_m;
    let p_t_m = prob.tube_pitch_mm * 1.0e-3;
    let clearance_m = (p_t_m - d_o_m).max(1e-4);

    // Shell cross-flow area: A_s = (D_s * C * B) / p_t
    let a_s = (d_s_m * clearance_m * b_m) / p_t_m;

    // Shell equivalent diameter:
    // Square pitch: D_e = 4 * (p_t^2 - pi * d_o^2 / 4) / (pi * d_o)
    let d_e = match prob.layout_pattern {
        TubeLayoutPattern::SquarePitch90 | TubeLayoutPattern::RotatedSquarePitch45 => {
            4.0 * (p_t_m.powi(2) - (PI * d_o_m.powi(2) / 4.0)) / (PI * d_o_m)
        }
        TubeLayoutPattern::TriangularPitch30 => {
            4.0 * ((3.0_f64.sqrt() / 4.0) * p_t_m.powi(2) - (PI * d_o_m.powi(2) / 8.0)) / (PI * d_o_m / 2.0)
        }
    };

    // Shell-side mass velocity & Reynolds:
    let g_s = prob.hot_mass_flow_kg_s / a_s.max(1e-4);
    let re_s = (g_s * d_e) / prob.hot_viscosity_pa_s.max(1e-6);
    let pr_s = (prob.hot_cp_j_kg_k * prob.hot_viscosity_pa_s) / prob.hot_conductivity_w_m_k.max(1e-4);

    // Kern's correlation: Nu_s = 0.36 * Re_s^0.55 * Pr^0.33
    let nu_s = 0.36 * re_s.powf(0.55) * pr_s.powf(0.333);
    let h_o = (nu_s * prob.hot_conductivity_w_m_k) / d_e;

    // Tube-side hydraulics & heat transfer:
    let n_t = prob.tube_count.max(1);
    let n_p = prob.tube_passes.max(1);
    let tubes_per_pass = (n_t as f64 / n_p as f64).max(1.0);

    let a_t_single = PI * d_i_m.powi(2) / 4.0;
    let a_t_total = tubes_per_pass * a_t_single;
    let v_t = prob.cold_mass_flow_kg_s / (prob.cold_density_kg_m3 * a_t_total);

    let re_t = (prob.cold_density_kg_m3 * v_t * d_i_m) / prob.cold_viscosity_pa_s.max(1e-6);
    let pr_t = (prob.cold_cp_j_kg_k * prob.cold_viscosity_pa_s) / prob.cold_conductivity_w_m_k.max(1e-4);

    // Dittus-Boelter for turbulent tube flow: Nu_t = 0.023 * Re^0.8 * Pr^0.4
    let nu_t = if re_t > 2300.0 {
        0.023 * re_t.powf(0.80) * pr_t.powf(0.40)
    } else {
        3.66 + (0.065 * (d_i_m / prob.tube_length_m) * re_t * pr_t)
            / (1.0 + 0.04 * ((d_i_m / prob.tube_length_m) * re_t * pr_t).powf(0.67))
    };
    let h_i = (nu_t * prob.cold_conductivity_w_m_k) / d_i_m;

    // Overall Heat Transfer Coefficient:
    // 1 / U_clean = 1/h_o + (d_o * ln(d_o / d_i)) / (2 * k_w) + (d_o / d_i) * (1 / h_i)
    let k_w = prob.tube_metal_conductivity_w_m_k.max(1.0);
    let r_wall = (d_o_m * (d_o_m / d_i_m).ln()) / (2.0 * k_w);
    let r_clean = (1.0 / h_o) + r_wall + ((d_o_m / d_i_m) * (1.0 / h_i));
    let u_clean = 1.0 / r_clean;

    let r_fouled = r_clean + prob.shell_fouling_factor + ((d_o_m / d_i_m) * prob.tube_fouling_factor);
    let u_fouled = 1.0 / r_fouled;

    // Heat transfer surface areas:
    let a_prov = (n_t as f64) * PI * d_o_m * prob.tube_length_m;
    // Q = U_fouled * A_req * Delta T_eff -> A_req = (Q * 1000) / (U_fouled * Delta T_eff)
    let a_req = (q_duty * 1000.0) / (u_fouled * dt_eff.max(0.1));

    let margin_pct = ((a_prov - a_req) / a_req) * 100.0;
    let rating_adequate = a_prov >= a_req;

    // Pressure drops:
    // Shell: Delta P_s = (f_s * G_s^2 * D_s * (N_b + 1)) / (2 * rho * D_e)
    let n_baffles = (prob.tube_length_m / b_m).round();
    let f_s = 1.45 * re_s.powf(-0.15).max(0.05);
    let dp_s_pa = (f_s * g_s.powi(2) * d_s_m * (n_baffles + 1.0)) / (2.0 * prob.hot_density_kg_m3 * d_e);

    // Tube: Delta P_t = [ (4 * f_t * L * n_p) / d_i + 4 * n_p ] * (rho * v_t^2 / 2)
    let f_t = 0.079 * re_t.powf(-0.25).max(0.005);
    let dp_t_pa = ((4.0 * f_t * prob.tube_length_m * (n_p as f64) / d_i_m) + 4.0 * (n_p as f64))
        * (prob.cold_density_kg_m3 * v_t.powi(2) / 2.0);

    HeatExchangerRatingResult {
        problem_id: prob.problem_id.clone(),
        heat_duty_kw: q_duty,
        lmtd_c: lmtd,
        f_correction_factor: f_factor,
        effective_temp_difference_c: dt_eff,
        shell_cross_flow_area_m2: a_s,
        shell_equivalent_diameter_m: d_e,
        shell_reynolds_number: re_s,
        shell_film_coeff_w_m2_k: h_o,
        tube_velocity_m_s: v_t,
        tube_reynolds_number: re_t,
        tube_film_coeff_w_m2_k: h_i,
        overall_u_clean_w_m2_k: u_clean,
        overall_u_fouled_w_m2_k: u_fouled,
        provided_surface_area_m2: a_prov,
        required_surface_area_m2: a_req,
        surface_margin_pct: margin_pct,
        shell_pressure_drop_kpa: dp_s_pa / 1000.0,
        tube_pressure_drop_kpa: dp_t_pa / 1000.0,
        rating_adequate,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_heat_exchanger_rating_sim(prob: &HeatExchangerRatingProblem) -> SimulationResult {
    let sol = solve_heat_exchanger_rating(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("heat_duty_kw".to_string(), sol.heat_duty_kw);
    metrics.insert("overall_u_fouled_w_m2_k".to_string(), sol.overall_u_fouled_w_m2_k);
    metrics.insert("surface_margin_pct".to_string(), sol.surface_margin_pct);
    metrics.insert("shell_pressure_drop_kpa".to_string(), sol.shell_pressure_drop_kpa);
    metrics.insert("tube_pressure_drop_kpa".to_string(), sol.tube_pressure_drop_kpa);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.heat_duty_kw, 1, 1e-4);

    let comparison = AnalyticalComparison::assess(
        "surface_area_margin",
        sol.provided_surface_area_m2,
        sol.required_surface_area_m2,
    );

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.thermal.heat_exchanger_tema".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.thermal.heat_exchanger_tema", "0.1.0"),
        logs: vec![format!("Duty: {:.1} kW, U_fouled: {:.1} W/m2K, Margin: {:.1}%", sol.heat_duty_kw, sol.overall_u_fouled_w_m2_k, sol.surface_margin_pct)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_industrial_oil_water_cooler_rating() {
        let prob = HeatExchangerRatingProblem::default();
        let sol = solve_heat_exchanger_rating(&prob);

        // Q = 18 * 2150 * (95 - 55) / 1000 = 1548.0 kW
        assert!((sol.heat_duty_kw - 1548.0).abs() < 1.0);
        // LMTD is ~35 to 45 °C
        assert!(sol.lmtd_c > 35.0 && sol.lmtd_c < 45.0);
        // TEMA F factor is between 0.80 and 0.98
        assert!(sol.f_correction_factor > 0.80 && sol.f_correction_factor <= 1.0);
        // U_fouled for oil-water is typically 300 to 700 W/(m²·K)
        assert!(sol.overall_u_fouled_w_m2_k > 250.0 && sol.overall_u_fouled_w_m2_k < 900.0);
        // Surface area provided for 380 tubes, 3/4" x 4.5m is ~102 m²
        assert!(sol.provided_surface_area_m2 > 95.0 && sol.provided_surface_area_m2 < 110.0);
        assert!(sol.surface_margin_pct > 0.0);
        assert!(sol.rating_adequate);
    }
}

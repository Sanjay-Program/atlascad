//! Power Transformer Electromagnetics & Thermal Rating Module.
//!
//! Evaluates core and copper losses, percentage impedance and voltage regulation,
//! electromechanical efficiency, IEC 60076-2 top-oil and hot-spot winding temperatures,
//! and insulation relative thermal aging in accordance with IEC 60076 and IEEE C57.12.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Power transformer cooling class per IEC 60076.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TransformerCoolingClass {
    /// Oil Natural Air Natural (ONAN).
    ONAN,
    /// Oil Natural Air Forced (ONAF).
    ONAF,
    /// Oil Forced Air Forced (OFAF).
    OFAF,
}

/// Power transformer problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerTransformerProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Rated three-phase apparent power in Megavolt-Amperes (MVA).
    pub rated_apparent_power_mva: f64,
    /// Primary line-to-line voltage in kilovolts (kV).
    pub primary_voltage_kv: f64,
    /// Secondary line-to-line voltage in kilovolts (kV).
    pub secondary_voltage_kv: f64,
    /// Percentage short-circuit impedance %Z at rated MVA.
    pub percent_impedance_z: f64,
    /// No-load core loss in kilowatts (kW) (iron losses P0).
    pub no_load_loss_kw: f64,
    /// Full-load copper load loss in kilowatts (kW) (Pk at 75°C).
    pub load_loss_kw: f64,
    /// Operating load fraction beta = S_load / S_rated (0.0 to 1.5).
    pub operating_load_factor: f64,
    /// Operating load power factor cos(phi) (typically 0.80 to 1.0).
    pub operating_power_factor: f64,
    /// Ambient air temperature in degrees Celsius (°C).
    pub ambient_temperature_c: f64,
    /// Rated top-oil temperature rise over ambient in degrees Celsius (°C) (e.g. 55°C).
    pub rated_top_oil_rise_c: f64,
    /// Rated average winding temperature rise in degrees Celsius (°C) (e.g. 65°C).
    pub rated_winding_rise_c: f64,
    /// Hot-spot winding factor H (typically 1.1 to 1.3 per IEC 60076-2).
    pub hot_spot_factor_h: f64,
    /// Transformer cooling method.
    #[serde(default = "default_cooling_class")]
    pub cooling_class: TransformerCoolingClass,
}

fn default_problem_id() -> String {
    "power_transformer_problem".to_string()
}

fn default_cooling_class() -> TransformerCoolingClass {
    TransformerCoolingClass::ONAN
}

impl Default for PowerTransformerProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            rated_apparent_power_mva: 40.0,
            primary_voltage_kv: 110.0,
            secondary_voltage_kv: 33.0,
            percent_impedance_z: 10.5,
            no_load_loss_kw: 22.0,
            load_loss_kw: 165.0,
            operating_load_factor: 1.0,
            operating_power_factor: 0.85,
            ambient_temperature_c: 30.0,
            rated_top_oil_rise_c: 55.0,
            rated_winding_rise_c: 65.0,
            hot_spot_factor_h: 1.15,
            cooling_class: default_cooling_class(),
        }
    }
}

/// Power transformer analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerTransformerResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Primary full-load phase current in Amperes (A).
    pub primary_rated_current_a: f64,
    /// Secondary full-load phase current in Amperes (A).
    pub secondary_rated_current_a: f64,
    /// Percent resistance component %R.
    pub percent_resistance_r: f64,
    /// Percent reactance component %X.
    pub percent_reactance_x: f64,
    /// Full-load percentage voltage regulation at operating power factor (%).
    pub percent_voltage_regulation: f64,
    /// Total operational electrical losses at evaluated load in kilowatts (kW).
    pub total_operating_loss_kw: f64,
    /// Power conversion efficiency percentage at evaluated operating load (%).
    pub operating_efficiency_pct: f64,
    /// Optimal load factor beta_max yielding maximum efficiency.
    pub optimal_load_factor_max_eff: f64,
    /// Maximum achievable efficiency percentage (%).
    pub maximum_efficiency_pct: f64,
    /// Operating top-oil temperature in degrees Celsius (°C).
    pub top_oil_temperature_c: f64,
    /// Operating hot-spot winding temperature in degrees Celsius (°C).
    pub hot_spot_temperature_c: f64,
    /// IEEE relative insulation aging factor (1.0 at reference 110°C hot spot).
    pub relative_insulation_aging_factor: f64,
    /// Whether transformer operates within thermal (<= 118°C) and regulation limits.
    pub is_safe: bool,
}

/// Solves power transformer electromechanical rating, losses, regulation, and thermal rise.
pub fn solve_power_transformer(
    problem: &PowerTransformerProblem,
) -> Result<PowerTransformerResult, String> {
    if problem.rated_apparent_power_mva <= 0.0 || problem.primary_voltage_kv <= 0.0 {
        return Err("Apparent power and primary voltage must be positive".to_string());
    }
    if problem.secondary_voltage_kv <= 0.0 || problem.percent_impedance_z <= 0.0 {
        return Err("Secondary voltage and impedance must be positive".to_string());
    }
    if problem.operating_load_factor <= 0.0 || problem.operating_power_factor <= 0.0 {
        return Err("Load factor and power factor must be positive".to_string());
    }

    let s_rated_kva = problem.rated_apparent_power_mva * 1000.0;

    // 1. Rated line currents for 3-phase: I = S / (sqrt(3) * V)
    let sqrt3 = 3.0_f64.sqrt();
    let i1_rated = s_rated_kva / (sqrt3 * problem.primary_voltage_kv);
    let i2_rated = s_rated_kva / (sqrt3 * problem.secondary_voltage_kv);

    // 2. Impedance components:
    // %R = (P_k_kW / S_rated_kVA) * 100
    let pct_r = (problem.load_loss_kw / s_rated_kva) * 100.0;
    if pct_r >= problem.percent_impedance_z {
        return Err("Load losses exceed total percent impedance".to_string());
    }
    let pct_x = (problem.percent_impedance_z.powi(2) - pct_r.powi(2)).sqrt();

    // 3. Voltage regulation:
    // %VR = beta * (%R * cos_phi + %X * sin_phi) + [beta * (%X * cos_phi - %R * sin_phi)]^2 / 200
    let beta = problem.operating_load_factor;
    let cos_phi = problem.operating_power_factor.clamp(0.1, 1.0);
    let sin_phi = (1.0 - cos_phi.powi(2)).sqrt();

    let term_lin = beta * (pct_r * cos_phi + pct_x * sin_phi);
    let term_quad = (beta * (pct_x * cos_phi - pct_r * sin_phi)).powi(2) / 200.0;
    let pct_vr = term_lin + term_quad;

    // 4. Operating losses and efficiency:
    // P_total(beta) = P_0 + beta^2 * P_k
    let p_0 = problem.no_load_loss_kw;
    let p_k = problem.load_loss_kw;
    let p_loss_kw = p_0 + beta.powi(2) * p_k;

    // Useful active power delivered: P_out = beta * S_rated * cos_phi
    let p_out_kw = beta * s_rated_kva * cos_phi;
    let eff_pct = (p_out_kw / (p_out_kw + p_loss_kw)) * 100.0;

    // Maximum efficiency occurs where P_0 = beta^2 * P_k => beta_opt = sqrt(P_0 / P_k)
    let beta_opt = (p_0 / p_k).sqrt();
    let p_out_opt_kw = beta_opt * s_rated_kva * cos_phi;
    let p_loss_opt_kw = 2.0 * p_0;
    let max_eff_pct = (p_out_opt_kw / (p_out_opt_kw + p_loss_opt_kw)) * 100.0;

    // 5. Thermal Rise per IEC 60076-2:
    // Top oil rise exponent x = 0.8 (ONAN), 0.9 (ONAF), 1.0 (OFAF)
    let (exp_oil, exp_wind) = match problem.cooling_class {
        TransformerCoolingClass::ONAN => (0.8, 1.6),
        TransformerCoolingClass::ONAF => (0.9, 1.6),
        TransformerCoolingClass::OFAF => (1.0, 2.0),
    };

    let loss_ratio_r = p_k / p_0;
    let total_loss_ratio = (1.0 + loss_ratio_r * beta.powi(2)) / (1.0 + loss_ratio_r);
    let delta_theta_top = problem.rated_top_oil_rise_c * total_loss_ratio.powf(exp_oil);
    let top_oil_temp_c = problem.ambient_temperature_c + delta_theta_top;

    // Winding gradient g_r = delta_theta_w - delta_theta_top
    let g_r = (problem.rated_winding_rise_c - problem.rated_top_oil_rise_c).max(5.0);
    let winding_gradient = g_r * beta.powf(exp_wind * 0.5);

    // Hot-spot temperature: theta_hs = theta_amb + delta_theta_top + H * g_r * beta^y
    let hot_spot_temp_c = top_oil_temp_c + problem.hot_spot_factor_h * winding_gradient;

    // IEEE / IEC relative aging factor: V = 2^((theta_hs - 110) / 6.0)
    let relative_aging = 2.0_f64.powf((hot_spot_temp_c - 110.0) / 6.0);

    // Safety checks: Hot spot temperature <= 118.0 °C (IEC continuous limit) and %VR <= 7.0 %
    let is_safe = hot_spot_temp_c <= 118.0 && pct_vr <= 7.0;

    Ok(PowerTransformerResult {
        problem_id: problem.problem_id.clone(),
        primary_rated_current_a: i1_rated,
        secondary_rated_current_a: i2_rated,
        percent_resistance_r: pct_r,
        percent_reactance_x: pct_x,
        percent_voltage_regulation: pct_vr,
        total_operating_loss_kw: p_loss_kw,
        operating_efficiency_pct: eff_pct,
        optimal_load_factor_max_eff: beta_opt,
        maximum_efficiency_pct: max_eff_pct,
        top_oil_temperature_c: top_oil_temp_c,
        hot_spot_temperature_c: hot_spot_temp_c,
        relative_insulation_aging_factor: relative_aging,
        is_safe,
    })
}

/// Generic simulation adapter for power transformer.
pub fn solve_power_transformer_sim(problem: &PowerTransformerProblem) -> SimulationResult {
    match solve_power_transformer(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "percent_voltage_regulation".to_string(),
                result.percent_voltage_regulation,
            );
            metrics.insert(
                "operating_efficiency_pct".to_string(),
                result.operating_efficiency_pct,
            );
            metrics.insert(
                "total_operating_loss_kw".to_string(),
                result.total_operating_loss_kw,
            );
            metrics.insert(
                "top_oil_temperature_c".to_string(),
                result.top_oil_temperature_c,
            );
            metrics.insert(
                "hot_spot_temperature_c".to_string(),
                result.hot_spot_temperature_c,
            );
            metrics.insert(
                "relative_insulation_aging_factor".to_string(),
                result.relative_insulation_aging_factor,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.hot_spot_temperature_c, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "operating_efficiency_pct",
                result.operating_efficiency_pct,
                99.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.transformer_iec60076".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.electrical.transformer_iec60076",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Transformer IEC 60076: η = {:.2}%, %VR = {:.2}%, P_loss = {:.1} kW, T_top = {:.1} °C, T_hs = {:.1} °C (Aging = {:.2}x, Safe: {})",
                    result.operating_efficiency_pct,
                    result.percent_voltage_regulation,
                    result.total_operating_loss_kw,
                    result.top_oil_temperature_c,
                    result.hot_spot_temperature_c,
                    result.relative_insulation_aging_factor,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.transformer_iec60076".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.electrical.transformer_iec60076",
                    "0.1.0",
                ),
                logs: vec![format!("Transformer simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_power_transformer_nominal() {
        let prob = PowerTransformerProblem::default();
        let res = solve_power_transformer(&prob).expect("solver failed");

        assert!(res.primary_rated_current_a > 200.0 && res.primary_rated_current_a < 220.0);
        assert!(res.percent_resistance_r > 0.35 && res.percent_resistance_r < 0.45);
        assert!(res.percent_reactance_x > 10.0 && res.percent_reactance_x < 11.0);
        assert!(res.percent_voltage_regulation > 5.0 && res.percent_voltage_regulation < 7.0);
        assert!(res.operating_efficiency_pct > 99.0 && res.operating_efficiency_pct < 99.8);
        assert!(res.optimal_load_factor_max_eff > 0.30 && res.optimal_load_factor_max_eff < 0.45);
        assert!(res.top_oil_temperature_c > 75.0 && res.top_oil_temperature_c < 90.0);
        assert!(res.hot_spot_temperature_c > 90.0 && res.hot_spot_temperature_c < 115.0);
        assert!(res.is_safe);
    }
}

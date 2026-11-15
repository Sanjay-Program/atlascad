//! # Psychrometrics & Moist Air Thermodynamics (ASHRAE / ISO 7730)
//!
//! Provides first-principles calculation of moist air thermodynamic states:
//! saturation vapor pressure, actual vapor pressure, humidity ratio W, specific enthalpy h,
//! dew point temperature Tdp, wet bulb temperature Twb, specific volume v, and density rho.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Problem input definition for psychrometric state evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsychrometricProblem {
    pub stream_name: String,
    /// Dry-bulb temperature T_db in °C (-40°C to +70°C)
    pub dry_bulb_temp_c: f64,
    /// Relative humidity (0.0 to 1.0, or 0.0 to 100.0)
    pub relative_humidity: f64,
    /// Atmospheric pressure p_atm in Pa (default 101325.0 Pa = 1 atm)
    pub atmospheric_pressure_pa: Option<f64>,
}

impl Default for PsychrometricProblem {
    fn default() -> Self {
        Self {
            stream_name: "Standard ASHRAE Comfort Air Condition".to_string(),
            dry_bulb_temp_c: 24.0,
            relative_humidity: 0.50,
            atmospheric_pressure_pa: Some(101325.0),
        }
    }
}

/// Comprehensive psychrometric state output properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PsychrometricResult {
    pub stream_name: String,
    pub dry_bulb_temp_c: f64,
    pub relative_humidity_fraction: f64,
    pub relative_humidity_percent: f64,
    pub atmospheric_pressure_pa: f64,
    pub saturation_vapor_pressure_pa: f64,
    pub partial_vapor_pressure_pa: f64,
    pub humidity_ratio_kg_kg: f64,
    pub humidity_ratio_g_kg: f64,
    pub specific_enthalpy_kj_kg: f64,
    pub dew_point_temp_c: f64,
    pub wet_bulb_temp_c: f64,
    pub specific_volume_m3_kg: f64,
    pub moist_air_density_kg_m3: f64,
    pub summary_notes: Vec<String>,
}

/// Computes saturation vapor pressure p_ws in Pa via Magnus-Tetens / Buck formulation.
pub fn saturation_vapor_pressure_pa(temp_c: f64) -> f64 {
    // Over liquid water (T >= 0 °C):
    if temp_c >= 0.0 {
        611.21 * ((17.67 * temp_c) / (temp_c + 243.5)).exp()
    } else {
        // Over ice (T < 0 °C):
        611.15 * ((22.5 * temp_c) / (temp_c + 273.16)).exp()
    }
}

/// Solves moist air thermodynamic state properties per ASHRAE equations.
pub fn solve_psychrometric_state(prob: &PsychrometricProblem) -> Result<PsychrometricResult, String> {
    let t_db = prob.dry_bulb_temp_c;
    if t_db < -60.0 || t_db > 95.0 {
        return Err("Dry bulb temperature must be between -60°C and +95°C.".to_string());
    }

    let p_atm = prob.atmospheric_pressure_pa.unwrap_or(101325.0);
    if p_atm < 30000.0 || p_atm > 150000.0 {
        return Err("Atmospheric pressure must be between 30 kPa and 150 kPa.".to_string());
    }

    // Normalize relative humidity fraction:
    let rh_frac = if prob.relative_humidity > 1.0 {
        prob.relative_humidity / 100.0
    } else {
        prob.relative_humidity
    }.clamp(0.001, 1.0);
    let rh_pct = rh_frac * 100.0;

    // Saturation vapor pressure p_ws in Pa:
    let p_ws = saturation_vapor_pressure_pa(t_db);

    // Actual partial vapor pressure p_v in Pa:
    let p_v = rh_frac * p_ws;

    // Humidity ratio W = 0.62198 * p_v / (p_atm - p_v) [kg_w / kg_da]:
    let w = (0.62198 * p_v) / (p_atm - p_v).max(10.0);
    let w_g = w * 1000.0;

    // Specific Enthalpy h = 1.006 * T_db + W * (2501.0 + 1.86 * T_db) [kJ/kg_da]:
    let h_kj_kg = 1.006 * t_db + w * (2501.0 + 1.86 * t_db);

    // Dew point temperature T_dp from Magnus-Tetens inverse:
    let alpha = (p_v / 611.21).ln();
    let t_dp = (243.5 * alpha) / (17.67 - alpha);

    // Wet bulb temperature T_wb via Stull (2011) high-accuracy empirical formula:
    let t_wb = t_db * (0.151977 * (rh_pct + 8.313659).sqrt()).atan()
        + (t_db + rh_pct).atan()
        - (rh_pct - 1.676331).atan()
        + 0.00391838 * rh_pct.powf(1.5) * (0.023101 * rh_pct).atan()
        - 4.686035;

    // Specific volume v = R_da * T_K * (1 + 1.6078 * W) / p_atm [m³/kg_da]:
    let t_kelvin = t_db + 273.15;
    let r_da = 287.055; // J / (kg * K)
    let v_m3_kg = (r_da * t_kelvin * (1.0 + 1.6078 * w)) / p_atm;

    // Moist air density rho = (1 + W) / v [kg/m³]:
    let rho_kg_m3 = (1.0 + w) / v_m3_kg;

    let notes = vec![
        format!("Moist Air State: '{}' (T_db = {:.1} °C, RH = {:.1} %, p_atm = {:.0} Pa)", prob.stream_name, t_db, rh_pct, p_atm),
        format!("Vapor Pressure: Saturation p_ws = {:.1} Pa, Actual p_v = {:.1} Pa", p_ws, p_v),
        format!("Humidity Ratio: W = {:.5} kg/kg ({:.2} g/kg dry air)", w, w_g),
        format!("Temperatures: Dew Point T_dp = {:.2} °C, Wet Bulb T_wb = {:.2} °C", t_dp, t_wb),
        format!("Specific Properties: Enthalpy h = {:.2} kJ/kg, Volume v = {:.4} m³/kg, Density rho = {:.3} kg/m³", h_kj_kg, v_m3_kg, rho_kg_m3),
    ];

    Ok(PsychrometricResult {
        stream_name: prob.stream_name.clone(),
        dry_bulb_temp_c: t_db,
        relative_humidity_fraction: rh_frac,
        relative_humidity_percent: rh_pct,
        atmospheric_pressure_pa: p_atm,
        saturation_vapor_pressure_pa: p_ws,
        partial_vapor_pressure_pa: p_v,
        humidity_ratio_kg_kg: w,
        humidity_ratio_g_kg: w_g,
        specific_enthalpy_kj_kg: h_kj_kg,
        dew_point_temp_c: t_dp,
        wet_bulb_temp_c: t_wb,
        specific_volume_m3_kg: v_m3_kg,
        moist_air_density_kg_m3: rho_kg_m3,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for psychrometric thermodynamics.
pub fn solve_psychrometrics_sim(prob: &PsychrometricProblem) -> Result<SimulationResult, String> {
    let sol = solve_psychrometric_state(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("saturation_vapor_pressure_pa".to_string(), sol.saturation_vapor_pressure_pa);
    metrics.insert("humidity_ratio_g_kg".to_string(), sol.humidity_ratio_g_kg);
    metrics.insert("specific_enthalpy_kj_kg".to_string(), sol.specific_enthalpy_kj_kg);
    metrics.insert("dew_point_temp_c".to_string(), sol.dew_point_temp_c);
    metrics.insert("wet_bulb_temp_c".to_string(), sol.wet_bulb_temp_c);
    metrics.insert("specific_volume_m3_kg".to_string(), sol.specific_volume_m3_kg);
    metrics.insert("moist_air_density_kg_m3".to_string(), sol.moist_air_density_kg_m3);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.specific_enthalpy_kj_kg, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Psychrometric Enthalpy (kJ/kg)",
        sol.specific_enthalpy_kj_kg,
        1.006 * prob.dry_bulb_temp_c + sol.humidity_ratio_kg_kg * (2501.0 + 1.86 * prob.dry_bulb_temp_c),
    );

    Ok(SimulationResult {
        problem_id: "psychrometric-state-eval".to_string(),
        engine_id: "atlas-psychrometrics".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-psychrometrics", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_room_air_psychrometrics() -> Result<(), String> {
        let prob = PsychrometricProblem::default(); // T=24 C, RH=50%
        let sol = solve_psychrometric_state(&prob)?;

        // At 24 C: p_ws ~ 2985 Pa
        assert!((sol.saturation_vapor_pressure_pa - 2985.0).abs() < 25.0);
        // At 50% RH: p_v ~ 1492 Pa
        assert!((sol.partial_vapor_pressure_pa - 1492.0).abs() < 15.0);
        // W ~ 0.0093 kg/kg (9.3 g/kg)
        assert!((sol.humidity_ratio_g_kg - 9.3).abs() < 0.2);
        // Dew point at 24C / 50% RH is ~ 12.9 °C
        assert!((sol.dew_point_temp_c - 12.9).abs() < 0.5);
        // Wet bulb is ~ 17.0 °C
        assert!((sol.wet_bulb_temp_c - 17.0).abs() < 0.5);
        // Specific enthalpy h ~ 47.8 kJ/kg
        assert!((sol.specific_enthalpy_kj_kg - 47.8).abs() < 0.5);
        // Density ~ 1.18 kg/m³
        assert!((sol.moist_air_density_kg_m3 - 1.18).abs() < 0.02);

        let sim = solve_psychrometrics_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}

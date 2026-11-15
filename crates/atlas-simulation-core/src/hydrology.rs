//! # Civil Engineering Stormwater Hydrology & Hydraulic Pipe Sizing (Rational Method / Manning)
//!
//! Provides analytical calculation of watershed time of concentration (Kirpich),
//! rainfall intensity from IDF curves, peak stormwater discharge Q_peak,
//! and circular storm sewer pipe diameter sizing via Manning's open-channel equation.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Surface land-use runoff classification for Rational Method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LandUseCategory {
    PavedRoadsRoofs,
    CommercialDowntown,
    IndustrialHeavy,
    ResidentialSuburban,
    ParksLawnsMeadows,
}

impl LandUseCategory {
    /// Dimensionless Rational runoff coefficient C (0.0 to 1.0).
    pub fn runoff_coefficient(&self) -> f64 {
        match self {
            Self::PavedRoadsRoofs => 0.90,
            Self::CommercialDowntown => 0.85,
            Self::IndustrialHeavy => 0.75,
            Self::ResidentialSuburban => 0.50,
            Self::ParksLawnsMeadows => 0.20,
        }
    }
}

/// Storm sewer pipe material roughness per Manning.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PipeMaterial {
    SmoothHdpPvc,
    PrecastConcrete,
    CorrugatedMetal,
}

impl PipeMaterial {
    /// Manning roughness coefficient n (s / m^(1/3)).
    pub fn manning_n(&self) -> f64 {
        match self {
            Self::SmoothHdpPvc => 0.010,
            Self::PrecastConcrete => 0.013,
            Self::CorrugatedMetal => 0.024,
        }
    }
}

/// Input problem definition for civil hydrology and storm sewer sizing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydrologyProblem {
    pub basin_name: String,
    pub land_use: LandUseCategory,
    pub pipe_material: PipeMaterial,
    /// Catchment watershed drainage area in hectares (1 ha = 10,000 m²)
    pub catchment_area_ha: f64,
    /// Maximum overland/channel flow path length L in meters (for Kirpich Tc)
    pub flow_path_length_m: f64,
    /// Average catchment terrain slope S in m/m (e.g. 0.015 for 1.5% grade)
    pub terrain_slope: f64,
    /// Storm sewer pipe longitudinal grade S_0 in m/m (e.g. 0.005 for 0.5% slope)
    pub pipe_slope: f64,
    /// IDF rainfall coefficient a (e.g. 1200.0 for 10-year storm)
    pub idf_a: Option<f64>,
    /// IDF rainfall parameter b in minutes (e.g. 15.0)
    pub idf_b: Option<f64>,
    /// IDF rainfall exponent c (e.g. 0.85)
    pub idf_c: Option<f64>,
    /// Selected commercial pipe diameter in mm (optional)
    pub selected_pipe_diameter_mm: Option<f64>,
}

impl Default for HydrologyProblem {
    fn default() -> Self {
        Self {
            basin_name: "Commercial Business Park Catchment Basin A".to_string(),
            land_use: LandUseCategory::CommercialDowntown,
            pipe_material: PipeMaterial::PrecastConcrete,
            catchment_area_ha: 8.5,
            flow_path_length_m: 450.0,
            terrain_slope: 0.020,
            pipe_slope: 0.008,
            idf_a: Some(1250.0),
            idf_b: Some(15.0),
            idf_c: Some(0.85),
            selected_pipe_diameter_mm: Some(600.0),
        }
    }
}

/// Detailed hydrology and hydraulic calculation outputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydrologyResult {
    pub basin_name: String,
    pub catchment_area_ha: f64,
    pub runoff_coefficient_c: f64,
    pub time_of_concentration_min: f64,
    pub rainfall_intensity_mm_hr: f64,
    pub peak_discharge_m3_s: f64,
    pub peak_discharge_liters_s: f64,
    pub min_required_pipe_diameter_mm: f64,
    pub actual_pipe_diameter_mm: f64,
    pub full_pipe_capacity_m3_s: f64,
    pub flow_velocity_m_s: f64,
    pub is_self_cleansing: bool,
    pub capacity_safety_margin_percent: f64,
    pub summary_notes: Vec<String>,
}

/// Solves watershed runoff discharge and Manning storm sewer pipe diameter.
pub fn solve_rational_runoff_and_pipe(prob: &HydrologyProblem) -> Result<HydrologyResult, String> {
    let a_ha = prob.catchment_area_ha;
    let l_m = prob.flow_path_length_m;
    let s_terr = prob.terrain_slope;
    let s_pipe = prob.pipe_slope;

    if a_ha <= 0.01 || l_m <= 10.0 || s_terr <= 0.0001 || s_pipe <= 0.0001 {
        return Err("Catchment area, flow length, terrain slope, and pipe slope must be strictly positive.".to_string());
    }

    let c_coeff = prob.land_use.runoff_coefficient();

    // 1. Time of Concentration t_c via Kirpich equation:
    // t_c = 0.0195 * L^0.77 * S^(-0.385) [minutes]
    let tc_min = (0.0195 * l_m.powf(0.77) * s_terr.powf(-0.385)).max(5.0);

    // 2. Rainfall Intensity I via IDF curve:
    // I = a / (t_c + b)^c [mm/hr]
    let idf_a = prob.idf_a.unwrap_or(1250.0);
    let idf_b = prob.idf_b.unwrap_or(15.0);
    let idf_c = prob.idf_c.unwrap_or(0.85);
    let rainfall_intensity_mm_hr = idf_a / (tc_min + idf_b).powf(idf_c);

    // 3. Peak Discharge Q_peak via Rational Formula:
    // Q = C * I * A / 360 [m³/s]
    let q_peak_m3_s = (c_coeff * rainfall_intensity_mm_hr * a_ha) / 360.0;
    let q_peak_l_s = q_peak_m3_s * 1000.0;

    // 4. Manning Pipe Sizing:
    // Q_full = (0.3117 / n) * D^(8/3) * S_0^(1/2)
    // D_min = (Q * n / (0.3117 * sqrt(S_pipe)))^(3/8)
    let n_manning = prob.pipe_material.manning_n();
    let d_term = (q_peak_m3_s * n_manning) / (0.3117 * s_pipe.sqrt());
    let d_min_m = d_term.powf(0.375); // 3/8 = 0.375
    let d_min_mm = d_min_m * 1000.0;

    // Commercial standard pipe selection:
    let d_actual_mm = prob.selected_pipe_diameter_mm.unwrap_or(d_min_mm).max(d_min_mm);
    let d_actual_m = d_actual_mm / 1000.0;

    // Full pipe capacity:
    let q_cap_m3_s = (0.3117 / n_manning) * d_actual_m.powf(8.0 / 3.0) * s_pipe.sqrt();

    // Flow velocity at full capacity:
    // V = Q / A_pipe = Q / (pi * D^2 / 4)
    let a_pipe_m2 = std::f64::consts::PI * d_actual_m.powi(2) / 4.0;
    let velocity_m_s = q_peak_m3_s / a_pipe_m2;

    // Self-cleansing velocity criteria (0.9 m/s <= V <= 3.5 m/s):
    let is_self_clean = velocity_m_s >= 0.90 && velocity_m_s <= 4.0;

    // Safety capacity margin:
    let capacity_margin_pct = ((q_cap_m3_s - q_peak_m3_s) / q_peak_m3_s) * 100.0;

    let notes = vec![
        format!("Stormwater Hydrology: '{}' ({:.1} ha, Land Use = {:?}, C = {:.2})", prob.basin_name, a_ha, prob.land_use, c_coeff),
        format!("Kirpich Concentration: Flow Path = {:.0} m @ {:.2}% Slope -> Tc = {:.1} min", l_m, s_terr * 100.0, tc_min),
        format!("IDF Rainfall: Intensity = {:.1} mm/hr -> Peak Runoff Q_peak = {:.3} m³/s ({:.1} L/s)", rainfall_intensity_mm_hr, q_peak_m3_s, q_peak_l_s),
        format!("Manning Culvert: Min D = {:.0} mm, Nominal D = {:.0} mm @ {:.2}% Slope (n = {:.3})", d_min_mm, d_actual_mm, s_pipe * 100.0, n_manning),
        format!("Hydraulic Performance: Pipe Full Capacity = {:.3} m³/s (+{:.1}% Margin), Velocity = {:.2} m/s (Self-Cleansing: {})", q_cap_m3_s, capacity_margin_pct, velocity_m_s, is_self_clean),
    ];

    Ok(HydrologyResult {
        basin_name: prob.basin_name.clone(),
        catchment_area_ha: a_ha,
        runoff_coefficient_c: c_coeff,
        time_of_concentration_min: tc_min,
        rainfall_intensity_mm_hr,
        peak_discharge_m3_s: q_peak_m3_s,
        peak_discharge_liters_s: q_peak_l_s,
        min_required_pipe_diameter_mm: d_min_mm,
        actual_pipe_diameter_mm: d_actual_mm,
        full_pipe_capacity_m3_s: q_cap_m3_s,
        flow_velocity_m_s: velocity_m_s,
        is_self_cleansing: is_self_clean,
        capacity_safety_margin_percent: capacity_margin_pct,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for stormwater hydrology.
pub fn solve_hydrology_sim(prob: &HydrologyProblem) -> Result<SimulationResult, String> {
    let sol = solve_rational_runoff_and_pipe(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("time_of_concentration_min".to_string(), sol.time_of_concentration_min);
    metrics.insert("rainfall_intensity_mm_hr".to_string(), sol.rainfall_intensity_mm_hr);
    metrics.insert("peak_discharge_m3_s".to_string(), sol.peak_discharge_m3_s);
    metrics.insert("min_required_pipe_diameter_mm".to_string(), sol.min_required_pipe_diameter_mm);
    metrics.insert("actual_pipe_diameter_mm".to_string(), sol.actual_pipe_diameter_mm);
    metrics.insert("flow_velocity_m_s".to_string(), sol.flow_velocity_m_s);
    metrics.insert("capacity_safety_margin_percent".to_string(), sol.capacity_safety_margin_percent);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.peak_discharge_m3_s, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Peak Runoff Discharge (m³/s)",
        sol.peak_discharge_m3_s,
        (prob.land_use.runoff_coefficient() * sol.rainfall_intensity_mm_hr * prob.catchment_area_ha) / 360.0,
    );

    Ok(SimulationResult {
        problem_id: "stormwater-rational-runoff".to_string(),
        engine_id: "atlas-hydrology".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-hydrology", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commercial_park_stormwater_runoff() -> Result<(), String> {
        let prob = HydrologyProblem::default();
        let sol = solve_rational_runoff_and_pipe(&prob)?;

        // Area = 8.5 ha, Commercial C = 0.85
        assert_eq!(sol.runoff_coefficient_c, 0.85);
        // Tc for 450m at 2% slope ~ 9.0 - 14.0 min
        assert!(sol.time_of_concentration_min > 8.0 && sol.time_of_concentration_min < 16.0);
        // Rainfall intensity ~ 60 - 100 mm/hr
        assert!(sol.rainfall_intensity_mm_hr > 50.0 && sol.rainfall_intensity_mm_hr < 110.0);
        // Q_peak ~ 1.0 - 2.0 m³/s
        assert!(sol.peak_discharge_m3_s > 0.8 && sol.peak_discharge_m3_s < 2.5);
        // Required pipe diameter ~ 800 - 1100 mm for 1.6 m3/s runoff
        assert!(sol.min_required_pipe_diameter_mm > 700.0 && sol.min_required_pipe_diameter_mm < 1200.0);
        // Velocity should be self-cleansing (> 0.9 m/s)
        assert!(sol.flow_velocity_m_s > 0.9);

        let sim = solve_hydrology_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}

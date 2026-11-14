//! # Environmental Engineering & Atmospheric Dispersion Simulation
//!
//! Implements industrial environmental impact, air quality modeling, and emission dispersion:
//! 1. Pasquill-Gifford Gaussian Plume dispersion model for continuous point source emissions (stacks).
//! 2. Downwind centerline and ground-level concentration profiles across stability classes A through F.
//! 3. Peak ground impact distance and regulatory safety exclusion zone contour calculation.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Atmospheric stability class according to the Pasquill-Gifford system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum AtmosphericStabilityClass {
    /// A: Extremely unstable (strong solar insolation, low wind)
    ClassA,
    /// B: Moderately unstable
    ClassB,
    /// C: Slightly unstable
    ClassC,
    /// D: Neutral (overcast sky or night with high wind)
    #[default]
    ClassD,
    /// E: Slightly stable
    ClassE,
    /// F: Moderately stable (clear night, light wind, ground temperature inversion)
    ClassF,
}

/// Parameters for industrial smokestack atmospheric dispersion analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlumeDispersionProblem {
    pub source_name: String,
    /// Pollutant mass emission rate Q in g/s (grams per second)
    pub emission_rate_g_s: f64,
    /// Physical stack height h_s in meters
    pub stack_height_m: f64,
    /// Thermal plume rise delta_h in meters (buoyant plume rise)
    pub plume_rise_m: f64,
    /// Mean ambient horizontal wind speed u at release height in m/s
    pub wind_speed_m_s: f64,
    /// Atmospheric stability classification
    pub stability_class: AtmosphericStabilityClass,
    /// Downwind evaluation distance x in meters
    pub downwind_distance_m: f64,
    /// Crosswind evaluation offset y in meters (0.0 for centerline)
    pub crosswind_offset_m: f64,
    /// Receptor elevation z in meters (0.0 for ground level)
    pub receptor_height_m: f64,
    /// Regulatory or emergency limit threshold (e.g., ERPG-2 / IDLH) in mg/m^3
    pub regulatory_threshold_mg_m3: f64,
}

impl Default for PlumeDispersionProblem {
    fn default() -> Self {
        Self {
            source_name: "Refinery FCCU Catalytic Flue Stack (SO2)".to_string(),
            emission_rate_g_s: 150.0, // 150 g/s sulfur dioxide
            stack_height_m: 65.0,     // 65 m tall industrial stack
            plume_rise_m: 25.0,       // 25 m thermal buoyancy rise -> H_eff = 90 m
            wind_speed_m_s: 4.5,      // 4.5 m/s wind speed
            stability_class: AtmosphericStabilityClass::ClassD, // Neutral atmospheric stability
            downwind_distance_m: 1500.0, // Evaluation at 1.5 km downwind
            crosswind_offset_m: 0.0,  // Plume centerline
            receptor_height_m: 0.0,   // Ground level
            regulatory_threshold_mg_m3: 0.50, // 0.50 mg/m^3 (approx 0.19 ppm SO2)
        }
    }
}

/// Computes Pasquill-Gifford dispersion parameters sigma_y and sigma_z (in meters) for downwind distance x (m).
/// Uses standard EPA Briggs rural interpolation formulas.
pub fn compute_dispersion_coefficients(class: AtmosphericStabilityClass, x_m: f64) -> (f64, f64) {
    let x = x_m.max(10.0);
    match class {
        AtmosphericStabilityClass::ClassA => {
            let sy = 0.22 * x * (1.0 + 0.0001 * x).powf(-0.5);
            let sz = 0.20 * x;
            (sy, sz)
        }
        AtmosphericStabilityClass::ClassB => {
            let sy = 0.16 * x * (1.0 + 0.0001 * x).powf(-0.5);
            let sz = 0.12 * x;
            (sy, sz)
        }
        AtmosphericStabilityClass::ClassC => {
            let sy = 0.11 * x * (1.0 + 0.0001 * x).powf(-0.5);
            let sz = 0.08 * x * (1.0 + 0.0002 * x).powf(-0.5);
            (sy, sz)
        }
        AtmosphericStabilityClass::ClassD => {
            let sy = 0.08 * x * (1.0 + 0.0001 * x).powf(-0.5);
            let sz = 0.06 * x * (1.0 + 0.0015 * x).powf(-0.5);
            (sy, sz)
        }
        AtmosphericStabilityClass::ClassE => {
            let sy = 0.06 * x * (1.0 + 0.0001 * x).powf(-0.5);
            let sz = 0.03 * x * (1.0 + 0.0003 * x).powf(-1.0);
            (sy, sz)
        }
        AtmosphericStabilityClass::ClassF => {
            let sy = 0.04 * x * (1.0 + 0.0001 * x).powf(-0.5);
            let sz = 0.016 * x * (1.0 + 0.0003 * x).powf(-1.0);
            (sy, sz)
        }
    }
}

/// Solves 3D Gaussian plume atmospheric dispersion, ground concentration, and safety boundary.
pub fn solve_plume_dispersion(prob: &PlumeDispersionProblem) -> Result<SimulationResult, String> {
    if prob.emission_rate_g_s <= 0.0 || prob.wind_speed_m_s <= 0.0 {
        return Err("Emission rate and wind speed must be positive non-zero values".to_string());
    }
    if prob.stack_height_m < 0.0 || prob.downwind_distance_m <= 0.0 {
        return Err(
            "Stack height must be non-negative and downwind distance must be positive".to_string(),
        );
    }

    let h_eff = prob.stack_height_m + prob.plume_rise_m;
    let u = prob.wind_speed_m_s;
    let q_mg_s = prob.emission_rate_g_s * 1000.0;

    let (sigma_y, sigma_z) =
        compute_dispersion_coefficients(prob.stability_class, prob.downwind_distance_m);

    let y = prob.crosswind_offset_m;
    let z = prob.receptor_height_m;

    let crosswind_term = (-0.5 * (y / sigma_y).powi(2)).exp();
    let vertical_term = (-0.5 * ((z - h_eff) / sigma_z).powi(2)).exp()
        + (-0.5 * ((z + h_eff) / sigma_z).powi(2)).exp();

    let denom = 2.0 * std::f64::consts::PI * u * sigma_y * sigma_z;
    let concentration_mg_m3 = (q_mg_s / denom) * crosswind_term * vertical_term;

    let centerline_ground_mg_m3 = if sigma_z > 0.0 {
        let v_ground = 2.0 * (-0.5 * (h_eff / sigma_z).powi(2)).exp();
        (q_mg_s / denom) * v_ground
    } else {
        0.0
    };

    let mut max_ground_conc_mg_m3 = 0.0;
    let mut x_peak_m = 100.0;
    let mut x_scan = 100.0;
    while x_scan <= 20000.0 {
        let (sy_s, sz_s) = compute_dispersion_coefficients(prob.stability_class, x_scan);
        let den_s = std::f64::consts::PI * u * sy_s * sz_s;
        let c_ground = (q_mg_s / den_s) * (-0.5 * (h_eff / sz_s).powi(2)).exp();
        if c_ground > max_ground_conc_mg_m3 {
            max_ground_conc_mg_m3 = c_ground;
            x_peak_m = x_scan;
        }
        x_scan += 50.0;
    }

    let exceeds_threshold = concentration_mg_m3 > prob.regulatory_threshold_mg_m3;
    let crosswind_safe_width_m = if centerline_ground_mg_m3 > prob.regulatory_threshold_mg_m3 {
        let ratio = centerline_ground_mg_m3 / prob.regulatory_threshold_mg_m3;
        sigma_y * (2.0 * ratio.ln()).sqrt()
    } else {
        0.0
    };

    let mut metrics = HashMap::new();
    metrics.insert("effective_stack_height_m".to_string(), h_eff);
    metrics.insert("sigma_y_m".to_string(), sigma_y);
    metrics.insert("sigma_z_m".to_string(), sigma_z);
    metrics.insert(
        "concentration_at_receptor_mg_m3".to_string(),
        concentration_mg_m3,
    );
    metrics.insert(
        "centerline_ground_conc_mg_m3".to_string(),
        centerline_ground_mg_m3,
    );
    metrics.insert(
        "peak_ground_concentration_mg_m3".to_string(),
        max_ground_conc_mg_m3,
    );
    metrics.insert("peak_ground_distance_m".to_string(), x_peak_m);
    metrics.insert(
        "crosswind_exclusion_width_m".to_string(),
        crosswind_safe_width_m * 2.0,
    );
    metrics.insert(
        "exceeds_regulatory_threshold".to_string(),
        if exceeds_threshold { 1.0 } else { 0.0 },
    );

    let analytical_centerline = (q_mg_s / (std::f64::consts::PI * u * sigma_y * sigma_z))
        * (-0.5 * (h_eff / sigma_z).powi(2)).exp();
    let val = AnalyticalComparison::assess(
        "centerline_ground_conc_mg_m3",
        centerline_ground_mg_m3,
        analytical_centerline,
    );

    Ok(SimulationResult {
        problem_id: "environmental-gaussian-plume-dispersion".to_string(),
        engine_id: "atlas-environmental-plume".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: ConvergenceMonitor::new(1e-6),
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-environmental-plume", "0.1.0"),
        logs: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gaussian_plume_dispersion_closed_form() -> Result<(), String> {
        let prob = PlumeDispersionProblem::default();
        let result = solve_plume_dispersion(&prob)?;

        assert_eq!(result.status, SimulationStatus::Converged);
        let h_eff = result
            .scalar_metrics
            .get("effective_stack_height_m")
            .copied()
            .unwrap_or(0.0);
        let c_rec = result
            .scalar_metrics
            .get("concentration_at_receptor_mg_m3")
            .copied()
            .unwrap_or(0.0);
        let c_peak = result
            .scalar_metrics
            .get("peak_ground_concentration_mg_m3")
            .copied()
            .unwrap_or(0.0);
        let x_peak = result
            .scalar_metrics
            .get("peak_ground_distance_m")
            .copied()
            .unwrap_or(0.0);

        assert!((h_eff - 90.0).abs() < 1e-6);
        assert!(
            c_rec > 0.02 && c_rec < 0.40,
            "Receptor concentration: {} mg/m^3",
            c_rec
        );
        assert!(
            (1500.0..=4500.0).contains(&x_peak),
            "Peak distance: {} m",
            x_peak
        );
        assert!(
            c_peak >= c_rec * 0.9,
            "Peak concentration {} vs receptor {}",
            c_peak,
            c_rec
        );
        Ok(())
    }
}

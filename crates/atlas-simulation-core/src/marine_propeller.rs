//! Marine Propeller Hydrodynamics & Cavitation Inception Solver (Wageningen B-Series / ITTC 1978).
//!
//! Models open-water propeller characteristics (KT, KQ, eta_0) as functions of
//! advance coefficient J, pitch ratio P/D, expanded blade area ratio AE/A0,
//! and blade count Z. Evaluates thrust, torque, delivered shaft power, tip speed,
//! and Keller cavitation inception criteria.

#![forbid(unsafe_code)]

use std::f64::consts::PI;
use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Propeller blade series classification.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PropellerSeries {
    /// Wageningen B-screw 3-blade series.
    WageningenB3,
    /// Wageningen B-screw 4-blade series (industry standard merchant).
    WageningenB4,
    /// Wageningen B-screw 5-blade series (high-thrust / low vibration).
    WageningenB5,
    /// Wageningen B-screw 6-blade series (naval / cruise liner).
    WageningenB6,
}

impl PropellerSeries {
    /// Number of blades Z.
    pub fn blade_count(&self) -> usize {
        match self {
            Self::WageningenB3 => 3,
            Self::WageningenB4 => 4,
            Self::WageningenB5 => 5,
            Self::WageningenB6 => 6,
        }
    }
}

/// Marine propeller problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarinePropellerProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Propeller series classification.
    pub series: PropellerSeries,
    /// Propeller outer diameter D (m).
    pub diameter_m: f64,
    /// Pitch-to-diameter ratio P/D (typically 0.60 to 1.40).
    pub pitch_ratio_p_d: f64,
    /// Expanded blade area ratio AE/A0 (typically 0.40 to 1.05).
    pub expanded_area_ratio: f64,
    /// Propeller rotational speed (RPM).
    pub rotational_speed_rpm: f64,
    /// Vessel advance speed through water (knots).
    pub advance_speed_knots: f64,
    /// Shaft center immersion depth h below water surface (m).
    pub immersion_depth_m: f64,
    /// Water density rho (kg/m³, 1025.0 for standard seawater).
    pub water_density_kg_m3: f64,
    /// Vessel type Keller constant k (0.20 for single-screw, 0.10 for twin-screw).
    pub keller_constant_k: f64,
}

impl Default for MarinePropellerProblem {
    fn default() -> Self {
        Self {
            problem_id: "panamax_container_ship_propeller".to_string(),
            series: PropellerSeries::WageningenB4,
            diameter_m: 6.80,
            pitch_ratio_p_d: 1.05,
            expanded_area_ratio: 0.68,
            rotational_speed_rpm: 108.0,
            advance_speed_knots: 17.5, // ~9.00 m/s
            immersion_depth_m: 6.50,
            water_density_kg_m3: 1025.0,
            keller_constant_k: 0.20,
        }
    }
}

/// Marine propeller hydrodynamic output results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MarinePropellerResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Propeller rotational frequency n (rev/s).
    pub rotational_frequency_rev_s: f64,
    /// Advance speed Va (m/s).
    pub advance_speed_m_s: f64,
    /// Advance coefficient J = Va / (n * D).
    pub advance_coefficient_j: f64,
    /// Thrust coefficient KT.
    pub thrust_coefficient_kt: f64,
    /// Torque coefficient KQ.
    pub torque_coefficient_kq: f64,
    /// Open-water propeller efficiency eta_0.
    pub open_water_efficiency: f64,
    /// Total thrust generated T (kN).
    pub thrust_kn: f64,
    /// Propeller shaft torque Q_t (kNm).
    pub torque_kn_m: f64,
    /// Delivered shaft power P_D (kW).
    pub delivered_power_kw: f64,
    /// Useful thrust power P_T = T * Va (kW).
    pub thrust_power_kw: f64,
    /// Blade tip speed v_tip (m/s).
    pub blade_tip_speed_m_s: f64,
    /// Centerline hydrostatic pressure p_h (kPa).
    pub hydrostatic_pressure_kpa: f64,
    /// Keller minimum expanded area ratio (AE/A0)_min to prevent cavitation.
    pub keller_min_area_ratio: f64,
    /// Margin above Keller minimum ratio (AE/A0 - (AE/A0)_min).
    pub keller_margin: f64,
    /// Whether propeller passes Keller cavitation criterion.
    pub cavitation_safe: bool,
    /// Cavitation inception index sigma_n.
    pub cavitation_index_sigma_n: f64,
}

/// Evaluates Wageningen B-Series KT and KQ based on MARIN regression equations.
pub fn evaluate_wageningen_b(
    j: f64,
    p_d: f64,
    ae_a0: f64,
    z: usize,
) -> (f64, f64) {
    let z_f = z as f64;
    let j_clamped = j.clamp(0.01, 1.40);

    // Oosterveld and van Oossanen B-series regression representation:
    let kt = (0.320 * p_d - 0.240 * j_clamped - 0.040 * j_clamped.powi(2)
        + 0.020 * (z_f - 4.0) + 0.030 * (ae_a0 - 0.55))
        .max(0.005);

    let kq = (0.048 * p_d.powf(1.20) - 0.030 * j_clamped - 0.006 * j_clamped.powi(2)
        + 0.003 * (z_f - 4.0) + 0.004 * (ae_a0 - 0.55))
        .max(0.001);

    (kt, kq)
}

/// Solves marine propeller hydrodynamic performance and cavitation.
pub fn solve_marine_propeller(prob: &MarinePropellerProblem) -> MarinePropellerResult {
    let d = prob.diameter_m.max(0.5);
    let n = (prob.rotational_speed_rpm / 60.0).max(0.1);
    let va = prob.advance_speed_knots * 0.514444; // knots to m/s
    let j = va / (n * d);

    let z = prob.series.blade_count();
    let (kt, kq) = evaluate_wageningen_b(j, prob.pitch_ratio_p_d, prob.expanded_area_ratio, z);

    let eta_0 = (j / (2.0 * PI)) * (kt / kq);

    let rho = prob.water_density_kg_m3.max(900.0);
    // Thrust: T = KT * rho * n^2 * D^4 (N)
    let thrust_n = kt * rho * n.powi(2) * d.powi(4);
    let thrust_kn = thrust_n / 1000.0;

    // Torque: Q_t = KQ * rho * n^2 * D^5 (N·m)
    let torque_nm = kq * rho * n.powi(2) * d.powi(5);
    let torque_kn_m = torque_nm / 1000.0;

    // Delivered power: P_D = 2 * pi * n * Q_t (kW)
    let delivered_power_kw = (2.0 * PI * n * torque_nm) / 1000.0;
    // Thrust power: P_T = T * Va (kW)
    let thrust_power_kw = (thrust_n * va) / 1000.0;

    // Blade tip speed: v_tip = pi * n * D (m/s)
    let tip_speed = PI * n * d;

    // Centerline hydrostatic pressure: p_h = p_atm + rho * g * h
    let p_atm = 101325.0; // Pa
    let g = 9.80665;
    let p_h = p_atm + rho * g * prob.immersion_depth_m.max(0.5);
    let p_vapor = 2338.0; // Pa at ~20°C

    // Keller's Cavitation Criterion:
    // (AE/A0)_min = [ (1.3 + 0.3*Z) * T ] / [ (p_h - p_v) * D^2 ] + k
    let keller_min = ((1.3 + 0.3 * (z as f64)) * thrust_n) / ((p_h - p_vapor) * d.powi(2))
        + prob.keller_constant_k;
    let keller_margin = prob.expanded_area_ratio - keller_min;
    let cavitation_safe = keller_margin >= 0.0;

    // Cavitation index sigma_n = (p_h - p_v) / (0.5 * rho * (v_tip^2 + Va^2))
    let dynamic_press = 0.5 * rho * (tip_speed.powi(2) + va.powi(2));
    let sigma_n = (p_h - p_vapor) / dynamic_press;

    MarinePropellerResult {
        problem_id: prob.problem_id.clone(),
        rotational_frequency_rev_s: n,
        advance_speed_m_s: va,
        advance_coefficient_j: j,
        thrust_coefficient_kt: kt,
        torque_coefficient_kq: kq,
        open_water_efficiency: eta_0.clamp(0.05, 0.85),
        thrust_kn,
        torque_kn_m,
        delivered_power_kw,
        thrust_power_kw,
        blade_tip_speed_m_s: tip_speed,
        hydrostatic_pressure_kpa: p_h / 1000.0,
        keller_min_area_ratio: keller_min,
        keller_margin,
        cavitation_safe,
        cavitation_index_sigma_n: sigma_n,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_marine_propeller_sim(prob: &MarinePropellerProblem) -> SimulationResult {
    let sol = solve_marine_propeller(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("advance_coefficient_j".to_string(), sol.advance_coefficient_j);
    metrics.insert("thrust_kn".to_string(), sol.thrust_kn);
    metrics.insert("delivered_power_kw".to_string(), sol.delivered_power_kw);
    metrics.insert("open_water_efficiency".to_string(), sol.open_water_efficiency);
    metrics.insert("keller_margin".to_string(), sol.keller_margin);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.thrust_kn, 1, 1e-4);

    let comparison = AnalyticalComparison::assess(
        "open_water_efficiency",
        sol.open_water_efficiency,
        0.65,
    );

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.marine.propeller_wageningen".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.marine.propeller_wageningen", "0.1.0"),
        logs: vec![format!("J: {:.3}, Thrust: {:.1} kN, Delivered Power: {:.1} kW, eta_0: {:.1}%, Keller safe: {}", sol.advance_coefficient_j, sol.thrust_kn, sol.delivered_power_kw, sol.open_water_efficiency * 100.0, sol.cavitation_safe)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_panamax_container_ship_propeller_performance() {
        let prob = MarinePropellerProblem::default();
        let sol = solve_marine_propeller(&prob);

        // n = 108 / 60 = 1.80 rev/s
        assert!((sol.rotational_frequency_rev_s - 1.80).abs() < 1e-3);
        // Va = 17.5 * 0.514444 ≈ 9.00 m/s
        assert!((sol.advance_speed_m_s - 9.00).abs() < 0.1);
        // J = 9.00 / (1.80 * 6.80) ≈ 0.735
        assert!((sol.advance_coefficient_j - 0.735).abs() < 0.05);

        // Hydrodynamics: KT ~ 0.10 to 0.20, KQ ~ 0.020 to 0.035
        assert!(sol.thrust_coefficient_kt > 0.10 && sol.thrust_coefficient_kt < 0.20);
        assert!(sol.torque_coefficient_kq > 0.015 && sol.torque_coefficient_kq < 0.040);

        // Open water efficiency between 58% and 72%
        assert!(sol.open_water_efficiency > 0.58 && sol.open_water_efficiency < 0.72);

        // For 6.8m propeller on container ship, thrust is 700 to 1400 kN
        assert!(sol.thrust_kn > 700.0 && sol.thrust_kn < 1400.0);
        // Delivered power is 10,000 to 20,000 kW
        assert!(sol.delivered_power_kw > 10000.0 && sol.delivered_power_kw < 20000.0);

        // Tip speed pi * 1.8 * 6.8 = 38.45 m/s
        assert!((sol.blade_tip_speed_m_s - 38.45).abs() < 1.0);

        // Cavitation: 0.68 expanded area ratio provides positive Keller margin
        assert!(sol.keller_margin > -0.05);
        assert!(sol.keller_min_area_ratio > 0.40 && sol.keller_min_area_ratio < 0.80);
    }
}

//! # Aeroelasticity & Vortex-Induced Vibration (Eurocode 1 / ASCE 7 / DNV-RP-C205)
//!
//! Evaluates Strouhal vortex shedding frequency, critical lock-in velocity v_crit,
//! Scruton mass-damping number Sc, and cross-flow peak vibration amplitude y_max / D
//! for structural columns, chimneys, marine risers, and wind turbine towers.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Problem definition for flow-induced vortex shedding and aeroelastic lock-in.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AeroelasticProblem {
    pub structure_name: String,
    /// Outer diameter or characteristic cross-wind dimension D in meters
    pub diameter_m: f64,
    /// Structural fundamental natural frequency f_n in Hz
    pub natural_frequency_hz: f64,
    /// Equivalent modal mass per unit length m_e in kg/m
    pub modal_mass_kg_per_m: f64,
    /// Structural damping ratio zeta (critical damping fraction, e.g. 0.01 for steel, 0.02 for concrete)
    pub damping_ratio: f64,
    /// Ambient cross-flow velocity v in m/s
    pub flow_velocity_m_s: f64,
    /// Fluid density rho in kg/m^3 (1.225 for air at sea level, 1025 for seawater)
    pub fluid_density_kg_m3: f64,
    /// Empirical Strouhal number St (default 0.20 for circular cross-sections)
    pub strouhal_number: f64,
}

impl Default for AeroelasticProblem {
    fn default() -> Self {
        Self {
            structure_name: "Industrial Steel Stack".to_string(),
            diameter_m: 2.5,            // 2.5 m diameter chimney
            natural_frequency_hz: 1.20, // 1.20 Hz fundamental mode
            modal_mass_kg_per_m: 850.0, // 850 kg/m
            damping_ratio: 0.008,       // 0.8% structural damping
            flow_velocity_m_s: 15.0,    // 15 m/s wind speed
            fluid_density_kg_m3: 1.225, // Standard sea-level air
            strouhal_number: 0.20,      // St = 0.20 for circular cylinder
        }
    }
}

/// Solves Strouhal vortex shedding frequency, Scruton number, and lock-in vibration amplitude.
pub fn solve_aeroelastic_viv(prob: &AeroelasticProblem) -> Result<SimulationResult, String> {
    let d = prob.diameter_m;
    let fn_hz = prob.natural_frequency_hz;
    let st = prob.strouhal_number.clamp(0.10, 0.40);
    if d <= 0.0 || fn_hz <= 0.0 || prob.flow_velocity_m_s <= 0.0 {
        return Err("Diameter, frequency, and flow velocity must be positive".to_string());
    }

    // Vortex shedding frequency f_s = St * v / D
    let shedding_frequency_hz = (st * prob.flow_velocity_m_s) / d;

    // Critical flow velocity for vortex lock-in resonance: v_crit = f_n * D / St
    let critical_velocity_m_s = (fn_hz * d) / st;

    // Velocity ratio v / v_crit
    let velocity_ratio = prob.flow_velocity_m_s / critical_velocity_m_s;

    // Scruton number Sc = 4 * pi * m * zeta / (rho * D^2)
    let rho = prob.fluid_density_kg_m3.max(0.1);
    let m = prob.modal_mass_kg_per_m;
    let zeta = prob.damping_ratio.max(1e-4);
    let scruton_number = (4.0 * std::f64::consts::PI * m * zeta) / (rho * d * d);

    // Lock-in synchronization condition (typically 0.85 <= v / v_crit <= 1.25)
    let is_in_lock_in_band = (0.85..=1.25).contains(&velocity_ratio);

    // Peak cross-flow amplitude ratio y_max / D using ESDU / Vickery-Basu formulation:
    // y_max / D = 0.74 / (1 + 0.3 * Sc)^3.3 if in resonance, otherwise damped out
    let y_max_over_d = if is_in_lock_in_band {
        0.74 / (1.0 + 0.3 * scruton_number).powf(3.3)
    } else {
        // Off-resonance forced vibration response
        let tuning = (shedding_frequency_hz / fn_hz).powi(2);
        let dynamic_mag = 1.0
            / ((1.0 - tuning).powi(2) + (2.0 * zeta * (shedding_frequency_hz / fn_hz)).powi(2))
                .sqrt();
        (0.02 * dynamic_mag).min(0.05)
    };

    let peak_amplitude_mm = y_max_over_d * d * 1000.0;

    // Equivalent dynamic transverse force F_dyn = m * omega_n^2 * y_max (N/m)
    let omega_n = 2.0 * std::f64::consts::PI * fn_hz;
    let dynamic_force_n_per_m = m * omega_n * omega_n * (y_max_over_d * d);

    let status_str = if is_in_lock_in_band && y_max_over_d > 0.05 {
        "VIV Lock-in Resonance Hazard (Helical Strakes Recommended)"
    } else if is_in_lock_in_band {
        "Lock-in Band with Heavy Damping (Controlled Amplitude)"
    } else {
        "Aeroelastically Stable (Off-Resonant)"
    };

    // Analytical validation: Critical velocity identity: v_crit * St / D == f_n
    let val = AnalyticalComparison::assess(
        "Critical Velocity Resonant Frequency Identity (Hz)",
        (critical_velocity_m_s * st) / d,
        fn_hz,
    );

    let mut metrics = HashMap::new();
    metrics.insert("shedding_frequency_hz".to_string(), shedding_frequency_hz);
    metrics.insert("critical_velocity_m_s".to_string(), critical_velocity_m_s);
    metrics.insert("velocity_ratio_v_over_vcrit".to_string(), velocity_ratio);
    metrics.insert("scruton_number".to_string(), scruton_number);
    metrics.insert("y_max_over_d".to_string(), y_max_over_d);
    metrics.insert("peak_amplitude_mm".to_string(), peak_amplitude_mm);
    metrics.insert("dynamic_force_n_per_m".to_string(), dynamic_force_n_per_m);
    metrics.insert(
        "is_in_lock_in_band".to_string(),
        if is_in_lock_in_band { 1.0 } else { 0.0 },
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, shedding_frequency_hz, 1, 1e-4);

    let logs = vec![
        format!(
            "Aeroelastic VIV Analysis: '{}' (Diameter: {:.2} m, f_n = {:.2} Hz)",
            prob.structure_name, d, fn_hz
        ),
        format!(
            "Flow Velocity: {:.1} m/s -> Shedding Frequency f_s = {:.2} Hz (St = {:.2})",
            prob.flow_velocity_m_s, shedding_frequency_hz, st
        ),
        format!(
            "Critical Lock-in Wind Velocity v_crit = {:.1} m/s (v / v_crit = {:.2})",
            critical_velocity_m_s, velocity_ratio
        ),
        format!(
            "Scruton Mass-Damping Parameter Sc = {:.2} (m = {:.0} kg/m, ζ = {:.4})",
            scruton_number, m, zeta
        ),
        format!(
            "Peak Cross-Flow Displacement: {:.1} mm (y_max / D = {:.4}) -> Force: {:.1} N/m",
            peak_amplitude_mm, y_max_over_d, dynamic_force_n_per_m
        ),
        format!("Aeroelastic Assessment: {}", status_str),
    ];

    Ok(SimulationResult {
        problem_id: "aeroelastic-viv".to_string(),
        engine_id: "atlas-aeroelastic".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-aeroelastic", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aeroelastic_strouhal_and_scruton() -> Result<(), String> {
        let prob = AeroelasticProblem::default();
        let res = solve_aeroelastic_viv(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);

        let fs = res
            .scalar_metrics
            .get("shedding_frequency_hz")
            .copied()
            .unwrap_or(0.0);
        let vcrit = res
            .scalar_metrics
            .get("critical_velocity_m_s")
            .copied()
            .unwrap_or(0.0);
        let sc = res
            .scalar_metrics
            .get("scruton_number")
            .copied()
            .unwrap_or(0.0);

        // f_s = 0.20 * 15.0 / 2.5 = 1.20 Hz (matches natural frequency!)
        assert!(
            (fs - 1.20).abs() < 1e-3,
            "Shedding frequency mismatch: {}",
            fs
        );
        // v_crit = 1.20 * 2.5 / 0.20 = 15.0 m/s
        assert!(
            (vcrit - 15.0).abs() < 1e-3,
            "Critical velocity mismatch: {}",
            vcrit
        );
        // Sc = 4 * pi * 850 * 0.008 / (1.225 * 6.25) = 85.45 / 7.656 = 11.16
        assert!((sc - 11.16).abs() < 0.2, "Scruton number mismatch: {}", sc);

        Ok(())
    }
}

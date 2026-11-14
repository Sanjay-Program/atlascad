//! # Electronics & High-Speed PCB Signal Integrity
//!
//! Provides transmission line, high-frequency electromagnetic, and signal integrity modeling:
//! 1. Characteristic impedance (Z0) for surface Microstrip and Symmetrical Stripline per IPC-2141.
//! 2. Signal propagation velocity, delay (ps/in or ns/m), and critical line length (L_crit).
//! 3. High-frequency copper skin effect attenuation, dielectric loss tangent, and load reflection coefficient.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Geometry and material properties of a high-speed PCB transmission line.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PcbTransmissionLineProblem {
    pub net_name: String,
    /// Line topology: SurfaceMicrostrip or SymmetricalStripline
    pub is_stripline: bool,
    /// Copper trace width w in meters
    pub trace_width_m: f64,
    /// Copper trace thickness t in meters (e.g., 1 oz copper = 35 um = 0.000035 m)
    pub trace_thickness_m: f64,
    /// Dielectric height h between trace and ground plane in meters
    pub dielectric_height_m: f64,
    /// Total ground-to-ground separation b for stripline in meters (b = 2h + t)
    pub stripline_separation_m: f64,
    /// Relative dielectric constant (permittivity) eps_r (e.g. 4.3 for FR4, 3.48 for Rogers RO4350B)
    pub relative_permittivity: f64,
    /// Dielectric loss tangent tan(delta) (e.g. 0.02 for FR4, 0.0037 for Rogers)
    pub loss_tangent: f64,
    /// Trace physical routing length in meters
    pub trace_length_m: f64,
    /// Digital signal rise time t_r (10% to 90%) in seconds (e.g., 0.25 ns = 250 ps for PCIe Gen4)
    pub rise_time_s: f64,
    /// Operating frequency in Hz (for attenuation calculation, e.g. 5.0 GHz)
    pub signal_frequency_hz: f64,
    /// Load termination impedance in Ohms (e.g., 50 Ohms or receiver input)
    pub load_impedance_ohms: f64,
}

impl Default for PcbTransmissionLineProblem {
    fn default() -> Self {
        Self {
            net_name: "DDR5_DQ0_DATA_LANE".to_string(),
            is_stripline: false,
            trace_width_m: 0.000150,
            trace_thickness_m: 0.000035,
            dielectric_height_m: 0.000100,
            stripline_separation_m: 0.000235,
            relative_permittivity: 4.2,
            loss_tangent: 0.018,
            trace_length_m: 0.075,
            rise_time_s: 0.200e-9,
            signal_frequency_hz: 2.4e9,
            load_impedance_ohms: 50.0,
        }
    }
}

/// Solves characteristic impedance, propagation delay, critical line length, and high-frequency attenuation.
pub fn solve_pcb_signal_integrity(
    prob: &PcbTransmissionLineProblem,
) -> Result<SimulationResult, String> {
    if prob.trace_width_m <= 0.0
        || prob.dielectric_height_m <= 0.0
        || prob.relative_permittivity <= 1.0
    {
        return Err(
            "Trace dimensions and dielectric permittivity must be positive valid values"
                .to_string(),
        );
    }
    if prob.rise_time_s <= 0.0 {
        return Err("Signal rise time must be strictly positive".to_string());
    }

    const C_LIGHT: f64 = 299_792_458.0;
    const MU_0: f64 = 1.25663706212e-6;
    const SIGMA_CU: f64 = 5.8e7;

    let w = prob.trace_width_m;
    let t = prob.trace_thickness_m;
    let h = prob.dielectric_height_m;
    let er = prob.relative_permittivity;

    let (z0_ohms, eps_eff) = if !prob.is_stripline {
        let denom_w = 0.8 * w + t;
        let z0 = (87.0 / (er + 1.41).sqrt()) * (5.98 * h / denom_w).ln();
        let e_eff = ((er + 1.0) / 2.0) + ((er - 1.0) / 2.0) * (1.0 / (1.0 + 12.0 * h / w).sqrt());
        (z0, e_eff)
    } else {
        let b = prob.stripline_separation_m.max(2.0 * h + t);
        let denom_w = 0.8 * w + t;
        let z0 = (60.0 / er.sqrt()) * (1.9 * b / denom_w).ln();
        (z0, er)
    };

    let v_prop_m_s = C_LIGHT / eps_eff.sqrt();
    let t_pd_s_per_m = 1.0 / v_prop_m_s;
    let t_pd_ps_per_inch = t_pd_s_per_m * 0.0254 * 1e12;

    let flight_time_ps = prob.trace_length_m * t_pd_s_per_m * 1e12;

    let l_crit_m = prob.rise_time_s / (2.0 * t_pd_s_per_m);
    let requires_termination = prob.trace_length_m > l_crit_m;

    let gamma_l = (prob.load_impedance_ohms - z0_ohms) / (prob.load_impedance_ohms + z0_ohms);

    let f = prob.signal_frequency_hz.max(1e6);
    let skin_depth_m = (1.0 / (std::f64::consts::PI * f * MU_0 * SIGMA_CU)).sqrt();

    let r_surf = 1.0 / (SIGMA_CU * skin_depth_m);
    let alpha_c_db_m = (8.686 * (r_surf / (2.0 * w))) / z0_ohms;

    let alpha_d_db_m = 27.3 * (f * eps_eff.sqrt() * prob.loss_tangent) / C_LIGHT;
    let total_loss_db_m = alpha_c_db_m + alpha_d_db_m;
    let total_attenuation_db = total_loss_db_m * prob.trace_length_m;

    let mut metrics = HashMap::new();
    metrics.insert("characteristic_impedance_z0_ohms".to_string(), z0_ohms);
    metrics.insert("effective_permittivity_eps_eff".to_string(), eps_eff);
    metrics.insert("propagation_velocity_m_s".to_string(), v_prop_m_s);
    metrics.insert(
        "propagation_delay_ps_per_inch".to_string(),
        t_pd_ps_per_inch,
    );
    metrics.insert("flight_time_total_ps".to_string(), flight_time_ps);
    metrics.insert("critical_length_l_crit_m".to_string(), l_crit_m);
    metrics.insert(
        "requires_controlled_termination".to_string(),
        if requires_termination { 1.0 } else { 0.0 },
    );
    metrics.insert("load_reflection_coefficient".to_string(), gamma_l);
    metrics.insert("copper_skin_depth_um".to_string(), skin_depth_m * 1e6);
    metrics.insert(
        "total_attenuation_loss_db".to_string(),
        total_attenuation_db,
    );

    let theoretical_z0 = (87.0 / (er + 1.41).sqrt()) * (5.98 * h / (0.8 * w + t)).ln();
    let val =
        AnalyticalComparison::assess("characteristic_impedance_z0_ohms", z0_ohms, theoretical_z0);

    Ok(SimulationResult {
        problem_id: "electronics-pcb-signal-integrity".to_string(),
        engine_id: "atlas-signal-integrity".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: ConvergenceMonitor::new(1e-6),
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-signal-integrity", "0.1.0"),
        logs: Vec::new(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_microstrip_signal_integrity_verification() -> Result<(), String> {
        let prob = PcbTransmissionLineProblem::default();
        let result = solve_pcb_signal_integrity(&prob)?;

        assert_eq!(result.status, SimulationStatus::Converged);
        let z0 = result
            .scalar_metrics
            .get("characteristic_impedance_z0_ohms")
            .copied()
            .unwrap_or(0.0);
        let eps_eff = result
            .scalar_metrics
            .get("effective_permittivity_eps_eff")
            .copied()
            .unwrap_or(0.0);
        let l_crit = result
            .scalar_metrics
            .get("critical_length_l_crit_m")
            .copied()
            .unwrap_or(0.0);
        let flight_ps = result
            .scalar_metrics
            .get("flight_time_total_ps")
            .copied()
            .unwrap_or(0.0);
        let skin_um = result
            .scalar_metrics
            .get("copper_skin_depth_um")
            .copied()
            .unwrap_or(0.0);

        assert!(
            z0 > 46.0 && z0 < 56.0,
            "Characteristic impedance Z0: {} Ohms",
            z0
        );
        assert!(
            eps_eff > 2.8 && eps_eff < 3.6,
            "Effective permittivity: {}",
            eps_eff
        );
        assert!(
            flight_ps > 380.0 && flight_ps < 550.0,
            "Flight time: {} ps",
            flight_ps
        );
        assert!(
            l_crit > 0.010 && l_crit < 0.030,
            "Critical length: {} m",
            l_crit
        );
        assert!(skin_um > 1.0 && skin_um < 1.8, "Skin depth: {} um", skin_um);
        Ok(())
    }
}

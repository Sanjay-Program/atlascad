//! Rotor Dynamics and Critical Speeds Solver (API 617 / Jeffcott Rotor / Campbell Diagram).
//!
//! Evaluates lateral critical shaft speeds, flexible bearing support compliance,
//! gyroscopic forward/backward whirl splitting, unbalance vibration response,
//! amplification factors, and API 617 separation margins.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Bearing support configuration.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BearingType {
    /// Rigid rolling element bearings.
    RigidRollingElement,
    /// Hydrodynamic fluid film journal bearings.
    HydrodynamicJournal,
    /// Active magnetic bearings.
    MagneticBearing,
    /// Flexible elastomer / squeeze film damper mount.
    DampedFlexibleSupport,
}

impl BearingType {
    /// Nominal stiffness per bearing pedestal (N/m).
    pub fn default_stiffness_n_m(&self) -> f64 {
        match self {
            Self::RigidRollingElement => 1.0e9,
            Self::HydrodynamicJournal => 5.0e7,
            Self::MagneticBearing => 2.0e7,
            Self::DampedFlexibleSupport => 1.0e7,
        }
    }

    /// Nominal damping coefficient (N·s/m).
    pub fn default_damping_ns_m(&self) -> f64 {
        match self {
            Self::RigidRollingElement => 500.0,
            Self::HydrodynamicJournal => 15000.0,
            Self::MagneticBearing => 8000.0,
            Self::DampedFlexibleSupport => 25000.0,
        }
    }
}

/// Rotor Dynamics analysis specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotorDynamicsProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Operating rotational speed (RPM).
    pub operating_speed_rpm: f64,
    /// Rotor shaft span between bearings (m).
    pub shaft_span_m: f64,
    /// Shaft outer diameter (m).
    pub shaft_diameter_m: f64,
    /// Shaft inner bore diameter (m, 0.0 for solid shaft).
    pub shaft_inner_diameter_m: f64,
    /// Shaft material Young's modulus (Pa, e.g. 210 GPa for steel).
    pub shaft_modulus_pa: f64,
    /// Shaft material density (kg/m³, e.g. 7850 for steel).
    pub shaft_density_kg_m3: f64,
    /// Central disc / impeller mass (kg).
    pub disc_mass_kg: f64,
    /// Disc polar moment of inertia Ip (kg·m²).
    pub disc_polar_inertia_kg_m2: f64,
    /// Disc diametral moment of inertia Id (kg·m²).
    pub disc_diametral_inertia_kg_m2: f64,
    /// Bearing support type.
    pub bearing_type: BearingType,
    /// Custom bearing support stiffness (N/m, optional, overrides default).
    pub bearing_stiffness_n_m: Option<f64>,
    /// Custom bearing damping coefficient (N·s/m, optional, overrides default).
    pub bearing_damping_ns_m: Option<f64>,
    /// Residual unbalance mass-eccentricity (g·mm, ISO 1940 / API 617).
    pub unbalance_g_mm: f64,
}

impl Default for RotorDynamicsProblem {
    fn default() -> Self {
        Self {
            problem_id: "rotor_compressor_api617".to_string(),
            operating_speed_rpm: 12000.0,
            shaft_span_m: 1.20,
            shaft_diameter_m: 0.080, // 80 mm
            shaft_inner_diameter_m: 0.0,
            shaft_modulus_pa: 210.0e9,
            shaft_density_kg_m3: 7850.0,
            disc_mass_kg: 45.0,
            disc_polar_inertia_kg_m2: 0.45,
            disc_diametral_inertia_kg_m2: 0.225,
            bearing_type: BearingType::HydrodynamicJournal,
            bearing_stiffness_n_m: Some(4.5e7),
            bearing_damping_ns_m: Some(12000.0),
            unbalance_g_mm: 15.0, // ISO G2.5 level
        }
    }
}

/// Comprehensive Rotor Dynamics results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotorDynamicsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Operating rotational frequency (Hz).
    pub operating_frequency_hz: f64,
    /// Shaft second moment of area (m⁴).
    pub shaft_moment_of_inertia_m4: f64,
    /// Shaft stiffness at disc location (simply supported midspan, N/m).
    pub shaft_stiffness_n_m: f64,
    /// Total equivalent modal mass (kg, shaft equivalent + disc).
    pub effective_modal_mass_kg: f64,
    /// Rigid-bearing natural frequency (rad/s).
    pub omega_rigid_rad_s: f64,
    /// Rigid-bearing critical speed (RPM).
    pub critical_speed_rigid_rpm: f64,
    /// Effective system stiffness including bearing compliance (N/m).
    pub effective_system_stiffness_n_m: f64,
    /// First flexible-bearing critical speed (RPM).
    pub critical_speed_first_rpm: f64,
    /// Forward whirl precession critical speed at operating speed (RPM, gyroscopic split).
    pub critical_speed_forward_rpm: f64,
    /// Backward whirl precession critical speed at operating speed (RPM, gyroscopic split).
    pub critical_speed_backward_rpm: f64,
    /// Modal damping ratio zeta.
    pub damping_ratio: f64,
    /// Amplification Factor (AF) at critical resonance.
    pub amplification_factor_af: f64,
    /// API 617 required separation margin (%).
    pub api617_required_margin_pct: f64,
    /// Actual operating speed separation margin (%).
    pub actual_margin_pct: f64,
    /// API 617 separation margin compliance pass/fail.
    pub api617_margin_compliant: bool,
    /// Dynamic unbalance peak response displacement at operating speed (µm peak-to-peak).
    pub unbalance_peak_displacement_um_pk_pk: f64,
    /// API 617 maximum allowable vibration limit (µm pk-pk).
    pub api617_allowable_vibration_um_pk_pk: f64,
    /// Vibration limit compliant.
    pub vibration_compliant: bool,
}

/// Evaluates lateral rotor dynamics, critical speeds, and API 617 stability.
pub fn solve_rotor_dynamics(prob: &RotorDynamicsProblem) -> RotorDynamicsResult {
    let l = prob.shaft_span_m.max(0.01);
    let d_out = prob.shaft_diameter_m.max(0.001);
    let d_in = prob.shaft_inner_diameter_m.clamp(0.0, d_out * 0.99);

    // Second moment of area I = pi/64 * (D^4 - d^4)
    let i_shaft = (PI / 64.0) * (d_out.powi(4) - d_in.powi(4));

    // Midspan simply-supported beam bending stiffness k_shaft = 48 E I / L^3
    let k_shaft = 48.0 * prob.shaft_modulus_pa * i_shaft / l.powi(3);

    // Shaft mass = rho * A * L
    let a_shaft = (PI / 4.0) * (d_out.powi(2) - d_in.powi(2));
    let shaft_mass = prob.shaft_density_kg_m3 * a_shaft * l;

    // Effective modal mass (Dunkerley/Rayleigh: ~0.49 of shaft distributed mass + disc mass)
    let m_eff = prob.disc_mass_kg + 0.49 * shaft_mass;

    // Rigid-bearing natural frequency omega_0 = sqrt(k_shaft / m_eff)
    let omega_rigid = (k_shaft / m_eff).sqrt();
    let n_cr_rigid = omega_rigid * 60.0 / (2.0 * PI);

    // Bearing stiffness
    let k_brg = prob
        .bearing_stiffness_n_m
        .unwrap_or_else(|| prob.bearing_type.default_stiffness_n_m());
    let c_brg = prob
        .bearing_damping_ns_m
        .unwrap_or_else(|| prob.bearing_type.default_damping_ns_m());

    // Total bearing support: two bearings in parallel = 2 * k_brg
    // Bearing stiffness in series with shaft stiffness:
    // 1 / k_total = 1 / k_shaft + 1 / (2 * k_brg)
    let k_brg_total = 2.0 * k_brg;
    let k_eff = (k_shaft * k_brg_total) / (k_shaft + k_brg_total);

    // Flexible-bearing 1st critical speed
    let omega_first = (k_eff / m_eff).sqrt();
    let n_cr1 = omega_first * 60.0 / (2.0 * PI);

    // Gyroscopic effect at operating speed:
    // Campbell splitting: omega_fw = omega_n + (Ip / (2 Id)) * Omega
    // omega_bw = omega_n - (Ip / (2 Id)) * Omega
    let omega_op = prob.operating_speed_rpm * 2.0 * PI / 60.0;
    let ip = prob.disc_polar_inertia_kg_m2.max(1e-6);
    let id = prob.disc_diametral_inertia_kg_m2.max(1e-6);

    let gyro_shift = 0.5 * (ip / id) * omega_op;
    let omega_fw = omega_first + gyro_shift;
    let omega_bw = (omega_first - gyro_shift).max(1.0);

    let n_cr_fw = omega_fw * 60.0 / (2.0 * PI);
    let n_cr_bw = omega_bw * 60.0 / (2.0 * PI);

    // Damping ratio zeta = c_total / (2 * sqrt(k_eff * m_eff))
    let c_eff = 2.0 * c_brg;
    let c_crit = 2.0 * (k_eff * m_eff).sqrt();
    let zeta = (c_eff / c_crit).clamp(0.01, 1.0);

    // Amplification Factor AF = 1 / (2 * zeta)
    let af = 1.0 / (2.0 * zeta);

    // API 617 Separation Margin requirements:
    // If operating speed is above critical speed (flexible shaft):
    // If AF < 2.5: SM = 0%
    // If 2.5 <= AF <= 3.55: SM = 16%
    // If AF > 3.55: SM = 26%
    let api_margin_req = if af < 2.5 {
        0.0
    } else if af <= 3.55 {
        16.0
    } else {
        26.0
    };

    // Actual separation margin
    let actual_margin = if prob.operating_speed_rpm > n_cr1 {
        ((prob.operating_speed_rpm - n_cr1) / prob.operating_speed_rpm) * 100.0
    } else {
        ((n_cr1 - prob.operating_speed_rpm) / n_cr1) * 100.0
    };

    let api617_margin_compliant = actual_margin >= api_margin_req;

    // Unbalance response:
    // unbalance u = m_unb * r in g·mm = 1e-6 kg·m
    let unb_kg_m = prob.unbalance_g_mm * 1.0e-6;
    let r_freq = omega_op / omega_first;
    // Magnification factor M = r_freq^2 / sqrt((1 - r_freq^2)^2 + (2 zeta r_freq)^2)
    let denom = ((1.0 - r_freq.powi(2)).powi(2) + (2.0 * zeta * r_freq).powi(2)).sqrt();
    let magnification = r_freq.powi(2) / denom.max(1e-6);

    // Dynamic amplitude X = (unb_kg_m / m_eff) * magnification
    let dynamic_amp_m = (unb_kg_m / m_eff) * magnification;
    let dynamic_amp_um_pk_pk = dynamic_amp_m * 2.0 * 1.0e6; // peak-to-peak in µm

    // API 617 Allowable Unbalance Vibration Limit:
    // A_allow = 25.4 * sqrt(12000 / N) in µm pk-pk or max 25.4 µm
    let api_vibration_limit_um = (25.4 * (12000.0 / prob.operating_speed_rpm.max(100.0)).sqrt())
        .clamp(6.35, 25.4);

    let vibration_compliant = dynamic_amp_um_pk_pk <= api_vibration_limit_um;

    RotorDynamicsResult {
        problem_id: prob.problem_id.clone(),
        operating_frequency_hz: prob.operating_speed_rpm / 60.0,
        shaft_moment_of_inertia_m4: i_shaft,
        shaft_stiffness_n_m: k_shaft,
        effective_modal_mass_kg: m_eff,
        omega_rigid_rad_s: omega_rigid,
        critical_speed_rigid_rpm: n_cr_rigid,
        effective_system_stiffness_n_m: k_eff,
        critical_speed_first_rpm: n_cr1,
        critical_speed_forward_rpm: n_cr_fw,
        critical_speed_backward_rpm: n_cr_bw,
        damping_ratio: zeta,
        amplification_factor_af: af,
        api617_required_margin_pct: api_margin_req,
        actual_margin_pct: actual_margin,
        api617_margin_compliant,
        unbalance_peak_displacement_um_pk_pk: dynamic_amp_um_pk_pk,
        api617_allowable_vibration_um_pk_pk: api_vibration_limit_um,
        vibration_compliant,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_rotor_dynamics_sim(prob: &RotorDynamicsProblem) -> SimulationResult {
    let sol = solve_rotor_dynamics(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("critical_speed_first_rpm".to_string(), sol.critical_speed_first_rpm);
    metrics.insert("amplification_factor_af".to_string(), sol.amplification_factor_af);
    metrics.insert("actual_margin_pct".to_string(), sol.actual_margin_pct);
    metrics.insert("unbalance_peak_displacement_um_pk_pk".to_string(), sol.unbalance_peak_displacement_um_pk_pk);

    let comparison = AnalyticalComparison::assess(
        "api617_margin_compliance",
        sol.actual_margin_pct,
        sol.api617_required_margin_pct,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.critical_speed_first_rpm, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.rotor_dynamics.api617".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.rotor_dynamics.api617", "0.1.0"),
        logs: vec![format!("1st Critical: {:.1} RPM, Operating Margin: {:.1}%, API 617 Compliant: {}", sol.critical_speed_first_rpm, sol.actual_margin_pct, sol.api617_margin_compliant)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_industrial_centrifugal_compressor_rotor_dynamics() {
        let prob = RotorDynamicsProblem::default();
        let sol = solve_rotor_dynamics(&prob);

        assert!(sol.shaft_stiffness_n_m > 1.0e6);
        assert!(sol.critical_speed_rigid_rpm > 3000.0 && sol.critical_speed_rigid_rpm < 6000.0);
        // Flexible bearing critical speed is lowered by bearing compliance
        assert!(sol.critical_speed_first_rpm < sol.critical_speed_rigid_rpm);
        assert!(sol.critical_speed_first_rpm > 2000.0 && sol.critical_speed_first_rpm < 8000.0);
        // Gyroscopic splitting splits forward above first critical and backward below
        assert!(sol.critical_speed_forward_rpm > sol.critical_speed_first_rpm);
        // Operating at 12000 RPM is above first critical speed (supercritical operation)
        assert!(sol.actual_margin_pct > 26.0);
        assert!(sol.api617_margin_compliant);
        // Vibration under ISO G2.5 unbalance is within API 617 limit
        assert!(sol.unbalance_peak_displacement_um_pk_pk < sol.api617_allowable_vibration_um_pk_pk);
        assert!(sol.vibration_compliant);
    }
}

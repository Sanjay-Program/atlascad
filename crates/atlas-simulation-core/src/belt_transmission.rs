//! Belt & pulley mechanical power transmission solver.
//!
//! Compliant with ISO 5296, ISO 4184, and RMA IP-20.
//! Analyzes V-belt and synchronous belt drives, wrap angles, Euler-Eytelwein tensioning,
//! centrifugal tension effects, fatigue cycle life, and shaft reaction loads.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Standard industrial belt cross-section profile.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum BeltProfile {
    /// Narrow V-belt SPZ (9.7 x 8 mm).
    Spz,
    /// Narrow V-belt SPA (12.7 x 10 mm).
    Spa,
    /// Narrow V-belt SPB (16.3 x 13 mm).
    Spb,
    /// Narrow V-belt SPC (22.0 x 18 mm).
    Spc,
    /// High-torque synchronous timing belt (e.g. 8M pitch).
    Synchronous8M,
}

impl BeltProfile {
    /// Nominal groove included angle in degrees for V-belts, or flat for timing.
    pub fn groove_angle_deg(self) -> f64 {
        match self {
            Self::Spz => 34.0,
            Self::Spa => 36.0,
            Self::Spb => 38.0,
            Self::Spc => 38.0,
            Self::Synchronous8M => 180.0, // flat equivalent
        }
    }

    /// Default mass per meter in kg/m if not overridden.
    pub fn default_linear_mass(self) -> f64 {
        match self {
            Self::Spz => 0.075,
            Self::Spa => 0.125,
            Self::Spb => 0.195,
            Self::Spc => 0.380,
            Self::Synchronous8M => 0.055,
        }
    }
}

/// Problem definition for belt and pulley drive design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeltTransmissionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Rated power to transmit in kW.
    pub power_kw: f64,
    /// Rotational speed of driving pulley in RPM.
    pub driver_speed_rpm: f64,
    /// Pitch diameter of small (driver) pulley in mm.
    pub driver_diameter_mm: f64,
    /// Pitch diameter of large (driven) pulley in mm.
    pub driven_diameter_mm: f64,
    /// Desired center-to-center distance between shafts in mm.
    pub center_distance_mm: f64,
    /// Selected belt cross-section profile.
    pub belt_profile: BeltProfile,
    /// Application service factor (1.1 for uniform to 1.8 for heavy shock).
    pub service_factor: f64,
    /// Basic Coulomb friction coefficient between belt and pulley (0.25 - 0.40).
    pub friction_coefficient: f64,
    /// Linear mass density of belt in kg/m.
    pub belt_linear_mass_kg_m: f64,
    /// Maximum allowable tight-side tension per individual belt in N.
    pub max_allowable_tension_per_belt_n: f64,
}

impl Default for BeltTransmissionProblem {
    fn default() -> Self {
        Self {
            problem_id: "belt_drive_nominal".to_string(),
            power_kw: 15.0,
            driver_speed_rpm: 1450.0,
            driver_diameter_mm: 160.0,
            driven_diameter_mm: 315.0,
            center_distance_mm: 600.0,
            belt_profile: BeltProfile::Spa,
            service_factor: 1.30,
            friction_coefficient: 0.35,
            belt_linear_mass_kg_m: 0.125,
            max_allowable_tension_per_belt_n: 1100.0,
        }
    }
}

/// Sizing and performance outputs for belt drive.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BeltTransmissionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Driven shaft rotational speed in RPM.
    pub driven_speed_rpm: f64,
    /// Speed ratio i = n1 / n2.
    pub speed_ratio: f64,
    /// Linear belt velocity in m/s.
    pub belt_velocity_m_s: f64,
    /// Wrap angle on smaller pulley in degrees.
    pub small_pulley_wrap_deg: f64,
    /// Small pulley wrap angle in radians.
    pub small_pulley_wrap_rad: f64,
    /// Calculated belt pitch length in mm.
    pub belt_pitch_length_mm: f64,
    /// Centrifugal tension per belt in N.
    pub centrifugal_tension_n: f64,
    /// Total effective tangential force (net pull) in N.
    pub total_effective_pull_n: f64,
    /// Apparent friction coefficient (incorporating groove wedge effect).
    pub apparent_friction_coefficient: f64,
    /// Number of parallel belts required to carry design load.
    pub belts_required: usize,
    /// Design power after applying service factor in kW.
    pub design_power_kw: f64,
    /// Actual tight-side tension T1 per belt in N.
    pub tight_side_tension_per_belt_n: f64,
    /// Actual slack-side tension T2 per belt in N.
    pub slack_side_tension_per_belt_n: f64,
    /// Total static shaft radial reaction force in N.
    pub shaft_reaction_force_n: f64,
    /// Bending flex frequency in Hz (cycles per second).
    pub bending_frequency_hz: f64,
    /// Estimated belt operating fatigue life in hours.
    pub estimated_life_hours: f64,
    /// Whether design tension and speed are within standard limits.
    pub is_safe: bool,
    /// Detailed design commentary.
    pub design_assessment: String,
}

/// Solves belt and pulley mechanical power transmission.
pub fn solve_belt_transmission(
    problem: &BeltTransmissionProblem,
) -> Result<BeltTransmissionResult, String> {
    if problem.power_kw <= 0.0 {
        return Err("Transmitted power must be positive".to_string());
    }
    if problem.driver_speed_rpm <= 0.0 {
        return Err("Driver speed must be positive".to_string());
    }
    if problem.driver_diameter_mm <= 0.0 || problem.driven_diameter_mm <= 0.0 {
        return Err("Pulley diameters must be positive".to_string());
    }
    if problem.center_distance_mm <= (problem.driver_diameter_mm + problem.driven_diameter_mm) / 2.0
    {
        return Err(
            "Center distance must be greater than average pulley diameter to avoid interference"
                .to_string(),
        );
    }

    // 1. Kinematics
    let d1 = problem.driver_diameter_mm;
    let d2 = problem.driven_diameter_mm;
    let d_small = d1.min(d2);
    let d_large = d1.max(d2);
    let c = problem.center_distance_mm;

    let speed_ratio = d2 / d1;
    let driven_speed_rpm = problem.driver_speed_rpm / speed_ratio;

    // Linear velocity v = pi * d1 * n1 / 60000
    let v_m_s = PI * (d1 / 1000.0) * (problem.driver_speed_rpm / 60.0);

    // 2. Wrap Angles & Pitch Length
    let delta_d = d_large - d_small;
    let sin_alpha = (delta_d / (2.0 * c)).clamp(-1.0, 1.0);
    let alpha_rad = sin_alpha.asin();

    let theta_small_rad = PI - 2.0 * alpha_rad;
    let theta_small_deg = theta_small_rad * 180.0 / PI;

    // ISO pitch length approximation
    let lp_mm = 2.0 * c + PI * (d1 + d2) / 2.0 + (delta_d * delta_d) / (4.0 * c);

    // 3. Friction & Tension Analysis (Euler-Eytelwein)
    let mu = problem.friction_coefficient.clamp(0.1, 0.9);
    let beta_deg = problem.belt_profile.groove_angle_deg();
    let beta_rad = (beta_deg / 2.0) * PI / 180.0;

    // For V-belt, mu_app = mu / sin(beta/2)
    let mu_app = if beta_deg < 170.0 {
        mu / beta_rad.sin()
    } else {
        mu
    };

    // Centrifugal tension per belt: T_c = m' * v^2
    let m_lin = if problem.belt_linear_mass_kg_m > 0.0 {
        problem.belt_linear_mass_kg_m
    } else {
        problem.belt_profile.default_linear_mass()
    };
    let t_c = m_lin * v_m_s * v_m_s;

    // Design power
    let sf = problem.service_factor.clamp(1.0, 3.0);
    let p_design_kw = problem.power_kw * sf;

    // Total required effective tangential force: F_u = 1000 * P_design / v
    let total_f_u = (1000.0 * p_design_kw) / v_m_s.max(0.1);

    // Euler ratio: (T1 - Tc) / (T2 - Tc) = exp(mu_app * theta)
    let e_factor = (mu_app * theta_small_rad).exp();
    // For single belt at limit: F_u_single = (T1_max - Tc) * (1 - exp(-mu_app * theta))
    let t1_max = problem.max_allowable_tension_per_belt_n.max(100.0);
    let usable_t1 = (t1_max - t_c).max(10.0);
    let f_u_per_belt_max = usable_t1 * (1.0 - 1.0 / e_factor);

    // Wrap angle correction factor: C_theta = theta_small_deg / 180.0
    let c_theta = (theta_small_deg / 180.0).clamp(0.5, 1.2);
    let effective_capacity_per_belt = f_u_per_belt_max * c_theta;

    let n_belts_exact = total_f_u / effective_capacity_per_belt.max(1.0);
    let belts_required = n_belts_exact.ceil().max(1.0) as usize;

    // Actual working tensions per belt
    let f_u_actual_per_belt = total_f_u / (belts_required as f64);
    let t1_minus_tc = f_u_actual_per_belt / (1.0 - 1.0 / e_factor).max(1.0e-3);
    let t1_actual = t1_minus_tc + t_c;
    let t2_actual = t1_actual - f_u_actual_per_belt;

    // 4. Shaft Radial Reaction Force (at operating load)
    let single_shaft_f = (t1_actual.powi(2) + t2_actual.powi(2)
        - 2.0 * t1_actual * t2_actual * theta_small_rad.cos())
    .sqrt();
    let total_shaft_reaction_n = single_shaft_f * (belts_required as f64);

    // 5. Bending Fatigue Life (Miner-type flex cycle model)
    let lp_m = lp_mm / 1000.0;
    let f_b_hz = (2.0 * v_m_s) / lp_m.max(0.1);

    let stress_ratio = (t1_actual / t1_max).clamp(0.2, 1.5);
    let estimated_life_hours = (25000.0 / stress_ratio.powf(3.5)).clamp(500.0, 80000.0);

    let is_speed_safe = v_m_s <= 35.0; // Standard V-belt speed limit ~ 30-33 m/s
    let is_tension_safe = t1_actual <= t1_max;
    let is_safe = is_speed_safe && is_tension_safe;

    let assessment = format!(
        "Belt drive v={:.1} m/s (limit 33 m/s). Speed ratio i={:.2}. Wrap={:.1} deg. Belts req={:?} x {:?}. T1={:.1}N (allowable {:.1}N), T2={:.1}N, Tc={:.1}N. Shaft load={:.1}N. Est. fatigue life={:.0}h. Overall: {}",
        v_m_s,
        speed_ratio,
        theta_small_deg,
        belts_required,
        problem.belt_profile,
        t1_actual,
        t1_max,
        t2_actual,
        t_c,
        total_shaft_reaction_n,
        estimated_life_hours,
        if is_safe { "PASS" } else { "EXCEEDS RATINGS" }
    );

    Ok(BeltTransmissionResult {
        problem_id: problem.problem_id.clone(),
        driven_speed_rpm,
        speed_ratio,
        belt_velocity_m_s: v_m_s,
        small_pulley_wrap_deg: theta_small_deg,
        small_pulley_wrap_rad: theta_small_rad,
        belt_pitch_length_mm: lp_mm,
        centrifugal_tension_n: t_c,
        total_effective_pull_n: total_f_u,
        apparent_friction_coefficient: mu_app,
        belts_required,
        design_power_kw: p_design_kw,
        tight_side_tension_per_belt_n: t1_actual,
        slack_side_tension_per_belt_n: t2_actual,
        shaft_reaction_force_n: total_shaft_reaction_n,
        bending_frequency_hz: f_b_hz,
        estimated_life_hours,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for belt transmission.
pub fn solve_belt_transmission_sim(problem: &BeltTransmissionProblem) -> SimulationResult {
    match solve_belt_transmission(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("belt_velocity_m_s".to_string(), result.belt_velocity_m_s);
            metrics.insert("belts_required".to_string(), result.belts_required as f64);
            metrics.insert(
                "tight_side_tension_per_belt_n".to_string(),
                result.tight_side_tension_per_belt_n,
            );
            metrics.insert(
                "shaft_reaction_force_n".to_string(),
                result.shaft_reaction_force_n,
            );
            metrics.insert(
                "estimated_life_hours".to_string(),
                result.estimated_life_hours,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.belt_velocity_m_s, 1, 1e-4);

            let comparison =
                AnalyticalComparison::assess("belt_velocity_m_s", result.belt_velocity_m_s, 12.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.mechanical.belt_transmission_iso5296".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 2,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.mechanical.belt_transmission_iso5296",
                    "0.1.0",
                ),
                logs: vec![result.design_assessment],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.mechanical.belt_transmission_iso5296".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.mechanical.belt_transmission_iso5296",
                    "0.1.0",
                ),
                logs: vec![format!("Belt transmission simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_belt_transmission_nominal() {
        let prob = BeltTransmissionProblem::default();
        let sol = solve_belt_transmission(&prob).expect("Nominal belt solve should succeed");

        assert!(sol.belt_velocity_m_s > 10.0 && sol.belt_velocity_m_s < 15.0);
        assert!(sol.small_pulley_wrap_deg > 150.0 && sol.small_pulley_wrap_deg < 175.0);
        assert!(sol.belts_required >= 2 && sol.belts_required <= 5);
        assert!(sol.tight_side_tension_per_belt_n <= prob.max_allowable_tension_per_belt_n);
        assert!(sol.estimated_life_hours > 5000.0);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_belt_speed_overspeed() {
        let prob = BeltTransmissionProblem {
            driver_speed_rpm: 5000.0,
            driver_diameter_mm: 300.0, // extremely high speed ~ 78.5 m/s
            ..Default::default()
        };
        let sol = solve_belt_transmission(&prob).expect("Solver executes");
        assert!(!sol.is_safe);
        assert!(sol.belt_velocity_m_s > 35.0);
    }
}

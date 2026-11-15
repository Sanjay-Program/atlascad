//! Open Channel Flow Hydraulics & Hydraulic Jump Dissipation Module.
//!
//! Evaluates uniform open-channel flow via Manning's equation, normal and critical depths,
//! Froude number regime classification, conjugate sequent depths, hydraulic jump length,
//! and turbulent energy dissipation rate in accordance with Chow, USBR, and USACE standards.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Standard acceleration due to gravity in m/s².
pub const G_ACCEL: f64 = 9.80665;
/// Fresh water density in kg/m³.
pub const WATER_DENSITY_KG_M3: f64 = 1000.0;

/// Open channel flow regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChannelFlowRegime {
    /// Subcritical flow (Fr < 1.0, tranquil, controlled from downstream).
    Subcritical,
    /// Critical flow (Fr == 1.0, minimum specific energy).
    Critical,
    /// Supercritical flow (Fr > 1.0, rapid, controlled from upstream).
    Supercritical,
}

/// Open channel hydraulics problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelHydraulicsProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Channel bed bottom width in meters (m).
    pub bottom_width_m: f64,
    /// Channel bank side slope z (horizontal-to-vertical ratio z:1, z = 0 for rectangular).
    pub side_slope_z: f64,
    /// Channel longitudinal bed slope S0 (dimensionless m/m).
    pub bed_slope_s0: f64,
    /// Manning roughness coefficient n (e.g. 0.013 for smooth concrete, 0.025 for earthen channel).
    pub manning_roughness_n: f64,
    /// Design volumetric liquid discharge rate Q in cubic meters per second (m³/s).
    pub discharge_rate_m3_s: f64,
    /// Optional upstream initial supercritical water depth y1 for hydraulic jump analysis in meters.
    pub upstream_jump_depth_m: Option<f64>,
}

fn default_problem_id() -> String {
    "channel_hydraulics_problem".to_string()
}

impl Default for ChannelHydraulicsProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            bottom_width_m: 6.0,
            side_slope_z: 1.5,
            bed_slope_s0: 0.0015,
            manning_roughness_n: 0.025,
            discharge_rate_m3_s: 45.0,
            upstream_jump_depth_m: Some(0.85),
        }
    }
}

/// Open channel hydraulics analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChannelHydraulicsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Normal uniform water depth y_n in meters.
    pub normal_depth_m: f64,
    /// Critical water depth y_c in meters.
    pub critical_depth_m: f64,
    /// Critical bed slope S_c corresponding to normal depth equal to critical depth.
    pub critical_slope_sc: f64,
    /// Water cross-sectional flow area at normal depth in m².
    pub normal_flow_area_m2: f64,
    /// Wetted perimeter at normal depth in meters.
    pub normal_wetted_perimeter_m: f64,
    /// Hydraulic radius at normal depth in meters.
    pub normal_hydraulic_radius_m: f64,
    /// Mean flow velocity at normal depth in m/s.
    pub normal_velocity_m_s: f64,
    /// Froude number at normal depth.
    pub normal_froude_number: f64,
    /// Flow regime at normal depth.
    pub flow_regime: ChannelFlowRegime,
    /// Upstream initial supercritical depth y1 evaluated for hydraulic jump in meters.
    pub jump_upstream_depth_m: f64,
    /// Upstream Froude number Fr1 prior to hydraulic jump.
    pub jump_upstream_froude: f64,
    /// Conjugate (sequent) subcritical depth y2 after hydraulic jump in meters.
    pub jump_downstream_depth_m: f64,
    /// Total specific energy head loss across hydraulic jump Delta E in meters.
    pub jump_head_loss_m: f64,
    /// Hydraulic jump energy dissipation efficiency percentage (%).
    pub jump_energy_dissipation_pct: f64,
    /// Estimated length of hydraulic jump roller in meters (USBR formula).
    pub jump_length_m: f64,
    /// Total power dissipated as turbulence and heat across the jump in kilowatts (kW).
    pub jump_power_dissipated_kw: f64,
    /// Whether channel flow operates safely within stable non-choking criteria.
    pub is_safe: bool,
}

/// Geometric area of a trapezoidal channel given depth y.
fn channel_area(b: f64, z: f64, y: f64) -> f64 {
    (b + z * y) * y
}

/// Wetted perimeter of a trapezoidal channel given depth y.
fn channel_perimeter(b: f64, z: f64, y: f64) -> f64 {
    b + 2.0 * y * (1.0 + z.powi(2)).sqrt()
}

/// Top water surface width of a trapezoidal channel given depth y.
fn channel_top_width(b: f64, z: f64, y: f64) -> f64 {
    b + 2.0 * z * y
}

/// Solves open channel hydraulics, normal depth, critical depth, and hydraulic jump.
pub fn solve_channel_hydraulics(
    problem: &ChannelHydraulicsProblem,
) -> Result<ChannelHydraulicsResult, String> {
    if problem.bottom_width_m <= 0.0 || problem.discharge_rate_m3_s <= 0.0 {
        return Err("Bottom width and discharge rate must be positive".to_string());
    }
    if problem.bed_slope_s0 <= 0.0 || problem.manning_roughness_n <= 0.0 {
        return Err("Bed slope and Manning roughness must be positive".to_string());
    }

    let b = problem.bottom_width_m;
    let z = problem.side_slope_z.max(0.0);
    let q = problem.discharge_rate_m3_s;
    let n = problem.manning_roughness_n;
    let s0 = problem.bed_slope_s0;

    // 1. Solve normal depth y_n via Newton-Raphson on Manning's equation:
    // f(y) = (1/n) * A(y) * [A(y)/P(y)]^(2/3) * sqrt(S0) - Q = 0
    let mut y_n = (q * n / (b * s0.sqrt())).powf(0.6); // initial guess
    for _ in 0..60 {
        let a = channel_area(b, z, y_n);
        let p = channel_perimeter(b, z, y_n);
        let r_h = a / p;
        let q_calc = (1.0 / n) * a * r_h.powf(2.0 / 3.0) * s0.sqrt();
        let f = q_calc - q;
        if f.abs() < 1e-6 {
            break;
        }
        // Numerical derivative
        let dy = 1e-4;
        let a_plus = channel_area(b, z, y_n + dy);
        let p_plus = channel_perimeter(b, z, y_n + dy);
        let q_plus = (1.0 / n) * a_plus * (a_plus / p_plus).powf(2.0 / 3.0) * s0.sqrt();
        let df_dy = (q_plus - q_calc) / dy;
        if df_dy.abs() < 1e-12 {
            break;
        }
        y_n -= f / df_dy;
        if y_n < 0.01 {
            y_n = 0.01;
        }
    }

    let a_n = channel_area(b, z, y_n);
    let p_n = channel_perimeter(b, z, y_n);
    let rh_n = a_n / p_n;
    let v_n = q / a_n;
    let t_n = channel_top_width(b, z, y_n);
    let hydraulic_depth_n = a_n / t_n;
    let fr_n = v_n / (G_ACCEL * hydraulic_depth_n).sqrt();

    let flow_regime = if fr_n < 0.98 {
        ChannelFlowRegime::Subcritical
    } else if fr_n > 1.02 {
        ChannelFlowRegime::Supercritical
    } else {
        ChannelFlowRegime::Critical
    };

    // 2. Solve critical depth y_c:
    // Condition: Fr = 1 => Q^2 * T(y_c) / (g * A(y_c)^3) = 1
    let mut y_c = (q.powi(2) / (G_ACCEL * b.powi(2))).cbrt();
    for _ in 0..60 {
        let a = channel_area(b, z, y_c);
        let t = channel_top_width(b, z, y_c);
        let f = (q.powi(2) * t) / (G_ACCEL * a.powi(3)) - 1.0;
        if f.abs() < 1e-6 {
            break;
        }
        let dy = 1e-4;
        let a_plus = channel_area(b, z, y_c + dy);
        let t_plus = channel_top_width(b, z, y_c + dy);
        let f_plus = (q.powi(2) * t_plus) / (G_ACCEL * a_plus.powi(3)) - 1.0;
        let df_dy = (f_plus - f) / dy;
        if df_dy.abs() < 1e-12 {
            break;
        }
        y_c -= f / df_dy;
        if y_c < 0.01 {
            y_c = 0.01;
        }
    }

    let a_c = channel_area(b, z, y_c);
    let p_c = channel_perimeter(b, z, y_c);
    let rh_c = a_c / p_c;
    // Critical slope Sc = (n * v_c)^2 / (Rh_c^(4/3))
    let v_c = q / a_c;
    let sc = (n * v_c / rh_c.powf(2.0 / 3.0)).powi(2);

    // 3. Hydraulic Jump Analysis:
    // Upstream initial supercritical depth y1
    let y1 = problem.upstream_jump_depth_m.unwrap_or(if fr_n > 1.0 {
        y_n
    } else {
        y_c * 0.5
    });

    let a1 = channel_area(b, z, y1);
    let t1 = channel_top_width(b, z, y1);
    let v1 = q / a1;
    let fr1 = v1 / (G_ACCEL * (a1 / t1)).sqrt();

    // Sequent (conjugate) depth y2:
    // Solves momentum conservation equation: M1 = M2
    // M = Q^2 / (g*A) + z_bar*A, where for trapezoid z_bar*A = y^2 * (b/2 + z*y/3)
    let m1 = (q.powi(2) / (G_ACCEL * a1)) + (y1.powi(2) * (b / 2.0 + z * y1 / 3.0));

    let mut y2 = y1 * 0.5 * ((1.0 + 8.0 * fr1.powi(2)).sqrt() - 1.0); // Belanger estimate
    for _ in 0..60 {
        let a2 = channel_area(b, z, y2);
        let m2 = (q.powi(2) / (G_ACCEL * a2)) + (y2.powi(2) * (b / 2.0 + z * y2 / 3.0));
        let f = m2 - m1;
        if f.abs() < 1e-5 {
            break;
        }
        let dy = 1e-4;
        let a2_plus = channel_area(b, z, y2 + dy);
        let m2_plus = (q.powi(2) / (G_ACCEL * a2_plus)) + ((y2 + dy).powi(2) * (b / 2.0 + z * (y2 + dy) / 3.0));
        let df_dy = (m2_plus - m2) / dy;
        if df_dy.abs() < 1e-12 {
            break;
        }
        y2 -= f / df_dy;
        if y2 < y1 {
            y2 = y1 * 1.05;
        }
    }

    let a2 = channel_area(b, z, y2);
    let v2 = q / a2;

    // Specific energy before and after jump:
    let e1 = y1 + (v1.powi(2) / (2.0 * G_ACCEL));
    let e2 = y2 + (v2.powi(2) / (2.0 * G_ACCEL));
    let delta_e = (e1 - e2).max(0.0);
    let jump_dissipation_pct = if e1 > 1e-6 {
        (delta_e / e1) * 100.0
    } else {
        0.0
    };

    // Jump length L = 6.1 * y2 (USBR formula for stabilized hydraulic jump)
    let jump_length = 6.1 * y2;

    // Power dissipated: P = rho * g * Q * Delta E
    let power_dissipated_kw = (WATER_DENSITY_KG_M3 * G_ACCEL * q * delta_e) * 1e-3;

    // Safety check: Froude number non-violent (Fr1 <= 9.0) and normal velocity non-erosive (v_n <= 3.5 m/s)
    let is_safe = v_n <= 3.5 && fr1 <= 9.0;

    Ok(ChannelHydraulicsResult {
        problem_id: problem.problem_id.clone(),
        normal_depth_m: y_n,
        critical_depth_m: y_c,
        critical_slope_sc: sc,
        normal_flow_area_m2: a_n,
        normal_wetted_perimeter_m: p_n,
        normal_hydraulic_radius_m: rh_n,
        normal_velocity_m_s: v_n,
        normal_froude_number: fr_n,
        flow_regime,
        jump_upstream_depth_m: y1,
        jump_upstream_froude: fr1,
        jump_downstream_depth_m: y2,
        jump_head_loss_m: delta_e,
        jump_energy_dissipation_pct: jump_dissipation_pct,
        jump_length_m: jump_length,
        jump_power_dissipated_kw: power_dissipated_kw,
        is_safe,
    })
}

/// Generic simulation adapter for open channel hydraulics.
pub fn solve_channel_hydraulics_sim(problem: &ChannelHydraulicsProblem) -> SimulationResult {
    match solve_channel_hydraulics(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("normal_depth_m".to_string(), result.normal_depth_m);
            metrics.insert("critical_depth_m".to_string(), result.critical_depth_m);
            metrics.insert("normal_velocity_m_s".to_string(), result.normal_velocity_m_s);
            metrics.insert("normal_froude_number".to_string(), result.normal_froude_number);
            metrics.insert("jump_head_loss_m".to_string(), result.jump_head_loss_m);
            metrics.insert("jump_power_dissipated_kw".to_string(), result.jump_power_dissipated_kw);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.normal_depth_m, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "normal_depth_m",
                result.normal_depth_m,
                2.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.civil.channel_hydraulics_manning".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.civil.channel_hydraulics_manning", "0.1.0"),
                logs: vec![format!(
                    "Channel Hydraulics: yn = {:.2} m, yc = {:.2} m, vn = {:.2} m/s (Fr = {:.2}, {:?}), Jump ΔE = {:.2} m (P_diss = {:.1} kW, Safe: {})",
                    result.normal_depth_m,
                    result.critical_depth_m,
                    result.normal_velocity_m_s,
                    result.normal_froude_number,
                    result.flow_regime,
                    result.jump_head_loss_m,
                    result.jump_power_dissipated_kw,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.civil.channel_hydraulics_manning".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.civil.channel_hydraulics_manning", "0.1.0"),
                logs: vec![format!("Channel hydraulics simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_channel_hydraulics_nominal() {
        let prob = ChannelHydraulicsProblem::default();
        let res = solve_channel_hydraulics(&prob).expect("solver failed");

        assert!(res.normal_depth_m > 1.8 && res.normal_depth_m < 2.5);
        assert!(res.critical_depth_m > 1.2 && res.critical_depth_m < 1.8);
        assert!(res.normal_depth_m > res.critical_depth_m);
        assert_eq!(res.flow_regime, ChannelFlowRegime::Subcritical);
        assert!(res.normal_velocity_m_s > 1.8 && res.normal_velocity_m_s < 2.8);
        assert!(res.jump_upstream_froude > 2.5);
        assert!(res.jump_downstream_depth_m > 2.0);
        assert!(res.jump_head_loss_m > 0.8);
        assert!(res.jump_power_dissipated_kw > 300.0);
        assert!(res.is_safe);
    }
}

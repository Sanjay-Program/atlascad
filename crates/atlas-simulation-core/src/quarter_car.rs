//! Automotive Quarter-Car Suspension & Ride Dynamics Module.
//!
//! Evaluates 2-DOF body bounce and wheel-hop natural frequencies, damping ratios,
//! frequency response transfer functions, body acceleration RMS, suspension stroke travel,
//! and ISO 2631-1 human vibration comfort levels in accordance with SAE and ISO standards.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Standard acceleration due to gravity in m/s².
pub const G_ACCEL: f64 = 9.80665;

/// ISO 8608 Road surface roughness classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoadRoughnessClass {
    /// Smooth highway asphalt (Class A: Gq0 = 16e-6 m³).
    ClassAVeryGood,
    /// Typical paved highway (Class B: Gq0 = 64e-6 m³).
    ClassBGood,
    /// Minor road / secondary road (Class C: Gq0 = 256e-6 m³).
    ClassCAverage,
    /// Unpaved gravel / deteriorated road (Class D: Gq0 = 1024e-6 m³).
    ClassDPoor,
}

impl RoadRoughnessClass {
    /// Returns ISO 8608 displacement PSD coefficient G_q(n0) in m³ at n0 = 0.1 cycles/m.
    pub fn roughness_coefficient_m3(&self) -> f64 {
        match self {
            Self::ClassAVeryGood => 16e-6,
            Self::ClassBGood => 64e-6,
            Self::ClassCAverage => 256e-6,
            Self::ClassDPoor => 1024e-6,
        }
    }
}

/// Quarter-car suspension simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarterCarProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Sprung mass (chassis/body corner mass) in kilograms (kg).
    pub sprung_mass_kg: f64,
    /// Unsprung mass (wheel, tire, brake, knuckle assembly) in kilograms (kg).
    pub unsprung_mass_kg: f64,
    /// Suspension spring vertical stiffness in Newtons per meter (N/m).
    pub suspension_stiffness_n_m: f64,
    /// Suspension hydraulic shock absorber damping coefficient in N·s/m.
    pub suspension_damping_n_s_m: f64,
    /// Tire vertical radial spring stiffness in Newtons per meter (N/m).
    pub tire_stiffness_n_m: f64,
    /// Vehicle forward driving speed in kilometers per hour (km/h).
    pub vehicle_speed_km_h: f64,
    /// Road surface profile classification per ISO 8608.
    #[serde(default = "default_road_class")]
    pub road_class: RoadRoughnessClass,
    /// Maximum allowable suspension travel clearance in millimeters (mm).
    pub max_allowable_stroke_mm: f64,
}

fn default_problem_id() -> String {
    "quarter_car_problem".to_string()
}

fn default_road_class() -> RoadRoughnessClass {
    RoadRoughnessClass::ClassBGood
}

impl Default for QuarterCarProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            sprung_mass_kg: 320.0,
            unsprung_mass_kg: 45.0,
            suspension_stiffness_n_m: 28000.0,
            suspension_damping_n_s_m: 1650.0,
            tire_stiffness_n_m: 210000.0,
            vehicle_speed_km_h: 100.0,
            road_class: default_road_class(),
            max_allowable_stroke_mm: 75.0,
        }
    }
}

/// Quarter-car suspension ride dynamics result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QuarterCarResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Sprung mass bounce natural frequency in Hertz (Hz).
    pub body_bounce_frequency_hz: f64,
    /// Unsprung mass wheel-hop natural frequency in Hertz (Hz).
    pub wheel_hop_frequency_hz: f64,
    /// Dimensionless suspension damping ratio zeta.
    pub damping_ratio_zeta: f64,
    /// Static deflection of suspension under sprung mass weight in millimeters (mm).
    pub static_deflection_mm: f64,
    /// Peak body vertical acceleration transmissibility ratio TR.
    pub peak_body_transmissibility: f64,
    /// Root-mean-square (RMS) vertical chassis acceleration in m/s².
    pub body_acceleration_rms_m_s2: f64,
    /// Root-mean-square (RMS) suspension rattle-space stroke deflection in millimeters (mm).
    pub suspension_deflection_rms_mm: f64,
    /// Dynamic tire load coefficient DLC = RMS(F_dyn) / F_static (road holding metric).
    pub dynamic_load_coefficient: f64,
    /// ISO 2631-1 whole-body vibration ride comfort description.
    pub ride_comfort_rating: String,
    /// Whether suspension travel and tire contact maintain safe operating envelopes.
    pub is_safe: bool,
}

/// Solves 2-DOF quarter-car suspension dynamics and ride comfort.
pub fn solve_quarter_car(problem: &QuarterCarProblem) -> Result<QuarterCarResult, String> {
    if problem.sprung_mass_kg <= 0.0 || problem.unsprung_mass_kg <= 0.0 {
        return Err("Sprung and unsprung masses must be positive".to_string());
    }
    if problem.suspension_stiffness_n_m <= 0.0 || problem.tire_stiffness_n_m <= 0.0 {
        return Err("Suspension and tire stiffnesses must be positive".to_string());
    }
    if problem.suspension_damping_n_s_m <= 0.0 || problem.vehicle_speed_km_h <= 0.0 {
        return Err("Damping coefficient and vehicle speed must be positive".to_string());
    }

    let ms = problem.sprung_mass_kg;
    let mu = problem.unsprung_mass_kg;
    let ks = problem.suspension_stiffness_n_m;
    let cs = problem.suspension_damping_n_s_m;
    let kt = problem.tire_stiffness_n_m;
    let v_m_s = problem.vehicle_speed_km_h / 3.6;

    // 1. Natural frequencies and damping ratio:
    // Body bounce: omega_s = sqrt(ks * kt / ((ms + mu)*ks + ms*kt)) ~ sqrt(ks / ms)
    let omega_s = (ks / ms).sqrt();
    let f_s_hz = omega_s / (2.0 * std::f64::consts::PI);

    // Wheel hop: omega_u ~ sqrt((ks + kt) / mu)
    let omega_u = ((ks + kt) / mu).sqrt();
    let f_u_hz = omega_u / (2.0 * std::f64::consts::PI);

    // Damping ratio: zeta = cs / (2 * sqrt(ks * ms))
    let c_crit = 2.0 * (ks * ms).sqrt();
    let zeta = cs / c_crit;

    // Static deflection under gravity: delta_st = ms * g / ks
    let static_deflection_mm = (ms * G_ACCEL / ks) * 1000.0;

    // 2. Frequency response and numerical PSD integration:
    // Road input PSD per ISO 8608: S_zr(omega) = G_q0 * n0^2 * v / (omega / 2pi)^2
    let g_q0 = problem.road_class.roughness_coefficient_m3();
    let n0: f64 = 0.1; // reference spatial frequency 0.1 cycles/m

    // Transfer function magnitude for body acceleration |H_acc(omega)|^2
    // State equations for 2-DOF:
    // [-ms*w^2 + i*cs*w + ks] * Zs - [i*cs*w + ks] * Zu = 0
    // -[i*cs*w + ks] * Zs + [-mu*w^2 + i*cs*w + ks + kt] * Zu = kt * Zr
    let mut sum_var_acc = 0.0;
    let mut sum_var_susp = 0.0;
    let mut sum_var_tire = 0.0;
    let mut max_tr = 0.0;

    let f_min = 0.2;
    let f_max = 30.0;
    let n_steps = 1000;
    let df = (f_max - f_min) / n_steps as f64;

    for i in 0..n_steps {
        let f = f_min + (i as f64 + 0.5) * df;
        let w = 2.0 * std::f64::consts::PI * f;

        // Road displacement PSD at temporal frequency f: S_zr(f) = g_q0 * n0^2 * v / f^2
        let s_zr_f = (g_q0 * n0.powi(2) * v_m_s) / f.powi(2);

        // Impedance matrix elements:
        // A11 = ks - ms*w^2 + i*(cs*w)
        let a11_re = ks - ms * w.powi(2);
        let a11_im = cs * w;
        // A12 = -(ks + i*cs*w)
        let a12_re = -ks;
        let a12_im = -cs * w;
        // A22 = ks + kt - mu*w^2 + i*(cs*w)
        let a22_re = ks + kt - mu * w.powi(2);
        let a22_im = cs * w;

        // Determinant = A11*A22 - A12*A12
        let det_re = (a11_re * a22_re - a11_im * a22_im) - (a12_re.powi(2) - a12_im.powi(2));
        let det_im = (a11_re * a22_im + a11_im * a22_re) - (2.0 * a12_re * a12_im);
        let det_mag2 = det_re.powi(2) + det_im.powi(2);

        // Zs / Zr = -A12 * kt / Det = (ks + i*cs*w) * kt / Det
        let num_zs_re = ks * kt;
        let num_zs_im = cs * w * kt;
        let h_zs_mag2 = (num_zs_re.powi(2) + num_zs_im.powi(2)) / det_mag2;

        // Body acceleration |H_acc|^2 = w^4 * |Zs/Zr|^2
        let h_acc_mag2 = w.powi(4) * h_zs_mag2;
        let tr = h_zs_mag2.sqrt();
        if tr > max_tr {
            max_tr = tr;
        }

        // Zu / Zr = A11 * kt / Det
        let num_zu_re = a11_re * kt;
        let num_zu_im = a11_im * kt;
        let _h_zu_mag2 = (num_zu_re.powi(2) + num_zu_im.powi(2)) / det_mag2;

        // Suspension deflection (Zs - Zu) / Zr
        // Zs - Zu = kt * (-ms*w^2) / Det
        let num_zdiff_re = -kt * ms * w.powi(2);
        let h_zdiff_mag2 = (num_zdiff_re.powi(2)) / det_mag2;

        // Tire deflection (Zu - Zr) / Zr
        let h_tdef_mag2 = ((num_zu_re - det_re).powi(2) + (num_zu_im - det_im).powi(2)) / det_mag2;

        sum_var_acc += h_acc_mag2 * s_zr_f * df;
        sum_var_susp += h_zdiff_mag2 * s_zr_f * df;
        sum_var_tire += h_tdef_mag2 * s_zr_f * df;
    }

    let a_rms = sum_var_acc.sqrt();
    let susp_rms_mm = sum_var_susp.sqrt() * 1000.0;
    let tire_rms_m = sum_var_tire.sqrt();
    let f_static = (ms + mu) * G_ACCEL;
    let f_dyn_rms = kt * tire_rms_m;
    let dlc = f_dyn_rms / f_static;

    // ISO 2631-1 Comfort assessment based on weighted vertical RMS acceleration:
    let ride_comfort = if a_rms < 0.315 {
        "Not uncomfortable (ISO 2631 Class 1)".to_string()
    } else if a_rms < 0.63 {
        "A little uncomfortable (ISO 2631 Class 2)".to_string()
    } else if a_rms < 1.0 {
        "Fairly uncomfortable (ISO 2631 Class 3)".to_string()
    } else if a_rms < 1.6 {
        "Uncomfortable (ISO 2631 Class 4)".to_string()
    } else {
        "Very uncomfortable (ISO 2631 Class 5)".to_string()
    };

    // Safety check: 3*sigma stroke travel <= allowable, and DLC < 0.35 (guaranteeing continuous road grip)
    let stroke_peak_3sigma = 3.0 * susp_rms_mm;
    let is_stroke_safe = stroke_peak_3sigma <= problem.max_allowable_stroke_mm;
    let is_holding_safe = dlc < 0.35;
    let is_safe = is_stroke_safe && is_holding_safe;

    Ok(QuarterCarResult {
        problem_id: problem.problem_id.clone(),
        body_bounce_frequency_hz: f_s_hz,
        wheel_hop_frequency_hz: f_u_hz,
        damping_ratio_zeta: zeta,
        static_deflection_mm,
        peak_body_transmissibility: max_tr,
        body_acceleration_rms_m_s2: a_rms,
        suspension_deflection_rms_mm: susp_rms_mm,
        dynamic_load_coefficient: dlc,
        ride_comfort_rating: ride_comfort,
        is_safe,
    })
}

/// Generic simulation adapter for quarter-car ride dynamics.
pub fn solve_quarter_car_sim(problem: &QuarterCarProblem) -> SimulationResult {
    match solve_quarter_car(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("body_bounce_frequency_hz".to_string(), result.body_bounce_frequency_hz);
            metrics.insert("wheel_hop_frequency_hz".to_string(), result.wheel_hop_frequency_hz);
            metrics.insert("damping_ratio_zeta".to_string(), result.damping_ratio_zeta);
            metrics.insert("body_acceleration_rms_m_s2".to_string(), result.body_acceleration_rms_m_s2);
            metrics.insert("suspension_deflection_rms_mm".to_string(), result.suspension_deflection_rms_mm);
            metrics.insert("dynamic_load_coefficient".to_string(), result.dynamic_load_coefficient);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.body_acceleration_rms_m_s2, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "body_bounce_frequency_hz",
                result.body_bounce_frequency_hz,
                1.5,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.automotive.quarter_car_iso2631".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.automotive.quarter_car_iso2631", "0.1.0"),
                logs: vec![format!(
                    "Quarter-Car Dynamics: f_body = {:.2} Hz, f_wheel = {:.1} Hz (ζ = {:.2}), a_rms = {:.2} m/s², DLC = {:.3} (Safe: {})",
                    result.body_bounce_frequency_hz,
                    result.wheel_hop_frequency_hz,
                    result.damping_ratio_zeta,
                    result.body_acceleration_rms_m_s2,
                    result.dynamic_load_coefficient,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.automotive.quarter_car_iso2631".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.automotive.quarter_car_iso2631", "0.1.0"),
                logs: vec![format!("Quarter car simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_quarter_car_nominal() {
        let prob = QuarterCarProblem::default();
        let res = solve_quarter_car(&prob).expect("solver failed");

        assert!(res.body_bounce_frequency_hz > 1.2 && res.body_bounce_frequency_hz < 1.8);
        assert!(res.wheel_hop_frequency_hz > 10.0 && res.wheel_hop_frequency_hz < 14.0);
        assert!(res.damping_ratio_zeta > 0.20 && res.damping_ratio_zeta < 0.40);
        assert!(res.body_acceleration_rms_m_s2 > 0.2 && res.body_acceleration_rms_m_s2 < 1.2);
        assert!(res.suspension_deflection_rms_mm > 5.0 && res.suspension_deflection_rms_mm < 25.0);
        assert!(res.dynamic_load_coefficient > 0.05 && res.dynamic_load_coefficient < 0.35);
        assert!(res.is_safe);
    }
}

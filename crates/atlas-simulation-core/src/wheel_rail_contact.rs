//! Wheel-Rail Contact Mechanics & Track Hunting Stability (Kalker / Hertz / Nadal / UIC 518).
//!
//! Evaluates 3D Hertzian elliptical contact patch geometry, peak contact normal pressure,
//! Kalker linear longitudinal and lateral creep coefficients, Klingel kinematic hunting instability frequency,
//! and Nadal wheel climb derailment safety quotient (Y/Q).

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Standard UIC 60 / 60E1 steel rail profile properties.
pub const E_STEEL_PA: f64 = 2.10e11;
/// Poisson's ratio for wheel and rail steel.
pub const POISSON_STEEL: f64 = 0.30;
/// Shear modulus of steel G in Pascals.
pub const G_STEEL_PA: f64 = E_STEEL_PA / (2.0 * (1.0 + POISSON_STEEL));

/// Wheel-rail contact problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelRailProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Static nominal wheel vertical load in kiloNewtons (kN) (e.g. 100.0 kN = 10 tonnes axle load per wheel).
    pub wheel_vertical_load_kn: f64,
    /// Nominal wheel tread radius in millimeters (mm) (e.g. 460.0 mm for 920 mm diameter wheel).
    pub wheel_radius_mm: f64,
    /// Transverse rail crown radius of curvature in millimeters (mm) (e.g. 300.0 mm for UIC 60).
    pub rail_crown_radius_mm: f64,
    /// Forward train travel speed in kilometers per hour (km/h) (e.g. 200.0 km/h).
    pub train_speed_km_h: f64,
    /// Effective wheel tread conicity angle in radians (e.g. 0.025 rad for 1:40 standard cone).
    #[serde(default = "default_conicity")]
    pub effective_conicity_rad: f64,
    /// Wheelset rolling circle lateral semi-spacing in meters (e.g. 0.750 m for 1435 mm standard gauge).
    #[serde(default = "default_track_gauge_half_m")]
    pub track_gauge_half_m: f64,
    /// Wheel flange contact angle in degrees (deg) (e.g. 70.0° for UIC standard flange).
    #[serde(default = "default_flange_angle_deg")]
    pub flange_contact_angle_deg: f64,
    /// Wheel-rail steel-on-steel friction coefficient (e.g. 0.35 under dry clean rail).
    #[serde(default = "default_friction_coeff")]
    pub friction_coefficient: f64,
    /// Applied lateral guiding track force Y in kiloNewtons (kN) (e.g. 35.0 kN).
    #[serde(default = "default_applied_lateral_kn")]
    pub applied_lateral_force_kn: f64,
}

fn default_problem_id() -> String {
    "wheel_rail_contact_problem".to_string()
}

fn default_conicity() -> f64 {
    0.025
}

fn default_track_gauge_half_m() -> f64 {
    0.750
}

fn default_flange_angle_deg() -> f64 {
    70.0
}

fn default_friction_coeff() -> f64 {
    0.35
}

fn default_applied_lateral_kn() -> f64 {
    35.0
}

impl Default for WheelRailProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            wheel_vertical_load_kn: 100.0,
            wheel_radius_mm: 460.0,
            rail_crown_radius_mm: 300.0,
            train_speed_km_h: 200.0,
            effective_conicity_rad: 0.025,
            track_gauge_half_m: 0.750,
            flange_contact_angle_deg: 70.0,
            friction_coefficient: 0.35,
            applied_lateral_force_kn: 35.0,
        }
    }
}

/// Wheel-rail contact simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WheelRailResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Contact ellipse longitudinal semi-axis a in millimeters (mm).
    pub contact_semi_axis_a_mm: f64,
    /// Contact ellipse transverse semi-axis b in millimeters (mm).
    pub contact_semi_axis_b_mm: f64,
    /// Contact patch surface area in square millimeters (mm²).
    pub contact_patch_area_mm2: f64,
    /// Maximum Hertzian contact normal pressure p0 in MegaPascals (MPa).
    pub max_hertz_pressure_mpa: f64,
    /// Mean contact normal pressure in MegaPascals (MPa).
    pub mean_contact_pressure_mpa: f64,
    /// Kalker linear longitudinal creep coefficient f11 in MegaNewtons (MN).
    pub kalker_f11_mn: f64,
    /// Kalker linear lateral creep coefficient f22 in MegaNewtons (MN).
    pub kalker_f22_mn: f64,
    /// Klingel kinematic hunting oscillation frequency in Hertz (Hz).
    pub klingel_hunting_frequency_hz: f64,
    /// Klingel hunting wavelength along track in meters (m).
    pub hunting_wavelength_m: f64,
    /// Applied lateral-to-vertical force ratio (Y/Q)_applied.
    pub applied_y_q_ratio: f64,
    /// Limiting Nadal wheel-climb derailment safety quotient (Y/Q)_limit.
    pub nadal_limit_y_q: f64,
    /// Derailment safety margin factor (Nadal_limit / Applied_Y_Q).
    pub derailment_safety_factor: f64,
    /// Whether operating point is safe against flange climb derailment (SF >= 1.20).
    pub is_derailment_safe: bool,
    /// Whether contact stress is within rolling contact fatigue (RCF) shakedown limit (p0 <= 1500 MPa).
    pub is_contact_stress_safe: bool,
}

/// Solves wheel-rail Hertzian contact, Kalker creepage, Klingel hunting, and Nadal derailment.
pub fn solve_wheel_rail_contact(problem: &WheelRailProblem) -> Result<WheelRailResult, String> {
    if problem.wheel_vertical_load_kn <= 0.0
        || problem.wheel_radius_mm <= 0.0
        || problem.rail_crown_radius_mm <= 0.0
    {
        return Err("Load and radii must be positive".to_string());
    }
    if problem.train_speed_km_h <= 0.0 {
        return Err("Train speed must be positive".to_string());
    }
    if problem.effective_conicity_rad <= 0.0 || problem.track_gauge_half_m <= 0.0 {
        return Err("Conicity and track gauge must be positive".to_string());
    }
    if problem.flange_contact_angle_deg <= 0.0 || problem.flange_contact_angle_deg >= 90.0 {
        return Err("Flange angle must be between 0 and 90 degrees".to_string());
    }

    let q_n = problem.wheel_vertical_load_kn * 1000.0;
    let r_w_m = problem.wheel_radius_mm * 1e-3;
    let r_r_m = problem.rail_crown_radius_mm * 1e-3;

    // 1. 3D Hertzian Contact Formulation:
    // Curvature sum: 1/R_eq = 1/r_w + 1/r_r
    // Principal relative curvatures:
    // A = 1 / (2 * r_r), B = 1 / (2 * r_w)
    let a_curv = 1.0 / (2.0 * r_r_m);
    let b_curv = 1.0 / (2.0 * r_w_m);
    let sum_curv = a_curv + b_curv;

    // Equivalent Young's modulus E*:
    // 1/E* = (1 - nu1^2)/E1 + (1 - nu2^2)/E2 = 2 * (1 - nu^2) / E
    let e_star = E_STEEL_PA / (2.0 * (1.0 - POISSON_STEEL.powi(2)));

    // Hertz elliptical semi-axes coefficients (rational approximation based on theta = acos((A-B)/(A+B))):
    let cos_theta = ((a_curv - b_curv) / sum_curv).abs();
    // For typical rail/wheel ratio ~ 0.65 (A/B ~ 1.5):
    let m_hertz = 1.15 + 0.40 * cos_theta;
    let n_hertz = 0.88 - 0.25 * cos_theta;

    let scale_factor = (3.0 * q_n / (4.0 * e_star * sum_curv)).powf(1.0 / 3.0);
    let a_semi_m = m_hertz * scale_factor;
    let b_semi_m = n_hertz * scale_factor;

    let a_semi_mm = a_semi_m * 1000.0;
    let b_semi_mm = b_semi_m * 1000.0;
    let patch_area_mm2 = std::f64::consts::PI * a_semi_mm * b_semi_mm;

    // Peak Hertzian contact pressure p0 [Pa] -> [MPa]:
    // p0 = 1.5 * Q / (pi * a * b)
    let p0_pa = 1.50 * q_n / (std::f64::consts::PI * a_semi_m * b_semi_m);
    let p0_mpa = p0_pa * 1e-6;
    let p_mean_mpa = (2.0 / 3.0) * p0_mpa;

    // 2. Kalker Linear Creep Coefficients (Kalker's simplified theory):
    // f11 = C11 * G * a * b [N]
    // f22 = C22 * G * a * b [N]
    // For rail steel ellipse (a/b ~ 1.3 - 1.8), C11 ~ 4.2, C22 ~ 3.6
    let ab_m2 = a_semi_m * b_semi_m;
    let f11_n = 4.25 * G_STEEL_PA * ab_m2;
    let f22_n = 3.65 * G_STEEL_PA * ab_m2;
    let f11_mn = f11_n * 1e-6;
    let f22_mn = f22_n * 1e-6;

    // 3. Klingel Kinematic Hunting Instability:
    // Wavelength: lambda = 2 * pi * sqrt(r_w * b0 / gamma_e) [m]
    let gamma_e = problem.effective_conicity_rad;
    let b0 = problem.track_gauge_half_m;
    let wavelength_m = 2.0 * std::f64::consts::PI * (r_w_m * b0 / gamma_e).sqrt();

    // Speed in m/s:
    let v_m_s = problem.train_speed_km_h / 3.60;
    let hunting_freq_hz = v_m_s / wavelength_m;

    // 4. Nadal Wheel-Climb Derailment Criterion:
    // (Y/Q)_Nadal = (tan(delta) - mu) / (1 + mu * tan(delta))
    let delta_rad = problem.flange_contact_angle_deg.to_radians();
    let mu = problem.friction_coefficient;
    let tan_delta = delta_rad.tan();
    let nadal_limit = (tan_delta - mu) / (1.0 + mu * tan_delta);

    let y_applied = problem.applied_lateral_force_kn;
    let q_applied = problem.wheel_vertical_load_kn;
    let y_q_applied = y_applied / q_applied;

    let sf_derail = nadal_limit / y_q_applied;
    let is_derail_safe = sf_derail >= 1.20;
    let is_stress_safe = p0_mpa <= 1500.0; // standard RCF shakedown limit for rail steel

    Ok(WheelRailResult {
        problem_id: problem.problem_id.clone(),
        contact_semi_axis_a_mm: a_semi_mm,
        contact_semi_axis_b_mm: b_semi_mm,
        contact_patch_area_mm2: patch_area_mm2,
        max_hertz_pressure_mpa: p0_mpa,
        mean_contact_pressure_mpa: p_mean_mpa,
        kalker_f11_mn: f11_mn,
        kalker_f22_mn: f22_mn,
        klingel_hunting_frequency_hz: hunting_freq_hz,
        hunting_wavelength_m: wavelength_m,
        applied_y_q_ratio: y_q_applied,
        nadal_limit_y_q: nadal_limit,
        derailment_safety_factor: sf_derail,
        is_derailment_safe: is_derail_safe,
        is_contact_stress_safe: is_stress_safe,
    })
}

/// Executes wheel-rail contact simulation and packages into SimulationResult envelope.
pub fn solve_wheel_rail_contact_sim(problem: &WheelRailProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_wheel_rail_contact(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "contact_semi_axis_a_mm".to_string(),
                res.contact_semi_axis_a_mm,
            );
            metrics.insert(
                "contact_semi_axis_b_mm".to_string(),
                res.contact_semi_axis_b_mm,
            );
            metrics.insert(
                "max_hertz_pressure_mpa".to_string(),
                res.max_hertz_pressure_mpa,
            );
            metrics.insert("kalker_f11_mn".to_string(), res.kalker_f11_mn);
            metrics.insert("kalker_f22_mn".to_string(), res.kalker_f22_mn);
            metrics.insert(
                "klingel_hunting_frequency_hz".to_string(),
                res.klingel_hunting_frequency_hz,
            );
            metrics.insert("nadal_limit_y_q".to_string(), res.nadal_limit_y_q);
            metrics.insert(
                "derailment_safety_factor".to_string(),
                res.derailment_safety_factor,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.max_hertz_pressure_mpa, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "max_hertz_pressure_mpa",
                res.max_hertz_pressure_mpa,
                1100.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.rail.wheel_rail_contact".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.rail.wheel_rail_contact", "0.1.0"),
                logs: vec![format!(
                    "Wheel-Rail Contact (Kalker / Hertz / Nadal): Patch = {:.2} x {:.2} mm ({:.1} mm²), p0 = {:.1} MPa, f11 = {:.2} MN, f22 = {:.2} MN, Klingel f_h = {:.2} Hz (λ = {:.1} m @ {:.0} km/h), (Y/Q)_app = {:.2}, Nadal Limit = {:.2} (SF = {:.2}), Safe: {}",
                    res.contact_semi_axis_a_mm,
                    res.contact_semi_axis_b_mm,
                    res.contact_patch_area_mm2,
                    res.max_hertz_pressure_mpa,
                    res.kalker_f11_mn,
                    res.kalker_f22_mn,
                    res.klingel_hunting_frequency_hz,
                    res.hunting_wavelength_m,
                    problem.train_speed_km_h,
                    res.applied_y_q_ratio,
                    res.nadal_limit_y_q,
                    res.derailment_safety_factor,
                    res.is_derailment_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.rail.wheel_rail_contact".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.rail.wheel_rail_contact", "0.1.0"),
                logs: vec![format!("Wheel-rail contact simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wheel_rail_nominal() {
        let prob = WheelRailProblem::default();
        let res = solve_wheel_rail_contact(&prob).expect("solver failed");
        println!(
            "Wheel-Rail: a = {:.2} mm, b = {:.2} mm, p0 = {:.1} MPa, f11 = {:.2} MN, f_h = {:.2} Hz, Nadal = {:.2}, SF = {:.2}",
            res.contact_semi_axis_a_mm,
            res.contact_semi_axis_b_mm,
            res.max_hertz_pressure_mpa,
            res.kalker_f11_mn,
            res.klingel_hunting_frequency_hz,
            res.nadal_limit_y_q,
            res.derailment_safety_factor
        );

        assert!(res.contact_semi_axis_a_mm > 5.0 && res.contact_semi_axis_a_mm < 12.0);
        assert!(res.contact_semi_axis_b_mm > 4.0 && res.contact_semi_axis_b_mm < 9.0);
        assert!(res.max_hertz_pressure_mpa > 800.0 && res.max_hertz_pressure_mpa < 1400.0);
        assert!(res.kalker_f11_mn > 10.0 && res.kalker_f11_mn < 25.0);
        assert!(res.klingel_hunting_frequency_hz > 1.5 && res.klingel_hunting_frequency_hz < 3.5);
        assert!(res.nadal_limit_y_q > 1.0 && res.nadal_limit_y_q < 1.5);
        assert!(res.derailment_safety_factor >= 1.20);
        assert!(res.is_derailment_safe);
        assert!(res.is_contact_stress_safe);
    }
}

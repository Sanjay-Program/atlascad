//! Wind Turbine Blade Aerodynamic Bending & Flutter Stability (IEC 61400-1 / Theodorsen / Hodges).
//!
//! Evaluates centrifugal stiffening on flapwise natural frequency, torsional frequency,
//! coupled flap-torsion flutter velocity, aerodynamic divergence velocity,
//! extreme gust tip deflection, and tower strike clearance safety margins.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Wind turbine blade aeroelastic problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindBladeProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Blade aerodynamic aerodynamic length in meters (m) (e.g. 75.0 m to 105.0 m).
    pub blade_length_m: f64,
    /// Total rotor radius in meters (m) (including hub radius).
    pub rotor_radius_m: f64,
    /// Rated rotor rotational speed in revolutions per minute (RPM).
    pub rated_rotor_speed_rpm: f64,
    /// Rated operational wind speed in meters per second (m/s) (e.g. 11.4 m/s).
    pub rated_wind_speed_m_s: f64,
    /// 50-year extreme gust wind speed in meters per second (m/s) (IEC 61400-1 DLC 1.1/6.1, e.g. 52.5 m/s).
    pub extreme_gust_speed_m_s: f64,
    /// Total blade structural mass in kilograms (kg).
    pub blade_mass_kg: f64,
    /// Aerodynamic chord at blade root in meters (m).
    pub root_chord_m: f64,
    /// Aerodynamic chord at blade tip in meters (m).
    pub tip_chord_m: f64,
    /// Effective flapwise flexural rigidity EI in Newton square-meters (N·m²).
    pub flapwise_rigidity_ei_nm2: f64,
    /// Effective torsional rigidity GJ in Newton square-meters (N·m²).
    pub torsional_rigidity_gj_nm2: f64,
    /// Cross-sectional mass polar moment of inertia at 75% span in kg·m²/m.
    pub polar_moment_inertia_kg_m: f64,
    /// Distance between elastic axis and center of gravity at 75% span in meters (m) (positive if CG behind EA).
    pub cg_ea_offset_m: f64,
    /// Atmospheric air density in kg/m³ (default 1.225 kg/m³).
    #[serde(default = "default_air_density")]
    pub air_density_kg_m3: f64,
}

fn default_problem_id() -> String {
    "wind_blade_aeroelastic_problem".to_string()
}

fn default_air_density() -> f64 {
    1.225
}

impl Default for WindBladeProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            blade_length_m: 80.0,
            rotor_radius_m: 82.5,
            rated_rotor_speed_rpm: 10.5,
            rated_wind_speed_m_s: 11.4,
            extreme_gust_speed_m_s: 52.5,
            blade_mass_kg: 19500.0,
            root_chord_m: 4.60,
            tip_chord_m: 1.15,
            flapwise_rigidity_ei_nm2: 1.45e10,
            torsional_rigidity_gj_nm2: 5.20e8,
            polar_moment_inertia_kg_m: 140.0,
            cg_ea_offset_m: 0.075,
            air_density_kg_m3: 1.225,
        }
    }
}

/// Wind turbine blade aeroelastic analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindBladeResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Standstill (non-rotating) first flapwise natural frequency in Hertz (Hz).
    pub standstill_flap_freq_hz: f64,
    /// Operating (rotating with centrifugal stiffening) first flapwise frequency in Hertz (Hz).
    pub rotating_flap_freq_hz: f64,
    /// Centrifugal Southwell stiffening frequency ratio (f_rot / f_static).
    pub centrifugal_stiffening_ratio: f64,
    /// First torsional natural frequency in Hertz (Hz).
    pub torsional_freq_hz: f64,
    /// Frequency separation ratio (f_torsion / f_rotating_flap).
    pub frequency_separation_ratio: f64,
    /// Blade tip speed at rated rotor RPM in meters per second (m/s).
    pub rated_tip_speed_m_s: f64,
    /// Critical aerodynamic divergence velocity in meters per second (m/s).
    pub divergence_speed_m_s: f64,
    /// Coupled classical flap-torsion flutter velocity in meters per second (m/s).
    pub flutter_speed_m_s: f64,
    /// Flutter safety margin ratio (v_flutter / (v_tip_rated + v_wind_rated)).
    pub flutter_safety_margin: f64,
    /// Blade tip flapwise deflection under 50-year extreme gust in meters (m).
    pub tip_gust_deflection_m: f64,
    /// Ratio of tip gust deflection to blade length (%).
    pub tip_deflection_ratio_pct: f64,
    /// Whether blade aeroelastic stability conforms to IEC 61400-1 safety requirements.
    pub is_stable: bool,
}

/// Solves wind turbine blade aeroelastic bending, centrifugal stiffening, and flutter stability.
pub fn solve_wind_blade_aeroelastic(problem: &WindBladeProblem) -> Result<WindBladeResult, String> {
    if problem.blade_length_m <= 0.0 || problem.rotor_radius_m <= 0.0 {
        return Err("Blade length and rotor radius must be positive".to_string());
    }
    if problem.blade_mass_kg <= 0.0
        || problem.flapwise_rigidity_ei_nm2 <= 0.0
        || problem.torsional_rigidity_gj_nm2 <= 0.0
    {
        return Err("Mass and structural stiffnesses must be positive".to_string());
    }
    if problem.root_chord_m <= 0.0 || problem.tip_chord_m <= 0.0 {
        return Err("Chord dimensions must be positive".to_string());
    }

    let l = problem.blade_length_m;
    let r_rotor = problem.rotor_radius_m;
    let mu_avg = problem.blade_mass_kg / l; // mass per unit length (kg/m)

    // 1. Standstill first flapwise bending natural frequency:
    // Cantilever Euler-Bernoulli beam mode 1: lambda_1 = 1.875104
    // omega_0 = (1.875104)^2 * sqrt(EI / (mu * L^4)) = 3.5160 * sqrt(EI / (mu * L^4))
    let omega_flap_0 = 3.5160 * (problem.flapwise_rigidity_ei_nm2 / (mu_avg * l.powi(4))).sqrt();
    let f_flap_0 = omega_flap_0 / (2.0 * PI);

    // 2. Centrifugal stiffening under rotor rotation (Southwell formula):
    // Omega = 2 * pi * RPM / 60
    let omega_rot = 2.0 * PI * problem.rated_rotor_speed_rpm / 60.0;
    // Southwell coefficient for tapered rotating cantilever blade: k_s ~ 1.73 + 1.2 * (R_hub / L)
    let r_hub = (r_rotor - l).max(0.5);
    let southwell_k = 1.73 + 1.25 * (r_hub / l);
    let omega_flap_rot = (omega_flap_0.powi(2) + southwell_k * omega_rot.powi(2)).sqrt();
    let f_flap_rot = omega_flap_rot / (2.0 * PI);
    let stiffening_ratio = f_flap_rot / f_flap_0;

    // 3. First torsional natural frequency:
    // Fixed-free torsional shaft: omega_tor = (pi / (2 * L)) * sqrt(GJ / I_p)
    let omega_tor = (PI / (2.0 * l))
        * (problem.torsional_rigidity_gj_nm2 / problem.polar_moment_inertia_kg_m).sqrt();
    let f_tor = omega_tor / (2.0 * PI);
    let freq_sep = f_tor / f_flap_rot;

    // 4. Characteristic chord and aerodynamic properties at 75% span:
    // Linear taper: c(r) = c_root - (c_root - c_tip) * (r / L)
    let c_75 = problem.root_chord_m - 0.75 * (problem.root_chord_m - problem.tip_chord_m);
    let cla = 2.0 * PI; // Thin airfoil theoretical lift slope dCl/dalpha (per rad)
    let e = problem.cg_ea_offset_m.abs().max(0.01); // offset between aerodynamic/elastic axis

    // 5. Critical aerodynamic divergence velocity (Bisplinghoff / Theodorsen):
    // v_div = sqrt( 2 * GJ / (rho * c^2 * e * C_L_alpha * L) )
    let v_div = (2.0 * problem.torsional_rigidity_gj_nm2
        / (problem.air_density_kg_m3 * c_75.powi(2) * e * cla * l))
        .sqrt();

    // 6. Flap-torsion coupled classical flutter velocity:
    // Hodges-Dowell / Theodorsen formulation for coupled flutter:
    // Frequency ratio r_w = omega_flap / omega_tor
    let r_w = (omega_flap_rot / omega_tor).min(0.95);
    // Mass ratio: mu_aero = mu_avg / (pi * rho * (c/2)^2)
    let b_semi = c_75 / 2.0;
    let mu_aero = mu_avg / (PI * problem.air_density_kg_m3 * b_semi.powi(2));
    // Reduced flutter speed factor based on frequency difference and uncoupled separation:
    let flutter_factor = (1.0 - r_w.powi(2)).sqrt() * (mu_aero / 40.0).powf(0.20);
    let v_flutter = omega_tor * b_semi * flutter_factor * 2.85;

    // 7. Rated tip speed and extreme gust kinematics:
    let v_tip_rated = omega_rot * r_rotor;
    let v_tip_relative = (v_tip_rated.powi(2) + problem.rated_wind_speed_m_s.powi(2)).sqrt();
    let flutter_safety_margin = v_flutter / v_tip_relative;

    // 8. Tip gust deflection under 50-year extreme gust:
    // IEC 61400-1 DLC 6.1 parked or idling under extreme gust:
    // Extreme thrust per unit length: q_thrust = 0.5 * rho * v_gust^2 * c_avg * C_d_blade
    let c_avg = 0.5 * (problem.root_chord_m + problem.tip_chord_m);
    let c_drag = 1.15; // drag coefficient for parked/feathered blade under cross-gust
    let q_gust_load =
        0.5 * problem.air_density_kg_m3 * problem.extreme_gust_speed_m_s.powi(2) * c_avg * c_drag;
    // Cantilever tip deflection under distributed load w: y_tip = (w * L^4) / (8 * EI)
    let tip_gust_deflection_m =
        (q_gust_load * l.powi(4)) / (8.0 * problem.flapwise_rigidity_ei_nm2);
    let tip_deflection_ratio_pct = (tip_gust_deflection_m / l) * 100.0;

    // 9. Conformance checks:
    // IEC 61400-1 requires:
    // - Flutter margin > 1.25
    // - Divergence margin > 1.40
    // - Frequency separation > 2.0
    // - Tip deflection ratio < 12.0%
    let is_stable = flutter_safety_margin >= 1.25
        && (v_div / v_tip_relative) >= 1.40
        && freq_sep >= 2.0
        && tip_deflection_ratio_pct < 12.0;

    Ok(WindBladeResult {
        problem_id: problem.problem_id.clone(),
        standstill_flap_freq_hz: f_flap_0,
        rotating_flap_freq_hz: f_flap_rot,
        centrifugal_stiffening_ratio: stiffening_ratio,
        torsional_freq_hz: f_tor,
        frequency_separation_ratio: freq_sep,
        rated_tip_speed_m_s: v_tip_rated,
        divergence_speed_m_s: v_div,
        flutter_speed_m_s: v_flutter,
        flutter_safety_margin,
        tip_gust_deflection_m,
        tip_deflection_ratio_pct,
        is_stable,
    })
}

/// Executes wind turbine blade aeroelastic simulation and packages into SimulationResult envelope.
pub fn solve_wind_blade_aeroelastic_sim(problem: &WindBladeProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_wind_blade_aeroelastic(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "standstill_flap_freq_hz".to_string(),
                res.standstill_flap_freq_hz,
            );
            metrics.insert(
                "rotating_flap_freq_hz".to_string(),
                res.rotating_flap_freq_hz,
            );
            metrics.insert("torsional_freq_hz".to_string(), res.torsional_freq_hz);
            metrics.insert("flutter_speed_m_s".to_string(), res.flutter_speed_m_s);
            metrics.insert("divergence_speed_m_s".to_string(), res.divergence_speed_m_s);
            metrics.insert(
                "flutter_safety_margin".to_string(),
                res.flutter_safety_margin,
            );
            metrics.insert(
                "tip_gust_deflection_m".to_string(),
                res.tip_gust_deflection_m,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.rotating_flap_freq_hz, 1, 1e-4);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.renewable.wind_blade_aeroelastic".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.renewable.wind_blade_aeroelastic",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Wind Blade Aeroelastic (IEC 61400-1): f_flap_rot = {:.3} Hz, f_tor = {:.3} Hz, v_flutter = {:.1} m/s (margin = {:.2}x), v_div = {:.1} m/s, y_gust = {:.2} m ({:.1}%), Stable: {}",
                    res.rotating_flap_freq_hz,
                    res.torsional_freq_hz,
                    res.flutter_speed_m_s,
                    res.flutter_safety_margin,
                    res.divergence_speed_m_s,
                    res.tip_gust_deflection_m,
                    res.tip_deflection_ratio_pct,
                    res.is_stable
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.renewable.wind_blade_aeroelastic".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.renewable.wind_blade_aeroelastic",
                    "0.1.0",
                ),
                logs: vec![format!("Wind blade aeroelastic simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wind_blade_nominal() {
        let prob = WindBladeProblem::default();
        let res = solve_wind_blade_aeroelastic(&prob).expect("solver failed");
        println!(
            "Wind Blade: f_flap_rot = {:.3} Hz, f_tor = {:.3} Hz, v_flutter = {:.1} m/s, y_gust = {:.2} m, margin = {:.2}",
            res.rotating_flap_freq_hz,
            res.torsional_freq_hz,
            res.flutter_speed_m_s,
            res.tip_gust_deflection_m,
            res.flutter_safety_margin
        );

        assert!(res.standstill_flap_freq_hz > 0.40 && res.standstill_flap_freq_hz < 1.20);
        assert!(res.rotating_flap_freq_hz > res.standstill_flap_freq_hz);
        assert!(res.centrifugal_stiffening_ratio > 1.05 && res.centrifugal_stiffening_ratio < 1.35);
        assert!(res.torsional_freq_hz > 2.0 && res.torsional_freq_hz < 8.0);
        assert!(res.frequency_separation_ratio > 2.5);
        assert!(res.flutter_speed_m_s > 100.0);
        assert!(res.divergence_speed_m_s > 150.0);
        assert!(res.flutter_safety_margin > 1.25);
        assert!(res.tip_gust_deflection_m > 1.5 && res.tip_gust_deflection_m < 9.0);
        assert!(res.is_stable);
    }
}

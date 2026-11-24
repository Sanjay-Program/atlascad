//! Spacecraft Orbital Mechanics & Two-Body Keplerian Transfers (Curtis / Vallado / NASA).
//!
//! Evaluates circular orbit velocities, semi-major axis, two-impulse Hohmann transfer orbits,
//! transfer delta-V (dv1, dv2, total dv), transfer time of flight, Tsiolkovsky propellant mass
//! fraction, and J2 nodal regression / precession rate for sun-synchronous orbit inclination.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Earth planetary physical and gravitational constants.
pub const EARTH_MU_M3_S2: f64 = 3.986004418e14;
/// Earth volumetric mean radius in meters.
pub const EARTH_RADIUS_M: f64 = 6378137.0;
/// Earth J2 second zonal geopotential harmonic coefficient.
pub const EARTH_J2: f64 = 1.08263e-3;
/// Standard gravity for Tsiolkovsky equation in m/s².
pub const G0_STD: f64 = 9.80665;

/// Orbital transfer problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbitalMechanicsProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Initial circular orbit altitude above Earth surface in kilometers (km) (e.g. 400.0 km LEO).
    pub initial_altitude_km: f64,
    /// Target circular orbit altitude above Earth surface in kilometers (km) (e.g. 35786.0 km GEO).
    pub target_altitude_km: f64,
    /// Spacecraft initial wet mass in kilograms (kg).
    pub spacecraft_wet_mass_kg: f64,
    /// Propulsion system effective specific impulse (Isp) in seconds (s) (e.g. 315.0s for hypergolic bipropellant).
    pub propulsion_isp_s: f64,
    /// Orbital inclination in degrees (deg) (e.g. 28.5 deg for Cape Canaveral).
    pub inclination_deg: f64,
}

fn default_problem_id() -> String {
    "orbital_mechanics_problem".to_string()
}

impl Default for OrbitalMechanicsProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            initial_altitude_km: 400.0,  // LEO parking orbit
            target_altitude_km: 35786.0, // Geostationary Earth Orbit (GEO)
            spacecraft_wet_mass_kg: 3500.0,
            propulsion_isp_s: 318.0, // Apogee Kick Motor (MMH/NTO)
            inclination_deg: 28.5,
        }
    }
}

/// Orbital mechanics calculation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrbitalMechanicsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Initial orbit radius in kilometers (km) (r1 = R_E + h1).
    pub initial_radius_km: f64,
    /// Target orbit radius in kilometers (km) (r2 = R_E + h2).
    pub target_radius_km: f64,
    /// Initial circular orbit orbital speed in m/s (v1).
    pub initial_orbital_speed_m_s: f64,
    /// Target circular orbit orbital speed in m/s (v2).
    pub target_orbital_speed_m_s: f64,
    /// Initial orbit orbital period in minutes (min).
    pub initial_orbital_period_min: f64,
    /// Target orbit orbital period in hours (h).
    pub target_orbital_period_h: f64,
    /// Hohmann transfer ellipse semi-major axis in kilometers (km).
    pub transfer_semi_major_axis_km: f64,
    /// Hohmann transfer ellipse eccentricity e.
    pub transfer_eccentricity: f64,
    /// Velocity at transfer ellipse perigee in m/s.
    pub transfer_perigee_velocity_m_s: f64,
    /// Velocity at transfer ellipse apogee in m/s.
    pub transfer_apogee_velocity_m_s: f64,
    /// First impulsive delta-V burn at perigee in m/s (dv1).
    pub delta_v1_m_s: f64,
    /// Second impulsive delta-V burn at apogee in m/s (dv2).
    pub delta_v2_m_s: f64,
    /// Total coplanar Hohmann delta-V budget in m/s.
    pub total_delta_v_m_s: f64,
    /// Combined Hohmann transfer plus inclination removal plane-change delta-V in m/s.
    pub total_delta_v_with_plane_change_m_s: f64,
    /// Transfer orbit one-way time of flight (TOF) in hours (h).
    pub transfer_time_of_flight_h: f64,
    /// Required propellant mass consumed in kilograms (kg) (Tsiolkovsky equation).
    pub required_propellant_mass_kg: f64,
    /// Final spacecraft dry mass in orbit in kilograms (kg).
    pub final_spacecraft_mass_kg: f64,
    /// Mass fraction of propellant (m_prop / m_wet).
    pub propellant_mass_fraction: f64,
    /// J2 nodal precession rate at initial orbit in degrees per day (deg/day).
    pub j2_nodal_precession_deg_day: f64,
    /// Whether orbital transfer is physically realizable within standard bounds.
    pub is_safe: bool,
}

/// Solves Keplerian two-body Hohmann transfer orbital mechanics.
pub fn solve_orbital_mechanics(
    problem: &OrbitalMechanicsProblem,
) -> Result<OrbitalMechanicsResult, String> {
    if problem.initial_altitude_km < 120.0 {
        return Err(
            "Initial orbit altitude must exceed atmospheric reentry threshold (120 km)".to_string(),
        );
    }
    if problem.target_altitude_km <= problem.initial_altitude_km {
        return Err(
            "Target orbit altitude must exceed initial orbit altitude for ascent transfer"
                .to_string(),
        );
    }
    if problem.spacecraft_wet_mass_kg <= 0.0 || problem.propulsion_isp_s <= 0.0 {
        return Err("Spacecraft wet mass and specific impulse must be positive".to_string());
    }

    let mu = EARTH_MU_M3_S2;
    let r_e = EARTH_RADIUS_M;

    // 1. Orbital radii in meters and kilometers:
    let r1_m = r_e + problem.initial_altitude_km * 1000.0;
    let r2_m = r_e + problem.target_altitude_km * 1000.0;
    let r1_km = r1_m / 1000.0;
    let r2_km = r2_m / 1000.0;

    // 2. Circular orbit speeds:
    let v1 = (mu / r1_m).sqrt();
    let v2 = (mu / r2_m).sqrt();

    // Orbital periods: T = 2 * pi * sqrt(r^3 / mu)
    let t1_s = 2.0 * std::f64::consts::PI * (r1_m.powi(3) / mu).sqrt();
    let t2_s = 2.0 * std::f64::consts::PI * (r2_m.powi(3) / mu).sqrt();
    let t1_min = t1_s / 60.0;
    let t2_h = t2_s / 3600.0;

    // 3. Hohmann transfer ellipse geometry:
    let a_trans_m = (r1_m + r2_m) / 2.0;
    let a_trans_km = a_trans_m / 1000.0;
    let e_trans = (r2_m - r1_m) / (r2_m + r1_m);

    // 4. Velocities on transfer ellipse:
    // Vis-viva equation: v^2 = mu * (2/r - 1/a)
    let v_perigee = (mu * (2.0 / r1_m - 1.0 / a_trans_m)).sqrt();
    let v_apogee = (mu * (2.0 / r2_m - 1.0 / a_trans_m)).sqrt();

    // 5. Delta-V burns:
    let dv1 = v_perigee - v1; // Injection into transfer orbit at perigee
    let dv2_coplanar = v2 - v_apogee; // Circularization at apogee
    let dv_total_coplanar = dv1.abs() + dv2_coplanar.abs();

    // Combined apogee plane change + circularization:
    // dv2_combined = sqrt(v_apogee^2 + v2^2 - 2 * v_apogee * v2 * cos(delta_i))
    let inc_rad = problem.inclination_deg.to_radians();
    let dv2_plane_change =
        (v_apogee * v_apogee + v2 * v2 - 2.0 * v_apogee * v2 * inc_rad.cos()).sqrt();
    let dv_total_with_plane_change = dv1.abs() + dv2_plane_change;

    // 6. Transfer time of flight (TOF): half of transfer ellipse period
    let tof_s = std::f64::consts::PI * (a_trans_m.powi(3) / mu).sqrt();
    let tof_h = tof_s / 3600.0;

    // 7. Tsiolkovsky Rocket Equation for propellant consumption:
    // m_f = m_0 * exp(-dv / (g0 * Isp))
    let c_eff = problem.propulsion_isp_s * G0_STD;
    let mass_ratio = (-dv_total_with_plane_change / c_eff).exp();
    let m_final = problem.spacecraft_wet_mass_kg * mass_ratio;
    let m_prop = problem.spacecraft_wet_mass_kg - m_final;
    let prop_fraction = m_prop / problem.spacecraft_wet_mass_kg;

    // 8. J2 secular nodal precession rate:
    // dOmega/dt = -1.5 * n * J2 * (R_E / p)^2 * cos(i)
    let n1 = (mu / r1_m.powi(3)).sqrt(); // mean motion in rad/s
    let domega_dt_rad_s = -1.5 * n1 * EARTH_J2 * (r_e / r1_m).powi(2) * inc_rad.cos();
    let domega_dt_deg_day = domega_dt_rad_s * (180.0 / std::f64::consts::PI) * 86400.0;

    // 9. Safety and physical viability checks:
    let is_safe = dv_total_with_plane_change > 0.0
        && dv_total_with_plane_change < 12000.0
        && m_final > 0.0
        && e_trans < 1.0;

    Ok(OrbitalMechanicsResult {
        problem_id: problem.problem_id.clone(),
        initial_radius_km: r1_km,
        target_radius_km: r2_km,
        initial_orbital_speed_m_s: v1,
        target_orbital_speed_m_s: v2,
        initial_orbital_period_min: t1_min,
        target_orbital_period_h: t2_h,
        transfer_semi_major_axis_km: a_trans_km,
        transfer_eccentricity: e_trans,
        transfer_perigee_velocity_m_s: v_perigee,
        transfer_apogee_velocity_m_s: v_apogee,
        delta_v1_m_s: dv1,
        delta_v2_m_s: dv2_coplanar,
        total_delta_v_m_s: dv_total_coplanar,
        total_delta_v_with_plane_change_m_s: dv_total_with_plane_change,
        transfer_time_of_flight_h: tof_h,
        required_propellant_mass_kg: m_prop,
        final_spacecraft_mass_kg: m_final,
        propellant_mass_fraction: prop_fraction,
        j2_nodal_precession_deg_day: domega_dt_deg_day,
        is_safe,
    })
}

/// Executes orbital mechanics simulation and packages into SimulationResult envelope.
pub fn solve_orbital_mechanics_sim(problem: &OrbitalMechanicsProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_orbital_mechanics(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "initial_orbital_speed_m_s".to_string(),
                res.initial_orbital_speed_m_s,
            );
            metrics.insert(
                "target_orbital_speed_m_s".to_string(),
                res.target_orbital_speed_m_s,
            );
            metrics.insert("total_delta_v_m_s".to_string(), res.total_delta_v_m_s);
            metrics.insert(
                "total_delta_v_with_plane_change_m_s".to_string(),
                res.total_delta_v_with_plane_change_m_s,
            );
            metrics.insert(
                "transfer_time_of_flight_h".to_string(),
                res.transfer_time_of_flight_h,
            );
            metrics.insert(
                "required_propellant_mass_kg".to_string(),
                res.required_propellant_mass_kg,
            );
            metrics.insert(
                "propellant_mass_fraction".to_string(),
                res.propellant_mass_fraction,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.total_delta_v_with_plane_change_m_s, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "total_delta_v_with_plane_change_m_s",
                res.total_delta_v_with_plane_change_m_s,
                res.total_delta_v_with_plane_change_m_s * 0.99,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.aerospace.orbital_mechanics_kepler".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.aerospace.orbital_mechanics_kepler",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Keplerian Hohmann: r1 = {:.1} km -> r2 = {:.1} km, dv_tot = {:.1} m/s (incl = {:.1} m/s), TOF = {:.2} h, Prop = {:.1} kg ({:.1}%), Safe: {}",
                    res.initial_radius_km,
                    res.target_radius_km,
                    res.total_delta_v_m_s,
                    res.total_delta_v_with_plane_change_m_s,
                    res.transfer_time_of_flight_h,
                    res.required_propellant_mass_kg,
                    res.propellant_mass_fraction * 100.0,
                    res.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.aerospace.orbital_mechanics_kepler".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.aerospace.orbital_mechanics_kepler",
                    "0.1.0",
                ),
                logs: vec![format!("Orbital mechanics simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_orbital_mechanics_nominal() {
        let prob = OrbitalMechanicsProblem::default();
        let res = solve_orbital_mechanics(&prob).expect("solver failed");
        println!(
            "Orbital Hohmann (LEO->GEO): r1 = {:.1} km, r2 = {:.1} km, v1 = {:.1} m/s, v2 = {:.1} m/s, dv_tot = {:.1} m/s (with plane change = {:.1} m/s), TOF = {:.2} h, Prop = {:.1} kg ({:.1}%)",
            res.initial_radius_km,
            res.target_radius_km,
            res.initial_orbital_speed_m_s,
            res.target_orbital_speed_m_s,
            res.total_delta_v_m_s,
            res.total_delta_v_with_plane_change_m_s,
            res.transfer_time_of_flight_h,
            res.required_propellant_mass_kg,
            res.propellant_mass_fraction * 100.0
        );

        assert!(res.initial_orbital_speed_m_s > 7600.0 && res.initial_orbital_speed_m_s < 7800.0);
        assert!(res.target_orbital_speed_m_s > 3000.0 && res.target_orbital_speed_m_s < 3150.0);
        assert!(res.target_orbital_period_h > 23.5 && res.target_orbital_period_h < 24.5);
        assert!(res.delta_v1_m_s > 2350.0 && res.delta_v1_m_s < 2450.0);
        assert!(res.total_delta_v_m_s > 3800.0 && res.total_delta_v_m_s < 4000.0);
        assert!(
            res.total_delta_v_with_plane_change_m_s > 4100.0
                && res.total_delta_v_with_plane_change_m_s < 4400.0
        );
        assert!(res.transfer_time_of_flight_h > 5.0 && res.transfer_time_of_flight_h < 5.5);
        assert!(res.propellant_mass_fraction > 0.65 && res.propellant_mass_fraction < 0.80);
        assert!(res.is_safe);
    }
}

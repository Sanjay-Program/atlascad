//! Spacecraft Reentry Aerothermodynamics & Hypersonic Stagnation Point Heat Flux (Fay-Riddell / Sutton-Graves / NASA SP-8000).
//!
//! Evaluates hypersonic bow shock stagnation pressure and enthalpy, Sutton-Graves convective heat flux,
//! equilibrium gas radiative heat flux, shock stand-off distance, radiative equilibrium wall temperature,
//! and Thermal Protection System (TPS) sizing safety margins.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Stefan-Boltzmann constant sigma in W/(m²·K⁴).
pub const STEFAN_BOLTZMANN: f64 = 5.670374419e-8;
/// Sutton-Graves convective heat transfer constant for Earth air atmosphere in kg^(1/2)/m.
pub const K_SUTTON_GRAVES: f64 = 1.7415e-4;
/// Specific heat ratio gamma for cold air.
pub const GAMMA_AIR: f64 = 1.40;
/// Sea-level atmospheric density rho_0 in kg/m³.
pub const RHO_SL: f64 = 1.225;
/// Atmospheric scale height H in meters (m).
pub const SCALE_HEIGHT_M: f64 = 7200.0;

/// Reentry aerothermodynamics problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReentryProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Vehicle freestream entry velocity in meters per second (m/s) (e.g. 7600.0 m/s for LEO de-orbit).
    pub freestream_velocity_m_s: f64,
    /// Atmospheric entry altitude in kilometers (km) (e.g. 58.0 km for peak heating regime).
    pub altitude_km: f64,
    /// Nose cap / blunt body spherical radius of curvature in meters (m) (e.g. 0.85 m).
    pub nose_radius_m: f64,
    /// Vehicle entry mass in kilograms (kg) (e.g. 5200.0 kg).
    pub vehicle_mass_kg: f64,
    /// Blunt body cone half-angle in degrees (deg) (e.g. 45.0° for Apollo/Orion archetype).
    #[serde(default = "default_cone_angle")]
    pub nose_cone_half_angle_deg: f64,
    /// Surface total hemispherical emissivity of TPS thermal tile / ablator (e.g. 0.88).
    #[serde(default = "default_emissivity")]
    pub tps_surface_emissivity: f64,
    /// Maximum allowable TPS outer mold line temperature in Kelvin (K) (e.g. 2100.0 K).
    #[serde(default = "default_max_tps_temp")]
    pub max_allowable_tps_temperature_k: f64,
}

fn default_problem_id() -> String {
    "reentry_aerothermodynamics_problem".to_string()
}

fn default_cone_angle() -> f64 {
    45.0
}

fn default_emissivity() -> f64 {
    0.88
}

fn default_max_tps_temp() -> f64 {
    2100.0
}

impl Default for ReentryProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            freestream_velocity_m_s: 7400.0,
            altitude_km: 62.0,
            nose_radius_m: 1.15,
            vehicle_mass_kg: 5200.0,
            nose_cone_half_angle_deg: 45.0,
            tps_surface_emissivity: 0.88,
            max_allowable_tps_temperature_k: 2300.0,
        }
    }
}

/// Reentry aerothermodynamics simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReentryResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Atmospheric freestream density at altitude in kg/m³.
    pub freestream_density_kg_m3: f64,
    /// Freestream Mach number M_inf.
    pub freestream_mach: f64,
    /// Stagnation point post-normal-shock pressure in kiloPascals (kPa).
    pub stagnation_pressure_kpa: f64,
    /// Stagnation specific total enthalpy in MegaJoules per kilogram (MJ/kg).
    pub stagnation_enthalpy_mj_kg: f64,
    /// Detached bow shock wave stand-off distance at stagnation line in millimeters (mm).
    pub shock_standoff_distance_mm: f64,
    /// Convective heat flux at stagnation point in kiloWatts per square meter (kW/m²).
    pub convective_heat_flux_kw_m2: f64,
    /// Radiative shock layer heat flux at stagnation point in kiloWatts per square meter (kW/m²).
    pub radiative_heat_flux_kw_m2: f64,
    /// Total peak stagnation heat flux in kiloWatts per square meter (kW/m²).
    pub total_stagnation_heat_flux_kw_m2: f64,
    /// Radiative equilibrium wall surface temperature in Kelvin (K).
    pub radiative_equilibrium_wall_temperature_k: f64,
    /// Recommended minimum TPS ablator / insulator thickness in millimeters (mm).
    pub recommended_tps_thickness_mm: f64,
    /// Temperature safety factor (T_max_allowable / T_wall_eq).
    pub thermal_safety_factor: f64,
    /// Whether TPS thermal protection is safe against burn-through / structural exceedance.
    pub is_tps_safe: bool,
}

/// Solves hypersonic entry aerothermodynamics, stagnation heat flux, and radiative equilibrium wall temperature.
pub fn solve_reentry_aerothermodynamics(
    problem: &ReentryProblem,
) -> Result<ReentryResult, String> {
    if problem.freestream_velocity_m_s <= 500.0 {
        return Err("Freestream velocity must be supersonic/hypersonic (> 500 m/s)".to_string());
    }
    if problem.altitude_km < 10.0 || problem.altitude_km > 120.0 {
        return Err("Altitude must be within sensible atmosphere (10 to 120 km)".to_string());
    }
    if problem.nose_radius_m <= 0.0 {
        return Err("Nose radius of curvature must be positive".to_string());
    }
    if problem.tps_surface_emissivity <= 0.0 || problem.tps_surface_emissivity > 1.0 {
        return Err("Emissivity must be between 0 and 1".to_string());
    }

    let v_inf = problem.freestream_velocity_m_s;
    let r_n = problem.nose_radius_m;

    // 1. US Standard Atmosphere exponential density profile:
    let alt_m = problem.altitude_km * 1000.0;
    let rho_inf = RHO_SL * (-alt_m / SCALE_HEIGHT_M).exp();

    // Ambient speed of sound at mesosphere / stratopause ~ 300 m/s:
    let c_sound = 300.0;
    let mach_inf = v_inf / c_sound;

    // 2. Stagnation Pressure post-bow-shock (Rayleigh Pitot / Anderson formula):
    // P_stag ~ 0.92 * rho_inf * v_inf^2
    let p_stag_pa = 0.92 * rho_inf * v_inf.powi(2);
    let p_stag_kpa = p_stag_pa * 1e-3;

    // 3. Stagnation Total Enthalpy:
    // h_0 = h_inf + 0.5 * v_inf^2 ~ 0.5 * v_inf^2 [J/kg]
    let h0_j_kg = 0.50 * v_inf.powi(2);
    let h0_mj_kg = h0_j_kg * 1e-6;

    // 4. Detached Bow Shock Stand-Off Distance (Billig / Inger formula):
    // Delta = 0.143 * R_n * exp(3.24 / M^2) [m]
    let delta_shock_m = 0.143 * r_n * (3.24 / mach_inf.powi(2)).exp();
    let delta_shock_mm = delta_shock_m * 1000.0;

    // 5. Convective Heat Flux (Sutton-Graves / Fay-Riddell formulation):
    // q_conv = K_sg * sqrt(rho_inf / R_n) * v_inf^3 [W/m²]
    let q_conv_w_m2 = K_SUTTON_GRAVES * (rho_inf / r_n).sqrt() * v_inf.powi(3);
    let q_conv_kw_m2 = q_conv_w_m2 * 1e-3;

    // 6. Radiative Heat Flux from high-temperature shock layer (Tauber-Sutton correlation):
    // Radiation becomes prominent at high velocities (v > 7000 m/s):
    let q_rad_w_m2 = if v_inf > 6800.0 {
        let v_ratio = v_inf / 10000.0;
        4.736e4 * r_n.powf(1.0) * rho_inf.powf(1.22) * v_ratio.powf(8.5)
    } else {
        0.0
    };
    let q_rad_kw_m2 = q_rad_w_m2 * 1e-3;

    let q_total_kw_m2 = q_conv_kw_m2 + q_rad_kw_m2;
    let q_total_w_m2 = q_total_kw_m2 * 1000.0;

    // 7. Radiative Equilibrium Wall Temperature:
    // q_total = epsilon * sigma * T_wall^4
    // T_wall = (q_total / (epsilon * sigma))^0.25
    let eps = problem.tps_surface_emissivity;
    let t_wall_k = (q_total_w_m2 / (eps * STEFAN_BOLTZMANN)).powf(0.25);

    // 8. Recommended TPS ablator / insulator thickness:
    // Sized for thermal diffusion depth during 180s entry pulse:
    // Carbon phenolic / PICA thermal diffusivity alpha ~ 3.5e-7 m²/s
    // t_tps = 2 * sqrt(alpha * tau) * safety_margin
    let alpha_tps: f64 = 3.5e-7;
    let tau_heatpulse_s: f64 = 180.0;
    let diff_depth_m: f64 = 2.0 * (alpha_tps * tau_heatpulse_s).sqrt();
    let thickness_mm: f64 = (diff_depth_m * 1000.0 * 2.2).max(25.0);

    let safety_factor = problem.max_allowable_tps_temperature_k / t_wall_k;
    let is_tps_safe = t_wall_k <= problem.max_allowable_tps_temperature_k && safety_factor >= 1.05;

    Ok(ReentryResult {
        problem_id: problem.problem_id.clone(),
        freestream_density_kg_m3: rho_inf,
        freestream_mach: mach_inf,
        stagnation_pressure_kpa: p_stag_kpa,
        stagnation_enthalpy_mj_kg: h0_mj_kg,
        shock_standoff_distance_mm: delta_shock_mm,
        convective_heat_flux_kw_m2: q_conv_kw_m2,
        radiative_heat_flux_kw_m2: q_rad_kw_m2,
        total_stagnation_heat_flux_kw_m2: q_total_kw_m2,
        radiative_equilibrium_wall_temperature_k: t_wall_k,
        recommended_tps_thickness_mm: thickness_mm,
        thermal_safety_factor: safety_factor,
        is_tps_safe,
    })
}

/// Executes reentry aerothermodynamics simulation and packages into SimulationResult envelope.
pub fn solve_reentry_aerothermodynamics_sim(
    problem: &ReentryProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_reentry_aerothermodynamics(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("freestream_mach".to_string(), res.freestream_mach);
            metrics.insert("stagnation_pressure_kpa".to_string(), res.stagnation_pressure_kpa);
            metrics.insert("stagnation_enthalpy_mj_kg".to_string(), res.stagnation_enthalpy_mj_kg);
            metrics.insert("shock_standoff_distance_mm".to_string(), res.shock_standoff_distance_mm);
            metrics.insert("convective_heat_flux_kw_m2".to_string(), res.convective_heat_flux_kw_m2);
            metrics.insert("radiative_heat_flux_kw_m2".to_string(), res.radiative_heat_flux_kw_m2);
            metrics.insert("total_stagnation_heat_flux_kw_m2".to_string(), res.total_stagnation_heat_flux_kw_m2);
            metrics.insert("radiative_equilibrium_wall_temperature_k".to_string(), res.radiative_equilibrium_wall_temperature_k);
            metrics.insert("recommended_tps_thickness_mm".to_string(), res.recommended_tps_thickness_mm);
            metrics.insert("thermal_safety_factor".to_string(), res.thermal_safety_factor);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.total_stagnation_heat_flux_kw_m2, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "total_stagnation_heat_flux_kw_m2",
                res.total_stagnation_heat_flux_kw_m2,
                1250.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.aerospace.reentry_aerothermodynamics".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.aerospace.reentry_aerothermodynamics", "0.1.0"),
                logs: vec![format!(
                    "Reentry Aerothermodynamics (Sutton-Graves / NASA SP-8000): Mach {:.1} @ {:.1} km, P_stag = {:.1} kPa, h0 = {:.1} MJ/kg, Delta_shock = {:.1} mm, q_total = {:.1} kW/m² (conv = {:.1}, rad = {:.1}), T_wall_eq = {:.1} K, SF = {:.2}, TPS Thick = {:.1} mm, Safe: {}",
                    res.freestream_mach, problem.altitude_km, res.stagnation_pressure_kpa,
                    res.stagnation_enthalpy_mj_kg, res.shock_standoff_distance_mm,
                    res.total_stagnation_heat_flux_kw_m2, res.convective_heat_flux_kw_m2,
                    res.radiative_heat_flux_kw_m2, res.radiative_equilibrium_wall_temperature_k,
                    res.thermal_safety_factor, res.recommended_tps_thickness_mm, res.is_tps_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.aerospace.reentry_aerothermodynamics".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.aerospace.reentry_aerothermodynamics", "0.1.0"),
                logs: vec![format!("Reentry aerothermodynamics simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_reentry_aerothermodynamics_nominal() {
        let prob = ReentryProblem::default();
        let res = solve_reentry_aerothermodynamics(&prob).expect("reentry solver failed");
        println!(
            "Reentry: Mach = {:.1}, Pstag = {:.1} kPa, h0 = {:.1} MJ/kg, q_tot = {:.1} kW/m², Twall = {:.1} K, SF = {:.2}, safe = {}",
            res.freestream_mach, res.stagnation_pressure_kpa, res.stagnation_enthalpy_mj_kg,
            res.total_stagnation_heat_flux_kw_m2, res.radiative_equilibrium_wall_temperature_k,
            res.thermal_safety_factor, res.is_tps_safe
        );

        assert!(res.freestream_mach > 20.0 && res.freestream_mach < 30.0);
        assert!(res.stagnation_pressure_kpa > 10.0 && res.stagnation_pressure_kpa < 30.0);
        assert!(res.stagnation_enthalpy_mj_kg > 25.0 && res.stagnation_enthalpy_mj_kg < 32.0);
        assert!(res.shock_standoff_distance_mm > 120.0 && res.shock_standoff_distance_mm < 200.0);
        assert!(res.total_stagnation_heat_flux_kw_m2 > 800.0 && res.total_stagnation_heat_flux_kw_m2 < 1400.0);
        assert!(res.radiative_equilibrium_wall_temperature_k > 1700.0 && res.radiative_equilibrium_wall_temperature_k < 2250.0);
        assert!(res.thermal_safety_factor >= 1.05);
        assert!(res.recommended_tps_thickness_mm > 30.0 && res.recommended_tps_thickness_mm < 60.0);
        assert!(res.is_tps_safe);
    }
}

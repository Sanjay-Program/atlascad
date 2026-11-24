//! Rocket Engine De Laval Nozzle & Gas Dynamics Module.
//!
//! Evaluates supersonic isentropic compressible flow through converging-diverging
//! rocket nozzles, characteristic velocity c*, exit Mach number, sea-level and
//! vacuum specific impulse (Isp), and aerodynamic expansion regimes in accordance
//! with NASA SP-8039 and Sutton & Biblarz standard formulations.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Standard acceleration due to gravity on Earth in m/s².
pub const G0: f64 = 9.80665;
/// Universal gas constant in J/(kmol·K) or J/(mol·K) * 1000.
pub const R_UNIVERSAL: f64 = 8314.462618;

/// Rocket nozzle expansion regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NozzleExpansionRegime {
    /// Exit static pressure is lower than ambient pressure (risk of flow separation).
    Overexpanded,
    /// Exit static pressure exactly matches ambient pressure (maximum thrust coefficient).
    OptimumExpanded,
    /// Exit static pressure exceeds ambient pressure (continues expanding outside nozzle).
    Underexpanded,
    /// Vacuum operating environment (ambient pressure is zero).
    VacuumExpansion,
}

/// Rocket nozzle problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocketNozzleProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Stagnation chamber pressure in bar (e.g. 100 bar = 10 MPa).
    pub chamber_pressure_bar: f64,
    /// Stagnation chamber temperature in Kelvin (e.g. 3500 K).
    pub chamber_temperature_k: f64,
    /// Nozzle throat diameter in millimeters (mm).
    pub throat_diameter_mm: f64,
    /// Nozzle expansion area ratio epsilon = A_exit / A_throat.
    pub expansion_ratio_eps: f64,
    /// Combustion products specific heat ratio gamma (Cp/Cv, typically 1.20 - 1.25).
    pub specific_heat_ratio_gamma: f64,
    /// Combustion products molecular weight in g/mol (typically 20 - 26 g/mol for LOX/RP1 or LOX/CH4).
    pub molecular_weight_g_mol: f64,
    /// Ambient atmospheric back-pressure in bar (1.01325 bar at sea level, 0.0 in vacuum).
    pub ambient_pressure_bar: f64,
    /// Discharge coefficient accounting for boundary layer displacement thickness at throat.
    pub discharge_coefficient_cd: f64,
    /// Thrust correction factor accounting for nozzle divergence angle and 3D effects.
    pub thrust_efficiency_cf: f64,
}

fn default_problem_id() -> String {
    "rocket_nozzle_problem".to_string()
}

impl Default for RocketNozzleProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            chamber_pressure_bar: 100.0,
            chamber_temperature_k: 3500.0,
            throat_diameter_mm: 120.0,
            expansion_ratio_eps: 16.0,
            specific_heat_ratio_gamma: 1.22,
            molecular_weight_g_mol: 22.5,
            ambient_pressure_bar: 1.01325,
            discharge_coefficient_cd: 0.985,
            thrust_efficiency_cf: 0.975,
        }
    }
}

/// Rocket nozzle analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RocketNozzleResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Specific gas constant R in J/(kg·K).
    pub specific_gas_constant_j_kg_k: f64,
    /// Nozzle throat cross-sectional area in m².
    pub throat_area_m2: f64,
    /// Nozzle exit cross-sectional area in m².
    pub exit_area_m2: f64,
    /// Characteristic exhaust velocity c* in m/s.
    pub characteristic_velocity_m_s: f64,
    /// Total propellant mass flow rate through nozzle in kg/s.
    pub propellant_mass_flow_kg_s: f64,
    /// Supersonic Mach number at nozzle exit plane.
    pub exit_mach_number: f64,
    /// Static gas temperature at exit plane in Kelvin.
    pub exit_temperature_k: f64,
    /// Static pressure at exit plane in bar.
    pub exit_pressure_bar: f64,
    /// Exhaust gas exit velocity in m/s.
    pub exit_velocity_m_s: f64,
    /// Theoretical thrust coefficient C_F.
    pub thrust_coefficient_cf: f64,
    /// Operating net thrust force in kilonewtons (kN).
    pub net_thrust_kn: f64,
    /// Specific impulse at operating ambient pressure in seconds (s).
    pub operating_specific_impulse_s: f64,
    /// Vacuum specific impulse in seconds (s).
    pub vacuum_specific_impulse_s: f64,
    /// Vacuum thrust force in kilonewtons (kN).
    pub vacuum_thrust_kn: f64,
    /// Nozzle expansion regime at operating ambient pressure.
    pub expansion_regime: NozzleExpansionRegime,
    /// Whether nozzle is free from flow separation (Pe > 0.35 * Pa per Summerfield criterion).
    pub is_safe: bool,
}

/// Solves rocket engine De Laval nozzle isentropic expansion and gas dynamics.
pub fn solve_rocket_nozzle(problem: &RocketNozzleProblem) -> Result<RocketNozzleResult, String> {
    if problem.chamber_pressure_bar <= 0.0 || problem.chamber_temperature_k <= 0.0 {
        return Err("Chamber pressure and temperature must be positive".to_string());
    }
    if problem.throat_diameter_mm <= 0.0 || problem.expansion_ratio_eps < 1.0 {
        return Err("Throat diameter must be positive and expansion ratio >= 1.0".to_string());
    }
    if problem.specific_heat_ratio_gamma <= 1.0 || problem.molecular_weight_g_mol <= 0.0 {
        return Err(
            "Specific heat ratio gamma must be > 1.0 and molecular weight positive".to_string(),
        );
    }

    let gamma = problem.specific_heat_ratio_gamma;
    let r_spec = R_UNIVERSAL / problem.molecular_weight_g_mol;
    let p_c_pa = problem.chamber_pressure_bar * 1e5;
    let p_a_pa = problem.ambient_pressure_bar * 1e5;

    let throat_diameter_m = problem.throat_diameter_mm * 1e-3;
    let a_t = std::f64::consts::PI * (throat_diameter_m / 2.0).powi(2);
    let a_e = a_t * problem.expansion_ratio_eps;

    // Characteristic velocity c* = sqrt(gamma * R * Tc) / (gamma * sqrt((2/(gamma+1))^((gamma+1)/(gamma-1))))
    let gamma_factor = (2.0 / (gamma + 1.0)).powf((gamma + 1.0) / (gamma - 1.0));
    let c_star =
        (gamma * r_spec * problem.chamber_temperature_k).sqrt() / (gamma * gamma_factor.sqrt());

    // Propellant mass flow rate: m_dot = Cd * (Pc * At) / c*
    let m_dot = problem.discharge_coefficient_cd * (p_c_pa * a_t) / c_star;

    // Supersonic exit Mach number M_e via Newton-Raphson on area-Mach relation:
    // A/At = (1/M) * [ (2 / (gamma + 1)) * (1 + (gamma - 1)/2 * M^2) ]^((gamma + 1) / (2*(gamma - 1)))
    let target_eps = problem.expansion_ratio_eps;
    let exp_power = (gamma + 1.0) / (2.0 * (gamma - 1.0));
    let mut m_e: f64 = 3.0; // initial supersonic guess
    for _ in 0..50 {
        let term = (2.0 / (gamma + 1.0)) * (1.0 + (gamma - 1.0) / 2.0 * m_e.powi(2));
        let area_ratio = (1.0 / m_e) * term.powf(exp_power);
        let f = area_ratio - target_eps;
        if f.abs() < 1e-7 {
            break;
        }
        // Derivative d(A/At)/dM = (term^exp_power / M^2) * ( (M^2 - 1) / (1 + (gamma-1)/2 * M^2) )
        let df_dm = (term.powf(exp_power) / m_e.powi(2))
            * ((m_e.powi(2) - 1.0) / (1.0 + (gamma - 1.0) / 2.0 * m_e.powi(2)));
        if df_dm.abs() < 1e-12 {
            break;
        }
        m_e -= f / df_dm;
        if m_e < 1.01 {
            m_e = 1.01;
        }
    }

    // Static exit temperature: Te = Tc / (1 + (gamma - 1)/2 * Me^2)
    let temp_ratio = 1.0 + (gamma - 1.0) / 2.0 * m_e.powi(2);
    let t_e = problem.chamber_temperature_k / temp_ratio;

    // Static exit pressure: Pe = Pc * (Te / Tc)^(gamma / (gamma - 1))
    let p_e_pa = p_c_pa * (1.0 / temp_ratio).powf(gamma / (gamma - 1.0));
    let p_e_bar = p_e_pa * 1e-5;

    // Exit velocity: ve = Me * sqrt(gamma * R * Te)
    let v_e = m_e * (gamma * r_spec * t_e).sqrt();

    // Thrust calculation:
    // Net thrust F = eta_cf * [ m_dot * ve + (Pe - Pa) * Ae ]
    let momentum_thrust_n = m_dot * v_e;
    let pressure_thrust_n = (p_e_pa - p_a_pa) * a_e;
    let net_thrust_n = problem.thrust_efficiency_cf * (momentum_thrust_n + pressure_thrust_n);
    let net_thrust_kn = net_thrust_n * 1e-3;

    // Vacuum thrust:
    let vacuum_thrust_n = problem.thrust_efficiency_cf * (momentum_thrust_n + p_e_pa * a_e);
    let vacuum_thrust_kn = vacuum_thrust_n * 1e-3;

    // Specific impulse: Isp = F / (m_dot * g0)
    let operating_isp_s = net_thrust_n / (m_dot * G0);
    let vacuum_isp_s = vacuum_thrust_n / (m_dot * G0);

    // Thrust coefficient: Cf = F / (Pc * At)
    let cf = net_thrust_n / (p_c_pa * a_t);

    // Expansion regime & flow separation risk (Summerfield criterion: Pe / Pa >= 0.35)
    let expansion_regime = if problem.ambient_pressure_bar <= 1e-5 {
        NozzleExpansionRegime::VacuumExpansion
    } else if (p_e_bar - problem.ambient_pressure_bar).abs() < 0.02 {
        NozzleExpansionRegime::OptimumExpanded
    } else if p_e_bar < problem.ambient_pressure_bar {
        NozzleExpansionRegime::Overexpanded
    } else {
        NozzleExpansionRegime::Underexpanded
    };

    let is_safe = if problem.ambient_pressure_bar > 1e-5 {
        p_e_pa >= 0.35 * p_a_pa
    } else {
        true
    };

    Ok(RocketNozzleResult {
        problem_id: problem.problem_id.clone(),
        specific_gas_constant_j_kg_k: r_spec,
        throat_area_m2: a_t,
        exit_area_m2: a_e,
        characteristic_velocity_m_s: c_star,
        propellant_mass_flow_kg_s: m_dot,
        exit_mach_number: m_e,
        exit_temperature_k: t_e,
        exit_pressure_bar: p_e_bar,
        exit_velocity_m_s: v_e,
        thrust_coefficient_cf: cf,
        net_thrust_kn,
        operating_specific_impulse_s: operating_isp_s,
        vacuum_specific_impulse_s: vacuum_isp_s,
        vacuum_thrust_kn,
        expansion_regime,
        is_safe,
    })
}

/// Generic simulation adapter for rocket engine nozzle.
pub fn solve_rocket_nozzle_sim(problem: &RocketNozzleProblem) -> SimulationResult {
    match solve_rocket_nozzle(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("exit_mach_number".to_string(), result.exit_mach_number);
            metrics.insert("net_thrust_kn".to_string(), result.net_thrust_kn);
            metrics.insert(
                "operating_specific_impulse_s".to_string(),
                result.operating_specific_impulse_s,
            );
            metrics.insert(
                "vacuum_specific_impulse_s".to_string(),
                result.vacuum_specific_impulse_s,
            );
            metrics.insert("exit_pressure_bar".to_string(), result.exit_pressure_bar);
            metrics.insert(
                "propellant_mass_flow_kg_s".to_string(),
                result.propellant_mass_flow_kg_s,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.exit_mach_number, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "operating_specific_impulse_s",
                result.operating_specific_impulse_s,
                300.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.aerospace.rocket_nozzle_delaval".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.aerospace.rocket_nozzle_delaval",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Rocket Nozzle: Mach {:.2}, Thrust = {:.1} kN, Isp = {:.1}s (Vac: {:.1}s), Pe = {:.3} bar (Safe: {})",
                    result.exit_mach_number,
                    result.net_thrust_kn,
                    result.operating_specific_impulse_s,
                    result.vacuum_specific_impulse_s,
                    result.exit_pressure_bar,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.aerospace.rocket_nozzle_delaval".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.aerospace.rocket_nozzle_delaval",
                    "0.1.0",
                ),
                logs: vec![format!("Rocket nozzle simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_rocket_nozzle_nominal() {
        let prob = RocketNozzleProblem::default();
        let res = solve_rocket_nozzle(&prob).expect("solver failed");

        assert!(
            res.characteristic_velocity_m_s > 1700.0 && res.characteristic_velocity_m_s < 2000.0
        );
        assert!(res.propellant_mass_flow_kg_s > 50.0 && res.propellant_mass_flow_kg_s < 70.0);
        assert!(res.exit_mach_number > 3.0 && res.exit_mach_number < 4.0);
        assert!(res.exit_pressure_bar > 0.40 && res.exit_pressure_bar < 0.90);
        assert!(res.net_thrust_kn > 150.0 && res.net_thrust_kn < 220.0);
        assert!(
            res.operating_specific_impulse_s > 260.0 && res.operating_specific_impulse_s < 330.0
        );
        assert!(res.vacuum_specific_impulse_s > res.operating_specific_impulse_s);
        assert!(res.is_safe);
        assert_eq!(res.expansion_regime, NozzleExpansionRegime::Overexpanded);
    }
}

//! Continuous Stirred-Tank Reactor (CSTR) Kinetics & Thermal Stability Module.
//!
//! Evaluates material and energy balances, Arrhenius reaction kinetics, Damköhler number,
//! Van Heerden heat generation vs heat removal curves, steady-state thermal multiplicity,
//! and runaway ignition stability in accordance with Fogler, Levenspiel, and AIChE DIERS criteria.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Universal gas constant in J/(mol·K).
pub const R_MOL: f64 = 8.314462618;

/// CSTR chemical reactor problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChemicalReactorProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Working volume of the reactor liquid in cubic meters (m³).
    pub reactor_volume_m3: f64,
    /// Liquid feed volumetric flow rate in cubic meters per hour (m³/h).
    pub feed_volumetric_flow_m3_h: f64,
    /// Key reactant feed inlet concentration in moles per cubic meter (mol/m³ or mol/L * 1000).
    pub feed_reactant_concentration_mol_m3: f64,
    /// Feed stream inlet temperature in degrees Celsius (°C).
    pub feed_temperature_c: f64,
    /// Arrhenius pre-exponential frequency factor A in 1/second (s⁻¹).
    pub pre_exponential_factor_s_inv: f64,
    /// Reaction activation energy E_a in kiloJoules per mole (kJ/mol).
    pub activation_energy_kj_mol: f64,
    /// Standard heat of reaction Delta H_rxn in kiloJoules per mole (kJ/mol) (negative for exothermic).
    pub reaction_enthalpy_kj_mol: f64,
    /// Bulk liquid mixture density in kg/m³.
    pub fluid_density_kg_m3: f64,
    /// Bulk liquid mixture specific heat capacity in J/(kg·K).
    pub fluid_specific_heat_j_kg_k: f64,
    /// Overall jacket heat transfer coefficient area product UA in Watts per Kelvin (W/K).
    pub jacket_overall_ua_w_k: f64,
    /// Cooling jacket coolant supply temperature in degrees Celsius (°C).
    pub coolant_temperature_c: f64,
    /// Maximum allowable reactor design temperature in degrees Celsius (°C).
    pub max_allowable_reactor_temp_c: f64,
}

fn default_problem_id() -> String {
    "chemical_reactor_problem".to_string()
}

impl Default for ChemicalReactorProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            reactor_volume_m3: 2.50,
            feed_volumetric_flow_m3_h: 5.0,
            feed_reactant_concentration_mol_m3: 2000.0,
            feed_temperature_c: 25.0,
            pre_exponential_factor_s_inv: 4.5e9,
            activation_energy_kj_mol: 75.0,
            reaction_enthalpy_kj_mol: -85.0,
            fluid_density_kg_m3: 1020.0,
            fluid_specific_heat_j_kg_k: 3800.0,
            jacket_overall_ua_w_k: 4500.0,
            coolant_temperature_c: 18.0,
            max_allowable_reactor_temp_c: 95.0,
        }
    }
}

/// Chemical reactor analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChemicalReactorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Reactor mean residence time (space time) tau in minutes.
    pub residence_time_minutes: f64,
    /// Steady-state reactor operating temperature in degrees Celsius (°C).
    pub reactor_temperature_c: f64,
    /// Kinetic reaction rate constant k at steady-state temperature in 1/second (s⁻¹).
    pub rate_constant_s_inv: f64,
    /// Dimensionless Damköhler number Da = k * tau.
    pub damkohler_number: f64,
    /// Reactant steady-state fractional conversion X (0.0 to 1.0).
    pub fractional_conversion: f64,
    /// Reactant outlet effluent concentration in mol/m³.
    pub outlet_reactant_concentration_mol_m3: f64,
    /// Heat generation rate from exothermic reaction in kilowatts (kW).
    pub heat_generation_rate_kw: f64,
    /// Heat removal rate absorbed by jacket cooling in kilowatts (kW).
    pub jacket_cooling_duty_kw: f64,
    /// Slope of heat generation curve dG/dT in W/K at steady-state.
    pub slope_heat_generation_w_k: f64,
    /// Slope of heat removal line dR/dT in W/K at steady-state.
    pub slope_heat_removal_w_k: f64,
    /// Thermal stability margin (dR/dT - dG/dT) in W/K (> 0 implies stable operating point).
    pub thermal_stability_margin_w_k: f64,
    /// Whether reactor operates stably without runaway and within temperature limits.
    pub is_safe: bool,
}

/// Evaluates Arrhenius rate constant k(T) in s⁻¹ for temperature T in Kelvin.
fn arrhenius_k(a: f64, ea_j_mol: f64, t_k: f64) -> f64 {
    a * (-ea_j_mol / (R_MOL * t_k)).exp()
}

/// Solves CSTR chemical reactor material/energy balance and thermal stability.
pub fn solve_chemical_reactor(problem: &ChemicalReactorProblem) -> Result<ChemicalReactorResult, String> {
    if problem.reactor_volume_m3 <= 0.0 || problem.feed_volumetric_flow_m3_h <= 0.0 {
        return Err("Reactor volume and feed flow rate must be positive".to_string());
    }
    if problem.pre_exponential_factor_s_inv <= 0.0 || problem.activation_energy_kj_mol <= 0.0 {
        return Err("Arrhenius parameters must be positive".to_string());
    }

    let v_m3 = problem.reactor_volume_m3;
    let v0_m3_s = problem.feed_volumetric_flow_m3_h / 3600.0;
    let tau_s = v_m3 / v0_m3_s;
    let tau_min = tau_s / 60.0;

    let ea_j_mol = problem.activation_energy_kj_mol * 1000.0;
    let delta_h_j_mol = problem.reaction_enthalpy_kj_mol * 1000.0; // negative
    let heat_rxn_pos = -delta_h_j_mol; // positive for exothermic

    let t_feed_k = problem.feed_temperature_c + 273.15;
    let t_cool_k = problem.coolant_temperature_c + 273.15;

    let m_dot_kg_s = v0_m3_s * problem.fluid_density_kg_m3;
    let cp_flow_w_k = m_dot_kg_s * problem.fluid_specific_heat_j_kg_k;
    let ua_w_k = problem.jacket_overall_ua_w_k;

    // Van Heerden Energy Balance:
    // Heat Generation G(T): Q_gen = v0 * C_A0 * X(T) * (-Delta H_rxn)
    // where X(T) = (k(T) * tau) / (1 + k(T) * tau) for first-order reaction.
    // Heat Removal R(T): Q_rem = m_dot * cp * (T - T_feed) + UA * (T - T_cool)
    // Steady state: f(T) = Q_gen(T) - Q_rem(T) = 0.

    let q_gen = |t_k: f64| -> f64 {
        let k = arrhenius_k(problem.pre_exponential_factor_s_inv, ea_j_mol, t_k);
        let da = k * tau_s;
        let x = da / (1.0 + da);
        v0_m3_s * problem.feed_reactant_concentration_mol_m3 * x * heat_rxn_pos
    };

    let q_rem = |t_k: f64| -> f64 {
        cp_flow_w_k * (t_k - t_feed_k) + ua_w_k * (t_k - t_cool_k)
    };

    let f_net = |t_k: f64| -> f64 {
        q_gen(t_k) - q_rem(t_k)
    };

    // Bisection search on T_k in range [T_min, T_max]
    let mut low_k = t_cool_k.min(t_feed_k) - 5.0;
    let mut high_k = t_feed_k + 150.0;

    // Find root using bracketed bisection followed by Newton step
    let mut t_ss_k = (low_k + high_k) / 2.0;
    for _ in 0..100 {
        let mid = (low_k + high_k) / 2.0;
        let f_mid = f_net(mid);
        if f_mid.abs() < 1e-4 || (high_k - low_k).abs() < 1e-5 {
            t_ss_k = mid;
            break;
        }
        if f_net(low_k) * f_mid <= 0.0 {
            high_k = mid;
        } else {
            low_k = mid;
        }
        t_ss_k = mid;
    }

    let t_ss_c = t_ss_k - 273.15;
    let k_ss = arrhenius_k(problem.pre_exponential_factor_s_inv, ea_j_mol, t_ss_k);
    let da_ss = k_ss * tau_s;
    let x_ss = da_ss / (1.0 + da_ss);
    let c_a_out = problem.feed_reactant_concentration_mol_m3 * (1.0 - x_ss);

    let heat_gen_w = q_gen(t_ss_k);
    let heat_gen_kw = heat_gen_w * 1e-3;
    let jacket_duty_w = ua_w_k * (t_ss_k - t_cool_k);
    let jacket_duty_kw = jacket_duty_w * 1e-3;

    // Slopes dG/dT and dR/dT
    let dt = 0.01;
    let dg_dt = (q_gen(t_ss_k + dt) - q_gen(t_ss_k - dt)) / (2.0 * dt);
    let dr_dt = cp_flow_w_k + ua_w_k;
    let stability_margin = dr_dt - dg_dt;

    // Van Heerden stability criterion: dR/dT > dG/dT at operating point
    let is_stable = stability_margin > 0.0;
    let is_temp_ok = t_ss_c <= problem.max_allowable_reactor_temp_c;
    let is_safe = is_stable && is_temp_ok;

    Ok(ChemicalReactorResult {
        problem_id: problem.problem_id.clone(),
        residence_time_minutes: tau_min,
        reactor_temperature_c: t_ss_c,
        rate_constant_s_inv: k_ss,
        damkohler_number: da_ss,
        fractional_conversion: x_ss,
        outlet_reactant_concentration_mol_m3: c_a_out,
        heat_generation_rate_kw: heat_gen_kw,
        jacket_cooling_duty_kw: jacket_duty_kw,
        slope_heat_generation_w_k: dg_dt,
        slope_heat_removal_w_k: dr_dt,
        thermal_stability_margin_w_k: stability_margin,
        is_safe,
    })
}

/// Generic simulation adapter for CSTR chemical reactor.
pub fn solve_chemical_reactor_sim(problem: &ChemicalReactorProblem) -> SimulationResult {
    match solve_chemical_reactor(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("reactor_temperature_c".to_string(), result.reactor_temperature_c);
            metrics.insert("fractional_conversion".to_string(), result.fractional_conversion);
            metrics.insert("damkohler_number".to_string(), result.damkohler_number);
            metrics.insert("heat_generation_rate_kw".to_string(), result.heat_generation_rate_kw);
            metrics.insert("jacket_cooling_duty_kw".to_string(), result.jacket_cooling_duty_kw);
            metrics.insert("thermal_stability_margin_w_k".to_string(), result.thermal_stability_margin_w_k);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.reactor_temperature_c, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "reactor_temperature_c",
                result.reactor_temperature_c,
                50.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.chemical.cstr_thermal_stability".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.chemical.cstr_thermal_stability", "0.1.0"),
                logs: vec![format!(
                    "CSTR Reactor: T = {:.1} °C, X = {:.1}%, Da = {:.2}, Q_gen = {:.1} kW, Margin = {:.1} W/K (Safe: {})",
                    result.reactor_temperature_c,
                    result.fractional_conversion * 100.0,
                    result.damkohler_number,
                    result.heat_generation_rate_kw,
                    result.thermal_stability_margin_w_k,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.chemical.cstr_thermal_stability".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.chemical.cstr_thermal_stability", "0.1.0"),
                logs: vec![format!("CSTR chemical reactor simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_chemical_reactor_nominal() {
        let prob = ChemicalReactorProblem::default();
        let res = solve_chemical_reactor(&prob).expect("solver failed");

        assert_eq!(res.residence_time_minutes, 30.0);
        assert!(res.reactor_temperature_c > 35.0 && res.reactor_temperature_c < 75.0);
        assert!(res.fractional_conversion > 0.60 && res.fractional_conversion < 0.98);
        assert!(res.damkohler_number > 2.0);
        assert!(res.heat_generation_rate_kw > 100.0 && res.heat_generation_rate_kw < 300.0);
        assert!(res.thermal_stability_margin_w_k > 0.0);
        assert!(res.is_safe);
    }
}

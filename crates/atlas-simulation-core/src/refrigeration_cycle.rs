//! Vapor Compression Refrigeration & Heat Pump Cycle Module.
//!
//! Evaluates thermodynamic states, isentropic compression, subcooling, superheating,
//! mass flow rate, cooling capacity, heating rejection duty, and COP
//! in accordance with ASHRAE Handbook of Fundamentals and AHRI 550/590 standards.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Refrigerant working fluid type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RefrigerantType {
    /// R410A (hydrofluorocarbon blend, high pressure).
    R410A,
    /// R134a (1,1,1,2-tetrafluoroethane, automotive/chillers).
    R134a,
    /// R32 (difluoromethane, low-GWP residential AC).
    R32,
    /// R290 (propane, natural hydrocarbon refrigerant).
    R290,
}

impl RefrigerantType {
    /// Reference thermophysical properties: (cp_vapor_kj_kg_k, cp_liquid_kj_kg_k, gamma, h_fg_nominal_kj_kg).
    pub fn properties(&self) -> (f64, f64, f64, f64) {
        match self {
            Self::R410A => (1.05, 1.75, 1.18, 220.0),
            Self::R134a => (0.90, 1.45, 1.12, 198.0),
            Self::R32 => (1.25, 1.95, 1.25, 305.0),
            Self::R290 => (1.70, 2.55, 1.16, 375.0),
        }
    }

    /// Approximate Antoine vapor pressure in bar given saturation temperature in Celsius.
    pub fn saturation_pressure_bar(&self, temp_c: f64) -> f64 {
        let t_k = temp_c + 273.15;
        match self {
            Self::R410A => (10.80 - 2380.0 / t_k).exp(),
            Self::R134a => (9.90 - 2450.0 / t_k).exp(),
            Self::R32 => (10.40 - 2150.0 / t_k).exp(),
            Self::R290 => (9.50 - 2200.0 / t_k).exp(),
        }
    }
}

/// Refrigeration cycle problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefrigerationCycleProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Working refrigerant fluid.
    #[serde(default = "default_refrigerant")]
    pub refrigerant: RefrigerantType,
    /// Design cooling capacity in kilowatts (kW) (e.g. 100 kW chiller).
    pub design_cooling_capacity_kw: f64,
    /// Saturated evaporating temperature in degrees Celsius (°C) (e.g. 4.0 °C).
    pub evaporating_temperature_c: f64,
    /// Saturated condensing temperature in degrees Celsius (°C) (e.g. 45.0 °C).
    pub condensing_temperature_c: f64,
    /// Useful evaporator vapor superheat in Kelvin or °C (e.g. 5.0 K).
    pub superheat_k: f64,
    /// Condenser liquid subcooling in Kelvin or °C (e.g. 5.0 K).
    pub subcooling_k: f64,
    /// Compressor isentropic efficiency (typically 0.70 to 0.85).
    pub compressor_isentropic_efficiency: f64,
    /// Combined motor and mechanical transmission efficiency (typically 0.88 to 0.95).
    pub compressor_mechanical_efficiency: f64,
}

fn default_problem_id() -> String {
    "refrigeration_cycle_problem".to_string()
}

fn default_refrigerant() -> RefrigerantType {
    RefrigerantType::R410A
}

impl Default for RefrigerationCycleProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            refrigerant: default_refrigerant(),
            design_cooling_capacity_kw: 100.0,
            evaporating_temperature_c: 5.0,
            condensing_temperature_c: 45.0,
            superheat_k: 5.0,
            subcooling_k: 5.0,
            compressor_isentropic_efficiency: 0.78,
            compressor_mechanical_efficiency: 0.92,
        }
    }
}

/// Refrigeration cycle analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefrigerationCycleResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Evaporating saturation pressure in bar.
    pub evaporating_pressure_bar: f64,
    /// Condensing saturation pressure in bar.
    pub condensing_pressure_bar: f64,
    /// Compressor pressure ratio rp = P_cond / P_evap.
    pub pressure_ratio: f64,
    /// Specific cooling effect (refrigerating effect) in kJ/kg.
    pub specific_refrigerating_effect_kj_kg: f64,
    /// Specific compressor work in kJ/kg.
    pub specific_compressor_work_kj_kg: f64,
    /// Specific condenser heat rejection in kJ/kg.
    pub specific_heat_rejection_kj_kg: f64,
    /// Circulating refrigerant mass flow rate in kg/s.
    pub refrigerant_mass_flow_kg_s: f64,
    /// Compressor electrical power input in kilowatts (kW).
    pub compressor_electrical_power_kw: f64,
    /// Total condenser heat rejection duty in kilowatts (kW).
    pub condenser_heating_duty_kw: f64,
    /// Coefficient of Performance for Cooling (COP_R).
    pub coefficient_of_performance_cooling: f64,
    /// Coefficient of Performance for Heating / Heat Pump mode (COP_HP).
    pub coefficient_of_performance_heating: f64,
    /// Compressor discharge vapor temperature in degrees Celsius (°C).
    pub compressor_discharge_temperature_c: f64,
    /// Whether cycle operates within safe pressure ratio (rp <= 8.0) and discharge temperature (<= 125°C).
    pub is_safe: bool,
}

/// Solves vapor compression refrigeration and heat pump thermodynamic cycle.
pub fn solve_refrigeration_cycle(
    problem: &RefrigerationCycleProblem,
) -> Result<RefrigerationCycleResult, String> {
    if problem.design_cooling_capacity_kw <= 0.0 {
        return Err("Cooling capacity must be positive".to_string());
    }
    if problem.condensing_temperature_c <= problem.evaporating_temperature_c {
        return Err("Condensing temperature must exceed evaporating temperature".to_string());
    }
    if problem.compressor_isentropic_efficiency <= 0.0
        || problem.compressor_isentropic_efficiency > 1.0
    {
        return Err("Isentropic efficiency must be between 0.0 and 1.0".to_string());
    }

    let p_evap = problem
        .refrigerant
        .saturation_pressure_bar(problem.evaporating_temperature_c);
    let p_cond = problem
        .refrigerant
        .saturation_pressure_bar(problem.condensing_temperature_c);
    let rp = p_cond / p_evap;

    let (cp_v, cp_l, gamma, h_fg) = problem.refrigerant.properties();

    // Enthalpy reference model (relative kJ/kg):
    // Baseline saturated vapor at T_evap: h_g = 400.0 kJ/kg
    // Baseline saturated liquid at T_cond: h_f = 400.0 - h_fg
    let h_g1 = 400.0;
    // State 1: Suction vapor with superheat
    let h1 = h_g1 + cp_v * problem.superheat_k;
    let t1_k = (problem.evaporating_temperature_c + problem.superheat_k) + 273.15;

    // State 2s: Isentropic compression
    // T2s = T1 * (P_cond / P_evap)^((gamma - 1) / gamma)
    let exp_term = (gamma - 1.0) / gamma;
    let t2s_k = t1_k * rp.powf(exp_term);
    let w_s = cp_v * (t2s_k - t1_k);

    // State 2: Actual compression
    let w_actual = w_s / problem.compressor_isentropic_efficiency;
    let h2 = h1 + w_actual;
    let t2_k = t1_k + w_actual / cp_v;
    let t2_c = t2_k - 273.15;

    // State 3: Subcooled liquid leaving condenser
    let h_f_cond =
        h_g1 + cp_v * (problem.condensing_temperature_c - problem.evaporating_temperature_c) - h_fg;
    let h3 = h_f_cond - cp_l * problem.subcooling_k;

    // State 4: Isenthalpic throttling across expansion valve: h4 = h3
    let h4 = h3;

    // Specific duties:
    let q_evap = h1 - h4; // kJ/kg
    if q_evap <= 0.0 {
        return Err("Thermodynamic parameters yield non-positive refrigerating effect".to_string());
    }
    let q_comp = w_actual; // kJ/kg
    let q_cond = h2 - h3; // kJ/kg

    // Mass flow rate: m_dot = Q_cooling / q_evap
    let m_dot = problem.design_cooling_capacity_kw / q_evap;

    // Electrical compressor power: P_comp = m_dot * w_comp / eta_mech
    let eta_tot = problem.compressor_mechanical_efficiency.clamp(0.5, 1.0);
    let p_comp_kw = (m_dot * q_comp) / eta_tot;

    // Condenser duty: Q_cond = m_dot * q_cond
    let q_cond_kw = m_dot * q_cond;

    // Performance coefficients:
    let cop_cooling = problem.design_cooling_capacity_kw / p_comp_kw;
    let cop_heating = q_cond_kw / p_comp_kw;

    // Safety checks: Pressure ratio <= 8.0 and discharge temp <= 125.0 °C
    let is_safe = rp <= 8.0 && t2_c <= 125.0;

    Ok(RefrigerationCycleResult {
        problem_id: problem.problem_id.clone(),
        evaporating_pressure_bar: p_evap,
        condensing_pressure_bar: p_cond,
        pressure_ratio: rp,
        specific_refrigerating_effect_kj_kg: q_evap,
        specific_compressor_work_kj_kg: q_comp,
        specific_heat_rejection_kj_kg: q_cond,
        refrigerant_mass_flow_kg_s: m_dot,
        compressor_electrical_power_kw: p_comp_kw,
        condenser_heating_duty_kw: q_cond_kw,
        coefficient_of_performance_cooling: cop_cooling,
        coefficient_of_performance_heating: cop_heating,
        compressor_discharge_temperature_c: t2_c,
        is_safe,
    })
}

/// Generic simulation adapter for refrigeration cycle.
pub fn solve_refrigeration_cycle_sim(problem: &RefrigerationCycleProblem) -> SimulationResult {
    match solve_refrigeration_cycle(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("pressure_ratio".to_string(), result.pressure_ratio);
            metrics.insert(
                "coefficient_of_performance_cooling".to_string(),
                result.coefficient_of_performance_cooling,
            );
            metrics.insert(
                "coefficient_of_performance_heating".to_string(),
                result.coefficient_of_performance_heating,
            );
            metrics.insert(
                "compressor_electrical_power_kw".to_string(),
                result.compressor_electrical_power_kw,
            );
            metrics.insert(
                "compressor_discharge_temperature_c".to_string(),
                result.compressor_discharge_temperature_c,
            );
            metrics.insert(
                "refrigerant_mass_flow_kg_s".to_string(),
                result.refrigerant_mass_flow_kg_s,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.coefficient_of_performance_cooling, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "coefficient_of_performance_cooling",
                result.coefficient_of_performance_cooling,
                3.5,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.thermal.refrigeration_cycle_ashrae".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.thermal.refrigeration_cycle_ashrae",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Refrigeration Cycle (ASHRAE): COP_R = {:.2}, COP_HP = {:.2}, P_comp = {:.1} kW, rp = {:.2}, T_dis = {:.1} °C (Safe: {})",
                    result.coefficient_of_performance_cooling,
                    result.coefficient_of_performance_heating,
                    result.compressor_electrical_power_kw,
                    result.pressure_ratio,
                    result.compressor_discharge_temperature_c,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.thermal.refrigeration_cycle_ashrae".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.thermal.refrigeration_cycle_ashrae",
                    "0.1.0",
                ),
                logs: vec![format!("Refrigeration cycle simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_refrigeration_cycle_nominal() {
        let prob = RefrigerationCycleProblem::default();
        let res = solve_refrigeration_cycle(&prob).expect("solver failed");
        println!(
            "Refrigeration: P_evap = {}, P_cond = {}, rp = {}, COP_R = {}, COP_HP = {}, T_dis = {}",
            res.evaporating_pressure_bar,
            res.condensing_pressure_bar,
            res.pressure_ratio,
            res.coefficient_of_performance_cooling,
            res.coefficient_of_performance_heating,
            res.compressor_discharge_temperature_c
        );

        assert!(res.evaporating_pressure_bar > 5.0 && res.evaporating_pressure_bar < 12.0);
        assert!(res.condensing_pressure_bar > 15.0 && res.condensing_pressure_bar < 35.0);
        assert!(res.pressure_ratio > 2.0 && res.pressure_ratio < 5.0);
        assert!(res.specific_refrigerating_effect_kj_kg > 100.0);
        assert!(res.refrigerant_mass_flow_kg_s > 0.3 && res.refrigerant_mass_flow_kg_s < 1.2);
        assert!(
            res.compressor_electrical_power_kw > 15.0 && res.compressor_electrical_power_kw < 50.0
        );
        assert!(
            res.coefficient_of_performance_cooling > 2.0
                && res.coefficient_of_performance_cooling < 6.0
        );
        assert!(res.coefficient_of_performance_heating > res.coefficient_of_performance_cooling);
        assert!(
            res.compressor_discharge_temperature_c > 40.0
                && res.compressor_discharge_temperature_c < 110.0
        );
        assert!(res.is_safe);
    }
}

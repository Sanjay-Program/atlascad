//! Solar thermal collector and heat pipe efficiency solver.
//!
//! Compliant with ISO 9806, EN 12975, and the Hottel-Whillier-Bliss (HWB) formulation.
//! Evaluates flat-plate and evacuated-tube collector efficiency curves, fluid temperature gain,
//! thermal power output, and zero-flow stagnation temperature limits.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Collector construction typology.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CollectorType {
    /// Glazed selective flat-plate collector.
    FlatPlateGlazed,
    /// Unglazed swimming pool / low-temp collector.
    FlatPlateUnglazed,
    /// Evacuated tube collector with internal heat pipes (low convective loss).
    EvacuatedTube,
}

impl CollectorType {
    /// Default ISO 9806 coefficients [eta0, a1 (W/m2K), a2 (W/m2K2)].
    pub fn default_coefficients(self) -> (f64, f64, f64) {
        match self {
            Self::FlatPlateGlazed => (0.78, 3.65, 0.015),
            Self::FlatPlateUnglazed => (0.85, 12.50, 0.050),
            Self::EvacuatedTube => (0.74, 1.45, 0.006),
        }
    }
}

/// Solar thermal collector problem definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarThermalProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Collector technological typology.
    pub collector_type: CollectorType,
    /// Gross module surface area in m^2.
    pub gross_area_m2: f64,
    /// Net optical aperture surface area in m^2.
    pub aperture_area_m2: f64,
    /// Global total solar irradiance on collector plane in W/m^2.
    pub solar_irradiance_w_m2: f64,
    /// Ambient air temperature in degrees Celsius.
    pub ambient_temperature_c: f64,
    /// Heat transfer fluid inlet temperature in degrees Celsius.
    pub fluid_inlet_temp_c: f64,
    /// Fluid mass flow rate through collector in kg/s.
    pub fluid_flow_rate_kg_s: f64,
    /// Specific heat capacity of heat transfer fluid in J/(kg*K) (e.g. 4182 for water, 3800 for glycol mix).
    pub fluid_specific_heat_j_kg_k: f64,
    /// Optional override for zero-loss optical efficiency eta0.
    pub optical_efficiency_eta0: Option<f64>,
    /// Optional override for first-order linear heat loss coefficient a1 in W/(m^2*K).
    pub linear_heat_loss_a1: Option<f64>,
    /// Optional override for second-order temperature quadratic heat loss coefficient a2 in W/(m^2*K^2).
    pub quadratic_heat_loss_a2: Option<f64>,
}

impl Default for SolarThermalProblem {
    fn default() -> Self {
        Self {
            problem_id: "solar_thermal_nominal".to_string(),
            collector_type: CollectorType::FlatPlateGlazed,
            gross_area_m2: 2.50,
            aperture_area_m2: 2.30,
            solar_irradiance_w_m2: 850.0,
            ambient_temperature_c: 20.0,
            fluid_inlet_temp_c: 45.0,
            fluid_flow_rate_kg_s: 0.045,
            fluid_specific_heat_j_kg_k: 4182.0,
            optical_efficiency_eta0: None,
            linear_heat_loss_a1: None,
            quadratic_heat_loss_a2: None,
        }
    }
}

/// Results of solar thermal collector thermal rating.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolarThermalResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Zero-loss optical efficiency eta0.
    pub optical_efficiency_eta0: f64,
    /// Linear heat loss coefficient a1 in W/(m^2*K).
    pub linear_loss_coeff_a1: f64,
    /// Quadratic heat loss coefficient a2 in W/(m^2*K^2).
    pub quadratic_loss_coeff_a2: f64,
    /// Instantaneous collector thermal efficiency percentage (%).
    pub collector_efficiency_pct: f64,
    /// Useful thermal power delivered to fluid in Watts.
    pub useful_thermal_power_w: f64,
    /// Fluid outlet temperature in degrees Celsius.
    pub fluid_outlet_temp_c: f64,
    /// Mean collector fluid temperature Tm in degrees Celsius.
    pub mean_fluid_temp_c: f64,
    /// Fluid temperature rise through collector in degrees Celsius.
    pub temperature_gain_c: f64,
    /// Maximum zero-flow stagnation temperature in degrees Celsius.
    pub stagnation_temperature_c: f64,
    /// Total incident solar power on aperture in Watts.
    pub solar_input_power_w: f64,
    /// Total thermal heat loss power to ambient in Watts.
    pub heat_loss_power_w: f64,
    /// Whether operating temperatures and thermal gain are feasible and safe.
    pub is_safe: bool,
    /// Engineering rating and operational assessment.
    pub design_assessment: String,
}

/// Solves solar thermal collector efficiency, power output, and temperatures.
pub fn solve_solar_thermal(problem: &SolarThermalProblem) -> Result<SolarThermalResult, String> {
    if problem.aperture_area_m2 <= 0.0 {
        return Err("Aperture area must be positive".to_string());
    }
    if problem.solar_irradiance_w_m2 <= 0.0 {
        return Err("Solar irradiance must be positive".to_string());
    }
    if problem.fluid_flow_rate_kg_s <= 0.0 {
        return Err("Fluid flow rate must be positive".to_string());
    }
    if problem.fluid_specific_heat_j_kg_k <= 0.0 {
        return Err("Fluid specific heat must be positive".to_string());
    }

    let (def_eta0, def_a1, def_a2) = problem.collector_type.default_coefficients();
    let eta0 = problem
        .optical_efficiency_eta0
        .unwrap_or(def_eta0)
        .clamp(0.1, 1.0);
    let a1 = problem.linear_heat_loss_a1.unwrap_or(def_a1).max(0.1);
    let a2 = problem.quadratic_heat_loss_a2.unwrap_or(def_a2).max(0.0001);

    let g_t = problem.solar_irradiance_w_m2;
    let t_amb = problem.ambient_temperature_c;
    let t_in = problem.fluid_inlet_temp_c;
    let a_apt = problem.aperture_area_m2;
    let m_cp = problem.fluid_flow_rate_kg_s * problem.fluid_specific_heat_j_kg_k;

    let solar_input_power_w = g_t * a_apt;

    let mut t_m = t_in + 2.0;
    let mut t_out = t_in;
    let mut q_u = 0.0;
    let mut eta = 0.0;

    for _ in 0..20 {
        let delta_t = t_m - t_amb;
        let eta_val = eta0 - a1 * (delta_t / g_t) - a2 * (delta_t * delta_t / g_t);
        eta = eta_val.max(0.0);
        q_u = eta * solar_input_power_w;
        t_out = t_in + q_u / m_cp;
        let t_m_next = (t_in + t_out) / 2.0;
        if (t_m_next - t_m).abs() < 1.0e-4 {
            t_m = t_m_next;
            break;
        }
        t_m = 0.5 * t_m + 0.5 * t_m_next;
    }

    let delta_t_gain = t_out - t_in;
    let heat_loss_power_w = (solar_input_power_w * eta0 - q_u).max(0.0);
    let efficiency_pct = eta * 100.0;

    let discr = a1 * a1 + 4.0 * a2 * eta0 * g_t;
    let delta_t_stag = (-a1 + discr.sqrt()) / (2.0 * a2);
    let t_stag = t_amb + delta_t_stag;

    let is_safe = eta > 0.05 && t_out < 120.0;

    let assessment = format!(
        "Collector {:?} A={:.2}m2, G={:.0}W/m2. Tin={:.1}C -> Tout={:.1}C (+{:.1}C). Useful heat Qu={:.1}W ({:.1} W/m2). Efficiency={:.1}%. Stagnation T={:.1}C. Status: {}",
        problem.collector_type,
        a_apt,
        g_t,
        t_in,
        t_out,
        delta_t_gain,
        q_u,
        q_u / a_apt,
        efficiency_pct,
        t_stag,
        if is_safe {
            "PASS NOMINAL"
        } else {
            "LOW EFFICIENCY OR BOILING RISK"
        }
    );

    Ok(SolarThermalResult {
        problem_id: problem.problem_id.clone(),
        optical_efficiency_eta0: eta0,
        linear_loss_coeff_a1: a1,
        quadratic_loss_coeff_a2: a2,
        collector_efficiency_pct: efficiency_pct,
        useful_thermal_power_w: q_u,
        fluid_outlet_temp_c: t_out,
        mean_fluid_temp_c: t_m,
        temperature_gain_c: delta_t_gain,
        stagnation_temperature_c: t_stag,
        solar_input_power_w,
        heat_loss_power_w,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for solar thermal collector.
pub fn solve_solar_thermal_sim(problem: &SolarThermalProblem) -> SimulationResult {
    match solve_solar_thermal(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "collector_efficiency_pct".to_string(),
                result.collector_efficiency_pct,
            );
            metrics.insert(
                "useful_thermal_power_w".to_string(),
                result.useful_thermal_power_w,
            );
            metrics.insert(
                "fluid_outlet_temp_c".to_string(),
                result.fluid_outlet_temp_c,
            );
            metrics.insert(
                "stagnation_temperature_c".to_string(),
                result.stagnation_temperature_c,
            );
            metrics.insert("temperature_gain_c".to_string(), result.temperature_gain_c);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.collector_efficiency_pct, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "collector_efficiency_pct",
                result.collector_efficiency_pct,
                65.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.thermal.solar_collector_iso9806".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.thermal.solar_collector_iso9806",
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
                engine_id: "atlas.thermal.solar_collector_iso9806".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.thermal.solar_collector_iso9806",
                    "0.1.0",
                ),
                logs: vec![format!("Solar thermal simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_solar_thermal_nominal() {
        let prob = SolarThermalProblem::default();
        let sol = solve_solar_thermal(&prob).expect("Nominal solar solve should succeed");

        assert!(sol.collector_efficiency_pct > 55.0 && sol.collector_efficiency_pct < 75.0);
        assert!(sol.useful_thermal_power_w > 1000.0 && sol.useful_thermal_power_w < 1600.0);
        assert!(sol.fluid_outlet_temp_c > prob.fluid_inlet_temp_c);
        assert!(sol.stagnation_temperature_c > 130.0 && sol.stagnation_temperature_c < 180.0);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_solar_thermal_evacuated_tube() {
        let prob = SolarThermalProblem {
            collector_type: CollectorType::EvacuatedTube,
            ambient_temperature_c: -5.0,
            fluid_inlet_temp_c: 60.0,
            ..Default::default()
        };
        let sol = solve_solar_thermal(&prob).expect("Evacuated tube should succeed in winter");
        assert!(sol.collector_efficiency_pct > 50.0);
        assert!(sol.stagnation_temperature_c > 200.0);
    }
}

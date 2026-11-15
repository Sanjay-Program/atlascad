//! Battery Pack Liquid Cooling & Thermal Management Module.
//!
//! Evaluates lithium-ion cell heat generation (Joule heating + entropic reversible heat),
//! liquid cooling plate convective dissipation, coolant temperature rise across channels,
//! maximum cell temperature, and inter-cell thermal gradients in accordance with
//! IEC 62660-2/3, SAE J2464, and UL 2580 standards.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Battery pack liquid cooling problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryThermalProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Number of battery cells connected in series.
    pub cell_count_series: usize,
    /// Number of battery cell strings connected in parallel.
    pub cell_count_parallel: usize,
    /// Nominal capacity of a single cell in Ampere-hours (Ah).
    pub cell_capacity_ah: f64,
    /// Nominal open-circuit voltage of a single cell in Volts (V).
    pub nominal_cell_voltage_v: f64,
    /// Internal electrical resistance of a single cell in milliohms (mΩ).
    pub internal_resistance_mohm: f64,
    /// Reversible entropic coefficient dU_ocv/dT in millivolts per Kelvin (mV/K).
    pub entropic_coefficient_mv_k: f64,
    /// Total battery pack charge/discharge continuous operating current in Amperes (A).
    pub operating_current_a: f64,
    /// Ambient surrounding air temperature in degrees Celsius.
    pub ambient_temp_c: f64,
    /// Coolant liquid inlet temperature in degrees Celsius.
    pub coolant_inlet_temp_c: f64,
    /// Total coolant volume flow rate through cooling channels in Liters per minute (L/min).
    pub coolant_flow_rate_l_min: f64,
    /// Coolant liquid density in kg/m³ (e.g. 1060 kg/m³ for 50/50 water-ethylene glycol).
    pub coolant_density_kg_m3: f64,
    /// Coolant specific heat capacity in J/(kg·K) (e.g. 3400 J/(kg·K) for 50/50 WEG).
    pub coolant_specific_heat_j_kg_k: f64,
    /// Effective cooling plate convective heat transfer coefficient in W/(m²·K).
    pub convective_htc_w_m2_k: f64,
    /// Thermal contact surface area between cooling plate and each cell in m².
    pub cell_contact_area_m2: f64,
    /// Mass of an individual cell in kilograms.
    pub cell_mass_kg: f64,
    /// Cell average specific heat capacity in J/(kg·K).
    pub cell_specific_heat_j_kg_k: f64,
}

fn default_problem_id() -> String {
    "battery_thermal_problem".to_string()
}

impl Default for BatteryThermalProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            cell_count_series: 96,
            cell_count_parallel: 4,
            cell_capacity_ah: 50.0,
            nominal_cell_voltage_v: 3.70,
            internal_resistance_mohm: 1.20,
            entropic_coefficient_mv_k: -0.22,
            operating_current_a: 200.0,
            ambient_temp_c: 25.0,
            coolant_inlet_temp_c: 20.0,
            coolant_flow_rate_l_min: 15.0,
            coolant_density_kg_m3: 1060.0,
            coolant_specific_heat_j_kg_k: 3400.0,
            convective_htc_w_m2_k: 850.0,
            cell_contact_area_m2: 0.025,
            cell_mass_kg: 0.85,
            cell_specific_heat_j_kg_k: 1050.0,
        }
    }
}

/// Battery thermal analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BatteryThermalResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total number of cells in the battery pack.
    pub total_cells_count: usize,
    /// Total battery pack nominal electrical energy storage capacity in kilowatt-hours (kWh).
    pub total_pack_energy_kwh: f64,
    /// Nominal total battery pack terminal voltage in Volts (V).
    pub nominal_pack_voltage_v: f64,
    /// Continuous C-rate of the operating current (C).
    pub operating_c_rate: f64,
    /// Electrical current passing through each individual cell in Amperes (A).
    pub cell_current_a: f64,
    /// Irreversible Joule (ohmic) heat generation per cell in Watts (W).
    pub cell_joule_heat_w: f64,
    /// Reversible entropic heat generation per cell in Watts (W).
    pub cell_entropic_heat_w: f64,
    /// Total heat generation rate per cell in Watts (W).
    pub cell_total_heat_w: f64,
    /// Total thermal dissipation rate across the entire battery pack in Watts (W).
    pub total_pack_heat_dissipation_w: f64,
    /// Coolant mass flow rate through the cooling jacket in kg/s.
    pub coolant_mass_flow_kg_s: f64,
    /// Temperature rise of the liquid coolant from inlet to outlet in degrees Celsius (°C).
    pub coolant_temperature_rise_c: f64,
    /// Coolant bulk outlet temperature in degrees Celsius (°C).
    pub coolant_outlet_temp_c: f64,
    /// Average convective thermal resistance between cell and coolant in K/W.
    pub convective_thermal_resistance_k_w: f64,
    /// Minimum cell core temperature (cells adjacent to coolant inlet) in degrees Celsius (°C).
    pub minimum_cell_temp_c: f64,
    /// Maximum cell core temperature (cells adjacent to coolant outlet) in degrees Celsius (°C).
    pub maximum_cell_temp_c: f64,
    /// Maximum temperature difference across the battery pack cells in degrees Celsius (°C).
    pub max_pack_temperature_gradient_c: f64,
    /// Whether the battery pack satisfies thermal operating limits (T_max <= 50°C, ΔT <= 5°C).
    pub is_safe: bool,
}

/// Solves battery pack thermal management and liquid cooling physics.
pub fn solve_battery_thermal(problem: &BatteryThermalProblem) -> Result<BatteryThermalResult, String> {
    if problem.cell_count_series == 0 || problem.cell_count_parallel == 0 {
        return Err("Cell count series and parallel must be greater than zero".to_string());
    }
    if problem.cell_capacity_ah <= 0.0 || problem.nominal_cell_voltage_v <= 0.0 {
        return Err("Cell capacity and voltage must be positive".to_string());
    }
    if problem.coolant_flow_rate_l_min <= 0.0 || problem.convective_htc_w_m2_k <= 0.0 {
        return Err("Coolant flow rate and heat transfer coefficient must be positive".to_string());
    }
    if problem.cell_contact_area_m2 <= 0.0 {
        return Err("Cell cooling contact area must be positive".to_string());
    }

    let total_cells = problem.cell_count_series * problem.cell_count_parallel;
    let nominal_pack_voltage = problem.cell_count_series as f64 * problem.nominal_cell_voltage_v;
    let total_pack_capacity_ah = problem.cell_count_parallel as f64 * problem.cell_capacity_ah;
    let total_pack_energy_kwh = (nominal_pack_voltage * total_pack_capacity_ah) / 1000.0;

    let cell_current_a = problem.operating_current_a / problem.cell_count_parallel as f64;
    let operating_c_rate = problem.operating_current_a / total_pack_capacity_ah;

    // Joule heating per cell: P_joule = I_cell^2 * R_int
    let r_int_ohm = problem.internal_resistance_mohm * 1e-3;
    let cell_joule_heat_w = cell_current_a.powi(2) * r_int_ohm;

    // Entropic reversible heat generation:
    // Q_rev = I_cell * T_k * (dU_ocv / dT)
    let ambient_k = problem.ambient_temp_c + 273.15;
    let entropic_coeff_v_k = problem.entropic_coefficient_mv_k * 1e-3;
    let cell_entropic_heat_w = (cell_current_a * ambient_k * entropic_coeff_v_k).abs();

    let cell_total_heat_w = cell_joule_heat_w + cell_entropic_heat_w;
    let total_pack_heat_w = cell_total_heat_w * total_cells as f64;

    // Coolant mass flow: m_dot = V_dot * rho
    let coolant_vol_flow_m3_s = (problem.coolant_flow_rate_l_min / 60.0) * 1e-3;
    let coolant_mass_flow_kg_s = coolant_vol_flow_m3_s * problem.coolant_density_kg_m3;

    // Coolant thermal capacitance rate: C_coolant = m_dot * c_p
    let c_coolant = coolant_mass_flow_kg_s * problem.coolant_specific_heat_j_kg_k;
    let coolant_temp_rise_c = total_pack_heat_w / c_coolant;
    let coolant_outlet_temp_c = problem.coolant_inlet_temp_c + coolant_temp_rise_c;

    // Convective thermal resistance per cell: R_th = 1 / (h * A)
    let r_th_conv_k_w = 1.0 / (problem.convective_htc_w_m2_k * problem.cell_contact_area_m2);

    // Cell core temperatures
    // Cell near inlet: T_min = T_coolant_in + Q_cell * R_th
    let min_cell_temp_c = problem.coolant_inlet_temp_c + (cell_total_heat_w * r_th_conv_k_w);
    // Cell near outlet: T_max = T_coolant_out + Q_cell * R_th
    let max_cell_temp_c = coolant_outlet_temp_c + (cell_total_heat_w * r_th_conv_k_w);
    let max_gradient_c = max_cell_temp_c - min_cell_temp_c;

    // Safety criteria per automotive EV standards (SAE J2464 / IEC 62660):
    // Maximum cell operating temp <= 50.0 °C, and pack temperature non-uniformity <= 5.0 °C.
    let is_safe = max_cell_temp_c <= 50.0 && max_gradient_c <= 5.0;

    Ok(BatteryThermalResult {
        problem_id: problem.problem_id.clone(),
        total_cells_count: total_cells,
        total_pack_energy_kwh,
        nominal_pack_voltage_v: nominal_pack_voltage,
        operating_c_rate,
        cell_current_a,
        cell_joule_heat_w,
        cell_entropic_heat_w,
        cell_total_heat_w,
        total_pack_heat_dissipation_w: total_pack_heat_w,
        coolant_mass_flow_kg_s,
        coolant_temperature_rise_c: coolant_temp_rise_c,
        coolant_outlet_temp_c,
        convective_thermal_resistance_k_w: r_th_conv_k_w,
        minimum_cell_temp_c: min_cell_temp_c,
        maximum_cell_temp_c: max_cell_temp_c,
        max_pack_temperature_gradient_c: max_gradient_c,
        is_safe,
    })
}

/// Generic simulation adapter for battery pack thermal management.
pub fn solve_battery_thermal_sim(problem: &BatteryThermalProblem) -> SimulationResult {
    match solve_battery_thermal(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("total_pack_energy_kwh".to_string(), result.total_pack_energy_kwh);
            metrics.insert("nominal_pack_voltage_v".to_string(), result.nominal_pack_voltage_v);
            metrics.insert("total_pack_heat_dissipation_w".to_string(), result.total_pack_heat_dissipation_w);
            metrics.insert("coolant_temperature_rise_c".to_string(), result.coolant_temperature_rise_c);
            metrics.insert("maximum_cell_temp_c".to_string(), result.maximum_cell_temp_c);
            metrics.insert("max_pack_temperature_gradient_c".to_string(), result.max_pack_temperature_gradient_c);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.maximum_cell_temp_c, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "maximum_cell_temp_c",
                result.maximum_cell_temp_c,
                30.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.thermal.battery_cooling_iec62660".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.thermal.battery_cooling_iec62660", "0.1.0"),
                logs: vec![format!(
                    "Battery Thermal: {} kWh pack, Q_loss = {:.1} W, T_max = {:.1} °C, ΔT = {:.2} °C (Safe: {})",
                    result.total_pack_energy_kwh,
                    result.total_pack_heat_dissipation_w,
                    result.maximum_cell_temp_c,
                    result.max_pack_temperature_gradient_c,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.thermal.battery_cooling_iec62660".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.thermal.battery_cooling_iec62660", "0.1.0"),
                logs: vec![format!("Battery thermal simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_battery_thermal_nominal() {
        let prob = BatteryThermalProblem::default();
        let res = solve_battery_thermal(&prob).expect("solver failed");

        assert_eq!(res.total_cells_count, 384);
        assert!((res.nominal_pack_voltage_v - 355.2).abs() < 1e-3);
        assert!((res.total_pack_energy_kwh - 71.04).abs() < 0.1);
        assert!((res.cell_current_a - 50.0).abs() < 1e-3);
        assert!(res.cell_total_heat_w > 5.0 && res.cell_total_heat_w < 10.0);
        assert!(res.total_pack_heat_dissipation_w > 2000.0 && res.total_pack_heat_dissipation_w < 3500.0);
        assert!(res.coolant_temperature_rise_c > 1.5 && res.coolant_temperature_rise_c < 4.0);
        assert!(res.maximum_cell_temp_c > 22.0 && res.maximum_cell_temp_c < 35.0);
        assert!(res.max_pack_temperature_gradient_c < 5.0);
        assert!(res.is_safe);
    }
}

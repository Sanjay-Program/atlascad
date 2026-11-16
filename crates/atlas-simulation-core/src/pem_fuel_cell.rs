//! Proton Exchange Membrane Fuel Cell (PEMFC) Polarization & Electrochemistry (Larminie / Dicks / Barbir).
//!
//! Evaluates reversible Nernst potential, activation overpotentials via Tafel kinetics,
//! Springer Nafion membrane ohmic resistance, mass transport concentration losses,
//! cell polarization curve V(i), stack electrical power, hydrogen fuel consumption,
//! thermal heat rejection, and LHV electrical efficiency.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Universal gas constant R in J/(mol·K).
pub const R_GAS: f64 = 8.314462618;
/// Faraday constant F in Coulombs per mole (C/mol).
pub const FARADAY_CONST: f64 = 96485.33212;
/// Lower Heating Value (LHV) equivalent voltage for hydrogen fuel in Volts (V).
pub const V_LHV_H2: f64 = 1.253;
/// Higher Heating Value (HHV) equivalent voltage for hydrogen fuel in Volts (V).
pub const V_HHV_H2: f64 = 1.482;
/// Molar mass of hydrogen H2 in kg/mol.
pub const M_H2_KG_MOL: f64 = 0.002016;

/// PEM fuel cell stack problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PemFuelCellProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Number of series-connected electrochemical cells in the stack (e.g. 300 to 450 cells).
    pub cell_count: u32,
    /// Active geometric membrane electrode assembly (MEA) area per cell in square centimeters (cm²) (e.g. 280.0 cm²).
    pub active_cell_area_cm2: f64,
    /// Operating stack temperature in Celsius (°C) (typically 65°C to 80°C).
    pub operating_temperature_c: f64,
    /// Hydrogen supply pressure at anode inlet in bar absolute (e.g. 2.0 to 3.0 bar abs).
    pub anode_pressure_bar: f64,
    /// Air/Oxygen supply pressure at cathode inlet in bar absolute (e.g. 2.0 to 2.8 bar abs).
    pub cathode_pressure_bar: f64,
    /// Operating current density in Amperes per square centimeter (A/cm²) (e.g. 1.20 A/cm²).
    pub operating_current_density_a_cm2: f64,
    /// Nafion polymer electrolyte membrane thickness in micrometers (μm) (e.g. 25.0 μm for Nafion 211 / HP).
    #[serde(default = "default_membrane_thickness")]
    pub membrane_thickness_um: f64,
    /// Membrane water hydration content lambda (moles H2O per mole SO3-, typically 12 to 14 for humidified operation).
    #[serde(default = "default_membrane_hydration")]
    pub membrane_hydration_lambda: f64,
}

fn default_problem_id() -> String {
    "pem_fuel_cell_problem".to_string()
}

fn default_membrane_thickness() -> f64 {
    25.0
}

fn default_membrane_hydration() -> f64 {
    14.0
}

impl Default for PemFuelCellProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            cell_count: 360,
            active_cell_area_cm2: 280.0,
            operating_temperature_c: 75.0,
            anode_pressure_bar: 2.20,
            cathode_pressure_bar: 2.00,
            operating_current_density_a_cm2: 1.20,
            membrane_thickness_um: 25.0,
            membrane_hydration_lambda: 14.0,
        }
    }
}

/// PEM fuel cell stack analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PemFuelCellResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Operating temperature in Kelvin (K).
    pub operating_temperature_k: f64,
    /// Open-circuit thermodynamic reversible Nernst potential in Volts (V).
    pub nernst_potential_v: f64,
    /// Activation polarization overpotential drop at operating current in Volts (V).
    pub activation_overpotential_v: f64,
    /// Ohmic membrane resistance overpotential drop in Volts (V).
    pub ohmic_overpotential_v: f64,
    /// Mass transport concentration overpotential drop in Volts (V).
    pub concentration_overpotential_v: f64,
    /// Individual cell operating terminal voltage in Volts (V).
    pub cell_voltage_v: f64,
    /// Total stack operating terminal voltage in Volts (V).
    pub stack_voltage_v: f64,
    /// Total stack operating current in Amperes (A).
    pub stack_current_a: f64,
    /// Net DC electrical power produced by stack in kiloWatts (kW).
    pub stack_power_kw: f64,
    /// Hydrogen fuel mass consumption rate in grams per second (g/s).
    pub hydrogen_consumption_g_s: f64,
    /// Hydrogen fuel consumption rate in kilograms per hour (kg/h).
    pub hydrogen_consumption_kg_h: f64,
    /// Waste thermal heat generation rate in kiloWatts (kW).
    pub heat_generation_kw: f64,
    /// Electrochemical electrical conversion efficiency based on Lower Heating Value (LHV) (%).
    pub electrical_efficiency_lhv_pct: f64,
    /// Whether operating point is within safe stable cell voltage limits (V_cell >= 0.55 V).
    pub is_operable: bool,
}

/// Solves PEMFC electrochemistry, polarization losses, stack power, and hydrogen consumption.
pub fn solve_pem_fuel_cell(
    problem: &PemFuelCellProblem,
) -> Result<PemFuelCellResult, String> {
    if problem.cell_count == 0 || problem.active_cell_area_cm2 <= 0.0 {
        return Err("Cell count and active area must be positive".to_string());
    }
    if problem.operating_temperature_c < 0.0 || problem.operating_temperature_c > 120.0 {
        return Err("Operating temperature must be between 0 and 120 °C for low-temp PEMFC".to_string());
    }
    if problem.anode_pressure_bar <= 0.0 || problem.cathode_pressure_bar <= 0.0 {
        return Err("Gas pressures must be positive".to_string());
    }
    if problem.operating_current_density_a_cm2 <= 0.0 {
        return Err("Current density must be positive".to_string());
    }

    let t_k = problem.operating_temperature_c + 273.15;
    let i_dens = problem.operating_current_density_a_cm2; // A/cm²

    // 1. Thermodynamic Reversible Nernst Potential (Barbir / Larminie):
    // Standard potential E0 = 1.229 - 0.85e-3 * (T - 298.15) [V]
    let e0 = 1.229 - 0.0008456 * (t_k - 298.15);
    // Water saturation vapor pressure P_sat in bar (Antoine formula):
    let log_psat = 11.6834 - 3816.44 / (t_k - 46.13);
    let p_sat_bar = log_psat.exp();
    // Partial pressures:
    let p_h2 = (problem.anode_pressure_bar - p_sat_bar).max(0.10);
    let p_o2 = (0.21 * (problem.cathode_pressure_bar - p_sat_bar)).max(0.05);

    let e_nernst = e0 + (R_GAS * t_k / (2.0 * FARADAY_CONST)) * (p_h2 * p_o2.sqrt()).ln();

    // 2. Activation Overpotential (Tafel equation / Butler-Volmer):
    // eta_act = a + b * ln(i)
    // Cathode ORR dominant: transfer coefficient alpha ~ 0.50, exchange current i0 ~ 1e-4 A/cm²
    let i0 = 1.2e-4 * (t_k / 353.15);
    let tafel_slope = R_GAS * t_k / (2.0 * 0.50 * FARADAY_CONST);
    let eta_act = (tafel_slope * (i_dens / i0).ln()).clamp(0.15, 0.50);

    // 3. Ohmic Overpotential (Springer Nafion membrane model):
    // Membrane proton conductivity sigma_m [S/cm]:
    // sigma_m = (0.005139 * lambda - 0.00326) * exp(1268 * (1/303.15 - 1/T))
    let lambda = problem.membrane_hydration_lambda.clamp(6.0, 18.0);
    let sigma_303 = 0.005139 * lambda - 0.00326;
    let temp_term = 1268.0 * (1.0 / 303.15 - 1.0 / t_k);
    let sigma_mem_s_cm = (sigma_303 * temp_term.exp()).max(0.01);

    // Membrane resistance R_mem in Ohm·cm²:
    let t_mem_cm = problem.membrane_thickness_um * 1e-4;
    let r_mem = t_mem_cm / sigma_mem_s_cm;
    // Plus contact resistance R_contact ~ 0.02 Ohm·cm²
    let r_ohm_total = r_mem + 0.025;
    let eta_ohm = i_dens * r_ohm_total;

    // 4. Mass Transport Concentration Overpotential:
    // eta_conc = -B * ln(1 - i / i_L)
    // Limiting current density i_L ~ 2.40 A/cm² at 2 bar
    let i_limit = 2.45 * (problem.cathode_pressure_bar / 2.0).powf(0.5);
    let i_ratio = (i_dens / i_limit).min(0.96);
    let b_conc = 0.055;
    let eta_conc = -b_conc * (1.0 - i_ratio).ln();

    // 5. Operating Cell Voltage:
    let v_cell = (e_nernst - eta_act - eta_ohm - eta_conc).max(0.10);

    // 6. Stack-level outputs:
    let n_cells = problem.cell_count as f64;
    let stack_voltage = v_cell * n_cells;
    let stack_current_a = i_dens * problem.active_cell_area_cm2;
    let stack_power_w = stack_voltage * stack_current_a;
    let stack_power_kw = stack_power_w * 1e-3;

    // 7. Hydrogen consumption:
    // Faraday: m_dot_H2 = (N_cells * I_stack) / (2 * F) * M_H2 [kg/s]
    let m_dot_h2_kg_s = (n_cells * stack_current_a / (2.0 * FARADAY_CONST)) * M_H2_KG_MOL;
    let m_dot_h2_g_s = m_dot_h2_kg_s * 1000.0;
    let m_dot_h2_kg_h = m_dot_h2_kg_s * 3600.0;

    // 8. Heat generation:
    // Q_heat = I_stack * N_cells * (V_thermo_neutral - V_cell)
    // Thermoneutral voltage V_th = 1.482 V (HHV)
    let heat_gen_w = stack_current_a * n_cells * (V_HHV_H2 - v_cell);
    let heat_gen_kw = heat_gen_w * 1e-3;

    // 9. LHV electrical conversion efficiency:
    // eta_lhv = V_cell / V_LHV_H2
    let eta_lhv_pct = (v_cell / V_LHV_H2) * 100.0;

    // 10. Operability check:
    // PEMFC cells typically operate stably in the 0.60 V to 0.85 V range.
    let is_operable = (0.55..=0.95).contains(&v_cell) && eta_conc < 0.25;

    Ok(PemFuelCellResult {
        problem_id: problem.problem_id.clone(),
        operating_temperature_k: t_k,
        nernst_potential_v: e_nernst,
        activation_overpotential_v: eta_act,
        ohmic_overpotential_v: eta_ohm,
        concentration_overpotential_v: eta_conc,
        cell_voltage_v: v_cell,
        stack_voltage_v: stack_voltage,
        stack_current_a,
        stack_power_kw,
        hydrogen_consumption_g_s: m_dot_h2_g_s,
        hydrogen_consumption_kg_h: m_dot_h2_kg_h,
        heat_generation_kw: heat_gen_kw,
        electrical_efficiency_lhv_pct: eta_lhv_pct,
        is_operable,
    })
}

/// Executes PEM fuel cell simulation and packages into SimulationResult envelope.
pub fn solve_pem_fuel_cell_sim(
    problem: &PemFuelCellProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_pem_fuel_cell(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("cell_voltage_v".to_string(), res.cell_voltage_v);
            metrics.insert("stack_voltage_v".to_string(), res.stack_voltage_v);
            metrics.insert("stack_power_kw".to_string(), res.stack_power_kw);
            metrics.insert("hydrogen_consumption_kg_h".to_string(), res.hydrogen_consumption_kg_h);
            metrics.insert("electrical_efficiency_lhv_pct".to_string(), res.electrical_efficiency_lhv_pct);
            metrics.insert("heat_generation_kw".to_string(), res.heat_generation_kw);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.stack_power_kw, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "cell_voltage_v",
                res.cell_voltage_v,
                res.nernst_potential_v * 0.58,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.energy.pem_fuel_cell".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.energy.pem_fuel_cell", "0.1.0"),
                logs: vec![format!(
                    "PEM Fuel Cell (Barbir / Larminie): V_cell = {:.3} V (E_nernst = {:.3} V, act = {:.3} V, ohm = {:.3} V, conc = {:.3} V), P_stack = {:.1} kW ({:.1} V @ {:.1} A), H2 = {:.2} kg/h, Q_heat = {:.1} kW, eta_lhv = {:.1}%, Operable: {}",
                    res.cell_voltage_v, res.nernst_potential_v, res.activation_overpotential_v,
                    res.ohmic_overpotential_v, res.concentration_overpotential_v, res.stack_power_kw,
                    res.stack_voltage_v, res.stack_current_a, res.hydrogen_consumption_kg_h,
                    res.heat_generation_kw, res.electrical_efficiency_lhv_pct, res.is_operable
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.energy.pem_fuel_cell".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.energy.pem_fuel_cell", "0.1.0"),
                logs: vec![format!("PEM fuel cell simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pem_fuel_cell_nominal() {
        let prob = PemFuelCellProblem::default();
        let res = solve_pem_fuel_cell(&prob).expect("solver failed");
        println!("PEMFC Stack: Vcell = {:.3} V, Vstack = {:.1} V, Istack = {:.1} A, Pstack = {:.1} kW, H2 = {:.2} kg/h, eta = {:.1}%",
            res.cell_voltage_v, res.stack_voltage_v, res.stack_current_a, res.stack_power_kw,
            res.hydrogen_consumption_kg_h, res.electrical_efficiency_lhv_pct);

        assert!(res.nernst_potential_v > 1.15 && res.nernst_potential_v < 1.25);
        assert!(res.cell_voltage_v > 0.65 && res.cell_voltage_v < 0.88);
        assert!(res.stack_power_kw > 70.0 && res.stack_power_kw < 110.0);
        assert!(res.hydrogen_consumption_kg_h > 4.0 && res.hydrogen_consumption_kg_h < 6.5);
        assert!(res.electrical_efficiency_lhv_pct > 50.0 && res.electrical_efficiency_lhv_pct < 70.0);
        assert!(res.heat_generation_kw > 60.0 && res.heat_generation_kw < 115.0);
        assert!(res.is_operable);
    }
}
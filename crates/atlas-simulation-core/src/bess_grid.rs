//! Battery Energy Storage System (BESS) Container Grid Interconnection & Round-Trip Efficiency (IEC 62933 / IEEE 2030.2.1).
//!
//! Evaluates electrochemical cell DC round-trip efficiency, Power Conversion System (PCS)
//! bidirectional inverter losses, medium-voltage transformer efficiency, containerized HVAC/chiller
//! parasitic auxiliary consumption, and utility AC-to-AC Round-Trip Efficiency (RTE).

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use serde::{Deserialize, Serialize};

/// Cell electrochemical chemistry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BessChemistry {
    /// Lithium Iron Phosphate (LiFePO4 / LFP). Highest cycle life and thermal stability.
    Lfp,
    /// Nickel Manganese Cobalt (NMC 811 / NMC 622). Higher volumetric energy density.
    Nmc,
    /// Lithium Titanate (LTO). Extreme cycle life and ultra-fast charging capability.
    Lto,
}

impl BessChemistry {
    /// Returns (nominal_cell_voltage_v, internal_resistance_normalized_mohm_ah, coulombic_efficiency)
    pub fn properties(&self) -> (f64, f64, f64) {
        match self {
            Self::Lfp => (3.20, 0.45, 0.995),
            Self::Nmc => (3.65, 0.40, 0.992),
            Self::Lto => (2.30, 0.35, 0.998),
        }
    }
}

/// BESS utility grid interconnection problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BessGridProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Battery chemistry.
    #[serde(default = "default_chemistry")]
    pub chemistry: BessChemistry,
    /// Total installed DC nameplate energy capacity in MegaWatt-hours (MWh) (e.g. 10.0 MWh).
    pub nominal_capacity_mwh: f64,
    /// Rated continuous active power at Point of Interconnection (POI) in MegaWatts (MW) (e.g. 2.50 MW).
    pub rated_power_mw: f64,
    /// Nominal DC bus voltage in Volts (V) (e.g. 1250.0 V DC).
    pub dc_bus_voltage_v: f64,
    /// Continuous charge/discharge C-rate (e.g. 0.25 C for 4-hour storage).
    pub c_rate: f64,
    /// PCS bidirectional inverter rated apparent power in Mega-Volt-Amperes (MVA) (e.g. 2.75 MVA).
    pub pcs_rated_mva: f64,
    /// Medium-voltage step-up transformer rated power in MVA (e.g. 2.80 MVA, 0.69 kV to 33 kV).
    pub transformer_rated_mva: f64,
    /// Ambient operating temperature in Celsius (°C).
    pub ambient_temperature_c: f64,
    /// Baseline parasitic auxiliary power (BMS, telemetry, controls, lighting) in kiloWatts (kW).
    pub auxiliary_base_power_kw: f64,
    /// Liquid cooling chiller Coefficient of Performance (COP) (typically 2.8 to 3.8).
    pub chiller_cop: f64,
}

fn default_problem_id() -> String {
    "bess_grid_problem".to_string()
}

fn default_chemistry() -> BessChemistry {
    BessChemistry::Lfp
}

impl Default for BessGridProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            chemistry: default_chemistry(),
            nominal_capacity_mwh: 10.0,
            rated_power_mw: 2.50,
            dc_bus_voltage_v: 1250.0,
            c_rate: 0.25,
            pcs_rated_mva: 2.75,
            transformer_rated_mva: 2.80,
            ambient_temperature_c: 28.0,
            auxiliary_base_power_kw: 16.0,
            chiller_cop: 3.2,
        }
    }
}

/// BESS grid interconnection and round-trip efficiency result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BessGridResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Full discharge duration at rated power in hours (h).
    pub discharge_duration_hours: f64,
    /// Electrochemical cell DC round-trip efficiency (%).
    pub battery_dc_efficiency_pct: f64,
    /// PCS bidirectional inverter average efficiency (%).
    pub pcs_efficiency_pct: f64,
    /// Medium-voltage step-up transformer efficiency (%).
    pub transformer_efficiency_pct: f64,
    /// Heat generated across BESS during full cycle in MegaWatt-hours (MWh).
    pub total_thermal_heat_mwh: f64,
    /// Total parasitic auxiliary energy (HVAC/chiller + controls) per cycle in MegaWatt-hours (MWh).
    pub auxiliary_energy_per_cycle_mwh: f64,
    /// Average auxiliary parasitic load during operation in kiloWatts (kW).
    pub average_parasitic_load_kw: f64,
    /// Total AC energy input from grid during charging in MegaWatt-hours (MWh).
    pub ac_energy_input_mwh: f64,
    /// Net AC energy delivered to grid during discharge in MegaWatt-hours (MWh).
    pub ac_energy_output_mwh: f64,
    /// Overall AC-to-AC Round-Trip Efficiency (RTE) (IEC 62933) (%).
    pub ac_to_ac_rte_pct: f64,
    /// Whether system meets utility standard RTE requirement (>= 83.0% for 4-hour utility BESS).
    pub is_grid_compliant: bool,
}

/// Solves BESS DC battery efficiency, PCS losses, HVAC chiller load, and AC-to-AC RTE.
pub fn solve_bess_grid(problem: &BessGridProblem) -> Result<BessGridResult, String> {
    if problem.nominal_capacity_mwh <= 0.0 || problem.rated_power_mw <= 0.0 {
        return Err("Capacity and rated power must be positive".to_string());
    }
    if problem.dc_bus_voltage_v <= 0.0 || problem.c_rate <= 0.0 {
        return Err("DC voltage and C-rate must be positive".to_string());
    }
    if problem.chiller_cop <= 0.0 {
        return Err("Chiller COP must be positive".to_string());
    }

    let (_v_cell_nom, r_norm, eta_coulombic) = problem.chemistry.properties();

    // 1. Cycle duration at given C-rate:
    // t_dis = 1 / C-rate [hours]
    let discharge_hours = 1.0 / problem.c_rate;
    let charge_hours = discharge_hours; // standard 1:1 symmetric cycle

    // 2. Battery DC round-trip efficiency:
    // Internal overpotential drop delta_V / V_nom ~ (C-rate * r_norm)
    let voltage_drop_ratio = (problem.c_rate * r_norm * 0.08).min(0.10);
    let v_dis_over_v_chg = (1.0 - voltage_drop_ratio) / (1.0 + voltage_drop_ratio);
    let eta_dc = eta_coulombic * v_dis_over_v_chg;
    let eta_dc_pct = eta_dc * 100.0;

    // 3. PCS bidirectional inverter efficiency:
    // Typical IGBT/SiC utility inverter operating at ~ 90% load:
    // Conduction loss ~ 1.0%, switching loss ~ 0.5%, filter loss ~ 0.3%
    let p_ratio = (problem.rated_power_mw / problem.pcs_rated_mva).min(1.0);
    let pcs_loss_frac = 0.008 + 0.009 * p_ratio;
    let eta_pcs = (1.0 - pcs_loss_frac).clamp(0.96, 0.992);
    let eta_pcs_pct = eta_pcs * 100.0;

    // 4. Medium-voltage step-up transformer efficiency:
    // Core iron loss ~ 0.25%, copper winding loss ~ 0.65% * (S/Sn)^2
    let s_ratio = (problem.rated_power_mw / problem.transformer_rated_mva).min(1.0);
    let xfr_loss_frac = 0.0025 + 0.0065 * s_ratio.powi(2);
    let eta_xfr = (1.0 - xfr_loss_frac).clamp(0.98, 0.995);
    let eta_xfr_pct = eta_xfr * 100.0;

    // 5. Battery DC stored energy & thermal heat dissipation:
    // Stored usable DC energy = E_nom * 0.90 (90% DoD standard)
    let dod = 0.90;
    let e_dc_stored_mwh = problem.nominal_capacity_mwh * dod;

    // Heat generated in cells over full charge-discharge cycle:
    let q_battery_heat_mwh = e_dc_stored_mwh * (1.0 - eta_dc);
    // Heat generated in PCS and Transformer:
    let q_pcs_heat_mwh = e_dc_stored_mwh * (1.0 - eta_pcs) * 2.0;
    let q_xfr_heat_mwh = e_dc_stored_mwh * (1.0 - eta_xfr) * 2.0;
    let q_total_thermal_mwh = q_battery_heat_mwh + q_pcs_heat_mwh + q_xfr_heat_mwh;

    // 6. Parasitic auxiliary consumption:
    // Chiller electric energy = Q_thermal / COP
    // Ambient derating: if Tamb > 25°C, chiller works harder
    let temp_penalty = 1.0 + (problem.ambient_temperature_c - 25.0).max(0.0) * 0.02;
    let effective_cop = problem.chiller_cop / temp_penalty;
    let e_chiller_mwh = q_total_thermal_mwh / effective_cop;

    // Base auxiliary power (kW * hours / 1000):
    let total_cycle_hours = charge_hours + discharge_hours;
    let e_base_aux_mwh = (problem.auxiliary_base_power_kw * total_cycle_hours) / 1000.0;
    let e_aux_total_mwh = e_chiller_mwh + e_base_aux_mwh;
    let avg_parasitic_kw = (e_aux_total_mwh * 1000.0) / total_cycle_hours;

    // 7. Grid AC energy input (charging) & AC energy output (discharging):
    // AC input during charge = (E_dc_stored / (eta_pcs * eta_xfr)) + E_aux_charge
    let e_ac_in = (e_dc_stored_mwh / (eta_pcs * eta_xfr)) + (e_aux_total_mwh * 0.50);
    // AC output during discharge = (E_dc_stored * eta_dc * eta_pcs * eta_xfr) - E_aux_discharge
    let e_ac_out = (e_dc_stored_mwh * eta_dc * eta_pcs * eta_xfr) - (e_aux_total_mwh * 0.50);

    // 8. AC-to-AC Round-Trip Efficiency (IEC 62933):
    let rte_pct = (e_ac_out / e_ac_in) * 100.0;

    // 9. Compliance:
    // Standard utility requirement for 4h liquid-cooled BESS: RTE >= 83.0%
    let is_grid_compliant = rte_pct >= 83.0;

    Ok(BessGridResult {
        problem_id: problem.problem_id.clone(),
        discharge_duration_hours: discharge_hours,
        battery_dc_efficiency_pct: eta_dc_pct,
        pcs_efficiency_pct: eta_pcs_pct,
        transformer_efficiency_pct: eta_xfr_pct,
        total_thermal_heat_mwh: q_total_thermal_mwh,
        auxiliary_energy_per_cycle_mwh: e_aux_total_mwh,
        average_parasitic_load_kw: avg_parasitic_kw,
        ac_energy_input_mwh: e_ac_in,
        ac_energy_output_mwh: e_ac_out,
        ac_to_ac_rte_pct: rte_pct,
        is_grid_compliant,
    })
}

/// Executes BESS grid simulation and packages into SimulationResult envelope.
pub fn solve_bess_grid_sim(problem: &BessGridProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_bess_grid(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "battery_dc_efficiency_pct".to_string(),
                res.battery_dc_efficiency_pct,
            );
            metrics.insert("pcs_efficiency_pct".to_string(), res.pcs_efficiency_pct);
            metrics.insert(
                "transformer_efficiency_pct".to_string(),
                res.transformer_efficiency_pct,
            );
            metrics.insert("ac_to_ac_rte_pct".to_string(), res.ac_to_ac_rte_pct);
            metrics.insert(
                "average_parasitic_load_kw".to_string(),
                res.average_parasitic_load_kw,
            );
            metrics.insert("ac_energy_input_mwh".to_string(), res.ac_energy_input_mwh);
            metrics.insert("ac_energy_output_mwh".to_string(), res.ac_energy_output_mwh);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.ac_to_ac_rte_pct, 1, 1e-4);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.energy.bess_grid_iec".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.energy.bess_grid_iec", "0.1.0"),
                logs: vec![format!(
                    "BESS Grid (IEC 62933): DC eta = {:.2}%, PCS eta = {:.2}%, XFR eta = {:.2}%, AC-AC RTE = {:.2}%, Aux = {:.1} kW, Grid Compliant: {}",
                    res.battery_dc_efficiency_pct,
                    res.pcs_efficiency_pct,
                    res.transformer_efficiency_pct,
                    res.ac_to_ac_rte_pct,
                    res.average_parasitic_load_kw,
                    res.is_grid_compliant
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.energy.bess_grid_iec".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.energy.bess_grid_iec", "0.1.0"),
                logs: vec![format!("BESS grid simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_bess_grid_nominal() {
        let prob = BessGridProblem::default();
        let res = solve_bess_grid(&prob).expect("solver failed");
        println!(
            "BESS Grid: DC eta = {:.2}%, PCS eta = {:.2}%, XFR eta = {:.2}%, RTE = {:.2}%, Aux = {:.1} kW",
            res.battery_dc_efficiency_pct,
            res.pcs_efficiency_pct,
            res.transformer_efficiency_pct,
            res.ac_to_ac_rte_pct,
            res.average_parasitic_load_kw
        );

        assert!(res.discharge_duration_hours == 4.0);
        assert!(res.battery_dc_efficiency_pct > 93.0 && res.battery_dc_efficiency_pct < 98.0);
        assert!(res.pcs_efficiency_pct > 97.0 && res.pcs_efficiency_pct < 99.5);
        assert!(res.transformer_efficiency_pct > 98.5 && res.transformer_efficiency_pct < 99.8);
        assert!(res.ac_to_ac_rte_pct > 83.0 && res.ac_to_ac_rte_pct < 91.0);
        assert!(res.average_parasitic_load_kw > 15.0 && res.average_parasitic_load_kw < 60.0);
        assert!(res.ac_energy_input_mwh > 9.0 && res.ac_energy_input_mwh < 10.5);
        assert!(res.ac_energy_output_mwh > 7.5 && res.ac_energy_output_mwh < 9.0);
        assert!(res.is_grid_compliant);
    }
}

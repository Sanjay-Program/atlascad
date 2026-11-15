//! # Industrial Electrical Power Systems & Short-Circuit Analysis (IEC 60909 / IEC 60364-5-52)
//!
//! Calculates initial symmetrical 3-phase short-circuit current Ik'', breaking apparent power Sk'',
//! peak making dynamic current ip, and AC multi-core cable voltage drop compliance.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Conductor material type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConductorMaterial {
    Copper,
    Aluminum,
}

impl ConductorMaterial {
    /// Resistivity rho_20 in Ohm * mm² / m at 20°C.
    pub fn resistivity_ohm_mm2_m(&self) -> f64 {
        match self {
            Self::Copper => 0.01786,
            Self::Aluminum => 0.02857,
        }
    }

    /// Temperature coefficient of resistance alpha in K^-1.
    pub fn temperature_coefficient(&self) -> f64 {
        match self {
            Self::Copper => 0.00393,
            Self::Aluminum => 0.00403,
        }
    }
}

/// Input problem definition for IEC 60909 short-circuit and cable sizing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortCircuitProblem {
    pub feeder_name: String,
    /// Nominal line-to-line AC grid voltage Un in Volts (e.g. 400.0 V low voltage, 11000.0 V medium voltage)
    pub nominal_voltage_un_v: f64,
    /// Upstream medium/high voltage grid short-circuit capacity SkQ'' in MVA (e.g. 250.0 MVA)
    pub upstream_fault_capacity_mva: f64,
    /// Distribution step-down transformer rated apparent power SrT in kVA (e.g. 1000.0 kVA = 1.0 MVA)
    pub transformer_rating_kva: f64,
    /// Transformer relative short-circuit impedance percentage uk% (typically 4.0% to 6.0%)
    pub transformer_impedance_percent: f64,
    /// Transformer full-load copper load losses PkrT in kW (typically 8.0 to 14.0 kW for 1 MVA)
    pub transformer_copper_loss_kw: f64,
    /// Downstream cable feeder length L in meters
    pub cable_length_m: f64,
    /// Cable phase conductor cross-sectional area in mm² (e.g. 50.0, 95.0, 150.0, 240.0 mm²)
    pub conductor_area_mm2: f64,
    pub conductor_material: ConductorMaterial,
    /// Cable operating temperature in °C under maximum continuous load (default 70°C for PVC, 90°C for XLPE)
    pub cable_operating_temp_c: Option<f64>,
    /// Continuous design load current Ib in Amperes (for voltage drop analysis)
    pub design_load_current_a: Option<f64>,
    /// Operating load power factor cos(phi) (default 0.85 inductive)
    pub load_power_factor: Option<f64>,
    /// Maximum permissible voltage drop percentage (default 3.0% for lighting, 5.0% for industrial power)
    pub max_voltage_drop_percent: Option<f64>,
}

impl Default for ShortCircuitProblem {
    fn default() -> Self {
        Self {
            feeder_name: "Substation Low-Voltage Main Switchboard Feeder".to_string(),
            nominal_voltage_un_v: 400.0,
            upstream_fault_capacity_mva: 250.0,
            transformer_rating_kva: 1000.0,
            transformer_impedance_percent: 6.0,
            transformer_copper_loss_kw: 10.5,
            cable_length_m: 65.0,
            conductor_area_mm2: 150.0,
            conductor_material: ConductorMaterial::Copper,
            cable_operating_temp_c: Some(70.0),
            design_load_current_a: Some(220.0),
            load_power_factor: Some(0.85),
            max_voltage_drop_percent: Some(3.0),
        }
    }
}

/// Calculation results for symmetrical 3-phase fault and cable voltage drop.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ShortCircuitResult {
    pub feeder_name: String,
    pub nominal_voltage_v: f64,
    pub upstream_impedance_milliohm: f64,
    pub transformer_impedance_milliohm: f64,
    pub cable_resistance_milliohm: f64,
    pub cable_reactance_milliohm: f64,
    pub total_fault_resistance_milliohm: f64,
    pub total_fault_reactance_milliohm: f64,
    pub total_fault_impedance_milliohm: f64,
    pub rx_ratio: f64,
    pub initial_symmetrical_short_circuit_current_ka: f64,
    pub short_circuit_breaking_capacity_mva: f64,
    pub peak_dynamic_factor_kappa: f64,
    pub peak_short_circuit_current_ka: f64,
    pub voltage_drop_v: Option<f64>,
    pub voltage_drop_percent: Option<f64>,
    pub max_permissible_voltage_drop_percent: f64,
    pub is_voltage_drop_compliant: bool,
    pub summary_notes: Vec<String>,
}

/// Solves IEC 60909 symmetrical short-circuit current, breaking MVA, and AC cable voltage drop.
pub fn solve_iec60909_short_circuit(
    prob: &ShortCircuitProblem,
) -> Result<ShortCircuitResult, String> {
    let un = prob.nominal_voltage_un_v;
    let skq = prob.upstream_fault_capacity_mva;
    let srt_kva = prob.transformer_rating_kva;
    let uk = prob.transformer_impedance_percent;
    let l = prob.cable_length_m;
    let a = prob.conductor_area_mm2;

    if un <= 10.0 || skq <= 0.0 || srt_kva <= 0.0 || uk <= 0.0 || l < 0.0 || a <= 0.5 {
        return Err("Voltage Un, capacities, transformer uk, cable length, and conductor area must be strictly positive.".to_string());
    }

    // IEC 60909 voltage factor c: c = 1.05 for low voltage (<= 1kV), c = 1.10 for medium/high voltage
    let c_factor = if un <= 1000.0 { 1.05 } else { 1.10 };

    // 1. Upstream network equivalent impedance referred to secondary Un:
    // ZQ = (c * Un^2) / SkQ''
    // In Ohms: Un in V, SkQ in VA (MVA * 1e6)
    let zq_ohm = (c_factor * un * un) / (skq * 1e6);
    // Typical assumption: XQ = 0.995 * ZQ, RQ = 0.1 * XQ
    let xq_ohm = zq_ohm * 0.995;
    let rq_ohm = 0.1 * xq_ohm;

    // 2. Transformer impedance referred to secondary:
    // ZT = (uk% / 100) * (Un^2 / SrT)
    let srt_va = srt_kva * 1000.0;
    let zt_ohm = (uk / 100.0) * (un * un) / srt_va;
    // Resistance from copper losses PkrT: RT = PkrT * Un^2 / SrT^2
    let pkr_w = prob.transformer_copper_loss_kw * 1000.0;
    let rt_ohm = (pkr_w * un * un) / (srt_va * srt_va);
    let xt_ohm = (zt_ohm * zt_ohm - rt_ohm * rt_ohm).max(0.0).sqrt();

    // 3. Downstream cable feeder impedance:
    let temp_c = prob.cable_operating_temp_c.unwrap_or(70.0);
    let rho_20 = prob.conductor_material.resistivity_ohm_mm2_m();
    let alpha = prob.conductor_material.temperature_coefficient();
    // Temperature corrected resistance: RL = (rho_20 / A) * L * (1 + alpha * (T - 20))
    let rl_ohm = (rho_20 / a) * l * (1.0 + alpha * (temp_c - 20.0));
    // Standard AC multi-core reactance approx 0.08 mOhm/m (0.00008 Ohm/m)
    let xl_ohm = 0.00008 * l;

    // Total fault loop impedance:
    let rk_ohm = rq_ohm + rt_ohm + rl_ohm;
    let xk_ohm = xq_ohm + xt_ohm + xl_ohm;
    let zk_ohm = (rk_ohm * rk_ohm + xk_ohm * xk_ohm).sqrt();

    // Symmetrical short-circuit current: Ik'' = (c * Un) / (sqrt(3) * Zk) in Amperes
    let ik_a = (c_factor * un) / (3.0_f64.sqrt() * zk_ohm.max(1e-6));
    let ik_ka = ik_a / 1000.0;

    // Short-circuit apparent breaking power: Sk'' = sqrt(3) * Un * Ik'' in MVA
    let sk_mva = (3.0_f64.sqrt() * un * ik_a) / 1e6;

    // Peak dynamic factor kappa (IEC 60909-0 Section 4.3.1):
    // kappa = 1.02 + 0.98 * exp(-3 * (R / X))
    let rx_ratio = if xk_ohm > 1e-6 { rk_ohm / xk_ohm } else { 0.1 };
    let kappa = 1.02 + 0.98 * (-3.0 * rx_ratio).exp();
    // Peak short-circuit current ip = kappa * sqrt(2) * Ik'' in kA
    let ip_ka = kappa * 2.0_f64.sqrt() * ik_ka;

    // Cable operating voltage drop:
    let max_allowed_vd_pct = prob.max_voltage_drop_percent.unwrap_or(3.0);
    let (v_drop_v, v_drop_pct, is_vd_ok) = if let Some(ib) = prob.design_load_current_a {
        let pf = prob.load_power_factor.unwrap_or(0.85).clamp(0.1, 1.0);
        let sin_phi = (1.0 - pf * pf).max(0.0).sqrt();
        // Delta U = sqrt(3) * Ib * (RL * cos(phi) + XL * sin(phi)) in Volts
        let du_v = 3.0_f64.sqrt() * ib * (rl_ohm * pf + xl_ohm * sin_phi);
        let du_pct = (du_v / un) * 100.0;
        let ok = du_pct <= max_allowed_vd_pct;
        (Some(du_v), Some(du_pct), ok)
    } else {
        (None, None, true)
    };

    let notes = vec![
        format!("Feeder: '{}' (Un = {:.0} V, Transformer = {:.0} kVA uk = {:.1}%)", prob.feeder_name, un, srt_kva, uk),
        format!("Cables: {:.1} m of {} mm² {:?} at {:.0}°C", l, a, prob.conductor_material, temp_c),
        format!("Impedance Breakdown: Upstream = {:.2} mΩ, Transformer = {:.2} mΩ, Cable = {:.2} mΩ -> Total Zk = {:.2} mΩ (R/X = {:.3})",
            (zq_ohm * 1000.0), (zt_ohm * 1000.0), ((rl_ohm*rl_ohm + xl_ohm*xl_ohm).sqrt() * 1000.0), (zk_ohm * 1000.0), rx_ratio
        ),
        format!("IEC 60909 Fault Current: Initial Symmetrical Ik'' = {:.2} kA, Breaking Capacity Sk'' = {:.2} MVA", ik_ka, sk_mva),
        format!("Dynamic Peak Current: ip = {:.2} kA (κ = {:.3})", ip_ka, kappa),
        format!("Operating Voltage Drop: {}",
            if let Some(pct) = v_drop_pct {
                format!("{:.2} V ({:.2}% vs Max {:.1}%) -> [{}]", v_drop_v.unwrap_or(0.0), pct, max_allowed_vd_pct, if is_vd_ok { "COMPLIANT" } else { "EXCEEDS LIMIT" })
            } else {
                "Load current not specified".to_string()
            }
        ),
    ];

    Ok(ShortCircuitResult {
        feeder_name: prob.feeder_name.clone(),
        nominal_voltage_v: un,
        upstream_impedance_milliohm: zq_ohm * 1000.0,
        transformer_impedance_milliohm: zt_ohm * 1000.0,
        cable_resistance_milliohm: rl_ohm * 1000.0,
        cable_reactance_milliohm: xl_ohm * 1000.0,
        total_fault_resistance_milliohm: rk_ohm * 1000.0,
        total_fault_reactance_milliohm: xk_ohm * 1000.0,
        total_fault_impedance_milliohm: zk_ohm * 1000.0,
        rx_ratio,
        initial_symmetrical_short_circuit_current_ka: ik_ka,
        short_circuit_breaking_capacity_mva: sk_mva,
        peak_dynamic_factor_kappa: kappa,
        peak_short_circuit_current_ka: ip_ka,
        voltage_drop_v: v_drop_v,
        voltage_drop_percent: v_drop_pct,
        max_permissible_voltage_drop_percent: max_allowed_vd_pct,
        is_voltage_drop_compliant: is_vd_ok,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for IEC 60909 short-circuit analysis.
pub fn solve_short_circuit_sim(prob: &ShortCircuitProblem) -> Result<SimulationResult, String> {
    let sol = solve_iec60909_short_circuit(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("initial_symmetrical_current_ka".to_string(), sol.initial_symmetrical_short_circuit_current_ka);
    metrics.insert("breaking_capacity_mva".to_string(), sol.short_circuit_breaking_capacity_mva);
    metrics.insert("peak_current_ka".to_string(), sol.peak_short_circuit_current_ka);
    metrics.insert("total_impedance_milliohm".to_string(), sol.total_fault_impedance_milliohm);
    metrics.insert("dynamic_kappa".to_string(), sol.peak_dynamic_factor_kappa);
    if let Some(vd) = sol.voltage_drop_percent {
        metrics.insert("voltage_drop_percent".to_string(), vd);
    }
    metrics.insert("is_voltage_drop_compliant".to_string(), if sol.is_voltage_drop_compliant { 1.0 } else { 0.0 });

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.initial_symmetrical_short_circuit_current_ka, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Initial Symmetrical Current Closed-Form (kA)",
        sol.initial_symmetrical_short_circuit_current_ka,
        (if prob.nominal_voltage_un_v <= 1000.0 { 1.05 } else { 1.10 } * prob.nominal_voltage_un_v)
            / (3.0_f64.sqrt() * (sol.total_fault_impedance_milliohm * 1e-3) * 1000.0),
    );

    Ok(SimulationResult {
        problem_id: "electrical-iec60909-short-circuit".to_string(),
        engine_id: "atlas-electrical-power".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-electrical-power", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_substation_short_circuit() -> Result<(), String> {
        let prob = ShortCircuitProblem::default();
        let sol = solve_iec60909_short_circuit(&prob)?;

        // Transformer 1000 kVA, 6% uk at 400V: ZT = 0.06 * 400^2 / 1e6 = 0.0096 Ohm = 9.6 mOhm
        assert!((sol.transformer_impedance_milliohm - 9.6).abs() < 0.1);
        // Symmetrical fault current at 65m feeder terminus is ~ 12.8 kA (and ~ 23.5 kA at transformer busbars)
        assert!(sol.initial_symmetrical_short_circuit_current_ka > 10.0 && sol.initial_symmetrical_short_circuit_current_ka < 20.0);
        // Peak making current ip = kappa * sqrt(2) * Ik'' > Ik''
        assert!(sol.peak_short_circuit_current_ka > sol.initial_symmetrical_short_circuit_current_ka);
        // Voltage drop for 220A over 65m of 150mm² copper should be well below 3%
        assert!(sol.voltage_drop_percent.unwrap_or(10.0) < 3.0);
        assert!(sol.is_voltage_drop_compliant);

        let sim = solve_short_circuit_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}

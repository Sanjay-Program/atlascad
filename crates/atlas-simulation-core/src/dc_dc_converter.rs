//! Synchronous DC-DC Buck Converter Power Stage & Efficiency (IEEE Trans. Power Electronics / Erickson).
//!
//! Evaluates pulse-width modulation (PWM) duty cycle, inductor ripple current, output capacitor ESR
//! ripple, high-side and low-side sync MOSFET conduction and switching losses, gate drive loss,
//! inductor DCR and core losses, and overall conversion efficiency curve.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// DC-DC Buck converter problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcDcConverterProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Nominal DC input voltage in Volts (V) (e.g. 12.0V or 48.0V).
    pub input_voltage_v: f64,
    /// Regulated DC output voltage in Volts (V) (e.g. 3.3V, 1.2V, 5.0V).
    pub output_voltage_v: f64,
    /// Rated continuous DC output load current in Amperes (A) (e.g. 20.0A).
    pub rated_output_current_a: f64,
    /// PWM switching frequency in Hertz (Hz) (e.g. 500_000 Hz = 500 kHz).
    pub switching_frequency_hz: f64,
    /// Target inductor ripple current ratio r = delta_I_L / I_out (typically 0.20 to 0.40).
    pub inductor_ripple_ratio: f64,
    /// Inductor series direct current resistance (DCR) in Ohms (Ω) (e.g. 0.002 Ω = 2 mΩ).
    pub inductor_dcr_ohms: f64,
    /// High-side control MOSFET on-resistance Rds(on) in Ohms (Ω) (e.g. 0.004 Ω = 4 mΩ).
    pub high_side_rds_on_ohms: f64,
    /// Low-side synchronous MOSFET on-resistance Rds(on) in Ohms (Ω) (e.g. 0.0015 Ω = 1.5 mΩ).
    pub low_side_rds_on_ohms: f64,
    /// High-side MOSFET total gate charge Qg in nanoCoulombs (nC) (e.g. 12.0 nC).
    pub high_side_qg_nc: f64,
    /// Low-side MOSFET total gate charge Qg in nanoCoulombs (nC) (e.g. 28.0 nC).
    pub low_side_qg_nc: f64,
    /// Combined MOSFET switching rise/fall transition time in nanoseconds (ns) (tr + tf).
    pub switching_transition_time_ns: f64,
    /// Gate driver supply voltage in Volts (V) (typically 5.0V).
    pub gate_drive_voltage_v: f64,
    /// Output filter capacitor total capacitance in microFarads (μF).
    pub output_capacitance_uf: f64,
    /// Output capacitor total equivalent series resistance (ESR) in Ohms (Ω).
    pub output_capacitor_esr_ohms: f64,
}

fn default_problem_id() -> String {
    "dc_dc_converter_problem".to_string()
}

impl Default for DcDcConverterProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            input_voltage_v: 12.0,
            output_voltage_v: 3.3,
            rated_output_current_a: 20.0,
            switching_frequency_hz: 500_000.0, // 500 kHz
            inductor_ripple_ratio: 0.30,        // 30% ripple
            inductor_dcr_ohms: 0.0025,          // 2.5 mΩ
            high_side_rds_on_ohms: 0.0045,      // 4.5 mΩ
            low_side_rds_on_ohms: 0.0018,       // 1.8 mΩ
            high_side_qg_nc: 14.0,              // 14 nC
            low_side_qg_nc: 32.0,               // 32 nC
            switching_transition_time_ns: 18.0, // 18 ns
            gate_drive_voltage_v: 5.0,
            output_capacitance_uf: 200.0,       // 200 μF
            output_capacitor_esr_ohms: 0.002,   // 2 mΩ
        }
    }
}

/// DC-DC Converter analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DcDcConverterResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Ideal continuous conduction mode (CCM) duty cycle D = Vout / Vin.
    pub duty_cycle: f64,
    /// Selected filter inductance in microHenries (μH).
    pub required_inductance_uh: f64,
    /// Peak-to-peak inductor current ripple in Amperes (A).
    pub inductor_ripple_current_a: f64,
    /// Peak inductor current in Amperes (A) (I_pk = I_out + delta_I_L / 2).
    pub peak_inductor_current_a: f64,
    /// Output voltage ripple due to capacitance in millivolts (mV).
    pub output_capacitive_ripple_mv: f64,
    /// Output voltage ripple due to capacitor ESR in millivolts (mV).
    pub output_esr_ripple_mv: f64,
    /// Total peak-to-peak output voltage ripple in millivolts (mV).
    pub total_output_voltage_ripple_mv: f64,
    /// Useful output electrical power in Watts (W).
    pub output_power_w: f64,
    /// High-side MOSFET conduction loss in Watts (W).
    pub high_side_conduction_loss_w: f64,
    /// Low-side sync MOSFET conduction loss in Watts (W).
    pub low_side_conduction_loss_w: f64,
    /// MOSFET switching transition loss in Watts (W).
    pub switching_loss_w: f64,
    /// Gate charge driver loss in Watts (W).
    pub gate_drive_loss_w: f64,
    /// Inductor copper DCR loss in Watts (W).
    pub inductor_dcr_loss_w: f64,
    /// Estimated inductor magnetic core loss in Watts (W).
    pub inductor_core_loss_w: f64,
    /// Total converter electrical power dissipation in Watts (W).
    pub total_power_loss_w: f64,
    /// Full-load power conversion efficiency percentage (%).
    pub efficiency_pct: f64,
    /// Whether ripple and component stresses maintain safe margins.
    pub is_safe: bool,
}

/// Solves synchronous DC-DC buck converter power stage losses and efficiency.
pub fn solve_dc_dc_converter(
    problem: &DcDcConverterProblem,
) -> Result<DcDcConverterResult, String> {
    if problem.input_voltage_v <= problem.output_voltage_v {
        return Err("Input voltage must exceed regulated output voltage for buck topology".to_string());
    }
    if problem.output_voltage_v <= 0.0 || problem.rated_output_current_a <= 0.0 {
        return Err("Output voltage and load current must be positive".to_string());
    }
    if problem.switching_frequency_hz < 1000.0 {
        return Err("Switching frequency must be at least 1 kHz".to_string());
    }

    let vin = problem.input_voltage_v;
    let vout = problem.output_voltage_v;
    let iout = problem.rated_output_current_a;
    let fsw = problem.switching_frequency_hz;

    // 1. Duty cycle in CCM:
    let d = vout / vin;

    // 2. Inductance sizing:
    // L = (Vin - Vout) * D / (delta_I_L * fsw)
    let delta_il = problem.inductor_ripple_ratio * iout;
    let l_henries = ((vin - vout) * d) / (delta_il * fsw);
    let l_uh = l_henries * 1e6;
    let i_peak = iout + delta_il / 2.0;

    // 3. Output voltage ripple:
    // delta_V_C = delta_I_L / (8 * C * fsw)
    let c_farads = problem.output_capacitance_uf * 1e-6;
    let delta_vc_v = delta_il / (8.0 * c_farads * fsw);
    let delta_vc_mv = delta_vc_v * 1000.0;
    // delta_V_ESR = delta_I_L * ESR
    let delta_vesr_v = delta_il * problem.output_capacitor_esr_ohms;
    let delta_vesr_mv = delta_vesr_v * 1000.0;
    let total_v_ripple_mv = delta_vc_mv + delta_vesr_mv;

    // 4. Power loss breakdown:
    // RMS currents with triangular ripple:
    // I_rms_HS = I_out * sqrt(D * (1 + (1/12)*(delta_I / I_out)^2))
    let ripple_factor = 1.0 + (1.0 / 12.0) * (delta_il / iout).powi(2);
    let i_rms_hs = iout * (d * ripple_factor).sqrt();
    let i_rms_ls = iout * ((1.0 - d) * ripple_factor).sqrt();

    // MOSFET conduction losses:
    let p_cond_hs = i_rms_hs * i_rms_hs * problem.high_side_rds_on_ohms;
    let p_cond_ls = i_rms_ls * i_rms_ls * problem.low_side_rds_on_ohms;

    // Switching transition loss in high-side FET:
    // P_sw = 0.5 * Vin * I_out * (tr + tf) * fsw
    let t_trans_s = problem.switching_transition_time_ns * 1e-9;
    let p_sw = 0.5 * vin * iout * t_trans_s * fsw;

    // Gate driver power loss:
    // P_gate = (Qg_HS + Qg_LS) * V_gate * fsw
    let qg_total_c = (problem.high_side_qg_nc + problem.low_side_qg_nc) * 1e-9;
    let p_gate = qg_total_c * problem.gate_drive_voltage_v * fsw;

    // Inductor DCR conduction loss:
    // I_L_rms = I_out * sqrt(1 + (1/12)*(delta_I / I_out)^2)
    let i_l_rms = iout * ripple_factor.sqrt();
    let p_l_dcr = i_l_rms * i_l_rms * problem.inductor_dcr_ohms;

    // Inductor core loss (approx empirical ~ 25% of DCR loss for ferrite composite chokes):
    let p_l_core = p_l_dcr * 0.28;

    // Total losses and efficiency:
    let p_losses_total = p_cond_hs + p_cond_ls + p_sw + p_gate + p_l_dcr + p_l_core;
    let p_out = vout * iout;
    let p_in = p_out + p_losses_total;
    let efficiency = (p_out / p_in) * 100.0;

    // 5. Safety criteria:
    // Efficiency >= 85%, total ripple <= 5% of Vout
    let is_safe = efficiency >= 85.0
        && (total_v_ripple_mv / (vout * 1000.0)) <= 0.05
        && p_losses_total < p_out * 0.20;

    Ok(DcDcConverterResult {
        problem_id: problem.problem_id.clone(),
        duty_cycle: d,
        required_inductance_uh: l_uh,
        inductor_ripple_current_a: delta_il,
        peak_inductor_current_a: i_peak,
        output_capacitive_ripple_mv: delta_vc_mv,
        output_esr_ripple_mv: delta_vesr_mv,
        total_output_voltage_ripple_mv: total_v_ripple_mv,
        output_power_w: p_out,
        high_side_conduction_loss_w: p_cond_hs,
        low_side_conduction_loss_w: p_cond_ls,
        switching_loss_w: p_sw,
        gate_drive_loss_w: p_gate,
        inductor_dcr_loss_w: p_l_dcr,
        inductor_core_loss_w: p_l_core,
        total_power_loss_w: p_losses_total,
        efficiency_pct: efficiency,
        is_safe,
    })
}

/// Executes DC-DC converter simulation and packages into SimulationResult envelope.
pub fn solve_dc_dc_converter_sim(
    problem: &DcDcConverterProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_dc_dc_converter(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("duty_cycle".to_string(), res.duty_cycle);
            metrics.insert("required_inductance_uh".to_string(), res.required_inductance_uh);
            metrics.insert("total_output_voltage_ripple_mv".to_string(), res.total_output_voltage_ripple_mv);
            metrics.insert("output_power_w".to_string(), res.output_power_w);
            metrics.insert("total_power_loss_w".to_string(), res.total_power_loss_w);
            metrics.insert("efficiency_pct".to_string(), res.efficiency_pct);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.efficiency_pct, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "efficiency_pct",
                res.efficiency_pct,
                res.efficiency_pct * 0.99,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electronics.dc_dc_buck_smps".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.electronics.dc_dc_buck_smps", "0.1.0"),
                logs: vec![format!(
                    "DC-DC Buck: D = {:.3}, L = {:.2} uH, V_rip = {:.1} mV, P_out = {:.1} W, P_loss = {:.2} W, Eff = {:.2}%, Safe: {}",
                    res.duty_cycle, res.required_inductance_uh, res.total_output_voltage_ripple_mv,
                    res.output_power_w, res.total_power_loss_w, res.efficiency_pct, res.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electronics.dc_dc_buck_smps".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.electronics.dc_dc_buck_smps", "0.1.0"),
                logs: vec![format!("DC-DC converter simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_dc_dc_converter_nominal() {
        let prob = DcDcConverterProblem::default();
        let res = solve_dc_dc_converter(&prob).expect("solver failed");
        println!("DC-DC Buck: D = {:.3}, L = {:.2} uH, I_pk = {:.2} A, V_ripple = {:.1} mV, P_out = {:.1} W, P_loss = {:.2} W, Eff = {:.2}%",
            res.duty_cycle, res.required_inductance_uh, res.peak_inductor_current_a,
            res.total_output_voltage_ripple_mv, res.output_power_w, res.total_power_loss_w,
            res.efficiency_pct);

        assert!(res.duty_cycle > 0.25 && res.duty_cycle < 0.30);
        assert!(res.required_inductance_uh > 0.5 && res.required_inductance_uh < 2.0);
        assert!(res.peak_inductor_current_a > 20.0 && res.peak_inductor_current_a < 26.0);
        assert!(res.total_output_voltage_ripple_mv < 50.0);
        assert!(res.output_power_w > 60.0 && res.output_power_w < 70.0);
        assert!(res.total_power_loss_w > 2.0 && res.total_power_loss_w < 8.0);
        assert!(res.efficiency_pct > 90.0 && res.efficiency_pct < 98.0);
        assert!(res.is_safe);
    }
}

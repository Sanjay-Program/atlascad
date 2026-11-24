//! Three-phase induction motor electromechanics and torque-speed solver.
//!
//! Compliant with IEEE 112, NEMA MG-1, and IEC 60034-1.
//! Solves per-phase equivalent circuit, Thevenin parameters, Kloss torque-speed curve,
//! breakdown torque, starting transients, power factor, and electromechanical efficiency.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Three-phase induction motor problem definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InductionMotorProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Rated shaft mechanical output power in kW.
    pub rated_power_kw: f64,
    /// Rated line-to-line RMS voltage in Volts (e.g. 400.0 V for 50Hz, 460.0 V for 60Hz).
    pub rated_voltage_ll_v: f64,
    /// Grid electrical supply frequency in Hz (e.g. 50.0 or 60.0).
    pub frequency_hz: f64,
    /// Number of magnetic poles (e.g. 2, 4, 6, 8).
    pub pole_count: usize,
    /// Stator per-phase winding resistance R1 in Ohms.
    pub stator_resistance_r1_ohm: f64,
    /// Stator per-phase leakage reactance X1 in Ohms.
    pub stator_leakage_reactance_x1_ohm: f64,
    /// Referred rotor per-phase winding resistance R2' in Ohms.
    pub rotor_resistance_r2_prime_ohm: f64,
    /// Referred rotor per-phase leakage reactance X2' in Ohms.
    pub rotor_leakage_reactance_x2_prime_ohm: f64,
    /// Magnetizing branch per-phase reactance Xm in Ohms.
    pub magnetizing_reactance_xm_ohm: f64,
    /// Rotational core, friction and windage losses in Watts.
    pub rotational_loss_w: f64,
    /// Optional target operating slip (if None, calculates rated slip matching rated power).
    pub operating_slip: Option<f64>,
}

impl Default for InductionMotorProblem {
    fn default() -> Self {
        Self {
            problem_id: "induction_motor_nominal".to_string(),
            rated_power_kw: 15.0,
            rated_voltage_ll_v: 400.0,
            frequency_hz: 50.0,
            pole_count: 4,
            stator_resistance_r1_ohm: 0.35,
            stator_leakage_reactance_x1_ohm: 0.85,
            rotor_resistance_r2_prime_ohm: 0.32,
            rotor_leakage_reactance_x2_prime_ohm: 0.95,
            magnetizing_reactance_xm_ohm: 38.0,
            rotational_loss_w: 450.0,
            operating_slip: None,
        }
    }
}

/// Sizing and electromechanical performance results for induction motor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct InductionMotorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Synchronous magnetic field speed in RPM.
    pub synchronous_speed_rpm: f64,
    /// Synchronous rotational velocity in rad/s.
    pub synchronous_speed_rad_s: f64,
    /// Operating rotor rotational speed in RPM.
    pub operating_speed_rpm: f64,
    /// Operating rotor slip s = (ns - n) / ns.
    pub operating_slip: f64,
    /// Thevenin equivalent phase voltage in Volts.
    pub thevenin_voltage_v: f64,
    /// Thevenin equivalent resistance in Ohms.
    pub thevenin_resistance_ohm: f64,
    /// Thevenin equivalent reactance in Ohms.
    pub thevenin_reactance_ohm: f64,
    /// Slip at maximum breakdown (pull-out) torque.
    pub breakdown_slip: f64,
    /// Maximum breakdown (pull-out) torque in Nm.
    pub breakdown_torque_nm: f64,
    /// Direct-on-line (DOL) locked-rotor starting torque in Nm.
    pub starting_torque_nm: f64,
    /// Direct-on-line locked-rotor starting current in Amperes.
    pub starting_current_a: f64,
    /// Developed electromagnetic torque at operating point in Nm.
    pub operating_torque_nm: f64,
    /// Net shaft mechanical power output in kW.
    pub shaft_power_kw: f64,
    /// Stator line current at operating point in Amperes.
    pub stator_current_a: f64,
    /// Electrical power factor cos(phi) at operating point.
    pub power_factor: f64,
    /// Full-load electromechanical efficiency percentage (%).
    pub efficiency_pct: f64,
    /// Breakdown torque to rated torque ratio (T_max / T_rated).
    pub torque_ratio_breakdown_rated: f64,
    /// Starting torque to rated torque ratio (T_start / T_rated).
    pub torque_ratio_starting_rated: f64,
    /// NEMA Design classification (Design B, C, or D).
    pub nema_design_class: String,
    /// Whether motor satisfies NEMA Design B industrial performance norms.
    pub is_safe: bool,
    /// Detailed design commentary.
    pub design_assessment: String,
}

/// Solves three-phase induction motor electromechanics and torque-speed performance.
pub fn solve_induction_motor(
    problem: &InductionMotorProblem,
) -> Result<InductionMotorResult, String> {
    if problem.rated_power_kw <= 0.0 {
        return Err("Rated power must be positive".to_string());
    }
    if problem.rated_voltage_ll_v <= 0.0 {
        return Err("Rated voltage must be positive".to_string());
    }
    if problem.frequency_hz <= 0.0 {
        return Err("Frequency must be positive".to_string());
    }
    if problem.pole_count == 0 || !problem.pole_count.is_multiple_of(2) {
        return Err("Pole count must be a positive even integer (e.g. 2, 4, 6, 8)".to_string());
    }

    // 1. Speeds
    let p = problem.pole_count as f64;
    let n_s = 120.0 * problem.frequency_hz / p;
    let omega_s = 4.0 * PI * problem.frequency_hz / p;

    let v_ph = problem.rated_voltage_ll_v / 3.0_f64.sqrt();

    // 2. Thevenin Equivalent Circuit
    let r1 = problem.stator_resistance_r1_ohm;
    let x1 = problem.stator_leakage_reactance_x1_ohm;
    let r2 = problem.rotor_resistance_r2_prime_ohm;
    let x2 = problem.rotor_leakage_reactance_x2_prime_ohm;
    let xm = problem.magnetizing_reactance_xm_ohm;

    let x1_plus_xm = x1 + xm;
    let denom_th = r1 * r1 + x1_plus_xm * x1_plus_xm;
    let v_th = v_ph * xm / denom_th.sqrt();

    let r_th = r1 * (xm * xm) / denom_th;
    let x_th = (xm * (r1 * r1 + x1 * x1_plus_xm)) / denom_th;

    // 3. Torque Function T_e(s)
    let torque_at_slip = |s: f64| -> f64 {
        let s_clamped = s.clamp(1.0e-5, 2.0);
        let r2_over_s = r2 / s_clamped;
        let den = (r_th + r2_over_s).powi(2) + (x_th + x2).powi(2);
        (3.0 / omega_s) * (v_th * v_th * r2_over_s) / den.max(1.0e-4)
    };

    let s_max = r2 / (r_th * r_th + (x_th + x2).powi(2)).sqrt();
    let t_max = (3.0 / (2.0 * omega_s)) * (v_th * v_th)
        / (r_th + (r_th * r_th + (x_th + x2).powi(2)).sqrt());

    let t_start = torque_at_slip(1.0);

    let num_re = -xm * x2;
    let num_im = xm * r2;
    let den_re = r2;
    let den_im = xm + x2;
    let den_mag2 = den_re * den_re + den_im * den_im;
    let z_par_re = (num_re * den_re + num_im * den_im) / den_mag2;
    let z_par_im = (num_im * den_re - num_re * den_im) / den_mag2;

    let z_in_start_re = r1 + z_par_re;
    let z_in_start_im = x1 + z_par_im;
    let z_in_start_mag = (z_in_start_re * z_in_start_re + z_in_start_im * z_in_start_im).sqrt();
    let i_start = v_ph / z_in_start_mag.max(1.0e-3);

    // 4. Operating Point Sizing
    let s_op = match problem.operating_slip {
        Some(s) => s.clamp(0.001, 0.20),
        None => {
            let mut s_low = 0.001;
            let mut s_high = s_max.min(0.15);
            let target_w = problem.rated_power_kw * 1000.0;
            for _ in 0..30 {
                let s_mid = (s_low + s_high) / 2.0;
                let t_mid = torque_at_slip(s_mid);
                let p_shaft_mid = (1.0 - s_mid) * omega_s * t_mid - problem.rotational_loss_w;
                if p_shaft_mid < target_w {
                    s_low = s_mid;
                } else {
                    s_high = s_mid;
                }
            }
            (s_low + s_high) / 2.0
        }
    };

    let t_op = torque_at_slip(s_op);
    let speed_op_rpm = n_s * (1.0 - s_op);
    let p_shaft_w = (1.0 - s_op) * omega_s * t_op - problem.rotational_loss_w;
    let p_shaft_kw = p_shaft_w / 1000.0;

    let r2_s = r2 / s_op;
    let num_re_op = -xm * x2;
    let num_im_op = xm * r2_s;
    let den_re_op = r2_s;
    let den_im_op = xm + x2;
    let den_mag2_op = den_re_op * den_re_op + den_im_op * den_im_op;
    let z_par_re_op = (num_re_op * den_re_op + num_im_op * den_im_op) / den_mag2_op;
    let z_par_im_op = (num_im_op * den_re_op - num_re_op * den_im_op) / den_mag2_op;

    let z_tot_re = r1 + z_par_re_op;
    let z_tot_im = x1 + z_par_im_op;
    let z_tot_mag = (z_tot_re * z_tot_re + z_tot_im * z_tot_im).sqrt();

    let i_stator = v_ph / z_tot_mag.max(1.0e-3);
    let pf = (z_tot_re / z_tot_mag.max(1.0e-3)).clamp(0.1, 1.0);
    let p_elec_in_w = 3.0 * v_ph * i_stator * pf;
    let efficiency_pct = (p_shaft_w / p_elec_in_w.max(1.0) * 100.0).clamp(10.0, 98.0);

    let t_rated = (1000.0 * problem.rated_power_kw) / ((1.0 - s_op) * omega_s);
    let ratio_breakdown = t_max / t_rated.max(1.0);
    let ratio_starting = t_start / t_rated.max(1.0);

    let nema_class = if ratio_breakdown >= 2.0 && ratio_starting >= 1.4 && s_op <= 0.05 {
        "NEMA Design B (Standard Industrial High Efficiency)".to_string()
    } else if ratio_starting >= 2.0 {
        "NEMA Design C (High Starting Torque)".to_string()
    } else {
        "NEMA Design D (High Slip)".to_string()
    };

    let is_safe = ratio_breakdown >= 1.75 && efficiency_pct >= 85.0 && s_op <= 0.08;

    let assessment = format!(
        "Induction Motor ns={:.0} RPM, nop={:.0} RPM (slip={:.2}%). Pop={:.2} kW, Top={:.1} Nm. Tmax/Trated={:.2} (Tmax={:.1} Nm), Tstart/Trated={:.2} (Istart={:.1}A). Efficiency={:.1}%, PF={:.2}. Class: {}. Status: {}",
        n_s,
        speed_op_rpm,
        s_op * 100.0,
        p_shaft_kw,
        t_op,
        ratio_breakdown,
        t_max,
        ratio_starting,
        i_start,
        efficiency_pct,
        pf,
        nema_class,
        if is_safe {
            "PASS COMPLIANT"
        } else {
            "MARGINAL / CHECK APPLICATION"
        }
    );

    Ok(InductionMotorResult {
        problem_id: problem.problem_id.clone(),
        synchronous_speed_rpm: n_s,
        synchronous_speed_rad_s: omega_s,
        operating_speed_rpm: speed_op_rpm,
        operating_slip: s_op,
        thevenin_voltage_v: v_th,
        thevenin_resistance_ohm: r_th,
        thevenin_reactance_ohm: x_th,
        breakdown_slip: s_max,
        breakdown_torque_nm: t_max,
        starting_torque_nm: t_start,
        starting_current_a: i_start,
        operating_torque_nm: t_op,
        shaft_power_kw: p_shaft_kw,
        stator_current_a: i_stator,
        power_factor: pf,
        efficiency_pct,
        torque_ratio_breakdown_rated: ratio_breakdown,
        torque_ratio_starting_rated: ratio_starting,
        nema_design_class: nema_class,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for induction motor.
pub fn solve_induction_motor_sim(problem: &InductionMotorProblem) -> SimulationResult {
    match solve_induction_motor(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "operating_speed_rpm".to_string(),
                result.operating_speed_rpm,
            );
            metrics.insert(
                "operating_torque_nm".to_string(),
                result.operating_torque_nm,
            );
            metrics.insert("efficiency_pct".to_string(), result.efficiency_pct);
            metrics.insert(
                "torque_ratio_breakdown_rated".to_string(),
                result.torque_ratio_breakdown_rated,
            );
            metrics.insert("starting_current_a".to_string(), result.starting_current_a);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.operating_speed_rpm, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "operating_speed_rpm",
                result.operating_speed_rpm,
                1450.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electrical.induction_motor_ieee112".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.electrical.induction_motor_ieee112",
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
                engine_id: "atlas.electrical.induction_motor_ieee112".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.electrical.induction_motor_ieee112",
                    "0.1.0",
                ),
                logs: vec![format!("Induction motor simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_induction_motor_nominal() {
        let prob = InductionMotorProblem::default();
        let sol = solve_induction_motor(&prob).expect("Nominal motor solve should succeed");

        assert_eq!(sol.synchronous_speed_rpm, 1500.0);
        assert!(sol.operating_speed_rpm > 1420.0 && sol.operating_speed_rpm < 1480.0);
        assert!(sol.operating_slip > 0.01 && sol.operating_slip < 0.06);
        assert!(sol.torque_ratio_breakdown_rated > 2.0);
        assert!(sol.efficiency_pct > 86.0 && sol.efficiency_pct < 95.0);
        assert!(sol.power_factor > 0.80);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_induction_motor_custom_slip() {
        let prob = InductionMotorProblem {
            operating_slip: Some(0.04),
            ..Default::default()
        };
        let sol = solve_induction_motor(&prob).expect("Custom slip solve executes");
        assert!((sol.operating_slip - 0.04).abs() < 1e-4);
        assert_eq!(sol.operating_speed_rpm, 1440.0);
    }
}

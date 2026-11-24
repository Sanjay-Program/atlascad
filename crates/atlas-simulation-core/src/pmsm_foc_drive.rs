//! Three-Phase Permanent Magnet Synchronous Motor (PMSM) FOC Drive Dynamics (Park-Clarke / MTPA / IEEE 1415)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Direct-quadrature (d-q) synchronous reference frame voltage balances (v_d, v_q).
//! 2. Electromagnetic torque T_e including permanent magnet alignment and rotor saliency reluctance torque.
//! 3. Maximum Torque Per Ampere (MTPA) trajectory for interior permanent magnet synchronous machines (IPMSM).
//! 4. Copper resistive losses P_cu, iron eddy/hysteresis losses P_fe, and electrical drive efficiency eta_e.
//! 5. DC bus voltage utilization, back-EMF limit ellipse, and field-weakening threshold.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Rotor permanent magnet arrangement architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PmsmRotorTopology {
    /// Surface-Mounted Permanent Magnet (SPMSM): L_d ~ L_q (non-salient, zero reluctance torque, i_d = 0 control).
    SurfaceMountedSPMSM,
    /// Interior Permanent Magnet (IPMSM): L_q > L_d (magnetic saliency xi > 1.0, MTPA reluctance torque boost).
    InteriorIPMSM,
}

/// Input parameters for three-phase PMSM motor drive analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmsmDriveProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor permanent magnet topological arrangement.
    pub rotor_topology: PmsmRotorTopology,
    /// Number of pole pairs p (e.g. p = 4 for 8-pole motor).
    pub pole_pairs_p: u32,
    /// Stator phase winding resistance R_s in Ohms (Ω, standard 0.01 to 2.5 Ω).
    pub stator_resistance_ohms: f64,
    /// Direct-axis synchronous inductance L_d in milliHenries (mH, standard 0.1 to 15.0 mH).
    pub d_axis_inductance_mh: f64,
    /// Quadrature-axis synchronous inductance L_q in milliHenries (mH, standard 0.1 to 25.0 mH).
    pub q_axis_inductance_mh: f64,
    /// Permanent magnet flux linkage lambda_pm in Volt-seconds / Weber (Wb, standard 0.02 to 0.50 Wb).
    pub permanent_magnet_flux_wb: f64,
    /// Motor mechanical shaft rotational speed in revolutions per minute (RPM, standard 500 to 18,000 RPM).
    pub rotational_speed_rpm: f64,
    /// Quadrature-axis torque-producing stator current i_q in Amperes (A, standard 5.0 to 350.0 A).
    pub q_axis_current_iq_a: f64,
    /// Inverter DC bus link voltage V_dc in Volts (V, standard 48 to 800 V).
    pub inverter_dc_bus_voltage_v: f64,
}

impl Default for PmsmDriveProblem {
    fn default() -> Self {
        Self {
            problem_id: "ev_traction_ipmsm_motor_drive".to_string(),
            rotor_topology: PmsmRotorTopology::InteriorIPMSM,
            pole_pairs_p: 4,                  // 8-pole EV traction motor
            stator_resistance_ohms: 0.025,    // 25 mΩ stator winding
            d_axis_inductance_mh: 0.45,       // L_d = 0.45 mH
            q_axis_inductance_mh: 0.95,       // L_q = 0.95 mH (Saliency ratio L_q/L_d = 2.11)
            permanent_magnet_flux_wb: 0.095,  // 0.095 Wb NdFeB rotor flux
            rotational_speed_rpm: 4500.0,     // 4500 RPM cruise speed
            q_axis_current_iq_a: 120.0,       // 120 A q-axis current
            inverter_dc_bus_voltage_v: 650.0, // 650 V EV boost converter DC bus (e.g. Toyota THS-II / high-voltage inverter)
        }
    }
}

/// Simulation results for PMSM vector control voltages, torques, powers, and efficiency.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PmsmDriveResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Mechanical shaft angular velocity omega_m in radians per second (rad/s).
    pub mechanical_angular_velocity_rad_s: f64,
    /// Electrical angular frequency omega_e = p * omega_m in radians per second (rad/s).
    pub electrical_angular_velocity_rad_s: f64,
    /// Direct-axis current i_d in Amperes (A) calculated via MTPA or i_d = 0.
    pub d_axis_current_id_a: f64,
    /// Total stator phase current magnitude I_s = sqrt(i_d^2 + i_q^2) in Amperes (A).
    pub stator_current_magnitude_a: f64,
    /// Direct-axis steady-state voltage v_d in Volts (V).
    pub d_axis_voltage_vd_v: f64,
    /// Quadrature-axis steady-state voltage v_q in Volts (V).
    pub q_axis_voltage_vq_v: f64,
    /// Phase voltage magnitude V_s = sqrt(v_d^2 + v_q^2) in Volts (V).
    pub phase_voltage_magnitude_v: f64,
    /// Line-to-line RMS terminal voltage V_LL,rms = sqrt(3/2) * V_s in Volts (V).
    pub line_to_line_voltage_rms_v: f64,
    /// Permanent magnet alignment torque component in Newton-meters (N·m).
    pub pm_alignment_torque_nm: f64,
    /// Rotor magnetic saliency reluctance torque component in Newton-meters (N·m).
    pub reluctance_torque_nm: f64,
    /// Total electromagnetic torque T_e = T_pm + T_rel in Newton-meters (N·m).
    pub total_electromagnetic_torque_nm: f64,
    /// Mechanical output shaft brake power in kiloWatts (kW).
    pub mechanical_shaft_power_kw: f64,
    /// Stator copper Joule heating power loss P_cu in Watts (W).
    pub copper_losses_w: f64,
    /// Stator iron core eddy and hysteresis loss P_fe in Watts (W).
    pub iron_core_losses_w: f64,
    /// Overall motor electrical-to-mechanical conversion efficiency in percentage (%).
    pub motor_efficiency_pct: f64,
    /// Inverter voltage modulation index m_a = sqrt(3) * V_s / V_dc (must be <= 1.0 for linear SVPWM).
    pub inverter_modulation_index: f64,
    /// Whether required stator voltage is within inverter DC bus linear limits (m_a <= 1.0).
    pub is_voltage_within_inverter_limits: bool,
    /// Whether electrical drive efficiency meets high-efficiency benchmark (>= 90.0%).
    pub is_efficiency_high: bool,
    /// Whether motor is operating in base-speed MTPA regime without requiring field weakening.
    pub is_within_base_speed_envelope: bool,
    /// Overall PMSM motor drive feasibility and stability status.
    pub is_pmsm_drive_viable: bool,
}

/// Solves three-phase PMSM field-oriented control, d-q decoupled voltages, MTPA, and efficiency.
pub fn solve_pmsm_foc_drive(problem: &PmsmDriveProblem) -> Result<PmsmDriveResult, String> {
    if problem.pole_pairs_p == 0 {
        return Err("Pole pairs must be positive".to_string());
    }
    if problem.stator_resistance_ohms <= 0.0001 {
        return Err("Stator resistance must be positive".to_string());
    }
    if problem.d_axis_inductance_mh <= 0.01 || problem.q_axis_inductance_mh <= 0.01 {
        return Err("Synchronous inductances must be positive".to_string());
    }
    if problem.permanent_magnet_flux_wb <= 0.001 {
        return Err("Permanent magnet flux must be positive".to_string());
    }
    if problem.rotational_speed_rpm <= 10.0 {
        return Err("Rotational speed must exceed 10 RPM".to_string());
    }
    if problem.q_axis_current_iq_a <= 0.1 {
        return Err("Q-axis current must be positive".to_string());
    }
    if problem.inverter_dc_bus_voltage_v <= 10.0 {
        return Err("Inverter DC bus voltage must exceed 10 V".to_string());
    }

    let p = problem.pole_pairs_p as f64;
    let omega_m = (2.0 * std::f64::consts::PI * problem.rotational_speed_rpm) / 60.0; // rad/s
    let omega_e = p * omega_m; // electrical rad/s

    let r_s = problem.stator_resistance_ohms;
    let l_d = problem.d_axis_inductance_mh * 1.0e-3; // Henries
    let l_q = problem.q_axis_inductance_mh * 1.0e-3; // Henries
    let lambda_pm = problem.permanent_magnet_flux_wb;

    let i_q = problem.q_axis_current_iq_a;

    // Direct-axis current i_d calculation:
    let i_d = match problem.rotor_topology {
        PmsmRotorTopology::SurfaceMountedSPMSM => {
            // For SPMSM (L_d = L_q), MTPA optimal trajectory is i_d = 0:
            0.0
        }
        PmsmRotorTopology::InteriorIPMSM => {
            // For IPMSM (L_q > L_d), MTPA optimal d-axis current:
            // i_d = lambda_pm / (4 * (L_q - L_d)) - sqrt(lambda_pm^2 / (16 * (L_q - L_d)^2) + i_q^2 / 2)
            let delta_l = l_q - l_d;
            if delta_l > 1.0e-6 {
                let term1 = lambda_pm / (4.0 * delta_l);
                let term2 = (term1.powi(2) + (i_q * i_q) / 2.0).sqrt();
                term1 - term2 // negative d-axis demagnetizing current assisting reluctance torque
            } else {
                0.0
            }
        }
    };

    let i_s = (i_d * i_d + i_q * i_q).sqrt();

    // Steady-state d-q decoupled voltage equations (Park transformation):
    // v_d = R_s * i_d - omega_e * L_q * i_q [V]
    let v_d = r_s * i_d - omega_e * l_q * i_q;
    // v_q = R_s * i_q + omega_e * (L_d * i_d + lambda_pm) [V]
    let v_q = r_s * i_q + omega_e * (l_d * i_d + lambda_pm);

    let v_s = (v_d * v_d + v_q * v_q).sqrt();
    let v_ll_rms = (3.0 / 2.0_f64).sqrt() * v_s;

    // Torque components:
    // T_pm = 1.5 * p * lambda_pm * i_q [N*m]
    let t_pm = 1.5 * p * lambda_pm * i_q;
    // T_rel = 1.5 * p * (L_d - L_q) * i_d * i_q [N*m]
    let t_rel = 1.5 * p * (l_d - l_q) * i_d * i_q;
    let t_total = t_pm + t_rel;

    // Mechanical power: P_mech = T_total * omega_m [W] -> [kW]:
    let p_mech_w = t_total * omega_m;
    let p_mech_kw = p_mech_w / 1000.0;

    // Copper losses: P_cu = 1.5 * R_s * (i_d^2 + i_q^2) [W]:
    let p_cu = 1.5 * r_s * (i_d.powi(2) + i_q.powi(2));

    // Stator iron core loss empirical model (Bertotti model approximation):
    // P_fe ~ k_fe * omega_e^1.55 [W]
    let k_fe = 0.012;
    let p_fe = k_fe * omega_e.powf(1.55);

    // Total electrical input power: P_elec = P_mech + P_cu + P_fe [W]:
    let p_elec_w = p_mech_w + p_cu + p_fe;
    let efficiency_pct = if p_elec_w > 0.0 {
        (p_mech_w / p_elec_w) * 100.0
    } else {
        0.0
    };

    // Inverter Space Vector PWM (SVPWM) maximum linear voltage limit:
    // V_s,max = V_dc / sqrt(3) -> Modulation index m_a = sqrt(3) * V_s / V_dc
    let m_a = (3.0_f64.sqrt() * v_s) / problem.inverter_dc_bus_voltage_v;

    // Quality assessments:
    let is_voltage_ok = m_a <= 1.0;
    let is_eff_ok = efficiency_pct >= 90.0;
    let is_base_speed = m_a <= 0.95; // above 0.95 requires field-weakening flux reduction
    let is_overall_ok = is_voltage_ok && is_eff_ok && (t_total > 0.0);

    Ok(PmsmDriveResult {
        problem_id: problem.problem_id.clone(),
        mechanical_angular_velocity_rad_s: omega_m,
        electrical_angular_velocity_rad_s: omega_e,
        d_axis_current_id_a: i_d,
        stator_current_magnitude_a: i_s,
        d_axis_voltage_vd_v: v_d,
        q_axis_voltage_vq_v: v_q,
        phase_voltage_magnitude_v: v_s,
        line_to_line_voltage_rms_v: v_ll_rms,
        pm_alignment_torque_nm: t_pm,
        reluctance_torque_nm: t_rel,
        total_electromagnetic_torque_nm: t_total,
        mechanical_shaft_power_kw: p_mech_kw,
        copper_losses_w: p_cu,
        iron_core_losses_w: p_fe,
        motor_efficiency_pct: efficiency_pct,
        inverter_modulation_index: m_a,
        is_voltage_within_inverter_limits: is_voltage_ok,
        is_efficiency_high: is_eff_ok,
        is_within_base_speed_envelope: is_base_speed,
        is_pmsm_drive_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ev_traction_ipmsm_motor() -> Result<(), String> {
        let prob = PmsmDriveProblem::default();
        let res = solve_pmsm_foc_drive(&prob)?;

        assert!(res.electrical_angular_velocity_rad_s > 1800.0);
        assert!(
            res.d_axis_current_id_a < 0.0,
            "IPMSM MTPA should inject negative d-axis current"
        );
        assert!(res.pm_alignment_torque_nm > 60.0);
        assert!(
            res.reluctance_torque_nm > 5.0,
            "Reluctance torque should contribute positively"
        );
        assert!(res.total_electromagnetic_torque_nm > res.pm_alignment_torque_nm);
        assert!(res.mechanical_shaft_power_kw > 30.0);
        assert!(
            res.motor_efficiency_pct >= 90.0,
            "EV traction motor efficiency >= 90%"
        );
        assert!(
            res.inverter_modulation_index <= 1.0,
            "Should remain within inverter linear SVPWM range"
        );
        assert!(res.is_voltage_within_inverter_limits);
        assert!(res.is_efficiency_high);
        assert!(res.is_pmsm_drive_viable);
        Ok(())
    }
}

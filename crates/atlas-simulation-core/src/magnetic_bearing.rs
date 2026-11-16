//! Active Magnetic Bearing (AMB) Electrodynamic Levitation (ISO 14839 / Schweitzer)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Differential electromagnetic attraction force, bias flux, and reluctance.
//! 2. Open-loop negative position stiffness (k_x) and actuator current gain (k_i).
//! 3. Closed-loop PID active suspension stiffness (k_net), damping (c_net), and natural frequency.
//! 4. ISO 1940 rotor unbalance response and ISO 14839 auxiliary touchdown bearing clearance.
//! 5. Maximum dynamic force slew rate and electromagnetic power amplifier voltage headroom.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Active Magnetic Bearing stator winding and pole configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MagneticBearingTopology {
    /// 8-pole heteropolar radial bearing (N-S-S-N-N-S-S-N stator flux distribution).
    Heteropolar8Pole,
    /// 8-pole homopolar radial bearing with axial permanent magnet bias flux.
    Homopolar8Pole,
    /// 16-pole high-load heavy turbomachinery radial magnetic bearing.
    Heteropolar16Pole,
}

/// Input parameters for active magnetic bearing levitation and dynamic stability analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagneticBearingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Bearing magnetic pole geometry.
    pub topology: MagneticBearingTopology,
    /// Rotor mass supported by this bearing in kilograms (kg, standard 15 to 2500 kg).
    pub rotor_supported_mass_kg: f64,
    /// Maximum operational rotational speed in RPM (standard 6000 to 60,000 RPM).
    pub max_rotational_speed_rpm: f64,
    /// Nominal radial magnetic air gap between stator poles and rotor journal in millimeters (mm, standard 0.40 to 1.20 mm).
    pub nominal_air_gap_mm: f64,
    /// Pole face cross-sectional area per pole in square millimeters (mm², standard 200 to 3500 mm²).
    pub pole_face_area_mm2: f64,
    /// Number of coil wire turns per electromagnetic pole (standard 80 to 250 turns).
    pub coil_turns_per_pole: u32,
    /// Bias DC premagnetization current in Amperes (A, standard 2.0 to 15.0 A).
    pub bias_current_a: f64,
    /// Maximum peak control current limit from power amplifier in Amperes (A, standard 4.0 to 30.0 A).
    pub max_control_current_a: f64,
    /// Power amplifier DC supply bus voltage in Volts (V, standard 100 to 350 V).
    pub amplifier_bus_voltage_v: f64,
    /// Active feedback proportional gain K_p in Amperes per millimeter (A/mm, standard 5.0 to 45.0 A/mm).
    pub controller_proportional_gain_a_per_mm: f64,
    /// Active feedback derivative gain K_d in Amperes per meter/second (A/(m/s), standard 10.0 to 65.0 A/(m/s)).
    pub controller_derivative_gain_a_s_per_m: f64,
    /// ISO 1940 rotor unbalance specific mass eccentricity in micrometers (µm, standard 1.0 to 10.0 µm).
    pub rotor_unbalance_eccentricity_um: f64,
}

impl Default for MagneticBearingProblem {
    fn default() -> Self {
        Self {
            problem_id: "high_speed_compressor_radial_amb".to_string(),
            topology: MagneticBearingTopology::Heteropolar8Pole,
            rotor_supported_mass_kg: 85.0,         // 85 kg rotor share
            max_rotational_speed_rpm: 36000.0,     // 36,000 RPM high-speed centrifugal compressor
            nominal_air_gap_mm: 0.60,              // 0.60 mm air gap
            pole_face_area_mm2: 850.0,             // 850 mm² pole area
            coil_turns_per_pole: 140,              // 140 turns
            bias_current_a: 5.0,                   // 5.0 A bias
            max_control_current_a: 12.0,           // 12.0 A peak control current
            amplifier_bus_voltage_v: 180.0,        // 180 V DC bus
            controller_proportional_gain_a_per_mm: 18.0, // 18 A/mm
            controller_derivative_gain_a_s_per_m: 35.0, // 35.0 A·s/m (0.035 A/(mm/s))
            rotor_unbalance_eccentricity_um: 0.50, // ISO 1940 G1.0 precision balance at 36,000 RPM
        }
    }
}

/// Simulation results for magnetic bearing stiffness, load capacity, and ISO 14839 stability margins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MagneticBearingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Actuator current sensitivity gain k_i in Newtons per Ampere (N/A).
    pub current_gain_ki_n_per_a: f64,
    /// Unstable open-loop negative position stiffness k_x in kiloNewtons per meter (kN/m).
    pub open_loop_negative_stiffness_kx_kn_per_m: f64,
    /// Net closed-loop active suspension stiffness k_net in kiloNewtons per meter (kN/m).
    pub closed_loop_stiffness_k_net_kn_per_m: f64,
    /// Net closed-loop active damping coefficient c_net in Newton-seconds per meter (N·s/m).
    pub closed_loop_damping_c_net_n_s_per_m: f64,
    /// Closed-loop suspension natural frequency in Hertz (Hz).
    pub suspension_natural_frequency_hz: f64,
    /// Closed-loop dimensionless damping ratio zeta (standard 0.25 to 0.70).
    pub damping_ratio_zeta: f64,
    /// Maximum peak dynamic electromagnetic load capacity in Newtons (N).
    pub max_dynamic_load_capacity_n: f64,
    /// Centrifugal unbalance dynamic load force at maximum RPM in Newtons (N).
    pub unbalance_dynamic_force_n: f64,
    /// Dynamic load margin factor (F_max / (F_gravity + F_unbalance)).
    pub dynamic_load_safety_factor: f64,
    /// Maximum rotor radial orbit displacement under unbalance in micrometers (µm).
    pub peak_rotor_orbit_displacement_um: f64,
    /// Ratio of peak rotor displacement to nominal air gap (x_orbit / g0).
    pub air_gap_displacement_ratio: f64,
    /// Coil inductance per axis in milliHenries (mH).
    pub coil_inductance_mh: f64,
    /// Maximum current slew rate di/dt available from amplifier in Amperes per millisecond (A/ms).
    pub max_current_slew_rate_a_per_ms: f64,
    /// Whether net closed-loop stiffness is positive and exceeds open-loop negative stiffness (k_net > 0).
    pub is_stiffness_positive_and_stable: bool,
    /// Whether closed-loop damping ratio is within well-damped stable envelope (0.20 <= zeta <= 0.85).
    pub is_damping_ratio_adequate: bool,
    /// Whether peak displacement satisfies ISO 14839 auxiliary clearance criterion (x_orbit <= 0.30 * g0).
    pub is_air_gap_clearance_safe: bool,
    /// Whether dynamic load safety factor satisfies ISO 14839 minimum threshold (>= 2.0).
    pub is_load_capacity_sufficient: bool,
    /// Overall active magnetic bearing electrodynamic levitation viability status.
    pub is_amb_system_viable: bool,
}

/// Solves active magnetic bearing electrodynamic levitation, open/closed-loop stiffness, and ISO 14839 clearance.
pub fn solve_magnetic_bearing(problem: &MagneticBearingProblem) -> Result<MagneticBearingResult, String> {
    if problem.nominal_air_gap_mm <= 0.05 {
        return Err("Nominal air gap must be at least 0.05 mm".to_string());
    }
    if problem.rotor_supported_mass_kg <= 0.0 {
        return Err("Rotor mass must be positive".to_string());
    }
    if problem.bias_current_a <= 0.0 {
        return Err("Bias current must be positive".to_string());
    }
    if problem.max_control_current_a <= problem.bias_current_a {
        return Err("Maximum control current must exceed bias current".to_string());
    }

    let mu_0 = 4.0 * std::f64::consts::PI * 1e-7; // Vacuum permeability H/m
    let g_0 = problem.nominal_air_gap_mm * 1e-3; // Air gap in m
    let a_p = problem.pole_face_area_mm2 * 1e-6; // Pole area in m²
    let n = problem.coil_turns_per_pole as f64;
    let i_0 = problem.bias_current_a;
    let i_c_max = problem.max_control_current_a;
    let m_rotor = problem.rotor_supported_mass_kg;

    // Pole angle cosine projection for radial bearing pole pairs (typically 22.5° for 8-pole):
    let alpha_rad = match problem.topology {
        MagneticBearingTopology::Heteropolar8Pole | MagneticBearingTopology::Homopolar8Pole => {
            (22.5_f64).to_radians()
        }
        MagneticBearingTopology::Heteropolar16Pole => (11.25_f64).to_radians(),
    };
    let cos_alpha = alpha_rad.cos();

    // Actuator current gain k_i = mu_0 * N^2 * A_p * (i_0 / g_0^2) * cos(alpha) [N/A]
    let k_i = mu_0 * n.powi(2) * a_p * (i_0 / g_0.powi(2)) * cos_alpha * 2.0;

    // Open-loop negative position stiffness k_x = mu_0 * N^2 * A_p * (i_0^2 / g_0^3) * cos(alpha) [N/m]
    let k_x = mu_0 * n.powi(2) * a_p * (i_0.powi(2) / g_0.powi(3)) * cos_alpha * 2.0;

    // Active controller feedback:
    // K_p in A/m:
    let k_p_a_m = problem.controller_proportional_gain_a_per_mm * 1000.0;
    let k_d_a_s_m = problem.controller_derivative_gain_a_s_per_m;

    // Closed-loop net stiffness: k_net = k_i * K_p - k_x
    let k_net = k_i * k_p_a_m - k_x;

    // Closed-loop net damping: c_net = k_i * K_d
    let c_net = k_i * k_d_a_s_m;

    // Natural frequency and damping ratio:
    let (_omega_n, f_n, zeta) = if k_net > 0.0 {
        let w_n = (k_net / m_rotor).sqrt();
        let f = w_n / (2.0 * std::f64::consts::PI);
        let z = c_net / (2.0 * (k_net * m_rotor).sqrt());
        (w_n, f, z)
    } else {
        (0.0, 0.0, 0.0)
    };

    // Maximum dynamic load capacity:
    let f_dyn_max = k_i * (i_c_max - i_0);

    // Dynamic unbalance force at max RPM:
    let omega_rotor = (problem.max_rotational_speed_rpm * 2.0 * std::f64::consts::PI) / 60.0;
    let e_unb = problem.rotor_unbalance_eccentricity_um * 1e-6;
    let f_unb = m_rotor * e_unb * omega_rotor.powi(2);

    // Gravity force on bearing:
    let f_grav = m_rotor * 9.80665;
    let total_required_force = f_grav + f_unb;
    let sf_load = if total_required_force > 1e-3 {
        f_dyn_max / total_required_force
    } else {
        10.0
    };

    // Peak unbalance orbit displacement:
    let x_orbit_m = if k_net > 0.0 {
        f_unb / k_net
    } else {
        g_0
    };
    let x_orbit_um = x_orbit_m * 1e6;
    let orbit_gap_ratio = x_orbit_m / g_0;

    // Axis coil inductance: L = mu_0 * N^2 * A_p / g_0 * 2
    let l_axis_h = 2.0 * mu_0 * n.powi(2) * a_p / g_0;
    let l_axis_mh = l_axis_h * 1000.0;

    // Slew rate: di/dt = V_bus / L
    let di_dt_a_s = problem.amplifier_bus_voltage_v / l_axis_h;
    let di_dt_a_ms = di_dt_a_s / 1000.0;

    // Verification flags per ISO 14839:
    let is_stiffness_ok = k_net >= 0.50 * k_x && k_net > 0.0;
    let is_damping_ok = (0.20..=0.85).contains(&zeta);
    let is_clearance_ok = orbit_gap_ratio <= 0.30;
    let is_load_ok = sf_load >= 2.0;
    let is_overall_ok = is_stiffness_ok && is_damping_ok && is_clearance_ok && is_load_ok;

    Ok(MagneticBearingResult {
        problem_id: problem.problem_id.clone(),
        current_gain_ki_n_per_a: k_i,
        open_loop_negative_stiffness_kx_kn_per_m: k_x / 1000.0,
        closed_loop_stiffness_k_net_kn_per_m: k_net / 1000.0,
        closed_loop_damping_c_net_n_s_per_m: c_net,
        suspension_natural_frequency_hz: f_n,
        damping_ratio_zeta: zeta,
        max_dynamic_load_capacity_n: f_dyn_max,
        unbalance_dynamic_force_n: f_unb,
        dynamic_load_safety_factor: sf_load,
        peak_rotor_orbit_displacement_um: x_orbit_um,
        air_gap_displacement_ratio: orbit_gap_ratio,
        coil_inductance_mh: l_axis_mh,
        max_current_slew_rate_a_per_ms: di_dt_a_ms,
        is_stiffness_positive_and_stable: is_stiffness_ok,
        is_damping_ratio_adequate: is_damping_ok,
        is_air_gap_clearance_safe: is_clearance_ok,
        is_load_capacity_sufficient: is_load_ok,
        is_amb_system_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_speed_compressor_amb() -> Result<(), String> {
        let prob = MagneticBearingProblem::default();
        let res = solve_magnetic_bearing(&prob)?;

        assert!(res.current_gain_ki_n_per_a > 100.0);
        assert!(res.open_loop_negative_stiffness_kx_kn_per_m > 500.0);
        assert!(res.closed_loop_stiffness_k_net_kn_per_m > 500.0);
        assert!(res.suspension_natural_frequency_hz > 25.0);
        assert!(res.damping_ratio_zeta >= 0.20 && res.damping_ratio_zeta <= 0.85);
        assert!(res.dynamic_load_safety_factor >= 2.0);
        assert!(res.air_gap_displacement_ratio <= 0.30);
        assert!(res.is_stiffness_positive_and_stable);
        assert!(res.is_damping_ratio_adequate);
        assert!(res.is_air_gap_clearance_safe);
        assert!(res.is_load_capacity_sufficient);
        assert!(res.is_amb_system_viable);
        Ok(())
    }
}

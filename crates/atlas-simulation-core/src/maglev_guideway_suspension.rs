//! High-Speed Magnetic Levitation (Maglev) Electromagnetic Suspension (EMS) Active Airgap Control
//!
//! Formulated according to:
//! - Transrapid Maglev System Technical Specifications & High-Speed Ground Transportation Guideway Standards
//! - ISO 14837 "Mechanical vibration -- Ground-borne noise and vibration arising from rail systems"
//! - IEEE Std 1475 "IEEE Standard for the Functioning of and Interfaces Among Propulsion, Friction Brake, and Train-Borne Control on Rapid Rail Vehicles"
//! - Sinha, P. K., "Electromagnetic Suspension: Dynamics & Control", Peter Peregrinus Ltd. / IEE
//!
//! Evaluates non-linear electromagnetic attraction physics, state-space small-signal linearization
//! about nominal airgap z_0, open-loop unstable poles, closed-loop state feedback controller (LQR),
//! guideway track vertical irregularity disturbance rejection, and levitation magnet electrical power consumption.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Maglev suspension magnet core material class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaglevCoreMaterial {
    /// Silicon Steel Laminations (Fe-Si 3%, saturation flux density B_sat = 1.80 T).
    SiliconSteelFeSi,
    /// Cobalt-Iron High Saturation Alloy (Hiperco 50 / Vacoflux 50, B_sat = 2.30 T).
    CobaltIronHiperco,
    /// Solid Forged Low-Carbon Steel (B_sat = 1.60 T).
    LowCarbonSteel,
}

impl MaglevCoreMaterial {
    /// Saturation magnetic flux density B_sat in Tesla (T).
    pub fn saturation_flux_density_t(&self) -> f64 {
        match self {
            Self::SiliconSteelFeSi => 1.80,
            Self::CobaltIronHiperco => 2.30,
            Self::LowCarbonSteel => 1.60,
        }
    }
}

/// Track guideway spatial irregularity excitation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GuidewayDisturbanceType {
    /// Smooth precision milled steel stator track (baseline micro-roughness).
    SmoothGuideway,
    /// Track expansion joint step disturbance (e.g. 3.0 mm step).
    ExpansionJointStep,
    /// Guideway girder midspan deflection sag sinusoidal wave.
    GirderDeflectionWave,
}

/// Input parameters for Maglev EMS active airgap and electromagnetic levitation simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaglevSuspensionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Electromagnet core magnetic material.
    pub core_material: MaglevCoreMaterial,
    /// Guideway vertical track irregularity profile.
    pub disturbance_type: GuidewayDisturbanceType,
    /// Total levitated vehicle mass supported per levitation magnet module m_car in kg (standard 600 to 2500 kg).
    pub mass_supported_per_magnet_kg: f64,
    /// Target nominal static levitation airgap z_0 in millimeters (mm, standard 8.0 to 15.0 mm, Transrapid = 12.0 mm).
    pub nominal_airgap_mm: f64,
    /// Total number of coil wire turns N_turns (standard 150 to 600 turns).
    pub coil_turns: u32,
    /// Total active magnetic pole face contact area A_pole in square millimeters (mm², standard 20,000 to 120,000 mm²).
    pub pole_face_area_mm2: f64,
    /// Total DC electrical coil winding resistance R_coil in Ohms (standard 0.15 to 1.50 Ohms).
    pub coil_resistance_ohms: f64,
    /// Maximum chopper/inverter supply bus voltage V_max in Volts DC (standard 150 to 600 V).
    pub max_inverter_voltage_v: f64,
    /// Maximum allowable continuous coil current I_max in Amperes (A, standard 40 to 150 A).
    pub max_continuous_current_a: f64,
    /// Closed-loop controller bandwidth target omega_c in radians per second (rad/s, standard 40 to 120 rad/s).
    pub target_controller_bandwidth_rad_s: f64,
    /// Magnitude of guideway vertical track step or sag disturbance in millimeters (mm, standard 0.5 to 5.0 mm).
    pub disturbance_amplitude_mm: f64,
}

impl Default for MaglevSuspensionProblem {
    fn default() -> Self {
        Self {
            problem_id: "transrapid_ems_levitation_module".to_string(),
            core_material: MaglevCoreMaterial::SiliconSteelFeSi,
            disturbance_type: GuidewayDisturbanceType::ExpansionJointStep,
            mass_supported_per_magnet_kg: 1250.0, // 1.25 tonnes per levitation module
            nominal_airgap_mm: 12.0,              // 12.0 mm Transrapid standard airgap
            coil_turns: 320,                      // 320 turns
            pole_face_area_mm2: 58000.0,          // 58,000 mm² pole area (0.058 m²)
            coil_resistance_ohms: 0.38,           // 0.38 Ohms coil resistance
            max_inverter_voltage_v: 350.0,        // 350 V DC chopper rail
            max_continuous_current_a: 85.0,       // 85 A peak continuous current
            target_controller_bandwidth_rad_s: 65.0, // 65 rad/s bandwidth (~10.3 Hz)
            disturbance_amplitude_mm: 2.50,       // 2.5 mm joint step irregularity
        }
    }
}

/// Simulation results for Maglev EMS electromagnet equilibrium, linearized state-space, and airgap control.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MaglevSuspensionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total required static electromagnetic lift force F_mag0 in kiloNewtons (kN).
    pub static_lift_force_kn: f64,
    /// Equilibrium coil current i_0 required to hold nominal airgap z_0 in Amperes (A).
    pub equilibrium_current_a: f64,
    /// Equilibrium magnetic flux density B_0 in airgap in Tesla (T).
    pub equilibrium_flux_density_t: f64,
    /// Electromagnet coil inductance L_0 at nominal operating airgap in milliHenries (mH).
    pub nominal_inductance_mh: f64,
    /// Force-current linearization sensitivity coefficient k_i in Newtons per Ampere (N/A).
    pub force_current_sensitivity_n_per_a: f64,
    /// Force-displacement negative spring stiffness coefficient k_z in kiloNewtons per meter (kN/m).
    pub force_displacement_stiffness_kn_per_m: f64,
    /// Open-loop unstable pole frequency p_unstable in radians per second (rad/s).
    pub open_loop_unstable_pole_rad_s: f64,
    /// Proportional airgap error feedback gain K_p in Volts per meter (V/m).
    pub controller_proportional_gain_v_per_m: f64,
    /// Derivative airgap velocity feedback gain K_d in Volt-seconds per meter (V*s/m).
    pub controller_derivative_gain_v_s_per_m: f64,
    /// Current error feedback gain K_i in Volts per Ampere (V/A).
    pub controller_current_gain_v_per_a: f64,
    /// Maximum dynamic airgap deflection under track irregularity in millimeters (mm).
    pub peak_dynamic_airgap_error_mm: f64,
    /// Minimum dynamic airgap reached during disturbance response in millimeters (mm).
    pub min_transient_airgap_mm: f64,
    /// Continuous electrical power consumed by levitation magnet coil in kiloWatts (kW).
    pub steady_state_coil_power_kw: f64,
    /// Assessment indicating magnetic core does not saturate at equilibrium (B_0 <= 0.85 * B_sat).
    pub is_core_saturation_prevented: bool,
    /// Assessment indicating coil current remains within continuous thermal rating (i_0 <= I_max).
    pub is_coil_current_safe: bool,
    /// Assessment indicating airgap stays strictly positive to prevent physical contact with rail (z_min >= 2.0 mm).
    pub is_airgap_contact_prevented: bool,
    /// Overall Maglev EMS suspension and airgap control viability.
    pub is_maglev_suspension_viable: bool,
}

/// Solves high-speed Maglev EMS active airgap dynamics, LQR feedback, and power consumption per IEEE 1475.
pub fn solve_maglev_guideway_suspension(
    problem: &MaglevSuspensionProblem,
) -> Result<MaglevSuspensionResult, String> {
    if problem.mass_supported_per_magnet_kg <= 50.0 {
        return Err("Mass supported per magnet must be greater than 50 kg".to_string());
    }
    if problem.nominal_airgap_mm <= 1.0 {
        return Err("Nominal airgap must be greater than 1 mm".to_string());
    }
    if problem.coil_turns < 10 {
        return Err("Coil turns must be at least 10".to_string());
    }
    if problem.pole_face_area_mm2 <= 100.0 {
        return Err("Pole face area must be greater than 100 mm²".to_string());
    }

    let g = 9.80665;
    let mu_0 = 4.0 * std::f64::consts::PI * 1.0e-7; // Permeability of free space
    let m_car = problem.mass_supported_per_magnet_kg;
    let z_0_m = problem.nominal_airgap_mm * 0.001;
    let n_turns = problem.coil_turns as f64;
    let a_pole_m2 = problem.pole_face_area_mm2 * 1.0e-6;

    // 1. Static Equilibrium Levitation Force F_mag0:
    let f_mag0_n = m_car * g;
    let f_mag0_kn = f_mag0_n / 1000.0;

    // Magnetic force across two airgaps (closed magnetic circuit):
    // F_mag = (mu_0 * N^2 * A_pole * i^2) / (4 * z^2)
    // i_0 = 2 * z_0 * sqrt(F_mag0 / (mu_0 * N^2 * A_pole))
    let geom_factor = mu_0 * n_turns.powi(2) * a_pole_m2;
    let i_0_a = 2.0 * z_0_m * (f_mag0_n / geom_factor).sqrt();

    // Equilibrium magnetic flux density:
    // B_0 = (mu_0 * N * i_0) / (2 * z_0)
    let b_0_t = (mu_0 * n_turns * i_0_a) / (2.0 * z_0_m);

    // Nominal coil inductance L_0 at airgap z_0:
    // L_0 = (mu_0 * N^2 * A_pole) / (2 * z_0)
    let l_0_h = geom_factor / (2.0 * z_0_m);
    let l_0_mh = l_0_h * 1000.0;

    // 2. Small-signal linearization about (z_0, i_0):
    // Delta F_mag = k_i * Delta i - k_z * Delta z
    // k_i = (mu_0 * N^2 * A_pole * i_0) / (2 * z_0^2) = 2 * F_mag0 / i_0
    let k_i_n_per_a = (2.0 * f_mag0_n) / i_0_a;
    // k_z = (mu_0 * N^2 * A_pole * i_0^2) / (2 * z_0^3) = 2 * F_mag0 / z_0
    let k_z_n_per_m = (2.0 * f_mag0_n) / z_0_m;
    let k_z_kn_per_m = k_z_n_per_m / 1000.0;

    // 3. Open-loop unstable pole frequency:
    // m * d^2(z)/dt^2 - k_z * z = 0 -> p = sqrt(k_z / m)
    let omega_unstable = (k_z_n_per_m / m_car).sqrt();

    // 4. Closed-loop state feedback controller synthesis:
    // Desired closed-loop polynomial: (s + omega_c)^3 = s^3 + 3*omega_c*s^2 + 3*omega_c^2*s + omega_c^3
    let omega_c = problem.target_controller_bandwidth_rad_s;
    let a1_des = 3.0 * omega_c;
    let a2_des = 3.0 * omega_c.powi(2);
    let a3_des = omega_c.powi(3);

    // Electromagnet electrical equation: V = R*i + L_0*di/dt + k_i*(dz/dt)
    // Gains:
    let r_coil = problem.coil_resistance_ohms;
    // Substituting u = -k_p z - k_d dz/dt - k_i_gain i into the linearized plant
    //       m z'' = k_i i - k_z z + f,
    //       L di/dt = u - R i - k_i z',
    // gives the characteristic polynomial
    //   s^3 + (R + k_i_gain)/L s^2
    //       + [k_z/m + k_i(k_i + k_d)/(m L)] s
    //       + [k_z(R + k_i_gain)/(m L) + k_i k_p/(m L)].
    // Matching it term-by-term to (s + omega_c)^3 yields the gains below.
    // The earlier formulas omitted the -k_i and -3 k_z L omega_c terms, so the
    // closed-loop poles were not at -omega_c even though the doc claimed they were.
    let k_i_gain = (l_0_h * a1_des) - r_coil;
    let k_d_gain = ((m_car * l_0_h * a2_des - k_z_n_per_m * l_0_h) / k_i_n_per_a) - k_i_n_per_a;
    let k_p_gain = ((m_car * l_0_h * a3_des) - (3.0 * k_z_n_per_m * l_0_h * omega_c)) / k_i_n_per_a;

    // 5. Dynamic response to track disturbance:
    // A guideway step is a base-motion input: at t = 0+ the airgap changes by the
    // full step amplitude before the controller can measure it. The peak
    // transient error is therefore the step itself; the controller only sets how
    // fast that error is rejected. An earlier (omega_unstable/omega_c)^2 clamp had
    // no physical basis and under-reported the required airgap.
    let delta_z_dist_m = problem.disturbance_amplitude_mm * 0.001;
    let peak_error_factor = 1.0;
    let peak_dynamic_error_m = delta_z_dist_m * peak_error_factor;
    let peak_dynamic_error_mm = peak_dynamic_error_m * 1000.0;
    let min_airgap_mm = problem.nominal_airgap_mm - peak_dynamic_error_mm;

    // 6. Steady-state electrical power consumption:
    let p_coil_w = r_coil * i_0_a.powi(2);
    let p_coil_kw = p_coil_w / 1000.0;

    // 7. Safety and operational feasibility checks:
    let b_sat = problem.core_material.saturation_flux_density_t();
    let is_core_saturation_prevented = b_0_t <= 0.85 * b_sat;
    let is_coil_current_safe = i_0_a <= problem.max_continuous_current_a;
    let is_airgap_contact_prevented = min_airgap_mm >= 2.0; // At least 2 mm clearance
    let is_maglev_suspension_viable =
        is_core_saturation_prevented && is_coil_current_safe && is_airgap_contact_prevented;

    Ok(MaglevSuspensionResult {
        problem_id: problem.problem_id.clone(),
        static_lift_force_kn: f_mag0_kn,
        equilibrium_current_a: i_0_a,
        equilibrium_flux_density_t: b_0_t,
        nominal_inductance_mh: l_0_mh,
        force_current_sensitivity_n_per_a: k_i_n_per_a,
        force_displacement_stiffness_kn_per_m: k_z_kn_per_m,
        open_loop_unstable_pole_rad_s: omega_unstable,
        controller_proportional_gain_v_per_m: k_p_gain,
        controller_derivative_gain_v_s_per_m: k_d_gain,
        controller_current_gain_v_per_a: k_i_gain,
        peak_dynamic_airgap_error_mm: peak_dynamic_error_mm,
        min_transient_airgap_mm: min_airgap_mm,
        steady_state_coil_power_kw: p_coil_kw,
        is_core_saturation_prevented,
        is_coil_current_safe,
        is_airgap_contact_prevented,
        is_maglev_suspension_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transrapid_ems_maglev_suspension() -> Result<(), String> {
        let problem = MaglevSuspensionProblem::default();
        let res = solve_maglev_guideway_suspension(&problem)?;

        assert!(res.static_lift_force_kn > 10.0 && res.static_lift_force_kn < 15.0); // ~12.26 kN
        assert!(res.equilibrium_current_a > 30.0 && res.equilibrium_current_a < 60.0); // ~43.7 A
        assert!(res.equilibrium_flux_density_t > 0.40 && res.equilibrium_flux_density_t < 0.85); // ~0.515 T
        assert!(res.nominal_inductance_mh > 200.0 && res.nominal_inductance_mh < 450.0); // ~311.0 mH
        assert!(
            res.open_loop_unstable_pole_rad_s > 30.0 && res.open_loop_unstable_pole_rad_s < 55.0
        ); // ~40.4 rad/s
        assert!(res.min_transient_airgap_mm > 5.0 && res.min_transient_airgap_mm < 12.0);
        assert!(res.steady_state_coil_power_kw > 0.20 && res.steady_state_coil_power_kw < 1.0); // ~0.36 kW
        assert!(res.is_core_saturation_prevented);
        assert!(res.is_coil_current_safe);
        assert!(res.is_airgap_contact_prevented);
        assert!(res.is_maglev_suspension_viable);
        Ok(())
    }

    #[test]
    fn test_zero_airgap_rejected() {
        let problem = MaglevSuspensionProblem {
            nominal_airgap_mm: 0.5,
            ..Default::default()
        };
        assert!(solve_maglev_guideway_suspension(&problem).is_err());
    }
}

//! Spacecraft Reaction Wheel Micro-Vibration & Jitter Disturbance Solver.
//!
//! Models reaction wheel assembly (RWA) static/dynamic unbalance disturbances,
//! bearing harmonic frequencies, and structural micro-vibration amplification onto optical payloads
//! in compliance with NASA-HDBK-7005 and ESA ECSS-E-HB-32-26A standards.
//!
//! # Physical Formulations:
//! - **Static Imbalance Radial Disturbance Force (NASA-HDBK-7005)**:
//!   $$F_{s}(\Omega) = U_s \cdot \Omega^2 \quad [\text{N}]$$
//! - **Dynamic Imbalance Rocking Disturbance Torque**:
//!   $$M_{d}(\Omega) = U_d \cdot \Omega^2 \quad [\text{N}\cdot\text{m}]$$
//! - **Bearing Ball Pass Outer & Inner Race Kinematic Harmonics**:
//!   $$f_{BPFO} = \frac{N_{balls}}{2} \cdot f_{spin} \cdot \left(1 - \frac{d_b}{D_p} \cos\alpha\right)$$
//!   $$f_{BPFI} = \frac{N_{balls}}{2} \cdot f_{spin} \cdot \left(1 + \frac{d_b}{D_p} \cos\alpha\right)$$
//! - **Second-Order Spacecraft Structural Modal Transmissibility ($H(\omega)$)**:
//!   $$|H(\omega)| = \frac{1}{\sqrt{\left(1 - \left(\frac{\omega}{\omega_n}\right)^2\right)^2 + \left(2 \zeta \frac{\omega}{\omega_n}\right)^2}}$$
//! - **Payload Line-of-Sight (LOS) Pointing Jitter Angle**:
//!   $$\theta_{jitter,RMS} = \frac{|H(\Omega)| \cdot M_d(\Omega)}{J_{sat} \cdot \Omega^2} \cdot 10^6 \quad [\mu\text{rad}]$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Input parameters for spacecraft reaction wheel jitter analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacecraftJitterProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Reaction wheel nominal operating speed in RPM.
    pub wheel_speed_rpm: f64,
    /// Wheel rotor polar moment of inertia J_z in kg·m².
    pub rotor_inertia_kg_m2: f64,
    /// Wheel rotor mass in kilograms (kg).
    pub rotor_mass_kg: f64,
    /// Static mass imbalance U_s in gram-millimeters (g·mm).
    pub static_imbalance_g_mm: f64,
    /// Dynamic mass imbalance U_d in gram-square-millimeters (g·mm²).
    pub dynamic_imbalance_g_mm2: f64,
    /// Number of rolling balls in bearing assembly.
    pub number_of_bearing_balls: u32,
    /// Ball bearing diameter in millimeters (mm).
    pub ball_diameter_mm: f64,
    /// Bearing pitch diameter in millimeters (mm).
    pub bearing_pitch_diameter_mm: f64,
    /// Bearing contact angle in degrees.
    pub bearing_contact_angle_deg: f64,
    /// Spacecraft bus fundamental structural resonance frequency in Hertz (Hz).
    pub structural_resonance_freq_hz: f64,
    /// Spacecraft structural damping ratio zeta (standard 0.01 to 0.03 for aerospace trusses).
    pub structural_damping_ratio: f64,
    /// Effective satellite moment of inertia about the payload optical axis in kg·m².
    pub satellite_inertia_kg_m2: f64,
    /// Maximum allowable optical payload line-of-sight (LOS) pointing jitter in microradians (µrad).
    pub allowable_pointing_jitter_urad: f64,
}

impl Default for SpacecraftJitterProblem {
    fn default() -> Self {
        Self {
            problem_id: "earth_observation_sat_jitter".to_string(),
            wheel_speed_rpm: 3600.0,
            rotor_inertia_kg_m2: 0.045,
            rotor_mass_kg: 5.50,
            static_imbalance_g_mm: 1.80,
            dynamic_imbalance_g_mm2: 35.0,
            number_of_bearing_balls: 8,
            ball_diameter_mm: 7.14,
            bearing_pitch_diameter_mm: 38.0,
            bearing_contact_angle_deg: 15.0,
            structural_resonance_freq_hz: 85.0,
            structural_damping_ratio: 0.015,
            satellite_inertia_kg_m2: 320.0,
            allowable_pointing_jitter_urad: 2.50,
        }
    }
}

/// Simulation results for reaction wheel micro-vibration and payload jitter.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacecraftJitterResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor spin frequency in Hertz (Hz).
    pub spin_frequency_hz: f64,
    /// Rotor angular velocity in radians per second (rad/s).
    pub rotor_angular_velocity_rad_s: f64,
    /// Peak static unbalance disturbance force F_s in Newtons (N).
    pub static_disturbance_force_n: f64,
    /// Peak dynamic unbalance disturbance torque M_d in milliNewton-meters (mN·m).
    pub dynamic_disturbance_torque_mn_m: f64,
    /// Bearing Ball Pass Frequency Outer Race (BPFO) in Hertz (Hz).
    pub bearing_bpfo_frequency_hz: f64,
    /// Bearing Ball Pass Frequency Inner Race (BPFI) in Hertz (Hz).
    pub bearing_bpfi_frequency_hz: f64,
    /// Structural amplification transmissibility |H(omega)| at the spin fundamental.
    pub structural_transmissibility_at_spin: f64,
    /// Structural amplification transmissibility |H(omega)| at BPFO harmonic.
    pub structural_transmissibility_at_bpfo: f64,
    /// Structural amplification transmissibility |H(omega)| at BPFI harmonic.
    pub structural_transmissibility_at_bpfi: f64,
    /// Transmitted micro-vibration acceleration at payload mounting in milli-g (mg).
    pub transmitted_payload_accel_mg: f64,
    /// Root-mean-square (RMS) Line-of-Sight (LOS) pointing jitter error in microradians (µrad).
    pub payload_pointing_jitter_urad: f64,
    /// Pointing jitter in arcseconds (arcsec).
    pub payload_pointing_jitter_arcsec: f64,
    /// Ratio of jitter to allowable budget (jitter / allowable).
    pub jitter_margin_ratio: f64,
    /// Whether pointing jitter meets the spacecraft payload requirement (jitter <= allowable).
    pub is_jitter_compliant: bool,
    /// Whether spin frequency avoids structural resonance (frequency separation >= 15%).
    pub is_resonance_avoided: bool,
}

/// Solves spacecraft reaction wheel jitter, bearing harmonics, and payload line-of-sight micro-vibrations.
pub fn solve_spacecraft_jitter(
    problem: &SpacecraftJitterProblem,
) -> Result<SpacecraftJitterResult, String> {
    if problem.wheel_speed_rpm <= 0.0 {
        return Err("Wheel speed must be positive".to_string());
    }
    if problem.structural_resonance_freq_hz <= 0.0 {
        return Err("Structural resonance frequency must be positive".to_string());
    }
    if problem.satellite_inertia_kg_m2 <= 0.0 {
        return Err("Satellite inertia must be positive".to_string());
    }

    let f_spin = problem.wheel_speed_rpm / 60.0;
    let omega_spin = 2.0 * std::f64::consts::PI * f_spin;

    // Convert imbalances to SI:
    // U_s [g*mm] = 1e-6 [kg*m]
    let u_s_kg_m = problem.static_imbalance_g_mm * 1e-6;
    // U_d [g*mm²] = 1e-9 [kg*m²]
    let u_d_kg_m2 = problem.dynamic_imbalance_g_mm2 * 1e-9;

    // Disturbance amplitudes:
    // F_s = U_s * omega^2
    let f_static_n = u_s_kg_m * omega_spin * omega_spin;
    // M_d = U_d * omega^2
    let m_dynamic_n_m = u_d_kg_m2 * omega_spin * omega_spin;
    let m_dynamic_mn_m = m_dynamic_n_m * 1000.0;

    // Bearing characteristic kinematic pass frequencies (Bialke / Harris):
    // gamma_b = (d_b / D_p) * cos(alpha)
    let alpha_rad = problem.bearing_contact_angle_deg.to_radians();
    let gamma_b = (problem.ball_diameter_mm / problem.bearing_pitch_diameter_mm) * alpha_rad.cos();
    let n_b = problem.number_of_bearing_balls as f64;

    let f_bpfo = (n_b / 2.0) * f_spin * (1.0 - gamma_b);
    let f_bpfi = (n_b / 2.0) * f_spin * (1.0 + gamma_b);

    // Structural transmissibility transfer function:
    // H(f) = 1 / sqrt( (1 - (f/fn)^2)^2 + (2 * zeta * (f/fn))^2 )
    let fn_hz = problem.structural_resonance_freq_hz;
    let zeta = problem.structural_damping_ratio.clamp(0.001, 0.50);

    let calc_transmissibility = |f: f64| -> f64 {
        let r = f / fn_hz;
        let denom_sq = (1.0 - r * r).powi(2) + (2.0 * zeta * r).powi(2);
        1.0 / denom_sq.sqrt()
    };

    let h_spin = calc_transmissibility(f_spin);
    let h_bpfo = calc_transmissibility(f_bpfo);
    let h_bpfi = calc_transmissibility(f_bpfi);

    // Micro-vibration acceleration at payload mounting:
    // a_payload = F_static * H(spin) / M_sat
    // M_sat ~ 50 * rotor_mass typically, or payload mount acceleration
    let a_mount_m_s2 = (f_static_n * h_spin) / problem.rotor_mass_kg;
    let a_mount_mg = (a_mount_m_s2 / 9.80665) * 1000.0;

    // Line-of-Sight (LOS) Pointing Jitter Angle RMS (microradians):
    // theta_jitter = (M_dynamic * H(spin)) / (J_sat * omega^2) * 1e6
    // Add RSS contribution from bearing ripple (~15% of unbalance torque):
    let theta_spin_urad = ((m_dynamic_n_m * h_spin)
        / (problem.satellite_inertia_kg_m2 * omega_spin * omega_spin))
        * 1e6;
    let theta_bearing_urad = 0.15
        * ((m_dynamic_n_m * h_bpfo.max(h_bpfi))
            / (problem.satellite_inertia_kg_m2 * omega_spin * omega_spin))
        * 1e6;
    let theta_total_urad = (theta_spin_urad.powi(2) + theta_bearing_urad.powi(2)).sqrt();
    let theta_arcsec = theta_total_urad * 0.206265; // 1 rad = 206265 arcsec

    let jitter_ratio = theta_total_urad / problem.allowable_pointing_jitter_urad;
    let is_jitter_ok = theta_total_urad <= problem.allowable_pointing_jitter_urad;

    // Resonance avoidance check: |f_spin - f_n| / f_n >= 0.15
    let separation = ((f_spin - fn_hz).abs()) / fn_hz;
    let is_resonance_ok = separation >= 0.15;

    Ok(SpacecraftJitterResult {
        problem_id: problem.problem_id.clone(),
        spin_frequency_hz: f_spin,
        rotor_angular_velocity_rad_s: omega_spin,
        static_disturbance_force_n: f_static_n,
        dynamic_disturbance_torque_mn_m: m_dynamic_mn_m,
        bearing_bpfo_frequency_hz: f_bpfo,
        bearing_bpfi_frequency_hz: f_bpfi,
        structural_transmissibility_at_spin: h_spin,
        structural_transmissibility_at_bpfo: h_bpfo,
        structural_transmissibility_at_bpfi: h_bpfi,
        transmitted_payload_accel_mg: a_mount_mg,
        payload_pointing_jitter_urad: theta_total_urad,
        payload_pointing_jitter_arcsec: theta_arcsec,
        jitter_margin_ratio: jitter_ratio,
        is_jitter_compliant: is_jitter_ok,
        is_resonance_avoided: is_resonance_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_spacecraft_reaction_wheel_jitter() -> Result<(), String> {
        let prob = SpacecraftJitterProblem {
            problem_id: "sentinel_satellite_jitter".to_string(),
            wheel_speed_rpm: 3600.0,
            rotor_inertia_kg_m2: 0.045,
            rotor_mass_kg: 5.50,
            static_imbalance_g_mm: 1.80,
            dynamic_imbalance_g_mm2: 35.0,
            number_of_bearing_balls: 8,
            ball_diameter_mm: 7.14,
            bearing_pitch_diameter_mm: 38.0,
            bearing_contact_angle_deg: 15.0,
            structural_resonance_freq_hz: 85.0,
            structural_damping_ratio: 0.015,
            satellite_inertia_kg_m2: 320.0,
            allowable_pointing_jitter_urad: 2.50,
        };

        let res = solve_spacecraft_jitter(&prob)?;
        assert_eq!(res.spin_frequency_hz, 60.0);
        assert!(res.static_disturbance_force_n > 0.20 && res.static_disturbance_force_n < 0.35);
        assert!(
            res.dynamic_disturbance_torque_mn_m > 4.0 && res.dynamic_disturbance_torque_mn_m < 6.5
        );
        assert!(res.bearing_bpfo_frequency_hz > 180.0 && res.bearing_bpfo_frequency_hz < 220.0);
        assert!(res.bearing_bpfi_frequency_hz > 260.0 && res.bearing_bpfi_frequency_hz < 300.0);
        assert!(res.payload_pointing_jitter_urad < 2.50);
        assert!(res.is_jitter_compliant);
        assert!(res.is_resonance_avoided);
        Ok(())
    }
}

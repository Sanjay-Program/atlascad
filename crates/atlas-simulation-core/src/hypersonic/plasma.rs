//! Nonequilibrium Air Chemistry, Boundary Layer Plasma Sheath, and RF Blackout.
//!
//! Models high-temperature gas ionization, electron number density, plasma frequency,
//! electromagnetic wave attenuation (L-band, X-band, Ka-band, Optical), and
//! Magnetohydrodynamic (MHD) magnetic window transmission mitigation.

use serde::{Deserialize, Serialize};

/// Plasma sheath state and electromagnetic propagation properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlasmaSheathResult {
    /// Stagnation temperature in Kelvin
    pub shock_temperature_k: f64,
    /// Electron number density in electrons/m^3
    pub electron_density_m3: f64,
    /// Angular plasma frequency in rad/s
    pub plasma_frequency_rad_s: f64,
    /// Plasma cutoff frequency in GHz
    pub plasma_cutoff_freq_ghz: f64,
    /// Electron-neutral collision frequency in 1/s
    pub collision_frequency_hz: f64,
    /// Boundary layer sheath thickness in meters
    pub sheath_thickness_m: f64,
    /// GPS L1 band (1.575 GHz) attenuation in dB
    pub attenuation_l_band_db: f64,
    /// Radar X-band (10.0 GHz) attenuation in dB
    pub attenuation_x_band_db: f64,
    /// Satellite Ka-band (35.0 GHz) attenuation in dB
    pub attenuation_ka_band_db: f64,
    /// Optical laser channel (193.4 THz / 1550 nm) attenuation in dB
    pub attenuation_optical_db: f64,
    /// Whether standard RF communications experience blackout
    pub is_rf_blackout: bool,
    /// Minimum magnetic field (Tesla) required to open an MHD Whistler window
    pub mhd_window_b_field_tesla: f64,
}

/// Evaluates nonequilibrium ionization and plasma sheath blackout along the vehicle.
#[must_use]
pub fn evaluate_plasma_sheath(
    mach: f64,
    altitude_m: f64,
    ambient_temp_k: f64,
    ambient_density_kg_m3: f64,
    nose_radius_m: f64,
) -> PlasmaSheathResult {
    let speed_of_light = 299_792_458.0;
    let elem_charge = 1.602_176_634e-19;
    let eps0 = 8.854_187_812_8e-12;
    let electron_mass = 9.109_383_701_5e-31;

    // 1. Shock layer stagnation temperature with vibrational excitation & dissociation
    // High-temperature air real gas approximation: T_shock ~ T_inf * (1 + 0.18 * M^2)^0.88
    let shock_temp_k = ambient_temp_k * (1.0 + 0.18 * mach.powi(2)).powf(0.88);

    // 2. Equilibrium/nonequilibrium electron number density via associative ionization
    // Dominant mechanism in hypersonic air: N + O <-> NO+ + e- (effective threshold ~ 2.8 eV)
    let assoc_ionization_ev = 2.80;
    let k_boltzmann_ev = 8.617_333_262e-5; // eV/K
    let exp_term = (-assoc_ionization_ev / (k_boltzmann_ev * shock_temp_k.max(300.0))).exp();

    // Neutral particle number density: n = rho / m_air (m_air ~ 4.81e-26 kg)
    let n_neutral = ambient_density_kg_m3 / 4.81e-26;
    let base_ne = 4.0e22 * (shock_temp_k / 5000.0).powf(1.5) * exp_term;
    let electron_density_m3 = base_ne.clamp(1.0e16, 5.0e20);

    // 3. Angular plasma frequency: omega_p = sqrt(n_e * e^2 / (eps0 * m_e))
    let omega_p = ((electron_density_m3 * elem_charge * elem_charge) / (eps0 * electron_mass)).sqrt();
    let fp_ghz = (omega_p / (2.0 * std::f64::consts::PI)) / 1.0e9;

    // 4. Electron-neutral collision frequency: nu_en ~ n_neutral * sigma * v_th
    let v_thermal_e = ((8.0 * 1.380_649e-23 * shock_temp_k) / (std::f64::consts::PI * electron_mass)).sqrt();
    let collision_cross_section = 1.5e-19; // m^2
    let nu_en = n_neutral * collision_cross_section * v_thermal_e;

    // 5. Boundary layer sheath thickness: delta ~ 0.38 * x / Re_x^0.2, scaled with nose radius
    let sheath_thickness_m = (0.05 * (nose_radius_m / 0.05).sqrt() * (altitude_m / 30000.0).powf(0.3)).clamp(0.02, 0.25);

    // Helper: compute electromagnetic wave attenuation through plasma sheath in dB
    // Attenuation constant: alpha = (omega / c) * Im(sqrt(1 - omega_p^2 / (omega * (omega - i * nu))))
    let calc_attenuation_db = |freq_ghz: f64| -> f64 {
        let omega = 2.0 * std::f64::consts::PI * freq_ghz * 1.0e9;
        let denom = omega.powi(2) + nu_en.powi(2);
        let eps_real = 1.0 - (omega_p.powi(2) / denom);
        let eps_imag = (omega_p.powi(2) * nu_en) / (omega * denom);

        // Magnitude and phase of complex permittivity
        let mag = (eps_real.powi(2) + eps_imag.powi(2)).sqrt();
        let phase = eps_imag.atan2(eps_real);
        let k_imag = (omega / speed_of_light) * mag.sqrt() * (phase / 2.0).sin().abs();

        let atten_np = k_imag * sheath_thickness_m;
        (atten_np * 8.685_889_638).clamp(0.0, 180.0) // 1 Np = 8.686 dB
    };

    let atten_l = calc_attenuation_db(1.575); // GPS L1
    let atten_x = calc_attenuation_db(10.0);  // X-Band radar / downlink
    let atten_ka = calc_attenuation_db(35.0); // Ka-Band high-bandwidth satellite
    let atten_opt = 0.005;                    // 1550 nm optical laser communication (193.4 THz)

    let is_rf_blackout = atten_x > 20.0 || atten_l > 20.0;

    // Minimum magnetic field B to induce Whistler mode transmission: omega_c = e*B/m_e >= omega_p^2 / omega
    let target_omega = 2.0 * std::f64::consts::PI * 10.0e9; // 10 GHz
    let req_omega_c = omega_p.powi(2) / target_omega;
    let mhd_b_field = (req_omega_c * electron_mass / elem_charge).clamp(0.1, 2.5);

    PlasmaSheathResult {
        shock_temperature_k: shock_temp_k,
        electron_density_m3,
        plasma_frequency_rad_s: omega_p,
        plasma_cutoff_freq_ghz: fp_ghz,
        collision_frequency_hz: nu_en,
        sheath_thickness_m,
        attenuation_l_band_db: atten_l,
        attenuation_x_band_db: atten_x,
        attenuation_ka_band_db: atten_ka,
        attenuation_optical_db: atten_opt,
        is_rf_blackout,
        mhd_window_b_field_tesla: mhd_b_field,
    }
}

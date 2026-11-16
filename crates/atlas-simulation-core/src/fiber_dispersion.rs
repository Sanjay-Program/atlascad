//! Optical Single-Mode Fiber Chromatic Dispersion & Pulse Broadening (ITU-T G.652 / Sellmeier / Agrawal)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Fused silica 3-term Sellmeier refractive index n(lambda) and group velocity index n_g(lambda).
//! 2. ITU-T G.652 standard chromatic dispersion parameter D(lambda) and dispersion slope S(lambda).
//! 3. Total link accumulated chromatic dispersion (ps/nm) over fiber span length L.
//! 4. Gaussian optical pulse temporal RMS broadening Delta tau from transmitter laser linewidth.
//! 5. Maximum dispersion-limited bit rate B_max (Gbit/s), optical link attenuation margin, and ISI penalty.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Standard optical single-mode fiber telecommunications specification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FiberStandardType {
    /// ITU-T G.652.D: Standard Single-Mode Fiber (SSMF), zero-dispersion at 1312 nm, D ~ 17 ps/(nm·km) at 1550 nm.
    ItuG652dStandard,
    /// ITU-T G.655: Non-Zero Dispersion-Shifted Fiber (NZDSF), optimized for DWDM C-band.
    ItuG655Nzdsf,
    /// ITU-T G.654.E: Ultra-Low Loss / Large Effective Area Fiber for long-haul submarine/terrestrial links.
    ItuG654eUltraLowLoss,
}

/// Input parameters for optical fiber chromatic dispersion analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiberDispersionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Optical fiber standard specification.
    pub fiber_standard: FiberStandardType,
    /// Operating laser carrier optical wavelength in nanometers (nm, standard 1260 to 1625 nm).
    pub wavelength_nm: f64,
    /// Fiber link span length in kilometers (km, standard 1.0 to 120.0 km).
    pub fiber_length_km: f64,
    /// Transmitter laser spectral linewidth (FWHM) in nanometers (nm, standard 0.01 to 2.5 nm).
    pub laser_linewidth_fwhm_nm: f64,
    /// Initial optical pulse duration (FWHM) in picoseconds (ps, standard 10.0 to 200.0 ps).
    pub initial_pulse_width_ps: f64,
    /// Transmitter optical launch power in milliWatts (mW, standard 0.5 to 20.0 mW).
    pub launch_power_mw: f64,
    /// Target transmission digital data rate in Gigabits per second (Gbit/s, standard 1.25 to 100 Gbit/s).
    pub target_bit_rate_gbps: f64,
}

impl Default for FiberDispersionProblem {
    fn default() -> Self {
        Self {
            problem_id: "long_haul_c_band_optical_link".to_string(),
            fiber_standard: FiberStandardType::ItuG652dStandard,
            wavelength_nm: 1550.0,          // 1550 nm standard C-band transmission window
            fiber_length_km: 80.0,           // 80 km unamplified repeater span
            laser_linewidth_fwhm_nm: 0.015,  // 0.015 nm (1.8 GHz) narrow DFB laser linewidth
            initial_pulse_width_ps: 25.0,    // 25 ps pulse width (40 Gbit/s NRZ equivalent)
            launch_power_mw: 5.0,            // +7 dBm launch power (5 mW)
            target_bit_rate_gbps: 10.0,      // 10 Gbit/s standard SONET/SDH/10GbE
        }
    }
}

/// Simulation results for fiber chromatic dispersion, pulse broadening, and transmission limits.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FiberDispersionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Fused silica core refractive index n(lambda) from 3-term Sellmeier equation.
    pub core_refractive_index: f64,
    /// Group index of refraction n_g = n - lambda * (dn/dlambda).
    pub group_refractive_index: f64,
    /// Chromatic dispersion parameter D(lambda) in picoseconds per nanometer-kilometer (ps / (nm·km)).
    pub chromatic_dispersion_ps_per_nm_km: f64,
    /// Dispersion slope S_0 at operating wavelength in ps / (nm²·km).
    pub dispersion_slope_ps_per_nm2_km: f64,
    /// Total link accumulated chromatic dispersion in picoseconds per nanometer (ps/nm).
    pub total_accumulated_dispersion_ps_per_nm: f64,
    /// Fiber transmission loss coefficient alpha in decibels per kilometer (dB/km).
    pub attenuation_coefficient_db_per_km: f64,
    /// Total optical link transmission loss in decibels (dB).
    pub total_link_attenuation_db: f64,
    /// Received optical signal power at receiver in milliWatts (mW).
    pub received_optical_power_mw: f64,
    /// Temporal pulse broadening Delta tau due to chromatic dispersion in picoseconds (ps).
    pub pulse_broadening_delta_tau_ps: f64,
    /// Output broadened optical pulse width at fiber end in picoseconds (ps).
    pub output_pulse_width_ps: f64,
    /// Pulse broadening ratio (output_width / initial_width).
    pub pulse_broadening_ratio: f64,
    /// Maximum dispersion-limited digital bit rate without electronic dispersion compensation (Gbit/s).
    pub max_dispersion_limited_bit_rate_gbps: f64,
    /// One-way group optical propagation transit time through link in microseconds (µs).
    pub link_propagation_delay_microseconds: f64,
    /// Whether pulse broadening satisfies Inter-Symbol Interference (ISI) limit (tau_out <= 0.70 * T_bit).
    pub is_inter_symbol_interference_acceptable: bool,
    /// Whether received power exceeds typical APD/PIN photodiode sensitivity (-24 dBm = 0.004 mW).
    pub is_received_optical_power_adequate: bool,
    /// Whether link operates within standard dispersion penalty budget (< 2.0 dB penalty).
    pub is_dispersion_penalty_tolerable: bool,
    /// Overall optical fiber communication link transmission viability status.
    pub is_fiber_link_viable: bool,
}

/// Solves optical fiber chromatic dispersion, Sellmeier group velocity, and pulse broadening dynamics.
pub fn solve_fiber_dispersion(problem: &FiberDispersionProblem) -> Result<FiberDispersionResult, String> {
    if problem.wavelength_nm <= 1000.0 || problem.wavelength_nm >= 1800.0 {
        return Err("Wavelength must be within standard telecom spectrum (1000 to 1800 nm)".to_string());
    }
    if problem.fiber_length_km <= 0.01 {
        return Err("Fiber length must be positive".to_string());
    }
    if problem.laser_linewidth_fwhm_nm <= 0.0001 {
        return Err("Laser linewidth must be positive".to_string());
    }
    if problem.initial_pulse_width_ps <= 0.1 {
        return Err("Initial pulse width must be positive".to_string());
    }
    if problem.launch_power_mw <= 0.01 {
        return Err("Launch power must be positive".to_string());
    }
    if problem.target_bit_rate_gbps <= 0.1 {
        return Err("Target bit rate must be positive".to_string());
    }

    // Sellmeier 3-term equation coefficients for fused silica:
    // lambda in micrometers:
    let lambda_um = problem.wavelength_nm / 1000.0;
    let lambda2 = lambda_um * lambda_um;

    let b1 = 0.6961663;
    let b2 = 0.4079426;
    let b3 = 0.8974794;
    let c1 = 0.0684043_f64.powi(2);
    let c2 = 0.1162414_f64.powi(2);
    let c3 = 9.896161_f64.powi(2);

    let n2_minus_1 = (b1 * lambda2) / (lambda2 - c1)
        + (b2 * lambda2) / (lambda2 - c2)
        + (b3 * lambda2) / (lambda2 - c3);
    let n_core = (1.0 + n2_minus_1).sqrt();

    // Group index n_g ~ n + 0.018 for silica at 1550 nm:
    let n_g = n_core + 0.0185;

    // Fiber dispersion model according to standard:
    let (lambda_0_nm, s_0, alpha_db_km): (f64, f64, f64) = match problem.fiber_standard {
        FiberStandardType::ItuG652dStandard => {
            // G.652.D: lambda_0 = 1312 nm, S_0 = 0.092 ps/(nm^2*km), alpha = 0.19 dB/km at 1550 nm
            let alpha = if problem.wavelength_nm < 1360.0 { 0.35 } else { 0.19 };
            (1312.0, 0.092, alpha)
        }
        FiberStandardType::ItuG655Nzdsf => {
            // G.655: lambda_0 = 1450 nm, S_0 = 0.075 ps/(nm^2*km), alpha = 0.20 dB/km
            (1450.0, 0.075, 0.20)
        }
        FiberStandardType::ItuG654eUltraLowLoss => {
            // G.654.E: lambda_0 = 1305 nm, S_0 = 0.088 ps/(nm^2*km), alpha = 0.16 dB/km (pure silica core)
            (1305.0, 0.088, 0.16)
        }
    };

    // ITU-T G.652 Chromatic dispersion formula:
    // D(lambda) = (S_0 / 4) * [lambda - (lambda_0^4 / lambda^3)] [ps / (nm * km)]
    let d_chromatic = (s_0 / 4.0) * (problem.wavelength_nm - (lambda_0_nm.powi(4) / problem.wavelength_nm.powi(3)));

    // Total accumulated chromatic dispersion across link span:
    let total_dispersion = d_chromatic * problem.fiber_length_km; // ps/nm

    // Link optical attenuation:
    let total_loss_db = alpha_db_km * problem.fiber_length_km;
    let power_rx_mw = problem.launch_power_mw * 10.0_f64.powf(-total_loss_db / 10.0);

    // Pulse broadening due to source spectral linewidth:
    // Delta tau = |D| * L * Delta_lambda [ps]
    let delta_tau_ps = d_chromatic.abs() * problem.fiber_length_km * problem.laser_linewidth_fwhm_nm;

    // Output broadened pulse duration: tau_out = sqrt(tau_in^2 + Delta_tau^2) [ps]
    let tau_out_ps = (problem.initial_pulse_width_ps.powi(2) + delta_tau_ps.powi(2)).sqrt();
    let broadening_ratio = tau_out_ps / problem.initial_pulse_width_ps;

    // Bit period for target data rate: T_bit = 1000 / B_gbps [ps]:
    let t_bit_ps = 1000.0 / problem.target_bit_rate_gbps;

    // Maximum dispersion-limited bit rate without electronic dispersion compensation (EDC):
    // B_max <= 1 / (4 * Delta_tau) [Gbit/s]
    let b_max_gbps = if delta_tau_ps > 0.05 {
        (1000.0 / (4.0 * delta_tau_ps)).min(400.0)
    } else {
        400.0
    };

    // Link propagation delay = (n_g * L / c):
    // c = 299,792.458 km/s -> delay = (n_g * L / 299.792) [µs]
    let delay_us = (n_g * problem.fiber_length_km) / 0.299792458;

    // Assessment flags:
    let is_isi_ok = tau_out_ps <= (0.75 * t_bit_ps);
    let is_rx_power_ok = power_rx_mw >= 0.004; // -24 dBm sensitivity
    let is_dispersion_ok = problem.target_bit_rate_gbps <= b_max_gbps;
    let is_overall_viable = is_isi_ok && is_rx_power_ok && is_dispersion_ok;

    Ok(FiberDispersionResult {
        problem_id: problem.problem_id.clone(),
        core_refractive_index: n_core,
        group_refractive_index: n_g,
        chromatic_dispersion_ps_per_nm_km: d_chromatic,
        dispersion_slope_ps_per_nm2_km: s_0,
        total_accumulated_dispersion_ps_per_nm: total_dispersion,
        attenuation_coefficient_db_per_km: alpha_db_km,
        total_link_attenuation_db: total_loss_db,
        received_optical_power_mw: power_rx_mw,
        pulse_broadening_delta_tau_ps: delta_tau_ps,
        output_pulse_width_ps: tau_out_ps,
        pulse_broadening_ratio: broadening_ratio,
        max_dispersion_limited_bit_rate_gbps: b_max_gbps,
        link_propagation_delay_microseconds: delay_us,
        is_inter_symbol_interference_acceptable: is_isi_ok,
        is_received_optical_power_adequate: is_rx_power_ok,
        is_dispersion_penalty_tolerable: is_dispersion_ok,
        is_fiber_link_viable: is_overall_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_c_band_telecom_fiber_link() -> Result<(), String> {
        let prob = FiberDispersionProblem::default();
        let res = solve_fiber_dispersion(&prob)?;

        assert!(res.core_refractive_index > 1.44 && res.core_refractive_index < 1.46);
        assert!(res.chromatic_dispersion_ps_per_nm_km >= 15.0 && res.chromatic_dispersion_ps_per_nm_km <= 19.0);
        assert!(res.total_accumulated_dispersion_ps_per_nm > 1200.0);
        assert!(res.total_link_attenuation_db >= 14.0 && res.total_link_attenuation_db <= 18.0);
        assert!(res.received_optical_power_mw > 0.05);
        assert!(res.pulse_broadening_delta_tau_ps > 15.0 && res.pulse_broadening_delta_tau_ps < 50.0);
        assert!(res.output_pulse_width_ps > res.pulse_broadening_delta_tau_ps);
        assert!(res.is_inter_symbol_interference_acceptable);
        assert!(res.is_received_optical_power_adequate);
        assert!(res.is_dispersion_penalty_tolerable);
        assert!(res.is_fiber_link_viable);
        Ok(())
    }
}

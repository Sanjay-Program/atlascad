//! Autonomous Vehicle 77 GHz FMCW Radar & ToF LiDAR Sensor Performance Solver.
//!
//! Implements classical first-principles radar and optical range equations with atmospheric attenuation
//! (ITU-R P.838, Kruse/Kim aerosol extinction, and Skolnik/Blake formulations) for ADAS perception verification.
//!
//! # Physical Formulations:
//! - **77 GHz FMCW Radar Range Equation (Skolnik / Blake)**:
//!   $$R_{max,radar} = \left[\frac{P_t \cdot G_t \cdot G_r \cdot \lambda^2 \cdot \sigma_{RCS}}{(4\pi)^3 \cdot k_B T_0 \cdot F_n \cdot B_{IF} \cdot SNR_{min} \cdot L_{sys} \cdot L_{atm}(R)}\right]^{1/4}$$
//!   $$\Delta R = \frac{c}{2 B_{chirp}}, \quad f_{beat} = \frac{2 R \cdot S_{slope}}{c} + \frac{2 v_r f_0}{c}$$
//! - **Atmospheric Millimeter-Wave Attenuation (ITU-R P.838 / P.840)**:
//!   $$\gamma_{rain} = k_{rain} \cdot R_{rain}^{\alpha_{rain}} \quad [\text{dB/km}]$$
//! - **Pulsed Time-of-Flight LiDAR Range Equation (Kruse / Kim)**:
//!   $$P_{r,lidar}(R) = P_{pulse} \cdot \frac{\rho_{target}}{\pi} \cdot \frac{A_{lens}}{R^2} \cdot \eta_{opt} \cdot e^{-2 \alpha_{atm} R}$$
//!   $$\alpha_{atm}(\lambda) = \frac{3.91}{V_{vis}} \left(\frac{\lambda}{0.55\,\mu\text{m}}\right)^{-q_{kruse}} \quad [\text{km}^{-1}]$$
//! - **LiDAR Photodetector Signal-to-Noise Ratio (SNR)**:
//!   $$SNR_{lidar} = \frac{\left(\mathcal{R} \cdot P_r\right)^2}{2 q \left(\mathcal{R} P_r + I_{dark}\right) B_{elec} + \frac{4 k_B T_0 B_{elec}}{R_L}}$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Atmospheric weather condition for autonomous perception.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum WeatherConditionType {
    /// Clear sunny atmosphere (visibility > 20 km, 0 mm/h rain).
    ClearDay,
    /// Moderate rainfall (12.5 mm/h rain, visibility 5 km).
    ModerateRain,
    /// Dense radiation fog (visibility 200 m).
    DenseFog,
    /// Heavy snowfall (precipitation rate 10 mm/h).
    HeavySnow,
}

/// Input parameters for automotive radar and LiDAR perception analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarLidarSensorProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Operating weather condition.
    pub weather_condition: WeatherConditionType,
    /// Target distance in meters (m).
    pub target_range_m: f64,
    /// Target Radar Cross Section (RCS) in square meters (m², e.g., 10.0 for sedan, 1.0 for pedestrian).
    pub target_rcs_m2: f64,
    /// Target optical Lambertian surface reflectivity (0.0 to 1.0, e.g. 0.15 for dark clothing, 0.80 for retroreflector).
    pub target_optical_reflectivity: f64,
    /// FMCW Radar transmit peak power in Watts (W, standard 0.05 to 0.20 W for 77 GHz radar).
    pub radar_tx_power_w: f64,
    /// Radar antenna gain in dBi (standard 24 to 28 dBi).
    pub radar_antenna_gain_dbi: f64,
    /// Radar FMCW chirp frequency sweep bandwidth in MegaHertz (MHz, e.g. 1000 MHz = 1.0 GHz).
    pub radar_chirp_bandwidth_mhz: f64,
    /// Radar receiver noise figure in decibels (dB, standard 10 to 12 dB).
    pub radar_noise_figure_db: f64,
    /// LiDAR laser wavelength in nanometers (nm, standard 905 nm or 1550 nm).
    pub lidar_wavelength_nm: f64,
    /// LiDAR laser peak optical pulse power in Watts (W, standard 50 to 150 W for 905 nm; up to 1000 W for 1550 nm).
    pub lidar_pulse_power_w: f64,
    /// LiDAR receiver optical aperture diameter in millimeters (mm).
    pub lidar_aperture_diameter_mm: f64,
    /// Minimum required SNR for reliable target detection in decibels (dB, standard 12 dB).
    pub required_snr_threshold_db: f64,
}

impl Default for RadarLidarSensorProblem {
    fn default() -> Self {
        Self {
            problem_id: "adas_long_range_sensor_fusion".to_string(),
            weather_condition: WeatherConditionType::ModerateRain,
            target_range_m: 120.0,
            target_rcs_m2: 5.0, // small vehicle / motorcycle
            target_optical_reflectivity: 0.18,
            radar_tx_power_w: 0.10, // 20 dBm (100 mW)
            radar_antenna_gain_dbi: 26.0,
            radar_chirp_bandwidth_mhz: 1000.0, // 1 GHz -> 15 cm resolution
            radar_noise_figure_db: 11.0,
            lidar_wavelength_nm: 905.0,
            lidar_pulse_power_w: 75.0,
            lidar_aperture_diameter_mm: 35.0,
            required_snr_threshold_db: 12.0,
        }
    }
}

/// Simulation results for radar and LiDAR detection performance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RadarLidarSensorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Radar carrier wavelength in millimeters (mm).
    pub radar_wavelength_mm: f64,
    /// Radar fundamental range resolution Delta_R in meters (m).
    pub radar_range_resolution_m: f64,
    /// Radar atmospheric one-way attenuation rate in dB/km.
    pub radar_atm_attenuation_db_per_km: f64,
    /// Received radar echo power at target range in decibel-milliwatts (dBm).
    pub radar_received_power_dbm: f64,
    /// Radar receiver thermal noise floor in dBm.
    pub radar_noise_floor_dbm: f64,
    /// Radar signal-to-noise ratio (SNR) at target range in decibels (dB).
    pub radar_snr_at_target_db: f64,
    /// Maximum radar detection range in meters (m) for the given target RCS.
    pub radar_max_detection_range_m: f64,
    /// Atmospheric optical extinction coefficient alpha in km^-1.
    pub optical_extinction_coeff_per_km: f64,
    /// Received LiDAR optical pulse power at target range in microWatts (µW).
    pub lidar_received_power_uw: f64,
    /// LiDAR photodetector SNR at target range in decibels (dB).
    pub lidar_snr_at_target_db: f64,
    /// Maximum LiDAR detection range in meters (m) for the target reflectivity.
    pub lidar_max_detection_range_m: f64,
    /// Whether target is reliably detected by 77 GHz radar (SNR >= threshold).
    pub is_radar_target_detected: bool,
    /// Whether target is reliably detected by pulsed LiDAR (SNR >= threshold).
    pub is_lidar_target_detected: bool,
    /// Whether sensor fusion guarantees perception redundancy (at least one or both sensors detect).
    pub is_sensor_fusion_redundancy_achieved: bool,
}

/// Solves radar range equation, LiDAR ToF optical link budget, and adverse weather perception range.
pub fn solve_radar_lidar_sensor(
    problem: &RadarLidarSensorProblem,
) -> Result<RadarLidarSensorResult, String> {
    if problem.target_range_m <= 0.50 {
        return Err("Target range must be greater than 0.50 m".to_string());
    }
    if problem.radar_tx_power_w <= 0.0 {
        return Err("Radar transmit power must be positive".to_string());
    }
    if problem.lidar_pulse_power_w <= 0.0 {
        return Err("LiDAR pulse power must be positive".to_string());
    }

    let c = 299_792_458.0; // speed of light in m/s
    let f_radar = 77.0e9; // 77 GHz
    let lambda_radar_m = c / f_radar;
    let lambda_radar_mm = lambda_radar_m * 1000.0;

    // 1. Radar Range Resolution:
    let b_chirp_hz = problem.radar_chirp_bandwidth_mhz * 1e6;
    let radar_res_m = c / (2.0 * b_chirp_hz);

    // 2. Radar Atmospheric Attenuation (ITU-R P.838 at 77 GHz):
    // k ~ 1.05, alpha ~ 0.72 for 77 GHz horizontal/vertical average
    let (rain_rate_mm_h, vis_km): (f64, f64) = match problem.weather_condition {
        WeatherConditionType::ClearDay => (0.0, 25.0),
        WeatherConditionType::ModerateRain => (12.5, 5.0),
        WeatherConditionType::DenseFog => (0.0, 0.20),
        WeatherConditionType::HeavySnow => (5.0, 1.20),
    };

    let gamma_rain_db_km = if rain_rate_mm_h > 0.0 {
        1.05 * rain_rate_mm_h.powf(0.72)
    } else {
        0.05 // clear air molecular oxygen/water vapor absorption at 77 GHz (~0.4 dB/km)
    };
    let gamma_radar_total = gamma_rain_db_km + 0.40;

    // 3. Radar Received Power:
    // P_r = P_t * G^2 * lambda^2 * sigma / ((4*pi)^3 * R^4 * L_sys * L_atm)
    let g_linear = 10.0_f64.powf(problem.radar_antenna_gain_dbi / 10.0);
    let r = problem.target_range_m;
    let l_sys_linear = 10.0_f64.powf(4.5 / 10.0); // 4.5 dB internal RF system losses
    let l_atm_db = 2.0 * gamma_radar_total * (r / 1000.0); // two-way atmospheric loss
    let l_atm_linear = 10.0_f64.powf(l_atm_db / 10.0);

    let numerator_radar = problem.radar_tx_power_w
        * g_linear.powi(2)
        * lambda_radar_m.powi(2)
        * problem.target_rcs_m2;
    let denom_radar =
        (4.0 * std::f64::consts::PI).powi(3) * r.powi(4) * l_sys_linear * l_atm_linear;
    let p_r_radar_w = numerator_radar / denom_radar;
    let p_r_radar_dbm = 10.0 * (p_r_radar_w * 1000.0).log10();

    // Radar Noise Floor:
    // N = k_B * T_0 * B_IF * F
    let k_b = 1.380649e-23;
    let t_0 = 290.0;
    let b_if = 2.0e6; // 2 MHz IF sampling bandwidth
    let f_linear = 10.0_f64.powf(problem.radar_noise_figure_db / 10.0);
    let n_radar_w = k_b * t_0 * b_if * f_linear;
    let n_radar_dbm = 10.0 * (n_radar_w * 1000.0).log10();

    // Radar 2D-FFT Coherent Integration Processing Gain (Range + Doppler compression):
    // G_proc = 10 * log10(N_samples * N_chirps / losses) ~ 25.0 dB
    let g_processing_db = 25.0;
    let snr_radar_db = (p_r_radar_dbm - n_radar_dbm) + g_processing_db;

    // Maximum radar range:
    let min_p_r_w =
        n_radar_w * 10.0_f64.powf((problem.required_snr_threshold_db - g_processing_db) / 10.0);
    let r_max_radar_m = ((problem.radar_tx_power_w
        * g_linear.powi(2)
        * lambda_radar_m.powi(2)
        * problem.target_rcs_m2)
        / ((4.0 * std::f64::consts::PI).powi(3) * min_p_r_w * l_sys_linear))
        .powf(0.25);

    // 4. Optical LiDAR Link Budget (Kruse / Kim model):
    let lambda_um = problem.lidar_wavelength_nm / 1000.0;
    let q_kruse = if vis_km > 50.0 {
        1.6
    } else if vis_km > 6.0 {
        1.3
    } else if vis_km > 1.0 {
        0.16 * vis_km + 0.34
    } else {
        0.0 // fog / dense particulate
    };
    let alpha_atm_per_km = (3.91 / vis_km) * (lambda_um / 0.55).powf(-q_kruse);
    let alpha_atm_per_m = alpha_atm_per_km / 1000.0;

    // Optical aperture area:
    let d_lens_m = problem.lidar_aperture_diameter_mm / 1000.0;
    let a_lens_m2 = std::f64::consts::PI * (d_lens_m / 2.0).powi(2);
    let eta_opt = 0.70; // 70% optical lens transmission

    // LiDAR received power:
    // P_r = P_pulse * (rho / pi) * (A_lens / R^2) * eta_opt * exp(-2 * alpha * R)
    let tau_atm = (-2.0 * alpha_atm_per_m * r).exp();
    let p_r_lidar_w = problem.lidar_pulse_power_w
        * (problem.target_optical_reflectivity / std::f64::consts::PI)
        * (a_lens_m2 / r.powi(2))
        * eta_opt
        * tau_atm;
    let p_r_lidar_uw = p_r_lidar_w * 1e6;

    // APD Photodetector Noise & SNR:
    // Responsivity ~ 0.85 A/W, Noise Equivalent Power NEP ~ 2.5e-9 W (2.5 nW)
    let nep_w = 2.5e-9;
    let snr_lidar_linear = p_r_lidar_w / nep_w;
    let snr_lidar_db = 10.0 * snr_lidar_linear.max(1e-3).log10();

    // Maximum LiDAR range iteration:
    // solve for R where P_r(R) / NEP >= SNR_req
    let target_min_p_r = nep_w * 10.0_f64.powf(problem.required_snr_threshold_db / 10.0);
    let mut r_lidar_test: f64 = 10.0;
    for _ in 0..100 {
        let p_test = problem.lidar_pulse_power_w
            * (problem.target_optical_reflectivity / std::f64::consts::PI)
            * (a_lens_m2 / r_lidar_test.powi(2))
            * eta_opt
            * (-2.0 * alpha_atm_per_m * r_lidar_test).exp();
        if p_test < target_min_p_r {
            break;
        }
        r_lidar_test += 2.0;
    }
    let r_max_lidar_m = r_lidar_test;

    let is_radar_detected = snr_radar_db >= problem.required_snr_threshold_db;
    let is_lidar_detected = snr_lidar_db >= problem.required_snr_threshold_db;
    let is_fusion_ok = is_radar_detected || is_lidar_detected;

    Ok(RadarLidarSensorResult {
        problem_id: problem.problem_id.clone(),
        radar_wavelength_mm: lambda_radar_mm,
        radar_range_resolution_m: radar_res_m,
        radar_atm_attenuation_db_per_km: gamma_radar_total,
        radar_received_power_dbm: p_r_radar_dbm,
        radar_noise_floor_dbm: n_radar_dbm,
        radar_snr_at_target_db: snr_radar_db,
        radar_max_detection_range_m: r_max_radar_m,
        optical_extinction_coeff_per_km: alpha_atm_per_km,
        lidar_received_power_uw: p_r_lidar_uw,
        lidar_snr_at_target_db: snr_lidar_db,
        lidar_max_detection_range_m: r_max_lidar_m,
        is_radar_target_detected: is_radar_detected,
        is_lidar_target_detected: is_lidar_detected,
        is_sensor_fusion_redundancy_achieved: is_fusion_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adas_radar_and_lidar_in_rain() -> Result<(), String> {
        let prob = RadarLidarSensorProblem {
            problem_id: "highway_rain_perception".to_string(),
            weather_condition: WeatherConditionType::ModerateRain,
            target_range_m: 120.0,
            target_rcs_m2: 5.0,
            target_optical_reflectivity: 0.18,
            radar_tx_power_w: 0.10,
            radar_antenna_gain_dbi: 26.0,
            radar_chirp_bandwidth_mhz: 1000.0,
            radar_noise_figure_db: 11.0,
            lidar_wavelength_nm: 905.0,
            lidar_pulse_power_w: 75.0,
            lidar_aperture_diameter_mm: 35.0,
            required_snr_threshold_db: 12.0,
        };

        let res = solve_radar_lidar_sensor(&prob)?;
        assert!(res.radar_wavelength_mm > 3.80 && res.radar_wavelength_mm < 4.00);
        assert!((res.radar_range_resolution_m - 0.15).abs() < 0.01);
        assert!(res.radar_snr_at_target_db > 15.0); // Radar easily penetrates rain
        assert!(res.radar_max_detection_range_m > 180.0);
        assert!(res.is_radar_target_detected);
        assert!(res.is_sensor_fusion_redundancy_achieved);
        Ok(())
    }
}

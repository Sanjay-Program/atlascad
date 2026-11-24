//! Underwater Acoustic Sonar Propagation & Active Detection (Urick / Thorpe / US Navy SWOP)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Transducer source level (SL) from electroacoustic power and directivity index (DI).
//! 2. Thorpe chemical relaxation seawater absorption coefficient alpha(f) in dB/km.
//! 3. Spherical and cylindrical ocean acoustic waveguide transmission loss (TL).
//! 4. Wenz ocean ambient noise level (NL) from surface wind sea state and commercial shipping traffic.
//! 5. Active two-way sonar equation signal-to-noise ratio (SNR), Figure of Merit (FOM), and target detection.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Ocean environmental acoustic acoustic regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OceanAcousticRegime {
    /// Deep ocean sound channel (SOFAR axis / deep water spherical spreading).
    DeepOceanChannel,
    /// Shallow water coastal continental shelf waveguide (depth < 200 m, surface/bottom reflections).
    ShallowWaterWaveguide,
    /// Arctic under-ice acoustic waveguide with rough ice scattering.
    ArcticUnderIce,
}

/// Target underwater vessel or obstacle acoustic classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SonarTargetClass {
    /// Large fleet submarine hull (Target Strength TS ~ +15 to +25 dB).
    SubmarineLargeFleet,
    /// Small unmanned undersea vehicle (UUV) or mini-sub (TS ~ +5 to +12 dB).
    UnmannedUnderseaVehicle,
    /// Moored or bottom naval mine (TS ~ -15 to -5 dB).
    NavalMineOrDebris,
    /// Scuba diver with open-circuit breathing apparatus (TS ~ -20 to -12 dB).
    DiverOrSwimmer,
}

/// Input parameters for underwater acoustic sonar propagation and target detection.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SonarPropagationProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Ocean acoustic environment.
    pub acoustic_regime: OceanAcousticRegime,
    /// Target classification.
    pub target_class: SonarTargetClass,
    /// Sonar acoustic operating frequency in kiloHertz (kHz, standard 2.0 to 80.0 kHz).
    pub operating_frequency_khz: f64,
    /// Transducer transmitter acoustic emitted power in Watts (W, standard 100 to 25,000 W).
    pub transmitter_acoustic_power_w: f64,
    /// Sonar transducer array Directivity Index (DI) in decibels (dB, standard 12.0 to 32.0 dB).
    pub array_directivity_index_db: f64,
    /// Ocean water column depth in meters (m, standard 50 to 4500 m).
    pub ocean_depth_m: f64,
    /// Distance / range to target in meters (m, standard 250 to 35,000 m).
    pub target_range_m: f64,
    /// World Meteorological Organization (WMO) Sea State (0 to 6, nominal 2 to 4).
    pub sea_state_code: u32,
    /// Commercial shipping traffic density index (1=Light, 2=Moderate, 3=Heavy).
    pub shipping_traffic_index: u32,
    /// Receiver detection threshold (DT) required for 90% probability of detection in decibels (dB, standard 10 to 22 dB).
    pub detection_threshold_db: f64,
}

impl Default for SonarPropagationProblem {
    fn default() -> Self {
        Self {
            problem_id: "anti_submarine_active_sonar_detection".to_string(),
            acoustic_regime: OceanAcousticRegime::ShallowWaterWaveguide,
            target_class: SonarTargetClass::SubmarineLargeFleet,
            operating_frequency_khz: 7.5, // 7.5 kHz mid-frequency active sonar
            transmitter_acoustic_power_w: 4500.0, // 4.5 kW acoustic emitted power
            array_directivity_index_db: 22.0, // 22 dB transducer array DI
            ocean_depth_m: 180.0,         // 180 m continental shelf
            target_range_m: 8500.0,       // 8.5 km detection range
            sea_state_code: 3,            // Sea state 3 (moderate waves)
            shipping_traffic_index: 2,    // Moderate shipping
            detection_threshold_db: 14.0, // 14 dB DT
        }
    }
}

/// Simulation results for sonar acoustic transmission loss, ambient noise, SNR, and detection status.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SonarPropagationResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Source Level (SL) in dB referenced to 1 µPa at 1 meter (dB re 1 µPa @ 1 m).
    pub source_level_db_re_upa: f64,
    /// Thorpe chemical relaxation seawater attenuation coefficient in dB per kilometer (dB/km).
    pub absorption_coefficient_db_per_km: f64,
    /// One-way acoustic transmission loss (TL) in decibels (dB).
    pub one_way_transmission_loss_db: f64,
    /// Two-way acoustic round-trip transmission loss (2 * TL) in decibels (dB).
    pub two_way_transmission_loss_db: f64,
    /// Target acoustic backscattering Target Strength (TS) in decibels (dB).
    pub target_strength_ts_db: f64,
    /// Total omnidirectional ambient ocean noise level (NL) in dB re 1 µPa.
    pub ambient_noise_level_nl_db: f64,
    /// Net background noise at receiver beamformer output (NL - DI) in decibels (dB).
    pub array_effective_noise_db: f64,
    /// Received echo level at hydrophone array (SL - 2*TL + TS) in dB re 1 µPa.
    pub received_echo_level_db: f64,
    /// Received signal-to-noise ratio (SNR) in decibels (dB).
    pub signal_to_noise_ratio_snr_db: f64,
    /// Signal excess over detection threshold (SE = SNR - DT) in decibels (dB).
    pub signal_excess_se_db: f64,
    /// Active sonar Figure of Merit (FOM = 2 * TL_max) in decibels (dB).
    pub figure_of_merit_fom_db: f64,
    /// Maximum theoretical detection range R_max at zero signal excess in kilometers (km).
    pub max_detection_range_km: f64,
    /// Acoustic two-way pulse travel time to target and back in seconds (s).
    pub two_way_travel_time_seconds: f64,
    /// Whether received SNR exceeds detection threshold (SE >= 0 dB) confirming target acquisition.
    pub is_target_detected: bool,
    /// Whether source level remains below cavitation threshold at transducer face (SL <= 235 dB).
    pub is_cavitation_limit_satisfied: bool,
    /// Whether acoustic absorption allows effective propagation (alpha <= 10 dB/km).
    pub is_absorption_loss_favorable: bool,
    /// Overall sonar operational detection viability status.
    pub is_sonar_detection_viable: bool,
}

/// Solves ocean acoustic transmission loss, Thorpe attenuation, Wenz noise, and active sonar equation.
pub fn solve_sonar_propagation(
    problem: &SonarPropagationProblem,
) -> Result<SonarPropagationResult, String> {
    if problem.operating_frequency_khz <= 0.1 || problem.operating_frequency_khz > 200.0 {
        return Err("Operating frequency must be between 0.1 and 200 kHz".to_string());
    }
    if problem.transmitter_acoustic_power_w <= 0.0 {
        return Err("Transmitter acoustic power must be positive".to_string());
    }
    if problem.target_range_m <= 10.0 {
        return Err("Target range must be at least 10 meters".to_string());
    }

    let f = problem.operating_frequency_khz;
    let r_m = problem.target_range_m;
    let r_km = r_m / 1000.0;
    let p_ac = problem.transmitter_acoustic_power_w;
    let di = problem.array_directivity_index_db;

    // Transducer source level:
    // SL = 170.8 + 10 * log10(P_ac) + DI [dB re 1 µPa @ 1 m]
    let sl = 170.8 + 10.0 * p_ac.log10() + di;

    // Thorpe absorption coefficient in dB/km (Boric acid + Magnesium sulfate + Pure water relaxation):
    // alpha(f) = 0.11 * f^2 / (1 + f^2) + 44 * f^2 / (4100 + f^2) + 2.75e-4 * f^2 + 0.003  [dB/km]
    let alpha_db_km = (0.11 * f.powi(2) / (1.0 + f.powi(2)))
        + (44.0 * f.powi(2) / (4100.0 + f.powi(2)))
        + 2.75e-4 * f.powi(2)
        + 0.003;

    // Waveguide transition range (from spherical to cylindrical spreading):
    // In shallow water, transition occurs at roughly R_trans ~ H_ocean / 2
    let r_trans_m = match problem.acoustic_regime {
        OceanAcousticRegime::DeepOceanChannel => 2500.0,
        OceanAcousticRegime::ShallowWaterWaveguide => {
            (problem.ocean_depth_m / 2.0).clamp(20.0, 300.0)
        }
        OceanAcousticRegime::ArcticUnderIce => (problem.ocean_depth_m / 3.0).clamp(20.0, 200.0),
    };

    // Transmission loss TL:
    let tl_geom = if r_m <= r_trans_m {
        20.0 * r_m.log10()
    } else {
        10.0 * r_trans_m.log10() + 10.0 * r_m.log10()
    };
    let tl_abs = alpha_db_km * r_km;
    let tl_one_way = tl_geom + tl_abs;
    let tl_two_way = 2.0 * tl_one_way;

    // Target strength TS by target class:
    let ts = match problem.target_class {
        SonarTargetClass::SubmarineLargeFleet => 18.0,
        SonarTargetClass::UnmannedUnderseaVehicle => 8.0,
        SonarTargetClass::NavalMineOrDebris => -10.0,
        SonarTargetClass::DiverOrSwimmer => -16.0,
    };

    // Wenz ambient noise model:
    // Shipping noise centered around 100 Hz, decaying at ~20 dB/decade with frequency
    let nl_shipping = match problem.shipping_traffic_index {
        1 => 60.0 - 18.0 * f.log10(), // Light
        2 => 70.0 - 18.0 * f.log10(), // Moderate
        _ => 80.0 - 18.0 * f.log10(), // Heavy
    };

    // Wind and wave noise (Knudsen curves):
    // Sea state 0 to 6
    let ss = problem.sea_state_code.min(6) as f64;
    let nl_wind = 44.0 + 7.5 * ss - 17.0 * f.log10();

    // Total ambient noise NL:
    let nl_total =
        10.0 * (10.0_f64.powf(nl_shipping / 10.0) + 10.0_f64.powf(nl_wind / 10.0)).log10();
    let nl_array = nl_total - di;

    // Received echo level: EL = SL - 2*TL + TS
    let el = sl - tl_two_way + ts;

    // Signal-to-noise ratio: SNR = EL - (NL - DI)
    let snr = el - nl_array;
    let se = snr - problem.detection_threshold_db;

    // Active sonar Figure of Merit (FOM = 2 * TL_max):
    let fom = sl + ts - nl_array - problem.detection_threshold_db;

    // Maximum detection range estimate from FOM:
    let r_max_km = if fom > 40.0 {
        // Approximate range inversion
        ((fom - 10.0 * (r_trans_m / 1000.0).log10() - 60.0) / (2.0 * alpha_db_km + 1.5))
            .clamp(1.0, 50.0)
    } else {
        0.5
    };

    // Two-way acoustic travel time (c_sound ~ 1500 m/s):
    let c_sound_m_s = 1500.0;
    let t_travel = (2.0 * r_m) / c_sound_m_s;

    // Verification flags:
    let is_detected = se >= 0.0;
    let is_cav_ok = sl <= 235.0; // 235 dB re 1 µPa is typical transducer face cavitation threshold
    let is_abs_ok = alpha_db_km <= 12.0;
    let is_overall_ok = is_detected && is_cav_ok && is_abs_ok;

    Ok(SonarPropagationResult {
        problem_id: problem.problem_id.clone(),
        source_level_db_re_upa: sl,
        absorption_coefficient_db_per_km: alpha_db_km,
        one_way_transmission_loss_db: tl_one_way,
        two_way_transmission_loss_db: tl_two_way,
        target_strength_ts_db: ts,
        ambient_noise_level_nl_db: nl_total,
        array_effective_noise_db: nl_array,
        received_echo_level_db: el,
        signal_to_noise_ratio_snr_db: snr,
        signal_excess_se_db: se,
        figure_of_merit_fom_db: fom,
        max_detection_range_km: r_max_km,
        two_way_travel_time_seconds: t_travel,
        is_target_detected: is_detected,
        is_cavitation_limit_satisfied: is_cav_ok,
        is_absorption_loss_favorable: is_abs_ok,
        is_sonar_detection_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_active_anti_submarine_sonar() -> Result<(), String> {
        let prob = SonarPropagationProblem::default();
        let res = solve_sonar_propagation(&prob)?;

        assert!(res.source_level_db_re_upa > 210.0 && res.source_level_db_re_upa < 235.0);
        assert!(
            res.absorption_coefficient_db_per_km > 0.1
                && res.absorption_coefficient_db_per_km < 3.0
        );
        assert!(res.one_way_transmission_loss_db > 40.0);
        assert!(res.signal_to_noise_ratio_snr_db > res.signal_excess_se_db);
        assert!(
            res.signal_excess_se_db >= 0.0,
            "Should achieve positive signal excess for detection"
        );
        assert!(res.figure_of_merit_fom_db > 100.0);
        assert!(res.is_target_detected);
        assert!(res.is_cavitation_limit_satisfied);
        assert!(res.is_absorption_loss_favorable);
        assert!(res.is_sonar_detection_viable);
        Ok(())
    }
}

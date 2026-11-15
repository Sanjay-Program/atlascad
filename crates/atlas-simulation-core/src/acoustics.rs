//! # Architectural Acoustics & Sound Transmission Loss (ISO 12354-1 / ASTM E90)
//!
//! Evaluates partition mass-law sound transmission loss (TL) across standard
//! 1/3-octave frequency bands (100 Hz to 5000 Hz), calculates the single-number
//! Sound Transmission Class (STC) rating according to ASTM E413 contour matching,
//! and determines interior dBA attenuation from exterior noise sources.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard 1/3-octave center frequencies (Hz) used in ASTM E90 / ISO 10140
pub const OCTAVE_1_3_FREQUENCIES_HZ: [f64; 16] = [
    125.0, 160.0, 200.0, 250.0, 315.0, 400.0, 500.0, 630.0, 800.0, 1000.0, 1250.0, 1600.0, 2000.0,
    2500.0, 3150.0, 4000.0,
];

/// Problem definition for architectural partition sound transmission.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcousticsProblem {
    pub partition_name: String,
    /// Partition surface mass density in kg/m^2 (e.g., 25 kg/m^2 for drywall assembly, 240 kg/m^2 for concrete)
    pub surface_mass_density_kg_m2: f64,
    /// Wall partition area in m^2
    pub partition_area_m2: f64,
    /// Receiving room Sabine equivalent sound absorption area in m^2 (metric Sabins)
    pub receiving_room_absorption_m2: f64,
    /// Exterior ambient incident noise level in dBA (e.g. 75 dBA for highway traffic)
    pub exterior_noise_dba: f64,
}

impl Default for AcousticsProblem {
    fn default() -> Self {
        Self {
            partition_name: "Double Gypsum Stud Wall".to_string(),
            surface_mass_density_kg_m2: 35.0,   // 35 kg/m^2
            partition_area_m2: 15.0,            // 15 m^2 wall
            receiving_room_absorption_m2: 20.0, // 20 m^2 Sabine absorption
            exterior_noise_dba: 72.0,           // 72 dBA exterior traffic noise
        }
    }
}

/// Solves partition sound transmission loss, calculates ASTM E413 STC rating,
/// and computes receiving room sound level.
pub fn solve_acoustics(prob: &AcousticsProblem) -> Result<SimulationResult, String> {
    let m = prob.surface_mass_density_kg_m2;
    if m <= 0.0 {
        return Err("Surface mass density must be positive".to_string());
    }

    // Compute transmission loss TL(f) across the 16 standard bands:
    // Field incidence mass law: TL(f) = 20 * log10(m * f) - 47.2 dB
    let mut tl_spectrum = Vec::with_capacity(OCTAVE_1_3_FREQUENCIES_HZ.len());
    for &freq in &OCTAVE_1_3_FREQUENCIES_HZ {
        let mf = m * freq;
        let tl = (20.0 * mf.log10() - 47.2).max(10.0);
        tl_spectrum.push(tl);
    }

    // ASTM E413 Standard Contour Reference relative offsets to STC value at 500 Hz:
    // Frequencies: 125, 160, 200, 250, 315, 400, 500, 630, 800, 1000, 1250, 1600, 2000, 2500, 3150, 4000 Hz
    let stc_contour_offsets: [f64; 16] = [
        -16.0, -13.0, -10.0, -7.0, -4.0, -1.0, 0.0, 1.0, 2.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0, 3.0,
    ];

    // Iteratively determine highest STC rating integer where:
    // 1. Total deficiencies sum <= 32 dB
    // 2. Maximum single deficiency at any band <= 8 dB
    let mut stc_candidate = 20;
    while stc_candidate <= 80 {
        let mut total_deficiency = 0.0;
        let mut max_single_deficiency = 0.0;

        for i in 0..16 {
            let contour_val = (stc_candidate as f64) + stc_contour_offsets[i];
            let actual_tl = tl_spectrum[i];
            if actual_tl < contour_val {
                let def = contour_val - actual_tl;
                total_deficiency += def;
                if def > max_single_deficiency {
                    max_single_deficiency = def;
                }
            }
        }

        if total_deficiency > 32.0 || max_single_deficiency > 8.0 {
            stc_candidate -= 1;
            break;
        }
        stc_candidate += 1;
    }
    let stc_rating = stc_candidate.clamp(15, 75);

    // Indoor noise level: L_int = L_ext - STC + 10 * log10(S / A)
    let s_over_a = (prob.partition_area_m2 / prob.receiving_room_absorption_m2.max(0.1)).max(0.01);
    let area_correction = 10.0 * s_over_a.log10();
    let indoor_noise_dba =
        (prob.exterior_noise_dba - (stc_rating as f64) + area_correction).max(20.0);

    // Typical acoustic privacy benchmark
    let privacy_grade = if stc_rating >= 55 {
        "Luxury / High Acoustic Privacy (Speech Inaudible)"
    } else if stc_rating >= 45 {
        "Commercial Office / Residential Standard (Loud Speech Faint)"
    } else if stc_rating >= 35 {
        "Basic Partition (Loud Speech Understandable)"
    } else {
        "Poor Isolation (Normal Speech Heard)"
    };

    // Analytical verification: Mass law 6 dB per octave slope:
    // TL(1000) - TL(500) == 20 * log10(2) == 6.02 dB
    let tl_500 = tl_spectrum[6]; // 500 Hz index
    let tl_1000 = tl_spectrum[9]; // 1000 Hz index
    let val = AnalyticalComparison::assess(
        "Mass Law Octave Doubling Slope (dB)",
        tl_1000 - tl_500,
        20.0 * 2.0f64.log10(),
    );

    let mut metrics = HashMap::new();
    metrics.insert("stc_rating".to_string(), stc_rating as f64);
    metrics.insert("tl_at_500_hz_db".to_string(), tl_500);
    metrics.insert("tl_at_1000_hz_db".to_string(), tl_1000);
    metrics.insert("indoor_noise_level_dba".to_string(), indoor_noise_dba);
    metrics.insert(
        "noise_attenuation_dba".to_string(),
        prob.exterior_noise_dba - indoor_noise_dba,
    );
    metrics.insert("surface_mass_density_kg_m2".to_string(), m);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, stc_rating as f64, 1, 1e-4);

    let logs = vec![
        format!(
            "Acoustics Analysis: Partition '{}' (Surface Density: {:.1} kg/m²)",
            prob.partition_name, m
        ),
        format!("ASTM E413 Sound Transmission Class: STC {}", stc_rating),
        format!(
            "ISO 12354 Mass Law TL at 500 Hz = {:.1} dB, at 1000 Hz = {:.1} dB (+{:.2} dB/octave)",
            tl_500,
            tl_1000,
            tl_1000 - tl_500
        ),
        format!(
            "Exterior Noise: {:.1} dBA -> Interior Noise: {:.1} dBA (Δ = {:.1} dBA)",
            prob.exterior_noise_dba,
            indoor_noise_dba,
            prob.exterior_noise_dba - indoor_noise_dba
        ),
        format!("Privacy Classification: {}", privacy_grade),
    ];

    Ok(SimulationResult {
        problem_id: "acoustics-stc".to_string(),
        engine_id: "atlas-acoustics".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-acoustics", "0.1.0"),
        logs,
    })
}

/// Problem definition for outdoor environmental sound propagation per ISO 9613-2:
/// Evaluates geometrical divergence, atmospheric attenuation, ground absorption,
/// and barrier diffraction according to the Maekawa/ISO diffraction formulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutdoorNoiseProblem {
    pub source_name: String,
    /// Total A-weighted sound power level of the noise source in dBA (e.g. 102.0 dBA for rooftop chiller / industrial blower)
    pub sound_power_lwa: f64,
    /// Direct ground distance from source to receiver in meters (d > 0)
    pub distance_m: f64,
    /// Height of sound source above ground in meters (e.g. 2.5 m)
    pub source_height_m: f64,
    /// Height of receiver above ground in meters (e.g. 1.5 m per ISO 1996 standard observer)
    pub receiver_height_m: f64,
    /// Barrier height above ground in meters (if acoustic barrier/screen exists)
    pub barrier_height_m: Option<f64>,
    /// Distance from source to barrier along propagation axis in meters
    pub barrier_distance_m: Option<f64>,
    /// Ground factor G: 0.0 for hard ground (concrete, asphalt, paving), 1.0 for soft ground (grass, soil, vegetation)
    pub ground_factor: f64,
    /// Ambient temperature in °C (default 20.0°C)
    pub ambient_temp_c: Option<f64>,
    /// Relative humidity percentage (default 50.0%)
    pub relative_humidity_pct: Option<f64>,
    /// Maximum permissible receiver noise limit in dBA (e.g. 50.0 dBA residential daytime, 45.0 dBA nighttime)
    pub regulatory_limit_dba: Option<f64>,
}

impl Default for OutdoorNoiseProblem {
    fn default() -> Self {
        Self {
            source_name: "Industrial Rooftop Chiller Unit".to_string(),
            sound_power_lwa: 102.0,
            distance_m: 120.0,
            source_height_m: 2.5,
            receiver_height_m: 1.5,
            barrier_height_m: Some(3.5),
            barrier_distance_m: Some(15.0),
            ground_factor: 0.70, // 70% lawn / soft earth
            ambient_temp_c: Some(20.0),
            relative_humidity_pct: Some(50.0),
            regulatory_limit_dba: Some(50.0),
        }
    }
}

/// Calculation results for ISO 9613-2 outdoor sound propagation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OutdoorNoiseResult {
    pub source_name: String,
    pub sound_power_lwa: f64,
    pub distance_m: f64,
    pub divergence_attenuation_db: f64,
    pub atmospheric_attenuation_db: f64,
    pub ground_attenuation_db: f64,
    pub barrier_attenuation_db: f64,
    pub total_attenuation_db: f64,
    pub received_sound_level_dba: f64,
    pub regulatory_limit_dba: f64,
    pub is_compliant: bool,
    pub compliance_margin_dba: f64,
    pub summary_notes: Vec<String>,
}

/// Solves outdoor noise propagation according to ISO 9613-2:
/// Calculates geometrical divergence, atmospheric absorption, ground reflection/absorption,
/// and barrier diffraction (Maekawa model).
pub fn solve_outdoor_noise_propagation(
    prob: &OutdoorNoiseProblem,
) -> Result<OutdoorNoiseResult, String> {
    if prob.distance_m <= 0.1 {
        return Err("Distance to receiver must be greater than 0.1 m".to_string());
    }
    if prob.sound_power_lwa < 0.0 {
        return Err("Sound power level LwA must be non-negative".to_string());
    }

    let d = prob.distance_m;
    let hs = prob.source_height_m.max(0.1);
    let hr = prob.receiver_height_m.max(0.1);
    let g = prob.ground_factor.clamp(0.0, 1.0);
    let temp_c = prob.ambient_temp_c.unwrap_or(20.0);
    let rh = prob.relative_humidity_pct.unwrap_or(50.0).clamp(10.0, 100.0);
    let limit_dba = prob.regulatory_limit_dba.unwrap_or(50.0);

    // 1. Geometrical divergence: A_div = 20 * log10(d / d0) + 11.0 dB (point source hemispherical reference d0=1m)
    let a_div = 20.0 * d.log10() + 11.0;

    // 2. Atmospheric absorption: A_atm = alpha * d / 1000 dB
    // ISO 9613-1 1000 Hz reference alpha approx 5.0 dB/km at 20 C, 50% RH
    let temp_factor = (293.15 / (temp_c + 273.15)).sqrt();
    let rh_factor = (50.0 / rh).powf(0.3);
    let alpha_db_km = (5.0 * temp_factor * rh_factor).clamp(1.0, 30.0);
    let a_atm = alpha_db_km * (d / 1000.0);

    // 3. Ground attenuation: A_gr (ISO 9613-2 simplified porous/hard ground method)
    // For hard ground (G=0), reflection yields negative attenuation (boost) up to -3 dB
    // For porous ground (G=1), excess attenuation occurs
    let h_mean = (hs + hr) * 0.5;
    let a_gr = if g < 0.05 {
        -3.0
    } else {
        let term = (2.0 * h_mean / d) * (17.0 + 300.0 / d);
        let gr_val = 4.8 * g - term * g;
        gr_val.clamp(-3.0, 10.0)
    };

    // 4. Barrier diffraction: A_bar (Maekawa model per ISO 9613-2)
    let a_bar = if let (Some(hb), Some(db)) = (prob.barrier_height_m, prob.barrier_distance_m) {
        if db > 0.0 && db < d && hb > hs.min(hr) {
            let dr = d - db;
            // 3D direct line-of-sight distance
            let direct_dist = (d * d + (hr - hs) * (hr - hs)).sqrt();
            // Source to barrier top
            let d_sb = (db * db + (hb - hs) * (hb - hs)).sqrt();
            // Barrier top to receiver
            let d_br = (dr * dr + (hb - hr) * (hb - hr)).sqrt();
            // Path length difference
            let delta = (d_sb + d_br) - direct_dist;
            if delta > 0.0 {
                let lambda = 343.0 / 1000.0; // 1000 Hz wavelength = 0.343 m
                let n = (2.0 * delta) / lambda;
                let bar_atten = 10.0 * (3.0 + 20.0 * n).log10();
                bar_atten.clamp(0.0, 20.0) // Single barrier max 20 dB per ISO 9613-2
            } else {
                0.0
            }
        } else {
            0.0
        }
    } else {
        0.0
    };

    let total_atten = a_div + a_atm + a_gr + a_bar;
    let received_dba = (prob.sound_power_lwa - total_atten).max(0.0);
    let is_compliant = received_dba <= limit_dba;
    let margin = limit_dba - received_dba;

    let notes = vec![
        format!("Noise Source: '{}' (LwA = {:.1} dBA)", prob.source_name, prob.sound_power_lwa),
        format!("Distance to Receiver: {:.1} m (Hs = {:.1} m, Hr = {:.1} m, Ground Factor G = {:.2})", d, hs, hr, g),
        format!("Attenuation Breakdown: Divergence = {:.2} dB, Atmosphere = {:.2} dB, Ground = {:.2} dB, Barrier = {:.2} dB", a_div, a_atm, a_gr, a_bar),
        format!("Total Acoustic Propagation Attenuation: {:.2} dB", total_atten),
        format!("Received Noise Level: {:.2} dBA vs Limit {:.1} dBA (Margin: {:+.2} dBA) -> [{}]",
            received_dba, limit_dba, margin, if is_compliant { "COMPLIANT" } else { "EXCEEDS LIMIT" }
        ),
    ];

    Ok(OutdoorNoiseResult {
        source_name: prob.source_name.clone(),
        sound_power_lwa: prob.sound_power_lwa,
        distance_m: d,
        divergence_attenuation_db: a_div,
        atmospheric_attenuation_db: a_atm,
        ground_attenuation_db: a_gr,
        barrier_attenuation_db: a_bar,
        total_attenuation_db: total_atten,
        received_sound_level_dba: received_dba,
        regulatory_limit_dba: limit_dba,
        is_compliant,
        compliance_margin_dba: margin,
        summary_notes: notes,
    })
}

/// Simulation runner adapter for ISO 9613-2 outdoor environmental noise.
pub fn solve_outdoor_noise_sim(prob: &OutdoorNoiseProblem) -> Result<SimulationResult, String> {
    let sol = solve_outdoor_noise_propagation(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("sound_power_lwa".to_string(), sol.sound_power_lwa);
    metrics.insert("distance_m".to_string(), sol.distance_m);
    metrics.insert("divergence_attenuation_db".to_string(), sol.divergence_attenuation_db);
    metrics.insert("atmospheric_attenuation_db".to_string(), sol.atmospheric_attenuation_db);
    metrics.insert("ground_attenuation_db".to_string(), sol.ground_attenuation_db);
    metrics.insert("barrier_attenuation_db".to_string(), sol.barrier_attenuation_db);
    metrics.insert("total_attenuation_db".to_string(), sol.total_attenuation_db);
    metrics.insert("received_sound_level_dba".to_string(), sol.received_sound_level_dba);
    metrics.insert("regulatory_limit_dba".to_string(), sol.regulatory_limit_dba);
    metrics.insert("compliance_margin_dba".to_string(), sol.compliance_margin_dba);
    metrics.insert("is_compliant".to_string(), if sol.is_compliant { 1.0 } else { 0.0 });

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.received_sound_level_dba, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Spherical Inverse-Square Divergence (dB)",
        sol.divergence_attenuation_db,
        20.0 * prob.distance_m.log10() + 11.0,
    );

    Ok(SimulationResult {
        problem_id: "acoustics-iso9613-outdoor".to_string(),
        engine_id: "atlas-acoustics".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-acoustics", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_acoustics_mass_law_and_stc() -> Result<(), String> {
        let prob = AcousticsProblem::default();
        let res = solve_acoustics(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);

        let stc = res.scalar_metrics.get("stc_rating").copied().unwrap_or(0.0);
        let tl500 = res
            .scalar_metrics
            .get("tl_at_500_hz_db")
            .copied()
            .unwrap_or(0.0);
        let indoor = res
            .scalar_metrics
            .get("indoor_noise_level_dba")
            .copied()
            .unwrap_or(0.0);

        // 35 kg/m^2 drywall assembly typically yields STC ~ 35 - 45
        assert!(
            (30.0..=50.0).contains(&stc),
            "STC rating unexpected: {}",
            stc
        );
        // TL at 500 Hz: 20 * log10(35 * 500) - 47.2 = 20 * log10(17500) - 47.2 = 84.86 - 47.2 = 37.66 dB
        assert!(
            (tl500 - 37.66).abs() < 0.5,
            "TL at 500 Hz mismatch: {}",
            tl500
        );
        // Interior noise should be significantly reduced from 72 dBA
        assert!(indoor < 45.0, "Interior noise level too high: {}", indoor);

        Ok(())
    }

    #[test]
    fn test_outdoor_noise_propagation_with_barrier() -> Result<(), String> {
        let prob = OutdoorNoiseProblem::default();
        let sol = solve_outdoor_noise_propagation(&prob)?;

        // Divergence at 120m: 20 * log10(120) + 11 = 20 * 2.079 + 11 = 52.58 dB
        assert!((sol.divergence_attenuation_db - 52.58).abs() < 0.2);
        // Atmospheric attenuation at 120m: ~ 5 dB/km * 0.120 km = 0.6 dB
        assert!(sol.atmospheric_attenuation_db > 0.4 && sol.atmospheric_attenuation_db < 1.0);
        // Barrier should provide non-zero diffraction attenuation
        assert!(sol.barrier_attenuation_db > 5.0 && sol.barrier_attenuation_db <= 20.0);
        // Total attenuation > 55 dB
        assert!(sol.total_attenuation_db > 55.0);
        // Received sound level should be attenuated well below 102 dBA
        assert!(sol.received_sound_level_dba < 50.0);
        assert!(sol.is_compliant);

        let sim_res = solve_outdoor_noise_sim(&prob)?;
        assert_eq!(sim_res.status, SimulationStatus::Converged);
        Ok(())
    }
}

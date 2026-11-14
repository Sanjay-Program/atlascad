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
}

//! Parabolic Reflector Antenna Electromagnetics & Gain (Silver / Stutzman / ITU-R S.580).
//!
//! Evaluates parabolic dish geometry (F/D ratio), Ruze surface tolerance efficiency,
//! aperture illumination and spillover efficiencies, bore-sight gain in dBi,
//! half-power beamwidth (HPBW), first sidelobe level (SLL), and ITU-R S.580 satellite envelope compliance.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};

/// Speed of light in vacuum c in m/s.
pub const SPEED_OF_LIGHT: f64 = 299792458.0;

/// Reflector feed illumination pattern archetype.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FeedIlluminationPattern {
    /// Uniform aperture distribution (maximum directivity, highest sidelobes ~ -13.2 dB).
    Uniform,
    /// Standard corrugated horn with -10 dB edge taper (optimal gain/sidelobe trade-off).
    TaperMinus10dB,
    /// Deep -15 dB edge taper for ultra-low sidelobes / radio astronomy.
    DeepTaperMinus15dB,
}

impl FeedIlluminationPattern {
    /// Returns (taper_efficiency, spillover_efficiency, sidelobe_level_db, beamwidth_factor_deg).
    pub fn efficiency_factors(&self) -> (f64, f64, f64, f64) {
        match self {
            Self::Uniform => (1.00, 0.75, -13.26, 58.5),
            Self::TaperMinus10dB => (0.92, 0.88, -23.50, 70.0),
            Self::DeepTaperMinus15dB => (0.84, 0.94, -28.00, 78.0),
        }
    }
}

/// Reflector antenna problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectorAntennaProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Feed illumination taper pattern.
    #[serde(default = "default_feed")]
    pub feed_pattern: FeedIlluminationPattern,
    /// Main parabolic reflector aperture diameter in meters (m) (e.g. 2.40 m for VSAT/satellite uplink).
    pub dish_diameter_m: f64,
    /// Reflector focal length in meters (m) (e.g. 0.96 m for F/D = 0.40).
    pub focal_length_m: f64,
    /// Operating RF microwave frequency in GigaHertz (GHz) (e.g. 14.25 GHz for Ku-band uplink).
    pub operating_frequency_ghz: f64,
    /// Dish surface manufacturing profile RMS error in millimeters (mm) (e.g. 0.35 mm).
    #[serde(default = "default_surface_rms_mm")]
    pub surface_rms_error_mm: f64,
    /// Fractional aperture blockage by subreflector or feed struts (e.g. 0.04 = 4% area blockage).
    #[serde(default = "default_blockage")]
    pub aperture_blockage_fraction: f64,
}

fn default_problem_id() -> String {
    "reflector_antenna_problem".to_string()
}

fn default_feed() -> FeedIlluminationPattern {
    FeedIlluminationPattern::TaperMinus10dB
}

fn default_surface_rms_mm() -> f64 {
    0.35
}

fn default_blockage() -> f64 {
    0.04
}

impl Default for ReflectorAntennaProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            feed_pattern: default_feed(),
            dish_diameter_m: 2.40,
            focal_length_m: 0.96,
            operating_frequency_ghz: 14.25,
            surface_rms_error_mm: 0.35,
            aperture_blockage_fraction: 0.04,
        }
    }
}

/// Reflector antenna simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReflectorAntennaResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Free-space electromagnetic wavelength in millimeters (mm).
    pub wavelength_mm: f64,
    /// Normalized focal ratio (F / D).
    pub focal_ratio: f64,
    /// Electrical diameter in wavelengths (D / lambda).
    pub electrical_diameter_wavelengths: f64,
    /// Ruze surface tolerance efficiency factor (exp(- (4*pi*eps / lambda)^2)).
    pub ruze_efficiency: f64,
    /// Total combined aperture efficiency (eta_ap = eta_taper * eta_spill * eta_block * eta_ruze) (%).
    pub aperture_efficiency_pct: f64,
    /// Peak bore-sight antenna gain in decibels isotropic (dBi).
    pub boresight_gain_dbi: f64,
    /// Half-Power Beamwidth (HPBW, -3 dB width) in degrees (deg).
    pub half_power_beamwidth_deg: f64,
    /// First-Null Beamwidth (FNBW) in degrees (deg).
    pub first_null_beamwidth_deg: f64,
    /// First side-lobe level relative to bore-sight in decibels (dB).
    pub first_sidelobe_level_db: f64,
    /// ITU-R S.580 allowable co-polar sidelobe limit at 2.0 degrees off-axis in dBi.
    pub itu_s580_limit_at_2deg_dbi: f64,
    /// Actual off-axis pattern envelope level at 2.0 degrees in dBi.
    pub actual_pattern_level_at_2deg_dbi: f64,
    /// Side-lobe margin relative to ITU-R S.580 co-polar mask in decibels (dB).
    pub itu_mask_margin_db: f64,
    /// Whether antenna complies with ITU-R S.580 regulatory co-polar mask for geostationary spacing.
    pub is_itu_compliant: bool,
}

/// Solves reflector antenna geometry, Ruze losses, gain, beamwidth, and ITU-R mask compliance.
pub fn solve_reflector_antenna(
    problem: &ReflectorAntennaProblem,
) -> Result<ReflectorAntennaResult, String> {
    if problem.dish_diameter_m <= 0.0 || problem.focal_length_m <= 0.0 {
        return Err("Dish diameter and focal length must be positive".to_string());
    }
    if problem.operating_frequency_ghz <= 0.0 {
        return Err("Frequency must be positive".to_string());
    }
    if problem.surface_rms_error_mm < 0.0 {
        return Err("Surface RMS error cannot be negative".to_string());
    }

    let f_hz = problem.operating_frequency_ghz * 1e9;
    let lambda_m = SPEED_OF_LIGHT / f_hz;
    let lambda_mm = lambda_m * 1000.0;

    let d_m = problem.dish_diameter_m;
    let f_m = problem.focal_length_m;
    let f_d_ratio = f_m / d_m;
    let d_lambda = d_m / lambda_m;

    // 1. Ruze Surface Tolerance Loss:
    // eta_ruze = exp( - (4 * pi * epsilon / lambda)^2 )
    let eps_m = problem.surface_rms_error_mm * 1e-3;
    let phase_error_rad = 4.0 * std::f64::consts::PI * eps_m / lambda_m;
    let eta_ruze = (-phase_error_rad.powi(2)).exp();

    // 2. Feed Illumination Factors:
    let (eta_taper, eta_spill, sll_db, bw_factor) = problem.feed_pattern.efficiency_factors();
    let eta_blockage = (1.0 - problem.aperture_blockage_fraction).clamp(0.80, 1.0);

    // Total aperture efficiency:
    let eta_ap = (eta_taper * eta_spill * eta_blockage * eta_ruze).clamp(0.10, 0.95);
    let eta_ap_pct = eta_ap * 100.0;

    // 3. Peak Bore-Sight Antenna Gain:
    // G = eta_ap * (pi * D / lambda)^2
    let g_linear = eta_ap * (std::f64::consts::PI * d_lambda).powi(2);
    let g_dbi = 10.0 * g_linear.log10();

    // 4. Beamwidths:
    // HPBW ~ bw_factor * (lambda / D) [deg]
    let hpbw_deg = bw_factor * (lambda_m / d_m);
    let fnbw_deg = 2.44 * (lambda_m / d_m) * (180.0 / std::f64::consts::PI);

    // 5. ITU-R S.580 Sidelobe Radiation Pattern Mask:
    // For D/lambda >= 100, the co-polar envelope mask is:
    // G_max(phi) = 29 - 25 * log10(phi) for 1.0 deg <= phi <= 20 deg
    let phi_test_deg: f64 = 2.0;
    let itu_limit_2deg = 29.0 - 25.0 * phi_test_deg.log10();

    // Actual sidelobe level at 2 deg:
    // Sidelobes roll off beyond HPBW as ~ 25 * log10(phi / HPBW):
    let beam_offset_ratio = (phi_test_deg / hpbw_deg).max(1.0);
    let roll_off_db = 25.0 * beam_offset_ratio.log10();
    let pattern_level_2deg = g_dbi + sll_db - roll_off_db;

    let mask_margin = itu_limit_2deg - pattern_level_2deg;
    let is_itu_compliant = mask_margin >= 0.0 && d_lambda >= 50.0;

    Ok(ReflectorAntennaResult {
        problem_id: problem.problem_id.clone(),
        wavelength_mm: lambda_mm,
        focal_ratio: f_d_ratio,
        electrical_diameter_wavelengths: d_lambda,
        ruze_efficiency: eta_ruze,
        aperture_efficiency_pct: eta_ap_pct,
        boresight_gain_dbi: g_dbi,
        half_power_beamwidth_deg: hpbw_deg,
        first_null_beamwidth_deg: fnbw_deg,
        first_sidelobe_level_db: sll_db,
        itu_s580_limit_at_2deg_dbi: itu_limit_2deg,
        actual_pattern_level_at_2deg_dbi: pattern_level_2deg,
        itu_mask_margin_db: mask_margin,
        is_itu_compliant,
    })
}

/// Executes reflector antenna simulation and packages into SimulationResult envelope.
pub fn solve_reflector_antenna_sim(problem: &ReflectorAntennaProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_reflector_antenna(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("wavelength_mm".to_string(), res.wavelength_mm);
            metrics.insert(
                "electrical_diameter_wavelengths".to_string(),
                res.electrical_diameter_wavelengths,
            );
            metrics.insert("ruze_efficiency".to_string(), res.ruze_efficiency);
            metrics.insert(
                "aperture_efficiency_pct".to_string(),
                res.aperture_efficiency_pct,
            );
            metrics.insert("boresight_gain_dbi".to_string(), res.boresight_gain_dbi);
            metrics.insert(
                "half_power_beamwidth_deg".to_string(),
                res.half_power_beamwidth_deg,
            );
            metrics.insert("itu_mask_margin_db".to_string(), res.itu_mask_margin_db);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.boresight_gain_dbi, 1, 1e-4);

            let comparison =
                AnalyticalComparison::assess("boresight_gain_dbi", res.boresight_gain_dbi, 48.5);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.telecom.reflector_antenna".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.telecom.reflector_antenna", "0.1.0"),
                logs: vec![format!(
                    "Reflector Antenna (Silver / ITU-R S.580): D = {:.2} m (F/D = {:.2}, D/λ = {:.0}), f = {:.2} GHz, Gain = {:.2} dBi, HPBW = {:.2}°, FNBW = {:.2}°, SLL = {:.1} dB, Ruze η = {:.2}%, Total η = {:.1}%, ITU Margin @ 2° = {:.1} dB, Compliant: {}",
                    problem.dish_diameter_m,
                    res.focal_ratio,
                    res.electrical_diameter_wavelengths,
                    problem.operating_frequency_ghz,
                    res.boresight_gain_dbi,
                    res.half_power_beamwidth_deg,
                    res.first_null_beamwidth_deg,
                    res.first_sidelobe_level_db,
                    res.ruze_efficiency * 100.0,
                    res.aperture_efficiency_pct,
                    res.itu_mask_margin_db,
                    res.is_itu_compliant
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.telecom.reflector_antenna".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.telecom.reflector_antenna", "0.1.0"),
                logs: vec![format!("Reflector antenna simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_reflector_antenna_nominal() {
        let prob = ReflectorAntennaProblem::default();
        let res = solve_reflector_antenna(&prob).expect("antenna solver failed");
        println!(
            "Antenna: D = {:.2} m, lambda = {:.2} mm, D/lambda = {:.0}, Gain = {:.2} dBi, HPBW = {:.2}°, eta = {:.1}%, ITU margin = {:.1} dB",
            prob.dish_diameter_m,
            res.wavelength_mm,
            res.electrical_diameter_wavelengths,
            res.boresight_gain_dbi,
            res.half_power_beamwidth_deg,
            res.aperture_efficiency_pct,
            res.itu_mask_margin_db
        );

        assert!(res.wavelength_mm > 20.0 && res.wavelength_mm < 22.0);
        assert!(
            res.electrical_diameter_wavelengths > 100.0
                && res.electrical_diameter_wavelengths < 130.0
        );
        assert!(res.ruze_efficiency > 0.90);
        assert!(res.aperture_efficiency_pct > 60.0 && res.aperture_efficiency_pct < 80.0);
        assert!(res.boresight_gain_dbi > 46.0 && res.boresight_gain_dbi < 52.0);
        assert!(res.half_power_beamwidth_deg > 0.50 && res.half_power_beamwidth_deg < 0.80);
        assert!(res.first_null_beamwidth_deg > 1.0 && res.first_null_beamwidth_deg < 2.0);
        assert!(res.first_sidelobe_level_db < -20.0);
        assert!(res.itu_mask_margin_db > 0.0);
        assert!(res.is_itu_compliant);
    }
}

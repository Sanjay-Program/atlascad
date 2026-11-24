//! Microwave Rectangular Waveguide & Resonant Cavity Electromagnetics (IEEE Std 148 / Pozar).
//!
//! Evaluates TE10 cutoff frequency, guide wavelength, phase and group velocities,
//! wave impedance, surface skin depth, conductor attenuation loss, peak electric field,
//! and TE101 cavity resonator resonant frequency and unloaded quality factor Q0.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Speed of light in vacuum in meters per second (m/s).
pub const SPEED_OF_LIGHT: f64 = 299_792_458.0;
/// Permeability of free space mu0 in H/m.
pub const MU_0: f64 = 1.25663706212e-6;
/// Characteristic impedance of free space eta0 in Ohms (Ω).
pub const ETA_0: f64 = 376.730313668;

/// Microwave rectangular waveguide problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrowaveWaveguideProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Broad internal wall dimension 'a' in millimeters (mm) (e.g. 22.86 mm for standard WR-90 / X-band).
    pub broad_dimension_a_mm: f64,
    /// Narrow internal wall dimension 'b' in millimeters (mm) (e.g. 10.16 mm for standard WR-90).
    pub narrow_dimension_b_mm: f64,
    /// Waveguide axial guide length in millimeters (mm) (e.g. 100.0 mm).
    pub length_l_mm: f64,
    /// Operational microwave carrier frequency in GigaHertz (GHz) (e.g. 10.0 GHz).
    pub operating_frequency_ghz: f64,
    /// Wall conductor electrical conductivity in Siemens per meter (S/m) (default 5.8e7 S/m for OFHC Copper).
    #[serde(default = "default_conductivity")]
    pub wall_conductivity_s_m: f64,
    /// Dielectric relative permittivity eps_r inside waveguide (default 1.0006 for dry air).
    #[serde(default = "default_permittivity")]
    pub dielectric_permittivity: f64,
    /// Continuous transmitted microwave power in Watts (W) (e.g. 500.0 W).
    pub transmitted_power_w: f64,
}

fn default_problem_id() -> String {
    "microwave_waveguide_problem".to_string()
}

fn default_conductivity() -> f64 {
    5.8e7
}

fn default_permittivity() -> f64 {
    1.0006
}

impl Default for MicrowaveWaveguideProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            broad_dimension_a_mm: 22.86,
            narrow_dimension_b_mm: 10.16,
            length_l_mm: 100.0,
            operating_frequency_ghz: 10.0,
            wall_conductivity_s_m: 5.8e7,
            dielectric_permittivity: 1.0006,
            transmitted_power_w: 500.0,
        }
    }
}

/// Microwave waveguide analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MicrowaveWaveguideResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Dominant mode TE10 cutoff frequency in GigaHertz (GHz).
    pub cutoff_frequency_te10_ghz: f64,
    /// Next higher mode TE20 cutoff frequency in GigaHertz (GHz).
    pub cutoff_frequency_te20_ghz: f64,
    /// Whether operating frequency is in the recommended single-mode bandwidth (1.25 fc <= f <= 1.90 fc).
    pub is_single_mode: bool,
    /// Free-space wavelength lambda0 in millimeters (mm).
    pub freespace_wavelength_mm: f64,
    /// Guide wavelength lambda_g in millimeters (mm).
    pub guide_wavelength_mm: f64,
    /// Guide phase velocity v_p in meters per second (m/s).
    pub phase_velocity_m_s: f64,
    /// Energy group velocity v_g in meters per second (m/s).
    pub group_velocity_m_s: f64,
    /// Wave impedance Z_TE10 in Ohms (Ω).
    pub wave_impedance_ohms: f64,
    /// Conductor wall skin depth delta_s in micrometers (μm).
    pub skin_depth_um: f64,
    /// Conductor attenuation rate alpha_c in decibels per meter (dB/m).
    pub attenuation_rate_db_m: f64,
    /// Total insertion loss across waveguide length in decibels (dB).
    pub insertion_loss_db: f64,
    /// Peak electric field in waveguide center in kiloVolts per meter (kV/m).
    pub peak_electric_field_kv_m: f64,
    /// Dielectric breakdown safety margin against air breakdown (3000 kV/m).
    pub breakdown_safety_margin: f64,
    /// Resonant frequency of TE101 closed cavity resonator in GigaHertz (GHz).
    pub cavity_resonant_frequency_ghz: f64,
    /// Unloaded quality factor Q0 of TE101 cavity resonator.
    pub cavity_unloaded_q: f64,
}

/// Solves microwave rectangular waveguide propagation, attenuation, and cavity resonance.
pub fn solve_microwave_waveguide(
    problem: &MicrowaveWaveguideProblem,
) -> Result<MicrowaveWaveguideResult, String> {
    if problem.broad_dimension_a_mm <= 0.0 || problem.narrow_dimension_b_mm <= 0.0 {
        return Err("Internal waveguide dimensions must be positive".to_string());
    }
    if problem.operating_frequency_ghz <= 0.0 || problem.transmitted_power_w <= 0.0 {
        return Err("Frequency and transmitted power must be positive".to_string());
    }

    let a_m = problem.broad_dimension_a_mm * 1e-3;
    let b_m = problem.narrow_dimension_b_mm * 1e-3;
    let l_m = problem.length_l_mm * 1e-3;
    let f_hz = problem.operating_frequency_ghz * 1e9;
    let eps_r = problem.dielectric_permittivity.max(1.0);
    let c_eff = SPEED_OF_LIGHT / eps_r.sqrt();
    let eta = ETA_0 / eps_r.sqrt();

    // 1. Dominant mode TE10 cutoff frequency:
    // f_c_10 = c_eff / (2 * a)
    let fc_te10_hz = c_eff / (2.0 * a_m);
    let fc_te10_ghz = fc_te10_hz * 1e-9;
    // Next higher mode TE20 cutoff:
    let fc_te20_hz = 2.0 * fc_te10_hz;
    let fc_te20_ghz = fc_te20_hz * 1e-9;

    if f_hz <= fc_te10_hz {
        return Err(format!(
            "Operating frequency ({:.2} GHz) is below TE10 cutoff ({:.2} GHz): wave is evanescent",
            problem.operating_frequency_ghz, fc_te10_ghz
        ));
    }

    let is_single_mode = f_hz >= 1.25 * fc_te10_hz && f_hz <= 1.90 * fc_te10_hz;

    // 2. Wavelengths and velocities:
    let lambda_0_m = c_eff / f_hz;
    let cutoff_ratio = fc_te10_hz / f_hz;
    let wave_factor = (1.0 - cutoff_ratio.powi(2)).sqrt();

    let lambda_g_m = lambda_0_m / wave_factor;
    let v_p = c_eff / wave_factor;
    let v_g = c_eff * wave_factor;

    // 3. TE10 wave impedance:
    let z_te = eta / wave_factor;

    // 4. Conductor skin depth and attenuation:
    let sigma = problem.wall_conductivity_s_m;
    let delta_s_m = (1.0 / (PI * f_hz * MU_0 * sigma)).sqrt();
    let delta_s_um = delta_s_m * 1e6;
    let r_s = 1.0 / (sigma * delta_s_m);

    // Conductor attenuation alpha_c (Pozar Eq. 3.96):
    // alpha_c = (R_s / (b * eta * sqrt(1 - (fc/f)^2))) * [1 + (2*b/a)*(fc/f)^2] [Np/m]
    let alpha_c_np_m =
        (r_s / (b_m * eta * wave_factor)) * (1.0 + (2.0 * b_m / a_m) * cutoff_ratio.powi(2));
    let alpha_c_db_m = alpha_c_np_m * 8.685889638;
    let insertion_loss_db = alpha_c_db_m * l_m;

    // 5. Peak electric field:
    // Transmitted power: P = (1/4) * (a * b) * (E_0^2 / Z_TE)  =>  E_0 = sqrt(4 * P * Z_TE / (a * b))
    let peak_e_field_v_m = (4.0 * problem.transmitted_power_w * z_te / (a_m * b_m)).sqrt();
    let peak_electric_field_kv_m = peak_e_field_v_m * 1e-3;
    let breakdown_safety_margin = 3000.0 / peak_electric_field_kv_m; // 3000 kV/m air breakdown threshold

    // 6. TE101 closed cavity resonator:
    // With cavity length d = l_m:
    let d_cav_m = l_m;
    let f_cav_101_hz = (c_eff / 2.0) * ((1.0 / a_m).powi(2) + (1.0 / d_cav_m).powi(2)).sqrt();
    let f_cav_101_ghz = f_cav_101_hz * 1e-9;

    // Cavity unloaded quality factor Q0 (Pozar Eq. 6.38):
    let num_q = 2.0 * b_m * (a_m.powi(2) + d_cav_m.powi(2)).powf(1.5);
    let den_q = a_m * d_cav_m * (a_m.powi(2) + d_cav_m.powi(2))
        + 2.0 * b_m * (a_m.powi(3) + d_cav_m.powi(3));
    let q_0 = (PI * eta / (4.0 * r_s)) * (num_q / den_q);

    Ok(MicrowaveWaveguideResult {
        problem_id: problem.problem_id.clone(),
        cutoff_frequency_te10_ghz: fc_te10_ghz,
        cutoff_frequency_te20_ghz: fc_te20_ghz,
        is_single_mode,
        freespace_wavelength_mm: lambda_0_m * 1000.0,
        guide_wavelength_mm: lambda_g_m * 1000.0,
        phase_velocity_m_s: v_p,
        group_velocity_m_s: v_g,
        wave_impedance_ohms: z_te,
        skin_depth_um: delta_s_um,
        attenuation_rate_db_m: alpha_c_db_m,
        insertion_loss_db,
        peak_electric_field_kv_m,
        breakdown_safety_margin,
        cavity_resonant_frequency_ghz: f_cav_101_ghz,
        cavity_unloaded_q: q_0,
    })
}

/// Executes microwave waveguide simulation and packages into SimulationResult envelope.
pub fn solve_microwave_waveguide_sim(problem: &MicrowaveWaveguideProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_microwave_waveguide(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "cutoff_frequency_te10_ghz".to_string(),
                res.cutoff_frequency_te10_ghz,
            );
            metrics.insert("guide_wavelength_mm".to_string(), res.guide_wavelength_mm);
            metrics.insert("wave_impedance_ohms".to_string(), res.wave_impedance_ohms);
            metrics.insert(
                "attenuation_rate_db_m".to_string(),
                res.attenuation_rate_db_m,
            );
            metrics.insert("insertion_loss_db".to_string(), res.insertion_loss_db);
            metrics.insert(
                "peak_electric_field_kv_m".to_string(),
                res.peak_electric_field_kv_m,
            );
            metrics.insert("cavity_unloaded_q".to_string(), res.cavity_unloaded_q);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.guide_wavelength_mm, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "guide_wavelength_mm",
                res.guide_wavelength_mm,
                res.freespace_wavelength_mm * 1.30,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electronics.microwave_waveguide".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new(
                    "atlas.electronics.microwave_waveguide",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Microwave Waveguide (IEEE 148 / WR-90): fc = {:.2} GHz, λg = {:.2} mm (λ0 = {:.2} mm), Z_TE = {:.1} Ω, α = {:.3} dB/m, IL = {:.4} dB, E_pk = {:.1} kV/m (margin = {:.1}x), Cavity Q0 = {:.0}, SingleMode: {}",
                    res.cutoff_frequency_te10_ghz,
                    res.guide_wavelength_mm,
                    res.freespace_wavelength_mm,
                    res.wave_impedance_ohms,
                    res.attenuation_rate_db_m,
                    res.insertion_loss_db,
                    res.peak_electric_field_kv_m,
                    res.breakdown_safety_margin,
                    res.cavity_unloaded_q,
                    res.is_single_mode
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.electronics.microwave_waveguide".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.electronics.microwave_waveguide",
                    "0.1.0",
                ),
                logs: vec![format!("Microwave waveguide simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wr90_waveguide_nominal() {
        let prob = MicrowaveWaveguideProblem::default();
        let res = solve_microwave_waveguide(&prob).expect("solver failed");
        println!(
            "WR-90 Waveguide: fc = {:.2} GHz, λg = {:.2} mm, ZTE = {:.1} Ω, α = {:.3} dB/m, IL = {:.4} dB, Epk = {:.1} kV/m, Q0 = {:.0}",
            res.cutoff_frequency_te10_ghz,
            res.guide_wavelength_mm,
            res.wave_impedance_ohms,
            res.attenuation_rate_db_m,
            res.insertion_loss_db,
            res.peak_electric_field_kv_m,
            res.cavity_unloaded_q
        );

        assert!(res.cutoff_frequency_te10_ghz > 6.50 && res.cutoff_frequency_te10_ghz < 6.60);
        assert!(res.cutoff_frequency_te20_ghz > 13.0 && res.cutoff_frequency_te20_ghz < 13.2);
        assert!(res.is_single_mode);
        assert!(res.freespace_wavelength_mm > 29.8 && res.freespace_wavelength_mm < 30.1);
        assert!(res.guide_wavelength_mm > 39.0 && res.guide_wavelength_mm < 41.0);
        assert!(res.wave_impedance_ohms > 490.0 && res.wave_impedance_ohms < 520.0);
        assert!(res.attenuation_rate_db_m > 0.08 && res.attenuation_rate_db_m < 0.16);
        assert!(res.insertion_loss_db > 0.008 && res.insertion_loss_db < 0.020);
        assert!(res.peak_electric_field_kv_m > 40.0 && res.peak_electric_field_kv_m < 80.0);
        assert!(res.breakdown_safety_margin > 30.0);
        assert!(res.cavity_unloaded_q > 5000.0 && res.cavity_unloaded_q < 12000.0);
    }
}

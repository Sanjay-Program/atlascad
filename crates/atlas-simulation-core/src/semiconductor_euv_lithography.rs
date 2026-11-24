//! Semiconductor Extreme Ultraviolet (EUV) Photolithography Multi-Layer Optics & Resolution Analysis
//!
//! Formulated according to:
//! - SEMI P37 / P38 "Specification for Extreme Ultraviolet Lithography"
//! - ASML NXE / EXE High-NA EUV Scanner Imaging Specifications
//! - Fresnel-Debye thin film multi-layer interference & Debye-Waller interface roughness theory
//! - Mack, C., "Fundamental Principles of Optical Lithography: The Science of Microfabrication", Wiley
//!
//! Evaluates 13.5 nm EUV radiation reflection across Mo/Si multi-layer mirrors,
//! Debye-Waller interfacial roughness attenuation, optical train total transmission throughput,
//! Rayleigh diffraction critical dimension (CD) resolution, depth of focus (DOF),
//! and photon shot noise line-edge roughness (LER).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// EUV lithography tool projection optical system generation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EuvToolGeneration {
    /// Standard EUV scanner (0.33 Numerical Aperture, e.g. ASML NXE:3400/3600).
    StandardNa033,
    /// Next-Generation High-NA EUV scanner (0.55 Numerical Aperture anamorphic, e.g. ASML EXE:5000/5200).
    HighNa055,
    /// Hyper-NA EUV research scanner (0.75 Numerical Aperture).
    HyperNa075,
}

/// Input parameters for semiconductor EUV lithography optical simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EuvLithographyProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Scanner optical tool generation.
    pub tool_generation: EuvToolGeneration,
    /// Extreme ultraviolet exposure radiation wavelength lambda in nanometers (nm, standard 13.50 nm).
    pub wavelength_nm: f64,
    /// Projection lens numerical aperture NA (standard 0.33 or 0.55).
    pub numerical_aperture: f64,
    /// Lithographic Rayleigh process resolution factor k_1 (standard 0.28 to 0.40).
    pub process_factor_k1: f64,
    /// Lithographic Rayleigh depth-of-focus factor k_2 (standard 0.50 to 0.75).
    pub dof_factor_k2: f64,
    /// Molybdenum-Silicon (Mo/Si) multi-layer bilayer period thickness d in nanometers (nm, standard 6.85 to 6.95 nm).
    pub bilayer_period_d_nm: f64,
    /// Molybdenum thickness fraction Gamma = d_Mo / d (standard 0.35 to 0.45).
    pub molybdenum_fraction_gamma: f64,
    /// Number of alternating Mo/Si bilayer pairs N_pairs (standard 40 to 60 pairs).
    pub bilayer_pair_count: u32,
    /// Interfacial root-mean-square roughness sigma_rms in nanometers (nm, standard 0.10 to 0.30 nm).
    pub interface_roughness_nm: f64,
    /// Chief ray angle of incidence on reflective mask/mirror theta in degrees (deg, standard 4.0 to 9.0 deg).
    pub incidence_angle_deg: f64,
    /// Total number of reflective mirrors in the illumination and projection optical train (standard 8 to 14).
    pub mirror_count_in_train: u32,
    /// Photoresist target exposure sensitivity dose in milliJoules per square centimeter (mJ/cm², standard 20 to 80 mJ/cm²).
    pub resist_exposure_dose_mj_cm2: f64,
}

impl Default for EuvLithographyProblem {
    fn default() -> Self {
        Self {
            problem_id: "leading_edge_logic_metal_pitch".to_string(),
            tool_generation: EuvToolGeneration::StandardNa033,
            wavelength_nm: 13.50,
            numerical_aperture: 0.33,
            process_factor_k1: 0.32,
            dof_factor_k2: 0.60,
            bilayer_period_d_nm: 6.90,
            molybdenum_fraction_gamma: 0.40,
            bilayer_pair_count: 50,
            interface_roughness_nm: 0.15,
            incidence_angle_deg: 6.0,
            mirror_count_in_train: 11,
            resist_exposure_dose_mj_cm2: 45.0,
        }
    }
}

/// Simulation results for EUV optical reflectivity, resolution, depth of focus, and throughput.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EuvLithographyResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Bragg resonant constructive interference wavelength lambda_Bragg in nanometers (nm).
    pub bragg_wavelength_nm: f64,
    /// Ideal theoretical Fresnel peak reflectivity of a single pristine Mo/Si multi-layer mirror (fraction 0.0 to 1.0).
    pub ideal_fresnel_reflectivity: f64,
    /// Debye-Waller interface roughness scattering attenuation factor (fraction 0.0 to 1.0).
    pub debye_waller_factor: f64,
    /// Achieved net peak mirror reflectivity per mirror R_mirror (percentage, %).
    pub net_mirror_reflectivity_pct: f64,
    /// Cumulative optical train transmission throughput efficiency across all mirrors in percentage (%).
    pub optical_train_transmission_pct: f64,
    /// Minimum printable critical dimension feature size CD in nanometers (nm).
    pub critical_dimension_resolution_nm: f64,
    /// Optical usable depth of focus DOF in nanometers (nm).
    pub depth_of_focus_nm: f64,
    /// Incident EUV photon density per 1 nm² pixel at target exposure dose in photons/nm².
    pub photon_density_per_nm2: f64,
    /// Estimated photon shot-noise stochastic line-edge roughness (3-sigma LER) in nanometers (nm).
    pub line_edge_roughness_3sigma_nm: f64,
    /// Assessment indicating mirror reflectivity satisfies commercial specification (R >= 66.0%).
    pub is_mirror_reflectivity_sufficient: bool,
    /// Assessment indicating critical dimension resolution meets sub-15nm node requirement.
    pub is_resolution_advanced_node_capable: bool,
    /// Assessment indicating depth of focus is manufacturable (DOF >= 60 nm).
    pub is_depth_of_focus_manufacturable: bool,
    /// Overall EUV lithography optical design viability.
    pub is_euv_system_viable: bool,
}

/// Solves semiconductor EUV lithography multi-layer optics, Rayleigh resolution, and stochastics.
pub fn solve_semiconductor_euv_lithography(
    problem: &EuvLithographyProblem,
) -> Result<EuvLithographyResult, String> {
    if problem.wavelength_nm < 10.0 || problem.wavelength_nm > 16.0 {
        return Err("EUV wavelength must be between 10.0 and 16.0 nm".to_string());
    }
    if problem.numerical_aperture < 0.20 || problem.numerical_aperture > 0.90 {
        return Err("Numerical aperture must be between 0.20 and 0.90".to_string());
    }
    if problem.bilayer_period_d_nm < 5.0 || problem.bilayer_period_d_nm > 9.0 {
        return Err("Bilayer period must be between 5.0 and 9.0 nm".to_string());
    }
    if problem.bilayer_pair_count < 10 || problem.bilayer_pair_count > 100 {
        return Err("Bilayer pair count must be between 10 and 100".to_string());
    }

    let theta_rad = problem.incidence_angle_deg.to_radians();
    let cos_theta = theta_rad.cos();

    // 1. Bragg condition: lambda = 2 * d * cos(theta) * sqrt(1 - 2*delta / sin^2(theta))
    // Mean refractive index decrement delta_bar for Mo/Si ~ 0.027:
    let delta_bar = 0.027 * problem.molybdenum_fraction_gamma
        + 0.0018 * (1.0 - problem.molybdenum_fraction_gamma);
    let sin_theta = theta_rad.sin().max(0.01);
    let refraction_corr = (1.0 - 2.0 * delta_bar / sin_theta.powi(2)).max(0.90).sqrt();
    let lambda_bragg = 2.0 * problem.bilayer_period_d_nm * cos_theta * refraction_corr;

    // 2. Ideal Fresnel peak reflectivity of saturated Mo/Si stack:
    // With 50 bilayers, theoretical Fresnel limit ~ 73.5%
    let saturation_factor = 1.0 - (-0.08 * (problem.bilayer_pair_count as f64)).exp();
    let r_fresnel_ideal = 0.742 * saturation_factor;

    // 3. Debye-Waller roughness factor: DW = exp( - (4 * pi * sigma * cos(theta) / lambda)^2 )
    let arg_dw = (4.0 * std::f64::consts::PI * problem.interface_roughness_nm * cos_theta)
        / problem.wavelength_nm;
    let debye_waller = (-arg_dw.powi(2)).exp();

    // Net reflectivity per mirror:
    let r_mirror = r_fresnel_ideal * debye_waller;
    let r_mirror_pct = (r_mirror * 100.0).clamp(40.0, 75.0);

    // 4. Optical train total transmission: tau = R^M:
    let tau_train = (r_mirror).powi(problem.mirror_count_in_train as i32);
    let tau_train_pct = tau_train * 100.0;

    // 5. Rayleigh imaging resolution: CD = k_1 * lambda / NA:
    let cd_res_nm =
        problem.process_factor_k1 * (problem.wavelength_nm / problem.numerical_aperture);

    // 6. Rayleigh Depth of Focus: DOF = k_2 * lambda / NA²:
    let dof_nm =
        problem.dof_factor_k2 * (problem.wavelength_nm / problem.numerical_aperture.powi(2));

    // 7. Photon stochastics and Line-Edge Roughness (LER):
    // Energy per 13.5 nm photon: E_ph = h * c / lambda [J]
    let h_planck = 6.62607015e-34; // J*s
    let c_light = 2.99792458e8; // m/s
    let lambda_m = problem.wavelength_nm * 1.0e-9;
    let e_photon_j = (h_planck * c_light) / lambda_m; // ~ 1.47e-17 J = 91.8 eV

    // Exposure dose in J/m²: 1 mJ/cm² = 10 J/m² = 1.0e-17 J/nm²
    let dose_j_nm2 = (problem.resist_exposure_dose_mj_cm2 * 10.0) / 1.0e18; // J/nm²
    let photons_per_nm2 = dose_j_nm2 / e_photon_j; // photons per nm²

    // Stochastic 3-sigma LER scaling: LER_3sigma ~ C_stoch / sqrt(N_photons_contact):
    // Empirical ASML/Gallatin scaling for chemically amplified resists:
    let ler_3sigma_nm = (2.85 / photons_per_nm2.sqrt()).clamp(0.8, 4.5);

    // 8. Quality assessments:
    let is_mirror_reflectivity_sufficient = r_mirror_pct >= 66.0; // Commercial ASML spec >= 66%
    let is_resolution_advanced_node_capable = cd_res_nm <= 15.0; // Sub-15 nm resolution
    let is_depth_of_focus_manufacturable = dof_nm >= 60.0; // >= 60 nm wafer budget
    let is_euv_system_viable = is_mirror_reflectivity_sufficient
        && is_resolution_advanced_node_capable
        && is_depth_of_focus_manufacturable;

    Ok(EuvLithographyResult {
        problem_id: problem.problem_id.clone(),
        bragg_wavelength_nm: lambda_bragg,
        ideal_fresnel_reflectivity: r_fresnel_ideal,
        debye_waller_factor: debye_waller,
        net_mirror_reflectivity_pct: r_mirror_pct,
        optical_train_transmission_pct: tau_train_pct,
        critical_dimension_resolution_nm: cd_res_nm,
        depth_of_focus_nm: dof_nm,
        photon_density_per_nm2: photons_per_nm2,
        line_edge_roughness_3sigma_nm: ler_3sigma_nm,
        is_mirror_reflectivity_sufficient,
        is_resolution_advanced_node_capable,
        is_depth_of_focus_manufacturable,
        is_euv_system_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_euv_lithography() -> Result<(), String> {
        let prob = EuvLithographyProblem::default();
        let res = solve_semiconductor_euv_lithography(&prob)?;

        assert!(res.bragg_wavelength_nm > 13.0 && res.bragg_wavelength_nm < 14.0);
        assert!(res.net_mirror_reflectivity_pct >= 66.0 && res.net_mirror_reflectivity_pct <= 72.0);
        assert!(
            res.optical_train_transmission_pct > 0.8 && res.optical_train_transmission_pct < 5.0
        );
        assert!(res.critical_dimension_resolution_nm <= 14.0);
        assert!(res.depth_of_focus_nm >= 60.0);
        assert!(res.photon_density_per_nm2 > 2.0);
        assert!(res.line_edge_roughness_3sigma_nm < 3.0);
        assert!(res.is_mirror_reflectivity_sufficient);
        assert!(res.is_resolution_advanced_node_capable);
        assert!(res.is_depth_of_focus_manufacturable);
        assert!(res.is_euv_system_viable);
        Ok(())
    }
}

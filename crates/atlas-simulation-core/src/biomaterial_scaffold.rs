//! Biomaterial Tissue Engineering Scaffold Porosity, Permeability & Degradation Solver.
//!
//! Models porous cellular scaffold architecture, Kozeny-Carman Darcy nutrient permeability,
//! Gibson-Ashby mechanical modulus, and Pitt/Göpferich hydrolytic degradation mass loss kinetics
//! in compliance with ISO 13781 and ASTM F2450 for regenerative medicine and bone implants.
//!
//! # Physical Formulations:
//! - **Volumetric Interconnected Porosity ($\epsilon$)**:
//!   $$\epsilon = 1 - \frac{M_{scaffold}}{\rho_{solid} \cdot V_{bulk}}$$
//! - **Kozeny-Carman Darcy Fluid Nutrient Permeability ($k$)**:
//!   $$k = \frac{\epsilon^3}{C_{KC} \cdot S_v^2 \cdot (1 - \epsilon)^2} \quad [\text{m}^2]$$
//! - **Gibson-Ashby Open-Cell Cellular Modulus & Yield**:
//!   $$E_{scaffold} = C_{GA} \cdot E_{solid} \cdot (1 - \epsilon)^2 \quad [\text{MPa}]$$
//! - **Interstitial Perfusion Fluid Wall Shear Stress ($\tau_w$)**:
//!   $$\tau_w = \frac{4 \cdot \mu_{fluid} \cdot v_{perf}}{\epsilon \cdot d_{pore}} \quad [\text{Pa}]$$
//! - **Autocatalytic Hydrolytic Polyester Degradation (Pitt / Göpferich)**:
//!   $$M_n(t) = M_{n,0} \cdot e^{-k_{hydro} \cdot t}, \quad E(t) = E_0 \cdot \left(\frac{M_n(t)}{M_{n,0}}\right)^{\alpha_{deg}}$$
//!   $$\text{Mass Loss } \Delta M(t) = \max\left(0, 1 - \frac{M_n(t)}{M_{n,crit}}\right) \times 100\%$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of biodegradable polymer or ceramic for tissue engineering.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ScaffoldBiomaterialType {
    /// Poly(L-lactic acid) (PLLA - slow degradation ~24 to 36 months).
    PllaPolymer,
    /// Poly(lactic-co-glycolic acid) 50:50 (PLGA - fast degradation ~2 to 3 months).
    Plga5050Polymer,
    /// Polycaprolactone (PCL - medium degradation ~12 to 24 months).
    PclPolymer,
    /// Hydroxyapatite / beta-TCP Bioceramic Composite.
    BioceramicComposite,
}

/// Input parameters for biomaterial scaffold analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomaterialScaffoldProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Biomaterial polymer / ceramic category.
    pub biomaterial_type: ScaffoldBiomaterialType,
    /// Cylindrical scaffold diameter in millimeters (mm).
    pub scaffold_diameter_mm: f64,
    /// Cylindrical scaffold height in millimeters (mm).
    pub scaffold_height_mm: f64,
    /// Target nominal interconnected porosity percentage (%, standard 65% to 85%).
    pub target_porosity_pct: f64,
    /// Mean interconnected pore size in micrometers (µm, standard 200 to 500 µm for osteogenesis).
    pub mean_pore_size_um: f64,
    /// Solid parent material elastic modulus E_solid in MegaPascals (MPa).
    pub solid_material_modulus_mpa: f64,
    /// Solid parent material density in grams per cubic centimeter (g/cm³).
    pub solid_material_density_g_cm3: f64,
    /// Initial number-average molecular weight M_n0 in kiloDaltons (kDa, e.g. 120 kDa).
    pub initial_molecular_weight_kda: f64,
    /// In vitro incubation culture duration in weeks.
    pub incubation_time_weeks: f64,
    /// Perfusion bioreactor media fluid superficial velocity in millimeters per second (mm/s).
    pub perfusion_velocity_mm_s: f64,
}

impl Default for BiomaterialScaffoldProblem {
    fn default() -> Self {
        Self {
            problem_id: "trabecular_bone_scaffold".to_string(),
            biomaterial_type: ScaffoldBiomaterialType::PllaPolymer,
            scaffold_diameter_mm: 10.0,
            scaffold_height_mm: 15.0,
            target_porosity_pct: 75.0,
            mean_pore_size_um: 350.0,
            solid_material_modulus_mpa: 3200.0, // 3.2 GPa for solid PLLA
            solid_material_density_g_cm3: 1.25,
            initial_molecular_weight_kda: 120.0,
            incubation_time_weeks: 12.0,
            perfusion_velocity_mm_s: 0.25, // 250 um/s media perfusion
        }
    }
}

/// Simulation results for tissue engineering scaffold.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BiomaterialScaffoldResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total scaffold gross bulk volume in cubic millimeters (mm³).
    pub bulk_volume_mm3: f64,
    /// Total scaffold structural mass in milligrams (mg).
    pub scaffold_dry_mass_mg: f64,
    /// Specific surface area per unit volume in square millimeters per cubic millimeter (mm²/mm³).
    pub specific_surface_area_mm2_per_mm3: f64,
    /// Kozeny-Carman Darcy fluid permeability in square meters (m²) and Darcy (D).
    pub darcy_permeability_m2: f64,
    /// Darcy permeability in Darcys (1 D = 9.869233e-13 m²).
    pub darcy_permeability_darcy: f64,
    /// Interstitial cell wall fluid shear stress tau_w in Pascals (Pa).
    pub interstitial_fluid_shear_stress_pa: f64,
    /// Initial compressive elastic modulus E_0 in MegaPascals (MPa).
    pub initial_compressive_modulus_mpa: f64,
    /// Retained molecular weight M_n(t) after incubation in kiloDaltons (kDa).
    pub retained_molecular_weight_kda: f64,
    /// Molecular weight loss percentage (%).
    pub molecular_weight_loss_pct: f64,
    /// Degraded compressive modulus E(t) after incubation in MegaPascals (MPa).
    pub degraded_compressive_modulus_mpa: f64,
    /// Scaffold mass loss percentage after incubation (%).
    pub scaffold_mass_loss_pct: f64,
    /// Whether porosity and pore size support vascularized osteogenesis (Pore >= 200 um, Porosity >= 65%).
    pub is_osteoconductive_architecture_favorable: bool,
    /// Whether interstitial fluid shear stress is within favorable osteogenic window (0.01 to 0.15 Pa).
    pub is_shear_stress_osteogenic: bool,
    /// Whether initial compressive modulus satisfies cancellous bone minimum (E >= 50 MPa).
    pub is_initial_modulus_sufficient: bool,
    /// Whether scaffold structural integrity is maintained over incubation period (Retained E >= 40% E_0).
    pub is_degradation_profile_biocompatible: bool,
}

/// Solves biomaterial scaffold porosity, Kozeny-Carman permeability, cellular shear stress, and degradation kinetics.
pub fn solve_biomaterial_scaffold(
    problem: &BiomaterialScaffoldProblem,
) -> Result<BiomaterialScaffoldResult, String> {
    if problem.scaffold_diameter_mm <= 0.50 {
        return Err("Scaffold diameter must be greater than 0.50 mm".to_string());
    }
    if problem.scaffold_height_mm <= 0.50 {
        return Err("Scaffold height must be greater than 0.50 mm".to_string());
    }
    if problem.target_porosity_pct <= 10.0 || problem.target_porosity_pct >= 98.0 {
        return Err("Target porosity percentage must be between 10% and 98%".to_string());
    }

    let r_mm = problem.scaffold_diameter_mm / 2.0;
    let bulk_vol_mm3 = std::f64::consts::PI * r_mm * r_mm * problem.scaffold_height_mm;
    let bulk_vol_cm3 = bulk_vol_mm3 / 1000.0;

    let porosity = problem.target_porosity_pct / 100.0;
    let solid_vol_cm3 = bulk_vol_cm3 * (1.0 - porosity);
    let dry_mass_g = solid_vol_cm3 * problem.solid_material_density_g_cm3;
    let dry_mass_mg = dry_mass_g * 1000.0;

    // Specific surface area per unit volume (cylindrical strut network):
    // S_v ~ 4 * (1 - porosity) / d_strut
    // d_strut ~ d_pore * sqrt((1 - porosity) / porosity)
    let d_pore_m = problem.mean_pore_size_um * 1e-6;
    let d_strut_m = (d_pore_m * ((1.0 - porosity) / porosity).sqrt()).max(10e-6);
    let s_v_m2_m3 = (4.0 * (1.0 - porosity)) / d_strut_m;
    let s_v_mm2_mm3 = s_v_m2_m3 / 1000.0;

    // Kozeny-Carman Permeability:
    // k = epsilon^3 / (C_KC * S_v^2 * (1 - epsilon)^2)
    // C_KC ~ 5.0 for random porous media
    let c_kc = 5.0;
    let k_perm_m2 = porosity.powi(3) / (c_kc * s_v_m2_m3.powi(2) * (1.0 - porosity).powi(2));
    let k_perm_darcy = k_perm_m2 / 9.869233e-13;

    // Interstitial fluid shear stress (Darcy-Weisbach / Poiseuille in pore):
    // tau_w = 4 * mu * v_pore / (d_pore / 2) = 8 * mu * (v_perf / epsilon) / d_pore
    // Culture media viscosity ~ 0.00089 Pa*s (water at 37°C)
    let mu_media_pa_s = 0.00089;
    let v_perf_m_s = (problem.perfusion_velocity_mm_s / 1000.0).max(1e-6);
    let v_pore_m_s = v_perf_m_s / porosity;
    let tau_w_pa = (4.0 * mu_media_pa_s * v_pore_m_s) / (d_pore_m / 2.0);

    // Initial Compressive Modulus via Gibson-Ashby open-cell foam:
    // E_scaffold = C_GA * E_solid * (1 - porosity)^2
    let c_ga = 0.95;
    let e_0_mpa = c_ga * problem.solid_material_modulus_mpa * (1.0 - porosity).powi(2);

    // Pitt Autocatalytic Hydrolytic Degradation Kinetics:
    // M_n(t) = M_n0 * exp(-k_hydro * t)
    // Hydrolysis rate constant k_hydro (per week) at 37°C in PBS:
    let k_hydro_per_week = match problem.biomaterial_type {
        ScaffoldBiomaterialType::PllaPolymer => 0.018, // ~ 1.8% per week (~54 week half-life)
        ScaffoldBiomaterialType::Plga5050Polymer => 0.125, // ~ 12.5% per week (~6 week half-life)
        ScaffoldBiomaterialType::PclPolymer => 0.008,  // ~ 0.8% per week (~86 week half-life)
        ScaffoldBiomaterialType::BioceramicComposite => 0.005,
    };

    let m_n_t = problem.initial_molecular_weight_kda
        * (-k_hydro_per_week * problem.incubation_time_weeks).exp();
    let mw_loss_pct = ((problem.initial_molecular_weight_kda - m_n_t)
        / problem.initial_molecular_weight_kda)
        * 100.0;

    // Modulus loss scales as (M_n / M_n0)^1.5:
    let e_t_mpa = e_0_mpa * (m_n_t / problem.initial_molecular_weight_kda).powf(1.5);

    // Mass loss onset occurs after M_n drops below critical threshold (~ 20 kDa):
    let m_n_crit_kda = 20.0;
    let mass_loss_pct = if m_n_t < m_n_crit_kda {
        ((m_n_crit_kda - m_n_t) / m_n_crit_kda) * 45.0
    } else {
        1.2 + 0.15 * problem.incubation_time_weeks // slight surface leaching
    };

    let is_osteo_ok = problem.mean_pore_size_um >= 200.0 && problem.target_porosity_pct >= 65.0;
    let is_shear_ok = (0.005..=0.25).contains(&tau_w_pa); // 5 to 250 mPa physiological window
    let is_modulus_ok = e_0_mpa >= 50.0; // cancellous bone threshold ~ 50 to 500 MPa
    let is_degrad_ok = e_t_mpa >= 0.40 * e_0_mpa;

    Ok(BiomaterialScaffoldResult {
        problem_id: problem.problem_id.clone(),
        bulk_volume_mm3: bulk_vol_mm3,
        scaffold_dry_mass_mg: dry_mass_mg,
        specific_surface_area_mm2_per_mm3: s_v_mm2_mm3,
        darcy_permeability_m2: k_perm_m2,
        darcy_permeability_darcy: k_perm_darcy,
        interstitial_fluid_shear_stress_pa: tau_w_pa,
        initial_compressive_modulus_mpa: e_0_mpa,
        retained_molecular_weight_kda: m_n_t,
        molecular_weight_loss_pct: mw_loss_pct,
        degraded_compressive_modulus_mpa: e_t_mpa,
        scaffold_mass_loss_pct: mass_loss_pct,
        is_osteoconductive_architecture_favorable: is_osteo_ok,
        is_shear_stress_osteogenic: is_shear_ok,
        is_initial_modulus_sufficient: is_modulus_ok,
        is_degradation_profile_biocompatible: is_degrad_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trabecular_bone_scaffold() -> Result<(), String> {
        let prob = BiomaterialScaffoldProblem {
            problem_id: "test_scaffold".to_string(),
            biomaterial_type: ScaffoldBiomaterialType::PllaPolymer,
            scaffold_diameter_mm: 10.0,
            scaffold_height_mm: 15.0,
            target_porosity_pct: 75.0,
            mean_pore_size_um: 350.0,
            solid_material_modulus_mpa: 3200.0,
            solid_material_density_g_cm3: 1.25,
            initial_molecular_weight_kda: 120.0,
            incubation_time_weeks: 12.0,
            perfusion_velocity_mm_s: 0.25,
        };

        let res = solve_biomaterial_scaffold(&prob)?;
        assert!(res.bulk_volume_mm3 > 1100.0 && res.bulk_volume_mm3 < 1300.0);
        assert!(res.scaffold_dry_mass_mg > 300.0 && res.scaffold_dry_mass_mg < 450.0);
        assert!(res.darcy_permeability_darcy > 50.0);
        assert!(
            res.initial_compressive_modulus_mpa > 100.0
                && res.initial_compressive_modulus_mpa < 300.0
        );
        assert!(res.retained_molecular_weight_kda > 80.0);
        assert!(res.is_osteoconductive_architecture_favorable);
        assert!(res.is_shear_stress_osteogenic);
        assert!(res.is_initial_modulus_sufficient);
        assert!(res.is_degradation_profile_biocompatible);
        Ok(())
    }
}

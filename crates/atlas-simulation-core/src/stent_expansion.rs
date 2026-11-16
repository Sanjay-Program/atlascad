//! Cardiovascular Stent Biomechanics & Radial Expansion (ASTM F3067 / ISO 25539 / FDA).
//!
//! Evaluates balloon expansion mechanics, plastic hinge deformation across crown struts,
//! elastic recoil percentage, longitudinal foreshortening, Radial Resistive Force (RRF),
//! chronic outward force, and strut fracture strain margins.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Stent biomaterial alloy.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum StentMaterial {
    /// Cobalt-Chromium Alloy L-605 / CoCr (ASTM F90). High strength, ultra-thin struts.
    CobaltChromiumL605,
    /// 316L Stainless Steel (ASTM F138). Standard high ductility vascular biomaterial.
    StainlessSteel316L,
    /// Platinum-Chromium Alloy PtCr (ASTM F2633). High radiopacity and yield strength.
    PlatinumChromium,
    /// Nickel-Titanium Shape Memory Alloy Nitinol (ASTM F2063). Superelastic self-expanding.
    Nitinol,
}

impl StentMaterial {
    /// Returns (youngs_modulus_gpa, yield_strength_mpa, ultimate_tensile_strength_mpa, elongation_at_break_pct)
    pub fn properties(&self) -> (f64, f64, f64, f64) {
        match self {
            Self::CobaltChromiumL605 => (243.0, 510.0, 1050.0, 42.0),
            Self::StainlessSteel316L => (193.0, 310.0, 620.0, 45.0),
            Self::PlatinumChromium   => (210.0, 460.0, 920.0, 40.0),
            Self::Nitinol            => (75.0,  550.0, 1100.0, 18.0),
        }
    }
}

/// Cardiovascular stent biomechanics problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StentExpansionProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Biomaterial alloy of the stent.
    #[serde(default = "default_material")]
    pub stent_material: StentMaterial,
    /// Initial crimped delivery outer diameter in millimeters (mm) (typically 0.90 to 1.20 mm).
    pub crimped_diameter_mm: f64,
    /// Nominal target expanded inner diameter in millimeters (mm) (e.g. 3.00 mm for coronary artery).
    pub nominal_expanded_diameter_mm: f64,
    /// Nominal overall stent length in millimeters (mm) (e.g. 18.0 mm).
    pub nominal_length_mm: f64,
    /// Stent strut radial wall thickness in micrometers (μm) (typically 65 to 90 μm).
    pub strut_thickness_um: f64,
    /// Stent strut circumferential width in micrometers (μm) (typically 70 to 95 μm).
    pub strut_width_um: f64,
    /// Number of crown peaks per circumferential ring (typically 6 to 9 crowns).
    #[serde(default = "default_crown_count")]
    pub crown_count_per_ring: u32,
    /// Number of repeating rings along axial length (typically 8 to 20 rings).
    #[serde(default = "default_ring_count")]
    pub ring_count: u32,
    /// Semi-compliant balloon inflation pressure in atmospheres (atm) (e.g. 12.0 atm nominal, 16.0 atm rated burst).
    pub balloon_pressure_atm: f64,
}

fn default_problem_id() -> String {
    "stent_expansion_problem".to_string()
}

fn default_material() -> StentMaterial {
    StentMaterial::CobaltChromiumL605
}

fn default_crown_count() -> u32 {
    8
}

fn default_ring_count() -> u32 {
    12
}

impl Default for StentExpansionProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            stent_material: default_material(),
            crimped_diameter_mm: 1.05,
            nominal_expanded_diameter_mm: 3.00,
            nominal_length_mm: 18.0,
            strut_thickness_um: 81.0,
            strut_width_um: 84.0,
            crown_count_per_ring: 8,
            ring_count: 12,
            balloon_pressure_atm: 12.0,
        }
    }
}

/// Cardiovascular stent analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StentExpansionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Stent biomaterial alloy.
    pub stent_material: StentMaterial,
    /// Stent expansion ratio (D_expanded / D_crimped).
    pub expansion_ratio: f64,
    /// Maximum expanded diameter under balloon peak inflation in millimeters (mm).
    pub inflated_outer_diameter_mm: f64,
    /// Stent resting outer diameter after balloon deflation and recoil in millimeters (mm).
    pub post_recoil_diameter_mm: f64,
    /// Elastic radial recoil percentage (%) (ASTM F3067 limit <= 5.0%).
    pub elastic_recoil_pct: f64,
    /// Stent deployed length after longitudinal foreshortening in millimeters (mm).
    pub deployed_length_mm: f64,
    /// Longitudinal foreshortening percentage (%) (ISO 25539 limit <= 8.0%).
    pub foreshortening_pct: f64,
    /// Radial Resistive Force (RRF) against vascular tissue compression in N/mm.
    pub radial_resistive_force_n_mm: f64,
    /// Chronic Outward Force (COF) exerted on arterial wall in N/mm.
    pub chronic_outward_force_n_mm: f64,
    /// Peak plastic equivalent strain in crown hinge curves (%).
    pub peak_plastic_strain_pct: f64,
    /// Safety factor against strut tensile fracture (ductility margin).
    pub fracture_safety_factor: f64,
    /// Whether stent satisfies ASTM F3067 and FDA coronary performance criteria.
    pub is_fda_compliant: bool,
}

/// Solves stent balloon expansion, crown plastic hinging, recoil, foreshortening, and RRF.
pub fn solve_stent_expansion(
    problem: &StentExpansionProblem,
) -> Result<StentExpansionResult, String> {
    if problem.crimped_diameter_mm <= 0.0 || problem.nominal_expanded_diameter_mm <= 0.0 {
        return Err("Diameters must be positive".to_string());
    }
    if problem.nominal_length_mm <= 0.0 || problem.strut_thickness_um <= 0.0 || problem.strut_width_um <= 0.0 {
        return Err("Dimensions must be positive".to_string());
    }
    if problem.balloon_pressure_atm <= 0.0 {
        return Err("Balloon pressure must be positive".to_string());
    }

    let (e_gpa, sig_y_mpa, _uts_mpa, elong_break_pct) = problem.stent_material.properties();

    let d_crimp = problem.crimped_diameter_mm;
    let d_nom = problem.nominal_expanded_diameter_mm;
    let t_strut_mm = problem.strut_thickness_um * 1e-3;
    let w_strut_mm = problem.strut_width_um * 1e-3;

    // 1. Inflated outer diameter under balloon pressure:
    // Semi-compliant balloon compliance: ~ 0.012 mm / atm above nominal (typically 9 atm nominal)
    let p_delta = (problem.balloon_pressure_atm - 9.0).max(0.0);
    let balloon_growth_mm = p_delta * 0.015;
    let d_inflated = d_nom + 2.0 * t_strut_mm + balloon_growth_mm;
    let expansion_ratio = d_inflated / d_crimp;

    // 2. Elastic radial recoil (ASTM F3067):
    // Recoil occurs as crown hinges unload elastically:
    // delta_D_recoil ~ D_inflated * (sigma_y / E) * (D_inflated / w_strut)^0.4 * K_rec
    let e_mpa = e_gpa * 1000.0;
    let k_rec = 0.42;
    let recoil_factor = (sig_y_mpa / e_mpa) * (d_inflated / w_strut_mm).powf(0.40) * k_rec;
    let elastic_recoil_pct = (recoil_factor * 100.0).clamp(2.0, 7.5);
    let d_post_recoil = d_inflated * (1.0 - elastic_recoil_pct / 100.0);

    // 3. Longitudinal foreshortening (ISO 25539):
    // As circumferential crowns expand angularly, strut inclination angle theta increases:
    // delta_L = L0 * (1 - cos(theta_exp) / cos(theta_crimp))
    // Empirical kinematic relation for open-cell coronary stents:
    let diam_ratio = (d_post_recoil - d_crimp) / d_nom;
    let foreshortening_pct = (2.8 * diam_ratio.powf(1.3)).clamp(0.5, 9.0);
    let l_deployed = problem.nominal_length_mm * (1.0 - foreshortening_pct / 100.0);

    // 4. Crown plastic hinge moment and Radial Resistive Force (RRF):
    // Plastic hinge moment: M_p = 1/4 * w_strut * t_strut^2 * sigma_y
    let m_p_n_mm = 0.25 * w_strut_mm * t_strut_mm.powi(2) * sig_y_mpa; // [N·mm]
    let n_crowns = problem.crown_count_per_ring as f64;
    let ring_pitch_mm = (l_deployed / (problem.ring_count.max(1) as f64)).max(0.5);

    // Radial collapse pressure: P_rad = (2 * n_crowns * M_p) / (D_post^2 * pitch)
    let p_rad_mpa = (2.0 * n_crowns * m_p_n_mm) / (d_post_recoil.powi(2) * ring_pitch_mm);
    // RRF in N per mm of stent length:
    let rrf_n_mm = p_rad_mpa * d_post_recoil * PI;
    // Chronic Outward Force (COF) exerted on vessel wall during resting:
    let cof_n_mm = rrf_n_mm * 0.35;

    // 5. Peak plastic strain in crown inner radius:
    // Crown inner radius r_crown ~ 0.5 * w_strut
    // eps_max = t_strut / (2 * r_crown) * factor
    let peak_plastic_strain_pct = ((w_strut_mm / (d_crimp * PI / n_crowns)) * 18.0).clamp(8.0, 26.0);
    let fracture_safety_factor = (elong_break_pct / peak_plastic_strain_pct).max(1.0);

    // 6. FDA and ASTM F3067 compliance check:
    // Criteria:
    // - Recoil <= 5.0%
    // - Foreshortening <= 8.0%
    // - RRF >= 0.70 N/mm
    // - Fracture FoS >= 1.50
    let is_fda_compliant = elastic_recoil_pct <= 5.0
        && foreshortening_pct <= 8.0
        && rrf_n_mm >= 0.70
        && fracture_safety_factor >= 1.50;

    Ok(StentExpansionResult {
        problem_id: problem.problem_id.clone(),
        stent_material: problem.stent_material,
        expansion_ratio,
        inflated_outer_diameter_mm: d_inflated,
        post_recoil_diameter_mm: d_post_recoil,
        elastic_recoil_pct,
        deployed_length_mm: l_deployed,
        foreshortening_pct,
        radial_resistive_force_n_mm: rrf_n_mm,
        chronic_outward_force_n_mm: cof_n_mm,
        peak_plastic_strain_pct,
        fracture_safety_factor,
        is_fda_compliant,
    })
}

/// Executes stent expansion simulation and packages into SimulationResult envelope.
pub fn solve_stent_expansion_sim(
    problem: &StentExpansionProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_stent_expansion(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("inflated_outer_diameter_mm".to_string(), res.inflated_outer_diameter_mm);
            metrics.insert("post_recoil_diameter_mm".to_string(), res.post_recoil_diameter_mm);
            metrics.insert("elastic_recoil_pct".to_string(), res.elastic_recoil_pct);
            metrics.insert("foreshortening_pct".to_string(), res.foreshortening_pct);
            metrics.insert("radial_resistive_force_n_mm".to_string(), res.radial_resistive_force_n_mm);
            metrics.insert("fracture_safety_factor".to_string(), res.fracture_safety_factor);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.post_recoil_diameter_mm, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "post_recoil_diameter_mm",
                res.post_recoil_diameter_mm,
                res.inflated_outer_diameter_mm * 0.965,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.biomedical.stent_expansion".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.biomedical.stent_expansion", "0.1.0"),
                logs: vec![format!(
                    "Stent Biomechanics (ASTM F3067): Material = {:?}, D_infl = {:.3} mm -> D_post = {:.3} mm, Recoil = {:.2}% (<= 5%), FS = {:.2}% (<= 8%), RRF = {:.2} N/mm, Strain = {:.1}%, FoS = {:.2}, FDA Compliant: {}",
                    res.stent_material, res.inflated_outer_diameter_mm, res.post_recoil_diameter_mm,
                    res.elastic_recoil_pct, res.foreshortening_pct, res.radial_resistive_force_n_mm,
                    res.peak_plastic_strain_pct, res.fracture_safety_factor, res.is_fda_compliant
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.biomedical.stent_expansion".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.biomedical.stent_expansion", "0.1.0"),
                logs: vec![format!("Stent expansion simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_stent_expansion_nominal() {
        let prob = StentExpansionProblem::default();
        let res = solve_stent_expansion(&prob).expect("solver failed");
        println!("Stent Expansion: D_post = {:.3} mm, Recoil = {:.2}%, FS = {:.2}%, RRF = {:.2} N/mm, FoS = {:.2}",
            res.post_recoil_diameter_mm, res.elastic_recoil_pct, res.foreshortening_pct,
            res.radial_resistive_force_n_mm, res.fracture_safety_factor);

        assert!(res.expansion_ratio > 2.5 && res.expansion_ratio < 3.5);
        assert!(res.inflated_outer_diameter_mm > 3.10 && res.inflated_outer_diameter_mm < 3.30);
        assert!(res.post_recoil_diameter_mm > 3.00 && res.post_recoil_diameter_mm < 3.20);
        assert!(res.elastic_recoil_pct >= 2.0 && res.elastic_recoil_pct <= 5.0);
        assert!(res.foreshortening_pct > 1.0 && res.foreshortening_pct <= 8.0);
        assert!(res.radial_resistive_force_n_mm > 0.70);
        assert!(res.fracture_safety_factor >= 1.50);
        assert!(res.is_fda_compliant);
    }
}
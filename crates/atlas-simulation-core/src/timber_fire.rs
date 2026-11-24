//! Structural Timber Fire Resistance & Charring Kinetics Solver (Eurocode 5 EN 1995-1-2 / ISO 834).
//!
//! Models 1D/2D charring kinetics of mass timber, glued laminated timber (glulam),
//! and solid structural softwood under ISO 834 standard fire exposure.
//! Evaluates effective charring depth, reduced residual cross-section,
//! section modulus W_ef, accidental fire limit state bending stresses,
//! and verifies structural fire resistance ratings (R30, R60, R90, R120).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;

/// Structural timber material grade and species classification.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TimberGrade {
    /// Glued laminated timber GL24h (EN 14080).
    GlulamGL24h,
    /// High-strength glued laminated timber GL28h (EN 14080).
    GlulamGL28h,
    /// Premium glued laminated timber GL32h (EN 14080).
    GlulamGL32h,
    /// Standard structural softwood C24 (EN 338).
    SoftwoodC24,
    /// High-grade structural softwood C30 (EN 338).
    SoftwoodC30,
    /// Structural hardwood D30 (EN 338).
    HardwoodD30,
}

impl TimberGrade {
    /// Characteristic bending strength f_m,k (MPa / N/mm²).
    pub fn characteristic_bending_strength_mpa(&self) -> f64 {
        match self {
            Self::GlulamGL24h => 24.0,
            Self::GlulamGL28h => 28.0,
            Self::GlulamGL32h => 32.0,
            Self::SoftwoodC24 => 24.0,
            Self::SoftwoodC30 => 30.0,
            Self::HardwoodD30 => 30.0,
        }
    }

    /// Characteristic density rho_k (kg/m³).
    pub fn characteristic_density_kg_m3(&self) -> f64 {
        match self {
            Self::GlulamGL24h => 385.0,
            Self::GlulamGL28h => 425.0,
            Self::GlulamGL32h => 440.0,
            Self::SoftwoodC24 => 350.0,
            Self::SoftwoodC30 => 380.0,
            Self::HardwoodD30 => 530.0,
        }
    }

    /// 1D nominal charring rate beta_0 (mm/min) per EN 1995-1-2 Table 3.1.
    pub fn nominal_charring_rate_mm_min(&self) -> f64 {
        match self {
            Self::GlulamGL24h | Self::GlulamGL28h | Self::GlulamGL32h => 0.65,
            Self::SoftwoodC24 | Self::SoftwoodC30 => 0.80,
            Self::HardwoodD30 => 0.55,
        }
    }

    /// Fire strength 20% fractile factor k_fi per EN 1995-1-2 Table 2.1.
    pub fn fire_strength_factor_k_fi(&self) -> f64 {
        match self {
            Self::GlulamGL24h | Self::GlulamGL28h | Self::GlulamGL32h => 1.15,
            Self::SoftwoodC24 | Self::SoftwoodC30 => 1.25,
            Self::HardwoodD30 => 1.15,
        }
    }
}

/// Exposure boundary conditions of timber element during fire.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum FireExposureSides {
    /// Exposed on 3 sides: sides and bottom (top protected by concrete slab or gypsum floor).
    ThreeSides,
    /// Exposed on all 4 sides: freestanding column or unshielded beam.
    FourSides,
    /// Exposed on 1 side: wall, ceiling, or roof panel.
    OneSide,
}

/// Timber fire structural engineering problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimberFireProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Timber grade and species.
    pub timber_grade: TimberGrade,
    /// Initial member breadth b (mm).
    pub initial_breadth_mm: f64,
    /// Initial member depth/height h (mm).
    pub initial_depth_mm: f64,
    /// Clear beam span L (m).
    pub span_m: f64,
    /// Sides exposed to fire.
    pub exposure: FireExposureSides,
    /// Fire exposure duration t_fire (minutes), e.g. 30 (R30), 60 (R60), 90 (R90).
    pub fire_duration_min: f64,
    /// Ambient ultimate limit state (ULS) design bending moment M_Ed (kNm).
    pub ambient_moment_ed_kn_m: f64,
    /// Accidental fire reduction factor eta_fi = M_Ed,fi / M_Ed (typically 0.60 to 0.70).
    pub fire_load_ratio_eta_fi: f64,
}

impl Default for TimberFireProblem {
    fn default() -> Self {
        Self {
            problem_id: "glulam_roof_beam_r60_fire".to_string(),
            timber_grade: TimberGrade::GlulamGL28h,
            initial_breadth_mm: 200.0,
            initial_depth_mm: 500.0,
            span_m: 8.0,
            exposure: FireExposureSides::ThreeSides,
            fire_duration_min: 60.0, // R60 1-hour standard fire
            ambient_moment_ed_kn_m: 140.0,
            fire_load_ratio_eta_fi: 0.60, // 60% of cold ULS moment
        }
    }
}

/// Solution output from timber fire structural resistance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TimberFireResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Standard gas temperature inside fire compartment per ISO 834 (°C).
    pub iso834_gas_temp_c: f64,
    /// Nominal 1D charring rate beta_0 (mm/min).
    pub nominal_charring_rate_mm_min: f64,
    /// 1D charring depth d_char,0 (mm).
    pub charring_depth_dchar_mm: f64,
    /// Zero-strength heated transition layer thickness k_0 (mm).
    pub zero_strength_layer_k0_mm: f64,
    /// Effective total charring depth d_ef = d_char + k_0 (mm).
    pub effective_char_depth_def_mm: f64,
    /// Residual effective breadth b_ef (mm).
    pub residual_breadth_bef_mm: f64,
    /// Residual effective depth h_ef (mm).
    pub residual_depth_hef_mm: f64,
    /// Residual effective cross-sectional area A_ef (mm²).
    pub residual_area_aef_mm2: f64,
    /// Initial cold cross-sectional area A_0 (mm²).
    pub initial_area_a0_mm2: f64,
    /// Cross-sectional area loss percentage (%).
    pub area_loss_pct: f64,
    /// Residual elastic section modulus W_ef (mm³).
    pub residual_section_modulus_wef_mm3: f64,
    /// Accidental fire design moment M_Ed,fi (kNm).
    pub fire_design_moment_med_fi_kn_m: f64,
    /// Fire design bending strength f_m,d,fi (MPa).
    pub fire_design_strength_fmd_fi_mpa: f64,
    /// Actual bending stress in residual core sigma_m,fi (MPa).
    pub fire_bending_stress_sigmam_fi_mpa: f64,
    /// Structural utilization ratio UR_fi = sigma_m,fi / f_m,d,fi.
    pub fire_utilization_ratio: f64,
    /// Whether the member satisfies fire limit state (UR_fi <= 1.0).
    pub fire_resistance_verified: bool,
    /// Estimated critical fire endurance duration before failure (minutes).
    pub critical_fire_endurance_min: f64,
}

/// Computes ISO 834 standard fire furnace temperature at time t (minutes).
pub fn iso834_temperature_c(t_min: f64) -> f64 {
    if t_min <= 0.0 {
        20.0
    } else {
        20.0 + 345.0 * (8.0 * t_min + 1.0).log10()
    }
}

/// Solves timber fire structural resistance per Eurocode 5 EN 1995-1-2.
pub fn solve_timber_fire(prob: &TimberFireProblem) -> TimberFireResult {
    let t_fire = prob.fire_duration_min.max(1.0);
    let beta_0 = prob.timber_grade.nominal_charring_rate_mm_min();

    // ISO 834 gas temperature
    let gas_temp_c = iso834_temperature_c(t_fire);

    // 1D charring depth
    let d_char_0 = beta_0 * t_fire;

    // Zero-strength layer k0 (EN 1995-1-2 Section 4.2.2)
    // For t < 20 min: k0 = (t / 20) * 7.0 mm; for t >= 20 min: k0 = 7.0 mm
    let k0 = if t_fire < 20.0 {
        (t_fire / 20.0) * 7.0
    } else {
        7.0
    };

    let d_ef = d_char_0 + k0;

    // Residual cross section based on exposure
    let (b_ef, h_ef) = match prob.exposure {
        FireExposureSides::ThreeSides => {
            let b = (prob.initial_breadth_mm - 2.0 * d_ef).max(0.0);
            let h = (prob.initial_depth_mm - d_ef).max(0.0);
            (b, h)
        }
        FireExposureSides::FourSides => {
            let b = (prob.initial_breadth_mm - 2.0 * d_ef).max(0.0);
            let h = (prob.initial_depth_mm - 2.0 * d_ef).max(0.0);
            (b, h)
        }
        FireExposureSides::OneSide => {
            let b = prob.initial_breadth_mm;
            let h = (prob.initial_depth_mm - d_ef).max(0.0);
            (b, h)
        }
    };

    let initial_area = prob.initial_breadth_mm * prob.initial_depth_mm;
    let residual_area = b_ef * h_ef;
    let area_loss_pct = ((initial_area - residual_area) / initial_area * 100.0).clamp(0.0, 100.0);

    // Residual section modulus W_ef = (b_ef * h_ef^2) / 6
    let w_ef = if b_ef > 0.0 && h_ef > 0.0 {
        (b_ef * h_ef.powi(2)) / 6.0
    } else {
        1e-6
    };

    // Fire design moment: M_Ed,fi = eta_fi * M_Ed
    let m_ed_fi = prob.fire_load_ratio_eta_fi * prob.ambient_moment_ed_kn_m;

    // Fire design bending strength: f_m,d,fi = k_mod,fi * k_fi * (f_m,k / gamma_M,fi)
    // k_mod,fi = 1.0, gamma_M,fi = 1.0
    let k_mod_fi = 1.0;
    let k_fi = prob.timber_grade.fire_strength_factor_k_fi();
    let gamma_m_fi = 1.0;
    let f_m_k = prob.timber_grade.characteristic_bending_strength_mpa();
    let f_m_d_fi = k_mod_fi * k_fi * (f_m_k / gamma_m_fi);

    // Fire bending stress: sigma_m,fi = M_Ed,fi * 10^6 / W_ef (N/mm²)
    let sigma_m_fi = (m_ed_fi * 1e6) / w_ef;
    let ur_fi = if w_ef > 1.0 {
        sigma_m_fi / f_m_d_fi
    } else {
        999.0
    };
    let fire_verified = ur_fi <= 1.0 && b_ef > 10.0 && h_ef > 10.0;

    // Estimate critical fire endurance (bisection or secant from 1 to 240 min)
    let mut t_crit = t_fire;
    for t_step in 5..=240 {
        let t = t_step as f64;
        let d_c = beta_0 * t;
        let k_val = if t < 20.0 { (t / 20.0) * 7.0 } else { 7.0 };
        let d_e = d_c + k_val;
        let (b_t, h_t) = match prob.exposure {
            FireExposureSides::ThreeSides => (
                prob.initial_breadth_mm - 2.0 * d_e,
                prob.initial_depth_mm - d_e,
            ),
            FireExposureSides::FourSides => (
                prob.initial_breadth_mm - 2.0 * d_e,
                prob.initial_depth_mm - 2.0 * d_e,
            ),
            FireExposureSides::OneSide => (prob.initial_breadth_mm, prob.initial_depth_mm - d_e),
        };
        if b_t <= 5.0 || h_t <= 5.0 {
            t_crit = t;
            break;
        }
        let w_t = (b_t * h_t.powi(2)) / 6.0;
        let sigma_t = (m_ed_fi * 1e6) / w_t;
        if sigma_t > f_m_d_fi {
            t_crit = t;
            break;
        }
        t_crit = t;
    }

    TimberFireResult {
        problem_id: prob.problem_id.clone(),
        iso834_gas_temp_c: gas_temp_c,
        nominal_charring_rate_mm_min: beta_0,
        charring_depth_dchar_mm: d_char_0,
        zero_strength_layer_k0_mm: k0,
        effective_char_depth_def_mm: d_ef,
        residual_breadth_bef_mm: b_ef,
        residual_depth_hef_mm: h_ef,
        residual_area_aef_mm2: residual_area,
        initial_area_a0_mm2: initial_area,
        area_loss_pct,
        residual_section_modulus_wef_mm3: w_ef,
        fire_design_moment_med_fi_kn_m: m_ed_fi,
        fire_design_strength_fmd_fi_mpa: f_m_d_fi,
        fire_bending_stress_sigmam_fi_mpa: sigma_m_fi,
        fire_utilization_ratio: ur_fi,
        fire_resistance_verified: fire_verified,
        critical_fire_endurance_min: t_crit,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_timber_fire_sim(prob: &TimberFireProblem) -> SimulationResult {
    let sol = solve_timber_fire(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert(
        "effective_char_depth_def_mm".to_string(),
        sol.effective_char_depth_def_mm,
    );
    metrics.insert(
        "residual_breadth_bef_mm".to_string(),
        sol.residual_breadth_bef_mm,
    );
    metrics.insert(
        "residual_depth_hef_mm".to_string(),
        sol.residual_depth_hef_mm,
    );
    metrics.insert(
        "fire_utilization_ratio".to_string(),
        sol.fire_utilization_ratio,
    );
    metrics.insert(
        "critical_fire_endurance_min".to_string(),
        sol.critical_fire_endurance_min,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.fire_utilization_ratio, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.structural.timber_fire_ec5".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: None,
        provenance: SimulationProvenance::new("atlas.structural.timber_fire_ec5", "0.1.0"),
        logs: vec![format!(
            "Fire duration: {:.0} min, Char depth: {:.1} mm, UR: {:.2}, Verified: {}",
            prob.fire_duration_min,
            sol.effective_char_depth_def_mm,
            sol.fire_utilization_ratio,
            sol.fire_resistance_verified
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_glulam_roof_beam_r60_fire_verification() {
        let prob = TimberFireProblem::default();
        let sol = solve_timber_fire(&prob);

        // Standard ISO 834 fire after 60 min reaches ~945°C
        assert!((sol.iso834_gas_temp_c - 945.3).abs() < 10.0);

        // Beta_0 for glulam is 0.65 mm/min => d_char = 0.65 * 60 = 39.0 mm
        assert!((sol.charring_depth_dchar_mm - 39.0).abs() < 1e-3);
        // k0 = 7.0 mm => d_ef = 39.0 + 7.0 = 46.0 mm
        assert!((sol.effective_char_depth_def_mm - 46.0).abs() < 1e-3);

        // 3-sided exposure on 200 x 500 mm:
        // b_ef = 200 - 2 * 46 = 108 mm
        assert!((sol.residual_breadth_bef_mm - 108.0).abs() < 1e-3);
        // h_ef = 500 - 46 = 454 mm
        assert!((sol.residual_depth_hef_mm - 454.0).abs() < 1e-3);

        // Residual section modulus W_ef = (108 * 454^2) / 6 = 3,709,928 mm³
        assert!((sol.residual_section_modulus_wef_mm3 - 3_710_112.0).abs() < 2000.0);

        // Fire moment M_Ed,fi = 0.60 * 140 = 84.0 kNm
        assert!((sol.fire_design_moment_med_fi_kn_m - 84.0).abs() < 1e-3);

        // Bending stress: 84 * 10^6 / 3.71e6 ≈ 22.6 MPa
        assert!(
            sol.fire_bending_stress_sigmam_fi_mpa > 20.0
                && sol.fire_bending_stress_sigmam_fi_mpa < 25.0
        );

        // Design fire strength: f_m,d,fi = 1.0 * 1.15 * 28.0 = 32.2 MPa
        assert!((sol.fire_design_strength_fmd_fi_mpa - 32.2).abs() < 1e-3);

        // Utilization ratio UR ≈ 22.6 / 32.2 ≈ 0.70 < 1.0
        assert!(sol.fire_utilization_ratio < 0.85);
        assert!(sol.fire_resistance_verified);

        // Critical endurance should exceed 60 minutes (R60 passed)
        assert!(sol.critical_fire_endurance_min >= 60.0);
    }
}

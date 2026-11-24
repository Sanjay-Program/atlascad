//! Reinforced Concrete Beam Flexural Design & Crack Width Control (Eurocode 2 EN 1992-1-1 / ACI 318).
//!
//! Solves ultimate limit state (ULS) flexural moment capacity, neutral axis depth,
//! ductility verification, minimum/maximum reinforcement limits, and serviceability
//! limit state (SLS) characteristic crack width wk calculation.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;

/// Structural concrete strength class per EN 1992-1-1 Table 3.1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum ConcreteGrade {
    C20_25,
    C25_30,
    C30_37,
    C35_45,
    C40_50,
    C50_60,
}

impl ConcreteGrade {
    /// Characteristic cylinder compressive strength f_ck (MPa).
    pub fn characteristic_strength_fck_mpa(&self) -> f64 {
        match self {
            Self::C20_25 => 20.0,
            Self::C25_30 => 25.0,
            Self::C30_37 => 30.0,
            Self::C35_45 => 35.0,
            Self::C40_50 => 40.0,
            Self::C50_60 => 50.0,
        }
    }

    /// Design compressive strength f_cd = alpha_cc * f_ck / gamma_c (MPa).
    /// alpha_cc = 0.85, gamma_c = 1.50.
    pub fn design_compressive_strength_fcd_mpa(&self) -> f64 {
        0.85 * self.characteristic_strength_fck_mpa() / 1.50
    }

    /// Mean tensile strength f_ctm = 0.30 * f_ck^(2/3) (MPa).
    pub fn mean_tensile_strength_fctm_mpa(&self) -> f64 {
        0.30 * self.characteristic_strength_fck_mpa().powf(2.0 / 3.0)
    }

    /// Secant modulus of elasticity E_cm (GPa).
    pub fn secant_modulus_ecm_gpa(&self) -> f64 {
        22.0 * ((self.characteristic_strength_fck_mpa() + 8.0) / 10.0).powf(0.3)
    }
}

/// Reinforced concrete problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinforcedConcreteProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Concrete strength class.
    pub concrete_grade: ConcreteGrade,
    /// Reinforcing steel characteristic yield strength f_yk (MPa, typically 500 MPa).
    pub rebar_yield_strength_fyk_mpa: f64,
    /// Beam breadth b (mm).
    pub breadth_b_mm: f64,
    /// Beam overall depth h (mm).
    pub depth_h_mm: f64,
    /// Concrete clear cover c_nom to main tensile reinforcement (mm).
    pub concrete_cover_c_mm: f64,
    /// Main tension rebar diameter phi (mm, e.g. 16, 20, 25 mm).
    pub rebar_diameter_mm: f64,
    /// Number of main tension rebars.
    pub rebar_count: usize,
    /// Applied design ultimate bending moment M_Ed (kNm).
    pub applied_moment_ed_kn_m: f64,
    /// Ratio of quasi-permanent service moment to ULS moment (typically 0.60 to 0.70).
    pub quasi_permanent_moment_ratio: f64,
    /// Maximum allowable characteristic crack width w_max (mm, 0.30 mm per EN 1992-1-1).
    pub max_allowable_crack_width_mm: f64,
}

impl Default for ReinforcedConcreteProblem {
    fn default() -> Self {
        Self {
            problem_id: "building_floor_beam_span".to_string(),
            concrete_grade: ConcreteGrade::C30_37,
            rebar_yield_strength_fyk_mpa: 500.0,
            breadth_b_mm: 300.0,
            depth_h_mm: 600.0,
            concrete_cover_c_mm: 35.0,
            rebar_diameter_mm: 20.0,
            rebar_count: 4, // 4x T20 = 1256 mm²
            applied_moment_ed_kn_m: 240.0,
            quasi_permanent_moment_ratio: 0.65,
            max_allowable_crack_width_mm: 0.30,
        }
    }
}

/// Reinforced concrete structural analysis results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReinforcedConcreteResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Effective depth d = h - c - phi/2 (mm).
    pub effective_depth_d_mm: f64,
    /// Total provided tension reinforcement area A_s (mm²).
    pub provided_steel_area_as_mm2: f64,
    /// Minimum required reinforcement area A_s,min (mm²).
    pub min_steel_area_as_min_mm2: f64,
    /// Maximum allowable reinforcement area A_s,max = 0.04 * b * h (mm²).
    pub max_steel_area_as_max_mm2: f64,
    /// Neutral axis depth at ULS x (mm).
    pub neutral_axis_depth_x_mm: f64,
    /// Neutral axis depth ratio xi = x / d.
    pub neutral_axis_ratio_xi: f64,
    /// Internal lever arm z = d - 0.4*x (mm).
    pub internal_lever_arm_z_mm: f64,
    /// Design flexural resistance moment M_Rd (kNm).
    pub design_moment_resistance_mrd_kn_m: f64,
    /// Flexural utilization ratio UR = M_Ed / M_Rd.
    pub flexural_utilization_ratio: f64,
    /// Whether ULS flexural capacity is adequate (M_Ed <= M_Rd).
    pub uls_flexure_adequate: bool,
    /// Steel stress under quasi-permanent service load sigma_s (MPa).
    pub steel_service_stress_sigma_s_mpa: f64,
    /// Maximum crack spacing s_r,max (mm).
    pub max_crack_spacing_sr_max_mm: f64,
    /// Mean strain difference (eps_sm - eps_cm).
    pub mean_strain_difference: f64,
    /// Calculated characteristic crack width w_k (mm).
    pub characteristic_crack_width_wk_mm: f64,
    /// Whether crack width complies with SLS limit (w_k <= w_max).
    pub crack_width_compliant: bool,
}

/// Solves reinforced concrete beam flexure and crack width per Eurocode 2.
pub fn solve_reinforced_concrete(prob: &ReinforcedConcreteProblem) -> ReinforcedConcreteResult {
    let b = prob.breadth_b_mm.max(50.0);
    let h = prob.depth_h_mm.max(100.0);
    let phi = prob.rebar_diameter_mm.max(6.0);
    let c = prob.concrete_cover_c_mm.max(15.0);

    // Effective depth d = h - c - phi / 2
    let d = (h - c - (phi / 2.0)).max(50.0);

    // Provided tension reinforcement: A_s = n * pi * phi^2 / 4
    let n_bars = prob.rebar_count.max(1);
    let a_s = (n_bars as f64) * (PI * phi.powi(2) / 4.0);

    // Concrete properties
    let _f_ck = prob.concrete_grade.characteristic_strength_fck_mpa();
    let f_cd = prob.concrete_grade.design_compressive_strength_fcd_mpa();
    let f_ctm = prob.concrete_grade.mean_tensile_strength_fctm_mpa();

    // Steel design strength: f_yd = f_yk / 1.15
    let f_yd = prob.rebar_yield_strength_fyk_mpa / 1.15;
    let e_s = 200_000.0; // MPa

    // Limits on reinforcement area (EN 1992-1-1 §9.2.1.1):
    // A_s,min = max(0.26 * (f_ctm / f_yk) * b * d, 0.0013 * b * d)
    let a_s_min = (0.26 * (f_ctm / prob.rebar_yield_strength_fyk_mpa) * b * d).max(0.0013 * b * d);
    let a_s_max = 0.04 * b * h;

    // ULS Flexural Equilibrium:
    // Compressive force: F_c = eta * f_cd * b * (lambda * x) where lambda = 0.80, eta = 1.0
    // Tensile force: F_s = A_s * f_yd
    // x = (A_s * f_yd) / (0.80 * f_cd * b)
    let x = (a_s * f_yd) / (0.80 * f_cd * b);
    let xi = x / d;

    // Lever arm: z = d - 0.4 * x
    let z = (d - 0.40 * x).min(0.95 * d).max(0.5 * d);

    // Moment resistance: M_Rd = A_s * f_yd * z * 10^-6 (kNm)
    let m_rd = (a_s * f_yd * z) / 1e6;
    let ur_flexure = prob.applied_moment_ed_kn_m / m_rd;
    let uls_adequate = ur_flexure <= 1.0 && xi <= 0.45 && a_s >= a_s_min && a_s <= a_s_max;

    // SLS Crack Width Verification (EN 1992-1-1 §7.3.4):
    let m_qp = prob.quasi_permanent_moment_ratio * prob.applied_moment_ed_kn_m;

    // Cracked transformed section neutral axis x_cr:
    // alpha_e = E_s / E_c,eff where creep factor phi_ef ≈ 2.0 -> E_c,eff ≈ E_cm / 3.0
    let e_cm = prob.concrete_grade.secant_modulus_ecm_gpa() * 1000.0; // MPa
    let alpha_e = (e_s / (e_cm / 3.0)).clamp(10.0, 25.0);

    // 0.5 * b * x_cr^2 + alpha_e * A_s * x_cr - alpha_e * A_s * d = 0
    let b_quad = alpha_e * a_s;
    let c_quad = -alpha_e * a_s * d;
    let x_cr = (-b_quad + (b_quad.powi(2) - 4.0 * (0.5 * b) * c_quad).sqrt()) / (2.0 * 0.5 * b);

    // Lever arm in cracked state: z_cr = d - x_cr / 3
    let z_cr = d - x_cr / 3.0;
    let sigma_s = (m_qp * 1e6) / (a_s * z_cr.max(1.0));

    // Effective tension area A_c,eff (EN 1992-1-1 §7.3.2):
    // h_c,eff = min(2.5 * (h - d), (h - x_cr) / 3, h / 2)
    let h_c_eff = (2.5 * (h - d))
        .min((h - x_cr) / 3.0)
        .min(h / 2.0)
        .max(2.0 * phi);
    let a_c_eff = b * h_c_eff;
    let rho_p_eff = (a_s / a_c_eff).max(0.001);

    // Maximum crack spacing s_r,max (EN 1992-1-1 Eq 7.11):
    // s_r,max = 3.4 * c + 0.425 * k1 * k2 * phi / rho_p,eff (k1=0.8 for ribbed, k2=0.5 for bending)
    let k1 = 0.8;
    let k2 = 0.5;
    let s_r_max = 3.4 * c + 0.425 * k1 * k2 * (phi / rho_p_eff);

    // Mean strain difference: (eps_sm - eps_cm) per Eq 7.9:
    let k_t = 0.40; // Long-term loading
    let f_ct_eff = f_ctm;
    let term1 = (sigma_s - k_t * (f_ct_eff / rho_p_eff) * (1.0 + alpha_e * rho_p_eff)) / e_s;
    let term2 = 0.60 * (sigma_s / e_s);
    let eps_diff = term1.max(term2).max(0.0);

    // Characteristic crack width: w_k = s_r,max * (eps_sm - eps_cm)
    let w_k = s_r_max * eps_diff;
    let crack_compliant = w_k <= prob.max_allowable_crack_width_mm;

    ReinforcedConcreteResult {
        problem_id: prob.problem_id.clone(),
        effective_depth_d_mm: d,
        provided_steel_area_as_mm2: a_s,
        min_steel_area_as_min_mm2: a_s_min,
        max_steel_area_as_max_mm2: a_s_max,
        neutral_axis_depth_x_mm: x,
        neutral_axis_ratio_xi: xi,
        internal_lever_arm_z_mm: z,
        design_moment_resistance_mrd_kn_m: m_rd,
        flexural_utilization_ratio: ur_flexure,
        uls_flexure_adequate: uls_adequate,
        steel_service_stress_sigma_s_mpa: sigma_s,
        max_crack_spacing_sr_max_mm: s_r_max,
        mean_strain_difference: eps_diff,
        characteristic_crack_width_wk_mm: w_k,
        crack_width_compliant: crack_compliant,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_reinforced_concrete_sim(prob: &ReinforcedConcreteProblem) -> SimulationResult {
    let sol = solve_reinforced_concrete(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert(
        "design_moment_resistance_mrd_kn_m".to_string(),
        sol.design_moment_resistance_mrd_kn_m,
    );
    metrics.insert(
        "flexural_utilization_ratio".to_string(),
        sol.flexural_utilization_ratio,
    );
    metrics.insert(
        "characteristic_crack_width_wk_mm".to_string(),
        sol.characteristic_crack_width_wk_mm,
    );
    metrics.insert(
        "neutral_axis_ratio_xi".to_string(),
        sol.neutral_axis_ratio_xi,
    );
    metrics.insert(
        "steel_service_stress_sigma_s_mpa".to_string(),
        sol.steel_service_stress_sigma_s_mpa,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.design_moment_resistance_mrd_kn_m, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.civil.reinforced_concrete_ec2".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: None,
        provenance: SimulationProvenance::new("atlas.civil.reinforced_concrete_ec2", "0.1.0"),
        logs: vec![format!(
            "M_Rd: {:.1} kNm (UR: {:.2}), xi: {:.3}, w_k: {:.3} mm (limit {:.2} mm), Compliant: {}",
            sol.design_moment_resistance_mrd_kn_m,
            sol.flexural_utilization_ratio,
            sol.neutral_axis_ratio_xi,
            sol.characteristic_crack_width_wk_mm,
            prob.max_allowable_crack_width_mm,
            sol.crack_width_compliant
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_c30_floor_beam_flexure_and_cracking() {
        let prob = ReinforcedConcreteProblem::default();
        let sol = solve_reinforced_concrete(&prob);

        // Effective depth d = 600 - 35 - 10 = 555 mm
        assert!((sol.effective_depth_d_mm - 555.0).abs() < 1e-3);

        // 4x T20 = 4 * pi * 100 = 1256.6 mm²
        assert!((sol.provided_steel_area_as_mm2 - 1256.6).abs() < 1.0);

        // ULS neutral axis depth x = (1256.6 * 434.78) / (0.8 * 17.0 * 300) = 546377 / 4080 ≈ 133.9 mm
        assert!((sol.neutral_axis_depth_x_mm - 133.9).abs() < 2.0);
        // xi = 133.9 / 555 = 0.241 < 0.45 (ductile under-reinforced)
        assert!(sol.neutral_axis_ratio_xi < 0.30);

        // Lever arm z = 555 - 0.4 * 133.9 = 501.4 mm
        assert!((sol.internal_lever_arm_z_mm - 501.4).abs() < 2.0);

        // M_Rd = 1256.6 * 434.78 * 501.4 / 1e6 ≈ 274.0 kNm
        assert!(
            sol.design_moment_resistance_mrd_kn_m > 260.0
                && sol.design_moment_resistance_mrd_kn_m < 290.0
        );
        // Applied M_Ed = 240 kNm -> UR ≈ 0.87 < 1.0
        assert!(sol.flexural_utilization_ratio < 1.0);
        assert!(sol.uls_flexure_adequate);

        // Service stress in steel sigma_s under 0.65 * 240 = 156 kNm: ~260 to 300 MPa
        assert!(
            sol.steel_service_stress_sigma_s_mpa > 220.0
                && sol.steel_service_stress_sigma_s_mpa < 320.0
        );

        // Characteristic crack width w_k should be between 0.15 mm and 0.28 mm <= 0.30 mm limit
        assert!(
            sol.characteristic_crack_width_wk_mm > 0.10
                && sol.characteristic_crack_width_wk_mm <= 0.30
        );
        assert!(sol.crack_width_compliant);
    }
}

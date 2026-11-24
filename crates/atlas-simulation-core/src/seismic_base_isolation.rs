// crates/atlas-simulation-core/src/seismic_base_isolation.rs
//
// Civil & Structural Earthquake Engineering:
// Elastomeric Lead-Rubber Bearing (LRB) Seismic Base Isolation Mechanics.
//
// Based on:
// - ASCE 7-22: Minimum Design Loads and Associated Criteria for Buildings and Other Structures (Chapter 17: Seismic Design Requirements for Seismically Isolated Structures)
// - FEMA P-751: NEHRP Recommended Seismic Provisions: Design Examples (Chapter 12: Seismically Isolated Structures)
// - Eurocode 8 (EN 1998-1): Design of structures for earthquake resistance — Part 1: General rules, seismic actions and rules for buildings
// - Kelly & Konstantinidis: "Mechanics of Rubber Bearings for Seismic and Vibration Isolation" (Wiley)
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "seismic_base_isolation_problem".to_string()
}

/// Seismic isolator bearing elastomer rubber compound grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RubberCompoundGrade {
    /// Low-damping natural rubber (G ~ 0.40 MPa).
    LowDampingNaturalRubber,
    /// Standard engineering natural rubber (G ~ 0.45 MPa).
    #[default]
    StandardNaturalRubber,
    /// High-damping synthetic rubber (G ~ 0.60 MPa).
    HighDampingRubber,
}

/// Seismic base isolation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeismicBaseIsolationProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Elastomer rubber compound grade.
    #[serde(default)]
    pub rubber_grade: RubberCompoundGrade,
    /// Total superstructure seismic dead + live operating mass in metric tonnes (t).
    pub superstructure_mass_tonnes: f64,
    /// Total number of lead-rubber bearings (LRBs) supporting the isolated building.
    pub bearing_count: u32,
    /// Outer diameter of circular elastomeric bearing in millimeters (mm).
    pub bearing_outer_diameter_mm: f64,
    /// Diameter of central cylindrical energy-dissipating lead plug in millimeters (mm).
    pub lead_core_diameter_mm: f64,
    /// Total thickness of all internal rubber elastomer layers (net rubber height Tr) in millimeters (mm).
    pub total_rubber_thickness_mm: f64,
    /// Dynamic shear yield strength of high-purity chemical lead plug in MegaPascals (MPa).
    pub lead_yield_strength_mpa: f64,
    /// ASCE 7 MCE_R 1-second mapped spectral response acceleration S1 (dimensionless, fraction of g).
    pub mapped_spectral_accel_s1_g: f64,
    /// ASCE 7 Site coefficient factor Fv for site soil classification (e.g. 1.20 for Site Class C/D).
    pub site_coefficient_fv: f64,
    /// Fixed-base (un-isolated) building fundamental vibration period in seconds (s).
    pub fixed_base_period_s: f64,
}

impl Default for SeismicBaseIsolationProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            rubber_grade: RubberCompoundGrade::StandardNaturalRubber,
            superstructure_mass_tonnes: 4500.0,
            bearing_count: 16,
            bearing_outer_diameter_mm: 800.0,
            lead_core_diameter_mm: 160.0,
            total_rubber_thickness_mm: 200.0,
            lead_yield_strength_mpa: 10.5,
            mapped_spectral_accel_s1_g: 0.60,
            site_coefficient_fv: 1.20,
            fixed_base_period_s: 0.55,
        }
    }
}

/// Seismic base isolation simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SeismicBaseIsolationResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total seismic building weight W in kiloNewtons (kN).
    pub total_seismic_weight_kn: f64,
    /// Lead core cross-sectional area per bearing in square millimeters (mm²).
    pub lead_core_area_mm2: f64,
    /// Net rubber elastomer cross-sectional area per bearing in square millimeters (mm²).
    pub rubber_net_area_mm2: f64,
    /// Characteristic yield strength Qd of a single bearing in kiloNewtons (kN).
    pub single_bearing_yield_force_qd_kn: f64,
    /// Total isolation system characteristic yield strength Qd_total in kiloNewtons (kN).
    pub total_system_yield_force_qd_kn: f64,
    /// Post-yield horizontal stiffness Kd of a single bearing in kiloNewtons per meter (kN/m).
    pub single_bearing_post_yield_stiffness_kd_kn_m: f64,
    /// Total isolation system post-yield stiffness Kd_total in kiloNewtons per meter (kN/m).
    pub total_system_post_yield_stiffness_kd_kn_m: f64,
    /// ASCE 7-22 design 1-second spectral acceleration parameter S_D1 in g.
    pub design_spectral_accel_sd1_g: f64,
    /// Seismically isolated effective fundamental period T_eff at design displacement in seconds (s).
    pub effective_isolated_period_s: f64,
    /// Effective equivalent viscous damping ratio beta_eff of isolation system at design displacement (%).
    pub effective_damping_ratio_pct: f64,
    /// ASCE 7-22 damping adjustment reduction factor B_D (Table 17.5-1).
    pub damping_factor_bd: f64,
    /// Maximum Design Earthquake (DE) horizontal spectral displacement D_D in millimeters (mm).
    pub design_horizontal_displacement_dd_mm: f64,
    /// Peak maximum total shear strain in rubber elastomer (gamma = D_D / Tr) (%).
    pub rubber_total_shear_strain_pct: f64,
    /// Design isolated base shear force V_b transmitted to superstructure in kiloNewtons (kN).
    pub isolated_base_shear_force_kn: f64,
    /// Design base shear seismic coefficient (C_s = V_b / W).
    pub base_shear_coefficient: f64,
    /// Un-isolated conventional fixed-base building design base shear in kiloNewtons (kN).
    pub fixed_base_shear_force_kn: f64,
    /// Seismic base shear reduction ratio (V_b / V_fixed) achieved by isolation system.
    pub base_shear_reduction_ratio: f64,
    /// Structural seismic force attenuation percentage (%).
    pub seismic_force_attenuation_pct: f64,
    /// Whether rubber shear strain is within safe elastomer fatigue limit (gamma <= 150%).
    pub is_rubber_strain_safe: bool,
    /// Whether base isolation period achieves standard target shift (T_eff >= 2.5 * T_fixed).
    pub is_period_shift_adequate: bool,
    /// Whether seismic base shear is attenuated by at least 50% compared to fixed-base building.
    pub is_base_isolation_effective: bool,
}

/// Solves elastomeric lead-rubber bearing hysteretic loop, effective damping, displacement, and base shear attenuation.
pub fn solve_seismic_base_isolation(
    problem: &SeismicBaseIsolationProblem,
) -> Result<SeismicBaseIsolationResult, String> {
    if problem.superstructure_mass_tonnes <= 50.0 || problem.bearing_count == 0 {
        return Err("Building mass and bearing count must be positive".to_string());
    }
    if problem.bearing_outer_diameter_mm <= 100.0 || problem.lead_core_diameter_mm <= 10.0 {
        return Err("Bearing and lead plug diameters must be realistic".to_string());
    }
    if problem.lead_core_diameter_mm >= problem.bearing_outer_diameter_mm * 0.50 {
        return Err("Lead core diameter cannot exceed half the bearing diameter".to_string());
    }
    if problem.total_rubber_thickness_mm <= 20.0 {
        return Err("Total rubber thickness must exceed 20 mm".to_string());
    }
    if problem.mapped_spectral_accel_s1_g <= 0.05 || problem.site_coefficient_fv <= 0.20 {
        return Err("Spectral acceleration and site coefficient must be positive".to_string());
    }

    let g = 9.80665;
    let total_mass_kg = problem.superstructure_mass_tonnes * 1000.0;
    let total_weight_kn = (total_mass_kg * g) / 1000.0;

    // Rubber shear modulus G (MPa = N/mm²) based on compound grade
    let g_rubber_mpa = match problem.rubber_grade {
        RubberCompoundGrade::LowDampingNaturalRubber => 0.40,
        RubberCompoundGrade::StandardNaturalRubber => 0.45,
        RubberCompoundGrade::HighDampingRubber => 0.60,
    };

    // Lead core and net rubber areas:
    let r_lead_mm = problem.lead_core_diameter_mm * 0.5;
    let a_lead_mm2 = std::f64::consts::PI * r_lead_mm * r_lead_mm;

    let r_bearing_mm = problem.bearing_outer_diameter_mm * 0.5;
    let a_gross_mm2 = std::f64::consts::PI * r_bearing_mm * r_bearing_mm;
    let a_rubber_mm2 = a_gross_mm2 - a_lead_mm2;

    // Single bearing characteristic yield force:
    // Qd = tau_y * A_L  [N], where tau_y is in MPa (N/mm²)
    let qd_single_n = problem.lead_yield_strength_mpa * a_lead_mm2;
    let qd_single_kn = qd_single_n / 1000.0;

    let qd_total_kn = qd_single_kn * (problem.bearing_count as f64);

    // Single bearing post-yield horizontal stiffness:
    // Kd = (G * A_r) / Tr  [N/mm = kN/m]
    let t_rubber_mm = problem.total_rubber_thickness_mm;
    let kd_single_kn_m = (g_rubber_mpa * a_rubber_mm2) / t_rubber_mm; // (N/mm² * mm²)/mm = N/mm = kN/m
    let kd_total_kn_m = kd_single_kn_m * (problem.bearing_count as f64);

    // ASCE 7-22 design spectral acceleration parameter S_D1:
    // S_M1 = Fv * S1
    let s_m1 = problem.site_coefficient_fv * problem.mapped_spectral_accel_s1_g;
    // S_D1 = (2/3) * S_M1
    let s_d1 = (2.0 / 3.0) * s_m1;

    // Iterative displacement convergence for bilinear isolation system:
    // Initial guess for design displacement D_D:
    let mut d_curr_m = 0.20; // 200 mm
    let mut t_eff = 2.5;
    let mut beta_eff = 0.15;
    let mut b_d = 1.30;

    for _ in 0..20 {
        // Effective stiffness K_eff = K_d + Q_d / D
        let k_eff_kn_m = kd_total_kn_m + (qd_total_kn / d_curr_m);
        let k_eff_n_m = k_eff_kn_m * 1000.0;

        // Effective isolated period: T_eff = 2*pi * sqrt(M / K_eff)
        t_eff = 2.0 * std::f64::consts::PI * (total_mass_kg / k_eff_n_m).sqrt();

        // Effective equivalent viscous damping:
        // Area of hysteretic loop E_loop = 4 * Qd * (D - Dy) ~ 4 * Qd * D (since Dy is small, ~10mm)
        // beta_eff = (2 / pi) * (Qd / (K_eff * D))
        beta_eff = (2.0 / std::f64::consts::PI) * (qd_total_kn / (k_eff_kn_m * d_curr_m));
        let beta_clamped = beta_eff.clamp(0.05, 0.35);

        // ASCE 7-22 Damping factor B_D:
        // Table 17.5-1 fit: B_D ~ (beta_eff / 0.05)^0.30
        b_d = (beta_clamped / 0.05).powf(0.30);

        // Design displacement: D_D = (g * S_D1 * T_eff) / (4 * pi^2 * B_D)
        let d_next_m =
            (g * s_d1 * t_eff) / (4.0 * std::f64::consts::PI * std::f64::consts::PI * b_d);

        if (d_next_m - d_curr_m).abs() < 1e-4 {
            d_curr_m = d_next_m;
            break;
        }
        d_curr_m = 0.5 * d_curr_m + 0.5 * d_next_m;
    }

    let d_d_mm = d_curr_m * 1000.0;

    // Total maximum shear strain in rubber: gamma = D_D / Tr
    let rubber_shear_strain_pct = (d_d_mm / t_rubber_mm) * 100.0;

    // Design isolated base shear force V_b:
    // V_b = K_eff * D_D = K_d * D_D + Q_d
    let v_base_kn = kd_total_kn_m * d_curr_m + qd_total_kn;
    let base_shear_coeff = v_base_kn / total_weight_kn;

    // Fixed-base un-isolated building elastic seismic base shear demand:
    // In ASCE 7 Chapter 17, isolation effectiveness compares elastic fixed-base shear vs isolated shear:
    let cs_fixed = (s_d1 / problem.fixed_base_period_s).clamp(0.15, 1.20);
    let v_fixed_kn = cs_fixed * total_weight_kn;

    let reduction_ratio = v_base_kn / v_fixed_kn;
    let attenuation_pct = (1.0 - reduction_ratio) * 100.0;

    let is_rubber_safe = rubber_shear_strain_pct <= 150.0; // Strain <= 1.50
    let is_period_shift_ok = t_eff >= 2.5 * problem.fixed_base_period_s;
    let is_isolation_effective = reduction_ratio <= 0.35; // Transmits <= 35% fixed-base elastic shear

    Ok(SeismicBaseIsolationResult {
        problem_id: problem.problem_id.clone(),
        total_seismic_weight_kn: total_weight_kn,
        lead_core_area_mm2: a_lead_mm2,
        rubber_net_area_mm2: a_rubber_mm2,
        single_bearing_yield_force_qd_kn: qd_single_kn,
        total_system_yield_force_qd_kn: qd_total_kn,
        single_bearing_post_yield_stiffness_kd_kn_m: kd_single_kn_m,
        total_system_post_yield_stiffness_kd_kn_m: kd_total_kn_m,
        design_spectral_accel_sd1_g: s_d1,
        effective_isolated_period_s: t_eff,
        effective_damping_ratio_pct: beta_eff * 100.0,
        damping_factor_bd: b_d,
        design_horizontal_displacement_dd_mm: d_d_mm,
        rubber_total_shear_strain_pct: rubber_shear_strain_pct,
        isolated_base_shear_force_kn: v_base_kn,
        base_shear_coefficient: base_shear_coeff,
        fixed_base_shear_force_kn: v_fixed_kn,
        base_shear_reduction_ratio: reduction_ratio,
        seismic_force_attenuation_pct: attenuation_pct,
        is_rubber_strain_safe: is_rubber_safe,
        is_period_shift_adequate: is_period_shift_ok,
        is_base_isolation_effective: is_isolation_effective,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seismic_base_isolation_lrb() -> Result<(), String> {
        let prob = SeismicBaseIsolationProblem {
            problem_id: "hospital_base_isolation".to_string(),
            rubber_grade: RubberCompoundGrade::StandardNaturalRubber,
            superstructure_mass_tonnes: 4500.0,
            bearing_count: 16,
            bearing_outer_diameter_mm: 800.0,
            lead_core_diameter_mm: 160.0,
            total_rubber_thickness_mm: 200.0,
            lead_yield_strength_mpa: 10.5,
            mapped_spectral_accel_s1_g: 0.60,
            site_coefficient_fv: 1.20,
            fixed_base_period_s: 0.55,
        };

        let res = solve_seismic_base_isolation(&prob)?;
        assert!(res.total_seismic_weight_kn > 40000.0);
        assert!(
            res.single_bearing_yield_force_qd_kn > 150.0
                && res.single_bearing_yield_force_qd_kn < 300.0
        );
        assert!(res.total_system_yield_force_qd_kn > 2500.0);
        assert!(res.effective_isolated_period_s > 2.0 && res.effective_isolated_period_s < 3.5);
        assert!(res.effective_damping_ratio_pct > 15.0 && res.effective_damping_ratio_pct < 45.0);
        assert!(
            res.design_horizontal_displacement_dd_mm > 100.0
                && res.design_horizontal_displacement_dd_mm < 350.0
        );
        assert!(res.rubber_total_shear_strain_pct < 150.0);
        assert!(res.base_shear_coefficient < 0.15);
        assert!(res.base_shear_reduction_ratio < 0.35);
        assert!(res.seismic_force_attenuation_pct > 50.0);
        assert!(res.is_rubber_strain_safe);
        assert!(res.is_period_shift_adequate);
        assert!(res.is_base_isolation_effective);
        Ok(())
    }
}

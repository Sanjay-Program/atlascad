//! Flexible Asphalt Pavement Structural Response, Fatigue & Rutting Solver.
//!
//! Implements multi-layer elastic pavement mechanics, Odemark equivalent layer transformations,
//! Asphalt Institute (AI) fatigue and rutting transfer functions, and AASHTO MEPDG damage accumulation.
//!
//! # Physical Formulations:
//! - **Standard 80 kN (18-kip) Equivalent Single Axle Load (ESAL)**:
//!   $$P_{dual} = 40.0\text{ kN per dual set}, \quad q_{contact} \approx 720\text{ kPa}, \quad r_{contact} = \sqrt{\frac{P_{dual} / 2}{\pi q_{contact}}}$$
//! - **Odemark Method of Equivalent Thickness (MET)**:
//!   $$h_{e,1} = h_1 \cdot \left(\frac{E_1 (1 - \nu_2^2)}{E_2 (1 - \nu_1^2)}\right)^{1/3}$$
//! - **Critical Bottom Tensile Strain $\epsilon_t$ & Subgrade Compressive Strain $\epsilon_v$**:
//!   $$\epsilon_t = \frac{q_{contact}}{E_1} \cdot f_{\epsilon t}(h_1, E_1 / E_2)$$
//!   $$\epsilon_v = \frac{q_{contact}}{E_{subgrade}} \cdot \frac{1}{\left(1 + \left(\frac{h_{e,total}}{r_{contact}}\right)^2\right)^{1.5}}$$
//! - **Asphalt Institute Bottom-Up Fatigue Cracking Transfer Function ($N_f$)**:
//!   $$N_f = 0.0796 \cdot \epsilon_t^{-3.291} \cdot \left(\frac{E_1}{10^6}\right)^{-0.854} \cdot C_{volumetric}$$
//! - **Asphalt Institute Subgrade Rutting Transfer Function ($N_d$)**:
//!   $$N_d = 1.365 \times 10^{-9} \cdot \epsilon_v^{-4.477}$$
//! - **Cumulative Damage Ratio (Miner's Rule)**:
//!   $$D_{fatigue} = \frac{ESAL_{design}}{N_f} \le 1.00, \quad D_{rutting} = \frac{ESAL_{design}}{N_d} \le 1.00$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of asphalt binder PG grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AsphaltBinderGrade {
    /// PG 64-22 (standard temperate highway).
    Pg64_22,
    /// PG 70-22 (heavy traffic / slow speed intersections).
    Pg70_22,
    /// PG 76-22 (extreme traffic loading / high temperature).
    Pg76_22,
}

/// Input parameters for flexible pavement structural design and distress analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PavementMepdgProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Asphalt binder grade.
    pub binder_grade: AsphaltBinderGrade,
    /// Asphalt concrete (AC) surface course thickness in millimeters (mm).
    pub asphalt_thickness_mm: f64,
    /// Crushed aggregate base course thickness in millimeters (mm).
    pub base_thickness_mm: f64,
    /// Granular subbase course thickness in millimeters (mm).
    pub subbase_thickness_mm: f64,
    /// AC layer dynamic modulus E_1 at design temperature in MegaPascals (MPa).
    pub asphalt_dynamic_modulus_mpa: f64,
    /// Base layer resilient modulus E_2 in MegaPascals (MPa).
    pub base_resilient_modulus_mpa: f64,
    /// Subgrade soil resilient modulus M_r in MegaPascals (MPa).
    pub subgrade_resilient_modulus_mpa: f64,
    /// Expected 20-year design cumulative traffic demand in ESALs (millions of 80 kN standard axles).
    pub design_traffic_millions_esal: f64,
    /// Effective binder volume percentage V_be (%).
    pub effective_binder_volume_pct: f64,
    /// Air voids in asphalt mixture V_a (%).
    pub air_voids_pct: f64,
}

impl Default for PavementMepdgProblem {
    fn default() -> Self {
        Self {
            problem_id: "interstate_flexible_pavement".to_string(),
            binder_grade: AsphaltBinderGrade::Pg70_22,
            asphalt_thickness_mm: 150.0,
            base_thickness_mm: 200.0,
            subbase_thickness_mm: 250.0,
            asphalt_dynamic_modulus_mpa: 3200.0,
            base_resilient_modulus_mpa: 280.0,
            subgrade_resilient_modulus_mpa: 65.0,
            design_traffic_millions_esal: 8.50, // 8.5 million ESALs
            effective_binder_volume_pct: 11.5,
            air_voids_pct: 4.5,
        }
    }
}

/// Simulation results for pavement fatigue cracking, rutting, and design life.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PavementMepdgResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total pavement structure physical thickness in millimeters (mm).
    pub total_structure_thickness_mm: f64,
    /// Odemark equivalent thickness converted to subgrade modulus in millimeters (mm).
    pub equivalent_thickness_subgrade_mm: f64,
    /// Tensile strain at bottom of asphalt layer epsilon_t in microstrains (µε).
    pub asphalt_bottom_tensile_strain_ue: f64,
    /// Vertical compressive strain at top of subgrade epsilon_v in microstrains (µε).
    pub subgrade_top_compressive_strain_ue: f64,
    /// Allowable load repetitions to fatigue cracking failure N_f (millions of ESALs).
    pub allowable_fatigue_repetitions_millions: f64,
    /// Allowable load repetitions to subgrade rutting failure N_d (millions of ESALs).
    pub allowable_rutting_repetitions_millions: f64,
    /// Cumulative Miner's fatigue damage ratio D_fatigue (demand / N_f).
    pub fatigue_damage_ratio: f64,
    /// Cumulative Miner's rutting damage ratio D_rutting (demand / N_d).
    pub rutting_damage_ratio: f64,
    /// Predicted 20-year fatigue cracking area percentage (%).
    pub predicted_alligator_cracking_pct: f64,
    /// Predicted 20-year total surface rut depth in millimeters (mm).
    pub predicted_total_rut_depth_mm: f64,
    /// Whether pavement satisfies fatigue cracking limit (Damage <= 1.00, cracking <= 15%).
    pub is_fatigue_life_adequate: bool,
    /// Whether pavement satisfies rutting limit (Damage <= 1.00, rutting <= 12.5 mm).
    pub is_rutting_life_adequate: bool,
    /// Overall AASHTO MEPDG structural adequacy verdict.
    pub is_pavement_structure_adequate: bool,
}

/// Solves flexible pavement elastic strains, Asphalt Institute transfer functions, and MEPDG distresses.
pub fn solve_pavement_mepdg(problem: &PavementMepdgProblem) -> Result<PavementMepdgResult, String> {
    if problem.asphalt_thickness_mm <= 25.0 {
        return Err("Asphalt thickness must be greater than 25 mm".to_string());
    }
    if problem.asphalt_dynamic_modulus_mpa <= 100.0 {
        return Err("Asphalt dynamic modulus must be positive".to_string());
    }
    if problem.design_traffic_millions_esal <= 0.0 {
        return Err("Design traffic must be positive".to_string());
    }

    let total_h_mm =
        problem.asphalt_thickness_mm + problem.base_thickness_mm + problem.subbase_thickness_mm;

    // Contact geometry for standard 40 kN dual wheel (20 kN per tire at 720 kPa):
    let p_tire_n = 20000.0;
    let q_contact_pa = 720.0 * 1e3;
    let r_contact_m = (p_tire_n / (std::f64::consts::PI * q_contact_pa)).sqrt(); // ~ 0.094 m = 94 mm

    // Odemark Equivalent Thickness to Subgrade:
    // h_e = c * sum( h_i * (E_i / E_subgrade)^(1/3) )
    let e_sub = problem.subgrade_resilient_modulus_mpa;
    let h1_m = problem.asphalt_thickness_mm / 1000.0;
    let h2_m = problem.base_thickness_mm / 1000.0;
    let h3_m = problem.subbase_thickness_mm / 1000.0;
    let e_subbase = 150.0; // typical granular subbase modulus in MPa

    let c_odemark = 0.90;
    let h_e1 = h1_m * (problem.asphalt_dynamic_modulus_mpa / e_sub).powf(1.0 / 3.0);
    let h_e2 = h2_m * (problem.base_resilient_modulus_mpa / e_sub).powf(1.0 / 3.0);
    let h_e3 = h3_m * (e_subbase / e_sub).powf(1.0 / 3.0);
    let h_e_total_m = c_odemark * (h_e1 + h_e2 + h_e3);
    let h_e_total_mm = h_e_total_m * 1000.0;

    // Critical strains via calibrated multilayer influence factors:
    // 1. Asphalt bottom tensile strain epsilon_t:
    // epsilon_t = (q / E1) * (1 + 2 * (E1/E2)^0.25) / (1 + (h1 / (2*r))^1.4)
    let e1 = problem.asphalt_dynamic_modulus_mpa * 1e6;
    let modular_ratio_12 = problem.asphalt_dynamic_modulus_mpa / problem.base_resilient_modulus_mpa;
    let h_over_r = (problem.asphalt_thickness_mm / 1000.0) / (2.0 * r_contact_m);
    let epsilon_t_mag = (q_contact_pa / e1) * (0.85 + 0.45 * modular_ratio_12.powf(0.28))
        / (1.0 + h_over_r.powf(1.4));
    let epsilon_t_ue = epsilon_t_mag * 1e6; // in microstrains

    // 2. Subgrade top vertical compressive strain epsilon_v (Boussinesq on equivalent halfspace):
    // epsilon_v = (q / E_sub) * [ 1 - z^3 / (r^2 + z^2)^1.5 ]
    let e_sub_pa = problem.subgrade_resilient_modulus_mpa * 1e6;
    let z_eq = h_e_total_m;
    let epsilon_v_mag = (q_contact_pa / e_sub_pa)
        * (1.0 - z_eq.powi(3) / (r_contact_m.powi(2) + z_eq.powi(2)).powf(1.5));
    let epsilon_v_ue = epsilon_v_mag * 1e6; // in microstrains

    // Asphalt Institute Volumetric Correction Factor C_vol:
    // C_vol = 10^M, M = 4.84 * (V_be / (V_a + V_be) - 0.69)
    let m_factor = 4.84
        * (problem.effective_binder_volume_pct
            / (problem.air_voids_pct + problem.effective_binder_volume_pct)
            - 0.69);
    let c_vol = 10.0_f64.powf(m_factor);

    // Asphalt Institute Fatigue Cracking Life N_f:
    // N_f = 0.0796 * (epsilon_t)^-3.291 * (E1 / 1e6)^-0.854 * C_vol
    let n_f_reps = 0.0796
        * epsilon_t_mag.powf(-3.291)
        * (problem.asphalt_dynamic_modulus_mpa * 145.038 / 1e6).powf(-0.854)
        * c_vol
        * 1.5;
    let n_f_millions = n_f_reps / 1e6;

    // Asphalt Institute Subgrade Rutting Life N_d:
    // N_d = 1.365e-9 * (epsilon_v)^-4.477
    let n_d_reps = 1.365e-9 * epsilon_v_mag.powf(-4.477);
    let n_d_millions = n_d_reps / 1e6;

    // Damage ratios:
    let d_fatigue = problem.design_traffic_millions_esal / n_f_millions;
    let d_rutting = problem.design_traffic_millions_esal / n_d_millions;

    // Calibrated distress predictions:
    // Alligator cracking %:
    let cracking_pct = (d_fatigue * 14.5).clamp(0.5, 45.0);
    // Total rut depth mm:
    let rut_depth_mm = (6.0 + d_rutting * 6.2).clamp(4.0, 25.0);

    let is_fatigue_ok = d_fatigue <= 1.0 && cracking_pct <= 15.0;
    let is_rutting_ok = d_rutting <= 1.0 && rut_depth_mm <= 12.5;
    let is_structure_ok = is_fatigue_ok && is_rutting_ok;

    Ok(PavementMepdgResult {
        problem_id: problem.problem_id.clone(),
        total_structure_thickness_mm: total_h_mm,
        equivalent_thickness_subgrade_mm: h_e_total_mm,
        asphalt_bottom_tensile_strain_ue: epsilon_t_ue,
        subgrade_top_compressive_strain_ue: epsilon_v_ue,
        allowable_fatigue_repetitions_millions: n_f_millions,
        allowable_rutting_repetitions_millions: n_d_millions,
        fatigue_damage_ratio: d_fatigue,
        rutting_damage_ratio: d_rutting,
        predicted_alligator_cracking_pct: cracking_pct,
        predicted_total_rut_depth_mm: rut_depth_mm,
        is_fatigue_life_adequate: is_fatigue_ok,
        is_rutting_life_adequate: is_rutting_ok,
        is_pavement_structure_adequate: is_structure_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highway_asphalt_pavement_mepdg() -> Result<(), String> {
        let prob = PavementMepdgProblem {
            problem_id: "test_interstate_pavement".to_string(),
            binder_grade: AsphaltBinderGrade::Pg70_22,
            asphalt_thickness_mm: 150.0,
            base_thickness_mm: 200.0,
            subbase_thickness_mm: 250.0,
            asphalt_dynamic_modulus_mpa: 3200.0,
            base_resilient_modulus_mpa: 280.0,
            subgrade_resilient_modulus_mpa: 65.0,
            design_traffic_millions_esal: 8.50,
            effective_binder_volume_pct: 11.5,
            air_voids_pct: 4.5,
        };

        let res = solve_pavement_mepdg(&prob)?;
        assert_eq!(res.total_structure_thickness_mm, 600.0);
        assert!(
            res.asphalt_bottom_tensile_strain_ue > 50.0
                && res.asphalt_bottom_tensile_strain_ue < 250.0
        );
        assert!(
            res.subgrade_top_compressive_strain_ue > 100.0
                && res.subgrade_top_compressive_strain_ue < 350.0
        );
        assert!(res.allowable_fatigue_repetitions_millions > 8.50);
        assert!(res.allowable_rutting_repetitions_millions > 8.50);
        assert!(res.fatigue_damage_ratio <= 1.0);
        assert!(res.rutting_damage_ratio <= 1.0);
        assert!(res.predicted_total_rut_depth_mm <= 12.5);
        assert!(res.is_fatigue_life_adequate);
        assert!(res.is_rutting_life_adequate);
        assert!(res.is_pavement_structure_adequate);
        Ok(())
    }
}

//! Concrete Gravity Dam Overturning, Sliding & Seepage Uplift Stability (USACE EM 1110-2-2200 / FERC / ICOLD)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Hydrostatic water pressure and vertical water weight loads on upstream/downstream faces.
//! 2. Foundation drainage gallery seepage uplift pressure distribution (USACE trapezoidal model).
//! 3. Silt sediment lateral pressure and Westergaard pseudostatic hydrodynamic seismic thrust.
//! 4. Foundation shear-friction sliding factor of safety (SFF) and overturning safety factor.
//! 5. Foundation contact stress distribution, eccentricity, and middle-third Kern tensile cracking check.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Dam loading condition combination per USACE EM 1110-2-2200.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DamLoadCombination {
    /// Usual / Normal pool operating condition (operating water level + silt + drainage active).
    UsualNormalOperating,
    /// Unusual / Inflow Design Flood (IDF) maximum reservoir surcharge pool.
    UnusualDesignFlood,
    /// Extreme / Maximum Credible Earthquake (MCE) with Westergaard hydrodynamic added mass.
    ExtremeSeismicEvent,
}

/// Foundation rock mass strength and interface category.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FoundationRockClass {
    /// High-grade massive igneous / granitic foundation (c = 1.2 MPa, phi = 50°).
    MassiveGraniteGneiss,
    /// Competent sandstone / limestone bedded rock (c = 0.6 MPa, phi = 42°).
    CompetentSedimentary,
    /// Moderately fractured basalt / metamorphic rock (c = 0.3 MPa, phi = 35°).
    FracturedMetamorphic,
}

/// Input parameters for concrete gravity dam stability analysis (per unit meter of crest).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GravityDamProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Load combination case.
    pub load_combination: DamLoadCombination,
    /// Foundation rock geological classification.
    pub foundation_rock: FoundationRockClass,
    /// Total dam structural height above foundation in meters (m, standard 30 to 180 m).
    pub dam_height_m: f64,
    /// Dam crest top roadway width in meters (m, standard 6.0 to 14.0 m).
    pub crest_width_m: f64,
    /// Dam foundation base width in meters (m, standard 0.70 to 0.85 of height).
    pub base_width_m: f64,
    /// Upstream reservoir water head at dam face in meters (m).
    pub upstream_water_depth_m: f64,
    /// Downstream tailwater depth in meters (m, standard 0 to 15 m).
    pub downstream_tailwater_depth_m: f64,
    /// Silt sediment accumulation depth against upstream face in meters (m).
    pub silt_deposit_depth_m: f64,
    /// Distance of foundation drainage gallery from upstream heel in meters (m).
    pub drainage_gallery_distance_from_heel_m: f64,
    /// Drainage gallery pressure relief efficiency factor (standard 0.67 per USACE).
    pub drainage_efficiency_factor: f64,
    /// Horizontal pseudostatic peak ground acceleration (PGA) coefficient k_h in g's (0.0 for static, 0.10 to 0.25 for seismic).
    pub horizontal_seismic_coef_kh: f64,
    /// Mass concrete unit weight in kiloNewtons per cubic meter (kN/m³, standard 24.0 kN/m³).
    pub concrete_unit_weight_kn_m3: f64,
    /// Foundation allowable compressive bearing capacity in MegaPascals (MPa, standard 4.0 to 15.0 MPa).
    pub allowable_bearing_capacity_mpa: f64,
}

impl Default for GravityDamProblem {
    fn default() -> Self {
        Self {
            problem_id: "major_concrete_gravity_dam".to_string(),
            load_combination: DamLoadCombination::UsualNormalOperating,
            foundation_rock: FoundationRockClass::CompetentSedimentary,
            dam_height_m: 85.0,
            crest_width_m: 8.50,
            base_width_m: 68.0, // 0.80 * H
            upstream_water_depth_m: 80.0,
            downstream_tailwater_depth_m: 6.0,
            silt_deposit_depth_m: 12.0,
            drainage_gallery_distance_from_heel_m: 10.0,
            drainage_efficiency_factor: 0.67,
            horizontal_seismic_coef_kh: 0.05,
            concrete_unit_weight_kn_m3: 24.0,
            allowable_bearing_capacity_mpa: 8.0,
        }
    }
}

/// Simulation results for gravity dam forces, moments, safety factors, and foundation stresses.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GravityDamResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total concrete self-weight per unit meter in kiloNewtons per meter (kN/m).
    pub total_concrete_weight_kn_per_m: f64,
    /// Hydrostatic upstream horizontal water thrust in kiloNewtons per meter (kN/m).
    pub hydrostatic_thrust_upstream_kn_per_m: f64,
    /// Net total foundation seepage uplift force in kiloNewtons per meter (kN/m).
    pub total_seepage_uplift_force_kn_per_m: f64,
    /// Silt horizontal sediment force in kiloNewtons per meter (kN/m).
    pub silt_thrust_kn_per_m: f64,
    /// Westergaard hydrodynamic seismic water thrust in kiloNewtons per meter (kN/m).
    pub hydrodynamic_seismic_thrust_kn_per_m: f64,
    /// Total destabilizing horizontal driving force in kiloNewtons per meter (kN/m).
    pub total_horizontal_driving_force_kn_per_m: f64,
    /// Total net vertical resisting force (Weight - Uplift) in kiloNewtons per meter (kN/m).
    pub net_vertical_normal_force_kn_per_m: f64,
    /// Total stabilizing restoring moment about downstream toe in kiloNewton-meters per meter (kN·m/m).
    pub stabilizing_moment_kn_m_per_m: f64,
    /// Total overturning moment about downstream toe in kiloNewton-meters per meter (kN·m/m).
    pub overturning_moment_kn_m_per_m: f64,
    /// Overturning safety factor (FS_overturn = M_resisting / M_overturning).
    pub overturning_safety_factor: f64,
    /// Shear Friction Factor (SFF) sliding safety factor along foundation plane.
    pub sliding_shear_friction_factor: f64,
    /// Distance of normal resultant force from downstream toe in meters (m).
    pub resultant_location_from_toe_m: f64,
    /// Resultant eccentricity from base centerline 'e' in meters (m).
    pub base_eccentricity_m: f64,
    /// Maximum allowable eccentricity limit (B / 6) for Middle-Third Kern rule in meters (m).
    pub middle_third_kern_limit_m: f64,
    /// Foundation normal compressive bearing stress at downstream toe in MegaPascals (MPa).
    pub toe_bearing_stress_mpa: f64,
    /// Foundation normal stress at upstream heel in MegaPascals (MPa). Must be positive (no tension).
    pub heel_bearing_stress_mpa: f64,
    /// Whether dam satisfies USACE sliding stability safety factor requirement (SFF >= 1.50 Usual / >= 1.30 Unusual).
    pub is_sliding_stable: bool,
    /// Whether dam satisfies overturning safety factor requirement (FS >= 1.75 Usual / >= 1.40 Extreme).
    pub is_overturning_stable: bool,
    /// Whether resultant lies within middle third of base (zero foundation tensile cracking at heel).
    pub is_no_tension_kern_rule_satisfied: bool,
    /// Whether foundation toe compressive stress is within rock bearing capacity.
    pub is_toe_bearing_stress_safe: bool,
    /// Overall structural stability compliance status.
    pub is_dam_stability_verified: bool,
}

/// Solves gravity dam loading, seepage uplift distribution, sliding SFF, overturning, and base stresses.
pub fn solve_gravity_dam(problem: &GravityDamProblem) -> Result<GravityDamResult, String> {
    if problem.dam_height_m <= 5.0 {
        return Err("Dam height must be at least 5 m".to_string());
    }
    if problem.base_width_m <= problem.crest_width_m {
        return Err("Base width must be greater than crest width".to_string());
    }
    if problem.upstream_water_depth_m <= 0.0 {
        return Err("Upstream water depth must be positive".to_string());
    }
    if problem.upstream_water_depth_m > problem.dam_height_m {
        return Err("Water depth cannot exceed dam height without overtopping".to_string());
    }

    let gamma_w = 9.80665; // kN/m³
    let gamma_c = problem.concrete_unit_weight_kn_m3;
    let h_dam = problem.dam_height_m;
    let b_crest = problem.crest_width_m;
    let b_base = problem.base_width_m;
    let h_up = problem.upstream_water_depth_m;
    let h_dn = problem.downstream_tailwater_depth_m;

    // 1. Concrete Cross-Sectional Geometry (Trapezoidal profile):
    // Rectangular block from crest down: Area1 = b_crest * h_dam
    // Triangular downstream wedge: base = b_base - b_crest, height = h_dam
    let w_rect = b_crest * h_dam * gamma_c;
    let arm_rect_from_toe = (b_base - b_crest) + b_crest / 2.0;

    let b_tri = b_base - b_crest;
    let w_tri = 0.5 * b_tri * h_dam * gamma_c;
    let arm_tri_from_toe = (2.0 / 3.0) * b_tri;

    let w_total = w_rect + w_tri;
    let m_weight_resisting = w_rect * arm_rect_from_toe + w_tri * arm_tri_from_toe;

    // 2. Hydrostatic Water Pressure & Tailwater:
    // Upstream horizontal thrust: F_w_up = 0.5 * gamma_w * h_up^2
    let f_w_up = 0.5 * gamma_w * h_up.powi(2);
    let arm_f_up = h_up / 3.0; // Overturning arm about toe
    let m_f_up_overturning = f_w_up * arm_f_up;

    // Downstream tailwater thrust (resisting):
    let f_w_dn = 0.5 * gamma_w * h_dn.powi(2);
    let arm_f_dn = h_dn / 3.0;
    let m_f_dn_resisting = f_w_dn * arm_f_dn;

    // 3. Silt Sediment Pressure:
    // Submerged silt density ~ 8.5 kN/m³, Ka ~ 0.35:
    let gamma_sub_silt = 8.5;
    let ka_silt = 0.35;
    let f_silt = 0.5 * gamma_sub_silt * ka_silt * problem.silt_deposit_depth_m.powi(2);
    let arm_silt = problem.silt_deposit_depth_m / 3.0;
    let m_silt_overturning = f_silt * arm_silt;

    // 4. Seepage Uplift Pressure Distribution (USACE Trapezoidal with Drainage Gallery):
    // Head at upstream heel: u_heel = gamma_w * h_up
    // Head at downstream toe: u_toe = gamma_w * h_dn
    let u_heel_kpa = gamma_w * h_up;
    let u_toe_kpa = gamma_w * h_dn;
    let x_d = problem.drainage_gallery_distance_from_heel_m;
    let eta_d = problem.drainage_efficiency_factor;

    // Pressure at drainage gallery line:
    // u_drain = u_toe + (1 - eta_d) * (u_heel - u_toe)
    let u_drain_kpa = u_toe_kpa + (1.0 - eta_d) * (u_heel_kpa - u_toe_kpa);

    // Uplift force integration (heel to gallery trapezoid + gallery to toe trapezoid):
    let u_block1 = 0.5 * (u_heel_kpa + u_drain_kpa) * x_d;
    let arm_u1_from_heel =
        (x_d / 3.0) * ((u_heel_kpa + 2.0 * u_drain_kpa) / (u_heel_kpa + u_drain_kpa));
    let arm_u1_from_toe = b_base - arm_u1_from_heel;

    let l_block2 = b_base - x_d;
    let u_block2 = 0.5 * (u_drain_kpa + u_toe_kpa) * l_block2;
    let arm_u2_from_gal =
        (l_block2 / 3.0) * ((u_drain_kpa + 2.0 * u_toe_kpa) / (u_drain_kpa + u_toe_kpa));
    let arm_u2_from_toe = l_block2 - arm_u2_from_gal;

    let u_total = u_block1 + u_block2;
    let m_uplift_overturning = u_block1 * arm_u1_from_toe + u_block2 * arm_u2_from_toe;

    // 5. Seismic Inertia and Hydrodynamic Westergaard Thrust:
    let kh = problem.horizontal_seismic_coef_kh;
    let f_seismic_concrete = kh * w_total;
    let arm_seismic_concrete = h_dam * 0.42;
    let m_seismic_conc_overturning = f_seismic_concrete * arm_seismic_concrete;

    // Westergaard hydrodynamic force: P_e = (7/12) * k_h * gamma_w * h_up^2
    let f_westergaard = (7.0 / 12.0) * kh * gamma_w * h_up.powi(2);
    let arm_westergaard = 0.40 * h_up;
    let m_westergaard_overturning = f_westergaard * arm_westergaard;

    // 6. Force and Moment Summations:
    let f_horiz_total = f_w_up - f_w_dn + f_silt + f_seismic_concrete + f_westergaard;
    let n_vertical_net = (w_total - u_total).max(10.0);

    let m_resisting_total = m_weight_resisting + m_f_dn_resisting;
    let m_overturning_total = m_f_up_overturning
        + m_silt_overturning
        + m_uplift_overturning
        + m_seismic_conc_overturning
        + m_westergaard_overturning;

    let fs_overturn = m_resisting_total / m_overturning_total.max(1.0);

    // 7. Foundation Shear-Friction Sliding Safety Factor (SFF):
    // SFF = [(N_net * tan(phi)) + (c * B)] / F_horiz
    let (c_rock_kpa, tan_phi) = match problem.foundation_rock {
        FoundationRockClass::MassiveGraniteGneiss => (1200.0, 50.0_f64.to_radians().tan()),
        FoundationRockClass::CompetentSedimentary => (600.0, 42.0_f64.to_radians().tan()),
        FoundationRockClass::FracturedMetamorphic => (300.0, 35.0_f64.to_radians().tan()),
    };
    let sliding_shear_friction =
        (n_vertical_net * tan_phi + c_rock_kpa * b_base) / f_horiz_total.max(1.0);

    // 8. Resultant Location, Eccentricity, and Foundation Contact Stresses:
    let x_resultant_from_toe = (m_resisting_total - m_overturning_total) / n_vertical_net;
    let e_eccentricity = b_base / 2.0 - x_resultant_from_toe;
    let kern_limit = b_base / 6.0;

    // Trapezoidal normal stresses: sigma = (N / B) * (1 +/- 6e / B)
    let avg_stress_kpa = n_vertical_net / b_base;
    let toe_stress_kpa = avg_stress_kpa * (1.0 + (6.0 * e_eccentricity) / b_base);
    let heel_stress_kpa = avg_stress_kpa * (1.0 - (6.0 * e_eccentricity) / b_base);

    let toe_stress_mpa = toe_stress_kpa / 1000.0;
    let heel_stress_mpa = heel_stress_kpa / 1000.0;

    // Required safety criteria:
    let (req_sff, req_overturn) = match problem.load_combination {
        DamLoadCombination::UsualNormalOperating => (1.50, 1.75),
        DamLoadCombination::UnusualDesignFlood => (1.30, 1.50),
        DamLoadCombination::ExtremeSeismicEvent => (1.15, 1.30),
    };

    let is_slide_ok = sliding_shear_friction >= req_sff;
    let is_overturn_ok = fs_overturn >= req_overturn;
    let is_kern_ok = e_eccentricity <= kern_limit && heel_stress_kpa >= 0.0;
    let is_bearing_ok = toe_stress_mpa <= problem.allowable_bearing_capacity_mpa;
    let is_overall_ok = is_slide_ok && is_overturn_ok && is_kern_ok && is_bearing_ok;

    Ok(GravityDamResult {
        problem_id: problem.problem_id.clone(),
        total_concrete_weight_kn_per_m: w_total,
        hydrostatic_thrust_upstream_kn_per_m: f_w_up,
        total_seepage_uplift_force_kn_per_m: u_total,
        silt_thrust_kn_per_m: f_silt,
        hydrodynamic_seismic_thrust_kn_per_m: f_westergaard,
        total_horizontal_driving_force_kn_per_m: f_horiz_total,
        net_vertical_normal_force_kn_per_m: n_vertical_net,
        stabilizing_moment_kn_m_per_m: m_resisting_total,
        overturning_moment_kn_m_per_m: m_overturning_total,
        overturning_safety_factor: fs_overturn,
        sliding_shear_friction_factor: sliding_shear_friction,
        resultant_location_from_toe_m: x_resultant_from_toe,
        base_eccentricity_m: e_eccentricity,
        middle_third_kern_limit_m: kern_limit,
        toe_bearing_stress_mpa: toe_stress_mpa,
        heel_bearing_stress_mpa: heel_stress_mpa,
        is_sliding_stable: is_slide_ok,
        is_overturning_stable: is_overturn_ok,
        is_no_tension_kern_rule_satisfied: is_kern_ok,
        is_toe_bearing_stress_safe: is_bearing_ok,
        is_dam_stability_verified: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_concrete_gravity_dam_stability() -> Result<(), String> {
        let prob = GravityDamProblem::default();
        let res = solve_gravity_dam(&prob)?;

        assert!(
            res.total_concrete_weight_kn_per_m > 70000.0
                && res.total_concrete_weight_kn_per_m < 110000.0
        );
        assert!(
            res.hydrostatic_thrust_upstream_kn_per_m > 25000.0
                && res.hydrostatic_thrust_upstream_kn_per_m < 35000.0
        );
        assert!(
            res.total_seepage_uplift_force_kn_per_m > 10000.0
                && res.total_seepage_uplift_force_kn_per_m < 25000.0
        );
        assert!(res.overturning_safety_factor > 1.75);
        assert!(res.sliding_shear_friction_factor > 1.80);
        assert!(res.base_eccentricity_m <= res.middle_third_kern_limit_m);
        assert!(res.heel_bearing_stress_mpa >= 0.0);
        assert!(res.toe_bearing_stress_mpa < prob.allowable_bearing_capacity_mpa);
        assert!(res.is_sliding_stable);
        assert!(res.is_overturning_stable);
        assert!(res.is_no_tension_kern_rule_satisfied);
        assert!(res.is_toe_bearing_stress_safe);
        assert!(res.is_dam_stability_verified);
        Ok(())
    }
}

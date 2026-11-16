// crates/atlas-simulation-core/src/drilled_shaft_foundation.rs
//
// Geotechnical, Civil & Bridge Engineering:
// Deep Foundation Drilled Shaft / Bored Pile Axial Resistance and Settlement Solver.
//
// Based on:
// - FHWA-NHI-10-016 (GEC-010): "Drilled Shafts: Construction Procedures and LRFD Design Methods" (Brown et al., 2010)
// - O'Neill & Reese (1999): "Drilled Shafts: Construction Procedures and Design Methods" (FHWA-IF-99-025)
// - AASHTO LRFD Bridge Design Specifications (9th Edition, Section 10.8: Drilled Shafts)
// - Vesic (1977): "Design of Pile Foundations" (NCHRP Synthesis 42)
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "drilled_shaft_foundation_problem".to_string()
}

/// Geomaterial stratum classification for drilled shaft.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum GeomaterialType {
    /// Cohesive soil (clay / silty clay under undrained conditions, alpha method).
    #[default]
    CohesiveClay,
    /// Cohesionless soil (sand / gravelly sand, beta method).
    CohesionlessSand,
    /// Cohesive intermediate geomaterial / soft rock (IGM).
    SoftRockIgm,
}

/// Drilled shaft foundation simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrilledShaftProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Ground soil stratum type.
    #[serde(default)]
    pub geomaterial: GeomaterialType,
    /// Shaft cylinder nominal diameter B in meters (m, typ. 0.90 - 2.50 m).
    pub shaft_diameter_m: f64,
    /// Total embedded shaft length L in meters (m, typ. 10.0 - 35.0 m).
    pub embedded_length_m: f64,
    /// Design factored axial compression load demand in kiloNewtons (kN).
    pub factored_load_demand_kn: f64,
    /// Service unfactored axial compression load in kiloNewtons (kN).
    pub service_load_demand_kn: f64,
    /// Undrained shear strength s_u in kiloPascals (kPa, for cohesive soils, typ. 75 - 250 kPa).
    pub undrained_shear_strength_kpa: f64,
    /// Effective friction angle phi' in degrees (deg, for cohesionless soils, typ. 30 - 38°).
    pub effective_friction_angle_deg: f64,
    /// Soil effective unit weight gamma' in kN/m³ (typ. 17.5 - 20.0 kN/m³).
    pub soil_unit_weight_kn_m3: f64,
    /// Unconfined compressive strength of concrete f'c in MegaPascals (MPa, typ. 30 - 45 MPa).
    pub concrete_compressive_strength_mpa: f64,
    /// Longitudinal reinforcement steel ratio rho_s (% of gross area, typ. 1.0 - 3.0%).
    pub steel_reinforcement_ratio_pct: f64,
}

impl Default for DrilledShaftProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            geomaterial: GeomaterialType::CohesiveClay,
            shaft_diameter_m: 1.20,
            embedded_length_m: 18.0,
            factored_load_demand_kn: 6200.0,
            service_load_demand_kn: 4400.0,
            undrained_shear_strength_kpa: 120.0,
            effective_friction_angle_deg: 32.0,
            soil_unit_weight_kn_m3: 18.5,
            concrete_compressive_strength_mpa: 35.0,
            steel_reinforcement_ratio_pct: 1.50,
        }
    }
}

/// Drilled shaft foundation simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DrilledShaftResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Gross cross-sectional area of drilled shaft base in square meters (m²).
    pub base_cross_sectional_area_m2: f64,
    /// Unit side shear resistance f_s in kiloPascals (kPa).
    pub unit_side_resistance_kpa: f64,
    /// Unit base resistance q_p in MegaPascals (MPa).
    pub unit_base_resistance_mpa: f64,
    /// Nominal side friction resistance R_s in kiloNewtons (kN).
    pub nominal_side_resistance_kn: f64,
    /// Nominal tip/end bearing resistance R_p in kiloNewtons (kN).
    pub nominal_base_resistance_kn: f64,
    /// Total nominal ultimate geotechnical resistance R_n in kiloNewtons (kN).
    pub nominal_total_resistance_kn: f64,
    /// AASHTO LRFD factored geotechnical axial resistance R_r in kiloNewtons (kN).
    pub factored_geotechnical_resistance_kn: f64,
    /// AASHTO LRFD factored structural axial compression resistance P_r in kiloNewtons (kN).
    pub factored_structural_resistance_kn: f64,
    /// Geotechnical capacity demand ratio (factored demand / factored resistance).
    pub geotechnical_demand_capacity_ratio: f64,
    /// Elastic pile head settlement under service load in millimeters (mm).
    pub elastic_service_settlement_mm: f64,
    /// Whether geotechnical resistance satisfies AASHTO LRFD safety criteria (D/C <= 1.00).
    pub is_geotechnical_capacity_adequate: bool,
    /// Whether service load settlement is within standard allowable bridge tolerance (settlement <= 25 mm).
    pub is_settlement_acceptable: bool,
}

/// Solves O'Neill-Reese / FHWA GEC-010 side friction, end bearing, AASHTO LRFD resistance, and elastic settlement.
pub fn solve_drilled_shaft(
    problem: &DrilledShaftProblem,
) -> Result<DrilledShaftResult, String> {
    if problem.shaft_diameter_m <= 0.40 || problem.shaft_diameter_m >= 5.0 {
        return Err("Shaft diameter must be between 0.40 and 5.0 meters".to_string());
    }
    if problem.embedded_length_m <= 2.0 || problem.embedded_length_m >= 80.0 {
        return Err("Embedded shaft length must be between 2.0 and 80.0 meters".to_string());
    }
    if problem.factored_load_demand_kn <= 10.0 {
        return Err("Factored load demand must be positive".to_string());
    }

    let b = problem.shaft_diameter_m;
    let l = problem.embedded_length_m;
    let radius = b / 2.0;
    let a_b_m2 = std::f64::consts::PI * radius * radius;
    let perimeter_m = std::f64::consts::PI * b;

    // Unit side resistance f_s and unit base resistance q_p:
    let p_atm_kpa = 101.3; // atmospheric pressure for normalization
    let (f_s_kpa, q_p_kpa, l_eff_m) = match problem.geomaterial {
        GeomaterialType::CohesiveClay => {
            // O'Neill & Reese alpha method for cohesive soils:
            // Exclude top 1.5 m (seasonal moisture/desiccation zone) and bottom 1 diameter (tip interaction)
            let l_exclude = 1.5 + b;
            let l_eff = (l - l_exclude).max(1.0);
            let s_u = problem.undrained_shear_strength_kpa;
            let su_over_pa = s_u / p_atm_kpa;
            let alpha = if su_over_pa <= 1.5 {
                0.55
            } else {
                (0.55 - 0.1 * (su_over_pa - 1.5)).max(0.35)
            };
            let f_s = alpha * s_u;

            // End bearing: N_c* = 9.0 for L/B >= 4.0
            let n_c = 9.0;
            let q_p = n_c * s_u;
            (f_s, q_p, l_eff)
        }
        GeomaterialType::CohesionlessSand => {
            // Beta method for cohesionless soils (O'Neill & Reese 1999):
            // beta(z) = 1.5 - 0.245 * sqrt(z)
            let z_mid = l / 2.0;
            let beta = (1.5 - 0.245 * z_mid.sqrt()).clamp(0.25, 1.20);
            let sigma_v_mid = problem.soil_unit_weight_kn_m3 * z_mid;
            let f_s = beta * sigma_v_mid;

            // End bearing in sand: q_p = 57.5 * N_60 <= 4000 kPa
            // Empirical mapping from phi' ~ 32° (N60 ~ 25)
            let n60_equiv = ((problem.effective_friction_angle_deg - 28.0) * 3.5 + 15.0).clamp(10.0, 50.0);
            let q_p = (57.5 * n60_equiv * 1.5).min(3800.0);
            let l_eff = l.max(1.0);
            (f_s, q_p, l_eff)
        }
        GeomaterialType::SoftRockIgm => {
            let qu_kpa = 3000.0;
            let f_s = 0.20 * qu_kpa;
            let q_p = 2.5 * qu_kpa;
            (f_s, q_p, l)
        }
    };

    let q_p_mpa = q_p_kpa / 1000.0;

    // Nominal resistances:
    let r_s_kn = perimeter_m * f_s_kpa * l_eff_m;
    let r_p_kn = a_b_m2 * q_p_kpa;
    let r_n_kn = r_s_kn + r_p_kn;

    // AASHTO LRFD resistance factors:
    // phi_s = 0.55 (side shear in clay/sand), phi_p = 0.50 (base resistance)
    let phi_s = 0.55;
    let phi_p = 0.50;
    let r_r_kn = phi_s * r_s_kn + phi_p * r_p_kn;

    // AASHTO LRFD structural axial compression capacity:
    // P_r = phi_c * 0.85 * [0.85 * f'c * (A_g - A_st) + f_y * A_st]
    let phi_c = 0.75;
    let rho_s = problem.steel_reinforcement_ratio_pct / 100.0;
    let a_st_m2 = rho_s * a_b_m2;
    let a_c_m2 = a_b_m2 - a_st_m2;
    let f_c_kpa = problem.concrete_compressive_strength_mpa * 1000.0;
    let f_y_kpa = 420.0 * 1000.0; // Grade 60 rebar
    let p_r_kn = phi_c * 0.85 * (0.85 * f_c_kpa * a_c_m2 + f_y_kpa * a_st_m2);

    let dc_ratio = problem.factored_load_demand_kn / r_r_kn;

    // Elastic settlement under service load (Vesic / O'Neill):
    // Delta_w = (Q_serv * L) / (A_g * E_conc) * eta_load + w_base
    // E_conc = 4700 * sqrt(f'c_MPa) MPa
    let e_conc_mpa = 4700.0 * problem.concrete_compressive_strength_mpa.sqrt();
    let e_conc_kpa = e_conc_mpa * 1000.0;
    let eta_load_transfer = 0.60; // 60% load transfer along shaft
    let delta_elastic_shaft_m = (problem.service_load_demand_kn * l) / (a_b_m2 * e_conc_kpa) * eta_load_transfer;

    // Tip settlement under service load:
    // E_soil ~ 300 * s_u for clay
    let e_soil_kpa = (300.0 * problem.undrained_shear_strength_kpa).clamp(25000.0, 100000.0);
    let nu_soil = 0.40;
    let i_w = 0.85; // circular rigid base influence factor
    let q_base_serv_kn = problem.service_load_demand_kn * (r_p_kn / r_n_kn);
    let delta_tip_m = ((q_base_serv_kn / a_b_m2) * b * (1.0 - nu_soil * nu_soil) * i_w) / e_soil_kpa;

    let delta_total_mm = (delta_elastic_shaft_m + delta_tip_m) * 1000.0;

    let is_cap_ok = dc_ratio <= 1.0;
    let is_settle_ok = delta_total_mm <= 25.0; // 25 mm standard bridge tolerance

    Ok(DrilledShaftResult {
        problem_id: problem.problem_id.clone(),
        base_cross_sectional_area_m2: a_b_m2,
        unit_side_resistance_kpa: f_s_kpa,
        unit_base_resistance_mpa: q_p_mpa,
        nominal_side_resistance_kn: r_s_kn,
        nominal_base_resistance_kn: r_p_kn,
        nominal_total_resistance_kn: r_n_kn,
        factored_geotechnical_resistance_kn: r_r_kn,
        factored_structural_resistance_kn: p_r_kn,
        geotechnical_demand_capacity_ratio: dc_ratio,
        elastic_service_settlement_mm: delta_total_mm,
        is_geotechnical_capacity_adequate: is_cap_ok,
        is_settlement_acceptable: is_settle_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highway_bridge_drilled_shaft() -> Result<(), String> {
        let prob = DrilledShaftProblem {
            problem_id: "overpass_pier_shaft".to_string(),
            geomaterial: GeomaterialType::CohesiveClay,
            shaft_diameter_m: 1.20,
            embedded_length_m: 18.0,
            factored_load_demand_kn: 3200.0,
            service_load_demand_kn: 2200.0,
            undrained_shear_strength_kpa: 120.0,
            effective_friction_angle_deg: 32.0,
            soil_unit_weight_kn_m3: 18.5,
            concrete_compressive_strength_mpa: 35.0,
            steel_reinforcement_ratio_pct: 1.50,
        };

        let res = solve_drilled_shaft(&prob)?;
        assert!(res.base_cross_sectional_area_m2 > 1.0 && res.base_cross_sectional_area_m2 < 1.3);
        assert!(res.unit_side_resistance_kpa > 50.0 && res.unit_side_resistance_kpa < 80.0);
        assert!(res.unit_base_resistance_mpa > 0.9 && res.unit_base_resistance_mpa < 1.3);
        assert!(res.nominal_side_resistance_kn > 3000.0);
        assert!(res.nominal_base_resistance_kn > 1000.0);
        assert!(res.nominal_total_resistance_kn > 4500.0);
        assert!(res.factored_geotechnical_resistance_kn > 2500.0);
        assert!(res.factored_structural_resistance_kn > 15000.0);
        assert!(res.geotechnical_demand_capacity_ratio <= 1.25);
        assert!(res.elastic_service_settlement_mm > 5.0 && res.elastic_service_settlement_mm < 25.0);
        assert!(res.is_settlement_acceptable);
        Ok(())
    }
}

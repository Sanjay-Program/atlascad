//! Mechanically Stabilized Earth (MSE) Wall Geogrid Stability (FHWA-NHI-10-024 / AASHTO LRFD)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Rankine/Coulomb active lateral earth pressure distribution sigma_h(z) with traffic surcharge.
//! 2. Maximum tensile force per reinforcement layer T_max,i based on vertical spacing S_v.
//! 3. Long-term design strength T_al accounting for installation damage, creep, and durability reduction factors.
//! 4. Layer-by-layer tensile rupture safety factors FS_rupture against ultimate limit state.
//! 5. Effective anchorage embedment length L_e and soil-geogrid pullout resistance safety factor FS_pullout.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Polymeric geogrid reinforcement material grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GeogridPolymerType {
    /// High-Density Polyethylene (HDPE) Uniaxial Geogrid: High creep resistance, stiff monolithic ribs.
    HdpeUniaxial,
    /// High-Tenacity Polyester (PET) Biaxial/Uniaxial Geogrid: High tensile modulus, low strain.
    PolyesterPet,
    /// Polypropylene (PP) Biaxial Geogrid: Base stabilization and low-height retaining walls.
    PolypropylenePp,
}

/// Input parameters for Mechanically Stabilized Earth (MSE) retaining wall analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MseWallProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Geogrid polymer material classification.
    pub polymer_type: GeogridPolymerType,
    /// Total wall vertical height H in meters (m, standard 3.0 to 18.0 m).
    pub wall_height_h_m: f64,
    /// Total geogrid reinforcement length L in meters (m, standard 0.7*H to 1.2*H).
    pub reinforcement_length_l_m: f64,
    /// Vertical layer spacing of geogrid sheets S_v in meters (m, standard 0.40 to 1.0 m).
    pub vertical_layer_spacing_m: f64,
    /// Reinforced backfill soil unit weight gamma in kiloNewtons per cubic meter (kN/m³, standard 17 to 22 kN/m³).
    pub soil_unit_weight_kn_m3: f64,
    /// Reinforced backfill internal friction angle phi_r in degrees (deg, standard 28 to 42 deg).
    pub soil_friction_angle_deg: f64,
    /// Uniform live load traffic surcharge q on wall crest in kiloPascals (kPa, standard 5 to 25 kPa).
    pub crest_surcharge_q_kpa: f64,
    /// Ultimate manufacturer tensile strength of geogrid T_ult in kiloNewtons per meter (kN/m, standard 30 to 200 kN/m).
    pub geogrid_ultimate_strength_kn_m: f64,
    /// Installation damage reduction factor RF_id (standard 1.05 to 1.30).
    pub rf_installation_damage: f64,
    /// Creep deformation reduction factor RF_cr (standard 1.40 to 2.60).
    pub rf_creep_rupture: f64,
    /// Chemical and biological durability reduction factor RF_d (standard 1.10 to 1.30).
    pub rf_durability: f64,
}

impl Default for MseWallProblem {
    fn default() -> Self {
        Self {
            problem_id: "highway_bridge_abutment_mse_wall".to_string(),
            polymer_type: GeogridPolymerType::HdpeUniaxial,
            wall_height_h_m: 8.0,               // 8.0 m highway bridge approach retaining wall
            reinforcement_length_l_m: 6.0,      // 6.0 m geogrid embedment (L = 0.75*H >= 0.70*H)
            vertical_layer_spacing_m: 0.60,     // 0.60 m layer spacing (13 layers)
            soil_unit_weight_kn_m3: 19.5,       // Well-graded crushed gravel fill
            soil_friction_angle_deg: 34.0,      // phi = 34 deg
            crest_surcharge_q_kpa: 12.0,        // 12 kPa AASHTO highway traffic surcharge
            geogrid_ultimate_strength_kn_m: 110.0, // 110 kN/m HDPE uniaxial geogrid (e.g. UX1600MSE / Miragrid 10XT)
            rf_installation_damage: 1.15,
            rf_creep_rupture: 1.65,
            rf_durability: 1.15,
        }
    }
}

/// Simulation results for MSE wall internal stability, tensile stresses, and pullout capacities.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MseWallResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Rankine active earth pressure coefficient K_a = tan²(45° - phi/2).
    pub active_earth_pressure_coef_ka: f64,
    /// Total number of geogrid reinforcement layers.
    pub total_reinforcement_layers: usize,
    /// Long-term allowable design geogrid tensile strength T_al = T_ult / (RF_id * RF_cr * RF_d) in kN/m.
    pub allowable_design_strength_tal_kn_m: f64,
    /// Peak tensile load occurring in the lowest reinforcement layer T_max,bot in kN/m.
    pub max_layer_tensile_force_kn_m: f64,
    /// Critical minimum factor of safety against geogrid tensile rupture FS_rupture (target >= 1.50).
    pub min_rupture_safety_factor: f64,
    /// Effective anchorage embedment length L_e for the critical upper layer in meters (m).
    pub critical_embedment_length_le_m: f64,
    /// Available geogrid pullout resistance P_r in critical layer in kN/m.
    pub critical_pullout_resistance_kn_m: f64,
    /// Critical minimum factor of safety against geogrid pullout failure FS_pullout (target >= 1.50).
    pub min_pullout_safety_factor: f64,
    /// Reinforcement length to wall height ratio (L / H, AASHTO minimum 0.70).
    pub length_to_height_ratio: f64,
    /// Total weight of reinforced soil block per unit wall meter in kN/m.
    pub reinforced_soil_mass_weight_kn_m: f64,
    /// Whether minimum reinforcement embedment length satisfies AASHTO LRFD (L >= 0.70 * H).
    pub is_embedment_length_adequate: bool,
    /// Whether all reinforcement layers satisfy minimum tensile rupture safety criteria (FS >= 1.50).
    pub is_tensile_rupture_safe: bool,
    /// Whether all reinforcement layers satisfy pullout anchorage resistance criteria (FS >= 1.50).
    pub is_pullout_resistance_safe: bool,
    /// Overall Mechanically Stabilized Earth retaining wall internal stability status.
    pub is_mse_wall_stable: bool,
}

/// Solves Mechanically Stabilized Earth (MSE) geogrid internal stability, rupture, and pullout.
pub fn solve_mse_retaining_wall(problem: &MseWallProblem) -> Result<MseWallResult, String> {
    if problem.wall_height_h_m <= 1.0 {
        return Err("Wall height must exceed 1.0 m".to_string());
    }
    if problem.reinforcement_length_l_m <= 0.5 {
        return Err("Reinforcement length must exceed 0.5 m".to_string());
    }
    if problem.vertical_layer_spacing_m <= 0.1 || problem.vertical_layer_spacing_m > 2.0 {
        return Err("Vertical layer spacing must be between 0.1 and 2.0 m".to_string());
    }
    if problem.soil_friction_angle_deg <= 15.0 || problem.soil_friction_angle_deg >= 50.0 {
        return Err("Soil friction angle must be between 15 and 50 degrees".to_string());
    }
    if problem.soil_unit_weight_kn_m3 <= 10.0 {
        return Err("Soil unit weight must exceed 10 kN/m³".to_string());
    }
    if problem.geogrid_ultimate_strength_kn_m <= 5.0 {
        return Err("Geogrid ultimate strength must exceed 5 kN/m".to_string());
    }

    let phi_rad = problem.soil_friction_angle_deg.to_radians();

    // Rankine active lateral earth pressure coefficient:
    // K_a = tan²(45° - phi/2) = (1 - sin(phi)) / (1 + sin(phi))
    let k_a = (1.0 - phi_rad.sin()) / (1.0 + phi_rad.sin());

    // Total reduction factor: RF_total = RF_id * RF_cr * RF_d
    let rf_total = problem.rf_installation_damage * problem.rf_creep_rupture * problem.rf_durability;
    if rf_total <= 1.0 {
        return Err("Combined reduction factor must exceed 1.0".to_string());
    }

    // Long-term allowable geogrid design strength T_al:
    let t_al = problem.geogrid_ultimate_strength_kn_m / rf_total; // kN/m

    // Reinforcement layer count and positions:
    let s_v = problem.vertical_layer_spacing_m;
    let n_layers = (problem.wall_height_h_m / s_v).floor() as usize;
    if n_layers == 0 {
        return Err("At least one reinforcement layer required".to_string());
    }

    // Rankine active failure plane angle with horizontal: theta = 45° + phi/2:
    let theta_rad = std::f64::consts::FRAC_PI_4 + phi_rad / 2.0;

    let mut min_fs_rupture = f64::MAX;
    let mut min_fs_pullout = f64::MAX;
    let mut max_t_layer = 0.0;
    let mut crit_le = 0.0;
    let mut crit_pr = 0.0;

    // Soil-geogrid pullout interaction parameters (FHWA-NHI-10-024):
    // F* = pullout friction factor = 0.80 * tan(phi) for geogrids
    // alpha = scale effect correction factor ~ 0.80
    // C = 2 surface factor (top and bottom shear surfaces)
    let f_star = 0.80 * phi_rad.tan();
    let alpha_scale = 0.80;

    for i in 1..=n_layers {
        // Depth from crest to layer i:
        let z_i = (i as f64) * s_v;

        // Total vertical stress at layer depth: sigma_v = gamma * z_i + q [kPa]:
        let sigma_v = problem.soil_unit_weight_kn_m3 * z_i + problem.crest_surcharge_q_kpa;

        // Horizontal active stress at layer depth: sigma_h = K_a * sigma_v [kPa]:
        let sigma_h = k_a * sigma_v;

        // Tensile force per unit meter of wall width: T_i = sigma_h * S_v [kN/m]:
        let t_i = sigma_h * s_v;
        if t_i > max_t_layer {
            max_t_layer = t_i;
        }

        // Factor of safety against tensile rupture:
        let fs_rup = t_al / t_i.max(0.1);
        if fs_rup < min_fs_rupture {
            min_fs_rupture = fs_rup;
        }

        // Distance from active failure plane to back of reinforced zone:
        // Width of active wedge at depth z_i = (H - z_i) / tan(theta)
        let active_wedge_width = (problem.wall_height_h_m - z_i) / theta_rad.tan();
        let l_e = (problem.reinforcement_length_l_m - active_wedge_width).max(1.0); // minimum 1.0 m embedment

        // Pullout resistance per unit width: P_r = 2 * alpha * F* * sigma_v * L_e [kN/m]:
        let p_r = 2.0 * alpha_scale * f_star * sigma_v * l_e;

        // Factor of safety against pullout:
        let fs_pull = p_r / t_i.max(0.1);
        if fs_pull < min_fs_pullout {
            min_fs_pullout = fs_pull;
            crit_le = l_e;
            crit_pr = p_r;
        }
    }

    let lh_ratio = problem.reinforcement_length_l_m / problem.wall_height_h_m;
    let reinforced_weight = problem.soil_unit_weight_kn_m3 * problem.wall_height_h_m * problem.reinforcement_length_l_m;

    // Design verification:
    let is_l_ok = lh_ratio >= 0.70;
    let is_rup_ok = min_fs_rupture >= 1.50;
    let is_pull_ok = min_fs_pullout >= 1.50;
    let is_overall_ok = is_l_ok && is_rup_ok && is_pull_ok;

    Ok(MseWallResult {
        problem_id: problem.problem_id.clone(),
        active_earth_pressure_coef_ka: k_a,
        total_reinforcement_layers: n_layers,
        allowable_design_strength_tal_kn_m: t_al,
        max_layer_tensile_force_kn_m: max_t_layer,
        min_rupture_safety_factor: min_fs_rupture,
        critical_embedment_length_le_m: crit_le,
        critical_pullout_resistance_kn_m: crit_pr,
        min_pullout_safety_factor: min_fs_pullout,
        length_to_height_ratio: lh_ratio,
        reinforced_soil_mass_weight_kn_m: reinforced_weight,
        is_embedment_length_adequate: is_l_ok,
        is_tensile_rupture_safe: is_rup_ok,
        is_pullout_resistance_safe: is_pull_ok,
        is_mse_wall_stable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_highway_mse_retaining_wall() -> Result<(), String> {
        let prob = MseWallProblem::default();
        let res = solve_mse_retaining_wall(&prob)?;

        assert!(res.active_earth_pressure_coef_ka > 0.25 && res.active_earth_pressure_coef_ka < 0.32);
        assert_eq!(res.total_reinforcement_layers, 13);
        assert!(res.allowable_design_strength_tal_kn_m > 35.0 && res.allowable_design_strength_tal_kn_m < 55.0);
        assert!(res.max_layer_tensile_force_kn_m > 15.0 && res.max_layer_tensile_force_kn_m < 35.0);
        assert!(res.min_rupture_safety_factor >= 1.50, "Should satisfy AASHTO LRFD rupture safety factor >= 1.50");
        assert!(res.min_pullout_safety_factor >= 1.50, "Should satisfy AASHTO LRFD pullout safety factor >= 1.50");
        assert!(res.length_to_height_ratio >= 0.70);
        assert!(res.is_embedment_length_adequate);
        assert!(res.is_tensile_rupture_safe);
        assert!(res.is_pullout_resistance_safe);
        assert!(res.is_mse_wall_stable);
        Ok(())
    }
}

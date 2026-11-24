//! Industrial Belt Conveyor Bulk Material Handling & Drive Power (DIN 22101 / ISO 5048 / CEMA)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Troughed three-roll idler cross-sectional bulk material surcharge area A(b, beta, lambda).
//! 2. Continuous volumetric flow Q and mass conveyance throughput capacity I_m (metric tons/hour).
//! 3. Primary motion resistances F_H (bearing friction, belt flexure, indentation resistance).
//! 4. Incline/decline lift resistance F_St and secondary resistances (skirting, acceleration).
//! 5. Euler-Eytelwein capstan drive pulley slip limits T_1 / T_2 <= exp(mu * theta).
//! 6. Electric motor drive shaft brake power P_drive (kW) and belt carcass tensile safety factor.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Conveyor idler troughing roll angle configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TroughingAngleClass {
    /// 20-degree troughing: Flat or gentle bulk materials (grain, dry woodchips).
    Trough20Deg,
    /// 35-degree troughing: Standard heavy industrial mining (coal, crushed limestone, iron ore).
    Trough35Deg,
    /// 45-degree troughing: Deep troughing for maximum capacity and low spillage.
    Trough45Deg,
}

/// Conveyor belt reinforcement carcass construction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BeltCarcassType {
    /// Polyester-Polyamide (EP) Textile Fabric Plies: High elongation, flexible, standard duties.
    TextileFabricEP,
    /// Steel Cord (ST): High-tensile, low elongation, long-distance overland conveyors.
    SteelCordST,
}

/// Input parameters for industrial belt conveyor engineering analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConveyorProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Belt carcass reinforcement type.
    pub carcass_type: BeltCarcassType,
    /// Idler troughing angle geometry.
    pub troughing_angle: TroughingAngleClass,
    /// Belt width B in millimeters (mm, standard 500 to 2400 mm).
    pub belt_width_mm: f64,
    /// Center-to-center conveyor installation length L in meters (m, standard 10 to 5000 m).
    pub conveyor_length_m: f64,
    /// Vertical elevation lift height H in meters (m, positive for incline, negative for decline).
    pub elevation_lift_m: f64,
    /// Operating belt linear travel speed v in meters per second (m/s, standard 1.0 to 6.5 m/s).
    pub belt_speed_m_s: f64,
    /// Bulk material loose density rho_bulk in kilograms per cubic meter (kg/m³, standard 600 to 2800 kg/m³).
    pub material_bulk_density_kg_m3: f64,
    /// Material dynamic angle of surcharge beta in degrees (deg, standard 10 to 30 deg).
    pub surcharge_angle_deg: f64,
    /// Nominal conveyor belt unit weight q_b in kilograms per meter (kg/m, standard 10 to 75 kg/m).
    pub belt_unit_weight_kg_m: f64,
    /// Carrying side rotating idler weight per unit length q_ro in kilograms per meter (kg/m, standard 15 to 60 kg/m).
    pub idlers_carrying_weight_kg_m: f64,
    /// Return side rotating idler weight per unit length q_ru in kilograms per meter (kg/m, standard 5 to 25 kg/m).
    pub idlers_return_weight_kg_m: f64,
    /// Artificial global friction coefficient f according to DIN 22101 / ISO 5048 (standard 0.015 to 0.035).
    pub friction_factor_f: f64,
    /// Drive pulley wrap contact angle theta in degrees (deg, standard 180 to 240 deg).
    pub drive_wrap_angle_deg: f64,
    /// Friction coefficient between drive pulley lagging and rubber belt (standard 0.25 to 0.40).
    pub drive_friction_mu: f64,
    /// Nominal manufacturer belt carcass breaking strength in kiloNewtons per meter width (kN/m).
    pub belt_rated_breaking_strength_kn_m: f64,
    /// Mechanical drive transmission efficiency (standard 0.88 to 0.96).
    pub drive_mechanical_efficiency: f64,
}

impl Default for ConveyorProblem {
    fn default() -> Self {
        Self {
            problem_id: "overland_crushed_ore_conveyor".to_string(),
            carcass_type: BeltCarcassType::SteelCordST,
            troughing_angle: TroughingAngleClass::Trough35Deg,
            belt_width_mm: 1200.0,                     // 1200 mm (B = 1.2 m)
            conveyor_length_m: 450.0,                  // 450 m center-to-center run
            elevation_lift_m: 28.0,                    // 28 m elevation gain (incline)
            belt_speed_m_s: 3.15,                      // 3.15 m/s belt velocity
            material_bulk_density_kg_m3: 1650.0,       // 1650 kg/m³ crushed copper-gold ore
            surcharge_angle_deg: 20.0,                 // 20 deg dynamic surcharge
            belt_unit_weight_kg_m: 24.5,               // 24.5 kg/m heavy rubber belt
            idlers_carrying_weight_kg_m: 28.0, // 28 kg/m 3-roll trough idlers @ 1.2m spacing
            idlers_return_weight_kg_m: 8.5,    // 8.5 kg/m flat return rolls @ 3.0m spacing
            friction_factor_f: 0.022,          // Standard DIN 22101 coefficient
            drive_wrap_angle_deg: 210.0,       // 210° snubbed drive pulley
            drive_friction_mu: 0.35,           // Ceramic/rubber diamond lagging
            belt_rated_breaking_strength_kn_m: 1250.0, // ST1250 belt (1250 kN/m)
            drive_mechanical_efficiency: 0.92, // 92% gearbox + coupling efficiency
        }
    }
}

/// Simulation results for belt conveyor capacity, motion resistances, tensions, and motor power.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConveyorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Theoretical cross-sectional area of bulk material on belt in square meters (m²).
    pub material_cross_section_m2: f64,
    /// Hourly volumetric conveyance throughput capacity in cubic meters per hour (m³/h).
    pub volumetric_capacity_m3_h: f64,
    /// Continuous mass conveyance throughput capacity in metric tons per hour (t/h).
    pub mass_throughput_t_h: f64,
    /// Linear material payload mass per unit length q_G in kilograms per meter (kg/m).
    pub material_unit_mass_kg_m: f64,
    /// Primary horizontal frictional motion resistance F_H in kiloNewtons (kN).
    pub primary_resistance_fh_kn: f64,
    /// Elevation lift slope resistance F_St in kiloNewtons (kN).
    pub slope_resistance_fst_kn: f64,
    /// Secondary and accessory resistances F_N in kiloNewtons (kN).
    pub secondary_resistance_fn_kn: f64,
    /// Total peripheral driving force required at pulley circumference F_U in kiloNewtons (kN).
    pub peripheral_drive_force_fu_kn: f64,
    /// Slack-side belt tension T_2 in kiloNewtons (kN).
    pub slack_side_tension_t2_kn: f64,
    /// Tight-side maximum belt tension T_1 in kiloNewtons (kN).
    pub tight_side_tension_t1_kn: f64,
    /// Euler-Eytelwein capstan drive slip safety factor (ratio of allowable to required tension ratio).
    pub drive_slip_safety_factor: f64,
    /// Mechanical shaft power required at drive pulley in kiloWatts (kW).
    pub pulley_shaft_power_kw: f64,
    /// Electrical motor installed brake power in kiloWatts (kW).
    pub motor_brake_power_kw: f64,
    /// Maximum operating belt carcass tensile safety factor.
    pub belt_tensile_safety_factor: f64,
    /// Assessment indicating Euler-Eytelwein non-slip drive adhesion compliance.
    pub is_drive_slip_safe: bool,
    /// Assessment indicating belt carcass strength meets DIN 22101 safety limits (>= 6.5 for ST, >= 8.0 for EP).
    pub is_belt_strength_sufficient: bool,
    /// Overall conveyor engineering design viability.
    pub is_conveyor_design_viable: bool,
}

/// Solves industrial belt conveyor kinematics, resistances, and drive power according to DIN 22101 / ISO 5048.
pub fn solve_conveyor_bulk_handling(problem: &ConveyorProblem) -> Result<ConveyorResult, String> {
    if problem.belt_width_mm < 300.0 || problem.belt_width_mm > 3200.0 {
        return Err("Belt width must be between 300 and 3200 mm".to_string());
    }
    if problem.conveyor_length_m < 2.0 {
        return Err("Conveyor length must be at least 2.0 meters".to_string());
    }
    if problem.belt_speed_m_s <= 0.1 || problem.belt_speed_m_s > 10.0 {
        return Err("Belt speed must be between 0.1 and 10.0 m/s".to_string());
    }
    if problem.material_bulk_density_kg_m3 <= 100.0 {
        return Err("Material bulk density must be positive".to_string());
    }

    let b_m = problem.belt_width_mm / 1000.0;
    let beta_rad = problem.surcharge_angle_deg.to_radians();
    let lambda_deg: f64 = match problem.troughing_angle {
        TroughingAngleClass::Trough20Deg => 20.0,
        TroughingAngleClass::Trough35Deg => 35.0,
        TroughingAngleClass::Trough45Deg => 45.0,
    };
    let lambda_rad = lambda_deg.to_radians();

    // Standard DIN 22101 / CEMA 3-roll idler cross-sectional area:
    // Effective load edge clearance c = 0.055 * B + 23 mm (converted to meters)
    let c_edge = 0.055 * b_m + 0.023;
    let b_eff = (b_m - 2.0 * c_edge).max(0.1);

    // Center roll length l_c ~ 0.38 * B for standard 3-roll equal-length idlers:
    let l_c = 0.38 * b_eff;
    let l_side = (b_eff - l_c) / 2.0;

    // Trapezoidal lower trough area A_1:
    let h_trough = l_side * lambda_rad.sin();
    let w_top_trough = l_c + 2.0 * l_side * lambda_rad.cos();
    let a_1 = 0.5 * (l_c + w_top_trough) * h_trough;

    // Circular segment surcharge area A_2 (approximated via parabolic/circular segment arc):
    // A_2 = (w_top_trough^2 / 6) * tan(beta)
    let a_2 = (w_top_trough.powi(2) / 6.0) * beta_rad.tan();
    let a_total = (a_1 + a_2).max(0.001);

    // Volumetric capacity: I_v = 3600 * A * v [m³/h]
    let iv_m3_h = 3600.0 * a_total * problem.belt_speed_m_s;

    // Mass throughput: I_m = I_v * rho_bulk / 1000 [t/h]
    let im_t_h = iv_m3_h * (problem.material_bulk_density_kg_m3 / 1000.0);

    // Material weight per linear meter: q_G = I_m / (3.6 * v) [kg/m]
    let q_g = im_t_h / (3.6 * problem.belt_speed_m_s);

    let g = 9.80665; // m/s²

    // DIN 22101 Primary Motion Resistance F_H:
    // F_H = f * L * g * [ q_G + q_B + q_ro + (q_B + q_ru) ]
    let q_carrying = q_g + problem.belt_unit_weight_kg_m + problem.idlers_carrying_weight_kg_m;
    let q_return = problem.belt_unit_weight_kg_m + problem.idlers_return_weight_kg_m;
    let f_h_n = problem.friction_factor_f * problem.conveyor_length_m * g * (q_carrying + q_return);
    let f_h_kn = f_h_n / 1000.0;

    // Slope / Elevation Resistance F_St:
    // F_St = q_G * g * H [N] (positive for lifting, negative for regenerating decline)
    let f_st_n = q_g * g * problem.elevation_lift_m;
    let f_st_kn = f_st_n / 1000.0;

    // Secondary resistances F_N (skirtboards, material acceleration, cleaner friction ~ 8-15% of F_H):
    let f_n_n = 0.12 * f_h_n.abs() + 2500.0; // 12% F_H + base cleaner/skirt friction
    let f_n_kn = f_n_n / 1000.0;

    // Total required peripheral driving force at drive pulley F_U:
    let f_u_n = (f_h_n + f_st_n + f_n_n).max(500.0);
    let f_u_kn = f_u_n / 1000.0;

    // Euler-Eytelwein capstan friction drive:
    // F_U = T_1 - T_2, with T_1 / T_2 <= exp(mu * theta)
    // T_2,min = F_U / (exp(mu * theta) - 1)
    let theta_rad = problem.drive_wrap_angle_deg.to_radians();
    let e_mu_theta = (problem.drive_friction_mu * theta_rad).exp();

    let t2_min_n = f_u_n / (e_mu_theta - 1.0);
    // Take-up tension ensures slack-side tension exceeds minimum:
    let t2_n = (1.15 * t2_min_n).max(2500.0); // 15% margin
    let t1_n = t2_n + f_u_n;

    let t1_kn = t1_n / 1000.0;
    let t2_kn = t2_n / 1000.0;

    let actual_ratio = t1_n / t2_n.max(1.0);
    let allowable_ratio = e_mu_theta;
    let drive_slip_sf = allowable_ratio / actual_ratio.max(1.0);

    // Pulley shaft power: P = F_U * v [kW]:
    let p_pulley_kw = (f_u_n * problem.belt_speed_m_s) / 1000.0;
    // Motor brake power:
    let p_motor_kw = p_pulley_kw / problem.drive_mechanical_efficiency.max(0.5);

    // Belt carcass breaking strength: B_total = B_m * rated_strength [kN]:
    let belt_breaking_load_kn = b_m * problem.belt_rated_breaking_strength_kn_m;
    let belt_tensile_sf = belt_breaking_load_kn / t1_kn.max(1.0);

    let min_required_sf = match problem.carcass_type {
        BeltCarcassType::TextileFabricEP => 8.0,
        BeltCarcassType::SteelCordST => 6.5,
    };

    let is_slip_safe = drive_slip_sf >= 1.05;
    let is_strength_safe = belt_tensile_sf >= min_required_sf;
    let is_overall_ok = is_slip_safe && is_strength_safe && (im_t_h > 0.0);

    Ok(ConveyorResult {
        problem_id: problem.problem_id.clone(),
        material_cross_section_m2: a_total,
        volumetric_capacity_m3_h: iv_m3_h,
        mass_throughput_t_h: im_t_h,
        material_unit_mass_kg_m: q_g,
        primary_resistance_fh_kn: f_h_kn,
        slope_resistance_fst_kn: f_st_kn,
        secondary_resistance_fn_kn: f_n_kn,
        peripheral_drive_force_fu_kn: f_u_kn,
        slack_side_tension_t2_kn: t2_kn,
        tight_side_tension_t1_kn: t1_kn,
        drive_slip_safety_factor: drive_slip_sf,
        pulley_shaft_power_kw: p_pulley_kw,
        motor_brake_power_kw: p_motor_kw,
        belt_tensile_safety_factor: belt_tensile_sf,
        is_drive_slip_safe: is_slip_safe,
        is_belt_strength_sufficient: is_strength_safe,
        is_conveyor_design_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_overland_ore_conveyor() -> Result<(), String> {
        let prob = ConveyorProblem::default();
        let res = solve_conveyor_bulk_handling(&prob)?;

        assert!(res.material_cross_section_m2 > 0.08 && res.material_cross_section_m2 < 0.18);
        assert!(res.mass_throughput_t_h > 1500.0 && res.mass_throughput_t_h < 3500.0);
        assert!(
            res.peripheral_drive_force_fu_kn > 60.0 && res.peripheral_drive_force_fu_kn < 160.0
        );
        assert!(res.motor_brake_power_kw > 200.0 && res.motor_brake_power_kw < 550.0);
        assert!(
            res.drive_slip_safety_factor >= 1.05,
            "Capstan drive slip safety factor >= 1.05"
        );
        assert!(
            res.belt_tensile_safety_factor >= 6.5,
            "Steel cord carcass tensile safety factor >= 6.5"
        );
        assert!(res.is_drive_slip_safe);
        assert!(res.is_belt_strength_sufficient);
        assert!(res.is_conveyor_design_viable);
        Ok(())
    }
}

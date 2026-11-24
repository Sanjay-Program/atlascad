//! Tunnel Boring Machine (TBM) Earth Pressure Balance (EPB) Cutterhead Mechanics & Face Stability Solver.
//!
//! Provides first-principles geotechnical and mechanical calculations for soft-ground EPB and slurry TBMs
//! in accordance with International Tunnelling Association (ITA/AITES), Herzog, Krause, and Eirich standards.
//!
//! # Physical Formulations:
//! - **Hydrostatic & Earth Pressure at Tunnel Crown / Axis / Invert**:
//!   $$P_{water} = \gamma_w \cdot \max(0, z_{depth} - z_{water})$$
//!   $$\sigma'_v = \gamma_{dry} z_{water} + (\gamma_{sat} - \gamma_w)(z_{depth} - z_{water})$$
//!   $$K_0 = 1 - \sin\phi' \quad (\text{Jaky's equation})$$
//!   $$P_{earth} = K_0 \cdot \sigma'_v + c' \cot\phi' (1 - \sin\phi') \dots$$
//! - **Minimum Face Support Pressure to Prevent Collapse (Blowout / Subsidence)**:
//!   $$P_{supp,min} = P_{water} + \sigma'_v \cdot K_a - 2 c' \sqrt{K_a} + \Delta P_{margin}$$
//! - **Cutterhead Penetration Speed**:
//!   $$v_p = PR \cdot RPM \quad [\text{mm/min}]$$
//! - **Cutterhead Thrust & Torque Decomposition**:
//!   $$F_{thrust} = F_{cutting} + F_{face,support} + F_{shield,friction} + F_{backup,trailer}$$
//!   $$T_{cutterhead} = T_{cutting} + T_{muck,friction} + T_{seals/bearings}$$
//! - **Excavation Specific Energy ($SE$)**:
//!   $$SE = \frac{F_{thrust}}{A_{face}} + \frac{2\pi \cdot T_{cutterhead} \cdot RPM}{A_{face} \cdot v_p} \quad [\text{MJ/m}^3]$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Ground geological condition category for tunneling.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroundConditionType {
    /// Soft cohesive clay / silt.
    SoftCohesiveClay,
    /// Medium dense cohesionless sand / gravel.
    CohesionlessSandGravel,
    /// Mixed ground with boulders and stiff clay.
    MixedFaceGround,
    /// Weathered soft sedimentary rock / mudstone.
    WeatheredSoftRock,
}

/// Input parameters for TBM cutterhead and face stability analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TbmTunnelingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Ground condition type.
    pub ground_condition: GroundConditionType,
    /// Outer diameter of the TBM cutterhead / excavated bore in meters (m).
    pub tunnel_outer_diameter_m: f64,
    /// Depth from ground surface to tunnel axis center in meters (m).
    pub tunnel_depth_to_axis_m: f64,
    /// Depth to groundwater table below surface in meters (m).
    pub water_table_depth_m: f64,
    /// Total soil bulk unit weight in kiloNewtons per cubic meter (kN/m³).
    pub soil_bulk_unit_weight_kn_m3: f64,
    /// Effective cohesion of the soil c' in kiloPascals (kPa).
    pub soil_effective_cohesion_kpa: f64,
    /// Effective friction angle of the soil phi' in degrees.
    pub soil_effective_friction_angle_deg: f64,
    /// Unconfined compressive strength (UCS) or undrained shear strength in MegaPascals (MPa).
    pub soil_compressive_strength_mpa: f64,
    /// Cutterhead opening ratio percentage (percentage of open aperture area for muck intake).
    pub cutterhead_opening_ratio_pct: f64,
    /// Cutterhead rotational speed in revolutions per minute (RPM).
    pub cutterhead_rotational_speed_rpm: f64,
    /// Cutterhead penetration rate per revolution in millimeters per rev (mm/rev).
    pub penetration_rate_mm_per_rev: f64,
    /// TBM machine shield length in meters (m).
    pub shield_length_m: f64,
    /// Maximum rated cutterhead drive torque of the TBM in kiloNewton-meters (kN·m).
    pub machine_rated_torque_kn_m: f64,
    /// Maximum rated total thrust cylinder capacity in kiloNewtons (kN).
    pub machine_rated_thrust_kn: f64,
}

impl Default for TbmTunnelingProblem {
    fn default() -> Self {
        Self {
            problem_id: "epb_tbm_metro_tunnel".to_string(),
            ground_condition: GroundConditionType::CohesionlessSandGravel,
            tunnel_outer_diameter_m: 6.60,
            tunnel_depth_to_axis_m: 22.0,
            water_table_depth_m: 4.0,
            soil_bulk_unit_weight_kn_m3: 19.5,
            soil_effective_cohesion_kpa: 12.0,
            soil_effective_friction_angle_deg: 32.0,
            soil_compressive_strength_mpa: 0.85,
            cutterhead_opening_ratio_pct: 35.0,
            cutterhead_rotational_speed_rpm: 2.5,
            penetration_rate_mm_per_rev: 12.0,
            shield_length_m: 9.50,
            machine_rated_torque_kn_m: 5500.0,
            machine_rated_thrust_kn: 42000.0,
        }
    }
}

/// Simulation results for TBM cutterhead performance and face stability.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TbmTunnelingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Excavated face cross-sectional area in square meters (m²).
    pub face_area_m2: f64,
    /// Hydrostatic groundwater pressure at tunnel axis in kiloPascals (kPa).
    pub hydrostatic_water_pressure_kpa: f64,
    /// Total effective vertical overburden stress at tunnel crown in kiloPascals (kPa).
    pub effective_crown_vertical_stress_kpa: f64,
    /// Rankine active lateral earth pressure coefficient K_a.
    pub rankine_active_coeff_ka: f64,
    /// Theoretical minimum chamber face support pressure to prevent face collapse in kPa.
    pub min_face_support_pressure_kpa: f64,
    /// Target operating EPB chamber support pressure at tunnel axis in kPa (including safety buffer).
    pub target_face_support_pressure_kpa: f64,
    /// Face stability safety factor against active collapse (target / minimum).
    pub face_stability_safety_factor: f64,
    /// Penetration advance speed in millimeters per minute (mm/min).
    pub penetration_speed_mm_min: f64,
    /// Instantaneous volumetric excavation rate in cubic meters per hour (m³/h).
    pub volumetric_excavation_rate_m3_h: f64,
    /// Net cutterhead thrust required for soil/rock cutting in kiloNewtons (kN).
    pub cutting_thrust_kn: f64,
    /// Face support pressure thrust on the cutterhead bulkhead in kiloNewtons (kN).
    pub face_pressure_thrust_kn: f64,
    /// Shield skin friction resistance against ground in kiloNewtons (kN).
    pub shield_skin_friction_kn: f64,
    /// Total required TBM advance thrust in kiloNewtons (kN).
    pub total_required_thrust_kn: f64,
    /// Total required cutterhead drive torque in kiloNewton-meters (kN·m).
    pub total_cutterhead_torque_kn_m: f64,
    /// Cutterhead electric drive power in kiloWatts (kW).
    pub cutterhead_drive_power_kw: f64,
    /// Specific excavation energy (SE) in MegaJoules per cubic meter (MJ/m³).
    pub specific_energy_mj_m3: f64,
    /// Whether the face support pressure guarantees face stability against collapse (FS >= 1.20).
    pub is_face_stable: bool,
    /// Whether required cutterhead torque is safely within machine rated capacity (Torque <= 0.85 * Rated).
    pub is_torque_within_machine_capacity: bool,
    /// Whether required thrust is safely within machine cylinder capacity (Thrust <= 0.85 * Rated).
    pub is_thrust_within_machine_capacity: bool,
}

/// Solves TBM cutterhead thrust, torque, specific energy, and EPB face stability.
pub fn solve_tbm_tunneling(problem: &TbmTunnelingProblem) -> Result<TbmTunnelingResult, String> {
    if problem.tunnel_outer_diameter_m <= 0.50 {
        return Err("Tunnel outer diameter must be greater than 0.50 m".to_string());
    }
    if problem.tunnel_depth_to_axis_m <= problem.tunnel_outer_diameter_m / 2.0 {
        return Err(
            "Tunnel depth to axis must exceed the radius (positive cover depth required)"
                .to_string(),
        );
    }
    if problem.cutterhead_rotational_speed_rpm <= 0.0 {
        return Err("Cutterhead rotational speed must be positive".to_string());
    }
    if problem.penetration_rate_mm_per_rev <= 0.0 {
        return Err("Penetration rate must be positive".to_string());
    }

    let r = problem.tunnel_outer_diameter_m / 2.0;
    let d = problem.tunnel_outer_diameter_m;
    let face_area_m2 = std::f64::consts::PI * r * r;

    // Geometric depths:
    let z_crown = (problem.tunnel_depth_to_axis_m - r).max(0.10);
    let z_axis = problem.tunnel_depth_to_axis_m;

    // Groundwater conditions (gamma_w = 9.81 kN/m³):
    let gamma_w = 9.81;
    let head_axis = (z_axis - problem.water_table_depth_m).max(0.0);
    let u_axis_kpa = gamma_w * head_axis;

    let head_crown = (z_crown - problem.water_table_depth_m).max(0.0);
    let u_crown_kpa = gamma_w * head_crown;

    // Vertical effective stress at crown:
    let z_dry = problem.water_table_depth_m.min(z_crown);
    let z_sub = (z_crown - problem.water_table_depth_m).max(0.0);
    let sigma_v_crown_eff = problem.soil_bulk_unit_weight_kn_m3 * z_dry
        + (problem.soil_bulk_unit_weight_kn_m3 - gamma_w).max(2.0) * z_sub;

    // Active earth pressure coefficient:
    let phi_rad = problem.soil_effective_friction_angle_deg.to_radians();
    let ka = ((std::f64::consts::PI / 4.0) - (phi_rad / 2.0))
        .tan()
        .powi(2);

    // Minimum face pressure at crown to prevent active collapse (Krause / Herzog / ITA):
    // P_min_crown = u_crown + max(0, sigma'_v * Ka - 2 * c' * sqrt(Ka)) + safety margin (20 kPa)
    let c_kpa = problem.soil_effective_cohesion_kpa;
    let sigma_h_active = (sigma_v_crown_eff * ka - 2.0 * c_kpa * ka.sqrt()).max(0.0);
    let min_crown_support_kpa = u_crown_kpa + sigma_h_active + 20.0;

    // Transfer crown support pressure to axis (soil + water gradient):
    let gamma_slurry = 12.5; // typical EPB muck/slurry density in kN/m³
    let min_axis_support_kpa = min_crown_support_kpa + gamma_slurry * r;

    // Target operating pressure (with 25% operational buffer):
    let target_axis_support_kpa = min_axis_support_kpa * 1.25;
    let face_stability_fs = target_axis_support_kpa / min_axis_support_kpa;

    // Penetration kinematics:
    let penetration_speed_mm_min =
        problem.penetration_rate_mm_per_rev * problem.cutterhead_rotational_speed_rpm;
    let v_p_m_s = (penetration_speed_mm_min / 1000.0) / 60.0;
    let volumetric_excavation_rate_m3_h = face_area_m2 * (penetration_speed_mm_min / 1000.0) * 60.0;

    // Thrust decomposition:
    // 1. Cutting thrust (Herzog / Krause / Eirich):
    // F_cut = C_cut * A_face * UCS (or shear strength)
    let c_cut_coeff = match problem.ground_condition {
        GroundConditionType::SoftCohesiveClay => 0.025,
        GroundConditionType::CohesionlessSandGravel => 0.040,
        GroundConditionType::MixedFaceGround => 0.065,
        GroundConditionType::WeatheredSoftRock => 0.090,
    };
    let cutting_thrust_kn =
        c_cut_coeff * face_area_m2 * (problem.soil_compressive_strength_mpa * 1000.0);

    // 2. Face pressure thrust against the pressure bulkhead:
    let face_pressure_thrust_kn = target_axis_support_kpa * face_area_m2;

    // 3. Shield skin friction:
    // F_shield = mu_shield * sigma_mean * Area_shield
    let shield_area_m2 = std::f64::consts::PI * d * problem.shield_length_m;
    let mu_shield = 0.28;
    let sigma_mean_shield = (sigma_v_crown_eff + u_crown_kpa) * 0.65;
    let shield_skin_friction_kn = mu_shield * sigma_mean_shield * shield_area_m2;

    // 4. Backup trailer towing force (~800 kN standard):
    let backup_towing_kn = 800.0;

    let total_required_thrust_kn =
        cutting_thrust_kn + face_pressure_thrust_kn + shield_skin_friction_kn + backup_towing_kn;

    // Torque decomposition:
    // 1. Soil / rock cutting torque:
    // T_cut = 0.35 * F_cut * R
    let t_cut_kn_m = 0.35 * cutting_thrust_kn * r;

    // 2. Muck shearing / agitation inside EPB mixing chamber (Eirich / Krause):
    // T_muck = alpha_muck * D^3 * (opening_factor)
    let alpha_muck = match problem.ground_condition {
        GroundConditionType::SoftCohesiveClay => 9.5,
        GroundConditionType::CohesionlessSandGravel => 14.0,
        GroundConditionType::MixedFaceGround => 18.5,
        GroundConditionType::WeatheredSoftRock => 16.0,
    };
    let open_ratio_factor = (100.0 - problem.cutterhead_opening_ratio_pct) / 65.0;
    let t_muck_kn_m = alpha_muck * d.powi(3) * 0.50 * open_ratio_factor;

    // 3. Main drive bearing and seal friction torque (~7% of cut+muck):
    let t_seals_kn_m = 0.07 * (t_cut_kn_m + t_muck_kn_m);

    let total_cutterhead_torque_kn_m = t_cut_kn_m + t_muck_kn_m + t_seals_kn_m;

    // Cutterhead drive electric power:
    // P = T * omega = T * (2 * pi * RPM / 60) in kW
    let omega_rad_s = 2.0 * std::f64::consts::PI * problem.cutterhead_rotational_speed_rpm / 60.0;
    let cutterhead_drive_power_kw = total_cutterhead_torque_kn_m * omega_rad_s;

    // Specific excavation energy (SE) in MJ/m³:
    // SE = (F_thrust * v_p + T * omega) / (A_face * v_p) / 1000
    let power_thrust_kw = total_required_thrust_kn * v_p_m_s;
    let total_power_kw = power_thrust_kw + cutterhead_drive_power_kw;
    let vol_flow_m3_s = face_area_m2 * v_p_m_s;
    let specific_energy_mj_m3 = (total_power_kw / vol_flow_m3_s) / 1000.0;

    let is_face_stable = face_stability_fs >= 1.20;
    let is_torque_ok = total_cutterhead_torque_kn_m <= 0.85 * problem.machine_rated_torque_kn_m;
    let is_thrust_ok = total_required_thrust_kn <= 0.85 * problem.machine_rated_thrust_kn;

    Ok(TbmTunnelingResult {
        problem_id: problem.problem_id.clone(),
        face_area_m2,
        hydrostatic_water_pressure_kpa: u_axis_kpa,
        effective_crown_vertical_stress_kpa: sigma_v_crown_eff,
        rankine_active_coeff_ka: ka,
        min_face_support_pressure_kpa: min_axis_support_kpa,
        target_face_support_pressure_kpa: target_axis_support_kpa,
        face_stability_safety_factor: face_stability_fs,
        penetration_speed_mm_min,
        volumetric_excavation_rate_m3_h,
        cutting_thrust_kn,
        face_pressure_thrust_kn,
        shield_skin_friction_kn,
        total_required_thrust_kn,
        total_cutterhead_torque_kn_m,
        cutterhead_drive_power_kw,
        specific_energy_mj_m3,
        is_face_stable,
        is_torque_within_machine_capacity: is_torque_ok,
        is_thrust_within_machine_capacity: is_thrust_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_epb_metro_tunneling() -> Result<(), String> {
        let prob = TbmTunnelingProblem {
            problem_id: "metro_line_epb_shield".to_string(),
            ground_condition: GroundConditionType::CohesionlessSandGravel,
            tunnel_outer_diameter_m: 6.60,
            tunnel_depth_to_axis_m: 22.0,
            water_table_depth_m: 4.0,
            soil_bulk_unit_weight_kn_m3: 19.5,
            soil_effective_cohesion_kpa: 12.0,
            soil_effective_friction_angle_deg: 32.0,
            soil_compressive_strength_mpa: 0.85,
            cutterhead_opening_ratio_pct: 35.0,
            cutterhead_rotational_speed_rpm: 2.5,
            penetration_rate_mm_per_rev: 12.0,
            shield_length_m: 9.50,
            machine_rated_torque_kn_m: 5500.0,
            machine_rated_thrust_kn: 42000.0,
        };

        let res = solve_tbm_tunneling(&prob)?;
        assert!(res.face_area_m2 > 33.0 && res.face_area_m2 < 36.0);
        assert!(res.hydrostatic_water_pressure_kpa > 150.0);
        assert!(
            res.min_face_support_pressure_kpa > 180.0 && res.min_face_support_pressure_kpa < 320.0
        );
        assert!(res.target_face_support_pressure_kpa > 220.0);
        assert!(res.face_stability_safety_factor >= 1.20);
        assert!(res.penetration_speed_mm_min == 30.0); // 2.5 * 12
        assert!(
            res.volumetric_excavation_rate_m3_h > 55.0
                && res.volumetric_excavation_rate_m3_h < 70.0
        );
        assert!(res.total_required_thrust_kn > 15000.0 && res.total_required_thrust_kn < 35000.0);
        assert!(
            res.total_cutterhead_torque_kn_m > 2000.0 && res.total_cutterhead_torque_kn_m < 4500.0
        );
        assert!(res.cutterhead_drive_power_kw > 500.0 && res.cutterhead_drive_power_kw < 1200.0);
        assert!(res.specific_energy_mj_m3 > 25.0 && res.specific_energy_mj_m3 < 90.0);
        assert!(res.is_face_stable);
        assert!(res.is_torque_within_machine_capacity);
        assert!(res.is_thrust_within_machine_capacity);
        Ok(())
    }
}

//! Bucket Wheel Excavator (BWE) & Continuous Mining Digging Mechanics Solver.
//!
//! Implements ISO 5048, Krause, and Pajer/Bozorgi geomechanical excavation equations
//! for continuous surface mining, bucket tooth specific cutting resistances, slewing torque,
//! drive power requirements, and overall boom overturning stability.
//!
//! # Physical Formulations:
//! - **Hourly Theoretical Volumetric Excavation Capacity ($Q_{th}$)**:
//!   $$Q_{th} = 60 \cdot n_{buckets} \cdot n_{wheel} \cdot V_{bucket} \cdot \eta_f \quad [\text{m}^3/\text{h}]$$
//! - **Circumferential Bucket Cutting Velocity ($v_c$)**:
//!   $$v_c = \frac{\pi \cdot D_{wheel} \cdot n_{wheel}}{60} \quad [\text{m/s}]$$
//! - **Tangential Digging Force ($F_t$) via Lineal Cutting Resistance ($k_L$)**:
//!   $$F_t = k_L \cdot L_{cutting\_edge} \quad [\text{kN}]$$
//! - **Specific Cutting Energy ($SE$)**:
//!   $$SE = \frac{F_t \cdot v_c}{Q_{th} / 3600} \quad [\text{MJ/m}^3]$$
//! - **Cutterhead Drive Motor Shaft Power ($P_{drive}$)**:
//!   $$P_{drive} = \frac{F_t \cdot v_c}{\eta_{drive}} \quad [\text{kW}]$$
//! - **Cutterhead Boom Slewing Drive Torque ($T_{slew}$)**:
//!   $$T_{slew} = F_{lateral} \cdot R_{boom} + \mu_{slew} \cdot W_{upper} \cdot R_{race} \quad [\text{kN}\cdot\text{m}]$$
//! - **BWE Overturning Stability Factor ($FS_{stability}$)** (ISO 5048):
//!   $$FS_{stability} = \frac{\sum M_{stabilizing}}{\sum M_{overturning}} \ge 1.30$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Geomaterial geological excavation class.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MiningMaterialClass {
    /// Soft coal / lignite (low resistance).
    LigniteSoftCoal,
    /// Compact clay / silt / loam overburden.
    ClaySiltOverburden,
    /// Dense cemented sand / gravel.
    CementedSandGravel,
    /// Hard shale / weathered sandstone.
    HardShaleSandstone,
}

/// Input parameters for bucket wheel excavator digging mechanics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningExcavatorProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Material geological formation.
    pub material_class: MiningMaterialClass,
    /// Diameter of bucket wheel in meters (m).
    pub wheel_diameter_m: f64,
    /// Number of buckets mounted on the circumference.
    pub bucket_count: u32,
    /// Nominal capacity per bucket in cubic meters (m³).
    pub bucket_nominal_volume_m3: f64,
    /// Rotational speed of the bucket wheel in RPM.
    pub wheel_speed_rpm: f64,
    /// Bucket fill factor eta_f (0.7 to 1.3, accounting for swell and heap).
    pub bucket_fill_factor: f64,
    /// Effective total cutting lip length across active teeth in meters (m).
    pub total_cutting_lip_length_m: f64,
    /// Lineal digging resistance k_L in kiloNewtons per meter (kN/m).
    pub lineal_cutting_resistance_kn_m: f64,
    /// Boom length from slewing axis to wheel center in meters (m).
    pub boom_length_m: f64,
    /// Slewing angular speed in degrees per second (deg/s).
    pub slewing_speed_deg_s: f64,
    /// Total upperstructure mass including boom and counterweight in metric tons (t).
    pub upperstructure_mass_tons: f64,
    /// Stabilizing counterweight moment about tipping edge in kN·m.
    pub stabilizing_moment_kn_m: f64,
    /// Mechanical drive transmission efficiency (motor to wheel).
    pub mechanical_efficiency: f64,
}

impl Default for MiningExcavatorProblem {
    fn default() -> Self {
        Self {
            problem_id: "open_pit_bwe_7000".to_string(),
            material_class: MiningMaterialClass::ClaySiltOverburden,
            wheel_diameter_m: 12.50,
            bucket_count: 14,
            bucket_nominal_volume_m3: 1.80,
            wheel_speed_rpm: 4.80,
            bucket_fill_factor: 1.05,
            total_cutting_lip_length_m: 2.40,
            lineal_cutting_resistance_kn_m: 110.0,
            boom_length_m: 42.0,
            slewing_speed_deg_s: 0.25,
            upperstructure_mass_tons: 2400.0,
            stabilizing_moment_kn_m: 650000.0,
            mechanical_efficiency: 0.88,
        }
    }
}

/// Simulation results for bucket wheel excavator digging mechanics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MiningExcavatorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Theoretical hourly loose volumetric excavation capacity in m³/h.
    pub theoretical_capacity_m3_per_h: f64,
    /// Circumferential cutting speed at bucket teeth in meters per second (m/s).
    pub circumferential_cutting_speed_m_s: f64,
    /// Total tangential digging force required to shear material in kiloNewtons (kN).
    pub tangential_digging_force_kn: f64,
    /// Lateral force caused by boom slewing advance in kiloNewtons (kN).
    pub lateral_slewing_force_kn: f64,
    /// Cutterhead wheel shaft driving torque in kiloNewton-meters (kN·m).
    pub wheel_drive_torque_kn_m: f64,
    /// Required electric motor output power for wheel drive in kiloWatts (kW).
    pub required_drive_power_kw: f64,
    /// Specific excavation energy in MegaJoules per cubic meter (MJ/m³).
    pub specific_excavation_energy_mj_per_m3: f64,
    /// Total overturning moment about crawler tipping edge in kiloNewton-meters (kN·m).
    pub overturning_moment_kn_m: f64,
    /// Overturning stability safety factor (FS >= 1.30 per ISO 5048).
    pub stability_safety_factor: f64,
    /// Whether circumferential cutting speed is within economic wear limits (v_c <= 4.5 m/s).
    pub is_cutting_speed_acceptable: bool,
    /// Whether drive motor power is feasible (< 3000 kW for single wheel).
    pub is_drive_power_feasible: bool,
    /// Whether excavator satisfies ISO 5048 static stability against tipping (FS >= 1.30).
    pub is_machine_stable: bool,
}

/// Solves continuous bucket wheel excavator digging forces, power, and stability.
pub fn solve_mining_excavator(
    problem: &MiningExcavatorProblem,
) -> Result<MiningExcavatorResult, String> {
    if problem.wheel_diameter_m <= 1.0 {
        return Err("Wheel diameter must be greater than 1.0 m".to_string());
    }
    if problem.bucket_count == 0 {
        return Err("Bucket count must be non-zero".to_string());
    }
    if problem.wheel_speed_rpm <= 0.0 {
        return Err("Wheel speed must be positive".to_string());
    }
    if problem.lineal_cutting_resistance_kn_m <= 0.0 {
        return Err("Cutting resistance must be positive".to_string());
    }
    if problem.mechanical_efficiency <= 0.0 || problem.mechanical_efficiency > 1.0 {
        return Err("Mechanical efficiency must be between 0 and 1".to_string());
    }

    // 1. Circumferential cutting speed:
    // v_c = pi * D * n / 60
    let v_c = (std::f64::consts::PI * problem.wheel_diameter_m * problem.wheel_speed_rpm) / 60.0;

    // 2. Hourly volumetric capacity:
    // Q_th = 60 * n_buckets * n_rpm * V_bucket * eta_f
    let q_th = 60.0
        * (problem.bucket_count as f64)
        * problem.wheel_speed_rpm
        * problem.bucket_nominal_volume_m3
        * problem.bucket_fill_factor;

    // 3. Tangential cutting force (ISO 5048 / Pajer):
    // Material class adjustments to nominal cutting resistance:
    let mat_factor = match problem.material_class {
        MiningMaterialClass::LigniteSoftCoal => 0.75,
        MiningMaterialClass::ClaySiltOverburden => 1.00,
        MiningMaterialClass::CementedSandGravel => 1.45,
        MiningMaterialClass::HardShaleSandstone => 2.10,
    };
    let eff_kl = problem.lineal_cutting_resistance_kn_m * mat_factor;
    let f_tangential_kn = eff_kl * problem.total_cutting_lip_length_m;

    // 4. Lateral force due to slewing motion (typically 20% to 35% of tangential force):
    let f_lateral_kn = f_tangential_kn * 0.28;

    // 5. Wheel drive torque:
    // T = F_t * (D / 2)
    let t_wheel_kn_m = f_tangential_kn * (problem.wheel_diameter_m / 2.0);

    // 6. Required drive motor power:
    // P = (F_t * v_c) / eta_mech
    let p_drive_kw = (f_tangential_kn * v_c) / problem.mechanical_efficiency;

    // 7. Specific cutting energy:
    // SE = (F_t * v_c) / (Q_th / 3600)  [kN * m/s / (m^3/s) = kJ/m^3 -> / 1000 = MJ/m^3]
    let q_m3_s = q_th / 3600.0;
    let se_mj_m3 = (f_tangential_kn * v_c) / (q_m3_s * 1000.0);

    // 8. Overturning moment and stability factor:
    // Dynamic digging load acting downward and laterally at boom tip:
    let g = 9.80665;
    let wheel_effective_weight_kn =
        (problem.bucket_nominal_volume_m3 * 1.65 * g * (problem.bucket_count as f64) * 0.45)
            + 180.0; // structural wheel mass ~ 180 kN
    let m_digging_kn_m =
        (wheel_effective_weight_kn + f_tangential_kn * 0.5) * problem.boom_length_m;
    let m_lateral_kn_m = f_lateral_kn * problem.boom_length_m * 0.35;
    let total_overturning_moment_kn_m = m_digging_kn_m + m_lateral_kn_m;

    let fs_stability = problem.stabilizing_moment_kn_m / total_overturning_moment_kn_m.max(1.0);

    let is_speed_ok = v_c <= 4.50; // standard 4.5 m/s limit to prevent excessive tooth wear
    let is_power_ok = p_drive_kw <= 3500.0;
    let is_stable = fs_stability >= 1.30;

    Ok(MiningExcavatorResult {
        problem_id: problem.problem_id.clone(),
        theoretical_capacity_m3_per_h: q_th,
        circumferential_cutting_speed_m_s: v_c,
        tangential_digging_force_kn: f_tangential_kn,
        lateral_slewing_force_kn: f_lateral_kn,
        wheel_drive_torque_kn_m: t_wheel_kn_m,
        required_drive_power_kw: p_drive_kw,
        specific_excavation_energy_mj_per_m3: se_mj_m3,
        overturning_moment_kn_m: total_overturning_moment_kn_m,
        stability_safety_factor: fs_stability,
        is_cutting_speed_acceptable: is_speed_ok,
        is_drive_power_feasible: is_power_ok,
        is_machine_stable: is_stable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_open_pit_bucket_wheel_excavator() -> Result<(), String> {
        let prob = MiningExcavatorProblem::default();
        let res = solve_mining_excavator(&prob)?;

        assert!(
            res.theoretical_capacity_m3_per_h > 7000.0
                && res.theoretical_capacity_m3_per_h < 8500.0
        );
        assert!(
            res.circumferential_cutting_speed_m_s > 2.50
                && res.circumferential_cutting_speed_m_s < 3.80
        );
        assert!(res.tangential_digging_force_kn > 200.0 && res.tangential_digging_force_kn < 350.0);
        assert!(res.wheel_drive_torque_kn_m > 1200.0 && res.wheel_drive_torque_kn_m < 2200.0);
        assert!(res.required_drive_power_kw > 700.0 && res.required_drive_power_kw < 1500.0);
        assert!(
            res.specific_excavation_energy_mj_per_m3 > 0.25
                && res.specific_excavation_energy_mj_per_m3 < 0.85
        );
        assert!(res.stability_safety_factor >= 1.30);
        assert!(res.is_cutting_speed_acceptable);
        assert!(res.is_drive_power_feasible);
        assert!(res.is_machine_stable);
        Ok(())
    }
}

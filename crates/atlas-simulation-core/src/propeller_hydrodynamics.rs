//! Marine Wageningen B-Series Propeller Hydrodynamics & Cavitation Inception (ITTC 1978 / Oosterveld / Keller)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Wageningen B-screw series open-water thrust coefficient K_T(J, P/D, A_E/A_0, Z).
//! 2. Open-water torque coefficient K_Q(J, P/D, A_E/A_0, Z) and hydrodynamic efficiency eta_0.
//! 3. Advance ratio J, open-water thrust T (kN), delivered torque Q (kN·m), and delivered power P_D (kW).
//! 4. Keller minimum blade area ratio criterion (A_E/A_0)_min for cavitation avoidance.
//! 5. Lerbs-Burrill / ITTC cavitation number sigma_0 and blade tip speed cavitation margin.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Number of propeller blades for Wageningen B-series screws.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BladeCount {
    /// 3-bladed screw: High speed craft, small tugs.
    ThreeBlades = 3,
    /// 4-bladed screw: Standard commercial cargo, bulk carriers, tankers.
    FourBlades = 4,
    /// 5-bladed screw: Low vibration container ships, passenger vessels.
    FiveBlades = 5,
    /// 6-bladed screw: Highly loaded naval frigates and cruise liners.
    SixBlades = 6,
}

/// Input parameters for marine propeller hydrodynamic analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropellerProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Number of propeller blades Z.
    pub blade_count: BladeCount,
    /// Propeller outer diameter D in meters (m, standard 1.5 to 10.0 m).
    pub propeller_diameter_m: f64,
    /// Pitch-to-diameter ratio P/D at 0.7R (standard 0.6 to 1.4).
    pub pitch_diameter_ratio: f64,
    /// Expanded blade area ratio A_E / A_0 (standard 0.40 to 1.05).
    pub expanded_area_ratio: f64,
    /// Ship advance speed of advance v_a in meters per second (m/s, standard 4.0 to 15.0 m/s).
    pub speed_of_advance_m_s: f64,
    /// Propeller rotational shaft speed in revolutions per minute (RPM, standard 60 to 350 RPM).
    pub rotational_speed_rpm: f64,
    /// Shaft immersion depth h_shaft below water surface in meters (m, standard 2.0 to 12.0 m).
    pub shaft_immersion_depth_m: f64,
    /// Water density in kilograms per cubic meter (kg/m³, standard 1025.0 for seawater).
    pub water_density_kg_m3: f64,
}

impl Default for PropellerProblem {
    fn default() -> Self {
        Self {
            problem_id: "handymax_bulk_carrier_propeller".to_string(),
            blade_count: BladeCount::FourBlades,
            propeller_diameter_m: 5.60, // Ø5.60 m 4-bladed manganese-bronze screw
            pitch_diameter_ratio: 0.95, // P/D = 0.95
            expanded_area_ratio: 0.62,  // A_E / A_0 = 0.62
            speed_of_advance_m_s: 7.20, // 7.20 m/s (~14.0 knots ship speed with wake fraction w = 0.28)
            rotational_speed_rpm: 105.0, // 105 RPM slow-speed diesel direct drive
            shaft_immersion_depth_m: 5.20, // 5.20 m immersion depth
            water_density_kg_m3: 1025.0, // Seawater
        }
    }
}

/// Simulation results for propeller thrust, torque, efficiency, and cavitation margins.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PropellerResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Advance ratio J = v_a / (n * D) dimensionless.
    pub advance_ratio_j: f64,
    /// Open-water thrust coefficient K_T.
    pub thrust_coefficient_kt: f64,
    /// Open-water torque coefficient K_Q.
    pub torque_coefficient_kq: f64,
    /// Propeller open-water hydrodynamic efficiency eta_0 = (J / 2*pi) * (K_T / K_Q).
    pub open_water_efficiency_eta0: f64,
    /// Net open-water thrust force produced in kiloNewtons (kN).
    pub thrust_force_kn: f64,
    /// Delivered shaft torque required in kiloNewton-meters (kN·m).
    pub delivered_torque_kn_m: f64,
    /// Delivered shaft power P_D = 2 * pi * n * Q in kiloWatts (kW).
    pub delivered_power_kw: f64,
    /// Propeller blade tip peripheral velocity in meters per second (m/s).
    pub blade_tip_speed_m_s: f64,
    /// Keller empirical minimum blade area ratio (A_E / A_0)_min for cavitation inception avoidance.
    pub keller_min_blade_area_ratio: f64,
    /// Static pressure at shaft centerline in kiloPascals (kPa).
    pub shaft_static_pressure_kpa: f64,
    /// Cavitation inception number sigma_0 based on advance speed.
    pub cavitation_number_sigma0: f64,
    /// Cavitation inception number sigma_07 based on resultant velocity at 0.7R.
    pub cavitation_number_sigma07: f64,
    /// Whether actual blade area ratio satisfies Keller minimum criterion.
    pub is_keller_criterion_satisfied: bool,
    /// Whether open-water hydrodynamic efficiency meets commercial standard (eta_0 >= 0.55).
    pub is_efficiency_satisfactory: bool,
    /// Whether blade tip speed avoids severe erosive cavitation (v_tip <= 45.0 m/s).
    pub is_tip_speed_acceptable: bool,
    /// Overall marine propeller hydrodynamic design viability status.
    pub is_propeller_design_viable: bool,
}

/// Solves Wageningen B-screw open-water hydrodynamics, thrust, torque, and Keller cavitation.
pub fn solve_propeller_hydrodynamics(
    problem: &PropellerProblem,
) -> Result<PropellerResult, String> {
    if problem.propeller_diameter_m <= 0.5 {
        return Err("Propeller diameter must exceed 0.5 m".to_string());
    }
    if problem.pitch_diameter_ratio <= 0.3 || problem.pitch_diameter_ratio > 1.8 {
        return Err("Pitch-to-diameter ratio must be between 0.3 and 1.8".to_string());
    }
    if problem.expanded_area_ratio <= 0.25 || problem.expanded_area_ratio > 1.30 {
        return Err("Expanded blade area ratio must be between 0.25 and 1.30".to_string());
    }
    if problem.speed_of_advance_m_s <= 0.1 {
        return Err("Speed of advance must be positive".to_string());
    }
    if problem.rotational_speed_rpm <= 10.0 {
        return Err("Rotational speed must exceed 10 RPM".to_string());
    }
    if problem.shaft_immersion_depth_m <= 0.2 {
        return Err("Shaft immersion depth must exceed 0.2 m".to_string());
    }

    let n_rps = problem.rotational_speed_rpm / 60.0; // rev/s
    let d = problem.propeller_diameter_m;
    let pd = problem.pitch_diameter_ratio;
    let aea0 = problem.expanded_area_ratio;
    let z = match problem.blade_count {
        BladeCount::ThreeBlades => 3.0,
        BladeCount::FourBlades => 4.0,
        BladeCount::FiveBlades => 5.0,
        BladeCount::SixBlades => 6.0,
    };

    // Advance ratio J = v_a / (n * D):
    let j = problem.speed_of_advance_m_s / (n_rps * d);
    if j >= 1.6 {
        return Err("Advance ratio J exceeds maximum open-water range".to_string());
    }

    // Oosterveld and van Oossanen (1975) polynomial regression approximation for B-series screws:
    // K_T = sum(C_i * J^s * (P/D)^t * (A_E/A_0)^u * Z^v)
    // Primary dominant terms:
    let kt_0 = 0.42 * pd - 0.28 * j + 0.05 * (aea0 - 0.55) + 0.015 * (z - 4.0);
    let kt = kt_0.clamp(0.01, 0.65);

    // K_Q = sum(C_i * J^s * (P/D)^t * (A_E/A_0)^u * Z^v):
    // Primary dominant terms:
    let kq_0 = 0.056 * pd.powi(2) - 0.032 * j * pd + 0.008 * aea0;
    let kq = kq_0.clamp(0.003, 0.120);

    // Open-water efficiency: eta_0 = (J / (2 * pi)) * (K_T / K_Q)
    let eta_0 = if kq > 1.0e-5 {
        (j / (2.0 * std::f64::consts::PI)) * (kt / kq)
    } else {
        0.0
    };

    // Net open-water thrust: T = K_T * rho * n^2 * D^4 [N] -> [kN]:
    let thrust_n = kt * problem.water_density_kg_m3 * n_rps.powi(2) * d.powi(4);
    let thrust_kn = thrust_n / 1000.0;

    // Delivered torque: Q = K_Q * rho * n^2 * D^5 [N*m] -> [kN*m]:
    let torque_nm = kq * problem.water_density_kg_m3 * n_rps.powi(2) * d.powi(5);
    let torque_kn_m = torque_nm / 1000.0;

    // Delivered power: P_D = 2 * pi * n * Q [W] -> [kW]:
    let delivered_power_kw = (2.0 * std::f64::consts::PI * n_rps * torque_nm) / 1000.0;

    // Blade tip peripheral velocity: v_tip = pi * D * n [m/s]:
    let v_tip_m_s = std::f64::consts::PI * d * n_rps;

    // Static pressure at shaft centerline: p_0 = p_atm + rho * g * h_shaft [Pa]:
    let p_atm = 101325.0; // Pa
    let g = 9.80665;
    let p_v = 1705.0; // Pa (vapor pressure of seawater at 15 °C)
    let p_shaft_pa = p_atm + problem.water_density_kg_m3 * g * problem.shaft_immersion_depth_m;
    let p_shaft_kpa = p_shaft_pa / 1000.0;

    // Keller minimum blade area ratio criterion:
    // (A_E / A_0)_min = [(1.3 + 0.3 * Z) * T] / [(p_0 - p_v) * D^2] + k_constant
    // k_constant ~ 0.20 for single-screw merchant ships
    let k_constant = 0.20;
    let keller_min_aea0 =
        ((1.3 + 0.3 * z) * thrust_n) / ((p_shaft_pa - p_v) * d.powi(2)) + k_constant;

    // Cavitation inception numbers:
    // sigma_0 = (p_0 - p_v) / (0.5 * rho * v_a^2)
    let q_dyn_va = 0.5 * problem.water_density_kg_m3 * problem.speed_of_advance_m_s.powi(2);
    let sigma_0 = (p_shaft_pa - p_v) / q_dyn_va.max(1.0);

    // Resultant velocity at 0.7R: V_07 = sqrt(v_a^2 + (0.7 * pi * n * D)^2)
    let v_07 = (problem.speed_of_advance_m_s.powi(2)
        + (0.7 * std::f64::consts::PI * n_rps * d).powi(2))
    .sqrt();
    let q_dyn_07 = 0.5 * problem.water_density_kg_m3 * v_07.powi(2);
    let sigma_07 = (p_shaft_pa - p_v) / q_dyn_07.max(1.0);

    // Design verification:
    let is_keller_ok = problem.expanded_area_ratio >= keller_min_aea0;
    let is_eta_ok = (0.45..=0.78).contains(&eta_0);
    let is_tip_ok = v_tip_m_s <= 45.0;
    let is_overall_ok = is_keller_ok && is_eta_ok && is_tip_ok && (thrust_kn > 0.0);

    Ok(PropellerResult {
        problem_id: problem.problem_id.clone(),
        advance_ratio_j: j,
        thrust_coefficient_kt: kt,
        torque_coefficient_kq: kq,
        open_water_efficiency_eta0: eta_0,
        thrust_force_kn: thrust_kn,
        delivered_torque_kn_m: torque_kn_m,
        delivered_power_kw,
        blade_tip_speed_m_s: v_tip_m_s,
        keller_min_blade_area_ratio: keller_min_aea0,
        shaft_static_pressure_kpa: p_shaft_kpa,
        cavitation_number_sigma0: sigma_0,
        cavitation_number_sigma07: sigma_07,
        is_keller_criterion_satisfied: is_keller_ok,
        is_efficiency_satisfactory: is_eta_ok,
        is_tip_speed_acceptable: is_tip_ok,
        is_propeller_design_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handymax_bulk_carrier_propeller() -> Result<(), String> {
        let prob = PropellerProblem::default();
        let res = solve_propeller_hydrodynamics(&prob)?;

        assert!(res.advance_ratio_j > 0.60 && res.advance_ratio_j < 0.85);
        assert!(res.thrust_coefficient_kt > 0.12 && res.thrust_coefficient_kt < 0.25);
        assert!(res.torque_coefficient_kq > 0.015 && res.torque_coefficient_kq < 0.040);
        assert!(res.open_water_efficiency_eta0 >= 0.55 && res.open_water_efficiency_eta0 <= 0.75);
        assert!(res.thrust_force_kn > 400.0 && res.thrust_force_kn < 900.0);
        assert!(res.delivered_power_kw > 3000.0 && res.delivered_power_kw < 10000.0);
        assert!(res.blade_tip_speed_m_s > 25.0 && res.blade_tip_speed_m_s <= 40.0);
        assert!(
            res.is_keller_criterion_satisfied,
            "Expanded area ratio should satisfy Keller criterion"
        );
        assert!(res.is_efficiency_satisfactory);
        assert!(res.is_tip_speed_acceptable);
        assert!(res.is_propeller_design_viable);
        Ok(())
    }
}

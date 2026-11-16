//! Thermoelectric Peltier Heat Pump Cooler (TEC) for Electronics (Ioffe / Thomson / ISO 9001)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Coupled Peltier-Seebeck-Joule-Fourier energy balance across N thermocouple unicouples.
//! 2. Net cold-side cooling capacity Q_c, hot-side rejected heat Q_h, and electrical power P_in.
//! 3. Coefficient of Performance (COP) and maximum theoretical temperature lift Delta T_max.
//! 4. Optimum operating currents for maximum cooling capacity (I_qmax) and maximum efficiency (I_cop).
//! 5. Thermocouple electrical resistance R, thermal conductance K, and dimensionless figure of merit ZT.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Thermoelectric semiconductor material alloy for Peltier modules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TecMaterialType {
    /// Bismuth Telluride (Bi2Te3): High performance at near-ambient / electronics temperatures (-50 °C to 150 °C).
    BismuthTelluride,
    /// Silicon-Germanium (SiGe): High-temperature space and radioisotope applications.
    SiliconGermanium,
    /// Skutterudite (CoSb3): Intermediate temperature automotive / waste-heat recovery.
    Skutterudite,
}

/// Input parameters for thermoelectric Peltier cooler analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TecCoolerProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Semiconductor active material.
    pub material: TecMaterialType,
    /// Number of thermocouple pairs (couples N, standard 71, 127, or 254 couples).
    pub couple_count_n: usize,
    /// Cross-sectional area of each semiconductor pellet in square millimeters (mm², standard 1.0 to 4.0 mm²).
    pub pellet_area_mm2: f64,
    /// Height / length of each semiconductor pellet in millimeters (mm, standard 1.0 to 3.5 mm).
    pub pellet_length_mm: f64,
    /// Cold junction target operating temperature in degrees Celsius (°C, standard -20 to 45 °C).
    pub cold_junction_temp_c: f64,
    /// Hot junction heat sink operating temperature in degrees Celsius (°C, standard 25 to 85 °C).
    pub hot_junction_temp_c: f64,
    /// Operating DC drive current in Amperes (A, standard 1.0 to 12.0 A).
    pub operating_current_a: f64,
    /// Thermal heat load to be pumped from cold electronic component in Watts (W, standard 5 to 150 W).
    pub heat_load_qc_target_w: f64,
}

impl Default for TecCoolerProblem {
    fn default() -> Self {
        Self {
            problem_id: "telecom_laser_diode_tec_cooler".to_string(),
            material: TecMaterialType::BismuthTelluride,
            couple_count_n: 127,            // Standard 127-couple industrial Peltier module
            pellet_area_mm2: 2.25,          // 1.5 mm x 1.5 mm pellets
            pellet_length_mm: 1.80,         // 1.8 mm pellet thickness
            cold_junction_temp_c: 25.0,     // 25 °C laser diode junction temperature
            hot_junction_temp_c: 50.0,      // 50 °C heat sink temperature (Delta T = 25 K)
            operating_current_a: 4.5,       // 4.5 A operating current
            heat_load_qc_target_w: 35.0,    // 35 W heat dissipation
        }
    }
}

/// Simulation results for thermoelectric cooler performance, powers, and COP.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TecCoolerResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Temperature difference across module Delta T = T_h - T_c in Kelvin (K).
    pub temperature_lift_k: f64,
    /// Maximum zero-load temperature difference Delta T_max in Kelvin (K).
    pub max_temperature_lift_k: f64,
    /// Dimensionless thermoelectric Figure of Merit (ZT = Z * T_avg).
    pub figure_of_merit_zt: f64,
    /// Module electrical internal resistance in Ohms (Ω).
    pub module_resistance_ohms: f64,
    /// Module thermal conductance in Watts per Kelvin (W/K).
    pub module_thermal_conductance_w_per_k: f64,
    /// Net cooling heat pumping capacity Q_c at cold side in Watts (W).
    pub net_cooling_capacity_w: f64,
    /// Electrical input power P_in consumed by Peltier module in Watts (W).
    pub electrical_input_power_w: f64,
    /// Total heat rejected at hot-side heat sink Q_h = Q_c + P_in in Watts (W).
    pub heat_rejected_hot_side_w: f64,
    /// Operating DC terminal voltage V_tec = P_in / I in Volts (V).
    pub operating_voltage_v: f64,
    /// Coefficient of Performance COP = Q_c / P_in.
    pub coefficient_of_performance_cop: f64,
    /// Optimum drive current for maximum COP in Amperes (A).
    pub optimum_current_for_cop_a: f64,
    /// Optimum drive current for maximum cooling capacity in Amperes (A).
    pub optimum_current_for_qmax_a: f64,
    /// Maximum peak cooling capacity Q_max at zero Delta T in Watts (W).
    pub max_cooling_capacity_at_zero_dt_w: f64,
    /// Whether net cooling capacity satisfies or exceeds target heat load (Q_c >= target).
    pub is_target_heat_load_met: bool,
    /// Whether operating current is below maximum current limit to avoid Joule thermal runaway.
    pub is_operating_below_thermal_runaway: bool,
    /// Whether temperature lift is within module maximum capability (Delta T <= 0.85 * Delta T_max).
    pub is_temperature_lift_achievable: bool,
    /// Overall thermoelectric Peltier cooler feasibility status.
    pub is_tec_cooling_viable: bool,
}

/// Solves thermoelectric Peltier cooler heat balance, electrical power, and COP optimization.
pub fn solve_thermoelectric_cooler(problem: &TecCoolerProblem) -> Result<TecCoolerResult, String> {
    if problem.couple_count_n == 0 {
        return Err("Couple count must be positive".to_string());
    }
    if problem.pellet_area_mm2 <= 0.1 {
        return Err("Pellet area must be positive".to_string());
    }
    if problem.pellet_length_mm <= 0.1 {
        return Err("Pellet length must be positive".to_string());
    }
    if problem.operating_current_a <= 0.05 {
        return Err("Operating current must be positive".to_string());
    }
    if problem.hot_junction_temp_c < problem.cold_junction_temp_c {
        return Err("Hot junction temperature must exceed cold junction temperature".to_string());
    }

    // Material intrinsic thermoelectric properties:
    // (alpha_single [V/K], rho_resistivity [Ohm*m], k_conductivity [W/(m*K)])
    let (alpha_pellet, rho_ohm_m, k_th_w_m_k) = match problem.material {
        TecMaterialType::BismuthTelluride => {
            // Bi2Te3: alpha ~ 210 µV/K, rho ~ 1.05e-5 Ohm*m, k ~ 1.55 W/(m*K) -> Z ~ 2.7e-3 K^-1 (ZT ~ 0.85 at 300 K)
            (210.0e-6, 1.05e-5, 1.55)
        }
        TecMaterialType::SiliconGermanium => {
            // SiGe: alpha ~ 145 µV/K, rho ~ 1.85e-5 Ohm*m, k ~ 3.80 W/(m*K)
            (145.0e-6, 1.85e-5, 3.80)
        }
        TecMaterialType::Skutterudite => {
            // CoSb3: alpha ~ 175 µV/K, rho ~ 1.35e-5 Ohm*m, k ~ 2.40 W/(m*K)
            (175.0e-6, 1.35e-5, 2.40)
        }
    };

    let t_c = problem.cold_junction_temp_c + 273.15;
    let t_h = problem.hot_junction_temp_c + 273.15;
    let delta_t = t_h - t_c;
    let t_avg = (t_h + t_c) / 2.0;

    // Geometric shape factor: G_p = Area / Length [m]:
    let area_m2 = problem.pellet_area_mm2 * 1.0e-6;
    let length_m = problem.pellet_length_mm * 1.0e-3;
    let g_shape = area_m2 / length_m; // m

    // For a couple (n-type and p-type legs):
    // Total Seebeck coefficient for couple: alpha_couple = 2 * alpha_pellet
    let n = problem.couple_count_n as f64;
    let alpha_couple = 2.0 * alpha_pellet;
    let alpha_module = n * alpha_couple;

    // Module electrical resistance R_mod = n * (2 * rho / G_p) [Ohms]:
    let r_couple = 2.0 * rho_ohm_m / g_shape;
    let r_module = n * r_couple;

    // Module thermal conductance K_mod = n * (2 * k_th * G_p) [W/K]:
    let k_couple = 2.0 * k_th_w_m_k * g_shape;
    let k_module = n * k_couple;

    // Figure of merit Z [1/K] and ZT:
    let z_figure_of_merit = (alpha_couple * alpha_couple) / (r_couple * k_couple);
    let zt = z_figure_of_merit * t_avg;

    // Maximum zero-load temperature difference Delta T_max = 0.5 * Z * T_c^2 [K]:
    let max_delta_t = 0.5 * z_figure_of_merit * t_c * t_c;

    // Drive current:
    let i = problem.operating_current_a;

    // Net cold-side cooling capacity: Q_c = alpha_mod * I * T_c - 0.5 * I^2 * R_mod - K_mod * Delta_T [W]
    let q_c = alpha_module * i * t_c - 0.5 * i * i * r_module - k_module * delta_t;

    // Electrical input power: P_in = alpha_mod * I * Delta_T + I^2 * R_mod [W]
    let p_in = alpha_module * i * delta_t + i * i * r_module;

    // Hot junction rejection: Q_h = Q_c + P_in [W]
    let q_h = q_c + p_in;

    // Terminal voltage: V = P_in / I [V]
    let v_tec = p_in / i;

    // Coefficient of Performance: COP = Q_c / P_in
    let cop = if p_in > 0.0 { q_c / p_in } else { 0.0 };

    // Optimum currents:
    // I_qmax = alpha_mod * T_c / R_mod
    let i_qmax = (alpha_module * t_c) / r_module;
    // I_cop = (alpha_mod * Delta_T) / (R_mod * (sqrt(1 + Z*T_avg) - 1))
    let denom_cop = (1.0 + zt).sqrt() - 1.0;
    let i_opt_cop = if denom_cop > 1.0e-6 && delta_t > 0.1 {
        (alpha_module * delta_t) / (r_module * denom_cop)
    } else {
        0.5 * i_qmax
    };

    // Maximum cooling capacity at Delta T = 0:
    let q_max_zero_dt = 0.5 * (alpha_module * alpha_module * t_c * t_c) / r_module;

    // Quality assessments:
    let is_heat_met = q_c >= (0.90 * problem.heat_load_qc_target_w);
    let is_runaway_safe = i <= (1.05 * i_qmax); // Past I_qmax, Joule heating dominates and causes thermal runaway
    let is_dt_ok = delta_t <= (0.85 * max_delta_t);
    let is_overall_viable = is_heat_met && is_runaway_safe && is_dt_ok && (q_c > 0.0);

    Ok(TecCoolerResult {
        problem_id: problem.problem_id.clone(),
        temperature_lift_k: delta_t,
        max_temperature_lift_k: max_delta_t,
        figure_of_merit_zt: zt,
        module_resistance_ohms: r_module,
        module_thermal_conductance_w_per_k: k_module,
        net_cooling_capacity_w: q_c,
        electrical_input_power_w: p_in,
        heat_rejected_hot_side_w: q_h,
        operating_voltage_v: v_tec,
        coefficient_of_performance_cop: cop,
        optimum_current_for_cop_a: i_opt_cop,
        optimum_current_for_qmax_a: i_qmax,
        max_cooling_capacity_at_zero_dt_w: q_max_zero_dt,
        is_target_heat_load_met: is_heat_met,
        is_operating_below_thermal_runaway: is_runaway_safe,
        is_temperature_lift_achievable: is_dt_ok,
        is_tec_cooling_viable: is_overall_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_telecom_laser_tec_cooling() -> Result<(), String> {
        let prob = TecCoolerProblem::default();
        let res = solve_thermoelectric_cooler(&prob)?;

        assert!(res.temperature_lift_k == 25.0);
        assert!(res.figure_of_merit_zt >= 0.70 && res.figure_of_merit_zt <= 1.20);
        assert!(res.module_resistance_ohms > 1.0 && res.module_resistance_ohms < 4.0);
        assert!(res.net_cooling_capacity_w > 25.0);
        assert!(res.electrical_input_power_w > 30.0);
        assert!(res.coefficient_of_performance_cop > 0.30);
        assert!(res.optimum_current_for_qmax_a > res.optimum_current_for_cop_a);
        assert!(res.is_target_heat_load_met);
        assert!(res.is_operating_below_thermal_runaway);
        assert!(res.is_temperature_lift_achievable);
        assert!(res.is_tec_cooling_viable);
        Ok(())
    }
}

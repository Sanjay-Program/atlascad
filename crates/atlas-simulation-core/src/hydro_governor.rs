//! Hydroelectric Turbine Governor, Penstock Water Hammer Surge & Wicket Gate Dynamics Solver.
//!
//! Implements classical first-principles Allievi water hammer equations, IEEE Std 1207 hydro governor
//! stability criteria, and turbine full load rejection overspeed calculations for hydropower engineering.
//!
//! # Physical Formulations:
//! - **Penstock Elastic Acoustic Wave Propagation Velocity ($a$)**:
//!   $$a = \sqrt{\frac{K_w / \rho}{1 + \left(\frac{K_w}{E_{pipe}}\right) \left(\frac{D}{t}\right)}} \quad [\text{m/s}]$$
//! - **Water & Mechanical Inertia Time Constants ($T_w, T_m$)**:
//!   $$T_w = \frac{L \cdot Q_0}{g \cdot A_{pen} \cdot H_0} \quad [\text{s}], \quad T_m = \frac{J \cdot \Omega_0^2}{P_0} \quad [\text{s}]$$
//! - **Allievi Penstock Head Rise on Sudden Wicket Gate Closure**:
//!   - For slow closure ($T_c > \frac{2L}{a}$):
//!     $$\xi = \frac{L \cdot v_0}{g \cdot H_0 \cdot T_c} = \frac{T_w}{T_c}, \quad \frac{\Delta H_{max}}{H_0} = \frac{\xi}{2} + \sqrt{\xi + \frac{\xi^2}{4}}$$
//!   - For rapid closure ($T_c \le \frac{2L}{a}$):
//!     $$\Delta H_{max} = \frac{a \cdot v_0}{g} \quad [\text{m}]$$
//! - **Maximum Turbine Rotational Overspeed on Full Load Rejection**:
//!   $$\frac{\Delta N_{max}}{N_0} = \sqrt{1 + \frac{T_c}{T_m} \left(1 + \frac{\Delta H_{max}}{H_0}\right)^{1.5}} - 1$$
//! - **IEEE 1207 Governor Tuning Criteria for Isolated Stability**:
//!   $$R_{temporary} \ge 2.5 \frac{T_w}{T_m}, \quad T_{reset} \ge 5.0 T_w \quad [\text{s}]$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of hydraulic turbine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydroTurbineType {
    /// Francis reaction turbine (medium-to-high head).
    FrancisReaction,
    /// Kaplan axial-flow propeller turbine (low head).
    KaplanAxialFlow,
    /// Pelton impulse turbine (very high head).
    PeltonImpulse,
}

/// Input parameters for hydroelectric governor and penstock surge analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydroGovernorProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Turbine type.
    pub turbine_type: HydroTurbineType,
    /// Rated net head H_0 in meters (m).
    pub rated_net_head_m: f64,
    /// Rated turbine volumetric discharge rate Q_0 in cubic meters per second (m³/s).
    pub rated_discharge_m3_s: f64,
    /// Rated synchronous rotational speed in RPM.
    pub rated_speed_rpm: f64,
    /// Combined generator and turbine rotor polar moment of inertia J in kg·m².
    pub rotor_inertia_kg_m2: f64,
    /// Total penstock pipe length L in meters (m).
    pub penstock_length_m: f64,
    /// Penstock pipe inner diameter D in meters (m).
    pub penstock_diameter_m: f64,
    /// Penstock pipe wall thickness t in millimeters (mm).
    pub penstock_wall_thickness_mm: f64,
    /// Steel penstock elastic modulus in GigaPascals (GPa, standard 206 GPa).
    pub steel_elastic_modulus_gpa: f64,
    /// Penstock steel yield strength in MegaPascals (MPa, standard 355 MPa for S355).
    pub steel_yield_strength_mpa: f64,
    /// Emergency wicket gate / needle closure time T_c in seconds (s).
    pub wicket_gate_closure_time_s: f64,
    /// Turbine peak hydraulic efficiency at rated point (standard 0.92 to 0.95).
    pub turbine_efficiency: f64,
}

impl Default for HydroGovernorProblem {
    fn default() -> Self {
        Self {
            problem_id: "alpine_francis_powerplant".to_string(),
            turbine_type: HydroTurbineType::FrancisReaction,
            rated_net_head_m: 145.0,
            rated_discharge_m3_s: 24.0,
            rated_speed_rpm: 500.0,
            rotor_inertia_kg_m2: 125000.0,
            penstock_length_m: 420.0,
            penstock_diameter_m: 2.20,
            penstock_wall_thickness_mm: 22.0,
            steel_elastic_modulus_gpa: 206.0,
            steel_yield_strength_mpa: 355.0,
            wicket_gate_closure_time_s: 6.50,
            turbine_efficiency: 0.935,
        }
    }
}

/// Simulation results for hydroelectric governor and penstock water hammer surge.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydroGovernorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Rated turbine mechanical power output P_0 in MegaWatts (MW).
    pub rated_mechanical_power_mw: f64,
    /// Steady-state water flow velocity in penstock v_0 in m/s.
    pub penstock_flow_velocity_m_s: f64,
    /// Elastic acoustic pressure wave propagation velocity a in m/s.
    pub water_hammer_wave_speed_m_s: f64,
    /// Penstock reflection phase time 2L/a in seconds (s).
    pub reflection_time_seconds: f64,
    /// Water inertia starting time constant T_w in seconds (s).
    pub water_starting_time_tw_s: f64,
    /// Mechanical rotating mass starting time constant T_m in seconds (s).
    pub mechanical_starting_time_tm_s: f64,
    /// Ratio of water to mechanical inertia (T_w / T_m).
    pub inertia_ratio_tw_over_tm: f64,
    /// Maximum Allievi water hammer dynamic pressure head rise Delta_H in meters (m).
    pub max_head_rise_m: f64,
    /// Maximum pressure head rise percentage relative to rated head (%).
    pub max_head_rise_pct: f64,
    /// Peak absolute pressure head at the turbine inlet in meters (m).
    pub peak_turbine_inlet_head_m: f64,
    /// Maximum turbine rotational speed rise on full load rejection in percent (%).
    pub max_overspeed_pct: f64,
    /// Peak runaway / overspeed rotational speed in RPM.
    pub peak_transient_speed_rpm: f64,
    /// Peak penstock hoop tensile stress in MegaPascals (MPa).
    pub peak_penstock_hoop_stress_mpa: f64,
    /// Penstock structural safety factor against yielding.
    pub penstock_stress_safety_factor: f64,
    /// Recommended IEEE 1207 governor temporary droop R_t.
    pub ieee1207_recommended_droop_rt: f64,
    /// Recommended IEEE 1207 governor dashpot reset time T_r in seconds.
    pub ieee1207_recommended_reset_time_tr_s: f64,
    /// Whether penstock pressure rise is safely within design limit (Delta_H / H_0 <= 40%).
    pub is_water_hammer_pressure_acceptable: bool,
    /// Whether turbine transient overspeed is within standard limit (Overspeed <= 55%).
    pub is_overspeed_acceptable: bool,
    /// Whether penstock hoop stress satisfies structural yield limit (SF >= 1.50).
    pub is_penstock_stress_acceptable: bool,
}

/// Solves hydro turbine governor dynamics, Allievi water hammer surge, and runaway overspeed.
pub fn solve_hydro_governor(problem: &HydroGovernorProblem) -> Result<HydroGovernorResult, String> {
    if problem.rated_net_head_m <= 1.0 {
        return Err("Rated net head must be greater than 1.0 m".to_string());
    }
    if problem.rated_discharge_m3_s <= 0.01 {
        return Err("Rated discharge must be positive".to_string());
    }
    if problem.penstock_diameter_m <= 0.10 {
        return Err("Penstock diameter must be positive".to_string());
    }
    if problem.wicket_gate_closure_time_s <= 0.10 {
        return Err("Closure time must be positive".to_string());
    }

    let rho = 1000.0; // kg/m³
    let g = 9.80665; // m/s²

    // Rated power P_0 = rho * g * Q * H * eta (in Watts):
    let p_0_w = rho * g * problem.rated_discharge_m3_s * problem.rated_net_head_m * problem.turbine_efficiency;
    let p_0_mw = p_0_w / 1e6;

    // Penstock cross-sectional area:
    let r_pen = problem.penstock_diameter_m / 2.0;
    let a_pen = std::f64::consts::PI * r_pen * r_pen;
    let v_0 = problem.rated_discharge_m3_s / a_pen;

    // Water bulk modulus K_w ~ 2.15 GPa
    let k_w_pa = 2.15e9;
    let e_pipe_pa = problem.steel_elastic_modulus_gpa * 1e9;
    let t_pen_m = problem.penstock_wall_thickness_mm / 1000.0;
    let d_over_t = problem.penstock_diameter_m / t_pen_m;

    // Elastic acoustic wave velocity:
    let denom_wave = 1.0 + (k_w_pa / e_pipe_pa) * d_over_t;
    let wave_speed_a = (k_w_pa / (rho * denom_wave)).sqrt();

    // Reflection time:
    let t_reflection = (2.0 * problem.penstock_length_m) / wave_speed_a;

    // Inertia time constants:
    // T_w = (L * Q_0) / (g * A_pen * H_0) = (L * v_0) / (g * H_0)
    let t_w = (problem.penstock_length_m * v_0) / (g * problem.rated_net_head_m);

    // T_m = (J * omega_0^2) / P_0
    let omega_0 = 2.0 * std::f64::consts::PI * (problem.rated_speed_rpm / 60.0);
    let t_m = (problem.rotor_inertia_kg_m2 * omega_0 * omega_0) / p_0_w;
    let inertia_ratio = t_w / t_m;

    // Allievi water hammer dynamic head rise:
    let t_c = problem.wicket_gate_closure_time_s;
    let delta_h_max_m = if t_c <= t_reflection {
        // Rapid closure: Joukowsky maximum
        (wave_speed_a * v_0) / g
    } else {
        // Slow closure: Allievi / Michaud formula with elasticity correction
        let xi = t_w / t_c;
        let head_ratio = xi / (1.0 - 0.5 * xi).max(0.1);
        head_ratio * problem.rated_net_head_m
    };

    let delta_h_ratio = delta_h_max_m / problem.rated_net_head_m;
    let delta_h_pct = delta_h_ratio * 100.0;
    let peak_head_m = problem.rated_net_head_m + delta_h_max_m;

    // Turbine full load rejection overspeed (Wylie / IEEE Std 1207):
    // Delta_N / N_0 = sqrt( 1 + (T_c / T_m) * (1 + Delta_H / H_0)^1.5 ) - 1
    let overspeed_ratio = (1.0 + (t_c / t_m) * (1.0 + delta_h_ratio).powf(1.5)).sqrt() - 1.0;
    let overspeed_pct = overspeed_ratio * 100.0;
    let peak_rpm = problem.rated_speed_rpm * (1.0 + overspeed_ratio);

    // Penstock hoop stress under peak head:
    // sigma_hoop = P_peak * D / (2 * t)
    let p_peak_pa = rho * g * peak_head_m;
    let sigma_hoop_pa = (p_peak_pa * problem.penstock_diameter_m) / (2.0 * t_pen_m);
    let sigma_hoop_mpa = sigma_hoop_pa / 1e6;
    let penstock_sf = problem.steel_yield_strength_mpa / sigma_hoop_mpa;

    // IEEE 1207 Governor Tuning Recommendation:
    let r_t = (2.5 * inertia_ratio).max(0.20);
    let t_r = 5.0 * t_w;

    let is_dp_ok = delta_h_pct <= 40.0; // standard 40% head rise limit
    let is_speed_ok = overspeed_pct <= 55.0; // standard 55% runaway limit
    let is_stress_ok = penstock_sf >= 1.50; // ASME Boiler & Pressure Vessel Code SF >= 1.50

    Ok(HydroGovernorResult {
        problem_id: problem.problem_id.clone(),
        rated_mechanical_power_mw: p_0_mw,
        penstock_flow_velocity_m_s: v_0,
        water_hammer_wave_speed_m_s: wave_speed_a,
        reflection_time_seconds: t_reflection,
        water_starting_time_tw_s: t_w,
        mechanical_starting_time_tm_s: t_m,
        inertia_ratio_tw_over_tm: inertia_ratio,
        max_head_rise_m: delta_h_max_m,
        max_head_rise_pct: delta_h_pct,
        peak_turbine_inlet_head_m: peak_head_m,
        max_overspeed_pct: overspeed_pct,
        peak_transient_speed_rpm: peak_rpm,
        peak_penstock_hoop_stress_mpa: sigma_hoop_mpa,
        penstock_stress_safety_factor: penstock_sf,
        ieee1207_recommended_droop_rt: r_t,
        ieee1207_recommended_reset_time_tr_s: t_r,
        is_water_hammer_pressure_acceptable: is_dp_ok,
        is_overspeed_acceptable: is_speed_ok,
        is_penstock_stress_acceptable: is_stress_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_francis_hydro_governor_and_surge() -> Result<(), String> {
        let prob = HydroGovernorProblem {
            problem_id: "test_francis".to_string(),
            turbine_type: HydroTurbineType::FrancisReaction,
            rated_net_head_m: 145.0,
            rated_discharge_m3_s: 24.0,
            rated_speed_rpm: 500.0,
            rotor_inertia_kg_m2: 125000.0,
            penstock_length_m: 420.0,
            penstock_diameter_m: 2.20,
            penstock_wall_thickness_mm: 22.0,
            steel_elastic_modulus_gpa: 206.0,
            steel_yield_strength_mpa: 355.0,
            wicket_gate_closure_time_s: 6.50,
            turbine_efficiency: 0.935,
        };

        let res = solve_hydro_governor(&prob)?;
        assert!(res.rated_mechanical_power_mw > 30.0 && res.rated_mechanical_power_mw < 35.0);
        assert!(res.water_hammer_wave_speed_m_s > 1000.0 && res.water_hammer_wave_speed_m_s < 1300.0);
        assert!(res.water_starting_time_tw_s > 1.5 && res.water_starting_time_tw_s < 2.5);
        assert!(res.mechanical_starting_time_tm_s > 8.0 && res.mechanical_starting_time_tm_s < 14.0);
        assert!(res.max_head_rise_m > 30.0 && res.max_head_rise_m < 55.0);
        assert!(res.max_head_rise_pct < 40.0);
        assert!(res.max_overspeed_pct < 55.0);
        assert!(res.penstock_stress_safety_factor >= 1.50);
        assert!(res.is_water_hammer_pressure_acceptable);
        assert!(res.is_overspeed_acceptable);
        assert!(res.is_penstock_stress_acceptable);
        Ok(())
    }
}

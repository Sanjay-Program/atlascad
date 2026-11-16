//! Subsurface Mine Ventilation Aerodynamics & Fan Power (Atkinson / MSHA / McPherson).
//!
//! Evaluates Atkinson airway aerodynamic friction resistance, shock head losses, natural ventilation
//! thermal buoyancy pressure (NVE), main surface/booster fan operating pressure, mechanical air power,
//! motor electrical consumption, and toxic/methane gas dilution statutory compliance.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Airway rock wall surface roughness type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AirwayLiningType {
    /// Smooth shotcreted or cast concrete tunnel (k ~ 0.0035 N·s²/m⁴).
    SmoothConcrete,
    /// Regular drill-and-blast unsupported sedimentary rock (k ~ 0.0095 N·s²/m⁴).
    RoughBlastedRock,
    /// Heavily timbered or rock-bolted jagged drift (k ~ 0.0160 N·s²/m⁴).
    TimberedRock,
}

impl AirwayLiningType {
    /// Returns nominal Atkinson friction factor k in N·s²/m⁴ at standard air density 1.20 kg/m³.
    pub fn friction_factor_k(&self) -> f64 {
        match self {
            Self::SmoothConcrete   => 0.0035,
            Self::RoughBlastedRock => 0.0095,
            Self::TimberedRock     => 0.0160,
        }
    }
}

/// Subsurface mine ventilation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineVentilationProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Airway lining roughness category.
    #[serde(default = "default_lining")]
    pub lining_type: AirwayLiningType,
    /// Total airway segment length in meters (m) (e.g. 1400.0 m).
    pub airway_length_m: f64,
    /// Airway cross-sectional area in square meters (m²) (e.g. 20.0 m² for 5m x 4m drift).
    pub airway_area_m2: f64,
    /// Airway wetted perimeter in meters (m) (e.g. 18.0 m).
    pub airway_perimeter_m: f64,
    /// Required design volumetric airflow rate in cubic meters per second (m³/s) (e.g. 85.0 m³/s).
    pub airflow_rate_m3_s: f64,
    /// Ambient air density in airway in kg/m³ (e.g. 1.22 kg/m³).
    #[serde(default = "default_air_density")]
    pub air_density_kg_m3: f64,
    /// Airway minor shock loss factor X (bends, regulators, shaft collars, e.g. 1.50).
    #[serde(default = "default_shock_factor")]
    pub shock_loss_factor: f64,
    /// Vertical depth of mining horizon in meters (m) (e.g. 650.0 m).
    #[serde(default = "default_depth_m")]
    pub vertical_depth_m: f64,
    /// Temperature difference between downcast and upcast shafts in Celsius (ΔT, e.g. 12.0 °C).
    #[serde(default = "default_delta_t_c")]
    pub downcast_upcast_delta_temp_c: f64,
    /// Ventilation fan total aerodynamic efficiency (e.g. 0.78 for axial flow fan).
    #[serde(default = "default_fan_efficiency")]
    pub fan_efficiency: f64,
    /// Subsurface methane emission rate in cubic meters per second (m³/s) (e.g. 0.12 m³/s).
    #[serde(default = "default_methane_emission")]
    pub methane_emission_m3_s: f64,
}

fn default_problem_id() -> String {
    "mine_ventilation_problem".to_string()
}

fn default_lining() -> AirwayLiningType {
    AirwayLiningType::RoughBlastedRock
}

fn default_air_density() -> f64 {
    1.22
}

fn default_shock_factor() -> f64 {
    1.50
}

fn default_depth_m() -> f64 {
    400.0
}

fn default_delta_t_c() -> f64 {
    5.0
}

fn default_fan_efficiency() -> f64 {
    0.78
}

fn default_methane_emission() -> f64 {
    0.12
}

impl Default for MineVentilationProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            lining_type: default_lining(),
            airway_length_m: 1400.0,
            airway_area_m2: 20.0,
            airway_perimeter_m: 18.0,
            airflow_rate_m3_s: 85.0,
            air_density_kg_m3: 1.22,
            shock_loss_factor: 1.50,
            vertical_depth_m: 400.0,
            downcast_upcast_delta_temp_c: 5.0,
            fan_efficiency: 0.78,
            methane_emission_m3_s: 0.12,
        }
    }
}

/// Subsurface mine ventilation simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MineVentilationResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Mean air velocity in airway in meters per second (m/s).
    pub air_velocity_m_s: f64,
    /// Atkinson airway aerodynamic resistance R in N·s²/m⁸ (Atkinsons).
    pub atkinson_resistance_ns2_m8: f64,
    /// Frictional aerodynamic pressure loss in Pascals (Pa).
    pub friction_pressure_drop_pa: f64,
    /// Shock / minor dynamic pressure loss in Pascals (Pa).
    pub shock_pressure_drop_pa: f64,
    /// Total circuit airway pressure drop in Pascals (Pa).
    pub total_airway_pressure_drop_pa: f64,
    /// Natural ventilation thermal buoyancy pressure (NVE) in Pascals (Pa).
    pub natural_ventilation_pressure_pa: f64,
    /// Required mechanical ventilation fan total pressure in Pascals (Pa).
    pub required_fan_pressure_pa: f64,
    /// Net aerodynamic air power delivered to ventilation stream in kiloWatts (kW).
    pub air_power_kw: f64,
    /// Fan drive motor electrical shaft power consumption in kiloWatts (kW).
    pub fan_shaft_power_kw: f64,
    /// Return airway diluted methane concentration (%).
    pub return_methane_pct: f64,
    /// Whether methane concentration is below MSHA statutory threshold (<= 1.0%).
    pub is_methane_safe: bool,
    /// Whether air velocity is within recommended regulatory comfort range (0.5 to 8.0 m/s).
    pub is_velocity_acceptable: bool,
}

/// Solves subsurface airway resistance, pressure drops, NVE buoyancy, fan power, and gas dilution.
pub fn solve_mine_ventilation(
    problem: &MineVentilationProblem,
) -> Result<MineVentilationResult, String> {
    if problem.airway_length_m <= 0.0 || problem.airway_area_m2 <= 0.0 || problem.airway_perimeter_m <= 0.0 {
        return Err("Airway dimensions must be positive".to_string());
    }
    if problem.airflow_rate_m3_s <= 0.0 || problem.air_density_kg_m3 <= 0.0 {
        return Err("Airflow rate and density must be positive".to_string());
    }
    if problem.fan_efficiency <= 0.0 || problem.fan_efficiency > 1.0 {
        return Err("Fan efficiency must be between 0 and 1".to_string());
    }

    let q_flow = problem.airflow_rate_m3_s;
    let a_area = problem.airway_area_m2;
    let perim = problem.airway_perimeter_m;
    let length = problem.airway_length_m;
    let rho = problem.air_density_kg_m3;

    // 1. Mean Air Velocity:
    let v_air = q_flow / a_area;

    // 2. Atkinson Airway Resistance:
    // k_adj = k_std * (rho / 1.20)
    let k_std = problem.lining_type.friction_factor_k();
    let k_adj = k_std * (rho / 1.20);
    // R = k * Perim * L / A^3 [N·s²/m⁸]
    let r_atkinson = (k_adj * perim * length) / a_area.powi(3);

    // 3. Frictional Pressure Drop (Square law):
    // Delta_P_fric = R * Q^2 [Pa]
    let p_fric = r_atkinson * q_flow.powi(2);

    // 4. Shock Head Loss:
    // Delta_P_shock = X * 0.5 * rho * v^2 [Pa]
    let p_shock = problem.shock_loss_factor * 0.50 * rho * v_air.powi(2);
    let p_total = p_fric + p_shock;

    // 5. Natural Ventilation Energy (NVE) / Thermal Buoyancy:
    // Ideal gas thermal expansion: delta_rho ~ rho * (Delta_T / T_mean)
    // T_mean ~ 295 K
    let t_mean_k = 295.15;
    let delta_rho = rho * (problem.downcast_upcast_delta_temp_c / t_mean_k);
    let g_accel = 9.80665;
    let p_nve = (delta_rho * g_accel * problem.vertical_depth_m).max(0.0);

    // 6. Required Mechanical Fan Pressure:
    // Fan must provide airway total drop minus natural buoyancy assistance:
    let p_fan = (p_total - p_nve).max(100.0);

    // 7. Aerodynamic Air Power and Electrical Shaft Power:
    // P_air = Delta_P_fan * Q [W] -> [kW]
    let p_air_w = p_fan * q_flow;
    let p_air_kw = p_air_w * 1e-3;

    // Motor electrical efficiency ~ 0.95
    let motor_eff = 0.95;
    let p_shaft_kw = p_air_kw / (problem.fan_efficiency * motor_eff);

    // 8. Dilution of Methane / Gas Contaminants:
    // C_CH4 = (V_dot_CH4 / Q_total) * 100%
    let return_ch4_pct = (problem.methane_emission_m3_s / q_flow) * 100.0;
    let is_methane_safe = return_ch4_pct <= 1.00; // MSHA 1.0% limit in return air

    // Velocity comfort range for mining drifts: 0.5 to 8.0 m/s:
    let is_velocity_acceptable = (0.50..=8.00).contains(&v_air);

    Ok(MineVentilationResult {
        problem_id: problem.problem_id.clone(),
        air_velocity_m_s: v_air,
        atkinson_resistance_ns2_m8: r_atkinson,
        friction_pressure_drop_pa: p_fric,
        shock_pressure_drop_pa: p_shock,
        total_airway_pressure_drop_pa: p_total,
        natural_ventilation_pressure_pa: p_nve,
        required_fan_pressure_pa: p_fan,
        air_power_kw: p_air_kw,
        fan_shaft_power_kw: p_shaft_kw,
        return_methane_pct: return_ch4_pct,
        is_methane_safe,
        is_velocity_acceptable,
    })
}

/// Executes mine ventilation simulation and packages into SimulationResult envelope.
pub fn solve_mine_ventilation_sim(
    problem: &MineVentilationProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_mine_ventilation(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("air_velocity_m_s".to_string(), res.air_velocity_m_s);
            metrics.insert("atkinson_resistance_ns2_m8".to_string(), res.atkinson_resistance_ns2_m8);
            metrics.insert("total_airway_pressure_drop_pa".to_string(), res.total_airway_pressure_drop_pa);
            metrics.insert("natural_ventilation_pressure_pa".to_string(), res.natural_ventilation_pressure_pa);
            metrics.insert("required_fan_pressure_pa".to_string(), res.required_fan_pressure_pa);
            metrics.insert("fan_shaft_power_kw".to_string(), res.fan_shaft_power_kw);
            metrics.insert("return_methane_pct".to_string(), res.return_methane_pct);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.required_fan_pressure_pa, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "required_fan_pressure_pa",
                res.required_fan_pressure_pa,
                2150.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.mining.mine_ventilation".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.mining.mine_ventilation", "0.1.0"),
                logs: vec![format!(
                    "Mine Ventilation (Atkinson / MSHA): v = {:.2} m/s, R = {:.5} N·s²/m⁸, dP_total = {:.1} Pa (fric = {:.1}, shock = {:.1}), NVE = {:.1} Pa, Fan dP = {:.1} Pa, P_shaft = {:.1} kW (air = {:.1} kW), CH4 = {:.2}%, Safe: {}",
                    res.air_velocity_m_s, res.atkinson_resistance_ns2_m8, res.total_airway_pressure_drop_pa,
                    res.friction_pressure_drop_pa, res.shock_pressure_drop_pa, res.natural_ventilation_pressure_pa,
                    res.required_fan_pressure_pa, res.fan_shaft_power_kw, res.air_power_kw,
                    res.return_methane_pct, res.is_methane_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.mining.mine_ventilation".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.mining.mine_ventilation", "0.1.0"),
                logs: vec![format!("Mine ventilation simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_mine_ventilation_nominal() {
        let prob = MineVentilationProblem::default();
        let res = solve_mine_ventilation(&prob).expect("solver failed");
        println!(
            "Mine Vent: v = {:.2} m/s, R = {:.5}, dP = {:.1} Pa, NVE = {:.1} Pa, Fan dP = {:.1} Pa, Pshaft = {:.1} kW, CH4 = {:.2}%",
            res.air_velocity_m_s, res.atkinson_resistance_ns2_m8, res.total_airway_pressure_drop_pa,
            res.natural_ventilation_pressure_pa, res.required_fan_pressure_pa, res.fan_shaft_power_kw,
            res.return_methane_pct
        );

        assert!(res.air_velocity_m_s > 3.5 && res.air_velocity_m_s < 5.0);
        assert!(res.atkinson_resistance_ns2_m8 > 0.020 && res.atkinson_resistance_ns2_m8 < 0.045);
        assert!(res.total_airway_pressure_drop_pa > 180.0 && res.total_airway_pressure_drop_pa < 320.0);
        assert!(res.natural_ventilation_pressure_pa > 50.0 && res.natural_ventilation_pressure_pa < 120.0);
        assert!(res.required_fan_pressure_pa > 120.0 && res.required_fan_pressure_pa < 220.0);
        assert!(res.fan_shaft_power_kw > 12.0 && res.fan_shaft_power_kw < 30.0);
        assert!(res.return_methane_pct > 0.05 && res.return_methane_pct < 0.50);
        assert!(res.is_methane_safe);
        assert!(res.is_velocity_acceptable);
    }
}

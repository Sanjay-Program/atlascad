//! Pumped Storage Hydroelectric Plant Reversible Pump-Turbine & Water Conduit Analysis
//!
//! Formulated according to:
//! - IEC 60041 "Field acceptance tests to determine the hydraulic performance of hydraulic turbines, storage pumps and pump-turbines"
//! - IEC 61362 "Guide to specification of hydraulic turbine control systems"
//! - IEEE Std 1207 "IEEE Guide for the Application of Turbine Governing Systems for Hydroelectric Generating Units"
//! - de Siervo, F. & de Leva, F., "Modern trends in selecting and designing reversible Francis pump-turbines"
//!
//! Evaluates static and dynamic head losses, reversible Francis pump-turbine pumping/generating duty,
//! specific speed kinematics, cavitation Thoma parameter vs plant submergence, shaft powers,
//! and complete water-to-wire round-trip energy efficiency.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Reversible pump-turbine hydraulic machine architecture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PumpTurbineType {
    /// Single-stage reversible Francis pump-turbine (RPT, typical head 100 to 750 m).
    FrancisReversible,
    /// Diagonal / Deriaz variable-pitch pump-turbine (medium head 40 to 180 m).
    DeriazVariablePitch,
    /// Multi-stage reversible centrifugal pump-turbine (ultra-high head > 700 m).
    MultiStageReversible,
}

/// Input parameters for pumped storage hydroelectric plant simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpedStorageProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Hydraulic machine architecture.
    pub runner_type: PumpTurbineType,
    /// Upper reservoir full supply operating water level elevation in meters (m).
    pub upper_reservoir_elevation_m: f64,
    /// Lower reservoir tailwater operating level elevation in meters (m).
    pub lower_reservoir_elevation_m: f64,
    /// Total penstock, pressure shaft, and tailrace conduit length L in meters (m, standard 300 to 4500 m).
    pub conduit_length_m: f64,
    /// Internal hydraulic diameter of steel-lined / concrete penstock in meters (m, standard 1.5 to 8.0 m).
    pub conduit_diameter_m: f64,
    /// Darcy-Weisbach pipe friction factor f (standard 0.012 to 0.022).
    pub darcy_friction_factor: f64,
    /// Sum of minor loss resistance coefficients (bends, inlet bellmouth, bifurcations, valves).
    pub minor_loss_k_total: f64,
    /// Nominal discharge in pumping mode Q_p in cubic meters per second (m³/s, standard 5 to 120 m³/s).
    pub pumping_discharge_m3_s: f64,
    /// Nominal discharge in generating mode Q_t in cubic meters per second (m³/s, standard 5 to 150 m³/s).
    pub generating_discharge_m3_s: f64,
    /// Machine synchronous rotational speed in RPM (standard 250 to 1000 RPM).
    pub rotational_speed_rpm: f64,
    /// Pump-turbine runner outer tip diameter D_1 in meters (m, standard 2.0 to 7.5 m).
    pub runner_diameter_m: f64,
    /// Runner centerline submergence depth below minimum tailwater in meters (m, standard 5 to 60 m).
    pub runner_submergence_depth_m: f64,
    /// Ambient barometric atmospheric pressure in kiloPascals (kPa, standard 85 to 102 kPa).
    pub atmospheric_pressure_kpa: f64,
    /// Water vapor pressure at operating temperature in kiloPascals (kPa, standard 1.7 to 3.5 kPa).
    pub water_vapor_pressure_kpa: f64,
    /// Hydraulic efficiency in pumping mode eta_p (standard 0.88 to 0.93).
    pub pump_hydraulic_efficiency: f64,
    /// Hydraulic efficiency in turbine generating mode eta_t (standard 0.90 to 0.94).
    pub turbine_hydraulic_efficiency: f64,
    /// Synchronous motor-generator combined electrical and mechanical efficiency eta_mg (standard 0.97 to 0.988).
    pub motor_generator_efficiency: f64,
}

impl Default for PumpedStorageProblem {
    fn default() -> Self {
        Self {
            problem_id: "alpine_pumped_storage_unit".to_string(),
            runner_type: PumpTurbineType::FrancisReversible,
            upper_reservoir_elevation_m: 1850.0,
            lower_reservoir_elevation_m: 1540.0,
            conduit_length_m: 1650.0,
            conduit_diameter_m: 3.80,
            darcy_friction_factor: 0.014,
            minor_loss_k_total: 1.85,
            pumping_discharge_m3_s: 32.0,
            generating_discharge_m3_s: 36.5,
            rotational_speed_rpm: 428.6,     // Standard 14-pole 50 Hz synchronous speed
            runner_diameter_m: 3.45,
            runner_submergence_depth_m: 35.0,// 35 m cavern submergence below tailwater
            atmospheric_pressure_kpa: 86.5,  // Mountain altitude barometric pressure
            water_vapor_pressure_kpa: 1.75,   // ~15°C cold mountain water
            pump_hydraulic_efficiency: 0.912,
            turbine_hydraulic_efficiency: 0.928,
            motor_generator_efficiency: 0.982,
        }
    }
}

/// Simulation results for pumped storage hydraulic machine performance, cavitation, and power.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpedStorageResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Gross static geodetic elevation head difference H_stat in meters (m).
    pub static_head_m: f64,
    /// Pumping mode total conduit friction and minor head loss in meters (m).
    pub pumping_head_loss_m: f64,
    /// Total dynamic manometric pumping head H_p in meters (m).
    pub total_dynamic_pumping_head_m: f64,
    /// Generating mode total conduit friction and minor head loss in meters (m).
    pub generating_head_loss_m: f64,
    /// Net effective generating turbine head H_t in meters (m).
    pub net_generating_head_m: f64,
    /// Dimensionless specific speed in pumping mode N_qp.
    pub pumping_specific_speed: f64,
    /// Mechanical shaft power required in pumping mode in MegaWatts (MW).
    pub pumping_shaft_power_mw: f64,
    /// Electrical power consumed by motor from grid in pumping mode in MegaWatts (MW).
    pub pumping_electrical_power_mw: f64,
    /// Mechanical shaft power generated in turbine mode in MegaWatts (MW).
    pub generating_shaft_power_mw: f64,
    /// Net electrical power exported to grid in generating mode in MegaWatts (MW).
    pub generating_electrical_power_mw: f64,
    /// Net Positive Suction Head Available (NPSHA) at runner center in meters (m).
    pub npsha_m: f64,
    /// Plant operational Thoma cavitation parameter sigma_plant.
    pub thoma_cavitation_sigma: f64,
    /// Critical Thoma cavitation parameter sigma_crit to avoid inception erosion.
    pub critical_thoma_sigma: f64,
    /// Cavitation inception safety margin factor (sigma_plant / sigma_crit).
    pub cavitation_safety_factor: f64,
    /// Complete water-to-wire round-trip cycle energy storage efficiency in percentage (%).
    pub round_trip_efficiency_pct: f64,
    /// Assessment indicating plant submergence satisfies cavitation criteria (safety factor >= 1.15).
    pub is_cavitation_safe: bool,
    /// Assessment indicating water flow velocity in conduit is within standard erosion limits (v <= 7.0 m/s).
    pub is_conduit_velocity_acceptable: bool,
    /// Assessment indicating round-trip efficiency meets modern benchmark (eta_rt >= 75.0%).
    pub is_round_trip_efficiency_competitive: bool,
    /// Overall pumped storage engineering viability.
    pub is_pumped_storage_viable: bool,
}

/// Solves pumped storage reversible pump-turbine hydraulics, cavitation, and power per IEC 60041 / IEC 61362.
pub fn solve_pumped_storage_hydro(problem: &PumpedStorageProblem) -> Result<PumpedStorageResult, String> {
    if problem.upper_reservoir_elevation_m <= problem.lower_reservoir_elevation_m {
        return Err("Upper reservoir elevation must be strictly higher than lower reservoir".to_string());
    }
    if problem.conduit_diameter_m <= 0.2 || problem.conduit_diameter_m > 15.0 {
        return Err("Conduit diameter must be between 0.2 and 15.0 meters".to_string());
    }
    if problem.pumping_discharge_m3_s <= 0.1 || problem.generating_discharge_m3_s <= 0.1 {
        return Err("Discharge rates must be strictly positive".to_string());
    }
    if problem.rotational_speed_rpm < 50.0 || problem.rotational_speed_rpm > 3600.0 {
        return Err("Rotational speed must be between 50 and 3600 RPM".to_string());
    }

    let g = 9.80665; // m/s²
    let rho_w = 998.2; // kg/m³

    // 1. Static geodetic head:
    let h_stat = problem.upper_reservoir_elevation_m - problem.lower_reservoir_elevation_m;

    // 2. Conduit hydraulics:
    let a_cond = std::f64::consts::PI * (problem.conduit_diameter_m / 2.0).powi(2);
    let v_p = problem.pumping_discharge_m3_s / a_cond;
    let v_t = problem.generating_discharge_m3_s / a_cond;

    // Head losses: h_loss = (f * L / D + K) * v² / (2g)
    let loss_coeff = problem.darcy_friction_factor * (problem.conduit_length_m / problem.conduit_diameter_m)
        + problem.minor_loss_k_total;
    let h_loss_p = loss_coeff * v_p.powi(2) / (2.0 * g);
    let h_loss_t = loss_coeff * v_t.powi(2) / (2.0 * g);

    // 3. Dynamic heads:
    // Pumping mode must overcome static head PLUS friction losses: H_p = H_stat + h_loss_p
    let h_p = h_stat + h_loss_p;
    // Generating mode delivers static head MINUS friction losses: H_t = H_stat - h_loss_t
    let h_t = (h_stat - h_loss_t).max(1.0);

    // 4. Specific speed in pumping mode: N_qp = n * sqrt(Q) / H^(3/4)
    let n_qp = problem.rotational_speed_rpm * problem.pumping_discharge_m3_s.sqrt() / h_p.powf(0.75);

    // 5. Pumping power:
    // Hydraulic fluid power: P_hyd = rho * g * Q * H
    let p_hyd_p_w = rho_w * g * problem.pumping_discharge_m3_s * h_p;
    let p_pump_shaft_mw = (p_hyd_p_w / problem.pump_hydraulic_efficiency) / 1.0e6;
    let p_pump_elec_mw = p_pump_shaft_mw / problem.motor_generator_efficiency;

    // 6. Generating power:
    let p_hyd_t_w = rho_w * g * problem.generating_discharge_m3_s * h_t;
    let p_turb_shaft_mw = (p_hyd_t_w * problem.turbine_hydraulic_efficiency) / 1.0e6;
    let p_turb_elec_mw = p_turb_shaft_mw * problem.motor_generator_efficiency;

    // 7. Cavitation analysis (NPSHA & Thoma sigma):
    // Atmospheric pressure head: h_atm = P_atm / (rho * g)
    let h_atm = (problem.atmospheric_pressure_kpa * 1000.0) / (rho_w * g);
    let h_vap = (problem.water_vapor_pressure_kpa * 1000.0) / (rho_w * g);
    // Suction friction loss (lower reservoir to runner inlet): ~5% of pumping conduit loss
    let h_suction_loss = 0.05 * h_loss_p;

    // NPSHA = h_atm - h_vap + Z_submergence - h_suction_loss:
    let npsha = h_atm - h_vap + problem.runner_submergence_depth_m - h_suction_loss;
    let thoma_sigma = npsha / h_p;

    // Critical Thoma parameter empirical correlation for Francis pump-turbines (IEC 60193 / de Siervo):
    // sigma_crit = 1.15e-3 * N_qp^(4/3)
    let sigma_crit = 1.15e-3 * n_qp.powf(4.0 / 3.0);
    let cavitation_safety_factor = if sigma_crit > 0.0 {
        thoma_sigma / sigma_crit
    } else {
        1.5
    };

    // 8. Round-trip water-to-wire energy efficiency:
    // eta_rt = E_gen / E_pump for equal water volume:
    // Volume V = Q_p * t_p = Q_t * t_t => t_t / t_p = Q_p / Q_t
    // eta_rt = (P_turb_elec * t_t) / (P_pump_elec * t_p) = (P_turb_elec / P_pump_elec) * (Q_p / Q_t)
    let round_trip_eff = (p_turb_elec_mw / p_pump_elec_mw) * (problem.pumping_discharge_m3_s / problem.generating_discharge_m3_s);
    let round_trip_efficiency_pct = (round_trip_eff * 100.0).clamp(50.0, 95.0);

    // 9. Engineering criteria:
    let is_cavitation_safe = cavitation_safety_factor >= 1.15;
    let max_v = v_p.max(v_t);
    let is_conduit_velocity_acceptable = max_v <= 7.0; // Standard unlined/lined penstock limit
    let is_round_trip_efficiency_competitive = round_trip_efficiency_pct >= 75.0;
    let is_pumped_storage_viable = is_cavitation_safe
        && is_conduit_velocity_acceptable
        && is_round_trip_efficiency_competitive;

    Ok(PumpedStorageResult {
        problem_id: problem.problem_id.clone(),
        static_head_m: h_stat,
        pumping_head_loss_m: h_loss_p,
        total_dynamic_pumping_head_m: h_p,
        generating_head_loss_m: h_loss_t,
        net_generating_head_m: h_t,
        pumping_specific_speed: n_qp,
        pumping_shaft_power_mw: p_pump_shaft_mw,
        pumping_electrical_power_mw: p_pump_elec_mw,
        generating_shaft_power_mw: p_turb_shaft_mw,
        generating_electrical_power_mw: p_turb_elec_mw,
        npsha_m: npsha,
        thoma_cavitation_sigma: thoma_sigma,
        critical_thoma_sigma: sigma_crit,
        cavitation_safety_factor,
        round_trip_efficiency_pct,
        is_cavitation_safe,
        is_conduit_velocity_acceptable,
        is_round_trip_efficiency_competitive,
        is_pumped_storage_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_alpine_pumped_storage_simulation() -> Result<(), String> {
        let prob = PumpedStorageProblem::default();
        let res = solve_pumped_storage_hydro(&prob)?;

        assert_eq!(res.static_head_m, 310.0);
        assert!(res.total_dynamic_pumping_head_m > 311.0 && res.total_dynamic_pumping_head_m < 325.0);
        assert!(res.net_generating_head_m > 290.0 && res.net_generating_head_m < 310.0);
        assert!(res.pumping_electrical_power_mw > 100.0 && res.pumping_electrical_power_mw < 130.0);
        assert!(res.generating_electrical_power_mw > 85.0 && res.generating_electrical_power_mw < 120.0);
        assert!(res.npsha_m > 30.0);
        assert!(res.cavitation_safety_factor >= 1.15);
        assert!(res.round_trip_efficiency_pct >= 75.0 && res.round_trip_efficiency_pct <= 85.0);
        assert!(res.is_cavitation_safe);
        assert!(res.is_conduit_velocity_acceptable);
        assert!(res.is_round_trip_efficiency_competitive);
        assert!(res.is_pumped_storage_viable);
        Ok(())
    }
}

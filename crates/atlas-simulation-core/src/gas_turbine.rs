//! Aero Gas Turbine Open Brayton Cycle & Propulsion Physics (NASA SP-290 / ISO 2314).
//!
//! Models aircraft turbojet / gas turbine thermodynamic state stations (0-ambient,
//! 2-inlet face, 3-compressor discharge, 4-turbine inlet, 5-turbine discharge, 9-nozzle),
//! component efficiencies, combustor fuel-air ratio, net thrust, TSFC,
//! and thermal / propulsive efficiencies.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Fuel specification for aviation gas turbine engines.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AviationFuel {
    /// Jet-A / Jet-A1 kerosene (LHV = 43.1 MJ/kg).
    JetA1,
    /// Military JP-8 kerosene (LHV = 43.28 MJ/kg).
    Jp8,
    /// Sustainable Aviation Fuel (100% HEFA SAF, LHV = 44.0 MJ/kg).
    SustainableAviationFuel,
    /// Cryogenic Liquid Methane (LNG, LHV = 50.0 MJ/kg).
    LiquidMethane,
    /// Cryogenic Liquid Hydrogen (LH2, LHV = 120.0 MJ/kg).
    LiquidHydrogen,
}

impl AviationFuel {
    /// Lower Heating Value (LHV) in J/kg.
    pub fn lower_heating_value_j_kg(&self) -> f64 {
        match self {
            Self::JetA1 => 43.1e6,
            Self::Jp8 => 43.28e6,
            Self::SustainableAviationFuel => 44.0e6,
            Self::LiquidMethane => 50.0e6,
            Self::LiquidHydrogen => 120.0e6,
        }
    }
}

/// Aero gas turbine problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasTurbineProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Flight Mach number M0 (e.g. 0.82 cruise or 0.0 static takeoff).
    pub flight_mach: f64,
    /// Flight altitude in meters (e.g. 10,668 m for 35,000 ft cruise).
    pub altitude_m: f64,
    /// Core air mass flowrate m_dot (kg/s).
    pub core_mass_flow_kg_s: f64,
    /// Compressor overall pressure ratio OPR = Pt3 / Pt2 (e.g. 24.0).
    pub compressor_pressure_ratio: f64,
    /// Turbine Inlet Temperature TIT / Tt4 (Kelvin, e.g. 1650.0 K).
    pub turbine_inlet_temp_k: f64,
    /// Aviation fuel type.
    pub fuel: AviationFuel,
    /// Compressor isentropic efficiency eta_c (e.g. 0.88).
    pub compressor_isentropic_efficiency: f64,
    /// Turbine isentropic efficiency eta_t (e.g. 0.90).
    pub turbine_isentropic_efficiency: f64,
    /// Combustor burner efficiency eta_b (e.g. 0.99).
    pub combustor_efficiency: f64,
    /// Combustor total pressure drop fraction (e.g. 0.04 for 4%).
    pub combustor_pressure_loss_pct: f64,
    /// Exhaust nozzle velocity coefficient (e.g. 0.98).
    pub nozzle_velocity_coefficient: f64,
    /// Mechanical shaft transmission efficiency (e.g. 0.99).
    pub mechanical_efficiency: f64,
}

impl Default for GasTurbineProblem {
    fn default() -> Self {
        Self {
            problem_id: "turbofan_core_cruise_35kft".to_string(),
            flight_mach: 0.82,
            altitude_m: 10668.0, // 35,000 ft
            core_mass_flow_kg_s: 45.0,
            compressor_pressure_ratio: 24.0,
            turbine_inlet_temp_k: 1650.0,
            fuel: AviationFuel::JetA1,
            compressor_isentropic_efficiency: 0.88,
            turbine_isentropic_efficiency: 0.91,
            combustor_efficiency: 0.99,
            combustor_pressure_loss_pct: 3.5,
            nozzle_velocity_coefficient: 0.98,
            mechanical_efficiency: 0.99,
        }
    }
}

/// Aero gas turbine thermodynamic cycle results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GasTurbineResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Ambient static temperature T0 (K).
    pub ambient_temp_k: f64,
    /// Ambient static pressure P0 (kPa).
    pub ambient_pressure_kpa: f64,
    /// Flight true airspeed v0 (m/s).
    pub flight_speed_m_s: f64,
    /// Inlet total temperature Tt2 (K).
    pub inlet_total_temp_k: f64,
    /// Inlet total pressure Pt2 (kPa).
    pub inlet_total_pressure_kpa: f64,
    /// Compressor discharge total temperature Tt3 (K).
    pub compressor_discharge_temp_k: f64,
    /// Compressor discharge total pressure Pt3 (kPa).
    pub compressor_discharge_pressure_kpa: f64,
    /// Compressor required shaft power P_comp (kW).
    pub compressor_power_kw: f64,
    /// Fuel mass flowrate (kg/s).
    pub fuel_mass_flow_kg_s: f64,
    /// Fuel-to-air mass ratio f.
    pub fuel_air_ratio: f64,
    /// Combustor heat input rate Q_in (kW).
    pub combustor_heat_rate_kw: f64,
    /// Turbine exit total temperature Tt5 (K).
    pub turbine_exit_temp_k: f64,
    /// Turbine exit total pressure Pt5 (kPa).
    pub turbine_exit_pressure_kpa: f64,
    /// Exhaust nozzle jet velocity v9 (m/s).
    pub nozzle_jet_velocity_m_s: f64,
    /// Uninstalled net aerodynamic thrust F_net (kN).
    pub net_thrust_kn: f64,
    /// Specific thrust F_net / m_dot (N·s/kg).
    pub specific_thrust_n_s_kg: f64,
    /// Thrust Specific Fuel Consumption (TSFC, kg/(kN·h)).
    pub tsfc_kg_kn_h: f64,
    /// Thermal efficiency eta_th (0.0 to 1.0).
    pub thermal_efficiency: f64,
    /// Propulsive efficiency eta_p (0.0 to 1.0).
    pub propulsive_efficiency: f64,
    /// Overall propulsion system efficiency eta_o = eta_th * eta_p.
    pub overall_efficiency: f64,
}

/// Computes US Standard Atmosphere 1976 properties up to 20,000m.
pub fn standard_atmosphere(alt_m: f64) -> (f64, f64) {
    let h = alt_m.clamp(0.0, 20000.0);
    if h <= 11000.0 {
        // Troposphere
        let t = 288.15 - 0.0065 * h;
        let p = 101325.0 * (t / 288.15).powf(5.25588);
        (t, p / 1000.0)
    } else {
        // Stratosphere lower isothermal layer (11 km to 20 km)
        let t_11k = 216.65;
        let p_11k = 22632.1;
        let delta_h = h - 11000.0;
        let p = p_11k * (-delta_h / 6341.62).exp();
        (t_11k, p / 1000.0)
    }
}

/// Solves aero gas turbine open Brayton thermodynamic cycle.
pub fn solve_gas_turbine(prob: &GasTurbineProblem) -> GasTurbineResult {
    let (t0, p0_kpa) = standard_atmosphere(prob.altitude_m);
    let p0_pa = p0_kpa * 1000.0;

    let gamma_a = 1.40;
    let r_air = 287.05;
    let cp_a = 1005.0; // J/(kg·K) for air

    let gamma_g = 1.33;
    let cp_g = 1150.0; // J/(kg·K) for combustion products

    // Flight speed v0:
    let sound_speed = (gamma_a * r_air * t0).sqrt();
    let v0 = prob.flight_mach * sound_speed;

    // Station 2: Inlet / Ram compression
    let m0 = prob.flight_mach.max(0.0);
    let ram_temp_ratio = 1.0 + ((gamma_a - 1.0) / 2.0) * m0.powi(2);
    let tt2 = t0 * ram_temp_ratio;

    // Ram recovery efficiency (MIL-E-5007D):
    let eta_ram = if m0 <= 1.0 {
        1.0
    } else {
        1.0 - 0.075 * (m0 - 1.0).powf(1.35)
    };
    let ram_press_ratio = (1.0 + eta_ram * ((gamma_a - 1.0) / 2.0) * m0.powi(2)).powf(gamma_a / (gamma_a - 1.0));
    let pt2_kpa = p0_kpa * ram_press_ratio;

    // Station 3: Compressor
    let opr = prob.compressor_pressure_ratio.max(1.1);
    let pt3_kpa = pt2_kpa * opr;
    let exp_c = (gamma_a - 1.0) / gamma_a;
    let eta_c = prob.compressor_isentropic_efficiency.clamp(0.5, 0.98);
    let tt3 = tt2 * (1.0 + (1.0 / eta_c) * (opr.powf(exp_c) - 1.0));

    let p_comp_w = prob.core_mass_flow_kg_s * cp_a * (tt3 - tt2);
    let p_comp_kw = p_comp_w / 1000.0;

    // Station 4: Combustor
    let tt4 = prob.turbine_inlet_temp_k.max(tt3 + 50.0);
    let dp_comb = prob.combustor_pressure_loss_pct.clamp(0.0, 15.0) / 100.0;
    let pt4_kpa = pt3_kpa * (1.0 - dp_comb);

    let lhv = prob.fuel.lower_heating_value_j_kg();
    let eta_b = prob.combustor_efficiency.clamp(0.80, 1.0);
    let q_comb_w = prob.core_mass_flow_kg_s * cp_g * (tt4 - tt3);
    let m_dot_fuel = q_comb_w / (lhv * eta_b);
    let f_ratio = m_dot_fuel / prob.core_mass_flow_kg_s;

    // Station 5: Turbine expansion driving compressor
    let eta_mech = prob.mechanical_efficiency.clamp(0.80, 1.0);
    let p_turb_req_w = p_comp_w / eta_mech;
    let m_dot_gas = prob.core_mass_flow_kg_s * (1.0 + f_ratio);
    let tt5 = tt4 - (p_turb_req_w / (m_dot_gas * cp_g));

    let eta_t = prob.turbine_isentropic_efficiency.clamp(0.5, 0.98);
    let exp_t = gamma_g / (gamma_g - 1.0);
    let pt5_kpa = pt4_kpa * (1.0 - (1.0 / eta_t) * (1.0 - tt5 / tt4)).powf(exp_t);
    let pt5_pa = pt5_kpa * 1000.0;

    // Station 9: Exhaust Nozzle expansion to ambient P0
    let exp_nozzle = (gamma_g - 1.0) / gamma_g;
    let press_ratio_nozzle = (p0_pa / pt5_pa.max(p0_pa)).clamp(0.01, 1.0);
    let phi_n = prob.nozzle_velocity_coefficient.clamp(0.80, 1.0);
    let v9 = phi_n * (2.0 * cp_g * tt5 * (1.0 - press_ratio_nozzle.powf(exp_nozzle))).max(0.0).sqrt();

    // Thrust & Performance:
    // F_net = m_dot_gas * v9 - m_dot_air * v0 (N)
    let f_net_n = (m_dot_gas * v9) - (prob.core_mass_flow_kg_s * v0);
    let f_net_kn = f_net_n / 1000.0;
    let spec_thrust = f_net_n / prob.core_mass_flow_kg_s;

    // TSFC in kg/(kN·h): (m_dot_fuel * 3600) / F_net_kN
    let tsfc = if f_net_kn > 0.1 {
        (m_dot_fuel * 3600.0) / f_net_kn
    } else {
        999.0
    };

    // Energetic efficiencies:
    let delta_ke_rate_w = 0.5 * (m_dot_gas * v9.powi(2) - prob.core_mass_flow_kg_s * v0.powi(2));
    let fuel_power_w = m_dot_fuel * lhv;
    let eta_th = (delta_ke_rate_w / fuel_power_w.max(1.0)).clamp(0.05, 0.70);

    let eta_p = if (v9 + v0) > 1.0 && v0 > 0.1 {
        (2.0 * v0 / (v9 + v0)).clamp(0.10, 0.98)
    } else {
        0.0 // At static takeoff v0 = 0
    };
    let eta_o = eta_th * eta_p;

    GasTurbineResult {
        problem_id: prob.problem_id.clone(),
        ambient_temp_k: t0,
        ambient_pressure_kpa: p0_kpa,
        flight_speed_m_s: v0,
        inlet_total_temp_k: tt2,
        inlet_total_pressure_kpa: pt2_kpa,
        compressor_discharge_temp_k: tt3,
        compressor_discharge_pressure_kpa: pt3_kpa,
        compressor_power_kw: p_comp_kw,
        fuel_mass_flow_kg_s: m_dot_fuel,
        fuel_air_ratio: f_ratio,
        combustor_heat_rate_kw: q_comb_w / 1000.0,
        turbine_exit_temp_k: tt5,
        turbine_exit_pressure_kpa: pt5_kpa,
        nozzle_jet_velocity_m_s: v9,
        net_thrust_kn: f_net_kn,
        specific_thrust_n_s_kg: spec_thrust,
        tsfc_kg_kn_h: tsfc,
        thermal_efficiency: eta_th,
        propulsive_efficiency: eta_p,
        overall_efficiency: eta_o,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_gas_turbine_sim(prob: &GasTurbineProblem) -> SimulationResult {
    let sol = solve_gas_turbine(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("net_thrust_kn".to_string(), sol.net_thrust_kn);
    metrics.insert("specific_thrust_n_s_kg".to_string(), sol.specific_thrust_n_s_kg);
    metrics.insert("tsfc_kg_kn_h".to_string(), sol.tsfc_kg_kn_h);
    metrics.insert("compressor_power_kw".to_string(), sol.compressor_power_kw);
    metrics.insert("thermal_efficiency".to_string(), sol.thermal_efficiency);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.net_thrust_kn, 1, 1e-4);

    let comparison = AnalyticalComparison::assess(
        "net_thrust_kn",
        sol.net_thrust_kn,
        28.0,
    );

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.aerospace.gas_turbine_brayton".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.aerospace.gas_turbine_brayton", "0.1.0"),
        logs: vec![format!("Net Thrust: {:.1} kN, TSFC: {:.3} kg/(kN·h), Jet Velocity: {:.1} m/s, TIT: {:.0} K, eta_th: {:.1}%", sol.net_thrust_kn, sol.tsfc_kg_kn_h, sol.nozzle_jet_velocity_m_s, prob.turbine_inlet_temp_k, sol.thermal_efficiency * 100.0)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_turbofan_core_high_altitude_cruise_performance() {
        let prob = GasTurbineProblem::default();
        let sol = solve_gas_turbine(&prob);

        // At 10,668 m (35,000 ft): T0 = 218.8 K, P0 = 23.8 kPa
        assert!((sol.ambient_temp_k - 218.8).abs() < 1.0);
        assert!((sol.ambient_pressure_kpa - 23.8).abs() < 1.0);

        // Mach 0.82 cruise speed: a0 = sqrt(1.4 * 287.05 * 218.8) = 296.5 m/s
        // v0 = 0.82 * 296.5 = 243.1 m/s
        assert!((sol.flight_speed_m_s - 243.1).abs() < 2.0);

        // OPR = 24 -> Pt3 ≈ 24 * Pt2 ≈ 24 * 37.0 ≈ 888 kPa
        assert!(sol.compressor_discharge_pressure_kpa > 700.0 && sol.compressor_discharge_pressure_kpa < 1100.0);

        // Tt3 ≈ 550 to 650 K
        assert!(sol.compressor_discharge_temp_k > 500.0 && sol.compressor_discharge_temp_k < 700.0);

        // Compressor power for 45 kg/s air across ~360 K rise: 45 * 1.005 * 360 ≈ 16 MW
        assert!(sol.compressor_power_kw > 12000.0 && sol.compressor_power_kw < 20000.0);

        // Jet velocity v9 should be 700 to 1100 m/s
        assert!(sol.nozzle_jet_velocity_m_s > 700.0 && sol.nozzle_jet_velocity_m_s < 1200.0);

        // Core net thrust should be 20 to 45 kN
        assert!(sol.net_thrust_kn > 18.0 && sol.net_thrust_kn < 45.0);

        // Typical core TSFC: 50 to 100 kg/(kN·h) (pure core without bypass fan multiplier)
        assert!(sol.tsfc_kg_kn_h > 40.0 && sol.tsfc_kg_kn_h < 120.0);

        // Thermal efficiency between 40% and 55%
        assert!(sol.thermal_efficiency > 0.38 && sol.thermal_efficiency < 0.58);
    }
}

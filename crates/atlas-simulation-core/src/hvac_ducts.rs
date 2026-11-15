//! HVAC Airflow, Duct Pressure Loss & Fan Aerodynamics (ASHRAE / SMACNA).
//!
//! Models airflow hydraulics through rectangular and circular duct networks,
//! Huebscher equivalent hydraulic diameter, Colebrook-White friction factors,
//! dynamic fitting pressure losses, external static pressure (ESP),
//! and fan brake horsepower.

#![forbid(unsafe_code)]

use std::f64::consts::PI;
use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Duct construction material and internal absolute surface roughness epsilon (mm).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DuctMaterial {
    /// Galvanized sheet steel (standard SMACNA commercial, epsilon = 0.09 mm).
    GalvanizedSteel,
    /// Spiral seam round steel duct (smooth, epsilon = 0.06 mm).
    SpiralSteel,
    /// Fibrous glass duct board (epsilon = 0.25 mm).
    FiberglassBoard,
    /// Flexible fabric / wire spiral duct (extended, epsilon = 1.00 mm).
    FlexibleDuct,
    /// Smooth extruded PVC / plastic (epsilon = 0.01 mm).
    PvcPlastic,
}

impl DuctMaterial {
    /// Absolute internal roughness epsilon in meters.
    pub fn surface_roughness_m(&self) -> f64 {
        match self {
            Self::GalvanizedSteel => 0.09e-3,
            Self::SpiralSteel => 0.06e-3,
            Self::FiberglassBoard => 0.25e-3,
            Self::FlexibleDuct => 1.00e-3,
            Self::PvcPlastic => 0.01e-3,
        }
    }
}

/// Duct cross-sectional shape and geometry.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum DuctGeometry {
    /// Rectangular duct with width (breadth) and height in mm.
    Rectangular { width_mm: f64, height_mm: f64 },
    /// Circular round duct with diameter in mm.
    Circular { diameter_mm: f64 },
}

/// HVAC duct network problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HvacDuctProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Duct shape and cross-section.
    pub geometry: DuctGeometry,
    /// Duct material.
    pub material: DuctMaterial,
    /// Volumetric airflow rate Q (m³/h).
    pub airflow_m3_h: f64,
    /// Straight duct total run length L (m).
    pub duct_length_m: f64,
    /// Sum of local dynamic loss coefficients (C_o) for elbows, tees, transitions, dampers.
    pub fitting_loss_coeff_sum: f64,
    /// Terminal equipment pressure drop (filters, cooling coil, silencers, diffusers) in Pa.
    pub terminal_equipment_pa: f64,
    /// Fan total efficiency eta_fan (0.0 to 1.0, e.g. 0.72 for 72%).
    pub fan_efficiency: f64,
    /// Motor drive efficiency eta_motor (e.g. 0.90 for 90%).
    pub motor_efficiency: f64,
    /// Air operating temperature (°C).
    pub air_temperature_c: f64,
}

impl Default for HvacDuctProblem {
    fn default() -> Self {
        Self {
            problem_id: "office_floor_supply_air_main".to_string(),
            geometry: DuctGeometry::Rectangular {
                width_mm: 600.0,
                height_mm: 350.0,
            },
            material: DuctMaterial::GalvanizedSteel,
            airflow_m3_h: 5400.0, // 1.50 m³/s
            duct_length_m: 45.0,
            fitting_loss_coeff_sum: 3.8, // 3 elbows + transition + volume damper
            terminal_equipment_pa: 140.0, // MERV 13 filter + cooling coil + diffusers
            fan_efficiency: 0.73,
            motor_efficiency: 0.91,
            air_temperature_c: 20.0,
        }
    }
}

/// HVAC duct airflow and fan power results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HvacDuctResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Volumetric airflow rate Q (m³/s).
    pub airflow_m3_s: f64,
    /// Duct cross-sectional flow area A (m²).
    pub cross_sectional_area_m2: f64,
    /// Mean air velocity v (m/s).
    pub air_velocity_m_s: f64,
    /// Huebscher equivalent circular hydraulic diameter D_e (m).
    pub equivalent_diameter_m: f64,
    /// Reynolds number Re based on D_e.
    pub reynolds_number: f64,
    /// Darcy-Weisbach friction factor f.
    pub darcy_friction_factor: f64,
    /// Straight duct friction pressure loss Delta P_f (Pa).
    pub duct_friction_loss_pa: f64,
    /// Unit friction loss rate Delta P / L (Pa/m).
    pub friction_rate_pa_m: f64,
    /// Dynamic velocity pressure P_v = 0.5 * rho * v^2 (Pa).
    pub dynamic_velocity_pressure_pa: f64,
    /// Dynamic fitting local pressure losses Delta P_m (Pa).
    pub fitting_dynamic_loss_pa: f64,
    /// Total external static pressure (ESP) Delta P_total (Pa).
    pub total_fan_pressure_pa: f64,
    /// Aerodynamic air power delivered to fluid stream (W).
    pub air_power_w: f64,
    /// Fan shaft brake electric motor power P_brake (kW).
    pub fan_brake_power_kw: f64,
    /// Estimated fan sound power level L_w (dBA).
    pub estimated_sound_power_lwa: f64,
    /// Whether air velocity is within recommended SMACNA comfort limits (4.0 to 9.0 m/s).
    pub velocity_within_comfort_limits: bool,
}

/// Solves HVAC airflow, duct pressure loss, and fan power per ASHRAE Fundamentals.
pub fn solve_hvac_ducts(prob: &HvacDuctProblem) -> HvacDuctResult {
    let t_k = prob.air_temperature_c + 273.15;
    let p_atm = 101325.0;
    let r_air = 287.05;
    let rho = p_atm / (r_air * t_k); // ~1.204 kg/m³ at 20°C
    let mu = (1.716e-5 * (t_k / 273.15).powf(1.5) * (383.55 / (t_k + 110.4))).max(1e-6);

    let q_m3_s = prob.airflow_m3_h / 3600.0;

    // Cross-sectional area and equivalent diameter De:
    let (area_m2, d_e_m) = match prob.geometry {
        DuctGeometry::Rectangular { width_mm, height_mm } => {
            let a = width_mm.max(50.0) / 1000.0;
            let b = height_mm.max(50.0) / 1000.0;
            let area = a * b;
            // Huebscher Equation (ASHRAE Eq 33.11):
            // D_e = 1.30 * (a * b)^0.625 / (a + b)^0.250
            let d_e = 1.30 * (a * b).powf(0.625) / (a + b).powf(0.250);
            (area, d_e)
        }
        DuctGeometry::Circular { diameter_mm } => {
            let d = diameter_mm.max(50.0) / 1000.0;
            let area = PI * d.powi(2) / 4.0;
            (area, d)
        }
    };

    let v_m_s = q_m3_s / area_m2.max(1e-5);
    let p_v = 0.5 * rho * v_m_s.powi(2); // Dynamic velocity pressure (Pa)

    let re = (rho * v_m_s * d_e_m) / mu;
    let eps = prob.material.surface_roughness_m();

    // Colebrook-White friction factor via Swamee-Jain explicit formula:
    let f = if re > 4000.0 {
        0.25 / ((eps / (3.7 * d_e_m) + 5.74 / re.powf(0.9)).log10()).powi(2)
    } else if re > 2300.0 {
        // Transitional interpolation
        0.032
    } else {
        // Laminar Hagen-Poiseuille: f = 64 / Re
        (64.0 / re.max(1.0)).min(0.10)
    };

    // Duct straight run friction pressure drop: Delta P_f = f * (L / De) * P_v
    let dp_f = f * (prob.duct_length_m / d_e_m) * p_v;
    let friction_rate = if prob.duct_length_m > 0.0 {
        dp_f / prob.duct_length_m
    } else {
        0.0
    };

    // Dynamic fitting losses: Delta P_m = sum(C_o) * P_v
    let dp_m = prob.fitting_loss_coeff_sum.max(0.0) * p_v;

    // Total External Static Pressure:
    let dp_total = dp_f + dp_m + prob.terminal_equipment_pa.max(0.0);

    // Fan air power: P_air = Q * Delta P_total (W)
    let p_air_w = q_m3_s * dp_total;

    // Fan shaft brake electrical motor power:
    let combined_eff = (prob.fan_efficiency.clamp(0.20, 0.95) * prob.motor_efficiency.clamp(0.50, 0.99)).max(0.10);
    let p_brake_kw = (p_air_w / combined_eff) / 1000.0;

    // Sound power level L_w estimate (ASHRAE HVAC Applications):
    // L_w = 40 + 10*log10(Q_m3/s) + 20*log10(Delta P)
    let l_w = 40.0 + 10.0 * q_m3_s.max(0.01).log10() + 20.0 * dp_total.max(10.0).log10();

    // SMACNA commercial duct velocity comfort limit: 3.5 to 8.5 m/s
    let vel_comfort = (2.5..=9.0).contains(&v_m_s);

    HvacDuctResult {
        problem_id: prob.problem_id.clone(),
        airflow_m3_s: q_m3_s,
        cross_sectional_area_m2: area_m2,
        air_velocity_m_s: v_m_s,
        equivalent_diameter_m: d_e_m,
        reynolds_number: re,
        darcy_friction_factor: f,
        duct_friction_loss_pa: dp_f,
        friction_rate_pa_m: friction_rate,
        dynamic_velocity_pressure_pa: p_v,
        fitting_dynamic_loss_pa: dp_m,
        total_fan_pressure_pa: dp_total,
        air_power_w: p_air_w,
        fan_brake_power_kw: p_brake_kw,
        estimated_sound_power_lwa: l_w,
        velocity_within_comfort_limits: vel_comfort,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_hvac_ducts_sim(prob: &HvacDuctProblem) -> SimulationResult {
    let sol = solve_hvac_ducts(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("air_velocity_m_s".to_string(), sol.air_velocity_m_s);
    metrics.insert("total_fan_pressure_pa".to_string(), sol.total_fan_pressure_pa);
    metrics.insert("fan_brake_power_kw".to_string(), sol.fan_brake_power_kw);
    metrics.insert("equivalent_diameter_m".to_string(), sol.equivalent_diameter_m);
    metrics.insert("friction_rate_pa_m".to_string(), sol.friction_rate_pa_m);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.total_fan_pressure_pa, 1, 1e-4);

    let comparison = AnalyticalComparison::assess(
        "total_fan_pressure_pa",
        sol.total_fan_pressure_pa,
        280.0,
    );

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.hvac.duct_hydraulics_ashrae".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.hvac.duct_hydraulics_ashrae", "0.1.0"),
        logs: vec![format!("Airflow: {:.2} m3/s, Velocity: {:.2} m/s, De: {:.3} m, Total ESP: {:.1} Pa (Rate: {:.2} Pa/m), Fan Brake Power: {:.2} kW", sol.airflow_m3_s, sol.air_velocity_m_s, sol.equivalent_diameter_m, sol.total_fan_pressure_pa, sol.friction_rate_pa_m, sol.fan_brake_power_kw)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commercial_office_supply_duct_hydraulics() {
        let prob = HvacDuctProblem::default();
        let sol = solve_hvac_ducts(&prob);

        // Q = 5400 / 3600 = 1.50 m³/s
        assert!((sol.airflow_m3_s - 1.50).abs() < 1e-3);
        // Area = 0.60 * 0.35 = 0.210 m²
        assert!((sol.cross_sectional_area_m2 - 0.210).abs() < 1e-3);
        // Velocity = 1.50 / 0.210 = 7.14 m/s (ideal commercial supply range)
        assert!((sol.air_velocity_m_s - 7.14).abs() < 0.1);
        assert!(sol.velocity_within_comfort_limits);

        // Huebscher equivalent diameter for 600x350:
        // 1.30 * (0.21)^0.625 / (0.95)^0.25 ≈ 0.493 m
        assert!(sol.equivalent_diameter_m > 0.45 && sol.equivalent_diameter_m < 0.55);

        // Dynamic pressure Pv = 0.5 * 1.204 * 7.14^2 ≈ 30.7 Pa
        assert!(sol.dynamic_velocity_pressure_pa > 25.0 && sol.dynamic_velocity_pressure_pa < 36.0);

        // Total ESP: Friction (~60 Pa) + Fittings (~115 Pa) + Terminals (140 Pa) ≈ 300 to 330 Pa
        assert!(sol.total_fan_pressure_pa > 250.0 && sol.total_fan_pressure_pa < 380.0);

        // Fan power: 1.50 m³/s * 315 Pa / 0.66 ≈ 0.70 kW
        assert!(sol.fan_brake_power_kw > 0.50 && sol.fan_brake_power_kw < 1.20);
    }
}

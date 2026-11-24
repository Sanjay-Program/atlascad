//! Centrifugal Pumps, Hydraulic Turbomachinery & Affinity Laws Solver (ISO 9906 / ASME PTC 10).
//!
//! Evaluates pump head-capacity characteristic curve H(Q), affinity law scaling
//! for speed and impeller diameter changes, system operating point intersection,
//! brake horsepower, hydraulic efficiency, and NPSH cavitation margin.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Fluid type for pumping applications.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PumpFluid {
    Water20C,
    Water80C,
    DieselFuel,
    EthyleneGlycol50,
    LightCrudeOil,
}

impl PumpFluid {
    /// Fluid density (kg/m³).
    pub fn density_kg_m3(&self) -> f64 {
        match self {
            Self::Water20C => 998.2,
            Self::Water80C => 971.8,
            Self::DieselFuel => 832.0,
            Self::EthyleneGlycol50 => 1070.0,
            Self::LightCrudeOil => 850.0,
        }
    }

    /// Vapor pressure at fluid operating temperature (Pa).
    pub fn vapor_pressure_pa(&self) -> f64 {
        match self {
            Self::Water20C => 2338.0,
            Self::Water80C => 47360.0,
            Self::DieselFuel => 1000.0,
            Self::EthyleneGlycol50 => 1800.0,
            Self::LightCrudeOil => 35000.0,
        }
    }
}

/// Centrifugal pump problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Fluid type.
    pub fluid: PumpFluid,
    /// Nominal rotational speed N1 (RPM).
    pub nominal_speed_rpm: f64,
    /// Nominal impeller outer diameter D1 (mm).
    pub nominal_impeller_diameter_mm: f64,
    /// Shutoff head H0 at zero flow (m).
    pub shutoff_head_m: f64,
    /// Internal hydraulic coefficient A in H(Q) = H0 - A*Q^2 (where Q is in m³/hr).
    pub pump_curve_coeff_a: f64,
    /// Best Efficiency Point (BEP) flowrate Q_bep (m³/hr).
    pub bep_flow_m3_hr: f64,
    /// Peak pump total efficiency at BEP (0.0 to 1.0, e.g. 0.82 for 82%).
    pub peak_efficiency: f64,
    /// Target new rotational speed N2 (RPM, optional, for affinity scaling).
    pub target_speed_rpm: Option<f64>,
    /// Target trimmed impeller diameter D2 (mm, optional, for affinity trimming).
    pub target_impeller_diameter_mm: Option<f64>,
    /// System static head H_static (m).
    pub system_static_head_m: f64,
    /// System piping friction coefficient k in H_sys = H_static + k*Q^2.
    pub system_friction_coeff_k: f64,
    /// Available NPSH at pump suction flange (m, NPSH_A).
    pub npsh_available_m: f64,
    /// Pump required NPSH at BEP (m, NPSH_R).
    pub npsh_required_m: f64,
}

impl Default for PumpProblem {
    fn default() -> Self {
        Self {
            problem_id: "centrifugal_water_feed_pump".to_string(),
            fluid: PumpFluid::Water20C,
            nominal_speed_rpm: 2950.0, // 2-pole 50Hz motor
            nominal_impeller_diameter_mm: 250.0,
            shutoff_head_m: 85.0,
            pump_curve_coeff_a: 0.0028, // H(Q) = 85 - 0.0028 * Q^2
            bep_flow_m3_hr: 120.0,
            peak_efficiency: 0.81,
            target_speed_rpm: Some(2950.0),
            target_impeller_diameter_mm: Some(250.0),
            system_static_head_m: 25.0,
            system_friction_coeff_k: 0.0032, // H_sys = 25 + 0.0032 * Q^2
            npsh_available_m: 6.5,
            npsh_required_m: 3.2,
        }
    }
}

/// Comprehensive pump operating point and affinity law results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PumpResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Speed scaling ratio N2 / N1.
    pub speed_ratio: f64,
    /// Impeller diameter trimming ratio D2 / D1.
    pub diameter_ratio: f64,
    /// Scaled shutoff head (m).
    pub scaled_shutoff_head_m: f64,
    /// Scaled curve coefficient A2.
    pub scaled_curve_coeff_a: f64,
    /// Operating flowrate Q_op at system intersection (m³/hr).
    pub operating_flow_m3_hr: f64,
    /// Operating flowrate in liters per second (L/s).
    pub operating_flow_l_s: f64,
    /// Operating total dynamic head H_op (m).
    pub operating_head_m: f64,
    /// Operating pump efficiency eta.
    pub operating_efficiency: f64,
    /// Hydraulic water power delivered to fluid (kW).
    pub hydraulic_power_kw: f64,
    /// Required shaft brake power (BHP in kW).
    pub brake_shaft_power_kw: f64,
    /// Operating point distance from BEP (%).
    pub bep_deviation_pct: f64,
    /// Net Positive Suction Head margin (NPSH_A - NPSH_R in meters).
    pub npsh_margin_m: f64,
    /// Cavitation safe (NPSH_A >= 1.10 * NPSH_R).
    pub cavitation_safe: bool,
    /// Specific speed N_s in metric units (RPM, m³/s, m).
    pub metric_specific_speed: f64,
}

/// Solves centrifugal pump performance, affinity laws, and system operating point.
pub fn solve_centrifugal_pump(prob: &PumpProblem) -> PumpResult {
    let n1 = prob.nominal_speed_rpm.max(100.0);
    let d1 = prob.nominal_impeller_diameter_mm.max(10.0);

    let n2 = prob.target_speed_rpm.unwrap_or(n1).max(100.0);
    let d2 = prob.target_impeller_diameter_mm.unwrap_or(d1).max(10.0);

    let speed_ratio = n2 / n1;
    let diam_ratio = d2 / d1;

    // Affinity laws:
    // Flow scales with (N2/N1) * (D2/D1)
    let flow_scale = speed_ratio * diam_ratio;
    // Head scales with (N2/N1)^2 * (D2/D1)^2
    let head_scale = (speed_ratio * diam_ratio).powi(2);

    let h0_scaled = prob.shutoff_head_m * head_scale;
    // H(Q) = H0 - A1 * (Q / flow_scale)^2 * head_scale = H0 - (A1 / flow_scale^2 * head_scale) * Q^2
    // Since flow_scale^2 = head_scale, the coefficient A remains identical if purely scaling speed/diam!
    // But when trimmed, A2 = A1 / flow_scale^2 * head_scale = A1 (exact identity)
    let a_scaled = prob.pump_curve_coeff_a;

    // System intersection:
    // H0_scaled - A_scaled * Q^2 = H_static + k_friction * Q^2
    // Q_op^2 * (A_scaled + k_friction) = H0_scaled - H_static
    let delta_h = h0_scaled - prob.system_static_head_m;
    let k_sum = a_scaled + prob.system_friction_coeff_k;

    let q_op_m3_hr = if delta_h > 0.0 && k_sum > 1e-6 {
        (delta_h / k_sum).sqrt()
    } else {
        0.0
    };

    let h_op_m = h0_scaled - a_scaled * q_op_m3_hr.powi(2);

    // Efficiency curve approximation around BEP:
    // eta(Q) = eta_bep * [ 1 - 0.5 * ((Q - Q_bep) / Q_bep)^2 ]
    let bep_scaled = prob.bep_flow_m3_hr * flow_scale;
    let bep_dev = if bep_scaled > 1e-3 {
        ((q_op_m3_hr - bep_scaled) / bep_scaled).abs()
    } else {
        0.0
    };

    let eta_op = (prob.peak_efficiency * (1.0 - 0.70 * bep_dev.powi(2))).clamp(0.20, 0.95);

    // Power calculations:
    // Q in m³/s = Q_m3_hr / 3600
    let q_m3_s = q_op_m3_hr / 3600.0;
    let rho = prob.fluid.density_kg_m3();
    let g = 9.80665;

    // P_hyd = rho * g * Q * H in Watts -> kW
    let p_hyd_kw = (rho * g * q_m3_s * h_op_m) / 1000.0;
    let p_brake_kw = p_hyd_kw / eta_op;

    // Cavitation & NPSH margin:
    let npsh_margin = prob.npsh_available_m - prob.npsh_required_m;
    let cavitation_safe = prob.npsh_available_m >= (1.10 * prob.npsh_required_m);

    // Metric specific speed:
    // N_s = N * sqrt(Q) / H^(3/4) with Q in m³/s and H in m
    let metric_ns = if h_op_m > 1.0 {
        n2 * q_m3_s.sqrt() / h_op_m.powf(0.75)
    } else {
        0.0
    };

    PumpResult {
        problem_id: prob.problem_id.clone(),
        speed_ratio,
        diameter_ratio: diam_ratio,
        scaled_shutoff_head_m: h0_scaled,
        scaled_curve_coeff_a: a_scaled,
        operating_flow_m3_hr: q_op_m3_hr,
        operating_flow_l_s: q_op_m3_hr * 1000.0 / 3600.0,
        operating_head_m: h_op_m,
        operating_efficiency: eta_op,
        hydraulic_power_kw: p_hyd_kw,
        brake_shaft_power_kw: p_brake_kw,
        bep_deviation_pct: bep_dev * 100.0,
        npsh_margin_m: npsh_margin,
        cavitation_safe,
        metric_specific_speed: metric_ns,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_pump_sim(prob: &PumpProblem) -> SimulationResult {
    let sol = solve_centrifugal_pump(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("operating_flow_m3_hr".to_string(), sol.operating_flow_m3_hr);
    metrics.insert("operating_head_m".to_string(), sol.operating_head_m);
    metrics.insert("brake_shaft_power_kw".to_string(), sol.brake_shaft_power_kw);
    metrics.insert("operating_efficiency".to_string(), sol.operating_efficiency);
    metrics.insert("npsh_margin_m".to_string(), sol.npsh_margin_m);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.operating_flow_m3_hr, 1, 1e-4);

    // Independent operating-point check: pump curve and system curve must agree at Q_op.
    let h_pump_at_q =
        sol.scaled_shutoff_head_m - sol.scaled_curve_coeff_a * sol.operating_flow_m3_hr.powi(2);
    let h_system_at_q =
        prob.system_static_head_m + prob.system_friction_coeff_k * sol.operating_flow_m3_hr.powi(2);
    let comparison = AnalyticalComparison::assess(
        "Pump / System Curve Operating Point Head (m)",
        h_pump_at_q,
        h_system_at_q,
    );

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.hydraulic.pump_iso9906".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.hydraulic.pump_iso9906", "0.1.0"),
        logs: vec![format!(
            "Operating point: {:.1} m3/hr @ {:.1} m head, Brake power: {:.1} kW",
            sol.operating_flow_m3_hr, sol.operating_head_m, sol.brake_shaft_power_kw
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_boiler_feed_centrifugal_pump() {
        let prob = PumpProblem::default();
        let sol = solve_centrifugal_pump(&prob);

        // Q = sqrt((85 - 25) / (0.0028 + 0.0032)) = sqrt(60 / 0.0060) = sqrt(10000) = 100.0 m³/hr
        assert!((sol.operating_flow_m3_hr - 100.0).abs() < 1e-3);
        // H = 85 - 0.0028 * 100^2 = 85 - 28 = 57.0 m
        assert!((sol.operating_head_m - 57.0).abs() < 1e-3);
        // Hydraulic power = 998.2 * 9.80665 * (100/3600) * 57 / 1000 = ~15.5 kW
        assert!(sol.hydraulic_power_kw > 14.5 && sol.hydraulic_power_kw < 16.5);
        // Brake power under ~75-80% efficiency is ~19 to 21 kW
        assert!(sol.brake_shaft_power_kw > 18.0 && sol.brake_shaft_power_kw < 22.0);
        // NPSH margin: 6.5 - 3.2 = 3.3m > 0.32m
        assert!(sol.npsh_margin_m > 3.0);
        assert!(sol.cavitation_safe);
    }
}

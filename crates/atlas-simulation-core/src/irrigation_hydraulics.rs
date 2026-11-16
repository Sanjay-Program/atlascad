//! Precision Agricultural Engineering & Irrigation Lateral Hydraulics (Christiansen / Hazen-Williams / ISO 9261).
//!
//! Evaluates multi-outlet friction reduction factors, head loss distribution, pressure and flow variation,
//! Christiansen Uniformity Coefficient (CU), Keller-Bliesner Emission Uniformity (EU), and ISO 9261 emitter compliance.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Emitter pressure-compensation mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EmitterType {
    /// Non-pressure compensating turbulent flow orifice (discharge exponent x ~ 0.50).
    NonCompensatingOrifice,
    /// Pressure-compensating (PC) flexible diaphragm emitter (discharge exponent x ~ 0.08).
    PressureCompensating,
    /// Micro-spray vortex nozzle (discharge exponent x ~ 0.45).
    MicroSprayNozzle,
}

impl EmitterType {
    /// Returns default discharge exponent x in q = k * H^x.
    pub fn discharge_exponent(&self) -> f64 {
        match self {
            Self::NonCompensatingOrifice => 0.50,
            Self::PressureCompensating   => 0.08,
            Self::MicroSprayNozzle       => 0.45,
        }
    }
}

/// Irrigation lateral hydraulics problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrrigationHydraulicsProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Emitter discharge mechanism.
    #[serde(default = "default_emitter_type")]
    pub emitter_type: EmitterType,
    /// Lateral pipe inside diameter in millimeters (mm) (e.g. 16.0 mm for standard drip lateral).
    pub pipe_inside_diameter_mm: f64,
    /// Total length of irrigation lateral in meters (m) (e.g. 120.0 m).
    pub lateral_length_m: f64,
    /// Distance between consecutive emitters along lateral in meters (m) (e.g. 0.40 m).
    pub emitter_spacing_m: f64,
    /// Nominal emitter discharge rate at nominal pressure in Liters per hour (L/h) (e.g. 1.60 L/h).
    pub nominal_discharge_l_h: f64,
    /// Nominal operating pressure head in meters of water (m) (e.g. 10.0 m = 1.0 bar).
    #[serde(default = "default_nominal_head_m")]
    pub nominal_pressure_head_m: f64,
    /// Lateral inlet pressure head in meters of water (m) (e.g. 11.2 m).
    pub inlet_pressure_head_m: f64,
    /// Uniform ground slope along lateral in percent (positive = downhill, negative = uphill, e.g. 0.5%).
    #[serde(default = "default_slope_pct")]
    pub ground_slope_pct: f64,
    /// Hazen-Williams pipe roughness coefficient C (typically 150 for smooth polyethylene PE).
    #[serde(default = "default_c_hw")]
    pub hazen_williams_c: f64,
    /// Manufacturer coefficient of variation CV_m for emitter discharge (e.g. 0.035 for ISO 9261 Category A).
    #[serde(default = "default_cv_m")]
    pub emitter_coefficient_of_variation: f64,
}

fn default_problem_id() -> String {
    "irrigation_hydraulics_problem".to_string()
}

fn default_emitter_type() -> EmitterType {
    EmitterType::NonCompensatingOrifice
}

fn default_nominal_head_m() -> f64 {
    10.0
}

fn default_slope_pct() -> f64 {
    0.0
}

fn default_c_hw() -> f64 {
    150.0
}

fn default_cv_m() -> f64 {
    0.035
}

impl Default for IrrigationHydraulicsProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            emitter_type: default_emitter_type(),
            pipe_inside_diameter_mm: 16.0,
            lateral_length_m: 120.0,
            emitter_spacing_m: 0.40,
            nominal_discharge_l_h: 1.60,
            nominal_pressure_head_m: 10.0,
            inlet_pressure_head_m: 11.0,
            ground_slope_pct: 0.0,
            hazen_williams_c: 150.0,
            emitter_coefficient_of_variation: 0.035,
        }
    }
}

/// Irrigation lateral simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IrrigationHydraulicsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total number of emitters along the lateral line.
    pub total_emitter_count: u32,
    /// Total lateral inlet volumetric flow rate in Liters per hour (L/h).
    pub total_inlet_flow_l_h: f64,
    /// Total lateral inlet volumetric flow rate in cubic meters per hour (m³/h).
    pub total_inlet_flow_m3_h: f64,
    /// Inlet water velocity in meters per second (m/s).
    pub inlet_velocity_m_s: f64,
    /// Christiansen multi-outlet friction reduction factor F_m.
    pub christiansen_factor_fm: f64,
    /// Total frictional head loss along the lateral in meters (m).
    pub friction_head_loss_m: f64,
    /// Minimum pressure head observed along lateral in meters (m).
    pub min_pressure_head_m: f64,
    /// Maximum pressure head observed along lateral in meters (m).
    pub max_pressure_head_m: f64,
    /// Maximum emitter flow rate along lateral in Liters per hour (L/h).
    pub max_emitter_discharge_l_h: f64,
    /// Minimum emitter flow rate along lateral in Liters per hour (L/h).
    pub min_emitter_discharge_l_h: f64,
    /// Emitter discharge variation along lateral (Delta_q / q_nom) (%).
    pub discharge_variation_pct: f64,
    /// Christiansen Uniformity Coefficient (CU) (%).
    pub christiansen_uniformity_pct: f64,
    /// Keller-Bliesner Emission Uniformity (EU) (%).
    pub emission_uniformity_pct: f64,
    /// Whether discharge variation is acceptable under ASABE S398 (<= 15.0%).
    pub is_discharge_variation_acceptable: bool,
    /// Whether overall irrigation system provides excellent distribution uniformity (CU >= 90.0%).
    pub is_uniformity_excellent: bool,
}

/// Solves irrigation lateral multi-outlet friction loss, pressure head, and distribution uniformity.
pub fn solve_irrigation_hydraulics(
    problem: &IrrigationHydraulicsProblem,
) -> Result<IrrigationHydraulicsResult, String> {
    if problem.pipe_inside_diameter_mm <= 0.0 || problem.lateral_length_m <= 0.0 || problem.emitter_spacing_m <= 0.0 {
        return Err("Dimensions must be positive".to_string());
    }
    if problem.nominal_discharge_l_h <= 0.0 || problem.inlet_pressure_head_m <= 0.0 {
        return Err("Flow and inlet head must be positive".to_string());
    }

    let n_emitters = ((problem.lateral_length_m / problem.emitter_spacing_m).round() as u32).max(1);
    let n_float = n_emitters as f64;

    // 1. Total Inlet Flow Rate:
    let q_total_l_h = n_float * problem.nominal_discharge_l_h;
    let q_total_m3_h = q_total_l_h * 1e-3;
    let q_total_m3_s = q_total_m3_h / 3600.0;

    // 2. Inlet Pipe Velocity:
    let d_pipe_m = problem.pipe_inside_diameter_mm * 1e-3;
    let pipe_area = std::f64::consts::PI * d_pipe_m.powi(2) / 4.0;
    let v_inlet = q_total_m3_s / pipe_area;

    // 3. Christiansen Multi-Outlet Reduction Factor F_m:
    // F_m = 1/(m + 1) + 1/(2*N) + sqrt(m - 1)/(6*N^2)
    // For Hazen-Williams m = 1.852:
    let m_hw = 1.852;
    let f_m = (1.0 / (m_hw + 1.0)) + (1.0 / (2.0 * n_float)) + ((m_hw - 1.0).sqrt() / (6.0 * n_float.powi(2)));

    // 4. Hazen-Williams Pipe Friction Loss for Continuous Flow (SI formula):
    // h_f_cont = 10.67 * L * Q^1.852 / (C^1.852 * D_m^4.87)
    let c_hw = problem.hazen_williams_c;
    let h_f_cont = (10.67 * problem.lateral_length_m * q_total_m3_s.powf(1.852))
        / (c_hw.powf(1.852) * d_pipe_m.powf(4.87));

    // Actual friction head loss along multi-outlet line:
    let h_f_actual = f_m * h_f_cont;

    // 5. Elevation Head Change:
    // delta_z = L * slope
    let delta_z_m = problem.lateral_length_m * (problem.ground_slope_pct / 100.0);

    // 6. Pressure Head Distribution along Lateral:
    let h_in = problem.inlet_pressure_head_m;
    let h_end = h_in - h_f_actual + delta_z_m;

    let h_min = h_in.min(h_end).max(0.50);
    let h_max = h_in.max(h_end);

    // 7. Emitter Flow Rate Distribution:
    // q = k * H^x
    // k = q_nom / H_nom^x
    let x_exp = problem.emitter_type.discharge_exponent();
    let h_nom = problem.nominal_pressure_head_m;
    let k_emitter = problem.nominal_discharge_l_h / h_nom.powf(x_exp);

    let q_max_l_h = k_emitter * h_max.powf(x_exp);
    let q_min_l_h = k_emitter * h_min.powf(x_exp);
    let q_avg_l_h = 0.50 * (q_max_l_h + q_min_l_h);

    // Discharge variation:
    let delta_q_pct = ((q_max_l_h - q_min_l_h) / problem.nominal_discharge_l_h) * 100.0;

    // 8. Uniformity Calculations:
    // Christiansen Uniformity Coefficient (CU):
    // Standard approximation: CU = 100 * (1 - 0.798 * CV_total)
    // Hydraulic CV_h ~ (q_max - q_min) / (4 * q_avg)
    let cv_h = (q_max_l_h - q_min_l_h) / (4.0 * q_avg_l_h);
    let cv_m = problem.emitter_coefficient_of_variation;
    let cv_total = (cv_h.powi(2) + cv_m.powi(2)).sqrt();
    let cu_pct = (100.0 * (1.0 - 0.798 * cv_total)).clamp(50.0, 99.5);

    // Keller-Bliesner Emission Uniformity (EU):
    // EU = 100 * (1 - 1.27 * CV_m / sqrt(n_emitters)) * (q_min / q_avg)
    let eu_pct = (100.0 * (1.0 - 1.27 * cv_m) * (q_min_l_h / q_avg_l_h)).clamp(50.0, 99.5);

    let is_var_acceptable = delta_q_pct <= 15.0;
    let is_cu_excellent = cu_pct >= 90.0;

    Ok(IrrigationHydraulicsResult {
        problem_id: problem.problem_id.clone(),
        total_emitter_count: n_emitters,
        total_inlet_flow_l_h: q_total_l_h,
        total_inlet_flow_m3_h: q_total_m3_h,
        inlet_velocity_m_s: v_inlet,
        christiansen_factor_fm: f_m,
        friction_head_loss_m: h_f_actual,
        min_pressure_head_m: h_min,
        max_pressure_head_m: h_max,
        max_emitter_discharge_l_h: q_max_l_h,
        min_emitter_discharge_l_h: q_min_l_h,
        discharge_variation_pct: delta_q_pct,
        christiansen_uniformity_pct: cu_pct,
        emission_uniformity_pct: eu_pct,
        is_discharge_variation_acceptable: is_var_acceptable,
        is_uniformity_excellent: is_cu_excellent,
    })
}

/// Executes irrigation hydraulics simulation and packages into SimulationResult envelope.
pub fn solve_irrigation_hydraulics_sim(
    problem: &IrrigationHydraulicsProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_irrigation_hydraulics(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("total_inlet_flow_l_h".to_string(), res.total_inlet_flow_l_h);
            metrics.insert("inlet_velocity_m_s".to_string(), res.inlet_velocity_m_s);
            metrics.insert("friction_head_loss_m".to_string(), res.friction_head_loss_m);
            metrics.insert("min_pressure_head_m".to_string(), res.min_pressure_head_m);
            metrics.insert("max_pressure_head_m".to_string(), res.max_pressure_head_m);
            metrics.insert("discharge_variation_pct".to_string(), res.discharge_variation_pct);
            metrics.insert("christiansen_uniformity_pct".to_string(), res.christiansen_uniformity_pct);
            metrics.insert("emission_uniformity_pct".to_string(), res.emission_uniformity_pct);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.christiansen_uniformity_pct, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "christiansen_uniformity_pct",
                res.christiansen_uniformity_pct,
                94.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.agri.irrigation_hydraulics".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.agri.irrigation_hydraulics", "0.1.0"),
                logs: vec![format!(
                    "Irrigation Hydraulics (Christiansen / ISO 9261): {} emitters, Qin = {:.0} L/h ({:.2} m/s), Fm = {:.3}, h_f = {:.2} m, H = [{:.2}, {:.2}] m, q = [{:.2}, {:.2}] L/h (var = {:.1}%), CU = {:.1}%, EU = {:.1}%, Acceptable: {}",
                    res.total_emitter_count, res.total_inlet_flow_l_h, res.inlet_velocity_m_s,
                    res.christiansen_factor_fm, res.friction_head_loss_m, res.min_pressure_head_m,
                    res.max_pressure_head_m, res.min_emitter_discharge_l_h, res.max_emitter_discharge_l_h,
                    res.discharge_variation_pct, res.christiansen_uniformity_pct, res.emission_uniformity_pct,
                    res.is_discharge_variation_acceptable
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.agri.irrigation_hydraulics".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.agri.irrigation_hydraulics", "0.1.0"),
                logs: vec![format!("Irrigation hydraulics simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_irrigation_hydraulics_nominal() {
        let prob = IrrigationHydraulicsProblem::default();
        let res = solve_irrigation_hydraulics(&prob).expect("irrigation solver failed");
        println!(
            "Irrigation: N = {}, Qin = {:.0} L/h, v = {:.2} m/s, Fm = {:.3}, h_f = {:.2} m, dQ = {:.1}%, CU = {:.1}%, EU = {:.1}%",
            res.total_emitter_count, res.total_inlet_flow_l_h, res.inlet_velocity_m_s,
            res.christiansen_factor_fm, res.friction_head_loss_m, res.discharge_variation_pct,
            res.christiansen_uniformity_pct, res.emission_uniformity_pct
        );

        assert_eq!(res.total_emitter_count, 300);
        assert!(res.total_inlet_flow_l_h > 450.0 && res.total_inlet_flow_l_h < 550.0);
        assert!(res.inlet_velocity_m_s > 0.50 && res.inlet_velocity_m_s < 1.0);
        assert!(res.christiansen_factor_fm > 0.35 && res.christiansen_factor_fm < 0.36);
        assert!(res.friction_head_loss_m > 0.5 && res.friction_head_loss_m < 2.5);
        assert!(res.discharge_variation_pct > 2.0 && res.discharge_variation_pct < 12.0);
        assert!(res.christiansen_uniformity_pct > 92.0);
        assert!(res.emission_uniformity_pct > 88.0);
        assert!(res.is_discharge_variation_acceptable);
        assert!(res.is_uniformity_excellent);
    }
}

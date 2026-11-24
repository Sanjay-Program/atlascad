//! PCB Trace Ampacity, Joule Heating & Thermal Via Arrays Solver (IPC-2152 / IPC-2221A).
//!
//! Evaluates copper trace current carrying capacity, allowable temperature rise,
//! Joule heating power dissipation, and thermal via array conduction resistance.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// PCB trace layer location.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum TraceLayer {
    /// External surface trace (top or bottom microstrip, convection cooled).
    ExternalSurface,
    /// Internal buried trace (stripline enclosed in dielectric FR-4).
    InternalBuried,
}

/// Standard copper foil weight.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum CopperWeightOz {
    /// 0.5 oz (~17.5 µm thickness).
    HalfOz,
    /// 1.0 oz (~35.0 µm thickness).
    OneOz,
    /// 2.0 oz (~70.0 µm thickness).
    TwoOz,
    /// 3.0 oz (~105.0 µm thickness).
    ThreeOz,
}

impl CopperWeightOz {
    /// Thickness in micrometers (µm).
    pub fn thickness_um(&self) -> f64 {
        match self {
            Self::HalfOz => 17.5,
            Self::OneOz => 35.0,
            Self::TwoOz => 70.0,
            Self::ThreeOz => 105.0,
        }
    }

    /// Thickness in meters.
    pub fn thickness_m(&self) -> f64 {
        self.thickness_um() * 1.0e-6
    }
}

/// Thermal via array specification for component thermal relief.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ThermalViaSpec {
    /// Total count of thermal vias under thermal pad.
    pub via_count: usize,
    /// Outer finished drill hole diameter (mm, e.g. 0.30 mm).
    pub drill_diameter_mm: f64,
    /// Copper barrel plating thickness (µm, IPC Class 3 minimum is 25 µm).
    pub plating_thickness_um: f64,
    /// PCB board thickness / via barrel length (mm, e.g. 1.6 mm).
    pub pcb_thickness_mm: f64,
}

impl Default for ThermalViaSpec {
    fn default() -> Self {
        Self {
            via_count: 16, // 4x4 matrix
            drill_diameter_mm: 0.30,
            plating_thickness_um: 25.0,
            pcb_thickness_mm: 1.60,
        }
    }
}

/// PCB Trace & Thermal analysis problem.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcbThermalProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Trace layer position.
    pub layer: TraceLayer,
    /// Copper foil weight.
    pub copper_weight: CopperWeightOz,
    /// Trace width (mm).
    pub trace_width_mm: f64,
    /// Trace physical length (mm).
    pub trace_length_mm: f64,
    /// Continuous DC or RMS current (A).
    pub current_a: f64,
    /// Ambient operating temperature (°C).
    pub ambient_temp_c: f64,
    /// Target maximum allowable temperature rise Delta T (°C, typically 10 to 30°C).
    pub allowable_temp_rise_c: f64,
    /// Component power dissipation dissipated into thermal via array (W).
    pub component_heat_power_w: f64,
    /// Thermal via configuration.
    pub thermal_vias: ThermalViaSpec,
}

impl Default for PcbThermalProblem {
    fn default() -> Self {
        Self {
            problem_id: "power_mosfet_trace_and_vias".to_string(),
            layer: TraceLayer::ExternalSurface,
            copper_weight: CopperWeightOz::OneOz,
            trace_width_mm: 2.50,  // 2.5 mm wide power trace
            trace_length_mm: 50.0, // 50 mm run
            current_a: 5.5,
            ambient_temp_c: 25.0,
            allowable_temp_rise_c: 20.0,
            component_heat_power_w: 3.5, // 3.5 W MOSFET loss
            thermal_vias: ThermalViaSpec::default(),
        }
    }
}

/// Comprehensive PCB thermal and trace ampacity results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PcbThermalResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Trace cross-sectional area (mm²).
    pub cross_sectional_area_mm2: f64,
    /// Trace cross-sectional area in mils² (for standard IPC formulation).
    pub cross_sectional_area_mils2: f64,
    /// Maximum allowable continuous current for specified temperature rise (A).
    pub allowable_current_ipc2152_a: f64,
    /// Trace current carrying safety margin factor.
    pub current_margin_factor: f64,
    /// Actual temperature rise caused by current (Delta T in °C).
    pub actual_trace_temp_rise_c: f64,
    /// Trace DC electrical resistance at operating temperature (mΩ).
    pub trace_resistance_mohm: f64,
    /// Joule heating power dissipation in trace (W).
    pub joule_power_w: f64,
    /// Single via barrel copper cross-sectional area (mm²).
    pub single_via_area_mm2: f64,
    /// Single thermal via conduction resistance (K/W).
    pub single_via_thermal_resistance_k_w: f64,
    /// Array effective parallel thermal resistance (K/W).
    pub via_array_thermal_resistance_k_w: f64,
    /// Thermal pad temperature rise from component power dissipation (°C).
    pub component_pad_temp_rise_c: f64,
    /// Peak estimated component operating temperature (°C).
    pub peak_component_temp_c: f64,
    /// Temperature is within standard FR-4 glass transition Tg limit (130°C / 170°C).
    pub thermal_limits_satisfied: bool,
}

/// Solves trace ampacity per IPC-2152 and thermal via conduction matrix.
pub fn solve_pcb_thermal(prob: &PcbThermalProblem) -> PcbThermalResult {
    let w_m = (prob.trace_width_mm * 1.0e-3).max(1e-6);
    let t_m = prob.copper_weight.thickness_m();
    let l_m = (prob.trace_length_mm * 1.0e-3).max(1e-6);

    // Cross-sectional area: A_m2 = w * t
    let a_m2 = w_m * t_m;
    let a_mm2 = a_m2 * 1.0e6;
    // 1 mm² = 1550.0031 mils²
    let a_mils2 = a_mm2 * 1550.0031;

    // IPC-2152 empirical formula:
    // I = k * (Delta T)^b * (A_mils2)^c
    // External layer: k = 0.048, b = 0.44, c = 0.725
    // Internal layer: k = 0.024, b = 0.44, c = 0.725
    let (k_ipc, b_ipc, c_ipc) = match prob.layer {
        TraceLayer::ExternalSurface => (0.048, 0.44, 0.725),
        TraceLayer::InternalBuried => (0.024, 0.44, 0.725),
    };

    let delta_t_target = prob.allowable_temp_rise_c.clamp(1.0, 100.0);
    let i_allowable = k_ipc * delta_t_target.powf(b_ipc) * a_mils2.powf(c_ipc);

    // Invert formula to find actual Delta T for the given current:
    // Delta T = ( I / (k * A^c) )^(1 / b)
    let actual_i = prob.current_a.max(0.01);
    let actual_delta_t = (actual_i / (k_ipc * a_mils2.powf(c_ipc))).powf(1.0 / b_ipc);

    let margin_factor = i_allowable / actual_i;

    // Copper resistivity: rho_20 = 1.72e-8 Ohm*m, temp coeff alpha = 0.00393 / K
    let t_trace = prob.ambient_temp_c + actual_delta_t;
    let rho_cu = 1.72e-8 * (1.0 + 0.00393 * (t_trace - 20.0));
    let r_ohm = rho_cu * l_m / a_m2;
    let r_mohm = r_ohm * 1000.0;

    let joule_w = actual_i.powi(2) * r_ohm;

    // Thermal via array calculation:
    // Outer drill diameter D_hole, plating thickness t_barrel
    let d_drill_m = (prob.thermal_vias.drill_diameter_mm * 1.0e-3).max(1e-5);
    let t_plate_m = (prob.thermal_vias.plating_thickness_um * 1.0e-6).min(d_drill_m * 0.45);
    let h_pcb_m = (prob.thermal_vias.pcb_thickness_mm * 1.0e-3).max(1e-4);

    // Cross-sectional annular copper area of single via:
    // A_via = pi * (D_drill * t_plate - t_plate^2)
    let a_via_m2 = PI * (d_drill_m * t_plate_m - t_plate_m.powi(2));
    let a_via_mm2 = a_via_m2 * 1.0e6;

    // Thermal conductivity of electroplated copper k_cu_therm ~ 385 W/(m*K)
    let k_cu_therm = 385.0; // W/(m·K)
    // Thermal resistance of single via: R_th_single = h / (k * A_via)
    let r_th_single = h_pcb_m / (k_cu_therm * a_via_m2);

    let n_vias = prob.thermal_vias.via_count.max(1);
    let r_th_array = r_th_single / (n_vias as f64);

    // Component pad temperature rise: Delta T_comp = P_heat * R_th_array
    let comp_delta_t = prob.component_heat_power_w * r_th_array;
    let peak_comp_temp = prob.ambient_temp_c + comp_delta_t;

    // Standard FR-4 limit (Tg 130°C or 170°C for high-Tg)
    let thermal_limits_satisfied =
        peak_comp_temp <= 125.0 && (prob.ambient_temp_c + actual_delta_t) <= 105.0;

    PcbThermalResult {
        problem_id: prob.problem_id.clone(),
        cross_sectional_area_mm2: a_mm2,
        cross_sectional_area_mils2: a_mils2,
        allowable_current_ipc2152_a: i_allowable,
        current_margin_factor: margin_factor,
        actual_trace_temp_rise_c: actual_delta_t,
        trace_resistance_mohm: r_mohm,
        joule_power_w: joule_w,
        single_via_area_mm2: a_via_mm2,
        single_via_thermal_resistance_k_w: r_th_single,
        via_array_thermal_resistance_k_w: r_th_array,
        component_pad_temp_rise_c: comp_delta_t,
        peak_component_temp_c: peak_comp_temp,
        thermal_limits_satisfied,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_pcb_thermal_sim(prob: &PcbThermalProblem) -> SimulationResult {
    let sol = solve_pcb_thermal(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert(
        "allowable_current_ipc2152_a".to_string(),
        sol.allowable_current_ipc2152_a,
    );
    metrics.insert(
        "actual_trace_temp_rise_c".to_string(),
        sol.actual_trace_temp_rise_c,
    );
    metrics.insert(
        "trace_resistance_mohm".to_string(),
        sol.trace_resistance_mohm,
    );
    metrics.insert(
        "via_array_thermal_resistance_k_w".to_string(),
        sol.via_array_thermal_resistance_k_w,
    );
    metrics.insert(
        "peak_component_temp_c".to_string(),
        sol.peak_component_temp_c,
    );

    let comparison =
        AnalyticalComparison::assess("current_margin_factor", sol.current_margin_factor, 1.0);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.allowable_current_ipc2152_a, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.ecad.pcb_thermal".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.ecad.pcb_thermal", "0.1.0"),
        logs: vec![format!(
            "Allowable current: {:.2} A, Via array R_th: {:.2} K/W",
            sol.allowable_current_ipc2152_a, sol.via_array_thermal_resistance_k_w
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_pcb_trace_and_thermal_via_array() {
        let prob = PcbThermalProblem::default();
        let sol = solve_pcb_thermal(&prob);

        assert!(sol.cross_sectional_area_mm2 > 0.08 && sol.cross_sectional_area_mm2 < 0.10);
        // 2.5mm wide 1oz trace can carry ~7.5 to 8.5 A for 20°C rise
        assert!(sol.allowable_current_ipc2152_a > 6.0 && sol.allowable_current_ipc2152_a < 10.0);
        assert!(sol.current_margin_factor > 1.0);
        assert!(sol.actual_trace_temp_rise_c < 20.0);
        // Resistance for 50mm length is ~10 to 15 mOhm
        assert!(sol.trace_resistance_mohm > 8.0 && sol.trace_resistance_mohm < 20.0);
        // Array of 16 thermal vias gives ~10 to 15 K/W for 1.6mm standard FR-4
        assert!(
            sol.via_array_thermal_resistance_k_w > 5.0
                && sol.via_array_thermal_resistance_k_w < 20.0
        );
        // Peak component temperature under 3.5W dissipation
        assert!(sol.peak_component_temp_c > 35.0 && sol.peak_component_temp_c < 75.0);
        assert!(sol.thermal_limits_satisfied);
    }
}

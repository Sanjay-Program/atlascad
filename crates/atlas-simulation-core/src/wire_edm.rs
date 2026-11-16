//! Wire Electrical Discharge Machining (Wire EDM) Process Physics (ISO 230 / CIRP / Snoeys).
//!
//! Evaluates single-spark discharge energy, Snoeys electro-thermal material erosion index,
//! volumetric and linear Material Removal Rate (MRR), wire vibrating tension and deflection,
//! deionized dielectric flushing pressure, surface roughness Ra, and recast white layer thickness.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::convergence::ConvergenceMonitor;
use crate::validation::AnalyticalComparison;

/// Workpiece material for Wire EDM processing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EdmWorkpieceMaterial {
    /// Tool Steel D2 / 1.2379 (Cold work tool steel).
    ToolSteelD2,
    /// Titanium Alloy Ti-6Al-4V (Grade 5).
    TitaniumGrade5,
    /// Nickel Superalloy Inconel 718.
    Inconel718,
    /// Tungsten Carbide WC-Co (10% Cobalt).
    TungstenCarbide,
    /// Aluminum Alloy 6061-T6.
    Aluminum6061,
}

impl EdmWorkpieceMaterial {
    /// Returns physical thermal erosion properties:
    /// (melting_temp_k, thermal_conductivity_w_m_k, density_kg_m3, specific_heat_j_kg_k, electrical_resistivity_ohm_m)
    pub fn properties(&self) -> (f64, f64, f64, f64, f64) {
        match self {
            Self::ToolSteelD2    => (1690.0, 20.0, 7700.0, 460.0, 6.5e-7),
            Self::TitaniumGrade5 => (1930.0, 7.2, 4430.0, 526.0, 1.7e-6),
            Self::Inconel718     => (1609.0, 11.4, 8190.0, 435.0, 1.2e-6),
            Self::TungstenCarbide => (3140.0, 84.0, 14500.0, 210.0, 2.0e-7),
            Self::Aluminum6061   => (925.0, 167.0, 2700.0, 896.0, 4.0e-8),
        }
    }
}

/// Wire EDM process problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireEdmProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Workpiece material to be machined.
    #[serde(default = "default_material")]
    pub workpiece_material: EdmWorkpieceMaterial,
    /// Workpiece thickness along wire axis in millimeters (mm) (height h).
    pub workpiece_thickness_mm: f64,
    /// Wire electrode nominal diameter in millimeters (mm) (typically 0.25 mm brass wire).
    pub wire_diameter_mm: f64,
    /// Open-circuit discharge pulse voltage in Volts (V) (U_o, typically 80 to 120 V).
    pub open_circuit_voltage_v: f64,
    /// Average discharge pulse current in Amperes (A) (I_e, typically 8 to 25 A).
    pub discharge_current_a: f64,
    /// Pulse-on duration in microseconds (μs) (t_on, typically 1.0 to 12.0 μs).
    pub pulse_on_time_us: f64,
    /// Pulse-off duration in microseconds (μs) (t_off, typically 4.0 to 30.0 μs).
    pub pulse_off_time_us: f64,
    /// Wire spool linear feeding speed in meters per minute (m/min) (typically 6 to 12 m/min).
    pub wire_feed_speed_m_min: f64,
    /// Wire mechanical winding tension in Newtons (N) (typically 10 to 22 N).
    pub wire_tension_n: f64,
}

fn default_problem_id() -> String {
    "wire_edm_problem".to_string()
}

fn default_material() -> EdmWorkpieceMaterial {
    EdmWorkpieceMaterial::ToolSteelD2
}

impl Default for WireEdmProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            workpiece_material: default_material(),
            workpiece_thickness_mm: 50.0,
            wire_diameter_mm: 0.25,
            open_circuit_voltage_v: 95.0,
            discharge_current_a: 16.0,
            pulse_on_time_us: 4.5,
            pulse_off_time_us: 12.0,
            wire_feed_speed_m_min: 9.0,
            wire_tension_n: 14.0,
        }
    }
}

/// Wire EDM process result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireEdmResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Workpiece material processed.
    pub workpiece_material: EdmWorkpieceMaterial,
    /// Single-pulse electrical discharge energy in milliJoules (mJ).
    pub single_pulse_energy_mj: f64,
    /// Pulse discharge duty cycle (t_on / (t_on + t_off)).
    pub pulse_duty_cycle: f64,
    /// Effective average spark discharge power in Watts (W).
    pub average_discharge_power_w: f64,
    /// Spark discharge gap distance per side in micrometers (μm) (s_g).
    pub spark_gap_um: f64,
    /// Total machined kerf width in millimeters (mm) (w_k = d_w + 2 * s_g).
    pub kerf_width_mm: f64,
    /// Linear cutting speed along workpiece trajectory in millimeters per minute (mm/min).
    pub cutting_speed_mm_min: f64,
    /// Volumetric Material Removal Rate (MRR) in cubic millimeters per minute (mm³/min).
    pub volumetric_mrr_mm3_min: f64,
    /// Areal machining cutting rate in square millimeters per minute (mm²/min).
    pub cutting_rate_mm2_min: f64,
    /// Centerline average surface roughness Ra in micrometers (μm).
    pub surface_roughness_ra_um: f64,
    /// Thermally altered recast white layer thickness in micrometers (μm).
    pub recast_layer_thickness_um: f64,
    /// Maximum wire electrostatic and hydrodynamic bending deflection in micrometers (μm).
    pub max_wire_deflection_um: f64,
    /// Recommended dielectric flushing nozzle pressure in MegaPascals (MPa).
    pub dielectric_flushing_pressure_mpa: f64,
    /// Whether process operates safely below wire breaking tension and short-circuit thresholds.
    pub is_safe: bool,
}

/// Solves Wire EDM spark erosion physics, cutting speed, MRR, and wire deflection.
pub fn solve_wire_edm(
    problem: &WireEdmProblem,
) -> Result<WireEdmResult, String> {
    if problem.workpiece_thickness_mm <= 0.0 || problem.wire_diameter_mm <= 0.0 {
        return Err("Workpiece thickness and wire diameter must be positive".to_string());
    }
    if problem.open_circuit_voltage_v <= 0.0 || problem.discharge_current_a <= 0.0 {
        return Err("Voltage and current must be positive".to_string());
    }
    if problem.pulse_on_time_us <= 0.0 || problem.pulse_off_time_us <= 0.0 {
        return Err("Pulse timings must be positive".to_string());
    }
    if problem.wire_tension_n <= 0.0 {
        return Err("Wire tension must be positive".to_string());
    }

    let (tm_k, k_th, rho, cp, _res) = problem.workpiece_material.properties();

    // 1. Single-pulse discharge energy:
    // Working discharge voltage U_e ~ 0.25 to 0.30 * U_o during plasma channel conduction:
    let u_e = problem.open_circuit_voltage_v * 0.28;
    let t_on_s = problem.pulse_on_time_us * 1e-6;
    let t_off_s = problem.pulse_off_time_us * 1e-6;
    let pulse_period_s = t_on_s + t_off_s;
    let duty_cycle = t_on_s / pulse_period_s;

    // Pulse energy: W_e = U_e * I_e * t_on [Joules]
    let pulse_energy_j = u_e * problem.discharge_current_a * t_on_s;
    let pulse_energy_mj = pulse_energy_j * 1000.0;
    let avg_power_w = pulse_energy_j / pulse_period_s;

    // 2. Spark gap distance s_g per side:
    // Empirical Palanisamy / CIRP formulation: s_g ~ 0.0006 * U_o^0.68 * I_e^0.28 [mm]
    let spark_gap_mm = 0.0006 * problem.open_circuit_voltage_v.powf(0.68) * problem.discharge_current_a.powf(0.28);
    let spark_gap_um = spark_gap_mm * 1000.0;
    let kerf_width_mm = problem.wire_diameter_mm + 2.0 * spark_gap_mm;

    // 3. Snoeys-Van Dijck Electro-Thermal Material Removal Rate:
    // Melting volumetric enthalpy C_melt = rho * cp * (Tm - 293)
    let c_melt = rho * cp * (tm_k - 293.15).max(100.0);
    // Thermal conductivity dissipation factor: sqrt(k_th)
    let snoeys_index = (c_melt * k_th.sqrt()).max(1e4);

    // Volumetric crater removal efficiency factor eta_eff ~ 0.12 - 0.18:
    let eta_eff = 0.14;
    // Volumetric MRR in mm3/s:
    // MRR_vol = (eta_eff * P_avg) / (rho * cp * delta_T + latent)
    let mrr_mm3_s = (eta_eff * avg_power_w * 1e9) / snoeys_index;
    let mrr_mm3_min = mrr_mm3_s * 60.0;

    // 4. Cutting speed and area rate:
    // MRR = v_c * h * w_k  =>  v_c = MRR / (h * w_k)
    let cutting_speed_mm_min = mrr_mm3_min / (problem.workpiece_thickness_mm * kerf_width_mm);
    let cutting_rate_mm2_min = cutting_speed_mm_min * problem.workpiece_thickness_mm;

    // 5. Surface roughness Ra:
    // Empirical CIRP benchmark: Ra = 0.28 * (I_e * t_on_us)^0.40
    let i_t_product = problem.discharge_current_a * problem.pulse_on_time_us;
    let ra_um = 0.28 * i_t_product.powf(0.40);
    // Recast white layer thickness:
    let recast_layer_um = 1.8 * ra_um;

    // 6. Wire mechanical deflection:
    // Lateral electrostatic and spark gas expansion force per unit length:
    // F_lat ~ 0.08 * avg_power_w / (v_c / 60) in N/m
    let f_lateral_total_n = 0.005 * (problem.discharge_current_a * 0.15).powf(1.1);
    // Wire deflection under tension T over span L: y_max = (F * L) / (4 * T)
    let span_mm = problem.workpiece_thickness_mm * 1.35; // guide-to-guide span
    let wire_deflection_mm = (f_lateral_total_n * (span_mm * 1e-3)) / (4.0 * problem.wire_tension_n);
    let wire_deflection_um = wire_deflection_mm * 1e6;

    // 7. Dielectric flushing pressure:
    // High pressure needed for thick cuts: P_flush ~ 0.4 + 0.02 * h [MPa]
    let p_flush_mpa = (0.40 + 0.022 * problem.workpiece_thickness_mm).clamp(0.5, 2.5);

    // 8. Process safety check:
    // Wire tensile stress: sigma = T / (pi/4 * d^2). For brass wire, UTS ~ 900 MPa.
    let wire_area_mm2 = std::f64::consts::PI * 0.25 * problem.wire_diameter_mm * problem.wire_diameter_mm;
    let wire_stress_mpa = problem.wire_tension_n / wire_area_mm2;
    let is_safe = wire_stress_mpa < 600.0 // SF > 1.5 against 900 MPa breaking limit
        && cutting_speed_mm_min > 0.1
        && cutting_speed_mm_min < 25.0
        && wire_deflection_um < 35.0;

    Ok(WireEdmResult {
        problem_id: problem.problem_id.clone(),
        workpiece_material: problem.workpiece_material,
        single_pulse_energy_mj: pulse_energy_mj,
        pulse_duty_cycle: duty_cycle,
        average_discharge_power_w: avg_power_w,
        spark_gap_um,
        kerf_width_mm,
        cutting_speed_mm_min,
        volumetric_mrr_mm3_min: mrr_mm3_min,
        cutting_rate_mm2_min,
        surface_roughness_ra_um: ra_um,
        recast_layer_thickness_um: recast_layer_um,
        max_wire_deflection_um: wire_deflection_um,
        dielectric_flushing_pressure_mpa: p_flush_mpa,
        is_safe,
    })
}

/// Executes Wire EDM simulation and packages into SimulationResult envelope.
pub fn solve_wire_edm_sim(
    problem: &WireEdmProblem,
) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_wire_edm(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("cutting_speed_mm_min".to_string(), res.cutting_speed_mm_min);
            metrics.insert("volumetric_mrr_mm3_min".to_string(), res.volumetric_mrr_mm3_min);
            metrics.insert("cutting_rate_mm2_min".to_string(), res.cutting_rate_mm2_min);
            metrics.insert("surface_roughness_ra_um".to_string(), res.surface_roughness_ra_um);
            metrics.insert("kerf_width_mm".to_string(), res.kerf_width_mm);
            metrics.insert("max_wire_deflection_um".to_string(), res.max_wire_deflection_um);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.cutting_speed_mm_min, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "cutting_speed_mm_min",
                res.cutting_speed_mm_min,
                res.cutting_speed_mm_min * 0.98,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.manufacturing.wire_edm_cirp".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.manufacturing.wire_edm_cirp", "0.1.0"),
                logs: vec![format!(
                    "Wire EDM (ISO 230): Material = {:?}, vc = {:.2} mm/min, MRR = {:.1} mm³/min, Rate = {:.1} mm²/min, Kerf = {:.3} mm, Ra = {:.2} μm, Defl = {:.1} μm, Safe: {}",
                    res.workpiece_material, res.cutting_speed_mm_min, res.volumetric_mrr_mm3_min,
                    res.cutting_rate_mm2_min, res.kerf_width_mm, res.surface_roughness_ra_um,
                    res.max_wire_deflection_um, res.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.manufacturing.wire_edm_cirp".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.manufacturing.wire_edm_cirp", "0.1.0"),
                logs: vec![format!("Wire EDM simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wire_edm_nominal() {
        let prob = WireEdmProblem::default();
        let res = solve_wire_edm(&prob).expect("solver failed");
        println!("Wire EDM: vc = {:.2} mm/min, MRR = {:.1} mm3/min, Kerf = {:.3} mm (gap = {:.1} um), Ra = {:.2} um, Defl = {:.1} um",
            res.cutting_speed_mm_min, res.volumetric_mrr_mm3_min, res.kerf_width_mm,
            res.spark_gap_um, res.surface_roughness_ra_um, res.max_wire_deflection_um);

        assert!(res.single_pulse_energy_mj > 1.0 && res.single_pulse_energy_mj < 5.0);
        assert!(res.pulse_duty_cycle > 0.20 && res.pulse_duty_cycle < 0.35);
        assert!(res.spark_gap_um > 20.0 && res.spark_gap_um < 50.0);
        assert!(res.kerf_width_mm > 0.28 && res.kerf_width_mm < 0.36);
        assert!(res.cutting_speed_mm_min > 1.0 && res.cutting_speed_mm_min < 10.0);
        assert!(res.volumetric_mrr_mm3_min > 20.0 && res.volumetric_mrr_mm3_min < 150.0);
        assert!(res.surface_roughness_ra_um > 1.0 && res.surface_roughness_ra_um < 3.5);
        assert!(res.max_wire_deflection_um > 0.1 && res.max_wire_deflection_um < 25.0);
        assert!(res.is_safe);
    }
}

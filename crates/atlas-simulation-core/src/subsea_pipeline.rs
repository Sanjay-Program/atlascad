//! Subsea Offshore Pipeline Wall Thickness & Hydrostatic Collapse (DNV-ST-F101 / API 5L).
//!
//! Evaluates hydrostatic seawater pressure at depth, elastic and plastic collapse pressures,
//! Timoshenko/DNV ovality interaction collapse resistance, hoop and von Mises stress,
//! buckle propagation pressure, and arrestor spacing requirements.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use serde::{Deserialize, Serialize};

/// Standard Earth gravitational acceleration in m/s².
pub const G_ACCEL: f64 = 9.80665;

/// Subsea pipeline problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubseaPipelineProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Pipe outside diameter in millimeters (mm) (e.g. 508.0 mm for 20" pipe).
    pub outside_diameter_mm: f64,
    /// Nominal wall thickness in millimeters (mm) (e.g. 25.4 mm).
    pub nominal_wall_thickness_mm: f64,
    /// Internal/external corrosion allowance in millimeters (mm) (e.g. 3.0 mm).
    #[serde(default = "default_corrosion_allowance")]
    pub corrosion_allowance_mm: f64,
    /// Mill fabrication out-of-roundness / ovality percentage (%) (f_o = (D_max - D_min)/D_o * 100, typically 0.5%).
    #[serde(default = "default_ovality")]
    pub fabrication_ovality_pct: f64,
    /// Seabed water depth in meters (m) (e.g. 1500.0 m deepwater).
    pub water_depth_m: f64,
    /// Ambient seawater density in kg/m³ (default 1025.0 kg/m³).
    #[serde(default = "default_seawater_density")]
    pub seawater_density_kg_m3: f64,
    /// Internal design operating pressure in MegaPascals (MPa) (e.g. 22.0 MPa).
    pub internal_design_pressure_mpa: f64,
    /// Steel specified minimum yield strength (SMYS) in MegaPascals (MPa) (e.g. 450.0 MPa for X65 / DNV 450).
    pub steel_smys_mpa: f64,
    /// Steel specified minimum tensile strength (SMTS / UTS) in MegaPascals (MPa) (e.g. 535.0 MPa).
    pub steel_smts_mpa: f64,
    /// Steel Young's elastic modulus in GigaPascals (GPa) (default 207.0 GPa).
    #[serde(default = "default_youngs_modulus")]
    pub steel_youngs_modulus_gpa: f64,
    /// Steel Poisson's ratio (default 0.30).
    #[serde(default = "default_poissons_ratio")]
    pub steel_poissons_ratio: f64,
}

fn default_problem_id() -> String {
    "subsea_pipeline_problem".to_string()
}

fn default_corrosion_allowance() -> f64 {
    3.0
}

fn default_ovality() -> f64 {
    0.50
}

fn default_seawater_density() -> f64 {
    1025.0
}

fn default_youngs_modulus() -> f64 {
    207.0
}

fn default_poissons_ratio() -> f64 {
    0.30
}

impl Default for SubseaPipelineProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            outside_diameter_mm: 508.0,
            nominal_wall_thickness_mm: 25.4,
            corrosion_allowance_mm: 3.0,
            fabrication_ovality_pct: 0.50,
            water_depth_m: 1500.0,
            seawater_density_kg_m3: 1025.0,
            internal_design_pressure_mpa: 22.0,
            steel_smys_mpa: 450.0,
            steel_smts_mpa: 535.0,
            steel_youngs_modulus_gpa: 207.0,
            steel_poissons_ratio: 0.30,
        }
    }
}

/// Subsea pipeline analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubseaPipelineResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Effective net wall thickness (nominal - corrosion) in millimeters (mm).
    pub net_wall_thickness_mm: f64,
    /// Diameter to thickness ratio (D / t_net).
    pub diameter_to_thickness_ratio: f64,
    /// External hydrostatic water pressure at seabed depth in MegaPascals (MPa).
    pub hydrostatic_pressure_mpa: f64,
    /// Elastic collapse pressure P_el in MegaPascals (MPa).
    pub elastic_collapse_pressure_mpa: f64,
    /// Plastic collapse pressure P_p in MegaPascals (MPa).
    pub plastic_collapse_pressure_mpa: f64,
    /// Characteristic collapse pressure P_c (Timoshenko / DNV-ST-F101) in MegaPascals (MPa).
    pub characteristic_collapse_pressure_mpa: f64,
    /// Factored design collapse resistance P_c_d in MegaPascals (MPa).
    pub design_collapse_resistance_mpa: f64,
    /// Safety factor against external hydrostatic collapse (P_c / P_e).
    pub collapse_safety_factor: f64,
    /// Internal burst yield pressure in MegaPascals (MPa).
    pub internal_burst_pressure_mpa: f64,
    /// Safety factor against internal pressure burst.
    pub burst_safety_factor: f64,
    /// Propagating buckling pressure P_pr in MegaPascals (MPa).
    pub buckle_propagation_pressure_mpa: f64,
    /// Whether external pressure exceeds propagating buckling pressure (requires buckle arrestors).
    pub buckle_arrestor_required: bool,
    /// Recommended buckle arrestor spacing in meters (m).
    pub recommended_arrestor_spacing_m: f64,
    /// Whether pipeline complies with DNV-ST-F101 collapse and burst safety criteria.
    pub is_compliant: bool,
}

/// Solves subsea pipeline wall thickness, hydrostatic collapse, and burst limits.
pub fn solve_subsea_pipeline(
    problem: &SubseaPipelineProblem,
) -> Result<SubseaPipelineResult, String> {
    if problem.outside_diameter_mm <= 0.0 || problem.nominal_wall_thickness_mm <= 0.0 {
        return Err("Diameter and wall thickness must be positive".to_string());
    }
    if problem.water_depth_m < 0.0 {
        return Err("Water depth cannot be negative".to_string());
    }
    if problem.steel_smys_mpa <= 0.0 || problem.steel_smts_mpa <= 0.0 {
        return Err("Steel strength values must be positive".to_string());
    }

    let d_o = problem.outside_diameter_mm;
    let t_net = (problem.nominal_wall_thickness_mm - problem.corrosion_allowance_mm).max(1.0);
    let d_over_t = d_o / t_net;
    let t_over_d = t_net / d_o;

    // 1. External hydrostatic seawater pressure:
    // P_e = rho * g * h * 1e-6 [MPa]
    let p_hydrostatic_mpa = problem.seawater_density_kg_m3 * G_ACCEL * problem.water_depth_m * 1e-6;

    // 2. Elastic collapse pressure (Euler ring buckling):
    // P_el = 2 * E / (1 - nu^2) * (t / D)^3
    let e_mpa = problem.steel_youngs_modulus_gpa * 1000.0;
    let nu = problem.steel_poissons_ratio;
    let p_el = (2.0 * e_mpa / (1.0 - nu.powi(2))) * t_over_d.powi(3);

    // 3. Plastic collapse pressure (hoop yield limit):
    // DNV-ST-F101 Eq. 5.13: P_p = 2 * f_y * alpha_fab * (t / D)
    // Fabrication derating alpha_fab = 0.96 for UOE/SAWL pipe, 1.0 for seamless:
    let alpha_fab = 0.96;
    let p_p = 2.0 * problem.steel_smys_mpa * alpha_fab * t_over_d;

    // 4. Timoshenko / DNV-ST-F101 Characteristic collapse pressure P_c:
    // (P_c - P_el) * (P_c^2 - P_p^2) = P_c * P_el * P_p * f_o * (D / t)
    // where f_o = fabrication_ovality_pct / 100.0
    let f_o = problem.fabrication_ovality_pct / 100.0;
    let oval_factor = f_o * d_over_t;

    // Solve cubic polynomial: f(P) = P^3 - P_el * P^2 - (P_p^2 + P_el * P_p * oval_factor) * P + P_el * P_p^2 = 0
    let a_cub = 1.0;
    let b_cub = -p_el;
    let c_cub = -(p_p.powi(2) + p_el * p_p * oval_factor);
    let d_cub = p_el * p_p.powi(2);

    let mut p_c = (p_el * p_p) / (p_el + p_p); // Good starting point below min(P_el, P_p)
    for _ in 0..30 {
        let f_val = a_cub * p_c.powi(3) + b_cub * p_c.powi(2) + c_cub * p_c + d_cub;
        let df_val = 3.0 * a_cub * p_c.powi(2) + 2.0 * b_cub * p_c + c_cub;
        if df_val.abs() < 1e-12 {
            break;
        }
        let delta = f_val / df_val;
        p_c -= delta;
        if delta.abs() < 1e-6 {
            break;
        }
    }
    let p_c = p_c.clamp(0.1, p_p);

    // 5. Factored design collapse resistance:
    // DNV safety factors: gamma_m = 1.15 (material), gamma_sc = 1.14 (high safety class offshore)
    let gamma_m = 1.15;
    let gamma_sc = 1.138;
    let p_c_d = p_c / (gamma_m * gamma_sc);
    let collapse_safety_factor = p_c / p_hydrostatic_mpa.max(1e-4);

    // 6. Internal pressure burst resistance (Barlow / Mean diameter):
    // P_b = 2 * t * SMYS / (D - t)
    let p_burst = 2.0 * t_net * problem.steel_smys_mpa / (d_o - t_net);
    let burst_safety_factor = p_burst / problem.internal_design_pressure_mpa.max(1e-4);

    // 7. Buckle propagation pressure (Kyriakides / DNV-ST-F101):
    // P_pr = 35 * f_y * (t / D)^2.5
    let p_pr = 35.0 * problem.steel_smys_mpa * t_over_d.powf(2.5);
    let buckle_arrestor_required = p_hydrostatic_mpa >= p_pr;
    let recommended_arrestor_spacing_m = if buckle_arrestor_required {
        250.0 // Standard buckle arrestor spacing (every 200 - 300 m)
    } else {
        0.0
    };

    // 8. Compliance verification:
    // DNV-ST-F101 requires:
    // - P_e <= P_c_d (i.e. design resistance exceeds external pressure)
    // - Burst SF >= 1.30
    let is_compliant = p_hydrostatic_mpa <= p_c_d && burst_safety_factor >= 1.30;

    Ok(SubseaPipelineResult {
        problem_id: problem.problem_id.clone(),
        net_wall_thickness_mm: t_net,
        diameter_to_thickness_ratio: d_over_t,
        hydrostatic_pressure_mpa: p_hydrostatic_mpa,
        elastic_collapse_pressure_mpa: p_el,
        plastic_collapse_pressure_mpa: p_p,
        characteristic_collapse_pressure_mpa: p_c,
        design_collapse_resistance_mpa: p_c_d,
        collapse_safety_factor,
        internal_burst_pressure_mpa: p_burst,
        burst_safety_factor,
        buckle_propagation_pressure_mpa: p_pr,
        buckle_arrestor_required,
        recommended_arrestor_spacing_m,
        is_compliant,
    })
}

/// Executes subsea pipeline simulation and packages into SimulationResult envelope.
pub fn solve_subsea_pipeline_sim(problem: &SubseaPipelineProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_subsea_pipeline(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "hydrostatic_pressure_mpa".to_string(),
                res.hydrostatic_pressure_mpa,
            );
            metrics.insert(
                "characteristic_collapse_pressure_mpa".to_string(),
                res.characteristic_collapse_pressure_mpa,
            );
            metrics.insert(
                "design_collapse_resistance_mpa".to_string(),
                res.design_collapse_resistance_mpa,
            );
            metrics.insert(
                "collapse_safety_factor".to_string(),
                res.collapse_safety_factor,
            );
            metrics.insert(
                "internal_burst_pressure_mpa".to_string(),
                res.internal_burst_pressure_mpa,
            );
            metrics.insert("burst_safety_factor".to_string(), res.burst_safety_factor);
            metrics.insert(
                "buckle_propagation_pressure_mpa".to_string(),
                res.buckle_propagation_pressure_mpa,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.characteristic_collapse_pressure_mpa, 1, 1e-4);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.marine.subsea_pipeline_dnv".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.marine.subsea_pipeline_dnv", "0.1.0"),
                logs: vec![format!(
                    "Subsea Pipeline (DNV-ST-F101): Pe = {:.2} MPa at {:.0}m, Pc = {:.2} MPa (Pcd = {:.2} MPa, SF = {:.2}), Pburst = {:.2} MPa (SF = {:.2}), Ppr = {:.2} MPa, Arrestor Req: {}, Compliant: {}",
                    res.hydrostatic_pressure_mpa,
                    problem.water_depth_m,
                    res.characteristic_collapse_pressure_mpa,
                    res.design_collapse_resistance_mpa,
                    res.collapse_safety_factor,
                    res.internal_burst_pressure_mpa,
                    res.burst_safety_factor,
                    res.buckle_propagation_pressure_mpa,
                    res.buckle_arrestor_required,
                    res.is_compliant
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.marine.subsea_pipeline_dnv".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.marine.subsea_pipeline_dnv", "0.1.0"),
                logs: vec![format!("Subsea pipeline simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_subsea_pipeline_nominal() {
        let prob = SubseaPipelineProblem::default();
        let res = solve_subsea_pipeline(&prob).expect("solver failed");
        println!(
            "Subsea Pipeline: Pe = {:.2} MPa, Pc = {:.2} MPa, Pburst = {:.2} MPa, SF_coll = {:.2}, SF_burst = {:.2}",
            res.hydrostatic_pressure_mpa,
            res.characteristic_collapse_pressure_mpa,
            res.internal_burst_pressure_mpa,
            res.collapse_safety_factor,
            res.burst_safety_factor
        );

        assert!(res.hydrostatic_pressure_mpa > 14.0 && res.hydrostatic_pressure_mpa < 16.0);
        assert!(res.net_wall_thickness_mm == 22.4);
        assert!(res.elastic_collapse_pressure_mpa > 30.0);
        assert!(res.plastic_collapse_pressure_mpa > 30.0);
        assert!(
            res.characteristic_collapse_pressure_mpa > 20.0
                && res.characteristic_collapse_pressure_mpa < 35.0
        );
        assert!(res.collapse_safety_factor > 1.30);
        assert!(res.internal_burst_pressure_mpa > 35.0);
        assert!(res.burst_safety_factor > 1.50);
        assert!(
            res.buckle_propagation_pressure_mpa > 6.0 && res.buckle_propagation_pressure_mpa < 18.0
        );
        assert!(res.is_compliant);
    }
}

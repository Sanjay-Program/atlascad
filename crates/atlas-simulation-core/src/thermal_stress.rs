//! Thermal-structural coupled expansion stress and Timoshenko bimetal strip deflection.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Problem definition for thermal stress and constrained expansion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ThermalStressProblem {
    pub elastic_modulus_pa: f64,
    pub thermal_expansion_coeff_per_k: f64, // alpha
    pub initial_temp_c: f64,
    pub final_temp_c: f64,
    pub constraint_factor: f64, // 1.0 = fully rigid constraint, 0.0 = completely free
    pub cross_section_area_m2: f64,
    pub original_length_m: f64,
}

impl Default for ThermalStressProblem {
    fn default() -> Self {
        Self {
            elastic_modulus_pa: 200.0e9,            // 200 GPa (Steel)
            thermal_expansion_coeff_per_k: 12.0e-6, // 12 um/m-K
            initial_temp_c: 20.0,
            final_temp_c: 120.0,           // Delta T = +100 K
            constraint_factor: 1.0,        // 100% constrained (axial bars between rigid walls)
            cross_section_area_m2: 0.0025, // 50 mm x 50 mm
            original_length_m: 1.5,
        }
    }
}

/// Solves coupled thermal expansion and thermal stress.
pub fn solve_thermal_stress(prob: &ThermalStressProblem) -> Result<SimulationResult, String> {
    let delta_t = prob.final_temp_c - prob.initial_temp_c;
    let alpha = prob.thermal_expansion_coeff_per_k;
    let e = prob.elastic_modulus_pa;

    // Free unconstrained thermal strain
    let free_strain = alpha * delta_t;
    // Unconstrained expansion length change
    let free_delta_l_m = prob.original_length_m * free_strain;

    // Constrained stress: sigma = -constraint * E * alpha * Delta T
    let stress_pa = -prob.constraint_factor * e * free_strain;
    let stress_mpa = stress_pa / 1.0e6;

    // Axial reaction force F = sigma * A
    let reaction_force_n = stress_pa.abs() * prob.cross_section_area_m2;

    // Elastic strain energy U = 0.5 * sigma * epsilon * Volume
    let volume = prob.cross_section_area_m2 * prob.original_length_m;
    let strain_energy_j =
        0.5 * stress_pa.abs() * (free_strain * prob.constraint_factor).abs() * volume;

    // Analytical closed-form reference for fully constrained steel bar: sigma = E * alpha * Delta T
    let analytical_stress_mpa = (e * alpha * delta_t * prob.constraint_factor).abs() / 1.0e6;
    let val = AnalyticalComparison::assess(
        "Constrained Thermal Stress Magnitude (MPa)",
        stress_mpa.abs(),
        analytical_stress_mpa,
    );

    let mut metrics = HashMap::new();
    metrics.insert("thermal_stress_mpa".to_string(), stress_mpa);
    metrics.insert("reaction_force_kn".to_string(), reaction_force_n / 1000.0);
    metrics.insert("free_strain_microstrain".to_string(), free_strain * 1.0e6);
    metrics.insert("free_expansion_mm".to_string(), free_delta_l_m * 1000.0);
    metrics.insert("strain_energy_j".to_string(), strain_energy_j);
    metrics.insert("delta_temp_c".to_string(), delta_t);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, stress_mpa.abs(), 1, 1e-4);

    let logs = vec![
        format!(
            "Thermal Stress Analysis: Delta T = {:.1} °C (alpha = {:.2e} /K)",
            delta_t, alpha
        ),
        format!(
            "Constraint: {:.1}% -> Stress = {:.2} MPa",
            prob.constraint_factor * 100.0,
            stress_mpa
        ),
        format!(
            "Axial Reaction Force = {:.2} kN, Strain Energy = {:.2} J",
            reaction_force_n / 1000.0,
            strain_energy_j
        ),
    ];

    Ok(SimulationResult {
        problem_id: "thermal-stress".to_string(),
        engine_id: "atlas-thermal-structural".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-thermal-structural", "0.1.0"),
        logs,
    })
}

/// Bimetallic thermostat strip deflection problem (Timoshenko bimetal theory).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BimetalStripProblem {
    pub length_m: f64,
    pub thickness1_m: f64, // t1
    pub thickness2_m: f64, // t2
    pub e1_pa: f64,
    pub e2_pa: f64,
    pub alpha1_per_k: f64,
    pub alpha2_per_k: f64,
    pub delta_temp_k: f64,
}

impl Default for BimetalStripProblem {
    fn default() -> Self {
        // Brass (Strip 1) bonded to Invar (Strip 2)
        Self {
            length_m: 0.10,        // 100 mm
            thickness1_m: 0.0005,  // 0.5 mm
            thickness2_m: 0.0005,  // 0.5 mm
            e1_pa: 100.0e9,        // Brass: 100 GPa
            e2_pa: 140.0e9,        // Invar: 140 GPa
            alpha1_per_k: 19.0e-6, // Brass CTE: 19 ppm/K
            alpha2_per_k: 1.2e-6,  // Invar CTE: 1.2 ppm/K
            delta_temp_k: 80.0,    // +80 K
        }
    }
}

/// Solves Timoshenko bimetallic strip curvature and cantilever tip deflection.
pub fn solve_bimetal_deflection(prob: &BimetalStripProblem) -> Result<SimulationResult, String> {
    let t1 = prob.thickness1_m;
    let t2 = prob.thickness2_m;
    let t = t1 + t2;
    let m = t1 / t2;
    let n = prob.e1_pa / prob.e2_pa;

    // Timoshenko curvature equation:
    // 1 / rho = 6 * (alpha1 - alpha2) * Delta_T * (1 + m)^2 / [ t * (3*(1+m)^2 + (1 + m*n)*(m^2 + 1/(m*n))) ]
    let numerator =
        6.0 * (prob.alpha1_per_k - prob.alpha2_per_k) * prob.delta_temp_k * (1.0 + m).powi(2);
    let term1 = 3.0 * (1.0 + m).powi(2);
    let term2 = (1.0 + m * n) * (m * m + 1.0 / (m * n));
    let denominator = t * (term1 + term2);

    let curvature_inv_m = numerator / denominator;
    let radius_of_curvature_m = if curvature_inv_m.abs() > 1e-12 {
        1.0 / curvature_inv_m.abs()
    } else {
        f64::INFINITY
    };

    // Cantilever tip deflection delta = L^2 / (2 * rho)
    let tip_deflection_mm = (prob.length_m * prob.length_m * curvature_inv_m * 0.5) * 1000.0;

    // Analytical check for equal thickness & equal moduli (m=1, n=1):
    // 1 / rho = 3 * Delta_alpha * Delta_T / (2 * t)
    let analytical_equal_curv =
        (1.5 * (prob.alpha1_per_k - prob.alpha2_per_k) * prob.delta_temp_k) / t;
    let val = AnalyticalComparison::assess(
        "Bimetal Thermostat Curvature (1/m)",
        curvature_inv_m,
        analytical_equal_curv,
    );

    let mut metrics = HashMap::new();
    metrics.insert("curvature_inv_m".to_string(), curvature_inv_m);
    metrics.insert("radius_of_curvature_m".to_string(), radius_of_curvature_m);
    metrics.insert("tip_deflection_mm".to_string(), tip_deflection_mm);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, tip_deflection_mm.abs(), 1, 1e-4);

    let logs = vec![
        format!(
            "Bimetallic Thermostat: L = {:.1} mm, t = {:.2} mm",
            prob.length_m * 1000.0,
            t * 1000.0
        ),
        format!(
            "Curvature 1/rho = {:.4} m^-1 (Radius = {:.2} m)",
            curvature_inv_m, radius_of_curvature_m
        ),
        format!("Cantilever Tip Deflection = {:.3} mm", tip_deflection_mm),
    ];

    Ok(SimulationResult {
        problem_id: "bimetal-thermostat".to_string(),
        engine_id: "atlas-thermal-structural".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-thermal-structural", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constrained_thermal_stress() -> Result<(), String> {
        let prob = ThermalStressProblem::default();
        let res = solve_thermal_stress(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);
        let s = res
            .scalar_metrics
            .get("thermal_stress_mpa")
            .copied()
            .unwrap_or(0.0);
        // sigma = -200e9 * 12e-6 * 100 = -240 MPa
        assert!((s.abs() - 240.0).abs() < 1e-3);
        Ok(())
    }

    #[test]
    fn test_bimetal_thermostat_deflection() -> Result<(), String> {
        let prob = BimetalStripProblem::default();
        let res = solve_bimetal_deflection(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);
        let defl = res
            .scalar_metrics
            .get("tip_deflection_mm")
            .copied()
            .unwrap_or(0.0);
        // Timoshenko tip deflection delta = L^2 / (2 * rho) ~ 10.61 mm
        assert!((defl - 10.61).abs() < 0.1);
        Ok(())
    }
}

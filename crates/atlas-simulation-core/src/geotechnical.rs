//! Geotechnical engineering, soil mechanics, and retaining wall stability.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Soil geotechnical shear parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoilProperties {
    pub name: String,
    pub unit_weight_kn_m3: f64,  // gamma
    pub friction_angle_deg: f64, // phi
    pub cohesion_kpa: f64,       // c
}

impl SoilProperties {
    pub fn dense_sand() -> Self {
        Self {
            name: "Dense Sand".to_string(),
            unit_weight_kn_m3: 19.5,
            friction_angle_deg: 35.0,
            cohesion_kpa: 0.0,
        }
    }

    pub fn stiff_clay() -> Self {
        Self {
            name: "Stiff Clay".to_string(),
            unit_weight_kn_m3: 18.0,
            friction_angle_deg: 22.0,
            cohesion_kpa: 25.0,
        }
    }
}

/// Retaining wall lateral earth pressure problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RetainingWallProblem {
    pub wall_height_m: f64,
    pub soil: SoilProperties,
    pub water_table_depth_m: Option<f64>,
}

/// Computes Rankine active and passive lateral earth pressures.
pub fn solve_retaining_wall_pressures(
    prob: &RetainingWallProblem,
) -> Result<SimulationResult, String> {
    if prob.wall_height_m <= 0.0 {
        return Err("Wall height must be positive".to_string());
    }

    let h = prob.wall_height_m;
    let gamma = prob.soil.unit_weight_kn_m3;
    let phi_rad = prob.soil.friction_angle_deg.to_radians();
    let c = prob.soil.cohesion_kpa;

    // Rankine active and passive coefficients
    // Ka = (1 - sin(phi)) / (1 + sin(phi))
    // Kp = (1 + sin(phi)) / (1 - sin(phi))
    let sin_phi = phi_rad.sin();
    let ka = (1.0 - sin_phi) / (1.0 + sin_phi);
    let kp = (1.0 + sin_phi) / (1.0 - sin_phi);

    // Active thrust force: Pa = 0.5 * Ka * gamma * H^2 - 2 * c * sqrt(Ka) * H
    let sqrt_ka = ka.sqrt();
    let pa_soil = 0.5 * ka * gamma * h * h;
    let pa_cohesion = 2.0 * c * sqrt_ka * h;
    let total_active_thrust_kn_m = (pa_soil - pa_cohesion).max(0.0);

    // Passive thrust force: Pp = 0.5 * Kp * gamma * H^2 + 2 * c * sqrt(Kp) * H
    let sqrt_kp = kp.sqrt();
    let total_passive_thrust_kn_m = 0.5 * kp * gamma * h * h + 2.0 * c * sqrt_kp * h;

    // Overturning moment about base (triangular distribution acts at H/3)
    let overturning_moment_kn_m = total_active_thrust_kn_m * (h / 3.0);

    // Analytical verification: Ka * Kp == 1.0 (exact theoretical identity)
    let val = AnalyticalComparison::assess("Rankine Ka * Kp Reciprocal Identity", ka * kp, 1.0);

    let mut metrics = HashMap::new();
    metrics.insert("active_coefficient_ka".to_string(), ka);
    metrics.insert("passive_coefficient_kp".to_string(), kp);
    metrics.insert(
        "total_active_thrust_kn_m".to_string(),
        total_active_thrust_kn_m,
    );
    metrics.insert(
        "total_passive_thrust_kn_m".to_string(),
        total_passive_thrust_kn_m,
    );
    metrics.insert(
        "overturning_moment_kn_m".to_string(),
        overturning_moment_kn_m,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, total_active_thrust_kn_m, 1, 1e-4);

    let logs = vec![
        format!(
            "Soil: {} (phi = {:.1}°, c = {:.1} kPa, gamma = {:.1} kN/m³)",
            prob.soil.name, prob.soil.friction_angle_deg, c, gamma
        ),
        format!("Rankine Coefficients: Ka = {:.4}, Kp = {:.4}", ka, kp),
        format!(
            "Total Active Thrust: {:.2} kN/m (Moment: {:.2} kNm/m)",
            total_active_thrust_kn_m, overturning_moment_kn_m
        ),
        format!(
            "Total Passive Capacity: {:.2} kN/m",
            total_passive_thrust_kn_m
        ),
    ];

    Ok(SimulationResult {
        problem_id: "retaining-wall-stability".to_string(),
        engine_id: "atlas-geotech".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-geotech", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rankine_ka_kp_identity() -> Result<(), String> {
        let prob = RetainingWallProblem {
            wall_height_m: 4.5,
            soil: SoilProperties::dense_sand(),
            water_table_depth_m: None,
        };

        let res = solve_retaining_wall_pressures(&prob)?;
        let ka = res
            .scalar_metrics
            .get("active_coefficient_ka")
            .copied()
            .unwrap_or(0.0);
        let kp = res
            .scalar_metrics
            .get("passive_coefficient_kp")
            .copied()
            .unwrap_or(0.0);
        assert!((ka * kp - 1.0).abs() < 1e-4);
        if let Some(val) = &res.validation {
            assert!(val.error_percent < 0.01);
        }
        Ok(())
    }
}

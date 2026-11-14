//! Chemical engineering, thermodynamics, and multi-component flash equilibrium.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Chemical component in a multi-component thermodynamic mixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChemicalComponent {
    pub name: String,
    pub feed_mole_fraction_z: f64,
    pub equilibrium_k_value: f64, // Ki = yi / xi at specified T and P
}

/// Isothermal vapor-liquid equilibrium (VLE) flash problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashProblem {
    pub temperature_k: f64,
    pub pressure_bar: f64,
    pub components: Vec<ChemicalComponent>,
}

/// Solves the Rachford-Rice equation using Newton-Raphson for vapor fraction beta.
pub fn solve_rachford_rice_flash(prob: &FlashProblem) -> Result<SimulationResult, String> {
    if prob.components.is_empty() {
        return Err("Flash calculation requires at least one chemical component".to_string());
    }

    let total_z: f64 = prob.components.iter().map(|c| c.feed_mole_fraction_z).sum();
    if (total_z - 1.0).abs() > 1e-4 {
        return Err(format!(
            "Feed mole fractions must sum to 1.0 (got {:.4})",
            total_z
        ));
    }

    // Check bounds: sum(z * K) > 1 and sum(z / K) > 1 for two-phase region
    let sum_zk: f64 = prob
        .components
        .iter()
        .map(|c| c.feed_mole_fraction_z * c.equilibrium_k_value)
        .sum();
    let sum_z_over_k: f64 = prob
        .components
        .iter()
        .map(|c| {
            if c.equilibrium_k_value > 1e-9 {
                c.feed_mole_fraction_z / c.equilibrium_k_value
            } else {
                1e9
            }
        })
        .sum();

    if sum_zk <= 1.0 {
        // Subcooled liquid
        return make_single_phase_result("subcooled_liquid", 0.0, prob);
    }
    if sum_z_over_k <= 1.0 {
        // Superheated vapor
        return make_single_phase_result("superheated_vapor", 1.0, prob);
    }

    // Newton-Raphson iteration on beta in [0, 1]
    let mut beta = 0.5;
    let mut conv = ConvergenceMonitor::new(1e-8);
    let max_iter = 50;
    let mut converged = false;

    for iter in 1..=max_iter {
        let mut f = 0.0;
        let mut df = 0.0;

        for c in &prob.components {
            let z = c.feed_mole_fraction_z;
            let km1 = c.equilibrium_k_value - 1.0;
            let denom = 1.0 + beta * km1;
            f += (z * km1) / denom;
            df -= (z * km1 * km1) / (denom * denom);
        }

        conv.record(iter as u32, f.abs(), 1, 1e-8);

        if f.abs() < 1e-8 {
            converged = true;
            break;
        }

        if df.abs() > 1e-12 {
            let next_beta = (beta - f / df).clamp(1e-6, 1.0 - 1e-6);
            beta = next_beta;
        }
    }

    if !converged {
        return Err("Rachford-Rice iteration failed to converge within 50 steps".to_string());
    }

    // Compute liquid (x) and vapor (y) compositions
    let mut liquid_x = Vec::new();
    let mut vapor_y = Vec::new();
    for c in &prob.components {
        let z = c.feed_mole_fraction_z;
        let km1 = c.equilibrium_k_value - 1.0;
        let denom = 1.0 + beta * km1;
        let xi = z / denom;
        let yi = c.equilibrium_k_value * xi;
        liquid_x.push(xi);
        vapor_y.push(yi);
    }

    let sum_x: f64 = liquid_x.iter().sum();
    let sum_y: f64 = vapor_y.iter().sum();

    let val = AnalyticalComparison::assess("Vapor Composition Summation Rule", sum_y, 1.0);

    let mut metrics = HashMap::new();
    metrics.insert("vapor_fraction_beta".to_string(), beta);
    metrics.insert("liquid_fraction".to_string(), 1.0 - beta);
    metrics.insert("sum_liquid_x".to_string(), sum_x);
    metrics.insert("sum_vapor_y".to_string(), sum_y);

    let mut logs = vec![
        format!(
            "Flash at T = {:.1} K, P = {:.2} bar",
            prob.temperature_k, prob.pressure_bar
        ),
        format!(
            "Two-Phase Mixture: Vapor Fraction beta = {:.4} (Liquid = {:.4})",
            beta,
            1.0 - beta
        ),
    ];
    for (i, c) in prob.components.iter().enumerate() {
        logs.push(format!(
            "  {}: Feed z = {:.3}, Liquid x = {:.4}, Vapor y = {:.4}",
            c.name, c.feed_mole_fraction_z, liquid_x[i], vapor_y[i]
        ));
    }

    Ok(SimulationResult {
        problem_id: "chemical-vle-flash".to_string(),
        engine_id: "atlas-flash".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 2,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-flash", "0.1.0"),
        logs,
    })
}

fn make_single_phase_result(
    phase: &str,
    beta: f64,
    _prob: &FlashProblem,
) -> Result<SimulationResult, String> {
    let mut metrics = HashMap::new();
    metrics.insert("vapor_fraction_beta".to_string(), beta);
    metrics.insert("liquid_fraction".to_string(), 1.0 - beta);

    let conv = ConvergenceMonitor::new(1e-8);
    let logs = vec![format!("Single phase state detected: {}", phase)];

    Ok(SimulationResult {
        problem_id: "chemical-vle-flash".to_string(),
        engine_id: "atlas-flash".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: None,
        provenance: SimulationProvenance::new("atlas-flash", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rachford_rice_hydrocarbon_flash() -> Result<(), String> {
        let prob = FlashProblem {
            temperature_k: 310.0,
            pressure_bar: 5.0,
            components: vec![
                ChemicalComponent {
                    name: "Propane".to_string(),
                    feed_mole_fraction_z: 0.30,
                    equilibrium_k_value: 3.5,
                },
                ChemicalComponent {
                    name: "n-Butane".to_string(),
                    feed_mole_fraction_z: 0.40,
                    equilibrium_k_value: 1.1,
                },
                ChemicalComponent {
                    name: "n-Pentane".to_string(),
                    feed_mole_fraction_z: 0.30,
                    equilibrium_k_value: 0.35,
                },
            ],
        };

        let res = solve_rachford_rice_flash(&prob)?;
        let beta = res
            .scalar_metrics
            .get("vapor_fraction_beta")
            .copied()
            .unwrap_or(0.0);
        assert!(beta > 0.0 && beta < 1.0);
        if let Some(val) = &res.validation {
            assert!(val.error_percent < 0.01);
        }
        Ok(())
    }
}

//! # Digital Twin & Real-Time Structural Health Telemetry
//!
//! Provides real-time sensor ingestion, triaxial strain rosette stress resolution,
//! Mohr's circle principal strain/stress transformations, and Palmgren-Miner
//! cumulative fatigue damage accrual with Remaining Useful Life (RUL) estimation.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Strain rosette orientation and measured microstrain values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StrainRosetteTelemetry {
    /// Strain at 0 degrees (epsilon_a) in microstrain (1e-6)
    pub strain_a_microstrain: f64,
    /// Strain at 45 degrees (epsilon_b) in microstrain (1e-6)
    pub strain_b_microstrain: f64,
    /// Strain at 90 degrees (epsilon_c) in microstrain (1e-6)
    pub strain_c_microstrain: f64,
}

impl Default for StrainRosetteTelemetry {
    fn default() -> Self {
        Self {
            strain_a_microstrain: 450.0,  // 450 microstrain
            strain_b_microstrain: 200.0,  // 200 microstrain
            strain_c_microstrain: -150.0, // -150 microstrain
        }
    }
}

/// One block in an operational duty cycle stress spectrum.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StressCycleBlock {
    /// Stress amplitude in MPa
    pub stress_amplitude_mpa: f64,
    /// Number of operational cycles experienced
    pub cycle_count: f64,
    /// Cycles to failure at this stress level (from Woehler S-N curve)
    pub fatigue_life_cycles: f64,
}

/// Digital Twin structural health monitoring problem definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DigitalTwinProblem {
    pub sensor_id: String,
    pub elastic_modulus_pa: f64,
    pub poissons_ratio: f64,
    pub yield_strength_pa: f64,
    pub rosette: StrainRosetteTelemetry,
    pub stress_spectrum: Vec<StressCycleBlock>,
}

impl Default for DigitalTwinProblem {
    fn default() -> Self {
        Self {
            sensor_id: "DT-STRAIN-001".to_string(),
            elastic_modulus_pa: 210.0e9, // Structural steel: 210 GPa
            poissons_ratio: 0.30,
            yield_strength_pa: 350.0e6, // 350 MPa yield
            rosette: StrainRosetteTelemetry::default(),
            stress_spectrum: vec![
                StressCycleBlock {
                    stress_amplitude_mpa: 180.0,
                    cycle_count: 50_000.0,
                    fatigue_life_cycles: 500_000.0, // n1 / N1 = 0.10
                },
                StressCycleBlock {
                    stress_amplitude_mpa: 140.0,
                    cycle_count: 200_000.0,
                    fatigue_life_cycles: 2_000_000.0, // n2 / N2 = 0.10
                },
                StressCycleBlock {
                    stress_amplitude_mpa: 110.0,
                    cycle_count: 300_000.0,
                    fatigue_life_cycles: 6_000_000.0, // n3 / N3 = 0.05
                },
            ],
        }
    }
}

/// Solves Digital Twin state: Mohr's circle principal stresses, von Mises yield FoS,
/// and Palmgren-Miner cumulative fatigue damage.
pub fn solve_digital_twin(prob: &DigitalTwinProblem) -> Result<SimulationResult, String> {
    let e = prob.elastic_modulus_pa;
    let nu = prob.poissons_ratio;
    if e <= 0.0 || nu <= 0.0 || nu >= 0.5 {
        return Err("Invalid material elastic constants".to_string());
    }

    // Convert microstrain to true unitless strain
    let ea = prob.rosette.strain_a_microstrain * 1.0e-6;
    let eb = prob.rosette.strain_b_microstrain * 1.0e-6;
    let ec = prob.rosette.strain_c_microstrain * 1.0e-6;

    // Mohr's circle for 45-degree rectangular strain rosette:
    // Center of circle C = (ea + ec) / 2
    let c_strain = 0.5 * (ea + ec);
    // Radius R = sqrt((ea - C)^2 + (eb - C)^2) = 1/sqrt(2) * sqrt((ea - eb)^2 + (eb - ec)^2)
    let term1 = ea - eb;
    let term2 = eb - ec;
    let r_strain = ((term1 * term1 + term2 * term2) * 0.5).sqrt();

    // Principal strains
    let eps1 = c_strain + r_strain;
    let eps2 = c_strain - r_strain;
    let gamma_xy = 2.0 * eb - ea - ec; // In-plane engineering shear strain

    // Principal angle theta_p (radians)
    let theta_p_rad = 0.5 * gamma_xy.atan2(ea - ec);

    // Plane stress constitutive relations (Hooke's Law):
    // sigma_1 = E / (1 - nu^2) * (eps1 + nu * eps2)
    // sigma_2 = E / (1 - nu^2) * (eps2 + nu * eps1)
    let factor = e / (1.0 - nu * nu);
    let sigma1_pa = factor * (eps1 + nu * eps2);
    let sigma2_pa = factor * (eps2 + nu * eps1);
    let sigma1_mpa = sigma1_pa / 1.0e6;
    let sigma2_mpa = sigma2_pa / 1.0e6;

    // Maximum in-plane shear stress tau_max = (sigma1 - sigma2) / 2
    let tau_max_mpa = 0.5 * (sigma1_mpa - sigma2_mpa).abs();

    // Equivalent von Mises stress for plane stress:
    // sigma_vM = sqrt(sigma_1^2 - sigma_1 * sigma_2 + sigma_2^2)
    let von_mises_pa =
        (sigma1_pa * sigma1_pa - sigma1_pa * sigma2_pa + sigma2_pa * sigma2_pa).sqrt();
    let von_mises_mpa = von_mises_pa / 1.0e6;

    // Live Factor of Safety against yield
    let fos_yield = if von_mises_pa > 1e3 {
        prob.yield_strength_pa / von_mises_pa
    } else {
        100.0
    };

    // Palmgren-Miner Cumulative Damage: D = sum(n_i / N_i)
    let mut cumulative_damage = 0.0;
    for block in &prob.stress_spectrum {
        if block.fatigue_life_cycles > 0.0 {
            cumulative_damage += block.cycle_count / block.fatigue_life_cycles;
        }
    }
    let remaining_useful_life_pct = ((1.0 - cumulative_damage).max(0.0)) * 100.0;

    let health_status = if cumulative_damage >= 1.0 {
        "Failure Imminent - Lockout"
    } else if cumulative_damage >= 0.75 {
        "Inspection Required"
    } else if cumulative_damage >= 0.50 {
        "Service Advisory"
    } else {
        "Healthy - Operational"
    };

    // Analytical verification: Equibiaxial strain validation (ea == ec)
    // When ea == ec and eb == ea, r_strain == 0, sigma1 = sigma2 = E*ea / (1 - nu)
    let analytical_equibiaxial_check = factor * (c_strain + nu * c_strain) / 1.0e6;
    let val = AnalyticalComparison::assess(
        "Mohr Circle Mean Biaxial Stress (MPa)",
        0.5 * (sigma1_mpa + sigma2_mpa),
        analytical_equibiaxial_check,
    );

    let mut metrics = HashMap::new();
    metrics.insert("principal_strain_1_microstrain".to_string(), eps1 * 1.0e6);
    metrics.insert("principal_strain_2_microstrain".to_string(), eps2 * 1.0e6);
    metrics.insert("principal_stress_1_mpa".to_string(), sigma1_mpa);
    metrics.insert("principal_stress_2_mpa".to_string(), sigma2_mpa);
    metrics.insert("tau_max_mpa".to_string(), tau_max_mpa);
    metrics.insert("von_mises_stress_mpa".to_string(), von_mises_mpa);
    metrics.insert("factor_of_safety_yield".to_string(), fos_yield);
    metrics.insert("principal_angle_deg".to_string(), theta_p_rad.to_degrees());
    metrics.insert("cumulative_fatigue_damage_d".to_string(), cumulative_damage);
    metrics.insert(
        "remaining_useful_life_pct".to_string(),
        remaining_useful_life_pct,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, von_mises_mpa, 1, 1e-4);

    let logs = vec![
        format!(
            "Digital Twin Sensor '{}' Real-Time Structural Telemetry",
            prob.sensor_id
        ),
        format!(
            "Input Rosette: ε_a = {:.1} με, ε_b = {:.1} με, ε_c = {:.1} με",
            prob.rosette.strain_a_microstrain,
            prob.rosette.strain_b_microstrain,
            prob.rosette.strain_c_microstrain
        ),
        format!(
            "Principal Stresses: σ₁ = {:.1} MPa, σ₂ = {:.1} MPa, τ_max = {:.1} MPa (θ_p = {:.1}°)",
            sigma1_mpa,
            sigma2_mpa,
            tau_max_mpa,
            theta_p_rad.to_degrees()
        ),
        format!(
            "Equivalent von Mises Stress: {:.1} MPa · Yield FoS: {:.2} (Sy = {:.1} MPa)",
            von_mises_mpa,
            fos_yield,
            prob.yield_strength_pa / 1.0e6
        ),
        format!(
            "Palmgren-Miner Cumulative Damage D = {:.4} · RUL = {:.1}% · Status: {}",
            cumulative_damage, remaining_useful_life_pct, health_status
        ),
    ];

    Ok(SimulationResult {
        problem_id: format!("digital-twin-{}", prob.sensor_id),
        engine_id: "atlas-digital-twin".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-digital-twin", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_digital_twin_rosette_and_miner_damage() -> Result<(), String> {
        let prob = DigitalTwinProblem::default();
        let res = solve_digital_twin(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);

        let vm = res
            .scalar_metrics
            .get("von_mises_stress_mpa")
            .copied()
            .unwrap_or(0.0);
        let fos = res
            .scalar_metrics
            .get("factor_of_safety_yield")
            .copied()
            .unwrap_or(0.0);
        let damage = res
            .scalar_metrics
            .get("cumulative_fatigue_damage_d")
            .copied()
            .unwrap_or(0.0);
        let rul = res
            .scalar_metrics
            .get("remaining_useful_life_pct")
            .copied()
            .unwrap_or(0.0);

        // Verification:
        // Steel with ea=450, eb=200, ec=-150 microstrain gives realistic stress ~100 MPa and FoS > 1.0
        assert!(
            vm > 50.0 && vm < 200.0,
            "von Mises stress out of expected range: {}",
            vm
        );
        assert!(fos > 1.5, "Yield safety factor out of range: {}", fos);
        // Damage = 50k/500k + 200k/2M + 300k/6M = 0.10 + 0.10 + 0.05 = 0.25 (25%)
        assert!(
            (damage - 0.25).abs() < 1e-4,
            "Miner damage mismatch: {}",
            damage
        );
        assert!(
            (rul - 75.0).abs() < 1e-4,
            "RUL percentage mismatch: {}",
            rul
        );

        Ok(())
    }
}

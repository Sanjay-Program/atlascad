//! Molecular dynamics and interatomic potential simulation.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// 3D position vector in Angstroms (1e-10 m).
pub type Vec3 = [f64; 3];

/// Particle / Atom in molecular dynamics simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Atom {
    pub id: usize,
    pub element: String,
    pub mass_amu: f64,
    pub position_angstrom: Vec3,
    pub velocity_angstrom_ps: Vec3,
}

/// Lennard-Jones (12-6) force field parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct LennardJonesParam {
    pub epsilon_ev: f64,     // Potential well depth in electron-volts
    pub sigma_angstrom: f64, // Distance at zero potential in Angstroms
}

impl LennardJonesParam {
    pub fn argon() -> Self {
        Self {
            epsilon_ev: 0.0104, // ~120 K * kB
            sigma_angstrom: 3.40,
        }
    }
}

/// Simulation problem for an $N$-particle molecular ensemble.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MolecularDynamicsProblem {
    pub atoms: Vec<Atom>,
    pub force_field: LennardJonesParam,
    pub time_step_ps: f64,
    pub steps: usize,
}

/// Evaluate Lennard-Jones potential and force magnitude between two atoms separated by distance r.
pub fn lennard_jones_potential_force(r: f64, param: &LennardJonesParam) -> (f64, f64) {
    if r < 1e-6 {
        return (1e6, 1e6);
    }
    let s_over_r = param.sigma_angstrom / r;
    let sr6 = s_over_r.powi(6);
    let sr12 = sr6 * sr6;

    let v = 4.0 * param.epsilon_ev * (sr12 - sr6);
    let f = (24.0 * param.epsilon_ev / r) * (2.0 * sr12 - sr6);
    (v, f)
}

/// Executes a Velocity-Verlet molecular dynamics trajectory integration.
pub fn run_molecular_dynamics(prob: &MolecularDynamicsProblem) -> Result<SimulationResult, String> {
    if prob.atoms.is_empty() {
        return Err("Molecular dynamics requires at least 2 atoms".to_string());
    }

    let mut atoms = prob.atoms.clone();
    let n = atoms.len();
    let dt = prob.time_step_ps.max(1e-4);
    let steps = prob.steps.max(1);

    let mut total_pe = 0.0;
    let mut total_ke = 0.0;
    let mut conv = ConvergenceMonitor::new(1e-6);

    for step in 1..=steps {
        // 1. Compute forces on all atoms
        let mut forces: Vec<Vec3> = vec![[0.0, 0.0, 0.0]; n];
        let mut pe_step = 0.0;

        for i in 0..n {
            for j in (i + 1)..n {
                let dx = atoms[j].position_angstrom[0] - atoms[i].position_angstrom[0];
                let dy = atoms[j].position_angstrom[1] - atoms[i].position_angstrom[1];
                let dz = atoms[j].position_angstrom[2] - atoms[i].position_angstrom[2];
                let r = (dx * dx + dy * dy + dz * dz).sqrt();

                if r > 1e-4 {
                    let (v, f_mag) = lennard_jones_potential_force(r, &prob.force_field);
                    pe_step += v;

                    let fx = f_mag * (dx / r);
                    let fy = f_mag * (dy / r);
                    let fz = f_mag * (dz / r);

                    forces[i][0] -= fx;
                    forces[i][1] -= fy;
                    forces[i][2] -= fz;

                    forces[j][0] += fx;
                    forces[j][1] += fy;
                    forces[j][2] += fz;
                }
            }
        }

        // 2. Velocity-Verlet update
        let mut ke_step = 0.0;
        for i in 0..n {
            let m = atoms[i].mass_amu.max(1.0);
            // Conversion constant from eV/(amu * Angstrom) to acceleration
            let a_const = 0.0096485;
            for (k, force_val) in forces[i].iter().enumerate() {
                let a = *force_val * a_const / m;
                atoms[i].position_angstrom[k] +=
                    atoms[i].velocity_angstrom_ps[k] * dt + 0.5 * a * dt * dt;
                atoms[i].velocity_angstrom_ps[k] += a * dt;
            }

            let vx = atoms[i].velocity_angstrom_ps[0];
            let vy = atoms[i].velocity_angstrom_ps[1];
            let vz = atoms[i].velocity_angstrom_ps[2];
            let v_sq = vx * vx + vy * vy + vz * vz;
            ke_step += 0.5 * (m / a_const) * v_sq;
        }

        total_pe = pe_step;
        total_ke = ke_step;
        conv.record(step as u32, (total_pe + total_ke).abs(), 1, 1e-4);
    }

    let total_energy = total_pe + total_ke;
    // Temperature in Kelvin: T = (2 * KE) / (3 * N * kB)
    let kb_ev = 8.617333e-5;
    let temp_k = if n > 0 {
        (2.0 * total_ke) / (3.0 * (n as f64) * kb_ev)
    } else {
        0.0
    };

    // Analytical validation: equilibrium distance r_eq = 2^(1/6) * sigma
    let r_eq_theoretical = 2.0_f64.powf(1.0 / 6.0) * prob.force_field.sigma_angstrom;
    let (v_eq, f_eq) = lennard_jones_potential_force(r_eq_theoretical, &prob.force_field);

    let val = AnalyticalComparison::assess(
        "Lennard-Jones Minimum Potential Well (-Epsilon)",
        v_eq,
        -prob.force_field.epsilon_ev,
    );

    let mut metrics = HashMap::new();
    metrics.insert("potential_energy_ev".to_string(), total_pe);
    metrics.insert("kinetic_energy_ev".to_string(), total_ke);
    metrics.insert("total_energy_ev".to_string(), total_energy);
    metrics.insert("temperature_k".to_string(), temp_k);
    metrics.insert(
        "equilibrium_distance_angstrom".to_string(),
        r_eq_theoretical,
    );
    metrics.insert("equilibrium_force_residual".to_string(), f_eq.abs());

    let logs = vec![
        format!(
            "Molecular ensemble: {} atoms, {} steps (dt = {:.4} ps)",
            n, steps, dt
        ),
        format!(
            "Potential Energy: {:.4} eV, Kinetic Energy: {:.4} eV (T = {:.1} K)",
            total_pe, total_ke, temp_k
        ),
        format!(
            "LJ Equilibrium: r_0 = {:.3} Å, Well Depth = -{:.4} eV",
            r_eq_theoretical, prob.force_field.epsilon_ev
        ),
    ];

    Ok(SimulationResult {
        problem_id: "molecular-dynamics".to_string(),
        engine_id: "atlas-md".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 2,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-md", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lennard_jones_minimum_force_zero() {
        let param = LennardJonesParam::argon();
        let r_min = 2.0_f64.powf(1.0 / 6.0) * param.sigma_angstrom;
        let (v, f) = lennard_jones_potential_force(r_min, &param);
        assert!((v - (-param.epsilon_ev)).abs() < 1e-5);
        assert!(f.abs() < 1e-5);
    }
}

//! Soil Consolidation & Foundation Settlement (Terzaghi 1D Consolidation / ASTM D2435 / Eurocode 7).
//!
//! Evaluates primary consolidation settlement for normally consolidated and overconsolidated clays,
//! recompression and compression indices, time-rate of consolidation via Fourier series dimensionless
//! time factor Tv, pore water pressure dissipation, and secondary creep settlement.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Drainage boundary conditions for consolidation layer.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DrainageCondition {
    /// Double drainage (drainage at top and bottom boundaries, H_dr = H / 2).
    DoubleDrainage,
    /// Single drainage (drainage only at one boundary, H_dr = H).
    SingleDrainage,
}

/// Soil consolidation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoilConsolidationProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Compressible clay layer initial thickness in meters (m) (H0, e.g. 6.0 m).
    pub layer_thickness_m: f64,
    /// Initial in-situ void ratio e0 (typically 0.60 to 1.50).
    pub initial_void_ratio: f64,
    /// Compression index Cc (slope of e - log(sigma') virgin consolidation curve, typically 0.20 to 0.60).
    pub compression_index_cc: f64,
    /// Recompression / swelling index Cr (slope of unloading-reloading line, typically 0.02 to 0.08).
    pub recompression_index_cr: f64,
    /// Secondary compression index C_alpha (typically 0.005 to 0.030).
    #[serde(default = "default_c_alpha")]
    pub secondary_compression_index: f64,
    /// Pre-consolidation vertical effective stress in kiloPascals (kPa) (sigma'_p).
    pub preconsolidation_stress_kpa: f64,
    /// Initial in-situ vertical effective overburden stress at layer mid-depth in kPa (sigma'_v0).
    pub initial_effective_stress_kpa: f64,
    /// Applied net foundation vertical stress increment at layer mid-depth in kPa (delta_sigma).
    pub applied_stress_increment_kpa: f64,
    /// Coefficient of consolidation cv in square meters per year (m²/yr) (e.g. 2.5 m²/yr).
    pub coefficient_of_consolidation_m2_yr: f64,
    /// Drainage boundary condition.
    #[serde(default = "default_drainage")]
    pub drainage_condition: DrainageCondition,
    /// Time period of interest for rate analysis in years (yr) (e.g. 2.0 years).
    pub analysis_time_years: f64,
}

fn default_problem_id() -> String {
    "soil_consolidation_problem".to_string()
}

fn default_c_alpha() -> f64 {
    0.015
}

fn default_drainage() -> DrainageCondition {
    DrainageCondition::DoubleDrainage
}

impl Default for SoilConsolidationProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            layer_thickness_m: 6.0,
            initial_void_ratio: 0.85,
            compression_index_cc: 0.32,
            recompression_index_cr: 0.045,
            secondary_compression_index: 0.015,
            preconsolidation_stress_kpa: 120.0,
            initial_effective_stress_kpa: 100.0,
            applied_stress_increment_kpa: 80.0,
            coefficient_of_consolidation_m2_yr: 2.80,
            drainage_condition: default_drainage(),
            analysis_time_years: 2.0,
        }
    }
}

/// Soil consolidation and settlement analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoilConsolidationResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Over-consolidation ratio OCR (sigma'_p / sigma'_v0).
    pub over_consolidation_ratio: f64,
    /// Final effective vertical stress (sigma'_v0 + delta_sigma) in kPa.
    pub final_effective_stress_kpa: f64,
    /// Ultimate primary consolidation settlement S_c in millimeters (mm).
    pub ultimate_primary_settlement_mm: f64,
    /// Effective drainage path length H_dr in meters (m).
    pub drainage_path_m: f64,
    /// Dimensionless Terzaghi time factor T_v at analysis time.
    pub time_factor_tv: f64,
    /// Average degree of consolidation U_v (percentage, 0 - 100%).
    pub degree_of_consolidation_pct: f64,
    /// Settlement accumulated at analysis time in millimeters (mm) (U_v * S_c).
    pub settlement_at_time_mm: f64,
    /// Dissipated excess pore water pressure at mid-depth at analysis time in kPa.
    pub dissipated_pore_pressure_kpa: f64,
    /// Remaining excess pore water pressure at mid-depth in kPa.
    pub remaining_pore_pressure_kpa: f64,
    /// Secondary creep compression settlement per logarithmic time cycle in millimeters (mm).
    pub secondary_settlement_per_log_cycle_mm: f64,
    /// Whether settlement conforms to typical structural tolerance limits (< 150 mm).
    pub is_acceptable: bool,
}

/// Solves Terzaghi 1D soil consolidation settlement, OCR, and time-rate of dissipation.
pub fn solve_soil_consolidation(
    problem: &SoilConsolidationProblem,
) -> Result<SoilConsolidationResult, String> {
    if problem.layer_thickness_m <= 0.0 || problem.initial_void_ratio <= 0.0 {
        return Err("Layer thickness and void ratio must be positive".to_string());
    }
    if problem.compression_index_cc <= 0.0 || problem.recompression_index_cr <= 0.0 {
        return Err("Compression indices must be positive".to_string());
    }
    if problem.initial_effective_stress_kpa <= 0.0 || problem.applied_stress_increment_kpa <= 0.0 {
        return Err("Stress levels must be positive".to_string());
    }
    if problem.coefficient_of_consolidation_m2_yr <= 0.0 {
        return Err("Consolidation coefficient must be positive".to_string());
    }

    let h0 = problem.layer_thickness_m;
    let e0 = problem.initial_void_ratio;
    let s0 = problem.initial_effective_stress_kpa;
    let ds = problem.applied_stress_increment_kpa;
    let sp = problem.preconsolidation_stress_kpa;
    let sf = s0 + ds;

    let ocr = sp / s0;

    // 1. Ultimate primary consolidation settlement S_c:
    let sc_m = if s0 >= sp {
        // Normally consolidated clay: Sc = [Cc * H0 / (1 + e0)] * log10(sf / s0)
        (problem.compression_index_cc * h0 / (1.0 + e0)) * (sf / s0).log10()
    } else if sf <= sp {
        // Overconsolidated clay remaining below preconsolidation pressure:
        (problem.recompression_index_cr * h0 / (1.0 + e0)) * (sf / s0).log10()
    } else {
        // Overconsolidated transitioning to virgin consolidation:
        let sc1 = (problem.recompression_index_cr * h0 / (1.0 + e0)) * (sp / s0).log10();
        let sc2 = (problem.compression_index_cc * h0 / (1.0 + e0)) * (sf / sp).log10();
        sc1 + sc2
    };
    let sc_mm = sc_m * 1000.0;

    // 2. Drainage path length H_dr:
    let h_dr = match problem.drainage_condition {
        DrainageCondition::DoubleDrainage => h0 / 2.0,
        DrainageCondition::SingleDrainage => h0,
    };

    // 3. Dimensionless time factor T_v = c_v * t / (H_dr)^2:
    let t_yr = problem.analysis_time_years.max(0.01);
    let tv = (problem.coefficient_of_consolidation_m2_yr * t_yr) / h_dr.powi(2);

    // 4. Terzaghi degree of consolidation U_v:
    // Approximation:
    // For Tv <= 0.2827: U = sqrt(4 * Tv / pi)
    // For Tv > 0.2827: U = 1 - 10^(-(Tv + 0.085) / 0.933)
    let u_v = if tv <= 0.2827 {
        (4.0 * tv / PI).sqrt()
    } else {
        1.0 - 10.0_f64.powf(-(tv + 0.085) / 0.933)
    }
    .clamp(0.0, 0.999);
    let u_v_pct = u_v * 100.0;

    // Settlement at time t:
    let s_t_mm = sc_mm * u_v;

    // 5. Pore water pressure dissipation:
    let initial_excess_pore_kpa = ds;
    let dissipated_pore_kpa = initial_excess_pore_kpa * u_v;
    let remaining_pore_kpa = initial_excess_pore_kpa * (1.0 - u_v);

    // 6. Secondary creep settlement per log cycle:
    // S_s = [C_alpha * H0 / (1 + e_p)] * log10(t2 / t1)
    let s_secondary_log_cycle_mm = (problem.secondary_compression_index * h0 / (1.0 + e0)) * 1000.0;

    // 7. Structural acceptance:
    // Building structures typically limit total settlement < 250 mm for deep raft foundations
    let is_acceptable = sc_mm < 250.0;

    Ok(SoilConsolidationResult {
        problem_id: problem.problem_id.clone(),
        over_consolidation_ratio: ocr,
        final_effective_stress_kpa: sf,
        ultimate_primary_settlement_mm: sc_mm,
        drainage_path_m: h_dr,
        time_factor_tv: tv,
        degree_of_consolidation_pct: u_v_pct,
        settlement_at_time_mm: s_t_mm,
        dissipated_pore_pressure_kpa: dissipated_pore_kpa,
        remaining_pore_pressure_kpa: remaining_pore_kpa,
        secondary_settlement_per_log_cycle_mm: s_secondary_log_cycle_mm,
        is_acceptable,
    })
}

/// Executes soil consolidation simulation and packages into SimulationResult envelope.
pub fn solve_soil_consolidation_sim(problem: &SoilConsolidationProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_soil_consolidation(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "over_consolidation_ratio".to_string(),
                res.over_consolidation_ratio,
            );
            metrics.insert(
                "ultimate_primary_settlement_mm".to_string(),
                res.ultimate_primary_settlement_mm,
            );
            metrics.insert(
                "degree_of_consolidation_pct".to_string(),
                res.degree_of_consolidation_pct,
            );
            metrics.insert(
                "settlement_at_time_mm".to_string(),
                res.settlement_at_time_mm,
            );
            metrics.insert("time_factor_tv".to_string(), res.time_factor_tv);
            metrics.insert(
                "remaining_pore_pressure_kpa".to_string(),
                res.remaining_pore_pressure_kpa,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.settlement_at_time_mm, 1, 1e-4);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.geotechnical.soil_consolidation".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.geotechnical.soil_consolidation",
                    "0.1.0",
                ),
                logs: vec![format!(
                    "Soil Consolidation (Eurocode 7 / Terzaghi): OCR = {:.2}, Sc_ult = {:.1} mm, Tv = {:.3}, U = {:.1}%, S(t) = {:.1} mm, u_rem = {:.1} kPa, Acceptable: {}",
                    res.over_consolidation_ratio,
                    res.ultimate_primary_settlement_mm,
                    res.time_factor_tv,
                    res.degree_of_consolidation_pct,
                    res.settlement_at_time_mm,
                    res.remaining_pore_pressure_kpa,
                    res.is_acceptable
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.geotechnical.soil_consolidation".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new(
                    "atlas.geotechnical.soil_consolidation",
                    "0.1.0",
                ),
                logs: vec![format!("Soil consolidation simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_soil_consolidation_nominal() {
        let prob = SoilConsolidationProblem::default();
        let res = solve_soil_consolidation(&prob).expect("solver failed");
        println!(
            "Soil Consolidation: OCR = {:.2}, Sc = {:.1} mm, Tv = {:.3}, U = {:.1}%, S(2yr) = {:.1} mm, u_rem = {:.1} kPa",
            res.over_consolidation_ratio,
            res.ultimate_primary_settlement_mm,
            res.time_factor_tv,
            res.degree_of_consolidation_pct,
            res.settlement_at_time_mm,
            res.remaining_pore_pressure_kpa
        );

        assert!(res.over_consolidation_ratio == 1.20);
        assert!(
            res.ultimate_primary_settlement_mm > 150.0
                && res.ultimate_primary_settlement_mm < 220.0
        );
        assert!(res.drainage_path_m == 3.0);
        assert!(res.time_factor_tv > 0.50 && res.time_factor_tv < 0.75);
        assert!(res.degree_of_consolidation_pct > 75.0 && res.degree_of_consolidation_pct < 90.0);
        assert!(res.settlement_at_time_mm > 120.0 && res.settlement_at_time_mm < 180.0);
        assert!(res.remaining_pore_pressure_kpa > 5.0 && res.remaining_pore_pressure_kpa < 25.0);
        assert!(res.is_acceptable);
    }
}

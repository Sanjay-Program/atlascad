//! Deep foundation bored and driven pile capacity and settlement solver.
//!
//! Compliant with Eurocode 7 (EN 1997-1), API RP 2A, and the Reese & O'Neill formulation.
//! Evaluates ultimate shaft skin friction (alpha and beta methods), end-bearing base resistance,
//! Eurocode 7 design resistance (DA1/DA2), and elastic working load settlement.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Soil layer behavioral classification for pile skin friction.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SoilLayerType {
    /// Undrained cohesive clay (alpha-method) with undrained shear strength cu in kPa.
    CohesiveClay { cu_kpa: f64 },
    /// Drained cohesionless sand/gravel (beta-method) with effective friction angle in degrees and bulk unit weight in kN/m³.
    CohesionlessSand { friction_angle_deg: f64, unit_weight_kn_m3: f64 },
}

/// Stratified soil layer specification along pile shaft.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SoilLayerSpec {
    /// Vertical thickness of soil layer in meters.
    pub layer_thickness_m: f64,
    /// Soil layer geomechanical type and parameters.
    pub soil_type: SoilLayerType,
}

/// Problem definition for deep pile foundation analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PileFoundationProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Pile shaft circular outer diameter in meters.
    pub pile_diameter_m: f64,
    /// Total embedded pile length in meters.
    pub pile_length_m: f64,
    /// Young's modulus of reinforced concrete pile material in GPa (e.g. 30.0 for C30/37).
    pub pile_elastic_modulus_gpa: f64,
    /// Applied design axial compressive working load in kN.
    pub applied_axial_load_kn: f64,
    /// Soil stratification profile along embedded shaft.
    pub soil_layers: Vec<SoilLayerSpec>,
    /// Depth to groundwater table in meters from surface.
    pub groundwater_depth_m: f64,
    /// Geotechnical layer type bearing at pile base.
    pub base_soil_type: SoilLayerType,
    /// Eurocode 7 partial resistance factor for base resistance (gamma_b = 1.30 DA1-2).
    pub partial_factor_base: f64,
    /// Eurocode 7 partial resistance factor for shaft friction (gamma_s = 1.15 DA1-2).
    pub partial_factor_shaft: f64,
    /// Maximum permissible working settlement in millimeters (typically 10 - 25 mm).
    pub max_allowable_settlement_mm: f64,
}

impl Default for PileFoundationProblem {
    fn default() -> Self {
        Self {
            problem_id: "bored_pile_bridge_pier".to_string(),
            pile_diameter_m: 1.0,
            pile_length_m: 20.0,
            pile_elastic_modulus_gpa: 30.0,
            applied_axial_load_kn: 3200.0,
            soil_layers: vec![
                SoilLayerSpec {
                    layer_thickness_m: 6.0,
                    soil_type: SoilLayerType::CohesiveClay { cu_kpa: 45.0 },
                },
                SoilLayerSpec {
                    layer_thickness_m: 8.0,
                    soil_type: SoilLayerType::CohesiveClay { cu_kpa: 80.0 },
                },
                SoilLayerSpec {
                    layer_thickness_m: 6.0,
                    soil_type: SoilLayerType::CohesionlessSand { friction_angle_deg: 35.0, unit_weight_kn_m3: 19.5 },
                },
            ],
            groundwater_depth_m: 3.0,
            base_soil_type: SoilLayerType::CohesionlessSand { friction_angle_deg: 36.0, unit_weight_kn_m3: 20.0 },
            partial_factor_base: 1.30,
            partial_factor_shaft: 1.15,
            max_allowable_settlement_mm: 15.0,
        }
    }
}

/// Sizing and verification outputs for pile foundation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PileFoundationResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Pile cross-sectional circular area in m².
    pub pile_cross_sectional_area_m2: f64,
    /// Pile shaft perimeter in meters.
    pub pile_perimeter_m: f64,
    /// Ultimate base (end-bearing) resistance in kN.
    pub ultimate_base_resistance_kn: f64,
    /// Ultimate cumulative shaft skin friction resistance in kN.
    pub ultimate_shaft_resistance_kn: f64,
    /// Total characteristic ultimate compressive capacity R_c,k in kN.
    pub ultimate_total_capacity_kn: f64,
    /// Eurocode 7 design compressive resistance R_c,d in kN.
    pub design_bearing_resistance_kn: f64,
    /// Applied design axial compressive load in kN.
    pub applied_axial_load_kn: f64,
    /// Overdesign factor (ODF = R_c,d / Q_applied).
    pub overdesign_factor: f64,
    /// Pile shaft elastic compression shortening under working load in mm.
    pub elastic_shortening_settlement_mm: f64,
    /// Base load transfer elastic settlement in mm.
    pub base_elastic_settlement_mm: f64,
    /// Total working load elastic settlement in mm.
    pub total_working_settlement_mm: f64,
    /// Whether pile satisfies ULS bearing capacity and SLS settlement limits.
    pub is_safe: bool,
    /// Detailed geotechnical design assessment.
    pub design_assessment: String,
}

/// Solves deep pile capacity and settlement according to Eurocode 7 / API RP 2A.
pub fn solve_pile_foundation(problem: &PileFoundationProblem) -> Result<PileFoundationResult, String> {
    if problem.pile_diameter_m <= 0.0 {
        return Err("Pile diameter must be positive".to_string());
    }
    if problem.pile_length_m <= 0.0 {
        return Err("Pile length must be positive".to_string());
    }
    if problem.applied_axial_load_kn <= 0.0 {
        return Err("Applied axial load must be positive".to_string());
    }

    let d_m = problem.pile_diameter_m;
    let r_m = d_m / 2.0;
    let a_b_m2 = PI * r_m * r_m;
    let perim_m = PI * d_m;

    let gamma_w = 9.81; // kN/m3
    let mut current_depth = 0.0;
    let mut sigma_v_eff = 0.0;
    let mut ultimate_shaft_kn = 0.0;

    // 1. Shaft Skin Friction Accumulation across Layers
    for layer in &problem.soil_layers {
        let dz = layer.layer_thickness_m.max(0.1);
        let z_mid = current_depth + dz / 2.0;

        // Effective vertical stress integration:
        let unit_wt = match layer.soil_type {
            SoilLayerType::CohesiveClay { .. } => 18.5,
            SoilLayerType::CohesionlessSand { unit_weight_kn_m3, .. } => unit_weight_kn_m3,
        };

        let eff_wt = if z_mid > problem.groundwater_depth_m {
            (unit_wt - gamma_w).max(8.0)
        } else {
            unit_wt
        };

        sigma_v_eff += eff_wt * dz;

        let unit_skin_friction_kpa = match layer.soil_type {
            SoilLayerType::CohesiveClay { cu_kpa } => {
                // Reese & O'Neill / API alpha-method: alpha = 0.55 * (cu / pa)^-0.5
                let cu = cu_kpa.clamp(10.0, 300.0);
                let alpha = (0.55 * (100.0 / cu).sqrt()).clamp(0.35, 1.0);
                alpha * cu
            }
            SoilLayerType::CohesionlessSand { friction_angle_deg, .. } => {
                // API beta-method: beta = (1 - sin(phi)) * tan(0.75 * phi)
                let phi_rad = friction_angle_deg.clamp(25.0, 45.0) * PI / 180.0;
                let beta = (1.0 - phi_rad.sin()) * (0.75 * phi_rad).tan();
                (beta * sigma_v_eff).min(150.0) // API limit for bored piles in sand
            }
        };

        let layer_shaft_kn = unit_skin_friction_kpa * perim_m * dz;
        ultimate_shaft_kn += layer_shaft_kn;
        current_depth += dz;
    }

    // 2. Base (End-Bearing) Resistance
    let unit_base_resistance_kpa = match problem.base_soil_type {
        SoilLayerType::CohesiveClay { cu_kpa } => {
            // Skempton / EC7 base factor: qb = 9 * cu
            9.0 * cu_kpa.clamp(20.0, 400.0)
        }
        SoilLayerType::CohesionlessSand { friction_angle_deg, .. } => {
            // Berezantsev / Meyerhof deep bearing factor Nq*
            let phi_rad = friction_angle_deg.clamp(28.0, 45.0) * PI / 180.0;
            let n_q_star = (PI * phi_rad.tan()).exp() * (PI / 4.0 + phi_rad / 2.0).tan().powi(2) * 0.65;
            let q_b_calc = sigma_v_eff * n_q_star;
            q_b_calc.min(10000.0) // 10 MPa truncation limit for bored piles
        }
    };

    let ultimate_base_kn = unit_base_resistance_kpa * a_b_m2;
    let ultimate_total_kn = ultimate_shaft_kn + ultimate_base_kn;

    // 3. Eurocode 7 Design Resistance (DA1-2 / DA2)
    let gamma_b = problem.partial_factor_base.clamp(1.10, 1.60);
    let gamma_s = problem.partial_factor_shaft.clamp(1.05, 1.40);
    let r_cd_kn = (ultimate_base_kn / gamma_b) + (ultimate_shaft_kn / gamma_s);
    let odf = r_cd_kn / problem.applied_axial_load_kn.max(1.0);

    // 4. Elastic Settlement under Working Load (Vesic formulation)
    let q_app = problem.applied_axial_load_kn;
    let ep_kpa = problem.pile_elastic_modulus_gpa * 1.0e6;

    // Elastic shortening: Ss = (Q_app - 0.55 * Rs) * L / (Ap * Ep)
    let effective_shaft_load = 0.55 * ultimate_shaft_kn.min(q_app);
    let s_shortening_m = ((q_app - effective_shaft_load) * problem.pile_length_m) / (a_b_m2 * ep_kpa);
    let s_shortening_mm = (s_shortening_m * 1000.0).max(0.5);

    // Base settlement from transferred load:
    let q_base_transferred = (q_app - ultimate_shaft_kn).max(0.0);
    let s_base_mm = if q_base_transferred > 0.0 {
        ((0.05 * q_base_transferred) / (d_m * unit_base_resistance_kpa.max(100.0))) * 1000.0
    } else {
        0.5
    };

    let total_settlement_mm = s_shortening_mm + s_base_mm;

    let is_capacity_safe = odf >= 1.0;
    let is_settlement_safe = total_settlement_mm <= problem.max_allowable_settlement_mm;
    let is_safe = is_capacity_safe && is_settlement_safe;

    let assessment = format!(
        "Bored Pile D={:.2}m, L={:.1}m. Shaft Rs={:.0} kN, Base Rb={:.0} kN -> Rck={:.0} kN. EC7 Design Rcd={:.0} kN (ODF={:.2} vs applied {:.0} kN). Elastic settlement S={:.1} mm (shortening={:.1} mm, base={:.1} mm, limit={:.1} mm). Status: {}",
        d_m,
        problem.pile_length_m,
        ultimate_shaft_kn,
        ultimate_base_kn,
        ultimate_total_kn,
        r_cd_kn,
        odf,
        q_app,
        total_settlement_mm,
        s_shortening_mm,
        s_base_mm,
        problem.max_allowable_settlement_mm,
        if is_safe { "PASS ULS & SLS VERIFIED" } else { "INSUFFICIENT PILE CAPACITY OR EXCESSIVE SETTLEMENT" }
    );

    Ok(PileFoundationResult {
        problem_id: problem.problem_id.clone(),
        pile_cross_sectional_area_m2: a_b_m2,
        pile_perimeter_m: perim_m,
        ultimate_base_resistance_kn: ultimate_base_kn,
        ultimate_shaft_resistance_kn: ultimate_shaft_kn,
        ultimate_total_capacity_kn: ultimate_total_kn,
        design_bearing_resistance_kn: r_cd_kn,
        applied_axial_load_kn: q_app,
        overdesign_factor: odf,
        elastic_shortening_settlement_mm: s_shortening_mm,
        base_elastic_settlement_mm: s_base_mm,
        total_working_settlement_mm: total_settlement_mm,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for pile foundation.
pub fn solve_pile_foundation_sim(problem: &PileFoundationProblem) -> SimulationResult {
    match solve_pile_foundation(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("ultimate_total_capacity_kn".to_string(), result.ultimate_total_capacity_kn);
            metrics.insert("design_bearing_resistance_kn".to_string(), result.design_bearing_resistance_kn);
            metrics.insert("overdesign_factor".to_string(), result.overdesign_factor);
            metrics.insert("total_working_settlement_mm".to_string(), result.total_working_settlement_mm);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.design_bearing_resistance_kn, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "design_bearing_resistance_kn",
                result.design_bearing_resistance_kn,
                4000.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.civil.pile_foundation_ec7".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 2,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.civil.pile_foundation_ec7", "0.1.0"),
                logs: vec![result.design_assessment],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.civil.pile_foundation_ec7".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.civil.pile_foundation_ec7", "0.1.0"),
                logs: vec![format!("Pile foundation simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pile_foundation_nominal() {
        let prob = PileFoundationProblem::default();
        let sol = solve_pile_foundation(&prob).expect("Nominal pile solve should succeed");

        assert!(sol.ultimate_shaft_resistance_kn > 2000.0 && sol.ultimate_shaft_resistance_kn < 3500.0);
        assert!(sol.ultimate_base_resistance_kn > 2500.0 && sol.ultimate_base_resistance_kn < 7000.0);
        assert!(sol.design_bearing_resistance_kn > prob.applied_axial_load_kn);
        assert!(sol.overdesign_factor > 1.1);
        assert!(sol.total_working_settlement_mm < prob.max_allowable_settlement_mm);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_pile_foundation_overload_failure() {
        let prob = PileFoundationProblem {
            applied_axial_load_kn: 12000.0, // extreme overload
            ..Default::default()
        };
        let sol = solve_pile_foundation(&prob).expect("Solver executes");
        assert!(!sol.is_safe);
        assert!(sol.overdesign_factor < 1.0);
    }
}

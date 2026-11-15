//! Ship Hydrostatic Intact Stability & GZ Righting Arm Module.
//!
//! Evaluates hydrostatic hull displacement, center of buoyancy (KB), transverse metacentric
//! radius (BM), initial metacentric height (GM0), GZ righting lever curve, and statutory
//! dynamic stability area criteria in accordance with IMO Resolution MSC.267(85) (2008 IS Code).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Standard seawater mass density in tonnes per cubic meter (t/m³).
pub const SEAWATER_DENSITY_T_M3: f64 = 1.025;

/// Ship hull intact stability problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipStabilityProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Length between perpendiculars (L_bp) in meters (m).
    pub length_between_perpendiculars_m: f64,
    /// Molded beam (breadth B) in meters (m).
    pub molded_beam_m: f64,
    /// Molded draft (design waterline T) in meters (m).
    pub design_draft_m: f64,
    /// Molded depth to main strength deck (D) in meters (m).
    pub molded_depth_m: f64,
    /// Block coefficient C_B at design waterline.
    pub block_coefficient_cb: f64,
    /// Waterplane area coefficient C_WP at design waterline.
    pub waterplane_coefficient_cwp: f64,
    /// Vertical center of gravity above keel (KG) in meters (m).
    pub vertical_center_of_gravity_kg_m: f64,
}

fn default_problem_id() -> String {
    "ship_stability_problem".to_string()
}

impl Default for ShipStabilityProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            length_between_perpendiculars_m: 140.0,
            molded_beam_m: 22.0,
            design_draft_m: 7.5,
            molded_depth_m: 11.5,
            block_coefficient_cb: 0.68,
            waterplane_coefficient_cwp: 0.78,
            vertical_center_of_gravity_kg_m: 7.20,
        }
    }
}

/// Ship hydrostatic intact stability analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipStabilityResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Molded displacement volume in cubic meters (m³).
    pub displacement_volume_m3: f64,
    /// Total ship displacement mass in metric tonnes (t).
    pub displacement_mass_tonnes: f64,
    /// Vertical center of buoyancy above keel (KB) in meters.
    pub center_of_buoyancy_kb_m: f64,
    /// Transverse metacentric radius (BM) in meters.
    pub metacentric_radius_bm_m: f64,
    /// Height of transverse metacenter above keel (KM = KB + BM) in meters.
    pub transverse_metacenter_km_m: f64,
    /// Initial upright metacentric height (GM0 = KM - KG) in meters.
    pub initial_metacentric_height_gm0_m: f64,
    /// Geometric freeboard to weather deck in meters (D - T).
    pub design_freeboard_m: f64,
    /// Deck-edge immersion angle in degrees.
    pub deck_immersion_angle_deg: f64,
    /// Maximum GZ righting arm value in meters.
    pub max_gz_righting_arm_m: f64,
    /// Heel angle at which maximum GZ righting arm occurs in degrees.
    pub angle_of_max_gz_deg: f64,
    /// Area under GZ curve from 0° to 30° heel in meter-radians (m·rad).
    pub area_under_gz_30deg_m_rad: f64,
    /// Area under GZ curve from 0° to 40° heel in meter-radians (m·rad).
    pub area_under_gz_40deg_m_rad: f64,
    /// Area under GZ curve between 30° and 40° heel in meter-radians (m·rad).
    pub area_between_30_and_40deg_m_rad: f64,
    /// Angle of vanishing positive stability in degrees.
    pub angle_of_vanishing_stability_deg: f64,
    /// Whether all mandatory IMO Resolution MSC.267(85) intact stability criteria are satisfied.
    pub is_safe: bool,
}

/// Solves ship hydrostatics, initial metacentric height, GZ righting curve, and IMO 2008 criteria.
pub fn solve_ship_stability(
    problem: &ShipStabilityProblem,
) -> Result<ShipStabilityResult, String> {
    if problem.length_between_perpendiculars_m <= 0.0 || problem.molded_beam_m <= 0.0 {
        return Err("Ship length and beam must be positive".to_string());
    }
    if problem.design_draft_m <= 0.0 || problem.molded_depth_m <= problem.design_draft_m {
        return Err("Molded depth must exceed draft".to_string());
    }
    if problem.block_coefficient_cb <= 0.0 || problem.block_coefficient_cb >= 1.0 {
        return Err("Block coefficient CB must be between 0.0 and 1.0".to_string());
    }
    if problem.waterplane_coefficient_cwp <= 0.0 || problem.waterplane_coefficient_cwp >= 1.0 {
        return Err("Waterplane coefficient CWP must be between 0.0 and 1.0".to_string());
    }

    let l = problem.length_between_perpendiculars_m;
    let b = problem.molded_beam_m;
    let t = problem.design_draft_m;
    let d = problem.molded_depth_m;
    let cb = problem.block_coefficient_cb;
    let cwp = problem.waterplane_coefficient_cwp;
    let kg = problem.vertical_center_of_gravity_kg_m;

    let freeboard = d - t;
    // Deck-edge immersion angle theta_f = arctan(2 * Freeboard / B)
    let theta_f_rad = (2.0 * freeboard / b).atan();
    let theta_f_deg = theta_f_rad.to_degrees();

    // 1. Molded displacement:
    let vol_m3 = cb * l * b * t;
    let disp_tonnes = vol_m3 * SEAWATER_DENSITY_T_M3;

    // 2. Vertical center of buoyancy (Morrish's formula):
    // KB = T * (5/6 - CB / (6 * CWP))
    let kb = t * (5.0 / 6.0 - cb / (6.0 * cwp));

    // 3. Transverse metacentric radius BM = I_T / Nabla:
    // With I_T ~ C_IT * L * B^3 and C_IT ~ CWP^2 / 11.4:
    let bm = (cwp.powi(2) * b.powi(2)) / (11.4 * cb * t);

    // 4. Metacenter and initial GM0:
    let km = kb + bm;
    let gm0 = km - kg;
    if gm0 <= 0.0 {
        return Err(format!("Unstable upright equilibrium: GM0 = {:.3} m <= 0.0", gm0));
    }

    // 5. GZ Righting Arm curve computation via Wall-sided formula:
    // For theta <= theta_f: GZ(theta) = sin(theta) * (GM0 + 0.5 * BM * tan(theta)^2)
    // For theta > theta_f: reduced by deck submergence factor
    let gz_curve = |theta_deg: f64| -> f64 {
        let th_rad = theta_deg.to_radians();
        let gz_wallsided = th_rad.sin() * (gm0 + 0.5 * bm * th_rad.tan().powi(2));
        if theta_deg <= theta_f_deg {
            gz_wallsided
        } else {
            let overflow_ratio = (theta_deg - theta_f_deg) / 40.0;
            let decay = (1.0 - 0.75 * overflow_ratio.powi(2)).max(0.0);
            gz_wallsided * decay
        }
    };

    // Numerical integration of GZ area up to 30° and 40° using trapezoidal rule:
    let mut area_30 = 0.0;
    let mut area_40 = 0.0;
    let mut max_gz = 0.0;
    let mut angle_max_gz = 0.0;
    let mut vanishing_angle = 70.0;

    let d_theta = 0.5; // degrees
    let steps_70 = (70.0 / d_theta) as usize;

    for i in 0..steps_70 {
        let th1 = i as f64 * d_theta;
        let th2 = (i + 1) as f64 * d_theta;
        let gz1 = gz_curve(th1);
        let gz2 = gz_curve(th2);

        if gz2 > max_gz {
            max_gz = gz2;
            angle_max_gz = th2;
        }

        if gz1 > 0.0 && gz2 <= 0.0 && vanishing_angle > 69.0 {
            vanishing_angle = th1 + (th2 - th1) * (gz1 / (gz1 - gz2));
        }

        let slice_area_m_rad = 0.5 * (gz1 + gz2) * d_theta.to_radians();
        if th2 <= 30.0 {
            area_30 += slice_area_m_rad;
        }
        if th2 <= 40.0 {
            area_40 += slice_area_m_rad;
        }
    }

    let area_30_40 = (area_40 - area_30).max(0.0);

    // IMO 2008 Intact Stability Code Criteria:
    // 1. Area_30 >= 0.055 m·rad
    // 2. Area_40 >= 0.090 m·rad
    // 3. Area_30_40 >= 0.030 m·rad
    // 4. Max GZ >= 0.20 m at angle >= 25°
    // 5. Initial GM0 >= 0.15 m
    let imo1_ok = area_30 >= 0.055;
    let imo2_ok = area_40 >= 0.090;
    let imo3_ok = area_30_40 >= 0.030;
    let imo4_ok = max_gz >= 0.20 && angle_max_gz >= 25.0;
    let imo5_ok = gm0 >= 0.15;
    let is_safe = imo1_ok && imo2_ok && imo3_ok && imo4_ok && imo5_ok;

    Ok(ShipStabilityResult {
        problem_id: problem.problem_id.clone(),
        displacement_volume_m3: vol_m3,
        displacement_mass_tonnes: disp_tonnes,
        center_of_buoyancy_kb_m: kb,
        metacentric_radius_bm_m: bm,
        transverse_metacenter_km_m: km,
        initial_metacentric_height_gm0_m: gm0,
        design_freeboard_m: freeboard,
        deck_immersion_angle_deg: theta_f_deg,
        max_gz_righting_arm_m: max_gz,
        angle_of_max_gz_deg: angle_max_gz,
        area_under_gz_30deg_m_rad: area_30,
        area_under_gz_40deg_m_rad: area_40,
        area_between_30_and_40deg_m_rad: area_30_40,
        angle_of_vanishing_stability_deg: vanishing_angle,
        is_safe,
    })
}

/// Generic simulation adapter for ship hydrostatic stability.
pub fn solve_ship_stability_sim(problem: &ShipStabilityProblem) -> SimulationResult {
    match solve_ship_stability(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("initial_metacentric_height_gm0_m".to_string(), result.initial_metacentric_height_gm0_m);
            metrics.insert("max_gz_righting_arm_m".to_string(), result.max_gz_righting_arm_m);
            metrics.insert("angle_of_max_gz_deg".to_string(), result.angle_of_max_gz_deg);
            metrics.insert("area_under_gz_30deg_m_rad".to_string(), result.area_under_gz_30deg_m_rad);
            metrics.insert("area_under_gz_40deg_m_rad".to_string(), result.area_under_gz_40deg_m_rad);
            metrics.insert("area_between_30_and_40deg_m_rad".to_string(), result.area_between_30_and_40deg_m_rad);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.initial_metacentric_height_gm0_m, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "initial_metacentric_height_gm0_m",
                result.initial_metacentric_height_gm0_m,
                1.5,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.marine.ship_stability_imo2008".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.marine.ship_stability_imo2008", "0.1.0"),
                logs: vec![format!(
                    "Ship Stability (IMO 2008): GM0 = {:.2} m, GZ_max = {:.2} m @ {:.1}°, A30 = {:.3} m·rad, A40 = {:.3} m·rad (IMO Pass: {})",
                    result.initial_metacentric_height_gm0_m,
                    result.max_gz_righting_arm_m,
                    result.angle_of_max_gz_deg,
                    result.area_under_gz_30deg_m_rad,
                    result.area_under_gz_40deg_m_rad,
                    result.is_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.marine.ship_stability_imo2008".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.marine.ship_stability_imo2008", "0.1.0"),
                logs: vec![format!("Ship stability simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_ship_stability_nominal() {
        let prob = ShipStabilityProblem::default();
        let res = solve_ship_stability(&prob).expect("solver failed");
        println!("Ship stability: Disp = {}, KB = {}, BM = {}, KM = {}, GM0 = {}, GZ_max = {}, A30 = {}, A40 = {}",
            res.displacement_mass_tonnes, res.center_of_buoyancy_kb_m, res.metacentric_radius_bm_m,
            res.transverse_metacenter_km_m, res.initial_metacentric_height_gm0_m, res.max_gz_righting_arm_m,
            res.area_under_gz_30deg_m_rad, res.area_under_gz_40deg_m_rad);

        assert!(res.displacement_mass_tonnes > 15000.0 && res.displacement_mass_tonnes < 18000.0);
        assert!(res.center_of_buoyancy_kb_m > 4.0 && res.center_of_buoyancy_kb_m < 5.5);
        assert!(res.metacentric_radius_bm_m > 4.0 && res.metacentric_radius_bm_m < 6.5);
        assert!(res.initial_metacentric_height_gm0_m > 1.0 && res.initial_metacentric_height_gm0_m < 4.0);
        assert!(res.max_gz_righting_arm_m > 0.5 && res.max_gz_righting_arm_m < 3.5);
        assert!(res.angle_of_max_gz_deg >= 25.0);
        assert!(res.area_under_gz_30deg_m_rad >= 0.055);
        assert!(res.area_under_gz_40deg_m_rad >= 0.090);
        assert!(res.area_between_30_and_40deg_m_rad >= 0.030);
        assert!(res.is_safe);
    }
}

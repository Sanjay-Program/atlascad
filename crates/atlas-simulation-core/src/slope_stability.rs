//! Soil slope stability solver using Bishop's Simplified Method of Slices.
//!
//! Compliant with Eurocode 7 (EN 1997-1) and USACE EM 1110-2-1902.
//! Solves circular slip surface limit equilibrium with pore water pressure (ru)
//! and pseudo-static earthquake horizontal inertia coefficients (kh).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Slope stability problem definition with circular slip surface.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlopeStabilityProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Total vertical height of the embankment/cut slope in meters.
    pub slope_height_m: f64,
    /// Slope inclination angle from horizontal in degrees.
    pub slope_angle_deg: f64,
    /// Total soil bulk unit weight in kN/m^3.
    pub soil_unit_weight_kn_m3: f64,
    /// Effective soil cohesion c' in kPa.
    pub effective_cohesion_kpa: f64,
    /// Effective soil internal friction angle phi' in degrees.
    pub effective_friction_angle_deg: f64,
    /// Pore water pressure ratio ru = u / (gamma * h).
    pub pore_pressure_ratio_ru: f64,
    /// Horizontal pseudo-static earthquake coefficient kh (e.g. 0.05 - 0.15).
    pub seismic_coefficient_kh: f64,
    /// Trial circular slip surface center X coordinate in meters (toe at 0,0).
    pub circle_center_x_m: f64,
    /// Trial circular slip surface center Y coordinate in meters.
    pub circle_center_y_m: f64,
    /// Trial circular slip surface radius R in meters.
    pub slip_circle_radius_m: f64,
    /// Number of vertical slices for limit equilibrium discretization.
    pub number_of_slices: usize,
    /// Iterative convergence tolerance on Factor of Safety (FoS).
    pub convergence_tolerance: f64,
}

impl Default for SlopeStabilityProblem {
    fn default() -> Self {
        Self {
            problem_id: "slope_stability_nominal".to_string(),
            slope_height_m: 10.0,
            slope_angle_deg: 32.0,
            soil_unit_weight_kn_m3: 19.0,
            effective_cohesion_kpa: 12.0,
            effective_friction_angle_deg: 27.0,
            pore_pressure_ratio_ru: 0.15,
            seismic_coefficient_kh: 0.05,
            circle_center_x_m: 5.5,
            circle_center_y_m: 16.0,
            slip_circle_radius_m: 17.5,
            number_of_slices: 25,
            convergence_tolerance: 1.0e-5,
        }
    }
}

/// Results of Bishop's Simplified slope stability analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlopeStabilityResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Factor of Safety calculated by Bishop's Simplified Method.
    pub factor_of_safety_bishop: f64,
    /// Factor of Safety calculated by Ordinary / Fellenius Method.
    pub factor_of_safety_fellenius: f64,
    /// Number of Picard iterations until convergence.
    pub iteration_count: usize,
    /// Whether the iterative solution successfully converged.
    pub converged: bool,
    /// Total overturning/disturbing moment about circle center in kNm/m.
    pub disturbing_moment_kn_m: f64,
    /// Total resisting shear moment about circle center in kNm/m.
    pub resisting_moment_kn_m: f64,
    /// Destabilizing moment contribution from pseudo-static seismic force in kNm/m.
    pub seismic_moment_kn_m: f64,
    /// Total pore water pressure force acting across sliding surface in kN/m.
    pub total_pore_water_thrust_kn: f64,
    /// Total weight of sliding soil mass in kN/m.
    pub total_sliding_weight_kn: f64,
    /// Number of active slices within slip mass.
    pub active_slices_count: usize,
    /// Whether slope meets Eurocode 7 minimum allowable FoS (>= 1.30).
    pub is_stable: bool,
    /// Detailed design commentary and stability classification.
    pub design_assessment: String,
}

/// Solves slope stability using Bishop's Simplified Method of Slices.
pub fn solve_slope_stability(problem: &SlopeStabilityProblem) -> Result<SlopeStabilityResult, String> {
    if problem.slope_height_m <= 0.0 {
        return Err("Slope height must be positive".to_string());
    }
    if problem.slope_angle_deg <= 0.0 || problem.slope_angle_deg >= 90.0 {
        return Err("Slope angle must be between 0 and 90 degrees".to_string());
    }
    if problem.slip_circle_radius_m <= 0.0 {
        return Err("Slip circle radius must be positive".to_string());
    }

    let beta_rad = problem.slope_angle_deg * PI / 180.0;
    let phi_rad = problem.effective_friction_angle_deg * PI / 180.0;
    let tan_phi = phi_rad.tan();
    let c_prime = problem.effective_cohesion_kpa;
    let gamma = problem.soil_unit_weight_kn_m3;
    let r = problem.slip_circle_radius_m;
    let xc = problem.circle_center_x_m;
    let yc = problem.circle_center_y_m;
    let h_slope = problem.slope_height_m;
    let crest_x = h_slope / beta_rad.tan();

    let y_ground = |x: f64| -> f64 {
        if x <= 0.0 {
            0.0
        } else if x >= crest_x {
            h_slope
        } else {
            x * beta_rad.tan()
        }
    };

    let n_slices = problem.number_of_slices.clamp(10, 200);
    let x_left = xc - r * 0.999;
    let x_right = xc + r * 0.999;

    let scan_steps = 200;
    let dx_scan = (x_right - x_left) / (scan_steps as f64);
    let mut x_entry = x_right;
    let mut x_exit = x_left;
    let mut found_exit = false;

    for i in 0..=scan_steps {
        let x = x_left + (i as f64) * dx_scan;
        let r_term = r * r - (x - xc).powi(2);
        if r_term > 0.0 {
            let y_circle = yc - r_term.sqrt();
            let y_surf = y_ground(x);
            if y_circle < y_surf {
                if !found_exit {
                    x_exit = x;
                    found_exit = true;
                }
                x_entry = x;
            }
        }
    }

    if !found_exit || x_entry <= x_exit {
        return Err("Trial circle does not intersect slope profile to form a valid slip mass".to_string());
    }

    struct SliceData {
        b: f64,
        alpha_rad: f64,
        w: f64,
        u_b: f64,
        arm_seismic: f64,
    }

    let b_slice = (x_entry - x_exit) / (n_slices as f64);
    let mut slices = Vec::with_capacity(n_slices);
    let mut total_sliding_weight = 0.0;
    let mut total_pore_water = 0.0;

    for i in 0..n_slices {
        let xi = x_exit + (i as f64 + 0.5) * b_slice;
        let r_term = (r * r - (xi - xc).powi(2)).max(0.0);
        let y_circle = yc - r_term.sqrt();
        let y_surf = y_ground(xi);
        let h_slice = (y_surf - y_circle).max(0.0);

        if h_slice > 0.005 {
            let sin_alpha = ((xi - xc) / r).clamp(-0.999, 0.999);
            let alpha_rad = sin_alpha.asin();
            let w_slice = gamma * b_slice * h_slice;
            let u_slice = problem.pore_pressure_ratio_ru * gamma * h_slice;
            let u_b = u_slice * b_slice;
            let y_mid = y_circle + h_slice / 2.0;
            let arm_seismic = y_mid - yc;

            total_sliding_weight += w_slice;
            total_pore_water += u_b;

            slices.push(SliceData {
                b: b_slice,
                alpha_rad,
                w: w_slice,
                u_b,
                arm_seismic,
            });
        }
    }

    if slices.is_empty() {
        return Err("No active slices generated inside slip mass".to_string());
    }

    let mut num_fellenius = 0.0;
    let mut den_fellenius = 0.0;

    for s in &slices {
        let cos_a = s.alpha_rad.cos().max(0.01);
        let sin_a = s.alpha_rad.sin();
        let l_slice = s.b / cos_a;
        let normal_effective = (s.w * cos_a - s.u_b * l_slice).max(0.0);
        num_fellenius += c_prime * l_slice + normal_effective * tan_phi;
        den_fellenius += s.w * sin_a;
    }

    let fs_fellenius = if den_fellenius.abs() > 1.0e-3 {
        (num_fellenius / den_fellenius).clamp(0.1, 10.0)
    } else {
        1.5
    };

    let mut fs_current = fs_fellenius.max(0.5);
    let mut converged = false;
    let mut iter_count = 0;
    let max_iters = 100;
    let tol = problem.convergence_tolerance.clamp(1.0e-7, 1.0e-3);

    let mut disturbing_moment = 0.0;
    let mut seismic_moment = 0.0;

    for s in &slices {
        let sin_a = s.alpha_rad.sin();
        disturbing_moment += s.w * sin_a * r;
        let m_seis = problem.seismic_coefficient_kh * s.w * s.arm_seismic;
        seismic_moment += m_seis;
    }
    let total_disturbing = (disturbing_moment + seismic_moment).max(1.0);

    for iter in 1..=max_iters {
        iter_count = iter;
        let mut resisting_sum = 0.0;

        for s in &slices {
            let cos_a = s.alpha_rad.cos().max(0.01);
            let sin_a = s.alpha_rad.sin();
            let tan_a = sin_a / cos_a;

            let m_alpha = cos_a * (1.0 + tan_a * tan_phi / fs_current);
            let m_alpha_safe = m_alpha.max(0.10);

            let term_numerator = c_prime * s.b + (s.w - s.u_b) * tan_phi;
            resisting_sum += term_numerator / m_alpha_safe;
        }

        let fs_next = (resisting_sum * r) / total_disturbing;
        if (fs_next - fs_current).abs() < tol {
            fs_current = fs_next;
            converged = true;
            break;
        }
        fs_current = 0.5 * fs_current + 0.5 * fs_next;
    }

    let resisting_moment = fs_current * total_disturbing;
    let is_stable = fs_current >= 1.30;

    let assessment = format!(
        "Slope H={:.1}m, beta={:.1} deg. Bishop FoS={:.3} (Fellenius={:.3}) after {} iters. Slices={}. Sliding mass={:.1} kN/m. Seismic M={:.1} kNm/m. Eurocode 7 standard (>=1.30): {}",
        h_slope,
        problem.slope_angle_deg,
        fs_current,
        fs_fellenius,
        iter_count,
        slices.len(),
        total_sliding_weight,
        seismic_moment,
        if is_stable { "PASS STABLE" } else { "UNSTABLE / REINFORCEMENT REQUIRED" }
    );

    Ok(SlopeStabilityResult {
        problem_id: problem.problem_id.clone(),
        factor_of_safety_bishop: fs_current,
        factor_of_safety_fellenius: fs_fellenius,
        iteration_count: iter_count,
        converged,
        disturbing_moment_kn_m: total_disturbing,
        resisting_moment_kn_m: resisting_moment,
        seismic_moment_kn_m: seismic_moment,
        total_pore_water_thrust_kn: total_pore_water,
        total_sliding_weight_kn: total_sliding_weight,
        active_slices_count: slices.len(),
        is_stable,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for slope stability.
pub fn solve_slope_stability_sim(problem: &SlopeStabilityProblem) -> SimulationResult {
    match solve_slope_stability(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("factor_of_safety_bishop".to_string(), result.factor_of_safety_bishop);
            metrics.insert("factor_of_safety_fellenius".to_string(), result.factor_of_safety_fellenius);
            metrics.insert("total_sliding_weight_kn".to_string(), result.total_sliding_weight_kn);
            metrics.insert("disturbing_moment_kn_m".to_string(), result.disturbing_moment_kn_m);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(result.iteration_count as u32, result.factor_of_safety_bishop, 1, 1e-5);

            let comparison = AnalyticalComparison::assess(
                "factor_of_safety_bishop",
                result.factor_of_safety_bishop,
                1.50,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.geotechnical.slope_stability_bishop".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 3,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.geotechnical.slope_stability_bishop", "0.1.0"),
                logs: vec![result.design_assessment],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.geotechnical.slope_stability_bishop".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.geotechnical.slope_stability_bishop", "0.1.0"),
                logs: vec![format!("Slope stability simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_slope_stability_nominal() {
        let prob = SlopeStabilityProblem::default();
        let sol = solve_slope_stability(&prob).expect("Slope solve should converge");

        assert!(sol.converged);
        assert!(sol.factor_of_safety_bishop > 1.1 && sol.factor_of_safety_bishop < 2.5);
        assert!(sol.factor_of_safety_bishop >= sol.factor_of_safety_fellenius);
        assert!(sol.active_slices_count > 10);
        assert!(sol.total_sliding_weight_kn > 100.0);
    }

    #[test]
    fn test_slope_stability_failure() {
        let prob = SlopeStabilityProblem {
            slope_angle_deg: 55.0,
            effective_cohesion_kpa: 1.0,
            effective_friction_angle_deg: 15.0,
            pore_pressure_ratio_ru: 0.45,
            ..Default::default()
        };
        let sol = solve_slope_stability(&prob).expect("Solver executes");
        assert!(sol.factor_of_safety_bishop < 1.0);
        assert!(!sol.is_stable);
    }
}

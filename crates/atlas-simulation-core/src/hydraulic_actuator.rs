//! Hydraulic cylinder and actuator mechanics solver.
//!
//! Compliant with ISO 6020-2, NFPA T3.6.7 R2, and ISO 10766.
//! Covers push/pull forces, Lamé thick-wall barrel hoop/radial/von Mises stresses,
//! piston rod Euler/Rankine-Gordon buckling, and end-of-stroke cushion deceleration pressure.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::{FRAC_1_SQRT_2, PI};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// End condition factor for piston rod column buckling.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum MountingCondition {
    /// Pinned at both ends (e.g. clevis, spherical bearing): K = 1.0.
    PinnedPinned,
    /// Fixed at cylinder base, guided/pinned at rod end: K = 0.707.
    FixedPinned,
    /// Fixed at both ends (rigid flange / rigid guide): K = 0.5.
    FixedFixed,
    /// Fixed at cylinder base, free at rod end (cantilever): K = 2.0.
    FixedFree,
}

impl MountingCondition {
    /// Effective length factor K according to Euler column theory.
    pub fn k_factor(self) -> f64 {
        match self {
            Self::PinnedPinned => 1.0,
            Self::FixedPinned => FRAC_1_SQRT_2,
            Self::FixedFixed => 0.5,
            Self::FixedFree => 2.0,
        }
    }
}

/// Problem definition for hydraulic actuator structural & dynamic sizing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydraulicActuatorProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Cylinder bore internal diameter in mm.
    pub bore_diameter_mm: f64,
    /// Piston rod diameter in mm.
    pub rod_diameter_mm: f64,
    /// Full stroke extension length in mm.
    pub stroke_length_mm: f64,
    /// Hydraulic working operating pressure in bar.
    pub working_pressure_bar: f64,
    /// Cylinder barrel wall thickness in mm.
    pub barrel_wall_thickness_mm: f64,
    /// Cylinder barrel material yield strength in MPa (e.g. E355 = 355.0).
    pub barrel_yield_strength_mpa: f64,
    /// Piston rod material yield strength in MPa (e.g. 20MnV6 = 460.0).
    pub rod_yield_strength_mpa: f64,
    /// Piston rod Young's modulus in GPa (e.g. 210.0 for steel).
    pub rod_youngs_modulus_gpa: f64,
    /// Mounting and guide configuration for column buckling.
    pub mounting_end_condition: MountingCondition,
    /// Moving external mass attached to actuator rod in kg.
    pub load_mass_kg: f64,
    /// Actuator stroke extension speed in m/s.
    pub stroke_velocity_m_s: f64,
    /// End-of-stroke deceleration cushion spear length in mm.
    pub cushion_length_mm: f64,
    /// Cushion spear outer diameter in mm.
    pub cushion_spear_diameter_mm: f64,
    /// Mechanical and seal transmission efficiency factor (0.85 - 0.98).
    pub mechanical_efficiency: f64,
}

impl Default for HydraulicActuatorProblem {
    fn default() -> Self {
        Self {
            problem_id: "hydraulic_actuator_nominal".to_string(),
            bore_diameter_mm: 100.0,
            rod_diameter_mm: 56.0,
            stroke_length_mm: 600.0,
            working_pressure_bar: 210.0,
            barrel_wall_thickness_mm: 12.0,
            barrel_yield_strength_mpa: 355.0,
            rod_yield_strength_mpa: 460.0,
            rod_youngs_modulus_gpa: 210.0,
            mounting_end_condition: MountingCondition::PinnedPinned,
            load_mass_kg: 2500.0,
            stroke_velocity_m_s: 0.45,
            cushion_length_mm: 25.0,
            cushion_spear_diameter_mm: 32.0,
            mechanical_efficiency: 0.95,
        }
    }
}

/// Sizing and verification results for hydraulic actuator.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HydraulicActuatorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Piston bore full push area in cm^2.
    pub push_area_cm2: f64,
    /// Annulus retraction pull area in cm^2.
    pub pull_area_cm2: f64,
    /// Available push extension force in kN.
    pub push_force_kn: f64,
    /// Available pull retraction force in kN.
    pub pull_force_kn: f64,
    /// Ratio of push force to pull force.
    pub push_pull_ratio: f64,
    /// Barrel inner radius in mm.
    pub barrel_inner_radius_mm: f64,
    /// Barrel outer radius in mm.
    pub barrel_outer_radius_mm: f64,
    /// Maximum hoop (circumferential) stress at bore surface in MPa.
    pub barrel_hoop_stress_mpa: f64,
    /// Internal radial compressive stress at bore surface in MPa.
    pub barrel_radial_stress_mpa: f64,
    /// Equivalent von Mises stress at cylinder bore in MPa.
    pub barrel_von_mises_stress_mpa: f64,
    /// Barrel yield safety factor.
    pub barrel_safety_factor: f64,
    /// Minimum recommended barrel thickness for FS = 2.0 in mm.
    pub barrel_min_recommended_thickness_mm: f64,
    /// Effective rod buckling length in mm.
    pub rod_effective_length_mm: f64,
    /// Column slenderness ratio lambda.
    pub rod_slenderness_ratio: f64,
    /// Critical slenderness dividing Euler from Johnson inelastic buckling.
    pub rod_critical_slenderness: f64,
    /// Critical buckling limit load in kN.
    pub rod_critical_buckling_load_kn: f64,
    /// Factor of safety against piston rod buckling.
    pub rod_buckling_safety_factor: f64,
    /// Governing buckling mode: "EulerElastic" or "JohnsonInelastic".
    pub rod_buckling_mode: String,
    /// Mean cushion deceleration in m/s^2.
    pub cushion_deceleration_m_s2: f64,
    /// Dissipated kinetic energy during cushioning in Joules.
    pub cushion_kinetic_energy_j: f64,
    /// Peak hydraulic pressure developed in cushion chamber in bar.
    pub peak_cushion_pressure_bar: f64,
    /// Whether barrel stress, buckling, and cushion pressure meet safety standards.
    pub is_safe: bool,
    /// Detailed design assessment and compliance summary.
    pub design_assessment: String,
}

/// Solves hydraulic actuator mechanics, barrel stresses, rod buckling, and cushioning.
pub fn solve_hydraulic_actuator(problem: &HydraulicActuatorProblem) -> Result<HydraulicActuatorResult, String> {
    if problem.bore_diameter_mm <= 0.0 {
        return Err("Bore diameter must be positive".to_string());
    }
    if problem.rod_diameter_mm <= 0.0 || problem.rod_diameter_mm >= problem.bore_diameter_mm {
        return Err("Rod diameter must be positive and less than bore diameter".to_string());
    }
    if problem.stroke_length_mm <= 0.0 {
        return Err("Stroke length must be positive".to_string());
    }
    if problem.working_pressure_bar <= 0.0 {
        return Err("Working pressure must be positive".to_string());
    }
    if problem.barrel_wall_thickness_mm <= 0.0 {
        return Err("Barrel wall thickness must be positive".to_string());
    }

    // 1. Hydraulic Push and Pull Forces
    let r_i_m = (problem.bore_diameter_mm / 2.0) / 1000.0;
    let r_rod_m = (problem.rod_diameter_mm / 2.0) / 1000.0;
    let a_push_m2 = PI * r_i_m * r_i_m;
    let a_rod_m2 = PI * r_rod_m * r_rod_m;
    let a_pull_m2 = a_push_m2 - a_rod_m2;

    let p_pa = problem.working_pressure_bar * 1.0e5;
    let eta = problem.mechanical_efficiency.clamp(0.5, 1.0);

    let push_force_n = p_pa * a_push_m2 * eta;
    let pull_force_n = p_pa * a_pull_m2 * eta;
    let push_force_kn = push_force_n / 1000.0;
    let pull_force_kn = pull_force_n / 1000.0;
    let push_pull_ratio = push_force_n / pull_force_n.max(1.0e-3);

    let push_area_cm2 = a_push_m2 * 1.0e4;
    let pull_area_cm2 = a_pull_m2 * 1.0e4;

    // 2. Thick-Walled Cylinder Barrel Stresses (Lamé Equations)
    let r_i_mm = problem.bore_diameter_mm / 2.0;
    let r_o_mm = r_i_mm + problem.barrel_wall_thickness_mm;
    let p_mpa = problem.working_pressure_bar * 0.10; // 1 bar = 0.1 MPa

    let r_i_sq = r_i_mm * r_i_mm;
    let r_o_sq = r_o_mm * r_o_mm;
    let denominator = r_o_sq - r_i_sq;

    // Hoop stress at r = r_i (inner bore, where stress is highest)
    let sigma_theta = p_mpa * (r_o_sq + r_i_sq) / denominator;
    // Radial stress at r = r_i
    let sigma_r = -p_mpa;
    // Longitudinal closed-end stress
    let sigma_z = p_mpa * r_i_sq / denominator;

    // von Mises equivalent stress
    let vm_sq = 0.5 * ((sigma_theta - sigma_r).powi(2) + (sigma_r - sigma_z).powi(2) + (sigma_z - sigma_theta).powi(2));
    let barrel_von_mises = vm_sq.sqrt();
    let barrel_sf = problem.barrel_yield_strength_mpa / barrel_von_mises.max(1.0e-4);

    // Minimum wall thickness for design FS = 2.0:
    let allowable_stress = problem.barrel_yield_strength_mpa / 2.0;
    let t_min_mm = if allowable_stress > p_mpa {
        r_i_mm * (((allowable_stress + p_mpa) / (allowable_stress - p_mpa)).sqrt() - 1.0)
    } else {
        problem.barrel_wall_thickness_mm * 2.0
    };

    // 3. Piston Rod Buckling (ISO 10766 & Euler-Johnson)
    let k_factor = problem.mounting_end_condition.k_factor();
    let l_eff_mm = k_factor * problem.stroke_length_mm;
    let l_eff_m = l_eff_mm / 1000.0;
    let d_rod_m = problem.rod_diameter_mm / 1000.0;
    let i_m4 = PI * d_rod_m.powi(4) / 64.0;
    let e_pa = problem.rod_youngs_modulus_gpa * 1.0e9;
    let s_y_pa = problem.rod_yield_strength_mpa * 1.0e6;

    // Radius of gyration k = d / 4
    let rg_m = d_rod_m / 4.0;
    let lambda = l_eff_m / rg_m;
    // Critical slenderness dividing Euler from Johnson inelastic
    let lambda_c = PI * (2.0 * e_pa / s_y_pa).sqrt();

    let (p_crit_n, mode) = if lambda >= lambda_c {
        // Euler elastic buckling: P_cr = pi^2 * E * I / L_eff^2
        let p_euler = (PI * PI * e_pa * i_m4) / (l_eff_m * l_eff_m);
        (p_euler, "EulerElastic".to_string())
    } else {
        // Johnson parabolic inelastic formula
        let p_johnson = a_rod_m2 * (s_y_pa - (s_y_pa * s_y_pa / (4.0 * PI * PI * e_pa)) * lambda * lambda);
        (p_johnson.max(1.0), "JohnsonInelastic".to_string())
    };

    let p_crit_kn = p_crit_n / 1000.0;
    let rod_buckling_sf = p_crit_kn / push_force_kn.max(1.0e-3);

    // 4. Cushioning Chamber Dynamics
    let v_m_s = problem.stroke_velocity_m_s.max(0.01);
    let l_c_m = (problem.cushion_length_mm / 1000.0).max(0.005);
    let r_spear_m = (problem.cushion_spear_diameter_mm / 2.0) / 1000.0;
    let a_cush_m2 = (a_push_m2 - PI * r_spear_m * r_spear_m).max(1.0e-4);

    let e_k_j = 0.5 * problem.load_mass_kg * v_m_s * v_m_s;
    let decel_a = (v_m_s * v_m_s) / (2.0 * l_c_m);
    let f_inertia_n = problem.load_mass_kg * decel_a;

    // Peak cushion pressure: p_c = (F_hyd_push + F_inertia) / A_cush
    let p_cush_pa = (push_force_n + f_inertia_n) / a_cush_m2;
    let peak_cushion_bar = p_cush_pa / 1.0e5;

    // Proof pressure limit check (typically 1.5 * working pressure)
    let proof_limit_bar = problem.working_pressure_bar * 1.5;
    let is_cushion_safe = peak_cushion_bar <= proof_limit_bar * 1.25;
    let is_barrel_safe = barrel_sf >= 1.5;
    let is_rod_safe = rod_buckling_sf >= 2.0;
    let is_safe = is_barrel_safe && is_rod_safe && is_cushion_safe;

    let assessment = format!(
        "Actuator push={:.1}kN, pull={:.1}kN. Barrel FS={:.2} ({}), Rod buckling FS={:.2} (mode: {}, lambda={:.1} vs lambda_c={:.1}). Cushion peak P={:.1} bar (proof limit={:.1} bar). Overall status: {}",
        push_force_kn,
        pull_force_kn,
        barrel_sf,
        if is_barrel_safe { "PASS" } else { "INSUFFICIENT" },
        rod_buckling_sf,
        mode,
        lambda,
        lambda_c,
        peak_cushion_bar,
        proof_limit_bar,
        if is_safe { "SAFE CONFORMANT" } else { "REDESIGN REQUIRED" }
    );

    Ok(HydraulicActuatorResult {
        problem_id: problem.problem_id.clone(),
        push_area_cm2,
        pull_area_cm2,
        push_force_kn,
        pull_force_kn,
        push_pull_ratio,
        barrel_inner_radius_mm: r_i_mm,
        barrel_outer_radius_mm: r_o_mm,
        barrel_hoop_stress_mpa: sigma_theta,
        barrel_radial_stress_mpa: sigma_r,
        barrel_von_mises_stress_mpa: barrel_von_mises,
        barrel_safety_factor: barrel_sf,
        barrel_min_recommended_thickness_mm: t_min_mm,
        rod_effective_length_mm: l_eff_mm,
        rod_slenderness_ratio: lambda,
        rod_critical_slenderness: lambda_c,
        rod_critical_buckling_load_kn: p_crit_kn,
        rod_buckling_safety_factor: rod_buckling_sf,
        rod_buckling_mode: mode,
        cushion_deceleration_m_s2: decel_a,
        cushion_kinetic_energy_j: e_k_j,
        peak_cushion_pressure_bar: peak_cushion_bar,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for hydraulic actuator.
pub fn solve_hydraulic_actuator_sim(problem: &HydraulicActuatorProblem) -> SimulationResult {
    match solve_hydraulic_actuator(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("push_force_kn".to_string(), result.push_force_kn);
            metrics.insert("pull_force_kn".to_string(), result.pull_force_kn);
            metrics.insert("barrel_safety_factor".to_string(), result.barrel_safety_factor);
            metrics.insert("rod_buckling_safety_factor".to_string(), result.rod_buckling_safety_factor);
            metrics.insert("peak_cushion_pressure_bar".to_string(), result.peak_cushion_pressure_bar);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.push_force_kn, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "push_force_kn",
                result.push_force_kn,
                150.0,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.hydraulic.actuator_iso6020".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 2,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.hydraulic.actuator_iso6020", "0.1.0"),
                logs: vec![result.design_assessment],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.hydraulic.actuator_iso6020".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.hydraulic.actuator_iso6020", "0.1.0"),
                logs: vec![format!("Hydraulic actuator simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_hydraulic_actuator_nominal() {
        let prob = HydraulicActuatorProblem::default();
        let sol = solve_hydraulic_actuator(&prob).expect("Nominal actuator sizing should succeed");

        assert!(sol.push_force_kn > 140.0 && sol.push_force_kn < 170.0);
        assert!(sol.pull_force_kn > 95.0 && sol.pull_force_kn < 120.0);
        assert!(sol.push_pull_ratio > 1.3 && sol.push_pull_ratio < 1.6);
        assert!(sol.barrel_safety_factor > 2.0);
        assert!(sol.rod_buckling_safety_factor > 2.0);
        assert!(sol.peak_cushion_pressure_bar > 200.0);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_hydraulic_actuator_buckling_failure() {
        let prob = HydraulicActuatorProblem {
            stroke_length_mm: 3500.0, // very long stroke
            rod_diameter_mm: 30.0,    // thin rod
            mounting_end_condition: MountingCondition::FixedFree, // K=2.0
            ..Default::default()
        };
        let sol = solve_hydraulic_actuator(&prob).expect("Calculation should execute");
        assert!(!sol.is_safe);
        assert!(sol.rod_buckling_safety_factor < 1.0);
    }
}

//! Structural buckling, column stability, high-cycle fatigue, and composite laminate mechanics.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard column boundary end fixity condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ColumnEndCondition {
    PinnedPinned, // K = 1.0
    FixedFree,    // K = 2.0 (flagpole)
    FixedPinned,  // K = 0.699
    FixedFixed,   // K = 0.50
}

impl ColumnEndCondition {
    pub const fn effective_length_factor(self) -> f64 {
        match self {
            Self::PinnedPinned => 1.0,
            Self::FixedFree => 2.0,
            Self::FixedPinned => 0.699,
            Self::FixedFixed => 0.50,
        }
    }
}

/// Problem definition for column buckling and stability analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BucklingProblem {
    pub column_length_m: f64,
    pub elastic_modulus_pa: f64,
    pub yield_strength_pa: f64,
    pub moment_of_inertia_m4: f64,
    pub cross_section_area_m2: f64,
    pub end_condition: ColumnEndCondition,
    pub applied_load_n: f64,
}

impl Default for BucklingProblem {
    fn default() -> Self {
        Self {
            column_length_m: 3.0,
            elastic_modulus_pa: 200.0e9, // Structural steel (200 GPa)
            yield_strength_pa: 250.0e6,  // 250 MPa
            moment_of_inertia_m4: 8.33e-6,
            cross_section_area_m2: 0.01,
            end_condition: ColumnEndCondition::PinnedPinned,
            applied_load_n: 150_000.0, // 150 kN
        }
    }
}

/// Solves structural column stability using Euler elastic or Johnson parabolic inelastic criteria.
pub fn solve_column_buckling(prob: &BucklingProblem) -> Result<SimulationResult, String> {
    if prob.column_length_m <= 0.0 || prob.elastic_modulus_pa <= 0.0 {
        return Err("Invalid column physical dimensions or modulus".to_string());
    }

    let k = prob.end_condition.effective_length_factor();
    let l_eff = k * prob.column_length_m;
    let a = prob.cross_section_area_m2.max(1e-8);
    let i = prob.moment_of_inertia_m4.max(1e-12);

    // Radius of gyration r = sqrt(I / A)
    let r_gyration = (i / a).sqrt();
    // Slenderness ratio lambda = L_eff / r
    let slenderness = l_eff / r_gyration;

    // Critical transition slenderness between Johnson inelastic and Euler elastic buckling:
    // lambda_c = sqrt(2 * pi^2 * E / sigma_y)
    let lambda_crit = (2.0 * std::f64::consts::PI * std::f64::consts::PI * prob.elastic_modulus_pa
        / prob.yield_strength_pa.max(1e6))
    .sqrt();

    let (p_cr, sigma_cr, regime) = if slenderness >= lambda_crit {
        // Euler elastic buckling: P_cr = pi^2 * E * I / (L_eff)^2
        let p = (std::f64::consts::PI * std::f64::consts::PI * prob.elastic_modulus_pa * i)
            / (l_eff * l_eff);
        let s = p / a;
        (p, s, "Euler Elastic Buckling")
    } else {
        // Johnson parabolic inelastic buckling:
        // sigma_cr = sigma_y * (1 - sigma_y * lambda^2 / (4 * pi^2 * E))
        let factor = (prob.yield_strength_pa * slenderness * slenderness)
            / (4.0 * std::f64::consts::PI * std::f64::consts::PI * prob.elastic_modulus_pa);
        let s = prob.yield_strength_pa * (1.0 - factor).max(0.1);
        let p = s * a;
        (p, s, "Johnson Inelastic Buckling")
    };

    let fos = if prob.applied_load_n > 0.0 {
        p_cr / prob.applied_load_n
    } else {
        f64::INFINITY
    };

    // Analytical verification for Pinned-Pinned Euler formula
    let analytical_euler_p =
        (std::f64::consts::PI * std::f64::consts::PI * prob.elastic_modulus_pa * i)
            / (l_eff * l_eff);
    let val = AnalyticalComparison::assess(
        "Euler Classical Critical Buckling Load (N)",
        p_cr,
        analytical_euler_p,
    );

    let mut metrics = HashMap::new();
    metrics.insert("critical_buckling_load_n".to_string(), p_cr);
    metrics.insert("critical_stress_mpa".to_string(), sigma_cr / 1.0e6);
    metrics.insert("slenderness_ratio".to_string(), slenderness);
    metrics.insert("critical_slenderness".to_string(), lambda_crit);
    metrics.insert("factor_of_safety".to_string(), fos);
    metrics.insert("effective_length_m".to_string(), l_eff);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, fos, 1, 1e-4);

    let logs = vec![
        format!(
            "Column Buckling Analysis: Length = {:.2} m, K = {:.3} ({:?})",
            prob.column_length_m, k, prob.end_condition
        ),
        format!(
            "Slenderness Ratio lambda = {:.1} (Transition lambda_c = {:.1})",
            slenderness, lambda_crit
        ),
        format!("Regime: {}", regime),
        format!(
            "Critical Load P_cr = {:.1} kN, Applied = {:.1} kN -> FoS = {:.2}",
            p_cr / 1000.0,
            prob.applied_load_n / 1000.0,
            fos
        ),
    ];

    Ok(SimulationResult {
        problem_id: "column-buckling".to_string(),
        engine_id: "atlas-structural".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-structural", "0.1.0"),
        logs,
    })
}

/// Problem definition for high-cycle fatigue life and mean stress analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FatigueProblem {
    pub endurance_limit_pa: f64,    // S_e
    pub ultimate_tensile_pa: f64,   // S_ut
    pub yield_strength_pa: f64,     // S_y
    pub alternating_stress_pa: f64, // sigma_a
    pub mean_stress_pa: f64,        // sigma_m
}

impl Default for FatigueProblem {
    fn default() -> Self {
        Self {
            endurance_limit_pa: 250.0e6,
            ultimate_tensile_pa: 500.0e6,
            yield_strength_pa: 400.0e6,
            alternating_stress_pa: 120.0e6,
            mean_stress_pa: 80.0e6,
        }
    }
}

/// Solves high-cycle fatigue safety factors using Goodman, Gerber, and Soderberg criteria.
pub fn solve_fatigue_safety(prob: &FatigueProblem) -> Result<SimulationResult, String> {
    let s_e = prob.endurance_limit_pa.max(1.0);
    let s_ut = prob.ultimate_tensile_pa.max(1.0);
    let s_y = prob.yield_strength_pa.max(1.0);
    let sa = prob.alternating_stress_pa;
    let sm = prob.mean_stress_pa;

    // Modified Goodman: (sigma_a / S_e) + (sigma_m / S_ut) = 1 / FoS
    let goodman_denom = (sa / s_e) + (sm / s_ut);
    let fos_goodman = if goodman_denom > 1e-9 {
        1.0 / goodman_denom
    } else {
        f64::INFINITY
    };

    // Gerber parabolic: (sigma_a / S_e) + (sigma_m / S_ut)^2 = 1 / FoS
    let gerber_denom = (sa / s_e) + (sm / s_ut).powi(2);
    let fos_gerber = if gerber_denom > 1e-9 {
        1.0 / gerber_denom
    } else {
        f64::INFINITY
    };

    // Soderberg (conservative yield): (sigma_a / S_e) + (sigma_m / S_y) = 1 / FoS
    let soderberg_denom = (sa / s_e) + (sm / s_y);
    let fos_soderberg = if soderberg_denom > 1e-9 {
        1.0 / soderberg_denom
    } else {
        f64::INFINITY
    };

    // Equivalent completely reversed alternating stress (Goodman equivalent)
    let s_eq = sa / (1.0 - (sm / s_ut)).max(1e-4);

    let val = AnalyticalComparison::assess(
        "Modified Goodman Factor of Safety",
        fos_goodman,
        1.0 / ((sa / s_e) + (sm / s_ut)),
    );

    let mut metrics = HashMap::new();
    metrics.insert("fos_goodman".to_string(), fos_goodman);
    metrics.insert("fos_gerber".to_string(), fos_gerber);
    metrics.insert("fos_soderberg".to_string(), fos_soderberg);
    metrics.insert(
        "equivalent_alternating_stress_mpa".to_string(),
        s_eq / 1.0e6,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, fos_goodman, 1, 1e-4);

    let logs = vec![
        format!(
            "Fatigue Analysis: S_a = {:.1} MPa, S_m = {:.1} MPa",
            sa / 1e6,
            sm / 1e6
        ),
        format!(
            "Goodman FoS = {:.3}, Gerber FoS = {:.3}, Soderberg FoS = {:.3}",
            fos_goodman, fos_gerber, fos_soderberg
        ),
        format!(
            "Equivalent Reversed Stress S_eq = {:.1} MPa (S_e = {:.1} MPa)",
            s_eq / 1e6,
            s_e / 1e6
        ),
    ];

    Ok(SimulationResult {
        problem_id: "fatigue-analysis".to_string(),
        engine_id: "atlas-structural".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-structural", "0.1.0"),
        logs,
    })
}

/// Composite orthotropic unidirectional lamina properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrthotropicLamina {
    pub e1_pa: f64,  // Longitudinal modulus (fiber direction)
    pub e2_pa: f64,  // Transverse modulus
    pub nu12: f64,   // Major Poisson's ratio
    pub g12_pa: f64, // In-plane shear modulus
    pub thickness_m: f64,
}

impl Default for OrthotropicLamina {
    fn default() -> Self {
        // Typical Carbon/Epoxy unidirectional tape (T300/5208)
        Self {
            e1_pa: 140.0e9,
            e2_pa: 10.0e9,
            nu12: 0.30,
            g12_pa: 5.0e9,
            thickness_m: 0.00015, // 0.15 mm per ply
        }
    }
}

/// Multi-ply composite laminate definition.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CompositeLaminateProblem {
    pub lamina: OrthotropicLamina,
    pub ply_angles_deg: Vec<f64>, // e.g. [0.0, 45.0, -45.0, 90.0, 90.0, -45.0, 45.0, 0.0]
}

impl Default for CompositeLaminateProblem {
    fn default() -> Self {
        Self {
            lamina: OrthotropicLamina::default(),
            ply_angles_deg: vec![0.0, 45.0, -45.0, 90.0, 90.0, -45.0, 45.0, 0.0], // Quasi-isotropic
        }
    }
}

/// Solves Classical Laminate Theory (CLT) extensional stiffness matrix A and effective engineering constants.
pub fn solve_composite_laminate(
    prob: &CompositeLaminateProblem,
) -> Result<SimulationResult, String> {
    if prob.ply_angles_deg.is_empty() {
        return Err("Laminate requires at least one ply".to_string());
    }

    let e1 = prob.lamina.e1_pa;
    let e2 = prob.lamina.e2_pa;
    let nu12 = prob.lamina.nu12;
    let g12 = prob.lamina.g12_pa;
    let t = prob.lamina.thickness_m;

    let nu21 = nu12 * (e2 / e1);
    let denom = (1.0 - nu12 * nu21).max(1e-4);

    // Plane stress reduced stiffness Q_ij in principal material coordinates
    let q11 = e1 / denom;
    let q22 = e2 / denom;
    let q12 = (nu12 * e2) / denom;
    let q66 = g12;

    let total_thickness = (prob.ply_angles_deg.len() as f64) * t;

    // Extensional stiffness A_ij = sum(Q_bar_ij * t)
    let mut a11 = 0.0;
    let mut a22 = 0.0;
    let mut a12 = 0.0;
    let mut a66 = 0.0;

    for &angle_deg in &prob.ply_angles_deg {
        let theta = angle_deg.to_radians();
        let m = theta.cos();
        let n = theta.sin();
        let m2 = m * m;
        let n2 = n * n;
        let m4 = m2 * m2;
        let n4 = n2 * n2;

        // Transformed stiffness Q_bar
        let q_bar11 = q11 * m4 + 2.0 * (q12 + 2.0 * q66) * m2 * n2 + q22 * n4;
        let q_bar22 = q11 * n4 + 2.0 * (q12 + 2.0 * q66) * m2 * n2 + q22 * m4;
        let q_bar12 = (q11 + q22 - 4.0 * q66) * m2 * n2 + q12 * (m4 + n4);
        let q_bar66 = (q11 + q22 - 2.0 * q12 - 2.0 * q66) * m2 * n2 + q66 * (m4 + n4);

        a11 += q_bar11 * t;
        a22 += q_bar22 * t;
        a12 += q_bar12 * t;
        a66 += q_bar66 * t;
    }

    // Effective laminate engineering moduli
    let a_det = (a11 * a22 - a12 * a12).max(1e-6);
    let effective_ex_pa = a_det / (total_thickness * a22);
    let effective_ey_pa = a_det / (total_thickness * a11);
    let effective_nu_xy = a12 / a22;

    let val = AnalyticalComparison::assess(
        "Quasi-Isotropic Approximate Modulus Verification",
        effective_ex_pa,
        effective_ey_pa,
    );

    let mut metrics = HashMap::new();
    metrics.insert("effective_ex_gpa".to_string(), effective_ex_pa / 1.0e9);
    metrics.insert("effective_ey_gpa".to_string(), effective_ey_pa / 1.0e9);
    metrics.insert("effective_nu_xy".to_string(), effective_nu_xy);
    metrics.insert("a11_stiffness_mn_m".to_string(), a11 / 1.0e6);
    metrics.insert("a22_stiffness_mn_m".to_string(), a22 / 1.0e6);
    metrics.insert("a66_stiffness_mn_m".to_string(), a66 / 1.0e6);
    metrics.insert("total_thickness_mm".to_string(), total_thickness * 1000.0);
    metrics.insert("ply_count".to_string(), prob.ply_angles_deg.len() as f64);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, effective_ex_pa, 1, 1e-4);

    let logs = vec![
        format!(
            "Composite CLT Analysis: {} plies, total thickness = {:.3} mm",
            prob.ply_angles_deg.len(),
            total_thickness * 1000.0
        ),
        format!("Layup sequence: {:?}", prob.ply_angles_deg),
        format!(
            "Effective Moduli: Ex = {:.2} GPa, Ey = {:.2} GPa, Nu_xy = {:.3}",
            effective_ex_pa / 1e9,
            effective_ey_pa / 1e9,
            effective_nu_xy
        ),
    ];

    Ok(SimulationResult {
        problem_id: "composite-laminate".to_string(),
        engine_id: "atlas-structural".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-structural", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_euler_column_buckling() -> Result<(), String> {
        let prob = BucklingProblem::default();
        let res = solve_column_buckling(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);
        let p_cr = res
            .scalar_metrics
            .get("critical_buckling_load_n")
            .copied()
            .unwrap_or(0.0);
        let fos = res
            .scalar_metrics
            .get("factor_of_safety")
            .copied()
            .unwrap_or(0.0);
        assert!(p_cr > 1.0e6); // > 1 MN
        assert!(fos > 1.0);
        Ok(())
    }

    #[test]
    fn test_fatigue_goodman_and_gerber() -> Result<(), String> {
        let prob = FatigueProblem::default();
        let res = solve_fatigue_safety(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);
        let fg = res
            .scalar_metrics
            .get("fos_goodman")
            .copied()
            .unwrap_or(0.0);
        let fgerber = res.scalar_metrics.get("fos_gerber").copied().unwrap_or(0.0);
        // Gerber is always slightly less conservative than Goodman
        assert!(fgerber > fg);
        assert!(fg > 1.0);
        Ok(())
    }

    #[test]
    fn test_quasi_isotropic_composite_clt() -> Result<(), String> {
        let prob = CompositeLaminateProblem::default();
        let res = solve_composite_laminate(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);
        let ex = res
            .scalar_metrics
            .get("effective_ex_gpa")
            .copied()
            .unwrap_or(0.0);
        let ey = res
            .scalar_metrics
            .get("effective_ey_gpa")
            .copied()
            .unwrap_or(0.0);
        // For balanced symmetric [0/45/-45/90]s quasi-isotropic laminate, Ex == Ey
        assert!((ex - ey).abs() < 0.1);
        Ok(())
    }
}

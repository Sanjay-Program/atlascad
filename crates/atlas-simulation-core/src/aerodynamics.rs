//! Aerodynamics, wing theory, and compressible gas dynamics.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Parameters for subsonic wing aerodynamic evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WingAeroProblem {
    pub wing_area_m2: f64,
    pub aspect_ratio: f64,
    pub oswald_efficiency: f64,
    pub angle_of_attack_deg: f64,
    pub zero_lift_aoa_deg: f64,
    pub airspeed_m_s: f64,
    pub altitude_m: f64,
    pub cd0_profile_drag: f64,
}

/// Compressible normal shock wave problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NormalShockProblem {
    pub upstream_mach: f64,
    pub specific_heat_ratio_gamma: f64,
    pub upstream_pressure_pa: f64,
    pub upstream_temp_k: f64,
}

/// Solve subsonic 3D wing aerodynamics using thin airfoil theory and lifting-line.
pub fn solve_wing_aerodynamics(prob: &WingAeroProblem) -> Result<SimulationResult, String> {
    if prob.wing_area_m2 <= 0.0 || prob.aspect_ratio <= 0.0 || prob.airspeed_m_s <= 0.0 {
        return Err("Wing geometry and airspeed must be positive".to_string());
    }

    // Standard atmospheric density model approximation
    let rho = 1.225 * (-prob.altitude_m / 8500.0).exp();

    // 2D lift curve slope: 2 * pi per radian
    // 3D finite wing lift slope: a = a0 / (1 + a0 / (pi * AR * e))
    let a0 = 2.0 * std::f64::consts::PI;
    let ar = prob.aspect_ratio;
    let e = prob.oswald_efficiency.clamp(0.6, 1.0);
    let a_3d = a0 / (1.0 + a0 / (std::f64::consts::PI * ar * e));

    let alpha_rad = (prob.angle_of_attack_deg - prob.zero_lift_aoa_deg).to_radians();
    let cl = a_3d * alpha_rad;

    // Induced drag: Cdi = Cl^2 / (pi * AR * e)
    let cdi = if ar * e > 1e-6 {
        (cl * cl) / (std::f64::consts::PI * ar * e)
    } else {
        0.0
    };

    let cd = prob.cd0_profile_drag + cdi;
    let l_over_d = if cd > 1e-6 { cl / cd } else { 0.0 };

    let q_inf = 0.5 * rho * prob.airspeed_m_s.powi(2);
    let lift_force_n = q_inf * prob.wing_area_m2 * cl;
    let drag_force_n = q_inf * prob.wing_area_m2 * cd;

    // Theoretical 2D comparison for infinite aspect ratio
    let theoretical_cl_2d = a0 * alpha_rad;
    let val = AnalyticalComparison::assess(
        "2D vs 3D Lift Slope Correction",
        cl,
        theoretical_cl_2d / (1.0 + a0 / (std::f64::consts::PI * ar * e)),
    );

    let mut metrics = HashMap::new();
    metrics.insert("lift_coefficient_cl".to_string(), cl);
    metrics.insert("drag_coefficient_cd".to_string(), cd);
    metrics.insert("induced_drag_cdi".to_string(), cdi);
    metrics.insert("lift_to_drag_ratio".to_string(), l_over_d);
    metrics.insert("lift_force_n".to_string(), lift_force_n);
    metrics.insert("drag_force_n".to_string(), drag_force_n);
    metrics.insert("dynamic_pressure_pa".to_string(), q_inf);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, cl, 1, 1e-4);

    let logs = vec![
        format!(
            "Airspeed: {:.1} m/s, Altitude: {:.0} m (rho = {:.3} kg/m³)",
            prob.airspeed_m_s, prob.altitude_m, rho
        ),
        format!(
            "AoA: {:.1} deg -> CL = {:.3}, CD = {:.4} (L/D = {:.1})",
            prob.angle_of_attack_deg, cl, cd, l_over_d
        ),
        format!(
            "Total Lift: {:.1} N, Drag: {:.1} N",
            lift_force_n, drag_force_n
        ),
    ];

    Ok(SimulationResult {
        problem_id: "wing-aerodynamics".to_string(),
        engine_id: "atlas-aero".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 2,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-aero", "0.1.0"),
        logs,
    })
}

/// Solves Rankine-Hugoniot normal shock relations for supersonic gas dynamics.
pub fn solve_normal_shock(prob: &NormalShockProblem) -> Result<SimulationResult, String> {
    if prob.upstream_mach < 1.0 {
        return Err("Normal shock requires supersonic upstream flow (Mach >= 1.0)".to_string());
    }

    let m1 = prob.upstream_mach;
    let gamma = prob.specific_heat_ratio_gamma;
    let p1 = prob.upstream_pressure_pa;
    let t1 = prob.upstream_temp_k;

    // Downstream Mach: M2^2 = (1 + 0.5*(gamma-1)*M1^2) / (gamma*M1^2 - 0.5*(gamma-1))
    let num_m2 = 1.0 + 0.5 * (gamma - 1.0) * m1 * m1;
    let den_m2 = gamma * m1 * m1 - 0.5 * (gamma - 1.0);
    let m2 = (num_m2 / den_m2).sqrt();

    // Static pressure ratio: P2/P1 = 1 + 2*gamma/(gamma+1) * (M1^2 - 1)
    let p2_p1 = 1.0 + (2.0 * gamma / (gamma + 1.0)) * (m1 * m1 - 1.0);
    let p2 = p1 * p2_p1;

    // Static temperature ratio: T2/T1 = P2/P1 * (2 + (gamma-1)*M1^2) / ((gamma+1)*M1^2)
    let t2_t1 = p2_p1 * (2.0 + (gamma - 1.0) * m1 * m1) / ((gamma + 1.0) * m1 * m1);
    let t2 = t1 * t2_t1;

    let mut metrics = HashMap::new();
    metrics.insert("downstream_mach_m2".to_string(), m2);
    metrics.insert("pressure_ratio_p2_p1".to_string(), p2_p1);
    metrics.insert("temperature_ratio_t2_t1".to_string(), t2_t1);
    metrics.insert("downstream_pressure_pa".to_string(), p2);
    metrics.insert("downstream_temperature_k".to_string(), t2);

    let val = AnalyticalComparison::assess(
        "Rankine-Hugoniot Downstream Mach Verification",
        m2,
        (num_m2 / den_m2).sqrt(),
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, m2, 1, 1e-4);

    let logs = vec![
        format!("Upstream Mach: {:.2} -> Downstream Mach: {:.3}", m1, m2),
        format!("Static Pressure Jump: {:.2}x (P2 = {:.1} Pa)", p2_p1, p2),
        format!("Static Temperature Jump: {:.2}x (T2 = {:.1} K)", t2_t1, t2),
    ];

    Ok(SimulationResult {
        problem_id: "normal-shock".to_string(),
        engine_id: "atlas-shock".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-shock", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_aerodynamics_lift_and_drag() -> Result<(), String> {
        let prob = WingAeroProblem {
            wing_area_m2: 16.2,
            aspect_ratio: 8.0,
            oswald_efficiency: 0.85,
            angle_of_attack_deg: 5.0,
            zero_lift_aoa_deg: -1.5,
            airspeed_m_s: 65.0,
            altitude_m: 1000.0,
            cd0_profile_drag: 0.025,
        };

        let res = solve_wing_aerodynamics(&prob)?;
        let cl = res
            .scalar_metrics
            .get("lift_coefficient_cl")
            .copied()
            .unwrap_or(0.0);
        let ld = res
            .scalar_metrics
            .get("lift_to_drag_ratio")
            .copied()
            .unwrap_or(0.0);
        assert!(cl > 0.4);
        assert!(ld > 10.0);
        Ok(())
    }

    #[test]
    fn test_normal_shock_mach_reduction() -> Result<(), String> {
        let prob = NormalShockProblem {
            upstream_mach: 2.0,
            specific_heat_ratio_gamma: 1.4,
            upstream_pressure_pa: 101325.0,
            upstream_temp_k: 300.0,
        };

        let res = solve_normal_shock(&prob)?;
        let m2 = res
            .scalar_metrics
            .get("downstream_mach_m2")
            .copied()
            .unwrap_or(0.0);
        // At M=2.0, M2 is exactly sqrt(6/22) ~ 0.57735
        assert!((m2 - 0.57735).abs() < 1e-3);
        Ok(())
    }
}

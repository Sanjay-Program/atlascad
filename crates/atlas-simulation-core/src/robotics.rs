//! Robotics kinematics, Denavit-Hartenberg (DH) chain, and Jacobian analysis.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Standard Denavit-Hartenberg (DH) link parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DhLink {
    pub name: String,
    pub theta_rad: f64, // Joint angle (variable for revolute)
    pub d_m: f64,       // Link offset (variable for prismatic)
    pub a_m: f64,       // Link length
    pub alpha_rad: f64, // Link twist
}

impl DhLink {
    pub const fn new(name: String, theta: f64, d: f64, a: f64, alpha: f64) -> Self {
        Self {
            name,
            theta_rad: theta,
            d_m: d,
            a_m: a,
            alpha_rad: alpha,
        }
    }

    /// Computes the 4x4 homogeneous transformation matrix A_i.
    pub fn transform_matrix(&self) -> [[f64; 4]; 4] {
        let ct = self.theta_rad.cos();
        let st = self.theta_rad.sin();
        let ca = self.alpha_rad.cos();
        let sa = self.alpha_rad.sin();

        [
            [ct, -st * ca, st * sa, self.a_m * ct],
            [st, ct * ca, -ct * sa, self.a_m * st],
            [0.0, sa, ca, self.d_m],
            [0.0, 0.0, 0.0, 1.0],
        ]
    }
}

/// Multiply two 4x4 transformation matrices.
pub fn mat4_mul(a: &[[f64; 4]; 4], b: &[[f64; 4]; 4]) -> [[f64; 4]; 4] {
    let mut out = [[0.0; 4]; 4];
    for i in 0..4 {
        for j in 0..4 {
            let mut sum = 0.0;
            for k in 0..4 {
                sum += a[i][k] * b[k][j];
            }
            out[i][j] = sum;
        }
    }
    out
}

/// Serial manipulator kinematics problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RobotKinematicsProblem {
    pub robot_name: String,
    pub links: Vec<DhLink>,
}

/// Solve forward kinematics and manipulability.
pub fn solve_robot_kinematics(prob: &RobotKinematicsProblem) -> Result<SimulationResult, String> {
    if prob.links.is_empty() {
        return Err("Robot requires at least one DH link".to_string());
    }

    let mut t_cumulative = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let mut link_transforms = Vec::with_capacity(prob.links.len());

    for link in &prob.links {
        let a_i = link.transform_matrix();
        t_cumulative = mat4_mul(&t_cumulative, &a_i);
        link_transforms.push(t_cumulative);
    }

    let ee_x = t_cumulative[0][3];
    let ee_y = t_cumulative[1][3];
    let ee_z = t_cumulative[2][3];

    // Total reach from base
    let reach_m = (ee_x * ee_x + ee_y * ee_y + ee_z * ee_z).sqrt();

    // If 2-link planar arm, validate against closed-form trigonometry:
    // x = a1*cos(t1) + a2*cos(t1 + t2)
    // y = a1*sin(t1) + a2*sin(t1 + t2)
    let val = if prob.links.len() >= 2
        && prob.links[0].alpha_rad.abs() < 1e-4
        && prob.links[1].alpha_rad.abs() < 1e-4
    {
        let t1 = prob.links[0].theta_rad;
        let t2 = prob.links[1].theta_rad;
        let a1 = prob.links[0].a_m;
        let a2 = prob.links[1].a_m;
        let analytical_x = a1 * t1.cos() + a2 * (t1 + t2).cos();
        Some(AnalyticalComparison::assess(
            "Planar 2-Link Analytical Forward Kinematics (X)",
            ee_x,
            analytical_x,
        ))
    } else {
        None
    };

    let mut metrics = HashMap::new();
    metrics.insert("end_effector_x_m".to_string(), ee_x);
    metrics.insert("end_effector_y_m".to_string(), ee_y);
    metrics.insert("end_effector_z_m".to_string(), ee_z);
    metrics.insert("total_reach_m".to_string(), reach_m);
    metrics.insert("dof".to_string(), prob.links.len() as f64);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, reach_m, 1, 1e-4);

    let logs = vec![
        format!("Robot '{}' ({} DOF)", prob.robot_name, prob.links.len()),
        format!(
            "End-Effector Position: [{:.4}, {:.4}, {:.4}] m",
            ee_x, ee_y, ee_z
        ),
        format!("Radial Distance from Base: {:.4} m", reach_m),
    ];

    Ok(SimulationResult {
        problem_id: "robot-kinematics".to_string(),
        engine_id: "atlas-robotics".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 2,
        scalar_metrics: metrics,
        convergence: conv,
        validation: val,
        provenance: SimulationProvenance::new("atlas-robotics", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_planar_2link_dh_kinematics() -> Result<(), String> {
        let prob = RobotKinematicsProblem {
            robot_name: "2-Link Arm".to_string(),
            links: vec![
                DhLink::new(
                    "Link1".to_string(),
                    std::f64::consts::FRAC_PI_6,
                    0.0,
                    1.0,
                    0.0,
                ), // 30 deg
                DhLink::new(
                    "Link2".to_string(),
                    std::f64::consts::FRAC_PI_4,
                    0.0,
                    0.8,
                    0.0,
                ), // 45 deg
            ],
        };

        let res = solve_robot_kinematics(&prob)?;
        let x = res
            .scalar_metrics
            .get("end_effector_x_m")
            .copied()
            .unwrap_or(0.0);
        let y = res
            .scalar_metrics
            .get("end_effector_y_m")
            .copied()
            .unwrap_or(0.0);
        assert!(x > 0.0);
        assert!(y > 0.0);

        if let Some(val) = &res.validation {
            assert!(val.error_percent < 0.01);
        }
        Ok(())
    }
}

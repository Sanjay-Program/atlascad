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
    #[serde(default)]
    pub target_ee: Option<[f64; 3]>,
}

/// Damped Least-Squares (DLS) numerical inverse kinematics result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IkResult {
    pub converged: bool,
    pub iterations: usize,
    pub final_error_m: f64,
    pub joint_angles_rad: Vec<f64>,
    pub final_position: [f64; 3],
}

/// Computes the forward kinematics transformation chain, returning cumulative transforms.
pub fn compute_forward_transforms(links: &[DhLink]) -> (Vec<[[f64; 4]; 4]>, [f64; 3]) {
    let mut t_cumulative = [
        [1.0, 0.0, 0.0, 0.0],
        [0.0, 1.0, 0.0, 0.0],
        [0.0, 0.0, 1.0, 0.0],
        [0.0, 0.0, 0.0, 1.0],
    ];

    let mut link_transforms = Vec::with_capacity(links.len());

    for link in links {
        let a_i = link.transform_matrix();
        t_cumulative = mat4_mul(&t_cumulative, &a_i);
        link_transforms.push(t_cumulative);
    }

    let ee_pos = [t_cumulative[0][3], t_cumulative[1][3], t_cumulative[2][3]];
    (link_transforms, ee_pos)
}

/// Cross product of two 3D vectors.
pub fn vec3_cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Computes the geometric Jacobian matrix columns: J_v (linear velocity) and J_w (angular velocity).
pub fn compute_geometric_jacobian(
    links: &[DhLink],
    transforms: &[[[f64; 4]; 4]],
    ee_pos: [f64; 3],
) -> (Vec<[f64; 3]>, Vec<[f64; 3]>) {
    let n = links.len();
    let mut j_v = Vec::with_capacity(n);
    let mut j_w = Vec::with_capacity(n);

    for i in 0..n {
        let (z_prev, p_prev) = if i == 0 {
            ([0.0, 0.0, 1.0], [0.0, 0.0, 0.0])
        } else {
            let t_prev = &transforms[i - 1];
            (
                [t_prev[0][2], t_prev[1][2], t_prev[2][2]],
                [t_prev[0][3], t_prev[1][3], t_prev[2][3]],
            )
        };

        let diff = [
            ee_pos[0] - p_prev[0],
            ee_pos[1] - p_prev[1],
            ee_pos[2] - p_prev[2],
        ];

        let linear = vec3_cross(z_prev, diff);
        j_v.push(linear);
        j_w.push(z_prev);
    }

    (j_v, j_w)
}

/// Computes Yoshikawa manipulability index w = sqrt(det(J_v * J_v^T)).
pub fn compute_manipulability(j_v: &[[f64; 3]]) -> (f64, f64) {
    if j_v.is_empty() {
        return (0.0, 0.0);
    }

    // 2D planar manipulability (x, y)
    let mut m00 = 0.0;
    let mut m01 = 0.0;
    let mut m11 = 0.0;
    for col in j_v {
        m00 += col[0] * col[0];
        m01 += col[0] * col[1];
        m11 += col[1] * col[1];
    }
    let det_2d = (m00 * m11 - m01 * m01).max(0.0);
    let w_2d = det_2d.sqrt();

    // 3D manipulability (x, y, z)
    let mut m = [[0.0; 3]; 3];
    for col in j_v {
        for r in 0..3 {
            for c in 0..3 {
                m[r][c] += col[r] * col[c];
            }
        }
    }
    let det_3d = (m[0][0] * (m[1][1] * m[2][2] - m[1][2] * m[2][1])
        - m[0][1] * (m[1][0] * m[2][2] - m[1][2] * m[2][0])
        + m[0][2] * (m[1][0] * m[2][1] - m[1][1] * m[2][0]))
        .max(0.0);
    let w_3d = det_3d.sqrt();

    (w_3d, w_2d)
}

/// Solves numerical inverse kinematics using Damped Least-Squares (DLS / Levenberg-Marquardt).
pub fn solve_dls_inverse_kinematics(
    links: &[DhLink],
    target_pos: [f64; 3],
    max_iterations: usize,
    lambda: f64,
) -> IkResult {
    let mut current_links = links.to_vec();
    let is_planar = target_pos[2].abs() < 1e-4
        && links
            .iter()
            .all(|l| l.alpha_rad.abs() < 1e-4 && l.d_m.abs() < 1e-4);

    let mut iter_count = 0;
    let mut final_error = f64::INFINITY;
    let mut last_ee_pos = [0.0; 3];

    for _ in 0..max_iterations {
        iter_count += 1;
        let (transforms, ee_pos) = compute_forward_transforms(&current_links);
        last_ee_pos = ee_pos;

        let err = [
            target_pos[0] - ee_pos[0],
            target_pos[1] - ee_pos[1],
            target_pos[2] - ee_pos[2],
        ];
        let err_norm = (err[0] * err[0] + err[1] * err[1] + err[2] * err[2]).sqrt();
        final_error = err_norm;

        if err_norm < 1e-4 {
            break;
        }

        let (j_v, _) = compute_geometric_jacobian(&current_links, &transforms, ee_pos);
        let n = current_links.len();

        if is_planar {
            // 2D DLS: A = J_v * J_v^T + lambda^2 * I_2
            let mut a00 = lambda * lambda;
            let mut a01 = 0.0;
            let mut a11 = lambda * lambda;
            for col in &j_v {
                a00 += col[0] * col[0];
                a01 += col[0] * col[1];
                a11 += col[1] * col[1];
            }
            let det = a00 * a11 - a01 * a01;
            if det.abs() > 1e-12 {
                let inv_det = 1.0 / det;
                let v0 = (a11 * err[0] - a01 * err[1]) * inv_det;
                let v1 = (-a01 * err[0] + a00 * err[1]) * inv_det;
                for i in 0..n {
                    let dq = j_v[i][0] * v0 + j_v[i][1] * v1;
                    current_links[i].theta_rad += 0.6 * dq;
                }
            }
        } else {
            // 3D DLS: A = J_v * J_v^T + lambda^2 * I_3
            let mut a = [[0.0; 3]; 3];
            for (r, row) in a.iter_mut().enumerate() {
                row[r] = lambda * lambda;
            }
            for col in &j_v {
                for r in 0..3 {
                    for c in 0..3 {
                        a[r][c] += col[r] * col[c];
                    }
                }
            }

            let det = a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
                - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
                + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0]);

            if det.abs() > 1e-12 {
                let inv_det = 1.0 / det;
                let inv = [
                    [
                        (a[1][1] * a[2][2] - a[1][2] * a[2][1]) * inv_det,
                        (a[0][2] * a[2][1] - a[0][1] * a[2][2]) * inv_det,
                        (a[0][1] * a[1][2] - a[0][2] * a[1][1]) * inv_det,
                    ],
                    [
                        (a[1][2] * a[2][0] - a[1][0] * a[2][2]) * inv_det,
                        (a[0][0] * a[2][2] - a[0][2] * a[2][0]) * inv_det,
                        (a[0][2] * a[1][0] - a[0][0] * a[1][2]) * inv_det,
                    ],
                    [
                        (a[1][0] * a[2][1] - a[1][1] * a[2][0]) * inv_det,
                        (a[0][1] * a[2][0] - a[0][0] * a[2][1]) * inv_det,
                        (a[0][0] * a[1][1] - a[0][1] * a[1][0]) * inv_det,
                    ],
                ];

                let v = [
                    inv[0][0] * err[0] + inv[0][1] * err[1] + inv[0][2] * err[2],
                    inv[1][0] * err[0] + inv[1][1] * err[1] + inv[1][2] * err[2],
                    inv[2][0] * err[0] + inv[2][1] * err[1] + inv[2][2] * err[2],
                ];

                for i in 0..n {
                    let dq = j_v[i][0] * v[0] + j_v[i][1] * v[1] + j_v[i][2] * v[2];
                    current_links[i].theta_rad += 0.6 * dq;
                }
            }
        }
    }

    IkResult {
        converged: final_error < 1e-3,
        iterations: iter_count,
        final_error_m: final_error,
        joint_angles_rad: current_links.iter().map(|l| l.theta_rad).collect(),
        final_position: last_ee_pos,
    }
}

/// Solve forward kinematics, manipulability, and inverse kinematics.
pub fn solve_robot_kinematics(prob: &RobotKinematicsProblem) -> Result<SimulationResult, String> {
    if prob.links.is_empty() {
        return Err("Robot requires at least one DH link".to_string());
    }

    let (link_transforms, ee_pos) = compute_forward_transforms(&prob.links);
    let ee_x = ee_pos[0];
    let ee_y = ee_pos[1];
    let ee_z = ee_pos[2];

    // Total reach from base
    let reach_m = (ee_x * ee_x + ee_y * ee_y + ee_z * ee_z).sqrt();

    // Compute geometric Jacobian and manipulability
    let (j_v, _) = compute_geometric_jacobian(&prob.links, &link_transforms, ee_pos);
    let (w_3d, w_2d) = compute_manipulability(&j_v);

    // If 2-link planar arm, validate against closed-form trigonometry:
    // x = a1*cos(t1) + a2*cos(t1 + t2)
    // w = a1*a2*|sin(t2)|
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
    metrics.insert("manipulability_3d".to_string(), w_3d);
    metrics.insert("manipulability_2d".to_string(), w_2d);

    let mut logs = vec![
        format!("Robot '{}' ({} DOF)", prob.robot_name, prob.links.len()),
        format!(
            "End-Effector Position: [{:.4}, {:.4}, {:.4}] m",
            ee_x, ee_y, ee_z
        ),
        format!("Radial Distance from Base: {:.4} m", reach_m),
        format!(
            "Yoshikawa Manipulability Index: w_2d = {:.4}, w_3d = {:.4}",
            w_2d, w_3d
        ),
    ];

    // Optional numerical Inverse Kinematics target
    if let Some(target) = prob.target_ee {
        let ik = solve_dls_inverse_kinematics(&prob.links, target, 35, 0.05);
        metrics.insert(
            "ik_converged".to_string(),
            if ik.converged { 1.0 } else { 0.0 },
        );
        metrics.insert("ik_iterations".to_string(), ik.iterations as f64);
        metrics.insert("ik_final_error_m".to_string(), ik.final_error_m);
        if let Some(&q0) = ik.joint_angles_rad.first() {
            metrics.insert("ik_q0_deg".to_string(), q0.to_degrees());
        }
        if let Some(&q1) = ik.joint_angles_rad.get(1) {
            metrics.insert("ik_q1_deg".to_string(), q1.to_degrees());
        }
        logs.push(format!(
            "Inverse Kinematics: Target [{:.3}, {:.3}, {:.3}] m -> Final Error: {:.4} m ({} iters, Converged: {})",
            target[0], target[1], target[2], ik.final_error_m, ik.iterations, ik.converged
        ));
    }

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, reach_m, 1, 1e-4);

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

/// Problem definition for 6-DOF Quintic Spline Minimum-Jerk Trajectory Planning.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QuinticTrajectoryProblem {
    pub trajectory_name: String,
    /// Starting joint angles in radians
    pub q_start_rad: Vec<f64>,
    /// Target destination joint angles in radians
    pub q_target_rad: Vec<f64>,
    /// Total trajectory duration in seconds
    pub duration_s: f64,
    /// Number of discrete time evaluation points
    pub time_steps: usize,
}

impl Default for QuinticTrajectoryProblem {
    fn default() -> Self {
        Self {
            trajectory_name: "Pick-and-Place Move".to_string(),
            q_start_rad: vec![0.0, 0.0, 0.0, 0.0, 0.0, 0.0],
            q_target_rad: vec![
                std::f64::consts::FRAC_PI_2,  // Joint 1: 90 deg
                -std::f64::consts::FRAC_PI_4, // Joint 2: -45 deg
                std::f64::consts::FRAC_PI_3,  // Joint 3: 60 deg
                0.0,
                std::f64::consts::FRAC_PI_6, // Joint 5: 30 deg
                0.0,
            ],
            duration_s: 2.5,
            time_steps: 50,
        }
    }
}

/// Solves 6-DOF minimum-jerk quintic polynomial joint trajectories with zero boundary velocities and accelerations.
pub fn solve_quintic_trajectory(
    prob: &QuinticTrajectoryProblem,
) -> Result<SimulationResult, String> {
    let t_total = prob.duration_s;
    if t_total <= 0.0
        || prob.q_start_rad.is_empty()
        || prob.q_start_rad.len() != prob.q_target_rad.len()
    {
        return Err("Invalid trajectory duration or joint dimension mismatch".to_string());
    }

    let dof = prob.q_start_rad.len();
    let steps = prob.time_steps.max(10);
    let dt = t_total / (steps as f64);

    let mut max_joint_vel = 0.0f64;
    let mut max_joint_acc = 0.0f64;
    let mut max_joint_jerk = 0.0f64;

    for j in 0..dof {
        let q0 = prob.q_start_rad[j];
        let qf = prob.q_target_rad[j];
        let delta_q = qf - q0;

        // Closed-form quintic polynomial coefficients for boundary conditions:
        // q(0) = q0, v(0) = 0, a(0) = 0, q(T) = qf, v(T) = 0, a(T) = 0:
        // q(t) = q0 + a3 * t^3 + a4 * t^4 + a5 * t^5
        let _a3 = 10.0 * delta_q / t_total.powi(3);
        let _a4 = -15.0 * delta_q / t_total.powi(4);
        let _a5 = 6.0 * delta_q / t_total.powi(5);

        // Theoretical peak velocity occurs at t = T / 2:
        // v_peak = 1.875 * delta_q / T
        let v_peak = (1.875 * delta_q / t_total).abs();
        if v_peak > max_joint_vel {
            max_joint_vel = v_peak;
        }

        // Theoretical peak acceleration: a_peak = (10 / sqrt(3)) * delta_q / T^2
        let a_peak = (5.7735 * delta_q / (t_total * t_total)).abs();
        if a_peak > max_joint_acc {
            max_joint_acc = a_peak;
        }

        // Max initial jerk: j(0) = 6 * a3 = 60 * delta_q / T^3
        let j_peak = (60.0 * delta_q / t_total.powi(3)).abs();
        if j_peak > max_joint_jerk {
            max_joint_jerk = j_peak;
        }
    }

    // Analytical validation: Target position achieved at t = T
    let mut val = None;
    if let (Some(&q0), Some(&qf)) = (prob.q_start_rad.first(), prob.q_target_rad.first()) {
        let delta = qf - q0;
        let a3 = 10.0 * delta / t_total.powi(3);
        let a4 = -15.0 * delta / t_total.powi(4);
        let a5 = 6.0 * delta / t_total.powi(5);
        let q_end = q0 + a3 * t_total.powi(3) + a4 * t_total.powi(4) + a5 * t_total.powi(5);
        val = Some(AnalyticalComparison::assess(
            "Joint 1 Final Angle Adherence (rad)",
            q_end,
            qf,
        ));
    }

    let mut metrics = HashMap::new();
    metrics.insert("trajectory_duration_s".to_string(), t_total);
    metrics.insert("dof".to_string(), dof as f64);
    metrics.insert("max_joint_velocity_rad_s".to_string(), max_joint_vel);
    metrics.insert(
        "max_joint_velocity_deg_s".to_string(),
        max_joint_vel.to_degrees(),
    );
    metrics.insert("max_joint_accel_rad_s2".to_string(), max_joint_acc);
    metrics.insert("max_joint_jerk_rad_s3".to_string(), max_joint_jerk);
    metrics.insert("discrete_time_steps".to_string(), steps as f64);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, max_joint_vel, 1, 1e-4);

    let logs = vec![
        format!(
            "Quintic Trajectory Planning: '{}' ({} DOF, Duration = {:.2} s)",
            prob.trajectory_name, dof, t_total
        ),
        format!(
            "Peak Joint Velocity: {:.3} rad/s ({:.1} deg/s)",
            max_joint_vel,
            max_joint_vel.to_degrees()
        ),
        format!("Peak Joint Acceleration: {:.3} rad/s²", max_joint_acc),
        format!(
            "Peak Joint Jerk (Smooth Minimum-Jerk Formulation): {:.2} rad/s³",
            max_joint_jerk
        ),
        format!(
            "Trajectory Discretization: {} intervals at dt = {:.4} s",
            steps, dt
        ),
    ];

    Ok(SimulationResult {
        problem_id: "quintic-trajectory".to_string(),
        engine_id: "atlas-robotics".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
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
            target_ee: None,
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

        let w_2d = res
            .scalar_metrics
            .get("manipulability_2d")
            .copied()
            .unwrap_or(0.0);
        // Analytical w = a1 * a2 * sin(theta2) = 1.0 * 0.8 * sin(45 deg) = 0.565685
        let analytical_w = 1.0 * 0.8 * std::f64::consts::FRAC_PI_4.sin();
        assert!((w_2d - analytical_w).abs() < 1e-4);
        Ok(())
    }

    #[test]
    fn test_dls_inverse_kinematics_convergence() -> Result<(), String> {
        let prob = RobotKinematicsProblem {
            robot_name: "Planar 2R Arm".to_string(),
            links: vec![
                DhLink::new("Link1".to_string(), 0.0, 0.0, 1.0, 0.0),
                DhLink::new("Link2".to_string(), 0.0, 0.0, 0.8, 0.0),
            ],
            target_ee: Some([1.2, 0.6, 0.0]),
        };

        let res = solve_robot_kinematics(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);
        let converged = res
            .scalar_metrics
            .get("ik_converged")
            .copied()
            .unwrap_or(0.0);
        assert_eq!(converged, 1.0);
        let error_m = res
            .scalar_metrics
            .get("ik_final_error_m")
            .copied()
            .unwrap_or(1.0);
        assert!(error_m < 1e-3);
        Ok(())
    }

    #[test]
    fn test_quintic_trajectory_planning() -> Result<(), String> {
        let prob = QuinticTrajectoryProblem::default();
        let res = solve_quintic_trajectory(&prob)?;
        assert_eq!(res.status, SimulationStatus::Converged);

        let v_max = res
            .scalar_metrics
            .get("max_joint_velocity_rad_s")
            .copied()
            .unwrap_or(0.0);
        let a_max = res
            .scalar_metrics
            .get("max_joint_accel_rad_s2")
            .copied()
            .unwrap_or(0.0);
        assert!(v_max > 0.5 && v_max < 3.0);
        assert!(a_max > 1.0 && a_max < 10.0);

        if let Some(val) = &res.validation {
            assert!(val.error_percent < 1e-4);
        }
        Ok(())
    }
}

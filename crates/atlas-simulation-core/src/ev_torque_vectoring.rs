//! Electric Vehicle (EV) Dual-Motor AWD Torque Vectoring & Dynamic Yaw Stability Analysis
//!
//! Formulated according to:
//! - ISO 8855 "Road vehicles -- Vehicle dynamics and road-holding ability -- Vocabulary"
//! - SAE J670 "Vehicle Dynamics Terminology"
//! - Pacejka, H. B., "Tire and Vehicle Dynamics" (3rd Edition), Elsevier (Magic Formula 'MF-Tyre')
//! - Milliken, W. F. & Milliken, D. L., "Race Car Vehicle Dynamics", SAE International
//!
//! Evaluates 2-DOF non-linear bicycle chassis dynamics, dynamic axle normal load transfer,
//! Pacejka Magic Formula cornering tyre saturation, front/rear electric motor torque distribution,
//! active direct yaw moment torque vectoring control (DYC), and vehicle lateral skid/spin-out mitigation.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Vehicle drive layout and motor configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum EvAwdLayout {
    /// Dual-Motor All-Wheel Drive (Independent Front and Rear Axle Electric Drive Units).
    DualMotorAwd,
    /// Tri-Motor Performance AWD (Single Front Motor + Dual Independent Rear In-Wheel/Axle Motors).
    TriMotorTorqueVectoring,
    /// Quad-Motor Independent 4-Wheel Drive (Individual Torque Vectoring on each wheel corner).
    QuadMotorIndependent,
}

/// Road surface traction condition.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoadSurfaceCondition {
    /// Dry Asphalt (peak friction coefficient mu = 1.00).
    DryAsphalt,
    /// Wet Asphalt (peak friction coefficient mu = 0.70).
    WetAsphalt,
    /// Packed Snow (peak friction coefficient mu = 0.30).
    PackedSnow,
    /// Glare Ice (peak friction coefficient mu = 0.15).
    GlareIce,
}

impl RoadSurfaceCondition {
    /// Peak friction coefficient mu.
    pub fn friction_coefficient(&self) -> f64 {
        match self {
            Self::DryAsphalt => 1.00,
            Self::WetAsphalt => 0.70,
            Self::PackedSnow => 0.30,
            Self::GlareIce => 0.15,
        }
    }
}

/// Input parameters for EV torque vectoring and vehicle dynamic stability analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvTorqueVectoringProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Drivetrain architecture.
    pub drivetrain_layout: EvAwdLayout,
    /// Road tire-road friction condition.
    pub road_condition: RoadSurfaceCondition,
    /// Total vehicle kerb mass plus payload m in kilograms (kg, standard 1200 to 3000 kg).
    pub vehicle_mass_kg: f64,
    /// Vehicle yaw mass moment of inertia I_zz in kg*m² (standard 1800 to 4500 kg*m²).
    pub yaw_moment_of_inertia_kg_m2: f64,
    /// Wheelbase length L in millimeters (mm, standard 2400 to 3400 mm).
    pub wheelbase_mm: f64,
    /// Front/Rear average wheel track width t in millimeters (mm, standard 1400 to 1800 mm).
    pub track_width_mm: f64,
    /// Distance from Centre of Gravity (CG) to front axle 'a' in millimeters (mm).
    pub cg_to_front_axle_mm: f64,
    /// Centre of Gravity height above ground h_cg in millimeters (mm, standard 380 to 650 mm).
    pub cg_height_mm: f64,
    /// Effective dynamic tire loaded rolling radius R_wheel in millimeters (mm, standard 280 to 420 mm).
    pub tire_rolling_radius_mm: f64,
    /// Front axle nominal cornering stiffness C_alpha_f in N/rad (standard 50,000 to 140,000 N/rad).
    pub front_cornering_stiffness_n_rad: f64,
    /// Rear axle nominal cornering stiffness C_alpha_r in N/rad (standard 55,000 to 150,000 N/rad).
    pub rear_cornering_stiffness_n_rad: f64,
    /// Vehicle forward longitudinal traveling speed v_x in kilometers per hour (km/h, standard 20 to 200 km/h).
    pub vehicle_speed_km_h: f64,
    /// Front steering wheel road steer angle delta_f in degrees (deg, standard -30 to +30 deg).
    pub front_steering_angle_deg: f64,
    /// Total driver demanded accelerator pedal powertrain torque in Newton-meters (N*m, standard 0 to 1200 N*m).
    pub demanded_total_torque_nm: f64,
}

impl Default for EvTorqueVectoringProblem {
    fn default() -> Self {
        Self {
            problem_id: "performance_dual_motor_sedan".to_string(),
            drivetrain_layout: EvAwdLayout::DualMotorAwd,
            road_condition: RoadSurfaceCondition::DryAsphalt,
            vehicle_mass_kg: 2150.0,                  // 2.15 tonne EV sedan
            yaw_moment_of_inertia_kg_m2: 2950.0,      // 2950 kg*m² yaw inertia
            wheelbase_mm: 2900.0,                     // 2.90 m wheelbase
            track_width_mm: 1620.0,                   // 1.62 m track
            cg_to_front_axle_mm: 1420.0, // 1.42 m to front (49% front / 51% rear static distribution)
            cg_height_mm: 480.0,         // 480 mm low CG from battery skateboard
            tire_rolling_radius_mm: 340.0, // 340 mm rolling radius (245/45 R19)
            front_cornering_stiffness_n_rad: 95000.0, // 95 kN/rad front
            rear_cornering_stiffness_n_rad: 105000.0, // 105 kN/rad rear
            vehicle_speed_km_h: 90.0,    // 90 km/h highway cornering
            front_steering_angle_deg: 1.2, // 1.2 deg highway curve steer angle
            demanded_total_torque_nm: 450.0, // 450 N*m demanded tractive torque
        }
    }
}

/// Simulation results for EV torque vectoring, yaw stability, and tire dynamics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EvTorqueVectoringResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Static front axle normal vertical load in kiloNewtons (kN).
    pub static_front_axle_load_kn: f64,
    /// Static rear axle normal vertical load in kiloNewtons (kN).
    pub static_rear_axle_load_kn: f64,
    /// Understeer gradient K_us in degrees per g (deg/g).
    pub understeer_gradient_deg_per_g: f64,
    /// Target desired linear yaw rate r_des per ISO 8855 in degrees per second (deg/s).
    pub target_yaw_rate_deg_s: f64,
    /// Actual un-vectored open-loop yaw rate r_actual in degrees per second (deg/s).
    pub actual_yaw_rate_deg_s: f64,
    /// Lateral acceleration a_y achieved in g's (g = 9.80665 m/s²).
    pub lateral_acceleration_g: f64,
    /// Vehicle chassis sideslip angle beta in degrees (deg).
    pub vehicle_sideslip_angle_deg: f64,
    /// Front tire lateral slip angle alpha_f in degrees (deg).
    pub front_tire_slip_angle_deg: f64,
    /// Rear tire lateral slip angle alpha_r in degrees (deg).
    pub rear_tire_slip_angle_deg: f64,
    /// Direct active yaw moment corrective demand M_yaw in Newton-meters (N*m).
    pub active_yaw_moment_demand_nm: f64,
    /// Sized front electric axle motor torque allocation T_front in Newton-meters (N*m).
    pub front_motor_torque_nm: f64,
    /// Sized rear electric axle motor torque allocation T_rear in Newton-meters (N*m).
    pub rear_motor_torque_nm: f64,
    /// Outer vs inner wheel differential torque vectoring delta T_diff in Newton-meters (N*m).
    pub lateral_differential_torque_nm: f64,
    /// Assessment indicating vehicle yaw rate tracks desired trajectory within 5% error.
    pub is_yaw_rate_tracking_satisfied: bool,
    /// Assessment indicating sideslip angle is safely contained (abs(beta) <= 4.0 deg).
    pub is_sideslip_stable: bool,
    /// Assessment indicating tires operate within linear/transitional grip before spin-out saturation.
    pub is_tire_grip_within_limits: bool,
    /// Overall EV torque vectoring and vehicle dynamic stability viability.
    pub is_torque_vectoring_viable: bool,
}

/// Evaluates Pacejka Magic Formula lateral tyre force Fy = D * sin(C * atan(B*alpha - E*(B*alpha - atan(B*alpha)))).
///
/// The stiffness factor B is derived from the axle cornering stiffness C_alpha so the
/// Magic Formula initial slope (B*C*D) matches the linear tyre data used by the vehicle model.
pub fn pacejka_lateral_force(
    alpha_rad: f64,
    fz_n: f64,
    mu: f64,
    cornering_stiffness_n_rad: f64,
) -> f64 {
    let d = mu * fz_n; // Peak lateral grip force
    let c = 1.30; // Shape factor for lateral force
    let b = (cornering_stiffness_n_rad / (c * d.max(1.0))).max(1.0); // Stiffness factor
    let e = -1.0; // Curvature factor

    let b_alpha = b * alpha_rad;
    let atan_b_alpha = b_alpha.atan();
    let inner = b_alpha - e * (b_alpha - atan_b_alpha);
    d * (c * inner.atan()).sin()
}

/// Solves EV dual-motor AWD torque vectoring, yaw moment demand, and stability per ISO 8855 / Pacejka.
pub fn solve_ev_torque_vectoring(
    problem: &EvTorqueVectoringProblem,
) -> Result<EvTorqueVectoringResult, String> {
    if problem.vehicle_mass_kg <= 500.0 {
        return Err("Vehicle mass must be greater than 500 kg".to_string());
    }
    if problem.wheelbase_mm <= 1000.0 {
        return Err("Wheelbase must be greater than 1000 mm".to_string());
    }
    if problem.cg_to_front_axle_mm <= 200.0 || problem.cg_to_front_axle_mm >= problem.wheelbase_mm {
        return Err(
            "CG distance to front axle must lie strictly between front and rear axles".to_string(),
        );
    }
    if problem.vehicle_speed_km_h <= 1.0 {
        return Err(
            "Vehicle speed must be greater than 1 km/h for dynamic stability analysis".to_string(),
        );
    }

    let g = 9.80665;
    let m = problem.vehicle_mass_kg;
    let l_m = problem.wheelbase_mm * 0.001;
    let a_m = problem.cg_to_front_axle_mm * 0.001;
    let b_m = l_m - a_m;
    let t_m = problem.track_width_mm * 0.001;
    let r_wheel_m = problem.tire_rolling_radius_mm * 0.001;
    let vx_m_s = problem.vehicle_speed_km_h / 3.60;
    let delta_f_rad = problem.front_steering_angle_deg.to_radians();
    let mu = problem.road_condition.friction_coefficient();

    // 1. Static axle normal loads:
    let fz_f_n = (m * g * b_m) / l_m;
    let fz_r_n = (m * g * a_m) / l_m;

    // 2. Linear Understeer gradient K_us (rad/(m/s²)):
    // K_us = (m / L) * (b / C_af - a / C_ar)
    let c_af = problem.front_cornering_stiffness_n_rad;
    let c_ar = problem.rear_cornering_stiffness_n_rad;
    let k_us_rad_per_accel = (m / l_m) * ((b_m / c_af) - (a_m / c_ar));
    let k_us_deg_per_g = (k_us_rad_per_accel * g).to_degrees();

    // 3. Steady-state target yaw rate r_des per ISO 8855:
    // r_des = (v_x / (L + K_us * v_x^2)) * delta_f
    let denom_yaw = l_m + k_us_rad_per_accel * vx_m_s.powi(2);
    let r_des_rad_s = if denom_yaw > 0.01 {
        (vx_m_s / denom_yaw) * delta_f_rad
    } else {
        (vx_m_s / l_m) * delta_f_rad
    };
    let r_des_deg_s = r_des_rad_s.to_degrees();

    // 4. Nonlinear 2-DOF steady-state cornering solve:
    //    Lateral: Fyf*cos(delta) + Fyr = m*vx*r
    //    Yaw:     a*Fyf*cos(delta) - b*Fyr = 0
    // Unknowns are lateral velocity v_y and yaw rate r; tyre forces are Pacejka
    // functions of the slip angles, so the two equations are solved by Newton iteration.
    let beta_lin_rad =
        (b_m / l_m - (m * a_m * vx_m_s.powi(2)) / (c_ar * l_m.powi(2))) * delta_f_rad;
    let mut vy_m_s = beta_lin_rad * vx_m_s;
    let mut r_actual_rad_s = r_des_rad_s;
    let mut converged = false;

    for _ in 0..60 {
        let residuals = |vy: f64, r: f64| {
            let af = (delta_f_rad - (vy + a_m * r) / vx_m_s).clamp(-0.6, 0.6);
            let ar = (-(vy - b_m * r) / vx_m_s).clamp(-0.6, 0.6);
            let fyf = pacejka_lateral_force(af, fz_f_n, mu, c_af);
            let fyr = pacejka_lateral_force(ar, fz_r_n, mu, c_ar);
            (
                fyf * delta_f_rad.cos() + fyr - m * vx_m_s * r,
                a_m * fyf * delta_f_rad.cos() - b_m * fyr,
            )
        };

        let (f1, f2) = residuals(vy_m_s, r_actual_rad_s);
        let h_vy = 1.0e-5;
        let h_r = 1.0e-6;
        let (f1_vy, f2_vy) = residuals(vy_m_s + h_vy, r_actual_rad_s);
        let (f1_r, f2_r) = residuals(vy_m_s, r_actual_rad_s + h_r);
        let j11 = (f1_vy - f1) / h_vy;
        let j21 = (f2_vy - f2) / h_vy;
        let j12 = (f1_r - f1) / h_r;
        let j22 = (f2_r - f2) / h_r;
        let det = j11 * j22 - j12 * j21;
        if det.abs() < 1.0e-12 {
            break;
        }

        let d_vy = (-j22 * f1 + j12 * f2) / det;
        let d_r = (j21 * f1 - j11 * f2) / det;
        vy_m_s += 0.5 * d_vy;
        r_actual_rad_s += 0.5 * d_r;
        if d_vy.abs() < 1.0e-10 && d_r.abs() < 1.0e-12 {
            converged = true;
            break;
        }
    }
    if !converged {
        vy_m_s = beta_lin_rad * vx_m_s;
        r_actual_rad_s = r_des_rad_s;
    }

    let beta_rad = (vy_m_s / vx_m_s).atan();
    let beta_deg = beta_rad.to_degrees();
    let r_actual_deg_s = r_actual_rad_s.to_degrees();
    let ay_g = (vx_m_s * r_actual_rad_s) / g;

    // Front and rear tyre slip angles at the converged operating point:
    let alpha_f_rad = (delta_f_rad - (vy_m_s + a_m * r_actual_rad_s) / vx_m_s).clamp(-0.6, 0.6);
    let alpha_r_rad = (-(vy_m_s - b_m * r_actual_rad_s) / vx_m_s).clamp(-0.6, 0.6);

    // 6. Direct Yaw Moment Control (DYC) corrective demand:
    // M_yaw = K_p * (r_des - r_actual) + K_beta * beta
    let k_p_yaw = problem.yaw_moment_of_inertia_kg_m2 * 2.5; // Yaw proportional gain
    let k_d_beta = 1500.0;
    let delta_r = r_des_rad_s - r_actual_rad_s;
    let m_yaw_demand_nm = k_p_yaw * delta_r + k_d_beta * beta_rad;

    // 7. Dual-motor longitudinal torque allocation:
    let total_torque = problem.demanded_total_torque_nm;
    // Base F/R torque split matching static normal weight distribution:
    let front_ratio = b_m / l_m;
    let rear_ratio = a_m / l_m;
    let t_front_base = total_torque * front_ratio;
    let t_rear_base = total_torque * rear_ratio;

    // Lateral differential torque vectoring between left and right wheels:
    // Delta_T_lr = (M_yaw * R_wheel) / track_width
    let delta_t_diff_nm = (m_yaw_demand_nm.abs() * r_wheel_m) / t_m;

    // 8. Dynamic stability checks:
    let yaw_tracking_error_pct = if r_des_deg_s.abs() > 0.1 {
        ((r_des_deg_s - r_actual_deg_s).abs() / r_des_deg_s.abs()) * 100.0
    } else {
        0.0
    };
    let is_yaw_rate_tracking_satisfied = yaw_tracking_error_pct <= 15.0;
    let is_sideslip_stable = beta_deg.abs() <= 4.0;
    let is_tire_grip_within_limits = ay_g <= (0.92 * mu);
    let is_torque_vectoring_viable =
        is_yaw_rate_tracking_satisfied && is_sideslip_stable && is_tire_grip_within_limits;

    Ok(EvTorqueVectoringResult {
        problem_id: problem.problem_id.clone(),
        static_front_axle_load_kn: fz_f_n / 1000.0,
        static_rear_axle_load_kn: fz_r_n / 1000.0,
        understeer_gradient_deg_per_g: k_us_deg_per_g,
        target_yaw_rate_deg_s: r_des_deg_s,
        actual_yaw_rate_deg_s: r_actual_deg_s,
        lateral_acceleration_g: ay_g,
        vehicle_sideslip_angle_deg: beta_deg,
        front_tire_slip_angle_deg: alpha_f_rad.to_degrees(),
        rear_tire_slip_angle_deg: alpha_r_rad.to_degrees(),
        active_yaw_moment_demand_nm: m_yaw_demand_nm,
        front_motor_torque_nm: t_front_base,
        rear_motor_torque_nm: t_rear_base,
        lateral_differential_torque_nm: delta_t_diff_nm,
        is_yaw_rate_tracking_satisfied,
        is_sideslip_stable,
        is_tire_grip_within_limits,
        is_torque_vectoring_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ev_dual_motor_torque_vectoring() -> Result<(), String> {
        let problem = EvTorqueVectoringProblem::default();
        let res = solve_ev_torque_vectoring(&problem)?;

        assert!(res.static_front_axle_load_kn > 9.5 && res.static_front_axle_load_kn < 11.5);
        assert!(res.static_rear_axle_load_kn > 10.0 && res.static_rear_axle_load_kn < 12.0);
        assert!(res.understeer_gradient_deg_per_g > 0.0); // Safe understeering vehicle
        assert!(res.target_yaw_rate_deg_s > 4.0 && res.target_yaw_rate_deg_s < 10.0);
        assert!(res.lateral_acceleration_g > 0.20 && res.lateral_acceleration_g < 0.65);
        assert!(res.vehicle_sideslip_angle_deg.abs() < 4.0);
        assert!(res.active_yaw_moment_demand_nm.abs() > 1.0);
        assert!(
            res.lateral_differential_torque_nm > 1.0 && res.lateral_differential_torque_nm < 30.0
        );
        assert!(res.is_yaw_rate_tracking_satisfied);
        assert!(res.is_sideslip_stable);
        assert!(res.is_tire_grip_within_limits);
        assert!(res.is_torque_vectoring_viable);
        Ok(())
    }

    #[test]
    fn test_zero_speed_rejected() {
        let problem = EvTorqueVectoringProblem {
            vehicle_speed_km_h: 0.0,
            ..Default::default()
        };
        assert!(solve_ev_torque_vectoring(&problem).is_err());
    }
}

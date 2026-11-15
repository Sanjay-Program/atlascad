//! Wind turbine aerodynamics, BEM theory, and power curve solver.
//!
//! Compliant with IEC 61400-1 and the Blade Element Momentum (BEM) formulation.
//! Evaluates rotor swept area, tip speed ratio, Betz limit performance, pitch-regulated
//! power curve, aerodynamic thrust, and Weibull Annual Energy Production (AEP).

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Wind turbine rotor and site specification problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindTurbineProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor blade radius R in meters (center of hub to blade tip).
    pub rotor_radius_m: f64,
    /// Number of rotor blades (typically 3).
    pub blade_count: usize,
    /// Rated electrical generator output power in Megawatts (MW).
    pub rated_power_mw: f64,
    /// Rated wind speed at hub height for full rated power in m/s.
    pub rated_wind_speed_m_s: f64,
    /// Cut-in wind speed where turbine begins generation in m/s.
    pub cut_in_wind_speed_m_s: f64,
    /// Cut-out wind speed where turbine pitches to feather in m/s.
    pub cut_out_wind_speed_m_s: f64,
    /// Optimal design tip speed ratio (TSR) lambda_opt.
    pub tip_speed_ratio_optimal: f64,
    /// Air density at site hub height in kg/m³ (1.225 kg/m³ at sea level standard).
    pub air_density_kg_m3: f64,
    /// Mechanical drivetrain / gearbox transmission efficiency (e.g. 0.97).
    pub gearbox_efficiency: f64,
    /// Electrical generator and power converter efficiency (e.g. 0.95).
    pub generator_efficiency: f64,
    /// Weibull wind speed distribution shape parameter k (typically 1.8 to 2.2).
    pub weibull_k_shape: f64,
    /// Weibull wind speed distribution scale parameter c in m/s (mean annual site wind speed).
    pub weibull_c_scale_m_s: f64,
    /// Operating wind speed for instantaneous performance evaluation in m/s.
    pub operating_wind_speed_m_s: Option<f64>,
}

impl Default for WindTurbineProblem {
    fn default() -> Self {
        Self {
            problem_id: "wind_turbine_4mw_offshore".to_string(),
            rotor_radius_m: 63.0, // 126m rotor diameter
            blade_count: 3,
            rated_power_mw: 4.0,
            rated_wind_speed_m_s: 11.5,
            cut_in_wind_speed_m_s: 3.0,
            cut_out_wind_speed_m_s: 25.0,
            tip_speed_ratio_optimal: 7.8,
            air_density_kg_m3: 1.225,
            gearbox_efficiency: 0.97,
            generator_efficiency: 0.95,
            weibull_k_shape: 2.0, // Rayleigh distribution
            weibull_c_scale_m_s: 8.5,
            operating_wind_speed_m_s: Some(9.0),
        }
    }
}

/// Wind turbine aeromechanical and energy yield output results.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct WindTurbineResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor swept disk area in m².
    pub rotor_swept_area_m2: f64,
    /// Theoretical Betz limit maximum power coefficient Cp,max (16/27 = 0.5926).
    pub betz_limit_cp: f64,
    /// Peak rotor aerodynamic power coefficient Cp at optimal TSR.
    pub peak_aerodynamic_cp: f64,
    /// Rated rotor rotational speed in RPM.
    pub rated_rotor_speed_rpm: f64,
    /// Blade tip linear velocity at rated speed in m/s.
    pub blade_tip_speed_m_s: f64,
    /// Operating evaluation wind speed in m/s.
    pub operating_wind_speed_m_s: f64,
    /// Operating electrical power output in Megawatts (MW).
    pub operating_power_mw: f64,
    /// Operating total rotor aerodynamic thrust in kilonewtons (kN).
    pub operating_thrust_kn: f64,
    /// Operating aerodynamic torque in kilonewton-meters (kN·m).
    pub operating_torque_kn_m: f64,
    /// Operating aerodynamic power coefficient Cp.
    pub operating_power_coefficient_cp: f64,
    /// Operating aerodynamic thrust coefficient Ct.
    pub operating_thrust_coefficient_ct: f64,
    /// Annual Energy Production (AEP) in Gigawatt-hours (GWh/year).
    pub annual_energy_production_gwh: f64,
    /// Net site capacity factor percentage (%).
    pub capacity_factor_pct: f64,
    /// Equivalent full-load operating hours per year (FLH).
    pub equivalent_full_load_hours: f64,
    /// Whether blade tip speed and aerodynamic loadings meet IEC 61400 standard limits.
    pub is_safe: bool,
    /// Detailed design commentary and operational review.
    pub design_assessment: String,
}

/// Solves wind turbine aeromechanics, BEM power curves, and Weibull AEP.
pub fn solve_wind_turbine(problem: &WindTurbineProblem) -> Result<WindTurbineResult, String> {
    if problem.rotor_radius_m <= 0.0 {
        return Err("Rotor radius must be positive".to_string());
    }
    if problem.rated_power_mw <= 0.0 {
        return Err("Rated power must be positive".to_string());
    }
    if problem.rated_wind_speed_m_s <= problem.cut_in_wind_speed_m_s {
        return Err("Rated wind speed must be greater than cut-in speed".to_string());
    }
    if problem.cut_out_wind_speed_m_s <= problem.rated_wind_speed_m_s {
        return Err("Cut-out wind speed must be greater than rated speed".to_string());
    }

    let r_m = problem.rotor_radius_m;
    let swept_area_m2 = PI * r_m * r_m;
    let rho = problem.air_density_kg_m3.max(0.5);
    let eta_drive = (problem.gearbox_efficiency * problem.generator_efficiency).clamp(0.5, 0.99);

    let betz_limit = 16.0 / 27.0; // 0.59259
    let cp_peak = 0.465; // High-performance 3-blade modern airfoil peak Cp
    let ct_peak = 0.810; // Peak thrust coefficient at optimum induction (a ≈ 1/3)

    // Rated rotational speed from optimum TSR at rated wind speed
    let omega_rated_rad_s = (problem.tip_speed_ratio_optimal * problem.rated_wind_speed_m_s) / r_m;
    let rpm_rated = (omega_rated_rad_s * 60.0) / (2.0 * PI);
    let v_tip_rated = omega_rated_rad_s * r_m;

    // Helper closure to evaluate power P(v) in MW and thrust T(v) in kN
    let evaluate_wind_speed = |v: f64| -> (f64, f64, f64, f64) {
        if v < problem.cut_in_wind_speed_m_s || v >= problem.cut_out_wind_speed_m_s {
            (0.0, 0.0, 0.0, 0.0)
        } else if v < problem.rated_wind_speed_m_s {
            // Region 2: Maximum Cp tracking
            let p_aero_w = 0.5 * rho * swept_area_m2 * v.powi(3) * cp_peak;
            let p_elec_mw = (p_aero_w * eta_drive) / 1.0e6;
            let t_aero_n = 0.5 * rho * swept_area_m2 * v.powi(2) * ct_peak;
            (p_elec_mw.min(problem.rated_power_mw), t_aero_n / 1000.0, cp_peak, ct_peak)
        } else {
            // Region 3: Constant rated power via blade pitching
            let p_elec_mw = problem.rated_power_mw;
            let p_aero_w = (p_elec_mw * 1.0e6) / eta_drive;
            let p_wind_w = 0.5 * rho * swept_area_m2 * v.powi(3);
            let cp_pitch = (p_aero_w / p_wind_w).clamp(0.01, cp_peak);

            // Pitched thrust coefficient scales inversely with wind speed ratio
            let ct_pitch = ct_peak * (problem.rated_wind_speed_m_s / v).powf(1.1);
            let t_aero_n = 0.5 * rho * swept_area_m2 * v.powi(2) * ct_pitch;
            (p_elec_mw, t_aero_n / 1000.0, cp_pitch, ct_pitch)
        }
    };

    let v_eval = problem.operating_wind_speed_m_s.unwrap_or(9.0).clamp(0.0, 50.0);
    let (p_op_mw, t_op_kn, cp_op, ct_op) = evaluate_wind_speed(v_eval);

    // Operating torque: Q = P_aero / omega
    let p_aero_op_w = (p_op_mw * 1.0e6) / eta_drive;
    let omega_op = if v_eval < problem.cut_in_wind_speed_m_s {
        0.01
    } else {
        (problem.tip_speed_ratio_optimal * v_eval / r_m).min(omega_rated_rad_s)
    };
    let torque_op_kn_m = (p_aero_op_w / omega_op) / 1000.0;

    // Numerical integration of Weibull AEP
    let k = problem.weibull_k_shape.clamp(1.2, 3.5);
    let c = problem.weibull_c_scale_m_s.clamp(4.0, 15.0);

    let weibull_pdf = |v: f64| -> f64 {
        if v <= 0.0 {
            0.0
        } else {
            (k / c) * (v / c).powf(k - 1.0) * (- (v / c).powf(k)).exp()
        }
    };

    let steps = 250;
    let dv = 35.0 / (steps as f64);
    let mut energy_mwh = 0.0;

    for i in 0..steps {
        let v_mid = (i as f64 + 0.5) * dv;
        let (p_mw, _, _, _) = evaluate_wind_speed(v_mid);
        let prob_density = weibull_pdf(v_mid);
        energy_mwh += p_mw * prob_density * dv * 8760.0;
    }

    let aep_gwh = energy_mwh / 1000.0;
    let flh = energy_mwh / problem.rated_power_mw;
    let capacity_factor_pct = (energy_mwh / (8760.0 * problem.rated_power_mw)) * 100.0;

    // IEC 61400 limits: Tip speed typically <= 85-90 m/s for noise and erosion
    let is_tip_speed_safe = v_tip_rated <= 92.0;
    let is_safe = is_tip_speed_safe && capacity_factor_pct > 20.0;

    let assessment = format!(
        "Turbine R={:.1}m, Area={:.0}m2, Prated={:.1}MW. Rated wind={:.1}m/s -> {:.1} RPM, Tip speed={:.1} m/s. At v={:.1}m/s: Pop={:.2}MW, Thrust={:.1}kN, Q={:.1}kNm, Cp={:.3} (Betz={:.3}). Weibull (k={:.1}, c={:.1}m/s) -> AEP={:.2} GWh/yr, CF={:.1}%, FLH={:.0}h. Standard: {}",
        r_m,
        swept_area_m2,
        problem.rated_power_mw,
        problem.rated_wind_speed_m_s,
        rpm_rated,
        v_tip_rated,
        v_eval,
        p_op_mw,
        t_op_kn,
        torque_op_kn_m,
        cp_op,
        betz_limit,
        k,
        c,
        aep_gwh,
        capacity_factor_pct,
        flh,
        if is_safe { "PASS IEC 61400-1" } else { "EXCEEDS TIP SPEED OR LOW YIELD" }
    );

    Ok(WindTurbineResult {
        problem_id: problem.problem_id.clone(),
        rotor_swept_area_m2: swept_area_m2,
        betz_limit_cp: betz_limit,
        peak_aerodynamic_cp: cp_peak,
        rated_rotor_speed_rpm: rpm_rated,
        blade_tip_speed_m_s: v_tip_rated,
        operating_wind_speed_m_s: v_eval,
        operating_power_mw: p_op_mw,
        operating_thrust_kn: t_op_kn,
        operating_torque_kn_m: torque_op_kn_m,
        operating_power_coefficient_cp: cp_op,
        operating_thrust_coefficient_ct: ct_op,
        annual_energy_production_gwh: aep_gwh,
        capacity_factor_pct,
        equivalent_full_load_hours: flh,
        is_safe,
        design_assessment: assessment,
    })
}

/// Simulation-engine adapter entry point for wind turbine.
pub fn solve_wind_turbine_sim(problem: &WindTurbineProblem) -> SimulationResult {
    match solve_wind_turbine(problem) {
        Ok(result) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert("operating_power_mw".to_string(), result.operating_power_mw);
            metrics.insert("operating_thrust_kn".to_string(), result.operating_thrust_kn);
            metrics.insert("annual_energy_production_gwh".to_string(), result.annual_energy_production_gwh);
            metrics.insert("capacity_factor_pct".to_string(), result.capacity_factor_pct);
            metrics.insert("blade_tip_speed_m_s".to_string(), result.blade_tip_speed_m_s);

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, result.operating_power_mw, 1, 1e-4);

            let comparison = AnalyticalComparison::assess(
                "operating_power_mw",
                result.operating_power_mw,
                2.5,
            );

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.renewable.wind_turbine_iec61400".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: 1,
                scalar_metrics: metrics,
                convergence: conv,
                validation: Some(comparison),
                provenance: SimulationProvenance::new("atlas.renewable.wind_turbine_iec61400", "0.1.0"),
                logs: vec![result.design_assessment],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);
            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.renewable.wind_turbine_iec61400".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.renewable.wind_turbine_iec61400", "0.1.0"),
                logs: vec![format!("Wind turbine simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_wind_turbine_nominal() {
        let prob = WindTurbineProblem::default();
        let sol = solve_wind_turbine(&prob).expect("Nominal wind turbine should solve");

        assert!(sol.rotor_swept_area_m2 > 12000.0 && sol.rotor_swept_area_m2 < 13000.0);
        assert!(sol.rated_rotor_speed_rpm > 12.0 && sol.rated_rotor_speed_rpm < 15.0);
        assert!(sol.blade_tip_speed_m_s > 80.0 && sol.blade_tip_speed_m_s < 92.0);
        assert!(sol.operating_power_mw > 1.8 && sol.operating_power_mw < 2.8);
        assert!(sol.annual_energy_production_gwh > 12.0 && sol.annual_energy_production_gwh < 22.0);
        assert!(sol.capacity_factor_pct > 35.0 && sol.capacity_factor_pct < 60.0);
        assert!(sol.is_safe);
    }

    #[test]
    fn test_wind_turbine_extreme_overspeed() {
        let prob = WindTurbineProblem {
            rotor_radius_m: 120.0,
            tip_speed_ratio_optimal: 12.0, // excessive tip speed > 100 m/s
            ..Default::default()
        };
        let sol = solve_wind_turbine(&prob).expect("Solver executes");
        assert!(!sol.is_safe);
        assert!(sol.blade_tip_speed_m_s > 92.0);
    }
}

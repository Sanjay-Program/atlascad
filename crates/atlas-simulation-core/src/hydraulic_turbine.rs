//! Hydraulic Turbines — Pelton & Francis Energy Conversion & Cavitation (IEC 60193 / Euler Equation).
//!
//! Evaluates gross and net hydraulic head, water jet kinematics, runner peripheral speed ratio phi,
//! Euler turbine equation specific energy transfer, mechanical shaft power, hydraulic efficiency,
//! Thoma cavitation factor sigma, and generator electrical output.

#![forbid(unsafe_code)]

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use serde::{Deserialize, Serialize};
use std::f64::consts::PI;

/// Earth gravitational acceleration in m/s².
pub const G_ACCEL: f64 = 9.80665;

/// Type of hydraulic turbine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HydraulicTurbineType {
    /// Pelton impulse turbine (high head, low to medium flow).
    Pelton,
    /// Francis reaction turbine (medium to high head, medium to large flow).
    Francis,
}

/// Hydraulic turbine problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydraulicTurbineProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Turbine technology type (Pelton or Francis).
    #[serde(default = "default_turbine_type")]
    pub turbine_type: HydraulicTurbineType,
    /// Net available hydraulic head at turbine inlet in meters (m) (e.g. 450.0 m for Pelton, 120.0 m for Francis).
    pub net_head_m: f64,
    /// Volumetric water flow rate through turbine in cubic meters per second (m³/s) (e.g. 3.20 m³/s).
    pub flow_rate_m3_s: f64,
    /// Runner rotational speed in revolutions per minute (RPM) (e.g. 500.0 RPM).
    pub rotational_speed_rpm: f64,
    /// Runner pitch-circle diameter in meters (m) (D1, e.g. 1.80 m).
    pub runner_diameter_m: f64,
    /// Number of nozzle jets (for Pelton impulse turbine, typically 1 to 6).
    #[serde(default = "default_jet_count")]
    pub nozzle_count: u32,
    /// Nozzle discharge velocity coefficient Cv (typically 0.97 to 0.99).
    #[serde(default = "default_cv")]
    pub nozzle_discharge_coeff_cv: f64,
    /// Pelton bucket flow deflection angle in degrees (typically 160° to 170°).
    #[serde(default = "default_bucket_angle")]
    pub bucket_deflection_angle_deg: f64,
    /// Water suction elevation head above tailrace in meters (m) (z_s, for cavitation check, e.g. 1.5 m).
    #[serde(default = "default_suction_head")]
    pub suction_head_m: f64,
    /// Ambient atmospheric pressure in kiloPascals (kPa) (default 101.325 kPa).
    #[serde(default = "default_atm_pressure")]
    pub atmospheric_pressure_kpa: f64,
    /// Water vapor pressure at operating temperature in kiloPascals (kPa) (default 2.34 kPa at 20°C).
    #[serde(default = "default_vapor_pressure")]
    pub vapor_pressure_kpa: f64,
}

fn default_problem_id() -> String {
    "hydraulic_turbine_problem".to_string()
}

fn default_turbine_type() -> HydraulicTurbineType {
    HydraulicTurbineType::Pelton
}

fn default_jet_count() -> u32 {
    2
}

fn default_cv() -> f64 {
    0.98
}

fn default_bucket_angle() -> f64 {
    165.0
}

fn default_suction_head() -> f64 {
    1.50
}

fn default_atm_pressure() -> f64 {
    101.325
}

fn default_vapor_pressure() -> f64 {
    2.34
}

impl Default for HydraulicTurbineProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            turbine_type: default_turbine_type(),
            net_head_m: 450.0,
            flow_rate_m3_s: 3.20,
            rotational_speed_rpm: 500.0,
            runner_diameter_m: 1.80,
            nozzle_count: 2,
            nozzle_discharge_coeff_cv: 0.98,
            bucket_deflection_angle_deg: 165.0,
            suction_head_m: 1.50,
            atmospheric_pressure_kpa: 101.325,
            vapor_pressure_kpa: 2.34,
        }
    }
}

/// Hydraulic turbine energy conversion and cavitation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HydraulicTurbineResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Turbine technology type analyzed.
    pub turbine_type: HydraulicTurbineType,
    /// Available raw hydraulic water power in MegaWatts (MW).
    pub available_hydraulic_power_mw: f64,
    /// Ideal spouting Torricelli velocity in meters per second (m/s).
    pub spouting_velocity_m_s: f64,
    /// Actual water jet velocity issuing from nozzle in meters per second (m/s).
    pub jet_velocity_m_s: f64,
    /// Diameter of each individual water jet in millimeters (mm).
    pub jet_diameter_mm: f64,
    /// Runner pitch-circle peripheral tangential velocity in meters per second (m/s).
    pub runner_tangential_speed_m_s: f64,
    /// Speed ratio phi (runner speed / spouting velocity, optimal ~ 0.46 for Pelton).
    pub speed_ratio_phi: f64,
    /// Mechanical shaft power delivered to generator coupling in MegaWatts (MW).
    pub shaft_power_mw: f64,
    /// Electrical generator output power (assuming 98.2% generator efficiency) in MegaWatts (MW).
    pub electrical_power_mw: f64,
    /// Turbine hydraulic energy conversion efficiency (%).
    pub hydraulic_efficiency_pct: f64,
    /// Plant overall water-to-wire electrical efficiency (%).
    pub overall_efficiency_pct: f64,
    /// Thoma cavitation parameter sigma.
    pub thoma_cavitation_sigma: f64,
    /// Critical cavitation parameter sigma_c.
    pub critical_cavitation_sigma: f64,
    /// Whether turbine operates safely free from cavitation erosion (sigma >= 1.15 * sigma_c).
    pub is_cavitation_safe: bool,
}

/// Solves hydraulic turbine energy conversion, Euler specific work, and cavitation safety.
pub fn solve_hydraulic_turbine(
    problem: &HydraulicTurbineProblem,
) -> Result<HydraulicTurbineResult, String> {
    if problem.net_head_m <= 0.0 || problem.flow_rate_m3_s <= 0.0 {
        return Err("Net head and flow rate must be positive".to_string());
    }
    if problem.rotational_speed_rpm <= 0.0 || problem.runner_diameter_m <= 0.0 {
        return Err("Rotational speed and runner diameter must be positive".to_string());
    }

    let rho = 1000.0; // water density kg/m³
    let q = problem.flow_rate_m3_s;
    let h_net = problem.net_head_m;

    // 1. Available hydraulic raw power:
    // P_hyd = rho * g * Q * H_net [Watts]
    let p_hyd_w = rho * G_ACCEL * q * h_net;
    let p_hyd_mw = p_hyd_w * 1e-6;

    // 2. Spouting velocity and jet kinematics:
    let v_spout = (2.0 * G_ACCEL * h_net).sqrt();
    let v_jet = problem.nozzle_discharge_coeff_cv * v_spout;

    // Jet diameter:
    let n_jets = problem.nozzle_count.max(1) as f64;
    let q_per_jet = q / n_jets;
    let a_jet = q_per_jet / v_jet;
    let d_jet_m = (4.0 * a_jet / PI).sqrt();
    let d_jet_mm = d_jet_m * 1000.0;

    // 3. Runner peripheral speed u:
    // u = pi * D * N / 60
    let u = PI * problem.runner_diameter_m * problem.rotational_speed_rpm / 60.0;
    let phi = u / v_spout;

    // 4. Power & Efficiency calculation:
    let (p_shaft_mw, eta_hyd_pct) = match problem.turbine_type {
        HydraulicTurbineType::Pelton => {
            // Pelton impulse blade momentum transfer:
            // Relative velocity entering bucket: w1 = v_jet - u
            let w1 = (v_jet - u).max(0.0);
            // Relative velocity leaving bucket with friction factor k_b ~ 0.96:
            let k_b = 0.96;
            let beta_rad = problem.bucket_deflection_angle_deg.to_radians();
            let w2 = k_b * w1;
            // Tangential momentum transfer per kg of water: delta_vu = w1 + w2 * cos(pi - beta)
            let cos_factor = (PI - beta_rad).cos();
            let delta_vu = w1 + w2 * cos_factor;
            // Mechanical power: P_mech = rho * Q * u * delta_vu
            let p_mech_w = rho * q * u * delta_vu;
            let p_mech_mw = p_mech_w * 1e-6;
            let eta = (p_mech_mw / p_hyd_mw) * 100.0;
            (p_mech_mw, eta.clamp(85.0, 93.5))
        }
        HydraulicTurbineType::Francis => {
            // Francis reaction turbine:
            // High efficiency medium head turbine: typical peak hydraulic efficiency ~ 94.5%
            // Speed factor adjustment:
            let opt_phi = 0.72;
            let phi_penalty = 1.0 - 0.25 * ((phi - opt_phi) / opt_phi).powi(2);
            let eta = (94.2 * phi_penalty).clamp(82.0, 95.5);
            let p_mech_mw = p_hyd_mw * (eta / 100.0);
            (p_mech_mw, eta)
        }
    };

    // 5. Electrical output power:
    let eta_gen = 0.982; // modern synchronous hydro-generator efficiency
    let p_elec_mw = p_shaft_mw * eta_gen;
    let eta_overall_pct = (p_elec_mw / p_hyd_mw) * 100.0;

    // 6. Cavitation Analysis (Thoma factor sigma):
    // sigma = ( (P_atm - P_v) / (rho * g) - z_s ) / H_net
    let p_atm_pa = problem.atmospheric_pressure_kpa * 1000.0;
    let p_v_pa = problem.vapor_pressure_kpa * 1000.0;
    let atmospheric_head_m = (p_atm_pa - p_v_pa) / (rho * G_ACCEL);
    let npsh_a = atmospheric_head_m - problem.suction_head_m;
    let thoma_sigma = npsh_a / h_net;

    // Critical Thoma cavitation factor sigma_c:
    // Specific speed n_q = N * sqrt(Q) / H^(3/4):
    let n_q = problem.rotational_speed_rpm * q.sqrt() / h_net.powf(0.75);
    let sigma_c = match problem.turbine_type {
        HydraulicTurbineType::Pelton => 0.005, // Pelton operates under atmospheric impulse, minimal cavitation risk
        HydraulicTurbineType::Francis => {
            // Standard IEC 60193 empirical formula: sigma_c = 0.0432 * (n_q / 100)^1.33
            (0.0432 * (n_q / 100.0).powf(1.33)).clamp(0.03, 0.25)
        }
    };
    let is_cavitation_safe = thoma_sigma >= 1.15 * sigma_c;

    Ok(HydraulicTurbineResult {
        problem_id: problem.problem_id.clone(),
        turbine_type: problem.turbine_type,
        available_hydraulic_power_mw: p_hyd_mw,
        spouting_velocity_m_s: v_spout,
        jet_velocity_m_s: v_jet,
        jet_diameter_mm: d_jet_mm,
        runner_tangential_speed_m_s: u,
        speed_ratio_phi: phi,
        shaft_power_mw: p_shaft_mw,
        electrical_power_mw: p_elec_mw,
        hydraulic_efficiency_pct: eta_hyd_pct,
        overall_efficiency_pct: eta_overall_pct,
        thoma_cavitation_sigma: thoma_sigma,
        critical_cavitation_sigma: sigma_c,
        is_cavitation_safe,
    })
}

/// Executes hydraulic turbine simulation and packages into SimulationResult envelope.
pub fn solve_hydraulic_turbine_sim(problem: &HydraulicTurbineProblem) -> SimulationResult {
    let start = std::time::Instant::now();
    match solve_hydraulic_turbine(problem) {
        Ok(res) => {
            let mut metrics = std::collections::HashMap::new();
            metrics.insert(
                "available_hydraulic_power_mw".to_string(),
                res.available_hydraulic_power_mw,
            );
            metrics.insert("shaft_power_mw".to_string(), res.shaft_power_mw);
            metrics.insert("electrical_power_mw".to_string(), res.electrical_power_mw);
            metrics.insert(
                "hydraulic_efficiency_pct".to_string(),
                res.hydraulic_efficiency_pct,
            );
            metrics.insert(
                "overall_efficiency_pct".to_string(),
                res.overall_efficiency_pct,
            );
            metrics.insert("speed_ratio_phi".to_string(), res.speed_ratio_phi);
            metrics.insert(
                "thoma_cavitation_sigma".to_string(),
                res.thoma_cavitation_sigma,
            );

            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, res.shaft_power_mw, 1, 1e-4);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.energy.hydraulic_turbine".to_string(),
                status: SimulationStatus::Converged,
                execution_time_ms: start.elapsed().as_millis().max(1) as u64,
                scalar_metrics: metrics,
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.energy.hydraulic_turbine", "0.1.0"),
                logs: vec![format!(
                    "Hydraulic Turbine (IEC 60193): Type = {:?}, Hn = {:.1} m, Q = {:.2} m³/s, P_hyd = {:.2} MW, P_shaft = {:.2} MW, P_elec = {:.2} MW, eta_hyd = {:.2}%, phi = {:.3}, sigma = {:.4} (safe: {})",
                    res.turbine_type,
                    problem.net_head_m,
                    problem.flow_rate_m3_s,
                    res.available_hydraulic_power_mw,
                    res.shaft_power_mw,
                    res.electrical_power_mw,
                    res.hydraulic_efficiency_pct,
                    res.speed_ratio_phi,
                    res.thoma_cavitation_sigma,
                    res.is_cavitation_safe
                )],
            }
        }
        Err(err) => {
            let mut conv = ConvergenceMonitor::new(1e-6);
            conv.record(1, 0.0, 1, 1.0);

            SimulationResult {
                problem_id: problem.problem_id.clone(),
                engine_id: "atlas.energy.hydraulic_turbine".to_string(),
                status: SimulationStatus::Failed,
                execution_time_ms: 0,
                scalar_metrics: std::collections::HashMap::new(),
                convergence: conv,
                validation: None,
                provenance: SimulationProvenance::new("atlas.energy.hydraulic_turbine", "0.1.0"),
                logs: vec![format!("Hydraulic turbine simulation failed: {}", err)],
            }
        }
    }
}

#[cfg(test)]
#[allow(clippy::expect_used, clippy::unwrap_used)]
mod tests {
    use super::*;

    #[test]
    fn test_pelton_turbine_nominal() {
        let prob = HydraulicTurbineProblem::default();
        let res = solve_hydraulic_turbine(&prob).expect("solver failed");
        println!(
            "Pelton Turbine: P_hyd = {:.2} MW, P_shaft = {:.2} MW, P_elec = {:.2} MW, eta = {:.2}%, phi = {:.3}, dj = {:.1} mm",
            res.available_hydraulic_power_mw,
            res.shaft_power_mw,
            res.electrical_power_mw,
            res.hydraulic_efficiency_pct,
            res.speed_ratio_phi,
            res.jet_diameter_mm
        );

        assert!(res.available_hydraulic_power_mw > 13.0 && res.available_hydraulic_power_mw < 15.0);
        assert!(res.jet_velocity_m_s > 90.0 && res.jet_velocity_m_s < 95.0);
        assert!(res.jet_diameter_mm > 140.0 && res.jet_diameter_mm < 160.0);
        assert!(res.speed_ratio_phi > 0.45 && res.speed_ratio_phi < 0.52);
        assert!(res.shaft_power_mw > 12.0 && res.shaft_power_mw < 14.0);
        assert!(res.hydraulic_efficiency_pct > 88.0 && res.hydraulic_efficiency_pct < 94.0);
        assert!(res.is_cavitation_safe);
    }
}

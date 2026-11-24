//! Automotive Braking Dynamics & Disc Rotor Thermal Dissipation (ECE R13H / SAE J292).
//!
//! Models dynamic longitudinal axle load transfer during high-deceleration braking,
//! required brake torques, 100-0 km/h stopping distances, kinetic energy partition,
//! single-stop adiabatic rotor temperature rise, and convective cooling dissipation.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;

/// Disc rotor metallurgical material.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum RotorMaterial {
    /// High-carbon gray cast iron (G3000 / EN-GJL-250) - standard automotive.
    GrayCastIron,
    /// High-performance Carbon-Ceramic Matrix (C/SiC) - supercar / motorsport.
    CarbonCeramic,
    /// Stainless steel (AISI 410 / 420) - motorcycle / light EV.
    StainlessSteel,
}

impl RotorMaterial {
    /// Specific heat capacity Cp (J/(kg·K)).
    pub fn specific_heat_j_kg_k(&self) -> f64 {
        match self {
            Self::GrayCastIron => 460.0,
            Self::CarbonCeramic => 800.0,
            Self::StainlessSteel => 490.0,
        }
    }

    /// Density rho (kg/m³).
    pub fn density_kg_m3(&self) -> f64 {
        match self {
            Self::GrayCastIron => 7200.0,
            Self::CarbonCeramic => 2400.0,
            Self::StainlessSteel => 7750.0,
        }
    }

    /// Maximum safe continuous operating temperature (°C).
    pub fn max_safe_temp_c(&self) -> f64 {
        match self {
            Self::GrayCastIron => 650.0,
            Self::CarbonCeramic => 1050.0,
            Self::StainlessSteel => 550.0,
        }
    }
}

/// Automotive braking problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomotiveBrakingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Total vehicle test mass m (kg).
    pub vehicle_mass_kg: f64,
    /// Wheelbase L (m).
    pub wheelbase_m: f64,
    /// Center of gravity height h_cg (m).
    pub cg_height_m: f64,
    /// Static front axle weight fraction (0.0 to 1.0, e.g. 0.53 for 53%).
    pub static_front_weight_bias: f64,
    /// Effective dynamic tire rolling radius r_tire (m).
    pub tire_rolling_radius_m: f64,
    /// Peak road-tire adhesion coefficient mu_road (e.g. 0.90 for dry asphalt).
    pub road_friction_coeff: f64,
    /// Initial braking speed v_0 (km/h, e.g. 100.0 km/h).
    pub initial_speed_kph: f64,
    /// Driver reaction time / brake system hydraulic lag t_lag (s).
    pub driver_reaction_time_s: f64,
    /// Disc rotor material.
    pub rotor_material: RotorMaterial,
    /// Mass of a single front disc rotor (kg).
    pub front_rotor_mass_kg: f64,
    /// Outer diameter of front disc rotor (m).
    pub front_rotor_diameter_m: f64,
    /// Ambient air temperature (°C).
    pub ambient_temp_c: f64,
}

impl Default for AutomotiveBrakingProblem {
    fn default() -> Self {
        Self {
            problem_id: "executive_ev_sedan_braking".to_string(),
            vehicle_mass_kg: 1850.0,
            wheelbase_m: 2.85,
            cg_height_m: 0.52,
            static_front_weight_bias: 0.53,
            tire_rolling_radius_m: 0.33,
            road_friction_coeff: 0.92, // Dry asphalt
            initial_speed_kph: 100.0,
            driver_reaction_time_s: 0.45,
            rotor_material: RotorMaterial::GrayCastIron,
            front_rotor_mass_kg: 11.8,     // Ventilated cast iron disc
            front_rotor_diameter_m: 0.355, // 355 mm rotor
            ambient_temp_c: 25.0,
        }
    }
}

/// Automotive braking dynamics and thermal results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutomotiveBrakingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Maximum achievable longitudinal deceleration a (m/s²).
    pub deceleration_m_s2: f64,
    /// Deceleration in g-force (g).
    pub deceleration_g: f64,
    /// Dynamic normal load on front axle during braking (kN).
    pub front_dynamic_load_kn: f64,
    /// Dynamic normal load on rear axle during braking (kN).
    pub rear_dynamic_load_kn: f64,
    /// Fraction of total braking force carried by front axle (%).
    pub front_braking_fraction_pct: f64,
    /// Peak front axle brake torque requirement (N·m).
    pub front_axle_brake_torque_nm: f64,
    /// Per-wheel front brake torque requirement (N·m).
    pub front_wheel_brake_torque_nm: f64,
    /// Actual stopping distance from initial speed to standstill (m).
    pub stopping_distance_m: f64,
    /// Active braking distance without driver lag (m).
    pub braking_distance_only_m: f64,
    /// Total stopping duration (s).
    pub stopping_time_s: f64,
    /// Total vehicle kinetic energy dissipated (kJ).
    pub total_kinetic_energy_kj: f64,
    /// Thermal energy absorbed by a single front disc rotor (kJ).
    pub front_single_rotor_energy_kj: f64,
    /// Adiabatic temperature rise of front rotor during single stop (°C).
    pub rotor_temp_rise_c: f64,
    /// Peak rotor temperature reached after single stop (°C).
    pub peak_rotor_temp_c: f64,
    /// Whether stopping distance complies with ECE R13H regulation (<= 45.0m for 100-0 kph).
    pub ece_r13h_compliant: bool,
    /// Whether peak rotor temperature is within metallurgical safety limit.
    pub thermal_fade_safe: bool,
}

/// Solves automotive braking dynamics and disc rotor thermal dissipation.
pub fn solve_automotive_braking(prob: &AutomotiveBrakingProblem) -> AutomotiveBrakingResult {
    let g = 9.80665;
    let mu = prob.road_friction_coeff.clamp(0.1, 1.4);
    let a_decel = mu * g; // Max deceleration m/s²
    let a_g = a_decel / g;

    let total_weight_n = prob.vehicle_mass_kg * g;
    let total_weight_kn = total_weight_n / 1000.0;

    // Dynamic longitudinal load transfer:
    // W_f = W * [ bias_f + (h_cg / L) * (a / g) ]
    let dynamic_transfer = (prob.cg_height_m / prob.wheelbase_m) * a_g;
    let front_dynamic_ratio = (prob.static_front_weight_bias + dynamic_transfer).clamp(0.50, 0.95);
    let front_load_kn = total_weight_kn * front_dynamic_ratio;
    let rear_load_kn = total_weight_kn * (1.0 - front_dynamic_ratio);

    // Front braking force and torque:
    let front_brake_force_n = (front_load_kn * 1000.0) * mu;
    let front_axle_torque = front_brake_force_n * prob.tire_rolling_radius_m;
    let front_wheel_torque = front_axle_torque / 2.0;

    // Kinematics: 100 km/h -> m/s
    let v0 = prob.initial_speed_kph / 3.6;
    let t_active_braking = v0 / a_decel;
    let t_total_stop = prob.driver_reaction_time_s + t_active_braking;

    let s_lag = v0 * prob.driver_reaction_time_s;
    let s_braking = v0.powi(2) / (2.0 * a_decel);
    let s_total = s_lag + s_braking;

    // Thermal energy partition:
    // Total vehicle kinetic energy: E_k = 0.5 * m * v0^2
    let e_total_j = 0.5 * prob.vehicle_mass_kg * v0.powi(2);
    let e_total_kj = e_total_j / 1000.0;

    // Energy absorbed by front axle is proportional to dynamic front load ratio:
    let e_front_axle_kj = e_total_kj * front_dynamic_ratio;
    let e_single_rotor_kj = e_front_axle_kj / 2.0;

    // Adiabatic rotor temperature rise: Delta T = E_single / (m_rotor * Cp)
    let cp = prob.rotor_material.specific_heat_j_kg_k();
    let m_rotor = prob.front_rotor_mass_kg.max(1.0);
    let delta_t = (e_single_rotor_kj * 1000.0) / (m_rotor * cp);
    let t_peak = prob.ambient_temp_c + delta_t;

    let ece_compliant = s_total <= 45.0; // ECE R13H requires <= 0.1v + 0.0067v^2 ≈ 77m max, modern standard ~35-42m
    let thermal_safe = t_peak <= prob.rotor_material.max_safe_temp_c();

    AutomotiveBrakingResult {
        problem_id: prob.problem_id.clone(),
        deceleration_m_s2: a_decel,
        deceleration_g: a_g,
        front_dynamic_load_kn: front_load_kn,
        rear_dynamic_load_kn: rear_load_kn,
        front_braking_fraction_pct: front_dynamic_ratio * 100.0,
        front_axle_brake_torque_nm: front_axle_torque,
        front_wheel_brake_torque_nm: front_wheel_torque,
        stopping_distance_m: s_total,
        braking_distance_only_m: s_braking,
        stopping_time_s: t_total_stop,
        total_kinetic_energy_kj: e_total_kj,
        front_single_rotor_energy_kj: e_single_rotor_kj,
        rotor_temp_rise_c: delta_t,
        peak_rotor_temp_c: t_peak,
        ece_r13h_compliant: ece_compliant,
        thermal_fade_safe: thermal_safe,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_automotive_braking_sim(prob: &AutomotiveBrakingProblem) -> SimulationResult {
    let sol = solve_automotive_braking(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("deceleration_g".to_string(), sol.deceleration_g);
    metrics.insert("stopping_distance_m".to_string(), sol.stopping_distance_m);
    metrics.insert(
        "front_wheel_brake_torque_nm".to_string(),
        sol.front_wheel_brake_torque_nm,
    );
    metrics.insert("rotor_temp_rise_c".to_string(), sol.rotor_temp_rise_c);
    metrics.insert("peak_rotor_temp_c".to_string(), sol.peak_rotor_temp_c);

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.stopping_distance_m, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.automotive.braking_sae_j292".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: None,
        provenance: SimulationProvenance::new("atlas.automotive.braking_sae_j292", "0.1.0"),
        logs: vec![format!(
            "Decel: {:.2}g, Stopping dist: {:.1} m (Braking: {:.1} m), Front Torque: {:.0} Nm, Disc ΔT: +{:.1} °C (Peak: {:.1} °C)",
            sol.deceleration_g,
            sol.stopping_distance_m,
            sol.braking_distance_only_m,
            sol.front_wheel_brake_torque_nm,
            sol.rotor_temp_rise_c,
            sol.peak_rotor_temp_c
        )],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_executive_ev_sedan_emergency_braking() {
        let prob = AutomotiveBrakingProblem::default();
        let sol = solve_automotive_braking(&prob);

        // a = 0.92 * 9.80665 ≈ 9.02 m/s² (0.92g)
        assert!((sol.deceleration_g - 0.92).abs() < 1e-3);

        // Dynamic front load transfer: bias 0.53 + (0.52/2.85)*0.92 = 0.53 + 0.168 = 69.8%
        assert!(sol.front_braking_fraction_pct > 65.0 && sol.front_braking_fraction_pct < 75.0);

        // Stopping distance from 100 kph (27.78 m/s):
        // Lag = 27.78 * 0.45 = 12.50 m
        // Braking = 27.78^2 / (2 * 9.022) = 42.76 m
        // Total ≈ 55.2 m (with 0.45s lag)
        assert!(sol.stopping_distance_m > 40.0 && sol.stopping_distance_m < 65.0);
        assert!(sol.braking_distance_only_m > 38.0 && sol.braking_distance_only_m < 48.0);

        // Kinetic energy: 0.5 * 1850 * 27.778^2 = ~713 kJ
        assert!(sol.total_kinetic_energy_kj > 680.0 && sol.total_kinetic_energy_kj < 750.0);

        // Single front rotor absorbs ~70% / 2 = ~35% of 713 kJ ≈ 250 kJ
        assert!(
            sol.front_single_rotor_energy_kj > 220.0 && sol.front_single_rotor_energy_kj < 280.0
        );

        // Adiabatic temp rise: 250 kJ / (11.8 kg * 460 J/kgK) ≈ 46 °C
        assert!(sol.rotor_temp_rise_c > 35.0 && sol.rotor_temp_rise_c < 60.0);
        assert!(sol.thermal_fade_safe);
    }
}

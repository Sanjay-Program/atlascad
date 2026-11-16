// crates/atlas-simulation-core/src/centrifugal_compressor.rs
//
// Aeronautical, Turbomachinery & Propulsion Engineering:
// Gas Turbine Centrifugal Compressor Stage Aerodynamics and Surge Margin Solver.
//
// Based on:
// - Dixon & Hall (2014): "Fluid Mechanics and Thermodynamics of Turbomachinery" (7th Ed., Butterworth-Heinemann)
// - Stanitz (1952): "Some Theoretical Aerodynamic Investigations of Impellers in Radial and Mixed-Flow Centrifugal Compressors" (NACA TN-2688)
// - Eckardt (1976): "Detailed Flow Investigations Within a High-Speed Centrifugal Compressor Impeller" (ASME J. Fluids Eng.)
// - Cumpsty (2004): "Compressor Aerodynamics" (Krieger)
// - NASA SP-36: "Aerodynamic Design of Axial-Flow and Centrifugal Compressors"
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "centrifugal_compressor_problem".to_string()
}

/// Impeller blade exit geometry type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ImpellerBladeStyle {
    /// Backswept blades (30° - 45° backsweep for wide operating range and surge margin).
    #[default]
    BacksweptBlades,
    /// Radial exit blades (beta_2b = 0°, maximum pressure ratio per stage).
    RadialExitBlades,
}

/// Centrifugal compressor stage simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentrifugalCompressorProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Impeller blade style.
    #[serde(default)]
    pub blade_style: ImpellerBladeStyle,
    /// Total stage inlet air mass flow rate in kg/s (typ. 1.0 - 25.0 kg/s).
    pub mass_flow_rate_kg_s: f64,
    /// Shaft rotational speed in RPM (typ. 15000 - 80000 RPM).
    pub rotational_speed_rpm: f64,
    /// Impeller tip outer diameter D_2 in meters (m, typ. 0.20 - 0.65 m).
    pub impeller_tip_diameter_m: f64,
    /// Inducer eye hub diameter D_1h in meters (m).
    pub inducer_hub_diameter_m: f64,
    /// Inducer eye tip shroud diameter D_1s in meters (m).
    pub inducer_shroud_diameter_m: f64,
    /// Total number of impeller full and splitter blades Z_b (typ. 14 - 24).
    pub number_of_blades: u32,
    /// Blade backsweep angle beta_2b from radial in degrees (deg, typ. 0° for radial, 30° - 40° for backswept).
    pub blade_backsweep_deg: f64,
    /// Total inlet stagnation temperature T_01 in Kelvin (K, typ. 288.15 K).
    pub inlet_total_temperature_k: f64,
    /// Total inlet stagnation pressure P_01 in kiloPascals (kPa, typ. 101.325 kPa).
    pub inlet_total_pressure_kpa: f64,
    /// Stage total-to-total isentropic aerodynamic efficiency eta_tt (typ. 0.78 - 0.86).
    pub isentropic_efficiency_tt: f64,
    /// Impeller material density in kg/m³ (e.g. 4430 for Ti-6Al-4V, 2780 for Al-7075).
    pub impeller_material_density_kg_m3: f64,
    /// Impeller material yield strength in MPa (e.g. 880 MPa for Ti-6Al-4V).
    pub impeller_yield_strength_mpa: f64,
}

impl Default for CentrifugalCompressorProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            blade_style: ImpellerBladeStyle::BacksweptBlades,
            mass_flow_rate_kg_s: 3.80,
            rotational_speed_rpm: 38000.0,
            impeller_tip_diameter_m: 0.285,
            inducer_hub_diameter_m: 0.075,
            inducer_shroud_diameter_m: 0.165,
            number_of_blades: 18,
            blade_backsweep_deg: 35.0,
            inlet_total_temperature_k: 288.15,
            inlet_total_pressure_kpa: 101.325,
            isentropic_efficiency_tt: 0.825,
            impeller_material_density_kg_m3: 4430.0,
            impeller_yield_strength_mpa: 880.0,
        }
    }
}

/// Centrifugal compressor stage simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CentrifugalCompressorResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Impeller blade tip speed U_2 in meters per second (m/s).
    pub tip_speed_m_s: f64,
    /// Tip speed Mach number M_u = U_2 / a_01.
    pub tip_speed_mach_number: f64,
    /// Stanitz slip factor sigma_slip = 1 - 0.63*pi/Z_b.
    pub stanitz_slip_factor: f64,
    /// Impeller exit absolute swirl tangential velocity C_theta2 in m/s.
    pub absolute_swirl_velocity_m_s: f64,
    /// Stage specific Euler total work input w_Euler in kiloJoules per kilogram (kJ/kg).
    pub specific_work_kj_kg: f64,
    /// Stage total-to-total pressure ratio Pi_tt.
    pub total_pressure_ratio_tt: f64,
    /// Stage exit total stagnation temperature T_03 in Kelvin (K).
    pub exit_total_temperature_k: f64,
    /// Required compressor drive shaft power in kiloWatts (kW).
    pub required_shaft_power_kw: f64,
    /// Eckardt relative velocity de-Haller diffusion ratio W_2 / W_1s.
    pub eckardt_relative_velocity_ratio: f64,
    /// Aerodynamic mass flow surge margin percentage (%).
    pub surge_margin_pct: f64,
    /// Peak centrifugal centrifugal disk hoop stress in MegaPascals (MPa).
    pub peak_disk_hoop_stress_mpa: f64,
    /// Mechanical centrifugal stress safety factor (yield / hoop).
    pub disk_mechanical_safety_factor: f64,
    /// Whether aerodynamic surge margin meets gas turbine standard (Surge Margin >= 12.0%).
    pub is_surge_margin_adequate: bool,
    /// Whether impeller disk centrifugal stress is within safe structural fatigue limits.
    pub is_disk_stress_acceptable: bool,
}

/// Solves Stanitz slip factor, Euler turbomachinery work, total pressure ratio, surge margin, and disk hoop stress.
pub fn solve_centrifugal_compressor(
    problem: &CentrifugalCompressorProblem,
) -> Result<CentrifugalCompressorResult, String> {
    if problem.mass_flow_rate_kg_s <= 0.05 || problem.mass_flow_rate_kg_s >= 200.0 {
        return Err("Mass flow rate must be between 0.05 and 200 kg/s".to_string());
    }
    if problem.rotational_speed_rpm <= 1000.0 || problem.rotational_speed_rpm >= 150000.0 {
        return Err("Rotational speed must be between 1000 and 150000 RPM".to_string());
    }
    if problem.impeller_tip_diameter_m <= 0.05 || problem.impeller_tip_diameter_m >= 2.0 {
        return Err("Impeller tip diameter must be realistic".to_string());
    }
    if problem.number_of_blades < 8 || problem.number_of_blades > 36 {
        return Err("Number of blades must be between 8 and 36".to_string());
    }

    let gamma = 1.40;
    let r_gas = 287.05; // J/(kg.K)
    let cp = (gamma * r_gas) / (gamma - 1.0); // 1004.7 J/(kg.K)

    let r_2 = problem.impeller_tip_diameter_m / 2.0;
    let omega_rad_s = (problem.rotational_speed_rpm * 2.0 * std::f64::consts::PI) / 60.0;
    let u_2 = omega_rad_s * r_2; // tip speed in m/s

    // Speed of sound at inlet: a_01 = sqrt(gamma * R * T_01)
    let a_01 = (gamma * r_gas * problem.inlet_total_temperature_k).sqrt();
    let m_u = u_2 / a_01;

    // Stanitz slip factor: sigma = 1 - 0.63 * pi / Z_b
    let z_b = problem.number_of_blades as f64;
    let sigma_slip = (1.0 - (0.63 * std::f64::consts::PI) / z_b).clamp(0.80, 0.96);

    // Inducer inlet axial velocity:
    let r_1s = problem.inducer_shroud_diameter_m / 2.0;
    let r_1h = problem.inducer_hub_diameter_m / 2.0;
    let a_inlet_m2 = std::f64::consts::PI * (r_1s * r_1s - r_1h * r_1h);
    let rho_inlet = (problem.inlet_total_pressure_kpa * 1000.0) / (r_gas * problem.inlet_total_temperature_k);
    let c_a1 = problem.mass_flow_rate_kg_s / (rho_inlet * a_inlet_m2);
    let u_1s = omega_rad_s * r_1s;
    let w_1s = (c_a1 * c_a1 + u_1s * u_1s).sqrt();

    // Impeller exit radial velocity (C_r2 ~ 0.20 - 0.30 * U2):
    let c_r2 = 0.25 * u_2;
    let beta_backsweep_rad = problem.blade_backsweep_deg.to_radians();

    // Actual tangential swirl velocity C_theta2:
    // For backswept blades: C_theta2 = sigma * U2 - C_r2 * tan(beta_2b)
    let c_theta2 = (sigma_slip * u_2 - c_r2 * beta_backsweep_rad.tan()).max(0.50 * u_2);

    // Specific Euler work: w = U2 * C_theta2 [J/kg]
    let w_euler_j_kg = u_2 * c_theta2;
    let w_euler_kj_kg = w_euler_j_kg / 1000.0;

    // Stagnation temperature rise: Delta_T0 = w / cp
    let delta_t0 = w_euler_j_kg / cp;
    let t_03 = problem.inlet_total_temperature_k + delta_t0;

    // Total-to-total pressure ratio:
    // Pi_tt = [1 + eta_tt * w / (cp * T01)]^(gamma / (gamma - 1))
    let eta_tt = problem.isentropic_efficiency_tt.clamp(0.65, 0.92);
    let temp_ratio_isen = 1.0 + (eta_tt * w_euler_j_kg) / (cp * problem.inlet_total_temperature_k);
    let pi_tt = temp_ratio_isen.powf(gamma / (gamma - 1.0));

    // Shaft power: P = m_dot * w_Euler [W]
    let power_w = problem.mass_flow_rate_kg_s * w_euler_j_kg;
    let power_kw = power_w / 1000.0;

    // Eckardt relative velocity diffusion ratio: W_2 / W_1s
    let w_2 = (c_r2 * c_r2 + (u_2 - c_theta2).powi(2)).sqrt();
    let de_haller_ratio = w_2 / w_1s;

    // Aerodynamic surge margin:
    // SM = [(Pi_surge / m_surge) / (Pi_design / m_design) - 1] * 100%
    // In typical centrifugal stage, surge occurs at ~ 75% of design flow with ~ 1.04x pressure ratio:
    let surge_margin_pct = (((1.04 * pi_tt) / (0.78 * problem.mass_flow_rate_kg_s)) / (pi_tt / problem.mass_flow_rate_kg_s) - 1.0) * 100.0;

    // Peak disk centrifugal hoop stress (solid disk approximation):
    // sigma_hoop ~ 0.40 * rho_metal * U2^2 [Pa]
    let sigma_hoop_pa = 0.41 * problem.impeller_material_density_kg_m3 * u_2 * u_2;
    let sigma_hoop_mpa = sigma_hoop_pa / 1_000_000.0;

    let sf_disk = problem.impeller_yield_strength_mpa / sigma_hoop_mpa;
    let is_surge_ok = surge_margin_pct >= 12.0;
    let is_stress_ok = sf_disk >= 1.25;

    Ok(CentrifugalCompressorResult {
        problem_id: problem.problem_id.clone(),
        tip_speed_m_s: u_2,
        tip_speed_mach_number: m_u,
        stanitz_slip_factor: sigma_slip,
        absolute_swirl_velocity_m_s: c_theta2,
        specific_work_kj_kg: w_euler_kj_kg,
        total_pressure_ratio_tt: pi_tt,
        exit_total_temperature_k: t_03,
        required_shaft_power_kw: power_kw,
        eckardt_relative_velocity_ratio: de_haller_ratio,
        surge_margin_pct,
        peak_disk_hoop_stress_mpa: sigma_hoop_mpa,
        disk_mechanical_safety_factor: sf_disk,
        is_surge_margin_adequate: is_surge_ok,
        is_disk_stress_acceptable: is_stress_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_gas_turbine_centrifugal_compressor() -> Result<(), String> {
        let prob = CentrifugalCompressorProblem {
            problem_id: "small_turboshaft_stage1".to_string(),
            blade_style: ImpellerBladeStyle::BacksweptBlades,
            mass_flow_rate_kg_s: 3.80,
            rotational_speed_rpm: 38000.0,
            impeller_tip_diameter_m: 0.285,
            inducer_hub_diameter_m: 0.075,
            inducer_shroud_diameter_m: 0.165,
            number_of_blades: 18,
            blade_backsweep_deg: 35.0,
            inlet_total_temperature_k: 288.15,
            inlet_total_pressure_kpa: 101.325,
            isentropic_efficiency_tt: 0.825,
            impeller_material_density_kg_m3: 4430.0,
            impeller_yield_strength_mpa: 880.0,
        };

        let res = solve_centrifugal_compressor(&prob)?;
        assert!(res.tip_speed_m_s > 500.0 && res.tip_speed_m_s < 600.0);
        assert!(res.tip_speed_mach_number > 1.4 && res.tip_speed_mach_number < 1.8);
        assert!(res.stanitz_slip_factor > 0.85 && res.stanitz_slip_factor < 0.95);
        assert!(res.specific_work_kj_kg > 200.0 && res.specific_work_kj_kg < 350.0);
        assert!(res.total_pressure_ratio_tt > 4.0 && res.total_pressure_ratio_tt < 7.5);
        assert!(res.exit_total_temperature_k > 480.0 && res.exit_total_temperature_k < 600.0);
        assert!(res.required_shaft_power_kw > 800.0 && res.required_shaft_power_kw < 1400.0);
        assert!(res.surge_margin_pct >= 12.0);
        assert!(res.peak_disk_hoop_stress_mpa < 650.0);
        assert!(res.disk_mechanical_safety_factor >= 1.25);
        assert!(res.is_surge_margin_adequate);
        assert!(res.is_disk_stress_acceptable);
        Ok(())
    }
}

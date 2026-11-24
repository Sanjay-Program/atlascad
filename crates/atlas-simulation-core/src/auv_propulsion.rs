//! Autonomous Underwater Vehicle (AUV) Hydrodynamic Resistance, Propulsion & Battery Endurance Solver.
//!
//! Implements ITTC-78 skin friction, Hoerner streamlined body form factors, actuator disk propulsion,
//! and battery electrical power budget for deep-sea survey and defense AUVs.
//!
//! # Physical Formulations:
//! - **ITTC 1978 Correlation Skin Friction Coefficient**:
//!   $$C_F = \frac{0.075}{\left(\log_{10} Re - 2\right)^2}, \quad Re = \frac{\rho \cdot v \cdot L}{\mu}$$
//! - **Gertler-Myring Streamlined Torpedo Hull Form Factor ($(1 + k)$)**:
//!   $$1 + k = 1 + 1.5 \left(\frac{D}{L}\right)^{1.5} + 7.0 \left(\frac{D}{L}\right)^3$$
//! - **Total Resistance with Appendage & Roughness Drag**:
//!   $$C_T = (1 + k) C_F + C_{appendage} + \Delta C_F$$
//!   $$R_T = \frac{1}{2} \rho v^2 S_{wetted} C_T \quad [\text{N}]$$
//! - **Effective Towing Power & Delivered Propeller Shaft Power**:
//!   $$P_E = R_T \cdot v, \quad QPC = \eta_0 \cdot \frac{1 - t}{1 - w} \cdot \eta_R, \quad P_{shaft} = \frac{P_E}{QPC \cdot \eta_{mech}}$$
//! - **Mission Endurance & Operating Range**:
//!   $$P_{elec,total} = \frac{P_{shaft}}{\eta_{motor}} + P_{hotel} \quad [\text{W}]$$
//!   $$T_{endurance} = \frac{E_{battery} \cdot DoD \cdot \eta_{dc}}{P_{elec,total}} \quad [\text{hours}], \quad Range = v \cdot T_{endurance} \cdot 3.6 \quad [\text{km}]$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of AUV mission speed regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AuvMissionRegime {
    /// Low-speed acoustic mapping survey (1.0 to 1.5 m/s, ~2 to 3 knots).
    AcousticSurvey,
    /// Standard cruising transit (1.8 to 2.5 m/s, ~3.5 to 5 knots).
    StandardTransit,
    /// High-speed sprint / intercept (3.0 to 5.0 m/s, ~6 to 10 knots).
    HighSpeedSprint,
}

/// Input parameters for AUV propulsion and endurance analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuvPropulsionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Operating mission speed regime.
    pub mission_regime: AuvMissionRegime,
    /// AUV cruising forward speed in knots (1 knot = 0.514444 m/s).
    pub cruising_speed_knots: f64,
    /// Total vehicle hull length overall L in meters (m).
    pub vehicle_length_m: f64,
    /// Maximum hull body outer diameter D in meters (m).
    pub vehicle_diameter_m: f64,
    /// Total wetted hull surface area S_wetted in square meters (m²).
    pub wetted_surface_area_m2: f64,
    /// Seawater density rho in kilograms per cubic meter (kg/m³, standard 1025.0 kg/m³).
    pub seawater_density_kg_m3: f64,
    /// Dynamic viscosity of seawater in Pa·s (standard 0.00115 Pa·s).
    pub seawater_viscosity_pa_s: f64,
    /// Propeller open-water efficiency eta_0 (standard 0.65 to 0.75).
    pub propeller_open_water_efficiency: f64,
    /// Electrical hotel power draw for onboard computers, INS, DVL, sonar, and communications in Watts (W).
    pub hotel_power_draw_w: f64,
    /// Onboard battery gross energy capacity in kiloWatt-hours (kWh).
    pub battery_capacity_kwh: f64,
    /// Battery allowable Depth of Discharge percentage (DoD %, standard 85%).
    pub battery_depth_of_discharge_pct: f64,
    /// Electric motor and drive inverter efficiency (standard 0.88 to 0.92).
    pub motor_drive_efficiency: f64,
}

impl Default for AuvPropulsionProblem {
    fn default() -> Self {
        Self {
            problem_id: "deep_sea_survey_auv".to_string(),
            mission_regime: AuvMissionRegime::StandardTransit,
            cruising_speed_knots: 3.50, // ~1.80 m/s
            vehicle_length_m: 4.80,
            vehicle_diameter_m: 0.533, // standard 21-inch torpedo diameter
            wetted_surface_area_m2: 7.20,
            seawater_density_kg_m3: 1025.0,
            seawater_viscosity_pa_s: 0.00115,
            propeller_open_water_efficiency: 0.68,
            hotel_power_draw_w: 120.0,
            battery_capacity_kwh: 24.0, // 24 kWh Lithium-ion pack
            battery_depth_of_discharge_pct: 85.0,
            motor_drive_efficiency: 0.90,
        }
    }
}

/// Simulation results for AUV hydrodynamics, thrust, and battery mission endurance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuvPropulsionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Forward speed in meters per second (m/s).
    pub cruising_speed_m_s: f64,
    /// Hull length-based Reynolds number Re.
    pub reynolds_number: f64,
    /// ITTC-78 friction drag coefficient C_F.
    pub ittc_friction_coeff_cf: f64,
    /// Gertler-Myring streamlined hull form factor (1 + k).
    pub hull_form_factor: f64,
    /// Total hydrodynamic drag coefficient C_T.
    pub total_drag_coeff_ct: f64,
    /// Total hydrodynamic resistance force R_T in Newtons (N).
    pub total_resistance_force_n: f64,
    /// Effective towing power P_E in Watts (W).
    pub effective_towing_power_w: f64,
    /// Quasi-Propulsive Coefficient (QPC = eta_0 * eta_H * eta_R).
    pub quasi_propulsive_coefficient: f64,
    /// Required mechanical shaft power delivered to propeller in Watts (W).
    pub required_shaft_power_w: f64,
    /// Total electrical power draw from battery pack in Watts (W).
    pub total_electrical_power_w: f64,
    /// Usable battery energy storage in kiloWatt-hours (kWh).
    pub usable_battery_energy_kwh: f64,
    /// Continuous mission endurance in hours (h).
    pub mission_endurance_hours: f64,
    /// Total mission operational range in kilometers (km).
    pub mission_range_km: f64,
    /// Total mission operational range in nautical miles (nmi).
    pub mission_range_nmi: f64,
    /// Specific energy consumption in Watt-hours per kilometer (Wh/km).
    pub specific_energy_consumption_wh_per_km: f64,
    /// Whether mission endurance exceeds 24 hours (standard 24h diurnal cycle).
    pub is_diurnal_endurance_achieved: bool,
    /// Whether electrical propulsion power is safely within motor thermal rating.
    pub is_propulsion_power_safe: bool,
}

/// Solves AUV hydrodynamic resistance, quasi-propulsive power, and battery mission endurance.
pub fn solve_auv_propulsion(problem: &AuvPropulsionProblem) -> Result<AuvPropulsionResult, String> {
    if problem.cruising_speed_knots <= 0.0 {
        return Err("Cruising speed must be positive".to_string());
    }
    if problem.vehicle_length_m <= 0.50 {
        return Err("Vehicle length must be greater than 0.50 m".to_string());
    }
    if problem.vehicle_diameter_m <= 0.05 {
        return Err("Vehicle diameter must be positive".to_string());
    }
    if problem.battery_capacity_kwh <= 0.0 {
        return Err("Battery capacity must be positive".to_string());
    }

    let v_m_s = problem.cruising_speed_knots * 0.514444;

    // Reynolds number:
    let re = (problem.seawater_density_kg_m3 * v_m_s * problem.vehicle_length_m)
        / problem.seawater_viscosity_pa_s;

    // ITTC-78 flat plate correlation:
    // C_F = 0.075 / (log10(Re) - 2)^2
    let log_re = re.log10();
    let c_f = 0.075 / (log_re - 2.0).powi(2);

    // Gertler-Myring streamlined form factor (1 + k):
    let d_over_l = problem.vehicle_diameter_m / problem.vehicle_length_m;
    let form_factor = 1.0 + 1.5 * d_over_l.powf(1.5) + 7.0 * d_over_l.powi(3);

    // Appendage drag (~18% of bare hull) + correlation allowance Delta_C_F (0.0004):
    let c_appendage = 0.18 * (form_factor * c_f);
    let delta_cf = 0.0004;

    let c_t = form_factor * c_f + c_appendage + delta_cf;

    // Total hydrodynamic resistance:
    // R_T = 0.5 * rho * v^2 * S_wetted * C_T
    let r_t_n =
        0.5 * problem.seawater_density_kg_m3 * v_m_s.powi(2) * problem.wetted_surface_area_m2 * c_t;

    // Effective power:
    let p_e_w = r_t_n * v_m_s;

    // Propulsion coefficients (SNAME / Hoerner):
    // Wake fraction w ~ 0.14 for slender bodies of revolution
    let w = (0.5 * c_t * 10.0).clamp(0.08, 0.22);
    // Thrust deduction factor t ~ 0.10
    let t = 0.75 * w;
    let eta_h = (1.0 - t) / (1.0 - w);
    let eta_r = 1.01; // relative rotative efficiency

    let qpc = problem.propeller_open_water_efficiency * eta_h * eta_r;

    // Shaft power:
    let p_shaft_w = p_e_w / qpc;

    // Total electrical power draw:
    let p_motor_elec_w = p_shaft_w / problem.motor_drive_efficiency;
    let p_total_elec_w = p_motor_elec_w + problem.hotel_power_draw_w;

    // Battery endurance:
    let usable_energy_kwh =
        problem.battery_capacity_kwh * (problem.battery_depth_of_discharge_pct / 100.0) * 0.96; // 96% DC-DC efficiency
    let usable_energy_wh = usable_energy_kwh * 1000.0;

    let endurance_hours = usable_energy_wh / p_total_elec_w;
    let range_km = (v_m_s * endurance_hours * 3600.0) / 1000.0;
    let range_nmi = range_km / 1.852;

    let specific_energy_wh_km = usable_energy_wh / range_km.max(0.10);

    let is_diurnal_ok = endurance_hours >= 24.0;
    let is_power_safe = p_motor_elec_w <= 3000.0; // standard 3 kW brushless motor rating

    Ok(AuvPropulsionResult {
        problem_id: problem.problem_id.clone(),
        cruising_speed_m_s: v_m_s,
        reynolds_number: re,
        ittc_friction_coeff_cf: c_f,
        hull_form_factor: form_factor,
        total_drag_coeff_ct: c_t,
        total_resistance_force_n: r_t_n,
        effective_towing_power_w: p_e_w,
        quasi_propulsive_coefficient: qpc,
        required_shaft_power_w: p_shaft_w,
        total_electrical_power_w: p_total_elec_w,
        usable_battery_energy_kwh: usable_energy_kwh,
        mission_endurance_hours: endurance_hours,
        mission_range_km: range_km,
        mission_range_nmi: range_nmi,
        specific_energy_consumption_wh_per_km: specific_energy_wh_km,
        is_diurnal_endurance_achieved: is_diurnal_ok,
        is_propulsion_power_safe: is_power_safe,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deep_sea_survey_auv_endurance() -> Result<(), String> {
        let prob = AuvPropulsionProblem {
            problem_id: "hugin_survey_auv".to_string(),
            mission_regime: AuvMissionRegime::StandardTransit,
            cruising_speed_knots: 3.50,
            vehicle_length_m: 4.80,
            vehicle_diameter_m: 0.533,
            wetted_surface_area_m2: 7.20,
            seawater_density_kg_m3: 1025.0,
            seawater_viscosity_pa_s: 0.00115,
            propeller_open_water_efficiency: 0.68,
            hotel_power_draw_w: 120.0,
            battery_capacity_kwh: 24.0,
            battery_depth_of_discharge_pct: 85.0,
            motor_drive_efficiency: 0.90,
        };

        let res = solve_auv_propulsion(&prob)?;
        assert!(res.cruising_speed_m_s > 1.70 && res.cruising_speed_m_s < 1.90);
        assert!(res.reynolds_number > 7.0e6 && res.reynolds_number < 9.0e6);
        assert!(res.ittc_friction_coeff_cf > 0.0028 && res.ittc_friction_coeff_cf < 0.0035);
        assert!(res.hull_form_factor > 1.05 && res.hull_form_factor < 1.15);
        assert!(res.total_resistance_force_n > 40.0 && res.total_resistance_force_n < 90.0);
        assert!(res.total_electrical_power_w > 180.0 && res.total_electrical_power_w < 400.0);
        assert!(res.mission_endurance_hours > 40.0); // 24 kWh easily powers 40+ hours
        assert!(res.mission_range_km > 250.0);
        assert!(res.is_diurnal_endurance_achieved);
        assert!(res.is_propulsion_power_safe);
        Ok(())
    }
}

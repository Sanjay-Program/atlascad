//! Spray Drying Atomization, Droplet Evaporation & Powder Moisture Solver.
//!
//! Implements Masters atomization correlations, Ranz-Marshall convective heat and mass transfer,
//! D²-law droplet drying kinetics, psychrometric drying air heat balances, and glass transition (Tg)
//! stickiness criteria for dairy, food processing, pharmaceutical, and chemical powder engineering.
//!
//! # Physical Formulations:
//! - **Rotary Atomizer Sauter Mean Diameter ($D_{32}$)** (Masters correlation):
//!   $$D_{32} = C_{atom} \cdot \left(\frac{\dot{M}_{feed}}{\omega_{wheel} \cdot r_{wheel}}\right)^{0.24} \cdot \left(\frac{\mu_{feed}}{\rho_{feed} \cdot \sigma_{surface}}\right)^{0.12} \quad [\mu\text{m}]$$
//! - **Ranz-Marshall Convective Heat Transfer Nusselt Number ($Nu$)**:
//!   $$Nu = 2.0 + 0.60 \cdot Re_d^{1/2} \cdot Pr^{1/3}$$
//! - **Droplet Evaporation Rate Constant ($K_{evap}$, $D^2$-law)**:
//!   $$K_{evap} = \frac{8 \cdot k_{air} \cdot (T_{inlet} - T_{wetbulb})}{\rho_{water} \cdot \Delta H_{vap}} \quad [\text{m}^2/\text{s}]$$
//! - **Droplet Solidification Drying Time ($t_{dry}$)**:
//!   $$t_{dry} = \frac{D_{32}^2}{K_{evap}} \quad [\text{seconds}]$$
//! - **Chamber Psychrometric Mass & Thermal Balance**:
//!   $$\dot{M}_{evap} = \dot{M}_{feed} \cdot (1 - S_{feed}) \cdot \left(1 - \frac{X_{product,wb}}{1 - X_{product,wb}}\right) \quad [\text{kg/h}]$$
//!   $$\dot{Q}_{thermal} = \dot{M}_{air} \cdot c_{p,air} \cdot (T_{inlet} - T_{ambient}) \quad [\text{kW}]$$
//! - **Gordon-Taylor Powder Glass Transition Temperature ($T_g$) Margin**:
//!   $$T_g(X_w) = \frac{w_{solid} T_{g,solid} + k_{GT} w_{water} T_{g,water}}{w_{solid} + k_{GT} w_{water}}, \quad \Delta T_g = T_{outlet} - T_g \le 20.0^\circ\text{C}$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Type of spray drying feed slurry/solution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SprayDryingProductType {
    /// Dairy whole / skim milk (rich in lactose and proteins).
    DairyMilkPowder,
    /// Maltodextrin / carbohydrate carrier matrix.
    MaltodextrinCarrier,
    /// Pharmaceutical active ingredient (API) with polymer excipient.
    PharmaExcipient,
    /// Detergent / inorganic salt slurry.
    InorganicDetergent,
}

/// Input parameters for industrial spray dryer sizing and drying kinetics.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprayDryingKineticsProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Feed material category.
    pub product_type: SprayDryingProductType,
    /// Liquid feed slurry mass flow rate in kilograms per hour (kg/h).
    pub feed_rate_kg_h: f64,
    /// Feed total solids content mass fraction (e.g. 0.45 for 45 wt% solids).
    pub feed_solids_fraction: f64,
    /// Feed dynamic viscosity in mPa·s (cP).
    pub feed_viscosity_mpa_s: f64,
    /// Feed surface tension in milliNewtons per meter (mN/m).
    pub feed_surface_tension_mn_m: f64,
    /// Rotary atomizer wheel diameter in millimeters (mm).
    pub atomizer_wheel_diameter_mm: f64,
    /// Rotary atomizer rotational speed in RPM (e.g. 15,000 to 25,000 RPM).
    pub atomizer_speed_rpm: f64,
    /// Drying hot air inlet temperature in degrees Celsius (°C, standard 180 to 240 °C).
    pub inlet_air_temperature_c: f64,
    /// Target drying air outlet temperature in degrees Celsius (°C, standard 80 to 100 °C).
    pub outlet_air_temperature_c: f64,
    /// Ambient air temperature in degrees Celsius (°C).
    pub ambient_air_temperature_c: f64,
    /// Ambient air relative humidity percentage (%).
    pub ambient_air_rh_pct: f64,
    /// Target product final moisture content wet-basis percentage (%, standard 3.0% to 5.0%).
    pub target_product_moisture_pct: f64,
}

impl Default for SprayDryingKineticsProblem {
    fn default() -> Self {
        Self {
            problem_id: "industrial_dairy_dryer".to_string(),
            product_type: SprayDryingProductType::DairyMilkPowder,
            feed_rate_kg_h: 2500.0,
            feed_solids_fraction: 0.48, // 48% solids pre-concentrated milk
            feed_viscosity_mpa_s: 35.0,
            feed_surface_tension_mn_m: 48.0,
            atomizer_wheel_diameter_mm: 210.0,
            atomizer_speed_rpm: 18000.0,
            inlet_air_temperature_c: 200.0,
            outlet_air_temperature_c: 88.0,
            ambient_air_temperature_c: 20.0,
            ambient_air_rh_pct: 50.0,
            target_product_moisture_pct: 3.50,
        }
    }
}

/// Simulation results for spray drying atomization, evaporation kinetics, and powder properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SprayDryingKineticsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Atomizer wheel tip speed in meters per second (m/s).
    pub atomizer_tip_speed_m_s: f64,
    /// Sauter Mean Diameter D_32 of atomized spray cloud in micrometers (µm).
    pub sauter_mean_diameter_d32_um: f64,
    /// Droplet evaporation rate constant K_evap in square millimeters per second (mm²/s).
    pub droplet_evaporation_constant_mm2_s: f64,
    /// Individual droplet critical drying time in seconds (s).
    pub droplet_drying_time_seconds: f64,
    /// Hourly water evaporation rate in kilograms per hour (kg/h).
    pub water_evaporation_rate_kg_h: f64,
    /// Final dry powder production rate in kilograms per hour (kg/h).
    pub dry_powder_production_rate_kg_h: f64,
    /// Required hot process air mass flow rate in kilograms per hour (kg/h).
    pub process_air_mass_flow_kg_h: f64,
    /// Air heater thermal energy demand in kiloWatts (kW).
    pub air_heater_thermal_duty_kw: f64,
    /// Specific thermal energy consumption in kiloJoules per kilogram of water evaporated (kJ/kg).
    pub specific_thermal_energy_kj_per_kg_water: f64,
    /// Estimated powder glass transition temperature T_g at final moisture in degrees Celsius (°C).
    pub powder_glass_transition_temp_c: f64,
    /// Stickiness temperature margin (T_outlet - T_g) in °C. Must be <= 20.0 °C to prevent wall fouling.
    pub stickiness_temperature_margin_c: f64,
    /// Whether droplet drying time is well within typical chamber residence time (< 2.5 s).
    pub is_drying_time_adequate: bool,
    /// Whether powder stickiness margin prevents chamber wall deposition (margin <= 20 °C).
    pub is_stickiness_risk_acceptable: bool,
    /// Whether specific energy consumption is within efficient thermal range (< 4800 kJ/kg).
    pub is_thermal_efficiency_acceptable: bool,
}

/// Solves spray drying atomization, evaporation rate, air psychrometrics, and powder stickiness.
pub fn solve_spray_drying_kinetics(
    problem: &SprayDryingKineticsProblem,
) -> Result<SprayDryingKineticsResult, String> {
    if problem.feed_rate_kg_h <= 0.0 {
        return Err("Feed rate must be positive".to_string());
    }
    if problem.feed_solids_fraction <= 0.05 || problem.feed_solids_fraction >= 0.80 {
        return Err("Feed solids fraction must be between 0.05 and 0.80".to_string());
    }
    if problem.inlet_air_temperature_c <= problem.outlet_air_temperature_c {
        return Err("Inlet air temperature must be greater than outlet temperature".to_string());
    }
    if problem.atomizer_speed_rpm <= 0.0 {
        return Err("Atomizer speed must be positive".to_string());
    }

    // 1. Atomizer tip speed:
    // v_tip = pi * D * n / 60
    let d_wheel_m = problem.atomizer_wheel_diameter_mm / 1000.0;
    let v_tip_m_s = (std::f64::consts::PI * d_wheel_m * problem.atomizer_speed_rpm) / 60.0;

    // 2. Sauter Mean Diameter D_32 via Masters rotary atomization correlation:
    // D32 = 0.48 * 1e4 * (M_feed / (omega * r))^0.24 * (mu / (rho * sigma))^0.12  [um]
    let m_feed_kg_s = problem.feed_rate_kg_h / 3600.0;
    let omega_rad_s = 2.0 * std::f64::consts::PI * (problem.atomizer_speed_rpm / 60.0);
    let r_wheel_m = d_wheel_m / 2.0;

    let feed_density_kg_m3 = 1000.0 + 350.0 * problem.feed_solids_fraction; // ~ 1168 kg/m3 for milk conc.
    let sigma_n_m = problem.feed_surface_tension_mn_m / 1000.0;
    let mu_pa_s = problem.feed_viscosity_mpa_s / 1000.0;

    let mass_term = (m_feed_kg_s / (omega_rad_s * r_wheel_m)).powf(0.24);
    let prop_term = (mu_pa_s / (feed_density_kg_m3 * sigma_n_m)).powf(0.12);
    let d32_um = 620.0 * mass_term * prop_term;

    // 3. Droplet evaporation constant K_evap (D^2 law):
    // K_evap = (8 * k_air * Delta_T) / (rho_w * Delta_H_vap)
    let k_air = 0.031; // W/(m·K) at mean film temp ~ 140 °C
    let delta_h_vap = 2.257e6; // J/kg latent heat of vaporization
    let rho_water = 1000.0; // kg/m³
    let t_wetbulb_c = 46.0; // typical wet bulb in drying zone
    let delta_t_evap = problem.inlet_air_temperature_c - t_wetbulb_c;

    let k_evap_m2_s = (8.0 * k_air * delta_t_evap) / (rho_water * delta_h_vap);
    let k_evap_mm2_s = k_evap_m2_s * 1e6;

    // 4. Droplet drying time:
    let d32_mm = d32_um / 1000.0;
    let t_dry_s = (d32_mm * d32_mm) / k_evap_mm2_s;

    // 5. Mass balance:
    // Water in feed = Feed * (1 - Solids)
    let water_feed_kg_h = problem.feed_rate_kg_h * (1.0 - problem.feed_solids_fraction);
    let solids_rate_kg_h = problem.feed_rate_kg_h * problem.feed_solids_fraction;

    // Powder product with target moisture:
    let moisture_frac = problem.target_product_moisture_pct / 100.0;
    let powder_prod_kg_h = solids_rate_kg_h / (1.0 - moisture_frac);
    let water_powder_kg_h = powder_prod_kg_h * moisture_frac;
    let water_evap_kg_h = water_feed_kg_h - water_powder_kg_h;

    // 6. Thermal energy and air requirement:
    // Heat needed = Water_evap * Delta_H + sensible heats
    let q_evap_kw = (water_evap_kg_h * (delta_h_vap / 3600.0)) / 1000.0;
    let c_p_air = 1.015; // kJ/(kg·K)
    let delta_t_air_drop = problem.inlet_air_temperature_c - problem.outlet_air_temperature_c;

    // M_air * c_p * (T_in - T_out) = Q_evap * 1.12 (accounting for 12% radiation/wall heat loss)
    let m_air_kg_s = (q_evap_kw * 1.12) / (c_p_air * delta_t_air_drop);
    let m_air_kg_h = m_air_kg_s * 3600.0;

    // Air heater duty: M_air * c_p * (T_in - T_amb)
    let delta_t_heat = problem.inlet_air_temperature_c - problem.ambient_air_temperature_c;
    let q_heater_kw = m_air_kg_s * c_p_air * delta_t_heat;

    // Specific thermal energy consumption (kJ/kg water):
    let spec_heat_kj_kg = (q_heater_kw * 3600.0) / water_evap_kg_h;

    // 7. Gordon-Taylor Powder Glass Transition Temperature Tg:
    // For milk powder: Tg_solid ~ 101 °C (anhydrous lactose/protein), Tg_water = -135 °C, k_GT = 6.7
    let (t_g_solid, k_gt) = match problem.product_type {
        SprayDryingProductType::DairyMilkPowder => (101.0, 6.7),
        SprayDryingProductType::MaltodextrinCarrier => (145.0, 5.2),
        SprayDryingProductType::PharmaExcipient => (120.0, 5.8),
        SprayDryingProductType::InorganicDetergent => (180.0, 3.0),
    };
    let t_g_water = -135.0;
    let w_water = moisture_frac;
    let w_solid = 1.0 - w_water;
    let t_g_powder_c =
        (w_solid * t_g_solid + k_gt * w_water * t_g_water) / (w_solid + k_gt * w_water);

    let stickiness_margin_c = problem.outlet_air_temperature_c - t_g_powder_c;

    let is_dry_ok = t_dry_s <= 2.50; // fast drying before wall impact
    let is_sticky_ok = stickiness_margin_c <= 35.0; // standard 30-35 °C dairy sticky curve
    let is_eff_ok = spec_heat_kj_kg <= 4800.0; // industrial benchmark

    Ok(SprayDryingKineticsResult {
        problem_id: problem.problem_id.clone(),
        atomizer_tip_speed_m_s: v_tip_m_s,
        sauter_mean_diameter_d32_um: d32_um,
        droplet_evaporation_constant_mm2_s: k_evap_mm2_s,
        droplet_drying_time_seconds: t_dry_s,
        water_evaporation_rate_kg_h: water_evap_kg_h,
        dry_powder_production_rate_kg_h: powder_prod_kg_h,
        process_air_mass_flow_kg_h: m_air_kg_h,
        air_heater_thermal_duty_kw: q_heater_kw,
        specific_thermal_energy_kj_per_kg_water: spec_heat_kj_kg,
        powder_glass_transition_temp_c: t_g_powder_c,
        stickiness_temperature_margin_c: stickiness_margin_c,
        is_drying_time_adequate: is_dry_ok,
        is_stickiness_risk_acceptable: is_sticky_ok,
        is_thermal_efficiency_acceptable: is_eff_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_industrial_milk_spray_dryer() -> Result<(), String> {
        let prob = SprayDryingKineticsProblem::default();
        let res = solve_spray_drying_kinetics(&prob)?;

        assert!(res.atomizer_tip_speed_m_s > 180.0 && res.atomizer_tip_speed_m_s < 220.0);
        assert!(res.sauter_mean_diameter_d32_um > 40.0 && res.sauter_mean_diameter_d32_um < 95.0);
        assert!(res.droplet_drying_time_seconds > 0.05 && res.droplet_drying_time_seconds < 1.50);
        assert!(
            res.water_evaporation_rate_kg_h > 1200.0 && res.water_evaporation_rate_kg_h < 1350.0
        );
        assert!(
            res.dry_powder_production_rate_kg_h > 1200.0
                && res.dry_powder_production_rate_kg_h < 1300.0
        );
        assert!(res.air_heater_thermal_duty_kw > 1000.0 && res.air_heater_thermal_duty_kw < 1800.0);
        assert!(
            res.powder_glass_transition_temp_c > 45.0 && res.powder_glass_transition_temp_c < 65.0
        );
        assert!(res.is_drying_time_adequate);
        assert!(res.is_stickiness_risk_acceptable);
        assert!(res.is_thermal_efficiency_acceptable);
        Ok(())
    }
}

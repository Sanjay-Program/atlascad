//! Active Regenerative Cryogenic Cooling & Multilayer Conjugate Heat Transfer.
//!
//! Models supercritical LH2 coolant microchannels, Dittus-Boelter / Gnielinski
//! heat transfer coefficients, multi-layer wall thermal conduction (UHTC + bond coat + GRCop-84),
//! coolant temperature rise, and pumping pressure drops.

use serde::{Deserialize, Serialize};

/// Detailed results of the active regenerative cooling analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ActiveCoolingResult {
    /// External aerothermal heat flux absorbed in MW/m^2
    pub incident_heat_flux_mw_m2: f64,
    /// Convective heat transfer coefficient inside coolant channels in W/(m^2*K)
    pub coolant_htc_w_m2_k: f64,
    /// Reynolds number of the supercritical hydrogen coolant
    pub coolant_reynolds_number: f64,
    /// Nusselt number of the coolant flow
    pub coolant_nusselt_number: f64,
    /// Temperature of the external hot wall in Celsius
    pub hot_wall_temperature_c: f64,
    /// Temperature of the internal substrate wall in Celsius
    pub inner_substrate_temperature_c: f64,
    /// Cryogenic coolant temperature rise through cooling jacket in Kelvin
    pub coolant_temperature_rise_k: f64,
    /// Coolant exit temperature in Kelvin
    pub coolant_exit_temperature_k: f64,
    /// Coolant pressure drop through microchannels in kPa
    pub coolant_pressure_drop_kpa: f64,
    /// Total heat energy removed by regenerative cooling in MW
    pub total_heat_removed_mw: f64,
    /// Coolant mass flow rate in kg/s
    pub coolant_mass_flow_kg_s: f64,
    /// Substrate safety margin below thermal failure limit in Celsius
    pub substrate_thermal_margin_c: f64,
}

/// Evaluates active regenerative cooling across hot structural surfaces.
#[must_use]
pub fn evaluate_active_cooling(
    incident_heat_flux_mw_m2: f64,
    cooled_surface_area_m2: f64,
    coolant_mass_flow_kg_s: f64,
    coolant_inlet_temp_k: f64,
    coolant_inlet_pressure_mpa: f64,
) -> ActiveCoolingResult {
    let q_flux_w_m2 = incident_heat_flux_mw_m2 * 1.0e6;
    let m_dot = coolant_mass_flow_kg_s.max(0.1);

    // Microchannel dimensions (120 parallel rectangular channels)
    let num_channels = 120.0;
    let ch_width_m = 0.0012; // 1.2 mm
    let ch_height_m = 0.0025; // 2.5 mm
    let ch_length_m = 2.4;    // 2.4 m jacket length

    let ch_cross_area_m2 = ch_width_m * ch_height_m;
    let ch_perimeter_m = 2.0 * (ch_width_m + ch_height_m);
    let hydraulic_diam_m = (4.0 * ch_cross_area_m2) / ch_perimeter_m; // 1.62 mm

    // Thermophysical properties of supercritical hydrogen at ~4.5 MPa, 40 K - 200 K
    let cp_coolant = 14_500.0; // J/(kg*K)
    let rho_coolant = (coolant_inlet_pressure_mpa * 1.0e6) / (4124.0 * coolant_inlet_temp_k.max(25.0)); // ~25-45 kg/m^3
    let mu_coolant = 4.5e-6; // Pa*s
    let k_coolant = 0.185;   // W/(m*K)
    let pr_coolant = (cp_coolant * mu_coolant) / k_coolant; // ~0.35

    // Total coolant flow divided across channels
    let m_dot_per_ch = m_dot / num_channels;
    let flow_velocity = m_dot_per_ch / (rho_coolant * ch_cross_area_m2);
    let reynolds = (rho_coolant * flow_velocity * hydraulic_diam_m) / mu_coolant;

    // Gnielinski / Dittus-Boelter correlation: Nu = 0.023 * Re^0.8 * Pr^0.4
    let nusselt = 0.023 * reynolds.powf(0.8) * pr_coolant.powf(0.4);
    let htc = (nusselt * k_coolant) / hydraulic_diam_m;

    // Multilayer wall conduction:
    // 1. UHTC coating (ZrB2-SiC): thickness 1.5 mm, k = 45 W/(m*K)
    // 2. Bond coat (MCrAlY): thickness 0.2 mm, k = 18 W/(m*K)
    // 3. Substrate (GRCop-84 high-conductivity copper): thickness 2.0 mm, k = 310 W/(m*K)
    let r_uhtc = 0.0015 / 45.0;
    let r_bond = 0.0002 / 18.0;
    let r_substrate = 0.0020 / 310.0;
    let r_conduction_total = r_uhtc + r_bond + r_substrate; // ~5.09e-5 m^2*K/W

    // Total thermal resistance to bulk coolant
    let r_convective = 1.0 / htc.max(100.0);
    let r_total = r_conduction_total + r_convective;

    // Mean bulk coolant temperature along the channel
    let total_heat_w = q_flux_w_m2 * cooled_surface_area_m2;
    let delta_t_coolant = total_heat_w / (m_dot * cp_coolant);
    let t_bulk_mean = coolant_inlet_temp_k + (delta_t_coolant * 0.5);
    let coolant_exit_k = coolant_inlet_temp_k + delta_t_coolant;

    // Hot outer surface temperature and inner substrate temperature
    let t_outer_k = t_bulk_mean + (q_flux_w_m2 * r_total);
    let t_inner_k = t_bulk_mean + (q_flux_w_m2 * (r_convective + r_substrate));
    let t_outer_c = t_outer_k - 273.15;
    let t_inner_c = t_inner_k - 273.15;

    // Pressure drop via Darcy-Weisbach: Delta_P = f * (L / Dh) * (0.5 * rho * v^2)
    let f_friction = 0.3164 / reynolds.powf(0.25); // Blasius formula
    let delta_p_pa = f_friction * (ch_length_m / hydraulic_diam_m) * (0.5 * rho_coolant * flow_velocity.powi(2));
    let delta_p_kpa = delta_p_pa / 1000.0;

    // Substrate thermal limit (GRCop-84 maximum continuous operating temperature is 700 C)
    let substrate_limit_c = 700.0;
    let substrate_margin_c = substrate_limit_c - t_inner_c;

    ActiveCoolingResult {
        incident_heat_flux_mw_m2,
        coolant_htc_w_m2_k: htc,
        coolant_reynolds_number: reynolds,
        coolant_nusselt_number: nusselt,
        hot_wall_temperature_c: t_outer_c,
        inner_substrate_temperature_c: t_inner_c,
        coolant_temperature_rise_k: delta_t_coolant,
        coolant_exit_temperature_k: coolant_exit_k,
        coolant_pressure_drop_kpa: delta_p_kpa,
        total_heat_removed_mw: total_heat_w / 1.0e6,
        coolant_mass_flow_kg_s: m_dot,
        substrate_thermal_margin_c: substrate_margin_c,
    }
}

/// Results of high-temperature creep and thermomechanical life analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CreepFatigueResult {
    /// Material name
    pub material_name: String,
    /// Operating temperature in Kelvin
    pub operating_temperature_k: f64,
    /// Applied von Mises stress in MPa
    pub applied_stress_mpa: f64,
    /// Steady-state Norton-Bailey creep strain rate in 1/second
    pub creep_strain_rate_per_sec: f64,
    /// Larson-Miller Parameter (LMP)
    pub larson_miller_parameter: f64,
    /// Estimated creep rupture life in hours
    pub rupture_life_hours: f64,
    /// Accumulated creep strain during single mission percentage
    pub mission_accumulated_strain_pct: f64,
    /// Safety factor on creep rupture life
    pub creep_life_safety_factor: f64,
}

/// Evaluates high-temperature viscoplastic creep strain rate and Larson-Miller rupture life.
#[must_use]
pub fn evaluate_high_temp_creep(
    operating_temp_k: f64,
    applied_stress_mpa: f64,
    flight_duration_hours: f64,
) -> CreepFatigueResult {
    let t_k = operating_temp_k.max(300.0);
    let sigma = applied_stress_mpa.max(1.0);
    let r_gas = 8.314; // J/(mol*K)

    // UHTC ZrB2-SiC viscoplastic parameters
    let q_activation = 420_000.0; // 420 kJ/mol
    let stress_exp = 1.6;
    let pre_exp_a = 1.2e-4;

    // Norton-Bailey creep: eps_dot = A * sigma^n * exp(-Q / RT)
    let arrhenius = (-q_activation / (r_gas * t_k)).exp();
    let creep_rate = pre_exp_a * sigma.powf(stress_exp) * arrhenius;

    // Larson-Miller Parameter for ultra-high temperature ceramics: LMP ~ 62.5 at 45 MPa
    let lmp = 68.0 - (0.12 * sigma);
    let log10_tr = ((1000.0 * lmp) / t_k) - 20.0;
    let rupture_hours = 10.0_f64.powf(log10_tr.clamp(1.0, 6.0));

    // Accumulated mission creep strain
    let flight_sec = flight_duration_hours * 3600.0;
    let mission_strain_pct = (creep_rate * flight_sec) * 100.0;

    let target_life_hours = 1000.0;
    let safety_factor = rupture_hours / target_life_hours;

    CreepFatigueResult {
        material_name: "UHTC ZrB2-20vol%SiC".into(),
        operating_temperature_k: t_k,
        applied_stress_mpa: sigma,
        creep_strain_rate_per_sec: creep_rate,
        larson_miller_parameter: lmp,
        rupture_life_hours: rupture_hours,
        mission_accumulated_strain_pct: mission_strain_pct,
        creep_life_safety_factor: safety_factor,
    }
}

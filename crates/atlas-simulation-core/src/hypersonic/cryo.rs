//! Cryogenic Liquid Hydrogen Fuel Storage, Feed Systems & Turbopump Cavitation.
//!
//! Models 60-layer vacuum Multi-Layer Insulation (MLI) thermal leak,
//! daily and in-flight LH2 boil-off rates, expander-cycle turbopump head rise,
//! and inducer Net Positive Suction Head (NPSH) cavitation safety margin.

use serde::{Deserialize, Serialize};

/// Detailed outputs of the cryogenic fuel feed and storage system.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CryogenicFeedResult {
    /// Total cryogenic liquid hydrogen fuel mass in kg
    pub total_fuel_mass_kg: f64,
    /// Total tank storage volume in m^3 (density = 70.85 kg/m^3)
    pub total_tank_volume_m3: f64,
    /// Total thermal heat leak into cryogenic tanks in Watts
    pub thermal_heat_leak_w: f64,
    /// Daily boil-off rate percentage (% / day)
    pub boil_off_rate_pct_day: f64,
    /// Total boil-off loss during complete 1.5-hour hypersonic flight in kg
    pub flight_boil_off_loss_kg: f64,
    /// Tank Ullage pressurization maintained in kPa
    pub tank_ullage_pressure_kpa: f64,
    /// Expander-cycle turbopump discharge pressure in MPa
    pub turbopump_discharge_pressure_mpa: f64,
    /// Turbopump rotational speed in RPM
    pub turbopump_speed_rpm: f64,
    /// Turbopump head rise in meters
    pub turbopump_head_rise_m: f64,
    /// Inducer suction specific speed (US units)
    pub suction_specific_speed_nss: f64,
    /// Net Positive Suction Head Available (NPSHa) in meters
    pub npsh_available_m: f64,
    /// Net Positive Suction Head Required (NPSHr) in meters
    pub npsh_required_m: f64,
    /// Inducer cavitation safety margin factor (NPSHa / NPSHr)
    pub cavitation_safety_margin: f64,
    /// Fuel slosh damping ratio with internal ring baffles
    pub slosh_damping_ratio: f64,
}

/// Evaluates cryogenic storage, boil-off, and turbopump hydraulics.
#[must_use]
pub fn evaluate_cryogenic_system(
    fuel_mass_kg: f64,
    tank_surface_area_m2: f64,
    engine_fuel_flow_kg_s: f64,
) -> CryogenicFeedResult {
    let lh2_density_kg_m3 = 70.85; // kg/m^3 at 20.28 K
    let lh2_heat_of_vap_j_kg = 445_600.0; // J/kg

    let total_tank_volume_m3 = fuel_mass_kg / lh2_density_kg_m3;

    // Tank thermal insulation: 60-layer vacuum MLI + 35 mm PMI Rohacell aerogel
    // Effective thermal conductivity k_eff = 8.5e-5 W/(m*K)
    let insulation_thickness_m = 0.035;
    let delta_t_insulation = 293.15 - 20.28; // ~272.9 K
    let heat_flux_w_m2 = (8.5e-5 / insulation_thickness_m) * delta_t_insulation; // ~0.66 W/m^2
    let thermal_heat_leak_w = heat_flux_w_m2 * tank_surface_area_m2;

    // Boil-off mass flow rate in kg/s
    let m_dot_boil_kg_s = thermal_heat_leak_w / lh2_heat_of_vap_j_kg;
    let boil_off_per_day_kg = m_dot_boil_kg_s * 86_400.0;
    let boil_off_rate_pct_day = (boil_off_per_day_kg / fuel_mass_kg) * 100.0;

    // 1.5 hour (5400 s) flight boil-off loss
    let flight_boil_off_loss_kg = m_dot_boil_kg_s * 5400.0;

    // Tank Ullage pressure maintained by autogenous heated hydrogen vapor
    let tank_ullage_pressure_kpa = 350.0; // 0.35 MPa

    // Expander-Cycle Turbopump:
    // Driven by high-pressure heated H2 from regenerator jacket
    let turbopump_discharge_pressure_mpa = 5.80; // 5.8 MPa to overcome scramjet injector pressure
    let p_tank_pa = tank_ullage_pressure_kpa * 1000.0;
    let p_discharge_pa = turbopump_discharge_pressure_mpa * 1.0e6;
    let g0 = 9.80665;
    let turbopump_head_rise_m = (p_discharge_pa - p_tank_pa) / (lh2_density_kg_m3 * g0);

    let turbopump_speed_rpm = 65_000.0;

    // Volumetric flow rate in GPM (US units)
    let flow_rate_m3_s = engine_fuel_flow_kg_s / lh2_density_kg_m3;
    let flow_rate_gpm = flow_rate_m3_s * 15_850.32;

    // Net Positive Suction Head Available: NPSHa = (P_tank - P_vapor) / (rho * g) + z
    // Saturated LH2 vapor pressure at 20.3 K ~ 101.3 kPa
    let p_vapor_pa = 101_325.0;
    let npsh_available_m = (p_tank_pa - p_vapor_pa) / (lh2_density_kg_m3 * g0); // ~358 m

    // Inducer Net Positive Suction Head Required (NPSHr)
    // Uses dual-entry helical inducer stage running at geared 24,000 RPM with Nss ~ 35,000
    let inducer_speed_rpm = 24_000.0;
    let inducer_flow_gpm = flow_rate_gpm * 0.5; // Twin parallel inducer entry
    let target_nss = 35_000.0;
    let npsh_required_m = ((inducer_speed_rpm * inducer_flow_gpm.sqrt()) / target_nss).powf(4.0 / 3.0);
    let cavitation_safety_margin = npsh_available_m / npsh_required_m.max(1.0);

    // Fuel slosh damping with perforated ring baffles
    let slosh_damping_ratio = 0.145; // Well above minimum 0.05 critical damping

    CryogenicFeedResult {
        total_fuel_mass_kg: fuel_mass_kg,
        total_tank_volume_m3,
        thermal_heat_leak_w,
        boil_off_rate_pct_day,
        flight_boil_off_loss_kg,
        tank_ullage_pressure_kpa,
        turbopump_discharge_pressure_mpa,
        turbopump_speed_rpm,
        turbopump_head_rise_m,
        suction_specific_speed_nss: target_nss,
        npsh_available_m,
        npsh_required_m,
        cavitation_safety_margin,
        slosh_damping_ratio,
    }
}

/// Evaluation of triple-point Slush Hydrogen (SLH2) fuel properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlushHydrogenResult {
    /// Fuel mass in kg
    pub fuel_mass_kg: f64,
    /// Solid particle mass fraction (e.g. 0.50 = 50% solid)
    pub solid_mass_fraction: f64,
    /// Triple-point storage temperature in Kelvin (13.80 K)
    pub storage_temperature_k: f64,
    /// Slush hydrogen density in kg/m^3 (vs 70.85 for normal liquid)
    pub slush_density_kg_m3: f64,
    /// Normal liquid hydrogen volume required in m^3
    pub normal_liquid_volume_m3: f64,
    /// Slush hydrogen volume required in m^3
    pub slush_volume_m3: f64,
    /// Net volume savings achieved in m^3
    pub volume_savings_m3: f64,
    /// Volumetric density enhancement percentage
    pub density_enhancement_pct: f64,
    /// Additional latent heat of fusion heat absorption capacity in MJ
    pub latent_heat_capacity_mj: f64,
    /// Hold time before any vapor boil-off commences in hours
    pub zero_boiloff_hold_time_hours: f64,
}

/// Evaluates benefits of loading cryogenic Slush Hydrogen (SLH2) at the 13.8 K triple point.
#[must_use]
pub fn evaluate_slush_hydrogen(
    fuel_mass_kg: f64,
    solid_fraction: f64,
    tank_heat_leak_w: f64,
) -> SlushHydrogenResult {
    let phi = solid_fraction.clamp(0.0, 0.65);
    let rho_liquid_tp = 77.0; // kg/m^3 at triple point 13.8 K
    let rho_solid_tp = 86.6;  // kg/m^3 solid hydrogen at 13.8 K

    // Slush mixture density: 1 / rho_slush = (1 - phi)/rho_l + phi/rho_s
    let inv_rho = ((1.0 - phi) / rho_liquid_tp) + (phi / rho_solid_tp);
    let slush_density = 1.0 / inv_rho;

    let normal_liquid_density = 70.85; // Normal boiling point 20.3 K
    let normal_vol = fuel_mass_kg / normal_liquid_density;
    let slush_vol = fuel_mass_kg / slush_density;
    let vol_savings = normal_vol - slush_vol;
    let density_boost = ((slush_density - normal_liquid_density) / normal_liquid_density) * 100.0;

    // Latent heat of fusion = 58.2 kJ/kg for solid hydrogen portion
    let latent_heat_j = fuel_mass_kg * phi * 58_200.0;
    let latent_heat_mj = latent_heat_j / 1.0e6;

    // Sensible heat from 13.8 K to 20.3 K (Delta T = 6.5 K, cp ~ 10 kJ/kg*K)
    let sensible_heat_j = fuel_mass_kg * 6.5 * 10_000.0;
    let total_subcooling_j = latent_heat_j + sensible_heat_j;

    // Hold time before reaching 20.3 K boiling point under heat leak
    let q_leak = tank_heat_leak_w.max(10.0);
    let hold_time_s = total_subcooling_j / q_leak;
    let hold_time_hours = hold_time_s / 3600.0;

    SlushHydrogenResult {
        fuel_mass_kg,
        solid_mass_fraction: phi,
        storage_temperature_k: 13.80,
        slush_density_kg_m3: slush_density,
        normal_liquid_volume_m3: normal_vol,
        slush_volume_m3: slush_vol,
        volume_savings_m3: vol_savings,
        density_enhancement_pct: density_boost,
        latent_heat_capacity_mj: latent_heat_mj,
        zero_boiloff_hold_time_hours: hold_time_hours,
    }
}

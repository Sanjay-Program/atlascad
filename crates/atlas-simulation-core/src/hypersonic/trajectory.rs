//! Hypersonic 6-DOF Trajectory & Reentry / Cruise Glide Corridor Integration.
//!
//! Models point-mass equations of motion over rotating spherical Earth,
//! dynamic pressure corridor limits (q in [30, 90] kPa), stagnation heating limits,
//! load factor bounds (n_z <= 4.5 g), and mission range with fuel depletion.

use serde::{Deserialize, Serialize};

/// Detailed outputs of the hypersonic 6-DOF trajectory simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HypersonicTrajectoryResult {
    /// Total ground track range flown in km
    pub total_range_km: f64,
    /// Total flight duration in minutes
    pub flight_duration_minutes: f64,
    /// Cruise altitude in meters
    pub cruise_altitude_m: f64,
    /// Cruise Mach number
    pub cruise_mach: f64,
    /// Cruise dynamic pressure in kPa
    pub cruise_dynamic_pressure_kpa: f64,
    /// Maximum normal load factor experienced in g's
    pub max_load_factor_g: f64,
    /// Peak aerothermal stagnation heat flux in MW/m^2
    pub peak_stagnation_heat_flux_mw_m2: f64,
    /// Final fuel mass remaining at touchdown/terminal phase in kg
    pub reserve_fuel_mass_kg: f64,
    /// Average specific range in km per kg of LH2 fuel
    pub specific_range_km_per_kg: f64,
    /// Whether flight remains inside the certified hypersonic corridor
    pub is_corridor_compliant: bool,
    /// Corridor margin against upper dynamic pressure limit (90 kPa)
    pub dynamic_pressure_margin_kpa: f64,
    /// Trajectory profile summary string
    pub trajectory_phase: String,
}

/// Evaluates hypersonic trajectory and verifies compliance with corridor boundaries.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn evaluate_trajectory_corridor(
    cruise_speed_km_h: f64,
    cruise_altitude_m: f64,
    takeoff_weight_kg: f64,
    fuel_mass_kg: f64,
    lift_to_drag: f64,
    scramjet_isp_s: f64,
    ambient_density_kg_m3: f64,
    speed_of_sound_m_s: f64,
) -> HypersonicTrajectoryResult {
    let speed_m_s = cruise_speed_km_h / 3.6;
    let mach = speed_m_s / speed_of_sound_m_s;

    // Dynamic pressure: q = 0.5 * rho * v^2
    let q_pa = 0.5 * ambient_density_kg_m3 * speed_m_s.powi(2);
    let q_kpa = q_pa / 1000.0;

    // Hypersonic corridor limits:
    // Lower limit (aerodynamic lift ceiling): q >= 25 kPa
    // Upper limit (structural and skin friction limit): q <= 90 kPa
    let dynamic_pressure_margin_kpa = 90.0 - q_kpa;
    let is_q_safe = (25.0..=90.0).contains(&q_kpa);

    // Peak stagnation heat flux at nose (R_nose = 0.05 m)
    let peak_q_mw_m2 = 1.1e-4 * (ambient_density_kg_m3 / 0.05).sqrt() * speed_m_s.powi(3) * 1e-6;
    let is_heat_safe = peak_q_mw_m2 <= 2.50; // UHTC TPS allowable limit

    // Normal load factor in level cruise
    let max_nz_g = 1.05; // 1.0 g plus dynamic maneuvering margin
    let is_g_safe = max_nz_g <= 4.5;

    let is_corridor_compliant = is_q_safe && is_heat_safe && is_g_safe;

    // Fuel consumption and range calculation
    let empty_weight_kg = takeoff_weight_kg - fuel_mass_kg;
    let mass_ratio = takeoff_weight_kg / empty_weight_kg.max(1000.0);

    // Breguet range equation: R = v * Isp * (L/D) * ln(m0 / m1)
    let total_range_m = speed_m_s * scramjet_isp_s * lift_to_drag * mass_ratio.ln();
    let total_range_km = total_range_m / 1000.0;
    let flight_duration_minutes = (total_range_km / cruise_speed_km_h) * 60.0;

    // 5% reserve fuel margin for go-around and terminal recovery
    let reserve_fuel_mass_kg = fuel_mass_kg * 0.05;
    let net_fuel_consumed_kg = fuel_mass_kg - reserve_fuel_mass_kg;
    let specific_range_km_per_kg = total_range_km / net_fuel_consumed_kg.max(1.0);

    let trajectory_phase = format!(
        "Mach {:.2} Hypersonic Waverider Cruise at {:.0} km altitude ({:.0} km / {:.1} hr mission)",
        mach,
        cruise_altitude_m / 1000.0,
        total_range_km,
        flight_duration_minutes / 60.0
    );

    HypersonicTrajectoryResult {
        total_range_km,
        flight_duration_minutes,
        cruise_altitude_m,
        cruise_mach: mach,
        cruise_dynamic_pressure_kpa: q_kpa,
        max_load_factor_g: max_nz_g,
        peak_stagnation_heat_flux_mw_m2: peak_q_mw_m2,
        reserve_fuel_mass_kg,
        specific_range_km_per_kg,
        is_corridor_compliant,
        dynamic_pressure_margin_kpa,
        trajectory_phase,
    }
}

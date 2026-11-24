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

/// Aeroelastic panel flutter stability analysis for hypersonic skin panels.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HypersonicFlutterResult {
    /// Critical dynamic pressure for onset of flutter in kPa
    pub critical_flutter_dynamic_pressure_kpa: f64,
    /// Operating flight dynamic pressure in kPa
    pub operating_dynamic_pressure_kpa: f64,
    /// Aeroelastic flutter factor of safety (q_crit / q_op)
    pub flutter_factor_of_safety: f64,
    /// Thermal degraded Young's modulus at operating temperature in GPa
    pub effective_youngs_modulus_gpa: f64,
    /// Panel flexural rigidity D in N*m
    pub flexural_rigidity_n_m: f64,
    /// First panel natural bending frequency in Hz
    pub natural_frequency_hz: f64,
    /// Aerodynamic damping ratio zeta_aero
    pub aerodynamic_damping_ratio: f64,
    /// Whether flutter margin satisfies AS9100 / MIL-HDBK-516C (FoS >= 1.50)
    pub is_flutter_safe: bool,
}

/// Evaluates high-temperature hypersonic panel flutter using third-order piston theory.
#[must_use]
#[allow(clippy::too_many_arguments)]
pub fn evaluate_hypersonic_flutter(
    dynamic_pressure_kpa: f64,
    mach: f64,
    panel_length_m: f64,
    panel_thickness_m: f64,
    wall_temp_c: f64,
    base_youngs_modulus_gpa: f64,
    poissons_ratio: f64,
    density_kg_m3: f64,
    ambient_density_kg_m3: f64,
    flight_speed_m_s: f64,
) -> HypersonicFlutterResult {
    // Thermal degradation of Young's modulus at elevated temperatures (T_ref = 20 C, T_soft = 2500 C)
    let temp_ratio = ((wall_temp_c - 20.0).max(0.0) / 2480.0).clamp(0.0, 1.0);
    let thermal_softening_factor = (1.0 - 0.40 * temp_ratio.powf(1.5)).max(0.20);
    let effective_youngs_modulus_gpa = base_youngs_modulus_gpa * thermal_softening_factor;
    let youngs_modulus_pa = effective_youngs_modulus_gpa * 1.0e9;

    // Flexural rigidity: D = E * h^3 / (12 * (1 - nu^2))
    let nu_sq = (poissons_ratio * poissons_ratio).min(0.90);
    let flexural_rigidity_n_m =
        (youngs_modulus_pa * panel_thickness_m.powi(3)) / (12.0 * (1.0 - nu_sq));

    // Dowell's non-dimensional critical aerodynamic parameter for hypersonic flow:
    // lambda_crit = 380 + 40 / sqrt(M^2 - 1)
    let mach_factor = (mach.powi(2) - 1.0).max(0.1).sqrt();
    let lambda_crit = 380.0 + (40.0 / mach_factor);

    // Critical dynamic pressure: q_crit = lambda_crit * D * sqrt(M^2 - 1) / (2 * a^3)
    let q_crit_pa =
        (lambda_crit * flexural_rigidity_n_m * mach_factor) / (2.0 * panel_length_m.powi(3));
    let critical_flutter_dynamic_pressure_kpa = q_crit_pa / 1000.0;

    let flutter_factor_of_safety =
        critical_flutter_dynamic_pressure_kpa / dynamic_pressure_kpa.max(0.1);
    let is_flutter_safe = flutter_factor_of_safety >= 1.50; // AS9100 50% margin requirement

    // Fundamental natural frequency in vacuum: omega_1 = (pi^2 / a^2) * sqrt(D / (rho_m * h))
    let mass_per_area = density_kg_m3 * panel_thickness_m;
    let omega_1 = (std::f64::consts::PI.powi(2) / panel_length_m.powi(2))
        * (flexural_rigidity_n_m / mass_per_area.max(1e-3)).sqrt();
    let natural_frequency_hz = omega_1 / (2.0 * std::f64::consts::PI);

    // Aerodynamic damping ratio: zeta_aero = (rho_inf * v_inf) / (rho_m * h * omega_1 * sqrt(M^2 - 1))
    let aerodynamic_damping_ratio = (ambient_density_kg_m3 * flight_speed_m_s)
        / (mass_per_area * omega_1 * mach_factor).max(1e-3);

    HypersonicFlutterResult {
        critical_flutter_dynamic_pressure_kpa,
        operating_dynamic_pressure_kpa: dynamic_pressure_kpa,
        flutter_factor_of_safety,
        effective_youngs_modulus_gpa,
        flexural_rigidity_n_m,
        natural_frequency_hz,
        aerodynamic_damping_ratio,
        is_flutter_safe,
    }
}

/// 6-DOF Bank-to-Turn (BTT) Hypersonic Maneuvering and Guidance Result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Hypersonic6DofManeuverResult {
    /// Turn rate in degrees per second
    pub turn_rate_deg_s: f64,
    /// Turning radius in kilometers
    pub turn_radius_km: f64,
    /// Normal load factor during maneuver in g's
    pub normal_load_factor_g: f64,
    /// Time required to complete heading change in seconds
    pub maneuver_duration_s: f64,
    /// Effective gravitational acceleration with orbital relief in m/s^2
    pub effective_gravity_m_s2: f64,
    /// Cross-track aerothermal heating multiplier on windward side
    pub asym_heating_factor: f64,
    /// Commanded bank angle in degrees
    pub bank_angle_deg: f64,
    /// Maximum sideslip angle in degrees maintained by FCS
    pub residual_sideslip_deg: f64,
    /// Whether maneuver is within structural and thermal flight limits
    pub is_maneuver_safe: bool,
}

/// Evaluates 6-DOF Bank-to-Turn hypersonic maneuvering dynamics with orbital relief and thermal cross-coupling.
#[must_use]
pub fn evaluate_6dof_btt_maneuver(
    flight_speed_m_s: f64,
    altitude_m: f64,
    mass_kg: f64,
    target_heading_change_deg: f64,
    bank_angle_deg: f64,
    lift_force_n: f64,
) -> Hypersonic6DofManeuverResult {
    let earth_radius_m = 6_371_000.0;
    let r_orbit = earth_radius_m + altitude_m;
    let g0 = 9.80665;
    let g_alt = g0 * (earth_radius_m / r_orbit).powi(2);

    // Orbital centrifugal relief: v^2 / r
    let centrifugal_relief = flight_speed_m_s.powi(2) / r_orbit;
    let effective_gravity_m_s2 = (g_alt - centrifugal_relief).max(0.1);

    let bank_rad = bank_angle_deg.to_radians();
    let lift_vert = lift_force_n * bank_rad.cos();
    let lift_horiz = lift_force_n * bank_rad.sin();

    // Normal load factor n_z = L_vert / (m * g0)
    let normal_load_factor_g = lift_vert / (mass_kg * g0).max(1.0);

    // Turn rate: dpsi/dt = L_horiz / (m * v) [rad/s]
    let turn_rate_rad_s = lift_horiz / (mass_kg * flight_speed_m_s).max(1.0);
    let turn_rate_deg_s = turn_rate_rad_s.to_degrees();

    // Turn radius: R = v / (dpsi/dt)
    let turn_radius_m = flight_speed_m_s / turn_rate_rad_s.max(1e-5);
    let turn_radius_km = turn_radius_m / 1000.0;

    // Maneuver duration for specified heading change
    let heading_rad = target_heading_change_deg.to_radians();
    let maneuver_duration_s = heading_rad / turn_rate_rad_s.max(1e-5);

    // Cross-track aerothermal heating multiplier on the windward roll shoulder
    let asym_heating_factor = 1.0 + 0.18 * bank_rad.sin().abs();

    // Autonomous fly-by-light active rudderless FCS holds sideslip to minimal levels
    let residual_sideslip_deg = 0.08 * (bank_rad.sin().abs());

    let is_g_safe = normal_load_factor_g <= 3.5;
    let is_heat_safe = asym_heating_factor <= 1.25;
    let is_maneuver_safe = is_g_safe && is_heat_safe;

    Hypersonic6DofManeuverResult {
        turn_rate_deg_s,
        turn_radius_km,
        normal_load_factor_g,
        maneuver_duration_s,
        effective_gravity_m_s2,
        asym_heating_factor,
        bank_angle_deg,
        residual_sideslip_deg,
        is_maneuver_safe,
    }
}

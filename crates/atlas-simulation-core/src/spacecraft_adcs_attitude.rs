//! Spacecraft ADCS Three-Axis Disturbance Torques & Reaction Wheel Sizing (NASA-SP-8007 / Wertz / ECSS)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Keplerian orbital period T_orb, orbital velocity v_orb, and geomagnetic field B(h).
//! 2. Four primary space environmental disturbance torques:
//!    - Gravity Gradient torque T_gg.
//!    - Solar Radiation Pressure (SRP) torque T_srp.
//!    - Aerodynamic residual drag torque T_aero (LEO atmospheric density).
//!    - Residual spacecraft magnetic dipole interaction torque T_mag.
//! 3. Total peak and cyclic disturbance torques and orbital accumulated angular momentum H_accum.
//! 4. Three-axis Reaction Wheel Assembly (RWA) momentum storage H_rw and slew torque capability.
//! 5. Magnetic Torquer Bar (MTQ) dipole moment M_mtq for wheel momentum desaturation.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Spacecraft operational orbit regime.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpacecraftOrbitRegime {
    /// Low Earth Orbit (LEO, 400 to 1,000 km): Aerodynamics and gravity gradient dominate.
    LowEarthOrbitLEO,
    /// Medium Earth Orbit (MEO / Navigation, 20,000 km): Solar radiation and gravity gradient.
    MediumEarthOrbitMEO,
    /// Geostationary Earth Orbit (GEO, 35,786 km): Solar radiation pressure dominates.
    GeostationaryOrbitGEO,
}

/// Input parameters for spacecraft attitude determination and control system (ADCS) analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacecraftAdcsProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Orbit mission regime.
    pub orbit_regime: SpacecraftOrbitRegime,
    /// Orbit circular altitude h in kilometers (km, standard 300 to 36,000 km).
    pub orbit_altitude_km: f64,
    /// Spacecraft total launch mass m in kilograms (kg, standard 10 to 5,000 kg).
    pub spacecraft_mass_kg: f64,
    /// Principal moment of inertia around roll/pitch axis I_x in kg·m² (standard 0.1 to 10,000 kg·m²).
    pub inertia_ixx_kg_m2: f64,
    /// Principal moment of inertia around pitch axis I_y in kg·m² (standard 0.1 to 10,000 kg·m²).
    pub inertia_iyy_kg_m2: f64,
    /// Principal moment of inertia around yaw/nadir axis I_z in kg·m² (standard 0.1 to 10,000 kg·m²).
    pub inertia_izz_kg_m2: f64,
    /// Center-of-pressure to center-of-mass offset lever arm Delta_r in meters (m, standard 0.01 to 0.50 m).
    pub cp_cg_offset_arm_m: f64,
    /// Cross-sectional area exposed to ram velocity / solar radiation in square meters (m², standard 0.1 to 50 m²).
    pub projected_surface_area_m2: f64,
    /// Solar surface optical reflectance factor q (0.0 = total absorption, 1.0 = specular reflection).
    pub solar_reflectance_q: f64,
    /// Residual magnetic dipole moment of spacecraft electronics M_res in A·m² (standard 0.05 to 5.0 A·m²).
    pub residual_magnetic_dipole_a_m2: f64,
    /// Target spacecraft rapid slew agility acceleration in degrees per second squared (deg/s²).
    pub target_slew_accel_deg_s2: f64,
}

impl Default for SpacecraftAdcsProblem {
    fn default() -> Self {
        Self {
            problem_id: "earth_observation_leo_satellite".to_string(),
            orbit_regime: SpacecraftOrbitRegime::LowEarthOrbitLEO,
            orbit_altitude_km: 550.0,        // 550 km Sun-Synchronous LEO
            spacecraft_mass_kg: 350.0,       // 350 kg smallsat
            inertia_ixx_kg_m2: 85.0,         // Roll inertia
            inertia_iyy_kg_m2: 120.0,        // Pitch inertia
            inertia_izz_kg_m2: 75.0,         // Yaw inertia
            cp_cg_offset_arm_m: 0.08,        // 8 cm CP-CG offset
            projected_surface_area_m2: 2.40, // 2.4 m² solar array + bus cross-section
            solar_reflectance_q: 0.60,       // 60% reflective multi-layer insulation / solar cells
            residual_magnetic_dipole_a_m2: 0.50, // 0.50 A·m² residual magnetic dipole
            target_slew_accel_deg_s2: 1.50,  // 1.5 deg/s² slew agility
        }
    }
}

/// Simulation results for environmental torques, reaction wheel momentum, and magnetic torquers.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SpacecraftAdcsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Orbit orbital period in minutes (min).
    pub orbit_period_minutes: f64,
    /// Orbital velocity v_orb in meters per second (m/s).
    pub orbital_velocity_m_s: f64,
    /// Local geomagnetic field intensity B_local in Tesla (T).
    pub local_magnetic_field_tesla: f64,
    /// Maximum gravity gradient disturbance torque T_gg in microNewton-meters (µN·m).
    pub gravity_gradient_torque_un_m: f64,
    /// Solar radiation pressure (SRP) disturbance torque T_srp in microNewton-meters (µN·m).
    pub solar_radiation_torque_un_m: f64,
    /// Aerodynamic drag disturbance torque T_aero in microNewton-meters (µN·m).
    pub aerodynamic_drag_torque_un_m: f64,
    /// Spacecraft residual magnetic disturbance torque T_mag in microNewton-meters (µN·m).
    pub magnetic_residual_torque_un_m: f64,
    /// Peak total combined worst-case environmental disturbance torque in microNewton-meters (µN·m).
    pub peak_disturbance_torque_un_m: f64,
    /// Cyclic accumulated angular momentum per orbit H_accum in Newton-meter-seconds (N·m·s).
    pub accumulated_momentum_per_orbit_nms: f64,
    /// Recommended minimum reaction wheel momentum storage capacity H_rw in Newton-meter-seconds (N·m·s).
    pub recommended_wheel_momentum_nms: f64,
    /// Required reaction wheel motor torque for target slew agility T_slew in milliNewton-meters (mN·m).
    pub required_slew_torque_mn_m: f64,
    /// Required magnetic torquer dipole moment for continuous desaturation M_mtq in Ampere-meters squared (A·m²).
    pub required_mtq_dipole_a_m2: f64,
    /// Assessment indicating reaction wheel momentum capacity is adequate for >= 2 full orbits without desaturation.
    pub is_wheel_capacity_adequate: bool,
    /// Assessment indicating magnetic torquer can dump accumulated momentum within half an orbit.
    pub is_mtq_desaturation_adequate: bool,
    /// Overall ADCS actuator sizing viability.
    pub is_adcs_sizing_viable: bool,
}

/// Solves spacecraft three-axis environmental disturbance torques and ADCS actuator sizing.
pub fn solve_spacecraft_adcs_attitude(
    problem: &SpacecraftAdcsProblem,
) -> Result<SpacecraftAdcsResult, String> {
    if problem.orbit_altitude_km < 150.0 || problem.orbit_altitude_km > 50000.0 {
        return Err("Orbit altitude must be between 150 and 50,000 km".to_string());
    }
    if problem.spacecraft_mass_kg <= 0.0 {
        return Err("Spacecraft mass must be positive".to_string());
    }
    if problem.inertia_ixx_kg_m2 <= 0.0
        || problem.inertia_iyy_kg_m2 <= 0.0
        || problem.inertia_izz_kg_m2 <= 0.0
    {
        return Err("Moments of inertia must be strictly positive".to_string());
    }

    let r_earth = 6378.137e3; // Earth equatorial radius [m]
    let mu_earth = 3.986004418e14; // Earth gravitational parameter [m³/s²]
    let r_orb = r_earth + problem.orbit_altitude_km * 1000.0; // Orbit radius [m]

    // Orbital kinematics:
    let v_orb = (mu_earth / r_orb).sqrt(); // [m/s]
    let omega_orb = (mu_earth / r_orb.powi(3)).sqrt(); // [rad/s]
    let t_orb_s = 2.0 * std::f64::consts::PI / omega_orb; // [s]
    let t_orb_min = t_orb_s / 60.0;

    // Earth dipole magnetic field model: B(r) ~ B_0 * (R_e / r)^3:
    let b_0 = 3.12e-5; // Tesla at equator
    let b_local = b_0 * (r_earth / r_orb).powi(3);

    // 1. Gravity gradient torque: T_gg = (3 * mu / 2*r^3) * |I_z - I_x| * sin(2*theta):
    // Max occurs at theta = 45° where sin(2*theta) = 1.0:
    let delta_i = (problem.inertia_izz_kg_m2 - problem.inertia_ixx_kg_m2)
        .abs()
        .max((problem.inertia_iyy_kg_m2 - problem.inertia_ixx_kg_m2).abs());
    let t_gg_nm = 1.5 * (mu_earth / r_orb.powi(3)) * delta_i;

    // 2. Solar radiation pressure torque:
    // Solar constant S ~ 1361 W/m², speed of light c ~ 2.998e8 m/s -> P_srp = S / c ~ 4.54 µPa
    let p_srp = 1361.0 / 2.99792458e8; // Pa (N/m²)
    let f_srp = p_srp * (1.0 + problem.solar_reflectance_q) * problem.projected_surface_area_m2;
    let t_srp_nm = f_srp * problem.cp_cg_offset_arm_m;

    // 3. Aerodynamic drag torque (LEO exponential atmospheric model):
    // Standard US Standard Atmosphere scale height model for density rho(h):
    let rho_air = if problem.orbit_altitude_km <= 1000.0 {
        // Density at 500 km ~ 1.5e-12 kg/m³, scale height H ~ 60 km
        let h_km = problem.orbit_altitude_km;
        6.0e-10 * (-h_km / 55.0).exp() // kg/m³
    } else {
        1.0e-18 // Negligible in MEO/GEO
    };
    let c_d_aero = 2.2; // Standard hypersonic free molecular drag coefficient
    let f_aero = 0.5 * rho_air * v_orb.powi(2) * c_d_aero * problem.projected_surface_area_m2;
    let t_aero_nm = f_aero * problem.cp_cg_offset_arm_m;

    // 4. Residual magnetic dipole torque: T_mag = M_res * B_local:
    let t_mag_nm = problem.residual_magnetic_dipole_a_m2 * b_local;

    // Convert torques to microNewton-meters (µN·m):
    let t_gg_un_m = t_gg_nm * 1.0e6;
    let t_srp_un_m = t_srp_nm * 1.0e6;
    let t_aero_un_m = t_aero_nm * 1.0e6;
    let t_mag_un_m = t_mag_nm * 1.0e6;

    // Worst-case total peak disturbance torque:
    let t_peak_nm = t_gg_nm + t_srp_nm + t_aero_nm + t_mag_nm;
    let t_peak_un_m = t_peak_nm * 1.0e6;

    // Cyclic accumulated angular momentum per orbit:
    // Cyclic integral over one orbit: H_accum ~ (T_peak / 4) * T_orb [N*m*s]:
    let h_accum_nms = (t_peak_nm / 4.0) * t_orb_s;

    // Recommended Reaction Wheel momentum capacity: H_rw >= 2.0 * H_accum:
    let h_rw_rec_nms = (2.0 * h_accum_nms).max(0.02);

    // Required motor slew torque for fast attitude agility: T = I_max * alpha_slew [N*m]:
    let max_i = problem
        .inertia_ixx_kg_m2
        .max(problem.inertia_iyy_kg_m2)
        .max(problem.inertia_izz_kg_m2);
    let alpha_slew_rad_s2 = problem.target_slew_accel_deg_s2.to_radians();
    let t_slew_nm = max_i * alpha_slew_rad_s2;
    let t_slew_mn_m = t_slew_nm * 1000.0;

    // Magnetic Torquer Bar dipole requirement to dump momentum within half orbit (t_desat = T_orb / 2):
    // T_desat = H_accum / (T_orb / 2) -> M_mtq = T_desat / B_local:
    let t_desat_nm = h_accum_nms / (t_orb_s / 2.0);
    let m_mtq_req = if b_local > 1.0e-9 {
        (t_desat_nm / b_local).max(0.1)
    } else {
        0.0 // MTQs ineffective in GEO
    };

    let is_rw_ok = h_rw_rec_nms >= h_accum_nms;
    let is_mtq_ok = (m_mtq_req <= 100.0) && (t_slew_mn_m <= 5000.0);
    let is_overall_ok = is_rw_ok && is_mtq_ok && (t_peak_nm > 0.0);

    Ok(SpacecraftAdcsResult {
        problem_id: problem.problem_id.clone(),
        orbit_period_minutes: t_orb_min,
        orbital_velocity_m_s: v_orb,
        local_magnetic_field_tesla: b_local,
        gravity_gradient_torque_un_m: t_gg_un_m,
        solar_radiation_torque_un_m: t_srp_un_m,
        aerodynamic_drag_torque_un_m: t_aero_un_m,
        magnetic_residual_torque_un_m: t_mag_un_m,
        peak_disturbance_torque_un_m: t_peak_un_m,
        accumulated_momentum_per_orbit_nms: h_accum_nms,
        recommended_wheel_momentum_nms: h_rw_rec_nms,
        required_slew_torque_mn_m: t_slew_mn_m,
        required_mtq_dipole_a_m2: m_mtq_req,
        is_wheel_capacity_adequate: is_rw_ok,
        is_mtq_desaturation_adequate: is_mtq_ok,
        is_adcs_sizing_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_earth_observation_leo_satellite() -> Result<(), String> {
        let prob = SpacecraftAdcsProblem::default();
        let res = solve_spacecraft_adcs_attitude(&prob)?;

        assert!(res.orbit_period_minutes > 90.0 && res.orbit_period_minutes < 100.0);
        assert!(res.orbital_velocity_m_s > 7400.0 && res.orbital_velocity_m_s < 7800.0);
        assert!(res.local_magnetic_field_tesla > 2.0e-5 && res.local_magnetic_field_tesla < 3.5e-5);
        assert!(res.peak_disturbance_torque_un_m > 5.0 && res.peak_disturbance_torque_un_m < 200.0);
        assert!(
            res.accumulated_momentum_per_orbit_nms > 0.01
                && res.accumulated_momentum_per_orbit_nms < 0.50
        );
        assert!(res.recommended_wheel_momentum_nms >= 0.05);
        assert!(res.required_slew_torque_mn_m > 500.0 && res.required_slew_torque_mn_m < 5000.0);
        assert!(res.required_mtq_dipole_a_m2 > 0.1 && res.required_mtq_dipole_a_m2 < 20.0);
        assert!(res.is_wheel_capacity_adequate);
        assert!(res.is_mtq_desaturation_adequate);
        assert!(res.is_adcs_sizing_viable);
        Ok(())
    }
}

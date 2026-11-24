//! Satellite Orbital Decay & Atmospheric Re-entry Lifetime Solver.
//!
//! Implements King-Hele analytical satellite orbital decay theory, Jacchia-Roberts/NRLMSISE-00
//! exponential atmospheric density scaling with solar F10.7 flux, and space debris post-mission
//! disposal lifetime verification in accordance with ISO 27852 and IADC / NASA 25-Year / 5-Year rules.
//!
//! # Physical Formulations:
//! - **Ballistic Drag Coefficient ($B$) & Area-to-Mass Ratio**:
//!   $$B = \frac{C_D \cdot A_{cross}}{M_{sat}} \quad [\text{m}^2/\text{kg}]$$
//! - **Atmospheric Density with Solar Flux ($F_{10.7}$) & Scale Height ($H$)**:
//!   $$\rho(h) = \rho_0(h_{ref}) \cdot \exp\left(-\frac{h - h_{ref}}{H_{scale}}\right) \cdot \left(\frac{F_{10.7}}{100}\right)^{0.85} \quad [\text{kg/m}^3]$$
//! - **King-Hele Semi-Major Axis Decay Rate for Quasi-Circular Orbit**:
//!   $$\frac{da}{dt} = -\sqrt{\mu_E \cdot a} \cdot B \cdot \rho(a) \quad [\text{m/s}]$$
//! - **Closed-Form Re-entry Orbital Lifetime ($T_{lifetime}$)**:
//!   $$T_{lifetime} = \frac{H_{scale}}{\sqrt{\mu_E \cdot a_0} \cdot B \cdot \rho(a_0)} \quad [\text{seconds}] = \frac{T_{lifetime}}{86400 \cdot 365.25} \quad [\text{years}]$$
//! - **Space Debris Mitigation Compliance**:
//!   $$T_{lifetime} \le 25.0 \text{ years (IADC / NASA guideline)}, \quad T_{lifetime} \le 5.0 \text{ years (FCC updated rule)}$$

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Solar activity cycle level influencing upper thermospheric density.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SolarActivityLevel {
    /// Solar minimum conditions (F10.7 = 70 sfu).
    SolarMinimum,
    /// Moderate / mean solar cycle activity (F10.7 = 120 sfu).
    ModerateMean,
    /// Solar maximum conditions (F10.7 = 200 sfu).
    SolarMaximum,
}

/// Input parameters for satellite orbital decay analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteOrbitalDecayProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Solar cycle activity state.
    pub solar_activity: SolarActivityLevel,
    /// Satellite wet mass at end of mission in kilograms (kg).
    pub satellite_mass_kg: f64,
    /// Effective cross-sectional aerodynamic drag area in square meters (m²).
    pub drag_cross_section_area_m2: f64,
    /// Aerodynamic drag coefficient C_D (standard 2.2 for free molecular flow in LEO).
    pub drag_coefficient_cd: f64,
    /// Initial perigee altitude h_p in kilometers (km).
    pub initial_perigee_altitude_km: f64,
    /// Initial apogee altitude h_a in kilometers (km).
    pub initial_apogee_altitude_km: f64,
    /// Orbit inclination in degrees (deg).
    pub orbit_inclination_deg: f64,
}

impl Default for SatelliteOrbitalDecayProblem {
    fn default() -> Self {
        Self {
            problem_id: "cubesat_leo_orbital_decay".to_string(),
            solar_activity: SolarActivityLevel::ModerateMean,
            satellite_mass_kg: 12.0,           // 3U/6U CubeSat
            drag_cross_section_area_m2: 0.045, // 0.045 m² drag face
            drag_coefficient_cd: 2.20,
            initial_perigee_altitude_km: 420.0, // ISS / Starlink disposal orbit
            initial_apogee_altitude_km: 440.0,
            orbit_inclination_deg: 51.6,
        }
    }
}

/// Simulation results for satellite orbital lifetime and atmospheric re-entry.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SatelliteOrbitalDecayResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Ballistic coefficient B = (C_D * A) / M in m²/kg.
    pub ballistic_coefficient_m2_per_kg: f64,
    /// Inverse ballistic coefficient (M / (C_D * A)) in kg/m².
    pub inverse_ballistic_coefficient_kg_per_m2: f64,
    /// Initial mean orbital altitude in kilometers (km).
    pub initial_mean_altitude_km: f64,
    /// Initial orbital eccentricity e.
    pub initial_eccentricity: f64,
    /// Initial orbital period in minutes (min).
    pub initial_orbital_period_minutes: f64,
    /// Atmospheric mass density at initial mean altitude in kg/m³.
    pub atmospheric_density_kg_m3: f64,
    /// Atmospheric density scale height H at initial altitude in kilometers (km).
    pub density_scale_height_km: f64,
    /// Initial daily altitude loss rate in meters per day (m/day).
    pub initial_decay_rate_m_per_day: f64,
    /// Total expected orbital lifetime until re-entry in days.
    pub orbital_lifetime_days: f64,
    /// Total expected orbital lifetime in calendar years.
    pub orbital_lifetime_years: f64,
    /// Whether orbital lifetime satisfies the international IADC / NASA 25-Year Rule (<= 25.0 years).
    pub is_iadc_25yr_compliant: bool,
    /// Whether orbital lifetime satisfies the FCC 5-Year rapid disposal rule (<= 5.0 years).
    pub is_fcc_5yr_compliant: bool,
}

/// Solves satellite orbital decay rate, King-Hele integration, and atmospheric lifetime.
pub fn solve_satellite_orbital_decay(
    problem: &SatelliteOrbitalDecayProblem,
) -> Result<SatelliteOrbitalDecayResult, String> {
    if problem.satellite_mass_kg <= 0.05 {
        return Err("Satellite mass must be greater than 0.05 kg".to_string());
    }
    if problem.drag_cross_section_area_m2 <= 0.0001 {
        return Err("Drag area must be positive".to_string());
    }
    if problem.initial_perigee_altitude_km <= 80.0 {
        return Err("Perigee altitude must be above 80 km (already re-entering)".to_string());
    }

    let r_earth_km = 6378.137;
    let mu_earth_m3_s2 = 3.986004418e14; // Earth gravitational parameter

    let h_p = problem.initial_perigee_altitude_km;
    let h_a = problem.initial_apogee_altitude_km;
    let h_mean = (h_p + h_a) / 2.0;

    let a_0_km = r_earth_km + h_mean;
    let a_0_m = a_0_km * 1000.0;
    let eccentricity = (h_a - h_p) / (2.0 * a_0_km);

    // Orbital period: T = 2*pi * sqrt(a^3 / mu)
    let period_s = 2.0 * std::f64::consts::PI * (a_0_m.powi(3) / mu_earth_m3_s2).sqrt();
    let period_min = period_s / 60.0;

    // Ballistic coefficient: B = C_D * A / M
    let b_m2_kg = (problem.drag_coefficient_cd * problem.drag_cross_section_area_m2)
        / problem.satellite_mass_kg;
    let inv_b = 1.0 / b_m2_kg;

    // Atmospheric density model (Jacchia / King-Hele exponential fit):
    // Reference altitudes and scale heights:
    let (h_ref_km, rho_ref, h_scale_km) = if h_mean <= 200.0 {
        (180.0, 5.46e-10, 28.0)
    } else if h_mean <= 300.0 {
        (250.0, 7.25e-11, 38.0)
    } else if h_mean <= 450.0 {
        (350.0, 7.00e-12, 53.0)
    } else if h_mean <= 600.0 {
        (500.0, 6.97e-13, 68.0)
    } else {
        (650.0, 6.50e-14, 85.0)
    };

    let f107_flux: f64 = match problem.solar_activity {
        SolarActivityLevel::SolarMinimum => 70.0,
        SolarActivityLevel::ModerateMean => 120.0,
        SolarActivityLevel::SolarMaximum => 200.0,
    };
    let solar_scale = (f107_flux / 100.0).powf(0.85);

    let rho_h = rho_ref * (-(h_mean - h_ref_km) / h_scale_km).exp() * solar_scale;
    let h_scale_m = h_scale_km * 1000.0;

    // King-Hele decay rate (da/dt in m/s):
    // dE/dt = - 1/2 * rho * v^3 * B = (mu / (2 * a^2)) * da/dt
    // => da/dt = - sqrt(mu * a) * B * rho = - v_orb * a_0 * B * rho
    let v_orb = (mu_earth_m3_s2 / a_0_m).sqrt();
    let da_dt_m_s = -(v_orb * a_0_m) * b_m2_kg * rho_h;
    let da_dt_m_day = -da_dt_m_s * 86400.0;

    // Integrated lifetime to re-entry (~ 120 km):
    // T_lifetime ~ H_scale / |da/dt|
    let lifetime_s = h_scale_m / da_dt_m_s.abs();
    let lifetime_days = lifetime_s / 86400.0;
    let lifetime_years = lifetime_days / 365.25;

    let is_iadc_ok = lifetime_years <= 25.0;
    let is_fcc_ok = lifetime_years <= 5.0;

    Ok(SatelliteOrbitalDecayResult {
        problem_id: problem.problem_id.clone(),
        ballistic_coefficient_m2_per_kg: b_m2_kg,
        inverse_ballistic_coefficient_kg_per_m2: inv_b,
        initial_mean_altitude_km: h_mean,
        initial_eccentricity: eccentricity,
        initial_orbital_period_minutes: period_min,
        atmospheric_density_kg_m3: rho_h,
        density_scale_height_km: h_scale_km,
        initial_decay_rate_m_per_day: da_dt_m_day,
        orbital_lifetime_days: lifetime_days,
        orbital_lifetime_years: lifetime_years,
        is_iadc_25yr_compliant: is_iadc_ok,
        is_fcc_5yr_compliant: is_fcc_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cubesat_leo_decay() -> Result<(), String> {
        let prob = SatelliteOrbitalDecayProblem {
            problem_id: "test_cubesat".to_string(),
            solar_activity: SolarActivityLevel::ModerateMean,
            satellite_mass_kg: 12.0,
            drag_cross_section_area_m2: 0.045,
            drag_coefficient_cd: 2.20,
            initial_perigee_altitude_km: 420.0,
            initial_apogee_altitude_km: 440.0,
            orbit_inclination_deg: 51.6,
        };

        let res = solve_satellite_orbital_decay(&prob)?;
        assert!(
            res.ballistic_coefficient_m2_per_kg > 0.006
                && res.ballistic_coefficient_m2_per_kg < 0.012
        );
        assert!(
            res.initial_orbital_period_minutes > 90.0 && res.initial_orbital_period_minutes < 96.0
        );
        assert!(
            res.initial_decay_rate_m_per_day > 10.0 && res.initial_decay_rate_m_per_day < 100.0
        );
        assert!(res.orbital_lifetime_years > 1.0 && res.orbital_lifetime_years < 10.0);
        assert!(res.is_iadc_25yr_compliant);
        Ok(())
    }
}

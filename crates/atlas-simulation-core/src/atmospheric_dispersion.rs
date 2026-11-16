//! Atmospheric Pollutant Gaussian Plume Dispersion (EPA AERMOD / Pasquill-Gifford / Briggs / 40 CFR 50)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Briggs buoyant and momentum plume rise formulas for effective emission height.
//! 2. Pasquill-Gifford atmospheric stability classification (Classes A through F).
//! 3. Crosswind and vertical atmospheric dispersion coefficients sigma_y(x) and sigma_z(x).
//! 4. Ground-level 2D/3D concentration mapping with ground reflection.
//! 5. Maximum ground-level impact distance and EPA NAAQS clean air compliance verification.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Pasquill-Gifford atmospheric stability classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AtmosphericStabilityClass {
    /// Class A: Extremely unstable (bright sunny summer day, light winds).
    ClassAExtremelyUnstable,
    /// Class B: Moderately unstable (moderate solar insolation).
    ClassBModeratelyUnstable,
    /// Class C: Slightly unstable (cloudy day or strong winds).
    ClassCSlightlyUnstable,
    /// Class D: Neutral (overcast day or night, windy).
    ClassDNeutral,
    /// Class E: Slightly stable (nighttime, moderate cloud cover).
    ClassESlightlyStable,
    /// Class F: Moderately stable (clear calm night with temperature inversion).
    ClassFModeratelyStable,
}

/// Target atmospheric pollutant category for EPA NAAQS comparison.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AirPollutantType {
    /// Sulfur Dioxide (SO2) - 1-hour primary NAAQS 196 µg/m³ (75 ppb).
    SulfurDioxideSO2,
    /// Nitrogen Dioxide (NO2) - 1-hour primary NAAQS 188 µg/m³ (100 ppb).
    NitrogenDioxideNO2,
    /// Carbon Monoxide (CO) - 1-hour primary NAAQS 40,000 µg/m³ (35 ppm).
    CarbonMonoxideCO,
    /// Fine Particulate Matter (PM2.5) - 24-hour primary NAAQS 35 µg/m³.
    ParticulatePM25,
}

/// Input parameters for industrial stack emission and Gaussian plume dispersion.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphericDispersionProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Atmospheric stability class.
    pub stability_class: AtmosphericStabilityClass,
    /// Emitted pollutant species.
    pub pollutant: AirPollutantType,
    /// Pollutant mass emission rate in grams per second (g/s, standard 10 to 500 g/s).
    pub emission_rate_g_s: f64,
    /// Physical chimney / stack height above ground level in meters (m, standard 30 to 180 m).
    pub stack_height_m: f64,
    /// Stack inside exit diameter in meters (m, standard 1.5 to 5.0 m).
    pub stack_inside_diameter_m: f64,
    /// Flue gas exit velocity in meters per second (m/s, standard 12 to 28 m/s).
    pub stack_gas_velocity_m_s: f64,
    /// Flue gas exit temperature in degrees Celsius (°C, standard 110 to 220 °C).
    pub stack_gas_temperature_c: f64,
    /// Ambient air temperature at stack height in degrees Celsius (°C, nominal 15 to 25 °C).
    pub ambient_air_temperature_c: f64,
    /// Mean wind speed at stack release height in meters per second (m/s, standard 2.0 to 15.0 m/s).
    pub wind_speed_at_stack_height_m_s: f64,
    /// Receptor downwind evaluation distance in meters (m).
    pub receptor_downwind_distance_m: f64,
    /// Receptor crosswind offset distance in meters (m, 0.0 for plume centerline).
    pub receptor_crosswind_distance_m: f64,
}

impl Default for AtmosphericDispersionProblem {
    fn default() -> Self {
        Self {
            problem_id: "industrial_power_plant_stack_dispersion".to_string(),
            stability_class: AtmosphericStabilityClass::ClassDNeutral,
            pollutant: AirPollutantType::SulfurDioxideSO2,
            emission_rate_g_s: 85.0,
            stack_height_m: 80.0,
            stack_inside_diameter_m: 2.80,
            stack_gas_velocity_m_s: 18.5,
            stack_gas_temperature_c: 140.0,
            ambient_air_temperature_c: 20.0,
            wind_speed_at_stack_height_m_s: 5.50,
            receptor_downwind_distance_m: 2500.0,
            receptor_crosswind_distance_m: 0.0, // Centerline
        }
    }
}

/// Simulation results for plume rise, Gaussian dispersion coefficients, and ground concentration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AtmosphericDispersionResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Briggs thermal buoyancy flux parameter F_b in m⁴/s³.
    pub buoyancy_flux_m4_per_s3: f64,
    /// Briggs momentum flux parameter F_m in m⁴/s².
    pub momentum_flux_m4_per_s2: f64,
    /// Final Briggs buoyant plume rise delta_h in meters (m).
    pub plume_rise_delta_h_m: f64,
    /// Effective emission height H_eff = h_stack + delta_h in meters (m).
    pub effective_stack_height_m: f64,
    /// Horizontal crosswind dispersion coefficient sigma_y at receptor in meters (m).
    pub sigma_y_m: f64,
    /// Vertical dispersion coefficient sigma_z at receptor in meters (m).
    pub sigma_z_m: f64,
    /// Predicted ground-level pollutant concentration at receptor in micrograms per cubic meter (µg/m³).
    pub ground_concentration_ug_per_m3: f64,
    /// Distance from stack to maximum ground-level concentration impact x_max in meters (m).
    pub distance_to_max_concentration_m: f64,
    /// Peak maximum ground-level centerline concentration C_max in micrograms per cubic meter (µg/m³).
    pub peak_max_ground_concentration_ug_per_m3: f64,
    /// Relevant US EPA NAAQS ambient limit threshold in micrograms per cubic meter (µg/m³).
    pub epa_naaqs_threshold_ug_per_m3: f64,
    /// Concentration ratio to NAAQS threshold (C_max / NAAQS). Must be <= 1.0.
    pub naaqs_compliance_ratio: f64,
    /// Whether peak ground-level concentration complies with US EPA NAAQS clean air limits.
    pub is_naaqs_compliant: bool,
    /// Whether effective plume height avoids building downwash and ground fumigation (H_eff >= 1.5 * h_s).
    pub is_plume_rise_adequate: bool,
}

/// Solves Briggs plume rise, Pasquill-Gifford dispersion, and EPA NAAQS ambient concentrations.
pub fn solve_atmospheric_dispersion(
    problem: &AtmosphericDispersionProblem,
) -> Result<AtmosphericDispersionResult, String> {
    if problem.emission_rate_g_s <= 0.0 {
        return Err("Emission rate must be positive".to_string());
    }
    if problem.stack_height_m <= 5.0 {
        return Err("Stack height must be at least 5 m".to_string());
    }
    if problem.stack_inside_diameter_m <= 0.20 {
        return Err("Stack diameter must be at least 0.20 m".to_string());
    }
    if problem.wind_speed_at_stack_height_m_s <= 0.50 {
        return Err("Wind speed must be at least 0.50 m/s".to_string());
    }
    if problem.receptor_downwind_distance_m <= 50.0 {
        return Err("Receptor distance must be at least 50 m".to_string());
    }

    let g_accel = 9.80665;
    let t_s_k = problem.stack_gas_temperature_c + 273.15;
    let t_a_k = problem.ambient_air_temperature_c + 273.15;
    let r_s = problem.stack_inside_diameter_m / 2.0;
    let v_s = problem.stack_gas_velocity_m_s;
    let u_s = problem.wind_speed_at_stack_height_m_s;

    // 1. Briggs Flux Parameters:
    // Buoyancy flux: F_b = g * v_s * r_s^2 * (T_s - T_a) / T_s (m⁴/s³)
    let delta_t = (problem.stack_gas_temperature_c - problem.ambient_air_temperature_c).max(0.0);
    let f_b = g_accel * v_s * r_s.powi(2) * (delta_t / t_s_k);

    // Momentum flux: F_m = v_s^2 * r_s^2 * (T_a / T_s) (m⁴/s²)
    let f_m = v_s.powi(2) * r_s.powi(2) * (t_a_k / t_s_k);

    // 2. Briggs Plume Rise (delta_h):
    let delta_h = match problem.stability_class {
        AtmosphericStabilityClass::ClassAExtremelyUnstable
        | AtmosphericStabilityClass::ClassBModeratelyUnstable
        | AtmosphericStabilityClass::ClassCSlightlyUnstable
        | AtmosphericStabilityClass::ClassDNeutral => {
            // Neutral / Unstable atmosphere:
            if f_b >= 55.0 {
                38.7 * (f_b.powf(0.60) / u_s)
            } else if f_b > 0.1 {
                21.4 * (f_b.powf(0.75) / u_s)
            } else {
                // Momentum dominated plume:
                3.0 * problem.stack_inside_diameter_m * (v_s / u_s)
            }
        }
        AtmosphericStabilityClass::ClassESlightlyStable
        | AtmosphericStabilityClass::ClassFModeratelyStable => {
            // Stable atmosphere:
            // Stability parameter s = (g / T_a) * (d_theta / dz)
            let d_theta_dz = match problem.stability_class {
                AtmosphericStabilityClass::ClassESlightlyStable => 0.020, // K/m
                _ => 0.035, // K/m
            };
            let s_stab = (g_accel / t_a_k) * d_theta_dz;
            let delta_h_buoyant = 2.6 * (f_b / (u_s * s_stab)).powf(0.3333);
            let delta_h_momentum = 1.5 * (f_m / (u_s * s_stab.sqrt())).powf(0.3333);
            delta_h_buoyant.max(delta_h_momentum)
        }
    };

    let h_eff = problem.stack_height_m + delta_h;

    // 3. Briggs Dispersion Coefficients sigma_y(x) and sigma_z(x) for Open Country (x in meters):
    let x_rec = problem.receptor_downwind_distance_m;
    let (sigma_y_m, sigma_z_m) = compute_briggs_sigmas(problem.stability_class, x_rec);

    // 4. Ground-Level Gaussian Plume Concentration at Receptor (z = 0):
    // C(x, y, 0) = [Q / (pi * u * sigma_y * sigma_z)] * exp(-y^2 / (2 * sigma_y^2)) * exp(-H^2 / (2 * sigma_z^2))
    // Q is in g/s; convert to µg/s (1 g = 1e6 µg) -> C in µg/m³
    let q_ug_s = problem.emission_rate_g_s * 1e6;
    let y_rec = problem.receptor_crosswind_distance_m;

    let c_rec_ug_m3 = (q_ug_s / (std::f64::consts::PI * u_s * sigma_y_m * sigma_z_m))
        * (-(y_rec.powi(2)) / (2.0 * sigma_y_m.powi(2))).exp()
        * (-(h_eff.powi(2)) / (2.0 * sigma_z_m.powi(2))).exp();

    // 5. Maximum Ground-Level Impact (Centerline Peak):
    // At x_max, sigma_z(x_max) ~ H_eff / sqrt(2)
    // Closed-form approximate inversion of x_max:
    let sigma_z_target = h_eff / std::f64::consts::SQRT_2;
    let mut x_low = 100.0;
    let mut x_high = 50000.0;
    for _ in 0..25 {
        let x_mid = 0.5 * (x_low + x_high);
        let (_, sz_mid) = compute_briggs_sigmas(problem.stability_class, x_mid);
        if sz_mid < sigma_z_target {
            x_low = x_mid;
        } else {
            x_high = x_mid;
        }
    }
    let x_max = 0.5 * (x_low + x_high);
    let (sy_max, _) = compute_briggs_sigmas(problem.stability_class, x_max);

    // Peak maximum concentration:
    // C_max = Q / (pi * e * u * sigma_y(x_max) * H_eff)
    let c_max_ug_m3 = q_ug_s / (std::f64::consts::PI * std::f64::consts::E * u_s * sy_max * h_eff);

    // 6. EPA NAAQS Clean Air Thresholds:
    let naaqs_limit = match problem.pollutant {
        AirPollutantType::SulfurDioxideSO2 => 196.0,   // 1-hour primary
        AirPollutantType::NitrogenDioxideNO2 => 188.0, // 1-hour primary
        AirPollutantType::CarbonMonoxideCO => 40000.0, // 1-hour primary
        AirPollutantType::ParticulatePM25 => 35.0,     // 24-hour primary
    };

    let naaqs_ratio = c_max_ug_m3 / naaqs_limit;
    let is_naaqs_ok = c_max_ug_m3 <= naaqs_limit;
    let is_plume_ok = h_eff >= (1.50 * problem.stack_height_m);

    Ok(AtmosphericDispersionResult {
        problem_id: problem.problem_id.clone(),
        buoyancy_flux_m4_per_s3: f_b,
        momentum_flux_m4_per_s2: f_m,
        plume_rise_delta_h_m: delta_h,
        effective_stack_height_m: h_eff,
        sigma_y_m,
        sigma_z_m,
        ground_concentration_ug_per_m3: c_rec_ug_m3,
        distance_to_max_concentration_m: x_max,
        peak_max_ground_concentration_ug_per_m3: c_max_ug_m3,
        epa_naaqs_threshold_ug_per_m3: naaqs_limit,
        naaqs_compliance_ratio: naaqs_ratio,
        is_naaqs_compliant: is_naaqs_ok,
        is_plume_rise_adequate: is_plume_ok,
    })
}

/// Helper function to compute Briggs rural dispersion coefficients.
fn compute_briggs_sigmas(class: AtmosphericStabilityClass, x: f64) -> (f64, f64) {
    let (sy_a, sz_a, sz_b, sz_c) = match class {
        AtmosphericStabilityClass::ClassAExtremelyUnstable => (0.22, 0.20, 0.0, 1.0),
        AtmosphericStabilityClass::ClassBModeratelyUnstable => (0.16, 0.12, 0.0, 1.0),
        AtmosphericStabilityClass::ClassCSlightlyUnstable => (0.11, 0.08, 0.0002, -0.5),
        AtmosphericStabilityClass::ClassDNeutral => (0.08, 0.06, 0.0015, -0.5),
        AtmosphericStabilityClass::ClassESlightlyStable => (0.06, 0.03, 0.0003, -1.0),
        AtmosphericStabilityClass::ClassFModeratelyStable => (0.04, 0.016, 0.0003, -1.0),
    };

    let sy = (sy_a * x) / (1.0 + 0.0001 * x).sqrt();
    let sz = if sz_b == 0.0 {
        sz_a * x
    } else {
        (sz_a * x) * (1.0 + sz_b * x).powf(sz_c)
    };

    (sy.max(1.0), sz.max(1.0))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_power_plant_stack_dispersion() -> Result<(), String> {
        let prob = AtmosphericDispersionProblem::default();
        let res = solve_atmospheric_dispersion(&prob)?;

        assert!(res.buoyancy_flux_m4_per_s3 > 80.0 && res.buoyancy_flux_m4_per_s3 < 200.0);
        assert!(res.plume_rise_delta_h_m > 70.0 && res.plume_rise_delta_h_m < 180.0);
        assert!(res.effective_stack_height_m > 150.0 && res.effective_stack_height_m < 280.0);
        assert!(res.sigma_y_m > 120.0 && res.sigma_y_m < 250.0);
        assert!(res.sigma_z_m > 40.0 && res.sigma_z_m < 120.0);
        assert!(res.distance_to_max_concentration_m > 5000.0 && res.distance_to_max_concentration_m < 12000.0);
        assert!(res.peak_max_ground_concentration_ug_per_m3 > 10.0 && res.peak_max_ground_concentration_ug_per_m3 < 50.0);
        assert!(res.naaqs_compliance_ratio < 1.0);
        assert!(res.is_naaqs_compliant);
        assert!(res.is_plume_rise_adequate);
        Ok(())
    }
}

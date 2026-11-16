// crates/atlas-simulation-core/src/rotorcraft_aerodynamics.rs
//
// Aerospace & Rotary-Wing Engineering:
// Rotorcraft Helicopter Hover and Forward Flight Aerodynamics Solver.
//
// Based on:
// - Leishman (2006): "Principles of Helicopter Aerodynamics" (Cambridge University Press)
// - Prouty (2002): "Helicopter Performance, Stability, and Control" (Krieger)
// - Glauert (1926): "A General Theory of the Autogyro including a Discussion of the Whistler"
// - Stepniewski & Keys (1984): "Rotary-Wing Aerodynamics" (NASA CR-3082)
// - FAA FAR Part 27 / Part 29: Airworthiness Standards for Transport Category Rotorcraft
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "rotorcraft_aerodynamics_problem".to_string()
}

/// Helicopter rotor operational flight state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum RotorFlightRegime {
    /// Stationary out-of-ground-effect (OGE) hover.
    #[default]
    HoverOge,
    /// Level cruising forward flight at specified airspeed.
    LevelForwardFlight,
    /// Vertical climb at steady climb rate.
    VerticalClimb,
}

/// Rotorcraft aerodynamics simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotorcraftAerodynamicsProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Flight regime under analysis.
    #[serde(default)]
    pub regime: RotorFlightRegime,
    /// Gross helicopter takeoff weight in kilograms (kg, e.g. 4500 kg for medium twin).
    pub gross_weight_kg: f64,
    /// Main rotor radius R in meters (m, typ. 5.5 - 8.5 m).
    pub rotor_radius_m: f64,
    /// Number of main rotor blades N_b (typ. 4 or 5).
    pub number_of_blades: u32,
    /// Mean blade chord c in meters (m, typ. 0.30 - 0.55 m).
    pub blade_chord_m: f64,
    /// Rotor rotational speed in RPM (typ. 300 - 450 RPM).
    pub rotor_speed_rpm: f64,
    /// Forward flight airspeed in knots (kt, e.g. 130 kt for cruise; 0.0 for hover).
    pub forward_speed_knots: f64,
    /// Ambient air density in kilograms per cubic meter (kg/m³, typ. 1.225 at sea level).
    pub air_density_kg_m3: f64,
    /// Speed of sound in ambient air in meters per second (m/s, typ. 340.3 m/s).
    pub speed_of_sound_m_s: f64,
    /// Blade mean section profile zero-lift drag coefficient C_d0 (typ. 0.0085 - 0.0110).
    pub blade_profile_cd0: f64,
    /// Fuselage equivalent flat plate parasite drag area f in square meters (m², typ. 1.2 - 2.5 m²).
    pub fuselage_flat_plate_area_m2: f64,
}

impl Default for RotorcraftAerodynamicsProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            regime: RotorFlightRegime::HoverOge,
            gross_weight_kg: 4300.0,
            rotor_radius_m: 6.50,
            number_of_blades: 4,
            blade_chord_m: 0.38,
            rotor_speed_rpm: 345.0,
            forward_speed_knots: 0.0,
            air_density_kg_m3: 1.225,
            speed_of_sound_m_s: 340.3,
            blade_profile_cd0: 0.0095,
            fuselage_flat_plate_area_m2: 1.65,
        }
    }
}

/// Rotorcraft aerodynamics simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RotorcraftAerodynamicsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Rotor disk area in square meters (m²).
    pub rotor_disk_area_m2: f64,
    /// Rotor solidity sigma (total blade area / disk area).
    pub rotor_solidity: f64,
    /// Blade tip speed V_tip = Omega * R in meters per second (m/s).
    pub blade_tip_speed_m_s: f64,
    /// Advance ratio mu = V_inf / V_tip (dimensionless).
    pub advance_ratio_mu: f64,
    /// Rotor thrust coefficient C_T.
    pub thrust_coefficient_ct: f64,
    /// Blade loading coefficient C_T / sigma.
    pub blade_loading_ct_over_sigma: f64,
    /// Inflow induced velocity v_i in meters per second (m/s).
    pub induced_velocity_m_s: f64,
    /// Rotor induced power in kiloWatts (kW).
    pub induced_power_kw: f64,
    /// Rotor blade profile drag power in kiloWatts (kW).
    pub profile_power_kw: f64,
    /// Fuselage parasite drag power in kiloWatts (kW).
    pub parasite_power_kw: f64,
    /// Total required main and tail rotor shaft engine power in kiloWatts (kW).
    pub total_engine_shaft_power_kw: f64,
    /// Hover aerodynamic Figure of Merit (FM = P_ideal / P_actual).
    pub figure_of_merit: f64,
    /// Advancing blade tip Mach number M_(1,90).
    pub advancing_tip_mach_number: f64,
    /// Whether rotor blade loading is safely below stall boundary (C_T / sigma <= 0.12).
    pub is_blade_loading_within_limits: bool,
    /// Whether advancing blade tip is free from severe transonic compressibility drag rise (M_tip <= 0.85).
    pub is_tip_mach_non_critical: bool,
}

/// Solves Glauert momentum theory, blade element profile drag, forward flight power bucket, and tip Mach number.
pub fn solve_rotorcraft_aerodynamics(
    problem: &RotorcraftAerodynamicsProblem,
) -> Result<RotorcraftAerodynamicsResult, String> {
    if problem.gross_weight_kg <= 100.0 || problem.gross_weight_kg >= 60000.0 {
        return Err("Gross weight must be between 100 and 60000 kg".to_string());
    }
    if problem.rotor_radius_m <= 1.0 || problem.blade_chord_m <= 0.05 {
        return Err("Rotor radius and blade chord must be realistic rotary wing dimensions".to_string());
    }
    if problem.number_of_blades < 2 || problem.number_of_blades > 8 {
        return Err("Number of blades must be between 2 and 8".to_string());
    }
    if problem.rotor_speed_rpm <= 50.0 || problem.rotor_speed_rpm >= 1500.0 {
        return Err("Rotor speed must be between 50 and 1500 RPM".to_string());
    }

    let g_accel = 9.80665;
    let thrust_n = problem.gross_weight_kg * g_accel;
    let disk_area_m2 = std::f64::consts::PI * problem.rotor_radius_m * problem.rotor_radius_m;

    // Rotor solidity: sigma = (N_b * c) / (pi * R)
    let total_blade_span_area = (problem.number_of_blades as f64) * problem.blade_chord_m * problem.rotor_radius_m;
    let sigma = total_blade_span_area / disk_area_m2;

    // Angular velocity and tip speed:
    let omega_rad_s = (problem.rotor_speed_rpm * 2.0 * std::f64::consts::PI) / 60.0;
    let v_tip_m_s = omega_rad_s * problem.rotor_radius_m;

    // Forward flight velocity in m/s:
    let v_inf_m_s = problem.forward_speed_knots * 0.514444; // 1 knot = 0.514444 m/s
    let mu = v_inf_m_s / v_tip_m_s;

    // Rotor nondimensional coefficients:
    // C_T = T / (rho * A * V_tip^2)
    let rho = problem.air_density_kg_m3;
    let c_t = thrust_n / (rho * disk_area_m2 * v_tip_m_s * v_tip_m_s);
    let ct_over_sigma = c_t / sigma;

    // Hover induced velocity (momentum theory): v_i0 = sqrt(T / (2 * rho * A))
    let v_i0 = (thrust_n / (2.0 * rho * disk_area_m2)).sqrt();

    // Inflow velocity in forward flight: Glauert relation
    // v_i = v_i0^2 / sqrt(V_inf^2 + v_i^2)
    let v_i = if v_inf_m_s < 1.0 {
        v_i0
    } else {
        // High speed approximation: v_i ~ v_i0^2 / V_inf
        // Smooth iteration for transition region:
        let mut v_est = v_i0;
        for _ in 0..5 {
            let denom = (v_inf_m_s * v_inf_m_s + v_est * v_est).sqrt();
            v_est = (v_i0 * v_i0) / denom;
        }
        v_est
    };

    // Induced power: P_i = kappa * T * v_i (kappa ~ 1.15 empirical non-uniform inflow factor)
    let kappa = 1.15;
    let p_i_w = kappa * thrust_n * v_i;
    let p_i_kw = p_i_w / 1000.0;

    // Profile power:
    // Hover: P_0 = (1/8) * rho * sigma * C_d0 * A * V_tip^3
    // Forward flight: P_0(mu) = P_0 * (1 + 4.65 * mu^2)
    let p_0_hover_w = 0.125 * rho * sigma * problem.blade_profile_cd0 * disk_area_m2 * v_tip_m_s.powi(3);
    let p_0_w = p_0_hover_w * (1.0 + 4.65 * mu * mu);
    let p_0_kw = p_0_w / 1000.0;

    // Parasite power: P_par = 0.5 * rho * V_inf^3 * f_plate
    let p_par_w = 0.5 * rho * v_inf_m_s.powi(3) * problem.fuselage_flat_plate_area_m2;
    let p_par_kw = p_par_w / 1000.0;

    // Tail rotor and transmission loss allowance (~ 10% of main rotor power):
    let main_rotor_kw = p_i_kw + p_0_kw + p_par_kw;
    let tail_and_losses_kw = main_rotor_kw * 0.10;
    let total_shaft_kw = main_rotor_kw + tail_and_losses_kw;

    // Hover Figure of Merit (FM):
    // FM = (T * v_i0) / (P_hover_actual)
    let ideal_hover_power_w = thrust_n * v_i0;
    let actual_hover_main_kw = (kappa * thrust_n * v_i0 + p_0_hover_w) / 1000.0;
    let figure_of_merit = (ideal_hover_power_w / 1000.0) / actual_hover_main_kw;

    // Advancing blade tip Mach number: M_(1,90) = (V_tip + V_inf) / a_sound
    let v_advancing_tip = v_tip_m_s + v_inf_m_s;
    let m_advancing_tip = v_advancing_tip / problem.speed_of_sound_m_s;

    let is_ct_sigma_ok = ct_over_sigma <= 0.12;
    let is_tip_mach_ok = m_advancing_tip <= 0.85;

    Ok(RotorcraftAerodynamicsResult {
        problem_id: problem.problem_id.clone(),
        rotor_disk_area_m2: disk_area_m2,
        rotor_solidity: sigma,
        blade_tip_speed_m_s: v_tip_m_s,
        advance_ratio_mu: mu,
        thrust_coefficient_ct: c_t,
        blade_loading_ct_over_sigma: ct_over_sigma,
        induced_velocity_m_s: v_i,
        induced_power_kw: p_i_kw,
        profile_power_kw: p_0_kw,
        parasite_power_kw: p_par_kw,
        total_engine_shaft_power_kw: total_shaft_kw,
        figure_of_merit,
        advancing_tip_mach_number: m_advancing_tip,
        is_blade_loading_within_limits: is_ct_sigma_ok,
        is_tip_mach_non_critical: is_tip_mach_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_helicopter_hover_and_cruise() -> Result<(), String> {
        // Test Hover OGE
        let hover_prob = RotorcraftAerodynamicsProblem {
            problem_id: "medium_twin_hover".to_string(),
            regime: RotorFlightRegime::HoverOge,
            gross_weight_kg: 4300.0,
            rotor_radius_m: 6.50,
            number_of_blades: 4,
            blade_chord_m: 0.38,
            rotor_speed_rpm: 345.0,
            forward_speed_knots: 0.0,
            air_density_kg_m3: 1.225,
            speed_of_sound_m_s: 340.3,
            blade_profile_cd0: 0.0095,
            fuselage_flat_plate_area_m2: 1.65,
        };

        let hover_res = solve_rotorcraft_aerodynamics(&hover_prob)?;
        assert!(hover_res.blade_tip_speed_m_s > 220.0 && hover_res.blade_tip_speed_m_s < 250.0);
        assert!(hover_res.induced_velocity_m_s > 10.0 && hover_res.induced_velocity_m_s < 14.0);
        assert!(hover_res.figure_of_merit > 0.65 && hover_res.figure_of_merit < 0.82);
        assert!(hover_res.total_engine_shaft_power_kw > 600.0 && hover_res.total_engine_shaft_power_kw < 950.0);
        assert!(hover_res.is_blade_loading_within_limits);
        assert!(hover_res.is_tip_mach_non_critical);

        // Test Forward Cruise at 130 knots
        let cruise_prob = RotorcraftAerodynamicsProblem {
            problem_id: "medium_twin_cruise_130kt".to_string(),
            regime: RotorFlightRegime::LevelForwardFlight,
            forward_speed_knots: 130.0,
            ..hover_prob
        };

        let cruise_res = solve_rotorcraft_aerodynamics(&cruise_prob)?;
        assert!(cruise_res.advance_ratio_mu > 0.25 && cruise_res.advance_ratio_mu < 0.35);
        assert!(cruise_res.induced_power_kw < hover_res.induced_power_kw); // Induced power drop in cruise
        assert!(cruise_res.parasite_power_kw > 150.0); // High parasite drag in cruise
        assert!(cruise_res.advancing_tip_mach_number > 0.80 && cruise_res.advancing_tip_mach_number < 0.89);
        Ok(())
    }
}

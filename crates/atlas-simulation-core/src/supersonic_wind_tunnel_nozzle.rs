//! Supersonic Wind Tunnel Convergent-Divergent de Laval Nozzle Aerodynamics & Shock Wave Analysis
//!
//! Formulated according to:
//! - NASA SP-3012 "Flow Properties of Normal Shock Waves" & Compressible Flow Tables
//! - Anderson, J. D., "Modern Compressible Flow: With Historical Perspective", McGraw-Hill
//! - ISO 5167 "Measurement of fluid flow by means of pressure differential devices inserted in circular cross-section conduits"
//! - Shapiro, A. H., "The Dynamics and Thermodynamics of Compressible Fluid Flow", Ronald Press
//!
//! Evaluates 1D isentropic compressible flow, sonic throat choked mass flow,
//! supersonic de Laval nozzle area-Mach ratio expansion, normal shock wave jump kinetics (Rankine-Hugoniot),
//! oblique shock deflection ramps, and supersonic wind tunnel test section flow quality.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Working gas fluid selection for supersonic wind tunnel facility.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SupersonicGasMedium {
    /// Dry Atmospheric Air (gamma = 1.400, R = 287.05 J/(kg*K)).
    DryAir,
    /// Pure Nitrogen N2 (gamma = 1.400, R = 296.80 J/(kg*K)).
    NitrogenN2,
    /// Carbon Dioxide CO2 (gamma = 1.289, R = 188.92 J/(kg*K), e.g. Martian atmospheric entry).
    CarbonDioxideCO2,
    /// Helium He (gamma = 1.667, R = 2077.1 J/(kg*K), e.g. high-Mach hypervelocity tunnel).
    HeliumHe,
}

impl SupersonicGasMedium {
    /// Ratio of specific heats gamma (cp / cv).
    pub fn specific_heat_ratio(&self) -> f64 {
        match self {
            Self::DryAir => 1.400,
            Self::NitrogenN2 => 1.400,
            Self::CarbonDioxideCO2 => 1.289,
            Self::HeliumHe => 1.667,
        }
    }

    /// Specific gas constant R in J/(kg*K).
    pub fn specific_gas_constant(&self) -> f64 {
        match self {
            Self::DryAir => 287.05,
            Self::NitrogenN2 => 296.80,
            Self::CarbonDioxideCO2 => 188.92,
            Self::HeliumHe => 2077.1,
        }
    }
}

/// Nozzle contour design geometry method.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum NozzleContourMethod {
    /// Method of Characteristics (MOC) minimum length nozzle with uniform parallel exit flow.
    MethodOfCharacteristicsMOC,
    /// Conical diverging nozzle with divergence half-angle correction.
    ConicalDiverging,
    /// Rao parabolic bell approximation for high thrust/test uniformity.
    RaoParabolicBell,
}

/// Input parameters for supersonic wind tunnel nozzle flow and shock wave simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersonicNozzleProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Gas medium.
    pub gas_medium: SupersonicGasMedium,
    /// Nozzle contouring methodology.
    pub contour_method: NozzleContourMethod,
    /// Stagnation total pressure in reservoir P_0 in kiloPascals (kPa, standard 100 to 5000 kPa).
    pub stagnation_pressure_kpa: f64,
    /// Stagnation total temperature in reservoir T_0 in Kelvin (K, standard 280 to 800 K).
    pub stagnation_temperature_k: f64,
    /// Target test section design Mach number M_design (standard 1.20 to 5.00).
    pub design_mach_number: f64,
    /// Throat cross-sectional diameter D_throat in millimeters (mm, standard 20 to 400 mm).
    pub throat_diameter_mm: f64,
    /// Ambient test section or exhaust backpressure P_back in kiloPascals (kPa, standard 5 to 101.3 kPa).
    pub ambient_backpressure_kpa: f64,
    /// Model wedge flow deflection half-angle in test section in degrees (deg, standard 0 to 25 deg).
    pub model_deflection_angle_deg: f64,
    /// Conical nozzle divergence half-angle alpha in degrees (standard 8 to 18 deg).
    pub divergence_half_angle_deg: f64,
}

impl Default for SupersonicNozzleProblem {
    fn default() -> Self {
        Self {
            problem_id: "trisonic_aerospace_tunnel_m2".to_string(),
            gas_medium: SupersonicGasMedium::DryAir,
            contour_method: NozzleContourMethod::MethodOfCharacteristicsMOC,
            stagnation_pressure_kpa: 450.0,         // 4.5 bar total pressure
            stagnation_temperature_k: 320.0,        // 320 K (47 °C)
            design_mach_number: 2.20,               // Mach 2.2 supersonic test section
            throat_diameter_mm: 85.0,               // 85 mm throat diameter
            ambient_backpressure_kpa: 42.0,         // 42 kPa ambient backpressure
            model_deflection_angle_deg: 10.0,       // 10 deg wedge deflection
            divergence_half_angle_deg: 12.0,        // 12 deg half-angle
        }
    }
}

/// Simulation results for supersonic nozzle flow, expansion, and shock wave properties.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupersonicNozzleResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Critical sonic throat cross-sectional area A_star in square millimeters (mm²).
    pub throat_area_mm2: f64,
    /// Nozzle exit cross-sectional area A_exit in square millimeters (mm²).
    pub exit_area_mm2: f64,
    /// Expansion area ratio (A_exit / A_star).
    pub area_expansion_ratio: f64,
    /// Choked sonic mass flow rate m_dot in kilograms per second (kg/s).
    pub choked_mass_flow_rate_kg_s: f64,
    /// Critical sonic throat static pressure P_star in kiloPascals (kPa).
    pub throat_static_pressure_kpa: f64,
    /// Critical sonic throat static temperature T_star in Kelvin (K).
    pub throat_static_temperature_k: f64,
    /// Critical sonic velocity at throat a_star in meters per second (m/s).
    pub throat_sonic_velocity_m_s: f64,
    /// Isentropic static pressure at nozzle exit P_exit in kiloPascals (kPa).
    pub exit_static_pressure_kpa: f64,
    /// Isentropic static temperature at nozzle exit T_exit in Kelvin (K).
    pub exit_static_temperature_k: f64,
    /// Flow velocity at nozzle exit V_exit in meters per second (m/s).
    pub exit_flow_velocity_m_s: f64,
    /// Normal shock downstream Mach number M_2 if shock forms at nozzle exit.
    pub normal_shock_downstream_mach: f64,
    /// Normal shock static pressure jump ratio (P_2 / P_1).
    pub normal_shock_pressure_ratio: f64,
    /// Total stagnation pressure recovery across normal shock (P_02 / P_01).
    pub normal_shock_total_pressure_recovery: f64,
    /// Oblique shock wave angle beta in degrees (deg) across model wedge in test section.
    pub oblique_shock_wave_angle_deg: f64,
    /// Oblique shock downstream Mach number M_2_oblique.
    pub oblique_shock_downstream_mach: f64,
    /// Diverging nozzle axial length L_divergent in millimeters (mm).
    pub nozzle_divergent_length_mm: f64,
    /// Assessment indicating nozzle operates choked at throat (P_0 / P_back >= critical ratio).
    pub is_choked_at_throat: bool,
    /// Assessment indicating exit flow is fully supersonic (M_exit >= 1.0).
    pub is_supersonic_flow_established: bool,
    /// Assessment indicating normal shock wave is avoided inside nozzle diverging duct.
    pub is_free_of_internal_shocks: bool,
    /// Overall supersonic wind tunnel nozzle aerodynamic viability.
    pub is_nozzle_aerodynamically_viable: bool,
}

/// Helper function to compute Area-Mach ratio A/A* for a given Mach number and gamma.
pub fn area_mach_ratio(mach: f64, gamma: f64) -> f64 {
    if mach <= 0.0 {
        return f64::INFINITY;
    }
    let gm1 = gamma - 1.0;
    let gp1 = gamma + 1.0;
    let factor = (2.0 + gm1 * mach * mach) / gp1;
    let exponent = gp1 / (2.0 * gm1);
    (1.0 / mach) * factor.powf(exponent)
}

/// Solves supersonic de Laval nozzle flow and shock wave kinetics per NASA SP-3012 / Anderson.
pub fn solve_supersonic_wind_tunnel_nozzle(
    problem: &SupersonicNozzleProblem,
) -> Result<SupersonicNozzleResult, String> {
    if problem.stagnation_pressure_kpa <= 10.0 {
        return Err("Stagnation pressure must be greater than 10 kPa".to_string());
    }
    if problem.stagnation_temperature_k <= 100.0 {
        return Err("Stagnation temperature must be greater than 100 K".to_string());
    }
    if problem.design_mach_number <= 1.0 {
        return Err("Design Mach number must be strictly supersonic (M > 1.0)".to_string());
    }
    if problem.throat_diameter_mm <= 1.0 {
        return Err("Throat diameter must be positive and greater than 1 mm".to_string());
    }

    let gamma = problem.gas_medium.specific_heat_ratio();
    let r_gas = problem.gas_medium.specific_gas_constant();
    let p0_pa = problem.stagnation_pressure_kpa * 1000.0;
    let t0_k = problem.stagnation_temperature_k;
    let m_des = problem.design_mach_number;

    // 1. Critical sonic throat parameters (M = 1.0):
    let gm1 = gamma - 1.0;
    let gp1 = gamma + 1.0;
    let t_star_k = t0_k * (2.0 / gp1);
    let p_star_pa = p0_pa * (2.0 / gp1).powf(gamma / gm1);
    let rho_star_kg_m3 = p_star_pa / (r_gas * t_star_k);
    let a_star_m_s = (gamma * r_gas * t_star_k).sqrt();

    let r_throat_m = (problem.throat_diameter_mm * 0.001) / 2.0;
    let a_throat_m2 = std::f64::consts::PI * r_throat_m.powi(2);
    let a_throat_mm2 = a_throat_m2 * 1.0e6;

    // Choked mass flow rate:
    let choked_m_dot_kg_s = rho_star_kg_m3 * a_throat_m2 * a_star_m_s;

    // 2. Isentropic expansion to design Mach number at nozzle exit:
    let ar_exit = area_mach_ratio(m_des, gamma);
    let a_exit_m2 = a_throat_m2 * ar_exit;
    let a_exit_mm2 = a_exit_m2 * 1.0e6;
    let r_exit_m = (a_exit_m2 / std::f64::consts::PI).sqrt();

    // Isentropic static temperature and pressure at exit:
    let t_exit_k = t0_k / (1.0 + 0.5 * gm1 * m_des.powi(2));
    let p_exit_pa = p0_pa / (1.0 + 0.5 * gm1 * m_des.powi(2)).powf(gamma / gm1);
    let a_exit_speed_m_s = (gamma * r_gas * t_exit_k).sqrt();
    let v_exit_m_s = m_des * a_exit_speed_m_s;

    // 3. Normal shock wave jump relations at exit Mach (Rankine-Hugoniot):
    // M2^2 = (2 + (gamma - 1)*M1^2) / (2*gamma*M1^2 - (gamma - 1))
    let num_m2 = 2.0 + gm1 * m_des.powi(2);
    let den_m2 = 2.0 * gamma * m_des.powi(2) - gm1;
    let m2_normal = (num_m2 / den_m2).sqrt();

    // P2 / P1 = 1 + (2*gamma / (gamma + 1)) * (M1^2 - 1)
    let p_ratio_normal = 1.0 + (2.0 * gamma / gp1) * (m_des.powi(2) - 1.0);

    // Stagnation pressure recovery P02 / P01:
    // P02 / P01 = [ (gp1 * M1^2) / (2 + gm1*M1^2) ]^(gamma/(gamma-1)) * [ gp1 / (2*gamma*M1^2 - gm1) ]^(1/(gamma-1))
    let term1 = (gp1 * m_des.powi(2)) / (2.0 + gm1 * m_des.powi(2));
    let term2 = gp1 / (2.0 * gamma * m_des.powi(2) - gm1);
    let p0_recovery = term1.powf(gamma / gm1) * term2.powf(1.0 / gm1);

    // 4. Oblique shock wave calculation on wedge ramp (theta-beta-M):
    let theta_rad = problem.model_deflection_angle_deg.to_radians();
    // Solve tan(theta) = 2 cot(beta) * (M1^2 sin^2(beta) - 1) / (M1^2 (gamma + cos(2*beta)) + 2)
    // Newton-Raphson for weak shock wave solution beta:
    let mut beta_rad = theta_rad + (1.0 / m_des).asin(); // Initial guess above Mach angle
    if beta_rad >= std::f64::consts::FRAC_PI_2 {
        beta_rad = 0.8 * std::f64::consts::FRAC_PI_2;
    }
    for _ in 0..30 {
        let sin_b = beta_rad.sin();
        let cos_b = beta_rad.cos();
        let sin2_b = sin_b * sin_b;
        let cos_2b = (2.0 * beta_rad).cos();

        let num = 2.0 * (cos_b / sin_b) * (m_des.powi(2) * sin2_b - 1.0);
        let den = m_des.powi(2) * (gamma + cos_2b) + 2.0;
        let tan_th_calc = num / den;
        let f = tan_th_calc - theta_rad.tan();

        // Numerical derivative
        let d_beta = 1.0e-5;
        let b2 = beta_rad + d_beta;
        let sin_b2 = b2.sin();
        let cos_b2 = b2.cos();
        let num2 = 2.0 * (cos_b2 / sin_b2) * (m_des.powi(2) * sin_b2 * sin_b2 - 1.0);
        let den2 = m_des.powi(2) * (gamma + (2.0 * b2).cos()) + 2.0;
        let df = (num2 / den2 - theta_rad.tan() - f) / d_beta;

        let delta = f / df;
        beta_rad -= delta;
        if delta.abs() < 1.0e-7 {
            break;
        }
    }
    let beta_deg = beta_rad.to_degrees();

    // Oblique shock downstream Mach number M2:
    let mn1 = m_des * beta_rad.sin();
    let mn2 = ((2.0 + gm1 * mn1.powi(2)) / (2.0 * gamma * mn1.powi(2) - gm1)).sqrt();
    let m2_oblique = mn2 / (beta_rad - theta_rad).sin();

    // 5. Nozzle divergent length:
    let half_angle_rad = problem.divergence_half_angle_deg.to_radians();
    let l_div_mm = match problem.contour_method {
        NozzleContourMethod::MethodOfCharacteristicsMOC => {
            // MOC nozzle length ~ 0.8 * (r_exit - r_throat) / tan(theta_inflection)
            0.85 * (r_exit_m - r_throat_m) * 1000.0 / half_angle_rad.tan()
        }
        NozzleContourMethod::ConicalDiverging => {
            (r_exit_m - r_throat_m) * 1000.0 / half_angle_rad.tan()
        }
        NozzleContourMethod::RaoParabolicBell => {
            0.80 * (r_exit_m - r_throat_m) * 1000.0 / half_angle_rad.tan()
        }
    };

    // 6. Safety and aerodynamic feasibility assessments:
    let p_back_pa = problem.ambient_backpressure_kpa * 1000.0;
    let critical_pressure_ratio = (2.0 / gp1).powf(gamma / gm1);
    let is_choked_at_throat = (p_back_pa / p0_pa) <= critical_pressure_ratio;
    let is_supersonic_flow_established = m_des >= 1.0 && p0_pa > p_back_pa;
    // Internal shock avoided if backpressure does not cause normal shock inside duct:
    let is_free_of_internal_shocks = p_back_pa <= p_exit_pa * p_ratio_normal;
    let is_nozzle_aerodynamically_viable =
        is_choked_at_throat && is_supersonic_flow_established && is_free_of_internal_shocks;

    Ok(SupersonicNozzleResult {
        problem_id: problem.problem_id.clone(),
        throat_area_mm2: a_throat_mm2,
        exit_area_mm2: a_exit_mm2,
        area_expansion_ratio: ar_exit,
        choked_mass_flow_rate_kg_s: choked_m_dot_kg_s,
        throat_static_pressure_kpa: p_star_pa / 1000.0,
        throat_static_temperature_k: t_star_k,
        throat_sonic_velocity_m_s: a_star_m_s,
        exit_static_pressure_kpa: p_exit_pa / 1000.0,
        exit_static_temperature_k: t_exit_k,
        exit_flow_velocity_m_s: v_exit_m_s,
        normal_shock_downstream_mach: m2_normal,
        normal_shock_pressure_ratio: p_ratio_normal,
        normal_shock_total_pressure_recovery: p0_recovery,
        oblique_shock_wave_angle_deg: beta_deg,
        oblique_shock_downstream_mach: m2_oblique,
        nozzle_divergent_length_mm: l_div_mm,
        is_choked_at_throat,
        is_supersonic_flow_established,
        is_free_of_internal_shocks,
        is_nozzle_aerodynamically_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_supersonic_mach22_air_nozzle() -> Result<(), String> {
        let problem = SupersonicNozzleProblem::default();
        let res = solve_supersonic_wind_tunnel_nozzle(&problem)?;

        assert!(res.throat_area_mm2 > 5000.0 && res.throat_area_mm2 < 6000.0);
        assert!(res.area_expansion_ratio > 1.90 && res.area_expansion_ratio < 2.15); // A/A* ~ 2.005 for M=2.2
        assert!(res.choked_mass_flow_rate_kg_s > 5.0 && res.choked_mass_flow_rate_kg_s < 8.0);
        assert!(res.exit_static_pressure_kpa > 35.0 && res.exit_static_pressure_kpa < 48.0);
        assert!(res.exit_flow_velocity_m_s > 500.0 && res.exit_flow_velocity_m_s < 600.0);
        assert!(res.normal_shock_downstream_mach > 0.50 && res.normal_shock_downstream_mach < 0.60);
        assert!(res.normal_shock_total_pressure_recovery > 0.60 && res.normal_shock_total_pressure_recovery < 0.70);
        assert!(res.oblique_shock_wave_angle_deg > 30.0 && res.oblique_shock_wave_angle_deg < 40.0);
        assert!(res.is_choked_at_throat);
        assert!(res.is_supersonic_flow_established);
        assert!(res.is_free_of_internal_shocks);
        assert!(res.is_nozzle_aerodynamically_viable);
        Ok(())
    }

    #[test]
    fn test_invalid_subsonic_mach() {
        let problem = SupersonicNozzleProblem {
            design_mach_number: 0.85,
            ..Default::default()
        };
        assert!(solve_supersonic_wind_tunnel_nozzle(&problem).is_err());
    }
}

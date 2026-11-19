//! Offshore Monopile / Jacket Wave & Current Hydrodynamics (API RP 2A-WSD / DNV-RP-C205 / Morison)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Airy linear wave dispersion relation omega^2 = g*k*tanh(k*d) via exact Newton-Raphson solver.
//! 2. Wave orbital kinematics (horizontal particle velocity u(z,t) and local acceleration du/dt).
//! 3. Collinear 1/7th power-law ocean current velocity profile u_c(z).
//! 4. Morison equation hydrodynamic drag and inertia forces along vertical cylinder column.
//! 5. Integrated total mudline base shear force F_base (kN) and overturning moment M_base (kN*m).
//! 6. Keulegan-Carpenter number KC, MacCamy-Fuchs diffraction limit check D/L <= 0.20, and foundation safety.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Offshore foundation structural structural member classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OffshoreStructureType {
    /// Offshore Wind Turbine Large-Diameter Monopile (D = 6 to 10 m).
    OffshoreWindMonopile,
    /// Oil & Gas Jacket Platform Leg / Bracing Member (D = 1 to 3 m).
    JacketPlatformLeg,
    /// Subsea Template / Wellhead Conductor Casing (D = 0.5 to 1.2 m).
    SubseaConductor,
}

/// Input parameters for offshore wave and current hydrodynamic loading analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffshoreWaveProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Offshore structure structural category.
    pub structure_type: OffshoreStructureType,
    /// Mean sea water depth d in meters (m, standard 15 to 150 m).
    pub water_depth_m: f64,
    /// Design extreme wave height H in meters (m, crest-to-trough, standard 3 to 30 m).
    pub wave_height_m: f64,
    /// Design wave peak period T in seconds (s, standard 5 to 20 s).
    pub wave_period_s: f64,
    /// Surface ocean current velocity v_c in meters per second (m/s, standard 0.2 to 2.5 m/s).
    pub surface_current_m_s: f64,
    /// Outer cylinder diameter D in meters (m, standard 0.5 to 12.0 m).
    pub cylinder_diameter_m: f64,
    /// Steel wall thickness t in millimeters (mm, standard 20 to 120 mm).
    pub wall_thickness_mm: f64,
    /// Steel yield strength f_y in MegaPascals (MPa, standard 355 to 500 MPa).
    pub steel_yield_strength_mpa: f64,
    /// Hydrodynamic drag coefficient C_D (standard 0.65 to 1.05 per API RP 2A).
    pub drag_coefficient_cd: f64,
    /// Hydrodynamic inertia/added mass coefficient C_M (standard 1.6 to 2.0 per API RP 2A).
    pub inertia_coefficient_cm: f64,
    /// Seawater mass density rho_w in kilograms per cubic meter (standard 1025 kg/m³).
    pub seawater_density_kg_m3: f64,
}

impl Default for OffshoreWaveProblem {
    fn default() -> Self {
        Self {
            problem_id: "north_sea_15mw_monopile".to_string(),
            structure_type: OffshoreStructureType::OffshoreWindMonopile,
            water_depth_m: 40.0,                // 40 m North Sea water depth
            wave_height_m: 14.5,                // 50-year extreme design wave height H = 14.5 m
            wave_period_s: 13.0,                // Peak wave period T = 13.0 s
            surface_current_m_s: 1.20,          // 1.2 m/s tidal + wind storm current
            cylinder_diameter_m: 8.50,          // Ø8.50 m heavy monopile
            wall_thickness_mm: 85.0,            // 85 mm structural steel wall
            steel_yield_strength_mpa: 355.0,    // S355 ML subsea structural steel
            drag_coefficient_cd: 0.70,          // Post-critical Reynolds smooth/marine fouled cylinder
            inertia_coefficient_cm: 2.00,       // Standard potential flow inertia coefficient
            seawater_density_kg_m3: 1025.0,     // Seawater
        }
    }
}

/// Simulation results for wave kinematics, Morison forces, base shear, and overturning moment.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OffshoreWaveResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Solved wave length L in meters (m).
    pub wavelength_m: f64,
    /// Wave celerity / phase velocity c = L / T in meters per second (m/s).
    pub wave_celerity_m_s: f64,
    /// Wave angular frequency omega = 2*pi / T in radians per second (rad/s).
    pub angular_frequency_rad_s: f64,
    /// Solved wavenumber k = 2*pi / L in radians per meter (rad/m).
    pub wavenumber_rad_m: f64,
    /// Relative water depth k*d dimensionless.
    pub relative_depth_kd: f64,
    /// Maximum horizontal surface wave particle velocity u_max in meters per second (m/s).
    pub surface_wave_velocity_m_s: f64,
    /// Maximum horizontal surface wave acceleration du/dt in meters per second squared (m/s²).
    pub surface_wave_acceleration_m_s2: f64,
    /// Keulegan-Carpenter number KC = u_max * T / D dimensionless.
    pub keulegan_carpenter_number: f64,
    /// Diffraction parameter D / L dimensionless (Morison equation valid when D/L <= 0.20).
    pub diffraction_parameter_d_l: f64,
    /// Peak hydrodynamic mudline base shear force F_base in kiloNewtons (kN).
    pub peak_base_shear_kn: f64,
    /// Peak hydrodynamic mudline overturning moment M_base in kiloNewton-meters (kN*m).
    pub peak_overturning_moment_kn_m: f64,
    /// Maximum bending stress at mudline section in MegaPascals (MPa).
    pub mudline_bending_stress_mpa: f64,
    /// Structural bending yield safety factor (f_y / sigma_bend).
    pub structural_bending_safety_factor: f64,
    /// Assessment indicating Morison formulation validity (diffraction D/L <= 0.20).
    pub is_morison_valid: bool,
    /// Assessment indicating structural bending resistance compliance (safety factor >= 1.35).
    pub is_bending_stress_acceptable: bool,
    /// Overall offshore hydro-structural viability.
    pub is_offshore_design_viable: bool,
}

/// Solves offshore wave dispersion, particle kinematics, and Morison loading according to API RP 2A / DNV-RP-C205.
pub fn solve_offshore_wave_loading(problem: &OffshoreWaveProblem) -> Result<OffshoreWaveResult, String> {
    if problem.water_depth_m < 2.0 || problem.water_depth_m > 1000.0 {
        return Err("Water depth must be between 2.0 and 1000.0 meters".to_string());
    }
    if problem.wave_height_m <= 0.1 || problem.wave_height_m > (0.78 * problem.water_depth_m) {
        return Err("Wave height must be positive and below shallow-water breaking limit (0.78*d)".to_string());
    }
    if problem.wave_period_s < 2.0 || problem.wave_period_s > 40.0 {
        return Err("Wave period must be between 2.0 and 40.0 seconds".to_string());
    }
    if problem.cylinder_diameter_m <= 0.1 {
        return Err("Cylinder diameter must be positive".to_string());
    }

    let g = 9.80665; // m/s²
    let omega = 2.0 * std::f64::consts::PI / problem.wave_period_s;
    let d = problem.water_depth_m;

    // Exact Newton-Raphson solution for Airy dispersion relation: omega^2 = g * k * tanh(k * d)
    // Initial guess from Eckart's deep/shallow water explicit approximation:
    let k_0 = (omega.powi(2) / g) / (omega * (d / g).sqrt()).tanh().sqrt();
    let mut k = k_0;
    for _ in 0..30 {
        let th = (k * d).tanh();
        let f_val = g * k * th - omega.powi(2);
        let df_dk = g * th + g * k * d * (1.0 - th.powi(2));
        let delta = f_val / df_dk;
        k -= delta;
        if delta.abs() < 1.0e-9 {
            break;
        }
    }

    let wavelength_l = 2.0 * std::f64::consts::PI / k;
    let celerity_c = wavelength_l / problem.wave_period_s;
    let kd = k * d;

    let h = problem.wave_height_m;
    let sinh_kd = kd.sinh();

    // Surface orbital velocity and acceleration (at z = 0, cos(omega*t) = 1, sin(omega*t) = 1):
    let u_surf_wave = (std::f64::consts::PI * h / problem.wave_period_s) * (kd.cosh() / sinh_kd);
    let du_surf_wave = (2.0 * std::f64::consts::PI.powi(2) * h / problem.wave_period_s.powi(2)) * (kd.cosh() / sinh_kd);

    // Keulegan-Carpenter number: KC = u_max * T / D:
    let kc = (u_surf_wave * problem.wave_period_s) / problem.cylinder_diameter_m;

    // Diffraction check: D / L <= 0.20 for Morison regime:
    let d_l = problem.cylinder_diameter_m / wavelength_l;

    // Numerical integration of Morison force from mudline (z = -d) to surface (z = 0):
    // f(z) = 0.5 * rho * C_D * D * (u + u_c)^2 + rho * C_M * (pi*D^2/4) * a
    let n_steps = 200;
    let dz = d / (n_steps as f64);
    let r_ext = problem.cylinder_diameter_m / 2.0;
    let a_cyl = std::f64::consts::PI * r_ext.powi(2);

    let mut total_f_base_n = 0.0;
    let mut total_m_base_nm = 0.0;

    for i in 0..n_steps {
        let z = -d + (i as f64 + 0.5) * dz; // depth coordinate from mudline (-d) to surface (0)
        let arm = z + d; // lever arm from mudline

        // Wave horizontal velocity at depth z:
        let cosh_k_zd = (k * (z + d)).cosh();
        let u_wave_z = (std::f64::consts::PI * h / problem.wave_period_s) * (cosh_k_zd / sinh_kd);
        let a_wave_z = (2.0 * std::f64::consts::PI.powi(2) * h / problem.wave_period_s.powi(2)) * (cosh_k_zd / sinh_kd);

        // 1/7th power-law current velocity at depth z:
        let u_curr_z = problem.surface_current_m_s * ((z + d) / d).max(0.001).powf(1.0 / 7.0);

        // Morison force components:
        // Peak force occurs near wave crest where drag and current combine:
        let u_tot = u_wave_z + u_curr_z;
        let f_drag = 0.5 * problem.seawater_density_kg_m3 * problem.drag_coefficient_cd * problem.cylinder_diameter_m * u_tot * u_tot.abs();
        let f_inertia = problem.seawater_density_kg_m3 * problem.inertia_coefficient_cm * a_cyl * a_wave_z;

        // In Morison equations for extreme load, drag and inertia are phase-offset by 90°.
        // Maximum combined load is bounded by sqrt(F_drag^2 + F_inertia^2) or max instantaneous sum:
        let f_section = (f_drag.powi(2) + f_inertia.powi(2)).sqrt();

        total_f_base_n += f_section * dz;
        total_m_base_nm += f_section * arm * dz;
    }

    let f_base_kn = total_f_base_n / 1000.0;
    let m_base_kn_m = total_m_base_nm / 1000.0;

    // Structural mudline bending stress:
    // Hollow circular cylinder: D_ext = D, D_int = D - 2*t
    let t_m = problem.wall_thickness_mm / 1000.0;
    let d_int = (problem.cylinder_diameter_m - 2.0 * t_m).max(0.05);
    // Elastic section modulus: Z_el = (pi / 32) * (D^4 - D_int^4) / D [m³]
    let z_el = (std::f64::consts::PI / 32.0) * (problem.cylinder_diameter_m.powi(4) - d_int.powi(4)) / problem.cylinder_diameter_m;

    // Bending stress: sigma = M / Z_el [N/m²] -> [MPa]:
    let sigma_bend_pa = total_m_base_nm / z_el.max(1.0e-5);
    let sigma_bend_mpa = sigma_bend_pa / 1.0e6;

    let structural_sf = problem.steel_yield_strength_mpa / sigma_bend_mpa.max(0.1);

    // Design verification:
    let is_morison_ok = d_l <= 0.20; // MacCamy-Fuchs diffraction limit
    let is_stress_ok = structural_sf >= 1.35; // API RP 2A allowable stress design margin
    let is_overall_ok = is_morison_ok && is_stress_ok && (f_base_kn > 0.0);

    Ok(OffshoreWaveResult {
        problem_id: problem.problem_id.clone(),
        wavelength_m: wavelength_l,
        wave_celerity_m_s: celerity_c,
        angular_frequency_rad_s: omega,
        wavenumber_rad_m: k,
        relative_depth_kd: kd,
        surface_wave_velocity_m_s: u_surf_wave,
        surface_wave_acceleration_m_s2: du_surf_wave,
        keulegan_carpenter_number: kc,
        diffraction_parameter_d_l: d_l,
        peak_base_shear_kn: f_base_kn,
        peak_overturning_moment_kn_m: m_base_kn_m,
        mudline_bending_stress_mpa: sigma_bend_mpa,
        structural_bending_safety_factor: structural_sf,
        is_morison_valid: is_morison_ok,
        is_bending_stress_acceptable: is_stress_ok,
        is_offshore_design_viable: is_overall_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_north_sea_monopile_wave_loading() -> Result<(), String> {
        let prob = OffshoreWaveProblem::default();
        let res = solve_offshore_wave_loading(&prob)?;

        assert!(res.wavelength_m > 160.0 && res.wavelength_m < 240.0);
        assert!(res.wave_celerity_m_s > 12.0 && res.wave_celerity_m_s < 20.0);
        assert!(res.surface_wave_velocity_m_s > 2.5 && res.surface_wave_velocity_m_s < 5.0);
        assert!(res.keulegan_carpenter_number > 3.0 && res.keulegan_carpenter_number < 10.0);
        assert!(res.diffraction_parameter_d_l <= 0.20, "Diffraction parameter D/L <= 0.20 ensures Morison validity");
        assert!(res.peak_base_shear_kn > 2000.0 && res.peak_base_shear_kn < 8000.0);
        assert!(res.peak_overturning_moment_kn_m > 40000.0 && res.peak_overturning_moment_kn_m < 200000.0);
        assert!(res.structural_bending_safety_factor >= 1.35, "API RP 2A structural bending safety factor >= 1.35");
        assert!(res.is_morison_valid);
        assert!(res.is_bending_stress_acceptable);
        assert!(res.is_offshore_design_viable);
        Ok(())
    }
}

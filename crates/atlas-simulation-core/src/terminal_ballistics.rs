// crates/atlas-simulation-core/src/terminal_ballistics.rs
//
// Defense, Space & Materials Engineering:
// Terminal Ballistics & Armor Hydrodynamic Penetration Mechanics.
//
// Based on:
// - Tate (1967, 1969): "A theory for the deceleration of long-rod penetrators"
// - Alekseevskii (1966): "Penetration of a rod into a target at high velocity"
// - Recht & Ipson (1963): "Ballistic perforation dynamics"
// - NATO STANAG 4569: Protection Levels for Occupants of Armoured Vehicles
// - Rosenberg & Dekel: "Terminal Ballistics" (Springer)
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "terminal_ballistics_problem".to_string()
}

/// Projectile material and construction type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ProjectileType {
    /// Heavy tungsten alloy (WHA) long-rod kinetic penetrator (density ~17,600 kg/m³).
    #[default]
    TungstenLongRod,
    /// Depleted uranium (DU) self-sharpening kinetic penetrator (density ~18,600 kg/m³).
    DepletedUraniumRod,
    /// Hardened tool steel kinetic core (density ~7,850 kg/m³).
    HardenedSteelCore,
}

/// Target armor material configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum TargetArmorType {
    /// Standard Rolled Homogeneous Armor steel (RHA, BHN ~300).
    #[default]
    RolledHomogeneousArmor,
    /// High-Hardness Armor steel (HHA, BHN ~500).
    HighHardnessArmor,
    /// Alumina ceramic composite layered tile (Al2O3 + Kevlar/Steel backing).
    AluminaCeramicComposite,
    /// Titanium alloy armor (Ti-6Al-4V).
    TitaniumArmor,
}

/// Terminal ballistics simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalBallisticsProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Projectile penetrator type.
    #[serde(default)]
    pub projectile_type: ProjectileType,
    /// Target armor material configuration.
    #[serde(default)]
    pub armor_type: TargetArmorType,
    /// Projectile total mass in kilograms (kg).
    pub projectile_mass_kg: f64,
    /// Projectile outer shank diameter in millimeters (mm).
    pub projectile_diameter_mm: f64,
    /// Projectile active core length in millimeters (mm).
    pub projectile_length_mm: f64,
    /// Projectile muzzle / impact velocity in meters per second (m/s).
    pub impact_velocity_m_s: f64,
    /// Impact NATO obliquity angle from armor plate normal in degrees (°).
    pub obliquity_angle_deg: f64,
    /// Target armor physical plate thickness in millimeters (mm).
    pub armor_thickness_mm: f64,
}

impl Default for TerminalBallisticsProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            projectile_type: ProjectileType::TungstenLongRod,
            armor_type: TargetArmorType::RolledHomogeneousArmor,
            projectile_mass_kg: 0.180,
            projectile_diameter_mm: 12.0,
            projectile_length_mm: 90.0,
            impact_velocity_m_s: 1450.0,
            obliquity_angle_deg: 30.0,
            armor_thickness_mm: 75.0,
        }
    }
}

/// Terminal ballistics simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TerminalBallisticsResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Normal component of impact velocity v_n = v0 * cos(theta) in meters per second (m/s).
    pub normal_impact_velocity_m_s: f64,
    /// Effective line-of-sight (LOS) target armor thickness along shotline in millimeters (mm).
    pub line_of_sight_thickness_mm: f64,
    /// Projectile material mass density in kilograms per cubic meter (kg/m³).
    pub projectile_density_kg_m3: f64,
    /// Target armor material mass density in kilograms per cubic meter (kg/m³).
    pub target_density_kg_m3: f64,
    /// Hydrodynamic density square-root contrast ratio gamma = sqrt(rho_p / rho_t).
    pub hydrodynamic_density_ratio: f64,
    /// Dynamic flow yield strength Y_p of projectile penetrator in MegaPascals (MPa).
    pub projectile_yield_strength_mpa: f64,
    /// Dynamic cavity expansion penetration resistance R_t of target armor in MegaPascals (MPa).
    pub target_penetration_resistance_mpa: f64,
    /// Tate-Alekseevskii hydrodynamic penetration interface velocity u in meters per second (m/s).
    pub penetration_interface_velocity_m_s: f64,
    /// Total hydrodynamic penetration depth into semi-infinite armor in millimeters (mm).
    pub total_penetration_depth_mm: f64,
    /// Ballistic limit velocity V50 required to perforate target in meters per second (m/s).
    pub ballistic_limit_v50_m_s: f64,
    /// Residual projectile velocity after perforating armor plate (0 if defeated) in m/s.
    pub residual_velocity_m_s: f64,
    /// Armor protection margin factor (SF = T_los / P_depth).
    pub armor_protection_margin_factor: f64,
    /// NATO STANAG 4569 kinetic protection level (Level 1 to 5) defeated by armor.
    pub stanag_4569_defeated_level: u32,
    /// Whether target armor successfully defeats projectile without perforation (SF >= 1.05).
    pub is_armor_defeating_projectile: bool,
    /// Whether impact is in hydrodynamic regime (normal velocity >= 1000 m/s).
    pub is_hydrodynamic_regime: bool,
}

/// Solves Tate-Alekseevskii hydrodynamic long-rod penetration, Recht-Ipson perforation, and armor protection margin.
pub fn solve_terminal_ballistics(
    problem: &TerminalBallisticsProblem,
) -> Result<TerminalBallisticsResult, String> {
    if problem.projectile_mass_kg <= 0.001 || problem.projectile_diameter_mm <= 1.0 || problem.projectile_length_mm <= 5.0 {
        return Err("Projectile mass and dimensions must be positive realistic values".to_string());
    }
    if problem.impact_velocity_m_s <= 100.0 || problem.armor_thickness_mm <= 1.0 {
        return Err("Impact velocity and armor thickness must be positive".to_string());
    }
    if problem.obliquity_angle_deg < 0.0 || problem.obliquity_angle_deg >= 85.0 {
        return Err("Obliquity angle must be between 0 and 85 degrees".to_string());
    }

    // Material properties (Rosenberg & Dekel)
    let (rho_p, y_p_mpa): (f64, f64) = match problem.projectile_type {
        ProjectileType::TungstenLongRod => (17600.0, 1850.0),    // WHA: rho=17.6 g/cm³, Yp=1.85 GPa
        ProjectileType::DepletedUraniumRod => (18600.0, 1950.0), // DU: rho=18.6 g/cm³, Yp=1.95 GPa
        ProjectileType::HardenedSteelCore => (7850.0, 1400.0),   // Tool steel: rho=7.85 g/cm³, Yp=1.40 GPa
    };

    let (rho_t, r_t_mpa): (f64, f64) = match problem.armor_type {
        TargetArmorType::RolledHomogeneousArmor => (7850.0, 4800.0),     // RHA: Rt ~ 4.5 * sigma_y ~ 4.8 GPa
        TargetArmorType::HighHardnessArmor => (7850.0, 5800.0),          // HHA: Rt ~ 5.8 GPa
        TargetArmorType::AluminaCeramicComposite => (3950.0, 7200.0),    // Al2O3: Rt ~ 7.2 GPa, lower density
        TargetArmorType::TitaniumArmor => (4430.0, 4200.0),              // Ti-6Al-4V: Rt ~ 4.2 GPa
    };

    let theta_rad = problem.obliquity_angle_deg.to_radians();
    let cos_theta = theta_rad.cos();
    let v_n = problem.impact_velocity_m_s * cos_theta;
    let t_los_mm = problem.armor_thickness_mm / cos_theta;

    let gamma = (rho_p / rho_t).sqrt();

    // Tate-Alekseevskii modified Bernoulli hydrodynamic equation:
    // Y_p + 0.5 * rho_p * (v_n - u)^2 = R_t + 0.5 * rho_t * u^2
    // Rearranging into standard quadratic equation for interface velocity u:
    // 0.5 * (rho_p - rho_t) * u^2 - (rho_p * v_n) * u + [0.5 * rho_p * v_n^2 + (Y_p - R_t) * 1e6] = 0
    let y_p_pa = y_p_mpa * 1e6;
    let r_t_pa = r_t_mpa * 1e6;
    let delta_res = y_p_pa - r_t_pa;

    let a_quad = 0.5 * (rho_p - rho_t);
    let b_quad = -rho_p * v_n;
    let c_quad = 0.5 * rho_p * v_n * v_n + delta_res;

    let u_inter = if a_quad.abs() < 1e-3 {
        // Equal density case: u = (0.5 * rho * v_n^2 + Yp - Rt) / (rho * v_n)
        (0.5 * rho_p * v_n * v_n + delta_res) / (rho_p * v_n)
    } else {
        let disc = b_quad * b_quad - 4.0 * a_quad * c_quad;
        if disc >= 0.0 {
            let u1 = (-b_quad - disc.sqrt()) / (2.0 * a_quad);
            let u2 = (-b_quad + disc.sqrt()) / (2.0 * a_quad);
            if (0.0..=v_n).contains(&u1) {
                u1
            } else if (0.0..=v_n).contains(&u2) {
                u2
            } else {
                // Approximate hydrodynamic limit
                (gamma / (1.0 + gamma)) * v_n
            }
        } else {
            (gamma / (1.0 + gamma)) * v_n
        }
    };

    let u_clamped = u_inter.clamp(50.0, v_n * 0.95);

    // Penetration depth into semi-infinite armor:
    // P = L0 * (u / (v_n - u))
    let l0_mm = problem.projectile_length_mm;
    let p_depth_mm = l0_mm * (u_clamped / (v_n - u_clamped));

    // Ballistic limit V50 calculation (Recht & Ipson):
    // Minimum normal velocity where P_depth >= T_los
    // For P = T_los: u / (v50_n - u) = T_los / L0
    let target_ratio = t_los_mm / l0_mm;
    // Hydrodynamic inversion: v50_n ~ sqrt(2 * (Rt - Yp) / rho_p) + ...
    let v50_n = if r_t_pa > y_p_pa {
        let v_rigid_threshold = ((2.0 * (r_t_pa - y_p_pa)) / rho_p).sqrt();
        let v50_est = v_rigid_threshold * (1.0 + target_ratio / gamma).sqrt();
        (v50_est / cos_theta).max(400.0)
    } else {
        (v_n * (target_ratio / (p_depth_mm / l0_mm))).max(300.0)
    };

    // Residual velocity via Recht-Ipson energy conservation:
    // V_res = (V_0^p - V50^p)^(1/p), p ~ 2.0
    let v_res = if problem.impact_velocity_m_s > v50_n {
        (problem.impact_velocity_m_s.powi(2) - v50_n.powi(2)).sqrt()
    } else {
        0.0
    };

    let margin_factor = t_los_mm / p_depth_mm;
    let is_defeating = margin_factor >= 1.05; // 5% thickness margin prevents perforation
    let is_hydro = v_n >= 1000.0;

    // STANAG 4569 defeated level:
    // Level 1: 7.62x51 Ball (v ~ 830 m/s)
    // Level 2: 7.62x39 API (v ~ 700 m/s)
    // Level 3: 7.62x51 AP (v ~ 930 m/s)
    // Level 4: 14.5x114 AP (v ~ 910 m/s)
    // Level 5: 25mm APFSDS (v ~ 1400 m/s)
    let stanag_level = if t_los_mm >= 100.0 && is_defeating {
        5
    } else if t_los_mm >= 60.0 && is_defeating {
        4
    } else if t_los_mm >= 30.0 && is_defeating {
        3
    } else if t_los_mm >= 15.0 && is_defeating {
        2
    } else if is_defeating {
        1
    } else {
        0
    };

    Ok(TerminalBallisticsResult {
        problem_id: problem.problem_id.clone(),
        normal_impact_velocity_m_s: v_n,
        line_of_sight_thickness_mm: t_los_mm,
        projectile_density_kg_m3: rho_p,
        target_density_kg_m3: rho_t,
        hydrodynamic_density_ratio: gamma,
        projectile_yield_strength_mpa: y_p_mpa,
        target_penetration_resistance_mpa: r_t_mpa,
        penetration_interface_velocity_m_s: u_clamped,
        total_penetration_depth_mm: p_depth_mm,
        ballistic_limit_v50_m_s: v50_n,
        residual_velocity_m_s: v_res,
        armor_protection_margin_factor: margin_factor,
        stanag_4569_defeated_level: stanag_level,
        is_armor_defeating_projectile: is_defeating,
        is_hydrodynamic_regime: is_hydro,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tungsten_rod_rha_penetration() -> Result<(), String> {
        let prob = TerminalBallisticsProblem {
            problem_id: "apfsds_rha_test".to_string(),
            projectile_type: ProjectileType::TungstenLongRod,
            armor_type: TargetArmorType::RolledHomogeneousArmor,
            projectile_mass_kg: 0.180,
            projectile_diameter_mm: 12.0,
            projectile_length_mm: 90.0,
            impact_velocity_m_s: 1450.0,
            obliquity_angle_deg: 30.0,
            armor_thickness_mm: 75.0,
        };

        let res = solve_terminal_ballistics(&prob)?;
        assert!(res.normal_impact_velocity_m_s > 1200.0);
        assert!(res.line_of_sight_thickness_mm > 80.0 && res.line_of_sight_thickness_mm < 90.0);
        assert!(res.hydrodynamic_density_ratio > 1.40);
        assert!(res.penetration_interface_velocity_m_s > 500.0 && res.penetration_interface_velocity_m_s < 1000.0);
        assert!(res.total_penetration_depth_mm > 50.0 && res.total_penetration_depth_mm < 95.0);
        assert!(res.ballistic_limit_v50_m_s > 800.0);
        assert!(res.armor_protection_margin_factor > 1.05);
        assert!(res.is_armor_defeating_projectile);
        assert!(res.is_hydrodynamic_regime);
        assert!(res.stanag_4569_defeated_level >= 4);
        Ok(())
    }
}

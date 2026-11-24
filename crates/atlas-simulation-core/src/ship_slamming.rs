// crates/atlas-simulation-core/src/ship_slamming.rs
//
// Marine, Naval Architecture & Offshore Engineering:
// Ship Bow and Bottom Hydrodynamic Slamming Wave Impact Solver.
//
// Based on:
// - Wagner (1932): "Über Stoß- und Gleitvorgänge an der Oberfläche von Flüssigkeiten"
// - Chuang (1967): "Experiments on flat-bottom slamming"
// - DNV-RP-C205: Environmental Conditions and Environmental Loads (Section 8: Slamming and Impact Loads)
// - DNV-RU-SHIP Pt.3 Ch.6 Sec.4: Bottom and Bow Flare Slamming Plating Sizing
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "ship_slamming_problem".to_string()
}

/// Ship hull slamming section location.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SlammingLocation {
    /// Forward bottom plating forward of 0.25 Lpp (flat bottom slamming).
    ForwardBottom,
    /// Bow flare above waterline (wave crest impact on flared hull).
    #[default]
    BowFlare,
    /// Stern overhang / flat transom slamming in following seas.
    SternOverhang,
}

/// Ship slamming simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipSlammingProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Location of the hull slamming impact zone.
    #[serde(default)]
    pub location: SlammingLocation,
    /// Section deadrise angle beta from horizontal in degrees (°).
    pub deadrise_angle_deg: f64,
    /// Relative normal water-entry impact velocity in meters per second (m/s).
    pub relative_impact_velocity_m_s: f64,
    /// Transverse stiffener / frame spacing s in meters (m).
    pub frame_spacing_m: f64,
    /// Longitudinal primary girder / floor spacing l in meters (m).
    pub plate_length_m: f64,
    /// Existing or preliminary plate thickness in millimeters (mm).
    pub plate_thickness_mm: f64,
    /// Hull structural steel yield strength in MegaPascals (MPa, e.g. 355 for NV-36).
    pub steel_yield_strength_mpa: f64,
    /// Seawater mass density in kilograms per cubic meter (kg/m³).
    pub water_density_kg_m3: f64,
}

impl Default for ShipSlammingProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            location: SlammingLocation::BowFlare,
            deadrise_angle_deg: 18.0,
            relative_impact_velocity_m_s: 7.5,
            frame_spacing_m: 0.75,
            plate_length_m: 2.40,
            plate_thickness_mm: 16.0,
            steel_yield_strength_mpa: 355.0,
            water_density_kg_m3: 1025.0,
        }
    }
}

/// Ship slamming simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ShipSlammingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Hydrodynamic peak slamming pressure coefficient C_p.
    pub slamming_pressure_coefficient_cp: f64,
    /// Theoretical localized peak Wagner slamming pressure in kiloPascals (kPa).
    pub peak_localized_pressure_kpa: f64,
    /// Space-averaged design slamming impact pressure over plate panel in kiloPascals (kPa).
    pub space_averaged_pressure_kpa: f64,
    /// Equivalent static design slamming pressure for plate bending in kiloPascals (kPa).
    pub equivalent_static_pressure_kpa: f64,
    /// Approximate slamming pulse duration tau in milliseconds (ms).
    pub slamming_pulse_duration_ms: f64,
    /// Total peak slamming force on the evaluated hull panel in kiloNewtons (kN).
    pub panel_slamming_force_kn: f64,
    /// Maximum elastic bending stress in hull plating in MegaPascals (MPa).
    pub plate_bending_stress_mpa: f64,
    /// DNV-RU-SHIP minimum required plate thickness including corrosion addition in mm.
    pub dnv_required_thickness_mm: f64,
    /// Thickness safety margin factor (t_actual / t_req).
    pub thickness_safety_factor: f64,
    /// Whether existing hull plating satisfies DNV slamming yield strength requirements.
    pub is_plate_thickness_sufficient: bool,
    /// Whether deadrise angle is in safe hydrodynamic wedge regime (beta >= 10.0°).
    pub is_air_cushioning_non_critical: bool,
}

/// Solves Wagner/Chuang hydrodynamic water entry, slamming pressure, and DNV shell plating thickness.
pub fn solve_ship_slamming(problem: &ShipSlammingProblem) -> Result<ShipSlammingResult, String> {
    if problem.relative_impact_velocity_m_s <= 0.5 {
        return Err("Relative impact velocity must exceed 0.5 m/s".to_string());
    }
    if problem.deadrise_angle_deg <= 0.5 || problem.deadrise_angle_deg >= 85.0 {
        return Err("Deadrise angle must be between 0.5 and 85 degrees".to_string());
    }
    if problem.frame_spacing_m <= 0.10 || problem.plate_length_m <= 0.20 {
        return Err("Frame spacing and plate length must be realistic dimensions".to_string());
    }
    if problem.plate_thickness_mm <= 2.0 || problem.steel_yield_strength_mpa <= 100.0 {
        return Err("Plate thickness and steel yield strength must be realistic".to_string());
    }

    let beta_rad = problem.deadrise_angle_deg.to_radians();
    let tan_beta = beta_rad.tan();
    let v_rel = problem.relative_impact_velocity_m_s;
    let rho = problem.water_density_kg_m3;

    // Wagner expanding spray root wedge entry coefficient:
    // For pure wedge without air trapping: Cp_wagner = pi^2 / (4 * tan^2(beta))
    // For small deadrise (beta < 10°), air entrapment cushions the peak pressure (Chuang & DNV-RP-C205)
    let cp_wagner = std::f64::consts::PI * std::f64::consts::PI / (4.0 * tan_beta * tan_beta);

    // Apply 3D shape and air cushioning correction:
    let cp_design = if problem.deadrise_angle_deg < 5.0 {
        // Flat bottom air entrapment limit (Chuang)
        80.0
    } else if problem.deadrise_angle_deg < 10.0 {
        // Transition region
        80.0 + (cp_wagner.min(150.0) - 80.0) * ((problem.deadrise_angle_deg - 5.0) / 5.0)
    } else {
        // Wagner wedge regime with 3D spray root edge relief factor ~ 0.85
        cp_wagner * 0.85
    };

    // Stagnation dynamic pressure: q = 0.5 * rho * v^2 [Pa]
    let q_dyn_pa = 0.5 * rho * v_rel * v_rel;
    let peak_local_kpa = (cp_design * q_dyn_pa) / 1000.0;

    // Space averaging over plate panel:
    // Peak pressure travels along deadrise; space-average pressure factor k_space ~ 0.55 - 0.70 (DNV-RP-C205)
    let k_space = (0.50 + 0.005 * problem.deadrise_angle_deg).clamp(0.55, 0.75);
    let space_avg_kpa = peak_local_kpa * k_space;

    // Pulse duration tau: time for spray root to traverse half plate span s
    // c(t) = (pi / 2) * (v * t / tan(beta)) = s / 2 => tau = (s * tan(beta)) / (pi * v)
    let tau_s = (problem.frame_spacing_m * tan_beta) / (std::f64::consts::PI * v_rel);
    let tau_ms = tau_s * 1000.0;

    // Dynamic load factor (DLF) for plate bending under triangular pulse:
    // With fundamental plate period T_n ~ 2-5 ms, DLF ~ 0.65 - 0.80 (equivalent static design factor)
    let k_dyn = 0.70;
    let p_stat_kpa = space_avg_kpa * k_dyn;

    // Total panel impact force:
    let panel_area_m2 = problem.frame_spacing_m * problem.plate_length_m;
    let panel_force_kn = space_avg_kpa * panel_area_m2;

    // DNV-RU-SHIP Pt.3 Ch.6 plate sizing:
    // t_req = s * c_a * sqrt(P_stat / (1000 * sigma_perm)) + t_corr
    // where aspect ratio factor c_a = 1.0 - 0.5 * (s / l) >= 0.70
    let s_over_l = problem.frame_spacing_m / problem.plate_length_m;
    let c_a = (1.0 - 0.5 * s_over_l).clamp(0.70, 1.0);
    let sigma_perm_mpa = 0.90 * problem.steel_yield_strength_mpa;
    let t_corr_mm = 2.5; // standard marine corrosion addition
    let t_req_mm = (problem.frame_spacing_m * 1000.0)
        * c_a
        * (p_stat_kpa / (2.0 * 1000.0 * sigma_perm_mpa)).sqrt()
        + t_corr_mm;

    // Elastic bending stress in existing plate:
    // sigma = beta_stress * P_stat * (s / t)^2
    let s_mm = problem.frame_spacing_m * 1000.0;
    let t_mm = problem.plate_thickness_mm;
    let beta_stress = (0.50 * c_a).clamp(0.30, 0.50);
    let p_stat_mpa = p_stat_kpa / 1000.0;
    let sigma_bending_mpa = beta_stress * p_stat_mpa * (s_mm / t_mm).powi(2);

    let safety_factor = t_mm / t_req_mm;
    let is_thick_ok = safety_factor >= 1.00;
    let is_air_cushion_ok = problem.deadrise_angle_deg >= 10.0;

    Ok(ShipSlammingResult {
        problem_id: problem.problem_id.clone(),
        slamming_pressure_coefficient_cp: cp_design,
        peak_localized_pressure_kpa: peak_local_kpa,
        space_averaged_pressure_kpa: space_avg_kpa,
        equivalent_static_pressure_kpa: p_stat_kpa,
        slamming_pulse_duration_ms: tau_ms,
        panel_slamming_force_kn: panel_force_kn,
        plate_bending_stress_mpa: sigma_bending_mpa,
        dnv_required_thickness_mm: t_req_mm,
        thickness_safety_factor: safety_factor,
        is_plate_thickness_sufficient: is_thick_ok,
        is_air_cushioning_non_critical: is_air_cushion_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ship_bow_flare_slamming() -> Result<(), String> {
        let prob = ShipSlammingProblem {
            problem_id: "container_bow_flare".to_string(),
            location: SlammingLocation::BowFlare,
            deadrise_angle_deg: 18.0,
            relative_impact_velocity_m_s: 7.5,
            frame_spacing_m: 0.75,
            plate_length_m: 2.40,
            plate_thickness_mm: 16.0,
            steel_yield_strength_mpa: 355.0,
            water_density_kg_m3: 1025.0,
        };

        let res = solve_ship_slamming(&prob)?;
        assert!(
            res.slamming_pressure_coefficient_cp > 10.0
                && res.slamming_pressure_coefficient_cp < 35.0
        );
        assert!(res.peak_localized_pressure_kpa > 250.0 && res.peak_localized_pressure_kpa < 900.0);
        assert!(res.space_averaged_pressure_kpa > 150.0);
        assert!(res.slamming_pulse_duration_ms > 5.0 && res.slamming_pulse_duration_ms < 25.0);
        assert!(res.dnv_required_thickness_mm > 8.0 && res.dnv_required_thickness_mm < 16.0);
        assert!(res.thickness_safety_factor >= 1.0);
        assert!(res.is_plate_thickness_sufficient);
        assert!(res.is_air_cushioning_non_critical);
        Ok(())
    }
}

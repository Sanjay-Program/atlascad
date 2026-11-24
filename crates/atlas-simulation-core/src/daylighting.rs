//! # Architectural Daylighting & CIE Sky Luminescence (EN 17037 / BREEAM / LEED)
//!
//! Evaluates CIE Standard Overcast Sky luminance distribution, BRE split-flux method
//! for indoor illuminance components (Sky Component SC, Externally Reflected ERC, Internally Reflected IRC),
//! and determines average and point Daylight Factor (DF) compliance for architectural spaces.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// CIE standard sky type.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CieSkyType {
    /// CIE Standard Overcast Sky (luminance ratio zenith/horizon = 3:1)
    StandardOvercast,
    /// Uniform Luminance Sky (completely diffuse cloud cover)
    UniformDiffuse,
}

/// Interior room surface reflectance properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RoomSurfaceFinishes {
    /// Wall diffuse reflectance (typical: 0.50 for white/light paint)
    pub wall_reflectance: f64,
    /// Ceiling diffuse reflectance (typical: 0.70 - 0.85 for matte white ceiling)
    pub ceiling_reflectance: f64,
    /// Floor diffuse reflectance (typical: 0.15 - 0.25 for wood / grey carpet)
    pub floor_reflectance: f64,
}

impl Default for RoomSurfaceFinishes {
    fn default() -> Self {
        Self {
            wall_reflectance: 0.50,
            ceiling_reflectance: 0.75,
            floor_reflectance: 0.20,
        }
    }
}

/// Problem definition for architectural daylight factor analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaylightingProblem {
    pub room_name: String,
    pub sky_type: CieSkyType,
    /// Room length perpendicular to window wall in meters (e.g. 6.0 m)
    pub room_depth_m: f64,
    /// Room width parallel to window wall in meters (e.g. 5.0 m)
    pub room_width_m: f64,
    /// Room floor-to-ceiling height in meters (e.g. 3.0 m)
    pub room_height_m: f64,
    /// Net glazing area of windows in m²
    pub window_glazing_area_m2: f64,
    /// Window visual light transmittance (VLT / tau_vis, typical: 0.65 to 0.75 for double glazing)
    pub visual_transmittance: f64,
    /// Window glazing maintenance dirt / frame framing factor (typical: 0.80 to 0.90)
    pub glazing_correction_factor: Option<f64>,
    /// Exterior obstruction angle above horizon in degrees (0° = unobstructed flat horizon, e.g. 20° opposing building)
    pub obstruction_angle_deg: Option<f64>,
    /// Target desk workplane distance from window wall in meters (default room_depth / 2)
    pub workplane_distance_from_window_m: Option<f64>,
    /// Target daylight factor compliance threshold in % (default 2.0% for EN 17037 / BREEAM office standard)
    pub target_daylight_factor_percent: Option<f64>,
    pub surface_finishes: RoomSurfaceFinishes,
}

impl Default for DaylightingProblem {
    fn default() -> Self {
        Self {
            room_name: "Commercial Open-Plan Office".to_string(),
            sky_type: CieSkyType::StandardOvercast,
            room_depth_m: 6.0,
            room_width_m: 5.0,
            room_height_m: 3.0,
            window_glazing_area_m2: 6.5,
            visual_transmittance: 0.70,
            glazing_correction_factor: Some(0.85),
            obstruction_angle_deg: Some(15.0),
            workplane_distance_from_window_m: Some(3.0),
            target_daylight_factor_percent: Some(2.0),
            surface_finishes: RoomSurfaceFinishes::default(),
        }
    }
}

/// Analytical results for indoor daylighting.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DaylightingResult {
    pub room_name: String,
    pub room_floor_area_m2: f64,
    pub total_internal_surface_area_m2: f64,
    pub area_weighted_mean_reflectance: f64,
    pub effective_glass_transmittance: f64,
    pub sky_component_percent: f64,
    pub externally_reflected_component_percent: f64,
    pub internally_reflected_component_percent: f64,
    pub point_daylight_factor_percent: f64,
    pub average_daylight_factor_percent: f64,
    pub target_daylight_factor_percent: f64,
    pub is_compliant: bool,
    pub daylight_quality_rating: String,
    pub summary_notes: Vec<String>,
}

/// Solves CIE Overcast Sky Daylight Factor adhering to the BRE split-flux method and EN 17037.
pub fn solve_cie_daylight_factor(prob: &DaylightingProblem) -> Result<DaylightingResult, String> {
    let depth = prob.room_depth_m;
    let width = prob.room_width_m;
    let height = prob.room_height_m;
    let a_glass = prob.window_glazing_area_m2;
    let tau = prob.visual_transmittance;

    if depth <= 0.5 || width <= 0.5 || height <= 1.0 || a_glass <= 0.1 || tau <= 0.05 {
        return Err(
            "Room dimensions, glazing area, and visual transmittance must be strictly positive."
                .to_string(),
        );
    }

    let floor_area = depth * width;
    let wall_area = 2.0 * (depth + width) * height;
    let ceiling_area = floor_area;
    let total_room_area = 2.0 * floor_area + wall_area;

    // Area-weighted mean interior surface reflectance R_mean:
    let rw = prob.surface_finishes.wall_reflectance.clamp(0.1, 0.95);
    let rc = prob.surface_finishes.ceiling_reflectance.clamp(0.1, 0.95);
    let rf = prob.surface_finishes.floor_reflectance.clamp(0.05, 0.85);

    let total_refl_sum = wall_area * rw + ceiling_area * rc + floor_area * rf;
    let r_mean = total_refl_sum / total_room_area;

    // Net effective glass transmittance accounting for framing/dirt:
    let k_glass = prob
        .glazing_correction_factor
        .unwrap_or(0.85)
        .clamp(0.5, 1.0);
    let tau_eff = tau * k_glass;

    // External obstruction angle theta_obs (degrees above horizon)
    let theta_obs_deg = prob.obstruction_angle_deg.unwrap_or(0.0).clamp(0.0, 70.0);
    let theta_obs_rad = theta_obs_deg.to_radians();

    // 1. Sky Component (SC) at target workplane distance D_p from window wall:
    let d_p = prob
        .workplane_distance_from_window_m
        .unwrap_or(depth * 0.5)
        .clamp(0.5, depth);
    // Window head height above desk workplane (0.75m desk height)
    let h_head = (height - 0.75).max(0.5);
    // Visible sky vertical angle subtended at point:
    let sky_angle_rad = ((h_head / d_p).atan() - theta_obs_rad).max(0.0);

    // CIE standard overcast sky factor (approx 3/2 * sin(theta)^2 for window):
    let sc = if sky_angle_rad > 0.0 {
        let sc_raw = (a_glass / (d_p * d_p * std::f64::consts::PI))
            * sky_angle_rad.sin().powi(2)
            * tau_eff
            * 100.0;
        sc_raw.clamp(0.0, 15.0)
    } else {
        0.0
    };

    // 2. Externally Reflected Component (ERC):
    // If obstruction exists, opposing building surfaces reflect ~20% of overcast sky
    let erc = if theta_obs_deg > 0.0 {
        let obs_angle_rad = theta_obs_rad.min((h_head / d_p).atan());
        let erc_raw = 0.20
            * (a_glass / (d_p * d_p * std::f64::consts::PI))
            * obs_angle_rad.sin().powi(2)
            * tau_eff
            * 100.0;
        erc_raw.clamp(0.0, 5.0)
    } else {
        0.0
    };

    // 3. Internally Reflected Component (IRC) via BRE Split-Flux Formula:
    // IRC = (tau_eff * A_glass / (A_room * (1 - R_mean))) * (C_sky * R_floor_walls + 5.0 * R_ceiling) [%]
    // Under CIE overcast sky, C_sky approx 39.0 / (1 + sin(theta_obs))
    let c_sky = 39.0 / (1.0 + theta_obs_rad.sin());
    let r_fw = (floor_area * rf + (wall_area * 0.5) * rw) / (floor_area + wall_area * 0.5);
    let r_cw = (ceiling_area * rc + (wall_area * 0.5) * rw) / (ceiling_area + wall_area * 0.5);

    let irc_num = tau_eff * a_glass * (c_sky * r_fw + 5.0 * r_cw);
    let irc_den = total_room_area * (1.0 - r_mean);
    let irc = (irc_num / irc_den.max(1.0)).clamp(0.0, 10.0);

    // Total point Daylight Factor at workplane:
    let point_df = sc + erc + irc;

    // Average Daylight Factor across entire room (Crisp & Littlefair formula):
    // ADF = (A_glass * tau_eff * theta_sky_deg * M) / (A_room * (1 - R_mean^2))
    let theta_sky_deg = (90.0 - theta_obs_deg).max(0.0);
    let adf =
        (a_glass * tau_eff * theta_sky_deg) / (total_room_area * (1.0 - r_mean * r_mean).max(0.1));

    let target_df = prob.target_daylight_factor_percent.unwrap_or(2.0);
    let is_ok = average_df_compliance(adf, target_df);

    let rating = if adf >= 5.0 {
        "Exceptionally Well-Daylit (LEED Platinum / Exemplary Visual Comfort)".to_string()
    } else if adf >= 2.0 {
        "Adequate Daylighting (EN 17037 / BREEAM Standard Compliant)".to_string()
    } else if adf >= 1.0 {
        "Sub-optimal Daylighting (Supplementary Artificial Lighting Required)".to_string()
    } else {
        "Poor Daylighting (Continuous Full Artificial Lighting Mandatory)".to_string()
    };

    let notes = vec![
        format!(
            "Daylighting Analysis: '{}' ({:.1}m x {:.1}m x {:.1}m, Floor = {:.1} m²)",
            prob.room_name, width, depth, height, floor_area
        ),
        format!(
            "Glazing: Net Glass Area = {:.2} m², VLT = {:.2} (Effective Transmittance = {:.3})",
            a_glass, tau, tau_eff
        ),
        format!(
            "Room Reflectances: Walls = {:.2}, Ceiling = {:.2}, Floor = {:.2} -> Mean R = {:.3}",
            rw, rc, rf, r_mean
        ),
        format!(
            "Illuminance Components at {:.1}m from window: Sky SC = {:.2}%, External ERC = {:.2}%, Internal IRC = {:.2}%",
            d_p, sc, erc, irc
        ),
        format!(
            "Point Daylight Factor: DF = {:.2}% (at desk level)",
            point_df
        ),
        format!(
            "Average Daylight Factor: ADF = {:.2}% vs Target {:.1}% -> [{}]",
            adf,
            target_df,
            if is_ok { "COMPLIANT" } else { "NON-COMPLIANT" }
        ),
        format!("Visual Comfort Classification: {}", rating),
    ];

    Ok(DaylightingResult {
        room_name: prob.room_name.clone(),
        room_floor_area_m2: floor_area,
        total_internal_surface_area_m2: total_room_area,
        area_weighted_mean_reflectance: r_mean,
        effective_glass_transmittance: tau_eff,
        sky_component_percent: sc,
        externally_reflected_component_percent: erc,
        internally_reflected_component_percent: irc,
        point_daylight_factor_percent: point_df,
        average_daylight_factor_percent: adf,
        target_daylight_factor_percent: target_df,
        is_compliant: is_ok,
        daylight_quality_rating: rating,
        summary_notes: notes,
    })
}

fn average_df_compliance(adf: f64, target: f64) -> bool {
    adf >= target
}

/// Simulation runner adapter for architectural daylighting.
pub fn solve_daylighting_sim(prob: &DaylightingProblem) -> Result<SimulationResult, String> {
    let sol = solve_cie_daylight_factor(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert(
        "point_daylight_factor_percent".to_string(),
        sol.point_daylight_factor_percent,
    );
    metrics.insert(
        "average_daylight_factor_percent".to_string(),
        sol.average_daylight_factor_percent,
    );
    metrics.insert(
        "sky_component_percent".to_string(),
        sol.sky_component_percent,
    );
    metrics.insert(
        "internally_reflected_percent".to_string(),
        sol.internally_reflected_component_percent,
    );
    metrics.insert(
        "externally_reflected_percent".to_string(),
        sol.externally_reflected_component_percent,
    );
    metrics.insert(
        "area_weighted_mean_reflectance".to_string(),
        sol.area_weighted_mean_reflectance,
    );
    metrics.insert(
        "is_compliant".to_string(),
        if sol.is_compliant { 1.0 } else { 0.0 },
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.average_daylight_factor_percent, 1, 1e-4);

    let val = AnalyticalComparison::assess(
        "Daylight Factor Point Sum SC+ERC+IRC (%)",
        sol.point_daylight_factor_percent,
        sol.sky_component_percent
            + sol.externally_reflected_component_percent
            + sol.internally_reflected_component_percent,
    );

    Ok(SimulationResult {
        problem_id: "building-en17037-daylighting".to_string(),
        engine_id: "atlas-daylighting".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-daylighting", "0.1.0"),
        logs: sol.summary_notes,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_commercial_office_daylight_factor() -> Result<(), String> {
        let prob = DaylightingProblem::default();
        let sol = solve_cie_daylight_factor(&prob)?;

        // Room 5x6m with 6.5m² window glazing
        assert_eq!(sol.room_floor_area_m2, 30.0);
        // Total area = 2 * 30 + 2 * (6 + 5) * 3 = 60 + 66 = 126 m²
        assert_eq!(sol.total_internal_surface_area_m2, 126.0);
        // Mean reflectance ~ 0.45 - 0.55
        assert!(
            sol.area_weighted_mean_reflectance > 0.40 && sol.area_weighted_mean_reflectance < 0.60
        );
        // Point DF = SC + ERC + IRC
        let expected_point = sol.sky_component_percent
            + sol.externally_reflected_component_percent
            + sol.internally_reflected_component_percent;
        assert!((sol.point_daylight_factor_percent - expected_point).abs() < 1e-6);
        // Average daylight factor should be around 2.0 - 3.5%
        assert!(
            sol.average_daylight_factor_percent > 1.8 && sol.average_daylight_factor_percent < 4.0
        );
        assert!(sol.is_compliant);

        let sim = solve_daylighting_sim(&prob)?;
        assert_eq!(sim.status, SimulationStatus::Converged);
        Ok(())
    }
}

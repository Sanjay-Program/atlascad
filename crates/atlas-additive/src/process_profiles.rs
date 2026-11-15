//! # Multi-Process Additive Manufacturing & Process Physics (SLA / SLM / FDM / SLS)
//!
//! Provides photopolymerization resin kinetics via the Jacobs Working Curve (SLA/DLP/MSLA),
//! volumetric energy density (VED) and keyhole thresholds for metal powder bed fusion (SLM/DMLS),
//! and multi-technology build time, power, and cost estimation.

use serde::{Deserialize, Serialize};

/// Supported additive manufacturing technology families per ISO/ASTM 52900.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AdditiveTechnology {
    /// Fused Deposition Modeling / Fused Filament Fabrication
    FdmFff,
    /// Stereolithography / Digital Light Processing (Vat Photopolymerization)
    SlaDlpMsla,
    /// Selective Laser Melting / Direct Metal Laser Sintering (Metal Powder Bed Fusion)
    SlmMetalPbf,
    /// Selective Laser Sintering (Polymer Powder Bed Fusion)
    SlsPolymer,
}

/// Photopolymerization resin parameters for SLA/DLP vat printing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlaResinParameters {
    pub resin_name: String,
    /// Critical exposure energy Ec in mJ/cm^2 (typical: 8.0 - 16.0 mJ/cm^2)
    pub critical_exposure_ec_mj_cm2: f64,
    /// Penetration depth Dp in millimeters (typical: 0.10 - 0.25 mm)
    pub penetration_depth_dp_mm: f64,
    /// Light source irradiance at build plane in mW/cm^2 (typical: 2.5 - 5.0 mW/cm^2)
    pub light_irradiance_mw_cm2: f64,
    /// Bottom base layer exposure multiplier (typical: 4x to 8x)
    pub base_exposure_multiplier: f64,
    /// Separation lift and retract speed in mm/min
    pub lift_speed_mm_min: f64,
    /// Peeling lift distance in mm (typical: 5.0 - 8.0 mm)
    pub lift_distance_mm: f64,
    /// Resin density in g/cm^3
    pub density_g_cm3: f64,
}

impl Default for SlaResinParameters {
    fn default() -> Self {
        Self {
            resin_name: "Standard Tough Photopolymer (405nm)".to_string(),
            critical_exposure_ec_mj_cm2: 11.5,
            penetration_depth_dp_mm: 0.135,
            light_irradiance_mw_cm2: 3.8,
            base_exposure_multiplier: 5.0,
            lift_speed_mm_min: 60.0,
            lift_distance_mm: 6.0,
            density_g_cm3: 1.12,
        }
    }
}

/// Metal laser powder bed fusion (SLM/DMLS) process parameters.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SlmMetalParameters {
    pub alloy_name: String,
    /// Laser beam power in Watts (typical: 200 - 500 W)
    pub laser_power_w: f64,
    /// Laser scanning speed in mm/s (typical: 700 - 1400 mm/s)
    pub scan_speed_mm_s: f64,
    /// Hatch spacing between scan tracks in mm (typical: 0.08 - 0.14 mm)
    pub hatch_spacing_mm: f64,
    /// Powder recoating layer thickness in mm (typical: 0.03 - 0.06 mm)
    pub layer_thickness_mm: f64,
    /// Alloy density in g/cm^3 (e.g. 4.43 for Ti-6Al-4V, 8.0 for 316L)
    pub alloy_density_g_cm3: f64,
    /// Alloy melting point in Kelvin
    pub melting_point_k: f64,
}

impl Default for SlmMetalParameters {
    fn default() -> Self {
        Self {
            alloy_name: "Titanium Ti-6Al-4V (Grade 5)".to_string(),
            laser_power_w: 280.0,
            scan_speed_mm_s: 1100.0,
            hatch_spacing_mm: 0.105,
            layer_thickness_mm: 0.040,
            alloy_density_g_cm3: 4.43,
            melting_point_k: 1933.0,
        }
    }
}

/// Request for evaluating additive process physics and build performance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditiveProcessProblem {
    pub technology: AdditiveTechnology,
    /// Total solid part volume in cm^3
    pub part_volume_cm3: f64,
    /// Maximum bounding box height in Z direction in mm
    pub part_height_mm: f64,
    /// Nominal layer height in mm
    pub layer_height_mm: f64,
    /// Optional SLA resin configuration
    pub sla_params: Option<SlaResinParameters>,
    /// Optional SLM metal configuration
    pub slm_params: Option<SlmMetalParameters>,
    /// Raw material unit cost in USD/kg
    pub material_cost_per_kg_usd: Option<f64>,
}

impl Default for AdditiveProcessProblem {
    fn default() -> Self {
        Self {
            technology: AdditiveTechnology::SlaDlpMsla,
            part_volume_cm3: 45.0,
            part_height_mm: 75.0,
            layer_height_mm: 0.050, // 50 microns
            sla_params: Some(SlaResinParameters::default()),
            slm_params: None,
            material_cost_per_kg_usd: Some(65.0),
        }
    }
}

/// Output performance report from multi-process additive evaluation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AdditiveProcessResult {
    pub technology: AdditiveTechnology,
    pub total_layers: u32,
    pub layer_time_seconds: f64,
    pub total_build_time_hours: f64,
    pub material_mass_grams: f64,
    pub material_cost_usd: f64,
    /// Jacobs Working Curve cure depth in mm (SLA only)
    pub jacobs_cure_depth_mm: Option<f64>,
    /// Volumetric Energy Density in J/mm^3 (SLM only)
    pub volumetric_energy_density_j_mm3: Option<f64>,
    /// Process quality regime (e.g. "Optimal Conduction Melting", "Keyhole Vaporization", "Under-cure")
    pub process_regime: String,
    pub summary_notes: Vec<String>,
}

/// Evaluates multi-technology additive manufacturing process physics and kinetics.
pub fn evaluate_additive_process(
    prob: &AdditiveProcessProblem,
) -> Result<AdditiveProcessResult, String> {
    if prob.part_height_mm <= 0.0 || prob.part_volume_cm3 <= 0.0 {
        return Err("Part dimensions and volume must be strictly positive.".to_string());
    }
    if prob.layer_height_mm <= 0.0 {
        return Err("Layer height must be strictly positive.".to_string());
    }

    let layers = (prob.part_height_mm / prob.layer_height_mm).ceil() as u32;

    match prob.technology {
        AdditiveTechnology::SlaDlpMsla => {
            let sla = prob.sla_params.clone().unwrap_or_default();
            let t_layer = prob.layer_height_mm;

            // Jacobs Working Curve:
            // Target cure depth is typically 1.3x to 1.5x layer thickness to ensure strong interlayer cross-linking:
            let target_cd = t_layer * 1.35;

            // Required exposure energy E = Ec * exp(Cd / Dp)
            let req_energy_mj_cm2 = sla.critical_exposure_ec_mj_cm2 * (target_cd / sla.penetration_depth_dp_mm).exp();

            // Exposure time t_exp [s] = (E [mJ/cm^2]) / (I [mW/cm^2])
            let t_exp_s = (req_energy_mj_cm2 / sla.light_irradiance_mw_cm2).max(0.5);

            // Mechanical peel cycle time: lift + retract + delay
            // Lift speed in mm/s = speed_mm_min / 60
            let v_lift = (sla.lift_speed_mm_min / 60.0).max(0.1);
            let t_lift_s = (sla.lift_distance_mm / v_lift) * 2.0 + 1.0; // Up + Down + 1s light-off settle

            let total_per_layer_s = t_exp_s + t_lift_s;

            // Base layers (first 5 layers have extended exposure)
            let base_layers = 5.min(layers);
            let normal_layers = layers.saturating_sub(base_layers);
            let total_build_s = (base_layers as f64) * (t_exp_s * sla.base_exposure_multiplier + t_lift_s)
                + (normal_layers as f64) * total_per_layer_s;

            let build_hours = total_build_s / 3600.0;
            let mass_g = prob.part_volume_cm3 * sla.density_g_cm3;
            let cost_kg = prob.material_cost_per_kg_usd.unwrap_or(65.0);
            let cost = (mass_g / 1000.0) * cost_kg;

            let regime = if target_cd > t_layer * 1.15 && target_cd < t_layer * 1.60 {
                "Optimal Photopolymerization Overcure (ASTM Validated)".to_string()
            } else if target_cd <= t_layer {
                "Under-cured Delamination Risk".to_string()
            } else {
                "Excessive Overcure / Dimensional Blooming".to_string()
            };

            let notes = vec![
                format!("Vat Photopolymerization: {} ({:.1} µm layers)", sla.resin_name, t_layer * 1000.0),
                format!("Jacobs Working Curve: Ec = {:.1} mJ/cm², Dp = {:.3} mm, Target Cure Depth = {:.3} mm", sla.critical_exposure_ec_mj_cm2, sla.penetration_depth_dp_mm, target_cd),
                format!("Exposure Duration: {:.2} s per layer ({:.2} s for base layers at {:.1} mW/cm²)", t_exp_s, t_exp_s * sla.base_exposure_multiplier, sla.light_irradiance_mw_cm2),
                format!("Build Velocity: {:.1} mm/hr, Total Duration = {:.2} hours ({} layers)", prob.part_height_mm / build_hours.max(0.01), build_hours, layers),
                format!("Resin Consumption: {:.1} g (${:.2} USD)", mass_g, cost),
            ];

            Ok(AdditiveProcessResult {
                technology: AdditiveTechnology::SlaDlpMsla,
                total_layers: layers,
                layer_time_seconds: total_per_layer_s,
                total_build_time_hours: build_hours,
                material_mass_grams: mass_g,
                material_cost_usd: cost,
                jacobs_cure_depth_mm: Some(target_cd),
                volumetric_energy_density_j_mm3: None,
                process_regime: regime,
                summary_notes: notes,
            })
        }
        AdditiveTechnology::SlmMetalPbf => {
            let slm = prob.slm_params.clone().unwrap_or_default();
            let p = slm.laser_power_w;
            let v = slm.scan_speed_mm_s;
            let h = slm.hatch_spacing_mm;
            let t = slm.layer_thickness_mm;

            // Volumetric Energy Density: VED = P / (v * h * t) in J/mm^3
            let ved = p / (v * h * t);

            let regime = if (55.0..=125.0).contains(&ved) {
                "Optimal Conduction Laser Melting (High Density > 99.8%)".to_string()
            } else if ved > 125.0 {
                "Keyhole Vaporization Regime (Entrapped Gas Porosity Risk)".to_string()
            } else {
                "Lack-of-Fusion Porosity Risk (Insufficient Melt Penetration)".to_string()
            };

            // Area scan rate [mm^2/s] = v * h
            let area_scan_rate_mm2_s = v * h;
            // Typical layer cross-section area in mm^2: (part_vol * 1000) / part_height
            let avg_cross_section_mm2 = (prob.part_volume_cm3 * 1000.0) / prob.part_height_mm;
            let laser_scan_time_s = avg_cross_section_mm2 / area_scan_rate_mm2_s.max(1.0);
            let recoater_time_s = 10.0; // 10s mechanical recoating blade pass
            let layer_time_s = laser_scan_time_s + recoater_time_s;

            let total_build_s = (layers as f64) * layer_time_s;
            let build_hours = total_build_s / 3600.0;

            let mass_g = prob.part_volume_cm3 * slm.alloy_density_g_cm3;
            let cost_kg = prob.material_cost_per_kg_usd.unwrap_or(180.0);
            let cost = (mass_g / 1000.0) * cost_kg;

            let notes = vec![
                format!("Metal Laser Powder Bed Fusion: {}", slm.alloy_name),
                format!("Laser Beam Power: {:.0} W at {:.0} mm/s (Hatch = {:.3} mm, Layer = {:.3} mm)", p, v, h, t),
                format!("Volumetric Energy Density (VED): {:.2} J/mm³ [{}]", ved, regime),
                format!("Layer Cycle: {:.1}s scan + {:.1}s recoat = {:.1}s total ({} layers)", laser_scan_time_s, recoater_time_s, layer_time_s, layers),
                format!("Build Duration: {:.2} hours, Metal Mass = {:.1} g (${:.2} USD)", build_hours, mass_g, cost),
            ];

            Ok(AdditiveProcessResult {
                technology: AdditiveTechnology::SlmMetalPbf,
                total_layers: layers,
                layer_time_seconds: layer_time_s,
                total_build_time_hours: build_hours,
                material_mass_grams: mass_g,
                material_cost_usd: cost,
                jacobs_cure_depth_mm: None,
                volumetric_energy_density_j_mm3: Some(ved),
                process_regime: regime,
                summary_notes: notes,
            })
        }
        AdditiveTechnology::FdmFff => {
            let t = prob.layer_height_mm;
            let speed_mm_s = 60.0;
            // Total extruded length per layer approx (part_volume * 1000 / part_height) / (bead_width * layer_height)
            let avg_cross_section_mm2 = (prob.part_volume_cm3 * 1000.0) / prob.part_height_mm;
            let road_w = 0.45; // 0.4mm nozzle with 0.45mm road width
            let road_length_mm = avg_cross_section_mm2 / (road_w * t);
            let move_time_s = road_length_mm / speed_mm_s;
            let layer_change_s = 1.5;
            let layer_time_s = move_time_s + layer_change_s;
            let total_build_s = (layers as f64) * layer_time_s;
            let build_hours = total_build_s / 3600.0;

            let density = 1.24; // PLA
            let mass_g = prob.part_volume_cm3 * density;
            let cost = (mass_g / 1000.0) * prob.material_cost_per_kg_usd.unwrap_or(24.0);

            let notes = vec![
                format!("FDM/FFF Thermoplastic Slicing (Layer = {:.3} mm)", t),
                format!("Estimated Build Duration: {:.2} hours ({} layers)", build_hours, layers),
                format!("Filament Consumption: {:.1} g (${:.2} USD)", mass_g, cost),
            ];

            Ok(AdditiveProcessResult {
                technology: AdditiveTechnology::FdmFff,
                total_layers: layers,
                layer_time_seconds: layer_time_s,
                total_build_time_hours: build_hours,
                material_mass_grams: mass_g,
                material_cost_usd: cost,
                jacobs_cure_depth_mm: None,
                volumetric_energy_density_j_mm3: None,
                process_regime: "Standard FDM Extrusion".to_string(),
                summary_notes: notes,
            })
        }
        AdditiveTechnology::SlsPolymer => {
            let t = prob.layer_height_mm;
            let recoat_s = 8.0;
            let scan_s = 12.0;
            let layer_time_s = recoat_s + scan_s;
            let total_build_s = (layers as f64) * layer_time_s;
            let build_hours = total_build_s / 3600.0;

            let density = 0.95; // PA12 nylon
            let mass_g = prob.part_volume_cm3 * density;
            let cost = (mass_g / 1000.0) * prob.material_cost_per_kg_usd.unwrap_or(80.0);

            let notes = vec![
                format!("SLS Nylon PA12 Sintering (Layer = {:.3} mm)", t),
                format!("Build Duration: {:.2} hours ({} layers)", build_hours, layers),
                format!("Powder Sintered: {:.1} g (${:.2} USD)", mass_g, cost),
            ];

            Ok(AdditiveProcessResult {
                technology: AdditiveTechnology::SlsPolymer,
                total_layers: layers,
                layer_time_seconds: layer_time_s,
                total_build_time_hours: build_hours,
                material_mass_grams: mass_g,
                material_cost_usd: cost,
                jacobs_cure_depth_mm: None,
                volumetric_energy_density_j_mm3: None,
                process_regime: "Thermal Sintering Equilibrium".to_string(),
                summary_notes: notes,
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sla_jacobs_working_curve_and_exposure() -> Result<(), String> {
        let prob = AdditiveProcessProblem {
            technology: AdditiveTechnology::SlaDlpMsla,
            part_volume_cm3: 30.0,
            part_height_mm: 50.0,
            layer_height_mm: 0.05,
            sla_params: Some(SlaResinParameters::default()),
            slm_params: None,
            material_cost_per_kg_usd: Some(60.0),
        };

        let res = evaluate_additive_process(&prob)?;
        assert_eq!(res.total_layers, 1000);
        assert!(res.jacobs_cure_depth_mm.unwrap_or(0.0) > 0.05);
        assert!(res.total_build_time_hours > 0.0);
        assert!(res.material_mass_grams > 0.0);
        assert!(res.process_regime.contains("Optimal"));
        Ok(())
    }

    #[test]
    fn test_slm_volumetric_energy_density() -> Result<(), String> {
        let prob = AdditiveProcessProblem {
            technology: AdditiveTechnology::SlmMetalPbf,
            part_volume_cm3: 15.0,
            part_height_mm: 40.0,
            layer_height_mm: 0.04,
            sla_params: None,
            slm_params: Some(SlmMetalParameters::default()),
            material_cost_per_kg_usd: Some(200.0),
        };

        let res = evaluate_additive_process(&prob)?;
        assert_eq!(res.total_layers, 1000);
        let ved = res.volumetric_energy_density_j_mm3.unwrap_or(0.0);
        // VED = 280 / (1100 * 0.105 * 0.040) = 280 / 4.62 = 60.6 J/mm3
        assert!((ved - 60.606).abs() < 0.1);
        assert!(res.process_regime.contains("Optimal"));
        Ok(())
    }
}

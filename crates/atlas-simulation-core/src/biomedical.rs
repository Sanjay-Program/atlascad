//! Biomedical engineering, hemodynamics, and biomechanical stress solver.

use serde::{Deserialize, Serialize};

/// Request parameters for arterial pulse wave and hemodynamic analysis.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HemodynamicsRequest {
    /// Young's modulus of arterial wall in Pa (typically 0.4e6 to 1.6e6 Pa).
    #[serde(default = "default_elastic_modulus")]
    pub arterial_elastic_modulus_pa: f64,
    /// Arterial wall thickness in meters.
    #[serde(default = "default_wall_thickness")]
    pub wall_thickness_m: f64,
    /// Internal lumen radius in meters.
    #[serde(default = "default_lumen_radius")]
    pub lumen_radius_m: f64,
    /// Blood density in kg/m³ (standard 1055.0 kg/m³).
    #[serde(default = "default_blood_density")]
    pub blood_density_kg_m3: f64,
    /// Heart rate in beats per minute (e.g. 72.0).
    #[serde(default = "default_heart_rate")]
    pub heart_rate_bpm: f64,
    /// Dynamic blood viscosity in Pa·s (standard 0.0035 Pa·s).
    #[serde(default = "default_blood_viscosity")]
    pub blood_viscosity_pa_s: f64,
}

fn default_elastic_modulus() -> f64 {
    800_000.0
}
fn default_wall_thickness() -> f64 {
    0.001
}
fn default_lumen_radius() -> f64 {
    0.012
}
fn default_blood_density() -> f64 {
    1055.0
}
fn default_heart_rate() -> f64 {
    72.0
}
fn default_blood_viscosity() -> f64 {
    0.0035
}

impl Default for HemodynamicsRequest {
    fn default() -> Self {
        Self {
            arterial_elastic_modulus_pa: default_elastic_modulus(),
            wall_thickness_m: default_wall_thickness(),
            lumen_radius_m: default_lumen_radius(),
            blood_density_kg_m3: default_blood_density(),
            heart_rate_bpm: default_heart_rate(),
            blood_viscosity_pa_s: default_blood_viscosity(),
        }
    }
}

/// Analytical results of arterial hemodynamics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HemodynamicsResult {
    /// Moens-Korteweg Pulse Wave Velocity (PWV) in m/s.
    pub pulse_wave_velocity_m_s: f64,
    /// Womersley dimensionless frequency parameter (alpha).
    pub womersley_number: f64,
    /// Hydrodynamic flow regime description.
    pub flow_regime: String,
    /// Wave transit time over a 0.5m thoracic-abdominal aortic path, in milliseconds.
    pub wave_transit_time_ms: f64,
    /// Analytical validity verification.
    pub is_physiologically_valid: bool,
}

/// Computes arterial pulse wave velocity and oscillatory flow characteristics.
pub fn solve_hemodynamics(req: &HemodynamicsRequest) -> Result<HemodynamicsResult, String> {
    if req.arterial_elastic_modulus_pa <= 0.0
        || req.wall_thickness_m <= 0.0
        || req.lumen_radius_m <= 0.0
        || req.blood_density_kg_m3 <= 0.0
        || req.heart_rate_bpm <= 0.0
        || req.blood_viscosity_pa_s <= 0.0
    {
        return Err("All hemodynamic parameters must be strictly positive".to_string());
    }

    // Moens-Korteweg equation: c0 = sqrt( (E * h) / (2 * R * rho) )
    let numerator = req.arterial_elastic_modulus_pa * req.wall_thickness_m;
    let denominator = 2.0 * req.lumen_radius_m * req.blood_density_kg_m3;
    let pwv = (numerator / denominator).sqrt();

    // Angular heart frequency: omega = 2 * pi * (BPM / 60)
    let omega = 2.0 * std::f64::consts::PI * (req.heart_rate_bpm / 60.0);

    // Womersley parameter: alpha = R * sqrt( (omega * rho) / mu )
    let womersley =
        req.lumen_radius_m * ((omega * req.blood_density_kg_m3) / req.blood_viscosity_pa_s).sqrt();

    let regime = if womersley > 10.0 {
        "High Womersley: Oscillatory Inertial Plug Flow (Aortic root core)".to_string()
    } else if womersley > 3.0 {
        "Intermediate Womersley: Combined Viscous & Inertial Pulsatile Flow".to_string()
    } else {
        "Low Womersley: Quasi-Steady Poiseuille Viscous Flow (Small arterioles)".to_string()
    };

    let path_length_m = 0.5;
    let transit_time_ms = (path_length_m / pwv) * 1000.0;
    let is_valid = (3.0..=25.0).contains(&pwv); // Typical human physiological range

    Ok(HemodynamicsResult {
        pulse_wave_velocity_m_s: (pwv * 100.0).round() / 100.0,
        womersley_number: (womersley * 100.0).round() / 100.0,
        flow_regime: regime,
        wave_transit_time_ms: (transit_time_ms * 10.0).round() / 10.0,
        is_physiologically_valid: is_valid,
    })
}

/// Request parameters for orthopedic cortical bone biomechanics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrthopedicBoneRequest {
    /// Compressive axial force in Newtons (e.g. 2500 N during gait).
    #[serde(default = "default_axial_force")]
    pub axial_force_n: f64,
    /// Bending moment in N·m (e.g. 45 N·m).
    #[serde(default = "default_bending_moment")]
    pub bending_moment_nm: f64,
    /// Outer cortical bone radius in meters (e.g. 0.015 m).
    #[serde(default = "default_outer_radius")]
    pub outer_radius_m: f64,
    /// Inner medullary canal radius in meters (e.g. 0.008 m).
    #[serde(default = "default_inner_radius")]
    pub inner_radius_m: f64,
    /// Ultimate compressive strength of cortical bone in MPa (standard 167.0 MPa).
    #[serde(default = "default_bone_strength")]
    pub ultimate_compressive_strength_mpa: f64,
}

fn default_axial_force() -> f64 {
    2500.0
}
fn default_bending_moment() -> f64 {
    45.0
}
fn default_outer_radius() -> f64 {
    0.015
}
fn default_inner_radius() -> f64 {
    0.008
}
fn default_bone_strength() -> f64 {
    167.0
}

impl Default for OrthopedicBoneRequest {
    fn default() -> Self {
        Self {
            axial_force_n: default_axial_force(),
            bending_moment_nm: default_bending_moment(),
            outer_radius_m: default_outer_radius(),
            inner_radius_m: default_inner_radius(),
            ultimate_compressive_strength_mpa: default_bone_strength(),
        }
    }
}

/// Biomechanical stress evaluation result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct OrthopedicBoneResult {
    /// Cross-sectional area of cortical bone in mm².
    pub cross_sectional_area_mm2: f64,
    /// Second moment of area in mm⁴.
    pub second_moment_of_area_mm4: f64,
    /// Axial compressive stress in MPa.
    pub axial_stress_mpa: f64,
    /// Peak outer-fiber bending stress in MPa.
    pub bending_stress_mpa: f64,
    /// Combined peak compressive stress in MPa.
    pub peak_combined_stress_mpa: f64,
    /// Factor of safety against bone fracture.
    pub factor_of_safety: f64,
    /// Biomechanical assessment status.
    pub status: String,
}

/// Solves combined axial and bending stresses in hollow cylindrical long bones.
pub fn solve_orthopedic_bone(req: &OrthopedicBoneRequest) -> Result<OrthopedicBoneResult, String> {
    if req.outer_radius_m <= req.inner_radius_m || req.inner_radius_m <= 0.0 {
        return Err("Outer radius must strictly exceed inner radius".to_string());
    }
    if req.ultimate_compressive_strength_mpa <= 0.0 {
        return Err("Ultimate compressive strength must be positive".to_string());
    }

    let pi = std::f64::consts::PI;
    let r_o = req.outer_radius_m;
    let r_i = req.inner_radius_m;

    // Cross-sectional area: A = pi * (ro^2 - ri^2)
    let area_m2 = pi * (r_o.powi(2) - r_i.powi(2));
    let area_mm2 = area_m2 * 1.0e6;

    // Second moment of area: I = (pi / 4) * (ro^4 - ri^4)
    let inertia_m4 = (pi / 4.0) * (r_o.powi(4) - r_i.powi(4));
    let inertia_mm4 = inertia_m4 * 1.0e12;

    // Axial stress: sigma_axial = P / A
    let sigma_axial_pa = req.axial_force_n / area_m2;
    let sigma_axial_mpa = sigma_axial_pa / 1.0e6;

    // Bending stress: sigma_bending = (M * c) / I
    let sigma_bending_pa = (req.bending_moment_nm * r_o) / inertia_m4;
    let sigma_bending_mpa = sigma_bending_pa / 1.0e6;

    // Peak combined compressive stress
    let peak_stress_mpa = sigma_axial_mpa + sigma_bending_mpa;
    let fos = req.ultimate_compressive_strength_mpa / peak_stress_mpa;

    let status = if fos >= 2.0 {
        "High Safety: Low Fracture Risk (Physiological Activity)".to_string()
    } else if fos >= 1.2 {
        "Moderate Safety: Demanding Load / Heavy Exercise".to_string()
    } else if fos >= 1.0 {
        "Marginal Safety: Microdamage Accumulation Possible".to_string()
    } else {
        "Failure Risk: Critical Overload Fracture Likely".to_string()
    };

    Ok(OrthopedicBoneResult {
        cross_sectional_area_mm2: (area_mm2 * 10.0).round() / 10.0,
        second_moment_of_area_mm4: (inertia_mm4 * 10.0).round() / 10.0,
        axial_stress_mpa: (sigma_axial_mpa * 100.0).round() / 100.0,
        bending_stress_mpa: (sigma_bending_mpa * 100.0).round() / 100.0,
        peak_combined_stress_mpa: (peak_stress_mpa * 100.0).round() / 100.0,
        factor_of_safety: (fos * 100.0).round() / 100.0,
        status,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pulse_wave_velocity_analytical_match() -> Result<(), String> {
        let req = HemodynamicsRequest {
            arterial_elastic_modulus_pa: 800_000.0,
            wall_thickness_m: 0.001,
            lumen_radius_m: 0.012,
            blood_density_kg_m3: 1055.0,
            heart_rate_bpm: 72.0,
            blood_viscosity_pa_s: 0.0035,
        };
        let res = solve_hemodynamics(&req)?;
        // c0 = sqrt( 800 / (2 * 0.012 * 1055) ) = sqrt( 800 / 25.32 ) = sqrt(31.595) = 5.62 m/s
        assert!((res.pulse_wave_velocity_m_s - 5.62).abs() < 0.1);
        assert!(res.womersley_number > 15.0); // Aortic trunk flow is strongly oscillatory
        assert!(res.is_physiologically_valid);
        Ok(())
    }

    #[test]
    fn test_orthopedic_bone_stress_and_fos() -> Result<(), String> {
        let req = OrthopedicBoneRequest::default();
        let res = solve_orthopedic_bone(&req)?;
        assert!(res.cross_sectional_area_mm2 > 400.0);
        assert!(res.factor_of_safety > 1.5);
        assert!(res.status.contains("Safety"));
        Ok(())
    }
}

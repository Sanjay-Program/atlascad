//! Marine engineering, naval architecture hydrostatics, and ocean wave dynamics.

use serde::{Deserialize, Serialize};

/// Request parameters for ship hydrostatics and intact transverse stability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarineHydrostaticsRequest {
    /// Ship waterline length in meters (L).
    #[serde(default = "default_hull_length")]
    pub hull_length_m: f64,
    /// Ship molded beam in meters (B).
    #[serde(default = "default_hull_beam")]
    pub hull_beam_m: f64,
    /// Ship molded draft in meters (T).
    #[serde(default = "default_hull_draft")]
    pub hull_draft_m: f64,
    /// Block coefficient Cb (typically 0.55 to 0.85).
    #[serde(default = "default_block_coeff")]
    pub block_coefficient: f64,
    /// Vertical center of gravity above keel in meters (KG).
    #[serde(default = "default_kg")]
    pub vertical_center_of_gravity_kg_m: f64,
    /// Water density in kg/m³ (seawater 1025.0 kg/m³, freshwater 1000.0 kg/m³).
    #[serde(default = "default_water_density")]
    pub water_density_kg_m3: f64,
    /// Heel angle in degrees for righting arm calculation (e.g. 5.0°).
    #[serde(default = "default_heel_angle")]
    pub heel_angle_deg: f64,
}

fn default_hull_length() -> f64 {
    50.0
}
fn default_hull_beam() -> f64 {
    10.0
}
fn default_hull_draft() -> f64 {
    3.0
}
fn default_block_coeff() -> f64 {
    0.65
}
fn default_kg() -> f64 {
    2.8
}
fn default_water_density() -> f64 {
    1025.0
}
fn default_heel_angle() -> f64 {
    5.0
}

impl Default for MarineHydrostaticsRequest {
    fn default() -> Self {
        Self {
            hull_length_m: default_hull_length(),
            hull_beam_m: default_hull_beam(),
            hull_draft_m: default_hull_draft(),
            block_coefficient: default_block_coeff(),
            vertical_center_of_gravity_kg_m: default_kg(),
            water_density_kg_m3: default_water_density(),
            heel_angle_deg: default_heel_angle(),
        }
    }
}

/// Naval hydrostatics and transverse intact stability report.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MarineHydrostaticsResult {
    /// Molded displaced volume in m³.
    pub displaced_volume_m3: f64,
    /// Total ship displacement in metric tonnes.
    pub displacement_tonnes: f64,
    /// Vertical center of buoyancy above keel (KB) in meters.
    pub center_of_buoyancy_kb_m: f64,
    /// Transverse metacentric radius (BM) in meters.
    pub transverse_metacentric_radius_bm_m: f64,
    /// Initial transverse metacentric height (GM) in meters.
    pub metacentric_height_gm_m: f64,
    /// Static righting lever (GZ) at the requested heel angle, in meters.
    pub righting_lever_gz_m: f64,
    /// Static righting moment in kN·m.
    pub righting_moment_kn_m: f64,
    /// Intact stability criteria compliance (GM > 0.15m per IMO A.749 code).
    pub is_statically_stable: bool,
}

/// Solves intact hydrostatic equilibrium and metacentric stability.
pub fn solve_hydrostatics(
    req: &MarineHydrostaticsRequest,
) -> Result<MarineHydrostaticsResult, String> {
    if req.hull_length_m <= 0.0 || req.hull_beam_m <= 0.0 || req.hull_draft_m <= 0.0 {
        return Err("Hull dimensions must be strictly positive".to_string());
    }
    if req.block_coefficient <= 0.1 || req.block_coefficient > 1.0 {
        return Err("Block coefficient must be between 0.1 and 1.0".to_string());
    }
    if req.water_density_kg_m3 <= 500.0 {
        return Err("Water density must be realistic (>= 500 kg/m³)".to_string());
    }

    let l = req.hull_length_m;
    let b = req.hull_beam_m;
    let t = req.hull_draft_m;
    let cb = req.block_coefficient;
    let rho = req.water_density_kg_m3;
    let g = 9.80665;

    // Displaced volume: nabla = L * B * T * Cb
    let volume_m3 = l * b * t * cb;
    let mass_kg = volume_m3 * rho;
    let displacement_tonnes = mass_kg / 1000.0;

    // Vertical center of buoyancy KB via Morrish approximation: KB = T * (5/6 - Cb/3)
    let kb = t * (0.833 - (0.333 * cb));

    // Waterplane area coefficient Cwp approximation: Cwp ~ Cb + 0.10
    let cwp = (cb + 0.10).min(0.98);
    // Transverse second moment of waterplane area: IT = (Cwp * L * B^3) / 12
    let i_transverse = (cwp * l * b.powi(3)) / 12.0;

    // Metacentric radius BM = IT / nabla
    let bm = i_transverse / volume_m3;

    // Metacentric height GM = KB + BM - KG
    let gm = kb + bm - req.vertical_center_of_gravity_kg_m;

    // Righting arm GZ(theta) = GM * sin(theta) for small heel angles
    let theta_rad = req.heel_angle_deg.to_radians();
    let gz = gm * theta_rad.sin();

    // Righting moment: MR = mass_kg * g * GZ / 1000 (kN·m)
    let righting_moment_kn_m = (mass_kg * g * gz) / 1000.0;

    // IMO intact stability code: initial GM0 >= 0.15 m
    let stable = gm >= 0.15;

    Ok(MarineHydrostaticsResult {
        displaced_volume_m3: (volume_m3 * 10.0).round() / 10.0,
        displacement_tonnes: (displacement_tonnes * 10.0).round() / 10.0,
        center_of_buoyancy_kb_m: (kb * 100.0).round() / 100.0,
        transverse_metacentric_radius_bm_m: (bm * 100.0).round() / 100.0,
        metacentric_height_gm_m: (gm * 100.0).round() / 100.0,
        righting_lever_gz_m: (gz * 1000.0).round() / 1000.0,
        righting_moment_kn_m: (righting_moment_kn_m * 10.0).round() / 10.0,
        is_statically_stable: stable,
    })
}

/// Request parameters for Airy linear water wave dispersion.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiryWaveRequest {
    /// Wave period in seconds (T, e.g. 8.0 s).
    #[serde(default = "default_wave_period")]
    pub wave_period_s: f64,
    /// Water depth in meters (d, e.g. 30.0 m).
    #[serde(default = "default_water_depth")]
    pub water_depth_m: f64,
    /// Significant wave height in meters (H, e.g. 2.0 m).
    #[serde(default = "default_wave_height")]
    pub wave_height_m: f64,
}

fn default_wave_period() -> f64 {
    8.0
}
fn default_water_depth() -> f64 {
    30.0
}
fn default_wave_height() -> f64 {
    2.0
}

impl Default for AiryWaveRequest {
    fn default() -> Self {
        Self {
            wave_period_s: default_wave_period(),
            water_depth_m: default_water_depth(),
            wave_height_m: default_wave_height(),
        }
    }
}

/// Airy linear surface gravity wave analytical result.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AiryWaveResult {
    /// Angular frequency in rad/s.
    pub angular_frequency_rad_s: f64,
    /// Wavelength in meters (lambda).
    pub wavelength_m: f64,
    /// Wave number in rad/m (k = 2*pi / lambda).
    pub wave_number_rad_m: f64,
    /// Wave phase speed (celerity) in m/s.
    pub wave_celerity_m_s: f64,
    /// Deep water asymptotic wavelength in meters.
    pub deep_water_wavelength_m: f64,
    /// Water depth regime classification.
    pub regime: String,
    /// Peak horizontal orbital particle velocity at the water surface in m/s.
    pub peak_surface_orbital_velocity_m_s: f64,
}

/// Solves Airy linear dispersion relation omega^2 = g * k * tanh(k * d) via Newton-Raphson.
pub fn solve_airy_wave(req: &AiryWaveRequest) -> Result<AiryWaveResult, String> {
    if req.wave_period_s <= 0.0 || req.water_depth_m <= 0.0 || req.wave_height_m <= 0.0 {
        return Err("Wave period, water depth, and wave height must be positive".to_string());
    }

    let g = 9.80665;
    let pi = std::f64::consts::PI;
    let omega = 2.0 * pi / req.wave_period_s;
    let d = req.water_depth_m;
    let h = req.wave_height_m;

    // Deep water wavelength: lambda0 = g * T^2 / (2 * pi)
    let lambda_0 = (g * req.wave_period_s.powi(2)) / (2.0 * pi);

    // Initial guess via Fenton & McKee explicit approximation:
    let y = omega.powi(2) * d / g;
    let mut k = (omega.powi(2) / g) * (1.0 / (y.tanh()).sqrt()).max(0.0001);

    // Newton-Raphson iteration on: f(k) = g * k * tanh(k * d) - omega^2
    for _ in 0..50 {
        let kd = (k * d).clamp(-50.0, 50.0);
        let th = kd.tanh();
        let sech2 = 1.0 - th.powi(2);

        let f = g * k * th - omega.powi(2);
        let df = g * th + g * k * d * sech2;

        if df.abs() < 1.0e-14 {
            break;
        }
        let delta = f / df;
        k -= delta;
        if delta.abs() < 1.0e-9 {
            break;
        }
    }

    let wavelength = 2.0 * pi / k;
    let celerity = wavelength / req.wave_period_s;

    let relative_depth = d / wavelength;
    let regime = if relative_depth > 0.5 {
        "Deep Water Regime (d / L > 0.5)".to_string()
    } else if relative_depth < 0.05 {
        "Shallow Water Regime (d / L < 0.05)".to_string()
    } else {
        "Intermediate Depth Regime (0.05 <= d / L <= 0.5)".to_string()
    };

    // Peak horizontal orbital surface velocity: u_max = (pi * H / T) / tanh(kd)
    let kd_clamped = (k * d).clamp(-50.0, 50.0);
    let u_surf = (pi * h / req.wave_period_s) / kd_clamped.tanh().max(0.01);

    Ok(AiryWaveResult {
        angular_frequency_rad_s: (omega * 1000.0).round() / 1000.0,
        wavelength_m: (wavelength * 100.0).round() / 100.0,
        wave_number_rad_m: (k * 10000.0).round() / 10000.0,
        wave_celerity_m_s: (celerity * 100.0).round() / 100.0,
        deep_water_wavelength_m: (lambda_0 * 100.0).round() / 100.0,
        regime,
        peak_surface_orbital_velocity_m_s: (u_surf * 100.0).round() / 100.0,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hydrostatics_stability_and_gm() -> Result<(), String> {
        let req = MarineHydrostaticsRequest::default();
        let res = solve_hydrostatics(&req)?;
        assert!(res.displaced_volume_m3 > 800.0);
        assert!(res.metacentric_height_gm_m > 0.5); // Statically stable
        assert!(res.is_statically_stable);
        assert!(res.righting_lever_gz_m > 0.0);
        Ok(())
    }

    #[test]
    fn test_airy_wave_dispersion_convergence() -> Result<(), String> {
        let req = AiryWaveRequest {
            wave_period_s: 8.0,
            water_depth_m: 50.0,
            wave_height_m: 2.0,
        };
        let res = solve_airy_wave(&req)?;
        // Deep water wavelength for 8s wave is ~99.9m
        assert!((res.deep_water_wavelength_m - 99.9).abs() < 1.0);
        assert!(res.wavelength_m > 90.0 && res.wavelength_m < 105.0);
        assert!(res.wave_celerity_m_s > 11.0);
        Ok(())
    }
}

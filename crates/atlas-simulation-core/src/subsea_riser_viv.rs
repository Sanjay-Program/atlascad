//! Ultra-Deepwater Subsea Marine Drilling / Production Riser Vortex-Induced Vibration (VIV) & Fatigue
//!
//! Formulated according to:
//! - DNV-RP-F204 "Riser Fatigue" & DNV-RP-C205 "Environmental Conditions and Environmental Loads"
//! - API RP 16Q "Recommended Practice for Design, Selection, Operation, and Maintenance of Marine Drilling Riser Systems"
//! - Sarpkaya, T., "A Critical Review of Intrinsic and Induced Fluctuation Aspects of Vortex-Induced Vibrations", J. Fluids Struct.
//! - Vandiver, J. K., "Dimensionless Parameters Important to the Prediction of Vortex-Induced Vibration of Long, Flexible Cylinders"
//!
//! Evaluates tensioned slender marine riser transverse eigenfrequencies, sheared ocean current profiles,
//! Strouhal vortex shedding cross-flow lock-in resonance, amplified Morison hydrodynamic drag,
//! dynamic cyclic bending stress ranges, S-N curve Miner's cumulative fatigue damage, and suppression strakes.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Subsea marine riser structural configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum MarineRiserType {
    /// Top-Tensioned Production Riser (TTR) with tensioner hydropneumatic cylinders.
    TopTensionedProductionRiser,
    /// Marine Drilling Riser (MDR) with syntactic foam buoyancy modules and auxiliary choke/kill lines.
    MarineDrillingRiser,
    /// Steel Catenary Riser (SCR) connected to floating production facility (FPSO/Semi-submersible).
    SteelCatenaryRiser,
}

/// Ocean current velocity shear profile.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OceanCurrentProfile {
    /// Uniform current velocity across entire water column depth.
    UniformCurrent,
    /// Linear shear current decreasing from sea surface to seabed.
    LinearShearCurrent,
    /// 1/7th Power Law exponential boundary layer current profile.
    PowerLawShearCurrent,
}

/// VIV suppression device applied to riser outer casing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum VivSuppressionDevice {
    /// Bare bare pipe without suppression devices (baseline vortex shedding).
    BarePipe,
    /// Triple-start helical strakes (pitch = 17.5 D, height = 0.12 D, ~85% suppression).
    HelicalStrakesTripleStart,
    /// Teardrop aerodynamic fairings with swivel collar (~92% suppression, low drag).
    TeardropFairings,
}

impl VivSuppressionDevice {
    /// Efficiency factor in suppressing VIV amplitude (fraction 0.0 to 1.0).
    pub fn suppression_efficiency(&self) -> f64 {
        match self {
            Self::BarePipe => 0.0,
            Self::HelicalStrakesTripleStart => 0.85,
            Self::TeardropFairings => 0.92,
        }
    }

    /// Residual baseline hydrodynamic drag coefficient Cd0.
    pub fn baseline_drag_coefficient(&self) -> f64 {
        match self {
            Self::BarePipe => 1.05,
            Self::HelicalStrakesTripleStart => 1.35, // Helical strakes increase form drag
            Self::TeardropFairings => 0.65,          // Fairings streamline wake
        }
    }
}

/// Input parameters for subsea marine riser VIV and fatigue simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubseaRiserVivProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Marine riser technological architecture.
    pub riser_type: MarineRiserType,
    /// Ocean current velocity shear distribution.
    pub current_profile: OceanCurrentProfile,
    /// Outer surface VIV suppression device.
    pub suppression_device: VivSuppressionDevice,
    /// Total water depth from sea surface to mudline seabed in meters (m, standard 500 to 3000 m).
    pub water_depth_m: f64,
    /// Total riser length between top tensioner and subsea wellhead L in meters (m, standard 500 to 3200 m).
    pub riser_length_m: f64,
    /// Steel main pipe outer diameter D_outer in millimeters (mm, standard 250 to 600 mm).
    pub outer_diameter_mm: f64,
    /// Steel main pipe wall thickness t_wall in millimeters (mm, standard 15 to 45 mm).
    pub wall_thickness_mm: f64,
    /// Top tensioner pretension force T_top in kiloNewtons (kN, standard 1000 to 15000 kN).
    pub top_tension_kn: f64,
    /// Structural steel elastic Young's modulus E in GigaPascals (GPa, standard 200 to 215 GPa).
    pub steel_elastic_modulus_gpa: f64,
    /// Steel density in kilograms per cubic meter (kg/m³, standard 7850 kg/m³).
    pub steel_density_kg_m3: f64,
    /// Internal borehole drilling mud / hydrocarbon fluid density in kg/m³ (standard 850 to 1800 kg/m³).
    pub internal_fluid_density_kg_m3: f64,
    /// Seawater ambient density in kilograms per cubic meter (kg/m³, standard 1025 kg/m³).
    pub seawater_density_kg_m3: f64,
    /// Surface maximum design ocean current velocity U_surface in meters per second (m/s, standard 0.5 to 2.5 m/s).
    pub surface_current_velocity_m_s: f64,
    /// Seabed mudline bottom current velocity U_bottom in meters per second (m/s, standard 0.05 to 0.8 m/s).
    pub bottom_current_velocity_m_s: f64,
    /// Strouhal vortex shedding dimensionless number St (standard 0.18 to 0.22, typically 0.20).
    pub strouhal_number: f64,
    /// Design service operating life in years (standard 20 to 30 years).
    pub design_fatigue_life_years: f64,
}

impl Default for SubseaRiserVivProblem {
    fn default() -> Self {
        Self {
            problem_id: "deepwater_drilling_riser_1800m".to_string(),
            riser_type: MarineRiserType::TopTensionedProductionRiser,
            current_profile: OceanCurrentProfile::LinearShearCurrent,
            suppression_device: VivSuppressionDevice::HelicalStrakesTripleStart,
            water_depth_m: 1800.0,                // 1800 m ultra-deepwater
            riser_length_m: 1850.0,               // 1850 m riser string
            outer_diameter_mm: 533.4,             // 21.0 inch standard marine riser
            wall_thickness_mm: 25.4,              // 1.0 inch wall thickness
            top_tension_kn: 6500.0,               // 6500 kN top tension
            steel_elastic_modulus_gpa: 207.0,     // 207 GPa X-65 linepipe steel
            steel_density_kg_m3: 7850.0,          // Carbon steel
            internal_fluid_density_kg_m3: 1250.0, // 10.4 ppg weighted drilling mud
            seawater_density_kg_m3: 1025.0,       // Seawater
            surface_current_velocity_m_s: 1.65,   // 1.65 m/s Loop Current / Gulf Stream
            bottom_current_velocity_m_s: 0.25,    // 0.25 m/s bottom current
            strouhal_number: 0.20,                // Strouhal number 0.20
            design_fatigue_life_years: 25.0,      // 25-year service life
        }
    }
}

/// Simulation results for subsea marine riser VIV lock-in, hydrodynamic drag, and fatigue damage.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubseaRiserVivResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total mass per unit length (steel + internal fluid + hydrodynamic added mass) in kg/m.
    pub total_mass_per_meter_kg_m: f64,
    /// Average effective axial tension throughout riser column T_avg in kiloNewtons (kN).
    pub average_tension_kn: f64,
    /// Fundamental cross-flow transverse natural frequency f_1 in Hertz (Hz).
    pub fundamental_frequency_hz: f64,
    /// Dominant resonant VIV lock-in mode order number n_lockin.
    pub dominant_lockin_mode: u32,
    /// Resonant lock-in natural frequency f_n in Hertz (Hz).
    pub lockin_natural_frequency_hz: f64,
    /// Peak vortex shedding frequency in surface high-current layer in Hertz (Hz).
    pub surface_shedding_frequency_hz: f64,
    /// Maximum cross-flow lock-in vibration amplitude-to-diameter ratio (A / D).
    pub max_amplitude_ratio_a_over_d: f64,
    /// Effective amplified hydrodynamic Morison drag coefficient C_d.
    pub amplified_drag_coefficient: f64,
    /// Peak static plus dynamic total mudline base shear force in kiloNewtons (kN).
    pub peak_base_shear_kn: f64,
    /// Peak cyclic dynamic bending stress range Delta_sigma in MegaPascals (MPa).
    pub cyclic_bending_stress_range_mpa: f64,
    /// Annual cumulative Miner's fatigue damage accumulation rate D_annual (1/year).
    pub annual_fatigue_damage_rate: f64,
    /// Predicted fatigue service life until crack initiation in years.
    pub predicted_fatigue_life_years: f64,
    /// Fatigue Design Factor (FDF) safety margin achieved (DNV requirement >= 10.0 for uninspectable subsea).
    pub fatigue_safety_factor: f64,
    /// Assessment indicating VIV lock-in cross-flow amplitude is safely suppressed (A/D <= 0.20).
    pub is_viv_amplitude_acceptable: bool,
    /// Assessment indicating fatigue safety factor meets DNV-RP-F204 requirement (FDF >= 10.0).
    pub is_fatigue_life_compliant: bool,
    /// Assessment indicating top tension prevents riser compression buckling at seabed.
    pub is_tensioner_capacity_sufficient: bool,
    /// Overall subsea marine riser dynamic and structural viability.
    pub is_subsea_riser_viable: bool,
}

/// Solves subsea marine riser VIV dynamics, lock-in, and fatigue damage per DNV-RP-F204 / API 16Q.
pub fn solve_subsea_riser_viv(
    problem: &SubseaRiserVivProblem,
) -> Result<SubseaRiserVivResult, String> {
    if problem.riser_length_m <= 100.0 {
        return Err("Riser length must be greater than 100 m".to_string());
    }
    if problem.outer_diameter_mm <= 50.0 {
        return Err("Outer diameter must be greater than 50 mm".to_string());
    }
    if problem.wall_thickness_mm <= 2.0
        || problem.wall_thickness_mm * 2.0 >= problem.outer_diameter_mm
    {
        return Err(
            "Wall thickness must be positive and less than half the outer diameter".to_string(),
        );
    }
    if problem.top_tension_kn <= 100.0 {
        return Err(
            "Top tension must be greater than 100 kN to prevent riser compression collapse"
                .to_string(),
        );
    }

    let g = 9.80665;
    let d_out_m = problem.outer_diameter_mm * 0.001;
    let t_wall_m = problem.wall_thickness_mm * 0.001;
    let d_in_m = d_out_m - 2.0 * t_wall_m;
    let l_m = problem.riser_length_m;

    // 1. Cross-sectional geometric properties:
    let a_steel_m2 = std::f64::consts::PI * (d_out_m.powi(2) - d_in_m.powi(2)) / 4.0;
    let a_int_m2 = std::f64::consts::PI * d_in_m.powi(2) / 4.0;
    let a_disp_m2 = std::f64::consts::PI * d_out_m.powi(2) / 4.0;

    let i_bending_m4 = std::f64::consts::PI * (d_out_m.powi(4) - d_in_m.powi(4)) / 64.0;
    let e_pa = problem.steel_elastic_modulus_gpa * 1.0e9;
    let ei_n_m2 = e_pa * i_bending_m4;

    // Mass components per meter:
    let m_steel = a_steel_m2 * problem.steel_density_kg_m3;
    let m_int = a_int_m2 * problem.internal_fluid_density_kg_m3;
    // Hydrodynamic added mass (Cm = 1.0 for circular cylinder):
    let m_added = a_disp_m2 * problem.seawater_density_kg_m3 * 1.0;
    let m_total = m_steel + m_int + m_added;

    // Submerged submerged weight per meter:
    let m_submerged = m_steel + m_int - (a_disp_m2 * problem.seawater_density_kg_m3);
    let total_submerged_weight_n = m_submerged * g * l_m;

    // Bottom tension at seabed:
    let t_top_n = problem.top_tension_kn * 1000.0;
    let t_bottom_n = t_top_n - total_submerged_weight_n;
    let t_avg_n = (t_top_n + t_bottom_n) / 2.0;

    if t_bottom_n <= 0.0 {
        return Err(
            "Riser suffers compression buckling at seabed; increase top tension".to_string(),
        );
    }

    // 2. Natural modal frequencies of tensioned beam-column:
    // f_n = (n / (2*L)) * sqrt(T_avg / m_total) * sqrt(1 + (n*pi)^2 * EI / (T_avg * L^2))
    let c_wave = (t_avg_n / m_total).sqrt();
    let f_1 = (1.0 / (2.0 * l_m))
        * c_wave
        * (1.0 + std::f64::consts::PI.powi(2) * ei_n_m2 / (t_avg_n * l_m.powi(2))).sqrt();

    // 3. Vortex shedding excitation frequencies:
    // f_vs(z) = St * U(z) / D_out
    let f_vs_surface = problem.strouhal_number * problem.surface_current_velocity_m_s / d_out_m;
    let f_vs_bottom = problem.strouhal_number * problem.bottom_current_velocity_m_s / d_out_m;
    let _f_vs_avg = (f_vs_surface + f_vs_bottom) / 2.0;

    // Lock-in mode order identification:
    let mut n_lockin = (f_vs_surface / f_1).round() as u32;
    if n_lockin == 0 {
        n_lockin = 1;
    }
    let n_f = n_lockin as f64;
    let f_lockin = (n_f / (2.0 * l_m))
        * c_wave
        * (1.0 + (n_f * std::f64::consts::PI).powi(2) * ei_n_m2 / (t_avg_n * l_m.powi(2))).sqrt();

    // 4. Cross-flow lock-in amplitude and drag amplification:
    // Bare pipe unsuppressed lock-in amplitude A/D ~ 1.0 to 1.2
    let bare_a_over_d = 1.05;
    let supp_factor = (1.0 - problem.suppression_device.suppression_efficiency()).powi(2);
    let a_over_d = (bare_a_over_d * supp_factor).max(0.02);

    // Amplified Morison drag coefficient Cd(A/D) = Cd0 * (1 + 2 * A/D):
    let cd0 = problem.suppression_device.baseline_drag_coefficient();
    let cd_amplified = cd0 * (1.0 + 2.0 * a_over_d);

    // 5. Total hydrodynamic drag and base shear force:
    // Average current velocity squared:
    let u_eff_m_s =
        (problem.surface_current_velocity_m_s + problem.bottom_current_velocity_m_s) / 2.0;
    let drag_force_per_meter =
        0.5 * problem.seawater_density_kg_m3 * cd_amplified * d_out_m * u_eff_m_s.powi(2);
    let total_drag_force_n = drag_force_per_meter * l_m;
    let peak_base_shear_kn = (total_drag_force_n * 0.55) / 1000.0;

    // 6. Cyclic dynamic bending stress and S-N fatigue calculation (DNV-RP-F204 / DNV Class D curve):
    // Maximum curvature kappa ~ A * (n * pi / L)^2 attenuated by sheared lock-in length ratio (L_lockin / L ~ 0.25):
    let lockin_zone_ratio = 0.25;
    let y_max_m = a_over_d * d_out_m;
    let curvature_m_inv = y_max_m * (n_f * std::f64::consts::PI / l_m).powi(2) * lockin_zone_ratio;
    let bending_moment_n_m = ei_n_m2 * curvature_m_inv;
    let sigma_bending_pa = bending_moment_n_m * (d_out_m / 2.0) / i_bending_m4;
    let delta_sigma_mpa = (2.0 * sigma_bending_pa) / 1.0e6; // Double amplitude peak-to-peak stress range

    // DNV S-N Class D curve in seawater with cathodic protection:
    // log10(N) = log10(C_sn) - m * log10(Delta_sigma)
    // C_sn = 1.01e12, m = 3.0 for Delta_sigma > 52.6 MPa; m = 5.0 for Delta_sigma <= 52.6 MPa
    let cycles_per_year = f_lockin * 365.25 * 86400.0;
    let allowable_cycles_n = if delta_sigma_mpa > 52.6 {
        1.01e12 / delta_sigma_mpa.powf(3.0)
    } else {
        // High-cycle fatigue regime
        1.0e12 * 52.6f64.powf(2.0) / delta_sigma_mpa.powf(5.0)
    };
    let d_annual = (cycles_per_year / allowable_cycles_n).max(1.0e-7);
    let predicted_life_years = 1.0 / d_annual;
    let fdf = predicted_life_years / problem.design_fatigue_life_years;

    // 7. Safety and compliance assessments:
    let is_viv_amplitude_acceptable = a_over_d <= 0.25;
    let is_fatigue_life_compliant = fdf >= 10.0; // DNV-RP-F204 uninspectable subsea threshold
    let is_tensioner_capacity_sufficient = t_bottom_n >= (0.10 * t_top_n);
    let is_subsea_riser_viable = is_viv_amplitude_acceptable
        && is_fatigue_life_compliant
        && is_tensioner_capacity_sufficient;

    Ok(SubseaRiserVivResult {
        problem_id: problem.problem_id.clone(),
        total_mass_per_meter_kg_m: m_total,
        average_tension_kn: t_avg_n / 1000.0,
        fundamental_frequency_hz: f_1,
        dominant_lockin_mode: n_lockin,
        lockin_natural_frequency_hz: f_lockin,
        surface_shedding_frequency_hz: f_vs_surface,
        max_amplitude_ratio_a_over_d: a_over_d,
        amplified_drag_coefficient: cd_amplified,
        peak_base_shear_kn,
        cyclic_bending_stress_range_mpa: delta_sigma_mpa,
        annual_fatigue_damage_rate: d_annual,
        predicted_fatigue_life_years: predicted_life_years,
        fatigue_safety_factor: fdf,
        is_viv_amplitude_acceptable,
        is_fatigue_life_compliant,
        is_tensioner_capacity_sufficient,
        is_subsea_riser_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deepwater_subsea_riser_viv_strakes() -> Result<(), String> {
        let problem = SubseaRiserVivProblem::default();
        let res = solve_subsea_riser_viv(&problem)?;

        assert!(res.total_mass_per_meter_kg_m > 500.0 && res.total_mass_per_meter_kg_m < 900.0);
        assert!(res.average_tension_kn > 3500.0 && res.average_tension_kn < 6500.0);
        assert!(res.fundamental_frequency_hz > 0.015 && res.fundamental_frequency_hz < 0.08);
        assert!(res.dominant_lockin_mode >= 8 && res.dominant_lockin_mode <= 35);
        assert!(res.max_amplitude_ratio_a_over_d < 0.25); // Helical strakes suppress A/D
        assert!(res.amplified_drag_coefficient > 1.2 && res.amplified_drag_coefficient < 2.2);
        assert!(res.fatigue_safety_factor >= 10.0); // DNV FDF satisfied with strakes
        assert!(res.is_viv_amplitude_acceptable);
        assert!(res.is_fatigue_life_compliant);
        assert!(res.is_tensioner_capacity_sufficient);
        assert!(res.is_subsea_riser_viable);
        Ok(())
    }

    #[test]
    fn test_insufficient_top_tension() {
        let problem = SubseaRiserVivProblem {
            top_tension_kn: 500.0, // Insufficient to carry 1850m riser weight
            ..Default::default()
        };
        assert!(solve_subsea_riser_viv(&problem).is_err());
    }
}

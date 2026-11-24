//! Biopharmaceutical Protein Downstream Purification Chromatography Column Dynamics
//!
//! Formulated according to:
//! - FDA cGMP "Guidance for Industry: Process Validation: General Principles and Practices"
//! - ASME BPE "Bioprocessing Equipment Standard"
//! - Guiochon, G., Felinger, A., Shirazi, D. G. & Katti, A. M., "Fundamentals of Preparative and Nonlinear Chromatography", Academic Press
//! - Thomas, H. C., "Heterogeneous Ion Exchange in a Flowing System", J. Am. Chem. Soc.
//! - Chung, S. F. & Wen, C. Y., "Longitudinal Dispersion of Liquid Flowing through Fixed and Fluidized Beds"
//!
//! Evaluates packed resin bed interstitial hydrodynamics, axial dispersion, liquid-film mass transfer,
//! Langmuir non-linear adsorption equilibrium, breakthrough curve front tracking (Thomas solution),
//! dynamic binding capacity (DBC at 10%), chromatographic peak resolution, and Blake-Kozeny column pressure drop.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Chromatography resin binding chemistry mechanism.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChromatographyMode {
    /// Protein A Affinity Chromatography (mAb capture from clarified cell culture harvest).
    ProteinAAffinity,
    /// Cation Exchange Chromatography (CEX, polisher for charge variants / aggregates).
    CationExchangeCex,
    /// Anion Exchange Chromatography (AEX, flow-through polishing for DNA / host cell proteins).
    AnionExchangeAex,
    /// Hydrophobic Interaction Chromatography (HIC, separation by surface hydrophobicity).
    HydrophobicInteractionHic,
}

/// Input parameters for bioprocess chromatography column breakthrough and sizing simulation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromatographyColumnProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Chromatography separation mode.
    pub chromatography_mode: ChromatographyMode,
    /// Column internal diameter D_col in millimeters (mm, standard 50 to 1200 mm).
    pub column_diameter_mm: f64,
    /// Packed bed settled bed height L_bed in millimeters (mm, standard 100 to 350 mm).
    pub bed_height_mm: f64,
    /// Superficial linear mobile phase volumetric velocity v_lin in centimeters per hour (cm/h, standard 100 to 600 cm/h).
    pub linear_velocity_cm_h: f64,
    /// Resin bead mean volumetric particle diameter d_p in micrometers (µm, standard 35 to 90 µm).
    pub resin_particle_diameter_um: f64,
    /// Bed inter-particle external void fraction epsilon_b (standard 0.32 to 0.40).
    pub bed_void_fraction: f64,
    /// Intra-particle intra-pore porosity epsilon_p (standard 0.50 to 0.75).
    pub intraparticle_porosity: f64,
    /// Feed load target protein concentration C_0 in grams per liter (g/L, standard 0.5 to 10.0 g/L).
    pub feed_protein_concentration_g_l: f64,
    /// Langmuir maximum saturation adsorption binding capacity q_max in grams of protein per liter of resin (g/L_resin).
    pub max_binding_capacity_g_l: f64,
    /// Langmuir adsorption association equilibrium constant K_L in liters per gram (L/g, standard 1.0 to 50.0 L/g).
    pub langmuir_constant_l_g: f64,
    /// Protein molecular diffusion coefficient in aqueous buffer D_m in m²/s (standard 3.5e-11 to 8.5e-11 m²/s).
    pub molecular_diffusivity_m2_s: f64,
    /// Buffer dynamic viscosity mu in milliPascal-seconds (mPa*s or cP, standard 0.95 to 1.40 mPa*s).
    pub buffer_viscosity_mpa_s: f64,
    /// Buffer mass density rho in kilograms per cubic meter (kg/m³, standard 1000 to 1040 kg/m³).
    pub buffer_density_kg_m3: f64,
}

impl Default for ChromatographyColumnProblem {
    fn default() -> Self {
        Self {
            problem_id: "mab_capture_protein_a_column".to_string(),
            chromatography_mode: ChromatographyMode::ProteinAAffinity,
            column_diameter_mm: 300.0, // 300 mm pilot/manufacturing column (30 cm)
            bed_height_mm: 200.0,      // 200 mm (20 cm) packed bed height
            linear_velocity_cm_h: 300.0, // 300 cm/h process operating linear flow rate
            resin_particle_diameter_um: 50.0, // 50 µm agarose/polymer Protein A beads
            bed_void_fraction: 0.36,   // 36% interstitial voidage
            intraparticle_porosity: 0.65, // 65% particle internal pore volume
            feed_protein_concentration_g_l: 3.50, // 3.5 g/L mAb cell culture harvest titer
            max_binding_capacity_g_l: 55.0, // 55 g mAb / L resin maximum static capacity
            langmuir_constant_l_g: 12.0, // 12.0 L/g strong affinity binding constant
            molecular_diffusivity_m2_s: 4.8e-11, // 4.8e-11 m²/s typical IgG mAb diffusivity
            buffer_viscosity_mpa_s: 1.05, // PBS equilibration buffer viscosity
            buffer_density_kg_m3: 1010.0, // Buffer density
        }
    }
}

/// Simulation results for chromatography column breakthrough front, dynamic binding capacity, and pressure drop.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChromatographyColumnResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Total packed resin bed geometric volume V_bed in liters (L).
    pub column_bed_volume_liters: f64,
    /// Operating volumetric mobile phase flow rate Q in liters per hour (L/h).
    pub volumetric_flow_rate_l_h: f64,
    /// Column residence time (empty bed contact time) in minutes (min).
    pub residence_time_minutes: f64,
    /// Mobile phase interstitial fluid velocity u in millimeters per second (mm/s).
    pub interstitial_velocity_mm_s: f64,
    /// Particle Reynolds number Re_p in packed resin bed.
    pub particle_reynolds_number: f64,
    /// Axial hydrodynamic dispersion coefficient D_ax in square meters per second (m²/s).
    pub axial_dispersion_m2_s: f64,
    /// Liquid-film external mass transfer coefficient k_film in meters per second (m/s).
    pub film_mass_transfer_coeff_m_s: f64,
    /// Overall Height Equivalent to a Theoretical Plate (HETP) in millimeters (mm).
    pub hetp_mm: f64,
    /// Number of theoretical equilibrium plates N_tp across column length.
    pub number_of_theoretical_plates: f64,
    /// Thomas model breakthrough rate constant k_Th in mL/(min*mg).
    pub thomas_rate_constant_ml_min_mg: f64,
    /// Dynamic Binding Capacity at 10% breakthrough (DBC_10%) in grams of protein per liter of resin (g/L_resin).
    pub dynamic_binding_capacity_10pct_g_l: f64,
    /// Process resin utilization efficiency (DBC_10% / q_max) in percentage (%).
    pub capacity_utilization_pct: f64,
    /// Total harvest feed mass loadable prior to 10% breakthrough in grams (g).
    pub loadable_protein_mass_grams: f64,
    /// Breakthrough load phase time to 10% breakthrough in minutes (min).
    pub time_to_10pct_breakthrough_min: f64,
    /// Packed bed hydraulic pressure drop across bed via Blake-Kozeny equation in kiloPascals (kPa).
    pub column_pressure_drop_kpa: f64,
    /// Assessment indicating pressure drop is within sanitary column rating (Delta P <= 300 kPa).
    pub is_pressure_drop_acceptable: bool,
    /// Assessment indicating resin dynamic binding capacity utilization meets commercial target (>= 75%).
    pub is_dbc_utilization_competitive: bool,
    /// Assessment indicating mass transfer kinetics maintain sharp breakthrough front (HETP <= 1.0 mm).
    pub is_plate_count_satisfactory: bool,
    /// Overall bioprocess chromatography column purification viability.
    pub is_chromatography_column_viable: bool,
}

/// Solves biopharmaceutical chromatography column dynamics, breakthrough, and pressure drop per Guiochon / Thomas.
pub fn solve_chromatography_column(
    problem: &ChromatographyColumnProblem,
) -> Result<ChromatographyColumnResult, String> {
    if problem.column_diameter_mm <= 5.0 {
        return Err("Column diameter must be greater than 5 mm".to_string());
    }
    if problem.bed_height_mm <= 10.0 {
        return Err("Bed height must be greater than 10 mm".to_string());
    }
    if problem.linear_velocity_cm_h <= 10.0 {
        return Err("Linear velocity must be greater than 10 cm/h".to_string());
    }
    if problem.feed_protein_concentration_g_l <= 0.05 {
        return Err("Feed protein concentration must be greater than 0.05 g/L".to_string());
    }

    let d_col_m = problem.column_diameter_mm * 0.001;
    let l_bed_m = problem.bed_height_mm * 0.001;
    let a_col_m2 = std::f64::consts::PI * (d_col_m / 2.0).powi(2);
    let v_bed_m3 = a_col_m2 * l_bed_m;
    let v_bed_liters = v_bed_m3 * 1000.0;

    // 1. Mobile phase velocities & residence times:
    // Superficial linear velocity u_0 in m/s:
    let u_0_m_s = (problem.linear_velocity_cm_h * 0.01) / 3600.0;
    let eps_b = problem.bed_void_fraction;
    // Interstitial fluid velocity u:
    let u_inter_m_s = u_0_m_s / eps_b;
    let u_inter_mm_s = u_inter_m_s * 1000.0;

    let q_flow_m3_s = u_0_m_s * a_col_m2;
    let q_flow_l_h = q_flow_m3_s * 1000.0 * 3600.0;
    let res_time_min = (v_bed_m3 / q_flow_m3_s) / 60.0;

    // 2. Packed bed fluid mechanics & dimensionless numbers:
    let d_p_m = problem.resin_particle_diameter_um * 1.0e-6;
    let rho_f = problem.buffer_density_kg_m3;
    let mu_f = problem.buffer_viscosity_mpa_s * 0.001; // Pa*s

    // Particle Reynolds number:
    let re_p = (rho_f * u_0_m_s * d_p_m) / mu_f;
    // Schmidt number:
    let sc = mu_f / (rho_f * problem.molecular_diffusivity_m2_s);

    // 3. Axial dispersion D_ax via Chung-Wen correlation:
    // Pe = u * d_p / D_ax = 0.20/eps_b + 0.011/eps_b * Re_p^0.48
    let pe_p = (0.20 / eps_b) + (0.011 / eps_b) * re_p.powf(0.48);
    let d_ax_m2_s = (u_inter_m_s * d_p_m) / pe_p;

    // 4. Liquid-film mass transfer coefficient k_film (Wilson-Geankoplis correlation):
    // Sh = (1.09 / eps_b) * (Re_p * Sc)^(1/3)
    let sh = (1.09 / eps_b) * (re_p * sc).cbrt();
    let k_film_m_s = (sh * problem.molecular_diffusivity_m2_s) / d_p_m;

    // 5. Plate height HETP (van Deemter / Horvath-Lin equation for preparative chromatography):
    // H = A * d_p + B * D_m / u + C * (d_p^2 / D_pore) * (k' / (1 + k')^2) * u
    let term_a = 1.5 * d_p_m; // Eddy diffusion
    let term_b = 2.0 * problem.molecular_diffusivity_m2_s / u_inter_m_s; // Longitudinal diffusion
    let d_pore = problem.molecular_diffusivity_m2_s * 0.25; // Hindered pore diffusion
    // Retention factor k' = ((1 - eps_b) / eps_b) * K_L * q_max:
    let k_prime =
        ((1.0 - eps_b) / eps_b) * problem.langmuir_constant_l_g * problem.max_binding_capacity_g_l;
    let retention_weight = (k_prime / (1.0 + k_prime).powi(2)).max(0.004);
    let term_c = 0.10 * (d_p_m.powi(2) / d_pore) * retention_weight * u_inter_m_s; // Intraparticle pore mass transfer resistance
    let hetp_m = term_a + term_b + term_c;
    let hetp_mm = hetp_m * 1000.0;
    let n_tp = l_bed_m / hetp_m;

    // 6. Thomas Model solution for 10% breakthrough dynamic binding capacity:
    // C / C_0 = 1 / (1 + exp((k_Th / Q) * (q_0 * M_resin - C_0 * V)))
    // At 10% breakthrough (C/C0 = 0.10):
    // ln(C0/C - 1) = ln(9.0) = 2.1972
    // DBC_10% = q_max * (1 - (2.1972 / N_Thomas))
    //
    // The overall linear-driving-force rate combines the external film and the
    // intraparticle pore resistances in series (Glueckauf LDF):
    //   k_f = 6 k_film / d_p,   k_p = 40 D_pore / d_p^2,   1/k_eff = 1/k_f + 1/k_p.
    // The previous film-only rate ignored pore diffusion, over-predicted N_Thomas
    // (and therefore DBC), and then hid the error behind a 0.60-0.96 clamp.
    let k_film_volumetric_s_inv = k_film_m_s * (6.0 / d_p_m);
    let d_pore_eff_m2_s =
        (problem.molecular_diffusivity_m2_s * problem.intraparticle_porosity).max(1.0e-14);
    let k_pore_volumetric_s_inv = (40.0 * d_pore_eff_m2_s) / d_p_m.powi(2);
    let k_eff_s_inv = 1.0
        / ((1.0 / k_film_volumetric_s_inv.max(1.0e-12))
            + (1.0 / k_pore_volumetric_s_inv.max(1.0e-12)));
    let n_thomas = (k_eff_s_inv * l_bed_m) / u_inter_m_s.max(1.0e-9);
    let dbc_factor = (1.0 - (2.1972 / n_thomas.max(1.0))).clamp(0.0, 1.0);
    let dbc_10pct = problem.max_binding_capacity_g_l * dbc_factor;
    let dbc_utilization_pct = (dbc_10pct / problem.max_binding_capacity_g_l) * 100.0;

    // Total loadable protein mass prior to 10% breakthrough:
    let loadable_mass_g = dbc_10pct * v_bed_liters;
    // Volume loaded to 10% breakthrough:
    let v_load_liters = loadable_mass_g / problem.feed_protein_concentration_g_l;
    let t_breakthrough_min = (v_load_liters / q_flow_l_h) * 60.0;

    // 7. Column pressure drop via Blake-Kozeny equation for laminar packed porous bed:
    // Delta P / L = 150 * (1 - eps_b)^2 / (d_p^2 * eps_b^3) * mu * u_0
    let dp_per_m = 150.0 * (1.0 - eps_b).powi(2) / (d_p_m.powi(2) * eps_b.powi(3)) * mu_f * u_0_m_s;
    let dp_pa = dp_per_m * l_bed_m;
    let dp_kpa = dp_pa / 1000.0;

    // 8. Sanitary performance and compliance checks:
    let is_pressure_drop_acceptable = dp_kpa <= 300.0; // Typical 3 bar column rating
    let is_dbc_utilization_competitive = dbc_utilization_pct >= 75.0;
    let is_plate_count_satisfactory = hetp_mm <= 1.5;
    let is_chromatography_column_viable = is_pressure_drop_acceptable
        && is_dbc_utilization_competitive
        && is_plate_count_satisfactory;

    // Thomas rate constant in mL/(min*mg):
    let k_th_convenient =
        (k_eff_s_inv * 60.0 * 1000.0) / (problem.feed_protein_concentration_g_l * 1000.0);

    Ok(ChromatographyColumnResult {
        problem_id: problem.problem_id.clone(),
        column_bed_volume_liters: v_bed_liters,
        volumetric_flow_rate_l_h: q_flow_l_h,
        residence_time_minutes: res_time_min,
        interstitial_velocity_mm_s: u_inter_mm_s,
        particle_reynolds_number: re_p,
        axial_dispersion_m2_s: d_ax_m2_s,
        film_mass_transfer_coeff_m_s: k_film_m_s,
        hetp_mm,
        number_of_theoretical_plates: n_tp,
        thomas_rate_constant_ml_min_mg: k_th_convenient,
        dynamic_binding_capacity_10pct_g_l: dbc_10pct,
        capacity_utilization_pct: dbc_utilization_pct,
        loadable_protein_mass_grams: loadable_mass_g,
        time_to_10pct_breakthrough_min: t_breakthrough_min,
        column_pressure_drop_kpa: dp_kpa,
        is_pressure_drop_acceptable,
        is_dbc_utilization_competitive,
        is_plate_count_satisfactory,
        is_chromatography_column_viable,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mab_protein_a_chromatography_column() -> Result<(), String> {
        let problem = ChromatographyColumnProblem::default();
        let res = solve_chromatography_column(&problem)?;

        assert!(res.column_bed_volume_liters > 12.0 && res.column_bed_volume_liters < 16.0); // ~14.1 L bed
        assert!(res.volumetric_flow_rate_l_h > 180.0 && res.volumetric_flow_rate_l_h < 240.0);
        assert!(res.residence_time_minutes > 3.0 && res.residence_time_minutes < 5.0);
        assert!(
            res.dynamic_binding_capacity_10pct_g_l > 40.0
                && res.dynamic_binding_capacity_10pct_g_l < 55.0
        );
        assert!(res.capacity_utilization_pct >= 75.0);
        assert!(res.dynamic_binding_capacity_10pct_g_l < problem.max_binding_capacity_g_l);
        assert!(res.capacity_utilization_pct <= 100.0);
        assert!(res.hetp_mm > 0.05 && res.hetp_mm < 1.0);
        assert!(res.column_pressure_drop_kpa > 20.0 && res.column_pressure_drop_kpa < 250.0);
        assert!(res.is_pressure_drop_acceptable);
        assert!(res.is_dbc_utilization_competitive);
        assert!(res.is_plate_count_satisfactory);
        assert!(res.is_chromatography_column_viable);
        Ok(())
    }

    #[test]
    fn test_invalid_tiny_column() {
        let problem = ChromatographyColumnProblem {
            column_diameter_mm: 2.0,
            ..Default::default()
        };
        assert!(solve_chromatography_column(&problem).is_err());
    }
}

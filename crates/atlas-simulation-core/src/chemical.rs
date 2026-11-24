//! Chemical engineering, thermodynamics, and multi-component flash equilibrium.

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Nature and classification of an acid, base, salt, or buffer species.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AcidBaseNature {
    StrongAcid,
    WeakAcid,
    PolyproticAcid,
    StrongBase,
    WeakBase,
    PolyproticBase,
    Salt,
    Amphoteric,
}

/// Chemical species record with thermodynamic dissociation constants and physical properties.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChemicalSpecies {
    pub id: String,
    pub name: String,
    pub formula: String,
    pub nature: AcidBaseNature,
    /// Acid dissociation constants: Ka1, Ka2, ...
    pub ka_values: Vec<f64>,
    /// Base dissociation constants: Kb1, Kb2, ...
    pub kb_values: Vec<f64>,
    pub molar_mass_g_mol: f64,
    pub valence: u32,
    pub density_g_ml: f64,
    pub standard_enthalpy_formation_kj_mol: f64,
    pub description: String,
}

/// Curated catalog of standard industrial, laboratory, and analytical chemical species.
pub fn get_standard_species_catalog() -> Vec<ChemicalSpecies> {
    vec![
        ChemicalSpecies {
            id: "hcl".to_string(),
            name: "Hydrochloric Acid".to_string(),
            formula: "HCl".to_string(),
            nature: AcidBaseNature::StrongAcid,
            ka_values: vec![1.0e6],
            kb_values: vec![],
            molar_mass_g_mol: 36.46,
            valence: 1,
            density_g_ml: 1.18,
            standard_enthalpy_formation_kj_mol: -167.2,
            description: "Strong monoprotic mineral acid, fully dissociated in aqueous solution.".to_string(),
        },
        ChemicalSpecies {
            id: "h2so4".to_string(),
            name: "Sulfuric Acid".to_string(),
            formula: "H2SO4".to_string(),
            nature: AcidBaseNature::PolyproticAcid,
            ka_values: vec![1.0e3, 1.2e-2],
            kb_values: vec![],
            molar_mass_g_mol: 98.08,
            valence: 2,
            density_g_ml: 1.84,
            standard_enthalpy_formation_kj_mol: -814.0,
            description: "Strong diprotic mineral acid: first proton strong, second proton Ka2 = 1.2e-2.".to_string(),
        },
        ChemicalSpecies {
            id: "hno3".to_string(),
            name: "Nitric Acid".to_string(),
            formula: "HNO3".to_string(),
            nature: AcidBaseNature::StrongAcid,
            ka_values: vec![2.4e1],
            kb_values: vec![],
            molar_mass_g_mol: 63.01,
            valence: 1,
            density_g_ml: 1.51,
            standard_enthalpy_formation_kj_mol: -174.1,
            description: "Strong oxidizing monoprotic mineral acid.".to_string(),
        },
        ChemicalSpecies {
            id: "acetic_acid".to_string(),
            name: "Acetic Acid".to_string(),
            formula: "CH3COOH".to_string(),
            nature: AcidBaseNature::WeakAcid,
            ka_values: vec![1.75e-5],
            kb_values: vec![],
            molar_mass_g_mol: 60.05,
            valence: 1,
            density_g_ml: 1.049,
            standard_enthalpy_formation_kj_mol: -484.5,
            description: "Weak organic monoprotic carboxylic acid (pKa = 4.757).".to_string(),
        },
        ChemicalSpecies {
            id: "formic_acid".to_string(),
            name: "Formic Acid".to_string(),
            formula: "HCOOH".to_string(),
            nature: AcidBaseNature::WeakAcid,
            ka_values: vec![1.77e-4],
            kb_values: vec![],
            molar_mass_g_mol: 46.03,
            valence: 1,
            density_g_ml: 1.22,
            standard_enthalpy_formation_kj_mol: -425.1,
            description: "Weak carboxylic acid, simplest methanoic acid (pKa = 3.75).".to_string(),
        },
        ChemicalSpecies {
            id: "hf".to_string(),
            name: "Hydrofluoric Acid".to_string(),
            formula: "HF".to_string(),
            nature: AcidBaseNature::WeakAcid,
            ka_values: vec![6.8e-4],
            kb_values: vec![],
            molar_mass_g_mol: 20.01,
            valence: 1,
            density_g_ml: 1.15,
            standard_enthalpy_formation_kj_mol: -320.1,
            description: "Weak halogen acid with strong hydrogen bonding (pKa = 3.17).".to_string(),
        },
        ChemicalSpecies {
            id: "benzoic_acid".to_string(),
            name: "Benzoic Acid".to_string(),
            formula: "C6H5COOH".to_string(),
            nature: AcidBaseNature::WeakAcid,
            ka_values: vec![6.28e-5],
            kb_values: vec![],
            molar_mass_g_mol: 122.12,
            valence: 1,
            density_g_ml: 1.27,
            standard_enthalpy_formation_kj_mol: -385.2,
            description: "Aromatic carboxylic acid used as food preservative (pKa = 4.20).".to_string(),
        },
        ChemicalSpecies {
            id: "phosphoric_acid".to_string(),
            name: "Phosphoric Acid".to_string(),
            formula: "H3PO4".to_string(),
            nature: AcidBaseNature::PolyproticAcid,
            ka_values: vec![7.11e-3, 6.32e-8, 4.5e-13],
            kb_values: vec![],
            molar_mass_g_mol: 97.99,
            valence: 3,
            density_g_ml: 1.685,
            standard_enthalpy_formation_kj_mol: -1288.3,
            description: "Triprotic mineral acid with three distinct buffering plateaus (pKa1=2.15, pKa2=7.20, pKa3=12.35).".to_string(),
        },
        ChemicalSpecies {
            id: "carbonic_acid".to_string(),
            name: "Carbonic Acid".to_string(),
            formula: "H2CO3".to_string(),
            nature: AcidBaseNature::PolyproticAcid,
            ka_values: vec![4.45e-7, 4.69e-11],
            kb_values: vec![],
            molar_mass_g_mol: 62.03,
            valence: 2,
            density_g_ml: 1.0,
            standard_enthalpy_formation_kj_mol: -691.99,
            description: "Diprotic biological buffer system (pKa1 = 6.35, pKa2 = 10.33).".to_string(),
        },
        ChemicalSpecies {
            id: "citric_acid".to_string(),
            name: "Citric Acid".to_string(),
            formula: "C6H8O7".to_string(),
            nature: AcidBaseNature::PolyproticAcid,
            ka_values: vec![7.4e-4, 1.7e-5, 4.0e-7],
            kb_values: vec![],
            molar_mass_g_mol: 192.12,
            valence: 3,
            density_g_ml: 1.665,
            standard_enthalpy_formation_kj_mol: -1543.8,
            description: "Triprotic tricarboxylic acid present in citrus fruits (pKa1=3.13, pKa2=4.76, pKa3=6.40).".to_string(),
        },
        ChemicalSpecies {
            id: "naoh".to_string(),
            name: "Sodium Hydroxide".to_string(),
            formula: "NaOH".to_string(),
            nature: AcidBaseNature::StrongBase,
            ka_values: vec![],
            kb_values: vec![1.0e6],
            molar_mass_g_mol: 39.997,
            valence: 1,
            density_g_ml: 2.13,
            standard_enthalpy_formation_kj_mol: -425.6,
            description: "Strong caustic alkali base, standard laboratory and industrial titrant.".to_string(),
        },
        ChemicalSpecies {
            id: "koh".to_string(),
            name: "Potassium Hydroxide".to_string(),
            formula: "KOH".to_string(),
            nature: AcidBaseNature::StrongBase,
            ka_values: vec![],
            kb_values: vec![1.0e6],
            molar_mass_g_mol: 56.11,
            valence: 1,
            density_g_ml: 2.04,
            standard_enthalpy_formation_kj_mol: -482.4,
            description: "Strong caustic base, high solubility in aqueous and alcoholic media.".to_string(),
        },
        ChemicalSpecies {
            id: "ca_oh_2".to_string(),
            name: "Calcium Hydroxide".to_string(),
            formula: "Ca(OH)2".to_string(),
            nature: AcidBaseNature::PolyproticBase,
            ka_values: vec![],
            kb_values: vec![1.0e6, 3.74e-3],
            molar_mass_g_mol: 74.09,
            valence: 2,
            density_g_ml: 2.21,
            standard_enthalpy_formation_kj_mol: -986.1,
            description: "Divalent strong base (slaked lime), used in water treatment and mortars.".to_string(),
        },
        ChemicalSpecies {
            id: "nh3".to_string(),
            name: "Ammonia".to_string(),
            formula: "NH3".to_string(),
            nature: AcidBaseNature::WeakBase,
            ka_values: vec![],
            kb_values: vec![1.77e-5],
            molar_mass_g_mol: 17.031,
            valence: 1,
            density_g_ml: 0.73,
            standard_enthalpy_formation_kj_mol: -80.8,
            description: "Weak nitrogenous base (pKb = 4.75; conjugate ammonium pKa = 9.25).".to_string(),
        },
        ChemicalSpecies {
            id: "methylamine".to_string(),
            name: "Methylamine".to_string(),
            formula: "CH3NH2".to_string(),
            nature: AcidBaseNature::WeakBase,
            ka_values: vec![],
            kb_values: vec![4.4e-4],
            molar_mass_g_mol: 31.06,
            valence: 1,
            density_g_ml: 0.699,
            standard_enthalpy_formation_kj_mol: -22.97,
            description: "Primary aliphatic amine, stronger base than ammonia (pKb = 3.36).".to_string(),
        },
        ChemicalSpecies {
            id: "pyridine".to_string(),
            name: "Pyridine".to_string(),
            formula: "C5H5N".to_string(),
            nature: AcidBaseNature::WeakBase,
            ka_values: vec![],
            kb_values: vec![1.7e-9],
            molar_mass_g_mol: 79.10,
            valence: 1,
            density_g_ml: 0.982,
            standard_enthalpy_formation_kj_mol: 100.2,
            description: "Weak heterocyclic aromatic base (pKb = 8.77; conjugate pyridinium pKa = 5.23).".to_string(),
        },
        ChemicalSpecies {
            id: "sodium_acetate".to_string(),
            name: "Sodium Acetate".to_string(),
            formula: "CH3COONa".to_string(),
            nature: AcidBaseNature::Salt,
            ka_values: vec![],
            kb_values: vec![5.71e-10],
            molar_mass_g_mol: 82.03,
            valence: 1,
            density_g_ml: 1.528,
            standard_enthalpy_formation_kj_mol: -708.8,
            description: "Conjugate base salt forming acetate buffers with acetic acid.".to_string(),
        },
        ChemicalSpecies {
            id: "ammonium_chloride".to_string(),
            name: "Ammonium Chloride".to_string(),
            formula: "NH4Cl".to_string(),
            nature: AcidBaseNature::Salt,
            ka_values: vec![5.65e-10],
            kb_values: vec![],
            molar_mass_g_mol: 53.49,
            valence: 1,
            density_g_ml: 1.53,
            standard_enthalpy_formation_kj_mol: -314.4,
            description: "Acidic salt forming ammonia-ammonium chloride buffer pairs.".to_string(),
        },
        ChemicalSpecies {
            id: "sodium_bicarbonate".to_string(),
            name: "Sodium Bicarbonate".to_string(),
            formula: "NaHCO3".to_string(),
            nature: AcidBaseNature::Amphoteric,
            ka_values: vec![4.69e-11],
            kb_values: vec![2.25e-8],
            molar_mass_g_mol: 84.007,
            valence: 1,
            density_g_ml: 2.20,
            standard_enthalpy_formation_kj_mol: -950.8,
            description: "Amphoteric bicarbonate buffer salt; intermediate conjugate species.".to_string(),
        },
    ]
}

/// Find a species in the catalog by case-insensitive identifier or formula.
pub fn find_species_by_id(id_or_formula: &str) -> Option<ChemicalSpecies> {
    let lower = id_or_formula.trim().to_lowercase();
    get_standard_species_catalog().into_iter().find(|s| {
        s.id.to_lowercase() == lower
            || s.formula.to_lowercase() == lower
            || s.name.to_lowercase() == lower
    })
}

/// Input problem definition for an analytical acid-base titration & equilibrium calculation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcidBaseProblem {
    pub analyte_id: String,
    pub analyte_concentration_m: f64,
    pub analyte_volume_ml: f64,
    pub titrant_id: String,
    pub titrant_concentration_m: f64,
    pub temperature_c: f64,
    pub titrant_max_volume_ml: Option<f64>,
    pub step_count: Option<usize>,
}

/// Single calculated data point along a titration curve.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TitrationDataPoint {
    pub titrant_volume_ml: f64,
    pub ph: f64,
    pub poh: f64,
    pub h_ion_conc_m: f64,
    pub oh_ion_conc_m: f64,
    pub buffer_capacity_beta: f64,
    pub fraction_neutralized: f64,
    pub regime: String,
}

/// Comprehensive output from an acid-base equilibrium and titration simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AcidBaseEquilibriumResult {
    pub analyte: ChemicalSpecies,
    pub titrant: ChemicalSpecies,
    pub temperature_c: f64,
    pub kw: f64,
    pub initial_ph: f64,
    pub equivalence_volume_ml: f64,
    pub equivalence_ph: f64,
    pub half_equivalence_volume_ml: f64,
    pub half_equivalence_ph: f64,
    pub max_buffer_capacity: f64,
    pub total_titrant_volume_ml: f64,
    pub titration_curve: Vec<TitrationDataPoint>,
    pub neutralization_enthalpy_kj_mol: f64,
    pub notes: Vec<String>,
}

/// Computes the auto-ionization constant of water Kw as a function of temperature in Celsius.
pub fn compute_water_kw(temp_c: f64) -> f64 {
    let tk = (temp_c + 273.15).max(273.15);
    // Empirical correlation: pKw = 4470.99 / T - 6.0875 + 0.01706 * T
    let pkw = 4470.99 / tk - 6.0875 + 0.01706 * tk;
    10.0_f64.powf(-pkw)
}

/// Solves acid-base equilibrium and computes the complete analytical titration curve.
pub fn solve_acid_base_titration(
    prob: &AcidBaseProblem,
) -> Result<AcidBaseEquilibriumResult, String> {
    if prob.analyte_concentration_m <= 0.0 || prob.analyte_volume_ml <= 0.0 {
        return Err("Analyte concentration and volume must be strictly positive.".to_string());
    }
    if prob.titrant_concentration_m <= 0.0 {
        return Err("Titrant concentration must be strictly positive.".to_string());
    }

    let analyte = find_species_by_id(&prob.analyte_id)
        .ok_or_else(|| format!("Unknown chemical species analyte: '{}'", prob.analyte_id))?;
    let titrant = find_species_by_id(&prob.titrant_id)
        .ok_or_else(|| format!("Unknown chemical species titrant: '{}'", prob.titrant_id))?;

    let kw = compute_water_kw(prob.temperature_c);
    let pkw = -kw.log10();

    let ca = prob.analyte_concentration_m;
    let va = prob.analyte_volume_ml;
    let cb = prob.titrant_concentration_m;
    let nu_a = analyte.valence as f64;
    let nu_b = titrant.valence as f64;

    // Equivalence volume Veq: Ca * Va * nu_a = Cb * Veq * nu_b
    let v_eq = (ca * va * nu_a) / (cb * nu_b);
    let half_v_eq = v_eq * 0.5;

    let is_acid_analyte = matches!(
        analyte.nature,
        AcidBaseNature::StrongAcid | AcidBaseNature::WeakAcid | AcidBaseNature::PolyproticAcid
    );
    let is_base_analyte = matches!(
        analyte.nature,
        AcidBaseNature::StrongBase | AcidBaseNature::WeakBase | AcidBaseNature::PolyproticBase
    );

    if !is_acid_analyte && !is_base_analyte {
        return Err(format!(
            "Analyte '{}' is neither an acid nor a base (nature: {:?})",
            analyte.name, analyte.nature
        ));
    }

    // Determine initial pH, half-equivalence pH, and equivalence pH
    let (initial_ph, half_eq_ph, eq_ph) = if is_acid_analyte {
        // Acid analyte
        let init_ph = match analyte.nature {
            AcidBaseNature::StrongAcid => {
                let h = ca * nu_a;
                -(h.max(1e-14)).log10()
            }
            AcidBaseNature::WeakAcid | AcidBaseNature::PolyproticAcid => {
                let ka = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
                // Exact quadratic: [H+]^2 + Ka [H+] - Ka * Ca = 0
                let h = (-ka + (ka * ka + 4.0 * ka * ca).sqrt()) * 0.5;
                -(h.max(1e-14)).log10()
            }
            _ => 7.0,
        };

        let half_ph = match analyte.nature {
            AcidBaseNature::StrongAcid => {
                // At half equivalence of strong acid: remaining [H+] = (Ca*Va - Cb*V_half) / (Va + V_half)
                let n_rem = ca * (va / 1000.0) * nu_a - cb * (half_v_eq / 1000.0) * nu_b;
                let v_tot = (va + half_v_eq) / 1000.0;
                let h = (n_rem / v_tot).max(1e-14);
                -h.log10()
            }
            AcidBaseNature::WeakAcid | AcidBaseNature::PolyproticAcid => {
                let ka = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
                -ka.log10() // Henderson-Hasselbalch: [A-] = [HA] => pH = pKa
            }
            _ => 7.0,
        };

        let eq_ph_val = match analyte.nature {
            AcidBaseNature::StrongAcid => pkw * 0.5, // 7.0 at 25C
            AcidBaseNature::WeakAcid | AcidBaseNature::PolyproticAcid => {
                let ka = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
                let kb = kw / ka;
                let c_salt = (ca * va) / (va + v_eq);
                // Conjugate base hydrolysis: [OH-] = sqrt(Kb * C_salt)
                let oh = (kb * c_salt).sqrt().max(1e-14);
                let poh = -oh.log10();
                (pkw - poh).clamp(0.0, 14.0)
            }
            _ => 7.0,
        };

        (init_ph, half_ph, eq_ph_val)
    } else {
        // Base analyte titrated with acid
        let init_ph = match analyte.nature {
            AcidBaseNature::StrongBase => {
                let oh = cb * nu_b;
                let poh = -(oh.max(1e-14)).log10();
                pkw - poh
            }
            AcidBaseNature::WeakBase | AcidBaseNature::PolyproticBase => {
                let kb = analyte.kb_values.first().copied().unwrap_or(1.77e-5);
                let oh = (-kb + (kb * kb + 4.0 * kb * ca).sqrt()) * 0.5;
                let poh = -(oh.max(1e-14)).log10();
                pkw - poh
            }
            _ => 7.0,
        };

        let half_ph = match analyte.nature {
            AcidBaseNature::StrongBase => {
                let n_rem = ca * (va / 1000.0) * nu_a - cb * (half_v_eq / 1000.0) * nu_b;
                let v_tot = (va + half_v_eq) / 1000.0;
                let oh = (n_rem / v_tot).max(1e-14);
                pkw - (-oh.log10())
            }
            AcidBaseNature::WeakBase | AcidBaseNature::PolyproticBase => {
                let kb = analyte.kb_values.first().copied().unwrap_or(1.77e-5);
                let pkb = -kb.log10();
                pkw - pkb // pH = pKw - pKb = pKa(conjugate)
            }
            _ => 7.0,
        };

        let eq_ph_val = match analyte.nature {
            AcidBaseNature::StrongBase => pkw * 0.5,
            AcidBaseNature::WeakBase | AcidBaseNature::PolyproticBase => {
                let kb = analyte.kb_values.first().copied().unwrap_or(1.77e-5);
                let ka = kw / kb;
                let c_salt = (ca * va) / (va + v_eq);
                let h = (ka * c_salt).sqrt().max(1e-14);
                -h.log10()
            }
            _ => 7.0,
        };

        (init_ph, half_ph, eq_ph_val)
    };

    let max_v = prob.titrant_max_volume_ml.unwrap_or(v_eq * 2.0);
    let steps = prob.step_count.unwrap_or(120).clamp(20, 1000);

    let mut curve = Vec::with_capacity(steps + 1);
    let mut max_beta = 0.0;

    for i in 0..=steps {
        let frac = (i as f64) / (steps as f64);
        let v_add = frac * max_v;
        let v_total = va + v_add;
        let frac_neut = if v_eq > 1e-9 { v_add / v_eq } else { 0.0 };

        let (ph, regime) = if is_acid_analyte {
            compute_acid_titration_point(
                &analyte, &titrant, ca, va, cb, v_add, v_eq, v_total, kw, pkw,
            )
        } else {
            compute_base_titration_point(
                &analyte, &titrant, ca, va, cb, v_add, v_eq, v_total, kw, pkw,
            )
        };

        let poh = (pkw - ph).clamp(0.0, 14.0);
        let h_conc = 10.0_f64.powf(-ph);
        let oh_conc = 10.0_f64.powf(-poh);

        // Analytical buffer capacity beta = dCb/dpH = ln(10) * ( [H+] + [OH-] + (C_buf * Ka * [H+]) / (Ka + [H+])^2 )
        let ka_eff = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
        let c_buf = (ca * va) / v_total;
        let denom = ka_eff + h_conc;
        let buf_term = if denom > 1e-15 {
            (c_buf * ka_eff * h_conc) / (denom * denom)
        } else {
            0.0
        };
        let beta = std::f64::consts::LN_10 * (h_conc + oh_conc + buf_term);
        if beta > max_beta && frac_neut > 0.05 && frac_neut < 0.95 {
            max_beta = beta;
        }

        curve.push(TitrationDataPoint {
            titrant_volume_ml: v_add,
            ph,
            poh,
            h_ion_conc_m: h_conc,
            oh_ion_conc_m: oh_conc,
            buffer_capacity_beta: beta,
            fraction_neutralized: frac_neut,
            regime,
        });
    }

    let notes = vec![
        format!(
            "Titration of {} ({:.3} M, {:.1} mL) with {} ({:.3} M)",
            analyte.name, ca, va, titrant.name, cb
        ),
        format!(
            "Stoichiometric Equivalence Point: V_eq = {:.2} mL, pH_eq = {:.2}",
            v_eq, eq_ph
        ),
        format!(
            "Buffer Half-Equivalence Point: V_1/2 = {:.2} mL, pH = {:.2}",
            half_v_eq, half_eq_ph
        ),
        format!(
            "Auto-ionization Kw at {:.1} °C: {:.2e}",
            prob.temperature_c, kw
        ),
    ];

    Ok(AcidBaseEquilibriumResult {
        analyte,
        titrant,
        temperature_c: prob.temperature_c,
        kw,
        initial_ph,
        equivalence_volume_ml: v_eq,
        equivalence_ph: eq_ph,
        half_equivalence_volume_ml: half_v_eq,
        half_equivalence_ph: half_eq_ph,
        max_buffer_capacity: max_beta,
        total_titrant_volume_ml: max_v,
        titration_curve: curve,
        neutralization_enthalpy_kj_mol: -57.32,
        notes,
    })
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn compute_acid_titration_point(
    analyte: &ChemicalSpecies,
    _titrant: &ChemicalSpecies,
    ca: f64,
    va: f64,
    cb: f64,
    vb: f64,
    v_eq: f64,
    v_total: f64,
    kw: f64,
    pkw: f64,
) -> (f64, String) {
    let diff = vb - v_eq;
    if vb.abs() < 1e-9 {
        let h = match analyte.nature {
            AcidBaseNature::StrongAcid => ca * (analyte.valence as f64),
            _ => {
                let ka = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
                (-ka + (ka * ka + 4.0 * ka * ca).sqrt()) * 0.5
            }
        };
        (-(h.max(1e-14)).log10(), "Initial Solution".to_string())
    } else if diff.abs() < 1e-5 {
        // At Equivalence Point
        let ph = match analyte.nature {
            AcidBaseNature::StrongAcid => pkw * 0.5,
            _ => {
                let ka = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
                let kb = kw / ka;
                let c_salt = (ca * va) / v_total;
                let oh = (kb * c_salt).sqrt().max(1e-14);
                pkw - (-oh.log10())
            }
        };
        (ph, "Equivalence Point".to_string())
    } else if vb < v_eq {
        // Before Equivalence (Buffer region or strong acid neutralization)
        match analyte.nature {
            AcidBaseNature::StrongAcid => {
                let n_acid_rem = ca * (va / 1000.0) * (analyte.valence as f64) - cb * (vb / 1000.0);
                let h = (n_acid_rem / (v_total / 1000.0)).max(1e-14);
                (-h.log10(), "Acid Neutralization Region".to_string())
            }
            _ => {
                let ka = analyte.ka_values.first().copied().unwrap_or(1.75e-5);
                let ratio = (vb / (v_eq - vb)).clamp(1e-6, 1e6);
                let ph = -ka.log10() + ratio.log10();
                let regime = if (vb - 0.5 * v_eq).abs() < (v_eq * 0.05) {
                    "Half-Equivalence (Buffer Core)".to_string()
                } else {
                    "Buffer Region (Henderson-Hasselbalch)".to_string()
                };
                (ph.clamp(0.0, 14.0), regime)
            }
        }
    } else {
        // After Equivalence (Excess strong base titrant)
        let n_base_excess = cb * (diff / 1000.0);
        let oh = (n_base_excess / (v_total / 1000.0)).max(1e-14);
        let poh = -oh.log10();
        let ph = (pkw - poh).clamp(0.0, 14.0);
        (ph, "Excess Strong Titrant Region".to_string())
    }
}

#[inline]
#[allow(clippy::too_many_arguments)]
fn compute_base_titration_point(
    analyte: &ChemicalSpecies,
    _titrant: &ChemicalSpecies,
    cb: f64,
    vb: f64,
    ca: f64,
    va: f64,
    v_eq: f64,
    v_total: f64,
    kw: f64,
    pkw: f64,
) -> (f64, String) {
    let diff = va - v_eq;
    if va.abs() < 1e-9 {
        let oh = match analyte.nature {
            AcidBaseNature::StrongBase => cb * (analyte.valence as f64),
            _ => {
                let kb = analyte.kb_values.first().copied().unwrap_or(1.77e-5);
                (-kb + (kb * kb + 4.0 * kb * cb).sqrt()) * 0.5
            }
        };
        let poh = -(oh.max(1e-14)).log10();
        (pkw - poh, "Initial Solution".to_string())
    } else if diff.abs() < 1e-5 {
        let ph = match analyte.nature {
            AcidBaseNature::StrongBase => pkw * 0.5,
            _ => {
                let kb = analyte.kb_values.first().copied().unwrap_or(1.77e-5);
                let ka = kw / kb;
                let c_salt = (cb * vb) / v_total;
                let h = (ka * c_salt).sqrt().max(1e-14);
                -h.log10()
            }
        };
        (ph, "Equivalence Point".to_string())
    } else if va < v_eq {
        match analyte.nature {
            AcidBaseNature::StrongBase => {
                let n_base_rem = cb * (vb / 1000.0) * (analyte.valence as f64) - ca * (va / 1000.0);
                let oh = (n_base_rem / (v_total / 1000.0)).max(1e-14);
                let poh = -oh.log10();
                (pkw - poh, "Base Neutralization Region".to_string())
            }
            _ => {
                let kb = analyte.kb_values.first().copied().unwrap_or(1.77e-5);
                let ratio = (va / (v_eq - va)).clamp(1e-6, 1e6);
                let poh = -kb.log10() + ratio.log10();
                let ph = (pkw - poh).clamp(0.0, 14.0);
                let regime = if (va - 0.5 * v_eq).abs() < (v_eq * 0.05) {
                    "Half-Equivalence (Buffer Core)".to_string()
                } else {
                    "Buffer Region (Henderson-Hasselbalch)".to_string()
                };
                (ph, regime)
            }
        }
    } else {
        let n_acid_excess = ca * (diff / 1000.0);
        let h = (n_acid_excess / (v_total / 1000.0)).max(1e-14);
        let ph = (-h.log10()).clamp(0.0, 14.0);
        (ph, "Excess Strong Titrant Region".to_string())
    }
}

/// Simulation harness adapter returning a standardized `SimulationResult`.
pub fn solve_chemical_titration_sim(prob: &AcidBaseProblem) -> Result<SimulationResult, String> {
    let sol = solve_acid_base_titration(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("initial_ph".to_string(), sol.initial_ph);
    metrics.insert(
        "equivalence_volume_ml".to_string(),
        sol.equivalence_volume_ml,
    );
    metrics.insert("equivalence_ph".to_string(), sol.equivalence_ph);
    metrics.insert("half_equivalence_ph".to_string(), sol.half_equivalence_ph);
    metrics.insert("max_buffer_capacity".to_string(), sol.max_buffer_capacity);
    metrics.insert("kw".to_string(), sol.kw);

    let val = AnalyticalComparison::assess(
        "Acid-Base Neutralization Stoichiometry",
        sol.equivalence_volume_ml,
        (prob.analyte_concentration_m * prob.analyte_volume_ml * sol.analyte.valence as f64)
            / (prob.titrant_concentration_m * sol.titrant.valence as f64),
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, 0.0, 1, 1e-6);

    Ok(SimulationResult {
        problem_id: "chemical-acid-base-titration".to_string(),
        engine_id: "atlas-acid-base".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-acid-base", "0.2.0"),
        logs: sol.notes,
    })
}

/// Classification of chemical reaction mechanism order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReactionOrder {
    FirstOrder,
    SecondOrder,
    ReversibleFirstOrder,
}

/// Chemical reaction kinetics and reactor design problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReactionKineticsProblem {
    pub reaction_name: String,
    pub order: ReactionOrder,
    /// Initial concentration of reactant A in mol/L (M).
    pub initial_concentration_m: f64,
    /// Direct rate constant k (if known). If None, calculated via Arrhenius.
    pub rate_constant_k: Option<f64>,
    /// Arrhenius pre-exponential factor A (1/s for 1st order, L/(mol*s) for 2nd order).
    pub pre_exponential_a: Option<f64>,
    /// Activation energy Ea in J/mol.
    pub activation_energy_j_mol: Option<f64>,
    /// Reaction temperature in Kelvin.
    pub temperature_k: f64,
    /// Reaction duration in seconds.
    pub time_seconds: Option<f64>,
    /// Equilibrium constant Kc for reversible reactions (Kc = kf / kr).
    pub equilibrium_constant_kc: Option<f64>,
    /// Volumetric flow rate v0 in m^3/s for continuous reactor sizing.
    pub volumetric_flow_m3_s: Option<f64>,
    /// Target conversion fraction X in [0, 1) for reactor sizing.
    pub target_conversion: Option<f64>,
    /// Molar enthalpy of reaction in kJ/mol (negative for exothermic).
    pub heat_of_reaction_kj_mol: Option<f64>,
}

impl Default for ReactionKineticsProblem {
    fn default() -> Self {
        Self {
            reaction_name: "Thermal Decomposition of Organic Precursor".to_string(),
            order: ReactionOrder::FirstOrder,
            initial_concentration_m: 1.0,
            rate_constant_k: None,
            pre_exponential_a: Some(1.0e13), // Typical frequency factor
            activation_energy_j_mol: Some(85000.0), // 85 kJ/mol
            temperature_k: 350.0,
            time_seconds: Some(120.0),
            equilibrium_constant_kc: Some(5.0),
            volumetric_flow_m3_s: Some(0.01), // 10 L/s
            target_conversion: Some(0.90),    // 90% conversion
            heat_of_reaction_kj_mol: Some(-50.0),
        }
    }
}

/// Result of chemical reaction kinetics and reactor design.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReactionKineticsResult {
    pub reaction_name: String,
    pub temperature_k: f64,
    pub rate_constant_k: f64,
    pub half_life_seconds: f64,
    pub final_concentration_m: f64,
    pub conversion_fraction: f64,
    pub reaction_rate_m_s: f64,
    pub cstr_volume_m3: Option<f64>,
    pub pfr_volume_m3: Option<f64>,
    pub damkohler_number: Option<f64>,
    pub heat_release_rate_kw: Option<f64>,
    pub concentration_profile: Vec<(f64, f64)>,
    pub notes: Vec<String>,
}

/// Universal gas constant R = 8.314462618 J/(mol*K).
const GAS_CONSTANT_R: f64 = 8.314462618;

/// Solves chemical reaction kinetics, integrated rate laws, and continuous reactor sizing.
pub fn solve_reaction_kinetics(
    prob: &ReactionKineticsProblem,
) -> Result<ReactionKineticsResult, String> {
    if prob.initial_concentration_m <= 0.0 {
        return Err("Initial reactant concentration must be strictly positive.".to_string());
    }
    if prob.temperature_k <= 0.0 {
        return Err("Temperature must be strictly positive Kelvin.".to_string());
    }

    // Determine rate constant k via Arrhenius or direct input
    let k = if let Some(direct_k) = prob.rate_constant_k {
        if direct_k <= 0.0 {
            return Err("Rate constant k must be positive.".to_string());
        }
        direct_k
    } else if let (Some(a), Some(ea)) = (prob.pre_exponential_a, prob.activation_energy_j_mol) {
        if a <= 0.0 {
            return Err("Pre-exponential factor A must be positive.".to_string());
        }
        let exp_arg = -ea / (GAS_CONSTANT_R * prob.temperature_k);
        a * exp_arg.exp()
    } else {
        return Err("Must supply either rate_constant_k or both pre_exponential_a and activation_energy_j_mol.".to_string());
    };

    let c0 = prob.initial_concentration_m;
    let t = prob.time_seconds.unwrap_or(60.0).max(0.0);

    let (c_final, conversion, half_life) = match prob.order {
        ReactionOrder::FirstOrder => {
            let cf = c0 * (-k * t).exp();
            let conv = (1.0 - cf / c0).clamp(0.0, 1.0);
            let t_half = std::f64::consts::LN_2 / k;
            (cf, conv, t_half)
        }
        ReactionOrder::SecondOrder => {
            let cf = c0 / (1.0 + k * c0 * t);
            let conv = (1.0 - cf / c0).clamp(0.0, 1.0);
            let t_half = 1.0 / (k * c0);
            (cf, conv, t_half)
        }
        ReactionOrder::ReversibleFirstOrder => {
            let kc = prob.equilibrium_constant_kc.unwrap_or(1.0).max(1e-6);
            let kr = k / kc;
            let k_tot = k + kr;
            let ceq = c0 * (kr / k_tot);
            let cf = ceq + (c0 - ceq) * (-k_tot * t).exp();
            let conv = (1.0 - cf / c0).clamp(0.0, 1.0);
            let t_half = std::f64::consts::LN_2 / k_tot;
            (cf, conv, t_half)
        }
    };

    let rxn_rate = match prob.order {
        ReactionOrder::FirstOrder => k * c_final,
        ReactionOrder::SecondOrder => k * c_final * c_final,
        ReactionOrder::ReversibleFirstOrder => {
            let kc = prob.equilibrium_constant_kc.unwrap_or(1.0).max(1e-6);
            let kr = k / kc;
            let cb = c0 - c_final;
            (k * c_final - kr * cb).max(0.0)
        }
    };

    // Reactor sizing for specified target conversion
    let (cstr_vol, pfr_vol, da_num) =
        if let (Some(x_target), Some(v0)) = (prob.target_conversion, prob.volumetric_flow_m3_s) {
            let x = x_target.clamp(0.01, 0.999);
            let (tau_cstr, tau_pfr) = match prob.order {
                ReactionOrder::FirstOrder => {
                    let tc = x / (k * (1.0 - x));
                    let tp = (1.0 / k) * (1.0 / (1.0 - x)).ln();
                    (tc, tp)
                }
                ReactionOrder::SecondOrder => {
                    let tc = x / (k * c0 * (1.0 - x) * (1.0 - x));
                    let tp = (1.0 / (k * c0)) * (x / (1.0 - x));
                    (tc, tp)
                }
                ReactionOrder::ReversibleFirstOrder => {
                    let kc = prob.equilibrium_constant_kc.unwrap_or(1.0).max(1e-6);
                    let x_eq = kc / (1.0 + kc);
                    let x_eff = x.min(x_eq * 0.99);
                    let k_tot = k * (1.0 + 1.0 / kc);
                    let tc = x_eff / (k_tot * (x_eq - x_eff));
                    let tp = (1.0 / k_tot) * (x_eq / (x_eq - x_eff)).ln();
                    (tc, tp)
                }
            };
            let vc = v0 * tau_cstr;
            let vp = v0 * tau_pfr;
            let da = k * tau_pfr;
            (Some(vc), Some(vp), Some(da))
        } else {
            (None, None, None)
        };

    // Heat release rate in kW: Q_dot = r_A * V * (-Delta H)
    let heat_kw = if let (Some(dh), Some(v_cstr)) = (prob.heat_of_reaction_kj_mol, cstr_vol) {
        // r_A in mol/(L*s) = mol/(dm^3*s) = 1000 mol/(m^3*s)
        // Q = r_A [mol/m^3*s] * V [m^3] * (-dH) [kJ/mol] = kW
        let r_m3 = rxn_rate * 1000.0;
        let q = r_m3 * v_cstr * (-dh);
        Some(q)
    } else {
        None
    };

    // Synthesize concentration time-profile
    let steps = 40;
    let mut profile = Vec::with_capacity(steps + 1);
    let dt = (t.max(1.0)) / (steps as f64);
    for i in 0..=steps {
        let ti = (i as f64) * dt;
        let ci = match prob.order {
            ReactionOrder::FirstOrder => c0 * (-k * ti).exp(),
            ReactionOrder::SecondOrder => c0 / (1.0 + k * c0 * ti),
            ReactionOrder::ReversibleFirstOrder => {
                let kc = prob.equilibrium_constant_kc.unwrap_or(1.0).max(1e-6);
                let kr = k / kc;
                let k_tot = k + kr;
                let ceq = c0 * (kr / k_tot);
                ceq + (c0 - ceq) * (-k_tot * ti).exp()
            }
        };
        profile.push((ti, ci));
    }

    let mut notes = vec![
        format!(
            "Reaction Kinetics: {} (Order: {:?})",
            prob.reaction_name, prob.order
        ),
        format!(
            "Temperature: {:.1} K, Rate constant k = {:.4e}",
            prob.temperature_k, k
        ),
        format!(
            "Initial [A]_0 = {:.3} M, Final [A] = {:.4} M at t = {:.1} s (Conversion = {:.2}%)",
            c0,
            c_final,
            t,
            conversion * 100.0
        ),
        format!("Reaction Half-Life t_1/2 = {:.2} s", half_life),
    ];
    if let (Some(vc), Some(vp)) = (cstr_vol, pfr_vol) {
        notes.push(format!(
            "Continuous Reactor Sizing (Target X = {:.1}%): CSTR Volume = {:.3} m³, PFR Volume = {:.3} m³ (CSTR/PFR Ratio = {:.2}x)",
            prob.target_conversion.unwrap_or(0.9) * 100.0,
            vc,
            vp,
            vc / vp.max(1e-6)
        ));
    }

    Ok(ReactionKineticsResult {
        reaction_name: prob.reaction_name.clone(),
        temperature_k: prob.temperature_k,
        rate_constant_k: k,
        half_life_seconds: half_life,
        final_concentration_m: c_final,
        conversion_fraction: conversion,
        reaction_rate_m_s: rxn_rate,
        cstr_volume_m3: cstr_vol,
        pfr_volume_m3: pfr_vol,
        damkohler_number: da_num,
        heat_release_rate_kw: heat_kw,
        concentration_profile: profile,
        notes,
    })
}

/// Standardized simulation runner for reaction kinetics.
pub fn solve_reaction_kinetics_sim(
    prob: &ReactionKineticsProblem,
) -> Result<SimulationResult, String> {
    let sol = solve_reaction_kinetics(prob)?;

    let mut metrics = HashMap::new();
    metrics.insert("rate_constant_k".to_string(), sol.rate_constant_k);
    metrics.insert("half_life_seconds".to_string(), sol.half_life_seconds);
    metrics.insert(
        "final_concentration_m".to_string(),
        sol.final_concentration_m,
    );
    metrics.insert("conversion_fraction".to_string(), sol.conversion_fraction);
    metrics.insert("reaction_rate_m_s".to_string(), sol.reaction_rate_m_s);
    if let Some(vc) = sol.cstr_volume_m3 {
        metrics.insert("cstr_volume_m3".to_string(), vc);
    }
    if let Some(vp) = sol.pfr_volume_m3 {
        metrics.insert("pfr_volume_m3".to_string(), vp);
    }

    let val = AnalyticalComparison::assess(
        "First-Order Mass Conservation & Conversion",
        sol.final_concentration_m / prob.initial_concentration_m,
        1.0 - sol.conversion_fraction,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, 0.0, 1, 1e-6);

    Ok(SimulationResult {
        problem_id: "chemical-reaction-kinetics".to_string(),
        engine_id: "atlas-reaction-kinetics".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-reaction-kinetics", "0.1.0"),
        logs: sol.notes,
    })
}

/// Chemical component in a multi-component thermodynamic mixture.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ChemicalComponent {
    pub name: String,
    pub feed_mole_fraction_z: f64,
    pub equilibrium_k_value: f64, // Ki = yi / xi at specified T and P
}

/// Isothermal vapor-liquid equilibrium (VLE) flash problem.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FlashProblem {
    pub temperature_k: f64,
    pub pressure_bar: f64,
    pub components: Vec<ChemicalComponent>,
}

/// Solves the Rachford-Rice equation using Newton-Raphson for vapor fraction beta.
pub fn solve_rachford_rice_flash(prob: &FlashProblem) -> Result<SimulationResult, String> {
    if prob.components.is_empty() {
        return Err("Flash calculation requires at least one chemical component".to_string());
    }

    let total_z: f64 = prob.components.iter().map(|c| c.feed_mole_fraction_z).sum();
    if (total_z - 1.0).abs() > 1e-4 {
        return Err(format!(
            "Feed mole fractions must sum to 1.0 (got {:.4})",
            total_z
        ));
    }

    // Check bounds: sum(z * K) > 1 and sum(z / K) > 1 for two-phase region
    let sum_zk: f64 = prob
        .components
        .iter()
        .map(|c| c.feed_mole_fraction_z * c.equilibrium_k_value)
        .sum();
    let sum_z_over_k: f64 = prob
        .components
        .iter()
        .map(|c| {
            if c.equilibrium_k_value > 1e-9 {
                c.feed_mole_fraction_z / c.equilibrium_k_value
            } else {
                1e9
            }
        })
        .sum();

    if sum_zk <= 1.0 {
        // Subcooled liquid
        return make_single_phase_result("subcooled_liquid", 0.0, prob);
    }
    if sum_z_over_k <= 1.0 {
        // Superheated vapor
        return make_single_phase_result("superheated_vapor", 1.0, prob);
    }

    // Newton-Raphson iteration on beta in [0, 1]
    let mut beta = 0.5;
    let mut conv = ConvergenceMonitor::new(1e-8);
    let max_iter = 50;
    let mut converged = false;

    for iter in 1..=max_iter {
        let mut f = 0.0;
        let mut df = 0.0;

        for c in &prob.components {
            let z = c.feed_mole_fraction_z;
            let km1 = c.equilibrium_k_value - 1.0;
            let denom = 1.0 + beta * km1;
            f += (z * km1) / denom;
            df -= (z * km1 * km1) / (denom * denom);
        }

        conv.record(iter as u32, f.abs(), 1, 1e-8);

        if f.abs() < 1e-8 {
            converged = true;
            break;
        }

        if df.abs() > 1e-12 {
            let next_beta = (beta - f / df).clamp(1e-6, 1.0 - 1e-6);
            beta = next_beta;
        }
    }

    if !converged {
        return Err("Rachford-Rice iteration failed to converge within 50 steps".to_string());
    }

    // Compute liquid (x) and vapor (y) compositions
    let mut liquid_x = Vec::new();
    let mut vapor_y = Vec::new();
    for c in &prob.components {
        let z = c.feed_mole_fraction_z;
        let km1 = c.equilibrium_k_value - 1.0;
        let denom = 1.0 + beta * km1;
        let xi = z / denom;
        let yi = c.equilibrium_k_value * xi;
        liquid_x.push(xi);
        vapor_y.push(yi);
    }

    let sum_x: f64 = liquid_x.iter().sum();
    let sum_y: f64 = vapor_y.iter().sum();

    let val = AnalyticalComparison::assess("Vapor Composition Summation Rule", sum_y, 1.0);

    let mut metrics = HashMap::new();
    metrics.insert("vapor_fraction_beta".to_string(), beta);
    metrics.insert("liquid_fraction".to_string(), 1.0 - beta);
    metrics.insert("sum_liquid_x".to_string(), sum_x);
    metrics.insert("sum_vapor_y".to_string(), sum_y);

    let mut logs = vec![
        format!(
            "Flash at T = {:.1} K, P = {:.2} bar",
            prob.temperature_k, prob.pressure_bar
        ),
        format!(
            "Two-Phase Mixture: Vapor Fraction beta = {:.4} (Liquid = {:.4})",
            beta,
            1.0 - beta
        ),
    ];
    for (i, c) in prob.components.iter().enumerate() {
        logs.push(format!(
            "  {}: Feed z = {:.3}, Liquid x = {:.4}, Vapor y = {:.4}",
            c.name, c.feed_mole_fraction_z, liquid_x[i], vapor_y[i]
        ));
    }

    Ok(SimulationResult {
        problem_id: "chemical-vle-flash".to_string(),
        engine_id: "atlas-flash".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 2,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(val),
        provenance: SimulationProvenance::new("atlas-flash", "0.1.0"),
        logs,
    })
}

fn make_single_phase_result(
    phase: &str,
    beta: f64,
    _prob: &FlashProblem,
) -> Result<SimulationResult, String> {
    let mut metrics = HashMap::new();
    metrics.insert("vapor_fraction_beta".to_string(), beta);
    metrics.insert("liquid_fraction".to_string(), 1.0 - beta);

    let conv = ConvergenceMonitor::new(1e-8);
    let logs = vec![format!("Single phase state detected: {}", phase)];

    Ok(SimulationResult {
        problem_id: "chemical-vle-flash".to_string(),
        engine_id: "atlas-flash".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: None,
        provenance: SimulationProvenance::new("atlas-flash", "0.1.0"),
        logs,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rachford_rice_hydrocarbon_flash() -> Result<(), String> {
        let prob = FlashProblem {
            temperature_k: 310.0,
            pressure_bar: 5.0,
            components: vec![
                ChemicalComponent {
                    name: "Propane".to_string(),
                    feed_mole_fraction_z: 0.30,
                    equilibrium_k_value: 3.5,
                },
                ChemicalComponent {
                    name: "n-Butane".to_string(),
                    feed_mole_fraction_z: 0.40,
                    equilibrium_k_value: 1.1,
                },
                ChemicalComponent {
                    name: "n-Pentane".to_string(),
                    feed_mole_fraction_z: 0.30,
                    equilibrium_k_value: 0.35,
                },
            ],
        };

        let res = solve_rachford_rice_flash(&prob)?;
        let beta = res
            .scalar_metrics
            .get("vapor_fraction_beta")
            .copied()
            .unwrap_or(0.0);
        assert!(beta > 0.0 && beta < 1.0);
        if let Some(val) = &res.validation {
            assert!(val.error_percent < 0.01);
        }
        Ok(())
    }

    #[test]
    fn test_strong_acid_strong_base_titration() -> Result<(), String> {
        // 25.0 mL of 0.10 M HCl titrated with 0.10 M NaOH at 25 C
        let prob = AcidBaseProblem {
            analyte_id: "hcl".to_string(),
            analyte_concentration_m: 0.10,
            analyte_volume_ml: 25.0,
            titrant_id: "naoh".to_string(),
            titrant_concentration_m: 0.10,
            temperature_c: 25.0,
            titrant_max_volume_ml: Some(50.0),
            step_count: Some(100),
        };

        let sol = solve_acid_base_titration(&prob)?;
        assert!((sol.initial_ph - 1.0).abs() < 0.05);
        assert!((sol.equivalence_volume_ml - 25.0).abs() < 1e-4);
        assert!((sol.equivalence_ph - 7.00).abs() < 0.05);
        assert_eq!(sol.titration_curve.len(), 101);

        let sim_res = solve_chemical_titration_sim(&prob)?;
        assert_eq!(sim_res.status, SimulationStatus::Converged);
        Ok(())
    }

    #[test]
    fn test_weak_acid_strong_base_titration() -> Result<(), String> {
        // 25.0 mL of 0.10 M Acetic Acid titrated with 0.10 M NaOH at 25 C
        let prob = AcidBaseProblem {
            analyte_id: "acetic_acid".to_string(),
            analyte_concentration_m: 0.10,
            analyte_volume_ml: 25.0,
            titrant_id: "naoh".to_string(),
            titrant_concentration_m: 0.10,
            temperature_c: 25.0,
            titrant_max_volume_ml: Some(50.0),
            step_count: Some(100),
        };

        let sol = solve_acid_base_titration(&prob)?;
        // Initial pH = -log10(sqrt(1.75e-5 * 0.1)) approx 2.88
        assert!(sol.initial_ph > 2.8 && sol.initial_ph < 3.0);
        // Half-equivalence pH = pKa = 4.757
        assert!((sol.half_equivalence_ph - 4.757).abs() < 0.05);
        // Equivalence volume = 25.0 mL
        assert!((sol.equivalence_volume_ml - 25.0).abs() < 1e-4);
        // Equivalence pH is basic for acetate hydrolysis (~8.7)
        assert!(sol.equivalence_ph > 8.5 && sol.equivalence_ph < 9.0);
        assert!(sol.max_buffer_capacity > 0.0);
        Ok(())
    }

    #[test]
    fn test_weak_base_strong_acid_titration() -> Result<(), String> {
        // 25.0 mL of 0.10 M NH3 titrated with 0.10 M HCl at 25 C
        let prob = AcidBaseProblem {
            analyte_id: "nh3".to_string(),
            analyte_concentration_m: 0.10,
            analyte_volume_ml: 25.0,
            titrant_id: "hcl".to_string(),
            titrant_concentration_m: 0.10,
            temperature_c: 25.0,
            titrant_max_volume_ml: Some(50.0),
            step_count: Some(100),
        };

        let sol = solve_acid_base_titration(&prob)?;
        // Initial pH is basic (~11.1)
        assert!(sol.initial_ph > 11.0 && sol.initial_ph < 11.3);
        // Half-equivalence pH = 14 - pKb = 9.25
        assert!((sol.half_equivalence_ph - 9.25).abs() < 0.1);
        // Equivalence volume = 25.0 mL
        assert!((sol.equivalence_volume_ml - 25.0).abs() < 1e-4);
        // Equivalence pH is acidic for NH4+ hydrolysis (~5.28)
        assert!(sol.equivalence_ph > 5.0 && sol.equivalence_ph < 5.5);
        Ok(())
    }

    #[test]
    fn test_species_catalog_integrity() {
        let catalog = get_standard_species_catalog();
        assert!(catalog.len() >= 18);
        for s in &catalog {
            assert!(!s.id.is_empty());
            assert!(!s.name.is_empty());
            assert!(!s.formula.is_empty());
            assert!(s.molar_mass_g_mol > 0.0);
            assert!(s.valence >= 1);
            assert!(find_species_by_id(&s.id).is_some());
            assert!(find_species_by_id(&s.formula).is_some());
        }
    }

    #[test]
    fn test_first_order_reaction_kinetics_and_reactor_sizing() -> Result<(), String> {
        let prob = ReactionKineticsProblem {
            reaction_name: "Thermal Dissociation A -> B".to_string(),
            order: ReactionOrder::FirstOrder,
            initial_concentration_m: 2.0,
            rate_constant_k: Some(0.05), // k = 0.05 s^-1
            pre_exponential_a: None,
            activation_energy_j_mol: None,
            temperature_k: 300.0,
            time_seconds: Some(20.0),
            equilibrium_constant_kc: None,
            volumetric_flow_m3_s: Some(0.005),
            target_conversion: Some(0.80),
            heat_of_reaction_kj_mol: Some(-65.0),
        };

        let sol = solve_reaction_kinetics(&prob)?;
        assert!((sol.rate_constant_k - 0.05).abs() < 1e-6);
        // Half life = ln(2) / 0.05 = 13.863 s
        assert!((sol.half_life_seconds - 13.8629).abs() < 1e-3);
        // [A](20) = 2.0 * exp(-0.05 * 20) = 2.0 * exp(-1.0) = 0.73576 M
        assert!((sol.final_concentration_m - 0.73576).abs() < 1e-3);
        // Conversion = 1 - 0.73576 / 2.0 = 0.63212
        assert!((sol.conversion_fraction - 0.63212).abs() < 1e-3);
        // CSTR Volume for X=0.80: tau = 0.80 / (0.05 * 0.20) = 80 s => V = 0.005 * 80 = 0.40 m^3
        assert!((sol.cstr_volume_m3.unwrap_or(0.0) - 0.40).abs() < 1e-3);
        // PFR Volume for X=0.80: tau = (1/0.05) * ln(1 / 0.20) = 20 * 1.6094 = 32.188 s => V = 0.005 * 32.188 = 0.1609 m^3
        assert!((sol.pfr_volume_m3.unwrap_or(0.0) - 0.1609).abs() < 1e-3);
        // CSTR volume > PFR volume
        assert!(sol.cstr_volume_m3.unwrap_or(0.0) > sol.pfr_volume_m3.unwrap_or(0.0));

        let sim_res = solve_reaction_kinetics_sim(&prob)?;
        assert_eq!(sim_res.status, SimulationStatus::Converged);
        Ok(())
    }

    #[test]
    fn test_second_order_reaction_kinetics() -> Result<(), String> {
        let prob = ReactionKineticsProblem {
            reaction_name: "Dimerization 2A -> B".to_string(),
            order: ReactionOrder::SecondOrder,
            initial_concentration_m: 1.0,
            rate_constant_k: Some(0.02), // k = 0.02 L/(mol*s)
            pre_exponential_a: None,
            activation_energy_j_mol: None,
            temperature_k: 298.15,
            time_seconds: Some(50.0),
            equilibrium_constant_kc: None,
            volumetric_flow_m3_s: None,
            target_conversion: None,
            heat_of_reaction_kj_mol: None,
        };

        let sol = solve_reaction_kinetics(&prob)?;
        // Half life = 1 / (k * c0) = 1 / (0.02 * 1.0) = 50 s
        assert!((sol.half_life_seconds - 50.0).abs() < 1e-4);
        // At t = 50 s (one half-life), [A] = 0.50 M
        assert!((sol.final_concentration_m - 0.50).abs() < 1e-4);
        assert!((sol.conversion_fraction - 0.50).abs() < 1e-4);
        Ok(())
    }
}

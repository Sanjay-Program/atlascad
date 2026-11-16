// crates/atlas-simulation-core/src/heart_valve_hemodynamics.rs
//
// Biomedical & Cardiovascular Engineering:
// Prosthetic Heart Valve Hemodynamics and Effective Orifice Area (EOA) Solver.
//
// Based on:
// - ISO 5840-1: Cardiovascular Implants - Cardiac Valve Prostheses - General Requirements
// - ISO 5840-2 / ISO 5840-3: Surgically Implanted and Transcatheter Heart Valve Substitutes
// - Gorlin & Gorlin (1951): "Hydraulic Formula for Calculation of the Area of the Stenotic Mitral Valve, Other Cardiac Valves, and Central Circulatory Shunts" (Am. Heart J.)
// - Pibarot & Dumesnil (2000): "Hemodynamic and Clinical Impact of Patient-Prosthesis Mismatch" (Circulation)
// - FDA Guidance for Industry and FDA Staff: Heart Valve Guidance
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "heart_valve_hemodynamics_problem".to_string()
}

/// Prosthetic heart valve design classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum ValveProsthesisType {
    /// Bileaflet mechanical heart valve with pyrolytic carbon leaflets (e.g. St. Jude / On-X).
    #[default]
    MechanicalBileaflet,
    /// Stented bovine pericardial / porcine tissue bioprosthetic valve.
    SurgicalTissueBioprosthesis,
    /// Transcatheter Aortic Valve Replacement (TAVR balloon/self-expanding stent).
    TranscatheterAorticTavr,
}

/// Patient-Prosthesis Mismatch (PPM) severity grade.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum PatientProsthesisMismatchGrade {
    /// Clinically insignificant / no mismatch (iEOA > 0.85 cm²/m²).
    #[default]
    None,
    /// Moderate mismatch (0.65 < iEOA <= 0.85 cm²/m²).
    Moderate,
    /// Severe hemodynamically compromising mismatch (iEOA <= 0.65 cm²/m²).
    Severe,
}

/// Heart valve hemodynamics simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartValveProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Valve prosthesis technology.
    #[serde(default)]
    pub valve_type: ValveProsthesisType,
    /// Nominal valve tissue annulus size in millimeters (mm, typ. 19 to 29 mm).
    pub nominal_size_mm: f64,
    /// Heart rate in beats per minute (bpm, typ. 60 - 90 bpm).
    pub heart_rate_bpm: f64,
    /// Left ventricular stroke volume in milliliters (mL/beat, typ. 60 - 100 mL).
    pub stroke_volume_ml: f64,
    /// Systolic ejection period (SEP) in seconds (s, typ. 0.26 - 0.34 s).
    pub systolic_ejection_period_s: f64,
    /// Patient body surface area (BSA) in square meters (m², typ. 1.6 - 2.2 m²).
    pub patient_bsa_m2: f64,
    /// Mean transvalvular systolic pressure gradient in millimeters of mercury (mmHg, typ. 6 - 20 mmHg).
    pub mean_pressure_gradient_mmhg: f64,
    /// Peak transvalvular jet velocity in meters per second (m/s, typ. 1.5 - 3.2 m/s).
    pub peak_jet_velocity_m_s: f64,
    /// Dynamic leaflet closing volume in milliliters per beat (mL, typ. 1.0 - 5.0 mL).
    pub dynamic_closing_volume_ml: f64,
    /// Static transvalvular closed leakage volume in milliliters per beat (mL, typ. 1.0 - 6.0 mL).
    pub static_leakage_volume_ml: f64,
}

impl Default for HeartValveProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            valve_type: ValveProsthesisType::MechanicalBileaflet,
            nominal_size_mm: 23.0,
            heart_rate_bpm: 70.0,
            stroke_volume_ml: 75.0,
            systolic_ejection_period_s: 0.30,
            patient_bsa_m2: 1.85,
            mean_pressure_gradient_mmhg: 10.5,
            peak_jet_velocity_m_s: 2.15,
            dynamic_closing_volume_ml: 2.8,
            static_leakage_volume_ml: 3.2,
        }
    }
}

/// Heart valve hemodynamics simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HeartValveResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Cardiac output in liters per minute (L/min).
    pub cardiac_output_l_min: f64,
    /// Root-mean-square forward systolic volumetric flow rate Q_rms in mL/s.
    pub rms_systolic_flow_rate_ml_s: f64,
    /// Peak transvalvular pressure gradient via modified Bernoulli in mmHg.
    pub peak_pressure_gradient_mmhg: f64,
    /// Gorlin hydraulic Effective Orifice Area (EOA) in square centimeters (cm²).
    pub effective_orifice_area_cm2: f64,
    /// Indexed Effective Orifice Area (iEOA = EOA / BSA) in cm²/m².
    pub indexed_eoa_cm2_per_m2: f64,
    /// Patient-Prosthesis Mismatch (PPM) classification grade.
    pub mismatch_grade: PatientProsthesisMismatchGrade,
    /// Total regurgitant volume per beat (closing + leakage) in milliliters (mL).
    pub total_regurgitant_volume_ml: f64,
    /// Regurgitant fraction percentage (% of stroke volume).
    pub regurgitant_fraction_pct: f64,
    /// Geometric orifice area of the valve housing in cm².
    pub geometric_orifice_area_cm2: f64,
    /// Discharge coefficient C_d = EOA / GOA.
    pub valve_discharge_coefficient: f64,
    /// Whether EOA satisfies ISO 5840 minimum performance requirements.
    pub is_hemodynamic_eoa_acceptable: bool,
    /// Whether regurgitant fraction is within ISO 5840 strict safety limit (RF <= 15.0%).
    pub is_regurgitation_safe: bool,
    /// Whether severe patient-prosthesis mismatch is prevented (iEOA > 0.65 cm²/m²).
    pub is_severe_mismatch_prevented: bool,
}

/// Solves Gorlin EOA, modified Bernoulli peak gradients, regurgitant fraction, and ISO 5840 compliance.
pub fn solve_heart_valve_hemodynamics(
    problem: &HeartValveProblem,
) -> Result<HeartValveResult, String> {
    if problem.nominal_size_mm <= 15.0 || problem.nominal_size_mm >= 35.0 {
        return Err("Valve nominal annulus size must be between 15 and 35 mm".to_string());
    }
    if problem.heart_rate_bpm <= 30.0 || problem.heart_rate_bpm >= 220.0 {
        return Err("Heart rate must be between 30 and 220 bpm".to_string());
    }
    if problem.stroke_volume_ml <= 10.0 || problem.stroke_volume_ml >= 200.0 {
        return Err("Stroke volume must be between 10 and 200 mL".to_string());
    }
    if problem.systolic_ejection_period_s <= 0.10 || problem.systolic_ejection_period_s >= 0.60 {
        return Err("Systolic ejection period must be between 0.10 and 0.60 seconds".to_string());
    }
    if problem.patient_bsa_m2 <= 0.50 || problem.patient_bsa_m2 >= 3.50 {
        return Err("Body surface area BSA must be between 0.50 and 3.50 m²".to_string());
    }

    // Cardiac output: CO = HR * SV / 1000 [L/min]
    let co_l_min = (problem.heart_rate_bpm * problem.stroke_volume_ml) / 1000.0;

    // Root-mean-square forward flow rate during ejection: Q_rms = SV / SEP [mL/s]
    let q_rms_ml_s = problem.stroke_volume_ml / problem.systolic_ejection_period_s;

    // Modified Bernoulli equation: Delta_P_peak = 4.0 * v_peak^2 [mmHg]
    let dp_peak_mmhg = 4.0 * problem.peak_jet_velocity_m_s.powi(2);

    // Gorlin & Gorlin hydraulic formulation for Effective Orifice Area:
    // EOA = Q_rms / (51.6 * c_v * sqrt(Delta_P_mean))
    // Standard empirical constant c_v ~ 1.0 for aortic, 0.88 for mitral. For aortic valves: constant = 51.6
    let mean_dp = problem.mean_pressure_gradient_mmhg.max(1.0);
    let gorlin_constant = 51.6;
    let eoa_cm2 = q_rms_ml_s / (gorlin_constant * mean_dp.sqrt());

    // Indexed EOA: iEOA = EOA / BSA [cm2/m2]
    let ieoa_cm2_m2 = eoa_cm2 / problem.patient_bsa_m2;

    // Patient-Prosthesis Mismatch (PPM) classification:
    let mismatch_grade = if ieoa_cm2_m2 > 0.85 {
        PatientProsthesisMismatchGrade::None
    } else if ieoa_cm2_m2 > 0.65 {
        PatientProsthesisMismatchGrade::Moderate
    } else {
        PatientProsthesisMismatchGrade::Severe
    };

    // Total regurgitant volume and fraction:
    let v_regurg_ml = problem.dynamic_closing_volume_ml + problem.static_leakage_volume_ml;
    let regurg_fraction_pct = (v_regurg_ml / problem.stroke_volume_ml) * 100.0;

    // Geometric Orifice Area (GOA) based on nominal internal valve diameter:
    let d_internal_cm = (problem.nominal_size_mm - 3.0) / 10.0; // housing ring deduction
    let goa_cm2 = (std::f64::consts::PI / 4.0) * d_internal_cm * d_internal_cm;
    let c_d = (eoa_cm2 / goa_cm2).clamp(0.40, 0.95);

    // ISO 5840 limits:
    // Minimum EOA for 23 mm aortic valve is >= 1.30 cm2
    let iso_min_eoa = 0.06 * problem.nominal_size_mm; // ~ 1.38 cm2 for 23 mm
    let is_eoa_ok = eoa_cm2 >= iso_min_eoa;
    let is_regurg_ok = regurg_fraction_pct <= 15.0; // ISO 5840 15% regurgitation limit
    let is_severe_ppm_prevented = ieoa_cm2_m2 > 0.65;

    Ok(HeartValveResult {
        problem_id: problem.problem_id.clone(),
        cardiac_output_l_min: co_l_min,
        rms_systolic_flow_rate_ml_s: q_rms_ml_s,
        peak_pressure_gradient_mmhg: dp_peak_mmhg,
        effective_orifice_area_cm2: eoa_cm2,
        indexed_eoa_cm2_per_m2: ieoa_cm2_m2,
        mismatch_grade,
        total_regurgitant_volume_ml: v_regurg_ml,
        regurgitant_fraction_pct: regurg_fraction_pct,
        geometric_orifice_area_cm2: goa_cm2,
        valve_discharge_coefficient: c_d,
        is_hemodynamic_eoa_acceptable: is_eoa_ok,
        is_regurgitation_safe: is_regurg_ok,
        is_severe_mismatch_prevented: is_severe_ppm_prevented,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mechanical_bileaflet_aortic_valve() -> Result<(), String> {
        let prob = HeartValveProblem {
            problem_id: "aortic_23mm_bileaflet".to_string(),
            valve_type: ValveProsthesisType::MechanicalBileaflet,
            nominal_size_mm: 23.0,
            heart_rate_bpm: 70.0,
            stroke_volume_ml: 75.0,
            systolic_ejection_period_s: 0.30,
            patient_bsa_m2: 1.85,
            mean_pressure_gradient_mmhg: 10.5,
            peak_jet_velocity_m_s: 2.15,
            dynamic_closing_volume_ml: 2.8,
            static_leakage_volume_ml: 3.2,
        };

        let res = solve_heart_valve_hemodynamics(&prob)?;
        assert!(res.cardiac_output_l_min > 4.5 && res.cardiac_output_l_min < 6.0);
        assert!(res.rms_systolic_flow_rate_ml_s > 200.0 && res.rms_systolic_flow_rate_ml_s < 300.0);
        assert!(res.peak_pressure_gradient_mmhg > 15.0 && res.peak_pressure_gradient_mmhg < 25.0);
        assert!(res.effective_orifice_area_cm2 > 1.30 && res.effective_orifice_area_cm2 < 1.80);
        assert!(res.indexed_eoa_cm2_per_m2 > 0.75);
        assert!(res.total_regurgitant_volume_ml > 5.0 && res.total_regurgitant_volume_ml < 8.0);
        assert!(res.regurgitant_fraction_pct < 12.0);
        assert!(res.is_hemodynamic_eoa_acceptable);
        assert!(res.is_regurgitation_safe);
        assert!(res.is_severe_mismatch_prevented);
        Ok(())
    }
}

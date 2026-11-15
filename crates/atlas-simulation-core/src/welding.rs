//! Welding Metallurgy, Heat Input & Thermal Cooling Rates Solver (AWS D1.1 / ISO 15614 / EN 1011-2).
//!
//! Evaluates arc welding net heat input, 2D/3D Rosenthal moving heat source cooling times (t8/5),
//! Carbon Equivalents (IIW CE, Ito-Bessyo Pcm, CET), minimum preheat temperature,
//! and heat-affected zone (HAZ) peak Vickers hardness.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

use crate::convergence::ConvergenceMonitor;
use crate::execution::{SimulationResult, SimulationStatus};
use crate::provenance::SimulationProvenance;
use crate::validation::AnalyticalComparison;

/// Arc welding process with thermal arc efficiency (ISO 15614 / EN 1011-1).
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum WeldingProcess {
    /// Shielded Metal Arc Welding (SMAW / MMA) - efficiency ~0.80.
    Smaw,
    /// Gas Metal Arc Welding (GMAW / MIG/MAG) - efficiency ~0.85.
    Gmaw,
    /// Flux-Cored Arc Welding (FCAW) - efficiency ~0.80.
    Fcaw,
    /// Gas Tungsten Arc Welding (GTAW / TIG) - efficiency ~0.60.
    Gtaw,
    /// Submerged Arc Welding (SAW) - efficiency ~0.95.
    Saw,
    /// Laser Beam Welding (LBW) - efficiency ~0.70.
    Laser,
}

impl WeldingProcess {
    /// Thermal transfer efficiency eta.
    pub fn arc_efficiency(&self) -> f64 {
        match self {
            Self::Smaw => 0.80,
            Self::Gmaw => 0.85,
            Self::Fcaw => 0.80,
            Self::Gtaw => 0.60,
            Self::Saw => 0.95,
            Self::Laser => 0.70,
        }
    }
}

/// Chemical composition of structural steel for weldability and carbon equivalents.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SteelComposition {
    /// Carbon content (wt%).
    pub carbon_pct: f64,
    /// Manganese content (wt%).
    pub manganese_pct: f64,
    /// Silicon content (wt%).
    pub silicon_pct: f64,
    /// Chromium content (wt%).
    pub chromium_pct: f64,
    /// Molybdenum content (wt%).
    pub molybdenum_pct: f64,
    /// Vanadium content (wt%).
    pub vanadium_pct: f64,
    /// Nickel content (wt%).
    pub nickel_pct: f64,
    /// Copper content (wt%).
    pub copper_pct: f64,
    /// Boron content (wt%, typically 0.0005 - 0.003).
    pub boron_pct: f64,
}

impl Default for SteelComposition {
    fn default() -> Self {
        // Typical S355J2 / ASTM A572 Gr 50 structural steel
        Self {
            carbon_pct: 0.16,
            manganese_pct: 1.40,
            silicon_pct: 0.35,
            chromium_pct: 0.05,
            molybdenum_pct: 0.02,
            vanadium_pct: 0.01,
            nickel_pct: 0.04,
            copper_pct: 0.03,
            boron_pct: 0.0002,
        }
    }
}

/// Diffusible hydrogen scale per ISO 3690 / AWS D1.1.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum HydrogenLevel {
    /// Extra low hydrogen: H <= 5 mL / 100g.
    H5,
    /// Low hydrogen: H <= 10 mL / 100g.
    H10,
    /// Medium hydrogen: H <= 15 mL / 100g.
    H15,
}

impl HydrogenLevel {
    pub fn ml_per_100g(&self) -> f64 {
        match self {
            Self::H5 => 5.0,
            Self::H10 => 10.0,
            Self::H15 => 15.0,
        }
    }
}

/// Welding problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeldingProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Welding process.
    pub process: WeldingProcess,
    /// Arc voltage (V).
    pub arc_voltage_v: f64,
    /// Welding current (A).
    pub welding_current_a: f64,
    /// Travel speed (mm/s).
    pub travel_speed_mm_s: f64,
    /// Base plate thickness (mm).
    pub plate_thickness_mm: f64,
    /// Initial workpiece preheat / ambient temperature (°C).
    pub preheat_temp_c: f64,
    /// Steel chemical composition.
    pub composition: SteelComposition,
    /// Diffusible hydrogen level.
    pub hydrogen_level: HydrogenLevel,
    /// Joint restraint intensity factor (kJ/(mm·°C) or EN 1011 restraint index).
    pub joint_restraint_factor: f64,
}

impl Default for WeldingProblem {
    fn default() -> Self {
        Self {
            problem_id: "structural_column_butt_weld".to_string(),
            process: WeldingProcess::Gmaw,
            arc_voltage_v: 28.0,
            welding_current_a: 240.0,
            travel_speed_mm_s: 4.5, // 270 mm/min
            plate_thickness_mm: 25.0,
            preheat_temp_c: 20.0,
            composition: SteelComposition::default(),
            hydrogen_level: HydrogenLevel::H5,
            joint_restraint_factor: 1.0,
        }
    }
}

/// Comprehensive welding metallurgy and cooling rate results.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WeldingResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Gross heat input (kJ/mm).
    pub gross_heat_input_kj_mm: f64,
    /// Net heat input factoring thermal efficiency (kJ/mm).
    pub net_heat_input_kj_mm: f64,
    /// Transition plate thickness between 2D and 3D heat flow (mm).
    pub transition_thickness_mm: f64,
    /// Dominant heat conduction mode ("2D Thin Plate", "3D Thick Plate", or "Intermediate").
    pub heat_conduction_regime: String,
    /// Cooling time from 800°C to 500°C (t8/5, seconds).
    pub cooling_time_t8_5_s: f64,
    /// IIW Carbon Equivalent CE (wt%).
    pub carbon_equivalent_iiw: f64,
    /// Ito-Bessyo cold cracking susceptibility parameter Pcm (wt%).
    pub carbon_equivalent_pcm: f64,
    /// Dearden & O'Neill carbon equivalent CET (wt%).
    pub carbon_equivalent_cet: f64,
    /// Recommended minimum preheat temperature per EN 1011-2 / AWS D1.1 (°C).
    pub recommended_preheat_temp_c: f64,
    /// Current preheat temperature provides safe margin against hydrogen-induced cold cracking.
    pub preheat_sufficient: bool,
    /// Estimated maximum HAZ peak Vickers hardness (HV10).
    pub max_haz_hardness_hv: f64,
    /// Hardness limit compliant (e.g. ISO 15614-1 max 380 HV for non-sour service, 250 HV for sour).
    pub haz_hardness_acceptable: bool,
}

/// Calculates welding heat input, cooling rate t8/5, CE, preheat, and HAZ hardness.
pub fn solve_welding_metallurgy(prob: &WeldingProblem) -> WeldingResult {
    let v_speed = prob.travel_speed_mm_s.max(0.1);
    let u = prob.arc_voltage_v.max(1.0);
    let i = prob.welding_current_a.max(1.0);
    let eta = prob.process.arc_efficiency();

    // Gross heat input E_gross = (U * I * 60) / (1000 * (v * 60)) = (U * I) / (1000 * v) in kJ/mm
    let gross_heat_input = (u * i) / (1000.0 * v_speed);
    let net_heat_input = gross_heat_input * eta; // kJ/mm

    // Material thermal properties for structural carbon-manganese steel:
    // Thermal conductivity lambda = 0.041 W/(mm·K) = 41 W/(m·K)
    // Volumetric heat capacity rho*c = 0.0045 J/(mm³·K) = 4.5e6 J/(m³·K)
    let lambda = 0.041; // W/(mm·°C)
    let rho_c = 0.0045; // J/(mm³·°C)
    let t0 = prob.preheat_temp_c.clamp(0.0, 350.0);
    let d = prob.plate_thickness_mm.max(1.0);

    // Transition thickness between 2D and 3D heat flow:
    // d_trans = sqrt( (E_net * 1000) / (2 * rho_c) * ( 1/(500 - T0) + 1/(800 - T0) ) )
    let d_trans = (((net_heat_input * 1000.0) / (2.0 * rho_c))
        * (1.0 / (500.0 - t0) + 1.0 / (800.0 - t0)))
        .sqrt();

    // Cooling time t8/5 (cooling from 800°C to 500°C):
    // 3D Thick plate cooling time:
    // t8_5_3d = ((E_net * 1000) / (2 * pi * lambda)) * ( 1/(500 - T0) - 1/(800 - T0) )
    let t8_5_3d = ((net_heat_input * 1000.0) / (2.0 * std::f64::consts::PI * lambda))
        * (1.0 / (500.0 - t0) - 1.0 / (800.0 - t0));

    // 2D Thin plate cooling time:
    // t8_5_2d = ((E_net * 1000)^2 / (4 * pi * lambda * rho_c * d^2)) * ( 1/(500 - T0)^2 - 1/(800 - T0)^2 )
    let t8_5_2d = (((net_heat_input * 1000.0).powi(2))
        / (4.0 * std::f64::consts::PI * lambda * rho_c * d.powi(2)))
        * (1.0 / (500.0 - t0).powi(2) - 1.0 / (800.0 - t0).powi(2));

    let (t8_5, regime) = if d >= 1.2 * d_trans {
        (t8_5_3d, "3D Thick Plate".to_string())
    } else if d <= 0.8 * d_trans {
        (t8_5_2d, "2D Thin Plate".to_string())
    } else {
        // Intermediate transition weighting
        let w = (d - 0.8 * d_trans) / (0.4 * d_trans);
        let t_interp = (1.0 - w) * t8_5_2d + w * t8_5_3d;
        (t_interp, "Intermediate Transition".to_string())
    };

    // Carbon equivalents:
    let comp = &prob.composition;
    // IIW CE = C + Mn/6 + (Cr+Mo+V)/5 + (Ni+Cu)/15
    let ce_iiw = comp.carbon_pct
        + (comp.manganese_pct / 6.0)
        + ((comp.chromium_pct + comp.molybdenum_pct + comp.vanadium_pct) / 5.0)
        + ((comp.nickel_pct + comp.copper_pct) / 15.0);

    // Ito-Bessyo Pcm = C + Si/30 + (Mn+Cu+Cr)/20 + Ni/60 + Mo/15 + V/10 + 5*B
    let pcm = comp.carbon_pct
        + (comp.silicon_pct / 30.0)
        + ((comp.manganese_pct + comp.copper_pct + comp.chromium_pct) / 20.0)
        + (comp.nickel_pct / 60.0)
        + (comp.molybdenum_pct / 15.0)
        + (comp.vanadium_pct / 10.0)
        + (5.0 * comp.boron_pct);

    // CET = C + (Mn+Mo)/10 + (Cr+Cu)/20 + Ni/40
    let cet = comp.carbon_pct
        + ((comp.manganese_pct + comp.molybdenum_pct) / 10.0)
        + ((comp.chromium_pct + comp.copper_pct) / 20.0)
        + (comp.nickel_pct / 40.0);

    // Minimum preheat temperature per EN 1011-2 Method B / Cottrell:
    // Tp = 700 * CET + 160 * tanh(d / 35) + 62 * (HD)^0.35 + (53*CET - 32)*Q - 390
    let hd = prob.hydrogen_level.ml_per_100g();
    let q_heat = net_heat_input.clamp(0.5, 4.0);
    let mut t_preheat = 700.0 * cet
        + 160.0 * (d / 35.0).tanh()
        + 62.0 * hd.powf(0.35)
        + (53.0 * cet - 32.0) * q_heat
        - 390.0;
    if t_preheat < 20.0 {
        t_preheat = 20.0; // Ambient minimum
    }

    let preheat_sufficient = prob.preheat_temp_c >= (t_preheat - 5.0);

    // HAZ Maximum Hardness estimation (Yurioka formula):
    // HV_max = 406*C + 164*CE + 183 - (369*C - 149*CE + 96) * log10(t8_5)
    let log_t = t8_5.max(0.5).log10();
    let mut hv_max = 406.0 * comp.carbon_pct + 164.0 * ce_iiw + 183.0
        - (369.0 * comp.carbon_pct - 149.0 * ce_iiw + 96.0) * log_t;
    if hv_max < 160.0 {
        hv_max = 160.0;
    }

    // ISO 15614-1 standard limit for structural steel is typically 380 HV10
    let haz_hardness_acceptable = hv_max <= 380.0;

    WeldingResult {
        problem_id: prob.problem_id.clone(),
        gross_heat_input_kj_mm: gross_heat_input,
        net_heat_input_kj_mm: net_heat_input,
        transition_thickness_mm: d_trans,
        heat_conduction_regime: regime,
        cooling_time_t8_5_s: t8_5,
        carbon_equivalent_iiw: ce_iiw,
        carbon_equivalent_pcm: pcm,
        carbon_equivalent_cet: cet,
        recommended_preheat_temp_c: t_preheat,
        preheat_sufficient,
        max_haz_hardness_hv: hv_max,
        haz_hardness_acceptable,
    }
}

/// Simulation runner adapter for unified verification.
pub fn solve_welding_sim(prob: &WeldingProblem) -> SimulationResult {
    let sol = solve_welding_metallurgy(prob);
    let mut metrics = std::collections::HashMap::new();
    metrics.insert("net_heat_input_kj_mm".to_string(), sol.net_heat_input_kj_mm);
    metrics.insert("cooling_time_t8_5_s".to_string(), sol.cooling_time_t8_5_s);
    metrics.insert("carbon_equivalent_iiw".to_string(), sol.carbon_equivalent_iiw);
    metrics.insert("max_haz_hardness_hv".to_string(), sol.max_haz_hardness_hv);

    let comparison = AnalyticalComparison::assess(
        "haz_hardness_limit",
        sol.max_haz_hardness_hv,
        380.0,
    );

    let mut conv = ConvergenceMonitor::new(1e-6);
    conv.record(1, sol.cooling_time_t8_5_s, 1, 1e-4);

    SimulationResult {
        problem_id: prob.problem_id.clone(),
        engine_id: "atlas.welding.metallurgy".to_string(),
        status: SimulationStatus::Converged,
        execution_time_ms: 1,
        scalar_metrics: metrics,
        convergence: conv,
        validation: Some(comparison),
        provenance: SimulationProvenance::new("atlas.welding.metallurgy", "0.1.0"),
        logs: vec![format!("Net heat input: {:.2} kJ/mm, t8/5: {:.1}s, Peak HV: {:.0}", sol.net_heat_input_kj_mm, sol.cooling_time_t8_5_s, sol.max_haz_hardness_hv)],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_standard_s355_gmaw_structural_welding() {
        let prob = WeldingProblem::default();
        let sol = solve_welding_metallurgy(&prob);

        assert!(sol.gross_heat_input_kj_mm > 1.2 && sol.gross_heat_input_kj_mm < 2.0);
        assert!(sol.net_heat_input_kj_mm > 1.0 && sol.net_heat_input_kj_mm < 1.8);
        assert!(sol.cooling_time_t8_5_s > 2.0 && sol.cooling_time_t8_5_s < 20.0);
        assert!(sol.carbon_equivalent_iiw > 0.35 && sol.carbon_equivalent_iiw < 0.45);
        assert!(sol.max_haz_hardness_hv > 200.0 && sol.max_haz_hardness_hv < 350.0);
        assert!(sol.haz_hardness_acceptable);
    }
}

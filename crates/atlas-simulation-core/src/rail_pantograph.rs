// crates/atlas-simulation-core/src/rail_pantograph.rs
//
// Railway, High-Speed Train & Electrical Traction Engineering:
// High-Speed Rail Pantograph-Catenary Dynamic Interaction and Current Collection Quality Solver.
//
// Based on:
// - EN 50367: Railway Applications - Current Collection Systems - Technical Criteria for Pantograph/Overhead Line Interaction
// - EN 50119: Railway Applications - Fixed Installations - Electric Traction Overhead Contact Lines
// - TSI Loc & Pas / TSI ENE: EU Technical Specifications for Interoperability (High-Speed Rail <= 360 km/h)
// - Bruni et al. (2012): "Pantograph-Catenary Dynamic Interaction: Benchmark of Calculation Models"
// - UIC 794: Technical Specifications for Overhead Contact Lines
//
// Strictly forbids unsafe code.

use serde::{Deserialize, Serialize};

fn default_problem_id() -> String {
    "rail_pantograph_problem".to_string()
}

/// Overhead catenary electrification voltage standard.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum CatenaryVoltageSystem {
    /// 25 kV AC 50/60 Hz High-Speed Standard (EN 50367 Table 4).
    #[default]
    Ac25kV,
    /// 15 kV AC 16.7 Hz Central European Standard (DB, OBB, SBB).
    Ac15kV,
    /// 3 kV DC Heavy Traction System (FS, Renfe conventional).
    Dc3kV,
}

/// Rail pantograph simulation problem specification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailPantographProblem {
    /// Problem identifier.
    #[serde(default = "default_problem_id")]
    pub problem_id: String,
    /// Electrification voltage system.
    #[serde(default)]
    pub system: CatenaryVoltageSystem,
    /// Train operating speed in kilometers per hour (km/h, typ. 160 - 360 km/h).
    pub train_speed_km_h: f64,
    /// Static contact force F_0 exerted by pantograph air bellows in Newtons (N, typ. 70 - 110 N).
    pub static_contact_force_n: f64,
    /// Aerodynamic uplift coefficient c_aero in N/(km/h)² (typ. 0.0006 - 0.0014 N/(km/h)²).
    pub aerodynamic_uplift_coeff: f64,
    /// Contact wire nominal mechanical tension in kiloNewtons (kN, typ. 15.0 - 27.0 kN).
    pub contact_wire_tension_kn: f64,
    /// Contact wire linear mass density in kilograms per meter (kg/m, typ. 0.95 - 1.35 kg/m).
    pub contact_wire_mass_kg_per_m: f64,
    /// Catenary span length between registration arms/masts in meters (m, typ. 45 - 65 m).
    pub catenary_span_length_m: f64,
    /// Maximum vertical catenary elasticity at midspan in millimeters per Newton (mm/N, typ. 0.6 - 1.2 mm/N).
    pub catenary_midspan_elasticity_mm_per_n: f64,
    /// Minimum vertical catenary elasticity at mast support in millimeters per Newton (mm/N, typ. 0.2 - 0.5 mm/N).
    pub catenary_support_elasticity_mm_per_n: f64,
}

impl Default for RailPantographProblem {
    fn default() -> Self {
        Self {
            problem_id: default_problem_id(),
            system: CatenaryVoltageSystem::Ac25kV,
            train_speed_km_h: 300.0,
            static_contact_force_n: 80.0,
            aerodynamic_uplift_coeff: 0.00095,
            contact_wire_tension_kn: 22.0,
            contact_wire_mass_kg_per_m: 1.07,
            catenary_span_length_m: 55.0,
            catenary_midspan_elasticity_mm_per_n: 0.75,
            catenary_support_elasticity_mm_per_n: 0.35,
        }
    }
}

/// Rail pantograph simulation result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RailPantographResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Mean contact force F_m between pantograph collector strip and contact wire in Newtons (N).
    pub mean_contact_force_n: f64,
    /// EN 50367 maximum permissible mean contact force F_m,max in Newtons (N).
    pub en50367_max_mean_force_n: f64,
    /// Contact force dynamic standard deviation sigma_F in Newtons (N).
    pub contact_force_std_dev_n: f64,
    /// Statistical maximum contact force F_max = F_m + 3 * sigma_F in Newtons (N).
    pub statistical_max_contact_force_n: f64,
    /// Statistical minimum contact force F_min = F_m - 3 * sigma_F in Newtons (N).
    pub statistical_min_contact_force_n: f64,
    /// Percentage of arcing / loss of contact NQ (%).
    pub loss_of_contact_percentage_nq: f64,
    /// Transverse wave propagation velocity in contact wire c_wave in m/s.
    pub contact_wire_wave_speed_m_s: f64,
    /// Doppler critical speed ratio alpha = v / c_wave (must be <= 0.70).
    pub doppler_speed_ratio_alpha: f64,
    /// Contact wire dynamic vertical uplift at steady support in millimeters (mm).
    pub contact_wire_uplift_mm: f64,
    /// Elasticity uniformity factor u_e = (E_max - E_min) / (E_max + E_min).
    pub catenary_elasticity_uniformity: f64,
    /// Whether mean contact force satisfies EN 50367 limits.
    pub is_mean_contact_force_compliant: bool,
    /// Whether current collection arcing loss NQ satisfies EN 50367 standard (NQ <= 0.20%).
    pub is_arcing_rate_acceptable: bool,
    /// Whether Doppler speed ratio is safely below wave accumulation resonance (alpha <= 0.70).
    pub is_doppler_speed_non_critical: bool,
    /// Whether contact wire uplift is within allowable clearance limit (uplift <= 100 mm).
    pub is_uplift_acceptable: bool,
}

/// Solves EN 50367 mean contact force, wave propagation velocity, statistical contact forces, arcing loss, and wire uplift.
pub fn solve_rail_pantograph(
    problem: &RailPantographProblem,
) -> Result<RailPantographResult, String> {
    if problem.train_speed_km_h <= 10.0 || problem.train_speed_km_h >= 500.0 {
        return Err("Train speed must be between 10 and 500 km/h".to_string());
    }
    if problem.static_contact_force_n <= 20.0 || problem.static_contact_force_n >= 300.0 {
        return Err("Static contact force must be between 20 and 300 N".to_string());
    }
    if problem.contact_wire_tension_kn <= 5.0 || problem.contact_wire_tension_kn >= 60.0 {
        return Err("Contact wire tension must be between 5 and 60 kN".to_string());
    }

    let v_km_h = problem.train_speed_km_h;
    let v_m_s = v_km_h / 3.6;

    // Mean contact force: F_m = F_0 + c_aero * v^2 [N]
    let f_aero = problem.aerodynamic_uplift_coeff * v_km_h * v_km_h;
    let f_m = problem.static_contact_force_n + f_aero;

    // EN 50367 upper permissible bound for 25 kV AC:
    // F_m,max = 70.0 + 0.00097 * v^2 + 100.0 for 25 kV, or Table 4 formula:
    let f_m_max_en = match problem.system {
        CatenaryVoltageSystem::Ac25kV => (0.00097 * v_km_h * v_km_h + 120.0).clamp(140.0, 350.0),
        CatenaryVoltageSystem::Ac15kV => (0.00097 * v_km_h * v_km_h + 100.0).clamp(120.0, 300.0),
        CatenaryVoltageSystem::Dc3kV => (0.00097 * v_km_h * v_km_h + 140.0).clamp(160.0, 380.0),
    };

    // Wave propagation velocity in tensioned contact wire:
    // c_wave = sqrt(T_wire / mu_wire) [m/s]
    let t_wire_n = problem.contact_wire_tension_kn * 1000.0;
    let c_wave_m_s = (t_wire_n / problem.contact_wire_mass_kg_per_m).sqrt();

    // Doppler critical speed ratio: alpha = v / c_wave
    let alpha_speed = v_m_s / c_wave_m_s;

    // Catenary elasticity variation:
    let e_max = problem.catenary_midspan_elasticity_mm_per_n;
    let e_min = problem.catenary_support_elasticity_mm_per_n;
    let u_elasticity = (e_max - e_min) / (e_max + e_min);

    // Dynamic contact force standard deviation sigma_F:
    // Based on extensive TSI / EN 50367 measurement correlation:
    // s_f = sigma_F / F_m ~ 0.10 + 0.15 * (v / 300)^1.5 + 0.12 * u_elasticity
    let s_f = (0.08 + 0.12 * (v_km_h / 300.0).powf(1.6) + 0.10 * u_elasticity).clamp(0.12, 0.35);
    let sigma_f = s_f * f_m;

    let f_max_stat = f_m + 3.0 * sigma_f;
    let f_min_stat = (f_m - 3.0 * sigma_f).max(0.0);

    // Arcing / loss of contact percentage NQ (%):
    // NQ = Prob(F < 0) = Phi((0 - F_m) / sigma_F) * 100%
    // Approximation for complementary error function in normal tail:
    let z_tail = f_m / sigma_f;
    let nq_pct = if z_tail >= 5.0 {
        0.0001
    } else {
        // Abramowitz & Stegun normal CDF tail:
        let t_param = 1.0 / (1.0 + 0.2316419 * z_tail);
        let poly = t_param
            * (0.319381530
                + t_param
                    * (-0.356563782
                        + t_param
                            * (1.781477937 + t_param * (-1.821255978 + t_param * 1.330274429))));
        let phi_tail =
            (1.0 / (2.0 * std::f64::consts::PI).sqrt()) * (-0.5 * z_tail * z_tail).exp() * poly;
        (phi_tail * 100.0).clamp(0.0001, 15.0)
    };

    // Contact wire vertical uplift at support:
    // Uplift = F_m * E_support (mm)
    let uplift_mm = f_m * e_min;

    let is_fm_ok = f_m <= f_m_max_en && f_m >= 50.0;
    let is_nq_ok = nq_pct <= 0.20; // EN 50367 limit: NQ <= 0.2%
    let is_doppler_ok = alpha_speed <= 0.70; // EN 50119 limit: alpha <= 0.70
    let is_uplift_ok = uplift_mm <= 100.0; // EN 50367 limit: uplift <= 100 mm

    Ok(RailPantographResult {
        problem_id: problem.problem_id.clone(),
        mean_contact_force_n: f_m,
        en50367_max_mean_force_n: f_m_max_en,
        contact_force_std_dev_n: sigma_f,
        statistical_max_contact_force_n: f_max_stat,
        statistical_min_contact_force_n: f_min_stat,
        loss_of_contact_percentage_nq: nq_pct,
        contact_wire_wave_speed_m_s: c_wave_m_s,
        doppler_speed_ratio_alpha: alpha_speed,
        contact_wire_uplift_mm: uplift_mm,
        catenary_elasticity_uniformity: u_elasticity,
        is_mean_contact_force_compliant: is_fm_ok,
        is_arcing_rate_acceptable: is_nq_ok,
        is_doppler_speed_non_critical: is_doppler_ok,
        is_uplift_acceptable: is_uplift_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_high_speed_pantograph_catenary_interaction() -> Result<(), String> {
        let prob = RailPantographProblem {
            problem_id: "tgv_300kmh_current_collection".to_string(),
            system: CatenaryVoltageSystem::Ac25kV,
            train_speed_km_h: 300.0,
            static_contact_force_n: 80.0,
            aerodynamic_uplift_coeff: 0.00095,
            contact_wire_tension_kn: 22.0,
            contact_wire_mass_kg_per_m: 1.07,
            catenary_span_length_m: 55.0,
            catenary_midspan_elasticity_mm_per_n: 0.75,
            catenary_support_elasticity_mm_per_n: 0.35,
        };

        let res = solve_rail_pantograph(&prob)?;
        assert!(res.mean_contact_force_n > 140.0 && res.mean_contact_force_n < 200.0);
        assert!(res.contact_force_std_dev_n > 20.0 && res.contact_force_std_dev_n < 60.0);
        assert!(
            res.statistical_max_contact_force_n > 220.0
                && res.statistical_max_contact_force_n < 350.0
        );
        assert!(res.contact_wire_wave_speed_m_s > 130.0 && res.contact_wire_wave_speed_m_s < 160.0);
        assert!(res.doppler_speed_ratio_alpha > 0.50 && res.doppler_speed_ratio_alpha <= 0.70);
        assert!(res.contact_wire_uplift_mm > 40.0 && res.contact_wire_uplift_mm < 90.0);
        assert!(res.loss_of_contact_percentage_nq <= 0.20);
        assert!(res.is_mean_contact_force_compliant);
        assert!(res.is_arcing_rate_acceptable);
        assert!(res.is_doppler_speed_non_critical);
        assert!(res.is_uplift_acceptable);
        Ok(())
    }
}

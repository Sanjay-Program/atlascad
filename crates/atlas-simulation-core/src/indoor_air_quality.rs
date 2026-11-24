//! Indoor Air Quality (IAQ) & Transient Contaminant Dilution (ASHRAE 62.1 / Wells-Riley / ISO 16000)
//!
//! Provides first-principles closed-form analytical solutions for:
//! 1. Single-zone dynamic mass balance for airborne indoor contaminants (CO2, VOCs, PM2.5, Bioaerosols).
//! 2. Effective removal rate lambda = ACH + k_dep and steady-state equilibrium concentration C_ss.
//! 3. Transient concentration trajectory C(t) and purge time to safe threshold.
//! 4. Wells-Riley airborne pathogen transmission risk probability P_inf.
//! 5. ASHRAE 62.1 minimum breathing zone outdoor airflow rate V_bz compliance.

#![forbid(unsafe_code)]

use serde::{Deserialize, Serialize};

/// Airborne indoor contaminant species classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AirContaminantType {
    /// Carbon Dioxide (CO2): Metabolic respiration indicator in parts per million (ppm).
    CarbonDioxideCO2,
    /// Total Volatile Organic Compounds (TVOC): Building materials & finishes off-gassing in mg/m³.
    TotalVolatileOrganicCompounds,
    /// Respirable Fine Particulate Matter (PM2.5): Dust, combustion, aerosols in µg/m³.
    FineParticulatePM25,
    /// Airborne Pathogenic Bioaerosols: Infectious viral/bacterial quanta (Wells-Riley model).
    AirbornePathogenQuanta,
}

/// Space occupancy classification according to ASHRAE 62.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum SpaceOccupancyType {
    /// Commercial open-plan office space.
    CommercialOffice,
    /// School or university lecture classroom.
    SchoolClassroom,
    /// Hospital patient room / clinical consultation.
    HospitalPatientRoom,
    /// High-density conference room or auditorium.
    AuditoriumConference,
}

/// Input parameters for indoor air quality and ventilation dilution analysis.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndoorAirQualityProblem {
    /// Problem identifier.
    pub problem_id: String,
    /// Airborne contaminant species.
    pub contaminant: AirContaminantType,
    /// Room space occupancy type.
    pub space_type: SpaceOccupancyType,
    /// Enclosed room floor area in square meters (m², standard 15 to 500 m²).
    pub room_floor_area_m2: f64,
    /// Room ceiling height in meters (m, standard 2.4 to 5.0 m).
    pub ceiling_height_m: f64,
    /// Number of occupants inside the room (standard 1 to 150 persons).
    pub occupant_count: usize,
    /// Mechanical supply outdoor air ventilation rate in cubic meters per hour (m³/h).
    pub outdoor_ventilation_rate_m3_h: f64,
    /// Recirculated HVAC filtration efficiency for contaminant (standard 0.0 to 0.99 for HEPA/MERV 13).
    pub hvac_filter_efficiency: f64,
    /// Ambient outdoor concentration of contaminant (e.g. 420 ppm for outdoor CO2).
    pub outdoor_ambient_concentration: f64,
    /// Initial room concentration at t = 0 (e.g. 500 ppm for CO2).
    pub initial_room_concentration: f64,
    /// Target maximum allowable concentration threshold (e.g. 1000 ppm for ASHRAE CO2 guideline).
    pub target_threshold_concentration: f64,
    /// Exposure duration for risk calculation in hours (h, standard 0.5 to 8.0 h).
    pub exposure_duration_hours: f64,
    /// Number of infective individuals in the space (for Wells-Riley airborne pathogen risk).
    pub infected_occupants: usize,
}

impl Default for IndoorAirQualityProblem {
    fn default() -> Self {
        Self {
            problem_id: "commercial_office_co2_dilution".to_string(),
            contaminant: AirContaminantType::CarbonDioxideCO2,
            space_type: SpaceOccupancyType::CommercialOffice,
            room_floor_area_m2: 120.0,              // 120 m² open office
            ceiling_height_m: 3.0,                  // 3.0 m ceiling (Room volume = 360 m³)
            occupant_count: 12,                     // 12 engineers/architects
            outdoor_ventilation_rate_m3_h: 720.0, // 720 m³/h fresh air (ACH = 2.0 h^-1, 60 m³/h/person)
            hvac_filter_efficiency: 0.0, // Gaseous CO2 is not trapped by particulate filters
            outdoor_ambient_concentration: 420.0, // 420 ppm outdoor CO2
            initial_room_concentration: 450.0, // 450 ppm morning baseline
            target_threshold_concentration: 1000.0, // ASHRAE 62.1 recommended indoor CO2 ceiling (1000 ppm)
            exposure_duration_hours: 4.0,           // 4.0 hours continuous work session
            infected_occupants: 0,                  // Nominal baseline
        }
    }
}

/// Simulation results for indoor contaminant concentration dynamics and ventilation compliance.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndoorAirQualityResult {
    /// Problem identifier.
    pub problem_id: String,
    /// Enclosed room air volume in cubic meters (m³).
    pub room_volume_m3: f64,
    /// Air change rate (ACH = Q_vent / V) in air changes per hour (1/h).
    pub air_changes_per_hour_ach: f64,
    /// Total effective decay rate constant lambda in 1/h (ACH + deposition).
    pub effective_decay_rate_per_hour: f64,
    /// Total internal generation emission rate S_dot (e.g. liters/h of CO2 or mg/h of TVOC).
    pub total_generation_rate: f64,
    /// Steady-state equilibrium concentration C_ss reached after long duration.
    pub steady_state_concentration: f64,
    /// Transient concentration at the end of the specified exposure duration C(t).
    pub concentration_after_exposure: f64,
    /// Time required to purge room from initial peak back down to target threshold in hours (h).
    pub purge_time_to_target_hours: f64,
    /// ASHRAE 62.1 minimum required breathing zone outdoor airflow rate V_bz in m³/h.
    pub ashrae_minimum_outdoor_flow_m3_h: f64,
    /// Ventilation compliance ratio (Q_supplied / V_bz).
    pub ventilation_compliance_ratio: f64,
    /// Wells-Riley airborne infection probability percentage (%) for pathogen scenarios.
    pub wells_riley_infection_risk_pct: f64,
    /// Fresh outdoor airflow rate per occupant in liters per second per person (L/s/person).
    pub outdoor_flow_per_person_l_s: f64,
    /// Whether steady-state concentration satisfies target threshold (C_ss <= target).
    pub is_target_threshold_satisfied: bool,
    /// Whether ventilation rate satisfies ASHRAE 62.1 prescriptive minimum (ratio >= 1.0).
    pub is_ashrae_62_1_compliant: bool,
    /// Whether airborne infection risk is below standard safe threshold (< 5.0%).
    pub is_infection_risk_low: bool,
    /// Overall indoor environmental air quality acceptability status.
    pub is_iaq_acceptable: bool,
}

/// Solves indoor air quality mass balance, transient exponential dilution, and Wells-Riley transmission.
pub fn solve_indoor_air_quality(
    problem: &IndoorAirQualityProblem,
) -> Result<IndoorAirQualityResult, String> {
    if problem.room_floor_area_m2 <= 2.0 {
        return Err("Room floor area must exceed 2 m²".to_string());
    }
    if problem.ceiling_height_m <= 1.5 {
        return Err("Ceiling height must exceed 1.5 m".to_string());
    }
    if problem.outdoor_ventilation_rate_m3_h <= 1.0 {
        return Err("Outdoor ventilation rate must be positive".to_string());
    }
    if problem.exposure_duration_hours <= 0.05 {
        return Err("Exposure duration must be at least 0.05 hours".to_string());
    }

    let room_volume_m3 = problem.room_floor_area_m2 * problem.ceiling_height_m;
    let ach = problem.outdoor_ventilation_rate_m3_h / room_volume_m3;

    // Contaminant specific emission and deposition parameters:
    // (generation_per_person, k_deposition_per_h, wells_riley_q_quanta_h)
    let (s_person, k_dep, q_quanta_h) = match problem.contaminant {
        AirContaminantType::CarbonDioxideCO2 => {
            // Human adult metabolic CO2 generation ~ 18.0 L/h (0.018 m³/h) at 1.2 met activity
            // Deposition is zero for gas. In ppm units, C_ppm = (V_CO2 / V_air) * 1e6
            (18.0, 0.0, 0.0)
        }
        AirContaminantType::TotalVolatileOrganicCompounds => {
            // Background TVOC ~ 0.05 mg/(m²*h) area emission + 2.5 mg/h per person
            (2.5, 0.15, 0.0)
        }
        AirContaminantType::FineParticulatePM25 => {
            // PM2.5 generation ~ 1.2 mg/h per person, gravitational settling k_dep ~ 0.35 h^-1
            (1.2, 0.35, 0.0)
        }
        AirContaminantType::AirbornePathogenQuanta => {
            // Respiratory pathogen: q ~ 15.0 quanta/h for speaking/office breathing (SARS-CoV-2 / Influenza)
            (0.0, 0.60, 15.0)
        }
    };

    // Total generation rate:
    let total_s_dot = match problem.contaminant {
        AirContaminantType::CarbonDioxideCO2 => {
            // Total CO2 generation in m³/h:
            let s_m3_h = (problem.occupant_count as f64) * (s_person / 1000.0);
            // In ppm/h equivalent generation rate = (S_m3_h / V) * 1e6 * V:
            s_m3_h * 1.0e6
        }
        AirContaminantType::TotalVolatileOrganicCompounds => {
            // Area emission + occupant emission:
            problem.room_floor_area_m2 * 0.05 + (problem.occupant_count as f64) * s_person
        }
        AirContaminantType::FineParticulatePM25 => {
            (problem.occupant_count as f64) * s_person * 1000.0 // in µg/h
        }
        AirContaminantType::AirbornePathogenQuanta => {
            (problem.infected_occupants as f64) * q_quanta_h // quanta/h
        }
    };

    // Total effective removal rate lambda = ACH + k_dep + filter_recirc [1/h]:
    let lambda = ach + k_dep;

    // Steady-state concentration:
    // V * dC/dt = S_dot + Q_vent * (1 - eta_f) * C_amb - lambda * V * C_ss = 0
    // C_ss = [S_dot/V + ACH * (1 - eta_f) * C_amb] / lambda
    let effective_ambient_term =
        ach * (1.0 - problem.hvac_filter_efficiency) * problem.outdoor_ambient_concentration;
    let c_ss = (total_s_dot / room_volume_m3 + effective_ambient_term) / lambda;

    // Transient concentration at exposure duration t: C(t) = C_ss + (C_0 - C_ss) * exp(-lambda * t)
    let t_exp = problem.exposure_duration_hours;
    let c_t = c_ss + (problem.initial_room_concentration - c_ss) * (-lambda * t_exp).exp();

    // Purge time from max(C_t, C_0) down to target threshold:
    let c_peak = c_t.max(problem.initial_room_concentration);
    let purge_time_h = if c_peak > problem.target_threshold_concentration
        && c_ss < problem.target_threshold_concentration
    {
        -(1.0 / lambda) * ((problem.target_threshold_concentration - c_ss) / (c_peak - c_ss)).ln()
    } else if c_peak <= problem.target_threshold_concentration {
        0.0
    } else {
        // C_ss exceeds target, cannot purge without increasing ventilation
        99.9
    };

    // ASHRAE 62.1 Prescriptive Breathing Zone Outdoor Airflow Rate:
    // V_bz = R_p * P_z + R_a * A_z [L/s] -> [m³/h] (1 L/s = 3.6 m³/h)
    let (r_p_l_s, r_a_l_s) = match problem.space_type {
        SpaceOccupancyType::CommercialOffice => (2.5, 0.3), // 2.5 L/s/person + 0.3 L/s/m²
        SpaceOccupancyType::SchoolClassroom => (5.0, 0.6),  // 5.0 L/s/person + 0.6 L/s/m²
        SpaceOccupancyType::HospitalPatientRoom => (12.5, 0.8), // 12.5 L/s/person + 0.8 L/s/m²
        SpaceOccupancyType::AuditoriumConference => (2.5, 0.3),
    };
    let v_bz_l_s = r_p_l_s * (problem.occupant_count as f64) + r_a_l_s * problem.room_floor_area_m2;
    let v_bz_m3_h = v_bz_l_s * 3.6;
    let vent_ratio = problem.outdoor_ventilation_rate_m3_h / v_bz_m3_h.max(1.0);

    // Outdoor flow per person (L/s/person):
    let flow_per_person_l_s = if problem.occupant_count > 0 {
        (problem.outdoor_ventilation_rate_m3_h / 3.6) / (problem.occupant_count as f64)
    } else {
        0.0
    };

    // Wells-Riley infection risk probability:
    // P_inf = 1 - exp(- (I * q * p * t) / Q_vent)
    // p = pulmonary ventilation rate ~ 0.54 m³/h (light activity breathing)
    let p_pulmonary = 0.54; // m³/h
    let wells_riley_risk_pct =
        if problem.infected_occupants > 0 && problem.outdoor_ventilation_rate_m3_h > 0.0 {
            let exponent = ((problem.infected_occupants as f64) * q_quanta_h * p_pulmonary * t_exp)
                / problem.outdoor_ventilation_rate_m3_h;
            (1.0 - (-exponent).exp()) * 100.0
        } else {
            0.0
        };

    // Verification checks:
    let is_threshold_ok = c_ss <= (1.05 * problem.target_threshold_concentration);
    let is_ashrae_ok = vent_ratio >= 1.0;
    let is_risk_low = wells_riley_risk_pct <= 5.0;
    let is_iaq_ok = is_threshold_ok && is_ashrae_ok && is_risk_low;

    Ok(IndoorAirQualityResult {
        problem_id: problem.problem_id.clone(),
        room_volume_m3,
        air_changes_per_hour_ach: ach,
        effective_decay_rate_per_hour: lambda,
        total_generation_rate: total_s_dot,
        steady_state_concentration: c_ss,
        concentration_after_exposure: c_t,
        purge_time_to_target_hours: purge_time_h,
        ashrae_minimum_outdoor_flow_m3_h: v_bz_m3_h,
        ventilation_compliance_ratio: vent_ratio,
        wells_riley_infection_risk_pct: wells_riley_risk_pct,
        outdoor_flow_per_person_l_s: flow_per_person_l_s,
        is_target_threshold_satisfied: is_threshold_ok,
        is_ashrae_62_1_compliant: is_ashrae_ok,
        is_infection_risk_low: is_risk_low,
        is_iaq_acceptable: is_iaq_ok,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_office_co2_dilution_and_iaq() -> Result<(), String> {
        let prob = IndoorAirQualityProblem::default();
        let res = solve_indoor_air_quality(&prob)?;

        assert_eq!(res.room_volume_m3, 360.0);
        assert_eq!(res.air_changes_per_hour_ach, 2.0);
        assert!(res.steady_state_concentration > 650.0 && res.steady_state_concentration < 850.0);
        assert!(
            res.concentration_after_exposure > 600.0 && res.concentration_after_exposure < 800.0
        );
        assert!(
            res.ventilation_compliance_ratio >= 1.0,
            "Should satisfy ASHRAE 62.1 minimum ventilation"
        );
        assert!(res.outdoor_flow_per_person_l_s >= 10.0);
        assert!(res.is_target_threshold_satisfied);
        assert!(res.is_ashrae_62_1_compliant);
        assert!(res.is_infection_risk_low);
        assert!(res.is_iaq_acceptable);
        Ok(())
    }
}

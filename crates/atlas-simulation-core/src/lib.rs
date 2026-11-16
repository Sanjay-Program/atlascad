//! # atlas-simulation-core
//!
//! Standardized engineering simulation problem definitions, boundary conditions,
//! materials, solver settings, convergence telemetry, and analytical validation.

pub mod acoustics;
pub mod aerodynamics;
pub mod aeroelasticity;
pub mod automotive_braking;
pub mod battery_ev;
pub mod battery_thermal;
pub mod belt_transmission;
pub mod biomedical;
pub mod boundary_conditions;
pub mod building_physics;
pub mod cavitation;
pub mod cfd;
pub mod channel_hydraulics;
pub mod chemical;
pub mod chemical_reactor;
pub mod convergence;
pub mod daylighting;
pub mod dc_dc_converter;
pub mod digital_twin;
pub mod distillation;
pub mod elastoplasticity;
pub mod electromagnetics;
pub mod engine;
pub mod environmental;
pub mod execution;
pub mod fasteners;
pub mod fracture;
pub mod fsi;
pub mod fusion_tokamak;
pub mod gas_compression;
pub mod gas_turbine;
pub mod gear_fatigue;
pub mod gears;
pub mod generative;
pub mod geotechnical;
pub mod geotechnical_bearing;
pub mod heat_exchanger;
pub mod heat_exchanger_rating;
pub mod hvac_ducts;
pub mod hydraulic_actuator;
pub mod hydrology;
pub mod hypersonic;
pub mod induction_motor;
pub mod injection_molding;
pub mod journal_bearing;
pub mod machining_physics;
pub mod marine;
pub mod marine_propeller;
pub mod materials;
pub mod model;
pub mod molecular;
pub mod nuclear_shielding;
pub mod orbital_mechanics;
pub mod pcb_thermal;
pub mod pile_foundation;
pub mod piping_flexibility;
pub mod power_flow;
pub mod power_transformer;
pub mod pressure_vessel;
pub mod problem;
pub mod provenance;
pub mod psychrometrics;
pub mod pumps;
pub mod quarter_car;
pub mod rainflow;
pub mod refrigeration_cycle;
pub mod reinforced_concrete;
pub mod robotics;
pub mod rock_mechanics;
pub mod rocket_nozzle;
pub mod rotor_dynamics;
pub mod seismic_analysis;
pub mod shafts_keys;
pub mod sheet_metal;
pub mod ship_stability;
pub mod shock_response;
pub mod short_circuit;
pub mod signal_integrity;
pub mod slope_stability;
pub mod solar;
pub mod solar_thermal;
pub mod springs;
pub mod steam_rankine;
pub mod solver_settings;
pub mod structural;
pub mod structural_connections;
pub mod thermal_stress;
pub mod timber_fire;
pub mod transmission_line;
pub mod validation;
pub mod water_hammer;
pub mod welding;
pub mod wind_turbine;
pub mod wire_edm;
pub mod wind_blade_aeroelastic;
pub mod subsea_pipeline;
pub mod combustor_aerothermal;
pub mod bess_grid;
pub mod soil_consolidation;
pub mod hydraulic_turbine;
pub mod stent_expansion;
pub mod microwave_waveguide;
pub mod gis_dielectric;
pub mod pem_fuel_cell;
pub mod superconducting_magnet;
pub mod reentry_aerothermodynamics;
pub mod lpbf_meltpool;
pub mod phononic_bandgap;

pub mod prelude {
    pub use crate::acoustics::{
        AcousticsProblem, OutdoorNoiseProblem, OutdoorNoiseResult, solve_acoustics,
        solve_outdoor_noise_propagation, solve_outdoor_noise_sim,
    };
    pub use crate::aerodynamics::{
        NormalShockProblem, WingAeroProblem, solve_normal_shock, solve_wing_aerodynamics,
    };
    pub use crate::aeroelasticity::{AeroelasticProblem, solve_aeroelastic_viv};
    pub use crate::automotive_braking::{
        AutomotiveBrakingProblem, AutomotiveBrakingResult, RotorMaterial,
        solve_automotive_braking, solve_automotive_braking_sim,
    };
    pub use crate::battery_ev::{
        BatteryPackProblem, VehicleDynamicsProblem, solve_battery_pack, solve_vehicle_dynamics,
    };
    pub use crate::battery_thermal::{
        BatteryThermalProblem, BatteryThermalResult,
        solve_battery_thermal, solve_battery_thermal_sim,
    };
    pub use crate::belt_transmission::{
        BeltProfile, BeltTransmissionProblem, BeltTransmissionResult,
        solve_belt_transmission, solve_belt_transmission_sim,
    };
    pub use crate::biomedical::{
        HemodynamicsRequest, HemodynamicsResult, OrthopedicBoneRequest, OrthopedicBoneResult,
        solve_hemodynamics, solve_orthopedic_bone,
    };
    pub use crate::boundary_conditions::BoundaryCondition;
    pub use crate::building_physics::{
        BuildingAssemblyProblem, BuildingLayer, solve_building_u_value,
    };
    pub use crate::cavitation::{CavitationProblem, solve_cavitation_npsh};
    pub use crate::cfd::{FluidProperties, PipeFlowProblem, solve_pipe_flow};
    pub use crate::channel_hydraulics::{
        ChannelFlowRegime, ChannelHydraulicsProblem, ChannelHydraulicsResult,
        solve_channel_hydraulics, solve_channel_hydraulics_sim,
    };
    pub use crate::chemical::{
        AcidBaseEquilibriumResult, AcidBaseNature, AcidBaseProblem, ChemicalComponent,
        ChemicalSpecies, FlashProblem, ReactionKineticsProblem, ReactionKineticsResult,
        ReactionOrder, TitrationDataPoint, find_species_by_id, get_standard_species_catalog,
        solve_acid_base_titration, solve_chemical_titration_sim, solve_rachford_rice_flash,
        solve_reaction_kinetics, solve_reaction_kinetics_sim,
    };
    pub use crate::chemical_reactor::{
        ChemicalReactorProblem, ChemicalReactorResult,
        solve_chemical_reactor, solve_chemical_reactor_sim,
    };
    pub use crate::convergence::{ConvergenceMonitor, IterationTelemetry};
    pub use crate::daylighting::{
        CieSkyType, DaylightingProblem, DaylightingResult, RoomSurfaceFinishes,
        solve_cie_daylight_factor, solve_daylighting_sim,
    };
    pub use crate::digital_twin::{
        DigitalTwinProblem, StrainRosetteTelemetry, StressCycleBlock, solve_digital_twin,
    };
    pub use crate::distillation::{
        DistillationProblem, DistillationResult, FeedThermalCondition, StagePoint,
        solve_distillation_column, solve_distillation_sim,
    };
    pub use crate::elastoplasticity::{ElastoplasticProblem, solve_elastoplasticity};
    pub use crate::electromagnetics::{MicrostripProblem, solve_microstrip_line};
    pub use crate::engine::SimulationEngineAdapter;
    pub use crate::environmental::{
        AtmosphericStabilityClass, PlumeDispersionProblem, compute_dispersion_coefficients,
        solve_plume_dispersion,
    };
    pub use crate::execution::{SimulationResult, SimulationStatus};
    pub use crate::fasteners::{
        BoltPropertyClass, BoltedJointProblem, BoltedJointResult, MetricThreadSpec,
        get_standard_metric_threads, solve_bolted_joint, solve_bolted_joint_sim,
    };
    pub use crate::fracture::{
        CrackConfiguration, FractureMaterial, FractureProblem, FractureResult,
        solve_fracture_sim, solve_lefm_and_paris_growth,
    };
    pub use crate::fsi::{FsiProblem, solve_fsi_coupled};
    pub use crate::gas_compression::{
        CompressionStageData, GasCompressionProblem, GasCompressionResult, ProcessGas,
        solve_gas_compression, solve_gas_compression_sim,
    };
    pub use crate::gas_turbine::{
        AviationFuel, GasTurbineProblem, GasTurbineResult,
        solve_gas_turbine, solve_gas_turbine_sim, standard_atmosphere,
    };
    pub use crate::gear_fatigue::{
        GearFatigueProblem, GearFatigueResult,
        solve_gear_fatigue, solve_gear_fatigue_sim,
    };
    pub use crate::gears::{
        GearToothProfile, GearType, InvoluteGearPairProblem, InvoluteGearPairResult,
        solve_gear_pair_sim, solve_involute_gear_pair,
    };
    pub use crate::generative::{
        GenerativeMesh, GenerativeSolution, TopologyOptProblem, filter_sensitivities,
        reconstruct_generative_mesh, solve_topology_optimization,
    };
    pub use crate::geotechnical::{
        RetainingWallProblem, SoilProperties, solve_retaining_wall_pressures,
    };
    pub use crate::geotechnical_bearing::{
        GeotechnicalBearingProblem, GeotechnicalBearingResult, SoilType,
        solve_geotechnical_bearing, solve_geotechnical_bearing_sim,
    };
    pub use crate::heat_exchanger::{FlowArrangement, HeatExchangerProblem, solve_heat_exchanger};
    pub use crate::heat_exchanger_rating::{
        HeatExchangerRatingProblem, HeatExchangerRatingResult, TubeLayoutPattern,
        solve_heat_exchanger_rating, solve_heat_exchanger_rating_sim,
    };
    pub use crate::hvac_ducts::{
        DuctGeometry, DuctMaterial, HvacDuctProblem, HvacDuctResult,
        solve_hvac_ducts, solve_hvac_ducts_sim,
    };
    pub use crate::hydraulic_actuator::{
        HydraulicActuatorProblem, HydraulicActuatorResult, MountingCondition,
        solve_hydraulic_actuator, solve_hydraulic_actuator_sim,
    };
    pub use crate::hydrology::{
        HydrologyProblem, HydrologyResult, LandUseCategory, PipeMaterial,
        solve_hydrology_sim, solve_rational_runoff_and_pipe,
    };
    pub use crate::hypersonic::{
        AtmosphericState, HypersonicAirplaneProblem, HypersonicSimulationSummary,
        compute_standard_atmosphere, solve_hypersonic_airplane,
        avionics::{HypersonicAvionicsResult, evaluate_avionics_system},
        bom::{HypersonicBomSummary, HypersonicComponent, build_hypersonic_bom},
        cryo::{
            CryogenicFeedResult, SlushHydrogenResult, evaluate_cryogenic_system,
            evaluate_slush_hydrogen,
        },
        plasma::{
            MhdEnergyBypassResult, PlasmaSheathResult, SuperconductingMagnetResult,
            evaluate_mhd_energy_bypass, evaluate_plasma_sheath,
            evaluate_superconducting_hts_magnet,
        },
        scramjet::{ScramjetEngineResult, evaluate_scramjet_cycle},
        thermal::{
            ActiveCoolingResult, CreepFatigueResult, evaluate_active_cooling,
            evaluate_high_temp_creep,
        },
        trajectory::{
            Hypersonic6DofManeuverResult, HypersonicFlutterResult, HypersonicTrajectoryResult,
            evaluate_6dof_btt_maneuver, evaluate_hypersonic_flutter,
            evaluate_trajectory_corridor,
        },
    };
    pub use crate::induction_motor::{
        InductionMotorProblem, InductionMotorResult,
        solve_induction_motor, solve_induction_motor_sim,
    };
    pub use crate::journal_bearing::{
        JournalBearingProblem, JournalBearingResult,
        solve_journal_bearing, solve_journal_bearing_sim,
    };
    pub use crate::machining_physics::{
        KienzleMachiningProblem, KienzleMachiningResult, MachiningOperation, WorkpieceMachinability,
        solve_kienzle_machining, solve_machining_physics_sim,
    };
    pub use crate::marine::{
        AiryWaveRequest, AiryWaveResult, MarineHydrostaticsRequest, MarineHydrostaticsResult,
        solve_airy_wave, solve_hydrostatics,
    };
    pub use crate::marine_propeller::{
        MarinePropellerProblem, MarinePropellerResult, PropellerSeries,
        evaluate_wageningen_b, solve_marine_propeller, solve_marine_propeller_sim,
    };
    pub use crate::materials::SimulationMaterial;
    pub use crate::molecular::{
        Atom, LennardJonesParam, MolecularDynamicsProblem, run_molecular_dynamics,
    };
    pub use crate::nuclear_shielding::{
        RadiationShieldingProblem, ShieldLayer, solve_radiation_shielding,
    };
    pub use crate::pcb_thermal::{
        CopperWeightOz, PcbThermalProblem, PcbThermalResult, ThermalViaSpec, TraceLayer,
        solve_pcb_thermal, solve_pcb_thermal_sim,
    };
    pub use crate::pile_foundation::{
        PileFoundationProblem, PileFoundationResult, SoilLayerSpec, SoilLayerType,
        solve_pile_foundation, solve_pile_foundation_sim,
    };
    pub use crate::piping_flexibility::{
        PipingFlexibilityProblem, PipingFlexibilityResult,
        solve_piping_flexibility, solve_piping_flexibility_sim,
    };
    pub use crate::power_flow::{
        BusType, ComplexNumber, PowerFlowProblem, TransmissionLine, build_bus_admittance_matrix,
        solve_power_flow, y_bus_frobenius_norm,
    };
    pub use crate::power_transformer::{
        PowerTransformerProblem, PowerTransformerResult, TransformerCoolingClass,
        solve_power_transformer, solve_power_transformer_sim,
    };
    pub use crate::pressure_vessel::{
        HeadType, JointEfficiencyType, PressureVesselProblem, PressureVesselResult,
        VesselMaterial, solve_asme_pressure_vessel, solve_pressure_vessel_sim,
    };
    pub use crate::problem::{AnalysisType, SimulationProblem};
    pub use crate::provenance::SimulationProvenance;
    pub use crate::psychrometrics::{
        PsychrometricProblem, PsychrometricResult, saturation_vapor_pressure_pa,
        solve_psychrometric_state, solve_psychrometrics_sim,
    };
    pub use crate::pumps::{
        PumpFluid, PumpProblem, PumpResult, solve_centrifugal_pump, solve_pump_sim,
    };
    pub use crate::quarter_car::{
        QuarterCarProblem, QuarterCarResult, RoadRoughnessClass,
        solve_quarter_car, solve_quarter_car_sim,
    };
    pub use crate::rainflow::{
        RainflowCycle, RainflowProblem, count_rainflow_cycles, solve_rainflow_fatigue,
    };
    pub use crate::refrigeration_cycle::{
        RefrigerantType, RefrigerationCycleProblem, RefrigerationCycleResult,
        solve_refrigeration_cycle, solve_refrigeration_cycle_sim,
    };
    pub use crate::reinforced_concrete::{
        ConcreteGrade, ReinforcedConcreteProblem, ReinforcedConcreteResult,
        solve_reinforced_concrete, solve_reinforced_concrete_sim,
    };
    pub use crate::robotics::{
        DhLink, IkResult, QuinticTrajectoryProblem, RobotKinematicsProblem,
        compute_forward_transforms, compute_geometric_jacobian, compute_manipulability,
        solve_dls_inverse_kinematics, solve_quintic_trajectory, solve_robot_kinematics,
    };
    pub use crate::rock_mechanics::{
        BlastVibrationProblem, HoekBrownProblem, RmrClassificationProblem, solve_blast_vibration,
        solve_hoek_brown, solve_rmr_classification,
    };
    pub use crate::rocket_nozzle::{
        NozzleExpansionRegime, RocketNozzleProblem, RocketNozzleResult,
        solve_rocket_nozzle, solve_rocket_nozzle_sim,
    };
    pub use crate::rotor_dynamics::{
        BearingType, RotorDynamicsProblem, RotorDynamicsResult, solve_rotor_dynamics,
        solve_rotor_dynamics_sim,
    };
    pub use crate::shafts_keys::{
        Din6885KeyDimensions, HubMaterial, ShaftKeyProblem, ShaftKeyResult, ShaftMaterial,
        lookup_din6885_key, solve_shaft_and_key, solve_shaft_keys_sim,
    };
    pub use crate::sheet_metal::{
        SheetMetalBend, SheetMetalMaterial, SheetMetalPartProblem, SheetMetalUnfoldResult,
        solve_sheet_metal_sim, solve_sheet_metal_unfold,
    };
    pub use crate::ship_stability::{
        ShipStabilityProblem, ShipStabilityResult,
        solve_ship_stability, solve_ship_stability_sim,
    };
    pub use crate::shock_response::{ShockResponseProblem, solve_shock_response_spectrum};
    pub use crate::short_circuit::{
        ConductorMaterial, ShortCircuitProblem, ShortCircuitResult, solve_iec60909_short_circuit,
        solve_short_circuit_sim,
    };
    pub use crate::signal_integrity::{PcbTransmissionLineProblem, solve_pcb_signal_integrity};
    pub use crate::slope_stability::{
        SlopeStabilityProblem, SlopeStabilityResult,
        solve_slope_stability, solve_slope_stability_sim,
    };
    pub use crate::solar::{SolarPvProblem, solve_solar_pv};
    pub use crate::solar_thermal::{
        CollectorType, SolarThermalProblem, SolarThermalResult,
        solve_solar_thermal, solve_solar_thermal_sim,
    };
    pub use crate::solver_settings::SolverSettings;
    pub use crate::springs::{
        HelicalSpringProblem, HelicalSpringResult, SpringEndType, SpringMaterial,
        solve_helical_compression_spring, solve_helical_spring_sim,
    };
    pub use crate::steam_rankine::{
        SteamRankineProblem, SteamRankineResult,
        solve_steam_rankine, solve_steam_rankine_sim,
    };
    pub use crate::structural::{
        BucklingProblem, ColumnEndCondition, CompositeFailureProblem, CompositeLaminateProblem,
        FatigueProblem, LaminaStrengthLimits, OrthotropicLamina, solve_column_buckling,
        solve_composite_failure, solve_composite_laminate, solve_fatigue_safety,
    };
    pub use crate::structural_connections::{
        BoltDiameter, BoltGrade, ConnectionProblem, ConnectionResult, StructuralSteelGrade,
        solve_structural_connection, solve_structural_connection_sim,
    };
    pub use crate::thermal_stress::{
        BimetalStripProblem, ThermalStressProblem, solve_bimetal_deflection, solve_thermal_stress,
    };
    pub use crate::timber_fire::{
        FireExposureSides, TimberFireProblem, TimberFireResult, TimberGrade,
        iso834_temperature_c, solve_timber_fire, solve_timber_fire_sim,
    };
    pub use crate::transmission_line::{
        TransmissionLineProblem, TransmissionLineResult,
        solve_transmission_line, solve_transmission_line_sim,
    };
    pub use crate::validation::{AnalyticalComparison, ValidationStatus};
    pub use crate::water_hammer::{WaterHammerProblem, solve_water_hammer};
    pub use crate::welding::{
        HydrogenLevel, SteelComposition, WeldingProblem, WeldingProcess, WeldingResult,
        solve_welding_metallurgy, solve_welding_sim,
    };
    pub use crate::wind_turbine::{
        WindTurbineProblem, WindTurbineResult,
        solve_wind_turbine, solve_wind_turbine_sim,
    };
    pub use crate::dc_dc_converter::{
        DcDcConverterProblem, DcDcConverterResult,
        solve_dc_dc_converter, solve_dc_dc_converter_sim,
    };
    pub use crate::fusion_tokamak::{
        FusionTokamakProblem, FusionTokamakResult,
        solve_fusion_tokamak, solve_fusion_tokamak_sim,
    };
    pub use crate::injection_molding::{
        InjectionMoldingProblem, InjectionMoldingResult, PolymerResin,
        solve_injection_molding, solve_injection_molding_sim,
    };
    pub use crate::orbital_mechanics::{
        OrbitalMechanicsProblem, OrbitalMechanicsResult,
        solve_orbital_mechanics, solve_orbital_mechanics_sim,
    };
    pub use crate::seismic_analysis::{
        SeismicAnalysisProblem, SeismicAnalysisResult, SeismicFramingSystem,
        SeismicSiteClass, StoryForceRecord, solve_seismic_analysis, solve_seismic_analysis_sim,
    };
    pub use crate::wire_edm::{
        EdmWorkpieceMaterial, WireEdmProblem, WireEdmResult,
        solve_wire_edm, solve_wire_edm_sim,
    };
    pub use crate::wind_blade_aeroelastic::{
        WindBladeProblem, WindBladeResult,
        solve_wind_blade_aeroelastic, solve_wind_blade_aeroelastic_sim,
    };
    pub use crate::subsea_pipeline::{
        SubseaPipelineProblem, SubseaPipelineResult,
        solve_subsea_pipeline, solve_subsea_pipeline_sim,
    };
    pub use crate::combustor_aerothermal::{
        CombustorProblem, CombustorResult,
        solve_combustor, solve_combustor_sim,
    };
    pub use crate::bess_grid::{
        BessChemistry, BessGridProblem, BessGridResult,
        solve_bess_grid, solve_bess_grid_sim,
    };
    pub use crate::soil_consolidation::{
        DrainageCondition, SoilConsolidationProblem, SoilConsolidationResult,
        solve_soil_consolidation, solve_soil_consolidation_sim,
    };
    pub use crate::hydraulic_turbine::{
        HydraulicTurbineProblem, HydraulicTurbineResult, HydraulicTurbineType,
        solve_hydraulic_turbine, solve_hydraulic_turbine_sim,
    };
    pub use crate::stent_expansion::{
        StentExpansionProblem, StentExpansionResult, StentMaterial,
        solve_stent_expansion, solve_stent_expansion_sim,
    };
    pub use crate::microwave_waveguide::{
        MicrowaveWaveguideProblem, MicrowaveWaveguideResult,
        solve_microwave_waveguide, solve_microwave_waveguide_sim,
    };
    pub use crate::gis_dielectric::{
        GisDielectricProblem, GisDielectricResult, GisInsulatingGas,
        solve_gis_dielectric, solve_gis_dielectric_sim,
    };
    pub use crate::pem_fuel_cell::{
        PemFuelCellProblem, PemFuelCellResult,
        solve_pem_fuel_cell, solve_pem_fuel_cell_sim,
    };
    pub use crate::superconducting_magnet::{
        CryoMagnetProblem, CryoMagnetResult, SuperconductorMaterial,
        solve_cryo_magnet, solve_cryo_magnet_sim,
    };
    pub use crate::reentry_aerothermodynamics::{
        ReentryProblem, ReentryResult,
        solve_reentry_aerothermodynamics, solve_reentry_aerothermodynamics_sim,
    };
    pub use crate::lpbf_meltpool::{
        LpbfPowderMaterial, LpbfProblem, LpbfProcessRegime, LpbfResult,
        solve_lpbf_meltpool, solve_lpbf_meltpool_sim,
    };
    pub use crate::phononic_bandgap::{
        PhononicCrystalProblem, PhononicCrystalResult, PhononicLayer,
        solve_phononic_bandgap, solve_phononic_bandgap_sim,
    };
}

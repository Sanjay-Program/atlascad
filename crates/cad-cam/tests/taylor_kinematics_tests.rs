//! Tests for Taylor tool life physics and kinematic machining cycle time estimation.

use cad_cam::{
    CuttingCondition, KinematicLimits, Machine, MachineEnvelope, Material, Operation, Setup,
    SetupSheet, Stock, Tool, ToolGeometry, ToolId, Toolpath, ToolpathId, ToolpathPoint,
    WorkCoordinate, generate,
};

type Failure = Box<dyn std::error::Error>;

#[test]
fn taylor_tool_life_and_wear_estimates() {
    for &material in &Material::ALL {
        let params = material.taylor_parameters();
        assert!(
            params.n > 0.15 && params.n < 0.5,
            "reasonable Taylor exponent for {material:?}"
        );
        assert!(
            params.c >= 150.0 && params.c <= 1500.0,
            "reasonable cutting speed constant for {material:?}"
        );

        let data = material.cutting_data();
        let estimate = material.estimate_tool_wear(data.surface_speed_m_min, 10.0);

        assert!(
            estimate.tool_life_minutes > 1.0,
            "tool life should be reasonable"
        );
        assert!(
            estimate.wear_fraction > 0.0,
            "wear fraction should be positive"
        );
        assert_eq!(estimate.wear_percent, estimate.wear_fraction * 100.0);
        assert!(
            (estimate.tool_life_minutes
                - estimate.cutting_time_minutes
                - estimate.remaining_life_minutes)
                .abs()
                < 1.0e-6
        );
    }

    // Specific analytical check on aluminium:
    // V_c = 250 m/min, n = 0.35, C = 800 m/min
    // T = (800 / 250)^(1 / 0.35) = 3.2^(2.85714) = 27.751 min
    let alu_est = Material::Aluminium.estimate_tool_wear(250.0, 13.8757);
    assert!((alu_est.tool_life_minutes - 27.751).abs() < 0.1);
    assert!((alu_est.wear_fraction - 0.50).abs() < 0.01);
    assert!((alu_est.wear_percent - 50.0).abs() < 1.0);
}

#[test]
fn kinematic_cycle_time_trapezoidal_and_triangular_profiles() -> Result<(), Failure> {
    let limits = KinematicLimits {
        cutting_acceleration_mm_s2: 1000.0,
        rapid_acceleration_mm_s2: 1000.0,
        rapid_feed_mm_min: 6000.0, // 100 mm/s
        spindle_dwell_seconds: 1.5,
        tool_change_seconds: 5.0,
    };

    // Long move (trapezoidal): feed = 1200 mm/min (20 mm/s), L = 100 mm
    // d_acc = v^2 / a = 400 / 1000 = 0.4 mm << 100 mm
    // t_motion = (v / a) + (L / v) = (20 / 1000) + (100 / 20) = 0.02 + 5.0 = 5.02 s
    let tp_long = Toolpath::new(
        ToolpathId::new(1).ok_or("valid toolpath id")?,
        ToolId::new(1).ok_or("valid tool id")?,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 0.0]),
            ToolpathPoint::feed([100.0, 0.0, 0.0], 1200.0),
        ],
        5000.0,
        10.0,
    );
    let cycle_long = tp_long.kinematic_cycle_time(&limits);
    assert!((cycle_long.cutting_time_seconds - 5.02).abs() < 1.0e-3);
    assert_eq!(cycle_long.dwell_time_seconds, 1.5);
    assert!((cycle_long.total_seconds - 6.52).abs() < 1.0e-3);

    // Short move (triangular): feed = 6000 mm/min (100 mm/s), L = 0.5 mm
    // d_acc = 10000 / 1000 = 10 mm > 0.5 mm -> triangular profile
    // t_motion = 2 * sqrt(L / a) = 2 * sqrt(0.5 / 1000) = 2 * sqrt(0.0005) = 2 * 0.0223606 = 0.04472 s
    let tp_short = Toolpath::new(
        ToolpathId::new(2).ok_or("valid toolpath id")?,
        ToolId::new(1).ok_or("valid tool id")?,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 0.0]),
            ToolpathPoint::rapid([0.5, 0.0, 0.0]),
        ],
        5000.0,
        10.0,
    );
    let cycle_short = tp_short.kinematic_cycle_time(&limits);
    assert!((cycle_short.rapid_time_seconds - 0.04472).abs() < 1.0e-4);
    Ok(())
}

#[test]
fn setup_sheet_renders_kinematics_and_tool_wear() -> Result<(), Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [50.0, 50.0, 20.0])?;
    let setup = Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        25.0,
    )?;

    let tool = Tool::new(
        ToolId::new(1).ok_or("valid tool id")?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?;

    let facing = generate(
        &Operation::Facing {
            depth_mm: 1.0,
            stepover_mm: 3.0,
        },
        &setup,
        &tool,
        ToolpathId::new(1).ok_or("valid toolpath id")?,
    )?;

    let mut sheet = SetupSheet::new("production plate", &setup).with_material(Material::Aluminium);
    sheet.add_operation("face milling", &tool, &facing);

    let rendered = sheet.render();
    assert!(rendered.contains("## Kinematics & Tool Wear"));
    assert!(rendered.contains("Workpiece material: **aluminium 6061**"));
    assert!(rendered.contains("Estimated Kinematic Cycle Time:"));
    assert!(rendered.contains("Taylor Life (min)"));
    assert!(rendered.contains("Wear Fraction"));
    assert!(rendered.contains("| T1 |"));

    let cycle = sheet.kinematic_cycle_time(&KinematicLimits::default());
    assert!(cycle.total_seconds > 0.0);
    assert!(cycle.cutting_time_seconds > 0.0);

    let wear_list = sheet.tool_wear_estimates(Material::Aluminium);
    assert_eq!(wear_list.len(), 1);
    let (t_id, wear) = &wear_list[0];
    assert_eq!(*t_id, tool.id);
    assert!(wear.tool_life_minutes > 0.0);
    assert!(wear.wear_fraction >= 0.0);

    Ok(())
}

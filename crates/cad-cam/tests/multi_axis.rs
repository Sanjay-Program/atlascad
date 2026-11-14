use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, Operation, PostFormat, Postprocessor, Setup, Stock,
    Tool, ToolGeometry, ToolId, ToolpathId, WorkCoordinate, generate,
};

#[test]
fn rotary_4axis_toolpath_and_kinematics() -> Result<(), Box<dyn std::error::Error>> {
    let stock = Stock::new([0.0, -30.0, -30.0], [100.0, 30.0, 30.0])?;
    let setup = Setup::new(
        Machine::Milling4Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        40.0,
    )?;

    let tool = Tool::new(
        ToolId::new(1).ok_or("valid id")?,
        "4-Axis Ball Nose",
        ToolGeometry::BallEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?;

    let op = Operation::Rotary4Axis {
        cylinder_radius_mm: 20.0,
        axial_length_mm: 50.0,
        rotary_step_deg: 30.0,
        depth_mm: 2.0,
    };

    let path_id = ToolpathId::new(101).ok_or("valid id")?;
    let toolpath = generate(&op, &setup, &tool, path_id)?;

    assert!(toolpath.points.len() > 10);
    assert!(toolpath.cutting_length_mm() > 100.0);

    // Verify tool orientation vectors exist on cutting moves
    let oriented_points: Vec<_> = toolpath
        .points
        .iter()
        .filter(|p| p.orientation.is_some())
        .collect();
    assert!(!oriented_points.is_empty());

    let post = Postprocessor::new(PostFormat::LinuxCnc, "ROTARY_4AXIS_TEST");
    let gcode = post.emit(&toolpath, &setup)?;
    assert!(gcode.contains("X"));
    assert!(gcode.contains("Y"));
    assert!(gcode.contains("Z"));
    assert!(gcode.contains("M3"));

    Ok(())
}

#[test]
fn multiaxis_5_simultaneous_toolpath_and_gcode_emission() -> Result<(), Box<dyn std::error::Error>>
{
    let stock = Stock::new([0.0, 0.0, 0.0], [100.0, 100.0, 50.0])?;
    let setup = Setup::new(
        Machine::Milling5Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        60.0,
    )?;

    let tool = Tool::new(
        ToolId::new(2).ok_or("valid id")?,
        "5-Axis Endmill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 8.0,
            flute_length_mm: 25.0,
        },
        CuttingCondition::conservative_default(),
        2.5,
    )?;

    let profile = vec![
        [10.0, 10.0, 48.0],
        [30.0, 10.0, 46.0],
        [50.0, 30.0, 45.0],
        [70.0, 50.0, 44.0],
    ];
    let orientations = vec![
        [0.0, 0.1, 0.995],
        [0.05, 0.1, 0.993],
        [0.1, 0.05, 0.993],
        [0.15, 0.0, 0.988],
    ];

    let op = Operation::MultiAxis5 {
        profile,
        tool_orientations: orientations,
        depth_mm: 2.0,
    };

    let path_id = ToolpathId::new(102).ok_or("valid id")?;
    let toolpath = generate(&op, &setup, &tool, path_id)?;

    assert_eq!(toolpath.points.len(), 6); // 1 rapid, 1 plunge, 3 feeds, 1 retract feed

    let post = Postprocessor::new(PostFormat::LinuxCnc, "MULTIAXIS_5_TEST");
    let gcode = post.emit(&toolpath, &setup)?;

    // Emitted G-code should contain 5-axis (I, J, K) orientation vector coordinates
    assert!(gcode.contains("I"));
    assert!(gcode.contains("J"));
    assert!(gcode.contains("K"));

    Ok(())
}

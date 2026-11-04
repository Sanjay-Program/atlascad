//! 3D finishing: a ball nose cutter follows the sampled surface one radius above it.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, MoveKind, Operation, Setup, Stock, Tool,
    ToolGeometry, ToolId, Toolpath, ToolpathId, WorkCoordinate, generate,
};

type Failure = Box<dyn std::error::Error>;

fn id(raw: u64) -> Result<ToolId, Failure> {
    ToolId::new(raw).ok_or_else(|| "tool ids start at one".into())
}

fn path_id(raw: u64) -> Result<ToolpathId, Failure> {
    ToolpathId::new(raw).ok_or_else(|| "toolpath ids start at one".into())
}

fn setup(clearance: f64) -> Result<Setup, Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        clearance,
    )?)
}

fn ball_tool() -> Result<Tool, Failure> {
    Ok(Tool::new(
        id(1)?,
        "6 mm ball end mill",
        ToolGeometry::BallEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?)
}

fn flat_tool() -> Result<Tool, Failure> {
    Ok(Tool::new(
        id(1)?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?)
}

/// A 20 x 20 x 12 mm block at the origin.
fn block() -> Result<Stock, Failure> {
    Ok(Stock::new([0.0, 0.0, 0.0], [20.0, 20.0, 12.0])?)
}

#[test]
fn finishing_follows_the_part_one_ball_radius_above_it() -> Result<(), Failure> {
    let toolpath = generate(
        &Operation::Finish3D {
            part: block()?.to_mesh(),
            stepover_mm: 4.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        &setup(25.0)?,
        &ball_tool()?,
        path_id(1)?,
    )?;
    toolpath.validate(&setup(25.0)?)?;

    // The block top is at 12 mm and the ball radius is 3 mm, so every finishing cut is at z = 15.
    // Retracts are feed moves to the clearance plane; the surface cuts are the ones below it.
    let cuts: Vec<&cad_cam::ToolpathPoint> = toolpath
        .points
        .iter()
        .filter(|point| point.kind != MoveKind::Rapid && point.position[2] < 20.0)
        .collect();
    assert!(!cuts.is_empty());
    assert!(
        cuts.iter()
            .all(|point| (point.position[2] - 15.0).abs() < 1.0e-6),
        "a finishing cut left the one-radius offset"
    );
    assert!(
        cuts.iter()
            .all(|point| point.position[0] <= 20.0 + 1.0e-6 && point.position[1] <= 20.0 + 1.0e-6),
        "finishing cut outside the part footprint"
    );
    Ok(())
}

#[test]
fn a_stock_allowance_lifts_the_finishing_surface() -> Result<(), Failure> {
    let toolpath = generate(
        &Operation::Finish3D {
            part: block()?.to_mesh(),
            stepover_mm: 4.0,
            stock_to_leave_mm: 0.4,
            resolution_mm: 2.0,
        },
        &setup(25.0)?,
        &ball_tool()?,
        path_id(1)?,
    )?;
    let cut_z = toolpath
        .points
        .iter()
        .filter(|point| point.kind != MoveKind::Rapid)
        .map(|point| point.position[2])
        .fold(f64::INFINITY, f64::min);
    assert!((cut_z - 15.4).abs() < 1.0e-6, "cut at {cut_z}, expected 15.4");
    Ok(())
}

#[test]
fn finishing_requires_a_ball_nose_cutter() -> Result<(), Failure> {
    let error = generate(
        &Operation::Finish3D {
            part: block()?.to_mesh(),
            stepover_mm: 4.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        &setup(25.0)?,
        &flat_tool()?,
        path_id(1)?,
    )
    .err();
    assert!(error.is_some(), "a flat end mill must be refused");
    Ok(())
}

#[test]
fn finishing_needs_headroom_under_the_clearance_plane() -> Result<(), Failure> {
    // A part top at 19 mm plus the 3 mm ball radius is 22 mm, above a 21 mm clearance plane.
    let tall = Stock::new([0.0, 0.0, 0.0], [20.0, 20.0, 19.0])?;
    let error = generate(
        &Operation::Finish3D {
            part: tall.to_mesh(),
            stepover_mm: 4.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        &setup(21.0)?,
        &ball_tool()?,
        path_id(1)?,
    )
    .err();
    assert!(
        error.is_some(),
        "the finishing surface must fit below the clearance plane"
    );
    Ok(())
}

#[test]
fn a_toolpath_with_no_surface_to_follow_is_refused() -> Result<(), Failure> {
    // A part outside the stock footprint leaves the height field empty.
    let outside = Stock::new([100.0, 100.0, 0.0], [110.0, 110.0, 12.0])?;
    let error = generate(
        &Operation::Finish3D {
            part: outside.to_mesh(),
            stepover_mm: 4.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        &setup(25.0)?,
        &ball_tool()?,
        path_id(1)?,
    )
    .err();
    assert!(error.is_some(), "nothing to finish must be refused");
    Ok(())
}

#[test]
fn a_finished_toolpath_is_still_validated() -> Result<(), Failure> {
    let setup = setup(25.0)?;
    let toolpath: Toolpath = generate(
        &Operation::Finish3D {
            part: block()?.to_mesh(),
            stepover_mm: 4.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        &setup,
        &ball_tool()?,
        path_id(1)?,
    )?;
    toolpath.validate(&setup)?;
    Ok(())
}

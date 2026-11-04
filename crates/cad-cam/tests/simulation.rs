//! Material removal: a carving is measured against arithmetic, not against a picture.
//!
//! Facing a 40 x 30 x 20 mm block 2 mm deep removes exactly 2 400 mm³, so the preview must report
//! that within the grid's discretisation.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, MaterialRemoval, Operation, Setup, Stock, Tool,
    ToolGeometry, ToolId, Toolpath, ToolpathId, ToolpathPoint, WorkCoordinate, generate,
};

type Failure = Box<dyn std::error::Error>;

fn stock() -> Result<Stock, Failure> {
    Ok(Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?)
}

fn setup() -> Result<Setup, Failure> {
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock()?,
        WorkCoordinate::standard(),
        25.0,
    )?)
}

fn end_mill() -> Result<Tool, Failure> {
    Ok(Tool::new(
        ToolId::new(1).ok_or("tool ids start at one")?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?)
}

#[test]
fn facing_removes_the_arithmetic_volume() -> Result<(), Failure> {
    let setup = setup()?;
    let tool = end_mill()?;
    let toolpath = generate(
        &Operation::Facing {
            depth_mm: 2.0,
            stepover_mm: 4.0,
        },
        &setup,
        &tool,
        ToolpathId::new(1).ok_or("toolpath ids start at one")?,
    )?;

    let mut removal = MaterialRemoval::new(stock()?, 1.0)?;
    removal.carve(&toolpath, &tool);

    let expected = 40.0 * 30.0 * 2.0;
    let measured = removal.removed_volume_mm3();
    assert!(
        (measured - expected).abs() / expected < 0.05,
        "facing removed {measured} mm3, expected about {expected} mm3"
    );
    assert!((removal.max_cut_depth_mm() - 2.0).abs() < 1.0e-6);
    assert!((removal.height_at(20.0, 15.0) - 18.0).abs() < 1.0e-6);
    assert!(!removal.render().is_empty());
    Ok(())
}

#[test]
fn a_pocket_carves_only_its_own_area() -> Result<(), Failure> {
    let setup = setup()?;
    let tool = end_mill()?;
    let toolpath = generate(
        &Operation::Pocket {
            region: [[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]],
            depth_mm: 5.0,
            stepdown_mm: 2.0,
            stepover_mm: 4.0,
        },
        &setup,
        &tool,
        ToolpathId::new(1).ok_or("toolpath ids start at one")?,
    )?;

    let mut removal = MaterialRemoval::new(stock()?, 1.0)?;
    removal.carve(&toolpath, &tool);

    // The pocket is 20 x 10 mm and 5 mm deep: 1 000 mm³. The corners stay rounded because the
    // tool centre follows the radius-shrunk loop, so the corners are short of the theoretical
    // volume but nothing outside the region may be touched.
    let measured = removal.removed_volume_mm3();
    assert!(
        measured > 900.0 && measured < 1_100.0,
        "pocket removal {measured} mm3 is not in the plausible band"
    );
    assert!(
        removal.height_at(20.0, 15.0) < 16.0,
        "the pocket centre should be cut to the pocket floor"
    );
    assert!(
        (removal.height_at(2.0, 2.0) - 20.0).abs() < 1.0e-6,
        "material outside the pocket must be untouched"
    );
    Ok(())
}

#[test]
fn rapids_remove_nothing() -> Result<(), Failure> {
    let mut removal = MaterialRemoval::new(stock()?, 1.0)?;
    let path = Toolpath::new(
        ToolpathId::new(1).ok_or("toolpath ids start at one")?,
        ToolId::new(1).ok_or("tool ids start at one")?,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::rapid([30.0, 20.0, 25.0]),
        ],
        12_000.0,
        25.0,
    );
    removal.carve(&path, &end_mill()?);
    assert_eq!(removal.removed_volume_mm3(), 0.0);
    assert_eq!(removal.max_cut_depth_mm(), 0.0);
    Ok(())
}

#[test]
fn the_preview_reports_its_fraction_and_renders_the_stock() -> Result<(), Failure> {
    let mut removal = MaterialRemoval::new(stock()?, 2.0)?;
    let tool = end_mill()?;
    let path = generate(
        &Operation::Facing {
            depth_mm: 20.0,
            stepover_mm: 4.0,
        },
        &setup()?,
        &tool,
        ToolpathId::new(1).ok_or("toolpath ids start at one")?,
    )?;
    removal.carve(&path, &tool);
    // A 20 mm facing pass removes the whole block, within a grid cell of its boundary.
    assert!(removal.removal_fraction() > 0.9);
    let rendered = removal.render();
    assert!(rendered.contains("removed"));
    assert!(rendered.contains('#'));
    Ok(())
}

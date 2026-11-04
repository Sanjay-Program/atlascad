//! Holder clearance: the holder must never enter material the program has not removed yet.

use cad_cam::{
    CuttingCondition, Holder, Machine, MachineEnvelope, Operation, Setup, SimulationError, Stock,
    Tool, ToolGeometry, ToolId, Toolpath, ToolpathId, ToolpathPoint, WorkCoordinate,
    check_holder_clearance, generate,
};

type Failure = Box<dyn std::error::Error>;

fn id(raw: u64) -> Result<ToolId, Failure> {
    ToolId::new(raw).ok_or_else(|| "tool ids start at one".into())
}

fn path_id(raw: u64) -> Result<ToolpathId, Failure> {
    ToolpathId::new(raw).ok_or_else(|| "toolpath ids start at one".into())
}

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

fn tool(flute: f64, holder: Option<(f64, f64)>) -> Result<Tool, Failure> {
    let tool = Tool::new(
        id(1)?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: flute,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?;
    match holder {
        Some((diameter, length)) => Ok(tool.with_holder(Holder::new(diameter, length)?)?),
        None => Ok(tool),
    }
}

#[test]
fn a_short_tool_with_a_wide_holder_entering_stock_is_refused() -> Result<(), Failure> {
    let path = Toolpath::new(
        path_id(1)?,
        id(1)?,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::plunge([10.0, 10.0, 1.0], 200.0),
        ],
        12_000.0,
        25.0,
    );
    // The 8 mm flutes put the holder face at 9 mm at full depth. The plunge is sampled too, so the
    // check fires on the way down the moment the holder face reaches the 20 mm stock top.
    match check_holder_clearance(&path, &tool(8.0, Some((20.0, 40.0)))?, stock()?, 1.0) {
        Err(SimulationError::HolderCollision {
            index,
            holder_bottom,
            material_top,
        }) => {
            assert_eq!(index, 1);
            assert!(
                (18.0..20.0).contains(&holder_bottom),
                "the collision must fire as the holder reaches the stock top, got {holder_bottom}"
            );
            assert!((material_top - 20.0).abs() < 1.0e-6);
        }
        other => return Err(format!("expected a holder collision, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn long_flutes_keep_the_holder_clear() -> Result<(), Failure> {
    let path = Toolpath::new(
        path_id(1)?,
        id(1)?,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::plunge([10.0, 10.0, 1.0], 200.0),
        ],
        12_000.0,
        25.0,
    );
    // 1 mm + 20 mm of flutes puts the holder at 21 mm, above the 20 mm stock top.
    check_holder_clearance(&path, &tool(20.0, Some((20.0, 40.0)))?, stock()?, 1.0)?;
    Ok(())
}

#[test]
fn a_tool_without_a_holder_skips_the_check() -> Result<(), Failure> {
    let path = Toolpath::new(
        path_id(1)?,
        id(1)?,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::plunge([10.0, 10.0, 1.0], 200.0),
        ],
        12_000.0,
        25.0,
    );
    check_holder_clearance(&path, &tool(8.0, None)?, stock()?, 1.0)?;
    Ok(())
}

#[test]
fn a_holder_narrower_than_the_tool_is_refused() -> Result<(), Failure> {
    let narrow = Holder::new(4.0, 40.0)?;
    let error = Tool::new(
        id(1)?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?
    .with_holder(narrow)
    .err();
    assert!(error.is_some(), "a 4 mm holder cannot hold a 6 mm tool");
    Ok(())
}

#[test]
fn a_deep_narrow_slot_collides_with_a_wide_holder() -> Result<(), Failure> {
    let setup = setup()?;
    let path = generate(
        &Operation::Slot {
            start: [10.0, 10.0],
            end: [30.0, 10.0],
            depth_mm: 19.0,
            stepdown_mm: 2.0,
        },
        &setup,
        &tool(8.0, Some((20.0, 40.0)))?,
        path_id(1)?,
    )?;
    // The 6 mm slot itself is cleared, but the 20 mm holder above it is not: the stock beside the
    // slot still stands at 20 mm when the holder reaches 18 mm.
    let error = check_holder_clearance(&path, &tool(8.0, Some((20.0, 40.0)))?, stock()?, 1.0).err();
    assert!(
        matches!(error, Some(SimulationError::HolderCollision { .. })),
        "a 19 mm slot with 8 mm flutes must collide, got {error:?}"
    );
    Ok(())
}

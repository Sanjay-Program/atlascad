//! Wall finishing on a block that stands 10 mm inside the stock.
//!
//! Roughing leaves one tool radius (3 mm) of stock on every wall. Wall finishing must remove that
//! shell without gouging the part, level by level, no deeper than the tool's maximum depth of cut.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, MaterialRemoval, Operation, Setup, Stock, Tool,
    ToolGeometry, ToolId, Toolpath, ToolpathError, ToolpathId, WorkCoordinate, generate,
};
use cad_contract::mesh::Mesh;

type Failure = Box<dyn std::error::Error>;

const PART_MIN: [f64; 2] = [10.0, 5.0];
const PART_MAX: [f64; 2] = [30.0, 25.0];

fn id(raw: u64) -> Result<ToolId, Failure> {
    ToolId::new(raw).ok_or_else(|| "test tool ids start at one".into())
}

fn path_id(raw: u64) -> Result<ToolpathId, Failure> {
    ToolpathId::new(raw).ok_or_else(|| "test toolpath ids start at one".into())
}

fn stock() -> Result<Stock, Failure> {
    Ok(Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?)
}

fn setup(stock: Stock) -> Result<Setup, Failure> {
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        stock.max[2] + 5.0,
    )?)
}

fn tool() -> Result<Tool, Failure> {
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

/// A 20 x 20 x 12 mm block standing inside the stock, with walls on all four sides.
fn part() -> Mesh {
    Stock {
        min: [PART_MIN[0], PART_MIN[1], 0.0],
        max: [PART_MAX[0], PART_MAX[1], 12.0],
    }
    .to_mesh()
}

fn generate_op(operation: &Operation, raw_id: u64) -> Result<Toolpath, Failure> {
    Ok(generate(operation, &setup(stock()?)?, &tool()?, path_id(raw_id)?)?)
}

fn wall_path() -> Result<Toolpath, Failure> {
    generate_op(
        &Operation::FinishWalls {
            part: part(),
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        2,
    )
}

/// The xy distance from a point to the part rectangle: zero inside it.
fn distance_to_part(point: [f64; 2]) -> f64 {
    let dx = (PART_MIN[0] - point[0])
        .max(point[0] - PART_MAX[0])
        .max(0.0);
    let dy = (PART_MIN[1] - point[1])
        .max(point[1] - PART_MAX[1])
        .max(0.0);
    (dx * dx + dy * dy).sqrt()
}

#[test]
fn wall_finishing_clears_the_shell_roughing_leaves() -> Result<(), Failure> {
    let walls = wall_path()?;
    let cuts: Vec<_> = walls
        .points
        .iter()
        .filter(|point| point.kind.is_cutting())
        .collect();
    assert!(!cuts.is_empty(), "wall finishing produced no cutting move");
    for point in &cuts {
        let distance = distance_to_part([point.position[0], point.position[1]]);
        assert!(
            distance >= 3.0 - 1.0e-9,
            "a wall cut gouges the part: {distance} mm from the wall at {:?}",
            point.position
        );
        assert!(
            distance <= 8.0,
            "a wall cut is too far from the part to remove the shell: {distance} mm"
        );
    }

    let rough = generate_op(
        &Operation::Rough3D {
            part: part(),
            stepover_mm: 4.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 2.0,
        },
        1,
    )?;
    let stock = stock()?;
    let mut without = MaterialRemoval::new(stock, 2.0)?;
    without.carve(&rough, &tool()?);
    let mut with = MaterialRemoval::new(stock, 2.0)?;
    with.carve(&rough, &tool()?);
    with.carve(&walls, &tool()?);
    let gain = with.removed_volume_mm3() - without.removed_volume_mm3();
    assert!(
        gain > 100.0,
        "wall finishing removed only {gain} mm3 of the roughing shell"
    );
    Ok(())
}

#[test]
fn wall_levels_respect_the_maximum_depth_of_cut() -> Result<(), Failure> {
    let walls = wall_path()?;
    // Retracts are feed moves too, so only the points below the clearance plane are levels.
    let mut depths: Vec<f64> = walls
        .points
        .iter()
        .filter(|point| point.kind.is_cutting() && point.position[2] < 24.0)
        .map(|point| point.position[2])
        .collect();
    depths.sort_by(|a, b| b.total_cmp(a));
    depths.dedup_by(|a, b| (*a - *b).abs() < 1.0e-9);
    assert!(
        depths.len() >= 3,
        "expected several wall levels, got {depths:?}"
    );
    for pair in depths.windows(2) {
        assert!(
            pair[0] - pair[1] <= 2.0 + 1.0e-9,
            "a wall level drops deeper than the 2 mm maximum: {pair:?}"
        );
    }
    assert!(
        depths[depths.len() - 1] >= 0.0,
        "a wall cut went below the stock floor: {depths:?}"
    );
    Ok(())
}

#[test]
fn a_part_that_fills_the_stock_has_no_walls() -> Result<(), Failure> {
    let stock = stock()?;
    let error = generate(
        &Operation::FinishWalls {
            part: stock.to_mesh(),
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.0,
            resolution_mm: 2.0,
        },
        &setup(stock)?,
        &tool()?,
        path_id(2)?,
    )
    .err();
    assert!(
        matches!(error, Some(ToolpathError::NothingToMachine)),
        "a part filling the block has no wall to finish, got {error:?}"
    );
    Ok(())
}

#[test]
fn wall_finishing_refuses_a_negative_allowance() -> Result<(), Failure> {
    let error = generate(
        &Operation::FinishWalls {
            part: part(),
            stepdown_mm: 2.0,
            stock_to_leave_mm: -0.5,
            resolution_mm: 2.0,
        },
        &setup(stock()?)?,
        &tool()?,
        path_id(2)?,
    )
    .err();
    assert!(
        matches!(error, Some(ToolpathError::InvalidParameter(_))),
        "a negative allowance must be refused, got {error:?}"
    );
    Ok(())
}

//! 3D roughing on a two-height part.
//!
//! The stock is 40 x 30 x 20 mm. The part is a 16 mm tall block (x 5..20) beside an 8 mm tall
//! block (x 20..35), both y 5..25. Roughing must clear stock above 8.5 mm everywhere it can reach
//! and must never cut the tall block below its own top.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, MoveKind, Operation, Setup, Stock, Tool,
    ToolGeometry, ToolId, Toolpath, ToolpathId, WorkCoordinate, generate,
};
use cad_contract::mesh::Mesh;

type Failure = Box<dyn std::error::Error>;

fn id(raw: u64) -> Result<ToolId, Failure> {
    ToolId::new(raw).ok_or_else(|| "test tool ids start at one".into())
}

fn path_id(raw: u64) -> Result<ToolpathId, Failure> {
    ToolpathId::new(raw).ok_or_else(|| "test toolpath ids start at one".into())
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

fn end_mill() -> Result<Tool, Failure> {
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

fn merge(a: &Mesh, b: &Mesh) -> Mesh {
    let mut positions = a.positions.clone();
    let offset = positions.len() as u32;
    positions.extend_from_slice(&b.positions);
    let mut triangles = a.triangles.clone();
    triangles.extend(b.triangles.iter().map(|triangle| {
        [
            triangle[0] + offset,
            triangle[1] + offset,
            triangle[2] + offset,
        ]
    }));
    Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    }
}

/// The two-block part: a 16 mm block and an 8 mm block side by side.
fn two_block_part() -> Mesh {
    let tall = Stock {
        min: [5.0, 5.0, 0.0],
        max: [20.0, 25.0, 16.0],
    };
    let low = Stock {
        min: [20.0, 5.0, 0.0],
        max: [35.0, 25.0, 8.0],
    };
    merge(&tall.to_mesh(), &low.to_mesh())
}

fn rough(part: Mesh) -> Result<Toolpath, Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    Ok(generate(
        &Operation::Rough3D {
            part,
            stepover_mm: 4.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 1.0,
        },
        &setup(stock)?,
        &end_mill()?,
        path_id(1)?,
    )?)
}

#[test]
fn roughing_clears_above_the_part_and_leaves_the_tall_block_alone() -> Result<(), Failure> {
    let toolpath = rough(two_block_part())?;

    // The toolpath is valid: inside the envelope, above the floor, no rapid through the stock.
    toolpath.validate(&setup(Stock::new([0.0; 3], [40.0, 30.0, 20.0])?)?)?;

    // The tall block spans x 5..20, y 5..25 and stands 16 mm high. Inside it, one tool radius away
    // from every wall, nothing may be cut below its top.
    let interior_violation = toolpath.points.iter().any(|point| {
        point.kind != MoveKind::Rapid
            && point.position[2] < 16.0 - 1.0e-6
            && point.position[0] > 8.0
            && point.position[0] < 17.0
            && point.position[1] > 8.0
            && point.position[1] < 22.0
    });
    assert!(
        !interior_violation,
        "roughing cut into the tall block below its top"
    );

    // The low block (8 mm) is roughed down to the deepest level that still leaves the 0.5 mm
    // allowance: z = 10, with 8 + 0.5 < 10. Nothing cuts below that over the block.
    let low_block_cut = toolpath.points.iter().any(|point| {
        point.kind != MoveKind::Rapid
            && (point.position[2] - 10.0).abs() < 1.0e-6
            && point.position[0] > 22.0
            && point.position[0] < 32.0
    });
    assert!(
        low_block_cut,
        "the low block was not roughed to its finish level"
    );
    assert!(
        !toolpath.points.iter().any(|point| {
            point.kind != MoveKind::Rapid
                && point.position[2] < 10.0 - 1.0e-6
                && point.position[0] > 22.0
                && point.position[0] < 32.0
                && point.position[1] > 8.0
                && point.position[1] < 22.0
        }),
        "the low block was cut below its finish level"
    );

    // Outside the part footprint the stock is cleared all the way down to the bottom skin.
    let ring_cut = toolpath.points.iter().any(|point| {
        point.kind != MoveKind::Rapid
            && point.position[2] < 2.0
            && point.position[0] > 36.0
            && point.position[1] > 8.0
            && point.position[1] < 22.0
    });
    assert!(
        ring_cut,
        "stock beside the part was left standing above the block bottom"
    );

    // The first stock slab above the part is removed too, so roughing starts at 18 mm.
    assert!(
        toolpath
            .points
            .iter()
            .any(|point| (point.position[2] - 18.0).abs() < 1.0e-6)
    );
    Ok(())
}

#[test]
fn roughing_splits_the_cut_into_tool_depth_levels() -> Result<(), Failure> {
    let toolpath = rough(two_block_part())?;
    let mut levels: Vec<f64> = toolpath
        .points
        .iter()
        .filter(|point| point.kind != MoveKind::Rapid)
        .map(|point| point.position[2])
        .fold(Vec::new(), |mut unique, z| {
            if !unique.iter().any(|seen: &f64| (*seen - z).abs() < 1.0e-6) {
                unique.push(z);
            }
            unique
        });
    levels.sort_by(|a, b| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));
    assert_eq!(
        levels,
        vec![25.0, 18.0, 16.0, 14.0, 12.0, 10.0, 8.0, 6.0, 4.0, 2.0, 0.5],
        "clearance retracts plus cutting levels to the 0.5 mm bottom skin"
    );
    Ok(())
}

#[test]
fn a_part_that_fills_the_stock_needs_no_roughing() -> Result<(), Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    let error = generate(
        &Operation::Rough3D {
            part: stock.to_mesh(),
            stepover_mm: 4.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 2.0,
        },
        &setup(stock)?,
        &end_mill()?,
        path_id(1)?,
    )
    .err();
    assert!(
        matches!(error, Some(cad_cam::ToolpathError::NothingToMachine)),
        "a full block has nothing to rough, got {error:?}"
    );
    Ok(())
}

#[test]
fn roughing_needs_a_positive_stepover() -> Result<(), Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    let error = generate(
        &Operation::Rough3D {
            part: two_block_part(),
            stepover_mm: 0.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 2.0,
        },
        &setup(stock)?,
        &end_mill()?,
        path_id(1)?,
    )
    .err();
    assert!(error.is_some(), "a zero stepover must be refused");
    Ok(())
}

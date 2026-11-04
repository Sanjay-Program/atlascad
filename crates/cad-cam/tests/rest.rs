//! Rest machining on a part with a slot a large tool cannot enter.
//!
//! The stock is 20 x 20 x 15 mm. The part is a 5 mm base with two 10 mm legs, occupying y 5..15 and
//! leaving a 3 mm wide channel from x 8.5 to 11.5 down to the base top at z 5. A 6 mm roughing tool
//! cannot enter the channel, so it leaves the full 15 mm stock standing there while clearing the
//! ring around the part. A 2 mm rest tool must clear that channel without touching the legs or
//! cutting the base below its allowance.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, MaterialRemoval, MoveKind, Operation, Setup, Stock,
    Tool, ToolGeometry, ToolId, Toolpath, ToolpathId, WorkCoordinate, generate,
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

fn mill(diameter: f64) -> Result<Tool, Failure> {
    Ok(Tool::new(
        id(1)?,
        format!("{diameter} mm flat end mill"),
        ToolGeometry::FlatEndMill {
            diameter_mm: diameter,
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

/// The base plus two legs, leaving a 3 mm channel.
fn channel_part() -> Mesh {
    let base = Stock {
        min: [0.0, 5.0, 0.0],
        max: [20.0, 15.0, 5.0],
    };
    let left = Stock {
        min: [0.0, 5.0, 5.0],
        max: [8.5, 15.0, 15.0],
    };
    let right = Stock {
        min: [11.5, 5.0, 5.0],
        max: [20.0, 15.0, 15.0],
    };
    merge(&merge(&base.to_mesh(), &left.to_mesh()), &right.to_mesh())
}

fn stock() -> Result<Stock, Failure> {
    Ok(Stock::new([0.0; 3], [20.0, 20.0, 15.0])?)
}

fn rough(part: &Mesh) -> Result<Toolpath, Failure> {
    Ok(generate(
        &Operation::Rough3D {
            part: part.clone(),
            stepover_mm: 4.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 2.0,
        },
        &setup(stock()?)?,
        &mill(6.0)?,
        path_id(1)?,
    )?)
}

fn rest(part: &Mesh) -> Result<Toolpath, Failure> {
    Ok(generate(
        &Operation::Rest3D {
            part: part.clone(),
            prior_tool_mm: 6.0,
            prior_stepover_mm: 4.0,
            stepover_mm: 1.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 2.0,
        },
        &setup(stock()?)?,
        &mill(2.0)?,
        path_id(2)?,
    )?)
}

#[test]
fn the_large_tool_leaves_the_channel_and_the_rest_tool_clears_it() -> Result<(), Failure> {
    let part = channel_part();
    let rough_path = rough(&part)?;
    let rest_path = rest(&part)?;
    rest_path.validate(&setup(stock()?)?)?;

    // The prior roughing pass cannot enter a 3 mm channel with a 6 mm cutter.
    let mut model = MaterialRemoval::new(stock()?, 0.5)?;
    model.carve(&rough_path, &mill(6.0)?);
    assert!(
        (model.height_at(10.0, 10.0) - 15.0).abs() < 1.0e-6,
        "the roughing pass was expected to leave the channel at full height, found {}",
        model.height_at(10.0, 10.0)
    );

    // Every rest cut below the top levels stays in the channel centre, never over a leg.
    let strays: Vec<_> = rest_path
        .points
        .iter()
        .filter(|point| point.kind != MoveKind::Rapid && point.position[2] < 14.5)
        .filter(|point| !(9.4..=10.6).contains(&point.position[0]))
        .map(|point| point.position)
        .collect();
    assert!(
        strays.is_empty(),
        "rest cuts outside the channel: {strays:?}"
    );
    assert!(
        rest_path
            .points
            .iter()
            .any(|point| point.kind != MoveKind::Rapid && (point.position[2] - 7.0).abs() < 1.0e-6),
        "the rest pass should reach the lowest level above the 5.5 mm target"
    );

    // After the rest pass the channel is clear down to one stepdown above its target, the legs
    // are untouched, and nothing cuts the base below its allowance.
    model.carve(&rest_path, &mill(2.0)?);
    let channel = model.height_at(10.0, 10.0);
    assert!(
        (5.0..=7.5).contains(&channel),
        "the channel should be cleared to 5..7.5 mm, found {channel}"
    );
    for (x, y) in [(4.0, 10.0), (16.0, 10.0)] {
        assert!(
            (model.height_at(x, y) - 15.0).abs() < 1.0e-6,
            "the rest pass cut the leg at ({x}, {y})"
        );
    }
    assert!(
        (model.height_at(10.0, 2.0) - 0.5).abs() < 1.0e-6,
        "rest machining re-cut the ring the roughing pass had cleared"
    );
    Ok(())
}

#[test]
fn a_rest_tool_that_is_not_smaller_is_refused() -> Result<(), Failure> {
    let error = generate(
        &Operation::Rest3D {
            part: channel_part(),
            prior_tool_mm: 2.0,
            prior_stepover_mm: 1.5,
            stepover_mm: 1.0,
            stepdown_mm: 2.0,
            stock_to_leave_mm: 0.5,
            resolution_mm: 2.0,
        },
        &setup(stock()?)?,
        &mill(2.0)?,
        path_id(2)?,
    )
    .err();
    let message = error.map(|error| error.to_string()).unwrap_or_default();
    assert!(
        message.contains("larger than the 2 mm rest tool"),
        "expected a prior-tool size refusal, got {message}"
    );
    Ok(())
}

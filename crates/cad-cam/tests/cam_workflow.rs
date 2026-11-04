//! Reference fixtures for the CAM pipeline.
//!
//! The stock is a 40 x 30 x 20 mm block on a 6 mm flat end mill. Every expected number below is
//! arithmetic, not a snapshot: the fixture either covers the block or it does not.

use cad_cam::tool_path::POSITION_TOLERANCE_MM;
use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, MoveKind, Operation, Setup, Stock, Tool,
    ToolGeometry, ToolId, Toolpath, ToolpathId, WorkCoordinate, generate,
};

type Failure = Box<dyn std::error::Error>;

fn id(raw: u64) -> Result<ToolId, Failure> {
    ToolId::new(raw).ok_or_else(|| "test tool ids start at one".into())
}

fn path_id(raw: u64) -> Result<ToolpathId, Failure> {
    ToolpathId::new(raw).ok_or_else(|| "test toolpath ids start at one".into())
}

fn setup() -> Result<Setup, Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        25.0,
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

fn run(operation: &Operation) -> Result<Toolpath, Failure> {
    Ok(generate(operation, &setup()?, &end_mill()?, path_id(1)?)?)
}

#[test]
fn facing_is_a_zig_zag_raster_over_the_whole_block() -> Result<(), Failure> {
    let toolpath = run(&Operation::Facing {
        depth_mm: 2.0,
        stepover_mm: 4.0,
    })?;

    // The 6 mm tool covers a 46 mm wide pass and the raster spans -3 .. 33 mm in y in 4 mm steps:
    // 10 lines of (rapid, plunge, cut, retract).
    assert_eq!(toolpath.points.len(), 40);
    assert_eq!(toolpath.points[0].position, [-3.0, -3.0, 25.0]);
    assert!(!toolpath.points[0].kind.is_cutting());
    assert_eq!(toolpath.points[1].position, [-3.0, -3.0, 18.0]);
    assert_eq!(toolpath.points[2].position, [43.0, -3.0, 18.0]);
    assert_eq!(toolpath.points[2].feed_mm_min, 600.0);
    assert_eq!(toolpath.points[3].position, [43.0, -3.0, 25.0]);

    // The cut follows the block: the top face is removed to z = 20 - 2.
    assert!(toolpath
        .points
        .iter()
        .filter(|point| point.kind.is_cutting())
        .all(|point| point.position[2] >= 18.0 - POSITION_TOLERANCE_MM));
    assert!(toolpath.cutting_length_mm() > 0.0);
    assert!(toolpath.estimated_minutes() > 0.0);
    Ok(())
}

#[test]
fn a_contour_is_closed_at_every_depth() -> Result<(), Failure> {
    let profile = vec![[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]];
    let toolpath = run(&Operation::Contour {
        profile,
        depth_mm: 5.0,
        stepdown_mm: 2.0,
        offset_mm: 0.0,
    })?;

    // 5 mm at 2 mm per pass is three passes (2, 4, 5). The first enters at the clearance plane;
    // the next two start already above the loop origin, so they plunge in place.
    assert_eq!(toolpath.points.len(), 7 + 6 + 6);
    let levels: Vec<f64> = toolpath
        .points
        .iter()
        .map(|point| point.position[2])
        .fold(Vec::new(), |mut unique, z| {
            if !unique.iter().any(|seen: &f64| (*seen - z).abs() < 1.0e-9) {
                unique.push(z);
            }
            unique
        });
    assert_eq!(levels, vec![25.0, 18.0, 16.0, 15.0]);

    // The tool-centre path is the profile offset outwards by the 3 mm radius.
    let first_level: Vec<[f64; 2]> = toolpath
        .points
        .iter()
        .filter(|point| point.kind.is_cutting() && (point.position[2] - 18.0).abs() < 1.0e-9)
        .map(|point| [point.position[0], point.position[1]])
        .collect();
    assert_eq!(first_level.first(), Some(&[7.0, 7.0]));
    assert_eq!(first_level.last(), Some(&[7.0, 7.0]), "the loop closes");
    Ok(())
}

#[test]
fn a_pocket_clears_its_region_inside_the_tool_radius() -> Result<(), Failure> {
    let region = [[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]];
    let toolpath = run(&Operation::Pocket {
        region,
        depth_mm: 4.0,
        stepdown_mm: 2.0,
        stepover_mm: 4.0,
    })?;

    // Two depths: a boundary loop plus a two-line raster each. The first raster line starts where
    // the loop's retract left the tool, so it plunges in place.
    assert_eq!(toolpath.points.len(), 2 * 14);
    for point in toolpath.points.iter().filter(|point| point.kind.is_cutting()) {
        assert!(
            point.position[0] >= 13.0 - POSITION_TOLERANCE_MM
                && point.position[0] <= 27.0 + POSITION_TOLERANCE_MM,
            "the pocket cut leaves the radius-shrunk region: {:?}",
            point.position
        );
    }
    Ok(())
}

#[test]
fn drilling_pecks_and_retracts() -> Result<(), Failure> {
    let toolpath = run(&Operation::Drill {
        holes: vec![[10.0, 10.0], [20.0, 15.0]],
        depth_mm: 6.0,
        peck_mm: 4.0,
    })?;

    // Each hole: rapid, peck to 4 mm, chip clear, peck to 6 mm, retract.
    assert_eq!(toolpath.points.len(), 10);
    let depths: Vec<f64> = toolpath
        .points
        .iter()
        .filter(|point| point.kind.is_cutting())
        .map(|point| point.position[2])
        .collect();
    assert_eq!(depths, vec![16.0, 20.0, 14.0, 25.0, 16.0, 20.0, 14.0, 25.0]);
    Ok(())
}

#[test]
fn a_slot_runs_between_its_endpoints_at_every_depth() -> Result<(), Failure> {
    let toolpath = run(&Operation::Slot {
        start: [5.0, 5.0],
        end: [25.0, 5.0],
        depth_mm: 4.0,
        stepdown_mm: 2.0,
    })?;

    assert_eq!(toolpath.points.len(), 8);
    let cuts: Vec<[f64; 3]> = toolpath
        .points
        .iter()
        .filter(|point| point.kind == MoveKind::Feed && point.position[2] < 25.0)
        .map(|point| point.position)
        .collect();
    assert!(cuts.contains(&[25.0, 5.0, 18.0]));
    assert!(cuts.contains(&[25.0, 5.0, 16.0]));
    Ok(())
}

#[test]
fn every_operation_passes_its_own_validation() -> Result<(), Failure> {
    let setup = setup()?;
    let tool = end_mill()?;
    let operations = [
        Operation::Facing {
            depth_mm: 2.0,
            stepover_mm: 4.0,
        },
        Operation::Contour {
            profile: vec![[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]],
            depth_mm: 5.0,
            stepdown_mm: 2.0,
            offset_mm: 0.5,
        },
        Operation::Pocket {
            region: [[10.0, 10.0], [30.0, 10.0], [30.0, 20.0], [10.0, 20.0]],
            depth_mm: 4.0,
            stepdown_mm: 2.0,
            stepover_mm: 4.0,
        },
        Operation::Drill {
            holes: vec![[10.0, 10.0]],
            depth_mm: 6.0,
            peck_mm: 3.0,
        },
        Operation::Slot {
            start: [5.0, 5.0],
            end: [25.0, 5.0],
            depth_mm: 4.0,
            stepdown_mm: 2.0,
        },
    ];
    for operation in operations {
        let toolpath = generate(&operation, &setup, &tool, path_id(1)?)?;
        toolpath.validate(&setup)?;
        assert!(!toolpath.points.is_empty());
    }
    Ok(())
}

#[test]
fn an_operation_deeper_than_the_block_is_refused() -> Result<(), Failure> {
    let error = generate(
        &Operation::Facing {
            depth_mm: 25.0,
            stepover_mm: 4.0,
        },
        &setup()?,
        &end_mill()?,
        path_id(1)?,
    )
    .err();
    assert!(error.is_some(), "a 25 mm face cut on a 20 mm block must fail");
    Ok(())
}

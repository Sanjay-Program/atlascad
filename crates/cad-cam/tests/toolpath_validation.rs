//! The acceptance cases a toolpath must be refused for.
//!
//! Each test builds the smallest toolpath that exhibits one fault and asserts the specific error,
//! so a regression cannot be masked by a different failure firing first.

use cad_cam::{
    Machine, MachineEnvelope, MoveKind, Setup, Stock, Toolpath, ToolpathError, ToolpathId,
    ToolpathPoint, WorkCoordinate,
};

fn setup() -> Result<Setup, Box<dyn std::error::Error>> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    Ok(Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        25.0,
    )?)
}

fn toolpath(points: Vec<ToolpathPoint>) -> Result<Toolpath, Box<dyn std::error::Error>> {
    let id = ToolpathId::new(1).ok_or("toolpath ids start at one")?;
    let tool_id = cad_cam::ToolId::new(1).ok_or("tool ids start at one")?;
    Ok(Toolpath::new(id, tool_id, points, 12_000.0, 25.0))
}

#[test]
fn a_reference_toolpath_is_accepted() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::plunge([0.0, 0.0, 18.0], 200.0),
        ToolpathPoint::feed([10.0, 0.0, 18.0], 600.0),
        ToolpathPoint::feed([10.0, 0.0, 25.0], 600.0),
    ])?;
    path.validate(&setup)?;
    assert_eq!(path.points.len(), 4);
    Ok(())
}

#[test]
fn a_toolpath_shorter_than_two_moves_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![ToolpathPoint::rapid([0.0, 0.0, 25.0])])?;
    assert_eq!(path.validate(&setup), Err(ToolpathError::TooShort { points: 1 }));
    Ok(())
}

#[test]
fn a_non_finite_coordinate_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::rapid([f64::NAN, 0.0, 25.0]),
    ])?;
    assert_eq!(
        path.validate(&setup),
        Err(ToolpathError::NonFinite { index: 1, axis: 0 })
    );
    Ok(())
}

#[test]
fn a_cutting_move_without_a_feed_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::feed([10.0, 0.0, 18.0], 0.0),
    ])?;
    assert_eq!(
        path.validate(&setup),
        Err(ToolpathError::InvalidFeed {
            index: 1,
            feed: 0.0
        })
    );
    Ok(())
}

#[test]
fn a_zero_length_move_is_a_discontinuity_not_a_cut() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
    ])?;
    assert_eq!(
        path.validate(&setup),
        Err(ToolpathError::ZeroLengthMove { index: 1 })
    );
    Ok(())
}

#[test]
fn a_jump_longer_than_one_move_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::feed([0.0, 0.0, 18.0], 600.0),
        ToolpathPoint::feed([400.0, 0.0, 18.0], 600.0),
    ])?;
    match path.validate(&setup) {
        Err(ToolpathError::Discontinuous { index, distance }) => {
            assert_eq!(index, 1);
            assert!((distance - 400.0).abs() < 1.0e-9);
        }
        other => return Err(format!("expected a discontinuity, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_move_outside_the_machine_envelope_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    let envelope = MachineEnvelope::new([-100.0; 3], [100.0; 3])?;
    let setup = Setup::new(
        Machine::Milling3Axis,
        envelope,
        stock,
        WorkCoordinate::standard(),
        25.0,
    )?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::rapid([150.0, 0.0, 25.0]),
    ])?;
    match path.validate(&setup) {
        Err(ToolpathError::OutsideEnvelope {
            index,
            axis,
            value,
            min,
            max,
        }) => {
            assert_eq!(index, 1);
            assert_eq!(axis, 0);
            assert!((value - 150.0).abs() < 1.0e-9);
            assert!((min + 100.0).abs() < 1.0e-9);
            assert!((max - 100.0).abs() < 1.0e-9);
        }
        other => return Err(format!("expected an envelope refusal, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn cutting_below_the_stock_floor_is_a_gouge() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::plunge([10.0, 10.0, -1.0], 200.0),
    ])?;
    assert_eq!(
        path.validate(&setup),
        Err(ToolpathError::Gouge {
            index: 1,
            z: -1.0,
            floor: 0.0
        })
    );
    Ok(())
}

#[test]
fn a_rapid_through_the_stock_is_a_collision() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let path = toolpath(vec![
        ToolpathPoint::rapid([0.0, 0.0, 25.0]),
        ToolpathPoint::rapid([10.0, 10.0, 5.0]),
    ])?;
    match path.validate(&setup) {
        Err(ToolpathError::Collision { index, position }) => {
            assert_eq!(index, 1);
            assert_eq!(position, [10.0, 10.0, 5.0]);
        }
        other => return Err(format!("expected a collision, got {other:?}").into()),
    }
    Ok(())
}

#[test]
fn a_clearance_plane_below_the_stock_top_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let id = ToolpathId::new(1).ok_or("toolpath ids start at one")?;
    let tool_id = cad_cam::ToolId::new(1).ok_or("tool ids start at one")?;
    let path = Toolpath::new(
        id,
        tool_id,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 15.0]),
            ToolpathPoint::feed([10.0, 0.0, 15.0], 600.0),
        ],
        12_000.0,
        15.0,
    );
    assert!(matches!(
        path.validate(&setup),
        Err(ToolpathError::InvalidClearance { .. })
    ));
    Ok(())
}

#[test]
fn an_unspun_toolpath_is_refused() -> Result<(), Box<dyn std::error::Error>> {
    let setup = setup()?;
    let id = ToolpathId::new(1).ok_or("toolpath ids start at one")?;
    let tool_id = cad_cam::ToolId::new(1).ok_or("tool ids start at one")?;
    let path = Toolpath::new(
        id,
        tool_id,
        vec![
            ToolpathPoint::rapid([0.0, 0.0, 25.0]),
            ToolpathPoint::feed([10.0, 0.0, 18.0], 600.0),
        ],
        0.0,
        25.0,
    );
    assert_eq!(
        path.validate(&setup),
        Err(ToolpathError::InvalidSpindle { rpm: 0.0 })
    );
    Ok(())
}

#[test]
fn move_kinds_map_to_their_g_words() {
    assert_eq!(MoveKind::Rapid.word(), "G0");
    assert_eq!(MoveKind::Feed.word(), "G1");
    assert_eq!(MoveKind::Plunge.word(), "G1");
    assert!(!MoveKind::Rapid.is_cutting());
    assert!(MoveKind::Plunge.is_cutting());
}

use cad_contract::ids::DocumentId;
use cad_contract::units::Unit;
use cad_drafting::{Draft, EntityKind, measure_distance, measure_polygon_area};

#[test]
fn test_measure_distance_and_area() -> Result<(), Box<dyn std::error::Error>> {
    let dist = measure_distance([0.0, 0.0], [30.0, 40.0])?;
    assert!((dist.distance - 50.0).abs() < 1e-9);
    assert!((dist.delta_x - 30.0).abs() < 1e-9);
    assert!((dist.delta_y - 40.0).abs() < 1e-9);

    // 10x10 square
    let square = [[0.0, 0.0], [10.0, 0.0], [10.0, 10.0], [0.0, 10.0]];
    let area_inq = measure_polygon_area(&square)?;
    assert!((area_inq.area - 100.0).abs() < 1e-9);
    assert!((area_inq.perimeter - 40.0).abs() < 1e-9);
    assert!((area_inq.centroid[0] - 5.0).abs() < 1e-9);
    assert!((area_inq.centroid[1] - 5.0).abs() < 1e-9);
    Ok(())
}

#[test]
fn test_translate_and_copy() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(1);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    let l1 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 0.0],
            end: [50.0, 0.0],
        },
    )?;

    draft.translate_entities(&[l1], 10.0, 20.0)?;
    let ent = draft.entity(l1).ok_or("entity exists")?;
    assert_eq!(
        ent.kind,
        EntityKind::Line {
            start: [10.0, 20.0],
            end: [60.0, 20.0]
        }
    );

    // Undo translation
    draft.undo()?;
    let ent_undone = draft.entity(l1).ok_or("entity exists")?;
    assert_eq!(
        ent_undone.kind,
        EntityKind::Line {
            start: [0.0, 0.0],
            end: [50.0, 0.0]
        }
    );

    // Copy entity
    let copies = draft.copy_entities(&[l1], 0.0, 15.0)?;
    assert_eq!(copies.len(), 1);
    let copy_ent = draft.entity(copies[0]).ok_or("copy exists")?;
    assert_eq!(
        copy_ent.kind,
        EntityKind::Line {
            start: [0.0, 15.0],
            end: [50.0, 15.0]
        }
    );
    Ok(())
}

#[test]
fn test_rotate_and_scale() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(2);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    let l1 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 0.0],
            end: [10.0, 0.0],
        },
    )?;

    // Rotate 90 degrees around origin
    draft.rotate_entities(&[l1], [0.0, 0.0], std::f64::consts::FRAC_PI_2)?;
    let ent = draft.entity(l1).ok_or("entity exists")?;
    if let EntityKind::Line { start, end } = ent.kind {
        assert!(start[0].abs() < 1e-6);
        assert!(start[1].abs() < 1e-6);
        assert!(end[0].abs() < 1e-6);
        assert!((end[1] - 10.0).abs() < 1e-6);
    } else {
        return Err("expected line".into());
    }

    // Scale circle by 2.0
    let c1 = draft.add_entity(
        "0",
        EntityKind::Circle {
            center: [0.0, 0.0],
            radius: 5.0,
        },
    )?;

    draft.scale_entities(&[c1], [0.0, 0.0], 2.0)?;
    let ent_c = draft.entity(c1).ok_or("circle exists")?;
    if let EntityKind::Circle { radius, .. } = ent_c.kind {
        assert!((radius - 10.0).abs() < 1e-9);
    } else {
        return Err("expected circle".into());
    }
    Ok(())
}

#[test]
fn test_mirror() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(3);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    let l1 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 5.0],
            end: [20.0, 5.0],
        },
    )?;

    // Mirror across X-axis ([0,0] to [1,0]), duplicating
    let mirrored_ids = draft.mirror_entities(&[l1], [0.0, 0.0], [1.0, 0.0], false)?;
    assert_eq!(mirrored_ids.len(), 1);

    let original = draft.entity(l1).ok_or("orig exists")?;
    let mirrored = draft.entity(mirrored_ids[0]).ok_or("mirror exists")?;

    assert_eq!(
        original.kind,
        EntityKind::Line {
            start: [0.0, 5.0],
            end: [20.0, 5.0]
        }
    );
    assert_eq!(
        mirrored.kind,
        EntityKind::Line {
            start: [0.0, -5.0],
            end: [20.0, -5.0]
        }
    );
    Ok(())
}

#[test]
fn test_offset_line_circle_rectangle() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(4);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    let l1 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 0.0],
            end: [100.0, 0.0],
        },
    )?;

    // Offset +10 (left normal is +Y)
    let off_l1 = draft.offset_entity(l1, 10.0, 1.0)?;
    let off_ent = draft.entity(off_l1).ok_or("exists")?;
    if let EntityKind::Line { start, end } = off_ent.kind {
        assert!((start[1] - 10.0).abs() < 1e-6);
        assert!((end[1] - 10.0).abs() < 1e-6);
    } else {
        return Err("expected line".into());
    }

    // Offset circle
    let c1 = draft.add_entity(
        "0",
        EntityKind::Circle {
            center: [0.0, 0.0],
            radius: 15.0,
        },
    )?;
    let off_c1 = draft.offset_entity(c1, 5.0, 1.0)?;
    if let EntityKind::Circle { radius, .. } = draft.entity(off_c1).ok_or("exists")?.kind {
        assert!((radius - 20.0).abs() < 1e-6);
    }

    // Offset rectangle
    let r1 = draft.add_rectangle("0", [0.0, 0.0], 50.0, 30.0)?;
    let off_r1 = draft.offset_entity(r1, 5.0, 1.0)?;
    if let EntityKind::Rectangle {
        origin,
        width,
        height,
    } = draft.entity(off_r1).ok_or("exists")?.kind
    {
        assert!((origin[0] - (-5.0)).abs() < 1e-6);
        assert!((origin[1] - (-5.0)).abs() < 1e-6);
        assert!((width - 60.0).abs() < 1e-6);
        assert!((height - 40.0).abs() < 1e-6);
    }
    Ok(())
}

#[test]
fn test_fillet_and_chamfer() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(5);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    // Two lines meeting at origin: [0, 20] -> [0, 0] and [0, 0] -> [20, 0]
    let l1 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 20.0],
            end: [0.0, 0.0],
        },
    )?;
    let l2 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 0.0],
            end: [20.0, 0.0],
        },
    )?;

    // Fillet with R = 5.0
    let opt_arc_id = draft.fillet(l1, l2, 5.0)?;
    let arc_id = opt_arc_id.ok_or("fillet arc created")?;

    let ent_arc = draft.entity(arc_id).ok_or("arc exists")?;
    if let EntityKind::Arc { center, radius, .. } = ent_arc.kind {
        assert!((radius - 5.0).abs() < 1e-6);
        assert!((center[0] - 5.0).abs() < 1e-6);
        assert!((center[1] - 5.0).abs() < 1e-6);
    } else {
        return Err("expected arc".into());
    }

    // Chamfer test on fresh lines
    let l3 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [50.0, 20.0],
            end: [50.0, 0.0],
        },
    )?;
    let l4 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [50.0, 0.0],
            end: [70.0, 0.0],
        },
    )?;

    let bevel_id = draft.chamfer(l3, l4, 4.0, 3.0)?;
    let bevel_ent = draft.entity(bevel_id).ok_or("bevel exists")?;
    if let EntityKind::Line { start, end } = bevel_ent.kind {
        let (p1, p2) = (start, end);
        let dist = ((p2[0] - p1[0]).powi(2) + (p2[1] - p1[1]).powi(2)).sqrt();
        assert!((dist - 5.0).abs() < 1e-6);
    } else {
        return Err("expected bevel line".into());
    }
    Ok(())
}

#[test]
fn test_explode() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(6);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    let r1 = draft.add_rectangle("0", [0.0, 0.0], 40.0, 25.0)?;

    let line_ids = draft.explode_entity(r1)?;
    assert_eq!(line_ids.len(), 4);

    for id in &line_ids {
        let ent = draft.entity(*id).ok_or("line exists")?;
        assert!(matches!(ent.kind, EntityKind::Line { .. }));
    }
    // Original rectangle is gone
    assert!(draft.entity(r1).is_none());

    // Undo restores the rectangle
    draft.undo()?;
    assert!(draft.entity(r1).is_some());
    Ok(())
}

#[test]
fn test_hatch_and_multiview() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(7);
    let mut draft = Draft::new("Test Drawing", doc_id, Unit::Millimeter);

    // 20x20 square boundary
    let square = [[0.0, 0.0], [20.0, 0.0], [20.0, 20.0], [0.0, 20.0]];

    // Add ANSI31 crosshatch lines with 4.0 mm spacing
    let hatch_ids = draft.add_hatch("0", &square, "ANSI31", 45.0, 4.0)?;
    assert!(!hatch_ids.is_empty());
    for id in &hatch_ids {
        let ent = draft.entity(*id).ok_or("hatch line exists")?;
        assert!(matches!(ent.kind, EntityKind::Line { .. }));
    }

    // Test NET grid hatch
    let net_ids = draft.add_hatch("0", &square, "NET", 45.0, 4.0)?;
    assert!(net_ids.len() > hatch_ids.len());

    // Undo removes the hatch lines
    draft.undo()?;
    assert!(draft.entity(net_ids[0]).is_none());
    Ok(())
}

#[test]
fn test_constructions_circle_arc_polygon() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(8);
    let mut draft = Draft::new("Test Constructions", doc_id, Unit::Millimeter);

    // 1. Circle from 2 diameter points
    let c1 = draft.add_circle_2p("0", [0.0, 0.0], [100.0, 0.0])?;
    let ent_c1 = draft.entity(c1).ok_or("c1 exists")?;
    match ent_c1.kind {
        EntityKind::Circle { center, radius } => {
            assert!((center[0] - 50.0).abs() < 1e-9);
            assert!((center[1] - 0.0).abs() < 1e-9);
            assert!((radius - 50.0).abs() < 1e-9);
        }
        _ => return Err("expected Circle".into()),
    }

    // 2. Circle from 3 points
    let c2 = draft.add_circle_3p("0", [0.0, 0.0], [100.0, 0.0], [50.0, 50.0])?;
    let ent_c2 = draft.entity(c2).ok_or("c2 exists")?;
    match ent_c2.kind {
        EntityKind::Circle { center, radius } => {
            assert!((center[0] - 50.0).abs() < 1e-9);
            assert!((center[1] - 0.0).abs() < 1e-9);
            assert!((radius - 50.0).abs() < 1e-9);
        }
        _ => return Err("expected Circle".into()),
    }

    // 3. Arc from 3 points
    let a1 = draft.add_arc_3p("0", [100.0, 0.0], [50.0, 50.0], [0.0, 0.0])?;
    let ent_a1 = draft.entity(a1).ok_or("a1 exists")?;
    match ent_a1.kind {
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => {
            assert!((center[0] - 50.0).abs() < 1e-9);
            assert!((radius - 50.0).abs() < 1e-9);
            assert!((start_degrees - 0.0).abs() < 1e-5);
            assert!((end_degrees - 180.0).abs() < 1e-5);
        }
        _ => return Err("expected Arc".into()),
    }

    // 4. Inscribed Hexagon
    let hex = draft.add_polygon("0", [0.0, 0.0], 25.0, 6, true, 0.0)?;
    let ent_hex = draft.entity(hex).ok_or("hex exists")?;
    match &ent_hex.kind {
        EntityKind::Polyline { points, closed } => {
            assert!(*closed);
            assert_eq!(points.len(), 6);
            for p in points {
                let r = (p[0] * p[0] + p[1] * p[1]).sqrt();
                assert!((r - 25.0).abs() < 1e-5);
            }
        }
        _ => return Err("expected Polyline".into()),
    }

    Ok(())
}

#[test]
fn test_join_entities() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(9);
    let mut draft = Draft::new("Test Join", doc_id, Unit::Millimeter);

    let l1 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 0.0],
            end: [20.0, 0.0],
        },
    )?;
    let l2 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [20.0, 0.0],
            end: [20.0, 15.0],
        },
    )?;
    let l3 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [20.0, 15.0],
            end: [0.0, 15.0],
        },
    )?;

    // Join 3 lines into an open polyline
    let joined = draft.join_entities(&[l1, l2, l3], 1e-4)?;
    assert!(draft.entity(l1).is_none());
    assert!(draft.entity(l2).is_none());
    assert!(draft.entity(l3).is_none());

    let ent_joined = draft.entity(joined).ok_or("joined exists")?;
    match &ent_joined.kind {
        EntityKind::Polyline { points, closed } => {
            assert!(!*closed);
            assert_eq!(points.len(), 4);
            assert_eq!(points[0], [0.0, 0.0]);
            assert_eq!(points[3], [0.0, 15.0]);
        }
        _ => return Err("expected Polyline".into()),
    }

    // Add closing line
    let l4 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 15.0],
            end: [0.0, 0.0],
        },
    )?;

    // Join polyline with l4 into closed polyline
    let closed_poly = draft.join_entities(&[joined, l4], 1e-4)?;
    let ent_closed = draft.entity(closed_poly).ok_or("closed exists")?;
    match &ent_closed.kind {
        EntityKind::Polyline { points, closed } => {
            assert!(*closed);
            assert_eq!(points.len(), 4);
        }
        _ => return Err("expected Polyline".into()),
    }

    // Undo restores previous state
    draft.undo()?;
    assert!(draft.entity(closed_poly).is_none());
    assert!(draft.entity(joined).is_some());
    assert!(draft.entity(l4).is_some());

    Ok(())
}

#[test]
fn test_rectangular_and_polar_array() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(10);
    let mut draft = Draft::new("Test Array", doc_id, Unit::Millimeter);

    let pin = draft.add_entity(
        "0",
        EntityKind::Circle {
            center: [10.0, 10.0],
            radius: 2.0,
        },
    )?;

    // Rectangular array: 3 rows, 4 cols -> 11 new copies (12 total items)
    let rect_copies = draft.rectangular_array(&[pin], 3, 4, 15.0, 20.0)?;
    assert_eq!(rect_copies.len(), 11);

    // Undo removes all array copies
    draft.undo()?;
    for id in &rect_copies {
        assert!(draft.entity(*id).is_none());
    }
    assert!(draft.entity(pin).is_some());

    // Polar array: 6 items in 360° circle around origin -> 5 new copies
    let polar_copies = draft.polar_array(&[pin], 6, [0.0, 0.0], 360.0, true)?;
    assert_eq!(polar_copies.len(), 5);

    for id in &polar_copies {
        let ent = draft.entity(*id).ok_or("polar copy exists")?;
        match ent.kind {
            EntityKind::Circle { center, radius } => {
                assert!((radius - 2.0).abs() < 1e-9);
                let d_from_origin = (center[0] * center[0] + center[1] * center[1]).sqrt();
                let orig_d = (10.0_f64 * 10.0 + 10.0 * 10.0).sqrt();
                assert!((d_from_origin - orig_d).abs() < 1e-5);
            }
            _ => return Err("expected Circle".into()),
        }
    }

    // Undo removes polar array
    draft.undo()?;
    assert!(draft.entity(polar_copies[0]).is_none());
    assert!(draft.entity(pin).is_some());

    Ok(())
}

#[test]
fn test_engineering_section_properties() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(11);
    let mut draft = Draft::new("Test Section", doc_id, Unit::Millimeter);

    // 1. Rectangle 20 x 10 centered at [0, 0] (origin at [-10, -5])
    let rect_id = draft.add_rectangle("0", [-10.0, -5.0], 20.0, 10.0)?;
    let rect_props = draft.inquire_section(rect_id)?;
    assert!((rect_props.area - 200.0).abs() < 1e-6);
    assert!((rect_props.centroid[0] - 0.0).abs() < 1e-6);
    assert!((rect_props.centroid[1] - 0.0).abs() < 1e-6);
    // Ixx = b * h^3 / 12 = 20 * 1000 / 12 = 1666.667
    assert!((rect_props.i_xx - (20.0 * 1000.0 / 12.0)).abs() < 1e-3);
    // Iyy = h * b^3 / 12 = 10 * 8000 / 12 = 6666.667
    assert!((rect_props.i_yy - (10.0 * 8000.0 / 12.0)).abs() < 1e-3);
    // J = Ixx + Iyy
    assert!((rect_props.j_polar - (rect_props.i_xx + rect_props.i_yy)).abs() < 1e-6);
    // Sx = Ixx / 5.0
    assert!((rect_props.s_x - (rect_props.i_xx / 5.0)).abs() < 1e-3);
    // Sy = Iyy / 10.0
    assert!((rect_props.s_y - (rect_props.i_yy / 10.0)).abs() < 1e-3);

    // 2. Circle radius 10 at [50, 50]
    let circ_id = draft.add_entity(
        "0",
        EntityKind::Circle {
            center: [50.0, 50.0],
            radius: 10.0,
        },
    )?;
    let circ_props = draft.inquire_section(circ_id)?;
    let expected_area = std::f64::consts::PI * 100.0;
    assert!((circ_props.area - expected_area).abs() < 1e-6);
    assert!((circ_props.centroid[0] - 50.0).abs() < 1e-6);
    assert!((circ_props.centroid[1] - 50.0).abs() < 1e-6);
    assert!((circ_props.r_x - 5.0).abs() < 1e-6);
    assert!((circ_props.r_y - 5.0).abs() < 1e-6);

    Ok(())
}

#[test]
fn test_trim_and_extend() -> Result<(), Box<dyn std::error::Error>> {
    let doc_id = DocumentId::new(12);
    let mut draft = Draft::new("Test Trim & Extend", doc_id, Unit::Millimeter);

    // 1. Endpoint Trim
    let target = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 10.0],
            end: [30.0, 10.0],
        },
    )?;
    let boundary = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [10.0, 0.0],
            end: [10.0, 20.0],
        },
    )?;

    // Click near start (x=5) to trim away the [0, 10] to [10, 10] segment
    let remaining = draft.trim(target, &[boundary], [5.0, 10.0])?;
    assert_eq!(remaining.len(), 1);
    let ent_rem = draft.entity(remaining[0]).ok_or("remaining line exists")?;
    match ent_rem.kind {
        EntityKind::Line { start, end } => {
            assert!((start[0] - 10.0).abs() < 1e-5);
            assert!((start[1] - 10.0).abs() < 1e-5);
            assert!((end[0] - 30.0).abs() < 1e-5);
            assert!((end[1] - 10.0).abs() < 1e-5);
        }
        _ => return Err("expected Line".into()),
    }

    // Undo restores target line
    draft.undo()?;
    assert!(draft.entity(target).is_some());

    // 2. Interior Trim with two boundaries
    let boundary2 = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [20.0, 0.0],
            end: [20.0, 20.0],
        },
    )?;
    // Click in middle (x=15) to trim away the middle segment [10, 20]
    let pieces = draft.trim(target, &[boundary, boundary2], [15.0, 10.0])?;
    assert_eq!(pieces.len(), 2);
    let p1 = draft.entity(pieces[0]).ok_or("piece 1")?;
    let p2 = draft.entity(pieces[1]).ok_or("piece 2")?;
    match (p1.kind.clone(), p2.kind.clone()) {
        (EntityKind::Line { start: s1, end: e1 }, EntityKind::Line { start: s2, end: e2 }) => {
            assert!((s1[0] - 0.0).abs() < 1e-5);
            assert!((e1[0] - 10.0).abs() < 1e-5);
            assert!((s2[0] - 20.0).abs() < 1e-5);
            assert!((e2[0] - 30.0).abs() < 1e-5);
        }
        _ => return Err("expected two Lines".into()),
    }

    // Undo
    draft.undo()?;

    // 3. Extend
    let short_line = draft.add_entity(
        "0",
        EntityKind::Line {
            start: [0.0, 10.0],
            end: [5.0, 10.0],
        },
    )?;
    // Extend toward boundary at x=10 by clicking near end (x=4)
    let ext_id = draft.extend(short_line, &[boundary], [4.5, 10.0])?;
    let ext_ent = draft.entity(ext_id).ok_or("extended exists")?;
    match ext_ent.kind {
        EntityKind::Line { start, end } => {
            assert!((start[0] - 0.0).abs() < 1e-5);
            assert!((end[0] - 10.0).abs() < 1e-5);
        }
        _ => return Err("expected Line".into()),
    }

    // Undo restores short line
    draft.undo()?;
    let restored = draft.entity(short_line).ok_or("restored exists")?;
    match restored.kind {
        EntityKind::Line { end, .. } => {
            assert!((end[0] - 5.0).abs() < 1e-5);
        }
        _ => return Err("expected Line".into()),
    }

    Ok(())
}

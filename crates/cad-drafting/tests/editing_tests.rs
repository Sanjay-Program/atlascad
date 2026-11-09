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

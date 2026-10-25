//! A PCB Studio board derived into a Part Studio (`docs/PCB_IN_PART_STUDIO.md` P1): the board's
//! solid and its components' mesh parts, placed by the Derived feature's locations, following
//! the board's layout live, with ids that survive layout edits.
//!
//! The board is the KiCad course's finished board ([`cadrs_eda::getting_started::gs18`]):
//! a 45 × 50 mm outline, 1.6 mm thick, with BT1 on the bottom and D1, R1 on top. Expected
//! values are worked out from the layout here, not read back from the model.
#![cfg(feature = "occt")]

use cadrs_core::command::Command;
use cadrs_core::commands::AddFeature;
use cadrs_core::derived::{self, AddDerived, DerivedBoard, DerivedFeature, SetDerived};
use cadrs_core::document::{Element, FeatureKind};
use cadrs_core::external::{RefAt, Resolver, SourceRef};
use cadrs_core::mate::{ConnectorOrigin, ConnectorRef, MateConnectorFeature};
use cadrs_core::pcb::board3d::{BoardAnchor, BoardSeat, ComponentFilter};
use cadrs_core::pcb::{AddBoard, BoardId, SetDesign};
use cadrs_core::{Document, ElementId, FeatureId, History, Part, Store};
use cadrs_eda::units::{mm, to_mm};

const STUDIO: ElementId = ElementId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0001);
const PCB: ElementId = ElementId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0002);
const DERIVED: FeatureId = FeatureId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0003);
const LOC: FeatureId = FeatureId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0004);

fn run(doc: &mut Document, h: &mut History, c: &dyn Command) {
    h.execute(doc, c).unwrap_or_else(|e| panic!("{}: {e}", c.label()));
}

fn course() -> cadrs_eda::Design {
    cadrs_eda::getting_started::gs18(&cadrs_eda::library::LibraryTable::builtin())
}

/// A document with a PCB Studio holding the course board and a Part Studio with a mate
/// connector at (0, 0, 100).
fn document() -> (Document, History, BoardId) {
    let mut doc = Document::empty("Board in a box");
    let mut ps = Element::part_studio("Part Studio 1");
    ps.id = STUDIO;
    let mut pcb = Element::pcb_studio("PCB Studio 1");
    pcb.id = PCB;
    doc.elements.push(ps);
    doc.elements.push(pcb);
    let mut h = History::default();
    run(&mut doc, &mut h, &AddBoard { element: PCB, name: Some("Course".into()), design: Box::new(course()), imported_from: None });
    let id = doc.element(PCB).unwrap().pcb().unwrap().boards[0].id;
    let mc = MateConnectorFeature { origin: Some(ConnectorOrigin::Origin), offset: [0.0, 0.0, 100.0], offset_expr: ["0 mm", "0 mm", "100 mm"].map(String::from), ..Default::default() };
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: LOC, base_name: "Mate connector".into(), kind: FeatureKind::MateConnector(mc) });
    (doc, h, id)
}

/// Derives the board into the Part Studio with `d`'s options.
fn derive(doc: &mut Document, h: &mut History, board: BoardId, mut d: DerivedFeature) -> Result<(), cadrs_core::CommandError> {
    d.board.get_or_insert_with(|| Box::new(DerivedBoard::new(board, "", Default::default()))).board = board;
    let mut res = Resolver::new(Store::new(std::env::temp_dir().join("cadrs-derived-board-unused")));
    let r = SourceRef { document: None, at: RefAt::Workspace, element: PCB, pinned: false };
    let got = derived::resolve(&mut res, doc, None, d, r).map_err(cadrs_core::CommandError::from)?;
    h.execute(doc, &AddDerived { element: STUDIO, feature: DERIVED, derived: got.derived, links: got.links })
}

fn build(doc: &Document) -> std::sync::Arc<cadrs_core::rebuild::Build> {
    cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features())
}

fn bounds(p: &Part) -> ([f64; 3], [f64; 3]) {
    let mut lo = [f64::MAX; 3];
    let mut hi = [f64::MIN; 3];
    for q in &p.solid.positions {
        for k in 0..3 {
            lo[k] = lo[k].min(q[k]);
            hi[k] = hi[k].max(q[k]);
        }
    }
    (lo, hi)
}

fn derived_parts(b: &cadrs_core::rebuild::Build) -> Vec<&Part> {
    b.parts.iter().filter(|p| p.feature == DERIVED).collect()
}

#[test]
fn the_board_and_its_components_come_in_at_the_location() {
    let (mut doc, mut h, board) = document();
    let d = DerivedFeature { locations: vec![ConnectorRef::Feature(LOC)], ..Default::default() };
    derive(&mut doc, &mut h, board, d).unwrap();
    let b = build(&doc);
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let parts = derived_parts(&b);
    let names: Vec<&str> = parts.iter().map(|p| p.name.as_str()).collect();
    assert_eq!(names.len(), 4, "{names:?}");
    assert_eq!(names[0], "Course");
    for r in ["BT1", "D1", "R1"] {
        assert!(names.iter().any(|n| n.starts_with(&format!("{r} "))), "{r} in {names:?}");
    }
    // The board: 45 × 50 × 1.6, its outline's centre on the connector, bottom face on z = 100.
    let board_part = parts[0];
    let (lo, hi) = bounds(board_part);
    assert!((hi[0] - lo[0] - 45.0).abs() < 1e-6 && (hi[1] - lo[1] - 50.0).abs() < 1e-6, "{lo:?} {hi:?}");
    assert!((lo[0] + 22.5).abs() < 1e-6 && (lo[1] + 25.0).abs() < 1e-6);
    assert!((lo[2] - 100.0).abs() < 1e-9 && (hi[2] - 101.6).abs() < 1e-9);
    // Its volume: the outline's area times the thickness, less the holes through it.
    let layout = &course().board;
    let holes: f64 = cadrs_eda::fab::holes(layout).iter().filter(|h| to_mm(h.size.w) >= cadrs_core::pcb::board3d::MIN_HOLE).map(|h| std::f64::consts::PI * (to_mm(h.size.w) / 2.0).powi(2)).sum();
    let v = board_part.mass.unwrap().volume;
    assert!((v - (45.0 * 50.0 - holes) * 1.6).abs() < 1e-3, "volume {v}");
    // The components are mesh parts (their volume from their closed meshes), on their side of
    // the board.
    let d1 = parts.iter().find(|p| p.name.starts_with("D1 ")).unwrap();
    assert!(d1.mass.is_some_and(|m| m.volume > 1.0), "{:?}", d1.mass);
    assert!(bounds(d1).1[2] > 101.6 + 3.0);
    let bt1 = parts.iter().find(|p| p.name.starts_with("BT1 ")).unwrap();
    assert!(bounds(bt1).0[2] < 100.0 - 1.0, "BT1 hangs below");
    // The board's skins ride on it.
    assert_eq!(board_part.solid.images.len(), 2);
    // What the Feature list and the edit mode read.
    let out = &b.derived[&DERIVED];
    assert_eq!(out.boards.len(), 1);
    assert_eq!(out.footprints.len(), 3);
    let f = out.boards[0].frame;
    assert!((f.origin[0] - (0.0 - 22.5)).abs() < 1e-9 && (f.origin[2] - 100.0).abs() < 1e-9, "{f:?}");
}

#[test]
fn skins_missing_from_the_blob_cache_are_made_again() {
    // A rebuild restored from the last run's snapshot names skins only that run made: the
    // cached output showing them is made again, and the skins with it.
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let skins_of = |b: &cadrs_core::rebuild::Build| derived_parts(b)[0].solid.images.iter().map(|i| i.blob.clone()).collect::<Vec<_>>();
    let before = skins_of(&build(&doc));
    assert_eq!(before.len(), 2);
    for blob in &before {
        cadrs_core::blobs::forget(blob);
    }
    let after = skins_of(&build(&doc));
    assert_eq!(after, before, "the same pictures");
    assert!(after.iter().all(|b| cadrs_core::blobs::contains(b)), "loaded again");
}

#[test]
fn layout_edits_follow_and_keep_part_and_face_ids() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let before = build(&doc);
    let d1 = derived_parts(&before).into_iter().find(|p| p.name.starts_with("D1 ")).unwrap().clone();
    // A mate connector on D1's top face keeps its place on it.
    let top = d1.solid.faces.iter().filter(|f| f.plane.is_some_and(|p| p.normal()[2] > 0.99)).max_by(|a, b| a.center.unwrap()[2].total_cmp(&b.center.unwrap()[2])).unwrap();
    let face = cadrs_core::document::FaceRef { part: d1.id, face: top.name, seed: top.center.unwrap() };
    let mc = MateConnectorFeature { origin: Some(ConnectorOrigin::Face(face)), ..Default::default() };
    let on_d1 = FeatureId::new();
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: on_d1, base_name: "Mate connector".into(), kind: FeatureKind::MateConnector(mc) });
    let at = build(&doc).connectors[&on_d1];
    // Move D1 5 mm along x in the PCB Studio.
    let mut design = doc.element(PCB).unwrap().pcb().unwrap().board(board).unwrap().design().unwrap().clone();
    let i = cadrs_eda::board_edit::footprint_index(&design.board, "D1").unwrap();
    let to = design.board.footprints[i].placement.at + cadrs_eda::units::Pt::new(mm(5.0), 0);
    cadrs_eda::board_edit::move_footprint(&mut design.board, i, to);
    run(&mut doc, &mut h, &SetDesign { element: PCB, board, design: Box::new(design), label: "Move D1".into() });
    // The Derived feature follows (one command, refreshed by the history).
    let after = build(&doc);
    assert!(after.errors.is_empty(), "{:?}", after.errors);
    let moved = after.part(d1.id).expect("D1 keeps its part id");
    let names = |p: &Part| p.solid.faces.iter().map(|f| f.name).collect::<Vec<_>>();
    assert_eq!(names(moved), names(&d1), "and its face names");
    let (a, b) = (bounds(&d1), bounds(moved));
    assert!((b.0[0] - a.0[0] - 5.0).abs() < 1e-6 && (b.0[1] - a.0[1]).abs() < 1e-6);
    let now = after.connectors[&on_d1];
    assert!((now.origin[0] - at.origin[0] - 5.0).abs() < 1e-6 && (now.origin[1] - at.origin[1]).abs() < 1e-6, "{at:?} → {now:?}");
    // Undo puts it back.
    h.undo(&mut doc).unwrap();
    let back = build(&doc);
    assert!((bounds(back.part(d1.id).unwrap()).0[0] - a.0[0]).abs() < 1e-6);
}

#[test]
fn options_filter_and_seat_the_board() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let FeatureKind::Derived(mut d) = doc.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind.clone() else { unreachable!() };
    let o = &mut d.board.as_mut().unwrap().options;
    o.components = ComponentFilter::Top;
    o.seat = BoardSeat::Top;
    o.anchor = BoardAnchor::Origin;
    run(&mut doc, &mut h, &SetDerived { element: STUDIO, feature: DERIVED, derived: *d, links: vec![], label: "Edit derived".into() });
    let b = build(&doc);
    let parts = derived_parts(&b);
    assert_eq!(parts.len(), 3, "the board, D1 and R1");
    // The top face on the origin's plane: the board below it; the layout's origin on the origin.
    let (lo, hi) = bounds(parts[0]);
    assert!((hi[2]).abs() < 1e-9 && (lo[2] + 1.6).abs() < 1e-9, "{lo:?} {hi:?}");
    let region = cadrs_eda::outline::board_region(&course().board);
    let xmin = region.iter().flatten().map(|p| to_mm(p.x)).fold(f64::MAX, f64::min);
    assert!((lo[0] - xmin).abs() < 1e-6);
}

#[test]
fn a_board_is_derived_once_per_part_studio() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let mut res = Resolver::new(Store::new(std::env::temp_dir().join("cadrs-derived-board-unused")));
    let r = SourceRef { document: None, at: RefAt::Workspace, element: PCB, pinned: false };
    let d = DerivedFeature { board: Some(Box::new(DerivedBoard::new(board, "", Default::default()))), ..Default::default() };
    let got = derived::resolve(&mut res, &doc, None, d, r).unwrap();
    let e = h.execute(&mut doc, &AddDerived { element: STUDIO, feature: FeatureId::new(), derived: got.derived, links: vec![] }).unwrap_err();
    assert!(e.to_string().contains("derived only once"), "{e}");
}

#[test]
fn renaming_the_board_renames_the_part_and_the_source() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    run(&mut doc, &mut h, &cadrs_core::pcb::RenameBoard { element: PCB, board, name: "Main board".into() });
    let FeatureKind::Derived(d) = &doc.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind else { unreachable!() };
    assert_eq!(d.describe(), "PCB Studio 1 › Main board");
    assert_eq!(derived_parts(&build(&doc))[0].name, "Main board");
}

#[test]
fn the_enclosure_sample_builds_with_its_named_parts() {
    use cadrs_core::samples::pcb_enclosure as pe;
    let doc = pe::document().unwrap();
    let b = cadrs_core::rebuild::build(doc.element(pe::STUDIO).unwrap().features());
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let props = doc.element(pe::STUDIO).unwrap().part_props();
    let names: Vec<&str> = b.parts.iter().map(|p| cadrs_core::parts::display_name(p, props)).collect();
    for n in ["Tube", "End plate", "Plug", "Live pin", "Earth pin", "Neutral pin"] {
        assert!(names.contains(&n), "{n} in {names:?}");
    }
    let loc = b.connectors[&pe::BOARD_LOCATION];
    assert!((loc.origin[2] - pe::BOARD_Z).abs() < 1e-9);
    assert_eq!(doc.element(pe::PCB).unwrap().pcb().unwrap().boards[0].name(), "AC sensor");
}

#[test]
fn the_enclosure_board_derives_cleanly() {
    use cadrs_core::samples::pcb_enclosure as pe;
    let mut doc = pe::document().unwrap();
    let mut h = History::default();
    let board = doc.element(pe::PCB).unwrap().pcb().unwrap().boards[0].id;
    let d = DerivedFeature { locations: vec![ConnectorRef::Feature(pe::BOARD_LOCATION)], board: Some(Box::new(DerivedBoard::new(board, "", Default::default()))), ..Default::default() };
    let mut res = Resolver::new(Store::new(std::env::temp_dir().join("cadrs-derived-board-unused")));
    let r = SourceRef { document: None, at: RefAt::Workspace, element: pe::PCB, pinned: false };
    let got = derived::resolve(&mut res, &doc, None, d, r).unwrap();
    h.execute(&mut doc, &AddDerived { element: pe::STUDIO, feature: DERIVED, derived: got.derived, links: vec![] }).unwrap();
    let b = cadrs_core::rebuild::build(doc.element(pe::STUDIO).unwrap().features());
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    // The large module under the board runs into the end plate, the plug and the three pins
    // (that is what the scenarios fix); nothing else does (U2 sits on a pin's tip: touching).
    let out = &b.derived[&DERIVED];
    let props = doc.element(pe::STUDIO).unwrap().part_props();
    let name = |p: cadrs_core::PartId| b.part(p).map(|x| cadrs_core::parts::display_name(x, props).to_string()).unwrap_or_default();
    let mut hits: Vec<(String, String)> = out.collisions.iter().map(|c| (name(c.component), name(c.other))).collect();
    hits.sort();
    let want: Vec<(String, String)> = ["Earth pin", "End plate", "Live pin", "Neutral pin", "Plug"].iter().map(|o| ("PS1 HLK-PM01".to_string(), o.to_string())).collect();
    assert_eq!(hits, want);
    assert!(b.warning(DERIVED).is_some_and(|w| w.contains("PS1 HLK-PM01 runs into")));
    // Its bottom face on Board location, 1.6 mm below the pins' tips.
    let out = &b.derived[&DERIVED];
    assert!((out.boards[0].frame.origin[2] - pe::BOARD_Z).abs() < 1e-9);
}

/// The course board's design in the PCB Studio, as a copy to edit.
fn design_of(doc: &Document, board: BoardId) -> cadrs_eda::Design {
    doc.element(PCB).unwrap().pcb().unwrap().board(board).unwrap().design().unwrap().clone()
}

fn set_design(doc: &mut Document, h: &mut History, board: BoardId, design: cadrs_eda::Design) {
    run(doc, h, &SetDesign { element: PCB, board, design: Box::new(design), label: "Edit".into() });
}

#[test]
fn turning_flipping_and_renaming_a_footprint_keeps_its_part_and_faces() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let before = build(&doc);
    let r1 = derived_parts(&before).into_iter().find(|p| p.name.starts_with("R1 ")).unwrap().clone();
    let names = |p: &Part| p.solid.faces.iter().map(|f| f.name).collect::<Vec<_>>();
    // R1 turned a quarter turn, then flipped to the bottom, then renamed R7.
    let mut d = design_of(&doc, board);
    let i = cadrs_eda::board_edit::footprint_index(&d.board, "R1").unwrap();
    cadrs_eda::board_edit::rotate_footprint(&mut d.board, i, 90.0);
    set_design(&mut doc, &mut h, board, d.clone());
    let turned = build(&doc).part(r1.id).expect("still there after turning").clone();
    assert_eq!(names(&turned), names(&r1));
    cadrs_eda::board_edit::flip_footprint(&mut d.board, i);
    set_design(&mut doc, &mut h, board, d.clone());
    let flipped = build(&doc).part(r1.id).expect("still there after flipping").clone();
    assert_eq!(names(&flipped), names(&r1));
    // On the bottom: its body under the board (a through-hole resistor: only its leads come up
    // through it).
    // Its body (4.8 mm up) goes under (to 1.6 − 4.8 = −3.2), its leads' ends (1.4 below the
    // bottom face) come up 1.4 above the top face (to 3.0).
    let (a, b) = (bounds(&r1), bounds(&flipped));
    assert!((a.1[2] - 4.8).abs() < 1e-3 && (b.0[2] + 3.2).abs() < 1e-3 && (b.1[2] - 3.0).abs() < 1e-3, "{a:?} → {b:?}");
    if let Some(f) = d.board.footprints[i].footprint.field_mut(cadrs_eda::symbol::fields::REFERENCE) {
        f.text.text.text = "R7".into();
    }
    set_design(&mut doc, &mut h, board, d);
    let b = build(&doc);
    let renamed = b.part(r1.id).expect("still there after re-annotating");
    assert!(renamed.name.starts_with("R7 "), "{}", renamed.name);
    assert_eq!(names(renamed), names(&r1));
}

#[test]
fn a_new_hole_renames_no_board_face() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let b0 = build(&doc);
    let board_part = derived_parts(&b0)[0].clone();
    // Every side face of the board with where its middle is.
    // (The caps keep their names but not their centroids: a hole moves those.)
    let sides = |p: &Part| -> Vec<(cadrs_kernel::naming::FaceName, [f64; 3])> {
        p.solid.faces.iter().filter(|f| f.plane.is_none_or(|pl| pl.normal()[2].abs() < 0.5)).filter(|f| !matches!(f.name.origin, cadrs_kernel::naming::FaceOrigin::Imported { .. })).map(|f| (f.name, f.center.unwrap())).collect()
    };
    let before = sides(&board_part);
    assert!(before.len() > 4, "outline sides and hole walls named");
    // A mounting hole (a new through-hole footprint) near a corner.
    let lib = cadrs_eda::library::LibraryTable::builtin();
    let mut d = design_of(&doc, board);
    let mut fp = cadrs_eda::ac_board::wire_pad();
    fp.models.clear();
    d.board.footprints.push(cadrs_eda::board::PlacedFootprint { id: uuid::Uuid::from_u128(77), footprint: fp, placement: cadrs_eda::footprint::FootprintPlacement { at: cadrs_eda::units::Pt::mm(5.0, 5.0), ..Default::default() }, locked: false, symbol: None });
    let _ = lib;
    set_design(&mut doc, &mut h, board, d);
    let after = sides(build(&doc).part(board_part.id).unwrap());
    assert_eq!(after.len(), before.len() + 1, "one more wall");
    // Every face there before has the same name and the same place.
    for (n, c) in &before {
        let found = after.iter().find(|(m, _)| m == n).unwrap_or_else(|| panic!("{n:?} lost"));
        assert!((0..3).all(|k| (found.1[k] - c[k]).abs() < 1e-6));
    }
}

#[test]
fn a_document_with_a_derived_board_reads_back_the_same() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature { locations: vec![ConnectorRef::Feature(LOC)], ..Default::default() }).unwrap();
    let text = ron::to_string(&doc).unwrap();
    let back: Document = ron::from_str(&text).unwrap();
    let get = |d: &Document| match &d.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind {
        FeatureKind::Derived(x) => (**x.board.as_ref().unwrap()).clone(),
        _ => unreachable!(),
    };
    let (a, b) = (get(&doc), get(&back));
    assert_eq!(a.hash, b.hash, "the hash is worked out again");
    assert_ne!(b.hash, 0);
    assert_eq!(a, b);
    let parts = |d: &Document| derived_parts(&build(d)).iter().map(|p| (p.id, p.name.clone())).collect::<Vec<_>>();
    assert_eq!(parts(&doc), parts(&back));
}

#[test]
fn a_version_reference_stays_as_the_version() {
    use cadrs_core::history_log::HistoryLog;
    let (mut doc, mut h, board) = document();
    let store = Store::new(std::env::temp_dir().join(format!("cadrs-derived-board-v-{}", uuid::Uuid::new_v4())));
    store.create(&doc, &cadrs_core::DocumentMeta::new("me", 1_000)).unwrap();
    let mut log = HistoryLog::start(&doc, 1_000, "me");
    let v1 = log.create_version("V1", "", 1_001, "me");
    let mut res = Resolver::new(store.clone());
    let d = DerivedFeature { board: Some(Box::new(DerivedBoard::new(board, "", Default::default()))), ..Default::default() };
    let got = derived::resolve(&mut res, &doc, Some(&log), d, SourceRef::version(None, PCB, v1)).unwrap();
    run(&mut doc, &mut h, &AddDerived { element: STUDIO, feature: DERIVED, derived: got.derived, links: got.links });
    let d1 = |doc: &Document| derived_parts(&build(doc)).into_iter().find(|p| p.name.starts_with("D1 ")).map(|p| bounds(p).0);
    let at = d1(&doc).unwrap();
    // The workspace moves D1; the version reference doesn't follow.
    let mut d = design_of(&doc, board);
    let i = cadrs_eda::board_edit::footprint_index(&d.board, "D1").unwrap();
    let to = d.board.footprints[i].placement.at + cadrs_eda::units::Pt::new(mm(5.0), 0);
    cadrs_eda::board_edit::move_footprint(&mut d.board, i, to);
    set_design(&mut doc, &mut h, board, d);
    assert_eq!(d1(&doc).unwrap(), at);
    let FeatureKind::Derived(x) = &doc.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind else { unreachable!() };
    assert!(x.copy.is_some(), "a frozen copy");
    assert_eq!(x.version_name, "V1");
}

#[test]
fn switching_the_view_keeps_the_rebuild_key() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let key = |doc: &Document| format!("{:?}", doc.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind);
    let before = key(&doc);
    let FeatureKind::Derived(mut d) = doc.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind.clone() else { unreachable!() };
    d.board.as_mut().unwrap().options.view = cadrs_core::pcb::board3d::BoardView::Layout;
    run(&mut doc, &mut h, &SetDerived { element: STUDIO, feature: DERIVED, derived: *d, links: vec![], label: "Show layout".into() });
    // The rebuild's cache key is the feature's Debug text: the same, so nothing rebuilds.
    assert_eq!(key(&doc), before);
    h.undo(&mut doc).unwrap();
    let FeatureKind::Derived(d) = &doc.element(STUDIO).unwrap().feature(DERIVED).unwrap().kind else { unreachable!() };
    assert_eq!(d.board.as_ref().unwrap().options.view, cadrs_core::pcb::board3d::BoardView::ThreeD, "undone");
}

#[test]
fn a_transform_moves_the_board_and_its_components_together() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let b0 = build(&doc);
    let parts: Vec<Part> = derived_parts(&b0).into_iter().cloned().collect();
    let t = cadrs_core::transform::TransformFeature { transform_type: cadrs_core::transform::TransformType::TranslateXyz, parts: parts.iter().map(|p| p.id).collect(), dz: 20.0, dz_expr: "20 mm".into(), ..Default::default() };
    let tf = FeatureId::new();
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: tf, base_name: "Transform".into(), kind: FeatureKind::Transform(t) });
    let b1 = build(&doc);
    assert!(b1.errors.is_empty() && b1.warning(tf).is_none(), "{:?} {:?}", b1.errors, b1.warnings);
    for p in &parts {
        let moved = b1.part(p.id).unwrap();
        assert!((bounds(moved).0[2] - bounds(p).0[2] - 20.0).abs() < 1e-6, "{} moved up 20", p.name);
    }
    // Picking the board alone moves its components too (undo, then the board part only).
    h.undo(&mut doc).unwrap();
    let board_part = b0.derived[&DERIVED].boards[0].part;
    let t = cadrs_core::transform::TransformFeature { transform_type: cadrs_core::transform::TransformType::TranslateXyz, parts: vec![board_part], dz: 20.0, dz_expr: "20 mm".into(), ..Default::default() };
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: tf, base_name: "Transform".into(), kind: FeatureKind::Transform(t) });
    let b1 = build(&doc);
    for p in &parts {
        let moved = b1.part(p.id).unwrap();
        assert!((bounds(moved).0[2] - bounds(p).0[2] - 20.0).abs() < 1e-6, "{} moved up 20 with the board", p.name);
    }
    // The board's frame (its layout's place) went with it.
    let (f0, f1) = (b0.derived.values().next().unwrap().boards[0].frame, b1.derived.values().next().unwrap().boards[0].frame);
    assert!((f1.origin[2] - f0.origin[2] - 20.0).abs() < 1e-9 && f1.u == f0.u && f1.v == f0.v, "{f0:?} → {f1:?}");
}

#[test]
fn a_part_studio_deriving_one_with_a_board_brings_its_components() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let mut second = Element::part_studio("Part Studio 2");
    let host = ElementId::new();
    second.id = host;
    doc.elements.push(second);
    let mut res = Resolver::new(Store::new(std::env::temp_dir().join("cadrs-derived-board-unused")));
    let r = SourceRef { document: None, at: RefAt::Workspace, element: STUDIO, pinned: false };
    let got = derived::resolve(&mut res, &doc, None, DerivedFeature::default(), r).unwrap();
    let f = FeatureId::new();
    run(&mut doc, &mut h, &AddDerived { element: host, feature: f, derived: got.derived, links: vec![] });
    let b = cadrs_core::rebuild::build(doc.element(host).unwrap().features());
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let names: Vec<&str> = b.parts.iter().map(|p| p.name.as_str()).collect();
    for r in ["BT1", "D1", "R1"] {
        assert!(names.iter().any(|n| n.starts_with(&format!("{r} "))), "{r} in {names:?}");
    }
    let out = &b.derived[&f];
    assert_eq!(out.boards.len(), 1);
    assert_eq!(out.footprints.len(), 3);
}

#[test]
fn parts_waiting_off_the_board_stay_out_unless_asked() {
    use cadrs_core::samples::pcb_enclosure as pe;
    let mut doc = pe::document().unwrap();
    let mut h = History::default();
    let board = doc.element(pe::PCB).unwrap().pcb().unwrap().boards[0].id;
    let mut res = Resolver::new(Store::new(std::env::temp_dir().join("cadrs-derived-board-unused")));
    let r = SourceRef { document: None, at: RefAt::Workspace, element: pe::PCB, pinned: false };
    let d = DerivedFeature { board: Some(Box::new(DerivedBoard::new(board, "", Default::default()))), ..Default::default() };
    let got = derived::resolve(&mut res, &doc, None, d, r).unwrap();
    h.execute(&mut doc, &AddDerived { element: pe::STUDIO, feature: DERIVED, derived: got.derived, links: vec![] }).unwrap();
    let names = |doc: &Document| cadrs_core::rebuild::build(doc.element(pe::STUDIO).unwrap().features()).parts.iter().filter(|p| p.feature == DERIVED).map(|p| p.name.clone()).collect::<Vec<_>>();
    assert!(!names(&doc).iter().any(|n| n.starts_with("U1 ")), "U1 waits off the board");
    let FeatureKind::Derived(mut x) = doc.element(pe::STUDIO).unwrap().feature(DERIVED).unwrap().kind.clone() else { unreachable!() };
    x.board.as_mut().unwrap().options.off_board = true;
    h.execute(&mut doc, &SetDerived { element: pe::STUDIO, feature: DERIVED, derived: *x, links: vec![], label: "x".into() }).unwrap();
    assert!(names(&doc).iter().any(|n| n.starts_with("U1 ")));
}

#[test]
fn the_fixed_point_keeps_the_parts_where_they_are_when_the_outline_changes() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let d1 = |doc: &Document| derived_parts(&build(doc)).into_iter().find(|p| p.name.starts_with("D1 ")).map(|p| bounds(p).0).unwrap();
    let at = d1(&doc);
    // The outline grows 10 mm to the right: its centre moves, the parts don't.
    let mut d = design_of(&doc, board);
    let region = cadrs_eda::outline::board_region(&d.board);
    let bb = region.iter().flatten().fold(None, |acc, p| Some(cadrs_eda::units::Bounds::union(acc, cadrs_eda::units::Bounds::of(*p)))).unwrap();
    d.board.shapes.retain(|s| s.layer != cadrs_eda::layer::Layer::Outline);
    cadrs_eda::outline::add_rect(&mut d.board, bb.min, bb.max + cadrs_eda::units::Pt::new(mm(10.0), 0));
    set_design(&mut doc, &mut h, board, d);
    let now = d1(&doc);
    assert!((0..3).all(|k| (now[k] - at[k]).abs() < 1e-6), "{at:?} → {now:?}");
}

// ---------------------------------------------------------------------------------------------
// P5: a board's outline from a Part Studio sketch

const OUTLINE_SKETCH: FeatureId = FeatureId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0010);
const FROM_SKETCH: FeatureId = FeatureId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0011);
const NEW_PCB: ElementId = ElementId::from_u128(0x7c00_0000_0000_0000_0000_0000_0000_0012);

/// A Part Studio with "Sketch 1" on Top: a 40 × 30 rectangle from (−20, −10) with a ⌀6 hole at
/// (5, 5).
fn sketched() -> (Document, History) {
    use cadrs_core::commands::{AddSketch, EditSketch};
    use cadrs_sketch::{SketchOp, Vec2};
    let mut doc = Document::empty("Board from a sketch");
    let mut ps = Element::part_studio("Part Studio 1");
    ps.id = STUDIO;
    doc.elements.push(ps);
    let mut h = History::default();
    run(&mut doc, &mut h, &AddSketch { element: STUDIO, feature: OUTLINE_SKETCH, plane: Some(cadrs_sketch::PlaneRef::Top) });
    let rect = SketchOp::AddPolyline { points: vec![Vec2::new(-20.0, -10.0), Vec2::new(20.0, -10.0), Vec2::new(20.0, 20.0), Vec2::new(-20.0, 20.0)], closed: true, construction: false, label: "Add rectangle" };
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: rect });
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::AddCircle { center: Vec2::new(5.0, 5.0), radius: 3.0, construction: false } });
    (doc, h)
}

fn outline_regions(doc: &Document) -> Vec<cadrs_core::document::RegionRef> {
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    cadrs_core::samples::region_refs(OUTLINE_SKETCH, &g, &[cadrs_sketch::Vec2::new(-15.0, 0.0)])
}

fn create_from_sketch(doc: &mut Document, h: &mut History) {
    let regions = outline_regions(doc);
    run(doc, h, &cadrs_core::pcb::outline_link::CreateBoardFromSketch { element: STUDIO, sketch: OUTLINE_SKETCH, regions, pcb: None, new_pcb: NEW_PCB, name: "Main board".into(), thickness: 1.6, seat: cadrs_core::pcb::board3d::BoardSeat::Bottom, circles_as_holes: false, feature: FROM_SKETCH });
}

fn board_area(doc: &Document) -> f64 {
    let b = &doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0];
    cadrs_eda::poly::area(&cadrs_eda::outline::board_region(&b.design().unwrap().board)) / 1e12
}

#[test]
fn a_board_made_from_a_sketch_lies_on_it() {
    let (mut doc, mut h) = sketched();
    create_from_sketch(&mut doc, &mut h);
    // A new PCB Studio with the board: the rectangle less the hole.
    let hole = std::f64::consts::PI * 9.0;
    assert!((board_area(&doc) - (40.0 * 30.0 - hole)).abs() < 0.1, "{}", board_area(&doc));
    assert_eq!(doc.element(NEW_PCB).unwrap().name, "PCB Studio 1");
    // The Derived feature puts it on the sketch: x −20…20, y −10…20, z 0…1.6.
    let b = cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features());
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let board = b.parts.iter().find(|p| p.feature == FROM_SKETCH).unwrap();
    let (lo, hi) = bounds(board);
    for (k, want) in [(0, -20.0), (1, -10.0), (2, 0.0)] {
        assert!((lo[k] - want).abs() < 1e-6, "{lo:?}");
    }
    for (k, want) in [(0, 20.0), (1, 20.0), (2, 1.6)] {
        assert!((hi[k] - want).abs() < 1e-6, "{hi:?}");
    }
    // One undo step takes it all back.
    h.undo(&mut doc).unwrap();
    assert!(doc.element(NEW_PCB).is_none());
    assert!(doc.element(STUDIO).unwrap().feature(FROM_SKETCH).is_none());
}

#[test]
fn the_outline_follows_the_sketch_until_the_link_is_broken() {
    use cadrs_core::commands::EditSketch;
    use cadrs_sketch::{SketchOp, Vec2};
    let (mut doc, mut h) = sketched();
    create_from_sketch(&mut doc, &mut h);
    // Widen the rectangle 10 mm to the right: its two right corners move.
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let corner = |x: f64, y: f64| g.point_at(Vec2::new(x, y), 1e-6).unwrap();
    let moves = vec![(corner(20.0, -10.0), Vec2::new(30.0, -10.0)), (corner(20.0, 20.0), Vec2::new(30.0, 20.0))];
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::MovePoints { moves } });
    let hole = std::f64::consts::PI * 9.0;
    assert!((board_area(&doc) - (50.0 * 30.0 - hole)).abs() < 0.1, "{}", board_area(&doc));
    // The derived board follows in the same step.
    let b = cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features());
    let board = b.parts.iter().find(|p| p.feature == FROM_SKETCH).unwrap();
    assert!((bounds(board).1[0] - 30.0).abs() < 1e-6);
    // The linked shapes are locked in the layout.
    let pcb = doc.element(NEW_PCB).unwrap().pcb().unwrap();
    let sb = &pcb.boards[0];
    let link = sb.outline_link.as_ref().unwrap();
    assert!(sb.design().unwrap().board.shapes.iter().filter(|s| link.shapes.contains(&s.id)).all(|s| s.locked));
    // Break the link: the shapes stay, unlocked, and no longer follow.
    let board_id = sb.id;
    run(&mut doc, &mut h, &cadrs_core::pcb::outline_link::SetOutlineLink { pcb: NEW_PCB, board: board_id, link: None, keep_parts: false });
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let corner = |x: f64, y: f64| g.point_at(Vec2::new(x, y), 1e-6).unwrap();
    let moves = vec![(corner(30.0, -10.0), Vec2::new(40.0, -10.0)), (corner(30.0, 20.0), Vec2::new(40.0, 20.0))];
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::MovePoints { moves } });
    assert!((board_area(&doc) - (50.0 * 30.0 - hole)).abs() < 0.1, "no longer follows: {}", board_area(&doc));
    let sb = &doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0];
    assert!(sb.outline_link.is_none());
    assert!(sb.design().unwrap().board.shapes.iter().filter(|s| s.layer == cadrs_eda::layer::Layer::Outline).all(|s| !s.locked));
}

#[test]
fn a_gone_region_keeps_the_last_outline_and_says_why() {
    use cadrs_core::commands::EditSketch;
    use cadrs_sketch::SketchOp;
    let (mut doc, mut h) = sketched();
    create_from_sketch(&mut doc, &mut h);
    let before = board_area(&doc);
    // Delete every curve of the sketch.
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let curves: Vec<_> = g.curves.keys().collect();
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::Delete { curves, points: vec![], dimensions: vec![], constraints: vec![] } });
    assert!((board_area(&doc) - before).abs() < 1e-9);
    let sb = &doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0];
    assert!(sb.outline_link.as_ref().unwrap().problem.is_some());
}

#[test]
fn an_outline_cant_follow_a_sketch_after_the_board() {
    use cadrs_core::commands::AddSketch;
    let (mut doc, mut h) = sketched();
    create_from_sketch(&mut doc, &mut h);
    // A second sketch after the Derived feature (it could be drawn on the board).
    let late = FeatureId::new();
    run(&mut doc, &mut h, &AddSketch { element: STUDIO, feature: late, plane: Some(cadrs_sketch::PlaneRef::Front) });
    let board = doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0].id;
    let e = h.execute(&mut doc, &cadrs_core::pcb::outline_link::SetOutlineLink { pcb: NEW_PCB, board, link: Some((STUDIO, late, vec![])), keep_parts: false }).unwrap_err();
    assert!(e.to_string().contains("comes after"), "{e}");
}

/// A face of `p` whose normal is `n` (the highest one along it).
fn face_facing(p: &Part, n: [f64; 3]) -> cadrs_core::document::FaceRef {
    let along = |f: &&cadrs_core::solid::SolidFace| f.center.map_or(f64::MIN, |c| c[0] * n[0] + c[1] * n[1] + c[2] * n[2]);
    let f = p.solid.faces.iter().filter(|f| f.plane.is_some_and(|q| { let m = q.normal(); m[0] * n[0] + m[1] * n[1] + m[2] * n[2] > 0.99 })).max_by(|a, b| along(a).total_cmp(&along(b))).unwrap();
    cadrs_core::document::FaceRef { part: p.id, face: f.name, seed: f.center.unwrap() }
}

#[test]
fn a_mate_connector_on_a_component_turns_and_flips_with_it() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let before = build(&doc);
    let d1 = derived_parts(&before).into_iter().find(|p| p.name.starts_with("D1 ")).unwrap().clone();
    let face = face_facing(&d1, [0.0, 0.0, 1.0]);
    let on_d1 = FeatureId::new();
    let mc = MateConnectorFeature { origin: Some(ConnectorOrigin::Face(face)), ..Default::default() };
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: on_d1, base_name: "Mate connector".into(), kind: FeatureKind::MateConnector(mc) });
    let at = build(&doc).connectors[&on_d1];
    let mut d = design_of(&doc, board);
    let i = cadrs_eda::board_edit::footprint_index(&d.board, "D1").unwrap();
    let centre = d.board.footprints[i].placement.at;
    // A quarter turn about the footprint's origin: the connector stays on the top face (up),
    // its place turned about the footprint's.
    cadrs_eda::board_edit::rotate_footprint(&mut d.board, i, 90.0);
    set_design(&mut doc, &mut h, board, d.clone());
    let b = build(&doc);
    assert!(b.errors.is_empty() && b.warning(on_d1).is_none(), "{:?} {:?}", b.errors, b.warnings);
    let turned = b.connectors[&on_d1];
    let (cx, cy) = (to_mm(centre.x), to_mm(centre.y));
    let o = |f: &cadrs_sketch::PlaneFrame| {
        // The board's origin is the location's (0, 0, 100); its anchor the fixed outline centre.
        let lay = b.derived[&DERIVED].boards[0].frame;
        cadrs_core::pcb::frames::to_board(&lay, f.origin)
    };
    let (a0, a1) = (o(&at), o(&turned));
    let r0 = [a0[0] - cx, a0[1] - cy];
    let r1 = [a1[0] - cx, a1[1] - cy];
    assert!((r1[0] + r0[1]).abs() < 1e-3 && (r1[1] - r0[0]).abs() < 1e-3 && (a1[2] - a0[2]).abs() < 1e-6, "{a0:?} → {a1:?} about ({cx}, {cy})");
    assert!(turned.normal()[2] > 0.99);
    // Flipped to the bottom: the face now looks down, under the board.
    cadrs_eda::board_edit::flip_footprint(&mut d.board, i);
    set_design(&mut doc, &mut h, board, d);
    let b = build(&doc);
    assert!(b.errors.is_empty() && b.warning(on_d1).is_none(), "{:?} {:?}", b.errors, b.warnings);
    let flipped = b.connectors[&on_d1];
    assert!(flipped.normal()[2] < -0.99, "{flipped:?}");
    assert!(o(&flipped)[2] < 0.0, "under the board: {:?}", o(&flipped));
}

#[test]
fn modelling_features_leave_components_alone_and_say_so() {
    use cadrs_core::applied::{EdgeOrFace, FilletFeature};
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let b = build(&doc);
    let d1 = derived_parts(&b).into_iter().find(|p| p.name.starts_with("D1 ")).unwrap().clone();
    let pcb = derived_parts(&b).into_iter().find(|p| p.solid.faces.len() > 6 && b.derived[&DERIVED].boards[0].part == p.id).unwrap().clone();
    // Only a component's face: the fillet fails, saying why.
    let only = FeatureId::new();
    let f = FilletFeature { entities: vec![EdgeOrFace::Face(face_facing(&d1, [0.0, 0.0, 1.0]))], size: 0.2, size_expr: "0.2 mm".into(), ..Default::default() };
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: only, base_name: "Fillet".into(), kind: FeatureKind::Fillet(f) });
    let b = build(&doc);
    let e = b.error(only).unwrap_or_default();
    assert!(e.contains("D1") && e.contains("board component"), "{e:?}");
    h.undo(&mut doc).unwrap();
    // With the board's top face too: the board is rounded, D1 left alone with a warning.
    let both = FeatureId::new();
    let f = FilletFeature { entities: vec![EdgeOrFace::Face(face_facing(&pcb, [0.0, 0.0, 1.0])), EdgeOrFace::Face(face_facing(&d1, [0.0, 0.0, 1.0]))], size: 0.2, size_expr: "0.2 mm".into(), ..Default::default() };
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: both, base_name: "Fillet".into(), kind: FeatureKind::Fillet(f) });
    let b = build(&doc);
    assert!(b.error(both).is_none(), "{:?}", b.error(both));
    let w = b.warning(both).unwrap_or_default();
    assert!(w.contains("Left alone") && w.contains("D1"), "{w:?}");
    assert!(b.part(pcb.id).unwrap().solid.faces.len() > pcb.solid.faces.len(), "the board's top edges rounded");
}

#[test]
fn a_rounded_board_on_front_lies_on_its_sketch_with_arcs() {
    use cadrs_core::commands::{AddSketch, EditSketch};
    use cadrs_sketch::{SketchOp, Vec2};
    let mut doc = Document::empty("Rounded");
    let mut ps = Element::part_studio("Part Studio 1");
    ps.id = STUDIO;
    doc.elements.push(ps);
    let mut h = History::default();
    run(&mut doc, &mut h, &AddSketch { element: STUDIO, feature: OUTLINE_SKETCH, plane: Some(cadrs_sketch::PlaneRef::Front) });
    let rect = SketchOp::AddPolyline { points: vec![Vec2::new(0.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(30.0, 20.0), Vec2::new(0.0, 20.0)], closed: true, construction: false, label: "Add rectangle" };
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: rect });
    // Each corner rounded, r 3.
    for c in [(0.0, 0.0), (30.0, 0.0), (30.0, 20.0), (0.0, 20.0)] {
        let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
        let corner = g.point_at(Vec2::new(c.0, c.1), 1e-6).unwrap();
        run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::Fillet { corner, radius: 3.0, equal_to: None } });
    }
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let regions = cadrs_core::samples::region_refs(OUTLINE_SKETCH, &g, &[Vec2::new(15.0, 10.0)]);
    run(&mut doc, &mut h, &cadrs_core::pcb::outline_link::CreateBoardFromSketch { element: STUDIO, sketch: OUTLINE_SKETCH, regions, pcb: None, new_pcb: NEW_PCB, name: "Rounded".into(), thickness: 1.0, seat: cadrs_core::pcb::board3d::BoardSeat::Top, circles_as_holes: false, feature: FROM_SKETCH });
    // The outline has the arcs (as arcs, not just their chords): 600 − (4 − π) 9.
    let want = 30.0 * 20.0 - (4.0 - std::f64::consts::PI) * 9.0;
    // (The region's arcs are polygons to 1 µm: a little under.)
    assert!((board_area(&doc) - want).abs() < 0.1, "{} vs {want}", board_area(&doc));
    let d = doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0].design().unwrap().board.clone();
    assert_eq!(d.shapes.iter().filter(|s| matches!(s.shape.geom, cadrs_eda::graphics::Geom::Arc { .. })).count(), 4, "four arcs");
    // On Front: sketch x along world x, sketch y along world z; 1 mm thick, its top face on the
    // sketch, so it stands behind it (Front's normal is −y).
    let b = cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features());
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let (lo, hi) = bounds(b.parts.iter().find(|p| p.feature == FROM_SKETCH).unwrap());
    let close = |a: f64, b: f64| (a - b).abs() < 1e-6;
    assert!(close(lo[0], 0.0) && close(hi[0], 30.0) && close(lo[2], 0.0) && close(hi[2], 20.0), "{lo:?} {hi:?}");
    assert!(close(lo[1], 0.0) && close(hi[1], 1.0), "{lo:?} {hi:?}");
}

#[test]
fn a_dimension_edit_resizes_the_board() {
    use cadrs_core::commands::EditSketch;
    use cadrs_sketch::{Dimension, DimensionKind, SketchOp};
    let (mut doc, mut h) = sketched();
    create_from_sketch(&mut doc, &mut h);
    // The mounting hole's diameter dimensioned (6), then set to 10.
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let curve = g.curves.iter().find(|(_, c)| matches!(c.kind, cadrs_sketch::CurveKind::Circle { .. })).map(|(id, _)| id).unwrap();
    let dimension = Dimension { kind: DimensionKind::Diameter { curve }, value: 6.0, offset: 0.5, along: 0.0, driven: false };
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::SetDimension { dimension, moves: vec![], radii: vec![] } });
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let id = g.dimensions.keys().next().unwrap();
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::SetDimensionValue { id, value: 10.0 } });
    // The board's hole follows in the same step.
    let hole = std::f64::consts::PI * 25.0;
    assert!((board_area(&doc) - (40.0 * 30.0 - hole)).abs() < 0.1, "{}", board_area(&doc));
    let b = cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features());
    let board = b.parts.iter().find(|p| p.feature == FROM_SKETCH).unwrap();
    let v = board.mass.unwrap().volume;
    assert!((v - (40.0 * 30.0 - hole) * 1.6).abs() < 0.5, "{v}");
}

#[test]
fn a_sketch_moved_below_the_board_breaks_the_link_and_says_why() {
    let (mut doc, mut h) = sketched();
    create_from_sketch(&mut doc, &mut h);
    // The sketch dragged below the Derived feature (it could now Use the board's edges).
    let last = doc.element(STUDIO).unwrap().features().len() - 1;
    run(&mut doc, &mut h, &cadrs_core::commands::MoveFeature { element: STUDIO, feature: OUTLINE_SKETCH, to: last, label: "Move".into() });
    let sb = &doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0];
    let p = sb.outline_link.as_ref().unwrap().problem.clone().unwrap_or_default();
    assert!(p.contains("comes after"), "{p}");
    // The Derived feature fails (its plane is the sketch's), saying the sketch must come first.
    let b = cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features());
    let e = b.error(FROM_SKETCH).unwrap_or_default().to_string();
    assert!(e.contains("must come before"), "{e}");
    // Moved back: the link holds again.
    run(&mut doc, &mut h, &cadrs_core::commands::MoveFeature { element: STUDIO, feature: OUTLINE_SKETCH, to: 0, label: "Move".into() });
    assert!(doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0].outline_link.as_ref().unwrap().problem.is_none());
}

#[test]
fn a_step_export_writes_the_components_as_solids() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let b = build(&doc);
    let parts: Vec<(cadrs_core::PartId, String)> = derived_parts(&b).into_iter().map(|p| (p.id, p.name.clone())).collect();
    let req = cadrs_core::export::StepRequest { parts: parts.clone(), y_up: false, individual: false };
    let files = cadrs_core::rebuild::export_step(doc.element(STUDIO).unwrap().active_features(), req).wait().expect("exports");
    let text = String::from_utf8_lossy(&files[0].bytes).to_string();
    // A product per part, by its name, and solids (not faceted shells) for the components.
    for (_, name) in &parts {
        assert!(text.contains(name.as_str()), "{name} in the STEP file");
    }
    assert!(text.matches("MANIFOLD_SOLID_BREP").count() > parts.len() / 2, "solids");
    // The components where the Part Studio has them: read back, they span the heights the mesh
    // parts do (the parts above the board and BT1 under it).
    let mut res_doc = Document::empty("Read back");
    let mut ps = Element::part_studio("Read");
    let rid = ElementId::new();
    ps.id = rid;
    res_doc.elements.push(ps);
    let mut h2 = History::default();
    let f = FeatureId::new();
    run(&mut res_doc, &mut h2, &cadrs_core::import::AddImport { element: rid, feature: f, file_name: "board.step".into(), bytes: std::sync::Arc::new(files[0].bytes.clone()), y_axis_up: false, units: None });
    let back = cadrs_core::rebuild::build(res_doc.element(rid).unwrap().features());
    assert!(back.errors.is_empty(), "{:?}", back.errors);
    let span = |ps: &mut dyn Iterator<Item = &Part>| ps.fold((f64::MAX, f64::MIN), |(lo, hi), p| (lo.min(bounds(p).0[2]), hi.max(bounds(p).1[2])));
    let (lo, hi) = span(&mut back.parts.iter());
    let (mlo, mhi) = span(&mut derived_parts(&b).into_iter());
    assert!((lo - mlo).abs() < 0.05 && (hi - mhi).abs() < 0.05, "STEP {lo} … {hi}, meshes {mlo} … {mhi}");
    assert!(hi > 1.6 + 1.0 && lo < -1.0, "{lo} … {hi}");
}

#[test]
fn an_idf_board_derives_as_its_outline_and_package_boxes() {
    let dir = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/idf/vision controller");
    let read = |f: &str| std::fs::read_to_string(dir.join(f)).unwrap();
    let pcb = cadrs_core::pcb::PcbBoard::read(&read("Vision PCB.emn"), &read("Vision PCB.emp")).unwrap();
    let placed = pcb.board.placements.iter().filter(|p| pcb.library.package(&p.package, &p.part_number).is_some()).count();
    let thickness = pcb.thickness();
    let mut doc = Document::empty("IDF");
    let mut ps = Element::part_studio("Part Studio 1");
    ps.id = STUDIO;
    let mut el = Element::pcb_studio("PCB Studio 1");
    el.id = PCB;
    doc.elements.push(ps);
    doc.elements.push(el);
    let mut h = History::default();
    let name = pcb.name().to_string();
    run(&mut doc, &mut h, &cadrs_core::pcb::ImportBoard { element: PCB, board: Box::new(pcb), source: cadrs_core::pcb::BoardSource::Idf { emn: format!("{name}.emn"), emp: None } });
    let board = doc.element(PCB).unwrap().pcb().unwrap().boards[0].id;
    derive(&mut doc, &mut h, board, DerivedFeature::default()).expect("an IDF board derives");
    let b = build(&doc);
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let parts = derived_parts(&b);
    // The board (its thickness from the file) and a box per placed package.
    let board_part = parts.iter().find(|p| b.derived[&DERIVED].boards[0].part == p.id).unwrap();
    let (lo, hi) = bounds(board_part);
    assert!((hi[2] - lo[2] - thickness).abs() < 1e-6, "{lo:?} {hi:?}");
    assert_eq!(parts.len(), placed + 1);
    // Derived again (no change in the studio): the same parts, by id.
    let ids: Vec<cadrs_core::PartId> = parts.iter().map(|p| p.id).collect();
    let again = cadrs_core::pcb::design::idf_layout(&doc.element(PCB).unwrap().pcb().unwrap().boards[0].board);
    assert_eq!(cadrs_core::pcb::board3d::layout_hash(&again), cadrs_core::derived::derived_of(&doc, STUDIO, DERIVED).unwrap().board.as_ref().unwrap().hash);
    let b2 = build(&doc);
    assert_eq!(derived_parts(&b2).iter().map(|p| p.id).collect::<Vec<_>>(), ids);
}

#[test]
fn a_drawing_view_shows_the_board_and_its_components_or_its_layout() {
    use cadrs_core::views::{self, ViewRequest};
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let request = || ViewRequest {
        part: None,
        frame: cadrs_kernel::ViewFrame::new(nalgebra::Vector3::new(0.0, 1.0, 0.0), nalgebra::Vector3::x()),
        options: cadrs_kernel::ProjectOptions::default(),
        shaded: false,
        props: Vec::new(),
        appearances: Vec::new(),
        cut: None,
        intersections: false,
        flat: false,
    };
    let features = doc.element(STUDIO).unwrap().active_features();
    let g = views::project(&features, request()).expect("the view projects");
    let parts = derived_parts(&build(&doc)).len();
    assert_eq!(g.parts.len(), parts, "the board and every component with a model");
    assert!(!g.projection.edges.is_empty());
    // Shown as its layout: the board alone.
    let mut d = cadrs_core::derived::derived_of(&doc, STUDIO, DERIVED).unwrap().clone();
    d.board.as_mut().unwrap().options.view = cadrs_core::pcb::board3d::BoardView::Layout;
    run(&mut doc, &mut h, &SetDerived { element: STUDIO, feature: DERIVED, derived: d, links: vec![], label: "Layout".into() });
    let g = views::project(&doc.element(STUDIO).unwrap().active_features(), request()).unwrap();
    assert_eq!(g.parts.len(), 1);
    // Edge on, no picture; square on, the face seen carries the layout's picture (under the
    // view's lines), the far face's none.
    assert!(g.pictures.is_empty(), "{:?}", g.pictures.len());
    for (dir, x) in [(-1.0, 1.0), (1.0, -1.0)] {
        let r = ViewRequest { frame: cadrs_kernel::ViewFrame::new(nalgebra::Vector3::new(0.0, 0.0, dir), nalgebra::Vector3::new(x, 0.0, 0.0)), ..request() };
        let g = views::project(&doc.element(STUDIO).unwrap().active_features(), r).unwrap();
        assert_eq!(g.pictures.len(), 1, "looking along z {dir}");
        let p = &g.pictures[0];
        // It covers the board (40 × 30), its corners within the view's outline.
        let area = (p.u[0] * p.v[1] - p.u[1] * p.v[0]).abs();
        let (lo, hi) = g.bounds.unwrap();
        assert!(area > 0.9 * (hi[0] - lo[0]) * (hi[1] - lo[1]), "{area} {lo:?} {hi:?}");
    }
}

#[test]
fn circles_can_be_mounting_holes_that_follow_the_sketch() {
    use cadrs_core::commands::EditSketch;
    use cadrs_sketch::{SketchOp, Vec2};
    let (mut doc, mut h) = sketched();
    let regions = outline_regions(&doc);
    run(&mut doc, &mut h, &cadrs_core::pcb::outline_link::CreateBoardFromSketch { element: STUDIO, sketch: OUTLINE_SKETCH, regions, pcb: None, new_pcb: NEW_PCB, name: "Holes".into(), thickness: 1.6, seat: cadrs_core::pcb::board3d::BoardSeat::Bottom, circles_as_holes: true, feature: FROM_SKETCH });
    let hole = |doc: &Document| {
        let d = doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0].design().unwrap().clone();
        let f = d.board.footprints.iter().find(|f| f.reference() == "H1").expect("a mounting hole H1").clone();
        (f.id, f.placement.at, f.footprint.pads[0].clone(), d.board.shapes.iter().filter(|s| matches!(s.shape.geom, cadrs_eda::graphics::Geom::Circle { .. })).count())
    };
    // The ⌀6 circle at (5, 5): a ⌀6 non-plated hole there, no cut-out circle in the outline.
    let (id, at, pad, cut) = hole(&doc);
    assert_eq!(at, cadrs_eda::units::Pt::mm(5.0, 5.0));
    assert_eq!((pad.kind, pad.drill.map(|d| d.size.w)), (cadrs_eda::footprint::PadKind::NonPlated, Some(mm(6.0))));
    assert_eq!(cut, 0);
    // The board still has the hole through it (the pad's drill) and its full outline area.
    let b = cadrs_core::rebuild::build(doc.element(STUDIO).unwrap().features());
    let v = b.parts.iter().find(|p| p.feature == FROM_SKETCH).unwrap().mass.unwrap().volume;
    assert!((v - (40.0 * 30.0 - std::f64::consts::PI * 9.0) * 1.6).abs() < 0.5, "{v}");
    // The circle moved in the sketch: the hole follows, keeping its footprint (and part).
    let g = doc.element(STUDIO).unwrap().feature(OUTLINE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let c = g.point_at(Vec2::new(5.0, 5.0), 1e-6).unwrap();
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::MovePoints { moves: vec![(c, Vec2::new(-5.0, 10.0))] } });
    let (id2, at2, ..) = hole(&doc);
    assert_eq!((id2, at2), (id, cadrs_eda::units::Pt::mm(-5.0, 10.0)));
}

#[test]
fn a_board_outline_offset_from_the_tubes_inner_edges_follows_the_tube() {
    use cadrs_core::commands::{AddSketch, EditSketch};
    use cadrs_core::samples::pcb_enclosure as pe;
    use cadrs_sketch::{Link, SketchOp, Vec2};
    let mut doc = pe::document().unwrap();
    let mut h = History::default();
    let sketch = OUTLINE_SKETCH;
    run(&mut doc, &mut h, &AddSketch { element: pe::STUDIO, feature: sketch, plane: Some(cadrs_sketch::PlaneRef::Top) });
    // Use: the tube's four inner edges on Top (its inside is 40 × 40).
    let features = doc.element(pe::STUDIO).unwrap().features().to_vec();
    let build = cadrs_core::rebuild::build(&features);
    let tube = build.parts.iter().find(|p| p.feature == pe::TUBE).unwrap().solid.clone();
    let frame = doc.element(pe::STUDIO).unwrap().feature(sketch).unwrap().sketch().unwrap().plane.unwrap().frame();
    let ctx = cadrs_core::links::LinkContext { solids: vec![(pe::TUBE, &*tube)], features: &features };
    let inner = |e: &&cadrs_core::solid::SolidEdge| e.points.iter().all(|p| p[2].abs() < 1e-6 && (p[0].abs().max(p[1].abs()) - 20.0).abs() < 1e-6);
    let items: Vec<_> = tube.edges.iter().filter(inner).map(|e| {
        let link = Link::Edge { feature: pe::TUBE.0, edge: e.name };
        (ctx.shape(link, &frame).unwrap(), link)
    }).collect();
    assert_eq!(items.len(), 4);
    run(&mut doc, &mut h, &EditSketch { element: pe::STUDIO, feature: sketch, op: SketchOp::Use { items } });
    // Offset 1 mm inward: whichever side makes a 38 × 38 square.
    let g = |doc: &Document| doc.element(pe::STUDIO).unwrap().feature(sketch).unwrap().sketch().unwrap().geometry.clone();
    let first = g(&doc).curves.keys().next().unwrap();
    let (chain, _) = cadrs_sketch::edit::chain_of(&g(&doc), first);
    let square = |doc: &Document| cadrs_core::samples::region_refs(sketch, &g(doc), &[Vec2::new(0.0, 0.0)]);
    for left in [true, false] {
        run(&mut doc, &mut h, &EditSketch { element: pe::STUDIO, feature: sketch, op: SketchOp::Offset { chain: chain.clone(), distance: 1.0, left, label: (0.0, 0.0) } });
        let r = cadrs_sketch::region::regions(&g(&doc));
        if r.iter().any(|r| (r.area() - 38.0 * 38.0).abs() < 1e-6) {
            break;
        }
        h.undo(&mut doc);
    }
    let regions = square(&doc);
    run(&mut doc, &mut h, &cadrs_core::pcb::outline_link::CreateBoardFromSketch { element: pe::STUDIO, sketch, regions, pcb: Some(pe::PCB), new_pcb: NEW_PCB, name: "Fitted".into(), thickness: 1.6, seat: BoardSeat::Bottom, circles_as_holes: false, feature: FROM_SKETCH });
    let size = |doc: &Document| {
        let b = cadrs_core::rebuild::build(doc.element(pe::STUDIO).unwrap().features());
        assert!(b.errors.is_empty(), "{:?}", b.errors);
        let (lo, hi) = bounds(b.parts.iter().find(|p| p.feature == FROM_SKETCH).unwrap());
        [hi[0] - lo[0], hi[1] - lo[1]]
    };
    let close = |a: [f64; 2], b: [f64; 2]| (a[0] - b[0]).abs() < 1e-3 && (a[1] - b[1]).abs() < 1e-3;
    assert!(close(size(&doc), [38.0, 38.0]), "{:?}", size(&doc));
    // The tube's inside narrowed to 36 wide: the used edges, their offset and the board follow.
    let tg = doc.element(pe::STUDIO).unwrap().feature(pe::TUBE_SKETCH).unwrap().sketch().unwrap().geometry.clone();
    let moves: Vec<_> = [(20.0, 20.0), (20.0, -20.0)].iter().map(|&(x, y)| (tg.point_at(Vec2::new(x, y), 1e-6).unwrap(), Vec2::new(18.0, y))).collect();
    run(&mut doc, &mut h, &EditSketch { element: pe::STUDIO, feature: pe::TUBE_SKETCH, op: SketchOp::MovePoints { moves } });
    assert!(close(size(&doc), [36.0, 38.0]), "{:?}", size(&doc));
}

#[test]
fn a_mounting_hole_keeps_its_id_through_undo_redo_and_a_new_circle() {
    use cadrs_core::commands::EditSketch;
    use cadrs_sketch::{SketchOp, Vec2};
    let (mut doc, mut h) = sketched();
    let regions = outline_regions(&doc);
    run(&mut doc, &mut h, &cadrs_core::pcb::outline_link::CreateBoardFromSketch { element: STUDIO, sketch: OUTLINE_SKETCH, regions, pcb: None, new_pcb: NEW_PCB, name: "Holes".into(), thickness: 1.6, seat: BoardSeat::Bottom, circles_as_holes: true, feature: FROM_SKETCH });
    let holes = |doc: &Document| {
        let d = doc.element(NEW_PCB).unwrap().pcb().unwrap().boards[0].design().unwrap().clone();
        let mut v: Vec<(i64, uuid::Uuid)> = d.board.footprints.iter().filter(|f| f.reference().starts_with('H')).map(|f| (f.placement.at.x, f.id)).collect();
        v.sort();
        v
    };
    let first = holes(&doc);
    assert_eq!(first.len(), 1);
    // A second circle (left of the first): the first hole keeps its id.
    run(&mut doc, &mut h, &EditSketch { element: STUDIO, feature: OUTLINE_SKETCH, op: SketchOp::AddCircle { center: Vec2::new(-10.0, 5.0), radius: 1.5, construction: false } });
    let two = holes(&doc);
    assert_eq!(two.len(), 2);
    assert_eq!(two[1], first[0], "{two:?}");
    // Undone and redone: the new hole comes back with the same id (its part with it).
    h.undo(&mut doc);
    assert_eq!(holes(&doc), first);
    h.redo(&mut doc);
    assert_eq!(holes(&doc), two);
}

#[test]
fn a_deleted_board_stays_as_it_was_with_a_warning() {
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let parts = derived_parts(&build(&doc)).len();
    assert!(build(&doc).warning(DERIVED).is_none_or(|w| !w.contains("is gone")));
    run(&mut doc, &mut h, &cadrs_core::pcb::DeleteBoard { element: PCB, board });
    // The parts stay (the last layout), the feature says why.
    let b = build(&doc);
    assert_eq!(derived_parts(&b).len(), parts);
    assert!(b.warning(DERIVED).is_some_and(|w| w.contains("is gone from")), "{:?}", b.warnings);
    // Undone: following its board again.
    h.undo(&mut doc);
    assert!(build(&doc).warning(DERIVED).is_none_or(|w| !w.contains("is gone")));
}

#[test]
fn a_component_moved_apart_exports_where_it_was_moved() {
    use cadrs_core::transform::{TransformFeature, TransformType};
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let b0 = build(&doc);
    let board_part = b0.derived[&DERIVED].boards[0].part;
    let comp = derived_parts(&b0).into_iter().find(|p| p.id != board_part).unwrap().clone();
    // The component alone 30 along x, then the board (with its components) turned a quarter
    // about z and lifted 20.
    let t1 = TransformFeature { transform_type: TransformType::TranslateXyz, parts: vec![comp.id], dx: 30.0, dx_expr: "30 mm".into(), ..Default::default() };
    run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: FeatureId::new(), base_name: "Transform".into(), kind: FeatureKind::Transform(t1) });
    let mut turn = TransformFeature::new(TransformType::Rotate);
    turn.parts = vec![board_part];
    turn.axis = Some(cadrs_core::document::AxisRef::Connector(ConnectorRef::Feature(LOC)));
    turn.angle = 90.0;
    turn.angle_expr = "90 deg".into();
    let lift = TransformFeature { transform_type: TransformType::TranslateXyz, parts: vec![board_part], dz: 20.0, dz_expr: "20 mm".into(), ..Default::default() };
    for t in [turn, lift] {
        run(&mut doc, &mut h, &AddFeature { element: STUDIO, feature: FeatureId::new(), base_name: "Transform".into(), kind: FeatureKind::Transform(t) });
    }
    let b = build(&doc);
    assert!(b.errors.is_empty(), "{:?}", b.errors);
    let mesh = b.part(comp.id).unwrap().clone();
    // (It did turn: its x and y spans swapped places, and it went up.)
    assert!((bounds(&mesh).0[2] - bounds(&comp).0[2] - 20.0).abs() < 1e-6);
    // Its STEP solid is where its mesh is.
    let req = cadrs_core::export::StepRequest { parts: vec![(comp.id, comp.name.clone())], y_up: false, individual: false };
    let files = cadrs_core::rebuild::export_step(doc.element(STUDIO).unwrap().active_features(), req).wait().expect("exports");
    let mut back_doc = Document::empty("Read back");
    let mut ps = Element::part_studio("Read");
    let rid = ElementId::new();
    ps.id = rid;
    back_doc.elements.push(ps);
    let mut h2 = History::default();
    run(&mut back_doc, &mut h2, &cadrs_core::import::AddImport { element: rid, feature: FeatureId::new(), file_name: "c.step".into(), bytes: std::sync::Arc::new(files[0].bytes.clone()), y_axis_up: false, units: None });
    let back = cadrs_core::rebuild::build(back_doc.element(rid).unwrap().features());
    let (lo, hi) = back.parts.iter().map(bounds).fold(([f64::MAX; 3], [f64::MIN; 3]), |(a, b), (l, u)| (std::array::from_fn(|k| a[k].min(l[k])), std::array::from_fn(|k| b[k].max(u[k]))));
    let (mlo, mhi) = bounds(&mesh);
    for k in 0..3 {
        assert!((lo[k] - mlo[k]).abs() < 0.3 && (hi[k] - mhi[k]).abs() < 0.3, "axis {k}: STEP {lo:?} … {hi:?}, mesh {mlo:?} … {mhi:?}");
    }
}

#[test]
fn an_outline_cant_follow_a_sketch_after_a_derive_that_brings_the_board_in() {
    use cadrs_core::commands::{AddSketch, EditSketch};
    use cadrs_sketch::{SketchOp, Vec2};
    // Part Studio 1 derives the board; Part Studio 2 derives Part Studio 1, then has a sketch.
    let (mut doc, mut h, board) = document();
    derive(&mut doc, &mut h, board, DerivedFeature::default()).unwrap();
    let ps2 = ElementId::new();
    let mut e = Element::part_studio("Part Studio 2");
    e.id = ps2;
    doc.elements.push(e);
    let mut res = Resolver::new(Store::new(std::env::temp_dir().join("cadrs-derived-board-unused")));
    let r = SourceRef { document: None, at: RefAt::Workspace, element: STUDIO, pinned: false };
    let got = derived::resolve(&mut res, &doc, None, DerivedFeature::default(), r).unwrap();
    run(&mut doc, &mut h, &AddDerived { element: ps2, feature: FeatureId::new(), derived: got.derived, links: got.links });
    let sketch = FeatureId::new();
    run(&mut doc, &mut h, &AddSketch { element: ps2, feature: sketch, plane: Some(cadrs_sketch::PlaneRef::Top) });
    let rect = SketchOp::AddPolyline { points: vec![Vec2::new(0.0, 0.0), Vec2::new(30.0, 0.0), Vec2::new(30.0, 20.0), Vec2::new(0.0, 20.0)], closed: true, construction: false, label: "Add rectangle" };
    run(&mut doc, &mut h, &EditSketch { element: ps2, feature: sketch, op: rect });
    let g = doc.element(ps2).unwrap().feature(sketch).unwrap().sketch().unwrap().geometry.clone();
    let regions = cadrs_core::samples::region_refs(sketch, &g, &[Vec2::new(15.0, 10.0)]);
    let link = cadrs_core::pcb::outline_link::SetOutlineLink { pcb: PCB, board, link: Some((ps2, sketch, regions)), keep_parts: true };
    let e = h.execute(&mut doc, &link).unwrap_err().to_string();
    assert!(e.contains("which derives Part Studio 1"), "{e}");
}

#[test]
fn copies_on_locations_too_close_run_into_each_other() {
    let (mut doc, mut h, board) = document();
    let at = |doc: &mut Document, h: &mut History, z: f64, x: f64| {
        let id = FeatureId::new();
        let mc = MateConnectorFeature { origin: Some(ConnectorOrigin::Origin), offset: [x, 0.0, z], offset_expr: [format!("{x} mm"), "0 mm".into(), format!("{z} mm")], ..Default::default() };
        run(doc, h, &AddFeature { element: STUDIO, feature: id, base_name: "Mate connector".into(), kind: FeatureKind::MateConnector(mc) });
        id
    };
    // A second copy 3 mm over the first (z 100): the first's components stand into its board.
    let close = at(&mut doc, &mut h, 103.0, 0.0);
    derive(&mut doc, &mut h, board, DerivedFeature { locations: vec![ConnectorRef::Feature(LOC), ConnectorRef::Feature(close)], ..Default::default() }).unwrap();
    let b = build(&doc);
    assert!(b.warning(DERIVED).is_some_and(|w| w.contains("runs into") && w.contains("(2)")), "{:?}", b.warning(DERIVED));
    // 500 mm aside: clear.
    h.undo(&mut doc);
    let far = at(&mut doc, &mut h, 100.0, 500.0);
    derive(&mut doc, &mut h, board, DerivedFeature { locations: vec![ConnectorRef::Feature(LOC), ConnectorRef::Feature(far)], ..Default::default() }).unwrap();
    assert!(build(&doc).warning(DERIVED).is_none_or(|w| !w.contains("runs into")), "{:?}", build(&doc).warning(DERIVED));
}

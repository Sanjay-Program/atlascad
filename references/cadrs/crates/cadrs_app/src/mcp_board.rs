//! The MCP server's PCB board tools (`docs/PCB_IN_PART_STUDIO.md` P7): read a board, derive it
//! into a Part Studio, make one from a sketch, and move, add or remove its footprints, the
//! fitting done by name and number ([`cadrs_core::pcb::ops`]). Each change is one undo step of
//! the open document.

use bevy::prelude::*;
use cadrs_core::derived::{AddDerived, DerivedBoard, DerivedFeature};
use cadrs_core::external::{RefAt, SourceRef};
use cadrs_core::mate::ConnectorRef;
use cadrs_core::pcb::board3d::{BoardAnchor, BoardSeat, BoardView};
use cadrs_core::pcb::ops;
use cadrs_core::pcb::{BoardId, SetDesign};
use cadrs_core::{ElementId, FeatureId};
use cadrs_eda::layer::Side;
use cadrs_mcp::tools;
use serde_json::{Value, json};

use crate::ActiveDocument;

fn doc(world: &World) -> Result<&ActiveDocument, String> {
    world.get_resource::<ActiveDocument>().ok_or_else(|| "no document is open: call open_document first".into())
}

fn board(world: &World, b: &tools::BoardName) -> Result<(ElementId, BoardId), String> {
    ops::find_board(&doc(world)?.doc, b.pcb_studio.as_deref(), &b.board)
}

/// The Part Studio a call names, or the active tab.
fn studio(world: &World, name: Option<&str>) -> Result<ElementId, String> {
    let d = doc(world)?;
    let e = match name {
        Some(n) => d.doc.elements.iter().find(|e| e.name == n).ok_or_else(|| format!("no tab named {n:?}"))?,
        None => d.active_element().ok_or("no tab is active")?,
    };
    if !matches!(e.kind, cadrs_core::document::ElementKind::PartStudio { .. }) {
        return Err(format!("{:?} is not a Part Studio", e.name));
    }
    Ok(e.id)
}

fn feature_id(world: &World, studio: ElementId, name: &str) -> Result<FeatureId, String> {
    doc(world)?.doc.element(studio).and_then(|e| e.features().iter().find(|f| f.name == name)).map(|f| f.id).ok_or_else(|| format!("no feature named {name:?}"))
}

fn execute(world: &mut World, cmd: &dyn cadrs_core::Command) -> Result<(), String> {
    world.get_resource_mut::<ActiveDocument>().ok_or("no document is open")?.execute(cmd).map_err(|e| e.to_string())
}

fn report(world: &World, (el, b): (ElementId, BoardId)) -> Result<Value, String> {
    serde_json::to_value(ops::board_report(&doc(world)?.doc, el, b)?).map_err(|e| e.to_string())
}

/// Changes a board's design by `f` and stores it (one undo step named `label`); returns the
/// board's report.
fn edit(world: &mut World, b: &tools::BoardName, label: &str, f: impl FnOnce(&mut cadrs_eda::Design) -> Result<(), String>) -> Result<Value, String> {
    let (el, id) = board(world, b)?;
    let mut d = ops::design_of(&doc(world)?.doc, el, id)?;
    f(&mut d)?;
    execute(world, &SetDesign { element: el, board: id, design: Box::new(d), label: label.into() })?;
    report(world, (el, id))
}

pub fn get_board(world: &mut World, p: &tools::BoardName) -> Result<Value, String> {
    let b = board(world, p)?;
    report(world, b)
}

pub fn get_board_fit(world: &mut World, p: &tools::BoardFit) -> Result<Value, String> {
    let (pcb, id) = board(world, &p.board)?;
    let ps = studio(world, p.part_studio.as_deref())?;
    let d = &doc(world)?.doc;
    let f = ops::deriving_feature(d, ps, pcb, id).ok_or("the board isn't derived into that Part Studio: derive_board first")?;
    let el = d.element(ps).ok_or("no Part Studio")?;
    let build = cadrs_core::rebuild::build(el.features());
    let copies = build.derived.get(&f).map_or(0, |o| o.boards.len());
    let k = copy_index(p.copy, copies)?;
    let frame = build.derived.get(&f).and_then(|o| o.boards.get(k)).map(|c| c.frame).ok_or("the feature derives no board")?;
    let mut parts = vec![];
    for part in build.parts.iter().filter(|x| x.feature != f) {
        let name = cadrs_core::parts::display_name(part, el.part_props()).to_string();
        let axes = cadrs_core::pcb::fit::axes(&[&*part.solid], &frame, -1e6, 1e6);
        if !axes.is_empty() {
            parts.push(json!({ "part": name, "axes": axes.iter().map(|(a, r)| json!({ "at": [round(a[0]), round(a[1])], "radius": round(*r) })).collect::<Vec<_>>() }));
        }
    }
    let name = |x: cadrs_core::PartId| build.part(x).map(|p| cadrs_core::parts::display_name(p, el.part_props()).to_string()).unwrap_or_default();
    let collisions: Vec<String> = build.derived.get(&f).map(|o| o.collisions.iter().map(|c| cadrs_core::pcb::fit::collision_text(&name(c.component), &name(c.other), c)).collect()).unwrap_or_default();
    Ok(json!({ "feature": el.feature(f).map(|x| x.name.clone()), "copy": k + 1, "copies": copies, "pins_and_holes": parts, "collisions": collisions }))
}

/// A board copy's index from the tools' 1-based `copy` (omitted: the first), among `copies`.
fn copy_index(copy: Option<usize>, copies: usize) -> Result<usize, String> {
    match copy.unwrap_or(1) {
        k if (1..=copies.max(1)).contains(&k) => Ok(k - 1),
        k => Err(format!("copy {k}: the board is derived {copies} time(s) there")),
    }
}

/// The region of sketch `g` to make a board's outline of: the smallest holding `inside`, or
/// the largest.
fn region_of(g: &cadrs_sketch::Sketch, sketch: cadrs_core::FeatureId, inside: Option<[f64; 2]>) -> Result<Vec<cadrs_core::document::RegionRef>, String> {
    let Some([x, y]) = inside else { return Ok(crate::board_in_studio::largest_region(g, sketch)) };
    let all = cadrs_sketch::region::regions_shared(g);
    let r = all.iter().filter(|r| r.contains(cadrs_sketch::Vec2::new(x, y))).min_by(|a, b| a.area().total_cmp(&b.area()));
    r.map(|r| vec![cadrs_core::document::RegionRef::new(sketch, r)]).ok_or_else(|| format!("no closed region of the sketch holds ({x}, {y})"))
}

fn round(v: f64) -> f64 {
    (v * 1e3).round() / 1e3
}

fn seat(s: Option<&str>) -> Result<BoardSeat, String> {
    match s.map(str::to_ascii_lowercase).as_deref() {
        None | Some("bottom") => Ok(BoardSeat::Bottom),
        Some("top") => Ok(BoardSeat::Top),
        Some(o) => Err(format!("seat {o:?}: bottom or top")),
    }
}

pub fn derive_board(world: &mut World, p: &tools::DeriveBoard) -> Result<Value, String> {
    let (pcb, id) = board(world, &p.board)?;
    let ps = studio(world, p.part_studio.as_deref())?;
    let location = p.location.as_deref().map(|n| feature_id(world, ps, n)).transpose()?;
    let on_sketch = p.on_sketch.as_deref().map(|n| feature_id(world, ps, n)).transpose()?;
    let mut db = DerivedBoard::new(id, "", Default::default());
    db.options.seat = seat(p.seat.as_deref())?;
    db.options.view = match p.view.as_deref().map(str::to_ascii_lowercase).as_deref() {
        None | Some("3d") => BoardView::ThreeD,
        Some("layout") => BoardView::Layout,
        Some(o) => return Err(format!("view {o:?}: 3d or layout")),
    };
    db.on_sketch = on_sketch;
    let d = DerivedFeature { locations: location.map(ConnectorRef::Feature).into_iter().collect(), board: Some(Box::new(db)), ..Default::default() };
    let r = SourceRef { document: None, at: RefAt::Workspace, element: pcb, pinned: false };
    let doc_now = doc(world)?.doc.clone();
    let got = {
        let mut res = crate::linked::resolver(world);
        cadrs_core::derived::resolve(&mut res.0, &doc_now, None, d, r).map_err(|e| e.to_string())?
    };
    let mut derived = got.derived;
    // The anchor: fixed by default (resolve set it); origin on a sketch; or as asked.
    if let Some(b) = derived.board.as_mut() {
        b.options.anchor = match p.anchor.as_deref().map(str::to_ascii_lowercase).as_deref() {
            None if on_sketch.is_some() => BoardAnchor::Origin,
            None | Some("fixed") => b.options.anchor,
            Some("origin") => BoardAnchor::Origin,
            Some("centre") | Some("center") => BoardAnchor::Center,
            Some(r) => BoardAnchor::Footprint(b.layout.footprints.iter().find(|f| f.reference().eq_ignore_ascii_case(r)).map(|f| f.id).ok_or_else(|| format!("anchor {r:?}: fixed, origin, centre or a footprint's reference"))?),
        };
    }
    let feature = FeatureId::new();
    execute(world, &AddDerived { element: ps, feature, derived, links: got.links })?;
    let el = doc(world)?.doc.element(ps).ok_or("no Part Studio")?;
    let build = cadrs_core::rebuild::build(el.features());
    Ok(json!({
        "feature": el.feature(feature).map(|f| f.name.clone()),
        "error": build.error(feature),
        "warning": build.warning(feature),
        "parts": build.parts.iter().filter(|x| x.feature == feature).map(|x| cadrs_core::parts::display_name(x, el.part_props()).to_string()).collect::<Vec<_>>(),
    }))
}

pub fn create_board_from_sketch(world: &mut World, p: &tools::CreateBoardFromSketch) -> Result<Value, String> {
    let ps = studio(world, p.part_studio.as_deref())?;
    let sketch = feature_id(world, ps, &p.sketch)?;
    let regions = {
        let d = doc(world)?;
        let g = d.doc.element(ps).and_then(|e| e.feature(sketch)).and_then(|f| f.sketch()).map(|s| s.geometry.clone()).ok_or("not a sketch")?;
        region_of(&g, sketch, p.inside)?
    };
    if regions.is_empty() {
        return Err("the sketch has no closed region".into());
    }
    let pcb = match &p.pcb_studio {
        Some(n) => Some(doc(world)?.doc.elements.iter().find(|e| e.name == *n && e.pcb().is_some()).map(|e| e.id).ok_or_else(|| format!("no PCB Studio named {n:?}"))?),
        None => None,
    };
    let cmd = cadrs_core::pcb::outline_link::CreateBoardFromSketch {
        element: ps,
        sketch,
        regions,
        pcb,
        new_pcb: ElementId::new(),
        name: p.name.clone(),
        thickness: p.thickness.unwrap_or(1.6),
        seat: seat(p.seat.as_deref())?,
        circles_as_holes: p.circles_as_holes.unwrap_or(false),
        feature: FeatureId::new(),
    };
    let feature = cmd.feature;
    execute(world, &cmd)?;
    // The board the new Derived feature derives (its name may have been made unique).
    let b = cadrs_core::derived::derived_of(&doc(world)?.doc, ps, feature).and_then(|d| Some((d.source?.element, d.board.as_ref()?.board))).ok_or("the board is gone")?;
    report(world, b)
}

fn side(s: Option<&str>) -> Result<Option<Side>, String> {
    match s.map(str::to_ascii_lowercase).as_deref() {
        None => Ok(None),
        Some("top") => Ok(Some(Side::Top)),
        Some("bottom") => Ok(Some(Side::Bottom)),
        Some(o) => Err(format!("side {o:?}: top or bottom")),
    }
}

pub fn move_footprint(world: &mut World, p: &tools::MoveFootprint) -> Result<Value, String> {
    let at = match (p.x, p.y) {
        (Some(x), Some(y)) => Some([x, y]),
        (None, None) => None,
        _ => return Err("give both x and y".into()),
    };
    let side = side(p.side.as_deref())?;
    // A pad onto a part's pin: the axis nearest the pad (after any turn or flip asked for).
    let onto = match (&p.onto_part, &p.onto_connector) {
        (Some(_), Some(_)) => return Err("give onto_part or onto_connector, not both".into()),
        (None, Some(c)) => {
            let pad = p.pad.clone().ok_or("onto_connector needs the pad to centre")?;
            let (pcb, id) = board(world, &p.board)?;
            let ps = studio(world, p.part_studio.as_deref())?;
            let d = &doc(world)?.doc;
            let f = ops::deriving_feature(d, ps, pcb, id).ok_or("the board isn't derived into that Part Studio")?;
            let copies = cadrs_core::rebuild::build(d.element(ps).ok_or("no Part Studio")?.features()).derived.get(&f).map_or(0, |o| o.boards.len());
            let at = ops::connector_on_board(d, ps, f, c, copy_index(p.copy, copies)?)?;
            Some((pad, vec![(at, 0.0)]))
        }
        (Some(part), None) => {
            let pad = p.pad.clone().ok_or("onto_part needs the pad to centre")?;
            let (pcb, id) = board(world, &p.board)?;
            let ps = studio(world, p.part_studio.as_deref())?;
            let d = &doc(world)?.doc;
            let f = ops::deriving_feature(d, ps, pcb, id).ok_or("the board isn't derived into that Part Studio")?;
            let copies = cadrs_core::rebuild::build(d.element(ps).ok_or("no Part Studio")?.features()).derived.get(&f).map_or(0, |o| o.boards.len());
            let axes = ops::axes_of_part(d, ps, f, part, copy_index(p.copy, copies)?)?;
            Some((pad, axes))
        }
        (None, None) => None,
    };
    let label = format!("Move {}", p.reference);
    let reference = p.reference.clone();
    let (angle, which) = (p.angle, p.axis);
    edit(world, &p.board, &label, move |d| {
        ops::place_footprint(d, &reference, at, angle, side)?;
        if let Some((pad, axes)) = onto {
            let i = cadrs_eda::board_edit::footprint_index(&d.board, &reference).ok_or("gone")?;
            let f = &d.board.footprints[i];
            let pp = f.footprint.pads.iter().find(|x| x.number == pad).ok_or_else(|| format!("{reference} has no pad {pad:?}"))?;
            let c = f.placement.apply(pp.at);
            let c = [cadrs_eda::units::to_mm(c.x), cadrs_eda::units::to_mm(c.y)];
            let target = match which {
                Some(k) => axes.get(k).ok_or_else(|| format!("the part has {} axes", axes.len()))?.0,
                None => axes.iter().min_by(|a, b| (a.0[0] - c[0]).hypot(a.0[1] - c[1]).total_cmp(&(b.0[0] - c[0]).hypot(b.0[1] - c[1]))).map(|a| a.0).ok_or("no axis")?,
            };
            ops::pad_onto(d, &reference, &pad, target)?;
        }
        Ok(())
    })
}

pub fn add_footprint(world: &mut World, p: &tools::AddFootprint) -> Result<Value, String> {
    let nets: Vec<(String, String)> = p.nets.iter().map(|(a, b)| (a.clone(), b.clone())).collect();
    let (from, r, v, at) = (p.copy_of.clone(), p.reference.clone(), p.value.clone(), [p.x, p.y]);
    edit(world, &p.board, &format!("Add {}", p.reference), move |d| ops::copy_footprint(d, &from, &r, &v, at, &nets).map(|_| ()))
}

pub fn remove_footprint(world: &mut World, p: &tools::RemoveFootprint) -> Result<Value, String> {
    let r = p.reference.clone();
    edit(world, &p.board, &format!("Remove {}", p.reference), move |d| ops::remove_footprint(d, &r))
}

pub fn link_outline(world: &mut World, p: &tools::LinkOutline) -> Result<Value, String> {
    let (pcb, id) = board(world, &p.board)?;
    let Some(sketch_name) = &p.sketch else {
        execute(world, &cadrs_core::pcb::outline_link::SetOutlineLink { pcb, board: id, link: None, keep_parts: false })?;
        return report(world, (pcb, id));
    };
    let ps = studio(world, p.part_studio.as_deref())?;
    let sketch = feature_id(world, ps, sketch_name)?;
    let d = &doc(world)?.doc;
    let g = d.element(ps).and_then(|e| e.feature(sketch)).and_then(|f| f.sketch()).map(|s| s.geometry.clone()).ok_or("not a sketch")?;
    let regions = region_of(&g, sketch, p.inside)?;
    if regions.is_empty() {
        return Err("the sketch has no closed region".into());
    }
    let link = cadrs_core::pcb::outline_link::SetOutlineLink { pcb, board: id, link: Some((ps, sketch, regions)), keep_parts: p.keep_parts.unwrap_or(true) };
    execute(world, &link)?;
    report(world, (pcb, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadrs_core::samples::pcb_enclosure as pe;

    /// A world with the mains-sensor sample open on its Enclosure tab.
    fn world() -> World {
        let mut w = World::new();
        w.insert_resource(crate::DocumentStore(cadrs_core::Store::new(std::env::temp_dir().join("cadrs-mcp-board-tests"))));
        let mut d = ActiveDocument::new(pe::document().unwrap());
        d.set_active(pe::STUDIO);
        w.insert_resource(d);
        w
    }

    fn args<T: serde::de::DeserializeOwned>(v: Value) -> T {
        serde_json::from_value(v).unwrap()
    }

    #[test]
    fn a_board_is_derived_fitted_by_its_copy_and_a_pad_put_on_a_pin() {
        let mut w = world();
        let got = derive_board(&mut w, &args(json!({ "board": "AC sensor", "location": "Board location 1" }))).unwrap();
        assert!(got["error"].is_null(), "{got}");
        assert!(got["parts"].as_array().unwrap().len() > 5, "{got}");
        // The plug's pins square to the board, on its first (only) copy; a second is refused.
        let fit = get_board_fit(&mut w, &args(json!({ "board": "AC sensor" }))).unwrap();
        assert_eq!(fit["copies"], 1);
        let parts: Vec<&str> = fit["pins_and_holes"].as_array().unwrap().iter().filter_map(|p| p["part"].as_str()).collect();
        assert!(["Live pin", "Earth pin", "Neutral pin"].iter().all(|n| parts.contains(n)), "{parts:?}");
        assert!(get_board_fit(&mut w, &args(json!({ "board": "AC sensor", "copy": 2 }))).unwrap_err().contains("derived 1 time"));
        // J4's pad 1 onto the Live pin: one undo step, the pad on the pin's axis.
        let live = fit["pins_and_holes"].as_array().unwrap().iter().find(|p| p["part"] == "Live pin").unwrap()["axes"][0]["at"].clone();
        let r = move_footprint(&mut w, &args(json!({ "board": "AC sensor", "reference": "J4", "pad": "1", "onto_part": "Live pin" }))).unwrap();
        let pad = r["footprints"].as_array().unwrap().iter().find(|f| f["reference"] == "J4").unwrap()["pads"].as_array().unwrap().iter().find(|p| p["number"] == "1").unwrap()["at"].clone();
        let (a, b) = (live.as_array().unwrap(), pad.as_array().unwrap());
        assert!((0..2).all(|k| (a[k].as_f64().unwrap() - b[k].as_f64().unwrap()).abs() < 1e-3), "{live} vs {pad}");
        assert_eq!(w.resource::<ActiveDocument>().history.undo_label(), Some("Move J4"));
    }

    #[test]
    fn a_board_from_a_sketch_takes_the_region_asked_for() {
        // Sketch 2: the end plate, a 44 × 44 square round a 12 × 24 opening.
        let mut w = world();
        let size = |r: &Value| {
            let o = r["outline"].as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>();
            (o[2] - o[0], o[3] - o[1])
        };
        let r = create_board_from_sketch(&mut w, &args(json!({ "sketch": "Sketch 2", "name": "Opening", "inside": [0.0, 0.0] }))).unwrap();
        let board = board(&w, &args(json!({ "board": "Opening" }))).unwrap();
        let r2 = report(&w, board).unwrap();
        assert_eq!(size(&r2), (12.0, 24.0), "{r}");
        assert!(create_board_from_sketch(&mut w, &args(json!({ "sketch": "Sketch 2", "name": "Nowhere", "inside": [100.0, 0.0] }))).unwrap_err().contains("no closed region"));
        // Omitted: the largest (the plate round the opening).
        create_board_from_sketch(&mut w, &args(json!({ "sketch": "Sketch 2", "name": "Plate" }))).unwrap();
        let r3 = report(&w, board_of(&w, "Plate")).unwrap();
        assert_eq!(size(&r3), (44.0, 44.0));
    }

    #[test]
    fn a_pad_goes_onto_a_mate_connectors_axis() {
        // Board location (its Z up, square to the board) is where the outline's centre is:
        // layout (15.5, 19.5).
        let mut w = world();
        derive_board(&mut w, &args(json!({ "board": "AC sensor", "location": "Board location 1" }))).unwrap();
        let r = move_footprint(&mut w, &args(json!({ "board": "AC sensor", "reference": "J4", "pad": "1", "onto_connector": "Board location 1" }))).unwrap();
        let pad = r["footprints"].as_array().unwrap().iter().find(|f| f["reference"] == "J4").unwrap()["pads"].as_array().unwrap().iter().find(|p| p["number"] == "1").unwrap()["at"].clone();
        let at: Vec<f64> = pad.as_array().unwrap().iter().map(|v| v.as_f64().unwrap()).collect();
        assert!((at[0] - 15.5).abs() < 1e-3 && (at[1] - 19.5).abs() < 1e-3, "{pad}");
        assert!(move_footprint(&mut w, &args(json!({ "board": "AC sensor", "reference": "J4", "pad": "1", "onto_connector": "Nope" }))).unwrap_err().contains("no mate connector"));
    }

    fn board_of(w: &World, name: &str) -> (ElementId, BoardId) {
        board(w, &args(json!({ "board": name }))).unwrap()
    }
}

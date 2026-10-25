//! Board edits by name and number, for tools that work without a view (the MCP server's
//! `get_board` / `move_footprint` / `add_footprint`, scripts): find a board, read it as a
//! report, move a footprint (to a place, or a pad onto a pin of a part in a Part Studio the
//! board is derived into), add one (a copy of another, its pads on given nets), remove one
//! (with its schematic symbol), shift a whole layout. Each returns the new design: the caller
//! stores it with [`super::SetDesign`] (one undo step).

use cadrs_eda::Design;
use cadrs_eda::board::Board;
use cadrs_eda::layer::{Layer, Side};
use cadrs_eda::units::{Pt, to_mm};
use serde::Serialize;
use uuid::Uuid;

use super::BoardId;
use crate::document::{Document, FeatureKind};
use crate::ids::{ElementId, FeatureId};

/// The board named `board` in the PCB Studio named `studio` (any PCB Studio when `None`).
pub fn find_board(doc: &Document, studio: Option<&str>, board: &str) -> Result<(ElementId, BoardId), String> {
    for el in doc.elements.iter().filter(|e| studio.is_none_or(|s| e.name == s)) {
        let Some(s) = el.pcb() else { continue };
        if let Some(b) = s.boards.iter().find(|b| b.name() == board) {
            return Ok((el.id, b.id));
        }
    }
    let known: Vec<String> = doc.elements.iter().filter_map(|e| e.pcb().map(|s| (e, s))).flat_map(|(e, s)| s.boards.iter().map(move |b| format!("{} ({})", b.name(), e.name))).collect();
    Err(format!("no board {board:?}{}; the boards: {}", studio.map(|s| format!(" in {s:?}")).unwrap_or_default(), known.join(", ")))
}

/// A board's design, to change.
pub fn design_of(doc: &Document, el: ElementId, board: BoardId) -> Result<Design, String> {
    doc.element(el).and_then(|e| e.pcb()).and_then(|s| s.board(board)).and_then(|b| b.design()).cloned().ok_or_else(|| "the board isn't designed in cadrs (an IDF board has no layout)".into())
}

/// A pad as a report shows it (mm, on the board).
#[derive(Clone, Debug, Serialize)]
pub struct PadReport {
    pub number: String,
    pub at: [f64; 2],
    pub net: String,
    /// The hole's diameter (mm) of a through-hole pad.
    pub drill: Option<f64>,
}

/// A footprint as a report shows it.
#[derive(Clone, Debug, Serialize)]
pub struct FootprintReport {
    pub reference: String,
    pub value: String,
    pub footprint: String,
    pub at: [f64; 2],
    pub angle: f64,
    pub side: &'static str,
    pub on_board: bool,
    pub pads: Vec<PadReport>,
}

/// Where a board is derived.
#[derive(Clone, Debug, Serialize)]
pub struct DerivedReport {
    pub part_studio: String,
    pub feature: String,
    pub warning: Option<String>,
}

/// What `get_board` says about a board (mm).
#[derive(Clone, Debug, Serialize)]
pub struct BoardReport {
    pub name: String,
    pub pcb_studio: String,
    pub thickness: f64,
    /// The outline's box: x0, y0, x1, y1.
    pub outline: Option<[f64; 4]>,
    pub outline_follows: Option<String>,
    pub nets: Vec<String>,
    pub footprints: Vec<FootprintReport>,
    pub tracks: usize,
    pub vias: usize,
    pub zones: usize,
    pub derived_in: Vec<DerivedReport>,
    /// The design rules checker's errors (overlapping courtyards, clearances, keep-outs), as
    /// "Courtyards overlap: J2, J4".
    pub drc: Vec<String>,
}

/// The report of a board (see [`BoardReport`]).
pub fn board_report(doc: &Document, el: ElementId, board: BoardId) -> Result<BoardReport, String> {
    let e = doc.element(el).ok_or("no such tab")?;
    let sb = e.pcb().and_then(|s| s.board(board)).ok_or("no such board")?;
    let d = sb.design().ok_or("the board isn't designed in cadrs")?;
    let b = &d.board;
    let region = cadrs_eda::outline::board_region(b);
    let bb = region.iter().flatten().fold(None, |acc, p| Some(cadrs_eda::units::Bounds::union(acc, cadrs_eda::units::Bounds::of(*p))));
    let mm2 = |p: Pt| [round(to_mm(p.x)), round(to_mm(p.y))];
    let footprints = b
        .footprints
        .iter()
        .map(|f| FootprintReport {
            reference: f.reference().to_string(),
            value: f.footprint.field(cadrs_eda::symbol::fields::VALUE).map(|v| v.text.text.text.clone()).unwrap_or_default(),
            footprint: f.footprint.id.clone(),
            at: mm2(f.placement.at),
            angle: f.placement.angle,
            side: if f.placement.side == Side::Top { "top" } else { "bottom" },
            on_board: super::board3d::on_board(&region, f),
            pads: f.footprint.pads.iter().map(|p| PadReport { number: p.number.clone(), at: mm2(f.placement.apply(p.at)), net: p.net.clone().unwrap_or_default(), drill: p.drill.as_ref().map(|d| round(to_mm(d.size.w))) }).collect(),
        })
        .collect();
    let follows = sb.outline_link.as_ref().map(|l| {
        let studio = doc.element(l.element);
        let sketch = studio.and_then(|s| s.feature(l.sketch)).map(|f| f.name.clone()).unwrap_or_default();
        let mut s = format!("{sketch} in {}", studio.map(|s| s.name.as_str()).unwrap_or_default());
        if let Some(p) = &l.problem {
            s.push_str(&format!(" (not now: {p})"));
        }
        s
    });
    let mut derived_in = vec![];
    for ps in &doc.elements {
        let deriving: Vec<&crate::document::Feature> = ps
            .features()
            .iter()
            .filter(|f| matches!(&f.kind, FeatureKind::Derived(x) if x.source.is_some_and(|r| r.element == el && r.document.is_none_or(|d| d == doc.id)) && x.board.as_ref().is_some_and(|db| db.board == board)))
            .collect();
        if deriving.is_empty() {
            continue;
        }
        // (One rebuild per Part Studio.)
        let build = crate::rebuild::build(ps.features());
        derived_in.extend(deriving.iter().map(|f| DerivedReport { part_studio: ps.name.clone(), feature: f.name.clone(), warning: build.warning(f.id).map(String::from) }));
    }
    Ok(BoardReport {
        name: sb.name().to_string(),
        pcb_studio: e.name.clone(),
        thickness: to_mm(b.thickness),
        outline: bb.map(|bb| [round(to_mm(bb.min.x)), round(to_mm(bb.min.y)), round(to_mm(bb.max.x)), round(to_mm(bb.max.y))]),
        outline_follows: follows,
        nets: b.nets.clone(),
        footprints,
        tracks: b.tracks.len(),
        vias: b.vias.len(),
        zones: b.zones.len(),
        derived_in,
        drc: cadrs_eda::drc::check(b).errors().take(60).map(|v| format!("{}: {}", v.message, v.items.join(", "))).collect(),
    })
}

fn round(v: f64) -> f64 {
    (v * 1e4).round() / 1e4
}

fn index(d: &Design, reference: &str) -> Result<usize, String> {
    cadrs_eda::board_edit::footprint_index(&d.board, reference).ok_or_else(|| format!("no footprint {reference:?} on the board"))
}

/// Puts the footprint `reference` at `at` (its origin, mm), turned to `angle`, on `side`
/// (each kept when `None`); its tracks' ends follow a move, as a drag's do.
pub fn place_footprint(d: &mut Design, reference: &str, at: Option<[f64; 2]>, angle: Option<f64>, side: Option<Side>) -> Result<(), String> {
    use cadrs_eda::board_edit as be;
    let i = index(d, reference)?;
    if let Some(s) = side
        && d.board.footprints[i].placement.side != s
    {
        be::flip_footprint(&mut d.board, i);
    }
    if let Some(a) = angle {
        let turn = a - d.board.footprints[i].placement.angle;
        if turn.abs() > 1e-9 {
            be::rotate_footprint(&mut d.board, i, turn);
        }
    }
    if let Some([x, y]) = at {
        let delta = Pt::mm(x, y) - d.board.footprints[i].placement.at;
        be::drag_footprint(&mut d.board, i, delta);
    }
    Ok(())
}

/// Moves the footprint `reference` so its pad `pad` is centred on `onto` (mm).
pub fn pad_onto(d: &mut Design, reference: &str, pad: &str, onto: [f64; 2]) -> Result<(), String> {
    let i = index(d, reference)?;
    let f = &d.board.footprints[i];
    let p = f.footprint.pads.iter().find(|p| p.number == pad).ok_or_else(|| format!("{reference} has no pad {pad:?}"))?;
    let now = f.placement.apply(p.at);
    let delta = Pt::mm(onto[0], onto[1]) - now;
    cadrs_eda::board_edit::drag_footprint(&mut d.board, i, delta);
    Ok(())
}

/// Removes the footprint `reference` from the board (its tracks stay, to be rerouted) and its
/// symbol from the schematic, so updating the layout from the schematic doesn't bring it back.
pub fn remove_footprint(d: &mut Design, reference: &str) -> Result<(), String> {
    let i = index(d, reference)?;
    d.board.footprints.remove(i);
    for s in &mut d.schematic.sheets {
        s.symbols.retain(|p| p.reference() != reference);
    }
    Ok(())
}

/// Adds a copy of the footprint `from` as `reference` (with `value`), its origin at `at` (mm),
/// its pads on `nets` (pad number, net; the others on none). Returns its id. It is a board-only
/// footprint (no schematic symbol), which updating the layout from the schematic keeps.
pub fn copy_footprint(d: &mut Design, from: &str, reference: &str, value: &str, at: [f64; 2], nets: &[(String, String)]) -> Result<Uuid, String> {
    if cadrs_eda::board_edit::footprint_index(&d.board, reference).is_some() {
        return Err(format!("{reference} is on the board already"));
    }
    let mut f = d.board.footprints[index(d, from)?].clone();
    f.id = Uuid::new_v4();
    f.symbol = None;
    f.placement.at = Pt::mm(at[0], at[1]);
    if let Some(r) = f.footprint.field_mut(cadrs_eda::symbol::fields::REFERENCE) {
        r.text.text.text = reference.into();
    }
    if let Some(v) = f.footprint.field_mut(cadrs_eda::symbol::fields::VALUE) {
        v.text.text.text = value.into();
    }
    for p in &mut f.footprint.pads {
        p.net = nets.iter().find(|(n, _)| *n == p.number).map(|(_, net)| net.clone());
    }
    for (_, net) in nets {
        if !net.is_empty() && !d.board.nets.contains(net) {
            d.board.nets.push(net.clone());
            d.board.nets.sort();
        }
    }
    let id = f.id;
    d.board.footprints.push(f);
    Ok(id)
}

/// Moves everything on the layout but its outline by `d` (mm): a board whose outline now comes
/// from elsewhere (a sketch) keeps its parts' arrangement.
pub fn shift_layout(b: &mut Board, d: [f64; 2]) {
    let d = Pt::mm(d[0], d[1]);
    for f in &mut b.footprints {
        f.placement.at = f.placement.at + d;
    }
    for t in &mut b.tracks {
        t.a = t.a + d;
        t.b = t.b + d;
        t.mid = t.mid.map(|m| m + d);
    }
    for v in &mut b.vias {
        v.at = v.at + d;
    }
    for z in &mut b.zones {
        for r in &mut z.outline {
            r.iter_mut().for_each(|p| *p = *p + d);
        }
        // Filled again when asked.
        z.filled.clear();
    }
    for s in b.shapes.iter_mut().filter(|s| s.layer != Layer::Outline) {
        s.shape.geom = s.shape.geom.translated(d);
    }
    for t in &mut b.texts {
        t.text.at = t.text.at + d;
    }
}

/// The round features of the part `part` (by its name in the Parts list) of the Part Studio
/// `studio` that stand square to the board derived there by `feature`: where each axis meets
/// the board (layout mm) and its radius. A plug's pins, a stand-off's
/// hole. `copy`: which of the feature's board copies (0 first: one per location).
pub fn axes_of_part(doc: &Document, studio: ElementId, feature: FeatureId, part: &str, copy: usize) -> Result<Vec<([f64; 2], f64)>, String> {
    let el = doc.element(studio).ok_or("no such Part Studio")?;
    let build = crate::rebuild::build(el.features());
    let out = build.derived.get(&feature).ok_or("the feature derives no board")?;
    let copy = out.boards.get(copy).ok_or_else(|| format!("the feature derives {} cop{} of the board", out.boards.len(), if out.boards.len() == 1 { "y" } else { "ies" }))?;
    let p = build.parts.iter().find(|p| crate::parts::display_name(p, el.part_props()) == part).ok_or_else(|| format!("no part {part:?} in {}", el.name))?;
    // (The board copy's frame is the layout's: its origin is where the layout's (0, 0) is.)
    let axes = super::fit::axes(&[&p.solid], &copy.frame, -1e6, 1e6);
    if axes.is_empty() {
        return Err(format!("{part} has no round feature square to the board"));
    }
    Ok(axes)
}

/// Where the mate connector `connector` (by its name in the Feature list) of the Part Studio
/// `studio` stands on the board derived there by `feature` (copy `copy`): its origin on the
/// board (layout mm), if its Z axis is square to the board (a connector on a pin's end, a
/// socket's axis).
pub fn connector_on_board(doc: &Document, studio: ElementId, feature: FeatureId, connector: &str, copy: usize) -> Result<[f64; 2], String> {
    let el = doc.element(studio).ok_or("no such Part Studio")?;
    let id = el.features().iter().find(|f| f.name == connector && matches!(f.kind, FeatureKind::MateConnector(_))).map(|f| f.id).ok_or_else(|| format!("no mate connector {connector:?} in {}", el.name))?;
    let build = crate::rebuild::build(el.features());
    let c = build.connectors.get(&id).ok_or_else(|| format!("{connector} didn't build"))?;
    let b = build.derived.get(&feature).and_then(|o| o.boards.get(copy)).ok_or("the feature derives no such board copy")?;
    match super::fit::connector_entity(c, &b.frame) {
        super::fit::Entity::Axis(p, _) => {
            let q = super::frames::to_board(&b.frame, p);
            Ok([q[0], q[1]])
        }
        _ => Err(format!("{connector}'s Z axis isn't square to the board: a pad goes onto an axis through the board")),
    }
}

/// The Derived feature of the Part Studio `studio` that derives the board `board` (the first).
pub fn deriving_feature(doc: &Document, studio: ElementId, pcb: ElementId, board: BoardId) -> Option<FeatureId> {
    doc.element(studio)?.features().iter().find_map(|f| match &f.kind {
        FeatureKind::Derived(d) if d.source.is_some_and(|r| r.element == pcb) && d.board.as_ref().is_some_and(|b| b.board == board) => Some(f.id),
        _ => None,
    })
}

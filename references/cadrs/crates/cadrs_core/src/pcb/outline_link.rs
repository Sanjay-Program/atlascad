//! A board's outline driven by a Part Studio sketch (`docs/PCB_IN_PART_STUDIO.md` §8): the
//! sketch regions picked make the board's outline (their outer boundaries) and cut-outs (their
//! holes), in the sketch's coordinates (board mm = sketch mm, both y up), and follow the sketch
//! **live**: after every command, undo and redo ([`refresh`], run with the Derived features'
//! refresh) the outline is made again when the sketch changed.
//!
//! - The link ([`OutlineLink`], on the board) names the Part Studio, the sketch and its regions
//!   ([`RegionRef`]: they follow sketch edits as an extrude's do) and the outline shapes it wrote;
//!   those are locked in the layout (its tools leave them alone) until **Break link**
//!   ([`SetOutlineLink`] with none).
//! - **Create PCB board** ([`CreateBoardFromSketch`], one undo step): a native board with the
//!   sketch's outline and the link, in a PCB Studio of the document (a new one when asked), and
//!   a Derived feature of it in the Part Studio placed on the sketch's plane with the board's
//!   origin on the sketch's origin, so the board lies exactly on its sketch.
//! - A link to a sketch whose regions no longer resolve keeps the last good outline; the board
//!   says why ([`OutlineLink::problem`]).
//! - Lines stay lines, arcs arcs (full circles circles), cubic Béziers Béziers; ellipse pieces
//!   become polylines within 1 µm.

use serde::{Deserialize, Serialize};
use uuid::Uuid;

use cadrs_eda::board::BoardShape;
use cadrs_eda::graphics::{Fill, Geom, Shape, Stroke};
use cadrs_eda::layer::Layer;
use cadrs_eda::units::{Pt, mm};

use crate::command::{Command, CommandError, Scope};
use crate::document::{Document, Element, ElementKind, FeatureKind, RegionRef};
use crate::ids::{ElementId, FeatureId};

use super::{BoardId, studio_mut};

/// Where a board's outline comes from (see the module docs).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct OutlineLink {
    /// The Part Studio.
    pub element: ElementId,
    pub sketch: FeatureId,
    /// The sketch regions that make the board.
    pub regions: Vec<RegionRef>,
    /// The outline shapes the link wrote (locked in the layout).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub shapes: Vec<Uuid>,
    /// Why the outline couldn't follow the sketch the last time it tried, if it couldn't.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
    /// The sketch's circles inside the outline are mounting holes (non-plated hole footprints,
    /// `holes`), not cut-outs of the outline.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub circles_as_holes: bool,
    /// The mounting-hole footprints the link wrote (in order, so a hole keeps its footprint).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub holes: Vec<Uuid>,
}

impl OutlineLink {
    /// A link to `regions` of the sketch `sketch` in the Part Studio `element`, before anything
    /// is written.
    pub fn new(element: ElementId, sketch: FeatureId, regions: Vec<RegionRef>, circles_as_holes: bool) -> Self {
        OutlineLink { element, sketch, regions, shapes: vec![], problem: None, circles_as_holes, holes: vec![] }
    }
}

/// A board outline taken from a sketch: the drawings and the round holes in it (centre, radius).
#[derive(Clone, Debug, PartialEq)]
pub struct SketchOutline {
    pub geoms: Vec<Geom>,
    pub circles: Vec<(Pt, cadrs_eda::units::Nm)>,
}

/// [`outline_of`] with the holes that are whole circles apart.
pub fn outline_parts(geometry: &cadrs_sketch::Sketch, regions: &[RegionRef]) -> Result<SketchOutline, String> {
    if regions.is_empty() {
        return Err("No sketch regions to make the board from".into());
    }
    let mut out = SketchOutline { geoms: vec![], circles: vec![] };
    for r in regions {
        let region = r.resolve(geometry).ok_or("A region of the sketch is gone")?;
        out.geoms.extend(region.outer_pieces.iter().map(piece_geom));
        for hole in &region.hole_pieces {
            match hole.iter().map(piece_geom).collect::<Vec<_>>().as_slice() {
                [Geom::Circle { center, radius }] => out.circles.push((*center, *radius)),
                g => out.geoms.extend(g.iter().cloned()),
            }
        }
    }
    Ok(out)
}

fn pt(v: cadrs_sketch::Vec2) -> Pt {
    Pt::mm(v.x, v.y)
}

/// One boundary piece of a sketch region as an outline drawing.
fn piece_geom(p: &cadrs_sketch::region::Piece) -> Geom {
    use cadrs_sketch::region::Piece;
    match p {
        Piece::Line(a, b) => Geom::Line { a: pt(*a), b: pt(*b) },
        Piece::Arc(g) if g.sweep.abs() >= std::f64::consts::TAU - 1e-9 => Geom::Circle { center: pt(g.center), radius: mm(g.radius) },
        Piece::Arc(g) => {
            let at = |t: f64| pt(g.center + cadrs_sketch::Vec2::new(t.cos(), t.sin()) * g.radius);
            Geom::Arc { start: at(g.start_angle), mid: at(g.start_angle + g.sweep / 2.0), end: at(g.start_angle + g.sweep) }
        }
        Piece::Bezier(b) => Geom::Bezier { pts: b.p.map(pt) },
        Piece::Ellipse { g, t0, sweep } => {
            // Chords within 1 µm: n segments with a half-chord sag r(1 − cos(Δ/2)) ≤ 0.001.
            let r = g.a.length().max(g.minor + g.offset.abs());
            let step = 2.0 * (1.0 - 0.001 / r.max(0.001)).clamp(-1.0, 1.0).acos();
            let n = ((sweep.abs() / step.max(1e-3)).ceil() as usize).clamp(4, 2000);
            Geom::Polyline { pts: (0..=n).map(|i| pt(g.point_at(t0 + sweep * i as f64 / n as f64))).collect(), closed: false }
        }
    }
}

/// The outline drawings of `regions` of the sketch `geometry`: each region's outer boundary
/// and its holes. `Err` when a region no longer resolves.
pub fn outline_of(geometry: &cadrs_sketch::Sketch, regions: &[RegionRef]) -> Result<Vec<Geom>, String> {
    Ok(outline_geoms(&outline_parts(geometry, regions)?))
}

/// An outline's drawings with its round holes as circles (cut-outs).
fn outline_geoms(o: &SketchOutline) -> Vec<Geom> {
    o.geoms.iter().cloned().chain(o.circles.iter().map(|(center, radius)| Geom::Circle { center: *center, radius: *radius })).collect()
}

/// An id made from `what` (the same each time: redoing a step that made a hole or a shape gives
/// it its id again, and its part keeps its id).
fn stable_id(what: &str) -> Uuid {
    let h = |seed: &str| cadrs_kernel::naming::stable_hash(format!("{seed}|{what}").as_bytes()) as u128;
    Uuid::from_u128(h("a") << 64 | h("b"))
}

/// The outline shapes for `geoms`, locked, their ids from `key` and their order.
fn shapes_of(geoms: Vec<Geom>, key: &str) -> Vec<BoardShape> {
    geoms.into_iter().enumerate().map(|(i, geom)| BoardShape { id: stable_id(&format!("{key}|shape {i}")), shape: Shape { geom, stroke: Stroke::width(mm(0.05)), fill: Fill::None }, layer: Layer::Outline, locked: true, net: String::new() }).collect()
}

/// The sketch `sketch` of the Part Studio `element`, as it is.
fn sketch_of(doc: &Document, element: ElementId, sketch: FeatureId) -> Result<&cadrs_sketch::Sketch, String> {
    let el = doc.element(element).ok_or("The Part Studio of the board's outline is gone")?;
    let f = el.feature(sketch).ok_or("The sketch of the board's outline is gone")?;
    f.sketch().map(|s| &s.geometry).ok_or_else(|| "The board's outline isn't a sketch".to_string())
}

/// Replaces the board's linked outline shapes (and mounting holes) with `o`'s (and its
/// mechanical board).
fn apply_outline(b: &mut super::StudioBoard, o: SketchOutline) {
    let Some(link) = b.outline_link.as_mut() else { return };
    let Some(design) = b.design.as_mut() else { return };
    let key = format!("{:?}|{:?}|{:?}", b.id, link.element, link.sketch);
    let old = std::mem::take(&mut link.shapes);
    design.board.shapes.retain(|s| !old.contains(&s.id));
    let mut geoms = o.geoms;
    if link.circles_as_holes {
        // Each circle a mounting hole footprint keeping its id (and part): the one at the same
        // place, else the i-th, else one made from its number.
        let old_holes = std::mem::take(&mut link.holes);
        let at: Vec<(Uuid, Pt)> = old_holes.iter().filter_map(|id| design.board.footprints.iter().find(|f| f.id == *id).map(|f| (*id, f.placement.at))).collect();
        design.board.footprints.retain(|f| !old_holes.contains(&f.id));
        // (Each old hole once: two circles at one place don't share it.)
        let mut ids: Vec<Option<Uuid>> = Vec::new();
        for (c, _) in &o.circles {
            let id = at.iter().find(|(id, p)| p == c && !ids.contains(&Some(*id))).map(|(id, _)| *id);
            ids.push(id);
        }
        for i in 0..ids.len() {
            if ids[i].is_none() {
                let free = |id: &Uuid| !ids.contains(&Some(*id));
                ids[i] = old_holes.get(i).copied().filter(free).or_else(|| Some(stable_id(&format!("{key}|hole {i}"))).filter(free));
            }
        }
        for (i, ((center, radius), id)) in o.circles.iter().zip(ids).enumerate() {
            let id = id.unwrap_or_else(|| stable_id(&format!("{key}|hole {i}|{center:?}")));
            design.board.footprints.push(mounting_hole(id, i + 1, *center, *radius));
            link.holes.push(id);
        }
    } else {
        geoms.extend(o.circles.into_iter().map(|(center, radius)| Geom::Circle { center, radius }));
    }
    let new = shapes_of(geoms, &key);
    link.shapes = new.iter().map(|s| s.id).collect();
    design.board.shapes.extend(new);
    b.design_changed();
}

/// A mounting hole: a non-plated hole footprint `H<n>` of the circle's size at its centre.
fn mounting_hole(id: Uuid, n: usize, center: Pt, radius: cadrs_eda::units::Nm) -> cadrs_eda::board::PlacedFootprint {
    use cadrs_eda::footprint::{FootprintPlacement, MountKind, PadKind, PadShape, new_footprint, new_pad};
    use cadrs_eda::units::{Size, to_mm};
    let d = 2 * radius;
    let mut f = new_footprint(&format!("MountingHole:MountingHole_{:.1}mm", to_mm(d)), "Mounting hole (from the outline's sketch)", MountKind::ThroughHole, Pt::new(0, radius + mm(1.0)), Pt::new(0, -radius - mm(1.0)), Pt::ZERO);
    if let Some(r) = f.field_mut(cadrs_eda::symbol::fields::REFERENCE) {
        r.text.text.text = format!("H{n}");
    }
    let mut pad = new_pad("", PadShape::Circle, Pt::ZERO, Size::new(d, d), Some(d));
    pad.kind = PadKind::NonPlated;
    pad.layers = cadrs_eda::layer::LayerSet::of(&[Layer::TopMask, Layer::BottomMask]);
    f.pads.push(pad);
    cadrs_eda::board::PlacedFootprint { id, footprint: f, placement: FootprintPlacement { at: center, ..Default::default() }, locked: true, symbol: None }
}

/// The linked mounting holes (centre, radius), in order.
fn linked_holes(b: &super::StudioBoard) -> Vec<(Pt, cadrs_eda::units::Nm)> {
    let (Some(link), Some(d)) = (&b.outline_link, b.design()) else { return vec![] };
    link.holes.iter().filter_map(|id| d.board.footprints.iter().find(|f| f.id == *id)).filter_map(|f| f.footprint.pads.first().map(|p| (f.placement.at, p.size.w / 2))).collect()
}

/// The linked shapes' drawings, in order.
fn linked_geoms(b: &super::StudioBoard) -> Vec<Geom> {
    let (Some(link), Some(d)) = (&b.outline_link, b.design()) else { return vec![] };
    link.shapes.iter().filter_map(|id| d.board.shapes.iter().find(|s| s.id == *id).map(|s| s.shape.geom.clone())).collect()
}

/// Brings every linked board outline up to date with its sketch (see the module docs).
pub fn refresh(doc: &mut Document) {
    // (tab, board, the new outline or why not)
    let mut updates: Vec<(usize, BoardId, Result<SketchOutline, String>)> = vec![];
    for (i, el) in doc.elements.iter().enumerate() {
        let Some(s) = el.pcb() else { continue };
        for b in s.boards.iter().filter(|b| b.design().is_some()) {
            let Some(link) = &b.outline_link else { continue };
            // (Its sketch may have been moved below the board's Derived feature since.)
            let want = check_order(doc, el.id, b.id, link.element, link.sketch).map_err(|e| e.to_string()).and_then(|()| sketch_of(doc, link.element, link.sketch)).and_then(|g| outline_parts(g, &link.regions));
            let now = |o: &SketchOutline| {
                if link.circles_as_holes {
                    o.geoms == linked_geoms(b) && o.circles == linked_holes(b)
                } else {
                    outline_geoms(o) == linked_geoms(b)
                }
            };
            match &want {
                Ok(o) if now(o) && link.problem.is_none() => {}
                Err(e) if link.problem.as_deref() == Some(e.as_str()) => {}
                _ => updates.push((i, b.id, want)),
            }
        }
    }
    for (i, id, want) in updates {
        let Some(b) = doc.elements[i].pcb_mut().and_then(|s| s.board_mut(id)) else { continue };
        match want {
            Ok(g) => {
                if let Some(l) = b.outline_link.as_mut() {
                    l.problem = None;
                }
                apply_outline(b, g);
            }
            Err(e) => {
                if let Some(l) = b.outline_link.as_mut() {
                    l.problem = Some(e);
                }
            }
        }
    }
}

/// Whether the board's outline shape `id` follows a sketch (and can't be edited in the layout).
pub fn is_linked(b: &super::StudioBoard, id: Uuid) -> bool {
    b.outline_link.as_ref().is_some_and(|l| l.shapes.contains(&id))
}

/// Refuses a link whose sketch comes after a Derived feature that brings the board in, in its
/// Part Studio or through a Part Studio it derives (the sketch could then be drawn on the board
/// itself: a loop).
fn check_order(doc: &Document, pcb: ElementId, board: BoardId, element: ElementId, sketch: FeatureId) -> Result<(), CommandError> {
    let el = doc.element(element).ok_or(CommandError::ElementNotFound(element))?;
    let features = el.features();
    let at = features.iter().position(|f| f.id == sketch).ok_or_else(|| CommandError::Invalid("The sketch is gone".into()))?;
    let mut seen = vec![element];
    let via = features[..at].iter().find_map(|f| brings_board(doc, f, pcb, board, &mut seen).map(|v| (f, v)));
    match via {
        None => Ok(()),
        Some((_, None)) => Err(CommandError::Invalid(format!("{} comes after the board's Derived feature: an outline can't follow a sketch that may be drawn on the board", features[at].name))),
        Some((f, Some(studio))) => Err(CommandError::Invalid(format!("{} comes after {}, which derives {studio} with the board in it: an outline can't follow a sketch that may be drawn on the board", features[at].name, f.name))),
    }
}

/// Whether the feature `f` brings the board in: `Some(None)` itself, `Some(Some(tab))` through
/// the Part Studio `tab` it derives (at any depth); `seen`: the tabs already looked through.
fn brings_board(doc: &Document, f: &crate::document::Feature, pcb: ElementId, board: BoardId, seen: &mut Vec<ElementId>) -> Option<Option<String>> {
    let FeatureKind::Derived(d) = &f.kind else { return None };
    let r = d.source?;
    if r.document.is_some_and(|x| x != doc.id) {
        return None;
    }
    if r.element == pcb {
        return d.board.as_ref().is_some_and(|b| b.board == board).then_some(None);
    }
    if seen.contains(&r.element) {
        return None;
    }
    seen.push(r.element);
    let src = doc.element(r.element)?;
    src.features().iter().any(|g| brings_board(doc, g, pcb, board, seen).is_some()).then(|| Some(src.name.clone()))
}

/// Links a board's outline to sketch regions (its outline replaced by theirs), or breaks the
/// link (`link: None`: the shapes stay, unlocked). One undo step.
#[derive(Debug, Clone)]
pub struct SetOutlineLink {
    pub pcb: ElementId,
    pub board: BoardId,
    pub link: Option<(ElementId, FeatureId, Vec<RegionRef>)>,
    /// When linking: everything else on the layout moves with the outline's centre, so the
    /// parts keep their place on the board (a layout drawn elsewhere taking a sketch's outline).
    pub keep_parts: bool,
}

impl Command for SetOutlineLink {
    fn label(&self) -> String {
        if self.link.is_some() { "Link board outline to sketch".into() } else { "Break board outline link".into() }
    }
    fn scope(&self) -> Scope {
        Scope::Whole
    }
    fn apply(&self, doc: &mut Document) -> Result<(), CommandError> {
        match &self.link {
            Some((element, sketch, regions)) => {
                check_order(doc, self.pcb, self.board, *element, *sketch)?;
                let geoms = sketch_of(doc, *element, *sketch).and_then(|g| outline_parts(g, regions)).map_err(CommandError::Invalid)?;
                let s = studio_mut(doc, self.pcb)?;
                let b = s.board_mut(self.board).ok_or_else(|| CommandError::Invalid("no such board".into()))?;
                let design = b.design.as_mut().ok_or_else(|| CommandError::Invalid("The board isn't designed in cadrs".into()))?;
                let before = super::board3d::outline_center(&design.board);
                // The sketch's outline replaces the drawn one.
                design.board.shapes.retain(|s| s.layer != Layer::Outline);
                b.outline_link = Some(OutlineLink::new(*element, *sketch, regions.clone(), false));
                apply_outline(b, geoms);
                if self.keep_parts
                    && let Some(design) = b.design.as_mut()
                {
                    let after = super::board3d::outline_center(&design.board);
                    super::ops::shift_layout(&mut design.board, [after[0] - before[0], after[1] - before[1]]);
                    b.design_changed();
                }
                Ok(())
            }
            None => {
                let s = studio_mut(doc, self.pcb)?;
                let b = s.board_mut(self.board).ok_or_else(|| CommandError::Invalid("no such board".into()))?;
                let link = b.outline_link.take().ok_or_else(|| CommandError::Invalid("The board's outline isn't linked".into()))?;
                if let Some(d) = b.design.as_mut() {
                    for sh in d.board.shapes.iter_mut().filter(|s| link.shapes.contains(&s.id)) {
                        sh.locked = false;
                    }
                }
                b.design_changed();
                Ok(())
            }
        }
    }
}

/// Part Studio → **Create PCB board** (see the module docs): one undo step.
#[derive(Debug, Clone)]
pub struct CreateBoardFromSketch {
    /// The Part Studio and its sketch, and the regions that make the board.
    pub element: ElementId,
    pub sketch: FeatureId,
    pub regions: Vec<RegionRef>,
    /// The PCB Studio to add the board to; `None`: a new one (with the id `new_pcb`).
    pub pcb: Option<ElementId>,
    pub new_pcb: ElementId,
    /// The board's name (made unique in its studio).
    pub name: String,
    /// Its thickness (mm) and which of its faces lies on the sketch.
    pub thickness: f64,
    pub seat: super::board3d::BoardSeat,
    /// The sketch's circles inside the outline as mounting holes (non-plated hole footprints),
    /// else as cut-outs.
    pub circles_as_holes: bool,
    /// The Derived feature to add.
    pub feature: FeatureId,
}

impl Command for CreateBoardFromSketch {
    fn label(&self) -> String {
        format!("Create PCB board {}", self.name)
    }
    fn scope(&self) -> Scope {
        Scope::Whole
    }
    fn apply(&self, doc: &mut Document) -> Result<(), CommandError> {
        let outline = sketch_of(doc, self.element, self.sketch).and_then(|g| outline_parts(g, &self.regions)).map_err(CommandError::Invalid)?;
        let pcb = match self.pcb {
            Some(p) => p,
            None => {
                let n = (1..).find(|n| !doc.elements.iter().any(|e| e.name == format!("PCB Studio {n}"))).unwrap_or(1);
                let mut el = Element::pcb_studio(format!("PCB Studio {n}"));
                el.id = self.new_pcb;
                doc.elements.push(el);
                self.new_pcb
            }
        };
        if !matches!(doc.element(pcb).map(|e| &e.kind), Some(ElementKind::PcbStudio(_))) {
            return Err(CommandError::Invalid("not a PCB Studio".into()));
        }
        if !(self.thickness > 0.0 && self.thickness < 100.0) {
            return Err(CommandError::Invalid("A board's thickness is a length above 0".into()));
        }
        let mut design = cadrs_eda::Design::new();
        design.board.thickness = cadrs_eda::units::mm(self.thickness);
        design.board.stackup = cadrs_eda::board::Stackup::standard(design.board.copper_layers, design.board.thickness);
        super::AddBoard { element: pcb, name: Some(self.name.clone()), design: Box::new(design), imported_from: None }.apply(doc)?;
        let s = studio_mut(doc, pcb)?;
        let board = s.active.ok_or_else(|| CommandError::Invalid("no board".into()))?;
        let b = s.board_mut(board).ok_or_else(|| CommandError::Invalid("no board".into()))?;
        b.outline_link = Some(OutlineLink::new(self.element, self.sketch, self.regions.clone(), self.circles_as_holes));
        apply_outline(b, outline);
        // The Derived feature: the board on its sketch's plane, its origin on the sketch's.
        let el = doc.element(pcb).cloned().ok_or(CommandError::ElementNotFound(pcb))?;
        let mut d = crate::derived::DerivedFeature { board: Some(Box::new(crate::derived::DerivedBoard::new(board, "", Default::default()))), ..Default::default() };
        d.fill_board_from(&el, "", "").map_err(CommandError::Invalid)?;
        if let Some(db) = d.board.as_mut() {
            db.options.anchor = super::board3d::BoardAnchor::Origin;
            db.options.seat = self.seat;
            db.on_sketch = Some(self.sketch);
        }
        d.source = Some(crate::external::SourceRef { document: None, at: crate::external::RefAt::Workspace, element: pcb, pinned: false });
        crate::commands::AddFeature { element: self.element, feature: self.feature, base_name: "Derived".into(), kind: FeatureKind::Derived(Box::new(d)) }.apply(doc)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use cadrs_sketch::region::Piece;
    use cadrs_sketch::Vec2;

    #[test]
    fn pieces_become_the_same_curves() {
        let line = piece_geom(&Piece::Line(Vec2::new(0.0, 0.0), Vec2::new(10.0, 0.0)));
        assert_eq!(line, Geom::Line { a: Pt::mm(0.0, 0.0), b: Pt::mm(10.0, 0.0) });
        // A quarter arc of radius 2 about (1, 1) from 0°: through (1 + √2, 1 + √2), to (1, 3).
        let g = cadrs_sketch::geom::ArcGeom { center: Vec2::new(1.0, 1.0), radius: 2.0, start_angle: 0.0, sweep: std::f64::consts::FRAC_PI_2 };
        let Geom::Arc { start, mid, end } = piece_geom(&Piece::Arc(g)) else { panic!() };
        assert_eq!(start, Pt::mm(3.0, 1.0));
        assert_eq!(end, Pt::mm(1.0, 3.0));
        let s2 = 2f64.sqrt();
        assert!((mid.x - mm(1.0 + s2)).abs() <= 1 && (mid.y - mm(1.0 + s2)).abs() <= 1);
        let full = cadrs_sketch::geom::ArcGeom { center: Vec2::new(5.0, 5.0), radius: 1.5, start_angle: 0.3, sweep: std::f64::consts::TAU };
        assert_eq!(piece_geom(&Piece::Arc(full)), Geom::Circle { center: Pt::mm(5.0, 5.0), radius: mm(1.5) });
    }
}

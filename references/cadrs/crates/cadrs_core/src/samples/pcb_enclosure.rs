//! An enclosure to fit a board into (`docs/PCB_IN_PART_STUDIO.md` scenarios and tests): a
//! stand-in for a mains device housing. In mm; every id is fixed.
//!
//! - Part Studio **Enclosure**: a rectangular **Tube** standing on Top (outer 44 × 44, walls
//!   2 mm, so the inside is 40 × 40; z 0…100), an **End plate** under it (z −3…0) with a
//!   12 × 24 opening, the **Plug** body in the opening (10 × 22, z −12…0) and its three pins
//!   standing up into the tube, ⌀2.4, z 0…14, at x = 0: **Live pin** at y = 8, **Earth pin** at
//!   y = 0 and **Neutral pin** at y = −8. **Board location**, a mate connector at (0, 0, 12)
//!   with the Part Studio's axes, is where a board goes (its top face flush with the pins' tips).
//! - **PCB Studio 1** with the board **AC sensor** ([`cadrs_eda::ac_board`]), designed in cadrs.

use cadrs_sketch::{PlaneRef, SketchOp, Vec2};

use crate::command::{CommandError, History};
use crate::commands::{AddExtrude, AddFeature, AddSketch, EditSketch, RenamePart, SetExtrude};
use crate::document::{Document, Element, FeatureKind};
use crate::ids::{DocumentId, ElementId, FeatureId, PartId};
use crate::mate::{ConnectorOrigin, MateConnectorFeature};

const fn fid(n: u128) -> FeatureId {
    FeatureId::from_u128(0x7e00_0000_0000_0000_0000_0000_0000_0000 + n)
}

pub const DOCUMENT: DocumentId = DocumentId::from_u128(0x7e00_0000_0000_0000_0000_0000_0000_0f00);
pub const DOCUMENT_NAME: &str = "Mains sensor";
pub const STUDIO: ElementId = ElementId::from_u128(0x7e00_0000_0000_0000_0000_0000_0000_0f01);
pub const PCB: ElementId = ElementId::from_u128(0x7e00_0000_0000_0000_0000_0000_0000_0f02);

pub const TUBE_SKETCH: FeatureId = fid(1);
pub const TUBE: FeatureId = fid(2);
pub const PLATE_SKETCH: FeatureId = fid(3);
pub const PLATE: FeatureId = fid(4);
pub const PLUG_SKETCH: FeatureId = fid(5);
pub const PLUG: FeatureId = fid(6);
pub const PINS_SKETCH: FeatureId = fid(7);
pub const PINS: FeatureId = fid(8);
pub const BOARD_LOCATION: FeatureId = fid(9);

/// The pins' centres (x, y) and names, top to bottom.
pub const PIN_AT: [(f64, f64, &str); 3] = [(0.0, 8.0, "Live pin"), (0.0, 0.0, "Earth pin"), (0.0, -8.0, "Neutral pin")];
pub const PIN_RADIUS: f64 = 1.2;
/// Flush with the board's top face once the board is on Board location (12 + 1.6).
pub const PIN_TOP: f64 = 13.6;
/// The board location's height.
pub const BOARD_Z: f64 = 12.0;

fn rect(x0: f64, y0: f64, x1: f64, y1: f64) -> SketchOp {
    SketchOp::AddPolyline { points: vec![Vec2::new(x0, y0), Vec2::new(x1, y0), Vec2::new(x1, y1), Vec2::new(x0, y1)], closed: true, construction: false, label: "Add rectangle" }
}

/// A sketch on Top with `ops`, extruded `depth` (down when `flip`) from the regions around
/// `seeds`.
#[allow(clippy::too_many_arguments)]
fn sketch_extrude(doc: &mut Document, h: &mut History, sketch: FeatureId, extrude: FeatureId, ops: Vec<SketchOp>, seeds: &[Vec2], depth: f64, flip: bool) -> Result<(), CommandError> {
    h.execute(doc, &AddSketch { element: STUDIO, feature: sketch, plane: Some(PlaneRef::Top) })?;
    for op in ops {
        h.execute(doc, &EditSketch { element: STUDIO, feature: sketch, op })?;
    }
    let geom = doc.element(STUDIO).and_then(|e| e.feature(sketch)).and_then(|f| f.sketch()).map(|s| s.geometry.clone()).ok_or(CommandError::Invalid("sketch".into()))?;
    h.execute(doc, &AddExtrude { element: STUDIO, feature: extrude, extrude: Default::default() })?;
    let mut e = super::extrude_of(super::region_refs(sketch, &geom, seeds), depth);
    e.flip = flip;
    h.execute(doc, &SetExtrude { element: STUDIO, feature: extrude, extrude: e, label: "Extrude".into() })
}

/// The document (see the module docs).
pub fn document() -> Result<Document, CommandError> {
    let mut doc = Document::empty(DOCUMENT_NAME);
    doc.id = DOCUMENT;
    let mut ps = Element::part_studio("Enclosure");
    ps.id = STUDIO;
    let mut pcb = Element::pcb_studio("PCB Studio 1");
    pcb.id = PCB;
    doc.elements.push(ps);
    doc.elements.push(pcb);
    let mut h = History::default();
    sketch_extrude(&mut doc, &mut h, TUBE_SKETCH, TUBE, vec![rect(-22.0, -22.0, 22.0, 22.0), rect(-20.0, -20.0, 20.0, 20.0)], &[Vec2::new(21.0, 0.0)], 100.0, false)?;
    sketch_extrude(&mut doc, &mut h, PLATE_SKETCH, PLATE, vec![rect(-22.0, -22.0, 22.0, 22.0), rect(-6.0, -12.0, 6.0, 12.0)], &[Vec2::new(21.0, 0.0)], 3.0, true)?;
    sketch_extrude(&mut doc, &mut h, PLUG_SKETCH, PLUG, vec![rect(-5.0, -11.0, 5.0, 11.0)], &[Vec2::new(0.0, 0.0)], 12.0, true)?;
    let circles = PIN_AT.iter().map(|(x, y, _)| SketchOp::AddCircle { center: Vec2::new(*x, *y), radius: PIN_RADIUS, construction: false }).collect();
    let seeds: Vec<Vec2> = PIN_AT.iter().map(|(x, y, _)| Vec2::new(*x, *y)).collect();
    sketch_extrude(&mut doc, &mut h, PINS_SKETCH, PINS, circles, &seeds, PIN_TOP, false)?;
    for (part, name) in [(PartId::new(TUBE, 0), "Tube"), (PartId::new(PLATE, 0), "End plate"), (PartId::new(PLUG, 0), "Plug")] {
        h.execute(&mut doc, &RenamePart { element: STUDIO, part, name: name.into() })?;
    }
    // The pins: one part per circle, named by where they are.
    let build = crate::rebuild::build(doc.element(STUDIO).map(|e| e.features()).unwrap_or_default());
    for p in build.parts.iter().filter(|p| p.feature == PINS) {
        let Some((lo, hi)) = p.solid.bounds() else { continue };
        let y = (lo[1] + hi[1]) / 2.0;
        if let Some((_, _, name)) = PIN_AT.iter().find(|(_, py, _)| (py - y).abs() < 1.0) {
            h.execute(&mut doc, &RenamePart { element: STUDIO, part: p.id, name: (*name).into() })?;
        }
    }
    let mc = MateConnectorFeature { origin: Some(ConnectorOrigin::Origin), move_on: true, offset: [0.0, 0.0, BOARD_Z], offset_expr: ["0 mm".into(), "0 mm".into(), format!("{BOARD_Z} mm")], ..Default::default() };
    h.execute(&mut doc, &AddFeature { element: STUDIO, feature: BOARD_LOCATION, base_name: "Board location".into(), kind: FeatureKind::MateConnector(mc) })?;
    let lib = cadrs_eda::library::LibraryTable::builtin();
    h.execute(&mut doc, &crate::pcb::AddBoard { element: PCB, name: Some("AC sensor".into()), design: Box::new(cadrs_eda::ac_board::design(&lib)), imported_from: None })?;
    Ok(doc)
}

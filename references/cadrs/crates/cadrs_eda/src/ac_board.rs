//! A stand-in for an AC current-sensing board (the `docs/PCB_IN_PART_STUDIO.md` scenarios): a
//! 31 × 39 mm board with its top corners cut, three wire pads for mains in and out (J4 live
//! in, J5 neutral in, J3 live out after the sensor), the current sensor U2 (SOIC-8), two
//! 2 × 10 headers for a microcontroller board (J1, J2), a few 0805 parts, a large AC/DC module
//! PS1 on the bottom and a smaller one, U1, not yet on the board (left of it), so the
//! scenarios can swap one for the other and line the wire pads up with a plug's pins.
//!
//! Layout coordinates are mm from the board's lower-left corner. Every id is fixed, so the
//! scenarios' screenshots are reproducible.

use crate::Design;
use crate::board::{Board, PlacedFootprint};
use crate::footprint::{Footprint, FootprintPlacement, Model3d, MountKind, PadShape, new_footprint, new_pad};
use crate::graphics::Geom;
use crate::layer::{Layer, Side};
use crate::library::LibraryTable;
use crate::model3d::{Axis, Body};
use crate::symbol::fields;
use crate::units::{Pt, Size, mm};
use uuid::Uuid;

/// The board's width and height (mm) and the size of its two top corner cuts.
pub const WIDTH: f64 = 31.0;
pub const HEIGHT: f64 = 39.0;
pub const CORNER: f64 = 2.5;

const fn uid(n: u128) -> Uuid {
    Uuid::from_u128(0xac00_0000_0000_0000_0000_0000_0000_0000 + n)
}

/// The footprints' ids, by reference.
pub const J1: Uuid = uid(1);
pub const J2: Uuid = uid(2);
pub const J3: Uuid = uid(3);
pub const J4: Uuid = uid(4);
pub const J5: Uuid = uid(5);
pub const U2: Uuid = uid(6);
pub const PS1: Uuid = uid(7);
pub const U1: Uuid = uid(8);
pub const R1: Uuid = uid(9);
pub const C1: Uuid = uid(10);

/// A wire pad (a 2.5 mm² wire soldered straight in): a round 3.6 mm pad, a 2.4 mm hole, and a
/// stub of wire as its model.
pub fn wire_pad() -> Footprint {
    let mut f = new_footprint("Connector_Wire:SolderWire-2.5sqmm_1x01_D2.4mm_OD3.6mm", "Soldered wire, 2.5 mm²", MountKind::ThroughHole, Pt::mm(0.0, 3.0), Pt::mm(0.0, -3.0), Pt::ZERO);
    f.pads.push(new_pad("1", PadShape::Circle, Pt::ZERO, Size::mm(3.6, 3.6), Some(mm(2.4))));
    let mut body = Body::default();
    body.cylinder([0.0, 0.0, 0.0], Axis::Z, 1.5, 4.0, [200, 120, 40]);
    f.models.push(Model3d { body: Some(body), ..Model3d::file("") });
    f
}

/// An AC/DC module with four pins (two mains in at one end, two DC out at the other), its
/// body `l` × `w` × `h` mm centred between them, pins `pitch` apart lengthways.
pub fn acdc_module(id: &str, l: f64, w: f64, h: f64, pitch: f64, color: [u8; 3]) -> Footprint {
    let mut f = new_footprint(id, "AC/DC power module", MountKind::ThroughHole, Pt::mm(0.0, w / 2.0 + 1.5), Pt::mm(0.0, -w / 2.0 - 1.5), Pt::ZERO);
    let x = pitch / 2.0;
    for (n, at) in [("1", Pt::mm(-x, 2.5)), ("2", Pt::mm(-x, -2.5)), ("3", Pt::mm(x, 5.0)), ("4", Pt::mm(x, -5.0))] {
        f.pads.push(new_pad(n, PadShape::Circle, at, Size::mm(2.0, 2.0), Some(mm(1.0))));
    }
    f.shapes.push(crate::footprint::FpShape {
        id: Uuid::new_v4(),
        shape: crate::graphics::Shape { geom: Geom::Rect { a: Pt::mm(-l / 2.0 - 0.25, -w / 2.0 - 0.25), b: Pt::mm(l / 2.0 + 0.25, w / 2.0 + 0.25) }, stroke: crate::graphics::Stroke::width(mm(0.05)), fill: crate::graphics::Fill::None },
        layer: Layer::TopCourtyard,
    });
    let mut body = Body::default();
    body.cuboid([-l / 2.0, -w / 2.0, 0.0], [l / 2.0, w / 2.0, h], color);
    f.models.push(Model3d { body: Some(body), ..Model3d::file("") });
    f
}

#[allow(clippy::too_many_arguments)]
fn place(id: Uuid, mut fp: Footprint, reference: &str, value: &str, at: (f64, f64), angle: f64, side: Side, nets: &[(&str, &str)]) -> PlacedFootprint {
    if let Some(r) = fp.field_mut(fields::REFERENCE) {
        r.text.text.text = reference.into();
    }
    if let Some(v) = fp.field_mut(fields::VALUE) {
        v.text.text.text = value.into();
    }
    for (pad, net) in nets {
        for p in fp.pads.iter_mut().filter(|p| p.number == *pad) {
            p.net = Some(net.to_string());
        }
    }
    PlacedFootprint { id, footprint: fp, placement: FootprintPlacement { at: Pt::mm(at.0, at.1), angle, side }, locked: false, symbol: None }
}

/// The board's layout (see the module docs).
pub fn board(lib: &LibraryTable) -> Board {
    let mut b = Board::default();
    let (w, h, c) = (WIDTH, HEIGHT, CORNER);
    let pts = [(0.0, 0.0), (w, 0.0), (w, h - c), (w - c, h), (c, h), (0.0, h - c)];
    for i in 0..pts.len() {
        let (a, z) = (pts[i], pts[(i + 1) % pts.len()]);
        crate::board_edit::add_shape(&mut b, Geom::Line { a: Pt::mm(a.0, a.1), b: Pt::mm(z.0, z.1) }, Layer::Outline);
    }
    let fp = |name: &str| lib.footprint(name).unwrap_or_else(|| panic!("{name}")).clone();
    let header = fp("Connector_PinHeader_2.54mm:PinHeader_2x10_P2.54mm_Vertical");
    b.footprints = vec![
        place(J1, header.clone(), "J1", "MCU left", (1.5, 31.5), 0.0, Side::Top, &[("1", "GND"), ("11", "CURRENT")]),
        place(J2, header, "J2", "MCU right", (26.9, 31.5), 0.0, Side::Top, &[("2", "GND"), ("15", "5V")]),
        place(J4, wire_pad(), "J4", "AC Live IN", (14.0, 3.5), 0.0, Side::Top, &[("1", "LIVE")]),
        place(J5, wire_pad(), "J5", "AC Neutral IN", (5.0, 3.5), 0.0, Side::Top, &[("1", "NEUTRAL")]),
        place(J3, wire_pad(), "J3", "AC Live OUT", (22.0, 3.5), 0.0, Side::Top, &[("1", "LOAD")]),
        place(U2, fp("Package_SO:SOIC-8_3.9x4.9mm_P1.27mm"), "U2", "ACS712", (18.0, 10.5), 90.0, Side::Top, &[("1", "LIVE"), ("2", "LIVE"), ("3", "LOAD"), ("4", "LOAD"), ("5", "GND"), ("8", "5V")]),
        place(R1, fp("Resistor_SMD:R_0805_2012Metric"), "R1", "10k", (18.0, 16.5), 0.0, Side::Top, &[("1", "CURRENT")]),
        place(C1, fp("Capacitor_SMD:C_0805_2012Metric"), "C1", "100nF", (13.5, 16.5), 0.0, Side::Top, &[("1", "GND"), ("2", "5V")]),
        place(PS1, acdc_module("Converter_ACDC:HLK-PM01", 34.0, 20.0, 15.0, 29.0, [30, 30, 30]), "PS1", "HLK-PM01", (15.5, 20.0), 90.0, Side::Bottom, &[("1", "LIVE"), ("2", "NEUTRAL"), ("3", "GND"), ("4", "5V")]),
        place(U1, acdc_module("Converter_ACDC:HLK-2M05", 25.0, 15.0, 12.0, 20.0, [40, 40, 44]), "U1", "HLK-2M05", (-18.0, 19.0), 90.0, Side::Bottom, &[("1", "LIVE"), ("2", "NEUTRAL"), ("3", "GND"), ("4", "5V")]),
    ];
    let mut nets: Vec<String> = b.footprints.iter().flat_map(|f| f.footprint.pads.iter().filter_map(|p| p.net.clone())).collect();
    nets.sort();
    nets.dedup();
    b.nets = nets;
    crate::board_edit::add_text(&mut b, "AC SENSOR", Pt::mm(15.5, 25.0), Layer::TopSilk);
    b
}

/// The design: the layout, an empty schematic sheet.
pub fn design(lib: &LibraryTable) -> Design {
    let mut d = Design { board: board(lib), ..Design::new() };
    d.ensure_sheet();
    d
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_is_closed_and_its_parts_sit_where_the_docs_say() {
        let lib = LibraryTable::builtin();
        let b = board(&lib);
        let region = crate::outline::board_region(&b);
        let area = crate::poly::area(&region) / 1e12;
        assert!((area - (WIDTH * HEIGHT - CORNER * CORNER)).abs() < 1e-6, "{area}");
        assert_eq!(b.footprints.len(), 10);
        let j4 = b.footprints.iter().find(|f| f.id == J4).unwrap();
        assert_eq!(j4.reference(), "J4");
        assert_eq!(j4.footprint.pads[0].net.as_deref(), Some("LIVE"));
        // U1 waits off the board, left of it.
        let u1 = b.footprints.iter().find(|f| f.id == U1).unwrap();
        assert!(u1.placement.at.x < 0);
        assert!(b.nets.contains(&"LOAD".to_string()));
    }
}

//! Board editing (GS16–GS17, GS25): moving, rotating and flipping footprints, dragging them
//! with their tracks attached, routing tracks with 45° bends, vias, selecting a connection's
//! tracks (U) and deleting.

use crate::board::{Board, Track, Via, ViaKind};
use crate::copper;
use crate::footprint::PadKind;
use crate::layer::{Layer, Side};
use crate::poly;
use crate::units::{Nm, Pt, normalize_deg};
use uuid::Uuid;

pub fn footprint_index(board: &Board, reference: &str) -> Option<usize> {
    board.footprints.iter().position(|f| f.reference() == reference)
}

/// Moves a footprint to `at` (M).
pub fn move_footprint(board: &mut Board, fp: usize, at: Pt) {
    board.footprints[fp].placement.at = at;
}

/// Turns a footprint by `deg` counter-clockwise about its anchor (R: 90).
pub fn rotate_footprint(board: &mut Board, fp: usize, deg: f64) {
    let p = &mut board.footprints[fp].placement;
    p.angle = normalize_deg(p.angle + deg);
}

/// Flips a footprint to the other side, mirrored left to right about its anchor (F).
pub fn flip_footprint(board: &mut Board, fp: usize) {
    let p = &mut board.footprints[fp].placement;
    p.side = p.side.flipped();
    p.angle = normalize_deg(180.0 - p.angle);
}

/// Track ends lying on footprint `fp`'s pads: (track index, which end: 0 = a, 1 = b).
fn attached_ends(board: &Board, fp: usize) -> Vec<(usize, u8)> {
    let mut out = vec![];
    let f = &board.footprints[fp];
    for (pi, pad) in f.footprint.pads.iter().enumerate() {
        let region = copper::pad_region(board, fp, pi, 0);
        for (ti, t) in board.tracks.iter().enumerate() {
            if !f.placement.layers(pad.layers).contains(t.layer) {
                continue;
            }
            for (e, p) in [(0u8, t.a), (1u8, t.b)] {
                if poly::contains(&region, p) {
                    out.push((ti, e));
                }
            }
        }
    }
    out
}

/// Drags a footprint by `d` (D): the track ends on its pads follow, so the tracks stay
/// connected (they stretch).
pub fn drag_footprint(board: &mut Board, fp: usize, d: Pt) {
    let ends = attached_ends(board, fp);
    board.footprints[fp].placement.at = board.footprints[fp].placement.at + d;
    for (ti, e) in ends {
        let t = &mut board.tracks[ti];
        if e == 0 { t.a = t.a + d } else { t.b = t.b + d }
    }
}

/// The pad under `p` on a copper layer: (footprint, pad).
pub fn pad_at(board: &Board, p: Pt) -> Option<(usize, usize)> {
    for (fi, f) in board.footprints.iter().enumerate() {
        for (pi, _) in f.footprint.pads.iter().enumerate() {
            if poly::contains(&copper::pad_region(board, fi, pi, 0), p) {
                return Some((fi, pi));
            }
        }
    }
    None
}

/// The pad's centre on the board.
pub fn pad_center(board: &Board, fp: usize, pad: usize) -> Pt {
    let f = &board.footprints[fp];
    f.placement.apply(f.footprint.pads[pad].at)
}

/// The layer a route starting at `p` goes on: an SMD pad's own layer (a bottom pad switches
/// to the bottom), else `active`.
pub fn start_layer(board: &Board, p: Pt, active: Layer) -> Layer {
    if let Some((fi, pi)) = pad_at(board, p) {
        let f = &board.footprints[fi];
        let pad = &f.footprint.pads[pi];
        if pad.kind == PadKind::Smd || pad.kind == PadKind::Connector {
            return if f.placement.side == Side::Bottom { Layer::BottomCopper } else { Layer::TopCopper };
        }
    }
    active
}

/// The net of the copper under `p` on `layer` (pads, tracks, vias), "" when none.
pub fn net_at(board: &Board, p: Pt, layer: Layer) -> String {
    if let Some((fi, pi)) = pad_at(board, p) {
        return board.footprints[fi].footprint.pads[pi].net.clone().unwrap_or_default();
    }
    if let Some(v) = board.vias.iter().find(|v| v.at.dist(p) <= v.diameter as f64 / 2.0) {
        return v.net.clone();
    }
    // A track ending here that has a net (another one there may be new, without one yet).
    board.tracks.iter().filter(|t| t.layer == layer && (t.a == p || t.b == p)).map(|t| t.net.clone()).find(|n| !n.is_empty()).unwrap_or_default()
}

/// The 45° path from `a` to `b`: a straight run then a diagonal (or the reverse with
/// `diagonal_first`), as the router lays a track between two clicks.
pub fn posture(a: Pt, b: Pt, diagonal_first: bool) -> Vec<Pt> {
    let (dx, dy) = (b.x - a.x, b.y - a.y);
    if dx == 0 || dy == 0 || dx.abs() == dy.abs() {
        return vec![a, b];
    }
    let diag = dx.abs().min(dy.abs());
    let d = Pt::new(diag * dx.signum(), diag * dy.signum());
    let mid = if diagonal_first { a + d } else { b - d };
    vec![a, mid, b]
}

/// Lays tracks along `pts` on `layer` with the net's track width (from its class unless
/// `width` is given); the net is taken from the copper at the start. Returns the new ids.
pub fn route(board: &mut Board, pts: &[Pt], layer: Layer, width: Option<Nm>) -> Vec<Uuid> {
    let net = pts.first().map(|p| net_at(board, *p, layer)).unwrap_or_default();
    let w = width.unwrap_or_else(|| board.rules.class_of(&net).track_width);
    let mut ids = vec![];
    for s in pts.windows(2) {
        if s[0] == s[1] {
            continue;
        }
        let id = Uuid::new_v4();
        board.tracks.push(Track { id, a: s[0], mid: None, b: s[1], width: w, layer, net: net.clone(), locked: false });
        ids.push(id);
    }
    ids
}

/// Adds a through via at `at` on `net` with the net class's size (V while routing).
pub fn add_via(board: &mut Board, at: Pt, net: &str) -> Uuid {
    let c = board.rules.class_of(net);
    let id = Uuid::new_v4();
    let (diameter, drill) = (c.via_diameter, c.via_drill);
    board.vias.push(Via { id, at, diameter, drill, kind: ViaKind::Through, from: Layer::TopCopper, to: Layer::BottomCopper, net: net.into(), locked: false, tented: None });
    id
}

/// The tracks of the connection a track belongs to (U): following shared ends both ways, up
/// to pads, and up to vias — or, pressing U again (`through_vias`), on through them, the vias
/// included.
pub fn select_connected(board: &Board, track: Uuid, through_vias: bool) -> Vec<Uuid> {
    let Some(start) = board.tracks.iter().position(|t| t.id == track) else { return vec![] };
    let on_pad = |p: Pt, layer: Layer| -> bool {
        pad_at(board, p).is_some_and(|(fi, pi)| board.footprints[fi].placement.layers(board.footprints[fi].footprint.pads[pi].layers).contains(layer))
    };
    let mut seen = vec![start];
    let mut vias: Vec<Uuid> = vec![];
    let mut todo = vec![start];
    while let Some(i) = todo.pop() {
        let t = &board.tracks[i];
        for end in [t.a, t.b] {
            if on_pad(end, t.layer) {
                continue;
            }
            let via = board.vias.iter().find(|v| v.at == end);
            if let Some(v) = via {
                if !through_vias {
                    continue;
                }
                if !vias.contains(&v.id) {
                    vias.push(v.id);
                }
            }
            for (j, o) in board.tracks.iter().enumerate() {
                let same_layer = o.layer == t.layer || via.is_some();
                if !seen.contains(&j) && same_layer && (o.a == end || o.b == end) {
                    seen.push(j);
                    todo.push(j);
                }
            }
        }
    }
    seen.into_iter().map(|i| board.tracks[i].id).chain(vias).collect()
}

pub fn delete_tracks(board: &mut Board, ids: &[Uuid]) {
    board.tracks.retain(|t| !ids.contains(&t.id));
    board.vias.retain(|v| !ids.contains(&v.id));
}


/// The line width drawings take on `layer` unless told: KiCad's defaults (silkscreen 0.12 mm,
/// fabrication 0.1, courtyard and outline 0.05, copper 0.2, the rest 0.15).
pub fn default_width(layer: Layer) -> Nm {
    let mm = crate::units::mm;
    match layer {
        Layer::TopSilk | Layer::BottomSilk => mm(0.12),
        Layer::TopFab | Layer::BottomFab => mm(0.1),
        Layer::TopCourtyard | Layer::BottomCourtyard | Layer::Outline => mm(0.05),
        l if l.is_copper() => mm(0.2),
        _ => mm(0.15),
    }
}

/// Draws a line, rectangle or circle on `layer` (the Draw tools); returns its id.
pub fn add_shape(board: &mut Board, geom: crate::graphics::Geom, layer: Layer) -> Uuid {
    let id = Uuid::new_v4();
    let shape = crate::graphics::Shape { geom, stroke: crate::graphics::Stroke::width(default_width(layer)), fill: crate::graphics::Fill::None };
    board.shapes.push(crate::board::BoardShape { id, shape, layer, locked: false, net: String::new() });
    id
}

/// Writes `text` on `layer` at `at` (1 mm high, 0.15 mm strokes; mirrored on the bottom so it
/// reads from below); returns its id.
pub fn add_text(board: &mut Board, text: &str, at: Pt, layer: Layer) -> Uuid {
    let id = Uuid::new_v4();
    let style = crate::graphics::TextStyle {
        size: crate::units::Size::mm(1.0, 1.0),
        thickness: Some(crate::units::mm(0.15)),
        mirrored: layer.side() == Some(Side::Bottom),
        ..Default::default()
    };
    let t = crate::graphics::Text { text: text.into(), at, angle: 0.0, style, visible: true };
    board.texts.push(crate::board::BoardText { id, text: t, layer, locked: false, knockout: false });
    id
}

/// Puts artwork on `layer`: each polygon a filled shape (a logo from [`crate::graphic_import`]).
/// Returns their ids.
pub fn add_artwork(board: &mut Board, polys: &[Vec<Pt>], layer: Layer) -> Vec<Uuid> {
    polys
        .iter()
        .filter(|p| p.len() >= 3)
        .map(|pts| {
            let id = Uuid::new_v4();
            let shape = crate::graphics::Shape { geom: crate::graphics::Geom::Polyline { pts: pts.clone(), closed: true }, stroke: crate::graphics::Stroke::width(0), fill: crate::graphics::Fill::Outline };
            board.shapes.push(crate::board::BoardShape { id, shape, layer, locked: false, net: String::new() });
            id
        })
        .collect()
}
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn copper_areas_and_text_are_picked_moved_and_reshaped() {
        use crate::units::mm;
        let mut b = Board::default();
        let square = vec![Pt::mm(0.0, 0.0), Pt::mm(10.0, 0.0), Pt::mm(10.0, 10.0), Pt::mm(0.0, 10.0)];
        let area = add_polygon(&mut b, square, Layer::TopCopper, "LIVE");
        let text = add_text(&mut b, "L", Pt::mm(30.0, 0.0), Layer::TopSilk);
        // A filled area is picked anywhere inside it, not only on its edge.
        assert_eq!(hit(&b, Pt::mm(5.0, 5.0), 0), Some(BoardItem::Shape(area)));
        assert_eq!(b.shapes[0].net, "LIVE");
        // Moved, with the text.
        move_items(&mut b, &[BoardItem::Shape(area), BoardItem::Text(text)], Pt::mm(1.0, 2.0));
        assert_eq!(hit(&b, Pt::mm(10.5, 11.5), 0), Some(BoardItem::Shape(area)));
        assert_eq!(b.texts[0].text.at, Pt::mm(31.0, 2.0));
        // A corner dragged out: the area grows that way.
        let c = corner_at(&b, BoardItem::Shape(area), Pt::mm(11.1, 12.1), mm(0.5)).unwrap();
        move_corner(&mut b, BoardItem::Shape(area), c.0, c.1, Pt::mm(20.0, 20.0));
        assert_eq!(hit(&b, Pt::mm(14.0, 14.0), 0), Some(BoardItem::Shape(area)));
        // A zone's corner too; its fill is taken off to be poured again.
        let z = crate::zone::add_zone(&mut b, "GND", Layer::BottomCopper, vec![Pt::mm(50.0, 0.0), Pt::mm(60.0, 0.0), Pt::mm(60.0, 10.0)]);
        crate::zone::fill_all(&mut b);
        move_corner(&mut b, BoardItem::Zone(z), 0, 1, Pt::mm(70.0, 0.0));
        let zone = b.zones.iter().find(|x| x.id == z).unwrap();
        assert_eq!(zone.outline[0][1], Pt::mm(70.0, 0.0));
        assert!(zone.filled.is_empty());
    }

    #[test]
    fn a_box_selects_what_is_inside_or_what_it_touches() {
        use crate::units::Bounds;
        let mut b = Board::default();
        let area = add_polygon(&mut b, vec![Pt::mm(0.0, 0.0), Pt::mm(10.0, 0.0), Pt::mm(10.0, 10.0)], Layer::TopCopper, "");
        let text = add_text(&mut b, "L", Pt::mm(30.0, 5.0), Layer::TopSilk);
        let r = Bounds { min: Pt::mm(-1.0, -1.0), max: Pt::mm(12.0, 12.0) };
        assert_eq!(box_select(&b, r, false), vec![BoardItem::Shape(area)]);
        // Over half of both: a window takes neither, a crossing both.
        let r = Bounds { min: Pt::mm(5.0, -1.0), max: Pt::mm(30.0, 12.0) };
        assert!(box_select(&b, r, false).is_empty());
        assert_eq!(box_select(&b, r, true), vec![BoardItem::Shape(area), BoardItem::Text(text)]);
        // Moved together.
        let found = box_select(&b, r, true);
        move_items(&mut b, &found, Pt::mm(0.0, 5.0));
        assert_eq!(b.texts[0].text.at, Pt::mm(30.0, 10.0));
        assert_eq!(hit(&b, Pt::mm(9.0, 7.0), 0), Some(BoardItem::Shape(area)));
    }

    #[test]
    fn a_board_seen_from_one_side_picks_that_sides_footprint() {
        // The AC sensor sample: C1 on top, over PS1 (a large module) on the bottom.
        let b = crate::ac_board::board(&crate::library::LibraryTable::builtin());
        let id = |r: &str| b.footprints[footprint_index(&b, r).unwrap()].id;
        let at = b.footprints[footprint_index(&b, "C1").unwrap()].placement.at;
        assert_eq!(hit_from(&b, at, 0, Some(crate::layer::Side::Top)), Some(BoardItem::Footprint(id("C1"))));
        assert_eq!(hit_from(&b, at, 0, Some(crate::layer::Side::Bottom)), Some(BoardItem::Footprint(id("PS1"))));
    }

    #[test]
    fn postures() {
        let p = Pt::mm;
        assert_eq!(posture(p(0.0, 0.0), p(10.0, 0.0), false).len(), 2);
        let s = posture(p(0.0, 0.0), p(10.0, 3.0), false);
        assert_eq!(s, vec![p(0.0, 0.0), p(7.0, 0.0), p(10.0, 3.0)]);
        let d = posture(p(0.0, 0.0), p(10.0, 3.0), true);
        assert_eq!(d[1], p(3.0, 3.0));
    }
}

/// Something on a board that can be selected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum BoardItem {
    Footprint(Uuid),
    Track(Uuid),
    Via(Uuid),
    Zone(Uuid),
    /// A drawn shape (the board outline, silkscreen art).
    Shape(Uuid),
    /// Text drawn on a layer.
    Text(Uuid),
}

impl BoardItem {
    pub fn id(self) -> Uuid {
        match self {
            BoardItem::Footprint(i) | BoardItem::Track(i) | BoardItem::Via(i) | BoardItem::Zone(i) | BoardItem::Shape(i) | BoardItem::Text(i) => i,
        }
    }
}

fn seg_dist(p: Pt, a: Pt, b: Pt) -> f64 {
    let (dx, dy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
    let l2 = dx * dx + dy * dy;
    let t = if l2 == 0.0 { 0.0 } else { (((p.x - a.x) as f64 * dx + (p.y - a.y) as f64 * dy) / l2).clamp(0.0, 1.0) };
    (p.x as f64 - (a.x as f64 + t * dx)).hypot(p.y as f64 - (a.y as f64 + t * dy))
}

/// The item under `p` (within `tol`): vias, then pads (their footprint), tracks, footprints
/// by their courtyard or drawing, drawn shapes (the outline), then zones.
pub fn hit(board: &Board, p: Pt, tol: Nm) -> Option<BoardItem> {
    hit_from(board, p, tol, None)
}

/// [`hit`] seen from one side of the board (a 3D view of it from above or below): a footprint on
/// that side wins over one on the other side under it.
pub fn hit_from(board: &Board, p: Pt, tol: Nm, side: Option<crate::layer::Side>) -> Option<BoardItem> {
    if let Some(v) = board.vias.iter().find(|v| v.at.dist(p) <= (v.diameter / 2 + tol) as f64) {
        return Some(BoardItem::Via(v.id));
    }
    // A pad wins over the track ending on it (one of the side seen first).
    let mut pads: Vec<usize> = (0..board.footprints.len()).collect();
    pads.sort_by_key(|fi| side.is_some_and(|sd| board.footprints[*fi].placement.side != sd));
    for fi in pads {
        if (0..board.footprints[fi].footprint.pads.len()).any(|pi| poly::contains(&copper::pad_region(board, fi, pi, 0), p)) {
            return Some(BoardItem::Footprint(board.footprints[fi].id));
        }
    }
    if let Some(t) = board.tracks.iter().find(|t| seg_dist(p, t.a, t.b) <= (t.width / 2 + tol) as f64) {
        return Some(BoardItem::Track(t.id));
    }
    let order = |f: &&crate::board::PlacedFootprint| side.is_some_and(|sd| f.placement.side != sd);
    let mut fps: Vec<&crate::board::PlacedFootprint> = board.footprints.iter().rev().collect();
    fps.sort_by_key(order);
    for f in fps {
        let mut b: Option<crate::units::Bounds> = None;
        for s in f.footprint.shapes.iter().filter(|s| matches!(s.layer, Layer::TopCourtyard | Layer::TopFab | Layer::TopSilk)) {
            for q in s.shape.geom.extent() {
                b = Some(crate::units::Bounds::union(b, crate::units::Bounds::of(f.placement.apply(q))));
            }
        }
        if b.is_some_and(|b| b.grow(tol).contains(p)) {
            return Some(BoardItem::Footprint(f.id));
        }
    }
    // Text by its box, drawn shapes by their line (the outline's edge, silkscreen art).
    if let Some(t) = board.texts.iter().rev().find(|t| crate::font::bounds(&t.text).is_some_and(|b| b.grow(tol).contains(p))) {
        return Some(BoardItem::Text(t.id));
    }
    for s in board.shapes.iter().rev() {
        let (pts, closed) = poly::geom_points(&s.shape.geom);
        let w = s.shape.stroke.width.max(1) + 2 * tol;
        let r = if closed { poly::stroke_closed(&pts, w) } else { poly::stroke(&pts, w) };
        // A filled shape (a copper area, a logo) anywhere inside it.
        let inside = closed && s.shape.fill != crate::graphics::Fill::None && poly::contains(&vec![pts.clone()], p);
        if inside || poly::contains(&r, p) {
            return Some(BoardItem::Shape(s.id));
        }
    }
    board.zones.iter().find(|z| z.outline.iter().any(|r| poly::contains(&vec![r.clone()], p))).map(|z| BoardItem::Zone(z.id))
}

/// Moves items by `d`: tracks and vias, zones (their fills with them), drawn shapes, text,
/// footprints.
pub fn move_items(board: &mut Board, items: &[BoardItem], d: Pt) {
    let ids: Vec<Uuid> = items.iter().map(|i| i.id()).collect();
    for t in board.tracks.iter_mut().filter(|t| ids.contains(&t.id)) {
        (t.a, t.b) = (t.a + d, t.b + d);
    }
    for v in board.vias.iter_mut().filter(|v| ids.contains(&v.id)) {
        v.at = v.at + d;
    }
    for z in board.zones.iter_mut().filter(|z| ids.contains(&z.id)) {
        z.outline.iter_mut().flatten().for_each(|p| *p = *p + d);
        for (_, polys) in &mut z.filled {
            for poly in polys {
                poly.outer.iter_mut().chain(poly.holes.iter_mut().flatten()).for_each(|p| *p = *p + d);
            }
        }
    }
    for s in board.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
        s.shape.geom = s.shape.geom.translated(d);
    }
    for t in board.texts.iter_mut().filter(|t| ids.contains(&t.id)) {
        t.text.at = t.text.at + d;
    }
    for f in board.footprints.iter_mut().filter(|f| ids.contains(&f.id)) {
        f.placement.at = f.placement.at + d;
    }
}

/// Moves the track ends (and vias) of `net` lying on any of `points` by `d`: dragging a track
/// (both its ends) stretches the tracks joined to it, dragging an end or a corner moves just
/// that point. Tracks left with no length go.
pub fn move_track_points(board: &mut Board, net: &str, points: &[Pt], d: Pt) {
    for t in board.tracks.iter_mut().filter(|t| t.net == net) {
        if points.contains(&t.a) {
            t.a = t.a + d;
        }
        if points.contains(&t.b) {
            t.b = t.b + d;
        }
    }
    for v in board.vias.iter_mut().filter(|v| v.net == net && points.contains(&v.at)) {
        v.at = v.at + d;
    }
    board.tracks.retain(|t| t.a != t.b);
}

/// Drags track `id` by `d`: the tracks joined at its ends stretch to follow, and an end on a
/// pad or via of its net stays on it through a new short track (from there to where the end
/// went), so nothing comes loose.
pub fn drag_track(board: &mut Board, id: Uuid, d: Pt) {
    let Some(t) = board.tracks.iter().find(|t| t.id == id).cloned() else { return };
    if d == Pt::ZERO {
        return;
    }
    let anchored = |b: &Board, p: Pt| {
        b.vias.iter().any(|v| v.net == t.net && v.at == p)
            || pad_at(b, p).is_some_and(|(fi, pi)| {
                let f = &b.footprints[fi];
                f.footprint.pads[pi].net.as_deref() == Some(t.net.as_str()) && f.placement.layers(f.footprint.pads[pi].layers).contains(t.layer)
            })
    };
    let mut stretch = vec![];
    for end in [t.a, t.b] {
        if anchored(board, end) {
            board.tracks.push(Track { id: Uuid::new_v4(), a: end, mid: None, b: end + d, width: t.width, layer: t.layer, net: t.net.clone(), locked: false });
        } else {
            stretch.push(end);
        }
    }
    if let Some(x) = board.tracks.iter_mut().find(|x| x.id == id) {
        (x.a, x.b) = (t.a + d, t.b + d);
    }
    for x in board.tracks.iter_mut().filter(|x| x.id != id && x.net == t.net && x.layer == t.layer) {
        if stretch.contains(&x.a) {
            x.a = x.a + d;
        }
        if stretch.contains(&x.b) {
            x.b = x.b + d;
        }
    }
    board.tracks.retain(|x| x.a != x.b);
}

/// The middle of what `items` cover (their corners, ends and anchors), on `grid`: where they
/// turn and flip about.
pub fn items_center(board: &Board, items: &[BoardItem], grid: Nm) -> Pt {
    use crate::units::Bounds;
    let ids: Vec<Uuid> = items.iter().map(|i| i.id()).collect();
    let mut pts: Vec<Pt> = vec![];
    pts.extend(board.tracks.iter().filter(|t| ids.contains(&t.id)).flat_map(|t| [t.a, t.b]));
    pts.extend(board.vias.iter().filter(|v| ids.contains(&v.id)).map(|v| v.at));
    pts.extend(board.zones.iter().filter(|z| ids.contains(&z.id)).flat_map(|z| z.outline.iter().flatten().copied()));
    pts.extend(board.shapes.iter().filter(|s| ids.contains(&s.id)).flat_map(|s| poly::geom_points(&s.shape.geom).0));
    pts.extend(board.texts.iter().filter(|t| ids.contains(&t.id)).map(|t| t.text.at));
    pts.extend(board.footprints.iter().filter(|f| ids.contains(&f.id)).map(|f| f.placement.at));
    let b = pts.into_iter().fold(None, |acc, p| Some(Bounds::union(acc, Bounds::of(p))));
    crate::sch_edit::snap(b.map_or(Pt::ZERO, |b| b.center()), grid.max(1))
}

/// Turns items `deg` counter-clockwise about `center` (R): zones (their fill poured again by
/// the app), drawn shapes, text, tracks, vias and footprints.
pub fn rotate_items(board: &mut Board, items: &[BoardItem], center: Pt, deg: f64) {
    let ids: Vec<Uuid> = items.iter().map(|i| i.id()).collect();
    let r = |p: Pt| center + (p - center).rotated(deg);
    for t in board.tracks.iter_mut().filter(|t| ids.contains(&t.id)) {
        (t.a, t.b) = (r(t.a), r(t.b));
    }
    for v in board.vias.iter_mut().filter(|v| ids.contains(&v.id)) {
        v.at = r(v.at);
    }
    for z in board.zones.iter_mut().filter(|z| ids.contains(&z.id)) {
        z.outline.iter_mut().flatten().for_each(|p| *p = r(*p));
        z.filled.clear();
    }
    for s in board.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
        let geom = if deg % 90.0 == 0.0 { s.shape.geom.clone() } else { s.shape.geom.rect_as_polyline() };
        s.shape.geom = geom.map(r);
    }
    for t in board.texts.iter_mut().filter(|t| ids.contains(&t.id)) {
        t.text.at = r(t.text.at);
        t.text.angle = normalize_deg(t.text.angle + deg);
    }
    for f in board.footprints.iter_mut().filter(|f| ids.contains(&f.id)) {
        f.placement.at = r(f.placement.at);
        f.placement.angle = normalize_deg(f.placement.angle + deg);
    }
}

/// Flips items to the other side of the board (F), left for right about `center`: zones,
/// drawn shapes, text and tracks change to the matching layer of the other side (text reads
/// mirrored there), vias stay through; footprints flip as [`flip_footprint`] does.
pub fn flip_items(board: &mut Board, items: &[BoardItem], center: Pt) {
    let ids: Vec<Uuid> = items.iter().map(|i| i.id()).collect();
    let m = |p: Pt| Pt::new(2 * center.x - p.x, p.y);
    for t in board.tracks.iter_mut().filter(|t| ids.contains(&t.id)) {
        (t.a, t.b) = (m(t.a), m(t.b));
        t.layer = t.layer.flipped();
    }
    for v in board.vias.iter_mut().filter(|v| ids.contains(&v.id)) {
        v.at = m(v.at);
    }
    for z in board.zones.iter_mut().filter(|z| ids.contains(&z.id)) {
        z.outline.iter_mut().flatten().for_each(|p| *p = m(*p));
        z.layers = z.layers.flipped();
        z.filled.clear();
    }
    for s in board.shapes.iter_mut().filter(|s| ids.contains(&s.id)) {
        s.shape.geom = s.shape.geom.map(m);
        s.layer = s.layer.flipped();
    }
    for t in board.texts.iter_mut().filter(|t| ids.contains(&t.id)) {
        t.text.at = m(t.text.at);
        t.text.angle = normalize_deg(-t.text.angle);
        t.text.style.mirrored = !t.text.style.mirrored;
        t.layer = t.layer.flipped();
    }
    let fps: Vec<usize> = (0..board.footprints.len()).filter(|&i| ids.contains(&board.footprints[i].id)).collect();
    for i in fps {
        let at = m(board.footprints[i].placement.at);
        board.footprints[i].placement.at = at;
        flip_footprint(board, i);
    }
}

/// The corners of a zone (each ring's points) or of a drawn polygon or rectangle, as (ring,
/// index, point): what can be dragged to reshape it.
pub fn corners(board: &Board, item: BoardItem) -> Vec<(usize, usize, Pt)> {
    use crate::graphics::Geom;
    match item {
        BoardItem::Zone(id) => board.zones.iter().find(|z| z.id == id).map_or(vec![], |z| z.outline.iter().enumerate().flat_map(|(r, ring)| ring.iter().enumerate().map(move |(i, p)| (r, i, *p))).collect()),
        BoardItem::Shape(id) => match board.shapes.iter().find(|s| s.id == id).map(|s| &s.shape.geom) {
            Some(Geom::Polyline { pts, .. }) => pts.iter().enumerate().map(|(i, p)| (0, i, *p)).collect(),
            Some(Geom::Rect { a, b }) => [*a, Pt::new(b.x, a.y), *b, Pt::new(a.x, b.y)].into_iter().enumerate().map(|(i, p)| (0, i, p)).collect(),
            Some(Geom::Line { a, b }) => vec![(0, 0, *a), (0, 1, *b)],
            _ => vec![],
        },
        _ => vec![],
    }
}

/// The corner of `item` within `tol` of `p` (the nearest), as (ring, index).
pub fn corner_at(board: &Board, item: BoardItem, p: Pt, tol: Nm) -> Option<(usize, usize)> {
    corners(board, item).into_iter().filter(|c| c.2.dist(p) <= tol as f64).min_by(|a, b| a.2.dist(p).total_cmp(&b.2.dist(p))).map(|c| (c.0, c.1))
}

/// Moves one corner of a zone or drawn shape to `to` (a rectangle becomes a polygon). A
/// zone's fill is taken off (the app pours zones again after every board change).
pub fn move_corner(board: &mut Board, item: BoardItem, ring: usize, index: usize, to: Pt) {
    use crate::graphics::Geom;
    match item {
        BoardItem::Zone(id) => {
            if let Some(z) = board.zones.iter_mut().find(|z| z.id == id)
                && let Some(p) = z.outline.get_mut(ring).and_then(|r| r.get_mut(index))
            {
                *p = to;
                z.filled.clear();
            }
        }
        BoardItem::Shape(id) => {
            let Some(s) = board.shapes.iter_mut().find(|s| s.id == id) else { return };
            match &mut s.shape.geom {
                Geom::Polyline { pts, .. } => {
                    if let Some(p) = pts.get_mut(index) {
                        *p = to;
                    }
                }
                Geom::Rect { a, b } => {
                    let mut pts = vec![*a, Pt::new(b.x, a.y), *b, Pt::new(a.x, b.y)];
                    if let Some(p) = pts.get_mut(index) {
                        *p = to;
                    }
                    s.shape.geom = Geom::Polyline { pts, closed: true };
                }
                Geom::Line { a, b } => {
                    if index == 0 {
                        *a = to;
                    } else {
                        *b = to;
                    }
                }
                _ => {}
            }
        }
        _ => {}
    }
}

/// Draws a filled polygon on `layer` (a copper area on a copper layer, on `net`); returns its
/// id.
pub fn add_polygon(board: &mut Board, pts: Vec<Pt>, layer: Layer, net: &str) -> Uuid {
    let id = add_shape(board, crate::graphics::Geom::Polyline { pts, closed: true }, layer);
    if let Some(s) = board.shapes.iter_mut().find(|s| s.id == id) {
        s.shape.fill = crate::graphics::Fill::Outline;
        s.net = net.to_string();
    }
    id
}

/// The items a box selects, by their extents: left to right (`crossing` false) those wholly
/// inside `r`, right to left those it touches. Footprints by their pads and drawings, tracks
/// and vias by their copper, zones by their outline, drawn shapes and text by what they cover.
pub fn box_select(board: &Board, r: crate::units::Bounds, crossing: bool) -> Vec<BoardItem> {
    use crate::units::Bounds;
    let take = |b: Option<Bounds>| {
        b.is_some_and(|b| if crossing { b.min.x <= r.max.x && b.max.x >= r.min.x && b.min.y <= r.max.y && b.max.y >= r.min.y } else { r.contains(b.min) && r.contains(b.max) })
    };
    let of = |pts: Vec<Pt>| pts.into_iter().fold(None, |acc, p| Some(Bounds::union(acc, Bounds::of(p))));
    let mut out = vec![];
    for f in &board.footprints {
        let mut b: Option<Bounds> = None;
        for p in &f.footprint.pads {
            b = Some(Bounds::union(b, Bounds::of(f.placement.apply(p.at)).grow(p.size.w.max(p.size.h) / 2)));
        }
        for s in f.footprint.shapes.iter().filter(|s| matches!(s.layer, Layer::TopCourtyard | Layer::TopFab | Layer::TopSilk)) {
            for q in s.shape.geom.extent() {
                b = Some(Bounds::union(b, Bounds::of(f.placement.apply(q))));
            }
        }
        if take(b) {
            out.push(BoardItem::Footprint(f.id));
        }
    }
    for t in &board.tracks {
        if take(of(vec![t.a, t.b]).map(|b| b.grow(t.width / 2))) {
            out.push(BoardItem::Track(t.id));
        }
    }
    for v in &board.vias {
        if take(Some(Bounds::of(v.at).grow(v.diameter / 2))) {
            out.push(BoardItem::Via(v.id));
        }
    }
    for z in &board.zones {
        if take(of(z.outline.iter().flatten().copied().collect())) {
            out.push(BoardItem::Zone(z.id));
        }
    }
    for s in &board.shapes {
        if take(of(poly::geom_points(&s.shape.geom).0)) {
            out.push(BoardItem::Shape(s.id));
        }
    }
    for t in &board.texts {
        if take(crate::font::bounds(&t.text)) {
            out.push(BoardItem::Text(t.id));
        }
    }
    out
}

/// Deletes items (Del).
pub fn delete_items(board: &mut Board, items: &[BoardItem]) {
    let ids: Vec<Uuid> = items.iter().map(|i| i.id()).collect();
    board.tracks.retain(|t| !ids.contains(&t.id));
    board.vias.retain(|v| !ids.contains(&v.id));
    board.footprints.retain(|f| !ids.contains(&f.id));
    board.zones.retain(|z| !ids.contains(&z.id));
    board.shapes.retain(|s| !ids.contains(&s.id));
    board.texts.retain(|t| !ids.contains(&t.id));
}

/// The footprint with this id.
pub fn footprint_by_id(board: &Board, id: Uuid) -> Option<usize> {
    board.footprints.iter().position(|f| f.id == id)
}

#[cfg(test)]
mod hit_tests {
    use super::*;
    use crate::library::LibraryTable;

    #[test]
    fn hits_on_the_course_board() {
        let lib = LibraryTable::builtin();
        let d = crate::getting_started::gs18(&lib);
        let b = &d.board;
        let r1 = footprint_index(b, "R1").unwrap();
        let pad2 = pad_center(b, r1, 1);
        assert_eq!(hit(b, pad2, 1000), Some(BoardItem::Footprint(b.footprints[r1].id)));
        let v = &b.vias[0];
        assert_eq!(hit(b, v.at, 1000), Some(BoardItem::Via(v.id)));
        // The middle of the led track, between D1 and R1.
        let t = b.tracks.iter().find(|t| t.net == "led").unwrap();
        let mid = Pt::new((t.a.x + t.b.x) / 2, (t.a.y + t.b.y) / 2);
        assert_eq!(hit(b, mid, 1000), Some(BoardItem::Track(t.id)));
        // Empty board area: the zone.
        assert!(matches!(hit(b, Pt::mm(2.0, 2.0), 1000), Some(BoardItem::Zone(_))));
        let mut b2 = b.clone();
        delete_items(&mut b2, &[BoardItem::Track(t.id)]);
        assert_eq!(b2.tracks.len(), b.tracks.len() - 1);
    }
}

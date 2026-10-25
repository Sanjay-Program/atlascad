//! Fitting a board into the parts around it (`docs/PCB_IN_PART_STUDIO.md` §7.3), on display
//! meshes (components are mesh parts, Q1), in the **board frame** of a board copy (layout x and
//! y in mm on its bottom face, z up through it).
//!
//! - [`section`]: what of some parts lies between two heights above (or below) the board,
//!   projected onto it — the parts in the way of components standing that tall, as a region
//!   of the layout (nm), so the layout can show it, components can snap to it and gaps can be
//!   measured to it.
//! - [`footprint_of`]: a part's own shadow on the board (its projection).
//! - [`collisions`]: components whose shadow overlaps another part's section over the
//!   component's own heights (they run into it), with how far.
//! - [`gaps`]: how far a component can move along ±x and ±y of the board before it meets
//!   something.
//! - [`axes`]: round features standing square to the board (pins, holes): where their axes meet
//!   it, to snap a pad onto.
//! - [`align`]: the move in the board's plane that puts an entity of a component onto a target
//!   (a pin's axis onto a hole's, a face onto a face).

use std::sync::Arc;

use cadrs_eda::poly::{self, Region};
use cadrs_eda::units::{Pt, mm};
use cadrs_sketch::PlaneFrame;

use crate::solid::Solid;

type V3 = [f64; 3];

pub use super::frames::{dir_to_board, to_board};

/// The part of the triangle `t` (board frame) between heights `z0` and `z1`, if any.
fn clip_slab(t: [V3; 3], z0: f64, z1: f64) -> Vec<V3> {
    let mut poly: Vec<V3> = t.to_vec();
    for (limit, below) in [(z0, false), (z1, true)] {
        let inside = |p: &V3| if below { p[2] <= limit } else { p[2] >= limit };
        let mut out = Vec::with_capacity(poly.len() + 2);
        for i in 0..poly.len() {
            let (a, b) = (poly[i], poly[(i + 1) % poly.len()]);
            let (ia, ib) = (inside(&a), inside(&b));
            if ia {
                out.push(a);
            }
            if ia != ib {
                let k = (limit - a[2]) / (b[2] - a[2]);
                out.push([a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, limit]);
            }
        }
        poly = out;
        if poly.len() < 3 {
            return vec![];
        }
    }
    poly
}

fn ring_of(pts: &[V3]) -> Option<Vec<Pt>> {
    let mut ring: Vec<Pt> = pts.iter().map(|p| Pt::mm(p[0], p[1])).collect();
    ring.dedup();
    if ring.len() < 3 {
        return None;
    }
    let a = poly::ring_area(&ring);
    // Edge-on facets (walls seen along) cast no shadow.
    if a.abs() < 1e6 {
        return None;
    }
    if a < 0.0 {
        ring.reverse();
    }
    Some(ring)
}

/// The box of a solid in a board frame.
pub fn bounds_in(frame: &PlaneFrame, s: &Solid) -> Option<(V3, V3)> {
    let mut it = s.positions.iter().map(|p| to_board(frame, *p));
    let first = it.next()?;
    Some(it.fold((first, first), |(lo, hi), p| ([lo[0].min(p[0]), lo[1].min(p[1]), lo[2].min(p[2])], [hi[0].max(p[0]), hi[1].max(p[1]), hi[2].max(p[2])])))
}

/// A closed solid's slice at height `z` of the board frame (its triangles in board coordinates,
/// `p`): the segments where it crosses the plane chained into rings, filled even-odd (layout
/// nm).
fn slice(p: &[V3], indices: &[u32], z: f64) -> Region {
    let key = |q: &V3| ((q[0] * 1e5).round() as i64, (q[1] * 1e5).round() as i64);
    let mut segs: Vec<((i64, i64), (i64, i64))> = Vec::new();
    for t in indices.as_chunks::<3>().0 {
        let v = [p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]];
        let mut cut: Vec<V3> = Vec::with_capacity(2);
        for i in 0..3 {
            let (a, b) = (v[i], v[(i + 1) % 3]);
            if (a[2] < z) != (b[2] < z) {
                let k = (z - a[2]) / (b[2] - a[2]);
                cut.push([a[0] + (b[0] - a[0]) * k, a[1] + (b[1] - a[1]) * k, z]);
            }
        }
        if let [a, b] = cut[..] {
            let (ka, kb) = (key(&a), key(&b));
            if ka != kb {
                segs.push((ka, kb));
            }
        }
    }
    // Chain them: each point to its segments.
    let mut at: std::collections::HashMap<(i64, i64), Vec<usize>> = Default::default();
    for (i, (a, b)) in segs.iter().enumerate() {
        at.entry(*a).or_default().push(i);
        at.entry(*b).or_default().push(i);
    }
    let mut used = vec![false; segs.len()];
    let mut rings: Region = Vec::new();
    for s0 in 0..segs.len() {
        if used[s0] {
            continue;
        }
        used[s0] = true;
        let start = segs[s0].0;
        let mut ring = vec![start];
        let mut cur = segs[s0].1;
        while cur != start {
            ring.push(cur);
            let Some(&next) = at.get(&cur).and_then(|v| v.iter().find(|i| !used[**i])) else { break };
            used[next] = true;
            cur = if segs[next].0 == cur { segs[next].1 } else { segs[next].0 };
        }
        if ring.len() >= 3 {
            // 1e5 per mm → nm: × 10.
            rings.push(ring.into_iter().map(|(x, y)| Pt::new(x * 10, y * 10)).collect());
        }
    }
    poly::union_even_odd(&rings)
}

/// What of `solids` lies between heights `z0` and `z1` of the board frame `frame`, projected onto
/// the board (layout nm): their faces' parts between the heights, and their slices at both
/// heights (so a wall standing through them counts). Only the parts within `near` (a layout
/// box, mm: x0, y0, x1, y1) are looked at, when given.
pub fn section(solids: &[&Solid], frame: &PlaneFrame, z0: f64, z1: f64, near: Option<[f64; 4]>) -> Region {
    let mut rings: Region = Vec::new();
    for s in solids {
        let Some((lo, hi)) = bounds_in(frame, s) else { continue };
        if hi[2] < z0 || lo[2] > z1 {
            continue;
        }
        if let Some([x0, y0, x1, y1]) = near
            && (hi[0] < x0 || lo[0] > x1 || hi[1] < y0 || lo[1] > y1)
        {
            continue;
        }
        let p: Vec<V3> = s.positions.iter().map(|q| to_board(frame, *q)).collect();
        for t in s.indices.as_chunks::<3>().0 {
            let tri = [p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]];
            // Each triangle against the heights and the box first (a tube around a component
            // has its box around it too, but few of its triangles near it).
            let (tlo, thi) = tri.iter().fold(([f64::MAX; 3], [f64::MIN; 3]), |(lo, hi), q| (std::array::from_fn(|i| lo[i].min(q[i])), std::array::from_fn(|i| hi[i].max(q[i]))));
            if thi[2] < z0 || tlo[2] > z1 {
                continue;
            }
            if let Some([x0, y0, x1, y1]) = near
                && (thi[0] < x0 || tlo[0] > x1 || thi[1] < y0 || tlo[1] > y1)
            {
                continue;
            }
            if let Some(r) = ring_of(&clip_slab(tri, z0, z1)) {
                rings.push(r);
            }
        }
        for z in [z0, z1] {
            if z > lo[2] && z < hi[2] {
                rings.extend(slice(&p, &s.indices, z));
            }
        }
    }
    poly::union(&rings)
}

/// A solid's shadow on the board of `frame` (its whole projection).
pub fn footprint_of(s: &Solid, frame: &PlaneFrame) -> Region {
    section(&[s], frame, -1e9, 1e9, None)
}

/// A component running into another part (the caller's keys for them).
#[derive(Clone, Debug, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct Collision<K> {
    pub component: K,
    pub other: K,
    /// How far they overlap seen square to the board (mm): the narrower side of the overlap's
    /// box, at the height band where it is largest.
    pub depth: f64,
    /// Over how much height they overlap (mm).
    pub height: f64,
}

/// A collision as the feature's warning says it: "J1 runs into Tube (0.80 mm in plan, 2.40 mm
/// high)".
pub fn collision_text<K>(component: &str, other: &str, c: &Collision<K>) -> String {
    format!("{component} runs into {other} ({:.2} mm in plan, {:.2} mm high)", c.depth, c.height)
}

/// The heights (board frame) of a solid's faces lying flat to the board, within `z0`…`z1`.
fn flat_heights(s: &Solid, frame: &PlaneFrame, z0: f64, z1: f64) -> Vec<f64> {
    s.faces.iter().filter_map(|f| face_heights(s, f, frame)).filter(|(lo, hi)| hi - lo < 1e-4 && *lo > z0 && *lo < z1).map(|(lo, _)| lo).collect()
}

/// Whether two solids overlap in the board frame `frame`, as (depth, height) (see
/// [`Collision`]). They are compared in height bands, split where either has a face flat to the
/// board (a connector's low body and its tall pin, a lid's ledge): a part over a component's
/// low step but clear of its tall part doesn't collide. Parts that only touch (at a face, or by
/// less than 0.01 mm) don't count.
fn overlap(c: &Solid, o: &Solid, frame: &PlaneFrame) -> Option<(f64, f64)> {
    let (lo, hi) = bounds_in(frame, c)?;
    let (olo, ohi) = bounds_in(frame, o)?;
    let eps = 1e-3;
    if ohi[0] < lo[0] + eps || olo[0] > hi[0] - eps || ohi[1] < lo[1] + eps || olo[1] > hi[1] - eps || ohi[2] < lo[2] + eps || olo[2] > hi[2] - eps {
        return None;
    }
    let mut cuts = vec![lo[2], hi[2]];
    cuts.extend(flat_heights(c, frame, lo[2], hi[2]));
    cuts.extend(flat_heights(o, frame, lo[2], hi[2]));
    cuts.extend([olo[2], ohi[2]].into_iter().filter(|z| *z > lo[2] && *z < hi[2]));
    cuts.sort_by(f64::total_cmp);
    cuts.dedup_by(|a, b| (*a - *b).abs() < 0.05);
    // Many steps (a finned heat sink): every other cut, until it is a handful.
    while cuts.len() > 17 {
        let last = *cuts.last().unwrap_or(&hi[2]);
        cuts = cuts.into_iter().step_by(2).collect();
        if cuts.last() != Some(&last) {
            cuts.push(last);
        }
    }
    let near = [lo[0], lo[1], hi[0], hi[1]];
    let (mut depth, mut height) = (0.0_f64, 0.0);
    for w in cuts.windows(2) {
        let (a, b) = (w[0] + eps, w[1] - eps);
        if b <= a {
            continue;
        }
        let sec = section(&[o], frame, a, b, Some(near));
        if sec.is_empty() {
            continue;
        }
        let mine = section(&[c], frame, a, b, None);
        let both = poly::intersection(&mine, &sec);
        let Some(bb) = both.iter().flatten().fold(None, |acc, p| Some(cadrs_eda::units::Bounds::union(acc, cadrs_eda::units::Bounds::of(*p)))) else { continue };
        let d = cadrs_eda::units::to_mm(bb.size().w.min(bb.size().h));
        if d > 0.01 {
            depth = depth.max(d);
            height += w[1] - w[0];
        }
    }
    (depth > 0.0).then_some((depth, height))
}

/// Components whose solids run into another part, in the board frame `frame` (see
/// [`overlap`]). The answers are kept per pair of solids (and frame) while both live, so a
/// rebuild where only one footprint moved works out only that one's again.
pub fn collisions<K: Clone>(components: &[(K, &Arc<Solid>)], others: &[(K, &Arc<Solid>)], frame: &PlaneFrame) -> Vec<Collision<K>> {
    use std::collections::HashMap;
    use std::hash::{Hash, Hasher};
    use std::sync::{Mutex, OnceLock, Weak};
    /// (The pair's solids, to tell a freed one's address reused, and their overlap.)
    type Entry = (Weak<Solid>, Weak<Solid>, Option<(f64, f64)>);
    type Cache = Mutex<HashMap<(usize, usize, u64), Entry>>;
    static CACHE: OnceLock<Cache> = OnceLock::new();
    let cache = CACHE.get_or_init(Default::default);
    let frame_key = {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        [frame.origin, frame.u, frame.v].map(|a| a.map(f64::to_bits)).hash(&mut h);
        h.finish()
    };
    let mut out = vec![];
    for (name, c) in components {
        for (other, o) in others {
            let key = (Arc::as_ptr(c) as usize, Arc::as_ptr(o) as usize, frame_key);
            let known = cache.lock().ok().and_then(|m| {
                m.get(&key).filter(|(a, b, _)| a.upgrade().is_some_and(|a| Arc::ptr_eq(&a, c)) && b.upgrade().is_some_and(|b| Arc::ptr_eq(&b, o))).map(|e| e.2)
            });
            let hit = known.unwrap_or_else(|| {
                let hit = overlap(c, o, frame);
                if let Ok(mut m) = cache.lock() {
                    if m.len() > 20_000 {
                        m.retain(|_, (a, b, _)| a.strong_count() > 0 && b.strong_count() > 0);
                    }
                    m.insert(key, (Arc::downgrade(c), Arc::downgrade(o), hit));
                }
                hit
            });
            if let Some((depth, height)) = hit {
                out.push(Collision { component: name.clone(), other: other.clone(), depth, height });
            }
        }
    }
    out
}

/// How far the region `of` (a component's shadow, nm) can move along +x, −x, +y and −y before it
/// meets `around` (the parts around, nm), up to `limit` mm; `None` past that. Exact for the
/// polygons: every corner of each cast along the move onto the other's edges.
pub fn gaps(of: &Region, around: &Region, limit: f64) -> [Option<f64>; 4] {
    if of.is_empty() {
        return [None; 4];
    }
    // Already touching or overlapping: no room either way.
    if poly::overlaps(of, around) {
        return [Some(0.0); 4];
    }
    let edges = |r: &Region| -> Vec<(Pt, Pt)> { r.iter().flat_map(|ring| (0..ring.len()).map(move |i| (ring[i], ring[(i + 1) % ring.len()]))).collect() };
    let (mine, theirs) = (edges(of), edges(around));
    // How far from `o` along `d` the nearest of `segments` is (nm).
    let cast = |o: Pt, d: (f64, f64), segments: &[(Pt, Pt)]| -> Option<f64> {
        let mut best: Option<f64> = None;
        for (a, b) in segments {
            let (ax, ay, bx, by) = (a.x as f64, a.y as f64, b.x as f64, b.y as f64);
            let (ex, ey) = (bx - ax, by - ay);
            let den = d.0 * ey - d.1 * ex;
            if den.abs() < 1e-12 {
                continue;
            }
            let (ox, oy) = (ax - o.x as f64, ay - o.y as f64);
            let t = (ox * ey - oy * ex) / den;
            let u = (ox * d.1 - oy * d.0) / den;
            if t >= 0.0 && (0.0..=1.0).contains(&u) {
                best = Some(best.map_or(t, |x: f64| x.min(t)));
            }
        }
        best
    };
    let mut out = [None; 4];
    for (i, d) in [(1.0, 0.0), (-1.0, 0.0), (0.0, 1.0), (0.0, -1.0)].into_iter().enumerate() {
        let back = (-d.0, -d.1);
        let forward = of.iter().flatten().filter_map(|p| cast(*p, d, &theirs));
        let backward = around.iter().flatten().filter_map(|p| cast(*p, back, &mine));
        out[i] = forward.chain(backward).fold(None, |acc: Option<f64>, t| Some(acc.map_or(t, |a| a.min(t)))).map(|t| t / 1e6).filter(|t| *t <= limit);
    }
    out
}

/// The room over a component (mm, along the board's normal from its top `from` up to the
/// first part of `solids` over its shadow `of`; `up` false: down from its bottom, under the
/// board), up to `limit`; `None` past that. Looked for from the shadow's corners and a grid
/// across it.
pub fn headroom(of: &Region, solids: &[&Solid], frame: &PlaneFrame, from: f64, up: bool, limit: f64) -> Option<f64> {
    let k = if up { 1.0 } else { -1.0 };
    let bb = of.iter().flatten().fold(None, |acc, p| Some(cadrs_eda::units::Bounds::union(acc, cadrs_eda::units::Bounds::of(*p))))?;
    let mut points: Vec<[f64; 2]> = of.iter().flatten().map(|p| [p.x as f64 / 1e6, p.y as f64 / 1e6]).collect();
    let n = 6;
    for i in 0..=n {
        for j in 0..=n {
            let x = bb.min.x + (bb.max.x - bb.min.x) * i / n;
            let y = bb.min.y + (bb.max.y - bb.min.y) * j / n;
            if poly::contains(of, Pt::new(x, y)) {
                points.push([x as f64 / 1e6, y as f64 / 1e6]);
            }
        }
    }
    let mut best: Option<f64> = None;
    for s in solids {
        let Some((lo, hi)) = bounds_in(frame, s) else { continue };
        if (up && hi[2] <= from) || (!up && lo[2] >= from) || lo[0] > bb.max.x as f64 / 1e6 || hi[0] < bb.min.x as f64 / 1e6 || lo[1] > bb.max.y as f64 / 1e6 || hi[1] < bb.min.y as f64 / 1e6 {
            continue;
        }
        let p: Vec<V3> = s.positions.iter().map(|q| to_board(frame, *q)).collect();
        let (x0, y0, x1, y1) = (bb.min.x as f64 / 1e6, bb.min.y as f64 / 1e6, bb.max.x as f64 / 1e6, bb.max.y as f64 / 1e6);
        for t in s.indices.as_chunks::<3>().0 {
            let [a, b, c] = [p[t[0] as usize], p[t[1] as usize], p[t[2] as usize]];
            // Only triangles over the shadow (and on the right side of `from`).
            let (tx0, tx1) = (a[0].min(b[0]).min(c[0]), a[0].max(b[0]).max(c[0]));
            let (ty0, ty1) = (a[1].min(b[1]).min(c[1]), a[1].max(b[1]).max(c[1]));
            if tx1 < x0 || tx0 > x1 || ty1 < y0 || ty0 > y1 {
                continue;
            }
            for q in &points {
                // Where the vertical line through q meets the triangle (barycentric in plan).
                let den = (b[1] - c[1]) * (a[0] - c[0]) + (c[0] - b[0]) * (a[1] - c[1]);
                if den.abs() < 1e-12 {
                    continue;
                }
                let w0 = ((b[1] - c[1]) * (q[0] - c[0]) + (c[0] - b[0]) * (q[1] - c[1])) / den;
                let w1 = ((c[1] - a[1]) * (q[0] - c[0]) + (a[0] - c[0]) * (q[1] - c[1])) / den;
                let w2 = 1.0 - w0 - w1;
                if w0 < -1e-9 || w1 < -1e-9 || w2 < -1e-9 {
                    continue;
                }
                let d = k * (w0 * a[2] + w1 * b[2] + w2 * c[2] - from);
                if d >= -1e-6 {
                    best = Some(best.map_or(d, |x: f64| x.min(d)));
                }
            }
        }
    }
    best.map(|d| d.max(0.0)).filter(|d| *d <= limit)
}

/// Round features of `solids` standing square to the board of `frame` (a cylinder whose axis
/// is along the board's normal: a pin, a hole) that reach heights `z0`…`z1`: where each axis
/// meets the board (layout, mm) and its radius.
pub fn axes(solids: &[&Solid], frame: &PlaneFrame, z0: f64, z1: f64) -> Vec<([f64; 2], f64)> {
    let mut out: Vec<([f64; 2], f64)> = vec![];
    for s in solids {
        for f in &s.faces {
            let (Some((p, d)), Some(r)) = (f.axis, f.radius) else { continue };
            let d = dir_to_board(frame, d);
            if d[2].abs() < 0.999 {
                continue;
            }
            // The face's own heights.
            let Some(face_z) = face_heights(s, f, frame) else { continue };
            if face_z.1 < z0 || face_z.0 > z1 {
                continue;
            }
            let q = to_board(frame, p);
            if !out.iter().any(|(c, rr)| (c[0] - q[0]).hypot(c[1] - q[1]) < 1e-4 && (rr - r).abs() < 1e-4) {
                out.push(([q[0], q[1]], r));
            }
        }
    }
    out
}

fn face_heights(s: &Solid, f: &crate::solid::SolidFace, frame: &PlaneFrame) -> Option<(f64, f64)> {
    let mut lo = f64::MAX;
    let mut hi = f64::MIN;
    for t in f.first_triangle..f.first_triangle + f.triangle_count {
        for k in 0..3 {
            let i = *s.indices.get(3 * t + k)? as usize;
            let z = to_board(frame, s.positions[i])[2];
            lo = lo.min(z);
            hi = hi.max(z);
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// An entity to align: a point, an axis (a point on it and its direction) or a plane (a point
/// on it and its normal), in Part Studio coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Entity {
    Point(V3),
    Axis(V3, V3),
    Plane(V3, V3),
}

/// A mate connector (its frame) as an Align target for the board on `board`: its Z axis when
/// that stands square to the board (a connector on a pin's end, say), else the plane its Z is
/// normal to (a connector on a wall's face).
pub fn connector_entity(connector: &PlaneFrame, board: &PlaneFrame) -> Entity {
    let z = connector.normal();
    if dot(z, board.normal()).abs() > 0.999 { Entity::Axis(connector.origin, z) } else { Entity::Plane(connector.origin, z) }
}

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// How a footprint moves to align (see [`align`]).
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Alignment {
    /// The turn about the footprint's origin (degrees, counter-clockwise on the layout), first.
    pub turn: f64,
    /// Then the move (layout mm).
    pub shift: [f64; 2],
    /// How far apart they still are out of the board's plane (mm, along its normal), when
    /// that counts (two points; a flat end onto a point).
    pub off_plane: Option<f64>,
}

/// How to move a footprint in the board's plane (frame `frame`; its origin at `pivot`, board
/// frame mm) so the entity `from` of its part meets `to`:
///
/// - two axes square to the board (a pin into a hole), points, or an axis and a point: moved so
///   they line up, seen square to the board;
/// - two faces standing square to the board: turned (by at most a quarter turn) until they are
///   parallel, then moved along their normal until they are in one plane (facing each other or
///   flush);
/// - a face lying flat to the board (a pin's or a wire's end) counts as its middle.
///
/// `None` when no move in the board's plane can do it (an axis lying in the board, a flat face
/// onto an upright one).
pub fn align(from: Entity, to: Entity, frame: &PlaneFrame, pivot: [f64; 2]) -> Option<Alignment> {
    let place = |e: Entity| -> Option<V3> {
        match e {
            Entity::Point(p) => Some(to_board(frame, p)),
            Entity::Axis(p, d) => {
                let d = dir_to_board(frame, d);
                (d[2].abs() > 0.999).then(|| to_board(frame, p))
            }
            Entity::Plane(..) => None,
        }
    };
    // A face lying flat (parallel to the board: a pin's or a wire's end) is its middle.
    let flat = |e: Entity| match e {
        Entity::Plane(p, n) if dir_to_board(frame, n)[2].abs() > 0.999 => Entity::Point(p),
        e => e,
    };
    let (from, to) = (flat(from), flat(to));
    match (from, to) {
        (Entity::Plane(p, n), Entity::Plane(q, m)) => {
            let (n, m) = (dir_to_board(frame, n), dir_to_board(frame, m));
            // Both normals in the board's plane.
            if n[2].abs() > 1e-3 || m[2].abs() > 1e-3 {
                return None;
            }
            // The smallest turn making them parallel (either way round).
            let mut turn = m[1].atan2(m[0]) - n[1].atan2(n[0]);
            let half = std::f64::consts::PI;
            while turn > half / 2.0 {
                turn -= half;
            }
            while turn < -half / 2.0 {
                turn += half;
            }
            if turn.abs() < 1e-9 {
                turn = 0.0;
            }
            let (c, s) = (turn.cos(), turn.sin());
            let p = to_board(frame, p);
            let r = [p[0] - pivot[0], p[1] - pivot[1]];
            let p2 = [pivot[0] + c * r[0] - s * r[1], pivot[1] + s * r[0] + c * r[1]];
            let len = m[0].hypot(m[1]);
            let (mx, my) = (m[0] / len, m[1] / len);
            let q = to_board(frame, q);
            let along = (q[0] - p2[0]) * mx + (q[1] - p2[1]) * my;
            Some(Alignment { turn: turn.to_degrees(), shift: [mx * along, my * along], off_plane: None })
        }
        (Entity::Plane(..), _) | (_, Entity::Plane(..)) => None,
        (a, b) => {
            let (p, q) = (place(a)?, place(b)?);
            let off_plane = matches!((a, b), (Entity::Point(_), Entity::Point(_))).then(|| q[2] - p[2]);
            Some(Alignment { turn: 0.0, shift: [q[0] - p[0], q[1] - p[1]], off_plane })
        }
    }
}

/// [`mm`] for whole layout moves (`dx`, `dy` mm).
pub fn delta(d: [f64; 2]) -> Pt {
    Pt::new(mm(d[0]), mm(d[1]))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cube(min: [f64; 3], max: [f64; 3]) -> Solid {
        let mut b = cadrs_eda::model3d::Body::default();
        b.cuboid(min, max, [1, 1, 1]);
        crate::pcb::mesh_solid::mesh_solid(&cadrs_eda::model3d::mesh(&b), uuid::Uuid::nil(), 1.0)
    }

    const TOP: PlaneFrame = PlaneFrame { origin: [0.0, 0.0, 10.0], u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0] };

    #[test]
    fn a_mate_connector_is_an_axis_or_a_plane_to_align_to() {
        // On a pin's end (its Z up, square to the board): a pin's axis goes onto its Z axis.
        let on_pin = PlaneFrame { origin: [3.0, 4.0, 13.6], u: [1.0, 0.0, 0.0], v: [0.0, 1.0, 0.0] };
        let to = connector_entity(&on_pin, &TOP);
        assert!(matches!(to, Entity::Axis(..)));
        let a = align(Entity::Axis([1.0, 1.0, 11.0], [0.0, 0.0, 1.0]), to, &TOP, [0.0, 0.0]).unwrap();
        assert_eq!((a.turn, a.shift), (0.0, [2.0, 3.0]));
        // On a wall's face (its Z along x): a component's face square to the board goes onto it.
        let on_wall = PlaneFrame { origin: [20.0, 0.0, 12.0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0] };
        let to = connector_entity(&on_wall, &TOP);
        assert!(matches!(to, Entity::Plane(..)));
        let a = align(Entity::Plane([5.0, 0.0, 11.0], [1.0, 0.0, 0.0]), to, &TOP, [0.0, 0.0]).unwrap();
        assert!((a.shift[0] - 15.0).abs() < 1e-9 && a.shift[1].abs() < 1e-9, "{a:?}");
    }

    #[test]
    fn a_section_is_what_lies_between_the_heights() {
        // A 10 × 10 block from 12 to 20 above Top, the board's bottom at z 10: between board
        // heights 0 and 5 (z 10–15) it is there, 10 × 10; between 15 and 20 (z 25–30) it isn't.
        let block = cube([0.0, 0.0, 12.0], [10.0, 10.0, 20.0]);
        let r = section(&[&block], &TOP, 0.0, 5.0, None);
        assert!((poly::area(&r) / 1e12 - 100.0).abs() < 1e-6, "{}", poly::area(&r) / 1e12);
        assert!(section(&[&block], &TOP, 15.0, 20.0, None).is_empty());
    }

    #[test]
    fn a_wall_standing_through_the_heights_is_in_them() {
        // A 2 × 10 wall from z 0 to 30: between board heights 1 and 3 (z 11–13) it is its
        // cross-section, though none of its faces lie flat there.
        let wall = cube([4.0, 0.0, 0.0], [6.0, 10.0, 30.0]);
        let r = section(&[&wall], &TOP, 1.0, 3.0, None);
        assert!((poly::area(&r) / 1e12 - 20.0).abs() < 1e-6, "{}", poly::area(&r) / 1e12);
    }

    #[test]
    fn gaps_are_the_free_distances_each_way() {
        // A 2 × 2 component at (0..2, 0..2); a wall from x 7 to 8 across; nothing elsewhere.
        let comp = vec![vec![Pt::mm(0.0, 0.0), Pt::mm(2.0, 0.0), Pt::mm(2.0, 2.0), Pt::mm(0.0, 2.0)]];
        let wall = vec![vec![Pt::mm(7.0, -10.0), Pt::mm(8.0, -10.0), Pt::mm(8.0, 10.0), Pt::mm(7.0, 10.0)]];
        let g = gaps(&comp, &wall, 50.0);
        assert!((g[0].unwrap() - 5.0).abs() < 1e-6);
        assert_eq!(g[1], None);
        assert_eq!(g[2], None);
    }

    #[test]
    fn gaps_go_by_the_shapes_not_their_boxes() {
        // A thin pin at x 5, y 0.9..1.1 between rays a box-sampled cast would miss, and an
        // L-shaped part whose box reaches past its arm.
        let comp = vec![vec![Pt::mm(0.0, 0.0), Pt::mm(2.0, 0.0), Pt::mm(2.0, 2.0), Pt::mm(0.0, 2.0)]];
        let pin = vec![vec![Pt::mm(5.0, 0.95), Pt::mm(5.2, 0.95), Pt::mm(5.2, 1.05), Pt::mm(5.0, 1.05)]];
        let g = gaps(&comp, &pin, 50.0);
        assert!((g[0].unwrap() - 3.0).abs() < 1e-6, "{g:?}");
        let ell = vec![vec![Pt::mm(0.0, 0.0), Pt::mm(4.0, 0.0), Pt::mm(4.0, 1.0), Pt::mm(1.0, 1.0), Pt::mm(1.0, 4.0), Pt::mm(0.0, 4.0)]];
        // A post at (3.5, 3): the L's arm along x stops at y 1, so the room up is 2 from the
        // arm's top, not from the box's.
        let post = vec![vec![Pt::mm(3.0, 3.0), Pt::mm(3.8, 3.0), Pt::mm(3.8, 3.5), Pt::mm(3.0, 3.5)]];
        let g = gaps(&ell, &post, 50.0);
        assert!((g[2].unwrap() - 2.0).abs() < 1e-6, "{g:?}");
    }

    #[test]
    fn headroom_is_up_to_the_first_part_over_the_shadow() {
        let shadow = vec![vec![Pt::mm(0.0, 0.0), Pt::mm(2.0, 0.0), Pt::mm(2.0, 2.0), Pt::mm(0.0, 2.0)]];
        // A lid 10..11 above the frame's origin (z 20..21 world), over it.
        let lid = cube([-5.0, -5.0, 20.0], [5.0, 5.0, 21.0]);
        let r = headroom(&shadow, &[&lid], &TOP, 4.0, true, 50.0).unwrap();
        assert!((r - 6.0).abs() < 1e-9, "{r}");
        assert_eq!(headroom(&shadow, &[&lid], &TOP, 4.0, true, 3.0), None);
        // Under the board: a floor 3 below its bottom (z 7 world).
        let floor = cube([-5.0, -5.0, 5.0], [5.0, 5.0, 7.0]);
        let r = headroom(&shadow, &[&floor], &TOP, 0.0, false, 50.0).unwrap();
        assert!((r - 3.0).abs() < 1e-9, "{r}");
    }

    #[test]
    fn a_component_in_a_wall_collides_by_its_overlap() {
        let comp = cube([0.0, 0.0, 11.6], [4.0, 4.0, 14.0]);
        let wall = cube([3.0, -5.0, 0.0], [10.0, 10.0, 30.0]);
        let far = cube([30.0, 0.0, 0.0], [31.0, 1.0, 30.0]);
        let (comp, wall, far) = (Arc::new(comp), Arc::new(wall), Arc::new(far));
        let c = collisions(&[("J1", &comp)], &[("Wall", &wall), ("Far", &far)], &TOP);
        assert_eq!(c.len(), 1);
        assert_eq!(c[0].other, "Wall");
        assert!((c[0].depth - 1.0).abs() < 1e-6, "{}", c[0].depth);
        assert!((c[0].height - 2.4).abs() < 1e-6, "{}", c[0].height);
    }

    /// Two boxes as one solid (a connector: a low body and a tall post at one end).
    fn stepped() -> Solid {
        let mut b = cadrs_eda::model3d::Body::default();
        b.cuboid([0.0, 0.0, 0.0], [10.0, 4.0, 2.0], [1, 1, 1]);
        b.cuboid([0.0, 0.0, 2.0], [2.0, 4.0, 12.0], [1, 1, 1]);
        crate::pcb::mesh_solid::mesh_solid(&cadrs_eda::model3d::mesh(&b), uuid::Uuid::nil(), 1.0)
    }

    #[test]
    fn a_part_over_a_low_step_of_a_tall_component_is_clear() {
        let comp = Arc::new(stepped());
        // A shelf at z 4..6 over the low body's far end, clear of the post (x 0..2).
        let shelf = Arc::new(cube([5.0, -1.0, 4.0], [12.0, 5.0, 6.0]));
        assert!(collisions(&[("J1", &comp)], &[("Shelf", &shelf)], &TOP).is_empty());
        // Moved over the post it runs into it, by 1 mm (x 1..2) over its 2 mm.
        let shelf = Arc::new(cube([1.0, -1.0, 4.0], [12.0, 5.0, 6.0]));
        let c = collisions(&[("J1", &comp)], &[("Shelf", &shelf)], &TOP);
        assert_eq!(c.len(), 1);
        assert!((c[0].depth - 1.0).abs() < 1e-3 && (c[0].height - 2.0).abs() < 1e-2, "{c:?}");
    }

    #[test]
    fn a_pin_square_to_the_board_aligns_onto_a_hole() {
        let shift = |a: Option<Alignment>| a.map(|a| a.shift);
        // A pin along z at (3, 4); a hole's axis along z at (10, 2): move (7, −2).
        let d = shift(align(Entity::Axis([3.0, 4.0, 11.0], [0.0, 0.0, 1.0]), Entity::Axis([10.0, 2.0, 0.0], [0.0, 0.0, -1.0]), &TOP, [0.0; 2])).unwrap();
        assert!((d[0] - 7.0).abs() < 1e-9 && (d[1] + 2.0).abs() < 1e-9);
        // A face onto a face along x: only the x part of the offset moves.
        let d = shift(align(Entity::Plane([2.0, 5.0, 11.0], [1.0, 0.0, 0.0]), Entity::Plane([9.0, -3.0, 0.0], [-1.0, 0.0, 0.0]), &TOP, [0.0; 2])).unwrap();
        assert!((d[0] - 7.0).abs() < 1e-9 && d[1].abs() < 1e-9);
        // A wire's flat end (its middle) onto a pin's axis.
        let d = shift(align(Entity::Plane([1.0, 1.0, 15.0], [0.0, 0.0, 1.0]), Entity::Axis([4.0, -1.0, 0.0], [0.0, 0.0, 1.0]), &TOP, [0.0; 2])).unwrap();
        assert!((d[0] - 3.0).abs() < 1e-9 && (d[1] + 2.0).abs() < 1e-9);
        // An axis lying in the board can't be met by a move in it.
        assert!(align(Entity::Axis([0.0; 3], [1.0, 0.0, 0.0]), Entity::Point([1.0, 1.0, 1.0]), &TOP, [0.0; 2]).is_none());
        // Two points: how far apart they stay out of the board's plane.
        let a = align(Entity::Point([0.0, 0.0, 12.0]), Entity::Point([1.0, 1.0, 15.0]), &TOP, [0.0; 2]).unwrap();
        assert_eq!(a.off_plane, Some(3.0));
    }

    #[test]
    fn a_face_turns_to_meet_a_face_then_moves_onto_it() {
        // A connector's face at (2, 0) facing +x (its footprint's origin at (0, 0)); a wall's
        // face at y = 10 facing −y: a quarter turn (+x → +y), the face then at (0, 2), moved
        // 8 up to the wall.
        let a = align(Entity::Plane([2.0, 0.0, 11.0], [1.0, 0.0, 0.0]), Entity::Plane([5.0, 10.0, 11.0], [0.0, -1.0, 0.0]), &TOP, [0.0, 0.0]).unwrap();
        assert!((a.turn - 90.0).abs() < 1e-9 || (a.turn + 90.0).abs() < 1e-9, "{a:?}");
        // Turned −90 the face would be at (0, −2) facing −y: 12 to go. Either way it ends on y 10.
        let c = a.turn.to_radians();
        let y = 2.0 * c.sin() + a.shift[1];
        assert!((y - 10.0).abs() < 1e-9 && a.shift[0].abs() < 1e-9, "{a:?}");
        // Slightly askew (10°): turned back by 10°.
        let n = [(80.0f64).to_radians().cos(), (80.0f64).to_radians().sin(), 0.0];
        let a = align(Entity::Plane([0.0, 2.0, 11.0], n), Entity::Plane([0.0, 9.0, 11.0], [0.0, -1.0, 0.0]), &TOP, [0.0, 0.0]).unwrap();
        assert!((a.turn - 10.0).abs() < 1e-9, "{a:?}");
    }

    #[test]
    fn pins_standing_up_are_found_with_their_place_and_radius() {
        let mut b = cadrs_eda::model3d::Body::default();
        b.cylinder([5.0, -2.0, 0.0], cadrs_eda::model3d::Axis::Z, 1.2, 20.0, [1, 1, 1]);
        let pin = crate::pcb::mesh_solid::mesh_solid(&cadrs_eda::model3d::mesh(&b), uuid::Uuid::nil(), 1.0);
        let a = axes(&[&pin], &TOP, -1.0, 3.0);
        assert_eq!(a.len(), 1);
        assert!((a[0].0[0] - 5.0).abs() < 1e-3 && (a[0].0[1] + 2.0).abs() < 1e-3 && (a[0].1 - 1.2).abs() < 1e-2, "{a:?}");
        // Not reaching those heights: none.
        assert!(axes(&[&pin], &TOP, 15.0, 20.0).is_empty());
    }
}

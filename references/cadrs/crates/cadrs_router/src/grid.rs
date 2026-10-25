//! The board as the router sees it: a grid of cells `pitch` apart on each copper layer, each
//! cell saying which nets may run a track's centre line through it, and each via site which
//! nets may drop a via there ([`cadrs_router_kernels::passable`]).
//!
//! - Copper of a net (pads, tracks, vias) grown by the clearance and half a track width: only
//!   that net (where two nets' grown copper meet, nobody). Copper on no net: nobody.
//! - Holes, grown by the hole clearance: only the pad's own net, on every layer (nobody for a
//!   hole on no net); no via near any hole, nor on or beside a surface-mount pad.
//! - Off the board, or within the edge clearance of it: nobody.
//! - Keep-outs forbidding tracks (vias): nobody on their layers (no via inside).
//! - Via sites: the same, grown by a via's radius instead of half a track's, on every layer
//!   (a through via is copper on all of them).
//!
//! Copper pours (zones) are left out: they are poured again around whatever is routed.
//! Sizes are the worst case of the net classes, so every net keeps its clearance.

use cadrs_eda::board::Board;
use cadrs_eda::copper::{Copper, Item};
use cadrs_eda::layer::Layer;
use cadrs_eda::poly::{self, Region};
use cadrs_eda::units::{Bounds, Nm, Pt};
use cadrs_router_kernels::{FREE, Params, WALL};

/// The sizes the grid is grown by: the largest of the net classes'.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Sizes {
    pub track: Nm,
    pub clearance: Nm,
    pub via: Nm,
    pub drill: Nm,
    pub edge: Nm,
    pub hole: Nm,
    pub hole_to_hole: Nm,
}

impl Sizes {
    pub fn of(board: &Board) -> Sizes {
        let r = &board.rules;
        let classes = r.net_classes.iter();
        Sizes {
            track: classes.clone().map(|c| c.track_width).max().unwrap_or(0).max(r.min_track_width),
            clearance: classes.clone().map(|c| c.clearance).max().unwrap_or(0).max(r.min_clearance),
            via: classes.clone().map(|c| c.via_diameter).max().unwrap_or(0).max(r.min_via_diameter),
            drill: classes.map(|c| c.via_drill).max().unwrap_or(0).max(r.min_drill),
            edge: r.copper_edge_clearance,
            hole: r.hole_clearance,
            hole_to_hole: r.hole_to_hole,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Grid {
    /// Its size and the search's costs (filled in by the router).
    pub params: Params,
    /// The centre of cell (0, 0).
    pub origin: Pt,
    pub pitch: Nm,
    /// The copper layers, top first (grid layer `l` is `layers[l]`).
    pub layers: Vec<Layer>,
    /// Per cell: [`FREE`], [`WALL`] or the one net (id) that may use it.
    pub owner: Vec<u32>,
    /// Per `x, y`: who may drop a via there.
    pub via_owner: Vec<u32>,
    /// Net names by id - 1 (id 0: no net).
    pub nets: Vec<String>,
    pub sizes: Sizes,
}

/// Grown outlines are drawn with chords a little inside the true curve: via sites are grown by
/// this much more, so a via just outside one is clear by the full amount. (Tracks are not: at
/// a grid step of 0.1 mm the cells between fine-pitch pads sit right at the clearance, and DRC
/// measures a track against the same chords.)
const SLACK: Nm = poly::MAX_ERROR + 1_000;

/// What a cell becomes when `net` also wants it.
fn merge(owner: u32, net: u32) -> u32 {
    if net == WALL || net == FREE {
        return WALL;
    }
    match owner {
        FREE => net,
        o if o == net => o,
        _ => WALL,
    }
}

fn bounds_of(r: &Region) -> Option<Bounds> {
    r.iter().flatten().fold(None, |acc, p| Some(Bounds::union(acc, Bounds::of(*p))))
}

impl Grid {
    /// The grid of `board` at `pitch` (nets numbered in name order, so a board always gives the
    /// same grid).
    pub fn build(board: &Board, pitch: Nm) -> Grid {
        let pitch = pitch.max(1);
        let sizes = Sizes::of(board);
        let layers: Vec<Layer> = board.copper().collect();
        let items = cadrs_eda::copper::items(board);
        let mut nets: Vec<String> = items.iter().map(|c| c.net.clone()).filter(|n| !n.is_empty()).collect();
        nets.extend(board.nets.iter().filter(|n| !n.is_empty()).cloned());
        nets.sort();
        nets.dedup();
        let outline = cadrs_eda::outline::board_region(board);
        let extent = bounds_of(&outline).or_else(|| items.iter().map(|c| c.bounds).reduce(|a, b| Bounds::union(Some(a), b))).unwrap_or(Bounds { min: Pt::ZERO, max: Pt::ZERO });
        let snap = |v: Nm| v.div_euclid(pitch) * pitch;
        let origin = Pt::new(snap(extent.min.x), snap(extent.min.y));
        let nx = ((extent.max.x - origin.x) / pitch + 1).max(1) as u32;
        let ny = ((extent.max.y - origin.y) / pitch + 1).max(1) as u32;
        let nl = layers.len().max(1) as u32;
        let params = Params { nx, ny, nl, slots: 1, ..Default::default() };
        let mut g = Grid { params, origin, pitch, layers, owner: vec![FREE; params.cells() as usize], via_owner: vec![FREE; (nx * ny) as usize], nets, sizes };
        g.walls_off_board(&outline);
        for c in &items {
            g.stamp_item(c);
        }
        for z in board.zones.iter() {
            let Some(k) = z.keepout else { continue };
            let region: Region = z.outline.clone();
            if k.tracks {
                let grown = poly::inflate(&region, sizes.track / 2);
                for (li, l) in g.layers.clone().into_iter().enumerate() {
                    if z.layers.contains(l) {
                        g.stamp_layer(&grown, li, WALL);
                    }
                }
            }
            if k.vias {
                g.stamp_sites(&poly::inflate(&region, sizes.via / 2 + SLACK), WALL);
            }
        }
        g
    }

    pub fn center(&self, x: u32, y: u32) -> Pt {
        Pt::new(self.origin.x + x as Nm * self.pitch, self.origin.y + y as Nm * self.pitch)
    }

    /// The cell nearest `p`, if on the grid.
    pub fn cell_at(&self, p: Pt) -> Option<(u32, u32)> {
        let f = |v: Nm, o: Nm| ((v - o) as f64 / self.pitch as f64).round() as i64;
        let (x, y) = (f(p.x, self.origin.x), f(p.y, self.origin.y));
        (x >= 0 && y >= 0 && x < self.params.nx as i64 && y < self.params.ny as i64).then_some((x as u32, y as u32))
    }

    /// The id of net `name` (0: none).
    pub fn net_id(&self, name: &str) -> u32 {
        self.nets.binary_search_by(|n| n.as_str().cmp(name)).map_or(0, |i| i as u32 + 1)
    }

    pub fn net_name(&self, id: u32) -> &str {
        id.checked_sub(1).and_then(|i| self.nets.get(i as usize)).map_or("", |s| s.as_str())
    }

    pub fn layer_index(&self, l: Layer) -> Option<usize> {
        self.layers.iter().position(|x| *x == l)
    }

    /// The cells whose centres lie in `r`, as `(x, y)`.
    pub fn cells_in(&self, r: &Region) -> Vec<(u32, u32)> {
        let Some(b) = bounds_of(r) else { return vec![] };
        let p = self.pitch;
        let lo = |v: Nm, o: Nm| (((v - o) as f64) / p as f64).ceil().max(0.0) as i64;
        let hi = |v: Nm, o: Nm, n: u32| ((((v - o) as f64) / p as f64).floor() as i64).min(n as i64 - 1);
        let (x0, x1) = (lo(b.min.x, self.origin.x), hi(b.max.x, self.origin.x, self.params.nx));
        let (y0, y1) = (lo(b.min.y, self.origin.y), hi(b.max.y, self.origin.y, self.params.ny));
        let mut out = vec![];
        for y in y0..=y1 {
            for x in x0..=x1 {
                if poly::contains(r, self.center(x as u32, y as u32)) {
                    out.push((x as u32, y as u32));
                }
            }
        }
        out
    }

    fn stamp_layer(&mut self, r: &Region, layer: usize, net: u32) {
        for (x, y) in self.cells_in(r) {
            let i = self.params.index(x, y, layer as u32) as usize;
            self.owner[i] = merge(self.owner[i], net);
        }
    }

    fn stamp_sites(&mut self, r: &Region, net: u32) {
        for (x, y) in self.cells_in(r) {
            let i = (y * self.params.nx + x) as usize;
            self.via_owner[i] = merge(self.via_owner[i], net);
        }
    }

    /// Nobody routes off the board or within its edge clearance (no outline: no edge).
    fn walls_off_board(&mut self, outline: &Region) {
        if outline.is_empty() {
            return;
        }
        let s = self.sizes;
        let tracks = poly::inflate(outline, -(s.edge + s.track / 2));
        let vias = poly::inflate(outline, -(s.edge + s.via / 2 + SLACK));
        for y in 0..self.params.ny {
            for x in 0..self.params.nx {
                let c = self.center(x, y);
                if !poly::contains(&tracks, c) {
                    for l in 0..self.params.nl {
                        let i = self.params.index(x, y, l) as usize;
                        self.owner[i] = WALL;
                    }
                }
                if !poly::contains(&vias, c) {
                    self.via_owner[(y * self.params.nx + x) as usize] = WALL;
                }
            }
        }
    }

    /// A copper item's ground: its net's (grown by clearance and half a track, or a via), its
    /// hole nobody's.
    fn stamp_item(&mut self, c: &Copper) {
        if matches!(c.item, Item::Zone(..)) {
            return;
        }
        let net = if c.net.is_empty() { WALL } else { self.net_id(&c.net) };
        let s = self.sizes;
        let mut on_any: Vec<Region> = vec![];
        for (l, r) in &c.layers {
            let Some(li) = self.layer_index(*l) else { continue };
            self.stamp_layer(&poly::inflate(r, s.clearance + s.track / 2), li, net);
            on_any.push(r.clone());
        }
        if !on_any.is_empty() {
            // No via on or beside a surface-mount pad, not even its own net's: a track leaves
            // the pad first (a via in the pad wicks its solder away).
            let smd = matches!(c.item, Item::Pad(..)) && c.hole.is_none();
            self.stamp_sites(&poly::inflate(&poly::union_all(&on_any), s.clearance + s.via / 2 + SLACK), if smd { WALL } else { net });
        }
        if let Some(h) = &c.hole {
            // Other nets keep clear of a pad's hole; its own net's track may end on the pad.
            let grown = poly::inflate(h, s.hole + s.track / 2);
            for li in 0..self.layers.len() {
                self.stamp_layer(&grown, li, net);
            }
            self.stamp_sites(&poly::inflate(h, (s.hole + s.via / 2).max(s.hole_to_hole + s.drill / 2) + SLACK), WALL);
        }
    }
}

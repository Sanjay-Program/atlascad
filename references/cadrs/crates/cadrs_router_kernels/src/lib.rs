//! The autorouter's search step, shared by the CPU and the GPU.
//!
//! A board is a grid of cells on each copper layer: `x` across, `y` up, `l` the layer (index
//! `(l * ny + y) * nx + x`). A route moves between neighbouring cells on a layer, straight or
//! diagonally (octilinear), or between layers at the same cell through a via. Each cell says
//! who may use it ([`passable`]): anyone, only one net (the clearance around that net's own
//! copper), or nobody (other nets' copper, holes, the board edge, keep-outs).
//!
//! The cost of entering a cell is the move's ([`move_cost`]: straight, diagonal, against the
//! layer's preferred direction, or a via) plus the cell's congestion (what the negotiating
//! router charges for ground other nets want). [`relax_cell`] is one step of the shortest-path
//! wavefront: a cell's distance becomes the least of its own and each neighbour's plus the cost
//! of the move. Repeating it over every cell until nothing changes (Bellman–Ford, as cuflow's
//! Lee router spreads its cost grid) gives every cell's distance from the sources; on the GPU
//! each invocation relaxes one cell of one net's grid (`relax`, the shader's entry), so many
//! nets are searched at once. The CPU router runs A* with the same costs, so both find routes
//! of the same cost.
//!
//! Everything here is `u32` arithmetic over flat slices, so rust-gpu compiles it to SPIR-V.

#![cfg_attr(target_arch = "spirv", no_std)]

/// No route (yet).
pub const INF: u32 = u32::MAX;
/// A cell nobody may use.
pub const WALL: u32 = u32::MAX;
/// A cell anyone may use.
pub const FREE: u32 = 0;

/// The search's settings, as the shader reads them (a uniform buffer: `u32`s only).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[repr(C)]
pub struct Params {
    pub nx: u32,
    pub ny: u32,
    pub nl: u32,
    /// Nets searched at once (the GPU's batch; the distance buffer holds one grid each).
    pub slots: u32,
    /// A straight step, and a diagonal one (about √2 times it).
    pub ortho: u32,
    pub diag: u32,
    /// Changing layer.
    pub via: u32,
    /// Added to a step against its layer's preferred direction (half of it to a diagonal).
    pub wrong_way: u32,
    /// Layers that prefer horizontal runs, and vertical ones (bit `l`; neither: no preference).
    pub h_layers: u32,
    pub v_layers: u32,
    /// To 48 bytes (a uniform buffer's array would need 16-byte alignment: plain words).
    pub _pad0: u32,
    pub _pad1: u32,
}

impl Params {
    pub fn cells(&self) -> u32 {
        self.nx * self.ny * self.nl
    }

    pub fn index(&self, x: u32, y: u32, l: u32) -> u32 {
        (l * self.ny + y) * self.nx + x
    }

    /// A cell's `(x, y, layer)`.
    pub fn coords(&self, i: u32) -> (u32, u32, u32) {
        let plane = self.nx * self.ny;
        let l = i / plane;
        let r = i - l * plane;
        (r % self.nx, r / self.nx, l)
    }
}

/// Whether `net` may use a cell owned as `owner` ([`FREE`], [`WALL`] or a net id).
pub fn passable(owner: u32, net: u32) -> bool {
    owner == FREE || (owner != WALL && owner == net)
}

/// `a + b`, staying at [`INF`] instead of wrapping.
pub fn sat_add(a: u32, b: u32) -> u32 {
    if a >= INF - b { INF } else { a + b }
}

/// The cost of a step on layer `l` by `(dx, dy)` (each -1, 0 or 1; not both 0).
pub fn move_cost(p: &Params, l: u32, dx: i32, dy: i32) -> u32 {
    let h = (p.h_layers >> l) & 1 == 1;
    let v = (p.v_layers >> l) & 1 == 1;
    if dx != 0 && dy != 0 {
        let extra = if h || v { p.wrong_way / 2 } else { 0 };
        return p.diag + extra;
    }
    let against = (h && dy != 0) || (v && dx != 0);
    if against { p.ortho + p.wrong_way } else { p.ortho }
}

/// The eight steps around a cell, straight ones first.
pub const STEPS: [(i32, i32); 8] = [(1, 0), (-1, 0), (0, 1), (0, -1), (1, 1), (-1, 1), (1, -1), (-1, -1)];

/// One relaxation of cell `i` for `net`: the least of its distance and each neighbour's plus
/// the cost of the move into it. `dist` holds the net's grid from `base`. A diagonal step may
/// not cut a corner past a cell the net can't use; a via needs the cell free for the net on
/// both layers and the via site (`via_owner`, one per `x, y`) free for it.
#[allow(clippy::too_many_arguments)]
pub fn relax_cell(p: &Params, net: u32, i: u32, owner: &[u32], via_owner: &[u32], cong: &[u32], base: u32, dist: &[u32]) -> u32 {
    let mut best = dist[(base + i) as usize];
    if !passable(owner[i as usize], net) {
        return best;
    }
    let (x, y, l) = p.coords(i);
    let enter = cong[i as usize];
    let mut k = 0;
    while k < 8 {
        let (dx, dy) = STEPS[k];
        k += 1;
        // The neighbour the step comes from.
        let fx = x as i32 - dx;
        let fy = y as i32 - dy;
        if fx < 0 || fy < 0 || fx >= p.nx as i32 || fy >= p.ny as i32 {
            continue;
        }
        let from = p.index(fx as u32, fy as u32, l);
        let d = dist[(base + from) as usize];
        if d == INF || !passable(owner[from as usize], net) {
            continue;
        }
        if dx != 0 && dy != 0 {
            let a = p.index(fx as u32, y, l);
            let b = p.index(x, fy as u32, l);
            if !passable(owner[a as usize], net) || !passable(owner[b as usize], net) {
                continue;
            }
        }
        let c = sat_add(sat_add(d, move_cost(p, l, dx, dy)), enter);
        if c < best {
            best = c;
        }
    }
    let site = y * p.nx + x;
    if passable(via_owner[site as usize], net) {
        let mut m = 0;
        while m < p.nl {
            if m != l {
                let from = p.index(x, y, m);
                let d = dist[(base + from) as usize];
                if d != INF && passable(owner[from as usize], net) {
                    let c = sat_add(sat_add(d, p.via), enter);
                    if c < best {
                        best = c;
                    }
                }
            }
            m += 1;
        }
    }
    best
}

#[cfg(target_arch = "spirv")]
use spirv_std::{glam::UVec3, spirv};

/// The SPIR-V entry point (rust-gpu; entry name `relax`): one invocation relaxes one cell (`x`)
/// of one net's grid (`y`, a slot of the batch), and flags the slot when its distance went
/// down.
#[cfg(target_arch = "spirv")]
#[spirv(compute(threads(64)))]
#[allow(clippy::too_many_arguments)]
pub fn relax(
    #[spirv(global_invocation_id)] id: UVec3,
    #[spirv(uniform, descriptor_set = 0, binding = 0)] p: &Params,
    #[spirv(storage_buffer, descriptor_set = 0, binding = 1)] owner: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 2)] via_owner: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 3)] cong: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 4)] nets: &[u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 5)] dist: &mut [u32],
    #[spirv(storage_buffer, descriptor_set = 0, binding = 6)] changed: &mut [u32],
) {
    let cells = p.cells();
    let i = id.x;
    let slot = id.y;
    if i >= cells || slot >= p.slots {
        return;
    }
    let base = slot * cells;
    let net = nets[slot as usize];
    let cur = dist[(base + i) as usize];
    // Neighbours may be written while this reads them: distances only fall, so a stale read
    // only delays the wavefront by a pass.
    let v = relax_cell(p, net, i, owner, via_owner, cong, base, dist);
    if v < cur {
        dist[(base + i) as usize] = v;
        changed[slot as usize] = 1;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params(nx: u32, ny: u32, nl: u32) -> Params {
        Params { nx, ny, nl, slots: 1, ortho: 10, diag: 14, via: 50, wrong_way: 0, ..Default::default() }
    }

    /// Relaxes every cell until nothing changes (the GPU's loop, on the CPU).
    fn wavefront(p: &Params, net: u32, owner: &[u32], via_owner: &[u32], sources: &[u32]) -> Vec<u32> {
        let cong = vec![0; p.cells() as usize];
        let mut dist = vec![INF; p.cells() as usize];
        for &s in sources {
            dist[s as usize] = 0;
        }
        loop {
            let mut changed = false;
            for i in 0..p.cells() {
                let v = relax_cell(p, net, i, owner, via_owner, &cong, 0, &dist);
                if v < dist[i as usize] {
                    dist[i as usize] = v;
                    changed = true;
                }
            }
            if !changed {
                return dist;
            }
        }
    }

    #[test]
    fn cells_index_and_back() {
        // OrthoRoute's lattice: every (x, y, layer) has its own index and comes back from it.
        let p = params(7, 5, 3);
        let mut seen = vec![false; p.cells() as usize];
        for l in 0..3 {
            for y in 0..5 {
                for x in 0..7 {
                    let i = p.index(x, y, l);
                    assert!(!seen[i as usize]);
                    seen[i as usize] = true;
                    assert_eq!(p.coords(i), (x, y, l));
                }
            }
        }
    }

    #[test]
    fn the_wavefront_spreads_octilinearly_and_around_walls() {
        let p = params(10, 10, 1);
        let free = vec![FREE; 100];
        let d = wavefront(&p, 1, &free, &free[..100], &[p.index(0, 0, 0)]);
        // Five diagonal steps and three straight ones.
        assert_eq!(d[p.index(8, 5, 0) as usize], 5 * 14 + 3 * 10);
        // A wall from (5, 0) to (5, 8): round its end at y 9.
        let mut owner = free.clone();
        for y in 0..9 {
            owner[p.index(5, y, 0) as usize] = WALL;
        }
        let d = wavefront(&p, 1, &owner, &free, &[p.index(0, 0, 0)]);
        assert_eq!(d[p.index(5, 4, 0) as usize], INF);
        let around = d[p.index(9, 0, 0) as usize];
        assert!(around > 9 * 10 && around < INF, "{around}");
        // A cell of another net blocks; one of this net's is its own.
        owner[p.index(5, 9, 0) as usize] = 7;
        let d = wavefront(&p, 1, &owner, &free, &[p.index(0, 0, 0)]);
        assert_eq!(d[p.index(9, 0, 0) as usize], INF);
        let d = wavefront(&p, 7, &owner, &free, &[p.index(0, 0, 0)]);
        assert_eq!(d[p.index(9, 0, 0) as usize], around);
    }

    #[test]
    fn a_diagonal_does_not_cut_a_corner() {
        let p = params(3, 3, 1);
        let mut owner = vec![FREE; 9];
        owner[p.index(1, 0, 0) as usize] = WALL;
        owner[p.index(0, 1, 0) as usize] = WALL;
        let d = wavefront(&p, 1, &owner, &[FREE; 9], &[p.index(0, 0, 0)]);
        assert_eq!(d[p.index(1, 1, 0) as usize], INF);
    }

    #[test]
    fn a_wall_on_one_layer_is_passed_through_vias() {
        // cuflow's two-layer Lee router: horizontal runs on layer 0, vertical on layer 1, a via
        // costs more than a step.
        let mut p = params(9, 9, 2);
        p.wrong_way = 40;
        p.h_layers = 0b01;
        p.v_layers = 0b10;
        let mut owner = vec![FREE; p.cells() as usize];
        for y in 0..9 {
            owner[p.index(4, y, 0) as usize] = WALL;
        }
        let sites = vec![FREE; 81];
        let d = wavefront(&p, 1, &owner, &sites, &[p.index(0, 4, 0)]);
        // Over the wall on layer 1 and back: two vias.
        let to = d[p.index(8, 4, 0) as usize];
        assert!(to >= 2 * p.via && to < INF, "{to}");
        // Straight along layer 0's preferred direction costs no more than the straight steps.
        assert_eq!(d[p.index(3, 4, 0) as usize], 30);
        // With no via site free the far side can't be reached.
        let d = wavefront(&p, 1, &owner, &[WALL; 81], &[p.index(0, 4, 0)]);
        assert_eq!(d[p.index(8, 4, 0) as usize], INF);
    }
}

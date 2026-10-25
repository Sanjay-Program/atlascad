//! One search on the grid: from a net's tree (every cell of it a start) to the nearest cell of
//! its other pieces of copper. The CPU runs A* over the moves of
//! [`cadrs_router_kernels`]; [`trace_back`] walks a finished distance grid (the GPU's) back
//! from a target the same way, so both give routes of the same cost.

use std::cmp::Reverse;
use std::collections::BinaryHeap;

use cadrs_router_kernels::{INF, STEPS, move_cost, passable, sat_add};

use crate::grid::Grid;

/// What entering a cell costs besides the move: its congestion, as the negotiation prices it
/// now ([`Costs::cong`]).
pub struct Costs<'a> {
    pub grid: &'a Grid,
    /// How many other nets' routes claim each cell.
    pub occ: &'a [u16],
    /// What fighting over each cell has cost so far.
    pub hist: &'a [u32],
    /// What one other net's claim on a cell costs now.
    pub present: u32,
}

impl Costs<'_> {
    pub fn cong(&self, i: u32) -> u32 {
        sat_add(self.hist[i as usize], (self.occ[i as usize] as u32).saturating_mul(self.present))
    }

    /// Every move out of cell `i` for `net`: (to, cost).
    fn moves(&self, net: u32, i: u32, mut f: impl FnMut(u32, u32)) {
        let g = self.grid;
        let p = &g.params;
        let (x, y, l) = p.coords(i);
        let free = |c: u32| passable(g.owner[c as usize], net);
        for (dx, dy) in STEPS {
            let (tx, ty) = (x as i32 + dx, y as i32 + dy);
            if tx < 0 || ty < 0 || tx >= p.nx as i32 || ty >= p.ny as i32 {
                continue;
            }
            let to = p.index(tx as u32, ty as u32, l);
            if !free(to) {
                continue;
            }
            if dx != 0 && dy != 0 && (!free(p.index(tx as u32, y, l)) || !free(p.index(x, ty as u32, l))) {
                continue;
            }
            f(to, sat_add(move_cost(p, l, dx, dy), self.cong(to)));
        }
        if passable(g.via_owner[(y * p.nx + x) as usize], net) {
            for m in (0..p.nl).filter(|m| *m != l) {
                let to = p.index(x, y, m);
                if free(to) {
                    f(to, sat_add(p.via, self.cong(to)));
                }
            }
        }
    }
}

/// The A* search's working arrays, kept between searches (stamped, not cleared).
#[derive(Default)]
pub struct Searcher {
    g: Vec<u32>,
    parent: Vec<u32>,
    seen: Vec<u32>,
    stamp: u32,
    target: Vec<u32>,
}

impl Searcher {
    /// The cheapest path for `net` from any of `sources` to any of `targets` (cells), source
    /// first; `None` when none is reachable.
    pub fn search(&mut self, c: &Costs, net: u32, sources: &[u32], targets: &[u32]) -> Option<Vec<u32>> {
        let p = &c.grid.params;
        let n = p.cells() as usize;
        if self.g.len() != n {
            *self = Searcher { g: vec![INF; n], parent: vec![INF; n], seen: vec![0; n], stamp: 0, target: vec![0; n] };
        }
        self.stamp = self.stamp.wrapping_add(1);
        if self.stamp == 0 {
            self.seen.fill(0);
            self.target.fill(0);
            self.stamp = 1;
        }
        let s = self.stamp;
        // The targets' box: the heuristic is the octilinear distance to it.
        let (mut x0, mut y0, mut x1, mut y1) = (u32::MAX, u32::MAX, 0, 0);
        for &t in targets {
            self.target[t as usize] = s;
            let (x, y, _) = p.coords(t);
            (x0, y0, x1, y1) = (x0.min(x), y0.min(y), x1.max(x), y1.max(y));
        }
        if targets.is_empty() {
            return None;
        }
        let (ortho, diag) = (p.ortho, p.diag.min(2 * p.ortho));
        let h = |i: u32| {
            let (x, y, _) = p.coords(i);
            let dx = if x < x0 { x0 - x } else { x.saturating_sub(x1) };
            let dy = if y < y0 { y0 - y } else { y.saturating_sub(y1) };
            let (lo, hi) = (dx.min(dy), dx.max(dy));
            lo * diag + (hi - lo) * ortho
        };
        let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();
        for &src in sources {
            if !passable(c.grid.owner[src as usize], net) {
                continue;
            }
            self.g[src as usize] = 0;
            self.parent[src as usize] = INF;
            self.seen[src as usize] = s;
            heap.push(Reverse((h(src), src)));
        }
        while let Some(Reverse((f, i))) = heap.pop() {
            let gi = self.g[i as usize];
            if f > sat_add(gi, h(i)) {
                continue;
            }
            if self.target[i as usize] == s {
                let mut path = vec![i];
                let mut cur = i;
                while self.parent[cur as usize] != INF {
                    cur = self.parent[cur as usize];
                    path.push(cur);
                }
                path.reverse();
                return Some(path);
            }
            let (seen, g, parent) = (&mut self.seen, &mut self.g, &mut self.parent);
            c.moves(net, i, |to, cost| {
                let ng = sat_add(gi, cost);
                if ng == INF {
                    return;
                }
                if seen[to as usize] != s || ng < g[to as usize] {
                    seen[to as usize] = s;
                    g[to as usize] = ng;
                    parent[to as usize] = i;
                    heap.push(Reverse((sat_add(ng, h(to)), to)));
                }
            });
        }
        None
    }
}

/// The path to `target` in a finished distance grid (`dist`: 0 at the sources, the cheapest
/// cost to each cell), source first: each step back is to a neighbour whose distance plus the
/// move's cost is the cell's.
pub fn trace_back(c: &Costs, net: u32, dist: &[u32], target: u32) -> Option<Vec<u32>> {
    if dist[target as usize] == INF {
        return None;
    }
    let mut path = vec![target];
    let mut cur = target;
    let mut guard = dist.len();
    while dist[cur as usize] != 0 {
        let want = dist[cur as usize];
        let mut prev = None;
        // A move from `from` into `cur` is a move out of `from` (the moves are symmetric).
        let p = &c.grid.params;
        let (x, y, l) = p.coords(cur);
        let mut consider = |from: u32| {
            if prev.is_some() || dist[from as usize] == INF {
                return;
            }
            c.moves(net, from, |to, cost| {
                if to == cur && sat_add(dist[from as usize], cost) == want && prev.is_none() {
                    prev = Some(from);
                }
            });
        };
        for (dx, dy) in STEPS {
            let (fx, fy) = (x as i32 - dx, y as i32 - dy);
            if fx >= 0 && fy >= 0 && fx < p.nx as i32 && fy < p.ny as i32 {
                consider(p.index(fx as u32, fy as u32, l));
            }
        }
        for m in (0..p.nl).filter(|m| *m != l) {
            consider(p.index(x, y, m));
        }
        cur = prev?;
        path.push(cur);
        guard -= 1;
        if guard == 0 {
            return None;
        }
    }
    path.reverse();
    Some(path)
}

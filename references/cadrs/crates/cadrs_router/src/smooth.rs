//! Straightens finished routes. The grid search takes any mix of straight and diagonal steps
//! of the same cost, so a route can come out as a staircase of little jogs; here each run on a
//! layer is redrawn, from where it is to as far along it as can be reached, as the shortest
//! 45° path there (one straight or diagonal line, or one of each), where that path stays on
//! ground the net may use and off every other net's claim. Vias and the cells other
//! connections start from stay where they are.

use cadrs_router_kernels::passable;

use crate::negotiate::{NetRoute, Negotiation};

/// How far along a run one redrawn piece may reach (cells): bounds the work on long runs.
const REACH: usize = 400;

/// Straightens every net's route, one net at a time against the others as they are by then.
pub fn all(neg: &mut Negotiation) {
    for j in 0..neg.jobs.len() {
        let route = neg.routes[j].clone();
        if route.paths.is_empty() {
            continue;
        }
        neg.rip(j);
        let smoothed = net(neg, j, &route);
        neg.put(j, smoothed);
    }
}

/// Job `j`'s route straightened (its own claim off the board, the others' on it).
fn net(neg: &Negotiation, j: usize, route: &NetRoute) -> NetRoute {
    let g = neg.grid;
    let p = &g.params;
    let id = neg.jobs[j].net;
    let free = |x: i32, y: i32, l: u32| {
        if x < 0 || y < 0 || x >= p.nx as i32 || y >= p.ny as i32 {
            return false;
        }
        let c = p.index(x as u32, y as u32, l) as usize;
        passable(g.owner[c], id) && neg.occ[c] == 0
    };
    // Where the connections start: they stay on the route.
    let starts: Vec<u32> = route.paths.iter().filter_map(|p| p.first().copied()).collect();
    let mut paths = vec![];
    for path in &route.paths {
        let mut out: Vec<u32> = vec![];
        let mut k = 0;
        while k < path.len() {
            let l = p.coords(path[k]).2;
            let end = path[k..].iter().position(|&c| p.coords(c).2 != l).map_or(path.len(), |n| k + n);
            let run: Vec<(i32, i32)> = path[k..end].iter().map(|&c| p.coords(c)).map(|(x, y, _)| (x as i32, y as i32)).collect();
            let fixed: Vec<bool> = path[k..end].iter().map(|c| starts.contains(c)).collect();
            out.extend(straighten(&run, &fixed, |x, y| free(x, y, l)).into_iter().map(|(x, y)| p.index(x as u32, y as u32, l)));
            k = end;
        }
        paths.push(out);
    }
    NetRoute { paths, missing: route.missing }
}

/// A run of cells (each a step from the last) redrawn with as few 45° turns as `free` allows;
/// its ends and the `fixed` cells kept.
fn straighten(run: &[(i32, i32)], fixed: &[bool], free: impl Fn(i32, i32) -> bool) -> Vec<(i32, i32)> {
    let Some(&first) = run.first() else { return vec![] };
    let mut out = vec![first];
    let mut i = 0;
    while i + 1 < run.len() {
        let next_fixed = (i + 1..run.len()).find(|&n| fixed[n]).unwrap_or(run.len() - 1);
        let lim = next_fixed.min(i + REACH);
        let mut to = i + 1;
        let mut cells = vec![run[i + 1]];
        for k in (i + 2..=lim).rev() {
            if let Some(c) = octilinear(run[i], run[k], &free) {
                (to, cells) = (k, c);
                break;
            }
        }
        out.extend(cells);
        i = to;
    }
    out
}

/// The cells after `a` of a shortest 45° path from `a` to `b`: a diagonal and a straight
/// line, in either order, all on `free` cells (a diagonal step not cutting a corner either).
fn octilinear(a: (i32, i32), b: (i32, i32), free: &impl Fn(i32, i32) -> bool) -> Option<Vec<(i32, i32)>> {
    let (dx, dy) = (b.0 - a.0, b.1 - a.1);
    let (sx, sy) = (dx.signum(), dy.signum());
    let diag = dx.abs().min(dy.abs());
    let line = dx.abs().max(dy.abs()) - diag;
    let straight = if dx.abs() > dy.abs() { (sx, 0) } else { (0, sy) };
    let walk = |legs: [((i32, i32), i32); 2]| {
        let mut at = a;
        let mut cells = vec![];
        for ((ux, uy), n) in legs {
            for _ in 0..n {
                let to = (at.0 + ux, at.1 + uy);
                if !free(to.0, to.1) || (ux != 0 && uy != 0 && (!free(to.0, at.1) || !free(at.0, to.1))) {
                    return None;
                }
                cells.push(to);
                at = to;
            }
        }
        Some(cells)
    };
    walk([((sx, sy), diag), (straight, line)]).or_else(|| walk([(straight, line), ((sx, sy), diag)]))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The jogs of a near-horizontal staircase go: one diagonal and one straight line.
    #[test]
    fn a_staircase_becomes_two_lines() {
        let run = [(0, 0), (1, 0), (2, 1), (3, 1), (4, 1), (5, 2), (6, 2), (7, 2)];
        let out = straighten(&run, &[false; 8], |_, _| true);
        assert_eq!(out.first(), Some(&(0, 0)));
        assert_eq!(out.last(), Some(&(7, 2)));
        let turns = out.windows(3).filter(|w| (w[1].0 - w[0].0, w[1].1 - w[0].1) != (w[2].0 - w[1].0, w[2].1 - w[1].1)).count();
        assert_eq!(turns, 1, "{out:?}");
    }

    /// A blocked shortcut keeps the route around the obstacle, and fixed cells stay on it.
    #[test]
    fn walls_and_fixed_cells_are_kept() {
        let run = [(0, 0), (0, 1), (0, 2), (1, 2), (2, 2), (2, 1), (2, 0)];
        let out = straighten(&run, &[false; 7], |x, y| !(x == 1 && y < 2));
        assert!(out.iter().all(|&(x, y)| !(x == 1 && y < 2)), "{out:?}");
        let out = straighten(&[(0, 0), (1, 0), (2, 1), (3, 1)], &[false, false, true, false], |_, _| true);
        assert!(out.contains(&(2, 1)), "{out:?}");
    }
}

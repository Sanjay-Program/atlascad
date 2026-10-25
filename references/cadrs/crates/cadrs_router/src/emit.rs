//! Routes back to copper: each run of cells on a layer becomes tracks from turn to turn (cells
//! in a straight or diagonal line make one track), and each change of layer a through via.

use cadrs_eda::board::{Track, Via, ViaKind};
use cadrs_eda::units::Pt;
use uuid::Uuid;

use crate::grid::Grid;
use crate::negotiate::{Job, NetRoute};

/// A net's route as tracks and vias (a via once where several connections change layer).
pub fn copper(grid: &Grid, job: &Job, route: &NetRoute, drill: cadrs_eda::units::Nm) -> (Vec<Track>, Vec<Via>) {
    let p = &grid.params;
    let net = grid.net_name(job.net).to_string();
    let mut tracks = vec![];
    let mut vias: Vec<Via> = vec![];
    let (top, bottom) = (grid.layers[0], *grid.layers.last().unwrap());
    for path in &route.paths {
        // The points where the run turns, per layer run.
        let mut run: Vec<(u32, u32)> = vec![];
        let mut layer = None;
        let flush = |run: &mut Vec<(u32, u32)>, layer: Option<u32>, tracks: &mut Vec<Track>| {
            let Some(l) = layer else { return };
            let mut corners: Vec<(u32, u32)> = vec![];
            for (k, &c) in run.iter().enumerate() {
                let turn = k == 0 || k + 1 == run.len() || {
                    let (a, b) = (run[k - 1], run[k + 1]);
                    let d1 = (c.0 as i64 - a.0 as i64, c.1 as i64 - a.1 as i64);
                    let d2 = (b.0 as i64 - c.0 as i64, b.1 as i64 - c.1 as i64);
                    d1 != d2
                };
                if turn {
                    corners.push(c);
                }
            }
            for w in corners.windows(2) {
                let (a, b) = (grid.center(w[0].0, w[0].1), grid.center(w[1].0, w[1].1));
                if a != b {
                    tracks.push(Track { id: Uuid::new_v4(), a, mid: None, b, width: job.width, layer: grid.layers[l as usize], net: net.clone(), locked: false });
                }
            }
            run.clear();
        };
        for &c in path {
            let (x, y, l) = p.coords(c);
            if layer.is_some_and(|m| m != l) {
                flush(&mut run, layer, &mut tracks);
                let at: Pt = grid.center(x, y);
                if !vias.iter().any(|v| v.at == at) {
                    vias.push(Via { id: Uuid::new_v4(), at, diameter: job.via, drill, kind: ViaKind::Through, from: top, to: bottom, net: net.clone(), locked: false, tented: None });
                }
            }
            layer = Some(l);
            run.push((x, y));
        }
        flush(&mut run, layer, &mut tracks);
    }
    (tracks, vias)
}

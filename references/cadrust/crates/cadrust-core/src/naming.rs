//! Persistent naming: every face of every body carries a name that survives edits
//! upstream in the timeline, and edges are named by the faces they border.
//!
//! Names are assigned when a face is born (from the sketch entity, primitive side or
//! feature that created it) and carried forward through later features:
//! - booleans, fillet, chamfer and shell: by the kernel's face history (`Solid::iter_history`);
//! - rigid transforms (move, mirror, pattern copies): by transforming the old face centers;
//! - anything else: by nearest center, then `{feature}/f{k}` for faces nobody claims.
//!
//! Edge name = sorted pair of its face names joined with `|`; a `#k` suffix separates
//! several edges between the same two faces. Fillet/chamfer selections store these names
//! next to the pick point; the name is tried first and the point is the fallback.

use cadrum::{DVec3, Edge, Face, Solid};
use std::collections::HashMap;

/// Face id -> (center, name) entries. Ids are global to the process, so one table serves
/// every body and every intermediate tool solid. A prism's two caps share one id (same
/// underlying shape, different placement), so an id may carry several entries and the
/// center picks between them.
#[derive(Default)]
pub struct Names {
    entries: HashMap<u64, Vec<(DVec3, String)>>,
}

const SAME: f64 = 1e-6;

impl Names {
    pub fn get(&self, face: &Face) -> Option<&str> {
        self.nearest(face.id(), face.center()).map(|(_, n)| n.as_str())
    }

    fn nearest(&self, id: u64, c: DVec3) -> Option<&(DVec3, String)> {
        self.entries.get(&id)?.iter().min_by(|a, b| (a.0 - c).length().partial_cmp(&(b.0 - c).length()).unwrap())
    }

    fn has_exact(&self, face: &Face) -> bool {
        let c = face.center();
        self.entries.get(&face.id()).map_or(false, |v| v.iter().any(|(oc, _)| (*oc - c).length() < SAME))
    }

    /// (center, name) of every named face of a solid, for the evaluation cache.
    pub fn face_entries(&self, solid: &Solid) -> Vec<(DVec3, String)> {
        solid.iter_face().filter_map(|fc| self.get(fc).map(|n| (fc.center(), n.to_string()))).collect()
    }

    /// Re-attach cached names to a solid restored from the cache (same geometry, new ids).
    pub fn restore(&mut self, solid: &Solid, entries: &[(DVec3, String)]) {
        for fc in solid.iter_face() {
            let c = fc.center();
            if let Some((_, n)) = entries.iter().find(|(oc, _)| (*oc - c).length() < SAME) {
                self.set(fc, n.clone());
            }
        }
    }

    pub fn set(&mut self, face: &Face, name: String) {
        let c = face.center();
        let v = self.entries.entry(face.id()).or_default();
        match v.iter_mut().find(|(oc, _)| (*oc - c).length() < SAME) {
            Some(e) => e.1 = name,
            None => v.push((c, name)),
        }
    }

    /// Name the faces of a fresh solid made from profile edges (extrude, revolve, ...).
    /// `probe(edge_mid)` returns a point on the lateral face grown from that edge; faces
    /// that no edge claims are caps, named `start`/`end` along `dir`.
    pub fn name_from_profile(&mut self, solid: &Solid, prefix: &str, edges: &[Edge], probe: impl Fn(DVec3) -> DVec3, dir: DVec3) {
        let [lo, hi] = solid.bounding_box();
        let tol = ((hi - lo).length() * 1e-3).max(1e-6);
        let faces: Vec<&Face> = solid.iter_face().collect();
        let mut claimed = vec![false; faces.len()];
        for (j, e) in edges.iter().enumerate() {
            let p = probe(crate::eval::edge_mid(&crate::eval::edge_points(e)));
            let mut best: Option<(usize, f64)> = None;
            for (i, fc) in faces.iter().enumerate() {
                if claimed[i] {
                    continue;
                }
                let (hit, _) = fc.project(p);
                let d = (hit - p).length();
                if d < tol && best.map_or(true, |(_, bd)| d < bd) {
                    best = Some((i, d));
                }
            }
            if let Some((i, _)) = best {
                claimed[i] = true;
                self.set(faces[i], format!("{prefix}/e{j}"));
            }
        }
        let mut caps: Vec<usize> = (0..faces.len()).filter(|i| !claimed[*i]).collect();
        caps.sort_by(|a, b| faces[*a].center().dot(dir).partial_cmp(&faces[*b].center().dot(dir)).unwrap());
        for (k, i) in caps.iter().enumerate() {
            let tag = match (k, caps.len()) {
                (0, 2) => "start".to_string(),
                (1, 2) => "end".to_string(),
                (0, 1) => "cap".to_string(),
                _ => format!("f{k}"),
            };
            self.set(faces[*i], format!("{prefix}/{tag}"));
        }
    }

    /// Name primitive faces by their outward normal at the center (box) or surface kind (cylinder, cone).
    pub fn name_primitive(&mut self, solid: &Solid, prefix: &str) {
        let mut counts: HashMap<String, usize> = HashMap::new();
        let faces: Vec<&Face> = solid.iter_face().collect();
        for fc in &faces {
            let c = fc.center();
            let kind = fc.surface().map(|s| s.kind);
            let base = match kind {
                Some(cadrum::SurfaceKind::Plane) => {
                    let (_, n) = fc.project(c);
                    axis_name(if n.length() > 0.5 { n } else { DVec3::Z })
                }
                Some(cadrum::SurfaceKind::Cylinder { .. }) => "side".to_string(),
                Some(cadrum::SurfaceKind::Cone { .. }) => "cone".to_string(),
                Some(cadrum::SurfaceKind::Sphere { .. }) => "sphere".to_string(),
                _ => "face".to_string(),
            };
            let n = counts.entry(base.clone()).or_insert(0);
            let name = if *n == 0 { format!("{prefix}/{base}") } else { format!("{prefix}/{base}{n}") };
            *n += 1;
            self.set(fc, name);
        }
    }

    /// Carry names onto `new` after a feature replaced `old_ids` (the body's faces before the
    /// feature). `transform` maps old centers to new ones for rigid moves (identity otherwise).
    /// Faces with no source get `{feature}/f{k}`, ordered by center for determinism.
    pub fn carry(&mut self, new: &Solid, feature: &str, transform: impl Fn(DVec3) -> DVec3) {
        let history: HashMap<u64, u64> = new.iter_history().map(|[post, src]| (post, src)).collect();
        let faces: Vec<&Face> = new.iter_face().collect();
        let [lo, hi] = new.bounding_box();
        let tol = ((hi - lo).length() * 1e-4).max(1e-6);
        let mut unnamed = Vec::new();
        for fc in &faces {
            if self.has_exact(fc) {
                continue;
            }
            let c = fc.center();
            // 1. kernel history (nearest entry when the source id is shared by several faces)
            if let Some(src) = history.get(&fc.id()) {
                if let Some((_, n)) = self.nearest(*src, c).cloned() {
                    self.set(fc, n);
                    continue;
                }
            }
            // 2. transformed center match against every known face
            let hit = self.entries.values().flatten().find(|(oc, _)| (transform(*oc) - c).length() < tol).map(|(_, n)| n.clone());
            if let Some(n) = hit {
                self.set(fc, n);
                continue;
            }
            unnamed.push(*fc);
        }
        unnamed.sort_by(|a, b| key(a.center()).partial_cmp(&key(b.center())).unwrap());
        let mut used: HashMap<String, usize> = HashMap::new();
        for fc in unnamed {
            let base = format!("{feature}/f");
            let k = used.entry(base.clone()).or_insert(0);
            // Skip indices already taken by earlier runs so names stay unique within the body.
            let mut name = format!("{base}{k}");
            while faces.iter().any(|o| self.get(o) == Some(name.as_str())) {
                *k += 1;
                name = format!("{base}{k}");
            }
            *k += 1;
            self.set(fc, name);
        }
    }

    /// Names of every edge of a solid, in `iter_edge` order.
    pub fn edge_names(&self, solid: &Solid) -> Vec<String> {
        let mut faces_of: HashMap<[i64; 3], Vec<String>> = HashMap::new();
        for fc in solid.iter_face() {
            let n = self.get(fc).unwrap_or("?").to_string();
            for e in fc.iter_edge() {
                faces_of.entry(crate::eval::mid_key(e)).or_default().push(n.clone());
            }
        }
        let mut names: Vec<String> = solid
            .iter_edge()
            .map(|e| {
                let mut fs = faces_of.get(&crate::eval::mid_key(e)).cloned().unwrap_or_default();
                fs.sort();
                fs.dedup();
                match fs.len() {
                    0 => "?".to_string(),
                    1 => format!("{}|seam", fs[0]),
                    _ => fs.join("|"),
                }
            })
            .collect();
        // Disambiguate repeated pairs by midpoint order.
        let edges: Vec<&Edge> = solid.iter_edge().collect();
        let mut groups: HashMap<String, Vec<usize>> = HashMap::new();
        for (i, n) in names.iter().enumerate() {
            groups.entry(n.clone()).or_default().push(i);
        }
        for (_, idx) in groups.into_iter().filter(|(_, v)| v.len() > 1) {
            let mut idx = idx;
            idx.sort_by(|a, b| key(crate::eval::edge_mid(&crate::eval::edge_points(edges[*a]))).partial_cmp(&key(crate::eval::edge_mid(&crate::eval::edge_points(edges[*b])))).unwrap());
            for (k, i) in idx.into_iter().enumerate() {
                names[i] = format!("{}#{k}", names[i]);
            }
        }
        names
    }
}

fn axis_name(n: DVec3) -> String {
    let a = [n.x.abs(), n.y.abs(), n.z.abs()];
    let i = (0..3).max_by(|a1, b1| a[*a1].partial_cmp(&a[*b1]).unwrap()).unwrap();
    let axis = ["x", "y", "z"][i];
    let sign = if [n.x, n.y, n.z][i] >= 0.0 { "+" } else { "-" };
    format!("{axis}{sign}")
}

/// Deterministic sort key for points (rounded so near-equal values do not flip order).
fn key(p: DVec3) -> (i64, i64, i64) {
    let r = |v: f64| (v * 1e4).round() as i64;
    (r(p.z), r(p.y), r(p.x))
}

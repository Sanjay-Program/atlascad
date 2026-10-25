//! Timeline evaluation: replay features in order on the kernel.

use crate::doc::{BoolOp, Document, EdgeSelect, Feature, HoleKind, NamedSelect, PlaneRef, SweepOrient};
use crate::expr::Scope;
use crate::naming::Names;
use crate::sketch::{self, Basis, Geo2, SketchGeometry};
use crate::{Error, Result};
use cadrum::{DVec3, Edge, Solid};
use serde::Serialize;
use std::collections::BTreeMap;

const EPS: f64 = 1e-6;

#[derive(Debug, Clone, Serialize)]
pub struct BodyInfo {
    pub id: String,
    pub volume: f64,
    pub area: f64,
    pub faces: usize,
    pub edges: usize,
    pub bbox_min: [f64; 3],
    pub bbox_max: [f64; 3],
}

#[derive(Debug, Clone, Serialize)]
pub struct FeatureStatus {
    pub id: String,
    pub kind: String,
    pub ok: bool,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub warning: Option<String>,
    /// Persistent names of the faces a feature picked by point (OffsetFace), for the editor to store.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub names: Option<Vec<String>>,
    pub millis: u128,
    /// Features this one reads from (dependency graph edges).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<String>,
    /// Outputs replayed from the evaluation cache instead of the kernel.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub cached: bool,
}

/// One B-rep edge as a polyline, for picking and crisp outlines in the viewer.
#[derive(Debug, Clone, Serialize)]
pub struct EdgeInfo {
    pub body: String,
    pub id: u64,
    /// Persistent name (`faceA|faceB`), stable across upstream edits.
    pub name: String,
    pub mid: [f64; 3],
    pub points: Vec<[f64; 3]>,
}

/// Evaluated sketch geometry in world coordinates.
#[derive(Debug, Clone, Serialize)]
pub struct SketchInfo {
    /// Persistent name of the face the sketch lies on, when it was placed on a body face.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub face_name: Option<String>,
    pub id: String,
    pub plane: PlaneRef,
    pub offset: f64,
    pub origin: [f64; 3],
    pub u: [f64; 3],
    pub v: [f64; 3],
    pub normal: [f64; 3],
    pub loops: Vec<Vec<[f64; 3]>>,
    pub open: Vec<Vec<[f64; 3]>>,
    pub entities: Vec<serde_json::Value>,
    pub dof: i64,
    pub underconstrained: Vec<usize>,
}

pub struct Evaluated {
    /// Final bodies in creation order.
    pub bodies: Vec<(String, Solid)>,
    pub status: Vec<FeatureStatus>,
    pub sketches: Vec<SketchInfo>,
    /// Persistent face names of every body (see `naming`).
    pub names: Names,
    /// Features replayed from the cache in this evaluation.
    pub reused: usize,
}

impl Evaluated {
    pub fn solids(&self) -> Vec<&Solid> {
        self.bodies.iter().map(|(_, s)| s).collect()
    }
    /// Solids of the listed bodies, or all when `ids` is empty.
    pub fn solids_of(&self, ids: &[String]) -> Vec<&Solid> {
        self.bodies.iter().filter(|(id, _)| ids.is_empty() || ids.contains(id)).map(|(_, s)| s).collect()
    }
    pub fn info(&self) -> Vec<BodyInfo> {
        self.bodies.iter().map(|(id, s)| body_info(id, s)).collect()
    }
    pub fn edges(&self) -> Vec<EdgeInfo> {
        let mut out = Vec::new();
        for (id, s) in &self.bodies {
            let names = self.names.edge_names(s);
            for (e, name) in s.iter_edge().zip(names) {
                let pts = edge_points(e);
                let mid = edge_mid(&pts);
                out.push(EdgeInfo { body: id.clone(), id: e.id(), name, mid: mid.to_array(), points: pts.iter().map(|p| p.to_array()).collect() });
            }
        }
        out
    }
    /// Face names of a body with their centers, for inspection and tests.
    pub fn face_names(&self, body: &str) -> Vec<(String, [f64; 3])> {
        let mut out = Vec::new();
        if let Some((_, s)) = self.bodies.iter().find(|(id, _)| id == body) {
            for fc in s.iter_face() {
                out.push((self.names.get(fc).unwrap_or("?").to_string(), fc.center().to_array()));
            }
        }
        out
    }
    pub fn has_errors(&self) -> bool {
        self.status.iter().any(|s| !s.ok)
    }
}

pub fn body_info(id: &str, s: &Solid) -> BodyInfo {
    let [lo, hi] = s.bounding_box();
    BodyInfo { id: id.to_string(), volume: s.volume(), area: s.area(), faces: s.iter_face().count(), edges: s.iter_edge().count(), bbox_min: lo.to_array(), bbox_max: hi.to_array() }
}

pub(crate) fn edge_points(e: &Edge) -> Vec<DVec3> {
    let pts = e.approximation_segments(cadrum::Tessellation { deflection_angular: 0.3, ..Default::default() });
    if pts.len() >= 2 {
        pts
    } else {
        vec![e.start_point(), e.end_point()]
    }
}

/// Midpoint along the polyline (by arc length), so closed edges get a stable point too.
pub(crate) fn edge_mid(pts: &[DVec3]) -> DVec3 {
    let total: f64 = pts.windows(2).map(|w| (w[1] - w[0]).length()).sum();
    if total < EPS {
        return pts[0];
    }
    let mut acc = 0.0;
    for w in pts.windows(2) {
        let l = (w[1] - w[0]).length();
        if acc + l >= total / 2.0 {
            let t = (total / 2.0 - acc) / l.max(EPS);
            return w[0] + (w[1] - w[0]) * t;
        }
        acc += l;
    }
    pts[pts.len() - 1]
}

/// Evaluate the whole timeline. `overrides` replaces parameter values (CLI --set).
/// A failing feature is reported in `status` and skipped; later features that
/// depend on it fail in turn with a clear message.
pub fn evaluate(doc: &Document, overrides: &BTreeMap<String, String>) -> Result<Evaluated> {
    evaluate_with(doc, overrides, None)
}

/// Evaluate with an incremental cache: features whose definition, parameter values and
/// upstream results are unchanged are replayed from `cache` (see `graph`).
pub fn evaluate_with(doc: &Document, overrides: &BTreeMap<String, String>, mut cache: Option<&mut crate::graph::EvalCache>) -> Result<Evaluated> {
    use crate::graph::{feature_key, Entry};
    let scope = Scope::new(&doc.parameters, overrides)?;
    let mut ctx = Ctx { sketches: BTreeMap::new(), bodies: Vec::new(), infos: Vec::new(), names: Names::default(), resolved: None };
    let mut status = Vec::new();
    let deps = crate::graph::dependency_graph(doc);
    let mut param_names: Vec<String> = doc.parameters.keys().cloned().collect();
    param_names.extend(overrides.keys().filter(|k| !doc.parameters.contains_key(*k)).cloned());
    // object id -> key of the feature that last wrote it
    let mut last: std::collections::HashMap<String, u64> = std::collections::HashMap::new();
    let mut reused = 0;
    if let Some(c) = cache.as_deref_mut() {
        c.begin();
    }

    for (i, f) in doc.timeline.iter().enumerate() {
        let t0 = Clock::start();
        let suppressed = doc.suppressed.iter().any(|s| s == f.id());
        let key = feature_key(f, &scope, suppressed, &param_names, &last);
        let before: Vec<(String, u64)> = ctx.bodies.iter().map(|(id, s)| (id.clone(), s.id())).collect();
        let mut cached = false;
        let mut r: Result<Option<String>> = Ok(None);
        if let Some(entry) = cache.as_deref_mut().and_then(|c| c.get(key)) {
            // Replay: drop removed bodies, restore solids with their names, re-add the sketch.
            for id in &entry.removed {
                remove_body(&mut ctx.bodies, id);
            }
            for (id, solid, names) in &entry.outputs {
                let s = solid.clone();
                ctx.names.restore(&s, names);
                match ctx.bodies.iter().position(|(bid, _)| bid == id) {
                    Some(p) => ctx.bodies[p].1 = s,
                    None => ctx.bodies.push((id.clone(), s)),
                }
            }
            if let Some((geo, info)) = &entry.sketch {
                ctx.sketches.insert(info.id.clone(), geo.clone());
                ctx.infos.push(info.clone());
            }
            ctx.resolved = entry.names.clone();
            r = Ok(entry.warning.clone());
            cached = true;
            reused += 1;
        } else if !suppressed {
            r = apply(f, &scope, &mut ctx);
        }
        // Carry face names onto whatever the feature produced (history first, then geometry).
        for (_, solid) in &ctx.bodies {
            ctx.names.carry(solid, f.id(), |c| c);
        }
        // Which objects did this feature write? New or replaced solids, plus its sketch.
        let changed: Vec<String> = ctx.bodies.iter().filter(|(id, s)| before.iter().find(|(b, _)| b == id).map_or(true, |(_, sid)| *sid != s.id())).map(|(id, _)| id.clone()).collect();
        let removed: Vec<String> = before.iter().filter(|(b, _)| !ctx.bodies.iter().any(|(id, _)| id == b)).map(|(b, _)| b.clone()).collect();
        for id in &removed {
            last.remove(id);
        }
        for id in &changed {
            last.insert(id.clone(), key);
        }
        if let Feature::Sketch { id, .. } = f {
            last.insert(id.clone(), key);
        }
        if let (Some(c), false, Ok(w)) = (cache.as_deref_mut(), cached, &r) {
            let outputs = ctx.bodies.iter().filter(|(id, _)| changed.contains(id)).map(|(id, s)| (id.clone(), s.clone(), ctx.names.face_entries(s))).collect();
            let sketch = match f {
                Feature::Sketch { id, .. } => ctx.sketches.get(id).cloned().zip(ctx.infos.last().cloned()),
                _ => None,
            };
            c.put(key, Entry::new(outputs, removed, sketch, w.clone(), ctx.resolved.clone()));
        }
        status.push(FeatureStatus {
            id: f.id().to_string(),
            kind: f.kind().to_string(),
            ok: r.is_ok(),
            message: match &r {
                Ok(_) if suppressed => "suppressed".to_string(),
                Ok(_) => String::new(),
                Err(e) => e.to_string(),
            },
            warning: r.ok().flatten(),
            names: ctx.resolved.take(),
            millis: t0.millis(),
            inputs: deps[i].clone(),
            cached,
        });
    }
    if let Some(c) = cache.as_deref_mut() {
        c.finish();
    }
    Ok(Evaluated { bodies: ctx.bodies, status, sketches: ctx.infos, names: ctx.names, reused })
}

struct Ctx {
    sketches: BTreeMap<String, SketchGeometry>,
    bodies: Vec<(String, Solid)>,
    infos: Vec<SketchInfo>,
    names: Names,
    /// Face names resolved by the feature being applied (reported in its status).
    resolved: Option<Vec<String>>,
}

fn feature_err(f: &Feature, message: impl Into<String>) -> Error {
    Error::Feature { feature: f.id().to_string(), message: message.into() }
}

fn find_body<'a>(bodies: &'a mut Vec<(String, Solid)>, id: &str, f: &Feature) -> Result<&'a mut (String, Solid)> {
    bodies.iter_mut().find(|(bid, _)| bid == id).ok_or_else(|| feature_err(f, format!("unknown body '{id}'")))
}

fn remove_body(bodies: &mut Vec<(String, Solid)>, id: &str) -> Option<Solid> {
    let i = bodies.iter().position(|(bid, _)| bid == id)?;
    Some(bodies.remove(i).1)
}

fn kernel<T>(f: &Feature, what: &str, r: std::result::Result<T, cadrum::Error>) -> Result<T> {
    r.map_err(|e| feature_err(f, format!("{what} failed: {e:?}")))
}

fn apply(f: &Feature, scope: &Scope, ctx: &mut Ctx) -> Result<Option<String>> {
    let mut warning = None;
    match f {
        Feature::Sketch { id, plane, offset, entities, constraints } => {
            let off = scope.eval(offset)?;
            let (basis, face_name) = resolve_plane(plane, off, &ctx.bodies, &ctx.names, f)?;
            let geo = sketch::evaluate_sketch(basis, entities, constraints, scope).map_err(|e| feature_err(f, e))?;
            let mut info = sketch_info(id, plane.clone(), off, &geo);
            info.face_name = face_name;
            ctx.infos.push(info);
            ctx.sketches.insert(id.clone(), geo);
        }
        Feature::Appearance { body, color, .. } => {
            let entry = find_body(&mut ctx.bodies, body, f)?;
            entry.1 = entry.1.clone().color(color.as_str());
        }
        Feature::Extrude { id, sketch, distance, distance2, symmetric, taper, through_all, op, target } => {
            let sk = ctx.sketches.get(sketch).ok_or_else(|| feature_err(f, format!("unknown sketch '{sketch}'")))?;
            let (mut d1, mut d2) = (scope.eval(distance)?, if *symmetric { scope.eval(distance)? } else { scope.eval(distance2)? });
            let taper_deg = scope.eval(taper)?;
            if *through_all {
                // Reach past the far side of the target body on each extruded side.
                let tid = target.as_deref().filter(|_| *op != BoolOp::NewBody).ok_or_else(|| feature_err(f, "through all needs a target body"))?;
                let (_, solid) = ctx.bodies.iter().find(|(bid, _)| bid == tid).ok_or_else(|| feature_err(f, format!("unknown body '{tid}'")))?;
                let [lo, hi] = solid.bounding_box();
                let reach = |sign: f64| (0..8).map(|k| DVec3::new(if k & 1 == 0 { lo.x } else { hi.x }, if k & 2 == 0 { lo.y } else { hi.y }, if k & 4 == 0 { lo.z } else { hi.z })).map(|c| (c - sk.origin).dot(sk.normal) * sign).fold(0.0, f64::max) + 1.0;
                d1 = reach(1.0);
                if *symmetric || d2 > 0.0 {
                    d2 = reach(-1.0);
                }
            }
            let (d1, d2) = (d1.max(0.0) + (-d2).max(0.0), d2.max(0.0) + (-d1).max(0.0));
            if d1 + d2 <= EPS {
                return Err(feature_err(f, "extrude distance must not be zero"));
            }
            let solids = if taper_deg.abs() < 1e-9 {
                let profiles = profile_edges(sk, f)?;
                // One prism from the far side of side 2 to the end of side 1.
                let shift = sk.normal * -d2;
                let shifted: Vec<Vec<Edge>> = profiles.iter().map(|p| p.iter().map(|e| e.clone().translate(shift)).collect()).collect();
                let dir = sk.normal * (d1 + d2);
                build_profiles(&shifted, f, "extrude", |p| Solid::extrude(p, dir), &mut ctx.names, Some((&|m: DVec3| m + dir * 0.5, dir)))?
            } else {
                tapered_extrude(sk, d1, d2, taper_deg.to_radians(), f, &mut ctx.names)?
            };
            place_all(&mut ctx.bodies, id, solids, *op, target.as_deref(), f)?;
        }
        Feature::Revolve { id, sketch, axis_start, axis_end, angle, op, target } => {
            let sk = ctx.sketches.get(sketch).ok_or_else(|| feature_err(f, format!("unknown sketch '{sketch}'")))?;
            let profiles = profile_edges(sk, f)?;
            let a0 = sk.to_world(scope.eval(&axis_start[0])?, scope.eval(&axis_start[1])?);
            let a1 = sk.to_world(scope.eval(&axis_end[0])?, scope.eval(&axis_end[1])?);
            let dir = a1 - a0;
            if dir.length() < EPS {
                return Err(feature_err(f, "axis start and end coincide"));
            }
            let ang = scope.eval(angle)?.to_radians();
            let axis = dir.normalize();
            let half = |m: DVec3| rotate_about(m, a0, axis, ang * 0.5);
            let cap_dir = axis.cross(sk.normal);
            let solids = build_profiles(&profiles, f, "revolve", |p| Solid::revolve(p, a0, axis, ang), &mut ctx.names, Some((&half, cap_dir)))?;
            place_all(&mut ctx.bodies, id, solids, *op, target.as_deref(), f)?;
        }
        Feature::Box { id, size, at, op, target } => {
            let s = scope.eval3(size)?;
            let p = scope.eval3(at)?;
            let solid = Solid::cube(p, p + s);
            ctx.names.name_primitive(&solid, id);
            place(&mut ctx.bodies, id, solid, *op, target.as_deref(), f)?;
        }
        Feature::Cylinder { id, radius, height, at, op, target } => {
            let (r, h, p) = (scope.eval(radius)?, scope.eval(height)?, scope.eval3(at)?);
            let solid = Solid::cylinder(r, DVec3::Z * h).translate(p);
            ctx.names.name_primitive(&solid, id);
            place(&mut ctx.bodies, id, solid, *op, target.as_deref(), f)?;
        }
        Feature::Sphere { id, radius, at, op, target } => {
            let (r, p) = (scope.eval(radius)?, scope.eval3(at)?);
            if r <= 0.0 {
                return Err(feature_err(f, "radius must be positive"));
            }
            let solid = Solid::sphere(r).translate(p);
            ctx.names.name_primitive(&solid, id);
            place(&mut ctx.bodies, id, solid, *op, target.as_deref(), f)?;
        }
        Feature::Torus { id, major, minor, at, op, target } => {
            let (r1, r2, p) = (scope.eval(major)?, scope.eval(minor)?, scope.eval3(at)?);
            if r2 <= 0.0 || r1 <= r2 {
                return Err(feature_err(f, "major radius must exceed the minor radius"));
            }
            let solid = Solid::torus(r1, r2, DVec3::Z).translate(p);
            ctx.names.name_primitive(&solid, id);
            place(&mut ctx.bodies, id, solid, *op, target.as_deref(), f)?;
        }
        Feature::Scale { body, factor, center, .. } => {
            let (k, c) = (scope.eval(factor)?, scope.eval3(center)?);
            if k <= 0.0 {
                return Err(feature_err(f, "scale factor must be positive"));
            }
            let entry = find_body(&mut ctx.bodies, body, f)?;
            entry.1 = entry.1.clone().scale(c, k);
            ctx.names.carry(&entry.1, f.id(), |p| c + (p - c) * k);
        }
        Feature::Split { id, body, plane, offset } => {
            let [_, _, n] = plane.basis();
            let origin = n * scope.eval(offset)?;
            let entry = find_body(&mut ctx.bodies, body, f)?;
            let half = Solid::half_space(origin, n);
            let mut keep = kernel(f, "split", (&entry.1 * &half).build());
            let mut other = kernel(f, "split", (&entry.1 - &half).build());
            // Whichever side the kernel's half space covers, the body id keeps the +normal part.
            if let Ok(k) = &keep {
                if (k.center() - origin).dot(n) < 0.0 {
                    std::mem::swap(&mut keep, &mut other);
                }
            }
            match (keep, other) {
                (Ok(a), Ok(b)) if a.volume() > EPS && b.volume() > EPS => {
                    ctx.names.carry(&a, f.id(), |p| p);
                    ctx.names.carry(&b, f.id(), |p| p);
                    entry.1 = a;
                    ctx.bodies.push((id.clone(), b));
                }
                _ => return Err(feature_err(f, "the plane does not cut the body")),
            }
        }
        Feature::OffsetFace { body, faces, distance, .. } => {
            let d = scope.eval(distance)?;
            if faces.is_empty() {
                return Err(feature_err(f, "pick at least one face"));
            }
            let entry = find_body(&mut ctx.bodies, body, f)?;
            let mut picked: Vec<&cadrum::Face> = Vec::new();
            let mut resolved = Vec::new();
            let mut rebound = false;
            for fr in faces {
                let (fc, by_point) = find_face(&entry.1, fr, &ctx.names).ok_or_else(|| feature_err(f, "no face near the picked point"))?;
                rebound |= by_point && fr.name.is_some();
                resolved.push(ctx.names.get(fc).unwrap_or("").to_string());
                if !picked.iter().any(|p| p.id() == fc.id()) {
                    picked.push(fc);
                }
            }
            if rebound {
                warning = Some("a picked face was re-bound by position".to_string());
            }
            ctx.resolved = Some(resolved);
            entry.1 = kernel(f, "offset face (the offset self-intersects)", entry.1.offset(d, picked, 1e-6))?;
        }
        Feature::Delete { body, .. } => {
            remove_body(&mut ctx.bodies, body).ok_or_else(|| feature_err(f, format!("unknown body '{body}'")))?;
        }
        Feature::Import { id, data, at, op, target, .. } => {
            let p = scope.eval3(at)?;
            let bytes = crate::b64::decode(data).ok_or_else(|| feature_err(f, "data is not base64"))?;
            let mut reader = bytes.as_slice();
            let solids = kernel(f, "read brep", Solid::read_brep(&mut reader))?;
            if solids.is_empty() {
                return Err(feature_err(f, "the file has no solid"));
            }
            let mut placed = Vec::new();
            for (n, s) in solids.into_iter().enumerate() {
                let s = s.translate(p);
                let name = if n == 0 { id.clone() } else { format!("{id}_{n}") };
                ctx.names.carry(&s, &name, |c| c);
                placed.push(s);
            }
            place_all(&mut ctx.bodies, id, placed, *op, target.as_deref(), f)?;
        }
        Feature::Fillet { body, radius, edges, .. } => {
            let r = scope.eval(radius)?;
            let entry = find_body(&mut ctx.bodies, body, f)?;
            let (sel, w) = select_edges(&entry.1, edges, &ctx.names);
            warning = w;
            if sel.is_empty() {
                return Err(feature_err(f, "edge selector matched nothing"));
            }
            entry.1 = kernel(f, "fillet (try a smaller radius)", entry.1.fillet_edges(r, sel))?;
        }
        Feature::Chamfer { body, distance, edges, .. } => {
            let d = scope.eval(distance)?;
            let entry = find_body(&mut ctx.bodies, body, f)?;
            let (sel, w) = select_edges(&entry.1, edges, &ctx.names);
            warning = w;
            if sel.is_empty() {
                return Err(feature_err(f, "edge selector matched nothing"));
            }
            entry.1 = kernel(f, "chamfer", entry.1.chamfer_edges(d, sel))?;
        }
        Feature::Shell { body, thickness, open, .. } => {
            let t = scope.eval(thickness)?;
            let entry = find_body(&mut ctx.bodies, body, f)?;
            let faces = select_faces(&entry.1, *open);
            entry.1 = kernel(f, "shell", entry.1.shell(t, faces))?;
        }
        Feature::Combine { target, tool, op, keep_tool, .. } => {
            let tool_solid = if *keep_tool {
                find_body(&mut ctx.bodies, tool, f)?.1.clone()
            } else {
                remove_body(&mut ctx.bodies, tool).ok_or_else(|| feature_err(f, format!("unknown tool body '{tool}'")))?
            };
            let entry = find_body(&mut ctx.bodies, target, f)?;
            let parts = kernel(f, "boolean", boolean(&entry.1, &tool_solid, *op))?;
            replace_with_parts(&mut ctx.bodies, target, f.id(), parts, f)?;
        }
        Feature::Move { body, translate, rotate_z, .. } => {
            let t = scope.eval3(translate)?;
            let a = scope.eval(rotate_z)?.to_radians();
            let entry = find_body(&mut ctx.bodies, body, f)?;
            entry.1 = entry.1.clone().rotate_z(a).translate(t);
            ctx.names.carry(&entry.1, f.id(), |c| rotate_about(c, DVec3::ZERO, DVec3::Z, a) + t);
        }
        Feature::Mirror { id, body, plane, offset, join } => {
            let [_, _, n] = plane.basis();
            let origin = n * scope.eval(offset)?;
            let entry = find_body(&mut ctx.bodies, body, f)?;
            let mirrored = entry.1.clone().mirror(origin, n);
            ctx.names.carry(&mirrored, f.id(), |c| c - n * (2.0 * (c - origin).dot(n)));
            if *join {
                let parts = kernel(f, "mirror join", Solid::boolean_build(&(&entry.1 + &mirrored)))?;
                replace_with_parts(&mut ctx.bodies, body, id, parts, f)?;
            } else {
                ctx.bodies.push((id.clone(), mirrored));
            }
        }
        Feature::Pattern { id, body, count, step, join } => {
            let (nx, ny) = (scope.eval(&count[0])?.max(1.0) as usize, scope.eval(&count[1])?.max(1.0) as usize);
            let (dx, dy) = (scope.eval(&step[0])?, scope.eval(&step[1])?);
            let base = find_body(&mut ctx.bodies, body, f)?.1.clone();
            let copies: Vec<Solid> = (0..nx).flat_map(|i| (0..ny).map(move |j| (i, j))).map(|(i, j)| base.clone().translate(DVec3::new(dx * i as f64, dy * j as f64, 0.0))).collect();
            for (k, c) in copies.iter().enumerate() {
                let (i, j) = (k / ny, k % ny);
                let t = DVec3::new(dx * i as f64, dy * j as f64, 0.0);
                ctx.names.carry(c, f.id(), |p| p + t);
            }
            // Fuse everything at once; touching copies merge, disjoint ones stay separate solids.
            let union = copies.iter().skip(1).fold(cadrum::Boolean::from(&copies[0]), |acc, c| acc + c);
            finish_pattern(ctx, f, body, id, join.as_deref(), union)?;
        }
        Feature::CircularPattern { id, body, count, angle, center, join } => {
            let n = scope.eval(count)?.max(1.0) as usize;
            let total = scope.eval(angle)?.to_radians();
            let c = DVec3::new(scope.eval(&center[0])?, scope.eval(&center[1])?, 0.0);
            let base = find_body(&mut ctx.bodies, body, f)?.1.clone();
            // A full turn spreads n copies over 360/n each; a partial angle spreads them over angle/(n-1).
            let step = if (total - std::f64::consts::TAU).abs() < 1e-9 { total / n as f64 } else if n > 1 { total / (n - 1) as f64 } else { 0.0 };
            let copies: Vec<Solid> = (0..n).map(|i| base.clone().rotate(c, DVec3::Z, step * i as f64)).collect();
            for (i, s) in copies.iter().enumerate() {
                ctx.names.carry(s, f.id(), |p| rotate_about(p, c, DVec3::Z, step * i as f64));
            }
            let union = copies.iter().skip(1).fold(cadrum::Boolean::from(&copies[0]), |acc, s| acc + s);
            finish_pattern(ctx, f, body, id, join.as_deref(), union)?;
        }
        Feature::Hole { body, at, diameter, depth, kind, head_diameter, head_depth, head_angle, .. } => {
            let p = scope.eval3(at)?;
            let (d, h) = (scope.eval(diameter)?, scope.eval(depth)?);
            if d <= 0.0 || h <= 0.0 {
                return Err(feature_err(f, "diameter and depth must be positive"));
            }
            // Start slightly above the surface so the cut always opens the top face.
            let lift = 0.01 * d.max(1.0);
            let mut tool = Solid::cylinder(d / 2.0, DVec3::Z * -(h + lift)).translate(p + DVec3::Z * lift);
            match kind {
                HoleKind::Simple => {}
                HoleKind::Counterbore => {
                    let (hd, hh) = (scope.eval(head_diameter)?, scope.eval(head_depth)?);
                    let head = Solid::cylinder(hd / 2.0, DVec3::Z * -(hh + lift)).translate(p + DVec3::Z * lift);
                    tool = kernel(f, "counterbore", (&tool + &head).build())?;
                }
                HoleKind::Countersink => {
                    let (hd, ang) = (scope.eval(head_diameter)?, scope.eval(head_angle)?.to_radians());
                    let hh = (hd / 2.0) / (ang / 2.0).tan();
                    let head = Solid::cone(hd / 2.0 + lift * (ang / 2.0).tan(), 0.0, DVec3::Z * -(hh + lift)).translate(p + DVec3::Z * lift);
                    tool = kernel(f, "countersink", (&tool + &head).build())?;
                }
            }
            ctx.names.name_primitive(&tool, f.id());
            let entry = find_body(&mut ctx.bodies, body, f)?;
            entry.1 = kernel(f, "hole cut", (&entry.1 - &tool).build())?;
        }
        Feature::Sweep { id, sketch, path, orient, op, target } => {
            let sk = ctx.sketches.get(sketch).ok_or_else(|| feature_err(f, format!("unknown sketch '{sketch}'")))?;
            let pk = ctx.sketches.get(path).ok_or_else(|| feature_err(f, format!("unknown path sketch '{path}'")))?;
            let profiles = profile_edges(sk, f)?;
            let chain = pk.open.first().or_else(|| pk.loops.first()).ok_or_else(|| feature_err(f, "path sketch has no entities"))?;
            let spine = chain_edges(pk, chain, f)?;
            let o = match orient {
                SweepOrient::Frenet => cadrum::ProfileOrient::Torsion,
                SweepOrient::Fixed => cadrum::ProfileOrient::Fixed,
            };
            let solids = build_profiles(&profiles, f, "sweep", |p| Solid::sweep(p, &spine, o), &mut ctx.names, None)?;
            place_all(&mut ctx.bodies, id, solids, *op, target.as_deref(), f)?;
        }
        Feature::Loft { id, sketches, ruled, op, target } => {
            if sketches.len() < 2 {
                return Err(feature_err(f, "loft needs at least two sketches"));
            }
            let mut sections: Vec<Vec<Edge>> = Vec::new();
            for sid in sketches {
                let sk = ctx.sketches.get(sid).ok_or_else(|| feature_err(f, format!("unknown sketch '{sid}'")))?;
                let outer = sk.loops.first().ok_or_else(|| feature_err(f, format!("sketch '{sid}' has no closed profile")))?;
                sections.push(chain_edges(sk, outer, f)?);
            }
            let solid = kernel(f, "loft", Solid::loft(sections.iter().map(|s| s.iter()), *ruled))?;
            place(&mut ctx.bodies, id, solid, *op, target.as_deref(), f)?;
        }
    }
    Ok(warning)
}

/// Face of `solid` picked by persistent name, else the face nearest the stored point.
/// The flag says the point was used.
fn find_face<'a>(solid: &'a Solid, fr: &crate::doc::FaceRef, names: &Names) -> Option<(&'a cadrum::Face, bool)> {
    if let Some(name) = &fr.name {
        if let Some(fc) = solid.iter_face().find(|fc| names.get(fc) == Some(name.as_str())) {
            return Some((fc, false));
        }
    }
    let p = DVec3::from_array(fr.point);
    let [lo, hi] = solid.bounding_box();
    let tol = (hi - lo).length() * 0.05;
    solid.iter_face().map(|fc| (fc, (fc.project(p).0 - p).length())).filter(|(_, d)| *d <= tol).min_by(|a, b| a.1.total_cmp(&b.1)).map(|(fc, _)| (fc, true))
}

/// Extrude with a draft angle: each loop is lofted (ruled) to its 2D offset copy at the far
/// end, holes are cut from the outer solid, and a second side is built the same way. Faces
/// are named `{feature}/f{k}` like other lofts.
fn tapered_extrude(sk: &SketchGeometry, d1: f64, d2: f64, taper: f64, f: &Feature, names: &mut Names) -> Result<Vec<Solid>> {
    if sk.loops.is_empty() {
        return Err(feature_err(f, "sketch has no closed profile"));
    }
    let side = |dist: f64, sign: f64, names: &mut Names, suffix: &str| -> Result<Option<Solid>> {
        if dist <= EPS {
            return Ok(None);
        }
        let t = dist * taper.tan();
        let lift = sk.normal * (sign * dist);
        let loft = |lp: &Vec<Geo2>, t: f64| -> Result<Solid> {
            let off = sketch::offset_loop(lp, t).ok_or_else(|| feature_err(f, "taper collapses the profile; use a smaller angle or distance"))?;
            let a = chain_edges(sk, lp, f)?;
            let b: Vec<Edge> = chain_edges(sk, &off, f)?.into_iter().map(|e| e.translate(lift)).collect();
            // Keep the section order along the extrude direction so the loft normals point outward.
            let sections: Vec<&Vec<Edge>> = if sign > 0.0 { vec![&a, &b] } else { vec![&b, &a] };
            kernel(f, "tapered extrude", Solid::loft(sections.iter().map(|s| s.iter()), true))
        };
        let mut parts = Vec::new();
        for (outer, holes) in sketch::profiles(&sk.loops) {
            let mut solid = loft(&sk.loops[outer], t)?;
            for h in holes {
                let hole = loft(&sk.loops[h], -t)?;
                solid = kernel(f, "tapered extrude", (&solid - &hole).build())?;
            }
            parts.push(solid);
        }
        let solid = if parts.len() == 1 {
            parts.pop().unwrap()
        } else {
            let union = parts.iter().skip(1).fold(cadrum::Boolean::from(&parts[0]), |acc, s| acc + s);
            let mut all = kernel(f, "tapered extrude", Solid::boolean_build(&union))?;
            all.pop().ok_or_else(|| feature_err(f, "tapered extrude produced no solid"))?
        };
        names.carry(&solid, &format!("{}{suffix}", f.id()), |c| c);
        Ok(Some(solid))
    };
    let up = side(d1, 1.0, names, "")?;
    let down = side(d2, -1.0, names, "/b")?;
    let solid = match (up, down) {
        (Some(a), Some(b)) => kernel(f, "tapered extrude", (&a + &b).build())?,
        (Some(a), None) | (None, Some(a)) => a,
        (None, None) => return Err(feature_err(f, "extrude distance must be positive")),
    };
    names.carry(&solid, f.id(), |c| c);
    Ok(vec![solid])
}

/// Basis of a named plane with offset, or of the nearest planar face of a body.
fn resolve_plane(plane: &PlaneRef, offset: f64, bodies: &[(String, Solid)], names: &Names, f: &Feature) -> Result<(Basis, Option<String>)> {
    match plane {
        PlaneRef::Named(p) => {
            let [u, v, n] = p.basis();
            Ok((Basis { origin: n * offset, u, v, normal: n }, None))
        }
        PlaneRef::Face { face } => {
            let (_, solid) = bodies.iter().find(|(id, _)| *id == face.body).ok_or_else(|| feature_err(f, format!("unknown body '{}'", face.body)))?;
            let p = DVec3::from_array(face.point);
            let planar = |fc: &cadrum::Face| matches!(fc.surface(), Some(sf) if matches!(sf.kind, cadrum::SurfaceKind::Plane));
            // Persistent name first; the point is the fallback for documents without one.
            let mut best: Option<(f64, DVec3, DVec3, String)> = None;
            if let Some(name) = &face.name {
                if let Some(fc) = solid.iter_face().find(|fc| planar(fc) && names.get(fc) == Some(name.as_str())) {
                    let (_, n) = fc.project(fc.center());
                    if n.length() > 0.5 {
                        best = Some((0.0, fc.center(), n.normalize(), name.clone()));
                    }
                }
            }
            if best.is_none() {
                for fc in solid.iter_face() {
                    if !planar(fc) {
                        continue;
                    }
                    let (hit, n) = fc.project(p);
                    let d = (hit - p).length();
                    if n.length() > 0.5 && best.as_ref().map_or(true, |(bd, _, _, _)| d < *bd) {
                        best = Some((d, fc.center(), n.normalize(), names.get(fc).unwrap_or("?").to_string()));
                    }
                }
            }
            let (d, center, n, name) = best.ok_or_else(|| feature_err(f, "body has no planar face"))?;
            let [lo, hi] = solid.bounding_box();
            if d > (hi - lo).length() * 0.05 {
                return Err(feature_err(f, format!("no planar face near the picked point (nearest is {d:.2} mm away)")));
            }
            let seed = if n.cross(DVec3::X).length() > 1e-3 { DVec3::X } else { DVec3::Y };
            let u = (seed - n * seed.dot(n)).normalize();
            let v = n.cross(u);
            Ok((Basis { origin: center, u, v, normal: n }, Some(name)))
        }
    }
}

/// Rotate `p` about the axis through `origin` with direction `axis` (unit) by `angle` radians.
fn rotate_about(p: DVec3, origin: DVec3, axis: DVec3, angle: f64) -> DVec3 {
    let v = p - origin;
    let (s, c) = angle.sin_cos();
    origin + v * c + axis.cross(v) * s + axis * (axis.dot(v) * (1.0 - c))
}

/// Pattern result: either fused into `join` (source consumed) or kept under the source body.
fn finish_pattern(ctx: &mut Ctx, f: &Feature, body: &str, id: &str, join: Option<&str>, union: cadrum::Boolean<Solid>) -> Result<()> {
    match join {
        Some(target) => {
            let copies = kernel(f, "pattern", Solid::boolean_build(&union))?;
            remove_body(&mut ctx.bodies, body);
            let entry = find_body(&mut ctx.bodies, target, f)?;
            let all = copies.iter().fold(cadrum::Boolean::from(&entry.1), |acc, c| acc + c);
            let parts = kernel(f, "pattern join", Solid::boolean_build(&all))?;
            replace_with_parts(&mut ctx.bodies, target, id, parts, f)
        }
        None => {
            let parts = kernel(f, "pattern", Solid::boolean_build(&union))?;
            replace_with_parts(&mut ctx.bodies, body, id, parts, f)
        }
    }
}

/// Put the first result solid back under `body`; extra disjoint solids become `{id}_{n}`.
fn replace_with_parts(bodies: &mut Vec<(String, Solid)>, body: &str, id: &str, parts: Vec<Solid>, f: &Feature) -> Result<()> {
    let mut it = parts.into_iter();
    let first = it.next().ok_or_else(|| feature_err(f, "operation produced no solid"))?;
    find_body(bodies, body, f)?.1 = first;
    for (n, s) in it.enumerate() {
        bodies.push((format!("{id}_{}", n + 1), s));
    }
    Ok(())
}

/// Place several result solids: NewBody registers `id`, `id_1`, ...; other ops apply each to the target.
fn place_all(bodies: &mut Vec<(String, Solid)>, id: &str, solids: Vec<Solid>, op: BoolOp, target: Option<&str>, f: &Feature) -> Result<()> {
    for (n, s) in solids.into_iter().enumerate() {
        let name = if n == 0 { id.to_string() } else { format!("{id}_{n}") };
        place(bodies, &name, s, op, target, f)?;
    }
    Ok(())
}

fn place(bodies: &mut Vec<(String, Solid)>, id: &str, solid: Solid, op: BoolOp, target: Option<&str>, f: &Feature) -> Result<()> {
    match op {
        BoolOp::NewBody => bodies.push((id.to_string(), solid)),
        _ => {
            let tid = target.ok_or_else(|| feature_err(f, "op needs a target body"))?;
            let entry = find_body(bodies, tid, f)?;
            let parts = kernel(f, "boolean", boolean(&entry.1, &solid, op))?;
            replace_with_parts(bodies, tid, id, parts, f)?;
        }
    }
    Ok(())
}

/// Boolean result as a list of solids: one for the usual case, several when a cut or
/// intersection splits the body (the caller keeps them all, largest first).
fn boolean(a: &Solid, b: &Solid, op: BoolOp) -> std::result::Result<Vec<Solid>, cadrum::Error> {
    let r = match op {
        BoolOp::NewBody | BoolOp::Join => Solid::boolean_build(&(a + b)),
        BoolOp::Cut => Solid::boolean_build(&(a - b)),
        BoolOp::Intersect => Solid::boolean_build(&(a * b)),
    }?;
    let mut parts: Vec<Solid> = r.into_iter().filter(|s| s.volume() > EPS).collect();
    parts.sort_by(|x, y| y.volume().total_cmp(&x.volume()));
    Ok(parts)
}

/// Kernel edges per profile (outer loop first, then its holes); one entry per disjoint profile.
fn profile_edges(sk: &SketchGeometry, f: &Feature) -> Result<Vec<Vec<Edge>>> {
    if sk.loops.is_empty() {
        return Err(feature_err(f, if sk.open.is_empty() { "sketch has no closed profile" } else { "sketch profile is not closed" }));
    }
    let mut out = Vec::new();
    for (outer, holes) in sketch::profiles(&sk.loops) {
        let mut edges = chain_edges(sk, &sk.loops[outer], f)?;
        for h in holes {
            edges.extend(chain_edges(sk, &sk.loops[h], f)?);
        }
        out.push(edges);
    }
    Ok(out)
}

/// Build one solid per profile and fuse them; disjoint profiles stay separate solids.
/// Faces are named `{feature}/e{j}` after the profile edge that grew them (`probe` maps an edge
/// midpoint onto its lateral face) and `{feature}/start|end` for the caps; sweeps and lofts fall
/// back to `{feature}/f{k}`.
fn build_profiles(
    profiles: &[Vec<Edge>],
    f: &Feature,
    what: &str,
    mut make: impl FnMut(&[Edge]) -> std::result::Result<Solid, cadrum::Error>,
    names: &mut Names,
    probe: Option<(&dyn Fn(DVec3) -> DVec3, DVec3)>,
) -> Result<Vec<Solid>> {
    let mut solids = Vec::new();
    for (i, p) in profiles.iter().enumerate() {
        let solid = kernel(f, what, make(p))?;
        let prefix = if i == 0 { f.id().to_string() } else { format!("{}/p{i}", f.id()) };
        match probe {
            Some((probe, dir)) => names.name_from_profile(&solid, &prefix, p, probe, dir),
            None => names.carry(&solid, &prefix, |c| c),
        }
        solids.push(solid);
    }
    if solids.len() == 1 {
        return Ok(solids);
    }
    let union = solids.iter().skip(1).fold(cadrum::Boolean::from(&solids[0]), |acc, s| acc + s);
    kernel(f, what, Solid::boolean_build(&union))
}

/// Kernel edges of one loop or open chain, in world coordinates.
fn chain_edges(sk: &SketchGeometry, lp: &[Geo2], f: &Feature) -> Result<Vec<Edge>> {
    let mut out = Vec::new();
    {
        for g in lp {
            let edges = match g {
                Geo2::Rect { x, y, w, h } => Edge::polygon(&[sk.to_world(*x, *y), sk.to_world(x + w, *y), sk.to_world(x + w, y + h), sk.to_world(*x, y + h)]),
                Geo2::Polygon(pts) => Edge::polygon(&pts.iter().map(|p| sk.to_world(p[0], p[1])).collect::<Vec<_>>()),
                Geo2::Circle { c, r } => Edge::circle(*r, sk.normal).map(|e| vec![e.translate(sk.to_world(c[0], c[1]))]),
                Geo2::Line(a, b) => Edge::line(sk.to_world(a[0], a[1]), sk.to_world(b[0], b[1])).map(|e| vec![e]),
                Geo2::Arc(a, m, b) => Edge::arc_3pts(sk.to_world(a[0], a[1]), sk.to_world(m[0], m[1]), sk.to_world(b[0], b[1])).map(|e| vec![e]),
            };
            out.extend(kernel(f, "sketch entity", edges)?);
        }
    }
    Ok(out)
}

fn sketch_info(id: &str, plane: PlaneRef, offset: f64, g: &SketchGeometry) -> SketchInfo {
    let poly = |lp: &Vec<Geo2>| -> Vec<[f64; 3]> { sketch::polyline(lp).iter().map(|p| g.to_world(p[0], p[1]).to_array()).collect() };
    SketchInfo {
        face_name: None,
        id: id.to_string(),
        plane,
        offset,
        origin: g.origin.to_array(),
        u: g.u.to_array(),
        v: g.v.to_array(),
        normal: g.normal.to_array(),
        loops: g.loops.iter().map(poly).collect(),
        open: g.open.iter().map(poly).collect(),
        entities: g.entities.iter().map(sketch::geo_json).collect(),
        dof: g.dof,
        underconstrained: g.underconstrained.clone(),
    }
}

fn z_range(s: &Solid) -> (f64, f64) {
    let [lo, hi] = s.bounding_box();
    (lo.z, hi.z)
}

pub(crate) fn mid_key(e: &Edge) -> [i64; 3] {
    edge_mid(&edge_points(e)).to_array().map(|x| (x * 1e4).round() as i64)
}

/// Seam edges (a cylinder's generatrix line) border a single face, twice;
/// every other edge of a closed solid borders two faces. Filleting a seam is
/// meaningless and makes OCCT fail, so selectors skip them. Edges are keyed by
/// midpoint because `Edge::id` is not stable across face iteration.
fn seam_edge_keys(s: &Solid) -> std::collections::HashSet<[i64; 3]> {
    let mut faces_per_edge: std::collections::HashMap<[i64; 3], usize> = std::collections::HashMap::new();
    for face in s.iter_face() {
        let keys: std::collections::HashSet<[i64; 3]> = face.iter_edge().map(mid_key).collect();
        for k in keys {
            *faces_per_edge.entry(k).or_default() += 1;
        }
    }
    faces_per_edge.into_iter().filter(|(_, n)| *n < 2).map(|(k, _)| k).collect()
}

fn select_edges<'a>(s: &'a Solid, sel: &EdgeSelect, names: &Names) -> (Vec<&'a Edge>, Option<String>) {
    let seams = seam_edge_keys(s);
    match sel {
        EdgeSelect::Named(named) => {
            let (zlo, zhi) = z_range(s);
            let edges: Vec<&Edge> = s.iter_edge()
                .filter(|e| !seams.contains(&mid_key(e)))
                .filter(|e| {
                    let (a, b) = (e.start_point(), e.end_point());
                    match named {
                        NamedSelect::All => true,
                        NamedSelect::Top => (a.z - zhi).abs() < EPS && (b.z - zhi).abs() < EPS,
                        NamedSelect::Bottom => (a.z - zlo).abs() < EPS && (b.z - zlo).abs() < EPS,
                        NamedSelect::Vertical => (a.x - b.x).abs() < EPS && (a.y - b.y).abs() < EPS && (a.z - b.z).abs() > EPS,
                    }
                })
                .collect();
            (edges, None)
        }
        EdgeSelect::Pick { pick, names: picked_names } => {
            let [lo, hi] = s.bounding_box();
            let tol = (hi - lo).length() * 0.02;
            let edge_names = names.edge_names(s);
            let all: Vec<&Edge> = s.iter_edge().collect();
            let edges: Vec<(&Edge, DVec3)> = s.iter_edge().filter(|e| !seams.contains(&mid_key(e))).map(|e| (e, edge_mid(&edge_points(e)))).collect();
            let mut out: Vec<&Edge> = Vec::new();
            let mut rebound = 0;
            let mut lost = 0;
            let mut by_position = 0;
            for (k, p) in pick.iter().enumerate() {
                // Persistent name first: exact, no tolerance, survives upstream edits that move the edge.
                let wanted = picked_names.as_ref().and_then(|v| v.get(k)).filter(|n| !n.is_empty());
                if let Some(name) = wanted {
                    if let Some(i) = edge_names.iter().position(|n| n == name) {
                        if !out.iter().any(|x| mid_key(x) == mid_key(all[i])) {
                            out.push(all[i]);
                        }
                        continue;
                    }
                    by_position += 1;
                }
                let p = DVec3::from_array(*p);
                match edges.iter().filter(|(_, m)| (*m - p).length() < tol).min_by(|a, b| (a.1 - p).length().partial_cmp(&(b.1 - p).length()).unwrap()) {
                    Some((e, m)) => {
                        if (*m - p).length() > tol * 0.05 {
                            rebound += 1;
                        }
                        if !out.iter().any(|x| mid_key(x) == mid_key(e)) {
                            out.push(e);
                        }
                    }
                    None => lost += 1,
                }
            }
            let mut notes = Vec::new();
            if by_position > 0 {
                notes.push(format!("{by_position} named edge(s) not found and re-bound by position"));
            }
            if rebound > 0 {
                notes.push(format!("{rebound} picked edge(s) moved and were re-bound to the nearest edge"));
            }
            if lost > 0 {
                notes.push(format!("{lost} picked edge(s) no longer exist and were skipped"));
            }
            (out, if notes.is_empty() { None } else { Some(notes.join("; ")) })
        }
    }
}

fn select_faces<'a>(s: &'a Solid, sel: NamedSelect) -> Vec<&'a cadrum::Face> {
    let (zlo, zhi) = z_range(s);
    s.iter_face()
        .filter(|face| {
            let c = face.center();
            match sel {
                NamedSelect::All => true,
                NamedSelect::Top => (c.z - zhi).abs() < EPS,
                NamedSelect::Bottom => (c.z - zlo).abs() < EPS,
                NamedSelect::Vertical => c.z > zlo + EPS && c.z < zhi - EPS,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::Expr;

    fn bracket() -> Document {
        Document::from_json(include_str!("../../../examples/bracket.cadrust.json")).unwrap()
    }

    #[test]
    fn bracket_evaluates_without_errors() {
        let ev = evaluate(&bracket(), &BTreeMap::new()).unwrap();
        assert!(!ev.has_errors(), "{:?}", ev.status);
        assert_eq!(ev.bodies.len(), 1);
        assert!(ev.info()[0].volume > 0.0);
        assert!(!ev.edges().is_empty());
        assert_eq!(ev.sketches.len(), 2);
    }

    #[test]
    fn parameter_override_changes_volume() {
        let doc = bracket();
        let a = evaluate(&doc, &BTreeMap::new()).unwrap().info()[0].volume;
        let mut o = BTreeMap::new();
        o.insert("width".to_string(), "80".to_string());
        let b = evaluate(&doc, &o).unwrap().info()[0].volume;
        assert!(b > a);
    }

    #[test]
    fn bad_expression_is_reported() {
        let mut doc = bracket();
        doc.parameters.insert("width".into(), Expr::Text("nope +".into()));
        assert!(evaluate(&doc, &BTreeMap::new()).is_err());
    }

    #[test]
    fn suppressed_feature_is_skipped() {
        let mut doc = bracket();
        doc.suppressed.push("round_corners".into());
        let ev = evaluate(&doc, &BTreeMap::new()).unwrap();
        assert!(!ev.has_errors(), "{:?}", ev.status);
        assert_eq!(ev.status[2].message, "suppressed");
    }

    #[test]
    fn line_arc_profile_and_pick_fillet() {
        let doc = Document::from_json(include_str!("../../../examples/sketch_profile.cadrust.json")).unwrap();
        let ev = evaluate(&doc, &BTreeMap::new()).unwrap();
        assert!(!ev.has_errors(), "{:?}", ev.status);
        // block (filleted), ring, and the disjoint pattern copy
        assert_eq!(ev.bodies.len(), 3);
        assert_eq!(ev.sketches[0].loops.len(), 2);
    }

    #[test]
    fn holes_patterns_sweep_and_loft() {
        let doc = Document::from_json(include_str!("../../../examples/phase2.cadrust.json")).unwrap();
        let ev = evaluate(&doc, &BTreeMap::new()).unwrap();
        assert!(!ev.has_errors(), "{:?}", ev.status);
        let ids: Vec<&str> = ev.bodies.iter().map(|(id, _)| id.as_str()).collect();
        assert_eq!(ids, ["plate", "pipe", "funnel"]);
        assert!(ev.sketches.iter().all(|s| !s.entities.is_empty()));
        // six pegs fused into the plate: more faces than the drilled plate alone
        assert!(ev.info()[0].faces > 13);
    }

    #[test]
    fn constraints_face_sketches_and_appearance() {
        let doc = Document::from_json(include_str!("../../../examples/phase3.cadrust.json")).unwrap();
        let ev = evaluate(&doc, &BTreeMap::new()).unwrap();
        assert!(!ev.has_errors(), "{:?}", ev.status);
        let outline = &ev.sketches[0];
        assert_eq!(outline.dof, 0);
        let x1 = outline.entities[0]["x1"].as_f64().unwrap();
        assert!((x1 - 50.0).abs() < 1e-6, "{x1}");
        assert!((ev.sketches[1].origin[2] - 10.0).abs() < 1e-6);
        assert!((ev.sketches[2].normal[0] - 1.0).abs() < 1e-6);
    }

    /// A fillet picked on the top front edge of a plate keeps working when the plate gets
    /// taller: the persistent name finds the moved edge, the stale pick point alone does not.
    fn run(json: &str) -> Evaluated {
        let doc = Document::from_json(json).unwrap();
        evaluate(&doc, &BTreeMap::new()).unwrap()
    }
    fn vol(ev: &Evaluated, id: &str) -> f64 {
        ev.info().iter().find(|b| b.id == id).map(|b| b.volume).unwrap_or(0.0)
    }
    const RECT: &str = r#"{ "type": "Sketch", "id": "sk", "plane": "XY", "entities": [ { "type": "Rect", "x": 0, "y": 0, "w": 20, "h": 10 }, { "type": "Circle", "cx": 10, "cy": 5, "r": 2 } ] }"#;

    #[test]
    fn extrude_two_sides_symmetric_and_taper() {
        let ev = run(&format!(r#"{{ "timeline": [ {RECT},
            {{ "type": "Extrude", "id": "a", "sketch": "sk", "distance": 6, "distance2": 4 }},
            {{ "type": "Extrude", "id": "b", "sketch": "sk", "distance": 5, "symmetric": true }},
            {{ "type": "Extrude", "id": "c", "sketch": "sk", "distance": 10, "taper": -10 }},
            {{ "type": "Extrude", "id": "d", "sketch": "sk", "distance": 10, "taper": 10, "distance2": 3 }} ] }}"#));
        assert!(!ev.has_errors(), "{:?}", ev.status);
        let info = ev.info();
        let a = &info[0];
        assert!((a.bbox_min[2] + 4.0).abs() < 1e-6 && (a.bbox_max[2] - 6.0).abs() < 1e-6);
        assert!((a.volume - (200.0 - std::f64::consts::PI * 4.0) * 10.0).abs() < 1e-3);
        assert!((info[1].bbox_min[2] + 5.0).abs() < 1e-6 && (info[1].bbox_max[2] - 5.0).abs() < 1e-6);
        // A negative taper shrinks the outer loop and grows the hole: less material than the prism.
        let straight = (200.0 - std::f64::consts::PI * 4.0) * 10.0;
        assert!(info[2].volume < straight * 0.95 && info[2].volume > straight * 0.5, "{}", info[2].volume);
        assert!(info[3].volume > straight && (info[3].bbox_min[2] + 3.0).abs() < 1e-6);
        // tapered side faces are not vertical
        assert!(info[2].faces >= 6);
    }

    #[test]
    fn extrude_through_all_cuts_the_whole_target() {
        let ev = run(&format!(r#"{{ "timeline": [
            {{ "type": "Box", "id": "plate", "size": [40, 40, 30], "at": [-10, -15, -7] }},
            {RECT},
            {{ "type": "Extrude", "id": "slot", "sketch": "sk", "distance": 1, "through_all": true, "symmetric": true, "op": "Cut", "target": "plate" }} ] }}"#));
        assert!(!ev.has_errors(), "{:?}", ev.status);
        // The cut goes through the plate; the circle's column survives as a separate body.
        assert_eq!(ev.bodies.len(), 2);
        assert!((vol(&ev, "plate") - (40.0 * 40.0 * 30.0 - 200.0 * 30.0)).abs() < 1e-3, "{}", vol(&ev, "plate"));
        assert!((vol(&ev, "slot_1") - std::f64::consts::PI * 4.0 * 30.0).abs() < 1e-3, "{}", vol(&ev, "slot_1"));
    }

    #[test]
    fn primitives_with_booleans_scale_split_offset_delete() {
        let ev = run(r#"{ "timeline": [
            { "type": "Box", "id": "b", "size": [10, 10, 10] },
            { "type": "Sphere", "id": "s", "radius": 3, "at": [10, 5, 5], "op": "Cut", "target": "b" },
            { "type": "Torus", "id": "t", "major": 10, "minor": 2, "at": [50, 0, 0] },
            { "type": "Cylinder", "id": "c", "radius": 2, "height": 5, "at": [5, 5, 10], "op": "Join", "target": "b" },
            { "type": "Scale", "id": "sc", "body": "t", "factor": 2, "center": [50, 0, 0] },
            { "type": "Box", "id": "k", "size": [10, 10, 10], "at": [100, 0, 0] },
            { "type": "Split", "id": "k_low", "body": "k", "plane": "XY", "offset": 4 },
            { "type": "Box", "id": "o", "size": [10, 10, 10], "at": [200, 0, 0] },
            { "type": "OffsetFace", "id": "pp", "body": "o", "faces": [ { "body": "o", "point": [205, 5, 10] } ], "distance": 5 },
            { "type": "Box", "id": "gone", "size": [1, 1, 1], "at": [300, 0, 0] },
            { "type": "Delete", "id": "del", "body": "gone" },
            { "type": "Box", "id": "m", "size": [10, 10, 10], "at": [400, 0, 0] },
            { "type": "Box", "id": "n", "size": [10, 10, 10], "at": [405, 0, 0] },
            { "type": "Combine", "id": "cb", "target": "m", "tool": "n", "op": "Cut", "keep_tool": true } ] }"#);
        assert!(!ev.has_errors(), "{:?}", ev.status);
        let pi = std::f64::consts::PI;
        // half sphere cut, cylinder joined on top
        assert!((vol(&ev, "b") - (1000.0 - 0.5 * 4.0 / 3.0 * pi * 27.0 + pi * 4.0 * 5.0)).abs() < 1e-2, "{}", vol(&ev, "b"));
        let torus = 2.0 * pi * pi * 10.0 * 4.0;
        assert!((vol(&ev, "t") - torus * 8.0).abs() < 1e-2, "{}", vol(&ev, "t"));
        assert!((vol(&ev, "k") - 600.0).abs() < 1e-6 && (vol(&ev, "k_low") - 400.0).abs() < 1e-6);
        assert!((vol(&ev, "o") - 1500.0).abs() < 1e-6, "{}", vol(&ev, "o"));
        assert!(ev.bodies.iter().all(|(id, _)| id != "gone"));
        assert!((vol(&ev, "m") - 500.0).abs() < 1e-6 && (vol(&ev, "n") - 1000.0).abs() < 1e-6);
    }

    #[test]
    fn import_brep_round_trip() {
        let ev = run(r#"{ "timeline": [ { "type": "Cylinder", "id": "c", "radius": 5, "height": 10 } ] }"#);
        let mut brep = Vec::new();
        Solid::write_brep(ev.solids(), &mut brep).unwrap();
        let doc = Document { timeline: vec![Feature::Import { id: "imp".into(), source: Some("c.step".into()), data: crate::b64::encode(&brep), at: [Expr::Num(1.0), Expr::Num(2.0), Expr::Num(3.0)], op: BoolOp::NewBody, target: None }], ..Default::default() };
        let ev2 = evaluate(&doc, &BTreeMap::new()).unwrap();
        assert!(!ev2.has_errors(), "{:?}", ev2.status);
        assert!((vol(&ev2, "imp") - vol(&ev, "c")).abs() < 1e-6);
        assert!((ev2.info()[0].bbox_min[2] - 3.0).abs() < 1e-6);
        assert!(ev2.face_names("imp").iter().all(|(n, _)| n.starts_with("imp/")));
    }

    #[test]
    fn cache_replays_unchanged_features_and_keeps_names() {
        let mut cache = crate::graph::EvalCache::new();
        let doc = bracket();
        let a = evaluate_with(&doc, &BTreeMap::new(), Some(&mut cache)).unwrap();
        assert_eq!(a.reused, 0);
        let b = evaluate_with(&doc, &BTreeMap::new(), Some(&mut cache)).unwrap();
        assert_eq!(b.reused, doc.timeline.len(), "{:?}", b.status.iter().map(|s| (s.id.clone(), s.cached)).collect::<Vec<_>>());
        assert!(b.status.iter().all(|s| s.cached && s.ok));
        assert_eq!(a.info()[0].volume, b.info()[0].volume);
        let mut na = a.face_names("base"); let mut nb = b.face_names("base");
        na.sort_by(|x, y| x.0.cmp(&y.0)); nb.sort_by(|x, y| x.0.cmp(&y.0));
        assert_eq!(na.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>(), nb.iter().map(|(n, _)| n.clone()).collect::<Vec<_>>());
        assert_eq!(a.edges().iter().map(|e| e.name.clone()).collect::<std::collections::BTreeSet<_>>(), b.edges().iter().map(|e| e.name.clone()).collect::<std::collections::BTreeSet<_>>());
        // A parameter edit recomputes only the features that mention it or sit downstream.
        let mut ov = BTreeMap::new();
        ov.insert("hole_d".to_string(), "9".to_string());
        let c = evaluate_with(&doc, &ov, Some(&mut cache)).unwrap();
        assert!(c.reused > 0 && c.reused < doc.timeline.len(), "{}", c.reused);
        assert!(c.info()[0].volume < a.info()[0].volume);
        // Undo: the previous document is still cached in full.
        let d = evaluate_with(&doc, &BTreeMap::new(), Some(&mut cache)).unwrap();
        assert_eq!(d.reused, doc.timeline.len());
        assert_eq!(d.info()[0].volume, a.info()[0].volume);
    }

    #[test]
    fn independent_branches_do_not_recompute() {
        let mut cache = crate::graph::EvalCache::new();
        let json = |r: f64| format!(r#"{{ "timeline": [
            {{ "type": "Box", "id": "a", "size": [10, 10, 10] }},
            {{ "type": "Fillet", "id": "a_r", "body": "a", "radius": {r} }},
            {{ "type": "Box", "id": "b", "size": [10, 10, 10], "at": [5, 0, 0] }},
            {{ "type": "Chamfer", "id": "b_c", "body": "b", "distance": 1 }},
            {{ "type": "Combine", "id": "both", "target": "a", "tool": "b", "op": "Join", "keep_tool": true }} ] }}"#);
        let d1 = Document::from_json(&json(1.0)).unwrap();
        let d2 = Document::from_json(&json(2.0)).unwrap();
        evaluate_with(&d1, &BTreeMap::new(), Some(&mut cache)).unwrap();
        let ev = evaluate_with(&d2, &BTreeMap::new(), Some(&mut cache)).unwrap();
        let cached: Vec<(String, bool)> = ev.status.iter().map(|s| (s.id.clone(), s.cached)).collect();
        assert_eq!(cached, vec![("a".into(), true), ("a_r".into(), false), ("b".into(), true), ("b_c".into(), true), ("both".into(), false)], "{cached:?}");
        let graph = crate::graph::dependency_graph(&d2);
        assert_eq!(graph[4], vec!["a_r".to_string(), "b_c".to_string()]);
        assert!(crate::graph::dot(&d2).contains("\"a_r\" -> \"both\""));
        // Deleting the tool later changes nothing upstream.
        let mut d3 = d2.clone();
        d3.timeline.push(Feature::Delete { id: "rm".into(), body: "b".into() });
        let ev = evaluate_with(&d3, &BTreeMap::new(), Some(&mut cache)).unwrap();
        assert_eq!(ev.reused, 5);
        assert_eq!(ev.bodies.len(), 1);
    }

    #[test]
    fn persistent_name_survives_upstream_edit() {
        let make = |with_name: bool| {
            let names = if with_name { r#", "names": ["base/e0|base/end"]"# } else { "" };
            Document::from_json(&format!(
                r#"{{ "parameters": {{ "h": "10" }}, "timeline": [
                    {{ "type": "Sketch", "id": "sk", "plane": "XY", "entities": [ {{ "type": "Rect", "x": 0, "y": 0, "w": 40, "h": 25 }} ] }},
                    {{ "type": "Extrude", "id": "base", "sketch": "sk", "distance": "h" }},
                    {{ "type": "Fillet", "id": "soft", "body": "base", "radius": 3, "edges": {{ "pick": [[20, 0, 10]]{names} }} }} ] }}"#
            ))
            .unwrap()
        };
        let plain = evaluate(&make(false), &BTreeMap::new()).unwrap();
        assert!(!plain.has_errors(), "{:?}", plain.status);
        let named = evaluate(&make(true), &BTreeMap::new()).unwrap();
        assert!(!named.has_errors() && named.status[2].warning.is_none(), "{:?}", named.status);
        assert!((plain.bodies[0].1.volume() - named.bodies[0].1.volume()).abs() < 1e-6);
        // Every face and edge is named, with no duplicates.
        let edges = named.edges();
        let mut seen = std::collections::HashSet::new();
        assert!(edges.iter().all(|e| !e.name.contains('?') && seen.insert(e.name.clone())), "{:?}", edges.iter().map(|e| &e.name).collect::<Vec<_>>());
        assert!(named.face_names("base").iter().any(|(n, _)| n == "soft/f0"));

        // Upstream edit: plate height 10 -> 30, the picked edge moves 20 mm.
        let taller: BTreeMap<String, String> = [("h".to_string(), "30".to_string())].into_iter().collect();
        let by_point = evaluate(&make(false), &taller).unwrap();
        assert!(by_point.has_errors() || by_point.status[2].warning.is_some(), "{:?}", by_point.status);
        let by_name = evaluate(&make(true), &taller).unwrap();
        assert!(!by_name.has_errors() && by_name.status[2].warning.is_none(), "{:?}", by_name.status);
        let box_volume = 40.0 * 25.0 * 30.0;
        let v = by_name.bodies[0].1.volume();
        assert!(v < box_volume - 1.0 && v > box_volume - 200.0, "{v}");
        // The fillet face still borders the top cap, i.e. it rounded the same edge.
        assert!(by_name.edges().iter().any(|e| e.name == "base/end|soft/f0"), "{:?}", by_name.edges().iter().map(|e| &e.name).collect::<Vec<_>>());
    }
}

/// Wall-clock timer that degrades to zero on wasm32, where `Instant` is unavailable.
struct Clock(#[cfg(not(target_arch = "wasm32"))] std::time::Instant);

impl Clock {
    fn start() -> Self {
        Clock(#[cfg(not(target_arch = "wasm32"))] std::time::Instant::now())
    }
    fn millis(&self) -> u128 {
        #[cfg(not(target_arch = "wasm32"))]
        { self.0.elapsed().as_millis() }
        #[cfg(target_arch = "wasm32")]
        { 0 }
    }
}

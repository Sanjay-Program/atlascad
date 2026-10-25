//! 2D sketch evaluation: resolve expressions, solve constraints with ezpz,
//! chain line/arc segments into closed loops, and map to the sketch plane.

use crate::doc::{Constraint, PointRef, SketchEntity};
use crate::expr::Scope;
use cadrum::DVec3;
use ezpz::datatypes::inputs::{DatumCircle, DatumCircularArc, DatumDistance, DatumLineSegment, DatumPoint};
use ezpz::{Constraint as C, ConstraintRequest, IdGenerator};

const TOL: f64 = 1e-4;

/// Numeric 2D geometry after expression and constraint evaluation.
#[derive(Debug, Clone, PartialEq)]
pub enum Geo2 {
    Rect { x: f64, y: f64, w: f64, h: f64 },
    Circle { c: [f64; 2], r: f64 },
    Polygon(Vec<[f64; 2]>),
    Line([f64; 2], [f64; 2]),
    /// start, point on arc, end
    Arc([f64; 2], [f64; 2], [f64; 2]),
}

/// Plane basis resolved by the caller (named plane or body face).
#[derive(Debug, Clone, Copy)]
pub struct Basis {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    pub normal: DVec3,
}

#[derive(Clone)]
pub struct SketchGeometry {
    pub origin: DVec3,
    pub u: DVec3,
    pub v: DVec3,
    pub normal: DVec3,
    /// Closed loops, largest first (outer), then holes.
    pub loops: Vec<Vec<Geo2>>,
    /// Segment chains that did not close.
    pub open: Vec<Vec<Geo2>>,
    /// Solved entities in document order (for the UI).
    pub entities: Vec<Geo2>,
    /// Remaining degrees of freedom, -1 when not analysed.
    pub dof: i64,
    /// Entity indices that still have free variables.
    pub underconstrained: Vec<usize>,
}

impl SketchGeometry {
    pub fn to_world(&self, x: f64, y: f64) -> DVec3 {
        self.origin + self.u * x + self.v * y
    }
}

pub fn evaluate_sketch(basis: Basis, entities: &[SketchEntity], constraints: &[Constraint], scope: &Scope) -> Result<SketchGeometry, String> {
    let Basis { origin, u, v, normal: n } = basis;
    let mut geo = Vec::with_capacity(entities.len());
    for e in entities {
        geo.push(match e {
            SketchEntity::Rect { x, y, w, h } => Geo2::Rect { x: ev(scope, x)?, y: ev(scope, y)?, w: ev(scope, w)?, h: ev(scope, h)? },
            SketchEntity::Circle { cx, cy, r } => Geo2::Circle { c: [ev(scope, cx)?, ev(scope, cy)?], r: ev(scope, r)? },
            SketchEntity::Polygon { points } => {
                if points.len() < 3 {
                    return Err("polygon needs at least 3 points".into());
                }
                Geo2::Polygon(points.iter().map(|p| Ok([ev(scope, &p[0])?, ev(scope, &p[1])?])).collect::<Result<_, String>>()?)
            }
            SketchEntity::Line { x0, y0, x1, y1 } => Geo2::Line([ev(scope, x0)?, ev(scope, y0)?], [ev(scope, x1)?, ev(scope, y1)?]),
            SketchEntity::Arc { x0, y0, xm, ym, x1, y1 } => Geo2::Arc([ev(scope, x0)?, ev(scope, y0)?], [ev(scope, xm)?, ev(scope, ym)?], [ev(scope, x1)?, ev(scope, y1)?]),
        });
    }
    let (dof, underconstrained) = if constraints.is_empty() { free_dof(&geo) } else { solve(&mut geo, constraints, scope)? };
    let entities = geo.clone();
    let (loops, open) = build_loops(geo);
    Ok(SketchGeometry { origin, u, v, normal: n, loops, open, entities, dof, underconstrained })
}

/// Without constraints every solvable entity is free.
fn free_dof(geo: &[Geo2]) -> (i64, Vec<usize>) {
    let mut dof = 0;
    let mut under = Vec::new();
    for (i, g) in geo.iter().enumerate() {
        let n = match g {
            Geo2::Line(..) => 4,
            Geo2::Circle { .. } => 3,
            Geo2::Arc(..) => 5,
            _ => 0,
        };
        if n > 0 {
            dof += n;
            under.push(i);
        }
    }
    (dof, under)
}

fn ev(scope: &Scope, e: &crate::Expr) -> Result<f64, String> {
    scope.eval(e).map_err(|err| err.to_string())
}

// ---------- constraint solving ----------

struct Vars {
    ids: IdGenerator,
    guesses: Vec<(ezpz::Id, f64)>,
}

impl Vars {
    fn point(&mut self, p: [f64; 2]) -> DatumPoint {
        let d = DatumPoint::new(&mut self.ids);
        self.guesses.push((d.id_x(), p[0]));
        self.guesses.push((d.id_y(), p[1]));
        d
    }
    fn scalar(&mut self, v: f64) -> DatumDistance {
        let id = self.ids.next_id();
        self.guesses.push((id, v));
        DatumDistance::new(id)
    }
}

/// Solver view of one entity. Rect/Polygon are rigid and not solvable.
enum Datum {
    Line(DatumLineSegment),
    Circle(DatumCircle),
    Arc(DatumCircularArc),
    Rigid,
}

fn arc_center(a: [f64; 2], m: [f64; 2], b: [f64; 2]) -> Result<[f64; 2], String> {
    let (ax, ay, bx, by, cx, cy) = (a[0], a[1], m[0], m[1], b[0], b[1]);
    let d = 2.0 * (ax * (by - cy) + bx * (cy - ay) + cx * (ay - by));
    if d.abs() < 1e-12 {
        return Err("arc points are collinear".into());
    }
    let ux = ((ax * ax + ay * ay) * (by - cy) + (bx * bx + by * by) * (cy - ay) + (cx * cx + cy * cy) * (ay - by)) / d;
    let uy = ((ax * ax + ay * ay) * (cx - bx) + (bx * bx + by * by) * (ax - cx) + (cx * cx + cy * cy) * (bx - ax)) / d;
    Ok([ux, uy])
}

/// Point on the arc between start and end, on the same side as the original mid point.
fn arc_mid(c: [f64; 2], a: [f64; 2], b: [f64; 2], old_mid: [f64; 2]) -> [f64; 2] {
    let r = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();
    let (ta, tb) = ((a[1] - c[1]).atan2(a[0] - c[0]), (b[1] - c[1]).atan2(b[0] - c[0]));
    let tm_old = (old_mid[1] - c[1]).atan2(old_mid[0] - c[0]);
    // Two candidate mid angles; keep the one closer to the previous mid direction.
    let mut best = [0.0; 2];
    let mut best_d = f64::MAX;
    for k in [0.0, std::f64::consts::PI] {
        let tm = (ta + tb) / 2.0 + k;
        let d = ((tm - tm_old + std::f64::consts::PI).rem_euclid(2.0 * std::f64::consts::PI) - std::f64::consts::PI).abs();
        if d < best_d {
            best_d = d;
            best = [c[0] + r * tm.cos(), c[1] + r * tm.sin()];
        }
    }
    best
}

fn solve(geo: &mut [Geo2], constraints: &[Constraint], scope: &Scope) -> Result<(i64, Vec<usize>), String> {
    let mut vars = Vars { ids: IdGenerator::default(), guesses: Vec::new() };
    let mut datums = Vec::with_capacity(geo.len());
    let mut reqs = Vec::new();
    let mut n_eqs: usize = 0;
    for g in geo.iter() {
        datums.push(match g {
            Geo2::Line(a, b) => Datum::Line(DatumLineSegment::new(vars.point(*a), vars.point(*b))),
            Geo2::Circle { c, r } => Datum::Circle(DatumCircle { center: vars.point(*c), radius: vars.scalar(*r) }),
            Geo2::Arc(a, m, b) => {
                let c = arc_center(*a, *m, *b)?;
                let arc = DatumCircularArc { center: vars.point(c), start: vars.point(*a), end: vars.point(*b) };
                reqs.push(ConstraintRequest::highest_priority(C::Arc(arc)));
                n_eqs += 1;
                Datum::Arc(arc)
            }
            _ => Datum::Rigid,
        });
    }
    let point = |r: &PointRef| -> Result<DatumPoint, String> {
        let d = datums.get(r.e).ok_or_else(|| format!("constraint references entity {} which does not exist", r.e))?;
        match (d, r.p.as_str()) {
            (Datum::Line(l), "start") => Ok(l.p0),
            (Datum::Line(l), "end") => Ok(l.p1),
            (Datum::Arc(a), "start") => Ok(a.start),
            (Datum::Arc(a), "end") => Ok(a.end),
            (Datum::Arc(a), "center") => Ok(a.center),
            (Datum::Circle(c), "center") => Ok(c.center),
            (Datum::Rigid, _) => Err(format!("entity {} is a rectangle or polygon and cannot be constrained", r.e)),
            (_, p) => Err(format!("entity {} has no point '{p}'", r.e)),
        }
    };
    let line = |e: usize| -> Result<DatumLineSegment, String> {
        match datums.get(e) {
            Some(Datum::Line(l)) => Ok(*l),
            Some(_) => Err(format!("entity {e} is not a line")),
            None => Err(format!("entity {e} does not exist")),
        }
    };
    for c in constraints {
        n_eqs += match c {
            Constraint::Coincident { .. } | Constraint::FixPoint { .. } => 2,
            _ => 1,
        };
        match c {
            Constraint::Horizontal { e } => reqs.push(ConstraintRequest::highest_priority(C::Horizontal(line(*e)?))),
            Constraint::Vertical { e } => reqs.push(ConstraintRequest::highest_priority(C::Vertical(line(*e)?))),
            Constraint::Coincident { a, b } => reqs.push(ConstraintRequest::highest_priority(C::PointsCoincident(point(a)?, point(b)?))),
            Constraint::Distance { a, b, value } => reqs.push(ConstraintRequest::highest_priority(C::Distance(point(a)?, point(b)?, ev(scope, value)?))),
            Constraint::Radius { e, value } => match datums.get(*e) {
                Some(Datum::Circle(c)) => reqs.push(ConstraintRequest::highest_priority(C::CircleRadius(*c, ev(scope, value)?))),
                Some(Datum::Arc(a)) => reqs.push(ConstraintRequest::highest_priority(C::ArcRadius(*a, ev(scope, value)?))),
                _ => return Err(format!("entity {e} is not a circle or arc")),
            },
            Constraint::FixPoint { a, x, y } => {
                let p = point(a)?;
                reqs.push(ConstraintRequest::highest_priority(C::Fixed(p.id_x(), ev(scope, x)?)));
                reqs.push(ConstraintRequest::highest_priority(C::Fixed(p.id_y(), ev(scope, y)?)));
            }
            Constraint::Equal { a, b } => match (datums.get(*a), datums.get(*b)) {
                (Some(Datum::Line(l0)), Some(Datum::Line(l1))) => reqs.push(ConstraintRequest::highest_priority(C::LinesEqualLength(*l0, *l1))),
                (Some(Datum::Circle(c0)), Some(Datum::Circle(c1))) => reqs.push(ConstraintRequest::highest_priority(C::ScalarEqual(c0.radius.id, c1.radius.id))),
                _ => return Err("equal needs two lines or two circles".into()),
            },
            Constraint::Parallel { a, b } => reqs.push(ConstraintRequest::highest_priority(C::LinesAtAngle(line(*a)?, line(*b)?, ezpz::datatypes::AngleKind::Parallel))),
            Constraint::Perpendicular { a, b } => reqs.push(ConstraintRequest::highest_priority(C::LinesAtAngle(line(*a)?, line(*b)?, ezpz::datatypes::AngleKind::Perpendicular))),
        }
    }
    let n_vars = vars.guesses.len();
    let solved = ezpz::solve_analysis(&reqs, vars.guesses, ezpz::Config::default()).map_err(|e| format!("sketch constraints could not be solved: {:?}", e.error()))?;
    let outcome = solved.outcome;
    if !outcome.is_satisfied() {
        return Err(format!("sketch constraints are inconsistent ({} unsatisfied)", outcome.unsatisfied().len()));
    }
    let free: std::collections::HashSet<ezpz::Id> = solved.analysis.underconstrained().iter().copied().collect();
    // Remaining freedom = variables - scalar equations (assumes independent constraints).
    let dof = (n_vars as i64 - n_eqs as i64).max(0);
    let mut under = Vec::new();
    for (i, d) in datums.iter().enumerate() {
        let ids: Vec<ezpz::Id> = match d {
            Datum::Line(l) => vec![l.p0.id_x(), l.p0.id_y(), l.p1.id_x(), l.p1.id_y()],
            Datum::Circle(c) => vec![c.center.id_x(), c.center.id_y(), c.radius.id],
            Datum::Arc(a) => vec![a.start.id_x(), a.start.id_y(), a.end.id_x(), a.end.id_y(), a.center.id_x(), a.center.id_y()],
            Datum::Rigid => vec![],
        };
        if ids.iter().any(|id| free.contains(id)) {
            under.push(i);
        }
    }
    let p2 = |p: &DatumPoint| {
        let q = outcome.final_value_point(p);
        [q.x, q.y]
    };
    for (g, d) in geo.iter_mut().zip(datums.iter()) {
        match (g, d) {
            (Geo2::Line(a, b), Datum::Line(l)) => {
                *a = p2(&l.p0);
                *b = p2(&l.p1);
            }
            (Geo2::Circle { c, r }, Datum::Circle(dc)) => {
                *c = p2(&dc.center);
                *r = outcome.final_value_distance(&dc.radius);
            }
            (Geo2::Arc(a, m, b), Datum::Arc(da)) => {
                let (c, na, nb) = (p2(&da.center), p2(&da.start), p2(&da.end));
                *m = arc_mid(c, na, nb, *m);
                *a = na;
                *b = nb;
            }
            _ => {}
        }
    }
    Ok((dof, under))
}

/// Document-shaped JSON of a solved entity.
pub fn geo_json(g: &Geo2) -> serde_json::Value {
    use serde_json::json;
    match g {
        Geo2::Rect { x, y, w, h } => json!({"type": "Rect", "x": x, "y": y, "w": w, "h": h}),
        Geo2::Circle { c, r } => json!({"type": "Circle", "cx": c[0], "cy": c[1], "r": r}),
        Geo2::Polygon(p) => json!({"type": "Polygon", "points": p}),
        Geo2::Line(a, b) => json!({"type": "Line", "x0": a[0], "y0": a[1], "x1": b[0], "y1": b[1]}),
        Geo2::Arc(a, m, b) => json!({"type": "Arc", "x0": a[0], "y0": a[1], "xm": m[0], "ym": m[1], "x1": b[0], "y1": b[1]}),
    }
}

// ---------- loops ----------

fn close(a: [f64; 2], b: [f64; 2]) -> bool {
    (a[0] - b[0]).abs() < TOL && (a[1] - b[1]).abs() < TOL
}

fn ends(g: &Geo2) -> ([f64; 2], [f64; 2]) {
    match g {
        Geo2::Line(a, b) => (*a, *b),
        Geo2::Arc(a, _, b) => (*a, *b),
        _ => unreachable!("closed entities have no ends"),
    }
}

fn reversed(g: &Geo2) -> Geo2 {
    match g {
        Geo2::Line(a, b) => Geo2::Line(*b, *a),
        Geo2::Arc(a, m, b) => Geo2::Arc(*b, *m, *a),
        other => other.clone(),
    }
}

/// Chain open segments by coincident endpoints. Returns (closed loops sorted by bbox area desc, open chains).
fn build_loops(geo: Vec<Geo2>) -> (Vec<Vec<Geo2>>, Vec<Vec<Geo2>>) {
    let mut loops: Vec<Vec<Geo2>> = Vec::new();
    let mut open: Vec<Vec<Geo2>> = Vec::new();
    let mut segs: Vec<Option<Geo2>> = Vec::new();
    for g in geo {
        match g {
            Geo2::Line(..) | Geo2::Arc(..) => segs.push(Some(g)),
            closed => loops.push(vec![closed]),
        }
    }
    loop {
        let Some(start) = segs.iter().position(|s| s.is_some()) else { break };
        let first = segs[start].take().unwrap();
        let (chain_start, mut tail) = ends(&first);
        let mut chain = vec![first];
        let mut closed = false;
        loop {
            if close(tail, chain_start) && chain.len() > 1 {
                closed = true;
                break;
            }
            let next = segs.iter().position(|s| s.as_ref().map_or(false, |g| close(ends(g).0, tail) || close(ends(g).1, tail)));
            let Some(i) = next else { break };
            let g = segs[i].take().unwrap();
            let g = if close(ends(&g).0, tail) { g } else { reversed(&g) };
            tail = ends(&g).1;
            chain.push(g);
        }
        if closed {
            loops.push(chain);
        } else {
            open.push(chain);
        }
    }
    loops.sort_by(|a, b| bbox_area(b).partial_cmp(&bbox_area(a)).unwrap());
    (loops, open)
}

/// Dense polyline of a loop or chain, for display.
pub fn polyline(lp: &[Geo2]) -> Vec<[f64; 2]> {
    let mut out = Vec::new();
    for g in lp {
        match g {
            Geo2::Rect { x, y, w, h } => out.extend([[*x, *y], [x + w, *y], [x + w, y + h], [*x, y + h], [*x, *y]]),
            Geo2::Polygon(p) => {
                out.extend(p.iter().copied());
                out.push(p[0]);
            }
            Geo2::Circle { c, r } => out.extend((0..=48).map(|i| {
                let t = i as f64 / 48.0 * std::f64::consts::TAU;
                [c[0] + r * t.cos(), c[1] + r * t.sin()]
            })),
            Geo2::Line(a, b) => {
                if out.is_empty() {
                    out.push(*a);
                }
                out.push(*b);
            }
            Geo2::Arc(a, m, b) => {
                if let Ok(c) = arc_center(*a, *m, *b) {
                    let r = ((a[0] - c[0]).powi(2) + (a[1] - c[1]).powi(2)).sqrt();
                    let ta = (a[1] - c[1]).atan2(a[0] - c[0]);
                    let tm = (m[1] - c[1]).atan2(m[0] - c[0]);
                    let tb = (b[1] - c[1]).atan2(b[0] - c[0]);
                    // sweep from a through m to b
                    let norm = |t: f64| t.rem_euclid(std::f64::consts::TAU);
                    let (dm, db) = (norm(tm - ta), norm(tb - ta));
                    let sweep = if dm < db { db } else { db - std::f64::consts::TAU };
                    let n = 24;
                    if out.is_empty() {
                        out.push(*a);
                    }
                    for i in 1..=n {
                        let t = ta + sweep * i as f64 / n as f64;
                        out.push([c[0] + r * t.cos(), c[1] + r * t.sin()]);
                    }
                } else {
                    out.push(*b);
                }
            }
        }
    }
    out
}

/// Point-in-polygon (even-odd) on the dense polyline of a loop.
fn contains_point(lp: &[Geo2], p: [f64; 2]) -> bool {
    let poly = polyline(lp);
    let mut inside = false;
    let n = poly.len();
    if n < 3 {
        return false;
    }
    let mut j = n - 1;
    for i in 0..n {
        let (a, b) = (poly[i], poly[j]);
        if (a[1] > p[1]) != (b[1] > p[1]) {
            let x = (b[0] - a[0]) * (p[1] - a[1]) / (b[1] - a[1]) + a[0];
            if p[0] < x {
                inside = !inside;
            }
        }
        j = i;
    }
    inside
}

/// Group closed loops into profiles: every loop not inside another loop is an
/// outer boundary; loops directly inside it are its holes. Returns indices into `loops`.
pub fn profiles(loops: &[Vec<Geo2>]) -> Vec<(usize, Vec<usize>)> {
    let sample = |lp: &Vec<Geo2>| polyline(lp)[0];
    // parent[i] = smallest loop that contains loop i (loops are sorted by bbox area desc)
    let mut parent: Vec<Option<usize>> = vec![None; loops.len()];
    for i in 0..loops.len() {
        let p = sample(&loops[i]);
        for j in (0..i).rev() {
            if contains_point(&loops[j], p) {
                parent[i] = Some(j);
                break;
            }
        }
    }
    let depth = |mut i: usize| {
        let mut d = 0;
        while let Some(p) = parent[i] {
            d += 1;
            i = p;
        }
        d
    };
    let mut out: Vec<(usize, Vec<usize>)> = Vec::new();
    for i in 0..loops.len() {
        if depth(i) % 2 == 0 {
            out.push((i, Vec::new()));
        }
    }
    for i in 0..loops.len() {
        if depth(i) % 2 == 1 {
            if let Some(p) = parent[i] {
                if let Some(entry) = out.iter_mut().find(|(o, _)| *o == p) {
                    entry.1.push(i);
                }
            }
        }
    }
    out
}

fn bbox_area(lp: &[Geo2]) -> f64 {
    let pts = polyline(lp);
    let (mut lo, mut hi) = ([f64::MAX; 2], [f64::MIN; 2]);
    for p in pts {
        lo = [lo[0].min(p[0]), lo[1].min(p[1])];
        hi = [hi[0].max(p[0]), hi[1].max(p[1])];
    }
    (hi[0] - lo[0]).max(0.0) * (hi[1] - lo[1]).max(0.0)
}

/// Signed area of a closed polyline (positive = counter-clockwise).
fn signed_area(pts: &[[f64; 2]]) -> f64 {
    let mut a = 0.0;
    for w in pts.windows(2) {
        a += w[0][0] * w[1][1] - w[1][0] * w[0][1];
    }
    a * 0.5
}

fn norm2(v: [f64; 2]) -> [f64; 2] {
    let l = (v[0] * v[0] + v[1] * v[1]).sqrt().max(1e-12);
    [v[0] / l, v[1] / l]
}

/// Intersection of the lines through `p` along `d` and through `q` along `e`; `None` when parallel.
fn line_meet(p: [f64; 2], d: [f64; 2], q: [f64; 2], e: [f64; 2]) -> Option<[f64; 2]> {
    let den = d[0] * e[1] - d[1] * e[0];
    if den.abs() < 1e-9 {
        return None;
    }
    let t = ((q[0] - p[0]) * e[1] - (q[1] - p[1]) * e[0]) / den;
    Some([p[0] + d[0] * t, p[1] + d[1] * t])
}

/// Offset a closed loop by `t` (positive = outward, away from the enclosed area), for tapered
/// extrudes. Rectangles, circles and polygons offset exactly; line / arc chains offset each entity
/// along its outward normal and meet neighbours at the intersection of their end tangents (an
/// arc keeps its offset mid point, so a sharp line-arc corner is approximate). Returns `None`
/// when the offset collapses the loop.
pub fn offset_loop(lp: &[Geo2], t: f64) -> Option<Vec<Geo2>> {
    if t.abs() < 1e-12 {
        return Some(lp.to_vec());
    }
    let ccw = signed_area(&polyline(lp)) > 0.0;
    // Outward normal of a travel direction.
    let out = |d: [f64; 2]| if ccw { [d[1], -d[0]] } else { [-d[1], d[0]] };
    if lp.len() == 1 {
        match &lp[0] {
            Geo2::Rect { x, y, w, h } => {
                return (w + 2.0 * t > 1e-9 && h + 2.0 * t > 1e-9).then(|| vec![Geo2::Rect { x: x - t, y: y - t, w: w + 2.0 * t, h: h + 2.0 * t }]);
            }
            Geo2::Circle { c, r } => return (r + t > 1e-9).then(|| vec![Geo2::Circle { c: *c, r: r + t }]),
            Geo2::Polygon(p) => {
                let n = p.len();
                let mut q = Vec::with_capacity(n);
                for i in 0..n {
                    let (a, b, c) = (p[(i + n - 1) % n], p[i], p[(i + 1) % n]);
                    let (d0, d1) = (norm2([b[0] - a[0], b[1] - a[1]]), norm2([c[0] - b[0], c[1] - b[1]]));
                    let (n0, n1) = (out(d0), out(d1));
                    let p0 = [b[0] + n0[0] * t, b[1] + n0[1] * t];
                    let p1 = [b[0] + n1[0] * t, b[1] + n1[1] * t];
                    q.push(line_meet(p0, d0, p1, d1).unwrap_or(p0));
                }
                let keep = signed_area(&{ let mut c = q.clone(); c.push(q[0]); c }) * signed_area(&polyline(lp)) > 0.0;
                return keep.then_some(vec![Geo2::Polygon(q)]);
            }
            _ => {}
        }
    }
    // Chain of lines and arcs: per-entity offset ends and tangents.
    struct Seg { s: [f64; 2], e: [f64; 2], ds: [f64; 2], de: [f64; 2], mid: Option<[f64; 2]> }
    let mut segs = Vec::new();
    for g in lp {
        match g {
            Geo2::Line(a, b) => {
                let d = norm2([b[0] - a[0], b[1] - a[1]]);
                let n = out(d);
                segs.push(Seg { s: [a[0] + n[0] * t, a[1] + n[1] * t], e: [b[0] + n[0] * t, b[1] + n[1] * t], ds: d, de: d, mid: None });
            }
            Geo2::Arc(a, m, b) => {
                let c = arc_center(*a, *m, *b).ok()?;
                let cross = (m[0] - a[0]) * (b[1] - a[1]) - (m[1] - a[1]) * (b[0] - a[0]);
                let sweep_ccw = cross > 0.0;
                let tangent = |p: [f64; 2]| { let r = [p[0] - c[0], p[1] - c[1]]; norm2(if sweep_ccw { [-r[1], r[0]] } else { [r[1], -r[0]] }) };
                let off = |p: [f64; 2]| { let n = out(tangent(p)); [p[0] + n[0] * t, p[1] + n[1] * t] };
                let oa = off(*a);
                if ((oa[0] - c[0]).powi(2) + (oa[1] - c[1]).powi(2)).sqrt() <= 1e-9 {
                    return None;
                }
                segs.push(Seg { s: off(*a), e: off(*b), ds: tangent(*a), de: tangent(*b), mid: Some(off(*m)) });
            }
            _ => return None,
        }
    }
    let n = segs.len();
    let mut joints = Vec::with_capacity(n);
    for i in 0..n {
        let (prev, next) = (&segs[(i + n - 1) % n], &segs[i]);
        let parallel = (prev.de[0] * next.ds[1] - prev.de[1] * next.ds[0]).abs() < 1e-6;
        joints.push(if parallel { [(prev.e[0] + next.s[0]) * 0.5, (prev.e[1] + next.s[1]) * 0.5] } else { line_meet(prev.e, prev.de, next.s, next.ds).unwrap_or(next.s) });
    }
    let mut outl = Vec::with_capacity(n);
    for i in 0..n {
        let (a, b) = (joints[i], joints[(i + 1) % n]);
        outl.push(match segs[i].mid {
            Some(m) => Geo2::Arc(a, m, b),
            None => Geo2::Line(a, b),
        });
    }
    (signed_area(&polyline(&outl)) * signed_area(&polyline(lp)) > 0.0).then_some(outl)
}

//! A footprint's 3D model as kernel solids (for STEP: real solids, not the mesh parts a
//! derived board shows): its model file when the document has it (STEP as is, VRML or OBJ as
//! meshed solids), else its generated body ([`cadrs_eda::model3d`]: boxes and cylinders as
//! extruded solids).

use cadrs_eda::footprint::Model3d;
use cadrs_eda::model3d::{Axis as MAxis, Solid};
use cadrs_kernel::{BodyId, Curve2, Extent, Kernel, Loop, Plane, Profile, Region};
use nalgebra::{Point2, Point3, Unit, Vector3};

fn first(r: cadrs_kernel::OpResult) -> Result<BodyId, String> {
    r.bodies.into_iter().next().ok_or_else(|| "no body".to_string())
}

/// A generated primitive as a kernel solid.
pub fn primitive(k: &mut dyn Kernel, s: &Solid) -> Result<BodyId, String> {
    match *s {
        Solid::Box { min, max } => {
            let plane = Plane { origin: Point3::new(0.0, 0.0, min[2]), ..Plane::top() };
            let (a, b) = (Point2::new(min[0], min[1]), Point2::new(max[0], max[1]));
            let c = [a, Point2::new(b.x, a.y), b, Point2::new(a.x, b.y)];
            let curves = (0..4).map(|i| Curve2::Line { a: c[i], b: c[(i + 1) % 4], source: None }).collect();
            let region = Region { outer: Loop { curves }, holes: vec![], source: None };
            first(k.extrude(&Profile::new(plane, vec![region]), Extent::Blind((max[2] - min[2]).max(1e-3))).map_err(|e| e.to_string())?)
        }
        Solid::Cylinder { base, axis, radius, length } => {
            let (normal, x_dir) = match axis {
                MAxis::X => (Vector3::x(), Vector3::y()),
                MAxis::Y => (Vector3::y(), Vector3::z()),
                MAxis::Z => (Vector3::z(), Vector3::x()),
            };
            let plane = Plane { origin: Point3::new(base[0], base[1], base[2]), x_dir: Unit::new_normalize(x_dir), normal: Unit::new_normalize(normal) };
            let region = Region { outer: Loop { curves: vec![Curve2::Circle { center: Point2::origin(), radius, source: None }] }, holes: vec![], source: None };
            first(k.extrude(&Profile::new(plane, vec![region]), Extent::Blind(length.max(1e-3))).map_err(|e| e.to_string())?)
        }
    }
}

/// A model file's solids (STEP imported, VRML meshed), in the file's frame and units.
pub fn file_solids(k: &mut dyn Kernel, bytes: &[u8], ext: &str) -> Result<(Vec<BodyId>, f64), String> {
    match ext.to_ascii_lowercase().as_str() {
        "step" | "stp" => Ok((k.import_step(bytes).map_err(|e| e.to_string())?, 1.0)),
        "wrl" | "vrml" | "obj" => {
            let text = String::from_utf8_lossy(bytes);
            let (meshes, unit) = if ext.eq_ignore_ascii_case("obj") { (cadrs_eda::obj::read(&text)?, 1.0) } else { (cadrs_eda::wrl::read(&text)?, cadrs_eda::wrl::KICAD_UNIT_MM) };
            let mut bodies = vec![];
            for m in meshes {
                let p = |i: u32| {
                    let q = m.positions[i as usize];
                    Point3::new(q[0] as f64, q[1] as f64, q[2] as f64)
                };
                let tris: Vec<[Point3<f64>; 3]> = m.indices.as_chunks::<3>().0.iter().map(|t| [p(t[0]), p(t[1]), p(t[2])]).collect();
                if let Ok(b) = k.mesh_solid(&tris, 1e-4) {
                    bodies.push(b);
                }
            }
            Ok((bodies, unit))
        }
        other => Err(format!("3D models of type .{other} aren't supported")),
    }
}

/// A model's solids: its file when the document has it, else its generated body.
pub fn model_bodies(k: &mut dyn Kernel, m: &Model3d) -> Result<(Vec<BodyId>, f64), String> {
    let ext = std::path::Path::new(&m.source).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default();
    if let Some(bytes) = m.blob.as_ref().and_then(|h| crate::blobs::get(h)) {
        let r = file_solids(k, &bytes, &ext);
        if r.as_ref().is_ok_and(|(b, _)| !b.is_empty()) || m.body.is_none() {
            return r;
        }
    }
    let body = m.body.as_ref().ok_or("no model")?;
    let mut v = vec![];
    for part in &body.parts {
        v.push(primitive(k, &part.solid)?);
    }
    Ok((v, 1.0))
}


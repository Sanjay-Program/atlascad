//! Footprint 3D models as display meshes, and where they sit on a native board (moved here from
//! `cadrs_pcb::mesh` so the rebuild can place a board's components in a Part Studio).
//!
//! - [`file_meshes`] reads a model file (STEP tessellated, VRML and OBJ as they are) into
//!   coloured meshes and its unit; [`cached_file_meshes`] does it once per blob for the whole
//!   process (the app's PCB Studio view and every rebuild share it).
//! - [`model_motion`] is how a model sits in its footprint's frame (KiCad's offset, rotation and
//!   scale); [`footprint_meshes`] gives a footprint's visible model in that frame.
//! - [`footprint_motion`] puts the footprint frame on the board: the **board frame** has the
//!   layout's x and y in mm and the board's bottom face at z = 0 (its top at the thickness), as
//!   `cadrs_pcb::placement` has for IDF boards. A top footprint is turned by its angle and lifted
//!   onto the top face; a bottom one is first turned over about its x axis (the layout mirrors
//!   bottom footprints in y, [`cadrs_eda::footprint::FootprintPlacement::apply`]), so its model
//!   hangs below the board. Both are rigid motions.

use std::collections::HashMap;
use std::sync::{Arc, OnceLock, RwLock};

use cadrs_eda::footprint::{Footprint, Model3d};
use cadrs_eda::model3d::Mesh;
use cadrs_kernel::Motion;
use nalgebra::{Matrix3, Rotation3, Vector3};

/// A model file's meshes and its unit (mm per file unit).
pub type ModelMeshes = Arc<(Vec<Mesh>, f64)>;

/// How finely STEP models are tessellated for display: 0.02 mm and 5°.
pub fn tessellation() -> cadrs_kernel::Tessellation {
    cadrs_kernel::Tessellation { deflection: 0.02, angle: std::f64::consts::PI / 36.0 }
}

/// A model file's meshes (`ext`: `wrl`/`vrml`, `obj`, `step`/`stp`) and its unit in mm: KiCad's
/// VRML is in 0.1 inch and coloured per shape; OBJ is in mm with its materials' colours; STEP
/// is in mm, one mesh per solid in a neutral grey.
#[cfg(feature = "occt")]
pub fn file_meshes(bytes: &[u8], ext: &str) -> Result<(Vec<Mesh>, f64), String> {
    use cadrs_kernel::Kernel;
    match ext.to_ascii_lowercase().as_str() {
        "wrl" | "vrml" => {
            let text = String::from_utf8_lossy(bytes);
            Ok((cadrs_eda::wrl::read(&text)?, cadrs_eda::wrl::KICAD_UNIT_MM))
        }
        "obj" => Ok((cadrs_eda::obj::read(&String::from_utf8_lossy(bytes))?, 1.0)),
        "step" | "stp" => {
            let mut k = cadrs_kernel::backend::occt::OcctKernel::new();
            let bodies = k.import_step(bytes).map_err(|e| e.to_string())?;
            let mut out = vec![];
            for b in bodies {
                let t = k.tessellate(b, tessellation()).map_err(|e| e.to_string());
                k.release(b);
                let t = t?;
                let f = |p: &nalgebra::Point3<f64>| [p.x as f32, p.y as f32, p.z as f32];
                out.push(Mesh {
                    color: [150, 150, 156],
                    positions: t.positions.iter().map(f).collect(),
                    normals: t.normals.iter().map(|n| [n.x as f32, n.y as f32, n.z as f32]).collect(),
                    indices: t.indices.iter().flatten().copied().collect(),
                    edges: t.edges.iter().map(|(_, pts)| pts.iter().map(f).collect()).collect(),
                });
            }
            if out.iter().all(|m| m.indices.is_empty()) {
                return Err("No solids in the STEP file".into());
            }
            Ok((out, 1.0))
        }
        other => Err(format!("3D models of type .{other} aren't supported (use STEP, VRML or OBJ)")),
    }
}

/// Without the kernel only VRML and OBJ models can be read.
#[cfg(not(feature = "occt"))]
pub fn file_meshes(bytes: &[u8], ext: &str) -> Result<(Vec<Mesh>, f64), String> {
    match ext.to_ascii_lowercase().as_str() {
        "wrl" | "vrml" => Ok((cadrs_eda::wrl::read(&String::from_utf8_lossy(bytes))?, cadrs_eda::wrl::KICAD_UNIT_MM)),
        "obj" => Ok((cadrs_eda::obj::read(&String::from_utf8_lossy(bytes))?, 1.0)),
        other => Err(format!("3D models of type .{other} need the solid-modelling kernel")),
    }
}

fn model_cache() -> &'static RwLock<HashMap<String, Result<ModelMeshes, String>>> {
    static CACHE: OnceLock<RwLock<HashMap<String, Result<ModelMeshes, String>>>> = OnceLock::new();
    CACHE.get_or_init(Default::default)
}

/// [`file_meshes`] of the blob `hash` (of type `ext`), read once per process. `Err` when the
/// blob isn't loaded or can't be read (the reason).
pub fn cached_file_meshes(hash: &str, ext: &str) -> Result<ModelMeshes, String> {
    if let Some(r) = model_cache().read().ok().and_then(|c| c.get(hash).cloned()) {
        return r;
    }
    let Some(bytes) = crate::blobs::get(hash) else { return Err("its file isn't loaded".into()) };
    let r = file_meshes(&bytes, ext).map(Arc::new);
    if let Ok(mut c) = model_cache().write() {
        c.insert(hash.to_string(), r.clone());
    }
    r
}

/// The file type of a model (its source's extension).
pub fn model_ext(m: &Model3d) -> String {
    std::path::Path::new(&m.source).extension().map(|e| e.to_string_lossy().into_owned()).unwrap_or_default()
}

/// The motion a footprint's 3D model sits by in its footprint's (package) frame, as KiCad
/// places models: scale (times `unit`, the file's unit in mm), turn by the negated angles about
/// X, then Y, then Z (degrees), then move by the offset (mm).
pub fn model_motion(m: &Model3d, unit: f64) -> Motion {
    let [sx, sy, sz] = m.scale.map(|s| s * unit);
    let [rx, ry, rz] = m.rotation.map(|a| -a.to_radians());
    let r = Rotation3::from_axis_angle(&Vector3::z_axis(), rz) * Rotation3::from_axis_angle(&Vector3::y_axis(), ry) * Rotation3::from_axis_angle(&Vector3::x_axis(), rx);
    Motion { linear: r.matrix() * Matrix3::from_diagonal(&Vector3::new(sx, sy, sz)), translation: Vector3::new(m.offset[0], m.offset[1], m.offset[2]) }
}

/// `m` moved by `motion` (positions, normals and edges; normals renormalised, so a scale is
/// fine).
pub fn moved_mesh(m: &Mesh, motion: &Motion) -> Mesh {
    let p = |q: &[f32; 3]| {
        let r = motion.linear * Vector3::new(q[0] as f64, q[1] as f64, q[2] as f64) + motion.translation;
        [r.x as f32, r.y as f32, r.z as f32]
    };
    // Normals go by the inverse transpose (a non-uniform scale bends them).
    let nmat = motion.linear.try_inverse().map(|i| i.transpose()).unwrap_or(motion.linear);
    let n = |q: &[f32; 3]| {
        let r = (nmat * Vector3::new(q[0] as f64, q[1] as f64, q[2] as f64)).normalize();
        [r.x as f32, r.y as f32, r.z as f32]
    };
    let mut out = Mesh { color: m.color, positions: m.positions.iter().map(p).collect(), normals: m.normals.iter().map(n).collect(), indices: m.indices.clone(), edges: m.edges.iter().map(|e| e.iter().map(p).collect()).collect() };
    // A mirroring motion turns the triangles inside out: wind them back.
    if motion.linear.determinant() < 0.0 {
        for t in out.indices.chunks_mut(3) {
            t.swap(1, 2);
        }
    }
    out
}

/// The visible model of a footprint (the first one with a file or a generated body).
pub fn shown_model(fp: &Footprint) -> Option<&Model3d> {
    fp.models.iter().find(|m| m.visible && (m.body.is_some() || m.blob.is_some()))
}

/// The meshes of a footprint's visible model in its footprint frame (z up from the board
/// surface), with its opacity: the model's file when it is loaded and readable, else its
/// generated body. `Ok(None)`: the footprint shows no model (as in KiCad). `Err`: its file
/// can't be read and it has no generated body.
pub fn footprint_meshes(fp: &Footprint) -> Result<Option<(Vec<Mesh>, f64)>, String> {
    let Some(m) = shown_model(fp) else { return Ok(None) };
    let mut why = None;
    if let Some(h) = &m.blob {
        match cached_file_meshes(h, &model_ext(m)) {
            Ok(file) if !file.0.is_empty() => {
                let motion = model_motion(m, file.1);
                return Ok(Some((file.0.iter().map(|g| moved_mesh(g, &motion)).collect(), m.opacity)));
            }
            Ok(_) => why = Some("its model file is empty".to_string()),
            Err(e) => why = Some(e),
        }
    }
    match &m.body {
        Some(body) => {
            let motion = model_motion(m, 1.0);
            Ok(Some((cadrs_eda::model3d::mesh(body).iter().map(|g| moved_mesh(g, &motion)).collect(), m.opacity)))
        }
        None => Err(format!("its 3D model {}: {}", m.source, why.unwrap_or_else(|| "no file".into()))),
    }
}

pub use super::frames::footprint_motion;

#[cfg(test)]
mod tests {
    use super::*;
    use cadrs_eda::footprint::FootprintPlacement;
    use cadrs_eda::layer::Side;
    use cadrs_eda::units::{Pt, to_mm};

    fn apply(m: &Motion, p: [f64; 3]) -> [f64; 3] {
        let r = m.linear * Vector3::new(p[0], p[1], p[2]) + m.translation;
        [r.x, r.y, r.z]
    }

    #[test]
    fn footprints_sit_on_their_face_where_the_layout_has_their_pads() {
        // A pad at (1, 2) mm of a footprint at (10, 20) turned 90°.
        for side in [Side::Top, Side::Bottom] {
            let pl = FootprintPlacement { at: Pt::mm(10.0, 20.0), angle: 90.0, side };
            let on_layout = pl.apply(Pt::mm(1.0, 2.0));
            let m = footprint_motion(&pl, 1.6);
            let q = apply(&m, [1.0, 2.0, 0.0]);
            assert!((q[0] - to_mm(on_layout.x)).abs() < 1e-9 && (q[1] - to_mm(on_layout.y)).abs() < 1e-9, "{side:?} {q:?} {on_layout:?}");
            // Up from the footprint is away from its face.
            let up = apply(&m, [1.0, 2.0, 1.0]);
            match side {
                Side::Top => assert!((q[2] - 1.6).abs() < 1e-12 && up[2] > q[2]),
                Side::Bottom => assert!(q[2].abs() < 1e-12 && up[2] < q[2]),
            }
            assert!((m.linear.determinant() - 1.0).abs() < 1e-12);
        }
    }

    #[test]
    fn a_mirroring_motion_keeps_triangles_facing_out() {
        let m = Mesh { color: [1, 2, 3], positions: vec![[0.0, 0.0, 0.0], [1.0, 0.0, 0.0], [0.0, 1.0, 0.0]], normals: vec![[0.0, 0.0, 1.0]; 3], indices: vec![0, 1, 2], edges: vec![] };
        let mirror = Motion { linear: Matrix3::from_diagonal(&Vector3::new(-1.0, 1.0, 1.0)), translation: Vector3::zeros() };
        let r = moved_mesh(&m, &mirror);
        let p = |i: u32| Vector3::new(r.positions[i as usize][0] as f64, r.positions[i as usize][1] as f64, r.positions[i as usize][2] as f64);
        let n = (p(r.indices[1]) - p(r.indices[0])).cross(&(p(r.indices[2]) - p(r.indices[0])));
        assert!(n.z > 0.0);
    }

    #[test]
    fn a_generated_model_is_in_the_footprint_frame() {
        let lib = cadrs_eda::library::LibraryTable::builtin();
        let fp = lib.footprint("Resistor_SMD:R_0805_2012Metric").unwrap();
        let (meshes, opacity) = footprint_meshes(fp).unwrap().unwrap();
        assert_eq!(opacity, 1.0);
        let zs: Vec<f32> = meshes.iter().flat_map(|m| m.positions.iter().map(|p| p[2])).collect();
        let (lo, hi) = (zs.iter().cloned().fold(f32::MAX, f32::min), zs.iter().cloned().fold(f32::MIN, f32::max));
        assert!(lo.abs() < 0.05 && hi > 0.3 && hi < 1.0, "{lo} {hi}");
    }
}

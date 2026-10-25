//! Where does the wrong difference volume come from?
//!
//! A coplanar difference reports 1053 mm³ instead of 840. This measures each shell of the boolean
//! result separately (signed volume, triangle count) so the fault is localized to truck's result
//! or to this adapter's mesh accumulation.
//!
//! Run: `cargo run --release -p cad-kernel-truck --example probe6`

use truck_meshalgo::prelude::*;
use truck_modeling::{Point3, Solid, Vector3, Wire, builder};

fn prism(s: f64, x: f64, y: f64, z0: f64, h: f64) -> Option<Solid> {
    let pts = [
        Point3::new(x, y, z0),
        Point3::new(x + s, y, z0),
        Point3::new(x + s, y + s, z0),
        Point3::new(x, y + s, z0),
    ];
    let vertices: Vec<_> = pts.iter().map(|p| builder::vertex(*p)).collect();
    let edges: Vec<_> = (0..4)
        .map(|i| builder::line(&vertices[i], &vertices[(i + 1) % 4]))
        .collect();
    let wire: Wire = edges.into_iter().collect();
    let face = builder::try_attach_plane(&[wire]).ok()?;
    Some(builder::tsweep(&face, Vector3::new(0.0, 0.0, h)))
}

/// Signed volume of one polygon mesh (divergence theorem over its triangles).
fn mesh_volume(mesh: &PolygonMesh) -> f64 {
    let positions = mesh.positions();
    let mut six_v = 0.0;
    for tri in mesh.tri_faces() {
        let (Some(a), Some(b), Some(c)) = (
            positions.get(tri[0].pos),
            positions.get(tri[1].pos),
            positions.get(tri[2].pos),
        ) else {
            continue;
        };
        six_v += a.x * (b.y * c.z - b.z * c.y) - a.y * (b.x * c.z - b.z * c.x)
            + a.z * (b.x * c.y - b.y * c.x);
    }
    six_v / 6.0
}

fn report(label: &str, solid: &Solid, tol: f64) {
    let shells = solid.boundaries();
    println!("{label}: {} shell(s)", shells.len());
    let mut total = 0.0;
    let mut triangles = 0;
    for (i, shell) in shells.iter().enumerate() {
        let meshed = shell.robust_triangulation(tol);
        let mut shell_volume = 0.0;
        let mut count = 0;
        let mut patches = 0;
        for face in meshed.face_iter() {
            let Some(polygon) = face.surface() else {
                continue;
            };
            patches += 1;
            count += polygon.tri_faces().len();
            shell_volume += mesh_volume(&polygon);
        }
        println!(
            "  shell {i}: {patches} face patches, {count} triangles, signed volume {shell_volume:.6}"
        );
        total += shell_volume;
        triangles += count;
    }
    println!("  total signed volume {total:.6}, {triangles} triangles");
}

fn main() {
    let tol = 0.05;
    let Some(plate) = prism(10.0, 0.0, 0.0, 0.0, 10.0) else {
        println!("plate failed");
        return;
    };
    println!(
        "# plate 10x10x10 = 1000 mm³; tool 4x4x10 at (3,3) = 160 mm³; expected difference 840 mm³"
    );
    report("plate", &plate, tol);

    // Coplanar tool, nudged up so the boolean succeeds (the adapter's retry).
    let Some(tool) = prism(4.0, 3.0, 3.0, 1e-4, 10.0) else {
        return;
    };
    let mut neg = tool.clone();
    neg.not();
    match truck_shapeops::and(&plate, &neg, 1e-3) {
        Some(result) => {
            let result = builder::translated(&result, Vector3::new(0.0, 0.0, -1e-4));
            println!();
            report("difference result", &result, tol);
            println!("expected 840 plus/minus the 1e-4 nudge (about 0.0016 mm³)");
        }
        None => println!("difference failed"),
    }

    // Contained, no coplanar faces: the case that reported two shells.
    let Some(inner) = prism(4.0, 3.0, 3.0, 2.0, 6.0) else {
        return;
    };
    let mut neg = inner.clone();
    neg.not();
    match truck_shapeops::and(&plate, &neg, 1e-3) {
        Some(result) => {
            println!();
            report("void result (tool z 2..8, volume 96)", &result, tol);
            println!("expected 904 mm³");
        }
        None => println!("void difference failed"),
    }

    // Piercing tool: no coplanar faces, expected 1000 - 4*4*10(through) = 840 as well.
    let Some(pierce) = prism(4.0, 3.0, 3.0, -1.0, 12.0) else {
        return;
    };
    let mut neg = pierce.clone();
    neg.not();
    match truck_shapeops::and(&plate, &neg, 1e-3) {
        Some(result) => {
            println!();
            report("piercing result (tool z -1..11)", &result, tol);
            println!("expected 840 mm³");
        }
        None => println!("piercing difference failed"),
    }
}

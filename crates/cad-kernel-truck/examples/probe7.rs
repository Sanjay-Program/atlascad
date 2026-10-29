//! What exactly is wrong with the cut's *tessellation*?
//!
//! The corpus now measures the through-cut volume correctly (84 000 mm³) after the adapter's weld +
//! orientation repair, but the surface area comes out 3 200 mm² too high at plate scale — exactly
//! twice the 40x40 hole. This dumps the mesh the adapter returns, grouped by connected component
//! and by the plane each triangle lies in, so the excess is attributed rather than guessed.
//!
//! Run: `cargo run --release -p cad-kernel-truck --example probe7`

use cad_contract::kernel::{BooleanOp, Kernel};
use cad_kernel_truck::TruckKernel;

fn rect(w: f64, h: f64, x: f64, y: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + w, y], [x + w, y + h], [x, y + h]]
}

fn triangle_area(a: [f64; 3], b: [f64; 3], c: [f64; 3]) -> f64 {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    let n = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    0.5 * (n[0] * n[0] + n[1] * n[1] + n[2] * n[2]).sqrt()
}

fn main() {
    let mut kernel = TruckKernel::new();
    let plate = match kernel.extrude(&rect(100.0, 100.0, 0.0, 0.0), 10.0) {
        Ok(id) => id,
        Err(e) => {
            println!("plate failed: {e}");
            return;
        }
    };
    let tool = match kernel.extrude(&rect(40.0, 40.0, 30.0, 30.0), 10.0) {
        Ok(id) => id,
        Err(e) => {
            println!("tool failed: {e}");
            return;
        }
    };
    let cut = match kernel.boolean(BooleanOp::Difference, plate, tool) {
        Ok(id) => id,
        Err(e) => {
            println!("difference failed: {e}");
            return;
        }
    };
    println!("difference volume: {:?}", kernel.volume(cut));

    let mesh = match kernel.tessellate(cut, 0.01) {
        Ok(mesh) => mesh,
        Err(e) => {
            println!("tessellate failed: {e}");
            return;
        }
    };
    println!(
        "mesh: {} vertices, {} triangles, area {:.6}, signed volume {:.6}, open edges {}",
        mesh.positions.len(),
        mesh.triangles.len(),
        mesh.surface_area(),
        mesh.signed_volume(),
        mesh.open_edges()
    );

    // Group triangles by the plane they lie in: every face of this fixture is axis-aligned.
    let mut z0 = 0u32;
    let mut z10 = 0u32;
    let mut x_planes = 0u32;
    let mut y_planes = 0u32;
    let mut slanted = 0u32;
    let mut z0_area = 0.0;
    let mut z10_area = 0.0;
    for tri in &mesh.triangles {
        let a = mesh.positions[tri[0] as usize];
        let b = mesh.positions[tri[1] as usize];
        let c = mesh.positions[tri[2] as usize];
        let area = triangle_area(a, b, c);
        let flat = |p: [f64; 3], v: f64| (p[0] - v).abs() < 1e-6;
        if flat(a, 0.0) && flat(b, 0.0) && flat(c, 0.0) {
            z0 += 1;
            z0_area += area;
        } else if (a[2] - 10.0).abs() < 1e-6
            && (b[2] - 10.0).abs() < 1e-6
            && (c[2] - 10.0).abs() < 1e-6
        {
            z10 += 1;
            z10_area += area;
        } else if (a[0] - b[0]).abs() < 1e-6 && (a[0] - c[0]).abs() < 1e-6 {
            x_planes += 1;
        } else if (a[1] - b[1]).abs() < 1e-6 && (a[1] - c[1]).abs() < 1e-6 {
            y_planes += 1;
        } else {
            slanted += 1;
        }
    }
    println!("triangles on z=0: {z0} (area {z0_area:.3}), on z=10: {z10} (area {z10_area:.3})");
    println!(
        "triangles on x=const: {x_planes}, on y=const: {y_planes}, non-axis-aligned: {slanted}"
    );

    // Every triangle whose centroid lies inside the hole's xy footprint but is NOT on a hole wall
    // is a face covering the hole, which is what would inflate the cap area.
    let mut hole_cover = 0u32;
    let mut hole_cover_area = 0.0;
    for tri in &mesh.triangles {
        let a = mesh.positions[tri[0] as usize];
        let b = mesh.positions[tri[1] as usize];
        let c = mesh.positions[tri[2] as usize];
        let mid = [
            (a[0] + b[0] + c[0]) / 3.0,
            (a[1] + b[1] + c[1]) / 3.0,
            (a[2] + b[2] + c[2]) / 3.0,
        ];
        let inside_hole = mid[0] > 30.0 + 1e-6
            && mid[0] < 70.0 - 1e-6
            && mid[1] > 30.0 + 1e-6
            && mid[1] < 70.0 - 1e-6;
        let on_wall = (mid[0] - 30.0).abs() < 1e-6
            || (mid[0] - 70.0).abs() < 1e-6
            || (mid[1] - 30.0).abs() < 1e-6
            || (mid[1] - 70.0).abs() < 1e-6;
        if inside_hole && !on_wall {
            hole_cover += 1;
            hole_cover_area += triangle_area(a, b, c);
            println!(
                "  hole-covering triangle: area {:.6} centroid {:?}",
                triangle_area(a, b, c),
                mid
            );
        }
    }
    println!("triangles covering the hole: {hole_cover} (area {hole_cover_area:.3})");

    // Exactly which edges are open? A watertight mesh has none, so the count alone is not enough
    // to judge whether the defect is the nudged membrane or a torn hole wall.
    use std::collections::BTreeMap;
    let mut counts: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for tri in &mesh.triangles {
        for (a, b) in [(tri[0], tri[1]), (tri[1], tri[2]), (tri[2], tri[0])] {
            let key = if a <= b { (a, b) } else { (b, a) };
            *counts.entry(key).or_insert(0) += 1;
        }
    }
    for (edge, count) in counts.iter().filter(|(_, c)| **c != 2) {
        let a = mesh.positions[edge.0 as usize];
        let b = mesh.positions[edge.1 as usize];
        println!("  edge {:?}->{:?} used {count} time(s)", a, b);
    }
}

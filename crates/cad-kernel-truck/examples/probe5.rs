//! Test the coplanar-face hypothesis.
//!
//! Every failing case so far has two operands whose top and/or bottom faces are **coplanar and
//! overlapping** (both extruded from z = 0 to the same height). The one succeeding case (truck's
//! "punched cube") used a tool that pierces the target, so no faces were coplanar.
//!
//! This program varies only the tool's z placement, keeping everything else identical.
//!
//! Run: `cargo run --release -p cad-kernel-truck --example probe5`

use truck_modeling::{Point3, Solid, Vector3, Wire, builder};

/// A rectangular prism from (`x`,`y`,`z0`) with side `s` and height `h`, built the way this
/// adapter's `extrude` builds a solid.
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

fn describe(label: &str, result: Option<Solid>) {
    match result {
        Some(solid) => println!("{label:<44} OK      shells {}", solid.boundaries().len()),
        None => println!("{label:<44} FAILED"),
    }
}

fn main() {
    let tol = 1e-3;
    let Some(plate) = prism(10.0, 0.0, 0.0, 0.0, 10.0) else {
        println!("could not build the plate");
        return;
    };
    println!("# plate 10x10x10 at z 0..10; tool 4x4 at (3,3); tolerance {tol:e}");
    println!();

    // A: tool exactly coplanar with the plate (z 0..10) — the case that fails.
    if let Some(tool) = prism(4.0, 3.0, 3.0, 0.0, 10.0) {
        let mut neg = tool.clone();
        neg.not();
        describe(
            "A coplanar z 0..10      diff  and(a,¬b)",
            truck_shapeops::and(&plate, &neg, tol),
        );
        describe(
            "A coplanar z 0..10      inter and(a,b)",
            truck_shapeops::and(&plate, &tool, tol),
        );
        describe(
            "A coplanar z 0..10      union or(a,b)",
            truck_shapeops::or(&plate, &tool, tol),
        );
    }

    // B: tool pierces through the plate (z -1..11) — no coplanar faces.
    if let Some(tool) = prism(4.0, 3.0, 3.0, -1.0, 12.0) {
        let mut neg = tool.clone();
        neg.not();
        describe(
            "B piercing z -1..11     diff  and(a,¬b)",
            truck_shapeops::and(&plate, &neg, tol),
        );
        describe(
            "B piercing z -1..11     inter and(a,b)",
            truck_shapeops::and(&plate, &tool, tol),
        );
        describe(
            "B piercing z -1..11     union or(a,b)",
            truck_shapeops::or(&plate, &tool, tol),
        );
    }

    // C: tool fully inside the plate (z 2..8) — a void, no coplanar faces, no boundary crossing.
    if let Some(tool) = prism(4.0, 3.0, 3.0, 2.0, 6.0) {
        let mut neg = tool.clone();
        neg.not();
        describe(
            "C contained z 2..8      diff  and(a,¬b)",
            truck_shapeops::and(&plate, &neg, tol),
        );
        describe(
            "C contained z 2..8      inter and(a,b)",
            truck_shapeops::and(&plate, &tool, tol),
        );
    }

    // D: tool touches only the top face (z 8..12): one coplanar pair avoided, one crossing.
    if let Some(tool) = prism(4.0, 3.0, 3.0, 8.0, 4.0) {
        let mut neg = tool.clone();
        neg.not();
        describe(
            "D partial z 8..12       diff  and(a,¬b)",
            truck_shapeops::and(&plate, &neg, tol),
        );
        describe(
            "D partial z 8..12       union or(a,b)",
            truck_shapeops::or(&plate, &tool, tol),
        );
    }

    // E: the workaround. Keep the tool coplanar, but move **only** the tool by a sub-tolerance
    // amount in z so no faces are coplanar, run the boolean, then move the result back.
    println!();
    println!("# E: coplanar case with only the tool nudged in z, result translated back");
    for dz in [1e-6_f64, 1e-5, 1e-4, 1e-3, 1e-2] {
        if let Some(tool) = prism(4.0, 3.0, 3.0, dz, 10.0) {
            let mut neg = tool.clone();
            neg.not();
            let result = truck_shapeops::and(&plate, &neg, tol)
                .map(|s| builder::translated(&s, Vector3::new(0.0, 0.0, -dz)));
            match result {
                Some(solid) => println!(
                    "nudge dz {dz:e}                           OK      shells {}",
                    solid.boundaries().len()
                ),
                None => println!("nudge dz {dz:e}                           FAILED"),
            }
        }
    }
}

//! Probe stock truck's boolean behaviour to establish exactly what this adapter can claim.
//!
//! Motivation: the first benchmark run produced no solids, while the unit tests appeared to pass.
//! The tests were at fault (they used `let Ok(..) = .. else { return }`, which turns a failure into
//! a pass). This program establishes the real behaviour empirically:
//!
//! 1. all three operations on overlapping boxes built with `tsweep`;
//! 2. the same operations across a tolerance sweep;
//! 3. `and`/`or` on the shape construction style truck's own tests use, as a control.
//!
//! Run: `cargo run --release -p cad-kernel-truck --example probe`

use cad_contract::kernel::{BooleanOp, Kernel};
use cad_kernel_truck::TruckKernel;
use truck_modeling::{Point3, Vector3, builder};

fn square(size: f64, x: f64, y: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
}

/// Run one boolean and describe the outcome, checking the volume against `expected`.
fn boolean_case(label: &str, op: BooleanOp, a: f64, b: f64, tol: f64, expected: f64) {
    let mut kernel = TruckKernel::new().with_boolean_tolerance(tol);
    let Ok(id_a) = kernel.extrude(&square(10.0, 0.0, 0.0), 10.0) else {
        println!("{label:<46} plate extrusion failed");
        return;
    };
    let Ok(id_b) = kernel.extrude(&square(10.0, a, b), 10.0) else {
        println!("{label:<46} tool extrusion failed");
        return;
    };
    match kernel.boolean(op, id_a, id_b) {
        Ok(id) => {
            let v = kernel.volume(id).unwrap_or(f64::NAN);
            let error = (v - expected).abs();
            let verdict = if error < 1.0 { "OK   " } else { "WRONG" };
            println!("{label:<46} {verdict} volume {v:>10.3} expected {expected:>10.3}");
        }
        Err(e) => {
            let text = e.to_string();
            let first = text.split(';').next().unwrap_or(&text);
            println!("{label:<46} FAILED {first}");
        }
    }
}

/// Build a 10 mm cube the way truck's own tests and docs do (vertex → edge → face → solid), then
/// intersect it with a copy offset in x. This is a control for the shape-construction style.
fn control_cube_and(tol: f64) {
    let cube = || {
        let v = builder::vertex(Point3::new(0.0, 0.0, 0.0));
        let e = builder::tsweep(&v, Vector3::new(10.0, 0.0, 0.0));
        let f = builder::tsweep(&e, Vector3::new(0.0, 10.0, 0.0));
        builder::tsweep(&f, Vector3::new(0.0, 0.0, 10.0))
    };
    let a = cube();
    let b = builder::translated(&cube(), Vector3::new(5.0, 0.0, 0.0));
    match truck_shapeops::and(&a, &b, tol) {
        Some(_) => println!(
            "{:<46} OK    (control: tsweep cube, and)",
            format!("control and, tol {tol:.0e}")
        ),
        None => println!(
            "{:<46} FAILED (control: tsweep cube, and)",
            format!("control and, tol {tol:.0e}")
        ),
    }
}

fn main() {
    println!("# boolean probe: two 10 x 10 x 10 mm boxes, second offset by (5, 5) mm");
    println!("# analytic: union 1500, intersection 500, difference 500 mm³");
    println!();

    println!("## all three operations, boolean tolerance 1e-3 mm");
    boolean_case("union  A ∪ B", BooleanOp::Union, 5.0, 5.0, 1e-3, 1500.0);
    boolean_case(
        "inter  A ∩ B",
        BooleanOp::Intersection,
        5.0,
        5.0,
        1e-3,
        500.0,
    );
    boolean_case("diff   A − B", BooleanOp::Difference, 5.0, 5.0, 1e-3, 500.0);

    println!();
    println!("## difference, tolerance sweep");
    for tol in [1e-1, 5e-2, 1e-2, 1e-3, 1e-4] {
        boolean_case(
            &format!("diff   A − B, tol {tol:.0e}"),
            BooleanOp::Difference,
            5.0,
            5.0,
            tol,
            500.0,
        );
    }

    println!();
    println!("## intersection, tolerance sweep");
    for tol in [1e-1, 5e-2, 1e-2, 1e-3, 1e-4] {
        boolean_case(
            &format!("inter  A ∩ B, tol {tol:.0e}"),
            BooleanOp::Intersection,
            5.0,
            5.0,
            tol,
            500.0,
        );
    }

    println!();
    println!("## control: truck's own cube construction, stock `and`");
    for tol in [5e-2, 1e-2, 1e-3] {
        control_cube_and(tol);
    }
}

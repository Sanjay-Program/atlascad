//! Diagnostic: cut nine square holes through a plate one at a time, reporting each step.
//!
//! This exists because the benchmark found that a *chain* of booleans can fail even though each
//! single boolean succeeds. It prints which step fails and what the kernel says, so the limitation
//! can be documented with evidence instead of guesswork.
//!
//! Run: `cargo run --release -p cad-kernel-truck --example chain`

use cad_contract::kernel::{BooleanOp, Kernel};
use cad_kernel_truck::TruckKernel;

fn square(size: f64, x: f64, y: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
}

fn main() {
    let mut kernel = TruckKernel::new();

    // Chain of unions: 10 mm boxes offset by 5 mm in x, each overlapping the previous by half.
    // The union of n boxes spans 10 + 5*(n-1) mm, so volume = 10 * span * 10.
    let mut current = match kernel.extrude(&square(10.0, 0.0, 0.0), 10.0) {
        Ok(id) => id,
        Err(e) => {
            println!("first box extrusion failed: {e}");
            return;
        }
    };
    println!(
        "box 0 volume {:.3} (expected 1000)",
        kernel.volume(current).unwrap_or(f64::NAN)
    );

    for i in 1..9 {
        let x = i as f64 * 5.0;
        let next = match kernel.extrude(&square(10.0, x, 0.0), 10.0) {
            Ok(id) => id,
            Err(e) => {
                println!("box {i} extrusion failed: {e}");
                break;
            }
        };
        match kernel.boolean(BooleanOp::Union, current, next) {
            Ok(joined) => {
                current = joined;
                let expected = 10.0 * (10.0 + 5.0 * i as f64) * 10.0;
                println!(
                    "union {i} at x={x} -> ok, volume {:.3} (expected {expected:.0}), retries so far {}",
                    kernel.volume(current).unwrap_or(f64::NAN),
                    kernel.coplanar_retries()
                );
            }
            Err(e) => {
                println!("union {i} at x={x} -> FAILED: {e}");
                break;
            }
        }
    }
    println!("final live solids: {}", kernel.live_solids());
}

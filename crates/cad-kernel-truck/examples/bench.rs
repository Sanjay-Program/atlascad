//! A benchmark and correctness harness for the truck adapter.
//!
//! Deliberately dependency-free (std only) so it can run on a constrained machine without pulling
//! in Criterion. It reports:
//!
//! * wall-clock time per operation class (extrude, boolean, tessellate);
//! * **correctness against analytic values** — a fast wrong answer is worthless;
//! * triangle counts per tolerance, which is the real cost driver for rendering and memory;
//! * **run-to-run stability**, which is where truck's upstream boolean is known to vary (see
//!   `apps/solidcraft/vendor/README.md`: upstream face order followed hash order of addresses);
//! * peak RSS of the process, because this machine's limit is memory, not CPU.
//!
//! Run with `cargo run --release -p cad-kernel-truck --example bench`.
//!
//! Methodology notes that must accompany any number taken from this program:
//! release build only, single-threaded (truck booleans are not reentrant), warm-up run discarded,
//! `--iters` repeats with the median reported, and the exact geometry described in each scenario.

use std::time::{Duration, Instant};

use cad_contract::kernel::{BooleanOp, Kernel};
use cad_kernel_truck::TruckKernel;

/// A square profile of side `size` with its lower-left corner at (`x`, `y`).
fn square(size: f64, x: f64, y: f64) -> Vec<[f64; 2]> {
    vec![[x, y], [x + size, y], [x + size, y + size], [x, y + size]]
}

/// Run `f`, returning its result and how long it took.
fn timed<T>(f: impl FnOnce() -> T) -> (T, Duration) {
    let start = Instant::now();
    let out = f();
    (out, start.elapsed())
}

fn median(mut samples: Vec<Duration>) -> Duration {
    if samples.is_empty() {
        return Duration::ZERO;
    }
    samples.sort_unstable();
    samples[samples.len() / 2]
}

/// Peak resident set size in MiB, from `/proc/self/status` (`VmHWM`), or `None` off Linux.
fn peak_rss_mib() -> Option<f64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    for line in status.lines() {
        if let Some(rest) = line.strip_prefix("VmHWM:") {
            let kib: f64 = rest.split_whitespace().next()?.parse().ok()?;
            return Some(kib / 1024.0);
        }
    }
    None
}

fn main() {
    let iters: usize = std::env::args()
        .position(|a| a == "--iters")
        .and_then(|i| std::env::args().nth(i + 1))
        .and_then(|v| v.parse().ok())
        .unwrap_or(5)
        .max(1);

    // Warm-up: the first truck call in a process pays one-time initialisation.
    {
        let mut kernel = TruckKernel::new();
        if let Ok(id) = kernel.extrude(&square(10.0, 0.0, 0.0), 5.0) {
            let _ = kernel.tessellate(id, 0.05);
        }
    }

    println!("# truck adapter benchmark");
    println!("# iters = {iters} (median reported), release build, single thread");
    if let Some(rss) = peak_rss_mib() {
        println!("# peak RSS so far: {rss:.1} MiB");
    }
    println!("# kernel advertises: {}", {
        let k = TruckKernel::new();
        let info = k.info();
        let mut names: Vec<&str> = info.capabilities.iter().map(|c| c.label()).collect();
        names.sort_unstable();
        names.join(", ")
    });
    println!();

    // ---------------------------------------------------------------- extrude
    // 100 boxes of 10 x 10 x 5 mm. Analytic total volume: 100 * 500 = 50 000 mm³.
    const BOXES: usize = 100;
    let mut samples = Vec::with_capacity(iters);
    let mut total_volume = 0.0;
    let mut volume_checked = true;
    for _ in 0..iters {
        let mut kernel = TruckKernel::new();
        let (made, elapsed) = timed(|| {
            let mut ids = Vec::with_capacity(BOXES);
            for i in 0..BOXES {
                let x = (i % 10) as f64 * 20.0;
                let y = (i / 10) as f64 * 20.0;
                match kernel.extrude(&square(10.0, x, y), 5.0) {
                    Ok(id) => ids.push(id),
                    Err(_) => return (ids, false),
                }
            }
            (ids, true)
        });
        let (ids, complete) = made;
        samples.push(elapsed);
        if complete {
            total_volume = ids.iter().map(|id| kernel.volume(*id).unwrap_or(0.0)).sum();
        } else {
            volume_checked = false;
        }
    }
    println!(
        "extrude           {BOXES} boxes            median {:>8.2} ms   ({:.0} boxes/s)",
        median(samples.clone()).as_secs_f64() * 1e3,
        BOXES as f64 / median(samples).as_secs_f64()
    );
    if volume_checked {
        let error = (total_volume - 50_000.0).abs();
        println!(
            "  correctness      volume {total_volume:.3} mm³ vs analytic 50000 mm³ (abs error {error:.6})"
        );
    }

    // -------------------------------------------------------------- booleans
    // Pairwise unions of two verified operands: a 10 x 10 x 10 mm box and a 4 x 4 x 10 mm box
    // overlapping by 2 x 2 x 10. Analytic union volume: 1000 + 160 - 40 = 1120 mm³.
    //
    // Pairwise rather than a chain: chaining a third union panics inside truck's intersection-curve
    // code (`tolerance must be no less than 1e-6`), which the adapter reports as `Internal` rather
    // than crashing. See docs/kernel-comparison.md. Union is used rather than difference because
    // difference returns malformed topology and is not advertised.
    const UNIONS: usize = 20;
    let mut samples = Vec::with_capacity(iters);
    let mut union_volumes = Vec::new();
    let mut retries = 0;
    for _ in 0..iters {
        let mut kernel = TruckKernel::new();
        let Ok(a) = kernel.extrude(&square(10.0, 0.0, 0.0), 10.0) else {
            continue;
        };
        let Ok(b) = kernel.extrude(&square(4.0, 8.0, 8.0), 10.0) else {
            continue;
        };
        let (made, elapsed) = timed(|| {
            let mut out = Vec::with_capacity(UNIONS);
            for _ in 0..UNIONS {
                match kernel.boolean(BooleanOp::Union, a, b) {
                    Ok(id) => out.push(id),
                    Err(_) => break,
                }
            }
            out
        });
        samples.push(elapsed);
        for id in &made {
            if let Ok(v) = kernel.volume(*id) {
                union_volumes.push(v);
            }
            kernel.release(*id);
        }
        retries += kernel.coplanar_retries();
    }
    let median_union = median(samples).as_secs_f64() * 1e3;
    println!(
        "boolean union     {UNIONS} pairwise unions median {median_union:>8.2} ms total ({:.2} ms each), {} results",
        median_union / UNIONS as f64,
        union_volumes.len()
    );
    if let (Some(min), Some(max)) = (
        union_volumes.iter().copied().reduce(f64::min),
        union_volumes.iter().copied().reduce(f64::max),
    ) {
        let error = (min - 1_120.0).abs();
        println!(
            "  correctness      volume min {min:.3} max {max:.3} mm³ vs analytic 1120 mm³ (abs error {error:.6})"
        );
        println!(
            "  stability        volume spread across all runs: {:.9} mm³",
            (max - min).abs()
        );
        println!(
            "  coplanar retries {retries} over {} unions",
            union_volumes.len()
        );
    }

    // ---------------------------------------------------------- tessellation
    // One 100 x 100 x 10 mm plate at four chord tolerances. Volume is 100 000 mm³ at every
    // tolerance: a coarser mesh must not change the enclosed volume, only its facet count.
    println!();
    println!("tessellation      chord tol   median time     triangles   volume error");
    let mut plate_kernel = TruckKernel::new();
    if let Ok(plate) = plate_kernel.extrude(&square(100.0, 0.0, 0.0), 10.0) {
        for tol in [1.0, 0.5, 0.1, 0.05, 0.01] {
            let mut samples = Vec::with_capacity(iters);
            let mut tris = 0;
            let mut error = f64::NAN;
            for _ in 0..iters {
                let (mesh, elapsed) = timed(|| plate_kernel.tessellate(plate, tol));
                samples.push(elapsed);
                if let Ok(mesh) = mesh {
                    tris = mesh.triangle_count();
                    error = (mesh.signed_volume().abs() - 100_000.0).abs();
                    if let Err(e) = mesh.validate() {
                        println!("  mesh at tol {tol} failed validation: {e}");
                    }
                }
            }
            println!(
                "                  {tol:>9.3}   {:>8.2} ms   {tris:>10}   {error:.6} mm³",
                median(samples).as_secs_f64() * 1e3
            );
        }
    } else {
        println!("  plate extrusion failed; tessellation skipped");
    }

    // ------------------------------------------------------------ handle cost
    // Creating and releasing many solids must not leak (checked by live_solids, not by inference).
    let mut kernel = TruckKernel::new();
    let mut ids = Vec::new();
    for i in 0..200 {
        if let Ok(id) = kernel.extrude(&square(1.0, i as f64 * 2.0, 0.0), 1.0) {
            ids.push(id);
        }
    }
    let live_before = kernel.live_solids();
    for id in &ids {
        kernel.release(*id);
    }
    println!();
    println!(
        "handles           created {live_before} solids, live after release {}, ids tracked {}",
        kernel.live_solids(),
        ids.len()
    );

    if let Some(rss) = peak_rss_mib() {
        println!("peak RSS          {rss:.1} MiB");
    }

    // A silent failure is worse than an explicit one: make the harness's exit status meaningful.
    // An empty result list must fail: `all()` over nothing is vacuously true.
    let expected_results = iters * UNIONS;
    if union_volumes.len() < expected_results {
        eprintln!(
            "FAIL: only {} of {expected_results} unions produced a solid",
            union_volumes.len()
        );
        std::process::exit(1);
    }
    if !union_volumes.iter().all(|v| (v - 1_120.0).abs() < 1.0) {
        eprintln!("FAIL: union volume outside 1 mm³ of the analytic 1120 mm³: {union_volumes:?}");
        std::process::exit(1);
    }
}

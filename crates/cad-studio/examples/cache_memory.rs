//! Peak memory of a long feature history, with and without the per-feature rebuild cache.
//!
//! The cache is bounded at 32 prefixes (`MAX_CACHE_ENTRIES`) and each entry owns one B-rep body and
//! its tessellation, so its cost is a real question rather than a footnote. This program builds a
//! history whose prefix meshes are large enough for that cost to show: N overlapping cylinders, each
//! sketched as a 64-point polyline and joined onto the running body. Every prefix is therefore a
//! *different* body, which is the worst case for the cache.
//!
//! Run one mode per process, because peak resident memory is a process-wide high-water mark and two
//! modes in one process cannot be told apart:
//!
//! ```text
//! cargo run --release -p cad-studio --example cache_memory -- uncached 32
//! cargo run --release -p cad-studio --example cache_memory -- cached 32
//! ```
//!
//! Prints the peak RSS (`VmHWM`), the elapsed build time, the final triangle count and, for the
//! cached mode, how many prefixes the cache holds. No number here is asserted by a test; the record
//! is the measurement in `docs/performance-baseline.md` §10.
use cad_contract::Kernel;
use cad_contract::ids::DocumentId;
use cad_contract::units::Unit;
use cad_document::{BuildCache, Project};
use std::time::Instant;

fn circle(radius: f64, points: usize, centre_x: f64) -> Vec<[f64; 2]> {
    (0..points)
        .map(|index| {
            let angle = std::f64::consts::TAU * index as f64 / points as f64;
            [centre_x + radius * angle.cos(), radius * angle.sin()]
        })
        .collect()
}

/// Peak resident set size in kilobytes, from `/proc/self/status`.
fn peak_rss_kb() -> Option<u64> {
    let status = std::fs::read_to_string("/proc/self/status").ok()?;
    status.lines().find_map(|line| {
        let value = line.strip_prefix("VmHWM:")?;
        value.split_whitespace().next()?.parse().ok()
    })
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let mut arguments = std::env::args().skip(1);
    let mode = arguments.next().unwrap_or_else(|| "cached".to_string());
    let features: usize = arguments
        .next()
        .and_then(|value| value.parse().ok())
        .unwrap_or(32);
    let cached = match mode.as_str() {
        "cached" => true,
        "uncached" => false,
        other => {
            eprintln!("unknown mode `{other}`; expected cached or uncached");
            std::process::exit(2);
        }
    };

    let mut project = Project::new("cache-memory", DocumentId::new(1), Unit::Millimeter);
    for index in 0..features {
        let sketch =
            project.add_polyline_sketch(circle(4.0, 64, index as f64 * 6.0), [0.0, 0.0])?;
        if index == 0 {
            project.add_extrude(sketch, "depth")?;
        } else {
            project.add_join_extrude(sketch, "depth")?;
        }
    }

    let mut kernel = cad_kernel_truck::TruckKernel::new();
    let start = Instant::now();
    if cached {
        let mut cache = BuildCache::new();
        let build = project.build_with_cache(&mut kernel, &mut cache)?;
        let elapsed = start.elapsed();
        println!(
            "mode=cached features={features} prefixes={} triangles={} complete={} elapsed_ms={} peak_rss_kb={}",
            cache.len(),
            build.mesh.as_ref().map_or(0, |mesh| mesh.triangle_count()),
            build.is_complete(),
            elapsed.as_millis(),
            peak_rss_kb().map_or("unknown".to_string(), |kb| kb.to_string())
        );
        // Keep the cache alive until after the measurement is read.
        std::hint::black_box(&cache);
    } else {
        let build = project.build(&mut kernel)?;
        let elapsed = start.elapsed();
        if let Some(solid) = build.solid {
            kernel.release(solid);
        }
        println!(
            "mode=uncached features={features} triangles={} complete={} elapsed_ms={} peak_rss_kb={}",
            build.mesh.as_ref().map_or(0, |mesh| mesh.triangle_count()),
            build.is_complete(),
            elapsed.as_millis(),
            peak_rss_kb().map_or("unknown".to_string(), |kb| kb.to_string())
        );
    }
    Ok(())
}

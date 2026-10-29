//! What does a rebuild cost, cold and cached?
//!
//! The 17.3 ms two-feature figure recorded in `docs/performance-baseline.md` §8 was a **cold**
//! rebuild of the whole history — what every HTTP request used to do. Since the studio now keeps a
//! [`BuildCache`], that is no longer what a request does, so this measures both paths on the same
//! workload:
//!
//! * `cold` — `Project::build`, the uncached history rebuild, i.e. the previous behaviour;
//! * `cached, unchanged` — `Project::build_with_cache` on an unedited history, i.e. what
//!   `GET /api/state` now does after the first request;
//! * `cached, last feature edited` — one parameter that only the last feature reads changes, which
//!   is the interactive case: the earlier features must not be re-extruded or re-unioned.
//!
//! Methodology, matching the recorded baseline so the numbers are comparable: release build only,
//! single-threaded, three warm-up rebuilds discarded, 20 timed samples, median reported. The cached
//! edit benchmark changes the parameter before each timed rebuild, so every sample is a real miss
//! on the last feature rather than a free re-read of an unchanged history.
//!
//! Run: `cargo run --release -p cad-studio --example rebuild_bench`

use cad_contract::ids::DocumentId;
use cad_contract::kernel::Kernel;
use cad_contract::units::Unit;
use cad_document::{BuildCache, ExtrudeOperation, Project};
use cad_kernel_truck::TruckKernel;
use std::time::{Duration, Instant};

/// Build `project` `samples` times with no cache, returning the median rebuild time and the volume.
fn time_cold(
    project: &Project,
    samples: usize,
) -> Result<(Duration, f64), Box<dyn std::error::Error>> {
    let mut kernel = TruckKernel::new();
    // Warm-up: the first rebuild pays for page faults and any lazily-initialised state in truck.
    for _ in 0..3 {
        let build = project.build(&mut kernel)?;
        if let Some(solid) = build.solid {
            kernel.release(solid);
        }
    }
    let mut times = Vec::with_capacity(samples);
    let mut volume = 0.0;
    for _ in 0..samples {
        let start = Instant::now();
        let build = project.build(&mut kernel)?;
        times.push(start.elapsed());
        if let Some(failure) = &build.failed {
            return Err(format!("rebuild failed: {}", failure.message).into());
        }
        volume = build
            .mesh
            .map(|mesh| mesh.signed_volume())
            .ok_or("rebuild produced no mesh")?;
        if let Some(solid) = build.solid {
            kernel.release(solid);
        }
    }
    Ok((median(times), volume))
}

/// Rebuild an unedited project `samples` times through the cache.
fn time_cached_unchanged(
    project: &Project,
    samples: usize,
) -> Result<(Duration, f64), Box<dyn std::error::Error>> {
    let mut kernel = TruckKernel::new();
    let mut cache = BuildCache::new();
    for _ in 0..3 {
        project.build_with_cache(&mut kernel, &mut cache)?;
    }
    let mut times = Vec::with_capacity(samples);
    let mut volume = 0.0;
    for _ in 0..samples {
        let start = Instant::now();
        let build = project.build_with_cache(&mut kernel, &mut cache)?;
        times.push(start.elapsed());
        if let Some(failure) = &build.failed {
            return Err(format!("rebuild failed: {}", failure.message).into());
        }
        volume = build
            .mesh
            .map(|mesh| mesh.signed_volume())
            .ok_or("rebuild produced no mesh")?;
    }
    cache.clear(&mut kernel);
    Ok((median(times), volume))
}

/// Change `parameter` before each timed rebuild, so every sample is a miss on the features that
/// read it and a hit on every feature before them.
fn time_cached_edit(
    project: &mut Project,
    parameter: &str,
    samples: usize,
) -> Result<(Duration, f64), Box<dyn std::error::Error>> {
    let mut kernel = TruckKernel::new();
    let mut cache = BuildCache::new();
    let base = project.parameter(parameter)?;
    for step in 0..3 {
        project.set_parameter(parameter, base + f64::from(step) * 0.1)?;
        project.build_with_cache(&mut kernel, &mut cache)?;
    }
    let resumed = project.parameter(parameter)?;
    let mut times = Vec::with_capacity(samples);
    let mut volume = 0.0;
    for sample in 0..samples {
        // A different value each time, so no sample can be served by a cache entry the previous
        // sample created.
        project.set_parameter(parameter, resumed + (sample as f64 + 1.0) * 0.1)?;
        let start = Instant::now();
        let build = project.build_with_cache(&mut kernel, &mut cache)?;
        times.push(start.elapsed());
        if let Some(failure) = &build.failed {
            return Err(format!("rebuild failed: {}", failure.message).into());
        }
        volume = build
            .mesh
            .map(|mesh| mesh.signed_volume())
            .ok_or("rebuild produced no mesh")?;
    }
    cache.clear(&mut kernel);
    Ok((median(times), volume))
}

fn median(mut times: Vec<Duration>) -> Duration {
    times.sort_unstable();
    times
        .get(times.len() / 2)
        .copied()
        .unwrap_or(Duration::ZERO)
}

fn ms(duration: Duration) -> f64 {
    duration.as_secs_f64() * 1000.0
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let samples = 20;

    // Two features: a 10 x 10 x 5 block, then a second sketch. The second sketch covers x,y in
    // 3..13, so it overlaps the block by 7 x 7.
    let mut join = Project::box_part(
        "join",
        DocumentId::new(1),
        Unit::Millimeter,
        [10.0, 10.0, 5.0],
    )?;
    let sketch = join.add_rectangle_sketch("width", "height", [3.0, 3.0])?;
    join.add_extrude_feature(sketch, "depth", ExtrudeOperation::Join)?;

    let mut cut = Project::box_part(
        "cut",
        DocumentId::new(2),
        Unit::Millimeter,
        [10.0, 10.0, 5.0],
    )?;
    let sketch = cut.add_rectangle_sketch("width", "height", [3.0, 3.0])?;
    cut.add_extrude_feature(sketch, "depth", ExtrudeOperation::Cut)?;

    // Three features: the cut project plus a join that puts material back on the far side.
    let mut mixed = cut.clone();
    let patch = mixed.add_rectangle_sketch("width", "height", [-20.0, 0.0])?;
    mixed.add_extrude_feature(patch, "depth", ExtrudeOperation::Join)?;

    // A two-feature project whose last feature reads its own parameter (`diameter`), so editing it
    // is the interactive case: only the last feature can change.
    let mut isolated = Project::box_part(
        "isolated",
        DocumentId::new(3),
        Unit::Millimeter,
        [10.0, 10.0, 5.0],
    )?;
    let sketch = isolated.add_rectangle_sketch("width", "height", [3.0, 3.0])?;
    isolated.add_extrude_feature(sketch, "diameter", ExtrudeOperation::Join)?;

    // A long history: one body and five joins, each on its own sketch, with the last feature's
    // depth on `diameter` so only it is invalidated by an edit. This is where a prefix cache should
    // show its scaling: a cold rebuild runs six extrudes and five unions, an edit of the last
    // feature runs one extrude and one union.
    let mut long = Project::box_part(
        "long",
        DocumentId::new(4),
        Unit::Millimeter,
        [10.0, 10.0, 5.0],
    )?;
    for step in 0..5 {
        let sketch = long.add_rectangle_sketch("width", "height", [2.0 * step as f64, 2.0])?;
        let depth_param = if step == 4 { "diameter" } else { "depth" };
        long.add_extrude_feature(sketch, depth_param, ExtrudeOperation::Join)?;
    }

    println!(
        "{:<34} {:>10} {:>10}   volume",
        "workload", "cold (ms)", "cached ms"
    );
    println!("{}", "-".repeat(76));

    for (label, project, parameter) in [
        ("join (2 features)", &join, "depth"),
        ("cut (2 features)", &cut, "depth"),
        ("cut+join (3 features)", &mixed, "depth"),
        ("join, last feature isolated", &isolated, "diameter"),
        ("6 features, last isolated", &long, "diameter"),
    ] {
        let (cold, cold_volume) = time_cold(project, samples)?;
        let (unchanged, _) = time_cached_unchanged(project, samples)?;
        let mut editable = project.clone();
        let (edited, edited_volume) = time_cached_edit(&mut editable, parameter, samples)?;
        println!(
            "{label:<34} {:>10.2} {:>10.2}   {cold_volume:.3} mm³ cold / {edited_volume:.3} mm³ after edit",
            ms(cold),
            ms(edited)
        );
        println!(
            "{:<34} {:>10} {:>10.2}   unchanged history",
            "",
            "(unchanged)",
            ms(unchanged)
        );
    }
    Ok(())
}

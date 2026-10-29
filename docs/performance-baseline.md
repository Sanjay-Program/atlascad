# Performance Baseline

**Measured:** 2026-10-21 on the development machine, release build, single thread.
**Purpose:** a reproducible *before* picture. No optimisation is claimed in this milestone, so there
are no "after" numbers and no speedup multipliers.

---

## 1. Method (read this before quoting any number)

| Item | Value |
|---|---|
| Hardware | 11th Gen Intel Core **i3-1125G4**, 4 cores / 8 threads, 2.00 GHz base |
| RAM | **7.4 GiB**, swap 7.4 GiB (largely consumed at the time of the run) |
| GPU | Intel **UHD Graphics G4** (integrated) — **no rendering numbers are reported**, see §6 |
| Storage | NVMe, 58 GB free |
| OS / kernel | Fedora 43, kernel 7.1.8-100.fc43.x86_64 |
| Toolchain | rustc / cargo **1.94.0**, `--release` (opt-level 3, no LTO — the workspace does not enable it) |
| Kernel under test | truck-modeling 0.6.0, truck-topology 0.6.0, truck-shapeops 0.4.0 (**patched**, see below), truck-meshalgo 0.4.0 (**patched**), truck-geometry 0.5.0 (**patched**) |
| Harness | `crates/cad-kernel-truck/examples/bench.rs` — std only, no Criterion |
| Repeats | `--iters 5`; **median** reported; a warm-up run is discarded before timing |
| Concurrency | Single-threaded. truck's booleans are not documented as reentrant; no parallelism is claimed |
| Memory | Peak RSS from `/proc/self/status` `VmHWM` |

Run it yourself:

```sh
cargo build --release -p cad-kernel-truck --examples
./target/release/examples/bench --iters 5
```

> **Patched kernel warning.** The booleans measured here run against SolidCraft's vendored patched
> `truck-shapeops`/`truck-geometry`/`truck-meshalgo` (see the root `Cargo.toml` `[patch.crates-io]`),
> because the published crates fail every properly overlapping boolean case measured in
> `docs/kernel-comparison.md`. The numbers describe the kernel the platform ships, not the crates.io
> baseline.

## 2. Results (verbatim harness output)

```
# truck adapter benchmark
# iters = 5 (median reported), release build, single thread
# peak RSS so far: 4.5 MiB
# kernel advertises: boolean intersection, boolean union, extrude, tessellate

extrude           100 boxes            median     0.67 ms   (148753 boxes/s)
  correctness      volume 50000.000 mm³ vs analytic 50000 mm³ (abs error 0.000000)
boolean union     20 pairwise unions median    40.87 ms total (2.04 ms each), 100 results
  correctness      volume min 1119.996 max 1119.996 mm³ vs analytic 1120 mm³ (abs error 0.003693)
  stability        volume spread across all runs: 0.000000000 mm³
  coplanar retries 100 over 100 unions

tessellation      chord tol   median time     triangles   volume error
                      1.000       0.09 ms           12   0.000000 mm³
                      0.500       0.12 ms           12   0.000000 mm³
                      0.100       0.09 ms           12   0.000000 mm³
                      0.050       0.09 ms           12   0.000000 mm³
                      0.010       0.09 ms           12   0.000000 mm³

handles           created 200 solids, live after release 0, ids tracked 200
peak RSS          6.4 MiB
```

## 3. What each number means — and what it does not

### Extrusion — 0.67 ms for 100 boxes (148 753 boxes/s)

Each "box" is a validated 4-point profile → planar face → `tsweep` → a stored solid. The analytic check
is exact (50 000.000 mm³ vs 50 000), so this is a throughput figure for **correct** geometry.

It is **not** a document-editing figure: no document, undo snapshot, spatial index or UI is involved.
A parametric rebuild also pays kernel evaluation of every feature, which this does not measure.

### Boolean union — 2.04 ms per union

20 pairwise unions per iteration, 100 total results, each measured against the analytic 1120 mm³.
Observed error **0.0037 mm³ (0.0003 %)**, which comes from the coplanar-face retry moving the tool
~1e-4 mm — the cost of the workaround, quantified.

Run-to-run volume spread is **0.0 mm³**, so the result is deterministic in volume even though
upstream truck's face *ordering* is not (see `docs/kernel-comparison.md` §4.4).

**Caveat that matters:** all 100 unions took the coplanar retry path. These fixtures are axis-aligned
boxes sharing z extents, which is the worst case for truck. A figure quoted from this benchmark is a
*lower bound* on boolean reliability cost, not a typical case.

### Tessellation — 12 triangles at every tolerance

A box is planar, so its triangle count cannot change with chord tolerance; 12 is the correct minimum
(2 per face). This verifies that tolerance is accepted and that the mesh is exact, **not** that
adaptive tessellation of curved surfaces works. Curved-face tessellation (cylinder, sphere, NURBS) was
not measured — no such fixture was built in this milestone.

Mesh validity is asserted in tests (`mesh.validate()`, `is_closed()`, signed volume 500 mm³ ± 1e-6).

### Memory — 6.4 MiB peak for 200 solids

Small by construction (a box is 6 planar faces), and the handle test proves no solid is retained after
`release()` (200 created, 0 live). It says nothing about a 1 M-entity drawing or a 500-face blend.

### Absolute throughput of the machine (for context)

Building `solidcraft-kernel` (186 files, 94 803 LOC) took **8 min 22 s** with `CARGO_BUILD_JOBS=2` and
peaked at **2.56 GB RSS**, with 257 175 major page faults against a full swap file. That is the real
cost of this hardware: a single crate build, not a micro-benchmark.

## 4. Reproducibility

| Artifact | Command | Expected |
|---|---|---|
| Contract tests | `cargo test --release -p cad-contract` | 54 pass + 1 doctest |
| Adapter tests | `cargo test --release -p cad-kernel-truck` | 13 pass |
| Benchmark | `./target/release/examples/bench --iters 5` | exit 0, volumes within 1 mm³ of analytic |
| Boolean failure matrix | `cargo run --release -p cad-kernel-truck --example probe` | coplanar rows FAILED, others OK |
| Coplanar retry threshold | `... --example probe5` | 1e-6 FAILED, 1e-5…1e-2 OK |
| Difference diagnosis | `... --example probe6` | raw difference 1053.3 vs analytic 840; inner shell reported **+**117.3. Resolved: the cavity walls were wound outward, and the adapter's shell-sign repair fixes it — the repaired fixtures are exact (docs/kernel-comparison.md §4.2) |
| Cut rebuild cost | `cargo run --release -p cad-studio --example rebuild_bench` | box+join 3.14 ms, box+cut **1.81 ms**, box+cut+join 4.05 ms (rebuild only; median of 20 on this host) |
| Chain limit | `... --example chain` | union 1 OK, union 2 Internal error |

Determinism: no random seeds, no time-of-day input, no network. The only variability is scheduling.

## 5. Baselines that still need to be established

These are the benchmarks the brief asks for that **do not exist yet**, and are listed so the gap is
explicit rather than implied:

1. **Document load/save** — `.zcad` correctness and application-level timing now exist (§7); no timing exists for DXF (CADCraft) or `.solidcraft` (SolidCraft) round trips.
2. **Large-drawing selection/snapping** — CADCraft uses `rstar 0.12` already; no baseline taken.
3. **Constraint solving** — CADCraft `constraints` (4 283 LOC), SolidCraft `sketch` (23 681 LOC).
4. **Undo/redo** — CADCraft's copy-on-write `EntityStore` was *designed* for cheap snapshots; the
   claim is read from the source, never measured.
5. **STEP/DXF import/export** — not measured. Binary STL export is timed in §7.
6. **Rendering frame time and picking** — deliberately not reported: the only GPU here is an
   integrated UHD G4, so any figure would mislead.
7. **Curved/NURBS tessellation and large assemblies** — not measured; no fixtures built.

## 6. Rules for future performance work in this repository

1. **Release builds only.** A debug-mode number is not a performance claim.
2. **Report the hardware and the input geometry** with every figure, as §1 does.
3. **Measure correctness in the same run as speed.** A faster wrong answer is a regression; that is
   why every benchmark here prints analytic volume error and exits non-zero when it fails.
4. **Never quote a single median as a distribution.** Use `--iters` high enough to expose variance,
   and report the spread (as the stability line does).
5. **Do not compare across machines.** The 8-minute crate build and 2 ms boolean belong to this i3.
6. **No GPU claims on this host.** Integrated graphics only; measure rendering on representative
   hardware or not at all.

## 7. Atlas Studio application baseline — 2026-10-25

**Measured:** debug build (`cargo build -p cad-studio`), same i3-1125G4 machine as §1. Debug figures
are recorded because that is how the application was exercised during development; they are **not
comparable to the release figures above**. Client: Python `http.client`, one new loopback connection
per request, 5 warm-up requests discarded. Project: `/tmp/atlas-demo.zcad`, a 28-triangle octagonal
prism with a 2 278-byte state payload.

| Workload | Method | Result |
|---|---|---|
| Process start → `/api/health` answered | 5 launches, 1 ms poll interval | 7–16 ms wall (median 10 ms), including project load, validation, one kernel evaluation and the bind |
| `GET /api/state` | 300 sequential requests | **2.66 ms median**, 4.29 ms p95, 27 ms max |
| `POST /api/parameter` (alternating depth 10/11 mm) | 30 requests | **4.44 ms median**, 5.53 ms p95 |
| `POST /api/save` (atomic write + fsync + directory sync) | 10 requests | **4.50 ms median** |
| `GET /api/export/stl` | 10 requests | **3.98 ms median**, 1 484-byte binary STL |
| Viewport readiness | Browser check in the preview pane | WebGL context created; the mesh renders and picks |

**The accept loop was a measured bottleneck.** It polled a non-blocking listener with a 10 ms sleep,
which added up to 10 ms to every new connection. The same 300-request client measured **10.22 ms
median** before the change and **2.66 ms** after (3.8× lower); a single `curl` request went from
4.8 ms to 2.4 ms median. The loop now blocks in `accept` and shutdown wakes it with one throwaway
loopback connection. Correctness is unchanged: the same 200 response, 2 278-byte payload and
28-triangle mesh are asserted by the HTTP integration suite.

The viewport is a WebGL 1 scene with orbit, zoom, pan, ray-cast face picking and a per-face highlight.
**No frame-time or GPU number is claimed**: the host has integrated Intel UHD graphics and no
representative rendering workload, which is the same limitation recorded in §5 item 6.

Startup includes document parse/recompute and snapshot construction. The document uses six undo
snapshots for a freshly created project (sketch creation, extrude, parameter edits), which is visible
in the API's `history` count; this is by design, not a leak.

## 8. Feature-graph rebuild — 2026-10-27

**Measured:** debug build, same host, same client conventions as §7. Project: 10 × 10 × 5 mm
rectangle → `new_body` extrude → second 10 × 10 mm rectangle moved to (8, 8) → `join` extrude, i.e.
two sketches, two features and one kernel union producing a 72-triangle, 980 mm³ body. Harness:
`target/studio-qa/measure.sh`; 20 samples per workload after five discarded warm-up requests.

| Workload | Result | Same workload before the join |
|---|---|---|
| Process start → `/api/health` | 31–34 ms (median 31 ms), 10 ms poll | — |
| `GET /api/state` | **17.4 ms median** of 20 | 3.5 ms |
| `POST /api/parameter` (rebuild both features + union) | **17.3 ms median**, 17.9 ms max | 4.44 ms single-feature baseline (§7) |
| `POST /api/save` (atomic write of the two-feature project) | 12.6 ms | 4.5 ms |
| `POST /api/sketch` / `POST /api/sketch/origin` | 2.2 ms / 1.7 ms | — |
| `GET /api/export/stl` (joined solid) | 16.8 ms | 3.98 ms |

**What the numbers say.** Every request rebuilds the whole history and re-tessellates it; there is no
per-feature cache. The union plus the larger mesh takes the request from ~3.5 ms to ~17 ms. That is
inside an interactive budget, so no optimisation was attempted — the measurement exists so the next
change can show a regression or a win rather than claim one. Correctness in the same session: the
tests assert the analytic 980 mm³ union (±1), the 18 × 18 × 5 bounds and the STL parse-back, and the
browser run reads 980 mm³ / 72 triangles from the live DOM.

**Not claimed:** any GPU frame time (same limitation as §7), cold-start from a cleared build cache,
release-build figures, or behaviour above two features — the two-feature workload is the only one
measured.

**Superseded for request cost by §9 (2026-10-29).** These figures describe the whole-history rebuild
that every request performed before the per-feature cache existed; the request path now reuses cached
prefixes, and §9 measures the same workloads before and after.

## 9. Per-feature rebuild cache — 2026-10-29

**What changed.** Every request used to rebuild the whole history in a fresh kernel: every extrude,
every boolean and the final tessellation, even when nothing had changed since the previous request.
`cad_document::BuildCache` now keeps the body after each prefix of the history, keyed by a hash of that
prefix's resolved inputs, and the studio holds one kernel and one cache for the session.

Two harnesses measured it, because they answer different questions:

* `target/studio-qa/measure-cache.sh` — the real HTTP API, debug build, 20 samples after warm-up, same
  host and client conventions as §8, so the numbers are comparable with the 17.3 ms record. Each
  parameter sample uses a **unique value**, so no sample can be served from an entry an earlier sample
  created.
* `cargo run --release -p cad-studio --example rebuild_bench` — in-process, release build, three
  warm-up rebuilds discarded, median of 20, cold and cached measured against the *same* project object
  in the same process, which is the only way to attribute overhead.

### 9.1 HTTP request latency (debug build)

Same two-feature project as §8 (10 × 10 × 5 block, second 10 × 10 sketch at (8, 8), join) for the first
two rows; a third feature whose depth reads `diameter` is added for the third row.

| Workload | Before (§8) | After, median of 20 | Max after |
|---|---|---|---|
| `GET /api/state`, unchanged history | 17.4 ms | **0.7 ms** (0.6–0.9 over three runs) | 1.1 ms |
| `POST /api/parameter` on `depth`, both features read it, unique value each sample | 17.3 ms | 18.5 ms | 20.8 ms |
| `POST /api/parameter` on `diameter`, read only by the last of three features | — (a full 3-feature rebuild) | 17.4–18.7 ms | 34.7 ms |

The middle row is the honest one: when both features read the parameter, both prefixes are invalidated
and the cache cannot help. It also must not hurt, and it does not beyond a few percent — the in-process
measurement below attributes that cost.

### 9.2 Rebuild cost, cold versus cached (release build)

Same process, same project, median of 20; the "cached, last edited" column changes a parameter only the
last feature reads before every sample.

| History | Cold rebuild | Cached, unchanged | Cached, last feature edited | Volume |
|---|---|---|---|---|
| join (2 features) | 2.55 ms | 0.00 ms | 2.35 ms | 755.011 → 1 087.213 mm³ |
| cut (2 features) | 1.61 ms | 0.00 ms | 1.55 ms | 255.000 → 367.200 mm³ |
| cut + join (3 features) | 3.04 ms | 0.00 ms | 3.12 ms | 755.000 → 1 087.200 mm³ |
| **6 features** (1 body + 5 joins) | **30.32 ms** | 0.01 ms | **7.88 ms** | 1 500.080 → 1 720.080 mm³ |

**What the numbers say.** The two- and three-feature histories are dominated by the boolean itself, so
saving one extrude is within noise (and the two-feature join case is 0.05 ms *slower*, which is the
mesh-clone cost on a miss). The six-feature history is where the cache pays: a cold rebuild runs six
extrudes and five unions, an edit of the last feature runs one extrude and one union, and the rebuild
drops from 30.3 ms to 7.9 ms. An unchanged history costs a mesh clone per prefix plus the payload
serialisation, measured at 0.01 ms here and 0.7 ms through HTTP in debug.

**Correctness in the same session.** The analytic volumes are unchanged and are asserted in
`crates/cad-document/src/lib.rs` (cached against uncached reports and mesh, edits of the first and last
feature, failure propagation, undo back to a cached revision, a cache handed to a different kernel
instance) and in `crates/cad-studio/tests/http_api.rs` (13 real-socket tests, including save/reopen and
STL parse-back through the cached path). The kernel-call log asserts that an unchanged rebuild issues
**no** extrude and **no** boolean call.

**Not claimed:** any speedup for a parameter that several features share, a multi-process or cross-session
cache, a release-build HTTP figure (§9.1 is debug, to stay comparable with §8), memory above 32 cached
prefixes (the bound is enforced by release, not measured), or hash-collision resistance beyond 64-bit
FNV-1a.

## 10. Rebuild-cache memory — 2026-10-29

§9 left one question open: the cache keeps up to 32 bodies and their tessellations alive, but its
memory cost was *enforced, not measured*. This is the measurement.

**Method.** `cargo run --release -p cad-studio --example cache_memory -- <cached|uncached> <N>`, one
mode per process (peak resident memory is a process-wide high-water mark, so two modes in one process
cannot be attributed). The history is N overlapping cylinders, each sketched as a 64-point polyline and
joined onto the running body, so every prefix is a *different* body — the worst case for the cache.
Peak RSS is `VmHWM` from `/proc/self/status`, read after the build. Same host as §9: Linux, 8 cores,
7 GB RAM, rustc 1.95.0, release build.

| Features | Final triangles | Uncached peak RSS | Cached peak RSS | Cache cost | Cached build |
|---:|---:|---:|---:|---:|---:|
| 8 | 2 554 | 10 684 kB | 12 592 kB | +1.9 MB | 641 ms |
| 16 | 5 194 | 16 460 kB | 25 724 kB | +9.3 MB | 2 422 ms |
| 32 | 10 460 | 27 572 kB | 68 796 kB | +41.2 MB | 9 574 ms |

A second 32-feature run measured 70 772 kB cached / 28 808 kB uncached, so the delta is stable to a few
megabytes. The cost per retained prefix in the 32-feature run is ~1.3 MB, and it grows with the prefix
index because the mesh does.

**What the numbers say.** The 32-prefix bound is safe for the histories this application builds today:
a 32-feature part costs ~69 MB resident, which is unremarkable for a desktop CAD session. The growth is
superlinear in history length (each prefix mesh is larger than the last, so the cache holds O(N²)
facets), which is the bound's real justification. No change was made: the measurement does not show a
problem at 32 features, and the alternative (`Arc<Mesh>` or caching only every k-th prefix) would
change the reuse that makes an undo cheap. It is now a recorded ceiling rather than an unmeasured claim.

**Not claimed:** memory for histories longer than 32 features (the bound discards older prefixes, and
beyond it the retained window is still the 32 newest), B-rep memory separately from mesh memory, or
behaviour under a different allocator or on another platform.

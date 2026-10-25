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
| Difference defect | `... --example probe6` | difference 1053.3 vs 840; inner shell +117.3 |
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

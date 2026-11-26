# AtlasCAD Performance and Reliability

**Date:** 2026-11-26
**Scope:** the release `cad-studio` server and the browser application it serves, on the host below.
Kernel micro-benchmarks live in [`../performance-baseline.md`](../performance-baseline.md) (measured
2026-10-21) and are cited, not re-run, here.

---

## 1. Method

| Item | Value |
| --- | --- |
| Host | Fedora 43, kernel 7.3.4, 8 logical cores, 7 GiB RAM |
| Toolchain | rustc 1.95.0, `--release` |
| Server | `./target/release/cad-studio --new --file <tmp>/perf.zcad --size 40,30,10 --listen 127.0.0.1:7622` |
| Startup | time from spawn until the first successful `GET /api/health`, polled every 5 ms |
| Latency | `curl -w '%{time_total}'`, 20 requests, median |
| Memory | `ps -o rss=` after the request set |
| Browser (prior) | headless Chrome 157 at 1440×900, measured 2026-11-25 |

No GPU frame times were captured. Software rendering was used for the browser checks, so no
frame-rate claim is made.

---

## 2. Measured results (this pass)

| Measurement | Result |
| --- | --- |
| Cold start to first answered health probe | **99 ms** (includes up to 5 ms polling error) |
| `GET /api/state` (one feature) | **0.97 ms** median of 20 |
| `GET /api/export/stl` | **1.05 ms** |
| Server RSS after the above | **9,036 KB** |
| `/api/state` payload | 1,450 bytes |
| Release `cad-studio` binary | 16,820,560 bytes |
| `/api/health` reported solvers | 140 analytical models ready |

Prior browser measurement (2026-11-25, cited): **814 ms** first load, **5,827** DOM nodes,
**0** failed network requests, **47 MB** server RSS, four headless `ReadPixels` driver warnings.

### 2.1 Large model — 10,092 triangles (measured 2026-11-26)

`python3 scripts/benchmark-large-model.py --binary target/release/cad-studio` generates a watertight
faceted STEP fixture (a 100 mm box whose six faces are subdivided, 10,092 triangles / 5,411,104
bytes), imports it into a freshly started release server and measures the result. The fixture is
real geometry, generated into a temporary directory; the triangle count is confirmed from the
exported binary STL, not taken from the generator's own claim.

| Measurement | Result |
| --- | --- |
| STEP import (`POST /api/import/step`) | **225 ms** |
| `GET /api/state`, median of 10 | **3.89 ms** |
| `/api/state` payload | **1,028,148 bytes** |
| `GET /api/export/stl` | **1.4 ms**, 505,284 bytes, 10,104 triangles (10,092 imported + 12 seed box) |
| Server RSS idle → after import | **8,672 KB → 59,020 KB** |

Enabling this required raising `MAX_BODY_BYTES` from 1 MiB to 16 MiB: at 1 MiB every STEP over
roughly 2k triangles was refused with `413`, so the documented 10k-face capability was unreachable
over HTTP. The cap still bounds a single request; the STEP reader applies its own entity and
triangle limits. The browser was not driven with this fixture, so no frame-time claim is made.

Kernel micro-benchmarks (2026-10-21, cited): 0.67 ms for 100 extruded boxes (148,753/s),
2.04 ms per boolean union with 0.0037 mm³ error, 6.4 MiB peak RSS for 200 solids.

---

## 3. What these numbers do and do not cover

**Covered:** a small document, a warm process, local loopback, no TLS termination, no concurrent
writers, no simulation solve, and (from §2.1) a 10k-triangle imported mesh measured at the HTTP and
RSS level.

**Not covered:** rendering/frame time for the 10k model, tessellation of curved/NURBS surfaces at
scale (the bundled STEP importer only reads faceted files), long editing sessions, memory growth,
GPU picking cost, cancellation latency under load, multi-user contention, disk-full and slow-disk
behaviour.

The 4 `GPU stall due to ReadPixels` messages are recorded as a **hypothesis** (renderer readback for
picking/screenshots) in the usability audit; no frame-time evidence exists, so no performance claim
is made.

---

## 4. Reliability behaviour (verified)

| Failure | Behaviour | Evidence |
| --- | --- | --- |
| Process killed after an unsaved edit | `.recovery` snapshot offered on restart; discard action; verified `kill -9` | 12 recovery assertions, HTTP tests |
| Half-written / corrupt project file | refused with a typed error; falls back to snapshot, then last good `.backup` | document tests |
| Kernel panic | caught and returned as `KernelError::Internal`; process survives | kernel adapter test |
| Kernel timeout / cancellation | budget deadline and cancellation token; instance remains usable | kernel tests |
| Feature that cannot rebuild | named with the kernel message; last good mesh stays renderable; export refused | HTTP tests |
| Foreign origin / host | `403 forbidden_origin` before routing | live check (curl without `Origin`), tests |
| Oversized or wrong-media request | `413` / `415` typed errors | HTTP tests |
| Engine not installed | health reports `unavailable`; run/benchmark return `501`, no result, no graph promotion | PR-001 tests + live check |
| Unsafe overwrite | `--new` refuses an existing file | observed in the usability audit |
| Duplicate job submission | queue de-duplicates by id (unit test) | `atlas-job-queue` test |

Not verified: disk-full behaviour, network interruption during a long solve, partial output from an
external engine (there is no adapter), and restart loops under systemd.

---

## 5. Resource guidance for low-memory hosts

- The server is small (9 MB RSS for a simple part) and starts in about 0.1 s.
- The browser page is large: 5,827 DOM nodes, 639 buttons, 1,375 ids. It is the first thing to
  virtualise if the shell grows.
- The workspace test gate defaults to `JOBS=2` because the recorded host is memory-constrained;
  `JOBS=4` is an explicit override.
- No remote-compute offload exists; do not claim one.

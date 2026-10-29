# Baseline Tests

**Canonical data:** `engineering/quality/tests.json`.
**Commands:** [../build-and-test.md](../build-and-test.md).

## 1. Executed in this workspace

| Id | Command | Passed | Failed | Skipped | Notes |
|---|---|---:|---:|---:|---|
| `t-cad-contract` | `cargo test --release -p cad-contract` | 55 | 0 | 0 | 54 unit + 1 doc test |
| `t-cad-kernel-truck-m11d` | `cargo test --offline -p cad-kernel-truck` | 35 | 0 | 0 | analytic extrude and revolve volumes, capability advertisement, manifold orientation, cancellation and deadlines; supersedes `t-cad-kernel-truck` (26) and `t-cad-kernel-truck-m11c` (31) |
| `t-cad-kernel-truck-corpus` | `cargo run -p cad-kernel-truck --example corpus` | 8 | 0 | 3 | the analytic boolean corpus: 11 fixtures, 8 ok, 3 recorded known failures (tangent contact, intersection near 1e6 mm, coplanar cut), 0 regressions, exit code 0 |
| `t-cad-engineering` | `cargo test --offline -p cad-engineering` | 30 | 0 | 0 | 4 unit + 26 integration, real git fixture; report totals exclude superseded records |
| `t-cad-cli` | `cargo test --offline -p cad-document -p cad-cli` | 10 | 0 | 0 | 9 end-to-end (units → header → kernel → STL) + 1 multi-process project workflow |
| `t-cad-agent-api` | `cargo test -p cad-agent-api` | 13 | 0 | 0 | contract version, grant intersection, zero budgets, error taxonomy |
| `t-cad-agent-runtime` | `cargo test -p cad-agent-runtime` | 16 | 0 | 0 | lifecycle, budgets, quarantine, the strict schema subset |
| `t-cad-agent-geometry` | `cargo test -p cad-agent-geometry` | 24 | 0 | 0 | 12 unit + 12 integration, the agent driven through a real registry |
| `t-cad-feature-graph` | `cargo test --offline -p cad-document` | 36 | 0 | 0 | **2026-10-27**, superseded by `t-cad-document-m11c` (51): profiles, schema 3 feature graph, two-feature union volume, dependency refusal, failure propagation, schema-1/2 migration, undo/redo, corrupt input |
| `t-cad-document-m11c` | `cargo test --offline -p cad-document` | 51 | 0 | 0 | **2026-10-29**, superseded by `t-cad-document-m11d` (57): the 36 above plus 7 per-feature-cache tests and 8 earlier cut/polyline tests; a cached edit of the last feature issues exactly one extrude and one union, an unchanged history issues none, an earlier edit invalidates every later prefix, a failure is reproduced rather than cached away, and a fresh kernel instance discards the cache |
| `t-cad-document-m11d` | `cargo test --offline -p cad-document` | 57 | 0 | 0 | **2026-10-29**, superseded by `t-cad-document-m11f` (61): the 51 above plus 6 revolve tests — axis resolution, refusal when the kernel does not advertise `Revolve`, a join revolve through the union, save/reopen with the axis, schema-4 migration to schema 5, and a cached revolve that does not revolve again |
| `t-cad-document-m11f` | `cargo test --offline -p cad-document` | 61 | 0 | 0 | **2026-10-29**: the 57 above plus 4 crash-recovery tests — an unsaved edit is offered from its snapshot while the file is untouched, a snapshot superseded by a save is deleted, a corrupt snapshot is ignored, and a truncated project file falls back to its snapshot |
| `t-cad-entitlements` | `cargo test --offline --workspace` | 10 | 0 | 0 | policy only; no billing provider exists |
| `t-cad-io` | `cargo test --offline -p cad-io` | 6 | 0 | 0 | binary STL facet layout, round trip, truncation and declared-count refusal |
| `t-cad-studio-m11` | `cargo test --offline -p cad-studio` | 13 | 0 | 0 | **2026-10-27**, superseded by `t-cad-studio-m11d` (17): 2 unit + 11 real-socket HTTP tests, including join volume and save/reopen, failed-feature reporting, foreign/empty Host, same-origin, body limits and STL parse-back |
| `t-cad-studio-m11d` | `cargo test --offline -p cad-studio` | 17 | 0 | 0 | **2026-10-29**, superseded by `t-cad-studio-m11f` (19): 2 unit + 15 HTTP tests; the four revolve ones cover an analytic annulus over a real socket through save/reopen to a watertight STL, a profile across the axis as a failed feature with export refused, the revolve endpoint's refusals, and the capability gate on the UI's feature-kind option |
| `t-cad-studio-m11f` | `cargo test --offline -p cad-studio` | 19 | 0 | 0 | **2026-10-29**: the 17 above plus 2 recovery tests — an unsaved edit survives a dropped server handle, is offered on restart with `recovered`/`dirty` true, and is promoted by a save; and discard returns to the file body, clears the flag and deletes the snapshot |
| `t-cad-studio-browser-m11` | `node target/studio-qa/drive.mjs <url>` (headless Chrome over CDP) | 7 | 0 | 0 | **2026-10-27**: assertions, not cargo tests — sketch → extrude → second sketch → origin move → join → failure banner → undo → save |
| `t-cad-studio-browser-m11d` | `node target/studio-qa/revolve.mjs <url>` (headless Chrome over CDP) | 11 | 0 | 0 | **2026-10-29**: assertions, not cargo tests — the Revolve option enabled on a capable kernel, depth field hidden and axis field shown, the turn added as a `built` row, `Turn: full revolution` and 3,133.72 mm³ in the properties (analytic 3,141.59), export enabled, and the row back after save + reload, with zero console errors |
| `t-cad-studio-browser-m11f` | `node target/studio-qa/recovery.mjs <url>` (headless Chrome over CDP) | 9 | 0 | 0 | **2026-10-29**: assertions, not cargo tests — the recovery banner with the time of the edit, the `unsaved` badge, the recovered 800 mm³ body, discard back to the file's 500 mm³ and `saved`, no banner after a later edit, save clearing the badge, zero console errors |
| `t-cadcraft-geom` | `cargo test -p cadcraft-geom` | 84 | 0 | 0 | from the audit session |

Total recorded passes: **398** (including the 27 browser assertions and the 84 CADCraft audit tests).
The workspace summary records **279** tests in the root workspace
(`t-platform-workspace-20261029-m11f`), up from 273 earlier the same day; the larger total includes
the CADCraft audit suite, the browser assertions and the corpus binary, which are not part of that
workspace run. Rows marked superseded are kept as history and are not counted.

The three "skipped" corpus fixtures are **recorded known failures**, not skipped tests: tangent
contact, intersection at coordinates near 1e6 mm and a coplanar cut ran and failed as the baseline
expects. The corpus prints them and the gate stays green, so a future fix shows up as an unexpected
pass rather than silently changing the answer.

## 2. Not executed

| Suite | Size | Reason |
|---|---|---|
| SolidCraft workspace | 5,884 test attributes (57 ignored) | memory and time on this host; one crate build is 8 m 22 s / 2.56 GB |
| CADCraft remaining crates | 2,212 attributes total; 84 executed | engine/UI require the heavy build |
| Kernel repositories' own suites | each has its own | not built here |
| The real kernel against the corpus | 10 fixtures | needs the ≥16 GB RAM host; see `docs/kernel-comparison.md` §3.1 |
| Any GUI, web or MCP test | — | not built here |
| Fuzzing, advisory scan, penetration test | — | tooling absent; see `docs/security-review.md` |

The SolidCraft row is recorded as `not_run` in `tests.json` rather than omitted, so the gap is visible
in every generated report.

### Dated run: 2026-10-27 (feature graph)

`cargo test --offline --workspace -j 2` — **239 passed, 0 failed, 0 skipped** in 11 packages, 2.39 s
wall, 133 640 KB peak RSS; `cargo fmt --all --check` clean and `cargo clippy --offline --workspace
--all-targets -D warnings` with 0 warnings. Twelve more tests than the 227 of 2026-10-25, all of them
in `cad-document` (+9) and `cad-studio` (+3); no earlier test was removed or skipped. The new
assertions are: an ordered two-feature history builds both extrudes and exactly one union with an
analytic volume; a shared parameter rebuilds every dependent feature; a join is refused when the
kernel does not advertise `BooleanUnion`; a second body or a join without a body is refused; a
failing join names its feature and keeps the last good mesh; loading refuses a feature that consumes
a sketch created after it; schema-2 features gain an explicit operation; undo removes the joined
feature and its geometry together; and over HTTP, two overlapping 10×10×5 boxes measure 980 mm³,
survive save/reopen and export as a 980 mm³ STL. Full record: `docs/quality/m11-feature-graph.md`.

### Dated run: 2026-10-29 (per-feature rebuild cache)

`cargo test --offline --workspace -j 2` — **261 passed, 0 failed, 0 skipped** in 11 packages, 3.5 s
wall; `cargo fmt --all --check` clean and `cargo clippy --offline --workspace --all-targets -D warnings`
with 0 warnings. Twenty-two more than the 239 of 2026-10-27: `cad-document` +15 (7 cache tests, plus
the cut and polyline tests added since that run), `cad-kernel-truck` +5 (2 counter/handle tests and the
transform tests) and `cad-studio` +2. No earlier test was removed or skipped. The new assertions are: an unchanged history rebuilds from the cache and issues no kernel
call; editing the last feature's own parameter issues exactly `extrude` + one `union`; editing an
earlier shared parameter rebuilds every dependent feature; undo back to a previously built revision
is free; a failing feature reports the same message through the cached and uncached paths and is not
cached; a cache built by one kernel instance is discarded when handed to another; clearing the cache
releases every handle; and a cached build equals an uncached build in reports and mesh.

### Dated run: 2026-10-29 (revolve)

`cargo test --offline --workspace -j 2` — **273 passed, 0 failed, 0 skipped** in 11 packages, 3.9 s
wall; `cargo fmt --all --check` clean and `cargo clippy --offline --workspace --all-targets -D warnings`
with 0 warnings. Twelve more than the 261 of the cache run: `cad-kernel-truck` +4, `cad-document` +6
and `cad-studio` +2. No earlier test was removed or skipped; one assertion was inverted — the CLI's
summary test asserted `revolve` is *not* advertised and now asserts it is, kept as an assertion so the
claim stays tested in both directions. The new assertions are: a rectangle on the axis measures the
analytic cylinder (150.796 mm³ against π·4²·3, within 0.05) and is watertight; an offset rectangle
measures the analytic annulus (113.097 mm³ against π·(4²−2²)·3) and is watertight; a partial turn and a
profile crossing the axis are refused with a reason; a revolve resolves its axis in millimetres, is
refused on a kernel that does not advertise `Revolve`, joins through the union, survives save/reopen,
migrates a schema-4 project, and is served from the cache on an unchanged rebuild; and over HTTP, the
same annulus survives save/reopen (volume equal to 1e-9) and exports as a watertight STL, while a
profile across the axis is a failed feature with export refused.

### Dated run: 2026-10-29 (crash recovery)

`cargo test --offline --workspace -j 2` — **279 passed, 0 failed, 0 skipped** in 11 packages; `cargo fmt
--all --check` clean and clippy `-D warnings` at 0 warnings. Six more than the 273 of the revolve run:
`cad-document` +4 and `cad-studio` +2, all for unsaved-edit recovery. A real `kill -9` on the release
binary was run as well: an unsaved edit (10×10×8 = 800 mm³) reopened at 800 mm³ with `recovered` and
`dirty` set, where before the change the same sequence reopened at the saved 500 mm³. The recovery
banner and the discard path were driven in headless Chrome (9 assertions, 0 console errors).

### Dated run: 2026-10-29 (delivery gate)

`./scripts/gate.sh` — **284 passed, 0 failed, 0 skipped** in 11 packages, then the analytic corpus (8
ok / 3 known-failure / 0 regressions), `atlas-eng validate` (0 errors, 0 warnings across 14 manifests),
the licence policy (83 packages, 83 allowed, offline), the SBOM dry run (CycloneDX 1.5, 83
components), and the live simulation (52 assertions: 31 HTTP, 21 driving the shipped `atlas` and `atlas-eng`
binaries, plus 7 in headless Chrome) — all in
about 6 s warm. Five more than the 279 of the crash-recovery run, all in `cad-engineering`: SPDX
expression tokenisation (`OR`, legacy `/`, parenthesised `AND`, `WITH`), a denied token beating an
allowed one, unknown or missing licences failing closed, the audit's package separation, and a
byte-identical SBOM. `./scripts/package.sh` then produced
`dist/atlas-cad-0.1.0-x86_64-unknown-linux-gnu.tar.gz` (3.1 MB), extracted it into a clean directory,
verified `SHA256SUMS`, ran `--help` on all three packaged binaries and built a 10×10×5 STL with the
packaged CLI. The CI workflow and `deny.toml` were added but have **not** run on a runner.

## 3. What the tests actually assert

* **Analytic values, not self-consistency:** 10×10×5 box = 500 mm³ ± 1e-6; union 1000 + 160 − 40 =
  1120 mm³ ± 1; intersection 2×2×10 = 40 mm³ ± 0.5; inch conversion 25.4³ mm³.
* **Manifold output:** welded mesh has 0 open edges and every face normal points outward for the box
  fixture (regression for a measured inward-wound cap).
* **Honest failure:** an operation a kernel does not advertise must be refused with `Unsupported`
  rather than approximated; a partial revolve and a profile crossing the axis must be `Invalid` with a
  reason; a stale or foreign handle must be `UnknownSolid`.
* **Interruption:** an expired deadline is `Timeout`; a cancelled call is `Cancelled`; the kernel
  instance and the agent both survive the interruption and serve the next call.
* **Control plane:** duplicate ids, unknown cross-references, schema drift, copyleft approval,
  unimplemented agents claiming registration, verified tasks without evidence, dependency cycles,
  changed-file detection, and idempotent scans are all negative-tested.
* **Agent platform:** the declared schema is enforced *before* the agent runs; the effective grant
  decides whether the file operation may run; an agent whose contract tests failed never becomes
  ready; the output and call budgets are enforced from outside the agent; every reply satisfies the
  contract's declared output schema; a traversing file name is refused before any filesystem call.
* **External verification:** the CLI's STL was parsed by an independent Python implementation —
  12 triangles, volume 500.0000, 0 open edges; the union fixture 82 triangles, 1120.0037 mm³, 0 open
  edges.

Tests return `Result` rather than early-returning on failure. An earlier version of the adapter tests
used `let Ok(x) = call() else { return }`, which made a completely broken boolean pass; that pattern is
documented and must not return.

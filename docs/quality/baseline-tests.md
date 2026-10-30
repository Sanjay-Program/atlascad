# Baseline Tests

**Canonical data:** `engineering/quality/tests.json`.
**Commands:** [../build-and-test.md](../build-and-test.md).

## 1. Executed in this workspace

| Id | Command | Passed | Failed | Skipped | Notes |
|---|---|---:|---:|---:|---|
| `t-platform-workspace-20261030` | `cargo test --offline --workspace -j 2` | 309 | 0 | 0 | **2026-10-30** — the whole root workspace including doctests. `cad-document` 78 (8 new: raised-plane translation, signed direction, inch plane, no-transform refusal, cache invalidation on a plane move, one-step placement undo, non-finite plane refusal, schema-7 → 8 migration) and `cad-studio` HTTP 25 (3 new analytic tests against the real Truck kernel: blind pocket on a raised plane, top-face boss, downward through-hole; all within 1e-2 mm³ of the inscribed-polygon analytic value, watertight exports). clippy `-D warnings` and fmt clean. |
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
| `t-cad-document-m11f` | `cargo test --offline -p cad-document` | 61 | 0 | 0 | **2026-10-29**, superseded by `t-cad-document-m11e` (65): the 57 above plus 4 crash-recovery tests — an unsaved edit is offered from its snapshot while the file is untouched, a snapshot superseded by a save is deleted, a corrupt snapshot is ignored, and a truncated project file falls back to its snapshot |
| `t-cad-document-m11e` | `cargo test --offline -p cad-document` | 65 | 0 | 0 | **2026-10-29**, superseded by `t-cad-document-m11fr` (70): the 61 above plus 4 arc-sector tests — a quarter sector resolves its arc from 0° to 90° at radius 10 starting at the centre, with the analytic inscribed-polygon area to 1e-9 (within 1% of the true quarter disc) and an exact prism volume; a 0° sweep, a 360° sweep and non-finite angles are refused without adding a sketch; a sector survives save/reopen with the same profile and volume; and a schema-5 payload migrates to schema 6 |
| `t-cad-document-m11fr` | `cargo test --offline -p cad-document` | 70 | 0 | 0 | **2026-10-29**: the 65 above plus 5 named-revision tests — a revision restores the history it captured (depth 12 → copy → depth 4 → restore = 1200 mm³) and the restore is one undo away; names are trimmed, bounded and unique case-insensitively; the newest 32 copies are kept with monotonic ids; schema 6 migrates to 7; and a truncated project with no snapshot opens the previous good `<file>.backup` and repairs it on save |
| `t-cad-entitlements` | `cargo test --offline --workspace` | 10 | 0 | 0 | policy only; no billing provider exists |
| `t-cad-io` | `cargo test --offline -p cad-io` | 6 | 0 | 0 | binary STL facet layout, round trip, truncation and declared-count refusal |
| `t-cad-studio-m11` | `cargo test --offline -p cad-studio` | 13 | 0 | 0 | **2026-10-27**, superseded by `t-cad-studio-m11d` (17): 2 unit + 11 real-socket HTTP tests, including join volume and save/reopen, failed-feature reporting, foreign/empty Host, same-origin, body limits and STL parse-back |
| `t-cad-studio-m11d` | `cargo test --offline -p cad-studio` | 17 | 0 | 0 | **2026-10-29**, superseded by `t-cad-studio-m11f` (19): 2 unit + 15 HTTP tests; the four revolve ones cover an analytic annulus over a real socket through save/reopen to a watertight STL, a profile across the axis as a failed feature with export refused, the revolve endpoint's refusals, and the capability gate on the UI's feature-kind option |
| `t-cad-studio-m11f` | `cargo test --offline -p cad-studio` | 19 | 0 | 0 | **2026-10-29**, superseded by `t-cad-studio-m11e` (20): the 17 above plus 2 recovery tests — an unsaved edit survives a dropped server handle, is offered on restart with `recovered`/`dirty` true, and is promoted by a save; and discard returns to the file body, clears the flag and deletes the snapshot |
| `t-cad-studio-m11e` | `cargo test --offline -p cad-studio` | 20 | 0 | 0 | **2026-10-29**, superseded by `t-cad-studio-m11fr` (24): the 19 above plus 1 real-socket test on the real Truck kernel — a quarter sector cuts a 10×10×5 plate to the analytic 500 − (n-gon area × 5) within 1e-2 mm³, bounds unchanged, undo back to 500 mm³, redo restoring the cut, save/reopen preserving the profile and both angles, and a watertight exported STL; two impossible sweeps are refused with 400. The measured count before this cycle was 19, not the 21 the share record recorded |
| `t-cad-studio-m11fr` | `cargo test --offline -p cad-studio` | 24 | 0 | 0 | **2026-10-29**: the 20 above plus 2 real-socket tests — named revisions record/edit/restore/undo/redo/save/reopen/delete over HTTP with a 404 for an unknown id, and a project file truncated after a save reopens the previous good copy with `recovery.source: "backup"` and the pre-edit body. Total includes the 2 unit tests |
| `t-cad-studio-browser-m11` | `node target/studio-qa/drive.mjs <url>` (headless Chrome over CDP) | 7 | 0 | 0 | **2026-10-27**: assertions, not cargo tests — sketch → extrude → second sketch → origin move → join → failure banner → undo → save |
| `t-cad-studio-browser-m11d` | `node target/studio-qa/revolve.mjs <url>` (headless Chrome over CDP) | 11 | 0 | 0 | **2026-10-29**: assertions, not cargo tests — the Revolve option enabled on a capable kernel, depth field hidden and axis field shown, the turn added as a `built` row, `Turn: full revolution` and 3,133.72 mm³ in the properties (analytic 3,141.59), export enabled, and the row back after save + reload, with zero console errors |
| `t-cad-studio-browser-m11f` | `node target/studio-qa/recovery.mjs <url>` (headless Chrome over CDP) | 9 | 0 | 0 | **2026-10-29**: assertions, not cargo tests — the recovery banner with the time of the edit, the `unsaved` badge, the recovered 800 mm³ body, discard back to the file's 500 mm³ and `saved`, no banner after a later edit, save clearing the badge, zero console errors |
| `t-cad-studio-browser-m11e` | `node scripts/browser-check.mjs <url>` (headless Chrome over CDP) | 3 | 0 | 0 | **2026-10-29**: the 3 sector assertions inside the gate's browser run — the Create form opens its Start°/Sweep° fields with the typed values, adding the sketch puts a `sector` row in the tree, and selecting it reopens the profile panel with `sector` and the angles 30/120 it was created with |
| `t-cad-studio-browser-m11fr` | `node scripts/browser-check.mjs <url>` (headless Chrome over CDP) | 4 | 0 | 0 | **2026-10-29**: the 4 new revision assertions (15 loopback + 17 shared in total) — the Revisions panel records `browser baseline` and shows the row, a depth edit through the parameter panel changes the measured body, Restore returns the original measurement, and the palette offers `Restore revision “browser baseline”`. The first attempt timed out on revision buttons rendered disabled while busy; `setBusy` now re-renders the list |
| `t-cadcraft-geom` | `cargo test -p cadcraft-geom` | 84 | 0 | 0 | from the audit session |

Total recorded passes: **419** (including the 34 browser assertions and the 84 CADCraft audit tests).
The workspace summary records **298** tests in the root workspace
(`t-platform-workspace-20261029-revisions`), up from 291 earlier the same day; the larger total
includes the CADCraft audit suite, the browser assertions and the corpus binary, which are not part
of that workspace run. Rows marked superseded are kept as history and are not counted.

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

### Dated run: 2026-10-29 (LAN sharing)

`cargo test --offline --workspace -j 2` — **286 passed, 0 failed, 0 skipped**; fmt clean and clippy
`-D warnings` at 0 warnings. The record is `t-platform-workspace-20261029-share`. Two new tests in
`cad-studio` cover the sharing boundary: without a token and with a wrong token every `/api/*` call is
`401 unauthorized`; the token is accepted as `X-Atlas-Token` and as `?token=`; a write without it is
`401`; a write from a foreign origin is `403`; a `Host` naming another port is `403`; a token-bearing
40×30×20 write measures 24000 mm³; the payload reports the copyable link; the page is served with
`Referrer-Policy: no-referrer`; and a loopback studio has no token, no link and `share: null`. The
live simulation grew to **56 HTTP/CLI assertions** (4 of them the share boundary) and the browser
check to **8 loopback + 10 shared**, where the shared page renders the **Share** panel with its
tokenised link, zero console errors in both runs.

### Dated run: 2026-10-29 (named revisions and backup, M11f)

`cargo test --offline --workspace -j 2` — **298 passed, 0 failed, 0 skipped**; fmt clean and clippy
`-D warnings` at 0 warnings. The record is `t-platform-workspace-20261029-revisions`. Seven new tests:
five in `cad-document` (named revision restore and undo, naming rules and the 32-copy bound,
schema-6→7 migration, and the truncated-file→backup path) and two in `cad-studio` (the revision
workflow over HTTP with a 404 for an unknown id, and the `recovery.source: "backup"` payload). The
corpus still prints 8 ok / 3 known failures / 0 regressions with no fixture skipped, manifests 0/0,
licences 83/83 and SBOM 83 components. The live simulation is unchanged at 56 HTTP/CLI assertions; the
browser check is **15 loopback + 17 shared**. The cache memory question from §9 is closed by
`bm-cache-memory-32` in `docs/performance-baseline.md` §10: 68,796 kB cached against 27,572 kB
uncached on a 32-feature, 10,460-triangle history.

### Dated run: 2026-10-29 (arc-sector profile, M11e)

`cargo test --offline --workspace -j 2` — **291 passed, 0 failed, 0 skipped**; fmt clean and clippy
`-D warnings` at 0 warnings. The record is `t-platform-workspace-20261029-m11e`. Five new tests: four
in `cad-document` (sector arc resolution and analytic area, refusal of a 0°/360°/non-finite sweep,
save/reopen, schema-5→6 migration) and one in `cad-studio` (a sector cut of a plate over a real socket,
analytic volume, undo/redo, save/reopen, watertight STL). The corpus still prints 8 ok / 3 known
failures / 0 regressions with no fixture skipped, the manifests validate 0/0, the licence policy is
83/83 allowed and the SBOM is 83 components. The live simulation is unchanged at 56 HTTP/CLI
assertions; the browser check grew to **11 loopback + 13 shared**. The gate now always rebuilds the
release binaries before the simulation — it previously reused a stale one, which let the browser
check drive a page that no longer matched the tree. This cycle found that defect by failing on it.

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

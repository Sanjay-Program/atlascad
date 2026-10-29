# M11 — Feature graph and multi-feature join: evidence

**Date:** 2026-10-27
**Scope:** `cad-document` (schema 3), `cad-studio` (API + browser workspace), `cad-cli` unchanged in
behaviour.
**Decision record:** [ADR-0009](../architecture-decisions/0009-feature-graph.md).
**Canonical data:** `engineering/quality/tests.json` (`t-cad-feature-graph`, `t-cad-studio-m11`,
`t-cad-studio-browser-m11`, `t-platform-workspace-20261027`),
`engineering/quality/benchmarks.json` (`bm-studio-m11-*`),
`engineering/quality/builds.json` (`b-platform-workspace-20261027`).

## 1. What was implemented

| Item | Status | Where |
|---|---|---|
| Project schema 3: ordered sketches and features with stable IDs | implemented, built, tested | `crates/cad-document/src/lib.rs` |
| Explicit `new_body` / `join` operation; one body per project | implemented, tested | same |
| Sequential rebuild through `cad_contract::Kernel`; join via `boolean(Union)` behind `supports(BooleanUnion)` | implemented, tested | same |
| Per-feature rebuild report and first-failure record; last good mesh kept | implemented, tested | same |
| Schema 1 → 2 → 3 migration on load, no on-disk rewrite | implemented, tested | same |
| HTTP: `POST /api/sketch`, `POST /api/sketch/origin`, `POST /api/feature/extrude`; ordered `features` and `failure` in `/api/state` | implemented, tested | `crates/cad-studio/src/lib.rs` |
| Model tree with per-feature operation, status badge and failure message; sketch placement panel; join form; failed-feature banner; export refusal | implemented, browser-verified | `crates/cad-studio/assets/{index.html,app.js,app.css}` |
| CLI | unchanged; still exercises a single-part project | `crates/cad-cli` |

**Not implemented, and shown as unavailable rather than faked:** cut (kernel difference is a
recorded failure), revolve, loft, fillet, chamfer, shell, patterns, multi-body, constraints,
dimensions and assemblies.

> As of 2026-10-27. Since then cut shipped (2026-10-28) and full-turn revolve shipped (2026-10-29,
> schema 5); loft, fillet, chamfer, shell, patterns, multi-body, constraints, dimensions and
> assemblies are still absent. Current state: `docs/tasks.md` and `docs/feature-matrix.md`.

## 2. Commands actually executed

```sh
cargo fmt --all --check
cargo clippy --offline --workspace --all-targets -j 2 -- -D warnings
cargo test --offline --workspace -j 2
cargo build --offline -p cad-studio -j 2
cargo run -p cad-studio -- --new target/studio-qa/measure.zcad --name measure --size 10,10,5 --port 7612
node target/studio-qa/drive.mjs http://127.0.0.1:7611
```

Browser driver: installed Chrome 151.0.7922.137, headless, over the DevTools protocol; DOM read
back from the real page, console errors and failed requests collected.

## 3. Outcomes

| Check | Result |
|---|---|
| `cargo fmt --all --check` | clean |
| `cargo clippy --offline --workspace --all-targets -D warnings` | 0 warnings |
| `cargo test --offline --workspace -j 2` | **239 passed, 0 failed, 0 skipped** in 11 packages, 2.39 s wall, 133 640 KB peak RSS |
| `cargo test -p cad-document` | 36 passed (27 recorded for schema 2 + 9 new) |
| `cargo test -p cad-studio` | 13 passed (2 unit + 11 HTTP integration; 3 new) |
| Browser workflow | 7 assertions passed, `consoleErrors` empty |

No earlier test was removed, renamed to hide a failure, or skipped.

## 4. What the tests assert

**Document (9 new tests).**

- A two-feature history builds both extrudes and exactly **one** union; the fake kernel counts the
  calls, and the analytic volume is asserted rather than a self-comparison.
- A shared parameter rebuilds every dependent feature in order.
- A two-feature project round-trips through save and reopen with the same volume and history.
- A join is refused with a stated reason when the kernel does not advertise `BooleanUnion`.
- A second `new_body`, and a `join` with no body, are refused.
- A failing join names its feature ID and keeps the last good mesh.
- Loading refuses a feature whose sketch was created after it, and refuses dangling references.
- A schema-2 feature gains `operation: new_body` on load; a schema-1 file still migrates twice.
- Undo removes the joined feature and its geometry together.

**Studio (3 new HTTP tests).** Real loopback server, real Truck kernel:

- Two 10 × 10 × 5 boxes overlapping 2 × 2 mm over their full height: **980 mm³ ± 1**
  (500 + 500 − 2·2·5), bounds `18 × 18 × 5`, two features `new body` then `join`, both `built`.
  After save the file is reloaded, served again by a new process and still measures 980 mm³; the
  binary STL export parses back to a 980 mm³ mesh.
- A feature that cannot rebuild is reported by ID with the kernel's message while the previous
  980 mm³ mesh stays renderable, and export is refused instead of writing a partial solid.
- The creation endpoints refuse an unknown sketch, an extrude on a missing sketch, a join before a
  body exists, and a profile with impossible parameters.

**Browser workflow (7 assertions).**

1. Initial tree is `Sketch 1 | Extrude 2`, badge `built`, 12 triangles, 500 mm³.
2. Adding a sketch gives `Sketch 3` at the requested origin; the extrude form targets it and offers
   `join` because a body already exists.
3. Moving it to (8, 8) is reflected in the tree (`origins 8, 8`).
4. Joining produces `Extrude 4`, badges `built`/`built`, status bar `72 tri · 980 mm³`, and the
   properties panel shows volume 980 mm³, surface 752 mm², 38 vertices, bounding box 18 × 18 × 5.
5. A forced unbuildable feature shows the banner *"Feature 6 could not rebuild: extrude failed:
   invalid input: profile is not a planar region…"*, the row badge `failed`, the status bar
   `72 tri · 980 mm³ · last good rebuild`, and the export button is disabled with the reason.
6. Undo removes the failed feature and its sketch; the tree and 980 mm³ geometry come back.
7. No console errors, no failed requests.

The palette path (new sketch, extrude, join, save, export) was exercised in a second driver run.

## 5. Measured cost

Same host as every other record: i3-1125G4, 4C/8T, 7.4 GB, Fedora 43, rustc 1.94.0, debug build,
loopback, one new connection per request. Harness: `target/studio-qa/measure.sh`.

| Workload | Result | Compare |
|---|---|---|
| Process start → `/api/health` on the two-feature project | 31–34 ms | 10 ms on the single-feature project (2026-10-25) |
| `GET /api/state` after the join | **17.4 ms** median of 20 | 3.5 ms before the join existed in the same process |
| `POST /api/parameter` (rebuild both features + union) | **17.3 ms** median, 17.9 ms max of 20 | 4.44 ms single-feature baseline |
| `POST /api/save` (two sketches, two features, cached mesh) | 12.6 ms | 4.5 ms single-feature |
| `POST /api/sketch` / `POST /api/sketch/origin` | 2.2 ms / 1.7 ms | — |
| `GET /api/export/stl` (joined solid) | 16.8 ms | 3.98 ms single-feature |

Interpretation: the cost is dominated by rebuilding the whole graph and re-tessellating it on every
request; there is no per-feature cache. ~17 ms is well inside an interactive budget, so no
optimisation was attempted. The measurement exists so the next stage can show a regression or a
win rather than assert one. See `docs/performance-baseline.md` §8.

## 6. Known limitations

- **Linear, single-body history.** No branching, suppression, patterns, multi-body or assemblies.
  A join whose bodies only touch tangentially hits the corpus's recorded `union_tangent_contact`
  failure: the app reports the join failure and keeps the previous body rather than emitting a
  wrong solid.
- **Whole-graph rebuild on every request**, including `GET /api/state`; there is no cache
  invalidation problem because there is no cache.
- **No constraint solver or dimensions.** A sketch is a validated closed profile placed by an
  origin; profiles are driven by parameters, not by dimension annotations.
- **Undo is snapshot-based** and bounded at 64 snapshots, so a very long history is not undoable
  forever.
- **Schema 2 files with one feature** mean exactly what they did before; a schema-2 file with several
  features never existed in practice and would be interpreted as first `new_body`, rest `join`.
- **No autosave or crash recovery.** Save remains explicit and atomic.

## 7. Next tasks

1. **Additional sketch entities.** Arcs and segmented profiles, with analytic area tests and at least
   one cut that exercises them. Revolve now exists but can only turn the primitives and a drawn
   polyline, so a lathe profile with a taper or a fillet is not expressible.
2. **Cache memory measurement.** Measure peak RSS with 32 cached prefixes on a large-mesh project
   before deciding whether the bound or the `Arc<Mesh>` representation changes.
3. **Named revisions and recovery snapshots** (M11f), the persistence piece collaboration and
   versioning need first.

Closed since this document was written: cut through a verified difference (2026-10-28), the
per-feature rebuild cache (2026-10-29) and full-turn revolve (2026-10-29, schema 5).

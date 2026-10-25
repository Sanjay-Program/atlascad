# Baseline Tests

**Canonical data:** `engineering/quality/tests.json`.
**Commands:** [../build-and-test.md](../build-and-test.md).

## 1. Executed in this workspace

| Id | Command | Passed | Failed | Skipped | Notes |
|---|---|---:|---:|---:|---|
| `t-cad-contract` | `cargo test --release -p cad-contract` | 55 | 0 | 0 | 54 unit + 1 doc test |
| `t-cad-kernel-truck` | `cargo test --release -p cad-kernel-truck` | 26 | 0 | 0 | analytic volumes, refusal of difference, manifold orientation, cancellation and deadlines |
| `t-cad-kernel-truck-corpus` | `cargo run -p cad-kernel-truck --example corpus` | 8 | 0 | 2 | the analytic boolean corpus: 8 ok, 2 recorded known failures, 3 difference fixtures recorded unsupported, exit code 0 |
| `t-cad-engineering` | `cargo test --offline -p cad-engineering` | 30 | 0 | 0 | 4 unit + 26 integration, real git fixture; report totals exclude superseded records |
| `t-cad-cli` | `cargo test --offline -p cad-document -p cad-cli` | 10 | 0 | 0 | 9 end-to-end (units → header → kernel → STL) + 1 multi-process project workflow |
| `t-cad-agent-api` | `cargo test -p cad-agent-api` | 13 | 0 | 0 | contract version, grant intersection, zero budgets, error taxonomy |
| `t-cad-agent-runtime` | `cargo test -p cad-agent-runtime` | 16 | 0 | 0 | lifecycle, budgets, quarantine, the strict schema subset |
| `t-cad-agent-geometry` | `cargo test -p cad-agent-geometry` | 24 | 0 | 0 | 12 unit + 12 integration, the agent driven through a real registry |
| `t-cad-document-schema2` | `cargo test --offline -p cad-document` | 27 | 0 | 0 | rectangle/circle/polygon profiles, schema-2 save/load, schema-1 migration, undo/redo, corrupt input |
| `t-cad-entitlements` | `cargo test --offline --workspace` | 10 | 0 | 0 | policy only; no billing provider exists |
| `t-cad-io` | `cargo test --offline -p cad-io` | 6 | 0 | 0 | binary STL facet layout, round trip, truncation and declared-count refusal |
| `t-cad-studio` | `cargo test --offline -p cad-studio` | 10 | 0 | 0 | 2 unit + 8 real-socket HTTP tests, including foreign/empty Host, same-origin, body limits and STL parse-back |
| `t-cadcraft-geom` | `cargo test -p cadcraft-geom` | 84 | 0 | 0 | from the audit session |

Total recorded passes: **319**. The workspace summary records **227** tests in the root workspace
(`t-platform-workspace-20261025-final`); the larger total includes the CADCraft audit suite and the
corpus binary, which are not part of that workspace run.

The two "skipped" corpus fixtures are **recorded known failures**, not skipped tests: tangent contact
and intersection at coordinates near 1e6 mm ran and failed as the baseline expects. The corpus prints
them and the gate stays green, so a future fix shows up as an unexpected pass rather than silently
changing the answer.

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

## 3. What the tests actually assert

* **Analytic values, not self-consistency:** 10×10×5 box = 500 mm³ ± 1e-6; union 1000 + 160 − 40 =
  1120 mm³ ± 1; intersection 2×2×10 = 40 mm³ ± 0.5; inch conversion 25.4³ mm³.
* **Manifold output:** welded mesh has 0 open edges and every face normal points outward for the box
  fixture (regression for a measured inward-wound cap).
* **Honest failure:** boolean difference must return `Unsupported`, never a wrong solid; a stale or
  foreign handle must be `UnknownSolid`.
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

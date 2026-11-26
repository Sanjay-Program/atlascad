# AtlasCAD Test Strategy

**Date:** 2026-11-26
**Principle:** a claim is only as strong as the check behind it. The strategy is layered so a
regression is caught at the cheapest layer that can see it, and so honesty properties (refusals,
provenance, approvals) have explicit tests rather than being implied by happy paths.

---

## 1. Layers

| Layer | What it proves | Command | Current size |
| --- | --- | --- | --- |
| Unit tests | units, geometry math, parsers, state machines | `cargo test --workspace` | part of 878 |
| Crate integration tests | document lifecycle, drafting, CAE, CAM, agents | same | 116 suites total |
| HTTP integration | routes, validation, auth boundary, exports, recovery | `cargo test -p cad-studio --test http_api` | 48 |
| Analytic kernel corpus | exact volumes/areas against analytic answers, with recorded known-failures | `cargo run -p cad-kernel-truck --example corpus` | 11 fixtures |
| Product harness | every route/export/recovery/share path on a live server | `python3 scripts/full-check.py` | 1,206 assertions |
| Live simulation | real release binaries, HTTP/CLI/share, `kill -9` | `./scripts/simulate.sh` | 56 |
| Browser checks | embedded assets boot, layout, commit-button reachability, tree, downloads, shared session | `node scripts/browser-check.mjs` (inside simulate) | 30 loopback + 34 shared |
| DOM accessibility | every form control has a programmatic name | `node scripts/a11y-audit.mjs` (inside simulate) | 191 controls, 0 unnamed |
| Large-model performance | import, state, STL and RSS on a 10k-triangle fixture | `python3 scripts/benchmark-large-model.py` | 10,092 triangles, exit 0 on count match |
| Deployment validation | shell syntax, compose interpolation, Dockerfile inputs, Caddyfile | `./deploy/validate.sh` | 17 scripts, 2 composes, 2 Caddyfiles |
| Supply chain | licence policy, deterministic SBOM | `atlas-eng licences`, `atlas-eng sbom` | 112 packages/components |

The release gate runs all of the above:

```sh
./scripts/gate.sh          # everything, including live simulation and full-check
./scripts/gate.sh --quick  # skip the live simulation
```

---

## 2. Tests added for integrity properties

| Property | Test |
| --- | --- |
| An engine without an adapter cannot report success | `engines_ecosystem_api`, `test_engine_manager_orchestration` |
| A benchmark cannot invent accuracy | `engine_discovery_guides_and_benchmark_api` |
| A refused job cannot promote a graph node | dependency-graph test (`assert_ne!(state, "Current")`) |
| A workflow without `auto_approve` stops at its gate | `agent_workflows_require_approval_by_default` |
| Command history uses a real clock and a record digest | command-history assertions in the enterprise API test |

These replace tests that previously asserted the fabricated values. The replacement is the point:
the old tests encoded the defect.

---

## 3. Known gaps

| Gap | Consequence | Proposed test |
| --- | --- | --- |
| No fuzzing of STEP/DXF/3MF parsers | malformed input risk is reviewed, not fuzzed | `cargo-fuzz` targets with size/count budgets |
| No axe-core/Lighthouse run | contrast still unmeasured (names are now audited) | axe-core in `browser-check.mjs` |
| Large-model browser behaviour unmeasured | frame time for a 10k model unknown | drive `benchmark-large-model.py`'s fixture in `browser-check.mjs` and sample frame times |
| No external-engine contract test | adapters absent; cannot verify | contract test per adapter (input file → output parse → provenance) |
| No Windows/macOS run | platform support unclaimed | CI matrix when those hosts exist |
| No sustained session/soak test | memory growth unknown | 1-hour edit loop with RSS sampling |
| No accessibility keyboard walkthrough | focus order partly unverified | scripted Tab traversal of core tasks |

---

## 4. Rules for future tests

1. Never assert a fabricated number to make an endpoint look functional; assert the refusal.
2. A fix is not complete until its regression test fails without the fix.
3. Prefer analytic or reference answers over snapshots of the program's own output.
4. Keep the gate runnable offline; tool-dependent checks must skip with an explicit reason, never
   silently pass.
5. Report blocked checks as blocked. The gate currently cannot run advisory scanning because
   `cargo-deny`/`cargo-audit` are not installed on this host.

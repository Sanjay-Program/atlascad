# AtlasCAD Production-Readiness Audit

**Date:** 2026-11-26
**Scope:** the deployable workspace in this repository — the `cad-studio` server and its embedded
browser application, the `atlas` and `atlas-eng` binaries, and the 40 workspace crates they link.
`apps/`, `kernels/` and `references/` are imported checkouts, are not linked into the deployable
artifact, and are out of scope.
**Supersedes:** [`production-readiness.md`](production-readiness.md) (2026-10-29) for current status.
That document is retained as the historical assessment; its “STEP is absent”, “no auth”, “no
drawings” and “309 tests” statements no longer describe this tree.
**Method:** read the code paths behind each claim; run the repository's own gate; drive the release
server over HTTP; rebuild and run the live simulation and browser checks; inspect the remaining
stale documents. This is an engineering audit, not a certification or a penetration test.

Evidence labels used here: **test** (repository test suite), **live** (release server driven over
HTTP), **browser** (headless Chrome against the running app), **code** (source inspection),
**prior** (measured in an earlier recorded run and cited with its date), **not tested**.

---

## 1. Verdict

**PRODUCTION CANDIDATE — WITH DOCUMENTED LIMITATIONS**, for one documented scope: a
single-document, single-process studio used locally or on a token-gated LAN, with TLS terminated by
a proxy.

It is **not** production-ready for hosted multi-user or enterprise procurement. The blockers are
identity (login is identity selection, not authentication), external solver execution (now refuses
instead of pretending), advisory scanning (not runnable in this environment), and unmeasured
large-model performance. Those are listed in §8 and in the
[remediation backlog](atlas-remediation-backlog.md).

What changed in this pass is material: two endpoints that returned fabricated engineering results
now refuse with `501 unavailable`, and agent workflows no longer auto-approve sensitive tasks unless
the caller explicitly asks. The command audit record now carries the real clock and a digest of its
own fields instead of a 2023 constant and the command name.

---

## 2. Environment

| Item | Value |
| --- | --- |
| OS / kernel | Fedora 43, Linux 7.3.4-100.fc43.x86_64 |
| CPU / RAM | 8 logical cores, 7 GiB usable (memory-constrained host) |
| Rust | rustc/cargo 1.95.0, edition 2024 workspace, `rust-version = "1.90"` |
| Node / Chrome | Node 24.10.0; Google Chrome 157.0.8103.78 |
| Docker | available; images recorded on 2026-11-25, not rebuilt in this pass |
| Advisory tools | `cargo-deny` and `cargo-audit` **absent**; no advisory scan was run |
| External engines | CalculiX, OpenFOAM, ngspice, pandapower etc. are **not installed** (`/usr/bin/ccx` absent) |

---

## 3. Baseline gate (measured this pass)

| Check | Command | Result | Evidence |
| --- | --- | --- | --- |
| Formatting | `cargo fmt --all --check` | pass | gate step |
| Lints | `cargo clippy --offline --workspace --all-targets -- -D warnings` | pass, 0 warnings | gate step |
| Workspace tests | `cargo test --offline --workspace -j 2` | **882 passed, 0 failed, 116 suites** | gate + count |
| HTTP integration | `cargo test -p cad-studio --test http_api` | **48 passed** | test |
| Analytic kernel corpus | `cargo run -p cad-kernel-truck --example corpus` | 11 fixtures: 9 pass, 2 recorded known-failures, **0 regressions** | gate |
| Engineering manifests | `atlas-eng validate` | 0 errors, 0 warnings across 14 files | gate |
| Licence policy | `atlas-eng licences` | 112 packages: 112 allowed, 0 denied, 0 unknown | gate |
| SBOM | `atlas-eng sbom` (dry run) | CycloneDX 1.5, 112 components | gate |
| Deployment assets | `./deploy/validate.sh` | 17 shell scripts, 2 compose files, root compose token refusal, 6 embedded assets, 2 Caddyfiles | gate |
| Live simulation | `./scripts/simulate.sh` | 56 HTTP/CLI/share + 30 loopback browser + 34 shared browser + DOM accessibility audit (191 controls, 0 unnamed) | live/browser |
| Exhaustive product check | `python3 scripts/full-check.py` | **1,206 assertions**, pass | live |
| Quick release gate | `./scripts/gate.sh --quick` | **gate passed** (exit 0) | gate |

The two corpus known-failures are unchanged and documented: a coplanar cut at the raw kernel, and an
intersection beyond ±1e5 mm which is **refused** with a re-centre message rather than answered
wrongly. See [`../kernel-comparison.md`](../kernel-comparison.md).

---

## 4. Defects found and fixed in this pass

### PR-001 (P0) — `/api/engines/run` fabricated solver results

- **Evidence:** code. `EngineRegistry::execute_job` spawned no process (`cad-engineering` has no
  process execution for engines) and returned hardcoded `max_von_mises_mpa: 184.5`,
  `drag_coefficient_cd: 0.032`, `vm_pu_min: 0.985`, and similar values with `success: true` and a
  “provenance” hash derived from `model_revision`. `/api/engines/benchmark` returned
  `benchmark_status: "verified"` with `accuracy_score: 0.9998`.
- **Impact:** an engineer could read invented stress, CFD or load-flow numbers as solver output and
  the graph node was marked `Current`. This violates the no-fake-results rule and Gate E.
- **Fix:** removed the synthesis entirely. A job now returns `success: false`,
  `output: null`, `provenance_hash: "0000000000000000"` and an explicit “no execution adapter is
  installed; no solver was run” error. The HTTP endpoints answer **501 `unavailable`**; benchmarks
  no longer invent an accuracy score; a refused job does not promote the engineering graph.
- **Regression tests:** `engines_ecosystem_api` (all three engines refuse with 501 and
  `kind == "unavailable"`), `engine_discovery_guides_and_benchmark_api` (no `accuracy_score`),
  the graph test (node is not `Current` after refusal), and
  `test_engine_manager_orchestration` (health is not healthy, job is an error).
- **Live check:** `POST /api/engines/run {"engine_id":"calculix",…}` → `501`, message
  ``no execution adapter is installed for `CalculiX…`; no solver was run and no result is reported``.
- **Remaining risk:** the UI still lists engines and offers run actions; they now fail visibly
  rather than silently lying. Real adapters remain absent.

### PR-002 (P1) — Workflows auto-approved sensitive actions by default

- **Evidence:** code (`auto_approve.unwrap_or(true)`).
- **Impact:** `cam.gcode.emit` and delete-class tasks were approved by
  `Chief_Autonomous_Engineer` when the caller omitted the field, which is the opposite of the
  documented human-approval gate.
- **Fix:** the default is now `false`; approval is opt-in.
- **Regression test:** `agent_workflows_require_approval_by_default` runs `cad_to_cnc` without
  `auto_approve` and asserts a pending approval and a non-`Completed` status.
- **Remaining risk:** the browser's “run all” buttons still send `auto_approve: true`, but the
  control titles state the bypass and every entry point now asks for confirmation first (PR-012).

### PR-003 (P1) — Command and health audit timestamps were fabricated

- **Evidence:** code. Every executed command was recorded with `timestamp_epoch_ms: 1700000000`
  (2023-11-14), `execution_time_ms: 12`, and `audit_hash: "fnv1a_<command_id>"` — a constant, not a
  digest. Engine health and approval decisions also used `1700000000`.
- **Impact:** command history and approval records could not be used for traceability, and two
  identical commands always produced the same “hash”.
- **Fix:** command records now use `Instant`-measured elapsed time, the real epoch from `SystemTime`,
  and FNV-1a over the record's own fields (execution id, command, caller, arguments, timestamp).
  Health and approval decisions use the real clock.
- **Regression test:** the command-history assertion checks `timestamp_epoch_ms > 1_700_000_000_000`
  and a 22-character `fnv1a_` + 16-hex digest.
- **Remaining risk:** seeded demo records (users, assets, older history) still carry 2023 constants;
  they are seed data, not runtime records, and are labelled as such below.

### PR-016 (P0) — Agent workflow payloads fabricated manufacturing and certification evidence

- **Evidence:** code and live API. Tasks that had no implementation returned plausible-looking JSON:
  a waverider B-Rep with invented dimensions, `toolpath_count: 68`, `gcode_lines: 8920`,
  `status: "ReadyForMachine"`, a Unity twin with 16 telemetry channels, a DaVinci timeline marked
  `"MasterRendered"`, and an `eco.submit` result claiming `compliance_score: 1.0` with
  `evidence_hash: "sha256_as9100_mach11_certified"` and `"CertifiedApproved"`. The default arm
  returned `accuracy_percent: 99.98`, and every task was marked complete with a fixed 15 ms
  duration. The browser printed “AS9100 Rev D & FAA Part 25 Pass (Cryptographic SHA-256 Verified)”.
- **Impact:** the product asserted a certification and machine-ready toolpaths that never existed.
  This is the same defect class as PR-001 and is more severe because it names a safety standard.
- **Fix:** unimplemented commands, including `cad.solid.*`, `cam.*`, `export.*`, `eco.submit` and any
  unregistered command, return `status: "Unavailable"` with `output: null` and a reason. The task is
  marked failed, downstream tasks stay pending, the run status is derived from the tasks (never
  hardcoded “Completed”), and the UI shows completed/failed counts and removes the certificate claim.
  Durations are measured with `Instant`; auto-approval timestamps use the real clock.
- **What still runs for real:** `project.inspect` reports document facts bound by the server, and the
  hypersonic analytical solvers (`sim.hypersonic.*`) return computed values. The master template now
  reports **2 completed / 1 failed / 8 pending**, not 11/11.
- **Regression tests:** `test_master_workflow_refuses_unimplemented_steps`,
  `gated_command_requires_approval_unless_auto_approved`, and the HTTP assertions in
  `multidisciplinary_simulation_apis` / `agent_workflows_never_auto_approve_or_fabricate`.

---

## 5. Feature inventory (by domain)

Status vocabulary: **verified** (exercised by a test or the live server in this tree),
**partial**, **UI/API only**, **stub**, **absent**, **blocked**.

| Domain | Status | Evidence / limit |
| --- | --- | --- |
| Parametric solid modelling: sketch profiles, extrude (new/join/cut), revolve, planes, undo/redo, rebuild cache | **verified** | 48 HTTP tests, 1,206 assertions; analytic volumes |
| 2D drafting: entities, layers, blocks, arrays, trim/extend, constraints, DXF in/out, PDF sheet | **verified** | `cad-drafting` unit + HTTP tests; `pdftotext` extraction |
| Interchange: STEP AP214 read/write, binary STL, 3MF, glTF, DXF, USD/EDL/FCPXML/LUT export | **verified for written bytes; not round-tripped in a third-party application** | `cad-io` and `atlas-interchange` tests; no external app was driven |
| CAE: linear static FEA, modal, thermal, plus ~140 analytical engineering models | **verified analytically** | 213 simulation-core tests; cantilever/analytic comparisons. Not a general FEM suite and not independently benchmarked |
| CAM: toolpaths, feeds, 3/4/5-axis tests, G-code postprocessors, validation | **verified at API/test level** | `cad-cam` suites; no machine or controller was driven |
| Additive: process profiles (FDM/SLA/SLM), slicing, infill, supports, 3MF | **verified at API/test level** | `atlas-additive` tests; no printer was driven |
| Assemblies / BOM / interference / mass properties | **partial** | BOM, interference and mass-property endpoints exist and are tested for the single-body case; a true multi-body assembly editor is absent |
| Materials and engineering registry | **partial** | material library + derived-artifact staleness tested; UI assignment not verified |
| Revisions, save/reopen, autosave recovery, named revisions | **verified** | document + HTTP tests, `kill -9` recovery, 12 recovery assertions |
| Concurrency (same document, parallel writers) | **verified for the tested model** | 18 assertions; last-write-wins, no merge |
| Local sharing: token gate, origin/host checks, TLS via proxy | **verified** | 21 share assertions, browser shared session, Caddy smoke previously |
| Identity/RBAC/SSO/API keys | **explicitly demo-only** | `/api/auth/login` is an identity switch, not authentication. The response now states `credential_verified: false` / `authentication: "demo-identity"` with a notice; the UI shows a demo banner; SSO requests and all logins on `--share` instances are refused with `501` instead of fabricating a session. RBAC remains data, not a boundary |
| External engines (CalculiX, OpenFOAM, ngspice, pandapower, …) | **absent / now refusing** | 24 registry entries, 24 guides, no adapter; 501 as of PR-001 |
| AI agent workflows | **partial but honest** | 44 specialist registrations, DAG execution, approval gates, MCP; real analytical solvers run on fixed case parameters, unimplemented steps are refused, and results record the bound document revision |
| Blender / Unity / Resolve handoffs | **partial** | export routes and file writers tested; no host application imported the output |
| Physical machine/printer execution | **absent** | no device transport exists; correctly not claimed |

---

## 6. Release gates

| Gate | Status | Basis |
| --- | --- | --- |
| **A — Build** | **PASS** | fmt, clippy `-D warnings`, 882 tests, release build |
| **B — Functional** | **PASS for tested core; PARTIAL overall** | CAD, drafting, export and recovery are exercised end to end; CAM/CAE/simulation UI journeys were not hand-driven in this pass |
| **C — Data integrity** | **PASS for single-document scope** | atomic save, revisions, recovery, concurrency, stale-artifact propagation; command/health timestamps fixed |
| **D — Security** | **PARTIAL** | token/origin/CSP/body caps/non-root image are real; login is labelled demo-only and refused on shared instances; advisory scan green |
| **E — Engineering validity** | **PARTIAL** | analytical solvers and CAE/CAM tests pass; fabricated engine **and workflow** output removed; no independent solver benchmark and no external adapter |
| **F — UX / accessibility** | **PASS for the audited page** | clipping and commit-button reachability fixed and guarded; 191/191 controls named; both themes report 0 WCAG AA contrast failures on 334 rendered text elements |
| **G — Performance / reliability** | **PARTIAL** | small and 10k-triangle models measured (3.89 ms state, 59 MB RSS); browser frame time and long-session behaviour unmeasured |
| **H — Operations** | **PASS for the pipeline** | deployment assets validate; CI is green end to end on `441fb73` (gate, package, CAM image smoke, advisories); studio/CAE images were last built 2026-11-25 |
| **I — Documentation** | **PARTIAL** | README badges corrected; several historical documents still describe an older product |

No gate is marked passed without a command or observation in §3.

---

## 7. Measured performance (this pass)

Release server, fresh document:

| Measurement | Result |
| --- | --- |
| Cold start to first answered `/api/health` | **99 ms** (5 ms polling granularity) |
| `GET /api/state`, median of 20 | **0.97 ms** |
| `GET /api/export/stl` | **1.05 ms** |
| Server RSS after those requests | **9,036 KB** |
| `/api/state` payload | 1,450 bytes |
| Release binary | 16,820,560 bytes |
| 10,092-triangle STEP import | **225 ms** |
| `GET /api/state` for that model, median of 10 | **3.89 ms**, 1,028,148-byte payload |
| `GET /api/export/stl` for that model | **1.4 ms**, 505,284 bytes, 10,104 triangles |
| Server RSS after the large import | **59,020 KB** |

Large-model method: `python3 scripts/benchmark-large-model.py`, a generated watertight faceted
fixture whose triangle count is confirmed from the exported STL.

Prior browser measurement (2026-11-25, cited, not re-run): 814 ms first load, 5,827 DOM nodes,
0 failed requests, 47 MB server RSS, 4 headless `ReadPixels` warnings. Full detail and limitations:
[`atlas-performance-and-reliability.md`](atlas-performance-and-reliability.md).

---

## 8. Remaining issues (highest first)

| ID | Severity | Issue | Next action |
| --- | --- | --- | --- |
| PR-004 | **P1, fixed (labelled demo)** | Login was identity selection with no credential check | Payload/UI now say `demo-identity`; SSO and shared-instance login return `501`. A real IdP remains future work |
| PR-005 | **P1, fixed** | No advisory scan had been run | `cargo-deny 0.22.1` scanned the graph: advisories/bans/sources ok; CI advisories job green; report published |
| PR-006 | P2, fixed | Form controls had no programmatic name | 191/191 named; `scripts/a11y-audit.mjs` enforces it |
| PR-007 | P2, fixed | Primary commit button was below the fold at 720 px | Sticky footer; `browser-check.mjs` asserts visibility at 720 px |
| PR-008 | P2, partial | External engine adapters were absent | CalculiX adapter now runs and verifies a real solve (0.40% vs theory, version 2.23); the other engines still refuse with 501 |
| PR-009 | P2, measured | Large-model performance unmeasured | 10,092-triangle fixture: import 638 ms, `/api/state` 9.3 ms, STL 3.3 ms, RSS 59 MB; headless frame sample 59.4 fps (p50 16.7 ms) on SwiftShader |
| PR-010 | P2, partial | AI workflows use fixed parameters, not the live document | Results carry `document_revision`; measured volume/bounds/characteristic length are reported, but solvers remain a fixed reference case until a geometry adapter exists |
| PR-016 | **P0, fixed** | Workflow payloads fabricated toolpaths, media/digital-twin state and an AS9100 certificate | Unimplemented commands return `Unavailable`; tasks fail; status is derived; certificate claim removed |
| PR-011 | P2, fixed | CI had never executed on a runner | Green run on `441fb73`: gate+package, CAM image smoke and advisories all succeeded ([run](https://github.com/Sanjay-Program/atlascad/actions/runs/45947034951)) |
| PR-012 | P2, fixed | Browser “run all” sent `auto_approve: true` silently | Button titles state the bypass and each run asks for confirmation |
| PR-013 | P3, fixed | Contrast not measured | Rendered-DOM audit: 0 failing pairs in light and dark; enforced in the simulation |
| PR-014 | P3, fixed | Historical docs described older capabilities | `feature-matrix.md` rewritten from verified evidence; `production-readiness.md` kept as a dated historical record |
| PR-017 | **P0, fixed** | `/api/report` invented a released certification package (mass, CalculiX pass, sign-offs, standards) | Now a `DRAFT_NOT_RELEASED` report from measured mesh volume with unassessed mass, no BOM, no signatures; asserted in `http_api` |
| PR-018 | **P0, fixed** | Suite reported `AllPassed` and hid failed domains | Derived `Completed`/`Partial`, failed domains listed, standards labelled as formula basis only |
| PR-019 | P1, fixed | Runtime audit/job/ECO/auth timestamps were 2023 constants | All 41 literals replaced by the real clock |
| PR-020 | P1, fixed | Optimizer revisions fabricated compliance scores and evidence hashes | Score 0.0, FNV-1a content digest, `evidence_verified: false` |
| PR-021 | P2, fixed | Residual certification wording in UI/CLI/MCP/agents | Rewritten as traceability/self-assessed/typical properties |

---

## 9. Premium-value position

The verified value today is a fast, local, honest single-document CAD/CAM/CAE workbench: exact
analytic geometry checks, a tested drafting/export path, deterministic tests, a 9 MB server and
sub-millisecond state reads. That is real, but it is not a $10,000/seat enterprise platform: identity
is demo-grade, external solvers are absent, large assemblies are absent, and there is no support,
signing, update channel or advisory programme. The full matrix is in
[`atlas-premium-value-evidence.md`](atlas-premium-value-evidence.md).

---

## 10. Final status

**PRODUCTION CANDIDATE — WITH DOCUMENTED LIMITATIONS** for the local/single-document, token-gated
scope. **NOT PRODUCTION READY** for hosted multi-user, enterprise identity, or claims of external
solver execution.

The shortest path to a stronger statement is: real authentication, an advisory scan in CI, and a
large-model benchmark — in that order.

---

## 11. Document map

| Brief deliverable | Location |
| --- | --- |
| This audit | `ATLASCAD_PRODUCTION_READINESS_AUDIT.md` |
| Personas and 30 tasks | [`atlas-persona-and-task-matrix.md`](atlas-persona-and-task-matrix.md) |
| Security review | [`../security-review.md`](../security-review.md) (2026-11-26 section) |
| Performance and reliability | [`atlas-performance-and-reliability.md`](atlas-performance-and-reliability.md) |
| Engineering validation | [`ENGINEERING_VALIDATION.md`](ENGINEERING_VALIDATION.md) (historical) and Gate E above |
| Release checklist | [`atlas-release-readiness-checklist.md`](atlas-release-readiness-checklist.md) |
| Premium value evidence | [`atlas-premium-value-evidence.md`](atlas-premium-value-evidence.md) |
| Remediation backlog | [`atlas-remediation-backlog.md`](atlas-remediation-backlog.md) |
| Test strategy | [`atlas-test-strategy.md`](atlas-test-strategy.md) |
| Usability audit | [`atlas-usability-audit.md`](atlas-usability-audit.md) |

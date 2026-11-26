# AtlasCAD Remediation Backlog

**Date:** 2026-11-26 · Companion to
[`ATLASCAD_PRODUCTION_READINESS_AUDIT.md`](ATLASCAD_PRODUCTION_READINESS_AUDIT.md).

Priority uses P0 (data loss / safety / fabricated engineering result), P1 (core workflow or security
boundary), P2 (significant reliability or usability), P3 (polish). Safety and integrity override
numeric priority.

| ID | P | Area | Finding | Status | Acceptance |
| --- | --- | --- | --- | --- | --- |
| PR-001 | **P0** | Engineering integrity | `/api/engines/run` and `/benchmark` returned synthesized solver results and marked graph nodes current | **fixed, tested, live-checked** | A job without an adapter returns 501 `unavailable`, `output: null`, no provenance, no promotion |
| PR-002 | **P1** | Agent safety | Workflows auto-approved `cam.gcode.emit`/delete tasks when `auto_approve` was omitted | **fixed, tested** | Omitted field leaves a pending approval; `cad_to_cnc` regression test |
| PR-003 | **P1** | Audit integrity | Command history used timestamp `1700000000`, fixed `12 ms`, and `fnv1a_<command>` as its “hash”; health/approval used the same constant | **fixed, tested** | Real clock, measured elapsed, digest over record fields |
| PR-004 | **P1** | Authentication | `/api/auth/login` selected a seeded identity by `user_id`/email without a credential and granted Chief Architect + enterprise plan | **fixed as labelled demo identity** | Login returns `credential_verified: false` and an explicit demo notice; the UI shows a banner; SSO returns `501`; any login on a `--share` instance returns `501`. A real IdP is still absent, so the build must not be hosted as an authentication boundary |
| PR-005 | **P1** | Supply chain | `cargo-deny`/`cargo-audit` absent; no advisory scan run; CI workflow never executed on a runner | **blocked (tooling)** | First advisory report published from CI; `deny.toml` job green |
| PR-006 | P2 | Accessibility | 59 of 191 static controls had no programmatic name | **fixed, tested** | `scripts/a11y-audit.mjs` reports 0/191 unnamed and fails otherwise; it runs in `simulate.sh` |
| PR-007 | P2 | UX | Primary commit button sat below the fold at 720 px | **fixed, tested** | Sticky commit footer; `browser-check.mjs` asserts the button is inside the panel at 1280×720 at two scroll positions |
| PR-008 | P2 | Integrations | 24 external engine entries, no execution adapter | **open / correctly refusing** | Adapter executes and validates output, or run UI is removed |
| PR-009 | P2 | Performance | No 10k-face benchmark, memory ceiling or frame-time data; a 1 MiB body cap also made such imports impossible over HTTP | **measured / cap fixed** | `scripts/benchmark-large-model.py`: 10,092 triangles import in 225 ms, `/api/state` 3.89 ms / 1,028,148 bytes, STL 1.4 ms, RSS 59,020 KB. Frame time still unmeasured |
| PR-010 | P2 | AI | Workflow tasks call analytical solvers with fixed parameters, not the active document revision | **open** | Task inputs bound to document revision and recorded in provenance |
| PR-011 | P2 | Delivery | CI config exists but has not run on a runner | **open** | One green runner execution linked from the release record |
| PR-012 | P2 | UX / safety | Browser “run all” sends `auto_approve: true` without saying it bypasses review | **fixed, tested by inspection** | Button titles state the bypass and each entry point asks for confirmation before running |
| PR-013 | P3 | Accessibility | Contrast ratios not measured (CSS sampler contradicted the rendered page) | **open** | axe-core/Lighthouse run recorded with failing pairs |
| PR-014 | P3 | Documentation | `production-readiness.md` and `feature-matrix.md` still describe an older capability set | **partially addressed** | Dated banners added; rewrite scheduled |
| PR-015 | P3 | Data | Seeded demo users/assets/history carry 2023 timestamps | **accepted** | Seed data is labelled; no runtime record uses a constant |

## Order of work

1. **PR-004** — closed by labelling: the identity switch is demo-only, SSO is refused, and shared
   instances refuse login. A real IdP is the remaining work before hosted multi-user use.
2. **PR-005** — an unscanned dependency graph is an unknown risk, not a passed gate (blocked on
   tooling in this environment).
3. **PR-009** — measured at 10k triangles; capture browser frame time next.
4. **PR-006 / PR-007 / PR-012** — closed with scripted or inspected checks.
5. **PR-008 / PR-010** — adapter and provenance work; refusal is the correct interim state.

PR-001/002/003 are closed only because each has a regression test in the workspace suite; a future
change that reintroduces synthetic output or auto-approval fails the gate.

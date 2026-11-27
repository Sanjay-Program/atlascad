# AtlasCAD Release Readiness Checklist

**Date:** 2026-11-26 · Every status below is backed by a command or observation in the
[production-readiness audit](ATLASCAD_PRODUCTION_READINESS_AUDIT.md). An unchecked item is not a
“nice to have”; it is a reason not to claim the corresponding scope.

---

## 1. Build and test

| Item | Status | Evidence |
| --- | --- | --- |
| Clean release build | **PASS** | `cargo build --release -p cad-studio -p cad-cli` |
| Formatting | **PASS** | `cargo fmt --all --check` |
| Clippy, warnings denied | **PASS** | `cargo clippy --workspace --all-targets -- -D warnings` |
| Workspace tests | **PASS** | 882 passed, 0 failed, 116 suites |
| HTTP integration tests | **PASS** | 48 passed |
| Analytic kernel corpus | **PASS with 2 recorded known-failures** | 9 ok, 0 regressions |
| Exhaustive product check | **PASS** | `full-check.py`, 1,206 assertions |
| Live HTTP/CLI/browser simulation | **PASS** | 56 + 29 + 33 assertions |

## 2. Security and supply chain

| Item | Status | Evidence / blocker |
| --- | --- | --- |
| Token required on shared `/api/*` | **PASS** | 21 share assertions, live 403 without origin |
| Origin and Host checks | **PASS** | live 403, HTTP tests |
| CSP, `nosniff`, frame denial, referrer policy | **PASS** | `crates/cad-studio/src/http.rs` |
| Request body/media-type caps | **PASS** | HTTP tests |
| Secrets not in args or image env | **PASS** | token file/env path, compose secret staging |
| Non-root container | **PASS (2026-11-25)** | `uid=10001(atlas)` recorded; image not rebuilt today |
| Dependency licence policy | **PASS** | 112 allowed, 0 denied, 0 unknown |
| SBOM generation | **PASS (dry run)** | CycloneDX 1.5, 112 components |
| Advisory scan | **BLOCKED** | `cargo-deny`/`cargo-audit` absent; no scan run |
| Real authentication | **FAIL for hosted scope** | login selects a seeded identity without a credential |
| CI executed on a runner | **PASS** | green on `441fb73`: gate, package, CAM image smoke, advisories ([run](https://github.com/Sanjay-Program/atlascad/actions/runs/45947034951)) |

## 3. Data integrity

| Item | Status | Evidence |
| --- | --- | --- |
| Atomic save | **PASS** | temp-file + fsync + rename tests |
| Save/reopen, undo/redo | **PASS** | HTTP + document tests |
| Crash recovery | **PASS** | `kill -9`, 12 assertions |
| Named revisions | **PASS** | restore survives save/reopen |
| Concurrency (parallel writers) | **PASS for the tested model** | 18 assertions; last-write-wins |
| Stale derived artifacts | **PASS** | dependency-graph propagation tests |
| Refused engine jobs do not promote graph nodes | **PASS** | PR-001 regression test |
| Runtime timestamps and audit digests | **PASS** | PR-003 regression assertion |

## 4. Deployment and operations

| Item | Status | Evidence |
| --- | --- | --- |
| Compose interpolation and token refusal | **PASS** | `deploy/validate.sh` |
| Shell syntax for deploy scripts | **PASS** | 17 scripts |
| Caddyfile syntax | **PASS** | 2 files, real `caddy:2` validation |
| Dockerfile copies only build inputs | **PASS** | explicit `COPY` guard |
| Embedded assets survive `.dockerignore` | **PASS** | 6 assets checked |
| Image built and smoke-tested | **RECORDED 2026-11-25** | studio 139 MB, cam 125 MB, cae 125 MB, quickstart 160 MB; not rebuilt today |
| Healthcheck reflects the app | **PASS** | `GET /api/health` over `/dev/tcp` |
| Backup/restore scripts | **PASS (syntax + prior run)** | `backup.sh`, `restore.sh` |
| TLS termination | **PASS via proxy (prior)** | Caddy smoke stack recorded |
| Rollback plan | **PARTIAL** | document file + volume restore; no versioned application rollback |
| Signed artifacts / update channel | **ABSENT** | do not claim |
| Support process / SLA | **ABSENT** | do not claim |

## 5. Documentation

| Item | Status |
| --- | --- |
| README test counts match the tree | **FIXED** (882 workspace / 49 HTTP) |
| Current readiness audit | **PASS** — this document set |
| Historical docs clearly marked | **PARTIAL** — banners added to the 2026-10-29 readiness and feature matrix |
| API documentation matches implemented routes | **PARTIAL** — 317 path literals; documented subset is accurate, not exhaustive |
| Known limitations published | **PASS** — audit §5/§8 and `docs/implementation/KNOWN_LIMITATIONS.md` |

---

## Release decision

**Ship as:** local or token-gated LAN single-document application, behind a TLS proxy for any
non-loopback use.

**Do not ship as:** hosted multi-user service, enterprise SSO platform, or a system that executes
CalculiX/OpenFOAM/ngspice jobs. The identity layer is explicitly demo-only (PR-004), the advisory
scan and CI are green (PR-005/PR-011), and no external engine adapter exists (PR-008).

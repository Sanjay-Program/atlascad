# Atlas CAD — Integration Report

**Generated:** 2026-10-29T05:35:32Z (unix 1793252132) · **Scanner:** cad-engineering/0.1.0
**Last observation:** 2026-10-29T05:35:32Z · **Host:** linux/x86_64 · 8 CPUs

> Generated file. Edit the manifests under `engineering/manifest/`, not this report.

## 1. Control plane

| Metric | Count |
|---|---:|
| Repositories declared | 24 |
| Repositories observed | 22 |
| — validated | 4 |
| — integrated | 2 |
| — verified | 9 |
| — blocked | 2 |
| — retired | 0 |
| Modules | 34 |
| Interfaces | 11 |
| Capabilities — verified | 20 |
| Capabilities — advertised only | 12 |
| Capabilities — absent | 3 |
| Agents — implemented | 1 |
| Agents — active | 1 |
| Tasks — proposed | 6 |
| Tasks — ready | 44 |
| Tasks — in progress | 0 |
| Tasks — blocked | 4 |
| Tasks — needs review | 0 |
| Tasks — failed | 0 |
| Tasks — verified | 43 |
| Tasks — cancelled | 6 |

## 2. Repositories

| Id | Kind | Status | Licence | HEAD | Verified commit | Tree |
|---|---|---|---|---|---|---|
| `repo.cadcraft` | Application | Validated | MIT OR Apache-2.0 | no git | 0cf3bdea83eb | — |
| `repo.solidcraft` | Application | Validated | MIT OR Apache-2.0 | no git | 775e5155bbff | — |
| `repo.truck` | Kernel | Integrated | Apache-2.0 | no git | 78d699b0792b | — |
| `repo.monstertruck` | Kernel | Discovered | Apache-2.0 | no git | d2ad1ed7aa42 | — |
| `repo.cascade-rs` | Kernel | Discovered | Apache-2.0 | no git | c68c1d2c3c7f | — |
| `repo.hagane` | Kernel | Discovered | MIT OR Apache-2.0 | no git | 45e423c9c5f1 | — |
| `repo.cadrum` | Kernel | Blocked | MIT wrapper + LGPL-2.1 bundled OpenCASCADE | no git | c59916dbbf13 | — |
| `repo.opencascade-rs` | Kernel | Blocked | LGPL-2.1 | no git | 32758df23a13 | — |
| `repo.cadrs` | Reference | Discovered | MIT OR Apache-2.0 | no git | 74dd5771cfc3 | — |
| `repo.cadrust` | Reference | Discovered | MIT | no git | 519cb390fd1c | — |
| `repo.bevy-pmetra` | Reference | Discovered | MIT OR Apache-2.0 | no git | 6933b23d75bd | — |
| `repo.opencadstudio` | External | Discovered | GPL-3.0-only | — | — | — |
| `repo.craft-libs` | External | Validated | MIT OR Apache-2.0 | — | — | — |
| `repo.cad-contract` | Platform | Integrated | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-kernel-truck` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-engineering` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-cli` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-agent-api` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-agent-runtime` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-agent-geometry` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-document` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-entitlements` | Platform | Validated | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-io` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |
| `repo.cad-studio` | Platform | Verified | LicenseRef-AtlasCAD-Personal-NonCommercial | no git | — | — |

## 3. Licence blockers

| Id | Licence | Status | Note |
|---|---|---|---|
| `repo.cascade-rs` | Apache-2.0 | NeedsReview | Declared in Cargo.toml, but no root LICENSE file ships with the checkout; the licence text must be confirmed before any use. |
| `repo.cadrum` | MIT wrapper + LGPL-2.1 bundled OpenCASCADE | Restricted | The MIT licence covers cadrum's Rust source only. A binary that statically links headless OpenCASCADE carries LGPL-2.1 obligations. Owner decision G1 is open; do not distribute. |
| `repo.opencascade-rs` | LGPL-2.1 | Prohibited | LGPL-2.1 bindings. Static linking into a closed-source binary does not satisfy section 6 without relink rights. Not in any shipping graph today. |
| `repo.opencadstudio` | GPL-3.0-only | Restricted | Verified from OpenCADStudio-main.zip: LICENSE is the GNU GPL v3 text, 1,797 files, 892 Rust. Copyleft: no code may enter the proprietary product. The archive stays unextracted; the local directory is empty. |

## 4. Recorded evidence

* Builds: **16** succeeded, **0** failed, **0** recorded as not run.
* Tests: **35** runs succeeded, **0** failed, **1** not run — 677 tests passed, 0 failed, 2 skipped across the counted per-package records (workspace summaries and superseded records excluded; imported applications and corpus examples are included, so this is not a single-suite root-workspace total).
* Benchmarks: **22** recorded measurement(s).
* Security: **8** open finding(s); **0/3** scans ran successfully.
  * S-1 — No advisory or dependency-policy scan is running (High, Blocked)
  * S-2 — LGPL OpenCASCADE is one cargo add away from the shipping binary (High, NeedsReview)
  * S-3 — Untrusted CAD files lack import budgets in SolidCraft IO (High, Proposed)
  * S-4 — MCP loopback control port and agent surface are unaudited (Medium, Proposed)
  * S-5 — Vendored dependency and patched truck provenance needs release controls (Medium, Proposed)
  * S-6 — Secret handling for future SaaS and AI features is convention, not a check (Medium, Proposed)
  * S-7 — No structured log policy for project content (Low, Proposed)
  * S-8 — The agent runtime is in-process and unscheduled (Medium, Proposed)

## 5. Changes

2 change(s) detected against the previous observation.

* **`repo.cad-document`** · FilesModified · 1 tracked file(s) changed against the previous fingerprint
  * ~ src/lib.rs
* **`repo.cad-studio`** · FilesModified · 6 tracked file(s) changed against the previous fingerprint
  * ~ assets/app.css
  * ~ assets/app.js
  * ~ assets/index.html
  * ~ src/lib.rs
  * ~ src/main.rs
  * ~ tests/http_api.rs

## 6. Tasks

Next unblocked tasks, in priority order:

| Id | Priority | Risk | Title |
|---|---|---|---|
| `task.scan.repo-missing.repo-opencadstudio.8e460d74` | BrokenBuild | High | repo.opencadstudio: `references/opencadstudio` disappeared from the scan |
| `task.scan.repo-discovered.repo-cad-io.5f5109b7` | IntegrationBlocker | Medium | repo.cad-io: `crates/cad-io` was not observed in the previous scan |
| `task.scan.repo-discovered.repo-cad-studio.87b6c2b6` | IntegrationBlocker | Medium | repo.cad-studio: `crates/cad-studio` was not observed in the previous scan |
| `task.scan.working-tree-dirty.repo-solidcraft.f4311c36` | IntegrationBlocker | Medium | repo.solidcraft: 7 uncommitted path(s) appeared |
| `task.scan.files-modified.repo-cad-agent-api.21bae687` | InterfaceCompatibility | Low | repo.cad-agent-api: 1 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-agent-geometry.d2aef8bb` | InterfaceCompatibility | Low | repo.cad-agent-geometry: 1 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-agent-geometry.ef9ee2f2` | InterfaceCompatibility | Low | repo.cad-agent-geometry: 2 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-agent-runtime.ceadb347` | InterfaceCompatibility | Low | repo.cad-agent-runtime: 1 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-cli.0e46e3c8` | InterfaceCompatibility | Low | repo.cad-cli: 1 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-cli.7e15d190` | InterfaceCompatibility | Low | repo.cad-cli: 4 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-cli.937c6904` | InterfaceCompatibility | Low | repo.cad-cli: 5 tracked file(s) changed against the previous fingerprint |
| `task.scan.files-modified.repo-cad-cli.b97fe8de` | InterfaceCompatibility | Low | repo.cad-cli: 1 tracked file(s) changed against the previous fingerprint |

## 7. Claims that are not yet verified

* Capability `cap.2d.drafting` is advertised by module.cadcraft.engine, module.cadcraft.ui-egui but no test is recorded against it.
* Capability `cap.2d.constraints` is advertised by module.cadcraft.constraints but no test is recorded against it.
* Capability `cap.io.dxf` is advertised by module.cadcraft.dxf, module.cadcraft.io but no test is recorded against it.
* Capability `cap.io.dwg` is advertised by module.cadcraft.dwg but no test is recorded against it.
* Capability `cap.document.2d` is advertised by module.cadcraft.doc but no test is recorded against it.
* Capability `cap.document.parametric` is advertised by module.solidcraft.doc but no test is recorded against it.
* Capability `cap.3d.sketch` is advertised by module.solidcraft.sketch but no test is recorded against it.
* Capability `cap.kernel.fillet` is advertised by module.solidcraft.kernel but no test is recorded against it.
* Capability `cap.io.step` is advertised by module.solidcraft.io but no test is recorded against it.
* Capability `cap.io.mesh` is advertised by module.solidcraft.io but no test is recorded against it.
* Capability `cap.mcp.server` is advertised by module.cadcraft.mcp, module.solidcraft.mcp but no test is recorded against it.
* Capability `cap.render.3d` is advertised by module.solidcraft.render but no test is recorded against it.
* Module `module.cadcraft.doc` has no recorded test status (Unknown).
* Module `module.cadcraft.dxf` has no recorded test status (Unknown).
* Module `module.cadcraft.dwg` has no recorded test status (Unknown).
* Module `module.cadcraft.io` has no recorded test status (Unknown).
* Module `module.cadcraft.engine` has no recorded test status (Unknown).
* Module `module.cadcraft.ui-egui` has no recorded test status (Unknown).
* Module `module.cadcraft.constraints` has no recorded test status (Unknown).
* Module `module.cadcraft.mcp` has no recorded test status (Unknown).
* Module `module.solidcraft.kernel` has no recorded test status (Unknown).
* Module `module.solidcraft.doc` has no recorded test status (Unknown).
* Module `module.solidcraft.sketch` has no recorded test status (Unknown).
* Module `module.solidcraft.io` has no recorded test status (Unknown).
* Module `module.solidcraft.geom` has no recorded test status (Unknown).
* Module `module.solidcraft.engine` has no recorded test status (Unknown).
* Module `module.solidcraft.ui-egui` has no recorded test status (Unknown).
* Module `module.solidcraft.render` has no recorded test status (Unknown).
* Module `module.solidcraft.gpu` has no recorded test status (Unknown).
* Module `module.solidcraft.mcp` has no recorded test status (Unknown).
* Module `module.solidcraft.vendor-truck` has no recorded test status (Unknown).
* Module `module.kernel.truck` has no recorded test status (Unknown).
* Module `module.kernel.cadrum` has no recorded test status (Unknown).
* Module `module.kernel.opencascade-rs` has no recorded test status (Unknown).
* Agent `agent.cad.import-export` is declared but not implemented; lifecycle is `Discovered`.
* Agent `agent.cad.design-rule-check` is declared but not implemented; lifecycle is `Discovered`.
* Agent `agent.ai.nl-to-cad` is declared but not implemented; lifecycle is `Discovered`.
* Security scan `cargo-audit` is recorded as NotRun: Not installed on this host; no advisory scan has been performed. Installing global tooling was outside this cycle's scope.
* Security scan `cargo-deny` is recorded as NotRun: Not installed. SolidCraft has a deny.toml; CADCraft does not.
* Security scan `cargo-license` is recorded as NotRun: Not installed. The licence inventory was produced from cargo metadata instead.

Counts above are derived from `engineering/manifest/*.json`. A capability is only `verified` when a recorded test exercised it; a build result is not a test result.

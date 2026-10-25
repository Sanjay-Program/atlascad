# Repository Catalog

**Canonical data:** `engineering/manifest/repositories.json` (validated by `atlas-eng validate`).
**Measured detail:** [../repository-audit.md](../repository-audit.md).
**Live table:** [../../engineering/reports/latest-integration-report.md](../../engineering/reports/latest-integration-report.md).

The original audit recorded seventeen entries and sixteen local paths. The 2026-10-23 control-plane
snapshot contains **twenty-two entries**: twenty-one local paths and one external dependency. Later,
the workspace was consolidated into one Git repository. The source revisions used for that import
are recorded in [`source-snapshot.md`](source-snapshot.md). This catalog's HEAD values below are
the revisions recorded by the historical audit; use the source snapshot for the imported revisions.

## Applications

| Id | Path | Version | Licence | HEAD | Integration |
|---|---|---|---|---|---|
| `repo.cadcraft` | `apps/cadcraft` | 0.10.0 | MIT OR Apache-2.0 | `0cf3bdea83eb` | historical audit revision |
| `repo.solidcraft` | `apps/solidcraft` | 0.18.3 | MIT OR Apache-2.0 | `775e5155bbff` | historical audit revision |

Note: SolidCraft's git remote is `tangency-labs/solidcraft` while its `Cargo.toml` declares
`bherbruck/solidcraft`; the checkout is a fork of that upstream.

## Kernels

| Id | Path | Licence | Role |
|---|---|---|---|
| `repo.truck` | `kernels/truck` | Apache-2.0 | primary kernel, integrated through SolidCraft's patched vendor crates |
| `repo.monstertruck` | `kernels/monstertruck` | Apache-2.0 | reference; a fork of truck, not integrated |
| `repo.cascade-rs` | `kernels/cascade-rs` | Apache-2.0 (declared; no LICENSE file) | candidate, not built |
| `repo.hagane` | `kernels/hagane` | MIT OR Apache-2.0 | candidate, not built |
| `repo.cadrum` | `kernels/cadrum` | MIT wrapper + LGPL-2.1 OCCT | **blocked**, outside the shipping graph |
| `repo.opencascade-rs` | `kernels/opencascade-rs` | LGPL-2.1 | **prohibited** in the shipping binary |

## References (read-only)

`repo.cadrs`, `repo.cadrust`, `repo.bevy-pmetra` (permissive), and `repo.opencadstudio`
(**GPL-3.0**, archive only, never extracted into the build).

## Platform crates

All eleven root platform crates inherit the personal, non-commercial license from the workspace
[`LICENSE`](../../LICENSE). The applications, kernels, and references keep their own component
licenses, as listed below and in the license inventory.

| Id | Path | Role | Status |
|---|---|---|---|
| `repo.cad-contract` | `crates/cad-contract` | shared contract | integrated, 55 tests |
| `repo.cad-kernel-truck` | `crates/cad-kernel-truck` | contract adapter over truck | verified, 26 tests + 10 corpus fixtures |
| `repo.cad-engineering` | `crates/cad-engineering` | control plane + CLI | verified, 30 tests |
| `repo.cad-cli` | `crates/cad-cli` | headless consumer, project workflow and STL output | verified, 10 tests |
| `repo.cad-agent-api` | `crates/cad-agent-api` | the versioned agent contract | verified, 13 tests |
| `repo.cad-agent-runtime` | `crates/cad-agent-runtime` | registry, lifecycle, grants, budgets, schema check | verified, 16 tests |
| `repo.cad-agent-geometry` | `crates/cad-agent-geometry` | the geometry-ops agent | verified, 24 tests |
| `repo.cad-document` | `crates/cad-document` | native parametric project payload and history | integrated, 27 tests |
| `repo.cad-io` | `crates/cad-io` | validated binary STL read/write shared by CLI and app | verified, 6 tests |
| `repo.cad-studio` | `crates/cad-studio` | local-first application shell, HTTP API and WebGL workspace | verified, 10 tests |
| `repo.cad-entitlements` | `crates/cad-entitlements` | local tier and quota policy; no billing provider | validated, 10 tests |

The platform crates are tracked in the consolidated workspace repository. The historical control
plane snapshot recorded content fingerprints under `task.versioncontrol.workspace`.

## External

| Id | Origin | Licence | Risk |
|---|---|---|---|
| `repo.craft-libs` | github.com/storytold/craft-libs (pinned revisions, vendored in CADCraft) | MIT OR Apache-2.0 | Approved; provenance and pinned source remain under supply-chain review |

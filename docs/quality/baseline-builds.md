# Baseline Builds

**Canonical data:** `engineering/quality/builds.json` (one record per command that actually ran).
**Commands and context:** [../build-and-test.md](../build-and-test.md).

Host: i3-1125G4, 4 cores / 8 threads, 7.4 GB RAM, rustc 1.94.0, no GPU claim.

## 1. Recorded builds

| Id | Command | Result | Wall | Peak RSS |
|---|---|---|---|---|
| `b-cadcraft-color` | `cargo build -p cadcraft-color` | success | 16.1 s | — |
| `b-cadcraft-geom` | `cargo build -p cadcraft-geom` | success | 4.5 s | — |
| `b-solidcraft-derive` | `CARGO_BUILD_JOBS=2 cargo build -p solidcraft-derive` | success | 24.9 s | — |
| `b-solidcraft-kernel` | `CARGO_BUILD_JOBS=2 cargo build -p solidcraft-kernel` | success | 8 m 22 s | 2.56 GB |
| `b-cad-contract` | `cargo test --release -p cad-contract` (cold) | success | 0.69 s | 136 MB |
| `b-cad-engineering` | `cargo test -p cad-engineering` (cold) | success | 10.27 s | 385 MB |
| `b-cad-engineering-build` | `cargo build -p cad-engineering` (cold) | success | 3.46 s | 386 MB |
| `b-cad-cli` | `cargo build -p cad-cli` (cold) | success | 0.68 s | 243 MB |
| `b-workspace` | `cargo build --workspace --all-targets -j 2` | success | 3.7 s | — |
| `b-cad-agent-geometry` | `cargo build -p cad-agent-geometry` | success | 0.5 s | — |
| `b-platform-workspace-20261025-final` | `cargo build --offline --workspace --all-targets -j 2` | success | 0.085 s (warm no-op) | 39 MB |

"Cold" means the package's own artifacts were removed with `cargo clean -p <package>`; its
dependencies stayed cached. No record claims a full dependency rebuild unless it says so.

The platform workspace is small enough to build whole: eleven crates, all targets. The final
2026-10-25 tree was already warm from the test gate and re-verified in 0.085 s; the older 3.7 s
seven-crate figure is historical. The *application* workspaces are not, and the rule in §2 still applies to them.

## 2. Not built, and why

| Target | Reason |
|---|---|
| Workspace-wide build of either application | one SolidCraft crate already needs 2.56 GB against 7.4 GB total |
| GUI binaries (`cadcraft`, `solidcraft`) | egui + wgpu link step on integrated graphics; not attempted |
| SolidCraft web build | no WASM target installed |
| Kernels `monstercraft`, `cascade-rs`, `hagane`, `cadrum`, `opencascade-rs` | cost/benefit on this host; reasons per kernel in `docs/kernel-comparison.md` §6 |

## 3. Rule

A build record is evidence of exactly one command. A successful compile is not evidence that the
application works, and a build record is never used here as a test record.

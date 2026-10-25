# Build spike, packaging blockers, and the next deployable gate

**Date:** 2026-10-25. **Machine:** 8 cores, 7.4 GiB RAM, 98 GB free disk, `rustc 1.94.0`,
Linux x86_64, all dependencies resolved **offline** from the local cargo registry cache.

This document records what was actually built and run here, what blocks a distributable
artifact, and the exact order of work to reach the first deployable build. Nothing below is
a claim about platforms that were not exercised.

---

## 1. Build spike — measured

| Build | Command | Result |
|---|---|---|
| SolidCraft CLI | `CARGO_BUILD_JOBS=1 cargo build --offline -p solidcraft-cli` | **Success — 37 m 26 s**, 0 errors, `dev` profile (`opt-level=2` + debuginfo) |
| SolidCraft desktop GUI | `CARGO_BUILD_JOBS=1 cargo +1.95.0 build --offline -p solidcraft` | **Success — 60 m 26 s**, 0 errors, 0 failures, after switching to the installed `1.95.0` toolchain (C2) |
| GUI smoke run | `timeout 8 solidcraft --sample` | Window came up, **Vulkan + MSAA 4x** initialised, stayed alive for the full 8 s (exit 124), exited cleanly with no leftover process |
| Target directory | — | **8.3 GB** after the CLI build |
| Produced binaries | `apps/solidcraft/target/debug/solidcraft-cli` (1.29 GB), `…/debug/solidcraft` (1.92 GB) | Both report version `0.18.3`; debug info included |
| GUI runtime | `solidcraft --version`, `--help` | `solidcraft 0.18.3`; usage: `solidcraft [--control PORT] [--sample] [--crash-log] [--safe-mode] [design.solidcraft | part.step]` |

### Constraints found (not assumptions)

- **C1 — memory forces `-j1`.** The host has 7.4 GiB total. A single `rustc` invocation peaks at
  ~1.5 GB (`solidcraft_doc`), and `cadmpeg-codec-freecad` reached 949 MB. Parallel jobs beyond
  1–2 risk the OOM killer. The 37-minute time is a `-j1` number on a constrained laptop, not a
  CI number.
- **C2 — the default toolchain is too old for the GUI.** `cargo build -p solidcraft` on the
  default `stable` (1.94.0) fails with *"rustc 1.94.0 is not supported by … egui 0.36.2,
  eframe, egui-wgpu, epaint … requires rustc 1.95"*. The `1.95.0` toolchain is installed, so the
  build proceeds with `cargo +1.95.0`. The toolchain must be **pinned** (e.g. `rust-toolchain.toml`)
  or release builds will be non-reproducible across machines.
- **C3 — release profile and cross-targets not yet exercised.** No `--release` build, no Windows
  or macOS target, and no cross-compilation has been run here. Time and disk for a release build
  are still unknown.

---

## 2. Reference part — verified end to end

Input: [`apps/solidcraft/examples/bracket.json`](../../apps/solidcraft/examples/bracket.json) —
a parameterised L-bracket: two sketches, an extrude, a fillet, a second sketch with two Ø8 holes,
and a cut extrude (12 commands, 6 features).

| Step | Command | Result |
|---|---|---|
| Build + export + save | `solidcraft-cli run bracket.json --out /tmp/bracket.step --save /tmp/bracket.solidcraft --timings` | RC 0; **272 ms** of engine time (240 ms of it the cut extrude); **955 ms** wall including process start |
| Measurements | `solidcraft-cli eval bracket.json` | volume **22 385.344 050 mm³**, area 9 215.314 mm², 11 faces, 25 edges, 18 vertices, mass 28.877 g, 6 features, **0 errors / 0 warnings** |
| Analytic cross-check | closed-form: L-profile 384 mm² × 60 mm − 2 × Ø8 × 6 mm − r2 fillet along 60 mm | **22 385.309 8 mm³** vs measured 22 385.344 1 → **Δ 0.034 mm³ = 0.000 153 %** |
| STEP export | `--out /tmp/bracket.step` | ISO-10303-21, **AP242** (`AP242_MANAGED_MODEL_BASED_3D_ENGINEERING_MIM_LF`), 17 475 bytes |
| STEP round trip | `solidcraft-cli eval --in /tmp/bracket.step` | RC 0 in **97 ms**; volume 22 385.350 3 mm³ → **Δ 0.006 mm³ (2.8 × 10⁻⁵ %)** vs native; 11 faces and 18 vertices preserved (25 → 27 edges from STEP topology splitting) |
| Native reopen | `solidcraft-cli eval --in /tmp/bracket.solidcraft` | **Bit-identical** to the original run (22 385.344 050 449 596 mm³, 11/25/18) — save/reopen equivalence holds |
| Headless render | `solidcraft-cli snapshot bracket.json --out /tmp/bracket.png` | 640 × 480 PNG, `renderer: gpu`, **423 ms** |

**What this proves:** a real B-rep kernel, parametric rebuild, fillets and boolean cut, analytic
accuracy to 1.5 × 10⁻⁶ relative, a working AP242 STEP round trip, a stable native format, and a
GPU-backed offscreen renderer — all on this 7.4 GiB machine in under a second per operation.

**What it does not prove:** assemblies, sheet metal, drawings, large-model performance, or
Windows/macOS packaging. The desktop GUI was smoke-tested on this machine only (window + Vulkan
init); it has no signed installer and has never been distributed.

---

## 3. Packaging and signing blockers (evidence in the tree)

| ID | Blocker | Evidence | Consequence |
|---|---|---|---|
| **P1** | **Nothing is code-signed for any OS** | `apps/solidcraft/.github/workflows/release.yml:7` — *"Nothing is signed yet."*; `packaging/macos/package.sh:78` uses ad-hoc `codesign --force --sign -`; `packaging/windows/package.ps1` contains **no** `signtool`/Authenticode step; Linux ships an unsigned tarball/deb | macOS Gatekeeper, Windows SmartScreen, and Linux repo acceptance all fail for a commercial product |
| **P2** | **Update trust is not enforced** | `crates/ui-egui/src/update/install.rs:34` — `pub const PUBLIC_KEY: Option<&str> = None;` while the release job signs `SHA256SUMS.txt` with minisign only when `UPDATE_SIGNING_KEY` exists | An updater that verifies nothing cannot be sold as "controlled updates"; we must generate **our own** keypair, set `PUBLIC_KEY`, and make the release job fail when the secret is missing |
| **P3** | **Release secrets and store identities belong to upstream** | Workflows reference `SNAPCRAFT_STORE_CREDENTIALS`, `UPDATE_SIGNING_KEY`, and the upstream repository; `winget.yml:50` publishes to `manifests/b/bherbruck/SolidCraft` | Our release pipeline cannot publish until we own a Snap account, Flathub app id, winget publisher id, Apple Developer ID, and a Windows signing certificate |
| **P4** | **The product root has no CI** | `<root>/.github/workflows/` does not exist; all pipelines live under `apps/solidcraft/.github/workflows/` | There is no build, test, or release automation for the product as a whole |
| **P5** | **Branding and identity are unresolved** | Binaries ship a `solvecraft-cli` alias and SolidCraft icons/names; "Atlas" trademark, domain, and package-name searches are not done | Cannot ship under a new name until search + rename are complete (see `COMMERCIAL-LICENSE.md`) |
| **P6** | **Build reproducibility** | `-j1`, 37 m 26 s, 8.3 GB target, toolchain mismatch (C2), no lockfile-pinned hermetic build, no SBOM | A signed release needs one pinned toolchain, one pinned lockfile, and a repeatable build log per artifact |

---

## 4. Next deployable gate — ordered

The first sellable deployment is, per the recorded owner decisions, a **signed desktop seat**.
Reaching it, in dependency order:

1. **D1 — Reproducible release build.** Pin `rust-toolchain.toml` (≥ 1.95), build
   `--release` for `solidcraft-cli` and `solidcraft` on Linux, record wall time, peak RSS, and
   artifact size. Exit: two binaries rebuilt from a clean `target/` with the same hash inputs.
   **Status: dev-profile builds of both binaries pass on this machine (37 m 26 s and 60 m 26 s,
   `-j1`) and the GUI runs; the `--release` build and the toolchain pin are still open.**
2. **D2 — Update trust.** Generate an Atlas minisign keypair, set `PUBLIC_KEY` in the updater,
   store the secret in CI only, and make the release job fail if it is absent. Exit: a package
   whose update path refuses an unsigned manifest (test: tampered `SHA256SUMS.txt` is rejected).
3. **D3 — Our own pipeline.** A root-level CI workflow that builds, tests, packages (nfpm
   `.deb`/`.rpm`, AppImage, macOS `.dmg` + Developer ID + notarization, Windows MSI + Authenticode)
   and attaches an SBOM and `NOTICE`. Exit: green run producing three OS artifacts.
4. **D4 — Clean-machine proof.** Install, launch, sign-in/off, update, and uninstall on a clean
   Windows, macOS, and Linux VM; capture screenshots and logs. Exit: no SmartScreen/Gatekeeper
   warnings; update applies and rolls back.
5. **D5 — Pilot artifact.** One signed desktop build delivered to a design partner under a pilot
   agreement, with crash reporting and a support contact. This is the first point at which any
   price in the recorded $2,400–$4,800 band can be quoted with evidence behind it.

Until D1–D4 exist, the only deployable thing in this repository is a **local, unsigned,
self-built binary** — which is what §2 exercised.

---

## 5. Reproducing §2

```bash
CARGO_BUILD_JOBS=1 cargo build --offline -p solidcraft-cli        # ~37 min at -j1
B=apps/solidcraft/target/debug/solidcraft-cli
$B run   apps/solidcraft/examples/bracket.json --out /tmp/bracket.step --save /tmp/bracket.solidcraft --timings
$B eval  apps/solidcraft/examples/bracket.json
$B eval  --in /tmp/bracket.step
$B eval  --in /tmp/bracket.solidcraft
$B snapshot apps/solidcraft/examples/bracket.json --out /tmp/bracket.png --width 640 --height 480
```

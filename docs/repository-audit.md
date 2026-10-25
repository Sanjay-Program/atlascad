# Repository Audit — Atlas CAD workspace

**Audit date:** 2026-10-21
**Workspace:** `/home/sanjay/Projects/zentron-cad`
**Method:** direct inspection of Git metadata, manifests, source trees, and build/test runs on this machine.
**Rule applied:** every claim below is either *measured here* (with the command shown) or marked
*read from the repository* (unverified by execution). Nothing is inferred from marketing copy.

---

## 0. Host environment (measured)

| Item | Value | How measured |
|---|---|---|
| OS | Fedora 43, kernel 7.1.8-100.fc43.x86_64 | `uname -a` |
| CPU | 11th Gen Intel Core i3-1125G4 @ 2.00 GHz, **4 cores / 8 threads** | `grep -m1 'model name' /proc/cpuinfo`, `nproc` |
| RAM | **7.4 GiB total**; ~1.3 GiB available, swap 7.4 GiB **~97 % consumed** at audit time | `/proc/meminfo` |
| GPU | Intel Tiger Lake-LP **UHD Graphics G4** (integrated), `/dev/dri/card1`, `renderD128` | `lspci`, `ls /dev/dri` |
| Disk | `/home` 475 GB, **58 GB free (88 % used)** | `df -h ~` |
| Rust | rustc / cargo **1.94.0**, stable, edition 2024 capable | `rustc --version` |
| Installed targets | `x86_64-unknown-linux-gnu`, `aarch64-linux-android`, `x86_64-pc-windows-gnu` | `rustup target list --installed` |
| **Missing** WASM target | `wasm32-unknown-unknown` **not installed** | same |
| Missing tooling | `cargo-audit`, `cargo-deny`, `cargo-license`, `cargo-nextest`, `wasm-pack`, `wasm-bindgen` | `command -v` |
| Network | crates.io reachable (via Cargo index), github.com reachable | `cargo fetch`, `git ls-remote` |

**Consequences for this project (not speculation — these shaped every decision below):**

1. The machine is **memory-bound, not CPU-bound**. A single `cargo build -p solidcraft-kernel`
   peaked at **2.56 GB RSS** and did 257 175 major page faults against a full swap file. Builds
   must be done **one crate at a time with `CARGO_BUILD_JOBS` reduced**, never workspace-wide.
2. The GPU is **integrated UHD**, so any rendering figure measured here is a *low-end floor*, not
   a representative desktop GPU result. No rendering performance claim may be generalised.
3. No WASM target and no `wasm-pack` are installed, so **no WASM build has been performed**. This
   audit does not claim WASM viability; it records what the repositories declare and what is
   missing to test it.
4. No `cargo-audit`/`cargo-deny` installed: advisory scanning is **not yet run** (see
   `docs/security-review.md`).

---

## 1. Repository inventory (measured)

All eleven checkouts are **clean** (`git status --porcelain` returned 0 paths for every repo), so
no uncommitted user work existed at audit time and nothing was at risk of being overwritten.
**No reset, clean, checkout, or stash was performed on any repository.**

| Path | Remote | Branch | HEAD | Working tree |
|---|---|---|---|---|
| `apps/cadcraft` | github.com/storytold/cadcraft | `main` | `0cf3bdea83ebbe02fb2e674b23f07de4acfa73da` | clean |
| `apps/solidcraft` | github.com/tangency-labs/solidcraft | `main` | `775e5155bbff9a01acbd6da09da2dfe87be708dc` | clean |
| `kernels/truck` | github.com/ricosjp/truck | `master` | `78d699b0792b39e27c2bf073b682986701644e91` | clean |
| `kernels/monstertruck` | github.com/virtualritz/monstertruck | `master` | `d2ad1ed7aa420b7f32cd2c1e3ad319b46d593652` | clean |
| `kernels/cascade-rs` | github.com/DonSqualo/cascade-rs | `master` | `c68c1d2c3c7ff24d9f0d21b926748273bb856842` | clean |
| `kernels/hagane` | github.com/rsasaki0109/Hagane | `main` | `45e423c9c5f1eea33a5ca23e44abde2d63cdd95d` | clean |
| `kernels/cadrum` | github.com/lzpel/cadrum | `main` | `c59916dbbf135c21c64d80f378b0674432df50bb` | clean |
| `kernels/opencascade-rs` | github.com/bschwind/opencascade-rs | `main` | `32758df23a137f4e7c786c618b0317fc3badb3b2` | clean |
| `references/cadrs` | github.com/gswdh/cadrs | `main` | `74dd5771cfc3f3c2c75958d273aa032d92d1e1f2` | clean |
| `references/cadrust` | github.com/mac999/cadrust | `main` | `519cb390fd1c2e3079dd520ac4899474565f6b98` | clean |
| `references/bevy_pmetra` | github.com/nilaysavant/bevy_pmetra | `master` | `6933b23d75bd3168ab398bb288a40a6cd3f2c516` | clean |
| `references/opencadstudio` | — (no Git) | — | — | **empty directory; source arrived as `OpenCADStudio-main.zip`, GPL-3.0 verified** (§1.1) |

### 1.1 `references/opencadstudio` — resolved during the audit

At the start of the audit the directory was present but **empty** (0 files, no `.git`), so its
licence could not be assessed. Later in the same session the source arrived as
**`OpenCADStudio-main.zip` (39 MB)** at the workspace root, which allowed verification:

| Check | Result |
|---|---|
| Archive contents | **1 797 files**, single top-level `OpenCADStudio-main/` |
| Language | **892 `.rs` files** (plus 396 `.svg`, 77 `.md`, 25 `.wgsl`, 16 `.py`) |
| `LICENSE` | **GNU GPL v3** (text verified: "GNU GENERAL PUBLIC LICENSE / Version 3, 29 June 2007") |
| Manifest | `name = "OpenCADStudio"`, `version = "2026.42.0"`; no `license` field in the root manifest |

**Conclusion: the brief's GPL-3.0 report is correct.** The archive stays **unpacked and outside the
reference tree**; no code from it has been copied anywhere in this workspace, so there is no licence
contamination. It may be read for architectural ideas only (see
`docs/architecture-decisions/0003-no-lgpl-in-shipping-binary.md`); no source may enter the product
unless the product itself is relicensed to GPL-3.0.
- Star counts, line counts and feature claims in the brief were **not assumed**; everything below
  is counted from the checkouts.

---

## 2. Language composition (measured: file counts, excluding `.git`)

| Repository | files | `.rs` | `.py` | `.ts/.js` | `.c/.cpp/.h` | size |
|---|---:|---:|---:|---:|---:|---:|
| apps/cadcraft | 750 | **618** | 1 | 0 | 0 | 21 MB |
| apps/solidcraft | 2249 | **1154** | 42 | 10 | 0 | 244 MB |
| kernels/truck | 339 | 274 | 0 | 3 | 0 | 50 MB |
| kernels/monstertruck | 538 | 456 | 0 | 3 | 0 | 54 MB |
| kernels/cascade-rs | 129 | 103 | 3 | 0 | 0 | 13 MB |
| kernels/hagane | 870 | 479 | 0 | 38 | 0 | 100 MB |
| kernels/cadrum | 135 | 53 | 3 | 0 | **5** | 5.1 MB |
| kernels/opencascade-rs | 192 | 117 | 0 | 0 | 0 | 4.9 MB |
| references/cadrs | 1779 | 719 | 7 | 2 | 0 | 54 MB |
| references/cadrust | 68 | 15 | 15 | 4 | 0 | 45 MB |
| references/bevy_pmetra | 107 | 86 | 0 | 1 | 0 | 8.0 MB |

**Finding:** the platform is **already Rust-first**. There is no non-Rust application to migrate —
which means "Phase 2 migration" (brief §6) is largely a **non-problem for applications**. The only
non-Rust surface is (a) Python helper scripts inside solidcraft (42 files, tooling/oracle scripts)
and (b) C/C++ inside `cadrum` (5 files, OCCT glue). Phase 2 therefore reduces to: *classify existing
Rust, do not rewrite kernels for a language metric.*

`cadrum`'s C/C++ is worth noting because it contradicts a purely "pure Rust" reading: the crate is
MIT-licensed and advertises static linking of **headless OpenCASCADE**, i.e. it embeds a C++
kernel. See `docs/license-inventory.md`.

---

## 3. CADCraft — architecture assessment

**Identity:** 2D drafting/CAD application (DXF/DWG), pure Rust, `MIT OR Apache-2.0`, version
`0.10.0`, `rust-version = "1.90"`, edition 2024, workspace with 12 library crates + apps + `xtask`.

**Measured crate sizes (Rust LOC by `cat | wc -l`):**

| Crate | files | LOC | Role |
|---|---:|---:|---|
| `engine` | 304 | 97 341 | command engine, tools, interaction |
| `ui-egui` | 93 | 29 979 | egui front end |
| `io` | 79 | 23 106 | file formats, plotting, export |
| `doc` | 28 | 10 689 | drawing database |
| `render` | 37 | 9 514 | 2D rendering |
| `geom` | 13 | 5 031 | 2D/3D math, curves, NURBS |
| `fonts` | 11 | 4 754 | font handling |
| `constraints` | 10 | 4 283 | constraint solver |
| `dxf` | 4 | 1 171 | DXF reader/writer |
| `mcp` | 8 | 1 075 | MCP server |
| `dwg` | 1 | 791 | DWG via `acadrust` |
| `color` | 1 | 276 | ACI / true colour |

**Document model (read from source):**
- `crates/doc/src/entity.rs`: `pub struct Handle(pub u64)` — a plain integer entity handle with
  `hex()`/`parse_hex()`. **This is already a stable, serialisable, application-level ID** (not a
  pointer), which is exactly what brief §12 asks for.
- `crates/doc/src/store.rs`: `EntityStore` is **copy-on-write**: entities live in
  `Arc<Vec<Arc<Entity>>>` chunks (chunk max 1024) with 64 sharded `HashMap<Handle,u32>` indexes.
  Cloning a store — used as an undo snapshot — copies only chunk/shard pointers. This is a genuinely
  good design and **already satisfies "immutable snapshots for undo"** (brief §9).
- Units: `INSUNITS` header variable (`crates/doc/src/header.rs`), i.e. DXF-native unit handling
  rather than a typed unit system.
- Non-modelled file data is deliberately preserved: `pub mod dynblock; pub mod foreign;` with the
  comment *"File data CADCraft doesn't model, kept for saving (#528)"* — partial compliance with the
  brief's "do not silently discard unsupported entities" requirement, though the README admits
  "Entities CADCraft doesn't model yet are not kept on save", so this is **partial**.

**Geometry architecture:** 2D-first with 3D math support. `cadcraft-geom` provides `Vec2`, `Vec3`,
`Mat3`, `Mat4`, `Line`, `Arc`, `Circle`, `Ellipse`, `Polyline` (bulges), `Spline` (NURBS),
`intersect_*`, `tangent_circles`, `Bounds2`. Tolerance is a single global `EPS: f64 = 1e-9` plus
angle helpers. **No B-rep, no solids, no 3D kernel dependency at all** — CADCraft is not a 3D modeler.

**Spatial indexing:** `rstar v0.12.2` is a direct dependency of `cadcraft-engine` (from
`cargo tree`), so an R-tree is already used for selection/snapping. This means brief §9's
"improve spatial indexing" is **partly pre-existing**, and any optimisation must be benchmarked
against a real R-tree baseline rather than assumed absent.

**Engineering hygiene (read from manifests):**
- `#![forbid(unsafe_code)]` in `geom` and `doc`; workspace lint `unsafe_code = "forbid"`;
  `#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic, ...)]` per crate.
- `clippy.toml`, `rustfmt.toml`, `ATTRIBUTION.md`, `NOTICE`, `LICENSE-MIT`, `LICENSE-APACHE`,
  `AGENTS.md`, `CLAUDE.md`, `docs/`, `xtask`.
- **2 212** `#[test]` attributes, **0** `#[ignore]`.
- CI: `freebsd.yml`, `nix.yml`, `packaging-lint.yml`, `release.yml`, `windows-arm64.yml`.
- Git dependencies on `github.com/storytold/craft-libs` pinned by `rev`
  (`craft-docpath`, `craft-ui`) — an external, non-crates.io source of truth for the UI layer.

**Build result (measured):**
- `cargo fetch` **succeeded** after network access (git deps were absent from `~/.cargo/git`).
- `cargo build -p cadcraft-color` → **OK** (16.1 s).
- `cargo build -p cadcraft-geom` → **OK** (4.5 s).
- `cargo test -p cadcraft-geom` → **84 passed; 0 failed** in 0.04 s.
- The full GUI app and `engine` crate were **not built** (see §8 — memory bound, deferred).

---

## 4. SolidCraft — architecture assessment

**Identity:** parametric 3D CAD application, pure Rust, `MIT OR Apache-2.0`, version `0.18.3`,
declares `rust-version = "1.95"`, edition 2024, 13 library crates + 3 apps + `xtask` + `vendor/`.

**Measured crate sizes:**

| Crate | files | LOC | Role |
|---|---:|---:|---|
| `ui-egui` | 279 | 129 669 | egui front end |
| `engine` | 267 | 103 127 | command engine |
| `kernel` | 186 | **94 803** | **B-rep kernel boundary (truck-backed)** |
| `doc` | 105 | 49 089 | parametric document/timeline |
| `sketch` | 47 | 23 681 | sketches + constraint solver |
| `geom` | 34 | 14 983 | geometry |
| `io` | 23 | 11 541 | STEP/IGES/3MF/STL/OBJ/glTF |
| `render` | 38 | 7 894 | rendering |
| `gpu` | 12 | 4 722 | GPU layer |
| `mcp` | 5 | 3 031 | MCP server |
| `space-mouse` | 3 | 2 192 | 3D mouse |
| `threedx-client` | 3 | 329 | 3Dconnexion client |
| `derive` | 1 | 282 | proc macros |

**Geometry-kernel dependency (the single most important fact in this audit):**

`apps/solidcraft/Cargo.toml` pins `truck-modeling 0.6`, `truck-shapeops 0.4`,
`truck-meshalgo 0.4`, `truck-stepio 0.3`, `truck-topology 0.6`, plus `spade 2.15`, with the comment
*"geometry kernel (Apache-2.0; see plan/adr/0001-geometry-kernel.md). Only crates/kernel uses it."*

`crates/kernel/src/lib.rs` states the architectural contract explicitly:

> "Everything SolidCraft does with solids goes through this crate … The implementation uses the
> `truck` crates … but **no truck type appears in the public API**, so the kernel can be replaced
> without touching the rest of the workspace. Every call into the third-party kernel runs under
> [`guard`]: a panic inside it becomes a `KernelError::Internal` instead of taking the application
> down."

**This means the brief's Phase 4 (`cad-kernel-api` + `cad-kernel-truck` adapter split) is already
implemented in production form by SolidCraft's `crates/kernel`.** Any plan that proposes inventing
a new kernel abstraction from scratch would duplicate 94 803 LOC of hard-won work. The correct move
is to *extract a small, stable contract* from what already exists and let both apps share it — which
is what this milestone does (see `docs/architecture.md`).

Verified error taxonomy in `crates/kernel/src/lib.rs` (`KernelError`): `Invalid`, `Failed`,
`TooLarge`, and `Internal` (third-party panic caught, model unchanged) — a ready-made
`Result`/capability/error vocabulary to build a shared contract on.

**Document model (read from source):**
- `crates/doc/src/lib.rs`: a parametric timeline ("user and model parameters, and a parametric
  timeline of features … Evaluation is incremental: a feature is recomputed only when it, a
  parameter it uses, or an earlier feature changed"). This is the brief's §9 "incremental
  recomputation / dependency graph" requirement, **already implemented**.
- `feature_refs.rs`, `face_anchor.rs` (`FaceAnchor`, `FaceAttachmentStatus`), `lineage.rs`,
  `remap_topology` — explicit handling of *stable references surviving recomputation*, which is
  exactly the hard part of brief §12. It exists; it must not be re-invented.
- Typed IDs exist (`names.rs: pub struct SketchId(pub u64)`); typing is **not uniform across all
  entity kinds**, which is a refactoring opportunity, not a correctness bug.
- `display_units.rs`, `expr.rs` (unit-aware expressions), `config.rs` (configurations).

**Engineering hygiene (read from manifests/tree):**
- `#![forbid(unsafe_code)]` + the same `clippy::unwrap_used/expect_used/panic` denies in `doc` and
  `kernel`.
- `deny.toml` (cargo-deny config), `THIRD-PARTY-LICENSES.txt`, `SECURITY.md`, `CONTRIBUTING.md`,
  `ATTRIBUTION.md`, `NOTICE`, `skills/`, `tools/`, `vendor/` (with its own README), `plan/` referenced.
- **5 884** `#[test]`; **57** `#[ignore]` (ignored tests exist and were **not** touched).
- CI is extensive: `ci.yml`, `freecad-oracle.yml`, `generated.yml`, `kernel-paused.yml`,
  `lists.yml`, `pages.yml`, `release.yml`, `snap-store.yml`, `winget.yml`, …
- README claims an **oracle**: "75 of 76 public reference parts match their analytic or
  FreeCAD-computed volume, area, centre of mass and bounding box … the other is a tracked bug."
  *Read from repository; not re-run here.* This is the strongest correctness signal in the whole
  workspace and is directly relevant to kernel selection (§6).

**Build results (measured):**
- `cargo build -p solidcraft-derive` → **OK** (24.9 s).
- `cargo build -p solidcraft-kernel` → **OK, exit 0**, 8 min 22 s wall, **peak RSS 2 555 608 kB
  (2.56 GB)**, 257 175 major page faults, 28.6 M file-system input blocks. Confirms truck +
  spade + the kernel layer compile on rustc 1.94.0 despite the declared MSRV of 1.95.
- **MSRV note:** `rust-version = "1.95"` is declared but **not enforced as a hard failure** by cargo
  1.94.0 for these crates (resolver v3 treats MSRV as resolution metadata). This is a **latent
  reproducibility risk**, not an error today: a future Cargo could hard-fail, and CI presumably uses
  a newer toolchain. Recommend pinning via `rust-toolchain.toml` (**neither app has one** — measured:
  no `rust-toolchain`/`rust-toolchain.toml` in either repo).

---

## 5. Kernel repositories — factual state

| Kernel | Language | License (verified) | Native dep | Measured here |
|---|---|---|---|---|
| `truck` | pure Rust | **Apache-2.0** (`LICENSE` = Apache 2.0 text) | none for modeling; `truck-js`/`truck-platform`/`truck-rendimpl` use `wasm-bindgen` | **Compiles & works** — proven transitively by `solidcraft-kernel` build |
| `monstertruck` | pure Rust | **Apache-2.0** | none (wasm-bindgen in workspace) | not built — see §6 |
| `cascade-rs` | pure Rust | **Apache-2.0** (`Cargo.toml`) | none (`opencascade-sys` commented out) | not built |
| `hagane` | pure Rust | **MIT OR Apache-2.0** | none (no `build.rs`) | not built |
| `cadrum` | Rust + **C/C++** | wrapper **MIT**, but statically links **headless OpenCASCADE (LGPL-2.1)** | `build.rs`, OCCT via prebuilt tarball or `cmake` (`source` feature) | not built — native OCCT + disk/RAM cost |
| `opencascade-rs` | Rust bindings | **LGPL-2.1** (`crates/opencascade-sys`, `occt-sys`, `opencascade` manifests) | `build.rs` + cmake → OCCT | not built — LGPL decision needed first |

`truck` declares no `license` in its root manifest because it uses a virtual workspace; the
`LICENSE` file is the authoritative Apache-2.0 text, and crates.io metadata reports Apache-2.0
(consistent with SolidCraft's comment).

---

## 6. Dependency graph and duplicate/conflicting versions

**CADCraft (`cargo tree -p cadcraft-engine --depth 1`):** internal crates `color`, `constraints`,
`doc`, `dxf`, `fonts`, `geom`, `render`; external `craft-docpath` (git), `log 0.4.34`,
`rstar 0.12.2`, `serde 1.0.229`, `serde_json 1.0.151`, `thiserror 2.0.21`; dev-only `cadcraft-io`.

**SolidCraft (`-p solidcraft-kernel`):** could not be listed with `--offline`
(`failed to download proptest v1.12.0`) — it needs its dev-dependency tree online. The *build*
succeeded online, so this is only an inspection limitation.

**Cross-project duplication (the real integration risk):**

| Concern | CADCraft | SolidCraft | Conflict |
|---|---|---|---|
| Version | 0.10.0 | 0.18.3 | independent |
| Edition | 2024 | 2024 | compatible |
| Rust MSRV | 1.90 | **1.95** | **highest wins (1.95)** |
| egui / eframe | `0.36` | (own `ui-egui`, egui-based) | must be aligned if ever in one process |
| serde | 1 | 1 | compatible |
| thiserror | 2 | 2 | compatible |
| Geometry types | `cadcraft-geom::{Vec2,Vec3}` | `solidcraft-geom` | **two separate 2D/3D vector types** |
| Kernel | none (2D only) | `truck` 0.6 through `crates/kernel` | only SolidCraft has a kernel |
| External git dep | `storytold/craft-libs` (`craft-ui`, `craft-docpath`) | none | CADCraft-only supply-chain risk |

There is **no crate-name collision** between the two workspaces today (different prefixes), so a
single Cargo workspace is *not* blocked by naming — it is blocked by (a) MSRV 1.95, (b) two
independent implementations of core concepts (`Vec2/Vec3`, document, undo), and (c) both apps
having their own `ui-egui`/`engine`/`doc` crates with the same *role* but incompatible APIs.

**Monstertruck vs Truck:** monstertruck is a **fork/rename of truck** (its README expands to
"Multifarious Omnificence, Nomenclature Standardized, Terminology Enhanced & Refactored Truck").
Two 456/274-file Apache-2.0 copies of the same kernel in one product is pure duplication with
divergence risk. **Recommendation: use upstream `truck` (already integrated, already proven by the
SolidCraft oracle) and keep `monstertruck` as a read-only reference.** No build was attempted, which
is the correct spend of an 8 GB machine.

---

## 7. What compiles / what was tested (measured — exact commands)

| # | Command | Result |
|---|---|---|
| 1 | `cargo fetch` in `apps/cadcraft` | **OK** (after allowing network; git deps `craft-libs` fetched) |
| 2 | `cargo build -p cadcraft-color` (cadcraft) | **OK** 16.14 s |
| 3 | `cargo build -p cadcraft-geom` (cadcraft) | **OK** 4.49 s |
| 4 | `cargo test -p cadcraft-geom` (cadcraft) | **84 passed, 0 failed**, 0.04 s |
| 5 | `cargo build -p solidcraft-derive` (solidcraft) | **OK** 24.91 s |
| 6 | `cargo build -p solidcraft-kernel` (solidcraft) | **OK**, 8 m 22 s, peak RSS 2.56 GB |

**Not attempted, with reasons:**

| Task | Reason |
|---|---|
| Workspace-wide `cargo build` of either app | Would exceed RAM (2.56 GB for *one* crate against ~1.3 GB available); brief §4 explicitly forbids it |
| `cargo test` on `solidcraft-kernel` | 5 884 tests + 186-file crate; needs a sustained build well beyond the memory/time budget *in this session*, and truck tests are heavy. Deferred with a concrete command recorded in `docs/build-and-test.md` |
| Building GUI apps (`cadcraft`, `solidcraft` binaries) | egui + wgpu + NAGA on UHD G4, largest link step, most memory. Deferred |
| WASM builds | `wasm32-unknown-unknown` target and `wasm-pack` not installed |
| `cargo audit` / `cargo deny` / license scan | Tools not installed (no network install performed) |
| Kernels `monstertruck`, `cascade-rs`, `hagane`, `cadrum`, `opencascade-rs` | See `docs/kernel-comparison.md` — the comparison did **not** require building all of them, and building them would have cost hours on this host |

---

## 8. Risk register (audit-level)

| # | Risk | Severity | Evidence | Mitigation |
|---|---|---|---|---|
| R1 | `references/opencadstudio` is **GPL-3.0** and easily mistaken for a normal reference | **Medium (legal)** | `LICENSE` = GPLv3; 892 `.rs` files | Keep it packed and outside the build; read for ideas only. Any copy into the product forces GPL-3.0 on the whole distribution. |
| R2 | `cadrum` is MIT-labelled but statically links LGPL-2.1 OCCT | **High (legal)** | `cadrum/Cargo.toml` features comment + README "statically-linked, headless OpenCASCADE" | Do **not** distribute statically-linked build; legal review before any use |
| R3 | `opencascade-rs` is **LGPL-2.1** | **High (legal)** | crate manifests | Keep out of a proprietary binary, or dynamic-link + relink rights; decision required |
| R4 | Two divergent geometry/document stacks in one product | High (engineering) | §3 vs §4 | Shared *contract* crate, not a merged implementation (see `docs/architecture.md`) |
| R5 | No `rust-toolchain.toml` in either app; MSRV 1.95 vs local 1.94 | Medium | measured | Pin toolchains per repo before CI claims reproducibility |
| R6 | CADCraft depends on external git `craft-libs` pinned by rev | Medium (supply chain) | `Cargo.toml` | Vendor/lock; treat as third-party for licensing |
| R7 | GPU is integrated UHD G4; rendering numbers won't transfer | Medium | `lspci` | Label all render figures as low-end floor |
| R8 | 58 GB free disk, swap 97 % full | Medium | `df`, `/proc/meminfo` | Incremental, single-crate builds; keep `target/` pruned |
| R9 | No advisory/licence tooling installed | Medium | `command -v` | Install `cargo-audit`/`cargo-deny` or run in CI |
| R10 | SolidCraft `vendor/` + 42 Python scripts are an unaudited third-party surface | Low-Med | tree | Included in `docs/third-party-dependencies.md` scope |

---

## 9. Audit conclusions (facts → decisions)

1. **Both apps are real, mature, Rust-first CAD applications**, not scaffolds. CADCraft =
   professional 2D drafting (DXF/DWG, 2 212 tests); SolidCraft = parametric 3D with a truck-backed
   B-rep kernel (5 884 tests, an oracle over 76 reference parts).
2. **The "choose a kernel" work is already done in practice**: SolidCraft has shipped truck behind a
   clean adapter for 186 files / 94 803 LOC. The decision is to **keep truck as the primary kernel**
   and make that decision explicit and measured (`docs/kernel-comparison.md`).
3. **The licensing question is concrete, not theoretical**: Apache-2.0 (truck) is safe for a closed
   product; LGPL-2.1 (OpenCASCADE via `opencascade-rs`, and the OCCT inside `cadrum`) is not, without
   deliberate compliance work. This is the one area that can block a paid distribution.
4. **The smallest verified integration milestone is not a rewrite.** It is a *contract crate*
   that both apps can adopt incrementally — units, stable IDs, capability discovery, kernel error
   taxonomy, and a transactional command envelope — proven against the real truck kernel.
   That work is implemented and tested in this milestone; see `docs/architecture.md` and
   `crates/cad-contract`.

**Next recommended task:** wire `cad-contract`'s `KernelAdapter` into `solidcraft/crates/kernel`
behind a feature flag (no behaviour change), then run the full `solidcraft-kernel` test suite to
prove the contract describes the real kernel. Details in `docs/roadmap.md`.

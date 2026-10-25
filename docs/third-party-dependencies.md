# Third-Party Dependencies

**Audit date:** 2026-10-23
**Method:** `cargo metadata --format-version 1` on each application (resolved graph, all targets),
then cross-checked against workspace manifests and `Cargo.lock`. Licence strings below are the
**declared SPDX expressions from the resolved packages**, not vendor marketing.

> Companion document: `docs/license-inventory.md` (obligations). This file covers *what is used, from
> where, at what version, and whether it is runtime or build-time*.

---

## 1. Scale of the dependency surface

| Application | Resolved packages | Duplicate crate names | External (non-workspace) |
|---|---:|---:|---:|
| CADCraft | **485** | 40 | ~470 |
| SolidCraft | **600** | 57 | ~584 |

Both numbers include cross-platform dependencies (Windows/Android/macOS crates are resolved even when
building on Linux), so they overstate the Linux runtime footprint. They are still the correct figure for
licence and supply-chain review.

---

## 2. Core runtime dependencies, by role

### 2.1 CADCraft (2D drafting)

| Crate | Version | Licence | Role | Runtime? |
|---|---|---|---|---|
| `egui` | 0.36.2 | MIT OR Apache-2.0 | immediate-mode UI | runtime |
| `eframe` | 0.36.2 | MIT OR Apache-2.0 | app shell / windowing | runtime |
| `egui-wgpu` | 0.36.2 | MIT OR Apache-2.0 | GPU backend for egui | runtime |
| `wgpu` | 30.0.1 | MIT OR Apache-2.0 | cross-platform GPU API | runtime |
| `naga` | 30.0.1 | MIT OR Apache-2.0 | shader translation (wgpu dep) | runtime |
| `winit` | 30.0.13 | Apache-2.0 | window/event loop | runtime |
| `tiny-skia` | 0.11.4 | **BSD-3-Clause** | CPU rasteriser | runtime |
| `lyon_tessellation` | 1.0.22 | MIT OR Apache-2.0 | 2D path tessellation | runtime |
| `skrifa` | **0.44.0 and 0.47.0** | MIT OR Apache-2.0 | font parsing — **two versions resolved** | runtime |
| `harfrust` | 0.12 | (workspace manifest) | text shaping | runtime |
| `image` | 0.25.10 | MIT OR Apache-2.0 | PNG/JPEG | runtime |
| `quick-xml` | 0.41.0 | MIT | SVG reader | runtime |
| `acadrust` | 0.6.4 | **MPL-2.0** | DWG ⇄ DXF bridge | runtime (DWG feature) |
| `rstar` | 0.12.2 | MIT OR Apache-2.0 | **R-tree** for selection/snapping | runtime |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 | data parallelism | runtime |
| `serde` | 1.0.229 | MIT OR Apache-2.0 | (de)serialisation | runtime |
| `thiserror` | **1.0.69 and 2.0.21** | MIT OR Apache-2.0 | error traits — **two majors resolved** | runtime |
| `tokio` | 1.53.2 | MIT | used by the MCP/JSON control channel | runtime |
| `craft-ui`, `craft-docpath` | `0.1.0` @ git `storytold/craft-libs` rev `5c874d51…` / `82a24b00…` | **MIT OR Apache-2.0**, read from pinned checkout manifests and license files | UI widgets; allowed-path policy for xrefs/data links | runtime |
| `egui_kittest` | 0.36 | MIT OR Apache-2.0 | UI snapshot testing | **dev/test only** |

### 2.2 SolidCraft (parametric 3D)

| Crate | Version | Licence | Role | Runtime? |
|---|---|---|---|---|
| `truck-modeling` | 0.6.0 | **Apache-2.0** | B-rep geometry/topology construction | runtime |
| `truck-topology` | 0.6.0 | Apache-2.0 | B-rep topology | runtime |
| `truck-shapeops` | 0.4.0 | Apache-2.0 | booleans — **vendored + patched** | runtime |
| `truck-meshalgo` | 0.4.0 | Apache-2.0 | tessellation — **vendored + patched** | runtime |
| `truck-stepio` | 0.3.0 | Apache-2.0 | STEP I/O — **vendored + patched** | runtime |
| `truck-geometry` | 0.4.0 (vendored) | Apache-2.0 | geometry primitives — **vendored** | runtime |
| `spade` | 2.15.1 | MIT OR Apache-2.0 | constrained Delaunay + Ruppert refinement | runtime |
| `egui` / `eframe` / `egui-wgpu` | 0.36.2 | MIT OR Apache-2.0 | UI | runtime |
| `wgpu` / `naga` | 30.0.1 | MIT OR Apache-2.0 | GPU | runtime |
| `winit` | 30.0.13 | Apache-2.0 | windowing | runtime |
| `tiny-skia` | **0.12.0** | BSD-3-Clause | CPU raster | runtime |
| `image` | 0.25.10 | MIT OR Apache-2.0 | images | runtime |
| `gltf` | 1.4.1 | MIT OR Apache-2.0 | glTF 2.0 import | runtime |
| `zip` | **8.6.0 and 9.0.4** | MIT | 3MF packages — **two versions resolved** | runtime |
| `quick-xml` | 0.41.0 | MIT | 3MF/XML | runtime |
| `chardetng` + `encoding_rs` | 0.1.17 / 0.8 | Apache/MIT | STEP legacy encodings | runtime |
| `schemars` | 1.2.3 | MIT | command parameter/result JSON schemas | runtime |
| `rayon` | 1.12.0 | MIT OR Apache-2.0 | parallelism | runtime |
| `serde` / `thiserror` | 1.0.229 / **1.0.69 + 2.0.21** | MIT OR Apache-2.0 | — | runtime |
| `tokio` | 1.53.2 | MIT | MCP server | runtime (MCP) |
| `sha2` | 0.10 | MIT OR Apache-2.0 | hashing (documents/configs) | runtime |
| `proptest` | 1.12.0 | MIT OR Apache-2.0 | **property tests** | **dev/test only** |

### 2.3 Vendored, patched crates (`apps/solidcraft/vendor/`)

Wired with `[patch.crates-io]`; 7 entries: `truck-shapeops`, `truck-meshalgo`, `truck-stepio`,
`truck-geometry`, `egui-wgpu`, `matext4cgmath`, `README.md`.

These are **not stock upstream**. `vendor/README.md` documents real upstream defects the patches work
around, including:

- shapeops: reject empty/single-point polylines and non-finite segments before endpoint indexing
  (i.e. **upstream could panic or corrupt topology on degenerate intersections**);
- shapeops: mesh both shells with `robust_triangulation` because faces from earlier booleans have
  boundaries that lie on their surface only within tolerance;
- shapeops: fall back to nearest parameter when exact projection fails;
- shapeops: **result shells/faces keep the source shell's order** — upstream "followed hash order of
  face addresses, so a boolean's faces came out in a different order on every run"
  (i.e. **upstream booleans are non-deterministic**);
- geometry/stepio: further tolerance and ordering fixes.

**Consequence for the platform:** "we use truck 0.6" means *"we use truck 0.6 plus four locally patched
crates carrying correctness and determinism fixes."* This is the strongest single argument for keeping
SolidCraft's existing kernel adapter rather than starting a fresh truck integration, and it must be
tracked as a maintenance obligation (upstream these fixes, or carry the fork deliberately).

---

## 3. Duplicate / conflicting versions (measured)

Duplicates that matter for a unified platform:

| Crate | CADCraft | SolidCraft | Note |
|---|---|---|---|
| `thiserror` | 1.0.69 **and** 2.0.21 | 1.0.69 **and** 2.0.21 | Two majors in both trees; `2` is the workspace choice, `1` arrives transitively. Harmless but increases binary size and build time. |
| `tiny-skia` | 0.11.4 | **0.12.0** | **Divergent rasteriser versions** — a shared render crate must pick one. |
| `skrifa` | 0.44.0 **and** 0.47.0 | 0.44.0 (+0.47 via manifest) | Font stack duplication. |
| `zip` | — | 8.6.0 **and** 9.0.4 | 3MF path. |
| `egui`/`eframe`/`wgpu` | 0.36.2 / 30.0.1 | 0.36.2 / 30.0.1 | **Aligned** — this is the good news for any future combined UI. |
| `getrandom` | 0.3.4, 0.4.3 | **0.2.17**, 0.3.4, 0.4.3 | WASM randomness sensitivity. |
| `bitflags`, `hashbrown`, `phf`, `nix`, `itertools`, `digest` | 2 versions each | 2–3 versions each | Routine ecosystem churn. |

**Total duplicate crate names:** 40 (CADCraft), 57 (SolidCraft). None is a *conflict* (Cargo permits
coexistence), but each pair is wasted compile time on a machine where a single crate build takes 8 minutes.

---

## 4. Dependencies deliberately **not** used (and why)

| Candidate | Status | Reason |
|---|---|---|
| `opencascade-rs` | **excluded** | LGPL-2.1 → licensing decision required, not present in either graph |
| `cadrum` | **excluded** | MIT wrapper over statically linked LGPL OCCT; native build via cmake/prebuilt tarball |
| `monstertruck` | **excluded** | A fork of truck; duplicate 456-file kernel with divergence risk; truck already integrated |
| `cascade-rs` | **not integrated** | Pure Rust, Apache-2.0, pre-1.0 parity effort; no measured advantage over the already-integrated truck |
| `hagane` | **not integrated** | Pure Rust, MIT/Apache; early ("an early, working foundation"); no measured advantage |
| `yrs` / Yjs | **not added** | Collaboration is Phase 13; deliberately deferred until single-user persistence is stable (brief §17). Not in either resolved graph. |
| `axum` / `sqlx` / Postgres | **not added** | SaaS backend is optional and later; adding it now would violate the "engine must work without cloud" requirement |
| `criterion` | **not added (yet)** | Neither app depends on it. Benchmarks in this milestone are a **self-contained harness** with no new dependency (see `docs/performance-baseline.md`); if benchmarking becomes continuous, Criterion is the recommended addition. |

---

## 5. Gaps in this inventory (explicit)

1. **`craft-libs` is approved and vendored.** The two pinned crates are dual MIT/Apache licensed and
   vendored under `apps/cadcraft/vendor/` with provenance and SHA-256 manifests. They remain outside
   registry advisory feeds; preserve the pin and review source updates.
2. **`vendor/` payload licences** are declared Apache-2.0 by `vendor/README.md` and match upstream; the
   individual vendored `Cargo.toml` files were not opened one by one.
3. **`references/opencadstudio`** cannot be inventoried — directory is empty.
4. **No advisory scan** (`cargo-audit`/`cargo-deny` not installed) — see `docs/security-review.md`.
   Cargo metadata for SolidCraft resolved `spade 2.15.1` as MIT OR Apache-2.0 and `zip 8.6.0` /
   `9.0.4` as MIT on 2026-10-23; these are SPDX declarations, not an advisory scan.
5. **Build-time vs runtime classification** is from manifest position (`[dependencies]` vs
   `[dev-dependencies]`) and known usage; `proptest`/`egui_kittest` are dev-only, everything else listed
   as runtime is reachable from a shipped binary (MCP/tokio only when that feature is used).

---

## 6. Recommendations

1. **Add `deny.toml` to CADCraft** — SolidCraft already has one; CADCraft does not. Target policy:
   allow `MIT`, `Apache-2.0`, `BSD-2/3-Clause`, `ISC`, `Zlib`, `Unicode-3.0`, `MPL-2.0` (with the
   unmodified-use note), **deny** `GPL-*`, `AGPL-*`, `SSPL-*`, and deny unknown licences.
2. **Record `craft-libs`** licence and vendor the pinned revisions.
3. **Collapse the `thiserror` 1/2 and `tiny-skia` 0.11/0.12 splits** when the crates are next touched —
   measurable build-time and size win, no behaviour change.
4. **Do not add a second geometry kernel.** The measured evidence (SolidCraft's 76-part oracle plus
   four patched truck crates) makes truck the only warranted primary kernel.
5. **Generate the aggregate third-party bundle in CI** from `cargo metadata` (script sketch in
   `docs/build-and-test.md` §6).

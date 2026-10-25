# License Inventory

**Audit date:** 2026-10-21 · **Scope:** every repository in `/home/sanjay/Projects/zentron-cad` and their
full resolved dependency graphs.
**Data source:** `cargo metadata --format-version 1` for each application (resolved graphs, not manifests),
`LICENSE`/`COPYING` files read directly, and per-crate manifests. Raw JSON kept at `/tmp/meta_cadcraft.json`
and `/tmp/meta_solidcraft.json` during the audit (regenerate with the commands in
`docs/build-and-test.md`).

> This document records *obligations*. It is an engineering inventory, **not legal advice**. Decisions
> marked **OWNER DECISION** must be approved by the project owner before distribution.

**Current root-workspace license:** the AtlasCAD-owned crates and documentation now use the personal,
non-commercial terms in the root [`LICENSE`](../LICENSE). That change applies prospectively to the
consolidated repository snapshot; the audit tables below preserve the component license findings from
their stated audit date. The GPL-3.0 archive discussed in the historical audit was removed without
being unpacked or copied into the repository.

---

## 1. Top-level project licensing (verified)

| Repository | Declared licence | Files present | Consequence |
|---|---|---|---|
| `AtlasCAD` root workspace | Custom personal, non-commercial terms (`Cargo.toml` `license-file`) | `LICENSE` | Personal and other non-commercial use only; commercial use requires written permission. Applies to root platform crates and AtlasCAD-authored root documentation, not imported subprojects. |
| `apps/cadcraft` | `MIT OR Apache-2.0` (`Cargo.toml`) | `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `ATTRIBUTION.md` | Permissive, dual. Owner may choose either for the combined work. Both allow closed-source distribution. |
| `apps/solidcraft` | `MIT OR Apache-2.0` | `LICENSE-MIT`, `LICENSE-APACHE`, `NOTICE`, `ATTRIBUTION.md`, `THIRD-PARTY-LICENSES.txt` | Same as above. Already ships a third-party licence bundle. |
| `kernels/truck` | Apache-2.0 | `LICENSE` (Apache-2.0 text) | Permissive + patent grant + NOTICE obligation. |
| `kernels/monstertruck` | Apache-2.0 | `LICENSE` (Apache-2.0 text) | Same; fork of truck. |
| `kernels/cascade-rs` | `Apache-2.0` (`Cargo.toml`) | no root `LICENSE` file | **Metadata-only evidence.** Before use, confirm the licence text ships with the source (add a copy). |
| `kernels/hagane` | `MIT OR Apache-2.0` | `LICENSE-MIT`, `LICENSE-APACHE` | Permissive. |
| `kernels/cadrum` | `MIT` (wrapper) | `LICENSE` (MIT, © 2026 cadrum Contributors) | **Wrapper is MIT; embedded OpenCASCADE is LGPL-2.1.** See §4.2. |
| `kernels/opencascade-rs` | **`LGPL-2.1`** | `LICENSE` (LGPL-2.1 text) | See §4.1 — **OWNER DECISION**. |
| `references/cadrs` | `MIT OR Apache-2.0` | `LICENSE-APACHE`, `LICENSE-MIT` | Reference only; not linked. |
| `references/cadrust` | `MIT` | `LICENSE` (MIT, © 2026 Taewook Kang) | Reference only. |
| `references/bevy_pmetra` | `MIT OR Apache-2.0` | `LICENSE-APACHE`, `LICENSE-MIT` | Reference only. |
| `references/opencadstudio` | **GPL-3.0** (verified from `OpenCADStudio-main.zip`) | `LICENSE` = GNU GPL v3 text; 1 797 files, 892 `.rs` | **Copyleft. No code may enter the proprietary product.** Archive left unextracted; read for ideas only. |

The imported CADCraft and SolidCraft application source declares `MIT OR Apache-2.0`, which permits
commercial use of those components subject to their notices and third-party obligations. The AtlasCAD
root platform crates have a separate personal, non-commercial license. A combined product that uses
the root crates therefore requires prior written permission for commercial use. The application
licenses do not override the root license or other imported component licenses.

---

## 2. Dependency licence distribution (measured, all targets)

### CADCraft — 485 resolved packages

| Licence expression | Count |
|---|---:|
| `MIT OR Apache-2.0` | 233 |
| `MIT` | 102 |
| `Apache-2.0 OR MIT` | 45 |
| `Unicode-3.0` | 18 |
| `MIT/Apache-2.0` (legacy syntax) | 14 |
| `Apache-2.0` | 12 |
| `Zlib OR Apache-2.0 OR MIT` | 10 |
| `MIT OR Apache-2.0 OR Zlib` | 6 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 6 |
| `Unlicense OR MIT` | 5 |
| … remainder: BSD, Zlib, ISC, MPL-2.0, dual GPL alternatives | ~34 |

**Only restrictive entries** (`grep` for GPL/MPL/CDDL/SSPL/unknown):

| Crate | Version | Licence | Assessment |
|---|---|---|---|
| `acadrust` | 0.6.4 | **MPL-2.0** | File-level copyleft. Linkable from proprietary code. Obligation: publish the MPL-covered *files* if modified. CADCraft's `crates/dwg` documents it as "used unmodified" — **that statement is the compliance control** and must remain true. |
| `colored` | 2.2.0 | **MPL-2.0** | CLI colour helper (dev/tooling path). Same file-level obligation; unmodified use is compliant. |
| `r-efi` | 5.3.0, 6.0.0 | `MIT OR Apache-2.0 OR LGPL-2.1-or-later` | **Dual — select MIT/Apache. No LGPL obligation.** |
| `self_cell` | 1.3.0 | `Apache-2.0 OR GPL-2.0-only` | **Dual — select Apache-2.0. No GPL obligation.** |

**No GPL-only dependency is present in CADCraft's resolved graph.**

### SolidCraft — 600 resolved packages

| Licence expression | Count |
|---|---:|
| `MIT OR Apache-2.0` | 296 |
| `MIT` | 119 |
| `Apache-2.0 OR MIT` | 49 |
| `Apache-2.0` | 33 |
| `Unicode-3.0` | 18 |
| `MIT/Apache-2.0` | 13 |
| `Zlib OR Apache-2.0 OR MIT` | 10 |
| `BSD-3-Clause` | 7 |
| `Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT` | 7 |
| … remainder: MIT OR Apache OR Zlib, Unlicense, BSD variants | ~48 |

Restrictive entries: **only the dual-licensed `r-efi` and `self_cell`** listed above
(select the permissive arm). **No MPL, no GPL-only, no unknown licence in SolidCraft's resolved graph.**

`truck-*` (the kernel) appears as **Apache-2.0**, consistent with the `LICENSE` file and with
`vendor/README.md` ("They keep their own licence").

---

## 3. Attribution and NOTICE obligations (Apache-2.0)

Apache-2.0 §4(d) requires preserving `NOTICE` content when redistributing. Actions already satisfied by
the repositories, plus what the unified platform must add:

| Requirement | CADCraft | SolidCraft | Unified platform |
|---|---|---|---|
| Keep `LICENSE-MIT` + `LICENSE-APACHE` | ✅ | ✅ | Ship both |
| Preserve upstream `NOTICE` | ✅ `NOTICE` | ✅ `NOTICE` | Aggregate both NOTICEs |
| State modifications to Apache-2.0 files | ✅ `ATTRIBUTION.md` | ✅ `vendor/README.md` marks changed lines `SolidCraft:` | Keep the `SolidCraft:` convention; extend it to any new patched crate |
| Third-party licence bundle for binary distribution | `ATTRIBUTION.md` | ✅ `THIRD-PARTY-LICENSES.txt` | **Generate one combined bundle** — currently missing at platform level |
| Copyright notices retained | ✅ | ✅ | ✅ |

**Gap:** there is no single aggregated third-party notice for a *combined* distribution. Generating one
is a release-engineering task (see `docs/roadmap.md`, item R-3). It can be produced from
`cargo metadata` without installing extra tooling (commands in `docs/build-and-test.md` §6).

---

## 4. Critical licence risks

### 4.1 `opencascade-rs` — LGPL-2.1 (HIGH)

Measured in manifests: `crates/occt-sys` → `LGPL-2.1`, `crates/opencascade-sys` → `LGPL-2.1`,
`crates/opencascade` → `LGPL-2.1`. LICENSE file is the LGPL-2.1 text. It builds OCCT through `build.rs`
+ cmake.

LGPL-2.1 permits use from a proprietary program **only** with the §6 conditions satisfied: dynamic
linking to the LGPL library *and* the ability for the recipient to relink with a modified version of it.
A statically linked, closed-source desktop binary does **not** satisfy this without providing object
files or equivalent relinkable form.

**Status: OWNER DECISION required. Do not link `opencascade-rs` into any proprietary build.**
It remains a *reference/candidate* only. Nothing in either app depends on it today (verified: not present
in either resolved graph).

### 4.2 `cadrum` — MIT wrapper, LGPL-2.1 payload (HIGH)

`cadrum/Cargo.toml` says `license` MIT and its README says *"a Rust CAD crate using statically-linked,
headless OpenCASCADE. Runs natively and in WebAssembly."* The `[features]` comment describes building
OCCT *"from upstream sources when the cache is empty, instead of downloading a prebuilt tarball"*, and
there are 5 C/C++ files in the tree.

**The MIT licence covers cadrum's own Rust source. It cannot relicense OpenCASCADE.** A binary that
statically links OCCT carries LGPL-2.1 obligations regardless of the wrapper's declared licence.
This is the single most commonly misread licence situation in CAD tooling, and it is exactly the trap
brief §5 warns about.

**Status: OWNER DECISION. Not usable in a closed-source distribution as a static link.** The crate also
offers a `pure` feature ("Pure Rust CAD backend (no OCCT dependency)") — that path would need separate
verification before it could be considered, and its maturity is unknown (a pure-Rust backend added to an
OCCT wrapper is not evidence of OCCT parity).

### 4.3 Vendored truck patches — `apps/solidcraft/vendor/` (MEDIUM, manageable)

7 entries: `egui-wgpu`, `matext4cgmath`, `truck-geometry`, `truck-meshalgo`, `truck-shapeops`,
`truck-stepio` (+ `README.md`). Wired via `[patch.crates-io]`. All Apache-2.0 (truck) / MIT-Apache
(egui) upstreams.

Obligations: retain upstream licence + copyright, and **mark modified files** (already done: changed
lines tagged `SolidCraft:`). Apache-2.0 §4(b) is satisfied by that marking.

Engineering consequence (carried into `docs/kernel-comparison.md`): the product does not use stock
truck — it uses **truck + 4 patched crates**. Upstream fixes must be tracked, or the fork becomes
permanent maintenance.

### 4.4 `craft-libs` Git dependency — CADCraft (MEDIUM, supply chain) — **resolved**

`craft-docpath` and `craft-ui` came from `github.com/storytold/craft-libs` pinned by `rev`
(`82a24b00…`, `5c874d51…`). They are **not on crates.io**, so their licence was not covered by any
registry metadata scan and reproducibility depended on GitHub keeping those commits reachable.

**Resolved in the 2026-10-22 cycle:** the licence was read from both pinned checkouts and recorded.

- **Licence:** `MIT OR Apache-2.0` — both checkouts' workspace `Cargo.toml` declare it, and each
  ships `LICENSE-MIT` and `LICENSE-APACHE`. All nine crates use `license.workspace = true`. The five
  vendored PDF-backend crates (`hayro`, `hayro-cmap`, `hayro-interpret`, `hayro-jbig2`,
  `hayro-syntax`) carry MIT/Apache dual licences; bundled data assets are CC0-1.0 or BSD-2-Clause
  style. **No copyleft found in either pinned tree.**
- **Vendored:** both pinned revisions are copied into `apps/cadcraft/vendor/` (`craft-libs-docpath/`
  at `82a24b00…`, `craft-libs-ui/` at `5c874d51…`) with `PROVENANCE.md` and per-file sha256
  manifests, so a release build does not need GitHub.
- **CADCraft now depends on them by path** (`apps/cadcraft/Cargo.toml`, `Cargo.lock` source fields
  removed).
- **Verification:** `cargo metadata` resolves both as path dependencies with `source: None`;
  `craft-docpath`'s standalone tests pass (12 unit + 1 doc test). The full CADCraft build could not
  be run on this host for a pre-existing, unrelated reason: `egui 0.36`'s `epaint` uses the unstable
  library feature `push_mut` (rust-lang/rust#135974), and the pinned revisions declare
  `rust-version = "1.95"` while this host has rustc 1.94.0. Neither limitation is introduced by
  vendoring.

### 4.5 `references/opencadstudio` — GPL-3.0 (verified)

The source arrived as `OpenCADStudio-main.zip` (39 MB) and its `LICENSE` is the **GNU GPL v3** text;
the archive holds 1 797 files, 892 of them Rust. It is therefore **GPL-3.0: no code may enter the
proprietary platform**. It stays packed and unextracted, and is a reference to read for behaviour and
architecture only — unless the owner decides to relicense the whole product as GPL-3.0, which would
end the proprietary subscription plan.

Engineering note: 892 Rust files is a substantial CAD application and a genuinely useful source of
ideas (command model, UI structure). Reading it is fine; copying from it is not.

---

## 5. Obligation summary for distribution

| Obligation | Status | Owner |
|---|---|---|
| AtlasCAD root crates used only non-commercially unless written permission is granted | ✅ stated in root `LICENSE` | owner / distributor |
| Application code MIT/Apache → closed distribution allowed | ✅ verified | — |
| Preserve MIT + Apache texts and NOTICEs | ✅ both apps do | release |
| Mark Apache-2.0 modifications (vendored truck) | ✅ `SolidCraft:` tags | maintainers |
| Aggregate a combined third-party notice bundle | ❌ **missing** | release (R-3) |
| MPL-2.0 `acadrust` used unmodified → no source publication needed | ✅ documented, must stay true | cadcraft maintainers |
| No static LGPL in proprietary binary | ⚠️ **must remain true** — exclude `opencascade-rs` and `cadrum`'s OCCT | owner decision |
| Dual-licensed deps resolved to permissive arm | ✅ (MIT/Apache chosen) | build |
| `craft-libs` licence recorded | ✅ **recorded** — MIT OR Apache-2.0; vendored at two pinned revisions | release/audit |
| `opencadstudio` assessed | ✅ **GPL-3.0, verified**; archive unextracted, nothing copied | owner |

The imported applications' permissive licenses do not override the AtlasCAD root license. Commercial
use of AtlasCAD root crates requires prior written permission. The remaining third-party distribution
obligations include keeping LGPL/OCCT paths out of any binary that cannot satisfy their terms and
producing the aggregate third-party notice bundle (`task.release.notice-bundle`). The `craft-libs`
Git dependency is recorded, pinned, and vendored.

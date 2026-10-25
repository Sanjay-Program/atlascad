# License Audit (summary)

**Canonical data:** `engineering/manifest/repositories.json` (per-entry licence and status),
`engineering/manifest/dependencies.json` (per-edge licence and risk),
`engineering/manifest/workspace.json` (allow/deny policy).
**Full analysis:** [../license-inventory.md](../license-inventory.md) — 485 CADCraft packages and 600
SolidCraft packages resolved from `cargo metadata`. Not legal advice.

## 1. Policy enforced by the validator

* Allowed: MIT, Apache-2.0, BSD-2/3, ISC, Zlib, Unicode-3.0, MPL-2.0, and their dual forms.
* Denied substrings: `GPL`, `SSPL`, `CDDL`, `Commons Clause`. `LGPL` matches `GPL` deliberately.
* An entry marked `approved` must be on the allow list; any deny-listed entry marked `approved` is a
  validation **error**, not a warning.

## 2. Status by repository

| Status | Repositories |
|---|---|
| approved | CADCraft, SolidCraft (both `MIT OR Apache-2.0`), truck, monstertruck, hagane, cadrs, cadrust, bevy_pmetra, and pinned `craft-libs` (`MIT OR Apache-2.0`) |
| needs_review | cascade-rs — Apache-2.0 declared in `Cargo.toml` but **no LICENSE file** in the checkout |
| unrecorded | none in the current curated dependency manifest; `spade` and both resolved `zip` versions were checked through Cargo metadata on 2026-10-23 |
| restricted | cadrum (MIT wrapper, **statically links LGPL-2.1 OpenCASCADE**) |
| prohibited | opencascade-rs (LGPL-2.1) |

The GPL-3.0 OpenCADStudio archive was examined during the 2026-10-21 audit, then removed without
unpacking or copying its source. It is not part of the consolidated repository snapshot.

The nine AtlasCAD root crates use the repository's custom personal, non-commercial license in
[`LICENSE`](../../LICENSE). That license is separate from this third-party dependency allow list. It
does not relicense any imported source tree.

## 3. Third-party constraints for commercial distribution

1. **LGPL / OpenCASCADE.** `opencascade-rs` is LGPL-2.1 and `cadrum` statically links OCCT. Neither is
   in a shipping graph today, but nothing but review prevents a future `cargo add`. Owner decision G1
   is open; the manifest holds both as `critical` dependency edges and the report lists them.
2. **Unrecorded `craft-libs`.** Two crates from a git remote, pinned by revision, invisible to advisory
   feeds, licence unknown.

MPL-2.0 appears twice (`acadrust`, `colored`) and is compliant **only while unmodified** — modifying
those files creates a source-publication obligation.

## 4. Obligations still open

* A single combined third-party notice bundle for distribution does not exist (`task.release.notice-bundle`).
* `cargo-deny` is not installed, so the policy is enforced by the manifest validator but not yet in CI.

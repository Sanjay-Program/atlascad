# ADR-0003: No statically linked LGPL in a shipping binary

- **Status:** Accepted (engineering). **Requires owner sign-off for commercial distribution.**
- **Date:** 2026-10-21
- **Deciders:** engineering; owner for G1 in `docs/roadmap.md`
- **Evidence:** `docs/license-inventory.md`

## Context

Both applications are `MIT OR Apache-2.0`, which permits closed-source distribution. Three third-party
situations complicate the *combined* product:

1. **`kernels/opencascade-rs`** — `LGPL-2.1` (verified in three crate manifests and its `LICENSE`).
   LGPL-2.1 §6 permits use from a proprietary program only with dynamic linking plus the recipient's
   ability to relink against a modified library.
2. **`kernels/cadrum`** — its own code is **MIT**, but it *"uses statically-linked, headless
   OpenCASCADE"*. The MIT grant cannot extend to OCCT: a binary that statically links OCCT carries
   LGPL-2.1 obligations regardless of the wrapper's declared licence.
3. **`references/opencadstudio`** — **absent** (empty directory). If it is GPL-3.0 as reported, no code
   from it may enter a proprietary distribution.

Also relevant but benign: `acadrust` (CADCraft's DWG bridge) is **MPL-2.0**, file-level copyleft —
linkable from proprietary code provided MPL-covered files that are modified are published. CADCraft
documents it as *"used unmodified"*; that statement is the compliance control and must remain true.
Dual-licensed crates (`r-efi`, `self_cell`) must be resolved to their permissive arm.

## Decision

1. **Do not link `opencascade-rs` into a shipped binary.** It stays a read-only reference.
2. **Do not ship a `cadrum` build that includes OCCT.** Its `pure` backend, if it is genuinely
   OCCT-free and mature, would need separate verification before it could be considered.
3. **Do not incorporate any code from `opencadstudio`** unless the product is relicensed to GPL-3.0.
4. **Keep `acadrust` unmodified**, and record that fact in the release notes.
5. Enforce (1) and (2) mechanically: a dependency-policy check (`cargo-deny`) plus a CI guard that
   fails if `opencascade*` or `cadrum` appear in a shipping graph.

## Consequences

* The product cannot use OpenCASCADE's STEP/IGES robustness or its fillet quality. This is why the
  truck-based kernel matters, and why its limitations (ADR-0002) are a product constraint, not just an
  engineering detail.
* If OpenCASCADE becomes necessary (e.g. a customer mandates STEP AP242 fidelity), the owner must
  choose: dynamic linking with relink rights, or relicensing. Both are business decisions with cost.
* The Apache-2.0 obligations for truck and the NOTICE/attribution duties for every dependency remain;
  an aggregated third-party notice bundle is required before release (roadmap M5, R-3).

## What this ADR is not

It is not legal advice, and it does not resolve G1. It records the engineering position — **exclude by
default** — so that no build quietly acquires an obligation nobody approved.

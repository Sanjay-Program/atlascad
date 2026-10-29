# ADR-0002: truck is the primary 3D kernel, with SolidCraft's patches

- **Status:** Accepted
- **Date:** 2026-10-21
- **Deciders:** engineering (this audit)
- **Evidence:** `docs/kernel-comparison.md`, `docs/performance-baseline.md`

## Context

Six kernels were available. Two facts dominated the decision:

1. **SolidCraft already ships truck behind a kernel-agnostic boundary** — `apps/solidcraft/crates/kernel`,
   94 803 LOC, with a documented promise that no truck type appears in its public API and a panic guard
   around every third-party call.
2. **The published truck crates are not sufficient on their own** (measured):
   * every boolean between solids with coplanar, overlapping faces returns no result, at three model
     scales, across a tolerance sweep, for both common construction styles;
   * a difference returns a solid whose volume is *larger* than the target's (1053 mm³ from a
   * **Resolved 2026-10-28:** the volume was wrong because the walls of the cavity were wound
     outward, not because the geometry was wrong. The adapter now gives each shell the sign its
     nesting depth requires, the analytic cut fixtures pass exactly, and `BooleanDifference` is
     advertised (`docs/kernel-comparison.md` §4.2). The decision recorded here — truck as the
     primary kernel, reached through an adapter — is unchanged.
     1000 mm³ plate), i.e. malformed topology;
   * a third chained union panics inside `intersection_curve` (*"tolerance must be no less than 1e-6"*).

SolidCraft vendors four patched truck crates that fix exactly those areas (intersection curves, face
division, integration, ordering) and adds retries, reseams, history and volume guards on top.

## Decision

**Use truck 0.6 through SolidCraft's kernel as the product's primary 3D kernel.** The shared contract
reaches it through an adapter. The published crates are used only with SolidCraft's
`[patch.crates-io]` patches applied.

**No second kernel.** All other candidates remain isolated, read-only references.

## Alternatives considered

| Alternative | Rejected because |
|---|---|
| **monstertruck** | A fork of the same kernel (456 vs 274 files, same Apache-2.0). Choosing it over the copy that already has a 76-part oracle and a production integration adds risk with no demonstrated gain. |
| **cascade-rs** | Apache-2.0, pure Rust, self-declared "80/20 parity with OpenCASCADE"; not measured. No advantage demonstrated over the already-integrated kernel. |
| **hagane** | MIT/Apache, pure Rust, self-described "early, working foundation". |
| **cadrum** | MIT wrapper that **statically links LGPL-2.1 OpenCASCADE**; the wrapper's licence does not cover the payload. Legal decision required, and it needs a native OCCT build. |
| **opencascade-rs** | **LGPL-2.1** bindings plus a full OCCT build; static linking into a closed-source binary does not satisfy §6. |
| **Write our own kernel** | 94 803 LOC of existing work, plus an oracle, plus four patched upstream crates. Not a realistic first milestone. |

## Consequences

* The platform inherits a **fork responsibility**: the four vendored crates must be tracked against
  upstream, and their fixes should be offered upstream rather than diverging forever.
* **Boolean difference and boolean chains are unavailable** through the shared adapter until either
  upstream or SolidCraft's kernel exposes reliable ones. The adapter refuses rather than returning a
  wrong solid.
* Coplanar booleans need a ~1e-4 mm tool nudge; this is implemented and counted, and costs ~0.004 mm³
  of volume accuracy on a 1120 mm³ part.
* A future kernel swap is an adapter, not a rewrite — the contract is the seam.
* The workspace's `cad-kernel-truck` crate depends on `apps/solidcraft/vendor/*` by path. That coupling
  is intentional and must be pinned/vendored before release packaging (security review S-5).

## Review trigger

Reopen this decision if: SolidCraft's kernel cannot represent a feature the product must ship; a
boolean corpus shows another pure-Rust kernel materially more reliable; OpenCASCADE's licence changes;
or WASM becomes mandatory and the truck-based kernel cannot meet the size/startup budget.

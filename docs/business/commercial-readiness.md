# Commercial readiness review

**Date:** 2026-10-25. **Verdict:** the current repository is **not sellable** as a
$5,000–$10,000 per-seat/year CAD product, and the root platform licence currently forbids commercial
sale. Both are fixable, but neither is a feature toggle.

This document is a product and engineering plan, not legal, tax, or financial advice. No revenue,
margin, timeline, or certification is claimed.

## 0. Owner decisions recorded 2026-10-25

- Commercial relicense of the root crates: **authorized by the owner**, subject to the written
  agreement and legal review described below. The non-commercial `LICENSE` remains the default; see
  [`COMMERCIAL-LICENSE.md`](../../COMMERCIAL-LICENSE.md).
- Flagship product: **productize SolidCraft**, not Atlas Studio.
- First paid deployment: **signed desktop, optional cloud** for review/PDM.
- Entry price: **$2,400–$4,800 per design seat/year**, with the Team bundle earning the
  $7,500–$9,500 band after PDM, SSO, audit, and support exist.

These decisions authorize the plan and the build spike. They do not make the current software
sellable, and they are not a substitute for a signed customer agreement.

---

## 1. The three hard blockers

### B1 — Licence: commercial sale is prohibited today

The root `LICENSE` is the **AtlasCAD Personal and Non-Commercial License 1.0**. It forbids using,
distributing, hosting, or selling the covered root crates as part of a commercial activity without
prior written permission from the rights holder. The imported trees under `apps/`, `kernels/`, and
`references/` retain their own licences.

To sell:

1. The rights holder must authorize and record a commercial licence (or dual licence) for the root
   crates and Atlas-authored documentation.
2. The commercial EULA, terms of service, privacy policy, DPA, support policy, and refund policy must
   be written and reviewed.
3. Third-party notices, SBOM, and source-offer obligations must be preserved. SolidCraft is
   MIT OR Apache-2.0; CADCraft is MIT OR Apache-2.0; `acadrust` is MPL-2.0; OCCT, if ever used, is
   LGPL-2.1 with the OCCT exception and needs counsel review.

Do not change the root licence silently. That is an owner decision with legal consequences.

### B2 — Product: the table stakes are not there

At $5k–$10k, the buyer compares against CATIA Essential ($7,229/year) and CATIA Master
($9,230/year), and expects production-grade:

- parametric parts and assemblies with predictable rebuilds;
- associative drawings with dimensions, tolerances, and annotations;
- tested STEP/IGES import and export, plus DXF where drafting is sold;
- PDM/revision/release control and traceable released drawings;
- collaboration/review, SSO, RBAC, audit logs, backup/restore;
- signed installers, controlled updates, crash recovery, support with response targets;
- security evidence appropriate to the deployment.

Atlas Studio today has one sketch, one extrude, no constraints, no drawings, no assemblies, no
STEP/IGES/DXF, no PDM, no accounts, no autosave/recovery, and no signed desktop packaging. It is a
verified demo and UI seed, not a sellable seat.

SolidCraft is the strongest asset: constraints, feature timeline, assemblies/joints/motion, sheet
metal/flat patterns, STEP/IGES/3MF/STL/OBJ/glTF, packaging, and ~5,640 test functions. Its own docs
say **drawings, surfaces, CAM, and simulation are deferred**. It does not have enterprise identity,
PDM, audit, signed installers, or a commercial support organization.

### B3 — Trust: no support, security evidence, or commercial operations

A $5k–$10k buyer will ask for a security questionnaire, a support contact, an SLA, data handling,
backup/restore, export control, and a migration plan. The repository has a security review, but no
SOC 2 report, no pen test, no incident process, no DPA, no status page, and no support desk. None of
those can be faked with a landing page.

---

## 2. What is actually strong in the repository

| Asset | What it gives us | Limitation |
|---|---|---|
| `apps/solidcraft` | Mature parametric 3D engine, constraints, assemblies, sheet metal, STEP/IGES, desktop/browser/CLI/MCP, packaging scripts, MIT/Apache | Not built on the 7.4 GB host; drawings/CAM/simulation deferred; no PDM/identity; unsigned downloads |
| `apps/cadcraft` | Mature 2D drafting, DXF/DWG, constraints, layouts, plotting, MCP | 2D only; DWG compatibility unverified; heavy build |
| `cad-contract` / `cad-kernel-truck` | Clean kernel boundary, budgets, capability gates, regression corpus | truck cannot do difference or boolean chains; not a production kernel for a broad commercial product |
| `cad-document` / `cad-studio` | Small, tested, understandable local app and native project format | One feature; not the commercial product base |
| `cad-agent-*` | Typed, permissioned agent runtime | One agent; no scheduler or process isolation |
| `cad-engineering` | Manifest-driven evidence and control plane | Needs to track commercial gates and release evidence |

**Recommendation:** use SolidCraft as the geometry/parametric/assembly/sheet-metal engine and desktop
base. Use CADCraft as the drafting/drawings source when that work starts. Keep Atlas Studio as a
focused, modern UI experiment and a possible lightweight product, not the flagship.

**Spike result (2026-10-25): the build condition passed.** SolidCraft's CLI built from source on
this 7.4 GiB machine in 37 m 26 s at `-j1`, and a parameterised bracket ran end to end — parametric
rebuild, fillet, boolean cut, analytic volume agreement to 0.000153 %, AP242 STEP export and
round trip to 2.8 × 10⁻⁵ %, bit-identical native save/reopen, GPU snapshot render. Packaging and
signing remain unproven (no OS code signing, no update key, no root CI). Full evidence, build
constraints, and the ordered deployable gate: [`packaging-and-deployment.md`](packaging-and-deployment.md).

---

## 3. Recommended product: Atlas Professional

**Positioning:** the fastest local-first mechanical CAD for small product-engineering and
fabrication teams, with optional cloud review/PDM. Not "the world's best CAD"; a focused product that
wins on a measurable workflow.

**Target customer (initial):** 5–50-seat mechanical/product teams and fabrication shops that design
configurable parts and sheet-metal assemblies, already use STEP/DXF with suppliers, and are
frustrated by cloud dependency, licence surprises, or heavyweight PLM.

**Deployment:** signed Windows/macOS/Linux desktop application with local compute and local files;
optional cloud workspace for sharing, review, and release control; offline activation and a
perpetual fallback for the local seat. Browser-only compute is not the launch plan.

**Differentiators to earn, in order:**

1. **Local performance and offline reliability** measured on ordinary hardware.
2. **Migration that tells the truth:** import STEP/IGES/DXF, report what survived, and keep native
   feature history only in Atlas files.
3. **Focused sheet-metal-to-shop workflow:** parameters → flat pattern → bend table → drawing/DXF →
   BOM → revision.
4. **Bounded AI assistance:** typed operations with plan/validate/preview/apply, never generated code.
5. **Transparent licensing:** published price, what survives cancellation, no forced cloud.

---

## 4. Pricing hypothesis

The price must match the evidence. Proposed ladder:

| Package | Price (illustrative, USD) | Contents | When it can be sold |
|---|---:|---|---|
| **Atlas Design** | **$2,400–$4,800 / seat / year** | Local CAD seat, parts/assemblies/sheet metal, STEP/IGES/DXF exchange, updates, standard support | After signed installer, reference workflows, crash recovery, drawings (or an explicit no-drawings limitation), and support exist |
| **Atlas Team** | **$7,500–$9,500 / seat / year** | Design + cloud PDM/release, review/guest access, SSO, RBAC, audit log, backup, priority support | After the cloud workspace and security controls are operating and tested |
| **Atlas Enterprise** | **From $12,000 / seat / year** | On-prem/private cloud, advanced admin, migration services, custom SLA, validated vertical templates | After a reference customer, security review, and support organization |
| **Design-partner pilot** | **$0–$2,500 / seat for 90 days** | Paid or discounted pilot with weekly workflow measurement | Immediately after B1 is resolved and a pilot agreement exists |

**Do not launch at $5k–$10k for the current product.** A $5k price with one feature is a credibility
loss that is hard to recover from. Earn the band with the Team bundle, or enter a validated vertical
where the outcome is worth the price.

The master prompt's earlier hypothesis (₹399/₹999 per month) is a different market. At $5k–$10k/year,
the buyer is an engineering organization with procurement, security, and support requirements.
Those are different products and different cost structures; do not mix the two.

### Illustrative unit economics (not a forecast)

At 100 seats on a $8,000 Team price: $800k ARR. Illustrative annual costs:

- support engineer (1 per ~150 seats, loaded): ~$120k;
- cloud/storage/backup (~$25/seat/month): ~$30k;
- payment fees (~3%): ~$24k;
- sales/marketing (~20% of revenue): ~$160k;
- security/compliance/legal/tooling: ~$60k;
- engineering (4 FTE loaded): ~$600k.

That leaves roughly $800k − $994k = **negative** before any profit at 100 seats. The product needs
roughly 200–300 seats at this price, or a lower cost base, to be a viable business. This is why
"$5k–$10k per seat" is not automatically a high-margin plan: support and engineering dominate at low
volume. Validate with real quotes and pilot data; do not use these illustrative numbers in an
investor deck without replacing them with measured costs.

---

## 5. Sellable v1 acceptance criteria

A release can be offered commercially only when all of these are true:

| Area | Acceptance criterion | Evidence required |
|---|---|---|
| Licence | Root commercial licence/EULA in place; third-party notices/SBOM complete | Signed owner decision; legal review; `NOTICE`/SBOM |
| Install | Signed Windows/macOS/Linux installers; uninstall; offline activation | Reproducible signed builds; clean-VM install tests |
| Core modeling | Parts, assemblies, constraints, sheet metal, STEP/IGES import/export on 20 reference models | Regression corpus with analytic/reference values; no known crash |
| Drawings | Associative drawing with views, dimensions, annotations, title block, PDF/DXF output, update on model change | Drawing fixtures and update tests; documented limits |
| Data safety | Autosave/journal, recovery after kill -9, backup/restore, atomic writes, schema migration | Fault-injection tests; restore drill |
| Interoperability | STEP/IGES import/export round trip with units/assembly structure; DXF exchange; documented losses | File corpus and compatibility report |
| Performance | Startup, open, rebuild, save, assembly load, drawing update measured on minimum hardware | Published benchmark with hardware and input |
| Security | Threat model, dependency scan, secret scan, signed updates, no telemetry without consent | Audit report; CI gates; vulnerability process |
| Cloud (Team) | Tenant isolation, SSO, RBAC, audit log, encryption, backup, deletion, rate limits | Multi-account isolation tests; pen test; DPA |
| Support | Published channels, severity/response targets, status page, known-issues policy | Support runbook and staffed queue |
| Migration | Import from at least one incumbent format; report what survived; rollback | Pilot customer migration record |
| Commercial | Price, terms, refunds, taxes, entitlement enforcement, invoicing | Signed contracts and billing integration |

"Implemented" is not "verified"; "verified" is not "supported"; "supported" is not "sellable".

---

## 6. Staged plan

### Gate 0 — Owner and market decisions (1–3 weeks)

- Confirm ownership and authorize a commercial licence path (or stop).
- Pick one initial segment and one reference workflow.
- Recruit 5 design partners; sign pilot agreements; collect 10 real models and current tool costs.
- Build and benchmark SolidCraft on a 32 GB machine; record startup, assembly load, STEP import,
  sheet-metal flat pattern, and packaging on Windows/macOS/Linux.
  **Partly done 2026-10-25** on a 7.4 GiB host: CLI build + reference-part/STEP evidence recorded
  in [`packaging-and-deployment.md`](packaging-and-deployment.md). Still open: release-profile build,
  assemblies, sheet metal, and Windows/macOS packaging.
- Trademark/domain/package-name search for "Atlas" in the CAD class. Do not claim availability
  without a search.

**Exit:** written go/no-go; if SolidCraft cannot be built/packaged reproducibly, revisit the engine.
Current status: build **passed**, packaging **not yet attempted** (gates D1–D5 in
[`packaging-and-deployment.md`](packaging-and-deployment.md)).

### Gate 1 — Credible desktop seat (3–6 months, 3–5 engineers)

- Signed installers, auto-update, crash reporting (opt-in), licence activation, offline fallback.
- Onboarding, sample projects, command palette, error explanations, keyboard map.
- 20 reference parts/assemblies; STEP/IGES/DXF exchange with a loss report.
- Autosave/recovery and backup/restore.
- Performance baselines on minimum hardware.
- Design-partner pilots; measure time-to-manufacturable-package and migration fidelity.

**Exit:** 5 paying pilots; zero data-loss incidents; no P0 crashes on reference workflows; measured
performance published. This is the first point a $2.4k–$4.8k Design seat is defensible.

### Gate 2 — Drawings and Team cloud (6–12 months)

- Associative drawings and PDF/DXF output, or an explicit partner integration if building is slower.
- Cloud workspace: projects, versions, review/comments, guest access, release workflow.
- Auth (OIDC/SAML), RBAC, audit log, tenant isolation, encryption, backup, deletion, rate limits.
- Billing/entitlements; DPA; support desk; status page.
- Security assessment and pen test; SOC 2 readiness only if customers require it.

**Exit:** Team bundle at $7.5k–$9.5k with a reference customer and a passed security review. This is
the earliest defensible entry into the $5k–$10k band.

### Gate 3 — Differentiation (12+ months)

- Sheet-metal-to-shop vertical package with validated templates and output.
- Bounded AI copilot for parameters/features/failure explanations with plan/validate/apply and audit.
- Migration service and partner integrations.
- Enterprise/on-prem package.

No "world #1" milestone exists. The goal is "measurably best at X for Y", with evidence.

---

## 7. Security and compliance workstream

- Threat model for desktop, update channel, licence server, cloud workspace, and AI boundary.
- SBOM, dependency/advisory gates, secret scanning, reproducible builds, signed artifacts.
- Encryption in transit and at rest; key management; tenant isolation; backup/restore drills.
- SSO, RBAC, audit, data retention/deletion, export controls, subprocessor list.
- Incident response, vulnerability disclosure, security contact, status page.
- SOC 2 Type II / ISO 27001 only when a customer requires it; budget 6–12 months and real audit cost.
- Do not train on customer designs without explicit informed opt-in.
- Do not send models to an AI provider without consent and a documented data-handling policy.

---

## 8. Decisions required before implementation

1. **Commercial rights.** Does the owner authorize a commercial relicense of the root crates?
2. **Flagship.** Productize SolidCraft's engine/desktop, continue Atlas Studio, or build a vertical
   sheet-metal product?
3. **Deployment.** Signed desktop first, private cloud, or on-prem appliance?
4. **Price.** Hold $5k–$10k from day one, or start at $2.4k–$4.8k and earn the band with the Team
   bundle?

Until 1–4 are answered, building commercial auth, billing, or a public deployment would be guessing.

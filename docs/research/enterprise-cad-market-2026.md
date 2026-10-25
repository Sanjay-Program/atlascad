# Professional CAD market at $5k–$10k per seat/year

**Research date:** 2026-10-25. **Purpose:** test whether Project Atlas can be sold as a
$5,000–$10,000 per-seat/year product, and what would have to be true for that claim.

This is market evidence, not a forecast, legal advice, or a claim that any vendor is better than
another. Prices move; re-check the checkout page before quoting a customer.

---

## 1. Verified price anchors

Only prices read directly from a vendor page on 2026-10-25 are marked **verified**. Prices carried
forward from the repository's earlier research (2026-10-23) are marked **recorded** and must be
re-verified before use in a proposal.

| Product / tier | Price | Basis | Status |
|---|---:|---|---|
| **CATIA Design Engineering — Essential** | **$7,229 / year** ($602/month billed annually) | Per user; parts, surfaces, sheet metal, assemblies, mechanism motion, 3D review, drafting, 3DEXPERIENCE collaboration | **Verified** — [Dassault buying guide](https://www.3ds.com/store/what-price-catia) |
| **CATIA Design Engineering — Master** | **$9,230 / year** ($769/month billed annually) | Higher Design Engineering package | **Verified** — same page |
| **CATIA Design Engineering — Elite** | **$14,926 / year** | Above the target band | **Verified** — same page |
| **CATIA Design & Simulation Engineering — Essential** | **$11,188 / year** | Above the target band | **Verified** — same page |
| **Onshape Standard** | **$1,500 / user / year** | Commercial CAD, assemblies, drawings, private storage, built-in PDM | **Verified** — [Onshape pricing](https://www.onshape.com/en/pricing) |
| **Onshape Professional** | **$2,500 / user / year** | Adds company-managed data, release management, advanced PDM, simulation, rendering, ECAD/MCAD, CAM | **Verified** — same page |
| **Onshape Enterprise** | Custom | Adds SSO, advanced admin/security, guest/light users, analytics, priority support | **Verified** — same page |
| **Alibre Design Expert** | **$500 / year**, or **$2,300 lifetime** | Sheet metal, weldments, PDM, libraries, rendering, variants | **Verified** — [Alibre pricing](https://www.alibre.com/buy-now/) |
| SOLIDWORKS Standard / Professional / Premium | $2,820 / $3,456 / $4,716 per year | US single-user, annual, taxes excluded | Recorded 2026-10-23 — [competitor analysis](competitor-analysis.md) |
| PTC Creo Design Essentials | $4,110 / year | Annual, support included, taxes extra | Recorded 2026-10-23; official page was not readable in the last pass |
| Autodesk Fusion | $680 / year | One user, annual | Recorded 2026-10-23 |
| Siemens Designcenter X | ~$9,950–$13,475 / year for higher tiers | Storefront listings; tier naming was ambiguous | **Low confidence** — do not quote without a current Siemens quote |

**What this means:** the $5k–$10k band is occupied by **CATIA Essential/Master**. It is above
Onshape Professional, SOLIDWORKS, Creo Essentials, and Fusion by a wide margin. A new product cannot
enter that band on a feature list alone. At that price the buyer is buying **production confidence,
interoperability, data control, support, and risk reduction**, not just modeling commands.

---

## 2. What the buyer expects at this price

The buyer bar below separates universal table stakes from enterprise-conditional requirements.
It is derived from vendor plan documentation and a PDM/PLM buyer framework; it is a procurement
checklist, not a measured survey.

| Area | Table stakes at $5k–$10k | Usually premium / conditional |
|---|---|---|
| **Parts** | History-based parametric modeling, robust constraints, predictable rebuilds, persistent references | Advanced surfacing, knowledge-based engineering, specialized composites |
| **Assemblies** | Component instances, mates/joints, subassemblies, interference, performance on real assemblies | Very large assemblies, motion, advanced configuration management |
| **Drawings** | Associative views, dimensions, annotations, tolerances, title blocks, reliable update and export | MBD, automated tolerancing, drawing standards automation |
| **Interoperability** | Tested STEP and IGES import **and** export; DXF where drafting is sold; units, assemblies, colors, metadata, healing behavior documented | Native translators, batch translation, translation governance |
| **PDM** | Version history, release/revision control, permissions, no accidental overwrite, traceable released drawing/model | Full PLM, ECO workflows, multi-site, enterprise BOM governance |
| **Collaboration** | Sharing, review/markup, comments, version awareness; lightweight reviewer access | Real-time co-editing, external workspaces, analytics |
| **Identity** | SSO (SAML/OIDC) for enterprise; RBAC by project/role | SCIM, delegated admin, multiple IdPs |
| **Audit** | Searchable access/change/export/release log with retention | SIEM streaming, immutable records, custom reports |
| **Security evidence** | Encryption, vulnerability handling, incident response, backup/restore; SOC 2 Type II or ISO 27001 for cloud | FedRAMP, sovereign cloud, customer-managed keys |
| **Deployment** | Signed installer, auto-update or controlled updates, offline-capable working set if sold as desktop | Regional hosting, dedicated tenant, on-prem |
| **Support** | Defined channels, severity levels, response targets, maintenance policy | 24×7, named TAM, service credits, migration services |
| **Onboarding** | Sample projects, training, admin setup, migration guidance | Tailored implementation and process redesign |

**Drawings are the sharpest gap in this repository.** SolidCraft's own documentation says surfaces,
drawings, CAM, and simulation are deferred. Atlas Studio has no drawing document at all. A product
without associative drawings is not credible at $5k–$10k for mechanical engineering teams.

**PDM is the second sharpest gap.** Onshape Professional at $2,500 includes built-in PDM and release
management. CATIA at $7,229 includes 3DEXPERIENCE collaboration. A $5k–$10k product with shared
folders and no release control would be underpriced relative to its claims and overpriced relative
to its evidence.

---

## 3. Where incumbents are actually weak

The defensible openings are narrower than "cheaper" or "AI".

| Weakness | Evidence | Realistic opening |
|---|---|---|
| Cloud dependency and latency | Onshape's own performance guidance requires a reliable connection; large models take time to load | Local-first compute, local files, optional sync; but local must be benchmarked on ordinary hardware |
| Subscription and licensing anxiety | SOLIDWORKS changed perpetual/subscription terms; buyers care what happens after cancellation | Transparent terms, perpetual fallback, offline activation, no loss of core local work |
| Migration risk | Neutral formats preserve geometry but commonly lose native feature history and downstream structure | Measured migration reports that state exactly what survived, plus coexistence with existing CAD |
| Interface complexity | Community reports about FreeCAD workbenches and CAD learning curves are anecdotal, not prevalence data | A focused workflow with guided defaults, explainable failures, and keyboard/command access |
| AI hype vs useful automation | Autodesk has announced assistant/drawing automation; editable B-rep generation is still an active research area | Bounded, inspectable operations: change a parameter, create a feature, explain a failure, prepare a drawing checklist — never unvalidated code |
| Support variability | Reseller-dependent support is real, but there is no reliable comparative evidence of broad dissatisfaction | Published response commitments and migration help can build trust, but support is expensive |

None of these is a moat by itself. The strongest hypothesis from the research is:

> **A local-first CAD product focused on one repeatable manufacturing workflow, with transparent
> licensing, measured migration fidelity, and guided automation to a manufacturing-ready package,
> can win initial adoption.** It will not beat CATIA or SOLIDWORKS on breadth, and it should not
> claim to be "world number one".

The most concrete candidate is a sheet-metal/fabrication workflow: configurable parts, flat
patterns, bend tables, drawings, BOM, and shop-ready DXF/STEP handoff. SolidCraft already has sheet
metal, flat patterns, bend tables, and DXF output; it does not have associative drawings or a
release-controlled PDM. That is a real foundation, but the workflow is not complete.

---

## 4. Technical reality

### Kernels

| Option | Commercial binary | Maturity | Cost/risk |
|---|---|---|---|
| **Truck** (Rust, Apache-2.0) | Permitted with notices | B-rep, booleans, meshing, healing; upstream fillets were still incomplete in the reviewed record | No royalty; high engineering and validation burden; current adapter cannot do difference or chains |
| **OpenCASCADE / OCCT** (LGPL-2.1 + OCCT exception) | Permitted subject to LGPL obligations; counsel must review linkage and notices | Broad B-rep, fillets, booleans, STEP/IGES, XDE assemblies | No licence fee; large C++ integration, build and support burden |
| **Parasolid / ACIS** | Commercial contract | The mature commercial choice; used by SOLIDWORKS, NX, Solid Edge, Onshape (Parasolid) | Quote-only; contractual dependency and royalties |
| **SolveSpace** (GPLv3) | Not suitable for a proprietary binary without a separate licence | Compact CAD + 2D solver | GPL copyleft |
| **STEPcode / ezdxf** | Permissive | Translators only, not kernels | Useful, but not a CAD engine |

The repository's current ADR excludes LGPL/OCCT. That decision should be revisited only with legal
review; the OCCT exception may permit a proprietary application, but the obligations are real.

### The asset that matters

`apps/solidcraft` is **far closer to a commercial CAD core than Atlas Studio**:

- ~1.15M Rust LOC, ~5,640 `#[test]` functions, a 94,803-LOC kernel over truck, and a reference-part
  oracle claiming 75/76 parts match analytic/FreeCAD values.
- Sketch constraints, parametric timeline, extrude/revolve/sweep/loft/fillet/chamfer/shell/draft/
  holes/threads/patterns, parameters and configurations.
- Assemblies with joints, motion studies, contact sets, interference and mass properties.
- Sheet metal with flanges, hems, fold/unfold, flat patterns and bend tables.
- STEP, IGES, 3MF, STL, OBJ, glTF, URDF and native files.
- Desktop, browser, CLI and MCP surfaces, plus packaging for Windows/macOS/Linux.
- MIT OR Apache-2.0, so commercial distribution is permitted with notices.

Its documented gaps: **drawings, surfaces, CAM, simulation**, signed installers, and an enterprise
data/identity layer. It was not built on the recorded 7.4 GB machine; one crate build peaked at
2.56 GB and took 8m22s. A 32 GB development machine is a practical prerequisite.

`apps/cadcraft` is a mature 2D drafting application (DXF/DWG, constraints, layouts, plotting, MCP)
and is the obvious source for a future drafting/drawings layer. It is MIT/Apache-2.0, with
`acadrust` (MPL-2.0) for DWG and an unverified native-DWG compatibility claim.

**Conclusion:** the fastest route to a product with real table stakes is to productize the
SolidCraft engine and desktop surface, and add the missing commercial layers. Rewriting a CAD
kernel or trying to grow Atlas Studio into a $5k product would be years of work with a worse
starting point.

---

## 5. What Atlas Studio is today

Atlas Studio is a verified single-feature local workflow: one rectangle/circle/polygon sketch, one
extrude, named parameters, feature tree, WebGL viewport, face picking, undo/redo, atomic save,
reopen, and binary STL export. It has 227 tests across the platform workspace and a clean local
security envelope (loopback bind, Host/Origin checks, body limits, timeouts).

It is a good **technology demonstration and UI seed**. It is **not** a commercial CAD product:

- no sketch constraints or dimensions beyond named profile parameters;
- no multiple features, booleans in the app, revolve, fillet, shell, or assemblies;
- no drawings, BOM, mass-properties report, or materials;
- no STEP/IGES/DXF import or export;
- no autosave, crash recovery, named revisions, or backup;
- no accounts, PDM, collaboration, audit, licensing, or billing;
- no native signed installer, auto-update, or crash reporting.

At $5k–$10k, none of these can be waved away as "roadmap".

---

## 6. Implications for pricing

1. **A bare modeler cannot carry $5k–$10k.** Onshape Professional at $2,500 includes PDM,
   simulation, CAM, and collaboration; CATIA Essential at $7,229 includes assemblies, drafting,
   sheet metal, and platform collaboration.
2. **The $5k–$10k price is a bundle price.** It is defensible only as CAD + PDM/release control +
   SSO/RBAC/audit + support + a validated vertical workflow (for example, sheet-metal fabrication).
3. **Start below the band to earn the right to charge it.** A realistic first commercial price is
   $2,500–$4,500 per seat/year for the design seat, with a $7,500–$9,500 team bundle once PDM,
   review, SSO, audit, and support exist. Enterprise/on-prem and validated vertical packages can
   exceed $10k.
4. **Do not sell "world number one".** Sell a measurable outcome: time from customer specification
   to a manufacturable package, migration fidelity, offline reliability, and support response.

---

## 7. Sources

- Dassault Systèmes, [CATIA price buying guide](https://www.3ds.com/store/what-price-catia), read
  2026-10-25.
- Onshape, [plans and pricing](https://www.onshape.com/en/pricing), read 2026-10-25.
- Alibre, [pricing and licensing](https://www.alibre.com/buy-now/), read 2026-10-25.
- Dassault Systèmes, [Design Engineering / 3DEXPERIENCE PLM Express](https://www.3ds.com/products/plm-express/design)
  (plan comparison; vendor page).
- Onshape, [import/export guide](https://www.onshape.com/en/resource-center/tech-tips/how-to-import-and-export-with-onshape)
  (10 Mar 2026) and [enterprise FAQ](https://www.onshape.com/en/enterprise/faq).
- Onshape, [performance and system requirements](https://www.onshape.com/en/blog/system-requirements-performance-compatibility).
- Tech-Clarity, [PDM buyer's guide](https://tech-clarity.com/pdm-buyers-guide/9009) (29 May 2020;
  used for the enduring procurement framework, not current product claims).
- Siemens, [Parasolid](https://www.siemens.com/en-us/products/plm-components/parasolid/) and
  [D-Cubed](https://www.siemens.com/en-us/products/plm-components/d-cubed/).
- Tech Soft 3D, [Parasolid](https://www.techsoft3d.com/developers/products/parasolid/).
- Open CASCADE, [OCCT public licence](https://occt3d.com/dev/doc/overview/html/occt_public_license.html);
  [STEP](https://occt3d.com/dev/doc/overview/html/occt_user_guides__step.html),
  [IGES](https://occt3d.com/dev/doc/overview/html/occt_user_guides__iges.html) and
  [XDE](https://occt3d.com/dev/doc/overview/html/occt_user_guides__xde.html).
- Truck, [repository](https://github.com/ricosjp/truck) and
  [shapeops crate](https://docs.rs/truck-shapeops/latest/truck_shapeops/).
- SolveSpace, [features](https://solvespace.com/features.pl) and
  [library/licensing](https://solvespace.com/library.pl).
- Repository evidence: [competitor analysis](competitor-analysis.md),
  [feature comparison](feature-comparison.md), [user pain points](user-pain-points.md),
  [feature matrix](../feature-matrix.md), [kernel comparison](../kernel-comparison.md),
  and the parent checkouts under `apps/`.

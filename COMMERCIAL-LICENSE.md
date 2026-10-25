# Commercial licensing

**Status:** the repository owner authorized preparation of a commercial licensing path on
2026-10-25. This document records the intended model and the obligations that must be settled before
a commercial sale. **It is not itself a licence grant, an order form, or legal advice.** Until a
signed commercial agreement exists, the non-commercial terms in [`LICENSE`](LICENSE) apply.

## Dual-licence model

| Use | Terms |
|---|---|
| Personal, educational, research, charitable, other non-commercial use | [`LICENSE`](LICENSE) |
| Commercial use, paid hosting, distribution in a commercial product, or use in a business's operations | Separate written commercial agreement with the rights holder |

The commercial agreement must be signed by the rights holder and the customer. A public price list,
a pilot agreement, or a purchase order does not replace it.

## What a commercial licence covers

- Original AtlasCAD materials in `crates/` and AtlasCAD-authored root documentation.
- The right to use, reproduce, distribute, and support the covered software in the customer's
  commercial activity, within the scope of the signed agreement.
- The support, update, security, and data-processing commitments that the agreement states. None is
  implied by this page.

## What it does not cover

- The imported trees under `apps/`, `kernels/`, and `references/`. They keep their own licences and
  notices. In particular:
  - `apps/solidcraft` is **MIT OR Apache-2.0**;
  - `apps/cadcraft` is **MIT OR Apache-2.0**;
  - `acadrust` (DWG) is **MPL-2.0**;
  - any OpenCASCADE/OCCT use is **LGPL-2.1 with the OCCT exception** and requires legal review;
  - GPL-3.0 material (for example SolveSpace or OpenCADStudio) must not enter a proprietary binary
    without a separate licensing decision.
- Third-party dependencies, assets, fonts, and trademarks. Their notices and attribution must be
  preserved in the distribution.
- Any trademark, service mark, or product name. A trademark search has **not** been completed, and
  no name availability is claimed.

## Intended commercial packaging (indicative, not an offer)

The owner selected a signed-desktop-first product with an entry price below the CATIA/Onshape
professional band:

| Package | Indicative annual price (USD, excluding tax) | Intended contents |
|---|---:|---|
| **Atlas Design** | **$2,400–$4,800 per seat** | Local CAD seat, standard support, updates |
| **Atlas Team** | **$7,500–$9,500 per seat** | Design plus cloud PDM/review, SSO, RBAC, audit, priority support |
| **Atlas Enterprise** | **From $12,000 per seat** | On-prem/private deployment, migration services, custom SLA, validated vertical templates |
| **Design-partner pilot** | **$0–$2,500 per seat for 90 days** | Time-boxed pilot with agreed success measures |

These figures are a planning hypothesis. They are not a quote, a commitment to include any feature,
or a guarantee that a package will be offered. A customer quote must state the exact SKU, term,
support level, data location, and third-party components.

## Terms that must exist before sale

1. A commercial EULA and order form.
2. A privacy policy and, for cloud processing, a data-processing agreement and subprocessor list.
3. A support policy with channels, severity definitions, response targets, and maintenance windows.
4. A refund, cancellation, and renewal policy, including what continues to work offline after
   cancellation.
5. Export-control and sanctions review; encryption/export notifications where applicable.
6. A security package: architecture, threat model, data flows, backup/restore, incident response,
   vulnerability disclosure, and current audit evidence (if any).
7. Third-party notices, SBOM, and source-offer obligations.
8. Trademark and domain clearance for the product name in the relevant classes and territories.

## Current blockers

The product is not yet sellable. The commercial readiness record lists the missing table stakes —
drawings, PDM, SSO/RBAC/audit, signed installers, crash recovery, support, and security evidence.

See [`docs/business/commercial-readiness.md`](docs/business/commercial-readiness.md) and
[`docs/research/enterprise-cad-market-2026.md`](docs/research/enterprise-cad-market-2026.md).

## Contact

Commercial licensing enquiries go to the repository owner. No public sales email, legal entity,
or contracting address is configured in this repository yet; those must be added before a customer
can sign.

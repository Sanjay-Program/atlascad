# Production-readiness assessment

**Date:** 2026-10-29 · **Scope:** the deployable artifacts in this repository — the root workspace
(the `cad-studio` server, the `atlas` CLI, `atlas-eng`, and the libraries they link) — measured on
this host against what a paying professional customer expects at a **$10,000–$50,000 per seat per
year** price point. The imported applications (`apps/`, `kernels/`, `references/`) are out of scope
for this assessment because they are not wired to the contract and are not part of the deployable
product.

**Method:** release build; measured startup, request latency, memory, throughput and output;
reproduction of the documented kernel limits; deliberate crash and corruption tests; a review of the
existing records (`docs/kernel-comparison.md`, `docs/security-review.md`, `docs/feature-matrix.md`,
`docs/quality/baseline-tests.md`). This is an engineering assessment, not a certification or a
penetration test, and it says so.

---

## 1. Verdict

**The product is not worth $10,000–$50,000 per year today, and no single change makes it so.**

What exists is a small, unusually honest local CAD application: one body, one sketch family,
extrude/cut/revolve, a per-feature rebuild cache, save/reopen, undo/redo, STL export, a loopback HTTP
API with real origin and size checks, and a test suite that measures analytic volumes instead of
comparing the program to itself. That is a credible **prototype or a free/inexpensive niche tool**.
It is not an enterprise CAD seat.

The gap is not polish. It is in the three things a paying professional buys:

1. **Geometry that survives real parts.** The booleans fail on ordinary modelling, measured in this
   cycle: a union of a body with a fully contained solid returns no result. The repository's own
   corpus records tangent contact, coordinates near 1e6 mm and the raw coplanar cut as failures, and
   three-operand unions panic inside truck (caught, reported, and then unavailable).
2. **Interoperability.** There is no STEP or IGES import or export anywhere in the platform — only
   binary STL out. A tool that cannot read the customer's STEP files and cannot write STEP back does
   not enter a professional workflow at any price.
3. **Modelling depth.** No sketch constraints or dimensions, no arcs (a drawn polyline only), no
   fillets, chamfers, shells, drafts, patterns, mirror or split, no multi-body, no assemblies, no
   drawings, no mass properties.

Add commercial scaffolding that does not exist yet — signed installers and an update channel, a
licence/billing mechanism, support with an SLA, a security programme with advisory scanning and a
SBOM, telemetry and crash-report policy, trademark clearance — and the honest price today is well
below the target.

The good news is that the engineering discipline is the strongest asset in the repository: refusals
instead of wrong solids, typed errors, atomic saves, panic containment, measured claims. That is
exactly the foundation an enterprise product needs, and it is the reason the program in §6 is
feasible rather than fantasy.

---

## 2. Measured evidence

Host: Linux, 8 cores, 7 GB RAM, rustc 1.95.0. Commands and numbers are reproducible from the
repository; the studio was driven through its real HTTP API and, for UI claims, through headless
Chrome.

| Measurement | Result | How |
|---|---:|---|
| Release build, `cad-studio` + `atlas` (deps warm) | 21.0 s wall, 287 MB peak RSS | `/usr/bin/time -v cargo build --offline --release -p cad-studio -p cad-cli` |
| `cad-studio` binary | 3.4 MB | `ls -l target/release/cad-studio` |
| Startup to first answered `/api/state` | 12.8 ms | fresh release server, first successful request |
| `GET /api/state`, one feature | 0.23 ms median of 20 | release server |
| `POST /api/feature/extrude` (cut, circle hole) | 7.0 ms | release server |
| `POST /api/feature/extrude` (join, overlapping boss) | 9.7 ms | release server |
| `POST /api/parameter` (full rebuild) | 27.1 ms | 4-feature project, one feature failing |
| 256-vertex polyline extrude | 4.9 ms | release server |
| Rebuild of the 256-vertex body | 5.1 ms | parameter change |
| `GET /api/export/stl`, 1 020 triangles | 0.5 ms, 51 KB | release server |
| Server resident memory, 256-vertex body | 7.2 MB | `ps -o rss=` |
| `atlas box --size 10,10,5 --out part.stl` | < 10 ms, 4.5 MB RSS, 684-byte STL, 12 triangles | release CLI |
| Workspace tests | **286 passed, 0 failed** | `cargo test --offline --workspace -j 2` |
| fmt / clippy | clean, 0 warnings | `cargo fmt --all --check`, `cargo clippy --workspace --all-targets -D warnings` |
| Kernel corpus | 8 must-pass ok, 3 recorded known failures, **0 regressions** | `cargo run -p cad-kernel-truck --example corpus` |
| Control-plane validation | 0 errors, 0 warnings | `atlas-eng validate` |
| Full release gate, warm | **6 s**, all checks | `./scripts/gate.sh` |
| Deployable bundle | 3.1 MB tarball, verified by extraction + checksums + smoke tests | `./scripts/package.sh` |
| Dependency licence audit | 83 packages, 83 allowed, 0 denied, 0 unknown | `atlas-eng licences` |
| CycloneDX SBOM | 83 components, byte-identical across runs | `atlas-eng sbom --write` |
| Live simulation | **56 assertions**: 31 over HTTP (cut, join, undo/redo, rebuild, save, restart, STL, `kill -9` recovery), 21 driving the shipped `atlas`/`atlas-eng` on real files, 4 on the shared-session token boundary | `./scripts/simulate.sh` |
| Live browser smoke check | **8 assertions** loopback + **9** shared (the Share panel), 0 console errors | `node scripts/browser-check.mjs <url>` |

**What this says:** the performance and memory story is genuinely good for parts of a few hundred
faces, and the error discipline is real. Neither is the problem.

### 2.1 Failures reproduced in this cycle

| Case | Result |
|---|---|
| Union of a 40×30×10 plate with a boss **fully inside** it | `join failed: … union produced no result; the operands may be tangent, coincident, or nearly degenerate` — the feature is marked failed and export is refused, which is honest, but the modelling operation is unavailable |
| Crash (`kill -9`) after an unsaved parameter edit | **24 000 mm³ lost, reopened at 12 000 mm³** before the fix in §5; now recovered from a snapshot, verified in the API and in the browser |
| Truncated (half-written) project file | refused with `not valid JSON: EOF while parsing a value`, exit 1 — correct, and now a snapshot can recover the edit if one exists |
| Coplanar cut at the raw kernel | recorded known failure; the adapter works around it with a ~1e-4 mm tool nudge, and the bound is tested |
| Three-operand boolean chain | recorded failure: the third union panics inside truck (`intersection_curve.rs:172`), caught and reported |

### 2.2 What was not verified here

* The GPU viewport beyond headless Chrome with software rendering; no framerate claim.
* `apps/cadcraft` and `apps/solidcraft` builds (host memory); they are not in the deployable artifact.
* STEP/IGES — there is nothing to verify: the capability enum names them, no code implements them.
* Multi-user, hosted, SSO, audit, TLS — not implemented.
* Installers, code signing, auto-update — not implemented.
* Advisory scanning (`cargo-audit`/`cargo-deny` not installed; no `deny.toml`, no CI configuration).

---

## 3. Functionality against what a professional buys

| Requirement at the $10k–50k/yr bar | Today | Evidence |
|---|---|---|
| Reliable B-rep booleans on real parts | **Fails**: contained union, tangent contact, 3+ chains, large coordinates | §2.1, `docs/kernel-comparison.md` §4 |
| STEP AP203/214/242 import **and** export | **Absent** | no implementation in `crates/` |
| IGES / DXF / native-neutral interchange | **Absent** | same |
| Constraint-driven sketches with dimensions | **Absent**: rectangle, circle, regular polygon, drawn polyline; parameters are document-level | `docs/feature-matrix.md` §3c |
| Fillets, chamfers, shells, drafts | **Absent** | `Capability` enum lists them; the adapter advertises none |
| Patterns, mirror, split, combine | **Absent** | same |
| Multi-body parts | **Absent**: a second `new_body` is refused by design | `cad-document` |
| Assemblies, mates, interference | **Absent** | — |
| Drawings, GD&T, BOM | **Absent** | — |
| Mass properties from exact geometry | **Absent** (mesh volume only) | `Capability::MassProperties` unimplemented |
| Large-model performance (10k+ faces) | **Unmeasured**; no tessellation resource budget | `docs/security-review.md` §3 |
| Crash safety / recovery | **Delivered this cycle** for unsaved edits and unreadable files | §5 |
| Named revisions / version history | **Absent** (M11f remainder) | `docs/tasks.md` |
| Signed installers, update channel | **Partial**: a checksummed tarball is built and re-verified by extraction; there is no installer, code signing, notarisation or update channel | `./scripts/package.sh`, `docs/operations/release-process.md` §6 |
| Licence management / billing | **Absent**: `cad-entitlements` is local policy, no provider | `docs/feature-matrix.md` |
| Authentication, tenancy, audit log | **Absent**: loopback only, origin checks, no token | `docs/security-review.md` S-4 |
| SBOM, advisory scanning, CI gate | **Partial**: the gate, licence policy and deterministic SBOM are delivered and pass locally; CI config and `deny.toml` are added but have never run on a runner, and advisories are not scanned on this host | §5, `docs/operations/release-process.md` |
| Support SLA, documentation, training | **Absent** | — |
| Vendor viability, trademark, commercial terms | **Draft only**: `COMMERCIAL-LICENSE.md` is not a grant; no trademark search | `COMMERCIAL-LICENSE.md` |

---

## 4. Findings, in severity order

**F-1 (Critical) — Geometry fails on ordinary parts.** The booleans are the product's core, and they
are the weakest part. A contained union returns no result; chains of three or more fail; tangent
contact fails; coordinates near 1e6 mm fail; the raw coplanar cut fails (worked around with a nudge).
A professional cannot model a bracket with a pocket and a boss without hitting one of these.
*Consequence:* unusable for paid work, and the worst kind of support load — the failure is correct but
the operation is missing.

**F-2 (Critical) — No neutral-format interoperability.** STL out only. No STEP import/export, no
IGES, no DXF. A customer's existing parts, suppliers and downstream CAM/PLM all speak STEP.
*Consequence:* the tool cannot enter or leave a professional workflow.

**F-3 (Critical) — No constraint-driven sketching.** Profiles are driven by four document parameters
and a drawn polyline. There are no dimensions, no relations, no arcs (except by drawing many points),
no construction geometry. Every professional part starts with a constrained sketch.
*Consequence:* parts cannot be authored to intent, only to numbers.

**F-4 (High) — No finishing operations or multi-body/assembly.** No fillet, chamfer, shell, draft,
pattern, mirror, split, multi-body, assembly, drawing or exact mass properties.
*Consequence:* even a correct solid cannot become a manufacturable part or a drawing.

**F-5 (High) — The product runs on the thin truck adapter, not the hardened kernel.** The
repository's own analysis says the published truck crates are insufficient and that SolidCraft's
kernel layer is what makes them usable, and the contract has not been implemented for that kernel
(M1). The corpus has never been run against it.
*Consequence:* the geometry ceiling is known in advance and is below the bar.

**F-6 (Medium) — No delivery or quality gate.** No CI configuration in the root workspace, no
`deny.toml`, `cargo-audit`/`cargo-deny` not installed. Every gate in this document was run by hand.
*Consequence:* a regression can ship; supply-chain advisories are not watched; a procurement security
questionnaire cannot be answered.

**F-7 (Medium) — Performance is measured only on small models.** 279 tests and the measurements above
cover parts with hundreds of faces. There is no large-model benchmark, no tessellation resource
budget, and no memory ceiling per document.
*Consequence:* unknown behaviour exactly where enterprise buyers test first.

**F-8 (Medium) — Single-user loopback only.** Origin checks stop cross-site requests, but any local
process can drive the server; no token, TLS, accounts, tenancy or audit log.
*Consequence:* fine as a desktop trust model; disqualifying for hosted or multi-user procurement.

**F-9 (Medium) — Commercial scaffolding absent.** No signed installers or update channel, no licence
enforcement or billing, no support process or SLA, no crash-report/telemetry policy, no trademark
clearance, and `COMMERCIAL-LICENSE.md` says itself that it is not a grant.
*Consequence:* nothing can be sold, invoiced or supported as it stands.

**F-10 (Low) — Contract API polish.** The public capability enum spells IGES as
`IgfesImport`/`IgfesExport`, and the contract has no written compatibility policy beyond its
version constant.
*Consequence:* small, but a paid customer's integrator reads this API.

---

## 5. Delivered in this cycle

**Crash-safe recovery for unsaved edits** (the measured data-loss defect in §2.1):

* `cad-document` writes an unsaved-edit snapshot next to the project (`<file>.recovery`) through the
  same atomic temp-file + fsync + rename path as a save, and never writes the project file itself.
  The snapshot records the file version it was taken from, so staleness is decided by file version,
  not by a clock.
* `open_with_recovery` prefers the snapshot when the file has not changed since, falls back to it when
  the project file cannot be read, deletes a superseded snapshot, and ignores a corrupt one.
* `cad-studio` autosaves after every successful mutation, reports `recovery` in `/api/state`, marks a
  recovered session `dirty`, deletes the snapshot on save, and exposes `POST /api/recovery/discard`.
  A failed snapshot write is surfaced in the payload instead of being swallowed.
* The UI shows a recovery banner with the time of the recovered edit and a discard action; the banner
  is absent when there is nothing to recover.

Verified: 4 new document tests (offered, stale-and-deleted, corrupt-ignored, unreadable-file fallback),
2 new HTTP tests (crash → recover → save; recover → discard), a real `kill -9` on the release binary
(12 000 → 24 000 → recovered 24 000), and a headless-Chrome run (banner text, discard, badge, zero
console errors). Workspace **279 tests, 0 failed**, fmt and clippy clean.

**Also delivered in this cycle — the delivery gate:**

* `scripts/gate.sh` runs the whole quality claim in one command: fmt, clippy with warnings denied, the
  workspace tests, the analytic corpus, manifest validation, the licence policy, the SBOM check and the
  live simulation (HTTP, both CLIs, and the browser when one is installed). `--quick` skips only the simulation.
* `atlas-eng licences` checks every dependency in the resolved graph against
  `engineering/policy/licences.json`, **offline**, and fails closed: an unrecognised licence is a
  failure, not a warning, and banned kernels are refused whatever their licence says. 83 packages, 83
  allowed.
* `atlas-eng sbom` emits a deterministic CycloneDX 1.5 SBOM (83 components, `pkg:cargo/…` purls,
  licence expressions and verdicts) that is byte-identical across runs.
* `scripts/package.sh` builds, stages and archives a deployable bundle
  (`atlas-cad-0.1.0-x86_64-unknown-linux-gnu.tar.gz`, 3.1 MB) with the three binaries, the licence and
  quality documents, the SBOM and `SHA256SUMS`, then extracts it into a clean directory, verifies the
  checksums and smoke-tests the packaged binaries.
* **LAN sharing, token-gated.** `--share` binds the local interface and every `/api/*` call must
  present a 128-bit session token (header or query); the page shows a **Share** panel with the link,
  and loopback use is unchanged. Tokens are compared without an early-exit, the page sends
  `Referrer-Policy: no-referrer`, and a shared write still has to be same-origin. This is one document
  shared live, not multi-user editing: last write wins, and stopping the process revokes the link.
* `scripts/simulate.py` drives the **real release server** through cut, join, undo/redo, a parameter
  rebuild, save, restart, STL export and a `kill -9` recovery (31 analytic assertions), the shipped
  `atlas` and `atlas-eng` binaries on real files (21 assertions: box and union STL volumes, a headless
  project edit and undo, manifest validation, the licence policy and a byte-identical SBOM), and a
  shared session's token boundary (4 assertions).
  `scripts/browser-check.mjs` loads the live page in headless Chrome loopback and shared (8 + 9
  assertions, zero console errors).
* `.github/workflows/ci.yml` and `deny.toml` add the advisory and provenance job. **The workflow has
  not been executed on a runner**; only the gate it calls was run here.

**Not** delivered: named revisions (M11f remainder), signing/notarisation/installers, an update
channel, a support process, and every Critical/High product finding above.

---

## 6. What would make it worth the price

Ordered by dependency and by what unblocks a sale. Sizes are rough and assume one experienced
engineer with this repository's existing discipline.

### P0 — Trustworthy delivery (2–4 weeks)

1. **Delivered:** the gate (fmt, clippy `-D warnings`, tests, corpus, manifests, licence policy, SBOM,
   live simulation) and a verified bundle. **Outstanding:** the CI workflow has not run on a runner,
   and advisories are not scanned on this host.
2. **Partial:** deterministic tarball with checksums and smoke tests. **Outstanding:** installers for
   Linux/macOS/Windows, code signing, notarisation, a versioned update channel.
3. **Partial:** crash recovery for unsaved edits and unreadable files is delivered. **Outstanding:**
   named revisions and retention of the last good on-disk revision.
4. **Outstanding:** crash-report and log policy that never records geometry or project text (S-7).
5. **Outstanding:** signed agreement template, support policy, trademark search, price sheet.

**Exit:** a clean clone builds and gates in CI; an installer is signed and updatable; a crash cannot
lose an edit; the legal and support paperwork exists.

### P1 — Interoperability (6–10 weeks)

1. STEP AP214 export from the kernel's B-rep, through the existing vendored `truck-stepio`.
2. STEP import into a body the document can edit, with import budgets (already implemented in
   SolidCraft's IO for its readers).
3. A round-trip oracle: export → import → compare volume, area, bounding box and shell count on a
   corpus of at least 50 representative parts; publish the table.
4. IGES and DXF as follow-ups, gated the same way.

**Exit:** a customer's STEP part opens, edits, and exports with measured round-trip fidelity, or the
tool refuses with a reason.

### P2 — Geometry (10–16 weeks)

1. Implement `cad_contract::Kernel` for `solidcraft-kernel` behind a feature flag (M1) and run both
   kernels against the corpus, per operation, in the report.
2. Close the boolean gaps or refuse them explicitly: contained unions, tangent contact, three-operand
   chains, large coordinates, coplanar cuts without the nudge.
3. Fillets and chamfers on the kernel that wins, then shells and drafts.
4. Tessellation resource budgets and a 10k-face benchmark with a published latency and memory target.

**Exit:** zero wrong solids on a 500-fixture corpus; at least five-operand boolean chains; a housing
with 20 fillets edits in under 200 ms.

### P3 — Modelling depth (12–24 weeks)

1. Constraint sketch: lines, arcs, circles, dimensions, relations, solver with a stated convergence
   policy.
2. Patterns, mirror, split; multi-body parts.
3. Assemblies with mates and interference checks.
4. Drawings: views, dimensions, BOM; exact mass properties.

**Exit:** a representative assembly (3 parts, 40+ features, patterns and fillets) is authored, drawn
and exported to STEP in under an hour by a trained user.

### P4 — Enterprise (20–32 weeks)

Multi-user or hosted operation with SSO and an audit log; PLM/PDM connectors; large-assembly
performance; a security programme (SBOM, advisory monitoring, penetration test, data-handling
statement); support SLAs and training.

**Exit:** a procurement security questionnaire can be answered with evidence, and three concurrent
users work on the same project without corruption.

### The price gate

Charge $10,000–$50,000 per seat only when all of the following are true *and measured*:

* a professional benchmark part models correctly, including pockets, bosses, fillets and patterns;
* STEP round-trip is lossless on a published 50-part corpus;
* no operation returns a wrong solid — it succeeds or refuses;
* an edit on a 10k-face part completes in under 200 ms and a crash cannot lose work;
* drawings and BOM export, and the part can be authored with constraints;
* three users can collaborate, with an audit trail;
* an installer is signed, updates, and the vendor can answer a security questionnaire.

Until then, price it as a **free or low-cost niche tool** and say exactly what it cannot do. The
repository already does that better than most; the mistake would be to charge enterprise money for
it before P1 and P2 are real.

---

## 7. Recommended first step

Start with **P0**, because it is cheap, it is entirely inside this repository, and it converts the
existing honesty into something a customer can rely on. Then **P1 (STEP)** before **P2**, because
interoperability is what turns a geometry engine into a tool a professional can adopt, and because a
STEP round-trip oracle gives P2 a way to prove correctness instead of asserting it.

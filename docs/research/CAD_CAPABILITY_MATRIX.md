# CAD Capability Matrix

**As of:** 2026-10-30. **Purpose:** one place that separates (a) what a reference product's own
documentation says it does, (b) what AtlasCAD actually does — verified by a recorded test — and (c) what
is merely proposed. A row is not evidence that AtlasCAD supports a feature.

**How to read the columns**

| Column | Meaning |
|---|---|
| Reference behaviour | What the named product does, as its vendor/project documents it. **Vendor claims are not AtlasCAD claims, and were not re-tested here.** |
| Atlas status | `Verified` = exercised by a recorded test/measurement on this host; `Partial` = implemented for a subset or with a recorded defect; `Absent`; `Proposed` = designed only. |
| Difficulty | S ≤ 1 day, M ≤ 1 week, L > 1 week, XL > 1 month for one engineer, *for this codebase*. |
| Required tests | What would have to pass before the status may move to `Verified`. |
| Licensing | What a lawful implementation would have to respect. |

**Source status.** Competitor facts for AutoCAD, Fusion, SOLIDWORKS, Onshape, FreeCAD and SolveSpace
are the ones recorded in [`feature-comparison.md`](feature-comparison.md) and
[`competitor-analysis.md`](competitor-analysis.md) from those vendors' own pages on 2026-10-23/25. They
were **not re-fetched on 2026-10-30**; treat their prices and version details as of that date. The KiCad
entry was checked on 2026-10-30 against `dev-docs.kicad.org` ("KiCad IPC API", last modified 2026-09,
which documents a stable multi-language API from KiCad 9.0). The IFC entry records the standard's
identity only: IFC 4.3 is published under ISO 16739-1, and the *edition year is deliberately not quoted*
here because ISO's catalogue entry for 16739-1:2018 describes IFC4 while secondary sources cite a 2024
revision — verify before quoting a year. BIM, GIS, CAM, CAE and plant rows describe the **workflow
requirements** of those domains; they are not claims about a specific product's current feature set.

---

## 1. General drafting and documentation

| Capability | Reference behaviour | Atlas status | Deps | Difficulty | Required tests | Licensing | Perf impact | Priority / limits |
|---|---|---|---|---|---|---|---|---|
| 2D primitives: line, polyline, arc, circle, ellipse, spline, point, construction geometry | AutoCAD: the core drafting set, with true arcs/splines and construction lines; FreeCAD's Draft workbench offers an open-source equivalent (`autodesk.com/products/autocad/features`, FreeCAD docs) | **Absent in the platform document**; present only inside the unbuilt `apps/cadcraft` tree, whose geometry crate passed 84 tests on 2026-10-21 | A 2D entity model in `cad-document`; an exact-arc contract operation, not a polygon | L | Analytic entity tests (tangency, trimming, self-intersection), save/load, 10k-entity selection timing | Own code; no vendor format needed | Viewport triangles and hit-testing grow with entity count; unmeasured | High for general use, but not next: the platform's 3D document has no 2D layer at all |
| Hatch, fills, text/MTEXT, leaders, dimensions, blocks/attributes, Xrefs | AutoCAD: mature since R12-era drafting; dimensions are associative and style-driven (vendor feature pages) | **Absent in the platform document** | Text shaping, font subsetting, dimension styles | L–XL | Layout/annotation round-trip, font fallback, dimension associativity | Bundled fonts and any font files need their own licences | Text shaping dominates redraw on large sheets | High, but only after the 2D entity model exists |
| Modify operations: move/copy/rotate/scale/mirror/array/offset/trim/extend/fillet/chamfer/stretch/break/join | AutoCAD and FreeCAD both ship the full modify set | **Absent** (no 2D entity graph to modify) | 2D entity model, robust curve intersection | L | Property-based tests on random geometry; degenerate-input tests | Own code | Interactive | High |
| Constraints and dimensions (2D sketch solver) | SolveSpace: an explicit DOF-counted solver; FreeCAD/SolidWorks/Fusion: sketch constraints with driving dimensions | **Absent.** Named parameters drive sizes; there is no solver and no DOF report | A constraint solver with a verified convergence policy | XL | Solver correctness on known-over/under-constrained sketches; failure must name the conflict | Own code; SolveSpace is GPLv3 and must not be copied | Solve time on every edit; must stay well under 16 ms for interactive sketches | High, and a hard dependency for real mechanical work |
| Layouts, paper space, viewports, page setup, line weights, PDF plot, batch plot | AutoCAD: layouts, viewports, plot styles, sheet sets (vendor docs) | **Absent.** There is no drawing/sheet document, only the model | A print/document pipeline; a PDF writer | XL | Golden-image or geometry-level PDF comparison; scale accuracy | Any PDF library licence | Plotting is offline from the viewport | Medium–high; required for "drawings", not for modelling |
| Drawing comparison / change highlighting | AutoCAD: DWG Compare highlights added/removed geometry | **Absent** | Two-document diff at entity level | L | Diff fixtures with moved/edited/deleted entities | Own code | Diff is a batch operation | Medium; enterprise-relevant |

## 2. Parametric 3D

| Capability | Reference behaviour | Atlas status | Deps | Difficulty | Required tests | Licensing | Perf impact | Priority / limits |
|---|---|---|---|---|---|---|---|---|
| Ordered feature history with edit/recompute | SOLIDWORKS, Fusion, Onshape, FreeCAD: feature tree, suppressed features, rebuild errors | **Verified**, linear and single-body: `.zcad` schema 8, ordered sketches/features, per-prefix `BuildCache`, a failed feature named by ID with the last good mesh kept | — | — | Already covered by `t-platform-workspace-20261030` | Own code | Cached rebuild 0.01 ms unchanged, 7.9 ms for the last of six features (release) | Done for the current scope |
| Sketch → extrude (new body / join / cut) and sketch plane placement | All mainstream parametric modellers sketch on a face or plane and extrude with a direction | **Verified** for horizontal planes: pocket on a raised plane, boss on a top face, downward through-hole all match analytic volumes over HTTP; `JOIN_OVERLAP_MM` handles a face-coincident join | `Transform` capability | — | — | Own code | +2.1 ms over a z = 0 join (`docs/performance-baseline.md` §11) | Done; tilted/face-aligned planes and multiple bodies remain absent |
| Full-turn revolve | Fusion/Onshape/SOLIDWORKS: revolve with angle, axis and cut/join modes | **Verified** for a full turn about a sketch axis; partial turns refused rather than returned as an open shell | `Revolve` capability | M (partial turns) | Partial-turn solid tests, axis-straddling profiles | Own code | Revolve is not benchmarked | Medium; partial revolve needs shells, which the document does not carry |
| Boolean union / cut / intersection | Universal | **Verified** for union and difference through the corpora and the app; intersection verified at the adapter level; **known failure** for coplanar operands without the nudge, and boolean chains of 3+ operands | Kernel boolean engine | XL (chain robustness) | The analytic corpus must stay at 0 regressions | Vendored patched truck (Apache-2.0) with recorded provenance | 1.5–4.8 ms per two-feature rebuild | Highest-value kernel work is chain and coplanar robustness |
| Sweep and loft | Fusion/Onshape/SOLIDWORKS/FreeCAD Part: sweep along a path, loft between profiles | **Absent.** `Capability::Sweep`/`Loft` exist in the contract but no adapter advertises them | Kernel sweep/loft with a path or section list | L | Analytic swept-volume tests, self-intersection refusal | Own code / truck capabilities | Unknown | Medium |
| Fillet / chamfer / shell / draft / holes / threads | All mechanical CAD; SolidCraft's own kernel has them (read from source, not run here) | **Absent through the contract.** `Capability::Fillet`/`Chamfer`/`Shell` are declared and unadvertised — a feature that needs one is refused with a reason | Truck or SolidCraft kernel support; new capability methods | XL | Edge-selection stability, tangent propagation, analytic volume checks | Own code | Fillet tessellation is expensive on curved faces | High for mechanical credibility |
| Linear/circular patterns, mirror of features | Universal in mechanical CAD | **Absent.** A raised sketch plane now makes them expressible for the first time | Feature-level replay, not body replay | M | Pattern count/spacing analytic volumes, disjoint and overlapping cases | Own code | N × union cost; the cache key must include the pattern | **Next**, and now unblocked |
| Reference planes/axes, datum geometry | Universal | **Partial**: a sketch has an XY position and a z height; there is no datum plane, axis or coordinate system object | A datum entity in the document | M | Datum persistence and reference integrity | Own code | Negligible | Medium |
| Multiple bodies / components and assemblies with mates | SOLIDWORKS/Fusion/Onshape: components, mates/joints, interference, exploded views | **Absent.** A second `new_body` is refused by design | Multi-body document, transforms per body, mate solver | XL | Assembly placement, mate solving, interference | Own code | Assembly rebuild is the dominant cost in real CAD | Very high, but a document-model change |
| Mass properties, interference detection | SOLIDWORKS/Fusion: mass, centre of gravity, interference reports | **Absent as a product feature.** A volume is measured from the tessellation; surface area and bounding box are in the state payload. No centre of mass, no material/mass model, no interference | Exact mass properties or a validated mesh integration; material table | M | Analytic cube/cylinder mass properties; interference fixture with a known overlap | Own code | Cheap for volume, more for exact integrals | Medium–high; `Capability::MassProperties` exists but is unadvertised |
| Configurations / design variants / design tables | SOLIDWORKS: configurations; Fusion: configurations; Onshape: configurations | **Absent.** Parameters are a single flat map | Parameter sets, configuration switching, rebuild scope | L | Switching a configuration restores its own parameter set and geometry | Own code | One rebuild per switch | Medium |

## 3. Interoperability

| Capability | Reference behaviour | Atlas status | Deps | Difficulty | Required tests | Licensing | Perf impact | Priority / limits |
|---|---|---|---|---|---|---|---|---|
| Native `.zcad` save/load + migration | — (Atlas-specific) | **Verified**: atomic save (temp file, rename), schema 1…7 migrated on load, newer schema refused, recovery snapshot and last-good backup | — | — | Already covered | Own code | Milliseconds for the fixtures | Done |
| Binary STL export/import | Universal | **Verified export** with a validated, watertight mesh; **import**: `cad-io` can read binary STL, no application importer is wired | An importer + a mesh-to-solid policy | M | Round-trip volume and manifold checks; refusal of non-manifold meshes | `cad-io` is own code | Linear in triangle count | Medium |
| DXF import/export | AutoCAD's native interchange; FreeCAD/SolveSpace export DXF | **Absent in the platform path**; a DXF implementation exists in `apps/cadcraft` (unbuilt here, no recorded test result) | A 2D entity model to map onto | L | Layer/unit/block/dimension round-trip fixtures, R12 vs R2018 | Own code; do not reuse a copyleft reader | Parsing large drawings | High for drafting adoption |
| DWG | AutoCAD's format | **Absent.** A third-party reader (`acadrust`, MPL-2.0) exists inside `apps/cadcraft`, used unmodified, with no corpus | A lawful, tested DWG corpus | XL | A representative DWG corpus, read and written, with measured fidelity | MPL-2.0 obligations; do **not** claim DWG compatibility without a corpus | Substantial | Blocked on a corpus and a licence review |
| STEP AP242/AP203 | Fusion/SOLIDWORKS/Onshape/FreeCAD: STEP is the neutral solid-exchange format | **Absent.** `truck-stepio` is available; nothing is wired. The contract declares `StepImport`/`StepExport`, unadvertised | B-rep mapping to/from the contract | L | Analytic STEP round-trip (volume, area, closed shells), tolerance and units | `truck-stepio` (Apache-2.0) with recorded provenance | Import dominates startup | High; the obvious next interop step after STL |
| IGES, 3MF, OBJ, glTF | FreeCAD and SolidCraft both list these | **Absent in the platform path**; SolidCraft's `crates/io` has them (not run here) | Mesh exporters first (OBJ/3MF/glTF are cheap from the validated mesh) | M for mesh formats, L for IGES | Round-trip and information-loss reporting | Own code / permissive libraries | Linear in triangles | Medium |
| IFC (BIM) | Revit/ArchiCAD and the openBIM ecosystem exchange IFC (published as ISO 16739-1; 4.3 is the current release family) | **Absent.** No BIM object model exists | A building object model (walls, slabs, openings, spaces) + an IFC mapping with MVDs | XL | Entity-level round-trip, property sets, unsupported-entity reporting | IFC schemas are published for implementation; no code may be copied | Large models are memory-bound | Very high for the architecture workspace, and nothing may be claimed before the object model exists |
| GeoJSON / GeoPackage / Shapefile / WMS | QGIS and GIS CAD tools | **Absent.** No CRS type, no projection library, no spatial index in the platform | A CRS-aware geometry layer | L | Coordinate round-trip with a named EPSG code and a measured error bound | PROJ (MIT) is the lawful choice; **do not hand-roll projections** | Must keep precision explicit | Medium; needs its own workspace |
| PDF output | AutoCAD/Fusion drawings, FreeCAD TechDraw | **Absent** | A sheet/layout model | L | Geometric PDF comparison, scale accuracy | PDF library licence review | Offline | Medium |

## 4. Automation, AI and platform

| Capability | Reference behaviour | Atlas status | Deps | Difficulty | Required tests | Licensing | Perf impact | Priority / limits |
|---|---|---|---|---|---|---|---|---|
| Versioned kernel/document contract | — (Atlas-specific) | **Verified**: `cad-contract` capability discovery, error taxonomy, mesh validation, versioned document header, 55 tests | — | — | Already covered | Own code | None | Done |
| Agent API with permissions and budgets | Fusion/Onshape public APIs; SolidCraft's in-app agent | **Verified** for one agent (`agent.cad.geometry-ops`) with grants, budgets, schema enforcement, quarantine; declared-but-unimplemented agents are recorded as such | — | — | Second agent with a different profile; a host outside the test harness | Own code | In-process, synchronous | Medium (M9) |
| Native scripting (Python et al.) | FreeCAD: Python console and macro API; AutoCAD: AutoLISP/Visual LISP; Onshape: FeatureScript | **Absent.** There is no scripting layer, no REPL and no recording of operations | A stable command/serialization layer + an embedded interpreter with sandboxing | XL | Language-binding conformance suite, sandbox escape tests, version pinning | Python (PSF) is embeddable; language **compatibility claims** need their own suites | Interpreter startup and memory | Medium; the CLI + HTTP API are the current automation surface |
| AutoLISP / Visual LISP / VBA / .NET compatibility | AutoCAD-specific ecosystems | **Absent and not claimed.** Exposing an AtlasCAD API is not compatibility | A separate interpreter/COM/CLR host per ecosystem | XL each | Per-ecosystem compatibility corpus | Varies; some are proprietary runtimes | Varies | Not scheduled; a documented subset would be the only honest route |
| MCP / agent interface | Both imported apps ship MCP servers; the agent platform has its own contract | **Absent in the platform applications** (the two imported apps have their own servers, unbuilt here) | An MCP server over the studio API | M | Tool-schema conformance and permission tests | Own code | Negligible | Medium; the brief asks for it "where already present and useful" |
| AI assistance with a typed plan and approval | Fusion/Onshape/SOLIDWORKS ship vendor AI assistants; behaviour and data terms differ per product | **Absent.** No model provider, no prompt pipeline, no credential policy | The scripting/command layer first; then intent parsing, validation and preview | XL | Plan validation, refusal paths, audit records, "no AI" mode | Model providers' terms; never train on customer drawings without consent | Remote inference cost and latency; must be optional | Low until the command layer exists — an AI that cannot execute validated commands is a demo |
| Cloud collaboration and enterprise administration | Onshape: real-time collaboration is core; Fusion/SOLIDWORKS: cloud data management; enterprise tiers add SSO, audit, admin | **Absent.** No accounts, no server, no billing; the studio is local-first with a token-gated LAN share | A multi-user document model (the current one is single-user and snapshot-based) | XL | Conflict merge, authorization, audit, retention | Cloud vendor terms | Server cost, modelled separately | Deferred by ADR and by `docs/tasks.md` §5 |

## 5. Rendering and hardware

| Capability | Reference behaviour | Atlas status | Deps | Difficulty | Required tests | Licensing | Perf impact | Priority / limits |
|---|---|---|---|---|---|---|---|---|
| Interactive 3D viewport | Universal | **Verified**: WebGL 1 in the browser, orbit/pan/zoom/fit, orthographic views, per-face picking, wireframe and grid toggles | — | — | Already browser-driven | Own code | Tessellation size dominates | Done for meshes |
| Shaded/edge display, clipping, sectioning, materials, transparency | Fusion/SOLIDWORKS/Blender-grade display | **Partial**: shaded + wireframe only; no materials, no clipping planes, no sections | Viewport shaders, per-face attributes | M–L | Visual/DOM assertions plus pixel sampling | Own code | Shader work is cheap; transparency needs sorting | Medium |
| High-quality offline rendering | Blender-grade path tracing; Fusion has cloud rendering | **Absent and explicitly out of scope** for normal modelling | A renderer or an external tool integration | XL | Image comparison and timing | Renderer licence | Heavy; requires a GPU or long CPU time | Low; the brief says not to force it |
| Adaptive quality / device profiles | Professional CAD exposes graphics settings and diagnostics | **Absent.** No quality profile, no capability probe, no diagnostic utility; the page falls back with a message when WebGL is unavailable | A probe + a settings model that changes real parameters | M | Profile changes must be observable in tessellation/cache behaviour | Own code | The point of the feature | Medium; see `docs/platform/SYSTEM_REQUIREMENTS.md` |

## 6. What this matrix does not establish

1. It does not compare AtlasCAD favourably with any product: most rows are `Absent`.
2. Vendor feature lists are marketing-level descriptions; they do not prove a workflow is usable, and
   nothing here was benchmarked against a competitor.
3. A `Verified` row means *a recorded test on this host passed*, not that the feature is complete for
   professional use. The limitations column and the linked task records are part of the claim.
4. No proprietary source code, protected interface or product asset was used. Where a candidate library
   is named, its licence and the obligation it creates are recorded first.

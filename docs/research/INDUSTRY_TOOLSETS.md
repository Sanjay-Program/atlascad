# Industry Toolsets — Requirements and Ordering

**As of:** 2026-10-30. **Purpose:** decide which industry workspace AtlasCAD can actually build next, and
what each one costs, *before* any of them is started. **Nothing in this document is implemented.** The
only verified workspace is the general parametric one; see
[`../roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](../roadmap/FEATURE_IMPLEMENTATION_STATUS.md).

## 1. The rule this document follows

> Industry workspaces must reuse shared geometry, selection, document, property and rendering
> infrastructure. Do not build seven independent CAD applications.

Concretely, every workspace below needs four things, in this order:

1. **A shared object model** — a typed entity with stable identity, properties, relationships and a
   lifecycle, stored in the same document as the general geometry. Existing precedent: the sketch
   profile enum and the feature graph. There is no generic *entity with properties* in `cad-document`
   today, only profiles and features.
2. **A shared geometry/placement layer** — the object knows its 3D placement and evaluates to
   geometry the kernel accepts. Today a feature is one profile plus one sweep; a wall is a swept
   profile *plus* opening subtractions with relationships.
3. **A domain validation layer** — connectivity, rules, schedules. This is where a drawing of pipes
   becomes a network, and where "we drew a P&ID" must not become "we validated a process".
4. **A document/report layer** — schedules, take-offs, sheets. None of this exists (no sheet or table
   model).

A workspace that skips step 1 becomes another application. That is the specific failure the brief
warns about, and it is why the ordering below is about shared infrastructure, not about which domain
looks best in a screenshot.

## 2. Dependency graph

```
                        ┌──────────────────────────┐
                        │ general document (done)  │
                        │ sketch · plane · extrude │
                        │ revolve · cut · cache    │
                        └────────────┬─────────────┘
                                     │
             ┌───────────────────────┼────────────────────────┐
             ▼                       ▼                        ▼
   multi-body + placements     patterns/mirror          exact mass properties
   (document model change)     (feature replay)         (Capability::MassProperties)
             │                       │                        │
             ├──────────────► MECHANICAL (parts, assemblies, drawings)
             │                       │
             │                       ├──► SHEET METAL / CAM (needs flat pattern, toolpaths)
             │                       └──► PLANT (needs pipe specs, isometrics)
             ▼
   BIM object model (walls/slabs/openings/spaces) ──► ARCHITECTURE ──► MEP
             │
             └──► IFC mapping (ISO 16739-1 family, MVD-scoped)
   CRS + projection layer ──► GIS ──► (CAD-to-GIS conversion)
   Entity/property model ──► ELECTRICAL (schematic net model; KiCad interop)
   Raster pipeline ──► LEGACY CONVERSION (independent of the above)
```

## 3. The seven workspaces

### A. Architecture / building design (BIM) — **last, not first**

**What a real BIM workflow needs:** walls with types and layers, doors/windows hosted in walls,
floors/slabs/roofs, columns/beams, stairs/railings, rooms with computed areas, levels and grids,
sections/elevations from the model, schedules generated from properties, and IFC exchange with an
explicit Model View Definition.

**Why it is not first:** it needs the largest object model of any workspace (host/opening relationships,
level ownership, type vs instance parameters, room boundaries computed from geometry) *and* a mapping
to a large external schema. Building that before the shared property/relationship model exists would
create exactly the "seven applications" outcome. An IFC import that drops unsupported entities must be
a designed report, not a silent side effect — that is only possible once entities have types.

**Honest scope for a first BIM slice** (once the object model exists): walls as swept profiles on a
level, hosted rectangular openings as subtractions, rooms as polygons with computed area, one schedule
(wall types and lengths), and IFC export of *only* what is mapped, with a recorded unsupported-entity
list. No doors-with-parameters, no stairs, no roof modelling, no clash detection before geometry exists.

**Licensing:** IFC schemas are published for implementation by buildingSMART; no code is copied. Any
IFC library used must have its licence recorded before adoption.

### B. Mechanical design — **the first workspace, and the cheapest**

**Why:** it reuses the most. A part is already a feature history; an assembly is components plus
placements; a drawing is views of a model. The missing pieces are document-model work that benefits
every workspace (multiple bodies, patterns, exact mass properties), not domain-specific modelling.

**First slice, in order:**
1. Multi-body: a feature targets a named body; a project holds a list of bodies. (M–L, document change)
2. Linear and circular patterns, mirror — replay the *feature*, not the body. (M, now unblocked by the
   sketch plane.)
3. Exact mass properties (volume, area, centroid, inertia) behind `Capability::MassProperties`, with
   analytic tests against a box, cylinder and annulus. (M)
4. Standard-part library with a recorded standard number, edition, unit system and source per part —
   and **no "standards compliant" claim** until each part is checked against the published dimensions.
   (M for a fastener set; XL for a real library)
5. Assemblies and mates/joints: placement, degrees of freedom, interference between components. (XL)

**What must not be claimed:** that a gear, bearing or fastener is standards-compliant because its
shape looks right; that a section view or exploded view is a manufacturing drawing.

### C. Electrical / electronics — **borrow the PCB ecosystem**

**What a real workflow needs:** symbols with pins, wires as nets (not lines), junctions, net names,
cross-references, wire numbering, connectivity validation, terminal and cable schedules, panel layout,
and a schematic-to-layout link.

**The important distinction:** a drawing of connected symbols is not a validated circuit. The netlist
is the product; the drawing is a view of it. So the first slice is a **net model** (components, pins,
nets, connectivity rules and diagnostics), then symbols/wires bound to it, then a layout view.

**Ecosystem decision:** do not build a PCB engine. KiCad's IPC API (documented at
`dev-docs.kicad.org` as a stable, multi-language interface from KiCad 9.0, checked 2026-10-30) is the
right integration point for hand-off, because it is an interface we can call rather than a file format
we would half-implement. That integration is **not** started: it needs a running KiCad, a recorded
version, and a documented subset of operations.

**Licensing:** KiCad is GPL-3.0. Calling its IPC API is an interface boundary, not code reuse; any
*bundling* of KiCad code has to be evaluated separately. Nothing here is a legal opinion.

### D. MEP — **needs BIM and mechanical first**

Ducts, pipes, fittings, equipment, terminals, system classification, slopes, connectivity, schedules
and take-offs. Every item needs an explicit **network** model (nodes, edges, system membership,
direction) or it becomes "lines and cylinders". A geometric clash between two ducts is a distance
query; *code compliance* is a rule set with a cited standard — the two must never be reported as one
thing. Depends on: the BIM object model (systems live in buildings), the placement layer, and the
report layer.

### E. Plant / piping / process — **depends on the same graph model as MEP**

Equipment, pipe routes, segments, fittings, valves, instruments, pipe specifications, line numbers,
nozzles, isometrics, P&IDs, schedules and BOM. The connectivity graph and the specification (line
class) engine are the whole feature; the 3D pipes are a render of it. A P&ID that reads correctly is
not evidence that a process is safe: any check must name its rule source and be tested with both
passing and failing fixtures.

### F. GIS / mapping / geospatial — **a separate layer, not a mode**

CRS handling, transformations, georeferenced rasters, vector layers with attribute tables, spatial
selection and queries, terrain, point clouds, GeoJSON/GeoPackage/Shapefile, WMS/WMTS/WFS, map layouts
with legends, and CAD-to-GIS conversion.

**Why it is its own layer:** precision and metadata are the product. A drawing that *looks* aligned is
not a correct transformation. The document would need an EPSG code per layer, units, a declared
transformation pipeline (PROJ, MIT) and a warning when CRS information is missing. Do not hand-roll
projections. Depends on: nothing above except the entity model; can be scheduled independently of BIM.

### G. Raster / legacy drawing conversion — **independent, and lowest risk to the core**

Thresholding, deskew, denoise, line/edge extraction, primitive fitting, OCR of text and dimensions,
scale calibration, duplicate-line removal, confidence indicators, manual correction, and export into
editable entities. This is a conventional CV pipeline with a review UI. The non-negotiable rule: a
low-confidence reading may never become an authoritative dimension. It needs no kernel change and no
document-model change beyond the 2D entity model it produces.

## 4. The decision

**First industry workspace: mechanical**, scheduled as shared document-model work:

| Step | Why first | Size |
|---|---|---|
| Multi-body document | A mechanical part set, a BIM building and a plant all need more than one body. It is the single most reused change. | L |
| Pattern / mirror | The most requested mechanical modelling feature, and the sketch plane just unblocked it. | M |
| Exact mass properties | Cheap, testable, and a prerequisite for BOM, material take-off and interference. | M |
| Standard parts + BOM | Real user value, but only with recorded standard, edition, units and source. | M then XL |
| Assemblies and mates | The expensive half of mechanical CAD; needs multi-body first. | XL |

**Second: GIS layer**, because it is independent of BIM/MEP and its correctness is measurable
(transformation error against a known point set) rather than visual.

**Not scheduled:** architecture/BIM, MEP, plant and electrical workspaces. Their first slice is an
object model that does not exist yet, and starting them now would produce exactly the disconnected
demos the brief forbids.

## 5. What no workspace may claim until its acceptance criteria pass

* A wall is not a "BIM object" until it carries type, level, material and host/opening relationships
  and exports to IFC with a stated MVD and a stated unsupported-entity list.
* A pipe route is not a "system" until connectivity, system classification and slope are validated by
  a rule with a named source.
* A schematic is not a circuit until a netlist is extracted and checked.
* A map is not georeferenced until an EPSG code and a transformation are recorded and a known point
  round-trips within a stated error.
* A vectorised scan is not a drawing until a human has accepted the uncertain geometry and the scale
  is calibrated against a known dimension.

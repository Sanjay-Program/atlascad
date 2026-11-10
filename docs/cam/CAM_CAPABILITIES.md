# CAM Capabilities

**As of:** 2026-11-04 · **Status:** Phase C 2.5D, 3D roughing and finishing, rest machining, feeds and speeds, and holder checking implemented and tested headlessly. Nothing here has been run on a machine.

The CAM workspace is `crates/cad-cam`, driven headlessly by `atlas cam gcode` and `atlas cam job`, and
runnable as a batch container — see [`docs/operations/cam-deployment.md`](../operations/cam-deployment.md).
It turns a stock block and an operation (or an STL part) into validated toolpaths, a material-removal
preview, a setup sheet and G-code. No program produced here has been sent to a physical machine.

## What works

| Capability | Where | Evidence |
|---|---|---|
| Machining setup: machine envelope, stock block, work coordinate system, clearance plane | `cad_cam::Setup`, `Stock`, `WorkCoordinate` | stock outside the envelope and a clearance plane below the stock top are refused |
| Tool library: flat/ball end mill, drill, face mill, cutting conditions, max depth of cut | `cad_cam::Tool`, `ToolLibrary` | duplicate ids, non-positive diameters and speeds above the machine limit are refused |
| **Feeds and speeds**: material cutting data (surface speed, chip load) turned into spindle speed, feed and plunge, clamped to machine limits with the binding limit reported | `cad_cam::feeds`, `atlas cam feeds`, `cam job --material/--flutes` | 6 domain cases and 3 CLI cases: aluminium at 6 mm and 3 flutes gives 13 262 rpm and 795 mm/min, an 8 000 rpm cap gives 480 mm/min and says `spindle_limited`, an unknown material exits 2; a material job posts `S13262 M3` and `F795` |
| Facing: zig-zag raster over the whole block | `Operation::Facing` | 40 moves for a 40 × 30 block at a 4 mm stepover |
| 2D contour: closed profile offset by tool radius, repeated per depth | `Operation::Contour` | offset corners and loop closure asserted |
| 2D pocket: boundary loop plus raster, held inside the tool radius | `Operation::Pocket` | no cut leaves the radius-shrunk region |
| Drilling with peck retracts and slotting | `Operation::Drill`, `Operation::Slot` | exact depth sequence per hole |
| **3D roughing**: part height field, Z-level raster, walls left one radius proud | `Operation::Rough3D`, `cad_cam::HeightField` | two-block fixture: the 16 mm block is untouched below its top, the 8 mm block stops at the deepest level that still leaves the 0.5 mm allowance, stock beside the part is cleared to the bottom skin, and levels respect the 2 mm maximum depth of cut |
| **3D finishing**: ball-nose raster following the sampled surface one radius above it | `Operation::Finish3D` | flat faces cut at part top + radius; the allowance lifts the surface; a flat end mill is refused; the surface must fit below the clearance plane |
| **Wall finishing**: a Z-level contour of the sampled part cross-section, one cell outside every wall | `Operation::FinishWalls` | a 20 x 20 x 12 block inside a 40 x 30 x 20 stock: every cut stays at least one tool radius from the wall, levels respect the 2 mm maximum depth of cut, the pass removes part of the roughing shell, a part filling the stock has nothing to finish, and a negative allowance is refused |
| **Rest machining**: a smaller flat tool clears what a larger prior roughing pass could not reach, from a simulated copy of that pass | `Operation::Rest3D` | a 3 mm channel the 6 mm tool leaves at full height is cleared by a 2 mm rest tool to 7 mm over a 5 mm base, the legs stay untouched and the ring the roughing pass cleared is not re-cut; a prior tool that is not larger is refused; the setup sheet and program carry the third tool |
| **Tool-holder clearance**: the holder column, against the progressively carved stock | `cad_cam::check_holder_clearance` | a short tool with a wide holder collides at the named move; long flutes and no holder pass; the width of the holder is checked, so a narrow slot does not clear a wide holder |
| Toolpath validation: continuity, axis limits, gouging, rapid-through-stock collision | `Toolpath::validate` | 12 acceptance cases |
| **Material-removal preview**: heightmap stock carved by the program, removed volume and a text render | `cad_cam::MaterialRemoval` | facing 40 × 30 × 20 mm 2 mm deep reports 2 400 mm³ within 5%; rapids remove nothing; a pocket stays inside its area |
| **Setup sheet and tool list**: machine, stock, WCS, clearance, tool table, operation table | `cad_cam::SetupSheet` | generated numbers are asserted against the toolpath; one tool row per tool |
| **PDF setup sheet**: paginated A4, Helvetica, no timestamps | `cad_cam::setup_sheet_pdf` | valid trailer whose `startxref` points at the xref table; deterministic bytes; parenthesised titles escaped; long sheets paginate |
| Postprocessors: Grbl, LinuxCNC, Mach3, modal moves, optional line numbers, tool changes in one program | `cad_cam::Postprocessor` | byte-exact fixtures per format; a two-tool program pauses on Grbl (`M0`) and changes with `T.. M6` on LinuxCNC, and one toolpath is byte-identical through `emit` and `emit_program` |
| CLI: `atlas cam feeds` (cutting data), `atlas cam gcode` (one operation) and `atlas cam job` (one program for every operation with a tool change, Markdown and PDF sheet, stock preview, JSON summary) | `cad-cli` | 19 CLI tests, including a job on a real STL made by `atlas box`, a roughing/wall/rest/finishing program with tool changes, a holder collision, the five job-file refusals, a material job and a recorded toolpath that passes the release gate |
| **Versioned job file**: schema 1, deny-unknown-fields, 64 KiB cap, part resolved beside the file, `material`/`flutes` and `rest.diameter_mm` | `atlas cam job --job` | five CLI cases: a full job with every input, an unknown field, a newer schema, a conflicting input flag, and a missing part (each refusal exits 2); a job-file run with `rest` is exercised too |
| **Toolpath provenance**: `--project` + `--record` writes a recompute-only `toolpath` artifact at the project revision | `atlas cam job`, `cad-engineering-model` | a recorded job passes `atlas artifacts check`; the artifact carries the operation/move/time summary and is stale after a design edit, where only a rerun releases it. The container does the same with `ATLAS_PROJECT` + `ATLAS_RECORD`, copying the project into the output so inputs stay read-only |
| **Batch image**: one job per container, hardened, exit codes 0/1/2/3 | `deploy/Dockerfile.cam`, `deploy/build-cam-image.sh` | image built and smoke-tested on the recorded host with `--smoke`: 614 moves / 18 176 mm³ / five files, collision exit 3 with nothing published, malformed job exit 2, uid 10001, no output volume exit 2 |

Every strategy emits the same discipline: rapid only above the clearance plane, plunge at the
plunge feed, retract at feed to the clearance plane, then the next rapid. Deep cuts are split into
passes no deeper than the tool's maximum depth of cut.

## Validation rules

A toolpath is refused, with the move index named, when:

* it has fewer than two moves, a non-finite coordinate, or an invalid/unset feed;
* a cutting move has no positive feed, or the spindle speed is not usable;
* the clearance plane is not above the stock top;
* a move is zero-length, or longer than 250 mm (a discontinuity, not a traverse);
* a move leaves the machine envelope (after mapping the work frame into machine coordinates);
* a cutting move goes below the stock floor (a gouge);
* a rapid passes through the stock body below the stock top (a crash).

3D roughing additionally refuses a part that fills the stock (`NothingToMachine`) and a non-positive
stepover, stepdown or resolution.

## Known limitations

* **3-axis milling only.** 4-axis, 5-axis and turning are modelled in the `Machine` enum but refused
  by `Setup` and `generate`.
* **Heightmap geometry.** 3D roughing samples the part with vertical rays and the preview uses a
  heightmap stock model, so neither can represent an undercut. A part with a sideways-opening
  cavity must not be roughed with this version.
* **Surface finishing is ball-nose raster only.** The tool centre stays one ball radius above each
  sampled surface point, which is conservative on a slope (it leaves a little more material than
  the ideal normal offset). It covers the height-field cells that hold part, so up to one cell
  (1–2 mm at the default resolution) at the part boundary is not cut. There is no pencil finishing,
  steep-and-shallow split or waterline strategy.
* **Wall finishing leaves about one sampling cell.** The wall pass contours the grid-sampled
  cross-section, not the mesh boundary: it stays a full cell outside every sampled wall so it
  cannot gouge, which leaves up to that cell on the wall. Sample finer (`--resolution`) for a
  thinner wall; a mesh-sliced contour with a true polygon offset is not implemented.
* **One program, one tool change.** `atlas cam job --finish MM` cuts roughing and walls with the
  flat end mill, then changes to the ball nose. Grbl gets `M0` with a comment (change the tool,
  cycle start); LinuxCNC and Mach3 get `T.. M6`. There is no automatic tool changer, tool-length
  probing or per-tool work offset, and no back plot of the change position.
* **Rest machining trusts a simulated prior pass.** `Operation::Rest3D` re-plans the prior roughing
  pass from the given prior tool diameter, stepover, stepdown, allowance and resolution, then
  carves it; it does not read the prior program. If the shop ran a different prior strategy, the
  residual it assumes is wrong. The rest pass cuts at Z levels, so up to one stepdown of material
  can remain above the target, and a channel narrower than the rest tool is still sampled shut.
* **Roughing leaves deliberate stock.** Walls are left one tool radius proud, horizontal part faces
  keep the requested allowance quantised to the Z-level grid (so the real allowance can exceed it
  by up to one stepdown), and the block bottom keeps the allowance as a skin. Stock outside the
  part footprint is cleared down to that skin.
* **Axis-aligned stock.** `Stock` is a block; a casting or near-net-shape stock is bounded by its box.
* **Convex profiles only for offsetting.** The contour offset uses miter normals, so a concave cusp
  is refused rather than approximated.
* **One clearance plane.** Every retract goes to the setup clearance; there is no per-feature
  clearance or full-retract option.
* **Holder checking is a heightmap column check.** The holder is tested as a vertical cylinder
  against the simulated stock in program order. The machine head, spindle nose, fixtures, clamps
  and rotary table are not modelled, and a holder that enters material from the side while the
  stock was never above it is not detected.
* **No machine simulation.** Feeds and speeds are typical starting values per material and tool,
  clamped to the machine limits; they are not a tool-vendor datasheet, and no program has been
  tested against a controller or a real cut.
* **Studio GUI integration.** Available via Studio Multiphysics HUD (`/api/cam/toolpaths`) and direct 2D drafting profile generator (`/api/draft/cam/profile`). Program files and setup sheets download directly in browser.
* **Toolpath persistence.** The engineering-model registry records toolpath summaries and timestamps at the design revision.
* **No produced program has been run.** The fixtures prove the text is what the postprocessor
  intends, not that a controller accepts it.

## Deliberately absent

Adaptive clearing, hole recognition, turning, nesting, multi-axis simultaneous motion, additive
build preparation, and machine simulation. Each needs its own producer, reference fixture and
limitation note before it appears in the table above.

## Acceptance rule for this page

A CAM row may move out of "planned" only when it has: a real input model, generated moves, a
reference fixture, a collision/limit test, and a recorded limitation. "The button exists" is not a
status. See [`docs/quality/ENGINEERING_VALIDATION.md`](../quality/ENGINEERING_VALIDATION.md).

## Reproduce

```text
cargo test --package cad-cam
cargo test --package cad-cli --test cam_gcode --test cam_job --test cam_feeds

# One face operation to a named program:
cargo run --package cad-cli --bin atlas -- cam gcode --size 40,30,20 --depth 2 --tool 6 \
    --stepover 4 --format grbl --out facing.nc

# A whole job from a part: program, setup sheet, stock preview, summary:
cargo run --package cad-cli --bin atlas -- box --size 20,20,12 --out /tmp/part.stl
cargo run --package cad-cli --bin atlas -- cam job --size 40,30,20 --part /tmp/part.stl \
    --tool 6 --stepover 4 --stepdown 2 --stock-to-leave 0.5 --resolution 2 \
    --format grbl --finish 6 --out-dir /tmp/cam-job

# This writes program.nc (3D roughing and wall finishing with the 6 mm flat end mill, then a
# 6 mm ball-nose surface pass after a tool change), setup.md and setup.pdf, stock-preview.txt,
# and summary.json. Adding --rest 2 inserts a third tool and a rest machining pass before the
# surface pass.

# Calculated cutting data for a material and tool (never a datasheet substitute):
cargo run --package cad-cli --bin atlas -- cam feeds --material aluminium --tool 6 --flutes 3 \
    --max-rpm 10000 --max-feed 1200

# The same job with calculated feeds, and a 2 mm rest pass over a 6 mm roughing pass:
cargo run --package cad-cli --bin atlas -- cam job --size 40,30,20 --part /tmp/part.stl \
    --material aluminium --flutes 3 --rest 2 --out-dir /tmp/cam-job-rest

# The same job with a modelled holder (a collision exits 3 before any file is written):
cargo run --package cad-cli --bin atlas -- cam job --size 40,30,20 --part /tmp/part.stl \
    --stepdown 2 --flute 20 --holder-diameter 20 --holder-length 40 \
    --out-dir /tmp/cam-job-holder
```

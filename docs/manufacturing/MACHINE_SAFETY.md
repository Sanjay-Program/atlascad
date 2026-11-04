# Machine Safety

**As of:** 2026-10-31 · **Status:** no machine output or machine control exists.

This workspace does **not** generate NC code, postprocess toolpaths, simulate a machine, or
communicate with a CNC machine, printer or robot. This page states the policy any future
manufacturing output must satisfy; it is not evidence that a machine was driven.

## Non-negotiable rules for future CAM/additive work

1. **No direct machine control.** No code in this repository may command a physical machine until the
   communication protocol, emergency-stop behaviour, machine limits, safety interlocks and operating
   procedure have been independently validated. Simulation and toolpath preview never replace
   physical safety procedures.
2. **Explicit review before posting.** Production machine code export requires a user review step.
   The application must show the operation list, machine, postprocessor version, tool list, stock,
   work coordinate system and the checks that ran.
3. **Fail closed on a check failure.** Axis-limit, collision, singularity, feed/speed and postprocessor
   validation failures refuse the export or mark it as unverified. They are never warnings that can
   be clicked past into a production file.
4. **Versioned postprocessors with fixtures.** A postprocessor is machine-specific, versioned and
   regression-tested against reference outputs. Changing it is a reviewed change, not a config tweak.
5. **Kinematics are not toolpaths.** Tool-axis orientation and machine kinematics are separate,
   testable components. A mathematically valid five-axis path is not automatically safe: singularities,
   rotary limits, retracts, fixture and holder collisions must each be checked against a machine model.
6. **Provenance is mandatory.** Any posted file must reference the model revision, setup, tool library,
   postprocessor version and checks performed. `atlas artifacts check` exits 3 when a toolpath record
   belongs to an older design revision; a release step must treat that as a stop.
7. **Material and tool assumptions are stated.** A feed/speed recommendation must name the material,
   tool geometry, coating and source. The built-in material library is typical handbook data, not a
   machining-data authority.
8. **Additive is not one process.** Polymer and metal processes have separate build preparation,
   support rules and machine profiles. Metal build simulation needs a validated thermal/distortion
   model, not a polymer slicer with a renamed profile.

## Current state of the checks

| Check | Status |
|---|---|
| Toolpath generation | **implemented for a 3-axis mill** (`cad-cam`: facing, contour, pocket, drilling, slotting, 3D Z-level roughing, Z-level wall finishing, ball-nose 3D finishing); every toolpath is re-validated before posting |
| Postprocessor and reference fixtures | **implemented** for Grbl, LinuxCNC and Mach3 with byte-exact regression fixtures, including a tool change in one program; not verified against a controller, and there is no tool-length probing or per-tool work offset |
| Machine kinematics and axis limits | **partially implemented**: moves are checked against a 3-axis envelope. There are no kinematics, rotary axes or singularities to check, and no five-axis path exists |
| Tool/holder/fixture collision | **partially implemented**: a modelled holder is checked as a vertical cylinder against a progressively carved heightmap stock, along each move. The machine head, spindle nose, fixtures, clamps and rotary table are not modelled, and side entry into material that was never above the holder is not detected |
| Material-removal or machine simulation | **partially implemented**: a heightmap stock-removal preview with removed volume and deepest cut. It cannot show undercuts and is not a machine simulation |
| Safe retract/linking strategy | **partially implemented**: cutting runs are separated by straight retracts to the validated clearance plane. There is no ramp or helical entry, and no linking strategy beyond the clearance plane |
| Revision-provenance gate for recorded toolpaths | **implemented** (`atlas artifacts check`, exit 3) |

The last row is a data-governance check only. It prevents a stale toolpath from being *called*
released; it does not make any toolpath safe. No program produced by this workspace has been run
on a physical machine, and nothing in the CAM crates constitutes a safety certification.

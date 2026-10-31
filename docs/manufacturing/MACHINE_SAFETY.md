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
| Toolpath generation | not implemented |
| Postprocessor and reference fixtures | not implemented |
| Machine kinematics and axis limits | not implemented |
| Tool/holder/fixture collision | not implemented |
| Material-removal or machine simulation | not implemented |
| Safe retract/linking strategy | not implemented |
| Revision-provenance gate for recorded toolpaths | **implemented** (`atlas artifacts check`, exit 3) |

The last row is a data-governance check only. It prevents a stale toolpath from being *called*
released; it does not make any toolpath safe.

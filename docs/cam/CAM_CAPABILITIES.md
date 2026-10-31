# CAM Capabilities

**As of:** 2026-10-31 · **Status:** not implemented.

There is **no CAM workspace in this workspace**. This page exists so the absence is recorded in the
location the brief expects, rather than being inferred from a feature list.

## What exists

| Thing | What it is | What it is not |
|---|---|---|
| `ArtifactKind::ManufacturingSetup`, `ArtifactKind::Toolpath` | metadata kinds a caller can record in an engineering-model registry, with a source revision and upstream artifacts | a CAM engine; no setup, stock, tool, toolpath or postprocessor is generated |
| `atlas artifacts check` | a release gate that fails (exit 3) when a recorded toolpath belongs to an older design revision | a toolpath validator; it checks revision provenance only |
| `docs/manufacturing/MACHINE_SAFETY.md` | the safety policy that any future CAM work must satisfy | evidence that a machine was driven |

No NC/G-code is generated, no postprocessor exists, no machine simulation exists, and no tool library
is implemented.

## Planned scope, in dependency order

The brief's CAM phases are accepted as the target, but each item needs a real producer and a
domain-specific test before it may be claimed. The order below is the one the dependencies require:

1. **2.5D foundations (Phase C).** Manufacturing setup, stock and work-coordinate-system model;
   tool library with geometry and cutting parameters; 2D contour, pocket, facing and drilling;
   toolpath visualisation; material-removal preview; setup sheet and tool list.
2. **Post-processing and safety.** A versioned postprocessor with reference fixtures, axis-limit and
   feed/speed checks, safe retract/linking strategies, tool and holder collision checks, and a
   machine simulation that is exercised on known geometry. Nothing may be posted as production code
   without an explicit user review step.
3. **3D milling.** 3D roughing, parallel/scallop/pencil finishing, steep-and-shallow, rest
   machining, hole recognition. Adaptive/constant-engagement clearing needs a measured path-length
   and engagement objective before the words "adaptive" or "constant tool load" may be used.
4. **Turning and turn-mill.** Facing, rough/finish turning, grooving, threading, drilling, boring,
   parting, live tooling, machine-specific validation.
5. **Nesting and fabrication.** Geometric nesting with rotation constraints, grain direction, kerf,
   spacing, multi-sheet yield and remnant management, measured against a stated objective.
6. **Multi-axis.** Indexed 3+2 and simultaneous 4/5-axis on separated tool-orientation and
   kinematics components, with singularity handling, rotary limits and machine-specific
   postprocessing.
7. **Additive.** Polymer (FFF/SLA/SLS) and metal (PBF, DED, binder jet) are separate process
   families with separate machine and material profiles. A polymer slicer is not a metal build
   preparation system.

## Acceptance rule for this page

A CAM row may move out of "planned" only when it has: a real input model, generated moves, a
reference fixture, a collision/limit test, and a recorded limitation. "The button exists" is not a
status. See [`docs/quality/ENGINEERING_VALIDATION.md`](../quality/ENGINEERING_VALIDATION.md).

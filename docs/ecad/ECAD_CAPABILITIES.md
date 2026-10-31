# ECAD Capabilities

**As of:** 2026-10-31 · **Status:** not implemented.

There is **no electronics workspace** in this repository: no schematic editor, no PCB editor, no
net model, no SPICE engine, no Gerber/drill export and no 3D PCB assembly.

## What exists

| Thing | What it is | What it is not |
|---|---|---|
| `ArtifactKind::PcbLayout` | a metadata record that can pin a layout to a design revision | a PCB document |
| `fr4.standard` in the material library | one typical FR-4 laminate entry, clearly marked as not certified | a PCB stack-up, copper model or thermal model |
| `ArtifactKind::Simulation` | can record that a SPICE result belongs to a revision | a SPICE engine |

## Required shape of the first real electronics slice (Phase E)

1. **A real electrical data model.** Symbols, component instances with stable ids, pins, wires,
   junctions, labels, hierarchical sheets, power symbols and nets. Nets must be derived from
   connectivity, not stored as a display string.
2. **Electrical rule checking** over that model: unconnected pins, output-to-output conflicts, power
   pin conflicts, missing values/footprints. Each rule needs a positive and a negative fixture.
3. **Schematic-to-PCB consistency.** Forward annotation, net synchronization, footprint assignment
   and cross-probing, with a test that changes a schematic and observes the PCB diff.
4. **PCB layout basics.** Outline, layers, pads, footprints, tracks, vias, copper zones and design
   rules; then Gerber and drill output against a reference fixture a fab house could read.
5. **SPICE.** Integrate an established engine (ngspice is the obvious candidate) only after its
   licence is reviewed. Validate with reference circuits: RC transient, diode rectifier, common-
   emitter DC bias, op-amp AC sweep. Waveforms and diagnostics are part of the deliverable; a
   netlist writer alone is not.
6. **3D integration and cooling.** Board geometry into the mechanical assembly, connector clearance,
   component power dissipation, and a validated thermal solver for hotspot estimates. A one-line
   temperature calculation must never be presented as CFD.

## Non-claims

* No schematic or PCB file format is read or written.
* No manufacturer's footprints or symbols are bundled.
* No SPICE engine is present or licensed.
* No electronics cooling analysis exists.

Acceptance and evidence rules are in
[`docs/quality/ENGINEERING_VALIDATION.md`](../quality/ENGINEERING_VALIDATION.md).

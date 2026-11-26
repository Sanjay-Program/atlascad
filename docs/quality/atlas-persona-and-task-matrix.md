# AtlasCAD Persona and Task Matrix

**Date:** 2026-11-26
**Purpose:** map the product's real capabilities to fifteen user perspectives and thirty tasks.
These are **testing perspectives**, not claims that human participants were observed.

**Status vocabulary:** **verified** — exercised by a test, the live server, or the browser check in
this tree; **partial** — the capability exists but the full journey was not completed; **gap** —
the capability is absent or blocked; **not driven** — entry points exist, behaviour unverified.

---

## 1. Personas

| # | Persona | Realistic goal | What the product actually supports |
| --- | --- | --- | --- |
| 1 | New user | create a part, save, find it again | guided first-run dialog, one-box project, explicit save, autosave recovery |
| 2 | Engineering student | learn sketch → extrude → analyse | four profile families, extrude/join/cut/revolve, analytic CAE presets |
| 3 | Professional CAD engineer | precise history-based modelling | feature tree, parameters, undo/redo, named revisions; **no constraints, fillets, assemblies** |
| 4 | CAE specialist | configure and inspect a study | FEA/modal/thermal endpoints with analytic validation; no general mesher or solver choice |
| 5 | CFD specialist | domains, fluids, boundary conditions | compressible/analytical CFD and hypersonic models; **no mesh-based CFD** |
| 6 | Manufacturing / CNC user | stock, tools, toolpaths, post | CAM operations, feeds, 3/4/5-axis tests, G-code writers; **no machine execution** |
| 7 | 3D-printing user | profile, orient, support, slice, export | FDM/SLA/SLM profiles, slicing, infill, supports, 3MF; **no printer link** |
| 8 | Blender creative user | move engineering assets into presentation | glTF/USD/OBJ export; **Blender itself not driven** |
| 9 | Unity / digital-twin user | preserve scale, connect behaviour | Unity package + glTF export tested as bytes; **no Unity import test** |
| 10 | DaVinci Resolve user | organise media, hand off | EDL/FCPXML/LUT/OpenUSD writers tested; **Resolve not driven** |
| 11 | AI automation user | goal → plan → approve → verify | 44 agent registrations, DAG, approval gates, MCP; task inputs are fixed case parameters |
| 12 | Expert power user | palette, shortcuts, saved layouts | command palette and shortcuts exist; layout persistence not verified |
| 13 | Accessibility-focused user | keyboard, names, contrast, motion | skip link, focusable canvas, dialogs, reduced-motion blocks; **191/191 controls named; 0 contrast failures in both themes (audited)** |
| 14 | Low-resource Linux user | 8 GB RAM, integrated GPU | 9 MB server, 99 ms start, WebGL software rendering in CI; large models unmeasured |
| 15 | Project manager / reviewer | revision, jobs, reports, approvals | revisions, `/api/report`, `/api/audit`, jobs, approval list; report UI not driven |

---

## 2. Thirty tasks

| # | Task | Persona | Status | Evidence / limitation |
| --- | --- | --- | --- | --- |
| 1 | Create a project | 1, 2 | **verified** | `--new` + HTTP fixture; browser task T1 |
| 2 | Open an existing project | 1, 3 | **verified** | save/reopen tests, revision restore |
| 3 | Save, save-as, close, reopen | 1, 3 | **partial** | save and reopen verified; no save-as route |
| 4 | Recover an interrupted save | 1, 3 | **verified** | `kill -9`, `.recovery`, 12 assertions |
| 5 | Create or import geometry | 2, 3 | **verified** | profile/extrude tests; STEP import as mesh body |
| 6 | Edit a sketch and constraints | 2, 3 | **partial** | profiles and parameters verified; no constraint solver |
| 7 | Modify a feature and rebuild | 3 | **verified** | parameter rebuild, incremental cache, failure reports |
| 8 | Create an assembly | 3, 6 | **gap** | BOM/interference/mass properties exist; no multi-body assembly editor |
| 9 | Assign a material | 3, 4 | **partial** | material library/API tested; UI assignment not driven |
| 10 | Create a drawing/document | 3, 15 | **verified** | drafting editor, DXF, PDF sheet |
| 11 | Configure a supported simulation | 4, 5 | **verified** | CAE/CFD endpoints and presets; UI journey not hand-driven |
| 12 | Run an actual external engine job | 4, 5, 6 | **gap (now honest)** | no adapter; 501 `unavailable` instead of fake results |
| 13 | Inspect results and units | 4 | **partial** | analytic payloads carry units; independent verification absent |
| 14 | Compare results | 4, 15 | **gap** | no comparison endpoint |
| 15 | Generate a report | 15 | **verified** | `/api/report` tested (12 assertions in full check) |
| 16 | Prepare a 3D print | 7 | **verified** | profiles, slice, supports via API |
| 17 | Inspect layer/toolpath preview | 7 | **not driven** | preview code exists; no recorded journey |
| 18 | Export manufacturing output | 6, 7 | **verified** | 3MF/STL/G-code bytes and tests |
| 19 | Prepare CNC workflow | 6 | **partial** | toolpaths/postprocessors tested; machine setup not end-to-end |
| 20 | Blender asset handoff | 8 | **partial** | export bytes tested; Blender not driven |
| 21 | Unity/digital-twin handoff | 9 | **partial** | Unity/glTF writers tested; Unity not driven |
| 22 | Resolve/media handoff | 10 | **partial** | EDL/FCPXML/LUT writers tested; Resolve not driven |
| 23 | Create and execute an AI task | 11 | **partial** | workflow runs call real analytic solvers with fixed parameters |
| 24 | Review/approve an AI plan | 11 | **verified** | approval gate test; default now opt-in |
| 25 | Cancel/retry a job | 11, 14 | **partial** | job queue cancel endpoint; retry semantics not driven |
| 26 | Find commands through search | 12 | **partial** | palette/registry exists; not measured in this pass |
| 27 | Use keyboard shortcuts | 12 | **partial** | shortcuts implemented; no systematic keyboard audit |
| 28 | Change and restore settings | 12, 13 | **partial** | theme persistence recorded earlier; broader settings unverified |
| 29 | Recover from validation errors | 1, 3 | **verified** | typed errors, refusal tests, feature-failure reporting |
| 30 | Review a project as a non-modeller | 15 | **partial** | audit/report endpoints tested; reviewer UI not driven |

---

## 3. Task-to-persona coverage

| Persona group | Verified tasks | Main gap |
| --- | --- | --- |
| New / student (1–2) | 1, 2, 4, 5, 7, 10, 15, 16, 18, 29 | onboarding depth, constraints |
| CAD / CAE / CFD (3–5) | 6, 7, 11, 13, 15 | assemblies, external solvers, result comparison |
| Manufacturing / printing (6–7) | 16, 18, 19 | machine execution, previews |
| Creative / twin (8–10) | none fully | host-application round trips |
| AI / power user (11–12) | 24 | document-bound task inputs, palette measurement |
| Accessibility / low-resource (13–14) | none fully | names, contrast, large models |
| Reviewer (15) | 15 | reviewer UI |

No task is marked verified because a route exists; each row names the test or journey behind it.
Tasks 12, 14 and 8 are capability gaps, not usability defects.

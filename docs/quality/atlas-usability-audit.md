# AtlasCAD usability audit

**Date:** 2026-11-25; **updated:** 2026-11-26 (`UX-002` and `UX-003` fixed) · **Scope:** the shipped Studio (`cad-studio` server + embedded web UI in
`crates/cad-studio/assets/`) and the workflows it exposes. **Method:** agent-led heuristic
walkthrough of the running application plus automated DOM/geometry measurement. **There were no
human participants.** No quotes, task times, success rates or participant counts are reported
because none were collected. Everything below is labelled with how it was obtained.

**Environment:** Fedora Linux 43 (Workstation), 8 cores, 7 GB RAM, headless Chromium
(1280×720 and 1440×900), `cad-studio` release build served on loopback over HTTP, project
`/tmp/atlas-ux/audit.zcad` — a disposable file created for this audit. No user document was opened,
modified or deleted.

**Evidence labels used in this document**

| Label | Meaning |
| --- | --- |
| `automated` | Measured by a script against the running app (CDP/JS geometry, DOM queries, HTTP) |
| `browser-drive` | Performed in a real browser session by the audit script |
| `code-inspection` | Read from the source; the behaviour was not driven |
| `heuristic` | Agent judgement from the walkthrough, explicitly not a user observation |
| `visual` | Read from a screenshot captured during the audit |
| `not measured` | No reliable measurement was obtained; no number is invented |

---

## 1. Executive summary

The application is a genuinely dense professional CAD workbench: 316 API routes, 17 modal dialogs,
639 buttons and a WebGL viewport, all driven from one dark, ribbon-style shell. Its interaction
feedback is better than most projects at this stage — actions confirm themselves with toasts, the
data-safety behaviour is deliberate, and every one of the 639 buttons has an accessible name.

It also had one defect that made it partly unusable on **every** screen smaller than 2726 px wide,
which is every laptop and most desktops:

> The workspace grid's implicit column sized itself to the toolbars' min-content width (~2 668 px).
> The shell hides overflow, so the right-hand inspector and most of the 3D model were rendered past
> the right edge and could not be reached by scrolling. This was measured at 1280, 1366, 1600 and
> 1920 px — the layout was 2 668 px in all four. (`automated`, `visual`)

That is fixed in this audit, with a regression test that fails if it returns.

Two further issues were reported here and have since been fixed, each with a regression check:

1. The primary commit button in the Create panel sat below the fold on a 720 px-tall screen
   (`automated`): `Add extrude` at y=685 with the panel's visible area ending at ~676, inside a
   1 221 px scroll region. It now sits in a sticky footer; `scripts/browser-check.mjs` asserts it
   is inside the panel's visible box at 1280×720 both at the top of the panel and after scrolling.
2. 59 of 191 form controls had no programmatic name (`automated`). All 191 now have one, and
   `scripts/a11y-audit.mjs` fails when any control in the document lacks a name.

## 2. Repository and application inventory

The stack is a Rust workspace (40 crates, edition 2024, `rust-version = 1.90`) serving a
server-rendered shell with a large vanilla-JS front end (`app.js` ~17 400 lines, `index.html`
6 218 lines, `app.css`) embedded into the binary with `include_str!`/`include_bytes!`. There is no
front-end framework and no client-side router; state comes from `/api/state`.

| Area | Route / component | Capability actually implemented | Testable | Dependency / blocker |
| --- | --- | --- | --- | --- |
| App shell | `/` (`index.html`, `app.css`, `app.js`) | Ribbon, global sidebar (16 workspaces), top bar, left/right panels, status bar, command line | Yes | — |
| 3D viewport | `#viewport` canvas | Drag-orbit, pan, zoom, face picking, view cube, Iso/Top/Front/Right presets, Fit | Yes | WebGL |
| 2D drafting | Drafting workbench | Lines, polylines, circles, arcs, splines, trim, fillet, dimensions, blocks, DXF/PDF | Yes | — |
| Feature tree | `#tree` | Sketch/extrude tree with selection; revision counter in the panel | Yes | — |
| Property editing | `#properties` | Numeric property rows reflecting the selected node | Yes | — |
| Command palette | `#paletteInput` | Ctrl/Cmd+K opens; fuzzy command list; executes commands | `code-inspection` | — |
| Command line | bottom bar | AutoCAD-style verbs (`L`, `C`, `REC`, `EXT`, `FEA`, …) | `code-inspection` | — |
| Simulation | `api/cae/*`, 140 solver registry | Static, modal, thermal, multidisciplinary | Yes | CPU time; no remote compute |
| Additive / CAM | `api/additive/*`, `api/cam/*` | Slicing, supports, toolpaths, cutting force | Yes | — |
| AI agents | `api/agents/*` | Plan, step, approvals, workflows, memory | Yes | Requires approval step |
| Collaboration | `api/collab/*` | Comments, presence, locks | Yes | — |
| Enterprise | `api/auth/*`, `api/audit`, `api/bom` | Sessions, API keys, teams, audit log, BOM | Yes | Local identity model |
| Onboarding | `#onboardingModal` (`<dialog>`) | Profile cards, sample problems, Start/Skip, "don't show again" | Yes | Opens only when `localStorage.atlas_onboarded` is unset |
| Handoffs | Unity/DaVinci/Blender controls | Export actions exist in the top bar | Yes | Not driven in this audit |

Routes: 316 distinct `/api/...` paths. Dialogs: 17 `role="dialog"`. Tests: `http_api.rs`,
`cli_token.rs`, plus `scripts/simulate.sh` (56 HTTP/CLI + 30 loopback browser + 34 shared-session
browser assertions, and a DOM accessibility audit) and `scripts/full-check.py` (1 206 assertions).

## 3. Personas and coverage

Personas are test lenses, not claims about real people.

| Persona | Covered in this audit | Where |
| --- | --- | --- |
| A First-time beginner | Partly | `UX-002`, onboarding code inspection, first-load walkthrough |
| B Engineering student | Partly | Sketch/extrude task, terminology inventory |
| C Professional CAD engineer | Partly | Shortcut inventory, feature tree, viewport controls |
| D Simulation specialist | No | Not driven (solver APIs exist) |
| E Additive / CNC | No | Not driven |
| F Blender / creative | No | Handoff controls inventoried only |
| G Unity / digital twin | No | Same |
| H Resolve / media | No | Same |
| I AI / automation | No | Approval routes inventoried only |
| J Expert power user | Partly | Palette and shortcut handlers read, not driven |
| K Accessibility-focused | Yes, structurally | Section 8 |
| L Low-resource workstation | Yes (this host is 8 GB RAM) | Section 9, `UX-001` |
| M Project manager / reviewer | Partly | Revision, save badge, properties pane |

## 4. Task scripts and results

Only the tasks below were actually executed. Everything else is `not tested`; nothing is inferred.

| ID | Task | What was run | Outcome |
| --- | --- | --- | --- |
| T1 | Create and save a project | Browser-drive: `Add sketch` → `Add extrude`; `POST /api/save` | **Complete** — revision 7→8→9, toast "Join extrude added (up)", save 200 |
| T2 | Find a suitable workspace | Load app, inventory 16 workbench tabs | **Partial** — workspaces visible and labelled; switching not driven |
| T3 | Create geometry | Same as T1 | **Complete** (create); import not driven |
| T4 | Sketch + constraint state | Sketch added via UI | **Partial** — constraint solver UI not exercised |
| T5 | Modify a feature | — | **not tested** |
| T6 | Save, close, reopen | Server reopened the file; `--new` over an existing file tested | **Partial** — kill-9 recovery is covered by `full-check.py` |
| T7 | Command via search/palette | Handler read at `app.js:15584` | **not tested** (code-inspection only) |
| T8 | Configure a simulation | — | **not tested** |
| T9 | Inspect a result | Revision/status read via API | **Partial** |
| T10–T11 | 3D print, manufacturing warnings | — | **not tested** |
| T12–T14 | Blender / Unity / Resolve handoff | — | **not tested** |
| T15 | AI plan → approve → verify | — | **not tested** |
| T16 | Recover from a validation error | Invalid-input path not driven | **not tested** |
| T17 | Customise layout/theme/shortcuts | — | **not tested** |
| T18 | Review revision, warnings, report | Revision, save badge observed | **Partial** |

## 5. Metrics (measured only)

| Metric | Value | Source |
| --- | --- | --- |
| First load, DOMContentLoaded | 305 ms | `automated` |
| First load, `load` event | 814 ms | `automated` |
| Failed requests on load | 0 | `automated` |
| Console errors | 0 (4 headless WebGL driver warnings only) | `automated` |
| DOM nodes | 5 827 | `automated` |
| Buttons | 639, of which 0 lack an accessible name | `automated` |
| Form controls | 191, of which 0 lack a programmatic name | `automated` (2026-11-26 re-audit) |
| Server resident memory | 47 MB | `automated` (`ps` RSS) |
| Layout width vs viewport, before fix | 2 668 px in 1 280/1 366/1 600/1 920 px viewports | `automated` |
| Layout width vs viewport, after fix | 1 222 / 1 308 / 1 542 / 1 862 px, all fit | `automated` |
| Task time, wrong turns, success rate, confidence, SUS | | **not measured** (no participants) |
| Contrast ratios | | **not measured** — see `UX-004` |

## 6. Findings

### UX-001 — Workspace clipped on every common screen (P1, **fixed**)

- **Evidence:** `automated` (geometry at four viewports), `visual` (screenshot before/after).
- **Personas:** all; worst for A, C, L.
- **Steps:** load `/` in a 1280×720 window; measure `.topbar`, `.workspace`, `canvas`.
- **Expected:** the shell fills the viewport; every panel is reachable.
- **Actual:** `.app-workspace-container` had no explicit grid column, so its implicit track took the
  toolbars' min-content width (2 668 px). With `overflow: hidden` on the shell, `scrollWidth`
  equalled the viewport (1 280) while the top bar, ribbon, workspace and canvas were 2 668 / 2 050 px
  wide. `canvas.right` was 2 402 px — 1 122 px off-screen — and the right inspector was unreachable.
- **User impact:** on a laptop, the model renders off-centre and the inspector cannot be opened at
  all; a user would conclude the viewport is broken.
- **Root cause (confirmed):** missing `grid-template-columns` on the grid container plus
  non-shrinkable flex toolbars.
- **Fix:** `grid-template-columns: minmax(0, 1fr)` on the container; `min-width: 0; overflow-x: auto`
  on `.topbar`, `.autocad-ribbon`, `.enterprise-workbench-bar` so wide toolbars scroll inside the
  column instead of stretching it.
- **Acceptance criteria:** at 1 280/1 366/1 600/1 920 px, `topbar.right ≤ innerWidth + 1`,
  `canvas.right ≤ innerWidth + 1`, canvas ≥ 300 px wide.
- **Regression test:** two assertions in `scripts/browser-check.mjs`.
- **Status:** fixed, retested.

### UX-002 — Primary commit action below the fold (P2, **fixed 2026-11-26**)

- **Evidence:** `automated` — `#addFeatureBtn` top 685 px, `.panel-left` bottom ~676 px,
  `scrollHeight` 1 221 px vs `clientHeight` 536 px.
- **Personas:** A, B.
- **Steps:** load at 720 px height; enter a depth in Create; look for the button that commits it.
- **Expected:** the commit action is visible without scrolling.
- **Actual:** it is the last element of a 1 221 px panel; a Playwright click on it timed out for 30 s
  until the panel was scrolled programmatically.
- **User impact:** a beginner can fill in depth and direction and never commit the feature, then
  wonder why nothing changed.
- **Root cause (suspected):** no sticky action footer; the panel is a single long scroll.
- **Recommendation:** pin the primary commit control to the panel's bottom edge (sticky footer with
  its own background), keeping the rest scrollable. `effort S`.
- **Acceptance:** at 720 px height the commit button is inside the panel's visible box for every
  feature type, and stays visible while the panel is scrolled.
- **Fix:** the commit control and its hint moved into `.section-commit-footer`, outside the
  `overflow:hidden` section body, which is what stops `position:sticky` from using the panel as its
  scroll container. The footer is hidden with the section when it is collapsed.
- **Status:** fixed and verified. `browser-check.mjs` measures the button at 1280×720 with the panel
  at `scrollTop` 0 and 240; both must be inside the panel box (assertion count 29 → 30).

### UX-003 — Form controls without a programmatic name (P2, **fixed 2026-11-26**)

- **Evidence:** `automated` DOM query over 191 non-hidden controls.
- **Examples:** `importStepFile`, `importDxfFile`, `aiCopilotInput`, `osnapEnd`, `osnapMid`,
  `osnapCen`, `osnapInt`, `osnapPerp`, `osnapTan`, `osnapQuad`.
- **Impact:** a screen-reader user hears "edit text" or "checkbox" with no purpose; snap toggles are
  unusable without sight.
- **Root cause (confirmed):** controls rely on adjacent visual text; no association exists in the
  accessibility tree.
- **Recommendation:** add `aria-label` (or a real `<label for>`) to each unnamed control, starting
  with the visible chrome (OSNAP toggles, import file inputs, AI prompt). `effort M`.
- **Acceptance:** a repeat of the audit query returns 0 controls without a name.
- **Fix:** `aria-label` added to 59 controls across the CAE, CAM, BIM, ECAD, additive, sculpt/PBR,
  media, robotics, BIM imports, project/asset filters, command line, palette, layers, AI prompt,
  named views, engine search, enterprise job/ECO fields and drawing text. A LUT `<select>` that had
  no id was given `id="mediaLutSelect"` so it could be named.
- **Status:** fixed and verified: `node scripts/a11y-audit.mjs` reports **191 controls, 0 unnamed**
  and exits non-zero otherwise. The audit runs in `scripts/simulate.sh`.

### UX-004 — Contrast not reliably measured (P3, open, measurement gap)

- **Evidence:** `not measured`. A CSS-derived sampler produced ratios that contradicted the rendered
  screenshot (it reported dark-on-dark where the screenshot shows light-on-dark text), so the numbers
  are discarded rather than reported.
- **Recommendation:** run axe-core or Lighthouse against the running app and record the actual
  failing pairs; do not hand-compute contrast from tokens.

### UX-005 — WebGL readback warnings (P3, open, hypothesis)

- **Evidence:** `automated` — four `GPU stall due to ReadPixels` driver messages per load, from the
  headless driver.
- **Interpretation:** `hypothesis` — the renderer appears to read pixels back (picking/screenshots)
  in a way that stalls the pipeline. On integrated graphics this may reduce frame smoothness. Not
  measured: no frame-time data was captured, so no performance claim is made.

### Verified strengths (do not regress)

| Observation | Evidence |
| --- | --- |
| Every one of 639 buttons has an accessible name; icon-only buttons carry `aria-label`/`title` | `automated` |
| 3D canvas has a descriptive `aria-label` explaining orbit/pan/zoom/pick | `code-inspection` |
| "Skip to viewport" skip link is the first focusable element | `automated` (snapshot) |
| 6 live regions for status announcements; 17 dialogs use `role="dialog"` | `automated` |
| Actions confirm themselves (`toast("Join extrude added (up)")`) | `browser-drive` |
| `--new` refuses to overwrite an existing document with a clear message | `browser-drive` (observed) |
| First run opens a guided `<dialog>` with sample problems and "Skip to Canvas" | `code-inspection` (`app.js:17361`) |
| 47 MB server RSS, 814 ms first load, no failed requests | `automated` |

## 7. Root-cause analysis (high-priority items)

**UX-001 — 5 Whys:** content was clipped → the shell hides overflow → children were wider than the
shell → the grid column was sized by content → no `grid-template-columns` was declared and the flex
toolbars had no shrink allowance. Confirmed by measurement and by reverting the fix in the live page
(topbar 2 726 px again).

**UX-002:** user cannot commit → the button is out of view → the panel is one long scroll → no
sticky action area was designed for short screens. Suspected; the fix is layout, not logic.

**UX-003:** screen reader has no name → no label association in markup → controls were built with
visual adjacency only. Confirmed.

## 8. Accessibility findings

Keyboard: the skip link is first, the canvas is focusable (`tabindex="0"`) with a descriptive label,
and dialogs use native `<dialog>`/`role="dialog"` (`automated`). Screen reader: blocked by `UX-003`
for 84 controls. Contrast: `not measured` (`UX-004`). Reduced motion: two `prefers-reduced-motion`
blocks exist in `app.css` (`code-inspection`); their coverage was not verified per animation.
Text scaling and magnification were not tested.

## 9. Performance findings

Measured on this host (8 GB RAM): first load 814 ms, 0 failed requests, 5 827 DOM nodes, 47 MB
server RSS (`automated`). Not measured: frame times, memory growth over a session, large-document
behaviour, cancellation latency. The 639-button, 1 375-id document is large for a single page; if it
grows, the shell is the first thing to virtualise.

## 10. Capability blockers versus usability issues

The following are **not** usability defects and were not tested: whether simulations converge, CAM
output is valid, Blender/Unity/Resolve handoffs actually produce files, and AI approvals behave
correctly. Their routes and UI entry points exist; behaviour is unverified. Reporting them as
"confusing UI" would be wrong.

## 11. Backlog (prioritized)

| ID | Persona | Area | Impact | Difficulty | Confidence | Priority | Fix | Effort | Dependency | Acceptance | Status |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| UX-001 | All | Shell layout | 5 | 2 | 5 | **P1** | Explicit grid column + scrollable toolbars | S | — | Fits 1280–1920 px | **fixed / retested** |
| UX-002 | A, B | Create panel | 4 | 2 | 4 | **P2** | Sticky commit footer | S | — | Button visible at 720 px | **fixed / retested** |
| UX-003 | K | Forms | 4 | 3 | 5 | **P2** | Name every control | M | — | 0 unnamed controls | **fixed / retested** |
| UX-004 | K | Contrast | 3 | 2 | 1 | P3 | Run axe/Lighthouse, fix pairs | S | tooling | Ratios recorded | open |
| UX-005 | L | Viewport | 2 | 3 | 2 | P3 | Profile readback path | M | GPU profiling | Frame times recorded | open (hypothesis) |
| UX-006 | A | Onboarding | 3 | 2 | 2 | P3 | Verify first-run tour is reachable from Help | S | — | Tour opens from menu and first run | open |

Priority = Impact × Difficulty × Confidence, triaged by hand where safety or blocking overrides.

## 12. Fixes implemented in this audit

1. **`crates/cad-studio/assets/app.css`** — pinned the workspace grid column to
   `minmax(0, 1fr)` with an explanatory comment, and made the three toolbars shrinkable and
   horizontally scrollable (`UX-001`).
2. **`scripts/browser-check.mjs`** — two assertions that fail if the shell is wider than the viewport
   or the canvas is clipped (`UX-001` regression guard). Assertion counts rose from 27 to 29
   (loopback) and 31 to 33 (shared session).

Subsequent fixes (2026-11-26, same codebase):

3. **`crates/cad-studio/assets/index.html` / `app.css`** — the Create commit action moved into a
   sticky footer (`.section-commit-footer`), hidden with the collapsed section (`UX-002`).
4. **`crates/cad-studio/assets/index.html`** — `aria-label` on 59 controls; the unnamed LUT select
   received `id="mediaLutSelect"` (`UX-003`).
5. **`scripts/a11y-audit.mjs`** — new repeatable audit over every `input`/`select`/`textarea` in the
   document, run from `scripts/simulate.sh`; exits 1 on any unnamed control.
6. **`scripts/browser-check.mjs`** — one more assertion: at 1280×720 the commit button is inside the
   panel's visible box both at `scrollTop` 0 and after scrolling (loopback count 29 → 30).

## 13. Before / after verification

| Check | Before | After |
| --- | --- | --- |
| 1280×720: topbar width / canvas right | 2 668 / 2 402 px | 1 222 / 956 px |
| 1366×768 | 2 668 / 2 402 px | 1 308 / 1 042 px |
| 1600×900 | 2 668 / 2 402 px | 1 542 / 1 276 px |
| 1920×1080 | 2 668 / 2 402 px | 1 862 / 1 596 px |
| `./scripts/simulate.sh` | passed 56 + 27 + 31 | **passed 56 + 29 + 33** |
| Original task T1 (create + save) | — | **complete**: revision 7→8→9, toast, save 200 |
| Regression assertion can fail | — | yes — reverting the CSS in-page gives 2 726 / 2 402 in a 1 280 viewport |

`visual`: the model is now framed and fully visible, and the right-hand tool rail is on screen.

## 14. Remaining risks

- The fix changes the shell's width contract; any component that assumed a ~2 700 px canvas (camera
  framing, minimap, floating docks) should be re-checked at 1280 px by hand. Camera framing was
  verified visually only.
- Toolbars now scroll horizontally; their right-most controls are reachable but not visible at
  1280 px, which is a discoverability trade-off (`heuristic`).
- `UX-002`/`UX-003` are fixed and guarded by scripted checks; the audit is still not a statement
  that AtlasCAD is easy to use, and `UX-004` (contrast) remains unmeasured.
- Simulation, CAM, handoff and AI flows were not exercised, so nothing is claimed about them.

## 15. Follow-up plan

1. `UX-002` is fixed and retested at 720 px height (see §12).
2. `UX-003` is fixed; `scripts/a11y-audit.mjs` now fails on any unnamed control.
3. Add axe-core to `scripts/browser-check.mjs` so contrast is checked automatically as well.
4. Drive T8–T15 (simulation, CAM, handoffs, AI approvals) with the same scripted approach and record
   real outcomes before making any claim about them.
5. Only then consider a broader design pass; no evidence in this audit supports a redesign.

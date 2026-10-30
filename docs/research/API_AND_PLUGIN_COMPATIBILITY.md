# API and Plugin Compatibility

**As of:** 2026-10-30. **Purpose:** state exactly what an external program can call today, what it
cannot, and what a compatibility claim would cost. Every "exists" entry below is backed by code and a
recorded test; every "does not exist" entry is a design decision, not an oversight to be waved away.

## 1. What exists, verified

| Surface | Where | What it is | Evidence |
|---|---|---|---|
| `cad_contract::Kernel` trait | `crates/cad-contract/src/kernel.rs` | The geometry contract: capability discovery, extrude, boolean, translate, revolve, tessellate, release, contains, volume, bounding box, profile validation | 55 tests in `cad-contract` |
| Capability set | `crates/cad-contract/src/capability.rs` | 20 declared capabilities; a caller must ask before using one, and an adapter that cannot do it must not advertise it | `bits_are_distinct_for_every_capability`, `missing_reports_the_gap` |
| Versioned document header | `crates/cad-contract/src/document.rs` | Header schema with migrations and refusal of a newer schema | `t-cad-contract` |
| `.zcad` project document | `crates/cad-document` | Schema 8 payload: parameters, sketches (plane height, five profile kinds), ordered features (extrude with direction, full-turn revolve; new-body/join/cut), undo/redo, named revisions; atomic save; recovery snapshot | 78 tests |
| Headless CLI | `crates/cad-cli` (`atlas`) | Contract consumer, binary STL and native project commands, no GUI | 10 tests |
| Studio HTTP API | `crates/cad-studio` | `GET /api/health`, `GET /api/state`, `GET /api/export/stl`; `POST /api/parameter`, `/api/sketch`, `/api/sketch/profile`, `/api/sketch/origin` (with optional `plane`), `/api/feature/extrude` (with `direction`), `/api/feature/revolve`, `/api/revision`, `/api/revision/restore`, `/api/revision/remove`, `/api/undo`, `/api/redo`, `/api/save`, `/api/recovery/discard` | 25 HTTP tests |
| Agent API | `crates/cad-agent-api`, `cad-agent-runtime` | Declared contract, permission grants, budgets, strict JSON-Schema subset, quarantine; one implemented agent (`agent.cad.geometry-ops`) | 13 + 16 + 24 tests |
| Engineering control plane | `crates/cad-engineering` (`atlas-eng`) | `validate`, `scan`, `report`, `tasks`, plus licence and SBOM checks | 30 tests |

### HTTP API contract details

* **Versioning:** the state payload carries `schema` (document payload version) and `kernel.name` /
  `kernel.version` / `kernel.capabilities`. There is no separate API version header, and none is
  promised: **the API is unversioned and may change between builds.** A client that needs stability
  should pin a build.
* **Errors:** every failure is JSON `{"error":{"code":…,"message":…}}` with a stable code
  (`invalid_project`, `invalid_parameter`, `unknown_sketch`, `unknown_revision`, `bad_request`,
  `invalid_json`, `feature_failed`, `kernel_failure`, `export_failure`, `unauthorized`,
  `forbidden_origin`, `payload_too_large`, `unsupported_media_type`, `method_not_allowed`,
  `nothing_to_undo`, `nothing_to_redo`, `internal_error`) and an HTTP status.
* **Permissions / trust boundary:** loopback only by default. `--share` binds one LAN interface and
  requires a ≥16-character token on every `/api/*` request (`X-Atlas-Token` or `?token=`). Every POST
  must carry a same-origin `Origin` and a JSON content type. Bodies are capped at 1 MiB, headers at
  16 KiB. No endpoint accepts a filesystem path — the project path is fixed at launch.
* **Execution limits:** a request is synchronous and single-threaded; there is no queue, no job id, no
  cancellation token in the HTTP surface. A long boolean blocks the request. Budgets exist on the
  kernel adapter (`Budget` with deadline and cancellation) but are not exposed over HTTP.

## 2. What does not exist

| Surface | Status | What an honest claim would require |
|---|---|---|
| Public plugin SDK | **Absent** | A versioned trait/ABI boundary, a manifest with declared permissions and dependencies, a sandbox, lifecycle events and a conformance suite. Adding a plugin loader to the in-process studio without a sandbox would give untrusted code the user's whole session. |
| Plugin permissions / sandbox | **Absent** | Untrusted plugins must not get shell, filesystem or network by default. The agent runtime's grant model (declaration ∩ owner decision, `shell`/`secrets` owner-gated) is the in-repo precedent to extend — but it runs in-process today, which is recorded security finding S-4/S-8. |
| Native scripting (Python, native command language) | **Absent** | An embedded interpreter plus a command serialization layer, a sandbox, and a conformance suite. An interpreter is not a plugin sandbox. |
| Batch conversion / headless modelling | **Partial** | `cad-cli` builds a `.zcad` and writes STL headlessly; there is no batch/queue mode, no directory walker, no format matrix beyond STL. |
| Custom commands / feature creation from outside | **Partial** | A client can create *existing* sketch profiles and features through HTTP; it cannot define a new feature kind, a new profile or a new document entity. |
| Document events / notifications | **Absent** | No webhook, no event stream, no change feed. Polling `/api/state` is the only mechanism. |
| Export pipelines | **Partial** | STL export only; no STEP/DXF/PDF/IFC pipeline. |
| Custom validation rules | **Absent** | No rule API. The design-rule-check agent is declared and unimplemented. |
| MCP server | **Absent in the platform apps** | The two imported applications have their own MCP servers (unbuilt and unverified here). A platform MCP server over the HTTP API is a small, testable addition, but it must reuse the same permission model. |
| **AutoLISP / Visual LISP compatibility** | **Absent and not claimed** | A Lisp interpreter with AutoCAD's function library, entity model semantics, symbol tables and reactor behaviour — plus a compatibility corpus. Exposing our own API is *not* this. |
| **VBA compatibility** | **Absent and not claimed** | An ActiveX/COM automation host with the AutoCAD object model. Different platform, different object model, different lifecycle. |
| **.NET / ObjectARX compatibility** | **Absent and not claimed** | A CLR host and the managed object model; ObjectARX is a native ABI that cannot be reproduced from a Rust document model. |
| **FeatureScript compatibility** | **Absent and not claimed** | Onshape's language and its standard library are product-specific. A compatible implementation would need the language, the library and its semantics. |
| Python automation "like FreeCAD" | **Absent and not claimed** | FreeCAD's API exposes its own workbench and document objects. A Python layer over our typed operations could exist, but it would be *our* API, not FreeCAD's. |

**Rule:** a compatibility row may only be marked implemented when a conformance test suite for that
ecosystem runs green. Prefer a documented subset over a broad claim.

## 3. Versioning and compatibility policy (what is actually promised)

1. **Document schema:** `.zcad` refuses an unknown field and refuses a newer schema. Loading a file
   never silently discards data. Migrations are one-way on load and never rewrite the file on disk.
2. **Contract:** `cad-contract` is a workspace crate with no published semver guarantee; the workspace
   version is `0.1.0`. A third-party adapter would have to pin a commit.
3. **Capabilities:** adding a capability is additive; a caller must ask. Removing one would be a
   breaking change for any adapter.
4. **HTTP API:** unversioned, as above. The test suite is the specification.
5. **Agent contract:** `cad-agent-api` has an explicit contract version and a strict schema subset;
   unsupported JSON-Schema keywords are refused rather than ignored. That is the one surface where a
   version number means something today.

## 4. Licensing constraints on extensions

| Candidate | Licence | Obligation / decision |
|---|---|---|
| `truck` + vendored patched `truck-shapeops` | Apache-2.0 | Permissive; provenance of the patch must be recorded (security finding S-5) |
| `acadrust` (DWG, inside `apps/cadcraft`) | MPL-2.0 | File-level copyleft on modified files; used unmodified today |
| OpenCASCADE / `cadrum` / `opencascade-rs` | LGPL-2.1 | **Excluded** by decision G1; static linking into a closed binary does not satisfy §6 without relink rights |
| KiCad (IPC API integration) | GPL-3.0 application | Interface boundary only unless KiCad code is bundled; a separate review is required before bundling |
| PROJ (for a future GIS layer) | MIT | Permissive; must be the projection implementation, not hand-rolled maths |
| Python (if embedded) | PSF licence | Permissive; embedding needs its own sandbox review |

## 5. What a plugin SDK would have to contain (design only, not built)

1. **Manifest:** id, version, contract version it targets, declared capabilities, declared
   permissions (`read_workspace`, `write_workspace`, `shell`, `network`, `secrets`), dependency list.
2. **Permission model:** effective grant = declaration ∩ owner decision; `shell`, `network` and
   `secrets` owner-gated and off by default; every call audited.
3. **Isolation:** a separate process (or a WASM instance) with a bounded message protocol. In-process
   loading is not a sandbox and must not be presented as one.
4. **Commands and events:** a small, typed command set (the existing document operations) plus
   document-changed and selection-changed events; no direct mutation of document internals.
5. **Failure containment:** a plugin panic or timeout quarantines the plugin, not the session; the
   runtime already does this for agents and the same mechanism should be reused.
6. **Conformance suite:** schema validation, permission refusal, budget enforcement, panic
   containment and a documented "what a plugin cannot do" list.

None of items 1–6 is implemented. The agent platform is the closest existing precedent and its
limitations (in-process, synchronous, no isolation) are recorded in `docs/security-review.md`.

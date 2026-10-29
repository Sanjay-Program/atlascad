# Security Review

**Date:** 2026-10-21, Atlas Studio section added 2026-10-25 · **Scope:** the eleven repositories in
this workspace plus the platform crates (`cad-contract`, `cad-kernel-truck`, `cad-document`, `cad-io`,
`cad-studio`, and the agent/engineering crates).
**Method:** reading the security-relevant code paths and manifests, plus measured checks. This is a
review, not a penetration test, and it says so.

---

## 1. Status summary

| Control | State | Evidence |
|---|---|---|
| `unsafe` in application crates | **Good** | CADCraft: **0** occurrences. SolidCraft: **16**, of which **15 are the macOS FFI declarations** in `crates/threedx-client/src/macos.rs` and **1** in `ui-egui`. Workspace lints `unsafe_code = "forbid"` in both apps, including `cadcraft-geom`, `cadcraft-doc`, `solidcraft-doc`, `solidcraft-kernel`. |
| Panic containment around the geometry kernel | **Good, and verified** | SolidCraft wraps every kernel call in `guard`. In the new adapter a **real panic inside truck** (`tolerance must be no less than 1e-6`, `intersection_curve.rs:172`) was caught and reported as `KernelError::Internal` instead of aborting. |
| `unwrap`/`expect`/`panic` in libraries | **Good** | Both apps `deny(clippy::unwrap_used, expect_used, panic, unimplemented, todo, unreachable)` per crate; the new crates do the same. |
| Resource limits on imported files | **Partial** | CADCraft `crates/io` has explicit caps: `MAX_PLOT_STYLE_TABLES = 1000`, `MAX_PLOT_STYLE_NAMES = 100_000`, `MAX_XDATA_ITEMS = 4096`, `MAX_PAYLOAD = 64 MiB`, export coordinates within ±1e12. SolidCraft has no equivalent caps visible in `crates/io` for STEP/3MF sizes. |
| Path confinement for file references | **Good, converging** | CADCraft has a dedicated policy crate (`craft-docpath`: "Which paths stored in a drawing (xrefs, data links) may be opened"). SolidCraft has `crates/io/src/vfs.rs` doing `canonicalize`-based confinement, and its MCP server requires `--allow-path DIR` (with `--allow-any-path` as an explicit opt-out). |
| Credentials in source or client bundles | **Good** | A targeted grep for `api_key|secret_key|BEGIN PRIVATE KEY` across both `crates/` trees returned only **environment-variable names** (`OPENAI_API_KEY`, `ANTHROPIC_API_KEY`) used to launch third-party agent CLIs — no key material. |
| Dependency advisory scanning | **NOT DONE — BLOCKED** | `cargo-audit`, `cargo-deny` and `cargo-license` are **not installed**; no scan was performed and no clean bill of health is claimed. |
| Licence compatibility | **Risk documented** | See `docs/license-inventory.md`; two LGPL/OCCT paths must not enter a proprietary binary. |
| Vulnerability disclosure process | **Good** | SolidCraft ships `SECURITY.md` with private reporting via GitHub advisories and names untrusted-file and MCP exposure as priority areas. CADCraft has no `SECURITY.md` (**gap**). |

## 2. Findings

### S-1 — No advisory or dependency-policy scan is running (HIGH, process)

`cargo-audit`, `cargo-deny` and `cargo-license` are absent. CADCraft has **no `deny.toml`**; SolidCraft
has one. With ~485 and ~600 resolved packages respectively, an unpatched advisory is a matter of time.

**Recommendation:** add `deny.toml` to CADCraft and run `cargo deny check advisories bans licenses
sources` in CI for both apps. Suggested policy: allow MIT/Apache-2.0/BSD/ISC/Zlib/Unicode-3.0,
allow MPL-2.0 with the "unmodified" note, deny GPL/AGPL/SSPL and unknown licences.

### S-2 — LGPL / OpenCASCADE is one import away from the shipping binary (HIGH, legal/architectural)

`kernels/opencascade-rs` is **LGPL-2.1**; `kernels/cadrum` is MIT but **statically links headless
OCCT**, so distributing its binary carries LGPL-2.1 obligations regardless of the wrapper's licence.
Neither is in either app's resolved graph today (verified). The danger is a future `cargo add`.

**Recommendation:** make it structural — a `deny.toml` rule plus a CI check that fails if
`opencascade*`/`cadrum` appear in a shipping dependency graph. Decide dynamically-linked LGPL or
exclude, in writing, before any commercial release.

### S-3 — Untrusted CAD files are the primary attack surface (MEDIUM-HIGH)

CAD files are attacker-controlled input with a history of memory-safety and resource-exhaustion bugs.
Relevant exposures here:

| Surface | Where | Current state |
|---|---|---|
| DXF (R12–2018, ASCII + binary) | `cadcraft/crates/dxf`, `crates/io` | Caps present (S-3a); parser is pure Rust with no `unsafe`. |
| DWG | `cadcraft/crates/dwg` via **`acadrust` (MPL-2.0)** | Third-party parser, unmodified. Its bugs are reachable from a file open; MPL-2.0 means fixes must be published if made. |
| SVG import | `quick-xml 0.41` | Comment in the manifest states external entity expansion is avoided beyond the five predefined entities — i.e. XXE considered. |
| STEP / IGES / 3MF / glTF | `solidcraft/crates/io` | `zip` (3MF) + `quick-xml`; `chardetng`/`encoding_rs` for legacy STEP encodings. **Import budgets now apply** (file size, entity/triangle count, zip entries, uncompressed size) with cancellation — see `crates/io/src/budgets.rs`. |
| Fonts (`.ttf/.otf/.ttc`) | `cadcraft/crates/fonts`, `skrifa` | Font parsing is a classic memory-safety surface; `skrifa` is pure Rust. |

**Recommendations:** ~~add import budgets to SolidCraft (max file size, max entity/triangle count,
max zip entry count and uncompressed size for 3MF → zip-bomb defence), and a wall-clock/ memory
ceiling per import that can be cancelled.~~ **Done in the 2026-10-22 cycle** —
`crates/io/src/budgets.rs` gives every reader an `ImportBudgets` (file bytes, entities, triangles,
zip entries, uncompressed bytes) checked *before* memory is spent, plus a `Cancel` token checked at
checkpoints; the plain entry points use the defaults, and the `_with` variants take custom budgets.
Treat "open untrusted file" as a separate, fuzzable entry point —
SolidCraft already has `pub mod fuzz;` in its kernel; CADCraft's DXF reader would benefit from a
`cargo-fuzz` target.

### S-4 — AI/MCP is the second attack surface (MEDIUM)

Both apps expose MCP (`crates/mcp` in each) and SolidCraft additionally has an in-app agent chat.

Verified defences:

* SolidCraft's MCP server requires `--allow-path DIR` and documents that it does **not** create
  missing folders, that paths must be on a local drive, and that hard links inside an allowed folder
  cannot be distinguished from ordinary files (**honest limitation, well documented**).
* CADCraft's MCP backend talks to the app over a **loopback control port** (`--control PORT` /
  `CADCRAFT_CONTROL_PORT`), audited below.

#### CADCraft loopback control port — audited 2026-10

Transport (`apps/cadcraft/src/control_server.rs`):

* **Bind.** `TcpListener::bind(("127.0.0.1", port))` only; the port is never bound to `0.0.0.0`. A
  failed bind is logged and the channel stays off — there is no fallback to a wider address, and the
  channel exists only when `--control PORT` / `CADCRAFT_CONTROL_PORT` asks for it.
* **Peers.** A connection whose peer address is not loopback is refused at `accept`, so widening the
  bind (or a machine that forwards the port) cannot expose the editor without a second change.
* **Protocol.** One JSON object per line. Anything else — an HTTP request line (a browser POST), the
  HTTP/2 preface, invalid UTF-8, JSON that is not an object, a non-string `method` — is answered with
  one error line and the connection is closed *before* the rest of the input is read, so a request
  smuggled into an HTTP body never reaches the app.
* **Bounds.** 16 connections, 64 queued requests (more answer `busy`), a 256 MiB line (whole drawings
  as base64 still fit), 10 min idle, 30 s write, 60 s reply. An over-long line is read for at most
  limit + 1 bytes and then dropped.
* **Timing.** A request that times out is cancelled if the app has not started it, and the reply's
  `state` (`not-run` / `started`) says whether it can still have run.

Request handling (`cadcraft-ui-egui::control`, `crates/mcp`): methods and parameters are named values,
not a script; a malformed or unknown request is an error reply and leaves the drawing untouched.
Evidence: `control_server.rs` tests
(`an_http_request_closes_the_connection_before_its_body_is_dispatched`,
`a_line_that_is_not_a_request_closes_the_connection`, `only_loopback_peers_are_served`,
`a_full_queue_answers_busy`, `an_idle_connection_is_closed`) and `crates/mcp/src/tests.rs`
(`bad_tool_calls_are_invalid_params`, `errors_are_tool_errors_not_crashes`).

Residual risks to keep in mind:

1. `--allow-any-path` disables the confinement entirely — it must stay an explicit, documented choice,
   never a default or a convenience for an agent.
2. **Any local process can drive the editor** while the control port is open. Loopback is the trust
   boundary, and the port is off by default, but a token — a random per-start secret the app logs and
   the MCP client passes — would close this. That is a wire-protocol change and is recorded as a
   follow-up, not done here.
3. AI features must execute **structured, validated commands**, never generated code or shell. Nothing
   in either tree suggests otherwise — commands are schema-typed (`schemars` in SolidCraft's command
   registry) — but this deserves an explicit test that malformed AI output is rejected.

#### Atlas Studio loopback HTTP shell — audited 2026-10-25

The root application (`crates/cad-studio`) adds a second loopback service, so it was reviewed against
the same standard:

* **Bind and peers.** `TcpListener::bind(("127.0.0.1", port))` only. There is no option to bind a
  public interface, no fallback address, and no tunnel or reverse-proxy mode.
* **Browser-origin checks.** Every request's `Host` must name the studio's own loopback host and
  port (`127.0.0.1:<port>` or `localhost:<port>`); every `POST` must carry a loopback same-origin
  `Origin` and `Content-Type: application/json`. A request with a foreign
  origin, a wrong media type, or a missing origin is refused before the document is touched. This is
  the DNS-rebinding and cross-site-request defence.
* **Bounds.** 16 KiB request headers, 1 MiB JSON bodies, 5 s read timeout, 10 s write timeout, 64
  simultaneous connections (further connections get a 503 rather than unbounded threads), and no
  client-supplied filesystem path. The project path is fixed at launch; the only writes are the
  project file (atomic temporary + fsync + rename) and the STL response.
* **Response headers.** `Cache-Control: no-store`, `X-Content-Type-Options: nosniff`,
  `Referrer-Policy: no-referrer`, `X-Frame-Options: DENY`, and a CSP of `default-src 'self'` with
  inline styles only. Assets are embedded in the binary; there is no asset directory to traverse.
* **Error handling.** Typed errors return a code and message, never a stack trace or path. A panic in
  a connection thread is caught with `catch_unwind`; the accept loop and other connections survive.
* **Verified by tests.** The eight HTTP integration tests cover foreign and empty `Host`, foreign
  `Origin`, the wrong media type, an oversized body, malformed JSON, unknown fields, unknown
  sketches/profiles, and a clean STL parse-back. Shutdown now wakes a blocking `accept` with one throwaway loopback connection, so
  the process does not linger after the server handle is dropped.

Residual risks: **any local process or local browser page that can reach loopback can drive the
studio while it runs** — on loopback there is no token or authentication, only origin checking. That
is the accepted desktop trust model, not a multi-user one; do not expose the port through a tunnel
and call it hosted. The app has no TLS, accounts, tenancy, or audit log.

##### Sharing on the local network — added 2026-10-29

`--share` (or any non-loopback `--host`) opts into a second trust model, and it was reviewed as a
boundary change rather than a convenience flag:

* **A token is mandatory, never optional.** A non-loopback bind always has a token: `--token`
  (rejected below 16 characters) or a generated 128-bit value from two `RandomState` hashes. There is
  no code path that binds a LAN interface without one.
* **What is protected.** Every `/api/*` request — reads, writes, export, health — requires
  `X-Atlas-Token` or `?token=`; a missing or wrong token is `401` before routing. The static page,
  CSS, script and icon are served without a token because a browser cannot present one before it has
  parsed the page, and they carry no document data.
* **Comparison.** `tokens_match` compares the whole presented token in constant time and does not
  early-return on length mismatch after the length check.
* **Host and origin under sharing.** The `Host` must name the studio's port (the address a colleague
  dialled cannot be known in advance); a write must still carry an `Origin` equal to the host it was
  served from. DNS rebinding therefore cannot read the document without the token, which is the
  defence that matters here.
* **Leak surface.** The token lives in the URL, so it can appear in browser history, a copied link,
  or a proxy log. The page is served with `Referrer-Policy: no-referrer` and the token is only put
  in an `X-Atlas-Token` header by the app, never logged. There is no rotation, no revocation endpoint
  and no audit trail: stopping the process revokes the link.
* **No confidentiality on the wire.** Plain HTTP on a LAN; anyone who can sniff the segment can read
  the document and the token. Not for hostile networks or the internet.
* **One document, one history.** Sharing is not multi-user editing: two clients share one undo stack
  and the last write wins. There is no per-client presence, locking or merge.
* **Verified.** `a_shared_studio_requires_its_token_and_keeps_the_same_origin_rule` (401 without and
  with a wrong token, header and query forms, 401 on an unauthenticated write, 403 on a foreign
  origin, 403 on a `Host` naming another port, a token-bearing 24000 mm³ write, the link in the
  payload, `Referrer-Policy` on the page) and `a_loopback_studio_has_no_token_and_no_share_link`;
  plus the live simulation's 4 share assertions and a headless-browser run of a shared page, where the page exports an STL with its own token. Import of untrusted CAD files is not
wired to the application at all; only the STL reader in `cad-io` exists, and it caps declared facet
counts before allocation and refuses truncated input.

### S-5 — Supply chain provenance (MEDIUM)

1. CADCraft's two `craft-libs` crates (`craft-docpath`, `craft-ui`) were pinned by git commit. Their
   licence is **recorded (MIT OR Apache-2.0)**, both pinned revisions are **vendored into**
   `apps/cadcraft/vendor/` with `PROVENANCE.md` and sha256 manifests, and CADCraft depends on them by
   path (no network needed for release builds). This part of S-5 is closed; see
   `docs/license-inventory.md` §4.4.
2. SolidCraft vendors **four patched truck crates** plus two UI crates with `[patch.crates-io]`.
   Patches are marked `SolidCraft:` and the vendor README documents them — good practice — but the
   fork must be tracked deliberately or it silently diverges from upstream security fixes.
3. The new workspace depends on `apps/solidcraft/vendor/*` by path. That is an intentional,
   documented coupling for now (see `docs/kernel-comparison.md` §4.4); it must not become a release
   packaging path without vendoring at a pinned revision.

### S-6 — Secrets handling for future SaaS/AI (MEDIUM, preventive)

No credentials are in the tree today. When AI inference or billing is added: keys belong on the
server or in an OS keychain, never in the desktop binary, the WASM bundle, project files, or logs.
`docs/license-inventory.md` and `docs/architecture.md` already require this; it should become a CI
check (grep for key-like patterns in packaged artifacts) rather than a convention.

### S-7 — Logging of project content (LOW, preventive)

No structured logging policy exists for user geometry. Crash reports and logs must not include
coordinates or project text by default. SolidCraft's `guard` records that a panic was recovered — the
right granularity; keep payloads out of logs.

### S-8 — The agent runtime is in-process and unscheduled (MEDIUM, architectural)

Added in the 2026-10-22 cycle. The agent platform (`crates/cad-agent-api`,
`crates/cad-agent-runtime`, `crates/cad-agent-geometry`) is the first place in this workspace where
*code chosen at runtime* can act. Its defences are real and tested:

* an agent is not trusted when it appears: it must pass contract validation, schema enforcement,
  permission checks and a contract-test report before it may become `ready`;
* the effective grant is the intersection of the agent's declaration and the owner's decision, and
  `shell` and `secrets` are owner-gated, so no agent can reach them without an explicit decision;
* the runtime enforces the call count, the output size and the wall-clock limit from outside the
  agent, and the agent cannot widen its own budget;
* a panicking agent is quarantined rather than taking the host down.

The residual risk is the boundary itself:

1. **An agent runs in the host process.** There is no sandbox. This is acceptable for first-party
   agents, whose code is reviewed with the rest of the repository, and it is **not** acceptable for
   third-party native code, which must be isolated in a separate process. The manifest records
   process isolation as the required strategy for such code; nothing third-party is loaded today.
2. **There is no scheduler.** A call is synchronous and bounded by the contract's budgets, so a
   long operation blocks the calling thread. A host that runs agents on a UI thread needs the job
   boundary (roadmap M10) before it can use this.
3. **The agent's filesystem read is confined but not sandboxed.** The workspace is canonicalised at
   construction, the file name must be a single path component, and the resolved path must still be
   inside the workspace after symlinks are followed. That defeats traversal and symlink escapes. It
   does **not** stop an agent from reading a large file *inside* the workspace: the cap is 1 MiB and
   the workspace itself is trusted content.
4. **A cancelled call is not a killed call.** Cancellation is cooperative; a pathological boolean
   inside truck cannot be interrupted until it returns.

## 3. What the new code does (and does not) add

`crates/cad-contract`, `crates/cad-kernel-truck`, `crates/cad-agent-api`, `crates/cad-agent-runtime`
and `crates/cad-agent-geometry` were written under `unsafe_code = "forbid"` and
`deny(clippy::unwrap_used, expect_used, panic)`:

* all caller input is validated before it reaches truck (profile: ≥3 finite points, no repeated
  consecutive point, non-zero area; tolerance: finite, positive, within kernel bounds);
* handles are namespaced per kernel instance, so a stale or foreign handle is `UnknownSolid`, not a
  use-after-free;
* a boolean result that violates volume conservation is **refused** rather than returned;
* a third-party panic becomes `KernelError::Internal` (measured);
* no file I/O, no network, no environment access, no secrets — in the contract and the adapter.

The **cancellation gap this review recorded is now closed**: every kernel operation has a
`_with_budget` twin that takes a deadline and a cancellation token, checks both before and after the
work and at internal checkpoints, and reports `Timeout`/`Cancelled`. An interrupted operation leaves
the kernel instance usable, which is asserted rather than assumed. The adapter still does **not**
implement tessellation resource budgets for very large models. The import budgets in SolidCraft IO
(S-3) are **now in place** (`crates/io/src/budgets.rs`, tested: zip-bomb and cancellation paths).

The agent crates add two new surfaces and bound both:

* **the filesystem**, for exactly one operation (reading a JSON profile file), confined as described
  in S-8 and capped at 1 MiB and 4096 points;
* **the runtime**, for the agent's own execution, bounded by the contract's call, output and
  wall-clock budgets and by the runtime's grant.

Neither surface includes the network, the shell, secrets or a write.

## 4. Prioritised actions

| # | Action | Why | Effort |
|---|---|---|---|
| 1 | Install/run `cargo-audit` + `cargo-deny`, add `deny.toml` to CADCraft, wire into CI | S-1: unknown advisory state across ~600 packages | Small |
| 2 | Add a CI guard that fails if `opencascade*`/`cadrum` reach a shipping graph | S-2: the one legal risk that can block distribution | Small |
| 3 | ~~Add import budgets + cancellation to SolidCraft `crates/io` (file size, entity count, zip entries/uncompressed size)~~ | **Done** in the 2026-10-22 cycle: `budgets.rs` (ImportBudgets + Cancel), enforced in threemf/STEP/STL readers | — |
| 4 | Add a `SECURITY.md` to CADCraft | S-1/process: no disclosure route today | Small |
| 5 | Fuzz the DXF and STEP readers with `cargo-fuzz`, starting from the existing example corpus | S-3: highest-value bug-finding per hour | Medium |
| 6 | ~~Record the `craft-libs` licence and vendor the pinned revisions for release builds~~ | **Done** in the 2026-10-22 cycle: licensed MIT OR Apache-2.0, vendored at two pinned revisions into `apps/cadcraft/vendor/`, CADCraft now depends by path | — |
| 7 | ~~Implement `Cancelled`/`Timeout` in the adapter and a per-operation deadline~~ | **Done** in the 2026-10-22 cycle (ADR-0006) | — |
| 8 | Isolate third-party agents in a separate process before any are loaded | S-8: an in-process agent is a sandboxed-away-from-the-host assumption that only holds for first-party code | Medium |
| 9 | Move agent execution off the UI thread (job boundary with bounded concurrency) | S-8: a synchronous call blocks the calling thread | Medium |

## 5. Limitations of this review

* No dynamic analysis, fuzzing, or penetration test was performed.
* The 2 212 + 5 884 existing tests were not run in full (see `docs/build-and-test.md`); a security
  review cannot rely on tests it did not execute.
* No advisory database was consulted (tooling absent).
* WASM, the web build, and the MCP transport internals were not assessed.
* Findings are based on reading the current checkouts at the recorded commit SHAs, not on a threat
  model supplied by the product owner.

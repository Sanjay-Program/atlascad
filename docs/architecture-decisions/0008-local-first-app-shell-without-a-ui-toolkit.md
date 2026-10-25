# 0008 — Local-first app shell without a new UI toolkit dependency

- **Status:** accepted
- **Date:** 2026-10-25
- **Supersedes:** none

## Context

The root workspace had a tested contract, kernel adapter, document model, CLI, control
plane, and agent runtime, but no usable product surface. The application checkouts under
`apps/` are large desktop projects with their own dependency graphs and licenses, and the
imported egui-based starter failed to compile against its pinned upstream crates on the
development machine. Rewriting either application to use the contract would have taken
weeks before one workflow could be demonstrated, contrary to the roadmap's preference for
a complete vertical slice.

The platform also had no UI framework decision. Adding a native toolkit (egui, iced,
Slint) would add a large dependency tree, a full-time rendering and input model, and a
platform accessibility story before the document and kernel boundaries were exercised by
a real workflow.

## Decision

`cad-studio` is a small, dependency-free Rust HTTP server plus a static browser workspace:

- The server owns the authoritative document. It uses the standard library's TCP listener,
  a bounded request parser, and `serde_json` for typed messages. It has no web framework,
  no async runtime, and no database.
- The browser owns presentation and input: a WebGL 1 viewport draws the kernel's
  tessellation, and DOM panels edit the same document through the HTTP API.
- All geometry runs in the Rust process through `cad-kernel-truck`. The browser never
  computes mass properties or edits the project file directly.
- The wire format is a separately serialised view model (`StateView`), so kernel handles
  and internal types never leak into the browser.
- Security defaults are part of the shell: loopback binding, `Host`/`Origin` validation on
  every request, a 1 MiB body cap, a closed route table, and rejection of path traversal in
  static assets.

## Consequences

**Good**

- One workflow can be completed and verified end to end without adopting an unproven UI
  stack.
- The browser is already installed and provides accessible, high-DPI, themable UI for free.
- The document, kernel, and API boundaries are exercised by an actual consumer, which is
  what makes later native front ends cheaper rather than more expensive.
- The server is testable with plain HTTP integration tests; no GUI driver is required.
- Startup is a few milliseconds and the binary stays small.

**Costs and risks**

- A browser tab is not a native desktop window. File dialogs, OS menus, and OS-level
  document integration are absent; projects are passed as paths on the command line.
- The HTTP transport is acceptable for loopback but is not the intended transport for a
  remote or multi-user deployment. Authentication, TLS, and tenancy are explicitly out of
  scope and must not be inferred from this decision.
- Two rendering paths could diverge if a native front end is added later. The view model
  in `cad-studio` is therefore the contract a second front end should consume.
- WebGL 1 caps triangle counts for very large models; the current document is a single
  extrusion, so this is not yet measured at assembly scale.

## Rejected alternatives

- **Adopt egui/iced now.** Rejected for this increment: it adds a large dependency tree and
  a new input/rendering model before the product workflow exists, and the imported app
  checkouts could not be built without upstream churn.
- **Serve only the CLI.** Rejected: it does not give beginners discoverability, direct
  manipulation, or a feature tree, which are core product requirements.
- **Cloud-hosted app now.** Rejected: remote geometry hosting, accounts, and billing are
  later milestones with unresolved cost and tenancy questions; local-first is the stated
  default.

## Follow-ups

- Add typed API versioning before any remote transport is considered.
- Revisit a native shell after Stage A is complete and after measuring where the browser
  shell actually constrains users.

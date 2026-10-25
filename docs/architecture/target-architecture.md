# Target Architecture

**Rule:** this is a direction, not permission to duplicate working code. The existing layering is in
[architecture.md](../architecture.md); this page states what the platform is moving toward and what
already exists.

## 1. Layering (current, with the control plane and the agent platform added)

```text
applications            CADCraft (2D desktop/CLI/MCP)   SolidCraft (3D desktop/web/CLI/MCP)
                                                                  │
integration clients     cad-cli ──────────┐      cad-studio (loopback HTTP + WebGL UI)
                                          ▼                   ▼
project payload         cad-document (parameters, sketch, feature, `.zcad` save/load)
                         │
contract                cad-contract  ◄── cad-kernel-truck    SolidCraft kernel (94,803 LOC)
(units, ids, tolerance,       ▲                                  │
 capabilities, errors,        │                                  ▼
 mesh, document header)       │                            truck 0.6 + vendored patches
                               │
codec                   cad-io (validated binary STL; CLI and studio)

agent contract          cad-agent-api  ◄── cad-agent-geometry   (the first agent)
                               ▲                │
                               │                └── extrude / boolean / tessellate / measure
                               │
agent host              cad-agent-runtime  (registry, lifecycle, grants, budgets, schema check)
                               ▲
                               │  (dev-dependency only: the edge is the test harness)
                        cad-agent-geometry's tests

engineering             cad-engineering  (manifests, scanner, tasks, report)
```

Dependency rules, enforced by review today and by the manifests' cross-reference validation:

* `cad-contract` depends on no CAD crate and no kernel.
* An adapter (`cad-kernel-truck`, a future OCCT adapter) may depend on `cad-contract`.
* Applications may depend on both. Nothing in the contract depends on an adapter, which is what makes
  a kernel swap possible.
* `cad-agent-api` depends only on `serde`/`serde_json`/`thiserror`. An agent may depend on it and on
  the code it computes with, but **never on `cad-agent-runtime`**: the runtime is the host, and a
  host that its agents cannot be moved away from is not a host.
* `cad-engineering` depends only on `serde`/`serde_json`/`thiserror`; it must remain runnable when
  nothing else builds.

## 2. What the target adds, in order

1. **Expand the persistent document payload**: schema 2 supports named parameters, a rectangle,
   circle or regular-polygon sketch, one extrude feature, bounded undo/redo, atomic save, validated
   load, schema-1 migration and CLI/studio STL export. Unknown fields and newer schemas are refused.
   General sketch constraints, multiple dependent features, persistent named revisions and recovery
   snapshots remain future work.
2. **Job execution boundary**: geometry work off the UI thread, bounded parallelism, deadlines.
   The agent runtime already carries the deadline and cancellation *policy*; what it does not have is
   a scheduler, so a long operation still blocks the calling thread.
3. **A second agent and a CLI host** (roadmap M9): the platform is proved by a second agent with a
   different permission profile, not by more tests of the first.
4. **SaaS boundary** only after 1–3: auth, ownership, storage, metering, entitlements, billing.
   Interfaces are defined in the product requirements; none is implemented.

**Done in this cycle:** the first application (Atlas Studio) over the project payload, the shared STL
codec, circle/polygon sketch profiles, schema-1 migration, and the application security envelope
(loopback binding, same-origin checks, body/header limits, timeouts, bounded concurrency). The agent
platform and the kernel adapter's cancellation/deadline support were delivered in the previous cycle.

## 3. Explicit non-goals

* Splitting into microservices. One workspace, one process family, no independent scaling need.
* Merging CADCraft and SolidCraft into one crate tree. They have incompatible document models and
  release cadences; the contract is the integration point.
* A second geometry kernel while the first has known boolean limits and no corpus.
* Rewriting the 42 SolidCraft Python helper scripts: they are tooling, not shipped code.
* Loading third-party native code in-process. The manifest records process isolation as the
  strategy for code the project does not own; it is not implemented yet, and nothing third-party is
  loaded today.

## 4. How this stays maintainable

* Every module in `engineering/manifest/modules.json` declares boundaries, public API, consumers and
  evidence.
* A new repository enters through the workflow in
  [engineering-control-plane.md](../engineering-control-plane.md) §11, not by copying it into an
  existing tree.
* Capability claims are validated against evidence; the report lists every advertised-but-unverified
  capability each time it is regenerated.

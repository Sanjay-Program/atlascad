# AtlasCAD

AtlasCAD is a Rust CAD engineering workspace. It brings together a small, headless platform layer, a Truck-backed geometry adapter, engineering and agent APIs, and separately maintained CAD applications and kernel projects used for integration work and comparison.

The root Rust workspace is the actively developed integration surface. The application and reference directories retain their own project layouts and licenses. Their presence here does not mean that every feature in those projects is integrated into the root platform.

## What is here

| Path | Purpose |
| --- | --- |
| `crates/cad-contract` | Shared units, IDs, tolerances, capabilities, errors, mesh and document-header types, and the kernel trait. |
| `crates/cad-kernel-truck` | Adapter implementing the shared kernel contract with Truck geometry crates. |
| `crates/cad-document` | Document and revision primitives. |
| `crates/cad-drafting` | 2D drawing document: layers, point/line/circle/arc/polyline/text/dimension entities, undo, versioned `.zdraft` save/load, DXF R12 reading and writing, sheet layout (A4–A0, Letter) with a title block, and a vector PDF plotter. |
| `crates/cad-cli` | Headless command-line client for exercising the document and geometry path and exporting STL. |
| `crates/cad-io` | Validated binary STL reading and writing shared by the CLI and the application. |
| `crates/cad-studio` | Atlas Studio: the local-first CAD application, its HTTP API, and its browser workspace (WebGL viewport, feature tree, parameters, command palette). |
| `crates/cad-agent-api`, `cad-agent-runtime`, `cad-agent-geometry` | Agent contracts, permission-aware runtime, and a geometry agent. |
| `crates/cad-engineering` | Repository inventory, validation, scan, task, and report tooling. |
| `crates/cad-entitlements` | Entitlement primitives. |
| `apps/cadcraft` | CADCraft 2D drafting application source checkout. |
| `apps/solidcraft` | SolidCraft 3D parametric CAD application source checkout, including the patched Truck crates used by the root workspace. |
| `kernels/` | Geometry kernel source checkouts used for evaluation and comparison. |
| `references/` | Additional CAD and visualization source checkouts used as references. |
| `docs/` | Architecture decisions, integration and license audits, research, measured baselines, and roadmap. |
| `engineering/` | Machine-readable project inventory, task registry, quality evidence, and generated reports. |

The root workspace excludes `apps`, `kernels`, and `references` from Cargo workspace discovery. Those projects have independent manifests and dependency graphs.

The source checkouts were consolidated into this repository as a snapshot. Their original Git metadata is not included; the upstream URLs and exact source revisions used for this import are recorded in [`docs/integration/source-snapshot.md`](docs/integration/source-snapshot.md). Their in-tree license and attribution files remain with the source.

The project owner reports that development began about a year before the work was first placed under Git. Git history starts with the consolidated import commit dated 2026-10-24, so it records when this snapshot entered Git, not the day-by-day development timeline. No dated intermediate snapshots were available to represent that earlier work as separate commits.

## Current platform status

The root platform has a versioned shared contract, a Truck adapter, a headless consumer, an engineering control plane, a small agent platform, and a working local CAD application. The root-workspace gate passes 359 tests across twelve packages on the recorded machine; startup, request latency, and export timings are in [`docs/performance-baseline.md`](docs/performance-baseline.md).

Stage A of the CAD roadmap is **partially implemented**. `cad-studio` supports an ordered feature graph: rectangle, circle, regular-polygon, drawn-polyline and arc-sector sketches, each on a horizontal plane at any height; extrude features that start a body, join it with a kernel union, or remove material with a kernel difference (cut), swept up or down from their sketch plane; full-turn revolve features that turn a sketch about one of its own axes; parameter and sketch-placement edits that rebuild every dependent feature; a per-feature rebuild cache so an unchanged or last-feature edit does not re-run the whole history; crash recovery that snapshots unsaved edits next to the project and restores them on the next start, including when the project file itself cannot be read; per-feature rebuild status with failure reporting; undo/redo; save, reopen, and binary STL export. A `.zcad` project written by any earlier schema opens through migration. Alongside the model, the studio has a **drawing editor**: a plan of the sketch you can draw on (line, circle, polyline, point, dimension, text) with layers, undo, save to a `.zdraft` beside the project, DXF R12 download, and **plot to a PDF sheet** on A4–A0 or Letter with a title block and overall dimensions. `cad-drafting` also reads a DXF written by another program (layers, colours, off/locked state, units, polylines and text, with unsupported entity types counted rather than dropped). It does **not** yet provide sketch constraints, dimensions *in the sketch*, sweeps or lofts, partial revolves, fillets, multi-body, assemblies, projected/section views, GD&T, a BOM, STL import, or STEP/IGES interchange; DXF and PDF are the only formats it writes or reads. Those remain planned work in [`docs/roadmap.md`](docs/roadmap.md).

The CADCraft and SolidCraft desktop applications are not wired to the shared contract. The root adapter advertises only the capabilities its tests exercise. Known kernel limitations and measured behavior are documented in `docs/kernel-comparison.md` and `docs/performance-baseline.md`.

## Requirements

- Rust toolchain compatible with the workspace `rust-version` and edition in `Cargo.toml` (the recorded baseline used Rust 1.94).
- A working C/C++ build toolchain may be needed by native dependencies.
- Disk space for Cargo dependencies and build artifacts. Build output is written under `target/` and is ignored by Git.

## Run it on your laptop

```sh
cargo build --release -p cad-studio -p cad-cli -p cad-engineering -j 2

# Create a project and open the studio. It prints the URL to open.
./target/release/cad-studio --new ~/bracket.zcad --name bracket --size 40,30,10
```

Open the printed `http://127.0.0.1:7519` in a browser. Then, in the page:

1. **Create** — *Add sketch* (rectangle, circle, polygon or drawn polyline) with an origin.
2. **Build** — pick the sketch, choose *extrude* or *revolve* and an operation (`new body`, `join`,
   `cut`), then *Add extrude*/*Add revolve*.
3. **Edit** — change a value under **Parameters** (or move a sketch under **Sketch placement**); the
   whole history rebuilds and the viewport, tree, properties and volume update together.
4. **Inspect** — drag to orbit, shift-drag to pan, scroll to zoom, click a face to read its position
   and normal; `Iso`/`Front`/`Top`/`Right`, `Fit`, `Wire`, `Grid` are in the viewport toolbar.
5. **Save / export** — `Ctrl/Cmd+S` writes the `.zcad` file; *Export STL* downloads a binary STL.

Reopen a project later with `./target/release/cad-studio --file ~/bracket.zcad`. Unsaved edits are
snapshotted next to the file, so a crash or `kill -9` reopens with the work restored and a banner to
keep or discard it. `--port 7600` picks another port.

### Share a session with a colleague

```sh
./target/release/cad-studio --file ~/bracket.zcad --share
#   url:   http://127.0.0.1:7519/?token=<generated>
#   share: http://192.168.1.20:7519/?token=<generated>
```

Send the `share:` link. Anyone on the same network who opens it can **view and edit** the open
document until the studio stops (the page shows a **Share** panel with the link to copy). The token is
required on every API call and is generated per session; `--token <32+ chars>` fixes one,
`--advertise name.local` sets the host in the link, and `--host 0.0.0.0` does the same as `--share`
without changing the printed host. Loopback-only use is unchanged: no token, no link. Do not expose
the port to the internet — this is a LAN convenience, not a hosted service.

## Build and verify

The whole gate — formatting, lints, tests, the analytic kernel corpus, manifest validation, the
dependency licence policy, the SBOM check and a live simulation of the real release server — is one
command:

```sh
./scripts/gate.sh          # ~6 s warm on the recorded host
./scripts/gate.sh --quick  # skip the simulation while editing
```

To build a verified, deployable bundle (tarball with the three binaries, the licence and quality
documents, a CycloneDX SBOM and `SHA256SUMS`):

```sh
./scripts/package.sh       # writes dist/atlas-cad-<version>-<target>.tar.gz
```

The individual commands, if you want them:

```sh
cargo fmt --all --check
cargo clippy --offline --workspace --all-targets -j 2 -- -D warnings
cargo test --offline --workspace -j 2
cargo build --offline --workspace --all-targets -j 2
```

The release process, including what is still missing before a commercial release (signing,
notarisation, an update channel, advisory monitoring), is in
[`docs/operations/release-process.md`](docs/operations/release-process.md).

The offline commands require dependencies to have been fetched already. For a first build with network access, omit `--offline` or run `cargo fetch` first. The application workspaces are substantially larger than the root platform; their own build guidance is in each application's README and in `docs/build-and-test.md`.

### CLI examples

```sh
cargo run -p cad-cli -- box --size 10,10,5 --out box.stl --header box.json
cargo run -p cad-cli -- union --a 10,10,10 --b 4,4,10 --offset 8,8,0 --out union.stl
cargo run -p cad-cli -- draft --file part.zcad --save part-plan.zdraft --out part-plan.dxf
cargo run -p cad-cli -- dxf-import --from supplier.dxf --save supplier.zdraft --out normalised.dxf
cargo run -p cad-cli -- sheet --file part.zcad --pdf part-a3.pdf --paper A3 --title "Bracket plate" --number AC-1001
```

See `cargo run -p cad-cli -- --help` for the current command syntax. STL files and JSON headers produced by these examples are written to the current directory.

### Atlas Studio (local CAD application)

```sh
cargo run -p cad-studio -- --new model.zcad
# Open http://127.0.0.1:7519

cargo run -p cad-studio -- --file model.zcad --port 7600
```

The server binds `127.0.0.1` only, rejects foreign `Host` and `Origin` headers, caps request bodies at 1 MiB, and makes no network calls or telemetry requests. Projects are plain JSON in the versioned `.zcad` format (schema 8; earlier schemas migrate on load); binary STL export uses `cad-io` and is validated before it is written. Sketch profiles are in millimetres and extrusion is limited to 0.1–10,000 mm. The first feature starts the body and later features join or cut it, where a feature is an extrude or a full turn of the sketch about one of its own axes; a feature that cannot rebuild is marked with the kernel's message and blocks export instead of writing a file that no longer matches the model. Rebuilds are cached per feature, so an unchanged request does no kernel work and an edit re-runs only the features that read it. The **Drawing** panel is an editor: arm Line, Circle, Poly, Point, Dim or Text, click in the plan, and each shape is validated and added by the drawing document; undo, delete, layers (on/off, locked) and **Save drawing** write a `.zdraft` beside the project. **DXF** downloads R12, and **Plot PDF** lays the drawing out on a titled sheet (paper, orientation, scale or `fit`, title, number, author). The panel says whether it is showing the generated plan (`model (live)`, which follows the sketch) or a saved drawing (`file`), which the model no longer overwrites. Drawing never touches the kernel, so it still works when a feature fails to rebuild.

| Shortcut | Action |
| --- | --- |
| `Ctrl/Cmd+S` | Save |
| `Ctrl/Cmd+Z` / `Shift+Ctrl/Cmd+Z` or `Ctrl/Cmd+Y` | Undo / redo |
| `Ctrl/Cmd+K` | Command palette (includes the drawing tools, save drawing and plot PDF) |
| `F` | Frame model |
| `W` / `G` | Toggle wireframe / grid |
| `1` / `2` / `3` / `4` | Front / top / right / isometric view |
| `?` | Shortcut help |
| `Esc` | Close a panel or the palette |

Mouse: drag to orbit, shift-drag to pan, wheel to zoom, click a solid face to select and measure it.
All creation commands (new sketch, extrude as new body, extrude as join, save, export) are in the
command palette; the model tree is the ordered history and selecting a row drives the properties
panel.

### Engineering tools

```sh
cargo run -p cad-engineering --bin atlas-eng -- validate
cargo run -p cad-engineering --bin atlas-eng -- scan --write
cargo run -p cad-engineering --bin atlas-eng -- report
cargo run -p cad-engineering --bin atlas-eng -- tasks
```

Read `docs/engineering-control-plane.md` before editing the curated manifests or generated state. The scanner is designed to observe the workspace; it must not rewrite the curated manifest.

## Design and project records

- [`docs/architecture.md`](docs/architecture.md) describes the current architecture and integration boundaries.
- [`docs/architecture-decisions/`](docs/architecture-decisions/) records decisions and their rationale.
- [`docs/roadmap.md`](docs/roadmap.md) tracks proposed milestones and open decision gates.
- [`docs/repository-audit.md`](docs/repository-audit.md) and [`docs/integration/repository-catalog.md`](docs/integration/repository-catalog.md) describe the inspected source checkouts.
- [`docs/license-inventory.md`](docs/license-inventory.md) and [`docs/integration/license-audit.md`](docs/integration/license-audit.md) record license findings and constraints.
- [`docs/build-and-test.md`](docs/build-and-test.md), [`docs/quality/`](docs/quality/), and `engineering/quality/` contain dated build, test, and benchmark evidence.
- [`docs/research/`](docs/research/) contains the local competitor, feature, and user-needs research, including [`CAD_CAPABILITY_MATRIX.md`](docs/research/CAD_CAPABILITY_MATRIX.md), [`INDUSTRY_TOOLSETS.md`](docs/research/INDUSTRY_TOOLSETS.md) and [`API_AND_PLUGIN_COMPATIBILITY.md`](docs/research/API_AND_PLUGIN_COMPATIBILITY.md).
- [`docs/platform/SYSTEM_REQUIREMENTS.md`](docs/platform/SYSTEM_REQUIREMENTS.md) records the verified minimum, recommended and advanced tiers, the platform support status, and the profiles and diagnostics utility that do **not** exist yet.
- [`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md) is the per-feature status ladder: what is verified end to end, what is measured, and what is only researched or planned.
- [`docs/business/commercial-readiness.md`](docs/business/commercial-readiness.md) records the market position, pricing hypothesis, and staged sellable-release plan; [`docs/business/packaging-and-deployment.md`](docs/business/packaging-and-deployment.md) records the measured SolidCraft build spike, the verified reference-part/STEP evidence, the packaging and signing blockers, and the ordered deployable gate. Neither is legal or financial advice.

Research and measurement claims are scoped to the cited local records and their dates. A recorded result is not a guarantee for another machine or a later revision.

## Source and licensing

The root platform crates and AtlasCAD-authored project documentation are licensed for personal, non-commercial use under [`LICENSE`](LICENSE). Commercial use is available only under a separate written agreement; see [`COMMERCIAL-LICENSE.md`](COMMERCIAL-LICENSE.md). This license does not apply to the imported application, kernel, and reference directories; those retain their own licenses and notices. **Do not treat this whole tree as covered by one license.** Check the license file and notices in each component before copying, redistributing, or combining its code. Commercial use of the AtlasCAD original materials requires prior written permission from the rights holder.

`OpenCADStudio-main.zip` is intentionally excluded. Its GPL-3.0 source archive is not part of this workspace's code. The archive must not be unpacked into, or copied into, this repository without a deliberate licensing decision.

## Contributing

Keep changes scoped to the project that owns the code. Preserve existing attribution, license files, and upstream notices in the application and reference checkouts. Record new measurements with the exact command, environment, and date; distinguish measured results from plans and unverified upstream claims. Do not add generated build output, credentials, or fabricated history to commits.

Before proposing an integration change, run the relevant root-workspace checks and update the architecture, license, or quality records when the change affects them. The root workspace favors safe Rust, bounded geometry inputs, explicit kernel capabilities, and errors over silent fallback behavior.

# System Requirements and Compatibility

**As of:** 2026-10-30. **Rule for this document:** every requirement is tied to a workload that was
measured on a named configuration, or it is marked **not measured**. A number that was not measured is
not a requirement; it is a question.

## 1. The recorded development host

| Part | Value | Source |
|---|---|---|
| CPU | Intel Core i3-1125G4 (4 cores / 8 threads) | `engineering/reports/latest-integration-report.md` header (8 CPUs observed) and `docs/architecture/current-state.md` §3 |
| RAM | 7.4 GB usable | same |
| GPU | Intel UHD integrated, no discrete GPU | same |
| OS | Linux (Fedora-family), rustc 1.95.0, Chrome available | measured runs in `engineering/quality/` |
| Disk | SSD (build and test runs are seconds, not minutes) | warm workspace test run 1.1 s for `cad-document`, 0.3 s for the HTTP suite |

This host **is** the low-resource target the brief names (i3-class CPU, 8 GB RAM, SSD, integrated
graphics). The platform workspace builds and tests on it; one SolidCraft crate build was recorded at a
2.56 GB peak RSS, which is why the imported applications are not built here.

## 2. Three tiers

### Minimum — application startup and the specified basic workflows

**Measured on the recorded host.** The workflows covered are: open or create a `.zcad` project, build
a plate with one sketch and one extrude, edit a parameter, add a second sketch and a join or cut, undo
and redo, save, reopen, export binary STL, and load the page in a browser with a working WebGL 1
context.

| Requirement | Value | Why this value |
|---|---|---|
| CPU | 2 cores, x86-64 with SSE2 | the workspace builds and runs on 8 threads with `-j 2`; no measured single-core floor exists below 2 |
| RAM | 2 GB free for the studio process | the largest measured document is a 32-feature history at 69 MB resident with the cache (`docs/performance-baseline.md` §10); the build toolchain is the larger consumer, and is not required to *run* a release binary |
| Disk | 500 MB for the application + project files; a project is capped at 64 MiB (`MAX_PROJECT_BYTES`) | measured cap, enforced on load |
| GPU | integrated graphics with WebGL 1; no discrete GPU required | the headless browser run uses `--disable-gpu --enable-unsafe-swiftshader` and still renders and passes DOM assertions |
| Display | 980 px wide is the breakpoint where the inspectors become drawers; 392 px was checked manually | recorded browser checks at 392 px and full width |
| Runtime libraries | a browser with WebGL 1; no AI, cloud or simulator dependencies | no outbound network call is made by the studio |

**Not measured at this tier:** any figure for a drawing larger than 10,460 triangles; any figure for a
point cloud; any Windows or macOS run (see §4).

### Recommended — comfortable everyday work

| Requirement | Value | Basis |
|---|---|---|
| CPU | 4 cores | the six-feature cold rebuild is 32.77 ms and a cached edit 8.81 ms on this host; the cache is what makes an interactive history possible, and it is single-threaded today, so cores do not yet help beyond the browser and the OS |
| RAM | 8 GB | the recorded host's 7.4 GB is enough for the whole root workspace gate, including clippy and tests |
| Disk | 2 GB free for the workspace plus build artefacts | `target/` for the eleven-package workspace; the imported applications need substantially more and are excluded |
| GPU | any WebGL 1-capable integrated GPU | the viewport is WebGL 1 with a fallback message when the context cannot start |
| Display | 1440 × 900 or larger | the recorded browser check window size |

### Advanced engineering — large assemblies, large drawings, point clouds, simulation, high-quality rendering

**No AtlasCAD workload at this tier has been measured, because none of those features exists.** Every
value below would be a guess, so they are listed as open questions rather than requirements:

| Question | Would be answered by |
|---|---|
| Memory for a 100k-entity 2D drawing | a 2D entity model and a load benchmark (blocked: no 2D layer) |
| Memory and rebuild time for an assembly of 1,000 components | multi-body + assembly benchmarks (blocked: single body) |
| Point-cloud memory per million points | a point-cloud layer (absent) |
| Simulation time and memory | a solver (absent) |
| GPU needed for high-quality rendering | a renderer (absent and out of scope) |

The brief's instruction is followed literally here: a requirement that is not tied to a tested
workload is not published as a requirement.

## 3. Graphics and driver compatibility

* The viewport is **WebGL 1** (not WebGL 2, not Vulkan, not a native GPU API). It is served by the
  user's browser, so the graphics driver support is the browser's.
* Capability detection is by **context creation, not vendor string**: if the context fails, the page
  shows "Viewport unavailable" with the reason and the rest of the application (tables, tree,
  parameters, export) keeps working. There is no vendor-specific code path.
* The measured browser runs use SwiftShader software rendering, so the application is known to render
  correctly without hardware acceleration.
* **Not verified:** Windows, macOS, Wayland vs X11 differences, high-DPI behaviour, or multi-monitor.

## 4. Platform support status

| Platform | Status | Evidence |
|---|---|---|
| Linux x86-64 | **Tested** | every recorded build, test, benchmark and browser run in this repository |
| Windows | **Unverified** | no build or run recorded. The code avoids Unix-only APIs in the platform crates, but that is a code reading, not a test |
| macOS | **Unverified** | same |
| Linux ARM64 | **Unverified** | same |
| WASM (browser build of the engine) | **Unverified** | the target is not installed; `Capability::Wasm` is declared and unadvertised |

A release must not list Windows or macOS as supported until a build, a test run and a browser check
exist for each.

## 5. Resource profiles — status and intended behaviour

**Nothing in this table is implemented.** The brief requires that a profile change real behaviour, so
this is the design contract a profile must meet, with the current fixed behaviour recorded beside it.

| Profile | What it must change | Current behaviour (fixed) |
|---|---|---|
| Low-resource | smaller rebuild-cache bound, coarser tessellation, no background prefetch, cheapest viewport shading | cache bound is fixed at 32 prefixes (~1.3 MB each at 32 features); chord tolerance is the document's 0.05 mm; nothing runs in the background; shaded+wireframe only |
| Balanced | default | the above; this is the only behaviour that exists |
| Performance | larger cache bound, finer tessellation, fewer quality reductions | not implemented |
| Large-model | streaming/lazy meshes, level-of-detail, viewport culling, reduced undo snapshots | not implemented; the mesh is sent whole to the browser on every state response |
| Rendering | high-quality offline path, decoupled from the interactive viewport | not implemented |

Until these exist, the application makes **no adaptive-quality claim**: it is a fixed, low-resource
configuration that happens to run on modest hardware.

## 6. Diagnostics utility — status

**Absent.** There is no hardware/software probe, no report generator and no opt-out because nothing is
collected. When it is built it must:

1. collect only what the compatibility decision needs (OS and version, CPU architecture, RAM, free
   disk, WebGL context version, browser name/version, application build id, kernel name/version and
   capability set);
2. explain each item in plain language, including which requirement it is compared against;
3. be **off by default** for anything that leaves the machine, with an explicit opt-in and a preview of
   exactly what would be sent;
4. never include project content, file paths or geometry.

The state payload already exposes kernel name/version/capabilities and the document schema, which is the
part of a diagnostic report that exists today.

## 7. Network and optional dependencies

| Feature | Requirement | Current status |
|---|---|---|
| Studio | none; binds loopback | implemented; no outbound call |
| LAN sharing | one open TCP port; a ≥16-character token | implemented and tested |
| AI inference | none when disabled; a provider credential and internet when enabled | absent |
| Cloud sync / accounts | absent | absent |
| Simulation | absent | absent |

The application must remain fully usable with all optional services disabled. Today that is the only
mode that exists, so the requirement is met by construction rather than by a switch.

## 8. How to check a machine against this document

```sh
cargo test --offline --workspace -j 2          # the engine and document on this machine
cargo run --release -p cad-studio --example rebuild_bench   # rebuild timings for this CPU
cargo run --release -p cad-studio --example cache_memory -- cached 32   # peak RSS with a 32-prefix cache
cargo run --offline -p cad-studio -- --new /tmp/check.zcad --size 10,10,5  # then open the printed URL
node scripts/browser-check.mjs http://127.0.0.1:7519        # the page, the viewport and the panels
```

The first three produce numbers comparable to §1–§2; the last two prove that the graphics path works on
that machine. A machine that fails the browser check is not a supported configuration for the studio,
even if the engine tests pass.

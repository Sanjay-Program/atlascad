# CAE Capabilities

**As of:** 2026-11-03 · **Status:** linear static structural analysis is implemented, integrated,
validated against analytic references, and packaged as a batch deployment job. Every other analysis
type is absent.

Implementation: [`crates/cad-cae`](../../crates/cad-cae/src/lib.rs). CLI: `atlas cae static`.
Deployment: `deploy/Dockerfile.cae` and [`docs/operations/cae-deployment.md`](../operations/cae-deployment.md).
Evidence: 10 unit tests, 6 analytic integration tests, 4 CLI end-to-end tests, one container smoke
test, and the measured convergence series below.

## 1. What is implemented

| Capability | State | Note |
|---|---|---|
| Linear static analysis (small strain, isotropic, homogeneous) | implemented, tested | one material per study, selected from the shared library |
| Volumetric mesh from the tessellated solid | implemented, tested | regular voxel grid; solid voxels become 8-node hexahedra |
| Fixed and component-fixed supports | implemented, tested | selected by a global axis plane, not by a face id |
| Face loads | implemented, tested | a total force vector distributed equally over the plane's element nodes |
| Stiffness assembly | implemented, unit-tested | 2x2x2 Gauss integration; symmetry, rigid translation and uniform-strain tests |
| Solver | implemented, tested | Jacobi-preconditioned conjugate gradient, reported iterations and relative residual |
| Displacement, von Mises stress, strain energy, reactions | implemented, tested | stress and peak locations reported; equilibrium residual reported |
| Factor of safety | implemented, tested | yield strength ÷ maximum von Mises = a linear-elastic stress ratio only |
| Mesh quality guard | implemented, tested | open meshes refused; voxel volume more than 25% off the body refused |
| Provenance | implemented, end-to-end tested | `--record ID` pins a `simulation` artifact to the project revision |
| Versioned study file | implemented, tested | `--study study.json` (schema 1, 64 KiB, unknown fields refused) is the job input |
| Self-contained result | implemented, tested | the result embeds the study definition, engine version and source document/revision |
| Batch container job | implemented, smoke-tested | non-root, read-only root, no network, exit 0/1/2/3; see the deployment contract |
| Result label and warnings | implemented, tested | every result carries the quality label and its assumptions |
| Modal, thermal, buckling, nonlinear, dynamic analysis | **absent** | no code |
| Injection moulding, electronics cooling | **absent** | needs validated specialist solvers and data |
| Result field storage, contour viewer, mesh export | **absent** | only the scalar summary is returned and recorded |
| Adaptive refinement, boundary-fitted or curved elements | **absent** | the voxel mesh is the stated approximation |
| Contact, plasticity, hyperelasticity, creep, fatigue | **absent** | linear elastic only |

## 2. How a study runs

```sh
# A person at a terminal: the study is a flag list.
atlas cae static --file bracket.zcad --voxel 2 --material steel.1018 \
    --fix x=0 --load x=40 --force 0,0,-250 \
    --out result.json --record 1 --label "bracket static"

# A deployment job: the study is a versioned file, and they cannot be mixed.
atlas cae static --file bracket.zcad --study bracket-static.json \
    --out result.json --record 1
```

* `--fix AXIS=COORD[@TOL]` fixes all three translations on a plane.
* `--hold AXIS=COORD:COMPONENT` fixes one translational component (a symmetry plane or roller).
* `--load AXIS=COORD[@TOL] --force X,Y,Z` applies a total force in newtons.
* `--voxel MM` is the element edge length; the grid starts at the body's bounding-box minimum.
* `--study FILE` reads the versioned job input instead of the flags; mixing the two is refused.
* `--record ID` records or replaces a `simulation` artifact at the project's current revision and
  saves the project, so `atlas artifacts check` can gate on it later.

The study inputs (voxel size, material id, supports, loads, solver controls) are a serialisable
`StaticStudy`. The result is a `StaticResult` with the mesh summary, the solver evidence and the
engineering numbers. The result file also embeds the **study definition**, the **engine version**
and the **source document/revision**, so it reproduces itself without a sidecar; the artifact record
keeps the summary and warnings, not the payload. A job's exit code is 0 success, 1 internal or I/O
fault, 2 bad input and 3 analysis refused.

## 3. Validation

### 3.1 Exact patch test — uniaxial tension

A 40 × 10 × 10 mm bar, four elements, one element across each lateral direction, so the four equal
nodal forces on the loaded face are the consistent load vector for a uniform traction. The supports
are the three symmetry planes, which the uniform-strain solution satisfies exactly.

| Quantity | Analytic | Solver |
|---|---|---|
| Maximum von Mises | 10 MPa | 10 MPa (relative error < 1e-8) |
| Strain energy | 1.0 N·mm | 1.0 N·mm (relative error < 1e-8) |
| Axial extension at 40 mm | 10 MPa × 40 mm ÷ 205 GPa = 1.9512e-3 mm | max displacement just above it (lateral contraction included) |
| Equilibrium `|applied + reaction| / |applied|` | 0 | < 1e-8 |

### 3.2 Cantilever convergence — Timoshenko beam

60 × 10 × 10 mm cantilever, 100 N transverse at the free end, AISI 1018 steel from the library
(E = 205 GPa, ν = 0.29). Reference: `PL³/(3EI) + PL/((5/6)GA) = 0.043052 mm`.

Measured with the release CLI on this host:

| Voxel | Elements | DOF | PCG iterations | Max displacement | Error vs reference | Wall clock |
|---:|---:|---:|---:|---:|---:|---:|
| 5.0 mm | 48 | 351 | 51 | 0.037498 mm | −12.90% | 0.00 s |
| 2.5 mm | 384 | 1 875 | 107 | 0.041352 mm | −3.95% | 0.01 s |
| 1.25 mm | 3 072 | 11 907 | 214 | 0.042513 mm | −1.25% | 0.13 s |

The error falls with refinement, as a fully integrated hex8 mesh should: the element is stiff in
bending, so the result approaches the beam solution from below. The test asserts that the 2.5 mm
result is closer than the 5 mm one and within 10% of the reference. Wall clock includes process
startup and is an order-of-magnitude figure, not a benchmark record.

The **maximum von Mises stress in the same series is 14.34 → 24.34 → 29.53 MPa**. It has not
converged: the idealised fully fixed end has a stress singularity, so a peak stress at a support,
corner or sharp re-entrant feature is mesh-dependent. This is exactly why the result is labelled a
preliminary study.

### 3.3 Refusals and diagnostics

The tests assert that each of these fails with a stated reason instead of producing a number:

* a mesh with open edges (a structural analysis needs a solid body);
* a voxel mesh whose volume differs from the body by more than 25%;
* a material id absent from the library, or one without a modulus or Poisson ratio;
* a support or load plane that selects no element node;
* a study with no supports, no loads, a zero load, or a non-positive voxel size;
* supports that constrain every degree of freedom;
* a solver that breaks down or does not reach the requested residual.

### 3.4 Container job

`deploy/build-cae-image.sh --smoke` builds the image and runs the hardened shape (no network,
read-only root, all capabilities dropped, 512 MB cap, two volumes). It asserts:

* a 384-element cantilever is within 10 % of the Timoshenko reference (measured 0.041352 mm,
  −3.95 %, 111 PCG iterations); the result embeds its study, engine and source revision;
* the read-only project file is byte-identical afterwards;
* an off-body load plane exits **3** and publishes no result; a malformed study exits **2**;
* the process runs as uid 10001, and a job without a result volume exits **2** rather than writing
  into an anonymous volume;
* recording a `simulation` inside the container leaves `atlas artifacts check` passing.

The measured resource envelope (64 000 elements in 7.8 s / 20 MB; 160 000 elements in 22.9 s / 43 MB;
the same 160 000-element study in 24.1 s inside the container under a 512 MB cap) is in
[`docs/operations/cae-deployment.md`](../operations/cae-deployment.md) §5. That page also states the
retry policy, the orchestration example that was **not** run on a cluster, and the security posture.

### 3.5 Unit tests of the element and solver

* the 24 × 24 element stiffness matrix is symmetric;
* a rigid-body translation produces zero internal force;
* a prescribed uniform uniaxial strain recovers the analytic stress in all three normal components;
* a ray crossing a triangle is counted once, ahead of the origin only, and a parallel triangle is not
  crossed (the point-in-solid classifier's foundation).

## 4. How to reproduce

```sh
cargo test --offline -p cad-cae -p cad-cli
cargo build --offline --release -p cad-cli
./target/release/atlas project new --file /tmp/cant.zcad --size 60,10,10
./target/release/atlas cae static --file /tmp/cant.zcad --voxel 2.5 \
    --material steel.1018 --fix x=0 --load x=60 --force 0,0,-100 \
    --out /tmp/result.json

# The deployment job, including the hardened runtime and the refusals.
deploy/build-cae-image.sh --smoke
```

## 5. Explicit claim boundaries

* **Not a certified analysis.** Every result carries
  `preliminary engineering study - linear elastic, not a certified analysis`.
* The mesh is a stair-step voxel approximation. Sloped and curved surfaces are quantised; a fillet,
  a hole and a thin wall need a voxel size well below the feature size, and the volume-error guard
  only catches gross mistakes.
* Loads are lumped equally over the selected nodes, not a consistent traction; local stress and
  deflection at the load face are approximations.
* Plane selections cannot target one curved face, one hole or one fillet.
* Hex8 elements are stiff in bending on coarse meshes, and nodal displacement is first-order.
* Peak stress at a fixed support is singular and mesh-dependent; the reported maximum is a mesh
  artifact if read as a design stress.
* The factor of safety is yield ÷ maximum von Mises on an idealised geometry. It ignores stress
  concentration at the real part, fatigue, buckling, residual stress, surface finish, temperature and
  material qualification.
* Material values are typical handbook figures, not a supplier specification; there are no
  temperature-dependent properties.
* No modal, thermal, buckling, nonlinear, dynamic, moulding or cooling analysis exists. No result is
  a substitute for a physical test.
* The solver is local. It makes no network call and costs nothing per run. It is packaged as a batch
  container, but that container is not an orchestrator: there is no queue, submission API, auth,
  internal timeout or cancellation. A very fine mesh is bounded by the 250 000-element /
  400 000-node limits and by local memory/time; see the deployment contract for the measured
  envelope and the exit-code/retry policy.
* The image is unsigned, unscanned and has no custom seccomp profile; the document and study parsers
  have not been fuzzed. Bounded inputs and a hardened runtime are not a security review.

## 6. Safety and manufacturing

A simulation result is not machine output and cannot drive a machine. No CAM or postprocessor exists
in this repository, so [`docs/manufacturing/MACHINE_SAFETY.md`](../manufacturing/MACHINE_SAFETY.md)
still governs: no generated program may reach a physical machine.

A linear static study must not be used as a release justification for a safety-critical part. It can
inform a preliminary sizing decision, and it is now trustworthy enough to exercise the artifact
provenance gate: a recorded study becomes stale when the design changes, and — unlike a drawing — it
cannot be released by review alone.

## 7. Next

1. Modal analysis on the same mesh (natural frequencies and mode shapes), with a known Euler–Bernoulli
   frequency reference and a convergence series.
2. Steady-state thermal analysis, then thermal stress coupling.
3. Store the displacement/stress field as a derived payload with a viewer, instead of only the
   summary, and measure result size and load time.
4. A boundary-fitted or tetrahedral mesher to remove the stair-step approximation, with element
   quality metrics.
5. Linear eigenvalue buckling only after the modal path is validated; post-buckling requires an
   established nonlinear solver.

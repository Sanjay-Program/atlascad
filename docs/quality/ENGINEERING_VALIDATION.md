# Engineering Validation

**As of:** 2026-11-03 · **Status:** Phase A, linear static CAE and its batch deployment job have
executable evidence; the other engineering domains have none because they are not implemented.

This page records how an engineering capability is allowed to move from "code exists" to "trusted",
and the evidence for what is implemented. The general status ladder remains
[`docs/roadmap/FEATURE_IMPLEMENTATION_STATUS.md`](../roadmap/FEATURE_IMPLEMENTATION_STATUS.md).

## 1. Rules

* A test must exercise the real implementation on a real input. A mocked solver, a placeholder result
  or a button-presence assertion does not promote a feature.
* Every result must be comparable to something independent: an analytic value, a published reference
  case, a conservation law, or a second implementation. Comparing a program to itself proves nothing.
* The test must be able to fail. A test that passes for any output is not evidence.
* Performance claims need a named host, workload and method. A debug-build in-process number is not
  an HTTP response time.
* Licensing and safety findings are part of validation, not an afterthought.
* A stale engineering result may not be released by acknowledgement alone when it makes a physical
  claim. Review is not recomputation.

## 2. Phase A evidence (2026-11-02, extended 2026-11-03)

### Command

```sh
cargo test --offline -p cad-engineering-model -p cad-document -p cad-cli -p cad-studio
```

### What is asserted

| Area | Assertion | Where |
|---|---|---|
| Material validation | impossible density, modulus, Poisson ratio and empty source are refused; duplicate ids (case-insensitive) are refused; all seven built-ins validate and state their non-certified source | `cad-engineering-model` unit tests |
| Artifact identity | zero and duplicate ids are refused; an artifact cannot belong to another document | unit tests |
| Dependency graph | unknown upstream, self-reference, two-node cycle and a hand-edited cycle are refused | unit tests |
| Staleness | equal revision is `current`; a later revision is `requires_review`; a chain propagates; a current artifact on a stale upstream is `blocked_by_upstream` | unit tests |
| Review semantics | review records the revision, does not change the source revision, keeps `is_current() == false`, and cannot predate the source | unit tests |
| Release policy | a reviewed simulation is `accepted_after_review` but **not** releasable and the gate still requires action; a reviewed drawing is releasable; a current result is releasable whatever its kind; a drawing depending on a policy-blocked study is `blocked_by_upstream` | unit tests |
| Fingerprint | same revision with different content becomes `requires_recompute`; missing fingerprints fall back to the revision number | unit test |
| Persistence | JSON round trip, unknown-field refusal, newer-schema refusal, 8 MiB limit, zero-revision refusal, atomic save with no temporary left behind | unit tests |
| Project-embedded registry | a recorded artifact survives save/reopen; a later design edit reports `requires_review`; review reports `accepted_after_review` with the original source revision; an artifact for another document is refused; a schema-8 project opens with no records | `cad-document` unit tests |
| CLI gate | create a project, record a drawing and a dependent toolpath, check passes; edit the model, check exits **3**; review the drawing, the drawing is releasable but the toolpath is not; review the toolpath too, the gate **still exits 3** because a toolpath is recompute-only; source revisions remain unchanged | `cad-cli/tests/artifacts_workflow.rs` |
| CLI refusal | an unknown upstream is refused and no file is written; an unknown artifact kind is refused | same file |
| Studio integration | a generated drawing is `current`; a saved drawing is pinned; a model edit makes it `requires_review`; `/api/engineering/review` acknowledges it and the saved project file contains the record; `/api/engineering` reports the same state plus seven materials marked non-certified | `cad-studio/tests/http_api.rs` |

## 3. Linear static CAE evidence (2026-11-03)

Implementation and full limits: [`docs/cae/CAE_CAPABILITIES.md`](../cae/CAE_CAPABILITIES.md).

### Command

```sh
cargo test --offline -p cad-cae -p cad-cli
```

### What is asserted

| Area | Assertion | Where |
|---|---|---|
| Element | 24 × 24 stiffness is symmetric; a rigid-body translation gives zero internal force; a prescribed uniform uniaxial strain recovers the analytic stress | `cad-cae` unit tests |
| Voxel classifier | an aligned cube fills its grid exactly; a ray ahead of the origin is counted, one behind is not; outside the triangle footprint is not; a triangle parallel to the ray is not; non-positive voxel sizes are refused | unit tests |
| Exact patch test | 40 × 10 × 10 mm bar under 1 000 N: von Mises 10 MPa and strain energy 1.0 N·mm both within 1e-8 relative; equilibrium residual < 1e-8; four elements | `cad-cae/tests/static_analysis.rs` |
| Bending convergence | 60 × 10 × 10 mm cantilever against `PL³/(3EI) + PL/((5/6)GA)`: refining 5 → 2.5 mm must reduce the error and the fine result must be within 10%; measured error −3.95% at 2.5 mm | same file |
| Refusals | open mesh, volume error over 25%, unknown material, empty plane selection, no supports, no loads, zero load, non-positive voxel, fully constrained body, non-converged solver | same file |
| End-to-end | `atlas cae static` on a real project evaluates the Truck mesh, writes the result JSON and records a `simulation` artifact at the project revision; editing the design makes the gate exit 3; review does **not** clear it (`accepted_after_review`, `releasable: false`, policy `recompute`); recomputing at the new revision clears it | `cad-cli/tests/cae_workflow.rs` |
| Study file contract | a versioned study round-trips; malformed, oversized, unknown-field and newer-schema files are `StudyFile` errors; a well-formed study with no supports is a validation refusal, not a broken file | `cad-cae` unit tests |
| Job reproducibility | `--study` and the equivalent flags produce identical mesh and result numbers; the written file equals the printed one; the result embeds the definition, `cad-cae` engine version and source document/revision; the atomic write leaves no temporary file | `cad-cli/tests/cae_workflow.rs` |
| Exit-code taxonomy | malformed study, newer schema and mixed file+flags exit **2**; an empty study and an unknown material exit **3**; a refused study publishes no result file | same file |
| Container job | the hardened container (no network, read-only root, capabilities dropped, 512 MB cap) runs a 384-element cantilever within 10 % of beam theory (3.95 %), keeps the read-only project byte-identical, exits **3** on an off-body load and **2** on a malformed study, runs as uid 10001, refuses a job without a result volume, and records a releasable `simulation` | `deploy/build-cae-image.sh --smoke` |

### Measured cantilever series

Release CLI on the recorded development host, 100 N, 60 × 10 × 10 mm, AISI 1018 from the library
(E = 205 GPa, ν = 0.29); reference 0.043052 mm:

| Voxel | Elements | DOF | Iterations | Displacement | Error |
|---:|---:|---:|---:|---:|---:|
| 5.0 mm | 48 | 351 | 51 | 0.037498 mm | −12.90% |
| 2.5 mm | 384 | 1 875 | 107 | 0.041352 mm | −3.95% |
| 1.25 mm | 3 072 | 11 907 | 214 | 0.042513 mm | −1.25% |

The peak von Mises stress in the same series (14.34, 24.34, 29.53 MPa) did **not** converge because
the idealised fixed end is singular. That is recorded as a limitation, not hidden.

Counts on this run: 10 `cad-cae` unit tests, 6 analytic integration tests, 4 CLI end-to-end tests,
4 release-policy/simulation unit tests in `cad-engineering-model`, and the full workspace gate at
**410 tests, 0 failures**, clippy `-D warnings` clean. The container smoke test ran separately and
asserted the deployment contract above.

Measured deployment envelope (release binary, recorded host): 64 000 elements in 7.8 s at 20 MB peak
RSS; 160 000 elements in 22.9 s at 43 MB; the same 160 000-element study in 24.1 s inside the
container under `--memory 512m`. Full table and method:
[`docs/operations/cae-deployment.md`](../operations/cae-deployment.md) §5.

## 4. What these tests do not prove

* They do not prove a modal, thermal, buckling, nonlinear, CAM, PCB or inspection result is correct:
  those producers do not exist.
* They do not prove a linear static result is accurate for a real part: the mesh is a stair-step
  voxel approximation, loads are lumped, and the peak stress at an idealised support is singular.
* They do not prove the built-in material values are right for a specific supplier or temperature.
* They do not prove a studio UI displays the stale state; the HTTP payload does. There is no study
  editor or result viewer.* They do not measure `assess` performance on a large registry.
* The cantilever reference is beam theory, not a published finite-element benchmark or a physical
test. A second independent reference is required before stronger accuracy claims.
* The Kubernetes Job in the deployment page was **not** run on a cluster; only the Docker shape was.
  The container smoke runs on one host with one Docker version, and does not prove an orchestrator's
  volume, deadline or retry behaviour.
* The container image is unsigned and unscanned, has no custom seccomp profile, and its parsers have
  not been fuzzed. Hardening flags and bounded inputs are not a security review.

## 5. Domain gates for future work

| Domain | Required before claiming implemented | Reference evidence |
|---|---|---|
| CAM | reference toolpaths, continuity, collision cases, axis-limit cases, generated G-code and postprocessor regression on fixtures | analytic path length, a known-good post output |
| CAE (modal, thermal, buckling) | analytic frequency/temperature/load-factor references and a mesh-convergence series for each; thermal energy balance; buckling interpretation limits | closed-form or published benchmark |
| CAE (nonlinear/dynamic) | an established external solver, a licence review, and published benchmark cases re-run | published benchmark output |
| ECAD | connectivity, ERC positive/negative fixtures, schematic↔PCB sync, SPICE reference circuits, Gerber/drill read-back | a reference netlist and a fab-readable output |
| Generative design | constraint satisfaction, reproducibility, objective comparison, manufacturing feasibility, independent re-analysis | each candidate re-evaluated, not trusted because it was produced |
| Assembly | joint solving, motion constraints, interference, component references, save/load integrity | analytic positions and interference volumes |
| Drawings | projection correctness, dimension references, scale, annotations, model-change updates, export fidelity | measured geometry on the sheet and a DXF read-back |

No domain may be promoted on a feature-list comparison. The acceptance criteria above are deliberately
stricter than "a user can click it".

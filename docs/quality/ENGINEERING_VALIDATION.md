# Engineering Validation

**As of:** 2026-11-02 · **Status:** Phase A has tests; later engineering domains have none because they
are not implemented.

This page records how an engineering capability is allowed to move from "code exists" to "trusted",
and the evidence for the Phase A unified engineering model. The general status ladder remains
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

## 2. Phase A evidence (2026-11-02)

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
| Fingerprint | same revision with different content becomes `requires_recompute`; missing fingerprints fall back to the revision number | unit test |
| Persistence | JSON round trip, unknown-field refusal, newer-schema refusal, 8 MiB limit, zero-revision refusal, atomic save with no temporary left behind | unit tests |
| Project-embedded registry | a recorded artifact survives save/reopen; a later design edit reports `requires_review`; review reports `accepted_after_review` with the original source revision; an artifact for another document is refused; a schema-8 project opens with no records | `cad-document` unit tests |
| CLI gate | create a project, record a drawing and a dependent toolpath, check passes; edit the model, check exits **3**; review one artifact, gate still fails; review both, gate passes; source revisions remain unchanged; the same workflow runs with the registry inside the project and no sidecar file | `cad-cli/tests/artifacts_workflow.rs` |
| CLI refusal | an unknown upstream is refused and no file is written; an unknown artifact kind is refused | same file |
| Studio integration | a generated drawing is `current`; a saved drawing is pinned; a model edit makes it `requires_review`; `/api/engineering/review` acknowledges it and the saved project file contains the record; `/api/engineering` reports the same state plus seven materials marked non-certified | `cad-studio/tests/http_api.rs` |

Counts on the implementation run: 15 model unit tests plus 1 doctest, 3 CLI end-to-end tests, and 2
HTTP tests (33 in the studio API suite), plus 3 `cad-document` persistence tests. The full workspace
gate passes 388 tests with 0 failures on the recorded host. The full workspace gate result is recorded in
`engineering/quality/tests.json` when it is run on this commit; until then, only the focused results
above are claimed.

## 3. What these tests do not prove

* They do not prove a simulation, toolpath, PCB layout or inspection result is correct: those
  producers do not exist. They prove the **provenance and staleness logic** is correct.
* They do not prove the built-in material values are right for a specific supplier or temperature.
  The library says so in every entry.
* They do not prove a studio UI displays the stale state; the HTTP payload does. No browser assertion
  was added for this field.
* They do not measure `assess` performance on a large registry.

## 4. Domain gates for future work

| Domain | Required before claiming implemented | Reference evidence |
|---|---|---|
| CAM | reference toolpaths, continuity, collision cases, axis-limit cases, generated G-code and postprocessor regression on fixtures | analytic path length, a known-good post output |
| CAE | analytical patch tests, cantilever/plate reference solutions, mesh convergence, solver convergence, reproducibility metadata | closed-form or published benchmark |
| ECAD | connectivity, ERC positive/negative fixtures, schematic↔PCB sync, SPICE reference circuits, Gerber/drill read-back | a reference netlist and a fab-readable output |
| Generative design | constraint satisfaction, reproducibility, objective comparison, manufacturing feasibility, independent re-analysis | each candidate re-evaluated, not trusted because it was produced |
| Assembly | joint solving, motion constraints, interference, component references, save/load integrity | analytic positions and interference volumes |
| Drawings | projection correctness, dimension references, scale, annotations, model-change updates, export fidelity | measured geometry on the sheet and a DXF read-back |

No domain may be promoted on a feature-list comparison. The acceptance criteria above are deliberately
stricter than "a user can click it".

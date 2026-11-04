# CAE Analysis Jobs

**As of:** 2026-11-03 · **Status:** one linear static study per container, built and smoke-tested end
to end on the development host. The job contract, the exit codes, the resource envelope and the
security posture are below. Orchestration beyond `docker run` is described but was **not executed**
on a cluster.

Implementation: [`crates/cad-cae`](../../crates/cad-cae/src/lib.rs), `atlas cae static`.
Image: `deploy/Dockerfile.cae`, `deploy/cae-entrypoint.sh`, `deploy/build-cae-image.sh`.
Analysis limits: [`docs/cae/CAE_CAPABILITIES.md`](../cae/CAE_CAPABILITIES.md) — read them first;
this page adds deployment, not accuracy.

## 1. What a job is

A CAE job is a **batch process**, not a service:

| Piece | Reality |
| --- | --- |
| Unit of work | one project file + one study file → one result file |
| Process | `atlas cae static`, one study, then exit |
| State | the project file and the result file; no database, no session |
| Network | none needed and none opened by the binary |
| Liveness | the exit code; there is no port and no healthcheck |
| Concurrency | one job per project file. Two jobs on one file race in the ordinary way |
| Timeout | owned by the supervisor. The solver has no internal wall-clock limit |
| Retry | none automatic. The exit code says whether a retry could help |

There is no queue, no HTTP submission endpoint, no job database and no cancellation API. A
supervisor (Docker, a Kubernetes Job, a CI runner, systemd) supplies those. That is the same choice
the studio deployment makes: the container is the contract.

### 1.1 What informed the shape

* **Verification is not validation.** ASME V&V 10 separates code verification (a patch test with a
  known solution), solution verification (a mesh-convergence series) and validation against
  experiment. This solver does the first two and claims no third; that is why the quality label is
  data and no deployment calls a result certified.
* **A batch job's failure signal is its exit code.** Kubernetes retries a failed Pod by default
  unless `backoffLimit` or `podFailurePolicy` says otherwise, so a deterministic refusal and a
  transient disk fault must not share a code. Hence 2 (fix the job), 3 (fix the study or design) and
  `backoffLimit: 0` in the example.
* **The inputs are the untrusted surface.** CIS Docker and OWASP container guidance — non-root,
  read-only root, dropped capabilities, `no-new-privileges`, explicit mounts — is the baseline here,
  and both input formats are bounded and fail closed. Neither is a substitute for a security review,
  which §7 states rather than assumes.

The batch shape is deliberate. The studio is a single-document server whose HTTP write timeout is
10 s; a 160 000-element solve takes about 23 s on the recorded host. Running the solve inside a
request would either time out or block every other request on the document. A separate job is the
honest unit.

## 2. The job contract

### 2.1 Inputs

| Input | Required | Limit |
| --- | --- | --- |
| `.zcad` project | yes | parsed by `cad-document`; a newer schema is refused, never downgraded |
| study file (JSON, schema 1) | yes | 64 KiB, unknown fields refused at every level |
| result path | yes | written atomically; the directory must exist and be writable |
| optional artifact id | no | records a `simulation` at the project revision; needs a writable project |

The study file is versioned because a deployment outlives a flag list:

```json
{
  "schema": 1,
  "study": {
    "name": "cantilever",
    "voxel_size_mm": 2.5,
    "material": "steel.1018",
    "supports": [
      {
        "face": { "axis": "x", "coordinate_mm": 0.0, "tolerance_mm": 0.001 },
        "fix": [true, true, true]
      }
    ],
    "loads": [
      {
        "face": { "axis": "x", "coordinate_mm": 60.0, "tolerance_mm": 0.001 },
        "total_force_n": [0.0, 0.0, -100.0]
      }
    ],
    "solver": { "relative_tolerance": 1e-9, "max_iterations": 40000 }
  }
}
```

Rules enforced before a solve (all refused with a reason, never clamped):

* `schema` must be 1; a larger number is refused as unsupported, not read as 1;
* `name` 1–120 characters; `material` must exist in the shared library and have a modulus and
  Poisson ratio;
* `voxel_size_mm` positive and finite;
* 1–64 supports and 1–64 loads; a support fixes at least one component; a load is finite and not
  zero; every plane has a finite coordinate and a positive finite tolerance;
* solver tolerance in `(0, 1e-2]`, iterations in `1..=1_000_000`;
* the resulting mesh at most 250 000 elements and 400 000 nodes, and within 25 % of the body volume.

The file and the flags are mutually exclusive: `--study` with `--voxel`, `--material`, `--fix`,
`--hold`, `--load`, `--force`, `--solver-tol`, `--max-iter` or `--name` is refused. A silent
override would make the embedded inputs disagree with the numbers.

### 2.2 Result

The result is printed to stdout and, with `--out`, published by write-to-temp + `fsync` + rename, so
a watcher never sees a partial file and a kill during a write leaves the previous file intact. It is
self-contained:

```text
{
  "study": "cantilever",
  "quality": "preliminary engineering study - linear elastic, not a certified analysis",
  "engine": "cad-cae 0.1.0",
  "definition": { "name": "cantilever", "voxel_size_mm": 2.5, "material": "steel.1018", "...": "..." },
  "source": { "document": 1, "revision": 6 },
  "material": { "id": "steel.1018", "youngs_modulus_mpa": 205000.0, "poisson_ratio": 0.29, "...": "..." },
  "mesh": { "elements": 384, "nodes": 1875, "degrees_of_freedom": 5625, "...": "..." },
  "solver": { "iterations": 111, "relative_residual": 3.1e-10, "converged": true, "...": "..." },
  "results": { "max_displacement_mm": 0.041352, "max_von_mises_mpa": 24.34, "...": "..." },
  "warnings": ["..."]
}
```

`definition` and `engine` are new in this cycle and are why a result file can be audited without a
sidecar: the study is embedded, and the engine version is recorded. `source` is the document and
revision the mesh was evaluated from; it is the same identity the artifact registry pins and the
release gate checks. No result is written when the study is refused.

### 2.3 Environment contract in the image

| Variable | Default | Meaning |
| --- | --- | --- |
| `ATLAS_PROJECT` | `/work/part.zcad` | project file |
| `ATLAS_STUDY` | `/work/study.json` | versioned study file |
| `ATLAS_RESULT` | `/results/result.json` | result path |
| `ATLAS_RECORD` | unset | artifact id to record at the project revision |
| `ATLAS_LABEL` | `linear static study` | label for that record |

The entrypoint checks that both inputs are readable and that the result directory exists and is
writable **before** starting the solver, and exits 2 if not. It then `exec`s `atlas`, so the
container's exit code is the solver's.

## 3. Exit codes and retry policy

| Code | Meaning | Retry unchanged? |
| --- | --- | --- |
| `0` | a result was produced and published | — |
| `1` | internal or I/O fault (unreadable kernel, full disk, failed rename) | **yes**, after checking the host |
| `2` | bad job input: malformed/oversized/newer study, missing or unreadable file, conflicting flags, unwritable result directory | no — fix the job |
| `3` | the input is well formed but the analysis refused it: open mesh, no node on a plane, unknown material, volume error, mesh too large, non-converged solve | no — fix the study or the design |

The distinction is the useful part. A supervisor that retries every non-zero exit turns a permanent
refusal into a retry storm; one that retries nothing hides a transient disk fault. `atlas` uses this
taxonomy for its other subcommands too (bad input is 2). `atlas artifacts check` also exits 3, for a
different reason: it is a release gate, not a solver.

Kubernetes retries failed Pods by default (`backoffLimit`), so a Job should set `backoffLimit: 0` and
let the exit code drive an explicit decision. A refusal is deterministic: the same study on the same
project produces the same refusal.

## 4. Run it

```sh
deploy/build-cae-image.sh            # release atlas + atlas-cae:<version>
deploy/build-cae-image.sh --smoke    # ... and run the contract end to end
```

The hardening flags below are the shape the smoke test exercises, not a suggestion:

```sh
docker run --rm \
  --network none \
  --read-only \
  --cap-drop ALL \
  --security-opt no-new-privileges \
  --memory 512m --pids-limit 256 \
  --tmpfs /tmp:rw,noexec,nosuid,size=16m \
  -v /srv/atlas/job:/work:ro,z \
  -v /srv/atlas/results:/results:z \
  -e ATLAS_PROJECT=/work/part.zcad \
  -e ATLAS_STUDY=/work/study.json \
  -e ATLAS_RESULT=/results/result.json \
  atlas-cae:0.1.0
```

`--network none` is not a restriction the solver works around: it makes no network call. `--read-only`
is why a result volume must be mounted; the entrypoint refuses with exit 2 rather than writing into
an anonymous volume nobody will find.

On RHEL, Fedora and other SELinux hosts the volumes need a relabel (`:z`, shared, or `:Z`, private)
or the container cannot write the results directory. The smoke test hit exactly this on the recorded
host; the permission failure was the entrypoint's `-w` check, which is the intended behaviour.

To record provenance in the same run, mount the project writable and set `ATLAS_RECORD`:

```sh
docker run ... -v /srv/atlas/job:/work:rw,z ... -e ATLAS_RECORD=1 \
  -e ATLAS_LABEL="bracket static" atlas-cae:0.1.0
atlas artifacts check --project /srv/atlas/job/part.zcad   # exit 3 when stale
```

Recording does not bump the design revision; it writes the artifact at the revision the mesh was
evaluated from. A later design edit makes the gate exit 3, and reviewing the artifact does **not**
clear it: a simulation is recompute-only.

## 5. Resource envelope (measured)

Release binary, 60 × 10 × 10 mm and 80 × 40 × 20 mm and 100 × 40 × 40 mm steel cantilevers,
100/1 000/1 500 N, on the recorded development host (Linux x86-64, Rust 1.95, 512 MB container cap):

| Elements | Nodes | DOF | PCG iterations | Wall clock | Peak RSS |
| ---: | ---: | ---: | ---: | ---: | ---: |
| 384 | 1 875 | 5 625 | 111 | 0.01 s host / 0.9–1.5 s container (warm/cold) | — |
| 64 000 | 69 741 | 209 223 | 512 | 7.8 s host | 20 MB |
| 160 000 | 169 781 | 509 343 | 603 | 22.9 s host / 24.1 s container | 43 MB |

The container figures include process startup and image entry. The 160 000-element case was run in
the hardened container under `--memory 512m` and converged. Memory grows with the degree-of-freedom
count and a handful of solver vectors; 512 MB is a floor with margin for the current 250 000-element
cap, not a proof for every geometry. Measure a representative part before setting a limit.

The solver has **no wall-clock timeout**. It bounds work by the iteration limit and the element/node
cap, but a large mesh can still take minutes. Set the supervisor's deadline
(`activeDeadlineSeconds`, `timeout`, `systemd-run --property=RuntimeMaxSec=`), and treat a kill as
"no result": the atomic publish means a killed job never leaves a partial one.

## 6. Provenance and the release gate

* The result embeds `definition`, `engine` and `source`. The engine version and the exact inputs are
  in the file, so a saved result can be reproduced without a sidecar.
* `--record ID` writes a `simulation` artifact into the project's embedded registry at the source
  revision, with the summary and warnings. `atlas artifacts check` exits 3 when the design has moved
  on, and a review cannot release a stale simulation — only a recompute can.
* The artifact stores the summary, not the result payload. Keep the result file: the registry says
  whether a result may be trusted, not what it was.
* **Single writer.** The project file is the state. A job that records while the studio has the
  document open races the same way two studios do; stop the studio or accept last-write-wins. The
  read-only result-only shape cannot corrupt the document because it never writes it.

## 7. Security posture

What the shape actually provides:

* **Bounded inputs.** The study file is limited to 64 KiB, versioned and deny-unknown-fields; the
  project is a versioned document parsed by `cad-document`; the mesher enforces element/node caps and
  a volume-error guard. Each refusal is a stated error, not a clamp.
* **Least privilege.** Non-root uid 10001, read-only root filesystem, all capabilities dropped,
  `no-new-privileges`, a PID cap, a memory cap, and no network. `tini` forwards `SIGTERM`; the solver
  has no handler and dies, and the atomic write means no partial result is published.
* **No secrets.** The image reads no token and needs none. There is no `ps`-visible secret because
  there is no secret.
* **No ambient authority.** The job writes only to the two paths the operator mounts.

What it does **not** provide:

* it is not a sandbox against a memory-safety bug in the Rust parser or the Truck kernel. A container
  is isolation from the host, not from an exploit that escapes it; keep the runtime patched;
* there is no custom seccomp profile beyond the runtime default, no image signing, no image scan, and
  no SBOM attached to this image (the workspace SBOM covers the dependency graph);
* the study/job parsers have not been fuzzed. They are bounded and fail closed, which is not the same
  as a security review;
* a hostile `.zcad` is still parsed in-process. The element/node caps bound the solver, not the
  document parser's memory; the document reader has its own schema and size limits.

## 8. Orchestration

A Kubernetes Job that matches the contract (this manifest was **not** executed on a cluster; the
Docker shape above was):

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: atlas-cae-bracket
spec:
  backoffLimit: 0
  activeDeadlineSeconds: 900
  template:
    spec:
      restartPolicy: Never
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        runAsGroup: 10001
        seccompProfile: { type: RuntimeDefault }
      containers:
        - name: study
          image: registry.example.com/atlas-cae:0.1.0
          env:
            - { name: ATLAS_PROJECT, value: /work/part.zcad }
            - { name: ATLAS_STUDY, value: /work/study.json }
            - { name: ATLAS_RESULT, value: /results/result.json }
          resources:
            requests: { cpu: "1", memory: 256Mi }
            limits: { memory: 512Mi }
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities: { drop: ["ALL"] }
          volumeMounts:
            - { name: inputs, mountPath: /work, readOnly: true }
            - { name: results, mountPath: /results }
            - { name: tmp, mountPath: /tmp }
      volumes:
        - name: inputs
          persistentVolumeClaim: { claimName: atlas-job-inputs }
        - name: results
          persistentVolumeClaim: { claimName: atlas-results }
        - name: tmp
          emptyDir: { sizeLimit: 16Mi }
```

A CI step has the same shape: build the image, run it with the same flags, then gate on the exit code
and on `atlas artifacts check` for the project it wrote. There is no server to deploy and no queue to
operate.

## 9. What this does not give you

* No job submission service, authentication, tenant isolation or audit log.
* No queue, scheduling, priority, cancellation endpoint or distributed/HPC solve. One study uses one
  process.
* No internal wall-clock timeout, no checkpoint/restart, no resume of an interrupted solve.
* No batch of studies in one invocation, no parametric sweep, no result database or field storage.
* No surface in the studio: a study is submitted and read as files, not clicked through.
* No modal, thermal, buckling, nonlinear or dynamic analysis — see the capability page for the
  solver itself.
* No certification. Every result carries
  `preliminary engineering study - linear elastic, not a certified analysis`, and that label is data.

## 10. Reproduce

```sh
cargo test --offline -p cad-cae -p cad-cli
deploy/build-cae-image.sh --smoke
```

The smoke test builds the image and asserts, in the hardened container: a 384-element cantilever is
within 10 % of the Timoshenko beam solution (measured 3.95 %); the result embeds its study, engine and
source revision; the read-only project is unchanged; an off-body load exits **3** and publishes
nothing; a malformed study exits **2**; the process is uid 10001; a job without a result volume exits
**2**; and recording a simulation inside the container leaves `atlas artifacts check` passing.

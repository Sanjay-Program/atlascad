# CAM Machining Jobs

**As of:** 2026-11-04 · **Status:** one CAM job per container, built and smoke-tested end to end on
the development host (image `atlas-cam:0.1.0`). The job contract, exit codes, resource envelope and
security posture are below; orchestration beyond `docker run` is described but was **not executed**
on a cluster.

Implementation: [`crates/cad-cam`](../../crates/cad-cam/src/lib.rs), `atlas cam job`.
Image: [`deploy/Dockerfile.cam`](../../deploy/Dockerfile.cam),
[`deploy/cam-entrypoint.sh`](../../deploy/cam-entrypoint.sh),
[`deploy/build-cam-image.sh`](../../deploy/build-cam-image.sh).
Machining limits: [`docs/cam/CAM_CAPABILITIES.md`](../cam/CAM_CAPABILITIES.md) — read them first; this
page adds deployment, not machining accuracy.

## 1. What a job is

A CAM job is a **batch process**, not a service:

| Piece | Reality |
| --- | --- |
| Unit of work | one versioned job file + one STL part → five output files |
| Process | `atlas cam job --job … --out-dir …`, one job, then exit |
| State | the job file, the part and the outputs; no database, no session |
| Network | none needed and none opened by the binary |
| Liveness | the exit code; there is no port and no healthcheck |
| Concurrency | one job per output directory. Two jobs on one directory race in the ordinary way |
| Timeout | owned by the supervisor. The planner has no internal wall-clock limit |
| Retry | none automatic. The exit code says whether a retry could help |

There is no queue, no HTTP submission endpoint, no job database and no cancellation API. A
supervisor (Docker, a Kubernetes Job, a CI runner, systemd) supplies those.

### 1.1 What informed the shape

* **A program is a file, not a response.** The job writes the same five artefacts an operator gets
  from the CLI: the program, a setup sheet as Markdown and PDF, a stock preview and a JSON summary.
  A container that answered over HTTP would still have to publish those files somewhere.
* **A batch job's failure signal is its exit code.** A deterministic refusal (holder collision, an
  impossible depth) and a transient disk fault must not share a code, so exit 3 and exit 1 stay
  distinct and a Job should run with `backoffLimit: 0`.
* **The inputs are the untrusted surface.** The job file is capped at 64 KiB, versioned and
  deny-unknown-fields; the part is parsed by `cad-io`'s binary STL reader. Non-root, read-only root,
  dropped capabilities, `no-new-privileges` and explicit mounts are the baseline. Neither is a
  substitute for a security review, which §7 states rather than assumes.
* **Nothing here has touched a machine.** The container posts a program; it does not verify it
  against a controller, a fixture or a real tool. That is a limit of the workspace, not of Docker.

## 2. The job contract

### 2.1 Inputs

| Input | Required | Limit |
| --- | --- | --- |
| job file (JSON, schema 1) | yes | 64 KiB, unknown fields refused at every level |
| STL part | when `job.part` is set | binary STL, read by `cad-io` |
| output directory | yes | must exist and be writable; five files are written atomically by rename |

The job file is versioned because a deployment outlives a flag list:

```json
{
  "schema": 1,
  "job": {
    "name": "deployment bracket",
    "stock_mm": [40, 30, 20],
    "part": "part.stl",
    "format": "grbl",
    "tool": {
      "diameter_mm": 6,
      "stepover_mm": 4,
      "stepdown_mm": 2,
      "flute_length_mm": 20,
      "stock_to_leave_mm": 0.5,
      "holder": { "diameter_mm": 20, "length_mm": 40 }
    },
    "finish": { "diameter_mm": 6 },
    "resolution_mm": 2
  }
}
```

`part` is resolved relative to the job file, so a mounted `/work` directory is self-contained. A
relative `part` that escapes the directory is not rewritten or sandboxed: the job reads the path it
names, and the mount is the boundary.

Defaults when a field is omitted: `format` = `grbl`, `resolution_mm` = 1,
`tool.stepover_mm` = 4, `tool.stepdown_mm` = 2, `tool.flute_length_mm` = 20,
`tool.stock_to_leave_mm` = 0, no holder, no finishing pass.

Rules enforced before planning (all refused with exit 2 and a reason, never clamped):

* `schema` must be 1; a larger number is refused as unsupported, not read as 1;
* `name` 1–120 characters; it becomes the program name in the header and the sheet;
* `stock_mm` is three positive finite numbers;
* `tool.diameter_mm` positive; `stepover_mm`, `stepdown_mm`, `resolution_mm` positive;
  `stock_to_leave_mm` finite and non-negative; `flute_length_mm` positive;
* a holder needs both `diameter_mm` and `length_mm` positive;
* `finish.diameter_mm` positive, and `job.finish` requires `job.part`;
* `format` is `grbl`, `linuxcnc` or `mach3`.

The job file and the input flags are mutually exclusive: `--job` with `--size`, `--part`, `--tool`,
`--stepover`, `--stepdown`, `--stock-to-leave`, `--resolution`, `--format`, `--flute`,
`--holder-diameter`, `--holder-length` or `--finish` is refused. A silent override would make the
embedded inputs disagree with the numbers. `--out-dir` is still required, because where a job writes
is an operator decision.

### 2.2 Outputs

Five files, each published by write-to-temp + rename, so a watcher never sees a partial one:

| File | Content |
| --- | --- |
| `program.<ext>` | one program for every operation, with a tool change (`M0` on Grbl, `T.. M6` on LinuxCNC/Mach3) |
| `setup.md` | machine, stock, WCS, clearance, tool table, per-operation moves/lengths/time |
| `setup.pdf` | the same sheet as a paginated A4 PDF with no timestamps (byte-identical for the same job) |
| `stock-preview.txt` | heightmap removal preview: removed volume, fraction, deepest cut |
| `summary.json` | printed to stdout too: engine version (`cad-cam 0.1.0`), operations, moves, lengths, times, removal, holder, job path, file list |

`summary.json` names the job file it ran (`job`), so a result can be traced to its inputs without a
sidecar. The summary records numbers, not the toolpaths; keep `program.*` as the artefact.

### 2.3 Environment contract in the image

| Variable | Default | Meaning |
| --- | --- | --- |
| `ATLAS_JOB` | `/work/job.json` | versioned job file |
| `ATLAS_OUT` | `/out` | output directory |

The entrypoint checks that the job is readable and the output directory exists and is writable
**before** planning, and exits 2 if not. It then `exec`s `atlas`, so the container's exit code is
the planner's. The part travels through the same `/work` mount as the job file.

## 3. Exit codes and retry policy

| Code | Meaning | Retry unchanged? |
| --- | --- | --- |
| `0` | a program and its four companion files were produced | — |
| `1` | internal or I/O fault (unreadable part after validation, full disk, failed rename) | **yes**, after checking the host |
| `2` | bad job input: unreadable/malformed/oversized/newer job, missing part, missing output directory, conflicting flags, a value out of range | no — fix the job |
| `3` | the job is well formed but the CAM checks refused it: holder collision, impossible depth, a part that fills the stock, no toolpath to make | no — fix the job or the design |

A refusal is deterministic: the same job on the same part produces the same refusal. Kubernetes
retries failed Pods by default, so a Job should set `backoffLimit: 0` and let the exit code drive an
explicit decision.

## 4. Run it

```sh
deploy/build-cam-image.sh            # release atlas + atlas-cam:<version>
deploy/build-cam-image.sh --smoke    # ... and run the contract end to end
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
  -v /srv/atlas/cam/job:/work:ro,z \
  -v /srv/atlas/cam/out:/out:z \
  -e ATLAS_JOB=/work/job.json \
  atlas-cam:0.1.0
```

`--network none` is not a restriction the planner works around: it makes no network call.
`--read-only` is why the output volume must be mounted; the entrypoint refuses with exit 2 rather
than writing into an anonymous volume nobody will find.

On RHEL, Fedora and other SELinux hosts the volumes need a relabel (`:z` shared, or `:Z` private) or
the container cannot write the output directory. The smoke test uses `:z` for exactly this reason.

A local run outside a container is the same command:

```sh
atlas cam job --job job.json --out-dir out
```

## 5. Resource envelope (measured)

Release binary, the 20 × 20 × 12 mm block in a 40 × 30 × 20 mm stock from §2, 6 mm flat end mill
plus 6 mm ball nose, 2 mm resolution, on the recorded development host (Linux x86-64, 512 MB
container cap):

| Quantity | Host | Container |
| --- | ---: | ---: |
| Wall clock | < 0.01 s | 0.55 s (process start + image entry included) |
| Peak RSS | 3.6 MB | — |
| Moves | 614 (360 roughing, 182 walls, 72 finishing) | same |
| Removed material | 18 176 mm³ of 24 000 mm³ (75.7 %) | same |
| Output | 28 KiB across five files | same |

The planner builds a height field over the stock footprint and a distance field at each wall level;
cost grows with the stock area divided by the resolution squared, not with the move count alone.
There is **no wall-clock timeout**: set the supervisor's deadline (`activeDeadlineSeconds`,
`timeout`, `systemd-run --property=RuntimeMaxSec=`) and treat a kill as "no result" — the atomic
publish means a killed job never leaves a partial program.

## 6. Provenance

* `summary.json` names the job file, the part, the format, the operations and the measured numbers.
* The setup sheet carries the same tool and operation table as the program.
* `atlas cam job --project PART.zcad --record ID [--label TEXT]` records (or replaces) a `toolpath`
  artifact in the project at its current revision and saves the project. `atlas artifacts check`
  then exits 3 once the design moves on, and a toolpath is **recompute-only**: reviewing it does not
  release it, so the answer is to rerun the job against the new revision. The artifact keeps the
  summary, not the program — keep `program.*` beside it.
* Recording needs a project, and a CAM job can start from a bare STL. Use a project when the part is
  a designed solid; use the STL-only job when it is not, and the summary plus program are the
  provenance.
* **Single writer.** The job only reads its inputs. Writing results into the same directory as the
  inputs is allowed but pointless; mount them separately.

## 7. Security posture

What the shape actually provides:

* **Bounded job input.** 64 KiB, schema 1, deny-unknown-fields, every number checked for finiteness
  and range. A refusal is a stated error, not a clamp.
* **Least privilege.** Non-root uid 10001, read-only root filesystem, all capabilities dropped,
  `no-new-privileges`, a PID cap, a memory cap and no network. `tini` forwards `SIGTERM`; the planner
  installs no handler and dies, and atomic writes mean no partial program is published.
* **No secrets.** The image reads no token and needs none.
* **No ambient authority.** The job writes only to the directory the operator mounts.

What it does **not** provide:

* it is not a sandbox against a memory-safety bug in the STL reader or the geometry adapter. A
  container isolates from the host, not from an exploit that escapes it; keep the runtime patched;
* there is no custom seccomp profile, no image signing, no image scan and no SBOM attached to this
  image;
* the job parser and the STL reader have not been fuzzed. They are bounded and fail closed, which is
  not a security review;
* a hostile STL is still parsed in-process; `cad-io`'s reader validates indices and finiteness, and
  the planner's grid is bounded by the stock, but the parser's memory is not capped by the job file.

## 8. Orchestration

A Kubernetes Job that matches the contract (this manifest was **not** executed on a cluster; the
Docker shape above was):

```yaml
apiVersion: batch/v1
kind: Job
metadata:
  name: atlas-cam-bracket
spec:
  backoffLimit: 0
  activeDeadlineSeconds: 300
  template:
    spec:
      restartPolicy: Never
      securityContext:
        runAsNonRoot: true
        runAsUser: 10001
        runAsGroup: 10001
        seccompProfile: { type: RuntimeDefault }
      containers:
        - name: cam
          image: registry.example.com/atlas-cam:0.1.0
          env:
            - { name: ATLAS_JOB, value: /work/job.json }
            - { name: ATLAS_OUT, value: /out }
          resources:
            requests: { cpu: "1", memory: 128Mi }
            limits: { memory: 512Mi }
          securityContext:
            allowPrivilegeEscalation: false
            readOnlyRootFilesystem: true
            capabilities: { drop: ["ALL"] }
          volumeMounts:
            - { name: inputs, mountPath: /work, readOnly: true }
            - { name: output, mountPath: /out }
            - { name: tmp, mountPath: /tmp }
      volumes:
        - name: inputs
          persistentVolumeClaim: { claimName: atlas-cam-inputs }
        - name: output
          persistentVolumeClaim: { claimName: atlas-cam-output }
        - name: tmp
          emptyDir: { sizeLimit: 16Mi }
```

A CI step has the same shape: build the image, run it with the same flags, gate on the exit code,
then archive `program.*`, `setup.md`, `setup.pdf` and `summary.json` as build artefacts.

## 9. What this does not give you

* No job submission service, authentication, tenant isolation or audit log.
* No queue, scheduling, priority, cancellation endpoint or distributed computation. One job is one
  process.
* No internal wall-clock timeout, checkpoint or resume.
* No batch of jobs in one invocation, no parametric sweep, no result database.
* No surface in the studio: a job is submitted and read as files, not clicked through.
* No verification against a real controller, no back plot on a machine model, no tool-length probing,
  no fixture check, and no feeds-and-speeds calculation. See the capability page for the machining
  limits; this page adds deployment, not accuracy.
* No certification and no safety sign-off. **No program produced here has been run on a physical
  machine.**

## 10. Reproduce

```sh
cargo test --offline -p cad-cam -p cad-cli
deploy/build-cam-image.sh --smoke

# Record a posted program against a design and gate a release on it:
atlas cam job --job job.json --out-dir out \
  --project part.zcad --record 7 --label "roughing and finishing"
atlas artifacts check --project part.zcad            # 0: the program is current
atlas project set --file part.zcad --param width --value 41
atlas artifacts check --project part.zcad            # 3: the design moved on
atlas cam job --job job.json --out-dir out2 \
  --project part.zcad --record 7                     # a recompute is the only release
atlas artifacts check --project part.zcad            # 0 again
```

The smoke test builds the image and asserts, in the hardened container: a 20 × 20 × 12 mm block in a
40 × 30 × 20 mm stock posts 614 moves as one program with a Grbl tool-change pause, three operations
(3D roughing, wall finishing, 3D finishing), 18 176 mm³ removed, five output files, a valid PDF; the
read-only inputs are unchanged; a holder collision exits **3** and publishes no program; a malformed
job exits **2**; the process is uid 10001; and a job without an output volume exits **2**.

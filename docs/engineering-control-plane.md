# Engineering Control Plane

**Status:** implemented, tested and running on this machine (2026-10-22).
**Location:** `crates/cad-engineering` (library + `atlas-eng` binary), state under `engineering/`.

---

## 1. Why it exists

Before this cycle, project state lived in prose documents and an agent transcript. Nothing could
answer, from the repository itself:

* which checkouts exist, at which commit, under which licence;
* which of them changed since the last review;
* what is *integrated* versus merely *present*;
* what work is blocked, and on what;
* whether a task had already been attempted.

That is a maintenance problem, not a documentation problem: a new repository, a moved HEAD or a
hand-edited file could arrive without any record. The control plane makes those facts machine
readable, validates them, and refuses the claims that cannot be supported.

## 2. Layout

```text
engineering/
├── manifest/            curated, human-reviewed facts (validated on every run)
│   ├── workspace.json   scan roots, ignore policy, licence allow/deny policy
│   ├── repositories.json
│   ├── modules.json
│   ├── agents.json
│   ├── interfaces.json
│   ├── capabilities.json
│   └── dependencies.json
├── tasks/               one task lives in exactly one file
│   ├── backlog.json     proposed / ready / blocked / failed / needs_review
│   ├── active.json      in_progress
│   └── completed.json   verified / cancelled
├── quality/             evidence, one record per command that actually ran
│   ├── builds.json  tests.json  benchmarks.json  security.json
├── state/               written by the scanner, never hand-edited
│   ├── scan.json        full observation, including per-file digests
│   ├── last-changes.json
│   └── scan-history.jsonl
├── migrations/          empty registry, reserved for document/data migrations
└── reports/latest-integration-report.md
```

`engineering/manifest/` is the curated record. `engineering/state/` is the observation. The scanner
**never** edits a curated manifest, so a source change cannot silently overwrite a known-good
integration record; it produces evidence and tasks instead.

## 3. Schema rules

* Every manifest is JSON with `schema_version`. Any other version is a load error — a newer schema is
  never interpreted silently.
* Every status is an enum in Rust (`snake_case` on disk). Unknown values and unknown fields
  (`serde(deny_unknown_fields)`) are rejected, so a typo cannot become a fact.
* A capability is `verified` only with an evidence reference. An interface cannot claim `integrated`
  without a contract test. A task cannot be `verified` without evidence, nor `failed` without an
  error. An agent cannot claim `registered` or later unless it is implemented and its tests pass.
* A licence on the deny list (GPL/SSPL/CDDL, which includes LGPL by substring) cannot be `approved`,
  and an `approved` licence must appear on the allow list.

## 4. Validation rules (implemented, not aspirational)

Run `atlas-eng validate`:

* Current snapshot (2026-10-23): 22 repositories (21 local paths and 1 external dependency), 32
  modules, 4 agents, 10 interfaces, 29 capabilities, 29 dependency edges, 11 build records, 13 test
  records, 9 benchmark records and 8 open security findings. Task counts are generated in the live
  report because scan discoveries add review items over time.
* Cross-references checked: repository → capability/interface/dependency, module → repository/
  capability/consumer, interface → provider module/consumers, capability → providers, task →
  dependencies and blocks, dependency → endpoints.
* Result on this workspace (2026-10-23): **0 errors, 0 warnings**. The `spade` and `zip` SPDX values
  were recorded from the locally resolved Cargo metadata. Their actual import-size limits and
  dependency advisories remain separate security concerns.

## 5. Fingerprints

Policy lives in `workspace.json`; the algorithm is documented in
`crates/cad-engineering/src/fingerprint.rs`.

* Depth-first walk, symlinks never followed, ignored directories (`.git`, `target`, `node_modules`,
  caches) and suffixes (`.zip`) skipped.
* Each file contributes `"<relative path>\0<digest>\n"` to an FNV-1a 64-bit stream, in sorted path
  order.
* A file's digest is FNV-1a over its bytes when ≤ `max_hash_bytes` (8 MiB); larger files contribute
  `meta:<len>:<mtime>` and are counted in `metadata_only`.
* FNV-1a is chosen for reproducibility across toolchains (unlike `DefaultHasher`). It is **not**
  cryptographic and is not used for integrity or security decisions.

Historical measured cost: 15 repositories, ~8,400 files, 23 MB peak RSS, **1.45 s** warm / 5.24 s cold
(`engineering/quality/benchmarks.json#bm-scan`). The current scan covers 22 repositories; its
current timing has not been benchmarked.

## 6. Scanning

`atlas-eng scan` is read-only. It:

1. discovers checkouts under `apps/`, `kernels/`, `references/`, `crates/` (a `.git`, `Cargo.toml` or
   `package.json` marks a candidate);
2. records Git HEAD, branch, origin and `git status --porcelain` count through the `git` binary (no
   libgit2 dependency);
3. fingerprints every declared path;
4. asks Cargo for the workspace package list with `cargo metadata --no-deps --offline`;
5. compares the observation with the previous one and with the curated declarations.

Every external command has a 20 s bound and its pipes are drained concurrently, so a large payload
cannot deadlock and a hung tool cannot stall a cycle. Timeouts are recorded in the observation notes
rather than producing an empty result silently.

**Cargo is invoked from the workspace root with `--manifest-path`.** This is not a style choice:
`kernels/hagane`, `kernels/monstertruck` and `references/cadrs` pin Rust 1.99.0 with
`rust-toolchain.toml`, so running cargo with the repository as the working directory made rustup try
to download a toolchain and time out (measured: 25 s per repository, package lists silently empty).
The regression test `a_pinned_toolchain_does_not_break_package_discovery` covers it.

## 7. Change detection

| Difference | Reported as | Task priority |
|---|---|---|
| checkout appears | `repository_discovered` | integration blocker |
| declared path or observed repo disappears | `repository_missing` | broken build |
| HEAD differs from the previous observation, or from `last_verified_commit` | `commit_changed` | integration blocker |
| a clean tree becomes dirty | `working_tree_dirty` | integration blocker |
| file digest map changes | `files_modified` (with up to 40 `+ / - / ~` paths) | interface compatibility |
| Cargo package list changes | `package_added` / `package_removed` | interface compatibility |

The first scan is a **baseline** and reports no changes: "everything changed" is not information.
Uncommitted work is never reset, cleaned or stashed.

**Known limitation:** `cargo metadata --no-deps` is a *workspace-wide* view, so when the platform
workspace gains a crate, each declared member reports the same `package_added` change with the same
digest. That is visible rather than hidden — identical digests point at one fact — but it produces one
task per member. Fixing it properly means attaching package inventory to a single workspace-root
observation, which this workspace has no scanned path for yet.

## 8. Tasks

* A generated task id is derived from the observed value:
  `task.scan.<kind>.<repo>.<first 8 hex of the value>`.
* `merge` inserts only ids it has not seen. An existing id is left exactly as it is, so a verified or
  cancelled task is never reopened, and re-running an unchanged scan creates nothing. Verified on the
  real workspace: two consecutive scans left `engineering/tasks/backlog.json` **byte-identical**
  (same SHA-256) with `changes: 0, tasks: 0 inserted`.
* `find_cycle` uses Kahn's algorithm to find the unorderable remainder, then walks `depends_on`
  inside it to report the actual cycle. A task that merely depends on a cycle is not blamed.

## 9. Commands

```sh
cargo run -p cad-engineering --bin atlas-eng -- validate          # 0 errors, 0 warnings (2026-10-23)
cargo run -p cad-engineering --bin atlas-eng -- scan              # dry run, writes nothing
cargo run -p cad-engineering --bin atlas-eng -- scan --write      # state + changes + tasks + report
cargo run -p cad-engineering --bin atlas-eng -- report            # render from recorded state
cargo run -p cad-engineering --bin atlas-eng -- tasks             # next unblocked tasks
```

`scan --write` writes only `engineering/state/`, `engineering/reports/`, and
`engineering/tasks/backlog.json` when a genuinely new task exists.

## 10. What it does not do (and will not pretend to)

* It does not build or test anything. Evidence in `quality/` is written by a person, a session or CI;
  the scanner never invents a result.
* It does not modify repositories, and there is no auto-fix mode.
* It does not audit licences, advisories or interfaces; it records classifications and refuses
  impossible combinations. `cargo-audit`/`cargo-deny` are still not installed.
* It does not resolve dependencies; the dependency manifest is curated and only cross-checked.
* It has no database. JSON is sufficient for one workspace and one maintainer; a database becomes
  justified when concurrent multi-user service operation exists, which it does not.

## 11. Adding a repository (the repeatable workflow)

1. `atlas-eng scan` — the checkout appears as `repo.unregistered.<path>` with a fingerprint.
2. Read its licence and record it (never assume the manifest's claim).
3. Add a `repositories.json` entry with a stable id, kind, licence status, role, evidence.
4. Add module/interface/capability entries only for what actually exists.
5. Run `validate`; fix cross-references.
6. Add the build/test commands, run them if the machine allows, record the result in `quality/`.
7. `scan --write` twice: the second run must report no changes.

**Exercised for real in the 2026-10-22 cycle.** The three agent crates were onboarded through
exactly this workflow: the scanner discovered each as `repo.unregistered.crates-cad-agent-*`, each was
given a stable id, licence, module, capability, interface and dependency entries, and the second
`scan --write` reported `changes: 0` and inserted no task. The scanner-generated discovery tasks for
the two earlier crates were closed as `verified`, and the placeholder `repo.unregistered.*` tasks
were closed as `cancelled` — superseded by the registration, which is a different event from the
repository having gone away.

One wrinkle worth recording: `cargo metadata` is evaluated **per workspace member**, so a single new
package appears once for every member with an identical digest. The scanner therefore generates one
"package added" task per member for one fact. They are closed together with the same evidence, and
the duplication is in the scanner's output rather than in the curated manifest.

## 12. Where the brief's document names live

The repository already had a documentation layout, so the required content is mapped rather than
duplicated (a duplicate status page is a page that drifts):

| Brief path | Actual location |
|---|---|
| `docs/architecture/current-state.md` | [current-state.md](architecture/current-state.md) (thin, links to the deep documents) |
| `docs/architecture/target-architecture.md` | [target-architecture.md](architecture/target-architecture.md) |
| `docs/integration/repository-catalog.md` | [repository-catalog.md](integration/repository-catalog.md) |
| `docs/integration/license-audit.md` | [license-audit.md](integration/license-audit.md) |
| `docs/quality/baseline-builds.md` | [baseline-builds.md](quality/baseline-builds.md) |
| `docs/quality/baseline-tests.md` | [baseline-tests.md](quality/baseline-tests.md) |
| `docs/decisions/` (ADRs) | [`docs/architecture-decisions/`](architecture-decisions/) (existing convention) |
| `engineering/reports/latest-integration-report.md` | generated |

## 13. Evidence for this document

| Claim | Evidence |
|---|---|
| 30 tests pass for the control plane | `cargo test -p cad-engineering` (4 unit + 26 integration) |
| Changed files are detected by path | `a_modified_file_is_reported_with_its_path` |
| A missing declared checkout is detected | `a_declared_repository_that_is_missing_is_reported` |
| A HEAD past the verified commit is detected | `a_head_past_the_verified_commit_is_reported` |
| Scans and task generation are idempotent | `a_scan_that_finds_nothing_new_is_idempotent`, `merging_the_same_tasks_twice_inserts_them_once`, plus the two byte-identical real scans |
| Cycles name only cycle members | `a_dependency_cycle_is_detected` |
| Schema drift is refused | `schema_drift_is_a_load_error` |
| Copyleft cannot be marked approved | `copyleft_cannot_be_marked_approved` |
| Unimplemented agents cannot claim registration | `an_agent_cannot_claim_registration_without_passing_tests` |
| The onboarding workflow works on a real repository | the three agent crates, onboarded 2026-10-22; a second `scan --write` reported `changes: 0` |
| The manifest describes the code that exists | `engineering/manifest/agents.json` lists one agent with `implemented: true` and three with `implemented: false`; the validator refuses an agent that claims `ready` without passing tests |

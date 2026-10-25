# ADR-0004: A versioned manifest control plane, scanned by Rust, not a database

- **Status:** Accepted
- **Date:** 2026-10-22
- **Deciders:** engineering (this cycle)
- **Evidence:** `docs/engineering-control-plane.md`, `crates/cad-engineering/tests/pipeline.rs`,
  `engineering/reports/latest-integration-report.md`

## Context

The workspace holds eleven independent Git checkouts plus platform crates, three of them
(`hagane`, `monstertruck`, `cadrs`) pinning a toolchain that is not installed. Project state was
recorded in prose and in an agent's transcript. Three concrete failures motivated this work:

1. A new checkout could be dropped into `apps/` or `kernels/` with nothing noticing.
2. A curated claim ("module X provides capability Y") could not be checked against anything, so a
   stale claim would survive indefinitely.
3. A prior session's evidence (builds, tests) lived in a document with no link to the commit it
   described.

The brief asks for persistent manifests, a Rust scanner/validator, stable ids, idempotent change
tracking and an auditable report. It also asks for a database only when multi-user operation
justifies one — which does not exist here.

## Decision

1. **JSON manifests, one file per domain, with `schema_version`.** JSON because the workspace already
   uses `serde_json` and `cargo metadata`; no new format dependency. A different version is a load
   error rather than a best-effort interpretation.
2. **Curated manifests are separate from observed state.** `engineering/manifest/**` is reviewed and
   may only be edited deliberately; `engineering/state/**` is written by the scanner. A scan cannot
   rewrite a claim, and an observation cannot be mistaken for a decision.
3. **Statuses are enums with validation rules that reject unsupportable claims** — a capability cannot
   be `verified` without evidence, an agent cannot claim `registered` while unimplemented, a copyleft
   licence cannot be `approved`.
4. **Generated task ids derive from the observed value**, not from time or a counter, so an unchanged
   scan reproduces the same ids and `merge` is a no-op.
5. **The scanner is read-only towards repositories** and bounded in time: 20 s per external command,
   symlinks never followed, ignore policy documented.
6. **No database, no daemon, no service.** A CLI that runs in ~1.5 s on the development machine is
   enough for one maintainer and is usable in CI unchanged.

## Alternatives considered

| Alternative | Why not now |
|---|---|
| SQLite state | Adds a binary artifact that must be migrated and inspected; the JSON state is 500 KB and readable. Revisit for multi-user/concurrent operation, as the brief requires. |
| `git2` for metadata | A large native dependency for four `git` invocations per repository; the `git` binary is already required to have these checkouts. |
| Deriving ids from directory names | Renaming a directory would create a new identity and orphan its history. Ids are explicit and stable; unregistered checkouts get a `repo.unregistered.<path>` id that is at least deterministic. |
| Reporting "everything changed" on the first scan | Produces one task per repository with no information. The first scan is a baseline. |
| Auto-updating `last_verified_commit` after a scan | That would convert "observed" into "verified" without running the tests the evidence depends on. The scanner reports drift; a session or CI records verification. |

## Consequences

* Two commands must be kept honest: `validate` (cross-references) and `scan --write` (observation).
* The manifests are a second place where facts live; they can go stale. Mitigation: drift detection on
  HEAD, fingerprint, packages, and validation rules that force evidence for strong claims.
* FNV-1a is not cryptographic. Fingerprints detect change; they do not prove integrity. Documented in
  the module and in the policy file.
* Files above 8 MiB are tracked by size and mtime. A same-size, same-mtime edit to such a file is not
  detected — an honest limitation rather than a silent one.
* Task generation is intentionally conservative: it creates review tasks, never code changes.

## Verification

`cargo test -p cad-engineering` — 2 unit + 26 integration tests, including a real Git repository
fixture and a regression for the toolchain-download defect. Real-workspace idempotency was checked by
running `scan --write` twice and comparing SHA-256 of `engineering/tasks/backlog.json`: identical,
with `changes: 0`.

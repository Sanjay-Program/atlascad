# engineering/

The machine-readable state of this project. Read `docs/engineering-control-plane.md` first.

```text
manifest/    curated, reviewed facts; validated on every run, never written by the scanner
tasks/       one task per id, in exactly one of backlog / active / completed
quality/     evidence: builds, tests, benchmarks, security — one record per command that ran
state/       observations written by `atlas-eng scan --write`; safe to delete
reports/     generated integration report
migrations/  registry for future document/data migrations (empty today)
```

Commands:

```sh
cargo run -p cad-engineering --bin atlas-eng -- validate
cargo run -p cad-engineering --bin atlas-eng -- scan --write
cargo run -p cad-engineering --bin atlas-eng -- report
cargo run -p cad-engineering --bin atlas-eng -- tasks
```

Rules:

1. Do not hand-edit `state/`. Delete it and rescan instead; the next scan becomes the baseline.
2. Do not mark anything `verified` without adding evidence in `quality/` or a test name.
3. Do not let the scanner write into `manifest/`; it deliberately cannot.
4. A scanner task is closed by a human or a session with a reason — never silently deleted.

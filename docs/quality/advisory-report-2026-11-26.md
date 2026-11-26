# Advisory and dependency-source report

**Date:** 2026-11-26 · **Scope:** the full workspace dependency graph (`Cargo.lock`) · **Policy:**
[`deny.toml`](../../deny.toml).

This is the first advisory scan actually executed for this tree. It was previously reported as
“blocked (tooling)”: `cargo-deny` was absent, and the CI job that runs it had never been observed.

## What was run

| Item | Value |
| --- | --- |
| Local command | `cargo deny check advisories bans sources` |
| Tool | `cargo-deny 0.22.1` (built from crates.io with `--locked`) |
| Repeatable script | [`scripts/advisories.sh`](../../scripts/advisories.sh) |
| Result | **advisories ok, bans ok, sources ok** |
| CI job | `advisories and dependency sources` — **success** on `25a5174` ([run](https://github.com/Sanjay-Program/atlascad/actions/runs/45861856673)) |

The CI job uses `EmbarkStudios/cargo-deny-action@v2` with `check advisories bans sources`, so the
local and CI checks share the same policy file.

## What it covers

- RustSec advisories, including yanked crates (`[advisories] yanked = "deny"`).
- Banned crates listed in `deny.toml` (OpenCASCADE bindings and the vendored `cadrum`).
- Wildcard dependencies (path-only workspace siblings exempted, external wildcards denied).
- Dependency provenance: registry vs git vs path sources, with unknown sources denied.

## What it does not cover

- No SBOM vulnerability enrichment, no container-image scanning, no licence text analysis beyond
  `atlas-eng licences` in the gate.
- A green scan means no *known* advisory matches this lockfile on that date. It is not a proof of
  absence, and the CI job needs the network, so it can fail for reasons unrelated to the code.
- The gate (`gate.sh`) stays offline by design; run `scripts/advisories.sh` separately or rely on CI.

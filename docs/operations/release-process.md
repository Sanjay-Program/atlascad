# Release process

**Status:** the local gate, the licence/SBOM check, the deployable bundle and the live simulation are
implemented and were executed on the recorded host on 2026-10-29. Code signing, notarisation and a
published update channel are **not** implemented; they need keys and infrastructure this repository
does not hold, and they are called out as outstanding in §6.

The rule this document exists for: **nothing ships that has not passed `scripts/gate.sh` on the same
commit.**

---

## 1. The gate

```sh
./scripts/gate.sh          # everything
./scripts/gate.sh --quick  # skip the live simulation while editing
JOBS=4 ./scripts/gate.sh   # on a host with more memory
```

The gate runs, in order and offline:

1. `cargo fmt --all --check`.
2. `cargo clippy --offline --workspace --all-targets -j 2 -- -D warnings`.
3. `cargo test --offline --workspace -j 2`.
4. `cargo run -p cad-kernel-truck --example corpus` — the analytic boolean corpus, which fails on a
   regression or an unexpected pass.
5. `atlas-eng validate` — every manifest cross-validated.
6. `atlas-eng licences` — every dependency's licence against
   [`engineering/policy/licences.json`](../../engineering/policy/licences.json). A denied **or
   unrecognised** licence fails: "not looked at" is how copyleft reaches a proprietary binary.
7. `atlas-eng sbom` (dry run) — the SBOM can be generated from the resolved graph.
8. `scripts/simulate.sh` — the real release server driven over HTTP through cut, join, undo/redo,
   parameter rebuild, save, restart, STL export and a `kill -9` recovery, then the live page loaded in
   headless Chrome (skipped with a message when no browser exists).

CI (`.github/workflows/ci.yml`) runs the same gate, then packages and uploads the bundle and SBOM, and
runs `cargo-deny` for advisories, banned crates and dependency sources. **The workflow has not been
executed on a runner yet** — it was added in this cycle on a host that cannot run GitHub Actions; the
gate it calls is the one that was executed here.

---

## 2. Packaging

```sh
./scripts/package.sh
```

Produces `dist/atlas-cad-<version>-<target>.tar.gz` containing:

| Path | What |
|---|---|
| `bin/cad-studio` | the local application |
| `bin/atlas` | the headless contract consumer |
| `bin/atlas-eng` | the control plane |
| `LICENSE`, `COMMERCIAL-LICENSE.md`, `README.md` | terms and scope |
| `docs/feature-matrix.md`, `docs/production-readiness.md` | what is verified and what is not |
| `atlascad.cdx.json` | CycloneDX 1.5 SBOM of the shipped graph |
| `SHA256SUMS` | checksums for every file above |
| `RUN.txt` | how to run and how to verify |

The script extracts the archive into a temporary directory, verifies the checksums, runs `--help` on
all three binaries and builds a box part with the packaged CLI. It refuses to declare success if any
of that fails.

**The bundle is a tarball, not an installer.** There is no `.deb`, `.rpm`, `.dmg`, `.msi`, desktop
entry, or uninstaller yet, and no update channel.

---

## 3. Versions

* The workspace version lives in `Cargo.toml` under `[workspace.package]`; the bundle name, the SBOM
  component and the binaries all take it from there.
* Payload schemas are versioned independently (`PROJECT_SCHEMA`, `RECOVERY_SCHEMA`,
  `DocumentHeader::schema`) and are **never** bumped without a migration and a test for it.
* The contract version is `cad_contract::CONTRACT_VERSION`; a breaking change to it requires a note
  in `docs/architecture.md` and a migration plan for consumers.

---

## 4. Reproducing a release record

Every claim in `docs/quality/` points at a record in `engineering/quality/`. After a release run:

```sh
cargo run -q -p cad-engineering --bin atlas-eng -- scan --write
cargo run -q -p cad-engineering --bin atlas-eng -- report --write
```

`scan --write` is idempotent: re-running it on an unchanged tree writes no new task. Then record the
test counts, the bundle size and the simulation result in `engineering/quality/tests.json` and
`engineering/quality/builds.json`, superseding the previous records rather than editing them.

---

## 5. Rollback

* A released bundle is immutable: revert by publishing the previous version, never by editing an
  archive.
* A `.zcad` schema change is the only migration that can strand a user. Every migration has a test
  (`cad-document` tests `schema_one_…`, `schema_two_…`, `schema_three_…`, `schema_four_…`); a schema
  bump without one is rejected in review.
* The recovery snapshot (`<project>.recovery`) is written atomically and never replaces the project,
  so rolling the application back cannot corrupt a document. A newer snapshot is ignored when its
  `RECOVERY_SCHEMA` is unknown.

---

## 6. Outstanding before a commercial release

These are blockers a paying customer or a procurement team will ask about, and they are **not** done:

1. **Code signing and notarisation** for Windows and macOS, and a signed `.deb`/`.rpm` for Linux.
2. **An update channel** with a signed manifest; today the user replaces the tarball by hand.
3. **Advisory monitoring actually running.** The CI job exists; no run is recorded, and
   `cargo-audit` is not installed on the development host.
4. **SLSA/provenance attestation** or an equivalent build-provenance statement.
5. **A support process** with an owner, response targets and a security contact.
6. **The commercial agreement itself**, signed; `COMMERCIAL-LICENSE.md` is a model, not a grant.
7. **Named revisions** (the remaining half of M11f), which a professional workflow expects from a
   versioned document.

Until 1–6 exist, the honest description of a bundle is "a verified development build", not "a
release".

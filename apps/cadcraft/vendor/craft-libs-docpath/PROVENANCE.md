# Vendoring provenance — craft-libs / craft-docpath

Vendored: 2026-10-22

Source repository: https://github.com/storytold/craft-libs
Source revision: 82a24b006eefa036b43c09a67fb9a3fbd17ffe63
Path within source: crates/docpath/
Licence: MIT OR Apache-2.0 (LICENSE-MIT, LICENSE-APACHE copied alongside)

Why vendored: CADCraft depends on this crate via a pinned git revision. The revision is not covered
by registry advisory feeds and its licence was never read. Vendoring pins the exact source into the
repository so a release build does not need GitHub, and so the licence can be audited here.

Changes versus upstream:
- Source files: none. Every source file below is byte-identical to the revision above
  (see the sha256 manifest).
- Cargo.toml: workspace fields inlined (`version = "0.1.0"`, `edition = "2024"`,
  `license = "MIT OR Apache-2.0"`, `rust-version = "1.95"`,
  `repository = "https://github.com/storytold/craft-libs"`); crate-level `[lints]` added from the
  upstream `[workspace.lints]` so behaviour is unchanged.
- No new dependencies; craft-docpath has none.

Verification:
- `cargo metadata --no-deps` resolves `craft-docpath 0.1.0` at this path with `source: None`.
- `cargo test --ignore-rust-version` passes: 12 unit tests + 1 doc test. (`--ignore-rust-version`
  is only used because this host has rustc 1.94.0 while the crate declares rust-version 1.95; the
  declaration is preserved unchanged.)
- Full CADCraft build (`cargo build -p cadcraft-engine`) could not be run here: it is blocked by a
  pre-existing, unrelated incompatibility — `egui 0.36` (a sibling dependency of the same app)
  uses the unstable library feature `push_mut` (rust-lang/rust#135974), which this host's stable
  rustc 1.94.0 does not support. That limitation exists identically with the git dependency and
  is not introduced by vendoring.

Integrity (sha256):
Cargo.toml fb9d5007b524b99830804c63948bd8208236ab216022bd69b61bbbef6ea6bc8c
LICENSE-APACHE 0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594
LICENSE-MIT 2f42c0cafc8ad394d91373fe1f342aee5453850b151a51405c68cae9ada86d26
NOTICE d8ba5d394b7cdd6ef5e6f02ae50d14ec061788d69f48d042e74b0d96ba515360
src/lib.rs 88c58c4e1e8447805913d190315cb152c6c7853ca64e3b7c573df157c7a773a1
tests/docpath.rs bc4bd7324b2bfb4d500ee1362991ebdba81b337a736f53d5071927b434f24d0b

# Vendoring provenance — craft-libs / craft-ui

Vendored: 2026-10-22

Source repository: https://github.com/storytold/craft-libs
Source revision: 5c874d51eeb3de70e735c8ce87a59a7dedd47675
Path within source: crates/ui/
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
  upstream `[workspace.lints]`; `egui` and `serde` dependencies written as literals from the
  upstream `[workspace.dependencies]` (`egui 0.36 default-features = false`, `serde 1` with
  `derive` optional).
- `dev-dependencies` preserved as written against crates.io (`egui_extras`, `egui_kittest`,
  `image`, `serde_json`).

Verification:
- `cargo metadata --no-deps` resolves `craft-ui 0.1.0` at this path with `source: None`.
- `cargo check --ignore-rust-version` on the vendored crate fails only in `epaint` (a transitive
  dependency of `egui`), which uses the unstable library feature `push_mut` (rust-lang/rust#135974)
  unsupported by this host's stable rustc 1.94.0. That failure exists identically with the git
  dependency and is unrelated to vendoring; craft-ui's own source compiles up to that point.
- Full CADCraft build (`cargo build -p cadcraft-ui-egui`) is blocked by the same pre-existing,
  unrelated `epaint` incompatibility, not by vendoring.

Integrity (sha256):
Cargo.toml a987c96890bf2c374c3383e6c8c96387e7b46ea8b1b5742b8a9ed705dcdac46a
LICENSE-APACHE 0d542e0c8804e39aa7f37eb00da5a762149dc682d7829451287e11b938e94594
LICENSE-MIT 2f42c0cafc8ad394d91373fe1f342aee5453850b151a51405c68cae9ada86d26
NOTICE 0690a5182986ea6706896673ccf615b0f0d9764f7c5cbba8456970ede0562a43
README.md bd804786082247e47383718db5b98c8a8628c561195727966007f8e0748eb0bb
src/lib.rs 79c4a1b3e2a516cf0493ed27467f69279954d60308cb9914f7e6924505855624
src/buttons/mod.rs 31c5b78e75fc9aacbf1c9b01e728dbf301ff5eceab02215bcf997d2de66c6c3a
src/buttons/tests.rs 4407d99340493d3b216e3a8b0c72a303e88ddd8e17c9934ac1bf18fc2a7dd201
src/docking/model.rs 1184fe72595ea4d7d820fb900e6105cdf25bc8f4921116a84250ebe68f90d344
src/docking/mod.rs a375ec1fc9363deaaca9154930cdba4948a70d5fe61a7e408b410d24fc372187
src/docking/surface.rs 6d94cebcbd053f527c9da8c98f8d522b0343a58a88e322482cbd926a1f750f38
src/docking/targets.rs 223eeb50894d42a13a2b3fd8dfae5f21dd74a0688344623b317c04c740171cd6
src/docking/tests.rs 217d4fe02d0e8298eb0e64049ce723247fd80f1c838b933e318c
src/icons/mod.rs 389872630f1668d48c29a09d15e3100a663d04fabf98b5d8ca02c00712d7f0f0
src/icons/tests.rs c2c35b8aa6448826fc140493001d25b4f6e3d586e52c917f89d179621726f609
src/layout/geometry.rs e5b35a34d89ca1d9e54364a7e2c1b2d0cd0ffabf7314339da3f9e912be72e4e4
src/layout/mod.rs 31f0f2afde706a85b1b2f863bcd5d63190715017421c94dc28feefaa0c43d97
src/layout/resize.rs 278d221cd08aa1f860747dd66df7e49504da5d8b711b9a787a527e461ecd2fd3
src/layout/tests.rs 42f4b5b1d14e42980b91883e1b8e4a294b37d3c2d1d09d70aa448c33ade1655d
src/layout/toolbar.rs 5b3f3f81d135f9d2b09c595c2ffe517ae90cd763e8574964b5c11c1e5a180d83
src/tabs/mod.rs 8cdf333b38f6b6f2a7f48f5517328dd070500443f8a69cf08a5ce0e654b71e44
src/tabs/tests.rs 43bd4e6b131850361b7d4db78ca361e733180bb20d31cf9da8e076448b9aad
src/theme/mod.rs 48ba3e6f55415f0d4392208d51bb8f9223c8fb90ea56a6c04d0cad606a5e797a
tests/docking_workflows.rs 60347371e463a7156e7a05be4e4c3221c777e41222edc219646970ede0562a43
tests/visual/fixtures.rs c3bda942e405b9513d7318e4902aa9f6538568532d524562cc72582946b5e077
tests/visual_regression.rs ec80a3cb5a64b3baf6dae59230b77742059ad2ee3f8b5a96ea7f79929ce47008

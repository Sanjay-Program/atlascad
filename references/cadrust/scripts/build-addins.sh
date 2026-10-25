#!/usr/bin/env bash
# Build every Rust add-in in addins/ to wasm32 and copy the modules into addins/dist/.
set -euo pipefail
cd "$(dirname "$0")/.."
cargo build --release --target wasm32-unknown-unknown -p gear-generator
mkdir -p addins/dist
cp target/wasm32-unknown-unknown/release/gear_generator.wasm addins/dist/gear-generator.wasm
ls -la addins/dist

#!/usr/bin/env bash
# Runs the same advisory, ban and source checks as the CI "advisories" job.
#
# This needs the network and a cargo-deny binary; it is deliberately not part of gate.sh, which must
# stay runnable offline. CI runs it through EmbarkStudios/cargo-deny-action.
#
#   ./scripts/advisories.sh
#
# Install cargo-deny with:
#   cargo install cargo-deny --locked --root "$HOME/.local"
set -euo pipefail
cd "$(dirname "$0")/.."

if ! command -v cargo-deny >/dev/null 2>&1; then
  echo "cargo-deny is not installed." >&2
  echo "install it with: cargo install cargo-deny --locked --root \"\$HOME/.local\"" >&2
  exit 127
fi

echo "cargo-deny $(cargo-deny --version)"
cargo deny check advisories bans sources

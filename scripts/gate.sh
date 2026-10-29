#!/usr/bin/env bash
# The release gate. Every claim in the docs is checked here, offline, in this order.
#
#   ./scripts/gate.sh          # everything, including the live HTTP simulation
#   ./scripts/gate.sh --quick  # skip the simulation (fast edit loop)
#
# The jobs count defaults to 2 because the recorded development host is memory-constrained.
# Override with JOBS=4. Every step runs; the script exits with the first failure.
set -euo pipefail
cd "$(dirname "$0")/.."

JOBS="${JOBS:-2}"
MODE="${1:-}"
QUICK=0
if [ "$MODE" = "--quick" ]; then
  QUICK=1
elif [ -n "$MODE" ]; then
  echo "usage: $0 [--quick]" >&2
  exit 2
fi

step() {
  printf '\n==> %s\n' "$1"
}

step "format"
cargo fmt --all --check

step "clippy (all targets, warnings are errors)"
cargo clippy --offline --workspace --all-targets -j "$JOBS" -- -D warnings

step "workspace tests"
cargo test --offline --workspace -j "$JOBS"

step "analytic kernel corpus"
cargo run --offline -q -p cad-kernel-truck --example corpus

step "engineering manifests"
cargo run --offline -q -p cad-engineering --bin atlas-eng -- validate

step "dependency licence policy"
cargo run --offline -q -p cad-engineering --bin atlas-eng -- licences

step "SBOM (dry run: fails if the graph cannot be read)"
cargo run --offline -q -p cad-engineering --bin atlas-eng -- sbom

if [ "$QUICK" -eq 0 ]; then
  step "live simulation: HTTP server, CLI, control plane and browser"
  ./scripts/simulate.sh
else
  step "live HTTP simulation (skipped: --quick)"
fi

printf '\n==> gate passed\n'

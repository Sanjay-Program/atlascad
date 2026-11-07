#!/usr/bin/env bash
# Verify every stack in this repository, not just the root platform.
#
# The repository contains one root Cargo workspace (the AtlasCAD platform) and eight independent
# source checkouts: six geometry kernels and two applications. Each has its own manifest, toolchain
# and external requirements, so "does it build" has a different answer for each. This script asks
# the question consistently and reports the answer per stack.
#
#   ./scripts/verify-stacks.sh                  # resolve + `cargo check` (fast, default)
#   ./scripts/verify-stacks.sh --metadata       # resolve manifests only (seconds)
#   ./scripts/verify-stacks.sh --test           # resolve + `cargo test`
#   ./scripts/verify-stacks.sh --stack truck    # one stack
#   ./scripts/verify-stacks.sh --workspace      # widen check/test to every workspace member
#   ./scripts/verify-stacks.sh --timeout 1200   # per-stack wall clock (default 900 s)
#
# Several checkouts carry their own `rust-toolchain.toml` (monstertruck pins nightly, hagane pins
# 1.99.0), so the first command in one of them may install a toolchain through rustup. That is a
# one-time download per toolchain, not a hang; CARGO_NET_OFFLINE=false is set for stacks that need
# a registry fetch, and CARGO_NET_OFFLINE=true when --offline is passed.
#
# Network is allowed: several checkouts have dependencies that are not in the local Cargo cache,
# and `--offline` turns "needs a fetch" into a misleading "no matching package". Pass --offline to
# force the cached graph.
#
# Exit status: 0 when every selected stack passes, 1 when any stack fails, 2 on a usage error.
# A stack that cannot build because a documented external requirement is missing is a FAIL with the
# requirement named, not a silent skip: the point is to know which of the six kernels actually run.
#
# A stack marked KNOWN-DEFECT in the table (a pre-existing breakage that is reported but not being
# repaired here) prints as KNOWN and does not fail the run, the same way the kernel corpus treats a
# recorded `known-fail` fixture. Fixing it turns the line into PASS on its own.
#
# Logs: dist/stack-verification/<stack>.log (git-ignored), one per stack.
set -uo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."

MODE=check
WIDEN=""
ONLY=""
TIMEOUT=900
OFFLINE=()
JOBS="${JOBS:-2}"

while [ $# -gt 0 ]; do
    case "$1" in
        --metadata) MODE=metadata ;;
        --check) MODE=check ;;
        --test) MODE=test ;;
        --workspace) WIDEN="--workspace" ;;
        --offline) OFFLINE=(--offline) ;;
        --stack)
            shift
            [ $# -gt 0 ] || { echo "verify-stacks.sh: --stack needs a name" >&2; exit 2; }
            ONLY="$1"
            ;;
        --timeout)
            shift
            [ $# -gt 0 ] || { echo "verify-stacks.sh: --timeout needs seconds" >&2; exit 2; }
            TIMEOUT="$1"
            ;;
        -h | --help)
            sed -n '2,27p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "verify-stacks.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

LOGDIR=dist/stack-verification
mkdir -p "$LOGDIR"

# name|path|external requirement, if any
STACKS=(
    "platform|.|root Rust workspace"
    "kernel-truck|kernels/truck|pure Rust"
    "kernel-monstertruck|kernels/monstertruck|nightly toolchain (rust-toolchain.toml)"
    "kernel-hagane|kernels/hagane|pure Rust"
    "kernel-cascade-rs|kernels/cascade-rs|pure Rust"
    "kernel-cadrum|kernels/cadrum|OpenCASCADE: prebuilt tarball downloaded by build.rs, or OCCT_SOURCE"
    "kernel-opencascade-rs|kernels/opencascade-rs|OpenCASCADE C++ build via occt-sys/cmake"
    "app-cadcraft|apps/cadcraft|git dependencies from github.com/storytold/craft-libs"
    "app-solidcraft|apps/solidcraft|~2.5 GB RSS for solidcraft-kernel; needs memory"
    "crate-cad-constraints|crates/cad-constraints|manifest only: no src/ and no [lib]/[bin], so cargo cannot parse it|known"
)

results=()

run_stack() {
    local name="$1" path="$2" requirement="$3" known="${4:-}"
    local log="$LOGDIR/$name.log"
    local started ended status detail
    started="$(date +%s)"

    if [ ! -f "$path/Cargo.toml" ]; then
        detail="no Cargo.toml at $path"
        if [ "$known" = "known" ]; then
            results+=("KNOWN|$name|0|$detail")
            printf '  %-22s KNOWN %s\n' "$name" "$detail"
        else
            results+=("FAIL|$name|0|$detail")
            printf '  %-22s FAIL  %s\n' "$name" "$detail"
        fi
        return
    fi

    # `--offline` is a cargo global option and goes before the subcommand; `--manifest-path` is a
    # per-subcommand option and must come after it. Putting both up front made every stack fail with
    # "unexpected argument '--manifest-path'" — caught by running this script against itself.
    local cargo=(cargo "${OFFLINE[@]}")
    case "$MODE" in
        metadata) cargo+=(metadata --no-deps --format-version 1 --manifest-path "$path/Cargo.toml") ;;
        check) cargo+=(check -j "$JOBS" $WIDEN --manifest-path "$path/Cargo.toml") ;;
        test) cargo+=(test -j "$JOBS" $WIDEN --manifest-path "$path/Cargo.toml") ;;
    esac

    : >"$log"
    printf '%s\n' "== $name ($path)" "== mode: $MODE ${WIDEN}" "== requires: $requirement" >>"$log"
    timeout "$TIMEOUT" "${cargo[@]}" >>"$log" 2>&1
    status=$?
    ended="$(date +%s)"

    if [ "$status" -eq 0 ]; then
        results+=("PASS|$name|$((ended - started))|$requirement")
        printf '  %-22s PASS  %ss\n' "$name" "$((ended - started))"
    elif [ "$status" -eq 124 ]; then
        results+=("FAIL|$name|$((ended - started))|timed out after ${TIMEOUT}s; see $log")
        printf '  %-22s FAIL  timed out after %ss (%s)\n' "$name" "$TIMEOUT" "$log"
    else
        detail="$(grep -m1 -E '^(error|Error|error\[)' "$log" | cut -c1-140)"
        [ -n "$detail" ] || detail="exit $status; see $log"
        if [ "$known" = "known" ]; then
            results+=("KNOWN|$name|$((ended - started))|$detail")
            printf '  %-22s KNOWN %s\n' "$name" "$detail"
        else
            results+=("FAIL|$name|$((ended - started))|$detail")
            printf '  %-22s FAIL  %s\n' "$name" "$detail"
        fi
    fi
}

printf 'verify-stacks: mode=%s, jobs=%s, timeout=%ss, members=%s\n\n' \
    "$MODE" "$JOBS" "$TIMEOUT" "${WIDEN:-default}"

for entry in "${STACKS[@]}"; do
    IFS='|' read -r name path requirement known <<<"$entry"
    if [ -n "$ONLY" ] && [ "$ONLY" != "$name" ]; then
        continue
    fi
    run_stack "$name" "$path" "$requirement" "$known"
done

failures=0
known_failures=0
for row in "${results[@]}"; do
    IFS='|' read -r status _ _ _ <<<"$row"
    case "$status" in
        PASS) ;;
        KNOWN) known_failures=$((known_failures + 1)) ;;
        *) failures=$((failures + 1)) ;;
    esac
done

printf '\n%d stack(s) verified, %d failed, %d known defect(s). Logs: %s/\n' \
    "${#results[@]}" "$failures" "$known_failures" "$LOGDIR"
[ "$failures" -eq 0 ]

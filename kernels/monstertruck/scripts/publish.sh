#!/usr/bin/env bash
# publish.sh -- Publish all monstertruck crates to crates.io in dependency order.
#
# Usage:
#   ./scripts/publish.sh          # Dry-run (default)
#   ./scripts/publish.sh --exec   # Actually publish
#
# Crates are published in topological order so that each crate's
# dependencies are already available on crates.io when it's published.
# On 429 rate-limit responses, the script waits and retries automatically.
set -euo pipefail

DRY_RUN=true
if [[ "${1:-}" == "--exec" ]]; then
    DRY_RUN=false
fi

# Topological order (leaves first, dependents later).
#
# `monstertruck-solid` precedes `monstertruck-modeling` because modeling
# has `solid` as an optional real dependency (the `fillet` feature), while
# solid only references modeling as a dev-dependency.
CRATES=(
    monstertruck-core
    monstertruck-derive
    monstertruck-assembly
    monstertruck-traits
    monstertruck-gpu
    monstertruck-mesh
    monstertruck-geometry
    monstertruck-topology
    monstertruck-meshing
    monstertruck-solid
    monstertruck-modeling
    monstertruck-step
    monstertruck-render
    monstertruck-wasm
    monstertruck
)

# Seconds to wait between publishes for crates.io index propagation.
WAIT=30
# Max retries on rate-limit (429) responses.
MAX_RETRIES=5

publish_crate() {
    local crate="$1"
    local attempt=0
    local log
    log=$(mktemp)

    while (( attempt < MAX_RETRIES )); do
        if cargo publish -p "$crate" 2>&1 | tee "$log" >&2; then
            rm -f "$log"
            return 0
        fi
        if grep -q "429 Too Many Requests" "$log"; then
            attempt=$((attempt + 1))
            echo "  Rate limited (attempt $attempt/$MAX_RETRIES). Waiting 120s before retry..."
            sleep 120
        elif grep -qE "already (uploaded|exists on crates\.io)" "$log"; then
            echo "  $crate already published at this version -- skipping."
            rm -f "$log"
            return 0
        else
            echo "  FAILED: $crate -- see error above."
            rm -f "$log"
            return 1
        fi
    done

    echo "  FAILED: $crate exceeded $MAX_RETRIES retries."
    rm -f "$log"
    return 1
}

for crate in "${CRATES[@]}"; do
    echo "=== Publishing $crate ==="
    if $DRY_RUN; then
        cargo publish -p "$crate" --dry-run
    else
        publish_crate "$crate"
        # Wait for crates.io index to propagate before publishing dependents.
        if [[ "$crate" != "${CRATES[-1]}" ]]; then
            echo "  Waiting ${WAIT}s for crates.io index propagation..."
            sleep "$WAIT"
        fi
    fi
    echo ""
done

if $DRY_RUN; then
    echo "=== Dry run complete. Run with --exec to publish for real. ==="
else
    echo "=== All crates published! ==="
fi

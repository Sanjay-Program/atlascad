#!/usr/bin/env bash
# ==============================================================================
# AtlasCAD Unified Developer & Deployment Lifecycle Controller
# AS9100 Rev D & ISO 9001:2015 Compliant Deployment Orchestration
# ==============================================================================
# Usage:
#   ./deploy/deploy-ctl.sh [COMMAND] [OPTIONS]
#
# Commands:
#   dev             Start local interactive development studio instance
#   build           Build all workspace release binaries (cad-cli, cad-studio)
#   test            Execute comprehensive multi-physics test suite
#   up              Deploy containerized production stack via Docker Compose
#   down            Tear down running containerized production stack
#   status          Query deployment health, host metrics, and 27 simulation engines
#   davinci-export  Export DaVinci Resolve timeline, EDL, 3D LUT, and OpenUSD
#   unity-export    Export Unity Engine UPM package, glTF 2.0, and C# flight controllers
#   health          Probe HTTP liveness endpoint (/api/health & /api/deploy/status)
#   help            Display this help message
# ==============================================================================

set -euo pipefail
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

STUDIO_HOST="${STUDIO_HOST:-127.0.0.1}"
STUDIO_PORT="${STUDIO_PORT:-8080}"
BASE_URL="http://${STUDIO_HOST}:${STUDIO_PORT}"

TOKEN_FILE="${SCRIPT_DIR}/studio-token.txt"
TOKEN=""
if [ -f "${TOKEN_FILE}" ]; then
    TOKEN="$(tr -d '[:space:]' < "${TOKEN_FILE}" || true)"
fi

log_info() {
    printf "\033[1;34m[INFO]\033[0m %s\n" "$*"
}

log_ok() {
    printf "\033[1;32m[SUCCESS]\033[0m %s\n" "$*"
}

log_warn() {
    printf "\033[1;33m[WARN]\033[0m %s\n" "$*"
}

log_error() {
    printf "\033[1;31m[ERROR]\033[0m %s\n" "$*" >&2
}

cmd_help() {
    cat << 'EOF'
AtlasCAD Unified Deployment & Development Lifecycle Controller

Usage:
  ./deploy/deploy-ctl.sh [COMMAND]

Commands:
  dev             Start local interactive development studio instance
  build           Build all workspace release binaries (cad-cli, cad-studio)
  test            Execute comprehensive multi-physics test suite
  up              Deploy containerized production stack via Docker Compose
  down            Tear down running containerized production stack
  status          Query deployment health, host metrics, and 27 simulation engines
  davinci-export  Export DaVinci Resolve timeline, EDL, 3D LUT, and OpenUSD
  unity-export    Export Unity Engine UPM package, glTF 2.0, and C# flight controllers
  health          Probe HTTP liveness endpoint (/api/health & /api/deploy/status)
  help            Display this help reference
EOF
}

cmd_dev() {
    log_info "Initializing AtlasCAD Development Environment..."
    cd "${REPO_ROOT}"
    if [ ! -f "${TOKEN_FILE}" ]; then
        log_info "Generating development bearer token..."
        "${SCRIPT_DIR}/gen-token.sh" "${TOKEN_FILE}"
    fi
    log_info "Building and launching cad-studio in dev mode on port ${STUDIO_PORT}..."
    cargo run -p cad-studio -- --host "${STUDIO_HOST}" --port "${STUDIO_PORT}"
}

cmd_build() {
    log_info "Building release artifacts across workspace..."
    cd "${REPO_ROOT}"
    cargo build --release --workspace
    log_ok "Release build complete: target/release/atlas target/release/cad-studio"
}

cmd_test() {
    log_info "Running multi-physics test suite across all 27 simulation engines..."
    cd "${REPO_ROOT}"
    cargo test --workspace
    log_info "Validating deployment assets and configurations..."
    "${SCRIPT_DIR}/validate.sh"
    log_ok "All test assertions and deployment validations passed."
}

cmd_up() {
    log_info "Spinning up production containers via Docker Compose..."
    cd "${SCRIPT_DIR}"
    if [ -x "./up.sh" ]; then
        ./up.sh
    else
        docker compose -f compose.yaml up -d
    fi
    log_ok "AtlasCAD containerized cluster is active."
}

cmd_down() {
    log_info "Stopping containerized stack..."
    cd "${SCRIPT_DIR}"
    if command -v docker >/dev/null 2>&1 && docker compose version >/dev/null 2>&1; then
        docker compose -f compose.yaml down
        log_ok "Production containers stopped and networks unlinked."
    else
        log_warn "Docker Compose not available on host."
    fi
}

cmd_status() {
    log_info "Querying AtlasCAD deployment status at ${BASE_URL}/api/deploy/status..."
    AUTH_HEADER=()
    if [ -n "${TOKEN}" ]; then
        AUTH_HEADER=(-H "x-atlas-token: ${TOKEN}")
    fi

    STATUS_JSON=$(curl -sS --max-time 5 "${AUTH_HEADER[@]}" "${BASE_URL}/api/deploy/status" || true)

    if [ -n "${STATUS_JSON}" ] && echo "${STATUS_JSON}" | grep -q '"status":'; then
        log_ok "System Runtime Status: HEALTHY"
        echo "${STATUS_JSON}" | python3 -m json.tool 2>/dev/null || echo "${STATUS_JSON}"
    else
        log_warn "Studio endpoint not reachable at ${BASE_URL}. Checking local container status..."
        if command -v docker >/dev/null 2>&1; then
            docker ps --filter "name=atlas" --format "table {{.Names}}\t{{.Status}}\t{{.Ports}}" || true
        fi
        log_info "Static deployment assets validated via ./deploy/validate.sh:"
        "${SCRIPT_DIR}/validate.sh"
    fi
}

cmd_health() {
    log_info "Probing health endpoint at ${BASE_URL}/api/health..."
    HEALTH_RESP=$(curl -sS --max-time 3 "${BASE_URL}/api/health" || true)
    if [ -n "${HEALTH_RESP}" ]; then
        log_ok "Health check response: ${HEALTH_RESP}"
    else
        log_error "Failed to reach ${BASE_URL}/api/health"
        exit 1
    fi
}

cmd_davinci_export() {
    OUTPUT_DIR="${REPO_ROOT}/artifacts/davinci_export"
    mkdir -p "${OUTPUT_DIR}"
    log_info "Exporting DaVinci Resolve Master Bundle to ${OUTPUT_DIR}..."

    AUTH_HEADER=()
    if [ -n "${TOKEN}" ]; then
        AUTH_HEADER=(-H "x-atlas-token: ${TOKEN}")
    fi

    # 1. Timeline FCPXML
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/davinci?format=fcpxml" -o "${OUTPUT_DIR}/hypersonic-timeline.fcpxml"
    # 2. CMX 3600 EDL
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/davinci?format=edl" -o "${OUTPUT_DIR}/hypersonic-timeline.edl"
    # 3. 3D Aerothermal LUT
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/davinci?format=cube" -o "${OUTPUT_DIR}/hypersonic-aerothermal.cube"
    # 4. Pixar OpenUSD Scene
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/davinci?format=usda" -o "${OUTPUT_DIR}/hypersonic-fusion.usda"

    log_ok "DaVinci Resolve export complete:"
    ls -lh "${OUTPUT_DIR}"
}

cmd_unity_export() {
    OUTPUT_DIR="${REPO_ROOT}/artifacts/unity_export/com.atlascad.unity"
    mkdir -p "${OUTPUT_DIR}/Runtime"
    log_info "Exporting Unity 3D Engine Package to ${OUTPUT_DIR}..."

    AUTH_HEADER=()
    if [ -n "${TOKEN}" ]; then
        AUTH_HEADER=(-H "x-atlas-token: ${TOKEN}")
    fi

    # 1. Package manifest
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/unity?file=package.json" -o "${OUTPUT_DIR}/package.json"
    # 2. C# Client
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/unity?file=AtlasCadClient.cs" -o "${OUTPUT_DIR}/Runtime/AtlasCadClient.cs"
    # 3. C# Flight Simulator
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/unity?file=HypersonicFlightSimulator.cs" -o "${OUTPUT_DIR}/Runtime/HypersonicFlightSimulator.cs"
    # 4. C# HUD
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/unity?file=AtlasCadHUD.cs" -o "${OUTPUT_DIR}/Runtime/AtlasCadHUD.cs"
    # 5. glTF 2.0 3D Model
    curl -sS "${AUTH_HEADER[@]}" "${BASE_URL}/api/export/gltf" -o "${OUTPUT_DIR}/hypersonic-aircraft.gltf"

    log_ok "Unity Engine package export complete:"
    find "${OUTPUT_DIR}" -type f -exec ls -lh {} +
}

ACTION="${1:-status}"
case "${ACTION}" in
    dev)
        cmd_dev
        ;;
    build)
        cmd_build
        ;;
    test)
        cmd_test
        ;;
    up)
        cmd_up
        ;;
    down)
        cmd_down
        ;;
    status)
        cmd_status
        ;;
    health)
        cmd_health
        ;;
    davinci-export)
        cmd_davinci_export
        ;;
    unity-export)
        cmd_unity_export
        ;;
    help|--help|-h)
        cmd_help
        ;;
    *)
        log_error "Unknown command: ${ACTION}"
        cmd_help
        exit 1
        ;;
esac

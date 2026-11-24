#!/usr/bin/env bash
# ==============================================================================
# AtlasCAD: One-Command Universal Launcher
# ==============================================================================
# Usage:
#   ./run.sh                  # Builds and launches workstation at http://127.0.0.1:7519
#   ./run.sh --open           # Launches and automatically opens default web browser
#   ./run.sh --port 8080      # Custom port
#   ./run.sh --share          # LAN collaboration mode with token
#   ./run.sh --docker         # Launch containerized stack via Docker Compose
# ==============================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
cd "$SCRIPT_DIR"

echo "=========================================================="
echo "  AtlasCAD: World #1 Engineering Workstation & CAE Studio  "
echo "=========================================================="

if [[ "${1:-}" == "--docker" ]]; then
    shift
    if command -v docker >/dev/null 2>&1; then
        echo "==> Starting AtlasCAD via Docker Compose..."
        docker compose up "$@"
        exit 0
    else
        echo "Error: docker command not found." >&2
        exit 1
    fi
fi

if command -v cargo >/dev/null 2>&1; then
    echo "==> Building and launching native AtlasCAD release binary..."
    cargo build --release -p cad-studio
    echo "==> Server active at http://127.0.0.1:7519"
    exec ./target/release/cad-studio --open "$@"
elif command -v docker >/dev/null 2>&1; then
    echo "==> Rust/cargo not found. Falling back to Docker Compose..."
    exec docker compose up "$@"
else
    echo "Error: Neither cargo (Rust) nor docker was found on your system."
    echo "To install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    echo "To install Docker: https://docs.docker.com/get-docker/"
    exit 1
fi

#!/usr/bin/env bash
# Validate the deployment assets without starting anything.
#
#   deploy/validate.sh        # shell syntax, compose interpolation, Caddyfile syntax
#
# This is the deployment half of the repository gate: it catches a broken script, a compose file
# that cannot be interpolated (the usual cause is a missing STUDIO_PUBLIC_HOST), or a Caddyfile that
# will not load, before any of them reach a host.
#
# Docker is optional. Without it the shell checks still run, compose interpolation is skipped with a
# clear message, and the run still succeeds: the gate must work on a machine that only compiles code.
# Caddyfile validation uses the `caddy` binary when present, else the cached caddy image, else it
# reports that it was skipped.
set -uo pipefail
root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

fail=0
note() { printf '    %s\n' "$1"; }
bad() {
    printf '    FAIL %s\n' "$1"
    fail=1
}

echo "==> shell syntax"
for script in deploy/*.sh scripts/*.sh; do
    if bash -n "$script" 2>/dev/null; then
        note "ok   $script"
    else
        bad "$script (bash -n)"
    fi
done

echo "==> compose interpolation"
if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
    note "skipped: the docker compose plugin is not installed"
else
    token="$(mktemp)"
    printf '%s' "0123456789abcdef0123456789abcdef" >"$token"
    export STUDIO_PUBLIC_HOST="validate.invalid"
    export STUDIO_TOKEN_FILE="$token"
    for file in deploy/compose.yaml deploy/compose.smoke.yaml; do
        if docker compose -f "$file" config --quiet >/dev/null 2>&1; then
            note "ok   $file"
        else
            bad "$file (docker compose config)"
            docker compose -f "$file" config --quiet 2>&1 | sed 's/^/         /'
        fi
    done
    rm -f "$token"
    unset STUDIO_PUBLIC_HOST STUDIO_TOKEN_FILE
fi

echo "==> root quickstart compose"
if ! command -v docker >/dev/null 2>&1 || ! docker compose version >/dev/null 2>&1; then
    note "skipped: the docker compose plugin is not installed"
else
    # The quickstart file must interpolate with a token, and must refuse to start without one:
    # an empty token here means an unauthenticated editor, so that is a failure, not a warning.
    if ATLAS_STUDIO_TOKEN=0123456789abcdef0123456789abcdef \
        docker compose -f docker-compose.yml config --quiet >/dev/null 2>&1; then
        note "ok   docker-compose.yml (with ATLAS_STUDIO_TOKEN)"
    else
        bad "docker-compose.yml (docker compose config with ATLAS_STUDIO_TOKEN set)"
        ATLAS_STUDIO_TOKEN=0123456789abcdef0123456789abcdef \
            docker compose -f docker-compose.yml config --quiet 2>&1 | sed 's/^/         /'
    fi
    if ATLAS_STUDIO_TOKEN= docker compose -f docker-compose.yml config --quiet >/dev/null 2>&1; then
        bad "docker-compose.yml starts without ATLAS_STUDIO_TOKEN (open, unauthenticated studio)"
    else
        note "ok   docker-compose.yml refuses to start without a token"
    fi
fi

echo "==> image build inputs"
# A `COPY . .` drags the reference checkouts (tens of GB) into the build context, and a media rule
# in .dockerignore silently removes an asset that lib.rs embeds with include_bytes!, which only
# fails once the whole dependency graph has compiled. Both are cheap to rule out here.
if grep -qE '^[[:space:]]*COPY[[:space:]]+(\.|\./)[[:space:]]' Dockerfile; then
    bad "Dockerfile copies the whole repository; copy explicit build inputs instead"
else
    note "ok   Dockerfile copies explicit build inputs"
fi
missing=0
found=0
while IFS=$'\t' read -r file asset; do
    [ -n "$asset" ] || continue
    found=$((found + 1))
    case "$asset" in
        /*) continue ;;
    esac
    target="$(dirname "$file")/$asset"
    if [ ! -e "$target" ]; then
        bad "embedded asset missing: $target (from $file)"
        missing=1
        continue
    fi
    rel="$(realpath --relative-to="$root" "$target" 2>/dev/null || printf '%s' "$target")"
    # Treat .dockerignore as git's global excludes file: the syntax is compatible, so this catches a
    # media rule that would strip an embedded asset from the build context before cargo reads it.
    if printf '%s\n' "$rel" |
        git -c core.excludesFile="$root/.dockerignore" check-ignore --no-index --stdin >/dev/null 2>&1; then
        bad "excluded by .dockerignore: $rel"
        missing=1
    fi
done < <(grep -rHo --include=*.rs -E 'include_(str|bytes)!\("[^"]+"\)' crates \
    | awk -F: '{ file = $1; if (match($0, /"[^"]+"/)) print file "\t" substr($0, RSTART + 1, RLENGTH - 2) }')
if [ "$found" -eq 0 ]; then
    bad "no include_bytes!/include_str! paths found; the asset scan is not working"
elif [ "$missing" -eq 0 ]; then
    note "ok   $found embedded assets exist and survive .dockerignore"
fi

echo "==> Caddyfile syntax"
caddy_output="$(mktemp)"
# 0 = valid, 1 = invalid (output in $caddy_output), 2 = no caddy available to check with.
caddy_validate() {
    local file="$1"
    if command -v caddy >/dev/null 2>&1; then
        STUDIO_PUBLIC_HOST=validate.invalid STUDIO_UPSTREAM=127.0.0.1:7519 \
            caddy validate --config "$file" --adapter caddyfile >"$caddy_output" 2>&1
        return $?
    fi
    if command -v docker >/dev/null 2>&1; then
        # Prefer the tag the stack uses; otherwise any cached caddy image can check the syntax.
        local image="caddy:${CADDY_IMAGE_TAG:-2}"
        if ! docker image inspect "$image" >/dev/null 2>&1; then
            image="$(docker image ls --format '{{.Repository}}:{{.Tag}}' | grep '^caddy:' | head -1)"
        fi
        if [ -n "$image" ] && docker image inspect "$image" >/dev/null 2>&1; then
            # --entrypoint explicitly: the official image's entrypoint is not applied when a
            # command is given on some docker versions, which made `validate` the executable.
            docker run --rm --entrypoint /usr/bin/caddy \
                -e STUDIO_PUBLIC_HOST=validate.invalid \
                -e STUDIO_UPSTREAM=127.0.0.1:7519 \
                -e STUDIO_LOG_FILE=/dev/stdout \
                -v "$root/$file:/etc/caddy/Caddyfile:ro,z" \
                "$image" validate --config /etc/caddy/Caddyfile --adapter caddyfile \
                >"$caddy_output" 2>&1
            return $?
        fi
    fi
    return 2
}
for file in deploy/Caddyfile deploy/Caddyfile.smoke; do
    caddy_validate "$file"
    status=$?
    case "$status" in
        0) note "ok   $file" ;;
        2) note "skipped $file (no caddy binary and no cached caddy image)" ;;
        *)
            bad "$file (caddy validate)"
            sed 's/^/         /' "$caddy_output"
            ;;
    esac
done
rm -f "$caddy_output"

if [ "$fail" -ne 0 ]; then
    echo "deployment assets are not valid" >&2
    exit 1
fi
echo "deployment assets are valid"

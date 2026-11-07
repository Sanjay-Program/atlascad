#!/usr/bin/env bash
# Write a fresh bearer token for the hosted stack.
#
#   cd deploy && ./gen-token.sh            # writes deploy/studio-token.txt, 0600
#   ./gen-token.sh /path/to/token          # somewhere else (STUDIO_TOKEN_FILE must match)
#
# The compose stack stages this file into a private volume at container start (the `atlas-token`
# one-shot service) and the studio reads it with `--token-file`, so the secret is never a container
# environment variable (which `docker inspect` would show) and never a command-line argument (which
# `ps` would show).
#
# It is NOT an operational secret until you keep it like one: 0600, on the same host, backed up
# separately from the document, never committed. Replacing the file changes nothing until the stack
# restarts (`docker compose up -d --force-recreate atlas-studio`), because the token is read once at
# startup.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

target="${1:-studio-token.txt}"
umask 077
head -c 32 /dev/urandom | od -An -tx1 | tr -d ' \n' >"$target"
chmod 0600 "$target"
printf 'ok: wrote %s (%s characters, 0600)\n' "$target" "$(wc -c <"$target" | tr -d ' ')"

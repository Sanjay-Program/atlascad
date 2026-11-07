#!/usr/bin/env bash
# Restore a document-volume backup into the hosted stack's volume.
#
#   cd deploy && ./restore.sh --file backups/atlas-studio-data-20261107T101500Z.tar.gz --yes
#
# It refuses while the stack is running (the studio is the only writer and its file is the state),
# verifies the archive's .sha256 when one sits beside it, takes a safety copy of the current volume
# first, and then replaces the volume contents with the archive as root so uid 10001 ownership is
# preserved.
#
# After a successful restore, start the stack again with ./up.sh or
# `docker compose -f compose.yaml up -d --wait`.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

PROJECT="${STUDIO_PROJECT:-atlas}"
VOLUME="${STUDIO_VOLUME:-${PROJECT}_atlas-studio-data}"
OUT="./backups"
FILE=""
YES=0
IMAGE="${BACKUP_IMAGE:-debian:bookworm-slim}"

while [ $# -gt 0 ]; do
    case "$1" in
        --file)
            shift
            [ $# -gt 0 ] || { echo "restore.sh: --file needs an archive" >&2; exit 2; }
            FILE="$1"
            ;;
        --out)
            shift
            [ $# -gt 0 ] || { echo "restore.sh: --out needs a directory" >&2; exit 2; }
            OUT="$1"
            ;;
        --volume)
            shift
            [ $# -gt 0 ] || { echo "restore.sh: --volume needs a name" >&2; exit 2; }
            VOLUME="$1"
            ;;
        --yes) YES=1 ;;
        -h | --help)
            sed -n '2,17p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "restore.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

if [ -z "$FILE" ]; then
    echo "restore.sh: --file <archive.tar.gz> is required" >&2
    exit 2
fi
if [ ! -f "$FILE" ]; then
    echo "restore.sh: $FILE does not exist" >&2
    exit 1
fi

if [ -f "$FILE.sha256" ]; then
    echo "==> verifying $FILE.sha256"
    (cd "$(dirname "$FILE")" && sha256sum -c "$(basename "$FILE").sha256")
fi

if ! docker volume inspect "$VOLUME" >/dev/null 2>&1; then
    echo "restore.sh: volume $VOLUME does not exist; create it with 'docker volume create $VOLUME'" >&2
    exit 1
fi

running="$(docker ps -q --filter "label=com.docker.compose.project=$PROJECT")"
if [ -n "$running" ]; then
    echo "restore.sh: compose project '$PROJECT' is running; stop it first" >&2
    echo "            (docker compose -f compose.yaml down)" >&2
    exit 1
fi

if [ "$YES" -eq 0 ]; then
    if [ ! -t 0 ]; then
        echo "restore.sh: refusing to replace $VOLUME without confirmation; pass --yes" >&2
        exit 1
    fi
    printf 'Replace the contents of volume %s with %s? [y/N] ' "$VOLUME" "$FILE"
    read -r answer
    case "$answer" in
        y | Y | yes | YES) ;;
        *) echo "restore.sh: aborted"; exit 1 ;;
    esac
fi

echo "==> safety copy of the current volume"
./backup.sh --volume "$VOLUME" --out "$OUT" --prefix pre-restore >/dev/null
echo "    kept in $OUT as pre-restore-<utc>.tar.gz"

echo "==> replacing the contents of $VOLUME"
docker run --rm -i --network none -v "$VOLUME":/data "$IMAGE" \
    sh -c 'find /data -mindepth 1 -delete && tar xzf - -C /data' <"$FILE"

echo "ok: restored $FILE into $VOLUME"
echo "    start the stack with ./up.sh"

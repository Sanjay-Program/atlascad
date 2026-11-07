#!/usr/bin/env bash
# Back up the hosted stack's document volume.
#
#   cd deploy && ./backup.sh                 # deploy/backups/atlas-studio-data-<utc>.tar.gz + .sha256
#   ./backup.sh --out /srv/backups --keep 30
#   ./backup.sh --live                       # snapshot while the stack runs (a save boundary is not
#                                            # guaranteed; the studio is single-writer)
#
# The .zcad file is the whole document, so this is the whole backup. The archive is written through
# a one-shot container with the volume mounted read-only; nothing in the stack is modified and no
# image other than the one named by BACKUP_IMAGE is needed.
#
# Restore with ./restore.sh --file <archive> --yes.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")"

PROJECT="${STUDIO_PROJECT:-atlas}"
VOLUME="${STUDIO_VOLUME:-${PROJECT}_atlas-studio-data}"
OUT="./backups"
KEEP=14
LIVE=0
PREFIX="atlas-studio-data"
IMAGE="${BACKUP_IMAGE:-debian:bookworm-slim}"

while [ $# -gt 0 ]; do
    case "$1" in
        --out)
            shift
            [ $# -gt 0 ] || { echo "backup.sh: --out needs a directory" >&2; exit 2; }
            OUT="$1"
            ;;
        --keep)
            shift
            [ $# -gt 0 ] || { echo "backup.sh: --keep needs a count" >&2; exit 2; }
            KEEP="$1"
            ;;
        --volume)
            shift
            [ $# -gt 0 ] || { echo "backup.sh: --volume needs a name" >&2; exit 2; }
            VOLUME="$1"
            ;;
        --prefix)
            shift
            [ $# -gt 0 ] || { echo "backup.sh: --prefix needs a name" >&2; exit 2; }
            PREFIX="$1"
            ;;
        --live) LIVE=1 ;;
        -h | --help)
            sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "backup.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

if ! docker volume inspect "$VOLUME" >/dev/null 2>&1; then
    echo "backup.sh: volume $VOLUME does not exist (docker volume ls)" >&2
    exit 1
fi

running="$(docker ps -q --filter "label=com.docker.compose.project=$PROJECT")"
if [ -n "$running" ] && [ "$LIVE" -eq 0 ]; then
    echo "backup.sh: compose project '$PROJECT' is running, and a copy taken now is only as" >&2
    echo "           consistent as the last save. Stop it first (docker compose -f compose.yaml" >&2
    echo "           down), or pass --live to accept the risk." >&2
    exit 1
fi

mkdir -p "$OUT"
out_abs="$(cd "$OUT" && pwd)"
stamp="$(date -u +%Y%m%dT%H%M%SZ)"
archive="$PREFIX-$stamp.tar.gz"

echo "==> archiving volume $VOLUME"
# tar writes to stdout and the host shell redirects it, so the archive is owned by the operator, not
# by root, and no host directory has to be mounted into the container.
docker run --rm --network none -v "$VOLUME":/data:ro "$IMAGE" tar czf - -C /data . >"$out_abs/$archive"
(
    cd "$out_abs"
    sha256sum "$archive" >"$archive.sha256"
)

# Retention: keep the newest $KEEP archives (and their checksums).
while IFS= read -r old; do
    [ -n "$old" ] || continue
    rm -f "$out_abs/$old" "$out_abs/$old.sha256"
done < <(ls -1t "$out_abs/$PREFIX"-[0-9]*.tar.gz 2>/dev/null | tail -n +$((KEEP + 1)) | xargs -r -n1 basename)

printf 'ok: %s (%s bytes)\n' "$out_abs/$archive" "$(wc -c <"$out_abs/$archive" | tr -d ' ')"
printf '    checksum %s.sha256\n' "$archive"
echo "    restore  ./restore.sh --file $out_abs/$archive --yes"

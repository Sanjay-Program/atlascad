#!/usr/bin/env bash
# Build the Atlas CAM job image, and optionally prove it posts a real part before you ship it.
#
#   deploy/build-cam-image.sh                 # release binary + image
#   deploy/build-cam-image.sh --smoke         # ... and run the job contract end to end
#   deploy/build-cam-image.sh --tag 1.2.3     # a tag other than the crate version
#
# The image is deliberately not built by `docker build` alone: the workspace build needs the host's
# Rust toolchain and crate cache (see deploy/Dockerfile.cam for why), so the binary is compiled here
# and copied in.
#
# --smoke runs the hardened shape an operator is told to run: no network, read-only root, no
# capabilities, a memory cap and two volumes. It asserts a real part end to end (three operations,
# one program with a tool change, Markdown/PDF sheet, preview, summary), that a holder collision keeps
# exit 3 and publishes nothing, that a malformed job exits 2, that the process is not root, that the
# read-only inputs were not modified, and that a job without an output volume is refused.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
tag="atlas-cam:${version}"
smoke=0

while [ $# -gt 0 ]; do
    case "$1" in
        --smoke) smoke=1 ;;
        --tag)
            shift
            [ $# -gt 0 ] || { echo "build-cam-image.sh: --tag needs a value" >&2; exit 2; }
            tag="atlas-cam:$1"
            ;;
        -h | --help)
            sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "build-cam-image.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

echo "==> building the release atlas binary"
cargo build --release --locked -p cad-cli

# The docker context is these three files and nothing else. Handing docker the repository root would
# send it `target/`, `.git/` and every saved document on this machine.
context="$(mktemp -d)"
work=""
results=""
refused_results=""
cleanup() { rm -rf "$context" "$work" "$results" "$refused_results"; }
trap cleanup EXIT
cp target/release/atlas "$context/atlas"
cp deploy/cam-entrypoint.sh "$context/cam-entrypoint.sh"

echo "==> building $tag"
docker build -f deploy/Dockerfile.cam -t "$tag" "$context"

if [ "$smoke" -eq 0 ]; then
    echo "ok: $tag"
    echo "    run: docker run --rm --network none --read-only --cap-drop ALL \\"
    echo "           --security-opt no-new-privileges -v INPUTS:/work:ro -v OUTPUTS:/out $tag"
    exit 0
fi

work="$(mktemp -d)"
results="$(mktemp -d)"
refused_results="$(mktemp -d)"
trap cleanup EXIT
# mktemp creates 0700 directories owned by the invoking user; the container runs as uid 10001 and
# must be able to read the inputs and write the output. These are the two permissions the contract
# actually needs.
chmod 0755 "$work"
chmod 0777 "$results" "$refused_results"

printf '==> a known part: 20 x 20 x 12 mm block in a 40 x 30 x 20 mm stock\n'
target/release/atlas box --size 20,20,12 --out "$work/part.stl" >/dev/null

cat >"$work/job.json" <<'JSON'
{
  "schema": 1,
  "job": {
    "name": "container bracket",
    "stock_mm": [40, 30, 20],
    "part": "part.stl",
    "format": "grbl",
    "tool": {
      "diameter_mm": 6,
      "stepover_mm": 4,
      "stepdown_mm": 2,
      "flute_length_mm": 20,
      "stock_to_leave_mm": 0.5,
      "holder": { "diameter_mm": 20, "length_mm": 40 }
    },
    "finish": { "diameter_mm": 6 },
    "resolution_mm": 2
  }
}
JSON

inputs_before="$(sha256sum "$work/job.json" "$work/part.stl" | cut -d' ' -f1 | tr '\n' ' ')"

# The hardened shape is the shape under test. A job that needs a writable path the operator did not
# provide must say so, not quietly write somewhere else.
run_job() { # run_job JOB OUTPUT_DIR
    docker run --rm \
        --network none \
        --read-only \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --memory 512m \
        --pids-limit 256 \
        --tmpfs /tmp:rw,noexec,nosuid,size=16m \
        -v "$work:/work:ro,z" \
        -v "$2:/out:z" \
        -e ATLAS_JOB="/work/$1" \
        -e ATLAS_OUT=/out \
        "$tag"
}

printf '==> successful job\n'
run_job job.json "$results" >/dev/null 2>"$work/job.stderr" || {
    echo "smoke failed: the job exited $?; stderr:" >&2
    cat "$work/job.stderr" >&2
    exit 1
}

python3 - "$results" <<'PY'
import json
import os
import sys

out = sys.argv[1]
summary = json.load(open(os.path.join(out, "summary.json")))
assert summary["operations"] == ["3D roughing", "wall finishing", "3D finishing"], summary
assert summary["moves"] > 100, summary["moves"]
assert summary["wall_moves"] > 10, summary["wall_moves"]
assert summary["finish_moves"] > 10, summary["finish_moves"]
assert summary["removed_mm3"] > 18000.0, summary["removed_mm3"]
assert summary["holder"] == "20 x 40 mm, clear of the stock", summary["holder"]
assert summary["job"].endswith("/job.json"), summary["job"]
assert len(summary["files"]) == 5, summary["files"]

program = open(os.path.join(out, "program.nc")).read()
assert "(change tool to T2 then cycle start)\nM0\n" in program, program
assert program.count("M30") == 1, "one program end"
assert "Z15 " in program, "the ball nose cuts 3 mm above the 12 mm top"

sheet = open(os.path.join(out, "setup.md")).read()
assert "| wall finishing | T1 |" in sheet, sheet
assert "| 3D finishing | T2 |" in sheet, sheet

pdf = open(os.path.join(out, "setup.pdf"), "rb").read()
assert pdf.startswith(b"%PDF-1.4\n"), "setup.pdf is not a PDF"
assert pdf.endswith(b"%%EOF\n"), "setup.pdf has no trailer"
assert os.path.getsize(os.path.join(out, "stock-preview.txt")) > 0

print(
    f"ok: {summary['moves']} moves in one program, {summary['removed_mm3']:.0f} mm3 removed, "
    f"{summary['estimated_minutes']:.1f} min, {len(summary['files'])} files"
)
PY

inputs_after="$(sha256sum "$work/job.json" "$work/part.stl" | cut -d' ' -f1 | tr '\n' ' ')"
[ "$inputs_before" = "$inputs_after" ] || {
    echo "smoke failed: the read-only input volume was modified" >&2
    exit 1
}

printf '==> a holder collision is refused and publishes nothing\n'
target/release/atlas box --size 20,20,19 --out "$work/island.stl" >/dev/null
python3 - "$work/job.json" "$work/collision.json" <<'PY'
import json
import sys

job = json.load(open(sys.argv[1]))
job["job"]["name"] = "container collision"
job["job"]["part"] = "island.stl"
job["job"]["tool"]["flute_length_mm"] = 8
job["job"].pop("finish", None)
json.dump(job, open(sys.argv[2], "w"))
PY
status=0
run_job collision.json "$refused_results" >/dev/null 2>"$work/collision.stderr" || status=$?
[ "$status" -eq 3 ] || {
    echo "smoke failed: a holder collision exited $status, expected 3" >&2
    cat "$work/collision.stderr" >&2
    exit 1
}
[ ! -e "$refused_results/program.nc" ] || {
    echo "smoke failed: a refused job published a program" >&2
    exit 1
}
grep -q "holder enters the stock" "$work/collision.stderr" || {
    echo "smoke failed: the refusal does not name the collision" >&2
    cat "$work/collision.stderr" >&2
    exit 1
}

printf '==> a malformed job is bad input\n'
printf '{not json' >"$work/broken.json"
status=0
run_job broken.json "$refused_results" >/dev/null 2>"$work/broken.stderr" || status=$?
[ "$status" -eq 2 ] || {
    echo "smoke failed: a malformed job exited $status, expected 2" >&2
    cat "$work/broken.stderr" >&2
    exit 1
}

printf '==> the job runs unprivileged\n'
uid="$(docker run --rm --entrypoint /usr/bin/id "$tag" -u)"
[ "$uid" = "10001" ] || {
    echo "smoke failed: the job runs as uid $uid, expected 10001" >&2
    exit 1
}

printf '==> a job without an output volume is refused, not published into nowhere\n'
status=0
docker run --rm \
    --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
    -v "$work:/work:ro,z" \
    -e ATLAS_JOB=/work/job.json \
    "$tag" >/dev/null 2>&1 || status=$?
[ "$status" -eq 2 ] || {
    echo "smoke failed: a job without an output volume exited $status, expected 2" >&2
    exit 1
}

echo "ok: $tag posted a part, changed tools, refused a collision, stayed unprivileged"

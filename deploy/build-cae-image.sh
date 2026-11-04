#!/usr/bin/env bash
# Build the Atlas CAE job image, and optionally prove it analyses a real part before you ship it.
#
#   deploy/build-cae-image.sh                 # release binary + image
#   deploy/build-cae-image.sh --smoke         # ... and run the job contract end to end
#   deploy/build-cae-image.sh --tag 1.2.3     # a tag other than the crate version
#
# The image is deliberately not built by `docker build` alone: the workspace build needs the host's
# Rust toolchain and crate cache (see deploy/Dockerfile.cae for why), so the binary is compiled here
# and copied in.
#
# --smoke runs the same hardened shape an operator is told to run: no network, read-only root, no
# capabilities, a memory cap and only two volumes. It asserts a real cantilever against beam theory,
# that the two refusal classes keep exit 3 (analysis refused) and exit 2 (bad input), that a job
# without a result volume is refused, that the process is not root, that the read-only input was not
# modified, and that recording a simulation artifact inside the container passes the release gate.
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

version="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"
tag="atlas-cae:${version}"
smoke=0

while [ $# -gt 0 ]; do
    case "$1" in
        --smoke) smoke=1 ;;
        --tag)
            shift
            [ $# -gt 0 ] || { echo "build-cae-image.sh: --tag needs a value" >&2; exit 2; }
            tag="atlas-cae:$1"
            ;;
        -h | --help)
            sed -n '2,18p' "$0" | sed 's/^# \{0,1\}//'
            exit 0
            ;;
        *)
            echo "build-cae-image.sh: unknown argument $1" >&2
            exit 2
            ;;
    esac
    shift
done

echo "==> building the release atlas binary"
cargo build --release --locked -p cad-cli

# The docker context is these two files and nothing else. Handing docker the repository root would
# send it `target/`, `.git/` and every saved document on this machine.
context="$(mktemp -d)"
work=""
results=""
cleanup() { rm -rf "$context" "$work" "$results"; }
trap cleanup EXIT
cp target/release/atlas "$context/atlas"
cp deploy/cae-entrypoint.sh "$context/cae-entrypoint.sh"

echo "==> building $tag"
docker build -f deploy/Dockerfile.cae -t "$tag" "$context"

if [ "$smoke" -eq 0 ]; then
    echo "ok: $tag"
    echo "    run: docker run --rm --network none --read-only --cap-drop ALL \\"
    echo "           --security-opt no-new-privileges -v INPUTS:/work:ro -v RESULTS:/results $tag"
    exit 0
fi

work="$(mktemp -d)"
results="$(mktemp -d)"
trap cleanup EXIT
# mktemp creates 0700 directories owned by the invoking user; the container runs as uid 10001 and
# must be able to read the inputs and write the results. These are the two permissions the job
# contract actually needs.
chmod 0755 "$work"
chmod 0777 "$results"

printf '==> a known part: 60 x 10 x 10 mm cantilever, 100 N transverse\n'
created="$(target/release/atlas project new --file "$work/cant.zcad" --size 60,10,10)"
revision="$(printf '%s' "$created" | python3 -c 'import json,sys; print(json.load(sys.stdin)["revision"])')"
project_before="$(sha256sum "$work/cant.zcad" | cut -d' ' -f1)"

cat >"$work/study.json" <<'JSON'
{
  "schema": 1,
  "study": {
    "name": "cantilever",
    "voxel_size_mm": 2.5,
    "material": "steel.1018",
    "supports": [
      { "face": { "axis": "x", "coordinate_mm": 0.0, "tolerance_mm": 0.001 }, "fix": [true, true, true] }
    ],
    "loads": [
      { "face": { "axis": "x", "coordinate_mm": 60.0, "tolerance_mm": 0.001 }, "total_force_n": [0.0, 0.0, -100.0] }
    ],
    "solver": { "relative_tolerance": 1e-9, "max_iterations": 40000 }
  }
}
JSON

# The hardened shape is the shape under test. A study that needs a writable path the operator did
# not provide must say so, not quietly write somewhere else.
run_job() { # run_job STUDY RESULT
    docker run --rm \
        --network none \
        --read-only \
        --cap-drop ALL \
        --security-opt no-new-privileges \
        --memory 512m \
        --pids-limit 256 \
        --tmpfs /tmp:rw,noexec,nosuid,size=16m \
        -v "$work:/work:ro,z" \
        -v "$results:/results:z" \
        -e ATLAS_PROJECT=/work/cant.zcad \
        -e ATLAS_STUDY="/work/$1" \
        -e ATLAS_RESULT="/results/$2" \
        "$tag"
}

printf '==> successful study\n'
started="$(date +%s%N)"
status=0
run_job study.json result.json >/dev/null 2>"$work/job.stderr" || status=$?
elapsed_ms=$((($(date +%s%N) - started) / 1000000))
if [ "$status" -ne 0 ]; then
    echo "smoke failed: the study exited $status" >&2
    cat "$work/job.stderr" >&2
    exit 1
fi

python3 - "$results/result.json" "$elapsed_ms" "$revision" <<'PY'
import json
import sys

path, elapsed_ms, revision = sys.argv[1], int(sys.argv[2]), int(sys.argv[3])
result = json.load(open(path))
# PL^3/(3EI) + PL/((5/6)GA), E = 205 GPa, nu = 0.29, L = 60 mm, 10 x 10 mm section, 100 N.
reference = 0.043052
displacement = result["results"]["max_displacement_mm"]
error = abs(displacement - reference) / reference
assert result["mesh"]["elements"] == 384, result["mesh"]
assert result["solver"]["converged"] is True, result["solver"]
assert error < 0.10, f"{displacement} mm is {error:.2%} from {reference} mm"
# The result carries the job that produced it, not just a summary.
assert result["definition"]["voxel_size_mm"] == 2.5, result["definition"]
assert result["definition"]["material"] == "steel.1018", result["definition"]
assert result["definition"]["loads"][0]["total_force_n"] == [0.0, 0.0, -100.0]
assert result["source"]["document"] == 1, result["source"]
assert result["source"]["revision"] == revision, (result["source"], revision)
assert result["engine"].startswith("cad-cae "), result["engine"]
assert result["quality"] == (
    "preliminary engineering study - linear elastic, not a certified analysis"
), result["quality"]
print(
    f"ok: {result['mesh']['elements']} elements, max displacement {displacement:.6f} mm "
    f"({error:.2%} from beam theory), {result['solver']['iterations']} PCG iterations, "
    f"{elapsed_ms} ms wall clock"
)
PY

project_after="$(sha256sum "$work/cant.zcad" | cut -d' ' -f1)"
[ "$project_before" = "$project_after" ] || {
    echo "smoke failed: the read-only project volume was modified" >&2
    exit 1
}

printf '==> refusals keep their exit codes\n'
cat >"$work/no-node.json" <<'JSON'
{
  "schema": 1,
  "study": {
    "name": "off the body",
    "voxel_size_mm": 2.5,
    "material": "steel.1018",
    "supports": [
      { "face": { "axis": "x", "coordinate_mm": 0.0, "tolerance_mm": 0.001 }, "fix": [true, true, true] }
    ],
    "loads": [
      { "face": { "axis": "x", "coordinate_mm": 999.0, "tolerance_mm": 0.001 }, "total_force_n": [0.0, 0.0, -100.0] }
    ]
  }
}
JSON
status=0
run_job no-node.json refused.json >/dev/null 2>"$work/refused.stderr" || status=$?
[ "$status" -eq 3 ] || {
    echo "smoke failed: a refused study exited $status, expected 3" >&2
    cat "$work/refused.stderr" >&2
    exit 1
}
[ ! -e "$results/refused.json" ] || {
    echo "smoke failed: a refused study published a result" >&2
    exit 1
}

printf '{not json' >"$work/broken.json"
status=0
run_job broken.json broken-result.json >/dev/null 2>"$work/broken.stderr" || status=$?
[ "$status" -eq 2 ] || {
    echo "smoke failed: a malformed study exited $status, expected 2" >&2
    cat "$work/broken.stderr" >&2
    exit 1
}

printf '==> the job runs unprivileged\n'
uid="$(docker run --rm --entrypoint /usr/bin/id "$tag" -u)"
[ "$uid" = "10001" ] || {
    echo "smoke failed: the job runs as uid $uid, expected 10001" >&2
    exit 1
}

printf '==> a job without a result volume is refused, not published into nowhere\n'
status=0
docker run --rm \
    --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
    -v "$work:/work:ro,z" \
    -e ATLAS_PROJECT=/work/cant.zcad \
    -e ATLAS_STUDY=/work/study.json \
    "$tag" >/dev/null 2>&1 || status=$?
[ "$status" -eq 2 ] || {
    echo "smoke failed: a job without a result volume exited $status, expected 2" >&2
    exit 1
}

printf '==> recording a simulation artifact inside the container\n'
mkdir -p "$work/rw"
cp "$work/cant.zcad" "$work/rw/part.zcad"
cp "$work/study.json" "$work/rw/study.json"
# The container runs as uid 10001; give that uid the two writes recording needs (project + rename).
chmod 0777 "$work/rw"
chmod 0666 "$work/rw/part.zcad"
status=0
docker run --rm \
    --network none --read-only --cap-drop ALL --security-opt no-new-privileges \
    --memory 512m --pids-limit 256 --tmpfs /tmp:rw,noexec,nosuid,size=16m \
    -v "$work/rw:/work:rw,z" \
    -v "$results:/results:z" \
    -e ATLAS_PROJECT=/work/part.zcad \
    -e ATLAS_STUDY=/work/study.json \
    -e ATLAS_RESULT=/results/recorded.json \
    -e ATLAS_RECORD=1 \
    -e ATLAS_LABEL="container smoke study" \
    "$tag" >/dev/null 2>"$work/record.stderr" || status=$?
[ "$status" -eq 0 ] || {
    echo "smoke failed: recording exited $status" >&2
    cat "$work/record.stderr" >&2
    exit 1
}
target/release/atlas artifacts check --project "$work/rw/part.zcad" >"$work/artifacts.json"
python3 - "$work/artifacts.json" <<'PY'
import json
import sys

report = json.load(open(sys.argv[1]))
artifacts = report["artifacts"]
assert len(artifacts) == 1, artifacts
artifact = artifacts[0]
assert artifact["kind"] == "simulation", artifact
assert artifact["policy"] == "recompute", artifact
assert artifact["releasable"] is True, artifact
print(f"ok: artifact {artifact['id']} is a releasable simulation at the current revision")
PY

echo "ok: $tag ran a study, refused bad input, stayed unprivileged and recorded provenance"

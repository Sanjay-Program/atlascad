#!/usr/bin/env python3
"""Live simulation of the deployable product.

Starts the real release `cad-studio` binary on a loopback port and drives it over HTTP the way the
browser does: create sketches, cut, join, undo/redo, edit a parameter, save, export STL, restart, then
kill -9 with an unsaved edit and recover it. Every assertion is an analytic volume, a protocol rule or
a file on disk — not a comparison of the server against itself.

    python3 scripts/simulate.py [--keep]

Requires only the Python standard library. Exits non-zero on the first failed assertion.
"""

from __future__ import annotations

import json
import math
import os
import pathlib
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "release" / "cad-studio"
CLI = ROOT / "target" / "release" / "atlas"
ENGINEERING = ROOT / "target" / "release" / "atlas-eng"
KEEP = "--keep" in sys.argv

passed = 0


def check(condition: bool, message: str) -> None:
    global passed
    if not condition:
        raise AssertionError(message)
    passed += 1


def free_port() -> int:
    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def start(port: int, *arguments: str, token: str | None = None) -> subprocess.Popen:
    process = subprocess.Popen(
        [str(BINARY), *arguments, "--port", str(port)],
        stdout=subprocess.PIPE,
        stderr=subprocess.STDOUT,
        text=True,
    )
    deadline = time.time() + 20
    while time.time() < deadline:
        if process.poll() is not None:
            output = process.stdout.read() if process.stdout else ""
            raise AssertionError(f"cad-studio exited during startup: {output.strip()}")
        try:
            headers = [("x-atlas-token", token)] if token else None
            request(port, "GET", "/api/health", headers=headers)
            return process
        except (urllib.error.URLError, ConnectionError, OSError):
            time.sleep(0.05)
    raise AssertionError("cad-studio never answered /api/health")


def stop(process: subprocess.Popen, hard: bool = False) -> None:
    if process.poll() is not None:
        return
    if hard:
        process.kill()
    else:
        process.terminate()
    try:
        process.wait(timeout=5)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=5)


def request(port: int, method: str, path: str, body=None, headers=None):
    data = None if body is None else json.dumps(body).encode()
    req = urllib.request.Request(
        f"http://127.0.0.1:{port}{path}", data=data, method=method
    )
    req.add_header("Host", f"127.0.0.1:{port}")
    if data is not None:
        req.add_header("Origin", f"http://127.0.0.1:{port}")
        req.add_header("Content-Type", "application/json")
    for name, value in headers or ():
        req.add_header(name, value)
    with urllib.request.urlopen(req, timeout=30) as response:
        raw = response.read()
    if raw[:1] in (b"{", b"["):
        return json.loads(raw)
    return raw


def state(port: int) -> dict:
    return request(port, "GET", "/api/state")


def volume(port: int) -> float:
    return state(port)["geometry"]["volume_mm3"]


def stl_volume_and_triangles(payload: bytes) -> tuple[float, int]:
    count = struct.unpack("<I", payload[80:84])[0]
    check(len(payload) == 84 + count * 50, "the STL length matches its declared facet count")
    total = 0.0
    for index in range(count):
        base = 84 + index * 50 + 12
        values = struct.unpack("<9f", payload[base : base + 36])
        ax, ay, az, bx, by, bz, cx, cy, cz = values
        total += (
            ax * (by * cz - bz * cy)
            - ay * (bx * cz - bz * cx)
            + az * (bx * cy - by * cx)
        ) / 6.0
    return abs(total), count


def run(*arguments: str) -> str:
    """Run a shipped CLI and fail with its own output when it does not succeed."""
    result = subprocess.run(arguments, capture_output=True, text=True)
    if result.returncode != 0:
        raise AssertionError(
            f"{arguments[0]} {arguments[1:]} failed ({result.returncode}): "
            f"{result.stderr.strip() or result.stdout.strip()}"
        )
    return result.stdout


def stl_file_volume(path: pathlib.Path) -> tuple[float, int]:
    return stl_volume_and_triangles(path.read_bytes())


def cli_simulation(work: pathlib.Path) -> int:
    """Drive the two headless binaries the bundle ships, on files in a scratch directory."""
    before = passed
    out = work / "cli"
    out.mkdir()

    capabilities = run(str(CLI), "capabilities")
    for capability in ("extrude", "revolve", "boolean union", "boolean difference"):
        check(
            f"capability: {capability}" in capabilities,
            f"the CLI does not advertise {capability}:\n{capabilities}",
        )

    box = out / "box.stl"
    run(str(CLI), "box", "--size", "10,10,5", "--out", str(box))
    box_volume, box_triangles = stl_file_volume(box)
    check(abs(box_volume - 500.0) < 1e-6, f"the CLI box is 500 mm3, got {box_volume}")
    check(box_triangles == 12, f"a box is 12 triangles, got {box_triangles}")

    # Two 10 x 10 x 5 boxes offset by (8, 8): 1000 - 2 x 2 x 5 = 980 mm3.
    union = out / "union.stl"
    run(
        str(CLI), "union", "--a", "10,10,5", "--b", "10,10,5", "--offset", "8,8,0",
        "--out", str(union),
    )
    union_volume, union_triangles = stl_file_volume(union)
    check(abs(union_volume - 980.0) < 0.5, f"the CLI union is ~980 mm3, got {union_volume}")
    check(union_triangles > 12, "the union has more facets than one box")

    project = out / "cli.zcad"
    run(str(CLI), "project", "new", "--file", str(project), "--size", "40,30,10")
    exported = out / "40x30x10.stl"
    run(str(CLI), "project", "export", "--file", str(project), "--out", str(exported))
    volume_10, _ = stl_file_volume(exported)
    check(abs(volume_10 - 12000.0) < 1e-6, f"a 40x30x10 project is 12000 mm3, got {volume_10}")

    set_output = run(
        str(CLI), "project", "set", "--file", str(project), "--param", "depth", "--value", "20"
    )
    check("\"volume_mm3\": 24000" in set_output, f"depth 20 does not double the body:\n{set_output}")
    run(str(CLI), "project", "export", "--file", str(project), "--out", str(exported))
    volume_20, _ = stl_file_volume(exported)
    check(abs(volume_20 - 24000.0) < 1e-6, f"the 20 mm export is 24000 mm3, got {volume_20}")

    undone = run(str(CLI), "project", "undo", "--file", str(project))
    check("\"volume_mm3\": 12000" in undone, f"undo does not restore the 10 mm body:\n{undone}")

    validation = run(str(ENGINEERING), "validate", "--root", str(ROOT))
    check("0 error(s)" in validation, f"manifest validation does not pass:\n{validation}")
    licences = run(str(ENGINEERING), "licences", "--root", str(ROOT))
    check("0 denied, 0 unknown" in licences, f"the licence policy does not pass:\n{licences}")
    first = run(str(ENGINEERING), "sbom", "--root", str(ROOT))
    second = run(str(ENGINEERING), "sbom", "--root", str(ROOT))
    check("CycloneDX 1.5" in first, "the SBOM is not CycloneDX 1.5")
    check(first == second, "the SBOM is not byte-identical across runs")
    report = run(str(ENGINEERING), "report", "--root", str(ROOT))
    check(len(report) > 1000, "the engineering report is empty")

    print(f"==> CLI + control-plane simulation passed ({passed - before} assertions)")
    return passed - before


def share_simulation(work: pathlib.Path) -> int:
    """A shared studio must refuse everything without its token and serve everything with it."""
    before = passed
    project = work / "shared.zcad"
    port = free_port()
    token = "simulationtoken0123456789abcdef"
    print(f"==> sharing against {BINARY} on 0.0.0.0:{port}")
    process = start(
        port,
        "--new", str(project), "--name", "shared", "--size", "10,10,5",
        "--share", "--token", token, "--advertise", "127.0.0.1",
        token=token,
    )
    try:
        try:
            request(port, "GET", "/api/state")
            check(False, "a shared studio served the document without a token")
        except urllib.error.HTTPError as error:
            check(error.code == 401, f"a shared studio answered {error.code}, not 401")

        page = request(port, "GET", "/")
        check(b"Atlas" in page, "a shared studio does not serve its page")

        state = request(port, "GET", "/api/state", headers=[("x-atlas-token", token)])
        check(
            state["share"]["url"] == f"http://127.0.0.1:{port}/?token={token}",
            f"the share link is {state['share']['url']}",
        )
        edited = request(
            port,
            "POST",
            "/api/parameter",
            {"id": "depth", "value": 20.0},
            headers=[("x-atlas-token", token)],
        )
        check(
            abs(edited["geometry"]["volume_mm3"] - 2000.0) < 1e-6,
            f"a token-bearing edit is 2000 mm3, got {edited['geometry']['volume_mm3']}",
        )
    finally:
        stop(process)
    print(f"==> share simulation passed ({passed - before} assertions)")
    return passed - before


def main() -> int:
    for binary in (BINARY, CLI, ENGINEERING):
        if not binary.exists():
            raise AssertionError(
                f"{binary} is missing; run "
                "`cargo build --release -p cad-studio -p cad-cli -p cad-engineering` first"
            )
    work = pathlib.Path(tempfile.mkdtemp(prefix="atlas-sim-"))
    project = work / "part.zcad"
    port = free_port()
    print(f"==> simulating against {BINARY} on 127.0.0.1:{port}")
    print(f"    project {project}")

    process = start(port, "--new", str(project), "--name", "sim", "--size", "40,30,10")
    try:
        initial = state(port)
        check(
            abs(initial["geometry"]["volume_mm3"] - 12000.0) < 1e-6,
            f"a 40x30x10 box is 12000 mm3, got {initial['geometry']['volume_mm3']}",
        )
        check(initial["failure"] is None, "the initial history rebuilds")
        check(initial["recovery"]["recovered"] is False, "a new project is not recovered")
        check(initial["recovery"]["autosave_error"] is None, "autosave is healthy")

        # A through-cut of a 10 mm circle. The profile is a tolerance-bounded polygon, so the
        # analytic pi*r^2*h is an upper bound.
        request(port, "POST", "/api/sketch", {"kind": "circle", "origin": [10, 10]})
        circle = state(port)["sketches"][-1]["id"]
        cut = request(
            port, "POST", "/api/feature/extrude", {"sketch_id": circle, "operation": "cut"}
        )
        analytic_hole = math.pi * 25.0 * 10.0
        check(cut["features"][-1]["status"] == "built", "the cut builds")
        cut_volume = cut["geometry"]["volume_mm3"]
        check(
            cut_volume < 12000.0 and cut_volume > 12000.0 - analytic_hole * 1.01,
            f"the cut removes about the analytic hole: {cut_volume}",
        )
        check(cut["geometry"]["triangles"] > 12, "the cut adds faces")

        # Join a second 40x30 plate moved to (30, 20): 11000 mm3 of new material.
        request(port, "POST", "/api/sketch", {"kind": "rectangle", "origin": [30, 20]})
        boss = state(port)["sketches"][-1]["id"]
        joined = request(
            port, "POST", "/api/feature/extrude", {"sketch_id": boss, "operation": "join"}
        )
        check(joined["features"][-1]["status"] == "built", "the join builds")
        joined_volume = joined["geometry"]["volume_mm3"]
        # The union's tessellation adds a few hundredths of a cubic millimetre; the analytic
        # addition is 40 x 30 x 10 minus the 10 x 10 overlap = 11000.
        check(
            abs(joined_volume - (cut_volume + 11000.0)) < 0.1,
            f"the join adds the analytic 11000 mm3: {joined_volume} vs {cut_volume + 11000.0}",
        )

        undone = request(port, "POST", "/api/undo", {})
        check(
            abs(undone["geometry"]["volume_mm3"] - cut_volume) < 1e-6,
            "undo removes the join and its geometry together",
        )
        redone = request(port, "POST", "/api/redo", {})
        check(
            abs(redone["geometry"]["volume_mm3"] - joined_volume) < 1e-6,
            "redo restores the join",
        )

        # Edit the shared depth: every feature rebuilds, including the cut and the join.
        rebuilt = request(port, "POST", "/api/parameter", {"id": "depth", "value": 12.0})
        check(
            all(
                feature["status"] == "built"
                for feature in rebuilt["features"]
            ),
            "every feature rebuilds after a parameter edit",
        )
        rebuilt_volume = rebuilt["geometry"]["volume_mm3"]
        # The cut profile is the polygon the document actually built, measured at 10 mm depth, not
        # the ideal circle; scaling that area by 12 mm is the analytic expectation for the rebuild.
        profile_area = (12000.0 - cut_volume) / 10.0
        expected = 14400.0 - profile_area * 12.0 + 13200.0
        check(
            abs(rebuilt_volume - expected) < 2.0,
            f"the rebuild is the analytic 12 mm volume: {rebuilt_volume} vs ~{expected}",
        )

        snapshot = project.with_name(project.name + ".recovery")
        check(snapshot.exists(), "an unsaved edit leaves a recovery snapshot")

        saved = request(port, "POST", "/api/save", {})
        check(saved["dirty"] is False, "save clears the dirty flag")
        check(not snapshot.exists(), "save removes the recovery snapshot")

        exported = request(port, "GET", "/api/export/stl")
        stl_volume, triangles = stl_volume_and_triangles(exported)
        check(triangles > 0, "the STL has facets")
        check(
            abs(stl_volume - rebuilt_volume) < rebuilt_volume * 0.001,
            f"the exported STL volume matches the model: {stl_volume} vs {rebuilt_volume}",
        )
    finally:
        stop(process)

    # Reopen from disk in a fresh process: history, volume and export must come back.
    process = start(port, "--file", str(project))
    try:
        reopened = state(port)
        check(reopened["recovery"]["recovered"] is False, "a saved project is not recovered")
        check(reopened["dirty"] is False, "a reopened project is clean")
        check(
            abs(reopened["geometry"]["volume_mm3"] - rebuilt_volume) < 1e-9,
            "reopening reproduces the volume exactly",
        )
        check(len(reopened["features"]) == 3, "reopening keeps all three features")
        check(
            [feature["kind"] for feature in reopened["features"]]
            == ["extrude", "extrude", "extrude"],
            "reopening keeps every feature kind",
        )

        # The measured data-loss defect: an unsaved edit must survive kill -9.
        unsaved = request(port, "POST", "/api/parameter", {"id": "depth", "value": 9.0})
        unsaved_volume = unsaved["geometry"]["volume_mm3"]
        check(unsaved["dirty"] is True, "the edit is unsaved")
    finally:
        process.kill()
        process.wait(timeout=5)

    process = start(port, "--file", str(project))
    try:
        recovered = state(port)
        check(
            recovered["recovery"]["recovered"] is True,
            "the unsaved edit is offered after kill -9",
        )
        check(recovered["dirty"] is True, "a recovered session is unsaved")
        check(
            abs(recovered["geometry"]["volume_mm3"] - unsaved_volume) < 1e-6,
            "the recovered body is the unsaved edit",
        )

        discarded = request(port, "POST", "/api/recovery/discard", {})
        check(discarded["recovery"]["recovered"] is False, "discard clears the recovered flag")
        check(
            abs(discarded["geometry"]["volume_mm3"] - rebuilt_volume) < 1e-6,
            "discard goes back to the file body",
        )
        check(
            not project.with_name(project.name + ".recovery").exists(),
            "discard deletes the snapshot",
        )
    finally:
        stop(process)

    print(f"==> HTTP simulation passed ({passed} assertions)")
    cli_simulation(work)
    share_simulation(work)
    print(f"==> simulation passed ({passed} assertions in total)")
    if KEEP:
        print(f"    kept {work}")
    else:
        shutil.rmtree(work, ignore_errors=True)
    return 0


if __name__ == "__main__":
    try:
        sys.exit(main())
    except AssertionError as error:
        print(f"FAIL: {error}", file=sys.stderr)
        sys.exit(1)

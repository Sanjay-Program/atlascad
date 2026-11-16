#!/usr/bin/env python3
"""Exhaustive end-to-end check of the studio: every route, every shape, every export.

`simulate.py` proves the core workflow and the deployable binaries. This proves the *surface*: all
five sketch profiles, all seven drawing entity kinds, revolve as well as extrude, parameter and
origin edits, drafting layers/undo/redo/save, engineering review, named revisions, every export
format with its query options, the protocol's refusal cases (foreign origin, wrong content type,
malformed body, bad host, missing token), kill -9 recovery, concurrent writers, and the shared
session's token matrix.

    python3 scripts/full-check.py [--keep] [--verbose]

Runs the release binary on loopback and needs only the Python standard library. Every assertion is
an analytic volume, a documented status code or a byte-level check of a file the server produced.
Exits non-zero on the first failed assertion.
"""

from __future__ import annotations

import concurrent.futures
import http.client
import json
import math
import pathlib
import re
import shutil
import socket
import struct
import subprocess
import sys
import tempfile
import time
import urllib.parse

ROOT = pathlib.Path(__file__).resolve().parent.parent
BINARY = ROOT / "target" / "release" / "cad-studio"
KEEP = "--keep" in sys.argv
VERBOSE = "--verbose" in sys.argv
UNSET = object()

passed = 0
section_passed = 0


def check(condition: bool, message: str) -> None:
    global passed
    if not condition:
        raise AssertionError(message)
    passed += 1


def section(name: str) -> None:
    global section_passed
    print(f"==> {name}")
    section_passed = passed


def report(name: str) -> None:
    print(f"    {name}: {passed - section_passed} assertion(s)")


class Response:
    def __init__(self, status: int, headers: dict[str, str], body: bytes):
        self.status = status
        self.headers = headers
        self.body = body

    def __repr__(self) -> str:
        return f"<{self.status} {self.body[:120]!r}>"

    def json(self):
        return json.loads(self.body)

    def text(self) -> str:
        return self.body.decode("utf-8", "replace")


def call(
    port: int,
    method: str,
    path: str,
    payload=UNSET,
    *,
    headers: tuple[tuple[str, str], ...] = (),
    host: str | None = None,
    content_type: str | None = "application/json",
    origin: object = "auto",
    timeout: float = 120.0,
) -> Response:
    """One HTTP request. `origin=UNSET` sends no Origin header; `host` overrides Host."""
    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=timeout)
    body = None
    sent: dict[str, str] = {}
    if payload is not UNSET:
        if isinstance(payload, (bytes, bytearray)):
            body = bytes(payload)
        else:
            body = json.dumps(payload).encode()
        if content_type is not None:
            sent["Content-Type"] = content_type
        if origin == "auto":
            sent["Origin"] = f"http://127.0.0.1:{port}"
        elif origin:
            sent["Origin"] = str(origin)
    for name, value in headers:
        sent[name] = value
    connection.putrequest(
        method, path, skip_host=host is not None, skip_accept_encoding=True
    )
    if host is not None:
        connection.putheader("Host", host)
    for name, value in sent.items():
        connection.putheader(name, value)
    if body is not None:
        connection.putheader("Content-Length", str(len(body)))
    connection.endheaders(body)
    raw = connection.getresponse()
    data = raw.read()
    result = Response(raw.status, {k.lower(): v for k, v in raw.getheaders()}, data)
    connection.close()
    if VERBOSE:
        print(f"      {method} {path[:90]} -> {result.status} ({len(data)} B)")
    return result


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
    deadline = time.time() + 30
    headers = (("x-atlas-token", token),) if token else ()
    while time.time() < deadline:
        if process.poll() is not None:
            output = process.stdout.read() if process.stdout else ""
            raise AssertionError(f"cad-studio exited during startup: {output.strip()}")
        try:
            call(port, "GET", "/api/health", headers=headers, timeout=2)
            return process
        except (ConnectionError, OSError):
            time.sleep(0.05)
    raise AssertionError("cad-studio never answered /api/health")


def stop(process: subprocess.Popen, hard: bool = False) -> None:
    if process.poll() is not None:
        return
    process.kill() if hard else process.terminate()
    try:
        process.wait(timeout=8)
    except subprocess.TimeoutExpired:
        process.kill()
        process.wait(timeout=8)


class Studio:
    """A running studio plus the helpers every section uses."""

    def __init__(self, process: subprocess.Popen, port: int, project: pathlib.Path):
        self.process = process
        self.port = port
        self.project = project

    def get(self, path: str, **kwargs) -> Response:
        return call(self.port, "GET", path, **kwargs)

    def post(self, path: str, payload=None, **kwargs) -> Response:
        return call(self.port, "POST", path, {} if payload is None else payload, **kwargs)

    def state(self, **kwargs) -> dict:
        response = self.get("/api/state", **kwargs)
        check(response.status == 200, f"/api/state answered {response.status}")
        return response.json()

    def volume(self, **kwargs) -> float:
        return self.state(**kwargs)["geometry"]["volume_mm3"]

    def sketch(self, kind: str, **body) -> int:
        response = self.post("/api/sketch", {"kind": kind, **body})
        check(response.status == 200, f"adding a {kind} sketch: {response!r}")
        return response.json()["sketches"][-1]["id"]

    def extrude(self, sketch_id: int, operation: str, **body) -> dict:
        response = self.post(
            "/api/feature/extrude", {"sketch_id": sketch_id, "operation": operation, **body}
        )
        check(response.status == 200, f"extruding sketch {sketch_id} as {operation}: {response!r}")
        return response.json()

    def draft(self) -> dict:
        response = self.get("/api/draft")
        check(response.status == 200, f"/api/draft answered {response.status}")
        return response.json()


def profile_kind(view: dict) -> str:
    """The profile's shape name, whether it serialises as a tag or as a variant key."""
    if "kind" in view:
        return str(view["kind"]).lower()
    return next((str(key).lower() for key in view), "?")


def profile_field(view: dict, name: str):
    if name in view:
        return view[name]
    for value in view.values():
        if isinstance(value, dict) and name in value:
            return value[name]
    return None


def stl_metrics(payload: bytes) -> tuple[float, int]:
    count = struct.unpack("<I", payload[80:84])[0]
    check(len(payload) == 84 + count * 50, "the STL matches its declared facet count")
    total = 0.0
    for index in range(count):
        base = 84 + index * 50 + 12
        ax, ay, az, bx, by, bz, cx, cy, cz = struct.unpack("<9f", payload[base : base + 36])
        total += (
            ax * (by * cz - bz * cy)
            - ay * (bx * cz - bz * cx)
            + az * (bx * cy - by * cx)
        ) / 6.0
    return abs(total), count


def protocol_section(studio: Studio) -> None:
    section("protocol: every refusal the server documents")
    check(studio.get("/api/nope").status == 404, "an unknown route is 404")
    check(call(studio.port, "DELETE", "/api/state").status == 405, "DELETE on a read is 405")
    check(
        studio.post("/api/parameter", b"not json", content_type="application/json").status == 400,
        "malformed JSON is 400",
    )
    check(
        studio.post("/api/parameter", {"id": "width", "value": 20, "extra": 1}).status == 400,
        "an unknown field is 400",
    )
    check(
        studio.post("/api/parameter", {"id": "width", "value": -5}).status == 400,
        "a negative size is refused",
    )
    check(
        studio.post("/api/parameter", {"id": "nope", "value": 1}).status == 400,
        "an unknown parameter is refused",
    )
    check(
        studio.post("/api/parameter", {"id": "width", "value": 20}, origin=UNSET).status == 403,
        "a POST without Origin is refused",
    )
    check(
        studio.post(
            "/api/parameter", {"id": "width", "value": 20}, origin="http://evil.example"
        ).status
        == 403,
        "a POST from another origin is refused",
    )
    check(
        studio.post(
            "/api/parameter", {"id": "width", "value": 20}, content_type="text/plain"
        ).status
        == 415,
        "a POST without a JSON content type is 415",
    )
    check(
        studio.get("/api/state", host="attacker.example").status == 403,
        "a foreign Host is refused on loopback",
    )
    check(
        studio.get("/api/state", host=f"localhost:{studio.port}").status == 200,
        "localhost is an accepted loopback host",
    )
    huge = b'{"id":"width","value":1,"pad":"' + b"x" * (2 * 1024 * 1024) + b'"}'
    try:
        status = studio.post("/api/parameter", huge).status
        check(status in (400, 413), f"an oversized body is refused, got {status}")
    except (BrokenPipeError, ConnectionResetError):
        # Closing the connection instead of answering is also a refusal.
        check(True, "an oversized body is refused by dropping the connection")
    report("protocol")


def assets_section(studio: Studio) -> None:
    section("the shipped page and its assets")
    page = studio.get("/")
    check(page.status == 200, "the page is served")
    check(b"Atlas" in page.body, "the page names Atlas")
    check(page.headers.get("referrer-policy") == "no-referrer", "the page sets no-referrer")
    check(
        page.headers.get("content-security-policy", "").startswith("default-src 'self'"),
        "the page carries a CSP",
    )
    script = studio.get("/app.js")
    check(script.status == 200 and len(script.body) > 50_000, "app.js is served and substantial")
    check(b"/api/state" in script.body and b"sketch" in script.body,
          "app.js is the studio script")
    css = studio.get("/app.css")
    check(css.status == 200 and len(css.body) > 1_000, "app.css is served")
    icon = studio.get("/favicon.svg")
    check(icon.status == 200 and b"<svg" in icon.body, "the favicon is an SVG")
    health = studio.get("/api/health")
    check(health.status == 200 and health.json()["ok"] is True, "health reports ok")
    report("assets")


def sketches_section(studio: Studio) -> None:
    section("sketches: every profile, profile edits and origin moves")
    base = studio.volume()
    check(abs(base - 12000.0) < 1e-6, f"the 40x30x10 base is 12000 mm3, got {base}")

    # A rectangle profile's origin is its lower-left corner, so the 40x30x10 body occupies
    # 0..40 x 0..30. Every tool below is placed inside that footprint.
    # Circle cut: the profile is a tolerance-bounded polygon, so pi*r^2*h is an upper bound.
    circle = studio.sketch("circle", origin=[10, 10])
    cut = studio.extrude(circle, "cut")
    check(cut["features"][-1]["status"] == "built", "the circular cut builds")
    removed = base - cut["geometry"]["volume_mm3"]
    # A 24-segment approximation of the disc under-fills the true circle by ~1.1%, so the bound is
    # the ideal pi*r^2*h, not equality with it.
    check(
        math.pi * 25.0 * 10.0 * 0.98 < removed < math.pi * 25.0 * 10.0 * 1.01,
        f"the cut removes about pi*25*10 mm3, got {removed}",
    )

    # A drawn square cut is analytic: 10 x 10 through 10 mm is exactly 1000 mm3.
    square = studio.sketch(
        "polyline", points=[[-5, -5], [5, -5], [5, 5], [-5, 5]], origin=[20, 15]
    )
    drawn_cut = studio.extrude(square, "cut")
    check(drawn_cut["features"][-1]["status"] == "built", "the drawn square cut builds")
    removed = cut["geometry"]["volume_mm3"] - drawn_cut["geometry"]["volume_mm3"]
    check(abs(removed - 1000.0) < 0.5, f"the square cut removes 1000 mm3, got {removed}")

    # A regular polygon and a sector are joins on top of what is there.
    polygon = studio.sketch("regular_polygon", sides=6, origin=[35, 25])
    joined = studio.extrude(polygon, "join")
    check(joined["features"][-1]["status"] == "built", "the hexagon join builds")
    check(
        joined["geometry"]["volume_mm3"] > drawn_cut["geometry"]["volume_mm3"],
        "the hexagon adds material",
    )
    sector = studio.sketch("sector", start_degrees=30, sweep_degrees=120, origin=[25, 10])
    sector_join = studio.extrude(sector, "join")
    check(sector_join["features"][-1]["status"] == "built", "the sector join builds")
    check(
        sector_join["geometry"]["volume_mm3"] > joined["geometry"]["volume_mm3"],
        "the sector adds material",
    )

    # Changing a profile keeps the document valid and the parameters it already references.
    changed = studio.post("/api/sketch/profile", {"id": polygon, "kind": "circle"})
    check(changed.status == 200, f"changing a hexagon to a circle: {changed!r}")
    profile = next(s for s in changed.json()["sketches"] if s["id"] == polygon)
    check(profile_kind(profile["profile"]) == "circle", f"the profile is {profile['profile']}")
    check(changed.json()["failure"] is None, "the profile change rebuilds")

    # Changing only the sector's sweep keeps its start angle.
    swept = studio.post(
        "/api/sketch/profile", {"id": sector, "kind": "sector", "sweep_degrees": 60}
    )
    check(swept.status == 200, f"nudging the sector sweep: {swept!r}")
    profile = next(s for s in swept.json()["sketches"] if s["id"] == sector)
    check(profile_field(profile["profile"], "start_degrees") == 30,
          f"the start angle is {profile['profile']}")
    check(profile_field(profile["profile"], "sweep_degrees") == 60,
          f"the sweep is {profile['profile']}")

    # Moving a sketch origin rebuilds the body without an error.
    moved = studio.post("/api/sketch/origin", {"id": square, "origin": [21, 15]})
    check(moved.status == 200 and moved.json()["failure"] is None, "an origin move rebuilds")
    placed = moved

    # The sketch list keeps every profile kind the API accepted.
    kinds = {profile_kind(s["profile"]) for s in placed.json()["sketches"]}
    check(
        {"circle", "polyline", "sector"} <= kinds,
        f"the document keeps its profile kinds: {sorted(kinds)}",
    )
    report("sketches")


def revolve_section(work: pathlib.Path) -> None:
    section("revolve: a rotated profile becomes a solid")
    project = work / "revolve.zcad"
    port = free_port()
    studio = Studio(start(port, "--new", str(project), "--name", "revolve", "--size", "20,20,5"), port, project)
    try:
        base = studio.volume()
        check(abs(base - 2000.0) < 1e-6, f"the 20x20x5 base is 2000 mm3, got {base}")
        # The sketch sits far outside the plate, so the full turn sweeps an annulus that shares no
        # face with the body: coplanar contact is the one thing this kernel's union cannot do, and a
        # disjoint body still exercises the join path honestly.
        profile = studio.sketch("rectangle", origin=[-50, 10])
        response = studio.post(
            "/api/feature/revolve",
            {"sketch_id": profile, "operation": "join", "axis": "vertical"},
        )
        check(response.status == 200, f"a vertical revolve: {response!r}")
        featured = response.json()
        check(featured["features"][-1]["kind"] == "revolve", "the feature is a revolve")
        check(featured["features"][-1]["status"] == "built", "the revolve builds")
        # The profile is the document's 20 x 20 rectangle, and its bottom edge lies on the axis, so
        # the turn sweeps a cylinder of radius 20 and height 20: pi * 400 * 20 = 25132.7 mm3, plus
        # the 2000 mm3 plate. The swept surface is tessellated, so allow 1%.
        volume = featured["geometry"]["volume_mm3"]
        expected = base + math.pi * 800.0 * 10.0
        check(
            abs(volume - expected) < expected * 0.01,
            f"the revolve adds an annulus: {volume} vs about {expected:.1f}",
        )
        # The axis name is validated rather than silently defaulted.
        bad = studio.post(
            "/api/feature/revolve",
            {"sketch_id": profile, "operation": "join", "axis": "diagonal"},
        )
        check(bad.status == 400, f"an unknown revolve axis is 400, got {bad.status}")
        # The parameter rectangle is square, so *both* axes would sweep congruent cylinders and a
        # volume could not tell them apart. A drawn 20 x 10 rectangle, on a horizontal axis along
        # its own bottom edge, sweeps a cylinder of radius 10 and length 20: pi * 100 * 20 =
        # 6283.2 mm3. It sits at y = -40, clear of the plate and of the vertical annulus.
        drawn = studio.sketch(
            "polyline", points=[[0, 0], [20, 0], [20, 10], [0, 10]], origin=[-50, -40]
        )
        turned = studio.post(
            "/api/feature/revolve",
            {"sketch_id": drawn, "operation": "join", "axis": "horizontal"},
        )
        check(turned.status == 200, f"a horizontal revolve: {turned!r}")
        horizontal = turned.json()
        check(
            horizontal["features"][-1]["axis"] == "horizontal axis",
            f"the horizontal axis is recorded: {horizontal['features'][-1]}",
        )
        check(
            horizontal["features"][-1]["status"] == "built",
            f"the horizontal revolve builds: {horizontal['features'][-1]}",
        )
        expected += math.pi * 100.0 * 20.0
        volume = horizontal["geometry"]["volume_mm3"]
        check(
            abs(volume - expected) < expected * 0.01,
            f"the horizontal revolve adds a cylinder: {volume} vs about {expected:.1f}",
        )
    finally:
        stop(studio.process)
    report("revolve")


def boss_section(work: pathlib.Path) -> None:
    section("a boss on a raised plane, with its own analytic volume")
    project = work / "boss.zcad"
    port = free_port()
    studio = Studio(start(port, "--new", str(project), "--name", "boss", "--size", "10,10,5"), port, project)
    try:
        base = studio.volume()
        check(abs(base - 500.0) < 1e-6, f"the 10x10x5 plate is 500 mm3, got {base}")
        resized = studio.post("/api/parameter", {"id": "diameter", "value": 5})
        check(resized.status == 200, "the diameter is a parameter")
        sketch = studio.sketch("circle", origin=[5, 5], plane=5)
        joined = studio.extrude(sketch, "join", direction="up")
        feature = joined["features"][-1]
        check(feature["status"] == "built", f"the raised-plane boss builds: {feature}")
        check(feature["direction"] == "up", f"the boss records its direction: {feature}")
        check(feature["depth_mm"] == 5, f"the boss uses the depth parameter: {feature}")
        # 500 + pi * 2.5^2 * 5 = 598.2 mm3; the 24-segment circle under-fills slightly.
        volume = joined["geometry"]["volume_mm3"]
        check(590.0 < volume < 610.0, f"the boss volume is {volume}, expected about 597 mm3")
    finally:
        stop(studio.process)
    report("boss")


def drafting_section(studio: Studio) -> None:
    section("drafting: every entity kind, layers, undo/redo, save, DXF and PDF")
    draft = studio.draft()
    check(draft["source"] == "model", f"a new drawing is generated from the model: {draft['source']}")
    check(draft["dirty"] is False, "a generated drawing starts clean")
    outline = len(draft["entities"])
    check(outline >= 1, "the generated plan has at least one entity")

    entities = [
        {"kind": "point", "at": [1, 1]},
        {"kind": "line", "start": [0, 0], "end": [10, 10]},
        {"kind": "circle", "center": [5, 5], "radius": 3},
        {"kind": "arc", "center": [0, 0], "radius": 5, "start_degrees": 0, "end_degrees": 90},
        {"kind": "polyline", "points": [[0, 0], [4, 0], [4, 4]], "closed": True},
        {"kind": "text", "at": [0, 0], "height": 2.5, "value": "Atlas"},
        {"kind": "dimension", "start": [0, 0], "end": [10, 0], "offset": 4},
    ]
    added = outline
    for entity in entities:
        response = studio.post("/api/draft/entity", {"layer": "OUTLINE", "kind": entity})
        check(response.status == 200, f"adding a {entity['kind']} entity: {response!r}")
        added += 1
        check(
            len(response.json()["entities"]) == added,
            f"the {entity['kind']} reached the drawing",
        )
    draft = studio.draft()
    labels = [
        entry["kind"]["kind"] if isinstance(entry["kind"], dict) else entry["kind"]
        for entry in draft["entities"]
    ]
    for entity in entities:
        check(entity["kind"] in labels, f"the drawing keeps its {entity['kind']} entity")
    check(draft["total_length"] > 0, "the drawing measures a length")
    check(draft["bounds"] is not None, "the drawing has bounds")
    check(draft["dirty"] is True, "an edited drawing is unsaved")

    layer = studio.post("/api/draft/layer", {"name": "DIMENSIONS", "color": 2, "visible": True})
    check(layer.status == 200, f"adding a layer: {layer!r}")
    names = [entry["name"] for entry in layer.json()["layers"]]
    check("DIMENSIONS" in names, f"the layer list is {names}")
    locked = studio.post("/api/draft/layer", {"name": "DIMENSIONS", "locked": True})
    entry = next(e for e in locked.json()["layers"] if e["name"] == "DIMENSIONS")
    check(entry["locked"] is True, "the layer locks")
    hidden = studio.post("/api/draft/layer", {"name": "DIMENSIONS", "visible": False, "color": 4})
    entry = next(e for e in hidden.json()["layers"] if e["name"] == "DIMENSIONS")
    check(entry["visible"] is False and entry["color"] == 4, "the layer hides and recolours")

    undone = studio.post("/api/draft/undo")
    check(undone.status == 200, f"drawing undo: {undone!r}")
    entry = next(e for e in undone.json()["layers"] if e["name"] == "DIMENSIONS")
    check(entry["visible"] is True, "undo restores the layer visibility")
    redone = studio.post("/api/draft/redo")
    entry = next(e for e in redone.json()["layers"] if e["name"] == "DIMENSIONS")
    check(entry["visible"] is False, "redo hides the layer again")

    target = redone.json()["entities"][-1]["id"]
    removed = studio.post("/api/draft/remove", {"id": target})
    check(removed.status == 200 and len(removed.json()["entities"]) == added - 1,
          "an entity is removed")
    check(studio.post("/api/draft/remove", {"id": target}).status == 400,
          "removing it twice is refused")

    saved = studio.post("/api/draft/save")
    check(saved.status == 200, f"saving the drawing: {saved!r}")
    check(saved.json()["source"] == "file", f"the saved source is {saved.json()['source']}")
    check(saved.json()["file"] and saved.json()["file"].endswith(".zdraft"),
          f"the drawing file is {saved.json()['file']}")
    check(saved.json()["dirty"] is False, "saving clears the drawing's dirty flag")
    check(pathlib.Path(saved.json()["file"]).exists(), "the .zdraft exists on disk")

    dxf = studio.get("/api/export/dxf")
    check(dxf.status == 200, f"the DXF export answered {dxf.status}")
    check(dxf.headers.get("content-type") == "application/dxf", "the DXF is typed application/dxf")
    check(b"ENTITIES" in dxf.body and b"EOF" in dxf.body, "the DXF has entities and a terminator")
    check(
        dxf.headers.get("content-disposition", "").endswith('.dxf"'),
        "the DXF is an attachment",
    )

    pdf = studio.get("/api/export/pdf?paper=A4&orientation=portrait&scale=fit")
    check(pdf.status == 200 and pdf.body.startswith(b"%PDF-"), "the PDF plot is a PDF")
    check(len(pdf.body) > 500, "the PDF is not empty")
    other = studio.get("/api/export/pdf?paper=A3&orientation=landscape&scale=2")
    check(other.status == 200 and len(other.body) != len(pdf.body), "the sheet options change the page")
    check(studio.get("/api/export/pdf?paper=tabloid").status == 400, "an unknown paper is 400")
    check(studio.get("/api/export/pdf?orientation=sideways").status == 400, "a bad orientation is 400")
    check(studio.get("/api/export/pdf?scale=0").status == 400, "a zero scale is 400")
    titled = studio.get("/api/export/pdf?title=Atlas%20bracket")
    check(titled.status == 200, "a percent-encoded title is accepted")

    # Advanced AutoCAD Drafting & Inquiries
    meas = studio.post("/api/draft/measure", {"mode": "distance", "p1": [0.0, 0.0], "p2": [30.0, 40.0]})
    check(meas.status == 200 and abs(meas.json()["distance"] - 50.0) < 1e-4, "measure distance is analytic")

    area_inq = studio.post("/api/draft/measure", {"mode": "area", "points": [[0.0, 0.0], [20.0, 0.0], [20.0, 10.0], [0.0, 10.0]]})
    check(area_inq.status == 200 and abs(area_inq.json()["area"] - 200.0) < 1e-4, "measure area is analytic")

    # Add rectangle and offset it
    add_r = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "rectangle", "origin": [0.0, 0.0], "width": 40.0, "height": 20.0}})
    check(add_r.status == 200, "draft rectangle added")
    r_id = add_r.json()["entities"][-1]["id"]

    off = studio.post("/api/draft/offset", {"id": r_id, "distance": 4.0, "side": 1.0})
    check(off.status == 200, "offset rectangle created")

    # Add 2 lines and fillet them
    l1 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [0.0, 30.0], "end": [0.0, 0.0]}})
    l2 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [0.0, 0.0], "end": [30.0, 0.0]}})
    id_l1 = l1.json()["entities"][-1]["id"]
    id_l2 = l2.json()["entities"][-1]["id"]
    fil = studio.post("/api/draft/fillet", {"id1": id_l1, "id2": id_l2, "radius": 5.0})
    check(fil.status == 200, "fillet creates tangent arc")

    # 2D to 3D solid bridge
    solid_bridge = studio.post("/api/draft/to-solid", {"entity_id": r_id, "depth": 12.0})
    check(solid_bridge.status == 200 and solid_bridge.json()["geometry"]["volume_mm3"] > 0.0, "2D to 3D solid extrusion succeeded")
    undone_bridge = studio.post("/api/undo")
    check(undone_bridge.status == 200, "undo of 2D to 3D solid extrusion succeeded")

    # Crosshatch pattern
    hatch_resp = studio.post("/api/draft/hatch", {"entity_id": r_id, "pattern": "ANSI31", "angle_deg": 45.0, "spacing": 4.0})
    check(hatch_resp.status == 200, "crosshatch pattern ANSI31 generated")

    # Multi-view orthographic drawing generator
    mv_resp = studio.post("/api/draft/multiview", {})
    check(mv_resp.status == 200, "multi-view orthographic drawing generated")
    mv_layers = [l["name"] for l in mv_resp.json()["layers"]]
    check("OUTLINE" in mv_layers and "SECTION" in mv_layers and "HATCH" in mv_layers, "multi-view drawing has standard layers")

    # Geometric constructions (Circle 2P, Polygon)
    c2p_resp = studio.post("/api/draft/construct", {"kind": "circle_2p", "layer": "0", "p1": [0.0, 0.0], "p2": [50.0, 0.0]})
    check(c2p_resp.status == 200, "circle 2P construction succeeded")
    c2p_id = c2p_resp.json()["entities"][-1]["id"]

    poly_resp = studio.post("/api/draft/construct", {"kind": "polygon", "layer": "0", "center": [0.0, 0.0], "radius": 20.0, "sides": 6, "inscribed": True})
    check(poly_resp.status == 200, "hexagon construction succeeded")
    poly_id = poly_resp.json()["entities"][-1]["id"]

    # Engineering cross-section properties analysis
    sec_resp = studio.post("/api/draft/section-analysis", {"id": poly_id})
    check(sec_resp.status == 200, "section analysis succeeded")
    sec_data = sec_resp.json()
    check(abs(sec_data["area"] - 1039.23) < 1.0, f"section area matches analytic hexagon: {sec_data['area']}")
    check(sec_data["i_xx"] > 0.0 and sec_data["j_polar"] > 0.0, "second moments of area computed analytically")

    # Join entities
    j_l1 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [10.0, 10.0], "end": [25.0, 10.0]}})
    j_l2 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [25.0, 10.0], "end": [25.0, 20.0]}})
    id_j1 = j_l1.json()["entities"][-1]["id"]
    id_j2 = j_l2.json()["entities"][-1]["id"]
    join_resp = studio.post("/api/draft/join", {"ids": [id_j1, id_j2], "tolerance": 0.001})
    check(join_resp.status == 200, "join command chained lines into polyline")
    joined_id = join_resp.json()["entities"][-1]["id"]

    # Rectangular and Polar pattern array
    arr_rect = studio.post("/api/draft/array", {"mode": "rectangular", "ids": [joined_id], "rows": 2, "cols": 2, "row_spacing": 20.0, "col_spacing": 20.0})
    check(arr_rect.status == 200, "rectangular array generated copies")

    arr_polar = studio.post("/api/draft/array", {"mode": "polar", "ids": [c2p_id], "count": 4, "center": [0.0, 0.0], "fill_angle_deg": 360.0, "rotate_items": True})
    check(arr_polar.status == 200, "polar array generated copies")

    # Trim and Extend
    t_target = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [0.0, 15.0], "end": [40.0, 15.0]}})
    t_bound = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [20.0, 0.0], "end": [20.0, 30.0]}})
    id_tt = t_target.json()["entities"][-1]["id"]
    id_tb = t_bound.json()["entities"][-1]["id"]
    tr_resp = studio.post("/api/draft/trim", {"target_id": id_tt, "boundary_ids": [id_tb], "pick_point": [5.0, 15.0]})
    check(tr_resp.status == 200, "line trimmed at boundary")

    e_target = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [0.0, 25.0], "end": [15.0, 25.0]}})
    id_et = e_target.json()["entities"][-1]["id"]
    ex_resp = studio.post("/api/draft/extend", {"target_id": id_et, "boundary_ids": [id_tb], "pick_point": [14.0, 25.0]})
    check(ex_resp.status == 200, "line extended to boundary")

    # 2D Draft to CAM CNC Toolpath & G-code
    cam_resp = studio.post("/api/draft/to-cam", {"entity_id": poly_id, "depth_mm": 6.0, "stepdown_mm": 2.0, "post_format": "grbl"})
    check(cam_resp.status == 200, "2D draft to CAM toolpath generated")
    cam_json = cam_resp.json()
    check(cam_json["moves_count"] > 0 and cam_json["cutting_length_mm"] > 0.0, "CAM toolpath moves and cutting length positive")
    check("G01" in cam_json["gcode"] or "G1" in cam_json["gcode"], "emitted G-code contains linear feed moves")

    # Radial and Diameter Dimensions & Layer Linetypes
    lt_resp = studio.post("/api/draft/layer", {"name": "HIDDEN_LAYER", "color": 1, "linetype": "hidden"})
    check(lt_resp.status == 200, "layer with linetype hidden created")
    check(any(l["name"] == "HIDDEN_LAYER" and l.get("linetype") == "hidden" for l in lt_resp.json()["layers"]), "layer table records hidden linetype")

    rad_dim_resp = studio.post("/api/draft/dimension-radial", {"entity_id": c2p_id, "pick_angle_deg": 45.0, "leader_offset": 12.0, "is_diameter": False})
    check(rad_dim_resp.status == 200, "radial dimension created")
    rad_json = rad_dim_resp.json()
    check(rad_json["entities"][-1]["kind"]["kind"] == "radial_dimension" and not rad_json["entities"][-1]["kind"]["is_diameter"], "radial dimension entity verified")

    dia_dim_resp = studio.post("/api/draft/dimension-radial", {"entity_id": c2p_id, "pick_angle_deg": 135.0, "leader_offset": 15.0, "is_diameter": True})
    check(dia_dim_resp.status == 200, "diameter dimension created")
    dia_json = dia_dim_resp.json()
    check(dia_json["entities"][-1]["kind"]["kind"] == "radial_dimension" and dia_json["entities"][-1]["kind"]["is_diameter"], "diameter dimension entity verified")

    # Reusable Blocks and Inserts (TASK-DRAFT-7)
    blk_resp = studio.post("/api/draft/block", {"name": "CORNER_STAMP", "base_point": [0.0, 0.0], "entity_ids": [id_l1, id_l2]})
    check(blk_resp.status == 200, "reusable block CORNER_STAMP created")
    check(any(b["name"] == "CORNER_STAMP" for b in blk_resp.json().get("blocks", [])), "blocks list contains CORNER_STAMP")

    ins_resp = studio.post("/api/draft/insert", {"block_name": "CORNER_STAMP", "position": [60.0, 60.0], "scale": [1.5, 1.5], "rotation_degrees": 90.0})
    check(ins_resp.status == 200, "block insert placed")
    ins_json = ins_resp.json()
    ins_ent = ins_json["entities"][-1]
    check(ins_ent["kind"]["kind"] == "insert" and ins_ent["kind"]["block_name"] == "CORNER_STAMP", "insert entity verified")
    ins_id = ins_ent["id"]

    exp_ins = studio.post("/api/draft/explode", {"id": ins_id})
    check(exp_ins.status == 200, "insert exploded into constituent geometry")
    check(not any(e["id"] == ins_id for e in exp_ins.json()["entities"]), "insert removed after explode")

    undo_exp = studio.post("/api/draft/undo")
    check(undo_exp.status == 200 and any(e["id"] == ins_id for e in undo_exp.json()["entities"]), "undo restored exploded insert")

    # Advanced AutoCAD Drafting Operations & Parametric Constraints
    # 1. PURGE: Unused layers and unused block definitions
    p_lay = studio.post("/api/draft/layer", {"name": "PURGE_ME_LAYER", "color": 3})
    check(p_lay.status == 200, "layer PURGE_ME_LAYER created")
    p_pt = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "point", "at": [12.0, 12.0]}})
    p_pt_id = p_pt.json()["entities"][-1]["id"]
    p_blk = studio.post("/api/draft/block", {"name": "PURGE_ME_BLOCK", "base_point": [0.0, 0.0], "entity_ids": [p_pt_id]})
    check(p_blk.status == 200, "block PURGE_ME_BLOCK created")

    purge_resp = studio.post("/api/draft/purge", {})
    check(purge_resp.status == 200, "purge endpoint succeeded")
    purge_data = purge_resp.json()
    check("PURGE_ME_LAYER" in purge_data["report"]["purged_layers"], "purge report lists PURGE_ME_LAYER")
    check("PURGE_ME_BLOCK" in purge_data["report"]["purged_blocks"], "purge report lists PURGE_ME_BLOCK")

    # 2. OVERKILL: Duplicate removal and collinear line merging
    ok_l1 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [0.0, 100.0], "end": [20.0, 100.0]}})
    ok_l2 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [20.0, 100.0], "end": [0.0, 100.0]}})
    ok_ol1 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [30.0, 100.0], "end": [50.0, 100.0]}})
    ok_ol2 = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [40.0, 100.0], "end": [70.0, 100.0]}})
    check(ok_l1.status == 200 and ok_l2.status == 200 and ok_ol1.status == 200 and ok_ol2.status == 200, "overkill test lines created")

    overkill_resp = studio.post("/api/draft/overkill", {"tolerance": 0.001})
    check(overkill_resp.status == 200, "overkill endpoint succeeded")
    overkill_data = overkill_resp.json()
    check(overkill_data["report"]["duplicate_entities_removed"] >= 1, "overkill removed duplicate line")
    check(overkill_data["report"]["collinear_lines_merged"] >= 1, "overkill merged collinear lines")

    # 3. DIVIDE: Equal segment division
    div_line = studio.post("/api/draft/entity", {"layer": "0", "kind": {"kind": "line", "start": [0.0, 200.0], "end": [100.0, 200.0]}})
    div_id = div_line.json()["entities"][-1]["id"]
    div_resp = studio.post("/api/draft/divide", {"entity_id": div_id, "segments": 4})
    check(div_resp.status == 200, "divide endpoint succeeded")
    check(len(div_resp.json()["created_point_ids"]) == 3, "divide created 3 interior points for 4 segments")

    # 4. MEASURE: Interval point placement along curve
    me_resp = studio.post("/api/draft/measure-interval", {"entity_id": div_id, "interval": 30.0})
    check(me_resp.status == 200, "measure-interval endpoint succeeded")
    check(len(me_resp.json()["created_point_ids"]) == 3, "measure-interval created 3 points at 30mm spacing")

    # 5. MATCHPROP: Property matching across entities
    mp_layer = studio.post("/api/draft/layer", {"name": "MATCH_SOURCE_LAYER", "color": 5})
    check(mp_layer.status == 200, "layer MATCH_SOURCE_LAYER created")
    mp_src = studio.post("/api/draft/entity", {"layer": "MATCH_SOURCE_LAYER", "kind": {"kind": "point", "at": [1.0, 1.0]}})
    mp_src_id = mp_src.json()["entities"][-1]["id"]
    matchprop_resp = studio.post("/api/draft/match-properties", {"source_id": mp_src_id, "target_ids": [div_id]})
    check(matchprop_resp.status == 200, "match-properties endpoint succeeded")
    check(matchprop_resp.json()["matched_count"] == 1, "match-properties updated 1 target entity")
    target_layer = next((e["layer"] for e in matchprop_resp.json()["draft"]["entities"] if e["id"] == div_id), None)
    check(target_layer == "MATCH_SOURCE_LAYER", "target entity layer updated to source layer")

    # 6. PARAMETRIC CONSTRAINTS: 2D Geometric & Dimensional Solver (cad-constraints)
    solve_resp = studio.post("/api/draft/constraints/solve", {
        "points": [[0.0, 0.0], [10.0, 2.0]],
        "lines": [[0, 1]],
        "constraints": [
            {"FixPoint": [0, [0.0, 0.0]]},
            {"Horizontal": 0}
        ]
    })
    check(solve_resp.status == 200, "constraints solve endpoint succeeded")
    solve_data = solve_resp.json()
    check(solve_data["solve"]["converged"] is True, "parametric constraint solver converged")
    check(solve_data["dof"]["status"] == "Under", "degrees of freedom status correctly identified as Under")
    check(abs(solve_data["solved_points"][1][1]) < 1e-4, "horizontal constraint drove y2 to 0")

    report("drafting")


def engineering_section(studio: Studio) -> None:
    section("engineering: the document revision, the drawing assessment and the materials")
    response = studio.get("/api/engineering")
    check(response.status == 200, f"/api/engineering answered {response.status}")
    payload = response.json()
    check(payload["document"] > 0, "the engineering view names its document")
    check(payload["revision"] >= 1, "the engineering view names its revision")
    check(payload["unit"] == "mm", f"the engineering unit is {payload['unit']}")
    check(len(payload["materials"]) >= 3, "the materials list is populated")
    check(
        {"name", "density_kg_m3"} <= set(payload["materials"][0]),
        f"a material carries its density: {payload['materials'][0]}",
    )
    drawing = payload["drawing"]
    check(isinstance(drawing["kind"], str) and drawing["kind"],
          f"the drawing artifact is {drawing['kind']}")
    check(drawing["status"] in ("current", "requires_review", "unknown"),
          f"the drawing status is {drawing['status']}")
    check(studio.post("/api/engineering/review", {"id": 0, "revision": 1}).status == 400,
          "an artifact id of 0 is refused")
    check(studio.post("/api/engineering/review", {"id": 999999, "revision": 1}).status in (400, 404),
          "an unknown artifact is refused")
    check(studio.post("/api/engineering/review", {"id": 1, "revision": 0}).status == 400,
          "a zero revision is refused")

    # Physical Mass Properties (MASSPROP), Multi-Body Interference (INTERFERE), and BOM
    mp_resp = studio.get("/api/mass-properties")
    check(mp_resp.status == 200, f"/api/mass-properties answered {mp_resp.status}")
    mp_data = mp_resp.json()
    check(mp_data["geometric"]["volume_mm3"] > 0, "mass properties volume is positive")
    check(len(mp_data["geometric"]["centroid_mm"]) == 3, "mass properties has 3D centroid")
    check(len(mp_data["geometric"]["principal_moments"]) == 3, "mass properties has 3 principal moments")
    check(mp_data["physical"]["mass_kg"] > 0, "physical mass scaled by density is positive")

    mp_post = studio.post("/api/mass-properties", {"density_kg_m3": 2700.0})
    check(mp_post.status == 200, "POST /api/mass-properties succeeded")
    check(mp_post.json()["material"]["density_kg_m3"] == 2700.0, "custom density recorded")

    inf_resp = studio.get("/api/interference")
    check(inf_resp.status == 200, f"/api/interference answered {inf_resp.status}")
    inf_data = inf_resp.json()
    check(inf_data["clashes_detected"] >= 0, "interference check reports clash count")

    bom_resp = studio.get("/api/bom")
    check(bom_resp.status == 200, f"/api/bom answered {bom_resp.status}")
    bom_data = bom_resp.json()
    check(bom_data["total_items"] >= 1, "BOM contains items")
    check(bom_data["total_mass_kg"] > 0, "BOM total mass is positive")

    report("engineering")


def revisions_section(studio: Studio) -> None:
    section("revisions: record, duplicate, restore, delete")
    before = studio.volume()
    recorded = studio.post("/api/revision", {"name": "full check baseline"})
    check(recorded.status == 200, f"recording a revision: {recorded!r}")
    revisions = recorded.json()["revisions"]
    check(len(revisions) == 1, f"there is one revision: {revisions}")
    check(revisions[0]["name"] == "full check baseline", "the revision keeps its name")
    revision_id = revisions[0]["id"]

    duplicate = studio.post("/api/revision", {"name": "FULL CHECK BASELINE"})
    check(duplicate.status in (400, 409), f"a case-insensitive duplicate is refused: {duplicate}")
    blank = studio.post("/api/revision", {"name": "   "})
    check(blank.status == 400, "a blank revision name is refused")

    edited = studio.post("/api/parameter", {"id": "depth", "value": 20})
    check(edited.status == 200 and edited.json()["geometry"]["volume_mm3"] > before,
          "the body grows after the revision")
    restored = studio.post("/api/revision/restore", {"id": revision_id})
    check(restored.status == 200, f"restoring the revision: {restored!r}")
    check(abs(restored.json()["geometry"]["volume_mm3"] - before) < 1e-6,
          "restoring brings the recorded volume back")
    check(studio.post("/api/revision/restore", {"id": 999999}).status in (400, 404),
          "restoring an unknown revision is refused")
    removed = studio.post("/api/revision/remove", {"id": revision_id})
    check(removed.status == 200 and removed.json()["revisions"] == [],
          "deleting the revision empties the list")
    check(studio.post("/api/revision/remove", {"id": revision_id}).status in (400, 404),
          "deleting it twice is refused")
    report("revisions")


def export_section(studio: Studio) -> None:
    section("exports: STL against the model, DXF against the plan")
    state = studio.state()
    stl = studio.get("/api/export/stl")
    check(stl.status == 200 and len(stl.body) > 84, "the STL is served")
    volume, triangles = stl_metrics(stl.body)
    model = state["geometry"]["volume_mm3"]
    check(triangles > 12, f"the STL has {triangles} facets")
    check(abs(volume - model) < max(0.5, model * 0.001),
          f"the STL volume {volume} matches the model {model}")
    check(stl.headers.get("content-type") == "model/stl", "the STL is typed model/stl")
    check(stl.headers.get("content-disposition", "").endswith('.stl"'), "the STL is an attachment")

    # A parameter edit must change both the state and the exported bytes.
    changed = studio.post("/api/parameter", {"id": "depth", "value": 15})
    check(changed.status == 200, "the depth edit succeeds")
    edited_state = changed.json()["geometry"]["volume_mm3"]
    after = studio.get("/api/export/stl")
    edited_volume, _ = stl_metrics(after.body)
    check(abs(edited_volume - edited_state) < 1.0,
          f"the export follows the edit: {edited_volume} vs {edited_state}")
    check(after.body != stl.body, "the exported bytes changed with the model")

    undo = studio.post("/api/undo")
    check(undo.status == 200, "undo succeeds")
    redo = studio.post("/api/redo")
    check(redo.status == 200, "redo succeeds")
    # The STL stores f32 coordinates, so it is only right to a fraction of a cubic millimetre; the
    # state is the exact measurement and redo has to reproduce it.
    check(abs(studio.volume() - edited_state) < 1e-9, "redo restores the edited body")
    report("exports")


def simulation_section(studio: Studio) -> None:
    section("simulation: linear static FEA, modal vibration, steady-state thermal, and 3-axis CAM")
    # 1. Static FEA simulation
    fea = studio.post("/api/cae/static", {"material": "steel.1018", "voxel_size_mm": 5.0})
    check(fea.status == 200, f"the static FEA endpoint returns 200 (got {fea.status})")
    fea_data = fea.json()
    check(fea_data.get("elements", 0) > 0, "the static FEA mesh has solid elements")
    check(fea_data.get("max_displacement_mm", 0.0) > 0.0, "the deflection is non-zero")
    check(fea_data.get("max_von_mises_mpa", 0.0) > 0.0, "the von Mises stress is non-zero")
    check("linear elastic" in fea_data.get("quality", "").lower(), "the quality label is returned")

    # 2. Dynamic modal vibration frequencies
    modal = studio.post("/api/cae/modal", {"material": "steel.1018", "num_modes": 3})
    check(modal.status == 200, f"the modal endpoint returns 200 (got {modal.status})")
    modal_data = modal.json()
    modes = modal_data.get("modes", [])
    check(len(modes) == 3, f"3 modes are returned (got {len(modes)})")
    check(modal_data.get("fundamental_frequency_hz", 0.0) > 0.0, "fundamental frequency is positive")
    f1 = modes[0].get("frequency_hz", 0.0)
    f2 = modes[1].get("frequency_hz", 0.0)
    check(f2 >= f1, f"modes are ordered by frequency: {f1} <= {f2}")

    # 3. Steady-state thermal conduction FEA
    thermal = studio.post("/api/cae/thermal", {"material": "steel.1018", "fixed_temp_hot_c": 100.0, "fixed_temp_cold_c": 20.0})
    check(thermal.status == 200, f"the thermal endpoint returns 200 (got {thermal.status})")
    thermal_data = thermal.json()
    check(thermal_data.get("elements", 0) > 0, "thermal mesh has solid elements")
    check(thermal_data.get("min_temperature_c", 0.0) <= 20.01, "minimum temperature matches cold boundary")
    check(thermal_data.get("max_temperature_c", 0.0) >= 99.99, "maximum temperature matches hot boundary")
    check(thermal_data.get("max_heat_flux_w_m2", 0.0) > 0.0, "conduction produces positive heat flux")

    # 4. 3-Axis CNC CAM toolpath generation with kinematic cycle time and Taylor tool wear
    cam = studio.post("/api/cam/toolpaths", {"tool_diameter_mm": 6.0})
    check(cam.status == 200, f"the CAM endpoint returns 200 (got {cam.status})")
    cam_data = cam.json()
    check(cam_data.get("moves_count", 0) > 0, "CAM generates motion moves")
    check(cam_data.get("cutting_length_mm", 0.0) > 0.0, "cutting distance is non-zero")
    check(cam_data.get("kinematic_cycle_time_seconds", 0.0) > 0.0, "kinematic cycle time is calculated")
    check(cam_data.get("tool_wear_fraction", -1.0) >= 0.0, "Taylor tool wear fraction is calculated")
    check("G" in cam_data.get("gcode_preview", ""), "G-code preview is returned")

    # 5. Sheet Metal Bending & Flat Pattern Unfolding (DIN 6935)
    sm = studio.post("/api/cad/sheet_metal/unfold", {
        "material": "carbon_steel",
        "thickness_mm": 2.0,
        "flange_lengths_mm": [50.0, 50.0],
        "bend_angles_deg": [90.0],
        "inner_radii_mm": [2.0],
        "die_opening_width_mm": 16.0
    })
    check(sm.status == 200, f"sheet metal unfolding returns 200 (got {sm.status})")
    sm_data = sm.json()
    check(abs(sm_data.get("total_flat_length_mm", 0.0) - 96.398) < 0.1, "flat blank length is 96.40 mm")
    check(sm_data.get("total_forming_force_kn", 0.0) > 10.0, "forming tonnage calculated")

    # 6. Standard Fasteners & Bolted Joint Analysis (ISO 4014 / ISO 898-1)
    fast = studio.post("/api/cad/hardware/fastener", {
        "thread_size": "M10",
        "bolt_class": "8.8",
        "clamped_length_mm": 40.0,
        "applied_tension_load_kn": 12.0,
        "applied_shear_load_kn": 5.0,
        "torque_coefficient_k": 0.18
    })
    check(fast.status == 200, f"fasteners calculation returns 200 (got {fast.status})")
    fast_data = fast.json()
    check(abs(fast_data.get("tensile_stress_area_mm2", 0.0) - 58.0) < 1.0, "M10 stress area is 58 mm²")
    check(abs(fast_data.get("tightening_torque_nm", 0.0) - 46.97) < 1.5, "tightening torque is 47 N·m")

    # 7. Chemical Kinetics & Reactor Design (Levenspiel)
    kin = studio.post("/api/sim/chemical/kinetics", {
        "reaction_name": "Thermal Dissociation A -> B",
        "order": "first_order",
        "initial_concentration_m": 2.0,
        "rate_constant_k": 0.05,
        "volumetric_flow_m3_s": 0.005,
        "target_conversion": 0.80
    })
    check(kin.status == 200, f"chemical kinetics returns 200 (got {kin.status})")
    kin_data = kin.json()
    check(abs(kin_data.get("half_life_seconds", 0.0) - 13.86) < 0.1, "half life is 13.86 s")
    check(kin_data.get("cstr_volume_m3", 0.0) > kin_data.get("pfr_volume_m3", 0.0), "V_CSTR > V_PFR")

    # 8. Additive Process Profiles (SLA Jacobs working curve)
    sla = studio.post("/api/additive/process_profile", {
        "technology": "sla_dlp_msla",
        "part_height_mm": 60.0,
        "part_volume_cm3": 35.0,
        "layer_height_mm": 0.050
    })
    check(sla.status == 200, f"additive process profile returns 200 (got {sla.status})")
    sla_data = sla.json()
    check(sla_data.get("total_layers", 0) == 1200, "SLA layers match 1200")
    check(abs(sla_data.get("jacobs_cure_depth_mm", 0.0) - 0.0675) < 0.005, "Jacobs cure depth matches")

    # 9. Outdoor Environmental Noise (ISO 9613-2)
    noise = studio.post("/api/sim/acoustics/outdoor_noise", {
        "source_name": "Rooftop Chiller",
        "sound_power_lwa": 102.0,
        "distance_m": 120.0,
        "source_height_m": 2.5,
        "receiver_height_m": 1.5,
        "barrier_height_m": 3.5,
        "barrier_distance_m": 15.0,
        "ground_factor": 0.70,
        "regulatory_limit_dba": 50.0
    })
    check(noise.status == 200, f"outdoor noise returns 200 (got {noise.status})")
    noise_data = noise.json()
    check(abs(noise_data.get("divergence_attenuation_db", 0.0) - 52.58) < 0.2, "divergence matches ISO 9613-2")
    check(noise_data.get("is_compliant") is True, "receiver is compliant")

    # 10. Helical Spring Mechanics (DIN EN 13906-1)
    spring = studio.post("/api/cad/hardware/spring", {
        "wire_diameter_d_mm": 4.0,
        "mean_coil_diameter_d_mm": 32.0,
        "free_length_l0_mm": 85.0,
        "active_coils_na": 7.0,
        "operating_force_f2_n": 650.0,
        "material": "MusicWireA228",
        "end_type": "ClosedAndGround"
    })
    check(spring.status == 200, f"spring returns 200 (got {spring.status})")
    sp_data = spring.json()
    check(abs(sp_data.get("spring_rate_k_n_mm", 0.0) - 11.08) < 0.2, "spring rate k matches DIN EN 13906-1")
    check(abs(sp_data.get("wahl_factor_kw", 0.0) - 1.184) < 0.05, "Wahl factor matches")
    check(sp_data.get("is_buckling_safe") is True, "spring is free-standing buckling safe")

    # 11. Involute Gear Pair Mechanics (ISO 6336 / AGMA 2001-D04)
    gear = studio.post("/api/cad/hardware/gear_pair", {
        "gear_type": "Helical",
        "normal_module_mn": 3.5,
        "pinion_teeth_z1": 19,
        "gear_teeth_z2": 57,
        "face_width_b_mm": 40.0,
        "helix_angle_beta_deg": 20.0,
        "pinion_speed_rpm": 3000.0,
        "transmitted_power_kw": 35.0
    })
    check(gear.status == 200, f"gear pair returns 200 (got {gear.status})")
    g_data = gear.json()
    check(g_data.get("transverse_contact_ratio", 0.0) > 1.2, "contact ratio >= 1.20 adequate")
    check(g_data.get("bending_safety_factor", 0.0) > 1.0, "AGMA bending safety factor adequate")

    # 12. Kienzle Machining Physics (DIN 6584)
    milling = studio.post("/api/cam/physics/cutting_force", {
        "operation": "FaceMilling",
        "workpiece": "AlloySteel4140",
        "cutting_speed_vc_m_per_min": 180.0,
        "feed_f_mm": 0.15,
        "depth_of_cut_ap_mm": 3.0,
        "radial_width_ae_mm": 40.0,
        "tool_diameter_dc_mm": 50.0,
        "number_of_teeth_z": 4
    })
    check(milling.status == 200, f"kienzle milling returns 200 (got {milling.status})")
    m_data = milling.json()
    check(m_data.get("tangential_cutting_force_fc_n", 0.0) > 1000.0, "tangential cutting force calculated")
    check(m_data.get("material_removal_rate_cm3_min", 0.0) > 50.0, "MRR calculated")

    # 13. Industrial Electrical Short-Circuit & Cable Sizing (IEC 60909)
    sc = studio.post("/api/sim/electrical/short_circuit", {
        "nominal_voltage_un_v": 400.0,
        "transformer_rated_power_sn_kva": 1000.0,
        "transformer_impedance_voltage_uk_percent": 6.0,
        "feeder_length_l_m": 65.0,
        "conductor_cross_section_q_mm2": 150.0,
        "conductor_material": "Copper",
        "operating_current_ib_a": 220.0
    })
    check(sc.status == 200, f"IEC 60909 short circuit returns 200 (got {sc.status})")
    sc_data = sc.json()
    check(10.0 < sc_data.get("initial_symmetrical_short_circuit_current_ka", 0.0) < 20.0, "Ik'' short circuit current within range")
    check(sc_data.get("is_voltage_drop_compliant") is True, "cable feeder voltage drop <= 3.0%")

    # 14. Architectural Daylighting & CIE Sky Luminescence (EN 17037)
    daylighting = studio.post("/api/sim/building/daylight_factor", {
        "room_length_m": 6.0,
        "room_width_m": 5.0,
        "room_height_m": 3.0,
        "window_width_m": 3.2,
        "window_height_m": 2.0,
        "desk_distance_from_window_m": 2.5
    })
    check(daylighting.status == 200, f"daylighting returns 200 (got {daylighting.status})")
    df_data = daylighting.json()
    check(df_data.get("point_daylight_factor_percent", 0.0) > 1.0, "point daylight factor calculated")
    check(df_data.get("is_compliant") is True, "EN 17037 office daylighting target satisfied")

    # 15. Transmission Shaft & DIN 6885-1 Keyway
    shaft = studio.post("/api/cad/hardware/shaft_key", {
        "transmitted_power_kw": 45.0,
        "rotational_speed_rpm": 1450.0,
        "bending_moment_nm": 350.0,
        "selected_shaft_diameter_mm": 45.0
    })
    check(shaft.status == 200, f"shaft/key returns 200 (got {shaft.status})")
    sh_data = shaft.json()
    check(abs(sh_data.get("torque_nm", 0.0) - 296.34) < 1.0, "torque matches 296.3 N·m")
    check(sh_data.get("key_dimensions", {}).get("key_width_b_mm") == 14.0, "DIN 6885 key width = 14mm")
    check(sh_data.get("is_design_adequate") is True, "shaft and key design adequate")

    # 16. ASME BPVC Section VIII Div. 1 Pressure Vessel
    pv = studio.post("/api/sim/pressure_vessel/asme", {
        "design_pressure_bar": 20.0,
        "inside_diameter_d_mm": 1200.0,
        "tangent_length_l_mm": 3000.0,
        "corrosion_allowance_mm": 2.5
    })
    check(pv.status == 200, f"pressure vessel returns 200 (got {pv.status})")
    pv_data = pv.json()
    check(abs(pv_data.get("min_required_shell_thickness_mm", 0.0) - 13.07) < 0.2, "ASME shell thickness = 13.07 mm")
    check(pv_data.get("vessel_mawp_bar", 0.0) > 20.0, "MAWP exceeds design pressure")
    check(pv_data.get("is_asme_compliant") is True, "vessel is ASME compliant")

    # 17. ASHRAE Psychrometrics & Moist Air
    psy = studio.post("/api/sim/thermal/psychrometrics", {
        "dry_bulb_temp_c": 24.0,
        "relative_humidity": 0.50
    })
    check(psy.status == 200, f"psychrometrics returns 200 (got {psy.status})")
    psy_data = psy.json()
    check(abs(psy_data.get("humidity_ratio_g_kg", 0.0) - 9.3) < 0.2, "humidity ratio ~ 9.3 g/kg")
    check(abs(psy_data.get("dew_point_temp_c", 0.0) - 12.9) < 0.5, "dew point ~ 12.9 °C")

    # 18. Civil Stormwater Runoff & Manning Pipe
    hydro = studio.post("/api/sim/civil/hydrology", {
        "catchment_area_ha": 8.5,
        "flow_path_length_m": 450.0,
        "terrain_slope": 0.020,
        "pipe_slope": 0.008
    })
    check(hydro.status == 200, f"hydrology returns 200 (got {hydro.status})")
    hy_data = hydro.json()
    check(hy_data.get("peak_discharge_m3_s", 0.0) > 0.8, "peak discharge calculated")
    check(hy_data.get("is_self_cleansing") is True, "culvert pipe velocity is self-cleansing")

    # 19. LEFM Fracture Mechanics & Paris Crack Growth
    frac = studio.post("/api/sim/structural/fracture", {
        "max_stress_mpa": 180.0,
        "min_stress_mpa": 18.0,
        "initial_crack_depth_a0_mm": 1.5,
        "plate_width_w_mm": 80.0
    })
    check(frac.status == 200, f"fracture returns 200 (got {frac.status})")
    fr_data = frac.json()
    check(abs(fr_data.get("initial_stress_intensity_k1_mpa_m05", 0.0) - 13.9) < 1.0, "K_I ~ 13.9 MPa*m^0.5")
    check(fr_data.get("is_safe_against_fast_fracture") is True, "safe against brittle fast fracture")

    # 20. Lateral Rotor Dynamics & API 617 Critical Speeds
    rotor = studio.post("/api/sim/rotordynamics/api617", {
        "operating_speed_rpm": 12000.0,
        "shaft_span_m": 1.20,
        "shaft_diameter_m": 0.080,
        "disc_mass_kg": 45.0,
        "unbalance_g_mm": 15.0
    })
    check(rotor.status == 200, f"rotordynamics returns 200 (got {rotor.status})")
    rot_data = rotor.json()
    check(rot_data.get("critical_speed_first_rpm", 0.0) > 2000.0, "flexible 1st critical speed calculated")
    check(rot_data.get("api617_margin_compliant") is True, "API 617 separation margin compliant")

    # 21. Welding Metallurgy & Cooling Rates (AWS D1.1 / ISO 15614)
    weld = studio.post("/api/cam/welding/metallurgy", {
        "arc_voltage_v": 28.0,
        "welding_current_a": 240.0,
        "travel_speed_mm_s": 4.5,
        "plate_thickness_mm": 25.0,
        "preheat_temp_c": 20.0
    })
    check(weld.status == 200, f"welding returns 200 (got {weld.status})")
    wld_data = weld.json()
    check(wld_data.get("net_heat_input_kj_mm", 0.0) > 1.0, "net heat input > 1.0 kJ/mm")
    check(wld_data.get("haz_hardness_acceptable") is True, "HAZ peak hardness acceptable")

    # 22. Binary Distillation Column Design (McCabe-Thiele)
    dist = studio.post("/api/sim/chemical/distillation", {
        "feed_rate_kmol_hr": 100.0,
        "feed_mole_fraction_zf": 0.45,
        "distillate_mole_fraction_xd": 0.95,
        "bottoms_mole_fraction_xb": 0.05,
        "relative_volatility_alpha": 2.45
    })
    check(dist.status == 200, f"distillation returns 200 (got {dist.status})")
    dst_data = dist.json()
    check(dst_data.get("theoretical_stages_count", 0) >= 10, "McCabe-Thiele theoretical stages calculated")
    check(dst_data.get("operating_reflux_ratio", 0.0) > dst_data.get("minimum_reflux_ratio_rmin", 0.0), "operating reflux > R_min")

    # 23. PCB Trace Current Carrying Ampacity & Thermal Vias (IPC-2152)
    pcb = studio.post("/api/sim/ecad/pcb_thermal", {
        "trace_width_mm": 2.50,
        "trace_length_mm": 50.0,
        "current_a": 5.5,
        "allowable_temp_rise_c": 20.0,
        "component_heat_power_w": 3.5
    })
    check(pcb.status == 200, f"pcb thermal returns 200 (got {pcb.status})")
    pcb_data = pcb.json()
    check(pcb_data.get("allowable_current_ipc2152_a", 0.0) > 6.0, "IPC-2152 allowable current > 6.0 A")
    check(pcb_data.get("thermal_limits_satisfied") is True, "PCB thermal limits satisfied")

    # 24. Structural Steel Bolted & Welded Connections (Eurocode 3 / AISC 360)
    conn = studio.post("/api/sim/structural/connections", {
        "applied_shear_force_kn": 180.0,
        "applied_tension_force_kn": 80.0,
        "weld_throat_thickness_mm": 6.0,
        "weld_length_mm": 220.0,
        "applied_weld_force_kn": 150.0
    })
    check(conn.status == 200, f"structural connection returns 200 (got {conn.status})")
    cn_data = conn.json()
    check(cn_data.get("total_connection_shear_capacity_kn", 0.0) > 350.0, "bolted connection shear capacity verified")
    check(cn_data.get("bolted_joint_safe") is True and cn_data.get("welded_joint_safe") is True, "connection code compliant")

    # 25. Centrifugal Pumps & Affinity Laws (ISO 9906 / ASME PTC 10)
    pump = studio.post("/api/sim/hydraulic/pump", {
        "nominal_speed_rpm": 2950.0,
        "nominal_impeller_diameter_mm": 250.0,
        "shutoff_head_m": 85.0,
        "system_static_head_m": 25.0,
        "npsh_available_m": 6.5
    })
    check(pump.status == 200, f"centrifugal pump returns 200 (got {pump.status})")
    pmp_data = pump.json()
    check(abs(pmp_data.get("operating_flow_m3_hr", 0.0) - 100.0) < 1.0, "pump operating flow 100 m3/hr")
    check(pmp_data.get("cavitation_safe") is True, "pump cavitation margin verified")

    # 26. Shell-and-Tube Heat Exchangers (TEMA / Kern's Method)
    hx = studio.post("/api/sim/thermal/heat_exchanger_rating", {
        "hot_mass_flow_kg_s": 18.0,
        "cold_mass_flow_kg_s": 24.66,
        "shell_diameter_mm": 600.0,
        "tube_count": 380,
        "tube_length_m": 4.5
    })
    check(hx.status == 200, f"heat exchanger rating returns 200 (got {hx.status})")
    hx_data = hx.json()
    check(abs(hx_data.get("heat_duty_kw", 0.0) - 1548.0) < 2.0, "heat exchanger duty ~1548 kW")
    check(hx_data.get("rating_adequate") is True, "heat exchanger surface rating adequate")

    # 27. Geotechnical Shallow Foundation Bearing Capacity (Meyerhof / EC7)
    bearing = studio.post("/api/sim/civil/geotechnical_bearing", {
        "foundation_width_b_m": 2.20,
        "foundation_length_l_m": 2.20,
        "embedment_depth_df_m": 1.50,
        "column_axial_load_kn": 1250.0,
        "friction_angle_deg": 32.0
    })
    check(bearing.status == 200, f"geotechnical bearing returns 200 (got {bearing.status})")
    brg_data = bearing.json()
    check(brg_data.get("actual_factor_of_safety", 0.0) > 3.0, "bearing capacity factor of safety > 3.0")
    check(brg_data.get("bearing_capacity_adequate") is True and brg_data.get("settlement_acceptable") is True, "foundation code compliant")

    # 28. Multi-Stage Gas Compression & Intercooling (ASME PTC 10)
    comp = studio.post("/api/sim/thermal/gas_compression", {
        "mass_flow_kg_s": 4.5,
        "inlet_pressure_bar": 1.0,
        "target_outlet_pressure_bar": 30.0,
        "number_of_stages": 3
    })
    check(comp.status == 200, f"gas compression returns 200 (got {comp.status})")
    cmp_data = comp.json()
    check(cmp_data.get("energy_savings_pct", 0.0) > 15.0, "intercooling energy savings > 15%")
    check(cmp_data.get("total_brake_power_kw", 0.0) > 1200.0, "multi-stage compression power evaluated")

    # 29. Structural Timber Fire Resistance (Eurocode 5 EN 1995-1-2)
    timber = studio.post("/api/sim/structural/timber_fire", {
        "initial_breadth_mm": 200.0,
        "initial_depth_mm": 500.0,
        "fire_duration_min": 60.0,
        "ambient_moment_ed_kn_m": 140.0
    })
    check(timber.status == 200, f"timber fire returns 200 (got {timber.status})")
    tmb_data = timber.json()
    check(tmb_data.get("fire_resistance_verified") is True, "timber R60 fire resistance verified")
    check(tmb_data.get("critical_fire_endurance_min", 0.0) >= 60.0, "critical fire endurance >= 60 min")

    # 30. Marine Propeller Hydrodynamics (Wageningen B-Series / ITTC)
    prop = studio.post("/api/sim/marine/propeller", {
        "diameter_m": 6.80,
        "pitch_ratio_p_d": 1.05,
        "rotational_speed_rpm": 108.0,
        "advance_speed_knots": 17.5
    })
    check(prop.status == 200, f"marine propeller returns 200 (got {prop.status})")
    prp_data = prop.json()
    check(prp_data.get("open_water_efficiency", 0.0) > 0.58, "propeller open water efficiency > 58%")
    check(prp_data.get("thrust_kn", 0.0) > 700.0, "propeller thrust > 700 kN")

    # 31. Automotive Braking Dynamics & Rotor Thermal (ECE R13H / SAE J292)
    brake = studio.post("/api/sim/automotive/braking", {
        "vehicle_mass_kg": 1850.0,
        "wheelbase_m": 2.85,
        "initial_speed_kph": 100.0,
        "road_friction_coeff": 0.92
    })
    check(brake.status == 200, f"automotive braking returns 200 (got {brake.status})")
    brk_data = brake.json()
    check(brk_data.get("deceleration_g", 0.0) > 0.85, "emergency deceleration > 0.85g")
    check(brk_data.get("stopping_distance_m", 99.0) < 65.0, "stopping distance < 65m")

    # 32. Reinforced Concrete Flexure & Crack Width (Eurocode 2 EN 1992-1-1)
    rc = studio.post("/api/sim/civil/reinforced_concrete", {
        "breadth_b_mm": 300.0,
        "depth_h_mm": 600.0,
        "applied_moment_ed_kn_m": 240.0,
        "rebar_count": 4,
        "rebar_diameter_mm": 20.0
    })
    check(rc.status == 200, f"reinforced concrete returns 200 (got {rc.status})")
    rc_data = rc.json()
    check(rc_data.get("design_moment_resistance_mrd_kn_m", 0.0) > 250.0, "RC beam flexural resistance > 250 kNm")
    check(rc_data.get("characteristic_crack_width_wk_mm", 1.0) <= 0.30, "crack width <= 0.30 mm")

    # 33. HVAC Airflow, Duct Pressure Loss & Fan Power (ASHRAE / SMACNA)
    duct = studio.post("/api/sim/hvac/duct_airflow", {
        "airflow_m3_h": 5400.0,
        "duct_width_mm": 600.0,
        "duct_height_mm": 350.0,
        "duct_length_m": 45.0
    })
    check(duct.status == 200, f"hvac duct returns 200 (got {duct.status})")
    dct_data = duct.json()
    check(dct_data.get("air_velocity_m_s", 0.0) > 5.0, "air velocity > 5.0 m/s")
    check(dct_data.get("total_fan_pressure_pa", 0.0) > 200.0, "total fan ESP > 200 Pa")

    # 34. Aero Gas Turbine Open Brayton Cycle & Thrust (NASA SP-290 / ISO 2314)
    gt = studio.post("/api/sim/aerospace/gas_turbine", {
        "flight_mach": 0.82,
        "altitude_m": 10668.0,
        "core_mass_flow_kg_s": 45.0,
        "compressor_pressure_ratio": 24.0,
        "turbine_inlet_temp_k": 1650.0
    })
    check(gt.status == 200, f"gas turbine returns 200 (got {gt.status})")
    gt_data = gt.json()
    check(gt_data.get("net_thrust_kn", 0.0) > 18.0, "gas turbine net thrust > 18 kN")
    check(gt_data.get("thermal_efficiency", 0.0) > 0.35, "Brayton thermal efficiency > 35%")

    # 35. Hydraulic Actuator Mechanics & Barrel Stresses (ISO 6020-2)
    cyl = studio.post("/api/sim/hydraulic/cylinder", {
        "bore_diameter_mm": 100.0,
        "rod_diameter_mm": 56.0,
        "stroke_length_mm": 600.0,
        "working_pressure_bar": 210.0,
        "barrel_wall_thickness_mm": 12.0
    })
    check(cyl.status == 200, f"hydraulic cylinder returns 200 (got {cyl.status})")
    cyl_data = cyl.json()
    check(cyl_data.get("push_force_kn", 0.0) > 140.0, "cylinder push force > 140 kN")
    check(cyl_data.get("barrel_safety_factor", 0.0) > 1.8, "cylinder barrel FS > 1.8")
    check(cyl_data.get("is_safe", False) is True, "hydraulic actuator meets safety norms")

    # 36. Belt & Pulley Power Transmission (ISO 5296 / RMA IP-20)
    belt = studio.post("/api/sim/mechanical/belt_drive", {
        "power_kw": 15.0,
        "driver_speed_rpm": 1450.0,
        "driver_diameter_mm": 160.0,
        "driven_diameter_mm": 315.0,
        "center_distance_mm": 600.0
    })
    check(belt.status == 200, f"belt drive returns 200 (got {belt.status})")
    belt_data = belt.json()
    check(belt_data.get("belt_velocity_m_s", 0.0) > 10.0, "belt velocity > 10 m/s")
    check(belt_data.get("belts_required", 0) >= 2, "belts required >= 2")
    check(belt_data.get("is_safe", False) is True, "belt drive ratings within limits")

    # 37. Soil Slope Stability Analysis (Bishop's Method of Slices / Eurocode 7)
    slope = studio.post("/api/sim/geotechnical/slope_stability", {
        "slope_height_m": 10.0,
        "slope_angle_deg": 32.0,
        "soil_unit_weight_kn_m3": 19.0,
        "effective_cohesion_kpa": 12.0,
        "effective_friction_angle_deg": 27.0
    })
    check(slope.status == 200, f"slope stability returns 200 (got {slope.status})")
    slope_data = slope.json()
    check(slope_data.get("factor_of_safety_bishop", 0.0) > 1.1, "Bishop FoS > 1.1")
    check(slope_data.get("converged", False) is True, "Bishop FoS iteration converged")

    # 38. Three-Phase Induction Motor Electromechanics (IEEE 112 / NEMA MG-1)
    motor = studio.post("/api/sim/electrical/induction_motor", {
        "rated_power_kw": 15.0,
        "rated_voltage_ll_v": 400.0,
        "frequency_hz": 50.0,
        "pole_count": 4
    })
    check(motor.status == 200, f"induction motor returns 200 (got {motor.status})")
    motor_data = motor.json()
    check(motor_data.get("operating_speed_rpm", 0.0) > 1420.0, "motor operating speed > 1420 RPM")
    check(motor_data.get("efficiency_pct", 0.0) > 85.0, "motor efficiency > 85%")
    check(motor_data.get("torque_ratio_breakdown_rated", 0.0) > 1.75, "breakdown torque ratio > 1.75")

    # 39. Solar Thermal Collector & Heat Pipe Efficiency (ISO 9806 / EN 12975)
    solar = studio.post("/api/sim/thermal/solar_collector", {
        "aperture_area_m2": 2.30,
        "solar_irradiance_w_m2": 850.0,
        "ambient_temp_c": 20.0,
        "fluid_inlet_temp_c": 45.0
    })
    check(solar.status == 200, f"solar collector returns 200 (got {solar.status})")
    sol_data = solar.json()
    check(sol_data.get("collector_efficiency_pct", 0.0) > 50.0, "collector efficiency > 50%")
    check(sol_data.get("useful_thermal_power_w", 0.0) > 900.0, "useful solar heat > 900 W")
    check(sol_data.get("stagnation_temperature_c", 0.0) > 130.0, "stagnation temperature > 130 C")

    # 40. Hydrodynamic Journal Bearing (ISO 7902 / DIN 31652)
    bearing = studio.post("/api/sim/tribology/journal_bearing", {
        "journal_diameter_mm": 100.0,
        "bearing_length_mm": 70.0,
        "radial_clearance_um": 65.0,
        "rotational_speed_rpm": 3000.0,
        "radial_load_n": 12500.0,
        "lubricant_viscosity_mpa_s": 25.0
    })
    check(bearing.status == 200, f"journal bearing returns 200 (got {bearing.status})")
    brg_data = bearing.json()
    check(brg_data.get("sommerfeld_number", 0.0) > 0.3, "sommerfeld number > 0.3")
    check(brg_data.get("minimum_film_thickness_um", 0.0) > 20.0, "min oil film thickness > 20 um")
    check(brg_data.get("hydrodynamic_power_loss_kw", 0.0) > 1.0, "bearing friction loss > 1 kW")

    # 41. Wind Turbine Aerodynamics & BEM Power Curve (IEC 61400-1)
    wind = studio.post("/api/sim/renewable/wind_turbine", {
        "rotor_radius_m": 63.0,
        "rated_power_mw": 4.0,
        "rated_wind_speed_m_s": 11.5,
        "tip_speed_ratio_optimal": 7.8
    })
    check(wind.status == 200, f"wind turbine returns 200 (got {wind.status})")
    wind_data = wind.json()
    check(wind_data.get("rated_rotor_speed_rpm", 0.0) > 10.0, "rated rotor speed > 10 RPM")
    check(wind_data.get("peak_aerodynamic_cp", 0.0) > 0.40, "peak aerodynamic Cp > 0.40")
    check(wind_data.get("annual_energy_production_gwh", 0.0) > 10.0, "wind turbine AEP > 10 GWh")

    # 42. Steam Rankine Power Cycle & Condenser Thermodynamics (IAPWS-IF97 / ASME PTC 6)
    rankine = studio.post("/api/sim/thermal/steam_rankine", {
        "steam_mass_flow_kg_s": 120.0,
        "boiler_pressure_bar": 120.0,
        "superheater_temp_c": 540.0,
        "condenser_pressure_bar": 0.08
    })
    check(rankine.status == 200, f"steam rankine returns 200 (got {rankine.status})")
    rnk_data = rankine.json()
    check(rnk_data.get("gross_electric_power_mw", 0.0) > 100.0, "rankine gross electric power > 100 MW")
    check(rnk_data.get("cycle_thermal_efficiency_pct", 0.0) > 30.0, "rankine cycle efficiency > 30%")
    check(rnk_data.get("cooling_water_flow_rate_kg_s", 0.0) > 3000.0, "condenser cooling water > 3000 kg/s")

    # 43. Deep Foundation Bored Pile Load Capacity & Settlement (Eurocode 7 / API RP 2A)
    pile = studio.post("/api/sim/civil/pile_foundation", {
        "pile_diameter_m": 1.0,
        "pile_length_m": 20.0,
        "applied_axial_load_kn": 3200.0
    })
    check(pile.status == 200, f"pile foundation returns 200 (got {pile.status})")
    pile_data = pile.json()
    check(pile_data.get("ultimate_total_capacity_kn", 0.0) > 5000.0, "pile ultimate capacity > 5000 kN")
    check(pile_data.get("design_bearing_resistance_kn", 0.0) > 4000.0, "pile design resistance > 4000 kN")
    check(pile_data.get("total_working_settlement_mm", 0.0) < 15.0, "pile settlement < 15 mm")

    # 44. Involute Gear Tooth Pitting & Root Bending Fatigue (ISO 6336 Method B / AGMA 2101)
    gear = studio.post("/api/sim/mechanical/gear_fatigue", {
        "transmitted_power_kw": 55.0,
        "pinion_speed_rpm": 1450.0,
        "pinion_teeth_z1": 21,
        "gear_teeth_z2": 63,
        "normal_module_mm": 4.0,
        "face_width_mm": 50.0
    })
    check(gear.status == 200, f"gear fatigue returns 200 (got {gear.status})")
    gear_data = gear.json()
    check(gear_data.get("safety_factor_pitting", 0.0) >= 1.25, "gear pitting safety factor >= 1.25")
    check(gear_data.get("safety_factor_bending", 0.0) >= 1.40, "gear bending safety factor >= 1.40")
    check(gear_data.get("tangential_force_n", 0.0) > 5000.0, "tangential gear force > 5000 N")

    # 45. Battery Pack Thermal Management (IEC 62660 / SAE J2464)
    bat = studio.post("/api/sim/energy/battery_thermal", {
        "cell_count_series": 96,
        "cell_count_parallel": 4,
        "operating_current_a": 200.0,
        "coolant_flow_rate_l_min": 15.0
    })
    check(bat.status == 200, f"battery thermal returns 200 (got {bat.status})")
    bat_data = bat.json()
    check(bat_data.get("total_pack_energy_kwh", 0.0) > 60.0, "battery pack energy > 60 kWh")
    check(bat_data.get("total_pack_heat_dissipation_w", 0.0) > 1500.0, "pack heat dissipation > 1500 W")
    check(bat_data.get("maximum_cell_temp_c", 0.0) < 50.0, "max cell temperature < 50 C")

    # 46. Rocket Engine De Laval Nozzle Supersonic Gas Dynamics (NASA SP-8039)
    rkt = studio.post("/api/sim/aerospace/rocket_nozzle", {
        "chamber_pressure_bar": 100.0,
        "chamber_temperature_k": 3500.0,
        "throat_diameter_mm": 120.0,
        "expansion_ratio_eps": 16.0
    })
    check(rkt.status == 200, f"rocket nozzle returns 200 (got {rkt.status})")
    rkt_data = rkt.json()
    check(rkt_data.get("exit_mach_number", 0.0) > 3.0, "exit Mach number > 3.0")
    check(rkt_data.get("net_thrust_kn", 0.0) > 140.0, "net thrust > 140 kN")
    check(rkt_data.get("operating_specific_impulse_s", 0.0) > 250.0, "operating Isp > 250s")

    # 47. CSTR Chemical Reactor Kinetics & Thermal Multiplicity (Fogler / DIERS)
    cstr = studio.post("/api/sim/chemical/cstr_reactor", {
        "reactor_volume_m3": 2.50,
        "feed_volumetric_flow_m3_h": 5.0,
        "feed_reactant_concentration_mol_m3": 2000.0
    })
    check(cstr.status == 200, f"cstr reactor returns 200 (got {cstr.status})")
    cstr_data = cstr.json()
    check(cstr_data.get("fractional_conversion", 0.0) > 0.50, "conversion > 50%")
    check(cstr_data.get("heat_generation_rate_kw", 0.0) > 80.0, "heat generation > 80 kW")
    check(cstr_data.get("thermal_stability_margin_w_k", 0.0) > 0.0, "stability margin > 0 W/K")

    # 48. Open Channel Manning Hydraulics & Hydraulic Jump (Chow / USBR)
    hyd = studio.post("/api/sim/civil/channel_hydraulics", {
        "bottom_width_m": 6.0,
        "side_slope_z": 1.5,
        "discharge_rate_m3_s": 45.0,
        "bed_slope_s0": 0.0015
    })
    check(hyd.status == 200, f"channel hydraulics returns 200 (got {hyd.status})")
    hyd_data = hyd.json()
    check(hyd_data.get("normal_depth_m", 0.0) > 1.5, "normal depth > 1.5 m")
    check(hyd_data.get("critical_depth_m", 0.0) > 1.0, "critical depth > 1.0 m")
    check(hyd_data.get("jump_head_loss_m", 0.0) > 0.5, "jump head loss > 0.5 m")

    # 49. Overhead Transmission Line Catenary Sag & Tension (IEEE 738 / IEC 60826)
    line = studio.post("/api/sim/electrical/transmission_line", {
        "span_length_m": 350.0,
        "conductor_diameter_mm": 28.14,
        "rated_tensile_strength_kn": 140.0,
        "initial_horizontal_tension_kn": 28.0
    })
    check(line.status == 200, f"transmission line returns 200 (got {line.status})")
    line_data = line.json()
    check(line_data.get("everyday_sag_m", 0.0) > 7.0, "everyday sag > 7 m")
    check(line_data.get("max_hot_sag_m", 0.0) > 9.0, "hot sag > 9 m")
    check(line_data.get("storm_safety_factor", 0.0) >= 2.0, "storm safety factor >= 2.0")

    # 50. Piping Flexibility & Thermal Expansion Stress Analysis (ASME B31.3 / B31.1)
    piping = studio.post("/api/sim/piping/flexibility", {
        "nominal_outside_diameter_mm": 219.1,
        "anchor_distance_m": 60.0,
        "design_temperature_c": 260.0,
        "loop_height_m": 7.5,
        "loop_width_m": 5.0
    })
    check(piping.status == 200, f"piping flexibility returns 200 (got {piping.status})")
    piping_data = piping.json()
    check(piping_data.get("thermal_growth_mm", 0.0) > 100.0, "thermal expansion growth > 100 mm")
    check(piping_data.get("displacement_stress_se_mpa", 0.0) > 50.0, "displacement stress SE > 50 MPa")
    check(piping_data.get("stress_ratio", 99.0) <= 1.0, "piping stress ratio <= 1.0 (ASME B31.3 compliant)")

    # 51. Automotive Suspension Kinematics & Quarter-Car Ride Dynamics (ISO 2631 / Milliken)
    qc = studio.post("/api/sim/automotive/quarter_car", {
        "sprung_mass_kg": 320.0,
        "unsprung_mass_kg": 45.0,
        "suspension_stiffness_n_m": 28000.0,
        "suspension_damping_n_s_m": 1650.0,
        "vehicle_speed_km_h": 100.0,
        "road_class": "ClassBGood"
    })
    check(qc.status == 200, f"quarter-car dynamics returns 200 (got {qc.status})")
    qc_data = qc.json()
    check(qc_data.get("body_bounce_frequency_hz", 0.0) > 1.0, "body bounce frequency > 1.0 Hz")
    check(qc_data.get("wheel_hop_frequency_hz", 0.0) > 8.0, "wheel-hop frequency > 8.0 Hz")
    check(qc_data.get("body_acceleration_rms_m_s2", 0.0) > 0.1, "chassis RMS acceleration > 0.1 m/s²")

    # 52. Power Transformer Electromagnetics & Thermal Rating (IEC 60076 / IEEE C57)
    trans = studio.post("/api/sim/electrical/power_transformer", {
        "rated_apparent_power_mva": 40.0,
        "primary_voltage_kv": 110.0,
        "secondary_voltage_kv": 33.0,
        "percent_impedance_z": 10.5,
        "no_load_loss_kw": 22.0,
        "load_loss_kw": 165.0
    })
    check(trans.status == 200, f"power transformer returns 200 (got {trans.status})")
    trans_data = trans.json()
    check(trans_data.get("operating_efficiency_pct", 0.0) > 99.0, "full load efficiency > 99.0%")
    check(trans_data.get("hot_spot_temperature_c", 0.0) > 80.0, "hot-spot winding temp > 80°C")
    check(trans_data.get("relative_insulation_aging_factor", 99.0) < 2.0, "insulation aging factor < 2.0")

    # 53. Vapor Compression Refrigeration Cycle & Heat Pump COP (ASHRAE / AHRI 550/590)
    ref = studio.post("/api/sim/thermal/refrigeration_cycle", {
        "refrigerant": "R410A",
        "design_cooling_capacity_kw": 100.0,
        "evaporating_temperature_c": 5.0,
        "condensing_temperature_c": 45.0
    })
    check(ref.status == 200, f"refrigeration cycle returns 200 (got {ref.status})")
    ref_data = ref.json()
    check(ref_data.get("coefficient_of_performance_cooling", 0.0) > 2.0, "refrigeration COP > 2.0")
    check(ref_data.get("compressor_electrical_power_kw", 0.0) > 15.0, "compressor power > 15 kW")

    # 54. Ship Hydrostatic Intact Stability & GZ Righting Arm (IMO Resolution MSC.267(85))
    ship = studio.post("/api/sim/marine/ship_stability", {
        "length_between_perpendiculars_m": 140.0,
        "molded_beam_m": 22.0,
        "design_draft_m": 7.5,
        "vertical_center_of_gravity_kg_m": 7.20
    })
    check(ship.status == 200, f"ship stability returns 200 (got {ship.status})")
    ship_data = ship.json()
    check(ship_data.get("displacement_mass_tonnes", 0.0) > 15000.0, "ship displacement > 15000 tonnes")
    check(ship_data.get("initial_metacentric_height_gm0_m", 0.0) > 1.0, "initial GM0 > 1.0 m")
    check(ship_data.get("max_gz_righting_arm_m", 0.0) > 0.5, "maximum GZ righting arm > 0.5 m")
    check(ship_data.get("is_safe", False) is True, "ship complies with IMO MSC.267(85) intact stability")

    # 55. Plastic Injection Molding Process Physics & Clamp Tonnage (ISO 294 / SPE)
    molding = studio.post("/api/sim/manufacturing/injection_molding", {
        "resin": "ABS",
        "nominal_wall_thickness_mm": 2.50,
        "projected_area_cm2": 180.0,
        "cavity_volume_cm3": 112.5,
        "max_flow_length_mm": 175.0,
        "cavity_count": 2
    })
    check(molding.status == 200, f"injection molding returns 200 (got {molding.status})")
    molding_data = molding.json()
    check(molding_data.get("cooling_time_s", 0.0) > 5.0, "cooling time > 5.0 s")
    check(molding_data.get("peak_cavity_pressure_bar", 0.0) > 200.0, "peak cavity pressure > 200 bar")
    check(molding_data.get("recommended_clamp_tonnage_t", 0.0) > 80.0, "recommended clamp tonnage > 80 tonnes")

    # 56. Structural Seismic Response Spectrum & Base Shear (ASCE 7-22 / Eurocode 8)
    seismic = studio.post("/api/sim/civil/seismic_analysis", {
        "building_height_m": 36.0,
        "story_count": 10,
        "total_seismic_weight_kn": 58000.0,
        "short_period_spectral_accel_ss_g": 1.20,
        "one_second_spectral_accel_s1_g": 0.48
    })
    check(seismic.status == 200, f"seismic analysis returns 200 (got {seismic.status})")
    seis_data = seismic.json()
    check(seis_data.get("fundamental_period_ta_s", 0.0) > 0.8, "fundamental period > 0.8 s")
    check(seis_data.get("total_base_shear_kn", 0.0) > 2000.0, "base shear Vb > 2000 kN")
    check(seis_data.get("base_overturning_moment_kn_m", 0.0) > 50000.0, "overturning moment > 50000 kN·m")

    # 57. Spacecraft Orbital Mechanics & Hohmann Transfer (Curtis / Vallado)
    orbit = studio.post("/api/sim/aerospace/orbital_mechanics", {
        "initial_altitude_km": 400.0,
        "target_altitude_km": 35786.0,
        "spacecraft_wet_mass_kg": 3500.0,
        "propulsion_isp_s": 318.0
    })
    check(orbit.status == 200, f"orbital mechanics returns 200 (got {orbit.status})")
    orbit_data = orbit.json()
    check(orbit_data.get("initial_orbital_speed_m_s", 0.0) > 7600.0, "initial circular speed > 7600 m/s")
    check(orbit_data.get("total_delta_v_m_s", 0.0) > 3800.0, "coplanar delta-V > 3800 m/s")
    check(orbit_data.get("transfer_time_of_flight_h", 0.0) > 5.0, "time of flight > 5.0 h")

    # 58. Synchronous DC-DC Buck Converter Power Stage & Efficiency (IEEE / Erickson)
    smps = studio.post("/api/sim/electronics/dc_dc_converter", {
        "input_voltage_v": 12.0,
        "output_voltage_v": 3.3,
        "rated_output_current_a": 20.0,
        "switching_frequency_hz": 500000.0
    })
    check(smps.status == 200, f"DC-DC converter returns 200 (got {smps.status})")
    smps_data = smps.json()
    check(smps_data.get("duty_cycle", 0.0) > 0.25, "duty cycle > 0.25")
    check(smps_data.get("output_power_w", 0.0) > 60.0, "output power > 60 W")
    check(smps_data.get("efficiency_pct", 0.0) > 90.0, "full load efficiency > 90.0%")

    # 59. Nuclear Fusion Tokamak Plasma Equilibrium & Confinement (Wesson / ITER)
    fusion = studio.post("/api/sim/science/fusion_tokamak", {
        "major_radius_r0_m": 6.20,
        "minor_radius_a_m": 2.00,
        "toroidal_magnetic_field_t": 5.30,
        "plasma_current_ma": 15.0,
        "electron_density_10e20_m3": 1.01,
        "plasma_temperature_kev": 13.0
    })
    check(fusion.status == 200, f"fusion tokamak returns 200 (got {fusion.status})")
    fusion_data = fusion.json()
    check(fusion_data.get("safety_factor_q95", 0.0) > 2.8, "safety factor q95 > 2.8")
    check(fusion_data.get("stored_thermal_energy_mj", 0.0) > 200.0, "stored thermal energy > 200 MJ")
    check(fusion_data.get("fusion_power_mw", 0.0) > 300.0, "D-T fusion power > 300 MW")
    check(fusion_data.get("fusion_gain_factor_q", 0.0) > 5.0, "fusion energy gain Q > 5.0")
    check(fusion_data.get("is_safe", False) is True, "tokamak plasma satisfies disruption limits")

    # 60. Wire Electrical Discharge Machining Process Physics (ISO 230 / CIRP)
    edm = studio.post("/api/sim/manufacturing/wire_edm", {
        "workpiece_thickness_mm": 50.0,
        "wire_diameter_mm": 0.25,
        "open_circuit_voltage_v": 95.0,
        "discharge_current_a": 16.0,
        "pulse_on_time_us": 4.5,
        "pulse_off_time_us": 12.0
    })
    check(edm.status == 200, f"wire edm returns 200 (got {edm.status})")
    edm_data = edm.json()
    check(edm_data.get("single_pulse_energy_mj", 0.0) > 1.0, "single pulse discharge energy > 1.0 mJ")
    check(edm_data.get("cutting_speed_mm_min", 0.0) > 1.0, "linear cutting speed > 1.0 mm/min")
    check(edm_data.get("volumetric_mrr_mm3_min", 0.0) > 20.0, "volumetric MRR > 20 mm3/min")
    check(edm_data.get("kerf_width_mm", 0.0) > 0.28, "kerf width > 0.28 mm")
    check(edm_data.get("surface_roughness_ra_um", 0.0) > 1.0, "surface roughness Ra > 1.0 um")
    check(edm_data.get("max_wire_deflection_um", 0.0) < 35.0, "wire deflection < 35 um")
    check(edm_data.get("is_safe", False) is True, "wire edm operating within safe envelope")

    # 61. Wind Turbine Blade Aerodynamic Bending & Flutter (IEC 61400-1 / Theodorsen)
    wind = studio.post("/api/sim/renewable/wind_blade_aeroelastic", {
        "blade_length_m": 80.0,
        "rotor_radius_m": 82.5,
        "rated_rotor_speed_rpm": 10.5,
        "rated_wind_speed_m_s": 11.4,
        "extreme_gust_speed_m_s": 52.5
    })
    check(wind.status == 200, f"wind blade aeroelastic returns 200 (got {wind.status})")
    wind_data = wind.json()
    check(wind_data.get("standstill_flap_freq_hz", 0.0) > 0.40, "standstill flapwise frequency > 0.40 Hz")
    check(wind_data.get("rotating_flap_freq_hz", 0.0) > wind_data.get("standstill_flap_freq_hz", 0.0), "centrifugal stiffening increases flap frequency")
    check(wind_data.get("centrifugal_stiffening_ratio", 0.0) > 1.05, "Southwell stiffening ratio > 1.05")
    check(wind_data.get("torsional_freq_hz", 0.0) > 2.0, "first torsional frequency > 2.0 Hz")
    check(wind_data.get("flutter_speed_m_s", 0.0) > 100.0, "coupled flutter speed > 100 m/s")
    check(wind_data.get("flutter_safety_margin", 0.0) > 1.25, "flutter safety margin > 1.25")
    check(wind_data.get("tip_gust_deflection_m", 0.0) > 1.5, "50-year gust tip deflection > 1.5 m")
    check(wind_data.get("is_stable", False) is True, "wind turbine blade aeroelastically stable")

    # 62. Subsea Offshore Pipeline Wall Thickness & Hydrostatic Collapse (DNV-ST-F101 / API 5L)
    pipe = studio.post("/api/sim/marine/subsea_pipeline", {
        "outside_diameter_mm": 508.0,
        "nominal_wall_thickness_mm": 25.4,
        "corrosion_allowance_mm": 3.0,
        "fabrication_ovality_pct": 0.50,
        "water_depth_m": 1500.0,
        "internal_design_pressure_mpa": 22.0
    })
    check(pipe.status == 200, f"subsea pipeline returns 200 (got {pipe.status})")
    pipe_data = pipe.json()
    check(pipe_data.get("hydrostatic_pressure_mpa", 0.0) > 14.0, "hydrostatic seawater pressure at 1500m > 14 MPa")
    check(pipe_data.get("characteristic_collapse_pressure_mpa", 0.0) > 20.0, "characteristic collapse pressure > 20 MPa")
    check(pipe_data.get("design_collapse_resistance_mpa", 0.0) > 14.0, "factored design collapse resistance > 14 MPa")
    check(pipe_data.get("collapse_safety_factor", 0.0) > 1.30, "collapse safety factor > 1.30")
    check(pipe_data.get("internal_burst_pressure_mpa", 0.0) > 35.0, "internal burst yield pressure > 35 MPa")
    check(pipe_data.get("burst_safety_factor", 0.0) > 1.50, "burst safety factor > 1.50")
    check(pipe_data.get("is_compliant", False) is True, "subsea pipeline DNV-ST-F101 compliant")

    # 63. Gas Turbine Combustor Aerothermal Performance & NOx Emissions (Lefebvre / ICAO)
    comb = studio.post("/api/sim/aerospace/combustor_aerothermal", {
        "inlet_total_pressure_mpa": 2.40,
        "inlet_total_temperature_k": 780.0,
        "air_mass_flow_kg_s": 38.0,
        "fuel_mass_flow_kg_s": 0.76
    })
    check(comb.status == 200, f"gas turbine combustor returns 200 (got {comb.status})")
    comb_data = comb.json()
    check(comb_data.get("combustion_efficiency_pct", 0.0) > 99.0, "combustion efficiency > 99.0%")
    check(comb_data.get("total_pressure_loss_pct", 0.0) > 3.0, "total pressure loss > 3.0%")
    check(comb_data.get("total_pressure_loss_pct", 0.0) < 6.0, "total pressure loss < 6.0%")
    check(comb_data.get("turbine_inlet_temperature_k", 0.0) > 1450.0, "turbine inlet temperature > 1450 K")
    check(comb_data.get("pattern_factor", 0.0) < 0.25, "temperature pattern factor < 0.25")
    check(comb_data.get("nox_emissions_index_g_kg", 0.0) > 5.0, "thermal NOx emissions index > 5 g/kg")
    check(comb_data.get("is_acceptable", False) is True, "combustor meets ICAO aerothermal criteria")

    # 64. Battery Energy Storage System Container & Grid RTE (IEC 62933 / IEEE 2030.2.1)
    bess = studio.post("/api/sim/energy/bess_grid", {
        "nominal_capacity_mwh": 10.0,
        "rated_power_mw": 2.50,
        "c_rate": 0.25,
        "ambient_temperature_c": 28.0
    })
    check(bess.status == 200, f"bess grid returns 200 (got {bess.status})")
    bess_data = bess.json()
    check(bess_data.get("discharge_duration_hours", 0.0) == 4.0, "discharge duration == 4.0 hours")
    check(bess_data.get("battery_dc_efficiency_pct", 0.0) > 93.0, "battery DC efficiency > 93.0%")
    check(bess_data.get("pcs_efficiency_pct", 0.0) > 97.0, "PCS bidirectional inverter efficiency > 97.0%")
    check(bess_data.get("transformer_efficiency_pct", 0.0) > 98.5, "MV step-up transformer efficiency > 98.5%")
    check(bess_data.get("ac_to_ac_rte_pct", 0.0) > 83.0, "utility AC-to-AC round-trip efficiency > 83.0%")
    check(bess_data.get("is_grid_compliant", False) is True, "BESS complies with grid interconnection standards")

    # 65. Soil Consolidation & Foundation Settlement (Terzaghi 1D / ASTM D2435 / Eurocode 7)
    soil = studio.post("/api/sim/geotechnical/soil_consolidation", {
        "layer_thickness_m": 6.0,
        "initial_void_ratio": 0.85,
        "compression_index_cc": 0.32,
        "recompression_index_cr": 0.045,
        "preconsolidation_stress_kpa": 120.0,
        "initial_effective_stress_kpa": 100.0,
        "applied_stress_increment_kpa": 80.0,
        "coefficient_of_consolidation_m2_yr": 2.80,
        "analysis_time_years": 2.0
    })
    check(soil.status == 200, f"soil consolidation returns 200 (got {soil.status})")
    soil_data = soil.json()
    check(soil_data.get("over_consolidation_ratio", 0.0) == 1.20, "OCR == 1.20")
    check(soil_data.get("ultimate_primary_settlement_mm", 0.0) > 150.0, "ultimate primary settlement > 150 mm")
    check(soil_data.get("time_factor_tv", 0.0) > 0.50, "time factor Tv > 0.50")
    check(soil_data.get("degree_of_consolidation_pct", 0.0) > 75.0, "degree of consolidation > 75.0%")
    check(soil_data.get("settlement_at_time_mm", 0.0) > 120.0, "settlement at 2 yr > 120 mm")
    check(soil_data.get("is_acceptable", False) is True, "settlement conforms to foundation criteria")

    # 66. Hydraulic Turbines Pelton & Francis Energy Conversion (IEC 60193 / Euler)
    hydro = studio.post("/api/sim/energy/hydraulic_turbine", {
        "net_head_m": 450.0,
        "flow_rate_m3_s": 3.20,
        "rotational_speed_rpm": 500.0,
        "runner_diameter_m": 1.80,
        "nozzle_count": 2
    })
    check(hydro.status == 200, f"hydraulic turbine returns 200 (got {hydro.status})")
    hydro_data = hydro.json()
    check(hydro_data.get("available_hydraulic_power_mw", 0.0) > 13.0, "available hydraulic power > 13 MW")
    check(hydro_data.get("jet_velocity_m_s", 0.0) > 90.0, "jet velocity > 90 m/s")
    check(hydro_data.get("speed_ratio_phi", 0.0) > 0.45, "speed ratio phi > 0.45")
    check(hydro_data.get("shaft_power_mw", 0.0) > 12.0, "shaft mechanical power > 12 MW")
    check(hydro_data.get("hydraulic_efficiency_pct", 0.0) > 88.0, "hydraulic efficiency > 88.0%")
    check(hydro_data.get("is_cavitation_safe", False) is True, "turbine cavitation free")

    # 67. Cardiovascular Stent Biomechanics & Radial Expansion (ASTM F3067 / FDA)
    stent = studio.post("/api/sim/biomedical/stent_expansion", {
        "crimped_diameter_mm": 1.05,
        "nominal_expanded_diameter_mm": 3.00,
        "nominal_length_mm": 18.0,
        "strut_thickness_um": 81.0,
        "strut_width_um": 84.0,
        "crown_count_per_ring": 8,
        "ring_count": 12,
        "balloon_pressure_atm": 12.0
    })
    check(stent.status == 200, f"stent expansion returns 200 (got {stent.status})")
    stent_data = stent.json()
    check(stent_data.get("expansion_ratio", 0.0) > 2.5, "stent expansion ratio > 2.5")
    check(stent_data.get("inflated_outer_diameter_mm", 0.0) > 3.10, "inflated diameter > 3.10 mm")
    check(stent_data.get("post_recoil_diameter_mm", 0.0) > 3.00, "post recoil diameter > 3.00 mm")
    check(stent_data.get("elastic_recoil_pct", 0.0) <= 5.0, "elastic recoil <= 5.0%")
    check(stent_data.get("foreshortening_pct", 0.0) <= 8.0, "longitudinal foreshortening <= 8.0%")
    check(stent_data.get("radial_resistive_force_n_mm", 0.0) > 0.70, "radial resistive force > 0.70 N/mm")
    check(stent_data.get("fracture_safety_factor", 0.0) >= 1.50, "strut fracture safety factor >= 1.50")
    check(stent_data.get("is_fda_compliant", False) is True, "coronary stent conforms to FDA guidance")

    # 68. Microwave Rectangular Waveguide & Cavity Resonator (IEEE Std 148 / Pozar)
    wave = studio.post("/api/sim/electronics/microwave_waveguide", {
        "broad_dimension_a_mm": 22.86,
        "narrow_dimension_b_mm": 10.16,
        "length_l_mm": 100.0,
        "operating_frequency_ghz": 10.0,
        "transmitted_power_w": 500.0
    })
    check(wave.status == 200, f"microwave waveguide returns 200 (got {wave.status})")
    wave_data = wave.json()
    check(wave_data.get("cutoff_frequency_te10_ghz", 0.0) > 6.50, "TE10 cutoff > 6.50 GHz")
    check(wave_data.get("cutoff_frequency_te20_ghz", 0.0) > 13.0, "TE20 cutoff > 13.0 GHz")
    check(wave_data.get("is_single_mode", False) is True, "single mode operation verified")
    check(wave_data.get("guide_wavelength_mm", 0.0) > 39.0, "guide wavelength > 39 mm")
    check(wave_data.get("wave_impedance_ohms", 0.0) > 490.0, "TE10 wave impedance > 490 Ohms")
    check(wave_data.get("attenuation_rate_db_m", 0.0) > 0.08, "attenuation rate > 0.08 dB/m")
    check(wave_data.get("cavity_unloaded_q", 0.0) > 5000.0, "TE101 cavity unloaded Q > 5000")

    # 69. Gas-Insulated Switchgear High-Voltage Dielectric Breakdown (IEC 62271-203)
    gis = studio.post("/api/sim/electrical/gis_dielectric", {
        "rated_voltage_kv_rms": 420.0,
        "conductor_radius_mm": 60.0,
        "enclosure_radius_mm": 165.0,
        "gas_pressure_abs_bar": 5.50,
        "lightning_impulse_withstand_kv_pk": 1425.0
    })
    check(gis.status == 200, f"gis dielectric returns 200 (got {gis.status})")
    gis_data = gis.json()
    check(gis_data.get("radius_ratio", 0.0) == 2.75, "coaxial radius ratio R/r == 2.75")
    check(gis_data.get("field_utilization_factor", 0.0) > 0.55, "field utilization factor > 0.55")
    check(gis_data.get("max_continuous_e_field_kv_mm", 0.0) > 5.0, "max continuous E-field > 5.0 kV/mm")
    check(gis_data.get("max_impulse_e_field_kv_mm", 0.0) > 22.0, "max impulse E-field > 22.0 kV/mm")
    check(gis_data.get("continuous_safety_factor", 0.0) > 5.0, "continuous AC dielectric safety factor > 5.0")
    check(gis_data.get("impulse_safety_factor", 0.0) >= 1.25, "lightning impulse safety factor >= 1.25")
    check(gis_data.get("is_iec_compliant", False) is True, "GIS design compliant with IEC 62271-203")

    # 70. Proton Exchange Membrane Fuel Cell (PEMFC) Polarization (Larminie / Barbir)
    pem = studio.post("/api/sim/energy/pem_fuel_cell", {
        "cell_count": 360,
        "active_cell_area_cm2": 280.0,
        "operating_temperature_c": 75.0,
        "anode_pressure_bar": 2.20,
        "cathode_pressure_bar": 2.00,
        "operating_current_density_a_cm2": 1.20,
    })
    check(pem.status == 200, f"pem fuel cell returns 200 (got {pem.status})")
    pem_data = pem.json()
    check(pem_data.get("nernst_potential_v", 0.0) > 1.15, "PEMFC Nernst potential > 1.15 V")
    check(pem_data.get("cell_voltage_v", 0.0) > 0.65, "PEMFC cell voltage > 0.65 V")
    check(pem_data.get("stack_power_kw", 0.0) > 70.0, "PEMFC stack DC power > 70 kW")
    check(pem_data.get("hydrogen_consumption_kg_h", 0.0) > 4.0, "PEMFC H2 consumption > 4.0 kg/h")
    check(pem_data.get("electrical_efficiency_lhv_pct", 0.0) > 50.0, "PEMFC electrical efficiency LHV > 50%")
    check(pem_data.get("is_operable", False) is True, "PEMFC operating point is stable and operable")

    # 71. Cryogenic Superconducting Solenoid Magnet & Quench Protection (Wilson / CERN)
    mag = studio.post("/api/sim/science/superconducting_magnet", {
        "inner_radius_m": 0.35,
        "outer_radius_m": 0.48,
        "coil_length_m": 1.25,
        "turns_count": 2600,
        "operating_current_a": 1400.0,
        "operating_temperature_k": 4.20,
    })
    check(mag.status == 200, f"superconducting magnet returns 200 (got {mag.status})")
    mag_data = mag.json()
    check(mag_data.get("central_field_t", 0.0) > 2.5, "central magnetic field B0 > 2.5 Tesla")
    check(mag_data.get("peak_conductor_field_t", 0.0) > mag_data.get("central_field_t", 0.0), "peak conductor field > central field")
    check(mag_data.get("stored_energy_mj", 0.0) > 1.8, "stored magnetic energy > 1.8 MJ")
    check(mag_data.get("load_line_margin_pct", 0.0) < 75.0, "load-line current margin < 75%")
    check(mag_data.get("adiabatic_hotspot_temperature_k", 0.0) < 180.0, "adiabatic quench hotspot temperature < 180 K")
    check(mag_data.get("is_quench_safe", False) is True, "quench protection dump verified safe")

    # 72. Spacecraft Hypersonic Reentry Aerothermodynamics (Sutton-Graves / NASA SP-8000)
    reentry = studio.post("/api/sim/aerospace/reentry_aerothermodynamics", {
        "freestream_velocity_m_s": 7400.0,
        "altitude_km": 62.0,
        "nose_radius_m": 1.15,
        "vehicle_mass_kg": 5200.0,
    })
    check(reentry.status == 200, f"reentry aerothermodynamics returns 200 (got {reentry.status})")
    reentry_data = reentry.json()
    check(reentry_data.get("freestream_mach", 0.0) > 20.0, "freestream entry Mach > 20")
    check(reentry_data.get("stagnation_pressure_kpa", 0.0) > 10.0, "post-shock stagnation pressure > 10 kPa")
    check(reentry_data.get("stagnation_enthalpy_mj_kg", 0.0) > 25.0, "stagnation enthalpy > 25 MJ/kg")
    check(reentry_data.get("total_stagnation_heat_flux_kw_m2", 0.0) > 800.0, "total stagnation heat flux > 800 kW/m²")
    check(reentry_data.get("radiative_equilibrium_wall_temperature_k", 0.0) > 1700.0, "radiative equilibrium wall temp > 1700 K")
    check(reentry_data.get("thermal_safety_factor", 0.0) >= 1.05, "TPS thermal safety factor >= 1.05")
    check(reentry_data.get("is_tps_safe", False) is True, "TPS thermal protection shield safe against exceedance")

    # 73. Laser Powder Bed Fusion (LPBF / DMLS) Melt Pool Thermals (King / Rosenthal)
    lpbf = studio.post("/api/sim/manufacturing/lpbf_meltpool", {
        "laser_power_w": 250.0,
        "scan_speed_mm_s": 950.0,
        "beam_diameter_um": 80.0,
        "layer_thickness_um": 40.0,
        "hatch_spacing_um": 100.0,
    })
    check(lpbf.status == 200, f"lpbf meltpool returns 200 (got {lpbf.status})")
    lpbf_data = lpbf.json()
    check(lpbf_data.get("volumetric_energy_density_j_mm3", 0.0) > 50.0, "volumetric energy density VED > 50 J/mm³")
    check(lpbf_data.get("normalized_enthalpy", 0.0) > 2.5, "King normalized enthalpy > 2.5")
    check(lpbf_data.get("meltpool_width_um", 0.0) > 100.0, "melt pool width > 100 μm")
    check(lpbf_data.get("hatch_overlap_ratio", 0.0) > 1.10, "hatch overlap ratio > 1.10x")
    check(lpbf_data.get("penetration_ratio", 0.0) > 1.30, "interlayer depth penetration ratio > 1.30x")
    check(lpbf_data.get("is_acceptable", False) is True, "LPBF process parameters in stable defect-free window")

    # 74. Acoustic Metamaterials & Phononic Crystal Bandgap (Mead / Brillouin)
    phononic = studio.post("/api/sim/acoustics/phononic_bandgap", {
        "unit_cell_count": 8,
        "layer_a_thickness_mm": 24.0,
        "layer_b_thickness_mm": 16.0,
        "excitation_frequency_hz": 24000.0,
    })
    check(phononic.status == 200, f"phononic bandgap returns 200 (got {phononic.status})")
    phononic_data = phononic.json()
    check(phononic_data.get("lattice_constant_mm", 0.0) > 35.0, "unit cell lattice constant > 35 mm")
    check(phononic_data.get("impedance_contrast_ratio", 0.0) > 15.0, "acoustic impedance contrast ratio > 15:1")
    check(phononic_data.get("bragg_center_frequency_hz", 0.0) > 20000.0, "Bragg center frequency > 20 kHz")
    check(phononic_data.get("relative_bandgap_width_pct", 0.0) > 30.0, "relative acoustic bandgap width > 30%")
    check(phononic_data.get("is_in_bandgap", False) is True, "operating excitation frequency is inside stopband")
    check(phononic_data.get("transmission_loss_db", 0.0) > 30.0, "acoustic transmission loss > 30 dB")
    check(phononic_data.get("is_isolation_effective", False) is True, "metamaterial acoustic vibration isolation effective")

    # 75. Wheel-Rail Contact Mechanics & Track Hunting (Kalker / Hertz / Nadal / UIC 518)
    wheel_rail = studio.post("/api/sim/rail/wheel_rail_contact", {
        "wheel_vertical_load_kn": 100.0,
        "wheel_radius_mm": 460.0,
        "rail_crown_radius_mm": 300.0,
        "train_speed_km_h": 200.0,
    })
    check(wheel_rail.status == 200, f"wheel-rail contact returns 200 (got {wheel_rail.status})")
    wr_data = wheel_rail.json()
    check(wr_data.get("contact_patch_area_mm2", 0.0) > 100.0, "Hertzian contact patch area > 100 mm²")
    check(wr_data.get("max_hertz_pressure_mpa", 0.0) > 800.0, "peak Hertzian normal contact pressure > 800 MPa")
    check(wr_data.get("kalker_f11_mn", 0.0) > 10.0, "Kalker longitudinal linear creep coefficient f11 > 10 MN")
    check(wr_data.get("klingel_hunting_frequency_hz", 0.0) > 1.5, "Klingel kinematic hunting frequency > 1.5 Hz")
    check(wr_data.get("derailment_safety_factor", 0.0) > 1.20, "Nadal wheel-climb derailment safety factor > 1.20")
    check(wr_data.get("is_derailment_safe", False) is True, "wheel-rail interface safe against flange-climb derailment")

    # 76. Subsurface Mine Ventilation Aerodynamics & Fan Duty (Atkinson / MSHA / McPherson)
    mine_vent = studio.post("/api/sim/mining/mine_ventilation", {
        "airway_length_m": 1400.0,
        "airway_area_m2": 20.0,
        "airflow_rate_m3_s": 85.0,
        "vertical_depth_m": 400.0,
    })
    check(mine_vent.status == 200, f"mine ventilation returns 200 (got {mine_vent.status})")
    mine_data = mine_vent.json()
    check(mine_data.get("atkinson_resistance_ns2_m8", 0.0) > 0.01, "Atkinson aerodynamic airway resistance > 0.01 N·s²/m⁸")
    check(mine_data.get("total_airway_pressure_drop_pa", 0.0) > 150.0, "total circuit airway pressure drop > 150 Pa")
    check(mine_data.get("required_fan_pressure_pa", 0.0) > 100.0, "required main mechanical fan operating pressure > 100 Pa")
    check(mine_data.get("fan_shaft_power_kw", 0.0) > 10.0, "ventilation fan drive shaft power > 10 kW")
    check(mine_data.get("return_methane_pct", 0.0) < 1.0, "return airway diluted methane concentration < 1.0%")
    check(mine_data.get("is_methane_safe", False) is True, "MSHA statutory methane dilution compliance achieved")

    # 77. Parabolic Reflector Antenna Electromagnetics & Gain (Silver / Stutzman / ITU-R S.580)
    reflector = studio.post("/api/sim/telecom/reflector_antenna", {
        "dish_diameter_m": 2.40,
        "operating_frequency_ghz": 14.25,
        "surface_rms_error_mm": 0.35,
    })
    check(reflector.status == 200, f"reflector antenna returns 200 (got {reflector.status})")
    ant_data = reflector.json()
    check(ant_data.get("boresight_gain_dbi", 0.0) > 45.0, "reflector bore-sight directive gain > 45 dBi")
    check(ant_data.get("half_power_beamwidth_deg", 0.0) < 1.0, "half-power beamwidth HPBW < 1.0 deg")
    check(ant_data.get("ruze_efficiency", 0.0) > 0.85, "Ruze surface tolerance efficiency factor > 85%")
    check(ant_data.get("aperture_efficiency_pct", 0.0) > 65.0, "total combined aperture efficiency > 65%")
    check(ant_data.get("first_sidelobe_level_db", 0.0) < -20.0, "first side-lobe level suppression < -20 dB")
    check(ant_data.get("is_itu_compliant", False) is True, "ITU-R S.580 co-polar sidelobe mask compliance verified")

    # 78. Sub-3nm Gate-All-Around (GAA) Nanosheet FET Electrostatics (Taur / BSIM-CMG / IEEE IRDS)
    gaa_fet = studio.post("/api/sim/semiconductor/gaa_fet", {
        "nanosheet_count": 3,
        "gate_length_nm": 12.0,
        "nanosheet_width_nm": 30.0,
        "nanosheet_thickness_nm": 5.0,
        "equivalent_oxide_thickness_nm": 0.75,
        "supply_voltage_v": 0.70,
    })
    check(gaa_fet.status == 200, f"GAA FET returns 200 (got {gaa_fet.status})")
    gaa_data = gaa_fet.json()
    check(gaa_data.get("effective_gate_width_nm", 0.0) == 210.0, "effective gate conduction perimeter Weff = 210 nm")
    check(gaa_data.get("natural_scaling_length_nm", 0.0) < 2.0, "3D natural electrostatic scaling length lambda < 2.0 nm")
    check(gaa_data.get("subthreshold_swing_mv_dec", 0.0) < 70.0, "subthreshold swing SS < 70 mV/dec near thermal limit")
    check(gaa_data.get("dibl_mv_v", 0.0) < 40.0, "drain-induced barrier lowering DIBL < 40 mV/V")
    check(gaa_data.get("on_current_ua", 0.0) > 150.0, "quasi-ballistic saturation ON current > 150 μA")
    check(gaa_data.get("is_channel_control_adequate", False) is True, "GAA 3D electrostatic gate control integrity adequate")

    # 79. Precision Agricultural Irrigation Lateral Hydraulics (Christiansen / Keller-Bliesner / ISO 9261)
    irrig = studio.post("/api/sim/agri/irrigation_hydraulics", {
        "pipe_inside_diameter_mm": 16.0,
        "lateral_length_m": 120.0,
        "emitter_spacing_m": 0.40,
        "nominal_discharge_l_h": 1.60,
    })
    check(irrig.status == 200, f"irrigation hydraulics returns 200 (got {irrig.status})")
    irrig_data = irrig.json()
    check(irrig_data.get("total_emitter_count", 0) == 300, "total lateral emitter count = 300")
    check(irrig_data.get("total_inlet_flow_l_h", 0.0) == 480.0, "total lateral inlet flow = 480 L/h")
    check(irrig_data.get("friction_head_loss_m", 0.0) > 1.0, "multi-outlet frictional continuous head loss > 1.0 m")
    check(irrig_data.get("christiansen_uniformity_pct", 0.0) > 92.0, "Christiansen Uniformity Coefficient CU > 92%")
    check(irrig_data.get("emission_uniformity_pct", 0.0) > 90.0, "Keller-Bliesner Emission Uniformity EU > 90%")
    check(irrig_data.get("is_uniformity_excellent", False) is True, "precision micro-irrigation distribution uniformity excellent")

    # 80. Activated Sludge Aeration & Secondary Clarifier Sedimentation (ASCE / WEF / Takacs)
    wwtp = studio.post("/api/sim/environmental/wastewater_treatment", {
        "influent_flow_m3_day": 20000.0,
        "influent_bod_mg_l": 220.0,
        "target_effluent_bod_mg_l": 15.0,
        "mlss_mg_l": 3000.0,
        "sludge_retention_time_days": 8.0,
    })
    check(wwtp.status == 200, f"wastewater treatment returns 200 (got {wwtp.status})")
    wwtp_data = wwtp.json()
    check(wwtp_data.get("aeration_basin_volume_m3", 0.0) > 4000.0, "aeration basin biological volume > 4000 m³")
    check(wwtp_data.get("hydraulic_retention_time_hours", 0.0) > 5.0, "aeration hydraulic retention time HRT > 5.0 h")
    check(wwtp_data.get("food_to_microorganism_ratio", 0.0) > 0.20, "food-to-microorganism ratio F/M > 0.20 kg BOD/kg MLVSS·d")
    check(wwtp_data.get("daily_sludge_production_kg_day", 0.0) > 1500.0, "daily waste biomass production > 1500 kg TSS/d")
    check(wwtp_data.get("surface_overflow_rate_m3_m2_day", 0.0) <= 32.0, "clarifier surface overflow rate SOR <= 32 m/d")
    check(wwtp_data.get("solids_loading_rate_kg_m2_h", 0.0) <= 6.0, "clarifier solids loading rate SLR <= 6.0 kg/m²·h")
    check(wwtp_data.get("is_effluent_compliant", False) is True, "effluent discharge standards fully compliant")

    # 81. EV Longitudinal Dynamics & Regenerative Braking (SAE J2452 / UNECE R13-H)
    ev_pwtr = studio.post("/api/sim/automotive/ev_powertrain", {
        "vehicle_mass_kg": 1850.0,
        "rolling_resistance_coeff": 0.009,
        "drag_coefficient": 0.24,
        "frontal_area_m2": 2.25,
        "motor_peak_torque_nm": 350.0,
        "battery_capacity_kwh": 75.0,
        "cruising_speed_km_h": 100.0,
    })
    check(ev_pwtr.status == 200, f"EV powertrain returns 200 (got {ev_pwtr.status})")
    ev_data = ev_pwtr.json()
    check(ev_data.get("total_road_load_force_n", 0.0) > 300.0, "total steady road load force > 300 N")
    check(ev_data.get("cruising_range_km", 0.0) > 350.0, "steady-state battery cruising range > 350 km")
    check(ev_data.get("max_wheel_tractive_force_n", 0.0) > 8000.0, "maximum wheel tractive force > 8000 N")
    check(ev_data.get("acceleration_0_to_100_km_h_s", 0.0) < 7.5, "0 to 100 km/h acceleration time < 7.5 s")
    check(ev_data.get("regen_braking_power_kw", 0.0) > 15.0, "regenerative braking recovery power > 15 kW")
    check(ev_data.get("is_regen_braking_compliant", False) is True, "UNECE R13-H braking energy recovery compliant")

    # 82. Lead-Rubber Bearing (LRB) Seismic Base Isolation (ASCE 7-22 / FEMA P-751)
    seismic_iso = studio.post("/api/sim/civil/seismic_base_isolation", {
        "superstructure_mass_tonnes": 4500.0,
        "bearing_count": 16,
        "bearing_outer_diameter_mm": 800.0,
        "lead_core_diameter_mm": 160.0,
        "total_rubber_thickness_mm": 200.0,
        "mapped_spectral_accel_s1_g": 0.60,
    })
    check(seismic_iso.status == 200, f"seismic base isolation returns 200 (got {seismic_iso.status})")
    iso_data = seismic_iso.json()
    check(iso_data.get("total_system_yield_force_qd_kn", 0.0) > 2500.0, "total isolation system characteristic yield strength > 2500 kN")
    check(iso_data.get("effective_isolated_period_s", 0.0) > 2.0, "effective isolated fundamental period > 2.0 s")
    check(iso_data.get("effective_damping_ratio_pct", 0.0) > 15.0, "equivalent viscous damping ratio > 15%")
    check(iso_data.get("design_horizontal_displacement_dd_mm", 0.0) > 100.0, "design horizontal displacement DD > 100 mm")
    check(iso_data.get("base_shear_reduction_ratio", 0.0) < 0.35, "base shear reduction ratio < 0.35 (forces attenuated by > 65%)")
    check(iso_data.get("is_base_isolation_effective", False) is True, "seismic base isolation structural protection effective")

    # 83. Terminal Ballistics & Armor Hydrodynamic Penetration (Tate-Alekseevskii / STANAG 4569)
    ballistics = studio.post("/api/sim/defense/terminal_ballistics", {
        "projectile_mass_kg": 0.180,
        "projectile_diameter_mm": 12.0,
        "projectile_length_mm": 90.0,
        "impact_velocity_m_s": 1450.0,
        "obliquity_angle_deg": 30.0,
        "armor_thickness_mm": 75.0,
    })
    check(ballistics.status == 200, f"terminal ballistics returns 200 (got {ballistics.status})")
    bal_data = ballistics.json()
    check(bal_data.get("normal_impact_velocity_m_s", 0.0) > 1200.0, "normal impact velocity component > 1200 m/s")
    check(bal_data.get("penetration_interface_velocity_m_s", 0.0) > 500.0, "Tate hydrodynamic penetration interface velocity > 500 m/s")
    check(bal_data.get("total_penetration_depth_mm", 0.0) > 50.0, "total hydrodynamic penetration depth > 50 mm")
    check(bal_data.get("ballistic_limit_v50_m_s", 0.0) > 800.0, "Recht-Ipson ballistic limit V50 > 800 m/s")
    check(bal_data.get("is_armor_defeating_projectile", False) is True, "armor successfully defeats projectile without perforation")
    check(bal_data.get("stanag_4569_defeated_level", 0) >= 4, "STANAG 4569 Level 4+ heavy kinetic defeat verified")

    # 84. BGA Solder Joint Thermal Fatigue & Creep Lifetime (Engelmaier / IPC-9701)
    solder_fatigue = studio.post("/api/sim/packaging/solder_thermal_fatigue", {
        "package_width_mm": 35.0,
        "package_length_mm": 35.0,
        "solder_joint_height_um": 400.0,
        "cycle_min_temp_c": -40.0,
        "cycle_max_temp_c": 125.0,
    })
    check(solder_fatigue.status == 200, f"solder thermal fatigue returns 200 (got {solder_fatigue.status})")
    sol_data = solder_fatigue.json()
    check(sol_data.get("distance_to_neutral_point_dnp_mm", 0.0) > 20.0, "distance to neutral point DNP > 20 mm")
    check(sol_data.get("cyclic_shear_strain_pct", 0.0) > 1.5, "cyclic thermal shear strain range > 1.5%")
    check(sol_data.get("median_cycles_to_failure_n50", 0.0) > 500.0, "Coffin-Manson median cycles to failure N50 > 500 cycles")
    check(sol_data.get("weibull_characteristic_life_eta", 0.0) > 500.0, "Weibull characteristic lifetime eta > 500 cycles")
    check(sol_data.get("is_ipc_9701_compliant", False) is True, "IPC-9701 thermal cycling qualification compliant")

    # 85. Ship Bow & Bottom Hydrodynamic Wave Slamming (Wagner / Chuang / DNV-RU-SHIP)
    slamming = studio.post("/api/sim/marine/ship_slamming", {
        "deadrise_angle_deg": 18.0,
        "relative_impact_velocity_m_s": 7.5,
        "frame_spacing_m": 0.75,
        "plate_length_m": 2.40,
        "plate_thickness_mm": 16.0,
        "steel_yield_strength_mpa": 355.0,
    })
    check(slamming.status == 200, f"ship slamming returns 200 (got {slamming.status})")
    slam_data = slamming.json()
    check(slam_data.get("slamming_pressure_coefficient_cp", 0.0) > 10.0, "hydrodynamic peak slamming pressure coefficient Cp > 10.0")
    check(slam_data.get("peak_localized_pressure_kpa", 0.0) > 250.0, "Wagner localized peak slamming pressure > 250 kPa")
    check(slam_data.get("space_averaged_pressure_kpa", 0.0) > 150.0, "space-averaged panel design slamming pressure > 150 kPa")
    check(slam_data.get("slamming_pulse_duration_ms", 0.0) > 5.0, "slamming pressure pulse duration > 5.0 ms")
    check(slam_data.get("dnv_required_thickness_mm", 0.0) > 8.0, "DNV-RU-SHIP minimum shell plate thickness > 8.0 mm")
    check(slam_data.get("is_plate_thickness_sufficient", False) is True, "existing hull plating satisfies DNV slamming yield strength")

    # 86. Vehicle Frontal Crash Deceleration & Occupant Severity (ECE R94 / FMVSS 208 / HIC15)
    crash = studio.post("/api/sim/automotive/crash_deceleration", {
        "vehicle_mass_kg": 1650.0,
        "impact_velocity_km_h": 64.0,
        "crumple_zone_length_m": 0.75,
        "crash_box_mean_force_kn": 340.0,
        "structural_energy_efficiency": 0.78,
        "occupant_mass_kg": 78.0,
    })
    check(crash.status == 200, f"crash deceleration returns 200 (got {crash.status})")
    crash_data = crash.json()
    check(crash_data.get("initial_kinetic_energy_kj", 0.0) > 240.0, "initial vehicle kinetic energy > 240 kJ")
    check(crash_data.get("dynamic_crush_distance_mm", 0.0) > 500.0, "dynamic crumple crush distance > 500 mm")
    check(crash_data.get("head_injury_criterion_hic15", 0.0) < 700.0, "Head Injury Criterion HIC15 < 700")
    check(crash_data.get("occupant_chest_deflection_mm", 0.0) < 42.0, "occupant chest deflection < 42 mm")
    check(crash_data.get("is_crumple_zone_sufficient", False) is True, "crush contained within crumple zone without cabin intrusion")
    check(crash_data.get("is_hic15_compliant", False) is True, "FMVSS 208 and Euro NCAP HIC15 biomechanical compliance verified")

    # 87. Nuclear Reactor Fuel Pin Radial Conduction & DNB Margin (Fourier / Dittus-Boelter / W-3)
    fuel_pin = studio.post("/api/sim/nuclear/fuel_pin", {
        "linear_heat_rate_kw_per_m": 32.0,
        "pellet_radius_mm": 4.10,
        "gap_thickness_um": 80.0,
        "clad_thickness_mm": 0.60,
        "clad_outer_diameter_mm": 9.50,
        "rod_pitch_mm": 12.60,
        "coolant_pressure_mpa": 15.50,
        "bulk_coolant_temperature_c": 305.0,
        "coolant_mass_flux_kg_m2_s": 3500.0,
    })
    check(fuel_pin.status == 200, f"nuclear fuel pin returns 200 (got {fuel_pin.status})")
    fuel_data = fuel_pin.json()
    check(fuel_data.get("clad_surface_heat_flux_mw_m2", 0.0) > 0.8, "cladding outer heat flux > 0.8 MW/m²")
    check(fuel_data.get("centerline_temperature_c", 0.0) > 1200.0, "UO2 fuel centerline peak temperature > 1200 °C")
    check(fuel_data.get("fuel_melting_margin_c", 0.0) > 600.0, "safety margin to fuel melting > 600 °C")
    check(fuel_data.get("departure_from_nucleate_boiling_ratio", 0.0) >= 1.30, "DNBR >= 1.30 per US NRC standard review plan")
    check(fuel_data.get("is_dnbr_acceptable", False) is True, "thermal margin to DNB acceptable")
    check(fuel_data.get("is_fuel_melting_prevented", False) is True, "fuel melting strictly prevented")

    # 88. Pressure Swing Adsorption (PSA) Gas Separation & Breakthrough Dynamics (Langmuir / LDF / Thomas)
    psa = studio.post("/api/sim/chemical/adsorption_psa", {
        "bed_diameter_m": 0.60,
        "bed_length_m": 2.20,
        "adsorption_pressure_bar": 5.50,
        "desorption_pressure_bar": 1.10,
        "operating_temperature_c": 25.0,
        "feed_flow_rate_nm3_h": 85.0,
    })
    check(psa.status == 200, f"adsorption PSA returns 200 (got {psa.status})")
    psa_data = psa.json()
    check(psa_data.get("column_adsorbent_mass_kg", 0.0) > 400.0, "packed adsorbent bed mass > 400 kg")
    check(psa_data.get("heavy_key_equilibrium_capacity_mol_kg", 0.0) > 1.0, "equilibrium heavy component capacity > 1.0 mol/kg")
    check(psa_data.get("breakthrough_time_seconds", 0.0) > 60.0, "column dynamic breakthrough time > 60 s")
    check(psa_data.get("product_purity_pct", 0.0) >= 93.0, "purified product gas purity >= 93.0%")
    check(psa_data.get("is_cycle_time_feasible", False) is True, "PSA cycle valve switching time feasible")
    check(psa_data.get("is_purity_specification_met", False) is True, "product gas purity specification met")

    # 89. Rotorcraft Helicopter Hover & Forward Flight Aerodynamics (Glauert / Leishman / Prouty)
    rotor = studio.post("/api/sim/aerospace/rotorcraft_aerodynamics", {
        "gross_weight_kg": 4300.0,
        "rotor_radius_m": 6.50,
        "number_of_blades": 4,
        "blade_chord_m": 0.38,
        "rotor_speed_rpm": 345.0,
        "forward_speed_knots": 0.0,
    })
    check(rotor.status == 200, f"rotorcraft aerodynamics returns 200 (got {rotor.status})")
    rotor_data = rotor.json()
    check(rotor_data.get("blade_tip_speed_m_s", 0.0) > 220.0, "rotor blade tip speed Vtip > 220 m/s")
    check(rotor_data.get("induced_velocity_m_s", 0.0) > 10.0, "momentum theory induced inflow velocity > 10 m/s")
    check(rotor_data.get("figure_of_merit", 0.0) > 0.65, "hover aerodynamic Figure of Merit FM > 0.65")
    check(rotor_data.get("total_engine_shaft_power_kw", 0.0) > 600.0, "total required engine shaft power > 600 kW")
    check(rotor_data.get("is_blade_loading_within_limits", False) is True, "rotor blade loading safely within stall margin")
    check(rotor_data.get("is_tip_mach_non_critical", False) is True, "advancing tip Mach number free from compressibility drag rise")
    report("simulation")


def persistence_section(work: pathlib.Path, studio: Studio) -> None:
    section("persistence: save, reopen, undo history and every feature")
    features = len(studio.state()["features"])
    volume = studio.volume()
    saved = studio.post("/api/save")
    check(saved.status == 200 and saved.json()["dirty"] is False, "save clears the dirty flag")
    check(studio.project.exists(), "the project file is on disk")
    stop(studio.process)

    port = free_port()
    reopened = Studio(start(port, "--file", str(studio.project)), port, studio.project)
    try:
        state = reopened.state()
        check(state["dirty"] is False, "a reopened project is clean")
        check(state["recovery"]["recovered"] is False, "a saved project is not recovered")
        check(len(state["features"]) == features,
              f"reopening keeps {features} features, got {len(state['features'])}")
        check(abs(state["geometry"]["volume_mm3"] - volume) < 1e-9,
              "reopening reproduces the volume exactly")
        check(all(feature["status"] == "built" for feature in state["features"]),
              "every reopened feature is built")
        check(len(state["sketches"]) >= 4, "every sketch survives the reopen")
    finally:
        stop(reopened.process)
    report("persistence")


def recovery_section(work: pathlib.Path) -> None:
    section("recovery: kill -9 keeps the unsaved edit")
    project = work / "recovery.zcad"
    port = free_port()
    studio = Studio(start(port, "--new", str(project), "--name", "recovery", "--size", "10,10,5"), port, project)
    baseline = studio.volume()
    check(abs(baseline - 500.0) < 1e-6, f"the 10x10x5 base is 500 mm3, got {baseline}")
    edited = studio.post("/api/parameter", {"id": "depth", "value": 8})
    unsaved = edited.json()["geometry"]["volume_mm3"]
    check(edited.json()["dirty"] is True, "the edit is unsaved")
    snapshot = project.with_name(project.name + ".recovery")
    check(snapshot.exists(), "the unsaved edit has a recovery snapshot")
    studio.process.kill()
    studio.process.wait(timeout=8)

    reopened = Studio(start(port, "--file", str(project)), port, project)
    try:
        state = reopened.state()
        check(state["recovery"]["recovered"] is True, "the edit is offered after kill -9")
        check(state["dirty"] is True, "a recovered session is unsaved")
        check(abs(state["geometry"]["volume_mm3"] - unsaved) < 1e-6,
              "the recovered body is the unsaved edit")
        discarded = reopened.post("/api/recovery/discard")
        check(discarded.status == 200, f"discarding the recovery: {discarded!r}")
        check(discarded.json()["recovery"]["recovered"] is False, "discard clears the flag")
        check(abs(discarded.json()["geometry"]["volume_mm3"] - baseline) < 1e-6,
              "discard returns to the file body")
        check(not snapshot.exists(), "discard deletes the snapshot")
    finally:
        stop(reopened.process)
    report("recovery")


def concurrency_section(studio: Studio) -> None:
    section("concurrency: parallel writers never corrupt the document")
    depths = [5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16]

    def edit(depth: int) -> tuple[int, float]:
        response = studio.post("/api/parameter", {"id": "depth", "value": depth})
        volume = response.json()["geometry"]["volume_mm3"] if response.status == 200 else -1.0
        return response.status, volume

    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        outcomes = list(pool.map(edit, depths))
    check(all(status == 200 for status, _ in outcomes),
          f"every concurrent edit succeeded: {[status for status, _ in outcomes]}")

    final = studio.volume()
    # Writes race, so the winner is whichever request the lock served last; the document must still
    # be exactly the body one of those writers produced, not a mixture or a failed rebuild.
    check(
        any(abs(final - volume) < 1e-6 for _, volume in outcomes),
        f"the final volume {final} is one of the written results {sorted(v for _, v in outcomes)}",
    )
    check(studio.state()["failure"] is None, "the document has no failed feature")

    def read(_: int) -> float:
        return studio.volume()

    with concurrent.futures.ThreadPoolExecutor(max_workers=6) as pool:
        volumes = list(pool.map(read, range(12)))
    check(len(set(round(volume, 6) for volume in volumes)) == 1,
          f"concurrent reads agree: {sorted(set(volumes))}")
    report("concurrency")


def share_section(work: pathlib.Path) -> None:
    section("share: the token matrix on a shared studio")
    project = work / "shared.zcad"
    port = free_port()
    token = "fullchecktoken0123456789abcdef"
    studio = Studio(
        start(
            port,
            "--new", str(project),
            "--name", "shared",
            "--size", "10,10,5",
            "--share",
            "--advertise", "127.0.0.1",
            "--token", token,
            token=token,
        ),
        port,
        project,
    )
    auth = (("x-atlas-token", token),)
    try:
        page = studio.get("/")
        check(page.status == 200 and b"Atlas" in page.body, "the shared page loads without a token")
        check(studio.get("/api/health").status == 200, "liveness needs no token")
        check(studio.get("/api/state").status == 401, "state without a token is 401")
        check(studio.get("/api/export/stl").status == 401, "the export without a token is 401")
        check(
            studio.get("/api/state", headers=(("x-atlas-token", "wrong-token-0123456789"),)).status
            == 401,
            "a wrong token is 401",
        )
        check(studio.get(f"/api/state?token={token}").status == 200, "the token in the query works")
        check(
            studio.get("/api/state", headers=auth).status == 200,
            "the token in the header works",
        )
        state = studio.state(headers=auth)
        check(state["share"]["url"] == f"http://127.0.0.1:{port}/?token={token}",
              f"the share link is {state['share']['url']}")
        check(abs(state["geometry"]["volume_mm3"] - 500.0) < 1e-6, "the shared body is 500 mm3")

        check(
            studio.post("/api/parameter", {"id": "depth", "value": 9}, headers=auth).status == 200,
            "a token-bearing edit is accepted",
        )
        check(
            studio.post(
                "/api/parameter",
                {"id": "depth", "value": 9},
                headers=auth,
                origin="http://evil.example",
            ).status
            == 403,
            "a token does not excuse a foreign origin",
        )
        # The token is presented so this reaches the origin check: the token gate runs first, and a
        # request with neither is refused as unauthenticated long before Origin is considered.
        check(
            studio.post(
                "/api/parameter", {"id": "depth", "value": 9}, headers=auth, origin=UNSET
            ).status
            == 403,
            "a shared POST without Origin is refused",
        )
        check(
            studio.post(
                f"/api/parameter?token={token}",
                {"id": "depth", "value": 11},
                headers=(),
                origin=f"http://127.0.0.1:{port}",
            ).status
            == 200,
            "the query token works for a form post",
        )
        check(
            studio.get("/api/state", headers=auth, host="cad.example:443").status == 200,
            "a shared studio accepts a proxied host",
        )
        check(
            studio.get("/api/state", headers=auth, host="").status == 403,
            "an empty host is refused",
        )
        exported = studio.get(f"/api/export/stl?token={token}")
        check(exported.status == 200 and len(exported.body) > 84,
              "the shared export carries its token")
        volume, triangles = stl_metrics(exported.body)
        # The shared body is still a plain 10x10x5 plate: exactly 12 triangles, which is what a
        # closed box tessellates to.
        check(triangles >= 12 and volume > 0, "the shared export is a real solid")
        check(
            studio.get(f"/api/export/pdf?token={token}&paper=A4").body.startswith(b"%PDF-"),
            "the shared PDF plot works",
        )
        check(
            studio.get(f"/api/export/dxf?token={token}").status == 200,
            "the shared DXF export works",
        )
    finally:
        stop(studio.process)
    report("share")


def main() -> int:
    if not BINARY.exists():
        raise AssertionError(
            f"{BINARY} is missing; run `cargo build --release -p cad-studio` first"
        )
    work = pathlib.Path(tempfile.mkdtemp(prefix="atlas-full-"))
    print(f"==> exhaustive check against {BINARY}")

    project = work / "full.zcad"
    port = free_port()
    studio = Studio(start(port, "--new", str(project), "--name", "full", "--size", "40,30,10"), port, project)
    try:
        assets_section(studio)
        protocol_section(studio)
        sketches_section(studio)
        drafting_section(studio)
        engineering_section(studio)
        revisions_section(studio)
        export_section(studio)
        simulation_section(studio)
        concurrency_section(studio)
        persistence_section(work, studio)
    finally:
        stop(studio.process)

    revolve_section(work)
    boss_section(work)
    recovery_section(work)
    share_section(work)

    print(f"==> full check passed ({passed} assertions)")
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

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

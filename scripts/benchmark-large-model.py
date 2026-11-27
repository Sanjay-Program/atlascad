#!/usr/bin/env python3
"""Large-model benchmark for the AtlasCAD studio.

Builds a watertight faceted STEP fixture (a 100 mm box whose six faces are subdivided into
``n x n`` quads, two triangles each), imports it into a freshly started ``cad-studio`` instance
and measures what an operator actually waits for:

    python3 scripts/benchmark-large-model.py [--binary PATH] [--grid 29] [--json]

``--grid 29`` produces 6 * 29 * 29 * 2 = 10 092 triangles, i.e. just over ten thousand STEP faces
(the reader/writer maps one ADVANCED_FACE to one triangle). The fixture is generated into a
temporary directory and is never written into the repository. Every number printed comes from this
run; nothing is estimated.

Exit status is non-zero when the import or any export fails, or when the imported triangle count
does not match the generated count.
"""

import argparse
import json
import os
import shutil
import statistics
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ORIGIN = "http://127.0.0.1:{port}"


def sample_frames(url, seconds):
    """Ask the headless-Chrome sampler for frame times; a missing browser is a recorded skip."""
    node = shutil.which("node")
    if not node:
        return {"skipped": "node is not installed"}
    script = os.path.join(os.path.dirname(os.path.abspath(__file__)), "frame-sample.mjs")
    try:
        run = subprocess.run(
            [node, script, url, "--seconds", str(seconds), "--json"],
            capture_output=True,
            text=True,
            timeout=seconds + 90,
        )
    except subprocess.TimeoutExpired:
        return {"skipped": "frame sampler timed out"}
    output = run.stdout.strip()
    if run.returncode != 0 or not output.startswith("{"):
        detail = (run.stderr or output or "no output").strip().replace("\n", " ")[:200]
        return {"skipped": detail}
    try:
        return json.loads(output)
    except json.JSONDecodeError as error:
        return {"skipped": f"unparseable sampler output: {error}"}


def free_port():
    import socket

    with socket.socket() as sock:
        sock.bind(("127.0.0.1", 0))
        return sock.getsockname()[1]


def box_surface(size, grid):
    """Triangles of a watertight box surface, outward winding, shared corner coordinates."""
    half = size / 2.0
    # (origin, du, dv, outward normal) for each face, before orientation is checked.
    faces = [
        ((-half, -half, -half), (0, size, 0), (size, 0, 0)),  # z = -half
        ((-half, -half, half), (size, 0, 0), (0, size, 0)),  # z = +half
        ((-half, -half, -half), (size, 0, 0), (0, 0, size)),  # y = -half
        ((-half, half, -half), (0, 0, size), (size, 0, 0)),  # y = +half
        ((-half, -half, -half), (0, size, 0), (0, 0, size)),  # x = -half
        ((half, -half, -half), (0, 0, size), (0, size, 0)),  # x = +half
    ]
    triangles = []

    def cross(a, b):
        return (
            a[1] * b[2] - a[2] * b[1],
            a[2] * b[0] - a[0] * b[2],
            a[0] * b[1] - a[1] * b[0],
        )

    def dot(a, b):
        return a[0] * b[0] + a[1] * b[1] + a[2] * b[2]

    for origin, du, dv in faces:
        normal = cross(du, dv)
        # The intended outward direction is from the box centre towards the face origin.
        outward = (
            origin[0] - 0,
            origin[1] - 0,
            origin[2] - 0,
        )
        if dot(normal, outward) < 0:
            du, dv = dv, du
            normal = cross(du, dv)
        length = max(dot(normal, normal), 1e-30) ** 0.5
        unit_normal = tuple(component / length for component in normal)
        unit_u = tuple(component / size for component in du)

        def point(i, j):
            u = i / grid
            v = j / grid
            return (
                origin[0] + du[0] * u + dv[0] * v,
                origin[1] + du[1] * u + dv[1] * v,
                origin[2] + du[2] * u + dv[2] * v,
            )

        for i in range(grid):
            for j in range(grid):
                p00, p10, p11, p01 = point(i, j), point(i + 1, j), point(i + 1, j + 1), point(i, j + 1)
                triangles.append((p00, p10, p11, unit_normal, unit_u))
                triangles.append((p00, p11, p01, unit_normal, unit_u))
    return triangles


def fmt(value):
    text = f"{value:.6f}"
    return text.rstrip("0").rstrip(".") if "." in text else text


def step_file(triangles, name):
    """AP214 faceted solid in the subset cad-io writes and reads."""
    out = [
        "ISO-10303-21;",
        "HEADER;",
        "FILE_DESCRIPTION(('AtlasCAD large-model benchmark','2;0'),'2;1');",
        f"FILE_NAME('{name}.stp','1970-01-01T00:00:00',('benchmark'),('benchmark'),'benchmark','benchmark','');",
        "FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));",
        "ENDSEC;",
        "DATA;",
        "#1=APPLICATION_CONTEXT('core data for automotive mechanical design processes');",
        "#2=APPLICATION_PROTOCOL_DEFINITION('international standard','config_control_design',1994,#1);",
        "#3=PRODUCT_CONTEXT('',#1,'mechanical');",
        f"#4=PRODUCT('{name}','{name}','',(#3));",
        "#5=PRODUCT_DEFINITION_FORMATION('','',#4);",
        "#6=PRODUCT_DEFINITION_CONTEXT('part definition',#1,'design');",
        "#7=PRODUCT_DEFINITION('design','',#5,#6);",
        "#8=PRODUCT_DEFINITION_SHAPE('','',#7);",
        "#9=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));",
        "#10=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));",
        "#11=(NAMED_UNIT(*)SOLID_ANGLE_UNIT()SI_UNIT($,.STERADIAN.));",
        "#12=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-6),#9,'distance_accuracy_value','');",
        "#13=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#12))"
        "GLOBAL_UNIT_ASSIGNED_CONTEXT((#9,#10,#11))REPRESENTATION_CONTEXT('',''));",
    ]
    next_id = 14
    faces = []
    for p0, p1, p2, normal, reference in triangles:
        # Three points, three vertices, a poly loop, its bound, two directions, a placement,
        # a plane and the face itself: 12 entities per triangle.
        point_ids = []
        vertex_ids = []
        for point in (p0, p1, p2):
            out.append(
                f"#{next_id}=CARTESIAN_POINT('',({fmt(point[0])},{fmt(point[1])},{fmt(point[2])}));"
            )
            point_ids.append(next_id)
            next_id += 1
            out.append(f"#{next_id}=VERTEX_POINT('',#{point_ids[-1]});")
            vertex_ids.append(next_id)
            next_id += 1
        out.append(
            f"#{next_id}=POLY_LOOP('',(#{vertex_ids[0]},#{vertex_ids[1]},#{vertex_ids[2]}));"
        )
        loop_id = next_id
        next_id += 1
        out.append(f"#{next_id}=FACE_OUTER_BOUND('',#{loop_id},.T.);")
        bound_id = next_id
        next_id += 1
        out.append(f"#{next_id}=DIRECTION('',({fmt(normal[0])},{fmt(normal[1])},{fmt(normal[2])}));")
        normal_id = next_id
        next_id += 1
        out.append(
            f"#{next_id}=DIRECTION('',({fmt(reference[0])},{fmt(reference[1])},{fmt(reference[2])}));"
        )
        reference_id = next_id
        next_id += 1
        out.append(
            f"#{next_id}=AXIS2_PLACEMENT_3D('',#{point_ids[0]},#{normal_id},#{reference_id});"
        )
        placement_id = next_id
        next_id += 1
        out.append(f"#{next_id}=PLANE('',#{placement_id});")
        plane_id = next_id
        next_id += 1
        out.append(f"#{next_id}=ADVANCED_FACE('',(#{bound_id}),#{plane_id},.T.);")
        faces.append(next_id)
        next_id += 1

    joined = ",".join(f"#{face}" for face in faces)
    out.append(f"#{next_id}=CLOSED_SHELL('',({joined}));")
    shell_id = next_id
    next_id += 1
    out.append(f"#{next_id}=MANIFOLD_SOLID_BREP('{name}',#{shell_id});")
    brep_id = next_id
    next_id += 1
    out.append(f"#{next_id}=ADVANCED_BREP_SHAPE_REPRESENTATION('{name}',(#{brep_id}),#13);")
    representation_id = next_id
    next_id += 1
    out.append(f"#{next_id}=SHAPE_DEFINITION_REPRESENTATION(#8,#{representation_id});")
    out.extend(["ENDSEC;", "END-ISO-10303-21;", ""])
    return "\n".join(out).encode()


class Studio:
    def __init__(self, binary, workdir):
        self.port = free_port()
        self.base = ORIGIN.format(port=self.port)
        log_path = os.path.join(workdir, "benchmark-server.log")
        self.log = open(log_path, "wb")
        self.process = subprocess.Popen(
            [
                binary,
                "--new",
                os.path.join(workdir, "large.zcad"),
                "--name",
                "large",
                "--size",
                "100,100,100",
                "--port",
                str(self.port),
            ],
            stdout=self.log,
            stderr=subprocess.STDOUT,
        )
        for _ in range(400):
            if self.call("/api/health", timeout=0.5)[0]:
                return
            time.sleep(0.05)
        raise SystemExit("the studio did not start; see the log in the temporary directory")

    def headers(self):
        return {"origin": self.base, "Content-Type": "application/json"}

    def call(self, path, data=None, timeout=120):
        request = urllib.request.Request(
            self.base + path,
            data=data,
            headers=self.headers(),
            method="POST" if data is not None else "GET",
        )
        started = time.perf_counter()
        try:
            with urllib.request.urlopen(request, timeout=timeout) as response:
                body = response.read()
                return response.status, body, time.perf_counter() - started
        except urllib.error.HTTPError as error:
            return error.code, error.read(), time.perf_counter() - started
        except Exception as error:  # noqa: BLE001 - surfaced to the operator below
            return 0, str(error).encode(), time.perf_counter() - started

    def rss_kb(self):
        try:
            with open(f"/proc/{self.process.pid}/status") as status:
                for line in status:
                    if line.startswith("VmRSS:"):
                        return int(line.split()[1])
        except OSError:
            pass
        return None

    def close(self):
        self.process.terminate()
        try:
            self.process.wait(timeout=10)
        except subprocess.TimeoutExpired:
            self.process.kill()
        self.log.close()


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--binary", default=None)
    parser.add_argument("--grid", type=int, default=29)
    parser.add_argument("--json", action="store_true")
    parser.add_argument(
        "--frames-seconds",
        type=float,
        default=5.0,
        help="headless frame-sampling window; 0 disables it",
    )
    args = parser.parse_args()

    if args.grid < 1:
        raise SystemExit("--grid must be positive")
    if not args.binary:
        for candidate in ("target/release/cad-studio", "target/debug/cad-studio"):
            if os.path.exists(candidate):
                args.binary = candidate
                break
        else:
            raise SystemExit("build cad-studio first, or pass --binary")

    triangles = box_surface(100.0, args.grid)
    payload = step_file(triangles, "large_model")
    expected = len(triangles)

    with tempfile.TemporaryDirectory(prefix="atlas-bench-") as workdir:
        studio = Studio(args.binary, workdir)
        try:
            rss_idle = studio.rss_kb()
            # The new project already contains one extruded box; the STL check compares the delta so
            # it does not depend on how many triangles that seed body happens to have.
            status, base_stl, _ = studio.call("/api/export/stl", timeout=120)
            if status != 200:
                print(f"baseline STL export failed: HTTP {status}")
                return 1
            base_triangles = max((len(base_stl) - 84) // 50, 0)
            status, body, import_seconds = studio.call(
                "/api/import/step",
                data=json.dumps({"text": payload.decode(), "name": "large_model"}).encode(),
                timeout=300,
            )
            if status != 200:
                print(f"import failed: HTTP {status} {body[:400].decode(errors='replace')}")
                return 1

            state_times = []
            state_bytes = 0
            for _ in range(10):
                status, body, seconds = studio.call("/api/state")
                if status != 200:
                    print(f"/api/state failed: HTTP {status}")
                    return 1
                state_times.append(seconds)
                state_bytes = len(body)

            status, stl, stl_seconds = studio.call("/api/export/stl", timeout=120)
            if status != 200:
                print(f"STL export failed: HTTP {status} {stl[:200].decode(errors='replace')}")
                return 1
            stl_triangles = max((len(stl) - 84) // 50, 0)
            imported_triangles = stl_triangles - base_triangles
            rss_loaded = studio.rss_kb()

            frame_sample = (
                sample_frames(studio.base, args.frames_seconds)
                if args.frames_seconds > 0
                else {"skipped": "disabled by --frames-seconds 0"}
            )

            result = {
                "fixture": f"100 mm box surface, {args.grid}x{args.grid} quads per face",
                "generated_triangles": expected,
                "generated_step_bytes": len(payload),
                "import_seconds": round(import_seconds, 4),
                "state_median_ms": round(statistics.median(state_times) * 1000, 2),
                "state_payload_bytes": state_bytes,
                "stl_seconds": round(stl_seconds, 4),
                "stl_bytes": len(stl),
                "stl_triangles": stl_triangles,
                "stl_base_triangles": base_triangles,
                "imported_triangles": imported_triangles,
                "rss_idle_kb": rss_idle,
                "rss_loaded_kb": rss_loaded,
                "binary": args.binary,
                "frame_sample": frame_sample,
            }
            if args.json:
                print(json.dumps(result, indent=2))
            else:
                print("large-model benchmark")
                for key, value in result.items():
                    print(f"  {key}: {value}")

            if imported_triangles != expected:
                print(
                    f"FAIL: STL adds {imported_triangles} triangles, expected {expected} "
                    f"(total {stl_triangles}, baseline {base_triangles})"
                )
                return 1
            return 0
        finally:
            studio.close()


if __name__ == "__main__":
    sys.exit(main())

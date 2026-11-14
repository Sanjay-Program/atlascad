#!/usr/bin/env python3
"""
AtlasCAD — End-to-End Web Plane Creation & Multi-Disciplinary Browser Test Harness
Spawns the compiled cad-studio release binary, executes browser-airplane-demo.mjs via CDP,
and validates full web interactivity, new plane creation, 3D feature extrusion,
aerodynamics, aeroelasticity, elastoplasticity, 5-axis CAM, 3D slicing, and 2D drafting.
"""

import os
import shutil
import socket
import subprocess
import sys
import time

def find_free_port():
    with socket.socket() as s:
        s.bind(("127.0.0.1", 0))
        return s.getsockname()[1]

def main():
    port = find_free_port()
    work_dir = "/tmp/atlas-plane-web-test"
    shutil.rmtree(work_dir, ignore_errors=True)
    os.makedirs(work_dir, exist_ok=True)
    zcad_file = os.path.join(work_dir, "airplane_project.zcad")

    # Start live release server
    cmd = [
        "target/release/cad-studio",
        "--new", zcad_file,
        "--name", "airplane_demo",
        "--size", "20,10,4",
        "--port", str(port)
    ]
    server = subprocess.Popen(cmd, stdout=subprocess.PIPE, stderr=subprocess.PIPE)

    try:
        # Wait for server to come online
        online = False
        import urllib.request
        for _ in range(60):
            try:
                with urllib.request.urlopen(f"http://127.0.0.1:{port}/api/health", timeout=1) as resp:
                    if resp.status == 200:
                        online = True
                        break
            except Exception:
                time.sleep(0.08)

        if not online:
            print("ERROR: cad-studio server failed to start", file=sys.stderr)
            sys.exit(1)

        print(f"[TEST HARNESS] Studio server running on http://127.0.0.1:{port}")

        # Execute headless Chrome browser test
        node_cmd = ["node", "scripts/browser-airplane-demo.mjs", f"http://127.0.0.1:{port}"]
        res = subprocess.run(node_cmd)
        if res.returncode != 0:
            print(f"ERROR: browser-airplane-demo.mjs failed with exit code {res.returncode}", file=sys.stderr)
            sys.exit(res.returncode)

    finally:
        server.terminate()
        try:
            server.wait(timeout=3)
        except subprocess.TimeoutExpired:
            server.kill()
        shutil.rmtree(work_dir, ignore_errors=True)

if __name__ == "__main__":
    main()

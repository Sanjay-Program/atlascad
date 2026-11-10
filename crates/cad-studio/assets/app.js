"use strict";

(() => {
  const byId = (id) => document.getElementById(id);
  const UNIT_MM = { millimeter: 1, centimeter: 10, meter: 1000, inch: 25.4, foot: 304.8 };

  const app = {
    project: null,
    busy: false,
    firstFit: true,
    pick: null,
    theme: "dark",
    viewport: null,
    parameterInputs: new Map(),
    palette: { open: false, index: 0, items: [] },
    helpOpen: false,
    toastTimer: 0,
    selected: null,
    lastFailureKey: null,
    lastRecoveryKey: null,
    // Set while the user is drawing a closed polyline on the z = 0 plane. Points are in document
    // units and in absolute coordinates; the created sketch uses origin [0, 0].
    draw: null,
  };

  // ---------------------------------------------------------------------------------------
  // Formatting and DOM helpers
  // ---------------------------------------------------------------------------------------

  const numberFormat = new Intl.NumberFormat(undefined, { maximumFractionDigits: 2 });

  function formatNumber(value, digits = 2) {
    if (!Number.isFinite(value)) return "—";
    if (Math.abs(value) >= 1000) return numberFormat.format(value);
    return Number(value.toFixed(digits)).toString();
  }

  function formatValue(value) {
    if (Math.abs(value) >= 1000 || Math.abs(value) < 0.01) return formatNumber(value, 3);
    return formatNumber(value, 2);
  }

  function element(tag, className, text) {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text !== undefined) node.textContent = text;
    return node;
  }

  function setStatus(message, tone = "") {
    const status = byId("statusMessage");
    status.textContent = message;
    status.className = `status-message${tone ? ` ${tone}` : ""}`;
  }

  function toast(message, isError = false) {
    const node = byId("toast");
    node.textContent = message;
    node.classList.toggle("error", isError);
    node.hidden = false;
    if (app.toastTimer) window.clearTimeout(app.toastTimer);
    app.toastTimer = window.setTimeout(() => {
      node.hidden = true;
      app.toastTimer = 0;
    }, isError ? 5200 : 2600);
  }

  function log(message, kind = "info") {
    const list = byId("log");
    const now = new Date();
    const time = now.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
    const row = element("li", kind === "error" ? "error" : "");
    row.append(element("time", "", time), element("span", "", message));
    list.prepend(row);
    while (list.children.length > 60) list.lastElementChild.remove();
  }

  function documentScale() {
    if (!app.project) return 1;
    return UNIT_MM[app.project.unit] || 1;
  }

  // ---------------------------------------------------------------------------------------
  // API
  // ---------------------------------------------------------------------------------------

  class ApiError extends Error {
    constructor(message, kind, status) {
      super(message);
      this.name = "ApiError";
      this.kind = kind;
      this.status = status;
    }
  }

  // A shared studio hands out a tokenised link. The page keeps the token and presents it as a
  // header; it is never copied into another URL or into the activity log.
  const shareToken = (() => {
    const token = new URLSearchParams(window.location.search).get("token");
    return token && token.length >= 16 ? token : null;
  })();

  async function api(path, options = {}) {
    const headers = new Headers(options.headers || {});
    if (shareToken) headers.set("x-atlas-token", shareToken);
    let response;
    try {
      response = await fetch(path, { ...options, headers });
    } catch (error) {
      throw new ApiError("The studio server is not reachable.", "offline", 0);
    }
    const contentType = response.headers.get("content-type") || "";
    let payload = null;
    if (contentType.includes("application/json")) {
      payload = await response.json().catch(() => null);
    }
    if (!response.ok) {
      const message = payload && payload.error && payload.error.message
        ? payload.error.message
        : `Request failed with status ${response.status}.`;
      throw new ApiError(message, payload && payload.error ? payload.error.kind : "http", response.status);
    }
    return payload;
  }

  // ---------------------------------------------------------------------------------------
  // WebGL viewport
  // ---------------------------------------------------------------------------------------

  const MESH_VERTEX = `
    attribute vec3 aPosition;
    attribute vec3 aNormal;
    uniform mat4 uViewProjection;
    varying vec3 vNormal;
    varying vec3 vWorld;
    void main() {
      vWorld = aPosition;
      vNormal = aNormal;
      gl_Position = uViewProjection * vec4(aPosition, 1.0);
    }
  `;

  const MESH_FRAGMENT = `
    precision mediump float;
    varying vec3 vNormal;
    varying vec3 vWorld;
    uniform vec3 uColor;
    uniform vec3 uEye;
    void main() {
      vec3 n = normalize(vNormal);
      if (!gl_FrontFacing) n = -n;
      vec3 key = normalize(vec3(0.42, 0.58, 0.88));
      vec3 fill = normalize(vec3(-0.65, -0.35, 0.32));
      float diffuse = max(dot(n, key), 0.0);
      float bounce = max(dot(n, fill), 0.0) * 0.22;
      vec3 viewDir = normalize(uEye - vWorld);
      float rim = pow(1.0 - max(dot(n, viewDir), 0.0), 2.4) * 0.16;
      vec3 color = uColor * (0.34 + 0.70 * diffuse) + vec3(bounce) + vec3(rim);
      gl_FragColor = vec4(color, 1.0);
    }
  `;

  const LINE_VERTEX = `
    attribute vec3 aPosition;
    attribute vec3 aColor;
    uniform mat4 uViewProjection;
    uniform float uPointSize;
    varying vec3 vColor;
    void main() {
      vColor = aColor;
      gl_Position = uViewProjection * vec4(aPosition, 1.0);
      gl_PointSize = uPointSize;
    }
  `;

  const LINE_FRAGMENT = `
    precision mediump float;
    varying vec3 vColor;
    uniform float uAlpha;
    void main() {
      gl_FragColor = vec4(vColor, uAlpha);
    }
  `;

  function compileShader(gl, type, source) {
    const shader = gl.createShader(type);
    if (!shader) throw new Error("WebGL could not allocate a shader.");
    gl.shaderSource(shader, source);
    gl.compileShader(shader);
    if (!gl.getShaderParameter(shader, gl.COMPILE_STATUS)) {
      const info = gl.getShaderInfoLog(shader) || "unknown shader error";
      gl.deleteShader(shader);
      throw new Error(info);
    }
    return shader;
  }

  function buildProgram(gl, vertexSource, fragmentSource) {
    const vertex = compileShader(gl, gl.VERTEX_SHADER, vertexSource);
    const fragment = compileShader(gl, gl.FRAGMENT_SHADER, fragmentSource);
    const program = gl.createProgram();
    if (!program) throw new Error("WebGL could not allocate a program.");
    gl.attachShader(program, vertex);
    gl.attachShader(program, fragment);
    gl.linkProgram(program);
    gl.deleteShader(vertex);
    gl.deleteShader(fragment);
    if (!gl.getProgramParameter(program, gl.LINK_STATUS)) {
      const info = gl.getProgramInfoLog(program) || "unknown link error";
      gl.deleteProgram(program);
      throw new Error(info);
    }
    return program;
  }

  function perspective(fovRadians, aspect, near, far) {
    const f = 1 / Math.tan(fovRadians / 2);
    const out = new Float32Array(16);
    out[0] = f / aspect;
    out[5] = f;
    out[10] = (far + near) / (near - far);
    out[11] = -1;
    out[14] = (2 * far * near) / (near - far);
    return out;
  }

  function subtract(a, b) {
    return [a[0] - b[0], a[1] - b[1], a[2] - b[2]];
  }

  function add(a, b) {
    return [a[0] + b[0], a[1] + b[1], a[2] + b[2]];
  }

  function scale(a, factor) {
    return [a[0] * factor, a[1] * factor, a[2] * factor];
  }

  function dot(a, b) {
    return a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
  }

  function cross(a, b) {
    return [
      a[1] * b[2] - a[2] * b[1],
      a[2] * b[0] - a[0] * b[2],
      a[0] * b[1] - a[1] * b[0],
    ];
  }

  function lengthOf(a) {
    return Math.sqrt(dot(a, a));
  }

  function normalize(a) {
    const length = lengthOf(a);
    if (!Number.isFinite(length) || length < 1e-12) return [0, 0, 1];
    return [a[0] / length, a[1] / length, a[2] / length];
  }

  function lookAt(eye, center, up) {
    const z = normalize(subtract(eye, center));
    const x = normalize(cross(up, z));
    const y = cross(z, x);
    const out = new Float32Array(16);
    out[0] = x[0]; out[1] = y[0]; out[2] = z[0]; out[3] = 0;
    out[4] = x[1]; out[5] = y[1]; out[6] = z[1]; out[7] = 0;
    out[8] = x[2]; out[9] = y[2]; out[10] = z[2]; out[11] = 0;
    out[12] = -dot(x, eye);
    out[13] = -dot(y, eye);
    out[14] = -dot(z, eye);
    out[15] = 1;
    return out;
  }

  function multiply(a, b) {
    const out = new Float32Array(16);
    for (let column = 0; column < 4; column += 1) {
      for (let row = 0; row < 4; row += 1) {
        let value = 0;
        for (let k = 0; k < 4; k += 1) {
          value += a[k * 4 + row] * b[column * 4 + k];
        }
        out[column * 4 + row] = value;
      }
    }
    return out;
  }

  function faceNormal(a, b, c) {
    return normalize(cross(subtract(b, a), subtract(c, a)));
  }

  function niceStep(target) {
    if (!Number.isFinite(target) || target <= 0) return 1;
    const power = Math.pow(10, Math.floor(Math.log10(target)));
    const normalized = target / power;
    let multiplier = 1;
    if (normalized >= 5) multiplier = 5;
    else if (normalized >= 2) multiplier = 2;
    return multiplier * power;
  }

  class Viewport {
    constructor(canvas) {
      this.canvas = canvas;
      this.gl = canvas.getContext("webgl", { antialias: true, alpha: false, depth: true });
      this.camera = { yaw: -2.32, pitch: 0.52, distance: 140, target: [0, 0, 0] };
      this.bounds = null;
      this.radius = 50;
      this.wireframe = false;
      this.grid = true;
      this.mesh = null;
      this.outline = null;
      this.pick = null;
      this.frameHandle = 0;
      this.pointer = null;
      this.dragDistance = 0;
      this.buffers = {};
      this.counts = { mesh: 0, scene: 0, wire: 0, marker: 0 };
      this.locations = {};
      this.ready = false;
      this.fallback = null;
      if (!this.gl) {
        this.showFallback("This browser or GPU does not provide WebGL, so the 3D viewport cannot render. The model still saves and exports correctly.");
        return;
      }
      try {
        this.initialize();
        this.ready = true;
        this.canvas.dataset.webgl = "ready";
      } catch (error) {
        this.showFallback(`The viewport could not start: ${error.message}`);
      }
      this.bindEvents();
    }

    showFallback(message) {
      this.fallback = message;
      this.canvas.dataset.webgl = "unavailable";
      const notice = byId("viewportNotice");
      notice.hidden = false;
      notice.innerHTML = "";
      notice.append(
        element("h3", "", "Viewport unavailable"),
        element("p", "", message)
      );
    }

    initialize() {
      const gl = this.gl;
      this.meshProgram = buildProgram(gl, MESH_VERTEX, MESH_FRAGMENT);
      this.lineProgram = buildProgram(gl, LINE_VERTEX, LINE_FRAGMENT);
      this.locations.meshPosition = gl.getAttribLocation(this.meshProgram, "aPosition");
      this.locations.meshNormal = gl.getAttribLocation(this.meshProgram, "aNormal");
      this.locations.meshViewProjection = gl.getUniformLocation(this.meshProgram, "uViewProjection");
      this.locations.meshColor = gl.getUniformLocation(this.meshProgram, "uColor");
      this.locations.meshEye = gl.getUniformLocation(this.meshProgram, "uEye");
      this.locations.linePosition = gl.getAttribLocation(this.lineProgram, "aPosition");
      this.locations.lineColor = gl.getAttribLocation(this.lineProgram, "aColor");
      this.locations.lineViewProjection = gl.getUniformLocation(this.lineProgram, "uViewProjection");
      this.locations.lineAlpha = gl.getUniformLocation(this.lineProgram, "uAlpha");
      this.locations.linePointSize = gl.getUniformLocation(this.lineProgram, "uPointSize");
      this.buffers.meshPosition = gl.createBuffer();
      this.buffers.meshNormal = gl.createBuffer();
      this.buffers.scene = gl.createBuffer();
      this.buffers.wire = gl.createBuffer();
      this.buffers.marker = gl.createBuffer();
      gl.enable(gl.DEPTH_TEST);
      gl.depthFunc(gl.LEQUAL);
      gl.enable(gl.BLEND);
      gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);
      gl.disable(gl.CULL_FACE);
    }

    bindEvents() {
      const canvas = this.canvas;
      canvas.addEventListener("contextmenu", (event) => event.preventDefault());
      canvas.addEventListener("pointerdown", (event) => {
        if (!this.ready) return;
        canvas.setPointerCapture(event.pointerId);
        this.pointer = {
          id: event.pointerId,
          x: event.clientX,
          y: event.clientY,
          pan: event.button === 2 || event.shiftKey,
          moved: 0,
        };
        this.dragDistance = 0;
        canvas.focus({ preventScroll: true });
      });
      canvas.addEventListener("pointermove", (event) => {
        if (!this.ready) return;
        if (!this.pointer || this.pointer.id !== event.pointerId) {
          try {
            const pt = this.groundAt(event.clientX, event.clientY);
            if (pt) {
              const coordsEl = byId("statusCoords");
              if (coordsEl) coordsEl.textContent = `X: ${pt[0].toFixed(3)}, Y: ${pt[1].toFixed(3)}, Z: 0.000`;
            }
          } catch (_) {}
          return;
        }
        const dx = event.clientX - this.pointer.x;
        const dy = event.clientY - this.pointer.y;
        this.pointer.x = event.clientX;
        this.pointer.y = event.clientY;
        this.dragDistance += Math.abs(dx) + Math.abs(dy);
        if (this.dragDistance < 2) return;
        if (this.pointer.pan) this.pan(dx, dy);
        else this.orbit(dx, dy);
        this.requestFrame();
      });
      canvas.addEventListener("pointerup", (event) => {
        if (!this.pointer || this.pointer.id !== event.pointerId) return;
        const wasClick = this.dragDistance < 4 && event.button === 0;
        this.pointer = null;
        if (!wasClick) return;
        // While drawing, a click places a point instead of inspecting a face.
        if (app.draw) addDrawPoint(event.clientX, event.clientY);
        else this.pickAt(event.clientX, event.clientY);
      });
      canvas.addEventListener("pointercancel", () => {
        this.pointer = null;
      });
      canvas.addEventListener("wheel", (event) => {
        if (!this.ready) return;
        event.preventDefault();
        const factor = Math.exp(event.deltaY * 0.0012);
        const minimum = Math.max(this.radius * 0.35, 0.05);
        const maximum = Math.max(this.radius * 80, 10000);
        this.camera.distance = Math.min(maximum, Math.max(minimum, this.camera.distance * factor));
        this.requestFrame();
      }, { passive: false });
      canvas.addEventListener("dblclick", () => this.fit());
      canvas.addEventListener("keydown", (event) => {
        if (event.key === "0") {
          event.preventDefault();
          this.fit();
        }
      });
      window.addEventListener("resize", () => this.requestFrame());
      if (typeof ResizeObserver === "function") {
        this.resizeObserver = new ResizeObserver(() => this.requestFrame());
        this.resizeObserver.observe(canvas.parentElement || canvas);
      }
    }

    orbit(dx, dy) {
      this.camera.yaw -= dx * 0.0075;
      this.camera.pitch = Math.max(-1.42, Math.min(1.42, this.camera.pitch + dy * 0.0075));
    }

    pan(dx, dy) {
      const forward = normalize(subtract(this.camera.target, this.eye()));
      let right = cross(forward, [0, 0, 1]);
      if (lengthOf(right) < 0.001) right = [0, 1, 0];
      right = normalize(right);
      const up = normalize(cross(right, forward));
      const scaleFactor = this.camera.distance * 0.0014;
      this.camera.target = add(
        this.camera.target,
        add(scale(right, -dx * scaleFactor), scale(up, dy * scaleFactor))
      );
      this.requestFrame();
    }

    eye() {
      const { yaw, pitch, distance, target } = this.camera;
      const cp = Math.cos(pitch);
      return [
        target[0] + distance * cp * Math.cos(yaw),
        target[1] + distance * cp * Math.sin(yaw),
        target[2] + distance * Math.sin(pitch),
      ];
    }

    setView(name) {
      if (name === "front") {
        this.camera.yaw = -Math.PI / 2;
        this.camera.pitch = 0;
      } else if (name === "top") {
        this.camera.yaw = -Math.PI / 2;
        this.camera.pitch = 1.42;
      } else if (name === "right") {
        this.camera.yaw = 0;
        this.camera.pitch = 0;
      } else {
        this.camera.yaw = -2.32;
        this.camera.pitch = 0.52;
      }
      this.requestFrame();
    }

    setModel(project) {
      const previousCenter = [...this.camera.target];
      const previousRadius = this.radius;
      this.outline = project.outline || null;
      this.mesh = project.mesh && project.mesh.triangles.length ? project.mesh : null;
      this.bounds = project.bounds || null;
      if (this.bounds) {
        const size = subtract(this.bounds.max, this.bounds.min);
        this.radius = Math.max(lengthOf(size) / 2, 0.5);
      } else {
        this.radius = 50;
      }
      if (this.ready) this.uploadGeometry();
      if (this.mesh) {
        const center = this.bounds
          ? [
            (this.bounds.min[0] + this.bounds.max[0]) / 2,
            (this.bounds.min[1] + this.bounds.max[1]) / 2,
            (this.bounds.min[2] + this.bounds.max[2]) / 2,
          ]
          : null;
        const moved = center ? lengthOf(subtract(center, previousCenter)) : 0;
        const resized = previousRadius > 0
          ? this.radius / previousRadius > 2.2 || this.radius / previousRadius < 0.45
          : true;
        if (app.firstFit || moved > Math.max(this.radius * 0.55, previousRadius * 0.55, 1) || resized) {
          app.firstFit = false;
          this.fit();
        }
      }
      this.requestFrame();
    }

    uploadGeometry() {
      const gl = this.gl;
      const mesh = this.mesh;
      if (mesh) {
        const positions = [];
        const normals = [];
        const wire = [];
        for (const triangle of mesh.triangles) {
          const a = mesh.positions[triangle[0]];
          const b = mesh.positions[triangle[1]];
          const c = mesh.positions[triangle[2]];
          if (!a || !b || !c) continue;
          const normal = faceNormal(a, b, c);
          positions.push(...a, ...b, ...c);
          normals.push(...normal, ...normal, ...normal);
          wire.push(...a, ...b, ...b, ...c, ...c, ...a);
        }
        this.counts.mesh = positions.length / 3;
        this.canvas.dataset.triangles = String(this.counts.mesh / 3);
        this.upload(this.buffers.meshPosition, new Float32Array(positions));
        this.upload(this.buffers.meshNormal, new Float32Array(normals));
        this.counts.wire = wire.length / 3;
        this.upload(this.buffers.wire, new Float32Array(wire));
      } else {
        this.counts.mesh = 0;
        this.counts.wire = 0;
        this.canvas.dataset.triangles = "0";
      }
      this.uploadSceneLines();
    }

    uploadSceneLines() {
      const gl = this.gl;
      const vertices = [];
      const dark = app.theme === "dark";
      const gridColor = dark ? [0.16, 0.21, 0.28] : [0.78, 0.84, 0.9];
      const majorColor = dark ? [0.24, 0.31, 0.4] : [0.64, 0.72, 0.81];
      const outlineColor = dark ? [0.35, 0.66, 1.0] : [0.04, 0.36, 0.78];
      const xColor = [0.83, 0.33, 0.38];
      const yColor = [0.3, 0.68, 0.38];
      const zColor = [0.36, 0.62, 0.98];

      const add = (a, b, color) => {
        vertices.push(a[0], a[1], a[2], color[0], color[1], color[2]);
        vertices.push(b[0], b[1], b[2], color[0], color[1], color[2]);
      };

      const size = this.bounds
        ? subtract(this.bounds.max, this.bounds.min)
        : [100, 100, 100];
      const span = Math.max(size[0], size[1], 20);
      const step = niceStep(span / 8);
      const half = step * 10;
      const center = this.bounds
        ? [(this.bounds.min[0] + this.bounds.max[0]) / 2, (this.bounds.min[1] + this.bounds.max[1]) / 2]
        : [0, 0];
      const cx = Math.floor(center[0] / step) * step;
      const cy = Math.floor(center[1] / step) * step;

      if (this.grid) {
        for (let index = -10; index <= 10; index += 1) {
          const major = index === 0;
          const color = major ? majorColor : gridColor;
          const x = cx + index * step;
          const y = cy + index * step;
          add([x, cy - half, 0], [x, cy + half, 0], color);
          add([cx - half, y, 0], [cx + half, y, 0], color);
        }
        add([0, 0, 0], [half * 0.85, 0, 0], xColor);
        add([0, 0, 0], [0, half * 0.85, 0], yColor);
        add([0, 0, 0], [0, 0, Math.max(size[2], step) * 1.4], zColor);
      }

      if (this.outline && this.outline.length > 1) {
        const z = 0.08;
        for (let index = 0; index < this.outline.length; index += 1) {
          const current = this.outline[index];
          const next = this.outline[(index + 1) % this.outline.length];
          add([current[0], current[1], z], [next[0], next[1], z], outlineColor);
        }
      }

      // The loop being drawn, plus a closing preview once it has three points and a cross at each
      // placed point. The document does not change until the loop is closed.
      if (app.draw && app.draw.points.length) {
        const scaleFactor = documentScale();
        const points = app.draw.points.map(([x, y]) => [x * scaleFactor, y * scaleFactor]);
        const ink = dark ? [0.36, 0.92, 0.72] : [0.03, 0.55, 0.36];
        const closing = dark ? [0.24, 0.6, 0.5] : [0.4, 0.66, 0.58];
        for (let index = 0; index + 1 < points.length; index += 1) {
          add(
            [points[index][0], points[index][1], 0.1],
            [points[index + 1][0], points[index + 1][1], 0.1],
            ink
          );
        }
        if (points.length >= 3) {
          const last = points[points.length - 1];
          add([last[0], last[1], 0.1], [points[0][0], points[0][1], 0.1], closing);
        }
        const tick = Math.max(this.radius * 0.012, 0.05);
        for (const point of points) {
          add([point[0] - tick, point[1], 0.1], [point[0] + tick, point[1], 0.1], ink);
          add([point[0], point[1] - tick, 0.1], [point[0], point[1] + tick, 0.1], ink);
        }
      }

      this.counts.scene = vertices.length / 6;
      this.upload(this.buffers.scene, new Float32Array(vertices));
    }

    upload(buffer, data) {
      const gl = this.gl;
      gl.bindBuffer(gl.ARRAY_BUFFER, buffer);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
    }

    setMarker(point, color) {
      if (!this.ready) return;
      const gl = this.gl;
      if (!point) {
        this.counts.marker = 0;
        this.requestFrame();
        return;
      }
      const data = new Float32Array([
        point[0], point[1], point[2],
        color[0], color[1], color[2],
      ]);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers.marker);
      gl.bufferData(gl.ARRAY_BUFFER, data, gl.DYNAMIC_DRAW);
      this.counts.marker = 1;
      this.requestFrame();
    }

    fit() {
      if (!this.bounds && !this.mesh) return;
      let bounds = this.bounds;
      if (!bounds && this.mesh) {
        const min = [Infinity, Infinity, Infinity];
        const max = [-Infinity, -Infinity, -Infinity];
        for (const position of this.mesh.positions) {
          for (let axis = 0; axis < 3; axis += 1) {
            min[axis] = Math.min(min[axis], position[axis]);
            max[axis] = Math.max(max[axis], position[axis]);
          }
        }
        bounds = { min, max };
      }
      if (!bounds) return;
      this.camera.target = [
        (bounds.min[0] + bounds.max[0]) / 2,
        (bounds.min[1] + bounds.max[1]) / 2,
        (bounds.min[2] + bounds.max[2]) / 2,
      ];
      const size = subtract(bounds.max, bounds.min);
      this.radius = Math.max(lengthOf(size) / 2, 0.5);
      const fov = (50 * Math.PI) / 180;
      this.camera.distance = (this.radius / Math.tan(fov / 2)) * 1.65;
      this.camera.distance = Math.max(this.camera.distance, this.radius * 1.4);
      this.requestFrame();
    }

    matrices() {
      const width = this.canvas.width || 1;
      const height = this.canvas.height || 1;
      const aspect = width / Math.max(height, 1);
      const near = Math.max(0.01, this.camera.distance - this.radius * 4);
      const far = this.camera.distance + this.radius * 10 + 1000;
      const projection = perspective((50 * Math.PI) / 180, aspect, near, far);
      const eye = this.eye();
      const view = lookAt(eye, this.camera.target, [0, 0, 1]);
      return { eye, viewProjection: multiply(projection, view), aspect };
    }

    resize() {
      const ratio = Math.min(window.devicePixelRatio || 1, 2);
      const width = Math.max(1, Math.round(this.canvas.clientWidth * ratio));
      const height = Math.max(1, Math.round(this.canvas.clientHeight * ratio));
      if (width !== this.canvas.width || height !== this.canvas.height) {
        this.canvas.width = width;
        this.canvas.height = height;
        return true;
      }
      return false;
    }

    requestFrame() {
      if (!this.ready) return;
      if (this.frameHandle) return;
      this.frameHandle = window.requestAnimationFrame(() => {
        this.frameHandle = 0;
        this.render();
      });
    }

    render() {
      if (!this.ready) return;
      this.resize();
      const gl = this.gl;
      gl.viewport(0, 0, this.canvas.width, this.canvas.height);
      const canvasColor = app.theme === "dark" ? [0.035, 0.05, 0.075] : [0.91, 0.94, 0.97];
      gl.clearColor(canvasColor[0], canvasColor[1], canvasColor[2], 1);
      gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);

      const { eye, viewProjection } = this.matrices();

      gl.useProgram(this.lineProgram);
      gl.uniformMatrix4fv(this.locations.lineViewProjection, false, viewProjection);
      gl.uniform1f(this.locations.linePointSize, 1);
      gl.uniform1f(this.locations.lineAlpha, 1);
      gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers.scene);
      gl.enableVertexAttribArray(this.locations.linePosition);
      gl.enableVertexAttribArray(this.locations.lineColor);
      gl.vertexAttribPointer(this.locations.linePosition, 3, gl.FLOAT, false, 24, 0);
      gl.vertexAttribPointer(this.locations.lineColor, 3, gl.FLOAT, false, 24, 12);
      gl.drawArrays(gl.LINES, 0, this.counts.scene);

      if (this.mesh && this.counts.mesh) {
        gl.useProgram(this.meshProgram);
        gl.uniformMatrix4fv(this.locations.meshViewProjection, false, viewProjection);
        const color = app.theme === "dark" ? [0.42, 0.56, 0.7] : [0.28, 0.45, 0.62];
        gl.uniform3f(this.locations.meshColor, color[0], color[1], color[2]);
        gl.uniform3f(this.locations.meshEye, eye[0], eye[1], eye[2]);

        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers.meshPosition);
        gl.enableVertexAttribArray(this.locations.meshPosition);
        gl.vertexAttribPointer(this.locations.meshPosition, 3, gl.FLOAT, false, 0, 0);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers.meshNormal);
        gl.enableVertexAttribArray(this.locations.meshNormal);
        gl.vertexAttribPointer(this.locations.meshNormal, 3, gl.FLOAT, false, 0, 0);
        gl.drawArrays(gl.TRIANGLES, 0, this.counts.mesh);
      }

      if (this.wireframe && this.counts.wire) {
        gl.useProgram(this.lineProgram);
        gl.uniformMatrix4fv(this.locations.lineViewProjection, false, viewProjection);
        gl.uniform1f(this.locations.linePointSize, 1);
        gl.uniform1f(this.locations.lineAlpha, app.theme === "dark" ? 0.42 : 0.5);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers.wire);
        gl.enableVertexAttribArray(this.locations.linePosition);
        gl.vertexAttribPointer(this.locations.linePosition, 3, gl.FLOAT, false, 0, 0);
        gl.disableVertexAttribArray(this.locations.lineColor);
        gl.vertexAttrib3f(this.locations.lineColor, 0.62, 0.74, 0.88);
        gl.drawArrays(gl.LINES, 0, this.counts.wire);
      }

      if (this.pick && this.counts.marker) {
        gl.useProgram(this.lineProgram);
        gl.uniformMatrix4fv(this.locations.lineViewProjection, false, viewProjection);
        gl.uniform1f(this.locations.linePointSize, 9);
        gl.uniform1f(this.locations.lineAlpha, 1);
        gl.bindBuffer(gl.ARRAY_BUFFER, this.buffers.marker);
        gl.enableVertexAttribArray(this.locations.linePosition);
        gl.enableVertexAttribArray(this.locations.lineColor);
        gl.vertexAttribPointer(this.locations.linePosition, 3, gl.FLOAT, false, 24, 0);
        gl.vertexAttribPointer(this.locations.lineColor, 3, gl.FLOAT, false, 24, 12);
        gl.drawArrays(gl.POINTS, 0, 1);
      }
    }

    // The point under the cursor on the z = 0 drawing plane, in millimetres, or null when the
    // view is edge-on to the plane (where the ray has no single intersection).
    groundAt(clientX, clientY) {
      if (!this.ready || !this.camera || !this.canvas) return null;
      const rect = this.canvas.getBoundingClientRect();
      if (!rect || rect.width < 1 || rect.height < 1) return null;
      const ndcX = ((clientX - rect.left) / rect.width) * 2 - 1;
      const ndcY = 1 - ((clientY - rect.top) / rect.height) * 2;
      const eye = this.eye();
      if (!eye || !Array.isArray(eye) || eye.length < 3) return null;
      const target = this.camera.target || [0, 0, 0];
      const forward = normalize(subtract(target, eye));
      let right = cross(forward, [0, 0, 1]);
      if (lengthOf(right) < 0.001) right = [0, 1, 0];
      right = normalize(right);
      const up = normalize(cross(right, forward));
      const aspect = rect.width / Math.max(rect.height, 1);
      const tanHalf = Math.tan((50 * Math.PI) / 180 / 2);
      const direction = normalize(add(
        forward,
        add(scale(right, ndcX * aspect * tanHalf), scale(up, ndcY * tanHalf))
      ));
      if (Math.abs(direction[2]) < 1e-9) return null;
      const t = -eye[2] / direction[2];
      if (!Number.isFinite(t) || t <= 1e-6) return null;
      return [eye[0] + direction[0] * t, eye[1] + direction[1] * t];
    }

    groundPlanePoint(clientX, clientY) {
      return this.groundAt(clientX, clientY);
    }

    pickAt(clientX, clientY) {
      if (!this.mesh) {
        clearPick();
        return;
      }
      const rect = this.canvas.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) return;
      const ndcX = ((clientX - rect.left) / rect.width) * 2 - 1;
      const ndcY = 1 - ((clientY - rect.top) / rect.height) * 2;
      const eye = this.eye();
      const forward = normalize(subtract(this.camera.target, eye));
      let right = cross(forward, [0, 0, 1]);
      if (lengthOf(right) < 0.001) right = [0, 1, 0];
      right = normalize(right);
      const up = normalize(cross(right, forward));
      const aspect = rect.width / Math.max(rect.height, 1);
      const tanHalf = Math.tan((50 * Math.PI) / 180 / 2);
      const direction = normalize(add(
        forward,
        add(scale(right, ndcX * aspect * tanHalf), scale(up, ndcY * tanHalf))
      ));

      let best = null;
      const positions = this.mesh.positions;
      for (const triangle of this.mesh.triangles) {
        const a = positions[triangle[0]];
        const b = positions[triangle[1]];
        const c = positions[triangle[2]];
        if (!a || !b || !c) continue;
        const edge1 = subtract(b, a);
        const edge2 = subtract(c, a);
        const h = cross(direction, edge2);
        const determinant = dot(edge1, h);
        if (Math.abs(determinant) < 1e-10) continue;
        const inverse = 1 / determinant;
        const s = subtract(eye, a);
        const u = inverse * dot(s, h);
        if (u < 0 || u > 1) continue;
        const q = cross(s, edge1);
        const v = inverse * dot(direction, q);
        if (v < 0 || u + v > 1) continue;
        const t = inverse * dot(edge2, q);
        if (t <= 1e-6) continue;
        if (!best || t < best.t) {
          best = {
            t,
            point: add(eye, scale(direction, t)),
            normal: faceNormal(a, b, c),
            triangle,
          };
        }
      }

      if (!best) {
        clearPick();
        setStatus("Nothing under the cursor.", "");
        return;
      }
      const point = best.point;
      this.pick = best;
      this.setMarker(point, [1.0, 0.62, 0.25]);
      showPick(best);
    }

    refreshTheme() {
      if (!this.ready) return;
      this.uploadSceneLines();
      this.requestFrame();
    }
  }

  // ---------------------------------------------------------------------------------------
  // UI state rendering
  // ---------------------------------------------------------------------------------------

  function sketchById(id) {
    const project = app.project;
    if (!project) return null;
    return project.sketches.find((candidate) => candidate.id === id) || null;
  }

  function featureById(id) {
    const project = app.project;
    if (!project) return null;
    return project.features.find((candidate) => candidate.id === id) || null;
  }

  // The selection resolved against the current payload, or null when it names nothing any more.
  function selection() {
    if (!app.selected) return null;
    if (app.selected.kind === "sketch") {
      const node = sketchById(app.selected.id);
      return node ? { kind: "sketch", id: node.id, node } : null;
    }
    const node = featureById(app.selected.id);
    return node ? { kind: "feature", id: node.id, node } : null;
  }

  // The sketch the profile and placement controls act on: the selected sketch, the sketch the
  // selected feature consumes, or, with nothing selected, the sketch of the last feature.
  function activeSketch() {
    const project = app.project;
    if (!project || !project.sketches.length) return null;
    const selected = selection();
    if (selected) {
      if (selected.kind === "sketch") return selected.node;
      const sketch = sketchById(selected.node.sketch_id);
      if (sketch) return sketch;
    }
    const feature = project.features[project.features.length - 1];
    if (feature) {
      const sketch = sketchById(feature.sketch_id);
      if (sketch) return sketch;
    }
    return project.sketches[0];
  }

  // The ordered history: sketches and features in creation order. For features that is also build
  // order, because a feature is always allocated an ID after the sketch it consumes.
  function historyEntries() {
    const project = app.project;
    if (!project) return [];
    const entries = [
      ...project.sketches.map((sketch) => ({ kind: "sketch", id: sketch.id, sketch })),
      ...project.features.map((feature) => ({ kind: "feature", id: feature.id, feature })),
    ];
    entries.sort((a, b) => a.id - b.id);
    return entries;
  }

  function select(kind, id) {
    app.selected = { kind, id };
    renderTree();
    renderProperties();
    syncProfileControls();
    syncPlacementControls();
    syncCreateControls();
  }

  function applyState(next) {
    app.project = next;
    byId("docTitle").textContent = next.name;
    byId("docTitle").title = next.file;
    byId("fileName").textContent = next.file.split(/[\\/]/).pop() || next.file;
    byId("fileName").title = next.file;
    byId("unitLabel").textContent = `${next.unit} (${next.units})`;
    byId("revisionLabel").textContent = String(next.revision);
    byId("historyLabel").textContent = `${next.history.undo} undo · ${next.history.redo} redo`;
    const badge = byId("dirtyBadge");
    badge.textContent = next.dirty ? "unsaved" : "saved";
    badge.classList.toggle("dirty", Boolean(next.dirty));
    byId("statusUnits").textContent = next.units;

    byId("undoBtn").disabled = app.busy || next.history.undo === 0;
    byId("redoBtn").disabled = app.busy || next.history.redo === 0;
    byId("saveBtn").disabled = app.busy;
    const exportable = next.features.length > 0 && !next.failure;
    byId("exportBtn").disabled = app.busy || !exportable;
    byId("exportBtn").title = next.failure
      ? `Export is refused while feature ${next.failure.feature} cannot rebuild`
      : "Export binary STL";

    // A selection that names an undone entity is cleared rather than left dangling.
    if (app.selected && !selection()) app.selected = null;

    renderParameters();
    renderTree();
    renderProperties();
    renderRevisions();
    renderFailure(next.failure);
    renderRecovery(next.recovery);
    renderShare(next.share);
    syncProfileControls();
    syncPlacementControls();
    syncCreateControls();
    updateGeometryStatus();
    clearPick();
    if (app.viewport) app.viewport.setModel(next);
    refreshDrawing();
  }

  function renderParameters() {
    const container = byId("parameters");
    const project = app.project;
    if (!project) return;
    const ids = project.parameters.map((parameter) => parameter.id);
    const known = [...app.parameterInputs.keys()];
    const rebuild = ids.length !== known.length || ids.some((id, index) => id !== known[index]);
    if (rebuild) {
      container.textContent = "";
      app.parameterInputs.clear();
      for (const parameter of project.parameters) {
        const row = element("div", "parameter-row");
        const label = element("label", "", parameter.id);
        label.htmlFor = `param-${parameter.id}`;
        const wrap = element("div", "parameter-input");
        const input = document.createElement("input");
        input.id = `param-${parameter.id}`;
        input.type = "number";
        input.step = "any";
        input.min = "0.000001";
        input.inputMode = "decimal";
        input.value = String(parameter.value);
        input.setAttribute("aria-label", `${parameter.id} value`);
        const unit = element("span", "", project.units);
        wrap.append(input, unit);
        row.append(label, wrap);
        container.append(row);
        app.parameterInputs.set(parameter.id, input);
        input.addEventListener("change", () => commitParameter(parameter.id, input));
        input.addEventListener("blur", () => commitParameter(parameter.id, input));
        input.addEventListener("keydown", (event) => {
          if (event.key === "Enter") {
            event.preventDefault();
            commitParameter(parameter.id, input);
            input.blur();
          } else if (event.key === "Escape") {
            input.value = String(currentParameter(parameter.id));
            input.blur();
          }
        });
      }
    }
    for (const parameter of project.parameters) {
      const input = app.parameterInputs.get(parameter.id);
      if (input && document.activeElement !== input) {
        input.value = String(parameter.value);
      }
    }
  }

  function currentParameter(id) {
    const parameter = app.project && app.project.parameters.find((candidate) => candidate.id === id);
    return parameter ? parameter.value : 0;
  }

  async function commitParameter(id, input) {
    if (app.busy) return;
    const value = Number(input.value);
    if (!Number.isFinite(value) || value <= 0) {
      toast(`${id} must be a positive number.`, true);
      log(`${id} must be a positive number.`, "error");
      input.value = String(currentParameter(id));
      return;
    }
    if (Math.abs(value - currentParameter(id)) < 1e-12) {
      input.value = String(currentParameter(id));
      return;
    }
    await mutate("/api/parameter", { id, value }, `Set ${id} to ${formatValue(value)}`);
  }

  function renderTree() {
    const tree = byId("tree");
    const project = app.project;
    tree.textContent = "";
    if (!project) return;
    const entries = historyEntries();
    if (!entries.length) {
      tree.append(element("p", "hint", "The tree is empty. Add a sketch to start."));
      return;
    }
    const selected = selection();
    const unitScale = documentScale();
    for (const entry of entries) {
      const isSelected = Boolean(selected) && selected.kind === entry.kind && selected.id === entry.id;
      const row = document.createElement("button");
      row.type = "button";
      row.className = `tree-item${isSelected ? " selected" : ""}`;
      row.setAttribute("aria-pressed", String(isSelected));
      row.addEventListener("click", () => select(entry.kind, entry.id));
      const head = element("span", "tree-row");
      if (entry.kind === "sketch") {
        const sketch = entry.sketch;
        head.append(
          element("span", "tree-icon", "▱"),
          element("span", "tree-label", `Sketch ${sketch.id}`),
          element(
            "span",
            "tree-meta",
            `origins ${formatNumber(sketch.origin[0])}, ${formatNumber(sketch.origin[1])}`
          ),
          element("span", "tree-badge", profileName(sketch.profile.kind))
        );
        row.title = `Sketch ${sketch.id}: ${profileName(sketch.profile.kind)}, ${sketch.segments} segments`;
      } else {
        const feature = entry.feature;
        const label = feature.kind === "revolve" ? "Revolve" : "Extrude";
        const detail =
          feature.kind === "revolve"
            ? feature.axis
            : `${formatNumber(feature.depth_mm / unitScale)} ${project.units}`;
        head.append(
          element("span", "tree-icon", "▣"),
          element("span", "tree-label", `${label} ${feature.id}`),
          element("span", "tree-meta", `${feature.operation} · ${detail}`),
          element("span", `tree-badge ${feature.status}`, feature.status)
        );
        row.title = `${label} ${feature.id} (${feature.operation}) from sketch ${feature.sketch_id}`;
      }
      row.append(head);
      if (entry.kind === "feature" && entry.feature.message) {
        row.append(element("span", "tree-message", entry.feature.message));
      }
      tree.append(row);
    }
  }

  // One line per change of failure, so a rebuild that fails once is not re-announced on every later
  // payload (a parameter edit re-renders the whole state).
  // Unsaved work is protected by a snapshot next to the project file. Two things need saying: the
  // session started from one, or a snapshot write is failing. Either is shown until it stops being
  // true, and `recovered` is only cleared by saving or discarding, so the banner cannot lie.
  function renderRecovery(recovery) {
    const banner = byId("recoveryBanner");
    const discard = byId("recoveryDiscard");
    const message = byId("recoveryMessage");
    if (!recovery || (!recovery.recovered && !recovery.autosave_error)) {
      banner.hidden = true;
      app.lastRecoveryKey = null;
      return;
    }
    banner.hidden = false;
    if (recovery.autosave_error) {
      const text = `Unsaved changes are not protected: ${recovery.autosave_error}`;
      message.textContent = text;
      discard.hidden = !recovery.recovered;
      if (app.lastRecoveryKey !== text) log(text, "error");
      app.lastRecoveryKey = text;
      return;
    }
    const when = recovery.saved_at_unix
      ? new Date(recovery.saved_at_unix * 1000).toLocaleTimeString([], {
          hour: "2-digit",
          minute: "2-digit",
        })
      : "an earlier session";
    // A backup is the previous save, not this session's edits, and the two mean very different
    // things to a user deciding whether to save over the file.
    const text =
      recovery.source === "backup"
        ? `The project file could not be read. Opened the previous saved copy from ${when}; save to rebuild the file with it.`
        : recovery.file_unreadable
          ? `The project file could not be read. Recovered unsaved changes from ${when}: save to replace the file with them.`
          : `Recovered unsaved changes from ${when}. Save to keep them, or discard to go back to the file on disk.`;
    message.textContent = text;
    // There is nothing to go back to when the file itself is unreadable.
    discard.hidden = recovery.file_unreadable;
    if (app.lastRecoveryKey !== text) {
      log(text);
      toast("Recovered unsaved changes.");
    }
    app.lastRecoveryKey = text;
  }

  async function discardRecovery() {
    await mutate("/api/recovery/discard", {}, "Discarded the recovered changes");
  }

  // ---------------------------------------------------------------------------------------
  // Named revisions
  // ---------------------------------------------------------------------------------------

  function formatRevisionTime(seconds) {
    if (!seconds) return "unknown time";
    return new Date(seconds * 1000).toLocaleString([], {
      month: "short",
      day: "numeric",
      hour: "2-digit",
      minute: "2-digit",
    });
  }

  // The panel and the palette both read the same payload list, so a revision created by any route
  // shows up in both without a second source of truth.
  function renderRevisions() {
    const project = app.project;
    const list = byId("revisions");
    const label = byId("revisionsLabel");
    list.textContent = "";
    const revisions = project && Array.isArray(project.revisions) ? project.revisions : [];
    label.textContent = `${revisions.length} saved`;
    byId("addRevisionBtn").disabled = app.busy;
    if (!revisions.length) {
      list.append(element("li", "hint", "No revisions yet. Save one before a risky edit."));
      return;
    }
    for (const revision of revisions) {
      const row = document.createElement("li");
      row.className = "revision-row";
      const text = element("div", "revision-text");
      text.append(
        element("span", "revision-name", revision.name),
        element(
          "span",
          "revision-meta",
          `${formatRevisionTime(revision.created_at_unix)} · ${revision.features} features · rev ${revision.revision}`
        )
      );
      const actions = element("div", "revision-actions");
      const restore = document.createElement("button");
      restore.type = "button";
      restore.className = "button ghost small";
      restore.textContent = "Restore";
      restore.disabled = app.busy;
      restore.dataset.revisionRestore = String(revision.id);
      restore.addEventListener("click", () => restoreRevision(revision.id, revision.name));
      const remove = document.createElement("button");
      remove.type = "button";
      remove.className = "link";
      remove.textContent = "delete";
      remove.disabled = app.busy;
      remove.addEventListener("click", () => removeRevision(revision.id, revision.name));
      actions.append(restore, remove);
      row.append(text, actions);
      list.append(row);
    }
  }

  async function addRevision() {
    const project = app.project;
    if (!project) return;
    const input = byId("revisionName");
    let name = input.value.trim();
    if (!name) {
      // A default that cannot collide with an existing name, so the button always works.
      name = `Revision ${(project.revisions ? project.revisions.length : 0) + 1}`;
    }
    const next = await mutate("/api/revision", { name }, `Saved revision “${name}”`);
    if (next) {
      input.value = "";
      log(`Revision “${name}” recorded`);
    }
  }

  async function restoreRevision(id, name) {
    await mutate("/api/revision/restore", { id }, `Restored revision “${name}”`);
  }

  async function removeRevision(id, name) {
    await mutate("/api/revision/remove", { id }, `Deleted revision “${name}”`);
  }

  // The share link is rendered from the server's payload, so it names the address this session was
  // started on rather than whatever the local browser happens to be using.
  let shareLink = null;

  function renderShare(share) {
    const section = byId("shareSection");
    shareLink = share && share.url ? share.url : null;
    section.hidden = !shareLink;
    if (!shareLink) return;
    const input = byId("shareLink");
    if (input.value !== shareLink) input.value = shareLink;
  }

  async function copyShareLink() {
    if (!shareLink) return;
    try {
      await navigator.clipboard.writeText(shareLink);
      toast("Share link copied. Anyone with it can edit until you stop the studio.");
      log("Share link copied.");
    } catch (error) {
      const input = byId("shareLink");
      input.focus();
      input.select();
      toast("Select the link and copy it.", true);
    }
  }

  function renderFailure(failure) {
    const banner = byId("failureBanner");
    banner.hidden = !failure;
    const key = failure ? `${failure.feature}:${failure.message}` : null;
    if (failure) {
      byId("failureMessage").textContent = `Feature ${failure.feature} could not rebuild: ${failure.message}`;
      if (key !== app.lastFailureKey) {
        log(`Feature ${failure.feature} failed: ${failure.message}`, "error");
        toast(`Feature ${failure.feature} could not rebuild.`, true);
      }
    }
    app.lastFailureKey = key;
  }

  function syncPlacementControls() {
    const sketch = activeSketch();
    const label = byId("placementLabel");
    const x = byId("originX");
    const y = byId("originY");
    const z = byId("originZ");
    const button = byId("moveSketchBtn");
    const disabled = app.busy || !sketch;
    x.disabled = disabled;
    y.disabled = disabled;
    z.disabled = disabled;
    button.disabled = disabled;
    if (!sketch) {
      label.textContent = "no sketch";
      return;
    }
    label.textContent = `sketch ${sketch.id} · plane ${formatNumber(sketch.plane_z || 0)}`;
    if (document.activeElement !== x) x.value = String(sketch.origin[0]);
    if (document.activeElement !== y) y.value = String(sketch.origin[1]);
    if (document.activeElement !== z) z.value = String(sketch.plane_z || 0);
  }

  function syncCreateControls() {
    const project = app.project;
    const kind = byId("newSketchKind").value;
    byId("newSidesField").hidden = kind !== "regular_polygon";
    byId("newSectorField").hidden = kind !== "sector";
    byId("addSketchBtn").disabled = app.busy;
    byId("addSketchBtn").textContent = kind === "polyline" ? "Draw polyline" : "Add sketch";

    // An extrude and a revolve need different inputs, so only the relevant field is shown and the
    // button names the operation it will actually perform.
    const featureKind = byId("newFeatureKind").value;
    let revolves = featureKind === "revolve";
    byId("newDepthField").hidden = revolves;
    byId("newDirectionField").hidden = revolves;
    byId("newAxisField").hidden = !revolves;
    byId("addFeatureBtn").textContent = revolves ? "Add revolve" : "Add extrude";

    const sketchSelect = byId("newFeatureSketch");
    const chosenSketch = sketchSelect.value;
    sketchSelect.textContent = "";
    for (const sketch of project ? project.sketches : []) {
      const option = document.createElement("option");
      option.value = String(sketch.id);
      option.textContent = `Sketch ${sketch.id} · ${profileName(sketch.profile.kind)}`;
      sketchSelect.append(option);
    }
    const sketchIds = [...sketchSelect.options].map((option) => option.value);
    // An explicit selection decides the extrude target; with nothing selected the form keeps
    // whatever the user picked, and only falls back to the newest sketch when there is no choice.
    const active = activeSketch();
    if (active && selection()) sketchSelect.value = String(active.id);
    else if (sketchIds.includes(chosenSketch)) sketchSelect.value = chosenSketch;
    else if (active) sketchSelect.value = String(active.id);
    sketchSelect.disabled = app.busy || !sketchIds.length;

    const depthSelect = byId("newFeatureDepth");
    const chosenDepth = depthSelect.value;
    depthSelect.textContent = "";
    for (const parameter of project ? project.parameters : []) {
      const option = document.createElement("option");
      option.value = parameter.id;
      option.textContent = `${parameter.id} (${formatNumber(parameter.value)})`;
      depthSelect.append(option);
    }
    const depthIds = [...depthSelect.options].map((option) => option.value);
    if (depthIds.includes(chosenDepth)) depthSelect.value = chosenDepth;
    else if (depthIds.includes("depth")) depthSelect.value = "depth";
    depthSelect.disabled = app.busy || !depthIds.length;

    // At any point in the history only one class of operation is legal, so the control keeps only
    // the available choices; the document refuses anything else.
    const hasBody = Boolean(project && project.features.length);
    const operationSelect = byId("newFeatureOperation");
    // Rebuild the options from scratch so the unavailable ones are not selectable. Cut is offered
    // only when the kernel advertises boolean difference, because a cut on a kernel without it has
    // no geometry behind the button. The truck adapter advertises it: a cut whose tool pierces
    // the body is exact (verified analytically and through this API), and the document refuses one
    // that stops below the body's top face instead of accepting the wrong solid the raw boolean
    // returns.
    const capabilities = project && project.kernel ? project.kernel.capabilities : [];
    const supportsDifference = capabilities.includes("boolean difference");
    const supportsRevolve = capabilities.includes("revolve");
    const revolveOption = byId("newFeatureKind").querySelector('option[value="revolve"]');
    // A kind the kernel cannot build is not offered, the same rule the cut operation follows.
    revolveOption.disabled = !supportsRevolve;
    if (!supportsRevolve && revolves) {
      byId("newFeatureKind").value = "extrude";
      revolves = false;
      byId("newDepthField").hidden = false;
      byId("newAxisField").hidden = true;
      byId("addFeatureBtn").textContent = "Add extrude";
    }
    operationSelect.replaceChildren();
    const newBody = document.createElement("option");
    newBody.value = "new_body";
    newBody.textContent = "New body";
    const join = document.createElement("option");
    join.value = "join";
    join.textContent = "Join";
    operationSelect.append(newBody);
    if (hasBody) {
      operationSelect.append(join);
      if (supportsDifference) {
        const cut = document.createElement("option");
        cut.value = "cut";
        cut.textContent = "Cut";
        operationSelect.append(cut);
      }
      if (![...operationSelect.options].some((option) => option.value === operationSelect.value)) {
        operationSelect.value = "join";
      }
      byId("createHint").textContent = supportsDifference
        ? "This project has one body; a new feature joins it or cuts from it."
        : "This project has one body; a new feature joins it. Cut is unavailable because the active kernel does not advertise boolean difference.";
    } else {
      byId("createHint").textContent =
        "The first feature starts the body; later features join it. A revolve turns the sketch a full turn about one of its own axes.";
    }
    operationSelect.disabled = app.busy;
    byId("addFeatureBtn").disabled = app.busy || !sketchIds.length;
  }

  function profileName(kind) {
    if (kind === "rectangle") return "rectangle";
    if (kind === "circle") return "circle";
    if (kind === "regular_polygon") return "polygon";
    return kind;
  }

  function renderProperties() {
    const container = byId("properties");
    container.textContent = "";
    const project = app.project;
    if (!project) return;
    const geometry = project.geometry;
    const bounds = project.bounds;
    const unitScale = documentScale();
    const size = bounds ? subtract(bounds.max, bounds.min).map((value) => value / unitScale) : null;
    const rows = [
      ["Volume", `${formatNumber(geometry.volume_mm3 / (unitScale * unitScale * unitScale))} ${project.units}³`],
      ["Surface", `${formatNumber(geometry.surface_area_mm2 / (unitScale * unitScale))} ${project.units}²`],
      ["Triangles", String(geometry.triangles)],
      ["Vertices", String(geometry.vertices)],
      ["Bounding box", size ? `${formatNumber(size[0])} × ${formatNumber(size[1])} × ${formatNumber(size[2])}` : "—"],
      ["Rebuild", project.failure ? `stopped at feature ${project.failure.feature}` : "all features built"],
    ];
    const selected = selection();
    if (selected && selected.kind === "sketch") {
      const sketch = selected.node;
      rows.push(
        ["Selected", `Sketch ${sketch.id}`],
        ["Profile", profileName(sketch.profile.kind)],
        ["Origin", `${formatNumber(sketch.origin[0])}, ${formatNumber(sketch.origin[1])} ${project.units}`],
        ["Plane", `${formatNumber(sketch.plane_z || 0)} ${project.units}`],
        ["Segments", String(sketch.segments)]
      );
      if (sketch.profile.kind === "polyline" && Array.isArray(sketch.profile.points)) {
        rows.push([
          "Loop",
          sketch.profile.points
            .map((point) => `(${formatNumber(point[0])}, ${formatNumber(point[1])})`)
            .join(" "),
        ]);
      }
    } else if (selected && selected.kind === "feature") {
      const feature = selected.node;
      rows.push(
        ["Selected", `${feature.kind === "revolve" ? "Revolve" : "Extrude"} ${feature.id}`],
        ["Operation", feature.operation],
        ["Status", feature.status],
        ["Sketch", String(feature.sketch_id)]
      );
      if (feature.kind === "revolve") {
        rows.push(["Axis", feature.axis], ["Turn", "full revolution"]);
      } else {
        rows.push(
          ["Depth parameter", feature.depth_param],
          ["Depth", `${formatNumber(feature.depth_mm / unitScale)} ${project.units}`],
          ["Direction", feature.direction === "down" ? "down (−Z)" : "up (+Z)"]
        );
      }
      if (typeof feature.volume_mm3 === "number") {
        rows.push([
          "Volume after",
          `${formatNumber(feature.volume_mm3 / (unitScale * unitScale * unitScale))} ${project.units}³`,
        ]);
      }
      if (feature.message) rows.push(["Message", feature.message]);
    } else {
      rows.push(["Selected", "nothing — click a tree row"]);
    }
    for (const [label, value] of rows) {
      container.append(element("span", "", label), element("span", "", value));
    }

    const kernel = byId("kernelInfo");
    kernel.textContent = "";
    const info = project.kernel;
    kernel.append(
      element("span", "", "Name"),
      element("span", "", `${info.name} ${info.version}`),
      element("span", "", "Capabilities"),
      element("span", "", info.capabilities.join(", ")),
      element("span", "", "Schema"),
      element("span", "", String(project.schema))
    );
  }

  function updateGeometryStatus() {
    const project = app.project;
    if (!project) return;
    const unitScale = documentScale();
    const stale = project.failure ? " · last good rebuild" : "";
    byId("statusGeometry").textContent = `${project.geometry.triangles} tri · ${formatNumber(project.geometry.volume_mm3 / (unitScale * unitScale * unitScale))} ${project.units}³${stale}`;
    const notice = byId("viewportNotice");
    if (!app.viewport || !app.viewport.fallback) {
      notice.hidden = project.geometry.triangles > 0;
    }
  }

  function syncProfileControls() {
    const sketch = activeSketch();
    const select = byId("profileKind");
    const sidesField = byId("sidesField");
    const hint = byId("profileHint");
    const segments = byId("segmentsLabel");
    if (!sketch) {
      select.disabled = true;
      byId("sidesInput").disabled = true;
      byId("sectorStart").disabled = true;
      byId("sectorSweep").disabled = true;
      sidesField.hidden = true;
      byId("sectorField").hidden = true;
      segments.textContent = "no sketch";
      return;
    }
    select.disabled = app.busy;
    byId("sidesInput").disabled = app.busy;
    byId("sectorStart").disabled = app.busy;
    byId("sectorSweep").disabled = app.busy;
    select.value = sketch.profile.kind;
    const isPolygon = sketch.profile.kind === "regular_polygon";
    sidesField.hidden = !isPolygon;
    if (isPolygon) byId("sidesInput").value = String(sketch.profile.sides);
    const isSector = sketch.profile.kind === "sector";
    byId("sectorField").hidden = !isSector;
    if (isSector) {
      byId("sectorStart").value = String(sketch.profile.start_degrees);
      byId("sectorSweep").value = String(sketch.profile.sweep_degrees);
    }
    segments.textContent = `sketch ${sketch.id} · ${sketch.segments} segments`;
    if (sketch.profile.kind === "rectangle") {
      hint.textContent = "A rectangle is sized by width × height.";
    } else if (sketch.profile.kind === "circle") {
      hint.textContent = "A circle is approximated by a tolerance-bounded polygon.";
    } else if (sketch.profile.kind === "polyline") {
      hint.textContent =
        "A drawn polyline can be replaced by a rectangle, circle, polygon, or sector; draw another to change the loop.";
    } else if (sketch.profile.kind === "sector") {
      hint.textContent =
        "A sector is one arc of the diameter circle plus the two radii to its centre. A full turn is a circle.";
    } else {
      hint.textContent = "A regular polygon is circumscribed by the diameter.";
    }
  }

  function showPick(pick) {
    const card = byId("pickCard");
    const scale = documentScale();
    const point = [pick.point[0] / scale, pick.point[1] / scale, pick.point[2] / scale];
    const units = app.project ? app.project.units : "mm";
    card.hidden = false;
    byId("pickX").textContent = `${formatNumber(point[0])} ${units}`;
    byId("pickY").textContent = `${formatNumber(point[1])} ${units}`;
    byId("pickZ").textContent = `${formatNumber(point[2])} ${units}`;
    byId("pickNormal").textContent = pick.normal.map((value) => formatNumber(value, 3)).join(", ");
    byId("statusPick").textContent = `X ${formatNumber(point[0])}  Y ${formatNumber(point[1])}  Z ${formatNumber(point[2])} ${units}`;
  }

  function clearPick() {
    byId("pickCard").hidden = true;
    byId("statusPick").textContent = "—";
    if (app.viewport) {
      app.viewport.pick = null;
      app.viewport.setMarker(null);
    }
  }

  // ---------------------------------------------------------------------------------------
  // Mutations
  // ---------------------------------------------------------------------------------------

  function setBusy(value) {
    app.busy = value;
    if (app.project) {
      byId("undoBtn").disabled = value || app.project.history.undo === 0;
      byId("redoBtn").disabled = value || app.project.history.redo === 0;
      byId("saveBtn").disabled = value;
      const exportable = app.project.features.length > 0 && !app.project.failure;
      byId("exportBtn").disabled = value || !exportable;
      byId("profileKind").disabled = value;
    }
    // Revision rows are built inside `applyState`, which runs while the app is still busy, so their
    // buttons would otherwise stay disabled until the next mutation.
    renderRevisions();
    syncCreateControls();
    syncPlacementControls();
  }

  // Returns the new state payload on success and null on failure, so a caller that needs to follow
  // up (selecting the entity it just created) can use the result without re-fetching it.
  async function mutate(path, body, label) {
    if (app.busy) return null;
    setBusy(true);
    setStatus("Rebuilding geometry…", "busy");
    try {
      const next = await api(path, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body),
      });
      applyState(next);
      setStatus("Ready", "");
      log(label);
      toast(label);
      return next;
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus(message, "error");
      log(message, "error");
      toast(message, true);
      try {
        applyState(await api("/api/state"));
      } catch (refreshError) {
        // The server may have stopped; the message already says so.
      }
      return null;
    } finally {
      setBusy(false);
    }
  }

  async function undo() {
    await mutate("/api/undo", {}, "Undo");
  }

  async function redo() {
    await mutate("/api/redo", {}, "Redo");
  }

  async function save() {
    const ok = await mutate("/api/save", {}, "Project saved");
    if (ok) toast("Saved to the project file.");
  }

  // The download goes through fetch so a shared session can present its token as a header. A plain
  // anchor to /api/export/stl would be a 401 in a shared studio, which is exactly the case a user
  // would hit first when a colleague exports.
  async function exportStl() {
    const project = app.project;
    if (!project) return;
    if (project.failure) {
      toast(`Feature ${project.failure.feature} cannot rebuild, so the export would not match the model.`, true);
      return;
    }
    try {
      const headers = new Headers();
      if (shareToken) headers.set("x-atlas-token", shareToken);
      const response = await fetch("/api/export/stl", { headers });
      if (!response.ok) {
        const payload = await response.json().catch(() => null);
        throw new ApiError(
          payload && payload.error ? payload.error.message : `Export failed with status ${response.status}.`,
          "export_failure",
          response.status
        );
      }
      const blob = await response.blob();
      downloadBlob(blob, `${project.name.replace(/[^A-Za-z0-9_-]+/g, "_") || "part"}.stl`);
      log("Exported binary STL");
      toast("STL download started.");
    } catch (error) {
      toast(error.message, true);
      log(`Export failed: ${error.message}`, "error");
    }
  }

  function downloadBlob(blob, filename) {
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = filename;
    document.body.append(anchor);
    anchor.click();
    anchor.remove();
    URL.revokeObjectURL(url);
  }

  // The plan drawing is a separate document from the model, so it is fetched on its own. Two
  // properties matter here: a failed feature must not blank the drawing (drafting never touches the
  // kernel), and a slow response must not overwrite a newer one, hence the request counter.
  let drawingRequest = 0;

  // The editor's own state. It is deliberately small: which tool is armed, the points of the shape
  // being drawn, which entity is selected, which layer new geometry goes on, and Osnap/Polar state.
  const drawState = {
    tool: "select",
    points: [],
    selected: null,
    layer: null,
    osnap: true,
    polar: true,
    activeSnap: null,
    polarHit: null,
    snapOverride: null,
    viewZoom: 1.0,
    viewPan: [0, 0],
  };

  async function refreshDrawing() {
    const request = ++drawingRequest;
    try {
      const draft = await api("/api/draft");
      if (request !== drawingRequest) return;
      applyDrawing(draft);
    } catch (error) {
      if (request !== drawingRequest) return;
      const preview = byId("drawingPreview");
      preview.textContent = "";
      preview.append(element("span", "hint", `The drawing could not be built: ${error.message}`));
      byId("drawingEntities").textContent = "—";
      byId("downloadDxfBtn").disabled = true;
      byId("plotPdfBtn").disabled = true;
      return;
    }
  }

  // Replace the drawing with the server's copy. The server is the authority: an edit is only real
  // once it has been accepted, so the panel never shows geometry the document does not hold.
  function applyDrawing(draft) {
    if (draft && draft.draft) {
      draft = draft.draft;
    }
    app.drawing = draft;
    if (!drawState.layer || !(draft.layers || []).some((layer) => layer.name === drawState.layer)) {
      const layers = draft.layers || [];
      // Prefer the layer the plan is on, then any layer that accepts geometry, so a new line does not
      // land on layer 0 while the outline is somewhere else.
      const preferred = layers.find((layer) => layer.name === "OUTLINE" && layer.visible !== false && !layer.locked)
        || layers.find((layer) => layer.visible !== false && !layer.locked)
        || layers[0];
      drawState.layer = preferred ? preferred.name : null;
    }
    if (drawState.selected && !(draft.entities || []).some((entity) => entity.id === drawState.selected)) {
      drawState.selected = null;
    }
    renderDrawing(draft);
  }

  async function drawingMutate(path, body, label) {
    if (app.busy) return null;
    setBusy(true);
    try {
      const payload = await api(path, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(body || {}),
      });
      applyDrawing(payload);
      if (label) {
        log(label);
        toast(label);
      }
      return payload;
    } catch (error) {
      toast(error.message, true);
      log(error.message, "error");
      return null;
    } finally {
      setBusy(false);
      // The panel was rendered while the app was busy, so its buttons are re-enabled here.
      if (app.drawing) syncDrawingControls();
    }
  }

  function drawingPath(entity) {
    const kind = entity.kind;
    switch (kind.kind) {
      case "line":
        return `M ${kind.start[0]} ${kind.start[1]} L ${kind.end[0]} ${kind.end[1]}`;
      case "polyline": {
        const d = kind.points.map((p, i) => `${i ? "L" : "M"} ${p[0]} ${p[1]}`).join(" ");
        return kind.closed && kind.points.length > 2 ? `${d} Z` : d;
      }
      case "circle":
        return `M ${kind.center[0] - kind.radius} ${kind.center[1]} a ${kind.radius} ${kind.radius} 0 1 0 ${2 * kind.radius} 0 a ${kind.radius} ${kind.radius} 0 1 0 ${-2 * kind.radius} 0`;
      case "arc": {
        const sweep = ((kind.end_degrees - kind.start_degrees) % 360 + 360) % 360;
        const [cx, cy] = kind.center;
        const a0 = (kind.start_degrees * Math.PI) / 180;
        const a1 = (kind.end_degrees * Math.PI) / 180;
        const sx = cx + kind.radius * Math.cos(a0);
        const sy = cy + kind.radius * Math.sin(a0);
        const ex = cx + kind.radius * Math.cos(a1);
        const ey = cy + kind.radius * Math.sin(a1);
        // The group is mirrored in y, so a counter-clockwise arc is sweep-flag 0.
        return `M ${sx} ${sy} A ${kind.radius} ${kind.radius} 0 ${sweep > 180 ? 1 : 0} 0 ${ex} ${ey}`;
      }
      case "rectangle": {
        const [x, y] = kind.origin;
        const { width, height } = kind;
        return `M ${x} ${y} L ${x + width} ${y} L ${x + width} ${y + height} L ${x} ${y + height} Z`;
      }
      case "ellipse": {
        const [cx, cy] = kind.center;
        const rx = kind.radius_x;
        const ry = kind.radius_y;
        return `M ${cx - rx} ${cy} a ${rx} ${ry} 0 1 0 ${2 * rx} 0 a ${rx} ${ry} 0 1 0 ${-2 * rx} 0`;
      }
      case "spline": {
        if (!kind.points || kind.points.length < 2) return null;
        const pts = sampleSplinePoints(kind.points, kind.closed, 16);
        const d = pts.map((p, i) => `${i ? "L" : "M"} ${p[0]} ${p[1]}`).join(" ");
        return kind.closed ? `${d} Z` : d;
      }
      default:
        return null;
    }
  }

  function sampleSplinePoints(points, closed, samplesPerSegment = 16) {
    const n = points.length;
    if (n < 2) return points.slice();
    const steps = Math.max(2, samplesPerSegment);
    const sampled = [];
    const numSegments = closed ? n : n - 1;
    for (let i = 0; i < numSegments; i++) {
      const p1 = points[i];
      const p2 = points[(i + 1) % n];
      const p0 = closed
        ? points[(i + n - 1) % n]
        : i === 0
        ? [2 * p1[0] - p2[0], 2 * p1[1] - p2[1]]
        : points[i - 1];
      const p3 = closed
        ? points[(i + 2) % n]
        : i + 2 < n
        ? points[i + 2]
        : [2 * p2[0] - p1[0], 2 * p2[1] - p1[1]];

      const startStep = i === 0 ? 0 : 1;
      for (let step = startStep; step <= steps; step++) {
        const t = step / steps;
        const t2 = t * t;
        const t3 = t2 * t;
        const x = 0.5 * (
          2 * p1[0] +
          (-p0[0] + p2[0]) * t +
          (2 * p0[0] - 5 * p1[0] + 4 * p2[0] - p3[0]) * t2 +
          (-p0[0] + 3 * p1[0] - 3 * p2[0] + p3[0]) * t3
        );
        const y = 0.5 * (
          2 * p1[1] +
          (-p0[1] + p2[1]) * t +
          (2 * p0[1] - 5 * p1[1] + 4 * p2[1] - p3[1]) * t2 +
          (-p0[1] + 3 * p1[1] - 3 * p2[1] + p3[1]) * t3
        );
        sampled.push([x, y]);
      }
    }
    return sampled;
  }

  const SVG_NS = "http://www.w3.org/2000/svg";

  function svgNode(name, attributes) {
    const node = document.createElementNS(SVG_NS, name);
    for (const [key, value] of Object.entries(attributes)) node.setAttribute(key, String(value));
    return node;
  }

  // The lines and value of a dimension, in model units, so the editor and the plot agree.
  function dimensionGeometry(kind) {
    const [sx, sy] = kind.start;
    const [ex, ey] = kind.end;
    const dx = ex - sx;
    const dy = ey - sy;
    const length = Math.hypot(dx, dy) || 1e-9;
    const nx = -dy / length;
    const ny = dx / length;
    const a = [sx + nx * kind.offset, sy + ny * kind.offset];
    const b = [ex + nx * kind.offset, ey + ny * kind.offset];
    const tick = Math.max(length * 0.02, 0.5);
    return {
      lines: [
        [kind.start, a],
        [kind.end, b],
        [a, b],
        [[a[0] - tick, a[1] - tick], [a[0] + tick, a[1] + tick]],
        [[b[0] - tick, b[1] - tick], [b[0] + tick, b[1] + tick]],
      ],
      mid: [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2],
      value: kind.text || formatNumber(length),
    };
  }

  function radialDimensionGeometry(kind) {
    const [cx, cy] = kind.center;
    const [tx, ty] = kind.target;
    const dx = tx - cx;
    const dy = ty - cy;
    const r = Math.hypot(dx, dy) || 1e-9;
    const ux = dx / r;
    const uy = dy / r;
    const offset = kind.leader_offset != null ? kind.leader_offset : 15.0;
    const elbow = [tx + ux * offset, ty + uy * offset];
    const shoulderDx = ux >= 0 ? 6.0 : -6.0;
    const shoulder = [elbow[0] + shoulderDx, elbow[1]];
    const tick = Math.min(Math.max(r * 0.05, 0.5), 3.0);
    const nx = -uy * tick * 0.5;
    const ny = ux * tick * 0.5;

    const lines = [];
    if (kind.is_diameter) {
      lines.push([[cx - ux * r, cy - uy * r], [tx, ty]]);
    } else {
      lines.push([[cx, cy], [tx, ty]]);
    }
    lines.push([[tx, ty], elbow]);
    lines.push([elbow, shoulder]);
    lines.push([
      [tx - ux * tick + nx, ty - uy * tick + ny],
      [tx, ty],
    ]);
    lines.push([
      [tx - ux * tick - nx, ty - uy * tick - ny],
      [tx, ty],
    ]);

    const valStr = kind.is_diameter ? `Ø ${(r * 2).toFixed(2)}` : `R ${r.toFixed(2)}`;
    return {
      lines,
      textAt: [Math.min(elbow[0], shoulder[0]) + 1.0, shoulder[1] + 1.0],
      value: kind.text || valStr,
    };
  }

  function findObjectSnap(draft, mousePoint, basePoint, scale, aperturePx = 12) {
    if (!draft || !Array.isArray(draft.entities)) return null;
    const maxDist = aperturePx / Math.max(scale, 1e-6);
    const entities = draft.entities;
    const layers = draft.layers || [];

    const isVisible = (layerName) => {
      const layer = layers.find((l) => l.name === layerName);
      return !layer || layer.visible !== false;
    };

    const dist = (p1, p2) => Math.hypot(p1[0] - p2[0], p1[1] - p2[1]);

    const endpoints = [];
    const midpoints = [];
    const centers = [];
    const quadrants = [];
    const nodes = [];
    const segments = [];
    const circles = [];

    for (const entity of entities) {
      if (!isVisible(entity.layer)) continue;
      const k = entity.kind;
      switch (k.kind) {
        case "point":
          nodes.push(k.at);
          break;
        case "line":
          endpoints.push(k.start, k.end);
          midpoints.push([(k.start[0] + k.end[0]) / 2, (k.start[1] + k.end[1]) / 2]);
          segments.push([k.start, k.end]);
          break;
        case "polyline":
          for (let i = 0; i < k.points.length; i++) {
            endpoints.push(k.points[i]);
            if (i + 1 < k.points.length) {
              midpoints.push([(k.points[i][0] + k.points[i + 1][0]) / 2, (k.points[i][1] + k.points[i + 1][1]) / 2]);
              segments.push([k.points[i], k.points[i + 1]]);
            }
          }
          if (k.closed && k.points.length > 2) {
            const p0 = k.points[0];
            const pLast = k.points[k.points.length - 1];
            midpoints.push([(p0[0] + pLast[0]) / 2, (p0[1] + pLast[1]) / 2]);
            segments.push([pLast, p0]);
          }
          break;
        case "circle":
          centers.push(k.center);
          quadrants.push(
            [k.center[0] + k.radius, k.center[1]],
            [k.center[0] - k.radius, k.center[1]],
            [k.center[0], k.center[1] + k.radius],
            [k.center[0], k.center[1] - k.radius]
          );
          circles.push({ center: k.center, radius: k.radius });
          break;
        case "arc": {
          centers.push(k.center);
          const [cx, cy] = k.center;
          const a0 = (k.start_degrees * Math.PI) / 180;
          const a1 = (k.end_degrees * Math.PI) / 180;
          endpoints.push(
            [cx + k.radius * Math.cos(a0), cy + k.radius * Math.sin(a0)],
            [cx + k.radius * Math.cos(a1), cy + k.radius * Math.sin(a1)]
          );
          const sweep = ((k.end_degrees - k.start_degrees) % 360 + 360) % 360;
          const aMid = ((k.start_degrees + sweep / 2) * Math.PI) / 180;
          midpoints.push([cx + k.radius * Math.cos(aMid), cy + k.radius * Math.sin(aMid)]);
          for (const card of [0, 90, 180, 270]) {
            if (((card - k.start_degrees) % 360 + 360) % 360 <= sweep) {
              const rad = (card * Math.PI) / 180;
              quadrants.push([cx + k.radius * Math.cos(rad), cy + k.radius * Math.sin(rad)]);
            }
          }
          circles.push({ center: k.center, radius: k.radius });
          break;
        }
        case "rectangle": {
          const [x, y] = k.origin;
          const { width, height } = k;
          const p0 = [x, y];
          const p1 = [x + width, y];
          const p2 = [x + width, y + height];
          const p3 = [x, y + height];
          endpoints.push(p0, p1, p2, p3);
          midpoints.push(
            [(p0[0] + p1[0]) / 2, (p0[1] + p1[1]) / 2],
            [(p1[0] + p2[0]) / 2, (p1[1] + p2[1]) / 2],
            [(p2[0] + p3[0]) / 2, (p2[0] + p3[1]) / 2],
            [(p3[0] + p0[0]) / 2, (p3[1] + p0[1]) / 2]
          );
          segments.push([p0, p1], [p1, p2], [p2, p3], [p3, p0]);
          break;
        }
        case "ellipse": {
          centers.push(k.center);
          quadrants.push(
            [k.center[0] + k.radius_x, k.center[1]],
            [k.center[0] - k.radius_x, k.center[1]],
            [k.center[0], k.center[1] + k.radius_y],
            [k.center[0], k.center[1] - k.radius_y]
          );
          break;
        }
        case "spline": {
          if (k.points && k.points.length >= 2) {
            endpoints.push(k.points[0], k.points[k.points.length - 1]);
            const sampled = sampleSplinePoints(k.points, k.closed, 16);
            for (let i = 0; i < sampled.length - 1; i++) {
              segments.push([sampled[i], sampled[i + 1]]);
            }
          }
          break;
        }
      }
    }

    const override = drawState.snapOverride;
    if (override) {
      if (override === "Endpoint") {
        for (const pt of endpoints) if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Endpoint", symbol: "square" };
        return null;
      }
      if (override === "Midpoint") {
        for (const pt of midpoints) if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Midpoint", symbol: "triangle" };
        return null;
      }
      if (override === "Center") {
        for (const pt of centers) if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Center", symbol: "circle" };
        return null;
      }
      if (override === "Quadrant") {
        for (const pt of quadrants) if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Quadrant", symbol: "diamond" };
        return null;
      }
      if (override === "Intersection") {
        for (let i = 0; i < segments.length; i++) {
          for (let j = i + 1; j < segments.length; j++) {
            const [a, b] = segments[i];
            const [c, d] = segments[j];
            const denom = (b[0] - a[0]) * (d[1] - c[1]) - (b[1] - a[1]) * (d[0] - c[0]);
            if (Math.abs(denom) > 1e-9) {
              const t = ((c[0] - a[0]) * (d[1] - c[1]) - (c[1] - a[1]) * (d[0] - c[0])) / denom;
              const u = ((c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])) / denom;
              if (t >= -1e-5 && t <= 1 + 1e-5 && u >= -1e-5 && u <= 1 + 1e-5) {
                const ix = a[0] + t * (b[0] - a[0]);
                const iy = a[1] + t * (b[1] - a[1]);
                const ipt = [ix, iy];
                if (dist(ipt, mousePoint) <= maxDist) return { point: ipt, kind: "Intersection", symbol: "cross" };
              }
            }
          }
        }
        return null;
      }
      if (override === "Perpendicular" && basePoint) {
        for (const [a, b] of segments) {
          const dx = b[0] - a[0];
          const dy = b[1] - a[1];
          const lenSq = dx * dx + dy * dy;
          if (lenSq > 1e-9) {
            const t = ((basePoint[0] - a[0]) * dx + (basePoint[1] - a[1]) * dy) / lenSq;
            if (t >= 0 && t <= 1) {
              const foot = [a[0] + t * dx, a[1] + t * dy];
              if (dist(foot, mousePoint) <= maxDist) return { point: foot, kind: "Perpendicular", symbol: "perpendicular" };
            }
          }
        }
        return null;
      }
      if (override === "Tangent" && basePoint) {
        for (const circ of circles) {
          const [cx, cy] = circ.center;
          const r = circ.radius;
          const d = Math.hypot(basePoint[0] - cx, basePoint[1] - cy);
          if (d > r + 1e-6) {
            const alpha = Math.atan2(basePoint[1] - cy, basePoint[0] - cx);
            const beta = Math.acos(r / d);
            const tan1 = [cx + r * Math.cos(alpha + beta), cy + r * Math.sin(alpha + beta)];
            const tan2 = [cx + r * Math.cos(alpha - beta), cy + r * Math.sin(alpha - beta)];
            for (const tp of [tan1, tan2]) {
              if (dist(tp, mousePoint) <= maxDist) return { point: tp, kind: "Tangent", symbol: "tangent" };
            }
          }
        }
        return null;
      }
      if (override === "Node") {
        for (const pt of nodes) if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Node", symbol: "node" };
        return null;
      }
      if (override === "Nearest") {
        for (const [a, b] of segments) {
          const dx = b[0] - a[0];
          const dy = b[1] - a[1];
          const lenSq = dx * dx + dy * dy;
          if (lenSq > 1e-9) {
            const t = Math.max(0, Math.min(1, ((mousePoint[0] - a[0]) * dx + (mousePoint[1] - a[1]) * dy) / lenSq));
            const nearPt = [a[0] + t * dx, a[1] + t * dy];
            if (dist(nearPt, mousePoint) <= maxDist) return { point: nearPt, kind: "Nearest", symbol: "hourglass" };
          }
        }
        return null;
      }
    }

    // 1. Endpoint
    for (const pt of endpoints) {
      if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Endpoint", symbol: "square" };
    }
    // 2. Midpoint
    for (const pt of midpoints) {
      if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Midpoint", symbol: "triangle" };
    }
    // 3. Center
    for (const pt of centers) {
      if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Center", symbol: "circle" };
    }
    // 4. Quadrant
    for (const pt of quadrants) {
      if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Quadrant", symbol: "diamond" };
    }

    // 5. Intersection
    for (let i = 0; i < segments.length; i++) {
      for (let j = i + 1; j < segments.length; j++) {
        const [a, b] = segments[i];
        const [c, d] = segments[j];
        const denom = (b[0] - a[0]) * (d[1] - c[1]) - (b[1] - a[1]) * (d[0] - c[0]);
        if (Math.abs(denom) > 1e-9) {
          const t = ((c[0] - a[0]) * (d[1] - c[1]) - (c[1] - a[1]) * (d[0] - c[0])) / denom;
          const u = ((c[0] - a[0]) * (b[1] - a[1]) - (c[1] - a[1]) * (b[0] - a[0])) / denom;
          if (t >= -1e-5 && t <= 1 + 1e-5 && u >= -1e-5 && u <= 1 + 1e-5) {
            const ix = a[0] + t * (b[0] - a[0]);
            const iy = a[1] + t * (b[1] - a[1]);
            const ipt = [ix, iy];
            if (dist(ipt, mousePoint) <= maxDist) return { point: ipt, kind: "Intersection", symbol: "cross" };
          }
        }
      }
    }

    // 6. Perpendicular (relative to basePoint)
    if (basePoint) {
      for (const [a, b] of segments) {
        const dx = b[0] - a[0];
        const dy = b[1] - a[1];
        const lenSq = dx * dx + dy * dy;
        if (lenSq > 1e-9) {
          const t = ((basePoint[0] - a[0]) * dx + (basePoint[1] - a[1]) * dy) / lenSq;
          if (t >= 0 && t <= 1) {
            const foot = [a[0] + t * dx, a[1] + t * dy];
            if (dist(foot, mousePoint) <= maxDist) return { point: foot, kind: "Perpendicular", symbol: "perpendicular" };
          }
        }
      }
    }

    // 7. Tangent (relative to basePoint)
    if (basePoint) {
      for (const circ of circles) {
        const [cx, cy] = circ.center;
        const r = circ.radius;
        const d = Math.hypot(basePoint[0] - cx, basePoint[1] - cy);
        if (d > r + 1e-6) {
          const alpha = Math.atan2(basePoint[1] - cy, basePoint[0] - cx);
          const beta = Math.acos(r / d);
          const tan1 = [cx + r * Math.cos(alpha + beta), cy + r * Math.sin(alpha + beta)];
          const tan2 = [cx + r * Math.cos(alpha - beta), cy + r * Math.sin(alpha - beta)];
          for (const tp of [tan1, tan2]) {
            if (dist(tp, mousePoint) <= maxDist) return { point: tp, kind: "Tangent", symbol: "tangent" };
          }
        }
      }
    }

    // 8. Node
    for (const pt of nodes) {
      if (dist(pt, mousePoint) <= maxDist) return { point: pt, kind: "Node", symbol: "node" };
    }

    // 9. Nearest
    let bestNearest = null;
    let minNearDist = maxDist;
    for (const [a, b] of segments) {
      const dx = b[0] - a[0];
      const dy = b[1] - a[1];
      const lenSq = dx * dx + dy * dy;
      if (lenSq > 1e-9) {
        const t = Math.max(0, Math.min(1, ((mousePoint[0] - a[0]) * dx + (mousePoint[1] - a[1]) * dy) / lenSq));
        const nearPt = [a[0] + t * dx, a[1] + t * dy];
        const d = dist(nearPt, mousePoint);
        if (d <= minNearDist) {
          minNearDist = d;
          bestNearest = nearPt;
        }
      }
    }
    if (bestNearest) return { point: bestNearest, kind: "Nearest", symbol: "hourglass" };

    return null;
  }

  function findPolarTracking(basePoint, mousePoint, scale, aperturePx = 12) {
    if (!basePoint) return null;
    const dx = mousePoint[0] - basePoint[0];
    const dy = mousePoint[1] - basePoint[1];
    const dist = Math.hypot(dx, dy);
    if (dist * scale < aperturePx) return null;

    const angleDeg = ((Math.atan2(dy, dx) * 180) / Math.PI + 360) % 360;
    const standardAngles = [0, 45, 90, 135, 180, 225, 270, 315];
    const tolDeg = 3.5;

    for (const target of standardAngles) {
      let diff = Math.abs(angleDeg - target);
      if (diff > 180) diff = 360 - diff;
      if (diff <= tolDeg) {
        const rad = (target * Math.PI) / 180;
        const snapPoint = [
          basePoint[0] + dist * Math.cos(rad),
          basePoint[1] + dist * Math.sin(rad),
        ];
        return {
          point: snapPoint,
          angleDeg: target,
          dist: Math.round(dist * 10) / 10,
          base: basePoint,
        };
      }
    }
    return null;
  }

  function updateSnapOverlay(svg) {
    if (!svg) return;
    const snapFlip = svg.querySelector("#drawingSnapFlip");
    const snapUpright = svg.querySelector("#drawingSnapUpright");
    if (!snapFlip || !snapUpright) return;

    snapFlip.textContent = "";
    snapUpright.textContent = "";

    const box = svg.viewBox.baseVal;
    const rect = svg.getBoundingClientRect();
    const scale = Math.min(rect.width / box.width, rect.height / box.height) || 1;
    const size = 12 / scale;

    if (drawState.polarHit) {
      const p = drawState.polarHit;
      const [bx, by] = p.base;
      const [sx, sy] = p.point;
      const dirX = sx - bx;
      const dirY = sy - by;
      const len = Math.hypot(dirX, dirY) || 1;
      const ext = Math.max(box.width, box.height) * 2;
      const ex = bx + (dirX / len) * ext;
      const ey = by + (dirY / len) * ext;

      snapFlip.append(svgNode("line", {
        x1: bx,
        y1: by,
        x2: ex,
        y2: ey,
        class: "drawing-polar-line",
      }));

      const text = svgNode("text", {
        x: sx + 14 / scale,
        y: -sy - 6 / scale,
        class: "drawing-snap-label",
      });
      text.textContent = `Polar: ${p.angleDeg}° (${p.dist})`;
      snapUpright.append(text);
    }

    if (drawState.activeSnap) {
      const s = drawState.activeSnap;
      const [px, py] = s.point;

      let markerNode;
      switch (s.symbol) {
        case "square":
          markerNode = svgNode("rect", {
            x: px - size / 2,
            y: py - size / 2,
            width: size,
            height: size,
            class: "drawing-snap-marker",
          });
          break;
        case "triangle":
          markerNode = svgNode("polygon", {
            points: `${px},${py + size / 2} ${px - size / 2},${py - size / 2} ${px + size / 2},${py - size / 2}`,
            class: "drawing-snap-marker",
          });
          break;
        case "circle":
          markerNode = svgNode("circle", {
            cx: px,
            cy: py,
            r: size / 2,
            class: "drawing-snap-marker",
          });
          break;
        case "diamond":
          markerNode = svgNode("polygon", {
            points: `${px},${py + size / 2} ${px + size / 2},${py} ${px},${py - size / 2} ${px - size / 2},${py}`,
            class: "drawing-snap-marker",
          });
          break;
        case "cross":
          markerNode = svgNode("path", {
            d: `M ${px - size / 2} ${py - size / 2} L ${px + size / 2} ${py + size / 2} M ${px + size / 2} ${py - size / 2} L ${px - size / 2} ${py + size / 2}`,
            class: "drawing-snap-marker",
          });
          break;
        default:
          markerNode = svgNode("rect", {
            x: px - size / 2,
            y: py - size / 2,
            width: size,
            height: size,
            class: "drawing-snap-marker",
          });
          break;
      }
      snapFlip.append(markerNode);

      const label = svgNode("text", {
        x: px + 12 / scale,
        y: -py - 6 / scale,
        class: "drawing-snap-label",
      });
      label.textContent = s.kind;
      snapUpright.append(label);
    }
  }

  function renderDrawing(draft) {
    const preview = byId("drawingPreview");
    preview.textContent = "";
    const entities = Array.isArray(draft.entities) ? draft.entities : [];
    const layers = Array.isArray(draft.layers) ? draft.layers : [];
    byId("drawingLabel").textContent = `${draft.unit} · ${draft.source === "file" ? "saved" : "plan"}`;
    byId("drawingEntities").textContent = `${entities.length}`;
    byId("drawingLayers").textContent = layers.length ? layers.map((l) => l.name).join(", ") : "—";
    byId("drawingLength").textContent = `${formatNumber(draft.total_length)} ${draft.unit}`;
    byId("drawingSource").textContent = draft.source === "file"
      ? `${draft.file ? draft.file.split(/[\\/]/).pop() : "file"}${draft.dirty ? " (unsaved)" : ""}`
      : "model (live)";
    byId("drawingHint").textContent = draft.source === "file"
      ? "Saved beside the project. The model no longer changes it — edit it here and save again."
      : "Follows the model until you draw on it or save it, then it is yours.";
    syncDrawingControls();

    const bounds = draft.bounds;
    if (!entities.length || !Array.isArray(bounds)) {
      byId("drawingExtents").textContent = "—";
      preview.append(element("span", "hint", "This drawing is empty. Pick a tool and click to draw."));
      return;
    }

    const [minX, minY, maxX, maxY] = bounds;
    const width = Math.max(maxX - minX, 1e-9);
    const height = Math.max(maxY - minY, 1e-9);
    const unitSize = Math.max(width, height);
    const pad = unitSize * 0.08;
    byId("drawingExtents").textContent = `${formatNumber(width)} × ${formatNumber(height)} ${draft.unit}`;

    const baseW = width + pad * 2;
    const baseH = height + pad * 2;
    const zoom = drawState.viewZoom || 1.0;
    const pan = drawState.viewPan || [0, 0];
    const zoomW = baseW / zoom;
    const zoomH = baseH / zoom;
    const centerX = (minX + maxX) / 2 + pan[0];
    const centerY = -(minY + maxY) / 2 + pan[1];

    const svg = svgNode("svg", {
      // The document is y-up and SVG is y-down, so the geometry group is mirrored and the viewBox
      // covers the mirrored range. Text is placed in a second, upright group.
      viewBox: `${centerX - zoomW / 2} ${centerY - zoomH / 2} ${zoomW} ${zoomH}`,
      preserveAspectRatio: "xMidYMid meet",
      role: "application",
      "aria-label": `Drawing of ${draft.name}`,
      class: drawState.tool === "select" ? "select-mode" : "",
    });
    const flip = svgNode("g", { transform: "scale(1,-1)" });
    const upright = svgNode("g", {});

    for (const entity of entities) {
      const layer = layers.find((l) => l.name === entity.layer);
      if (layer && layer.visible === false) continue;
      const selected = drawState.selected === entity.id;
      const kind = entity.kind;
      if (kind.kind === "text") {
        const node = svgNode("text", {
          x: kind.at[0],
          y: -kind.at[1],
          "font-size": Math.max(kind.height, unitSize * 0.02),
          "data-entity-id": entity.id,
          class: "drawing-text",
          fill: selected ? "var(--warning)" : "",
        });
        node.textContent = kind.value;
        upright.append(node);
        continue;
      }
      if (kind.kind === "dimension") {
        const geometry = dimensionGeometry(kind);
        for (const [from, to] of geometry.lines) {
          flip.append(svgNode("path", {
            d: `M ${from[0]} ${from[1]} L ${to[0]} ${to[1]}`,
            "data-entity-id": entity.id,
            class: `drawing-dimension${selected ? " selected" : ""}`,
          }));
        }
        const label = svgNode("text", {
          x: geometry.mid[0],
          y: -geometry.mid[1],
          "font-size": Math.max(unitSize * 0.035, 1e-6),
          "text-anchor": "middle",
          "data-entity-id": entity.id,
          class: "drawing-text",
        });
        label.textContent = geometry.value;
        upright.append(label);
        continue;
      }
      if (kind.kind === "radial_dimension") {
        const geometry = radialDimensionGeometry(kind);
        for (const [from, to] of geometry.lines) {
          flip.append(svgNode("path", {
            d: `M ${from[0]} ${from[1]} L ${to[0]} ${to[1]}`,
            "data-entity-id": entity.id,
            class: `drawing-dimension${selected ? " selected" : ""}`,
          }));
        }
        const label = svgNode("text", {
          x: geometry.textAt[0],
          y: -geometry.textAt[1],
          "font-size": Math.max(unitSize * 0.035, 1e-6),
          "text-anchor": "start",
          "data-entity-id": entity.id,
          class: "drawing-text",
        });
        label.textContent = geometry.value;
        upright.append(label);
        continue;
      }
      if (kind.kind === "point") {
        flip.append(svgNode("circle", {
          cx: kind.at[0],
          cy: kind.at[1],
          r: unitSize * 0.01,
          "data-entity-id": entity.id,
          class: "drawing-point",
        }));
        continue;
      }
      if (kind.kind === "insert") {
        const blocks = (drawing && drawing.blocks) || [];
        const block = blocks.find((b) => (b.name || "").toLowerCase() === (kind.block_name || "").toLowerCase());
        if (block && Array.isArray(block.entities)) {
          const [posx, posy] = kind.position || [0, 0];
          const [sx, sy] = kind.scale || [1, 1];
          const rot = kind.rotation_degrees || 0;
          const [bx, by] = block.base_point || [0, 0];
          const group = svgNode("g", {
            transform: `translate(${posx}, ${posy}) rotate(${rot}) scale(${sx}, ${sy}) translate(${-bx}, ${-by})`,
            "data-entity-id": entity.id,
            class: `drawing-insert${selected ? " selected" : ""}`,
          });
          for (const child of block.entities) {
            const childD = drawingPath({ kind: child });
            if (childD) {
              group.append(svgNode("path", {
                d: childD,
                class: `drawing-entity${selected ? " selected" : ""}`,
              }));
            }
          }
          flip.append(group);
        }
        continue;
      }
      const d = drawingPath(entity);
      if (!d) continue;
      const strokeDash = layer && layer.linetype ? ({
        hidden: "3,1.5",
        center: "8,1.5,1.5,1.5",
        phantom: "10,1.5,1.5,1.5,1.5,1.5",
        dashed: "5,2",
        dot: "0.5,1.5",
      }[String(layer.linetype).toLowerCase()] || "") : "";
      const pathAttrs = {
        d,
        "data-entity-id": entity.id,
        class: `drawing-entity${selected ? " selected" : ""}`,
      };
      if (strokeDash) {
        pathAttrs["stroke-dasharray"] = strokeDash;
      }
      flip.append(svgNode("path", pathAttrs));
    }

    // The shape being drawn: its points and the segments between them.
    if (drawState.points.length) {
      const d = drawState.points.map((p, i) => `${i ? "L" : "M"} ${p[0]} ${p[1]}`).join(" ");
      if (drawState.points.length > 1) flip.append(svgNode("path", { d, class: "drawing-overlay" }));
      for (const point of drawState.points) {
        flip.append(svgNode("circle", {
          cx: point[0],
          cy: point[1],
          r: unitSize * 0.012,
          class: "drawing-overlay-point",
        }));
      }
    }

    const snapLayer = svgNode("g", { id: "drawingSnapLayer" });
    const snapFlip = svgNode("g", { id: "drawingSnapFlip", transform: "scale(1,-1)" });
    const snapUpright = svgNode("g", { id: "drawingSnapUpright" });
    snapLayer.append(snapFlip, snapUpright);

    svg.append(flip, upright, snapLayer);
    preview.append(svg);
    updateSnapOverlay(svg);
  }

  // ---------------------------------------------------------------------------------------
  // Drawing editor
  // ---------------------------------------------------------------------------------------

  function drawingLayer(name) {
    const draft = app.drawing;
    if (!draft || !Array.isArray(draft.layers)) return null;
    return draft.layers.find((layer) => layer.name === name) || null;
  }

  function syncDrawingControls() {
    const draft = app.drawing;
    const entities = draft && Array.isArray(draft.entities) ? draft.entities : [];
    const layers = draft && Array.isArray(draft.layers) ? draft.layers : [];
    for (const button of document.querySelectorAll(".draw-tools [data-tool]")) {
      button.setAttribute("aria-pressed", String(button.dataset.tool === drawState.tool));
    }
    const select = byId("layerSelect");
    const names = layers.map((layer) => layer.name);
    const same = select.options.length === names.length
      && names.every((name, index) => select.options[index].value === name);
    if (!same) {
      select.textContent = "";
      for (const layer of layers) {
        const option = document.createElement("option");
        option.value = layer.name;
        option.textContent = layer.visible === false ? `${layer.name} (off)` : layer.name;
        select.append(option);
      }
    }
    const current = drawState.layer || (names.length ? names[0] : null);
    if (current) select.value = current;
    const layer = drawingLayer(current);
    const visible = byId("layerVisibleBtn");
    visible.textContent = layer && layer.visible === false ? "Off" : "On";
    visible.setAttribute("aria-pressed", String(Boolean(layer) && layer.visible !== false));
    const locked = byId("layerLockBtn");
    locked.textContent = layer && layer.locked ? "Locked" : "Unlocked";
    locked.setAttribute("aria-pressed", String(Boolean(layer) && layer.locked));
    const linetypeSelect = byId("layerLinetypeSelect");
    if (linetypeSelect && layer) {
      const lt = typeof layer.linetype === "string"
        ? layer.linetype
        : (layer.linetype && typeof layer.linetype === "object" && layer.linetype.kind ? layer.linetype.kind : "continuous");
      linetypeSelect.value = (lt || "continuous").toLowerCase();
    }
    byId("polylineActions").hidden =
      (drawState.tool !== "polyline" && drawState.tool !== "spline") || drawState.points.length === 0;
    byId("saveDrawingBtn").disabled = app.busy || !draft;
    byId("downloadDxfBtn").disabled = app.busy || entities.length === 0;
    byId("plotPdfBtn").disabled = app.busy || entities.length === 0;
    byId("drawUndoBtn").disabled = app.busy || !draft || !draft.undo;
    byId("drawDeleteBtn").disabled = app.busy || drawState.selected === null;
    const selected = drawState.selected === null
      ? null
      : entities.find((entity) => entity.id === drawState.selected) || null;
    byId("drawingSelection").textContent = selected
      ? `#${selected.id} ${selected.kind.kind}`
      : "nothing";
  }

  // Screen → model. The viewBox is mirrored in y and letterboxed by `xMidYMid meet`, so the scale and
  // offsets are measured rather than assumed: a wrong mapping would place geometry somewhere the
  // user did not click, which is worse than refusing to draw.
  function modelPointFromEvent(svg, event) {
    const box = svg.viewBox.baseVal;
    const rect = svg.getBoundingClientRect();
    if (box.width <= 0 || box.height <= 0 || rect.width <= 0 || rect.height <= 0) return null;
    const scale = Math.min(rect.width / box.width, rect.height / box.height);
    const offsetX = rect.left + (rect.width - box.width * scale) / 2;
    const offsetY = rect.top + (rect.height - box.height * scale) / 2;
    const viewX = box.x + (event.clientX - offsetX) / scale;
    const viewY = box.y + (event.clientY - offsetY) / scale;
    const round = (value) => Math.round(value * 1000) / 1000;
    return [round(viewX), round(-viewY)];
  }

  function setDrawTool(tool) {
    drawState.tool = tool;
    drawState.points = [];
    renderDrawing(app.drawing || { entities: [], layers: [], bounds: null, unit: "mm" });
    if (tool !== "select") setStatus(`${tool} tool: click in the drawing`, "");
  }

  async function placeDrawingPoint(point) {
    if (!drawState.layer) {
      toast("This drawing has no layer to draw on.", true);
      return;
    }
    const tool = drawState.tool;
    if (tool === "point") {
      await drawingMutate("/api/draft/entity", {
        layer: drawState.layer,
        kind: { kind: "point", at: point },
      }, "Point added");
      return;
    }
    if (tool === "text") {
      const value = byId("drawText").value.trim();
      if (!value) {
        toast("Type the text in the Text field first.", true);
        return;
      }
      await drawingMutate("/api/draft/entity", {
        layer: drawState.layer,
        kind: { kind: "text", at: point, height: 3.5, value },
      }, "Text added");
      return;
    }
    if (tool === "dimension") {
      drawState.points.push(point);
      if (drawState.points.length < 2) {
        renderDrawing(app.drawing);
        return;
      }
      const [start, end] = drawState.points;
      drawState.points = [];
      const length = Math.hypot(end[0] - start[0], end[1] - start[1]);
      await drawingMutate("/api/draft/entity", {
        layer: drawState.layer,
        kind: { kind: "dimension", start, end, offset: Math.max(length * 0.15, 0.5), text: null },
      }, "Dimension added");
      return;
    }
    if (tool === "polyline" || tool === "spline") {
      drawState.points.push(point);
      renderDrawing(app.drawing);
      return;
    }
    // Line, circle, rectangle, and ellipse take two clicks.
    drawState.points.push(point);
    if (drawState.points.length < 2) {
      renderDrawing(app.drawing);
      return;
    }
    const [first, second] = drawState.points;
    drawState.points = [];
    if (tool === "line") {
      await drawingMutate("/api/draft/entity", {
        layer: drawState.layer,
        kind: { kind: "line", start: first, end: second },
      }, "Line added");
    } else if (tool === "circle") {
      const radius = Math.hypot(second[0] - first[0], second[1] - first[1]);
      if (!(radius > 0)) {
        toast("A circle needs a point away from its centre.", true);
        renderDrawing(app.drawing);
        return;
      }
      await drawingMutate("/api/draft/entity", {
        layer: drawState.layer,
        kind: { kind: "circle", center: first, radius: Math.round(radius * 1000) / 1000 },
      }, "Circle added");
    } else if (tool === "rectangle") {
      const origin = [Math.min(first[0], second[0]), Math.min(first[1], second[1])];
      const width = Math.abs(second[0] - first[0]);
      const height = Math.abs(second[1] - first[1]);
      if (width > 0 && height > 0) {
        await drawingMutate("/api/draft/entity", {
          layer: drawState.layer,
          kind: { kind: "rectangle", origin, width, height },
        }, "Rectangle added");
      }
    } else if (tool === "ellipse") {
      const radius_x = Math.abs(second[0] - first[0]);
      const radius_y = Math.abs(second[1] - first[1]) || (radius_x / 2);
      if (radius_x > 0 && radius_y > 0) {
        await drawingMutate("/api/draft/entity", {
          layer: drawState.layer,
          kind: { kind: "ellipse", center: first, radius_x, radius_y },
        }, "Ellipse added");
      }
    }
  }

  async function finishPolyline(closed) {
    if (drawState.points.length < 2) {
      toast(`${drawState.tool === "spline" ? "A spline" : "A polyline"} needs at least two points.`, true);
      return;
    }
    const points = drawState.points;
    const isSpline = drawState.tool === "spline";
    drawState.points = [];
    const kind = isSpline
      ? { kind: "spline", points, closed }
      : { kind: "polyline", points, closed };
    await drawingMutate("/api/draft/entity", {
      layer: drawState.layer,
      kind,
    }, closed ? `Closed ${isSpline ? "spline" : "polyline"} added` : `${isSpline ? "Spline" : "Polyline"} added`);
  }

  async function deleteSelected() {
    if (drawState.selected === null) {
      toast("Select an entity first.", true);
      return;
    }
    const id = drawState.selected;
    drawState.selected = null;
    await drawingMutate("/api/draft/remove", { id }, `Entity ${id} deleted`);
  }

  async function setLayerFlag(flag, value) {
    const name = byId("layerSelect").value;
    if (!name) return;
    await drawingMutate("/api/draft/layer", { name, [flag]: value }, null);
  }

  async function addLayer() {
    const input = byId("layerName");
    const name = input.value.trim();
    if (!name) {
      toast("A layer needs a name.", true);
      return;
    }
    const payload = await drawingMutate("/api/draft/layer", { name, color: 7 }, `Layer ${name} added`);
    if (payload) {
      input.value = "";
      drawState.layer = name;
      renderDrawing(payload);
    }
  }

  async function saveDrawing() {
    const payload = await drawingMutate("/api/draft/save", {}, null);
    if (payload) {
      log(`Drawing saved to ${payload.file}`);
      toast("Drawing saved beside the project.");
    }
  }

  async function plotPdf() {
    const params = new URLSearchParams();
    params.set("paper", byId("sheetPaper").value);
    params.set("orientation", byId("sheetOrientation").value);
    const scale = byId("sheetScale").value.trim();
    if (scale) params.set("scale", scale);
    for (const [id, name] of [["sheetTitle", "title"], ["sheetNumber", "number"], ["sheetAuthor", "author"]]) {
      const value = byId(id).value.trim();
      if (value) params.set(name, value);
    }
    if (!byId("sheetDimensions").checked) params.set("dimensions", "0");
    try {
      const headers = new Headers();
      if (shareToken) headers.set("x-atlas-token", shareToken);
      const response = await fetch(`/api/export/pdf?${params.toString()}`, { headers });
      if (!response.ok) {
        const payload = await response.json().catch(() => null);
        throw new ApiError(
          payload && payload.error ? payload.error.message : `Plot failed with status ${response.status}.`,
          "export_failure",
          response.status
        );
      }
      const blob = await response.blob();
      const stem = app.project ? app.project.name : "drawing";
      downloadBlob(blob, `${stem.replace(/[^A-Za-z0-9_-]+/g, "_") || "drawing"}-${byId("sheetPaper").value.toLowerCase()}.pdf`);
      log("Plotted a PDF sheet");
      toast("PDF download started.");
    } catch (error) {
      toast(error.message, true);
      log(`Plot failed: ${error.message}`, "error");
    }
  }

  async function exportDxf() {
    try {
      const headers = new Headers();
      if (shareToken) headers.set("x-atlas-token", shareToken);
      const response = await fetch("/api/export/dxf", { headers });
      if (!response.ok) {
        const payload = await response.json().catch(() => null);
        throw new ApiError(
          payload && payload.error ? payload.error.message : `Export failed with status ${response.status}.`,
          "export_failure",
          response.status
        );
      }
      const blob = await response.blob();
      const stem = app.project ? app.project.name : "drawing";
      downloadBlob(blob, `${stem.replace(/[^A-Za-z0-9_-]+/g, "_") || "drawing"}-plan.dxf`);
      log("Exported DXF R12 plan");
      toast("DXF download started.");
    } catch (error) {
      toast(error.message, true);
      log(`DXF export failed: ${error.message}`, "error");
    }
  }

  async function setProfile(kind) {
    const sketch = activeSketch();
    if (!sketch) {
      toast("This project has no sketch to change.", true);
      return;
    }
    const body = { id: sketch.id, kind };
    if (kind === "regular_polygon") {
      const sides = Number(byId("sidesInput").value);
      if (!Number.isInteger(sides) || sides < 3 || sides > 256) {
        toast("A polygon needs an integer side count from 3 to 256.", true);
        return;
      }
      body.sides = sides;
      if (sketch.profile.kind === kind && sketch.profile.sides === sides) return;
    } else if (kind === "sector") {
      const start = Number(byId("sectorStart").value);
      const sweep = Number(byId("sectorSweep").value);
      if (!Number.isFinite(start)) {
        toast("A sector start angle needs a finite number of degrees.", true);
        return;
      }
      if (!Number.isFinite(sweep) || sweep <= 0 || sweep >= 360) {
        toast("A sector sweep must be between 0 and 360 degrees; use a circle for a full turn.", true);
        return;
      }
      body.start_degrees = start;
      body.sweep_degrees = sweep;
      if (
        sketch.profile.kind === kind &&
        sketch.profile.start_degrees === start &&
        sketch.profile.sweep_degrees === sweep
      ) {
        return;
      }
    } else if (sketch.profile.kind === kind) {
      return;
    }
    await mutate("/api/sketch/profile", body, `Profile changed to ${profileName(kind)}`);
  }

  function newestId(list) {
    let newest = null;
    for (const item of list) {
      if (newest === null || item.id > newest) newest = item.id;
    }
    return newest;
  }

  // The entity just created is the one with the largest ID, because IDs are allocated in order.
  function selectNewest(kind, payload) {
    const list = kind === "sketch" ? payload.sketches : payload.features;
    const id = newestId(list);
    if (id !== null) select(kind, id);
  }

  function originFrom(xId, yId) {
    const x = Number(byId(xId).value);
    const y = Number(byId(yId).value);
    if (!Number.isFinite(x) || !Number.isFinite(y)) return null;
    return [x, y];
  }

  async function addSketch() {
    const kind = byId("newSketchKind").value;
    if (kind === "polyline") {
      startDrawing();
      return;
    }
    const origin = originFrom("newOriginX", "newOriginY");
    if (!origin) {
      toast("A sketch origin needs two finite numbers.", true);
      return;
    }
    const plane = Number(byId("newOriginZ").value);
    if (!Number.isFinite(plane)) {
      toast("A sketch plane needs a finite height.", true);
      return;
    }
    const body = { kind, origin, plane };
    if (kind === "regular_polygon") {
      const sides = Number(byId("newSidesInput").value);
      if (!Number.isInteger(sides) || sides < 3 || sides > 256) {
        toast("A polygon needs an integer side count from 3 to 256.", true);
        return;
      }
      body.sides = sides;
    } else if (kind === "sector") {
      const start = Number(byId("newSectorStart").value);
      const sweep = Number(byId("newSectorSweep").value);
      if (!Number.isFinite(start)) {
        toast("A sector start angle needs a finite number of degrees.", true);
        return;
      }
      if (!Number.isFinite(sweep) || sweep <= 0 || sweep >= 360) {
        toast("A sector sweep must be between 0 and 360 degrees; use a circle for a full turn.", true);
        return;
      }
      body.start_degrees = start;
      body.sweep_degrees = sweep;
    }
    const next = await mutate("/api/sketch", body, `Sketch added (${profileName(kind)})`);
    if (next) selectNewest("sketch", next);
  }

  async function addFeature() {
    const project = app.project;
    if (!project || !project.sketches.length) {
      toast("Add a sketch before building a feature from it.", true);
      return;
    }
    const sketchId = Number(byId("newFeatureSketch").value);
    const kind = byId("newFeatureKind").value;
    const operation = byId("newFeatureOperation").value;
    if (!Number.isInteger(sketchId) || sketchId <= 0) {
      toast("Pick a sketch first.", true);
      return;
    }
    const action = operation === "join" ? "Join" : operation === "cut" ? "Cut" : "New body";
    if (kind === "revolve") {
      const axis = byId("newFeatureAxis").value;
      const next = await mutate(
        "/api/feature/revolve",
        { sketch_id: sketchId, operation, axis },
        `${action} revolve added`
      );
      if (next) selectNewest("feature", next);
      return;
    }
    const depthParam = byId("newFeatureDepth").value;
    if (!depthParam) {
      toast("Pick a depth parameter first.", true);
      return;
    }
    const direction = byId("newFeatureDirection").value;
    const next = await mutate(
      "/api/feature/extrude",
      { sketch_id: sketchId, depth_param: depthParam, operation, direction },
      `${action} extrude added (${direction === "down" ? "down" : "up"})`
    );
    if (next) selectNewest("feature", next);
  }

  async function moveSketch() {
    const sketch = activeSketch();
    if (!sketch) {
      toast("Select a sketch to move.", true);
      return;
    }
    const origin = originFrom("originX", "originY");
    if (!origin) {
      toast("A sketch origin needs two finite numbers.", true);
      return;
    }
    const plane = Number(byId("originZ").value);
    if (!Number.isFinite(plane)) {
      toast("A sketch plane needs a finite height.", true);
      return;
    }
    const unchanged =
      origin[0] === sketch.origin[0] &&
      origin[1] === sketch.origin[1] &&
      plane === (sketch.plane_z || 0);
    if (unchanged) return;
    await mutate(
      "/api/sketch/origin",
      { id: sketch.id, origin, plane },
      `Sketch ${sketch.id} placed at ${formatNumber(origin[0])}, ${formatNumber(origin[1])}, plane ${formatNumber(plane)}`
    );
  }

  // ---------------------------------------------------------------------------------------
  // Polyline drawing
  // ---------------------------------------------------------------------------------------

  function updateDrawBar() {
    const bar = byId("drawBar");
    bar.hidden = !app.draw;
    if (!app.draw) return;
    const count = app.draw.points.length;
    const frozen = app.busy || app.draw.submitting;
    byId("drawCount").textContent = `${count} point${count === 1 ? "" : "s"}`;
    byId("drawUndoPoint").disabled = frozen || count === 0;
    byId("drawClose").disabled = frozen || count < 3;
    byId("drawCancel").disabled = frozen;
  }

  function refreshDraft() {
    updateDrawBar();
    byId("viewport").classList.toggle("drawing", Boolean(app.draw));
    if (!app.viewport) return;
    app.viewport.uploadSceneLines();
    app.viewport.requestFrame();
  }

  function startDrawing() {
    if (app.busy) return;
    if (app.draw) {
      setStatus("A polyline is already being drawn; close it or press Esc first.", "");
      return;
    }
    app.draw = { points: [], submitting: false };
    clearPick();
    // On a narrow layout the project drawer covers the viewport; drawing needs it out of the way.
    closeDrawers();
    refreshDraft();
    setStatus("Click the z = 0 grid plane to place polyline points.", "busy");
    log("Polyline drawing started");
    byId("viewport").focus({ preventScroll: true });
  }

  function endDrawing() {
    app.draw = null;
    refreshDraft();
    setStatus("Ready", "");
  }

  function cancelDrawing() {
    if (!app.draw) return;
    const dropped = app.draw.points.length;
    endDrawing();
    log(
      dropped
        ? `Polyline drawing cancelled (${dropped} points discarded)`
        : "Polyline drawing cancelled"
    );
  }

  function undoDrawPoint() {
    if (!app.draw || app.draw.submitting || !app.draw.points.length) return;
    app.draw.points.pop();
    const count = app.draw.points.length;
    refreshDraft();
    setStatus(`Point removed; ${count} point${count === 1 ? "" : "s"} placed.`, "");
  }

  function roundDocument(value) {
    return Math.round(value * 1000) / 1000;
  }

  function addDrawPoint(clientX, clientY) {
    if (!app.draw || app.draw.submitting) return;
    const ground = app.viewport ? app.viewport.groundAt(clientX, clientY) : null;
    if (!ground) {
      toast("That view is edge-on to the drawing plane. Orbit or use the top view, then click again.", true);
      return;
    }
    const scaleFactor = documentScale();
    const point = [roundDocument(ground[0] / scaleFactor), roundDocument(ground[1] / scaleFactor)];
    const last = app.draw.points[app.draw.points.length - 1];
    // A repeated click would be a zero-length segment, which the document refuses.
    if (last && last[0] === point[0] && last[1] === point[1]) return;
    app.draw.points.push(point);
    refreshDraft();
    setStatus(
      `Point ${app.draw.points.length} at ${formatNumber(point[0])}, ${formatNumber(point[1])} — Enter closes the loop.`,
      ""
    );
  }

  async function finishDrawing() {
    if (!app.draw || app.draw.submitting) return;
    if (app.draw.points.length < 3) {
      toast("A closed loop needs at least three points.", true);
      return;
    }
    const points = app.draw.points.slice();
    // The points are absolute document coordinates, so the sketch origin stays at 0.
    app.draw.submitting = true;
    refreshDraft();
    const next = await mutate(
      "/api/sketch",
      { kind: "polyline", origin: [0, 0], points },
      `Sketch added (polyline, ${points.length} points)`
    );
    if (!next) {
      // The request failed; the loop stays on screen so the user can retry or adjust it.
      if (app.draw) {
        app.draw.submitting = false;
        refreshDraft();
      }
      return;
    }
    endDrawing();
    selectNewest("sketch", next);
  }

  // The palette drives the panel controls so both routes share one code path.
  async function createSketch(kind) {
    byId("newSketchKind").value = kind;
    byId("newSidesField").hidden = kind !== "regular_polygon";
    await addSketch();
  }

  async function extrudeSketch(operation) {
    const sketch = activeSketch();
    if (sketch) byId("newFeatureSketch").value = String(sketch.id);
    byId("newFeatureKind").value = "extrude";
    byId("newFeatureOperation").value = operation;
    await addFeature();
  }

  async function revolveSketch(operation) {
    const sketch = activeSketch();
    if (sketch) byId("newFeatureSketch").value = String(sketch.id);
    byId("newFeatureKind").value = "revolve";
    byId("newFeatureOperation").value = operation;
    await addFeature();
  }

  function locateFailure() {
    const project = app.project;
    if (!project || !project.failure) return;
    select("feature", project.failure.feature);
    const row = byId("tree").querySelector(".tree-item.selected");
    if (row) row.scrollIntoView({ block: "nearest" });
  }

  function toggleWireframe() {
    if (!app.viewport) return;
    app.viewport.wireframe = !app.viewport.wireframe;
    byId("wireBtn").setAttribute("aria-pressed", String(app.viewport.wireframe));
    app.viewport.requestFrame();
    log(`Wireframe ${app.viewport.wireframe ? "on" : "off"}`);
  }

  function toggleGrid() {
    if (!app.viewport) return;
    app.viewport.grid = !app.viewport.grid;
    byId("gridBtn").setAttribute("aria-pressed", String(app.viewport.grid));
    app.viewport.uploadSceneLines();
    app.viewport.requestFrame();
    log(`Grid ${app.viewport.grid ? "on" : "off"}`);
  }

  function fit() {
    if (app.viewport) app.viewport.fit();
  }

  function focusParameter(id) {
    const input = app.parameterInputs.get(id);
    if (!input) return;
    input.focus();
    input.select();
  }

  // ---------------------------------------------------------------------------------------
  // AutoCAD Command Line & Engineering Simulation HUD
  // ---------------------------------------------------------------------------------------

  const cliHistory = [];
  let cliHistoryIndex = -1;
  let lastExecutedCommand = null;

  async function runCaeStatic() {
    toast("Running Linear Static Structural FEA solver...");
    try {
      const data = await api("/api/cae/static", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ material: "steel.1018" }),
      });
      showSimulationHud("Linear Static Structural FEA", `
        <div class="sim-metric-grid">
          <div class="sim-metric-box">
            <div class="label">Max von Mises Stress</div>
            <div class="value">${data.max_von_mises_mpa.toFixed(2)} MPa</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Max Deflection</div>
            <div class="value">${data.max_displacement_mm.toFixed(4)} mm</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Factor of Safety</div>
            <div class="value">${data.factor_of_safety ? data.factor_of_safety.toFixed(2) : "N/A"}</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Hex8 Elements / Nodes</div>
            <div class="value">${data.elements} / ${data.nodes}</div>
          </div>
        </div>
        <p class="hint">Solver iterations: ${data.iterations} | Equilibrium residual: ${data.equilibrium_residual.toExponential(2)}</p>
        <p class="hint" style="color: var(--warning); margin-top: 8px;">${data.quality}</p>
      `);
      toast(`FEA Complete: Max Stress ${data.max_von_mises_mpa.toFixed(2)} MPa`);
    } catch (e) {
      toast(`CAE error: ${e.message}`, true);
    }
  }

  async function runCaeModal() {
    toast("Running Dynamic Vibration Modal Frequency solver...");
    try {
      const data = await api("/api/cae/modal", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ material: "steel.1018", num_modes: 3 }),
      });
      const modesRows = data.modes.map((m) => `
        <tr>
          <td><strong>Mode ${m.mode}</strong></td>
          <td>${m.frequency_hz.toFixed(2)} Hz</td>
          <td>${m.angular_frequency_rad_s.toFixed(2)} rad/s</td>
          <td>${m.generalized_mass.toFixed(4)}</td>
        </tr>
      `).join("");
      showSimulationHud("Dynamic Vibration Natural Frequencies", `
        <div class="sim-metric-grid">
          <div class="sim-metric-box">
            <div class="label">Fundamental Frequency (f1)</div>
            <div class="value" style="color: var(--accent);">${data.fundamental_frequency_hz ? data.fundamental_frequency_hz.toFixed(2) : "N/A"} Hz</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Voxel Hex8 Elements</div>
            <div class="value">${data.elements} (${data.voxel_size_mm} mm)</div>
          </div>
        </div>
        <table class="sim-hud-table">
          <thead>
            <tr><th>Mode</th><th>Cyclic Freq (Hz)</th><th>Angular Freq (rad/s)</th><th>Gen. Mass</th></tr>
          </thead>
          <tbody>${modesRows}</tbody>
        </table>
        <p class="hint" style="color: var(--warning); margin-top: 8px;">${data.quality}</p>
      `);
      toast(`Modal Analysis Complete: f1 = ${data.fundamental_frequency_hz ? data.fundamental_frequency_hz.toFixed(2) : "N/A"} Hz`);
    } catch (e) {
      toast(`Modal error: ${e.message}`, true);
    }
  }

  async function runCamToolpaths() {
    toast("Generating 3-Axis CNC Machining Toolpaths & G-Code...");
    try {
      const data = await api("/api/cam/toolpaths", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ tool_diameter_mm: 6.0 }),
      });
      showSimulationHud("3-Axis CNC Machining & G-Code", `
        <div class="sim-metric-grid">
          <div class="sim-metric-box">
            <div class="label">Tool / Operation</div>
            <div class="value" style="font-size: 15px;">${data.tool_name}</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Total Motion Moves</div>
            <div class="value">${data.moves_count} moves</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Cutting / Rapid Length</div>
            <div class="value">${data.cutting_length_mm.toFixed(1)} / ${data.rapid_length_mm.toFixed(1)} mm</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Est. Machining Time</div>
            <div class="value">${data.estimated_minutes.toFixed(2)} min</div>
          </div>
        </div>
        <div class="section-head"><h3 style="font-size: 12px; margin: 8px 0;">Post-Processed G-Code (LinuxCNC / RS274)</h3></div>
        <pre class="sim-code-preview"><code>${data.gcode_preview}</code></pre>
      `);
      toast(`CAM Complete: ${data.moves_count} moves, ~${data.estimated_minutes.toFixed(2)} min`);
    } catch (e) {
      toast(`CAM error: ${e.message}`, true);
    }
  }

  async function runCaeThermal() {
    toast("Running Steady-State Thermal Conduction FEA solver...");
    try {
      const data = await api("/api/cae/thermal", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          material: "steel.1018",
          fixed_temp_hot_c: 100.0,
          fixed_temp_cold_c: 20.0,
        }),
      });
      showSimulationHud("Steady-State Thermal Conduction FEA", `
        <div class="sim-metric-grid">
          <div class="sim-metric-box">
            <div class="label">Temperature Range</div>
            <div class="value" style="color: #ff9800;">${data.min_temperature_c.toFixed(1)}°C — ${data.max_temperature_c.toFixed(1)}°C</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Avg Temperature</div>
            <div class="value">${data.avg_temperature_c.toFixed(1)}°C</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Max Heat Flux Vector</div>
            <div class="value">${(data.max_heat_flux_w_m2 / 1000).toFixed(1)} kW/m²</div>
          </div>
          <div class="sim-metric-box">
            <div class="label">Hex8 Elements / Nodes</div>
            <div class="value">${data.elements} / ${data.nodes}</div>
          </div>
        </div>
        <p class="hint">${data.summary}</p>
        <p class="hint" style="color: var(--warning); margin-top: 8px;">${data.quality}</p>
      `);
      toast(`Thermal Analysis Complete: Max Heat Flux ${(data.max_heat_flux_w_m2 / 1000).toFixed(1)} kW/m²`);
    } catch (e) {
      toast(`Thermal error: ${e.message}`, true);
    }
  }

  function openSimLauncher() {
    showSimulationHud("Engineering Multiphysics & CAM", `
      <div class="sim-launcher-grid">
        <div class="sim-launcher-card" id="simCardStatic">
          <h4>Linear Static Stress FEA</h4>
          <p>Compute von Mises stress distribution, displacement field, and factor of safety on Hex8 voxel mesh.</p>
          <button type="button" class="button wide" id="launchStaticBtn">Run Static FEA</button>
        </div>
        <div class="sim-launcher-card" id="simCardModal">
          <h4>Dynamic Modal Vibration</h4>
          <p>Solve natural resonant frequencies (f1, f2, f3) and dynamic mode shapes via Lanczos eigenvalue analysis.</p>
          <button type="button" class="button wide" id="launchModalBtn">Run Modal Analysis</button>
        </div>
        <div class="sim-launcher-card" id="simCardThermal">
          <h4>Steady-State Thermal Conduction</h4>
          <p>Solve Fourier thermal gradient [K_th]{T} = {Q}, nodal temperature field, and heat flux vectors.</p>
          <button type="button" class="button wide" id="launchThermalBtn">Run Thermal Conduction</button>
        </div>
        <div class="sim-launcher-card" id="simCardCam">
          <h4>3-Axis CNC Machining & G-Code</h4>
          <p>Generate roughing and contouring toolpaths, collision checks, feeds & speeds, and exportable G-code.</p>
          <button type="button" class="button wide" id="launchCamBtn">Generate CNC Toolpaths</button>
        </div>
      </div>
    `);
    byId("launchStaticBtn")?.addEventListener("click", () => runCaeStatic());
    byId("launchModalBtn")?.addEventListener("click", () => runCaeModal());
    byId("launchThermalBtn")?.addEventListener("click", () => runCaeThermal());
    byId("launchCamBtn")?.addEventListener("click", () => runCamToolpaths());
  }

  function showSimulationHud(title, htmlContent) {
    byId("simHudTitle").textContent = title;
    byId("simHudContent").innerHTML = htmlContent;
    byId("simulationHud").hidden = false;
  }

  function closeSimHud() {
    byId("simulationHud").hidden = true;
  }

  // ---------------------------------------------------------------------------------------
  // AutoCAD Advanced Editing & 3D Bridge Handlers
  // ---------------------------------------------------------------------------------------

  function cadOffset() {
    if (!drawState.selected) {
      toast("Please select an entity first to offset", true);
      return;
    }
    const val = prompt("Specify offset distance (e.g. 5.0):", "5.0");
    if (!val) return;
    const dist = parseFloat(val);
    if (isNaN(dist) || dist <= 0) {
      toast("Invalid offset distance", true);
      return;
    }
    drawingMutate("/api/draft/offset", { id: drawState.selected, distance: dist, side: 1.0 }, "Offset created");
  }

  function cadFillet() {
    if (drawState.filletFirst) {
      if (!drawState.selected || drawState.selected === drawState.filletFirst) {
        toast("Select a second distinct line to complete fillet", true);
        return;
      }
      const id1 = drawState.filletFirst;
      const id2 = drawState.selected;
      const r = drawState.filletRadius || 0.0;
      drawState.filletFirst = null;
      drawingMutate("/api/draft/fillet", { id1, id2, radius: r }, "Fillet completed");
      return;
    }
    const rVal = prompt("Specify fillet radius (0 for clean corner):", "0.0");
    if (rVal === null) return;
    const r = parseFloat(rVal);
    drawState.filletRadius = isNaN(r) || r < 0 ? 0.0 : r;
    if (drawState.selected) {
      drawState.filletFirst = drawState.selected;
      toast(`Fillet armed (R=${drawState.filletRadius}). Now click second line.`);
    } else {
      toast(`Fillet armed (R=${drawState.filletRadius}). Select first line.`);
    }
  }

  function cadChamfer() {
    if (drawState.chamferFirst) {
      if (!drawState.selected || drawState.selected === drawState.chamferFirst) {
        toast("Select a second distinct line to complete chamfer", true);
        return;
      }
      const id1 = drawState.chamferFirst;
      const id2 = drawState.selected;
      const d1 = drawState.chamferD1 || 5.0;
      const d2 = drawState.chamferD2 || 5.0;
      drawState.chamferFirst = null;
      drawingMutate("/api/draft/chamfer", { id1, id2, d1, d2 }, "Chamfer completed");
      return;
    }
    const dVal = prompt("Specify chamfer cutback distances d1, d2 (e.g. 5.0, 5.0):", "5.0, 5.0");
    if (!dVal) return;
    const parts = dVal.split(/[\s,]+/);
    const d1 = parseFloat(parts[0]) || 5.0;
    const d2 = parseFloat(parts[1]) || d1;
    drawState.chamferD1 = d1;
    drawState.chamferD2 = d2;
    if (drawState.selected) {
      drawState.chamferFirst = drawState.selected;
      toast(`Chamfer armed (${d1}, ${d2}). Now click second line.`);
    } else {
      toast(`Chamfer armed (${d1}, ${d2}). Select first line.`);
    }
  }

  function cadMirror() {
    if (!drawState.selected) {
      toast("Please select an entity first to mirror", true);
      return;
    }
    drawingMutate("/api/draft/transform", {
      op: "mirror",
      ids: [drawState.selected],
      p1: [0.0, 0.0],
      p2: [1.0, 0.0],
      erase_source: false
    }, "Entity mirrored across axis");
  }

  function cadRotate() {
    if (!drawState.selected) {
      toast("Please select an entity first to rotate", true);
      return;
    }
    const val = prompt("Specify rotation angle in degrees (e.g. 45.0):", "45.0");
    if (!val) return;
    const ang = parseFloat(val);
    if (isNaN(ang)) return;
    drawingMutate("/api/draft/transform", {
      op: "rotate",
      ids: [drawState.selected],
      angle_deg: ang,
      center: [0.0, 0.0]
    }, "Entity rotated");
  }

  function cadScale() {
    if (!drawState.selected) {
      toast("Please select an entity first to scale", true);
      return;
    }
    const val = prompt("Specify scale factor (e.g. 2.0):", "2.0");
    if (!val) return;
    const factor = parseFloat(val);
    if (isNaN(factor) || factor <= 0) return;
    drawingMutate("/api/draft/transform", {
      op: "scale",
      ids: [drawState.selected],
      factor: factor,
      base: [0.0, 0.0]
    }, "Entity scaled");
  }

  function cadMove() {
    if (!drawState.selected) {
      toast("Please select an entity first to move", true);
      return;
    }
    const val = prompt("Specify displacement vector dx, dy (e.g. 10, 10):", "10, 10");
    if (!val) return;
    const parts = val.split(/[\s,]+/);
    const dx = parseFloat(parts[0]) || 0;
    const dy = parseFloat(parts[1]) || 0;
    drawingMutate("/api/draft/transform", {
      op: "translate",
      ids: [drawState.selected],
      dx,
      dy
    }, "Entity moved");
  }

  function cadCopy() {
    if (!drawState.selected) {
      toast("Please select an entity first to copy", true);
      return;
    }
    const val = prompt("Specify copy displacement vector dx, dy (e.g. 15, 15):", "15, 15");
    if (!val) return;
    const parts = val.split(/[\s,]+/);
    const dx = parseFloat(parts[0]) || 0;
    const dy = parseFloat(parts[1]) || 0;
    drawingMutate("/api/draft/transform", {
      op: "copy",
      ids: [drawState.selected],
      dx,
      dy
    }, "Entity copied");
  }

  function cadExplode() {
    if (!drawState.selected) {
      toast("Please select a rectangle or polyline to explode", true);
      return;
    }
    drawingMutate("/api/draft/explode", { id: drawState.selected }, "Exploded into individual lines");
  }

  async function cadDist() {
    const val1 = prompt("First point x1, y1 (or leave blank to use origin):", "0, 0");
    if (val1 === null) return;
    const val2 = prompt("Second point x2, y2:", "30, 40");
    if (val2 === null) return;
    const p1Parts = val1.split(/[\s,]+/);
    const p2Parts = val2.split(/[\s,]+/);
    const p1 = [parseFloat(p1Parts[0]) || 0, parseFloat(p1Parts[1]) || 0];
    const p2 = [parseFloat(p2Parts[0]) || 0, parseFloat(p2Parts[1]) || 0];

    try {
      const res = await api("/api/draft/measure", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ mode: "distance", p1, p2 })
      });
      const msg = `DIST: ${res.distance.toFixed(3)} mm, ΔX: ${res.delta_x.toFixed(3)}, ΔY: ${res.delta_y.toFixed(3)}, Angle: ${res.angle_deg.toFixed(1)}°`;
      toast(msg);
      log(msg);
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadArea() {
    if (!app.drawing || !app.drawing.entities) return;
    let points = null;
    if (drawState.selected) {
      const ent = app.drawing.entities.find((e) => e.id === drawState.selected);
      if (ent && ent.kind) {
        if (ent.kind.kind === "rectangle") {
          const o = ent.kind.origin;
          const w = ent.kind.width;
          const h = ent.kind.height;
          points = [[o[0], o[1]], [o[0] + w, o[1]], [o[0] + w, o[1] + h], [o[0], o[1] + h]];
        } else if (ent.kind.kind === "polyline" && ent.kind.closed) {
          points = ent.kind.points;
        }
      }
    }
    if (!points) {
      const val = prompt("Enter polygon coordinates (e.g. 0,0 20,0 20,10 0,10):", "0,0 20,0 20,10 0,10");
      if (!val) return;
      const raw = val.trim().split(/\s+/);
      points = raw.map((pair) => {
        const xy = pair.split(",");
        return [parseFloat(xy[0]) || 0, parseFloat(xy[1]) || 0];
      });
    }
    try {
      const res = await api("/api/draft/measure", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ mode: "area", points })
      });
      const msg = `AREA: ${res.area.toFixed(3)} mm², PERIMETER: ${res.perimeter.toFixed(3)} mm, Centroid: (${res.centroid[0].toFixed(2)}, ${res.centroid[1].toFixed(2)})`;
      toast(msg);
      log(msg);
    } catch (e) {
      toast(e.message, true);
    }
  }

  function cadList() {
    if (!drawState.selected || !app.drawing) {
      toast("No entity selected. Click an entity in the drawing to inspect.", true);
      return;
    }
    const ent = app.drawing.entities.find((e) => e.id === drawState.selected);
    if (!ent) {
      toast("Entity not found", true);
      return;
    }
    const msg = `Entity #${ent.id}: kind=${ent.kind.kind}, layer=${ent.layer}, data=${JSON.stringify(ent.kind)}`;
    toast(msg);
    log(msg);
  }

  async function cadToSolid() {
    if (!drawState.selected) {
      toast("Select a closed 2D shape (rectangle, circle, polyline) to extrude into 3D solid", true);
      return;
    }
    const val = prompt("Specify 3D extrusion depth (mm):", "15.0");
    if (val === null) return;
    const depth = parseFloat(val) || 15.0;
    try {
      const payload = await api("/api/draft/to-solid", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ entity_id: drawState.selected, depth })
      });
      applyState(payload);
      setView("iso");
      const msg = `Extruded 2D shape #${drawState.selected} into 3D Solid Model (depth ${depth} mm)!`;
      toast(msg);
      log(msg);
    } catch (e) {
      toast(e.message, true);
      log(e.message, "error");
    }
  }

  async function cadHatch() {
    if (!drawState.selected) {
      toast("Please select a closed entity (rectangle, polyline) to hatch", true);
      return;
    }
    const pat = prompt("Specify hatch pattern (ANSI31, NET):", "ANSI31");
    if (!pat) return;
    const spacingStr = prompt("Specify hatch line spacing (mm):", "3.0");
    const spacing = parseFloat(spacingStr) || 3.0;
    try {
      await drawingMutate("/api/draft/hatch", {
        entity_id: drawState.selected,
        pattern: pat.trim().toUpperCase(),
        angle_deg: 45.0,
        spacing: spacing,
      }, `Hatch pattern (${pat}) applied`);
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadMultiview() {
    try {
      await drawingMutate("/api/draft/multiview", {}, "Generated 4-View Orthographic Drawing with Sections!");
      toast("Orthographic Multi-View drawing generated!");
    } catch (e) {
      toast(e.message, true);
    }
  }

  function cadJoin() {
    if (!app.drawing || !app.drawing.entities) {
      toast("No entities available to join", true);
      return;
    }
    let ids = [];
    if (drawState.selected) {
      ids = [drawState.selected];
      const otherPrompt = prompt(
        `Joining entity ${drawState.selected}. Enter additional entity IDs to join (comma-separated), or leave blank:`,
        ""
      );
      if (otherPrompt) {
        const parsed = otherPrompt
          .split(/[\s,]+/)
          .map((s) => parseInt(s, 10))
          .filter((n) => !isNaN(n));
        ids = ids.concat(parsed);
      }
    } else {
      const val = prompt("Enter entity IDs to join (comma-separated, e.g. 1, 2, 3):", "");
      if (!val) return;
      ids = val
        .split(/[\s,]+/)
        .map((s) => parseInt(s, 10))
        .filter((n) => !isNaN(n));
    }
    if (ids.length === 0) {
      toast("No entities specified for JOIN", true);
      return;
    }
    drawingMutate("/api/draft/join", { ids, tolerance: 0.001 }, "Joined into Polyline");
  }

  function cadArray(defaultMode) {
    if (!drawState.selected) {
      toast("Please select an entity first to array", true);
      return;
    }
    let mode = defaultMode;
    if (!mode) {
      const choice = prompt("Specify array type: [R]ectangular or [P]olar:", "R");
      if (!choice) return;
      mode = choice.toUpperCase().startsWith("P") ? "polar" : "rectangular";
    }
    if (mode === "polar") {
      const countVal = prompt("Enter number of items in polar array:", "6");
      if (!countVal) return;
      const count = parseInt(countVal, 10) || 6;
      const centerVal = prompt("Enter center of rotation cx, cy (or leave blank for origin):", "0, 0");
      const cParts = (centerVal || "0, 0").split(/[\s,]+/);
      const center = [parseFloat(cParts[0]) || 0, parseFloat(cParts[1]) || 0];
      const angleVal = prompt("Enter fill angle in degrees (e.g. 360):", "360");
      const fillAngle = parseFloat(angleVal) || 360.0;
      drawingMutate(
        "/api/draft/array",
        {
          mode: "polar",
          ids: [drawState.selected],
          count,
          center,
          fill_angle_deg: fillAngle,
          rotate_items: true,
        },
        "Polar array created"
      );
    } else {
      const gridVal = prompt("Enter rows, columns (e.g. 3, 4):", "3, 4");
      if (!gridVal) return;
      const gParts = gridVal.split(/[\s,]+/);
      const rows = parseInt(gParts[0], 10) || 2;
      const cols = parseInt(gParts[1], 10) || 2;
      const spaceVal = prompt("Enter row spacing, column spacing (e.g. 20, 25):", "20, 25");
      const sParts = (spaceVal || "20, 25").split(/[\s,]+/);
      const rowSpacing = parseFloat(sParts[0]) || 20.0;
      const colSpacing = parseFloat(sParts[1]) || 25.0;
      drawingMutate(
        "/api/draft/array",
        {
          mode: "rectangular",
          ids: [drawState.selected],
          rows,
          cols,
          row_spacing: rowSpacing,
          col_spacing: colSpacing,
        },
        "Rectangular array created"
      );
    }
  }

  function cadPolygon() {
    const sidesVal = prompt("Enter number of sides (3-1024):", "6");
    if (!sidesVal) return;
    const sides = parseInt(sidesVal, 10) || 6;
    const centerVal = prompt("Enter center coordinates cx, cy (or 0, 0):", "0, 0");
    const cParts = (centerVal || "0, 0").split(/[\s,]+/);
    const center = [parseFloat(cParts[0]) || 0, parseFloat(cParts[1]) || 0];
    const rVal = prompt("Enter radius of circle:", "25.0");
    if (!rVal) return;
    const radius = parseFloat(rVal) || 25.0;
    const inChoice = prompt("Inscribed in circle [I] or Circumscribed [C]?", "I");
    const inscribed = !(inChoice && inChoice.toUpperCase().startsWith("C"));
    drawingMutate(
      "/api/draft/construct",
      {
        kind: "polygon",
        layer: drawState.layer || "0",
        center,
        radius,
        sides,
        inscribed,
        start_angle_deg: 0.0,
      },
      "Regular polygon created"
    );
  }

  async function cadSection() {
    if (!drawState.selected) {
      toast("Please select a closed entity (Circle, Rectangle, or closed Polyline) to analyze", true);
      return;
    }
    try {
      const res = await api("/api/draft/section-analysis", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ id: drawState.selected }),
      });
      const msg = `SECTION PROPERTIES:
Area: ${res.area.toFixed(2)} mm², Perimeter: ${res.perimeter.toFixed(2)} mm
Centroid: (${res.centroid[0].toFixed(2)}, ${res.centroid[1].toFixed(2)})
Ixx: ${res.i_xx.toFixed(1)} mm⁴, Iyy: ${res.i_yy.toFixed(1)} mm⁴, J: ${res.j_polar.toFixed(1)} mm⁴
Sx: ${res.s_x.toFixed(1)} mm³, Sy: ${res.s_y.toFixed(1)} mm³
rx: ${res.r_x.toFixed(2)} mm, ry: ${res.r_y.toFixed(2)} mm
Principal: I1=${res.i_1.toFixed(1)}, I2=${res.i_2.toFixed(1)}, θp=${res.principal_angle_deg.toFixed(1)}°`;
      toast(`Section: Area=${res.area.toFixed(1)} mm², Ixx=${res.i_xx.toFixed(0)}, Iyy=${res.i_yy.toFixed(0)}`);
      log(msg);
    } catch (e) {
      toast(e.message, true);
    }
  }

  function cadTrim() {
    if (!drawState.selected) {
      toast("Please select a line to trim", true);
      return;
    }
    const val = prompt(
      "Enter pick point on segment to remove (x, y), or leave blank to trim near midpoint:",
      ""
    );
    let pick_point = [0, 0];
    if (val) {
      const parts = val.split(/[\s,]+/);
      pick_point = [parseFloat(parts[0]) || 0, parseFloat(parts[1]) || 0];
    } else if (app.drawing) {
      const ent = app.drawing.entities.find((e) => e.id === drawState.selected);
      if (ent && ent.kind && ent.kind.kind === "line") {
        pick_point = [
          (ent.kind.start[0] + ent.kind.end[0]) * 0.5,
          (ent.kind.start[1] + ent.kind.end[1]) * 0.5,
        ];
      }
    }
    drawingMutate(
      "/api/draft/trim",
      { target_id: drawState.selected, boundary_ids: [], pick_point },
      "Line trimmed at boundaries"
    );
  }

  function cadExtend() {
    if (!drawState.selected) {
      toast("Please select a line to extend", true);
      return;
    }
    const val = prompt("Enter pick point near the endpoint to extend (x, y):", "");
    let pick_point = [0, 0];
    if (val) {
      const parts = val.split(/[\s,]+/);
      pick_point = [parseFloat(parts[0]) || 0, parseFloat(parts[1]) || 0];
    } else if (app.drawing) {
      const ent = app.drawing.entities.find((e) => e.id === drawState.selected);
      if (ent && ent.kind && ent.kind.kind === "line") {
        pick_point = ent.kind.end;
      }
    }
    drawingMutate(
      "/api/draft/extend",
      { target_id: drawState.selected, boundary_ids: [], pick_point },
      "Line extended to boundary"
    );
  }

  async function cadToCam() {
    if (!drawState.selected) {
      toast("Please select a closed profile (Rectangle, Circle, or Polyline) for CAM toolpath", true);
      return;
    }
    const depthVal = prompt("Enter milling depth in mm (e.g. 10.0):", "10.0");
    if (!depthVal) return;
    const depth_mm = parseFloat(depthVal) || 10.0;
    const stepVal = prompt("Enter stepdown per pass in mm (e.g. 3.0):", "3.0");
    const stepdown_mm = parseFloat(stepVal) || 3.0;
    const postChoice = prompt("Select CNC Controller: [1] LinuxCNC, [2] Grbl, [3] Mach3:", "1");
    let post_format = "linuxcnc";
    if (postChoice === "2" || (postChoice && postChoice.toLowerCase().includes("grbl"))) {
      post_format = "grbl";
    }
    if (postChoice === "3" || (postChoice && postChoice.toLowerCase().includes("mach"))) {
      post_format = "mach3";
    }

    try {
      const res = await api("/api/draft/to-cam", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ entity_id: drawState.selected, depth_mm, stepdown_mm, post_format }),
      });
      const gcodeSample = res.gcode.split("\n").slice(0, 15).join("\n");
      const msg = `CAM TOOLPATH GENERATED:
Profile: ${res.profile_points} points, Moves: ${res.moves_count}
Cutting Length: ${res.cutting_length_mm.toFixed(1)} mm, Rapid Length: ${res.rapid_length_mm.toFixed(1)} mm
G-code Preview:
${gcodeSample}`;
      toast(`CAM Generated: ${res.moves_count} moves, ${res.cutting_length_mm.toFixed(1)} mm cut`);
      log(msg);
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadDimRadial(isDiameter = false) {
    let entityId = drawState.selected || null;
    let pickAngle = 45.0;
    if (!entityId) {
      const input = prompt(
        isDiameter
          ? "DIMDIAMETER: Enter Circle/Arc Entity ID to dimension (or leave empty to enter center & target):"
          : "DIMRADIUS: Enter Circle/Arc Entity ID to dimension (or leave empty to enter center & target):"
      );
      if (input && input.trim()) {
        entityId = parseInt(input.trim(), 10);
      }
    }

    let payload;
    if (entityId) {
      const angleStr = prompt("Leader angle in degrees (default 45):", "45");
      pickAngle = angleStr ? parseFloat(angleStr) || 45.0 : 45.0;
      payload = {
        layer: drawState.activeLayer || "0",
        entity_id: entityId,
        pick_angle_deg: pickAngle,
        leader_offset: 15.0,
        is_diameter: isDiameter,
      };
    } else {
      const centerStr = prompt("Enter center point x,y (e.g. 50,50):", "0,0");
      if (!centerStr) return;
      const [cx, cy] = centerStr.split(",").map(parseFloat);
      const targetStr = prompt("Enter rim target point x,y (e.g. 75,50):", "25,0");
      if (!targetStr) return;
      const [tx, ty] = targetStr.split(",").map(parseFloat);
      payload = {
        layer: drawState.activeLayer || "0",
        center: [cx, cy],
        target: [tx, ty],
        leader_offset: 15.0,
        is_diameter: isDiameter,
      };
    }

    try {
      const res = await api("/api/draft/dimension-radial", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify(payload),
      });
      applyDraft(res);
      toast(isDiameter ? "Diameter dimension added" : "Radius dimension added");
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadLinetype() {
    const layerName = drawState.activeLayer || "0";
    const ltype = prompt(
      `Set linetype for layer '${layerName}' (continuous, hidden, center, phantom, dashed, dot):`,
      "hidden"
    );
    if (!ltype) return;
    try {
      const res = await api("/api/draft/layer", {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({
          name: layerName,
          linetype: ltype.trim().toLowerCase(),
        }),
      });
      applyDraft(res);
      toast(`Layer '${layerName}' linetype set to ${ltype.toUpperCase()}`);
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadBlock() {
    const name = prompt("BLOCK: Enter block name:");
    if (!name || !name.trim()) return;
    const bpStr = prompt("BLOCK: Enter base point X, Y (default 0, 0):", "0, 0");
    const bpParts = (bpStr || "0, 0").split(",").map((s) => parseFloat(s.trim()));
    const base_point = [isNaN(bpParts[0]) ? 0 : bpParts[0], isNaN(bpParts[1]) ? 0 : bpParts[1]];
    let entity_ids = [];
    if (drawState.selected) {
      entity_ids = [drawState.selected];
    } else {
      const idsStr = prompt("BLOCK: Enter comma-separated entity IDs (e.g. 1, 2):");
      if (idsStr) {
        entity_ids = idsStr.split(",").map((s) => parseInt(s.trim(), 10)).filter((n) => !isNaN(n));
      }
    }
    if (!entity_ids.length) {
      toast("BLOCK cancelled: No entities selected", true);
      return;
    }
    drawingMutate("/api/draft/block", {
      name: name.trim(),
      base_point,
      entity_ids,
    }, `Block '${name.trim()}' created with ${entity_ids.length} entities`);
  }

  async function cadInsert() {
    const blocks = (app.drawing && app.drawing.blocks) || [];
    const blockNames = blocks.map((b) => b.name).join(", ");
    const namePrompt = blockNames ? `INSERT: Enter block name (${blockNames}):` : "INSERT: Enter block name:";
    const block_name = prompt(namePrompt);
    if (!block_name || !block_name.trim()) return;
    const posStr = prompt("INSERT: Enter insertion position X, Y (default 0, 0):", "0, 0");
    const posParts = (posStr || "0, 0").split(",").map((s) => parseFloat(s.trim()));
    const position = [isNaN(posParts[0]) ? 0 : posParts[0], isNaN(posParts[1]) ? 0 : posParts[1]];
    const scaleStr = prompt("INSERT: Enter scale (e.g. 1 or 2, 2):", "1");
    let scale = [1, 1];
    if (scaleStr) {
      const scParts = scaleStr.split(",").map((s) => parseFloat(s.trim()));
      if (scParts.length === 1 && !isNaN(scParts[0])) {
        scale = [scParts[0], scParts[0]];
      } else if (scParts.length >= 2 && !isNaN(scParts[0]) && !isNaN(scParts[1])) {
        scale = [scParts[0], scParts[1]];
      }
    }
    const rotStr = prompt("INSERT: Enter rotation angle in degrees (default 0):", "0");
    const rotation_degrees = parseFloat(rotStr) || 0;

    drawingMutate("/api/draft/insert", {
      layer: drawState.activeLayer || "0",
      block_name: block_name.trim(),
      position,
      scale,
      rotation_degrees,
    }, `Block '${block_name.trim()}' inserted at [${position[0]}, ${position[1]}]`);
  }

  async function cadPurge() {
    try {
      const res = await drawingMutate("/api/draft/purge", {}, null);
      if (res && res.report) {
        const lCount = (res.report.purged_layers || []).length;
        const bCount = (res.report.purged_blocks || []).length;
        if (lCount === 0 && bCount === 0) {
          toast("Purge: Drawing is already clean (0 unreferenced items)");
        } else {
          toast(`PURGE complete: Removed ${lCount} unused layer(s), ${bCount} unused block(s)`);
        }
      }
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadOverkill() {
    const inputTol = prompt("OVERKILL: Enter tolerance in mm (default 0.0001):", "0.0001");
    if (inputTol === null) return;
    const tolerance = parseFloat(inputTol) || 0.0001;
    try {
      const res = await drawingMutate("/api/draft/overkill", { tolerance }, null);
      if (res && res.report) {
        const dCount = res.report.duplicate_entities_removed || 0;
        const mCount = res.report.collinear_lines_merged || 0;
        if (dCount === 0 && mCount === 0) {
          toast("OVERKILL: No duplicate or overlapping geometry found");
        } else {
          toast(`OVERKILL complete: Deleted ${dCount} duplicate(s), merged ${mCount} collinear line(s)`);
        }
      }
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadDivide() {
    let id = drawState.selected;
    if (!id) {
      const inputId = prompt("DIVIDE: Enter Entity ID to divide:", "");
      if (!inputId) return;
      id = parseInt(inputId, 10);
    }
    const inputSegs = prompt(`DIVIDE Entity #${id}: Enter number of segments (>= 2):`, "4");
    if (!inputSegs) return;
    const segments = parseInt(inputSegs, 10);
    if (!segments || segments < 2) {
      toast("Divide requires at least 2 segments", true);
      return;
    }
    try {
      const res = await drawingMutate("/api/draft/divide", { entity_id: id, segments }, null);
      if (res && res.created_point_ids) {
        toast(`DIVIDE: Created ${res.created_point_ids.length} points along Entity #${id}`);
      }
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadMeasureInterval() {
    let id = drawState.selected;
    if (!id) {
      const inputId = prompt("MEASURE: Enter Entity ID to measure intervals along:", "");
      if (!inputId) return;
      id = parseInt(inputId, 10);
    }
    const inputDist = prompt(`MEASURE Entity #${id}: Enter segment length interval in mm:`, "25.0");
    if (!inputDist) return;
    const interval = parseFloat(inputDist);
    if (!interval || interval <= 0) {
      toast("Interval must be positive", true);
      return;
    }
    try {
      const res = await drawingMutate("/api/draft/measure-interval", { entity_id: id, interval }, null);
      if (res && res.created_point_ids) {
        toast(`MEASURE: Placed ${res.created_point_ids.length} points at ${interval}mm intervals along Entity #${id}`);
      }
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadMatchProp() {
    let srcId = drawState.selected;
    if (!srcId) {
      const inputSrc = prompt("MATCHPROP: Enter source Entity ID to copy properties from:", "");
      if (!inputSrc) return;
      srcId = parseInt(inputSrc, 10);
    }
    const inputTargets = prompt(`MATCHPROP from Entity #${srcId}: Enter target Entity IDs (comma-separated):`, "");
    if (!inputTargets) return;
    const target_ids = inputTargets
      .split(/[\s,]+/)
      .map((s) => parseInt(s, 10))
      .filter((n) => !isNaN(n) && n > 0);
    if (target_ids.length === 0) {
      toast("No valid target entities specified", true);
      return;
    }
    try {
      const res = await drawingMutate("/api/draft/match-properties", { source_id: srcId, target_ids }, null);
      if (res) {
        toast(`MATCHPROP: Updated properties of ${res.matched_count || 0} entity/entities`);
      }
    } catch (e) {
      toast(e.message, true);
    }
  }

  async function cadConstraintsSolve() {
    toast("Parametric Constraint Engine: Solving geometric & dimensional relations...");
    try {
      const lines = (app.drawing?.entities || []).filter((e) => e.kind && e.kind.kind === "line");
      if (lines.length === 0) {
        toast("No lines in drawing to constrain. Draw lines with 'L' command first.", true);
        return;
      }
      const points = [];
      const ptMap = new Map();
      function getPtIdx(pt) {
        const key = `${pt[0].toFixed(3)},${pt[1].toFixed(3)}`;
        if (ptMap.has(key)) return ptMap.get(key);
        const idx = points.length;
        points.push([pt[0], pt[1]]);
        ptMap.set(key, idx);
        return idx;
      }
      const lineDefs = [];
      const updateMaps = [];
      const constraints = [];
      lines.forEach((l, idx) => {
        const p1 = getPtIdx(l.kind.start);
        const p2 = getPtIdx(l.kind.end);
        lineDefs.push([p1, p2]);
        updateMaps.push({ entity_id: l.id, line_index: idx });
        const dx = Math.abs(l.kind.end[0] - l.kind.start[0]);
        const dy = Math.abs(l.kind.end[1] - l.kind.start[1]);
        if (dy < dx * 0.1) {
          constraints.push({ Horizontal: idx });
        } else if (dx < dy * 0.1) {
          constraints.push({ Vertical: idx });
        }
      });
      if (points.length > 0) {
        constraints.push({ FixPoint: [0, points[0]] });
      }
      const res = await drawingMutate("/api/draft/constraints/solve", {
        points,
        lines: lineDefs,
        constraints,
        update_entities: updateMaps,
      }, null);
      if (res && res.solve) {
        toast(`PARAMETRIC SOLVER: Converged in ${res.solve.iterations} iters, DOF status: ${res.dof.status} (free DOF: ${res.dof.total_dof})`);
      }
    } catch (e) {
      toast(e.message, true);
    }
  }

  const autocadCommands = [
    { alias: "L", name: "LINE", desc: "Draw 2D line (2 clicks or coordinates)", run: () => setDrawTool("line") },
    { alias: "C", name: "CIRCLE", desc: "Draw 2D circle (centre, radius)", run: () => {
      if ((drawState.tool === "polyline" || drawState.tool === "spline") && drawState.points.length >= 2) {
        finishPolyline(true);
      } else {
        setDrawTool("circle");
      }
    }},
    { alias: "REC", name: "RECTANGLE", desc: "Draw 2D rectangle (opposite corners)", run: () => setDrawTool("rectangle") },
    { alias: "EL", name: "ELLIPSE", desc: "Draw 2D ellipse (centre, axis radii)", run: () => setDrawTool("ellipse") },
    { alias: "PL", name: "PLINE", desc: "Draw 2D polyline (multi-segment)", run: () => setDrawTool("polyline") },
    { alias: "SPL", name: "SPLINE", desc: "Draw Catmull-Rom spline curve", run: () => setDrawTool("spline") },
    { alias: "PO", name: "POINT", desc: "Place 2D point entity", run: () => setDrawTool("point") },
    { alias: "DIM", name: "DIMALIGNED", desc: "Measure & dimension between 2 points", run: () => setDrawTool("dimension") },
    { alias: "DRA", name: "DIMRADIUS", desc: "Create radial dimension for arc or circle", run: () => cadDimRadial(false) },
    { alias: "DDI", name: "DIMDIAMETER", desc: "Create diameter dimension for circle or arc", run: () => cadDimRadial(true) },
    { alias: "LTYPE", name: "LINETYPE", desc: "Set layer linetype (CONTINUOUS, HIDDEN, CENTER, PHANTOM, DASHED, DOT)", run: () => cadLinetype() },
    { alias: "B", name: "BLOCK", desc: "Create reusable block definition from entities", run: () => cadBlock() },
    { alias: "I", name: "INSERT", desc: "Insert reusable block instance", run: () => cadInsert() },
    { alias: "INS", name: "INSERT", desc: "Insert reusable block instance", run: () => cadInsert() },
    { alias: "T", name: "TEXT", desc: "Place annotation text", run: () => setDrawTool("text") },
    { alias: "O", name: "OFFSET", desc: "Offset entity by distance (parallel/concentric)", run: () => cadOffset() },
    { alias: "TR", name: "TRIM", desc: "Trim line at cutting boundaries", run: () => cadTrim() },
    { alias: "EX", name: "EXTEND", desc: "Extend line to boundary edges", run: () => cadExtend() },
    { alias: "F", name: "FILLET", desc: "Fillet corner between two lines (R=0 to clean)", run: () => cadFillet() },
    { alias: "CHA", name: "CHAMFER", desc: "Chamfer corner between two lines", run: () => cadChamfer() },
    { alias: "MI", name: "MIRROR", desc: "Mirror selected entity across axis", run: () => cadMirror() },
    { alias: "RO", name: "ROTATE", desc: "Rotate entity around base point", run: () => cadRotate() },
    { alias: "SC", name: "SCALE", desc: "Scale entity by factor", run: () => cadScale() },
    { alias: "M", name: "MOVE", desc: "Translate entity by (dx, dy)", run: () => cadMove() },
    { alias: "CO", name: "COPY", desc: "Duplicate entity shifted by (dx, dy)", run: () => cadCopy() },
    { alias: "CP", name: "COPY", desc: "Duplicate entity shifted by (dx, dy)", run: () => cadCopy() },
    { alias: "X", name: "EXPLODE", desc: "Decompose rectangle/polyline into lines", run: () => cadExplode() },
    { alias: "J", name: "JOIN", desc: "Join lines and open polylines into a unified polyline", run: () => cadJoin() },
    { alias: "AR", name: "ARRAY", desc: "Create rectangular or polar pattern array", run: () => cadArray() },
    { alias: "ARRAYRECT", name: "ARRAYRECT", desc: "Create rectangular 2D pattern array", run: () => cadArray("rectangular") },
    { alias: "ARRAYPOLAR", name: "ARRAYPOLAR", desc: "Create polar circular pattern array", run: () => cadArray("polar") },
    { alias: "POL", name: "POLYGON", desc: "Draw regular polygon (inscribed or circumscribed)", run: () => cadPolygon() },
    { alias: "SEC", name: "SECPROPS", desc: "Engineering cross-section properties (moments of inertia, section modulus)", run: () => cadSection() },
    { alias: "SECTION", name: "SECTION", desc: "Engineering cross-section properties (moments of inertia, section modulus)", run: () => cadSection() },
    { alias: "CAM2D", name: "CAM2D", desc: "Generate CNC toolpath and G-code from 2D profile", run: () => cadToCam() },
    { alias: "DI", name: "DIST", desc: "Measure distance, delta & angle between points", run: () => cadDist() },
    { alias: "AA", name: "AREA", desc: "Measure enclosed area and perimeter", run: () => cadArea() },
    { alias: "LI", name: "LIST", desc: "List properties of selected entity", run: () => cadList() },
    { alias: "3D", name: "EXTDRAFT", desc: "Extrude 2D sketch profile into 3D solid", run: () => cadToSolid() },
    { alias: "H", name: "HATCH", desc: "Crosshatch boundary with ANSI31 or NET pattern", run: () => cadHatch() },
    { alias: "BH", name: "BHATCH", desc: "Boundary crosshatch with ANSI31 pattern", run: () => cadHatch() },
    { alias: "MV", name: "MULTIVIEW", desc: "Generate 4-view orthographic drawing with cross-sections", run: () => cadMultiview() },
    { alias: "ORTHO", name: "ORTHOGRAPHIC", desc: "Generate 4-view orthographic drawing with cross-sections", run: () => cadMultiview() },
    { alias: "PU", name: "PURGE", desc: "Purge unused layers and unreferenced blocks", run: () => cadPurge() },
    { alias: "PURGE", name: "PURGE", desc: "Purge unused layers and unreferenced blocks", run: () => cadPurge() },
    { alias: "OV", name: "OVERKILL", desc: "Delete duplicates and merge collinear lines", run: () => cadOverkill() },
    { alias: "OVERKILL", name: "OVERKILL", desc: "Delete duplicates and merge collinear lines", run: () => cadOverkill() },
    { alias: "DIV", name: "DIVIDE", desc: "Divide entity into equal segments by placing points", run: () => cadDivide() },
    { alias: "DIVIDE", name: "DIVIDE", desc: "Divide entity into equal segments by placing points", run: () => cadDivide() },
    { alias: "ME", name: "MEASURE", desc: "Place points at length intervals along entity", run: () => cadMeasureInterval() },
    { alias: "MEASURE", name: "MEASURE", desc: "Place points at length intervals along entity", run: () => cadMeasureInterval() },
    { alias: "MA", name: "MATCHPROP", desc: "Match layer and properties across entities", run: () => cadMatchProp() },
    { alias: "MATCHPROP", name: "MATCHPROP", desc: "Match layer and properties across entities", run: () => cadMatchProp() },
    { alias: "CON", name: "CONSTRAIN", desc: "Solve 2D geometric and dimensional parametric constraints", run: () => cadConstraintsSolve() },
    { alias: "PARAMETRIC", name: "PARAMETRIC", desc: "Solve 2D geometric and dimensional parametric constraints", run: () => cadConstraintsSolve() },
    { alias: "DOF", name: "DOF", desc: "Analyze degrees of freedom and solve constraints", run: () => cadConstraintsSolve() },
    { alias: "E", name: "ERASE", desc: "Delete selected drawing entity", run: () => deleteSelected() },
    { alias: "U", name: "UNDO", desc: "Undo last operation", run: () => undo() },
    { alias: "REDO", name: "REDO", desc: "Redo last operation", run: () => redo() },
    { alias: "F3", name: "OSNAP", desc: "Toggle object snap (13 precision modes)", run: () => {
      drawState.osnap = !drawState.osnap;
      byId("osnapToggleBtn")?.setAttribute("aria-pressed", String(drawState.osnap));
      const badge = byId("statusOsnapIndicator");
      if (badge) {
        badge.textContent = `OSNAP: ${drawState.osnap ? "ON" : "OFF"}`;
        badge.classList.toggle("active", drawState.osnap);
      }
      toast(`Object Snap ${drawState.osnap ? "ON" : "OFF"}`);
    }},
    { alias: "F10", name: "POLAR", desc: "Toggle polar tracking angles", run: () => {
      drawState.polar = !drawState.polar;
      byId("polarToggleBtn")?.setAttribute("aria-pressed", String(drawState.polar));
      const badge = byId("statusPolarIndicator");
      if (badge) {
        badge.textContent = `POLAR: ${drawState.polar ? "ON" : "OFF"}`;
        badge.classList.toggle("active", drawState.polar);
      }
      toast(`Polar Tracking ${drawState.polar ? "ON" : "OFF"}`);
    }},
    { alias: "Z", name: "ZOOM", desc: "Zoom to fit model in viewport", run: () => fit() },
    { alias: "W", name: "WIREFRAME", desc: "Toggle wireframe mesh mode", run: () => toggleWireframe() },
    { alias: "G", name: "GRID", desc: "Toggle ground grid", run: () => toggleGrid() },
    { alias: "EXT", name: "EXTRUDE", desc: "3D extrude selected sketch", run: () => extrudeSketch("new_body") },
    { alias: "REV", name: "REVOLVE", desc: "3D revolve selected sketch", run: () => revolveSketch("new_body") },
    { alias: "FEA", name: "STATIC", desc: "Run linear static FEA (stress & deflection)", run: () => runCaeStatic() },
    { alias: "MODAL", name: "VIBRATION", desc: "Run dynamic modal natural frequency analysis", run: () => runCaeModal() },
    { alias: "HEAT", name: "THERMAL", desc: "Run steady-state thermal conduction FEA", run: () => runCaeThermal() },
    { alias: "THERMAL", name: "THERMAL", desc: "Run steady-state thermal conduction FEA", run: () => runCaeThermal() },
    { alias: "SIM", name: "SIMULATION", desc: "Open Engineering Multiphysics & CAM simulation launcher", run: () => openSimLauncher() },
    { alias: "SIMULATION", name: "SIMULATION", desc: "Open Engineering Multiphysics & CAM simulation launcher", run: () => openSimLauncher() },
    { alias: "A", name: "ARC", desc: "Draw arc or circle segment", run: () => setDrawTool("circle") },
    { alias: "ARC", name: "ARC", desc: "Draw arc or circle segment", run: () => setDrawTool("circle") },
    { alias: "MT", name: "MTEXT", desc: "Place multiline annotation text", run: () => setDrawTool("text") },
    { alias: "MTEXT", name: "MTEXT", desc: "Place multiline annotation text", run: () => setDrawTool("text") },
    { alias: "P", name: "PAN", desc: "Pan view (Middle drag in 2D, Shift drag in 3D)", run: () => toast("Pan: drag with middle mouse button or Shift+drag") },
    { alias: "PAN", name: "PAN", desc: "Pan view (Middle drag in 2D, Shift drag in 3D)", run: () => toast("Pan: drag with middle mouse button or Shift+drag") },
    { alias: "RE", name: "REGEN", desc: "Regenerate drawing and 3D graphics", run: () => { refreshDrawing(); if (app.viewport) app.viewport.requestFrame(); toast("Regenerated display graphics"); } },
    { alias: "REGEN", name: "REGEN", desc: "Regenerate drawing and 3D graphics", run: () => { refreshDrawing(); if (app.viewport) app.viewport.requestFrame(); toast("Regenerated display graphics"); } },
    { alias: "CAM", name: "GCODE", desc: "Generate 3-axis CNC toolpaths & G-code", run: () => runCamToolpaths() },
    { alias: "DXF", name: "DXFOUT", desc: "Export plan drawing as AutoCAD R12 DXF", run: () => exportDxf() },
    { alias: "PDF", name: "PLOT", desc: "Plot drawing as ISO/ANSI vector sheet", run: () => plotPdf() },
    { alias: "STL", name: "STLOUT", desc: "Export binary 3D mesh STL", run: () => exportStl() },
    { alias: "QSAVE", name: "SAVE", desc: "Save project and drawing", run: () => save() },
    { alias: "HELP", name: "?", desc: "Show keyboard shortcuts & command reference", run: () => openHelp() },
  ];

  function handleCliInput(rawText) {
    const text = rawText.trim();
    if (!text) {
      if (lastExecutedCommand) {
        lastExecutedCommand.run();
        toast(`Repeated: ${lastExecutedCommand.name}`);
      }
      return;
    }
    cliHistory.push(text);
    cliHistoryIndex = cliHistory.length;

    // Check for polyline close/finish commands
    if ((drawState.tool === "polyline" || drawState.tool === "spline") && drawState.points.length >= 2) {
      if (text.toUpperCase() === "C" || text.toUpperCase() === "CLOSE") {
        finishPolyline(true);
        toast(`Closed ${drawState.tool}`);
        return;
      }
      if (text.toUpperCase() === "F" || text.toUpperCase() === "FINISH" || text.toUpperCase() === "O" || text.toUpperCase() === "OPEN") {
        finishPolyline(false);
        toast(`Finished ${drawState.tool}`);
        return;
      }
    }

    // Try parsing as coordinate or distance if a drawing tool is active
    if (drawState.tool !== "select") {
      const coordPoint = parseCoordinateString(text);
      if (coordPoint) {
        placeDrawingPoint(coordPoint);
        toast(`Specified point: (${coordPoint[0]}, ${coordPoint[1]})`);
        return;
      }
    }

    // Match command alias or name
    const upper = text.toUpperCase();

    // Single-shot OSNAP overrides
    const snapMap = {
      END: "Endpoint",
      MID: "Midpoint",
      CEN: "Center",
      INT: "Intersection",
      PER: "Perpendicular",
      TAN: "Tangent",
      QUA: "Quadrant",
      NOD: "Node",
      NEA: "Nearest",
    };
    if (snapMap[upper]) {
      drawState.snapOverride = snapMap[upper];
      document.querySelectorAll(".snap-pill").forEach((b) => {
        b.classList.toggle("active", b.dataset.snap === snapMap[upper]);
      });
      toast(`Single-shot Snap: ${drawState.snapOverride}`);
      return;
    }

    const matched = autocadCommands.find((c) => c.alias === upper || c.name === upper);
    if (matched) {
      lastExecutedCommand = matched;
      matched.run();
      toast(`Command: ${matched.name} (${matched.desc})`);
      return;
    }

    // Partial prefix match
    const prefixMatched = autocadCommands.find((c) => c.name.startsWith(upper) || c.alias.startsWith(upper));
    if (prefixMatched) {
      lastExecutedCommand = prefixMatched;
      prefixMatched.run();
      toast(`Command: ${prefixMatched.name} (${prefixMatched.desc})`);
      return;
    }

    toast(`Unknown command: "${text}". Type HELP for reference.`, true);
  }

  function parseCoordinateString(text) {
    // Relative Polar: @dist<angle
    if (text.startsWith("@") && text.includes("<")) {
      const parts = text.slice(1).split("<");
      if (parts.length === 2) {
        const dist = parseFloat(parts[0]);
        const angleDeg = parseFloat(parts[1]);
        if (!isNaN(dist) && !isNaN(angleDeg)) {
          const rad = angleDeg * (Math.PI / 180);
          const base = drawState.points.length ? drawState.points[drawState.points.length - 1] : [0, 0];
          return [Math.round((base[0] + dist * Math.cos(rad)) * 1000) / 1000, Math.round((base[1] + dist * Math.sin(rad)) * 1000) / 1000];
        }
      }
    }
    // Absolute Polar: dist<angle
    if (!text.startsWith("@") && text.includes("<")) {
      const parts = text.split("<");
      if (parts.length === 2) {
        const dist = parseFloat(parts[0]);
        const angleDeg = parseFloat(parts[1]);
        if (!isNaN(dist) && !isNaN(angleDeg)) {
          const rad = angleDeg * (Math.PI / 180);
          return [Math.round((dist * Math.cos(rad)) * 1000) / 1000, Math.round((dist * Math.sin(rad)) * 1000) / 1000];
        }
      }
    }
    if (text.startsWith("@")) {
      const parts = text.slice(1).trim().split(/[\s,]+/);
      if (parts.length === 2) {
        const dx = parseFloat(parts[0]);
        const dy = parseFloat(parts[1]);
        if (!isNaN(dx) && !isNaN(dy)) {
          const base = drawState.points.length ? drawState.points[drawState.points.length - 1] : [0, 0];
          return [Math.round((base[0] + dx) * 1000) / 1000, Math.round((base[1] + dy) * 1000) / 1000];
        }
      }
    }
    const parts = text.split(/[\s,]+/);
    if (parts.length === 2) {
      const x = parseFloat(parts[0]);
      const y = parseFloat(parts[1]);
      if (!isNaN(x) && !isNaN(y)) {
        return [Math.round(x * 1000) / 1000, Math.round(y * 1000) / 1000];
      }
    }
    if (parts.length === 1) {
      const dist = parseFloat(parts[0]);
      if (!isNaN(dist) && dist !== 0) {
        const base = drawState.points.length ? drawState.points[drawState.points.length - 1] : [0, 0];
        let angle = 0;
        if (drawState.polarHit) {
          angle = drawState.polarHit.angle_deg * (Math.PI / 180);
        }
        return [
          Math.round((base[0] + dist * Math.cos(angle)) * 1000) / 1000,
          Math.round((base[1] + dist * Math.sin(angle)) * 1000) / 1000,
        ];
      }
    }
    return null;
  }

  // ---------------------------------------------------------------------------------------
  // Command palette
  // ---------------------------------------------------------------------------------------

  function commandList() {
    const project = app.project;
    const commands = [
      { label: "Save project", hint: "Ctrl S", keywords: "save write", run: () => save() },
      { label: "Undo", hint: "Ctrl Z", keywords: "history back", run: () => undo(), disabled: !project || project.history.undo === 0 },
      { label: "Redo", hint: "Ctrl Y", keywords: "history forward", run: () => redo(), disabled: !project || project.history.redo === 0 },
      { label: "Export binary STL", hint: "download", keywords: "export mesh stl", run: () => exportStl(), disabled: !project || project.features.length === 0 },
      { label: "Export plan drawing as DXF", hint: "download", keywords: "export draft drawing dxf plan 2d", run: () => exportDxf(), disabled: !project },
      { label: "Plot the drawing as a PDF sheet", hint: "print", keywords: "plot print pdf sheet paper a3", run: () => plotPdf(), disabled: !project },
      { label: "Save the drawing beside the project", hint: ".zdraft", keywords: "drawing save zdraft draft", run: () => saveDrawing(), disabled: !project },
      { label: "Drawing: undo", hint: "", keywords: "drawing draft undo", run: () => drawingMutate("/api/draft/undo", {}, "Drawing undone"), disabled: !project },
      { label: "Drawing tool: line (L)", hint: "2 clicks", keywords: "draw line l", run: () => setDrawTool("line"), disabled: !project },
      { label: "Drawing tool: circle (C)", hint: "centre, edge", keywords: "draw circle hole c", run: () => setDrawTool("circle"), disabled: !project },
      { label: "Drawing tool: rectangle (REC)", hint: "corners", keywords: "draw rectangle box rec", run: () => setDrawTool("rectangle"), disabled: !project },
      { label: "Drawing tool: ellipse (EL)", hint: "centre, radii", keywords: "draw ellipse oval el", run: () => setDrawTool("ellipse"), disabled: !project },
      { label: "Drawing tool: polyline (PL)", hint: "multi-segment", keywords: "draw polyline pl", run: () => setDrawTool("polyline"), disabled: !project },
      { label: "Drawing tool: spline (SPL)", hint: "smooth curve", keywords: "draw spline curve spl", run: () => setDrawTool("spline"), disabled: !project },
      { label: "Drawing tool: point (PO)", hint: "single click", keywords: "draw point po", run: () => setDrawTool("point"), disabled: !project },
      { label: "Drawing tool: dimension (DIM)", hint: "2 clicks", keywords: "draw dimension measure dim", run: () => setDrawTool("dimension"), disabled: !project },
      { label: "Drawing tool: text (T)", hint: "click to place", keywords: "draw text annotation t dt", run: () => setDrawTool("text"), disabled: !project },
      { label: "Drawing: finish polyline/spline", hint: "Enter", keywords: "finish polyline spline open", run: () => finishPolyline(false), disabled: !project },
      { label: "Drawing: close polyline/spline (C)", hint: "Close", keywords: "close polyline spline", run: () => finishPolyline(true), disabled: !project },
      { label: "Drawing: delete selected entity (DEL)", hint: "Erase", keywords: "delete erase del remove entity", run: () => deleteSelected(), disabled: !project },
      { label: "CAE: Run linear static structural FEA (FEA)", hint: "stress/deflection", keywords: "cae fea static stress deflection simulation", run: () => runCaeStatic(), disabled: !project || project.features.length === 0 },
      { label: "CAE: Run dynamic modal vibration analysis (MODAL)", hint: "frequencies", keywords: "cae modal vibration frequency natural modes", run: () => runCaeModal(), disabled: !project || project.features.length === 0 },
      { label: "CAE: Run steady-state thermal conduction FEA (THERMAL)", hint: "heat flux", keywords: "cae thermal heat conduction temperature fea", run: () => runCaeThermal(), disabled: !project || project.features.length === 0 },
      { label: "CAM: Generate 3-axis CNC toolpaths & G-code (CAM)", hint: "milling/gcode", keywords: "cam cnc toolpath gcode machining milling", run: () => runCamToolpaths(), disabled: !project || project.features.length === 0 },
      { label: "Engineering: Open Multiphysics & CAM Simulation Launcher", hint: "simulation", keywords: "simulation fea modal thermal cam gcode multiphysics", run: () => openSimLauncher() },
      { label: "Save a named revision", hint: "snapshot", keywords: "revision snapshot named save checkpoint", run: () => addRevision(), disabled: !project },
      ...(project && Array.isArray(project.revisions)
        ? project.revisions.map((revision) => ({
            label: `Restore revision “${revision.name}”`,
            hint: formatRevisionTime(revision.created_at_unix),
            keywords: `revision restore revert ${revision.name}`,
            run: () => restoreRevision(revision.id, revision.name),
          }))
        : []),
      { label: "Fit model to view", hint: "F", keywords: "zoom frame camera", run: () => fit() },
      { label: "Toggle wireframe", hint: "W", keywords: "edges display", run: () => toggleWireframe() },
      { label: "Toggle grid", hint: "G", keywords: "floor display", run: () => toggleGrid() },
      { label: "View: isometric", hint: "4", keywords: "camera iso", run: () => setView("iso") },
      { label: "View: front", hint: "1", keywords: "camera front", run: () => setView("front") },
      { label: "View: top", hint: "2", keywords: "camera top", run: () => setView("top") },
      { label: "View: right", hint: "3", keywords: "camera right", run: () => setView("right") },
      { label: "Profile: rectangle", hint: "box", keywords: "sketch shape box rectangle", run: () => setProfile("rectangle") },
      { label: "Profile: circle", hint: "disc", keywords: "sketch shape circle cylinder disc", run: () => setProfile("circle") },
      { label: "Profile: regular polygon", hint: "prism", keywords: "sketch shape polygon prism nut", run: () => setProfile("regular_polygon") },
      { label: "New sketch: rectangle", hint: "create", keywords: "add create sketch rectangle box", run: () => createSketch("rectangle") },
      { label: "New sketch: circle", hint: "create", keywords: "add create sketch circle cylinder disc", run: () => createSketch("circle") },
      { label: "New sketch: regular polygon", hint: "create", keywords: "add create sketch polygon prism nut", run: () => createSketch("regular_polygon") },
      { label: "New sketch: polyline (draw in the viewport)", hint: "draw", keywords: "add create sketch polyline path draw freeform bracket", run: () => createSketch("polyline") },
      { label: "Extrude the selected sketch as a new body", hint: "solid", keywords: "add create extrude solid new body feature", run: () => extrudeSketch("new_body") },
      { label: "Extrude the selected sketch as a join", hint: "union", keywords: "add create extrude join union boolean feature", run: () => extrudeSketch("join") },
      { label: "Extrude the selected sketch as a cut", hint: "difference", keywords: "add create extrude cut difference boolean feature", run: () => extrudeSketch("cut") },
      { label: "Revolve the selected sketch as a new body", hint: "turn", keywords: "add create revolve lathe turn axis solid new body feature", run: () => revolveSketch("new_body") },
      { label: "Revolve the selected sketch as a join", hint: "union", keywords: "add create revolve lathe turn axis join union boolean feature", run: () => revolveSketch("join") },
      { label: "Toggle light/dark theme", hint: "appearance", keywords: "theme dark light", run: () => toggleTheme() },
      { label: "Keyboard shortcuts", hint: "?", keywords: "help keys", run: () => openHelp() },
    ];
    if (project) {
      for (const parameter of project.parameters) {
        commands.push({
          label: `Set parameter “${parameter.id}”`,
          hint: formatValue(parameter.value),
          keywords: `parameter dimension ${parameter.id}`,
          run: () => focusParameter(parameter.id),
        });
      }
    }
    return commands.filter((command) => !command.disabled);
  }

  function openPalette() {
    app.palette.open = true;
    byId("palette").hidden = false;
    byId("paletteInput").value = "";
    app.palette.index = 0;
    renderPalette("");
    byId("paletteInput").focus();
  }

  function closePalette() {
    app.palette.open = false;
    byId("palette").hidden = true;
    app.palette.items = [];
  }

  function renderPalette(query) {
    const list = byId("paletteList");
    const needle = query.trim().toLowerCase();
    const commands = commandList().filter((command) => {
      if (!needle) return true;
      return `${command.label} ${command.keywords} ${command.hint}`.toLowerCase().includes(needle);
    });
    app.palette.items = commands.slice(0, 40);
    app.palette.index = Math.min(app.palette.index, Math.max(0, app.palette.items.length - 1));
    list.textContent = "";
    if (!commands.length) {
      const empty = element("li", "empty", "No command matches that search.");
      list.append(empty);
      return;
    }
    app.palette.items.forEach((command, index) => {
      const item = document.createElement("li");
      item.setAttribute("role", "option");
      item.setAttribute("aria-selected", index === app.palette.index ? "true" : "false");
      const button = document.createElement("button");
      button.type = "button";
      button.append(
        element("span", "palette-label", command.label),
        element("span", "palette-hint", command.hint || "")
      );
      button.addEventListener("click", () => runCommand(command));
      item.append(button);
      list.append(item);
    });
    const selected = list.querySelector('li[aria-selected="true"]');
    if (selected) selected.scrollIntoView({ block: "nearest" });
  }

  function movePalette(delta) {
    if (!app.palette.items.length) return;
    app.palette.index = (app.palette.index + delta + app.palette.items.length) % app.palette.items.length;
    renderPalette(byId("paletteInput").value);
  }

  function runCommand(command) {
    closePalette();
    try {
      command.run();
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      toast(message, true);
      log(message, "error");
    }
  }

  // ---------------------------------------------------------------------------------------
  // Theme, help, view shortcuts
  // ---------------------------------------------------------------------------------------

  function applyTheme(theme) {
    app.theme = theme === "light" ? "light" : "dark";
    document.documentElement.dataset.theme = app.theme;
    try {
      localStorage.setItem("atlas-theme", app.theme);
    } catch (error) {
      // Private browsing may refuse storage; the theme still applies for this session.
    }
    if (app.viewport) app.viewport.refreshTheme();
  }

  function toggleTheme() {
    applyTheme(app.theme === "dark" ? "light" : "dark");
  }

  function openHelp() {
    app.helpOpen = true;
    byId("help").hidden = false;
  }

  function closeHelp() {
    app.helpOpen = false;
    byId("help").hidden = true;
  }

  function setView(name) {
    if (app.viewport) {
      app.viewport.setView(name);
      log(`View: ${name}`);
    }
  }

  function closeDrawers() {
    document.body.classList.remove("panel-left-open", "panel-right-open");
    for (const id of ["projectBtn", "inspectorBtn"]) {
      const button = byId(id);
      if (button) button.setAttribute("aria-expanded", "false");
    }
  }

  function toggleDrawer(side) {
    const openClass = side === "left" ? "panel-left-open" : "panel-right-open";
    const willOpen = !document.body.classList.contains(openClass);
    closeDrawers();
    if (willOpen) document.body.classList.add(openClass);
    byId(side === "left" ? "projectBtn" : "inspectorBtn")
      .setAttribute("aria-expanded", String(willOpen));
  }

  // ---------------------------------------------------------------------------------------
  // Boot and events
  // ---------------------------------------------------------------------------------------

  function bindUi() {
    byId("undoBtn").addEventListener("click", undo);
    byId("redoBtn").addEventListener("click", redo);
    byId("saveBtn").addEventListener("click", save);
    byId("copyShareBtn").addEventListener("click", copyShareLink);
    byId("recoveryDiscard").addEventListener("click", discardRecovery);
    byId("exportBtn").addEventListener("click", exportStl);
    byId("downloadDxfBtn").addEventListener("click", exportDxf);
    for (const button of document.querySelectorAll(".draw-tools [data-tool]")) {
      button.addEventListener("click", () => setDrawTool(button.dataset.tool));
    }
    const preview = byId("drawingPreview");
    let isPanningDrawing = false;
    let panStart = [0, 0];

    preview.addEventListener("wheel", (event) => {
      const svg = preview.querySelector("svg");
      if (!svg || !app.drawing) return;
      event.preventDefault();
      const factor = Math.exp(-event.deltaY * 0.0015);
      drawState.viewZoom = Math.min(50, Math.max(0.1, (drawState.viewZoom || 1.0) * factor));
      renderDrawing(app.drawing);
    }, { passive: false });

    preview.addEventListener("pointerdown", (event) => {
      if (event.button === 1 || (event.button === 0 && event.altKey)) {
        isPanningDrawing = true;
        panStart = [event.clientX, event.clientY];
        preview.setPointerCapture(event.pointerId);
        event.preventDefault();
      }
    });

    preview.addEventListener("pointerup", (event) => {
      if (isPanningDrawing) {
        isPanningDrawing = false;
        try { preview.releasePointerCapture(event.pointerId); } catch (_) {}
      }
    });

    preview.addEventListener("dblclick", () => {
      drawState.viewZoom = 1.0;
      drawState.viewPan = [0, 0];
      if (app.drawing) renderDrawing(app.drawing);
    });

    preview.addEventListener("pointermove", (event) => {
      if (isPanningDrawing) {
        const svg = preview.querySelector("svg");
        if (svg && panStart) {
          const box = svg.viewBox && svg.viewBox.baseVal;
          const rect = svg.getBoundingClientRect();
          if (box && box.width > 0 && box.height > 0 && rect && rect.width > 0 && rect.height > 0) {
            const scale = Math.min(rect.width / box.width, rect.height / box.height);
            if (scale > 0 && Number.isFinite(scale)) {
              const dx = event.clientX - panStart[0];
              const dy = event.clientY - panStart[1];
              panStart = [event.clientX, event.clientY];
              if (!drawState.viewPan) drawState.viewPan = [0, 0];
              drawState.viewPan[0] -= dx / scale;
              drawState.viewPan[1] += dy / scale;
              renderDrawing(app.drawing);
            }
          }
        }
        return;
      }
      const svg = preview.querySelector("svg");
      if (!svg || !app.drawing || drawState.tool === "select") {
        if (drawState.activeSnap || drawState.polarHit) {
          drawState.activeSnap = null;
          drawState.polarHit = null;
          if (svg) updateSnapOverlay(svg);
        }
        return;
      }
      const rawPoint = modelPointFromEvent(svg, event);
      if (!rawPoint) return;
      const coordsEl = byId("statusCoords");
      if (coordsEl) coordsEl.textContent = `X: ${rawPoint[0].toFixed(3)}, Y: ${rawPoint[1].toFixed(3)}`;
      const box = svg.viewBox.baseVal;
      const rect = svg.getBoundingClientRect();
      const scale = Math.min(rect.width / box.width, rect.height / box.height);

      const basePoint = drawState.points.length ? drawState.points[drawState.points.length - 1] : null;
      let snap = null;
      if (drawState.osnap || drawState.snapOverride) {
        snap = findObjectSnap(app.drawing, rawPoint, basePoint, scale, 12);
      }
      let polar = null;
      if (!snap && drawState.polar && basePoint) {
        polar = findPolarTracking(basePoint, rawPoint, scale, 12);
      }
      drawState.activeSnap = snap;
      drawState.polarHit = polar;
      updateSnapOverlay(svg);
    });

    preview.addEventListener("pointerleave", () => {
      const svg = preview.querySelector("svg");
      if (drawState.activeSnap || drawState.polarHit) {
        drawState.activeSnap = null;
        drawState.polarHit = null;
        if (svg) updateSnapOverlay(svg);
      }
    });

    preview.addEventListener("click", (event) => {
      if (event.button !== 0 || event.altKey) return;
      const svg = preview.querySelector("svg");
      if (!svg || !app.drawing) return;
      if (drawState.tool === "select") {
        const target = event.target;
        const node = target && typeof target.closest === "function"
          ? target.closest("[data-entity-id]")
          : null;
        drawState.selected = node ? Number(node.dataset.entityId) : null;
        renderDrawing(app.drawing);
        return;
      }
      let point = modelPointFromEvent(svg, event);
      if (drawState.activeSnap) {
        point = drawState.activeSnap.point;
      } else if (drawState.polarHit) {
        point = drawState.polarHit.point;
      }
      if (point) placeDrawingPoint(point);
      if (drawState.snapOverride) {
        drawState.snapOverride = null;
        document.querySelectorAll(".snap-pill").forEach((b) => b.classList.remove("active"));
      }
    });

    function syncOsnapPolarBadges() {
      const osnapBadge = byId("statusOsnapIndicator");
      if (osnapBadge) {
        osnapBadge.textContent = `OSNAP: ${drawState.osnap ? "ON" : "OFF"}`;
        osnapBadge.classList.toggle("active", drawState.osnap);
      }
      const polarBadge = byId("statusPolarIndicator");
      if (polarBadge) {
        polarBadge.textContent = `POLAR: ${drawState.polar ? "ON" : "OFF"}`;
        polarBadge.classList.toggle("active", drawState.polar);
      }
    }

    byId("osnapToggleBtn").addEventListener("click", () => {
      drawState.osnap = !drawState.osnap;
      byId("osnapToggleBtn").setAttribute("aria-pressed", String(drawState.osnap));
      syncOsnapPolarBadges();
      toast(`Object Snap ${drawState.osnap ? "ON" : "OFF"}`);
    });
    byId("polarToggleBtn").addEventListener("click", () => {
      drawState.polar = !drawState.polar;
      byId("polarToggleBtn").setAttribute("aria-pressed", String(drawState.polar));
      syncOsnapPolarBadges();
      toast(`Polar Tracking ${drawState.polar ? "ON" : "OFF"}`);
    });
    byId("statusOsnapIndicator")?.addEventListener("click", () => {
      drawState.osnap = !drawState.osnap;
      byId("osnapToggleBtn")?.setAttribute("aria-pressed", String(drawState.osnap));
      syncOsnapPolarBadges();
      toast(`Object Snap ${drawState.osnap ? "ON" : "OFF"}`);
    });
    byId("statusPolarIndicator")?.addEventListener("click", () => {
      drawState.polar = !drawState.polar;
      byId("polarToggleBtn")?.setAttribute("aria-pressed", String(drawState.polar));
      syncOsnapPolarBadges();
      toast(`Polar Tracking ${drawState.polar ? "ON" : "OFF"}`);
    });

    const cliInput = byId("commandPromptInput");
    const cliSuggestions = byId("commandSuggestions");

    cliInput?.addEventListener("input", () => {
      const val = cliInput.value.trim().toUpperCase();
      if (!val) {
        cliSuggestions.hidden = true;
        return;
      }
      const matches = autocadCommands.filter((c) =>
        c.alias.startsWith(val) || c.name.startsWith(val) || c.desc.toUpperCase().includes(val)
      );
      if (matches.length > 0) {
        cliSuggestions.innerHTML = "";
        matches.slice(0, 8).forEach((m, idx) => {
          const item = document.createElement("div");
          item.className = `command-suggestion-item ${idx === 0 ? "active" : ""}`;
          item.innerHTML = `
            <span class="suggestion-alias">${m.alias} (${m.name})</span>
            <span class="suggestion-desc">${m.desc}</span>
          `;
          item.addEventListener("mousedown", (e) => {
            e.preventDefault();
            cliInput.value = "";
            cliSuggestions.hidden = true;
            m.run();
          });
          cliSuggestions.appendChild(item);
        });
        cliSuggestions.hidden = false;
      } else {
        cliSuggestions.hidden = true;
      }
    });

    cliInput?.addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        const val = cliInput.value.trim();
        cliSuggestions.hidden = true;
        cliInput.value = "";
        handleCliInput(val);
      } else if (event.key === "Escape") {
        cliInput.value = "";
        cliSuggestions.hidden = true;
        cliInput.blur();
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        if (cliHistory.length > 0 && cliHistoryIndex > 0) {
          cliHistoryIndex--;
          cliInput.value = cliHistory[cliHistoryIndex];
        }
      } else if (event.key === "ArrowDown") {
        event.preventDefault();
        if (cliHistoryIndex < cliHistory.length - 1) {
          cliHistoryIndex++;
          cliInput.value = cliHistory[cliHistoryIndex];
        } else {
          cliHistoryIndex = cliHistory.length;
          cliInput.value = "";
        }
      }
    });
    byId("polylineFinishBtn").addEventListener("click", () => finishPolyline(false));
    byId("polylineCloseBtn").addEventListener("click", () => finishPolyline(true));
    byId("polylineCancelBtn").addEventListener("click", () => {
      drawState.points = [];
      renderDrawing(app.drawing);
    });
    byId("drawUndoBtn").addEventListener("click", () =>
      drawingMutate("/api/draft/undo", {}, "Drawing undone")
    );
    byId("drawDeleteBtn").addEventListener("click", deleteSelected);
    byId("toolOffsetBtn")?.addEventListener("click", () => cadOffset());
    byId("toolFilletBtn")?.addEventListener("click", () => cadFillet());
    byId("toolChamferBtn")?.addEventListener("click", () => cadChamfer());
    byId("toolMirrorBtn")?.addEventListener("click", () => cadMirror());
    byId("toolRotateBtn")?.addEventListener("click", () => cadRotate());
    byId("toolScaleBtn")?.addEventListener("click", () => cadScale());
    byId("toolMoveBtn")?.addEventListener("click", () => cadMove());
    byId("toolCopyBtn")?.addEventListener("click", () => cadCopy());
    byId("toolExplodeBtn")?.addEventListener("click", () => cadExplode());
    byId("toolTrimBtn")?.addEventListener("click", () => cadTrim());
    byId("toolExtendBtn")?.addEventListener("click", () => cadExtend());
    byId("toolJoinBtn")?.addEventListener("click", () => cadJoin());
    byId("toolArrayBtn")?.addEventListener("click", () => cadArray());
    byId("toolPolygonBtn")?.addEventListener("click", () => cadPolygon());
    byId("toolDimRadiusBtn")?.addEventListener("click", () => cadDimRadial(false));
    byId("toolDimDiameterBtn")?.addEventListener("click", () => cadDimRadial(true));
    byId("toolBlockBtn")?.addEventListener("click", () => cadBlock());
    byId("toolInsertBtn")?.addEventListener("click", () => cadInsert());
    byId("toolSectionBtn")?.addEventListener("click", () => cadSection());
    byId("toolDistBtn")?.addEventListener("click", () => cadDist());
    byId("toolAreaBtn")?.addEventListener("click", () => cadArea());
    byId("toolHatchBtn")?.addEventListener("click", () => cadHatch());
    byId("toolPurgeBtn")?.addEventListener("click", () => cadPurge());
    byId("toolOverkillBtn")?.addEventListener("click", () => cadOverkill());
    byId("toolDivideBtn")?.addEventListener("click", () => cadDivide());
    byId("toolMeasureIntervalBtn")?.addEventListener("click", () => cadMeasureInterval());
    byId("toolMatchPropBtn")?.addEventListener("click", () => cadMatchProp());
    byId("toolParametricBtn")?.addEventListener("click", () => cadConstraintsSolve());
    byId("toolToSolidBtn")?.addEventListener("click", () => cadToSolid());
    byId("toolToCamBtn")?.addEventListener("click", () => cadToCam());
    byId("toolMultiviewBtn")?.addEventListener("click", () => cadMultiview());
    document.querySelectorAll(".snap-pill").forEach((btn) => {
      btn.addEventListener("click", () => {
        const snapType = btn.dataset.snap;
        if (drawState.snapOverride === snapType) {
          drawState.snapOverride = null;
          btn.classList.remove("active");
          toast("Snap override cleared");
        } else {
          drawState.snapOverride = snapType;
          document.querySelectorAll(".snap-pill").forEach((b) => b.classList.remove("active"));
          btn.classList.add("active");
          toast(`Single-shot Snap: ${snapType}`);
        }
      });
    });
    byId("layerSelect").addEventListener("change", () => {
      drawState.layer = byId("layerSelect").value;
      syncDrawingControls();
    });
    byId("layerVisibleBtn").addEventListener("click", () => {
      const layer = drawingLayer(byId("layerSelect").value);
      setLayerFlag("visible", !(layer && layer.visible !== false));
    });
    byId("layerLockBtn").addEventListener("click", () => {
      const layer = drawingLayer(byId("layerSelect").value);
      setLayerFlag("locked", !(layer && layer.locked));
    });
    byId("layerLinetypeSelect")?.addEventListener("change", (e) => {
      const layerName = byId("layerSelect").value;
      if (layerName) {
        drawingMutate("/api/draft/layer", {
          name: layerName,
          linetype: e.target.value,
        }, `Linetype set to ${e.target.value}`);
      }
    });
    byId("addLayerBtn").addEventListener("click", addLayer);
    byId("layerName").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        addLayer();
      }
    });
    byId("saveDrawingBtn").addEventListener("click", saveDrawing);
    byId("plotPdfBtn").addEventListener("click", plotPdf);
    byId("paletteBtn").addEventListener("click", openPalette);
    byId("simBtn")?.addEventListener("click", openSimLauncher);
    byId("themeBtn").addEventListener("click", toggleTheme);
    byId("fitBtn").addEventListener("click", fit);
    byId("wireBtn").addEventListener("click", toggleWireframe);
    byId("gridBtn").addEventListener("click", toggleGrid);
    document.querySelectorAll(".axis-gizmo button").forEach((btn) => {
      btn.addEventListener("click", (e) => {
        e.stopPropagation();
        const view = btn.dataset.axis;
        if (view) setView(view);
      });
    });
    byId("clearLog").addEventListener("click", () => {
      byId("log").textContent = "";
    });
    byId("pickClose").addEventListener("click", clearPick);
    byId("projectBtn").addEventListener("click", () => toggleDrawer("left"));
    byId("inspectorBtn").addEventListener("click", () => toggleDrawer("right"));
    byId("drawerScrim").addEventListener("click", closeDrawers);
    if (window.matchMedia) {
      const wide = window.matchMedia("(min-width: 1181px)");
      if (typeof wide.addEventListener === "function") {
        wide.addEventListener("change", (event) => {
          if (event.matches) closeDrawers();
        });
      }
    }
    for (const button of document.querySelectorAll(".view-toolbar [data-view]")) {
      button.addEventListener("click", () => setView(button.dataset.view));
    }
    byId("profileKind").addEventListener("change", () => {
      setProfile(byId("profileKind").value);
    });
    byId("sidesInput").addEventListener("change", () => {
      if (byId("profileKind").value === "regular_polygon") setProfile("regular_polygon");
    });
    byId("sidesInput").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        setProfile("regular_polygon");
        byId("sidesInput").blur();
      }
    });
    byId("sidesInput").addEventListener("blur", () => {
      if (byId("profileKind").value === "regular_polygon") setProfile("regular_polygon");
    });
    for (const id of ["sectorStart", "sectorSweep"]) {
      byId(id).addEventListener("change", () => {
        if (byId("profileKind").value === "sector") setProfile("sector");
      });
      byId(id).addEventListener("keydown", (event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          if (byId("profileKind").value === "sector") setProfile("sector");
          byId(id).blur();
        }
      });
    }
    byId("addRevisionBtn").addEventListener("click", addRevision);
    byId("revisionName").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        addRevision();
      }
    });
    byId("addSketchBtn").addEventListener("click", addSketch);
    byId("addFeatureBtn").addEventListener("click", addFeature);
    byId("newFeatureKind").addEventListener("change", () => {
      const revolves = byId("newFeatureKind").value === "revolve";
      byId("newDepthField").hidden = revolves;
      byId("newAxisField").hidden = !revolves;
      byId("addFeatureBtn").textContent = revolves ? "Add revolve" : "Add extrude";
    });
    byId("moveSketchBtn").addEventListener("click", moveSketch);
    byId("drawUndoPoint").addEventListener("click", undoDrawPoint);
    byId("drawClose").addEventListener("click", finishDrawing);
    byId("drawCancel").addEventListener("click", cancelDrawing);
    byId("failureLocate").addEventListener("click", locateFailure);
    byId("newSketchKind").addEventListener("change", () => {
      const kind = byId("newSketchKind").value;
      byId("newSidesField").hidden = kind !== "regular_polygon";
      byId("newSectorField").hidden = kind !== "sector";
      byId("addSketchBtn").textContent = kind === "polyline" ? "Draw polyline" : "Add sketch";
      if (kind === "regular_polygon") byId("newSidesInput").focus();
      if (kind === "sector") byId("newSectorStart").focus();
    });
    // Enter walks the creation form: X, then Y, then submit.
    byId("newOriginX").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        byId("newOriginY").focus();
      }
    });
    byId("newOriginY").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        addSketch();
      }
    });
    byId("newSidesInput").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        addSketch();
      }
    });
    for (const id of ["newSectorStart", "newSectorSweep"]) {
      byId(id).addEventListener("keydown", (event) => {
        if (event.key === "Enter") {
          event.preventDefault();
          addSketch();
        }
      });
    }
    byId("originX").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        byId("originY").focus();
      }
    });
    byId("originY").addEventListener("keydown", (event) => {
      if (event.key === "Enter") {
        event.preventDefault();
        moveSketch();
      } else if (event.key === "Escape") {
        event.preventDefault();
        const sketch = activeSketch();
        if (sketch) byId("originY").value = String(sketch.origin[1]);
      }
    });
    byId("paletteInput").addEventListener("input", (event) => {
      app.palette.index = 0;
      renderPalette(event.target.value);
    });
    byId("paletteInput").addEventListener("keydown", (event) => {
      if (event.key === "ArrowDown") {
        event.preventDefault();
        movePalette(1);
      } else if (event.key === "ArrowUp") {
        event.preventDefault();
        movePalette(-1);
      } else if (event.key === "Enter") {
        event.preventDefault();
        const command = app.palette.items[app.palette.index];
        if (command) runCommand(command);
      } else if (event.key === "Escape") {
        event.preventDefault();
        closePalette();
      }
    });
    for (const closer of document.querySelectorAll("[data-close]")) {
      closer.addEventListener("click", () => {
        if (closer.dataset.close === "palette") closePalette();
        if (closer.dataset.close === "help") closeHelp();
        if (closer.dataset.close === "simHud") closeSimHud();
      });
    }
    document.addEventListener("keydown", (event) => {
      const target = event.target;
      const typing = target instanceof HTMLElement
        && (target.tagName === "INPUT" || target.tagName === "SELECT" || target.tagName === "TEXTAREA");
      if ((event.ctrlKey || event.metaKey) && event.key.toLowerCase() === "k") {
        event.preventDefault();
        if (app.palette.open) closePalette();
        else openPalette();
        return;
      }
      if (event.key === "F3") {
        event.preventDefault();
        drawState.osnap = !drawState.osnap;
        const btn = byId("osnapToggleBtn");
        if (btn) btn.setAttribute("aria-pressed", String(drawState.osnap));
        const badge = byId("statusOsnapIndicator");
        if (badge) {
          badge.textContent = `OSNAP: ${drawState.osnap ? "ON" : "OFF"}`;
          badge.classList.toggle("active", drawState.osnap);
        }
        toast(`Object Snap ${drawState.osnap ? "ON" : "OFF"}`);
        return;
      }
      if (event.key === "F10") {
        event.preventDefault();
        drawState.polar = !drawState.polar;
        const btn = byId("polarToggleBtn");
        if (btn) btn.setAttribute("aria-pressed", String(drawState.polar));
        const badge = byId("statusPolarIndicator");
        if (badge) {
          badge.textContent = `POLAR: ${drawState.polar ? "ON" : "OFF"}`;
          badge.classList.toggle("active", drawState.polar);
        }
        toast(`Polar Tracking ${drawState.polar ? "ON" : "OFF"}`);
        return;
      }
      if (event.key === "Escape") {
        if (app.palette.open) closePalette();
        if (app.helpOpen) closeHelp();
        closeSimHud();
        const cli = byId("commandPromptInput");
        if (cli) {
          cli.value = "";
          cli.blur();
          byId("commandSuggestions").hidden = true;
        }
        if (app.draw) cancelDrawing();
        closeDrawers();
        if (app.viewport && app.viewport.pick) clearPick();
        return;
      }
      if (event.key === "Enter" && app.draw && !typing) {
        // Let a focused button run its own action instead of closing the loop twice.
        if (!(target instanceof HTMLElement && target.tagName === "BUTTON")) {
          event.preventDefault();
          finishDrawing();
          return;
        }
      }
      if (app.palette.open) return;
      if (typing) return;
      if (!typing && event.key.length === 1 && !event.ctrlKey && !event.metaKey && !event.altKey) {
        const navKeys = ["1", "2", "3", "4", "f", "F", "w", "W", "g", "G", "?"];
        if (!navKeys.includes(event.key)) {
          const cli = byId("commandPromptInput");
          if (cli && document.activeElement !== cli) {
            cli.focus();
            cli.value = event.key;
            cli.dispatchEvent(new Event("input"));
            event.preventDefault();
            return;
          }
        }
      }
      if (event.ctrlKey || event.metaKey) {
        if (event.key.toLowerCase() === "s") {
          event.preventDefault();
          save();
        } else if (event.key.toLowerCase() === "z") {
          event.preventDefault();
          if (event.shiftKey) redo();
          else undo();
        } else if (event.key.toLowerCase() === "y") {
          event.preventDefault();
          redo();
        }
        return;
      }
      if (event.altKey) return;
      switch (event.key) {
        case "f":
        case "F":
          event.preventDefault();
          fit();
          break;
        case "w":
        case "W":
          event.preventDefault();
          toggleWireframe();
          break;
        case "g":
        case "G":
          event.preventDefault();
          toggleGrid();
          break;
        case "1":
          setView("front");
          break;
        case "2":
          setView("top");
          break;
        case "3":
          setView("right");
          break;
        case "4":
          setView("iso");
          break;
        case "?":
          event.preventDefault();
          openHelp();
          break;
        default:
          break;
      }
    });
  }

  async function boot() {
    let stored = null;
    try {
      stored = localStorage.getItem("atlas-theme");
    } catch (error) {
      stored = null;
    }
    const prefersLight = window.matchMedia && window.matchMedia("(prefers-color-scheme: light)").matches;
    applyTheme(stored || (prefersLight ? "light" : "dark"));
    bindUi();
    app.viewport = new Viewport(byId("viewport"));
    if (app.viewport.fallback) {
      log(app.viewport.fallback, "error");
    }
    setStatus("Loading project…", "busy");
    try {
      const state = await api("/api/state");
      applyState(state);
      setStatus("Ready", "");
      log(`Opened ${state.name} · ${state.geometry.triangles} triangles`);
    } catch (error) {
      const message = error instanceof Error ? error.message : String(error);
      setStatus(message, "error");
      log(message, "error");
      toast(message, true);
    }
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", boot);
  } else {
    boot();
  }
})();

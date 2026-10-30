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
        if (!this.pointer || this.pointer.id !== event.pointerId) return;
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
      const rect = this.canvas.getBoundingClientRect();
      if (rect.width < 1 || rect.height < 1) return null;
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
      if (Math.abs(direction[2]) < 1e-9) return null;
      const t = -eye[2] / direction[2];
      if (!Number.isFinite(t) || t <= 1e-6) return null;
      return [eye[0] + direction[0] * t, eye[1] + direction[1] * t];
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
    // no geometry behind the button. The truck adapter advertises it: a cut whose tool overshoots
    // the body is exact (verified analytically and through this API).
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
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `${project.name.replace(/[^A-Za-z0-9_-]+/g, "_") || "part"}.stl`;
      document.body.append(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      log("Exported binary STL");
      toast("STL download started.");
    } catch (error) {
      toast(error.message, true);
      log(`Export failed: ${error.message}`, "error");
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
  // Command palette
  // ---------------------------------------------------------------------------------------

  function commandList() {
    const project = app.project;
    const commands = [
      { label: "Save project", hint: "Ctrl S", keywords: "save write", run: () => save() },
      { label: "Undo", hint: "Ctrl Z", keywords: "history back", run: () => undo(), disabled: !project || project.history.undo === 0 },
      { label: "Redo", hint: "Ctrl Y", keywords: "history forward", run: () => redo(), disabled: !project || project.history.redo === 0 },
      { label: "Export binary STL", hint: "download", keywords: "export mesh stl", run: () => exportStl(), disabled: !project || project.features.length === 0 },
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
    byId("paletteBtn").addEventListener("click", openPalette);
    byId("themeBtn").addEventListener("click", toggleTheme);
    byId("fitBtn").addEventListener("click", fit);
    byId("wireBtn").addEventListener("click", toggleWireframe);
    byId("gridBtn").addEventListener("click", toggleGrid);
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
      if (event.key === "Escape") {
        if (app.palette.open) closePalette();
        if (app.helpOpen) closeHelp();
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

// Frame-time sampling against a running studio with a model already loaded.
//
//   node scripts/frame-sample.mjs http://127.0.0.1:7791 [--seconds 5] [--json]
//
// The page is opened in headless Chrome, waited on until the feature tree and the WebGL canvas
// are live, and then sampled with requestAnimationFrame for the requested window. Headless Chrome
// without a GPU renders through SwiftShader, so the numbers describe this software renderer, not
// a workstation GPU; they are reported with the renderer string so the caveat travels with them.
// Exits 0 when frames were sampled, SKIP (exit 0) when Chrome is absent, 1 on failure.
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';

const TARGET = process.argv[2];
if (!TARGET) {
  console.error('usage: node scripts/frame-sample.mjs <url> [--seconds 5] [--json]');
  process.exit(2);
}
const asJson = process.argv.includes('--json');
const secondsIndex = process.argv.indexOf('--seconds');
const seconds = secondsIndex > 0 ? Number(process.argv[secondsIndex + 1]) : 5;
if (!Number.isFinite(seconds) || seconds <= 0 || seconds > 60) {
  console.error('--seconds must be between 0 and 60');
  process.exit(2);
}

const CHROME_CANDIDATES = [
  '/opt/google/chrome/chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
];
const CHROME = CHROME_CANDIDATES.find((path) => existsSync(path));
if (!CHROME) {
  console.log('SKIP: no Chrome found; frame time was not measured');
  process.exit(0);
}

const PORT = 9394 + (process.pid % 200);
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const SAMPLE = `(async () => {
  const duration = ${Math.round(seconds * 1000)};
  const canvas = document.querySelector('canvas');
  const renderer = (() => {
    try {
      const gl = canvas && (canvas.getContext('webgl2') || canvas.getContext('webgl'));
      if (!gl) return 'no webgl context';
      const ext = gl.getExtension('WEBGL_debug_renderer_info');
      return ext ? gl.getParameter(ext.UNMASKED_RENDERER_WEBGL) : gl.getParameter(gl.RENDERER);
    } catch (error) {
      return 'unavailable: ' + error.message;
    }
  })();
  const deltas = [];
  let last = performance.now();
  const started = last;
  await new Promise((resolve) => {
    const tick = (now) => {
      deltas.push(now - last);
      last = now;
      if (now - started >= duration) resolve();
      else requestAnimationFrame(tick);
    };
    requestAnimationFrame(tick);
  });
  deltas.shift();
  const sorted = [...deltas].sort((a, b) => a - b);
  const at = (p) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * p))] || 0;
  const elapsed = last - started;
  const slow = deltas.filter((ms) => ms > 20).length;
  return {
    frames: deltas.length,
    elapsed_ms: Math.round(elapsed),
    fps: Math.round((deltas.length / elapsed) * 10000) / 10,
    p50_ms: Math.round(at(0.5) * 100) / 100,
    p95_ms: Math.round(at(0.95) * 100) / 100,
    max_ms: Math.round((sorted[sorted.length - 1] || 0) * 100) / 100,
    slow_frames_over_20ms: slow,
    renderer,
    canvas_triangles: canvas ? Number(canvas.dataset.triangles || 0) : 0,
  };
})()`;

const browser = spawn(
  CHROME,
  [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=/tmp/atlas-frames-${process.pid}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-gpu',
    '--enable-unsafe-swiftshader',
    '--window-size=1440,900',
    TARGET,
  ],
  { stdio: 'ignore' }
);

let socket;
try {
  let page = null;
  for (let attempt = 0; attempt < 80 && !page; attempt += 1) {
    try {
      const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
      page = targets.find((target) => target.type === 'page' && target.url.startsWith('http'));
    } catch {
      // still starting
    }
    if (!page) await sleep(250);
  }
  if (!page) throw new Error('no page target');

  socket = new WebSocket(page.webSocketDebuggerUrl);
  await new Promise((resolve, reject) => {
    socket.addEventListener('open', resolve);
    socket.addEventListener('error', reject);
  });

  let sequence = 0;
  const pending = new Map();
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(JSON.stringify(message.error)));
      else resolve(message.result);
    }
  });
  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      sequence += 1;
      pending.set(sequence, { resolve, reject });
      socket.send(JSON.stringify({ id: sequence, method, params }));
    });

  await send('Runtime.enable');
  for (let attempt = 0; attempt < 150; attempt += 1) {
    const ready = await send('Runtime.evaluate', {
      expression:
        'document.querySelectorAll("#tree .tree-item").length > 0 && !!document.querySelector("canvas")',
      returnByValue: true,
    });
    if (ready.result.value) break;
    await sleep(100);
  }
  // Let the first frame and the orbit camera settle before sampling.
  await sleep(1000);

  const result = await send('Runtime.evaluate', {
    expression: SAMPLE,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  const report = result.result.value;
  if (!report || report.frames < 2) throw new Error('no animation frames were sampled');

  if (asJson) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`frames: ${report.frames} over ${report.elapsed_ms} ms`);
    console.log(`fps: ${report.fps}`);
    console.log(`frame time p50/p95/max: ${report.p50_ms} / ${report.p95_ms} / ${report.max_ms} ms`);
    console.log(`slow frames (>20 ms): ${report.slow_frames_over_20ms}`);
    console.log(`triangles on canvas: ${report.canvas_triangles}`);
    console.log(`renderer: ${report.renderer} (headless software rendering)`);
  }
} catch (error) {
  console.error(`FAIL: ${error.message}`);
  process.exitCode = 1;
} finally {
  if (socket) socket.close();
  browser.kill();
}

// Advanced interaction driver for a running Atlas CAD Studio page.
//
// Where browser-check.mjs asserts that the page loads and one workflow completes, this drives the
// page the way a person does: real CDP mouse presses/moves/releases (orbit, pan, wheel zoom), real
// key presses (shortcuts, the command palette, typing into fields), clicks on every panel control,
// and downloads that are parsed rather than merely detected. It works against a loopback studio or
// the hosted stack through Caddy (pass --insecure for a local-CA certificate).
//
//   node scripts/browser-drive.mjs "http://127.0.0.1:7519/?token=..." --artifacts dist/browser-drive
//   node scripts/browser-drive.mjs "https://localhost:8443/?token=..." --insecure
//
// Exit status: 0 when every assertion holds, 1 on a failed assertion, 2 on a usage error.
import { spawn } from 'node:child_process';
import { createHash } from 'node:crypto';
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  statSync,
  writeFileSync,
} from 'node:fs';

const CHROME_CANDIDATES = [
  '/opt/google/chrome/chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
];

const args = process.argv.slice(2);
let target = null;
let artifacts = 'dist/browser-drive';
let insecure = false;
for (let index = 0; index < args.length; index += 1) {
  const value = args[index];
  if (value === '--artifacts') artifacts = args[++index];
  else if (value === '--insecure') insecure = true;
  else if (value === '-h' || value === '--help') {
    console.log('usage: node scripts/browser-drive.mjs <url> [--artifacts DIR] [--insecure]');
    process.exit(0);
  } else if (!target) target = value;
  else {
    console.error(`browser-drive: unknown argument ${value}`);
    process.exit(2);
  }
}
if (!target) {
  console.error('usage: node scripts/browser-drive.mjs <url> [--artifacts DIR] [--insecure]');
  process.exit(2);
}
if (!/^https?:\/\//.test(target)) {
  console.error('browser-drive: the target must be an http(s) URL');
  process.exit(2);
}

const CHROME = CHROME_CANDIDATES.find((path) => existsSync(path));
if (!CHROME) {
  console.log('SKIP: no Chrome found; the HTTP and full-check suites still cover the API');
  process.exit(0);
}

const PORT = 9392;
const PROFILE = `/tmp/atlas-browser-drive-${process.pid}`;
const DOWNLOADS = `/tmp/atlas-browser-drive-downloads-${process.pid}`;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let passed = 0;
const steps = [];
function check(condition, message) {
  if (!condition) throw new Error(message);
  passed += 1;
}
function step(name) {
  steps.push(name);
  console.log(`    ${name}`);
}

const chromeArgs = [
  '--headless=new',
  `--remote-debugging-port=${PORT}`,
  `--user-data-dir=${PROFILE}`,
  '--no-first-run',
  '--no-default-browser-check',
  '--disable-gpu',
  '--enable-unsafe-swiftshader',
  '--hide-scrollbars',
  '--window-size=1440,900',
  '--allow-insecure-localhost',
];
if (insecure || target.startsWith('https://')) chromeArgs.push('--ignore-certificate-errors');
// Start on about:blank and navigate once: launching at the target and then navigating again aborts
// the first load, which shows up as a spurious net::ERR_ABORTED in the request audit.
chromeArgs.push('about:blank');

const browser = spawn(CHROME, chromeArgs, { stdio: 'ignore' });

let socket;
let diagnose = null;
try {
  let page = null;
  for (let attempt = 0; attempt < 80 && !page; attempt += 1) {
    try {
      const response = await fetch(`http://127.0.0.1:${PORT}/json/list`);
      const targets = await response.json();
      page = targets.find((entry) => entry.type === 'page');
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
  const pageErrors = [];
  const failedRequests = [];
  const requestUrls = new Map();
  socket.addEventListener('message', (event) => {
    const message = JSON.parse(event.data);
    if (message.id && pending.has(message.id)) {
      const { resolve, reject } = pending.get(message.id);
      pending.delete(message.id);
      if (message.error) reject(new Error(JSON.stringify(message.error)));
      else resolve(message.result);
      return;
    }
    if (message.method === 'Runtime.exceptionThrown') {
      const details = message.params.exceptionDetails;
      const desc = details.exception ? (details.exception.description || details.exception.value || '') : '';
      pageErrors.push(`exception: ${details.text}${desc ? ` - ${desc}` : ''}`);
    }
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      pageErrors.push(
        `console.error: ${message.params.args.map((arg) => arg.value ?? arg.description ?? '').join(' ')}`
      );
    }
    if (message.method === 'Log.entryAdded' && message.params.entry.level === 'error') {
      pageErrors.push(`log: ${message.params.entry.text}`);
    }
    if (message.method === 'Network.requestWillBeSent') {
      requestUrls.set(message.params.requestId, message.params.request.url);
    }
    if (message.method === 'Network.responseReceived') {
      const { status, url } = message.params.response;
      if (status >= 400) failedRequests.push(`${status} ${url}`);
    }
    if (message.method === 'Network.loadingFailed') {
      const url = requestUrls.get(message.params.requestId) ?? '';
      failedRequests.push(`${message.params.errorText} ${url}`.trim());
    }
  });

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      sequence += 1;
      pending.set(sequence, { resolve, reject });
      socket.send(JSON.stringify({ id: sequence, method, params }));
    });

  await send('Page.enable');
  await send('Runtime.enable');
  await send('Log.enable');
  await send('Network.enable');
  await send('Emulation.setDeviceMetricsOverride', {
    width: 1440,
    height: 900,
    deviceScaleFactor: 1,
    mobile: false,
  });
  mkdirSync(DOWNLOADS, { recursive: true });
  mkdirSync(artifacts, { recursive: true });
  await send('Page.setDownloadBehavior', { behavior: 'allow', downloadPath: DOWNLOADS });

  const evaluate = async (expression) => {
    const result = await send('Runtime.evaluate', {
      expression,
      awaitPromise: true,
      returnByValue: true,
    });
    if (result.exceptionDetails) {
      throw new Error(
        `page exception: ${result.exceptionDetails.text} ${
          result.exceptionDetails.exception?.description ?? ''
        }`
      );
    }
    return result.result.value;
  };

  const waitFor = async (expression, label, timeout = 30000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await evaluate(expression)) return;
      await sleep(75);
    }
    throw new Error(`timed out waiting for ${label}`);
  };

  const idle = () => waitFor('!document.getElementById("saveBtn").disabled', 'the app to be idle');
  const hash = (data) => createHash('sha256').update(data).digest('hex').slice(0, 16);

  let shotNumber = 0;
  const screenshot = async (name) => {
    const { data } = await send('Page.captureScreenshot', { format: 'png' });
    const buffer = Buffer.from(data, 'base64');
    shotNumber += 1;
    const file = `${artifacts}/${String(shotNumber).padStart(2, '0')}-${name}.png`;
    writeFileSync(file, buffer);
    return { hash: hash(buffer), bytes: buffer.length, file };
  };

  // --- real input -----------------------------------------------------------------------------
  const buttonMask = { left: 1, right: 2, middle: 4 };
  const mouse = (type, x, y, options = {}) =>
    send('Input.dispatchMouseEvent', {
      type,
      x,
      y,
      button: options.button ?? 'left',
      buttons: options.buttons ?? 0,
      clickCount: options.clickCount ?? (type === 'mousePressed' || type === 'mouseReleased' ? 1 : 0),
      modifiers: options.modifiers ?? 0,
      deltaX: options.deltaX ?? 0,
      deltaY: options.deltaY ?? 0,
    });

  const rectOf = (selector) =>
    evaluate(`(() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return null;
      el.scrollIntoView({ block: "center", inline: "center" });
      const r = el.getBoundingClientRect();
      return { x: r.left + r.width / 2, y: r.top + r.height / 2, w: r.width, h: r.height };
    })()`);

  const clickSelector = async (selector, label = selector) => {
    const rect = await rectOf(selector);
    check(rect && rect.w > 0 && rect.h > 0, `the control ${label} is not visible`);
    await mouse('mouseMoved', rect.x, rect.y);
    await mouse('mousePressed', rect.x, rect.y, { buttons: buttonMask.left });
    await mouse('mouseReleased', rect.x, rect.y);
    await sleep(40);
  };

  // The plot options are a collapsed <details>; a user opens it before touching a field inside.
  const openSheetOptions = async () => {
    if (!(await evaluate('document.getElementById("sheetOptions").open'))) {
      await clickSelector('#sheetOptions summary', 'the Plot sheet summary');
    }
    await waitFor('document.getElementById("sheetOptions").open', 'the plot sheet options');
  };

  const canvasRect = async () => {
    const rect = await evaluate(`(() => {
      const r = document.getElementById("viewport").getBoundingClientRect();
      return { left: r.left, top: r.top, width: r.width, height: r.height };
    })()`);
    check(rect.width > 300 && rect.height > 200, `the viewport is ${rect.width}x${rect.height}`);
    return rect;
  };

  const drag = async (from, to, { shift = false, steps: count = 12 } = {}) => {
    const modifiers = shift ? 8 : 0;
    await mouse('mouseMoved', from.x, from.y, { modifiers });
    await mouse('mousePressed', from.x, from.y, { buttons: buttonMask.left, modifiers });
    for (let index = 1; index <= count; index += 1) {
      const x = from.x + ((to.x - from.x) * index) / count;
      const y = from.y + ((to.y - from.y) * index) / count;
      await mouse('mouseMoved', x, y, { buttons: buttonMask.left, modifiers });
    }
    await mouse('mouseReleased', to.x, to.y, { modifiers });
    await sleep(120);
  };

  const wheel = async (point, deltaY) => {
    await mouse('mouseMoved', point.x, point.y);
    await send('Input.dispatchMouseEvent', {
      type: 'mouseWheel',
      x: point.x,
      y: point.y,
      deltaX: 0,
      deltaY,
    });
    await sleep(120);
  };

  const KEYS = {
    Enter: { code: 'Enter', vk: 13 },
    Escape: { code: 'Escape', vk: 27 },
    Tab: { code: 'Tab', vk: 9 },
    ArrowDown: { code: 'ArrowDown', vk: 40 },
    ArrowUp: { code: 'ArrowUp', vk: 38 },
  };
  const keyEvent = (type, key, options = {}) => {
    const meta = KEYS[key] ?? { code: `Key${key.toUpperCase()}`, vk: key.toUpperCase().charCodeAt(0) };
    return send('Input.dispatchKeyEvent', {
      type,
      key,
      code: meta.code,
      windowsVirtualKeyCode: meta.vk,
      nativeVirtualKeyCode: meta.vk,
      modifiers: options.modifiers ?? 0,
      text: options.text,
      unmodifiedText: options.text,
    });
  };
  const pressKey = async (key, options = {}) => {
    await keyEvent(options.text ? 'keyDown' : 'rawKeyDown', key, options);
    await keyEvent('keyUp', key, options);
    await sleep(options.wait ?? 60);
  };
  const chord = async (key, modifiers) => pressKey(key, { modifiers, wait: 200 });
  // Global shortcuts are ignored while an input has focus (that is how typing "w" into a field
  // works), so a real user blurs first. Tab after typing leaves the next field focused.
  const blur = () =>
    evaluate(
      'document.activeElement instanceof HTMLElement ? (document.activeElement.blur(), true) : true'
    );
  const typeText = async (text) => {
    await send('Input.insertText', { text });
    await sleep(40);
  };
  const typeInto = async (selector, text) => {
    const rect = await rectOf(selector);
    if (rect && rect.w > 0 && rect.h > 0) {
      await mouse('mouseMoved', rect.x, rect.y);
      await mouse('mousePressed', rect.x, rect.y, { buttons: buttonMask.left });
      await mouse('mouseReleased', rect.x, rect.y);
    }
    // Clicking is what a user does, but a panel that scrolls can put the click on a neighbour. Pin
    // the focus and the selection to the intended field and verify the result, rather than trusting
    // that the click landed and that Ctrl+A selected the old value.
    const focused = await evaluate(`(() => {
      const el = document.querySelector(${JSON.stringify(selector)});
      if (!el) return false;
      el.focus();
      if (typeof el.select === "function") el.select();
      return document.activeElement === el;
    })()`);
    check(focused, `the field ${selector} could not take focus`);
    await typeText(text);
    const actual = await evaluate(
      `document.querySelector(${JSON.stringify(selector)}).value`
    );
    check(actual === text, `the field ${selector} holds ${JSON.stringify(actual)}, expected ${JSON.stringify(text)}`);
    await pressKey('Tab');
    await sleep(80);
  };

  const selectValue = async (selector, value) => {
    await evaluate(`(() => {
      const select = document.querySelector(${JSON.stringify(selector)});
      select.value = ${JSON.stringify(value)};
      select.dispatchEvent(new Event("change", { bubbles: true }));
      return select.value;
    })()`);
    await sleep(60);
  };

  const volumeMm3 = async () => {
    const text = await evaluate('document.getElementById("statusGeometry").textContent');
    // The status bar groups thousands (12,000 mm³), so take the whole number and drop the commas.
    const match = text.match(/([0-9][0-9,]*(?:\.[0-9]+)?)\s*mm/);
    check(Boolean(match), `the status bar has no volume: ${text}`);
    return Number(match[1].replace(/,/g, ""));
  };

  const near = (actual, expected, tolerance = 0.01) =>
    Math.abs(actual - expected) <= Math.max(Math.abs(expected) * tolerance, 0.5);

  const waitForVolume = async (expected, tolerance = 0.01) => {
    const deadline = Date.now() + 20000;
    let last = null;
    while (Date.now() < deadline) {
      last = await volumeMm3();
      if (near(last, expected, tolerance)) return last;
      await sleep(100);
    }
    return last;
  };

  const downloadNames = () => {
    try {
      return readdirSync(DOWNLOADS).filter((name) => !name.endsWith('.crdownload'));
    } catch {
      return [];
    }
  };
  const waitForDownload = async (predicate, label, timeout = 20000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      const names = downloadNames();
      if (names.some((name) => predicate(name, `${DOWNLOADS}/${name}`))) return names;
      await sleep(100);
    }
    throw new Error(`timed out waiting for ${label}`);
  };

  diagnose = async () => {
    try {
      const model = await evaluate(`({
        tree: Array.from(document.querySelectorAll("#tree .tree-label"), (n) => n.textContent),
        selected: document.querySelector("#tree .tree-item.selected .tree-label")?.textContent ?? null,
        parameters: Array.from(document.querySelectorAll("#parameters input"), (i) => ({ id: i.id, value: i.value })),
        status: document.getElementById("statusGeometry").textContent,
        properties: document.getElementById("properties").textContent.replace(/\\s+/g, " ").slice(0, 300),
        failure: document.getElementById("failureMessage").textContent,
      })`);
      console.error(`    model:     ${JSON.stringify(model)}`);
    } catch {
      // ignore
    }
    let state = { url: '?', title: '?', body: '?', readyState: '?' };
    try {
      state = await evaluate(`({
        url: location.href,
        title: document.title,
        readyState: document.readyState,
        body: (document.body ? document.body.innerText : "").slice(0, 400),
      })`);
    } catch {
      // the page may not be scriptable at all
    }
    try {
      const { data } = await send('Page.captureScreenshot', { format: 'png' });
      writeFileSync(`${artifacts}/failure.png`, Buffer.from(data, 'base64'));
    } catch {
      // ignore
    }
    console.error(`    page:      ${state.url} (${state.readyState})`);
    console.error(`    title:     ${state.title}`);
    console.error(`    body:      ${state.body.replace(/\s+/g, ' ').slice(0, 300)}`);
    console.error(`    errors:    ${pageErrors.join(' | ') || 'none'}`);
    console.error(`    requests:  ${failedRequests.join(' | ') || 'none'}`);
  };

  // --- load -----------------------------------------------------------------------------------
  const navigation = await send('Page.navigate', { url: target });
  if (navigation.errorText) {
    throw new Error(`navigation failed: ${navigation.errorText}`);
  }
  await waitFor('document.querySelectorAll("#tree .tree-item").length >= 2', 'the tree');
  await idle();
  const initial = await screenshot('loaded');

  step('page shell, project identity and a live kernel body');
  const title = await evaluate('document.title');
  check(title.includes('Atlas'), `the title is ${title}`);
  const identity = await evaluate(`({
    file: document.getElementById("fileName").textContent,
    units: document.getElementById("unitLabel").textContent,
    revision: document.getElementById("revisionLabel").textContent,
    history: document.getElementById("historyLabel").textContent,
    badge: document.getElementById("dirtyBadge").textContent.trim(),
    webgl: document.getElementById("viewport").dataset.webgl,
    triangles: Number(document.getElementById("viewport").dataset.triangles || 0),
  })`);
  check(identity.file.endsWith('.zcad'), `the file is ${identity.file}`);
  check(/\bmm\b/.test(identity.units), `the units are ${identity.units}`);
  check(identity.revision !== '—' && identity.history !== '—', 'revision and history are shown');
  // A document can arrive clean (saved) or with unsaved edits (for example a previous driver in the
  // same session); the end of this run proves Ctrl+S returns it to saved either way.
  check(['saved', 'unsaved'].includes(identity.badge), `the badge is ${identity.badge}`);
  check(identity.webgl === 'ready', `WebGL is ${identity.webgl}`);
  check(identity.triangles > 0, 'the viewport has no triangles');
  check(initial.bytes > 5000, `the rendered page is only ${initial.bytes} bytes (blank?)`);

  const labels = await evaluate(
    'Array.from(document.querySelectorAll("#tree .tree-label"), (row) => row.textContent.trim())'
  );
  check(labels.length >= 2, `the tree is ${labels.join('|')}`);
  const startVolume = await volumeMm3();
  check(startVolume > 0, `the starting volume is ${startVolume}`);

  // --- viewport: real pointer drags, wheel, picking -------------------------------------------
  step('orbit drag (mouse press → move → release) changes the rendered frame');
  const canvas = await canvasRect();
  const centre = { x: canvas.left + canvas.width / 2, y: canvas.top + canvas.height / 2 };
  const beforeOrbit = await screenshot('before-orbit');
  await drag(centre, { x: centre.x + 170, y: centre.y + 90 });
  const afterOrbit = await screenshot('after-orbit');
  check(afterOrbit.hash !== beforeOrbit.hash, 'the orbit drag did not redraw the viewport');

  step('shift-drag pans, and the pan is a different frame from the orbit');
  const beforePan = await screenshot('before-pan');
  await drag(centre, { x: centre.x - 150, y: centre.y + 100 }, { shift: true });
  const afterPan = await screenshot('after-pan');
  check(afterPan.hash !== beforePan.hash, 'the shift-drag did not pan the viewport');

  step('wheel zoom changes the frame, and Fit restores it');
  const beforeZoom = await screenshot('before-zoom');
  await wheel(centre, -420);
  const zoomedIn = await screenshot('zoomed-in');
  check(zoomedIn.hash !== beforeZoom.hash, 'the wheel did not zoom the viewport');
  await wheel(centre, 900);
  const zoomedOut = await screenshot('zoomed-out');
  check(zoomedOut.hash !== zoomedIn.hash, 'the second wheel step did not zoom');
  await clickSelector('#fitBtn', 'Fit');
  const fitted = await screenshot('fitted');
  check(fitted.hash !== zoomedOut.hash, 'Fit did not reframe the model');

  step('click a face and read its position and normal');
  await mouse('mouseMoved', centre.x, centre.y);
  await mouse('mousePressed', centre.x, centre.y, { buttons: buttonMask.left });
  await mouse('mouseReleased', centre.x, centre.y);
  await waitFor('!document.getElementById("pickCard").hidden', 'the face pick card');
  const pick = await evaluate(`({
    x: document.getElementById("pickX").textContent,
    y: document.getElementById("pickY").textContent,
    z: document.getElementById("pickZ").textContent,
    normal: document.getElementById("pickNormal").textContent,
    status: document.getElementById("statusPick").textContent,
  })`);
  check(/[0-9]/.test(pick.x) && /mm$/.test(pick.x), `the picked X is ${pick.x}`);
  check(/[0-9]/.test(pick.normal), `the picked normal is ${pick.normal}`);
  check(pick.status.startsWith('X '), `the status pick is ${pick.status}`);
  await screenshot('face-picked');
  await clickSelector('#pickClose', 'dismiss');

  step('view buttons and keyboard shortcuts (1–4, W, G, F)');
  const iso = await screenshot('view-iso');
  await clickSelector('[data-view="front"]', 'Front');
  const front = await screenshot('view-front');
  check(front.hash !== iso.hash, 'the Front button did not change the view');
  await pressKey('2', { text: '2' });
  const top = await screenshot('view-top');
  check(top.hash !== front.hash, 'the 2 shortcut did not switch to the top view');
  await pressKey('3', { text: '3' });
  const right = await screenshot('view-right');
  check(right.hash !== top.hash, 'the 3 shortcut did not switch to the right view');
  await pressKey('4', { text: '4' });

  const wireBefore = await evaluate('document.getElementById("wireBtn").getAttribute("aria-pressed")');
  await pressKey('w', { text: 'w' });
  const wireAfter = await evaluate('document.getElementById("wireBtn").getAttribute("aria-pressed")');
  check(wireBefore !== wireAfter, `W did not toggle wireframe (${wireBefore} → ${wireAfter})`);
  await pressKey('w', { text: 'w' });

  const gridBefore = await evaluate('document.getElementById("gridBtn").getAttribute("aria-pressed")');
  await pressKey('g', { text: 'g' });
  const gridAfter = await evaluate('document.getElementById("gridBtn").getAttribute("aria-pressed")');
  check(gridBefore !== gridAfter, `G did not toggle the grid (${gridBefore} → ${gridAfter})`);
  await pressKey('g', { text: 'g' });

  // Orientation Gizmo
  await clickSelector('#axisGizmo button[data-axis="top"]', 'Gizmo Top view');
  await clickSelector('#axisGizmo button[data-axis="iso"]', 'Gizmo Iso view');

  // Status bar clickable badges
  await clickSelector('#statusWireIndicator', 'Status Wire Badge');
  check((await evaluate('document.getElementById("statusWireIndicator").textContent')) === 'WIRE: ON', 'status wire badge is on');
  await clickSelector('#statusWireIndicator', 'Status Wire Badge back');
  check((await evaluate('document.getElementById("statusWireIndicator").textContent')) === 'WIRE: OFF', 'status wire badge is off');

  step('the tree selects a feature and the parameters belong to it');
  await clickSelector('#tree .tree-item:nth-child(2)', 'the extrude row');
  const selected = await evaluate(`({
    selected: document.querySelectorAll("#tree .tree-item.selected").length,
    properties: document.getElementById("properties").textContent,
    depth: document.getElementById("param-depth") ? document.getElementById("param-depth").value : null,
  })`);
  check(selected.selected === 1, 'the clicked tree row is not marked selected');
  check(selected.properties.includes('Extrude'), `the properties are ${selected.properties.slice(0, 120)}`);
  check(selected.depth !== null, 'the extrude has no depth parameter input');

  // --- model mutations through the page -------------------------------------------------------
  const dims = await evaluate(`({
    width: Number(document.getElementById("param-width")?.value ?? 0),
    height: Number(document.getElementById("param-height")?.value ?? 0),
    depth: Number(document.getElementById("param-depth")?.value ?? 0),
  })`);
  check(dims.width > 0 && dims.height > 0 && dims.depth > 0, `the rectangle size is ${JSON.stringify(dims)}`);
  const analyticBase = dims.width * dims.height * dims.depth;
  check(near(startVolume, analyticBase, 0.002), `the volume ${startVolume} is not w*h*d = ${analyticBase}`);

  step('typing a new depth rebuilds the body to the analytic volume');
  const newDepth = dims.depth + 2;
  await typeInto('#param-depth', String(newDepth));
  const rebuilt = await waitForVolume(dims.width * dims.height * newDepth, 0.002);
  check(
    near(rebuilt, dims.width * dims.height * newDepth, 0.002),
    `depth ${newDepth} produced ${rebuilt} mm³, expected ${dims.width * dims.height * newDepth}`
  );
  await screenshot('depth-rebuilt');

  step('Ctrl+Z undoes and Ctrl+Shift+Z redoes the rebuild');
  await blur();
  await pressKey('z', { modifiers: 2, wait: 250 });
  const undone = await waitForVolume(startVolume, 0.002);
  check(near(undone, startVolume, 0.002), `undo produced ${undone}, expected ${startVolume}`);
  await blur();
  await pressKey('z', { modifiers: 2 | 8, wait: 250 });
  const redone = await waitForVolume(dims.width * dims.height * newDepth, 0.002);
  check(near(redone, dims.width * dims.height * newDepth), `redo produced ${redone}`);

  step('create a circle sketch on the top plane and cut a through-hole');
  const topPlane = newDepth;
  await selectValue('#newSketchKind', 'circle');
  await typeInto('#newOriginX', String(dims.width / 2));
  await typeInto('#newOriginY', String(dims.height / 2));
  await typeInto('#newOriginZ', String(topPlane));
  await clickSelector('#addSketchBtn', 'Add sketch');
  await waitFor(
    `document.querySelectorAll("#tree .tree-item").length === ${labels.length + 1}`,
    'the circle sketch row'
  );
  const circleRow = `#tree .tree-item:nth-child(${labels.length + 1})`;
  await clickSelector(circleRow, 'the circle sketch');
  const diameter = 4;
  await typeInto('#param-diameter', String(diameter));
  await waitFor('!document.getElementById("saveBtn").disabled', 'the diameter edit');
  // The rows are buttons without a data id; the newest sketch is the last option the create form
  // offers, which is the one that was just added.
  const circleSketchId = await evaluate(
    '(() => { const s = document.getElementById("newFeatureSketch"); return s.options[s.options.length - 1].value; })()'
  );
  check(Boolean(circleSketchId), 'the create form does not offer the new sketch');
  await selectValue('#newFeatureSketch', circleSketchId);
  await selectValue('#newFeatureKind', 'extrude');
  await selectValue('#newFeatureOperation', 'cut');
  await selectValue('#newFeatureDirection', 'down');
  await selectValue('#newFeatureDepth', 'depth');
  await clickSelector('#addFeatureBtn', 'Add cut');
  const hole = Math.PI * (diameter / 2) ** 2 * newDepth;
  const expectedCut = dims.width * dims.height * newDepth - hole;
  const cutVolume = await waitForVolume(expectedCut, 0.02);
  check(near(cutVolume, expectedCut, 0.02), `the cut produced ${cutVolume}, expected ~${expectedCut.toFixed(1)}`);
  const cutRow = await evaluate(`({
    rows: document.querySelectorAll("#tree .tree-item").length,
    badges: Array.from(document.querySelectorAll("#tree .tree-badge"), (n) => n.textContent.trim()),
    failure: document.getElementById("failureBanner").hidden,
  })`);
  check(cutRow.rows === labels.length + 2, `the tree has ${cutRow.rows} rows after the cut`);
  check(cutRow.failure, 'the cut left the failure banner visible');
  await screenshot('hole-cut');

  // --- drawing editor: real clicks in the sheet ------------------------------------------------
  step('draw with every tool (line, circle, poly, point, dim, text) by clicking the sheet');
  const drawingClick = async (fx, fy) => {
    const box = await evaluate(`(() => {
      const svg = document.querySelector("#drawingPreview svg");
      if (!svg) return null;
      const r = svg.getBoundingClientRect();
      return { x: r.left + r.width * ${fx}, y: r.top + r.height * ${fy} };
    })()`);
    check(box, 'the drawing sheet is not rendered');
    await mouse('mouseMoved', box.x, box.y);
    await mouse('mousePressed', box.x, box.y, { buttons: buttonMask.left });
    await mouse('mouseReleased', box.x, box.y);
    await sleep(120);
  };
  const entities = () => evaluate('Number(document.getElementById("drawingEntities").textContent)');
  const entitiesBefore = await entities();
  check(entitiesBefore >= 1, `the drawing starts with ${entitiesBefore} entities`);

  await clickSelector('.draw-tools [data-tool="line"]', 'Line tool');
  await drawingClick(0.28, 0.4);
  await drawingClick(0.68, 0.4);
  await waitFor(`document.getElementById("drawingEntities").textContent === "${entitiesBefore + 1}"`, 'the drawn line');

  await clickSelector('.draw-tools [data-tool="circle"]', 'Circle tool');
  await drawingClick(0.5, 0.62);
  await drawingClick(0.56, 0.62);
  await waitFor(`document.getElementById("drawingEntities").textContent === "${entitiesBefore + 2}"`, 'the drawn circle');

  await clickSelector('.draw-tools [data-tool="polyline"]', 'Poly tool');
  await drawingClick(0.2, 0.75);
  await drawingClick(0.32, 0.82);
  await drawingClick(0.44, 0.75);
  await waitFor('!document.getElementById("polylineActions").hidden', 'the polyline actions');
  await clickSelector('#polylineFinishBtn', 'Finish polyline');
  await waitFor(
    `Number(document.getElementById("drawingEntities").textContent) >= ${entitiesBefore + 3}`,
    'the drawn polyline'
  );

  await clickSelector('.draw-tools [data-tool="point"]', 'Point tool');
  await drawingClick(0.72, 0.7);
  await waitFor(`document.getElementById("drawingEntities").textContent === "${entitiesBefore + 4}"`, 'the drawn point');

  await clickSelector('.draw-tools [data-tool="dimension"]', 'Dim tool');
  await drawingClick(0.3, 0.55);
  await drawingClick(0.6, 0.55);
  await waitFor(`document.getElementById("drawingEntities").textContent === "${entitiesBefore + 5}"`, 'the dimension');

  await typeInto('#drawText', 'PLATE');
  await clickSelector('.draw-tools [data-tool="text"]', 'Text tool');
  await drawingClick(0.55, 0.25);
  await waitFor(`document.getElementById("drawingEntities").textContent === "${entitiesBefore + 6}"`, 'the text');

  const afterTools = await evaluate(`({
    entities: Number(document.getElementById("drawingEntities").textContent),
    selection: document.getElementById("drawingSelection").textContent,
    length: document.getElementById("drawingLength").textContent,
    source: document.getElementById("drawingSource").textContent,
  })`);
  check(afterTools.entities === entitiesBefore + 6, `the drawing has ${afterTools.entities} entities`);
  check(/mm$/.test(afterTools.length), `the drawing length is ${afterTools.length}`);
  check(afterTools.source === 'model (live)', `the drawing source is ${afterTools.source}`);
  await screenshot('drawing-tools');

  // Test OSNAP and POLAR toggle controls
  await clickSelector('#osnapToggleBtn', 'OSNAP toggle');
  const osnapOff = await evaluate('document.getElementById("osnapToggleBtn").getAttribute("aria-pressed")');
  check(osnapOff === "false", `osnap did not toggle off: ${osnapOff}`);
  await clickSelector('#osnapToggleBtn', 'OSNAP toggle back on');
  const osnapOn = await evaluate('document.getElementById("osnapToggleBtn").getAttribute("aria-pressed")');
  check(osnapOn === "true", `osnap did not toggle on: ${osnapOn}`);

  await clickSelector('#polarToggleBtn', 'POLAR toggle');
  const polarOff = await evaluate('document.getElementById("polarToggleBtn").getAttribute("aria-pressed")');
  check(polarOff === "false", `polar did not toggle off: ${polarOff}`);
  await clickSelector('#polarToggleBtn', 'POLAR toggle back on');
  const polarOn = await evaluate('document.getElementById("polarToggleBtn").getAttribute("aria-pressed")');
  check(polarOn === "true", `polar did not toggle on: ${polarOn}`);

  await clickSelector('#drawingZoomFitBtn', 'Drawing Zoom Fit');

  step('drawing undo removes the last entity, and a layer can be added, hidden and locked');
  await clickSelector('#drawUndoBtn', 'drawing undo');
  await waitFor(`document.getElementById("drawingEntities").textContent === "${entitiesBefore + 5}"`, 'the drawing undo');

  await typeInto('#layerName', 'HOLES');
  await clickSelector('#addLayerBtn', 'Add layer');
  await waitFor(
    `Array.from(document.querySelectorAll("#layerSelect option"), (o) => o.value).includes("HOLES")`,
    'the HOLES layer'
  );
  await selectValue('#layerSelect', 'HOLES');
  const onBefore = await evaluate('document.getElementById("layerVisibleBtn").getAttribute("aria-pressed")');
  await clickSelector('#layerVisibleBtn', 'layer On');
  const onAfter = await evaluate('document.getElementById("layerVisibleBtn").getAttribute("aria-pressed")');
  check(onBefore !== onAfter, `the layer visibility did not toggle (${onBefore} → ${onAfter})`);
  await clickSelector('#layerVisibleBtn', 'layer On');
  const lockBefore = await evaluate('document.getElementById("layerLockBtn").getAttribute("aria-pressed")');
  await clickSelector('#layerLockBtn', 'layer lock');
  const lockAfter = await evaluate('document.getElementById("layerLockBtn").getAttribute("aria-pressed")');
  check(lockBefore !== lockAfter, `the layer lock did not toggle (${lockBefore} → ${lockAfter})`);
  await clickSelector('#layerLockBtn', 'layer unlock');

  step('save the drawing, download DXF and plot a titled PDF sheet');
  await clickSelector('#saveDrawingBtn', 'Save drawing');
  await waitFor(
    'document.getElementById("drawingSource").textContent.includes(".zdraft")',
    'the saved .zdraft'
  );
  const savedSource = await evaluate('document.getElementById("drawingSource").textContent');
  check(/\.zdraft$/.test(savedSource), `the drawing was saved as ${savedSource}`);

  await clickSelector('#downloadDxfBtn', 'DXF');
  await waitForDownload((name) => name.endsWith('.dxf'), 'the DXF download');
  const dxfName = downloadNames().find((name) => name.endsWith('.dxf'));
  const dxf = readFileSync(`${DOWNLOADS}/${dxfName}`, 'utf8');
  check(dxf.includes('SECTION') && dxf.trimEnd().endsWith('EOF'), 'the DXF is not a complete R12 file');
  check(/LINE|CIRCLE|POLYLINE/.test(dxf), 'the DXF has no geometry');

  await openSheetOptions();
  await selectValue('#sheetPaper', 'A4');
  await selectValue('#sheetOrientation', 'portrait');
  await typeInto('#sheetTitle', 'Browser drive sheet');
  await typeInto('#sheetNumber', 'AC-DRIVE-1');
  await clickSelector('#plotPdfBtn', 'Plot PDF');
  await waitForDownload((name) => name.endsWith('.pdf'), 'the plotted PDF');
  const pdfName = downloadNames().find((name) => name.endsWith('.pdf'));
  const pdf = readFileSync(`${DOWNLOADS}/${pdfName}`);
  check(pdf.subarray(0, 8).toString() === '%PDF-1.4', `the plot is not a PDF: ${pdf.subarray(0, 8)}`);
  check(pdf.length > 1000, `the plotted PDF is only ${pdf.length} bytes`);

  // --- revisions, palette, save, export -------------------------------------------------------
  step('named revision: record, change the model, restore, delete');
  const baseline = await volumeMm3();
  await typeInto('#revisionName', 'drive baseline');
  await clickSelector('#addRevisionBtn', 'Save revision');
  await waitFor('document.querySelectorAll("#revisions .revision-row").length === 1', 'the revision row');
  const revisionName = await evaluate('document.querySelector("#revisions .revision-name").textContent');
  check(revisionName === 'drive baseline', `the revision is named ${revisionName}`);

  await idle();
  await clickSelector('#tree .tree-item:nth-child(2)', 'the extrude row');
  const activeRow = await evaluate(
    'document.querySelector("#tree .tree-item.selected .tree-label")?.textContent ?? ""'
  );
  check(/^Extrude/.test(activeRow), `the selected row is ${activeRow}, not the first extrude`);
  // Change width rather than depth: width is not shared with the cut's tool depth, so the expected
  // volume stays a single analytic expression (the hole is fully inside the plate).
  const wider = dims.width + 3;
  const holeArea = Math.PI * (diameter / 2) ** 2;
  const changedExpected = (wider * dims.height - holeArea) * newDepth;
  await typeInto('#param-width', String(wider));
  const changed = await waitForVolume(changedExpected, 0.005);
  check(near(changed, changedExpected, 0.005), `the edit produced ${changed}, expected ${changedExpected}`);
  await screenshot('revision-edited');

  await clickSelector('#revisions [data-revision-restore]', 'Restore revision');
  const restored = await waitForVolume(baseline, 0.002);
  check(near(restored, baseline, 0.002), `restore produced ${restored}, expected ${baseline}`);
  await clickSelector('#revisions .revision-actions .link', 'Delete revision');
  await waitFor('document.querySelectorAll("#revisions .revision-row").length === 0', 'the deleted revision');

  step('command palette opens with Ctrl+K and runs a view command with the keyboard');
  const beforePaletteView = await screenshot('palette-view-before');
  await chord('k', 2);
  await waitFor('!document.getElementById("palette").hidden', 'the palette');
  await typeText('front');
  await waitFor(
    `document.querySelectorAll("#paletteList li[role=option]").length > 0`,
    'filtered commands'
  );
  const firstCommand = await evaluate(
    'document.querySelector("#paletteList .palette-label").textContent'
  );
  await pressKey('Enter', { wait: 250 });
  await waitFor('document.getElementById("palette").hidden', 'the palette to close');
  const afterPaletteView = await screenshot('palette-view-after');
  check(
    afterPaletteView.hash !== beforePaletteView.hash,
    `the palette ran "${firstCommand}" but the frame did not change`
  );

  await chord('k', 2);
  await waitFor('!document.getElementById("palette").hidden', 'the palette again');
  await pressKey('Escape');
  await waitFor('document.getElementById("palette").hidden', 'Escape to close the palette');

  step('Ctrl+S saves the project and the badge returns to saved');
  await blur();
  await chord('s', 2);
  await waitFor(
    'document.getElementById("dirtyBadge").textContent.trim() === "saved"',
    'the saved badge'
  );

  // Simulation HUD launcher check
  await clickSelector('#simBtn', 'Simulation Launcher');
  await waitFor('!document.getElementById("simulationHud").hidden', 'the simulation HUD open');
  await clickSelector('#simHudCloseBtn', 'Close Simulation HUD');
  await waitFor('document.getElementById("simulationHud").hidden', 'the simulation HUD closed');

  step('Export STL downloads a binary STL whose parsed volume matches the model');
  const shownVolume = await volumeMm3();
  const shownTriangles = Number(
    await evaluate('document.getElementById("viewport").dataset.triangles || 0')
  );
  await clickSelector('#exportBtn', 'Export STL');
  await waitForDownload((name) => name.endsWith('.stl'), 'the STL download');
  const stlName = downloadNames().find((name) => name.endsWith('.stl'));
  const stl = readFileSync(`${DOWNLOADS}/${stlName}`);
  const triangles = stl.readUInt32LE(80);
  check(stl.length === 84 + triangles * 50, `the STL length ${stl.length} does not match ${triangles} triangles`);
  check(triangles === shownTriangles, `the STL has ${triangles} triangles, the viewport ${shownTriangles}`);
  let volume = 0;
  for (let index = 0; index < triangles; index += 1) {
    const offset = 84 + index * 50 + 12;
    const ax = stl.readFloatLE(offset);
    const ay = stl.readFloatLE(offset + 4);
    const az = stl.readFloatLE(offset + 8);
    const bx = stl.readFloatLE(offset + 12);
    const by = stl.readFloatLE(offset + 16);
    const bz = stl.readFloatLE(offset + 20);
    const cx = stl.readFloatLE(offset + 24);
    const cy = stl.readFloatLE(offset + 28);
    const cz = stl.readFloatLE(offset + 32);
    volume += (ax * (by * cz - bz * cy) - ay * (bx * cz - bz * cx) + az * (bx * cy - by * cx)) / 6;
  }
  volume = Math.abs(volume);
  check(
    near(volume, shownVolume, 0.01),
    `the STL volume ${volume.toFixed(2)} mm³ does not match the model ${shownVolume} mm³`
  );

  step('no console errors and no failed requests');
  check(pageErrors.length === 0, `console errors: ${pageErrors.join(' | ')}`);
  check(failedRequests.length === 0, `failed requests: ${failedRequests.join(' | ')}`);
  const final = await screenshot('final');
  check(final.hash !== initial.hash, 'the final page is identical to the untouched page');

  const summary = {
    assertions: passed,
    steps,
    target,
    startVolume,
    finalVolume: shownVolume,
    stlVolume: Number(volume.toFixed(3)),
    stlTriangles: triangles,
    screenshots: shotNumber,
    downloads: downloadNames(),
  };
  writeFileSync(`${artifacts}/summary.json`, `${JSON.stringify(summary, null, 2)}\n`);
  console.log(`==> advanced browser drive passed (${passed} assertions, ${steps.length} steps)`);
  console.log(`    artifacts: ${artifacts}/ (${shotNumber} screenshots, ${summary.downloads.length} downloads)`);
} catch (error) {
  console.error(`FAIL: ${error.message}`);
  if (diagnose) await diagnose();
  process.exitCode = 1;
} finally {
  if (socket) socket.close();
  browser.kill();
}

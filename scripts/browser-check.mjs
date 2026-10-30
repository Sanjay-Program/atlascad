// Headless-browser smoke check against a running studio.
//
// This is the part of the simulation the HTTP client cannot see: the browser has to load the
// embedded assets, boot the WebGL viewport, render the tree from the live state and stay free of
// console errors. Usage:
//
//   node scripts/browser-check.mjs http://127.0.0.1:7788
//
// Exits 0 when every assertion holds, 1 otherwise. Skips (exit 0 with SKIP) when no Chrome is
// installed, so the gate stays runnable on a host without a browser.
import { spawn } from 'node:child_process';
import { existsSync, mkdirSync, readdirSync, statSync } from 'node:fs';

const TARGET = process.argv[2];
if (!TARGET) {
  console.error('usage: node scripts/browser-check.mjs <url>');
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
  console.log('SKIP: no Chrome found; the HTTP simulation still covers the API');
  process.exit(0);
}

const PORT = 9391;
const PROFILE = `/tmp/atlas-browser-check-${process.pid}`;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

let passed = 0;
function check(condition, message) {
  if (!condition) throw new Error(message);
  passed += 1;
}

async function devtools(path) {
  const response = await fetch(`http://127.0.0.1:${PORT}${path}`);
  return response.json();
}

const browser = spawn(
  CHROME,
  [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=${PROFILE}`,
    '--no-first-run',
    '--no-default-browser-check',
    '--disable-gpu',
    '--enable-unsafe-swiftshader',
    '--hide-scrollbars',
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
      const targets = await devtools('/json/list');
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
  const consoleErrors = [];
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
      consoleErrors.push(`exception: ${message.params.exceptionDetails.text}`);
    }
    if (message.method === 'Runtime.consoleAPICalled' && message.params.type === 'error') {
      consoleErrors.push(
        `console.error: ${message.params.args.map((arg) => arg.value ?? arg.description ?? '').join(' ')}`
      );
    }
    if (message.method === 'Log.entryAdded' && message.params.entry.level === 'error') {
      consoleErrors.push(`log: ${message.params.entry.text}`);
    }
  });

  const send = (method, params = {}) =>
    new Promise((resolve, reject) => {
      sequence += 1;
      pending.set(sequence, { resolve, reject });
      socket.send(JSON.stringify({ id: sequence, method, params }));
    });

  await send('Runtime.enable');
  await send('Log.enable');
  await send('Page.enable');

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

  const waitFor = async (expression, label, timeout = 25000) => {
    const deadline = Date.now() + timeout;
    while (Date.now() < deadline) {
      if (await evaluate(expression)) return;
      await sleep(100);
    }
    throw new Error(`timed out waiting for ${label}`);
  };

  await send('Page.navigate', { url: TARGET });
  await waitFor(
    'document.querySelectorAll("#tree .tree-item").length === 2',
    'the feature tree'
  );
  await waitFor('!document.getElementById("saveBtn").disabled', 'the app to be idle');

  const title = await evaluate('document.title');
  check(title.includes('Atlas'), `the page title is ${title}`);

  const labels = await evaluate(
    'Array.from(document.querySelectorAll("#tree .tree-label"), (row) => row.textContent.trim())'
  );
  check(labels.join('|') === 'Sketch 1|Extrude 2', `the tree is ${labels.join('|')}`);

  const properties = await evaluate(
    'document.getElementById("properties").textContent'
  );
  check(properties.includes('500 mm³'), `the properties pane shows the body: ${properties.slice(0, 120)}`);

  const badge = await evaluate('document.getElementById("dirtyBadge").textContent.trim()');
  check(badge === 'saved', `the project badge is ${badge}`);

  const recoveryHidden = await evaluate('document.getElementById("recoveryBanner").hidden');
  check(recoveryHidden, 'the recovery banner is hidden on a clean project');

  const exportEnabled = await evaluate('!document.getElementById("exportBtn").disabled');
  check(exportEnabled, 'export is enabled for a built body');

  // The plan drawing is fetched separately from the model, so it needs its own wait: the panel must
  // end up showing the sketch outline and a usable DXF button, not the loading placeholder.
  await waitFor(
    'document.querySelectorAll("#drawingPreview svg path").length === 1',
    'the plan preview'
  );
  const drawing = await evaluate(`({
    entities: document.getElementById("drawingEntities").textContent,
    layers: document.getElementById("drawingLayers").textContent,
    length: document.getElementById("drawingLength").textContent,
    disabled: document.getElementById("downloadDxfBtn").disabled,
  })`);
  check(drawing.entities === '1', `the drawing has ${drawing.entities} entities`);
  check(
    drawing.layers.split(', ').includes('OUTLINE'),
    `the drawing layers are ${drawing.layers}`
  );
  check(/mm$/.test(drawing.length), `the drawing length is ${drawing.length}`);
  check(!drawing.disabled, 'the DXF download is enabled for a drawing with geometry');

  // The editor: arm the line tool, click two points in the drawing, and check the document grew.
  // Each click is its own evaluation because the first click re-renders the SVG: dispatching both on
  // one (now detached) element would deliver only the first.
  await evaluate('document.querySelector(\'.draw-tools [data-tool="line"]\').click()');
  const clickDrawing = async (fx, fy) => {
    await evaluate(`(() => {
      const svg = document.querySelector("#drawingPreview svg");
      const rect = svg.getBoundingClientRect();
      svg.dispatchEvent(new MouseEvent("click", {
        clientX: rect.left + rect.width * ${fx},
        clientY: rect.top + rect.height * ${fy},
        bubbles: true,
      }));
      return true;
    })()`);
  };
  await clickDrawing(0.3, 0.5);
  await clickDrawing(0.7, 0.5);
  await waitFor(
    'document.getElementById("drawingEntities").textContent === "2"',
    'the drawn line to reach the document'
  );
  const drawn = await evaluate(`({
    paths: document.querySelectorAll("#drawingPreview svg path").length,
    source: document.getElementById("drawingSource").textContent,
    hint: document.getElementById("drawingHint").textContent,
  })`);
  check(drawn.paths >= 2, `the line is on the page (${drawn.paths} paths)`);
  check(drawn.source === 'model (live)', `the drawing source is ${drawn.source}`);

  // Saving turns the generated plan into a file of its own, which the panel says out loud.
  await evaluate('document.getElementById("saveDrawingBtn").click()');
  await waitFor(
    'document.getElementById("drawingSource").textContent.includes(".zdraft")',
    'the saved drawing file'
  );
  const savedSource = await evaluate('document.getElementById("drawingSource").textContent');
  check(/\.zdraft$/.test(savedSource), `the drawing was saved as ${savedSource}`);

  // A shared session shows a copyable link; a loopback session must not show one at all.
  const sharedTarget = /[?&]token=/.test(TARGET);
  const share = await evaluate(
    '({ hidden: document.getElementById("shareSection").hidden, value: document.getElementById("shareLink").value })'
  );
  check(
    share.hidden === !sharedTarget,
    `the share panel is ${share.hidden ? 'hidden' : 'shown'} for a ${sharedTarget ? 'shared' : 'loopback'} page`
  );
  if (sharedTarget) {
    check(/token=/.test(share.value), 'the share panel does not carry the token');
    // Export is the one download the page makes. Clicking the button and waiting for the download
    // proves the page's own fetch carried the token — a plain anchor would have been a 401.
    const downloads = `/tmp/atlas-browser-downloads-${process.pid}`;
    mkdirSync(downloads, { recursive: true });
    await send('Page.setDownloadBehavior', { behavior: 'allow', downloadPath: downloads });
    await evaluate('document.getElementById("exportBtn").click()');
    await waitFor(
      `Array.from(document.querySelectorAll("#log li")).some((row) => row.textContent.includes("Exported binary STL"))`,
      'the shared export',
      10000
    );
    const sizes = readdirSync(downloads).map((name) => statSync(`${downloads}/${name}`).size);
    check(
      sizes.some((size) => size > 84),
      `a shared session cannot export an STL with its token (${JSON.stringify(sizes)})`
    );
    await evaluate('document.getElementById("downloadDxfBtn").click()');
    await waitFor(
      `Array.from(document.querySelectorAll("#log li")).some((row) => row.textContent.includes("Exported DXF R12 plan"))`,
      'the shared DXF export',
      10000
    );
    const files = readdirSync(downloads);
    check(
      files.some((name) => name.endsWith('.dxf')),
      `a shared session cannot export a DXF with its token (${files.join(', ')})`
    );
    // The sheet plot is a second download, and in a shared session it takes the same token path.
    await evaluate('document.getElementById("plotPdfBtn").click()');
    await waitFor(
      `Array.from(document.querySelectorAll("#log li")).some((row) => row.textContent.includes("Plotted a PDF sheet"))`,
      'the shared PDF plot',
      10000
    );
    const afterPlot = readdirSync(downloads);
    check(
      afterPlot.some((name) => name.endsWith('.pdf')),
      `a shared session cannot plot a PDF with its token (${afterPlot.join(', ')})`
    );
  }

  // Named revisions: record the current body, change it through the parameter panel, then restore
  // the copy from the panel. This is the user-visible half of M11f, so it is driven as a user would.
  const baselineVolume = await evaluate(
    'document.getElementById("statusGeometry").textContent'
  );
  await evaluate(`(() => {
    document.getElementById("revisionName").value = "browser baseline";
    document.getElementById("addRevisionBtn").click();
    return true;
  })()`);
  await waitFor(
    'document.querySelectorAll("#revisions .revision-row").length === 1',
    'the named revision to appear'
  );
  const revisionName = await evaluate(
    'document.querySelector("#revisions .revision-name").textContent'
  );
  check(revisionName === 'browser baseline', `the revision is named ${revisionName}`);

  // A parameter edit through the panel must change the measured body.
  await evaluate(`(() => {
    const input = document.getElementById("param-depth");
    input.value = "12";
    input.dispatchEvent(new Event("change"));
    return true;
  })()`);
  await waitFor(
    `document.getElementById("statusGeometry").textContent !== ${JSON.stringify(baselineVolume)}`,
    'the thickened body'
  );
  check(true, 'the parameter edit reached the kernel');

  // Restoring the copy brings the original measurement back, and the palette offers the same action.
  await evaluate('document.querySelector("#revisions [data-revision-restore]").click()');
  await waitFor(
    `document.getElementById("statusGeometry").textContent === ${JSON.stringify(baselineVolume)}`,
    'the restored body'
  );
  const restoredVolume = await evaluate(
    'document.getElementById("statusGeometry").textContent'
  );
  check(
    restoredVolume === baselineVolume,
    `restore produced ${restoredVolume}, expected ${baselineVolume}`
  );
  const paletteHasRestore = await evaluate(`(() => {
    document.getElementById("paletteBtn").click();
    const input = document.getElementById("paletteInput");
    input.value = "restore revision";
    input.dispatchEvent(new Event("input"));
    const labels = Array.from(document.querySelectorAll("#paletteList .palette-label"), (n) => n.textContent);
    document.getElementById("paletteInput").dispatchEvent(new KeyboardEvent("keydown", { key: "Escape", bubbles: true }));
    return labels.some((label) => label.includes("Restore revision"));
  })()`);
  check(paletteHasRestore, 'the command palette does not offer the revision restore');

  // The arc-based sector is the M11e sketch entity, so its controls have to reach the API from the
  // page, not just exist in the markup. This runs last because it adds a third entity to the tree.
  await evaluate(`(() => {
    const kind = document.getElementById("newSketchKind");
    kind.value = "sector";
    kind.dispatchEvent(new Event("change"));
    document.getElementById("newSectorStart").value = "30";
    document.getElementById("newSectorSweep").value = "120";
    return true;
  })()`);
  const sectorForm = await evaluate(
    '({ shown: !document.getElementById("newSectorField").hidden, start: document.getElementById("newSectorStart").value })'
  );
  check(
    sectorForm.shown && sectorForm.start === '30',
    `the sector form did not open with its angles: ${JSON.stringify(sectorForm)}`
  );
  await evaluate('document.getElementById("addSketchBtn").click()');
  await waitFor(
    'document.querySelectorAll("#tree .tree-item").length === 3',
    'the sector sketch to appear in the tree'
  );
  const sectorRow = await evaluate(
    'Array.from(document.querySelectorAll("#tree .tree-badge"), (node) => node.textContent.trim())'
  );
  check(sectorRow.includes('sector'), `the new sketch is not a sector: ${JSON.stringify(sectorRow)}`);
  // Selecting the new sketch must show its own angles, not the defaults.
  const sectorProfile = await evaluate(`(() => {
    const sketch = document.querySelectorAll("#tree .tree-item")[2];
    sketch.click();
    return {
      hidden: document.getElementById("sectorField").hidden,
      start: document.getElementById("sectorStart").value,
      sweep: document.getElementById("sectorSweep").value,
      kind: document.getElementById("profileKind").value,
    };
  })()`);
  check(
    !sectorProfile.hidden &&
      sectorProfile.start === '30' &&
      sectorProfile.sweep === '120' &&
      sectorProfile.kind === 'sector',
    `the sector profile panel shows ${JSON.stringify(sectorProfile)}`
  );

  // M11g — a sketch on a raised plane and a boss joined onto the top face. The page has to send the
  // plane height and the extrude direction, and both panels have to show what the document stored.
  // The plate is 10 x 10 x 5, so a 5 mm circle at (5,5) on plane 5 is an inscribed boss.
  await evaluate(`(() => {
    const diameter = document.getElementById("param-diameter");
    diameter.value = "5";
    diameter.dispatchEvent(new Event("change"));
    return true;
  })()`);
  await waitFor('!document.getElementById("saveBtn").disabled', 'the diameter edit');
  await evaluate(`(() => {
    const kind = document.getElementById("newSketchKind");
    kind.value = "circle";
    kind.dispatchEvent(new Event("change"));
    document.getElementById("newOriginX").value = "5";
    document.getElementById("newOriginY").value = "5";
    document.getElementById("newOriginZ").value = "5";
    return true;
  })()`);
  await evaluate('document.getElementById("addSketchBtn").click()');
  await waitFor(
    'document.querySelectorAll("#tree .tree-item").length === 4',
    'the raised sketch'
  );
  const placement = await evaluate(`(() => {
    document.querySelectorAll("#tree .tree-item")[3].click();
    return {
      plane: document.getElementById("originZ").value,
      label: document.getElementById("placementLabel").textContent,
      properties: document.getElementById("properties").textContent,
    };
  })()`);
  check(placement.plane === '5', `the placement field shows plane ${placement.plane}`);
  check(/plane 5/.test(placement.label), `the placement label is ${placement.label}`);
  check(
    placement.properties.includes('Plane'),
    `the sketch properties do not report its plane: ${placement.properties.slice(-120)}`
  );

  await evaluate(`(() => {
    const operation = document.getElementById("newFeatureOperation");
    operation.value = "join";
    operation.dispatchEvent(new Event("change"));
    document.getElementById("newFeatureDirection").value = "up";
    document.getElementById("addFeatureBtn").click();
    return true;
  })()`);
  await waitFor(
    'document.querySelectorAll("#tree .tree-item").length === 5',
    'the boss feature'
  );
  const boss = await evaluate(`({
    properties: document.getElementById("properties").textContent,
    volume: document.getElementById("statusGeometry").textContent,
  })`);
  check(
    boss.properties.includes('up (+Z)'),
    `the extrude panel does not show its direction: ${boss.properties.slice(-160)}`
  );
  const bossVolume = Number((boss.volume.match(/([0-9.]+)\s*mm/) || [])[1]);
  check(
    bossVolume > 590 && bossVolume < 610,
    `the boss volume is ${boss.volume}, expected about 597 mm³`
  );

  check(consoleErrors.length === 0, `console errors: ${consoleErrors.join(' | ')}`);

  console.log(`==> browser check passed (${passed} assertions)`);
} catch (error) {
  console.error(`FAIL: ${error.message}`);
  process.exitCode = 1;
} finally {
  if (socket) socket.close();
  browser.kill();
}

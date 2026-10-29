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
import { existsSync } from 'node:fs';

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

  check(consoleErrors.length === 0, `console errors: ${consoleErrors.join(' | ')}`);

  console.log(`==> browser check passed (${passed} assertions)`);
} catch (error) {
  console.error(`FAIL: ${error.message}`);
  process.exitCode = 1;
} finally {
  if (socket) socket.close();
  browser.kill();
}

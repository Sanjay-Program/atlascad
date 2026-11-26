// Accessibility audit against a running studio.
//
//   node scripts/a11y-audit.mjs http://127.0.0.1:7791 [--json]
//
// Reports every form control (input/select/textarea) that has no programmatic name: no
// aria-label, no aria-labelledby target with text, no <label for>, no wrapping <label>, and no
// title. Every control is audited, including those in hidden workspaces, modals and panels,
// because they become visible when the user switches context; only type=hidden is skipped.
// Exits 0 when every control is named, 1 otherwise, and SKIP (exit 0) when Chrome is not
// installed.
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';

const TARGET = process.argv[2];
if (!TARGET) {
  console.error('usage: node scripts/a11y-audit.mjs <url> [--json]');
  process.exit(2);
}
const asJson = process.argv.includes('--json');

const CHROME_CANDIDATES = [
  '/opt/google/chrome/chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
];
const CHROME = CHROME_CANDIDATES.find((path) => existsSync(path));
if (!CHROME) {
  console.log('SKIP: no Chrome found; cannot audit the rendered DOM');
  process.exit(0);
}

const PORT = 9392;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const AUDIT = `(() => {
  const named = (el) => {
    if (el.getAttribute('aria-label')?.trim()) return true;
    const labelledby = el.getAttribute('aria-labelledby');
    if (labelledby) {
      const text = labelledby
        .split(/\\s+/)
        .map((id) => document.getElementById(id)?.textContent ?? '')
        .join(' ')
        .trim();
      if (text) return true;
    }
    if (el.id && document.querySelector('label[for="' + CSS.escape(el.id) + '"]')) return true;
    if (el.closest('label')) return true;
    if (el.getAttribute('title')?.trim()) return true;
    return false;
  };
  const controls = Array.from(document.querySelectorAll('input, select, textarea'))
    .filter((el) => el.type !== 'hidden');
  const unnamed = controls.filter((el) => !named(el)).map((el) => ({
    id: el.id || null,
    tag: el.tagName.toLowerCase(),
    type: el.type || null,
    name: el.getAttribute('name'),
    placeholder: el.getAttribute('placeholder'),
    nearText: (el.closest('.field, .field-row, .toolbar-group, .panel-section, td, div')?.textContent ?? '')
      .replace(/\\s+/g, ' ')
      .trim()
      .slice(0, 80),
  }));
  return { total: controls.length, unnamed: unnamed.length, controls: unnamed };
})()`;

const browser = spawn(
  CHROME,
  [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=/tmp/atlas-a11y-${process.pid}`,
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
  // Wait for the shell to finish rendering its dynamic panels.
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const ready = await send('Runtime.evaluate', {
      expression: 'document.querySelectorAll("#tree .tree-item").length > 0',
      returnByValue: true,
    });
    if (ready.result.value) break;
    await sleep(100);
  }
  await sleep(500);

  const result = await send('Runtime.evaluate', {
    expression: AUDIT,
    returnByValue: true,
    awaitPromise: true,
  });
  if (result.exceptionDetails) throw new Error(result.exceptionDetails.text);
  const report = result.result.value;

  if (asJson) {
    console.log(JSON.stringify(report, null, 2));
  } else {
    console.log(`form controls: ${report.total}`);
    console.log(`without a programmatic name: ${report.unnamed}`);
    for (const control of report.controls) {
      console.log(`  ${control.tag}${control.type ? `[${control.type}]` : ''} #${control.id ?? '(no id)'} — ${control.nearText}`);
    }
  }
  if (report.unnamed > 0) process.exitCode = 1;
} catch (error) {
  console.error(`FAIL: ${error.message}`);
  process.exitCode = 1;
} finally {
  if (socket) socket.close();
  browser.kill();
}

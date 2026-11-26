// WCAG 2.1 contrast audit against a running studio.
//
//   node scripts/contrast-audit.mjs http://127.0.0.1:7791 [--json]
//
// Samples every visible element that directly renders text, resolves its effective background by
// walking up to the first opaque ancestor, and computes the contrast ratio from the *computed*
// colours, not from design tokens. Elements whose background involves an image or gradient are
// skipped rather than guessed at. Disabled controls are skipped (WCAG exempts inactive controls).
//
// Normal text requires 4.5:1, large text (>= 24 px, or >= 18.66 px bold) 3:1. Exits 1 when any
// sampled pair fails, 0 when all pass, and SKIP (exit 0) when Chrome is not installed.
// Pass `--theme dark` to measure the other theme; the default is whatever the app boots with.
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';

const TARGET = process.argv[2];
if (!TARGET) {
  console.error('usage: node scripts/contrast-audit.mjs <url> [--json]');
  process.exit(2);
}
const asJson = process.argv.includes('--json');
const traceIndex = process.argv.indexOf('--trace');
const traceSelector = traceIndex > 0 ? process.argv[traceIndex + 1] : null;
const themeIndex = process.argv.indexOf('--theme');
const theme = themeIndex > 0 ? process.argv[themeIndex + 1] : null;

const CHROME_CANDIDATES = [
  '/opt/google/chrome/chrome',
  '/usr/bin/google-chrome',
  '/usr/bin/chromium',
  '/usr/bin/chromium-browser',
];
const CHROME = CHROME_CANDIDATES.find((path) => existsSync(path));
if (!CHROME) {
  console.log('SKIP: no Chrome found; contrast was not measured');
  process.exit(0);
}

const PORT = 9393;
const sleep = (ms) => new Promise((resolve) => setTimeout(resolve, ms));

const AUDIT = `(() => {
  const parse = (value) => {
    const match = value.match(/rgba?\\(([^)]+)\\)/);
    if (!match) return null;
    const parts = match[1].split(/[,/\\s]+/).filter(Boolean).map(Number);
    if (parts.length < 3 || parts.some(Number.isNaN)) return null;
    return { r: parts[0], g: parts[1], b: parts[2], a: parts.length > 3 ? parts[3] : 1 };
  };
  const channel = (v) => {
    const s = v / 255;
    return s <= 0.03928 ? s / 12.92 : Math.pow((s + 0.055) / 1.055, 2.4);
  };
  const luminance = (c) => 0.2126 * channel(c.r) + 0.7152 * channel(c.g) + 0.0722 * channel(c.b);
  const ratio = (a, b) => {
    const la = luminance(a);
    const lb = luminance(b);
    return (Math.max(la, lb) + 0.05) / (Math.min(la, lb) + 0.05);
  };
  const over = (top, bottom) => ({
    r: top.r * top.a + bottom.r * (1 - top.a),
    g: top.g * top.a + bottom.g * (1 - top.a),
    b: top.b * top.a + bottom.b * (1 - top.a),
    a: 1,
  });
  const selector = (el) => {
    if (el.id) return '#' + el.id;
    const cls = (el.className || '').toString().trim().split(/\\s+/).slice(0, 2).join('.');
    return el.tagName.toLowerCase() + (cls ? '.' + cls : '');
  };
  const opaqueBackground = (el) => {
    let node = el;
    while (node && node !== document.documentElement) {
      const style = getComputedStyle(node);
      if (style.backgroundImage !== 'none') return { reason: 'background image or gradient' };
      const colour = parse(style.backgroundColor);
      if (colour && colour.a >= 0.999) return { colour };
      if (colour && colour.a > 0) {
        // Composite this translucent layer over the parent chain.
        const parent = node.parentElement;
        if (!parent) return { colour };
        const below = opaqueBackground(parent);
        if (!below.colour) return below;
        return { colour: over(colour, below.colour) };
      }
      node = node.parentElement;
    }
    return { colour: { r: 255, g: 255, b: 255, a: 1 } };
  };
  const hasOwnText = (el) =>
    Array.from(el.childNodes).some((node) => node.nodeType === Node.TEXT_NODE && node.textContent.trim().length > 0);

  const seen = [];
  let skippedBackground = 0;
  for (const el of document.querySelectorAll('body *')) {
    if (!hasOwnText(el)) continue;
    if (el.closest('[aria-hidden="true"], [hidden], script, style, svg')) continue;
    if (el.closest('button:disabled, input:disabled, select:disabled, textarea:disabled, [disabled]')) continue;
    const style = getComputedStyle(el);
    if (style.display === 'none' || style.visibility === 'hidden') continue;
    const box = el.getBoundingClientRect();
    if (box.width === 0 || box.height === 0) continue;
    const colour = parse(style.color);
    if (!colour) continue;
    const background = opaqueBackground(el);
    if (!background.colour) {
      skippedBackground += 1;
      continue;
    }
    if (colour.a < 0.999) {
      // Text alpha is rare here; composite it over the background before measuring.
      colour.a = colour.a;
    }
    const effective = colour.a < 0.999 ? over(colour, background.colour) : colour;
    const size = parseFloat(style.fontSize);
    const weight = Number(style.fontWeight) || 400;
    const large = size >= 24 || (weight >= 700 && size >= 18.66);
    const required = large ? 3 : 4.5;
    const value = ratio(effective, background.colour);
    seen.push({
      selector: selector(el),
      text: (el.textContent || '').trim().replace(/\\s+/g, ' ').slice(0, 50),
      fontSize: size,
      weight,
      required,
      ratio: Math.round(value * 100) / 100,
      foreground: style.color,
      background: 'rgb(' + [background.colour.r, background.colour.g, background.colour.b]
        .map((n) => Math.round(n)).join(', ') + ')',
      pass: value + 0.005 >= required,
    });
  }
  const failures = seen.filter((entry) => !entry.pass);
  return {
    sampled: seen.length,
    skippedBackground,
    failures: failures.length,
    worst: failures.sort((a, b) => a.ratio - b.ratio).slice(0, 40),
  };
})()`;

const browser = spawn(
  CHROME,
  [
    '--headless=new',
    `--remote-debugging-port=${PORT}`,
    `--user-data-dir=/tmp/atlas-contrast-${process.pid}`,
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
  for (let attempt = 0; attempt < 100; attempt += 1) {
    const ready = await send('Runtime.evaluate', {
      expression: 'document.querySelectorAll("#tree .tree-item").length > 0',
      returnByValue: true,
    });
    if (ready.result.value) break;
    await sleep(100);
  }
  if (theme) {
    await send('Runtime.evaluate', {
      expression: `document.documentElement.dataset.theme = ${JSON.stringify(theme)}`,
      returnByValue: true,
    });
    await sleep(300);
  }
  await sleep(500);

  if (traceSelector) {
    const chain = await send('Runtime.evaluate', {
      expression: `(() => {
        const el = document.querySelector(${JSON.stringify(traceSelector)});
        if (!el) return null;
        const rows = [];
        let node = el;
        while (node) {
          const s = getComputedStyle(node);
          rows.push({
            node: node.tagName.toLowerCase() + (node.id ? '#' + node.id : '') + (node.className ? '.' + node.className.toString().trim().split(/\\s+/).join('.') : ''),
            color: s.color,
            backgroundColor: s.backgroundColor,
            backgroundImage: s.backgroundImage.slice(0, 60),
            opacity: s.opacity,
          });
          node = node.parentElement;
        }
        return rows;
      })()`,
      returnByValue: true,
    });
    console.log(JSON.stringify(chain.result.value, null, 2));
    socket.close();
    browser.kill();
    process.exit(0);
  }

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
    console.log(`text elements sampled: ${report.sampled}`);
    console.log(`skipped (background image/gradient): ${report.skippedBackground}`);
    console.log(`failing pairs: ${report.failures}`);
    for (const entry of report.worst) {
      console.log(
        `  ${entry.ratio}:1 (needs ${entry.required}:1) ${entry.selector} — ${entry.foreground} on ${entry.background} — "${entry.text}"`
      );
    }
  }
  if (report.failures > 0) process.exitCode = 1;
} catch (error) {
  console.error(`FAIL: ${error.message}`);
  process.exitCode = 1;
} finally {
  if (socket) socket.close();
  browser.kill();
}

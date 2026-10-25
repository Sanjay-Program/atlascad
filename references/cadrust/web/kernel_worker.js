// Web Worker wrapper around the wasm kernel so evaluation never blocks the page.
import { loadKernel } from './cadrust_wasm.js';

const ready = loadKernel(new URL('cadrust_wasm.wasm', import.meta.url).href);

onmessage = async (ev) => {
  const { id, op, args } = ev.data;
  try {
    const kernel = await ready;
    if (op === 'ping') postMessage({ id, ok: true });
    else if (op === 'run') { const r = kernel.run(args[0]); postMessage({ id, ok: true, status: r.status, glb: r.glb.buffer }, [r.glb.buffer]); }
    else if (op === 'export') { const bytes = kernel.export(args[0], args[1], args[2] || []); postMessage({ id, ok: true, bytes: bytes.buffer }, [bytes.buffer]); }
    else if (op === 'import') { const data = kernel.import(args[0], new Uint8Array(args[1])); postMessage({ id, ok: true, data }); }
    else postMessage({ id, ok: false, error: `unknown op ${op}` });
  } catch (e) {
    postMessage({ id, ok: false, error: String(e && e.message || e) });
  }
};

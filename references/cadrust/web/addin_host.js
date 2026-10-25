// Browser host for cadrust add-ins. Loads the list from /api/addins (or a static addins.json
// in serverless mode) and runs wasm add-ins in-page with the same packed-buffer ABI as the
// kernel, or JavaScript add-ins through a dynamic import.
//
// JavaScript add-in contract: `export async function run(command, params, api)` returning
// `{ document?, message? }`; `api` offers `doc()`, `bodies()`, `export(format, bodyIds)`,
// `download(bytes, name)` and `notify(text)`.

export async function loadAddins(listUrl = '/api/addins') {
  const res = await fetch(listUrl);
  if (!res.ok) return [];
  const entries = await res.json();
  const base = new URL(listUrl, location.href);
  for (const e of entries) {
    e.url = new URL(e.url, base).href;
    // A static listing carries no manifest for wasm add-ins; ask the module itself.
    if (e.kind === 'wasm' && !e.manifest) {
      try {
        const inst = await instantiate(e.url); cache.set(e.url, inst);
        const [m] = inst.call(inst.ex.addin_manifest);
        e.manifest = JSON.parse(new TextDecoder().decode(m));
      } catch (err) { console.warn('add-in manifest failed:', e.url, err.message); e.manifest = null; }
    }
  }
  return entries.filter(e => e.manifest).map(e => ({ ...e, run: (command, params, doc, api) => runAddin(e, command, params, doc, api) }));
}

const cache = new Map();

async function runAddin(entry, command, params, doc, api) {
  if (entry.kind === 'js') {
    const mod = cache.get(entry.url) || (await import(entry.url));
    cache.set(entry.url, mod);
    return mod.run(command, params, api);
  }
  const inst = cache.get(entry.url) || (await instantiate(entry.url));
  cache.set(entry.url, inst);
  const req = JSON.stringify({ command, params, document: doc, selection: api.selection ? api.selection() : [] });
  const [out] = inst.call(inst.ex.addin_run, req);
  const resp = JSON.parse(new TextDecoder().decode(out));
  if (resp.error) throw new Error(resp.error);
  return resp;
}

async function instantiate(url) {
  const module = await WebAssembly.compileStreaming(fetch(url));
  const imports = {};
  for (const imp of WebAssembly.Module.imports(module)) (imports[imp.module] ||= {})[imp.name] = () => 0;
  const { exports: ex } = await WebAssembly.instantiate(module, imports);
  const enc = new TextEncoder();
  const put = (bytes) => { const p = ex.addin_alloc(bytes.length); new Uint8Array(ex.memory.buffer, p, bytes.length).set(bytes); return [p, bytes.length]; };
  const take = (ptr) => {
    const view = new DataView(ex.memory.buffer);
    const n = view.getUint32(ptr, true);
    const lens = []; for (let i = 0; i < n; i++) lens.push(view.getUint32(ptr + 4 + 4 * i, true));
    let off = ptr + 4 + 4 * n; const parts = [];
    for (const len of lens) { parts.push(new Uint8Array(ex.memory.buffer.slice(off, off + len))); off += len; }
    ex.addin_free(ptr, off - ptr);
    return parts;
  };
  const call = (fn, ...bufs) => {
    const handles = bufs.map(b => put(typeof b === 'string' ? enc.encode(b) : b));
    const out = take(fn(...handles.flat()));
    for (const [p, l] of handles) ex.addin_free(p, l);
    return out;
  };
  return { ex, call };
}

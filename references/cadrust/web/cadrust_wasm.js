// Glue for the serverless kernel (crates/cadrust-wasm). Works in browsers, workers and Node.
// Usage: const k = await loadKernel(url_or_bytes); k.run(docJson) -> { status, glb }; k.export(docJson, 'step', [bodyIds]) -> Uint8Array;
// k.import('step', bytes) -> base64 BRep text.

export async function loadKernel(source) {
  let module;
  if (source instanceof Uint8Array || source instanceof ArrayBuffer) module = await WebAssembly.compile(source);
  else if (typeof WebAssembly.compileStreaming === 'function' && typeof fetch === 'function') module = await WebAssembly.compileStreaming(fetch(source));
  else module = await WebAssembly.compile(await (await fetch(source)).arrayBuffer());
  const imports = {};
  for (const imp of WebAssembly.Module.imports(module)) {
    (imports[imp.module] ||= {})[imp.name] = imp.kind === 'function' ? () => 0 : undefined;
  }
  const inst = await WebAssembly.instantiate(module, imports);
  const ex = inst.exports;
  ex.cadrust_init();
  const enc = new TextEncoder(), dec = new TextDecoder();

  const put = (bytes) => { const p = ex.cadrust_alloc(bytes.length); new Uint8Array(ex.memory.buffer, p, bytes.length).set(bytes); return [p, bytes.length]; };
  const take = (ptr) => {
    const view = new DataView(ex.memory.buffer);
    const n = view.getUint32(ptr, true);
    const lens = []; for (let i = 0; i < n; i++) lens.push(view.getUint32(ptr + 4 + 4 * i, true));
    let off = ptr + 4 + 4 * n; const parts = [];
    for (const len of lens) { parts.push(new Uint8Array(ex.memory.buffer.slice(off, off + len))); off += len; }
    ex.cadrust_free(ptr, off - ptr);
    return parts;
  };
  const call = (fn, ...bufs) => {
    const handles = bufs.map(b => put(typeof b === 'string' ? enc.encode(b) : b));
    const out = take(fn(...handles.flat()));
    for (const [p, l] of handles) ex.cadrust_free(p, l);
    return out;
  };
  return {
    run(doc) {
      const [status, glb] = call(ex.cadrust_run, typeof doc === 'string' ? doc : JSON.stringify(doc));
      return { status: JSON.parse(dec.decode(status)), glb };
    },
    export(doc, format, bodies = []) {
      const [msg, bytes] = call(ex.cadrust_export, typeof doc === 'string' ? doc : JSON.stringify(doc), JSON.stringify({ format, bodies }));
      const m = dec.decode(msg);
      if (m !== 'ok') throw new Error(m);
      return bytes;
    },
    /** STEP or BRep bytes -> base64 BRep text for an Import feature. */
    import(format, bytes) {
      const [msg, data] = call(ex.cadrust_import, format, bytes);
      const m = dec.decode(msg);
      if (m !== 'ok') throw new Error(m);
      return dec.decode(data);
    },
    imports: WebAssembly.Module.imports(module).map(i => `${i.module}.${i.name}`),
  };
}

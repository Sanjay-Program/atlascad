//! wasm32 entry points for the serverless web app. No wasm-bindgen: the host
//! (web/cadrust_wasm.js or any other embedder) talks through four exports.
//!
//! Memory ABI: the host allocates input buffers with `cadrust_alloc`, writes bytes,
//! calls an operation which returns a packed buffer, and frees both.
//! Packed buffer layout: u32 part count, then u32 length per part, then the parts.
//! `cadrust_run` returns [status_json, glb]; `cadrust_export(doc, options_json)` returns [message, bytes]
//! where message is "ok" or an error text (bytes empty on error).

use cadrust_core::{evaluate, evaluate_with, Document, EvalCache, Solid};
use std::cell::RefCell;
use std::collections::BTreeMap;

thread_local! {
    /// Incremental evaluation cache of this worker (see cadrust_core::graph).
    static CACHE: RefCell<EvalCache> = RefCell::new(EvalCache::new());
}

#[cfg(target_arch = "wasm32")]
extern "C" {
    fn __wasm_call_ctors();
}

/// Run OCCT's static constructors. Must be called once before any other export.
#[no_mangle]
pub extern "C" fn cadrust_init() {
    #[cfg(target_arch = "wasm32")]
    {
        cadrum::__anchor_wasi_stub();
        unsafe { __wasm_call_ctors() };
    }
}

#[no_mangle]
pub extern "C" fn cadrust_alloc(len: u32) -> *mut u8 {
    let mut v = Vec::<u8>::with_capacity(len as usize);
    let p = v.as_mut_ptr();
    std::mem::forget(v);
    p
}

/// # Safety
/// `ptr` must come from `cadrust_alloc` or a packed result and `len` must be its full size.
#[no_mangle]
pub unsafe extern "C" fn cadrust_free(ptr: *mut u8, len: u32) {
    drop(Vec::from_raw_parts(ptr, 0, len as usize));
}

fn pack(parts: &[&[u8]]) -> *mut u8 {
    let mut out = Vec::with_capacity(4 + 4 * parts.len() + parts.iter().map(|p| p.len()).sum::<usize>());
    out.extend_from_slice(&(parts.len() as u32).to_le_bytes());
    for p in parts {
        out.extend_from_slice(&(p.len() as u32).to_le_bytes());
    }
    for p in parts {
        out.extend_from_slice(p);
    }
    out.shrink_to_fit();
    let p = out.as_mut_ptr();
    std::mem::forget(out);
    p
}

unsafe fn input<'a>(ptr: *const u8, len: u32) -> &'a [u8] {
    std::slice::from_raw_parts(ptr, len as usize)
}

/// Evaluate a document. Returns [status_json, glb]; on a parse error status_json
/// is `{"error": "..."}` and glb is empty.
#[no_mangle]
pub unsafe extern "C" fn cadrust_run(doc_ptr: *const u8, doc_len: u32) -> *mut u8 {
    let doc: Document = match serde_json::from_slice(input(doc_ptr, doc_len)) {
        Ok(d) => d,
        Err(e) => return pack(&[serde_json::json!({ "error": e.to_string() }).to_string().as_bytes(), &[]]),
    };
    let (ev, cache_len) = match CACHE.with(|c| { let mut c = c.borrow_mut(); evaluate_with(&doc, &BTreeMap::new(), Some(&mut c)).map(|ev| (ev, c.len())) }) {
        Ok(x) => x,
        Err(e) => return pack(&[serde_json::json!({ "error": e.to_string() }).to_string().as_bytes(), &[]]),
    };
    let mut glb = Vec::new();
    if !ev.bodies.is_empty() {
        if let Err(e) = Solid::mesh(ev.solids(), Default::default()).and_then(|m| m.write_gltf_binary(&mut glb)) {
            return pack(&[serde_json::json!({ "error": format!("mesh failed: {e:?}") }).to_string().as_bytes(), &[]]);
        }
    }
    let status = serde_json::json!({ "status": ev.status, "bodies": ev.info(), "edges": ev.edges(), "sketches": ev.sketches, "reused": ev.reused, "cache_entries": cache_len }).to_string();
    pack(&[status.as_bytes(), &glb])
}

/// Export bodies as step, brep, stl or glb. `opt` is JSON `{"format": "step", "bodies": ["id"...]}`
/// (empty or missing `bodies` means all). Returns [message, bytes].
#[no_mangle]
pub unsafe extern "C" fn cadrust_export(doc_ptr: *const u8, doc_len: u32, opt_ptr: *const u8, opt_len: u32) -> *mut u8 {
    let result = (|| -> Result<Vec<u8>, String> {
        let opt: serde_json::Value = serde_json::from_slice(input(opt_ptr, opt_len)).map_err(|e| e.to_string())?;
        let format = opt["format"].as_str().unwrap_or("step").to_string();
        let bodies: Vec<String> = opt["bodies"].as_array().map(|a| a.iter().filter_map(|v| v.as_str().map(String::from)).collect()).unwrap_or_default();
        let doc: Document = serde_json::from_slice(input(doc_ptr, doc_len)).map_err(|e| e.to_string())?;
        let ev = evaluate(&doc, &BTreeMap::new()).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let solids = ev.solids_of(&bodies);
        match format.as_str() {
            "step" => Solid::write_step(solids, &mut out).map_err(|e| format!("{e:?}"))?,
            "brep" => Solid::write_brep(solids, &mut out).map_err(|e| format!("{e:?}"))?,
            "stl" => Solid::mesh(solids, Default::default()).and_then(|m| m.write_stl(&mut out)).map_err(|e| format!("{e:?}"))?,
            "glb" => Solid::mesh(solids, Default::default()).and_then(|m| m.write_gltf_binary(&mut out)).map_err(|e| format!("{e:?}"))?,
            other => return Err(format!("unsupported format {other}")),
        }
        Ok(out)
    })();
    match result {
        Ok(bytes) => pack(&[b"ok", &bytes]),
        Err(e) => pack(&[e.as_bytes(), &[]]),
    }
}

/// Convert a STEP or BRep file to the base64 BRep text of an `Import` feature.
/// `fmt` is "step" or "brep". Returns [message, data] where message is "ok" or an error.
#[no_mangle]
pub unsafe extern "C" fn cadrust_import(fmt_ptr: *const u8, fmt_len: u32, ptr: *const u8, len: u32) -> *mut u8 {
    let result = (|| -> Result<String, String> {
        let fmt = std::str::from_utf8(input(fmt_ptr, fmt_len)).map_err(|e| e.to_string())?;
        let mut reader = input(ptr, len);
        let solids = match fmt {
            "step" | "stp" => Solid::read_step(&mut reader),
            "brep" => Solid::read_brep(&mut reader),
            other => return Err(format!("unsupported format {other}")),
        }
        .map_err(|e| format!("read failed: {e:?}"))?;
        if solids.is_empty() {
            return Err("the file has no solid".into());
        }
        let mut out = Vec::new();
        Solid::write_brep(solids.iter(), &mut out).map_err(|e| format!("{e:?}"))?;
        Ok(cadrust_core::b64::encode(&out))
    })();
    match result {
        Ok(data) => pack(&[b"ok", data.as_bytes()]),
        Err(e) => pack(&[e.as_bytes(), &[]]),
    }
}

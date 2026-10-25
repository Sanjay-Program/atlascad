//! Add-in side of the cadrust add-in ABI (see crates/cadrust-addin). Use the `addin!` macro:
//!
//! ```ignore
//! cadrust_addin_sdk::addin!(MANIFEST_JSON, |req: serde_json::Value| -> serde_json::Value { ... });
//! ```
//! The closure receives `{"command","params","document","selection"}` and returns
//! `{"document": ..}` or `{"error": ".."}`.

pub use serde_json;

/// Pack parts as [u32 count][u32 len...][bytes...] and leak the buffer to the host.
pub fn pack(parts: &[&[u8]]) -> *mut u8 {
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

/// Read a number parameter with a default.
pub fn num(params: &serde_json::Value, k: &str, default: f64) -> f64 {
    match params.get(k) {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(default),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(default),
        _ => default,
    }
}

#[macro_export]
macro_rules! addin {
    ($manifest:expr, $handler:expr) => {
        #[no_mangle]
        pub extern "C" fn addin_alloc(len: u32) -> *mut u8 {
            let mut v = Vec::<u8>::with_capacity(len as usize);
            let p = v.as_mut_ptr();
            std::mem::forget(v);
            p
        }

        #[no_mangle]
        pub unsafe extern "C" fn addin_free(ptr: *mut u8, len: u32) {
            drop(Vec::from_raw_parts(ptr, 0, len as usize));
        }

        #[no_mangle]
        pub extern "C" fn addin_manifest() -> *mut u8 {
            $crate::pack(&[($manifest).as_bytes()])
        }

        #[no_mangle]
        pub unsafe extern "C" fn addin_run(ptr: *const u8, len: u32) -> *mut u8 {
            let input = std::slice::from_raw_parts(ptr, len as usize);
            let handler: fn($crate::serde_json::Value) -> $crate::serde_json::Value = $handler;
            let out = match $crate::serde_json::from_slice::<$crate::serde_json::Value>(input) {
                Ok(req) => handler(req),
                Err(e) => $crate::serde_json::json!({ "error": format!("bad request: {e}") }),
            };
            $crate::pack(&[out.to_string().as_bytes()])
        }
    };
}

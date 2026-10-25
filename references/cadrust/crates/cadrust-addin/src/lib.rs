//! cadrust add-in host. An add-in is a `wasm32-unknown-unknown` module with four exports:
//!
//! - `addin_alloc(len: u32) -> ptr`, `addin_free(ptr, len)`
//! - `addin_manifest() -> ptr` : packed [manifest_json]
//! - `addin_run(ptr, len) -> ptr` : packed [response_json] for a request JSON
//!
//! Packed buffer: u32 part count, u32 length per part, then the parts (little endian).
//! Request: `{"command", "params", "document", "selection"}`.
//! Response: `{"document"?, "message"?, "error"?}`. The host replaces the document when
//! one is returned. The same ABI runs in the browser (web/addin_host.js) and here under wasmi,
//! so one add-in binary serves the web app and the CLI.

use serde::{Deserialize, Serialize};
use wasmi::{Engine, Instance, Linker, Memory, Module, Store, TypedFunc};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("wasm: {0}")]
    Wasm(String),
    #[error("add-in: {0}")]
    Addin(String),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
}

impl From<wasmi::errors::MemoryError> for Error {
    fn from(e: wasmi::errors::MemoryError) -> Self {
        Error::Wasm(e.to_string())
    }
}

impl From<wasmi::Error> for Error {
    fn from(e: wasmi::Error) -> Self {
        Error::Wasm(e.to_string())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Param {
    pub k: String,
    pub label: String,
    #[serde(default)]
    pub default: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Command {
    pub id: String,
    pub label: String,
    #[serde(default)]
    pub params: Vec<Param>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Manifest {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub version: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub commands: Vec<Command>,
}

#[derive(Debug, Clone, Serialize)]
pub struct Request<'a> {
    pub command: &'a str,
    pub params: serde_json::Value,
    pub document: &'a serde_json::Value,
    pub selection: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Default)]
pub struct Response {
    pub document: Option<serde_json::Value>,
    pub message: Option<String>,
    pub error: Option<String>,
}

pub struct Addin {
    store: Store<()>,
    memory: Memory,
    alloc: TypedFunc<u32, u32>,
    free: TypedFunc<(u32, u32), ()>,
    manifest_fn: TypedFunc<(), u32>,
    run_fn: TypedFunc<(u32, u32), u32>,
}

impl Addin {
    pub fn load(bytes: &[u8]) -> Result<Self, Error> {
        let engine = Engine::default();
        let module = Module::new(&engine, bytes)?;
        let mut store = Store::new(&engine, ());
        let linker = <Linker<()>>::new(&engine);
        let instance: Instance = linker.instantiate_and_start(&mut store, &module)?;
        let memory = instance.get_memory(&store, "memory").ok_or_else(|| Error::Addin("no exported memory".into()))?;
        let alloc = instance.get_typed_func::<u32, u32>(&store, "addin_alloc")?;
        let free = instance.get_typed_func::<(u32, u32), ()>(&store, "addin_free")?;
        let manifest_fn = instance.get_typed_func::<(), u32>(&store, "addin_manifest")?;
        let run_fn = instance.get_typed_func::<(u32, u32), u32>(&store, "addin_run")?;
        Ok(Self { store, memory, alloc, free, manifest_fn, run_fn })
    }

    pub fn load_file(path: &std::path::Path) -> Result<Self, Error> {
        let bytes = std::fs::read(path).map_err(|e| Error::Addin(format!("{}: {e}", path.display())))?;
        Self::load(&bytes)
    }

    fn unpack(&mut self, ptr: u32) -> Result<Vec<Vec<u8>>, Error> {
        let mut head = [0u8; 4];
        self.memory.read(&self.store, ptr as usize, &mut head)?;
        let n = u32::from_le_bytes(head) as usize;
        let mut lens = vec![0u8; 4 * n];
        self.memory.read(&self.store, ptr as usize + 4, &mut lens)?;
        let lens: Vec<usize> = lens.chunks(4).map(|c| u32::from_le_bytes([c[0], c[1], c[2], c[3]]) as usize).collect();
        let mut off = ptr as usize + 4 + 4 * n;
        let mut parts = Vec::with_capacity(n);
        for len in &lens {
            let mut buf = vec![0u8; *len];
            self.memory.read(&self.store, off, &mut buf)?;
            off += len;
            parts.push(buf);
        }
        let total = (4 + 4 * n + lens.iter().sum::<usize>()) as u32;
        self.free.call(&mut self.store, (ptr, total))?;
        Ok(parts)
    }

    pub fn manifest(&mut self) -> Result<Manifest, Error> {
        let ptr = self.manifest_fn.call(&mut self.store, ())?;
        let parts = self.unpack(ptr)?;
        let text = parts.first().ok_or_else(|| Error::Addin("empty manifest".into()))?;
        Ok(serde_json::from_slice(text)?)
    }

    pub fn run(&mut self, req: &Request) -> Result<Response, Error> {
        let text = serde_json::to_vec(req)?;
        let ptr = self.alloc.call(&mut self.store, text.len() as u32)?;
        self.memory.write(&mut self.store, ptr as usize, &text)?;
        let out = self.run_fn.call(&mut self.store, (ptr, text.len() as u32))?;
        self.free.call(&mut self.store, (ptr, text.len() as u32))?;
        let parts = self.unpack(out)?;
        let text = parts.first().ok_or_else(|| Error::Addin("empty response".into()))?;
        let resp: Response = serde_json::from_slice(text)?;
        if let Some(e) = &resp.error {
            return Err(Error::Addin(e.clone()));
        }
        Ok(resp)
    }
}

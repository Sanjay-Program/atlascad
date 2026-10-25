//! Minimal HTTP server for the web app: serves the static page and evaluates
//! documents posted by the browser. Geometry never leaves cadrust-core.

use anyhow::Result;
use axum::{
    body::Bytes,
    extract::State,
    http::{header, StatusCode},
    response::{Html, IntoResponse},
    routing::{get, post},
    Json, Router,
};
use cadrust_core::{evaluate, Document, Solid};
use std::collections::BTreeMap;
use axum::extract::Path as UrlPath;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

const INDEX_HTML: &str = include_str!("../../../web/index.html");
/// Static web files embedded next to the page; CADRUST_WEB_DIR overrides them during development.
const WEB_FILES: &[(&str, &str, &str)] = &[
    ("cadrust_wasm.js", "text/javascript", include_str!("../../../web/cadrust_wasm.js")),
    ("addin_host.js", "text/javascript", include_str!("../../../web/addin_host.js")),
    ("kernel_worker.js", "text/javascript", include_str!("../../../web/kernel_worker.js")),
];

struct AppState {
    doc: Mutex<Document>,
    path: PathBuf,
    addins: PathBuf,
    /// Incremental evaluation cache shared by every `/api/run` of this server.
    cache: Mutex<cadrust_core::EvalCache>,
}

pub fn run(doc: Document, path: PathBuf, addins: PathBuf, host: &str, port: u16) -> Result<()> {
    let rt = tokio::runtime::Runtime::new()?;
    rt.block_on(async move {
        let state = Arc::new(AppState { doc: Mutex::new(doc), path, addins, cache: Mutex::new(cadrust_core::EvalCache::new()) });
        let app = Router::new()
            .route("/", get(index))
            .route("/web/{file}", get(web_file))
            .route("/{file}", get(web_file))
            .route("/api/doc", get(get_doc))
            .route("/api/doc", post(save_doc))
            .route("/api/fs", get(fs_list))
            .route("/api/fs/file", get(fs_file))
            .route("/api/fs/mkdir", post(fs_mkdir))
            .route("/api/run", post(run_doc))
            .route("/api/graph", post(graph_doc))
            .route("/api/export", post(export_doc))
            .route("/api/import", post(import_file))
            .route("/api/addins", get(list_addins))
            .route("/api/examples", get(list_examples))
            .route("/examples/{file}", get(example_file))
            .route("/addins/{*file}", get(addin_file))
            .with_state(state);
        let addr = format!("{host}:{port}");
        eprintln!("cadrust web app at http://{addr}");
        let listener = tokio::net::TcpListener::bind(&addr).await?;
        axum::serve(listener, app).await?;
        Ok(())
    })
}

/// Serve the embedded page, or the file from CADRUST_WEB_DIR when set (live editing during development).
async fn index() -> Html<String> {
    if let Ok(dir) = std::env::var("CADRUST_WEB_DIR") {
        if let Ok(text) = std::fs::read_to_string(std::path::Path::new(&dir).join("index.html")) {
            return Html(text);
        }
    }
    Html(INDEX_HTML.to_string())
}

async fn web_file(UrlPath(file): UrlPath<String>) -> impl IntoResponse {
    if let Ok(dir) = std::env::var("CADRUST_WEB_DIR") {
        if !file.contains("..") {
            if let Ok(text) = std::fs::read_to_string(Path::new(&dir).join(&file)) {
                return (StatusCode::OK, [(header::CONTENT_TYPE, mime_of(&file))], text).into_response();
            }
        }
    }
    match WEB_FILES.iter().find(|(name, _, _)| *name == file) {
        Some((_, mime, text)) => (StatusCode::OK, [(header::CONTENT_TYPE, mime.to_string())], text.to_string()).into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

fn mime_of(file: &str) -> String {
    match file.rsplit('.').next().unwrap_or("") {
        "js" | "mjs" => "text/javascript",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "html" => "text/html",
        "css" => "text/css",
        _ => "application/octet-stream",
    }
    .to_string()
}

/// List add-ins found in the add-in directory: `<id>.wasm` modules (manifest read through the
/// wasm host) and `<id>/manifest.json` JavaScript add-ins. Both are served under /addins/.
async fn list_addins(State(st): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let dir = st.addins.clone();
    let list = tokio::task::spawn_blocking(move || scan_addins(&dir)).await.unwrap_or_default();
    Json(serde_json::Value::Array(list))
}

fn scan_addins(dir: &Path) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else { return out };
    let mut entries: Vec<_> = entries.flatten().collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let p = e.path();
        let name = e.file_name().to_string_lossy().to_string();
        if p.extension().map_or(false, |x| x == "wasm") {
            match cadrust_addin::Addin::load_file(&p).and_then(|mut a| a.manifest()) {
                Ok(m) => out.push(serde_json::json!({ "kind": "wasm", "url": format!("/addins/{name}"), "manifest": m })),
                Err(err) => eprintln!("add-in {name}: {err}"),
            }
        } else if p.is_dir() {
            if let Ok(text) = std::fs::read_to_string(p.join("manifest.json")) {
                match serde_json::from_str::<serde_json::Value>(&text) {
                    Ok(m) => {
                        let entry = m["entry"].as_str().unwrap_or("main.js").to_string();
                        out.push(serde_json::json!({ "kind": "js", "url": format!("/addins/{name}/{entry}"), "manifest": m }));
                    }
                    Err(err) => eprintln!("add-in {name}: manifest.json: {err}"),
                }
            }
        }
    }
    out
}

async fn addin_file(State(st): State<Arc<AppState>>, UrlPath(file): UrlPath<String>) -> impl IntoResponse {
    if file.contains("..") {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    match std::fs::read(st.addins.join(&file)) {
        Ok(bytes) => (StatusCode::OK, [(header::CONTENT_TYPE, mime_of(&file))], bytes).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

async fn get_doc(State(st): State<Arc<AppState>>) -> Json<Document> {
    let mut doc = st.doc.lock().unwrap().clone();
    if doc.name.is_none() {
        doc.name = Some(doc_stem(&st.path));
    }
    Json(doc)
}

/// "bracket" for examples/bracket.cadrust.json.
/// Folder the served document lives in: the root every file operation is confined to.
fn root(st: &AppState) -> PathBuf {
    st.path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."))
}

/// Resolve a relative path from the browser inside the root; `None` for anything that escapes it.
fn safe_rel(st: &AppState, rel: &str) -> Option<PathBuf> {
    let rel = rel.trim_matches('/');
    if rel.split('/').any(|c| c == ".." || c.starts_with('\\')) || rel.contains('\\') || rel.starts_with('/') {
        return None;
    }
    Some(if rel.is_empty() { root(st) } else { root(st).join(rel) })
}

type Query = axum::extract::Query<std::collections::HashMap<String, String>>;

/// Folders and `.cadrust.json` documents under `?path=` (relative to the root).
async fn fs_list(State(st): State<Arc<AppState>>, axum::extract::Query(q): Query) -> impl IntoResponse {
    let rel = q.get("path").cloned().unwrap_or_default();
    let Some(dir) = safe_rel(&st, &rel) else { return (StatusCode::BAD_REQUEST, "bad path").into_response() };
    let (mut dirs, mut files) = (Vec::new(), Vec::new());
    if let Ok(entries) = std::fs::read_dir(&dir) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            if name.starts_with('.') {
                continue;
            }
            let meta = e.metadata().ok();
            if meta.as_ref().map_or(false, |m| m.is_dir()) {
                dirs.push(name);
            } else if name.ends_with(".cadrust.json") {
                let modified = meta.as_ref().and_then(|m| m.modified().ok()).and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok()).map(|d| d.as_secs()).unwrap_or(0);
                let file = if rel.trim_matches('/').is_empty() { name.clone() } else { format!("{}/{name}", rel.trim_matches('/')) };
                files.push(serde_json::json!({ "name": name.trim_end_matches(".cadrust.json"), "file": file, "size": meta.map(|m| m.len()).unwrap_or(0), "modified": modified }));
            }
        }
    }
    dirs.sort();
    files.sort_by(|a, b| a["name"].as_str().cmp(&b["name"].as_str()));
    let served = st.path.strip_prefix(root(&st)).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    Json(serde_json::json!({ "path": rel.trim_matches('/'), "dirs": dirs, "files": files, "served": served })).into_response()
}

/// One document by relative path.
async fn fs_file(State(st): State<Arc<AppState>>, axum::extract::Query(q): Query) -> impl IntoResponse {
    let rel = q.get("path").cloned().unwrap_or_default();
    match safe_rel(&st, &rel).filter(|_| rel.ends_with(".cadrust.json")).and_then(|p| std::fs::read_to_string(p).ok()) {
        Some(text) => (StatusCode::OK, [(header::CONTENT_TYPE, "application/json".to_string())], text).into_response(),
        None => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Create a folder under the root (`{"path": "a/b"}`).
async fn fs_mkdir(State(st): State<Arc<AppState>>, Json(req): Json<serde_json::Value>) -> impl IntoResponse {
    let rel = req["path"].as_str().unwrap_or_default().to_string();
    let Some(dir) = safe_rel(&st, &rel).filter(|_| !rel.trim_matches('/').is_empty()) else { return (StatusCode::BAD_REQUEST, "bad path").into_response() };
    match std::fs::create_dir_all(&dir) {
        Ok(()) => Json(serde_json::json!({ "path": rel.trim_matches('/') })).into_response(),
        Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
    }
}

fn doc_stem(path: &Path) -> String {
    path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or_default().trim_end_matches(".json").trim_end_matches(".cadrust").to_string()
}

/// Designs next to the served document (the Data Panel lists them).
async fn list_examples(State(st): State<Arc<AppState>>) -> Json<serde_json::Value> {
    let dir = st.path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    let mut out = Vec::new();
    if let Ok(entries) = std::fs::read_dir(&dir) {
        let mut names: Vec<String> = entries.flatten().map(|e| e.file_name().to_string_lossy().to_string()).filter(|n| n.ends_with(".cadrust.json")).collect();
        names.sort();
        for n in names {
            out.push(serde_json::json!({ "name": n.trim_end_matches(".cadrust.json"), "file": n }));
        }
    }
    Json(serde_json::Value::Array(out))
}

async fn example_file(State(st): State<Arc<AppState>>, UrlPath(file): UrlPath<String>) -> impl IntoResponse {
    if file.contains("..") || file.contains('/') || !file.ends_with(".cadrust.json") {
        return (StatusCode::BAD_REQUEST, "bad path").into_response();
    }
    let dir = st.path.parent().map(Path::to_path_buf).unwrap_or_else(|| PathBuf::from("."));
    match std::fs::read_to_string(dir.join(&file)) {
        Ok(text) => (StatusCode::OK, [(header::CONTENT_TYPE, "application/json".to_string())], text).into_response(),
        Err(_) => (StatusCode::NOT_FOUND, "not found").into_response(),
    }
}

/// Persist the document posted by the browser. A named design is written next to the served
/// file as `<name>.cadrust.json`, so opening another design from the Data Panel never overwrites
/// the one given on the command line.
async fn save_doc(State(st): State<Arc<AppState>>, axum::extract::Query(q): Query, Json(doc): Json<Document>) -> impl IntoResponse {
    // `?path=sub/name.cadrust.json` (from the Save As dialog) wins; otherwise the design name decides.
    let target = match q.get("path").filter(|p| p.ends_with(".cadrust.json")) {
        Some(rel) => match safe_rel(&st, rel) {
            Some(p) => p,
            None => return (StatusCode::BAD_REQUEST, "bad path").into_response(),
        },
        None => match doc.name.as_deref().filter(|n| !n.is_empty() && !n.contains('/') && !n.contains("..")) {
            Some(n) if n != doc_stem(&st.path) => root(&st).join(format!("{n}.cadrust.json")),
            _ => st.path.clone(),
        },
    };
    if let Some(parent) = target.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let saved_rel = target.strip_prefix(root(&st)).map(|p| p.to_string_lossy().to_string()).unwrap_or_default();
    // Only a save of the served design replaces what `/api/doc` hands out; another design goes
    // to its own file and the page keeps working on it in memory.
    let served = target == st.path;
    match doc.to_json() {
        Ok(text) => match std::fs::write(&target, text) {
            Ok(()) => {
                if served {
                    *st.doc.lock().unwrap() = doc;
                }
                Json(serde_json::json!({ "saved": saved_rel })).into_response()
            }
            Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
        },
        Err(e) => (StatusCode::BAD_REQUEST, e.to_string()).into_response(),
    }
}

/// Evaluate and return a glb plus JSON status in a header-free envelope:
/// the body is glb; status and body info go in the `x-cadrust-status` header.
/// Graphviz DOT of the feature dependency graph.
async fn graph_doc(Json(doc): Json<Document>) -> impl IntoResponse {
    ([(header::CONTENT_TYPE, "text/vnd.graphviz")], cadrust_core::graph::dot(&doc))
}

async fn run_doc(State(st): State<Arc<AppState>>, Json(doc): Json<Document>) -> impl IntoResponse {
    let result = tokio::task::spawn_blocking(move || -> Result<(Vec<u8>, String), String> {
        let t0 = std::time::Instant::now();
        let mut cache = st.cache.lock().unwrap();
        let ev = cadrust_core::evaluate_with(&doc, &BTreeMap::new(), Some(&mut cache)).map_err(|e| e.to_string())?;
        let eval_ms = t0.elapsed().as_millis();
        let cache_len = cache.len();
        drop(cache);
        let mut glb = Vec::new();
        if !ev.bodies.is_empty() {
            Solid::mesh(ev.solids(), Default::default())
                .and_then(|m| m.write_gltf_binary(&mut glb))
                .map_err(|e| format!("mesh failed: {e:?}"))?;
        }
        let status = serde_json::json!({ "status": ev.status, "bodies": ev.info(), "edges": ev.edges(), "sketches": ev.sketches,
            "reused": ev.reused, "eval_ms": eval_ms, "cache_entries": cache_len }).to_string();
        Ok((glb, status))
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match result {
        Ok((glb, status)) => (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "model/gltf-binary".to_string()), ("x-cadrust-status".parse::<header::HeaderName>().unwrap(), status)],
            Bytes::from(glb),
        )
            .into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// Convert an uploaded STEP or BRep file (raw body; `?format=step|brep`) to the base64 BRep
/// text an `Import` feature carries. Returns `{"data": ..., "solids": n}`.
async fn import_file(axum::extract::Query(q): axum::extract::Query<std::collections::HashMap<String, String>>, body: Bytes) -> impl IntoResponse {
    let format = q.get("format").cloned().unwrap_or_else(|| "step".into());
    let result = tokio::task::spawn_blocking(move || import_to_brep(&format, &body)).await.unwrap_or_else(|e| Err(e.to_string()));
    match result {
        Ok((data, n)) => Json(serde_json::json!({ "data": data, "solids": n })).into_response(),
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

/// Shared by the server and the wasm build: read solids and re-encode them as base64 BRep.
pub fn import_to_brep(format: &str, bytes: &[u8]) -> Result<(String, usize), String> {
    let mut reader = bytes;
    let solids = match format {
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
    Ok((cadrust_core::b64::encode(&out), solids.len()))
}

#[derive(serde::Deserialize)]
struct ExportReq {
    doc: Document,
    format: String,
    /// Body ids to export; empty means all.
    #[serde(default)]
    bodies: Vec<String>,
}

async fn export_doc(State(st): State<Arc<AppState>>, axum::extract::Query(q): Query, Json(req): Json<ExportReq>) -> impl IntoResponse {
    let format = req.format.clone();
    let save_to = match q.get("path") {
        Some(rel) => match safe_rel(&st, rel).filter(|_| !rel.trim_matches('/').is_empty()) {
            Some(p) => Some(p),
            None => return (StatusCode::BAD_REQUEST, "bad path").into_response(),
        },
        None => None,
    };
    let result = tokio::task::spawn_blocking(move || -> Result<Vec<u8>, String> {
        let ev = evaluate(&req.doc, &BTreeMap::new()).map_err(|e| e.to_string())?;
        let mut out = Vec::new();
        let solids = ev.solids_of(&req.bodies);
        match req.format.as_str() {
            "step" => Solid::write_step(solids, &mut out).map_err(|e| format!("{e:?}"))?,
            "brep" => Solid::write_brep(solids, &mut out).map_err(|e| format!("{e:?}"))?,
            "stl" => Solid::mesh(solids, Default::default()).and_then(|m| m.write_stl(&mut out)).map_err(|e| format!("{e:?}"))?,
            "glb" => Solid::mesh(solids, Default::default()).and_then(|m| m.write_gltf_binary(&mut out)).map_err(|e| format!("{e:?}"))?,
            other => return Err(format!("unsupported format {other}")),
        }
        Ok(out)
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match result {
        Ok(bytes) => match save_to {
            Some(path) => {
                if let Some(parent) = path.parent() {
                    let _ = std::fs::create_dir_all(parent);
                }
                match std::fs::write(&path, &bytes) {
                    Ok(()) => Json(serde_json::json!({ "saved": path.strip_prefix(root(&st)).map(|p| p.to_string_lossy().to_string()).unwrap_or_default(), "bytes": bytes.len() })).into_response(),
                    Err(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()).into_response(),
                }
            }
            None => (
                StatusCode::OK,
                [(header::CONTENT_TYPE, "application/octet-stream".to_string()), (header::CONTENT_DISPOSITION, format!("attachment; filename=\"model.{format}\""))],
                Bytes::from(bytes),
            )
                .into_response(),
        },
        Err(e) => (StatusCode::BAD_REQUEST, e).into_response(),
    }
}

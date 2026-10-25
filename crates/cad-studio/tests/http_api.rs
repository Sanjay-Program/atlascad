//! The studio is tested through its real HTTP surface, not by calling its handlers directly.
//!
//! Every assertion is either an analytic geometric value or a protocol rule: a box's volume, a
//! circle's tessellation, a rejected foreign origin, a refused oversized body. A test that only
//! compared the server to itself would pass even if the viewport were blank.

use cad_studio::{default_project, start};
use serde_json::Value;
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::net::{SocketAddr, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("atlas-studio-test-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

struct Response {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

fn request(
    addr: SocketAddr,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<Response, Box<dyn std::error::Error>> {
    request_with_host(addr, &addr.to_string(), method, path, headers, body)
}

fn request_with_host(
    addr: SocketAddr,
    host: &str,
    method: &str,
    path: &str,
    headers: &[(&str, &str)],
    body: &[u8],
) -> Result<Response, Box<dyn std::error::Error>> {
    let mut stream = TcpStream::connect(addr)?;
    let mut head = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.len()
    );
    for (name, value) in headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    if !body.is_empty() {
        stream.write_all(body)?;
    }
    stream.flush()?;

    let mut bytes = Vec::new();
    stream.read_to_end(&mut bytes)?;
    let split = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or("response has no header terminator")?;
    let header_text = String::from_utf8_lossy(&bytes[..split]).into_owned();
    let mut lines = header_text.split("\r\n");
    let status_line = lines.next().ok_or("response has no status line")?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or("status line has no code")?
        .parse::<u16>()?;
    let mut parsed = BTreeMap::new();
    for line in lines {
        if let Some((name, value)) = line.split_once(':') {
            parsed.insert(name.trim().to_ascii_lowercase(), value.trim().to_string());
        }
    }
    Ok(Response {
        status,
        headers: parsed,
        body: bytes[split + 4..].to_vec(),
    })
}

fn json(response: &Response) -> Result<Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&response.body)?)
}

fn origin(addr: SocketAddr) -> String {
    format!("http://127.0.0.1:{}", addr.port())
}

fn post(addr: SocketAddr, path: &str, body: &str) -> Result<Response, Box<dyn std::error::Error>> {
    let origin = origin(addr);
    request(
        addr,
        "POST",
        path,
        &[("Origin", &origin), ("Content-Type", "application/json")],
        body.as_bytes(),
    )
}

fn fixture(
    tag: &str,
    size: [f64; 3],
) -> Result<(TempDir, PathBuf, cad_studio::StudioHandle), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join(format!("{tag}.zcad"));
    let project = default_project(tag, size)?;
    project.save(&path)?;
    let handle = start(project, path.clone(), 0)?;
    Ok((dir, path, handle))
}

#[test]
fn state_is_a_complete_renderable_payload() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("bracket", [10.0, 10.0, 5.0])?;
    let response = request(handle.addr(), "GET", "/api/state", &[], &[])?;
    assert_eq!(response.status, 200);
    assert_eq!(
        response.headers.get("content-type").map(String::as_str),
        Some("application/json; charset=utf-8")
    );
    let state = json(&response)?;
    assert_eq!(state["name"], "bracket");
    assert_eq!(state["schema"], 2);
    assert_eq!(state["file"], path.display().to_string());
    assert_eq!(state["units"], "mm");
    assert_eq!(state["dirty"], false);
    assert_eq!(state["geometry"]["triangles"], 12);
    assert!((state["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 500.0).abs() < 1e-6);
    assert_eq!(
        state["mesh"]["triangles"]
            .as_array()
            .ok_or("triangles")?
            .len(),
        12
    );
    assert_eq!(state["outline"].as_array().ok_or("outline")?.len(), 4);
    assert!(
        state["parameters"]
            .as_array()
            .ok_or("parameters")?
            .iter()
            .any(|parameter| parameter["id"] == "width")
    );

    let index = request(handle.addr(), "GET", "/", &[], &[])?;
    assert_eq!(index.status, 200);
    let html = String::from_utf8(index.body)?;
    assert!(html.contains("Atlas CAD"));
    assert!(html.contains("Command palette"));

    let script = request(handle.addr(), "GET", "/app.js", &[], &[])?;
    assert_eq!(script.status, 200);
    assert!(
        script
            .headers
            .get("content-type")
            .is_some_and(|value| value.starts_with("text/javascript"))
    );
    assert!(String::from_utf8(script.body)?.contains("WebGL"));
    Ok(())
}

#[test]
fn parameter_edit_undo_redo_save_and_stl_export_work() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("edit", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let edited = post(addr, "/api/parameter", r#"{"id":"width","value":20}"#)?;
    assert_eq!(edited.status, 200);
    let state = json(&edited)?;
    assert!((state["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 1000.0).abs() < 1e-6);
    assert_eq!(state["dirty"], true);
    assert!(
        state["history"]["undo"].as_u64().ok_or("undo")? > 0,
        "a parameter edit must be undoable"
    );

    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    let state = json(&undone)?;
    assert!((state["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 500.0).abs() < 1e-6);
    assert_eq!(state["history"]["redo"], 1);

    let redone = post(addr, "/api/redo", "{}")?;
    assert_eq!(redone.status, 200);
    assert!(
        (json(&redone)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 1000.0)
            .abs()
            < 1e-6
    );

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    let state = json(&saved)?;
    assert_eq!(state["dirty"], false);
    let reopened = cad_document::Project::load(&path)?;
    assert!((reopened.parameter("width")? - 20.0).abs() < 1e-12);

    let exported = request(addr, "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    assert_eq!(
        exported.headers.get("content-type").map(String::as_str),
        Some("model/stl")
    );
    assert!(
        exported
            .headers
            .get("content-disposition")
            .is_some_and(|value| value.contains("attachment"))
    );
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert_eq!(mesh.triangle_count(), 12);
    assert!((mesh.signed_volume().abs() - 1000.0).abs() < 1e-3);
    Ok(())
}

#[test]
fn profile_switch_rebuilds_circle_and_polygon_geometry() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("profiles", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let circle = post(addr, "/api/sketch/profile", r#"{"id":1,"kind":"circle"}"#)?;
    assert_eq!(circle.status, 200);
    let state = json(&circle)?;
    assert_eq!(state["sketches"][0]["profile"]["kind"], "circle");
    let segments = state["sketches"][0]["segments"]
        .as_u64()
        .ok_or("segments")? as usize;
    assert!((24..=256).contains(&segments));
    assert_eq!(
        state["outline"].as_array().ok_or("outline")?.len(),
        segments
    );
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    let exact = std::f64::consts::PI * 25.0 * 5.0;
    assert!(
        (volume - exact).abs() / exact < 0.02,
        "a 10 mm circle was {volume} against exact {exact}"
    );

    let polygon = post(
        addr,
        "/api/sketch/profile",
        r#"{"id":1,"kind":"regular_polygon","sides":6}"#,
    )?;
    assert_eq!(polygon.status, 200);
    let state = json(&polygon)?;
    assert_eq!(state["sketches"][0]["profile"]["kind"], "regular_polygon");
    assert_eq!(state["sketches"][0]["segments"], 6);
    let expected = 0.5 * 6.0 * 25.0 * (std::f64::consts::PI / 3.0).sin() * 5.0;
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - expected).abs() < 1e-6,
        "hexagon volume {volume} vs analytic {expected}"
    );

    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(json(&undone)?["sketches"][0]["profile"]["kind"], "circle");
    Ok(())
}

#[test]
fn a_persisted_project_reopens_with_its_profile_and_history()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("reopen", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();
    post(addr, "/api/sketch/profile", r#"{"id":1,"kind":"circle"}"#)?;
    post(addr, "/api/parameter", r#"{"id":"diameter","value":16}"#)?;
    post(addr, "/api/save", "{}")?;
    drop(handle);

    let project = cad_document::Project::load(&path)?;
    let reopened = start(project, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["sketches"][0]["profile"]["kind"], "circle");
    assert!((state["parameters"][3]["value"].as_f64().ok_or("diameter")? - 16.0).abs() < 1e-12);
    assert!(state["geometry"]["triangles"].as_u64().ok_or("triangles")? > 12);
    Ok(())
}

#[test]
fn a_foreign_or_empty_host_is_refused_before_routing() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("host", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let foreign = request_with_host(addr, "attacker.example", "GET", "/api/state", &[], &[])?;
    assert_eq!(foreign.status, 403);
    assert!(!String::from_utf8_lossy(&foreign.body).contains("volume_mm3"));

    let empty = request_with_host(addr, "", "GET", "/api/state", &[], &[])?;
    assert_eq!(empty.status, 403);

    let local = request_with_host(
        addr,
        &format!("localhost:{}", addr.port()),
        "GET",
        "/api/state",
        &[],
        &[],
    )?;
    assert_eq!(local.status, 200);
    Ok(())
}

#[test]
fn the_api_refuses_foreign_origins_media_types_and_oversized_bodies()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("security", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let missing_origin = request(
        addr,
        "POST",
        "/api/parameter",
        &[("Content-Type", "application/json")],
        br#"{"id":"width","value":20}"#,
    )?;
    assert_eq!(missing_origin.status, 403);

    let foreign = request(
        addr,
        "POST",
        "/api/parameter",
        &[
            ("Origin", "https://example.invalid"),
            ("Content-Type", "application/json"),
        ],
        br#"{"id":"width","value":20}"#,
    )?;
    assert_eq!(foreign.status, 403);

    let origin = origin(addr);
    let wrong_type = request(
        addr,
        "POST",
        "/api/parameter",
        &[("Origin", &origin), ("Content-Type", "text/plain")],
        br#"{"id":"width","value":20}"#,
    )?;
    assert_eq!(wrong_type.status, 415);

    let oversized = request(
        addr,
        "POST",
        "/api/parameter",
        &[
            ("Origin", &origin),
            ("Content-Type", "application/json"),
            ("Content-Length", "2097152"),
        ],
        &[],
    )?;
    assert_eq!(oversized.status, 413);

    let malformed = post(addr, "/api/parameter", "not json")?;
    assert_eq!(malformed.status, 400);
    let unknown_field = post(addr, "/api/parameter", r#"{"id":"width","value":20,"x":1}"#)?;
    assert_eq!(unknown_field.status, 400);

    let wrong_method = request(addr, "DELETE", "/api/state", &[], &[])?;
    assert_eq!(wrong_method.status, 405);

    let missing = request(addr, "GET", "/api/does-not-exist", &[], &[])?;
    assert_eq!(missing.status, 404);

    let negative = post(addr, "/api/parameter", r#"{"id":"width","value":-5}"#)?;
    assert_eq!(negative.status, 400);
    let error = json(&negative)?;
    assert_eq!(error["error"]["kind"], "invalid_project");

    Ok(())
}

#[test]
fn an_empty_project_reports_nothing_to_undo() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("empty.zcad");
    let project = cad_document::Project::new(
        "empty",
        cad_contract::ids::DocumentId::new(1),
        cad_contract::units::Unit::Millimeter,
    );
    project.save(&path)?;
    let handle = start(project, path, 0)?;
    let state = json(&request(handle.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["geometry"]["triangles"], 0);
    assert_eq!(
        state["mesh"]["triangles"]
            .as_array()
            .ok_or("triangles")?
            .len(),
        0
    );
    let no_undo = post(handle.addr(), "/api/undo", "{}")?;
    assert_eq!(no_undo.status, 409);
    assert_eq!(json(&no_undo)?["error"]["kind"], "nothing_to_undo");
    let no_export = request(handle.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(no_export.status, 400);
    Ok(())
}

#[test]
fn an_unknown_sketch_or_profile_is_a_clear_user_error() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("errors", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let unknown = post(addr, "/api/sketch/profile", r#"{"id":99,"kind":"circle"}"#)?;
    assert_eq!(unknown.status, 400);
    assert_eq!(json(&unknown)?["error"]["kind"], "unknown_sketch");

    let unknown_kind = post(addr, "/api/sketch/profile", r#"{"id":1,"kind":"teapot"}"#)?;
    assert_eq!(unknown_kind.status, 400);
    assert!(
        json(&unknown_kind)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown profile")
    );

    let bad_sides = post(
        addr,
        "/api/sketch/profile",
        r#"{"id":1,"kind":"regular_polygon","sides":2}"#,
    )?;
    assert_eq!(bad_sides.status, 400);
    assert!(
        json(&bad_sides)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("sides")
    );
    Ok(())
}

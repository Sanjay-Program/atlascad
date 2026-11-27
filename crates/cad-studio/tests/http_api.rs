//! The studio is tested through its real HTTP surface, not by calling its handlers directly.
//!
//! Every assertion is either an analytic geometric value or a protocol rule: a box's volume, a
//! circle's tessellation, a rejected foreign origin, a refused oversized body. A test that only
//! compared the server to itself would pass even if the viewport were blank.

#![allow(clippy::unwrap_used)]

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

fn get(addr: SocketAddr, path: &str) -> Result<Response, Box<dyn std::error::Error>> {
    let origin = origin(addr);
    request(addr, "GET", path, &[("Origin", &origin)], &[])
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
    assert_eq!(state["schema"], cad_document::PROJECT_SCHEMA);
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
    // The history is rendered from these fields: every sketch and feature, in build order, with the
    // operation and the rebuild outcome of each feature.
    assert_eq!(
        state["sketches"][0]["profile"]["kind"], "rectangle",
        "the tree names the profile of every sketch"
    );
    assert_eq!(state["features"][0]["operation"], "new body");
    assert_eq!(state["features"][0]["status"], "built");
    assert!(
        (state["features"][0]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 500.0)
            .abs()
            < 1e-6
    );
    assert!(state["failure"].is_null());
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
            // Twice the server's 16 MiB body cap; the declaration alone must be refused, without
            // waiting for the body to arrive.
            ("Content-Length", "33554432"),
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

/// The M11 workflow end to end: add a second sketch, move it so the two bodies overlap, join it into
/// the first through the real Truck kernel, then save and reopen the project.
#[test]
fn a_join_extrudes_a_second_sketch_into_the_body_and_survives_save_and_reopen()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("joined", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // The fixture is one 10 x 10 x 5 rectangle extruded as a new body.
    let initial = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    let volume = initial["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1e-6, "fixture volume {volume}");
    assert_eq!(initial["features"][0]["operation"], "new body");
    assert_eq!(initial["features"][0]["status"], "built");

    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"rectangle","origin":[0,0]}"#,
    )?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    let sketches = payload["sketches"].as_array().ok_or("sketches")?;
    assert_eq!(sketches.len(), 2);
    let second = sketches[1]["id"].as_u64().ok_or("second sketch id")?;
    assert_eq!(sketches[1]["profile"]["kind"], "rectangle");
    assert_eq!(sketches[1]["segments"], 4);
    // A new sketch is a document change, not a rebuild: the body is untouched.
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - 500.0).abs() < 1e-6,
        "volume after a sketch {volume}"
    );

    // Move the second sketch so the two 10 x 10 footprints overlap by 2 x 2 mm.
    let moved = post(
        addr,
        "/api/sketch/origin",
        &format!(r#"{{"id":{second},"origin":[8,8]}}"#),
    )?;
    assert_eq!(moved.status, 200);
    let payload = json(&moved)?;
    assert_eq!(
        payload["sketches"][1]["origin"],
        serde_json::json!([8.0, 8.0])
    );

    // The body already exists, so a second new body is refused and says so.
    let second_body = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{second},"operation":"new_body"}}"#),
    )?;
    assert_eq!(second_body.status, 400);
    assert!(
        json(&second_body)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("join")
    );

    let joined = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{second},"operation":"join"}}"#),
    )?;
    assert_eq!(joined.status, 200);
    let payload = json(&joined)?;
    // Two 10 x 10 x 5 boxes overlapping 2 x 2 over their full height: 500 + 500 - 2*2*5 = 980 mm³.
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 980.0).abs() < 1.0, "joined volume {volume}");
    let features = payload["features"].as_array().ok_or("features")?;
    assert_eq!(features.len(), 2);
    assert_eq!(features[0]["operation"], "new body");
    assert_eq!(features[0]["status"], "built");
    assert_eq!(features[1]["operation"], "join");
    assert_eq!(features[1]["status"], "built");
    assert_eq!(features[1]["sketch_id"].as_u64(), Some(second));
    assert!(features[1]["message"].is_null());
    assert!(payload["failure"].is_null());
    // The joined body spans both footprints.
    for (axis, expected) in [(0, 18.0), (1, 18.0), (2, 5.0)] {
        let max = payload["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        assert!((max - expected).abs() < 1e-3, "bound {axis} was {max}");
    }

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);

    // Reopen from the file: the ordered history and the joined solid come back through the kernel.
    let reopened = cad_document::Project::load(&path)?;
    assert_eq!(reopened.sketches().len(), 2);
    assert_eq!(reopened.features().len(), 2);
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 980.0).abs() < 1.0, "reopened volume {volume}");
    assert_eq!(state["features"][1]["operation"], "join");
    assert_eq!(state["features"][1]["status"], "built");
    assert_eq!(
        state["sketches"][1]["origin"],
        serde_json::json!([8.0, 8.0])
    );

    // The export is the joined solid, not a placeholder mesh.
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert!(
        (mesh.signed_volume().abs() - 980.0).abs() < 1.0,
        "exported volume {}",
        mesh.signed_volume()
    );
    Ok(())
}

/// A feature that cannot rebuild is a reported result, not a lost model: the last good mesh stays,
/// the feature is named with the kernel's message, export is refused, and undo removes the feature.
#[test]
fn a_feature_that_cannot_rebuild_is_reported_and_keeps_the_last_good_mesh()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("unbuildable", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"rectangle","origin":[0,0]}"#,
    )?;
    let payload = json(&added)?;
    let second = payload["sketches"][1]["id"].as_u64().ok_or("id")?;

    // 1e9 mm from the origin is finite, so the document accepts it, but Truck cannot attach a plane
    // to the resulting wire and the extrude fails. That measured limit (docs/kernel-comparison.md)
    // is the one reachable way to make a feature fail through this API, and it drives exactly the
    // path the UI has to show.
    let moved = post(
        addr,
        "/api/sketch/origin",
        &format!(r#"{{"id":{second},"origin":[1000000000,1000000000]}}"#),
    )?;
    assert_eq!(moved.status, 200);

    let joined = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{second},"operation":"join"}}"#),
    )?;
    // Accepted: the feature belongs to the history and its failure is part of the payload.
    assert_eq!(joined.status, 200);
    let payload = json(&joined)?;
    let features = payload["features"].as_array().ok_or("features")?;
    assert_eq!(features.len(), 2);
    assert_eq!(features[0]["status"], "built");
    assert_eq!(features[1]["status"], "failed");
    assert!(features[1]["volume_mm3"].is_null());
    let message = features[1]["message"].as_str().ok_or("message")?;
    assert!(message.contains("extrude failed"), "{message}");
    assert_eq!(
        payload["failure"]["feature"].as_u64(),
        features[1]["id"].as_u64()
    );

    // The geometry is the last feature that rebuilt: the 10 x 10 x 5 body, not the failed join.
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1e-6, "kept volume {volume}");

    // An export would not match the model, so it is refused and names the feature.
    let refused = request(addr, "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(refused.status, 400);
    assert!(
        json(&refused)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot be rebuilt")
    );

    // Undo removes the failing feature and the rebuild is clean again.
    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    let payload = json(&undone)?;
    assert_eq!(payload["features"].as_array().ok_or("features")?.len(), 1);
    assert!(payload["failure"].is_null());
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1e-6, "volume after undo {volume}");
    Ok(())
}

/// The creation endpoints validate their inputs the way the rest of the API does.
#[test]
fn the_creation_endpoints_refuse_impossible_requests() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("create.zcad");
    let mut project = cad_document::Project::new(
        "create",
        cad_contract::ids::DocumentId::new(1),
        cad_contract::units::Unit::Millimeter,
    );
    // Give the project a body so that `cut` is a meaningful request rather than a "no body" error.
    let sketch_id = project.add_rectangle_sketch("width", "height", [0.0, 0.0])?;
    project.add_extrude(sketch_id, "depth")?;
    project.save(&path)?;
    let sketch_id = project
        .sketches()
        .last()
        .ok_or("the fixture sketch must exist")?
        .id
        .get();
    let handle = start(project.clone(), path, 0)?;
    let addr = handle.addr();

    let unknown_shape = post(addr, "/api/sketch", r#"{"kind":"teapot"}"#)?;
    assert_eq!(unknown_shape.status, 400);
    assert!(
        json(&unknown_shape)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown profile")
    );

    let missing_sides = post(addr, "/api/sketch", r#"{"kind":"regular_polygon"}"#)?;
    assert_eq!(missing_sides.status, 400);

    let extra_field = post(addr, "/api/sketch", r#"{"kind":"circle","radius":4}"#)?;
    assert_eq!(extra_field.status, 400);
    assert_eq!(json(&extra_field)?["error"]["kind"], "invalid_json");

    // The kernel now advertises boolean difference, so the UI offers Cut and the list must say so:
    // the control is driven by this capability list, not by a hard-coded assumption.
    let before_cut = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    assert!(
        before_cut["kernel"]["capabilities"]
            .as_array()
            .ok_or("capabilities")?
            .iter()
            .any(|capability| capability.as_str() == Some("boolean difference")),
        "the UI decides whether to offer Cut from this list, so it must claim a verified difference"
    );

    // A cut of the body by its own sketch removes everything. The request is well-formed history,
    // so it is accepted; the rebuild reports by feature ID whatever the kernel says, and the failure
    // must never be a silently returned sliver.
    let cut = post(
        addr,
        "/api/feature/extrude",
        &format!(
            r#"{{"sketch_id":{}, "operation":"cut", "depth_param":"depth"}}"#,
            sketch_id
        ),
    )?;
    assert_eq!(cut.status, 200, "{}", String::from_utf8_lossy(&cut.body));
    let state = json(&cut)?;
    let features = state["features"].as_array().ok_or("features")?;
    assert_eq!(features.len(), 2);
    assert_eq!(features[1]["operation"], "cut");
    assert_eq!(
        features[1]["status"], "failed",
        "a cut that removes the whole body must be reported, not built: {state}"
    );
    assert_eq!(state["failure"]["feature"], features[1]["id"]);

    // An unknown operation is still refused as a bad request.
    let unknown_operation = post(
        addr,
        "/api/feature/extrude",
        &format!(
            r#"{{"sketch_id":{}, "operation":"slice", "depth_param":"depth"}}"#,
            sketch_id
        ),
    )?;
    assert_eq!(unknown_operation.status, 400);
    assert!(
        json(&unknown_operation)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("unknown operation")
    );

    // A cut before any body exists is refused with a "body" reason, not "unknown operation".
    let mut empty_project = cad_document::Project::new(
        "empty-cut",
        cad_contract::ids::DocumentId::new(2),
        cad_contract::units::Unit::Millimeter,
    );
    let empty_sketch = empty_project.add_rectangle_sketch("width", "height", [0.0, 0.0])?;
    let empty_path = dir.path().join("empty.zcad");
    empty_project.save(&empty_path)?;
    let empty_handle = start(empty_project.clone(), empty_path, 0)?;
    let empty_addr = empty_handle.addr();
    // A cut or join before any body exists is refused with a "body" reason, not "unknown
    // operation": the request is well-formed and the history is the problem.
    let empty_sketch_id = empty_sketch.get();
    for op_name in ["cut", "join"] {
        let refused = post(
            empty_addr,
            "/api/feature/extrude",
            &format!(
                r#"{{"sketch_id":{}, "operation":"{}", "depth_param":"depth"}}"#,
                empty_sketch_id, op_name
            ),
        )?;
        assert_eq!(refused.status, 400);
        let message = json(&refused)?
            .get("error")
            .and_then(|error| error.get("message"))
            .and_then(serde_json::Value::as_str)
            .unwrap_or_default()
            .to_owned();
        assert!(
            message.contains("body"),
            "expected a body reason for {op_name}: {message:?}"
        );
    }

    let unknown_sketch = post(
        addr,
        "/api/feature/extrude",
        r#"{"sketch_id":7,"operation":"new_body"}"#,
    )?;
    assert_eq!(unknown_sketch.status, 400);
    assert_eq!(json(&unknown_sketch)?["error"]["kind"], "unknown_sketch");

    let added = post(addr, "/api/sketch", r#"{"kind":"circle","origin":[0,0]}"#)?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    // The fixture already has the rectangle that starts the body; the new sketch is the newest one.
    let sketches = payload["sketches"].as_array().ok_or("sketches")?;
    let newest = sketches.last().ok_or("a sketch was added")?;
    assert_eq!(newest["profile"]["kind"], "circle");
    assert_eq!(newest["profile"]["diameter_param"], "diameter");

    // A document carries one body, so a second `new_body` is refused rather than silently
    // replacing the first.
    let newest_id = newest["id"].as_u64().ok_or("newest sketch id")?;
    let second_body = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{newest_id}, "operation":"new_body", "depth_param":"depth"}}"#),
    )?;
    assert_eq!(second_body.status, 400);
    assert!(
        json(&second_body)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("already has a body")
    );

    // An unknown depth parameter is refused by the document before the kernel is asked to build.
    let unknown_depth = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{newest_id}, "operation":"join", "depth_param":"thickness"}}"#),
    )?;
    assert_eq!(unknown_depth.status, 400);
    assert_eq!(json(&unknown_depth)?["error"]["kind"], "unknown_parameter");
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

    // A polyline without a loop, and a loop sent with a parameter-driven shape, are both refused.
    let no_points = post(addr, "/api/sketch", r#"{"kind":"polyline"}"#)?;
    assert_eq!(no_points.status, 400);
    assert!(
        json(&no_points)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("points")
    );

    let stray_points = post(
        addr,
        "/api/sketch",
        r#"{"kind":"rectangle","points":[[0,0],[10,0],[10,10]]}"#,
    )?;
    assert_eq!(stray_points.status, 400);
    assert!(
        json(&stray_points)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("only valid for a polyline")
    );
    Ok(())
}

/// The drawn-polyline workflow end to end on the real Truck kernel: a concave L-shaped loop created
/// through the API as a new body, edited by replacing the loop, undone, saved, reopened, exported.
#[test]
fn a_drawn_polyline_extrudes_saves_reopens_and_exports() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("drawn.zcad");
    let project = cad_document::Project::new(
        "drawn",
        cad_contract::ids::DocumentId::new(1),
        cad_contract::units::Unit::Millimeter,
    );
    project.save(&path)?;
    let handle = start(project, path.clone(), 0)?;
    let addr = handle.addr();

    // The L-shaped loop: 30 x 30 footprint with a 20 x 20 corner removed, area 500 mm².
    let l_profile = "[[0,0],[30,0],[30,10],[10,10],[10,30],[0,30]]";
    let added = post(
        addr,
        "/api/sketch",
        &format!(r#"{{"kind":"polyline","origin":[0,0],"points":{l_profile}}}"#),
    )?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    let sketch = payload["sketches"][0].clone();
    let sketch_id = sketch["id"].as_u64().ok_or("sketch id")?;
    assert_eq!(sketch["profile"]["kind"], "polyline");
    assert_eq!(sketch["segments"], 6);
    assert_eq!(
        sketch["profile"]["points"]
            .as_array()
            .ok_or("points")?
            .len(),
        6
    );
    // A sketch alone has no solid: the mesh is empty until a feature consumes it.
    assert_eq!(payload["geometry"]["triangles"], 0);

    // A two-point loop is refused before it becomes a sketch.
    let too_few = post(
        addr,
        "/api/sketch",
        r#"{"kind":"polyline","points":[[0,0],[10,0]]}"#,
    )?;
    assert_eq!(too_few.status, 400);
    assert!(
        json(&too_few)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("between 3 and")
    );

    let body = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{sketch_id},"operation":"new_body","depth_param":"depth"}}"#),
    )?;
    assert_eq!(body.status, 200);
    let payload = json(&body)?;
    // 500 mm² x 5 mm = 2500 mm³, the analytic volume of the L-shaped prism.
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 2500.0).abs() < 1.0, "drawn volume {volume}");
    assert_eq!(payload["features"][0]["status"], "built");
    assert_eq!(payload["features"][0]["operation"], "new body");
    assert!(payload["failure"].is_null());
    for (axis, expected) in [(0, 30.0), (1, 30.0), (2, 5.0)] {
        let max = payload["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        assert!((max - expected).abs() < 1e-3, "bound {axis} was {max}");
    }

    // Replacing the loop with a 10 x 10 square rebuilds the feature to 100 x 5 = 500 mm³.
    let smaller = post(
        addr,
        "/api/sketch/profile",
        &format!(
            r#"{{"id":{sketch_id},"kind":"polyline","points":[[0,0],[10,0],[10,10],[0,10]]}}"#
        ),
    )?;
    assert_eq!(smaller.status, 200);
    let payload = json(&smaller)?;
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1.0, "edited volume {volume}");
    assert_eq!(payload["sketches"][0]["segments"], 4);

    // Undo restores the L-shaped body and its loop together.
    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    let payload = json(&undone)?;
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 2500.0).abs() < 1.0, "volume after undo {volume}");
    assert_eq!(payload["sketches"][0]["segments"], 6);

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);

    // Reopen from disk: schema 4, the drawn loop, and the same analytic volume.
    let reopened = cad_document::Project::load(&path)?;
    assert_eq!(reopened.schema, cad_document::PROJECT_SCHEMA);
    let mut kernel = cad_kernel_truck::TruckKernel::new();
    let build = reopened.build(&mut kernel)?;
    assert!(build.is_complete());
    let reopened_volume = build.mesh.ok_or("reopened mesh")?.signed_volume();
    assert!(
        (reopened_volume - 2500.0).abs() < 1.0,
        "reopened {reopened_volume}"
    );
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["sketches"][0]["profile"]["kind"], "polyline");
    assert_eq!(
        state["sketches"][0]["profile"]["points"]
            .as_array()
            .ok_or("points")?
            .len(),
        6
    );
    assert!(
        state["geometry"]["triangles"].as_u64().unwrap_or(0) > 0,
        "the reopened drawn body must tessellate"
    );

    // The export is the drawn solid, not a placeholder mesh.
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert!(
        (mesh.signed_volume().abs() - 2500.0).abs() < 1.0,
        "exported volume {}",
        mesh.signed_volume()
    );
    Ok(())
}

/// A cut removes exactly the analytic volume it should, and the whole history survives save,
/// reopen, export and undo.
///
/// The numbers are the point of this test. The cut's depth equals the body's thickness, so the
/// tool's top and bottom faces are coincident with the body's — the case that made every earlier
/// difference measurement wrong. The document builds the tool with `CUT_OVERSHOOT_MM` of extra
/// length, so the assertion is the analytic 255 mm³ to a thousandth of a millimetre, not "close".
#[test]
fn a_cut_removes_the_analytic_volume_and_survives_save_reopen_and_undo()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("cut", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // The fixture is one 10 x 10 x 5 rectangle extruded as a new body: 500 mm³.
    let initial = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    let volume = initial["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1e-6, "fixture volume {volume}");

    // A second 10 x 10 sketch, moved so it covers x,y in 3..13: 7 x 7 of it lies inside the body.
    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"rectangle","origin":[3,3]}"#,
    )?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    let second = payload["sketches"][1]["id"]
        .as_u64()
        .ok_or("second sketch id")?;

    let cut = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{second},"operation":"cut"}}"#),
    )?;
    assert_eq!(cut.status, 200, "{}", String::from_utf8_lossy(&cut.body));
    let payload = json(&cut)?;
    // 500 mm³ less a 7 x 7 x 5 through-cut (245 mm³) = 255 mm³.
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - 255.0).abs() < 1e-3,
        "cut volume {volume}, analytic 255"
    );
    let features = payload["features"].as_array().ok_or("features")?;
    assert_eq!(features.len(), 2);
    assert_eq!(features[1]["operation"], "cut");
    assert_eq!(features[1]["status"], "built");
    assert!(features[1]["message"].is_null());
    assert!(payload["failure"].is_null());
    // The body is the same block: a cut cannot move or grow it.
    for (axis, expected) in [(0, 10.0), (1, 10.0), (2, 5.0)] {
        let max = payload["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        let min = payload["bounds"]["min"][axis].as_f64().ok_or("bound")?;
        assert!((max - expected).abs() < 1e-3, "max {axis} was {max}");
        assert!(min.abs() < 1e-3, "min {axis} was {min}");
    }

    // Undo removes the cut feature and its geometry together, leaving the original block.
    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    let payload = json(&undone)?;
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1e-6, "volume after undo {volume}");
    assert_eq!(payload["features"].as_array().map(Vec::len), Some(1));

    // Redo puts it back, then save and reopen from disk: history and solid both return.
    let redone = post(addr, "/api/redo", "{}")?;
    assert_eq!(redone.status, 200);
    let volume = json(&redone)?["geometry"]["volume_mm3"]
        .as_f64()
        .ok_or("volume")?;
    assert!((volume - 255.0).abs() < 1e-3, "volume after redo {volume}");

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);

    let reopened = cad_document::Project::load(&path)?;
    assert_eq!(reopened.features().len(), 2);
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - 255.0).abs() < 1e-3,
        "reopened cut volume {volume}"
    );
    assert_eq!(state["features"][1]["operation"], "cut");
    assert_eq!(state["features"][1]["status"], "built");

    // The export is the cut solid, not a placeholder mesh, and it is watertight: a through-cut
    // whose tessellation had open edges would sail through a volume-only assertion.
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert!(
        (mesh.signed_volume().abs() - 255.0).abs() < 1e-3,
        "exported volume {}",
        mesh.signed_volume()
    );
    assert_eq!(
        welded_open_edges(&mesh),
        0,
        "an exported solid must be a closed manifold"
    );
    Ok(())
}

/// A cut that stops below the body's top face is a failed feature, not a wrong solid.
///
/// The cut construction is measured exact only when the tool clears the top. On the real truck
/// kernel a contained tool returned the tool itself (0.776 mm³ against 499.215 analytic) and a tool
/// from an interior plane down past the bottom returned the removed chunk (62.117 mm³ against the
/// remaining 737.17), both reported as `built`. The guard refuses the feature with the tool end and
/// the body top and leaves the body untouched, so a wrong solid can never be saved or exported.
#[test]
fn a_cut_that_does_not_pass_through_the_body_is_refused_not_built()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("cut-pierce", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // A 10 x 10 sketch on the z = 2 plane, swept down by the 5 mm depth parameter: the tool spans
    // z -3..2, so it starts inside the 0..5 body and stops inside it.
    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"rectangle","origin":[0,0],"plane":2}"#,
    )?;
    assert_eq!(
        added.status,
        200,
        "{}",
        String::from_utf8_lossy(&added.body)
    );
    let added = json(&added)?;
    let second = added["sketches"][1]["id"]
        .as_u64()
        .ok_or("second sketch id")?;

    let cut = post(
        addr,
        "/api/feature/extrude",
        &format!(
            r#"{{"sketch_id":{second},"operation":"cut","depth_param":"depth","direction":"down"}}"#
        ),
    )?;
    assert_eq!(cut.status, 200, "{}", String::from_utf8_lossy(&cut.body));
    let state = json(&cut)?;

    let features = state["features"].as_array().ok_or("features")?;
    assert_eq!(features.len(), 2);
    assert_eq!(features[0]["status"], "built");
    assert_eq!(
        features[1]["status"], "failed",
        "a non-piercing cut must fail instead of returning a wrong solid: {state}"
    );
    let message = features[1]["message"].as_str().unwrap_or_default();
    assert!(
        message.contains("must clear the body's top face"),
        "the failure names the rule: {message}"
    );
    assert!(
        message.contains("2.000") && message.contains("5.000"),
        "the failure names the tool end and the body top: {message}"
    );
    assert_eq!(state["failure"]["feature"], features[1]["id"]);

    // The body is untouched, so the refused feature cannot be saved as a smaller/wrong solid.
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - 500.0).abs() < 1e-6,
        "volume after the refused cut: {volume}"
    );
    Ok(())
}

/// The arc-based sector profile through the real Truck kernel: an arc sketch cuts a pocket out of a
/// plate, and the result is measured against the analytic area of the polygon the arc resolves to.
///
/// This is the M11e regression: the sector is the new sketch entity, and using it as a **cut** tool
/// proves it reaches the boolean path and not only the extrude path. The tool is a quarter sector of
/// a 5 mm circle placed at the plate's centre, so none of its faces are coincident with the plate's
/// — the boolean case the kernel handles worst is deliberately avoided here, because this test is
/// about the profile, not about coincidence.
#[test]
fn an_arc_sector_cuts_its_analytic_volume_and_survives_save_reopen_and_undo()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("sector", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // The sector is driven by the `diameter` parameter, which the fixture leaves at its 10 mm
    // default. Shrink it to 5 mm so the quarter sector is radius 2.5 at the plate's centre and no
    // face of the tool is coincident with the plate.
    let resized = post(addr, "/api/parameter", r#"{"id":"diameter","value":5}"#)?;
    assert_eq!(resized.status, 200);

    // A quarter sector of that 5 mm circle: radius 2.5, 0° to 90°.
    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"sector","origin":[5,5],"start_degrees":0,"sweep_degrees":90}"#,
    )?;
    assert_eq!(
        added.status,
        200,
        "{}",
        String::from_utf8_lossy(&added.body)
    );
    let payload = json(&added)?;
    let sketch = payload["sketches"][1].clone();
    let sketch_id = sketch["id"].as_u64().ok_or("sector sketch id")?;
    assert_eq!(sketch["profile"]["kind"], "sector");
    assert_eq!(sketch["profile"]["start_degrees"], 0.0);
    assert_eq!(sketch["profile"]["sweep_degrees"], 90.0);
    let segments = sketch["segments"].as_u64().ok_or("segments")?;
    // One centre point plus the arc: a quarter turn of a tolerance-bounded circle is at least two
    // facets, and the whole lens stays bounded.
    assert!((4..=258).contains(&segments), "segments {segments}");

    // Both impossible sweeps are refused with a reason that names the alternative.
    let full_turn = post(
        addr,
        "/api/sketch",
        r#"{"kind":"sector","sweep_degrees":360}"#,
    )?;
    assert_eq!(full_turn.status, 400);
    assert!(
        json(&full_turn)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("circle"),
        "a full turn must point at the circle profile"
    );
    let empty = post(
        addr,
        "/api/sketch",
        r#"{"kind":"sector","sweep_degrees":0}"#,
    )?;
    assert_eq!(empty.status, 400);

    let cut = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{sketch_id},"operation":"cut"}}"#),
    )?;
    assert_eq!(cut.status, 200, "{}", String::from_utf8_lossy(&cut.body));
    let payload = json(&cut)?;
    // The arc resolves to an inscribed polygon, so the removed volume is the shoelace area of that
    // polygon x the 5 mm depth. Asserting that exact value (not "about a quarter disc") is what ties
    // the number to the geometry the document actually handed the kernel.
    let facets = (segments - 2) as f64;
    let radius = 2.5;
    let sector_area = 0.5 * facets * radius * radius * (std::f64::consts::FRAC_PI_2 / facets).sin();
    let analytic = 500.0 - sector_area * 5.0;
    // 2%, not 1%: at a 2.5 mm radius the full circle hits the 24-facet floor, so a quarter turn is
    // only six chords and its inscribed area is ~1.1% under the true quarter disc. The exact
    // assertion is the polygon value below; this one only says the arc is a real arc.
    assert!(
        (sector_area - std::f64::consts::PI * radius * radius / 4.0).abs()
            / (std::f64::consts::PI * radius * radius / 4.0)
            < 0.02,
        "the polygon area {sector_area} is not within 2% of the quarter disc"
    );
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "sector cut volume {volume}, analytic {analytic}"
    );
    assert_eq!(payload["features"][1]["operation"], "cut");
    assert_eq!(payload["features"][1]["status"], "built");
    assert!(payload["failure"].is_null());
    // A pocket does not move the plate's outer bounds.
    for (axis, expected) in [(0, 10.0), (1, 10.0), (2, 5.0)] {
        let max = payload["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        assert!((max - expected).abs() < 1e-3, "bound {axis} was {max}");
    }

    // Undo removes the sector cut, redo restores the same measurement.
    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    assert!(
        (json(&undone)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 500.0)
            .abs()
            < 1e-6
    );
    let redone = post(addr, "/api/redo", "{}")?;
    assert_eq!(redone.status, 200);
    let volume = json(&redone)?["geometry"]["volume_mm3"]
        .as_f64()
        .ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "volume after redo {volume}"
    );

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);

    let reopened = cad_document::Project::load(&path)?;
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["sketches"][1]["profile"]["kind"], "sector");
    assert_eq!(state["sketches"][1]["profile"]["start_degrees"], 0.0);
    assert_eq!(state["sketches"][1]["profile"]["sweep_degrees"], 90.0);
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "reopened sector cut volume {volume}, analytic {analytic}"
    );

    // The export is the cut solid and it is watertight: a pocket whose tessellation had open edges
    // would pass a volume-only assertion.
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert!(
        (mesh.signed_volume().abs() - analytic).abs() < 1e-2,
        "exported volume {}",
        mesh.signed_volume()
    );
    assert_eq!(
        welded_open_edges(&mesh),
        0,
        "an exported solid must be a closed manifold"
    );
    Ok(())
}

/// The area of the polygon a circle sketch resolves to: `facets` chords at `radius`.
///
/// The document tessellates a circle at the document tolerance, so the analytic volume of a pocket or
/// a boss is the polygon's, not the disc's. Asserting the polygon value ties the number to the
/// geometry the document actually handed the kernel; the disc comparison below only says the
/// polygon is a circle approximation.
fn polygon_area(facets: u64, radius: f64) -> f64 {
    0.5 * facets as f64 * radius * radius * (std::f64::consts::TAU / facets as f64).sin()
}

/// A pocket on a raised sketch plane: the sketch sits inside the plate and its tool sweeps up through
/// the top face, so the plate loses exactly the swept volume and keeps its outer bounds.
#[test]
fn a_blind_pocket_from_a_raised_plane_removes_its_analytic_volume()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("pocket", [20.0, 20.0, 5.0])?;
    let addr = handle.addr();

    let initial = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    let volume = initial["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 2000.0).abs() < 1e-6, "fixture volume {volume}");

    // A 10 mm circle at the plate's centre, on the plane z = 3, swept up 5 mm: the tool spans
    // z = 3..8, so it opens through the z = 5 top face and leaves a 2 mm deep pocket.
    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"circle","origin":[10,10],"plane":3}"#,
    )?;
    assert_eq!(
        added.status,
        200,
        "{}",
        String::from_utf8_lossy(&added.body)
    );
    let payload = json(&added)?;
    let sketch = &payload["sketches"][1];
    assert_eq!(sketch["plane_z"], 3.0);
    let sketch_id = sketch["id"].as_u64().ok_or("sketch id")?;
    let facets = sketch["segments"].as_u64().ok_or("segments")?;
    assert!(
        facets >= 24,
        "a 10 mm circle needs at least the 24-facet floor"
    );

    let cut = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{sketch_id},"operation":"cut"}}"#),
    )?;
    assert_eq!(cut.status, 200, "{}", String::from_utf8_lossy(&cut.body));
    let payload = json(&cut)?;
    let analytic = 2000.0 - polygon_area(facets, 5.0) * 2.0;
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "pocket volume {volume}, analytic {analytic}"
    );
    assert_eq!(payload["features"][1]["operation"], "cut");
    assert_eq!(payload["features"][1]["direction"], "up");
    assert_eq!(payload["features"][1]["status"], "built");
    assert!(payload["failure"].is_null());
    // The pocket is blind: the plate is still 20 x 20 x 5, it did not grow or shrink.
    for (axis, expected) in [(0, 20.0), (1, 20.0), (2, 5.0)] {
        let max = payload["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        assert!((max - expected).abs() < 1e-3, "max {axis} was {max}");
        let min = payload["bounds"]["min"][axis].as_f64().ok_or("bound")?;
        assert!(min.abs() < 1e-3, "min {axis} was {min}");
    }

    // Undo removes the pocket and its feature together; redo restores the same measurement.
    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    assert!(
        (json(&undone)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 2000.0)
            .abs()
            < 1e-6
    );
    let redone = post(addr, "/api/redo", "{}")?;
    assert_eq!(redone.status, 200);
    let volume = json(&redone)?["geometry"]["volume_mm3"]
        .as_f64()
        .ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "volume after redo {volume}"
    );

    // The plane survives save and reopen, and the exported solid is watertight.
    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);
    let reopened = cad_document::Project::load(&path)?;
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["sketches"][1]["plane_z"], 3.0);
    assert_eq!(state["features"][1]["direction"], "up");
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "reopened pocket volume {volume}, analytic {analytic}"
    );
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert_eq!(
        welded_open_edges(&mesh),
        0,
        "a pocketed solid must export as a closed manifold"
    );
    Ok(())
}

/// A boss on the top face: the sketch sits on z = 5 and sweeps up, so the body gains exactly the
/// swept volume. This is the case that shares a face with the body, and it must be exact.
#[test]
fn a_boss_on_the_top_face_is_joined_exactly() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("boss", [20.0, 20.0, 5.0])?;
    let addr = handle.addr();

    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"circle","origin":[10,10],"plane":5}"#,
    )?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    assert_eq!(payload["sketches"][1]["plane_z"], 5.0);
    let sketch_id = payload["sketches"][1]["id"].as_u64().ok_or("id")?;
    let facets = payload["sketches"][1]["segments"]
        .as_u64()
        .ok_or("segments")?;

    let joined = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{sketch_id},"operation":"join"}}"#),
    )?;
    assert_eq!(
        joined.status,
        200,
        "{}",
        String::from_utf8_lossy(&joined.body)
    );
    let payload = json(&joined)?;
    let analytic = 2000.0 + polygon_area(facets, 5.0) * 5.0;
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "boss volume {volume}, analytic {analytic}"
    );
    assert_eq!(payload["features"][1]["operation"], "join");
    assert_eq!(payload["features"][1]["status"], "built");
    assert!(payload["failure"].is_null());
    // The boss is 5 mm tall on a 5 mm plate.
    let max_z = payload["bounds"]["max"][2].as_f64().ok_or("z")?;
    assert!((max_z - 10.0).abs() < 1e-3, "top of the boss was {max_z}");

    // Save and reopen preserves the plane and the measurement, and the export is watertight.
    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);
    let reopened = cad_document::Project::load(&path)?;
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["sketches"][1]["plane_z"], 5.0);
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "reopened boss volume {volume}, analytic {analytic}"
    );
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert!(
        (mesh.signed_volume().abs() - analytic).abs() < 1e-2,
        "exported volume {}",
        mesh.signed_volume()
    );
    assert_eq!(welded_open_edges(&mesh), 0, "a boss must export watertight");
    Ok(())
}

/// A through-hole drilled down from the top face: the sketch is placed on z = 5, sweeps down the
/// full 5 mm plate, so both of the tool's faces are coincident with the body's and the cut has to be
/// exact rather than a hairline membrane.
#[test]
fn a_through_hole_swept_down_from_the_top_face_is_exact() -> Result<(), Box<dyn std::error::Error>>
{
    let (_dir, _path, handle) = fixture("through", [20.0, 20.0, 5.0])?;
    let addr = handle.addr();

    let added = post(
        addr,
        "/api/sketch",
        r#"{"kind":"circle","origin":[10,10],"plane":5}"#,
    )?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    let sketch_id = payload["sketches"][1]["id"].as_u64().ok_or("id")?;
    let facets = payload["sketches"][1]["segments"]
        .as_u64()
        .ok_or("segments")?;

    let cut = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{sketch_id},"operation":"cut","direction":"down"}}"#),
    )?;
    assert_eq!(cut.status, 200, "{}", String::from_utf8_lossy(&cut.body));
    let payload = json(&cut)?;
    assert_eq!(payload["features"][1]["direction"], "down");
    assert_eq!(payload["features"][1]["status"], "built");
    let analytic = 2000.0 - polygon_area(facets, 5.0) * 5.0;
    let volume = payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (volume - analytic).abs() < 1e-2,
        "through-hole volume {volume}, analytic {analytic}"
    );
    // A hole does not move the plate.
    for (axis, expected) in [(0, 20.0), (1, 20.0), (2, 5.0)] {
        let max = payload["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        assert!((max - expected).abs() < 1e-3, "max {axis} was {max}");
    }

    // Bad placement and direction inputs are refused by name rather than silently defaulted.
    let bad_plane = post(
        addr,
        "/api/sketch",
        r#"{"kind":"rectangle","plane":"high"}"#,
    )?;
    assert_eq!(bad_plane.status, 400);
    let bad_direction = post(
        addr,
        "/api/feature/extrude",
        &format!(r#"{{"sketch_id":{sketch_id},"operation":"cut","direction":"sideways"}}"#),
    )?;
    assert_eq!(bad_direction.status, 400);
    assert!(
        json(&bad_direction)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("direction"),
        "an unknown direction must name itself"
    );
    Ok(())
}

/// Named revisions through the real HTTP surface: record two, edit between them, restore, undo the
/// restore, save, and reopen with both copies still usable.
#[test]
fn a_named_revision_restores_its_copy_and_survives_save_and_reopen()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("revisions", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // 10 x 10 x 5 = 500 mm³ is the starting point, recorded as the first named copy.
    let recorded = post(addr, "/api/revision", r#"{"name":"thin"}"#)?;
    assert_eq!(
        recorded.status,
        200,
        "{}",
        String::from_utf8_lossy(&recorded.body)
    );
    let payload = json(&recorded)?;
    let revisions = payload["revisions"].as_array().ok_or("revisions")?;
    assert_eq!(revisions.len(), 1);
    assert_eq!(revisions[0]["name"], "thin");
    assert_eq!(revisions[0]["features"], 1);
    let thin_id = revisions[0]["id"].as_u64().ok_or("thin id")?;

    // Make the body thicker and record that as a second copy.
    let thickened = post(addr, "/api/parameter", r#"{"id":"depth","value":12}"#)?;
    assert_eq!(thickened.status, 200);
    assert!(
        (json(&thickened)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 1200.0)
            .abs()
            < 1e-6
    );
    let second = post(addr, "/api/revision", r#"{"name":"thick"}"#)?;
    assert_eq!(second.status, 200);
    assert_eq!(
        json(&second)?["revisions"].as_array().map(Vec::len),
        Some(2)
    );

    // A duplicate name and an empty name are refused, and the document keeps exactly two copies.
    let duplicate = post(addr, "/api/revision", r#"{"name":"THIN"}"#)?;
    assert_eq!(duplicate.status, 400);
    assert!(
        json(&duplicate)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("already exists")
    );
    let empty = post(addr, "/api/revision", r#"{"name":"   "}"#)?;
    assert_eq!(empty.status, 400);

    // Restoring the first copy brings the 5 mm depth back, and the restore is one undo away.
    let restored = post(
        addr,
        "/api/revision/restore",
        &format!(r#"{{"id":{thin_id}}}"#),
    )?;
    assert_eq!(
        restored.status,
        200,
        "{}",
        String::from_utf8_lossy(&restored.body)
    );
    let payload = json(&restored)?;
    assert!((payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 500.0).abs() < 1e-6);
    assert_eq!(payload["revisions"].as_array().map(Vec::len), Some(2));
    let undone = post(addr, "/api/undo", "{}")?;
    assert_eq!(undone.status, 200);
    assert!(
        (json(&undone)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 1200.0)
            .abs()
            < 1e-6
    );
    let redone = post(addr, "/api/redo", "{}")?;
    assert_eq!(redone.status, 200);
    assert!(
        (json(&redone)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 500.0)
            .abs()
            < 1e-6
    );

    // An unknown revision is a 404 with a typed error, not a silent no-op.
    let unknown = post(addr, "/api/revision/restore", r#"{"id":999}"#)?;
    assert_eq!(unknown.status, 404);
    assert_eq!(json(&unknown)?["error"]["kind"], "unknown_revision");

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);

    // The copies are persisted with the project, and restoring one after a reopen works.
    let reopened = cad_document::Project::load(&path)?;
    assert_eq!(reopened.revisions().len(), 2);
    let reopened = start(reopened, path, 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["revisions"].as_array().map(Vec::len), Some(2));
    assert_eq!(state["revisions"][0]["name"], "thin");
    let restored = post(
        reopened.addr(),
        "/api/revision/restore",
        &format!(r#"{{"id":{thin_id}}}"#),
    )?;
    assert_eq!(restored.status, 200);
    assert!(
        (json(&restored)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 500.0)
            .abs()
            < 1e-6
    );

    // Deleting a copy is immediate and typed, and it does not touch the geometry.
    let second_id = state["revisions"][1]["id"].as_u64().ok_or("second id")?;
    let removed = post(
        reopened.addr(),
        "/api/revision/remove",
        &format!(r#"{{"id":{second_id}}}"#),
    )?;
    assert_eq!(
        removed.status,
        200,
        "{}",
        String::from_utf8_lossy(&removed.body)
    );
    let payload = json(&removed)?;
    assert_eq!(payload["revisions"].as_array().map(Vec::len), Some(1));
    assert!((payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 500.0).abs() < 1e-6);
    Ok(())
}

/// A project file that cannot be read falls back to the previous good copy, and the studio reports
/// which copy the session is running on rather than pretending the file was fine.
#[test]
fn a_damaged_project_reopens_the_previous_saved_copy() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("damaged.zcad");
    let mut project = default_project("damaged", [10.0, 10.0, 5.0])?;
    project.save(&path)?;
    // The next save keeps the 5 mm file as `damaged.zcad.backup` and writes 8 mm.
    project.set_parameter("depth", 8.0)?;
    project.save(&path)?;
    std::fs::write(&path, b"{\"schema\": [not json")?;

    let (recovered, info) = cad_document::open_with_recovery(&path)?;
    let info = info.ok_or("the backup must be reported")?;
    assert_eq!(info.source, cad_document::RecoverySource::Backup);
    let handle = cad_studio::start_with_recovery(recovered, path, 0, Some(info))?;
    let state = json(&request(handle.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["recovery"]["recovered"], true);
    assert_eq!(state["recovery"]["file_unreadable"], true);
    assert_eq!(state["recovery"]["source"], "backup");
    assert!(
        (state["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 500.0).abs() < 1e-6,
        "the previous good copy is the 5 mm body"
    );
    Ok(())
}

/// Open edges of a mesh after merging vertices that share a position.
///
/// An STL stores three independent vertices per triangle, so [`Mesh::open_edges`] on what the reader
/// returns is not a watertightness measure — every edge of every triangle is unpaired by
/// construction. The writer emits the kernel's already-welded mesh, so equal positions survive the
/// f64 -> f32 -> f64 round trip as equal bit patterns, which makes exact keying correct here rather
/// than merely convenient.
fn welded_open_edges(mesh: &cad_contract::mesh::Mesh) -> usize {
    let mut key_of: BTreeMap<[u64; 3], u32> = BTreeMap::new();
    let mut remap: Vec<u32> = Vec::with_capacity(mesh.positions.len());
    for position in &mesh.positions {
        let key = [
            position[0].to_bits(),
            position[1].to_bits(),
            position[2].to_bits(),
        ];
        let next = u32::try_from(key_of.len()).unwrap_or(u32::MAX);
        let index = *key_of.entry(key).or_insert(next);
        remap.push(index);
    }
    let mut edges: BTreeMap<(u32, u32), i32> = BTreeMap::new();
    for tri in &mesh.triangles {
        let (Some(a), Some(b), Some(c)) = (
            remap.get(tri[0] as usize),
            remap.get(tri[1] as usize),
            remap.get(tri[2] as usize),
        ) else {
            continue;
        };
        for (from, to) in [(*a, *b), (*b, *c), (*c, *a)] {
            let key = if from <= to { (from, to) } else { (to, from) };
            let direction = if from <= to { 1 } else { -1 };
            *edges.entry(key).or_insert(0) += direction;
        }
    }
    edges.values().filter(|count| **count != 0).count()
}

/// A revolve turns a profile a full turn about one of the sketch's own axes, and the result is the
/// analytic solid of revolution — an annulus for a rectangle offset from the axis.
///
/// The volume assertion is a **band**, not an exact value, and the band is stated rather than hidden:
/// the document tessellates at a 0.05 mm chord tolerance, so a radius-4 cylinder is an inscribed
/// prism and its volume is below `π(R² − r²)h` by the polygon's sagitta error (~1.6%, about 1.9 mm³
/// here). The adapter's own tests at its finest tolerance assert the analytic value to 0.05 mm³; this
/// test asserts that the *application* path reaches the same geometry, with exact bounds and a
/// watertight export.
#[test]
fn a_revolve_builds_the_analytic_annulus_and_survives_save_reopen_and_undo()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("annulus.zcad");
    let mut project = cad_document::Project::new(
        "annulus",
        cad_contract::ids::DocumentId::new(2),
        cad_contract::units::Unit::Millimeter,
    );
    // A 2 x 3 rectangle from x = 2 to x = 4, so a full turn about the sketch's vertical axis gives
    // outer radius 4, inner radius 2, height 3.
    let sketch = project.add_polyline_sketch(
        vec![[2.0, 0.0], [4.0, 0.0], [4.0, 3.0], [2.0, 3.0]],
        [0.0, 0.0],
    )?;
    project.add_revolve(sketch, cad_document::RevolveAxis::Vertical)?;
    project.save(&path)?;
    let handle = start(project, path.clone(), 0)?;
    let addr = handle.addr();

    let state = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["schema"], cad_document::PROJECT_SCHEMA);
    assert_eq!(state["features"][0]["kind"], "revolve");
    assert_eq!(state["features"][0]["axis"], "vertical axis");
    assert_eq!(state["features"][0]["operation"], "new body");
    assert_eq!(state["features"][0]["status"], "built");
    assert!(state["features"][0]["depth_mm"].is_null());
    assert!(state["failure"].is_null());

    let analytic = std::f64::consts::PI * (16.0 - 4.0) * 3.0;
    let volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        volume > analytic * 0.98 && volume <= analytic + 1e-6,
        "annulus volume {volume}, analytic {analytic}"
    );

    // The turned solid is centred on the axis: the sweep of a 2..4 profile reaches ±4 in the fully
    // revolved direction and stays 0..3 along the axis. Bounds come from the tessellation, so the
    // radial extents are an inscribed polygon of the circle — never past the analytic radius, and
    // within the chord tolerance of it. The axial extents are planar caps and must be exact.
    for (axis, expected_min, expected_max, tolerance) in [
        (0, -4.0, 4.0, 1e-2),
        (1, 0.0, 3.0, 1e-6),
        (2, -4.0, 4.0, 1e-2),
    ] {
        let min = state["bounds"]["min"][axis].as_f64().ok_or("bound")?;
        let max = state["bounds"]["max"][axis].as_f64().ok_or("bound")?;
        assert!(
            min >= expected_min - 1e-9
                && max <= expected_max + 1e-9
                && (min - expected_min).abs() < tolerance
                && (max - expected_max).abs() < tolerance,
            "axis {axis}: {min}..{max}, expected {expected_min}..{expected_max}"
        );
    }

    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    drop(handle);

    // Reopen from disk: the feature, its axis and the same solid come back.
    let reopened = start(cad_document::Project::load(&path)?, path.clone(), 0)?;
    let state = json(&request(reopened.addr(), "GET", "/api/state", &[], &[])?)?;
    let reopened_volume = state["geometry"]["volume_mm3"].as_f64().ok_or("volume")?;
    assert!(
        (reopened_volume - volume).abs() < 1e-9,
        "reopened {reopened_volume}, was {volume}"
    );
    assert_eq!(state["features"][0]["axis"], "vertical axis");

    // The export is the turned solid and it is watertight: a volume-only assertion would pass on a
    // mesh with a torn seam along the sweep.
    let exported = request(reopened.addr(), "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(exported.status, 200);
    let mesh = cad_io::read_stl_binary(&exported.body)?;
    assert!(
        (mesh.signed_volume().abs() - volume).abs() < 0.01,
        "exported volume {}, model {volume}",
        mesh.signed_volume()
    );
    assert_eq!(
        welded_open_edges(&mesh),
        0,
        "a revolved annulus must export as a closed surface"
    );
    Ok(())
}

/// A profile that straddles the revolve axis is not a solid of revolution, and the application says
/// so as a failed feature with the kernel's reason, blocks export, and leaves the document intact.
#[test]
fn a_revolve_across_its_axis_is_a_failed_feature_not_a_geometry()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let path = dir.path().join("crossing.zcad");
    let mut project = cad_document::Project::new(
        "crossing",
        cad_contract::ids::DocumentId::new(3),
        cad_contract::units::Unit::Millimeter,
    );
    // x from -2 to 4: it crosses the vertical axis, so sweeping it would intersect itself.
    let sketch = project.add_polyline_sketch(
        vec![[-2.0, 0.0], [4.0, 0.0], [4.0, 3.0], [-2.0, 3.0]],
        [0.0, 0.0],
    )?;
    project.add_revolve(sketch, cad_document::RevolveAxis::Vertical)?;
    project.save(&path)?;
    let handle = start(project, path.clone(), 0)?;
    let addr = handle.addr();

    let state = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["features"][0]["status"], "failed");
    let message = state["features"][0]["message"]
        .as_str()
        .ok_or("failed feature message")?;
    assert!(message.contains("crosses"), "{message}");
    assert_eq!(state["failure"]["feature"], state["features"][0]["id"]);
    assert_eq!(state["geometry"]["triangles"], 0);

    let export = request(addr, "GET", "/api/export/stl", &[], &[])?;
    assert_eq!(export.status, 400);
    let error = json(&export)?;
    assert!(
        error["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("cannot be rebuilt"),
        "export must explain why it refused"
    );
    Ok(())
}

/// The measured data-loss defect this test closes: an edit that was never saved used to disappear
/// when the process died. The studio now keeps a snapshot next to the project file, offers it on the
/// next start, marks the session dirty, and lets the user promote it with a save or throw it away.
#[test]
fn an_unsaved_edit_survives_a_crash_and_can_be_saved_or_discarded()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("recovery", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // 10 x 10 x 5 = 500 mm3. The edit below is never saved.
    let edited = post(addr, "/api/parameter", r#"{"id":"depth","value":8.0}"#)?;
    assert_eq!(
        edited.status,
        200,
        "{}",
        String::from_utf8_lossy(&edited.body)
    );
    let payload = json(&edited)?;
    assert!((payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 800.0).abs() < 1e-6);
    assert_eq!(payload["recovery"]["recovered"], false);
    assert!(payload["recovery"]["autosave_error"].is_null());
    assert!(
        cad_document::recovery_path(&path).exists(),
        "an edit must leave a recovery snapshot behind"
    );

    // The process dies here: no save, no shutdown, no chance to write anything else.
    drop(handle);

    // The project file is still the last explicit save; the snapshot holds the edit.
    let on_disk = cad_document::Project::load(&path)?;
    let depth = on_disk
        .parameters()
        .iter()
        .find(|parameter| parameter.id == "depth")
        .map(|parameter| parameter.value);
    assert_eq!(depth, Some(5.0), "the snapshot must not rewrite the file");

    let (recovered, info) = cad_document::open_with_recovery(&path)?;
    let info = info.ok_or("the unsaved edit must survive the crash")?;
    assert!(!info.file_unreadable);

    let handle = cad_studio::start_with_recovery(recovered, path.clone(), 0, Some(info))?;
    let addr = handle.addr();
    let state = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["recovery"]["recovered"], true);
    assert_eq!(state["recovery"]["file_unreadable"], false);
    assert_eq!(
        state["dirty"], true,
        "a recovered session is unsaved by definition"
    );
    assert!(
        (state["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 800.0).abs() < 1e-6,
        "the recovered session must show the edit, not the file"
    );

    // Save promotes the snapshot, and the snapshot is removed only after the file is written.
    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    let saved = json(&saved)?;
    assert_eq!(saved["recovery"]["recovered"], false);
    assert_eq!(saved["dirty"], false);
    assert!(
        !cad_document::recovery_path(&path).exists(),
        "a save consumes the snapshot"
    );
    let reopened = cad_document::Project::load(&path)?;
    let depth = reopened
        .parameters()
        .iter()
        .find(|parameter| parameter.id == "depth")
        .map(|parameter| parameter.value);
    assert_eq!(depth, Some(8.0));
    Ok(())
}

#[test]
fn a_recovered_session_can_be_discarded_back_to_the_file() -> Result<(), Box<dyn std::error::Error>>
{
    let (dir, path, handle) = fixture("discard", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();
    post(addr, "/api/parameter", r#"{"id":"depth","value":8.0}"#)?;
    drop(handle);

    let (recovered, info) = cad_document::open_with_recovery(&path)?;
    let info = info.ok_or("recovery")?;
    let handle = cad_studio::start_with_recovery(recovered, path.clone(), 0, Some(info))?;
    let addr = handle.addr();

    let discarded = post(addr, "/api/recovery/discard", "{}")?;
    assert_eq!(
        discarded.status,
        200,
        "{}",
        String::from_utf8_lossy(&discarded.body)
    );
    let payload = json(&discarded)?;
    assert_eq!(payload["recovery"]["recovered"], false);
    assert_eq!(payload["dirty"], false);
    assert!(
        (payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 500.0).abs() < 1e-6,
        "discard must go back to the saved body"
    );
    assert!(!cad_document::recovery_path(&path).exists());
    // The file itself was never modified by any of this.
    let on_disk = cad_document::Project::load(&path)?;
    assert_eq!(on_disk.features().len(), 1);
    drop(dir);
    Ok(())
}

/// Sharing binds a non-loopback interface, so every API call must present the session token. The
/// page itself may load without it: it carries no document data, and the browser needs the page in
/// order to present the token at all.
#[test]
fn a_shared_studio_requires_its_token_and_keeps_the_same_origin_rule()
-> Result<(), Box<dyn std::error::Error>> {
    use cad_studio::{ShareOptions, start_with_options};
    use std::net::{IpAddr, Ipv4Addr};

    let dir = TempDir::new()?;
    let path = dir.path().join("shared.zcad");
    let project = default_project("shared", [40.0, 30.0, 10.0])?;
    project.save(&path)?;
    let handle = start_with_options(
        project,
        path.clone(),
        0,
        None,
        ShareOptions {
            bind: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            token: Some("0123456789abcdef0123456789abcdef".into()),
            host: Some("192.168.1.20".into()),
        },
    )?;
    let addr = handle.addr();
    let token = handle
        .token()
        .ok_or("a shared studio must have a token")?
        .to_string();

    // The share link names the advertised host, the real port and the token.
    let share = handle
        .share_url()
        .ok_or("a shared studio must have a share link")?;
    assert_eq!(
        share,
        format!("http://192.168.1.20:{}/?token={token}", addr.port())
    );
    assert!(handle.url().ends_with(&format!("/?token={token}")));

    // Reading the document without a token is refused, with the reason and the right status.
    let missing = request(addr, "GET", "/api/state", &[], &[])?;
    assert_eq!(missing.status, 401);
    assert_eq!(json(&missing)?["error"]["kind"], "unauthorized");

    // Liveness is the one route a shared studio answers without the token: a container healthcheck
    // or a proxy cannot hold the secret, and the reply is the same version string anyone can read
    // from the binary. It must not leak so much as the document's name.
    let health = request(addr, "GET", "/api/health", &[], &[])?;
    assert_eq!(health.status, 200);
    assert_eq!(json(&health)?["ok"], true);
    let body = String::from_utf8_lossy(&health.body);
    assert!(
        !body.contains("shared"),
        "health must not name the document: {body}"
    );

    let wrong = request(
        addr,
        "GET",
        "/api/state",
        &[("x-atlas-token", "not-the-token")],
        &[],
    )?;
    assert_eq!(wrong.status, 401);
    assert_eq!(json(&wrong)?["error"]["kind"], "unauthorized");

    // The query form is what the page opened from the share link uses on its first request.
    let by_query = request(addr, "GET", &format!("/api/state?token={token}"), &[], &[])?;
    assert_eq!(by_query.status, 200);
    assert!(
        (json(&by_query)?["geometry"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            - 12000.0)
            .abs()
            < 1e-6
    );

    let by_header = request(addr, "GET", "/api/state", &[("x-atlas-token", &token)], &[])?;
    assert_eq!(by_header.status, 200);
    // The payload tells the client what to offer for copying.
    assert_eq!(json(&by_header)?["share"]["url"], share);

    // The page shell loads so the token can be presented; it must not leak the URL to other origins.
    let page = request(addr, "GET", "/", &[], &[])?;
    assert_eq!(page.status, 200);
    assert_eq!(
        page.headers.get("referrer-policy").map(String::as_str),
        Some("no-referrer")
    );

    // A write needs both the token and a same-origin request. `Host` is the address the browser
    // dialled, exactly as a colleague's browser would send it.
    let page_host = format!("127.0.0.1:{}", addr.port());
    let origin = format!("http://{page_host}");
    let body = r#"{"id":"depth","value":20}"#;
    let no_token = request_with_host(
        addr,
        &page_host,
        "POST",
        "/api/parameter",
        &[("origin", &origin), ("content-type", "application/json")],
        body.as_bytes(),
    )?;
    assert_eq!(no_token.status, 401);

    let bad_origin = request_with_host(
        addr,
        &page_host,
        "POST",
        "/api/parameter",
        &[
            ("x-atlas-token", &token),
            ("origin", "http://evil.example"),
            ("content-type", "application/json"),
        ],
        body.as_bytes(),
    )?;
    assert_eq!(bad_origin.status, 403);

    let written = request_with_host(
        addr,
        &page_host,
        "POST",
        "/api/parameter",
        &[
            ("x-atlas-token", &token),
            ("origin", &origin),
            ("content-type", "application/json"),
        ],
        body.as_bytes(),
    )?;
    assert_eq!(
        written.status,
        200,
        "{}",
        String::from_utf8_lossy(&written.body)
    );
    let payload = json(&written)?;
    // 40 x 30 x 20 = 24000 mm3.
    assert!((payload["geometry"]["volume_mm3"].as_f64().ok_or("volume")? - 24000.0).abs() < 1e-6);

    // A shared studio is reached under names the server cannot know: a port-mapped container
    // (`-p 7599:7519`) and a TLS-terminating proxy both present a `Host` whose port is missing or
    // different. Refusing those made every real deployment return 403, so the token is the guard
    // and the name is not. An empty `Host` is still a malformed request.
    let proxied = request_with_host(
        addr,
        "cad.example.com",
        "GET",
        &format!("/api/state?token={token}"),
        &[],
        &[],
    )?;
    assert_eq!(
        proxied.status,
        200,
        "{}",
        String::from_utf8_lossy(&proxied.body)
    );
    let no_host = request_with_host(
        addr,
        "",
        "GET",
        &format!("/api/state?token={token}"),
        &[],
        &[],
    )?;
    assert_eq!(no_host.status, 403);
    // ... and the token is still required, so an unknown host reads nothing.
    let proxied_anonymous =
        request_with_host(addr, "cad.example.com", "GET", "/api/state", &[], &[])?;
    assert_eq!(proxied_anonymous.status, 401);
    drop(dir);
    Ok(())
}

/// A proxy that terminates TLS forwards the public `Host` while the browser reports an `https://`
/// origin, so a write must be accepted across that scheme difference and still refused for another
/// site.
#[test]
fn a_proxied_shared_studio_accepts_https_edits_and_refuses_another_site()
-> Result<(), Box<dyn std::error::Error>> {
    use cad_studio::{ShareOptions, start_with_options};
    use std::net::{IpAddr, Ipv4Addr};

    let dir = TempDir::new()?;
    let path = dir.path().join("proxied.zcad");
    let project = default_project("proxied", [40.0, 30.0, 10.0])?;
    project.save(&path)?;
    let token = "0123456789abcdef0123456789abcdef";
    let handle = start_with_options(
        project,
        path.clone(),
        0,
        None,
        ShareOptions {
            bind: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            token: Some(token.into()),
            host: Some("cad.example.com".into()),
        },
    )?;
    let addr = handle.addr();
    let body = r#"{"id":"depth","value":20}"#;

    let foreign = request_with_host(
        addr,
        "cad.example.com",
        "POST",
        "/api/parameter",
        &[
            ("x-atlas-token", token),
            ("origin", "https://evil.example"),
            ("content-type", "application/json"),
        ],
        body.as_bytes(),
    )?;
    assert_eq!(foreign.status, 403);

    let edited = request_with_host(
        addr,
        "cad.example.com",
        "POST",
        "/api/parameter",
        &[
            ("x-atlas-token", token),
            ("origin", "https://cad.example.com"),
            ("content-type", "application/json"),
        ],
        body.as_bytes(),
    )?;
    assert_eq!(
        edited.status,
        200,
        "{}",
        String::from_utf8_lossy(&edited.body)
    );
    // 40 x 30 x 20 = 24000 mm3: the edit the proxy forwarded reached the document.
    let volume = json(&edited)?["geometry"]["volume_mm3"]
        .as_f64()
        .ok_or("volume")?;
    assert!((volume - 24000.0).abs() < 1e-6);
    drop(dir);
    Ok(())
}

/// A token on a loopback-bound studio is what a TLS proxy on the same host needs: the socket is
/// private, the public name is not, and the document must still be guarded.
#[test]
fn a_loopback_studio_with_a_token_requires_it() -> Result<(), Box<dyn std::error::Error>> {
    use cad_studio::{ShareOptions, start_with_options};
    use std::net::{IpAddr, Ipv4Addr};

    let dir = TempDir::new()?;
    let path = dir.path().join("proxied-loopback.zcad");
    let project = default_project("behindproxy", [10.0, 10.0, 5.0])?;
    project.save(&path)?;
    let token = "0123456789abcdef0123456789abcdef";
    let handle = start_with_options(
        project,
        path.clone(),
        0,
        None,
        ShareOptions {
            bind: Some(IpAddr::V4(Ipv4Addr::LOCALHOST)),
            token: Some(token.into()),
            host: Some("cad.example.com".into()),
        },
    )?;
    let addr = handle.addr();
    assert!(addr.ip().is_loopback());
    assert_eq!(handle.token(), Some(token));
    assert_eq!(
        handle.share_url(),
        Some(format!(
            "http://cad.example.com:{}/?token={token}",
            addr.port()
        ))
    );

    let anonymous = request(addr, "GET", "/api/state", &[], &[])?;
    assert_eq!(anonymous.status, 401);
    let authorised = request(addr, "GET", &format!("/api/state?token={token}"), &[], &[])?;
    assert_eq!(authorised.status, 200);
    drop(dir);
    Ok(())
}

/// A loopback studio stays token-free, so the desktop workflow needs no credentials.
#[test]
fn a_loopback_studio_has_no_token_and_no_share_link() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("local", [10.0, 10.0, 5.0])?;
    assert!(handle.token().is_none());
    assert!(handle.share_url().is_none());
    let state = json(&request(handle.addr(), "GET", "/api/state", &[], &[])?)?;
    assert_eq!(state["share"], serde_json::Value::Null);
    Ok(())
}

#[test]
fn the_plan_drawing_follows_the_model_and_exports_a_dxf_another_program_can_open()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("plan", [40.0, 30.0, 10.0])?;
    let addr = handle.addr();

    let draft = request(addr, "GET", "/api/draft", &[], &[])?;
    assert_eq!(draft.status, 200);
    assert_eq!(
        draft.headers.get("content-type").map(String::as_str),
        Some("application/json; charset=utf-8")
    );
    let payload = json(&draft)?;
    assert_eq!(payload["name"], "plan plan");
    assert_eq!(payload["unit"], "mm");
    assert_eq!(payload["layers"].as_array().map(Vec::len), Some(2));
    assert_eq!(payload["entities"].as_array().map(Vec::len), Some(1));
    // A 40 x 30 plan rectangle: one closed four-point loop, 140 mm around.
    let entity = &payload["entities"][0];
    assert_eq!(entity["layer"], "OUTLINE");
    assert_eq!(entity["kind"]["kind"], "polyline");
    assert_eq!(entity["kind"]["closed"], true);
    assert_eq!(
        entity["kind"]["points"],
        serde_json::json!([[0.0, 0.0], [40.0, 0.0], [40.0, 30.0], [0.0, 30.0]])
    );
    assert_eq!(payload["bounds"], serde_json::json!([0.0, 0.0, 40.0, 30.0]));
    assert!((payload["total_length"].as_f64().ok_or("length")? - 140.0).abs() < 1e-9);

    // The export is the same drawing as a file, named after the project.
    let dxf = request(addr, "GET", "/api/export/dxf", &[], &[])?;
    assert_eq!(dxf.status, 200);
    assert_eq!(
        dxf.headers.get("content-type").map(String::as_str),
        Some("application/dxf")
    );
    let disposition = dxf
        .headers
        .get("content-disposition")
        .ok_or("no content-disposition")?;
    assert!(
        disposition.contains("filename=\"plan-plan.dxf\""),
        "{disposition}"
    );
    let text = String::from_utf8(dxf.body)?;
    assert!(text.starts_with("0\nSECTION\n"));
    assert!(text.contains("$ACADVER\n1\nAC1009\n"));
    assert!(text.contains("$INSUNITS\n70\n4\n"));
    assert!(text.contains("$EXTMAX\n10\n40\n20\n30\n"));
    assert!(text.contains("\n8\nOUTLINE\n"));
    assert_eq!(text.matches("\n0\nLINE\n").count(), 4);
    // Nothing may leak between files: the DXF ends with EOF, not with leftover state.
    assert!(text.trim_end().ends_with("0\nEOF"));

    // Editing the model changes the plan, so the drawing is derived rather than cached.
    let edit = post(addr, "/api/parameter", r#"{"id":"width","value":60}"#)?;
    assert_eq!(edit.status, 200);
    let payload = json(&request(addr, "GET", "/api/draft", &[], &[])?)?;
    assert_eq!(payload["bounds"], serde_json::json!([0.0, 0.0, 60.0, 30.0]));
    assert!((payload["total_length"].as_f64().ok_or("length")? - 180.0).abs() < 1e-9);
    Ok(())
}

#[test]
fn a_shared_studio_protects_the_drawing_and_its_dxf_like_every_other_route()
-> Result<(), Box<dyn std::error::Error>> {
    use cad_studio::{ShareOptions, start_with_options};
    use std::net::{IpAddr, Ipv4Addr};

    let dir = TempDir::new()?;
    let path = dir.path().join("shared-plan.zcad");
    let project = default_project("shared-plan", [20.0, 10.0, 5.0])?;
    project.save(&path)?;
    let handle = start_with_options(
        project,
        path.clone(),
        0,
        None,
        ShareOptions {
            bind: Some(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            token: Some("0123456789abcdef0123456789abcdef".into()),
            host: Some("192.168.1.20".into()),
        },
    )?;
    let addr = handle.addr();
    let token = handle
        .token()
        .ok_or("shared studio needs a token")?
        .to_string();

    // A plan is document data: it must not be readable without the token.
    let hidden = request(addr, "GET", "/api/draft", &[], &[])?;
    assert_eq!(hidden.status, 401);
    let hidden_dxf = request(addr, "GET", "/api/export/dxf", &[], &[])?;
    assert_eq!(hidden_dxf.status, 401);

    let shown = request(addr, "GET", "/api/draft", &[("x-atlas-token", &token)], &[])?;
    assert_eq!(shown.status, 200);
    assert_eq!(
        json(&shown)?["bounds"],
        serde_json::json!([0.0, 0.0, 20.0, 10.0])
    );
    // The download link a phone or a colleague clicks carries the token in the query.
    let downloaded = request(
        addr,
        "GET",
        &format!("/api/export/dxf?token={token}"),
        &[],
        &[],
    )?;
    assert_eq!(downloaded.status, 200);
    assert!(String::from_utf8(downloaded.body)?.contains("$EXTMAX\n10\n20\n20\n10\n"));
    Ok(())
}

#[test]
fn the_drawing_is_an_editable_document_of_its_own() -> Result<(), Box<dyn std::error::Error>> {
    let (dir, _path, handle) = fixture("edited", [40.0, 30.0, 10.0])?;
    let addr = handle.addr();

    // A fresh project's drawing is generated from the sketch and says so.
    let payload = json(&request(addr, "GET", "/api/draft", &[], &[])?)?;
    assert_eq!(payload["source"], "model");
    assert!(payload["file"].is_null());
    assert_eq!(payload["dirty"], false);
    assert_eq!(payload["entities"].as_array().map(Vec::len), Some(1));

    // Draw a hole on the outline layer.
    let added = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"OUTLINE","kind":{"kind":"circle","center":[20,15],"radius":5}}"#,
    )?;
    assert_eq!(added.status, 200);
    let payload = json(&added)?;
    assert_eq!(payload["entities"].as_array().map(Vec::len), Some(2));
    assert_eq!(payload["dirty"], true);
    let circle = payload["entities"][1]["id"].as_u64().ok_or("circle id")?;

    // Geometry the document refuses is refused here too, with the document's own reason.
    let bad = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"OUTLINE","kind":{"kind":"circle","center":[0,0],"radius":0}}"#,
    )?;
    assert_eq!(bad.status, 400);
    assert!(
        json(&bad)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("radius"),
        "the refusal names the radius"
    );
    // An unknown entity type is a bad request, not a panic or a silent no-op.
    let unknown = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"OUTLINE","kind":{"kind":"spline","through":[[0,0],[1,1]]}}"#,
    )?;
    assert_eq!(unknown.status, 400);

    // A new layer, then a removal and an undo that brings the geometry back.
    let layered = post(addr, "/api/draft/layer", r#"{"name":"NOTES","color":3}"#)?;
    assert_eq!(layered.status, 200);
    assert_eq!(json(&layered)?["layers"].as_array().map(Vec::len), Some(3));
    let removed = post(addr, "/api/draft/remove", &format!(r#"{{"id":{circle}}}"#))?;
    assert_eq!(removed.status, 200);
    assert_eq!(
        json(&removed)?["entities"].as_array().map(Vec::len),
        Some(1)
    );
    let undone = post(addr, "/api/draft/undo", "{}")?;
    assert_eq!(undone.status, 200);
    let payload = json(&undone)?;
    assert_eq!(payload["entities"].as_array().map(Vec::len), Some(2));
    // Two edits remain undoable: the circle and the layer. Removing it was undone.
    assert_eq!(payload["undo"].as_u64(), Some(2));
    let redone = post(addr, "/api/draft/redo", "{}")?;
    assert_eq!(json(&redone)?["entities"].as_array().map(Vec::len), Some(1));

    // Saving writes a .zdraft beside the project, and from then on the drawing is a file.
    let saved = post(addr, "/api/draft/save", "{}")?;
    assert_eq!(saved.status, 200);
    let payload = json(&saved)?;
    assert_eq!(payload["source"], "file");
    assert_eq!(payload["dirty"], false);
    let file = payload["file"].as_str().ok_or("drawing file")?;
    assert_eq!(std::path::Path::new(file), dir.path().join("edited.zdraft"));
    assert!(std::path::Path::new(file).is_file());

    // A model edit no longer overwrites a drawing that has been saved.
    let edit = post(addr, "/api/parameter", r#"{"id":"width","value":60}"#)?;
    assert_eq!(edit.status, 200);
    let payload = json(&request(addr, "GET", "/api/draft", &[], &[])?)?;
    assert_eq!(payload["source"], "file");
    assert_eq!(payload["bounds"], serde_json::json!([0.0, 0.0, 40.0, 30.0]));

    // The DXF carries what was saved: the outline and the layer that was added.
    let dxf = String::from_utf8(request(addr, "GET", "/api/export/dxf", &[], &[])?.body)?;
    assert!(dxf.contains("\n2\nNOTES\n"), "{dxf}");
    assert!(dxf.trim_end().ends_with("0\nEOF"));
    Ok(())
}

#[test]
fn a_drawing_plots_to_a_pdf_sheet_with_a_title_block() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("plot", [40.0, 30.0, 10.0])?;
    let addr = handle.addr();

    let pdf = request(
        addr,
        "GET",
        "/api/export/pdf?paper=A4&orientation=landscape&scale=1&title=Bracket%20plate&number=AC-1001&author=Atlas&notes=DO%20NOT%20SCALE",
        &[],
        &[],
    )?;
    assert_eq!(pdf.status, 200);
    assert_eq!(
        pdf.headers.get("content-type").map(String::as_str),
        Some("application/pdf")
    );
    assert!(
        pdf.headers
            .get("content-disposition")
            .is_some_and(|value| value.contains("plot-a4.pdf")),
        "named after the project and the paper"
    );
    let text = String::from_utf8(pdf.body)?;
    assert!(text.starts_with("%PDF-1.4\n"));
    assert!(text.trim_end().ends_with("%%EOF"));
    // A4 landscape in points, so a reader opens the page at the size it was plotted at.
    assert!(
        text.contains("/MediaBox [0 0 841.89 595.276]"),
        "{text:.300}"
    );
    for expected in [
        "(Bracket plate) Tj",
        "(AC-1001) Tj",
        "(Atlas) Tj",
        "(1:1) Tj",
        "(DO NOT SCALE) Tj",
        "(40) Tj",
        "(30) Tj",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }
    // Every text run is inside a text object, so no reader can silently drop one.
    assert_eq!(text.matches("Tj").count(), text.matches("BT ").count());

    // `fit` reduces a drawing that is larger than the page instead of clipping it.
    let fitted = request(
        addr,
        "GET",
        "/api/export/pdf?paper=A4&orientation=portrait&scale=fit",
        &[],
        &[],
    )?;
    assert_eq!(fitted.status, 200);
    assert!(String::from_utf8(fitted.body)?.contains("(1:1) Tj"));

    // An unknown paper, an unknown orientation and a scale that cannot fit are refusals with a reason.
    let bad_paper = request(addr, "GET", "/api/export/pdf?paper=B2", &[], &[])?;
    assert_eq!(bad_paper.status, 400);
    assert!(
        json(&bad_paper)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("A4"),
        "the message lists the paper sizes"
    );
    let bad_orientation = request(
        addr,
        "GET",
        "/api/export/pdf?orientation=sideways",
        &[],
        &[],
    )?;
    assert_eq!(bad_orientation.status, 400);
    let too_large = request(addr, "GET", "/api/export/pdf?paper=A4&scale=0.1", &[], &[])?;
    assert_eq!(too_large.status, 400);
    assert!(
        json(&too_large)?["error"]["message"]
            .as_str()
            .unwrap_or_default()
            .contains("does not fit"),
        "a scale that does not fit says so"
    );
    Ok(())
}

/// A drawing is a derived artifact, so the studio must say which model revision it belongs to. A
/// generated drawing follows the model; once saved it is pinned, and a later model edit leaves it
/// visibly stale instead of silently presenting it as current.
#[test]
fn the_engineering_summary_pins_the_drawing_and_materials_to_the_live_revision()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("engineering", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    let state = json(&request(addr, "GET", "/api/state", &[], &[])?)?;
    let first_revision = state["revision"].as_u64().ok_or("revision")?;
    assert_eq!(state["units"], "mm");

    // A generated plan is derived from the current revision.
    let draft = json(&request(addr, "GET", "/api/draft", &[], &[])?)?;
    assert_eq!(
        draft["derived_from_revision"].as_u64(),
        Some(first_revision)
    );
    assert_eq!(draft["derivation_status"], "current");
    assert_eq!(draft["source"], "model");

    // Saving turns it into a file drawing; the file records its source revision only through this
    // record, so the status must remain current until the model changes.
    let saved = post(addr, "/api/draft/save", "{}")?;
    assert_eq!(saved.status, 200);
    assert_eq!(json(&saved)?["derivation_status"], "current");

    let edited = post(addr, "/api/parameter", r#"{"id":"width","value":20}"#)?;
    assert_eq!(edited.status, 200);
    let second_revision = json(&edited)?["revision"].as_u64().ok_or("revision")?;
    assert!(second_revision > first_revision);

    let stale = json(&request(addr, "GET", "/api/draft", &[], &[])?)?;
    assert_eq!(
        stale["derived_from_revision"].as_u64(),
        Some(first_revision),
        "the saved drawing keeps the revision it was made at"
    );
    assert_eq!(stale["derivation_status"], "requires_review");

    let engineering = json(&request(addr, "GET", "/api/engineering", &[], &[])?)?;
    assert_eq!(engineering["document"].as_u64(), Some(1));
    assert_eq!(engineering["revision"].as_u64(), Some(second_revision));
    assert_eq!(engineering["unit"], "mm");
    assert_eq!(
        engineering["drawing"]["derived_from_revision"].as_u64(),
        Some(first_revision)
    );
    assert_eq!(engineering["drawing"]["status"], "requires_review");
    assert_eq!(
        engineering["drawing"]["assessment"]["status"]["state"],
        "requires_review"
    );
    assert_eq!(
        engineering["drawing"]["assessment"]["source"]["revision"].as_u64(),
        Some(first_revision)
    );
    assert_eq!(
        engineering["drawing"]["assessment"]["status"]["current_revision"].as_u64(),
        Some(second_revision)
    );
    assert_eq!(
        engineering["drawing"]["assessment"]["releasable"],
        serde_json::json!(false)
    );

    // The material library is real data, not an empty placeholder, and every entry is marked as a
    // typical value rather than a certified specification.
    let materials = engineering["materials"].as_array().ok_or("materials")?;
    assert_eq!(materials.len(), 7);
    assert_eq!(materials[0]["id"], "steel.1018");
    assert!(
        materials.iter().all(|material| material["source"]
            .as_str()
            .is_some_and(|source| source.contains("not a certified material specification"))),
        "the library must not present typical values as certified data"
    );
    Ok(())
}

/// The drawing's source revision must live in the project file, not only in this session: a restart
/// has to see the same stale pin, and a review must be recorded without rewriting that revision.
#[test]
fn a_saved_drawing_is_recorded_in_the_project_and_can_be_reviewed()
-> Result<(), Box<dyn std::error::Error>> {
    let (_dir, path, handle) = fixture("artifact-registry", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();
    let first = json(&request(addr, "GET", "/api/state", &[], &[])?)?["revision"]
        .as_u64()
        .ok_or("revision")?;

    let saved = post(addr, "/api/draft/save", "{}")?;
    assert_eq!(saved.status, 200);

    let edited = post(addr, "/api/parameter", r#"{"id":"width","value":20}"#)?;
    assert_eq!(edited.status, 200);
    let second = json(&edited)?["revision"].as_u64().ok_or("revision")?;
    assert!(second > first);

    let stale = json(&request(addr, "GET", "/api/engineering", &[], &[])?)?;
    assert_eq!(stale["drawing"]["status"], "requires_review");
    assert_eq!(
        stale["drawing"]["derived_from_revision"].as_u64(),
        Some(first)
    );
    assert_eq!(
        stale["drawing"]["assessment"]["releasable"],
        serde_json::json!(false)
    );

    // An explicit review acknowledges the stale result at the current revision.
    let review = post(
        addr,
        "/api/engineering/review",
        &format!(r#"{{"id":1,"revision":{second}}}"#),
    )?;
    assert_eq!(review.status, 200);
    let reviewed = json(&review)?;
    assert_eq!(reviewed["drawing"]["status"], "accepted_after_review");
    assert_eq!(
        reviewed["drawing"]["assessment"]["releasable"],
        serde_json::json!(true)
    );
    assert_eq!(
        reviewed["drawing"]["derived_from_revision"].as_u64(),
        Some(first),
        "a review must not rewrite the revision the drawing was made at"
    );

    // A zero revision is not a review target.
    let zero = post(addr, "/api/engineering/review", r#"{"id":1,"revision":0}"#)?;
    assert_eq!(zero.status, 400);

    // Save and read the file: the registry is inside the project document itself.
    let saved = post(addr, "/api/save", "{}")?;
    assert_eq!(saved.status, 200);
    let text = std::fs::read_to_string(&path)?;
    assert!(
        text.contains("\"artifacts\"") && text.contains("\"reviewed_revision\""),
        "the project file must carry the record, not a session-only claim"
    );

    // Reopen the saved project in a new studio: the pin and the review must survive the restart,
    // because a `.zdraft` file itself carries neither.
    drop(handle);
    let reopened_project = cad_document::Project::load(&path)?;
    let reopened = cad_studio::start(reopened_project, path.clone(), 0)?;
    let addr = reopened.addr();
    let draft = json(&request(addr, "GET", "/api/draft", &[], &[])?)?;
    assert_eq!(draft["source"], "file");
    assert_eq!(
        draft["derived_from_revision"].as_u64(),
        Some(first),
        "the saved drawing's revision must be read back from the project registry"
    );
    let engineering = json(&request(addr, "GET", "/api/engineering", &[], &[])?)?;
    assert_eq!(engineering["drawing"]["status"], "accepted_after_review");
    assert_eq!(
        engineering["drawing"]["assessment"]["source"]["revision"].as_u64(),
        Some(first)
    );
    Ok(())
}

#[test]
fn cae_and_cam_endpoints_execute_on_solid_model() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("cae-cam-test", [40.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. Run static FEA
    let fea_resp = post(addr, "/api/cae/static", r#"{"voxel_size_mm": 5.0}"#)?;
    assert_eq!(fea_resp.status, 200);
    let fea_json = json(&fea_resp)?;
    assert!(fea_json["elements"].as_u64().unwrap_or(0) > 0);
    assert!(fea_json["max_displacement_mm"].as_f64().unwrap_or(0.0) > 0.0);
    assert!(fea_json["max_von_mises_mpa"].as_f64().unwrap_or(0.0) > 0.0);

    // 2. Run modal dynamic vibration analysis
    let modal_resp = post(
        addr,
        "/api/cae/modal",
        r#"{"voxel_size_mm": 5.0, "num_modes": 2}"#,
    )?;
    assert_eq!(modal_resp.status, 200);
    let modal_json = json(&modal_resp)?;
    let modes = modal_json["modes"]
        .as_array()
        .ok_or("modes should be an array")?;
    assert_eq!(modes.len(), 2);
    let f1 = modes[0]["frequency_hz"]
        .as_f64()
        .ok_or("f1 should be a float")?;
    assert!(f1 > 0.0);

    // 3. Run CAM toolpath generation (Standard 3-axis Facing)
    let cam_resp = post(
        addr,
        "/api/cam/toolpaths",
        r#"{"tool_diameter_mm": 6.0, "depth_mm": 1.0}"#,
    )?;
    assert_eq!(cam_resp.status, 200);
    let cam_json = json(&cam_resp)?;
    assert!(cam_json["moves_count"].as_u64().unwrap_or(0) > 0);
    assert!(cam_json["cutting_length_mm"].as_f64().unwrap_or(0.0) > 0.0);
    let gcode = cam_json["gcode_preview"]
        .as_str()
        .ok_or("gcode_preview should be a string")?;
    assert!(gcode.contains('G'));
    assert!(cam_json["points"].as_array().is_some());

    // 4. Run 4-Axis Rotary CAM toolpath generation
    let cam_4axis_resp = post(
        addr,
        "/api/cam/toolpaths",
        r#"{"machine": "4axis", "operation": "rotary4", "depth_mm": 1.0}"#,
    )?;
    assert_eq!(cam_4axis_resp.status, 200);
    let cam_4axis_json = json(&cam_4axis_resp)?;
    assert_eq!(cam_4axis_json["machine"], "4-axis indexing");
    assert_eq!(cam_4axis_json["operation"], "4-axis rotary");
    assert!(cam_4axis_json["moves_count"].as_u64().unwrap_or(0) > 5);

    // 5. Run 5-Axis Simultaneous CAM toolpath generation
    let cam_5axis_resp = post(
        addr,
        "/api/cam/toolpaths",
        r#"{"machine": "5axis", "operation": "multiaxis5", "depth_mm": 1.0}"#,
    )?;
    assert_eq!(cam_5axis_resp.status, 200);
    let cam_5axis_json = json(&cam_5axis_resp)?;
    assert_eq!(cam_5axis_json["machine"], "5-axis simultaneous");
    assert_eq!(cam_5axis_json["operation"], "5-axis simultaneous");
    let gcode_5axis = cam_5axis_json["gcode"].as_str().ok_or("gcode string")?;
    assert!(gcode_5axis.contains('I'));
    assert!(gcode_5axis.contains('J'));
    assert!(gcode_5axis.contains('K'));

    Ok(())
}

/// Advanced CAD editing operations and 2D-to-3D parametric solid bridge via HTTP.
#[test]
fn drafting_editing_endpoints_and_2d_to_3d_bridge() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("advanced_cad", [20.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. Measure distance inquiry
    let meas_resp = post(
        addr,
        "/api/draft/measure",
        r#"{"mode":"distance","p1":[0.0,0.0],"p2":[30.0,40.0]}"#,
    )?;
    assert_eq!(meas_resp.status, 200);
    let meas_json = json(&meas_resp)?;
    let dist = meas_json["distance"]
        .as_f64()
        .ok_or("distance should be a float")?;
    assert!((dist - 50.0).abs() < 1e-6);

    // 2. Measure polygon area inquiry
    let area_resp = post(
        addr,
        "/api/draft/measure",
        r#"{"mode":"area","points":[[0.0,0.0],[20.0,0.0],[20.0,10.0],[0.0,10.0]]}"#,
    )?;
    assert_eq!(area_resp.status, 200);
    let area_json = json(&area_resp)?;
    let area = area_json["area"].as_f64().ok_or("area should be a float")?;
    assert!((area - 200.0).abs() < 1e-6);

    // 3. Add a rectangle in drawing
    let add_rect = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"rectangle","origin":[0.0,0.0],"width":50.0,"height":30.0}}"#,
    )?;
    assert_eq!(add_rect.status, 200);
    let draft_payload = json(&add_rect)?;
    let entities = draft_payload["entities"]
        .as_array()
        .ok_or("entities should be array")?;
    let rect_id = entities.last().ok_or("last entity")?["id"]
        .as_u64()
        .ok_or("id")?;

    // 4. Offset rectangle
    let offset_resp = post(
        addr,
        "/api/draft/offset",
        &format!(r#"{{"id":{rect_id},"distance":5.0,"side":1.0}}"#),
    )?;
    assert_eq!(offset_resp.status, 200);

    // 5. Add two lines and fillet them
    let add_l1 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,20.0],"end":[0.0,0.0]}}"#,
    )?;
    let id_l1 = json(&add_l1)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("l1")?["id"]
        .as_u64()
        .ok_or("id")?;

    let add_l2 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,0.0],"end":[20.0,0.0]}}"#,
    )?;
    let id_l2 = json(&add_l2)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("l2")?["id"]
        .as_u64()
        .ok_or("id")?;

    let fillet_resp = post(
        addr,
        "/api/draft/fillet",
        &format!(r#"{{"id1":{id_l1},"id2":{id_l2},"radius":4.0}}"#),
    )?;
    assert_eq!(fillet_resp.status, 200);

    // 6. 2D to 3D solid bridge: extrude the rectangle into a 3D solid!
    let solid_resp = post(
        addr,
        "/api/draft/to-solid",
        &format!(r#"{{"entity_id":{rect_id},"depth":15.0}}"#),
    )?;
    assert_eq!(
        solid_resp.status,
        200,
        "{}",
        String::from_utf8_lossy(&solid_resp.body)
    );
    let solid_json = json(&solid_resp)?;
    let volume = solid_json["geometry"]["volume_mm3"]
        .as_f64()
        .ok_or("volume should be float")?;
    // The model now has the extruded solid! Volume must be strictly positive
    assert!(volume > 0.0);

    // 7. Hatch closed rectangle with ANSI31 pattern
    let hatch_resp = post(
        addr,
        "/api/draft/hatch",
        &format!(r#"{{"entity_id":{rect_id},"pattern":"ANSI31","angle_deg":45.0,"spacing":4.0}}"#),
    )?;
    assert_eq!(hatch_resp.status, 200);
    let hatch_payload = json(&hatch_resp)?;
    let entities_after_hatch = hatch_payload["entities"].as_array().ok_or("entities")?;
    assert!(entities_after_hatch.iter().any(|e| e["layer"] == "HATCH"));

    // 8. Multi-View Orthographic Drawing Generator
    let mv_resp = post(addr, "/api/draft/multiview", "{}")?;
    assert_eq!(mv_resp.status, 200);
    let mv_payload = json(&mv_resp)?;
    let layers = mv_payload["layers"].as_array().ok_or("layers")?;
    assert!(layers.iter().any(|l| l["name"] == "OUTLINE"));
    assert!(layers.iter().any(|l| l["name"] == "SECTION"));
    assert!(layers.iter().any(|l| l["name"] == "HATCH"));
    assert!(layers.iter().any(|l| l["name"] == "DIMENSIONS"));

    Ok(())
}

/// Advanced AutoCAD geometric constructions, join, pattern array, and engineering section properties via HTTP.
#[test]
fn drafting_constructions_join_array_and_section_analysis() -> Result<(), Box<dyn std::error::Error>>
{
    let (_dir, _path, handle) = fixture("draft_advanced_suite", [30.0, 30.0, 10.0])?;
    let addr = handle.addr();

    // 1. Circle 2P construction
    let c2p = post(
        addr,
        "/api/draft/construct",
        r#"{"kind":"circle_2p","layer":"0","p1":[0.0,0.0],"p2":[40.0,0.0]}"#,
    )?;
    assert_eq!(c2p.status, 200);
    let c2p_json = json(&c2p)?;
    let entities = c2p_json["entities"].as_array().ok_or("entities array")?;
    let c2p_ent = entities.last().ok_or("c2p entity")?;
    assert_eq!(c2p_ent["kind"]["kind"], "circle");
    let c2p_id = c2p_ent["id"].as_u64().ok_or("c2p id")?;

    // 2. Section Analysis on Circle
    let sec_circle = post(
        addr,
        "/api/draft/section-analysis",
        &format!(r#"{{"id":{c2p_id}}}"#),
    )?;
    assert_eq!(sec_circle.status, 200);
    let sec_circ_json = json(&sec_circle)?;
    let circ_area = sec_circ_json["area"].as_f64().ok_or("area")?;
    assert!((circ_area - std::f64::consts::PI * 400.0).abs() < 1e-4);

    // 3. Regular Polygon construction (inscribed hexagon)
    let poly_resp = post(
        addr,
        "/api/draft/construct",
        r#"{"kind":"polygon","layer":"0","center":[0.0,0.0],"radius":20.0,"sides":6,"inscribed":true}"#,
    )?;
    assert_eq!(poly_resp.status, 200);
    let poly_json = json(&poly_resp)?;
    let poly_entities = poly_json["entities"].as_array().ok_or("entities")?;
    let poly_ent = poly_entities.last().ok_or("hex entity")?;
    assert_eq!(poly_ent["kind"]["kind"], "polyline");
    let poly_id = poly_ent["id"].as_u64().ok_or("hex id")?;

    // 4. Section Analysis on Hexagon
    let sec_hex = post(
        addr,
        "/api/draft/section-analysis",
        &format!(r#"{{"id":{poly_id}}}"#),
    )?;
    assert_eq!(sec_hex.status, 200);
    let sec_hex_json = json(&sec_hex)?;
    let hex_area = sec_hex_json["area"].as_f64().ok_or("area")?;
    // Regular hexagon area = (3 * sqrt(3) / 2) * R^2 = 2.598076 * 400 = 1039.23
    assert!((hex_area - 1039.23).abs() < 1.0);

    // 5. Join lines into polyline
    let add_l1 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[10.0,0.0],"end":[30.0,0.0]}}"#,
    )?;
    let id_l1 = json(&add_l1)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("l1")?["id"]
        .as_u64()
        .ok_or("id")?;
    let add_l2 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[30.0,0.0],"end":[30.0,20.0]}}"#,
    )?;
    let id_l2 = json(&add_l2)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("l2")?["id"]
        .as_u64()
        .ok_or("id")?;

    let join_resp = post(
        addr,
        "/api/draft/join",
        &format!(r#"{{"ids":[{id_l1},{id_l2}],"tolerance":0.001}}"#),
    )?;
    assert_eq!(join_resp.status, 200);
    let join_json = json(&join_resp)?;
    let join_entities = join_json["entities"].as_array().ok_or("entities")?;
    assert!(
        join_entities
            .iter()
            .all(|e| e["id"] != id_l1 && e["id"] != id_l2)
    );
    let joined_poly = join_entities.last().ok_or("joined poly")?;
    assert_eq!(joined_poly["kind"]["kind"], "polyline");
    let joined_id = joined_poly["id"].as_u64().ok_or("joined id")?;

    // 6. Rectangular array
    let array_rect = post(
        addr,
        "/api/draft/array",
        &format!(
            r#"{{"mode":"rectangular","ids":[{joined_id}],"rows":2,"cols":2,"row_spacing":25.0,"col_spacing":35.0}}"#
        ),
    )?;
    assert_eq!(array_rect.status, 200);
    let array_rect_json = json(&array_rect)?;
    let after_rect_entities = array_rect_json["entities"].as_array().ok_or("entities")?;
    // 2x2 = 4 total items (3 new copies)
    assert!(after_rect_entities.len() >= 4);

    // 7. Polar array
    let array_polar = post(
        addr,
        "/api/draft/array",
        &format!(
            r#"{{"mode":"polar","ids":[{joined_id}],"count":4,"center":[0.0,0.0],"fill_angle_deg":360.0,"rotate_items":true}}"#
        ),
    )?;
    assert_eq!(array_polar.status, 200);

    // 8. Trim line at boundary
    let trim_target = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,10.0],"end":[40.0,10.0]}}"#,
    )?;
    let t_id = json(&trim_target)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("trim target")?["id"]
        .as_u64()
        .ok_or("id")?;

    let trim_bound = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[20.0,0.0],"end":[20.0,30.0]}}"#,
    )?;
    let b_id = json(&trim_bound)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("trim bound")?["id"]
        .as_u64()
        .ok_or("id")?;

    let trim_resp = post(
        addr,
        "/api/draft/trim",
        &format!(r#"{{"target_id":{t_id},"boundary_ids":[{b_id}],"pick_point":[5.0,10.0]}}"#),
    )?;
    assert_eq!(trim_resp.status, 200);
    let trim_entities = json(&trim_resp)?["entities"]
        .as_array()
        .ok_or("entities")?
        .clone();
    assert!(trim_entities.iter().all(|e| e["id"] != t_id));

    // 9. Extend line to boundary
    let ext_target = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,5.0],"end":[15.0,5.0]}}"#,
    )?;
    let e_id = json(&ext_target)?["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("ext target")?["id"]
        .as_u64()
        .ok_or("id")?;

    let ext_resp = post(
        addr,
        "/api/draft/extend",
        &format!(r#"{{"target_id":{e_id},"boundary_ids":[{b_id}],"pick_point":[14.0,5.0]}}"#),
    )?;
    assert_eq!(ext_resp.status, 200);

    // 10. Direct 2D Draft to CAM toolpath & G-code generation
    let cam_resp = post(
        addr,
        "/api/draft/to-cam",
        &format!(
            r#"{{"entity_id":{poly_id},"depth_mm":8.0,"stepdown_mm":2.0,"post_format":"grbl"}}"#
        ),
    )?;
    assert_eq!(cam_resp.status, 200);
    let cam_data = json(&cam_resp)?;
    let moves = cam_data["moves_count"].as_u64().ok_or("moves_count")?;
    let cut_len = cam_data["cutting_length_mm"]
        .as_f64()
        .ok_or("cutting_length_mm")?;
    let gcode = cam_data["gcode"].as_str().ok_or("gcode")?;
    assert!(moves > 0);
    assert!(cut_len > 0.0);
    assert!(gcode.contains("G01") || gcode.contains("G1"));

    Ok(())
}

#[test]
fn drafting_radial_dimensions_and_linetypes() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("rad_dim_test", [20.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. Add layer with hidden linetype
    let l_resp = post(
        addr,
        "/api/draft/layer",
        r#"{"name":"DASHED_LAYER","color":1,"linetype":"hidden"}"#,
    )?;
    assert_eq!(l_resp.status, 200);
    let l_data = json(&l_resp)?;
    let l_ent = l_data["layers"]
        .as_array()
        .ok_or("layers")?
        .iter()
        .find(|l| l["name"] == "DASHED_LAYER")
        .ok_or("found layer")?;
    assert_eq!(l_ent["linetype"], "hidden");

    // 2. Add circle on DASHED_LAYER
    let c_resp = post(
        addr,
        "/api/draft/construct",
        r#"{"kind":"circle_2p","layer":"DASHED_LAYER","p1":[0.0,0.0],"p2":[40.0,0.0]}"#,
    )?;
    assert_eq!(c_resp.status, 200);
    let c_data = json(&c_resp)?;
    let circle_id = c_data["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("circle")?["id"]
        .as_u64()
        .ok_or("id")?;

    // 3. Add Radius dimension for circle
    let rad_resp = post(
        addr,
        "/api/draft/dimension-radial",
        &format!(
            r#"{{"entity_id":{circle_id},"pick_angle_deg":45.0,"leader_offset":12.0,"is_diameter":false}}"#
        ),
    )?;
    assert_eq!(rad_resp.status, 200);
    let rad_data = json(&rad_resp)?;
    let rad_ent = rad_data["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("rad dim")?;
    assert_eq!(rad_ent["kind"]["kind"], "radial_dimension");
    assert_eq!(rad_ent["kind"]["is_diameter"], false);

    // 4. Add Diameter dimension for circle
    let dia_resp = post(
        addr,
        "/api/draft/dimension-radial",
        &format!(
            r#"{{"entity_id":{circle_id},"pick_angle_deg":135.0,"leader_offset":15.0,"is_diameter":true}}"#
        ),
    )?;
    assert_eq!(dia_resp.status, 200);
    let dia_data = json(&dia_resp)?;
    let dia_ent = dia_data["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("dia dim")?;
    assert_eq!(dia_ent["kind"]["is_diameter"], true);

    // 5. Verify DXF export includes LTYPE table, DASHED_LAYER with HIDDEN, R20, %%c40
    let dxf_resp = request(addr, "GET", "/api/export/dxf", &[], &[])?;
    assert_eq!(dxf_resp.status, 200);
    let dxf_text = String::from_utf8_lossy(&dxf_resp.body);
    assert!(dxf_text.contains("LTYPE"));
    assert!(dxf_text.contains("HIDDEN"));
    assert!(dxf_text.contains("R20"));
    assert!(dxf_text.contains("%%c40"));

    Ok(())
}

#[test]
fn drafting_blocks_and_inserts_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("blocks_api_test", [30.0, 30.0, 10.0])?;
    let addr = handle.addr();

    // 1. Create entities to define a block: two lines forming an L-bracket
    let line1 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,0.0],"end":[20.0,0.0]}}"#,
    )?;
    assert_eq!(line1.status, 200);
    let l1_data = json(&line1)?;
    let id1 = l1_data["entities"]
        .as_array()
        .ok_or("ents")?
        .last()
        .ok_or("e")?["id"]
        .as_u64()
        .ok_or("id")?;

    let line2 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,0.0],"end":[0.0,15.0]}}"#,
    )?;
    assert_eq!(line2.status, 200);
    let l2_data = json(&line2)?;
    let id2 = l2_data["entities"]
        .as_array()
        .ok_or("ents")?
        .last()
        .ok_or("e")?["id"]
        .as_u64()
        .ok_or("id")?;

    // 2. Define block "L_BRACKET" from id1 and id2 with base point [0.0, 0.0]
    let blk_resp = post(
        addr,
        "/api/draft/block",
        &format!(r#"{{"name":"L_BRACKET","base_point":[0.0,0.0],"entity_ids":[{id1},{id2}]}}"#),
    )?;
    assert_eq!(blk_resp.status, 200);
    let blk_data = json(&blk_resp)?;
    let blocks = blk_data["blocks"].as_array().ok_or("blocks")?;
    assert!(blocks.iter().any(|b| b["name"] == "L_BRACKET"));

    // 3. Insert block instance placed at [50.0, 50.0] with scale [2.0, 2.0]
    let ins_resp = post(
        addr,
        "/api/draft/insert",
        r#"{"block_name":"L_BRACKET","position":[50.0,50.0],"scale":[2.0,2.0],"rotation_degrees":45.0}"#,
    )?;
    assert_eq!(ins_resp.status, 200);
    let ins_data = json(&ins_resp)?;
    let ins_ent = ins_data["entities"]
        .as_array()
        .ok_or("entities")?
        .last()
        .ok_or("insert")?;
    assert_eq!(ins_ent["kind"]["kind"], "insert");
    assert_eq!(ins_ent["kind"]["block_name"], "L_BRACKET");
    let ins_id = ins_ent["id"].as_u64().ok_or("ins_id")?;

    // 4. DXF export includes BLOCKS section and INSERT entity
    let dxf_resp = request(addr, "GET", "/api/export/dxf", &[], &[])?;
    assert_eq!(dxf_resp.status, 200);
    let dxf_text = String::from_utf8_lossy(&dxf_resp.body);
    assert!(dxf_text.contains("BLOCKS"));
    assert!(dxf_text.contains("L_BRACKET"));
    assert!(dxf_text.contains("INSERT"));

    // 5. Explode the insert via /api/draft/explode
    let exp_resp = post(addr, "/api/draft/explode", &format!(r#"{{"id":{ins_id}}}"#))?;
    assert_eq!(exp_resp.status, 200);
    let exp_data = json(&exp_resp)?;
    // Exploding replaces the insert with the two lines
    assert!(
        !exp_data["entities"]
            .as_array()
            .ok_or("ents")?
            .iter()
            .any(|e| e["id"] == ins_id)
    );

    // 6. Undo restores the insert
    let undo_resp = post(addr, "/api/draft/undo", "{}")?;
    assert_eq!(undo_resp.status, 200);
    let undo_data = json(&undo_resp)?;
    assert!(
        undo_data["entities"]
            .as_array()
            .ok_or("ents")?
            .iter()
            .any(|e| e["id"] == ins_id)
    );

    Ok(())
}

#[test]
fn drafting_autocad_advanced_endpoints() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("autocad_adv_test", [20.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. Purge: Add unused layer and unused block, then purge
    let l_resp = post(
        addr,
        "/api/draft/layer",
        r#"{"name":"UNUSED_LAYER","color":2}"#,
    )?;
    assert_eq!(l_resp.status, 200);

    let pt_resp = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"point","at":[1.0,1.0]}}"#,
    )?;
    assert_eq!(pt_resp.status, 200);
    let pt_id = json(&pt_resp)?["entities"]
        .as_array()
        .ok_or("ents")?
        .last()
        .ok_or("pt")?["id"]
        .as_u64()
        .ok_or("id")?;

    let blk_resp = post(
        addr,
        "/api/draft/block",
        &format!(r#"{{"name":"UNUSED_BLK","base_point":[0.0,0.0],"entity_ids":[{pt_id}]}}"#),
    )?;
    assert_eq!(blk_resp.status, 200);

    let purge_resp = post(addr, "/api/draft/purge", "{}")?;
    assert_eq!(purge_resp.status, 200);
    let purge_data = json(&purge_resp)?;
    assert_eq!(
        purge_data["report"]["purged_layers"]
            .as_array()
            .ok_or("purged_layers")?,
        &vec![serde_json::Value::String("UNUSED_LAYER".into())]
    );
    assert_eq!(
        purge_data["report"]["purged_blocks"]
            .as_array()
            .ok_or("purged_blocks")?,
        &vec![serde_json::Value::String("UNUSED_BLK".into())]
    );

    // 2. Overkill: Add duplicate line and collinear overlapping line
    let _l1 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,0.0],"end":[10.0,0.0]}}"#,
    )?;
    let _l2 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[10.0,0.0],"end":[0.0,0.0]}}"#, // duplicate
    )?;
    let _ol1 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[20.0,0.0],"end":[30.0,0.0]}}"#,
    )?;
    let _ol2 = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[25.0,0.0],"end":[40.0,0.0]}}"#, // overlapping collinear
    )?;

    let overkill_resp = post(addr, "/api/draft/overkill", r#"{"tolerance":0.001}"#)?;
    assert_eq!(overkill_resp.status, 200);
    let overkill_data = json(&overkill_resp)?;
    assert_eq!(overkill_data["report"]["duplicate_entities_removed"], 1);
    assert_eq!(overkill_data["report"]["collinear_lines_merged"], 1);

    // 3. Divide: divide a 100mm line into 4 segments
    let line_div = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"0","kind":{"kind":"line","start":[0.0,50.0],"end":[100.0,50.0]}}"#,
    )?;
    assert_eq!(line_div.status, 200);
    let l_data = json(&line_div)?;
    let line_id = l_data["entities"]
        .as_array()
        .ok_or("ents")?
        .last()
        .ok_or("line")?["id"]
        .as_u64()
        .ok_or("id")?;

    let div_resp = post(
        addr,
        "/api/draft/divide",
        &format!(r#"{{"entity_id":{line_id},"segments":4}}"#),
    )?;
    assert_eq!(div_resp.status, 200);
    let div_data = json(&div_resp)?;
    let div_points = div_data["created_point_ids"].as_array().ok_or("pt_ids")?;
    assert_eq!(div_points.len(), 3); // 3 interior division points

    // 4. Measure Interval: measure along the same line at 30mm intervals
    let me_resp = post(
        addr,
        "/api/draft/measure-interval",
        &format!(r#"{{"entity_id":{line_id},"interval":30.0}}"#),
    )?;
    assert_eq!(me_resp.status, 200);
    let me_data = json(&me_resp)?;
    let me_points = me_data["created_point_ids"].as_array().ok_or("me_pts")?;
    assert_eq!(me_points.len(), 3); // 30, 60, 90

    // 5. Match Properties: copy layer from source entity to target
    let src_resp = post(
        addr,
        "/api/draft/layer",
        r#"{"name":"SOURCE_LAYER","color":4}"#,
    )?;
    assert_eq!(src_resp.status, 200);
    let ent_src = post(
        addr,
        "/api/draft/entity",
        r#"{"layer":"SOURCE_LAYER","kind":{"kind":"point","at":[5.0,5.0]}}"#,
    )?;
    let src_id = json(&ent_src)?["entities"]
        .as_array()
        .ok_or("e")?
        .last()
        .ok_or("e")?["id"]
        .as_u64()
        .ok_or("id")?;

    let match_resp = post(
        addr,
        "/api/draft/match-properties",
        &format!(r#"{{"source_id":{src_id},"target_ids":[{line_id}]}}"#),
    )?;
    assert_eq!(match_resp.status, 200);
    let match_data = json(&match_resp)?;
    assert_eq!(match_data["matched_count"], 1);

    // Verify target line is now on SOURCE_LAYER
    let draft_ents = match_data["draft"]["entities"].as_array().ok_or("e")?;
    let target_ent = draft_ents
        .iter()
        .find(|e| e["id"] == line_id)
        .ok_or("target_ent")?;
    assert_eq!(target_ent["layer"], "SOURCE_LAYER");

    // 6. Constraint Solve via /api/draft/constraints/solve
    // Solve a horizontal line from (0,0) to (10, 5) -> y2 must become 0
    let solve_resp = post(
        addr,
        "/api/draft/constraints/solve",
        r#"{
            "points": [[0.0, 0.0], [10.0, 5.0]],
            "lines": [[0, 1]],
            "constraints": [
                {"FixPoint": [0, [0.0, 0.0]]},
                {"Horizontal": 0}
            ]
        }"#,
    )?;
    assert_eq!(solve_resp.status, 200);
    let solve_data = json(&solve_resp)?;
    assert_eq!(solve_data["solve"]["converged"], true);
    assert_eq!(solve_data["dof"]["status"], "Under");
    let solved_pts = solve_data["solved_points"].as_array().ok_or("pts")?;
    let p1_y = solved_pts[1][1].as_f64().ok_or("y")?;
    assert!(
        p1_y.abs() < 1e-4,
        "Horizontal constraint should make y2 = 0"
    );

    Ok(())
}

#[test]
fn engine_benchmark_runs_a_real_solver_or_refuses() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("engine_benchmark", [20.0, 20.0, 10.0])?;
    let response = post(
        handle.addr(),
        "/api/engines/benchmark",
        r#"{"engine_id":"calculix"}"#,
    )?;
    // With ccx installed the endpoint must return a measured solve; without it, a refusal. It
    // must never return an invented accuracy score.
    if response.status == 200 {
        let data = json(&response)?;
        assert_eq!(data["benchmark_status"], "completed");
        let metrics = data["metrics"].as_object().ok_or("metrics")?;
        assert_eq!(metrics["nodes"], 45);
        assert_eq!(metrics["elements"], 16);
        let error = metrics["error_percent"].as_f64().ok_or("error_percent")?;
        assert!(error < 1.0, "solver deviates from theory by {error}%");
        assert!(
            data["provenance_hash"]
                .as_str()
                .ok_or("provenance")?
                .starts_with("fnv1a64-")
        );
    } else {
        assert_eq!(response.status, 501);
        let body = String::from_utf8_lossy(&response.body).to_lowercase();
        assert!(body.contains("no solver was run"), "{body}");
    }
    Ok(())
}

#[test]
fn engine_benchmark_runs_ngspice_or_refuses() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("engine_benchmark_ngspice", [20.0, 20.0, 10.0])?;
    let response = post(
        handle.addr(),
        "/api/engines/benchmark",
        r#"{"engine_id":"ngspice"}"#,
    )?;
    // With ngspice installed the endpoint must return a measured waveform; without it, a refusal.
    if response.status == 200 {
        let data = json(&response)?;
        assert_eq!(data["benchmark_status"], "completed");
        let metrics = data["metrics"].as_object().ok_or("metrics")?;
        let error = metrics["max_error_percent"]
            .as_f64()
            .ok_or("max_error_percent")?;
        assert!(
            error < 0.1,
            "ngspice deviates from 1 - exp(-t/RC) by {error}%"
        );
        let probes = metrics["probe_times_ms"]
            .as_array()
            .ok_or("probe_times_ms")?;
        assert_eq!(probes.len(), 6);
        assert!(
            !metrics["solver_version"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "the solver version must come from the binary"
        );
        assert!(
            data["provenance_hash"]
                .as_str()
                .ok_or("provenance")?
                .starts_with("fnv1a64-")
        );
    } else {
        assert_eq!(response.status, 501);
        let body = String::from_utf8_lossy(&response.body).to_lowercase();
        assert!(body.contains("no solver was run"), "{body}");
    }
    Ok(())
}

#[test]
fn ngspice_health_reports_the_installed_binary_or_refuses() -> Result<(), Box<dyn std::error::Error>>
{
    let (_dir, _path, handle) = fixture("engine_health_ngspice", [20.0, 20.0, 10.0])?;
    let response = post(
        handle.addr(),
        "/api/engines/health",
        r#"{"engine_id":"ngspice"}"#,
    )?;
    assert_eq!(response.status, 200);
    let data = json(&response)?;
    assert_eq!(data["engine_id"], "ngspice");
    match data["status"].as_str() {
        Some("healthy") => assert!(
            !data["detected_version"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "a healthy ngspice must report the version it detected"
        ),
        Some("unavailable") => assert!(
            data["details"]
                .as_str()
                .unwrap_or_default()
                .contains("No execution adapter")
        ),
        other => return Err(format!("unexpected health status {other:?}").into()),
    }
    Ok(())
}

#[test]
fn mass_properties_interference_and_bom_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("mass_prop_test", [20.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. GET /api/mass-properties
    let resp = request(addr, "GET", "/api/mass-properties", &[], &[])?;
    assert_eq!(resp.status, 200);
    let data = json(&resp)?;
    let vol = data["geometric"]["volume_mm3"].as_f64().ok_or("vol")?;
    assert!(vol > 0.0);
    let mass = data["physical"]["mass_kg"].as_f64().ok_or("mass")?;
    assert!(mass > 0.0);
    let centroid = data["geometric"]["centroid_mm"]
        .as_array()
        .ok_or("centroid")?;
    assert_eq!(centroid.len(), 3);
    let pm = data["geometric"]["principal_moments"]
        .as_array()
        .ok_or("pm")?;
    assert_eq!(pm.len(), 3);

    // 2. POST /api/mass-properties with custom density (Aluminum 2700 kg/m3)
    let post_resp = post(addr, "/api/mass-properties", r#"{"density_kg_m3": 2700.0}"#)?;
    assert_eq!(post_resp.status, 200);
    let post_data = json(&post_resp)?;
    let p_mass = post_data["physical"]["mass_kg"].as_f64().ok_or("pmass")?;
    let expected_mass = vol * 2700.0 * 1e-9;
    assert!((p_mass - expected_mass).abs() < 1e-6);

    // 3. GET /api/interference
    let inf_resp = request(addr, "GET", "/api/interference", &[], &[])?;
    assert_eq!(inf_resp.status, 200);
    let inf_data = json(&inf_resp)?;
    assert_eq!(inf_data["clashes_detected"].as_u64(), Some(0));

    // 4. GET /api/bom
    let bom_resp = request(addr, "GET", "/api/bom", &[], &[])?;
    assert_eq!(bom_resp.status, 200);
    let bom_data = json(&bom_resp)?;
    assert!(bom_data["total_items"].as_u64().unwrap_or(0) >= 1);
    assert!(bom_data["total_mass_kg"].as_f64().unwrap_or(0.0) > 0.0);

    Ok(())
}

#[test]
fn engines_ecosystem_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("engines_api_test", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // 1. GET /api/engines
    let resp = request(addr, "GET", "/api/engines", &[], &[])?;
    assert_eq!(resp.status, 200);
    let data = json(&resp)?;
    let count = data["count"].as_u64().ok_or("count")?;
    assert_eq!(count, 24);
    let engines = data["engines"].as_array().ok_or("engines")?;
    assert_eq!(engines.len(), 24);

    // Verify key engines exist across disciplines
    let ids: Vec<&str> = engines.iter().filter_map(|e| e["id"].as_str()).collect();
    assert!(ids.contains(&"openfoam"));
    assert!(ids.contains(&"calculix"));
    assert!(ids.contains(&"ngspice"));
    assert!(ids.contains(&"pandapower"));
    assert!(ids.contains(&"ifcopenshell"));
    assert!(ids.contains(&"mujoco"));
    assert!(ids.contains(&"su2"));
    assert!(ids.contains(&"epanet"));

    // 2. POST /api/engines/health (valid)
    let health_resp = post(addr, "/api/engines/health", r#"{"engine_id": "calculix"}"#)?;
    assert_eq!(health_resp.status, 200);
    let h_data = json(&health_resp)?;
    assert_eq!(h_data["engine_id"], "calculix");
    // With ccx installed (ATLAS_CCX or PATH) the adapter reports the detected binary; otherwise
    // the registry's refusal is reported. Both are honest, so accept either state.
    match h_data["status"].as_str() {
        Some("healthy") => assert!(
            !h_data["detected_version"]
                .as_str()
                .unwrap_or_default()
                .is_empty(),
            "a healthy CalculiX must report the version it detected"
        ),
        Some("unavailable") => assert!(
            h_data["details"]
                .as_str()
                .ok_or("details")?
                .contains("No execution adapter")
        ),
        other => return Err(format!("unexpected CalculiX health status {other:?}").into()),
    }

    // 3. POST /api/engines/health (unknown engine)
    let bad_health = post(
        addr,
        "/api/engines/health",
        r#"{"engine_id": "nonexistent"}"#,
    )?;
    assert_eq!(bad_health.status, 404);

    // 4-6. POST /api/engines/run refuses when no execution adapter is implemented. It must not
    // return a synthesized stress, load-flow or transient result: an engineering result has to come
    // from an engine that actually ran.
    for (engine_id, job_type) in [
        ("calculix", "static_fea"),
        ("pandapower", "ac_load_flow"),
        ("ngspice", "transient_analysis"),
    ] {
        let body = format!(r#"{{"engine_id": "{engine_id}", "job_type": "{job_type}"}}"#);
        let refused = post(addr, "/api/engines/run", &body)?;
        assert_eq!(refused.status, 501, "engine {engine_id} must refuse");
        let error = json(&refused)?;
        assert_eq!(error["error"]["kind"], "unavailable");
        let message = error["error"]["message"].as_str().ok_or("message")?;
        assert!(message.contains("no execution adapter"), "{message}");
    }

    Ok(())
}

#[test]
fn dependency_graph_invalidation_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("depgraph", [20.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. Initial GET /api/graph: all 8 nodes present, 7 edges, 0 stale nodes
    let resp = request(addr, "GET", "/api/graph", &[], &[])?;
    assert_eq!(resp.status, 200);
    let graph = json(&resp)?;
    assert_eq!(graph["node_count"], 8);
    assert_eq!(graph["edge_count"], 7);
    assert_eq!(graph["stale_count"], 0);
    assert_eq!(graph["is_stale"], false);

    // 2. Initial recompute plan is empty because nothing is stale
    let plan_resp = request(addr, "GET", "/api/graph/plan", &[], &[])?;
    assert_eq!(plan_resp.status, 200);
    let plan_data = json(&plan_resp)?;
    assert_eq!(plan_data["count"], 0);

    // 3. Modifying a CAD parameter triggers DAG invalidation
    let edit_resp = post(addr, "/api/parameter", r#"{"id":"depth","value":25.0}"#)?;
    assert_eq!(edit_resp.status, 200);

    // 4. Verify GET /api/graph shows stale nodes
    let stale_resp = request(addr, "GET", "/api/graph", &[], &[])?;
    assert_eq!(stale_resp.status, 200);
    let stale_graph = json(&stale_resp)?;
    let stale_count = stale_graph["stale_count"].as_u64().ok_or("stale_count")?;
    assert!(stale_count >= 6);
    assert_eq!(stale_graph["is_stale"], true);

    // 5. GET /api/graph/plan now returns topological recomputation order
    let active_plan_resp = request(addr, "GET", "/api/graph/plan", &[], &[])?;
    assert_eq!(active_plan_resp.status, 200);
    let active_plan = json(&active_plan_resp)?;
    let count = active_plan["count"].as_u64().ok_or("count")?;
    assert!(count >= 6);

    // 6. A refused engine job must not promote fea-structural to Current: no solver ran, so the
    // engineering graph keeps the node stale.
    let fea_run = post(
        addr,
        "/api/engines/run",
        r#"{"engine_id": "calculix", "job_type": "static_fea"}"#,
    )?;
    assert_eq!(fea_run.status, 501);
    let graph_after_fea = json(&request(addr, "GET", "/api/graph", &[], &[])?)?;
    let nodes = graph_after_fea["nodes"].as_array().ok_or("nodes array")?;
    let fea_node = nodes
        .iter()
        .find(|n| n["id"] == "fea-structural")
        .ok_or("fea node")?;
    assert_ne!(fea_node["state"], "Current");

    // 7. Manual invalidation endpoint POST /api/graph/invalidate
    let manual_inv = post(
        addr,
        "/api/graph/invalidate",
        r#"{"node_id":"cad-geometry","reason":"User triggered full invalidate"}"#,
    )?;
    assert_eq!(manual_inv.status, 200);
    let inv_data = json(&manual_inv)?;
    assert_eq!(inv_data["invalidated_node"], "cad-geometry");
    assert!(
        inv_data["affected_nodes"]
            .as_array()
            .ok_or("affected")?
            .len()
            >= 6
    );

    Ok(())
}

#[test]
fn engine_discovery_guides_and_benchmark_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("engines_test", [15.0, 15.0, 10.0])?;
    let addr = handle.addr();

    // 1. GET /api/engines/guides: returns 24 installation coordinates
    let guides_resp = request(addr, "GET", "/api/engines/guides", &[], &[])?;
    assert_eq!(guides_resp.status, 200);
    let guides_data = json(&guides_resp)?;
    assert_eq!(guides_data["count"], 24);
    let guides = guides_data["guides"].as_array().ok_or("guides")?;
    let ccx_guide = guides
        .iter()
        .find(|g| g["engine_id"] == "calculix")
        .ok_or("ccx guide")?;
    assert!(
        ccx_guide["canonical_git"]
            .as_str()
            .ok_or("git")?
            .contains("CalculiX")
    );

    // 2. GET /api/engines/discovery: returns discovery reports for all engines
    let disc_resp = request(addr, "GET", "/api/engines/discovery", &[], &[])?;
    assert_eq!(disc_resp.status, 200);
    let disc_data = json(&disc_resp)?;
    assert_eq!(disc_data["count"], 24);
    let reports = disc_data["reports"].as_array().ok_or("reports")?;
    let truck_rep = reports
        .iter()
        .find(|r| r["engine_id"] == "truck")
        .ok_or("truck rep")?;
    assert_eq!(truck_rep["status"], "native");

    // 3. POST /api/engines/benchmark refuses rather than reporting an invented accuracy score.
    let bench_resp = post(
        addr,
        "/api/engines/benchmark",
        r#"{"engine_id": "openfoam"}"#,
    )?;
    assert_eq!(bench_resp.status, 501);
    let bench_data = json(&bench_resp)?;
    assert_eq!(bench_data["error"]["kind"], "unavailable");
    assert!(bench_data.get("accuracy_score").is_none());

    Ok(())
}

#[test]
fn agent_workflows_never_auto_approve_or_fabricate() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("workflow_approval_default", [10.0, 10.0, 5.0])?;
    let addr = handle.addr();

    // `cad_to_cnc` starts with a CAM task that has no adapter, so the run must stop before the
    // gated G-code step and must not report completion. The gate itself is covered directly by
    // `gated_command_requires_approval_unless_auto_approved` in atlas-agent-orchestrator.
    let response = post(
        addr,
        "/api/agents/workflows/run_all",
        r#"{"workflow_id": "cad_to_cnc"}"#,
    )?;
    assert_eq!(response.status, 200);
    let payload = json(&response)?;
    assert_ne!(payload["summary"]["execution_status"], "Completed");
    let tasks = payload["tasks"].as_object().ok_or("tasks object")?;
    for task in tasks.values() {
        if task["command_id"] == "cam.gcode.emit" {
            assert_ne!(
                task["status"], "completed",
                "a gated G-code task must not complete without approval"
            );
            assert!(task.get("result").is_none());
        }
    }

    Ok(())
}

#[test]
fn enterprise_saas_audit_report_and_jobs_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("saas_enterprise_test", [20.0, 20.0, 10.0])?;
    let addr = handle.addr();

    // 1. GET /api/audit: verify cryptographic immutable audit trail
    let audit_resp = request(addr, "GET", "/api/audit", &[], &[])?;
    assert_eq!(audit_resp.status, 200);
    let audit_data = json(&audit_resp)?;
    assert_eq!(audit_data["integrity_verified"], true);
    let count = audit_data["count"].as_u64().ok_or("count")?;
    assert!(count >= 1);
    let evals = audit_data["compliance_evaluations"]
        .as_array()
        .ok_or("evals")?;
    assert_eq!(evals.len(), 3);

    // 2. GET /api/report: a draft with measured geometry and no invented evidence.
    let rep_resp = request(addr, "GET", "/api/report", &[], &[])?;
    assert_eq!(rep_resp.status, 200);
    let rep_data = json(&rep_resp)?;
    assert_eq!(rep_data["release_status"], "DRAFT_NOT_RELEASED");
    assert!(
        rep_data["mass_properties"]["volume_mm3"]
            .as_f64()
            .ok_or("volume")?
            > 0.0
    );
    assert_eq!(rep_data["mass_properties"]["assessed"], false);
    assert_eq!(rep_data["structural"]["compliance_status"], "NOT_ASSESSED");
    assert!(rep_data["bom"]["items"].as_array().ok_or("bom")?.is_empty());
    assert!(
        rep_data["sign_offs"]
            .as_array()
            .ok_or("signoffs")?
            .is_empty()
    );
    let serialized = serde_json::to_string(&rep_data)?;
    assert!(!serialized.contains("CERTIFIED_FOR_PRODUCTION"));
    assert!(!serialized.contains("CalculiX"));
    for evaluation in evals {
        assert_eq!(evaluation["status"], "TRACEABILITY_CHAIN_VALID");
        assert!(
            evaluation["assessment_scope"]
                .as_str()
                .ok_or("scope")?
                .contains("Not a certification")
        );
    }

    // 3. GET /api/org: verify enterprise organization quotas and role-based permissions
    let org_resp = request(addr, "GET", "/api/org", &[], &[])?;
    assert_eq!(org_resp.status, 200);
    let org_data = json(&org_resp)?;
    assert_eq!(org_data["tier"], "Enterprise 10k");
    assert_eq!(org_data["hpc_core_hours_quota"], 25000);
    assert_eq!(org_data["storage_quota_gb"], 5000);

    // 4. POST /api/jobs/submit: dispatch HPC simulation
    let submit_resp = post(
        addr,
        "/api/jobs/submit",
        r#"{"engine_id":"calculix","job_type":"nonlinear_fea","priority":"high","parameters":{"mesh_elements":150000}}"#,
    )?;
    assert_eq!(submit_resp.status, 200);
    let submit_data = json(&submit_resp)?;
    let job_id = submit_data["job_id"].as_str().ok_or("job_id")?;
    assert_eq!(submit_data["status"], "queued");

    // 5. GET /api/jobs: verify job is in queue
    let jobs_resp = request(addr, "GET", "/api/jobs", &[], &[])?;
    assert_eq!(jobs_resp.status, 200);
    let jobs_data = json(&jobs_resp)?;
    let job_list = jobs_data["jobs"].as_array().ok_or("jobs")?;
    assert!(job_list.iter().any(|j| j["job_id"] == job_id));

    // 6. POST /api/jobs/cancel: cancel simulation job
    let cancel_resp = post(
        addr,
        "/api/jobs/cancel",
        &format!(r#"{{"job_id":"{job_id}"}}"#),
    )?;
    assert_eq!(cancel_resp.status, 200);
    let cancel_data = json(&cancel_resp)?;
    assert_eq!(cancel_data["cancelled"], true);

    // 7. POST /api/eco: initiate and approve Engineering Change Order
    let eco_resp = post(
        addr,
        "/api/eco",
        r#"{"title":"Weight reduction pocket","description":"Pocket depth increased by 2mm for mass savings","approve":true}"#,
    )?;
    assert_eq!(eco_resp.status, 200);
    let eco_data = json(&eco_resp)?;
    assert_eq!(eco_data["status"], "approved");
    assert_eq!(eco_data["approved_by"], "chief_architect@atlascad.com");

    // 8. Re-check audit trail to verify ECO approval recorded
    let updated_audit_resp = request(addr, "GET", "/api/audit", &[], &[])?;
    assert_eq!(updated_audit_resp.status, 200);
    let updated_audit_data = json(&updated_audit_resp)?;
    assert_eq!(updated_audit_data["integrity_verified"], true);
    let updated_count = updated_audit_data["count"].as_u64().ok_or("new count")?;
    assert!(updated_count > count);

    Ok(())
}

#[test]
fn additive_manufacturing_slicing_api() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("additive_test", [30.0, 20.0, 10.0])?;
    let addr = handle.addr();

    let slice_resp = post(
        addr,
        "/api/additive/slice",
        r#"{"layer_height_mm": 0.20, "infill_density": 0.20, "infill_pattern": "honeycomb", "dialect": "marlin"}"#,
    )?;
    assert_eq!(slice_resp.status, 200);
    let sliced = json(&slice_resp)?;

    assert_eq!(sliced["model_name"], "additive_test");
    let layers = sliced["layers"].as_array().ok_or("layers")?;
    assert!(!layers.is_empty());
    assert!(layers.len() >= 40);

    let estimation = &sliced["estimation"];
    let print_time = estimation["total_print_time_seconds"]
        .as_f64()
        .ok_or("time")?;
    assert!(print_time > 0.0);

    let filament_mass = estimation["filament_mass_grams"].as_f64().ok_or("mass")?;
    assert!(filament_mass > 0.0);

    let gcode = sliced["gcode"].as_str().ok_or("gcode")?;
    assert!(gcode.contains("G21"));
    assert!(gcode.contains(";LAYER:0"));
    assert!(gcode.contains("M104 S0"));

    Ok(())
}

#[test]
fn multidisciplinary_simulation_apis() -> Result<(), Box<dyn std::error::Error>> {
    let (_dir, _path, handle) = fixture("sim_multi_test", [25.0, 25.0, 10.0])?;
    let addr = handle.addr();

    // 1. CFD Pipe Hydraulics
    let cfd_resp = post(
        addr,
        "/api/sim/cfd",
        r#"{"diameter_m": 0.05, "length_m": 12.0, "velocity_m_s": 1.2, "fluid": "water"}"#,
    )?;
    assert_eq!(cfd_resp.status, 200);
    let cfd = json(&cfd_resp)?;
    assert_eq!(cfd["status"], "Converged");
    let re = cfd["scalar_metrics"]["reynolds_number"]
        .as_f64()
        .ok_or("re")?;
    assert!(re > 1000.0);

    // 2. Aerodynamics Wing Lift/Drag
    let aero_resp = post(
        addr,
        "/api/sim/aerodynamics",
        r#"{"wing_area_m2": 15.0, "aspect_ratio": 8.0, "angle_of_attack_deg": 5.0, "airspeed_m_s": 70.0}"#,
    )?;
    assert_eq!(aero_resp.status, 200);
    let aero = json(&aero_resp)?;
    let cl = aero["scalar_metrics"]["lift_coefficient_cl"]
        .as_f64()
        .ok_or("cl")?;
    let ld = aero["scalar_metrics"]["lift_to_drag_ratio"]
        .as_f64()
        .ok_or("ld")?;
    assert!(cl > 0.3);
    assert!(ld > 10.0);

    // 3. Chemical Rachford-Rice Flash
    let chem_resp = post(
        addr,
        "/api/sim/chemical",
        r#"{"temperature_k": 315.0, "pressure_bar": 4.5}"#,
    )?;
    assert_eq!(chem_resp.status, 200);
    let chem = json(&chem_resp)?;
    let beta = chem["scalar_metrics"]["vapor_fraction_beta"]
        .as_f64()
        .ok_or("beta")?;
    assert!(beta > 0.0 && beta < 1.0);

    // 4. Robotics DH Kinematics
    let robot_resp = post(
        addr,
        "/api/sim/robotics",
        r#"{"joint_angles_deg": [20.0, 40.0, -30.0]}"#,
    )?;
    assert_eq!(robot_resp.status, 200);
    let robot = json(&robot_resp)?;
    let reach = robot["scalar_metrics"]["total_reach_m"]
        .as_f64()
        .ok_or("reach")?;
    assert!(reach > 0.5);
    let manip = robot["scalar_metrics"]["manipulability_2d"]
        .as_f64()
        .ok_or("manip")?;
    assert!(manip > 0.0);

    // 5. Electrical AC Power Flow
    let pflow_resp = post(
        addr,
        "/api/sim/powerflow",
        r#"{"p_load_mw": 50.0, "q_load_mvar": 20.0}"#,
    )?;
    assert_eq!(pflow_resp.status, 200);
    let pflow = json(&pflow_resp)?;
    let v2 = pflow["scalar_metrics"]["receiving_bus_voltage_pu"]
        .as_f64()
        .ok_or("v2")?;
    assert!(v2 > 0.85 && v2 < 1.05);
    let y_norm = pflow["scalar_metrics"]["ybus_frobenius_norm"]
        .as_f64()
        .ok_or("y_norm")?;
    assert!(y_norm > 0.0);

    // 6. Generative SIMP Topology Optimization
    let gen_resp = post(
        addr,
        "/api/sim/generative",
        r#"{"volume_fraction": 0.35, "iterations": 10}"#,
    )?;
    assert_eq!(gen_resp.status, 200);
    let generative_data = json(&gen_resp)?;
    let vol = generative_data["scalar_metrics"]["volume_fraction_achieved"]
        .as_f64()
        .ok_or("vol")?;
    assert!((vol - 0.35).abs() < 0.05);
    let triangles = generative_data["mesh"]["triangles"]
        .as_array()
        .ok_or("mesh triangles")?;
    assert!(!triangles.is_empty());

    // 7. Geotechnical Retaining Wall
    let geo_resp = post(addr, "/api/sim/geotechnical", r#"{"wall_height_m": 4.5}"#)?;
    assert_eq!(geo_resp.status, 200);
    let geo = json(&geo_resp)?;
    let ka = geo["scalar_metrics"]["active_coefficient_ka"]
        .as_f64()
        .ok_or("ka")?;
    let kp = geo["scalar_metrics"]["passive_coefficient_kp"]
        .as_f64()
        .ok_or("kp")?;
    assert!((ka * kp - 1.0).abs() < 1e-3);

    // 8. Biomedical Hemodynamics & Biomechanics
    let bio_resp = post(
        addr,
        "/api/sim/biomedical",
        r#"{"arterial_elastic_modulus_pa": 800000.0, "wall_thickness_m": 0.001, "lumen_radius_m": 0.012}"#,
    )?;
    assert_eq!(bio_resp.status, 200);
    let bio = json(&bio_resp)?;
    let pwv = bio["hemodynamics"]["pulse_wave_velocity_m_s"]
        .as_f64()
        .ok_or("pwv")?;
    assert!(pwv > 4.0 && pwv < 8.0);
    assert_eq!(bio["hemodynamics"]["is_physiologically_valid"], true);
    let bone_fos = bio["orthopedic_bone"]["factor_of_safety"]
        .as_f64()
        .ok_or("bone_fos")?;
    assert!(bone_fos > 1.5);

    // 9. Marine Hydrostatics & Airy Ocean Wave
    let marine_resp = post(
        addr,
        "/api/sim/marine",
        r#"{"hull_length_m": 50.0, "hull_beam_m": 10.0, "hull_draft_m": 3.0, "heel_angle_deg": 5.0}"#,
    )?;
    assert_eq!(marine_resp.status, 200);
    let marine = json(&marine_resp)?;
    let gm = marine["hydrostatics"]["metacentric_height_gm_m"]
        .as_f64()
        .ok_or("gm")?;
    assert!(gm > 0.5);
    assert_eq!(marine["hydrostatics"]["is_statically_stable"], true);
    let wavelength = marine["airy_wave"]["wavelength_m"]
        .as_f64()
        .ok_or("wavelength")?;
    assert!(wavelength > 80.0 && wavelength < 110.0);

    // 10. Column Buckling
    let buck_resp = post(
        addr,
        "/api/sim/buckling",
        r#"{"column_length_m": 3.0, "applied_load_n": 100000.0}"#,
    )?;
    assert_eq!(buck_resp.status, 200);
    let buck = json(&buck_resp)?;
    let p_cr = buck["scalar_metrics"]["critical_buckling_load_n"]
        .as_f64()
        .ok_or("p_cr")?;
    assert!(p_cr > 1.0e6);

    // 11. Fatigue Goodman / Gerber
    let fat_resp = post(
        addr,
        "/api/sim/fatigue",
        r#"{"alternating_stress_pa": 120000000.0, "mean_stress_pa": 60000000.0}"#,
    )?;
    assert_eq!(fat_resp.status, 200);
    let fat = json(&fat_resp)?;
    let fg = fat["scalar_metrics"]["fos_goodman"].as_f64().ok_or("fg")?;
    assert!(fg > 1.0);

    // 12. Composite CLT Laminate
    let comp_resp = post(
        addr,
        "/api/sim/composite",
        r#"{"ply_angles_deg": [0.0, 45.0, -45.0, 90.0, 90.0, -45.0, 45.0, 0.0]}"#,
    )?;
    assert_eq!(comp_resp.status, 200);
    let comp = json(&comp_resp)?;
    let ex = comp["scalar_metrics"]["effective_ex_gpa"]
        .as_f64()
        .ok_or("ex")?;
    assert!(ex > 30.0);

    // 13. Thermal-Structural Stress
    let th_resp = post(
        addr,
        "/api/sim/thermal_stress",
        r#"{"delta_temp_c": 100.0}"#,
    )?;
    assert_eq!(th_resp.status, 200);
    let th = json(&th_resp)?;
    let s_th = th["scalar_metrics"]["thermal_stress_mpa"]
        .as_f64()
        .ok_or("s_th")?;
    assert!(s_th < 0.0); // Compressive

    // 14. High-Speed RF Microstrip
    let em_resp = post(
        addr,
        "/api/sim/electromagnetics",
        r#"{"trace_width_mm": 3.0, "substrate_height_mm": 1.6}"#,
    )?;
    assert_eq!(em_resp.status, 200);
    let em = json(&em_resp)?;
    let z0 = em["scalar_metrics"]["characteristic_impedance_ohms"]
        .as_f64()
        .ok_or("z0")?;
    assert!(z0 > 45.0 && z0 < 55.0);

    // 15. Building Physics ISO 6946
    let bp_resp = post(
        addr,
        "/api/sim/building_physics",
        r#"{"wall_area_m2": 150.0}"#,
    )?;
    assert_eq!(bp_resp.status, 200);
    let bp = json(&bp_resp)?;
    let u_val = bp["scalar_metrics"]["u_value_w_m2_k"]
        .as_f64()
        .ok_or("u_val")?;
    assert!(u_val > 0.10 && u_val < 0.25);

    // 16. Additive Support Analysis
    let sup_resp = post(
        addr,
        "/api/additive/supports",
        r#"{"critical_angle_deg": 45.0}"#,
    )?;
    assert_eq!(sup_resp.status, 200);
    let sup = json(&sup_resp)?;
    assert!(sup.get("requires_support").is_some());

    // 17. Digital Twin Real-Time Strain Rosette & Palmgren-Miner Damage
    let dt_resp = post(
        addr,
        "/api/sim/digital_twin",
        r#"{"strain_a_microstrain": 450.0, "strain_b_microstrain": 200.0, "strain_c_microstrain": -150.0}"#,
    )?;
    assert_eq!(dt_resp.status, 200);
    let dt = json(&dt_resp)?;
    let vm = dt["scalar_metrics"]["von_mises_stress_mpa"]
        .as_f64()
        .ok_or("vm")?;
    assert!(vm > 50.0 && vm < 200.0);
    let damage = dt["scalar_metrics"]["cumulative_fatigue_damage_d"]
        .as_f64()
        .ok_or("damage")?;
    assert!((damage - 0.25).abs() < 1e-3);

    // 18. Architectural Acoustics & Sound Transmission Class
    let ac_resp = post(
        addr,
        "/api/sim/acoustics",
        r#"{"surface_mass_density_kg_m2": 35.0, "exterior_noise_dba": 72.0}"#,
    )?;
    assert_eq!(ac_resp.status, 200);
    let ac = json(&ac_resp)?;
    let stc = ac["scalar_metrics"]["stc_rating"].as_f64().ok_or("stc")?;
    assert!((30.0..=50.0).contains(&stc));

    // 19. Photovoltaic Solar Radiation & Energy Yield
    let sol_resp = post(
        addr,
        "/api/sim/solar",
        r#"{"array_area_m2": 120.0, "module_efficiency": 0.21, "surface_tilt_deg": 35.0}"#,
    )?;
    assert_eq!(sol_resp.status, 200);
    let sol = json(&sol_resp)?;
    let mwh = sol["scalar_metrics"]["annual_energy_yield_mwh"]
        .as_f64()
        .ok_or("mwh")?;
    assert!(mwh > 20.0 && mwh < 45.0);

    // 20. Aeroelasticity & Vortex-Induced Vibration
    let aero_resp = post(
        addr,
        "/api/sim/aeroelasticity",
        r#"{"diameter_m": 2.5, "natural_frequency_hz": 1.2, "flow_velocity_m_s": 15.0}"#,
    )?;
    assert_eq!(aero_resp.status, 200);
    let aero = json(&aero_resp)?;
    let fs = aero["scalar_metrics"]["shedding_frequency_hz"]
        .as_f64()
        .ok_or("fs")?;
    assert!((fs - 1.20).abs() < 1e-2);

    // 21. 6-DOF Quintic Spline Minimum-Jerk Trajectory
    let traj_resp = post(
        addr,
        "/api/sim/robotics_trajectory",
        r#"{"duration_s": 2.5, "time_steps": 50}"#,
    )?;
    assert_eq!(traj_resp.status, 200);
    let traj = json(&traj_resp)?;
    let v_max = traj["scalar_metrics"]["max_joint_velocity_rad_s"]
        .as_f64()
        .ok_or("v_max")?;
    assert!(v_max > 0.5 && v_max < 3.0);

    // 22. Composite Failure Criterion (Tsai-Wu / Tsai-Hill)
    let comp_resp = post(
        addr,
        "/api/sim/composite_failure",
        r#"{"sigma_1_mpa": 600.0, "sigma_2_mpa": 20.0, "tau_12_mpa": 30.0}"#,
    )?;
    assert_eq!(comp_resp.status, 200);
    let comp = json(&comp_resp)?;
    let sr_tw = comp["scalar_metrics"]["tsai_wu_strength_ratio"]
        .as_f64()
        .ok_or("sr_tw")?;
    assert!(sr_tw > 1.0);
    let comp_safe = comp["scalar_metrics"]["is_safe"]
        .as_f64()
        .ok_or("comp_safe")?;
    assert_eq!(comp_safe, 1.0);

    // 23. ASTM E1049 Rainflow Cycle Counting & Fatigue
    let rain_resp = post(
        addr,
        "/api/sim/rainflow",
        r#"{"stress_history_mpa": [0.0, 30.0, -20.0, 40.0, -40.0, 20.0, -10.0, 10.0, -30.0, 0.0]}"#,
    )?;
    assert_eq!(rain_resp.status, 200);
    let rain = json(&rain_resp)?;
    let max_range = rain["scalar_metrics"]["max_stress_range_mpa"]
        .as_f64()
        .ok_or("max_range")?;
    assert!((max_range - 80.0).abs() < 1e-3);
    let cycles = rain["scalar_metrics"]["extracted_cycle_count"]
        .as_f64()
        .ok_or("cycles")?;
    assert!(cycles >= 3.0);

    // 24. Two-Way Coupled Fluid-Structure Interaction (Coupled FSI)
    let fsi_resp = post(
        addr,
        "/api/sim/fsi",
        r#"{"length_m": 6.0, "fluid_velocity_m_s": 3.5}"#,
    )?;
    assert_eq!(fsi_resp.status, 200);
    let fsi = json(&fsi_resp)?;
    let defl_mm = fsi["scalar_metrics"]["coupled_tip_deflection_mm"]
        .as_f64()
        .ok_or("defl_mm")?;
    assert!(defl_mm > 5.0);
    let fsi_safe = fsi["scalar_metrics"]["is_safe"]
        .as_f64()
        .ok_or("fsi_safe")?;
    assert_eq!(fsi_safe, 1.0);

    // 25. Turbomachinery Cavitation & NPSH
    let cav_resp = post(
        addr,
        "/api/sim/cavitation",
        r#"{"fluid_temp_c": 60.0, "total_head_m": 45.0}"#,
    )?;
    assert_eq!(cav_resp.status, 200);
    let cav = json(&cav_resp)?;
    let npsha = cav["scalar_metrics"]["npsh_available_m"]
        .as_f64()
        .ok_or("npsha")?;
    assert!(npsha > 8.0);
    let cr = cav["scalar_metrics"]["cavitation_ratio"]
        .as_f64()
        .ok_or("cr")?;
    assert!(cr > 2.0);
    let cav_safe = cav["scalar_metrics"]["is_safe"]
        .as_f64()
        .ok_or("cav_safe")?;
    assert_eq!(cav_safe, 1.0);

    // 26. Nonlinear Elastoplasticity & Hardening
    let ep_resp = post(
        addr,
        "/api/sim/elastoplasticity",
        r#"{"applied_strain_pct": 1.5, "yield_strength_mpa": 880.0}"#,
    )?;
    assert_eq!(ep_resp.status, 200);
    let ep = json(&ep_resp)?;
    let ro_stress = ep["scalar_metrics"]["ramberg_osgood_stress_mpa"]
        .as_f64()
        .ok_or("ro_stress")?;
    assert!(ro_stress > 900.0 && ro_stress < 1150.0);
    let in_plastic = ep["scalar_metrics"]["is_in_plastic_regime"]
        .as_f64()
        .ok_or("in_plastic")?;
    assert_eq!(in_plastic, 1.0);

    // 27. Convection Heat Exchanger ε-NTU
    let hx_resp = post(
        addr,
        "/api/sim/heat_exchanger",
        r#"{"hot_inlet_temp_c": 90.0, "cold_inlet_temp_c": 20.0, "area_m2": 18.0}"#,
    )?;
    assert_eq!(hx_resp.status, 200);
    let hx = json(&hx_resp)?;
    let q_kw = hx["scalar_metrics"]["heat_duty_kw"]
        .as_f64()
        .ok_or("q_kw")?;
    assert!(q_kw > 400.0 && q_kw < 900.0);
    let eps = hx["scalar_metrics"]["effectiveness"]
        .as_f64()
        .ok_or("eps")?;
    assert!(eps > 0.40 && eps < 0.95);

    // 28. Water Hammer & Pressure Surge
    let wh_resp = post(
        addr,
        "/api/sim/water_hammer",
        r#"{"length_m": 800.0, "initial_velocity_m_s": 2.8, "valve_closure_time_s": 1.2}"#,
    )?;
    assert_eq!(wh_resp.status, 200);
    let wh = json(&wh_resp)?;
    let surge_bar = wh["scalar_metrics"]["joukowsky_surge_pressure_bar"]
        .as_f64()
        .ok_or("surge_bar")?;
    assert!(surge_bar > 20.0 && surge_bar < 50.0);
    let wh_safe = wh["scalar_metrics"]["is_safe"].as_f64().ok_or("wh_safe")?;
    assert_eq!(wh_safe, 1.0);

    // 29. Shock Response Spectrum (SRS)
    let srs_resp = post(
        addr,
        "/api/sim/shock_response",
        r#"{"peak_acceleration_g": 50.0, "pulse_duration_ms": 11.0}"#,
    )?;
    assert_eq!(srs_resp.status, 200);
    let srs = json(&srs_resp)?;
    let peak_g = srs["scalar_metrics"]["peak_srs_acceleration_g"]
        .as_f64()
        .ok_or("peak_g")?;
    assert!(peak_g > 65.0 && peak_g < 95.0);
    let daf = srs["scalar_metrics"]["dynamic_amplification_factor"]
        .as_f64()
        .ok_or("daf")?;
    assert!(daf > 1.3 && daf < 1.9);

    // 30. Dimensional Units & Conversions (Length & Pressure)
    let unit_resp = post(
        addr,
        "/api/units/convert",
        r#"{"value": 1.0, "category": "length", "from_unit": "in", "to_unit": "mm"}"#,
    )?;
    assert_eq!(unit_resp.status, 200);
    let unit_json = json(&unit_resp)?;
    let converted = unit_json["converted_value"].as_f64().ok_or("converted")?;
    assert!((converted - 25.4).abs() < 1e-6);

    let press_resp = post(
        addr,
        "/api/units/convert",
        r#"{"value": 10.0, "category": "pressure", "from_unit": "bar", "to_unit": "psi"}"#,
    )?;
    assert_eq!(press_resp.status, 200);
    let press_json = json(&press_resp)?;
    let psi = press_json["converted_value"].as_f64().ok_or("psi")?;
    assert!((psi - 145.038).abs() < 1e-1);

    // 31. Hypersonic Multi-Physics Aircraft Simulation (Mach 11 / 12,000 km/h)
    let hyp_resp = post(
        addr,
        "/api/sim/hypersonic",
        r#"{"target_speed_km_h": 12000.0, "cruise_altitude_m": 32000.0, "angle_of_attack_deg": 2.5}"#,
    )?;
    assert_eq!(hyp_resp.status, 200);
    let hyp_json = json(&hyp_resp)?;
    let mach = hyp_json["scalar_metrics"]["flight_mach"]
        .as_f64()
        .ok_or("mach")?;
    assert!(mach > 10.5 && mach < 11.5);
    let l_over_d = hyp_json["scalar_metrics"]["lift_to_drag_ratio"]
        .as_f64()
        .ok_or("l_over_d")?;
    assert!(l_over_d > 4.5 && l_over_d < 6.0);
    let stag_t = hyp_json["scalar_metrics"]["stagnation_recovery_temp_k"]
        .as_f64()
        .ok_or("stag_t")?;
    assert!(stag_t > 4000.0);
    let thrust_kn = hyp_json["scalar_metrics"]["scramjet_net_thrust_kn"]
        .as_f64()
        .ok_or("thrust_kn")?;
    assert!(thrust_kn > 100.0);
    let fos = hyp_json["scalar_metrics"]["structural_fos"]
        .as_f64()
        .ok_or("fos")?;
    assert!(fos > 1.5);

    // 31b. Hypersonic Subsystems: Plasma, Active Thermal, Scramjet, Cryo, and BOM
    let plasma_resp = post(
        addr,
        "/api/sim/hypersonic/plasma",
        r#"{"flight_mach": 11.0, "altitude_m": 32000.0}"#,
    )?;
    assert_eq!(plasma_resp.status, 200);
    let plasma_json = json(&plasma_resp)?;
    assert!(plasma_json["plasma_cutoff_freq_ghz"].as_f64().ok_or("fp")? > 10.0);
    assert_eq!(plasma_json["is_rf_blackout"].as_bool(), Some(true));

    let thermal_resp = post(
        addr,
        "/api/sim/hypersonic/thermal",
        r#"{"heat_flux_mw_m2": 1.45, "coolant_flow_kg_s": 4.93}"#,
    )?;
    assert_eq!(thermal_resp.status, 200);
    let thermal_json = json(&thermal_resp)?;
    assert!(thermal_json["coolant_htc_w_m2_k"].as_f64().ok_or("htc")? > 10_000.0);
    assert!(
        thermal_json["substrate_thermal_margin_c"]
            .as_f64()
            .ok_or("margin")?
            > 0.0
    );

    let scram_resp = post(
        addr,
        "/api/sim/hypersonic/scramjet",
        r#"{"flight_mach": 11.0, "equivalence_ratio": 1.05}"#,
    )?;
    assert_eq!(scram_resp.status, 200);
    let scram_json = json(&scram_resp)?;
    assert!(
        scram_json["isolator_unstart_margin"]
            .as_f64()
            .ok_or("margin")?
            > 1.20
    );
    assert!(
        scram_json["combustion_efficiency_pct"]
            .as_f64()
            .ok_or("eff")?
            > 80.0
    );

    let cryo_resp = post(
        addr,
        "/api/sim/hypersonic/cryo",
        r#"{"fuel_mass_kg": 16300.0}"#,
    )?;
    assert_eq!(cryo_resp.status, 200);
    let cryo_json = json(&cryo_resp)?;
    assert!(
        cryo_json["cavitation_safety_margin"]
            .as_f64()
            .ok_or("cav")?
            > 1.50
    );
    assert!(cryo_json["boil_off_rate_pct_day"].as_f64().ok_or("boil")? < 1.0);

    let bom_resp = get(addr, "/api/sim/hypersonic/bom")?;
    assert_eq!(bom_resp.status, 200);
    let bom_json = json(&bom_resp)?;
    assert_eq!(bom_json["component_count"].as_u64(), Some(21));
    assert!(bom_json["dry_weight_kg"].as_f64().ok_or("dry")? > 15_000.0);

    let mhd_resp = post(
        addr,
        "/api/sim/hypersonic/mhd",
        r#"{"flow_velocity_m_s": 3333.3, "b_field_tesla": 0.80, "channel_volume_m3": 0.25}"#,
    )?;
    assert_eq!(mhd_resp.status, 200);
    let mhd_json = json(&mhd_resp)?;
    assert!(mhd_json["extracted_power_mw"].as_f64().ok_or("mhd_p")? > 5.0);
    assert_eq!(mhd_json["avionics_bus_power_kw"].as_f64(), Some(104.2));

    let slush_resp = post(
        addr,
        "/api/sim/hypersonic/slush",
        r#"{"fuel_mass_kg": 16300.0, "solid_fraction": 0.50}"#,
    )?;
    assert_eq!(slush_resp.status, 200);
    let slush_json = json(&slush_resp)?;
    assert!(slush_json["volume_savings_m3"].as_f64().ok_or("vol")? > 20.0);
    assert!(
        slush_json["latent_heat_capacity_mj"]
            .as_f64()
            .ok_or("latent")?
            > 400.0
    );

    let creep_resp = post(
        addr,
        "/api/sim/hypersonic/creep",
        r#"{"operating_temp_k": 2545.0, "applied_stress_mpa": 45.0, "flight_duration_hours": 1.6}"#,
    )?;
    assert_eq!(creep_resp.status, 200);
    let creep_json = json(&creep_resp)?;
    assert!(creep_json["rupture_life_hours"].as_f64().ok_or("tr")? > 1000.0);
    assert!(
        creep_json["mission_accumulated_strain_pct"]
            .as_f64()
            .ok_or("strain")?
            < 0.1
    );

    // 32. DaVinci Resolve Timeline & VFX Bundle Exporter
    let davinci_resp = get(addr, "/api/export/davinci")?;
    assert_eq!(davinci_resp.status, 200);
    let davinci_json = json(&davinci_resp)?;
    assert_eq!(davinci_json["status"], "Success");
    assert_eq!(davinci_json["tracks_count"], 4);
    assert_eq!(davinci_json["markers_count"], 5);
    let fcpxml = davinci_json["fcpxml"].as_str().ok_or("fcpxml")?;
    assert!(fcpxml.contains("<xmeml version=\"5\">"));
    assert!(fcpxml.contains("sequence-atlas-hypersonic"));
    let edl = davinci_json["edl"].as_str().ok_or("edl")?;
    assert!(edl.contains("TITLE: ATLAS_HYPERSONIC_TIMELINE"));
    let lut = davinci_json["thermal_cube_lut"].as_str().ok_or("lut")?;
    assert!(lut.contains("LUT_3D_SIZE 17"));
    assert!(lut.contains("AtlasCAD_Aerothermal_Ironbow_3D"));
    let usda = davinci_json["open_usd_usda"].as_str().ok_or("usda")?;
    assert!(usda.contains("#usda 1.0"));
    assert!(usda.contains("WaveriderFuselage"));

    // DaVinci specific format query parameter
    let fcpxml_resp = get(addr, "/api/export/davinci?format=fcpxml")?;
    assert_eq!(fcpxml_resp.status, 200);
    assert_eq!(
        fcpxml_resp.headers.get("content-type").map(|s| s.as_str()),
        Some("application/xml")
    );

    // 33. Unity 3D Engine & Digital Twin Bridge
    let unity_resp = get(addr, "/api/export/unity")?;
    assert_eq!(unity_resp.status, 200);
    let unity_json = json(&unity_resp)?;
    assert_eq!(unity_json["status"], "Success");
    assert_eq!(unity_json["package_name"], "com.atlascad.unity");
    let manifest = unity_json["package_manifest"].as_str().ok_or("manifest")?;
    assert!(manifest.contains("com.atlascad.unity"));
    let client_cs = unity_json["scripts"]["AtlasCadClient.cs"]
        .as_str()
        .ok_or("client_cs")?;
    assert!(client_cs.contains("class AtlasCadClient"));
    assert!(client_cs.contains("HypersonicTelemetry"));
    let sim_cs = unity_json["scripts"]["HypersonicFlightSimulator.cs"]
        .as_str()
        .ok_or("sim_cs")?;
    assert!(sim_cs.contains("class HypersonicFlightSimulator"));
    let hud_cs = unity_json["scripts"]["AtlasCadHUD.cs"]
        .as_str()
        .ok_or("hud_cs")?;
    assert!(hud_cs.contains("class AtlasCadHUD"));
    let gltf = unity_json["gltf_json"].as_str().ok_or("gltf")?;
    assert!(gltf.contains("\"asset\""));

    let gltf_resp = get(addr, "/api/export/gltf")?;
    assert_eq!(gltf_resp.status, 200);
    assert_eq!(
        gltf_resp.headers.get("content-type").map(|s| s.as_str()),
        Some("model/gltf+json")
    );

    // 34. System Deployment Status & Health Monitoring
    let deploy_resp = get(addr, "/api/deploy/status")?;
    assert_eq!(deploy_resp.status, 200);
    let deploy_json = json(&deploy_resp)?;
    assert_eq!(deploy_json["status"], "healthy");
    assert_eq!(
        deploy_json["deployment_mode"],
        "containerized_and_standalone"
    );
    let total_engines = deploy_json["simulation_engines"]["total_engines"]
        .as_u64()
        .ok_or("total_engines")?;
    assert_eq!(total_engines, 27);
    assert_eq!(
        deploy_json["compliance"]["quality_management"],
        "traceability checks only; no certification"
    );
    assert_eq!(
        deploy_json["simulation_engines"]["status"],
        "analytical_domains_available"
    );

    // 35. Execute Commands via Command Platform
    let cmd_davinci = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "export.davinci.resolve", "arguments": {"timebase_fps": 30.0}}"#,
    )?;
    assert_eq!(cmd_davinci.status, 200);
    assert_eq!(json(&cmd_davinci)?["status"], "Success");

    let cmd_unity = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "export.unity.engine", "arguments": {"include_hud": true}}"#,
    )?;
    assert_eq!(cmd_unity.status, 200);
    assert_eq!(json(&cmd_unity)?["status"], "Success");

    let cmd_deploy = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "system.deploy.status", "arguments": {}}"#,
    )?;
    assert_eq!(cmd_deploy.status, 200);
    assert_eq!(json(&cmd_deploy)?["status"], "Success");

    // The recorded command carries a real clock reading and a digest of its own fields, not the
    // 2023 placeholder and command-name string every record used to share.
    let history = json(&request(addr, "GET", "/api/commands/history", &[], &[])?)?;
    let latest = history["history"]
        .as_array()
        .and_then(|records| records.first())
        .ok_or("command history")?;
    let recorded_at = latest["timestamp_epoch_ms"].as_u64().ok_or("timestamp")?;
    assert!(
        recorded_at > 1_700_000_000_000,
        "timestamp must be a real clock reading, got {recorded_at}"
    );
    let audit_hash = latest["audit_hash"].as_str().ok_or("audit hash")?;
    assert_eq!(audit_hash.len(), 22, "fnv1a_ plus 16 hex digits");
    assert!(audit_hash.starts_with("fnv1a_"));

    // 36. Atlas Create: Catmull-Clark Subdivision API
    let subdiv_resp = post(
        addr,
        "/api/create/subdivide",
        r#"{"mesh_id": "fuselage_skin", "levels": 2}"#,
    )?;
    assert_eq!(subdiv_resp.status, 200);
    let subdiv_json = json(&subdiv_resp)?;
    assert_eq!(subdiv_json["status"], "success");
    assert_eq!(subdiv_json["levels"], 2);
    assert!(subdiv_json["subdivided_vertices"].as_u64().unwrap_or(0) > 26);

    // 37. Atlas Create: Digital Sculpting Brush API
    let sculpt_resp = post(
        addr,
        "/api/create/sculpt",
        r#"{"brush": "clay", "radius": 2.0, "strength": 0.45}"#,
    )?;
    assert_eq!(sculpt_resp.status, 200);
    let sculpt_json = json(&sculpt_resp)?;
    assert_eq!(sculpt_json["status"], "success");
    assert_eq!(sculpt_json["brush"], "clay");
    assert!(sculpt_json["vertices_modified"].as_u64().unwrap_or(0) > 0);

    // 38. Atlas RealTime: ECS Scene Graph Hierarchy & Telemetry API
    let scene_resp = post(
        addr,
        "/api/realtime/scene",
        r#"{"name": "Hypersonic Flight Simulation"}"#,
    )?;
    assert_eq!(scene_resp.status, 200);
    let scene_json = json(&scene_resp)?;
    assert_eq!(scene_json["status"], "built");
    assert_eq!(scene_json["node_count"], 2);

    // 39. Atlas Media: Multi-Track Timeline & OTIO API
    let timeline_resp = post(
        addr,
        "/api/media/timeline",
        r#"{"title": "Hypersonic Mach 11 Commercial", "fps": 24}"#,
    )?;
    assert_eq!(timeline_resp.status, 200);
    let timeline_json = json(&timeline_resp)?;
    assert_eq!(timeline_json["status"], "ready");
    assert_eq!(timeline_json["otio_schema"], "Timeline.1");

    // 40. Atlas Media: Render Master & Color Grade API
    let render_resp = post(addr, "/api/media/render", r#"{}"#)?;
    assert_eq!(render_resp.status, 200);
    let render_json = json(&render_resp)?;
    assert_eq!(render_json["status"], "rendered");
    assert_eq!(
        render_json["export_format"],
        "ProRes 422 HQ / OpenTimelineIO"
    );

    // 41. Hypersonic Panel Flutter Aeroelastic Stability API
    let flutter_resp = post(
        addr,
        "/api/sim/hypersonic/flutter",
        r#"{"dynamic_pressure_kpa": 73.5, "flight_mach": 11.0, "wall_temp_c": 2272.0}"#,
    )?;
    assert_eq!(flutter_resp.status, 200);
    let flutter_json = json(&flutter_resp)?;
    assert_eq!(flutter_json["is_flutter_safe"], true);
    assert!(
        flutter_json["flutter_factor_of_safety"]
            .as_f64()
            .unwrap_or(0.0)
            >= 1.50
    );

    // 42. Hypersonic 6-DOF Bank-to-Turn Maneuver API
    let btt_resp = post(
        addr,
        "/api/sim/hypersonic/6dof",
        r#"{"flight_speed_m_s": 3333.3, "altitude_m": 32000.0, "target_heading_change_deg": 30.0, "bank_angle_deg": 55.0}"#,
    )?;
    assert_eq!(btt_resp.status, 200);
    let btt_json = json(&btt_resp)?;
    assert_eq!(btt_json["is_maneuver_safe"], true);
    assert!(btt_json["turn_radius_km"].as_f64().unwrap_or(0.0) > 100.0);

    // 43. REBCO 20K Superconducting Magnet API
    let magnet_resp = post(
        addr,
        "/api/sim/hypersonic/magnet",
        r#"{"target_b_field_tesla": 1.25, "channel_gap_m": 0.35, "coil_temperature_k": 20.0}"#,
    )?;
    assert_eq!(magnet_resp.status, 200);
    let magnet_json = json(&magnet_resp)?;
    assert_eq!(magnet_json["is_hts_stable"], true);
    assert!(
        magnet_json["critical_current_margin"]
            .as_f64()
            .unwrap_or(0.0)
            >= 1.30
    );

    // 44. Closed-Loop Autonomous Master Workflow Run API
    let run_all_resp = post(
        addr,
        "/api/agents/workflows/run_all",
        r#"{"workflow_id": "hypersonic_full_vehicle_master", "auto_approve": true}"#,
    )?;
    assert_eq!(run_all_resp.status, 200);
    let run_all_json = json(&run_all_resp)?;
    // The workflow must not claim success when adapters are missing: the real inspection and
    // analytical solve complete, the CAD step is refused, and everything downstream stays pending.
    assert_eq!(run_all_json["status"], "Failed");
    assert_eq!(run_all_json["summary"]["completed_tasks"], 2);
    assert_eq!(run_all_json["summary"]["failed_tasks"], 1);
    assert_eq!(run_all_json["summary"]["execution_status"], "Failed");
    assert_eq!(
        run_all_json["provenance"]["input_source"],
        "fixed_case_parameters"
    );
    assert!(run_all_json["provenance"]["document_revision"].is_u64());
    // Measured geometry is recorded, but no solver input is passed off as derived from it.
    let geometry = run_all_json["provenance"]["geometry_facts"]
        .as_object()
        .ok_or("geometry facts")?;
    assert!(geometry.contains_key("mesh_volume_mm3"));
    assert!(geometry.contains_key("characteristic_length_mm"));
    assert!(geometry.contains_key("triangle_count"));
    assert!(
        run_all_json["provenance"]["note"]
            .as_str()
            .ok_or("note")?
            .contains("geometry-to-solver adapter")
    );
    // No fabricated certification, toolpath or media claim in any task result.
    let tasks = run_all_json["tasks"].as_object().ok_or("tasks object")?;
    let serialized = serde_json::to_string(tasks)?;
    assert!(!serialized.contains("CertifiedApproved"));
    assert!(!serialized.contains("ReadyForMachine"));
    assert!(!serialized.contains("MasterRendered"));

    // 45. Multi-Disciplinary Workbenches HTML & Navigation Verification
    let index_resp = get(addr, "/")?;
    assert_eq!(index_resp.status, 200);
    let html = std::str::from_utf8(&index_resp.body).unwrap_or("");
    assert!(html.contains("id=\"aerospaceWorkbenchView\""));
    assert!(html.contains("id=\"civilWorkbenchView\""));
    assert!(html.contains("id=\"powerWorkbenchView\""));
    assert!(html.contains("id=\"chemicalWorkbenchView\""));
    assert!(html.contains("id=\"cfdWorkbenchView\""));
    assert!(html.contains("id=\"roboticsWorkbenchView\""));
    assert!(html.contains("id=\"environmentalWorkbenchView\""));
    assert!(html.contains("id=\"agentRunAllBtn\""));
    assert!(html.contains("value=\"create\""));
    assert!(html.contains("value=\"realtime\""));
    assert!(html.contains("value=\"media\""));
    assert!(html.contains("value=\"agents\""));
    assert!(html.contains("value=\"enterprise\""));

    // 46. 44 Specialized Engineering & Creative Agents Ecosystem API
    let agents_resp = get(addr, "/api/agents")?;
    assert_eq!(agents_resp.status, 200);
    let agents_json = json(&agents_resp)?;
    assert_eq!(agents_json["total_agents"], 44);
    assert_eq!(agents_json["target_ecosystem_size"], 44);

    // 47. Universal Command Platform Registry (48 Built-in Commands)
    let cmd_schemas_resp = get(addr, "/api/commands")?;
    assert_eq!(cmd_schemas_resp.status, 200);
    let cmd_schemas_json = json(&cmd_schemas_resp)?;
    assert!(cmd_schemas_json["total_commands"].as_u64().unwrap_or(0) >= 48);

    // 48. Enterprise Model Context Protocol (43 Tools via JSON-RPC 2.0)
    let mcp_tools_resp = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 100, "method": "tools/list", "params": {}}"#,
    )?;
    assert_eq!(mcp_tools_resp.status, 200);
    let mcp_tools_json = json(&mcp_tools_resp)?;
    let tools_list = mcp_tools_json["result"]["tools"]
        .as_array()
        .map(|v| v.len())
        .unwrap_or(0);
    assert!(tools_list >= 43);

    // 49. Standardized ISO 10303-21 STEP Part 21 CAD Exporter API
    let step_resp = get(addr, "/api/export/step")?;
    assert_eq!(step_resp.status, 200);
    assert_eq!(
        step_resp.headers.get("content-type").map(String::as_str),
        Some("application/step")
    );
    let step_str = std::str::from_utf8(&step_resp.body).unwrap_or("");
    assert!(step_str.contains("ISO-10303-21;"));
    assert!(step_str.contains("CONFIG_CONTROL_DESIGN"));
    assert!(step_str.contains("MANIFOLD_SOLID_BREP"));
    assert!(step_str.contains("CARTESIAN_POINT"));

    // 50. ASTM/ISO 52915 3D Manufacturing Format (3MF) Additive Exporter API
    let threemf_resp = get(addr, "/api/export/3mf")?;
    assert_eq!(threemf_resp.status, 200);
    assert_eq!(
        threemf_resp.headers.get("content-type").map(String::as_str),
        Some("model/3mf")
    );
    let threemf_str = std::str::from_utf8(&threemf_resp.body).unwrap_or("");
    assert!(threemf_str.contains("<model unit=\"millimeter\""));
    assert!(threemf_str.contains("<object id=\"1\""));
    assert!(threemf_str.contains("<triangle"));

    // 51. Universal Commands: Execute export.step.cad & export.threemf.additive
    let cmd_step = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "export.step.cad", "arguments": {}}"#,
    )?;
    assert_eq!(cmd_step.status, 200);

    let cmd_3mf = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "export.threemf.additive", "arguments": {}}"#,
    )?;
    assert_eq!(cmd_3mf.status, 200);

    // 52. Enterprise MCP Tools: atlas_export_step & atlas_export_threemf
    let mcp_step = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 101, "method": "tools/call", "params": {"name": "atlas_export_step", "arguments": {}}}"#,
    )?;
    assert_eq!(mcp_step.status, 200);

    let mcp_3mf = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 102, "method": "tools/call", "params": {"name": "atlas_export_threemf", "arguments": {}}}"#,
    )?;
    assert_eq!(mcp_3mf.status, 200);

    // 53. Analytical Chemical Acid-Base Equilibria & Titration API
    let titr_resp = post(
        addr,
        "/api/sim/chemical/acid_base",
        r#"{"analyte_id": "acetic_acid", "analyte_concentration_m": 0.10, "analyte_volume_ml": 25.0, "titrant_id": "naoh", "titrant_concentration_m": 0.10, "temperature_c": 25.0}"#,
    )?;
    assert_eq!(titr_resp.status, 200);
    let titr_json = json(&titr_resp)?;
    let init_ph = titr_json["initial_ph"].as_f64().ok_or("initial_ph")?;
    let v_eq = titr_json["equivalence_volume_ml"].as_f64().ok_or("v_eq")?;
    let eq_ph = titr_json["equivalence_ph"].as_f64().ok_or("eq_ph")?;
    let beta_max = titr_json["max_buffer_capacity"]
        .as_f64()
        .ok_or("beta_max")?;
    assert!(init_ph > 2.8 && init_ph < 3.0);
    assert!((v_eq - 25.0).abs() < 1e-4);
    assert!(eq_ph > 8.5 && eq_ph < 9.0);
    assert!(beta_max > 0.0);
    assert!(
        titr_json["titration_curve"]
            .as_array()
            .map(|a| a.len())
            .unwrap_or(0)
            > 50
    );

    // 54. Chemical Species Catalog API
    let species_resp = get(addr, "/api/sim/chemical/species")?;
    assert_eq!(species_resp.status, 200);
    let species_json = json(&species_resp)?;
    let species_list = species_json.as_array().ok_or("species list")?;
    assert!(species_list.len() >= 18);
    let has_acetic = species_list.iter().any(|s| s["id"] == "acetic_acid");
    let has_naoh = species_list.iter().any(|s| s["id"] == "naoh");
    let has_hcl = species_list.iter().any(|s| s["id"] == "hcl");
    assert!(has_acetic && has_naoh && has_hcl);

    // 55. Universal Commands: sim.chemical.acid_base & sim.chemical.species_catalog
    let cmd_titr = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "sim.chemical.acid_base", "arguments": {"analyte_id": "hcl", "analyte_concentration_m": 0.10, "analyte_volume_ml": 25.0, "titrant_id": "naoh", "titrant_concentration_m": 0.10}}"#,
    )?;
    assert_eq!(cmd_titr.status, 200);
    let cmd_titr_json = json(&cmd_titr)?;
    assert_eq!(cmd_titr_json["status"], "Success");
    let cmd_veq = cmd_titr_json["output"]["equivalence_volume_ml"]
        .as_f64()
        .ok_or("cmd_veq")?;
    assert!((cmd_veq - 25.0).abs() < 1e-4);

    let cmd_cat = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "sim.chemical.species_catalog", "arguments": {}}"#,
    )?;
    assert_eq!(cmd_cat.status, 200);

    // 56. Enterprise MCP Tools: atlas_chemical_acid_base & atlas_chemical_species_catalog
    let mcp_acid_base = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 103, "method": "tools/call", "params": {"name": "atlas_chemical_acid_base", "arguments": {"analyte_id": "acetic_acid"}}}"#,
    )?;
    assert_eq!(mcp_acid_base.status, 200);

    let mcp_species = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 104, "method": "tools/call", "params": {"name": "atlas_chemical_species_catalog", "arguments": {}}}"#,
    )?;
    assert_eq!(mcp_species.status, 200);

    // 57. Sheet Metal Bending & Flat Pattern Unfolding API (DIN 6935)
    let sm_resp = post(
        addr,
        "/api/cad/sheet_metal/unfold",
        r#"{"material": "carbon_steel", "thickness_mm": 2.0, "flange_lengths_mm": [50.0, 50.0], "bend_angles_deg": [90.0], "inner_radii_mm": [2.0], "die_opening_width_mm": 16.0}"#,
    )?;
    assert_eq!(sm_resp.status, 200);
    let sm_json = json(&sm_resp)?;
    let flat_len = sm_json["total_flat_length_mm"].as_f64().ok_or("flat_len")?;
    let forming_force = sm_json["total_forming_force_kn"]
        .as_f64()
        .ok_or("forming_force")?;
    assert!((flat_len - 96.398).abs() < 0.1);
    assert!(forming_force > 10.0 && forming_force < 40.0);
    let sm_bends = sm_json["bends"].as_array().ok_or("sm_bends")?;
    assert_eq!(sm_bends.len(), 1);
    let ba = sm_bends[0]["bend_allowance_mm"].as_f64().ok_or("ba")?;
    let bd = sm_bends[0]["bend_deduction_mm"].as_f64().ok_or("bd")?;
    assert!((ba - 4.398).abs() < 0.1);
    assert!((bd - 3.602).abs() < 0.1);

    // 58. Standard Fasteners & Bolted Joint Analysis API (ISO 4014 / ISO 898-1)
    let fast_resp = post(
        addr,
        "/api/cad/hardware/fastener",
        r#"{"thread_size": "M10", "bolt_class": "8.8", "clamped_length_mm": 40.0, "applied_tension_load_kn": 12.0, "applied_shear_load_kn": 5.0, "torque_coefficient_k": 0.18}"#,
    )?;
    assert_eq!(fast_resp.status, 200);
    let fast_json = json(&fast_resp)?;
    let at = fast_json["tensile_stress_area_mm2"].as_f64().ok_or("at")?;
    let fp = fast_json["proof_load_kn"].as_f64().ok_or("fp")?;
    let torque = fast_json["tightening_torque_nm"].as_f64().ok_or("torque")?;
    let fos_t = fast_json["tensile_safety_factor"].as_f64().ok_or("fos_t")?;
    assert!((at - 58.0).abs() < 1.0);
    assert!((fp - 34.79).abs() < 1.0);
    assert!((torque - 46.97).abs() < 1.5);
    assert!(fos_t > 1.15 && fos_t < 1.30);

    let fast_cat_resp = get(addr, "/api/cad/hardware/fasteners")?;
    assert_eq!(fast_cat_resp.status, 200);
    let fast_cat_json = json(&fast_cat_resp)?;
    let fast_list = fast_cat_json.as_array().ok_or("fasteners list")?;
    assert!(fast_list.len() >= 10);
    assert!(fast_list.iter().any(|b| b["designation"] == "M10"));
    assert!(fast_list.iter().any(|b| b["designation"] == "M16"));

    // 59. Chemical Reaction Kinetics & Reactor Sizing API
    let kin_resp = post(
        addr,
        "/api/sim/chemical/kinetics",
        r#"{"reaction_name": "Thermal Dissociation A -> B", "order": "first_order", "initial_concentration_m": 2.0, "rate_constant_k": 0.05, "volumetric_flow_m3_s": 0.005, "target_conversion": 0.80}"#,
    )?;
    assert_eq!(kin_resp.status, 200);
    let kin_json = json(&kin_resp)?;
    let t_half = kin_json["half_life_seconds"].as_f64().ok_or("t_half")?;
    let v_cstr = kin_json["cstr_volume_m3"].as_f64().ok_or("v_cstr")?;
    let v_pfr = kin_json["pfr_volume_m3"].as_f64().ok_or("v_pfr")?;
    assert!((t_half - 13.86).abs() < 0.1);
    assert!((v_cstr - 0.40).abs() < 0.02);
    assert!((v_pfr - 0.161).abs() < 0.02);
    assert!(v_cstr > v_pfr);

    // 60. Multi-Technology Additive Manufacturing Process Profiles API
    let add_sla_resp = post(
        addr,
        "/api/additive/process_profile",
        r#"{"technology": "sla_dlp_msla", "part_height_mm": 60.0, "part_volume_cm3": 35.0, "layer_height_mm": 0.050}"#,
    )?;
    assert_eq!(add_sla_resp.status, 200);
    let add_sla_json = json(&add_sla_resp)?;
    assert_eq!(add_sla_json["total_layers"], 1200);
    let cure_depth = add_sla_json["jacobs_cure_depth_mm"]
        .as_f64()
        .ok_or("cure_depth")?;
    assert!((cure_depth - 0.0675).abs() < 0.005);
    assert!(
        add_sla_json["total_build_time_hours"]
            .as_f64()
            .unwrap_or(0.0)
            > 0.0
    );

    let add_slm_resp = post(
        addr,
        "/api/additive/process_profile",
        r#"{"technology": "slm_metal_pbf", "part_height_mm": 40.0, "part_volume_cm3": 25.0, "layer_height_mm": 0.040, "laser_power_w": 280.0, "scan_speed_mm_s": 1100.0, "hatch_spacing_mm": 0.105}"#,
    )?;
    assert_eq!(add_slm_resp.status, 200);
    let add_slm_json = json(&add_slm_resp)?;
    let ved = add_slm_json["volumetric_energy_density_j_mm3"]
        .as_f64()
        .ok_or("ved")?;
    assert!((ved - 60.6).abs() < 1.0);
    let regime = add_slm_json["process_regime"].as_str().unwrap_or("");
    assert!(regime.contains("Optimal Conduction"));

    // 61. Outdoor Environmental Noise (ISO 9613-2) API
    let noise_resp = post(
        addr,
        "/api/sim/acoustics/outdoor_noise",
        r#"{"source_name": "Rooftop Industrial Chiller", "sound_power_lwa": 102.0, "distance_m": 120.0, "source_height_m": 2.5, "receiver_height_m": 1.5, "barrier_height_m": 3.5, "barrier_distance_m": 15.0, "ground_factor": 0.70, "regulatory_limit_dba": 50.0}"#,
    )?;
    assert_eq!(noise_resp.status, 200);
    let noise_json = json(&noise_resp)?;
    let div_att = noise_json["divergence_attenuation_db"]
        .as_f64()
        .ok_or("div_att")?;
    let bar_att = noise_json["barrier_attenuation_db"]
        .as_f64()
        .ok_or("bar_att")?;
    let rec_dba = noise_json["received_sound_level_dba"]
        .as_f64()
        .ok_or("rec_dba")?;
    let compliant = noise_json["is_compliant"].as_bool().ok_or("compliant")?;
    assert!((div_att - 52.58).abs() < 0.2);
    assert!(bar_att > 5.0 && bar_att <= 20.0);
    assert!(rec_dba < 50.0);
    assert!(compliant);

    // 62. Universal Commands Execution Verification
    let cmd_sm = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "cad.sheet_metal.unfold", "arguments": {"thickness_mm": 2.0, "flange_lengths_mm": [50.0, 50.0]}}"#,
    )?;
    assert_eq!(cmd_sm.status, 200);
    assert_eq!(json(&cmd_sm)?["status"], "Success");

    let cmd_fast = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "cad.hardware.fastener", "arguments": {"thread_size": "M10", "bolt_class": "8.8"}}"#,
    )?;
    assert_eq!(cmd_fast.status, 200);
    assert_eq!(json(&cmd_fast)?["status"], "Success");

    let cmd_kin = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "sim.chemical.kinetics", "arguments": {"initial_concentration_m": 2.0, "rate_constant_k": 0.05, "target_conversion": 0.80}}"#,
    )?;
    assert_eq!(cmd_kin.status, 200);
    assert_eq!(json(&cmd_kin)?["status"], "Success");

    let cmd_add = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "additive.process_profile.solve", "arguments": {"technology": "sla_dlp_msla", "part_height_mm": 60.0, "part_volume_cm3": 35.0, "layer_height_mm": 0.050}}"#,
    )?;
    assert_eq!(cmd_add.status, 200);
    assert_eq!(json(&cmd_add)?["status"], "Success");

    let cmd_noise = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "sim.acoustics.outdoor_noise", "arguments": {"sound_power_lwa": 102.0, "distance_m": 120.0}}"#,
    )?;
    assert_eq!(cmd_noise.status, 200);
    assert_eq!(json(&cmd_noise)?["status"], "Success");

    // 63. Enterprise MCP Tools Verification
    let mcp_sm = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 105, "method": "tools/call", "params": {"name": "atlas_cad_sheet_metal_unfold", "arguments": {"thickness_mm": 2.0}}}"#,
    )?;
    assert_eq!(mcp_sm.status, 200);

    let mcp_fast = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 106, "method": "tools/call", "params": {"name": "atlas_cad_hardware_fastener", "arguments": {"thread_size": "M10"}}}"#,
    )?;
    assert_eq!(mcp_fast.status, 200);

    let mcp_kin = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 107, "method": "tools/call", "params": {"name": "atlas_chemical_kinetics", "arguments": {"initial_concentration_m": 1.0}}}"#,
    )?;
    assert_eq!(mcp_kin.status, 200);

    let mcp_add = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 108, "method": "tools/call", "params": {"name": "atlas_additive_process_profile", "arguments": {"technology": "sla_dlp_msla", "part_height_mm": 50.0, "part_volume_cm3": 20.0, "layer_height_mm": 0.05}}}"#,
    )?;
    assert_eq!(mcp_add.status, 200);

    let mcp_noise = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 109, "method": "tools/call", "params": {"name": "atlas_acoustics_outdoor_noise", "arguments": {"sound_power_lwa": 100.0, "distance_m": 50.0}}}"#,
    )?;
    assert_eq!(mcp_noise.status, 200);

    // 64. Helical Spring Mechanics (DIN EN 13906-1)
    let res_spring = post(
        addr,
        "/api/cad/hardware/spring",
        r#"{
            "wire_diameter_d_mm": 4.0,
            "mean_coil_diameter_d_mm": 32.0,
            "free_length_l0_mm": 85.0,
            "active_coils_na": 7.0,
            "operating_force_f2_n": 650.0,
            "material": "MusicWireA228",
            "end_type": "ClosedAndGround"
        }"#,
    )?;
    assert_eq!(res_spring.status, 200);
    let s_json = json(&res_spring)?;
    let k = s_json["spring_rate_k_n_mm"].as_f64().unwrap();
    let kw = s_json["wahl_factor_kw"].as_f64().unwrap();
    let f0 = s_json["fundamental_frequency_hz"].as_f64().unwrap();
    assert!((k - 11.08).abs() < 0.2);
    assert!((kw - 1.184).abs() < 0.05);
    assert!(f0 > 180.0 && f0 < 250.0);
    assert!(s_json["is_buckling_safe"].as_bool().unwrap());

    // 65. Involute Spur & Helical Gear Pair (ISO 6336 / AGMA 2001-D04)
    let res_gear = post(
        addr,
        "/api/cad/hardware/gear_pair",
        r#"{
            "gear_type": "Helical",
            "normal_module_mn": 3.5,
            "pinion_teeth_z1": 19,
            "gear_teeth_z2": 57,
            "face_width_b_mm": 40.0,
            "helix_angle_beta_deg": 20.0,
            "pinion_speed_rpm": 3000.0,
            "transmitted_power_kw": 35.0
        }"#,
    )?;
    assert_eq!(res_gear.status, 200);
    let g_json = json(&res_gear)?;
    let cr = g_json["transverse_contact_ratio"].as_f64().unwrap();
    let ft = g_json["tangential_force_ft_n"].as_f64().unwrap();
    let sf = g_json["bending_safety_factor"].as_f64().unwrap();
    assert!(cr > 1.2);
    assert!((ft - 991.0).abs() < 50.0 || ft > 500.0);
    assert!(sf > 1.0);

    // 66. Kienzle Machining Physics (DIN 6584)
    let res_mach = post(
        addr,
        "/api/cam/physics/cutting_force",
        r#"{
            "operation": "FaceMilling",
            "workpiece": "AlloySteel4140",
            "cutting_speed_vc_m_per_min": 180.0,
            "feed_f_mm": 0.15,
            "depth_of_cut_ap_mm": 3.0,
            "radial_width_ae_mm": 40.0,
            "tool_diameter_dc_mm": 50.0,
            "number_of_teeth_z": 4
        }"#,
    )?;
    assert_eq!(res_mach.status, 200);
    let m_json = json(&res_mach)?;
    let fc = m_json["tangential_cutting_force_fc_n"].as_f64().unwrap();
    let pm = m_json["required_spindle_power_pm_kw"].as_f64().unwrap();
    let mrr = m_json["material_removal_rate_cm3_min"].as_f64().unwrap();
    assert!(fc > 1000.0 && fc < 2500.0);
    assert!(pm > 3.0 && pm < 12.0);
    assert!(mrr > 50.0 && mrr < 150.0);

    // 67. IEC 60909 Symmetrical Short-Circuit & Voltage Drop
    let res_sc = post(
        addr,
        "/api/sim/electrical/short_circuit",
        r#"{
            "nominal_voltage_un_v": 400.0,
            "transformer_rated_power_sn_kva": 1000.0,
            "transformer_impedance_voltage_uk_percent": 6.0,
            "feeder_length_l_m": 65.0,
            "conductor_cross_section_q_mm2": 150.0,
            "conductor_material": "Copper",
            "operating_current_ib_a": 220.0
        }"#,
    )?;
    assert_eq!(res_sc.status, 200);
    let sc_json = json(&res_sc)?;
    let ik = sc_json["initial_symmetrical_short_circuit_current_ka"]
        .as_f64()
        .unwrap();
    let ip = sc_json["peak_short_circuit_current_ka"].as_f64().unwrap();
    let v_drop = sc_json["voltage_drop_percent"].as_f64().unwrap();
    assert!(ik > 10.0 && ik < 20.0);
    assert!(ip > 18.0 && ip < 35.0);
    assert!(v_drop < 3.0);
    assert!(sc_json["is_voltage_drop_compliant"].as_bool().unwrap());

    // 68. CIE Overcast Sky Daylight Factor (EN 17037)
    let res_df = post(
        addr,
        "/api/sim/building/daylight_factor",
        r#"{
            "room_length_m": 6.0,
            "room_width_m": 5.0,
            "room_height_m": 3.0,
            "window_width_m": 3.2,
            "window_height_m": 2.0,
            "desk_distance_from_window_m": 2.5
        }"#,
    )?;
    assert_eq!(res_df.status, 200);
    let df_json = json(&res_df)?;
    let pt_df = df_json["point_daylight_factor_percent"].as_f64().unwrap();
    let avg_df = df_json["average_daylight_factor_percent"].as_f64().unwrap();
    assert!(pt_df > 1.0 && pt_df < 8.0);
    assert!(avg_df > 1.5 && avg_df < 5.0);
    assert!(df_json["is_compliant"].as_bool().unwrap());

    // 69. Universal Commands Execution Verification (Spring, Gear, CAM Force, Short Circuit, Daylight)
    let cmd_spring = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "cad.hardware.spring", "arguments": {"wire_diameter_d_mm": 4.0, "mean_coil_diameter_d_mm": 32.0, "free_length_l0_mm": 85.0, "active_coils_na": 7.0}}"#,
    )?;
    assert_eq!(cmd_spring.status, 200);
    assert_eq!(json(&cmd_spring)?["status"], "Success");

    let cmd_gear = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "cad.hardware.gear_pair", "arguments": {"normal_module_mn": 3.5, "pinion_teeth_z1": 19, "gear_teeth_z2": 57, "face_width_b_mm": 40.0}}"#,
    )?;
    assert_eq!(cmd_gear.status, 200);
    assert_eq!(json(&cmd_gear)?["status"], "Success");

    let cmd_cam = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "cam.physics.cutting_force", "arguments": {"cutting_speed_vc_m_per_min": 180.0, "feed_f_mm": 0.15, "depth_of_cut_ap_mm": 3.0}}"#,
    )?;
    assert_eq!(cmd_cam.status, 200);
    assert_eq!(json(&cmd_cam)?["status"], "Success");

    let cmd_sc = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "sim.electrical.short_circuit", "arguments": {"nominal_voltage_un_v": 400.0, "transformer_rated_power_sn_kva": 1000.0, "feeder_length_l_m": 65.0, "conductor_cross_section_q_mm2": 150.0}}"#,
    )?;
    assert_eq!(cmd_sc.status, 200);
    assert_eq!(json(&cmd_sc)?["status"], "Success");

    let cmd_df = post(
        addr,
        "/api/commands/execute",
        r#"{"command_id": "sim.building.daylight_factor", "arguments": {"room_length_m": 6.0, "room_width_m": 5.0, "room_height_m": 3.0, "window_width_m": 3.2, "window_height_m": 2.0}}"#,
    )?;
    assert_eq!(cmd_df.status, 200);
    assert_eq!(json(&cmd_df)?["status"], "Success");

    // 70. Enterprise MCP Tools Verification (Spring, Gear, CAM Force, Short Circuit, Daylight)
    let mcp_spring = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 110, "method": "tools/call", "params": {"name": "atlas_cad_hardware_spring", "arguments": {"wire_diameter_d_mm": 4.0}}}"#,
    )?;
    assert_eq!(mcp_spring.status, 200);

    let mcp_gear = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 111, "method": "tools/call", "params": {"name": "atlas_cad_hardware_gear_pair", "arguments": {"normal_module_mn": 3.5}}}"#,
    )?;
    assert_eq!(mcp_gear.status, 200);

    let mcp_cam = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 112, "method": "tools/call", "params": {"name": "atlas_cam_cutting_force", "arguments": {"feed_f_mm": 0.15}}}"#,
    )?;
    assert_eq!(mcp_cam.status, 200);

    let mcp_sc = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 113, "method": "tools/call", "params": {"name": "atlas_electrical_short_circuit", "arguments": {"nominal_voltage_un_v": 400.0}}}"#,
    )?;
    assert_eq!(mcp_sc.status, 200);

    let mcp_df = post(
        addr,
        "/api/mcp",
        r#"{"jsonrpc": "2.0", "id": 114, "method": "tools/call", "params": {"name": "atlas_building_daylight_factor", "arguments": {"room_length_m": 6.0}}}"#,
    )?;
    assert_eq!(mcp_df.status, 200);

    // 71. Transmission Shaft Sizing & DIN 6885-1 Keyway
    let res_shaft = post(
        addr,
        "/api/cad/hardware/shaft_key",
        r#"{
            "transmitted_power_kw": 45.0,
            "rotational_speed_rpm": 1450.0,
            "bending_moment_nm": 350.0,
            "selected_shaft_diameter_mm": 45.0
        }"#,
    )?;
    assert_eq!(res_shaft.status, 200);
    let sh_json = json(&res_shaft)?;
    assert!((sh_json["torque_nm"].as_f64().unwrap() - 296.34).abs() < 1.0);
    assert!(sh_json["min_required_diameter_mm"].as_f64().unwrap() > 25.0);
    assert_eq!(
        sh_json["key_dimensions"]["key_width_b_mm"]
            .as_f64()
            .unwrap(),
        14.0
    );
    assert!(sh_json["is_design_adequate"].as_bool().unwrap());

    // 72. ASME BPVC Section VIII Div 1 Pressure Vessel
    let res_pv = post(
        addr,
        "/api/sim/pressure_vessel/asme",
        r#"{
            "design_pressure_bar": 20.0,
            "inside_diameter_d_mm": 1200.0,
            "tangent_length_l_mm": 3000.0,
            "corrosion_allowance_mm": 2.5
        }"#,
    )?;
    assert_eq!(res_pv.status, 200);
    let pv_json = json(&res_pv)?;
    assert!((pv_json["min_required_shell_thickness_mm"].as_f64().unwrap() - 13.07).abs() < 0.2);
    assert!(pv_json["vessel_mawp_bar"].as_f64().unwrap() > 20.0);
    assert!(pv_json["hydrostatic_test_pressure_bar"].as_f64().unwrap() > 30.0);
    assert!(pv_json["is_asme_compliant"].as_bool().unwrap());

    // 73. ASHRAE Psychrometrics & Moist Air Thermodynamics
    let res_psy = post(
        addr,
        "/api/sim/thermal/psychrometrics",
        r#"{
            "dry_bulb_temp_c": 24.0,
            "relative_humidity": 0.50
        }"#,
    )?;
    assert_eq!(res_psy.status, 200);
    let psy_json = json(&res_psy)?;
    assert!((psy_json["humidity_ratio_g_kg"].as_f64().unwrap() - 9.3).abs() < 0.2);
    assert!((psy_json["dew_point_temp_c"].as_f64().unwrap() - 12.9).abs() < 0.5);
    assert!((psy_json["specific_enthalpy_kj_kg"].as_f64().unwrap() - 47.8).abs() < 0.5);

    // 74. Civil Stormwater Hydrology & Manning Hydraulics
    let res_hydro = post(
        addr,
        "/api/sim/civil/hydrology",
        r#"{
            "catchment_area_ha": 8.5,
            "flow_path_length_m": 450.0,
            "terrain_slope": 0.020,
            "pipe_slope": 0.008
        }"#,
    )?;
    assert_eq!(res_hydro.status, 200);
    let hydro_json = json(&res_hydro)?;
    assert!(hydro_json["peak_discharge_m3_s"].as_f64().unwrap() > 0.8);
    assert!(
        hydro_json["min_required_pipe_diameter_mm"]
            .as_f64()
            .unwrap()
            > 700.0
    );
    assert!(hydro_json["is_self_cleansing"].as_bool().unwrap());

    // 75. LEFM Stress Intensity & Paris Fatigue Crack Growth
    let res_frac = post(
        addr,
        "/api/sim/structural/fracture",
        r#"{
            "max_stress_mpa": 180.0,
            "min_stress_mpa": 18.0,
            "initial_crack_depth_a0_mm": 1.5,
            "plate_width_w_mm": 80.0
        }"#,
    )?;
    assert_eq!(res_frac.status, 200);
    let frac_json = json(&res_frac)?;
    assert!(
        (frac_json["initial_stress_intensity_k1_mpa_m05"]
            .as_f64()
            .unwrap()
            - 13.9)
            .abs()
            < 1.0
    );
    assert!(frac_json["critical_crack_length_ac_mm"].as_f64().unwrap() > 5.0);
    assert!(frac_json["fatigue_cycles_to_failure_nf"].as_f64().unwrap() > 1500.0);
    assert!(
        frac_json["is_safe_against_fast_fracture"]
            .as_bool()
            .unwrap()
    );

    // 76. Universal Commands Execution Verification (5 Advanced Domains)
    for cmd_id in [
        "cad.hardware.shaft_key",
        "sim.pressure_vessel.asme",
        "sim.thermal.psychrometrics",
        "sim.civil.hydrology",
        "sim.structural.fracture",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 77. Enterprise MCP Tool Calling (5 Advanced Domains)
    for tool_name in [
        "atlas_cad_hardware_shaft_key",
        "atlas_sim_pressure_vessel",
        "atlas_sim_psychrometrics",
        "atlas_sim_civil_hydrology",
        "atlas_sim_fracture_lefm",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 120, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 78. Lateral Rotor Dynamics & API 617 Critical Speeds
    let res_rotor = post(
        addr,
        "/api/sim/rotordynamics/api617",
        r#"{
            "operating_speed_rpm": 12000.0,
            "shaft_span_m": 1.20,
            "shaft_diameter_m": 0.080,
            "disc_mass_kg": 45.0,
            "unbalance_g_mm": 15.0
        }"#,
    )?;
    assert_eq!(res_rotor.status, 200);
    let rotor_json = json(&res_rotor)?;
    assert!(rotor_json["critical_speed_rigid_rpm"].as_f64().unwrap() > 3000.0);
    assert!(rotor_json["critical_speed_first_rpm"].as_f64().unwrap() > 2000.0);
    assert!(rotor_json["actual_margin_pct"].as_f64().unwrap() > 20.0);
    assert!(rotor_json["api617_margin_compliant"].as_bool().unwrap());
    assert!(rotor_json["vibration_compliant"].as_bool().unwrap());

    // 79. Welding Metallurgy & Cooling Rates (AWS D1.1 / ISO 15614)
    let res_weld = post(
        addr,
        "/api/cam/welding/metallurgy",
        r#"{
            "arc_voltage_v": 28.0,
            "welding_current_a": 240.0,
            "travel_speed_mm_s": 4.5,
            "plate_thickness_mm": 25.0,
            "preheat_temp_c": 20.0
        }"#,
    )?;
    assert_eq!(res_weld.status, 200);
    let weld_json = json(&res_weld)?;
    assert!(weld_json["gross_heat_input_kj_mm"].as_f64().unwrap() > 1.2);
    assert!(weld_json["net_heat_input_kj_mm"].as_f64().unwrap() > 1.0);
    assert!(weld_json["cooling_time_t8_5_s"].as_f64().unwrap() > 2.0);
    assert!(weld_json["carbon_equivalent_iiw"].as_f64().unwrap() > 0.35);
    assert!(weld_json["haz_hardness_acceptable"].as_bool().unwrap());

    // 80. Binary Distillation Column Design (McCabe-Thiele)
    let res_dist = post(
        addr,
        "/api/sim/chemical/distillation",
        r#"{
            "feed_rate_kmol_hr": 100.0,
            "feed_mole_fraction_zf": 0.45,
            "distillate_mole_fraction_xd": 0.95,
            "bottoms_mole_fraction_xb": 0.05,
            "relative_volatility_alpha": 2.45,
            "reflux_multiplier": 1.30
        }"#,
    )?;
    assert_eq!(res_dist.status, 200);
    let dist_json = json(&res_dist)?;
    assert!(dist_json["fenske_minimum_stages"].as_f64().unwrap() > 5.5);
    assert!(dist_json["minimum_reflux_ratio_rmin"].as_f64().unwrap() > 0.9);
    assert!(dist_json["theoretical_stages_count"].as_u64().unwrap() >= 10);
    assert!(dist_json["condenser_duty_kw"].as_f64().unwrap() > 500.0);

    // 81. PCB Trace Current Carrying Ampacity & Thermal Vias (IPC-2152)
    let res_pcb = post(
        addr,
        "/api/sim/ecad/pcb_thermal",
        r#"{
            "trace_width_mm": 2.50,
            "trace_length_mm": 50.0,
            "current_a": 5.5,
            "allowable_temp_rise_c": 20.0,
            "component_heat_power_w": 3.5
        }"#,
    )?;
    assert_eq!(res_pcb.status, 200);
    let pcb_json = json(&res_pcb)?;
    assert!(pcb_json["allowable_current_ipc2152_a"].as_f64().unwrap() > 6.0);
    assert!(pcb_json["current_margin_factor"].as_f64().unwrap() > 1.0);
    assert!(pcb_json["trace_resistance_mohm"].as_f64().unwrap() > 8.0);
    assert!(
        pcb_json["via_array_thermal_resistance_k_w"]
            .as_f64()
            .unwrap()
            > 5.0
    );
    assert!(pcb_json["thermal_limits_satisfied"].as_bool().unwrap());

    // 82. Structural Steel Bolted & Welded Connections (Eurocode 3 / AISC 360)
    let res_conn = post(
        addr,
        "/api/sim/structural/connections",
        r#"{
            "applied_shear_force_kn": 180.0,
            "applied_tension_force_kn": 80.0,
            "weld_throat_thickness_mm": 6.0,
            "weld_length_mm": 220.0,
            "applied_weld_force_kn": 150.0
        }"#,
    )?;
    assert_eq!(res_conn.status, 200);
    let conn_json = json(&res_conn)?;
    assert!(
        conn_json["single_bolt_shear_resistance_kn"]
            .as_f64()
            .unwrap()
            > 85.0
    );
    assert!(
        conn_json["total_connection_shear_capacity_kn"]
            .as_f64()
            .unwrap()
            > 350.0
    );
    assert!(conn_json["bolt_interaction_utilization"].as_f64().unwrap() < 1.0);
    assert!(conn_json["bolted_joint_safe"].as_bool().unwrap());
    assert!(conn_json["weld_utilization"].as_f64().unwrap() < 1.0);
    assert!(conn_json["welded_joint_safe"].as_bool().unwrap());

    // 83. Universal Commands Execution Verification (Next 5 Advanced Domains)
    for cmd_id in [
        "sim.rotordynamics.api617",
        "cam.welding.metallurgy",
        "sim.chemical.distillation",
        "sim.ecad.pcb_thermal",
        "sim.structural.connections",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 84. Enterprise MCP Tool Calling (Next 5 Advanced Domains)
    for tool_name in [
        "atlas_sim_rotordynamics",
        "atlas_cam_welding_metallurgy",
        "atlas_sim_chemical_distillation",
        "atlas_sim_pcb_thermal",
        "atlas_sim_structural_connections",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 130, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 85. Centrifugal Pumps & Affinity Laws (ISO 9906 / ASME PTC 10)
    let res_pump = post(
        addr,
        "/api/sim/hydraulic/pump",
        r#"{
            "nominal_speed_rpm": 2950.0,
            "nominal_impeller_diameter_mm": 250.0,
            "shutoff_head_m": 85.0,
            "system_static_head_m": 25.0,
            "npsh_available_m": 6.5
        }"#,
    )?;
    assert_eq!(res_pump.status, 200);
    let pump_json = json(&res_pump)?;
    assert!((pump_json["operating_flow_m3_hr"].as_f64().unwrap() - 100.0).abs() < 1.0);
    assert!((pump_json["operating_head_m"].as_f64().unwrap() - 57.0).abs() < 1.0);
    assert!(pump_json["brake_shaft_power_kw"].as_f64().unwrap() > 18.0);
    assert!(pump_json["npsh_margin_m"].as_f64().unwrap() > 3.0);
    assert!(pump_json["cavitation_safe"].as_bool().unwrap());

    // 86. Shell-and-Tube Heat Exchangers (TEMA / Kern's Method)
    let res_hx = post(
        addr,
        "/api/sim/thermal/heat_exchanger_rating",
        r#"{
            "hot_mass_flow_kg_s": 18.0,
            "cold_mass_flow_kg_s": 24.66,
            "shell_diameter_mm": 600.0,
            "tube_count": 380,
            "tube_length_m": 4.5
        }"#,
    )?;
    assert_eq!(res_hx.status, 200);
    let hx_json = json(&res_hx)?;
    assert!((hx_json["heat_duty_kw"].as_f64().unwrap() - 1548.0).abs() < 2.0);
    assert!(hx_json["overall_u_fouled_w_m2_k"].as_f64().unwrap() > 250.0);
    assert!(hx_json["surface_margin_pct"].as_f64().unwrap() > 0.0);
    assert!(hx_json["rating_adequate"].as_bool().unwrap());

    // 87. Geotechnical Shallow Foundation Bearing Capacity (Meyerhof / Eurocode 7)
    let res_bearing = post(
        addr,
        "/api/sim/civil/geotechnical_bearing",
        r#"{
            "foundation_width_b_m": 2.20,
            "foundation_length_l_m": 2.20,
            "embedment_depth_df_m": 1.50,
            "column_axial_load_kn": 1250.0,
            "friction_angle_deg": 32.0
        }"#,
    )?;
    assert_eq!(res_bearing.status, 200);
    let bearing_json = json(&res_bearing)?;
    assert!(
        bearing_json["ultimate_bearing_capacity_qult_kpa"]
            .as_f64()
            .unwrap()
            > 1500.0
    );
    assert!(
        bearing_json["allowable_bearing_capacity_qall_kpa"]
            .as_f64()
            .unwrap()
            > 500.0
    );
    assert!(bearing_json["actual_factor_of_safety"].as_f64().unwrap() > 3.0);
    assert!(bearing_json["estimated_settlement_mm"].as_f64().unwrap() < 25.0);
    assert!(bearing_json["bearing_capacity_adequate"].as_bool().unwrap());
    assert!(bearing_json["settlement_acceptable"].as_bool().unwrap());

    // 88. Multi-Stage Gas Compression & Intercooling (ASME PTC 10)
    let res_comp = post(
        addr,
        "/api/sim/thermal/gas_compression",
        r#"{
            "mass_flow_kg_s": 4.5,
            "inlet_pressure_bar": 1.0,
            "target_outlet_pressure_bar": 30.0,
            "number_of_stages": 3
        }"#,
    )?;
    assert_eq!(res_comp.status, 200);
    let comp_json = json(&res_comp)?;
    assert!((comp_json["stage_pressure_ratio"].as_f64().unwrap() - 3.107).abs() < 0.05);
    assert!(comp_json["total_brake_power_kw"].as_f64().unwrap() > 1200.0);
    assert!(comp_json["total_intercooler_duty_kw"].as_f64().unwrap() > 300.0);
    assert!(comp_json["energy_savings_pct"].as_f64().unwrap() > 15.0);

    // 89. Structural Timber Fire Resistance (Eurocode 5 EN 1995-1-2)
    let res_timber = post(
        addr,
        "/api/sim/structural/timber_fire",
        r#"{
            "initial_breadth_mm": 200.0,
            "initial_depth_mm": 500.0,
            "fire_duration_min": 60.0,
            "ambient_moment_ed_kn_m": 140.0
        }"#,
    )?;
    assert_eq!(res_timber.status, 200);
    let timber_json = json(&res_timber)?;
    assert!((timber_json["effective_char_depth_def_mm"].as_f64().unwrap() - 46.0).abs() < 1.0);
    assert!((timber_json["residual_breadth_bef_mm"].as_f64().unwrap() - 108.0).abs() < 1.0);
    assert!(timber_json["fire_utilization_ratio"].as_f64().unwrap() < 1.0);
    assert!(timber_json["fire_resistance_verified"].as_bool().unwrap());
    assert!(timber_json["critical_fire_endurance_min"].as_f64().unwrap() >= 60.0);

    // 90. Universal Commands Execution Verification (Batch 5 Domains)
    for cmd_id in [
        "sim.hydraulic.pump",
        "sim.thermal.heat_exchanger_rating",
        "sim.civil.geotechnical_bearing",
        "sim.thermal.gas_compression",
        "sim.structural.timber_fire",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 91. Enterprise MCP Tool Calling (Batch 5 Domains)
    for tool_name in [
        "atlas_sim_hydraulic_pump",
        "atlas_sim_heat_exchanger_rating",
        "atlas_sim_geotechnical_bearing",
        "atlas_sim_gas_compression",
        "atlas_sim_timber_fire",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 140, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 92. Marine Propeller Open-Water Hydrodynamics & Cavitation (Wageningen B-Series / ITTC)
    let res_prop = post(
        addr,
        "/api/sim/marine/propeller",
        r#"{
            "diameter_m": 6.80,
            "pitch_ratio_p_d": 1.05,
            "rotational_speed_rpm": 108.0,
            "advance_speed_knots": 17.5
        }"#,
    )?;
    assert_eq!(res_prop.status, 200);
    let prop_json = json(&res_prop)?;
    assert!(prop_json["open_water_efficiency"].as_f64().unwrap() > 0.58);
    assert!(prop_json["thrust_kn"].as_f64().unwrap() > 700.0);
    assert!(prop_json["delivered_power_kw"].as_f64().unwrap() > 10000.0);
    assert!(prop_json["keller_margin"].as_f64().unwrap() > -0.05);

    // 93. Automotive Braking Dynamics & Rotor Thermal Dissipation (ECE R13H / SAE J292)
    let res_brake = post(
        addr,
        "/api/sim/automotive/braking",
        r#"{
            "vehicle_mass_kg": 1850.0,
            "wheelbase_m": 2.85,
            "initial_speed_kph": 100.0,
            "road_friction_coeff": 0.92
        }"#,
    )?;
    assert_eq!(res_brake.status, 200);
    let brake_json = json(&res_brake)?;
    assert!(brake_json["deceleration_g"].as_f64().unwrap() > 0.85);
    assert!(brake_json["stopping_distance_m"].as_f64().unwrap() < 65.0);
    assert!(brake_json["front_wheel_brake_torque_nm"].as_f64().unwrap() > 1500.0);
    assert!(brake_json["rotor_temp_rise_c"].as_f64().unwrap() > 30.0);
    assert!(brake_json["thermal_fade_safe"].as_bool().unwrap());

    // 94. Reinforced Concrete Flexure & Crack Width (Eurocode 2 EN 1992-1-1)
    let res_rc = post(
        addr,
        "/api/sim/civil/reinforced_concrete",
        r#"{
            "breadth_b_mm": 300.0,
            "depth_h_mm": 600.0,
            "applied_moment_ed_kn_m": 240.0,
            "rebar_count": 4,
            "rebar_diameter_mm": 20.0
        }"#,
    )?;
    assert_eq!(res_rc.status, 200);
    let rc_json = json(&res_rc)?;
    assert!(
        rc_json["design_moment_resistance_mrd_kn_m"]
            .as_f64()
            .unwrap()
            > 250.0
    );
    assert!(rc_json["flexural_utilization_ratio"].as_f64().unwrap() < 1.0);
    assert!(
        rc_json["characteristic_crack_width_wk_mm"]
            .as_f64()
            .unwrap()
            <= 0.30
    );
    assert!(rc_json["uls_flexure_adequate"].as_bool().unwrap());
    assert!(rc_json["crack_width_compliant"].as_bool().unwrap());

    // 95. HVAC Airflow, Duct Pressure Loss & Fan Power (ASHRAE / SMACNA)
    let res_duct = post(
        addr,
        "/api/sim/hvac/duct_airflow",
        r#"{
            "airflow_m3_h": 5400.0,
            "duct_width_mm": 600.0,
            "duct_height_mm": 350.0,
            "duct_length_m": 45.0
        }"#,
    )?;
    assert_eq!(res_duct.status, 200);
    let duct_json = json(&res_duct)?;
    assert!(duct_json["air_velocity_m_s"].as_f64().unwrap() > 5.0);
    assert!(duct_json["total_fan_pressure_pa"].as_f64().unwrap() > 200.0);
    assert!(duct_json["fan_brake_power_kw"].as_f64().unwrap() > 0.40);
    assert!(
        duct_json["velocity_within_comfort_limits"]
            .as_bool()
            .unwrap()
    );

    // 96. Aero Gas Turbine Open Brayton Cycle & Thrust (NASA SP-290 / ISO 2314)
    let res_gt = post(
        addr,
        "/api/sim/aerospace/gas_turbine",
        r#"{
            "flight_mach": 0.82,
            "altitude_m": 10668.0,
            "core_mass_flow_kg_s": 45.0,
            "compressor_pressure_ratio": 24.0,
            "turbine_inlet_temp_k": 1650.0
        }"#,
    )?;
    assert_eq!(res_gt.status, 200);
    let gt_json = json(&res_gt)?;
    assert!(gt_json["net_thrust_kn"].as_f64().unwrap() > 18.0);
    assert!(gt_json["nozzle_jet_velocity_m_s"].as_f64().unwrap() > 700.0);
    assert!(gt_json["thermal_efficiency"].as_f64().unwrap() > 0.35);

    // 97. Universal Commands Execution Verification (Batch 6 Domains)
    for cmd_id in [
        "sim.marine.propeller",
        "sim.automotive.braking",
        "sim.civil.reinforced_concrete",
        "sim.hvac.duct_airflow",
        "sim.aerospace.gas_turbine",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 98. Enterprise MCP Tool Calling (Batch 6 Domains)
    for tool_name in [
        "atlas_sim_marine_propeller",
        "atlas_sim_automotive_braking",
        "atlas_sim_civil_reinforced_concrete",
        "atlas_sim_hvac_duct_airflow",
        "atlas_sim_aerospace_gas_turbine",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 150, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 99. Hydraulic Actuator Mechanics, Barrel Stresses & Buckling (ISO 6020-2)
    let res_actuator = post(
        addr,
        "/api/sim/hydraulic/cylinder",
        r#"{
            "bore_diameter_mm": 100.0,
            "rod_diameter_mm": 56.0,
            "stroke_length_mm": 600.0,
            "working_pressure_bar": 210.0,
            "barrel_wall_thickness_mm": 12.0
        }"#,
    )?;
    assert_eq!(res_actuator.status, 200);
    let act_json = json(&res_actuator)?;
    assert!(act_json["push_force_kn"].as_f64().unwrap() > 140.0);
    assert!(act_json["pull_force_kn"].as_f64().unwrap() > 95.0);
    assert!(act_json["barrel_safety_factor"].as_f64().unwrap() > 1.8);
    assert!(act_json["rod_buckling_safety_factor"].as_f64().unwrap() > 2.0);
    assert!(act_json["is_safe"].as_bool().unwrap());

    // 100. Belt & Pulley Power Transmission (ISO 5296 / RMA IP-20)
    let res_belt = post(
        addr,
        "/api/sim/mechanical/belt_drive",
        r#"{
            "power_kw": 15.0,
            "driver_speed_rpm": 1450.0,
            "driver_diameter_mm": 160.0,
            "driven_diameter_mm": 315.0,
            "center_distance_mm": 600.0
        }"#,
    )?;
    assert_eq!(res_belt.status, 200);
    let belt_json = json(&res_belt)?;
    assert!(belt_json["belt_velocity_m_s"].as_f64().unwrap() > 10.0);
    assert!(belt_json["belts_required"].as_u64().unwrap() >= 2);
    assert!(belt_json["shaft_reaction_force_n"].as_f64().unwrap() > 1000.0);
    assert!(belt_json["is_safe"].as_bool().unwrap());

    // 101. Soil Slope Stability Analysis (Bishop's Method of Slices / Eurocode 7)
    let res_slope = post(
        addr,
        "/api/sim/geotechnical/slope_stability",
        r#"{
            "slope_height_m": 10.0,
            "slope_angle_deg": 32.0,
            "soil_unit_weight_kn_m3": 19.0,
            "effective_cohesion_kpa": 12.0,
            "effective_friction_angle_deg": 27.0
        }"#,
    )?;
    assert_eq!(res_slope.status, 200);
    let slope_json = json(&res_slope)?;
    assert!(slope_json["factor_of_safety_bishop"].as_f64().unwrap() > 1.1);
    assert!(slope_json["converged"].as_bool().unwrap());
    assert!(slope_json["active_slices_count"].as_u64().unwrap() > 10);

    // 102. Three-Phase Induction Motor Electromechanics & Torque-Speed (IEEE 112 / NEMA MG-1)
    let res_motor = post(
        addr,
        "/api/sim/electrical/induction_motor",
        r#"{
            "rated_power_kw": 15.0,
            "rated_voltage_ll_v": 400.0,
            "frequency_hz": 50.0,
            "pole_count": 4
        }"#,
    )?;
    assert_eq!(res_motor.status, 200);
    let motor_json = json(&res_motor)?;
    assert_eq!(
        motor_json["synchronous_speed_rpm"].as_f64().unwrap(),
        1500.0
    );
    assert!(motor_json["operating_speed_rpm"].as_f64().unwrap() > 1420.0);
    assert!(motor_json["efficiency_pct"].as_f64().unwrap() > 85.0);
    assert!(motor_json["torque_ratio_breakdown_rated"].as_f64().unwrap() > 1.75);
    assert!(motor_json["is_safe"].as_bool().unwrap());

    // 103. Solar Thermal Collector & Heat Pipe Efficiency (ISO 9806 / EN 12975)
    let res_solar = post(
        addr,
        "/api/sim/thermal/solar_collector",
        r#"{
            "aperture_area_m2": 2.30,
            "solar_irradiance_w_m2": 850.0,
            "ambient_temp_c": 20.0,
            "fluid_inlet_temp_c": 45.0
        }"#,
    )?;
    assert_eq!(res_solar.status, 200);
    let solar_json = json(&res_solar)?;
    assert!(solar_json["collector_efficiency_pct"].as_f64().unwrap() > 50.0);
    assert!(solar_json["useful_thermal_power_w"].as_f64().unwrap() > 900.0);
    assert!(solar_json["stagnation_temperature_c"].as_f64().unwrap() > 130.0);
    assert!(solar_json["is_safe"].as_bool().unwrap());

    // 104. Universal Commands Execution Verification (Batch 7 Domains)
    for cmd_id in [
        "sim.hydraulic.cylinder",
        "sim.mechanical.belt_drive",
        "sim.geotechnical.slope_stability",
        "sim.electrical.induction_motor",
        "sim.thermal.solar_collector",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 105. Enterprise MCP Tool Calling (Batch 7 Domains)
    for tool_name in [
        "atlas_sim_hydraulic_cylinder",
        "atlas_sim_mechanical_belt_drive",
        "atlas_sim_geotechnical_slope_stability",
        "atlas_sim_electrical_induction_motor",
        "atlas_sim_thermal_solar_collector",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 160, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 106. Hydrodynamic Journal Bearing (ISO 7902 / DIN 31652)
    let res_bearing = post(
        addr,
        "/api/sim/tribology/journal_bearing",
        r#"{
            "journal_diameter_mm": 100.0,
            "bearing_length_mm": 70.0,
            "radial_clearance_um": 65.0,
            "rotational_speed_rpm": 3000.0,
            "radial_load_n": 12500.0,
            "lubricant_viscosity_mpa_s": 25.0
        }"#,
    )?;
    assert_eq!(res_bearing.status, 200);
    let bearing_json = json(&res_bearing)?;
    assert!(bearing_json["sommerfeld_number"].as_f64().unwrap() > 0.3);
    assert!(bearing_json["minimum_film_thickness_um"].as_f64().unwrap() > 20.0);
    assert!(bearing_json["hydrodynamic_power_loss_kw"].as_f64().unwrap() > 1.0);
    assert!(bearing_json["is_safe"].as_bool().unwrap());

    // 107. Wind Turbine Aerodynamics & BEM Power Curve (IEC 61400-1)
    let res_wind = post(
        addr,
        "/api/sim/renewable/wind_turbine",
        r#"{
            "rotor_radius_m": 63.0,
            "rated_power_mw": 4.0,
            "rated_wind_speed_m_s": 11.5,
            "tip_speed_ratio_optimal": 7.8
        }"#,
    )?;
    assert_eq!(res_wind.status, 200);
    let wind_json = json(&res_wind)?;
    assert!(wind_json["rated_rotor_speed_rpm"].as_f64().unwrap() > 10.0);
    assert!(wind_json["peak_aerodynamic_cp"].as_f64().unwrap() > 0.40);
    assert!(wind_json["annual_energy_production_gwh"].as_f64().unwrap() > 10.0);
    assert!(wind_json["is_safe"].as_bool().unwrap());

    // 108. Steam Rankine Power Cycle & Condenser Thermodynamics (IAPWS-IF97 / ASME PTC 6)
    let res_rankine = post(
        addr,
        "/api/sim/thermal/steam_rankine",
        r#"{
            "steam_mass_flow_kg_s": 120.0,
            "boiler_pressure_bar": 120.0,
            "superheater_temp_c": 540.0,
            "condenser_pressure_bar": 0.08
        }"#,
    )?;
    assert_eq!(res_rankine.status, 200);
    let rankine_json = json(&res_rankine)?;
    assert!(rankine_json["gross_electric_power_mw"].as_f64().unwrap() > 100.0);
    assert!(
        rankine_json["cycle_thermal_efficiency_pct"]
            .as_f64()
            .unwrap()
            > 30.0
    );
    assert!(
        rankine_json["cooling_water_flow_rate_kg_s"]
            .as_f64()
            .unwrap()
            > 3000.0
    );
    assert!(rankine_json["is_safe"].as_bool().unwrap());

    // 109. Deep Foundation Bored Pile Load Capacity & Settlement (Eurocode 7 / API RP 2A)
    let res_pile = post(
        addr,
        "/api/sim/civil/pile_foundation",
        r#"{
            "pile_diameter_m": 1.0,
            "pile_length_m": 20.0,
            "applied_axial_load_kn": 3200.0
        }"#,
    )?;
    assert_eq!(res_pile.status, 200);
    let pile_json = json(&res_pile)?;
    assert!(pile_json["ultimate_total_capacity_kn"].as_f64().unwrap() > 5000.0);
    assert!(pile_json["design_bearing_resistance_kn"].as_f64().unwrap() > 4000.0);
    assert!(pile_json["total_working_settlement_mm"].as_f64().unwrap() < 15.0);
    assert!(pile_json["is_safe"].as_bool().unwrap());

    // 110. Involute Gear Tooth Pitting & Root Bending Fatigue (ISO 6336 Method B / AGMA 2101)
    let res_gear = post(
        addr,
        "/api/sim/mechanical/gear_fatigue",
        r#"{
            "transmitted_power_kw": 55.0,
            "pinion_speed_rpm": 1450.0,
            "pinion_teeth_z1": 21,
            "gear_teeth_z2": 63,
            "normal_module_mm": 4.0,
            "face_width_mm": 50.0
        }"#,
    )?;
    assert_eq!(res_gear.status, 200);
    let gear_json = json(&res_gear)?;
    assert!(gear_json["safety_factor_pitting"].as_f64().unwrap() >= 1.25);
    assert!(gear_json["safety_factor_bending"].as_f64().unwrap() >= 1.40);
    assert!(gear_json["tangential_force_n"].as_f64().unwrap() > 5000.0);
    assert!(gear_json["is_safe"].as_bool().unwrap());

    // 111. Universal Commands Execution Verification (Batch 8 Domains)
    for cmd_id in [
        "sim.tribology.journal_bearing",
        "sim.renewable.wind_turbine",
        "sim.thermal.steam_rankine",
        "sim.civil.pile_foundation",
        "sim.mechanical.gear_fatigue",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 112. Enterprise MCP Tool Calling (Batch 8 Domains)
    for tool_name in [
        "atlas_sim_tribology_journal_bearing",
        "atlas_sim_renewable_wind_turbine",
        "atlas_sim_thermal_steam_rankine",
        "atlas_sim_civil_pile_foundation",
        "atlas_sim_mechanical_gear_fatigue",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 161, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 113. Battery Pack Liquid Cooling & Thermal Management (IEC 62660 / SAE J2464)
    let res_bat = post(
        addr,
        "/api/sim/energy/battery_thermal",
        r#"{
            "cell_count_series": 96,
            "cell_count_parallel": 4,
            "operating_current_a": 200.0,
            "coolant_flow_rate_l_min": 15.0
        }"#,
    )?;
    assert_eq!(res_bat.status, 200);
    let bat_json = json(&res_bat)?;
    assert!(bat_json["total_pack_energy_kwh"].as_f64().unwrap() > 60.0);
    assert!(bat_json["total_pack_heat_dissipation_w"].as_f64().unwrap() > 1500.0);
    assert!(bat_json["maximum_cell_temp_c"].as_f64().unwrap() < 50.0);
    assert!(bat_json["is_safe"].as_bool().unwrap());

    // 114. Rocket Engine De Laval Nozzle Supersonic Gas Dynamics (NASA SP-8039)
    let res_rocket = post(
        addr,
        "/api/sim/aerospace/rocket_nozzle",
        r#"{
            "chamber_pressure_bar": 100.0,
            "chamber_temperature_k": 3500.0,
            "throat_diameter_mm": 120.0,
            "expansion_ratio_eps": 16.0
        }"#,
    )?;
    assert_eq!(res_rocket.status, 200);
    let rkt_json = json(&res_rocket)?;
    assert!(rkt_json["exit_mach_number"].as_f64().unwrap() > 3.0);
    assert!(rkt_json["net_thrust_kn"].as_f64().unwrap() > 140.0);
    assert!(rkt_json["operating_specific_impulse_s"].as_f64().unwrap() > 250.0);
    assert!(rkt_json["is_safe"].as_bool().unwrap());

    // 115. CSTR Chemical Reactor Kinetics & Thermal Multiplicity (Fogler / DIERS)
    let res_cstr = post(
        addr,
        "/api/sim/chemical/cstr_reactor",
        r#"{
            "reactor_volume_m3": 2.50,
            "feed_volumetric_flow_m3_h": 5.0,
            "feed_reactant_concentration_mol_m3": 2000.0
        }"#,
    )?;
    assert_eq!(res_cstr.status, 200);
    let cstr_json = json(&res_cstr)?;
    assert!(cstr_json["fractional_conversion"].as_f64().unwrap() > 0.50);
    assert!(cstr_json["heat_generation_rate_kw"].as_f64().unwrap() > 80.0);
    assert!(cstr_json["thermal_stability_margin_w_k"].as_f64().unwrap() > 0.0);
    assert!(cstr_json["is_safe"].as_bool().unwrap());

    // 116. Open Channel Manning Hydraulics & Hydraulic Jump (Chow / USBR)
    let res_hyd = post(
        addr,
        "/api/sim/civil/channel_hydraulics",
        r#"{
            "bottom_width_m": 6.0,
            "side_slope_z": 1.5,
            "discharge_rate_m3_s": 45.0,
            "bed_slope_s0": 0.0015
        }"#,
    )?;
    assert_eq!(res_hyd.status, 200);
    let hyd_json = json(&res_hyd)?;
    assert!(hyd_json["normal_depth_m"].as_f64().unwrap() > 1.5);
    assert!(hyd_json["critical_depth_m"].as_f64().unwrap() > 1.0);
    assert!(hyd_json["jump_head_loss_m"].as_f64().unwrap() > 0.5);
    assert!(hyd_json["is_safe"].as_bool().unwrap());

    // 117. Overhead Transmission Line Catenary Sag & Tension (IEEE 738 / IEC 60826)
    let res_line = post(
        addr,
        "/api/sim/electrical/transmission_line",
        r#"{
            "span_length_m": 350.0,
            "conductor_diameter_mm": 28.14,
            "rated_tensile_strength_kn": 140.0,
            "initial_horizontal_tension_kn": 28.0
        }"#,
    )?;
    assert_eq!(res_line.status, 200);
    let line_json = json(&res_line)?;
    assert!(line_json["everyday_sag_m"].as_f64().unwrap() > 7.0);
    assert!(line_json["max_hot_sag_m"].as_f64().unwrap() > 9.0);
    assert!(line_json["storm_safety_factor"].as_f64().unwrap() >= 2.0);
    assert!(line_json["is_safe"].as_bool().unwrap());

    // 118. Universal Commands Execution Verification (Batch 9 Domains)
    for cmd_id in [
        "sim.energy.battery_thermal",
        "sim.aerospace.rocket_nozzle",
        "sim.chemical.cstr_reactor",
        "sim.civil.channel_hydraulics",
        "sim.electrical.transmission_line",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 119. Enterprise MCP Tool Calling (Batch 9 Domains)
    for tool_name in [
        "atlas_sim_energy_battery_thermal",
        "atlas_sim_aerospace_rocket_nozzle",
        "atlas_sim_chemical_cstr_reactor",
        "atlas_sim_civil_channel_hydraulics",
        "atlas_sim_electrical_transmission_line",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 162, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 120. Piping Flexibility & Thermal Expansion Stress Analysis (ASME B31.3 / B31.1)
    let res_piping = post(
        addr,
        "/api/sim/piping/flexibility",
        r#"{
            "nominal_outside_diameter_mm": 219.1,
            "anchor_distance_m": 60.0,
            "design_temperature_c": 260.0,
            "loop_height_m": 7.5,
            "loop_width_m": 5.0
        }"#,
    )?;
    assert_eq!(res_piping.status, 200);
    let piping_json = json(&res_piping)?;
    assert!(piping_json["thermal_growth_mm"].as_f64().unwrap() > 100.0);
    assert!(piping_json["displacement_stress_se_mpa"].as_f64().unwrap() > 50.0);
    assert!(piping_json["stress_ratio"].as_f64().unwrap() <= 1.0);
    assert!(piping_json["is_safe"].as_bool().unwrap());

    // 121. Automotive Suspension Kinematics & Quarter-Car Ride Dynamics (ISO 2631 / Milliken)
    let res_qc = post(
        addr,
        "/api/sim/automotive/quarter_car",
        r#"{
            "sprung_mass_kg": 320.0,
            "unsprung_mass_kg": 45.0,
            "suspension_stiffness_n_m": 28000.0,
            "suspension_damping_n_s_m": 1650.0,
            "vehicle_speed_km_h": 100.0,
            "road_class": "ClassBGood"
        }"#,
    )?;
    assert_eq!(res_qc.status, 200);
    let qc_json = json(&res_qc)?;
    assert!(qc_json["body_bounce_frequency_hz"].as_f64().unwrap() > 1.0);
    assert!(qc_json["wheel_hop_frequency_hz"].as_f64().unwrap() > 8.0);
    assert!(qc_json["body_acceleration_rms_m_s2"].as_f64().unwrap() > 0.1);
    assert!(qc_json["is_safe"].as_bool().unwrap());

    // 122. Power Transformer Electromagnetics & Thermal Rating (IEC 60076 / IEEE C57)
    let res_trans = post(
        addr,
        "/api/sim/electrical/power_transformer",
        r#"{
            "rated_apparent_power_mva": 40.0,
            "primary_voltage_kv": 110.0,
            "secondary_voltage_kv": 33.0,
            "percent_impedance_z": 10.5,
            "no_load_loss_kw": 22.0,
            "load_loss_kw": 165.0
        }"#,
    )?;
    assert_eq!(res_trans.status, 200);
    let trans_json = json(&res_trans)?;
    assert!(trans_json["operating_efficiency_pct"].as_f64().unwrap() > 99.0);
    assert!(trans_json["hot_spot_temperature_c"].as_f64().unwrap() > 80.0);
    assert!(
        trans_json["relative_insulation_aging_factor"]
            .as_f64()
            .unwrap()
            < 2.0
    );
    assert!(trans_json["is_safe"].as_bool().unwrap());

    // 123. Vapor Compression Refrigeration Cycle & Heat Pump COP (ASHRAE / AHRI 550/590)
    let res_ref = post(
        addr,
        "/api/sim/thermal/refrigeration_cycle",
        r#"{
            "refrigerant": "R410A",
            "design_cooling_capacity_kw": 100.0,
            "evaporating_temperature_c": 5.0,
            "condensing_temperature_c": 45.0
        }"#,
    )?;
    assert_eq!(res_ref.status, 200);
    let ref_json = json(&res_ref)?;
    assert!(
        ref_json["coefficient_of_performance_cooling"]
            .as_f64()
            .unwrap()
            > 2.0
    );
    assert!(ref_json["compressor_electrical_power_kw"].as_f64().unwrap() > 15.0);
    assert!(ref_json["is_safe"].as_bool().unwrap());

    // 124. Ship Hydrostatic Intact Stability & GZ Righting Arm (IMO Resolution MSC.267(85))
    let res_ship = post(
        addr,
        "/api/sim/marine/ship_stability",
        r#"{
            "length_between_perpendiculars_m": 140.0,
            "molded_beam_m": 22.0,
            "design_draft_m": 7.5,
            "vertical_center_of_gravity_kg_m": 7.20
        }"#,
    )?;
    assert_eq!(res_ship.status, 200);
    let ship_json = json(&res_ship)?;
    assert!(ship_json["displacement_mass_tonnes"].as_f64().unwrap() > 15000.0);
    assert!(
        ship_json["initial_metacentric_height_gm0_m"]
            .as_f64()
            .unwrap()
            > 1.0
    );
    assert!(ship_json["max_gz_righting_arm_m"].as_f64().unwrap() > 0.5);
    assert!(ship_json["is_safe"].as_bool().unwrap());

    // 125. Universal Commands Execution Verification (Batch 10 Domains)
    for cmd_id in [
        "sim.piping.flexibility",
        "sim.automotive.quarter_car",
        "sim.electrical.power_transformer",
        "sim.thermal.refrigeration_cycle",
        "sim.marine.ship_stability",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 126. Enterprise MCP Tool Calling (Batch 10 Domains)
    for tool_name in [
        "atlas_sim_piping_flexibility",
        "atlas_sim_automotive_quarter_car",
        "atlas_sim_electrical_power_transformer",
        "atlas_sim_thermal_refrigeration_cycle",
        "atlas_sim_marine_ship_stability",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 163, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 127. Plastic Injection Molding Process Physics & Clamp Tonnage (ISO 294 / SPE)
    let res_molding = post(
        addr,
        "/api/sim/manufacturing/injection_molding",
        r#"{
            "resin": "ABS",
            "nominal_wall_thickness_mm": 2.50,
            "projected_area_cm2": 180.0,
            "cavity_volume_cm3": 112.5,
            "max_flow_length_mm": 175.0,
            "cavity_count": 2
        }"#,
    )?;
    assert_eq!(res_molding.status, 200);
    let mold_json = json(&res_molding)?;
    assert!(mold_json["cooling_time_s"].as_f64().unwrap() > 5.0);
    assert!(mold_json["peak_cavity_pressure_bar"].as_f64().unwrap() > 200.0);
    assert!(mold_json["recommended_clamp_tonnage_t"].as_f64().unwrap() > 80.0);
    assert!(mold_json["is_safe"].as_bool().unwrap());

    // 128. Structural Seismic Response Spectrum & Base Shear (ASCE 7-22 / Eurocode 8)
    let res_seismic = post(
        addr,
        "/api/sim/civil/seismic_analysis",
        r#"{
            "building_height_m": 36.0,
            "story_count": 10,
            "total_seismic_weight_kn": 58000.0,
            "short_period_spectral_accel_ss_g": 1.20,
            "one_second_spectral_accel_s1_g": 0.48
        }"#,
    )?;
    assert_eq!(res_seismic.status, 200);
    let seis_json = json(&res_seismic)?;
    assert!(seis_json["fundamental_period_ta_s"].as_f64().unwrap() > 0.8);
    assert!(seis_json["total_base_shear_kn"].as_f64().unwrap() > 2000.0);
    assert!(seis_json["base_overturning_moment_kn_m"].as_f64().unwrap() > 50000.0);
    assert!(seis_json["is_safe"].as_bool().unwrap());

    // 129. Spacecraft Orbital Mechanics & Hohmann Transfer (Curtis / Vallado)
    let res_orbit = post(
        addr,
        "/api/sim/aerospace/orbital_mechanics",
        r#"{
            "initial_altitude_km": 400.0,
            "target_altitude_km": 35786.0,
            "spacecraft_wet_mass_kg": 3500.0,
            "propulsion_isp_s": 318.0
        }"#,
    )?;
    assert_eq!(res_orbit.status, 200);
    let orbit_json = json(&res_orbit)?;
    assert!(orbit_json["initial_orbital_speed_m_s"].as_f64().unwrap() > 7600.0);
    assert!(orbit_json["total_delta_v_m_s"].as_f64().unwrap() > 3800.0);
    assert!(orbit_json["transfer_time_of_flight_h"].as_f64().unwrap() > 5.0);
    assert!(orbit_json["required_propellant_mass_kg"].as_f64().unwrap() > 2000.0);
    assert!(orbit_json["is_safe"].as_bool().unwrap());

    // 130. Synchronous DC-DC Buck Converter Power Stage & Efficiency (IEEE / Erickson)
    let res_smps = post(
        addr,
        "/api/sim/electronics/dc_dc_converter",
        r#"{
            "input_voltage_v": 12.0,
            "output_voltage_v": 3.3,
            "rated_output_current_a": 20.0,
            "switching_frequency_hz": 500000.0
        }"#,
    )?;
    assert_eq!(res_smps.status, 200);
    let smps_json = json(&res_smps)?;
    assert!(smps_json["duty_cycle"].as_f64().unwrap() > 0.25);
    assert!(smps_json["output_power_w"].as_f64().unwrap() > 60.0);
    assert!(smps_json["efficiency_pct"].as_f64().unwrap() > 90.0);
    assert!(smps_json["is_safe"].as_bool().unwrap());

    // 131. Nuclear Fusion Tokamak Plasma Equilibrium & Confinement (Wesson / ITER)
    let res_fusion = post(
        addr,
        "/api/sim/science/fusion_tokamak",
        r#"{
            "major_radius_r0_m": 6.20,
            "minor_radius_a_m": 2.00,
            "toroidal_magnetic_field_t": 5.30,
            "plasma_current_ma": 15.0,
            "electron_density_10e20_m3": 1.01,
            "plasma_temperature_kev": 13.0
        }"#,
    )?;
    assert_eq!(res_fusion.status, 200);
    let fusion_json = json(&res_fusion)?;
    assert!(fusion_json["safety_factor_q95"].as_f64().unwrap() > 2.8);
    assert!(fusion_json["stored_thermal_energy_mj"].as_f64().unwrap() > 200.0);
    assert!(fusion_json["fusion_power_mw"].as_f64().unwrap() > 300.0);
    assert!(fusion_json["fusion_gain_factor_q"].as_f64().unwrap() > 5.0);
    assert!(fusion_json["is_safe"].as_bool().unwrap());

    // 132. Universal Commands Execution Verification (Batch 11 Domains)
    for cmd_id in [
        "sim.manufacturing.injection_molding",
        "sim.civil.seismic_analysis",
        "sim.aerospace.orbital_mechanics",
        "sim.electronics.dc_dc_converter",
        "sim.science.fusion_tokamak",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 133. Enterprise MCP Tool Calling (Batch 11 Domains)
    for tool_name in [
        "atlas_sim_manufacturing_injection_molding",
        "atlas_sim_civil_seismic_analysis",
        "atlas_sim_aerospace_orbital_mechanics",
        "atlas_sim_electronics_dc_dc_converter",
        "atlas_sim_science_fusion_tokamak",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 164, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 134. Wire Electrical Discharge Machining (Wire EDM) Spark Erosion Physics (ISO 230 / CIRP)
    let res_edm = post(
        addr,
        "/api/sim/manufacturing/wire_edm",
        r#"{
            "workpiece_thickness_mm": 50.0,
            "wire_diameter_mm": 0.25,
            "open_circuit_voltage_v": 95.0,
            "discharge_current_a": 16.0,
            "pulse_on_time_us": 4.5,
            "pulse_off_time_us": 12.0
        }"#,
    )?;
    assert_eq!(res_edm.status, 200);
    let edm_json = json(&res_edm)?;
    assert!(edm_json["single_pulse_energy_mj"].as_f64().unwrap() > 1.0);
    assert!(edm_json["cutting_speed_mm_min"].as_f64().unwrap() > 1.0);
    assert!(edm_json["volumetric_mrr_mm3_min"].as_f64().unwrap() > 20.0);
    assert!(edm_json["kerf_width_mm"].as_f64().unwrap() > 0.28);
    assert!(edm_json["surface_roughness_ra_um"].as_f64().unwrap() > 1.0);
    assert!(edm_json["max_wire_deflection_um"].as_f64().unwrap() < 35.0);
    assert!(edm_json["is_safe"].as_bool().unwrap());

    // 135. Wind Turbine Blade Aerodynamic Bending & Flutter (IEC 61400-1 / Theodorsen)
    let res_wind = post(
        addr,
        "/api/sim/renewable/wind_blade_aeroelastic",
        r#"{
            "blade_length_m": 80.0,
            "rotor_radius_m": 82.5,
            "rated_rotor_speed_rpm": 10.5,
            "rated_wind_speed_m_s": 11.4,
            "extreme_gust_speed_m_s": 52.5
        }"#,
    )?;
    assert_eq!(res_wind.status, 200);
    let wind_json = json(&res_wind)?;
    assert!(wind_json["standstill_flap_freq_hz"].as_f64().unwrap() > 0.40);
    assert!(
        wind_json["rotating_flap_freq_hz"].as_f64().unwrap()
            > wind_json["standstill_flap_freq_hz"].as_f64().unwrap()
    );
    assert!(wind_json["centrifugal_stiffening_ratio"].as_f64().unwrap() > 1.05);
    assert!(wind_json["torsional_freq_hz"].as_f64().unwrap() > 2.0);
    assert!(wind_json["flutter_speed_m_s"].as_f64().unwrap() > 100.0);
    assert!(wind_json["flutter_safety_margin"].as_f64().unwrap() > 1.25);
    assert!(wind_json["tip_gust_deflection_m"].as_f64().unwrap() > 1.5);
    assert!(wind_json["is_stable"].as_bool().unwrap());

    // 136. Subsea Offshore Pipeline Wall Thickness & Hydrostatic Collapse (DNV-ST-F101 / API 5L)
    let res_pipeline = post(
        addr,
        "/api/sim/marine/subsea_pipeline",
        r#"{
            "outside_diameter_mm": 508.0,
            "nominal_wall_thickness_mm": 25.4,
            "corrosion_allowance_mm": 3.0,
            "fabrication_ovality_pct": 0.50,
            "water_depth_m": 1500.0,
            "internal_design_pressure_mpa": 22.0
        }"#,
    )?;
    assert_eq!(res_pipeline.status, 200);
    let pipe_json = json(&res_pipeline)?;
    assert!(pipe_json["hydrostatic_pressure_mpa"].as_f64().unwrap() > 14.0);
    assert!(
        pipe_json["characteristic_collapse_pressure_mpa"]
            .as_f64()
            .unwrap()
            > 20.0
    );
    assert!(
        pipe_json["design_collapse_resistance_mpa"]
            .as_f64()
            .unwrap()
            > 14.0
    );
    assert!(pipe_json["collapse_safety_factor"].as_f64().unwrap() > 1.30);
    assert!(pipe_json["internal_burst_pressure_mpa"].as_f64().unwrap() > 35.0);
    assert!(pipe_json["burst_safety_factor"].as_f64().unwrap() > 1.50);
    assert!(pipe_json["is_compliant"].as_bool().unwrap());

    // 137. Gas Turbine Combustor Aerothermal Performance & NOx Emissions (Lefebvre / ICAO)
    let res_combustor = post(
        addr,
        "/api/sim/aerospace/combustor_aerothermal",
        r#"{
            "inlet_total_pressure_mpa": 2.40,
            "inlet_total_temperature_k": 780.0,
            "air_mass_flow_kg_s": 38.0,
            "fuel_mass_flow_kg_s": 0.76
        }"#,
    )?;
    assert_eq!(res_combustor.status, 200);
    let comb_json = json(&res_combustor)?;
    assert!(comb_json["combustion_efficiency_pct"].as_f64().unwrap() > 99.0);
    assert!(comb_json["total_pressure_loss_pct"].as_f64().unwrap() > 3.0);
    assert!(comb_json["total_pressure_loss_pct"].as_f64().unwrap() < 6.0);
    assert!(comb_json["turbine_inlet_temperature_k"].as_f64().unwrap() > 1450.0);
    assert!(comb_json["pattern_factor"].as_f64().unwrap() < 0.25);
    assert!(comb_json["nox_emissions_index_g_kg"].as_f64().unwrap() > 5.0);
    assert!(comb_json["is_acceptable"].as_bool().unwrap());

    // 138. Battery Energy Storage System Container & Grid RTE (IEC 62933 / IEEE 2030.2.1)
    let res_bess = post(
        addr,
        "/api/sim/energy/bess_grid",
        r#"{
            "nominal_capacity_mwh": 10.0,
            "rated_power_mw": 2.50,
            "c_rate": 0.25,
            "ambient_temperature_c": 28.0
        }"#,
    )?;
    assert_eq!(res_bess.status, 200);
    let bess_json = json(&res_bess)?;
    assert_eq!(bess_json["discharge_duration_hours"].as_f64().unwrap(), 4.0);
    assert!(bess_json["battery_dc_efficiency_pct"].as_f64().unwrap() > 93.0);
    assert!(bess_json["pcs_efficiency_pct"].as_f64().unwrap() > 97.0);
    assert!(bess_json["transformer_efficiency_pct"].as_f64().unwrap() > 98.5);
    assert!(bess_json["ac_to_ac_rte_pct"].as_f64().unwrap() > 83.0);
    assert!(bess_json["is_grid_compliant"].as_bool().unwrap());

    // 139. Universal Commands Execution Verification (Batch 12 Domains)
    for cmd_id in [
        "sim.manufacturing.wire_edm",
        "sim.renewable.wind_blade_aeroelastic",
        "sim.marine.subsea_pipeline",
        "sim.aerospace.combustor_aerothermal",
        "sim.energy.bess_grid",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 140. Enterprise MCP Tool Calling (Batch 12 Domains)
    for tool_name in [
        "atlas_sim_manufacturing_wire_edm",
        "atlas_sim_renewable_wind_blade_aeroelastic",
        "atlas_sim_marine_subsea_pipeline",
        "atlas_sim_aerospace_combustor_aerothermal",
        "atlas_sim_energy_bess_grid",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 165, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 141. Soil Consolidation & Settlement (Terzaghi 1D / ASTM D2435 / Eurocode 7)
    let res_soil = post(
        addr,
        "/api/sim/geotechnical/soil_consolidation",
        r#"{
            "layer_thickness_m": 6.0,
            "initial_void_ratio": 0.85,
            "compression_index_cc": 0.32,
            "recompression_index_cr": 0.045,
            "preconsolidation_stress_kpa": 120.0,
            "initial_effective_stress_kpa": 100.0,
            "applied_stress_increment_kpa": 80.0,
            "coefficient_of_consolidation_m2_yr": 2.80,
            "analysis_time_years": 2.0
        }"#,
    )?;
    assert_eq!(res_soil.status, 200);
    let soil_json = json(&res_soil)?;
    assert_eq!(
        soil_json["over_consolidation_ratio"].as_f64().unwrap(),
        1.20
    );
    assert!(
        soil_json["ultimate_primary_settlement_mm"]
            .as_f64()
            .unwrap()
            > 150.0
    );
    assert!(soil_json["time_factor_tv"].as_f64().unwrap() > 0.50);
    assert!(soil_json["degree_of_consolidation_pct"].as_f64().unwrap() > 75.0);
    assert!(soil_json["settlement_at_time_mm"].as_f64().unwrap() > 120.0);
    assert!(soil_json["is_acceptable"].as_bool().unwrap());

    // 142. Hydraulic Turbine Energy Conversion (IEC 60193 / Euler Equation)
    let res_hydro = post(
        addr,
        "/api/sim/energy/hydraulic_turbine",
        r#"{
            "net_head_m": 450.0,
            "flow_rate_m3_s": 3.20,
            "rotational_speed_rpm": 500.0,
            "runner_diameter_m": 1.80,
            "nozzle_count": 2
        }"#,
    )?;
    assert_eq!(res_hydro.status, 200);
    let hydro_json = json(&res_hydro)?;
    assert!(hydro_json["available_hydraulic_power_mw"].as_f64().unwrap() > 13.0);
    assert!(hydro_json["jet_velocity_m_s"].as_f64().unwrap() > 90.0);
    assert!(hydro_json["speed_ratio_phi"].as_f64().unwrap() > 0.45);
    assert!(hydro_json["shaft_power_mw"].as_f64().unwrap() > 12.0);
    assert!(hydro_json["hydraulic_efficiency_pct"].as_f64().unwrap() > 88.0);
    assert!(hydro_json["is_cavitation_safe"].as_bool().unwrap());

    // 143. Cardiovascular Stent Biomechanics & Radial Expansion (ASTM F3067 / FDA)
    let res_stent = post(
        addr,
        "/api/sim/biomedical/stent_expansion",
        r#"{
            "crimped_diameter_mm": 1.05,
            "nominal_expanded_diameter_mm": 3.00,
            "nominal_length_mm": 18.0,
            "strut_thickness_um": 81.0,
            "strut_width_um": 84.0,
            "crown_count_per_ring": 8,
            "ring_count": 12,
            "balloon_pressure_atm": 12.0
        }"#,
    )?;
    assert_eq!(res_stent.status, 200);
    let stent_json = json(&res_stent)?;
    assert!(stent_json["expansion_ratio"].as_f64().unwrap() > 2.5);
    assert!(stent_json["inflated_outer_diameter_mm"].as_f64().unwrap() > 3.10);
    assert!(stent_json["post_recoil_diameter_mm"].as_f64().unwrap() > 3.00);
    assert!(stent_json["elastic_recoil_pct"].as_f64().unwrap() <= 5.0);
    assert!(stent_json["foreshortening_pct"].as_f64().unwrap() <= 8.0);
    assert!(stent_json["radial_resistive_force_n_mm"].as_f64().unwrap() > 0.70);
    assert!(stent_json["fracture_safety_factor"].as_f64().unwrap() >= 1.50);
    assert!(stent_json["is_fda_compliant"].as_bool().unwrap());

    // 144. Microwave Rectangular Waveguide & Cavity Resonator (IEEE Std 148 / Pozar)
    let res_wave = post(
        addr,
        "/api/sim/electronics/microwave_waveguide",
        r#"{
            "broad_dimension_a_mm": 22.86,
            "narrow_dimension_b_mm": 10.16,
            "length_l_mm": 100.0,
            "operating_frequency_ghz": 10.0,
            "transmitted_power_w": 500.0
        }"#,
    )?;
    assert_eq!(res_wave.status, 200);
    let wave_json = json(&res_wave)?;
    assert!(wave_json["cutoff_frequency_te10_ghz"].as_f64().unwrap() > 6.50);
    assert!(wave_json["cutoff_frequency_te20_ghz"].as_f64().unwrap() > 13.0);
    assert!(wave_json["is_single_mode"].as_bool().unwrap());
    assert!(wave_json["guide_wavelength_mm"].as_f64().unwrap() > 39.0);
    assert!(wave_json["wave_impedance_ohms"].as_f64().unwrap() > 490.0);
    assert!(wave_json["attenuation_rate_db_m"].as_f64().unwrap() > 0.08);
    assert!(wave_json["cavity_unloaded_q"].as_f64().unwrap() > 5000.0);

    // 145. Gas-Insulated Switchgear High-Voltage Dielectric Breakdown (IEC 62271-203)
    let res_gis = post(
        addr,
        "/api/sim/electrical/gis_dielectric",
        r#"{
            "rated_voltage_kv_rms": 420.0,
            "conductor_radius_mm": 60.0,
            "enclosure_radius_mm": 165.0,
            "gas_pressure_abs_bar": 5.50,
            "lightning_impulse_withstand_kv_pk": 1425.0
        }"#,
    )?;
    assert_eq!(res_gis.status, 200);
    let gis_json = json(&res_gis)?;
    assert_eq!(gis_json["radius_ratio"].as_f64().unwrap(), 2.75);
    assert!(gis_json["field_utilization_factor"].as_f64().unwrap() > 0.55);
    assert!(gis_json["max_continuous_e_field_kv_mm"].as_f64().unwrap() > 5.0);
    assert!(gis_json["max_impulse_e_field_kv_mm"].as_f64().unwrap() > 22.0);
    assert!(gis_json["continuous_safety_factor"].as_f64().unwrap() > 5.0);
    assert!(gis_json["impulse_safety_factor"].as_f64().unwrap() >= 1.25);
    assert!(gis_json["is_iec_compliant"].as_bool().unwrap());

    // 146. Universal Commands Execution Verification (Batch 13 Domains)
    for cmd_id in [
        "sim.geotechnical.soil_consolidation",
        "sim.energy.hydraulic_turbine",
        "sim.biomedical.stent_expansion",
        "sim.electronics.microwave_waveguide",
        "sim.electrical.gis_dielectric",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 147. Enterprise MCP Tool Calling (Batch 13 Domains)
    for tool_name in [
        "atlas_sim_geotechnical_soil_consolidation",
        "atlas_sim_energy_hydraulic_turbine",
        "atlas_sim_biomedical_stent_expansion",
        "atlas_sim_electronics_microwave_waveguide",
        "atlas_sim_electrical_gis_dielectric",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 166, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 148. Proton Exchange Membrane Fuel Cell (PEMFC) Polarization (Larminie / Barbir)
    let res_pem = post(
        addr,
        "/api/sim/energy/pem_fuel_cell",
        r#"{
            "cell_count": 360,
            "active_cell_area_cm2": 280.0,
            "operating_temperature_c": 75.0,
            "anode_pressure_bar": 2.20,
            "cathode_pressure_bar": 2.00,
            "operating_current_density_a_cm2": 1.20
        }"#,
    )?;
    assert_eq!(res_pem.status, 200);
    let pem_json = json(&res_pem)?;
    assert!(pem_json["nernst_potential_v"].as_f64().unwrap() > 1.15);
    assert!(pem_json["cell_voltage_v"].as_f64().unwrap() > 0.65);
    assert!(pem_json["stack_power_kw"].as_f64().unwrap() > 70.0);
    assert!(pem_json["hydrogen_consumption_kg_h"].as_f64().unwrap() > 4.0);
    assert!(pem_json["electrical_efficiency_lhv_pct"].as_f64().unwrap() > 50.0);
    assert!(pem_json["is_operable"].as_bool().unwrap());

    // 149. Cryogenic Superconducting Solenoid Magnet & Quench Protection (Wilson / CERN)
    let res_mag = post(
        addr,
        "/api/sim/science/superconducting_magnet",
        r#"{
            "inner_radius_m": 0.35,
            "outer_radius_m": 0.48,
            "coil_length_m": 1.25,
            "turns_count": 2600,
            "operating_current_a": 1400.0,
            "operating_temperature_k": 4.20
        }"#,
    )?;
    assert_eq!(res_mag.status, 200);
    let mag_json = json(&res_mag)?;
    assert!(mag_json["central_field_t"].as_f64().unwrap() > 2.5);
    assert!(
        mag_json["peak_conductor_field_t"].as_f64().unwrap()
            > mag_json["central_field_t"].as_f64().unwrap()
    );
    assert!(mag_json["stored_energy_mj"].as_f64().unwrap() > 1.8);
    assert!(mag_json["load_line_margin_pct"].as_f64().unwrap() < 75.0);
    assert!(
        mag_json["adiabatic_hotspot_temperature_k"]
            .as_f64()
            .unwrap()
            < 180.0
    );
    assert!(mag_json["is_quench_safe"].as_bool().unwrap());
    assert!(mag_json["is_margin_adequate"].as_bool().unwrap());

    // 150. Spacecraft Hypersonic Reentry Aerothermodynamics & Stagnation Heat Flux (Sutton-Graves / NASA SP-8000)
    let res_reentry = post(
        addr,
        "/api/sim/aerospace/reentry_aerothermodynamics",
        r#"{
            "freestream_velocity_m_s": 7400.0,
            "altitude_km": 62.0,
            "nose_radius_m": 1.15,
            "vehicle_mass_kg": 5200.0
        }"#,
    )?;
    assert_eq!(res_reentry.status, 200);
    let reentry_json = json(&res_reentry)?;
    assert!(reentry_json["freestream_mach"].as_f64().unwrap() > 20.0);
    assert!(reentry_json["stagnation_pressure_kpa"].as_f64().unwrap() > 10.0);
    assert!(reentry_json["stagnation_enthalpy_mj_kg"].as_f64().unwrap() > 25.0);
    assert!(
        reentry_json["total_stagnation_heat_flux_kw_m2"]
            .as_f64()
            .unwrap()
            > 800.0
    );
    assert!(
        reentry_json["radiative_equilibrium_wall_temperature_k"]
            .as_f64()
            .unwrap()
            > 1700.0
    );
    assert!(reentry_json["thermal_safety_factor"].as_f64().unwrap() >= 1.05);
    assert!(reentry_json["is_tps_safe"].as_bool().unwrap());

    // 151. Laser Powder Bed Fusion (LPBF / DMLS) Melt Pool Thermals & Conduction Mode (King / Rosenthal)
    let res_lpbf = post(
        addr,
        "/api/sim/manufacturing/lpbf_meltpool",
        r#"{
            "laser_power_w": 250.0,
            "scan_speed_mm_s": 950.0,
            "beam_diameter_um": 80.0,
            "layer_thickness_um": 40.0,
            "hatch_spacing_um": 100.0
        }"#,
    )?;
    assert_eq!(res_lpbf.status, 200);
    let lpbf_json = json(&res_lpbf)?;
    assert!(
        lpbf_json["volumetric_energy_density_j_mm3"]
            .as_f64()
            .unwrap()
            > 50.0
    );
    assert!(lpbf_json["normalized_enthalpy"].as_f64().unwrap() > 2.5);
    assert!(lpbf_json["meltpool_width_um"].as_f64().unwrap() > 100.0);
    assert!(lpbf_json["hatch_overlap_ratio"].as_f64().unwrap() > 1.10);
    assert!(lpbf_json["penetration_ratio"].as_f64().unwrap() > 1.30);
    assert!(lpbf_json["is_acceptable"].as_bool().unwrap());

    // 152. Acoustic Metamaterials & Phononic Crystal Bandgap Mechanics (Mead / Brillouin)
    let res_phononic = post(
        addr,
        "/api/sim/acoustics/phononic_bandgap",
        r#"{
            "unit_cell_count": 8,
            "layer_a_thickness_mm": 24.0,
            "layer_b_thickness_mm": 16.0,
            "excitation_frequency_hz": 24000.0
        }"#,
    )?;
    assert_eq!(res_phononic.status, 200);
    let phononic_json = json(&res_phononic)?;
    assert!(phononic_json["lattice_constant_mm"].as_f64().unwrap() > 35.0);
    assert!(phononic_json["impedance_contrast_ratio"].as_f64().unwrap() > 15.0);
    assert!(phononic_json["bragg_center_frequency_hz"].as_f64().unwrap() > 20000.0);
    assert!(
        phononic_json["relative_bandgap_width_pct"]
            .as_f64()
            .unwrap()
            > 30.0
    );
    assert!(phononic_json["is_in_bandgap"].as_bool().unwrap());
    assert!(phononic_json["transmission_loss_db"].as_f64().unwrap() > 30.0);
    assert!(phononic_json["is_isolation_effective"].as_bool().unwrap());

    // 153. Universal Commands Execution Verification (Batch 14 Domains)
    for cmd_id in [
        "sim.energy.pem_fuel_cell",
        "sim.science.superconducting_magnet",
        "sim.aerospace.reentry_aerothermodynamics",
        "sim.manufacturing.lpbf_meltpool",
        "sim.acoustics.phononic_bandgap",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 154. Enterprise MCP Tool Calling (Batch 14 Domains)
    for tool_name in [
        "atlas_sim_energy_pem_fuel_cell",
        "atlas_sim_science_superconducting_magnet",
        "atlas_sim_aerospace_reentry_aerothermodynamics",
        "atlas_sim_manufacturing_lpbf_meltpool",
        "atlas_sim_acoustics_phononic_bandgap",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 167, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 155. Wheel-Rail Contact Mechanics & Track Hunting Stability (Kalker / Hertz / Nadal / UIC 518)
    let res_wheel_rail = post(
        addr,
        "/api/sim/rail/wheel_rail_contact",
        r#"{
            "wheel_vertical_load_kn": 100.0,
            "wheel_radius_mm": 460.0,
            "rail_crown_radius_mm": 300.0,
            "train_speed_km_h": 200.0,
            "effective_conicity_rad": 0.025,
            "track_gauge_half_m": 0.750,
            "flange_contact_angle_deg": 70.0,
            "friction_coefficient": 0.35,
            "applied_lateral_force_kn": 35.0
        }"#,
    )?;
    assert_eq!(res_wheel_rail.status, 200);
    let wr_json = json(&res_wheel_rail)?;
    assert!(wr_json["contact_patch_area_mm2"].as_f64().unwrap() > 100.0);
    assert!(wr_json["max_hertz_pressure_mpa"].as_f64().unwrap() > 800.0);
    assert!(wr_json["kalker_f11_mn"].as_f64().unwrap() > 10.0);
    assert!(wr_json["klingel_hunting_frequency_hz"].as_f64().unwrap() > 1.5);
    assert!(wr_json["derailment_safety_factor"].as_f64().unwrap() > 1.2);
    assert!(wr_json["is_contact_stress_safe"].as_bool().unwrap());
    assert!(wr_json["is_derailment_safe"].as_bool().unwrap());

    // 156. Subsurface Mine Ventilation Aerodynamics & Fan Duty (Atkinson / MSHA / McPherson)
    let res_mine = post(
        addr,
        "/api/sim/mining/mine_ventilation",
        r#"{
            "airway_length_m": 1400.0,
            "airway_area_m2": 20.0,
            "airway_perimeter_m": 18.0,
            "airflow_rate_m3_s": 85.0,
            "vertical_depth_m": 400.0,
            "methane_emission_m3_s": 0.12
        }"#,
    )?;
    assert_eq!(res_mine.status, 200);
    let mine_json = json(&res_mine)?;
    assert!(mine_json["atkinson_resistance_ns2_m8"].as_f64().unwrap() > 0.01);
    assert!(mine_json["total_airway_pressure_drop_pa"].as_f64().unwrap() > 150.0);
    assert!(mine_json["air_velocity_m_s"].as_f64().unwrap() > 3.0);
    assert!(mine_json["required_fan_pressure_pa"].as_f64().unwrap() > 100.0);
    assert!(mine_json["fan_shaft_power_kw"].as_f64().unwrap() > 10.0);
    assert!(mine_json["return_methane_pct"].as_f64().unwrap() < 1.0);
    assert!(mine_json["is_methane_safe"].as_bool().unwrap());

    // 157. Parabolic Reflector Antenna Electromagnetics & Gain (Silver / Stutzman / ITU-R S.580)
    let res_antenna = post(
        addr,
        "/api/sim/telecom/reflector_antenna",
        r#"{
            "dish_diameter_m": 2.40,
            "focal_length_m": 0.96,
            "operating_frequency_ghz": 14.25,
            "surface_rms_error_mm": 0.35,
            "aperture_blockage_fraction": 0.04
        }"#,
    )?;
    assert_eq!(res_antenna.status, 200);
    let ant_json = json(&res_antenna)?;
    assert!(ant_json["boresight_gain_dbi"].as_f64().unwrap() > 45.0);
    assert!(ant_json["half_power_beamwidth_deg"].as_f64().unwrap() < 1.0);
    assert!(ant_json["ruze_efficiency"].as_f64().unwrap() > 0.85);
    assert!(ant_json["aperture_efficiency_pct"].as_f64().unwrap() > 65.0);
    assert!(ant_json["first_sidelobe_level_db"].as_f64().unwrap() < -20.0);
    assert!(ant_json["is_itu_compliant"].as_bool().unwrap());

    // 158. Sub-3nm Gate-All-Around (GAA) Nanosheet FET Electrostatics (Taur / BSIM-CMG / IEEE IRDS)
    let res_gaa = post(
        addr,
        "/api/sim/semiconductor/gaa_fet",
        r#"{
            "nanosheet_count": 3,
            "gate_length_nm": 12.0,
            "nanosheet_width_nm": 30.0,
            "nanosheet_thickness_nm": 5.0,
            "equivalent_oxide_thickness_nm": 0.75,
            "supply_voltage_v": 0.70
        }"#,
    )?;
    assert_eq!(res_gaa.status, 200);
    let gaa_json = json(&res_gaa)?;
    assert_eq!(gaa_json["effective_gate_width_nm"].as_f64().unwrap(), 210.0);
    assert!(gaa_json["natural_scaling_length_nm"].as_f64().unwrap() < 2.0);
    assert!(gaa_json["subthreshold_swing_mv_dec"].as_f64().unwrap() < 70.0);
    assert!(gaa_json["dibl_mv_v"].as_f64().unwrap() < 40.0);
    assert!(gaa_json["on_current_ua"].as_f64().unwrap() > 150.0);
    assert!(gaa_json["is_channel_control_adequate"].as_bool().unwrap());

    // 159. Precision Agricultural Irrigation Lateral Hydraulics (Christiansen / Keller-Bliesner / ISO 9261)
    let res_irrig = post(
        addr,
        "/api/sim/agri/irrigation_hydraulics",
        r#"{
            "pipe_inside_diameter_mm": 16.0,
            "lateral_length_m": 120.0,
            "emitter_spacing_m": 0.40,
            "nominal_discharge_l_h": 1.60,
            "nominal_pressure_head_m": 10.0,
            "inlet_pressure_head_m": 11.0
        }"#,
    )?;
    assert_eq!(res_irrig.status, 200);
    let irrig_json = json(&res_irrig)?;
    assert_eq!(irrig_json["total_emitter_count"].as_u64().unwrap(), 300);
    assert_eq!(irrig_json["total_inlet_flow_l_h"].as_f64().unwrap(), 480.0);
    assert!(irrig_json["friction_head_loss_m"].as_f64().unwrap() > 1.0);
    assert!(irrig_json["christiansen_uniformity_pct"].as_f64().unwrap() > 92.0);
    assert!(irrig_json["emission_uniformity_pct"].as_f64().unwrap() > 90.0);
    assert!(irrig_json["is_uniformity_excellent"].as_bool().unwrap());

    // 160. Universal Commands Execution Verification (Batch 15 Domains)
    for cmd_id in [
        "sim.rail.wheel_rail_contact",
        "sim.mining.mine_ventilation",
        "sim.telecom.reflector_antenna",
        "sim.semiconductor.gaa_fet",
        "sim.agri.irrigation_hydraulics",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 161. Enterprise MCP Tool Calling (Batch 15 Domains)
    for tool_name in [
        "atlas_sim_rail_wheel_rail_contact",
        "atlas_sim_mining_mine_ventilation",
        "atlas_sim_telecom_reflector_antenna",
        "atlas_sim_semiconductor_gaa_fet",
        "atlas_sim_agri_irrigation_hydraulics",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 168, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 162. Activated Sludge Aeration & Secondary Clarifier Sedimentation (ASCE / WEF / Takacs)
    let res_wwtp = post(
        addr,
        "/api/sim/environmental/wastewater_treatment",
        r#"{
            "influent_flow_m3_day": 20000.0,
            "influent_bod_mg_l": 220.0,
            "target_effluent_bod_mg_l": 15.0,
            "mlss_mg_l": 3000.0,
            "sludge_retention_time_days": 8.0,
            "basin_depth_m": 4.5,
            "clarifier_diameter_m": 28.0,
            "clarifier_count": 2,
            "wastewater_temp_c": 20.0
        }"#,
    )?;
    assert_eq!(res_wwtp.status, 200);
    let wwtp_json = json(&res_wwtp)?;
    assert!(wwtp_json["aeration_basin_volume_m3"].as_f64().unwrap() > 4000.0);
    assert!(
        wwtp_json["hydraulic_retention_time_hours"]
            .as_f64()
            .unwrap()
            > 5.0
    );
    assert!(wwtp_json["food_to_microorganism_ratio"].as_f64().unwrap() > 0.20);
    assert!(
        wwtp_json["daily_sludge_production_kg_day"]
            .as_f64()
            .unwrap()
            > 1500.0
    );
    assert!(
        wwtp_json["surface_overflow_rate_m3_m2_day"]
            .as_f64()
            .unwrap()
            <= 32.0
    );
    assert!(wwtp_json["solids_loading_rate_kg_m2_h"].as_f64().unwrap() <= 6.0);
    assert!(wwtp_json["is_clarifier_sor_acceptable"].as_bool().unwrap());
    assert!(wwtp_json["is_clarifier_slr_acceptable"].as_bool().unwrap());
    assert!(wwtp_json["is_effluent_compliant"].as_bool().unwrap());

    // 163. EV Longitudinal Dynamics & Regenerative Braking (SAE J2452 / UNECE R13-H)
    let res_ev = post(
        addr,
        "/api/sim/automotive/ev_powertrain",
        r#"{
            "vehicle_mass_kg": 1850.0,
            "rolling_resistance_coeff": 0.009,
            "drag_coefficient": 0.24,
            "frontal_area_m2": 2.25,
            "tire_radius_m": 0.330,
            "gear_ratio": 9.0,
            "motor_peak_torque_nm": 350.0,
            "battery_capacity_kwh": 75.0,
            "cruising_speed_km_h": 100.0
        }"#,
    )?;
    assert_eq!(res_ev.status, 200);
    let ev_json = json(&res_ev)?;
    assert!(ev_json["total_road_load_force_n"].as_f64().unwrap() > 300.0);
    assert!(ev_json["wheel_power_at_cruise_kw"].as_f64().unwrap() > 8.0);
    assert!(ev_json["cruising_range_km"].as_f64().unwrap() > 350.0);
    assert!(ev_json["max_wheel_tractive_force_n"].as_f64().unwrap() > 8000.0);
    assert!(ev_json["acceleration_0_to_100_km_h_s"].as_f64().unwrap() < 7.5);
    assert!(ev_json["regen_braking_power_kw"].as_f64().unwrap() > 15.0);
    assert!(ev_json["is_cruising_speed_attainable"].as_bool().unwrap());
    assert!(ev_json["is_regen_braking_compliant"].as_bool().unwrap());

    // 164. Lead-Rubber Bearing (LRB) Seismic Base Isolation (ASCE 7-22 / FEMA P-751)
    let res_lrb = post(
        addr,
        "/api/sim/civil/seismic_base_isolation",
        r#"{
            "superstructure_mass_tonnes": 4500.0,
            "bearing_count": 16,
            "bearing_outer_diameter_mm": 800.0,
            "lead_core_diameter_mm": 160.0,
            "total_rubber_thickness_mm": 200.0,
            "lead_yield_strength_mpa": 10.5,
            "mapped_spectral_accel_s1_g": 0.60,
            "site_coefficient_fv": 1.20,
            "fixed_base_period_s": 0.55
        }"#,
    )?;
    assert_eq!(res_lrb.status, 200);
    let lrb_json = json(&res_lrb)?;
    assert!(lrb_json["total_seismic_weight_kn"].as_f64().unwrap() > 40000.0);
    assert!(lrb_json["total_system_yield_force_qd_kn"].as_f64().unwrap() > 2500.0);
    assert!(lrb_json["effective_isolated_period_s"].as_f64().unwrap() > 2.0);
    assert!(lrb_json["effective_damping_ratio_pct"].as_f64().unwrap() > 15.0);
    assert!(
        lrb_json["design_horizontal_displacement_dd_mm"]
            .as_f64()
            .unwrap()
            > 100.0
    );
    assert!(lrb_json["base_shear_reduction_ratio"].as_f64().unwrap() < 0.35);
    assert!(lrb_json["seismic_force_attenuation_pct"].as_f64().unwrap() > 50.0);
    assert!(lrb_json["is_rubber_strain_safe"].as_bool().unwrap());
    assert!(lrb_json["is_base_isolation_effective"].as_bool().unwrap());

    // 165. Terminal Ballistics & Armor Hydrodynamic Penetration (Tate-Alekseevskii / STANAG 4569)
    let res_ballistics = post(
        addr,
        "/api/sim/defense/terminal_ballistics",
        r#"{
            "projectile_mass_kg": 0.180,
            "projectile_diameter_mm": 12.0,
            "projectile_length_mm": 90.0,
            "impact_velocity_m_s": 1450.0,
            "obliquity_angle_deg": 30.0,
            "armor_thickness_mm": 75.0
        }"#,
    )?;
    assert_eq!(res_ballistics.status, 200);
    let bal_json = json(&res_ballistics)?;
    assert!(bal_json["normal_impact_velocity_m_s"].as_f64().unwrap() > 1200.0);
    assert!(bal_json["line_of_sight_thickness_mm"].as_f64().unwrap() > 80.0);
    assert!(
        bal_json["penetration_interface_velocity_m_s"]
            .as_f64()
            .unwrap()
            > 500.0
    );
    assert!(bal_json["total_penetration_depth_mm"].as_f64().unwrap() > 50.0);
    assert!(bal_json["ballistic_limit_v50_m_s"].as_f64().unwrap() > 800.0);
    assert!(bal_json["armor_protection_margin_factor"].as_f64().unwrap() > 1.05);
    assert!(bal_json["is_armor_defeating_projectile"].as_bool().unwrap());
    assert!(bal_json["stanag_4569_defeated_level"].as_u64().unwrap() >= 4);

    // 166. BGA Solder Joint Thermal Fatigue & Creep Lifetime (Engelmaier / IPC-9701)
    let res_solder = post(
        addr,
        "/api/sim/packaging/solder_thermal_fatigue",
        r#"{
            "package_width_mm": 35.0,
            "package_length_mm": 35.0,
            "solder_joint_height_um": 400.0,
            "solder_ball_pitch_mm": 1.0,
            "package_cte_ppm_k": 6.5,
            "pcb_cte_ppm_k": 16.0,
            "cycle_min_temp_c": -40.0,
            "cycle_max_temp_c": 125.0
        }"#,
    )?;
    assert_eq!(res_solder.status, 200);
    let sol_json = json(&res_solder)?;
    assert!(
        sol_json["distance_to_neutral_point_dnp_mm"]
            .as_f64()
            .unwrap()
            > 20.0
    );
    assert!(sol_json["cyclic_shear_strain_pct"].as_f64().unwrap() > 1.5);
    assert!(sol_json["median_cycles_to_failure_n50"].as_f64().unwrap() > 500.0);
    assert!(
        sol_json["weibull_characteristic_life_eta"]
            .as_f64()
            .unwrap()
            > 500.0
    );
    assert!(sol_json["is_ipc_9701_compliant"].as_bool().unwrap());

    // 167. Universal Commands Execution Verification (Batch 16 Domains)
    for cmd_id in [
        "sim.environmental.wastewater_treatment",
        "sim.automotive.ev_powertrain",
        "sim.civil.seismic_base_isolation",
        "sim.defense.terminal_ballistics",
        "sim.packaging.solder_thermal_fatigue",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 168. Enterprise MCP Tool Calling (Batch 16 Domains)
    for tool_name in [
        "atlas_sim_environmental_wastewater_treatment",
        "atlas_sim_automotive_ev_powertrain",
        "atlas_sim_civil_seismic_base_isolation",
        "atlas_sim_defense_terminal_ballistics",
        "atlas_sim_packaging_solder_thermal_fatigue",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 169, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 169. Ship Bow & Bottom Hydrodynamic Wave Slamming Impact (Wagner / Chuang / DNV-RU-SHIP)
    let res_slamming = post(
        addr,
        "/api/sim/marine/ship_slamming",
        r#"{
            "deadrise_angle_deg": 18.0,
            "relative_impact_velocity_m_s": 7.5,
            "frame_spacing_m": 0.75,
            "plate_length_m": 2.40,
            "plate_thickness_mm": 16.0,
            "steel_yield_strength_mpa": 355.0
        }"#,
    )?;
    assert_eq!(res_slamming.status, 200);
    let slam_json = json(&res_slamming)?;
    assert!(
        slam_json["slamming_pressure_coefficient_cp"]
            .as_f64()
            .unwrap()
            > 10.0
    );
    assert!(slam_json["peak_localized_pressure_kpa"].as_f64().unwrap() > 250.0);
    assert!(slam_json["space_averaged_pressure_kpa"].as_f64().unwrap() > 150.0);
    assert!(slam_json["slamming_pulse_duration_ms"].as_f64().unwrap() > 5.0);
    assert!(slam_json["dnv_required_thickness_mm"].as_f64().unwrap() > 8.0);
    assert!(slam_json["thickness_safety_factor"].as_f64().unwrap() >= 1.0);
    assert!(
        slam_json["is_plate_thickness_sufficient"]
            .as_bool()
            .unwrap()
    );
    assert!(
        slam_json["is_air_cushioning_non_critical"]
            .as_bool()
            .unwrap()
    );

    // 170. Vehicle Frontal Crash Deceleration & Occupant Severity (ECE R94 / FMVSS 208 / HIC15)
    let res_crash = post(
        addr,
        "/api/sim/automotive/crash_deceleration",
        r#"{
            "vehicle_mass_kg": 1650.0,
            "impact_velocity_km_h": 64.0,
            "crumple_zone_length_m": 0.75,
            "crash_box_mean_force_kn": 340.0,
            "structural_energy_efficiency": 0.78,
            "occupant_mass_kg": 78.0
        }"#,
    )?;
    assert_eq!(res_crash.status, 200);
    let crash_json = json(&res_crash)?;
    assert!(crash_json["initial_kinetic_energy_kj"].as_f64().unwrap() > 240.0);
    assert!(crash_json["dynamic_crush_distance_mm"].as_f64().unwrap() > 500.0);
    assert!(crash_json["crash_pulse_duration_ms"].as_f64().unwrap() > 50.0);
    assert!(crash_json["head_injury_criterion_hic15"].as_f64().unwrap() < 700.0);
    assert!(crash_json["occupant_chest_deflection_mm"].as_f64().unwrap() < 42.0);
    assert!(crash_json["is_crumple_zone_sufficient"].as_bool().unwrap());
    assert!(crash_json["is_hic15_compliant"].as_bool().unwrap());
    assert!(
        crash_json["is_chest_deflection_compliant"]
            .as_bool()
            .unwrap()
    );

    // 171. Nuclear Reactor Fuel Pin Radial Conduction & DNB Margin (Fourier / Dittus-Boelter / W-3)
    let res_fuel = post(
        addr,
        "/api/sim/nuclear/fuel_pin",
        r#"{
            "linear_heat_rate_kw_per_m": 32.0,
            "pellet_radius_mm": 4.10,
            "gap_thickness_um": 80.0,
            "clad_thickness_mm": 0.60,
            "clad_outer_diameter_mm": 9.50,
            "rod_pitch_mm": 12.60,
            "coolant_pressure_mpa": 15.50,
            "bulk_coolant_temperature_c": 305.0,
            "coolant_mass_flux_kg_m2_s": 3500.0
        }"#,
    )?;
    assert_eq!(res_fuel.status, 200);
    let fuel_json = json(&res_fuel)?;
    assert!(fuel_json["clad_surface_heat_flux_mw_m2"].as_f64().unwrap() > 0.8);
    assert!(
        fuel_json["coolant_heat_transfer_coeff_w_m2_k"]
            .as_f64()
            .unwrap()
            > 20000.0
    );
    assert!(fuel_json["clad_outer_temperature_c"].as_f64().unwrap() > 310.0);
    assert!(fuel_json["centerline_temperature_c"].as_f64().unwrap() > 1200.0);
    assert!(fuel_json["fuel_melting_margin_c"].as_f64().unwrap() > 600.0);
    assert!(
        fuel_json["departure_from_nucleate_boiling_ratio"]
            .as_f64()
            .unwrap()
            >= 1.30
    );
    assert!(fuel_json["is_dnbr_acceptable"].as_bool().unwrap());
    assert!(fuel_json["is_fuel_melting_prevented"].as_bool().unwrap());

    // 172. Pressure Swing Adsorption (PSA) Gas Separation & Breakthrough Dynamics (Langmuir / LDF / Thomas)
    let res_psa = post(
        addr,
        "/api/sim/chemical/adsorption_psa",
        r#"{
            "bed_diameter_m": 0.60,
            "bed_length_m": 2.20,
            "adsorption_pressure_bar": 5.50,
            "desorption_pressure_bar": 1.10,
            "operating_temperature_c": 25.0,
            "feed_flow_rate_nm3_h": 85.0
        }"#,
    )?;
    assert_eq!(res_psa.status, 200);
    let psa_json = json(&res_psa)?;
    assert!(psa_json["column_adsorbent_mass_kg"].as_f64().unwrap() > 400.0);
    assert!(
        psa_json["heavy_key_equilibrium_capacity_mol_kg"]
            .as_f64()
            .unwrap()
            > 1.0
    );
    assert!(psa_json["working_capacity_mol_kg"].as_f64().unwrap() > 0.4);
    assert!(psa_json["mass_transfer_zone_length_m"].as_f64().unwrap() > 0.2);
    assert!(psa_json["breakthrough_time_seconds"].as_f64().unwrap() > 60.0);
    assert!(psa_json["product_purity_pct"].as_f64().unwrap() >= 93.0);
    assert!(psa_json["is_cycle_time_feasible"].as_bool().unwrap());
    assert!(psa_json["is_purity_specification_met"].as_bool().unwrap());

    // 173. Rotorcraft Helicopter Hover & Forward Flight Aerodynamics (Glauert / Leishman / Prouty)
    let res_rotor = post(
        addr,
        "/api/sim/aerospace/rotorcraft_aerodynamics",
        r#"{
            "gross_weight_kg": 4300.0,
            "rotor_radius_m": 6.50,
            "number_of_blades": 4,
            "blade_chord_m": 0.38,
            "rotor_speed_rpm": 345.0,
            "forward_speed_knots": 0.0
        }"#,
    )?;
    assert_eq!(res_rotor.status, 200);
    let rotor_json = json(&res_rotor)?;
    assert!(rotor_json["blade_tip_speed_m_s"].as_f64().unwrap() > 220.0);
    assert!(rotor_json["induced_velocity_m_s"].as_f64().unwrap() > 10.0);
    assert!(rotor_json["figure_of_merit"].as_f64().unwrap() > 0.65);
    assert!(rotor_json["total_engine_shaft_power_kw"].as_f64().unwrap() > 600.0);
    assert!(
        rotor_json["is_blade_loading_within_limits"]
            .as_bool()
            .unwrap()
    );
    assert!(rotor_json["is_tip_mach_non_critical"].as_bool().unwrap());

    // 174. Universal Commands Execution Verification (Batch 17 Domains)
    for cmd_id in [
        "sim.marine.ship_slamming",
        "sim.automotive.crash_deceleration",
        "sim.nuclear.fuel_pin",
        "sim.chemical.adsorption_psa",
        "sim.aerospace.rotorcraft_aerodynamics",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 175. Enterprise MCP Tool Calling (Batch 17 Domains)
    for tool_name in [
        "atlas_sim_marine_ship_slamming",
        "atlas_sim_automotive_crash_deceleration",
        "atlas_sim_nuclear_fuel_pin",
        "atlas_sim_chemical_adsorption_psa",
        "atlas_sim_aerospace_rotorcraft_aerodynamics",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 176, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 176. Shell-and-Tube Heat Exchanger Rating & Pressure Drop (Bell-Delaware / TEMA)
    let res_hx = post(
        addr,
        "/api/sim/thermal/heat_exchanger_delaware",
        r#"{
            "shell_inside_diameter_m": 0.60,
            "number_of_tubes": 360,
            "tube_outer_diameter_mm": 19.05,
            "tube_wall_thickness_mm": 1.65,
            "tube_length_m": 4.80,
            "tube_pitch_mm": 23.81,
            "baffle_cut_pct": 25.0,
            "baffle_spacing_m": 0.30,
            "hot_inlet_temp_c": 120.0,
            "hot_outlet_temp_c": 65.0,
            "hot_mass_flow_kg_s": 18.5,
            "cold_inlet_temp_c": 25.0,
            "cold_outlet_temp_c": 50.0,
            "cold_mass_flow_kg_s": 32.0
        }"#,
    )?;
    assert_eq!(res_hx.status, 200);
    let hx_json = json(&res_hx)?;
    assert!(hx_json["thermal_duty_kw"].as_f64().unwrap() > 2000.0);
    assert!(hx_json["counterflow_lmtd_c"].as_f64().unwrap() > 45.0);
    assert!(hx_json["lmtd_correction_factor_f"].as_f64().unwrap() >= 0.80);
    assert!(hx_json["shell_side_corrected_htc_w_m2_k"].as_f64().unwrap() > 400.0);
    assert!(hx_json["overall_htc_w_m2_k"].as_f64().unwrap() > 200.0);
    assert!(hx_json["surface_area_overdesign_pct"].as_f64().unwrap() >= 10.0);
    assert!(hx_json["shell_side_pressure_drop_kpa"].as_f64().unwrap() < 70.0);
    assert!(hx_json["is_f_factor_acceptable"].as_bool().unwrap());
    assert!(hx_json["is_surface_area_adequate"].as_bool().unwrap());
    assert!(hx_json["is_pressure_drop_acceptable"].as_bool().unwrap());

    // 177. Drilled Shaft & Bored Pile Axial Resistance & Settlement (FHWA GEC-010 / AASHTO LRFD)
    let res_shaft = post(
        addr,
        "/api/sim/geotechnical/drilled_shaft",
        r#"{
            "shaft_diameter_m": 1.20,
            "embedded_length_m": 18.0,
            "factored_load_demand_kn": 3200.0,
            "service_load_demand_kn": 2200.0,
            "undrained_shear_strength_kpa": 120.0,
            "effective_friction_angle_deg": 32.0,
            "soil_unit_weight_kn_m3": 18.5,
            "concrete_compressive_strength_mpa": 35.0,
            "steel_reinforcement_ratio_pct": 1.50
        }"#,
    )?;
    assert_eq!(res_shaft.status, 200);
    let shaft_json = json(&res_shaft)?;
    assert!(shaft_json["base_cross_sectional_area_m2"].as_f64().unwrap() > 1.0);
    assert!(shaft_json["unit_side_resistance_kpa"].as_f64().unwrap() > 50.0);
    assert!(shaft_json["nominal_side_resistance_kn"].as_f64().unwrap() > 3000.0);
    assert!(shaft_json["nominal_total_resistance_kn"].as_f64().unwrap() > 4500.0);
    assert!(
        shaft_json["factored_geotechnical_resistance_kn"]
            .as_f64()
            .unwrap()
            > 2500.0
    );
    assert!(
        shaft_json["elastic_service_settlement_mm"]
            .as_f64()
            .unwrap()
            < 25.0
    );
    assert!(shaft_json["is_settlement_acceptable"].as_bool().unwrap());

    // 178. Prosthetic Heart Valve Hemodynamics & Effective Orifice Area (ISO 5840 / Gorlin)
    let res_valve = post(
        addr,
        "/api/sim/biomedical/heart_valve",
        r#"{
            "nominal_size_mm": 23.0,
            "heart_rate_bpm": 70.0,
            "stroke_volume_ml": 75.0,
            "systolic_ejection_period_s": 0.30,
            "patient_bsa_m2": 1.85,
            "mean_pressure_gradient_mmhg": 10.5,
            "peak_jet_velocity_m_s": 2.15,
            "dynamic_closing_volume_ml": 2.8,
            "static_leakage_volume_ml": 3.2
        }"#,
    )?;
    assert_eq!(res_valve.status, 200);
    let valve_json = json(&res_valve)?;
    assert!(valve_json["cardiac_output_l_min"].as_f64().unwrap() > 4.5);
    assert!(valve_json["rms_systolic_flow_rate_ml_s"].as_f64().unwrap() > 200.0);
    assert!(valve_json["peak_pressure_gradient_mmhg"].as_f64().unwrap() > 15.0);
    assert!(valve_json["effective_orifice_area_cm2"].as_f64().unwrap() > 1.30);
    assert!(valve_json["indexed_eoa_cm2_per_m2"].as_f64().unwrap() > 0.75);
    assert!(valve_json["regurgitant_fraction_pct"].as_f64().unwrap() < 12.0);
    assert!(
        valve_json["is_hemodynamic_eoa_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(valve_json["is_regurgitation_safe"].as_bool().unwrap());
    assert!(
        valve_json["is_severe_mismatch_prevented"]
            .as_bool()
            .unwrap()
    );

    // 179. Centrifugal Compressor Stage Aerodynamics & Surge Margin (Stanitz / Eckardt / Dixon)
    let res_comp = post(
        addr,
        "/api/sim/aerospace/centrifugal_compressor",
        r#"{
            "mass_flow_rate_kg_s": 3.80,
            "rotational_speed_rpm": 38000.0,
            "impeller_tip_diameter_m": 0.285,
            "inducer_hub_diameter_m": 0.075,
            "inducer_shroud_diameter_m": 0.165,
            "number_of_blades": 18,
            "blade_backsweep_deg": 35.0
        }"#,
    )?;
    assert_eq!(res_comp.status, 200);
    let comp_json = json(&res_comp)?;
    assert!(comp_json["tip_speed_m_s"].as_f64().unwrap() > 500.0);
    assert!(comp_json["stanitz_slip_factor"].as_f64().unwrap() > 0.85);
    assert!(comp_json["specific_work_kj_kg"].as_f64().unwrap() > 200.0);
    assert!(comp_json["total_pressure_ratio_tt"].as_f64().unwrap() > 4.0);
    assert!(comp_json["surge_margin_pct"].as_f64().unwrap() >= 12.0);
    assert!(comp_json["disk_mechanical_safety_factor"].as_f64().unwrap() >= 1.25);
    assert!(comp_json["is_surge_margin_adequate"].as_bool().unwrap());
    assert!(comp_json["is_disk_stress_acceptable"].as_bool().unwrap());

    // 180. High-Speed Rail Pantograph-Catenary Dynamic Interaction (EN 50367 / EN 50119)
    let res_panto = post(
        addr,
        "/api/sim/rail/pantograph",
        r#"{
            "train_speed_km_h": 300.0,
            "static_contact_force_n": 80.0,
            "aerodynamic_uplift_coeff": 0.00095,
            "contact_wire_tension_kn": 22.0,
            "contact_wire_mass_kg_per_m": 1.07,
            "catenary_span_length_m": 55.0,
            "catenary_midspan_elasticity_mm_per_n": 0.75,
            "catenary_support_elasticity_mm_per_n": 0.35
        }"#,
    )?;
    assert_eq!(res_panto.status, 200);
    let panto_json = json(&res_panto)?;
    assert!(panto_json["mean_contact_force_n"].as_f64().unwrap() > 140.0);
    assert!(panto_json["contact_force_std_dev_n"].as_f64().unwrap() > 20.0);
    assert!(panto_json["contact_wire_wave_speed_m_s"].as_f64().unwrap() > 130.0);
    assert!(panto_json["doppler_speed_ratio_alpha"].as_f64().unwrap() <= 0.70);
    assert!(panto_json["contact_wire_uplift_mm"].as_f64().unwrap() < 100.0);
    assert!(
        panto_json["loss_of_contact_percentage_nq"]
            .as_f64()
            .unwrap()
            <= 0.20
    );
    assert!(
        panto_json["is_mean_contact_force_compliant"]
            .as_bool()
            .unwrap()
    );
    assert!(panto_json["is_arcing_rate_acceptable"].as_bool().unwrap());
    assert!(
        panto_json["is_doppler_speed_non_critical"]
            .as_bool()
            .unwrap()
    );
    assert!(panto_json["is_uplift_acceptable"].as_bool().unwrap());

    // 181. Universal Commands Execution Verification (Batch 18 Domains)
    for cmd_id in [
        "sim.thermal.heat_exchanger_delaware",
        "sim.geotechnical.drilled_shaft",
        "sim.biomedical.heart_valve",
        "sim.aerospace.centrifugal_compressor",
        "sim.rail.pantograph",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 182. Enterprise MCP Tool Calling (Batch 18 Domains)
    for tool_name in [
        "atlas_sim_thermal_heat_exchanger_delaware",
        "atlas_sim_geotechnical_drilled_shaft",
        "atlas_sim_biomedical_heart_valve",
        "atlas_sim_aerospace_centrifugal_compressor",
        "atlas_sim_rail_pantograph",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 183, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 183. Tunnel Boring Machine Earth Pressure Balance (ITA / Herzog / Eirich)
    let res_tbm = post(
        addr,
        "/api/sim/geotechnical/tbm_tunneling",
        r#"{
            "tunnel_outer_diameter_m": 6.60,
            "tunnel_depth_to_axis_m": 22.0,
            "water_table_depth_m": 4.0,
            "soil_bulk_unit_weight_kn_m3": 19.5,
            "soil_effective_cohesion_kpa": 12.0,
            "soil_effective_friction_angle_deg": 32.0,
            "soil_compressive_strength_mpa": 0.85,
            "cutterhead_opening_ratio_pct": 35.0,
            "cutterhead_rotational_speed_rpm": 2.5,
            "penetration_rate_mm_per_rev": 12.0
        }"#,
    )?;
    assert_eq!(res_tbm.status, 200);
    let tbm_json = json(&res_tbm)?;
    assert!(tbm_json["face_area_m2"].as_f64().unwrap() > 33.0);
    assert!(tbm_json["hydrostatic_water_pressure_kpa"].as_f64().unwrap() > 150.0);
    assert!(tbm_json["min_face_support_pressure_kpa"].as_f64().unwrap() > 180.0);
    assert!(
        tbm_json["target_face_support_pressure_kpa"]
            .as_f64()
            .unwrap()
            > 220.0
    );
    assert!(tbm_json["face_stability_safety_factor"].as_f64().unwrap() >= 1.20);
    assert!(tbm_json["total_required_thrust_kn"].as_f64().unwrap() > 15000.0);
    assert!(tbm_json["total_cutterhead_torque_kn_m"].as_f64().unwrap() > 2000.0);
    assert!(tbm_json["cutterhead_drive_power_kw"].as_f64().unwrap() > 500.0);
    assert!(tbm_json["is_face_stable"].as_bool().unwrap());
    assert!(
        tbm_json["is_torque_within_machine_capacity"]
            .as_bool()
            .unwrap()
    );
    assert!(
        tbm_json["is_thrust_within_machine_capacity"]
            .as_bool()
            .unwrap()
    );

    // 184. Prestressed Post-Tensioned Concrete Girder Friction & Losses (ACI 318 / AASHTO)
    let res_prestress = post(
        addr,
        "/api/sim/structural/prestressed_concrete",
        r#"{
            "span_length_m": 32.0,
            "concrete_area_m2": 0.525,
            "concrete_moment_of_inertia_m4": 0.0785,
            "distance_to_top_fiber_m": 0.75,
            "distance_to_bottom_fiber_m": 0.65,
            "concrete_f_c_mpa": 45.0,
            "concrete_f_ci_mpa": 32.0,
            "tendon_steel_area_mm2": 2800.0,
            "tendon_modulus_mpa": 195000.0,
            "jacking_stress_ratio_of_fpu": 0.75,
            "midspan_eccentricity_m": 0.48,
            "end_eccentricity_m": 0.08,
            "service_moment_midspan_kn_m": 2150.0
        }"#,
    )?;
    assert_eq!(res_prestress.status, 200);
    let prestress_json = json(&res_prestress)?;
    assert!(prestress_json["initial_jacking_force_kn"].as_f64().unwrap() > 3800.0);
    assert!(prestress_json["friction_loss_pct"].as_f64().unwrap() > 1.0);
    assert!(
        prestress_json["anchorage_seating_length_m"]
            .as_f64()
            .unwrap()
            > 8.0
    );
    assert!(
        prestress_json["total_final_prestress_loss_pct"]
            .as_f64()
            .unwrap()
            > 10.0
    );
    assert!(
        prestress_json["final_effective_midspan_force_kn"]
            .as_f64()
            .unwrap()
            > 3000.0
    );
    assert!(
        prestress_json["is_service_tension_compliant"]
            .as_bool()
            .unwrap()
    );
    assert!(
        prestress_json["is_service_compression_compliant"]
            .as_bool()
            .unwrap()
    );
    assert!(
        prestress_json["is_jacking_stress_compliant"]
            .as_bool()
            .unwrap()
    );

    // 185. Spacecraft Reaction Wheel Dynamic Jitter & Micro-Vibration (NASA-HDBK-7005 / ECSS)
    let res_jitter = post(
        addr,
        "/api/sim/aerospace/spacecraft_jitter",
        r#"{
            "wheel_speed_rpm": 3600.0,
            "rotor_inertia_kg_m2": 0.045,
            "rotor_mass_kg": 5.50,
            "static_imbalance_g_mm": 1.80,
            "dynamic_imbalance_g_mm2": 35.0,
            "number_of_bearing_balls": 8,
            "structural_resonance_freq_hz": 85.0,
            "satellite_inertia_kg_m2": 320.0,
            "allowable_pointing_jitter_urad": 2.50
        }"#,
    )?;
    assert_eq!(res_jitter.status, 200);
    let jitter_json = json(&res_jitter)?;
    assert_eq!(jitter_json["spin_frequency_hz"].as_f64().unwrap(), 60.0);
    assert!(jitter_json["static_disturbance_force_n"].as_f64().unwrap() > 0.20);
    assert!(
        jitter_json["dynamic_disturbance_torque_mn_m"]
            .as_f64()
            .unwrap()
            > 4.0
    );
    assert!(jitter_json["bearing_bpfo_frequency_hz"].as_f64().unwrap() > 180.0);
    assert!(jitter_json["bearing_bpfi_frequency_hz"].as_f64().unwrap() > 260.0);
    assert!(
        jitter_json["payload_pointing_jitter_urad"]
            .as_f64()
            .unwrap()
            < 2.50
    );
    assert!(jitter_json["is_jitter_compliant"].as_bool().unwrap());
    assert!(jitter_json["is_resonance_avoided"].as_bool().unwrap());

    // 186. Autonomous Vehicle 77 GHz FMCW Radar & ToF LiDAR (ITU-R P.838 / Kruse / ISO 26262)
    let res_radar = post(
        addr,
        "/api/sim/automotive/radar_lidar_sensor",
        r#"{
            "target_range_m": 120.0,
            "target_rcs_m2": 5.0,
            "target_optical_reflectivity": 0.18,
            "radar_tx_power_w": 0.10,
            "radar_antenna_gain_dbi": 26.0,
            "lidar_pulse_power_w": 75.0,
            "required_snr_threshold_db": 12.0
        }"#,
    )?;
    assert_eq!(res_radar.status, 200);
    let radar_json = json(&res_radar)?;
    assert!(radar_json["radar_wavelength_mm"].as_f64().unwrap() > 3.80);
    assert!((radar_json["radar_range_resolution_m"].as_f64().unwrap() - 0.15).abs() < 0.02);
    assert!(radar_json["radar_snr_at_target_db"].as_f64().unwrap() > 15.0);
    assert!(radar_json["radar_max_detection_range_m"].as_f64().unwrap() > 180.0);
    assert!(radar_json["is_radar_target_detected"].as_bool().unwrap());
    assert!(
        radar_json["is_sensor_fusion_redundancy_achieved"]
            .as_bool()
            .unwrap()
    );

    // 187. Autonomous Underwater Vehicle (AUV) Hydrodynamics & Endurance (ITTC-78 / Hoerner)
    let res_auv = post(
        addr,
        "/api/sim/marine/auv_propulsion",
        r#"{
            "cruising_speed_knots": 3.50,
            "vehicle_length_m": 4.80,
            "vehicle_diameter_m": 0.533,
            "wetted_surface_area_m2": 7.20,
            "battery_capacity_kwh": 24.0,
            "hotel_power_draw_w": 120.0
        }"#,
    )?;
    assert_eq!(res_auv.status, 200);
    let auv_json = json(&res_auv)?;
    assert!(auv_json["cruising_speed_m_s"].as_f64().unwrap() > 1.70);
    assert!(auv_json["reynolds_number"].as_f64().unwrap() > 7.0e6);
    assert!(auv_json["total_resistance_force_n"].as_f64().unwrap() > 40.0);
    assert!(auv_json["total_electrical_power_w"].as_f64().unwrap() > 180.0);
    assert!(auv_json["mission_endurance_hours"].as_f64().unwrap() > 40.0);
    assert!(auv_json["mission_range_km"].as_f64().unwrap() > 250.0);
    assert!(auv_json["is_diurnal_endurance_achieved"].as_bool().unwrap());
    assert!(auv_json["is_propulsion_power_safe"].as_bool().unwrap());

    // 188. Universal Commands Execution Verification (Batch 19 Domains)
    for cmd_id in [
        "sim.geotechnical.tbm_tunneling",
        "sim.structural.prestressed_concrete",
        "sim.aerospace.spacecraft_jitter",
        "sim.automotive.radar_lidar_sensor",
        "sim.marine.auv_propulsion",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 189. Enterprise MCP Tool Calling (Batch 19 Domains)
    for tool_name in [
        "atlas_sim_geotechnical_tbm_tunneling",
        "atlas_sim_structural_prestressed_concrete",
        "atlas_sim_aerospace_spacecraft_jitter",
        "atlas_sim_automotive_radar_lidar_sensor",
        "atlas_sim_marine_auv_propulsion",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 189, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 190. Hydroelectric Turbine Governor, Penstock Surge & Wicket Gate Dynamics (Allievi / IEEE Std 1207)
    let hydro_res = post(
        addr,
        "/api/sim/energy/hydro_governor",
        r#"{
            "rated_net_head_m": 145.0,
            "rated_discharge_m3_s": 24.0,
            "rated_speed_rpm": 500.0,
            "rotor_inertia_kg_m2": 125000.0,
            "penstock_length_m": 420.0,
            "penstock_diameter_m": 2.20,
            "penstock_wall_thickness_mm": 22.0,
            "wicket_gate_closure_time_s": 6.50
        }"#,
    )?;
    assert_eq!(hydro_res.status, 200);
    let hydro_json = json(&hydro_res)?;
    assert!(hydro_json["rated_mechanical_power_mw"].as_f64().unwrap() > 30.0);
    assert!(hydro_json["water_hammer_wave_speed_m_s"].as_f64().unwrap() > 1000.0);
    assert!(hydro_json["water_starting_time_tw_s"].as_f64().unwrap() > 1.5);
    assert!(hydro_json["max_head_rise_m"].as_f64().unwrap() > 30.0);
    assert!(hydro_json["max_head_rise_pct"].as_f64().unwrap() < 40.0);
    assert!(hydro_json["max_overspeed_pct"].as_f64().unwrap() < 55.0);
    assert!(
        hydro_json["penstock_stress_safety_factor"]
            .as_f64()
            .unwrap()
            >= 1.50
    );
    assert!(
        hydro_json["is_water_hammer_pressure_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(hydro_json["is_overspeed_acceptable"].as_bool().unwrap());
    assert!(
        hydro_json["is_penstock_stress_acceptable"]
            .as_bool()
            .unwrap()
    );

    // 191. Semiconductor CMP Removal Rate, Dishing & Erosion (SEMI / Preston)
    let cmp_res = post(
        addr,
        "/api/sim/semiconductor/cmp_planarization",
        r#"{
            "wafer_diameter_mm": 300.0,
            "down_force_pressure_kpa": 20.68,
            "relative_velocity_m_s": 1.25,
            "polish_time_seconds": 60.0,
            "initial_step_height_nm": 450.0,
            "pattern_line_width_um": 2.50,
            "pattern_metal_density_pct": 50.0
        }"#,
    )?;
    assert_eq!(cmp_res.status, 200);
    let cmp_json = json(&cmp_res)?;
    assert!(cmp_json["preston_mrr_nm_per_min"].as_f64().unwrap() > 300.0);
    assert!(cmp_json["total_material_removed_nm"].as_f64().unwrap() > 350.0);
    assert!(cmp_json["planarization_efficiency_pct"].as_f64().unwrap() > 80.0);
    assert!(cmp_json["oxide_dishing_nm"].as_f64().unwrap() < 50.0);
    assert!(cmp_json["is_dishing_acceptable"].as_bool().unwrap());
    assert!(cmp_json["is_erosion_acceptable"].as_bool().unwrap());
    assert!(cmp_json["is_uniformity_acceptable"].as_bool().unwrap());

    // 192. Flexible Asphalt Pavement Structural Response, Fatigue & Rutting (AASHTO MEPDG)
    let pave_res = post(
        addr,
        "/api/sim/transportation/pavement_mepdg",
        r#"{
            "asphalt_thickness_mm": 150.0,
            "base_thickness_mm": 200.0,
            "subbase_thickness_mm": 250.0,
            "asphalt_dynamic_modulus_mpa": 3200.0,
            "base_resilient_modulus_mpa": 280.0,
            "subgrade_resilient_modulus_mpa": 65.0,
            "design_traffic_millions_esal": 8.50
        }"#,
    )?;
    assert_eq!(pave_res.status, 200);
    let pave_json = json(&pave_res)?;
    assert_eq!(
        pave_json["total_structure_thickness_mm"].as_f64().unwrap(),
        600.0
    );
    assert!(
        pave_json["asphalt_bottom_tensile_strain_ue"]
            .as_f64()
            .unwrap()
            > 50.0
    );
    assert!(
        pave_json["subgrade_top_compressive_strain_ue"]
            .as_f64()
            .unwrap()
            > 100.0
    );
    assert!(pave_json["fatigue_damage_ratio"].as_f64().unwrap() <= 1.0);
    assert!(pave_json["rutting_damage_ratio"].as_f64().unwrap() <= 1.0);
    assert!(pave_json["predicted_total_rut_depth_mm"].as_f64().unwrap() <= 12.5);
    assert!(pave_json["is_fatigue_life_adequate"].as_bool().unwrap());
    assert!(pave_json["is_rutting_life_adequate"].as_bool().unwrap());
    assert!(
        pave_json["is_pavement_structure_adequate"]
            .as_bool()
            .unwrap()
    );

    // 193. Biomaterial Tissue Engineering Scaffold Porosity, Permeability & Degradation (ISO 13781)
    let scaf_res = post(
        addr,
        "/api/sim/biomedical/scaffold_degradation",
        r#"{
            "scaffold_diameter_mm": 10.0,
            "scaffold_height_mm": 15.0,
            "target_porosity_pct": 75.0,
            "mean_pore_size_um": 350.0,
            "degradation_evaluation_weeks": 12.0
        }"#,
    )?;
    assert_eq!(scaf_res.status, 200);
    let scaf_json = json(&scaf_res)?;
    assert!(scaf_json["darcy_permeability_m2"].as_f64().unwrap() > 1e-11);
    assert!(
        scaf_json["initial_compressive_modulus_mpa"]
            .as_f64()
            .unwrap()
            > 150.0
    );
    assert!(
        scaf_json["degraded_compressive_modulus_mpa"]
            .as_f64()
            .unwrap()
            > 80.0
    );
    assert!(
        scaf_json["is_osteoconductive_architecture_favorable"]
            .as_bool()
            .unwrap()
    );
    assert!(scaf_json["is_shear_stress_osteogenic"].as_bool().unwrap());
    assert!(
        scaf_json["is_initial_modulus_sufficient"]
            .as_bool()
            .unwrap()
    );
    assert!(
        scaf_json["is_degradation_profile_biocompatible"]
            .as_bool()
            .unwrap()
    );

    // 194. Satellite Orbital Decay & Atmospheric Re-entry Lifetime (King-Hele / ISO 27852)
    let decay_res = post(
        addr,
        "/api/sim/aerospace/satellite_orbital_decay",
        r#"{
            "satellite_mass_kg": 12.0,
            "drag_cross_section_area_m2": 0.045,
            "drag_coefficient_cd": 2.20,
            "initial_perigee_altitude_km": 420.0,
            "initial_apogee_altitude_km": 440.0,
            "orbit_inclination_deg": 51.6
        }"#,
    )?;
    assert_eq!(decay_res.status, 200);
    let decay_json = json(&decay_res)?;
    assert!(
        decay_json["ballistic_coefficient_m2_per_kg"]
            .as_f64()
            .unwrap()
            > 0.006
    );
    assert!(
        decay_json["initial_orbital_period_minutes"]
            .as_f64()
            .unwrap()
            > 90.0
    );
    assert!(decay_json["initial_decay_rate_m_per_day"].as_f64().unwrap() > 10.0);
    assert!(decay_json["orbital_lifetime_years"].as_f64().unwrap() < 10.0);
    assert!(decay_json["is_iadc_25yr_compliant"].as_bool().unwrap());

    // 195. Universal Commands Execution Verification (Batch 20 Domains)
    for cmd_id in [
        "sim.energy.hydro_governor",
        "sim.semiconductor.cmp_planarization",
        "sim.transportation.pavement_mepdg",
        "sim.biomedical.scaffold_degradation",
        "sim.aerospace.satellite_orbital_decay",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 196. Enterprise MCP Tool Calling (Batch 20 Domains)
    for tool_name in [
        "atlas_sim_energy_hydro_governor",
        "atlas_sim_semiconductor_cmp_planarization",
        "atlas_sim_transportation_pavement_mepdg",
        "atlas_sim_biomedical_scaffold_degradation",
        "atlas_sim_aerospace_satellite_orbital_decay",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 196, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 197. Mining Bucket Wheel Excavator Digging Mechanics (ISO 5048 / Krause)
    let excavator_res = post(
        addr,
        "/api/sim/mining/bucket_wheel_excavator",
        r#"{
            "material_class": "ClaySiltOverburden",
            "wheel_diameter_m": 12.50,
            "bucket_count": 14,
            "bucket_nominal_volume_m3": 1.80,
            "wheel_speed_rpm": 4.80,
            "bucket_fill_factor": 1.05,
            "total_cutting_lip_length_m": 2.40,
            "lineal_cutting_resistance_kn_m": 110.0,
            "boom_length_m": 42.0,
            "slewing_speed_deg_s": 0.25,
            "upperstructure_mass_tons": 2400.0,
            "stabilizing_moment_kn_m": 650000.0,
            "mechanical_efficiency": 0.88
        }"#,
    )?;
    assert_eq!(excavator_res.status, 200);
    let excavator_json = json(&excavator_res)?;
    assert!(
        excavator_json["theoretical_capacity_m3_per_h"]
            .as_f64()
            .unwrap()
            > 7000.0
    );
    assert!(
        excavator_json["tangential_digging_force_kn"]
            .as_f64()
            .unwrap()
            > 200.0
    );
    assert!(excavator_json["required_drive_power_kw"].as_f64().unwrap() > 800.0);
    assert!(excavator_json["stability_safety_factor"].as_f64().unwrap() > 1.30);
    assert!(
        excavator_json["is_cutting_speed_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(excavator_json["is_drive_power_feasible"].as_bool().unwrap());
    assert!(excavator_json["is_machine_stable"].as_bool().unwrap());

    // 198. Two-Phase Gas-Liquid Pipe Flow Assurance (Lockhart-Martinelli / API RP 14E)
    let multiphase_res = post(
        addr,
        "/api/sim/process/multiphase_pipe_flow",
        r#"{
            "pipe_length_m": 8500.0,
            "pipe_inner_diameter_mm": 254.0,
            "elevation_change_m": 120.0,
            "liquid_mass_flow_kg_s": 35.0,
            "gas_mass_flow_kg_s": 4.50,
            "liquid_density_kg_m3": 840.0,
            "gas_density_kg_m3": 42.0,
            "liquid_viscosity_mpa_s": 2.80,
            "gas_viscosity_mpa_s": 0.016,
            "pipe_roughness_mm": 0.045,
            "api_erosion_constant_c": 122.0
        }"#,
    )?;
    assert_eq!(multiphase_res.status, 200);
    let multiphase_json = json(&multiphase_res)?;
    assert!(multiphase_json["mixture_density_kg_m3"].as_f64().unwrap() > 100.0);
    assert!(multiphase_json["mixture_velocity_m_s"].as_f64().unwrap() > 1.0);
    assert!(
        multiphase_json["lockhart_martinelli_param_x"]
            .as_f64()
            .unwrap()
            > 0.5
    );
    assert!(
        multiphase_json["in_situ_liquid_holdup_hl"]
            .as_f64()
            .unwrap()
            > 0.1
    );
    assert!(multiphase_json["total_pressure_drop_bar"].as_f64().unwrap() > 1.0);
    assert!(
        multiphase_json["is_erosion_velocity_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        multiphase_json["is_pressure_drop_acceptable"]
            .as_bool()
            .unwrap()
    );

    // 199. Geothermal Reservoir Doublet Well Heat Extraction (Gringarten / Bodvarsson)
    let geothermal_res = post(
        addr,
        "/api/sim/energy/geothermal_reservoir",
        r#"{
            "rock_type": "SandstoneAquifer",
            "reservoir_temperature_c": 160.0,
            "reinjection_temperature_c": 65.0,
            "brine_flow_rate_l_s": 75.0,
            "reservoir_thickness_m": 80.0,
            "well_spacing_distance_m": 1200.0,
            "wellbore_radius_m": 0.108,
            "reservoir_permeability_md": 450.0,
            "reservoir_porosity": 0.12,
            "pump_system_efficiency": 0.72,
            "project_lifespan_years": 30.0
        }"#,
    )?;
    assert_eq!(geothermal_res.status, 200);
    let geothermal_json = json(&geothermal_res)?;
    assert!(
        geothermal_json["initial_thermal_power_mw_th"]
            .as_f64()
            .unwrap()
            > 25.0
    );
    assert!(
        geothermal_json["parasitic_pump_power_kw_e"]
            .as_f64()
            .unwrap()
            > 50.0
    );
    assert!(geothermal_json["thermal_cop"].as_f64().unwrap() > 100.0);
    assert!(
        geothermal_json["thermal_breakthrough_years"]
            .as_f64()
            .unwrap()
            > 20.0
    );
    assert!(
        geothermal_json["is_breakthrough_delayed_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        geothermal_json["is_pumping_power_viable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        geothermal_json["is_thermal_cop_sustainable"]
            .as_bool()
            .unwrap()
    );

    // 200. Spray Drying Atomization, Droplet Evaporation & Powder Moisture (Masters / ASABE)
    let spray_res = post(
        addr,
        "/api/sim/process/spray_drying_kinetics",
        r#"{
            "product_type": "DairyMilkPowder",
            "feed_rate_kg_h": 1200.0,
            "feed_solids_fraction": 0.45,
            "feed_viscosity_mpa_s": 18.0,
            "feed_surface_tension_mn_m": 48.0,
            "atomizer_wheel_diameter_mm": 210.0,
            "atomizer_speed_rpm": 16000.0,
            "inlet_air_temperature_c": 195.0,
            "outlet_air_temperature_c": 88.0,
            "ambient_air_temperature_c": 20.0,
            "ambient_air_rh_pct": 50.0,
            "target_product_moisture_pct": 3.50
        }"#,
    )?;
    assert_eq!(spray_res.status, 200);
    let spray_json = json(&spray_res)?;
    assert!(spray_json["sauter_mean_diameter_d32_um"].as_f64().unwrap() > 20.0);
    assert!(spray_json["droplet_drying_time_seconds"].as_f64().unwrap() < 5.0);
    assert!(spray_json["water_evaporation_rate_kg_h"].as_f64().unwrap() > 600.0);
    assert!(
        spray_json["dry_powder_production_rate_kg_h"]
            .as_f64()
            .unwrap()
            > 500.0
    );
    assert!(spray_json["is_drying_time_adequate"].as_bool().unwrap());
    assert!(
        spray_json["is_stickiness_risk_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        spray_json["is_thermal_efficiency_acceptable"]
            .as_bool()
            .unwrap()
    );

    // 201. Radioisotope Thermoelectric Generator (RTG) (NASA SP-8000 / Ioffe)
    let rtg_res = post(
        addr,
        "/api/sim/energy/thermoelectric_rtg",
        r#"{
            "material_type": "SiliconGermaniumSiGe",
            "initial_thermal_power_w_th": 4400.0,
            "hot_junction_temp_k": 1273.15,
            "cold_junction_temp_k": 573.15,
            "couple_count": 572,
            "leg_length_mm": 20.0,
            "leg_area_mm2": 9.0,
            "mission_elapsed_years": 17.0,
            "fuel_half_life_years": 87.7,
            "annual_junction_degradation_pct": 0.70
        }"#,
    )?;
    assert_eq!(rtg_res.status, 200);
    let rtg_json = json(&rtg_res)?;
    assert!(rtg_json["zt_figure_of_merit"].as_f64().unwrap() > 0.7);
    assert!(rtg_json["conversion_efficiency_pct"].as_f64().unwrap() > 5.0);
    assert!(rtg_json["bol_electric_power_w_e"].as_f64().unwrap() > 250.0);
    assert!(rtg_json["eol_electric_power_w_e"].as_f64().unwrap() > 180.0);
    assert!(rtg_json["is_bus_voltage_adequate"].as_bool().unwrap());
    assert!(rtg_json["is_eol_power_sufficient"].as_bool().unwrap());
    assert!(
        rtg_json["is_conversion_efficiency_acceptable"]
            .as_bool()
            .unwrap()
    );

    // 202. Universal Commands Execution Verification (Batch 21 Domains)
    for cmd_id in [
        "sim.mining.bucket_wheel_excavator",
        "sim.process.multiphase_pipe_flow",
        "sim.energy.geothermal_reservoir",
        "sim.process.spray_drying_kinetics",
        "sim.energy.thermoelectric_rtg",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 203. Enterprise MCP Tool Calling (Batch 21 Domains)
    for tool_name in [
        "atlas_sim_mining_bucket_wheel_excavator",
        "atlas_sim_process_multiphase_pipe_flow",
        "atlas_sim_energy_geothermal_reservoir",
        "atlas_sim_process_spray_drying_kinetics",
        "atlas_sim_energy_thermoelectric_rtg",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 203, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 204. Nuclear Reactor Containment Post-LOCA Dynamics (10 CFR 50 / Tagami / Uchida)
    let loca_res = post(
        addr,
        "/api/sim/nuclear/containment_loca",
        r#"{
            "containment_type": "PrestressedConcreteLiner",
            "containment_free_volume_m3": 65000.0,
            "design_pressure_bar_a": 4.80,
            "design_temperature_c": 150.0,
            "initial_pressure_bar_a": 1.013,
            "initial_temperature_c": 40.0,
            "initial_relative_humidity_pct": 50.0,
            "blowdown_coolant_mass_tonnes": 260.0,
            "blowdown_enthalpy_gj": 380.0,
            "blowdown_duration_seconds": 32.0,
            "heat_sink_surface_area_m2": 32000.0,
            "spray_flow_rate_l_s": 650.0,
            "spray_temperature_c": 32.0
        }"#,
    )?;
    assert_eq!(loca_res.status, 200);
    let loca_json = json(&loca_res)?;
    assert!(
        loca_json["volumetric_energy_release_rate_kw_per_m3"]
            .as_f64()
            .unwrap()
            > 100.0
    );
    assert!(loca_json["tagami_condensing_htc_w_m2_k"].as_f64().unwrap() > 500.0);
    assert!(
        loca_json["peak_containment_pressure_bar_a"]
            .as_f64()
            .unwrap()
            < 4.80
    );
    assert!(loca_json["pressure_margin_pct"].as_f64().unwrap() >= 5.0);
    assert!(loca_json["spray_heat_removal_mw_th"].as_f64().unwrap() > 50.0);
    assert!(
        loca_json["is_pressure_margin_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(loca_json["is_temperature_within_design"].as_bool().unwrap());
    assert!(
        loca_json["is_24h_depressurization_compliant"]
            .as_bool()
            .unwrap()
    );
    assert!(
        loca_json["is_containment_integrity_verified"]
            .as_bool()
            .unwrap()
    );

    // 205. Atmospheric Pollutant Gaussian Plume Dispersion (EPA AERMOD / Pasquill-Gifford / Briggs)
    let plume_res = post(
        addr,
        "/api/sim/environmental/atmospheric_dispersion",
        r#"{
            "stability_class": "ClassDNeutral",
            "pollutant": "SulfurDioxideSO2",
            "emission_rate_g_s": 85.0,
            "stack_height_m": 80.0,
            "stack_inside_diameter_m": 2.80,
            "stack_gas_velocity_m_s": 18.5,
            "stack_gas_temperature_c": 140.0,
            "ambient_air_temperature_c": 20.0,
            "wind_speed_at_stack_height_m_s": 5.50,
            "receptor_downwind_distance_m": 8400.0,
            "receptor_crosswind_distance_m": 0.0
        }"#,
    )?;
    assert_eq!(plume_res.status, 200);
    let plume_json = json(&plume_res)?;
    assert!(plume_json["buoyancy_flux_m4_per_s3"].as_f64().unwrap() > 50.0);
    assert!(plume_json["plume_rise_delta_h_m"].as_f64().unwrap() > 50.0);
    assert!(plume_json["effective_stack_height_m"].as_f64().unwrap() > 130.0);
    assert!(
        plume_json["peak_max_ground_concentration_ug_per_m3"]
            .as_f64()
            .unwrap()
            < 196.0
    );
    assert!(plume_json["is_naaqs_compliant"].as_bool().unwrap());
    assert!(plume_json["is_plume_rise_adequate"].as_bool().unwrap());

    // 206. Multistage Rocket Ascent Trajectory & Orbital Injection (Tsiolkovsky / NASA SP-8010)
    let rocket_res = post(
        addr,
        "/api/sim/aerospace/rocket_ascent_trajectory",
        r#"{
            "staging_type": "TwoStageToOrbit",
            "gross_liftoff_mass_kg": 549054.0,
            "payload_mass_kg": 15600.0,
            "stage1_propellant_mass_kg": 411000.0,
            "stage1_dry_mass_kg": 25600.0,
            "stage1_thrust_sl_kn": 7607.0,
            "stage1_isp_vac_s": 311.0,
            "stage1_burn_time_s": 162.0,
            "stage2_propellant_mass_kg": 92670.0,
            "stage2_dry_mass_kg": 4000.0,
            "stage2_thrust_vac_kn": 981.0,
            "stage2_isp_vac_s": 348.0,
            "aerodynamic_reference_area_m2": 10.52,
            "drag_coefficient_cd": 0.32,
            "target_orbit_altitude_km": 300.0,
            "launch_latitude_deg": 28.5
        }"#,
    )?;
    assert_eq!(rocket_res.status, 200);
    let rocket_json = json(&rocket_res)?;
    assert!(
        rocket_json["liftoff_thrust_to_weight_ratio"]
            .as_f64()
            .unwrap()
            > 1.20
    );
    assert!(rocket_json["total_ideal_delta_v_m_s"].as_f64().unwrap() > 9000.0);
    assert!(
        rocket_json["achieved_orbital_velocity_m_s"]
            .as_f64()
            .unwrap()
            > 7700.0
    );
    assert!(rocket_json["max_q_dynamic_pressure_kpa"].as_f64().unwrap() < 45.0);
    assert!(rocket_json["is_liftoff_thrust_adequate"].as_bool().unwrap());
    assert!(
        rocket_json["is_orbit_injection_achieved"]
            .as_bool()
            .unwrap()
    );
    assert!(
        rocket_json["is_max_q_within_structural_limits"]
            .as_bool()
            .unwrap()
    );

    // 207. Concrete Gravity Dam Overturning & Sliding Stability (USACE EM 1110-2-2200 / FERC)
    let dam_res = post(
        addr,
        "/api/sim/civil/gravity_dam_stability",
        r#"{
            "load_combination": "UsualNormalOperating",
            "foundation_rock": "CompetentSedimentary",
            "dam_height_m": 85.0,
            "crest_width_m": 8.50,
            "base_width_m": 68.0,
            "upstream_water_depth_m": 80.0,
            "downstream_tailwater_depth_m": 6.0,
            "silt_deposit_depth_m": 12.0,
            "drainage_gallery_distance_from_heel_m": 10.0,
            "drainage_efficiency_factor": 0.67,
            "horizontal_seismic_coef_kh": 0.05,
            "concrete_unit_weight_kn_m3": 24.0,
            "allowable_bearing_capacity_mpa": 8.0
        }"#,
    )?;
    assert_eq!(dam_res.status, 200);
    let dam_json = json(&dam_res)?;
    assert!(dam_json["total_concrete_weight_kn_per_m"].as_f64().unwrap() > 70000.0);
    assert!(dam_json["overturning_safety_factor"].as_f64().unwrap() >= 1.75);
    assert!(dam_json["sliding_shear_friction_factor"].as_f64().unwrap() >= 1.50);
    assert!(dam_json["toe_bearing_stress_mpa"].as_f64().unwrap() < 8.0);
    assert!(dam_json["is_sliding_stable"].as_bool().unwrap());
    assert!(dam_json["is_overturning_stable"].as_bool().unwrap());
    assert!(
        dam_json["is_no_tension_kern_rule_satisfied"]
            .as_bool()
            .unwrap()
    );
    assert!(dam_json["is_dam_stability_verified"].as_bool().unwrap());

    // 208. Water Electrolyzer Hydrogen Production & Cell Polarization (Butler-Volmer / ISO 22734)
    let h2_res = post(
        addr,
        "/api/sim/energy/electrolyzer_hydrogen",
        r#"{
            "technology": "ProtonExchangeMembranePEM",
            "cell_temperature_c": 70.0,
            "hydrogen_operating_pressure_bar_a": 30.0,
            "current_density_a_per_cm2": 1.80,
            "active_cell_area_cm2": 600.0,
            "cell_count": 120,
            "membrane_thickness_um": 90.0,
            "membrane_water_content_lambda": 18.0,
            "anode_exchange_current_density_a_cm2": 0.0015,
            "cathode_exchange_current_density_a_cm2": 0.015,
            "limiting_current_density_a_cm2": 3.80
        }"#,
    )?;
    assert_eq!(h2_res.status, 200);
    let h2_json = json(&h2_res)?;
    assert!(h2_json["operating_cell_voltage_v"].as_f64().unwrap() > 1.80);
    assert!(h2_json["operating_cell_voltage_v"].as_f64().unwrap() < 2.30);
    assert!(
        h2_json["hydrogen_production_rate_kg_per_h"]
            .as_f64()
            .unwrap()
            > 4.5
    );
    assert!(
        h2_json["specific_energy_consumption_kwh_per_kg"]
            .as_f64()
            .unwrap()
            < 58.0
    );
    assert!(h2_json["stack_hhv_efficiency_pct"].as_f64().unwrap() > 65.0);
    assert!(h2_json["is_cell_voltage_acceptable"].as_bool().unwrap());
    assert!(h2_json["is_specific_energy_acceptable"].as_bool().unwrap());
    assert!(h2_json["is_stack_efficiency_acceptable"].as_bool().unwrap());
    assert!(h2_json["is_electrolyzer_viable"].as_bool().unwrap());

    // 209. Universal Commands Execution Verification (Batch 22 Domains)
    for cmd_id in [
        "sim.nuclear.containment_loca",
        "sim.environmental.atmospheric_dispersion",
        "sim.aerospace.rocket_ascent_trajectory",
        "sim.civil.gravity_dam_stability",
        "sim.energy.electrolyzer_hydrogen",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 210. Enterprise MCP Tool Calling (Batch 22 Domains)
    for tool_name in [
        "atlas_sim_nuclear_containment_loca",
        "atlas_sim_environmental_atmospheric_dispersion",
        "atlas_sim_aerospace_rocket_ascent_trajectory",
        "atlas_sim_civil_gravity_dam_stability",
        "atlas_sim_energy_electrolyzer_hydrogen",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 210, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 211. Optical Thin-Film Multilayer Coating (Fresnel / TMM / ISO 9211)
    let optics_res = post(
        addr,
        "/api/sim/optical/thin_film_optics",
        r#"{
            "coating_type": "DistributedBraggReflector",
            "polarization": "UnpolarizedAverage",
            "wavelength_nm": 632.8,
            "angle_of_incidence_deg": 0.0,
            "incident_medium_index": 1.000,
            "substrate_index": 1.52
        }"#,
    )?;
    assert_eq!(optics_res.status, 200);
    let optics_json = json(&optics_res)?;
    assert!(optics_json["reflectance_pct"].as_f64().unwrap() > 95.0);
    assert!(optics_json["transmittance_pct"].as_f64().unwrap() < 5.0);
    assert!(optics_json["dbr_stopband_bandwidth_nm"].as_f64().unwrap() > 50.0);
    assert!(optics_json["is_energy_conserved"].as_bool().unwrap());
    assert!(
        optics_json["is_target_reflectance_achieved"]
            .as_bool()
            .unwrap()
    );
    assert!(
        optics_json["is_thickness_manufacturable"]
            .as_bool()
            .unwrap()
    );
    assert!(optics_json["is_optical_design_viable"].as_bool().unwrap());

    // 212. Cryogenic Linde-Hampson Liquefaction & Joule-Thomson (ISO 21009 / Barron)
    let cryo_res = post(
        addr,
        "/api/sim/energy/cryogenic_liquefaction",
        r#"{
            "fluid_type": "NitrogenN2",
            "feed_gas_flow_rate_kg_h": 1200.0,
            "suction_pressure_bar_a": 1.05,
            "discharge_pressure_bar_a": 200.0,
            "ambient_aftercooler_temp_c": 20.0,
            "heat_exchanger_pinch_k": 4.0,
            "compressor_isothermal_efficiency": 0.72,
            "heat_exchanger_effectiveness": 0.97,
            "ambient_heat_leak_w": 180.0
        }"#,
    )?;
    assert_eq!(cryo_res.status, 200);
    let cryo_json = json(&cryo_res)?;
    assert!(cryo_json["liquid_yield_fraction"].as_f64().unwrap() >= 0.05);
    assert!(cryo_json["liquid_production_rate_kg_h"].as_f64().unwrap() > 60.0);
    assert!(cryo_json["compressor_power_kw_e"].as_f64().unwrap() > 100.0);
    assert!(
        cryo_json["specific_energy_consumption_kwh_per_kg"]
            .as_f64()
            .unwrap()
            < 3.20
    );
    assert!(cryo_json["is_below_inversion_pressure"].as_bool().unwrap());
    assert!(cryo_json["is_liquid_yield_viable"].as_bool().unwrap());
    assert!(cryo_json["is_specific_power_acceptable"].as_bool().unwrap());
    assert!(cryo_json["is_liquefaction_cycle_viable"].as_bool().unwrap());

    // 213. Hydrocyclone Solid-Liquid Mineral Classification (Plitt / Lynch-Rao / ISO 13320)
    let cyclone_res = post(
        addr,
        "/api/sim/process/hydrocyclone_separation",
        r#"{
            "application": "HardRockGrindingCircuit",
            "cyclone_diameter_mm": 350.0,
            "inlet_diameter_mm": 75.0,
            "vortex_finder_diameter_mm": 115.0,
            "apex_diameter_mm": 55.0,
            "free_vortex_height_mm": 1400.0,
            "feed_flow_rate_m3_h": 120.0,
            "solids_volumetric_concentration_pct": 18.0,
            "solid_density_kg_m3": 2800.0,
            "liquid_density_kg_m3": 1000.0,
            "target_evaluation_size_um": 75.0
        }"#,
    )?;
    assert_eq!(cyclone_res.status, 200);
    let cyclone_json = json(&cyclone_res)?;
    assert!(cyclone_json["inlet_slurry_velocity_m_s"].as_f64().unwrap() >= 4.0);
    assert!(cyclone_json["pressure_drop_kpa"].as_f64().unwrap() >= 50.0);
    assert!(cyclone_json["corrected_cut_size_d50c_um"].as_f64().unwrap() > 20.0);
    assert!(cyclone_json["sharpness_index_m"].as_f64().unwrap() >= 1.5);
    assert!(
        cyclone_json["is_discharge_in_spray_regime"]
            .as_bool()
            .unwrap()
    );
    assert!(
        cyclone_json["is_pressure_drop_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(cyclone_json["is_inlet_velocity_optimal"].as_bool().unwrap());
    assert!(cyclone_json["is_classification_viable"].as_bool().unwrap());

    // 214. Active Magnetic Bearing (AMB) Electrodynamic Levitation (ISO 14839 / Schweitzer)
    let amb_res = post(
        addr,
        "/api/sim/mechanical/magnetic_bearing",
        r#"{
            "topology": "Heteropolar8Pole",
            "rotor_supported_mass_kg": 85.0,
            "max_rotational_speed_rpm": 36000.0,
            "nominal_air_gap_mm": 0.60,
            "pole_face_area_mm2": 850.0,
            "coil_turns_per_pole": 140,
            "bias_current_a": 5.0,
            "max_control_current_a": 12.0,
            "amplifier_bus_voltage_v": 180.0,
            "controller_proportional_gain_a_per_mm": 18.0,
            "controller_derivative_gain_a_s_per_m": 35.0,
            "rotor_unbalance_eccentricity_um": 0.50
        }"#,
    )?;
    assert_eq!(amb_res.status, 200);
    let amb_json = json(&amb_res)?;
    assert!(amb_json["current_gain_ki_n_per_a"].as_f64().unwrap() > 100.0);
    assert!(
        amb_json["open_loop_negative_stiffness_kx_kn_per_m"]
            .as_f64()
            .unwrap()
            > 500.0
    );
    assert!(
        amb_json["closed_loop_stiffness_k_net_kn_per_m"]
            .as_f64()
            .unwrap()
            > 500.0
    );
    assert!(
        amb_json["suspension_natural_frequency_hz"]
            .as_f64()
            .unwrap()
            > 25.0
    );
    assert!(amb_json["dynamic_load_safety_factor"].as_f64().unwrap() >= 2.0);
    assert!(amb_json["air_gap_displacement_ratio"].as_f64().unwrap() <= 0.30);
    assert!(
        amb_json["is_stiffness_positive_and_stable"]
            .as_bool()
            .unwrap()
    );
    assert!(amb_json["is_damping_ratio_adequate"].as_bool().unwrap());
    assert!(amb_json["is_air_gap_clearance_safe"].as_bool().unwrap());
    assert!(amb_json["is_load_capacity_sufficient"].as_bool().unwrap());
    assert!(amb_json["is_amb_system_viable"].as_bool().unwrap());

    // 215. Underwater Acoustic Sonar Propagation & Active Detection (Urick / Thorpe)
    let sonar_res = post(
        addr,
        "/api/sim/marine/sonar_propagation",
        r#"{
            "acoustic_regime": "ShallowWaterWaveguide",
            "target_class": "SubmarineLargeFleet",
            "operating_frequency_khz": 7.5,
            "transmitter_acoustic_power_w": 4500.0,
            "array_directivity_index_db": 22.0,
            "ocean_depth_m": 180.0,
            "target_range_m": 8500.0,
            "sea_state_code": 3,
            "shipping_traffic_index": 2,
            "detection_threshold_db": 14.0
        }"#,
    )?;
    assert_eq!(sonar_res.status, 200);
    let sonar_json = json(&sonar_res)?;
    assert!(sonar_json["source_level_db_re_upa"].as_f64().unwrap() > 210.0);
    assert!(
        sonar_json["absorption_coefficient_db_per_km"]
            .as_f64()
            .unwrap()
            > 0.1
    );
    assert!(sonar_json["one_way_transmission_loss_db"].as_f64().unwrap() > 40.0);
    assert!(sonar_json["signal_excess_se_db"].as_f64().unwrap() >= 0.0);
    assert!(sonar_json["figure_of_merit_fom_db"].as_f64().unwrap() > 100.0);
    assert!(sonar_json["is_target_detected"].as_bool().unwrap());
    assert!(
        sonar_json["is_cavitation_limit_satisfied"]
            .as_bool()
            .unwrap()
    );
    assert!(
        sonar_json["is_absorption_loss_favorable"]
            .as_bool()
            .unwrap()
    );
    assert!(sonar_json["is_sonar_detection_viable"].as_bool().unwrap());

    // 216. Universal Commands Execution Verification (Batch 23 Domains)
    for cmd_id in [
        "sim.optical.thin_film_optics",
        "sim.energy.cryogenic_liquefaction",
        "sim.process.hydrocyclone_separation",
        "sim.mechanical.magnetic_bearing",
        "sim.marine.sonar_propagation",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 217. Enterprise MCP Tool Calling (Batch 23 Domains)
    for tool_name in [
        "atlas_sim_optical_thin_film_optics",
        "atlas_sim_energy_cryogenic_liquefaction",
        "atlas_sim_process_hydrocyclone_separation",
        "atlas_sim_mechanical_magnetic_bearing",
        "atlas_sim_marine_sonar_propagation",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 217, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 218. Electron Beam Welding (EBW) & Thermal Keyhole Fusion (Goldak 3D / ISO 13919-1)
    let ebw_res = post(
        addr,
        "/api/sim/manufacturing/electron_beam_welding",
        r#"{
            "alloy_grade": "Ti6Al4V",
            "acceleration_voltage_kv": 120.0,
            "beam_current_ma": 50.0,
            "travel_speed_mm_s": 15.0,
            "plate_thickness_mm": 25.0,
            "beam_focal_diameter_mm": 0.45,
            "ambient_temperature_c": 25.0
        }"#,
    )?;
    assert_eq!(ebw_res.status, 200);
    let ebw_json = json(&ebw_res)?;
    assert!(ebw_json["gross_beam_power_kw"].as_f64().unwrap() >= 5.0);
    assert!(ebw_json["net_absorbed_power_kw"].as_f64().unwrap() > 4.5);
    assert!(ebw_json["linear_heat_input_j_per_mm"].as_f64().unwrap() >= 250.0);
    assert!(ebw_json["penetration_depth_mm"].as_f64().unwrap() >= 15.0);
    assert!(ebw_json["weld_aspect_ratio"].as_f64().unwrap() >= 4.0);
    assert!(ebw_json["cooling_time_t8_5_seconds"].as_f64().unwrap() > 0.10);
    assert!(ebw_json["is_narrow_keyhole_achieved"].as_bool().unwrap());
    assert!(ebw_json["is_cooling_time_acceptable"].as_bool().unwrap());
    assert!(
        ebw_json["is_iso_13919_class_b_compliant"]
            .as_bool()
            .unwrap()
    );
    assert!(ebw_json["is_ebw_process_viable"].as_bool().unwrap());

    // 219. Industrial Crystallization Kinetics & Crystal Size Distribution (MSMPR / Mersmann)
    let crys_res = post(
        addr,
        "/api/sim/process/crystallization_kinetics",
        r#"{
            "solute": "PotassiumChloride",
            "operating_temperature_c": 35.0,
            "feed_volumetric_flow_m3_h": 25.0,
            "crystallizer_active_volume_m3": 50.0,
            "solution_concentration_kg_m3": 395.0,
            "crystal_volumetric_shape_factor": 0.85,
            "target_median_size_um": 350.0
        }"#,
    )?;
    assert_eq!(crys_res.status, 200);
    let crys_json = json(&crys_res)?;
    assert!(
        crys_json["saturation_concentration_kg_m3"]
            .as_f64()
            .unwrap()
            > 350.0
    );
    assert!(crys_json["supersaturation_delta_c_kg_m3"].as_f64().unwrap() > 5.0);
    assert!(
        crys_json["relative_supersaturation_sigma"]
            .as_f64()
            .unwrap()
            > 0.01
    );
    assert!(crys_json["residence_time_tau_hours"].as_f64().unwrap() >= 1.5);
    assert!(crys_json["median_crystal_size_l50_um"].as_f64().unwrap() >= 300.0);
    assert!(crys_json["crystal_production_rate_kg_h"].as_f64().unwrap() > 200.0);
    assert!(
        crys_json["is_supersaturation_controlled"]
            .as_bool()
            .unwrap()
    );
    assert!(
        crys_json["is_target_crystal_size_achieved"]
            .as_bool()
            .unwrap()
    );
    assert!(crys_json["is_crystallization_viable"].as_bool().unwrap());

    // 220. Thermoelectric Peltier Heat Pump Cooler (Ioffe / Thomson / ISO 9001)
    let tec_res = post(
        addr,
        "/api/sim/thermal/thermoelectric_cooler",
        r#"{
            "material": "BismuthTelluride",
            "couple_count_n": 127,
            "pellet_area_mm2": 2.25,
            "pellet_length_mm": 1.80,
            "cold_junction_temp_c": 25.0,
            "hot_junction_temp_c": 50.0,
            "operating_current_a": 4.5,
            "heat_load_qc_target_w": 35.0
        }"#,
    )?;
    assert_eq!(tec_res.status, 200);
    let tec_json = json(&tec_res)?;
    assert_eq!(tec_json["temperature_lift_k"].as_f64().unwrap(), 25.0);
    assert!(tec_json["figure_of_merit_zt"].as_f64().unwrap() >= 0.70);
    assert!(tec_json["net_cooling_capacity_w"].as_f64().unwrap() > 25.0);
    assert!(tec_json["electrical_input_power_w"].as_f64().unwrap() > 30.0);
    assert!(tec_json["coefficient_of_performance_cop"].as_f64().unwrap() > 0.30);
    assert!(tec_json["is_target_heat_load_met"].as_bool().unwrap());
    assert!(
        tec_json["is_operating_below_thermal_runaway"]
            .as_bool()
            .unwrap()
    );
    assert!(
        tec_json["is_temperature_lift_achievable"]
            .as_bool()
            .unwrap()
    );
    assert!(tec_json["is_tec_cooling_viable"].as_bool().unwrap());

    // 221. Centrifugal Slurry Pump Hydraulic De-Rating & Wear Life (ANSI/HI 12.1-12.6 / Cave)
    let pump_res = post(
        addr,
        "/api/sim/mechanical/slurry_pump_derating",
        r#"{
            "abrasiveness_class": "Class3Severe",
            "slurry_flow_rate_m3_h": 650.0,
            "clear_water_head_m": 45.0,
            "clear_water_efficiency": 0.78,
            "solid_specific_gravity": 2.75,
            "solids_concentration_wt_pct": 35.0,
            "median_particle_size_d50_um": 110.0,
            "impeller_diameter_mm": 680.0,
            "pump_rotational_speed_rpm": 850.0,
            "allowable_wear_thickness_mm": 25.0
        }"#,
    )?;
    assert_eq!(pump_res.status, 200);
    let pump_json = json(&pump_res)?;
    assert!(
        pump_json["slurry_mixture_specific_gravity"]
            .as_f64()
            .unwrap()
            > 1.20
    );
    assert!(pump_json["head_derating_ratio_hr"].as_f64().unwrap() >= 0.85);
    assert!(pump_json["derated_slurry_head_m"].as_f64().unwrap() >= 38.0);
    assert!(pump_json["shaft_power_kw"].as_f64().unwrap() > 100.0);
    assert!(pump_json["impeller_tip_speed_m_s"].as_f64().unwrap() <= 32.0);
    assert!(pump_json["estimated_wear_life_hours"].as_f64().unwrap() >= 3000.0);
    assert!(pump_json["is_head_derating_acceptable"].as_bool().unwrap());
    assert!(pump_json["is_tip_speed_within_limits"].as_bool().unwrap());
    assert!(pump_json["is_wear_life_satisfactory"].as_bool().unwrap());
    assert!(pump_json["is_slurry_pump_viable"].as_bool().unwrap());

    // 222. Optical Single-Mode Fiber Chromatic Dispersion & Pulse Broadening (ITU-T G.652 / Sellmeier)
    let fiber_res = post(
        addr,
        "/api/sim/optical/fiber_dispersion",
        r#"{
            "fiber_standard": "ItuG652dStandard",
            "wavelength_nm": 1550.0,
            "fiber_length_km": 80.0,
            "laser_linewidth_fwhm_nm": 0.015,
            "initial_pulse_width_ps": 25.0,
            "launch_power_mw": 5.0,
            "target_bit_rate_gbps": 10.0
        }"#,
    )?;
    assert_eq!(fiber_res.status, 200);
    let fiber_json = json(&fiber_res)?;
    assert!(fiber_json["core_refractive_index"].as_f64().unwrap() > 1.44);
    assert!(
        fiber_json["chromatic_dispersion_ps_per_nm_km"]
            .as_f64()
            .unwrap()
            >= 15.0
    );
    assert!(
        fiber_json["total_accumulated_dispersion_ps_per_nm"]
            .as_f64()
            .unwrap()
            > 1200.0
    );
    assert!(fiber_json["total_link_attenuation_db"].as_f64().unwrap() >= 14.0);
    assert!(fiber_json["received_optical_power_mw"].as_f64().unwrap() > 0.05);
    assert!(
        fiber_json["pulse_broadening_delta_tau_ps"]
            .as_f64()
            .unwrap()
            > 15.0
    );
    assert!(
        fiber_json["is_inter_symbol_interference_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        fiber_json["is_received_optical_power_adequate"]
            .as_bool()
            .unwrap()
    );
    assert!(
        fiber_json["is_dispersion_penalty_tolerable"]
            .as_bool()
            .unwrap()
    );
    assert!(fiber_json["is_fiber_link_viable"].as_bool().unwrap());

    // 223. Universal Commands Execution Verification (Batch 24 Domains)
    for cmd_id in [
        "sim.manufacturing.electron_beam_welding",
        "sim.process.crystallization_kinetics",
        "sim.thermal.thermoelectric_cooler",
        "sim.mechanical.slurry_pump_derating",
        "sim.optical.fiber_dispersion",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 224. Enterprise MCP Tool Calling (Batch 24 Domains)
    for tool_name in [
        "atlas_sim_manufacturing_electron_beam_welding",
        "atlas_sim_process_crystallization_kinetics",
        "atlas_sim_thermal_thermoelectric_cooler",
        "atlas_sim_mechanical_slurry_pump_derating",
        "atlas_sim_optical_fiber_dispersion",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 224, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 225. Marine Wageningen B-Series Propeller Hydrodynamics & Cavitation Inception (ITTC 1978 / Oosterveld / Keller)
    let prop_res = post(
        addr,
        "/api/sim/marine/propeller_hydrodynamics",
        r#"{
            "blade_count": "FourBlades",
            "propeller_diameter_m": 5.60,
            "pitch_diameter_ratio": 0.95,
            "expanded_area_ratio": 0.62,
            "speed_of_advance_m_s": 7.20,
            "rotational_speed_rpm": 105.0,
            "shaft_immersion_depth_m": 5.20,
            "water_density_kg_m3": 1025.0
        }"#,
    )?;
    assert_eq!(prop_res.status, 200);
    let prop_json = json(&prop_res)?;
    assert!(prop_json["advance_ratio_j"].as_f64().unwrap() > 0.60);
    assert!(prop_json["thrust_coefficient_kt"].as_f64().unwrap() > 0.12);
    assert!(prop_json["torque_coefficient_kq"].as_f64().unwrap() > 0.02);
    assert!(prop_json["open_water_efficiency_eta0"].as_f64().unwrap() > 0.55);
    assert!(prop_json["thrust_force_kn"].as_f64().unwrap() > 400.0);
    assert!(prop_json["delivered_power_kw"].as_f64().unwrap() > 3000.0);
    assert!(prop_json["blade_tip_speed_m_s"].as_f64().unwrap() <= 40.0);
    assert!(
        prop_json["is_keller_criterion_satisfied"]
            .as_bool()
            .unwrap()
    );
    assert!(prop_json["is_efficiency_satisfactory"].as_bool().unwrap());
    assert!(prop_json["is_tip_speed_acceptable"].as_bool().unwrap());
    assert!(prop_json["is_propeller_design_viable"].as_bool().unwrap());

    // 226. Indoor Air Quality (IAQ) & Transient Contaminant Dilution (ASHRAE 62.1 / Wells-Riley / ISO 16000)
    let iaq_res = post(
        addr,
        "/api/sim/environmental/indoor_air_quality",
        r#"{
            "contaminant": "CarbonDioxideCO2",
            "space_type": "CommercialOffice",
            "room_floor_area_m2": 120.0,
            "ceiling_height_m": 3.0,
            "occupant_count": 12,
            "outdoor_ventilation_rate_m3_h": 720.0,
            "hvac_filter_efficiency": 0.0,
            "outdoor_ambient_concentration": 420.0,
            "initial_room_concentration": 450.0,
            "target_threshold_concentration": 1000.0,
            "exposure_duration_hours": 4.0,
            "infected_occupants": 0
        }"#,
    )?;
    assert_eq!(iaq_res.status, 200);
    let iaq_json = json(&iaq_res)?;
    assert_eq!(iaq_json["room_volume_m3"].as_f64().unwrap(), 360.0);
    assert_eq!(iaq_json["air_changes_per_hour_ach"].as_f64().unwrap(), 2.0);
    assert!(iaq_json["steady_state_concentration"].as_f64().unwrap() > 650.0);
    assert!(iaq_json["ventilation_compliance_ratio"].as_f64().unwrap() >= 1.0);
    assert!(iaq_json["outdoor_flow_per_person_l_s"].as_f64().unwrap() >= 10.0);
    assert!(iaq_json["is_target_threshold_satisfied"].as_bool().unwrap());
    assert!(iaq_json["is_ashrae_62_1_compliant"].as_bool().unwrap());
    assert!(iaq_json["is_infection_risk_low"].as_bool().unwrap());
    assert!(iaq_json["is_iaq_acceptable"].as_bool().unwrap());

    // 227. Three-Phase PMSM FOC Drive Dynamics (Park-Clarke / MTPA / IEEE 1415)
    let pmsm_res = post(
        addr,
        "/api/sim/electrical/pmsm_foc_drive",
        r#"{
            "rotor_topology": "InteriorIPMSM",
            "pole_pairs_p": 4,
            "stator_resistance_ohms": 0.025,
            "d_axis_inductance_mh": 0.45,
            "q_axis_inductance_mh": 0.95,
            "permanent_magnet_flux_wb": 0.095,
            "rotational_speed_rpm": 4500.0,
            "q_axis_current_iq_a": 120.0,
            "inverter_dc_bus_voltage_v": 650.0
        }"#,
    )?;
    assert_eq!(pmsm_res.status, 200);
    let pmsm_json = json(&pmsm_res)?;
    assert!(
        pmsm_json["electrical_angular_velocity_rad_s"]
            .as_f64()
            .unwrap()
            > 1800.0
    );
    assert!(pmsm_json["d_axis_current_id_a"].as_f64().unwrap() < 0.0);
    assert!(pmsm_json["pm_alignment_torque_nm"].as_f64().unwrap() > 60.0);
    assert!(pmsm_json["reluctance_torque_nm"].as_f64().unwrap() > 5.0);
    assert!(pmsm_json["mechanical_shaft_power_kw"].as_f64().unwrap() > 30.0);
    assert!(pmsm_json["motor_efficiency_pct"].as_f64().unwrap() >= 90.0);
    assert!(pmsm_json["inverter_modulation_index"].as_f64().unwrap() <= 1.0);
    assert!(
        pmsm_json["is_voltage_within_inverter_limits"]
            .as_bool()
            .unwrap()
    );
    assert!(pmsm_json["is_efficiency_high"].as_bool().unwrap());
    assert!(pmsm_json["is_pmsm_drive_viable"].as_bool().unwrap());

    // 228. Mechanically Stabilized Earth (MSE) Wall Geogrid Stability (FHWA-NHI-10-024 / AASHTO LRFD)
    let mse_res = post(
        addr,
        "/api/sim/civil/mse_retaining_wall",
        r#"{
            "polymer_type": "HdpeUniaxial",
            "wall_height_h_m": 8.0,
            "reinforcement_length_l_m": 6.0,
            "vertical_layer_spacing_m": 0.60,
            "soil_unit_weight_kn_m3": 19.5,
            "soil_friction_angle_deg": 34.0,
            "crest_surcharge_q_kpa": 12.0,
            "geogrid_ultimate_strength_kn_m": 110.0,
            "rf_installation_damage": 1.15,
            "rf_creep_rupture": 1.65,
            "rf_durability": 1.15
        }"#,
    )?;
    assert_eq!(mse_res.status, 200);
    let mse_json = json(&mse_res)?;
    assert!(mse_json["active_earth_pressure_coef_ka"].as_f64().unwrap() > 0.25);
    assert_eq!(mse_json["total_reinforcement_layers"].as_u64().unwrap(), 13);
    assert!(
        mse_json["allowable_design_strength_tal_kn_m"]
            .as_f64()
            .unwrap()
            > 35.0
    );
    assert!(mse_json["max_layer_tensile_force_kn_m"].as_f64().unwrap() > 15.0);
    assert!(mse_json["min_rupture_safety_factor"].as_f64().unwrap() >= 1.50);
    assert!(mse_json["min_pullout_safety_factor"].as_f64().unwrap() >= 1.50);
    assert!(mse_json["length_to_height_ratio"].as_f64().unwrap() >= 0.70);
    assert!(mse_json["is_embedment_length_adequate"].as_bool().unwrap());
    assert!(mse_json["is_tensile_rupture_safe"].as_bool().unwrap());
    assert!(mse_json["is_pullout_resistance_safe"].as_bool().unwrap());
    assert!(mse_json["is_mse_wall_stable"].as_bool().unwrap());

    // 229. Gas Centrifuge Isotopic Separation & Separative Work (Cohen-Onsager / Dirac-Peierls / ISO 9001)
    let centrifuge_res = post(
        addr,
        "/api/sim/nuclear/isotope_centrifuge",
        r#"{
            "rotor_material": "CarbonFiberCfrp",
            "rotor_radius_m": 0.10,
            "rotor_length_m": 3.0,
            "rotational_speed_rpm": 65000.0,
            "operating_temperature_c": 55.0,
            "feed_mass_flow_kg_h": 0.85,
            "feed_assay_u235_fraction": 0.00711,
            "stage_cut_theta": 0.50,
            "circulation_efficiency": 0.75
        }"#,
    )?;
    assert_eq!(centrifuge_res.status, 200);
    let centrifuge_json = json(&centrifuge_res)?;
    assert!(
        centrifuge_json["rotor_peripheral_speed_m_s"]
            .as_f64()
            .unwrap()
            > 600.0
    );
    assert!(
        centrifuge_json["centrifugal_acceleration_g"]
            .as_f64()
            .unwrap()
            > 350000.0
    );
    assert!(
        centrifuge_json["rotor_burst_safety_factor"]
            .as_f64()
            .unwrap()
            >= 1.30
    );
    assert!(
        centrifuge_json["elementary_separation_factor_alpha0"]
            .as_f64()
            .unwrap()
            > 1.05
    );
    assert!(
        centrifuge_json["overall_stage_separation_factor_alpha"]
            .as_f64()
            .unwrap()
            > 1.10
    );
    assert!(
        centrifuge_json["separative_power_swu_per_year"]
            .as_f64()
            .unwrap()
            >= 10.0
    );
    assert!(centrifuge_json["is_rotor_stress_safe"].as_bool().unwrap());
    assert!(
        centrifuge_json["is_separation_effective"]
            .as_bool()
            .unwrap()
    );
    assert!(
        centrifuge_json["is_swu_capacity_commercial"]
            .as_bool()
            .unwrap()
    );
    assert!(
        centrifuge_json["is_centrifuge_stage_viable"]
            .as_bool()
            .unwrap()
    );

    // 230. Universal Commands Execution Verification (Batch 25 Domains)
    for cmd_id in [
        "sim.marine.propeller_hydrodynamics",
        "sim.environmental.indoor_air_quality",
        "sim.electrical.pmsm_foc_drive",
        "sim.civil.mse_retaining_wall",
        "sim.nuclear.isotope_centrifuge",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 231. Enterprise MCP Tool Calling (Batch 25 Domains)
    for tool_name in [
        "atlas_sim_marine_propeller_hydrodynamics",
        "atlas_sim_environmental_indoor_air_quality",
        "atlas_sim_electrical_pmsm_foc_drive",
        "atlas_sim_civil_mse_retaining_wall",
        "atlas_sim_nuclear_isotope_centrifuge",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 231, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 232. Industrial Belt Conveyor Bulk Material Handling & Drive Power (DIN 22101 / ISO 5048 / CEMA)
    let conveyor_res = post(
        addr,
        "/api/sim/mechanical/conveyor_bulk_handling",
        r#"{
            "carcass_type": "SteelCordST",
            "troughing_angle": "Trough35Deg",
            "belt_width_mm": 1200.0,
            "conveyor_length_m": 450.0,
            "elevation_lift_m": 28.0,
            "belt_speed_m_s": 3.15,
            "material_bulk_density_kg_m3": 1650.0,
            "surcharge_angle_deg": 20.0,
            "drive_wrap_angle_deg": 210.0,
            "drive_friction_mu": 0.35,
            "belt_rated_breaking_strength_kn_m": 1250.0,
            "drive_mechanical_efficiency": 0.92
        }"#,
    )?;
    assert_eq!(conveyor_res.status, 200);
    let conveyor_json = json(&conveyor_res)?;
    assert!(conveyor_json["material_cross_section_m2"].as_f64().unwrap() > 0.10);
    assert!(conveyor_json["volumetric_capacity_m3_h"].as_f64().unwrap() > 1200.0);
    assert!(conveyor_json["mass_throughput_t_h"].as_f64().unwrap() > 2000.0);
    assert!(
        conveyor_json["peripheral_drive_force_fu_kn"]
            .as_f64()
            .unwrap()
            > 80.0
    );
    assert!(conveyor_json["motor_brake_power_kw"].as_f64().unwrap() > 250.0);
    assert!(conveyor_json["slack_side_tension_t2_kn"].as_f64().unwrap() > 30.0);
    assert!(conveyor_json["tight_side_tension_t1_kn"].as_f64().unwrap() > 100.0);
    assert!(
        conveyor_json["belt_tensile_safety_factor"]
            .as_f64()
            .unwrap()
            >= 8.0
    );
    assert!(conveyor_json["is_drive_slip_safe"].as_bool().unwrap());
    assert!(
        conveyor_json["is_belt_strength_sufficient"]
            .as_bool()
            .unwrap()
    );
    assert!(
        conveyor_json["is_conveyor_design_viable"]
            .as_bool()
            .unwrap()
    );

    // 233. Offshore Monopile / Jacket Wave & Current Hydrodynamics (API RP 2A-WSD / DNV-RP-C205 / Morison)
    let wave_res = post(
        addr,
        "/api/sim/marine/offshore_wave_loading",
        r#"{
            "structure_type": "OffshoreWindMonopile",
            "water_depth_m": 40.0,
            "wave_height_m": 14.5,
            "wave_period_s": 13.0,
            "surface_current_m_s": 1.20,
            "cylinder_diameter_m": 8.50,
            "wall_thickness_mm": 85.0,
            "steel_yield_strength_mpa": 355.0,
            "drag_coefficient_cd": 0.70,
            "inertia_coefficient_cm": 2.00
        }"#,
    )?;
    assert_eq!(wave_res.status, 200);
    let wave_json = json(&wave_res)?;
    assert!(wave_json["wavelength_m"].as_f64().unwrap() > 150.0);
    assert!(wave_json["keulegan_carpenter_number"].as_f64().unwrap() > 4.0);
    assert!(wave_json["peak_base_shear_kn"].as_f64().unwrap() > 3000.0);
    assert!(wave_json["peak_overturning_moment_kn_m"].as_f64().unwrap() > 60000.0);
    assert!(wave_json["mudline_bending_stress_mpa"].as_f64().unwrap() > 15.0);
    assert!(
        wave_json["structural_bending_safety_factor"]
            .as_f64()
            .unwrap()
            >= 1.35
    );
    assert!(wave_json["is_morison_valid"].as_bool().unwrap());
    assert!(wave_json["is_bending_stress_acceptable"].as_bool().unwrap());
    assert!(wave_json["is_offshore_design_viable"].as_bool().unwrap());

    // 234. Spacecraft ADCS Three-Axis Disturbance Torques & Reaction Wheel Sizing (NASA-SP-8007 / Wertz / ECSS)
    let adcs_res = post(
        addr,
        "/api/sim/aerospace/spacecraft_adcs_attitude",
        r#"{
            "orbit_regime": "LowEarthOrbitLEO",
            "orbit_altitude_km": 550.0,
            "spacecraft_mass_kg": 350.0,
            "inertia_ixx_kg_m2": 85.0,
            "inertia_iyy_kg_m2": 120.0,
            "inertia_izz_kg_m2": 75.0,
            "cp_cg_offset_arm_m": 0.08,
            "projected_surface_area_m2": 2.40,
            "solar_reflectance_q": 0.60,
            "residual_magnetic_dipole_a_m2": 0.50,
            "target_slew_accel_deg_s2": 1.50
        }"#,
    )?;
    assert_eq!(adcs_res.status, 200);
    let adcs_json = json(&adcs_res)?;
    assert!(adcs_json["gravity_gradient_torque_un_m"].as_f64().unwrap() > 10.0);
    assert!(adcs_json["aerodynamic_drag_torque_un_m"].as_f64().unwrap() > 0.10);
    assert!(adcs_json["magnetic_residual_torque_un_m"].as_f64().unwrap() > 5.0);
    assert!(adcs_json["peak_disturbance_torque_un_m"].as_f64().unwrap() > 30.0);
    assert!(
        adcs_json["accumulated_momentum_per_orbit_nms"]
            .as_f64()
            .unwrap()
            > 0.04
    );
    assert!(
        adcs_json["recommended_wheel_momentum_nms"]
            .as_f64()
            .unwrap()
            > 0.08
    );
    assert!(adcs_json["required_slew_torque_mn_m"].as_f64().unwrap() > 500.0);
    assert!(adcs_json["is_wheel_capacity_adequate"].as_bool().unwrap());
    assert!(adcs_json["is_mtq_desaturation_adequate"].as_bool().unwrap());
    assert!(adcs_json["is_adcs_sizing_viable"].as_bool().unwrap());

    // 235. Lithium-Ion Battery Fast-Charging CC-CV Dynamics & Degradation (ISO 12405 / SAE J2464 / Newman)
    let battery_res = post(
        addr,
        "/api/sim/electrical/battery_fast_charging",
        r#"{
            "cathode_chemistry": "Nmc811HighEnergy",
            "nominal_capacity_ah": 75.0,
            "initial_soc_fraction": 0.10,
            "target_soc_fraction": 0.80,
            "charge_c_rate": 2.50,
            "internal_resistance_mohms": 0.85,
            "diffusion_time_constant_s": 95.0,
            "charge_cutoff_voltage_v": 4.20,
            "ambient_temperature_c": 25.0
        }"#,
    )?;
    assert_eq!(battery_res.status, 200);
    let battery_json = json(&battery_res)?;
    assert!(battery_json["cc_phase_current_a"].as_f64().unwrap() >= 180.0);
    assert!(battery_json["cc_phase_duration_minutes"].as_f64().unwrap() > 10.0);
    assert!(battery_json["total_charge_time_minutes"].as_f64().unwrap() < 25.0);
    assert!(battery_json["peak_heat_generation_w"].as_f64().unwrap() > 20.0);
    assert!(
        battery_json["min_anode_plating_margin_mv"]
            .as_f64()
            .unwrap()
            > 0.0
    );
    assert!(
        battery_json["estimated_cycle_life_to_80_soh"]
            .as_u64()
            .unwrap()
            > 1200
    );
    assert!(
        battery_json["is_fast_charge_time_competitive"]
            .as_bool()
            .unwrap()
    );
    assert!(
        battery_json["is_lithium_plating_prevented"]
            .as_bool()
            .unwrap()
    );
    assert!(
        battery_json["is_thermal_dissipation_manageable"]
            .as_bool()
            .unwrap()
    );
    assert!(battery_json["is_fast_charging_viable"].as_bool().unwrap());

    // 236. Industrial Packed Bed Gas Absorption & Wet Scrubber Sizing (Coulson-Richardson / Onda / EPA OAQPS)
    let scrubber_res = post(
        addr,
        "/api/sim/process/packed_bed_scrubber",
        r#"{
            "application": "AcidGasAbatement",
            "packing_type": "PallRings25mm",
            "flue_gas_flow_rate_m3_h": 8500.0,
            "inlet_pollutant_mole_fraction": 0.0120,
            "target_outlet_mole_fraction": 0.00024,
            "operating_temperature_c": 45.0,
            "operating_pressure_kpa": 101.325,
            "liquid_to_gas_ratio_multiplier": 1.50
        }"#,
    )?;
    assert_eq!(scrubber_res.status, 200);
    let scrubber_json = json(&scrubber_res)?;
    assert!(scrubber_json["column_diameter_m"].as_f64().unwrap() > 0.8);
    assert!(scrubber_json["active_bed_height_m"].as_f64().unwrap() > 3.0);
    assert!(scrubber_json["solvent_flow_rate_m3_h"].as_f64().unwrap() > 2.0);
    assert!(
        scrubber_json["number_of_transfer_units_ntu"]
            .as_f64()
            .unwrap()
            > 5.0
    );
    assert!(
        scrubber_json["total_gas_pressure_drop_pa"]
            .as_f64()
            .unwrap()
            <= 3500.0
    );
    assert!(
        scrubber_json["pollutant_removal_efficiency_pct"]
            .as_f64()
            .unwrap()
            >= 95.0
    );
    assert!(
        scrubber_json["is_removal_efficiency_met"]
            .as_bool()
            .unwrap()
    );
    assert!(
        scrubber_json["is_pressure_drop_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        scrubber_json["is_flooding_velocity_safe"]
            .as_bool()
            .unwrap()
    );
    assert!(scrubber_json["is_packed_bed_viable"].as_bool().unwrap());

    // 237. Universal Commands Execution Verification (Batch 26 Domains)
    for cmd_id in [
        "sim.mechanical.conveyor_bulk_handling",
        "sim.marine.offshore_wave_loading",
        "sim.aerospace.spacecraft_adcs_attitude",
        "sim.electrical.battery_fast_charging",
        "sim.process.packed_bed_scrubber",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 238. Enterprise MCP Tool Calling (Batch 26 Domains)
    for tool_name in [
        "atlas_sim_mechanical_conveyor_bulk_handling",
        "atlas_sim_marine_offshore_wave_loading",
        "atlas_sim_aerospace_spacecraft_adcs_attitude",
        "atlas_sim_electrical_battery_fast_charging",
        "atlas_sim_process_packed_bed_scrubber",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 238, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 239. Aircraft Landing Gear Oleo-Pneumatic Shock Strut & Dynamic Drop (FAR/CS 25.723 / Mil-L-8552 / Currey)
    let gear_res = post(
        addr,
        "/api/sim/aerospace/aircraft_landing_gear",
        r#"{
            "metering_type": "TaperedContourPin",
            "gear_landed_mass_kg": 18500.0,
            "vertical_sink_velocity_m_s": 3.05,
            "lift_to_weight_ratio": 0.85,
            "available_stroke_mm": 420.0,
            "piston_diameter_mm": 180.0,
            "gas_precharge_pressure_mpa": 4.20,
            "initial_gas_volume_liters": 14.50,
            "polytropic_exponent": 1.35,
            "orifice_diameter_mm": 34.0,
            "orifice_discharge_coeff": 0.68,
            "fluid_density_kg_m3": 885.0
        }"#,
    )?;
    assert_eq!(gear_res.status, 200);
    let gear_json = json(&gear_res)?;
    assert!(gear_json["max_strut_stroke_mm"].as_f64().unwrap() > 250.0);
    assert!(gear_json["max_strut_stroke_mm"].as_f64().unwrap() < 400.0);
    assert!(gear_json["peak_ground_reaction_force_kn"].as_f64().unwrap() > 200.0);
    assert!(gear_json["vertical_load_factor_g"].as_f64().unwrap() <= 2.50);
    assert!(gear_json["shock_strut_efficiency_pct"].as_f64().unwrap() >= 70.0);
    assert!(gear_json["is_stroke_margin_sufficient"].as_bool().unwrap());
    assert!(gear_json["is_load_factor_compliant"].as_bool().unwrap());
    assert!(
        gear_json["is_tire_deflection_acceptable"]
            .as_bool()
            .unwrap()
    );
    assert!(gear_json["is_landing_gear_viable"].as_bool().unwrap());

    // 240. Pumped Storage Hydroelectric Plant Reversible Pump-Turbine & Water Conduit (IEC 60041 / IEC 61362 / IEEE 1207)
    let hydro_res = post(
        addr,
        "/api/sim/energy/pumped_storage_hydro",
        r#"{
            "runner_type": "FrancisReversible",
            "upper_reservoir_elevation_m": 1850.0,
            "lower_reservoir_elevation_m": 1540.0,
            "conduit_length_m": 1650.0,
            "conduit_diameter_m": 3.80,
            "darcy_friction_factor": 0.014,
            "minor_loss_k_total": 1.85,
            "pumping_discharge_m3_s": 32.0,
            "generating_discharge_m3_s": 36.5,
            "rotational_speed_rpm": 428.6,
            "runner_submergence_depth_m": 35.0
        }"#,
    )?;
    assert_eq!(hydro_res.status, 200);
    let hydro_json = json(&hydro_res)?;
    assert!(hydro_json["total_dynamic_pumping_head_m"].as_f64().unwrap() > 311.0);
    assert!(hydro_json["pumping_electrical_power_mw"].as_f64().unwrap() > 90.0);
    assert!(
        hydro_json["generating_electrical_power_mw"]
            .as_f64()
            .unwrap()
            > 80.0
    );
    assert!(hydro_json["round_trip_efficiency_pct"].as_f64().unwrap() >= 75.0);
    assert!(hydro_json["cavitation_safety_factor"].as_f64().unwrap() >= 1.15);
    assert!(hydro_json["is_cavitation_safe"].as_bool().unwrap());
    assert!(
        hydro_json["is_round_trip_efficiency_competitive"]
            .as_bool()
            .unwrap()
    );
    assert!(hydro_json["is_pumped_storage_viable"].as_bool().unwrap());

    // 241. Semiconductor EUV Photolithography Multi-Layer Optics & Resolution (ASML / SEMI / Fresnel-Debye)
    let euv_res = post(
        addr,
        "/api/sim/semiconductor/euv_lithography",
        r#"{
            "tool_generation": "StandardNa033",
            "wavelength_nm": 13.50,
            "numerical_aperture": 0.33,
            "process_factor_k1": 0.32,
            "dof_factor_k2": 0.60,
            "bilayer_period_d_nm": 6.90,
            "molybdenum_fraction_gamma": 0.40,
            "bilayer_pair_count": 50,
            "interface_roughness_nm": 0.15,
            "incidence_angle_deg": 6.0,
            "mirror_count_in_train": 11,
            "resist_exposure_dose_mj_cm2": 45.0
        }"#,
    )?;
    assert_eq!(euv_res.status, 200);
    let euv_json = json(&euv_res)?;
    assert!(euv_json["net_mirror_reflectivity_pct"].as_f64().unwrap() >= 66.0);
    assert!(
        euv_json["critical_dimension_resolution_nm"]
            .as_f64()
            .unwrap()
            <= 15.0
    );
    assert!(euv_json["depth_of_focus_nm"].as_f64().unwrap() >= 60.0);
    assert!(euv_json["line_edge_roughness_3sigma_nm"].as_f64().unwrap() <= 3.0);
    assert!(
        euv_json["is_mirror_reflectivity_sufficient"]
            .as_bool()
            .unwrap()
    );
    assert!(
        euv_json["is_resolution_advanced_node_capable"]
            .as_bool()
            .unwrap()
    );
    assert!(
        euv_json["is_depth_of_focus_manufacturable"]
            .as_bool()
            .unwrap()
    );
    assert!(euv_json["is_euv_system_viable"].as_bool().unwrap());

    // 242. Geothermal Binary Organic Rankine Cycle (ORC) Power Plant (ASME PTC 46 / VDI 4670 / DiPippo)
    let orc_res = post(
        addr,
        "/api/sim/renewable/geothermal_binary_orc",
        r#"{
            "working_fluid": "Isopentane",
            "brine_mass_flow_kg_s": 85.0,
            "brine_inlet_temperature_c": 165.0,
            "min_reinjection_temperature_c": 72.0,
            "pinch_point_temp_diff_c": 6.0,
            "evaporator_pressure_bar": 18.5,
            "condensing_temperature_c": 36.0,
            "expander_isentropic_efficiency": 0.86,
            "pump_isentropic_efficiency": 0.78,
            "generator_efficiency": 0.97
        }"#,
    )?;
    assert_eq!(orc_res.status, 200);
    let orc_json = json(&orc_res)?;
    assert!(orc_json["thermal_duty_extracted_mw"].as_f64().unwrap() > 25.0);
    assert!(orc_json["net_electrical_power_mw"].as_f64().unwrap() > 3.0);
    assert!(
        orc_json["exergy_second_law_efficiency_pct"]
            .as_f64()
            .unwrap()
            >= 35.0
    );
    assert!(orc_json["is_silica_scaling_prevented"].as_bool().unwrap());
    assert!(
        orc_json["is_exergy_efficiency_competitive"]
            .as_bool()
            .unwrap()
    );
    assert!(orc_json["is_geothermal_orc_viable"].as_bool().unwrap());

    // 243. Cable-Stayed Bridge Stay Cable Rain-Wind Induced Vibration & Damping (FHWA-HRT-05-072 / PTI / Eurocode 1)
    let stay_res = post(
        addr,
        "/api/sim/structural/cable_stayed_bridge",
        r#"{
            "stay_class": "ParallelStrandSystemPSS",
            "cable_chord_length_m": 240.0,
            "outer_pipe_diameter_mm": 160.0,
            "chord_inclination_deg": 32.0,
            "cable_pretension_kn": 3600.0,
            "total_linear_mass_kg_m": 52.0,
            "elastic_modulus_gpa": 195.0,
            "net_steel_area_mm2": 6400.0,
            "inherent_damping_ratio": 0.0012,
            "damper_distance_from_deck_m": 5.50,
            "wind_speed_m_s": 14.5
        }"#,
    )?;
    assert_eq!(stay_res.status, 200);
    let stay_json = json(&stay_res)?;
    assert!(stay_json["midspan_static_sag_mm"].as_f64().unwrap() > 500.0);
    assert!(stay_json["fundamental_frequency_hz"].as_f64().unwrap() > 0.4);
    assert!(
        stay_json["optimal_damper_coefficient_kn_s_m"]
            .as_f64()
            .unwrap()
            > 30.0
    );
    assert!(stay_json["attained_damping_ratio"].as_f64().unwrap() >= 0.0060);
    assert!(stay_json["is_cable_sag_acceptable"].as_bool().unwrap());
    assert!(stay_json["is_rwiv_mitigated"].as_bool().unwrap());
    assert!(stay_json["is_stay_cable_viable"].as_bool().unwrap());

    // 244. Universal Commands Execution Verification (Batch 27 Domains)
    for cmd_id in [
        "sim.aerospace.aircraft_landing_gear",
        "sim.energy.pumped_storage_hydro",
        "sim.semiconductor.euv_lithography",
        "sim.renewable.geothermal_binary_orc",
        "sim.structural.cable_stayed_bridge",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 245. Enterprise MCP Tool Calling (Batch 27 Domains)
    for tool_name in [
        "atlas_sim_aerospace_aircraft_landing_gear",
        "atlas_sim_energy_pumped_storage_hydro",
        "atlas_sim_semiconductor_euv_lithography",
        "atlas_sim_renewable_geothermal_binary_orc",
        "atlas_sim_structural_cable_stayed_bridge",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 245, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 246. Supersonic Wind Tunnel Convergent-Divergent de Laval Nozzle Aerodynamics (NASA SP-3012 / Anderson / ISO 5167)
    let nozzle_res = post(
        addr,
        "/api/sim/aerospace/supersonic_nozzle",
        r#"{
            "design_mach_number": 2.20,
            "stagnation_pressure_kpa": 450.0,
            "stagnation_temperature_k": 320.0,
            "throat_diameter_mm": 85.0,
            "ambient_backpressure_kpa": 42.0,
            "model_deflection_angle_deg": 10.0
        }"#,
    )?;
    assert_eq!(nozzle_res.status, 200);
    let nozzle_json = json(&nozzle_res)?;
    assert!(nozzle_json["choked_mass_flow_rate_kg_s"].as_f64().unwrap() > 5.0);
    assert!(nozzle_json["exit_flow_velocity_m_s"].as_f64().unwrap() > 500.0);
    assert!(
        nozzle_json["normal_shock_downstream_mach"]
            .as_f64()
            .unwrap()
            < 1.0
    );
    assert!(
        nozzle_json["oblique_shock_wave_angle_deg"]
            .as_f64()
            .unwrap()
            > 30.0
    );
    assert!(nozzle_json["is_choked_at_throat"].as_bool().unwrap());
    assert!(
        nozzle_json["is_supersonic_flow_established"]
            .as_bool()
            .unwrap()
    );
    assert!(
        nozzle_json["is_nozzle_aerodynamically_viable"]
            .as_bool()
            .unwrap()
    );

    // 247. Electric Vehicle (EV) Dual-Motor AWD Torque Vectoring & Yaw Stability (ISO 8855 / Pacejka Magic Formula)
    let tv_res = post(
        addr,
        "/api/sim/automotive/ev_torque_vectoring",
        r#"{
            "drivetrain_layout": "DualMotorAwd",
            "vehicle_mass_kg": 2150.0,
            "wheelbase_mm": 2900.0,
            "track_width_mm": 1620.0,
            "cg_to_front_axle_mm": 1420.0,
            "yaw_moment_of_inertia_kg_m2": 2950.0,
            "front_cornering_stiffness_n_rad": 95000.0,
            "rear_cornering_stiffness_n_rad": 105000.0,
            "vehicle_speed_km_h": 90.0,
            "front_steering_angle_deg": 1.2,
            "demanded_total_torque_nm": 450.0
        }"#,
    )?;
    assert_eq!(tv_res.status, 200);
    let tv_json = json(&tv_res)?;
    assert!(tv_json["understeer_gradient_deg_per_g"].as_f64().unwrap() > 0.0);
    assert!(tv_json["target_yaw_rate_deg_s"].as_f64().unwrap() > 5.0);
    // DYC demand is signed: a negative value corrects the measured oversteer.
    assert!(
        tv_json["active_yaw_moment_demand_nm"]
            .as_f64()
            .unwrap()
            .abs()
            > 20.0
    );
    assert!(tv_json["is_yaw_rate_tracking_satisfied"].as_bool().unwrap());
    assert!(tv_json["is_sideslip_stable"].as_bool().unwrap());
    assert!(tv_json["is_torque_vectoring_viable"].as_bool().unwrap());

    // 248. Ultra-Deepwater Subsea Marine Riser Vortex-Induced Vibration & Fatigue (DNV-RP-F204 / API 16Q)
    let riser_res = post(
        addr,
        "/api/sim/marine/subsea_riser_viv",
        r#"{
            "suppression_device": "HelicalStrakesTripleStart",
            "riser_length_m": 1850.0,
            "water_depth_m": 1800.0,
            "outer_diameter_mm": 533.4,
            "wall_thickness_mm": 25.4,
            "top_tension_kn": 6500.0,
            "internal_fluid_density_kg_m3": 1250.0,
            "seawater_density_kg_m3": 1025.0,
            "surface_current_velocity_m_s": 1.65,
            "bottom_current_velocity_m_s": 0.25,
            "steel_elastic_modulus_gpa": 207.0
        }"#,
    )?;
    assert_eq!(riser_res.status, 200);
    let riser_json = json(&riser_res)?;
    assert!(riser_json["fundamental_frequency_hz"].as_f64().unwrap() > 0.01);
    assert!(riser_json["dominant_lockin_mode"].as_u64().unwrap() >= 20);
    assert!(riser_json["predicted_fatigue_life_years"].as_f64().unwrap() >= 25.0);
    assert!(riser_json["is_viv_amplitude_acceptable"].as_bool().unwrap());
    assert!(riser_json["is_fatigue_life_compliant"].as_bool().unwrap());
    assert!(riser_json["is_subsea_riser_viable"].as_bool().unwrap());

    // 249. Biopharmaceutical Protein Downstream Purification Chromatography Column Dynamics (FDA cGMP / ASME BPE)
    let chrom_res = post(
        addr,
        "/api/sim/chemical/chromatography_column",
        r#"{
            "column_diameter_mm": 300.0,
            "bed_height_mm": 200.0,
            "linear_velocity_cm_h": 300.0,
            "feed_protein_concentration_g_l": 3.50,
            "max_binding_capacity_g_l": 55.0,
            "langmuir_constant_l_g": 8.33,
            "resin_particle_diameter_um": 50.0,
            "bed_void_fraction": 0.36,
            "intraparticle_porosity": 0.65,
            "buffer_viscosity_mpa_s": 1.05
        }"#,
    )?;
    assert_eq!(chrom_res.status, 200);
    let chrom_json = json(&chrom_res)?;
    assert!(chrom_json["residence_time_minutes"].as_f64().unwrap() > 3.0);
    assert!(chrom_json["hetp_mm"].as_f64().unwrap() < 1.0);
    assert!(chrom_json["number_of_theoretical_plates"].as_f64().unwrap() > 400.0);
    assert!(
        chrom_json["dynamic_binding_capacity_10pct_g_l"]
            .as_f64()
            .unwrap()
            > 40.0
    );
    assert!(chrom_json["is_pressure_drop_acceptable"].as_bool().unwrap());
    assert!(
        chrom_json["is_dbc_utilization_competitive"]
            .as_bool()
            .unwrap()
    );
    assert!(
        chrom_json["is_chromatography_column_viable"]
            .as_bool()
            .unwrap()
    );

    // 250. High-Speed Maglev Electromagnetic Suspension Active Airgap Control (Transrapid / IEEE 1475)
    let maglev_res = post(
        addr,
        "/api/sim/transport/maglev_suspension",
        r#"{
            "mass_supported_per_magnet_kg": 1250.0,
            "nominal_airgap_mm": 12.0,
            "pole_face_area_mm2": 58000.0,
            "coil_turns": 320,
            "coil_resistance_ohms": 0.38,
            "disturbance_amplitude_mm": 2.50,
            "target_controller_bandwidth_rad_s": 65.0
        }"#,
    )?;
    assert_eq!(maglev_res.status, 200);
    let maglev_json = json(&maglev_res)?;
    assert!(maglev_json["equilibrium_current_a"].as_f64().unwrap() > 25.0);
    assert!(maglev_json["equilibrium_flux_density_t"].as_f64().unwrap() > 0.40);
    assert!(
        maglev_json["open_loop_unstable_pole_rad_s"]
            .as_f64()
            .unwrap()
            > 30.0
    );
    assert!(
        maglev_json["is_core_saturation_prevented"]
            .as_bool()
            .unwrap()
    );
    assert!(
        maglev_json["is_airgap_contact_prevented"]
            .as_bool()
            .unwrap()
    );
    assert!(
        maglev_json["is_maglev_suspension_viable"]
            .as_bool()
            .unwrap()
    );

    // 251. Universal Commands Execution Verification (Batch 28 Domains)
    for cmd_id in [
        "sim.aerospace.supersonic_nozzle",
        "sim.automotive.ev_torque_vectoring",
        "sim.marine.subsea_riser_viv",
        "sim.chemical.chromatography_column",
        "sim.transport.maglev_suspension",
    ] {
        let cmd_res = post(
            addr,
            "/api/commands/execute",
            &format!(r#"{{"command_id": "{cmd_id}", "arguments": {{}}}}"#),
        )?;
        assert_eq!(cmd_res.status, 200);
        assert_eq!(json(&cmd_res)?["status"], "Success");
    }

    // 252. Enterprise MCP Tool Calling (Batch 28 Domains)
    for tool_name in [
        "atlas_sim_aerospace_supersonic_nozzle",
        "atlas_sim_automotive_ev_torque_vectoring",
        "atlas_sim_marine_subsea_riser_viv",
        "atlas_sim_chemical_chromatography_column",
        "atlas_sim_transport_maglev_suspension",
    ] {
        let mcp_res = post(
            addr,
            "/api/mcp",
            &format!(
                r#"{{"jsonrpc": "2.0", "id": 252, "method": "tools/call", "params": {{"name": "{tool_name}", "arguments": {{}}}}}}"#
            ),
        )?;
        assert_eq!(mcp_res.status, 200);
    }

    // 253. Supersonic Aircraft Flight Simulation Video & Poster Binary Endpoints
    let poster_res = get(addr, "/flight-demo-poster.png")?;
    assert_eq!(poster_res.status, 200);
    assert_eq!(
        poster_res.headers.get("content-type").map(String::as_str),
        Some("image/png")
    );
    assert!(!poster_res.body.is_empty());

    let video_res = get(addr, "/atlascad-supersonic-flight.mp4")?;
    assert_eq!(video_res.status, 200);
    assert_eq!(
        video_res.headers.get("content-type").map(String::as_str),
        Some("video/mp4")
    );
    assert_eq!(
        video_res.headers.get("accept-ranges").map(String::as_str),
        Some("bytes")
    );
    assert!(video_res.body.len() > 1_000_000);

    // 254. Multi-Tier Universal Asset Store & Component Hub Endpoints
    let assets_res = get(addr, "/api/assets?visibility=public")?;
    assert_eq!(assets_res.status, 200);
    let assets_list: Vec<Value> = serde_json::from_slice(&assets_res.body)?;
    assert!(!assets_list.is_empty());
    assert!(assets_list.iter().any(|a| a["id"] == "ast_aero_waverider"));

    let aero_filter_res = get(addr, "/api/assets?category=Aerospace")?;
    assert_eq!(aero_filter_res.status, 200);
    let aero_list: Vec<Value> = serde_json::from_slice(&aero_filter_res.body)?;
    assert!(aero_list.iter().any(|a| a["id"] == "ast_aero_waverider"));

    let asset_detail_res = get(addr, "/api/assets/ast_aero_waverider")?;
    assert_eq!(asset_detail_res.status, 200);
    let detail_val = json(&asset_detail_res)?;
    assert_eq!(
        detail_val["record"]["name"],
        "Aerospace Supersonic Waverider"
    );
    assert!(
        !detail_val["record"]["parameters"]
            .as_array()
            .unwrap()
            .is_empty()
    );

    // Publish New Custom Part Asset
    let pub_res = post(
        addr,
        "/api/assets",
        r#"{
            "name": "Composite Flap Hinge Fitting",
            "description": "Additive manufactured Ti-6Al-4V hinge bracket for supersonic elevon controls.",
            "category": "Aerospace",
            "visibility": "team",
            "tags": ["hinge", "titanium", "additive", "elevon"],
            "author_id": "usr_sanjay",
            "author_name": "Sanjay (Chief Architect)",
            "team_id": "team_aero_structures"
        }"#,
    )?;
    assert_eq!(pub_res.status, 200);
    let published_asset = json(&pub_res)?;
    let published_id = published_asset["id"].as_str().unwrap();

    // Fork Asset into Private Workspace
    let fork_res = post(
        addr,
        &format!("/api/assets/{published_id}/fork"),
        r#"{
            "user_id": "usr_sanjay",
            "user_name": "Sanjay (Chief Architect)",
            "team_id": "team_aero_structures",
            "visibility": "private"
        }"#,
    )?;
    assert_eq!(fork_res.status, 200);
    let forked_val = json(&fork_res)?;
    assert_eq!(forked_val["author"]["user_id"], "usr_sanjay");
    assert_eq!(forked_val["visibility"], "Private");

    // Instantiate / Use Asset in Active Assembly
    let use_res = post(
        addr,
        &format!("/api/assets/{published_id}/use"),
        r#"{ "instance_name": "wing_left_hinge_inst", "parameters": { "pin_diameter": 12.0 } }"#,
    )?;
    assert_eq!(use_res.status, 200);
    let use_val = json(&use_res)?;
    assert_eq!(use_val["status"], "Instantiated");
    assert_eq!(use_val["linked_dependency"], true);

    // 255. Real-Time Multi-User Collaboration, Presence & 3D Spatial Comments
    let presence_res = get(addr, "/api/collab/presence")?;
    assert_eq!(presence_res.status, 200);
    let presence_val = json(&presence_res)?;
    assert!(presence_val["total_active"].as_u64().unwrap() >= 3);

    let update_pres_res = post(
        addr,
        "/api/collab/presence",
        r#"{
            "user_id": "usr_sanjay",
            "name": "Sanjay (Chief Architect)",
            "role": "Owner",
            "cursor_pos": [1.5, 4.0, 1.2],
            "active_tool": "3D Chamfer",
            "selected_entity": "hinge_fitting_edge_04"
        }"#,
    )?;
    assert_eq!(update_pres_res.status, 200);

    // Concurrency: Entity Lease / Optimistic Lock Acquisition and Release
    let lock_res = post(
        addr,
        "/api/collab/lock",
        r#"{ "entity_id": "body_fuselage_01", "user_id": "usr_sanjay", "duration_sec": 30 }"#,
    )?;
    assert_eq!(lock_res.status, 200);
    assert_eq!(json(&lock_res)?["acquired"], true);

    let unlock_res = post(
        addr,
        "/api/collab/unlock",
        r#"{ "entity_id": "body_fuselage_01", "user_id": "usr_sanjay" }"#,
    )?;
    assert_eq!(unlock_res.status, 200);
    assert_eq!(json(&unlock_res)?["released"], true);

    // 3D Spatial Annotations & Design Review Comments
    let comments_res = get(addr, "/api/collab/comments")?;
    assert_eq!(comments_res.status, 200);
    let comments_val = json(&comments_res)?;
    assert!(!comments_val["comments"].as_array().unwrap().is_empty());

    let create_cmt_res = post(
        addr,
        "/api/collab/comments",
        r#"{
            "pos_3d": [0.0, 4.0, 1.2],
            "text": "Review fastener clearance and shear stress under Mach 3.5 shockwave impingement.",
            "author_id": "usr_sarah",
            "author_name": "Sarah Chen (Structural Lead)"
        }"#,
    )?;
    assert_eq!(create_cmt_res.status, 200);
    let cmt_val = json(&create_cmt_res)?;
    let cmt_id = cmt_val["id"].as_str().unwrap();

    let reply_res = post(
        addr,
        &format!("/api/collab/comments/{cmt_id}/reply"),
        r#"{
            "text": "Verified with ANSI/AIAA S-080 high-temp joint fastener standards.",
            "author_id": "usr_sanjay",
            "author_name": "Sanjay (Chief Architect)"
        }"#,
    )?;
    assert_eq!(reply_res.status, 200);

    let resolve_res = post(
        addr,
        &format!("/api/collab/comments/{cmt_id}/resolve"),
        "{}",
    )?;
    assert_eq!(resolve_res.status, 200);
    assert_eq!(json(&resolve_res)?["status"], "Resolved");

    // 256. Production-Ready Enterprise Identity, RBAC, Multi-Tenant SSO & API Keys
    let auth_me_res = get(addr, "/api/auth/me")?;
    assert_eq!(auth_me_res.status, 200);
    let auth_val = json(&auth_me_res)?;
    assert_eq!(auth_val["authenticated"], true);
    assert_eq!(auth_val["user"]["user_id"], "usr_sanjay");
    assert_eq!(auth_val["enterprise_license"]["seats_allocated"], 4);
    assert_eq!(auth_val["enterprise_license"]["seats_total"], 10);
    assert_eq!(auth_val["teams"].as_array().unwrap().len(), 5);

    // Switch active role/user to Dr. Elena Rostova
    let switch_user_res = post(addr, "/api/auth/login", r#"{ "user_id": "usr_elena" }"#)?;
    assert_eq!(switch_user_res.status, 200);
    let elena_val = json(&switch_user_res)?;
    assert_eq!(elena_val["success"], true);
    assert_eq!(elena_val["user"]["user_id"], "usr_elena");
    // The identity switch must not be mistaken for authentication: it verifies no credential and
    // says so in the payload the UI renders.
    assert_eq!(elena_val["credential_verified"], false);
    assert_eq!(elena_val["authentication"], "demo-identity");
    assert!(
        elena_val["demo_notice"]
            .as_str()
            .unwrap_or_default()
            .contains("no credential")
    );

    // SSO has no adapter in this build, so the endpoint must refuse rather than invent a session.
    let sso_res = post(
        addr,
        "/api/auth/login",
        r#"{ "sso_provider": "Okta / SAML 2.0" }"#,
    )?;
    assert_eq!(sso_res.status, 501);
    assert_eq!(json(&sso_res)?["error"]["kind"], "unavailable");

    // Switch active team
    let switch_team_res = post(
        addr,
        "/api/auth/switch-team",
        r#"{ "team_id": "team_propulsion" }"#,
    )?;
    assert_eq!(switch_team_res.status, 200);
    assert_eq!(json(&switch_team_res)?["active_team_id"], "team_propulsion");

    // Generate Headless CI/CD Personal Access Token (PAT)
    let create_key_res = post(
        addr,
        "/api/auth/api-keys",
        r#"{ "name": "Automated Hypersonic Meshing Pipeline", "scopes": ["cad:read", "cad:mutate", "sim:hypersonic"] }"#,
    )?;
    assert_eq!(create_key_res.status, 200);
    let key_val = json(&create_key_res)?;
    let key_id = key_val["key"]["id"].as_str().unwrap();
    assert!(key_val["token"].as_str().unwrap().starts_with("atlas_pat_"));

    // Verify key appears in list
    let list_keys_res = get(addr, "/api/auth/api-keys")?;
    assert_eq!(list_keys_res.status, 200);
    assert!(
        json(&list_keys_res)?["api_keys"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["id"] == key_id)
    );

    // Revoke key
    let revoke_res = post(
        addr,
        "/api/auth/api-keys/revoke",
        &format!(r#"{{ "id": "{key_id}" }}"#),
    )?;
    assert_eq!(revoke_res.status, 200);

    // Register a new enterprise team member
    let reg_res = post(
        addr,
        "/api/auth/register",
        r#"{ "name": "Dr. Hans Weber", "email": "hans.weber@atlascad.internal", "role": "Aerodynamics Lead", "team_id": "team_aero_structures" }"#,
    )?;
    assert_eq!(reg_res.status, 200);
    let reg_val = json(&reg_res)?;
    assert_eq!(reg_val["success"], true);
    assert_eq!(reg_val["user"]["name"], "Dr. Hans Weber");
    assert_eq!(reg_val["credential_verified"], false);
    assert_eq!(reg_val["authentication"], "demo-identity");

    // Logout to guest
    let logout_res = post(addr, "/api/auth/logout", "{}")?;
    assert_eq!(logout_res.status, 200);
    let auth_me_logged_out = get(addr, "/api/auth/me")?;
    assert_eq!(json(&auth_me_logged_out)?["authenticated"], false);

    Ok(())
}

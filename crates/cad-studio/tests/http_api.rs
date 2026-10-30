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

    // A Host naming another port is not this studio, shared or not.
    let wrong_port = request_with_host(
        addr,
        "127.0.0.1:9999",
        "GET",
        "/api/state",
        &[("x-atlas-token", &token)],
        &[],
    )?;
    assert_eq!(wrong_port.status, 403);
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

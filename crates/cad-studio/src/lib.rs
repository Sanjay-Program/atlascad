//! Atlas CAD Studio — a local-first application shell over the platform's project and kernel.
//!
//! The studio is one process: it owns the [`Project`], evaluates it through the Truck adapter, and
//! serves a single browser tab on loopback. There is no cloud dependency, no account, and no
//! background service. Closing the process is the whole shutdown procedure.
//!
//! # Trust boundary
//!
//! * the listener binds `127.0.0.1` only, never a public interface;
//! * request headers are capped at 16 KiB and JSON bodies at 1 MiB;
//! * every `POST` must carry a same-origin `Origin` and a JSON content type, so another site cannot
//!   drive the model through the user's browser;
//! * no endpoint accepts a filesystem path from the client — the project path is fixed at launch;
//! * the only file written is the project itself and the only export is STL returned in the response.
//!
//! This is a desktop-grade local trust model, not a multi-tenant web service. Do not expose the port
//! through a tunnel and call it hosted.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod http;

use cad_contract::capability::KernelInfo;
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::kernel::Kernel;
use cad_document::{Evaluated, Project, ProjectError, SketchProfile};
use cad_kernel_truck::TruckKernel;
use serde::{Deserialize, Serialize};
use std::net::{SocketAddr, TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};

/// The default loopback port.
pub const DEFAULT_PORT: u16 = 7519;
/// Most simultaneous connections. A browser tab opens a handful; the rest are refused rather than
/// queued without bound.
pub const MAX_CONNECTIONS: usize = 64;

const INDEX_HTML: &str = include_str!("../assets/index.html");
const APP_CSS: &str = include_str!("../assets/app.css");
const APP_JS: &str = include_str!("../assets/app.js");
const FAVICON: &str = include_str!("../assets/favicon.svg");

/// Everything the studio can fail with.
#[derive(Debug)]
pub enum StudioError {
    Project(ProjectError),
    Kernel(cad_contract::error::KernelError),
    Stl(cad_io::StlError),
    Json(String),
    Io(String),
    BadRequest(String),
    Forbidden,
    NotFound,
    MethodNotAllowed,
    UnsupportedMediaType,
    PayloadTooLarge,
    Lock,
    Bind(std::io::Error),
}

impl std::fmt::Display for StudioError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            StudioError::Project(error) => write!(f, "project: {error}"),
            StudioError::Kernel(error) => write!(f, "kernel: {error}"),
            StudioError::Stl(error) => write!(f, "STL: {error}"),
            StudioError::Json(message) => write!(f, "JSON: {message}"),
            StudioError::Io(message) => write!(f, "I/O: {message}"),
            StudioError::BadRequest(message) => write!(f, "{message}"),
            StudioError::Forbidden => f.write_str("request is not from this studio's origin"),
            StudioError::NotFound => f.write_str("not found"),
            StudioError::MethodNotAllowed => f.write_str("method not allowed"),
            StudioError::UnsupportedMediaType => f.write_str("expected application/json"),
            StudioError::PayloadTooLarge => f.write_str("request body is too large"),
            StudioError::Lock => {
                f.write_str("studio state is unavailable after an internal failure")
            }
            StudioError::Bind(error) => write!(f, "cannot bind the studio port: {error}"),
        }
    }
}

impl std::error::Error for StudioError {}

impl From<ProjectError> for StudioError {
    fn from(error: ProjectError) -> Self {
        StudioError::Project(error)
    }
}

impl From<cad_contract::error::KernelError> for StudioError {
    fn from(error: cad_contract::error::KernelError) -> Self {
        StudioError::Kernel(error)
    }
}

impl From<cad_io::StlError> for StudioError {
    fn from(error: cad_io::StlError) -> Self {
        StudioError::Stl(error)
    }
}

impl From<serde_json::Error> for StudioError {
    fn from(error: serde_json::Error) -> Self {
        StudioError::Json(error.to_string())
    }
}

impl StudioError {
    fn status(&self) -> u16 {
        match self {
            StudioError::BadRequest(_) | StudioError::Json(_) => 400,
            StudioError::Forbidden => 403,
            StudioError::NotFound => 404,
            StudioError::MethodNotAllowed => 405,
            StudioError::PayloadTooLarge => 413,
            StudioError::UnsupportedMediaType => 415,
            StudioError::Kernel(_) => 422,
            StudioError::Project(error) => match error {
                ProjectError::Invalid(_)
                | ProjectError::UnknownParameter(_)
                | ProjectError::UnknownSketch(_)
                | ProjectError::UnsupportedSchema { .. }
                | ProjectError::Document(_) => 400,
                ProjectError::NothingToUndo | ProjectError::NothingToRedo => 409,
                ProjectError::Kernel(_) => 422,
                ProjectError::Io { .. } => 500,
            },
            StudioError::Stl(_) | StudioError::Io(_) | StudioError::Lock | StudioError::Bind(_) => {
                500
            }
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            StudioError::BadRequest(_) => "bad_request",
            StudioError::Json(_) => "invalid_json",
            StudioError::Forbidden => "forbidden_origin",
            StudioError::NotFound => "not_found",
            StudioError::MethodNotAllowed => "method_not_allowed",
            StudioError::UnsupportedMediaType => "unsupported_media_type",
            StudioError::PayloadTooLarge => "payload_too_large",
            StudioError::Project(ProjectError::NothingToUndo) => "nothing_to_undo",
            StudioError::Project(ProjectError::NothingToRedo) => "nothing_to_redo",
            StudioError::Project(ProjectError::UnknownParameter(_)) => "unknown_parameter",
            StudioError::Project(ProjectError::UnknownSketch(_)) => "unknown_sketch",
            StudioError::Project(_) => "invalid_project",
            StudioError::Kernel(_) => "kernel_failure",
            StudioError::Stl(_) => "export_failure",
            StudioError::Io(_) | StudioError::Lock | StudioError::Bind(_) => "internal_error",
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Payloads
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Serialize)]
struct ParameterView {
    id: String,
    value: f64,
}

#[derive(Debug, Serialize)]
struct SketchView {
    id: u64,
    origin: [f64; 2],
    profile: SketchProfile,
    segments: usize,
}

#[derive(Debug, Serialize)]
struct FeatureView {
    id: u64,
    sketch_id: u64,
    depth_param: String,
    depth_mm: f64,
}

#[derive(Debug, Serialize)]
struct MeshView {
    positions: Vec<[f64; 3]>,
    triangles: Vec<[u32; 3]>,
}

#[derive(Debug, Serialize)]
struct BoundsView {
    min: [f64; 3],
    max: [f64; 3],
}

#[derive(Debug, Serialize)]
struct GeometryView {
    volume_mm3: f64,
    surface_area_mm2: f64,
    triangles: usize,
    vertices: usize,
}

#[derive(Debug, Serialize)]
struct KernelView {
    name: String,
    version: String,
    capabilities: Vec<&'static str>,
}

#[derive(Debug, Serialize)]
struct HistoryView {
    undo: usize,
    redo: usize,
}

#[derive(Debug, Serialize)]
struct StatePayload {
    schema: u32,
    revision: u64,
    name: String,
    file: String,
    units: String,
    unit: String,
    dirty: bool,
    parameters: Vec<ParameterView>,
    sketches: Vec<SketchView>,
    features: Vec<FeatureView>,
    mesh: MeshView,
    outline: Option<Vec<[f64; 2]>>,
    bounds: Option<BoundsView>,
    geometry: GeometryView,
    kernel: KernelView,
    history: HistoryView,
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

struct StudioState {
    project: Project,
    path: PathBuf,
    /// Revision at the last successful save or load. `dirty` is a conservative comparison.
    saved_revision: u64,
}

impl StudioState {
    fn new(project: Project, path: PathBuf) -> Result<Self, StudioError> {
        project.validate()?;
        let state = StudioState {
            saved_revision: project.revision.get(),
            project,
            path,
        };
        // Evaluate once at startup so a broken project is refused before a URL is printed.
        let _ = state.payload()?;
        Ok(state)
    }

    fn snapshot(&self) -> Result<(Option<cad_contract::mesh::Mesh>, KernelInfo), StudioError> {
        let mut kernel = TruckKernel::new();
        let info = kernel.info();
        if self.project.features().is_empty() {
            return Ok((None, info));
        }
        let Evaluated {
            mesh,
            solid,
            volume_mm3: _,
            triangles: _,
        } = self.project.evaluate(&mut kernel)?;
        kernel.release(solid);
        Ok((Some(mesh), info))
    }

    fn payload(&self) -> Result<StatePayload, StudioError> {
        let (mesh, info) = self.snapshot()?;
        let unit = self.project.header.units;

        let parameters = self
            .project
            .parameters()
            .iter()
            .map(|parameter| ParameterView {
                id: parameter.id.clone(),
                value: parameter.value,
            })
            .collect();

        let mut sketches = Vec::with_capacity(self.project.sketches().len());
        for sketch in self.project.sketches() {
            sketches.push(SketchView {
                id: sketch.id.get(),
                origin: sketch.origin,
                profile: sketch.profile.clone(),
                segments: self.project.sketch_segment_count(sketch)?,
            });
        }

        let mut features = Vec::with_capacity(self.project.features().len());
        for feature in self.project.features() {
            features.push(FeatureView {
                id: feature.id.get(),
                sketch_id: feature.sketch_id.get(),
                depth_param: feature.depth_param.clone(),
                depth_mm: unit.to_mm(self.project.parameter(&feature.depth_param)?),
            });
        }

        let outline = self
            .project
            .features()
            .last()
            .and_then(|feature| self.project.sketch(feature.sketch_id))
            .or_else(|| self.project.sketches().first())
            .map(|sketch| self.project.sketch_profile_mm(sketch))
            .transpose()?;

        let (mesh_view, bounds, geometry) = match &mesh {
            Some(mesh) => (
                MeshView {
                    positions: mesh.positions.clone(),
                    triangles: mesh.triangles.clone(),
                },
                mesh.bounding_box().map(|box3| BoundsView {
                    min: box3.min,
                    max: box3.max,
                }),
                GeometryView {
                    volume_mm3: mesh.signed_volume(),
                    surface_area_mm2: mesh.surface_area(),
                    triangles: mesh.triangle_count(),
                    vertices: mesh.vertex_count(),
                },
            ),
            None => (
                MeshView {
                    positions: Vec::new(),
                    triangles: Vec::new(),
                },
                None,
                GeometryView {
                    volume_mm3: 0.0,
                    surface_area_mm2: 0.0,
                    triangles: 0,
                    vertices: 0,
                },
            ),
        };

        Ok(StatePayload {
            schema: self.project.schema,
            revision: self.project.revision.get(),
            name: self.project.name.clone(),
            file: self.path.display().to_string(),
            units: unit.symbol().to_string(),
            unit: serde_json::to_value(unit)?
                .as_str()
                .unwrap_or("millimeter")
                .to_string(),
            dirty: self.project.revision.get() != self.saved_revision,
            parameters,
            sketches,
            features,
            mesh: mesh_view,
            outline,
            bounds,
            geometry,
            kernel: KernelView {
                name: info.name,
                version: info.version,
                capabilities: info
                    .capabilities
                    .iter()
                    .map(|capability| capability.label())
                    .collect(),
            },
            history: HistoryView {
                undo: self.project.undo_depth(),
                redo: self.project.redo_depth(),
            },
        })
    }

    /// Apply a mutation, evaluate it, and restore the previous project if anything fails.
    ///
    /// A parameter edit that makes the kernel fail must not leave the document in the broken state:
    /// the user would then be unable to undo it, because the undo snapshot is inside the document.
    fn mutate<F>(&mut self, change: F) -> Result<StatePayload, StudioError>
    where
        F: FnOnce(&mut Project) -> Result<(), ProjectError>,
    {
        let backup = self.project.clone();
        if let Err(error) = change(&mut self.project) {
            self.project = backup;
            return Err(StudioError::Project(error));
        }
        match self.payload() {
            Ok(payload) => Ok(payload),
            Err(error) => {
                self.project = backup;
                Err(error)
            }
        }
    }

    fn save(&mut self) -> Result<StatePayload, StudioError> {
        self.project.save(&self.path)?;
        self.saved_revision = self.project.revision.get();
        self.payload()
    }

    fn stl(&self) -> Result<(Vec<u8>, String), StudioError> {
        if self.project.features().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to export".into(),
            ));
        }
        let mut kernel = TruckKernel::new();
        let evaluated = self.project.evaluate(&mut kernel)?;
        kernel.release(evaluated.solid);
        let bytes = cad_io::stl_bytes(&evaluated.mesh)?;
        Ok((bytes, export_name(&self.project.name)))
    }
}

fn export_name(name: &str) -> String {
    let mut stem: String = name
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .take(64)
        .collect();
    if stem.is_empty() {
        stem.push_str("part");
    }
    format!("{stem}.stl")
}

// ---------------------------------------------------------------------------------------------
// Public lifecycle
// ---------------------------------------------------------------------------------------------

/// A running studio. Dropping it stops the listener and joins the accept thread.
pub struct StudioHandle {
    addr: SocketAddr,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StudioHandle {
    /// The loopback address the studio is listening on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The URL to open in a browser.
    pub fn url(&self) -> String {
        format!("http://{}", self.addr)
    }

    /// Stop accepting connections and join the accept thread.
    ///
    /// The accept loop blocks in `accept`, so shutdown opens one throwaway loopback connection to
    /// wake it. Without that wake-up the join would wait for the next real client.
    pub fn shutdown(&mut self) {
        self.shutdown.store(true, Ordering::SeqCst);
        let _ = TcpStream::connect(self.addr);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

impl Drop for StudioHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Start the studio on `127.0.0.1:port` (port 0 asks the OS for a free port).
///
/// Returns once the listener is bound and the initial project has evaluated. A failed evaluation is
/// returned here rather than after the user has opened a page that cannot render.
pub fn start(project: Project, path: PathBuf, port: u16) -> Result<StudioHandle, StudioError> {
    let state = StudioState::new(project, path)?;
    let listener = TcpListener::bind(("127.0.0.1", port)).map_err(StudioError::Bind)?;
    let addr = listener.local_addr().map_err(StudioError::Bind)?;
    let state = Arc::new(Mutex::new(state));
    let shutdown = Arc::new(AtomicBool::new(false));
    let connections = Arc::new(AtomicUsize::new(0));

    let thread_shutdown = Arc::clone(&shutdown);
    let thread_state = Arc::clone(&state);
    let thread_connections = Arc::clone(&connections);
    let thread = thread::Builder::new()
        .name("atlas-studio-accept".into())
        .spawn(move || {
            accept_loop(
                listener,
                thread_state,
                thread_connections,
                thread_shutdown,
                addr,
            );
        })
        .map_err(StudioError::Bind)?;

    Ok(StudioHandle {
        addr,
        shutdown,
        thread: Some(thread),
    })
}

fn accept_loop(
    listener: TcpListener,
    state: Arc<Mutex<StudioState>>,
    connections: Arc<AtomicUsize>,
    shutdown: Arc<AtomicBool>,
    addr: SocketAddr,
) {
    while !shutdown.load(Ordering::SeqCst) {
        match listener.accept() {
            Ok((mut stream, _peer)) => {
                if shutdown.load(Ordering::SeqCst) {
                    break;
                }
                if connections.fetch_add(1, Ordering::SeqCst) >= MAX_CONNECTIONS {
                    connections.fetch_sub(1, Ordering::SeqCst);
                    let _ = http::write_response(
                        &mut stream,
                        &json_response(
                            503,
                            serde_json::json!({
                                "error": {
                                    "kind": "busy",
                                    "message": "the studio is handling too many connections"
                                }
                            }),
                        ),
                    );
                    continue;
                }
                let slot = ConnectionSlot(Arc::clone(&connections));
                let state = Arc::clone(&state);
                let _ = thread::Builder::new()
                    .name("atlas-studio-connection".into())
                    .spawn(move || {
                        let _slot = slot;
                        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                            handle_connection(stream, &state, addr.port());
                        }))
                        .ok();
                    });
            }
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
}

struct ConnectionSlot(Arc<AtomicUsize>);

impl Drop for ConnectionSlot {
    fn drop(&mut self) {
        self.0.fetch_sub(1, Ordering::SeqCst);
    }
}

// ---------------------------------------------------------------------------------------------
// Request handling
// ---------------------------------------------------------------------------------------------

fn handle_connection(mut stream: TcpStream, state: &Arc<Mutex<StudioState>>, port: u16) {
    let response = match http::read_request(&mut stream) {
        Ok(request) => route(state, &request, port),
        Err(http::HttpError::BodyTooLarge) => error_response(&StudioError::PayloadTooLarge),
        Err(http::HttpError::HeadersTooLarge) => json_response(
            431,
            serde_json::json!({
                "error": { "kind": "headers_too_large", "message": "request headers are too large" }
            }),
        ),
        Err(http::HttpError::Malformed(message)) => {
            error_response(&StudioError::BadRequest(message))
        }
        Err(http::HttpError::Io) => return,
    };
    let _ = http::write_response(&mut stream, &response);
    let _ = stream.shutdown(std::net::Shutdown::Both);
}

fn route(state: &Arc<Mutex<StudioState>>, request: &http::Request, port: u16) -> http::Response {
    // A browser resolving an attacker-controlled name to 127.0.0.1 still sends that name in `Host`.
    // Refusing anything but the studio's own loopback host is what stops DNS-rebinding reads.
    if let Err(error) = require_loopback_host(request, port) {
        return error_response(&error);
    }
    match (request.method.as_str(), request.path.as_str()) {
        ("GET", "/") => asset(INDEX_HTML, "text/html; charset=utf-8"),
        ("GET", "/app.css") => asset(APP_CSS, "text/css; charset=utf-8"),
        ("GET", "/app.js") => asset(APP_JS, "text/javascript; charset=utf-8"),
        ("GET", "/favicon.svg") => asset(FAVICON, "image/svg+xml"),
        ("GET", "/api/health") => json_response(
            200,
            serde_json::json!({ "ok": true, "version": env!("CARGO_PKG_VERSION") }),
        ),
        ("GET", "/api/state") => match with_state(state, |state| state.payload()) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/export/stl") => match with_state(state, |state| state.stl()) {
            Ok((bytes, filename)) => http::Response::new(200, "OK", "model/stl", bytes)
                .with_header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{filename}\""),
                ),
            Err(error) => error_response(&error),
        },
        ("POST", path) if path.starts_with("/api/") => {
            match require_same_origin(request, port).and_then(|()| require_json(request)) {
                Ok(()) => post_route(state, request),
                Err(error) => error_response(&error),
            }
        }
        ("POST", _) => error_response(&StudioError::MethodNotAllowed),
        (method, _) if method != "GET" => error_response(&StudioError::MethodNotAllowed),
        _ => error_response(&StudioError::NotFound),
    }
}

fn post_route(state: &Arc<Mutex<StudioState>>, request: &http::Request) -> http::Response {
    match request.path.as_str() {
        "/api/parameter" => match parse_body::<ParameterRequest>(request) {
            Ok(body) => match with_state(state, |state| {
                state.mutate(|project| project.set_parameter(&body.id, body.value))
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sketch/profile" => match parse_body::<ProfileRequest>(request) {
            Ok(body) => {
                let id = match EntityId::new(body.id) {
                    Ok(id) => id,
                    Err(error) => {
                        return error_response(&StudioError::BadRequest(error.to_string()));
                    }
                };
                match with_state(state, |state| {
                    state.mutate(|project| {
                        let current = project.sketch(id).map(|sketch| sketch.profile.clone());
                        let profile = profile_for(&body, current.as_ref())
                            .map_err(|error| ProjectError::Invalid(error.to_string()))?;
                        project.set_sketch_profile(id, profile)
                    })
                }) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/sketch/origin" => match parse_body::<OriginRequest>(request) {
            Ok(body) => {
                let id = match EntityId::new(body.id) {
                    Ok(id) => id,
                    Err(error) => {
                        return error_response(&StudioError::BadRequest(error.to_string()));
                    }
                };
                match with_state(state, |state| {
                    state.mutate(|project| project.set_sketch_origin(id, body.origin))
                }) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/undo" => match with_state(state, |state| state.mutate(Project::undo)) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/redo" => match with_state(state, |state| state.mutate(Project::redo)) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/save" => match with_state(state, |state| state.save()) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        _ => error_response(&StudioError::NotFound),
    }
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ParameterRequest {
    id: String,
    value: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ProfileRequest {
    id: u64,
    kind: String,
    #[serde(default)]
    sides: Option<u32>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginRequest {
    id: u64,
    origin: [f64; 2],
}

fn profile_for(
    request: &ProfileRequest,
    current: Option<&SketchProfile>,
) -> Result<SketchProfile, StudioError> {
    let kind = request.kind.trim().to_ascii_lowercase();
    match kind.as_str() {
        "rectangle" | "box" => {
            let (width_param, height_param) = match current {
                Some(SketchProfile::Rectangle {
                    width_param,
                    height_param,
                }) => (width_param.clone(), height_param.clone()),
                _ => ("width".to_string(), "height".to_string()),
            };
            Ok(SketchProfile::Rectangle {
                width_param,
                height_param,
            })
        }
        "circle" | "cylinder" => Ok(SketchProfile::Circle {
            diameter_param: "diameter".to_string(),
        }),
        "regular_polygon" | "polygon" | "prism" => {
            let sides = request
                .sides
                .ok_or_else(|| StudioError::BadRequest("a polygon needs a `sides` value".into()))?;
            Ok(SketchProfile::RegularPolygon {
                diameter_param: "diameter".to_string(),
                sides,
            })
        }
        other => Err(StudioError::BadRequest(format!(
            "unknown profile `{other}`; expected rectangle, circle, or regular_polygon"
        ))),
    }
}

fn require_loopback_host(request: &http::Request, port: u16) -> Result<(), StudioError> {
    let host = request
        .header("host")
        .ok_or(StudioError::Forbidden)?
        .trim()
        .to_ascii_lowercase();
    let allowed = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if allowed.iter().any(|candidate| candidate == &host) {
        Ok(())
    } else {
        Err(StudioError::Forbidden)
    }
}

fn require_same_origin(request: &http::Request, port: u16) -> Result<(), StudioError> {
    let origin = request
        .header("origin")
        .ok_or(StudioError::Forbidden)?
        .trim()
        .to_ascii_lowercase();
    let allowed = [
        format!("http://127.0.0.1:{port}"),
        format!("http://localhost:{port}"),
    ];
    if allowed.iter().any(|candidate| candidate == &origin) {
        Ok(())
    } else {
        Err(StudioError::Forbidden)
    }
}

fn require_json(request: &http::Request) -> Result<(), StudioError> {
    let content_type = request
        .header("content-type")
        .ok_or(StudioError::UnsupportedMediaType)?;
    if content_type
        .trim()
        .to_ascii_lowercase()
        .starts_with("application/json")
    {
        Ok(())
    } else {
        Err(StudioError::UnsupportedMediaType)
    }
}

fn parse_body<T: serde::de::DeserializeOwned>(request: &http::Request) -> Result<T, StudioError> {
    if request.body.is_empty() {
        return Err(StudioError::BadRequest("request body is empty".into()));
    }
    serde_json::from_slice(&request.body).map_err(StudioError::from)
}

fn with_state<T>(
    state: &Arc<Mutex<StudioState>>,
    operation: impl FnOnce(&mut StudioState) -> Result<T, StudioError>,
) -> Result<T, StudioError> {
    let mut guard = state.lock().map_err(|_| StudioError::Lock)?;
    operation(&mut guard)
}

fn asset(body: &str, content_type: &'static str) -> http::Response {
    http::Response::new(200, "OK", content_type, body.as_bytes().to_vec())
}

fn to_json<T: Serialize>(status: u16, payload: &T) -> http::Response {
    match serde_json::to_vec(payload) {
        Ok(body) => http::Response::new(
            status,
            http::reason_for(status),
            "application/json; charset=utf-8",
            body,
        ),
        Err(error) => error_response(&StudioError::Json(error.to_string())),
    }
}

fn json_response(status: u16, payload: serde_json::Value) -> http::Response {
    to_json(status, &payload)
}

fn error_response(error: &StudioError) -> http::Response {
    let status = error.status();
    json_response(
        status,
        serde_json::json!({
            "error": {
                "kind": error.kind(),
                "message": error.to_string(),
            }
        }),
    )
}

/// Convenience for the binary: default project for a new studio file.
pub fn default_project(name: &str, size: [f64; 3]) -> Result<Project, StudioError> {
    Ok(Project::box_part(
        name,
        DocumentId::new(1),
        cad_contract::units::Unit::Millimeter,
        size,
    )?)
}

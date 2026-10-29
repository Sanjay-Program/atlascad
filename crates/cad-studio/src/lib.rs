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
use cad_document::{
    BuildCache, ExtrudeOperation, FeatureFailure, FeatureKind, FeatureOutcome, FeatureReport,
    Project, ProjectError, RecoveryInfo, RevolveAxis, SketchProfile, clear_recovery,
};
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
                ProjectError::Kernel(_) | ProjectError::FeatureFailed { .. } => 422,
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
            StudioError::Project(ProjectError::FeatureFailed { .. }) => "feature_failed",
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
    /// `extrude` or `revolve`.
    kind: &'static str,
    /// For an extrude: the parameter that drives its depth.
    depth_param: Option<String>,
    /// For an extrude: that depth in millimetres.
    depth_mm: Option<f64>,
    /// For a revolve: `vertical` or `horizontal`, the sketch axis it turns about.
    axis: Option<&'static str>,
    /// `new_body`, `join`, or `cut`, in build order.
    operation: &'static str,
    /// `built`, `failed`, or `pending` when an earlier feature failed first.
    status: &'static str,
    /// Volume after this feature rebuilt, when it did.
    volume_mm3: Option<f64>,
    /// The kernel's message when this feature could not rebuild.
    message: Option<String>,
}

#[derive(Debug, Serialize)]
struct FailureView {
    feature: u64,
    message: String,
}

/// One rebuild of the whole history, in the form the payload needs it.
struct Rebuild {
    /// The mesh of the last feature that rebuilt, or nothing when no feature did.
    mesh: Option<cad_contract::mesh::Mesh>,
    /// One entry per attempted feature, in build order.
    reports: Vec<FeatureReport>,
    /// The feature that stopped the rebuild, if any.
    failure: Option<FeatureFailure>,
    kernel: KernelInfo,
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

/// The unsaved-edit snapshot this session is running on top of.
///
/// `recovered` stays true until the user saves (which promotes the snapshot to the project file) or
/// discards it, so a client can keep the warning visible for exactly as long as it is true.
#[derive(Debug, Serialize)]
struct RecoveryView {
    recovered: bool,
    saved_at_unix: Option<u64>,
    file_unreadable: bool,
    /// A snapshot write failed, so unsaved work is not currently protected. Shown, not swallowed.
    autosave_error: Option<String>,
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
    /// The first feature that could not rebuild. The mesh above is the last one that did.
    failure: Option<FailureView>,
    kernel: KernelView,
    history: HistoryView,
    recovery: RecoveryView,
}

// ---------------------------------------------------------------------------------------------
// State
// ---------------------------------------------------------------------------------------------

struct StudioState {
    project: Project,
    path: PathBuf,
    /// Revision at the last successful save or load. `dirty` is a conservative comparison.
    saved_revision: u64,
    /// The one kernel instance this session builds with. It stays alive so cached solids stay
    /// valid: a handle is only meaningful to the kernel that issued it.
    kernel: TruckKernel,
    /// Per-feature rebuild results keyed by the resolved inputs of their prefix. Owns every solid
    /// it hands out, which is why nothing below releases a `build.solid`.
    cache: BuildCache,
    /// Set when the session started from an unsaved-edit snapshot, cleared by save or discard.
    recovered: Option<RecoveryInfo>,
    /// The last autosave failure, if any. A failed snapshot must be visible rather than silent.
    autosave_error: Option<String>,
}

impl StudioState {
    fn new(
        project: Project,
        path: PathBuf,
        recovered: Option<RecoveryInfo>,
    ) -> Result<Self, StudioError> {
        project.validate()?;
        let saved_revision = project.revision.get();
        let mut state = StudioState {
            saved_revision,
            project,
            path,
            kernel: TruckKernel::new(),
            cache: BuildCache::new(),
            recovered,
            autosave_error: None,
        };
        // Evaluate once at startup so a broken project is refused before a URL is printed.
        let _ = state.payload()?;
        Ok(state)
    }

    /// Rebuild the history and hand back only the parts the UI may see.
    ///
    /// A failing feature is part of the result, not an error: the browser shows the feature tree
    /// with that feature marked and the mesh of the last feature that did rebuild.
    ///
    /// Features whose resolved inputs are unchanged since the last rebuild come from
    /// [`BuildCache`] instead of the kernel, so editing the last parameter of a long history does
    /// not re-extrude or re-union the features before it.
    fn rebuild(&mut self) -> Result<Rebuild, StudioError> {
        let info = self.kernel.info();
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        Ok(Rebuild {
            mesh: build.mesh,
            reports: build.reports,
            failure: build.failed,
            kernel: info,
        })
    }

    fn payload(&mut self) -> Result<StatePayload, StudioError> {
        let Rebuild {
            mesh,
            reports,
            failure,
            kernel: info,
        } = self.rebuild()?;
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
            let (status, volume_mm3, message) = match reports
                .iter()
                .find(|report| report.id == feature.id)
                .map(|report| &report.outcome)
            {
                Some(FeatureOutcome::Built { volume_mm3, .. }) => {
                    ("built", Some(*volume_mm3), None)
                }
                Some(FeatureOutcome::Failed { message }) => ("failed", None, Some(message.clone())),
                None => ("pending", None, None),
            };
            let (depth_param, depth_mm, axis) = match &feature.kind {
                FeatureKind::Extrude { depth_param } => (
                    Some(depth_param.clone()),
                    Some(unit.to_mm(self.project.parameter(depth_param)?)),
                    None,
                ),
                FeatureKind::Revolve { axis } => (None, None, Some(axis.label())),
            };
            features.push(FeatureView {
                id: feature.id.get(),
                sketch_id: feature.sketch_id.get(),
                kind: feature.kind.label(),
                depth_param,
                depth_mm,
                axis,
                operation: feature.operation.label(),
                status,
                volume_mm3,
                message,
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
            // A recovered session is dirty by definition: the file on disk is not what is on screen.
            dirty: self.recovered.is_some() || self.project.revision.get() != self.saved_revision,
            parameters,
            sketches,
            features,
            mesh: mesh_view,
            outline,
            bounds,
            geometry,
            failure: failure.map(|failure| FailureView {
                feature: failure.id.get(),
                message: failure.message,
            }),
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
            recovery: RecoveryView {
                recovered: self.recovered.is_some(),
                saved_at_unix: self.recovered.map(|info| info.saved_at_unix),
                file_unreadable: self.recovered.is_some_and(|info| info.file_unreadable),
                autosave_error: self.autosave_error.clone(),
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
        // Evaluate first: a mutation that cannot rebuild is rolled back, and nothing that was
        // rolled back may be written to the snapshot.
        let payload = match self.payload() {
            Ok(payload) => payload,
            Err(error) => {
                self.project = backup;
                return Err(error);
            }
        };
        self.autosave();
        let mut payload = payload;
        payload.recovery = self.recovery_view();
        Ok(payload)
    }

    /// Keep the unsaved edit in a recovery snapshot, next to the project file.
    ///
    /// A failure is recorded and reported in the state payload rather than failing the edit: a
    /// read-only directory must not make the application unusable, but it must not silently pretend
    /// the work is protected either.
    fn autosave(&mut self) {
        match self.project.save_recovery(&self.path) {
            Ok(_) => self.autosave_error = None,
            Err(error) => self.autosave_error = Some(error.to_string()),
        }
    }

    fn recovery_view(&self) -> RecoveryView {
        RecoveryView {
            recovered: self.recovered.is_some(),
            saved_at_unix: self.recovered.map(|info| info.saved_at_unix),
            file_unreadable: self.recovered.is_some_and(|info| info.file_unreadable),
            autosave_error: self.autosave_error.clone(),
        }
    }

    fn save(&mut self) -> Result<StatePayload, StudioError> {
        self.project.save(&self.path)?;
        self.saved_revision = self.project.revision.get();
        // The project file is now the newest copy, so the snapshot would only be a stale offer on
        // the next start. Failing to remove it is reported, not ignored.
        clear_recovery(&self.path)?;
        self.recovered = None;
        self.autosave_error = None;
        self.payload()
    }

    /// Throw away the recovered state and go back to the project file on disk.
    ///
    /// This is the escape hatch the recovery banner needs: a user who does not want the unsaved
    /// edit must not have to delete a file by hand.
    fn discard_recovery(&mut self) -> Result<StatePayload, StudioError> {
        let project = Project::load(&self.path)?;
        self.project = project;
        self.saved_revision = self.project.revision.get();
        self.cache.clear(&mut self.kernel);
        clear_recovery(&self.path)?;
        self.recovered = None;
        self.autosave_error = None;
        self.payload()
    }

    fn stl(&mut self) -> Result<(Vec<u8>, String), StudioError> {
        if self.project.features().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to export".into(),
            ));
        }
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt, so the export would not match the model: {}",
                failure.id, failure.message
            )));
        }
        let mesh = build
            .mesh
            .ok_or_else(|| StudioError::BadRequest("this project has no solid to export".into()))?;
        let bytes = cad_io::stl_bytes(&mesh)?;
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
    start_with_recovery(project, path, port, None)
}

/// Start the studio on a project recovered from an unsaved-edit snapshot.
///
/// `recovered` is reported through `/api/state` until the user saves or discards, so the client can
/// warn that the document on screen is not what the file on disk holds.
pub fn start_with_recovery(
    project: Project,
    path: PathBuf,
    port: u16,
    recovered: Option<RecoveryInfo>,
) -> Result<StudioHandle, StudioError> {
    let state = StudioState::new(project, path, recovered)?;
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
        "/api/sketch" => match parse_body::<NewSketchRequest>(request) {
            Ok(body) => {
                let profile = match profile_from_parts(&body.kind, body.sides, body.points, None) {
                    Ok(profile) => profile,
                    Err(error) => return error_response(&error),
                };
                let origin = body.origin.unwrap_or([0.0, 0.0]);
                match with_state(state, |state| {
                    state.mutate(|project| project.add_sketch(profile, origin).map(|_| ()))
                }) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/feature/extrude" => match parse_body::<NewFeatureRequest>(request) {
            Ok(body) => {
                let operation = match operation_for(&body.operation) {
                    Ok(operation) => operation,
                    Err(error) => return error_response(&error),
                };
                let sketch_id = match EntityId::new(body.sketch_id) {
                    Ok(id) => id,
                    Err(error) => {
                        return error_response(&StudioError::BadRequest(error.to_string()));
                    }
                };
                match with_state(state, |state| {
                    state.mutate(|project| {
                        project
                            .add_extrude_feature(sketch_id, &body.depth_param, operation)
                            .map(|_| ())
                    })
                }) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/feature/revolve" => match parse_body::<NewRevolveRequest>(request) {
            Ok(body) => {
                let operation = match operation_for(&body.operation) {
                    Ok(operation) => operation,
                    Err(error) => return error_response(&error),
                };
                let axis = match axis_for(&body.axis) {
                    Ok(axis) => axis,
                    Err(error) => return error_response(&error),
                };
                let sketch_id = match EntityId::new(body.sketch_id) {
                    Ok(id) => id,
                    Err(error) => {
                        return error_response(&StudioError::BadRequest(error.to_string()));
                    }
                };
                match with_state(state, |state| {
                    state.mutate(|project| {
                        project
                            .add_revolve_feature(sketch_id, axis, operation)
                            .map(|_| ())
                    })
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
        "/api/recovery/discard" => match with_state(state, |state| state.discard_recovery()) {
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
    /// A drawn loop, in document units, relative to the sketch origin. Required for `polyline` and
    /// refused for every other kind, so a request cannot silently discard a drawn profile.
    #[serde(default)]
    points: Option<Vec<[f64; 2]>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginRequest {
    id: u64,
    origin: [f64; 2],
}

/// A new sketch. The profile is driven by the document's default parameters (`width`/`height`, or
/// `diameter`), which is why the shape is all this request needs to carry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewSketchRequest {
    kind: String,
    #[serde(default)]
    origin: Option<[f64; 2]>,
    #[serde(default)]
    sides: Option<u32>,
    /// A drawn loop, in document units, relative to `origin`. Required for `polyline`.
    #[serde(default)]
    points: Option<Vec<[f64; 2]>>,
}

/// A new extrude. `operation` decides whether it starts the body or joins the one built so far; the
/// document refuses the wrong one for the position in history, so nothing here has to guess.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewFeatureRequest {
    sketch_id: u64,
    #[serde(default = "default_depth_param")]
    depth_param: String,
    operation: String,
}

fn default_depth_param() -> String {
    "depth".to_string()
}

/// A new revolve. It has no depth: the tool is the sketch turned a full turn about one of the
/// sketch's own axes, so the axis is the only extra input.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewRevolveRequest {
    sketch_id: u64,
    operation: String,
    #[serde(default = "default_axis")]
    axis: String,
}

fn default_axis() -> String {
    "vertical".to_string()
}

fn axis_for(name: &str) -> Result<RevolveAxis, StudioError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "vertical" | "y" => Ok(RevolveAxis::Vertical),
        "horizontal" | "x" => Ok(RevolveAxis::Horizontal),
        other => Err(StudioError::BadRequest(format!(
            "unknown revolve axis `{other}`; expected vertical or horizontal"
        ))),
    }
}

fn operation_for(name: &str) -> Result<ExtrudeOperation, StudioError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "new_body" | "new" | "body" => Ok(ExtrudeOperation::NewBody),
        "join" | "union" => Ok(ExtrudeOperation::Join),
        "cut" | "difference" => Ok(ExtrudeOperation::Cut),
        other => Err(StudioError::BadRequest(format!(
            "unknown operation `{other}`; expected new_body, join, or cut"
        ))),
    }
}

fn profile_for(
    request: &ProfileRequest,
    current: Option<&SketchProfile>,
) -> Result<SketchProfile, StudioError> {
    profile_from_parts(
        &request.kind,
        request.sides,
        request.points.clone(),
        current,
    )
}

/// Build a profile from the shape name the API accepts.
///
/// `current` is the profile being replaced, so an edit keeps the parameter names the document
/// already uses; a new sketch gets the document's default ones.
fn profile_from_parts(
    kind: &str,
    sides: Option<u32>,
    points: Option<Vec<[f64; 2]>>,
    current: Option<&SketchProfile>,
) -> Result<SketchProfile, StudioError> {
    let kind = kind.trim().to_ascii_lowercase();
    // A loop is only meaningful for a polyline; sending one with another kind is a caller bug that
    // would otherwise look like a successful profile change.
    if points.is_some() && kind != "polyline" && kind != "path" && kind != "draw" {
        return Err(StudioError::BadRequest(format!(
            "`points` is only valid for a polyline profile, not `{kind}`"
        )));
    }
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
            let sides = sides
                .ok_or_else(|| StudioError::BadRequest("a polygon needs a `sides` value".into()))?;
            Ok(SketchProfile::RegularPolygon {
                diameter_param: "diameter".to_string(),
                sides,
            })
        }
        "polyline" | "path" | "draw" => {
            let points = points.ok_or_else(|| {
                StudioError::BadRequest(
                    "a polyline needs a `points` array of at least three [x, y] pairs".into(),
                )
            })?;
            Ok(SketchProfile::Polyline { points })
        }
        other => Err(StudioError::BadRequest(format!(
            "unknown profile `{other}`; expected rectangle, circle, regular_polygon, or polyline"
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

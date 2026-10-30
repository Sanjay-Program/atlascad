//! Atlas CAD Studio — a local-first application shell over the platform's project and kernel.
//!
//! The studio is one process: it owns the [`Project`], evaluates it through the Truck adapter, and
//! serves a single browser tab on loopback. There is no cloud dependency, no account, and no
//! background service. Closing the process is the whole shutdown procedure.
//!
//! # Trust boundary
//!
//! By default the listener binds `127.0.0.1` only, never a public interface. Sharing is opt-in and
//! changes that in exactly one way:
//!
//! * [`ShareOptions`] can bind another interface (typically `0.0.0.0` on a local network). A
//!   non-loopback bind always requires a token — supplied or generated — and every `/api/*` request
//!   must present it as `X-Atlas-Token` or `?token=`. Static assets are served so the page can load;
//!   they carry no data.
//! * otherwise: request headers are capped at 16 KiB and JSON bodies at 1 MiB;
//! * every `POST` must carry a same-origin `Origin` and a JSON content type, so another site cannot
//!   drive the model through the user's browser;
//! * no endpoint accepts a filesystem path from the client — the project path is fixed at launch;
//! * the only file written is the project itself and the only export is STL returned in the response.
//!
//! A share link grants full edit access to the open document to anyone who holds it. This is still a
//! local-first tool, not a multi-tenant service: do not expose it through a tunnel or a public port
//! and call it hosted.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

mod http;

use cad_contract::capability::KernelInfo;
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::kernel::Kernel;
use cad_document::{
    BuildCache, ExtrudeDirection, ExtrudeOperation, FeatureFailure, FeatureKind, FeatureOutcome,
    FeatureReport, Project, ProjectError, RecoveryInfo, RecoverySource, RevisionSummary,
    RevolveAxis, SketchProfile, clear_recovery,
};
use cad_drafting::Draft;
use cad_kernel_truck::TruckKernel;
use serde::{Deserialize, Serialize};
use std::net::{IpAddr, Ipv4Addr, SocketAddr, TcpListener, TcpStream};
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
    Drafting(cad_drafting::PlanError),
    /// A drawing operation (edit, save, plot) failed; the drawing is its own document.
    DrawingDoc(cad_drafting::DraftError),
    /// A sheet could not be laid out: empty drawing, a scale that does not fit, a bad field.
    Sheet(cad_drafting::PlotError),
    Json(String),
    Io(String),
    BadRequest(String),
    Forbidden,
    /// Sharing is on and the request did not present the session token.
    Unauthorized,
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
            StudioError::Drafting(error) => write!(f, "drawing: {error}"),
            StudioError::DrawingDoc(error) => write!(f, "drawing: {error}"),
            StudioError::Sheet(error) => write!(f, "sheet: {error}"),
            StudioError::Json(message) => write!(f, "JSON: {message}"),
            StudioError::Io(message) => write!(f, "I/O: {message}"),
            StudioError::BadRequest(message) => write!(f, "{message}"),
            StudioError::Forbidden => f.write_str("request is not from this studio's origin"),
            StudioError::Unauthorized => {
                f.write_str("this studio is shared; the request needs its access token")
            }
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

impl From<cad_drafting::PlanError> for StudioError {
    fn from(error: cad_drafting::PlanError) -> Self {
        StudioError::Drafting(error)
    }
}

impl From<cad_drafting::DraftError> for StudioError {
    fn from(error: cad_drafting::DraftError) -> Self {
        StudioError::DrawingDoc(error)
    }
}

impl From<cad_drafting::PlotError> for StudioError {
    fn from(error: cad_drafting::PlotError) -> Self {
        StudioError::Sheet(error)
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
            StudioError::Unauthorized => 401,
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
                ProjectError::UnknownRevision(_) => 404,
                ProjectError::NothingToUndo | ProjectError::NothingToRedo => 409,
                ProjectError::Kernel(_) | ProjectError::FeatureFailed { .. } => 422,
                ProjectError::Io { .. } => 500,
            },
            // A plan can only fail because the project it describes is invalid, which the same
            // request would report as a 400 for any other endpoint.
            StudioError::Drafting(_) | StudioError::DrawingDoc(_) | StudioError::Sheet(_) => 400,
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
            StudioError::Unauthorized => "unauthorized",
            StudioError::NotFound => "not_found",
            StudioError::MethodNotAllowed => "method_not_allowed",
            StudioError::UnsupportedMediaType => "unsupported_media_type",
            StudioError::PayloadTooLarge => "payload_too_large",
            StudioError::Project(ProjectError::NothingToUndo) => "nothing_to_undo",
            StudioError::Project(ProjectError::NothingToRedo) => "nothing_to_redo",
            StudioError::Project(ProjectError::UnknownParameter(_)) => "unknown_parameter",
            StudioError::Project(ProjectError::UnknownSketch(_)) => "unknown_sketch",
            StudioError::Project(ProjectError::UnknownRevision(_)) => "unknown_revision",
            StudioError::Project(ProjectError::FeatureFailed { .. }) => "feature_failed",
            StudioError::Project(_) => "invalid_project",
            StudioError::Kernel(_) => "kernel_failure",
            StudioError::Stl(_) => "export_failure",
            StudioError::Drafting(_) | StudioError::DrawingDoc(_) => "drawing_failure",
            StudioError::Sheet(_) => "sheet_failure",
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
    /// Height of the sketch plane above z = 0, in document units.
    plane_z: f64,
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
    /// For an extrude: `up` or `down`, the way it sweeps from its sketch plane.
    direction: Option<&'static str>,
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

/// The link a colleague can open while this studio is running. Absent unless sharing is on.
#[derive(Debug, Serialize, Clone)]
struct ShareView {
    url: String,
}

/// How a shared session proves a caller may read and write the document.
#[derive(Debug, Clone)]
struct ShareAccess {
    token: String,
    url: String,
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
    /// `snapshot` for an unsaved-edit snapshot, `backup` for the previous good file.
    source: Option<&'static str>,
    /// A snapshot write failed, so unsaved work is not currently protected. Shown, not swallowed.
    autosave_error: Option<String>,
}

/// The drawing the panel shows and edits, ready for the SVG preview and for DXF/PDF export.
#[derive(Debug, Serialize)]
struct DraftView {
    name: String,
    unit: String,
    layers: Vec<cad_drafting::Layer>,
    entities: Vec<cad_drafting::Entity>,
    /// `[min_x, min_y, max_x, max_y]`, or null for an empty drawing.
    bounds: Option<[f64; 4]>,
    total_length: f64,
    /// `model` while the plan is generated from the sketch, `file` once it is (or was saved as) a
    /// `.zdraft` beside the project. The distinction matters: a `model` drawing follows the model,
    /// a `file` drawing does not.
    source: &'static str,
    file: Option<String>,
    dirty: bool,
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
    /// The first feature that could not rebuild. The mesh above is the last one that did.
    failure: Option<FailureView>,
    kernel: KernelView,
    history: HistoryView,
    recovery: RecoveryView,
    /// Named copies of the history, oldest first. Empty until the user records one.
    revisions: Vec<RevisionSummary>,
    /// Present only when the studio is shared beyond loopback; the UI offers it as a link.
    share: Option<ShareView>,
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
    /// Set when the studio is reached over a non-loopback interface. Every API call must then carry
    /// its token; set once, before the accept loop starts, and only read afterwards.
    share: Option<ShareAccess>,
    /// The drawing the panel shows. `None` until it is first needed, because a project that is only
    /// modelled never pays for a drawing it does not look at.
    drawing: Option<Draft>,
    /// Where the drawing lives when it is a file of its own (`<project stem>.zdraft`).
    drawing_file: Option<PathBuf>,
    /// Set by any drawing edit. A drawing generated from the model is regenerated when the model
    /// changes *unless* this is set: once a user has drawn on it, the drawing is theirs.
    drawing_edited: bool,
    /// The project revision the generated drawing was derived from.
    drawing_revision: u64,
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
            share: None,
            drawing: None,
            drawing_file: None,
            drawing_edited: false,
            drawing_revision: 0,
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
                plane_z: sketch.plane_z,
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
            let (depth_param, depth_mm, axis, direction) = match &feature.kind {
                FeatureKind::Extrude {
                    depth_param,
                    direction,
                } => (
                    Some(depth_param.clone()),
                    Some(unit.to_mm(self.project.parameter(depth_param)?)),
                    None,
                    Some(direction.label()),
                ),
                FeatureKind::Revolve { axis } => (None, None, Some(axis.label()), None),
            };
            features.push(FeatureView {
                id: feature.id.get(),
                sketch_id: feature.sketch_id.get(),
                kind: feature.kind.label(),
                depth_param,
                depth_mm,
                axis,
                direction,
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
                source: self.recovered.map(|info| match info.source {
                    RecoverySource::Snapshot => "snapshot",
                    RecoverySource::Backup => "backup",
                }),
                autosave_error: self.autosave_error.clone(),
            },
            revisions: self.project.revisions(),
            share: self.share.as_ref().map(|share| ShareView {
                url: share.url.clone(),
            }),
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
            source: self.recovered.map(|info| match info.source {
                RecoverySource::Snapshot => "snapshot",
                RecoverySource::Backup => "backup",
            }),
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

    /// Where this project's drawing is saved: the project's own stem with a `.zdraft` extension.
    ///
    /// A drawing is a document in its own right, so it lives in its own file. The project and the
    /// drawing can then be saved, opened and handed over independently, and a drawing is not a
    /// schema change to `.zcad`.
    fn drawing_path(&self) -> PathBuf {
        self.path.with_extension("zdraft")
    }

    /// Make sure the drawing exists and is not stale.
    ///
    /// A drawing generated from the model is regenerated when the model's revision changes, so the
    /// panel follows an edit to the sketch. The moment the user draws on it, or saves it, it becomes
    /// an independent document and the model no longer overwrites it — which is what a drafter
    /// expects, and the payload says which of the two the panel is showing.
    fn ensure_drawing(&mut self) -> Result<(), StudioError> {
        let revision = self.project.revision.get();
        let stale = match (
            &self.drawing,
            self.drawing_file.as_ref(),
            self.drawing_edited,
        ) {
            (None, _, _) => true,
            // A generated drawing follows the model until it is edited or saved.
            (Some(_), None, false) => self.drawing_revision != revision,
            _ => false,
        };
        if !stale {
            return Ok(());
        }
        if let Some(path) = self.drawing_file.clone() {
            self.drawing = Some(Draft::load(&path)?);
        } else {
            let sidecar = self.drawing_path();
            if sidecar.is_file() && !self.drawing_edited {
                self.drawing = Some(Draft::load(&sidecar)?);
                self.drawing_file = Some(sidecar);
            } else {
                self.drawing = Some(cad_drafting::plan_draft(&self.project)?);
                self.drawing_file = None;
            }
        }
        self.drawing_revision = revision;
        Ok(())
    }

    fn drawing_mut(&mut self) -> Result<&mut Draft, StudioError> {
        self.ensure_drawing()?;
        self.drawing
            .as_mut()
            .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))
    }

    fn draft_payload(&mut self) -> Result<DraftView, StudioError> {
        self.ensure_drawing()?;
        let draft = self
            .drawing
            .as_ref()
            .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))?;
        Ok(DraftView {
            name: draft.name.clone(),
            unit: draft.header.units.symbol().to_string(),
            layers: draft.layers().to_vec(),
            entities: draft.entities().to_vec(),
            bounds: draft.bounds(),
            total_length: draft.total_length(),
            source: if self.drawing_file.is_some() {
                "file"
            } else {
                "model"
            },
            file: self
                .drawing_file
                .as_ref()
                .map(|path| path.display().to_string()),
            dirty: self.drawing_edited,
            undo: draft.undo_depth(),
            redo: draft.redo_depth(),
        })
    }

    /// Add an entity the user drew. Validation is the drawing's, not this endpoint's: a radius of
    /// zero or a polyline of one point is refused by the document with its own message.
    fn drawing_add(
        &mut self,
        layer: &str,
        kind: cad_drafting::EntityKind,
    ) -> Result<DraftView, StudioError> {
        self.drawing_mut()?.add_entity(layer, kind)?;
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_remove(&mut self, id: EntityId) -> Result<DraftView, StudioError> {
        self.drawing_mut()?.remove_entity(id)?;
        self.drawing_edited = true;
        self.draft_payload()
    }

    /// Create a layer, or change the colour, visibility or lock of an existing one.
    fn drawing_layer(&mut self, request: &DrawingLayerRequest) -> Result<DraftView, StudioError> {
        {
            let draft = self.drawing_mut()?;
            if draft.layer(&request.name).is_none() {
                let color = request.color.unwrap_or(7);
                draft.add_layer(&request.name, color)?;
            }
            if let Some(color) = request.color
                && draft
                    .layer(&request.name)
                    .is_some_and(|layer| layer.color != color)
            {
                draft.set_layer_color(&request.name, color)?;
            }
            if let Some(visible) = request.visible
                && draft
                    .layer(&request.name)
                    .is_some_and(|layer| layer.visible != visible)
            {
                draft.set_layer_visible(&request.name, visible)?;
            }
            if let Some(locked) = request.locked
                && draft
                    .layer(&request.name)
                    .is_some_and(|layer| layer.locked != locked)
            {
                draft.set_layer_locked(&request.name, locked)?;
            }
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_undo(&mut self) -> Result<DraftView, StudioError> {
        self.drawing_mut()?.undo()?;
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_redo(&mut self) -> Result<DraftView, StudioError> {
        self.drawing_mut()?.redo()?;
        self.drawing_edited = true;
        self.draft_payload()
    }

    /// Write the drawing to its sidecar file. From then on it is a file, not a generated plan.
    fn drawing_save(&mut self) -> Result<DraftView, StudioError> {
        let path = self.drawing_path();
        {
            let draft = self.drawing_mut()?;
            draft.save(&path)?;
        }
        self.drawing_file = Some(path);
        self.drawing_edited = false;
        self.draft_payload()
    }

    fn dxf(&mut self) -> Result<(Vec<u8>, String), StudioError> {
        self.ensure_drawing()?;
        let draft = self
            .drawing
            .as_ref()
            .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))?;
        Ok((
            draft.to_dxf().into_bytes(),
            format!("{}-plan.dxf", export_stem(&self.project.name)),
        ))
    }

    /// Lay the drawing out on a sheet and serialise the page as PDF.
    fn pdf(&mut self, sheet: cad_drafting::Sheet) -> Result<(Vec<u8>, String), StudioError> {
        self.ensure_drawing()?;
        let draft = self
            .drawing
            .as_ref()
            .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))?;
        let plot = sheet.plot(draft)?;
        let pdf = cad_drafting::to_pdf(&plot, &draft.name);
        Ok((
            pdf.into_bytes(),
            format!(
                "{}-{}.pdf",
                export_stem(&self.project.name),
                sheet.paper.label().to_ascii_lowercase()
            ),
        ))
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
    format!("{}.stl", export_stem(name))
}

/// A file-system-safe stem for the project name, shared by every export.
fn export_stem(name: &str) -> String {
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
    stem
}

// ---------------------------------------------------------------------------------------------
// Public lifecycle
// ---------------------------------------------------------------------------------------------

/// How the studio is exposed beyond its default loopback binding.
///
/// Default is loopback with no token. A non-loopback `bind` is a share: it always requires a token,
/// and a caller that omits one gets a generated 128-bit session token reported by
/// [`StudioHandle::share_url`].
#[derive(Debug, Clone, Default)]
pub struct ShareOptions {
    /// The interface to bind. `None` keeps `127.0.0.1`.
    pub bind: Option<IpAddr>,
    /// A token to require. `None` generates one for the session. Shorter than 16 characters is
    /// refused rather than accepted as weak.
    pub token: Option<String>,
    /// The host (an IP or a name) a colleague should use. `None` falls back to the bound address.
    pub host: Option<String>,
}

/// A running studio. Dropping it stops the listener and joins the accept thread.
pub struct StudioHandle {
    addr: SocketAddr,
    /// Present when the session is shared: the token every API call must present.
    token: Option<String>,
    /// The host to advertise in the share link.
    host: String,
    shutdown: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl StudioHandle {
    /// The address the studio is listening on.
    pub fn addr(&self) -> SocketAddr {
        self.addr
    }

    /// The URL to open in a browser on this machine.
    ///
    /// A shared studio includes its token, because the page cannot call the API without it.
    pub fn url(&self) -> String {
        let host = match self.addr.ip() {
            IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
            IpAddr::V6(v6) if v6.is_unspecified() => "127.0.0.1".to_string(),
            ip => ip.to_string(),
        };
        match &self.token {
            Some(token) => format!("http://{host}:{}/?token={token}", self.addr.port()),
            None => format!("http://{host}:{}", self.addr.port()),
        }
    }

    /// The link to send a colleague, or `None` when the studio is loopback-only.
    ///
    /// Anyone with this link can read **and edit** the open document until the studio stops.
    pub fn share_url(&self) -> Option<String> {
        let token = self.token.as_ref()?;
        Some(format!(
            "http://{}:{}/?token={token}",
            self.host,
            self.addr.port()
        ))
    }

    /// The session token, when shared. Printed once at startup and never logged.
    pub fn token(&self) -> Option<&str> {
        self.token.as_deref()
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
    start_with_options(project, path, port, recovered, ShareOptions::default())
}

/// Start the studio with an explicit exposure policy.
///
/// Loopback (the default) needs no token. Any other bind is a shared session and always requires
/// one: `options.token`, or a generated 128-bit token reported by [`StudioHandle::share_url`].
/// Binding happens before the accept loop, and the project is validated before the port opens.
pub fn start_with_options(
    project: Project,
    path: PathBuf,
    port: u16,
    recovered: Option<RecoveryInfo>,
    options: ShareOptions,
) -> Result<StudioHandle, StudioError> {
    let mut state = StudioState::new(project, path, recovered)?;
    let bind = options.bind.unwrap_or(IpAddr::V4(Ipv4Addr::LOCALHOST));
    let shared = !bind.is_loopback();
    let token = if shared {
        let token = options.token.unwrap_or_else(new_share_token);
        if token.len() < 16 {
            return Err(StudioError::BadRequest(
                "a share token must be at least 16 characters".into(),
            ));
        }
        Some(token)
    } else {
        None
    };

    let listener = TcpListener::bind((bind, port)).map_err(StudioError::Bind)?;
    let addr = listener.local_addr().map_err(StudioError::Bind)?;
    let host = options.host.unwrap_or_else(|| advertise_host(bind));
    if let Some(token) = &token {
        state.share = Some(ShareAccess {
            token: token.clone(),
            url: format!("http://{host}:{}/?token={token}", addr.port()),
        });
    }
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
        token,
        host,
        shutdown,
        thread: Some(thread),
    })
}

/// The host to advertise when the caller did not name one.
fn advertise_host(bind: IpAddr) -> String {
    match bind {
        IpAddr::V4(v4) if v4.is_unspecified() => "127.0.0.1".to_string(),
        IpAddr::V6(v6) if v6.is_unspecified() => "127.0.0.1".to_string(),
        other => other.to_string(),
    }
}

/// A 128-bit session token.
///
/// `RandomState` is seeded from the operating system's entropy, so two hashes of distinct inputs
/// give independent 64-bit outputs without pulling a random-number crate into the trust boundary.
fn new_share_token() -> String {
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};
    let state = RandomState::new();
    let mut token = String::with_capacity(32);
    for round in 0u8..2 {
        let mut hasher = state.build_hasher();
        hasher.write_u8(round);
        hasher.write_u32(std::process::id());
        hasher.write_u128(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|elapsed| elapsed.as_nanos())
                .unwrap_or(0),
        );
        token.push_str(&format!("{:016x}", hasher.finish()));
    }
    token
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
    let share = match state.lock() {
        Ok(state) => state.share.clone(),
        Err(_) => return error_response(&StudioError::Lock),
    };
    let shared = share.is_some();
    // A browser resolving an attacker-controlled name to 127.0.0.1 still sends that name in `Host`.
    // On loopback, refusing anything but the studio's own host is what stops DNS-rebinding reads.
    // A shared studio is protected by its token instead, so it accepts the address it was reached on.
    if let Err(error) = require_host(request, port, shared) {
        return error_response(&error);
    }
    let path = path_only(&request.path);
    // The page itself, its CSS and its script carry no document data. Everything under /api/ does,
    // so a shared studio requires the token there and nowhere else — otherwise the browser could not
    // even load the page that is allowed to present the token.
    if let Some(share) = &share
        && path.starts_with("/api/")
        && let Err(error) = require_share_token(request, share)
    {
        return error_response(&error);
    }
    match (request.method.as_str(), path) {
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
        ("GET", "/api/draft") => match with_state(state, |state| state.draft_payload()) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/export/pdf") => {
            let query = request.path.clone();
            match with_state(state, |state| {
                // The sheet's title defaults to the project's name, which only the state knows.
                let title =
                    query_text(&query, "title").unwrap_or_else(|| state.project.name.clone());
                let sheet = sheet_from_query(&query, title)?;
                state.pdf(sheet)
            }) {
                Ok((bytes, filename)) => http::Response::new(200, "OK", "application/pdf", bytes)
                    .with_header(
                        "Content-Disposition",
                        format!("attachment; filename=\"{filename}\""),
                    ),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/export/dxf") => match with_state(state, |state| state.dxf()) {
            Ok((bytes, filename)) => http::Response::new(200, "OK", "application/dxf", bytes)
                .with_header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{filename}\""),
                ),
            Err(error) => error_response(&error),
        },
        ("POST", path) if path.starts_with("/api/") => {
            match require_same_origin(request, port, shared).and_then(|()| require_json(request)) {
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
    match path_only(&request.path) {
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
                    // One placement request covers the XY origin and, when present, the plane height,
                    // so a single click is a single undo step.
                    state.mutate(|project| match body.plane {
                        Some(plane_z) => project.set_sketch_placement(id, body.origin, plane_z),
                        None => project.set_sketch_origin(id, body.origin),
                    })
                }) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/sketch" => match parse_body::<NewSketchRequest>(request) {
            Ok(body) => {
                let profile = match profile_from_parts(
                    &body.kind,
                    body.sides,
                    body.points,
                    body.start_degrees,
                    body.sweep_degrees,
                    None,
                ) {
                    Ok(profile) => profile,
                    Err(error) => return error_response(&error),
                };
                let origin = body.origin.unwrap_or([0.0, 0.0]);
                let plane_z = body.plane.unwrap_or(0.0);
                match with_state(state, |state| {
                    state.mutate(|project| {
                        project
                            .add_sketch_on_plane(profile, origin, plane_z)
                            .map(|_| ())
                    })
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
                let direction = match direction_for(&body.direction) {
                    Ok(direction) => direction,
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
                            .add_extrude_feature_in_direction(
                                sketch_id,
                                &body.depth_param,
                                operation,
                                direction,
                            )
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
        "/api/draft/entity" => match parse_body::<DrawingEntityRequest>(request) {
            Ok(body) => match serde_json::from_value::<cad_drafting::EntityKind>(body.kind) {
                Ok(kind) => match with_state(state, |state| state.drawing_add(&body.layer, kind)) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&StudioError::BadRequest(format!(
                    "the entity is not a drawing entity: {error}"
                ))),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/remove" => match parse_body::<DrawingRemoveRequest>(request) {
            Ok(body) => match EntityId::new(body.id) {
                Ok(id) => match with_state(state, |state| state.drawing_remove(id)) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&StudioError::BadRequest(error.to_string())),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/layer" => match parse_body::<DrawingLayerRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_layer(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/undo" => match with_state(state, StudioState::drawing_undo) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/draft/redo" => match with_state(state, StudioState::drawing_redo) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/draft/save" => match with_state(state, StudioState::drawing_save) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/revision" => match parse_body::<RevisionRequest>(request) {
            Ok(body) => match with_state(state, |state| {
                state.mutate(|project| project.add_revision(&body.name).map(|_| ()))
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/revision/restore" => match parse_body::<RevisionIdRequest>(request) {
            Ok(body) => match with_state(state, |state| {
                state.mutate(|project| project.restore_revision(body.id))
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/revision/remove" => match parse_body::<RevisionIdRequest>(request) {
            Ok(body) => match with_state(state, |state| {
                state.mutate(|project| project.remove_revision(body.id))
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
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
    /// For a sector: where its arc starts, in degrees counter-clockwise from +X. Absent keeps the
    /// angles the sketch already has (or 0 for a new sector).
    #[serde(default)]
    start_degrees: Option<f64>,
    /// For a sector: how far its arc sweeps, in degrees. Must be between 0 and 360 exclusive.
    #[serde(default)]
    sweep_degrees: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct OriginRequest {
    id: u64,
    origin: [f64; 2],
    /// Plane height, in document units. Absent keeps the sketch on the plane it already has, which
    /// is what an older client sends.
    #[serde(default)]
    plane: Option<f64>,
}

/// An entity the user drew in the Drawing panel. `kind` is the drawing document's own tagged form, so
/// the server validates it with the same code that guards a loaded file.
#[derive(Debug, Deserialize)]
struct DrawingEntityRequest {
    layer: String,
    kind: serde_json::Value,
}

#[derive(Debug, Deserialize)]
struct DrawingRemoveRequest {
    id: u64,
}

#[derive(Debug, Deserialize)]
struct DrawingLayerRequest {
    name: String,
    #[serde(default)]
    color: Option<u8>,
    #[serde(default)]
    visible: Option<bool>,
    #[serde(default)]
    locked: Option<bool>,
}

/// A name for a new named revision.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionRequest {
    name: String,
}

/// The revision a restore or a delete applies to.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct RevisionIdRequest {
    id: u64,
}

/// A new sketch. The profile is driven by the document's default parameters (`width`/`height`, or
/// `diameter`), which is why the shape is all this request needs to carry.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewSketchRequest {
    kind: String,
    #[serde(default)]
    origin: Option<[f64; 2]>,
    /// Plane height, in document units. Defaults to 0, the world plane.
    #[serde(default)]
    plane: Option<f64>,
    #[serde(default)]
    sides: Option<u32>,
    /// A drawn loop, in document units, relative to `origin`. Required for `polyline`.
    #[serde(default)]
    points: Option<Vec<[f64; 2]>>,
    /// For a sector: where its arc starts, in degrees. Defaults to 0.
    #[serde(default)]
    start_degrees: Option<f64>,
    /// For a sector: how far its arc sweeps, in degrees. Defaults to 90.
    #[serde(default)]
    sweep_degrees: Option<f64>,
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
    /// `up` or `down`. Defaults to `up`, which is what a schema-7 extrude did.
    #[serde(default = "default_direction")]
    direction: String,
}

fn default_depth_param() -> String {
    "depth".to_string()
}

fn default_direction() -> String {
    "up".to_string()
}

fn direction_for(name: &str) -> Result<ExtrudeDirection, StudioError> {
    match name.trim().to_ascii_lowercase().as_str() {
        "up" | "+z" | "+1" => Ok(ExtrudeDirection::Up),
        "down" | "-z" | "-1" => Ok(ExtrudeDirection::Down),
        other => Err(StudioError::BadRequest(format!(
            "unknown extrude direction `{other}`; expected up or down"
        ))),
    }
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
        request.start_degrees,
        request.sweep_degrees,
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
    start_degrees: Option<f64>,
    sweep_degrees: Option<f64>,
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
        "sector" | "arc" | "pie" => {
            // An angle omitted by the request keeps what the sketch already has, so replacing a
            // rectangle with a sector and then nudging the sweep does not reset the start angle.
            let (start, sweep) = match current {
                Some(SketchProfile::Sector {
                    start_degrees,
                    sweep_degrees,
                    ..
                }) => (*start_degrees, *sweep_degrees),
                _ => (0.0, 90.0),
            };
            Ok(SketchProfile::Sector {
                diameter_param: "diameter".to_string(),
                start_degrees: start_degrees.unwrap_or(start),
                sweep_degrees: sweep_degrees.unwrap_or(sweep),
            })
        }
        other => Err(StudioError::BadRequest(format!(
            "unknown profile `{other}`; expected rectangle, circle, regular_polygon, polyline, or \
             sector"
        ))),
    }
}

fn path_only(path: &str) -> &str {
    path.split('?').next().unwrap_or(path)
}

/// A query parameter, unescaped only in the sense this server needs: share tokens are hex.
fn query_param<'a>(path: &'a str, name: &str) -> Option<&'a str> {
    let (_, query) = path.split_once('?')?;
    query.split('&').find_map(|pair| {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        (key == name && !value.is_empty()).then_some(value)
    })
}

/// A query parameter decoded for a title block: `%20` and `+` become spaces, `%XX` a byte.
fn query_text(path: &str, name: &str) -> Option<String> {
    let raw = query_param(path, name)?;
    let bytes = raw.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'%' if index + 3 <= bytes.len() => {
                let hex = std::str::from_utf8(&bytes[index + 1..index + 3]).ok();
                match hex.and_then(|digits| u8::from_str_radix(digits, 16).ok()) {
                    Some(value) => {
                        out.push(value);
                        index += 3;
                    }
                    None => {
                        out.push(b'%');
                        index += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    Some(String::from_utf8_lossy(&out).into_owned())
}

/// The sheet named by the query string of `/api/export/pdf`.
///
/// Every field is optional and defaults to the sheet in `cad_drafting`: A3 landscape at 1:1 with the
/// project's own name. `scale=fit` asks for a standard scale chosen to fill the page, which is what a
/// user who does not know the model's size needs; `scale=2` is 1:2.
fn sheet_from_query(path: &str, title: String) -> Result<cad_drafting::Sheet, StudioError> {
    let mut sheet = cad_drafting::Sheet::new(title);
    if let Some(paper) = query_text(path, "paper") {
        sheet.paper = cad_drafting::PaperSize::parse(&paper).ok_or_else(|| {
            StudioError::BadRequest(format!(
                "`{paper}` is not a paper size; use A4, A3, A2, A1, A0 or Letter"
            ))
        })?;
    }
    if let Some(orientation) = query_text(path, "orientation") {
        sheet.orientation = cad_drafting::Orientation::parse(&orientation).ok_or_else(|| {
            StudioError::BadRequest(format!(
                "`{orientation}` is not an orientation; use portrait or landscape"
            ))
        })?;
    }
    match query_text(path, "scale") {
        Some(value) if value.eq_ignore_ascii_case("fit") => sheet.scale = None,
        Some(value) => {
            let scale: f64 = value.parse().map_err(|_| {
                StudioError::BadRequest(format!(
                    "scale `{value}` is not a number; use 1 for 1:1, 2 for 1:2, or `fit`"
                ))
            })?;
            if !scale.is_finite() || scale <= 0.0 {
                return Err(StudioError::BadRequest(
                    "the sheet scale must be a positive number".into(),
                ));
            }
            sheet.scale = Some(scale);
        }
        None => {}
    }
    for (name, field) in [
        ("number", &mut sheet.number),
        ("revision", &mut sheet.revision),
        ("author", &mut sheet.author),
        ("date", &mut sheet.date),
        ("notes", &mut sheet.notes),
    ] {
        if let Some(value) = query_text(path, name) {
            *field = value;
        }
    }
    if let Some(margin) = query_text(path, "margin") {
        sheet.margin_mm = margin.parse().map_err(|_| {
            StudioError::BadRequest(format!("margin `{margin}` is not a number of millimetres"))
        })?;
    }
    if let Some(dimensions) = query_text(path, "dimensions") {
        sheet.dimensions = !matches!(dimensions.as_str(), "0" | "false" | "no");
    }
    sheet.validate()?;
    Ok(sheet)
}

/// Compare without leaking the position of the first mismatch through an early return.
fn tokens_match(presented: &str, expected: &str) -> bool {
    let (presented, expected) = (presented.as_bytes(), expected.as_bytes());
    if presented.len() != expected.len() {
        return false;
    }
    let mut difference = 0u8;
    for (left, right) in presented.iter().zip(expected) {
        difference |= left ^ right;
    }
    difference == 0
}

fn require_share_token(request: &http::Request, share: &ShareAccess) -> Result<(), StudioError> {
    let presented = request
        .header("x-atlas-token")
        .map(str::trim)
        .filter(|token| !token.is_empty())
        .or_else(|| query_param(&request.path, "token"))
        .ok_or(StudioError::Unauthorized)?;
    if tokens_match(presented, &share.token) {
        Ok(())
    } else {
        Err(StudioError::Unauthorized)
    }
}

fn require_host(request: &http::Request, port: u16, shared: bool) -> Result<(), StudioError> {
    let host = request
        .header("host")
        .ok_or(StudioError::Forbidden)?
        .trim()
        .to_ascii_lowercase();
    if shared {
        // The address a colleague's browser used is unknown to the server; the port is not.
        return if host.ends_with(&format!(":{port}")) {
            Ok(())
        } else {
            Err(StudioError::Forbidden)
        };
    }
    let allowed = [format!("127.0.0.1:{port}"), format!("localhost:{port}")];
    if allowed.iter().any(|candidate| candidate == &host) {
        Ok(())
    } else {
        Err(StudioError::Forbidden)
    }
}

fn require_same_origin(
    request: &http::Request,
    port: u16,
    shared: bool,
) -> Result<(), StudioError> {
    let origin = request
        .header("origin")
        .ok_or(StudioError::Forbidden)?
        .trim()
        .to_ascii_lowercase();
    if shared {
        // The page was reached at some address:port; a form post must come from that same origin.
        let host = request
            .header("host")
            .ok_or(StudioError::Forbidden)?
            .trim()
            .to_ascii_lowercase();
        return if origin == format!("http://{host}") {
            Ok(())
        } else {
            Err(StudioError::Forbidden)
        };
    }
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
    // A share link carries its token in the query string; never hand that URL to another origin.
    http::Response::new(200, "OK", content_type, body.as_bytes().to_vec())
        .with_header("Referrer-Policy", "no-referrer")
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

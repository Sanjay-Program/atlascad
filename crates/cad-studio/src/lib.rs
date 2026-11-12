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
use cad_contract::document::Revision;
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::kernel::Kernel;
use cad_document::{
    BuildCache, ExtrudeDirection, ExtrudeOperation, FeatureFailure, FeatureKind, FeatureOutcome,
    FeatureReport, Project, ProjectError, RecoveryInfo, RecoverySource, RevisionSummary,
    RevolveAxis, SketchProfile, clear_recovery,
};
use cad_drafting::Draft;
use cad_engineering_model::{
    ArtifactAssessment, ArtifactId, ArtifactKind, ArtifactRegistry, DerivedArtifact, DocumentRef,
    Material, MaterialLibrary,
};
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
    /// The engineering-model registry could not be built or assessed.
    Model(cad_engineering_model::ModelError),
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
            StudioError::Model(error) => write!(f, "engineering model: {error}"),
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

impl From<cad_engineering_model::ModelError> for StudioError {
    fn from(error: cad_engineering_model::ModelError) -> Self {
        StudioError::Model(error)
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
                ProjectError::Io { .. } | ProjectError::Engineering(_) => 500,
            },
            // A plan can only fail because the project it describes is invalid, which the same
            // request would report as a 400 for any other endpoint.
            StudioError::Drafting(_) | StudioError::DrawingDoc(_) | StudioError::Sheet(_) => 400,
            StudioError::Model(_)
            | StudioError::Stl(_)
            | StudioError::Io(_)
            | StudioError::Lock
            | StudioError::Bind(_) => 500,
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
            StudioError::Project(ProjectError::Engineering(_)) => "engineering_model_failure",
            StudioError::Project(_) => "invalid_project",
            StudioError::Kernel(_) => "kernel_failure",
            StudioError::Stl(_) => "export_failure",
            StudioError::Drafting(_) | StudioError::DrawingDoc(_) => "drawing_failure",
            StudioError::Sheet(_) => "sheet_failure",
            StudioError::Model(_) => "engineering_model_failure",
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
    #[serde(default)]
    blocks: Vec<cad_drafting::Block>,
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
    /// The model revision this drawing was generated from, when that is known. A drawing loaded
    /// from a `.zdraft` file does not record its source revision, so it is `None` rather than a
    /// guess.
    derived_from_revision: Option<u64>,
    /// `current`, `requires_review` or `unknown`.
    derivation_status: &'static str,
}

/// The one derived artifact the studio currently produces, assessed against the live revision.
#[derive(Debug, Serialize)]
struct DrawingEngineeringView {
    kind: &'static str,
    derived_from_revision: Option<u64>,
    status: &'static str,
    /// Present when the derivation revision is known; built by [`ArtifactRegistry::assess`].
    assessment: Option<ArtifactAssessment>,
}

/// Document identity, revision and the materials a study may assign.
#[derive(Debug, Serialize)]
struct EngineeringView {
    document: u64,
    revision: u64,
    unit: String,
    drawing: DrawingEngineeringView,
    /// Typical handbook values, not a certified material specification.
    materials: Vec<Material>,
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
    /// The project revision the drawing was generated from. `None` when the drawing came from a
    /// `.zdraft` file, which does not carry its source revision, or before one exists.
    drawing_revision: Option<u64>,
    /// Set when the embedded artifact registry changed without a design revision bump. Recording a
    /// result is not a design edit, so it must not move the revision, but the project file still
    /// differs from disk and has to be marked unsaved.
    artifacts_changed: bool,
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
            drawing_revision: None,
            artifacts_changed: false,
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
            dirty: self.recovered.is_some()
                || self.project.revision.get() != self.saved_revision
                || self.artifacts_changed,
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
        self.artifacts_changed = false;
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
        self.artifacts_changed = false;
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
            (Some(_), None, false) => self.drawing_revision != Some(revision),
            _ => false,
        };
        if !stale {
            return Ok(());
        }
        let generated = if let Some(path) = self.drawing_file.clone() {
            self.drawing = Some(Draft::load(&path)?);
            false
        } else {
            let sidecar = self.drawing_path();
            if sidecar.is_file() && !self.drawing_edited {
                self.drawing = Some(Draft::load(&sidecar)?);
                self.drawing_file = Some(sidecar);
                false
            } else {
                self.drawing = Some(cad_drafting::plan_draft(&self.project)?);
                self.drawing_file = None;
                true
            }
        };
        // A `.zdraft` file does not record the model revision it came from. The project's embedded
        // registry may still pin it, which is how provenance survives a restart; without a record
        // the status stays unknown rather than claiming the drawing is current.
        self.drawing_revision = if generated {
            Some(revision)
        } else {
            self.project.artifacts().and_then(|registry| {
                registry
                    .artifacts()
                    .iter()
                    .find(|artifact| artifact.kind == ArtifactKind::Drawing)
                    .map(|artifact| artifact.source.revision.get())
            })
        };
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
        let current = self.project.revision.get();
        let derivation_status = match self.drawing_revision {
            None => "unknown",
            Some(revision) if revision == current => "current",
            Some(_) => "requires_review",
        };
        Ok(DraftView {
            name: draft.name.clone(),
            unit: draft.header.units.symbol().to_string(),
            layers: draft.layers().to_vec(),
            entities: draft.entities().to_vec(),
            blocks: draft.blocks().to_vec(),
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
            derived_from_revision: self.drawing_revision,
            derivation_status,
        })
    }

    /// Document identity, the live revision, the built-in materials and the drawing's derivation
    /// status. The drawing is assessed through the same registry the CLI uses, so the application and
    /// the release gate cannot disagree about whether a derived result belongs to this revision.
    fn engineering_payload(&mut self) -> Result<EngineeringView, StudioError> {
        self.ensure_drawing()?;
        let document = self.project.header.document;
        let revision = self.project.revision;
        let current = DocumentRef::new(document, revision, None);
        // A saved drawing is stored in the project's embedded registry, so it survives a restart and
        // is assessed against the live revision by the same code the CLI release gate uses. A
        // drawing that only exists in memory (a generated plan, never saved) has no record, and its
        // state is derived from the revision it was generated at.
        let recorded = self.project.artifacts().and_then(|registry| {
            registry
                .artifacts()
                .iter()
                .find(|artifact| artifact.kind == ArtifactKind::Drawing)
                .cloned()
        });
        let (derived_from, status, assessment) = match recorded {
            Some(artifact) => {
                let registry = self.project.artifacts().ok_or_else(|| {
                    StudioError::BadRequest("the artifact registry is unavailable".into())
                })?;
                let report = registry.assess(current)?;
                let assessment = report
                    .artifacts
                    .into_iter()
                    .find(|candidate| candidate.id == artifact.id);
                let status = assessment
                    .as_ref()
                    .map(|assessment| assessment.status.label())
                    .unwrap_or("unknown");
                (Some(artifact.source.revision.get()), status, assessment)
            }
            None => match self.drawing_revision {
                Some(source) => {
                    let source_revision = Revision::from_raw(source).ok_or_else(|| {
                        StudioError::BadRequest("the drawing records a zero source revision".into())
                    })?;
                    let id = ArtifactId::new(1)
                        .ok_or_else(|| StudioError::BadRequest("invalid artifact id".into()))?;
                    let mut registry = ArtifactRegistry::new(document);
                    registry.add(DerivedArtifact::new(
                        id,
                        ArtifactKind::Drawing,
                        "drawing",
                        DocumentRef::new(document, source_revision, None),
                    ))?;
                    let report = registry.assess(current)?;
                    let assessment = report.artifacts.into_iter().next();
                    let status = assessment
                        .as_ref()
                        .map(|assessment| assessment.status.label())
                        .unwrap_or("unknown");
                    (Some(source), status, assessment)
                }
                None => (None, "unknown", None),
            },
        };
        Ok(EngineeringView {
            document: document.get(),
            revision: revision.get(),
            unit: self.project.header.units.symbol().to_string(),
            drawing: DrawingEngineeringView {
                kind: ArtifactKind::Drawing.label(),
                derived_from_revision: derived_from,
                status,
                assessment,
            },
            materials: MaterialLibrary::standard().materials().to_vec(),
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
            if let Some(linetype) = request.linetype
                && draft
                    .layer(&request.name)
                    .is_some_and(|layer| layer.linetype != linetype)
            {
                draft.set_layer_linetype(&request.name, linetype)?;
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
        // Saving pins the drawing to the model revision it now represents.
        self.drawing_revision = Some(self.project.revision.get());
        self.record_drawing_artifact()?;
        self.draft_payload()
    }

    fn drawing_transform(&mut self, req: &DraftTransformRequest) -> Result<DraftView, StudioError> {
        let ids: Result<Vec<EntityId>, _> = req
            .ids
            .iter()
            .map(|&id| EntityId::new(id).map_err(|e| StudioError::BadRequest(e.to_string())))
            .collect();
        let entity_ids = ids?;
        {
            let draft = self.drawing_mut()?;
            match req.op.as_str() {
                "translate" => {
                    let dx = req.dx.unwrap_or(0.0);
                    let dy = req.dy.unwrap_or(0.0);
                    draft.translate_entities(&entity_ids, dx, dy)?;
                }
                "copy" => {
                    let dx = req.dx.unwrap_or(0.0);
                    let dy = req.dy.unwrap_or(0.0);
                    draft.copy_entities(&entity_ids, dx, dy)?;
                }
                "rotate" => {
                    let center = req.center.unwrap_or([0.0, 0.0]);
                    let rad = req.angle_deg.unwrap_or(0.0).to_radians();
                    draft.rotate_entities(&entity_ids, center, rad)?;
                }
                "scale" => {
                    let base = req.base.unwrap_or([0.0, 0.0]);
                    let factor = req.factor.unwrap_or(1.0);
                    draft.scale_entities(&entity_ids, base, factor)?;
                }
                "mirror" => {
                    let p1 = req.p1.unwrap_or([0.0, 0.0]);
                    let p2 = req.p2.unwrap_or([1.0, 0.0]);
                    let erase = req.erase_source.unwrap_or(false);
                    draft.mirror_entities(&entity_ids, p1, p2, erase)?;
                }
                other => {
                    return Err(StudioError::BadRequest(format!(
                        "unknown transform operation `{other}`"
                    )));
                }
            }
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_offset(&mut self, req: &DraftOffsetRequest) -> Result<DraftView, StudioError> {
        let id = EntityId::new(req.id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let side = req.side.unwrap_or(1.0);
        {
            let draft = self.drawing_mut()?;
            draft.offset_entity(id, req.distance, side)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_fillet(&mut self, req: &DraftFilletRequest) -> Result<DraftView, StudioError> {
        let id1 = EntityId::new(req.id1).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let id2 = EntityId::new(req.id2).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        {
            let draft = self.drawing_mut()?;
            draft.fillet(id1, id2, req.radius)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_chamfer(&mut self, req: &DraftChamferRequest) -> Result<DraftView, StudioError> {
        let id1 = EntityId::new(req.id1).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let id2 = EntityId::new(req.id2).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        {
            let draft = self.drawing_mut()?;
            draft.chamfer(id1, id2, req.d1, req.d2)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_explode(&mut self, req: &DraftExplodeRequest) -> Result<DraftView, StudioError> {
        let id = EntityId::new(req.id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        {
            let draft = self.drawing_mut()?;
            draft.explode_entity(id)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_measure(&self, req: &DraftMeasureRequest) -> Result<serde_json::Value, StudioError> {
        match req.mode.as_str() {
            "distance" => {
                let p1 = req.p1.ok_or_else(|| {
                    StudioError::BadRequest("distance inquiry requires p1".into())
                })?;
                let p2 = req.p2.ok_or_else(|| {
                    StudioError::BadRequest("distance inquiry requires p2".into())
                })?;
                let inq = cad_drafting::measure_distance(p1, p2)?;
                serde_json::to_value(inq).map_err(|e| StudioError::Json(e.to_string()))
            }
            "area" => {
                let pts = req.points.as_deref().ok_or_else(|| {
                    StudioError::BadRequest("area inquiry requires polygon points".into())
                })?;
                let inq = cad_drafting::measure_polygon_area(pts)?;
                serde_json::to_value(inq).map_err(|e| StudioError::Json(e.to_string()))
            }
            other => Err(StudioError::BadRequest(format!(
                "unknown measure mode `{other}`"
            ))),
        }
    }

    fn drawing_to_solid(&mut self, req: &DraftToSolidRequest) -> Result<StatePayload, StudioError> {
        let ent_id =
            EntityId::new(req.entity_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let (profile, origin) = {
            let draft = self.drawing_mut()?;
            let ent = draft.entity(ent_id).ok_or_else(|| {
                StudioError::BadRequest(format!("unknown entity {}", req.entity_id))
            })?;
            match &ent.kind {
                cad_drafting::EntityKind::Rectangle {
                    origin,
                    width,
                    height,
                } => {
                    let w = *width;
                    let h = *height;
                    let orig = *origin;
                    let poly = cad_document::SketchProfile::Polyline {
                        points: vec![[0.0, 0.0], [w, 0.0], [w, h], [0.0, h]],
                    };
                    (poly, orig)
                }
                cad_drafting::EntityKind::Circle { center, radius } => {
                    let r = *radius;
                    let c = *center;
                    let mut pts = Vec::with_capacity(32);
                    for i in 0..32 {
                        let th = (i as f64) * std::f64::consts::TAU / 32.0;
                        pts.push([r * th.cos(), r * th.sin()]);
                    }
                    (cad_document::SketchProfile::Polyline { points: pts }, c)
                }
                cad_drafting::EntityKind::Polyline { points, closed } => {
                    if !closed || points.len() < 3 {
                        return Err(StudioError::BadRequest(
                            "to extrude as solid, polyline must be closed with at least 3 points"
                                .into(),
                        ));
                    }
                    let orig = points[0];
                    let rel_pts: Vec<[f64; 2]> = points
                        .iter()
                        .map(|p| [p[0] - orig[0], p[1] - orig[1]])
                        .collect();
                    (
                        cad_document::SketchProfile::Polyline { points: rel_pts },
                        orig,
                    )
                }
                other => {
                    return Err(StudioError::BadRequest(format!(
                        "entity of kind `{}` cannot be extruded to a 3D solid (only closed shapes: rectangle, circle, closed polyline)",
                        other.label()
                    )));
                }
            }
        };

        let depth = if req.depth.is_finite() && req.depth > 0.0 {
            req.depth
        } else {
            10.0
        };

        let operation = match req.operation.as_deref() {
            Some("join") => ExtrudeOperation::Join,
            Some("cut") => ExtrudeOperation::Cut,
            Some("new_body") => ExtrudeOperation::NewBody,
            Some(other) => {
                return Err(StudioError::BadRequest(format!(
                    "unknown operation `{other}`"
                )));
            }
            None => {
                if self.project.features().is_empty() {
                    ExtrudeOperation::NewBody
                } else {
                    ExtrudeOperation::Join
                }
            }
        };

        self.mutate(|project| {
            let param_id = format!("extr_{}", project.features().len() + 1);
            if project.parameter(&param_id).is_err() {
                project.add_parameter(param_id.clone(), depth)?;
            } else {
                project.set_parameter(&param_id, depth)?;
            }
            let sketch_id = project.add_sketch_on_plane(profile, origin, 0.0)?;
            project.add_extrude_feature_in_direction(
                sketch_id,
                &param_id,
                operation,
                ExtrudeDirection::Up,
            )?;
            Ok(())
        })
    }

    fn drawing_hatch(&mut self, req: &DraftHatchRequest) -> Result<DraftView, StudioError> {
        let pattern = req.pattern.as_deref().unwrap_or("ANSI31");
        let angle = req.angle_deg.unwrap_or(45.0);
        let spacing = req.spacing.unwrap_or(3.0);
        let layer_name = req.layer.as_deref().unwrap_or("HATCH");

        let points = if let Some(ent_id_num) = req.entity_id {
            let ent_id =
                EntityId::new(ent_id_num).map_err(|e| StudioError::BadRequest(e.to_string()))?;
            let draft = self.drawing_mut()?;
            let ent = draft
                .entity(ent_id)
                .ok_or_else(|| StudioError::BadRequest(format!("unknown entity {ent_id_num}")))?;
            match &ent.kind {
                cad_drafting::EntityKind::Rectangle {
                    origin,
                    width,
                    height,
                } => {
                    let o = *origin;
                    let w = *width;
                    let h = *height;
                    vec![o, [o[0] + w, o[1]], [o[0] + w, o[1] + h], [o[0], o[1] + h]]
                }
                cad_drafting::EntityKind::Polyline { points, closed } => {
                    if !closed || points.len() < 3 {
                        return Err(StudioError::BadRequest(
                            "polyline must be closed to hatch".into(),
                        ));
                    }
                    points.clone()
                }
                other => {
                    return Err(StudioError::BadRequest(format!(
                        "cannot hatch entity of kind `{}` (requires closed rectangle or polyline)",
                        other.label()
                    )));
                }
            }
        } else if let Some(ref pts) = req.points {
            if pts.len() < 3 {
                return Err(StudioError::BadRequest(
                    "hatch boundary requires at least 3 points".into(),
                ));
            }
            pts.clone()
        } else {
            return Err(StudioError::BadRequest(
                "hatch requires either entity_id or points".into(),
            ));
        };

        {
            let draft = self.drawing_mut()?;
            if draft.layer(layer_name).is_none() {
                draft.add_layer(layer_name, 2)?;
            }
            draft.add_hatch(layer_name, &points, pattern, angle, spacing)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_multiview(&mut self) -> Result<DraftView, StudioError> {
        let draft = cad_drafting::multiview_draft(&self.project)?;
        self.drawing = Some(draft);
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_join(&mut self, req: &DraftJoinRequest) -> Result<DraftView, StudioError> {
        let ids: Result<Vec<EntityId>, _> = req
            .ids
            .iter()
            .map(|&id| EntityId::new(id).map_err(|e| StudioError::BadRequest(e.to_string())))
            .collect();
        let entity_ids = ids?;
        let tol = req.tolerance.unwrap_or(1e-4);
        {
            let draft = self.drawing_mut()?;
            draft.join_entities(&entity_ids, tol)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_array(&mut self, req: &DraftArrayRequest) -> Result<DraftView, StudioError> {
        let ids: Result<Vec<EntityId>, _> = req
            .ids
            .iter()
            .map(|&id| EntityId::new(id).map_err(|e| StudioError::BadRequest(e.to_string())))
            .collect();
        let entity_ids = ids?;
        {
            let draft = self.drawing_mut()?;
            match req.mode.as_str() {
                "rectangular" => {
                    let rows = req.rows.unwrap_or(1);
                    let cols = req.cols.unwrap_or(1);
                    let row_spacing = req.row_spacing.unwrap_or(0.0);
                    let col_spacing = req.col_spacing.unwrap_or(0.0);
                    draft.rectangular_array(&entity_ids, rows, cols, row_spacing, col_spacing)?;
                }
                "polar" => {
                    let count = req.count.unwrap_or(2);
                    let center = req.center.unwrap_or([0.0, 0.0]);
                    let fill_angle = req.fill_angle_deg.unwrap_or(360.0);
                    let rotate = req.rotate_items.unwrap_or(true);
                    draft.polar_array(&entity_ids, count, center, fill_angle, rotate)?;
                }
                other => {
                    return Err(StudioError::BadRequest(format!(
                        "unknown array mode `{other}` (must be `rectangular` or `polar`)"
                    )));
                }
            }
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_construct(&mut self, req: &DraftConstructRequest) -> Result<DraftView, StudioError> {
        let layer_name = req.layer.as_deref().unwrap_or(cad_drafting::DEFAULT_LAYER);
        {
            let draft = self.drawing_mut()?;
            if draft.layer(layer_name).is_none() {
                draft.add_layer(layer_name, 7)?;
            }
            match req.kind.as_str() {
                "circle_2p" => {
                    let p1 = req
                        .p1
                        .ok_or_else(|| StudioError::BadRequest("circle_2p requires p1".into()))?;
                    let p2 = req
                        .p2
                        .ok_or_else(|| StudioError::BadRequest("circle_2p requires p2".into()))?;
                    draft.add_circle_2p(layer_name, p1, p2)?;
                }
                "circle_3p" => {
                    let p1 = req
                        .p1
                        .ok_or_else(|| StudioError::BadRequest("circle_3p requires p1".into()))?;
                    let p2 = req
                        .p2
                        .ok_or_else(|| StudioError::BadRequest("circle_3p requires p2".into()))?;
                    let p3 = req
                        .p3
                        .ok_or_else(|| StudioError::BadRequest("circle_3p requires p3".into()))?;
                    draft.add_circle_3p(layer_name, p1, p2, p3)?;
                }
                "arc_3p" => {
                    let p1 = req
                        .p1
                        .ok_or_else(|| StudioError::BadRequest("arc_3p requires p1".into()))?;
                    let p2 = req
                        .p2
                        .ok_or_else(|| StudioError::BadRequest("arc_3p requires p2".into()))?;
                    let p3 = req
                        .p3
                        .ok_or_else(|| StudioError::BadRequest("arc_3p requires p3".into()))?;
                    draft.add_arc_3p(layer_name, p1, p2, p3)?;
                }
                "polygon" => {
                    let center = req.center.unwrap_or([0.0, 0.0]);
                    let radius = req
                        .radius
                        .ok_or_else(|| StudioError::BadRequest("polygon requires radius".into()))?;
                    let sides = req.sides.unwrap_or(6);
                    let inscribed = req.inscribed.unwrap_or(true);
                    let start_angle = req.start_angle_deg.unwrap_or(0.0);
                    draft.add_polygon(layer_name, center, radius, sides, inscribed, start_angle)?;
                }
                other => {
                    return Err(StudioError::BadRequest(format!(
                        "unknown construction kind `{other}`"
                    )));
                }
            }
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_section(
        &mut self,
        req: &DraftSectionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let ent_id = EntityId::new(req.id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        self.ensure_drawing()?;
        let draft = self
            .drawing
            .as_ref()
            .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))?;
        let props = draft.inquire_section(ent_id)?;
        serde_json::to_value(props).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn drawing_trim(&mut self, req: &DraftTrimRequest) -> Result<DraftView, StudioError> {
        let target_id =
            EntityId::new(req.target_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let b_ids: Result<Vec<EntityId>, _> = req
            .boundary_ids
            .iter()
            .map(|&id| EntityId::new(id).map_err(|e| StudioError::BadRequest(e.to_string())))
            .collect();
        let boundary_ids = b_ids?;
        {
            let draft = self.drawing_mut()?;
            draft.trim(target_id, &boundary_ids, req.pick_point)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_extend(&mut self, req: &DraftExtendRequest) -> Result<DraftView, StudioError> {
        let target_id =
            EntityId::new(req.target_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let b_ids: Result<Vec<EntityId>, _> = req
            .boundary_ids
            .iter()
            .map(|&id| EntityId::new(id).map_err(|e| StudioError::BadRequest(e.to_string())))
            .collect();
        let boundary_ids = b_ids?;
        {
            let draft = self.drawing_mut()?;
            draft.extend(target_id, &boundary_ids, req.pick_point)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_to_cam(
        &mut self,
        req: &DraftToCamRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let ent_id =
            EntityId::new(req.entity_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        self.ensure_drawing()?;
        let draft = self
            .drawing
            .as_ref()
            .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))?;
        let ent = draft
            .entity(ent_id)
            .ok_or_else(|| StudioError::BadRequest(format!("unknown entity {}", req.entity_id)))?;

        let mut profile = match &ent.kind {
            cad_drafting::EntityKind::Rectangle {
                origin,
                width,
                height,
            } => {
                let [x, y] = origin;
                vec![
                    *origin,
                    [x + width, *y],
                    [x + width, y + height],
                    [*x, y + height],
                ]
            }
            cad_drafting::EntityKind::Polyline { points, closed } => {
                if !closed || points.len() < 3 {
                    return Err(StudioError::BadRequest(
                        "CAM profiling requires a closed polyline or rectangle".into(),
                    ));
                }
                points.clone()
            }
            cad_drafting::EntityKind::Circle { center, radius } => {
                let n = 32;
                let step = 2.0 * std::f64::consts::PI / (n as f64);
                let mut pts = Vec::with_capacity(n);
                for i in 0..n {
                    let ang = (i as f64) * step;
                    pts.push([
                        center[0] + radius * ang.cos(),
                        center[1] + radius * ang.sin(),
                    ]);
                }
                pts
            }
            other => {
                return Err(StudioError::BadRequest(format!(
                    "CAM profiling not supported for `{}` entity (requires Rectangle, Circle, or closed Polyline)",
                    other.label()
                )));
            }
        };

        let mut area = 0.0;
        let n = profile.len();
        for i in 0..n {
            let j = (i + 1) % n;
            area += profile[i][0] * profile[j][1] - profile[j][0] * profile[i][1];
        }
        if area < 0.0 {
            profile.reverse();
        }

        let mut min_x = f64::INFINITY;
        let mut max_x = f64::NEG_INFINITY;
        let mut min_y = f64::INFINITY;
        let mut max_y = f64::NEG_INFINITY;
        for p in &profile {
            if p[0] < min_x {
                min_x = p[0];
            }
            if p[0] > max_x {
                max_x = p[0];
            }
            if p[1] < min_y {
                min_y = p[1];
            }
            if p[1] > max_y {
                max_y = p[1];
            }
        }

        let depth_mm = req.depth_mm.unwrap_or(10.0);
        let stepdown_mm = req.stepdown_mm.unwrap_or(3.0);

        let margin = 10.0;
        let stock_min = [min_x - margin, min_y - margin, -depth_mm - 5.0];
        let stock_max = [max_x + margin, max_y + margin, 0.0];
        let stock = cad_cam::Stock::new(stock_min, stock_max)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let clearance_z = 5.0;
        let setup = cad_cam::Setup::new(
            cad_cam::Machine::Milling3Axis,
            cad_cam::MachineEnvelope::default_mill(),
            stock,
            cad_cam::WorkCoordinate::standard(),
            clearance_z,
        )
        .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let tool_id = cad_cam::ToolId::new(1)
            .ok_or_else(|| StudioError::BadRequest("invalid tool id".into()))?;
        let tool = cad_cam::Tool::new(
            tool_id,
            "Flat end mill",
            cad_cam::ToolGeometry::FlatEndMill {
                diameter_mm: 6.0,
                flute_length_mm: 20.0,
            },
            cad_cam::CuttingCondition::conservative_default(),
            stepdown_mm,
        )
        .map_err(|e| StudioError::BadRequest(format!("CAM tool error: {e}")))?;

        let op = cad_cam::Operation::Contour {
            profile: profile.clone(),
            depth_mm,
            stepdown_mm,
            offset_mm: 0.0,
        };

        let path_id = cad_cam::ToolpathId::new(1)
            .ok_or_else(|| StudioError::BadRequest("invalid path id".into()))?;
        let toolpath = cad_cam::generate(&op, &setup, &tool, path_id)
            .map_err(|e| StudioError::BadRequest(format!("CAM generation error: {e}")))?;

        let summary = cad_cam::summarize(&toolpath, &setup);
        let post_format = match req
            .post_format
            .as_deref()
            .unwrap_or("linuxcnc")
            .to_lowercase()
            .as_str()
        {
            "grbl" => cad_cam::PostFormat::Grbl,
            "mach3" => cad_cam::PostFormat::Mach3,
            _ => cad_cam::PostFormat::LinuxCnc,
        };
        let post = cad_cam::Postprocessor::new(post_format, "DRAFT_CONTOUR");
        let gcode = post
            .emit(&toolpath, &setup)
            .map_err(|e| StudioError::BadRequest(format!("CAM post error: {e}")))?;

        let resp = serde_json::json!({
            "entity_id": req.entity_id,
            "profile_points": profile.len(),
            "moves_count": toolpath.points.len(),
            "cutting_length_mm": toolpath.cutting_length_mm(),
            "rapid_length_mm": toolpath.rapid_length_mm(),
            "summary": summary,
            "gcode": gcode,
        });

        Ok(resp)
    }

    fn drawing_radial_dimension(
        &mut self,
        req: &DraftRadialDimensionRequest,
    ) -> Result<DraftView, StudioError> {
        let layer_name = req.layer.as_deref().unwrap_or(cad_drafting::DEFAULT_LAYER);
        let leader_offset = req.leader_offset.unwrap_or(15.0);
        let is_diameter = req.is_diameter.unwrap_or(false);

        {
            let draft = self.drawing_mut()?;
            if let Some(raw_id) = req.entity_id {
                let ent_id =
                    EntityId::new(raw_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
                let pick_angle = req.pick_angle_deg.unwrap_or(45.0);
                draft.add_dimension_for_curve(
                    layer_name,
                    ent_id,
                    pick_angle,
                    leader_offset,
                    is_diameter,
                    req.text.clone(),
                )?;
            } else if let (Some(center), Some(target)) = (req.center, req.target) {
                draft.add_radial_dimension(
                    layer_name,
                    center,
                    target,
                    leader_offset,
                    is_diameter,
                    req.text.clone(),
                )?;
            } else {
                return Err(StudioError::BadRequest(
                    "radial dimension requires either entity_id (of arc/circle) or center and target points".into(),
                ));
            }
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_add_block(&mut self, req: &DraftBlockRequest) -> Result<DraftView, StudioError> {
        let entities = {
            let draft = self.drawing_mut()?;
            let mut ents = Vec::new();
            for &id_raw in &req.entity_ids {
                let id =
                    EntityId::new(id_raw).map_err(|e| StudioError::BadRequest(e.to_string()))?;
                let ent = draft
                    .entity(id)
                    .ok_or_else(|| StudioError::BadRequest(format!("entity {id_raw} not found")))?;
                ents.push(ent.kind.clone());
            }
            ents
        };
        {
            let draft = self.drawing_mut()?;
            draft.add_block(&req.name, req.base_point, entities)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_add_insert(&mut self, req: &DraftInsertRequest) -> Result<DraftView, StudioError> {
        let layer = req.layer.as_deref().unwrap_or(cad_drafting::DEFAULT_LAYER);
        let scale = req.scale.unwrap_or([1.0, 1.0]);
        let rotation = req.rotation_degrees.unwrap_or(0.0);
        {
            let draft = self.drawing_mut()?;
            draft.add_insert(layer, &req.block_name, req.position, scale, rotation)?;
        }
        self.drawing_edited = true;
        self.draft_payload()
    }

    fn drawing_purge(&mut self) -> Result<DraftPurgeResponse, StudioError> {
        let report = {
            let draft = self.drawing_mut()?;
            draft.purge()
        };
        if !report.purged_layers.is_empty() || !report.purged_blocks.is_empty() {
            self.drawing_edited = true;
        }
        let draft = self.draft_payload()?;
        Ok(DraftPurgeResponse { report, draft })
    }

    fn drawing_overkill(
        &mut self,
        req: &DraftOverkillRequest,
    ) -> Result<DraftOverkillResponse, StudioError> {
        let report = {
            let draft = self.drawing_mut()?;
            draft.overkill(req.tolerance)
        };
        if report.duplicate_entities_removed > 0 || report.collinear_lines_merged > 0 {
            self.drawing_edited = true;
        }
        let draft = self.draft_payload()?;
        Ok(DraftOverkillResponse { report, draft })
    }

    fn drawing_divide(
        &mut self,
        req: &DraftDivideRequest,
    ) -> Result<DraftDivideResponse, StudioError> {
        let ent_id =
            EntityId::new(req.entity_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let created_point_ids = {
            let draft = self.drawing_mut()?;
            draft.divide(ent_id, req.segments)?
        };
        self.drawing_edited = true;
        let draft = self.draft_payload()?;
        Ok(DraftDivideResponse {
            created_point_ids: created_point_ids.into_iter().map(|id| id.get()).collect(),
            draft,
        })
    }

    fn drawing_measure_interval(
        &mut self,
        req: &DraftMeasureIntervalRequest,
    ) -> Result<DraftMeasureIntervalResponse, StudioError> {
        let ent_id =
            EntityId::new(req.entity_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let created_point_ids = {
            let draft = self.drawing_mut()?;
            draft.measure_intervals(ent_id, req.interval)?
        };
        self.drawing_edited = true;
        let draft = self.draft_payload()?;
        Ok(DraftMeasureIntervalResponse {
            created_point_ids: created_point_ids.into_iter().map(|id| id.get()).collect(),
            draft,
        })
    }

    fn drawing_match_properties(
        &mut self,
        req: &DraftMatchPropertiesRequest,
    ) -> Result<DraftMatchPropertiesResponse, StudioError> {
        let src_id =
            EntityId::new(req.source_id).map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let target_ids: Result<Vec<EntityId>, _> = req
            .target_ids
            .iter()
            .map(|&id| EntityId::new(id).map_err(|e| StudioError::BadRequest(e.to_string())))
            .collect();
        let target_ids = target_ids?;
        let matched_count = {
            let draft = self.drawing_mut()?;
            draft.match_properties(src_id, &target_ids)?
        };
        if matched_count > 0 {
            self.drawing_edited = true;
        }
        let draft = self.draft_payload()?;
        Ok(DraftMatchPropertiesResponse {
            matched_count,
            draft,
        })
    }

    fn drawing_solve_constraints(
        &mut self,
        req: &DraftConstraintsSolveRequest,
    ) -> Result<DraftConstraintsSolveResponse, StudioError> {
        let mut sys = cad_constraints::SketchSystem::new();
        let mut point_ids = Vec::with_capacity(req.points.len());
        for pt in &req.points {
            point_ids.push(sys.add_point(pt[0], pt[1]));
        }

        let mut line_ids = Vec::with_capacity(req.lines.len());
        for l in &req.lines {
            if l[0] >= point_ids.len() || l[1] >= point_ids.len() {
                return Err(StudioError::BadRequest(
                    "invalid point index in line".into(),
                ));
            }
            let lid = sys
                .add_line(point_ids[l[0]], point_ids[l[1]])
                .map_err(|e| StudioError::BadRequest(e.to_string()))?;
            line_ids.push(lid);
        }

        let mut circle_ids = Vec::with_capacity(req.circles.len());
        for c in &req.circles {
            if c.center >= point_ids.len() {
                return Err(StudioError::BadRequest(
                    "invalid center point index in circle".into(),
                ));
            }
            let cid = sys
                .add_circle(point_ids[c.center], c.radius)
                .map_err(|e| StudioError::BadRequest(e.to_string()))?;
            circle_ids.push(cid);
        }

        for c in &req.constraints {
            sys.add_constraint(c.clone())
                .map_err(|e| StudioError::BadRequest(e.to_string()))?;
        }

        let dof = sys.dof_analysis();

        let options = cad_constraints::SolveOptions {
            max_iterations: req.max_iterations.unwrap_or(100),
            tolerance: req.tolerance.unwrap_or(1e-8),
        };

        let solve = sys
            .solve_with_options(&options)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let solved_points: Vec<[f64; 2]> = sys.points().iter().map(|p| [p.x, p.y]).collect();

        let mut updated_entities = Vec::new();
        let mut draft_view = None;

        if let Some(ref mappings) = req.update_entities
            && solve.converged
        {
            self.ensure_drawing()?;
            let draft = self
                .drawing
                .as_mut()
                .ok_or_else(|| StudioError::BadRequest("the drawing is unavailable".into()))?;

            for map in mappings {
                let eid = EntityId::new(map.entity_id)
                    .map_err(|e| StudioError::BadRequest(e.to_string()))?;

                if let Some(l_idx) = map.line_index {
                    if l_idx < line_ids.len() {
                        let l = sys.line(line_ids[l_idx]).ok_or_else(|| {
                            StudioError::BadRequest("line not found in solved system".into())
                        })?;
                        let p1 = sys
                            .point(l.p1)
                            .ok_or_else(|| StudioError::BadRequest("point not found".into()))?;
                        let p2 = sys
                            .point(l.p2)
                            .ok_or_else(|| StudioError::BadRequest("point not found".into()))?;
                        let new_kind = cad_drafting::EntityKind::Line {
                            start: [p1.x, p1.y],
                            end: [p2.x, p2.y],
                        };
                        draft.update_entity_kind(eid, new_kind)?;
                        updated_entities.push(map.entity_id);
                    }
                } else if let Some(c_idx) = map.circle_index
                    && c_idx < circle_ids.len()
                {
                    let c = sys.circle(circle_ids[c_idx]).ok_or_else(|| {
                        StudioError::BadRequest("circle not found in solved system".into())
                    })?;
                    let center = sys
                        .point(c.center)
                        .ok_or_else(|| StudioError::BadRequest("point not found".into()))?;
                    let new_kind = cad_drafting::EntityKind::Circle {
                        center: [center.x, center.y],
                        radius: c.radius,
                    };
                    draft.update_entity_kind(eid, new_kind)?;
                    updated_entities.push(map.entity_id);
                }
            }

            if !updated_entities.is_empty() {
                self.drawing_edited = true;
                draft_view = Some(self.draft_payload()?);
            }
        }

        Ok(DraftConstraintsSolveResponse {
            solve,
            dof,
            solved_points,
            updated_entities,
            draft: draft_view,
        })
    }

    /// Store the saved drawing's source revision in the project's embedded registry.
    ///
    /// This is what makes the pin survive a restart: before it existed only the in-memory session
    /// knew which revision a `.zdraft` file came from.
    fn record_drawing_artifact(&mut self) -> Result<(), StudioError> {
        let document = self.project.header.document;
        let revision = self.project.revision;
        let id = ArtifactId::new(1)
            .ok_or_else(|| StudioError::BadRequest("invalid artifact id".into()))?;
        self.project.record_artifact(DerivedArtifact::new(
            id,
            ArtifactKind::Drawing,
            "drawing",
            DocumentRef::new(document, revision, None),
        ))?;
        self.artifacts_changed = true;
        Ok(())
    }

    /// Acknowledge a stale artifact's result at `revision` without pretending it was recomputed.
    fn review_artifact(
        &mut self,
        id: ArtifactId,
        revision: Revision,
    ) -> Result<EngineeringView, StudioError> {
        self.project.review_artifact(id, revision)?;
        self.artifacts_changed = true;
        self.engineering_payload()
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

    fn cae_static(&mut self, request: &CaeStaticRequest) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to analyze".into(),
            ));
        }
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }
        let mesh = build.mesh.ok_or_else(|| {
            StudioError::BadRequest("this project has no solid to analyze".into())
        })?;

        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let span_x = (max[0] - min[0]).max(1.0);
        let voxel_size = request
            .voxel_size_mm
            .unwrap_or_else(|| (span_x / 12.0).clamp(2.5, 10.0));

        let mut study = cad_cae::StaticStudy::new("studio-static", voxel_size, &request.material);
        study.supports = vec![cad_cae::Support::fixed(cad_cae::FaceSelection::new(
            cad_cae::Axis::X,
            min[0],
        ))];
        study.loads = vec![cad_cae::FaceLoad::new(
            cad_cae::FaceSelection::new(cad_cae::Axis::X, max[0]),
            [0.0, 0.0, -100.0],
        )];

        let library = cad_engineering_model::MaterialLibrary::standard();
        let result = cad_cae::run_static(&mesh, &study, &library)
            .map_err(|e| StudioError::BadRequest(format!("CAE simulation error: {e}")))?;

        Ok(serde_json::json!({
            "study": result.study,
            "voxel_size_mm": result.mesh.voxel_size_mm,
            "elements": result.mesh.elements,
            "nodes": result.mesh.nodes,
            "max_displacement_mm": result.results.max_displacement_mm,
            "max_von_mises_mpa": result.results.max_von_mises_mpa,
            "factor_of_safety": result.results.factor_of_safety,
            "equilibrium_residual": result.results.equilibrium_residual_ratio,
            "iterations": result.solver.iterations,
            "quality": result.quality,
            "warnings": result.warnings,
        }))
    }

    fn cae_modal(&mut self, request: &CaeModalRequest) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to analyze".into(),
            ));
        }
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }
        let mesh = build.mesh.ok_or_else(|| {
            StudioError::BadRequest("this project has no solid to analyze".into())
        })?;

        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let span_x = (max[0] - min[0]).max(1.0);
        let voxel_size = request
            .voxel_size_mm
            .unwrap_or_else(|| (span_x / 12.0).clamp(2.5, 10.0));
        let num_modes = request.num_modes.unwrap_or(3);

        let study = cad_cae::ModalStudy::new("studio-modal", voxel_size, &request.material)
            .with_modes(num_modes)
            .with_support(cad_cae::Support::fixed(cad_cae::FaceSelection::new(
                cad_cae::Axis::X,
                min[0],
            )));

        let library = cad_engineering_model::MaterialLibrary::standard();
        let result = cad_cae::run_modal(&mesh, &study, &library)
            .map_err(|e| StudioError::BadRequest(format!("CAE modal error: {e}")))?;

        let modes_summary: Vec<_> = result
            .modes
            .iter()
            .map(|m| {
                serde_json::json!({
                    "mode": m.mode,
                    "frequency_hz": m.frequency_hz,
                    "angular_frequency_rad_s": m.angular_frequency_rad_s,
                    "generalized_mass": m.generalized_mass,
                })
            })
            .collect();

        Ok(serde_json::json!({
            "study": result.study,
            "voxel_size_mm": result.mesh.voxel_size_mm,
            "elements": result.mesh.elements,
            "nodes": result.mesh.nodes,
            "fundamental_frequency_hz": result.fundamental_frequency_hz(),
            "modes": modes_summary,
            "quality": result.quality,
            "warnings": result.warnings,
        }))
    }

    fn cae_thermal(
        &mut self,
        request: &CaeThermalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to analyze".into(),
            ));
        }
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }
        let mesh = build.mesh.ok_or_else(|| {
            StudioError::BadRequest("this project has no solid to analyze".into())
        })?;

        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }
        let span_x = (max[0] - min[0]).max(1.0);
        let voxel_size = request
            .voxel_size_mm
            .unwrap_or_else(|| (span_x / 10.0).clamp(2.5, 10.0));

        let t_hot = request.fixed_temp_hot_c.unwrap_or(100.0);
        let t_cold = request.fixed_temp_cold_c.unwrap_or(20.0);

        let study = cad_cae::ThermalStudy::new("studio-thermal", voxel_size, &request.material)
            .with_fixed_temperature(cad_cae::FixedTemperature::new(
                cad_cae::FaceSelection::new(cad_cae::Axis::X, min[0]),
                t_hot,
            ))
            .with_fixed_temperature(cad_cae::FixedTemperature::new(
                cad_cae::FaceSelection::new(cad_cae::Axis::X, max[0]),
                t_cold,
            ));

        let library = cad_engineering_model::MaterialLibrary::standard();
        let result = cad_cae::run_thermal(&mesh, &study, &library)
            .map_err(|e| StudioError::BadRequest(format!("CAE thermal error: {e}")))?;

        Ok(serde_json::json!({
            "study": result.study,
            "voxel_size_mm": result.voxel_size_mm,
            "elements": result.element_count,
            "nodes": result.node_count,
            "min_temperature_c": result.min_temperature_c,
            "max_temperature_c": result.max_temperature_c,
            "avg_temperature_c": result.avg_temperature_c,
            "max_heat_flux_w_m2": result.max_heat_flux_w_m2,
            "avg_heat_flux_w_m2": result.avg_heat_flux_w_m2,
            "quality": result.quality_label,
            "summary": result.summary_line(),
            "warnings": result.warnings,
        }))
    }

    fn cam_toolpaths(&mut self, request: &CamRequest) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to machine".into(),
            ));
        }
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }
        let mesh = build.mesh.ok_or_else(|| {
            StudioError::BadRequest("this project has no solid to machine".into())
        })?;

        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }

        let margin = 5.0;
        let stock_min = [min[0] - margin, min[1] - margin, min[2] - margin];
        let stock_max = [max[0] + margin, max[1] + margin, max[2] + 2.0];
        let stock = cad_cam::Stock::new(stock_min, stock_max)
            .map_err(|e| StudioError::BadRequest(format!("CAM stock error: {e}")))?;
        let setup = cad_cam::Setup::new(
            cad_cam::Machine::Milling3Axis,
            cad_cam::MachineEnvelope::default_mill(),
            stock,
            cad_cam::WorkCoordinate::standard(),
            stock_max[2] + 5.0,
        )
        .map_err(|e| StudioError::BadRequest(format!("CAM setup error: {e}")))?;

        let tool_dia = request.tool_diameter_mm.unwrap_or(6.0);
        let tool_id = cad_cam::ToolId::new(1)
            .ok_or_else(|| StudioError::BadRequest("invalid tool id".into()))?;
        let tool = cad_cam::Tool::new(
            tool_id,
            "Flat end mill",
            cad_cam::ToolGeometry::FlatEndMill {
                diameter_mm: tool_dia,
                flute_length_mm: 20.0,
            },
            cad_cam::CuttingCondition::conservative_default(),
            2.0,
        )
        .map_err(|e| StudioError::BadRequest(format!("CAM tool error: {e}")))?;

        let op = cad_cam::Operation::Facing {
            depth_mm: request.depth_mm.unwrap_or(1.0),
            stepover_mm: request.stepover_mm.unwrap_or(tool_dia * 0.7),
        };
        let path_id = cad_cam::ToolpathId::new(1)
            .ok_or_else(|| StudioError::BadRequest("invalid path id".into()))?;
        let toolpath = cad_cam::generate(&op, &setup, &tool, path_id)
            .map_err(|e| StudioError::BadRequest(format!("CAM generation error: {e}")))?;

        let summary = cad_cam::summarize(&toolpath, &setup);
        let post = cad_cam::Postprocessor::new(cad_cam::PostFormat::LinuxCnc, "STUDIO_PROG");
        let gcode = post
            .emit(&toolpath, &setup)
            .map_err(|e| StudioError::BadRequest(format!("CAM post error: {e}")))?;
        let preview_lines: Vec<&str> = gcode.lines().take(25).collect();

        let limits = cad_cam::KinematicLimits::default();
        let cycle_time = toolpath.kinematic_cycle_time(&limits);
        let alu_wear = cad_cam::Material::Aluminium.estimate_tool_wear(
            std::f64::consts::PI * tool_dia * tool.cutting.spindle_rpm / 1000.0,
            toolpath.estimated_minutes(),
        );

        Ok(serde_json::json!({
            "tool_name": tool.name,
            "moves_count": toolpath.points.len(),
            "cutting_length_mm": toolpath.cutting_length_mm(),
            "rapid_length_mm": toolpath.rapid_length_mm(),
            "estimated_minutes": toolpath.estimated_minutes(),
            "kinematic_cycle_time_seconds": cycle_time.total_seconds,
            "tool_wear_fraction": alu_wear.wear_fraction,
            "tool_wear_percent": alu_wear.wear_percent,
            "summary": summary,
            "gcode_preview": preview_lines.join("\n"),
        }))
    }

    fn mass_properties_payload(
        &mut self,
        body_id: Option<&str>,
        material_id: Option<&str>,
        density_override: Option<f64>,
    ) -> Result<serde_json::Value, StudioError> {
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }

        let standard_lib = MaterialLibrary::standard();
        let (mat_name, density) = if let Some(d) = density_override {
            ("Custom Density", d)
        } else if let Some(m_id) = material_id {
            match standard_lib.get(m_id) {
                Some(m) => (m.name.as_str(), m.density_kg_m3),
                None => ("AISI 1018 Steel", 7850.0),
            }
        } else {
            ("AISI 1018 Steel", 7850.0)
        };

        let (target_id, mass_props) = if let Some(bid) = body_id {
            let props = build.body_mass_properties(bid).ok_or_else(|| {
                StudioError::BadRequest(format!("body '{bid}' not found or has no volume"))
            })?;
            (bid.to_string(), props)
        } else {
            let props = build.mass_properties().ok_or_else(|| {
                StudioError::BadRequest("no solid geometry found in project".into())
            })?;
            ("active_part".to_string(), props)
        };

        let physical = mass_props.with_density(density);

        Ok(serde_json::json!({
            "target": target_id,
            "material": {
                "name": mat_name,
                "density_kg_m3": density,
            },
            "geometric": mass_props,
            "physical": physical,
        }))
    }

    fn interference_payload(&mut self) -> Result<serde_json::Value, StudioError> {
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }
        let report = build.interference_check();
        serde_json::to_value(&report).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn bom_payload(
        &mut self,
        material_name: Option<&str>,
        density_override: Option<f64>,
    ) -> Result<serde_json::Value, StudioError> {
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        if let Some(failure) = build.failed {
            return Err(StudioError::BadRequest(format!(
                "feature {} cannot be rebuilt: {}",
                failure.id, failure.message
            )));
        }
        let bom = build.bill_of_materials(material_name, density_override);
        serde_json::to_value(&bom).map_err(|e| StudioError::Json(e.to_string()))
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
    // A token means the operator wants a guarded session, even when the socket is loopback. That is
    // the deployment shape in deploy/: a TLS proxy on the same host forwards to 127.0.0.1, so
    // deciding “shared” from the bind address alone would serve the document with no token behind a
    // public hostname — the worst of both worlds.
    let shared = !bind.is_loopback() || options.token.is_some();
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
    // even load the page that is allowed to present the token. The liveness route is the one
    // exception: it reports whether the process is serving and which version, never the document,
    // and a container or load balancer cannot be given a secret just to ask if we are alive.
    let liveness = request.method == "GET" && path == "/api/health";
    if let Some(share) = &share
        && path.starts_with("/api/")
        && !liveness
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
        ("GET", "/api/engineering") => {
            match with_state(state, |state| state.engineering_payload()) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            }
        }
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
        ("GET", "/api/mass-properties") => {
            let query = request.path.clone();
            let body = query_text(&query, "body");
            let material = query_text(&query, "material");
            let density = query_param(&query, "density").and_then(|d| d.parse::<f64>().ok());
            match with_state(state, |state| {
                state.mass_properties_payload(body.as_deref(), material.as_deref(), density)
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/interference") => {
            match with_state(state, |state| state.interference_payload()) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/bom") => {
            let query = request.path.clone();
            let material = query_text(&query, "material");
            let density = query_param(&query, "density").and_then(|d| d.parse::<f64>().ok());
            match with_state(state, |state| {
                state.bom_payload(material.as_deref(), density)
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            }
        }
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
        "/api/draft/transform" => match parse_body::<DraftTransformRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_transform(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/offset" => match parse_body::<DraftOffsetRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_offset(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/fillet" => match parse_body::<DraftFilletRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_fillet(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/chamfer" => match parse_body::<DraftChamferRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_chamfer(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/explode" => match parse_body::<DraftExplodeRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_explode(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/measure" => match parse_body::<DraftMeasureRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_measure(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/to-solid" => match parse_body::<DraftToSolidRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_to_solid(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/hatch" => match parse_body::<DraftHatchRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_hatch(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/multiview" => match with_state(state, StudioState::drawing_multiview) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/draft/join" => match parse_body::<DraftJoinRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_join(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/array" => match parse_body::<DraftArrayRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_array(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/construct" => match parse_body::<DraftConstructRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_construct(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/section-analysis" => match parse_body::<DraftSectionRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_section(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/trim" => match parse_body::<DraftTrimRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_trim(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/extend" => match parse_body::<DraftExtendRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_extend(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/to-cam" => match parse_body::<DraftToCamRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_to_cam(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/dimension-radial" => match parse_body::<DraftRadialDimensionRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_radial_dimension(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/block" => match parse_body::<DraftBlockRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_add_block(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/insert" => match parse_body::<DraftInsertRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_add_insert(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/purge" => match with_state(state, StudioState::drawing_purge) {
            Ok(payload) => to_json(200, &payload),
            Err(error) => error_response(&error),
        },
        "/api/draft/overkill" => match parse_body_or_default::<DraftOverkillRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_overkill(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/divide" => match parse_body::<DraftDivideRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_divide(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/measure-interval" => match parse_body::<DraftMeasureIntervalRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_measure_interval(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/match-properties" => match parse_body::<DraftMatchPropertiesRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_match_properties(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/constraints/solve" => {
            match parse_body::<DraftConstraintsSolveRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.drawing_solve_constraints(&body)) {
                        Ok(payload) => to_json(200, &payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/engineering/review" => match parse_body::<ArtifactReviewRequest>(request) {
            Ok(body) => {
                let id = match ArtifactId::new(body.id) {
                    Some(id) => id,
                    None => {
                        return error_response(&StudioError::BadRequest(
                            "an artifact id of 0 is not valid".into(),
                        ));
                    }
                };
                let revision = match Revision::from_raw(body.revision) {
                    Some(revision) => revision,
                    None => {
                        return error_response(&StudioError::BadRequest(
                            "a review revision must be positive".into(),
                        ));
                    }
                };
                match with_state(state, |state| state.review_artifact(id, revision)) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
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
        "/api/cae/static" => match parse_body_or_default::<CaeStaticRequest>(request) {
            Ok(body) => match with_state(state, |state| state.cae_static(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/cae/modal" => match parse_body_or_default::<CaeModalRequest>(request) {
            Ok(body) => match with_state(state, |state| state.cae_modal(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/cae/thermal" => match parse_body_or_default::<CaeThermalRequest>(request) {
            Ok(body) => match with_state(state, |state| state.cae_thermal(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/cam/toolpaths" => match parse_body_or_default::<CamRequest>(request) {
            Ok(body) => match with_state(state, |state| state.cam_toolpaths(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/mass-properties" => match parse_body_or_default::<MassPropertiesRequest>(request) {
            Ok(body) => match with_state(state, |state| {
                state.mass_properties_payload(
                    body.body.as_deref(),
                    body.material.as_deref(),
                    body.density_kg_m3,
                )
            }) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
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
    #[serde(default)]
    linetype: Option<cad_drafting::Linetype>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftRadialDimensionRequest {
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    target: Option<[f64; 2]>,
    #[serde(default)]
    entity_id: Option<u64>,
    #[serde(default)]
    pick_angle_deg: Option<f64>,
    #[serde(default)]
    leader_offset: Option<f64>,
    #[serde(default)]
    is_diameter: Option<bool>,
    #[serde(default)]
    text: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftBlockRequest {
    name: String,
    base_point: [f64; 2],
    #[serde(default)]
    entity_ids: Vec<u64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftInsertRequest {
    #[serde(default)]
    layer: Option<String>,
    block_name: String,
    position: [f64; 2],
    #[serde(default)]
    scale: Option<[f64; 2]>,
    #[serde(default)]
    rotation_degrees: Option<f64>,
}

#[derive(Debug, Serialize)]
struct DraftPurgeResponse {
    report: cad_drafting::PurgeReport,
    draft: DraftView,
}

#[derive(Debug, Deserialize, Default)]
#[serde(deny_unknown_fields)]
struct DraftOverkillRequest {
    #[serde(default)]
    tolerance: Option<f64>,
}

#[derive(Debug, Serialize)]
struct DraftOverkillResponse {
    report: cad_drafting::OverkillReport,
    draft: DraftView,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftDivideRequest {
    entity_id: u64,
    segments: usize,
}

#[derive(Debug, Serialize)]
struct DraftDivideResponse {
    created_point_ids: Vec<u64>,
    draft: DraftView,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftMeasureIntervalRequest {
    entity_id: u64,
    interval: f64,
}

#[derive(Debug, Serialize)]
struct DraftMeasureIntervalResponse {
    created_point_ids: Vec<u64>,
    draft: DraftView,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftMatchPropertiesRequest {
    source_id: u64,
    target_ids: Vec<u64>,
}

#[derive(Debug, Serialize)]
struct DraftMatchPropertiesResponse {
    matched_count: usize,
    draft: DraftView,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftConstraintsSolveRequest {
    #[serde(default)]
    points: Vec<[f64; 2]>,
    #[serde(default)]
    lines: Vec<[usize; 2]>,
    #[serde(default)]
    circles: Vec<DraftConstraintCircleInput>,
    #[serde(default)]
    constraints: Vec<cad_constraints::ConstraintKind>,
    #[serde(default)]
    update_entities: Option<Vec<DraftConstraintEntityMap>>,
    #[serde(default)]
    max_iterations: Option<usize>,
    #[serde(default)]
    tolerance: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftConstraintCircleInput {
    center: usize,
    radius: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftConstraintEntityMap {
    entity_id: u64,
    #[serde(default)]
    line_index: Option<usize>,
    #[serde(default)]
    circle_index: Option<usize>,
}

#[derive(Debug, Serialize)]
struct DraftConstraintsSolveResponse {
    solve: cad_constraints::SolveReport,
    dof: cad_constraints::DofReport,
    solved_points: Vec<[f64; 2]>,
    updated_entities: Vec<u64>,
    draft: Option<DraftView>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftTransformRequest {
    op: String,
    ids: Vec<u64>,
    #[serde(default)]
    dx: Option<f64>,
    #[serde(default)]
    dy: Option<f64>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    angle_deg: Option<f64>,
    #[serde(default)]
    base: Option<[f64; 2]>,
    #[serde(default)]
    factor: Option<f64>,
    #[serde(default)]
    p1: Option<[f64; 2]>,
    #[serde(default)]
    p2: Option<[f64; 2]>,
    #[serde(default)]
    erase_source: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftOffsetRequest {
    id: u64,
    distance: f64,
    #[serde(default)]
    side: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftFilletRequest {
    id1: u64,
    id2: u64,
    radius: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftChamferRequest {
    id1: u64,
    id2: u64,
    d1: f64,
    d2: f64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftExplodeRequest {
    id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftMeasureRequest {
    mode: String,
    #[serde(default)]
    p1: Option<[f64; 2]>,
    #[serde(default)]
    p2: Option<[f64; 2]>,
    #[serde(default)]
    points: Option<Vec<[f64; 2]>>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftToSolidRequest {
    entity_id: u64,
    #[serde(default)]
    depth: f64,
    #[serde(default)]
    operation: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftHatchRequest {
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    entity_id: Option<u64>,
    #[serde(default)]
    points: Option<Vec<[f64; 2]>>,
    #[serde(default)]
    pattern: Option<String>,
    #[serde(default)]
    angle_deg: Option<f64>,
    #[serde(default)]
    spacing: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftJoinRequest {
    ids: Vec<u64>,
    #[serde(default)]
    tolerance: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftArrayRequest {
    mode: String,
    ids: Vec<u64>,
    #[serde(default)]
    rows: Option<usize>,
    #[serde(default)]
    cols: Option<usize>,
    #[serde(default)]
    row_spacing: Option<f64>,
    #[serde(default)]
    col_spacing: Option<f64>,
    #[serde(default)]
    count: Option<usize>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    fill_angle_deg: Option<f64>,
    #[serde(default)]
    rotate_items: Option<bool>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftConstructRequest {
    kind: String,
    #[serde(default)]
    layer: Option<String>,
    #[serde(default)]
    p1: Option<[f64; 2]>,
    #[serde(default)]
    p2: Option<[f64; 2]>,
    #[serde(default)]
    p3: Option<[f64; 2]>,
    #[serde(default)]
    center: Option<[f64; 2]>,
    #[serde(default)]
    radius: Option<f64>,
    #[serde(default)]
    sides: Option<usize>,
    #[serde(default)]
    inscribed: Option<bool>,
    #[serde(default)]
    start_angle_deg: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftSectionRequest {
    id: u64,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftTrimRequest {
    target_id: u64,
    #[serde(default)]
    boundary_ids: Vec<u64>,
    pick_point: [f64; 2],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftExtendRequest {
    target_id: u64,
    #[serde(default)]
    boundary_ids: Vec<u64>,
    pick_point: [f64; 2],
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftToCamRequest {
    entity_id: u64,
    #[serde(default)]
    depth_mm: Option<f64>,
    #[serde(default)]
    stepdown_mm: Option<f64>,
    #[serde(default)]
    post_format: Option<String>,
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

/// Acknowledgment that a stale derived artifact was reviewed at a given design revision.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ArtifactReviewRequest {
    id: u64,
    revision: u64,
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
        // A shared studio is reached under whatever name the reader used: a LAN address, a DNS
        // name, a port-mapped container whose public port is not the one we listen on, or a proxy
        // that terminates TLS. Requiring the presented host to carry *our* port rejected every one
        // of those with 403 — including the container behind `-p 7599:7519` and the Caddy setup in
        // deploy/. The name proves nothing here; the token does. So any well-formed authority is
        // accepted, and the token check that follows is what actually guards the document.
        return if host.is_empty() {
            Err(StudioError::Forbidden)
        } else {
            Ok(())
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
        // The page was reached at some address:port; a form post must come from that same origin,
        // so a page on another site cannot drive edits. The scheme is either one: a proxy that
        // terminates TLS forwards the public `Host` while the browser reports `https://`, and
        // comparing against a hard-coded `http://` refused every proxied edit.
        let host = request
            .header("host")
            .ok_or(StudioError::Forbidden)?
            .trim()
            .to_ascii_lowercase();
        let authority = origin
            .strip_prefix("http://")
            .or_else(|| origin.strip_prefix("https://"));
        return match authority {
            Some(authority) if !host.is_empty() && authority == host => Ok(()),
            _ => Err(StudioError::Forbidden),
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

fn parse_body_or_default<T: serde::de::DeserializeOwned + Default>(
    request: &http::Request,
) -> Result<T, StudioError> {
    if request.body.is_empty() {
        Ok(T::default())
    } else {
        serde_json::from_slice(&request.body).map_err(StudioError::from)
    }
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct MassPropertiesRequest {
    #[serde(default)]
    body: Option<String>,
    #[serde(default)]
    material: Option<String>,
    #[serde(default)]
    density_kg_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaeStaticRequest {
    #[serde(default = "default_steel")]
    material: String,
    #[serde(default)]
    voxel_size_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaeModalRequest {
    #[serde(default = "default_steel")]
    material: String,
    #[serde(default)]
    voxel_size_mm: Option<f64>,
    #[serde(default)]
    num_modes: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CaeThermalRequest {
    #[serde(default = "default_steel")]
    material: String,
    #[serde(default)]
    voxel_size_mm: Option<f64>,
    #[serde(default = "default_temp_hot")]
    fixed_temp_hot_c: Option<f64>,
    #[serde(default = "default_temp_cold")]
    fixed_temp_cold_c: Option<f64>,
}

fn default_temp_hot() -> Option<f64> {
    Some(100.0)
}

fn default_temp_cold() -> Option<f64> {
    Some(20.0)
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CamRequest {
    #[serde(default)]
    tool_diameter_mm: Option<f64>,
    #[serde(default)]
    stepover_mm: Option<f64>,
    #[serde(default)]
    depth_mm: Option<f64>,
}

fn default_steel() -> String {
    "steel.1018".to_string()
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

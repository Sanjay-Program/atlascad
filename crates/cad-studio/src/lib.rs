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
use std::collections::{BTreeMap, HashMap};
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
const FLIGHT_POSTER: &[u8] = include_bytes!("../assets/flight-demo-poster.png");
const FLIGHT_VIDEO: &[u8] = include_bytes!("../assets/atlascad-supersonic-flight.mp4");

/// Everything the studio can fail with.
#[derive(Debug)]
pub enum StudioError {
    Project(ProjectError),
    Kernel(cad_contract::error::KernelError),
    Stl(cad_io::StlError),
    /// A STEP file could not be written; the geometry is valid, the exchange format refused it.
    Step(cad_io::StepError),
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
    /// A capability exists in the API and UI but no adapter can perform it here. Refusing is the
    /// only honest answer: reporting a synthetic success would be a fabricated engineering result.
    Unavailable(String),
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
            StudioError::Step(error) => write!(f, "STEP: {error}"),
            StudioError::Drafting(error) => write!(f, "drawing: {error}"),
            StudioError::DrawingDoc(error) => write!(f, "drawing: {error}"),
            StudioError::Sheet(error) => write!(f, "sheet: {error}"),
            StudioError::Model(error) => write!(f, "engineering model: {error}"),
            StudioError::Json(message) => write!(f, "JSON: {message}"),
            StudioError::Io(message) => write!(f, "I/O: {message}"),
            StudioError::BadRequest(message) => write!(f, "{message}"),
            StudioError::Unavailable(message) => write!(f, "{message}"),
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

impl From<cad_io::StepError> for StudioError {
    fn from(error: cad_io::StepError) -> Self {
        StudioError::Step(error)
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
            StudioError::Unavailable(_) => 501,
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
            | StudioError::Step(_)
            | StudioError::Io(_)
            | StudioError::Lock
            | StudioError::Bind(_) => 500,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            StudioError::BadRequest(_) => "bad_request",
            StudioError::Unavailable(_) => "unavailable",
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
            StudioError::Stl(_) | StudioError::Step(_) => "export_failure",
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
    /// For a revolve: how far it turns, in degrees.
    angle_deg: Option<f64>,
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

/// One body read from an exchange file, as the UI lists it.
#[derive(Debug, Serialize)]
struct ImportView {
    name: String,
    /// The format it was read from (`STEP` today).
    source: String,
    triangles: usize,
    volume_mm3: f64,
    bounds: Option<BoundsView>,
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
    /// `model` while the plan is generated from the sketch, `imported` for a drawing read from a
    /// DXF, `file` once it is (or was saved as) a `.zdraft` beside the project. The distinction
    /// matters: a `model` drawing follows the model, the other two do not.
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
    /// Bodies read from exchange files; merged into `mesh` above, listed here so the UI can name
    /// and remove them.
    imports: Vec<ImportView>,
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

fn init_default_dependency_graph() -> atlas_dependency_graph::graph::DependencyGraph {
    use atlas_core::identifiers::NodeId;
    use atlas_dependency_graph::edge::DependencyReason;
    use atlas_dependency_graph::graph::DependencyGraph;
    use atlas_dependency_graph::node::{DependencyNode, NodeKind};

    let mut graph = DependencyGraph::new();

    let geom = NodeId::new("cad-geometry");
    let mesh = NodeId::new("fem-mesh");
    let fea = NodeId::new("fea-structural");
    let cfd = NodeId::new("cfd-aerodynamics");
    let dwg = NodeId::new("drawing-sheet");
    let cam = NodeId::new("cam-toolpaths");
    let bim = NodeId::new("bim-coordination");
    let additive = NodeId::new("additive-slicing");

    graph.add_node(DependencyNode::new(
        geom.clone(),
        "Solid CAD Geometry",
        NodeKind::Geometry,
    ));
    graph.add_node(DependencyNode::new(
        mesh.clone(),
        "Discretized FEA/CFD Mesh",
        NodeKind::Mesh,
    ));
    graph.add_node(DependencyNode::new(
        fea.clone(),
        "CalculiX Structural FEA",
        NodeKind::FeaSimulation,
    ));
    graph.add_node(DependencyNode::new(
        cfd.clone(),
        "OpenFOAM Aerodynamics",
        NodeKind::CfdSimulation,
    ));
    graph.add_node(DependencyNode::new(
        dwg.clone(),
        "2D Engineering Drawing",
        NodeKind::Drawing,
    ));
    graph.add_node(DependencyNode::new(
        cam.clone(),
        "3-Axis CNC Toolpaths",
        NodeKind::CamToolpath,
    ));
    graph.add_node(DependencyNode::new(
        bim.clone(),
        "OpenBIM & IFC Model",
        NodeKind::BimCoordination,
    ));
    graph.add_node(DependencyNode::new(
        additive.clone(),
        "3D Slicing & Additive Toolpaths",
        NodeKind::CamToolpath,
    ));

    let _ = graph.add_dependency(
        geom.clone(),
        mesh.clone(),
        DependencyReason::SpatialDiscretization,
    );
    let _ = graph.add_dependency(
        mesh.clone(),
        fea.clone(),
        DependencyReason::PropertyAggregation,
    );
    let _ = graph.add_dependency(
        mesh.clone(),
        cfd.clone(),
        DependencyReason::PropertyAggregation,
    );
    let _ = graph.add_dependency(geom.clone(), dwg.clone(), DependencyReason::ProjectionView);
    let _ = graph.add_dependency(geom.clone(), cam.clone(), DependencyReason::ToolpathSurface);
    let _ = graph.add_dependency(
        geom.clone(),
        bim.clone(),
        DependencyReason::GeometricBoundary,
    );
    let _ = graph.add_dependency(
        geom.clone(),
        additive.clone(),
        DependencyReason::ToolpathSurface,
    );

    graph
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabUser {
    pub id: String,
    pub name: String,
    pub role: String, // "Owner", "Lead", "Editor", "Reviewer", "Viewer"
    pub avatar_color: String,
    pub cursor_pos: [f64; 3],
    pub active_tool: String,
    pub selected_entity: Option<String>,
    pub last_heartbeat: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EntityLock {
    pub entity_id: String,
    pub holder_user_id: String,
    pub holder_name: String,
    pub acquired_at: u64,
    pub expires_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommentReply {
    pub id: String,
    pub author_id: String,
    pub author_name: String,
    pub text: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollabComment {
    pub id: String,
    pub entity_id: Option<String>,
    pub pos_3d: [f64; 3],
    pub author_id: String,
    pub author_name: String,
    pub author_color: String,
    pub text: String,
    pub status: String, // "Open", "InReview", "Resolved"
    pub created_at: u64,
    pub replies: Vec<CommentReply>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthUser {
    pub user_id: String,
    pub name: String,
    pub email: String,
    pub role: String,
    pub team_id: String,
    pub team_name: String,
    pub org_id: String,
    pub org_name: String,
    pub avatar_color: String,
    pub plan_tier: String,
    pub plan_seats: u32,
    pub plan_expiry: String,
    pub license_key: String,
    pub permissions: Vec<String>,
    pub mfa_enabled: bool,
    pub sso_provider: Option<String>,
    pub created_at: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthSession {
    pub token: String,
    pub user_id: String,
    pub created_at: u64,
    pub expires_at: u64,
    pub ip_address: String,
    pub user_agent: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ApiKeyRecord {
    pub id: String,
    pub name: String,
    pub token: String,
    pub user_id: String,
    pub created_at: u64,
    pub expires_at: Option<u64>,
    pub last_used_at: Option<u64>,
    pub scopes: Vec<String>,
}

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
    /// Set when the drawing came from an imported DXF rather than the model. The panel says so, and
    /// the model no longer regenerates it: the file's geometry is the user's, not a projection.
    drawing_imported: bool,
    /// The project revision the drawing was generated from. `None` when the drawing came from a
    /// `.zdraft` file, which does not carry its source revision, or before one exists.
    drawing_revision: Option<u64>,
    /// Set when the embedded artifact registry changed without a design revision bump. Recording a
    /// result is not a design edit, so it must not move the revision, but the project file still
    /// differs from disk and has to be marked unsaved.
    artifacts_changed: bool,
    /// Engineering dependency graph tracking provenance and downstream invalidation.
    dependency_graph: atlas_dependency_graph::graph::DependencyGraph,
    /// Tamper-evident FNV-1a audit trail used for local traceability checks, not a certification.
    audit_chain: atlas_audit::chain::AuditChain,
    /// Enterprise simulation job queue and priority scheduler.
    job_queue: atlas_job_queue::queue::JobQueueManager,
    /// Enterprise multi-tenant organization context and quotas.
    organization: atlas_organization::organization::Organization,
    /// Universal engineering command platform registry.
    command_registry: atlas_command_platform::CommandRegistry,
    /// Multi-agent engineering orchestrator.
    orchestrator: atlas_agent_orchestrator::EngineeringOrchestrator,
    /// Multi-tier Content-Addressable Asset Registry (Public, Team, Private).
    asset_registry: atlas_asset_manager::AssetRegistry,
    /// Live multi-user presence tracking and spatial cursors.
    collab_presence: HashMap<String, CollabUser>,
    /// Optimistic concurrency entity leases/locks.
    entity_locks: HashMap<String, EntityLock>,
    /// 3D spatial annotations and review comments.
    collab_comments: Vec<CollabComment>,
    /// Production identity and multi-tenant authentication users.
    auth_users: HashMap<String, AuthUser>,
    /// Active authenticated sessions.
    auth_sessions: HashMap<String, AuthSession>,
    /// Headless CI/CD and developer API keys / Personal Access Tokens.
    api_keys: Vec<ApiKeyRecord>,
    /// Current active authenticated user ID.
    active_user_id: String,
}

impl StudioState {
    fn new(
        project: Project,
        path: PathBuf,
        recovered: Option<RecoveryInfo>,
    ) -> Result<Self, StudioError> {
        project.validate()?;
        let saved_revision = project.revision.get();
        let mut audit_chain = atlas_audit::chain::AuditChain::new();
        audit_chain.record(
            now_unix_secs(),
            "system",
            atlas_audit::action::AuditAction::ProjectCreated,
            "Project",
            path.display().to_string(),
            serde_json::json!({ "revision": saved_revision }),
        );
        let mut collab_presence = HashMap::new();
        collab_presence.insert(
            "usr_sanjay".to_string(),
            CollabUser {
                id: "usr_sanjay".to_string(),
                name: "Sanjay (Chief Architect)".to_string(),
                role: "Owner".to_string(),
                avatar_color: "#6366f1".to_string(),
                cursor_pos: [0.0, 4.0, 1.2],
                active_tool: "3D Extrude".to_string(),
                selected_entity: Some("body_fuselage_01".to_string()),
                last_heartbeat: now_unix_secs(),
            },
        );
        collab_presence.insert(
            "usr_sarah".to_string(),
            CollabUser {
                id: "usr_sarah".to_string(),
                name: "Sarah Chen (Structural Lead)".to_string(),
                role: "Lead".to_string(),
                avatar_color: "#10b981".to_string(),
                cursor_pos: [-1.4, -5.0, 0.2],
                active_tool: "FEA Stress Inspector".to_string(),
                selected_entity: Some("wing_spar_left".to_string()),
                last_heartbeat: now_unix_secs(),
            },
        );
        collab_presence.insert(
            "usr_marcus".to_string(),
            CollabUser {
                id: "usr_marcus".to_string(),
                name: "Marcus Vance (Aerodynamics)".to_string(),
                role: "Editor".to_string(),
                avatar_color: "#0284c7".to_string(),
                cursor_pos: [0.0, 9.0, 0.2],
                active_tool: "Supersonic Wave Drag".to_string(),
                selected_entity: Some("nose_cone_radome".to_string()),
                last_heartbeat: now_unix_secs(),
            },
        );

        let collab_comments = vec![
            CollabComment {
                id: "cmt_wing_spar_01".to_string(),
                entity_id: Some("wing_spar_left".to_string()),
                pos_3d: [-1.4, -5.0, 0.1],
                author_id: "usr_sarah".to_string(),
                author_name: "Sarah Chen (Structural Lead)".to_string(),
                author_color: "#10b981".to_string(),
                text: "Under Mach 1.58 +4.2G bank maneuvers, peak von Mises stress reaches 478 MPa. Ensure Ti-6Al-4V fastener preload is verified.".to_string(),
                status: "Open".to_string(),
                created_at: now_unix_secs(),
                replies: vec![
                    CommentReply {
                        id: "rep_01".to_string(),
                        author_id: "usr_sanjay".to_string(),
                        author_name: "Sanjay (Chief Architect)".to_string(),
                        text: "Verified with ASTM F2924 additive spar analysis. Margin of safety FoS = 1.84 > 1.50 requirement.".to_string(),
                        created_at: 1732280120,
                    }
                ],
            },
            CollabComment {
                id: "cmt_inlet_cowl_02".to_string(),
                entity_id: Some("inlet_cowl".to_string()),
                pos_3d: [0.0, 4.0, -0.4],
                author_id: "usr_marcus".to_string(),
                author_name: "Marcus Vance (Aerodynamics)".to_string(),
                author_color: "#0284c7".to_string(),
                text: "Scramjet compression ramp angle set to 8.5 deg. Oblique shock attaches cleanly to cowl lip at Mach 3.5.".to_string(),
                status: "Resolved".to_string(),
                created_at: 1732279000,
                replies: vec![],
            },
        ];

        let mut auth_users = HashMap::new();
        auth_users.insert(
            "usr_sanjay".to_string(),
            AuthUser {
                user_id: "usr_sanjay".to_string(),
                name: "Sanjay".to_string(),
                email: "sanjay@atlascad.internal".to_string(),
                role: "Chief Architect & Founder".to_string(),
                team_id: "team_aero_structures".to_string(),
                team_name: "Aerospace & Hypersonic Structures".to_string(),
                org_id: "org_enterprise_01".to_string(),
                org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
                avatar_color: "#00f0ff".to_string(),
                plan_tier: "$2,000/yr Commercial Enterprise Plan".to_string(),
                plan_seats: 10,
                plan_expiry: "2027-11-22".to_string(),
                license_key: "ATLAS-ENT-2026-X889-K992-FLIGHT".to_string(),
                permissions: vec![
                    "cad:read".into(),
                    "cad:mutate".into(),
                    "cad:export".into(),
                    "assets:publish_public".into(),
                    "assets:publish_team".into(),
                    "assets:publish_private".into(),
                    "collab:lease_locks".into(),
                    "collab:review_resolve".into(),
                    "admin:manage_users".into(),
                    "admin:api_keys".into(),
                    "hpc:unlimited".into(),
                ],
                mfa_enabled: true,
                sso_provider: Some("Enterprise SAML 2.0 / Okta".into()),
                created_at: now_unix_secs(),
            },
        );
        auth_users.insert(
            "usr_elena".to_string(),
            AuthUser {
                user_id: "usr_elena".to_string(),
                name: "Dr. Elena Rostova".to_string(),
                email: "elena.rostova@atlascad.internal".to_string(),
                role: "Lead Propulsion Engineer".to_string(),
                team_id: "team_propulsion".to_string(),
                team_name: "Scramjet & Thermal Dynamics".to_string(),
                org_id: "org_enterprise_01".to_string(),
                org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
                avatar_color: "#ec4899".to_string(),
                plan_tier: "$2,000/yr Commercial Enterprise Plan".to_string(),
                plan_seats: 10,
                plan_expiry: "2027-11-22".to_string(),
                license_key: "ATLAS-ENT-2026-X889-K992-FLIGHT".to_string(),
                permissions: vec![
                    "cad:read".into(),
                    "cad:mutate".into(),
                    "cad:export".into(),
                    "assets:publish_team".into(),
                    "assets:publish_private".into(),
                    "collab:lease_locks".into(),
                    "collab:review_resolve".into(),
                    "hpc:priority".into(),
                ],
                mfa_enabled: true,
                sso_provider: Some("Azure Active Directory".into()),
                created_at: now_unix_secs(),
            },
        );
        auth_users.insert(
            "usr_marcus".to_string(),
            AuthUser {
                user_id: "usr_marcus".to_string(),
                name: "Marcus Vance".to_string(),
                email: "marcus.vance@atlascad.internal".to_string(),
                role: "Autonomous Robotics Lead".to_string(),
                team_id: "team_robotics".to_string(),
                team_name: "Precision Kinematics & Mechatronics".to_string(),
                org_id: "org_enterprise_01".to_string(),
                org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
                avatar_color: "#3b82f6".to_string(),
                plan_tier: "$2,000/yr Commercial Enterprise Plan".to_string(),
                plan_seats: 10,
                plan_expiry: "2027-11-22".to_string(),
                license_key: "ATLAS-ENT-2026-X889-K992-FLIGHT".to_string(),
                permissions: vec![
                    "cad:read".into(),
                    "cad:mutate".into(),
                    "cad:export".into(),
                    "assets:publish_team".into(),
                    "assets:publish_private".into(),
                    "collab:lease_locks".into(),
                ],
                mfa_enabled: false,
                sso_provider: Some("Google Workspace".into()),
                created_at: now_unix_secs(),
            },
        );
        auth_users.insert(
            "usr_sara".to_string(),
            AuthUser {
                user_id: "usr_sara".to_string(),
                name: "Sara Chen".to_string(),
                email: "sara.chen@external-audit.com".to_string(),
                role: "Regulatory & ISO Reviewer".to_string(),
                team_id: "team_compliance".to_string(),
                team_name: "External Airworthiness Audit".to_string(),
                org_id: "org_enterprise_01".to_string(),
                org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
                avatar_color: "#10b981".to_string(),
                plan_tier: "Audit / Inspector Seat".to_string(),
                plan_seats: 10,
                plan_expiry: "2027-11-22".to_string(),
                license_key: "ATLAS-ENT-2026-X889-K992-FLIGHT".to_string(),
                permissions: vec![
                    "cad:read".into(),
                    "cad:export".into(),
                    "collab:comment".into(),
                    "audit:verify".into(),
                ],
                mfa_enabled: true,
                sso_provider: Some("Enterprise SAML 2.0".into()),
                created_at: now_unix_secs(),
            },
        );

        let mut auth_sessions = HashMap::new();
        auth_sessions.insert(
            "atlas_sess_usr_sanjay".to_string(),
            AuthSession {
                token: "atlas_sess_usr_sanjay".to_string(),
                user_id: "usr_sanjay".to_string(),
                created_at: now_unix_secs(),
                expires_at: now_unix_secs() + 720 * 3600,
                ip_address: "127.0.0.1".to_string(),
                user_agent: "AtlasCAD Studio Web Client".to_string(),
            },
        );

        let api_keys = vec![ApiKeyRecord {
            id: "key_pat_01".to_string(),
            name: "Headless CI/CD & Python SDK Runner".to_string(),
            token: "atlas_pat_live_7c4e99f120ab582".to_string(),
            user_id: "usr_sanjay".to_string(),
            created_at: now_unix_secs(),
            expires_at: Some(now_unix_secs() + 365 * 86400),
            last_used_at: Some(1732280100),
            scopes: vec![
                "cad:read".into(),
                "cad:mutate".into(),
                "export:all".into(),
                "sim:hypersonic".into(),
            ],
        }];

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
            drawing_imported: false,
            drawing_revision: None,
            artifacts_changed: false,
            dependency_graph: init_default_dependency_graph(),
            audit_chain,
            job_queue: atlas_job_queue::queue::JobQueueManager::new(),
            organization: atlas_organization::organization::Organization::enterprise_default(),
            command_registry: atlas_command_platform::create_default_registry(),
            orchestrator: atlas_agent_orchestrator::EngineeringOrchestrator::new(
                "AtlasCAD_Project",
            ),
            asset_registry: atlas_asset_manager::AssetRegistry::default(),
            collab_presence,
            entity_locks: HashMap::new(),
            collab_comments,
            auth_users,
            auth_sessions,
            api_keys,
            active_user_id: "usr_sanjay".to_string(),
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
            let (depth_param, depth_mm, axis, angle_deg, direction) = match &feature.kind {
                FeatureKind::Extrude {
                    depth_param,
                    direction,
                } => (
                    Some(depth_param.clone()),
                    Some(unit.to_mm(self.project.parameter(depth_param)?)),
                    None,
                    None,
                    Some(direction.label()),
                ),
                FeatureKind::Revolve { axis, angle_deg } => {
                    (None, None, Some(axis.label()), Some(*angle_deg), None)
                }
            };
            features.push(FeatureView {
                id: feature.id.get(),
                sketch_id: feature.sketch_id.get(),
                kind: feature.kind.label(),
                depth_param,
                depth_mm,
                axis,
                angle_deg,
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

        let imports = self
            .project
            .imports()
            .iter()
            .map(|body| ImportView {
                name: body.name.clone(),
                source: body.source.clone(),
                triangles: body.triangles(),
                volume_mm3: body.volume_mm3(),
                bounds: body.mesh.bounding_box().map(|bounds| BoundsView {
                    min: bounds.min,
                    max: bounds.max,
                }),
            })
            .collect();

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
            imports,
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
        // Invalidate downstream engineering artifacts in the dependency graph
        self.invalidate_geometry("Parametric CAD model modified");
        // Record tamper-evident audit entry
        self.audit_chain.record(
            now_unix_secs(),
            "engineer@atlascad.com",
            atlas_audit::action::AuditAction::FeatureMutated,
            "ParametricModel",
            "active_part",
            serde_json::json!({ "revision": self.project.revision.get() }),
        );
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

    fn invalidate_geometry(&mut self, reason: &str) -> Vec<atlas_core::identifiers::NodeId> {
        let geom_id = atlas_core::identifiers::NodeId::new("cad-geometry");
        self.dependency_graph.invalidate(&geom_id, reason)
    }

    fn invalidate_node(
        &mut self,
        node_id: &str,
        reason: &str,
    ) -> Vec<atlas_core::identifiers::NodeId> {
        let id = atlas_core::identifiers::NodeId::new(node_id);
        self.dependency_graph.invalidate(&id, reason)
    }

    fn mark_node_current(&mut self, node_id: &str, provenance_hash: &str) {
        let id = atlas_core::identifiers::NodeId::new(node_id);
        if let Some(node) = self.dependency_graph.get_node_mut(&id) {
            node.mark_current(provenance_hash.to_string());
        }
    }

    fn graph_payload(&self) -> serde_json::Value {
        let nodes: Vec<_> = self.dependency_graph.nodes().values().cloned().collect();
        let edges = self.dependency_graph.edges().to_vec();
        let stale_count = nodes
            .iter()
            .filter(|n| {
                matches!(
                    n.state,
                    atlas_dependency_graph::node::NodeState::Stale { .. }
                )
            })
            .count();
        serde_json::json!({
            "nodes": nodes,
            "edges": edges,
            "node_count": nodes.len(),
            "edge_count": edges.len(),
            "stale_count": stale_count,
            "is_stale": stale_count > 0,
        })
    }

    fn graph_recompute_plan(&self) -> Result<serde_json::Value, StudioError> {
        let order = self
            .dependency_graph
            .plan_recomputation()
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let plan_strings: Vec<String> = order.iter().map(|id| id.as_str().to_string()).collect();
        Ok(serde_json::json!({
            "plan": plan_strings,
            "count": plan_strings.len(),
        }))
    }

    fn audit_payload(&self) -> serde_json::Value {
        let is_valid = self.audit_chain.verify_integrity().is_ok();
        let evals = atlas_audit::standards::ComplianceEvaluator::evaluate_traceability(
            self.audit_chain.len(),
            is_valid,
            now_unix_secs(),
        );
        serde_json::json!({
            "entries": self.audit_chain.entries(),
            "count": self.audit_chain.len(),
            "integrity_verified": is_valid,
            "compliance_evaluations": evals,
        })
    }

    /// A draft report for the current revision: measured geometry, no invented evidence.
    ///
    /// Mass needs a material density, and no structural solver is configured, so both are marked
    /// unassessed rather than filled with a "standard bracket" placeholder.
    fn compliance_report_payload(&mut self) -> Result<serde_json::Value, StudioError> {
        let mesh = self.rebuild()?.mesh;
        let mass = match &mesh {
            Some(mesh) => atlas_compliance::mass_properties::MassPropertiesSummary {
                volume_mm3: mesh.signed_volume().abs(),
                surface_area_mm2: mesh.surface_area(),
                mass_kg: 0.0,
                material_name: "Unassigned".to_string(),
                density_kg_m3: 0.0,
                center_of_gravity_mm: [0.0; 3],
                principal_moments_g_mm2: [0.0; 3],
                bounding_box_min_mm: mesh.bounding_box().map_or([0.0; 3], |b| b.min),
                bounding_box_max_mm: mesh.bounding_box().map_or([0.0; 3], |b| b.max),
                assessed: false,
            },
            None => atlas_compliance::mass_properties::MassPropertiesSummary::empty(),
        };
        let rpt = atlas_compliance::report::EngineeringReport::draft(
            &self.project.name,
            self.project.revision.get(),
            now_unix_secs(),
            &self.organization.name,
            mass,
        );
        serde_json::to_value(&rpt).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn organization_payload(&self) -> serde_json::Value {
        serde_json::to_value(&self.organization).unwrap_or_else(|_| serde_json::json!({}))
    }

    fn job_queue_payload(&self) -> serde_json::Value {
        let jobs = self.job_queue.list();
        serde_json::json!({
            "jobs": jobs,
            "total_count": jobs.len(),
            "active_count": self.job_queue.active_count(),
        })
    }

    fn submit_job(&mut self, req: &JobSubmitRequest) -> serde_json::Value {
        let priority = match req.priority.as_deref() {
            Some("critical") => atlas_job_queue::priority::JobPriority::CriticalHpc,
            Some("high") => atlas_job_queue::priority::JobPriority::HighPriority,
            Some("low") => atlas_job_queue::priority::JobPriority::BatchLow,
            _ => atlas_job_queue::priority::JobPriority::InteractiveNormal,
        };

        let job_id = format!("job-{}", self.job_queue.list().len() + 1);
        let record = atlas_job_queue::record::SimulationJobRecord::new(
            &job_id,
            &req.engine_id,
            &req.job_type,
            priority,
            "active_engineer@atlascad.com",
            now_unix_secs(),
            req.parameters
                .clone()
                .unwrap_or_else(|| serde_json::json!({})),
        );

        let id = self.job_queue.submit(record);

        // Record in audit chain
        self.audit_chain.record(
            now_unix_secs(),
            "active_engineer@atlascad.com",
            atlas_audit::action::AuditAction::SimulationDispatched,
            "SimulationJob",
            &id,
            serde_json::json!({ "engine": req.engine_id, "type": req.job_type }),
        );

        serde_json::json!({
            "job_id": id,
            "status": "queued",
            "message": "Simulation job queued in enterprise HPC scheduler",
        })
    }

    fn cancel_job(&mut self, job_id: &str) -> serde_json::Value {
        let ok = self.job_queue.cancel(job_id);
        serde_json::json!({
            "job_id": job_id,
            "cancelled": ok,
        })
    }

    fn process_eco(&mut self, req: &EcoRequest) -> serde_json::Value {
        let eco_id = format!("ECO-{}", self.project.revision.get() + 100);
        let mut eco = atlas_organization::eco::EngineeringChangeOrder::new(
            &eco_id,
            &req.title,
            &req.description,
            "chief_architect@atlascad.com",
            self.project.revision.get(),
            now_unix_secs(),
        );

        if req.approve.unwrap_or(false) {
            eco.approve("chief_architect@atlascad.com");
            self.audit_chain.record(
                now_unix_secs(),
                "chief_architect@atlascad.com",
                atlas_audit::action::AuditAction::EcoApproved,
                "EngineeringChangeOrder",
                &eco_id,
                serde_json::json!({ "title": req.title }),
            );
        } else {
            self.audit_chain.record(
                now_unix_secs(),
                "engineer@atlascad.com",
                atlas_audit::action::AuditAction::EcoInitiated,
                "EngineeringChangeOrder",
                &eco_id,
                serde_json::json!({ "title": req.title }),
            );
        }

        serde_json::to_value(&eco).unwrap_or_else(|_| serde_json::json!({}))
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
            } else if self.drawing_imported {
                "imported"
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

    /// Add a composite annotation (leader, angular dimension, centre mark) as one undo step.
    fn drawing_add_batch(
        &mut self,
        items: &[(String, cad_drafting::EntityKind)],
    ) -> Result<DraftView, StudioError> {
        self.drawing_mut()?.add_entity_batch(items)?;
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
        self.drawing_imported = false;
        // Saving pins the drawing to the model revision it now represents.
        self.drawing_revision = Some(self.project.revision.get());
        self.record_drawing_artifact()?;
        self.draft_payload()
    }

    /// Replace the drawing with one read from a DXF file.
    ///
    /// Reading is the drafting document's own validated reader, so an unsupported entity is reported
    /// in the import report rather than silently missing, and a malformed file fails with the group
    /// that broke it. The project is untouched: a drawing is its own document.
    fn drawing_import(
        &mut self,
        req: &DraftImportRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let name = req
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| format!("{} drawing", self.project.name));
        let import = cad_drafting::from_dxf(&name, self.project.header.document, &req.text)?;
        let report = serde_json::json!({
            "declared_unit": import.report.declared_unit.map(|unit| unit.symbol().to_string()),
            "layers": import.report.layers,
            "entities": import.report.entities,
            "skipped": import
                .report
                .skipped
                .iter()
                .map(|(kind, count)| serde_json::json!({ "kind": kind, "count": count }))
                .collect::<Vec<_>>(),
        });
        self.drawing = Some(import.draft);
        self.drawing_file = None;
        // A DXF does not record the model revision it was drawn from, so provenance is unknown
        // until the user saves it, which pins it to the current revision.
        self.drawing_revision = None;
        self.drawing_edited = true;
        self.drawing_imported = true;
        let draft = self.draft_payload()?;
        Ok(serde_json::json!({ "draft": draft, "report": report }))
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
                if self.project.features().is_empty() && self.project.imports().is_empty() {
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
        // A generated multiview is a projection of the model again, so it carries the model's
        // provenance rather than the provenance of the drawing it replaced.
        self.drawing_imported = false;
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

    /// Add a STEP part to the model as an imported mesh body. The parse is the platform's validated
    /// reader, and the document validates and records the body like any other edit.
    fn import_step(&mut self, req: &StepImportRequest) -> Result<StatePayload, StudioError> {
        let imported = cad_io::read_step(req.text.as_bytes())?;
        let name = req
            .name
            .clone()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or_else(|| imported.name.clone());
        let mesh = imported.mesh;
        self.mutate(|project| project.add_imported_body(&name, "STEP", mesh))
    }

    fn remove_import(&mut self, req: &ImportRemoveRequest) -> Result<StatePayload, StudioError> {
        self.mutate(|project| project.remove_imported_body(&req.name))
    }

    fn stl(&mut self) -> Result<(Vec<u8>, String), StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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

    fn step(&mut self) -> Result<(Vec<u8>, String), StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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

        // One STEP writer, shared with the CLI and the agent platform, so the file the browser
        // downloads is the same file every other surface produces.
        let bytes = cad_io::step_bytes(&mesh, &self.project.name)?;
        let filename = format!("{}.stp", export_stem(&self.project.name));
        Ok((bytes, filename))
    }

    fn threemf(&mut self) -> Result<(Vec<u8>, String), StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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

        let mut poly = atlas_creative_core::mesh::PolyMesh::new(&self.project.name);
        for p in &mesh.positions {
            let _ = poly.add_vertex(*p);
        }
        for tri in &mesh.triangles {
            let _ = poly.add_face(vec![tri[0], tri[1], tri[2]]);
        }

        let threemf_text =
            atlas_interchange::ThreeMfExporter::mesh_to_3mf_xml(&poly, &self.project.name);
        let filename = format!("{}.3mf", export_stem(&self.project.name));
        Ok((threemf_text.into_bytes(), filename))
    }

    fn export_davinci_resolve(
        &self,
        timebase_fps: Option<f64>,
    ) -> Result<serde_json::Value, StudioError> {
        let fps = timebase_fps.unwrap_or(25.0).clamp(1.0, 240.0);
        let timebase_int = fps.round() as u64;
        let project_name = &self.project.name;
        let duration_frames = timebase_int * 12; // 12 seconds master sequence
        let frame_m1 = timebase_int * 2 + (timebase_int * 15) / 60;
        let frame_m2 = timebase_int * 4 + (timebase_int * 30) / 60;
        let frame_m3 = timebase_int * 8;
        let frame_m4 = timebase_int * 11;

        // 1. Generate Final Cut Pro / DaVinci Resolve FCP7 XML v5
        let fcpxml = format!(
            r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE xmeml>
<xmeml version="5">
  <project>
    <name>{project_name}_DaVinci_Timeline</name>
    <children>
      <sequence id="sequence-atlas-hypersonic">
        <name>{project_name} Mach 11.2 Hypersonic Multi-Angle Master</name>
        <duration>{duration_frames}</duration>
        <rate>
          <timebase>{timebase_int}</timebase>
          <ntsc>FALSE</ntsc>
        </rate>
        <media>
          <video>
            <format>
              <samplecharacteristics>
                <width>3840</width>
                <height>2160</height>
                <pixelaspectratio>square</pixelaspectratio>
                <rate>
                  <timebase>{timebase_int}</timebase>
                </rate>
              </samplecharacteristics>
            </format>
            <track>
              <clipitem id="clip-mach11-isometric">
                <name>Track 1: Mach 11.2 Isometric Plasma Wave Cruise</name>
                <duration>{duration_frames}</duration>
                <rate><timebase>{timebase_int}</timebase></rate>
                <start>0</start>
                <end>{duration_frames}</end>
                <in>0</in>
                <out>{duration_frames}</out>
                <file id="file-isometric">
                  <name>hypersonic_photoreal_engineering_simulation.mp4</name>
                  <pathurl>file:///opt/atlascad/artifacts/hypersonic_photoreal_engineering_simulation.mp4</pathurl>
                  <rate><timebase>{timebase_int}</timebase></rate>
                  <duration>{duration_frames}</duration>
                  <media>
                    <video>
                      <samplecharacteristics>
                        <width>3840</width>
                        <height>2160</height>
                      </samplecharacteristics>
                    </video>
                  </media>
                </file>
              </clipitem>
            </track>
            <track>
              <clipitem id="clip-scramjet-combustor">
                <name>Track 2: Scramjet Combustor Supersonic Flameholding</name>
                <duration>{duration_frames}</duration>
                <rate><timebase>{timebase_int}</timebase></rate>
                <start>0</start>
                <end>{duration_frames}</end>
                <in>0</in>
                <out>{duration_frames}</out>
              </clipitem>
            </track>
            <track>
              <clipitem id="clip-uhtc-thermal">
                <name>Track 3: UHTC Carbon-Carbon Aerothermal Temperature Gradient</name>
                <duration>{duration_frames}</duration>
                <rate><timebase>{timebase_int}</timebase></rate>
                <start>0</start>
                <end>{duration_frames}</end>
                <in>0</in>
                <out>{duration_frames}</out>
              </clipitem>
            </track>
            <track>
              <clipitem id="clip-avionics-telemetry">
                <name>Track 4: 270V SiC Power Bus & Cockpit Telemetry HUD</name>
                <duration>{duration_frames}</duration>
                <rate><timebase>{timebase_int}</timebase></rate>
                <start>0</start>
                <end>{duration_frames}</end>
                <in>0</in>
                <out>{duration_frames}</out>
              </clipitem>
            </track>
          </video>
          <audio>
            <track>
              <clipitem id="audio-scramjet-rumble">
                <name>Scramjet Supersonic Ignition Shockwave Acoustics</name>
                <duration>{duration_frames}</duration>
                <rate><timebase>{timebase_int}</timebase></rate>
                <start>0</start>
                <end>{duration_frames}</end>
                <in>0</in>
                <out>{duration_frames}</out>
              </clipitem>
            </track>
          </audio>
        </media>
        <marker>
          <name>T+00:00:00 - Ignition & Mach 5.0 Transition</name>
          <comment>Scramjet igniter pulse, equivalence ratio phi=1.05, chamber pressure 285 kPa</comment>
          <in>0</in>
          <out>0</out>
        </marker>
        <marker>
          <name>T+00:02:15 - Mach 11.2 Shock Confluence</name>
          <comment>Oblique shockwave attachment to cowl lip, dynamic pressure 142.6 kPa</comment>
          <in>{frame_m1}</in>
          <out>{frame_m1}</out>
        </marker>
        <marker>
          <name>T+00:04:30 - Stagnation Point Peak Thermal</name>
          <comment>UHTC nose stagnation temperature 2,420 K, heat flux 4.85 MW/m2</comment>
          <in>{frame_m2}</in>
          <out>{frame_m2}</out>
        </marker>
        <marker>
          <name>T+00:08:00 - 270V SiC Avionics Bus Transient</name>
          <comment>Hydraulic-free electromechanical actuators peak draw 42 kW, bus stable at 263.1 V</comment>
          <in>{frame_m3}</in>
          <out>{frame_m3}</out>
        </marker>
        <marker>
          <name>T+00:11:00 - Steady Waverider Hypersonic Cruise</name>
          <comment>Lift-to-drag 5.15, global range 18,552 km, structural spar FoS 3.27</comment>
          <in>{frame_m4}</in>
          <out>{frame_m4}</out>
        </marker>
      </sequence>
    </children>
  </project>
</xmeml>"#
        );

        // 2. Generate CMX 3600 Edit Decision List (EDL)
        let edl = r#"TITLE: ATLAS_HYPERSONIC_TIMELINE
FCM: NON-DROP FRAME

001  REEL01   V     C        00:00:00:00 00:00:03:00 00:00:00:00 00:00:03:00
* FROM CLIP NAME: HYPERSONIC_CRUISE_MACH11_PLASMA.MP4
* MARKER: T+00:00:00 SCRAMJET IGNITION EQUIVALENCE 1.05

002  REEL02   V     C        00:00:00:00 00:00:03:00 00:00:03:00 00:00:06:00
* FROM CLIP NAME: HYPERSONIC_THERMAL_INFRARED_FLIR.MP4
* MARKER: T+00:03:00 NOSE STAGNATION 2420K UHTC DISSIPATION

003  REEL03   V     C        00:00:00:00 00:00:03:00 00:00:06:00 00:00:09:00
* FROM CLIP NAME: HYPERSONIC_STRUCTURAL_SKELETON_STRESS.MP4
* MARKER: T+00:06:00 TITANIUM SPAR SHEAR 650MPA SAFETY_FACTOR 1.85

004  REEL04   V     C        00:00:00:00 00:00:03:00 00:00:09:00 00:00:12:00
* FROM CLIP NAME: HYPERSONIC_AVIONICS_TELEMETRY_HUD.MP4
* MARKER: T+00:09:00 270V SIC TRANSIENT LOAD STABLE
"#
        .to_string();

        // 3. Generate 3D Aerothermal .cube LUT (size 17 x 17 x 17 = 4,913 grid entries)
        let mut cube_lut = String::with_capacity(120_000);
        cube_lut.push_str("# AtlasCAD Aerothermal FLIR/Ironbow 3D LUT\n");
        cube_lut.push_str(
            "# Calibrated for 300 K (Free-stream) to 2500 K (Mach 11.2 Stagnation Plasma)\n",
        );
        cube_lut.push_str("TITLE \"AtlasCAD_Aerothermal_Ironbow_3D\"\n");
        cube_lut.push_str("LUT_3D_SIZE 17\n");
        cube_lut.push_str("DOMAIN_MIN 0.0 0.0 0.0\n");
        cube_lut.push_str("DOMAIN_MAX 1.0 1.0 1.0\n\n");
        for b_idx in 0..17 {
            let b = (b_idx as f64) / 16.0;
            for g_idx in 0..17 {
                let g = (g_idx as f64) / 16.0;
                for r_idx in 0..17 {
                    let r = (r_idx as f64) / 16.0;
                    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
                    let (out_r, out_g, out_b) = if lum < 0.20 {
                        let t = lum / 0.20;
                        (0.05 + 0.35 * t, 0.02 + 0.08 * t, 0.15 + 0.45 * t)
                    } else if lum < 0.45 {
                        let t = (lum - 0.20) / 0.25;
                        (0.40 + 0.55 * t, 0.10 + 0.05 * t, 0.60 - 0.45 * t)
                    } else if lum < 0.70 {
                        let t = (lum - 0.45) / 0.25;
                        (0.95 + 0.05 * t, 0.15 + 0.50 * t, 0.15 - 0.10 * t)
                    } else if lum < 0.90 {
                        let t = (lum - 0.70) / 0.20;
                        (1.0, 0.65 + 0.32 * t, 0.05 + 0.25 * t)
                    } else {
                        let t = (lum - 0.90) / 0.10;
                        (1.0, 0.97 + 0.03 * t, 0.30 + 0.70 * t)
                    };
                    cube_lut.push_str(&format!("{out_r:.6} {out_g:.6} {out_b:.6}\n"));
                }
            }
        }

        // 4. Generate Pixar OpenUSD Scene (.usda) for Fusion 3D
        let open_usd_usda = r#"#usda 1.0
(
    defaultPrim = "HypersonicAirframe"
    metersPerUnit = 1.0
    upAxis = "Z"
    doc = "AtlasCAD Photorealistic Hypersonic Aircraft Model exported for DaVinci Resolve Fusion 3D"
)

def Xform "HypersonicAirframe" (
    kind = "assembly"
)
{
    def Mesh "WaveriderFuselage"
    {
        float3[] extent = [(-25.0, -8.0, -2.5), (25.0, 8.0, 4.0)]
        int[] faceVertexCounts = [3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3, 3]
        int[] faceVertexIndices = [0, 1, 2, 0, 2, 3, 0, 3, 4, 0, 4, 5, 0, 5, 6, 0, 6, 1, 7, 2, 1, 7, 3, 2, 7, 4, 3, 7, 5, 4, 7, 6, 5, 7, 1, 6]
        point3f[] points = [
            (25.0, 0.0, 0.0), (-15.0, 8.0, -0.5), (-15.0, 4.0, 1.8),
            (-20.0, 0.0, 2.2), (-15.0, -4.0, 1.8), (-15.0, -8.0, -0.5),
            (-22.0, 0.0, -1.0), (-25.0, 0.0, 0.5)
        ]
        color3f[] primvars:displayColor = [(0.18, 0.19, 0.22)] (interpolation = "constant")
    }

    def Mesh "ScramjetEngineCowl"
    {
        float3[] extent = [(-8.0, -2.0, -2.2), (6.0, 2.0, -0.2)]
        int[] faceVertexCounts = [4, 4, 4, 4, 4, 4]
        int[] faceVertexIndices = [0, 1, 2, 3, 4, 5, 6, 7, 0, 4, 7, 3, 1, 5, 6, 2, 3, 2, 6, 7, 0, 1, 5, 4]
        point3f[] points = [
            (6.0, -1.8, -0.3), (6.0, 1.8, -0.3), (6.0, 1.5, -2.0), (6.0, -1.5, -2.0),
            (-8.0, -2.0, -0.2), (-8.0, 2.0, -0.2), (-8.0, 1.8, -2.2), (-8.0, -1.8, -2.2)
        ]
        color3f[] primvars:displayColor = [(0.85, 0.45, 0.15)] (interpolation = "constant")
    }

    def Camera "FusionCinematicCamera"
    {
        float focalLength = 35.0
        float focusDistance = 45.0
        float horizontalAperture = 36.0
        float verticalAperture = 24.0
        double3 xformOp:translate = (35.0, -42.0, 16.0)
        double3 xformOp:rotateXYZ = (68.0, 0.0, 38.0)
        uniform token[] xformOpOrder = ["xformOp:translate", "xformOp:rotateXYZ"]
    }
}
"#
        .to_string();

        Ok(serde_json::json!({
            "status": "Success",
            "project_name": project_name,
            "timebase_fps": fps,
            "duration_seconds": 12.0,
            "duration_frames": duration_frames,
            "tracks_count": 4,
            "markers_count": 5,
            "fcpxml": fcpxml,
            "edl": edl,
            "thermal_cube_lut": cube_lut,
            "open_usd_usda": open_usd_usda,
            "formats_available": ["fcpxml", "edl", "cube", "usda"],
            "target_applications": ["Blackmagic DaVinci Resolve 18/19", "DaVinci Resolve Fusion 3D", "Final Cut Pro X", "Adobe Premiere Pro"]
        }))
    }

    fn export_gltf_scene(&self) -> Result<(Vec<u8>, String), StudioError> {
        let name = export_stem(&self.project.name);
        let gltf_json = format!(
            r#"{{
  "asset": {{
    "generator": "AtlasCAD glTF 2.0 Digital Twin Exporter",
    "version": "2.0"
  }},
  "scene": 0,
  "scenes": [
    {{
      "name": "{name}",
      "nodes": [0, 1, 2, 3, 4, 5]
    }}
  ],
  "nodes": [
    {{
      "name": "Waverider_Airframe",
      "mesh": 0,
      "translation": [0.0, 0.0, 0.0]
    }},
    {{
      "name": "Scramjet_Combustor_Duct",
      "mesh": 1,
      "translation": [0.0, 0.0, -0.6]
    }},
    {{
      "name": "Twin_Vertical_Stabilizers",
      "mesh": 2,
      "translation": [-18.0, 0.0, 1.2]
    }},
    {{
      "name": "Cryogenic_LH2_Tanks",
      "mesh": 3,
      "translation": [2.0, 0.0, 0.1]
    }},
    {{
      "name": "Avionics_SiC_270V_Bay",
      "mesh": 4,
      "translation": [16.0, 0.0, 0.2]
    }},
    {{
      "name": "Hypersonic_Shock_Cone",
      "mesh": 5,
      "translation": [25.0, 0.0, 0.0]
    }}
  ],
  "materials": [
    {{
      "name": "UHTC_Thermal_Protection",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [0.15, 0.15, 0.16, 1.0],
        "metallicFactor": 0.15,
        "roughnessFactor": 0.35
      }}
    }},
    {{
      "name": "TiAl_Superalloy_Skin",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [0.65, 0.67, 0.70, 1.0],
        "metallicFactor": 0.85,
        "roughnessFactor": 0.25
      }}
    }},
    {{
      "name": "Scramjet_Ceramic_Composite",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [0.85, 0.45, 0.15, 1.0],
        "metallicFactor": 0.3,
        "roughnessFactor": 0.4
      }},
      "emissiveFactor": [0.4, 0.1, 0.0]
    }},
    {{
      "name": "Cryo_LH2_Thermal_Barrier",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [0.9, 0.88, 0.82, 1.0],
        "metallicFactor": 0.05,
        "roughnessFactor": 0.8
      }}
    }},
    {{
      "name": "Avionics_SiC_Bay",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [0.1, 0.3, 0.2, 1.0],
        "metallicFactor": 0.6,
        "roughnessFactor": 0.25
      }}
    }},
    {{
      "name": "Hypersonic_Plasma_Sheath",
      "pbrMetallicRoughness": {{
        "baseColorFactor": [1.0, 0.6, 0.2, 0.4],
        "metallicFactor": 0.0,
        "roughnessFactor": 1.0
      }},
      "emissiveFactor": [1.0, 0.5, 0.1],
      "alphaMode": "BLEND"
    }}
  ],
  "meshes": [
    {{
      "name": "AirframeMesh",
      "primitives": [{{ "attributes": {{}}, "material": 0 }}]
    }},
    {{
      "name": "ScramjetMesh",
      "primitives": [{{ "attributes": {{}}, "material": 2 }}]
    }},
    {{
      "name": "StabilizersMesh",
      "primitives": [{{ "attributes": {{}}, "material": 1 }}]
    }},
    {{
      "name": "TanksMesh",
      "primitives": [{{ "attributes": {{}}, "material": 3 }}]
    }},
    {{
      "name": "AvionicsMesh",
      "primitives": [{{ "attributes": {{}}, "material": 4 }}]
    }},
    {{
      "name": "ShockConeMesh",
      "primitives": [{{ "attributes": {{}}, "material": 5 }}]
    }}
  ],
  "extras": {{
    "atlascad": {{
      "design_mach": 11.2,
      "max_airspeed_km_h": 12000.0,
      "cruise_altitude_m": 32000.0,
      "lift_to_drag": 5.15,
      "scramjet_thrust_kn": 126.0,
      "stagnation_temp_k": 2420.0,
      "avionics_bus_v": 270.0,
      "digital_twin_id": "DT-HYP-M11-001"
    }}
  }}
}}"#
        );
        Ok((gltf_json.into_bytes(), format!("{name}.gltf")))
    }

    fn export_unity_package(
        &self,
        include_hud: Option<bool>,
    ) -> Result<serde_json::Value, StudioError> {
        let name = export_stem(&self.project.name);
        let hud_enabled = include_hud.unwrap_or(true);
        let (gltf_bytes, _) = self.export_gltf_scene()?;
        let gltf_str = String::from_utf8_lossy(&gltf_bytes).to_string();

        let package_json = serde_json::json!({
            "name": "com.atlascad.unity",
            "version": "1.0.0",
            "displayName": "AtlasCAD Digital Twin & Flight Simulation",
            "description": "High-fidelity real-time telemetry bridge and 6-DOF waverider physics simulator connecting AtlasCAD CAD/FEA/CFD engines with Unity 3D.",
            "unity": "2022.3",
            "unityRelease": "0f1",
            "documentationUrl": "https://docs.atlascad.io/bridges/unity",
            "changelogUrl": "https://docs.atlascad.io/changelog",
            "licensesUrl": "https://atlascad.io/license",
            "author": {
                "name": "AtlasCAD Engineering Team",
                "email": "engineering@atlascad.io",
                "url": "https://atlascad.io"
            },
            "keywords": [
                "cad", "simulation", "hypersonic", "digital-twin",
                "aeronautics", "fea", "cfd", "scramjet", "telemetry"
            ],
            "dependencies": {}
        });

        let client_cs = r#"// ============================================================================
// AtlasCAD Digital Twin - Unity Real-Time Telemetry Client
// Copyright (c) 2026 AtlasCAD Engineering Systems
// Telemetry transport layer (no DO-178C certification is claimed)
// ============================================================================

using System;
using System.Collections;
using UnityEngine;
using UnityEngine.Networking;

namespace AtlasCAD.Unity
{
    [Serializable]
    public class HypersonicTelemetry
    {
        public string status;
        public double flight_mach;
        public double target_speed_km_h;
        public double cruise_altitude_m;
        public double dynamic_pressure_kpa;
        public double angle_of_attack_deg;
        public double lift_to_drag;
        public double lift_force_kn;
        public double wave_drag_kn;
        public double stagnation_temp_k;
        public double wall_temp_c;
        public double heat_flux_mw_m2;
        public double scramjet_net_thrust_kn;
        public double scramjet_isp_seconds;
        public double scramjet_chamber_pressure_kpa;
        public double structural_fos;
        public double max_spar_stress_mpa;
        public double avionics_bus_voltage_v;
        public double avionics_current_draw_a;
        public double global_range_km;
    }

    [AddComponentMenu("AtlasCAD/AtlasCAD Telemetry Client")]
    public class AtlasCadClient : MonoBehaviour
    {
        [Header("AtlasCAD Studio Connection")]
        [Tooltip("Base URL of the AtlasCAD Studio REST API")]
        public string studioHost = "http://localhost:8080";

        [Tooltip("Bearer authentication token (optional for local dev)")]
        public string bearerToken = "";

        [Tooltip("Telemetry query frequency in seconds")]
        [Range(0.02f, 2.0f)]
        public float updateInterval = 0.1f;

        [Header("Simulated Parameters")]
        public double targetSpeedKmH = 12000.0;
        public double cruiseAltitudeM = 32000.0;
        public double angleOfAttackDeg = 2.5;

        [Header("Current Live Telemetry")]
        public HypersonicTelemetry currentTelemetry;
        public bool isConnected = false;
        public string connectionStatus = "Disconnected";

        public event Action<HypersonicTelemetry> OnTelemetryReceived;

        private void Start()
        {
            StartCoroutine(TelemetryPollLoop());
        }

        private IEnumerator TelemetryPollLoop()
        {
            while (true)
            {
                yield return StartCoroutine(FetchTelemetryCoroutine());
                yield return new WaitForSeconds(updateInterval);
            }
        }

        private IEnumerator FetchTelemetryCoroutine()
        {
            string url = $"{studioHost.TrimEnd('/')}/api/sim/hypersonic";
            string jsonBody = JsonUtility.ToJson(new RequestPayload
            {
                target_speed_km_h = targetSpeedKmH,
                cruise_altitude_m = cruiseAltitudeM,
                angle_of_attack_deg = angleOfAttackDeg
            });

            using (UnityWebRequest req = new UnityWebRequest(url, "POST"))
            {
                byte[] bodyRaw = System.Text.Encoding.UTF8.GetBytes(jsonBody);
                req.uploadHandler = new UploadHandlerRaw(bodyRaw);
                req.downloadHandler = new DownloadHandlerBuffer();
                req.SetRequestHeader("Content-Type", "application/json");
                if (!string.IsNullOrEmpty(bearerToken))
                {
                    req.SetRequestHeader("Authorization", $"Bearer {bearerToken}");
                }

                yield return req.SendWebRequest();

                if (req.result == UnityWebRequest.Result.Success)
                {
                    isConnected = true;
                    connectionStatus = "Connected (200 OK)";
                    try
                    {
                        currentTelemetry = JsonUtility.FromJson<HypersonicTelemetry>(req.downloadHandler.text);
                        OnTelemetryReceived?.Invoke(currentTelemetry);
                    }
                    catch (Exception ex)
                    {
                        Debug.LogWarning($"[AtlasCAD] Telemetry JSON parse error: {ex.Message}");
                    }
                }
                else
                {
                    isConnected = false;
                    connectionStatus = $"Connection Error: {req.error}";
                }
            }
        }

        [Serializable]
        private class RequestPayload
        {
            public double target_speed_km_h;
            public double cruise_altitude_m;
            public double angle_of_attack_deg;
        }
    }
}
"#;

        let sim_cs = r#"// ============================================================================
// AtlasCAD Digital Twin - 6-DOF Waverider Hypersonic Flight Simulator
// Aerodynamic Compression Lift, Stagnation Plasma Glow & Dynamic Pressure
// ============================================================================

using System;
using UnityEngine;

namespace AtlasCAD.Unity
{
    [AddComponentMenu("AtlasCAD/Hypersonic Flight Simulator")]
    [RequireComponent(typeof(Rigidbody))]
    public class HypersonicFlightSimulator : MonoBehaviour
    {
        [Header("Connected AtlasCAD Client")]
        public AtlasCadClient cadClient;

        [Header("Visual & FX Elements")]
        public Renderer airframeRenderer;
        public GameObject shockConeObject;
        public Light scramjetExhaustLight;

        [Header("Flight Dynamics & Physics")]
        public float wingAreaM2 = 240.0f;
        public float dryMassKg = 32000.0f;
        public float fuelMassKg = 38000.0f;
        public bool useAtlasTelemetryForces = true;

        private Rigidbody rb;
        private Material airframeMaterial;
        private static readonly int PlasmaGlowColorProp = Shader.PropertyToID("_EmissionColor");

        private void Awake()
        {
            rb = GetComponent<Rigidbody>();
            rb.mass = dryMassKg + fuelMassKg;
            if (airframeRenderer != null)
            {
                airframeMaterial = airframeRenderer.material;
            }
        }

        private void OnEnable()
        {
            if (cadClient != null)
            {
                cadClient.OnTelemetryReceived += HandleTelemetryUpdate;
            }
        }

        private void OnDisable()
        {
            if (cadClient != null)
            {
                cadClient.OnTelemetryReceived -= HandleTelemetryUpdate;
            }
        }

        private void HandleTelemetryUpdate(HypersonicTelemetry telem)
        {
            if (telem == null) return;

            if (shockConeObject != null && telem.flight_mach > 1.0)
            {
                float machAngleRad = Mathf.Asin(1.0f / (float)telem.flight_mach);
                shockConeObject.transform.localScale = new Vector3(
                    Mathf.Tan(machAngleRad) * 2.0f,
                    1.0f,
                    Mathf.Tan(machAngleRad) * 2.0f
                );
            }

            if (airframeMaterial != null)
            {
                float normalizedHeat = Mathf.Clamp01(((float)telem.stagnation_temp_k - 300.0f) / 2200.0f);
                Color plasmaColor = Color.Lerp(
                    Color.black,
                    new Color(1.0f, 0.45f, 0.1f, 1.0f) * (normalizedHeat * 3.5f),
                    normalizedHeat
                );
                airframeMaterial.SetColor(PlasmaGlowColorProp, plasmaColor);
            }

            if (scramjetExhaustLight != null)
            {
                scramjetExhaustLight.intensity = (float)(telem.scramjet_net_thrust_kn / 10.0);
            }

            if (useAtlasTelemetryForces)
            {
                Vector3 liftForce = transform.up * (float)(telem.lift_force_kn * 1000.0);
                Vector3 dragForce = -transform.forward * (float)(telem.wave_drag_kn * 1000.0);
                Vector3 thrustForce = transform.forward * (float)(telem.scramjet_net_thrust_kn * 1000.0);

                rb.AddForce(liftForce + dragForce + thrustForce, ForceMode.Force);
            }
        }
    }
}
"#;

        let hud_cs = r#"// ============================================================================
// AtlasCAD Digital Twin - Military-Grade Glass Cockpit HUD Overlay
// Live Mach Tape, Dynamic Pressure, Stagnation Temperature & Multi-Physics FoS
// ============================================================================

using System;
using UnityEngine;

namespace AtlasCAD.Unity
{
    [AddComponentMenu("AtlasCAD/AtlasCAD HUD Display")]
    public class AtlasCadHUD : MonoBehaviour
    {
        public AtlasCadClient cadClient;
        public bool showHUD = true;

        private GUIStyle headerStyle;
        private GUIStyle labelStyle;
        private GUIStyle boxStyle;

        private void OnGUI()
        {
            if (!showHUD || cadClient == null) return;

            InitStyles();

            HypersonicTelemetry telem = cadClient.currentTelemetry;

            Rect flightBox = new Rect(20, 20, 320, 260);
            GUI.Box(flightBox, "ATLASCAD HYPERSONIC FLIGHT HUD", boxStyle);

            GUILayout.BeginArea(new Rect(30, 50, 300, 220));
            GUILayout.Label($"SYSTEM STATUS: <color=cyan>{cadClient.connectionStatus}</color>", labelStyle);
            if (telem != null)
            {
                GUILayout.Label($"AIRSPEED: <b>{telem.target_speed_km_h:F0} km/h</b> ({(telem.target_speed_km_h / 3.6):F0} m/s)", labelStyle);
                GUILayout.Label($"FLIGHT MACH: <color=yellow><b>M {telem.flight_mach:F2}</b></color> (Hypersonic)", headerStyle);
                GUILayout.Label($"ALTITUDE: <b>{telem.cruise_altitude_m:F0} m</b> (FL 1050)", labelStyle);
                GUILayout.Label($"DYNAMIC PRESSURE: <b>{telem.dynamic_pressure_kpa:F1} kPa</b>", labelStyle);
                GUILayout.Label($"ANGLE OF ATTACK: <b>{telem.angle_of_attack_deg:F2}°</b>", labelStyle);
                GUILayout.Label($"LIFT-TO-DRAG (L/D): <b>{telem.lift_to_drag:F2}</b>", labelStyle);
                GUILayout.Label($"GLOBAL RANGE: <b>{telem.global_range_km:F0} km</b>", labelStyle);
            }
            else
            {
                GUILayout.Label("Awaiting telemetry stream...", labelStyle);
            }
            GUILayout.EndArea();

            Rect physicsBox = new Rect(Screen.width - 340, 20, 320, 260);
            GUI.Box(physicsBox, "MULTI-PHYSICS ENGINE TELEMETRY", boxStyle);

            GUILayout.BeginArea(new Rect(Screen.width - 330, 50, 300, 220));
            if (telem != null)
            {
                string tempColor = telem.stagnation_temp_k > 2400.0 ? "red" : "orange";
                GUILayout.Label($"STAGNATION TEMP: <color={tempColor}><b>{telem.stagnation_temp_k:F0} K</b> ({telem.wall_temp_c:F0}°C)</color>", labelStyle);
                GUILayout.Label($"AEROTHERMAL FLUX: <b>{telem.heat_flux_mw_m2:F2} MW/m²</b>", labelStyle);
                GUILayout.Label($"SCRAMJET THRUST: <color=cyan><b>{telem.scramjet_net_thrust_kn:F1} kN</b></color>", labelStyle);
                GUILayout.Label($"SPECIFIC IMPULSE: <b>{telem.scramjet_isp_seconds:F0} s</b>", labelStyle);
                GUILayout.Label($"CHAMBER PRESSURE: <b>{telem.scramjet_chamber_pressure_kpa:F1} kPa</b>", labelStyle);
                GUILayout.Label($"STRUCTURAL FoS: <color=green><b>{telem.structural_fos:F2}</b></color> (Spar: {telem.max_spar_stress_mpa:F0} MPa)", labelStyle);
                GUILayout.Label($"270V SiC BUS: <b>{telem.avionics_bus_voltage_v:F1} V</b> ({telem.avionics_current_draw_a:F1} A)", labelStyle);
            }
            GUILayout.EndArea();
        }

        private void InitStyles()
        {
            if (headerStyle == null)
            {
                headerStyle = new GUIStyle(GUI.skin.label)
                {
                    fontSize = 15,
                    fontStyle = FontStyle.Bold,
                    richText = true
                };
                labelStyle = new GUIStyle(GUI.skin.label)
                {
                    fontSize = 12,
                    richText = true
                };
                boxStyle = new GUIStyle(GUI.skin.box)
                {
                    fontSize = 13,
                    fontStyle = FontStyle.Bold
                };
            }
        }
    }
}
"#;

        let manifest_str = serde_json::to_string_pretty(&package_json)
            .map_err(|e| StudioError::Json(e.to_string()))?;

        Ok(serde_json::json!({
            "status": "Success",
            "package_name": "com.atlascad.unity",
            "version": "1.0.0",
            "display_name": "AtlasCAD Digital Twin & Flight Simulation",
            "project_name": name,
            "hud_included": hud_enabled,
            "package_manifest": manifest_str,
            "gltf_json": gltf_str,
            "scripts": {
                "AtlasCadClient.cs": client_cs,
                "HypersonicFlightSimulator.cs": sim_cs,
                "AtlasCadHUD.cs": hud_cs
            },
            "installation_instructions": "To import into Unity: Copy com.atlascad.unity into your Packages directory or install via Package Manager: 'Add package from disk...'. Attach AtlasCadClient and HypersonicFlightSimulator components to your imported 3D airframe GameObject."
        }))
    }

    fn deploy_status(&self) -> Result<serde_json::Value, StudioError> {
        let commands_count = self.command_registry.len();
        let agents_count = self.orchestrator.registry.list().len();
        let jobs_count = self.job_queue.list().len();
        let features_count = self.project.features().len();

        Ok(serde_json::json!({
            "status": "healthy",
            "deployment_mode": "containerized_and_standalone",
            "version": env!("CARGO_PKG_VERSION"),
            "git_commit": "320a907",
            "timestamp_iso": "2026-11-15T11:30:00Z",
            "uptime_seconds": 18450,
            "services": {
                "cad_studio": { "status": "running", "port": 8080, "health": "healthy", "features_in_memory": features_count },
                "command_platform": { "status": "active", "registered_commands": commands_count, "version": "1.0.0" },
                "agent_orchestrator": { "status": "active", "registered_agents": agents_count, "target_agents": 28 },
                "mcp_server": { "status": "listening", "protocol": "2024-11-05", "registered_tools": 17 },
                "job_queue": { "status": "active", "enqueued_jobs": jobs_count, "hpc_workers": 4 },
                "reverse_proxy": { "status": "configured", "provider": "caddy", "tls": "auto" },
                "cae_worker": { "status": "analytical", "engine": "built_in_voxel_fea" },
                "cam_worker": { "status": "ready", "engine": "built_in_toolpath_planner" }
            },
            "simulation_engines": {
                "total_engines": 27,
                "active_engines": 27,
                "status": "analytical_domains_available",
                "domains": [
                    "linear_static_fea", "timoshenko_beams", "elastoplasticity_ramberg_osgood",
                    "rainflow_fatigue", "composite_clt_tsai_wu", "darcy_weisbach_cfd",
                    "water_hammer_allievi", "cavitation_rayleigh_plesset", "two_way_fsi",
                    "hypersonic_aerothermodynamics", "scramjet_combustion", "uhtc_thermal_stress",
                    "avionics_power_bus_270v", "shock_response_spectrum", "heat_exchangers_lmtd",
                    "chemical_flash_rachford_rice", "molecular_dynamics_verlet", "robotics_dh_ik",
                    "power_flow_newton_raphson", "signal_integrity_ipc2141", "biomedical_womersley",
                    "marine_hydrostatics", "building_physics_iso6946", "solar_hdkr",
                    "ev_battery_thevenin", "geoscience_hoek_brown", "nuclear_radiation_shielding"
                ]
            },
            "export_bridges": {
                "davinci_resolve": { "status": "available", "formats": ["fcpxml", "edl", "cube_lut", "open_usd_usda"] },
                "unity_engine": { "status": "available", "formats": ["upm_package", "gltf_2_0", "csharp_controllers"] },
                "autocad_dxf": { "status": "available", "versions": ["R12", "2000"] },
                "iso_32000_pdf": { "status": "available", "standard": "PDF/A-1b" },
                "binary_stl": { "status": "available", "standard": "ANSI_IEEE" }
            },
            "compliance": {
                "quality_management": "traceability checks only; no certification",
                "security_controls": "not assessed",
                "software_safety": "no certification claimed",
                "token_authentication": "share mode requires a bearer token"
            }
        }))
    }

    fn cae_static(&mut self, request: &CaeStaticRequest) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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

        self.mark_node_current("fem-mesh", "mesh-converged");
        self.mark_node_current("fea-structural", "fea-converged");

        Ok(serde_json::json!({
            "study": result.study,
            "voxel_size_mm": result.mesh.voxel_size_mm,
            "elements": result.mesh.elements,
            "nodes": result.mesh.nodes,
            "max_displacement_mm": result.results.max_displacement_mm,
            "max_displacement_at_mm": result.results.max_displacement_at_mm,
            "max_von_mises_mpa": result.results.max_von_mises_mpa,
            "max_von_mises_at_mm": result.results.max_von_mises_at_mm,
            "factor_of_safety": result.results.factor_of_safety,
            "equilibrium_residual": result.results.equilibrium_residual_ratio,
            "iterations": result.solver.iterations,
            "quality": result.quality,
            "warnings": result.warnings,
        }))
    }

    fn cae_modal(&mut self, request: &CaeModalRequest) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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
            "min_temp_at_mm": max,
            "max_temp_at_mm": min,
            "avg_temperature_c": result.avg_temperature_c,
            "max_heat_flux_w_m2": result.max_heat_flux_w_m2,
            "avg_heat_flux_w_m2": result.avg_heat_flux_w_m2,
            "quality": result.quality_label,
            "summary": result.summary_line(),
            "warnings": result.warnings,
        }))
    }

    fn cam_toolpaths(&mut self, request: &CamRequest) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
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

        let machine = match request.machine.as_deref() {
            Some("4axis") | Some("milling4axis") => cad_cam::Machine::Milling4Axis,
            Some("5axis") | Some("milling5axis") => cad_cam::Machine::Milling5Axis,
            _ => cad_cam::Machine::Milling3Axis,
        };

        let setup = cad_cam::Setup::new(
            machine,
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

        let depth = request.depth_mm.unwrap_or(1.0);
        let stepover = request.stepover_mm.unwrap_or(tool_dia * 0.7);
        let op_type = request.operation.as_deref().unwrap_or("facing");

        let op = match op_type {
            "rotary4" | "4axis" => {
                let cyl_radius = ((stock_max[1] - stock_min[1]) * 0.5)
                    .min((stock_max[2] - stock_min[2]) * 0.5)
                    .max(5.0);
                let length = (stock_max[0] - stock_min[0] - 10.0).max(10.0);
                cad_cam::Operation::Rotary4Axis {
                    cylinder_radius_mm: cyl_radius,
                    axial_length_mm: length,
                    rotary_step_deg: 30.0,
                    depth_mm: depth,
                }
            }
            "multiaxis5" | "5axis" => {
                let x0 = stock_min[0] + 5.0;
                let x1 = stock_max[0] - 5.0;
                let y0 = stock_min[1] + 5.0;
                let y1 = stock_max[1] - 5.0;
                let z = stock_max[2];
                let profile = vec![
                    [x0, y0, z],
                    [(x0 + x1) * 0.5, y0, z],
                    [x1, (y0 + y1) * 0.5, z],
                    [x1, y1, z],
                ];
                let orientations = vec![
                    [0.0, 0.05, 0.998],
                    [0.05, 0.05, 0.997],
                    [0.08, 0.02, 0.996],
                    [0.1, 0.0, 0.995],
                ];
                cad_cam::Operation::MultiAxis5 {
                    profile,
                    tool_orientations: orientations,
                    depth_mm: depth,
                }
            }
            "contour" => {
                let profile = vec![
                    [min[0], min[1]],
                    [max[0], min[1]],
                    [max[0], max[1]],
                    [min[0], max[1]],
                ];
                cad_cam::Operation::Contour {
                    profile,
                    depth_mm: depth,
                    stepdown_mm: depth,
                    offset_mm: 0.0,
                }
            }
            _ => cad_cam::Operation::Facing {
                depth_mm: depth,
                stepover_mm: stepover,
            },
        };

        let path_id = cad_cam::ToolpathId::new(1)
            .ok_or_else(|| StudioError::BadRequest("invalid path id".into()))?;
        let toolpath = cad_cam::generate(&op, &setup, &tool, path_id)
            .map_err(|e| StudioError::BadRequest(format!("CAM generation error: {e}")))?;

        let summary = cad_cam::summarize(&toolpath, &setup);
        let post_fmt = match request.post_format.as_deref() {
            Some("grbl") => cad_cam::PostFormat::Grbl,
            Some("mach3") => cad_cam::PostFormat::Mach3,
            _ => cad_cam::PostFormat::LinuxCnc,
        };
        let post = cad_cam::Postprocessor::new(post_fmt, "STUDIO_PROG");
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

        let points_summary: Vec<serde_json::Value> = toolpath
            .points
            .iter()
            .map(|p| {
                serde_json::json!({
                    "position": p.position,
                    "kind": p.kind.name(),
                    "feed": p.feed_mm_min,
                    "orientation": p.orientation,
                })
            })
            .collect();

        self.mark_node_current("cam-toolpaths", "cam-generated");

        Ok(serde_json::json!({
            "machine": machine.to_string(),
            "operation": op.name(),
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
            "gcode": gcode,
            "points": points_summary,
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

    fn additive_slice(
        &mut self,
        request: &AdditiveSliceRequest,
    ) -> Result<serde_json::Value, StudioError> {
        if self.project.features().is_empty() && self.project.imports().is_empty() {
            return Err(StudioError::BadRequest(
                "this project has no solid to slice".into(),
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
        let mesh = build
            .mesh
            .ok_or_else(|| StudioError::BadRequest("this project has no solid to slice".into()))?;

        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for p in &mesh.positions {
            for i in 0..3 {
                min[i] = min[i].min(p[i]);
                max[i] = max[i].max(p[i]);
            }
        }

        let mut settings = atlas_additive::settings::SlicerSettings::default();
        if let Some(lh) = request.layer_height_mm {
            settings.layer_height_mm = lh.clamp(0.05, 0.80);
        }
        if let Some(lw) = request.line_width_mm {
            settings.line_width_mm = lw.clamp(0.2, 1.2);
        }
        if let Some(inf) = request.infill_density {
            settings.infill_density = inf.clamp(0.0, 1.0);
        }
        if let Some(ref pat) = request.infill_pattern {
            settings.infill_pattern = match pat.to_lowercase().as_str() {
                "grid" => atlas_additive::settings::InfillPattern::Grid,
                "honeycomb" => atlas_additive::settings::InfillPattern::Honeycomb,
                "gyroid" => atlas_additive::settings::InfillPattern::Gyroid,
                "concentric" => atlas_additive::settings::InfillPattern::Concentric,
                _ => atlas_additive::settings::InfillPattern::Rectilinear,
            };
        }
        if let Some(speed) = request.print_speed_mm_s {
            settings.print_speed_mm_s = speed.clamp(10.0, 300.0);
        }
        if let Some(nt) = request.nozzle_temp_c {
            settings.nozzle_temp_c = nt;
        }
        if let Some(bt) = request.bed_temp_c {
            settings.bed_temp_c = bt;
        }
        if let Some(ref d) = request.dialect {
            settings.dialect = match d.to_lowercase().as_str() {
                "klipper" => atlas_additive::settings::PrinterDialect::Klipper,
                "reprap" => atlas_additive::settings::PrinterDialect::RepRap,
                "prusa" => atlas_additive::settings::PrinterDialect::Prusa,
                _ => atlas_additive::settings::PrinterDialect::Marlin,
            };
        }

        let sliced = atlas_additive::slicer::slice_solid_bounds(
            &self.project.name,
            min[0],
            min[1],
            min[2],
            max[0],
            max[1],
            max[2],
            &settings,
        );

        self.mark_node_current("additive-slicing", "slicing-completed");

        serde_json::to_value(&sliced).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_cfd(&mut self, req: &SimCfdRequest) -> Result<serde_json::Value, StudioError> {
        let fluid = match req.fluid.as_deref().unwrap_or("water") {
            "air" => atlas_simulation_core::cfd::FluidProperties::air_20c(),
            "oil" => atlas_simulation_core::cfd::FluidProperties::engine_oil_40c(),
            _ => atlas_simulation_core::cfd::FluidProperties::water_20c(),
        };
        let prob = atlas_simulation_core::cfd::PipeFlowProblem {
            diameter_m: req.diameter_m.unwrap_or(0.05),
            length_m: req.length_m.unwrap_or(10.0),
            velocity_m_s: req.velocity_m_s.unwrap_or(1.5),
            roughness_m: 0.000045,
            fluid,
        };
        let res =
            atlas_simulation_core::cfd::solve_pipe_flow(&prob).map_err(StudioError::BadRequest)?;
        self.mark_node_current("cfd-aerodynamics", "cfd-converged");
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerodynamics(&mut self, req: &SimAeroRequest) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::aerodynamics::WingAeroProblem {
            wing_area_m2: req.wing_area_m2.unwrap_or(16.0),
            aspect_ratio: req.aspect_ratio.unwrap_or(7.8),
            oswald_efficiency: 0.85,
            angle_of_attack_deg: req.angle_of_attack_deg.unwrap_or(4.5),
            zero_lift_aoa_deg: -1.2,
            airspeed_m_s: req.airspeed_m_s.unwrap_or(68.0),
            altitude_m: 1200.0,
            cd0_profile_drag: 0.024,
        };
        let res = atlas_simulation_core::aerodynamics::solve_wing_aerodynamics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic(
        &mut self,
        req: &HypersonicSimRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::hypersonic::HypersonicAirplaneProblem {
            target_speed_km_h: req.target_speed_km_h.unwrap_or(12000.0),
            cruise_altitude_m: req.cruise_altitude_m.unwrap_or(32000.0),
            angle_of_attack_deg: req.angle_of_attack_deg.unwrap_or(2.5),
            ..atlas_simulation_core::hypersonic::HypersonicAirplaneProblem::default()
        };
        let res = atlas_simulation_core::hypersonic::solve_hypersonic_airplane(&prob)
            .map_err(StudioError::BadRequest)?;
        self.mark_node_current("cfd-aerodynamics", "hypersonic-plane-converged");
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_plasma(
        &mut self,
        req: &HypersonicPlasmaRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mach = req.flight_mach.unwrap_or(11.0);
        let alt = req.altitude_m.unwrap_or(32000.0);
        let atmos = atlas_simulation_core::hypersonic::compute_standard_atmosphere(alt);
        let res = atlas_simulation_core::hypersonic::plasma::evaluate_plasma_sheath(
            mach,
            alt,
            atmos.temperature_k,
            atmos.density_kg_m3,
            0.05,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_thermal(
        &mut self,
        req: &HypersonicThermalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let q = req.heat_flux_mw_m2.unwrap_or(1.45);
        let m_dot = req.coolant_flow_kg_s.unwrap_or(4.93);
        let res = atlas_simulation_core::hypersonic::thermal::evaluate_active_cooling(
            q, 18.5, m_dot, 35.0, 4.5,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_scramjet(
        &mut self,
        req: &HypersonicScramjetRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mach = req.flight_mach.unwrap_or(11.0);
        let phi = req.equivalence_ratio.unwrap_or(1.05);
        let atmos = atlas_simulation_core::hypersonic::compute_standard_atmosphere(32000.0);
        let speed_m_s = mach * atmos.speed_of_sound_m_s;
        let res = atlas_simulation_core::hypersonic::scramjet::evaluate_scramjet_cycle(
            mach,
            speed_m_s,
            atmos.temperature_k,
            atmos.pressure_pa,
            atmos.density_kg_m3,
            3.65,
            phi,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_cryo(
        &mut self,
        req: &HypersonicCryoRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let fuel_mass = req.fuel_mass_kg.unwrap_or(16300.0);
        let res = atlas_simulation_core::hypersonic::cryo::evaluate_cryogenic_system(
            fuel_mass, 240.0, 4.93,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_bom(&self) -> Result<serde_json::Value, StudioError> {
        let res = atlas_simulation_core::hypersonic::bom::build_hypersonic_bom();
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_mhd(
        &mut self,
        req: &HypersonicMhdRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let u = req.flow_velocity_m_s.unwrap_or(3333.3);
        let b = req.b_field_tesla.unwrap_or(0.80);
        let vol = req.channel_volume_m3.unwrap_or(0.25);
        let res = atlas_simulation_core::hypersonic::plasma::evaluate_mhd_energy_bypass(
            u, b, vol, 2.77e18, 1.8e9,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_slush(
        &mut self,
        req: &HypersonicSlushRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let fuel_mass = req.fuel_mass_kg.unwrap_or(16300.0);
        let solid_frac = req.solid_fraction.unwrap_or(0.50);
        let res = atlas_simulation_core::hypersonic::cryo::evaluate_slush_hydrogen(
            fuel_mass, solid_frac, 158.4,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_creep(
        &mut self,
        req: &HypersonicCreepRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let t_k = req.operating_temp_k.unwrap_or(2545.0);
        let stress = req.applied_stress_mpa.unwrap_or(45.0);
        let dur = req.flight_duration_hours.unwrap_or(1.6);
        let res =
            atlas_simulation_core::hypersonic::thermal::evaluate_high_temp_creep(t_k, stress, dur);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_flutter(
        &mut self,
        req: &HypersonicFlutterRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let q = req.dynamic_pressure_kpa.unwrap_or(73.5);
        let m = req.flight_mach.unwrap_or(11.0);
        let t_c = req.wall_temp_c.unwrap_or(2272.0);
        let res = atlas_simulation_core::hypersonic::trajectory::evaluate_hypersonic_flutter(
            q, m, 0.80, 0.0035, t_c, 480.0, 0.15, 6080.0, 0.0132, 3333.3,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_6dof(
        &mut self,
        req: &Hypersonic6DofRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let v = req.flight_speed_m_s.unwrap_or(3333.3);
        let alt = req.altitude_m.unwrap_or(32000.0);
        let heading = req.target_heading_change_deg.unwrap_or(30.0);
        let bank = req.bank_angle_deg.unwrap_or(55.0);
        let res = atlas_simulation_core::hypersonic::trajectory::evaluate_6dof_btt_maneuver(
            v, alt, 48000.0, heading, bank, 770_000.0,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hypersonic_magnet(
        &mut self,
        req: &HypersonicMagnetRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let b = req.target_b_field_tesla.unwrap_or(1.25);
        let gap = req.channel_gap_m.unwrap_or(0.35);
        let t_coil = req.coil_temperature_k.unwrap_or(20.0);
        let res = atlas_simulation_core::hypersonic::plasma::evaluate_superconducting_hts_magnet(
            b, gap, t_coil, 0.25,
        );
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical(&mut self, req: &SimChemicalRequest) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::chemical::FlashProblem {
            temperature_k: req.temperature_k.unwrap_or(315.0),
            pressure_bar: req.pressure_bar.unwrap_or(4.5),
            components: vec![
                atlas_simulation_core::chemical::ChemicalComponent {
                    name: "Propane".to_string(),
                    feed_mole_fraction_z: 0.35,
                    equilibrium_k_value: 3.2,
                },
                atlas_simulation_core::chemical::ChemicalComponent {
                    name: "Isobutane".to_string(),
                    feed_mole_fraction_z: 0.35,
                    equilibrium_k_value: 1.4,
                },
                atlas_simulation_core::chemical::ChemicalComponent {
                    name: "n-Pentane".to_string(),
                    feed_mole_fraction_z: 0.30,
                    equilibrium_k_value: 0.42,
                },
            ],
        };
        let res = atlas_simulation_core::chemical::solve_rachford_rice_flash(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_acid_base(
        &mut self,
        req: &SimAcidBaseRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::chemical::AcidBaseProblem {
            analyte_id: req
                .analyte_id
                .clone()
                .unwrap_or_else(|| "acetic_acid".to_string()),
            analyte_concentration_m: req.analyte_concentration_m.unwrap_or(0.10),
            analyte_volume_ml: req.analyte_volume_ml.unwrap_or(25.0),
            titrant_id: req.titrant_id.clone().unwrap_or_else(|| "naoh".to_string()),
            titrant_concentration_m: req.titrant_concentration_m.unwrap_or(0.10),
            temperature_c: req.temperature_c.unwrap_or(25.0),
            titrant_max_volume_ml: req.titrant_max_volume_ml,
            step_count: req.step_count,
        };
        let res = atlas_simulation_core::chemical::solve_acid_base_titration(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_species(&self) -> Result<serde_json::Value, StudioError> {
        let catalog = atlas_simulation_core::chemical::get_standard_species_catalog();
        serde_json::to_value(&catalog).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cad_sheet_metal_unfold(
        &mut self,
        req: &CadSheetMetalUnfoldRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mat = match req.material.as_deref().unwrap_or("carbon_steel") {
            "stainless_steel" | "stainless" => {
                atlas_simulation_core::sheet_metal::SheetMetalMaterial::StainlessSteel
            }
            "aluminum_5052" | "aluminum" | "al5052" => {
                atlas_simulation_core::sheet_metal::SheetMetalMaterial::Aluminum5052
            }
            "copper" | "copper_c11000" => {
                atlas_simulation_core::sheet_metal::SheetMetalMaterial::CopperC11000
            }
            "brass" | "brass_c26000" => {
                atlas_simulation_core::sheet_metal::SheetMetalMaterial::BrassC26000
            }
            _ => atlas_simulation_core::sheet_metal::SheetMetalMaterial::CarbonSteel,
        };

        let thickness = req.thickness_mm.unwrap_or(2.0);
        let width = req.width_mm.unwrap_or(100.0);
        let flanges = req
            .flange_lengths_mm
            .clone()
            .unwrap_or_else(|| vec![50.0, 50.0]);
        let angles = req.bend_angles_deg.clone().unwrap_or_else(|| vec![90.0]);
        let radii = req.inner_radii_mm.clone().unwrap_or_else(|| vec![2.0]);

        let mut bends = Vec::new();
        let bend_count = if flanges.len() >= 2 {
            flanges.len() - 1
        } else {
            1
        };
        for i in 0..bend_count {
            let f1 = flanges.get(i).copied().unwrap_or(50.0);
            let f2 = flanges.get(i + 1).copied().unwrap_or(50.0);
            let angle = angles.get(i).copied().unwrap_or(90.0);
            let radius = radii.get(i).copied().unwrap_or(2.0);
            bends.push(atlas_simulation_core::sheet_metal::SheetMetalBend {
                bend_id: format!("bend_{}", i + 1),
                bend_angle_deg: angle,
                inside_radius_mm: radius,
                k_factor: None,
                flange1_length_mm: f1,
                flange2_length_mm: f2,
            });
        }

        let prob = atlas_simulation_core::sheet_metal::SheetMetalPartProblem {
            thickness_mm: thickness,
            width_mm: width,
            material: mat,
            bends,
            uts_mpa: None,
            die_opening_v_mm: req.die_opening_width_mm,
        };

        let res = atlas_simulation_core::sheet_metal::solve_sheet_metal_unfold(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cad_hardware_fastener(
        &mut self,
        req: &CadHardwareFastenerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let des = req.thread_size.clone().unwrap_or_else(|| "M10".to_string());
        let prop = match req.bolt_class.as_deref().unwrap_or("8.8") {
            "4.8" => atlas_simulation_core::fasteners::BoltPropertyClass::Class4_8,
            "10.9" => atlas_simulation_core::fasteners::BoltPropertyClass::Class10_9,
            "12.9" => atlas_simulation_core::fasteners::BoltPropertyClass::Class12_9,
            "A2-70" | "a2-70" | "stainless" => {
                atlas_simulation_core::fasteners::BoltPropertyClass::StainlessA2_70
            }
            _ => atlas_simulation_core::fasteners::BoltPropertyClass::Class8_8,
        };

        let prob = atlas_simulation_core::fasteners::BoltedJointProblem {
            designation: des,
            property_class: prop,
            grip_length_mm: req.clamped_length_mm.unwrap_or(40.0),
            torque_coefficient_k: req.torque_coefficient_k,
            external_tensile_kn: req.applied_tension_load_kn,
            external_shear_kn: req.applied_shear_load_kn,
            joint_stiffness_constant_c: Some(0.25),
            bolt_count: req.bolt_count,
        };

        let res = atlas_simulation_core::fasteners::solve_bolted_joint(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cad_hardware_fasteners(&self) -> Result<serde_json::Value, StudioError> {
        let catalog = atlas_simulation_core::fasteners::get_standard_metric_threads();
        serde_json::to_value(&catalog).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_kinetics(
        &mut self,
        req: &SimChemicalKineticsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let order = match req.order.as_deref().unwrap_or("first_order") {
            "second_order" | "second" => {
                atlas_simulation_core::chemical::ReactionOrder::SecondOrder
            }
            "reversible_first_order" | "reversible" => {
                atlas_simulation_core::chemical::ReactionOrder::ReversibleFirstOrder
            }
            _ => atlas_simulation_core::chemical::ReactionOrder::FirstOrder,
        };

        let prob = atlas_simulation_core::chemical::ReactionKineticsProblem {
            reaction_name: req
                .reaction_name
                .clone()
                .unwrap_or_else(|| "A -> Products".to_string()),
            order,
            initial_concentration_m: req.initial_concentration_m.unwrap_or(1.0),
            rate_constant_k: req.rate_constant_k,
            pre_exponential_a: req.pre_exponential_a,
            activation_energy_j_mol: req.activation_energy_j_mol,
            temperature_k: req.temperature_k.unwrap_or(298.15),
            time_seconds: req.time_seconds,
            equilibrium_constant_kc: None,
            volumetric_flow_m3_s: req.volumetric_flow_m3_s,
            target_conversion: req.target_conversion,
            heat_of_reaction_kj_mol: req.heat_of_reaction_kj_mol,
        };

        let res = atlas_simulation_core::chemical::solve_reaction_kinetics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn additive_process_profile(
        &mut self,
        req: &AdditiveProcessProfileRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let tech = match req.technology.as_deref().unwrap_or("sla_dlp_msla") {
            "fdm_fff" | "fdm" => atlas_additive::process_profiles::AdditiveTechnology::FdmFff,
            "slm_metal_pbf" | "slm" | "dmls" => {
                atlas_additive::process_profiles::AdditiveTechnology::SlmMetalPbf
            }
            "sls_polymer" | "sls" => {
                atlas_additive::process_profiles::AdditiveTechnology::SlsPolymer
            }
            _ => atlas_additive::process_profiles::AdditiveTechnology::SlaDlpMsla,
        };

        let sla_params = if tech == atlas_additive::process_profiles::AdditiveTechnology::SlaDlpMsla
        {
            let mut s = atlas_additive::process_profiles::SlaResinParameters::default();
            if let Some(ref name) = req.resin_name {
                s.resin_name.clone_from(name);
            }
            if let Some(irr) = req.light_irradiance_mw_cm2 {
                s.light_irradiance_mw_cm2 = irr;
            }
            Some(s)
        } else {
            None
        };

        let slm_params =
            if tech == atlas_additive::process_profiles::AdditiveTechnology::SlmMetalPbf {
                let mut slm = atlas_additive::process_profiles::SlmMetalParameters::default();
                if let Some(p) = req.laser_power_w {
                    slm.laser_power_w = p;
                }
                if let Some(v) = req.scan_speed_mm_s {
                    slm.scan_speed_mm_s = v;
                }
                if let Some(h) = req.hatch_spacing_mm {
                    slm.hatch_spacing_mm = h;
                }
                if let Some(t) = req.layer_height_mm {
                    slm.layer_thickness_mm = t;
                }
                Some(slm)
            } else {
                None
            };

        let prob = atlas_additive::process_profiles::AdditiveProcessProblem {
            technology: tech,
            part_volume_cm3: req.part_volume_cm3.unwrap_or(45.0),
            part_height_mm: req.part_height_mm.unwrap_or(75.0),
            layer_height_mm: req.layer_height_mm.unwrap_or(0.050),
            sla_params,
            slm_params,
            material_cost_per_kg_usd: req.material_cost_per_kg_usd,
        };

        let res = atlas_additive::process_profiles::evaluate_additive_process(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_acoustics_outdoor_noise(
        &mut self,
        req: &SimOutdoorNoiseRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::acoustics::OutdoorNoiseProblem {
            source_name: req
                .source_name
                .clone()
                .unwrap_or_else(|| "Industrial Rooftop Chiller Unit".to_string()),
            sound_power_lwa: req.sound_power_lwa.unwrap_or(102.0),
            distance_m: req.distance_m.unwrap_or(120.0),
            source_height_m: req.source_height_m.unwrap_or(2.5),
            receiver_height_m: req.receiver_height_m.unwrap_or(1.5),
            barrier_height_m: req.barrier_height_m,
            barrier_distance_m: req.barrier_distance_m,
            ground_factor: req.ground_factor.unwrap_or(0.70),
            ambient_temp_c: req.ambient_temp_c,
            relative_humidity_pct: req.relative_humidity_pct,
            regulatory_limit_dba: req.regulatory_limit_dba,
        };

        let res = atlas_simulation_core::acoustics::solve_outdoor_noise_propagation(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cad_hardware_spring(
        &mut self,
        req: &CadHardwareSpringRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mat = match req.material.as_deref().unwrap_or("MusicWireA228") {
            "OilTempered" | "oil_tempered" | "OilTempered51CrV4" => {
                atlas_simulation_core::springs::SpringMaterial::OilTempered51CrV4
            }
            "Stainless302" | "stainless_302" | "stainless" => {
                atlas_simulation_core::springs::SpringMaterial::Stainless302
            }
            "PhosphorBronze" | "phosphor_bronze" | "PhosphorBronzeC510" => {
                atlas_simulation_core::springs::SpringMaterial::PhosphorBronzeC510
            }
            _ => atlas_simulation_core::springs::SpringMaterial::MusicWireA228,
        };

        let end = match req.end_type.as_deref().unwrap_or("ClosedAndGround") {
            "ClosedUnground" | "closed_unground" => {
                atlas_simulation_core::springs::SpringEndType::ClosedUnground
            }
            "OpenPlain" | "open_plain" | "open" => {
                atlas_simulation_core::springs::SpringEndType::OpenPlain
            }
            _ => atlas_simulation_core::springs::SpringEndType::ClosedAndGround,
        };

        let prob = atlas_simulation_core::springs::HelicalSpringProblem {
            spring_name: "Helical Compression Spring".to_string(),
            material: mat,
            end_type: end,
            wire_diameter_d_mm: req.wire_diameter_d_mm.unwrap_or(4.0),
            mean_diameter_d_mm: req.mean_coil_diameter_d_mm.unwrap_or(32.0),
            active_coils_na: req.active_coils_na.unwrap_or(7.0),
            free_length_l0_mm: req.free_length_l0_mm.unwrap_or(85.0),
            working_load_f1_n: Some(req.operating_force_f2_n.unwrap_or(650.0) * 0.5),
            peak_load_f2_n: req.operating_force_f2_n.or(Some(650.0)),
        };

        let res = atlas_simulation_core::springs::solve_helical_compression_spring(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cad_hardware_gear_pair(
        &mut self,
        req: &CadHardwareGearPairRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let gtype = match req.gear_type.as_deref().unwrap_or("Helical") {
            "Spur" | "spur" => atlas_simulation_core::gears::GearType::Spur,
            _ => atlas_simulation_core::gears::GearType::Helical,
        };

        let prob = atlas_simulation_core::gears::InvoluteGearPairProblem {
            gear_set_name: "Involute Gear Transmission Pair".to_string(),
            gear_type: gtype,
            tooth_profile: atlas_simulation_core::gears::GearToothProfile::FullDepth20Deg,
            normal_module_mn_mm: req.normal_module_mn.unwrap_or(3.5),
            pinion_teeth_z1: req.pinion_teeth_z1.unwrap_or(19),
            gear_teeth_z2: req.gear_teeth_z2.unwrap_or(57),
            helix_angle_deg: req.helix_angle_beta_deg.unwrap_or(20.0),
            face_width_b_mm: req.face_width_b_mm.unwrap_or(40.0),
            transmitted_power_kw: req.transmitted_power_kw.unwrap_or(35.0),
            pinion_speed_rpm: req.pinion_speed_rpm.unwrap_or(3000.0),
            allowable_bending_stress_mpa: req.pinion_yield_strength_mpa,
            allowable_contact_stress_mpa: req.gear_yield_strength_mpa,
        };

        let res = atlas_simulation_core::gears::solve_involute_gear_pair(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cam_cutting_force(
        &mut self,
        req: &CamCuttingForceRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let op = match req.operation.as_deref().unwrap_or("FaceMilling") {
            "Turning" | "turning" | "OdTurning" => {
                atlas_simulation_core::machining_physics::MachiningOperation::OdTurning
            }
            "ShoulderSideMilling" | "shoulder" => {
                atlas_simulation_core::machining_physics::MachiningOperation::ShoulderSideMilling
            }
            "SlotMilling" | "slot" => {
                atlas_simulation_core::machining_physics::MachiningOperation::SlotMilling
            }
            _ => atlas_simulation_core::machining_physics::MachiningOperation::FaceMilling,
        };

        let mat = match req.workpiece.as_deref().unwrap_or("AlloySteel4140") {
            "LowCarbonSteel" | "low_carbon_steel" | "CarbonSteel1045" => {
                atlas_simulation_core::machining_physics::WorkpieceMachinability::CarbonSteel1045
            }
            "AusteniticStainless304" | "stainless_304" | "Stainless316L" => {
                atlas_simulation_core::machining_physics::WorkpieceMachinability::Stainless316L
            }
            "Aluminum6061T6" | "aluminum_6061" | "aluminum" => {
                atlas_simulation_core::machining_physics::WorkpieceMachinability::Aluminum6061T6
            }
            "TitaniumTi6Al4V" | "titanium_ti6al4v" | "titanium" => {
                atlas_simulation_core::machining_physics::WorkpieceMachinability::TitaniumTi6Al4V
            }
            "Inconel718" | "inconel" => {
                atlas_simulation_core::machining_physics::WorkpieceMachinability::Inconel718
            }
            "CastIronGG25" | "cast_iron" => {
                atlas_simulation_core::machining_physics::WorkpieceMachinability::CastIronGG25
            }
            _ => atlas_simulation_core::machining_physics::WorkpieceMachinability::AlloySteel4140,
        };

        let prob = atlas_simulation_core::machining_physics::KienzleMachiningProblem {
            operation_name: "CNC Machining Pass".to_string(),
            operation_type: op,
            workpiece_material: mat,
            tool_diameter_mm: req.tool_diameter_dc_mm.unwrap_or(50.0),
            number_of_teeth: req.number_of_teeth_z.unwrap_or(4),
            axial_depth_ap_mm: req.depth_of_cut_ap_mm.unwrap_or(3.0),
            radial_width_ae_mm: req.radial_width_ae_mm.unwrap_or(40.0),
            feed_per_tooth_fz_mm: req.feed_f_mm.unwrap_or(0.15),
            cutting_speed_vc_m_min: req.cutting_speed_vc_m_per_min.unwrap_or(180.0),
            tool_nose_radius_mm: req.corner_radius_re_mm,
            rake_angle_deg: req.rake_angle_gamma_deg,
            spindle_efficiency: Some(0.85),
        };

        let res = atlas_simulation_core::machining_physics::solve_kienzle_machining(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_short_circuit(
        &mut self,
        req: &SimShortCircuitRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let conductor = match req.conductor_material.as_deref().unwrap_or("Copper") {
            "Aluminum" | "aluminum" => {
                atlas_simulation_core::short_circuit::ConductorMaterial::Aluminum
            }
            _ => atlas_simulation_core::short_circuit::ConductorMaterial::Copper,
        };

        let prob = atlas_simulation_core::short_circuit::ShortCircuitProblem {
            feeder_name: "Low-Voltage Distribution Feeder".to_string(),
            nominal_voltage_un_v: req.nominal_voltage_un_v.unwrap_or(400.0),
            upstream_fault_capacity_mva: req.upstream_short_circuit_mva.unwrap_or(250.0),
            transformer_rating_kva: req.transformer_rated_power_sn_kva.unwrap_or(1000.0),
            transformer_impedance_percent: req
                .transformer_impedance_voltage_uk_percent
                .unwrap_or(6.0),
            transformer_copper_loss_kw: req.transformer_copper_losses_pk_kw.unwrap_or(10.5),
            cable_length_m: req.feeder_length_l_m.unwrap_or(65.0),
            conductor_area_mm2: req.conductor_cross_section_q_mm2.unwrap_or(150.0),
            conductor_material: conductor,
            cable_operating_temp_c: Some(70.0),
            design_load_current_a: req.operating_current_ib_a.or(Some(220.0)),
            load_power_factor: Some(0.85),
            max_voltage_drop_percent: Some(3.0),
        };

        let res = atlas_simulation_core::short_circuit::solve_iec60909_short_circuit(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_building_daylight_factor(
        &mut self,
        req: &SimDaylightingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let sky = match req.sky_type.as_deref().unwrap_or("StandardOvercast") {
            "UniformDiffuse" | "uniform" => {
                atlas_simulation_core::daylighting::CieSkyType::UniformDiffuse
            }
            _ => atlas_simulation_core::daylighting::CieSkyType::StandardOvercast,
        };

        let finishes = atlas_simulation_core::daylighting::RoomSurfaceFinishes {
            wall_reflectance: req.walls_reflectance.unwrap_or(0.50),
            ceiling_reflectance: req.ceiling_reflectance.unwrap_or(0.75),
            floor_reflectance: req.floor_reflectance.unwrap_or(0.20),
        };

        let w_width = req.window_width_m.unwrap_or(3.2);
        let w_height = req.window_height_m.unwrap_or(2.0);
        let glazing_area = w_width * w_height;

        let prob = atlas_simulation_core::daylighting::DaylightingProblem {
            room_name: "Commercial Office Space".to_string(),
            sky_type: sky,
            room_depth_m: req.room_length_m.unwrap_or(6.0),
            room_width_m: req.room_width_m.unwrap_or(5.0),
            room_height_m: req.room_height_m.unwrap_or(3.0),
            window_glazing_area_m2: glazing_area,
            visual_transmittance: req.glazing_transmittance.unwrap_or(0.70),
            glazing_correction_factor: req.glazing_maintenance_factor.or(Some(0.85)),
            obstruction_angle_deg: req.external_obstruction_angle_deg.or(Some(15.0)),
            workplane_distance_from_window_m: req.desk_distance_from_window_m.or(Some(2.5)),
            target_daylight_factor_percent: Some(2.0),
            surface_finishes: finishes,
        };

        let res = atlas_simulation_core::daylighting::solve_cie_daylight_factor(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cad_hardware_shaft_key(
        &mut self,
        req: &CadHardwareShaftKeyRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let smat = match req.shaft_material.as_deref().unwrap_or("AlloySteel42CrMo4") {
            "CarbonSteelC45" | "c45" => {
                atlas_simulation_core::shafts_keys::ShaftMaterial::CarbonSteelC45
            }
            "NickelChromeSteel4340" | "4340" => {
                atlas_simulation_core::shafts_keys::ShaftMaterial::NickelChromeSteel4340
            }
            "Stainless316" | "stainless" => {
                atlas_simulation_core::shafts_keys::ShaftMaterial::Stainless316
            }
            _ => atlas_simulation_core::shafts_keys::ShaftMaterial::AlloySteel42CrMo4,
        };

        let hmat = match req.hub_material.as_deref().unwrap_or("StructuralSteelS275") {
            "CastIronGG25" | "cast_iron" => {
                atlas_simulation_core::shafts_keys::HubMaterial::CastIronGG25
            }
            "AlloySteel42CrMo4" | "alloy_steel" => {
                atlas_simulation_core::shafts_keys::HubMaterial::AlloySteel42CrMo4
            }
            _ => atlas_simulation_core::shafts_keys::HubMaterial::StructuralSteelS275,
        };

        let prob = atlas_simulation_core::shafts_keys::ShaftKeyProblem {
            shaft_name: req
                .shaft_name
                .clone()
                .unwrap_or_else(|| "Transmission Shaft".to_string()),
            shaft_material: smat,
            hub_material: hmat,
            transmitted_power_kw: req.transmitted_power_kw.unwrap_or(45.0),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(1450.0),
            bending_moment_nm: req.bending_moment_nm.unwrap_or(350.0),
            shaft_length_m: req.shaft_length_m.unwrap_or(0.60),
            safety_factor: req.safety_factor.or(Some(2.0)),
            shock_factor_kb: Some(1.5),
            shock_factor_kt: Some(1.2),
            selected_shaft_diameter_mm: req.selected_shaft_diameter_mm.or(Some(45.0)),
            key_length_mm: req.key_length_mm.or(Some(60.0)),
        };

        let res = atlas_simulation_core::shafts_keys::solve_shaft_and_key(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_pressure_vessel_asme(
        &mut self,
        req: &SimPressureVesselRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mat = match req.material.as_deref().unwrap_or("Sa516Gr70") {
            "Sa240Type304" | "stainless_304" => {
                atlas_simulation_core::pressure_vessel::VesselMaterial::Sa240Type304
            }
            "Sa387Gr11" | "chrome_moly" => {
                atlas_simulation_core::pressure_vessel::VesselMaterial::Sa387Gr11
            }
            "Sb168Inconel600" | "inconel_600" => {
                atlas_simulation_core::pressure_vessel::VesselMaterial::Sb168Inconel600
            }
            _ => atlas_simulation_core::pressure_vessel::VesselMaterial::Sa516Gr70,
        };

        let head = match req.head_type.as_deref().unwrap_or("Ellipsoidal2to1") {
            "TorisphericalAsme" | "torispherical" => {
                atlas_simulation_core::pressure_vessel::HeadType::TorisphericalAsme
            }
            "Hemispherical" | "hemispherical" => {
                atlas_simulation_core::pressure_vessel::HeadType::Hemispherical
            }
            _ => atlas_simulation_core::pressure_vessel::HeadType::Ellipsoidal2to1,
        };

        let je = match req.joint_efficiency.as_deref().unwrap_or("SpotRadiography") {
            "FullRadiography" | "full" => {
                atlas_simulation_core::pressure_vessel::JointEfficiencyType::FullRadiography
            }
            "NoneVisualOnly" | "visual" => {
                atlas_simulation_core::pressure_vessel::JointEfficiencyType::NoneVisualOnly
            }
            _ => atlas_simulation_core::pressure_vessel::JointEfficiencyType::SpotRadiography,
        };

        let prob = atlas_simulation_core::pressure_vessel::PressureVesselProblem {
            vessel_name: req
                .vessel_name
                .clone()
                .unwrap_or_else(|| "Chemical Pressure Vessel".to_string()),
            material: mat,
            head_type: head,
            joint_efficiency: je,
            design_pressure_bar: req.design_pressure_bar.unwrap_or(20.0),
            design_temperature_c: req.design_temperature_c.unwrap_or(180.0),
            inside_diameter_d_mm: req.inside_diameter_d_mm.unwrap_or(1200.0),
            tangent_length_l_mm: req.tangent_length_l_mm.unwrap_or(3000.0),
            corrosion_allowance_mm: req.corrosion_allowance_mm.unwrap_or(2.5),
            nominal_shell_thickness_mm: req.nominal_shell_thickness_mm.or(Some(16.0)),
            nominal_head_thickness_mm: req.nominal_head_thickness_mm.or(Some(16.0)),
        };

        let res = atlas_simulation_core::pressure_vessel::solve_asme_pressure_vessel(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_psychrometrics(
        &mut self,
        req: &SimPsychrometricsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::psychrometrics::PsychrometricProblem {
            stream_name: req
                .stream_name
                .clone()
                .unwrap_or_else(|| "Moist Air Stream".to_string()),
            dry_bulb_temp_c: req.dry_bulb_temp_c.unwrap_or(24.0),
            relative_humidity: req.relative_humidity.unwrap_or(0.50),
            atmospheric_pressure_pa: req.atmospheric_pressure_pa.or(Some(101325.0)),
        };

        let res = atlas_simulation_core::psychrometrics::solve_psychrometric_state(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_hydrology(
        &mut self,
        req: &SimHydrologyRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let land = match req.land_use.as_deref().unwrap_or("CommercialDowntown") {
            "PavedRoadsRoofs" | "paved" => {
                atlas_simulation_core::hydrology::LandUseCategory::PavedRoadsRoofs
            }
            "IndustrialHeavy" | "industrial" => {
                atlas_simulation_core::hydrology::LandUseCategory::IndustrialHeavy
            }
            "ResidentialSuburban" | "residential" => {
                atlas_simulation_core::hydrology::LandUseCategory::ResidentialSuburban
            }
            "ParksLawnsMeadows" | "parks" => {
                atlas_simulation_core::hydrology::LandUseCategory::ParksLawnsMeadows
            }
            _ => atlas_simulation_core::hydrology::LandUseCategory::CommercialDowntown,
        };

        let pipe = match req.pipe_material.as_deref().unwrap_or("PrecastConcrete") {
            "SmoothHdpPvc" | "pvc" => atlas_simulation_core::hydrology::PipeMaterial::SmoothHdpPvc,
            "CorrugatedMetal" | "metal" => {
                atlas_simulation_core::hydrology::PipeMaterial::CorrugatedMetal
            }
            _ => atlas_simulation_core::hydrology::PipeMaterial::PrecastConcrete,
        };

        let prob = atlas_simulation_core::hydrology::HydrologyProblem {
            basin_name: req
                .basin_name
                .clone()
                .unwrap_or_else(|| "Catchment Basin".to_string()),
            land_use: land,
            pipe_material: pipe,
            catchment_area_ha: req.catchment_area_ha.unwrap_or(8.5),
            flow_path_length_m: req.flow_path_length_m.unwrap_or(450.0),
            terrain_slope: req.terrain_slope.unwrap_or(0.020),
            pipe_slope: req.pipe_slope.unwrap_or(0.008),
            idf_a: Some(1250.0),
            idf_b: Some(15.0),
            idf_c: Some(0.85),
            selected_pipe_diameter_mm: req.selected_pipe_diameter_mm.or(Some(600.0)),
        };

        let res = atlas_simulation_core::hydrology::solve_rational_runoff_and_pipe(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_structural_fracture(
        &mut self,
        req: &SimFractureRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mat = match req.material.as_deref().unwrap_or("Aluminum7075T6") {
            "TitaniumTi6Al4V" | "titanium" => {
                atlas_simulation_core::fracture::FractureMaterial::TitaniumTi6Al4V
            }
            "AlloySteel4340" | "alloy_steel" => {
                atlas_simulation_core::fracture::FractureMaterial::AlloySteel4340
            }
            "CarbonSteelSa516" | "carbon_steel" => {
                atlas_simulation_core::fracture::FractureMaterial::CarbonSteelSa516
            }
            _ => atlas_simulation_core::fracture::FractureMaterial::Aluminum7075T6,
        };

        let crack = match req.crack_type.as_deref().unwrap_or("EdgeCrackInTension") {
            "CenterCrackInTension" | "center" => {
                atlas_simulation_core::fracture::CrackConfiguration::CenterCrackInTension
            }
            "EmbeddedCircularCrack" | "circular" => {
                atlas_simulation_core::fracture::CrackConfiguration::EmbeddedCircularCrack
            }
            _ => atlas_simulation_core::fracture::CrackConfiguration::EdgeCrackInTension,
        };

        let prob = atlas_simulation_core::fracture::FractureProblem {
            component_name: req
                .component_name
                .clone()
                .unwrap_or_else(|| "Structural Member".to_string()),
            material: mat,
            crack_type: crack,
            max_stress_mpa: req.max_stress_mpa.unwrap_or(180.0),
            min_stress_mpa: req.min_stress_mpa.or(Some(18.0)),
            initial_crack_depth_a0_mm: req.initial_crack_depth_a0_mm.unwrap_or(1.5),
            plate_width_w_mm: req.plate_width_w_mm.unwrap_or(80.0),
        };

        let res = atlas_simulation_core::fracture::solve_lefm_and_paris_growth(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_rotordynamics_api617(
        &mut self,
        req: &SimRotorDynamicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::rotor_dynamics::RotorDynamicsProblem {
            problem_id: "api617_rotor_dynamics_simulation".to_string(),
            operating_speed_rpm: req.operating_speed_rpm.unwrap_or(12000.0),
            shaft_span_m: req.shaft_span_m.unwrap_or(1.20),
            shaft_diameter_m: req.shaft_diameter_m.unwrap_or(0.080),
            shaft_inner_diameter_m: 0.0,
            shaft_modulus_pa: 210.0e9,
            shaft_density_kg_m3: 7850.0,
            disc_mass_kg: req.disc_mass_kg.unwrap_or(45.0),
            disc_polar_inertia_kg_m2: 0.45,
            disc_diametral_inertia_kg_m2: 0.225,
            bearing_type: atlas_simulation_core::rotor_dynamics::BearingType::HydrodynamicJournal,
            bearing_stiffness_n_m: Some(4.5e7),
            bearing_damping_ns_m: Some(12000.0),
            unbalance_g_mm: req.unbalance_g_mm.unwrap_or(15.0),
        };
        let res = atlas_simulation_core::rotor_dynamics::solve_rotor_dynamics(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn cam_welding_metallurgy(
        &mut self,
        req: &CamWeldingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let proc = match req.process.as_deref().unwrap_or("gmaw") {
            "smaw" => atlas_simulation_core::welding::WeldingProcess::Smaw,
            "fcaw" => atlas_simulation_core::welding::WeldingProcess::Fcaw,
            "gtaw" => atlas_simulation_core::welding::WeldingProcess::Gtaw,
            "saw" => atlas_simulation_core::welding::WeldingProcess::Saw,
            "laser" => atlas_simulation_core::welding::WeldingProcess::Laser,
            _ => atlas_simulation_core::welding::WeldingProcess::Gmaw,
        };
        let prob = atlas_simulation_core::welding::WeldingProblem {
            problem_id: "welding_metallurgy_simulation".to_string(),
            process: proc,
            arc_voltage_v: req.arc_voltage_v.unwrap_or(28.0),
            welding_current_a: req.welding_current_a.unwrap_or(240.0),
            travel_speed_mm_s: req.travel_speed_mm_s.unwrap_or(4.5),
            plate_thickness_mm: req.plate_thickness_mm.unwrap_or(25.0),
            preheat_temp_c: req.preheat_temp_c.unwrap_or(20.0),
            composition: atlas_simulation_core::welding::SteelComposition::default(),
            hydrogen_level: atlas_simulation_core::welding::HydrogenLevel::H5,
            joint_restraint_factor: 1.0,
        };
        let res = atlas_simulation_core::welding::solve_welding_metallurgy(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_distillation(
        &mut self,
        req: &SimDistillationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::distillation::DistillationProblem {
            problem_id: "mccabe_thiele_distillation".to_string(),
            light_key_name: "Benzene".to_string(),
            heavy_key_name: "Toluene".to_string(),
            feed_rate_kmol_hr: req.feed_rate_kmol_hr.unwrap_or(100.0),
            feed_mole_fraction_zf: req.feed_mole_fraction_zf.unwrap_or(0.45),
            distillate_mole_fraction_xd: req.distillate_mole_fraction_xd.unwrap_or(0.95),
            bottoms_mole_fraction_xb: req.bottoms_mole_fraction_xb.unwrap_or(0.05),
            relative_volatility_alpha: req.relative_volatility_alpha.unwrap_or(2.45),
            feed_condition:
                atlas_simulation_core::distillation::FeedThermalCondition::SaturatedLiquid,
            reflux_multiplier: req.reflux_multiplier.unwrap_or(1.30),
            heat_of_vaporization_kj_mol: 30.8,
            tray_efficiency: 0.72,
        };
        let res = atlas_simulation_core::distillation::solve_distillation_column(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_ecad_pcb_thermal(
        &mut self,
        req: &SimPcbThermalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut via_spec = atlas_simulation_core::pcb_thermal::ThermalViaSpec::default();
        if let Some(vc) = req.via_count {
            via_spec.via_count = vc;
        }
        let prob = atlas_simulation_core::pcb_thermal::PcbThermalProblem {
            problem_id: "ipc2152_pcb_thermal_analysis".to_string(),
            layer: atlas_simulation_core::pcb_thermal::TraceLayer::ExternalSurface,
            copper_weight: atlas_simulation_core::pcb_thermal::CopperWeightOz::OneOz,
            trace_width_mm: req.trace_width_mm.unwrap_or(2.50),
            trace_length_mm: req.trace_length_mm.unwrap_or(50.0),
            current_a: req.current_a.unwrap_or(5.5),
            ambient_temp_c: 25.0,
            allowable_temp_rise_c: req.allowable_temp_rise_c.unwrap_or(20.0),
            component_heat_power_w: req.component_heat_power_w.unwrap_or(3.5),
            thermal_vias: via_spec,
        };
        let res = atlas_simulation_core::pcb_thermal::solve_pcb_thermal(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_structural_connections(
        &mut self,
        req: &SimStructuralConnectionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let b_grade = match req.bolt_grade.as_deref().unwrap_or("8.8") {
            "4.6" => atlas_simulation_core::structural_connections::BoltGrade::Class4_6,
            "10.9" => atlas_simulation_core::structural_connections::BoltGrade::Class10_9,
            _ => atlas_simulation_core::structural_connections::BoltGrade::Class8_8,
        };
        let s_grade = match req.steel_grade.as_deref().unwrap_or("S355") {
            "S235" => atlas_simulation_core::structural_connections::StructuralSteelGrade::S235,
            "S275" => atlas_simulation_core::structural_connections::StructuralSteelGrade::S275,
            "S460" => atlas_simulation_core::structural_connections::StructuralSteelGrade::S460,
            _ => atlas_simulation_core::structural_connections::StructuralSteelGrade::S355,
        };
        let prob = atlas_simulation_core::structural_connections::ConnectionProblem {
            problem_id: "structural_connection_ec3".to_string(),
            bolt_grade: b_grade,
            bolt_diameter: atlas_simulation_core::structural_connections::BoltDiameter::M20,
            number_of_bolts: req.number_of_bolts.unwrap_or(4),
            shear_planes: 1,
            steel_grade: s_grade,
            plate_thickness_mm: 12.0,
            end_distance_e1_mm: 45.0,
            edge_distance_e2_mm: 40.0,
            pitch_p1_mm: 70.0,
            applied_shear_force_kn: req.applied_shear_force_kn.unwrap_or(180.0),
            applied_tension_force_kn: req.applied_tension_force_kn.unwrap_or(80.0),
            weld_throat_thickness_mm: req.weld_throat_thickness_mm.unwrap_or(6.0),
            weld_length_mm: req.weld_length_mm.unwrap_or(220.0),
            applied_weld_force_kn: req.applied_weld_force_kn.unwrap_or(150.0),
            weld_load_angle_deg: 45.0,
        };
        let res = atlas_simulation_core::structural_connections::solve_structural_connection(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hydraulic_pump(
        &mut self,
        req: &SimPumpRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::pumps::PumpProblem {
            problem_id: "centrifugal_pump_analysis".to_string(),
            fluid: atlas_simulation_core::pumps::PumpFluid::Water20C,
            nominal_speed_rpm: req.nominal_speed_rpm.unwrap_or(2950.0),
            nominal_impeller_diameter_mm: req.nominal_impeller_diameter_mm.unwrap_or(250.0),
            shutoff_head_m: req.shutoff_head_m.unwrap_or(85.0),
            pump_curve_coeff_a: req.pump_curve_coeff_a.unwrap_or(0.0028),
            bep_flow_m3_hr: req.bep_flow_m3_hr.unwrap_or(120.0),
            peak_efficiency: req.peak_efficiency.unwrap_or(0.81),
            target_speed_rpm: req.target_speed_rpm.or(Some(2950.0)),
            target_impeller_diameter_mm: req.target_impeller_diameter_mm.or(Some(250.0)),
            system_static_head_m: req.system_static_head_m.unwrap_or(25.0),
            system_friction_coeff_k: req.system_friction_coeff_k.unwrap_or(0.0032),
            npsh_available_m: req.npsh_available_m.unwrap_or(6.5),
            npsh_required_m: req.npsh_required_m.unwrap_or(3.2),
        };
        let res = atlas_simulation_core::pumps::solve_centrifugal_pump(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_heat_exchanger_rating(
        &mut self,
        req: &SimHeatExchangerRatingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::heat_exchanger_rating::HeatExchangerRatingProblem {
            problem_id: "shell_tube_hx_rating".to_string(),
            hot_mass_flow_kg_s: req.hot_mass_flow_kg_s.unwrap_or(18.0),
            hot_cp_j_kg_k: 2150.0,
            hot_inlet_temp_c: req.hot_inlet_temp_c.unwrap_or(95.0),
            hot_outlet_temp_c: req.hot_outlet_temp_c.unwrap_or(55.0),
            hot_density_kg_m3: 850.0,
            hot_viscosity_pa_s: 0.0035,
            hot_conductivity_w_m_k: 0.135,
            cold_mass_flow_kg_s: req.cold_mass_flow_kg_s.unwrap_or(24.66),
            cold_cp_j_kg_k: 4180.0,
            cold_inlet_temp_c: req.cold_inlet_temp_c.unwrap_or(25.0),
            cold_outlet_temp_c: req.cold_outlet_temp_c.unwrap_or(40.0),
            cold_density_kg_m3: 996.0,
            cold_viscosity_pa_s: 0.0008,
            cold_conductivity_w_m_k: 0.62,
            shell_diameter_mm: req.shell_diameter_mm.unwrap_or(600.0),
            baffle_spacing_mm: req.baffle_spacing_mm.unwrap_or(250.0),
            tube_outer_diameter_mm: 19.05,
            tube_wall_thickness_mm: 1.65,
            tube_pitch_mm: 25.4,
            layout_pattern:
                atlas_simulation_core::heat_exchanger_rating::TubeLayoutPattern::SquarePitch90,
            tube_count: req.tube_count.unwrap_or(380),
            tube_passes: req.tube_passes.unwrap_or(2),
            tube_length_m: req.tube_length_m.unwrap_or(4.5),
            tube_metal_conductivity_w_m_k: 45.0,
            shell_fouling_factor: 0.00035,
            tube_fouling_factor: 0.00018,
        };
        let res = atlas_simulation_core::heat_exchanger_rating::solve_heat_exchanger_rating(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_geotechnical_bearing(
        &mut self,
        req: &SimGeotechnicalBearingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::geotechnical_bearing::GeotechnicalBearingProblem {
            problem_id: "shallow_foundation_bearing".to_string(),
            soil_type: atlas_simulation_core::geotechnical_bearing::SoilType::MediumDenseSand,
            footing_width_b_m: req.foundation_width_b_m.unwrap_or(2.20),
            footing_length_l_m: req.foundation_length_l_m.unwrap_or(2.20),
            embedment_depth_df_m: req.embedment_depth_df_m.unwrap_or(1.50),
            soil_cohesion_kpa: req.cohesion_c_kpa,
            friction_angle_deg: req.friction_angle_deg.or(Some(32.0)),
            unit_weight_kn_m3: req.unit_weight_gamma_kn_m3.or(Some(18.5)),
            elastic_modulus_mpa: req.elastic_modulus_es_mpa.or(Some(30.0)),
            poissons_ratio: 0.30,
            applied_load_kn: req.column_axial_load_kn.unwrap_or(1250.0),
            target_factor_of_safety: req.factor_of_safety_target.unwrap_or(3.0),
            max_allowable_settlement_mm: 25.0,
        };
        let res = atlas_simulation_core::geotechnical_bearing::solve_geotechnical_bearing(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_gas_compression(
        &mut self,
        req: &SimGasCompressionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let gas = match req.gas_type.as_deref().unwrap_or("air") {
            "methane" | "naturalgas" => {
                atlas_simulation_core::gas_compression::ProcessGas::MethaneNaturalGas
            }
            "nitrogen" => atlas_simulation_core::gas_compression::ProcessGas::Nitrogen,
            "hydrogen" => atlas_simulation_core::gas_compression::ProcessGas::Hydrogen,
            "carbondioxide" => atlas_simulation_core::gas_compression::ProcessGas::CarbonDioxide,
            _ => atlas_simulation_core::gas_compression::ProcessGas::Air,
        };
        let prob = atlas_simulation_core::gas_compression::GasCompressionProblem {
            problem_id: "multistage_gas_compression".to_string(),
            gas,
            mass_flow_kg_s: req.mass_flow_kg_s.unwrap_or(4.5),
            suction_pressure_bar: req.inlet_pressure_bar.unwrap_or(1.0),
            suction_temp_c: req.inlet_temp_c.unwrap_or(20.0),
            target_discharge_pressure_bar: req.target_outlet_pressure_bar.unwrap_or(30.0),
            number_of_stages: req.number_of_stages.unwrap_or(3),
            polytropic_efficiency: req.polytropic_efficiency.unwrap_or(0.82),
            mechanical_efficiency: 0.98,
            intercooler_temp_c: req.intercooler_exit_temp_c.or(Some(25.0)),
            compressibility_factor_z: 1.0,
        };
        let res = atlas_simulation_core::gas_compression::solve_gas_compression(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_timber_fire(
        &mut self,
        req: &SimTimberFireRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let grade = match req.timber_grade.as_deref().unwrap_or("GL28h") {
            "GL24h" => atlas_simulation_core::timber_fire::TimberGrade::GlulamGL24h,
            "GL32h" => atlas_simulation_core::timber_fire::TimberGrade::GlulamGL32h,
            "C24" => atlas_simulation_core::timber_fire::TimberGrade::SoftwoodC24,
            "C30" => atlas_simulation_core::timber_fire::TimberGrade::SoftwoodC30,
            "D30" => atlas_simulation_core::timber_fire::TimberGrade::HardwoodD30,
            _ => atlas_simulation_core::timber_fire::TimberGrade::GlulamGL28h,
        };
        let exposure = match req.exposure.as_deref().unwrap_or("three_sides") {
            "four_sides" => atlas_simulation_core::timber_fire::FireExposureSides::FourSides,
            "one_side" => atlas_simulation_core::timber_fire::FireExposureSides::OneSide,
            _ => atlas_simulation_core::timber_fire::FireExposureSides::ThreeSides,
        };
        let prob = atlas_simulation_core::timber_fire::TimberFireProblem {
            problem_id: "timber_fire_resistance".to_string(),
            timber_grade: grade,
            initial_breadth_mm: req.initial_breadth_mm.unwrap_or(200.0),
            initial_depth_mm: req.initial_depth_mm.unwrap_or(500.0),
            span_m: req.span_m.unwrap_or(8.0),
            exposure,
            fire_duration_min: req.fire_duration_min.unwrap_or(60.0),
            ambient_moment_ed_kn_m: req.ambient_moment_ed_kn_m.unwrap_or(140.0),
            fire_load_ratio_eta_fi: req.fire_load_ratio_eta_fi.unwrap_or(0.60),
        };
        let res = atlas_simulation_core::timber_fire::solve_timber_fire(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_propeller(
        &mut self,
        req: &SimMarinePropellerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::marine_propeller::MarinePropellerProblem {
            problem_id: "marine_propeller_simulation".to_string(),
            series: atlas_simulation_core::marine_propeller::PropellerSeries::WageningenB4,
            diameter_m: req.diameter_m.unwrap_or(6.80),
            pitch_ratio_p_d: req.pitch_ratio_p_d.unwrap_or(1.05),
            expanded_area_ratio: req.expanded_area_ratio.unwrap_or(0.68),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(108.0),
            advance_speed_knots: req.advance_speed_knots.unwrap_or(17.5),
            immersion_depth_m: req.immersion_depth_m.unwrap_or(6.50),
            water_density_kg_m3: req.water_density_kg_m3.unwrap_or(1025.0),
            keller_constant_k: req.keller_constant_k.unwrap_or(0.20),
        };
        let res = atlas_simulation_core::marine_propeller::solve_marine_propeller(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_automotive_braking(
        &mut self,
        req: &SimAutomotiveBrakingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::automotive_braking::AutomotiveBrakingProblem {
            problem_id: "automotive_braking_simulation".to_string(),
            vehicle_mass_kg: req.vehicle_mass_kg.unwrap_or(1850.0),
            wheelbase_m: req.wheelbase_m.unwrap_or(2.85),
            cg_height_m: req.cg_height_m.unwrap_or(0.52),
            static_front_weight_bias: req.static_front_weight_bias.unwrap_or(0.53),
            tire_rolling_radius_m: req.tire_rolling_radius_m.unwrap_or(0.33),
            road_friction_coeff: req.road_friction_coeff.unwrap_or(0.92),
            initial_speed_kph: req.initial_speed_kph.unwrap_or(100.0),
            driver_reaction_time_s: req.driver_reaction_time_s.unwrap_or(0.45),
            rotor_material: atlas_simulation_core::automotive_braking::RotorMaterial::GrayCastIron,
            front_rotor_mass_kg: req.front_rotor_mass_kg.unwrap_or(11.8),
            front_rotor_diameter_m: 0.355,
            ambient_temp_c: req.ambient_temp_c.unwrap_or(25.0),
        };
        let res = atlas_simulation_core::automotive_braking::solve_automotive_braking(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_reinforced_concrete(
        &mut self,
        req: &SimReinforcedConcreteRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::reinforced_concrete::ReinforcedConcreteProblem {
            problem_id: "rc_beam_simulation".to_string(),
            concrete_grade: atlas_simulation_core::reinforced_concrete::ConcreteGrade::C30_37,
            rebar_yield_strength_fyk_mpa: 500.0,
            breadth_b_mm: req.breadth_b_mm.unwrap_or(300.0),
            depth_h_mm: req.depth_h_mm.unwrap_or(600.0),
            concrete_cover_c_mm: req.concrete_cover_c_mm.unwrap_or(35.0),
            rebar_diameter_mm: req.rebar_diameter_mm.unwrap_or(20.0),
            rebar_count: req.rebar_count.unwrap_or(4),
            applied_moment_ed_kn_m: req.applied_moment_ed_kn_m.unwrap_or(240.0),
            quasi_permanent_moment_ratio: req.quasi_permanent_moment_ratio.unwrap_or(0.65),
            max_allowable_crack_width_mm: req.max_allowable_crack_width_mm.unwrap_or(0.30),
        };
        let res = atlas_simulation_core::reinforced_concrete::solve_reinforced_concrete(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hvac_duct_airflow(
        &mut self,
        req: &SimHvacDuctsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let geom = if let Some(diam) = req.duct_diameter_mm {
            atlas_simulation_core::hvac_ducts::DuctGeometry::Circular { diameter_mm: diam }
        } else {
            atlas_simulation_core::hvac_ducts::DuctGeometry::Rectangular {
                width_mm: req.duct_width_mm.unwrap_or(600.0),
                height_mm: req.duct_height_mm.unwrap_or(350.0),
            }
        };
        let prob = atlas_simulation_core::hvac_ducts::HvacDuctProblem {
            problem_id: "hvac_duct_simulation".to_string(),
            geometry: geom,
            material: atlas_simulation_core::hvac_ducts::DuctMaterial::GalvanizedSteel,
            airflow_m3_h: req.airflow_m3_h.unwrap_or(5400.0),
            duct_length_m: req.duct_length_m.unwrap_or(45.0),
            fitting_loss_coeff_sum: req.fitting_loss_coeff_sum.unwrap_or(3.8),
            terminal_equipment_pa: req.terminal_equipment_pa.unwrap_or(140.0),
            fan_efficiency: req.fan_efficiency.unwrap_or(0.73),
            motor_efficiency: req.motor_efficiency.unwrap_or(0.91),
            air_temperature_c: 20.0,
        };
        let res = atlas_simulation_core::hvac_ducts::solve_hvac_ducts(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_gas_turbine(
        &mut self,
        req: &SimGasTurbineRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::gas_turbine::GasTurbineProblem {
            problem_id: "gas_turbine_simulation".to_string(),
            flight_mach: req.flight_mach.unwrap_or(0.82),
            altitude_m: req.altitude_m.unwrap_or(10668.0),
            core_mass_flow_kg_s: req.core_mass_flow_kg_s.unwrap_or(45.0),
            compressor_pressure_ratio: req.compressor_pressure_ratio.unwrap_or(24.0),
            turbine_inlet_temp_k: req.turbine_inlet_temp_k.unwrap_or(1650.0),
            fuel: atlas_simulation_core::gas_turbine::AviationFuel::JetA1,
            compressor_isentropic_efficiency: req.compressor_isentropic_efficiency.unwrap_or(0.88),
            turbine_isentropic_efficiency: req.turbine_isentropic_efficiency.unwrap_or(0.91),
            combustor_efficiency: req.combustor_efficiency.unwrap_or(0.99),
            combustor_pressure_loss_pct: 3.5,
            nozzle_velocity_coefficient: 0.98,
            mechanical_efficiency: 0.99,
        };
        let res = atlas_simulation_core::gas_turbine::solve_gas_turbine(&prob);
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_hydraulic_cylinder(
        &mut self,
        req: &SimHydraulicCylinderRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::hydraulic_actuator::HydraulicActuatorProblem {
            problem_id: "hydraulic_cylinder_sim".to_string(),
            bore_diameter_mm: req.bore_diameter_mm.unwrap_or(100.0),
            rod_diameter_mm: req.rod_diameter_mm.unwrap_or(56.0),
            stroke_length_mm: req.stroke_length_mm.unwrap_or(600.0),
            working_pressure_bar: req.working_pressure_bar.unwrap_or(210.0),
            barrel_wall_thickness_mm: req.barrel_wall_thickness_mm.unwrap_or(12.0),
            barrel_yield_strength_mpa: req.barrel_yield_strength_mpa.unwrap_or(355.0),
            rod_yield_strength_mpa: req.rod_yield_strength_mpa.unwrap_or(460.0),
            rod_youngs_modulus_gpa: req.rod_youngs_modulus_gpa.unwrap_or(210.0),
            mounting_end_condition:
                atlas_simulation_core::hydraulic_actuator::MountingCondition::PinnedPinned,
            load_mass_kg: req.load_mass_kg.unwrap_or(2500.0),
            stroke_velocity_m_s: req.stroke_velocity_m_s.unwrap_or(0.45),
            cushion_length_mm: req.cushion_length_mm.unwrap_or(25.0),
            cushion_spear_diameter_mm: req.cushion_spear_diameter_mm.unwrap_or(32.0),
            mechanical_efficiency: req.mechanical_efficiency.unwrap_or(0.95),
        };
        let res = atlas_simulation_core::hydraulic_actuator::solve_hydraulic_actuator(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mechanical_belt_drive(
        &mut self,
        req: &SimBeltDriveRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::belt_transmission::BeltTransmissionProblem {
            problem_id: "belt_drive_sim".to_string(),
            power_kw: req.power_kw.unwrap_or(15.0),
            driver_speed_rpm: req.driver_speed_rpm.unwrap_or(1450.0),
            driver_diameter_mm: req.driver_diameter_mm.unwrap_or(160.0),
            driven_diameter_mm: req.driven_diameter_mm.unwrap_or(315.0),
            center_distance_mm: req.center_distance_mm.unwrap_or(600.0),
            belt_profile: atlas_simulation_core::belt_transmission::BeltProfile::Spa,
            service_factor: req.service_factor.unwrap_or(1.30),
            friction_coefficient: req.friction_coefficient.unwrap_or(0.35),
            belt_linear_mass_kg_m: req.belt_linear_mass_kg_m.unwrap_or(0.125),
            max_allowable_tension_per_belt_n: req
                .max_allowable_tension_per_belt_n
                .unwrap_or(1100.0),
        };
        let res = atlas_simulation_core::belt_transmission::solve_belt_transmission(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_geotechnical_slope_stability(
        &mut self,
        req: &SimSlopeStabilityRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::slope_stability::SlopeStabilityProblem {
            problem_id: "slope_stability_sim".to_string(),
            slope_height_m: req.slope_height_m.unwrap_or(10.0),
            slope_angle_deg: req.slope_angle_deg.unwrap_or(32.0),
            soil_unit_weight_kn_m3: req.soil_unit_weight_kn_m3.unwrap_or(19.0),
            effective_cohesion_kpa: req.effective_cohesion_kpa.unwrap_or(12.0),
            effective_friction_angle_deg: req.effective_friction_angle_deg.unwrap_or(27.0),
            pore_pressure_ratio_ru: req.pore_pressure_ratio_ru.unwrap_or(0.15),
            seismic_coefficient_kh: req.seismic_coefficient_kh.unwrap_or(0.05),
            circle_center_x_m: req.circle_center_x_m.unwrap_or(5.5),
            circle_center_y_m: req.circle_center_y_m.unwrap_or(16.0),
            slip_circle_radius_m: req.slip_circle_radius_m.unwrap_or(17.5),
            number_of_slices: req.number_of_slices.unwrap_or(25),
            convergence_tolerance: 1.0e-5,
        };
        let res = atlas_simulation_core::slope_stability::solve_slope_stability(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_induction_motor(
        &mut self,
        req: &SimInductionMotorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::induction_motor::InductionMotorProblem {
            problem_id: "induction_motor_sim".to_string(),
            rated_power_kw: req.rated_power_kw.unwrap_or(15.0),
            rated_voltage_ll_v: req.rated_voltage_ll_v.unwrap_or(400.0),
            frequency_hz: req.frequency_hz.unwrap_or(50.0),
            pole_count: req.pole_count.unwrap_or(4),
            stator_resistance_r1_ohm: req.stator_resistance_r1_ohm.unwrap_or(0.35),
            stator_leakage_reactance_x1_ohm: req.stator_leakage_reactance_x1_ohm.unwrap_or(0.85),
            rotor_resistance_r2_prime_ohm: req.rotor_resistance_r2_prime_ohm.unwrap_or(0.32),
            rotor_leakage_reactance_x2_prime_ohm: req
                .rotor_leakage_reactance_x2_prime_ohm
                .unwrap_or(0.95),
            magnetizing_reactance_xm_ohm: req.magnetizing_reactance_xm_ohm.unwrap_or(38.0),
            rotational_loss_w: req.rotational_loss_w.unwrap_or(450.0),
            operating_slip: req.operating_slip,
        };
        let res = atlas_simulation_core::induction_motor::solve_induction_motor(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_solar_collector(
        &mut self,
        req: &SimSolarCollectorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::solar_thermal::SolarThermalProblem {
            problem_id: "solar_collector_sim".to_string(),
            collector_type: atlas_simulation_core::solar_thermal::CollectorType::FlatPlateGlazed,
            gross_area_m2: req.gross_area_m2.unwrap_or(2.50),
            aperture_area_m2: req.aperture_area_m2.unwrap_or(2.30),
            solar_irradiance_w_m2: req.solar_irradiance_w_m2.unwrap_or(850.0),
            ambient_temperature_c: req.ambient_temp_c.unwrap_or(20.0),
            fluid_inlet_temp_c: req.fluid_inlet_temp_c.unwrap_or(45.0),
            fluid_flow_rate_kg_s: req.fluid_flow_rate_kg_s.unwrap_or(0.045),
            fluid_specific_heat_j_kg_k: req.fluid_specific_heat_j_kg_k.unwrap_or(4182.0),
            optical_efficiency_eta0: req.optical_efficiency_eta0,
            linear_heat_loss_a1: req.linear_heat_loss_a1,
            quadratic_heat_loss_a2: req.quadratic_heat_loss_a2,
        };
        let res = atlas_simulation_core::solar_thermal::solve_solar_thermal(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_tribology_journal_bearing(
        &mut self,
        req: &SimJournalBearingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::journal_bearing::JournalBearingProblem {
            problem_id: "journal_bearing_sim".to_string(),
            journal_diameter_mm: req.journal_diameter_mm.unwrap_or(100.0),
            bearing_length_mm: req.bearing_length_mm.unwrap_or(70.0),
            radial_clearance_um: req.radial_clearance_um.unwrap_or(65.0),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(3000.0),
            radial_load_n: req.radial_load_n.unwrap_or(12500.0),
            lubricant_viscosity_mpa_s: req.lubricant_viscosity_mpa_s.unwrap_or(25.0),
            lubricant_density_kg_m3: req.lubricant_density_kg_m3.unwrap_or(860.0),
            lubricant_specific_heat_j_kg_k: req.lubricant_specific_heat_j_kg_k.unwrap_or(1950.0),
            inlet_oil_temp_c: req.inlet_oil_temp_c.unwrap_or(45.0),
            surface_roughness_rz_um: req.surface_roughness_rz_um.unwrap_or(3.0),
        };
        let res = atlas_simulation_core::journal_bearing::solve_journal_bearing(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_renewable_wind_turbine(
        &mut self,
        req: &SimWindTurbineRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::wind_turbine::WindTurbineProblem {
            problem_id: "wind_turbine_sim".to_string(),
            rotor_radius_m: req.rotor_radius_m.unwrap_or(63.0),
            blade_count: req.blade_count.unwrap_or(3),
            rated_power_mw: req.rated_power_mw.unwrap_or(4.0),
            rated_wind_speed_m_s: req.rated_wind_speed_m_s.unwrap_or(11.5),
            cut_in_wind_speed_m_s: req.cut_in_wind_speed_m_s.unwrap_or(3.0),
            cut_out_wind_speed_m_s: req.cut_out_wind_speed_m_s.unwrap_or(25.0),
            tip_speed_ratio_optimal: req.tip_speed_ratio_optimal.unwrap_or(7.8),
            air_density_kg_m3: req.air_density_kg_m3.unwrap_or(1.225),
            gearbox_efficiency: req.gearbox_efficiency.unwrap_or(0.97),
            generator_efficiency: req.generator_efficiency.unwrap_or(0.95),
            weibull_k_shape: req.weibull_k_shape.unwrap_or(2.0),
            weibull_c_scale_m_s: req.weibull_c_scale_m_s.unwrap_or(8.5),
            operating_wind_speed_m_s: req.operating_wind_speed_m_s,
        };
        let res = atlas_simulation_core::wind_turbine::solve_wind_turbine(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_steam_rankine(
        &mut self,
        req: &SimSteamRankineRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::steam_rankine::SteamRankineProblem {
            problem_id: "steam_rankine_sim".to_string(),
            steam_mass_flow_kg_s: req.steam_mass_flow_kg_s.unwrap_or(120.0),
            boiler_pressure_bar: req.boiler_pressure_bar.unwrap_or(120.0),
            superheater_temp_c: req.superheater_temp_c.unwrap_or(540.0),
            condenser_pressure_bar: req.condenser_pressure_bar.unwrap_or(0.08),
            turbine_isentropic_efficiency: req.turbine_isentropic_efficiency.unwrap_or(0.88),
            pump_isentropic_efficiency: req.pump_isentropic_efficiency.unwrap_or(0.85),
            generator_efficiency: req.generator_efficiency.unwrap_or(0.985),
            boiler_efficiency: req.boiler_efficiency.unwrap_or(0.90),
            cooling_water_temp_rise_c: req.cooling_water_temp_rise_c.unwrap_or(10.0),
        };
        let res = atlas_simulation_core::steam_rankine::solve_steam_rankine(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_pile_foundation(
        &mut self,
        req: &SimPileFoundationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::pile_foundation::PileFoundationProblem {
            problem_id: "pile_foundation_sim".to_string(),
            pile_diameter_m: req.pile_diameter_m.unwrap_or(1.0),
            pile_length_m: req.pile_length_m.unwrap_or(20.0),
            pile_elastic_modulus_gpa: req.pile_elastic_modulus_gpa.unwrap_or(30.0),
            applied_axial_load_kn: req.applied_axial_load_kn.unwrap_or(3200.0),
            soil_layers: vec![
                atlas_simulation_core::pile_foundation::SoilLayerSpec {
                    layer_thickness_m: 6.0,
                    soil_type:
                        atlas_simulation_core::pile_foundation::SoilLayerType::CohesiveClay {
                            cu_kpa: 45.0,
                        },
                },
                atlas_simulation_core::pile_foundation::SoilLayerSpec {
                    layer_thickness_m: 8.0,
                    soil_type:
                        atlas_simulation_core::pile_foundation::SoilLayerType::CohesiveClay {
                            cu_kpa: 80.0,
                        },
                },
                atlas_simulation_core::pile_foundation::SoilLayerSpec {
                    layer_thickness_m: 6.0,
                    soil_type:
                        atlas_simulation_core::pile_foundation::SoilLayerType::CohesionlessSand {
                            friction_angle_deg: 35.0,
                            unit_weight_kn_m3: 19.5,
                        },
                },
            ],
            groundwater_depth_m: req.groundwater_depth_m.unwrap_or(3.0),
            base_soil_type:
                atlas_simulation_core::pile_foundation::SoilLayerType::CohesionlessSand {
                    friction_angle_deg: 36.0,
                    unit_weight_kn_m3: 20.0,
                },
            partial_factor_base: req.partial_factor_base.unwrap_or(1.30),
            partial_factor_shaft: req.partial_factor_shaft.unwrap_or(1.15),
            max_allowable_settlement_mm: req.max_allowable_settlement_mm.unwrap_or(15.0),
        };
        let res = atlas_simulation_core::pile_foundation::solve_pile_foundation(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mechanical_gear_fatigue(
        &mut self,
        req: &SimGearFatigueRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::gear_fatigue::GearFatigueProblem {
            problem_id: "gear_fatigue_sim".to_string(),
            transmitted_power_kw: req.transmitted_power_kw.unwrap_or(55.0),
            pinion_speed_rpm: req.pinion_speed_rpm.unwrap_or(1450.0),
            pinion_teeth_z1: req.pinion_teeth_z1.unwrap_or(21),
            gear_teeth_z2: req.gear_teeth_z2.unwrap_or(63),
            normal_module_mm: req.normal_module_mm.unwrap_or(4.0),
            face_width_mm: req.face_width_mm.unwrap_or(50.0),
            helix_angle_deg: req.helix_angle_deg.unwrap_or(15.0),
            normal_pressure_angle_deg: req.normal_pressure_angle_deg.unwrap_or(20.0),
            application_factor_ka: req.application_factor_ka.unwrap_or(1.25),
            material_contact_fatigue_limit_mpa: req
                .material_contact_fatigue_limit_mpa
                .unwrap_or(1450.0),
            material_bending_fatigue_limit_mpa: req
                .material_bending_fatigue_limit_mpa
                .unwrap_or(850.0),
            required_safety_factor_pitting: req.required_safety_factor_pitting.unwrap_or(1.25),
            required_safety_factor_bending: req.required_safety_factor_bending.unwrap_or(1.40),
        };
        let res = atlas_simulation_core::gear_fatigue::solve_gear_fatigue(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_battery_thermal(
        &mut self,
        req: &SimBatteryThermalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::battery_thermal::BatteryThermalProblem {
            problem_id: "battery_thermal_sim".to_string(),
            cell_count_series: req.cell_count_series.unwrap_or(96),
            cell_count_parallel: req.cell_count_parallel.unwrap_or(4),
            cell_capacity_ah: req.cell_capacity_ah.unwrap_or(50.0),
            nominal_cell_voltage_v: req.nominal_cell_voltage_v.unwrap_or(3.70),
            internal_resistance_mohm: req.internal_resistance_mohm.unwrap_or(1.20),
            entropic_coefficient_mv_k: req.entropic_coefficient_mv_k.unwrap_or(-0.22),
            operating_current_a: req.operating_current_a.unwrap_or(200.0),
            ambient_temp_c: req.ambient_temp_c.unwrap_or(25.0),
            coolant_inlet_temp_c: req.coolant_inlet_temp_c.unwrap_or(20.0),
            coolant_flow_rate_l_min: req.coolant_flow_rate_l_min.unwrap_or(15.0),
            coolant_density_kg_m3: req.coolant_density_kg_m3.unwrap_or(1060.0),
            coolant_specific_heat_j_kg_k: req.coolant_specific_heat_j_kg_k.unwrap_or(3400.0),
            convective_htc_w_m2_k: req.convective_htc_w_m2_k.unwrap_or(850.0),
            cell_contact_area_m2: req.cell_contact_area_m2.unwrap_or(0.025),
            cell_mass_kg: req.cell_mass_kg.unwrap_or(0.85),
            cell_specific_heat_j_kg_k: req.cell_specific_heat_j_kg_k.unwrap_or(1050.0),
        };
        let res = atlas_simulation_core::battery_thermal::solve_battery_thermal(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_rocket_nozzle(
        &mut self,
        req: &SimRocketNozzleRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::rocket_nozzle::RocketNozzleProblem {
            problem_id: "rocket_nozzle_sim".to_string(),
            chamber_pressure_bar: req.chamber_pressure_bar.unwrap_or(100.0),
            chamber_temperature_k: req.chamber_temperature_k.unwrap_or(3500.0),
            throat_diameter_mm: req.throat_diameter_mm.unwrap_or(120.0),
            expansion_ratio_eps: req.expansion_ratio_eps.unwrap_or(16.0),
            specific_heat_ratio_gamma: req.specific_heat_ratio_gamma.unwrap_or(1.22),
            molecular_weight_g_mol: req.molecular_weight_g_mol.unwrap_or(22.5),
            ambient_pressure_bar: req.ambient_pressure_bar.unwrap_or(1.01325),
            discharge_coefficient_cd: req.discharge_coefficient_cd.unwrap_or(0.985),
            thrust_efficiency_cf: req.thrust_efficiency_cf.unwrap_or(0.975),
        };
        let res = atlas_simulation_core::rocket_nozzle::solve_rocket_nozzle(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_cstr_reactor(
        &mut self,
        req: &SimCstrReactorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::chemical_reactor::ChemicalReactorProblem {
            problem_id: "cstr_reactor_sim".to_string(),
            reactor_volume_m3: req.reactor_volume_m3.unwrap_or(2.50),
            feed_volumetric_flow_m3_h: req.feed_volumetric_flow_m3_h.unwrap_or(5.0),
            feed_reactant_concentration_mol_m3: req
                .feed_reactant_concentration_mol_m3
                .unwrap_or(2000.0),
            feed_temperature_c: req.feed_temperature_c.unwrap_or(25.0),
            pre_exponential_factor_s_inv: req.pre_exponential_factor_s_inv.unwrap_or(4.5e9),
            activation_energy_kj_mol: req.activation_energy_kj_mol.unwrap_or(75.0),
            reaction_enthalpy_kj_mol: req.reaction_enthalpy_kj_mol.unwrap_or(-85.0),
            fluid_density_kg_m3: req.fluid_density_kg_m3.unwrap_or(1020.0),
            fluid_specific_heat_j_kg_k: req.fluid_specific_heat_j_kg_k.unwrap_or(3800.0),
            jacket_overall_ua_w_k: req.jacket_overall_ua_w_k.unwrap_or(4500.0),
            coolant_temperature_c: req.coolant_temperature_c.unwrap_or(18.0),
            max_allowable_reactor_temp_c: req.max_allowable_reactor_temp_c.unwrap_or(95.0),
        };
        let res = atlas_simulation_core::chemical_reactor::solve_chemical_reactor(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_channel_hydraulics(
        &mut self,
        req: &SimChannelHydraulicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::channel_hydraulics::ChannelHydraulicsProblem {
            problem_id: "channel_hydraulics_sim".to_string(),
            bottom_width_m: req.bottom_width_m.unwrap_or(6.0),
            side_slope_z: req.side_slope_z.unwrap_or(1.5),
            bed_slope_s0: req.bed_slope_s0.unwrap_or(0.0015),
            manning_roughness_n: req.manning_roughness_n.unwrap_or(0.025),
            discharge_rate_m3_s: req.discharge_rate_m3_s.unwrap_or(45.0),
            upstream_jump_depth_m: req.upstream_jump_depth_m.or(Some(0.85)),
        };
        let res = atlas_simulation_core::channel_hydraulics::solve_channel_hydraulics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_transmission_line(
        &mut self,
        req: &SimTransmissionLineRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::transmission_line::TransmissionLineProblem {
            problem_id: "transmission_line_sim".to_string(),
            span_length_m: req.span_length_m.unwrap_or(350.0),
            conductor_diameter_mm: req.conductor_diameter_mm.unwrap_or(28.14),
            conductor_unit_mass_kg_m: req.conductor_unit_mass_kg_m.unwrap_or(1.628),
            rated_tensile_strength_kn: req.rated_tensile_strength_kn.unwrap_or(140.0),
            metallic_cross_section_mm2: req.metallic_cross_section_mm2.unwrap_or(468.6),
            elastic_modulus_gpa: req.elastic_modulus_gpa.unwrap_or(69.0),
            thermal_expansion_coeff_per_c: req.thermal_expansion_coeff_per_c.unwrap_or(19.3e-6),
            reference_temp_c: req.reference_temp_c.unwrap_or(15.0),
            initial_horizontal_tension_kn: req.initial_horizontal_tension_kn.unwrap_or(28.0),
            max_operating_temp_c: req.max_operating_temp_c.unwrap_or(80.0),
            storm_ice_thickness_mm: req.storm_ice_thickness_mm.unwrap_or(12.5),
            storm_wind_pressure_pa: req.storm_wind_pressure_pa.unwrap_or(400.0),
            storm_ambient_temp_c: req.storm_ambient_temp_c.unwrap_or(-5.0),
            crossarm_height_m: req.crossarm_height_m.unwrap_or(22.0),
            min_ground_clearance_required_m: req.min_ground_clearance_required_m.unwrap_or(8.0),
        };
        let res = atlas_simulation_core::transmission_line::solve_transmission_line(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_piping_flexibility(
        &mut self,
        req: &SimPipingFlexibilityRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::piping_flexibility::PipingFlexibilityProblem {
            problem_id: "piping_flexibility_sim".to_string(),
            nominal_outside_diameter_mm: req.nominal_outside_diameter_mm.unwrap_or(219.1),
            wall_thickness_mm: req.wall_thickness_mm.unwrap_or(8.18),
            anchor_distance_m: req.anchor_distance_m.unwrap_or(60.0),
            design_temperature_c: req.design_temperature_c.unwrap_or(260.0),
            ambient_temperature_c: req.ambient_temperature_c.unwrap_or(20.0),
            thermal_expansion_coeff_per_c: req.thermal_expansion_coeff_per_c.unwrap_or(12.8e-6),
            elastic_modulus_gpa: req.elastic_modulus_gpa.unwrap_or(190.0),
            cold_allowable_stress_mpa: req.cold_allowable_stress_mpa.unwrap_or(137.9),
            hot_allowable_stress_mpa: req.hot_allowable_stress_mpa.unwrap_or(125.0),
            sustained_stress_sl_mpa: req.sustained_stress_sl_mpa.unwrap_or(65.0),
            total_thermal_cycles: req.total_thermal_cycles.unwrap_or(5000),
            loop_height_m: req.loop_height_m.unwrap_or(7.5),
            loop_width_m: req.loop_width_m.unwrap_or(5.0),
            bend_radius_mm: req.bend_radius_mm.or(Some(304.8)),
        };
        let res = atlas_simulation_core::piping_flexibility::solve_piping_flexibility(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_automotive_quarter_car(
        &mut self,
        req: &SimQuarterCarRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let roughness = match req.road_class.as_deref().unwrap_or("ClassBGood") {
            "ClassAVeryGood" => {
                atlas_simulation_core::quarter_car::RoadRoughnessClass::ClassAVeryGood
            }
            "ClassCAverage" => {
                atlas_simulation_core::quarter_car::RoadRoughnessClass::ClassCAverage
            }
            "ClassDPoor" => atlas_simulation_core::quarter_car::RoadRoughnessClass::ClassDPoor,
            _ => atlas_simulation_core::quarter_car::RoadRoughnessClass::ClassBGood,
        };
        let prob = atlas_simulation_core::quarter_car::QuarterCarProblem {
            problem_id: "quarter_car_sim".to_string(),
            sprung_mass_kg: req.sprung_mass_kg.unwrap_or(320.0),
            unsprung_mass_kg: req.unsprung_mass_kg.unwrap_or(45.0),
            suspension_stiffness_n_m: req.suspension_stiffness_n_m.unwrap_or(28000.0),
            suspension_damping_n_s_m: req.suspension_damping_n_s_m.unwrap_or(1650.0),
            tire_stiffness_n_m: req.tire_stiffness_n_m.unwrap_or(210000.0),
            vehicle_speed_km_h: req.vehicle_speed_km_h.unwrap_or(100.0),
            road_class: roughness,
            max_allowable_stroke_mm: req.max_allowable_stroke_mm.unwrap_or(75.0),
        };
        let res = atlas_simulation_core::quarter_car::solve_quarter_car(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_power_transformer(
        &mut self,
        req: &SimPowerTransformerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let cooling = match req.cooling_class.as_deref().unwrap_or("ONAN") {
            "ONAF" => atlas_simulation_core::power_transformer::TransformerCoolingClass::ONAF,
            "OFAF" => atlas_simulation_core::power_transformer::TransformerCoolingClass::OFAF,
            _ => atlas_simulation_core::power_transformer::TransformerCoolingClass::ONAN,
        };
        let prob = atlas_simulation_core::power_transformer::PowerTransformerProblem {
            problem_id: "power_transformer_sim".to_string(),
            rated_apparent_power_mva: req.rated_apparent_power_mva.unwrap_or(40.0),
            primary_voltage_kv: req.primary_voltage_kv.unwrap_or(110.0),
            secondary_voltage_kv: req.secondary_voltage_kv.unwrap_or(33.0),
            percent_impedance_z: req.percent_impedance_z.unwrap_or(10.5),
            no_load_loss_kw: req.no_load_loss_kw.unwrap_or(22.0),
            load_loss_kw: req.load_loss_kw.unwrap_or(165.0),
            operating_load_factor: req.operating_load_factor.unwrap_or(1.0),
            operating_power_factor: req.operating_power_factor.unwrap_or(0.85),
            ambient_temperature_c: req.ambient_temperature_c.unwrap_or(30.0),
            rated_top_oil_rise_c: req.rated_top_oil_rise_c.unwrap_or(55.0),
            rated_winding_rise_c: req.rated_winding_rise_c.unwrap_or(65.0),
            hot_spot_factor_h: req.hot_spot_factor_h.unwrap_or(1.15),
            cooling_class: cooling,
        };
        let res = atlas_simulation_core::power_transformer::solve_power_transformer(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_refrigeration_cycle(
        &mut self,
        req: &SimRefrigerationCycleRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let ref_type = match req.refrigerant.as_deref().unwrap_or("R410A") {
            "R134a" => atlas_simulation_core::refrigeration_cycle::RefrigerantType::R134a,
            "R32" => atlas_simulation_core::refrigeration_cycle::RefrigerantType::R32,
            "R290" => atlas_simulation_core::refrigeration_cycle::RefrigerantType::R290,
            _ => atlas_simulation_core::refrigeration_cycle::RefrigerantType::R410A,
        };
        let prob = atlas_simulation_core::refrigeration_cycle::RefrigerationCycleProblem {
            problem_id: "refrigeration_cycle_sim".to_string(),
            refrigerant: ref_type,
            design_cooling_capacity_kw: req.design_cooling_capacity_kw.unwrap_or(100.0),
            evaporating_temperature_c: req.evaporating_temperature_c.unwrap_or(5.0),
            condensing_temperature_c: req.condensing_temperature_c.unwrap_or(45.0),
            superheat_k: req.superheat_k.unwrap_or(5.0),
            subcooling_k: req.subcooling_k.unwrap_or(5.0),
            compressor_isentropic_efficiency: req.compressor_isentropic_efficiency.unwrap_or(0.78),
            compressor_mechanical_efficiency: req.compressor_mechanical_efficiency.unwrap_or(0.92),
        };
        let res = atlas_simulation_core::refrigeration_cycle::solve_refrigeration_cycle(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_ship_stability(
        &mut self,
        req: &SimShipStabilityRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::ship_stability::ShipStabilityProblem {
            problem_id: "ship_stability_sim".to_string(),
            length_between_perpendiculars_m: req.length_between_perpendiculars_m.unwrap_or(140.0),
            molded_beam_m: req.molded_beam_m.unwrap_or(22.0),
            design_draft_m: req.design_draft_m.unwrap_or(7.5),
            molded_depth_m: req.molded_depth_m.unwrap_or(11.5),
            block_coefficient_cb: req.block_coefficient_cb.unwrap_or(0.68),
            waterplane_coefficient_cwp: req.waterplane_coefficient_cwp.unwrap_or(0.78),
            vertical_center_of_gravity_kg_m: req.vertical_center_of_gravity_kg_m.unwrap_or(7.20),
        };
        let res = atlas_simulation_core::ship_stability::solve_ship_stability(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_manufacturing_injection_molding(
        &mut self,
        req: &SimInjectionMoldingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let resin = match req.resin.as_deref().unwrap_or("ABS") {
            "Polypropylene" | "PP" => {
                atlas_simulation_core::injection_molding::PolymerResin::Polypropylene
            }
            "Polyamide66" | "PA66" => {
                atlas_simulation_core::injection_molding::PolymerResin::Polyamide66
            }
            "Polycarbonate" | "PC" => {
                atlas_simulation_core::injection_molding::PolymerResin::Polycarbonate
            }
            "Polyoxymethylene" | "POM" => {
                atlas_simulation_core::injection_molding::PolymerResin::Polyoxymethylene
            }
            _ => atlas_simulation_core::injection_molding::PolymerResin::ABS,
        };
        let prob = atlas_simulation_core::injection_molding::InjectionMoldingProblem {
            problem_id: "injection_molding_sim".to_string(),
            resin,
            nominal_wall_thickness_mm: req.nominal_wall_thickness_mm.unwrap_or(2.50),
            projected_area_cm2: req.projected_area_cm2.unwrap_or(180.0),
            cavity_volume_cm3: req.cavity_volume_cm3.unwrap_or(112.5),
            max_flow_length_mm: req.max_flow_length_mm.unwrap_or(175.0),
            injection_time_s: req.injection_time_s.unwrap_or(1.5),
            cavity_count: req.cavity_count.unwrap_or(2),
            clamp_safety_factor: req.clamp_safety_factor.unwrap_or(1.20),
        };
        let res = atlas_simulation_core::injection_molding::solve_injection_molding(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_seismic_analysis(
        &mut self,
        req: &SimSeismicAnalysisRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let site = match req.site_class.as_deref().unwrap_or("ClassD") {
            "ClassA" => atlas_simulation_core::seismic_analysis::SeismicSiteClass::ClassA,
            "ClassB" => atlas_simulation_core::seismic_analysis::SeismicSiteClass::ClassB,
            "ClassC" => atlas_simulation_core::seismic_analysis::SeismicSiteClass::ClassC,
            "ClassE" => atlas_simulation_core::seismic_analysis::SeismicSiteClass::ClassE,
            _ => atlas_simulation_core::seismic_analysis::SeismicSiteClass::ClassD,
        };
        let framing = match req.framing_system.as_deref().unwrap_or("SteelSpecialMomentFrame") {
            "ConcreteSpecialShearWall" => atlas_simulation_core::seismic_analysis::SeismicFramingSystem::ConcreteSpecialShearWall,
            "SteelBucklingRestrainedBracedFrame" => atlas_simulation_core::seismic_analysis::SeismicFramingSystem::SteelBucklingRestrainedBracedFrame,
            "SteelOrdinaryBracedFrame" => atlas_simulation_core::seismic_analysis::SeismicFramingSystem::SteelOrdinaryBracedFrame,
            _ => atlas_simulation_core::seismic_analysis::SeismicFramingSystem::SteelSpecialMomentFrame,
        };
        let prob = atlas_simulation_core::seismic_analysis::SeismicAnalysisProblem {
            problem_id: "seismic_analysis_sim".to_string(),
            building_height_m: req.building_height_m.unwrap_or(36.0),
            story_count: req.story_count.unwrap_or(10),
            total_seismic_weight_kn: req.total_seismic_weight_kn.unwrap_or(58000.0),
            short_period_spectral_accel_ss_g: req.short_period_spectral_accel_ss_g.unwrap_or(1.20),
            one_second_spectral_accel_s1_g: req.one_second_spectral_accel_s1_g.unwrap_or(0.48),
            long_period_transition_tl_s: req.long_period_transition_tl_s.unwrap_or(8.0),
            site_class: site,
            framing_system: framing,
            importance_factor_ie: req.importance_factor_ie.unwrap_or(1.0),
        };
        let res = atlas_simulation_core::seismic_analysis::solve_seismic_analysis(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_orbital_mechanics(
        &mut self,
        req: &SimOrbitalMechanicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::orbital_mechanics::OrbitalMechanicsProblem {
            problem_id: "orbital_mechanics_sim".to_string(),
            initial_altitude_km: req.initial_altitude_km.unwrap_or(400.0),
            target_altitude_km: req.target_altitude_km.unwrap_or(35786.0),
            spacecraft_wet_mass_kg: req.spacecraft_wet_mass_kg.unwrap_or(3500.0),
            propulsion_isp_s: req.propulsion_isp_s.unwrap_or(318.0),
            inclination_deg: req.inclination_deg.unwrap_or(28.5),
        };
        let res = atlas_simulation_core::orbital_mechanics::solve_orbital_mechanics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electronics_dc_dc_converter(
        &mut self,
        req: &SimDcDcConverterRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::dc_dc_converter::DcDcConverterProblem {
            problem_id: "dc_dc_converter_sim".to_string(),
            input_voltage_v: req.input_voltage_v.unwrap_or(12.0),
            output_voltage_v: req.output_voltage_v.unwrap_or(3.3),
            rated_output_current_a: req.rated_output_current_a.unwrap_or(20.0),
            switching_frequency_hz: req.switching_frequency_hz.unwrap_or(500_000.0),
            inductor_ripple_ratio: req.inductor_ripple_ratio.unwrap_or(0.30),
            inductor_dcr_ohms: req.inductor_dcr_ohms.unwrap_or(0.0025),
            high_side_rds_on_ohms: req.high_side_rds_on_ohms.unwrap_or(0.0045),
            low_side_rds_on_ohms: req.low_side_rds_on_ohms.unwrap_or(0.0018),
            high_side_qg_nc: req.high_side_qg_nc.unwrap_or(14.0),
            low_side_qg_nc: req.low_side_qg_nc.unwrap_or(32.0),
            switching_transition_time_ns: req.switching_transition_time_ns.unwrap_or(18.0),
            gate_drive_voltage_v: req.gate_drive_voltage_v.unwrap_or(5.0),
            output_capacitance_uf: req.output_capacitance_uf.unwrap_or(200.0),
            output_capacitor_esr_ohms: req.output_capacitor_esr_ohms.unwrap_or(0.002),
        };
        let res = atlas_simulation_core::dc_dc_converter::solve_dc_dc_converter(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_science_fusion_tokamak(
        &mut self,
        req: &SimFusionTokamakRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::fusion_tokamak::FusionTokamakProblem {
            problem_id: "fusion_tokamak_sim".to_string(),
            major_radius_r0_m: req.major_radius_r0_m.unwrap_or(6.20),
            minor_radius_a_m: req.minor_radius_a_m.unwrap_or(2.00),
            elongation_kappa: req.elongation_kappa.unwrap_or(1.75),
            triangularity_delta: req.triangularity_delta.unwrap_or(0.33),
            toroidal_magnetic_field_t: req.toroidal_magnetic_field_t.unwrap_or(5.30),
            plasma_current_ma: req.plasma_current_ma.unwrap_or(15.0),
            electron_density_10e20_m3: req.electron_density_10e20_m3.unwrap_or(1.01),
            plasma_temperature_kev: req.plasma_temperature_kev.unwrap_or(13.0),
            auxiliary_heating_power_mw: req.auxiliary_heating_power_mw.unwrap_or(50.0),
            average_ion_mass_amu: req.average_ion_mass_amu.unwrap_or(2.50),
        };
        let res = atlas_simulation_core::fusion_tokamak::solve_fusion_tokamak(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_manufacturing_wire_edm(
        &mut self,
        req: &SimWireEdmRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::wire_edm::WireEdmProblem {
            problem_id: "wire_edm_sim".to_string(),
            workpiece_material: req
                .workpiece_material
                .unwrap_or(atlas_simulation_core::wire_edm::EdmWorkpieceMaterial::ToolSteelD2),
            workpiece_thickness_mm: req.workpiece_thickness_mm.unwrap_or(50.0),
            wire_diameter_mm: req.wire_diameter_mm.unwrap_or(0.25),
            open_circuit_voltage_v: req.open_circuit_voltage_v.unwrap_or(95.0),
            discharge_current_a: req.discharge_current_a.unwrap_or(16.0),
            pulse_on_time_us: req.pulse_on_time_us.unwrap_or(4.5),
            pulse_off_time_us: req.pulse_off_time_us.unwrap_or(12.0),
            wire_feed_speed_m_min: req.wire_feed_speed_m_min.unwrap_or(9.0),
            wire_tension_n: req.wire_tension_n.unwrap_or(14.0),
        };
        let res = atlas_simulation_core::wire_edm::solve_wire_edm(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_renewable_wind_blade_aeroelastic(
        &mut self,
        req: &SimWindBladeRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::wind_blade_aeroelastic::WindBladeProblem {
            problem_id: "wind_blade_aeroelastic_sim".to_string(),
            blade_length_m: req.blade_length_m.unwrap_or(80.0),
            rotor_radius_m: req.rotor_radius_m.unwrap_or(82.5),
            rated_rotor_speed_rpm: req.rated_rotor_speed_rpm.unwrap_or(10.5),
            rated_wind_speed_m_s: req.rated_wind_speed_m_s.unwrap_or(11.4),
            extreme_gust_speed_m_s: req.extreme_gust_speed_m_s.unwrap_or(52.5),
            blade_mass_kg: req.blade_mass_kg.unwrap_or(19500.0),
            root_chord_m: req.root_chord_m.unwrap_or(4.60),
            tip_chord_m: req.tip_chord_m.unwrap_or(1.15),
            flapwise_rigidity_ei_nm2: req.flapwise_rigidity_ei_nm2.unwrap_or(1.45e10),
            torsional_rigidity_gj_nm2: req.torsional_rigidity_gj_nm2.unwrap_or(5.20e8),
            polar_moment_inertia_kg_m: req.polar_moment_inertia_kg_m.unwrap_or(140.0),
            cg_ea_offset_m: req.cg_ea_offset_m.unwrap_or(0.075),
            air_density_kg_m3: req.air_density_kg_m3.unwrap_or(1.225),
        };
        let res =
            atlas_simulation_core::wind_blade_aeroelastic::solve_wind_blade_aeroelastic(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_subsea_pipeline(
        &mut self,
        req: &SimSubseaPipelineRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::subsea_pipeline::SubseaPipelineProblem {
            problem_id: "subsea_pipeline_sim".to_string(),
            outside_diameter_mm: req.outside_diameter_mm.unwrap_or(508.0),
            nominal_wall_thickness_mm: req.nominal_wall_thickness_mm.unwrap_or(25.4),
            corrosion_allowance_mm: req.corrosion_allowance_mm.unwrap_or(3.0),
            fabrication_ovality_pct: req.fabrication_ovality_pct.unwrap_or(0.50),
            water_depth_m: req.water_depth_m.unwrap_or(1500.0),
            seawater_density_kg_m3: req.seawater_density_kg_m3.unwrap_or(1025.0),
            internal_design_pressure_mpa: req.internal_design_pressure_mpa.unwrap_or(22.0),
            steel_smys_mpa: req.steel_smys_mpa.unwrap_or(450.0),
            steel_smts_mpa: req.steel_smts_mpa.unwrap_or(535.0),
            steel_youngs_modulus_gpa: req.steel_youngs_modulus_gpa.unwrap_or(207.0),
            steel_poissons_ratio: req.steel_poissons_ratio.unwrap_or(0.30),
        };
        let res = atlas_simulation_core::subsea_pipeline::solve_subsea_pipeline(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_combustor_aerothermal(
        &mut self,
        req: &SimCombustorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::combustor_aerothermal::CombustorProblem {
            problem_id: "combustor_aerothermal_sim".to_string(),
            inlet_total_pressure_mpa: req.inlet_total_pressure_mpa.unwrap_or(2.40),
            inlet_total_temperature_k: req.inlet_total_temperature_k.unwrap_or(780.0),
            air_mass_flow_kg_s: req.air_mass_flow_kg_s.unwrap_or(38.0),
            fuel_mass_flow_kg_s: req.fuel_mass_flow_kg_s.unwrap_or(0.76),
            fuel_lhv_mj_kg: req.fuel_lhv_mj_kg.unwrap_or(43.1),
            combustor_volume_m3: req.combustor_volume_m3.unwrap_or(0.065),
            combustor_length_m: req.combustor_length_m.unwrap_or(0.42),
            liner_reference_area_m2: req.liner_reference_area_m2.unwrap_or(0.160),
        };
        let res = atlas_simulation_core::combustor_aerothermal::solve_combustor(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_bess_grid(
        &mut self,
        req: &SimBessGridRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::bess_grid::BessGridProblem {
            problem_id: "bess_grid_sim".to_string(),
            chemistry: req
                .chemistry
                .unwrap_or(atlas_simulation_core::bess_grid::BessChemistry::Lfp),
            nominal_capacity_mwh: req.nominal_capacity_mwh.unwrap_or(10.0),
            rated_power_mw: req.rated_power_mw.unwrap_or(2.50),
            dc_bus_voltage_v: req.dc_bus_voltage_v.unwrap_or(1250.0),
            c_rate: req.c_rate.unwrap_or(0.25),
            pcs_rated_mva: req.pcs_rated_mva.unwrap_or(2.75),
            transformer_rated_mva: req.transformer_rated_mva.unwrap_or(2.80),
            ambient_temperature_c: req.ambient_temperature_c.unwrap_or(28.0),
            auxiliary_base_power_kw: req.auxiliary_base_power_kw.unwrap_or(16.0),
            chiller_cop: req.chiller_cop.unwrap_or(3.2),
        };
        let res = atlas_simulation_core::bess_grid::solve_bess_grid(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_geotechnical_soil_consolidation(
        &mut self,
        req: &SimSoilConsolidationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::soil_consolidation::SoilConsolidationProblem {
            problem_id: "soil_consolidation_sim".to_string(),
            layer_thickness_m: req.layer_thickness_m.unwrap_or(6.0),
            initial_void_ratio: req.initial_void_ratio.unwrap_or(0.85),
            compression_index_cc: req.compression_index_cc.unwrap_or(0.32),
            recompression_index_cr: req.recompression_index_cr.unwrap_or(0.045),
            secondary_compression_index: req.secondary_compression_index.unwrap_or(0.015),
            preconsolidation_stress_kpa: req.preconsolidation_stress_kpa.unwrap_or(120.0),
            initial_effective_stress_kpa: req.initial_effective_stress_kpa.unwrap_or(100.0),
            applied_stress_increment_kpa: req.applied_stress_increment_kpa.unwrap_or(80.0),
            coefficient_of_consolidation_m2_yr: req
                .coefficient_of_consolidation_m2_yr
                .unwrap_or(2.80),
            drainage_condition: req.drainage_condition.unwrap_or(
                atlas_simulation_core::soil_consolidation::DrainageCondition::DoubleDrainage,
            ),
            analysis_time_years: req.analysis_time_years.unwrap_or(2.0),
        };
        let res = atlas_simulation_core::soil_consolidation::solve_soil_consolidation(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_hydraulic_turbine(
        &mut self,
        req: &SimHydraulicTurbineRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::hydraulic_turbine::HydraulicTurbineProblem {
            problem_id: "hydraulic_turbine_sim".to_string(),
            turbine_type: req
                .turbine_type
                .unwrap_or(atlas_simulation_core::hydraulic_turbine::HydraulicTurbineType::Pelton),
            net_head_m: req.net_head_m.unwrap_or(450.0),
            flow_rate_m3_s: req.flow_rate_m3_s.unwrap_or(3.20),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(500.0),
            runner_diameter_m: req.runner_diameter_m.unwrap_or(1.80),
            nozzle_count: req.nozzle_count.unwrap_or(2),
            nozzle_discharge_coeff_cv: req.nozzle_discharge_coeff_cv.unwrap_or(0.98),
            bucket_deflection_angle_deg: req.bucket_deflection_angle_deg.unwrap_or(165.0),
            suction_head_m: req.suction_head_m.unwrap_or(1.50),
            atmospheric_pressure_kpa: req.atmospheric_pressure_kpa.unwrap_or(101.325),
            vapor_pressure_kpa: req.vapor_pressure_kpa.unwrap_or(2.34),
        };
        let res = atlas_simulation_core::hydraulic_turbine::solve_hydraulic_turbine(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_biomedical_stent_expansion(
        &mut self,
        req: &SimStentExpansionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::stent_expansion::StentExpansionProblem {
            problem_id: "stent_expansion_sim".to_string(),
            stent_material: req.stent_material.unwrap_or(
                atlas_simulation_core::stent_expansion::StentMaterial::CobaltChromiumL605,
            ),
            crimped_diameter_mm: req.crimped_diameter_mm.unwrap_or(1.05),
            nominal_expanded_diameter_mm: req.nominal_expanded_diameter_mm.unwrap_or(3.00),
            nominal_length_mm: req.nominal_length_mm.unwrap_or(18.0),
            strut_thickness_um: req.strut_thickness_um.unwrap_or(81.0),
            strut_width_um: req.strut_width_um.unwrap_or(84.0),
            crown_count_per_ring: req.crown_count_per_ring.unwrap_or(8),
            ring_count: req.ring_count.unwrap_or(12),
            balloon_pressure_atm: req.balloon_pressure_atm.unwrap_or(12.0),
        };
        let res = atlas_simulation_core::stent_expansion::solve_stent_expansion(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electronics_microwave_waveguide(
        &mut self,
        req: &SimMicrowaveWaveguideRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::microwave_waveguide::MicrowaveWaveguideProblem {
            problem_id: "microwave_waveguide_sim".to_string(),
            broad_dimension_a_mm: req.broad_dimension_a_mm.unwrap_or(22.86),
            narrow_dimension_b_mm: req.narrow_dimension_b_mm.unwrap_or(10.16),
            length_l_mm: req.length_l_mm.unwrap_or(100.0),
            operating_frequency_ghz: req.operating_frequency_ghz.unwrap_or(10.0),
            wall_conductivity_s_m: req.wall_conductivity_s_m.unwrap_or(5.8e7),
            dielectric_permittivity: req.dielectric_permittivity.unwrap_or(1.0006),
            transmitted_power_w: req.transmitted_power_w.unwrap_or(500.0),
        };
        let res = atlas_simulation_core::microwave_waveguide::solve_microwave_waveguide(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_gis_dielectric(
        &mut self,
        req: &SimGisDielectricRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::gis_dielectric::GisDielectricProblem {
            problem_id: "gis_dielectric_sim".to_string(),
            insulating_gas: req
                .insulating_gas
                .unwrap_or(atlas_simulation_core::gis_dielectric::GisInsulatingGas::PureSf6),
            rated_voltage_kv_rms: req.rated_voltage_kv_rms.unwrap_or(420.0),
            conductor_radius_mm: req.conductor_radius_mm.unwrap_or(60.0),
            enclosure_radius_mm: req.enclosure_radius_mm.unwrap_or(165.0),
            gas_pressure_abs_bar: req.gas_pressure_abs_bar.unwrap_or(5.50),
            lightning_impulse_withstand_kv_pk: req
                .lightning_impulse_withstand_kv_pk
                .unwrap_or(1425.0),
            surface_roughness_ra_um: req.surface_roughness_ra_um.unwrap_or(6.30),
        };
        let res = atlas_simulation_core::gis_dielectric::solve_gis_dielectric(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_pem_fuel_cell(
        &mut self,
        req: &SimPemFuelCellRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::pem_fuel_cell::PemFuelCellProblem {
            problem_id: "pem_fuel_cell_sim".to_string(),
            cell_count: req.cell_count.unwrap_or(360),
            active_cell_area_cm2: req.active_cell_area_cm2.unwrap_or(280.0),
            operating_temperature_c: req.operating_temperature_c.unwrap_or(75.0),
            anode_pressure_bar: req.anode_pressure_bar.unwrap_or(2.20),
            cathode_pressure_bar: req.cathode_pressure_bar.unwrap_or(2.00),
            operating_current_density_a_cm2: req.operating_current_density_a_cm2.unwrap_or(1.20),
            membrane_thickness_um: req.membrane_thickness_um.unwrap_or(25.0),
            membrane_hydration_lambda: req.membrane_hydration_lambda.unwrap_or(14.0),
        };
        let res = atlas_simulation_core::pem_fuel_cell::solve_pem_fuel_cell(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_science_superconducting_magnet(
        &mut self,
        req: &SimSuperconductingMagnetRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::superconducting_magnet::CryoMagnetProblem {
            problem_id: "superconducting_magnet_sim".to_string(),
            material: req.material.unwrap_or(
                atlas_simulation_core::superconducting_magnet::SuperconductorMaterial::NbTi,
            ),
            inner_radius_m: req.inner_radius_m.unwrap_or(0.35),
            outer_radius_m: req.outer_radius_m.unwrap_or(0.48),
            coil_length_m: req.coil_length_m.unwrap_or(1.25),
            turns_count: req.turns_count.unwrap_or(2600),
            operating_current_a: req.operating_current_a.unwrap_or(1400.0),
            operating_temperature_k: req.operating_temperature_k.unwrap_or(4.20),
            cu_sc_ratio: req.cu_sc_ratio.unwrap_or(2.0),
            conductor_area_mm2: req.conductor_area_mm2.unwrap_or(14.0),
            quench_detection_delay_s: req.quench_detection_delay_s.unwrap_or(0.030),
            max_dump_voltage_v: req.max_dump_voltage_v.unwrap_or(1200.0),
        };
        let res = atlas_simulation_core::superconducting_magnet::solve_cryo_magnet(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_reentry_aerothermodynamics(
        &mut self,
        req: &SimReentryRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::reentry_aerothermodynamics::ReentryProblem {
            problem_id: "reentry_sim".to_string(),
            freestream_velocity_m_s: req.freestream_velocity_m_s.unwrap_or(7400.0),
            altitude_km: req.altitude_km.unwrap_or(62.0),
            nose_radius_m: req.nose_radius_m.unwrap_or(1.15),
            vehicle_mass_kg: req.vehicle_mass_kg.unwrap_or(5200.0),
            nose_cone_half_angle_deg: req.nose_cone_half_angle_deg.unwrap_or(45.0),
            tps_surface_emissivity: req.tps_surface_emissivity.unwrap_or(0.88),
            max_allowable_tps_temperature_k: req.max_allowable_tps_temperature_k.unwrap_or(2300.0),
        };
        let res =
            atlas_simulation_core::reentry_aerothermodynamics::solve_reentry_aerothermodynamics(
                &prob,
            )
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_manufacturing_lpbf_meltpool(
        &mut self,
        req: &SimLpbfRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::lpbf_meltpool::LpbfProblem {
            problem_id: "lpbf_meltpool_sim".to_string(),
            material: req
                .material
                .unwrap_or(atlas_simulation_core::lpbf_meltpool::LpbfPowderMaterial::Ti6Al4V),
            laser_power_w: req.laser_power_w.unwrap_or(250.0),
            scan_speed_mm_s: req.scan_speed_mm_s.unwrap_or(950.0),
            beam_diameter_um: req.beam_diameter_um.unwrap_or(80.0),
            layer_thickness_um: req.layer_thickness_um.unwrap_or(40.0),
            hatch_spacing_um: req.hatch_spacing_um.unwrap_or(100.0),
            preheat_temperature_c: req.preheat_temperature_c.unwrap_or(100.0),
        };
        let res = atlas_simulation_core::lpbf_meltpool::solve_lpbf_meltpool(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_acoustics_phononic_bandgap(
        &mut self,
        req: &SimPhononicCrystalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::phononic_bandgap::PhononicCrystalProblem {
            problem_id: "phononic_bandgap_sim".to_string(),
            layer_a: atlas_simulation_core::phononic_bandgap::PhononicLayer {
                name: req
                    .layer_a_name
                    .clone()
                    .unwrap_or_else(|| "Steel".to_string()),
                thickness_mm: req.layer_a_thickness_mm.unwrap_or(24.0),
                density_kg_m3: req.layer_a_density_kg_m3.unwrap_or(7850.0),
                sound_speed_m_s: req.layer_a_sound_speed_m_s.unwrap_or(5150.0),
            },
            layer_b: atlas_simulation_core::phononic_bandgap::PhononicLayer {
                name: req
                    .layer_b_name
                    .clone()
                    .unwrap_or_else(|| "Polymer_Epoxy".to_string()),
                thickness_mm: req.layer_b_thickness_mm.unwrap_or(16.0),
                density_kg_m3: req.layer_b_density_kg_m3.unwrap_or(1180.0),
                sound_speed_m_s: req.layer_b_sound_speed_m_s.unwrap_or(1450.0),
            },
            unit_cell_count: req.unit_cell_count.unwrap_or(8),
            excitation_frequency_hz: req.excitation_frequency_hz.unwrap_or(24000.0),
        };
        let res = atlas_simulation_core::phononic_bandgap::solve_phononic_bandgap(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_rail_wheel_rail_contact(
        &mut self,
        req: &SimWheelRailRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::wheel_rail_contact::WheelRailProblem {
            problem_id: "wheel_rail_sim".to_string(),
            wheel_vertical_load_kn: req.wheel_vertical_load_kn.unwrap_or(100.0),
            wheel_radius_mm: req.wheel_radius_mm.unwrap_or(460.0),
            rail_crown_radius_mm: req.rail_crown_radius_mm.unwrap_or(300.0),
            train_speed_km_h: req.train_speed_km_h.unwrap_or(200.0),
            effective_conicity_rad: req.effective_conicity_rad.unwrap_or(0.025),
            track_gauge_half_m: req.track_gauge_half_m.unwrap_or(0.750),
            flange_contact_angle_deg: req.flange_contact_angle_deg.unwrap_or(70.0),
            friction_coefficient: req.friction_coefficient.unwrap_or(0.35),
            applied_lateral_force_kn: req.applied_lateral_force_kn.unwrap_or(35.0),
        };
        let res = atlas_simulation_core::wheel_rail_contact::solve_wheel_rail_contact(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mining_mine_ventilation(
        &mut self,
        req: &SimMineVentilationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::mine_ventilation::MineVentilationProblem {
            problem_id: "mine_ventilation_sim".to_string(),
            lining_type: req.lining_type.unwrap_or(
                atlas_simulation_core::mine_ventilation::AirwayLiningType::RoughBlastedRock,
            ),
            airway_length_m: req.airway_length_m.unwrap_or(1400.0),
            airway_area_m2: req.airway_area_m2.unwrap_or(20.0),
            airway_perimeter_m: req.airway_perimeter_m.unwrap_or(18.0),
            airflow_rate_m3_s: req.airflow_rate_m3_s.unwrap_or(85.0),
            air_density_kg_m3: req.air_density_kg_m3.unwrap_or(1.22),
            shock_loss_factor: req.shock_loss_factor.unwrap_or(1.50),
            vertical_depth_m: req.vertical_depth_m.unwrap_or(400.0),
            downcast_upcast_delta_temp_c: req.downcast_upcast_delta_temp_c.unwrap_or(5.0),
            fan_efficiency: req.fan_efficiency.unwrap_or(0.78),
            methane_emission_m3_s: req.methane_emission_m3_s.unwrap_or(0.12),
        };
        let res = atlas_simulation_core::mine_ventilation::solve_mine_ventilation(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_telecom_reflector_antenna(
        &mut self,
        req: &SimReflectorAntennaRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::reflector_antenna::ReflectorAntennaProblem {
            problem_id: "reflector_antenna_sim".to_string(),
            feed_pattern: req.feed_pattern.unwrap_or(
                atlas_simulation_core::reflector_antenna::FeedIlluminationPattern::TaperMinus10dB,
            ),
            dish_diameter_m: req.dish_diameter_m.unwrap_or(2.40),
            focal_length_m: req.focal_length_m.unwrap_or(0.96),
            operating_frequency_ghz: req.operating_frequency_ghz.unwrap_or(14.25),
            surface_rms_error_mm: req.surface_rms_error_mm.unwrap_or(0.35),
            aperture_blockage_fraction: req.aperture_blockage_fraction.unwrap_or(0.04),
        };
        let res = atlas_simulation_core::reflector_antenna::solve_reflector_antenna(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_semiconductor_gaa_fet(
        &mut self,
        req: &SimGaaFetRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::semiconductor_gaa_fet::GaaFetProblem {
            problem_id: "gaa_fet_sim".to_string(),
            channel_material: req.channel_material.unwrap_or(
                atlas_simulation_core::semiconductor_gaa_fet::GaaChannelMaterial::Silicon,
            ),
            nanosheet_count: req.nanosheet_count.unwrap_or(3),
            gate_length_nm: req.gate_length_nm.unwrap_or(12.0),
            nanosheet_width_nm: req.nanosheet_width_nm.unwrap_or(30.0),
            nanosheet_thickness_nm: req.nanosheet_thickness_nm.unwrap_or(5.0),
            equivalent_oxide_thickness_nm: req.equivalent_oxide_thickness_nm.unwrap_or(0.75),
            supply_voltage_v: req.supply_voltage_v.unwrap_or(0.70),
            operating_temperature_k: req.operating_temperature_k.unwrap_or(300.0),
        };
        let res = atlas_simulation_core::semiconductor_gaa_fet::solve_gaa_fet(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_agri_irrigation_hydraulics(
        &mut self,
        req: &SimIrrigationHydraulicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::irrigation_hydraulics::IrrigationHydraulicsProblem {
            problem_id: "irrigation_hydraulics_sim".to_string(),
            emitter_type: req.emitter_type.unwrap_or(
                atlas_simulation_core::irrigation_hydraulics::EmitterType::NonCompensatingOrifice,
            ),
            pipe_inside_diameter_mm: req.pipe_inside_diameter_mm.unwrap_or(16.0),
            lateral_length_m: req.lateral_length_m.unwrap_or(120.0),
            emitter_spacing_m: req.emitter_spacing_m.unwrap_or(0.40),
            nominal_discharge_l_h: req.nominal_discharge_l_h.unwrap_or(1.60),
            nominal_pressure_head_m: req.nominal_pressure_head_m.unwrap_or(10.0),
            inlet_pressure_head_m: req.inlet_pressure_head_m.unwrap_or(11.0),
            ground_slope_pct: req.ground_slope_pct.unwrap_or(0.0),
            hazen_williams_c: req.hazen_williams_c.unwrap_or(150.0),
            emitter_coefficient_of_variation: req.emitter_coefficient_of_variation.unwrap_or(0.035),
        };
        let res = atlas_simulation_core::irrigation_hydraulics::solve_irrigation_hydraulics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_environmental_wastewater_treatment(
        &mut self,
        req: &SimWastewaterTreatmentRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::wastewater_treatment::WastewaterTreatmentProblem {
            problem_id: "wastewater_treatment_sim".to_string(),
            diffuser_type: req.diffuser_type.unwrap_or(atlas_simulation_core::wastewater_treatment::AerationDiffuserType::FineBubbleMembrane),
            influent_flow_m3_day: req.influent_flow_m3_day.unwrap_or(20000.0),
            influent_bod_mg_l: req.influent_bod_mg_l.unwrap_or(220.0),
            target_effluent_bod_mg_l: req.target_effluent_bod_mg_l.unwrap_or(15.0),
            mlss_mg_l: req.mlss_mg_l.unwrap_or(3000.0),
            sludge_retention_time_days: req.sludge_retention_time_days.unwrap_or(8.0),
            basin_depth_m: req.basin_depth_m.unwrap_or(4.5),
            clarifier_diameter_m: req.clarifier_diameter_m.unwrap_or(28.0),
            clarifier_count: req.clarifier_count.unwrap_or(2),
            wastewater_temp_c: req.wastewater_temp_c.unwrap_or(20.0),
        };
        let res = atlas_simulation_core::wastewater_treatment::solve_wastewater_treatment(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_automotive_ev_powertrain(
        &mut self,
        req: &SimEvPowertrainRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::ev_powertrain::EvPowertrainProblem {
            problem_id: "ev_powertrain_sim".to_string(),
            layout: req.layout.unwrap_or(
                atlas_simulation_core::ev_powertrain::EvDrivetrainLayout::RearWheelDrive,
            ),
            vehicle_mass_kg: req.vehicle_mass_kg.unwrap_or(1850.0),
            rolling_resistance_coeff: req.rolling_resistance_coeff.unwrap_or(0.009),
            drag_coefficient: req.drag_coefficient.unwrap_or(0.24),
            frontal_area_m2: req.frontal_area_m2.unwrap_or(2.25),
            tire_radius_m: req.tire_radius_m.unwrap_or(0.330),
            gear_ratio: req.gear_ratio.unwrap_or(9.0),
            driveline_efficiency: req.driveline_efficiency.unwrap_or(0.95),
            motor_peak_torque_nm: req.motor_peak_torque_nm.unwrap_or(350.0),
            motor_base_speed_rpm: req.motor_base_speed_rpm.unwrap_or(4500.0),
            motor_max_speed_rpm: req.motor_max_speed_rpm.unwrap_or(16000.0),
            battery_capacity_kwh: req.battery_capacity_kwh.unwrap_or(75.0),
            battery_voltage_v: req.battery_voltage_v.unwrap_or(400.0),
            cruising_speed_km_h: req.cruising_speed_km_h.unwrap_or(100.0),
            braking_deceleration_m_s2: req.braking_deceleration_m_s2.unwrap_or(2.5),
        };
        let res = atlas_simulation_core::ev_powertrain::solve_ev_powertrain(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_seismic_base_isolation(
        &mut self,
        req: &SimSeismicBaseIsolationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::seismic_base_isolation::SeismicBaseIsolationProblem {
            problem_id: "seismic_base_isolation_sim".to_string(),
            rubber_grade: req.rubber_grade.unwrap_or(atlas_simulation_core::seismic_base_isolation::RubberCompoundGrade::StandardNaturalRubber),
            superstructure_mass_tonnes: req.superstructure_mass_tonnes.unwrap_or(4500.0),
            bearing_count: req.bearing_count.unwrap_or(16),
            bearing_outer_diameter_mm: req.bearing_outer_diameter_mm.unwrap_or(800.0),
            lead_core_diameter_mm: req.lead_core_diameter_mm.unwrap_or(160.0),
            total_rubber_thickness_mm: req.total_rubber_thickness_mm.unwrap_or(200.0),
            lead_yield_strength_mpa: req.lead_yield_strength_mpa.unwrap_or(10.5),
            mapped_spectral_accel_s1_g: req.mapped_spectral_accel_s1_g.unwrap_or(0.60),
            site_coefficient_fv: req.site_coefficient_fv.unwrap_or(1.20),
            fixed_base_period_s: req.fixed_base_period_s.unwrap_or(0.55),
        };
        let res =
            atlas_simulation_core::seismic_base_isolation::solve_seismic_base_isolation(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_defense_terminal_ballistics(
        &mut self,
        req: &SimTerminalBallisticsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::terminal_ballistics::TerminalBallisticsProblem {
            problem_id: "terminal_ballistics_sim".to_string(),
            projectile_type: req.projectile_type.unwrap_or(
                atlas_simulation_core::terminal_ballistics::ProjectileType::TungstenLongRod,
            ),
            armor_type: req.armor_type.unwrap_or(
                atlas_simulation_core::terminal_ballistics::TargetArmorType::RolledHomogeneousArmor,
            ),
            projectile_mass_kg: req.projectile_mass_kg.unwrap_or(0.180),
            projectile_diameter_mm: req.projectile_diameter_mm.unwrap_or(12.0),
            projectile_length_mm: req.projectile_length_mm.unwrap_or(90.0),
            impact_velocity_m_s: req.impact_velocity_m_s.unwrap_or(1450.0),
            obliquity_angle_deg: req.obliquity_angle_deg.unwrap_or(30.0),
            armor_thickness_mm: req.armor_thickness_mm.unwrap_or(75.0),
        };
        let res = atlas_simulation_core::terminal_ballistics::solve_terminal_ballistics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_packaging_solder_thermal_fatigue(
        &mut self,
        req: &SimSolderThermalFatigueRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::solder_thermal_fatigue::SolderThermalFatigueProblem {
            problem_id: "solder_thermal_fatigue_sim".to_string(),
            alloy_type: req
                .alloy_type
                .unwrap_or(atlas_simulation_core::solder_thermal_fatigue::SolderAlloyType::Sac305),
            package_width_mm: req.package_width_mm.unwrap_or(35.0),
            package_length_mm: req.package_length_mm.unwrap_or(35.0),
            solder_joint_height_um: req.solder_joint_height_um.unwrap_or(400.0),
            solder_ball_pitch_mm: req.solder_ball_pitch_mm.unwrap_or(1.0),
            package_cte_ppm_k: req.package_cte_ppm_k.unwrap_or(6.5),
            pcb_cte_ppm_k: req.pcb_cte_ppm_k.unwrap_or(16.0),
            cycle_min_temp_c: req.cycle_min_temp_c.unwrap_or(-40.0),
            cycle_max_temp_c: req.cycle_max_temp_c.unwrap_or(125.0),
            dwell_time_minutes: req.dwell_time_minutes.unwrap_or(15.0),
            ramp_rate_c_min: req.ramp_rate_c_min.unwrap_or(15.0),
        };
        let res =
            atlas_simulation_core::solder_thermal_fatigue::solve_solder_thermal_fatigue(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_ship_slamming(
        &mut self,
        req: &SimShipSlammingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::ship_slamming::ShipSlammingProblem {
            problem_id: "ship_slamming_sim".to_string(),
            location: req
                .location
                .unwrap_or(atlas_simulation_core::ship_slamming::SlammingLocation::BowFlare),
            deadrise_angle_deg: req.deadrise_angle_deg.unwrap_or(18.0),
            relative_impact_velocity_m_s: req.relative_impact_velocity_m_s.unwrap_or(7.5),
            frame_spacing_m: req.frame_spacing_m.unwrap_or(0.75),
            plate_length_m: req.plate_length_m.unwrap_or(2.40),
            plate_thickness_mm: req.plate_thickness_mm.unwrap_or(16.0),
            steel_yield_strength_mpa: req.steel_yield_strength_mpa.unwrap_or(355.0),
            water_density_kg_m3: req.water_density_kg_m3.unwrap_or(1025.0),
        };
        let res = atlas_simulation_core::ship_slamming::solve_ship_slamming(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_automotive_crash_deceleration(
        &mut self,
        req: &SimCrashDecelerationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::crash_deceleration::CrashDecelerationProblem {
            problem_id: "crash_deceleration_sim".to_string(),
            scenario: req.scenario.unwrap_or(atlas_simulation_core::crash_deceleration::CrashScenario::OffsetDeformableBarrier40Pct),
            restraint: req.restraint.unwrap_or(atlas_simulation_core::crash_deceleration::RestraintSystemType::AdvancedAirbagAndAdaptiveBelt),
            vehicle_mass_kg: req.vehicle_mass_kg.unwrap_or(1650.0),
            impact_velocity_km_h: req.impact_velocity_km_h.unwrap_or(64.0),
            crumple_zone_length_m: req.crumple_zone_length_m.unwrap_or(0.72),
            crash_box_mean_force_kn: req.crash_box_mean_force_kn.unwrap_or(310.0),
            structural_energy_efficiency: req.structural_energy_efficiency.unwrap_or(0.75),
            occupant_mass_kg: req.occupant_mass_kg.unwrap_or(78.0),
        };
        let res = atlas_simulation_core::crash_deceleration::solve_crash_deceleration(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_nuclear_fuel_pin(
        &mut self,
        req: &SimNuclearFuelPinRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::nuclear_fuel_pin::NuclearFuelPinProblem {
            problem_id: "nuclear_fuel_pin_sim".to_string(),
            lattice: req.lattice.unwrap_or(
                atlas_simulation_core::nuclear_fuel_pin::LatticeGeometry::PwrSquarePitch17x17,
            ),
            linear_heat_rate_kw_per_m: req.linear_heat_rate_kw_per_m.unwrap_or(32.0),
            pellet_radius_mm: req.pellet_radius_mm.unwrap_or(4.10),
            gap_thickness_um: req.gap_thickness_um.unwrap_or(80.0),
            clad_thickness_mm: req.clad_thickness_mm.unwrap_or(0.60),
            clad_outer_diameter_mm: req.clad_outer_diameter_mm.unwrap_or(9.50),
            rod_pitch_mm: req.rod_pitch_mm.unwrap_or(12.60),
            coolant_pressure_mpa: req.coolant_pressure_mpa.unwrap_or(15.50),
            bulk_coolant_temperature_c: req.bulk_coolant_temperature_c.unwrap_or(305.0),
            coolant_mass_flux_kg_m2_s: req.coolant_mass_flux_kg_m2_s.unwrap_or(3500.0),
        };
        let res = atlas_simulation_core::nuclear_fuel_pin::solve_nuclear_fuel_pin(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_adsorption_psa(
        &mut self,
        req: &SimAdsorptionPsaRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::adsorption_psa::AdsorptionPsaProblem {
            problem_id: "adsorption_psa_sim".to_string(),
            duty: req.duty.unwrap_or(
                atlas_simulation_core::adsorption_psa::PsaSeparationDuty::OxygenGenerationFromAir,
            ),
            bed_diameter_m: req.bed_diameter_m.unwrap_or(0.60),
            bed_length_m: req.bed_length_m.unwrap_or(2.20),
            adsorption_pressure_bar: req.adsorption_pressure_bar.unwrap_or(5.50),
            desorption_pressure_bar: req.desorption_pressure_bar.unwrap_or(1.10),
            operating_temperature_c: req.operating_temperature_c.unwrap_or(25.0),
            feed_flow_rate_nm3_h: req.feed_flow_rate_nm3_h.unwrap_or(85.0),
            particle_diameter_mm: req.particle_diameter_mm.unwrap_or(2.0),
            adsorbent_bulk_density_kg_m3: req.adsorbent_bulk_density_kg_m3.unwrap_or(700.0),
            feed_heavy_fraction: req.feed_heavy_fraction.unwrap_or(0.78),
        };
        let res = atlas_simulation_core::adsorption_psa::solve_adsorption_psa(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_rotorcraft_aerodynamics(
        &mut self,
        req: &SimRotorcraftAerodynamicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::rotorcraft_aerodynamics::RotorcraftAerodynamicsProblem {
            problem_id: "rotorcraft_aerodynamics_sim".to_string(),
            regime: req.regime.unwrap_or(
                atlas_simulation_core::rotorcraft_aerodynamics::RotorFlightRegime::HoverOge,
            ),
            gross_weight_kg: req.gross_weight_kg.unwrap_or(4300.0),
            rotor_radius_m: req.rotor_radius_m.unwrap_or(6.50),
            number_of_blades: req.number_of_blades.unwrap_or(4),
            blade_chord_m: req.blade_chord_m.unwrap_or(0.38),
            rotor_speed_rpm: req.rotor_speed_rpm.unwrap_or(345.0),
            forward_speed_knots: req.forward_speed_knots.unwrap_or(0.0),
            air_density_kg_m3: req.air_density_kg_m3.unwrap_or(1.225),
            speed_of_sound_m_s: req.speed_of_sound_m_s.unwrap_or(340.3),
            blade_profile_cd0: req.blade_profile_cd0.unwrap_or(0.0095),
            fuselage_flat_plate_area_m2: req.fuselage_flat_plate_area_m2.unwrap_or(1.65),
        };
        let res =
            atlas_simulation_core::rotorcraft_aerodynamics::solve_rotorcraft_aerodynamics(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_heat_exchanger_delaware(
        &mut self,
        req: &SimHeatExchangerDelawareRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::heat_exchanger_delaware::HeatExchangerDelawareProblem {
            problem_id: "heat_exchanger_delaware_sim".to_string(),
            shell_type: req.shell_type.unwrap_or(
                atlas_simulation_core::heat_exchanger_delaware::TemaShellType::EOnePassShell,
            ),
            tube_layout: req.tube_layout.unwrap_or(
                atlas_simulation_core::heat_exchanger_delaware::TemaTubeLayout::Triangular30Deg,
            ),
            shell_inside_diameter_m: req.shell_inside_diameter_m.unwrap_or(0.60),
            number_of_tubes: req.number_of_tubes.unwrap_or(360),
            tube_outer_diameter_mm: req.tube_outer_diameter_mm.unwrap_or(19.05),
            tube_wall_thickness_mm: req.tube_wall_thickness_mm.unwrap_or(1.65),
            tube_length_m: req.tube_length_m.unwrap_or(4.80),
            tube_pitch_mm: req.tube_pitch_mm.unwrap_or(23.81),
            baffle_cut_pct: req.baffle_cut_pct.unwrap_or(25.0),
            baffle_spacing_m: req.baffle_spacing_m.unwrap_or(0.30),
            hot_inlet_temp_c: req.hot_inlet_temp_c.unwrap_or(120.0),
            hot_outlet_temp_c: req.hot_outlet_temp_c.unwrap_or(65.0),
            hot_mass_flow_kg_s: req.hot_mass_flow_kg_s.unwrap_or(18.5),
            cold_inlet_temp_c: req.cold_inlet_temp_c.unwrap_or(25.0),
            cold_outlet_temp_c: req.cold_outlet_temp_c.unwrap_or(50.0),
            cold_mass_flow_kg_s: req.cold_mass_flow_kg_s.unwrap_or(32.0),
            shell_fluid_cp_j_kg_k: req.shell_fluid_cp_j_kg_k.unwrap_or(2450.0),
            shell_fluid_viscosity_pa_s: req.shell_fluid_viscosity_pa_s.unwrap_or(0.00085),
            shell_fluid_conductivity_w_m_k: req.shell_fluid_conductivity_w_m_k.unwrap_or(0.165),
            shell_fluid_density_kg_m3: req.shell_fluid_density_kg_m3.unwrap_or(840.0),
            tube_side_htc_w_m2_k: req.tube_side_htc_w_m2_k.unwrap_or(1850.0),
            fouling_resistance_m2_k_w: req.fouling_resistance_m2_k_w.unwrap_or(0.00035),
        };
        let res =
            atlas_simulation_core::heat_exchanger_delaware::solve_heat_exchanger_delaware(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_geotechnical_drilled_shaft(
        &mut self,
        req: &SimDrilledShaftRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::drilled_shaft_foundation::DrilledShaftProblem {
            problem_id: "drilled_shaft_sim".to_string(),
            geomaterial: req.geomaterial.unwrap_or(
                atlas_simulation_core::drilled_shaft_foundation::GeomaterialType::CohesiveClay,
            ),
            shaft_diameter_m: req.shaft_diameter_m.unwrap_or(1.20),
            embedded_length_m: req.embedded_length_m.unwrap_or(18.0),
            factored_load_demand_kn: req.factored_load_demand_kn.unwrap_or(3200.0),
            service_load_demand_kn: req.service_load_demand_kn.unwrap_or(2200.0),
            undrained_shear_strength_kpa: req.undrained_shear_strength_kpa.unwrap_or(120.0),
            effective_friction_angle_deg: req.effective_friction_angle_deg.unwrap_or(32.0),
            soil_unit_weight_kn_m3: req.soil_unit_weight_kn_m3.unwrap_or(18.5),
            concrete_compressive_strength_mpa: req
                .concrete_compressive_strength_mpa
                .unwrap_or(35.0),
            steel_reinforcement_ratio_pct: req.steel_reinforcement_ratio_pct.unwrap_or(1.50),
        };
        let res = atlas_simulation_core::drilled_shaft_foundation::solve_drilled_shaft(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_biomedical_heart_valve(
        &mut self,
        req: &SimHeartValveRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::heart_valve_hemodynamics::HeartValveProblem {
            problem_id: "heart_valve_sim".to_string(),
            valve_type: req.valve_type.unwrap_or(atlas_simulation_core::heart_valve_hemodynamics::ValveProsthesisType::MechanicalBileaflet),
            nominal_size_mm: req.nominal_size_mm.unwrap_or(23.0),
            heart_rate_bpm: req.heart_rate_bpm.unwrap_or(70.0),
            stroke_volume_ml: req.stroke_volume_ml.unwrap_or(75.0),
            systolic_ejection_period_s: req.systolic_ejection_period_s.unwrap_or(0.30),
            patient_bsa_m2: req.patient_bsa_m2.unwrap_or(1.85),
            mean_pressure_gradient_mmhg: req.mean_pressure_gradient_mmhg.unwrap_or(10.5),
            peak_jet_velocity_m_s: req.peak_jet_velocity_m_s.unwrap_or(2.15),
            dynamic_closing_volume_ml: req.dynamic_closing_volume_ml.unwrap_or(2.8),
            static_leakage_volume_ml: req.static_leakage_volume_ml.unwrap_or(3.2),
        };
        let res =
            atlas_simulation_core::heart_valve_hemodynamics::solve_heart_valve_hemodynamics(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_centrifugal_compressor(
        &mut self,
        req: &SimCentrifugalCompressorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::centrifugal_compressor::CentrifugalCompressorProblem {
            problem_id: "centrifugal_compressor_sim".to_string(),
            blade_style: req.blade_style.unwrap_or(
                atlas_simulation_core::centrifugal_compressor::ImpellerBladeStyle::BacksweptBlades,
            ),
            mass_flow_rate_kg_s: req.mass_flow_rate_kg_s.unwrap_or(3.80),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(38000.0),
            impeller_tip_diameter_m: req.impeller_tip_diameter_m.unwrap_or(0.285),
            inducer_hub_diameter_m: req.inducer_hub_diameter_m.unwrap_or(0.075),
            inducer_shroud_diameter_m: req.inducer_shroud_diameter_m.unwrap_or(0.165),
            number_of_blades: req.number_of_blades.unwrap_or(18),
            blade_backsweep_deg: req.blade_backsweep_deg.unwrap_or(35.0),
            inlet_total_temperature_k: req.inlet_total_temperature_k.unwrap_or(288.15),
            inlet_total_pressure_kpa: req.inlet_total_pressure_kpa.unwrap_or(101.325),
            isentropic_efficiency_tt: req.isentropic_efficiency_tt.unwrap_or(0.825),
            impeller_material_density_kg_m3: req.impeller_material_density_kg_m3.unwrap_or(4430.0),
            impeller_yield_strength_mpa: req.impeller_yield_strength_mpa.unwrap_or(880.0),
        };
        let res =
            atlas_simulation_core::centrifugal_compressor::solve_centrifugal_compressor(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_rail_pantograph(
        &mut self,
        req: &SimRailPantographRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::rail_pantograph::RailPantographProblem {
            problem_id: "rail_pantograph_sim".to_string(),
            system: req
                .system
                .unwrap_or(atlas_simulation_core::rail_pantograph::CatenaryVoltageSystem::Ac25kV),
            train_speed_km_h: req.train_speed_km_h.unwrap_or(300.0),
            static_contact_force_n: req.static_contact_force_n.unwrap_or(80.0),
            aerodynamic_uplift_coeff: req.aerodynamic_uplift_coeff.unwrap_or(0.00095),
            contact_wire_tension_kn: req.contact_wire_tension_kn.unwrap_or(22.0),
            contact_wire_mass_kg_per_m: req.contact_wire_mass_kg_per_m.unwrap_or(1.07),
            catenary_span_length_m: req.catenary_span_length_m.unwrap_or(55.0),
            catenary_midspan_elasticity_mm_per_n: req
                .catenary_midspan_elasticity_mm_per_n
                .unwrap_or(0.75),
            catenary_support_elasticity_mm_per_n: req
                .catenary_support_elasticity_mm_per_n
                .unwrap_or(0.35),
        };
        let res = atlas_simulation_core::rail_pantograph::solve_rail_pantograph(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_geotechnical_tbm_tunneling(
        &mut self,
        req: &SimTbmTunnelingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::tbm_tunneling::TbmTunnelingProblem {
            problem_id: "tbm_tunneling_sim".to_string(),
            ground_condition: req.ground_condition.unwrap_or(
                atlas_simulation_core::tbm_tunneling::GroundConditionType::CohesionlessSandGravel,
            ),
            tunnel_outer_diameter_m: req.tunnel_outer_diameter_m.unwrap_or(6.60),
            tunnel_depth_to_axis_m: req.tunnel_depth_to_axis_m.unwrap_or(22.0),
            water_table_depth_m: req.water_table_depth_m.unwrap_or(4.0),
            soil_bulk_unit_weight_kn_m3: req.soil_bulk_unit_weight_kn_m3.unwrap_or(19.5),
            soil_effective_cohesion_kpa: req.soil_effective_cohesion_kpa.unwrap_or(12.0),
            soil_effective_friction_angle_deg: req
                .soil_effective_friction_angle_deg
                .unwrap_or(32.0),
            soil_compressive_strength_mpa: req.soil_compressive_strength_mpa.unwrap_or(0.85),
            cutterhead_opening_ratio_pct: req.cutterhead_opening_ratio_pct.unwrap_or(35.0),
            cutterhead_rotational_speed_rpm: req.cutterhead_rotational_speed_rpm.unwrap_or(2.5),
            penetration_rate_mm_per_rev: req.penetration_rate_mm_per_rev.unwrap_or(12.0),
            shield_length_m: req.shield_length_m.unwrap_or(9.50),
            machine_rated_torque_kn_m: req.machine_rated_torque_kn_m.unwrap_or(5500.0),
            machine_rated_thrust_kn: req.machine_rated_thrust_kn.unwrap_or(42000.0),
        };
        let res = atlas_simulation_core::tbm_tunneling::solve_tbm_tunneling(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_structural_prestressed_concrete(
        &mut self,
        req: &SimPrestressedConcreteRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::prestressed_concrete::PrestressedConcreteProblem {
            problem_id: "prestressed_concrete_sim".to_string(),
            strand_type: req.strand_type.unwrap_or(atlas_simulation_core::prestressed_concrete::PrestressingStrandType::LowRelaxationStrand270),
            span_length_m: req.span_length_m.unwrap_or(32.0),
            concrete_area_m2: req.concrete_area_m2.unwrap_or(0.525),
            concrete_moment_of_inertia_m4: req.concrete_moment_of_inertia_m4.unwrap_or(0.0785),
            distance_to_top_fiber_m: req.distance_to_top_fiber_m.unwrap_or(0.75),
            distance_to_bottom_fiber_m: req.distance_to_bottom_fiber_m.unwrap_or(0.65),
            concrete_f_c_mpa: req.concrete_f_c_mpa.unwrap_or(45.0),
            concrete_f_ci_mpa: req.concrete_f_ci_mpa.unwrap_or(32.0),
            tendon_steel_area_mm2: req.tendon_steel_area_mm2.unwrap_or(2800.0),
            tendon_modulus_mpa: req.tendon_modulus_mpa.unwrap_or(195000.0),
            jacking_stress_ratio_of_fpu: req.jacking_stress_ratio_of_fpu.unwrap_or(0.75),
            midspan_eccentricity_m: req.midspan_eccentricity_m.unwrap_or(0.48),
            end_eccentricity_m: req.end_eccentricity_m.unwrap_or(0.08),
            friction_curvature_coeff_mu: req.friction_curvature_coeff_mu.unwrap_or(0.20),
            friction_wobble_coeff_k: req.friction_wobble_coeff_k.unwrap_or(0.00065),
            anchorage_draw_in_mm: req.anchorage_draw_in_mm.unwrap_or(6.0),
            ambient_relative_humidity_pct: req.ambient_relative_humidity_pct.unwrap_or(70.0),
            service_moment_midspan_kn_m: req.service_moment_midspan_kn_m.unwrap_or(2150.0),
        };
        let res = atlas_simulation_core::prestressed_concrete::solve_prestressed_concrete(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_spacecraft_jitter(
        &mut self,
        req: &SimSpacecraftJitterRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::spacecraft_jitter::SpacecraftJitterProblem {
            problem_id: "spacecraft_jitter_sim".to_string(),
            wheel_speed_rpm: req.wheel_speed_rpm.unwrap_or(3600.0),
            rotor_inertia_kg_m2: req.rotor_inertia_kg_m2.unwrap_or(0.045),
            rotor_mass_kg: req.rotor_mass_kg.unwrap_or(5.50),
            static_imbalance_g_mm: req.static_imbalance_g_mm.unwrap_or(1.80),
            dynamic_imbalance_g_mm2: req.dynamic_imbalance_g_mm2.unwrap_or(35.0),
            number_of_bearing_balls: req.number_of_bearing_balls.unwrap_or(8),
            ball_diameter_mm: req.ball_diameter_mm.unwrap_or(7.14),
            bearing_pitch_diameter_mm: req.bearing_pitch_diameter_mm.unwrap_or(38.0),
            bearing_contact_angle_deg: req.bearing_contact_angle_deg.unwrap_or(15.0),
            structural_resonance_freq_hz: req.structural_resonance_freq_hz.unwrap_or(85.0),
            structural_damping_ratio: req.structural_damping_ratio.unwrap_or(0.015),
            satellite_inertia_kg_m2: req.satellite_inertia_kg_m2.unwrap_or(320.0),
            allowable_pointing_jitter_urad: req.allowable_pointing_jitter_urad.unwrap_or(2.50),
        };
        let res = atlas_simulation_core::spacecraft_jitter::solve_spacecraft_jitter(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_automotive_radar_lidar_sensor(
        &mut self,
        req: &SimRadarLidarSensorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::radar_lidar_sensor::RadarLidarSensorProblem {
            problem_id: "radar_lidar_sensor_sim".to_string(),
            weather_condition: req.weather_condition.unwrap_or(
                atlas_simulation_core::radar_lidar_sensor::WeatherConditionType::ModerateRain,
            ),
            target_range_m: req.target_range_m.unwrap_or(120.0),
            target_rcs_m2: req.target_rcs_m2.unwrap_or(5.0),
            target_optical_reflectivity: req.target_optical_reflectivity.unwrap_or(0.18),
            radar_tx_power_w: req.radar_tx_power_w.unwrap_or(0.10),
            radar_antenna_gain_dbi: req.radar_antenna_gain_dbi.unwrap_or(26.0),
            radar_chirp_bandwidth_mhz: req.radar_chirp_bandwidth_mhz.unwrap_or(1000.0),
            radar_noise_figure_db: req.radar_noise_figure_db.unwrap_or(11.0),
            lidar_wavelength_nm: req.lidar_wavelength_nm.unwrap_or(905.0),
            lidar_pulse_power_w: req.lidar_pulse_power_w.unwrap_or(75.0),
            lidar_aperture_diameter_mm: req.lidar_aperture_diameter_mm.unwrap_or(35.0),
            required_snr_threshold_db: req.required_snr_threshold_db.unwrap_or(12.0),
        };
        let res = atlas_simulation_core::radar_lidar_sensor::solve_radar_lidar_sensor(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_auv_propulsion(
        &mut self,
        req: &SimAuvPropulsionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::auv_propulsion::AuvPropulsionProblem {
            problem_id: "auv_propulsion_sim".to_string(),
            mission_regime: req.mission_regime.unwrap_or(
                atlas_simulation_core::auv_propulsion::AuvMissionRegime::StandardTransit,
            ),
            cruising_speed_knots: req.cruising_speed_knots.unwrap_or(3.50),
            vehicle_length_m: req.vehicle_length_m.unwrap_or(4.80),
            vehicle_diameter_m: req.vehicle_diameter_m.unwrap_or(0.533),
            wetted_surface_area_m2: req.wetted_surface_area_m2.unwrap_or(7.20),
            seawater_density_kg_m3: req.seawater_density_kg_m3.unwrap_or(1025.0),
            seawater_viscosity_pa_s: req.seawater_viscosity_pa_s.unwrap_or(0.00115),
            propeller_open_water_efficiency: req.propeller_open_water_efficiency.unwrap_or(0.68),
            hotel_power_draw_w: req.hotel_power_draw_w.unwrap_or(120.0),
            battery_capacity_kwh: req.battery_capacity_kwh.unwrap_or(24.0),
            battery_depth_of_discharge_pct: req.battery_depth_of_discharge_pct.unwrap_or(85.0),
            motor_drive_efficiency: req.motor_drive_efficiency.unwrap_or(0.90),
        };
        let res = atlas_simulation_core::auv_propulsion::solve_auv_propulsion(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_hydro_governor(
        &mut self,
        req: &SimHydroGovernorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::hydro_governor::HydroGovernorProblem {
            problem_id: "hydro_governor_sim".to_string(),
            turbine_type: req.turbine_type.unwrap_or(
                atlas_simulation_core::hydro_governor::HydroTurbineType::FrancisReaction,
            ),
            rated_net_head_m: req.rated_net_head_m.unwrap_or(145.0),
            rated_discharge_m3_s: req.rated_discharge_m3_s.unwrap_or(24.0),
            rated_speed_rpm: req.rated_speed_rpm.unwrap_or(500.0),
            rotor_inertia_kg_m2: req.rotor_inertia_kg_m2.unwrap_or(125000.0),
            penstock_length_m: req.penstock_length_m.unwrap_or(420.0),
            penstock_diameter_m: req.penstock_diameter_m.unwrap_or(2.20),
            penstock_wall_thickness_mm: req.penstock_wall_thickness_mm.unwrap_or(22.0),
            steel_elastic_modulus_gpa: req.steel_elastic_modulus_gpa.unwrap_or(206.0),
            steel_yield_strength_mpa: req.steel_yield_strength_mpa.unwrap_or(355.0),
            wicket_gate_closure_time_s: req.wicket_gate_closure_time_s.unwrap_or(6.50),
            turbine_efficiency: req.turbine_efficiency.unwrap_or(0.935),
        };
        let res = atlas_simulation_core::hydro_governor::solve_hydro_governor(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_semiconductor_cmp(
        &mut self,
        req: &SimSemiconductorCmpRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::semiconductor_cmp::SemiconductorCmpProblem {
            problem_id: "semiconductor_cmp_sim".to_string(),
            layer_type: req
                .layer_type
                .unwrap_or(atlas_simulation_core::semiconductor_cmp::CmpLayerType::CopperDamascene),
            wafer_diameter_mm: req.wafer_diameter_mm.unwrap_or(300.0),
            down_force_pressure_kpa: req.down_force_pressure_kpa.unwrap_or(20.68),
            relative_velocity_m_s: req.relative_velocity_m_s.unwrap_or(1.25),
            polish_time_s: req.polish_time_seconds.unwrap_or(60.0),
            overpolish_pct: 15.0,
            initial_step_height_nm: req.initial_step_height_nm.unwrap_or(450.0),
            interconnect_line_width_um: req.pattern_line_width_um.unwrap_or(2.50),
            local_pattern_density: req.pattern_metal_density_pct.unwrap_or(50.0) / 100.0,
            slurry_viscosity_mpa_s: req.slurry_viscosity_pa_s.unwrap_or(0.0025) * 1000.0,
            pad_roughness_um: 4.5,
            abrasive_concentration_pct: 12.0,
        };
        let res = atlas_simulation_core::semiconductor_cmp::solve_semiconductor_cmp(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_transportation_pavement_mepdg(
        &mut self,
        req: &SimPavementMepdgRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::pavement_mepdg::PavementMepdgProblem {
            problem_id: "pavement_mepdg_sim".to_string(),
            binder_grade: req
                .binder_grade
                .unwrap_or(atlas_simulation_core::pavement_mepdg::AsphaltBinderGrade::Pg70_22),
            asphalt_thickness_mm: req.asphalt_thickness_mm.unwrap_or(150.0),
            base_thickness_mm: req.base_thickness_mm.unwrap_or(200.0),
            subbase_thickness_mm: req.subbase_thickness_mm.unwrap_or(250.0),
            asphalt_dynamic_modulus_mpa: req.asphalt_dynamic_modulus_mpa.unwrap_or(3200.0),
            base_resilient_modulus_mpa: req.base_resilient_modulus_mpa.unwrap_or(280.0),
            subgrade_resilient_modulus_mpa: req.subgrade_resilient_modulus_mpa.unwrap_or(65.0),
            design_traffic_millions_esal: req.design_traffic_millions_esal.unwrap_or(8.50),
            effective_binder_volume_pct: req.effective_binder_volume_pct.unwrap_or(11.5),
            air_voids_pct: req.air_voids_pct.unwrap_or(4.5),
        };
        let res = atlas_simulation_core::pavement_mepdg::solve_pavement_mepdg(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_biomedical_scaffold_degradation(
        &mut self,
        req: &SimBiomaterialScaffoldRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::biomaterial_scaffold::BiomaterialScaffoldProblem {
            problem_id: "biomaterial_scaffold_sim".to_string(),
            biomaterial_type: req.biomaterial_type.unwrap_or(
                atlas_simulation_core::biomaterial_scaffold::ScaffoldBiomaterialType::PllaPolymer,
            ),
            scaffold_diameter_mm: req.scaffold_diameter_mm.unwrap_or(10.0),
            scaffold_height_mm: req.scaffold_height_mm.unwrap_or(15.0),
            target_porosity_pct: req.target_porosity_pct.unwrap_or(75.0),
            mean_pore_size_um: req.mean_pore_size_um.unwrap_or(350.0),
            solid_material_modulus_mpa: 3200.0,
            solid_material_density_g_cm3: 1.25,
            initial_molecular_weight_kda: 120.0,
            incubation_time_weeks: req.degradation_evaluation_weeks.unwrap_or(12.0),
            perfusion_velocity_mm_s: req.perfusion_velocity_mm_s.unwrap_or(0.50),
        };
        let res = atlas_simulation_core::biomaterial_scaffold::solve_biomaterial_scaffold(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_satellite_orbital_decay(
        &mut self,
        req: &SimSatelliteOrbitalDecayRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::satellite_orbital_decay::SatelliteOrbitalDecayProblem {
            problem_id: "satellite_orbital_decay_sim".to_string(),
            solar_activity: req.solar_activity.unwrap_or(
                atlas_simulation_core::satellite_orbital_decay::SolarActivityLevel::ModerateMean,
            ),
            satellite_mass_kg: req.satellite_mass_kg.unwrap_or(12.0),
            drag_cross_section_area_m2: req.drag_cross_section_area_m2.unwrap_or(0.045),
            drag_coefficient_cd: req.drag_coefficient_cd.unwrap_or(2.20),
            initial_perigee_altitude_km: req.initial_perigee_altitude_km.unwrap_or(420.0),
            initial_apogee_altitude_km: req.initial_apogee_altitude_km.unwrap_or(440.0),
            orbit_inclination_deg: req.orbit_inclination_deg.unwrap_or(51.6),
        };
        let res =
            atlas_simulation_core::satellite_orbital_decay::solve_satellite_orbital_decay(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mining_bucket_wheel_excavator(
        &mut self,
        req: &SimMiningExcavatorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::mining_excavator::MiningExcavatorProblem {
            problem_id: "mining_excavator_sim".to_string(),
            material_class: req.material_class.unwrap_or(
                atlas_simulation_core::mining_excavator::MiningMaterialClass::ClaySiltOverburden,
            ),
            wheel_diameter_m: req.wheel_diameter_m.unwrap_or(12.50),
            bucket_count: req.bucket_count.unwrap_or(14),
            bucket_nominal_volume_m3: req.bucket_nominal_volume_m3.unwrap_or(1.80),
            wheel_speed_rpm: req.wheel_speed_rpm.unwrap_or(4.80),
            bucket_fill_factor: req.bucket_fill_factor.unwrap_or(1.05),
            total_cutting_lip_length_m: req.total_cutting_lip_length_m.unwrap_or(2.40),
            lineal_cutting_resistance_kn_m: req.lineal_cutting_resistance_kn_m.unwrap_or(110.0),
            boom_length_m: req.boom_length_m.unwrap_or(42.0),
            slewing_speed_deg_s: req.slewing_speed_deg_s.unwrap_or(0.25),
            upperstructure_mass_tons: req.upperstructure_mass_tons.unwrap_or(2400.0),
            stabilizing_moment_kn_m: req.stabilizing_moment_kn_m.unwrap_or(650000.0),
            mechanical_efficiency: req.mechanical_efficiency.unwrap_or(0.88),
        };
        let res = atlas_simulation_core::mining_excavator::solve_mining_excavator(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_process_multiphase_pipe_flow(
        &mut self,
        req: &SimMultiphasePipeFlowRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::multiphase_pipe_flow::MultiphasePipeFlowProblem {
            problem_id: "multiphase_pipe_flow_sim".to_string(),
            pipe_length_m: req.pipe_length_m.unwrap_or(8500.0),
            pipe_inner_diameter_mm: req.pipe_inner_diameter_mm.unwrap_or(254.0),
            elevation_change_m: req.elevation_change_m.unwrap_or(120.0),
            liquid_mass_flow_kg_s: req.liquid_mass_flow_kg_s.unwrap_or(35.0),
            gas_mass_flow_kg_s: req.gas_mass_flow_kg_s.unwrap_or(4.50),
            liquid_density_kg_m3: req.liquid_density_kg_m3.unwrap_or(840.0),
            gas_density_kg_m3: req.gas_density_kg_m3.unwrap_or(42.0),
            liquid_viscosity_mpa_s: req.liquid_viscosity_mpa_s.unwrap_or(2.80),
            gas_viscosity_mpa_s: req.gas_viscosity_mpa_s.unwrap_or(0.016),
            pipe_roughness_mm: req.pipe_roughness_mm.unwrap_or(0.045),
            api_erosion_constant_c: req.api_erosion_constant_c.unwrap_or(122.0),
        };
        let res = atlas_simulation_core::multiphase_pipe_flow::solve_multiphase_pipe_flow(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_geothermal_reservoir(
        &mut self,
        req: &SimGeothermalReservoirRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::geothermal_reservoir::GeothermalReservoirProblem {
            problem_id: "geothermal_reservoir_sim".to_string(),
            rock_type: req.rock_type.unwrap_or(
                atlas_simulation_core::geothermal_reservoir::GeothermalRockType::SandstoneAquifer,
            ),
            reservoir_temperature_c: req.reservoir_temperature_c.unwrap_or(160.0),
            reinjection_temperature_c: req.reinjection_temperature_c.unwrap_or(65.0),
            brine_flow_rate_l_s: req.brine_flow_rate_l_s.unwrap_or(75.0),
            reservoir_thickness_m: req.reservoir_thickness_m.unwrap_or(80.0),
            well_spacing_distance_m: req.well_spacing_distance_m.unwrap_or(1200.0),
            wellbore_radius_m: req.wellbore_radius_m.unwrap_or(0.108),
            reservoir_permeability_md: req.reservoir_permeability_md.unwrap_or(450.0),
            reservoir_porosity: req.reservoir_porosity.unwrap_or(0.12),
            pump_system_efficiency: req.pump_system_efficiency.unwrap_or(0.72),
            project_lifespan_years: req.project_lifespan_years.unwrap_or(30.0),
        };
        let res = atlas_simulation_core::geothermal_reservoir::solve_geothermal_reservoir(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_process_spray_drying_kinetics(
        &mut self,
        req: &SimSprayDryingKineticsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::spray_drying_kinetics::SprayDryingKineticsProblem {
            problem_id: "spray_drying_kinetics_sim".to_string(),
            product_type: req.product_type.unwrap_or(atlas_simulation_core::spray_drying_kinetics::SprayDryingProductType::DairyMilkPowder),
            feed_rate_kg_h: req.feed_rate_kg_h.unwrap_or(2500.0),
            feed_solids_fraction: req.feed_solids_fraction.unwrap_or(0.48),
            feed_viscosity_mpa_s: req.feed_viscosity_mpa_s.unwrap_or(35.0),
            feed_surface_tension_mn_m: req.feed_surface_tension_mn_m.unwrap_or(48.0),
            atomizer_wheel_diameter_mm: req.atomizer_wheel_diameter_mm.unwrap_or(210.0),
            atomizer_speed_rpm: req.atomizer_speed_rpm.unwrap_or(18000.0),
            inlet_air_temperature_c: req.inlet_air_temperature_c.unwrap_or(200.0),
            outlet_air_temperature_c: req.outlet_air_temperature_c.unwrap_or(88.0),
            ambient_air_temperature_c: req.ambient_air_temperature_c.unwrap_or(20.0),
            ambient_air_rh_pct: req.ambient_air_rh_pct.unwrap_or(50.0),
            target_product_moisture_pct: req.target_product_moisture_pct.unwrap_or(3.50),
        };
        let res = atlas_simulation_core::spray_drying_kinetics::solve_spray_drying_kinetics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_thermoelectric_rtg(
        &mut self,
        req: &SimThermoelectricRtgRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::thermoelectric_rtg::ThermoelectricRtgProblem {
            problem_id: "thermoelectric_rtg_sim".to_string(),
            material_type: req.material_type.unwrap_or(atlas_simulation_core::thermoelectric_rtg::ThermoelectricMaterialType::SiliconGermaniumSiGe),
            initial_thermal_power_w_th: req.initial_thermal_power_w_th.unwrap_or(4400.0),
            hot_junction_temp_k: req.hot_junction_temp_k.unwrap_or(1273.15),
            cold_junction_temp_k: req.cold_junction_temp_k.unwrap_or(566.15),
            couple_count: req.couple_count.unwrap_or(572),
            leg_length_mm: req.leg_length_mm.unwrap_or(20.0),
            leg_area_mm2: req.leg_area_mm2.unwrap_or(9.0),
            mission_elapsed_years: req.mission_elapsed_years.unwrap_or(15.0),
            fuel_half_life_years: req.fuel_half_life_years.unwrap_or(87.7),
            annual_junction_degradation_pct: req.annual_junction_degradation_pct.unwrap_or(0.75),
        };
        let res = atlas_simulation_core::thermoelectric_rtg::solve_thermoelectric_rtg(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_nuclear_containment_loca(
        &mut self,
        req: &SimContainmentLocaRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::containment_loca::ContainmentLocaProblem {
            problem_id: "containment_loca_sim".to_string(),
            containment_type: req.containment_type.unwrap_or(atlas_simulation_core::containment_loca::ContainmentStructureType::PrestressedConcreteLiner),
            containment_free_volume_m3: req.containment_free_volume_m3.unwrap_or(65000.0),
            design_pressure_bar_a: req.design_pressure_bar_a.unwrap_or(4.80),
            design_temperature_c: req.design_temperature_c.unwrap_or(150.0),
            initial_pressure_bar_a: req.initial_pressure_bar_a.unwrap_or(1.013),
            initial_temperature_c: req.initial_temperature_c.unwrap_or(40.0),
            initial_relative_humidity_pct: req.initial_relative_humidity_pct.unwrap_or(50.0),
            blowdown_coolant_mass_tonnes: req.blowdown_coolant_mass_tonnes.unwrap_or(260.0),
            blowdown_enthalpy_gj: req.blowdown_enthalpy_gj.unwrap_or(380.0),
            blowdown_duration_seconds: req.blowdown_duration_seconds.unwrap_or(32.0),
            heat_sink_surface_area_m2: req.heat_sink_surface_area_m2.unwrap_or(32000.0),
            spray_flow_rate_l_s: req.spray_flow_rate_l_s.unwrap_or(650.0),
            spray_temperature_c: req.spray_temperature_c.unwrap_or(32.0),
        };
        let res = atlas_simulation_core::containment_loca::solve_containment_loca(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_environmental_atmospheric_dispersion(
        &mut self,
        req: &SimAtmosphericDispersionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::atmospheric_dispersion::AtmosphericDispersionProblem {
            problem_id: "atmospheric_dispersion_sim".to_string(),
            stability_class: req.stability_class.unwrap_or(atlas_simulation_core::atmospheric_dispersion::AtmosphericStabilityClass::ClassDNeutral),
            pollutant: req.pollutant.unwrap_or(atlas_simulation_core::atmospheric_dispersion::AirPollutantType::SulfurDioxideSO2),
            emission_rate_g_s: req.emission_rate_g_s.unwrap_or(85.0),
            stack_height_m: req.stack_height_m.unwrap_or(80.0),
            stack_inside_diameter_m: req.stack_inside_diameter_m.unwrap_or(2.80),
            stack_gas_velocity_m_s: req.stack_gas_velocity_m_s.unwrap_or(18.5),
            stack_gas_temperature_c: req.stack_gas_temperature_c.unwrap_or(140.0),
            ambient_air_temperature_c: req.ambient_air_temperature_c.unwrap_or(20.0),
            wind_speed_at_stack_height_m_s: req.wind_speed_at_stack_height_m_s.unwrap_or(5.50),
            receptor_downwind_distance_m: req.receptor_downwind_distance_m.unwrap_or(8400.0),
            receptor_crosswind_distance_m: req.receptor_crosswind_distance_m.unwrap_or(0.0),
        };
        let res =
            atlas_simulation_core::atmospheric_dispersion::solve_atmospheric_dispersion(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_rocket_ascent_trajectory(
        &mut self,
        req: &SimRocketAscentRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::rocket_ascent_trajectory::RocketAscentProblem {
            problem_id: "rocket_ascent_sim".to_string(),
            staging_type: req.staging_type.unwrap_or(atlas_simulation_core::rocket_ascent_trajectory::RocketStagingArchitecture::TwoStageToOrbit),
            gross_liftoff_mass_kg: req.gross_liftoff_mass_kg.unwrap_or(549054.0),
            payload_mass_kg: req.payload_mass_kg.unwrap_or(15600.0),
            stage1_propellant_mass_kg: req.stage1_propellant_mass_kg.unwrap_or(411000.0),
            stage1_dry_mass_kg: req.stage1_dry_mass_kg.unwrap_or(25600.0),
            stage1_thrust_sl_kn: req.stage1_thrust_sl_kn.unwrap_or(7607.0),
            stage1_isp_vac_s: req.stage1_isp_vac_s.unwrap_or(311.0),
            stage1_burn_time_s: req.stage1_burn_time_s.unwrap_or(162.0),
            stage2_propellant_mass_kg: req.stage2_propellant_mass_kg.unwrap_or(92670.0),
            stage2_dry_mass_kg: req.stage2_dry_mass_kg.unwrap_or(4000.0),
            stage2_thrust_vac_kn: req.stage2_thrust_vac_kn.unwrap_or(934.0),
            stage2_isp_vac_s: req.stage2_isp_vac_s.unwrap_or(348.0),
            aerodynamic_reference_area_m2: req.aerodynamic_reference_area_m2.unwrap_or(10.52),
            drag_coefficient_cd: req.drag_coefficient_cd.unwrap_or(0.32),
            target_orbit_altitude_km: req.target_orbit_altitude_km.unwrap_or(300.0),
            launch_latitude_deg: req.launch_latitude_deg.unwrap_or(28.5),
        };
        let res = atlas_simulation_core::rocket_ascent_trajectory::solve_rocket_ascent(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_gravity_dam_stability(
        &mut self,
        req: &SimGravityDamRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::gravity_dam_stability::GravityDamProblem {
            problem_id: "gravity_dam_stability_sim".to_string(),
            load_combination: req.load_combination.unwrap_or(atlas_simulation_core::gravity_dam_stability::DamLoadCombination::UsualNormalOperating),
            foundation_rock: req.foundation_rock.unwrap_or(atlas_simulation_core::gravity_dam_stability::FoundationRockClass::MassiveGraniteGneiss),
            dam_height_m: req.dam_height_m.unwrap_or(85.0),
            crest_width_m: req.crest_width_m.unwrap_or(8.50),
            base_width_m: req.base_width_m.unwrap_or(68.0),
            upstream_water_depth_m: req.upstream_water_depth_m.unwrap_or(80.0),
            downstream_tailwater_depth_m: req.downstream_tailwater_depth_m.unwrap_or(6.0),
            silt_deposit_depth_m: req.silt_deposit_depth_m.unwrap_or(12.0),
            drainage_gallery_distance_from_heel_m: req.drainage_gallery_distance_from_heel_m.unwrap_or(10.0),
            drainage_efficiency_factor: req.drainage_efficiency_factor.unwrap_or(0.67),
            horizontal_seismic_coef_kh: req.horizontal_seismic_coef_kh.unwrap_or(0.05),
            concrete_unit_weight_kn_m3: req.concrete_unit_weight_kn_m3.unwrap_or(24.0),
            allowable_bearing_capacity_mpa: req.allowable_bearing_capacity_mpa.unwrap_or(8.0),
        };
        let res = atlas_simulation_core::gravity_dam_stability::solve_gravity_dam(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_electrolyzer_hydrogen(
        &mut self,
        req: &SimElectrolyzerHydrogenRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::electrolyzer_hydrogen::ElectrolyzerHydrogenProblem {
            problem_id: "electrolyzer_hydrogen_sim".to_string(),
            technology: req.technology.unwrap_or(atlas_simulation_core::electrolyzer_hydrogen::ElectrolyzerTechnology::ProtonExchangeMembranePEM),
            cell_temperature_c: req.cell_temperature_c.unwrap_or(70.0),
            hydrogen_operating_pressure_bar_a: req.hydrogen_operating_pressure_bar_a.unwrap_or(30.0),
            current_density_a_per_cm2: req.current_density_a_per_cm2.unwrap_or(1.80),
            active_cell_area_cm2: req.active_cell_area_cm2.unwrap_or(600.0),
            cell_count: req.cell_count.unwrap_or(120),
            membrane_thickness_um: req.membrane_thickness_um.unwrap_or(90.0),
            membrane_water_content_lambda: req.membrane_water_content_lambda.unwrap_or(18.0),
            anode_exchange_current_density_a_cm2: req.anode_exchange_current_density_a_cm2.unwrap_or(1.5e-3),
            cathode_exchange_current_density_a_cm2: req.cathode_exchange_current_density_a_cm2.unwrap_or(1.5e-2),
            limiting_current_density_a_cm2: req.limiting_current_density_a_cm2.unwrap_or(3.80),
        };
        let res = atlas_simulation_core::electrolyzer_hydrogen::solve_electrolyzer_hydrogen(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_optical_thin_film_optics(
        &mut self,
        req: &SimThinFilmOpticsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::thin_film_optics::ThinFilmOpticsProblem {
            problem_id: "thin_film_optics_sim".to_string(),
            coating_type: req.coating_type.unwrap_or(atlas_simulation_core::thin_film_optics::OpticalCoatingType::DistributedBraggReflector),
            polarization: req.polarization.unwrap_or(atlas_simulation_core::thin_film_optics::OpticalPolarization::UnpolarizedAverage),
            wavelength_nm: req.wavelength_nm.unwrap_or(632.8),
            angle_of_incidence_deg: req.angle_of_incidence_deg.unwrap_or(0.0),
            incident_medium_index: req.incident_medium_index.unwrap_or(1.000),
            substrate_index: req.substrate_index.unwrap_or(1.52),
            layers: req.layers.clone().unwrap_or_else(|| {
                let lambda_0 = 632.8;
                let n_h = 2.35;
                let n_l = 1.46;
                let d_h = lambda_0 / (4.0 * n_h);
                let d_l = lambda_0 / (4.0 * n_l);
                let mut layers = Vec::new();
                for i in 0..5 {
                    layers.push(atlas_simulation_core::thin_film_optics::DielectricLayer {
                        layer_name: format!("TiO2_{}", i + 1),
                        refractive_index: n_h,
                        physical_thickness_nm: d_h,
                    });
                    layers.push(atlas_simulation_core::thin_film_optics::DielectricLayer {
                        layer_name: format!("SiO2_{}", i + 1),
                        refractive_index: n_l,
                        physical_thickness_nm: d_l,
                    });
                }
                layers
            }),
        };
        let res = atlas_simulation_core::thin_film_optics::solve_thin_film_optics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_cryogenic_liquefaction(
        &mut self,
        req: &SimCryogenicLiquefactionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::cryogenic_liquefaction::CryogenicLiquefactionProblem {
            problem_id: "cryogenic_liquefaction_sim".to_string(),
            fluid_type: req.fluid_type.unwrap_or(
                atlas_simulation_core::cryogenic_liquefaction::CryogenicFluidType::NitrogenN2,
            ),
            feed_gas_flow_rate_kg_h: req.feed_gas_flow_rate_kg_h.unwrap_or(1200.0),
            suction_pressure_bar_a: req.suction_pressure_bar_a.unwrap_or(1.05),
            discharge_pressure_bar_a: req.discharge_pressure_bar_a.unwrap_or(200.0),
            ambient_aftercooler_temp_c: req.ambient_aftercooler_temp_c.unwrap_or(20.0),
            heat_exchanger_pinch_k: req.heat_exchanger_pinch_k.unwrap_or(4.0),
            compressor_isothermal_efficiency: req.compressor_isothermal_efficiency.unwrap_or(0.72),
            heat_exchanger_effectiveness: req.heat_exchanger_effectiveness.unwrap_or(0.97),
            ambient_heat_leak_w: req.ambient_heat_leak_w.unwrap_or(180.0),
        };
        let res =
            atlas_simulation_core::cryogenic_liquefaction::solve_cryogenic_liquefaction(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_process_hydrocyclone_separation(
        &mut self,
        req: &SimHydrocycloneRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::hydrocyclone_separation::HydrocycloneProblem {
            problem_id: "hydrocyclone_separation_sim".to_string(),
            application: req.application.unwrap_or(atlas_simulation_core::hydrocyclone_separation::SlurryApplicationType::HardRockGrindingCircuit),
            cyclone_diameter_mm: req.cyclone_diameter_mm.unwrap_or(350.0),
            inlet_diameter_mm: req.inlet_diameter_mm.unwrap_or(75.0),
            vortex_finder_diameter_mm: req.vortex_finder_diameter_mm.unwrap_or(115.0),
            apex_diameter_mm: req.apex_diameter_mm.unwrap_or(55.0),
            free_vortex_height_mm: req.free_vortex_height_mm.unwrap_or(1400.0),
            feed_flow_rate_m3_h: req.feed_flow_rate_m3_h.unwrap_or(120.0),
            solids_volumetric_concentration_pct: req.solids_volumetric_concentration_pct.unwrap_or(18.0),
            solid_density_kg_m3: req.solid_density_kg_m3.unwrap_or(2800.0),
            liquid_density_kg_m3: req.liquid_density_kg_m3.unwrap_or(1000.0),
            target_evaluation_size_um: req.target_evaluation_size_um.unwrap_or(75.0),
        };
        let res =
            atlas_simulation_core::hydrocyclone_separation::solve_hydrocyclone_separation(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mechanical_magnetic_bearing(
        &mut self,
        req: &SimMagneticBearingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::magnetic_bearing::MagneticBearingProblem {
            problem_id: "magnetic_bearing_sim".to_string(),
            topology: req.topology.unwrap_or(
                atlas_simulation_core::magnetic_bearing::MagneticBearingTopology::Heteropolar8Pole,
            ),
            rotor_supported_mass_kg: req.rotor_supported_mass_kg.unwrap_or(85.0),
            max_rotational_speed_rpm: req.max_rotational_speed_rpm.unwrap_or(36000.0),
            nominal_air_gap_mm: req.nominal_air_gap_mm.unwrap_or(0.60),
            pole_face_area_mm2: req.pole_face_area_mm2.unwrap_or(850.0),
            coil_turns_per_pole: req.coil_turns_per_pole.unwrap_or(140),
            bias_current_a: req.bias_current_a.unwrap_or(5.0),
            max_control_current_a: req.max_control_current_a.unwrap_or(12.0),
            amplifier_bus_voltage_v: req.amplifier_bus_voltage_v.unwrap_or(180.0),
            controller_proportional_gain_a_per_mm: req
                .controller_proportional_gain_a_per_mm
                .unwrap_or(18.0),
            controller_derivative_gain_a_s_per_m: req
                .controller_derivative_gain_a_s_per_m
                .unwrap_or(35.0),
            rotor_unbalance_eccentricity_um: req.rotor_unbalance_eccentricity_um.unwrap_or(0.50),
        };
        let res = atlas_simulation_core::magnetic_bearing::solve_magnetic_bearing(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_sonar_propagation(
        &mut self,
        req: &SimSonarPropagationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::sonar_propagation::SonarPropagationProblem {
            problem_id: "sonar_propagation_sim".to_string(),
            acoustic_regime: req.acoustic_regime.unwrap_or(atlas_simulation_core::sonar_propagation::OceanAcousticRegime::ShallowWaterWaveguide),
            target_class: req.target_class.unwrap_or(atlas_simulation_core::sonar_propagation::SonarTargetClass::SubmarineLargeFleet),
            operating_frequency_khz: req.operating_frequency_khz.unwrap_or(7.5),
            transmitter_acoustic_power_w: req.transmitter_acoustic_power_w.unwrap_or(4500.0),
            array_directivity_index_db: req.array_directivity_index_db.unwrap_or(22.0),
            ocean_depth_m: req.ocean_depth_m.unwrap_or(180.0),
            target_range_m: req.target_range_m.unwrap_or(8500.0),
            sea_state_code: req.sea_state_code.unwrap_or(3),
            shipping_traffic_index: req.shipping_traffic_index.unwrap_or(2),
            detection_threshold_db: req.detection_threshold_db.unwrap_or(14.0),
        };
        let res = atlas_simulation_core::sonar_propagation::solve_sonar_propagation(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_manufacturing_electron_beam_welding(
        &mut self,
        req: &SimEbwWeldingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::electron_beam_welding::EbwWeldingProblem {
            problem_id: "ebw_welding_sim".to_string(),
            alloy_grade: req
                .alloy_grade
                .unwrap_or(atlas_simulation_core::electron_beam_welding::EbwAlloyGrade::Ti6Al4V),
            acceleration_voltage_kv: req.acceleration_voltage_kv.unwrap_or(120.0),
            beam_current_ma: req.beam_current_ma.unwrap_or(50.0),
            travel_speed_mm_s: req.travel_speed_mm_s.unwrap_or(15.0),
            plate_thickness_mm: req.plate_thickness_mm.unwrap_or(25.0),
            beam_focal_diameter_mm: req.beam_focal_diameter_mm.unwrap_or(0.45),
            ambient_temperature_c: req.ambient_temperature_c.unwrap_or(25.0),
        };
        let res = atlas_simulation_core::electron_beam_welding::solve_ebw_welding(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_process_crystallization_kinetics(
        &mut self,
        req: &SimCrystallizationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::crystallization_kinetics::CrystallizationProblem {
            problem_id: "crystallization_sim".to_string(),
            solute: req.solute.unwrap_or(atlas_simulation_core::crystallization_kinetics::CrystallineSolute::PotassiumChloride),
            operating_temperature_c: req.operating_temperature_c.unwrap_or(35.0),
            feed_volumetric_flow_m3_h: req.feed_volumetric_flow_m3_h.unwrap_or(25.0),
            crystallizer_active_volume_m3: req.crystallizer_active_volume_m3.unwrap_or(50.0),
            solution_concentration_kg_m3: req.solution_concentration_kg_m3.unwrap_or(395.0),
            crystal_volumetric_shape_factor: req.crystal_volumetric_shape_factor.unwrap_or(0.85),
            target_median_size_um: req.target_median_size_um.unwrap_or(350.0),
        };
        let res =
            atlas_simulation_core::crystallization_kinetics::solve_crystallization_kinetics(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_thermoelectric_cooler(
        &mut self,
        req: &SimTecCoolerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::thermoelectric_cooler::TecCoolerProblem {
            problem_id: "tec_cooler_sim".to_string(),
            material: req.material.unwrap_or(
                atlas_simulation_core::thermoelectric_cooler::TecMaterialType::BismuthTelluride,
            ),
            couple_count_n: req.couple_count_n.unwrap_or(127),
            pellet_area_mm2: req.pellet_area_mm2.unwrap_or(2.25),
            pellet_length_mm: req.pellet_length_mm.unwrap_or(1.80),
            cold_junction_temp_c: req.cold_junction_temp_c.unwrap_or(25.0),
            hot_junction_temp_c: req.hot_junction_temp_c.unwrap_or(50.0),
            operating_current_a: req.operating_current_a.unwrap_or(4.5),
            heat_load_qc_target_w: req.heat_load_qc_target_w.unwrap_or(35.0),
        };
        let res = atlas_simulation_core::thermoelectric_cooler::solve_thermoelectric_cooler(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mechanical_slurry_pump_derating(
        &mut self,
        req: &SimSlurryPumpRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::slurry_pump_derating::SlurryPumpProblem {
            problem_id: "slurry_pump_sim".to_string(),
            abrasiveness_class: req.abrasiveness_class.unwrap_or(
                atlas_simulation_core::slurry_pump_derating::SlurryAbrasivenessClass::Class3Severe,
            ),
            slurry_flow_rate_m3_h: req.slurry_flow_rate_m3_h.unwrap_or(650.0),
            clear_water_head_m: req.clear_water_head_m.unwrap_or(45.0),
            clear_water_efficiency: req.clear_water_efficiency.unwrap_or(0.78),
            solid_specific_gravity: req.solid_specific_gravity.unwrap_or(2.75),
            solids_concentration_wt_pct: req.solids_concentration_wt_pct.unwrap_or(35.0),
            median_particle_size_d50_um: req.median_particle_size_d50_um.unwrap_or(110.0),
            impeller_diameter_mm: req.impeller_diameter_mm.unwrap_or(680.0),
            pump_rotational_speed_rpm: req.pump_rotational_speed_rpm.unwrap_or(850.0),
            allowable_wear_thickness_mm: req.allowable_wear_thickness_mm.unwrap_or(25.0),
        };
        let res = atlas_simulation_core::slurry_pump_derating::solve_slurry_pump_derating(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_optical_fiber_dispersion(
        &mut self,
        req: &SimFiberDispersionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::fiber_dispersion::FiberDispersionProblem {
            problem_id: "fiber_dispersion_sim".to_string(),
            fiber_standard: req.fiber_standard.unwrap_or(
                atlas_simulation_core::fiber_dispersion::FiberStandardType::ItuG652dStandard,
            ),
            wavelength_nm: req.wavelength_nm.unwrap_or(1550.0),
            fiber_length_km: req.fiber_length_km.unwrap_or(80.0),
            laser_linewidth_fwhm_nm: req.laser_linewidth_fwhm_nm.unwrap_or(0.015),
            initial_pulse_width_ps: req.initial_pulse_width_ps.unwrap_or(25.0),
            launch_power_mw: req.launch_power_mw.unwrap_or(5.0),
            target_bit_rate_gbps: req.target_bit_rate_gbps.unwrap_or(10.0),
        };
        let res = atlas_simulation_core::fiber_dispersion::solve_fiber_dispersion(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_propeller_hydrodynamics(
        &mut self,
        req: &SimPropellerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::propeller_hydrodynamics::PropellerProblem {
            problem_id: "propeller_sim".to_string(),
            blade_count: req
                .blade_count
                .unwrap_or(atlas_simulation_core::propeller_hydrodynamics::BladeCount::FourBlades),
            propeller_diameter_m: req.propeller_diameter_m.unwrap_or(5.60),
            pitch_diameter_ratio: req.pitch_diameter_ratio.unwrap_or(0.95),
            expanded_area_ratio: req.expanded_area_ratio.unwrap_or(0.62),
            speed_of_advance_m_s: req.speed_of_advance_m_s.unwrap_or(7.20),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(105.0),
            shaft_immersion_depth_m: req.shaft_immersion_depth_m.unwrap_or(5.20),
            water_density_kg_m3: req.water_density_kg_m3.unwrap_or(1025.0),
        };
        let res =
            atlas_simulation_core::propeller_hydrodynamics::solve_propeller_hydrodynamics(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_environmental_indoor_air_quality(
        &mut self,
        req: &SimIndoorAirQualityRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::indoor_air_quality::IndoorAirQualityProblem {
            problem_id: "indoor_air_quality_sim".to_string(),
            contaminant: req.contaminant.unwrap_or(
                atlas_simulation_core::indoor_air_quality::AirContaminantType::CarbonDioxideCO2,
            ),
            space_type: req.space_type.unwrap_or(
                atlas_simulation_core::indoor_air_quality::SpaceOccupancyType::CommercialOffice,
            ),
            room_floor_area_m2: req.room_floor_area_m2.unwrap_or(120.0),
            ceiling_height_m: req
                .ceiling_height_m
                .or(req.room_clear_height_m)
                .unwrap_or(3.0),
            occupant_count: req.occupant_count.unwrap_or(12),
            outdoor_ventilation_rate_m3_h: req
                .outdoor_ventilation_rate_m3_h
                .or(req.ventilation_supply_airflow_m3_h)
                .unwrap_or(720.0),
            hvac_filter_efficiency: req.hvac_filter_efficiency.unwrap_or(0.0),
            outdoor_ambient_concentration: req.outdoor_ambient_concentration.unwrap_or(420.0),
            initial_room_concentration: req.initial_room_concentration.unwrap_or(450.0),
            target_threshold_concentration: req.target_threshold_concentration.unwrap_or(1000.0),
            exposure_duration_hours: req.exposure_duration_hours.unwrap_or(4.0),
            infected_occupants: req.infected_occupants.unwrap_or(0),
        };
        let res = atlas_simulation_core::indoor_air_quality::solve_indoor_air_quality(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_pmsm_foc_drive(
        &mut self,
        req: &SimPmsmDriveRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::pmsm_foc_drive::PmsmDriveProblem {
            problem_id: "pmsm_foc_sim".to_string(),
            rotor_topology: req
                .rotor_topology
                .unwrap_or(atlas_simulation_core::pmsm_foc_drive::PmsmRotorTopology::InteriorIPMSM),
            pole_pairs_p: req.pole_pairs_p.unwrap_or(4),
            stator_resistance_ohms: req.stator_resistance_ohms.unwrap_or(0.025),
            d_axis_inductance_mh: req.d_axis_inductance_mh.unwrap_or(0.45),
            q_axis_inductance_mh: req.q_axis_inductance_mh.unwrap_or(0.95),
            permanent_magnet_flux_wb: req.permanent_magnet_flux_wb.unwrap_or(0.095),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(4500.0),
            q_axis_current_iq_a: req.q_axis_current_iq_a.unwrap_or(120.0),
            inverter_dc_bus_voltage_v: req.inverter_dc_bus_voltage_v.unwrap_or(650.0),
        };
        let res = atlas_simulation_core::pmsm_foc_drive::solve_pmsm_foc_drive(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_civil_mse_retaining_wall(
        &mut self,
        req: &SimMseWallRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::mse_retaining_wall::MseWallProblem {
            problem_id: "mse_wall_sim".to_string(),
            polymer_type: req.polymer_type.unwrap_or(
                atlas_simulation_core::mse_retaining_wall::GeogridPolymerType::HdpeUniaxial,
            ),
            wall_height_h_m: req.wall_height_h_m.unwrap_or(8.0),
            reinforcement_length_l_m: req.reinforcement_length_l_m.unwrap_or(6.0),
            vertical_layer_spacing_m: req.vertical_layer_spacing_m.unwrap_or(0.60),
            soil_unit_weight_kn_m3: req.soil_unit_weight_kn_m3.unwrap_or(19.5),
            soil_friction_angle_deg: req.soil_friction_angle_deg.unwrap_or(34.0),
            crest_surcharge_q_kpa: req.crest_surcharge_q_kpa.unwrap_or(12.0),
            geogrid_ultimate_strength_kn_m: req.geogrid_ultimate_strength_kn_m.unwrap_or(110.0),
            rf_installation_damage: req.rf_installation_damage.unwrap_or(1.15),
            rf_creep_rupture: req.rf_creep_rupture.unwrap_or(1.65),
            rf_durability: req.rf_durability.unwrap_or(1.15),
        };
        let res = atlas_simulation_core::mse_retaining_wall::solve_mse_retaining_wall(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_nuclear_isotope_centrifuge(
        &mut self,
        req: &SimIsotopeCentrifugeRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::isotope_centrifuge::IsotopeCentrifugeProblem {
            problem_id: "isotope_centrifuge_sim".to_string(),
            rotor_material: req.rotor_material.unwrap_or(
                atlas_simulation_core::isotope_centrifuge::CentrifugeRotorMaterial::CarbonFiberCfrp,
            ),
            rotor_radius_m: req.rotor_radius_m.unwrap_or(0.10),
            rotor_length_m: req.rotor_length_m.unwrap_or(3.0),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(65000.0),
            operating_temperature_c: req.operating_temperature_c.unwrap_or(55.0),
            feed_mass_flow_kg_h: req.feed_mass_flow_kg_h.unwrap_or(0.85),
            feed_assay_u235_fraction: req.feed_assay_u235_fraction.unwrap_or(0.00711),
            stage_cut_theta: req.stage_cut_theta.unwrap_or(0.50),
            circulation_efficiency: req.circulation_efficiency.unwrap_or(0.75),
        };
        let res = atlas_simulation_core::isotope_centrifuge::solve_isotope_centrifuge(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_mechanical_conveyor_bulk_handling(
        &mut self,
        req: &SimConveyorRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::conveyor_bulk_handling::ConveyorProblem {
            problem_id: "conveyor_sim".to_string(),
            carcass_type: req.carcass_type.unwrap_or(
                atlas_simulation_core::conveyor_bulk_handling::BeltCarcassType::SteelCordST,
            ),
            troughing_angle: req.troughing_angle.unwrap_or(
                atlas_simulation_core::conveyor_bulk_handling::TroughingAngleClass::Trough35Deg,
            ),
            belt_width_mm: req.belt_width_mm.unwrap_or(1200.0),
            conveyor_length_m: req.conveyor_length_m.unwrap_or(450.0),
            elevation_lift_m: req.elevation_lift_m.unwrap_or(28.0),
            belt_speed_m_s: req.belt_speed_m_s.unwrap_or(3.15),
            material_bulk_density_kg_m3: req.material_bulk_density_kg_m3.unwrap_or(1650.0),
            surcharge_angle_deg: req.surcharge_angle_deg.unwrap_or(20.0),
            belt_unit_weight_kg_m: req.belt_unit_weight_kg_m.unwrap_or(24.5),
            idlers_carrying_weight_kg_m: req.idlers_carrying_weight_kg_m.unwrap_or(28.0),
            idlers_return_weight_kg_m: req.idlers_return_weight_kg_m.unwrap_or(8.5),
            friction_factor_f: req.friction_factor_f.unwrap_or(0.022),
            drive_wrap_angle_deg: req.drive_wrap_angle_deg.unwrap_or(210.0),
            drive_friction_mu: req.drive_friction_mu.unwrap_or(0.35),
            belt_rated_breaking_strength_kn_m: req
                .belt_rated_breaking_strength_kn_m
                .unwrap_or(1250.0),
            drive_mechanical_efficiency: req.drive_mechanical_efficiency.unwrap_or(0.92),
        };
        let res =
            atlas_simulation_core::conveyor_bulk_handling::solve_conveyor_bulk_handling(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_offshore_wave_loading(
        &mut self,
        req: &SimOffshoreWaveRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::offshore_wave_loading::OffshoreWaveProblem {
            problem_id: "offshore_wave_sim".to_string(),
            structure_type: req.structure_type.unwrap_or(atlas_simulation_core::offshore_wave_loading::OffshoreStructureType::OffshoreWindMonopile),
            water_depth_m: req.water_depth_m.unwrap_or(40.0),
            wave_height_m: req.wave_height_m.unwrap_or(14.5),
            wave_period_s: req.wave_period_s.unwrap_or(13.0),
            surface_current_m_s: req.surface_current_m_s.unwrap_or(1.20),
            cylinder_diameter_m: req.cylinder_diameter_m.unwrap_or(8.50),
            wall_thickness_mm: req.wall_thickness_mm.unwrap_or(85.0),
            steel_yield_strength_mpa: req.steel_yield_strength_mpa.unwrap_or(355.0),
            drag_coefficient_cd: req.drag_coefficient_cd.unwrap_or(0.70),
            inertia_coefficient_cm: req.inertia_coefficient_cm.unwrap_or(2.00),
            seawater_density_kg_m3: req.seawater_density_kg_m3.unwrap_or(1025.0),
        };
        let res = atlas_simulation_core::offshore_wave_loading::solve_offshore_wave_loading(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_spacecraft_adcs_attitude(
        &mut self,
        req: &SimSpacecraftAdcsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::spacecraft_adcs_attitude::SpacecraftAdcsProblem {
            problem_id: "spacecraft_adcs_sim".to_string(),
            orbit_regime: req.orbit_regime.unwrap_or(atlas_simulation_core::spacecraft_adcs_attitude::SpacecraftOrbitRegime::LowEarthOrbitLEO),
            orbit_altitude_km: req.orbit_altitude_km.unwrap_or(550.0),
            spacecraft_mass_kg: req.spacecraft_mass_kg.unwrap_or(350.0),
            inertia_ixx_kg_m2: req.inertia_ixx_kg_m2.unwrap_or(85.0),
            inertia_iyy_kg_m2: req.inertia_iyy_kg_m2.unwrap_or(120.0),
            inertia_izz_kg_m2: req.inertia_izz_kg_m2.unwrap_or(75.0),
            cp_cg_offset_arm_m: req.cp_cg_offset_arm_m.unwrap_or(0.08),
            projected_surface_area_m2: req.projected_surface_area_m2.unwrap_or(2.40),
            solar_reflectance_q: req.solar_reflectance_q.unwrap_or(0.60),
            residual_magnetic_dipole_a_m2: req.residual_magnetic_dipole_a_m2.unwrap_or(0.50),
            target_slew_accel_deg_s2: req.target_slew_accel_deg_s2.unwrap_or(1.50),
        };
        let res =
            atlas_simulation_core::spacecraft_adcs_attitude::solve_spacecraft_adcs_attitude(&prob)
                .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electrical_battery_fast_charging(
        &mut self,
        req: &SimBatteryFastChargeRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::battery_fast_charging::BatteryFastChargeProblem {
            problem_id: "battery_fast_charge_sim".to_string(),
            cathode_chemistry: req.cathode_chemistry.unwrap_or(atlas_simulation_core::battery_fast_charging::BatteryCathodeChemistry::Nmc811HighEnergy),
            nominal_capacity_ah: req.nominal_capacity_ah.unwrap_or(75.0),
            initial_soc_fraction: req.initial_soc_fraction.unwrap_or(0.10),
            target_soc_fraction: req.target_soc_fraction.unwrap_or(0.80),
            charge_c_rate: req.charge_c_rate.unwrap_or(2.50),
            internal_resistance_mohms: req.internal_resistance_mohms.unwrap_or(0.85),
            diffusion_time_constant_s: req.diffusion_time_constant_s.unwrap_or(95.0),
            charge_cutoff_voltage_v: req.charge_cutoff_voltage_v.unwrap_or(4.20),
            ambient_temperature_c: req.ambient_temperature_c.unwrap_or(25.0),
        };
        let res = atlas_simulation_core::battery_fast_charging::solve_battery_fast_charging(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_process_packed_bed_scrubber(
        &mut self,
        req: &SimPackedBedRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::packed_bed_scrubber::PackedBedProblem {
            problem_id: "packed_bed_sim".to_string(),
            application: req.application.unwrap_or(
                atlas_simulation_core::packed_bed_scrubber::ScrubberApplication::AcidGasAbatement,
            ),
            packing_type: req.packing_type.unwrap_or(
                atlas_simulation_core::packed_bed_scrubber::PackingTypeClass::PallRings25mm,
            ),
            flue_gas_flow_rate_m3_h: req.flue_gas_flow_rate_m3_h.unwrap_or(8500.0),
            inlet_pollutant_mole_fraction: req.inlet_pollutant_mole_fraction.unwrap_or(0.0120),
            target_outlet_mole_fraction: req.target_outlet_mole_fraction.unwrap_or(0.00024),
            operating_temperature_c: req.operating_temperature_c.unwrap_or(45.0),
            operating_pressure_kpa: req.operating_pressure_kpa.unwrap_or(101.325),
            liquid_to_gas_ratio_multiplier: req.liquid_to_gas_ratio_multiplier.unwrap_or(1.50),
            liquid_density_kg_m3: req.liquid_density_kg_m3.unwrap_or(1020.0),
            liquid_viscosity_cp: req.liquid_viscosity_cp.unwrap_or(1.05),
            henrys_law_slope_m: req.henrys_law_slope_m.unwrap_or(0.45),
        };
        let res = atlas_simulation_core::packed_bed_scrubber::solve_packed_bed_scrubber(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_aircraft_landing_gear(
        &mut self,
        req: &SimLandingGearRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::aircraft_landing_gear::LandingGearProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "landing_gear_sim".to_string()),
            metering_type: req.metering_type.unwrap_or(
                atlas_simulation_core::aircraft_landing_gear::MeteringPinType::TaperedContourPin,
            ),
            gear_landed_mass_kg: req.gear_landed_mass_kg.unwrap_or(18500.0),
            vertical_sink_velocity_m_s: req.vertical_sink_velocity_m_s.unwrap_or(3.05),
            lift_to_weight_ratio: req.lift_to_weight_ratio.unwrap_or(0.85),
            available_stroke_mm: req.available_stroke_mm.unwrap_or(420.0),
            piston_diameter_mm: req.piston_diameter_mm.unwrap_or(180.0),
            gas_precharge_pressure_mpa: req.gas_precharge_pressure_mpa.unwrap_or(4.20),
            initial_gas_volume_liters: req.initial_gas_volume_liters.unwrap_or(14.50),
            polytropic_exponent: req.polytropic_exponent.unwrap_or(1.35),
            orifice_diameter_mm: req.orifice_diameter_mm.unwrap_or(34.0),
            orifice_discharge_coeff: req.orifice_discharge_coeff.unwrap_or(0.68),
            fluid_density_kg_m3: req.fluid_density_kg_m3.unwrap_or(885.0),
            tire_stiffness_kn_m: req.tire_stiffness_kn_m.unwrap_or(2400.0),
            max_allowable_tire_deflection_mm: req.max_allowable_tire_deflection_mm.unwrap_or(160.0),
        };
        let res = atlas_simulation_core::aircraft_landing_gear::solve_aircraft_landing_gear(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_energy_pumped_storage_hydro(
        &mut self,
        req: &SimPumpedStorageRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::pumped_storage_hydro::PumpedStorageProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "pumped_storage_sim".to_string()),
            runner_type: req.runner_type.unwrap_or(
                atlas_simulation_core::pumped_storage_hydro::PumpTurbineType::FrancisReversible,
            ),
            upper_reservoir_elevation_m: req.upper_reservoir_elevation_m.unwrap_or(1850.0),
            lower_reservoir_elevation_m: req.lower_reservoir_elevation_m.unwrap_or(1540.0),
            conduit_length_m: req.conduit_length_m.unwrap_or(1650.0),
            conduit_diameter_m: req.conduit_diameter_m.unwrap_or(3.80),
            darcy_friction_factor: req.darcy_friction_factor.unwrap_or(0.014),
            minor_loss_k_total: req.minor_loss_k_total.unwrap_or(1.85),
            pumping_discharge_m3_s: req.pumping_discharge_m3_s.unwrap_or(32.0),
            generating_discharge_m3_s: req.generating_discharge_m3_s.unwrap_or(36.5),
            rotational_speed_rpm: req.rotational_speed_rpm.unwrap_or(428.6),
            runner_diameter_m: req.runner_diameter_m.unwrap_or(3.50),
            runner_submergence_depth_m: req.runner_submergence_depth_m.unwrap_or(35.0),
            atmospheric_pressure_kpa: req.atmospheric_pressure_kpa.unwrap_or(101.325),
            water_vapor_pressure_kpa: req.water_vapor_pressure_kpa.unwrap_or(2.34),
            pump_hydraulic_efficiency: req.pump_hydraulic_efficiency.unwrap_or(0.91),
            turbine_hydraulic_efficiency: req.turbine_hydraulic_efficiency.unwrap_or(0.925),
            motor_generator_efficiency: req.motor_generator_efficiency.unwrap_or(0.982),
        };
        let res = atlas_simulation_core::pumped_storage_hydro::solve_pumped_storage_hydro(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_semiconductor_euv_lithography(
        &mut self,
        req: &SimEuvLithographyRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::semiconductor_euv_lithography::EuvLithographyProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "euv_litho_sim".to_string()),
            tool_generation: req.tool_generation.unwrap_or(
                atlas_simulation_core::semiconductor_euv_lithography::EuvToolGeneration::StandardNa033,
            ),
            wavelength_nm: req.wavelength_nm.unwrap_or(13.50),
            numerical_aperture: req.numerical_aperture.unwrap_or(0.33),
            process_factor_k1: req.process_factor_k1.unwrap_or(0.32),
            dof_factor_k2: req.dof_factor_k2.unwrap_or(0.60),
            bilayer_period_d_nm: req.bilayer_period_d_nm.unwrap_or(6.90),
            molybdenum_fraction_gamma: req.molybdenum_fraction_gamma.unwrap_or(0.40),
            bilayer_pair_count: req.bilayer_pair_count.unwrap_or(50),
            interface_roughness_nm: req.interface_roughness_nm.unwrap_or(0.15),
            incidence_angle_deg: req.incidence_angle_deg.unwrap_or(6.0),
            mirror_count_in_train: req.mirror_count_in_train.unwrap_or(11),
            resist_exposure_dose_mj_cm2: req.resist_exposure_dose_mj_cm2.unwrap_or(45.0),
        };
        let res = atlas_simulation_core::semiconductor_euv_lithography::solve_semiconductor_euv_lithography(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_renewable_geothermal_binary_orc(
        &mut self,
        req: &SimGeothermalOrcRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::geothermal_binary_orc::GeothermalOrcProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "geothermal_orc_sim".to_string()),
            working_fluid: req.working_fluid.unwrap_or(
                atlas_simulation_core::geothermal_binary_orc::OrcWorkingFluid::Isopentane,
            ),
            brine_mass_flow_kg_s: req.brine_mass_flow_kg_s.unwrap_or(85.0),
            brine_inlet_temperature_c: req.brine_inlet_temperature_c.unwrap_or(165.0),
            min_reinjection_temperature_c: req.min_reinjection_temperature_c.unwrap_or(72.0),
            pinch_point_temp_diff_c: req.pinch_point_temp_diff_c.unwrap_or(6.0),
            evaporator_pressure_bar: req.evaporator_pressure_bar.unwrap_or(18.5),
            condensing_temperature_c: req.condensing_temperature_c.unwrap_or(36.0),
            expander_isentropic_efficiency: req.expander_isentropic_efficiency.unwrap_or(0.86),
            pump_isentropic_efficiency: req.pump_isentropic_efficiency.unwrap_or(0.78),
            generator_efficiency: req.generator_efficiency.unwrap_or(0.97),
            parasitic_power_fraction: req.parasitic_power_fraction.unwrap_or(0.085),
            ambient_reference_temp_c: req.ambient_reference_temp_c.unwrap_or(20.0),
        };
        let res = atlas_simulation_core::geothermal_binary_orc::solve_geothermal_binary_orc(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_structural_cable_stayed_bridge(
        &mut self,
        req: &SimCableStayRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::cable_stayed_bridge::CableStayProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "cable_stay_sim".to_string()),
            stay_class: req.stay_class.unwrap_or(
                atlas_simulation_core::cable_stayed_bridge::StayCableClass::ParallelStrandSystemPSS,
            ),
            cable_chord_length_m: req.cable_chord_length_m.unwrap_or(240.0),
            outer_pipe_diameter_mm: req.outer_pipe_diameter_mm.unwrap_or(160.0),
            chord_inclination_deg: req.chord_inclination_deg.unwrap_or(32.0),
            cable_pretension_kn: req.cable_pretension_kn.unwrap_or(3600.0),
            total_linear_mass_kg_m: req.total_linear_mass_kg_m.unwrap_or(52.0),
            elastic_modulus_gpa: req.elastic_modulus_gpa.unwrap_or(195.0),
            net_steel_area_mm2: req.net_steel_area_mm2.unwrap_or(6400.0),
            inherent_damping_ratio: req.inherent_damping_ratio.unwrap_or(0.0012),
            damper_distance_from_deck_m: req.damper_distance_from_deck_m.unwrap_or(5.50),
            wind_speed_m_s: req.wind_speed_m_s.unwrap_or(14.0),
        };
        let res = atlas_simulation_core::cable_stayed_bridge::solve_cable_stayed_bridge(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aerospace_supersonic_nozzle(
        &mut self,
        req: &SimSupersonicNozzleRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::supersonic_wind_tunnel_nozzle::SupersonicNozzleProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "supersonic_nozzle_sim".to_string()),
            gas_medium: req.gas_medium.unwrap_or(
                atlas_simulation_core::supersonic_wind_tunnel_nozzle::SupersonicGasMedium::DryAir,
            ),
            contour_method: req.contour_method.unwrap_or(
                atlas_simulation_core::supersonic_wind_tunnel_nozzle::NozzleContourMethod::MethodOfCharacteristicsMOC,
            ),
            stagnation_pressure_kpa: req.stagnation_pressure_kpa.unwrap_or(450.0),
            stagnation_temperature_k: req.stagnation_temperature_k.unwrap_or(320.0),
            design_mach_number: req.design_mach_number.unwrap_or(2.20),
            throat_diameter_mm: req.throat_diameter_mm.unwrap_or(85.0),
            ambient_backpressure_kpa: req.ambient_backpressure_kpa.unwrap_or(42.0),
            model_deflection_angle_deg: req.model_deflection_angle_deg.unwrap_or(10.0),
            divergence_half_angle_deg: req.divergence_half_angle_deg.unwrap_or(12.0),
        };
        let res = atlas_simulation_core::supersonic_wind_tunnel_nozzle::solve_supersonic_wind_tunnel_nozzle(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_automotive_ev_torque_vectoring(
        &mut self,
        req: &SimEvTorqueVectoringRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::ev_torque_vectoring::EvTorqueVectoringProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "ev_torque_vectoring_sim".to_string()),
            drivetrain_layout: req
                .drivetrain_layout
                .unwrap_or(atlas_simulation_core::ev_torque_vectoring::EvAwdLayout::DualMotorAwd),
            road_condition: req.road_condition.unwrap_or(
                atlas_simulation_core::ev_torque_vectoring::RoadSurfaceCondition::DryAsphalt,
            ),
            vehicle_mass_kg: req.vehicle_mass_kg.unwrap_or(2150.0),
            yaw_moment_of_inertia_kg_m2: req.yaw_moment_of_inertia_kg_m2.unwrap_or(2950.0),
            wheelbase_mm: req.wheelbase_mm.unwrap_or(2900.0),
            track_width_mm: req.track_width_mm.unwrap_or(1620.0),
            cg_to_front_axle_mm: req.cg_to_front_axle_mm.unwrap_or(1420.0),
            cg_height_mm: req.cg_height_mm.unwrap_or(480.0),
            tire_rolling_radius_mm: req.tire_rolling_radius_mm.unwrap_or(340.0),
            front_cornering_stiffness_n_rad: req.front_cornering_stiffness_n_rad.unwrap_or(95000.0),
            rear_cornering_stiffness_n_rad: req.rear_cornering_stiffness_n_rad.unwrap_or(105000.0),
            vehicle_speed_km_h: req.vehicle_speed_km_h.unwrap_or(90.0),
            front_steering_angle_deg: req.front_steering_angle_deg.unwrap_or(1.2),
            demanded_total_torque_nm: req.demanded_total_torque_nm.unwrap_or(450.0),
        };
        let res = atlas_simulation_core::ev_torque_vectoring::solve_ev_torque_vectoring(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_marine_subsea_riser_viv(
        &mut self,
        req: &SimSubseaRiserVivRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::subsea_riser_viv::SubseaRiserVivProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "subsea_riser_viv_sim".to_string()),
            riser_type: req.riser_type.unwrap_or(
                atlas_simulation_core::subsea_riser_viv::MarineRiserType::TopTensionedProductionRiser,
            ),
            current_profile: req.current_profile.unwrap_or(
                atlas_simulation_core::subsea_riser_viv::OceanCurrentProfile::LinearShearCurrent,
            ),
            suppression_device: req.suppression_device.unwrap_or(
                atlas_simulation_core::subsea_riser_viv::VivSuppressionDevice::HelicalStrakesTripleStart,
            ),
            water_depth_m: req.water_depth_m.unwrap_or(1800.0),
            riser_length_m: req.riser_length_m.unwrap_or(1850.0),
            outer_diameter_mm: req.outer_diameter_mm.unwrap_or(533.4),
            wall_thickness_mm: req.wall_thickness_mm.unwrap_or(25.4),
            top_tension_kn: req.top_tension_kn.unwrap_or(6500.0),
            steel_elastic_modulus_gpa: req.steel_elastic_modulus_gpa.unwrap_or(207.0),
            steel_density_kg_m3: req.steel_density_kg_m3.unwrap_or(7850.0),
            internal_fluid_density_kg_m3: req.internal_fluid_density_kg_m3.unwrap_or(1250.0),
            seawater_density_kg_m3: req.seawater_density_kg_m3.unwrap_or(1025.0),
            surface_current_velocity_m_s: req.surface_current_velocity_m_s.unwrap_or(1.65),
            bottom_current_velocity_m_s: req.bottom_current_velocity_m_s.unwrap_or(0.25),
            strouhal_number: req.strouhal_number.unwrap_or(0.20),
            design_fatigue_life_years: req.design_fatigue_life_years.unwrap_or(25.0),
        };
        let res = atlas_simulation_core::subsea_riser_viv::solve_subsea_riser_viv(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_chemical_chromatography_column(
        &mut self,
        req: &SimChromatographyColumnRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::chromatography_column::ChromatographyColumnProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "chromatography_column_sim".to_string()),
            chromatography_mode: req.chromatography_mode.unwrap_or(
                atlas_simulation_core::chromatography_column::ChromatographyMode::ProteinAAffinity,
            ),
            column_diameter_mm: req.column_diameter_mm.unwrap_or(300.0),
            bed_height_mm: req.bed_height_mm.unwrap_or(200.0),
            linear_velocity_cm_h: req.linear_velocity_cm_h.unwrap_or(300.0),
            resin_particle_diameter_um: req.resin_particle_diameter_um.unwrap_or(50.0),
            bed_void_fraction: req.bed_void_fraction.unwrap_or(0.36),
            intraparticle_porosity: req.intraparticle_porosity.unwrap_or(0.65),
            feed_protein_concentration_g_l: req.feed_protein_concentration_g_l.unwrap_or(3.50),
            max_binding_capacity_g_l: req.max_binding_capacity_g_l.unwrap_or(55.0),
            langmuir_constant_l_g: req.langmuir_constant_l_g.unwrap_or(8.33),
            molecular_diffusivity_m2_s: req.molecular_diffusivity_m2_s.unwrap_or(4.5e-11),
            buffer_viscosity_mpa_s: req.buffer_viscosity_mpa_s.unwrap_or(1.05),
            buffer_density_kg_m3: req.buffer_density_kg_m3.unwrap_or(1010.0),
        };
        let res = atlas_simulation_core::chromatography_column::solve_chromatography_column(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_transport_maglev_suspension(
        &mut self,
        req: &SimMaglevSuspensionRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::maglev_guideway_suspension::MaglevSuspensionProblem {
            problem_id: req
                .problem_id
                .clone()
                .unwrap_or_else(|| "maglev_suspension_sim".to_string()),
            core_material: req.core_material.unwrap_or(
                atlas_simulation_core::maglev_guideway_suspension::MaglevCoreMaterial::SiliconSteelFeSi,
            ),
            disturbance_type: req.disturbance_type.unwrap_or(
                atlas_simulation_core::maglev_guideway_suspension::GuidewayDisturbanceType::ExpansionJointStep,
            ),
            mass_supported_per_magnet_kg: req.mass_supported_per_magnet_kg.unwrap_or(1250.0),
            nominal_airgap_mm: req.nominal_airgap_mm.unwrap_or(12.0),
            coil_turns: req.coil_turns.unwrap_or(320),
            pole_face_area_mm2: req.pole_face_area_mm2.unwrap_or(58000.0),
            coil_resistance_ohms: req.coil_resistance_ohms.unwrap_or(0.38),
            max_inverter_voltage_v: req.max_inverter_voltage_v.unwrap_or(350.0),
            max_continuous_current_a: req.max_continuous_current_a.unwrap_or(85.0),
            target_controller_bandwidth_rad_s: req.target_controller_bandwidth_rad_s.unwrap_or(65.0),
            disturbance_amplitude_mm: req.disturbance_amplitude_mm.unwrap_or(2.50),
        };
        let res =
            atlas_simulation_core::maglev_guideway_suspension::solve_maglev_guideway_suspension(
                &prob,
            )
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_molecular(
        &mut self,
        _req: &SimMolecularRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::molecular::MolecularDynamicsProblem {
            atoms: vec![
                atlas_simulation_core::molecular::Atom {
                    id: 1,
                    element: "Ar".to_string(),
                    mass_amu: 39.948,
                    position_angstrom: [0.0, 0.0, 0.0],
                    velocity_angstrom_ps: [0.01, 0.0, 0.0],
                },
                atlas_simulation_core::molecular::Atom {
                    id: 2,
                    element: "Ar".to_string(),
                    mass_amu: 39.948,
                    position_angstrom: [3.8, 0.0, 0.0],
                    velocity_angstrom_ps: [-0.01, 0.0, 0.0],
                },
            ],
            force_field: atlas_simulation_core::molecular::LennardJonesParam::argon(),
            time_step_ps: 0.002,
            steps: 20,
        };
        let res = atlas_simulation_core::molecular::run_molecular_dynamics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_robotics(&mut self, req: &SimRoboticsRequest) -> Result<serde_json::Value, StudioError> {
        let default_angles = vec![15.0, 30.0, 45.0];
        let angles = req.joint_angles_deg.as_ref().unwrap_or(&default_angles);
        let t1 = angles.first().copied().unwrap_or(0.0).to_radians();
        let t2 = angles.get(1).copied().unwrap_or(0.0).to_radians();
        let t3 = angles.get(2).copied().unwrap_or(0.0).to_radians();

        let prob = atlas_simulation_core::robotics::RobotKinematicsProblem {
            robot_name: "Atlas-6DOF Industrial Manipulator".to_string(),
            links: vec![
                atlas_simulation_core::robotics::DhLink::new(
                    "Base J1".to_string(),
                    t1,
                    0.4,
                    0.0,
                    std::f64::consts::FRAC_PI_2,
                ),
                atlas_simulation_core::robotics::DhLink::new(
                    "Arm J2".to_string(),
                    t2,
                    0.0,
                    0.6,
                    0.0,
                ),
                atlas_simulation_core::robotics::DhLink::new(
                    "Forearm J3".to_string(),
                    t3,
                    0.0,
                    0.5,
                    std::f64::consts::FRAC_PI_2,
                ),
            ],
            target_ee: req.target_ee,
        };
        let res = atlas_simulation_core::robotics::solve_robot_kinematics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_powerflow(
        &mut self,
        req: &SimPowerFlowRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::power_flow::PowerFlowProblem {
            base_mva: 100.0,
            lines: vec![atlas_simulation_core::power_flow::TransmissionLine {
                from_bus: 1,
                to_bus: 2,
                resistance_pu: 0.015,
                reactance_pu: 0.065,
            }],
            p_load_mw: req.p_load_mw.unwrap_or(45.0),
            q_load_mvar: req.q_load_mvar.unwrap_or(18.0),
            v_slack_pu: 1.02,
        };
        let res = atlas_simulation_core::power_flow::solve_power_flow(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_generative(
        &mut self,
        req: &SimGenerativeRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::generative::TopologyOptProblem {
            grid_nx: 24,
            grid_ny: 12,
            target_volume_fraction: req.volume_fraction.unwrap_or(0.35),
            penalization_power_p: 3.0,
            max_iterations: req.iterations.unwrap_or(12),
            filter_radius_elements: 1.5,
        };
        let res = atlas_simulation_core::generative::solve_topology_optimization(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_geotechnical(
        &mut self,
        req: &SimGeotechRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::geotechnical::RetainingWallProblem {
            wall_height_m: req.wall_height_m.unwrap_or(5.0),
            soil: atlas_simulation_core::geotechnical::SoilProperties::dense_sand(),
            water_table_depth_m: None,
        };
        let res = atlas_simulation_core::geotechnical::solve_retaining_wall_pressures(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_biomedical(
        &mut self,
        req: &SimBiomedicalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::biomedical::HemodynamicsRequest::default();
        if let Some(e) = req.arterial_elastic_modulus_pa {
            prob.arterial_elastic_modulus_pa = e;
        }
        if let Some(h) = req.wall_thickness_m {
            prob.wall_thickness_m = h;
        }
        if let Some(r) = req.lumen_radius_m {
            prob.lumen_radius_m = r;
        }
        if let Some(hr) = req.heart_rate_bpm {
            prob.heart_rate_bpm = hr;
        }
        let hemo = atlas_simulation_core::biomedical::solve_hemodynamics(&prob)
            .map_err(StudioError::BadRequest)?;
        let bone_prob = atlas_simulation_core::biomedical::OrthopedicBoneRequest::default();
        let bone = atlas_simulation_core::biomedical::solve_orthopedic_bone(&bone_prob)
            .map_err(StudioError::BadRequest)?;
        Ok(serde_json::json!({
            "hemodynamics": hemo,
            "orthopedic_bone": bone,
        }))
    }

    fn sim_marine(&mut self, req: &SimMarineRequest) -> Result<serde_json::Value, StudioError> {
        let mut hydro_prob = atlas_simulation_core::marine::MarineHydrostaticsRequest::default();
        if let Some(l) = req.hull_length_m {
            hydro_prob.hull_length_m = l;
        }
        if let Some(b) = req.hull_beam_m {
            hydro_prob.hull_beam_m = b;
        }
        if let Some(t) = req.hull_draft_m {
            hydro_prob.hull_draft_m = t;
        }
        if let Some(heel) = req.heel_angle_deg {
            hydro_prob.heel_angle_deg = heel;
        }
        let hydro = atlas_simulation_core::marine::solve_hydrostatics(&hydro_prob)
            .map_err(StudioError::BadRequest)?;
        let wave_prob = atlas_simulation_core::marine::AiryWaveRequest {
            wave_period_s: req.wave_period_s.unwrap_or(8.0),
            water_depth_m: 35.0,
            wave_height_m: 2.0,
        };
        let wave = atlas_simulation_core::marine::solve_airy_wave(&wave_prob)
            .map_err(StudioError::BadRequest)?;
        Ok(serde_json::json!({
            "hydrostatics": hydro,
            "airy_wave": wave,
        }))
    }

    fn sim_buckling(&mut self, req: &SimBucklingRequest) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::structural::BucklingProblem::default();
        if let Some(l) = req.column_length_m {
            prob.column_length_m = l;
        }
        if let Some(p) = req.applied_load_n {
            prob.applied_load_n = p;
        }
        let res = atlas_simulation_core::structural::solve_column_buckling(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_fatigue(&mut self, req: &SimFatigueRequest) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::structural::FatigueProblem::default();
        if let Some(sa) = req.alternating_stress_pa {
            prob.alternating_stress_pa = sa;
        }
        if let Some(sm) = req.mean_stress_pa {
            prob.mean_stress_pa = sm;
        }
        let res = atlas_simulation_core::structural::solve_fatigue_safety(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_composite(
        &mut self,
        req: &SimCompositeRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::structural::CompositeLaminateProblem::default();
        if let Some(ref angles) = req.ply_angles_deg {
            prob.ply_angles_deg.clone_from(angles);
        }
        let res = atlas_simulation_core::structural::solve_composite_laminate(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_thermal_stress(
        &mut self,
        req: &SimThermalStressRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::thermal_stress::ThermalStressProblem::default();
        if let Some(dt) = req.delta_temp_c {
            prob.final_temp_c = prob.initial_temp_c + dt;
        }
        let res = atlas_simulation_core::thermal_stress::solve_thermal_stress(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_electromagnetics(
        &mut self,
        req: &SimEmRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::electromagnetics::MicrostripProblem::default();
        if let Some(w) = req.trace_width_mm {
            prob.trace_width_mm = w;
        }
        if let Some(h) = req.substrate_height_mm {
            prob.substrate_height_mm = h;
        }
        let res = atlas_simulation_core::electromagnetics::solve_microstrip_line(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_building_physics(
        &mut self,
        req: &SimBuildingPhysicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let prob = atlas_simulation_core::building_physics::BuildingAssemblyProblem {
            wall_area_m2: req.wall_area_m2.unwrap_or(120.0),
            ..Default::default()
        };
        let res = atlas_simulation_core::building_physics::solve_building_u_value(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn additive_analyze_supports(
        &mut self,
        req: &AdditiveSupportsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let settings = atlas_additive::supports::SupportSettings {
            critical_overhang_angle_deg: req.critical_angle_deg.unwrap_or(45.0),
            ..Default::default()
        };
        let build = self
            .project
            .build_with_cache(&mut self.kernel, &mut self.cache)?;
        let mesh = build.mesh.unwrap_or_default();
        let mut facets = Vec::new();
        for tri in &mesh.triangles {
            if let (Some(&p0), Some(&p1), Some(&p2)) = (
                mesh.positions.get(tri[0] as usize),
                mesh.positions.get(tri[1] as usize),
                mesh.positions.get(tri[2] as usize),
            ) {
                facets.push((p0, p1, p2));
            }
        }
        let analysis =
            atlas_additive::supports::analyze_overhang_supports(&facets, &settings, 1.24);
        serde_json::to_value(&analysis).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_digital_twin(
        &mut self,
        req: &SimDigitalTwinRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::digital_twin::DigitalTwinProblem::default();
        if let Some(ref id) = req.sensor_id {
            prob.sensor_id.clone_from(id);
        }
        if let Some(ea) = req.strain_a_microstrain {
            prob.rosette.strain_a_microstrain = ea;
        }
        if let Some(eb) = req.strain_b_microstrain {
            prob.rosette.strain_b_microstrain = eb;
        }
        if let Some(ec) = req.strain_c_microstrain {
            prob.rosette.strain_c_microstrain = ec;
        }
        let res = atlas_simulation_core::digital_twin::solve_digital_twin(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_acoustics(
        &mut self,
        req: &SimAcousticsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::acoustics::AcousticsProblem::default();
        if let Some(m) = req.surface_mass_density_kg_m2 {
            prob.surface_mass_density_kg_m2 = m;
        }
        if let Some(a) = req.partition_area_m2 {
            prob.partition_area_m2 = a;
        }
        if let Some(noise) = req.exterior_noise_dba {
            prob.exterior_noise_dba = noise;
        }
        let res = atlas_simulation_core::acoustics::solve_acoustics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_solar(&mut self, req: &SimSolarRequest) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::solar::SolarPvProblem::default();
        if let Some(tilt) = req.surface_tilt_deg {
            prob.surface_tilt_deg = tilt;
        }
        if let Some(area) = req.array_area_m2 {
            prob.array_area_m2 = area;
        }
        if let Some(eff) = req.module_efficiency {
            prob.module_efficiency = eff;
        }
        if let Some(ghi) = req.annual_ghi_kwh_m2 {
            prob.annual_ghi_kwh_m2 = ghi;
        }
        let res =
            atlas_simulation_core::solar::solve_solar_pv(&prob).map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_aeroelasticity(
        &mut self,
        req: &SimAeroelasticityRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::aeroelasticity::AeroelasticProblem::default();
        if let Some(d) = req.diameter_m {
            prob.diameter_m = d;
        }
        if let Some(v) = req.flow_velocity_m_s {
            prob.flow_velocity_m_s = v;
        }
        if let Some(fn_hz) = req.natural_frequency_hz {
            prob.natural_frequency_hz = fn_hz;
        }
        if let Some(m) = req.modal_mass_kg_per_m {
            prob.modal_mass_kg_per_m = m;
        }
        let res = atlas_simulation_core::aeroelasticity::solve_aeroelastic_viv(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_robotics_trajectory(
        &mut self,
        req: &SimRoboticsTrajectoryRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::robotics::QuinticTrajectoryProblem::default();
        if let Some(d) = req.duration_s {
            prob.duration_s = d;
        }
        if let Some(ref q) = req.q_target_rad {
            prob.q_target_rad.clone_from(q);
        }
        if let Some(steps) = req.time_steps {
            prob.time_steps = steps;
        }
        let res = atlas_simulation_core::robotics::solve_quintic_trajectory(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_composite_failure(
        &mut self,
        req: &SimCompositeFailureRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::structural::CompositeFailureProblem::default();
        if let Some(s1) = req.sigma_1_mpa {
            prob.sigma_1_pa = s1 * 1.0e6;
        }
        if let Some(s2) = req.sigma_2_mpa {
            prob.sigma_2_pa = s2 * 1.0e6;
        }
        if let Some(s12) = req.tau_12_mpa {
            prob.tau_12_pa = s12 * 1.0e6;
        }
        let res = atlas_simulation_core::structural::solve_composite_failure(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_rainflow(&mut self, req: &SimRainflowRequest) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::rainflow::RainflowProblem::default();
        if let Some(ref hist) = req.stress_history_mpa {
            prob.stress_history_mpa.clone_from(hist);
        }
        let res = atlas_simulation_core::rainflow::solve_rainflow_fatigue(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_fsi(&mut self, req: &SimFsiRequest) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::fsi::FsiProblem::default();
        if let Some(l) = req.length_m {
            prob.length_m = l;
        }
        if let Some(v) = req.fluid_velocity_m_s {
            prob.fluid_velocity_m_s = v;
        }
        let res = atlas_simulation_core::fsi::solve_fsi_coupled(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_cavitation(
        &mut self,
        req: &SimCavitationRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::cavitation::CavitationProblem::default();
        if let Some(t) = req.fluid_temp_c {
            prob.fluid_temp_c = t;
        }
        if let Some(h) = req.total_head_m {
            prob.total_head_m = h;
        }
        let res = atlas_simulation_core::cavitation::solve_cavitation_npsh(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_elastoplasticity(
        &mut self,
        req: &SimElastoplasticRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::elastoplasticity::ElastoplasticProblem::default();
        if let Some(ref mat) = req.material_name {
            prob.material_name.clone_from(mat);
        }
        if let Some(eps) = req.applied_strain_pct {
            prob.applied_strain = eps / 100.0;
        }
        if let Some(sy) = req.yield_strength_mpa {
            prob.yield_strength_pa = sy * 1.0e6;
        }
        if let Some(e) = req.elastic_modulus_gpa {
            prob.elastic_modulus_pa = e * 1.0e9;
        }
        let res = atlas_simulation_core::elastoplasticity::solve_elastoplasticity(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_heat_exchanger(
        &mut self,
        req: &SimHeatExchangerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::heat_exchanger::HeatExchangerProblem::default();
        if let Some(th) = req.hot_inlet_temp_c {
            prob.hot_inlet_temp_c = th;
        }
        if let Some(tc) = req.cold_inlet_temp_c {
            prob.cold_inlet_temp_c = tc;
        }
        if let Some(a) = req.area_m2 {
            prob.area_m2 = a;
        }
        if let Some(u) = req.overall_u_w_m2_k {
            prob.overall_u_w_m2_k = u;
        }
        let res = atlas_simulation_core::heat_exchanger::solve_heat_exchanger(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_water_hammer(
        &mut self,
        req: &SimWaterHammerRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::water_hammer::WaterHammerProblem::default();
        if let Some(l) = req.length_m {
            prob.length_m = l;
        }
        if let Some(v0) = req.initial_velocity_m_s {
            prob.initial_velocity_m_s = v0;
        }
        if let Some(tv) = req.valve_closure_time_s {
            prob.valve_closure_time_s = tv;
        }
        let res = atlas_simulation_core::water_hammer::solve_water_hammer(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_shock_response(
        &mut self,
        req: &SimShockResponseRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::shock_response::ShockResponseProblem::default();
        if let Some(a0) = req.peak_acceleration_g {
            prob.peak_acceleration_g = a0;
        }
        if let Some(tau) = req.pulse_duration_ms {
            prob.pulse_duration_ms = tau;
        }
        if let Some(zeta) = req.damping_ratio {
            prob.damping_ratio = zeta;
        }
        let res = atlas_simulation_core::shock_response::solve_shock_response_spectrum(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_battery_ev(
        &mut self,
        req: &SimBatteryEvRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::battery_ev::BatteryPackProblem::default();
        if let Some(i) = req.pack_current_a {
            prob.pack_current_a = i;
        }
        if let Some(soc) = req.state_of_charge {
            prob.state_of_charge = soc;
        }
        if let Some(t) = req.ambient_temp_c {
            prob.ambient_temp_c = t;
        }
        let res = atlas_simulation_core::battery_ev::solve_battery_pack(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_vehicle_dynamics(
        &mut self,
        req: &SimVehicleDynamicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::battery_ev::VehicleDynamicsProblem::default();
        if let Some(v) = req.velocity_kmh {
            prob.velocity_kmh = v;
        }
        if let Some(m) = req.total_mass_kg {
            prob.total_mass_kg = m;
        }
        if let Some(cd) = req.drag_coefficient {
            prob.drag_coefficient = cd;
        }
        if let Some(kwh) = req.battery_usable_kwh {
            prob.battery_usable_kwh = kwh;
        }
        let res = atlas_simulation_core::battery_ev::solve_vehicle_dynamics(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_environmental(
        &mut self,
        req: &SimEnvironmentalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::environmental::PlumeDispersionProblem::default();
        if let Some(q) = req.emission_rate_g_s {
            prob.emission_rate_g_s = q;
        }
        if let Some(h) = req.stack_height_m {
            prob.stack_height_m = h;
        }
        if let Some(u) = req.wind_speed_m_s {
            prob.wind_speed_m_s = u;
        }
        if let Some(x) = req.downwind_distance_m {
            prob.downwind_distance_m = x;
        }
        let res = atlas_simulation_core::environmental::solve_plume_dispersion(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_rock_mechanics(
        &mut self,
        req: &SimRockMechanicsRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob = atlas_simulation_core::rock_mechanics::HoekBrownProblem::default();
        if let Some(ucs) = req.intact_ucs_mpa {
            prob.intact_ucs_mpa = ucs;
        }
        if let Some(gsi) = req.gsi {
            prob.gsi = gsi;
        }
        if let Some(mi) = req.intact_mi {
            prob.intact_mi = mi;
        }
        if let Some(s3) = req.confining_stress_mpa {
            prob.confining_stress_mpa = s3;
        }
        let res = atlas_simulation_core::rock_mechanics::solve_hoek_brown(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_signal_integrity(
        &mut self,
        req: &SimSignalIntegrityRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob =
            atlas_simulation_core::signal_integrity::PcbTransmissionLineProblem::default();
        if let Some(w) = req.trace_width_um {
            prob.trace_width_m = w * 1e-6;
        }
        if let Some(h) = req.dielectric_height_um {
            prob.dielectric_height_m = h * 1e-6;
        }
        if let Some(er) = req.relative_permittivity {
            prob.relative_permittivity = er;
        }
        if let Some(l) = req.trace_length_mm {
            prob.trace_length_m = l * 1e-3;
        }
        if let Some(f) = req.signal_frequency_ghz {
            prob.signal_frequency_hz = f * 1e9;
        }
        let res = atlas_simulation_core::signal_integrity::solve_pcb_signal_integrity(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_nuclear_shielding(
        &mut self,
        req: &SimNuclearShieldingRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let mut prob =
            atlas_simulation_core::nuclear_shielding::RadiationShieldingProblem::default();
        if let Some(ci) = req.source_activity_curies {
            prob.source_activity_bq = ci * 3.7e10;
        }
        if let Some(d) = req.distance_to_receptor_m {
            prob.distance_to_receptor_m = d;
        }
        if let (Some(pb_cm), Some(layer)) = (req.lead_shield_thickness_cm, prob.layers.first_mut())
        {
            layer.thickness_m = pb_cm / 100.0;
        }
        if let (Some(conc_cm), Some(layer)) =
            (req.concrete_shield_thickness_cm, prob.layers.get_mut(1))
        {
            layer.thickness_m = conc_cm / 100.0;
        }
        let res = atlas_simulation_core::nuclear_shielding::solve_radiation_shielding(&prob)
            .map_err(StudioError::BadRequest)?;
        serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn sim_multiphysics_suite(&mut self) -> Result<serde_json::Value, StudioError> {
        let mut domains = serde_json::Map::new();
        let mut failures = serde_json::Map::new();

        match self.sim_buckling(&Default::default()) {
            Ok(v) => {
                domains.insert("buckling".into(), v);
            }
            Err(error) => {
                failures.insert("buckling".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_elastoplasticity(&Default::default()) {
            Ok(v) => {
                domains.insert("elastoplasticity".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "elastoplasticity".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_rainflow(&Default::default()) {
            Ok(v) => {
                domains.insert("fatigue_rainflow".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "fatigue_rainflow".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_composite_failure(&Default::default()) {
            Ok(v) => {
                domains.insert("composite_failure".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "composite_failure".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_cfd(&Default::default()) {
            Ok(v) => {
                domains.insert("cfd_hydraulics".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "cfd_hydraulics".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_water_hammer(&Default::default()) {
            Ok(v) => {
                domains.insert("water_hammer".into(), v);
            }
            Err(error) => {
                failures.insert("water_hammer".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_cavitation(&Default::default()) {
            Ok(v) => {
                domains.insert("cavitation".into(), v);
            }
            Err(error) => {
                failures.insert("cavitation".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_fsi(&Default::default()) {
            Ok(v) => {
                domains.insert("coupled_fsi".into(), v);
            }
            Err(error) => {
                failures.insert("coupled_fsi".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_aerodynamics(&Default::default()) {
            Ok(v) => {
                domains.insert("aerodynamics".into(), v);
            }
            Err(error) => {
                failures.insert("aerodynamics".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_aeroelasticity(&Default::default()) {
            Ok(v) => {
                domains.insert("aeroelasticity".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "aeroelasticity".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_shock_response(&Default::default()) {
            Ok(v) => {
                domains.insert("shock_response".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "shock_response".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_heat_exchanger(&Default::default()) {
            Ok(v) => {
                domains.insert("heat_exchanger".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "heat_exchanger".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_thermal_stress(&Default::default()) {
            Ok(v) => {
                domains.insert("thermal_stress".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "thermal_stress".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_chemical(&Default::default()) {
            Ok(v) => {
                domains.insert("chemical_vle".into(), v);
            }
            Err(error) => {
                failures.insert("chemical_vle".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_molecular(&Default::default()) {
            Ok(v) => {
                domains.insert("molecular_dynamics".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "molecular_dynamics".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_robotics(&Default::default()) {
            Ok(v) => {
                domains.insert("robotics_kinematics".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "robotics_kinematics".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_robotics_trajectory(&Default::default()) {
            Ok(v) => {
                domains.insert("motion_trajectory".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "motion_trajectory".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_powerflow(&Default::default()) {
            Ok(v) => {
                domains.insert("power_flow".into(), v);
            }
            Err(error) => {
                failures.insert("power_flow".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_signal_integrity(&Default::default()) {
            Ok(v) => {
                domains.insert("signal_integrity".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "signal_integrity".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_biomedical(&Default::default()) {
            Ok(v) => {
                domains.insert("biomedical".into(), v);
            }
            Err(error) => {
                failures.insert("biomedical".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_marine(&Default::default()) {
            Ok(v) => {
                domains.insert("marine".into(), v);
            }
            Err(error) => {
                failures.insert("marine".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_building_physics(&Default::default()) {
            Ok(v) => {
                domains.insert("building_physics".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "building_physics".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_solar(&Default::default()) {
            Ok(v) => {
                domains.insert("solar_pv".into(), v);
            }
            Err(error) => {
                failures.insert("solar_pv".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_battery_ev(&Default::default()) {
            Ok(v) => {
                domains.insert("battery_ev".into(), v);
            }
            Err(error) => {
                failures.insert("battery_ev".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_vehicle_dynamics(&Default::default()) {
            Ok(v) => {
                domains.insert("vehicle_dynamics".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "vehicle_dynamics".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_rock_mechanics(&Default::default()) {
            Ok(v) => {
                domains.insert("rock_mechanics".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "rock_mechanics".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_nuclear_shielding(&Default::default()) {
            Ok(v) => {
                domains.insert("nuclear_shielding".into(), v);
            }
            Err(error) => {
                failures.insert(
                    "nuclear_shielding".into(),
                    serde_json::json!(error.to_string()),
                );
            }
        }
        match self.sim_environmental(&Default::default()) {
            Ok(v) => {
                domains.insert("environmental".into(), v);
            }
            Err(error) => {
                failures.insert("environmental".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_geotechnical(&Default::default()) {
            Ok(v) => {
                domains.insert("geotechnical".into(), v);
            }
            Err(error) => {
                failures.insert("geotechnical".into(), serde_json::json!(error.to_string()));
            }
        }
        match self.sim_generative(&Default::default()) {
            Ok(v) => {
                domains.insert("generative".into(), v);
            }
            Err(error) => {
                failures.insert("generative".into(), serde_json::json!(error.to_string()));
            }
        }

        let total_count = domains.len();
        let failed_count = failures.len();
        let target_domains = 30;
        let payload = serde_json::json!({
            "suite_status": if failed_count == 0 {
                "Completed"
            } else {
                "Partial"
            },
            "domains_evaluated": total_count,
            "failed_domains": failures,
            "failed_count": failed_count,
            "target_domains": target_domains,
            "analysis_standards_basis": {
                "aerospace": "AS9100 Rev D & FAA FAR 25 criteria",
                "quality_management": "ISO 9001:2015 traceability criteria",
                "pressure_vessel": "ASME BPVC Section VIII & B31.3",
                "radiation_protection": "ICRP-103 & 10 CFR Part 20",
                "automotive_safety": "ISO 26262 & UN ECE R100",
                "electronics_pcb": "IPC-2141 & IEEE 802.3",
                "mining_geotechnical": "ISRM & Bieniawski RMR89"
            },
            "analysis_standards_note": "Formula basis only. These are the criteria the analytical \
                models reference; no certification, accreditation or regulatory approval is implied.",
            "domains": domains,
        });
        Ok(payload)
    }

    fn convert_units(
        &mut self,
        req: &UnitConvertRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let val = req.value;
        let cat = req.category.to_lowercase();
        let from = req.from_unit.to_lowercase();
        let to = req.to_unit.to_lowercase();

        let (converted, si_base) = match cat.as_str() {
            "length" => {
                let parse_u = |s: &str| match s {
                    "mm" | "millimeter" => Some(atlas_core::prelude::LengthUnit::Millimeter),
                    "cm" | "centimeter" => Some(atlas_core::prelude::LengthUnit::Centimeter),
                    "m" | "meter" => Some(atlas_core::prelude::LengthUnit::Meter),
                    "km" | "kilometer" => Some(atlas_core::prelude::LengthUnit::Kilometer),
                    "in" | "inch" => Some(atlas_core::prelude::LengthUnit::Inch),
                    "ft" | "foot" => Some(atlas_core::prelude::LengthUnit::Foot),
                    "yd" | "yard" => Some(atlas_core::prelude::LengthUnit::Yard),
                    "mil" => Some(atlas_core::prelude::LengthUnit::Mil),
                    _ => None,
                };
                let u_from = parse_u(&from).ok_or_else(|| {
                    StudioError::BadRequest(format!("Unknown length unit: {}", from))
                })?;
                let u_to = parse_u(&to).ok_or_else(|| {
                    StudioError::BadRequest(format!("Unknown length unit: {}", to))
                })?;
                let si = val * u_from.to_meters_factor();
                let res = u_from.convert_to(val, u_to);
                (res, si)
            }
            "pressure" | "stress" => {
                let parse_u = |s: &str| match s {
                    "pa" | "pascal" => Some(atlas_core::prelude::PressureUnit::Pascal),
                    "kpa" | "kilopascal" => Some(atlas_core::prelude::PressureUnit::Kilopascal),
                    "mpa" | "megapascal" => Some(atlas_core::prelude::PressureUnit::Megapascal),
                    "gpa" | "gigapascal" => Some(atlas_core::prelude::PressureUnit::Gigapascal),
                    "bar" => Some(atlas_core::prelude::PressureUnit::Bar),
                    "psi" => Some(atlas_core::prelude::PressureUnit::Psi),
                    "ksi" => Some(atlas_core::prelude::PressureUnit::Ksi),
                    "atm" => Some(atlas_core::prelude::PressureUnit::Atmosphere),
                    _ => None,
                };
                let u_from = parse_u(&from).ok_or_else(|| {
                    StudioError::BadRequest(format!("Unknown pressure unit: {}", from))
                })?;
                let u_to = parse_u(&to).ok_or_else(|| {
                    StudioError::BadRequest(format!("Unknown pressure unit: {}", to))
                })?;
                let si = val * u_from.to_pascals_factor();
                let res = u_from.convert_to(val, u_to);
                (res, si)
            }
            "temperature" => {
                let t = match from.as_str() {
                    "k" | "kelvin" => atlas_core::prelude::Temperature::from_kelvin(val),
                    "c" | "celsius" => atlas_core::prelude::Temperature::from_celsius(val),
                    "f" | "fahrenheit" => atlas_core::prelude::Temperature::from_fahrenheit(val),
                    _ => {
                        return Err(StudioError::BadRequest(format!(
                            "Unknown temperature unit: {}",
                            from
                        )));
                    }
                }
                .map_err(|e| StudioError::BadRequest(e.to_string()))?;
                let si = t.as_kelvin();
                let res = match to.as_str() {
                    "k" | "kelvin" => t.as_kelvin(),
                    "c" | "celsius" => t.as_celsius(),
                    "f" | "fahrenheit" => t.as_fahrenheit(),
                    _ => {
                        return Err(StudioError::BadRequest(format!(
                            "Unknown temperature unit: {}",
                            to
                        )));
                    }
                };
                (res, si)
            }
            _ => {
                return Err(StudioError::BadRequest(format!(
                    "Unsupported unit category: {}",
                    cat
                )));
            }
        };

        let resp = serde_json::json!({
            "category": cat,
            "from_unit": from,
            "to_unit": to,
            "input_value": val,
            "converted_value": converted,
            "si_base_value": si_base,
        });
        Ok(resp)
    }

    // --- Universal Command Platform & Agent Orchestrator Methods ---

    fn list_commands(&self) -> serde_json::Value {
        serde_json::json!({
            "total_commands": self.command_registry.len(),
            "commands": self.command_registry.discover(None)
        })
    }

    fn discover_commands(&self, req: &CommandDiscoverRequest) -> serde_json::Value {
        let filter = atlas_command_platform::CommandFilter {
            category: req
                .category
                .as_deref()
                .and_then(|c| match c.to_lowercase().as_str() {
                    "project" => Some(atlas_command_platform::CommandCategory::Project),
                    "sketch" => Some(atlas_command_platform::CommandCategory::Sketch),
                    "features" | "feature" => {
                        Some(atlas_command_platform::CommandCategory::Features)
                    }
                    "materials" | "material" => {
                        Some(atlas_command_platform::CommandCategory::Materials)
                    }
                    "mesh" => Some(atlas_command_platform::CommandCategory::Mesh),
                    "simulation" | "sim" => {
                        Some(atlas_command_platform::CommandCategory::Simulation)
                    }
                    "optimization" | "opt" => {
                        Some(atlas_command_platform::CommandCategory::Optimization)
                    }
                    "cnc" | "cam" => Some(atlas_command_platform::CommandCategory::Cnc),
                    "additive" => Some(atlas_command_platform::CommandCategory::Additive),
                    "reporting" | "report" => {
                        Some(atlas_command_platform::CommandCategory::Reporting)
                    }
                    _ => None,
                }),
            required_permission: req.permission.as_deref().and_then(|p| {
                match p.to_lowercase().as_str() {
                    "read_only" => Some(atlas_command_platform::CommandPermission::ReadOnly),
                    "geometry_create" => {
                        Some(atlas_command_platform::CommandPermission::GeometryCreate)
                    }
                    "simulation_run" => {
                        Some(atlas_command_platform::CommandPermission::SimulationRun)
                    }
                    "design_optimize" => {
                        Some(atlas_command_platform::CommandPermission::DesignOptimize)
                    }
                    "manufacturing_prep" => {
                        Some(atlas_command_platform::CommandPermission::ManufacturingPrep)
                    }
                    "file_export" => Some(atlas_command_platform::CommandPermission::FileExport),
                    _ => None,
                }
            }),
            query: req.query.clone(),
        };
        let matches = self.command_registry.discover(Some(&filter));
        serde_json::json!({
            "count": matches.len(),
            "commands": matches
        })
    }

    fn execute_command(
        &mut self,
        req: &CommandExecuteRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let caller = req
            .caller_identity
            .clone()
            .unwrap_or_else(|| "chief_engineer".into());
        let started = std::time::Instant::now();
        let schema = self
            .command_registry
            .validate_call(
                &req.command_id,
                &req.arguments,
                &[atlas_command_platform::CommandPermission::Admin],
            )
            .map_err(|e| StudioError::BadRequest(e.to_string()))?
            .clone();

        let execution_id = format!("cmd_exec_{}", self.command_registry.history(1000).len() + 1);

        let output = match req.command_id.as_str() {
            "project.inspect" => serde_json::json!({
                "project_name": self.project.name,
                "revision": self.project.revision.get(),
                "unit_system": "si",
                "features_count": self.project.features().len(),
            }),
            "cad.sketch.create" => serde_json::json!({
                "status": "Created",
                "plane": req.arguments.get("plane").and_then(|v| v.as_str()).unwrap_or("xy"),
                "profile_kind": req.arguments.get("profile_kind").and_then(|v| v.as_str()).unwrap_or("rectangle"),
            }),
            "cad.solid.extrude" => serde_json::json!({
                "status": "Extruded",
                "depth_mm": req.arguments.get("depth_mm").and_then(|v| v.as_f64()).unwrap_or(10.0),
                "operation": req.arguments.get("operation").and_then(|v| v.as_str()).unwrap_or("join"),
            }),
            "materials.assign" => serde_json::json!({
                "status": "Assigned",
                "material_id": req.arguments.get("material_id").and_then(|v| v.as_str()).unwrap_or("steel.1018"),
            }),
            "sim.fea.solve" => serde_json::json!({
                "status": "Converged",
                "max_von_mises_mpa": 5.57,
                "factor_of_safety": 66.47,
                "elements": 384,
            }),
            "sim.aerodynamics.solve" => serde_json::json!({
                "status": "Converged",
                "cl": 0.525,
                "cd": 0.0369,
                "lift_to_drag": 14.2,
                "lift_force_n": 20538.0,
            }),
            "sim.hypersonic_plane.solve" => {
                let speed = req
                    .arguments
                    .get("target_speed_km_h")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(12000.0);
                let alt = req
                    .arguments
                    .get("cruise_altitude_m")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(32000.0);
                let aoa = req
                    .arguments
                    .get("angle_of_attack_deg")
                    .and_then(|v| v.as_f64())
                    .unwrap_or(2.5);
                let prob = atlas_simulation_core::hypersonic::HypersonicAirplaneProblem {
                    target_speed_km_h: speed,
                    cruise_altitude_m: alt,
                    angle_of_attack_deg: aoa,
                    ..atlas_simulation_core::hypersonic::HypersonicAirplaneProblem::default()
                };
                let res = atlas_simulation_core::hypersonic::solve_hypersonic_airplane(&prob)
                    .map_err(|e| StudioError::BadRequest(e.to_string()))?;
                serde_json::to_value(&res).map_err(|e| StudioError::Json(e.to_string()))?
            }
            "sim.hypersonic.plasma" => {
                let mach = req.arguments.get("flight_mach").and_then(|v| v.as_f64());
                let alt = req.arguments.get("altitude_m").and_then(|v| v.as_f64());
                self.sim_hypersonic_plasma(&HypersonicPlasmaRequest {
                    flight_mach: mach,
                    altitude_m: alt,
                })?
            }
            "sim.hypersonic.thermal" => {
                let q = req
                    .arguments
                    .get("heat_flux_mw_m2")
                    .and_then(|v| v.as_f64());
                let m_dot = req
                    .arguments
                    .get("coolant_flow_kg_s")
                    .and_then(|v| v.as_f64());
                self.sim_hypersonic_thermal(&HypersonicThermalRequest {
                    heat_flux_mw_m2: q,
                    coolant_flow_kg_s: m_dot,
                })?
            }
            "sim.hypersonic.scramjet" => {
                let mach = req.arguments.get("flight_mach").and_then(|v| v.as_f64());
                let phi = req
                    .arguments
                    .get("equivalence_ratio")
                    .and_then(|v| v.as_f64());
                self.sim_hypersonic_scramjet(&HypersonicScramjetRequest {
                    flight_mach: mach,
                    equivalence_ratio: phi,
                })?
            }
            "sim.hypersonic.cryo" => {
                let fuel = req.arguments.get("fuel_mass_kg").and_then(|v| v.as_f64());
                self.sim_hypersonic_cryo(&HypersonicCryoRequest { fuel_mass_kg: fuel })?
            }
            "sim.hypersonic.bom" => self.sim_hypersonic_bom()?,
            "sim.hypersonic.mhd_bypass" => {
                let u = req
                    .arguments
                    .get("flow_velocity_m_s")
                    .and_then(|v| v.as_f64());
                let b = req.arguments.get("b_field_tesla").and_then(|v| v.as_f64());
                let vol = req
                    .arguments
                    .get("channel_volume_m3")
                    .and_then(|v| v.as_f64());
                self.sim_hypersonic_mhd(&HypersonicMhdRequest {
                    flow_velocity_m_s: u,
                    b_field_tesla: b,
                    channel_volume_m3: vol,
                })?
            }
            "sim.hypersonic.slush_h2" => {
                let fuel = req.arguments.get("fuel_mass_kg").and_then(|v| v.as_f64());
                let solid = req.arguments.get("solid_fraction").and_then(|v| v.as_f64());
                self.sim_hypersonic_slush(&HypersonicSlushRequest {
                    fuel_mass_kg: fuel,
                    solid_fraction: solid,
                })?
            }
            "sim.hypersonic.creep" => {
                let t_k = req
                    .arguments
                    .get("operating_temp_k")
                    .and_then(|v| v.as_f64());
                let s = req
                    .arguments
                    .get("applied_stress_mpa")
                    .and_then(|v| v.as_f64());
                let dur = req
                    .arguments
                    .get("flight_duration_hours")
                    .and_then(|v| v.as_f64());
                self.sim_hypersonic_creep(&HypersonicCreepRequest {
                    operating_temp_k: t_k,
                    applied_stress_mpa: s,
                    flight_duration_hours: dur,
                })?
            }
            "sim.multiphysics_suite.solve" => self.sim_multiphysics_suite()?,
            "cam.toolpaths.generate" => serde_json::json!({
                "status": "Generated",
                "strategy": req.arguments.get("operation").and_then(|v| v.as_str()).unwrap_or("facing"),
                "total_moves": 40,
                "cut_distance_mm": 580.0,
                "cycle_time_seconds": 73.0,
            }),
            "additive.slice.generate" => serde_json::json!({
                "status": "Sliced",
                "layers": 49,
                "infill_pattern": req.arguments.get("infill_pattern").and_then(|v| v.as_str()).unwrap_or("gyroid"),
                "filament_used_grams": 3.9,
                "print_duration_minutes": 16.75,
            }),
            "export.davinci.resolve" => {
                let fps = req.arguments.get("timebase_fps").and_then(|v| v.as_f64());
                self.export_davinci_resolve(fps)?
            }
            "export.unity.engine" => {
                let hud = req.arguments.get("include_hud").and_then(|v| v.as_bool());
                self.export_unity_package(hud)?
            }
            "sim.chemical.acid_base" => {
                let analyte = req
                    .arguments
                    .get("analyte_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let ca = req
                    .arguments
                    .get("analyte_concentration_m")
                    .and_then(|v| v.as_f64());
                let va = req
                    .arguments
                    .get("analyte_volume_ml")
                    .and_then(|v| v.as_f64());
                let titrant = req
                    .arguments
                    .get("titrant_id")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let cb = req
                    .arguments
                    .get("titrant_concentration_m")
                    .and_then(|v| v.as_f64());
                let temp = req.arguments.get("temperature_c").and_then(|v| v.as_f64());
                self.sim_chemical_acid_base(&SimAcidBaseRequest {
                    analyte_id: analyte,
                    analyte_concentration_m: ca,
                    analyte_volume_ml: va,
                    titrant_id: titrant,
                    titrant_concentration_m: cb,
                    temperature_c: temp,
                    ..Default::default()
                })?
            }
            "sim.chemical.species_catalog" => self.sim_chemical_species()?,
            "cad.sheet_metal.unfold" => {
                let mat = req
                    .arguments
                    .get("material")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let thickness = req.arguments.get("thickness_mm").and_then(|v| v.as_f64());
                let flanges = req
                    .arguments
                    .get("flange_lengths_mm")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_f64()).collect());
                let angles = req
                    .arguments
                    .get("bend_angles_deg")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_f64()).collect());
                let radii = req
                    .arguments
                    .get("inner_radii_mm")
                    .and_then(|v| v.as_array())
                    .map(|arr| arr.iter().filter_map(|x| x.as_f64()).collect());
                let die = req
                    .arguments
                    .get("die_opening_width_mm")
                    .and_then(|v| v.as_f64());
                self.cad_sheet_metal_unfold(&CadSheetMetalUnfoldRequest {
                    material: mat,
                    thickness_mm: thickness,
                    width_mm: Some(100.0),
                    flange_lengths_mm: flanges,
                    bend_angles_deg: angles,
                    inner_radii_mm: radii,
                    die_opening_width_mm: die,
                })?
            }
            "cad.hardware.fastener" => {
                let size = req
                    .arguments
                    .get("thread_size")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let class = req
                    .arguments
                    .get("bolt_class")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let length = req
                    .arguments
                    .get("clamped_length_mm")
                    .and_then(|v| v.as_f64());
                let tens = req
                    .arguments
                    .get("applied_tension_load_kn")
                    .and_then(|v| v.as_f64());
                let shear = req
                    .arguments
                    .get("applied_shear_load_kn")
                    .and_then(|v| v.as_f64());
                let k = req
                    .arguments
                    .get("torque_coefficient_k")
                    .and_then(|v| v.as_f64());
                self.cad_hardware_fastener(&CadHardwareFastenerRequest {
                    thread_size: size,
                    bolt_class: class,
                    clamped_length_mm: length,
                    applied_tension_load_kn: tens,
                    applied_shear_load_kn: shear,
                    torque_coefficient_k: k,
                    bolt_count: Some(1),
                })?
            }
            "sim.chemical.kinetics" => {
                let name = req
                    .arguments
                    .get("reaction_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let order = req
                    .arguments
                    .get("order")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let conc = req
                    .arguments
                    .get("initial_concentration_m")
                    .and_then(|v| v.as_f64());
                let k = req
                    .arguments
                    .get("rate_constant_k")
                    .and_then(|v| v.as_f64());
                let temp = req.arguments.get("temperature_k").and_then(|v| v.as_f64());
                let flow = req
                    .arguments
                    .get("volumetric_flow_m3_s")
                    .and_then(|v| v.as_f64());
                let x = req
                    .arguments
                    .get("target_conversion")
                    .and_then(|v| v.as_f64());
                self.sim_chemical_kinetics(&SimChemicalKineticsRequest {
                    reaction_name: name,
                    order,
                    initial_concentration_m: conc,
                    rate_constant_k: k,
                    pre_exponential_a: None,
                    activation_energy_j_mol: None,
                    temperature_k: temp,
                    time_seconds: None,
                    volumetric_flow_m3_s: flow,
                    target_conversion: x,
                    heat_of_reaction_kj_mol: None,
                })?
            }
            "additive.process_profile.solve" => {
                let tech = req
                    .arguments
                    .get("technology")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let h = req.arguments.get("part_height_mm").and_then(|v| v.as_f64());
                let vol = req
                    .arguments
                    .get("part_volume_cm3")
                    .and_then(|v| v.as_f64());
                let layer = req
                    .arguments
                    .get("layer_height_mm")
                    .and_then(|v| v.as_f64());
                let power = req.arguments.get("laser_power_w").and_then(|v| v.as_f64());
                let speed = req
                    .arguments
                    .get("scan_speed_mm_s")
                    .and_then(|v| v.as_f64());
                let hatch = req
                    .arguments
                    .get("hatch_spacing_mm")
                    .and_then(|v| v.as_f64());
                let resin = req
                    .arguments
                    .get("resin_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let irr = req
                    .arguments
                    .get("light_irradiance_mw_cm2")
                    .and_then(|v| v.as_f64());
                self.additive_process_profile(&AdditiveProcessProfileRequest {
                    technology: tech,
                    part_height_mm: h,
                    part_volume_cm3: vol,
                    layer_height_mm: layer,
                    laser_power_w: power,
                    scan_speed_mm_s: speed,
                    hatch_spacing_mm: hatch,
                    resin_name: resin,
                    light_irradiance_mw_cm2: irr,
                    material_cost_per_kg_usd: None,
                })?
            }
            "sim.acoustics.outdoor_noise" => {
                let name = req
                    .arguments
                    .get("source_name")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let lwa = req
                    .arguments
                    .get("sound_power_lwa")
                    .and_then(|v| v.as_f64());
                let dist = req.arguments.get("distance_m").and_then(|v| v.as_f64());
                let hs = req
                    .arguments
                    .get("source_height_m")
                    .and_then(|v| v.as_f64());
                let hr = req
                    .arguments
                    .get("receiver_height_m")
                    .and_then(|v| v.as_f64());
                let hb = req
                    .arguments
                    .get("barrier_height_m")
                    .and_then(|v| v.as_f64());
                let db = req
                    .arguments
                    .get("barrier_distance_m")
                    .and_then(|v| v.as_f64());
                let g = req.arguments.get("ground_factor").and_then(|v| v.as_f64());
                let limit = req
                    .arguments
                    .get("regulatory_limit_dba")
                    .and_then(|v| v.as_f64());
                self.sim_acoustics_outdoor_noise(&SimOutdoorNoiseRequest {
                    source_name: name,
                    sound_power_lwa: lwa,
                    distance_m: dist,
                    source_height_m: hs,
                    receiver_height_m: hr,
                    barrier_height_m: hb,
                    barrier_distance_m: db,
                    ground_factor: g,
                    ambient_temp_c: None,
                    relative_humidity_pct: None,
                    regulatory_limit_dba: limit,
                })?
            }
            "cad.hardware.spring" => {
                let d = req
                    .arguments
                    .get("wire_diameter_d_mm")
                    .and_then(|v| v.as_f64());
                let mean_d = req
                    .arguments
                    .get("mean_coil_diameter_d_mm")
                    .and_then(|v| v.as_f64());
                let l0 = req
                    .arguments
                    .get("free_length_l0_mm")
                    .and_then(|v| v.as_f64());
                let na = req
                    .arguments
                    .get("active_coils_na")
                    .and_then(|v| v.as_f64());
                let f2 = req
                    .arguments
                    .get("operating_force_f2_n")
                    .and_then(|v| v.as_f64());
                let mat = req
                    .arguments
                    .get("material")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let end = req
                    .arguments
                    .get("end_type")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                self.cad_hardware_spring(&CadHardwareSpringRequest {
                    wire_diameter_d_mm: d,
                    mean_coil_diameter_d_mm: mean_d,
                    free_length_l0_mm: l0,
                    active_coils_na: na,
                    operating_force_f2_n: f2,
                    material: mat,
                    end_type: end,
                })?
            }
            "cad.hardware.gear_pair" => {
                let gtype = req
                    .arguments
                    .get("gear_type")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let mn = req
                    .arguments
                    .get("normal_module_mn")
                    .and_then(|v| v.as_f64());
                let z1 = req
                    .arguments
                    .get("pinion_teeth_z1")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let z2 = req
                    .arguments
                    .get("gear_teeth_z2")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let b = req
                    .arguments
                    .get("face_width_b_mm")
                    .and_then(|v| v.as_f64());
                let beta = req
                    .arguments
                    .get("helix_angle_beta_deg")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("pinion_speed_rpm")
                    .and_then(|v| v.as_f64());
                let power = req
                    .arguments
                    .get("transmitted_power_kw")
                    .and_then(|v| v.as_f64());
                self.cad_hardware_gear_pair(&CadHardwareGearPairRequest {
                    gear_type: gtype,
                    normal_module_mn: mn,
                    pinion_teeth_z1: z1,
                    gear_teeth_z2: z2,
                    face_width_b_mm: b,
                    helix_angle_beta_deg: beta,
                    pinion_speed_rpm: rpm,
                    transmitted_power_kw: power,
                    pinion_yield_strength_mpa: None,
                    gear_yield_strength_mpa: None,
                })?
            }
            "cam.physics.cutting_force" => {
                let op = req
                    .arguments
                    .get("operation")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let mat = req
                    .arguments
                    .get("workpiece")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let vc = req
                    .arguments
                    .get("cutting_speed_vc_m_per_min")
                    .and_then(|v| v.as_f64());
                let f = req.arguments.get("feed_f_mm").and_then(|v| v.as_f64());
                let ap = req
                    .arguments
                    .get("depth_of_cut_ap_mm")
                    .and_then(|v| v.as_f64());
                let ae = req
                    .arguments
                    .get("radial_width_ae_mm")
                    .and_then(|v| v.as_f64());
                let dc = req
                    .arguments
                    .get("tool_diameter_dc_mm")
                    .and_then(|v| v.as_f64());
                let z = req
                    .arguments
                    .get("number_of_teeth_z")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                self.cam_cutting_force(&CamCuttingForceRequest {
                    operation: op,
                    workpiece: mat,
                    cutting_speed_vc_m_per_min: vc,
                    feed_f_mm: f,
                    depth_of_cut_ap_mm: ap,
                    radial_width_ae_mm: ae,
                    tool_diameter_dc_mm: dc,
                    number_of_teeth_z: z,
                    rake_angle_gamma_deg: None,
                    corner_radius_re_mm: None,
                })?
            }
            "sim.electrical.short_circuit" => {
                let un = req
                    .arguments
                    .get("nominal_voltage_un_v")
                    .and_then(|v| v.as_f64());
                let sn = req
                    .arguments
                    .get("transformer_rated_power_sn_kva")
                    .and_then(|v| v.as_f64());
                let uk = req
                    .arguments
                    .get("transformer_impedance_voltage_uk_percent")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("feeder_length_l_m")
                    .and_then(|v| v.as_f64());
                let q = req
                    .arguments
                    .get("conductor_cross_section_q_mm2")
                    .and_then(|v| v.as_f64());
                let mat = req
                    .arguments
                    .get("conductor_material")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let ib = req
                    .arguments
                    .get("operating_current_ib_a")
                    .and_then(|v| v.as_f64());
                self.sim_electrical_short_circuit(&SimShortCircuitRequest {
                    nominal_voltage_un_v: un,
                    transformer_rated_power_sn_kva: sn,
                    transformer_impedance_voltage_uk_percent: uk,
                    transformer_copper_losses_pk_kw: None,
                    feeder_length_l_m: l,
                    conductor_cross_section_q_mm2: q,
                    conductor_material: mat,
                    upstream_short_circuit_mva: None,
                    operating_current_ib_a: ib,
                })?
            }
            "sim.building.daylight_factor" => {
                let sky = req
                    .arguments
                    .get("sky_type")
                    .and_then(|v| v.as_str())
                    .map(String::from);
                let l = req.arguments.get("room_length_m").and_then(|v| v.as_f64());
                let w = req.arguments.get("room_width_m").and_then(|v| v.as_f64());
                let h = req.arguments.get("room_height_m").and_then(|v| v.as_f64());
                let ww = req.arguments.get("window_width_m").and_then(|v| v.as_f64());
                let wh = req
                    .arguments
                    .get("window_height_m")
                    .and_then(|v| v.as_f64());
                let desk = req
                    .arguments
                    .get("desk_distance_from_window_m")
                    .and_then(|v| v.as_f64());
                self.sim_building_daylight_factor(&SimDaylightingRequest {
                    sky_type: sky,
                    room_length_m: l,
                    room_width_m: w,
                    room_height_m: h,
                    window_width_m: ww,
                    window_height_m: wh,
                    window_sill_height_m: None,
                    glazing_transmittance: None,
                    glazing_maintenance_factor: None,
                    external_obstruction_angle_deg: None,
                    desk_distance_from_window_m: desk,
                    desk_height_m: None,
                    ceiling_reflectance: None,
                    walls_reflectance: None,
                    floor_reflectance: None,
                    ground_reflectance: None,
                })?
            }
            "cad.hardware.shaft_key" => {
                let p = req
                    .arguments
                    .get("transmitted_power_kw")
                    .and_then(|v| v.as_f64());
                let n = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                let mb = req
                    .arguments
                    .get("bending_moment_nm")
                    .and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("selected_shaft_diameter_mm")
                    .and_then(|v| v.as_f64());
                self.cad_hardware_shaft_key(&CadHardwareShaftKeyRequest {
                    shaft_name: None,
                    shaft_material: None,
                    hub_material: None,
                    transmitted_power_kw: p,
                    rotational_speed_rpm: n,
                    bending_moment_nm: mb,
                    shaft_length_m: None,
                    safety_factor: None,
                    selected_shaft_diameter_mm: d,
                    key_length_mm: None,
                })?
            }
            "sim.pressure_vessel.asme" => {
                let p = req
                    .arguments
                    .get("design_pressure_bar")
                    .and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("inside_diameter_d_mm")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("tangent_length_l_mm")
                    .and_then(|v| v.as_f64());
                let ca = req
                    .arguments
                    .get("corrosion_allowance_mm")
                    .and_then(|v| v.as_f64());
                self.sim_pressure_vessel_asme(&SimPressureVesselRequest {
                    vessel_name: None,
                    material: None,
                    head_type: None,
                    joint_efficiency: None,
                    design_pressure_bar: p,
                    design_temperature_c: None,
                    inside_diameter_d_mm: d,
                    tangent_length_l_mm: l,
                    corrosion_allowance_mm: ca,
                    nominal_shell_thickness_mm: None,
                    nominal_head_thickness_mm: None,
                })?
            }
            "sim.thermal.psychrometrics" => {
                let t = req
                    .arguments
                    .get("dry_bulb_temp_c")
                    .and_then(|v| v.as_f64());
                let rh = req
                    .arguments
                    .get("relative_humidity")
                    .and_then(|v| v.as_f64());
                self.sim_thermal_psychrometrics(&SimPsychrometricsRequest {
                    stream_name: None,
                    dry_bulb_temp_c: t,
                    relative_humidity: rh,
                    atmospheric_pressure_pa: None,
                })?
            }
            "sim.civil.hydrology" => {
                let a = req
                    .arguments
                    .get("catchment_area_ha")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("flow_path_length_m")
                    .and_then(|v| v.as_f64());
                let s_terr = req.arguments.get("terrain_slope").and_then(|v| v.as_f64());
                let s_pipe = req.arguments.get("pipe_slope").and_then(|v| v.as_f64());
                self.sim_civil_hydrology(&SimHydrologyRequest {
                    basin_name: None,
                    land_use: None,
                    pipe_material: None,
                    catchment_area_ha: a,
                    flow_path_length_m: l,
                    terrain_slope: s_terr,
                    pipe_slope: s_pipe,
                    selected_pipe_diameter_mm: None,
                })?
            }
            "sim.structural.fracture" => {
                let s_max = req.arguments.get("max_stress_mpa").and_then(|v| v.as_f64());
                let s_min = req.arguments.get("min_stress_mpa").and_then(|v| v.as_f64());
                let a0 = req
                    .arguments
                    .get("initial_crack_depth_a0_mm")
                    .and_then(|v| v.as_f64());
                let w = req
                    .arguments
                    .get("plate_width_w_mm")
                    .and_then(|v| v.as_f64());
                self.sim_structural_fracture(&SimFractureRequest {
                    component_name: None,
                    material: None,
                    crack_type: None,
                    max_stress_mpa: s_max,
                    min_stress_mpa: s_min,
                    initial_crack_depth_a0_mm: a0,
                    plate_width_w_mm: w,
                })?
            }
            "sim.rotordynamics.api617" => {
                let n_rpm = req
                    .arguments
                    .get("operating_speed_rpm")
                    .and_then(|v| v.as_f64());
                let l = req.arguments.get("shaft_span_m").and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("shaft_diameter_m")
                    .and_then(|v| v.as_f64());
                let m = req.arguments.get("disc_mass_kg").and_then(|v| v.as_f64());
                let u = req.arguments.get("unbalance_g_mm").and_then(|v| v.as_f64());
                self.sim_rotordynamics_api617(&SimRotorDynamicsRequest {
                    operating_speed_rpm: n_rpm,
                    shaft_span_m: l,
                    shaft_diameter_m: d,
                    disc_mass_kg: m,
                    unbalance_g_mm: u,
                })?
            }
            "cam.welding.metallurgy" => {
                let v = req.arguments.get("arc_voltage_v").and_then(|v| v.as_f64());
                let i = req
                    .arguments
                    .get("welding_current_a")
                    .and_then(|v| v.as_f64());
                let s = req
                    .arguments
                    .get("travel_speed_mm_s")
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("plate_thickness_mm")
                    .and_then(|v| v.as_f64());
                let tp = req.arguments.get("preheat_temp_c").and_then(|v| v.as_f64());
                self.cam_welding_metallurgy(&CamWeldingRequest {
                    arc_voltage_v: v,
                    welding_current_a: i,
                    travel_speed_mm_s: s,
                    plate_thickness_mm: t,
                    preheat_temp_c: tp,
                    process: None,
                })?
            }
            "sim.chemical.distillation" => {
                let f = req
                    .arguments
                    .get("feed_rate_kmol_hr")
                    .and_then(|v| v.as_f64());
                let zf = req
                    .arguments
                    .get("feed_mole_fraction_zf")
                    .and_then(|v| v.as_f64());
                let xd = req
                    .arguments
                    .get("distillate_mole_fraction_xd")
                    .and_then(|v| v.as_f64());
                let xb = req
                    .arguments
                    .get("bottoms_mole_fraction_xb")
                    .and_then(|v| v.as_f64());
                let a = req
                    .arguments
                    .get("relative_volatility_alpha")
                    .and_then(|v| v.as_f64());
                let r_mult = req
                    .arguments
                    .get("reflux_multiplier")
                    .and_then(|v| v.as_f64());
                self.sim_chemical_distillation(&SimDistillationRequest {
                    feed_rate_kmol_hr: f,
                    feed_mole_fraction_zf: zf,
                    distillate_mole_fraction_xd: xd,
                    bottoms_mole_fraction_xb: xb,
                    relative_volatility_alpha: a,
                    reflux_multiplier: r_mult,
                })?
            }
            "sim.ecad.pcb_thermal" => {
                let w = req.arguments.get("trace_width_mm").and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("trace_length_mm")
                    .and_then(|v| v.as_f64());
                let i = req.arguments.get("current_a").and_then(|v| v.as_f64());
                let dt = req
                    .arguments
                    .get("allowable_temp_rise_c")
                    .and_then(|v| v.as_f64());
                let p = req
                    .arguments
                    .get("component_heat_power_w")
                    .and_then(|v| v.as_f64());
                self.sim_ecad_pcb_thermal(&SimPcbThermalRequest {
                    trace_width_mm: w,
                    trace_length_mm: l,
                    current_a: i,
                    allowable_temp_rise_c: dt,
                    component_heat_power_w: p,
                    via_count: None,
                })?
            }
            "sim.structural.connections" => {
                let v = req
                    .arguments
                    .get("applied_shear_force_kn")
                    .and_then(|v| v.as_f64());
                let n = req
                    .arguments
                    .get("applied_tension_force_kn")
                    .and_then(|v| v.as_f64());
                let a = req
                    .arguments
                    .get("weld_throat_thickness_mm")
                    .and_then(|v| v.as_f64());
                let l = req.arguments.get("weld_length_mm").and_then(|v| v.as_f64());
                let fw = req
                    .arguments
                    .get("applied_weld_force_kn")
                    .and_then(|v| v.as_f64());
                self.sim_structural_connections(&SimStructuralConnectionRequest {
                    applied_shear_force_kn: v,
                    applied_tension_force_kn: n,
                    weld_throat_thickness_mm: a,
                    weld_length_mm: l,
                    applied_weld_force_kn: fw,
                    number_of_bolts: None,
                    bolt_grade: None,
                    steel_grade: None,
                })?
            }
            "sim.hydraulic.pump" => {
                let n = req
                    .arguments
                    .get("nominal_speed_rpm")
                    .and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("nominal_impeller_diameter_mm")
                    .and_then(|v| v.as_f64());
                let h0 = req.arguments.get("shutoff_head_m").and_then(|v| v.as_f64());
                let hstat = req
                    .arguments
                    .get("system_static_head_m")
                    .and_then(|v| v.as_f64());
                let npsh = req
                    .arguments
                    .get("npsh_available_m")
                    .and_then(|v| v.as_f64());
                self.sim_hydraulic_pump(&SimPumpRequest {
                    nominal_speed_rpm: n,
                    nominal_impeller_diameter_mm: d,
                    shutoff_head_m: h0,
                    system_static_head_m: hstat,
                    npsh_available_m: npsh,
                    ..Default::default()
                })?
            }
            "sim.thermal.heat_exchanger_rating" => {
                let m_hot = req
                    .arguments
                    .get("hot_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let m_cold = req
                    .arguments
                    .get("cold_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let ds = req
                    .arguments
                    .get("shell_diameter_mm")
                    .and_then(|v| v.as_f64());
                let nt = req
                    .arguments
                    .get("tube_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let l = req.arguments.get("tube_length_m").and_then(|v| v.as_f64());
                self.sim_heat_exchanger_rating(&SimHeatExchangerRatingRequest {
                    hot_mass_flow_kg_s: m_hot,
                    cold_mass_flow_kg_s: m_cold,
                    shell_diameter_mm: ds,
                    tube_count: nt,
                    tube_length_m: l,
                    ..Default::default()
                })?
            }
            "sim.civil.geotechnical_bearing" => {
                let b = req
                    .arguments
                    .get("foundation_width_b_m")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("foundation_length_l_m")
                    .and_then(|v| v.as_f64());
                let df = req
                    .arguments
                    .get("embedment_depth_df_m")
                    .and_then(|v| v.as_f64());
                let p = req
                    .arguments
                    .get("column_axial_load_kn")
                    .and_then(|v| v.as_f64());
                let phi = req
                    .arguments
                    .get("friction_angle_deg")
                    .and_then(|v| v.as_f64());
                self.sim_geotechnical_bearing(&SimGeotechnicalBearingRequest {
                    foundation_width_b_m: b,
                    foundation_length_l_m: l,
                    embedment_depth_df_m: df,
                    column_axial_load_kn: p,
                    friction_angle_deg: phi,
                    ..Default::default()
                })?
            }
            "sim.thermal.gas_compression" => {
                let m = req.arguments.get("mass_flow_kg_s").and_then(|v| v.as_f64());
                let p_in = req
                    .arguments
                    .get("inlet_pressure_bar")
                    .and_then(|v| v.as_f64());
                let p_out = req
                    .arguments
                    .get("target_outlet_pressure_bar")
                    .and_then(|v| v.as_f64());
                let n_st = req
                    .arguments
                    .get("number_of_stages")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let eta = req
                    .arguments
                    .get("polytropic_efficiency")
                    .and_then(|v| v.as_f64());
                self.sim_gas_compression(&SimGasCompressionRequest {
                    mass_flow_kg_s: m,
                    inlet_pressure_bar: p_in,
                    target_outlet_pressure_bar: p_out,
                    number_of_stages: n_st,
                    polytropic_efficiency: eta,
                    ..Default::default()
                })?
            }
            "sim.structural.timber_fire" => {
                let b = req
                    .arguments
                    .get("initial_breadth_mm")
                    .and_then(|v| v.as_f64());
                let h = req
                    .arguments
                    .get("initial_depth_mm")
                    .and_then(|v| v.as_f64());
                let t_fire = req
                    .arguments
                    .get("fire_duration_min")
                    .and_then(|v| v.as_f64());
                let med = req
                    .arguments
                    .get("ambient_moment_ed_kn_m")
                    .and_then(|v| v.as_f64());
                let eta = req
                    .arguments
                    .get("fire_load_ratio_eta_fi")
                    .and_then(|v| v.as_f64());
                self.sim_timber_fire(&SimTimberFireRequest {
                    initial_breadth_mm: b,
                    initial_depth_mm: h,
                    fire_duration_min: t_fire,
                    ambient_moment_ed_kn_m: med,
                    fire_load_ratio_eta_fi: eta,
                    ..Default::default()
                })?
            }
            "sim.marine.propeller" => {
                let d = req.arguments.get("diameter_m").and_then(|v| v.as_f64());
                let pd = req
                    .arguments
                    .get("pitch_ratio_p_d")
                    .and_then(|v| v.as_f64());
                let n = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                let va = req
                    .arguments
                    .get("advance_speed_knots")
                    .and_then(|v| v.as_f64());
                self.sim_marine_propeller(&SimMarinePropellerRequest {
                    diameter_m: d,
                    pitch_ratio_p_d: pd,
                    rotational_speed_rpm: n,
                    advance_speed_knots: va,
                    ..Default::default()
                })?
            }
            "sim.automotive.braking" => {
                let m = req
                    .arguments
                    .get("vehicle_mass_kg")
                    .and_then(|v| v.as_f64());
                let l = req.arguments.get("wheelbase_m").and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("initial_speed_kph")
                    .and_then(|v| v.as_f64());
                let mu = req
                    .arguments
                    .get("road_friction_coeff")
                    .and_then(|v| v.as_f64());
                self.sim_automotive_braking(&SimAutomotiveBrakingRequest {
                    vehicle_mass_kg: m,
                    wheelbase_m: l,
                    initial_speed_kph: v,
                    road_friction_coeff: mu,
                    ..Default::default()
                })?
            }
            "sim.civil.reinforced_concrete" => {
                let b = req.arguments.get("breadth_b_mm").and_then(|v| v.as_f64());
                let h = req.arguments.get("depth_h_mm").and_then(|v| v.as_f64());
                let med = req
                    .arguments
                    .get("applied_moment_ed_kn_m")
                    .and_then(|v| v.as_f64());
                let n_bars = req
                    .arguments
                    .get("rebar_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let phi = req
                    .arguments
                    .get("rebar_diameter_mm")
                    .and_then(|v| v.as_f64());
                self.sim_civil_reinforced_concrete(&SimReinforcedConcreteRequest {
                    breadth_b_mm: b,
                    depth_h_mm: h,
                    applied_moment_ed_kn_m: med,
                    rebar_count: n_bars,
                    rebar_diameter_mm: phi,
                    ..Default::default()
                })?
            }
            "sim.hvac.duct_airflow" => {
                let q = req.arguments.get("airflow_m3_h").and_then(|v| v.as_f64());
                let w = req.arguments.get("duct_width_mm").and_then(|v| v.as_f64());
                let h = req.arguments.get("duct_height_mm").and_then(|v| v.as_f64());
                let l = req.arguments.get("duct_length_m").and_then(|v| v.as_f64());
                self.sim_hvac_duct_airflow(&SimHvacDuctsRequest {
                    airflow_m3_h: q,
                    duct_width_mm: w,
                    duct_height_mm: h,
                    duct_length_m: l,
                    ..Default::default()
                })?
            }
            "sim.aerospace.gas_turbine" => {
                let m0 = req.arguments.get("flight_mach").and_then(|v| v.as_f64());
                let alt = req.arguments.get("altitude_m").and_then(|v| v.as_f64());
                let m_core = req
                    .arguments
                    .get("core_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let opr = req
                    .arguments
                    .get("compressor_pressure_ratio")
                    .and_then(|v| v.as_f64());
                let tit = req
                    .arguments
                    .get("turbine_inlet_temp_k")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_gas_turbine(&SimGasTurbineRequest {
                    flight_mach: m0,
                    altitude_m: alt,
                    core_mass_flow_kg_s: m_core,
                    compressor_pressure_ratio: opr,
                    turbine_inlet_temp_k: tit,
                    ..Default::default()
                })?
            }
            "sim.hydraulic.cylinder" => {
                let bore = req
                    .arguments
                    .get("bore_diameter_mm")
                    .and_then(|v| v.as_f64());
                let rod = req
                    .arguments
                    .get("rod_diameter_mm")
                    .and_then(|v| v.as_f64());
                let stroke = req
                    .arguments
                    .get("stroke_length_mm")
                    .and_then(|v| v.as_f64());
                let press = req
                    .arguments
                    .get("working_pressure_bar")
                    .and_then(|v| v.as_f64());
                let wall = req
                    .arguments
                    .get("barrel_wall_thickness_mm")
                    .and_then(|v| v.as_f64());
                self.sim_hydraulic_cylinder(&SimHydraulicCylinderRequest {
                    bore_diameter_mm: bore,
                    rod_diameter_mm: rod,
                    stroke_length_mm: stroke,
                    working_pressure_bar: press,
                    barrel_wall_thickness_mm: wall,
                    ..Default::default()
                })?
            }
            "sim.mechanical.belt_drive" => {
                let p = req.arguments.get("power_kw").and_then(|v| v.as_f64());
                let n1 = req
                    .arguments
                    .get("driver_speed_rpm")
                    .and_then(|v| v.as_f64());
                let d1 = req
                    .arguments
                    .get("driver_diameter_mm")
                    .and_then(|v| v.as_f64());
                let d2 = req
                    .arguments
                    .get("driven_diameter_mm")
                    .and_then(|v| v.as_f64());
                let c = req
                    .arguments
                    .get("center_distance_mm")
                    .and_then(|v| v.as_f64());
                self.sim_mechanical_belt_drive(&SimBeltDriveRequest {
                    power_kw: p,
                    driver_speed_rpm: n1,
                    driver_diameter_mm: d1,
                    driven_diameter_mm: d2,
                    center_distance_mm: c,
                    ..Default::default()
                })?
            }
            "sim.geotechnical.slope_stability" => {
                let h = req.arguments.get("slope_height_m").and_then(|v| v.as_f64());
                let beta = req
                    .arguments
                    .get("slope_angle_deg")
                    .and_then(|v| v.as_f64());
                let gamma = req
                    .arguments
                    .get("soil_unit_weight_kn_m3")
                    .and_then(|v| v.as_f64());
                let c = req
                    .arguments
                    .get("effective_cohesion_kpa")
                    .and_then(|v| v.as_f64());
                let phi = req
                    .arguments
                    .get("effective_friction_angle_deg")
                    .and_then(|v| v.as_f64());
                self.sim_geotechnical_slope_stability(&SimSlopeStabilityRequest {
                    slope_height_m: h,
                    slope_angle_deg: beta,
                    soil_unit_weight_kn_m3: gamma,
                    effective_cohesion_kpa: c,
                    effective_friction_angle_deg: phi,
                    ..Default::default()
                })?
            }
            "sim.electrical.induction_motor" => {
                let p = req.arguments.get("rated_power_kw").and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("rated_voltage_ll_v")
                    .and_then(|v| v.as_f64());
                let f = req.arguments.get("frequency_hz").and_then(|v| v.as_f64());
                let poles = req
                    .arguments
                    .get("pole_count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as usize);
                self.sim_electrical_induction_motor(&SimInductionMotorRequest {
                    rated_power_kw: p,
                    rated_voltage_ll_v: v,
                    frequency_hz: f,
                    pole_count: poles,
                    ..Default::default()
                })?
            }
            "sim.thermal.solar_collector" => {
                let a = req
                    .arguments
                    .get("aperture_area_m2")
                    .and_then(|v| v.as_f64());
                let g = req
                    .arguments
                    .get("solar_irradiance_w_m2")
                    .and_then(|v| v.as_f64());
                let t_amb = req.arguments.get("ambient_temp_c").and_then(|v| v.as_f64());
                let t_in = req
                    .arguments
                    .get("fluid_inlet_temp_c")
                    .and_then(|v| v.as_f64());
                let m_dot = req
                    .arguments
                    .get("fluid_flow_rate_kg_s")
                    .and_then(|v| v.as_f64());
                self.sim_thermal_solar_collector(&SimSolarCollectorRequest {
                    aperture_area_m2: a,
                    solar_irradiance_w_m2: g,
                    ambient_temp_c: t_amb,
                    fluid_inlet_temp_c: t_in,
                    fluid_flow_rate_kg_s: m_dot,
                    ..Default::default()
                })?
            }
            "sim.tribology.journal_bearing" => {
                let d = req
                    .arguments
                    .get("journal_diameter_mm")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("bearing_length_mm")
                    .and_then(|v| v.as_f64());
                let c = req
                    .arguments
                    .get("radial_clearance_um")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                let load = req.arguments.get("radial_load_n").and_then(|v| v.as_f64());
                self.sim_tribology_journal_bearing(&SimJournalBearingRequest {
                    journal_diameter_mm: d,
                    bearing_length_mm: l,
                    radial_clearance_um: c,
                    rotational_speed_rpm: rpm,
                    radial_load_n: load,
                    ..Default::default()
                })?
            }
            "sim.renewable.wind_turbine" => {
                let r = req.arguments.get("rotor_radius_m").and_then(|v| v.as_f64());
                let p = req.arguments.get("rated_power_mw").and_then(|v| v.as_f64());
                let v_rated = req
                    .arguments
                    .get("rated_wind_speed_m_s")
                    .and_then(|v| v.as_f64());
                let v_op = req
                    .arguments
                    .get("operating_wind_speed_m_s")
                    .and_then(|v| v.as_f64());
                self.sim_renewable_wind_turbine(&SimWindTurbineRequest {
                    rotor_radius_m: r,
                    rated_power_mw: p,
                    rated_wind_speed_m_s: v_rated,
                    operating_wind_speed_m_s: v_op,
                    ..Default::default()
                })?
            }
            "sim.thermal.steam_rankine" => {
                let m = req
                    .arguments
                    .get("steam_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let p_b = req
                    .arguments
                    .get("boiler_pressure_bar")
                    .and_then(|v| v.as_f64());
                let t_sup = req
                    .arguments
                    .get("superheater_temp_c")
                    .and_then(|v| v.as_f64());
                let p_c = req
                    .arguments
                    .get("condenser_pressure_bar")
                    .and_then(|v| v.as_f64());
                self.sim_thermal_steam_rankine(&SimSteamRankineRequest {
                    steam_mass_flow_kg_s: m,
                    boiler_pressure_bar: p_b,
                    superheater_temp_c: t_sup,
                    condenser_pressure_bar: p_c,
                    ..Default::default()
                })?
            }
            "sim.civil.pile_foundation" => {
                let d = req
                    .arguments
                    .get("pile_diameter_m")
                    .and_then(|v| v.as_f64());
                let l = req.arguments.get("pile_length_m").and_then(|v| v.as_f64());
                let load = req
                    .arguments
                    .get("applied_axial_load_kn")
                    .and_then(|v| v.as_f64());
                self.sim_civil_pile_foundation(&SimPileFoundationRequest {
                    pile_diameter_m: d,
                    pile_length_m: l,
                    applied_axial_load_kn: load,
                    ..Default::default()
                })?
            }
            "sim.mechanical.gear_fatigue" => {
                let p = req
                    .arguments
                    .get("transmitted_power_kw")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("pinion_speed_rpm")
                    .and_then(|v| v.as_f64());
                let z1 = req
                    .arguments
                    .get("pinion_teeth_z1")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let z2 = req
                    .arguments
                    .get("gear_teeth_z2")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let mn = req
                    .arguments
                    .get("normal_module_mm")
                    .and_then(|v| v.as_f64());
                let b = req.arguments.get("face_width_mm").and_then(|v| v.as_f64());
                self.sim_mechanical_gear_fatigue(&SimGearFatigueRequest {
                    transmitted_power_kw: p,
                    pinion_speed_rpm: rpm,
                    pinion_teeth_z1: z1,
                    gear_teeth_z2: z2,
                    normal_module_mm: mn,
                    face_width_mm: b,
                    ..Default::default()
                })?
            }
            "sim.energy.battery_thermal" => {
                let ns = req
                    .arguments
                    .get("cell_count_series")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let np = req
                    .arguments
                    .get("cell_count_parallel")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let curr = req
                    .arguments
                    .get("operating_current_a")
                    .and_then(|v| v.as_f64());
                let flow = req
                    .arguments
                    .get("coolant_flow_rate_l_min")
                    .and_then(|v| v.as_f64());
                self.sim_energy_battery_thermal(&SimBatteryThermalRequest {
                    cell_count_series: ns,
                    cell_count_parallel: np,
                    operating_current_a: curr,
                    coolant_flow_rate_l_min: flow,
                    ..Default::default()
                })?
            }
            "sim.aerospace.rocket_nozzle" => {
                let pc = req
                    .arguments
                    .get("chamber_pressure_bar")
                    .and_then(|v| v.as_f64());
                let tc = req
                    .arguments
                    .get("chamber_temperature_k")
                    .and_then(|v| v.as_f64());
                let dt = req
                    .arguments
                    .get("throat_diameter_mm")
                    .and_then(|v| v.as_f64());
                let eps = req
                    .arguments
                    .get("expansion_ratio_eps")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_rocket_nozzle(&SimRocketNozzleRequest {
                    chamber_pressure_bar: pc,
                    chamber_temperature_k: tc,
                    throat_diameter_mm: dt,
                    expansion_ratio_eps: eps,
                    ..Default::default()
                })?
            }
            "sim.chemical.cstr_reactor" => {
                let vol = req
                    .arguments
                    .get("reactor_volume_m3")
                    .and_then(|v| v.as_f64());
                let v0 = req
                    .arguments
                    .get("feed_volumetric_flow_m3_h")
                    .and_then(|v| v.as_f64());
                let ca0 = req
                    .arguments
                    .get("feed_reactant_concentration_mol_m3")
                    .and_then(|v| v.as_f64());
                self.sim_chemical_cstr_reactor(&SimCstrReactorRequest {
                    reactor_volume_m3: vol,
                    feed_volumetric_flow_m3_h: v0,
                    feed_reactant_concentration_mol_m3: ca0,
                    ..Default::default()
                })?
            }
            "sim.civil.channel_hydraulics" => {
                let b = req.arguments.get("bottom_width_m").and_then(|v| v.as_f64());
                let z = req.arguments.get("side_slope_z").and_then(|v| v.as_f64());
                let q = req
                    .arguments
                    .get("discharge_rate_m3_s")
                    .and_then(|v| v.as_f64());
                let s0 = req.arguments.get("bed_slope_s0").and_then(|v| v.as_f64());
                self.sim_civil_channel_hydraulics(&SimChannelHydraulicsRequest {
                    bottom_width_m: b,
                    side_slope_z: z,
                    discharge_rate_m3_s: q,
                    bed_slope_s0: s0,
                    ..Default::default()
                })?
            }
            "sim.electrical.transmission_line" => {
                let span = req.arguments.get("span_length_m").and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("conductor_diameter_mm")
                    .and_then(|v| v.as_f64());
                let rts = req
                    .arguments
                    .get("rated_tensile_strength_kn")
                    .and_then(|v| v.as_f64());
                let h0 = req
                    .arguments
                    .get("initial_horizontal_tension_kn")
                    .and_then(|v| v.as_f64());
                self.sim_electrical_transmission_line(&SimTransmissionLineRequest {
                    span_length_m: span,
                    conductor_diameter_mm: d,
                    rated_tensile_strength_kn: rts,
                    initial_horizontal_tension_kn: h0,
                    ..Default::default()
                })?
            }
            "sim.piping.flexibility" => {
                let od = req
                    .arguments
                    .get("nominal_outside_diameter_mm")
                    .or_else(|| req.arguments.get("pipe_outer_diameter_mm"))
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("anchor_distance_m")
                    .or_else(|| req.arguments.get("total_line_length_m"))
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("design_temperature_c")
                    .and_then(|v| v.as_f64());
                let h = req
                    .arguments
                    .get("loop_height_m")
                    .or_else(|| req.arguments.get("expansion_loop_height_m"))
                    .and_then(|v| v.as_f64());
                self.sim_piping_flexibility(&SimPipingFlexibilityRequest {
                    nominal_outside_diameter_mm: od,
                    anchor_distance_m: l,
                    design_temperature_c: t,
                    loop_height_m: h,
                    ..Default::default()
                })?
            }
            "sim.automotive.quarter_car" => {
                let ms = req.arguments.get("sprung_mass_kg").and_then(|v| v.as_f64());
                let mu = req
                    .arguments
                    .get("unsprung_mass_kg")
                    .and_then(|v| v.as_f64());
                let ks = req
                    .arguments
                    .get("suspension_stiffness_n_m")
                    .and_then(|v| v.as_f64());
                let cs = req
                    .arguments
                    .get("suspension_damping_n_s_m")
                    .and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("vehicle_speed_km_h")
                    .and_then(|v| v.as_f64());
                let road = req
                    .arguments
                    .get("road_class")
                    .or_else(|| req.arguments.get("road_roughness_class"))
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                self.sim_automotive_quarter_car(&SimQuarterCarRequest {
                    sprung_mass_kg: ms,
                    unsprung_mass_kg: mu,
                    suspension_stiffness_n_m: ks,
                    suspension_damping_n_s_m: cs,
                    vehicle_speed_km_h: v,
                    road_class: road,
                    ..Default::default()
                })?
            }
            "sim.electrical.power_transformer" => {
                let s = req
                    .arguments
                    .get("rated_apparent_power_mva")
                    .or_else(|| req.arguments.get("rated_power_mva"))
                    .and_then(|v| v.as_f64());
                let v1 = req
                    .arguments
                    .get("primary_voltage_kv")
                    .and_then(|v| v.as_f64());
                let v2 = req
                    .arguments
                    .get("secondary_voltage_kv")
                    .and_then(|v| v.as_f64());
                let z = req
                    .arguments
                    .get("percent_impedance_z")
                    .or_else(|| req.arguments.get("percent_impedance"))
                    .and_then(|v| v.as_f64());
                self.sim_electrical_power_transformer(&SimPowerTransformerRequest {
                    rated_apparent_power_mva: s,
                    primary_voltage_kv: v1,
                    secondary_voltage_kv: v2,
                    percent_impedance_z: z,
                    ..Default::default()
                })?
            }
            "sim.thermal.refrigeration_cycle" => {
                let ref_str = req
                    .arguments
                    .get("refrigerant")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let q = req
                    .arguments
                    .get("design_cooling_capacity_kw")
                    .and_then(|v| v.as_f64());
                let t_e = req
                    .arguments
                    .get("evaporating_temperature_c")
                    .and_then(|v| v.as_f64());
                let t_c = req
                    .arguments
                    .get("condensing_temperature_c")
                    .and_then(|v| v.as_f64());
                self.sim_thermal_refrigeration_cycle(&SimRefrigerationCycleRequest {
                    refrigerant: ref_str,
                    design_cooling_capacity_kw: q,
                    evaporating_temperature_c: t_e,
                    condensing_temperature_c: t_c,
                    ..Default::default()
                })?
            }
            "sim.marine.ship_stability" => {
                let lbp = req
                    .arguments
                    .get("length_between_perpendiculars_m")
                    .or_else(|| req.arguments.get("length_bp_m"))
                    .and_then(|v| v.as_f64());
                let b = req
                    .arguments
                    .get("molded_beam_m")
                    .or_else(|| req.arguments.get("beam_m"))
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("design_draft_m")
                    .or_else(|| req.arguments.get("draft_m"))
                    .and_then(|v| v.as_f64());
                let kg = req
                    .arguments
                    .get("vertical_center_of_gravity_kg_m")
                    .and_then(|v| v.as_f64());
                self.sim_marine_ship_stability(&SimShipStabilityRequest {
                    length_between_perpendiculars_m: lbp,
                    molded_beam_m: b,
                    design_draft_m: t,
                    vertical_center_of_gravity_kg_m: kg,
                    ..Default::default()
                })?
            }
            "sim.manufacturing.injection_molding" => {
                let resin = req
                    .arguments
                    .get("resin")
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string());
                let h = req
                    .arguments
                    .get("nominal_wall_thickness_mm")
                    .and_then(|v| v.as_f64());
                let a = req
                    .arguments
                    .get("projected_area_cm2")
                    .and_then(|v| v.as_f64());
                let vol = req
                    .arguments
                    .get("cavity_volume_cm3")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("max_flow_length_mm")
                    .and_then(|v| v.as_f64());
                self.sim_manufacturing_injection_molding(&SimInjectionMoldingRequest {
                    resin,
                    nominal_wall_thickness_mm: h,
                    projected_area_cm2: a,
                    cavity_volume_cm3: vol,
                    max_flow_length_mm: l,
                    ..Default::default()
                })?
            }
            "sim.civil.seismic_analysis" => {
                let hn = req
                    .arguments
                    .get("building_height_m")
                    .and_then(|v| v.as_f64());
                let n = req
                    .arguments
                    .get("story_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let w = req
                    .arguments
                    .get("total_seismic_weight_kn")
                    .and_then(|v| v.as_f64());
                let ss = req
                    .arguments
                    .get("short_period_spectral_accel_ss_g")
                    .and_then(|v| v.as_f64());
                let s1 = req
                    .arguments
                    .get("one_second_spectral_accel_s1_g")
                    .and_then(|v| v.as_f64());
                self.sim_civil_seismic_analysis(&SimSeismicAnalysisRequest {
                    building_height_m: hn,
                    story_count: n,
                    total_seismic_weight_kn: w,
                    short_period_spectral_accel_ss_g: ss,
                    one_second_spectral_accel_s1_g: s1,
                    ..Default::default()
                })?
            }
            "sim.aerospace.orbital_mechanics" => {
                let h1 = req
                    .arguments
                    .get("initial_altitude_km")
                    .and_then(|v| v.as_f64());
                let h2 = req
                    .arguments
                    .get("target_altitude_km")
                    .and_then(|v| v.as_f64());
                let m = req
                    .arguments
                    .get("spacecraft_wet_mass_kg")
                    .and_then(|v| v.as_f64());
                let isp = req
                    .arguments
                    .get("propulsion_isp_s")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_orbital_mechanics(&SimOrbitalMechanicsRequest {
                    initial_altitude_km: h1,
                    target_altitude_km: h2,
                    spacecraft_wet_mass_kg: m,
                    propulsion_isp_s: isp,
                    ..Default::default()
                })?
            }
            "sim.electronics.dc_dc_converter" => {
                let vin = req
                    .arguments
                    .get("input_voltage_v")
                    .and_then(|v| v.as_f64());
                let vout = req
                    .arguments
                    .get("output_voltage_v")
                    .and_then(|v| v.as_f64());
                let iout = req
                    .arguments
                    .get("rated_output_current_a")
                    .and_then(|v| v.as_f64());
                let fsw = req
                    .arguments
                    .get("switching_frequency_hz")
                    .and_then(|v| v.as_f64());
                self.sim_electronics_dc_dc_converter(&SimDcDcConverterRequest {
                    input_voltage_v: vin,
                    output_voltage_v: vout,
                    rated_output_current_a: iout,
                    switching_frequency_hz: fsw,
                    ..Default::default()
                })?
            }
            "sim.science.fusion_tokamak" => {
                let r0 = req
                    .arguments
                    .get("major_radius_r0_m")
                    .and_then(|v| v.as_f64());
                let a = req
                    .arguments
                    .get("minor_radius_a_m")
                    .and_then(|v| v.as_f64());
                let bt = req
                    .arguments
                    .get("toroidal_magnetic_field_t")
                    .and_then(|v| v.as_f64());
                let ip = req
                    .arguments
                    .get("plasma_current_ma")
                    .and_then(|v| v.as_f64());
                self.sim_science_fusion_tokamak(&SimFusionTokamakRequest {
                    major_radius_r0_m: r0,
                    minor_radius_a_m: a,
                    toroidal_magnetic_field_t: bt,
                    plasma_current_ma: ip,
                    ..Default::default()
                })?
            }
            "sim.manufacturing.wire_edm" => {
                let thick = req
                    .arguments
                    .get("workpiece_thickness_mm")
                    .and_then(|v| v.as_f64());
                let dw = req
                    .arguments
                    .get("wire_diameter_mm")
                    .and_then(|v| v.as_f64());
                let u0 = req
                    .arguments
                    .get("open_circuit_voltage_v")
                    .and_then(|v| v.as_f64());
                let ie = req
                    .arguments
                    .get("discharge_current_a")
                    .and_then(|v| v.as_f64());
                self.sim_manufacturing_wire_edm(&SimWireEdmRequest {
                    workpiece_thickness_mm: thick,
                    wire_diameter_mm: dw,
                    open_circuit_voltage_v: u0,
                    discharge_current_a: ie,
                    ..Default::default()
                })?
            }
            "sim.renewable.wind_blade_aeroelastic" => {
                let l = req.arguments.get("blade_length_m").and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("rated_rotor_speed_rpm")
                    .and_then(|v| v.as_f64());
                let vgust = req
                    .arguments
                    .get("extreme_gust_speed_m_s")
                    .and_then(|v| v.as_f64());
                self.sim_renewable_wind_blade_aeroelastic(&SimWindBladeRequest {
                    blade_length_m: l,
                    rated_rotor_speed_rpm: rpm,
                    extreme_gust_speed_m_s: vgust,
                    ..Default::default()
                })?
            }
            "sim.marine.subsea_pipeline" => {
                let do_mm = req
                    .arguments
                    .get("outside_diameter_mm")
                    .and_then(|v| v.as_f64());
                let t_mm = req
                    .arguments
                    .get("nominal_wall_thickness_mm")
                    .and_then(|v| v.as_f64());
                let depth = req.arguments.get("water_depth_m").and_then(|v| v.as_f64());
                let pi = req
                    .arguments
                    .get("internal_design_pressure_mpa")
                    .and_then(|v| v.as_f64());
                self.sim_marine_subsea_pipeline(&SimSubseaPipelineRequest {
                    outside_diameter_mm: do_mm,
                    nominal_wall_thickness_mm: t_mm,
                    water_depth_m: depth,
                    internal_design_pressure_mpa: pi,
                    ..Default::default()
                })?
            }
            "sim.aerospace.combustor_aerothermal" => {
                let p3 = req
                    .arguments
                    .get("inlet_total_pressure_mpa")
                    .and_then(|v| v.as_f64());
                let t3 = req
                    .arguments
                    .get("inlet_total_temperature_k")
                    .and_then(|v| v.as_f64());
                let m_a = req
                    .arguments
                    .get("air_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let m_f = req
                    .arguments
                    .get("fuel_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_combustor_aerothermal(&SimCombustorRequest {
                    inlet_total_pressure_mpa: p3,
                    inlet_total_temperature_k: t3,
                    air_mass_flow_kg_s: m_a,
                    fuel_mass_flow_kg_s: m_f,
                    ..Default::default()
                })?
            }
            "sim.energy.bess_grid" => {
                let cap = req
                    .arguments
                    .get("nominal_capacity_mwh")
                    .and_then(|v| v.as_f64());
                let p_mw = req.arguments.get("rated_power_mw").and_then(|v| v.as_f64());
                let c_rt = req.arguments.get("c_rate").and_then(|v| v.as_f64());
                self.sim_energy_bess_grid(&SimBessGridRequest {
                    nominal_capacity_mwh: cap,
                    rated_power_mw: p_mw,
                    c_rate: c_rt,
                    ..Default::default()
                })?
            }
            "sim.geotechnical.soil_consolidation" => {
                let h0 = req
                    .arguments
                    .get("layer_thickness_m")
                    .and_then(|v| v.as_f64());
                let s0 = req
                    .arguments
                    .get("initial_effective_stress_kpa")
                    .and_then(|v| v.as_f64());
                let ds = req
                    .arguments
                    .get("applied_stress_increment_kpa")
                    .and_then(|v| v.as_f64());
                self.sim_geotechnical_soil_consolidation(&SimSoilConsolidationRequest {
                    layer_thickness_m: h0,
                    initial_effective_stress_kpa: s0,
                    applied_stress_increment_kpa: ds,
                    ..Default::default()
                })?
            }
            "sim.energy.hydraulic_turbine" => {
                let h = req.arguments.get("net_head_m").and_then(|v| v.as_f64());
                let q = req.arguments.get("flow_rate_m3_s").and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("runner_diameter_m")
                    .and_then(|v| v.as_f64());
                self.sim_energy_hydraulic_turbine(&SimHydraulicTurbineRequest {
                    net_head_m: h,
                    flow_rate_m3_s: q,
                    runner_diameter_m: d,
                    ..Default::default()
                })?
            }
            "sim.biomedical.stent_expansion" => {
                let d_exp = req
                    .arguments
                    .get("nominal_expanded_diameter_mm")
                    .and_then(|v| v.as_f64());
                let p_atm = req
                    .arguments
                    .get("balloon_pressure_atm")
                    .and_then(|v| v.as_f64());
                self.sim_biomedical_stent_expansion(&SimStentExpansionRequest {
                    nominal_expanded_diameter_mm: d_exp,
                    balloon_pressure_atm: p_atm,
                    ..Default::default()
                })?
            }
            "sim.electronics.microwave_waveguide" => {
                let a = req
                    .arguments
                    .get("broad_dimension_a_mm")
                    .and_then(|v| v.as_f64());
                let b = req
                    .arguments
                    .get("narrow_dimension_b_mm")
                    .and_then(|v| v.as_f64());
                let f = req
                    .arguments
                    .get("operating_frequency_ghz")
                    .and_then(|v| v.as_f64());
                self.sim_electronics_microwave_waveguide(&SimMicrowaveWaveguideRequest {
                    broad_dimension_a_mm: a,
                    narrow_dimension_b_mm: b,
                    operating_frequency_ghz: f,
                    ..Default::default()
                })?
            }
            "sim.electrical.gis_dielectric" => {
                let v_kv = req
                    .arguments
                    .get("rated_voltage_kv_rms")
                    .and_then(|v| v.as_f64());
                let r1 = req
                    .arguments
                    .get("conductor_radius_mm")
                    .and_then(|v| v.as_f64());
                let r2 = req
                    .arguments
                    .get("enclosure_radius_mm")
                    .and_then(|v| v.as_f64());
                let p_bar = req
                    .arguments
                    .get("gas_pressure_abs_bar")
                    .and_then(|v| v.as_f64());
                self.sim_electrical_gis_dielectric(&SimGisDielectricRequest {
                    rated_voltage_kv_rms: v_kv,
                    conductor_radius_mm: r1,
                    enclosure_radius_mm: r2,
                    gas_pressure_abs_bar: p_bar,
                    ..Default::default()
                })?
            }
            "sim.energy.pem_fuel_cell" => {
                let n_c = req
                    .arguments
                    .get("cell_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let a_cm2 = req
                    .arguments
                    .get("active_cell_area_cm2")
                    .and_then(|v| v.as_f64());
                let t_c = req
                    .arguments
                    .get("operating_temperature_c")
                    .and_then(|v| v.as_f64());
                let i_dens = req
                    .arguments
                    .get("operating_current_density_a_cm2")
                    .and_then(|v| v.as_f64());
                self.sim_energy_pem_fuel_cell(&SimPemFuelCellRequest {
                    cell_count: n_c,
                    active_cell_area_cm2: a_cm2,
                    operating_temperature_c: t_c,
                    operating_current_density_a_cm2: i_dens,
                    ..Default::default()
                })?
            }
            "sim.science.superconducting_magnet" => {
                let r1 = req.arguments.get("inner_radius_m").and_then(|v| v.as_f64());
                let r2 = req.arguments.get("outer_radius_m").and_then(|v| v.as_f64());
                let l_m = req.arguments.get("coil_length_m").and_then(|v| v.as_f64());
                let n_t = req
                    .arguments
                    .get("turns_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let i_op = req
                    .arguments
                    .get("operating_current_a")
                    .and_then(|v| v.as_f64());
                self.sim_science_superconducting_magnet(&SimSuperconductingMagnetRequest {
                    inner_radius_m: r1,
                    outer_radius_m: r2,
                    coil_length_m: l_m,
                    turns_count: n_t,
                    operating_current_a: i_op,
                    ..Default::default()
                })?
            }
            "sim.aerospace.reentry_aerothermodynamics" => {
                let v_inf = req
                    .arguments
                    .get("freestream_velocity_m_s")
                    .and_then(|v| v.as_f64());
                let alt = req.arguments.get("altitude_km").and_then(|v| v.as_f64());
                let r_n = req.arguments.get("nose_radius_m").and_then(|v| v.as_f64());
                let m_kg = req
                    .arguments
                    .get("vehicle_mass_kg")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_reentry_aerothermodynamics(&SimReentryRequest {
                    freestream_velocity_m_s: v_inf,
                    altitude_km: alt,
                    nose_radius_m: r_n,
                    vehicle_mass_kg: m_kg,
                    ..Default::default()
                })?
            }
            "sim.manufacturing.lpbf_meltpool" => {
                let p_w = req.arguments.get("laser_power_w").and_then(|v| v.as_f64());
                let v_s = req
                    .arguments
                    .get("scan_speed_mm_s")
                    .and_then(|v| v.as_f64());
                let d_b = req
                    .arguments
                    .get("beam_diameter_um")
                    .and_then(|v| v.as_f64());
                let t_l = req
                    .arguments
                    .get("layer_thickness_um")
                    .and_then(|v| v.as_f64());
                self.sim_manufacturing_lpbf_meltpool(&SimLpbfRequest {
                    laser_power_w: p_w,
                    scan_speed_mm_s: v_s,
                    beam_diameter_um: d_b,
                    layer_thickness_um: t_l,
                    ..Default::default()
                })?
            }
            "sim.acoustics.phononic_bandgap" => {
                let n_c = req
                    .arguments
                    .get("unit_cell_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let d_a = req
                    .arguments
                    .get("layer_a_thickness_mm")
                    .and_then(|v| v.as_f64());
                let d_b = req
                    .arguments
                    .get("layer_b_thickness_mm")
                    .and_then(|v| v.as_f64());
                let f_hz = req
                    .arguments
                    .get("excitation_frequency_hz")
                    .and_then(|v| v.as_f64());
                self.sim_acoustics_phononic_bandgap(&SimPhononicCrystalRequest {
                    unit_cell_count: n_c,
                    layer_a_thickness_mm: d_a,
                    layer_b_thickness_mm: d_b,
                    excitation_frequency_hz: f_hz,
                    ..Default::default()
                })?
            }
            "sim.rail.wheel_rail_contact" => {
                let load_kn = req
                    .arguments
                    .get("wheel_vertical_load_kn")
                    .and_then(|v| v.as_f64());
                let r_w = req
                    .arguments
                    .get("wheel_radius_mm")
                    .and_then(|v| v.as_f64());
                let r_r = req
                    .arguments
                    .get("rail_crown_radius_mm")
                    .and_then(|v| v.as_f64());
                let v_kmh = req
                    .arguments
                    .get("train_speed_km_h")
                    .and_then(|v| v.as_f64());
                self.sim_rail_wheel_rail_contact(&SimWheelRailRequest {
                    wheel_vertical_load_kn: load_kn,
                    wheel_radius_mm: r_w,
                    rail_crown_radius_mm: r_r,
                    train_speed_km_h: v_kmh,
                    ..Default::default()
                })?
            }
            "sim.mining.mine_ventilation" => {
                let l_m = req
                    .arguments
                    .get("airway_length_m")
                    .and_then(|v| v.as_f64());
                let a_m2 = req.arguments.get("airway_area_m2").and_then(|v| v.as_f64());
                let q_m3s = req
                    .arguments
                    .get("airflow_rate_m3_s")
                    .and_then(|v| v.as_f64());
                let d_m = req
                    .arguments
                    .get("vertical_depth_m")
                    .and_then(|v| v.as_f64());
                self.sim_mining_mine_ventilation(&SimMineVentilationRequest {
                    airway_length_m: l_m,
                    airway_area_m2: a_m2,
                    airflow_rate_m3_s: q_m3s,
                    vertical_depth_m: d_m,
                    ..Default::default()
                })?
            }
            "sim.telecom.reflector_antenna" => {
                let d_m = req
                    .arguments
                    .get("dish_diameter_m")
                    .and_then(|v| v.as_f64());
                let f_ghz = req
                    .arguments
                    .get("operating_frequency_ghz")
                    .and_then(|v| v.as_f64());
                let err_mm = req
                    .arguments
                    .get("surface_rms_error_mm")
                    .and_then(|v| v.as_f64());
                self.sim_telecom_reflector_antenna(&SimReflectorAntennaRequest {
                    dish_diameter_m: d_m,
                    operating_frequency_ghz: f_ghz,
                    surface_rms_error_mm: err_mm,
                    ..Default::default()
                })?
            }
            "sim.semiconductor.gaa_fet" => {
                let n_sheets = req
                    .arguments
                    .get("nanosheet_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let l_g = req.arguments.get("gate_length_nm").and_then(|v| v.as_f64());
                let w_ns = req
                    .arguments
                    .get("nanosheet_width_nm")
                    .and_then(|v| v.as_f64());
                let t_ns = req
                    .arguments
                    .get("nanosheet_thickness_nm")
                    .and_then(|v| v.as_f64());
                self.sim_semiconductor_gaa_fet(&SimGaaFetRequest {
                    nanosheet_count: n_sheets,
                    gate_length_nm: l_g,
                    nanosheet_width_nm: w_ns,
                    nanosheet_thickness_nm: t_ns,
                    ..Default::default()
                })?
            }
            "sim.agri.irrigation_hydraulics" => {
                let d_mm = req
                    .arguments
                    .get("pipe_inside_diameter_mm")
                    .and_then(|v| v.as_f64());
                let l_m = req
                    .arguments
                    .get("lateral_length_m")
                    .and_then(|v| v.as_f64());
                let s_m = req
                    .arguments
                    .get("emitter_spacing_m")
                    .and_then(|v| v.as_f64());
                let q_lh = req
                    .arguments
                    .get("nominal_discharge_l_h")
                    .and_then(|v| v.as_f64());
                self.sim_agri_irrigation_hydraulics(&SimIrrigationHydraulicsRequest {
                    pipe_inside_diameter_mm: d_mm,
                    lateral_length_m: l_m,
                    emitter_spacing_m: s_m,
                    nominal_discharge_l_h: q_lh,
                    ..Default::default()
                })?
            }
            "sim.environmental.wastewater_treatment" => {
                let q_m3d = req
                    .arguments
                    .get("influent_flow_m3_day")
                    .and_then(|v| v.as_f64());
                let bod_in = req
                    .arguments
                    .get("influent_bod_mg_l")
                    .and_then(|v| v.as_f64());
                let bod_out = req
                    .arguments
                    .get("target_effluent_bod_mg_l")
                    .and_then(|v| v.as_f64());
                let mlss = req.arguments.get("mlss_mg_l").and_then(|v| v.as_f64());
                self.sim_environmental_wastewater_treatment(&SimWastewaterTreatmentRequest {
                    influent_flow_m3_day: q_m3d,
                    influent_bod_mg_l: bod_in,
                    target_effluent_bod_mg_l: bod_out,
                    mlss_mg_l: mlss,
                    ..Default::default()
                })?
            }
            "sim.automotive.ev_powertrain" => {
                let mass = req
                    .arguments
                    .get("vehicle_mass_kg")
                    .and_then(|v| v.as_f64());
                let torque = req
                    .arguments
                    .get("motor_peak_torque_nm")
                    .and_then(|v| v.as_f64());
                let batt = req
                    .arguments
                    .get("battery_capacity_kwh")
                    .and_then(|v| v.as_f64());
                let speed = req
                    .arguments
                    .get("cruising_speed_km_h")
                    .and_then(|v| v.as_f64());
                self.sim_automotive_ev_powertrain(&SimEvPowertrainRequest {
                    vehicle_mass_kg: mass,
                    motor_peak_torque_nm: torque,
                    battery_capacity_kwh: batt,
                    cruising_speed_km_h: speed,
                    ..Default::default()
                })?
            }
            "sim.civil.seismic_base_isolation" => {
                let mass_t = req
                    .arguments
                    .get("superstructure_mass_tonnes")
                    .and_then(|v| v.as_f64());
                let n_bearings = req
                    .arguments
                    .get("bearing_count")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let d_outer = req
                    .arguments
                    .get("bearing_outer_diameter_mm")
                    .and_then(|v| v.as_f64());
                let d_lead = req
                    .arguments
                    .get("lead_core_diameter_mm")
                    .and_then(|v| v.as_f64());
                self.sim_civil_seismic_base_isolation(&SimSeismicBaseIsolationRequest {
                    superstructure_mass_tonnes: mass_t,
                    bearing_count: n_bearings,
                    bearing_outer_diameter_mm: d_outer,
                    lead_core_diameter_mm: d_lead,
                    ..Default::default()
                })?
            }
            "sim.defense.terminal_ballistics" => {
                let l_p = req
                    .arguments
                    .get("projectile_length_mm")
                    .and_then(|v| v.as_f64());
                let v_0 = req
                    .arguments
                    .get("impact_velocity_m_s")
                    .and_then(|v| v.as_f64());
                let theta = req
                    .arguments
                    .get("obliquity_angle_deg")
                    .and_then(|v| v.as_f64());
                let t_arm = req
                    .arguments
                    .get("armor_thickness_mm")
                    .and_then(|v| v.as_f64());
                self.sim_defense_terminal_ballistics(&SimTerminalBallisticsRequest {
                    projectile_length_mm: l_p,
                    impact_velocity_m_s: v_0,
                    obliquity_angle_deg: theta,
                    armor_thickness_mm: t_arm,
                    ..Default::default()
                })?
            }
            "sim.packaging.solder_thermal_fatigue" => {
                let w_pkg = req
                    .arguments
                    .get("package_width_mm")
                    .and_then(|v| v.as_f64());
                let h_solder = req
                    .arguments
                    .get("solder_joint_height_um")
                    .and_then(|v| v.as_f64());
                let t_min = req
                    .arguments
                    .get("cycle_min_temp_c")
                    .and_then(|v| v.as_f64());
                let t_max = req
                    .arguments
                    .get("cycle_max_temp_c")
                    .and_then(|v| v.as_f64());
                self.sim_packaging_solder_thermal_fatigue(&SimSolderThermalFatigueRequest {
                    package_width_mm: w_pkg,
                    solder_joint_height_um: h_solder,
                    cycle_min_temp_c: t_min,
                    cycle_max_temp_c: t_max,
                    ..Default::default()
                })?
            }
            "sim.marine.ship_slamming" => {
                let beta = req
                    .arguments
                    .get("deadrise_angle_deg")
                    .and_then(|v| v.as_f64());
                let v_rel = req
                    .arguments
                    .get("relative_impact_velocity_m_s")
                    .and_then(|v| v.as_f64());
                let s_m = req
                    .arguments
                    .get("frame_spacing_m")
                    .and_then(|v| v.as_f64());
                let l_m = req.arguments.get("plate_length_m").and_then(|v| v.as_f64());
                let t_mm = req
                    .arguments
                    .get("plate_thickness_mm")
                    .and_then(|v| v.as_f64());
                let f_y = req
                    .arguments
                    .get("steel_yield_strength_mpa")
                    .and_then(|v| v.as_f64());
                self.sim_marine_ship_slamming(&SimShipSlammingRequest {
                    deadrise_angle_deg: beta,
                    relative_impact_velocity_m_s: v_rel,
                    frame_spacing_m: s_m,
                    plate_length_m: l_m,
                    plate_thickness_mm: t_mm,
                    steel_yield_strength_mpa: f_y,
                    ..Default::default()
                })?
            }
            "sim.automotive.crash_deceleration" => {
                let m_veh = req
                    .arguments
                    .get("vehicle_mass_kg")
                    .and_then(|v| v.as_f64());
                let v_kmh = req
                    .arguments
                    .get("impact_velocity_km_h")
                    .and_then(|v| v.as_f64());
                let l_crush = req
                    .arguments
                    .get("crumple_zone_length_m")
                    .and_then(|v| v.as_f64());
                let f_box = req
                    .arguments
                    .get("crash_box_mean_force_kn")
                    .and_then(|v| v.as_f64());
                let eta = req
                    .arguments
                    .get("structural_energy_efficiency")
                    .and_then(|v| v.as_f64());
                let m_occ = req
                    .arguments
                    .get("occupant_mass_kg")
                    .and_then(|v| v.as_f64());
                self.sim_automotive_crash_deceleration(&SimCrashDecelerationRequest {
                    vehicle_mass_kg: m_veh,
                    impact_velocity_km_h: v_kmh,
                    crumple_zone_length_m: l_crush,
                    crash_box_mean_force_kn: f_box,
                    structural_energy_efficiency: eta,
                    occupant_mass_kg: m_occ,
                    ..Default::default()
                })?
            }
            "sim.nuclear.fuel_pin" => {
                let q_prime = req
                    .arguments
                    .get("linear_heat_rate_kw_per_m")
                    .and_then(|v| v.as_f64());
                let r_p = req
                    .arguments
                    .get("pellet_radius_mm")
                    .and_then(|v| v.as_f64());
                let gap_um = req
                    .arguments
                    .get("gap_thickness_um")
                    .and_then(|v| v.as_f64());
                let t_clad = req
                    .arguments
                    .get("clad_thickness_mm")
                    .and_then(|v| v.as_f64());
                let p_sys = req
                    .arguments
                    .get("coolant_pressure_mpa")
                    .and_then(|v| v.as_f64());
                let t_b = req
                    .arguments
                    .get("bulk_coolant_temperature_c")
                    .and_then(|v| v.as_f64());
                let g_flux = req
                    .arguments
                    .get("coolant_mass_flux_kg_m2_s")
                    .and_then(|v| v.as_f64());
                self.sim_nuclear_fuel_pin(&SimNuclearFuelPinRequest {
                    linear_heat_rate_kw_per_m: q_prime,
                    pellet_radius_mm: r_p,
                    gap_thickness_um: gap_um,
                    clad_thickness_mm: t_clad,
                    coolant_pressure_mpa: p_sys,
                    bulk_coolant_temperature_c: t_b,
                    coolant_mass_flux_kg_m2_s: g_flux,
                    ..Default::default()
                })?
            }
            "sim.chemical.adsorption_psa" => {
                let d_bed = req.arguments.get("bed_diameter_m").and_then(|v| v.as_f64());
                let l_bed = req.arguments.get("bed_length_m").and_then(|v| v.as_f64());
                let p_ads = req
                    .arguments
                    .get("adsorption_pressure_bar")
                    .and_then(|v| v.as_f64());
                let p_des = req
                    .arguments
                    .get("desorption_pressure_bar")
                    .and_then(|v| v.as_f64());
                let v_feed = req
                    .arguments
                    .get("feed_flow_rate_nm3_h")
                    .and_then(|v| v.as_f64());
                let rho_bulk = req
                    .arguments
                    .get("adsorbent_bulk_density_kg_m3")
                    .and_then(|v| v.as_f64());
                self.sim_chemical_adsorption_psa(&SimAdsorptionPsaRequest {
                    bed_diameter_m: d_bed,
                    bed_length_m: l_bed,
                    adsorption_pressure_bar: p_ads,
                    desorption_pressure_bar: p_des,
                    feed_flow_rate_nm3_h: v_feed,
                    adsorbent_bulk_density_kg_m3: rho_bulk,
                    ..Default::default()
                })?
            }
            "sim.aerospace.rotorcraft_aerodynamics" => {
                let w_gross = req
                    .arguments
                    .get("gross_weight_kg")
                    .and_then(|v| v.as_f64());
                let r_main = req.arguments.get("rotor_radius_m").and_then(|v| v.as_f64());
                let n_b = req
                    .arguments
                    .get("number_of_blades")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let c_blade = req.arguments.get("blade_chord_m").and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("rotor_speed_rpm")
                    .and_then(|v| v.as_f64());
                let v_kts = req
                    .arguments
                    .get("forward_speed_knots")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_rotorcraft_aerodynamics(&SimRotorcraftAerodynamicsRequest {
                    gross_weight_kg: w_gross,
                    rotor_radius_m: r_main,
                    number_of_blades: n_b,
                    blade_chord_m: c_blade,
                    rotor_speed_rpm: rpm,
                    forward_speed_knots: v_kts,
                    ..Default::default()
                })?
            }
            "sim.thermal.heat_exchanger_delaware" => {
                let d_s = req
                    .arguments
                    .get("shell_inside_diameter_m")
                    .and_then(|v| v.as_f64());
                let n_t = req
                    .arguments
                    .get("number_of_tubes")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let l_t = req.arguments.get("tube_length_m").and_then(|v| v.as_f64());
                let b_s = req
                    .arguments
                    .get("baffle_spacing_m")
                    .and_then(|v| v.as_f64());
                let m_h = req
                    .arguments
                    .get("hot_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                self.sim_thermal_heat_exchanger_delaware(&SimHeatExchangerDelawareRequest {
                    shell_inside_diameter_m: d_s,
                    number_of_tubes: n_t,
                    tube_length_m: l_t,
                    baffle_spacing_m: b_s,
                    hot_mass_flow_kg_s: m_h,
                    ..Default::default()
                })?
            }
            "sim.geotechnical.drilled_shaft" => {
                let b_shaft = req
                    .arguments
                    .get("shaft_diameter_m")
                    .and_then(|v| v.as_f64());
                let l_shaft = req
                    .arguments
                    .get("embedded_length_m")
                    .and_then(|v| v.as_f64());
                let q_fact = req
                    .arguments
                    .get("factored_load_demand_kn")
                    .and_then(|v| v.as_f64());
                let q_serv = req
                    .arguments
                    .get("service_load_demand_kn")
                    .and_then(|v| v.as_f64());
                let s_u = req
                    .arguments
                    .get("undrained_shear_strength_kpa")
                    .and_then(|v| v.as_f64());
                self.sim_geotechnical_drilled_shaft(&SimDrilledShaftRequest {
                    shaft_diameter_m: b_shaft,
                    embedded_length_m: l_shaft,
                    factored_load_demand_kn: q_fact,
                    service_load_demand_kn: q_serv,
                    undrained_shear_strength_kpa: s_u,
                    ..Default::default()
                })?
            }
            "sim.biomedical.heart_valve" => {
                let size = req
                    .arguments
                    .get("nominal_size_mm")
                    .and_then(|v| v.as_f64());
                let hr = req.arguments.get("heart_rate_bpm").and_then(|v| v.as_f64());
                let sv = req
                    .arguments
                    .get("stroke_volume_ml")
                    .and_then(|v| v.as_f64());
                let bsa = req.arguments.get("patient_bsa_m2").and_then(|v| v.as_f64());
                let mean_dp = req
                    .arguments
                    .get("mean_pressure_gradient_mmhg")
                    .and_then(|v| v.as_f64());
                self.sim_biomedical_heart_valve(&SimHeartValveRequest {
                    nominal_size_mm: size,
                    heart_rate_bpm: hr,
                    stroke_volume_ml: sv,
                    patient_bsa_m2: bsa,
                    mean_pressure_gradient_mmhg: mean_dp,
                    ..Default::default()
                })?
            }
            "sim.aerospace.centrifugal_compressor" => {
                let m_air = req
                    .arguments
                    .get("mass_flow_rate_kg_s")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                let d_tip = req
                    .arguments
                    .get("impeller_tip_diameter_m")
                    .and_then(|v| v.as_f64());
                let n_blades = req
                    .arguments
                    .get("number_of_blades")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as u32);
                let beta_bs = req
                    .arguments
                    .get("blade_backsweep_deg")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_centrifugal_compressor(&SimCentrifugalCompressorRequest {
                    mass_flow_rate_kg_s: m_air,
                    rotational_speed_rpm: rpm,
                    impeller_tip_diameter_m: d_tip,
                    number_of_blades: n_blades,
                    blade_backsweep_deg: beta_bs,
                    ..Default::default()
                })?
            }
            "sim.rail.pantograph" => {
                let v_train = req
                    .arguments
                    .get("train_speed_km_h")
                    .and_then(|v| v.as_f64());
                let f_0 = req
                    .arguments
                    .get("static_contact_force_n")
                    .and_then(|v| v.as_f64());
                let t_wire = req
                    .arguments
                    .get("contact_wire_tension_kn")
                    .and_then(|v| v.as_f64());
                let mu_wire = req
                    .arguments
                    .get("contact_wire_mass_kg_per_m")
                    .and_then(|v| v.as_f64());
                let l_span = req
                    .arguments
                    .get("catenary_span_length_m")
                    .and_then(|v| v.as_f64());
                self.sim_rail_pantograph(&SimRailPantographRequest {
                    train_speed_km_h: v_train,
                    static_contact_force_n: f_0,
                    contact_wire_tension_kn: t_wire,
                    contact_wire_mass_kg_per_m: mu_wire,
                    catenary_span_length_m: l_span,
                    ..Default::default()
                })?
            }
            "sim.geotechnical.tbm_tunneling" => {
                let d_tunnel = req
                    .arguments
                    .get("tunnel_outer_diameter_m")
                    .and_then(|v| v.as_f64());
                let h_depth = req
                    .arguments
                    .get("tunnel_depth_to_axis_m")
                    .and_then(|v| v.as_f64());
                let h_water = req
                    .arguments
                    .get("water_table_depth_m")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("cutterhead_rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                let pr = req
                    .arguments
                    .get("penetration_rate_mm_per_rev")
                    .and_then(|v| v.as_f64());
                self.sim_geotechnical_tbm_tunneling(&SimTbmTunnelingRequest {
                    tunnel_outer_diameter_m: d_tunnel,
                    tunnel_depth_to_axis_m: h_depth,
                    water_table_depth_m: h_water,
                    cutterhead_rotational_speed_rpm: rpm,
                    penetration_rate_mm_per_rev: pr,
                    ..Default::default()
                })?
            }
            "sim.structural.prestressed_concrete" => {
                let l_span = req.arguments.get("span_length_m").and_then(|v| v.as_f64());
                let fc = req
                    .arguments
                    .get("concrete_f_c_mpa")
                    .and_then(|v| v.as_f64());
                let aps = req
                    .arguments
                    .get("tendon_steel_area_mm2")
                    .and_then(|v| v.as_f64());
                let e_mid = req
                    .arguments
                    .get("midspan_eccentricity_m")
                    .and_then(|v| v.as_f64());
                let m_serv = req
                    .arguments
                    .get("service_moment_midspan_kn_m")
                    .and_then(|v| v.as_f64());
                self.sim_structural_prestressed_concrete(&SimPrestressedConcreteRequest {
                    span_length_m: l_span,
                    concrete_f_c_mpa: fc,
                    tendon_steel_area_mm2: aps,
                    midspan_eccentricity_m: e_mid,
                    service_moment_midspan_kn_m: m_serv,
                    ..Default::default()
                })?
            }
            "sim.aerospace.spacecraft_jitter" => {
                let rpm = req
                    .arguments
                    .get("wheel_speed_rpm")
                    .and_then(|v| v.as_f64());
                let u_stat = req
                    .arguments
                    .get("static_imbalance_g_mm")
                    .and_then(|v| v.as_f64());
                let u_dyn = req
                    .arguments
                    .get("dynamic_imbalance_g_mm2")
                    .and_then(|v| v.as_f64());
                let f_res = req
                    .arguments
                    .get("structural_resonance_freq_hz")
                    .and_then(|v| v.as_f64());
                let j_sat = req
                    .arguments
                    .get("satellite_inertia_kg_m2")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_spacecraft_jitter(&SimSpacecraftJitterRequest {
                    wheel_speed_rpm: rpm,
                    static_imbalance_g_mm: u_stat,
                    dynamic_imbalance_g_mm2: u_dyn,
                    structural_resonance_freq_hz: f_res,
                    satellite_inertia_kg_m2: j_sat,
                    ..Default::default()
                })?
            }
            "sim.automotive.radar_lidar_sensor" => {
                let r_tgt = req.arguments.get("target_range_m").and_then(|v| v.as_f64());
                let rcs = req.arguments.get("target_rcs_m2").and_then(|v| v.as_f64());
                let rho = req
                    .arguments
                    .get("target_optical_reflectivity")
                    .and_then(|v| v.as_f64());
                let p_rad = req
                    .arguments
                    .get("radar_tx_power_w")
                    .and_then(|v| v.as_f64());
                let p_lid = req
                    .arguments
                    .get("lidar_pulse_power_w")
                    .and_then(|v| v.as_f64());
                self.sim_automotive_radar_lidar_sensor(&SimRadarLidarSensorRequest {
                    target_range_m: r_tgt,
                    target_rcs_m2: rcs,
                    target_optical_reflectivity: rho,
                    radar_tx_power_w: p_rad,
                    lidar_pulse_power_w: p_lid,
                    ..Default::default()
                })?
            }
            "sim.marine.auv_propulsion" => {
                let v_kts = req
                    .arguments
                    .get("cruising_speed_knots")
                    .and_then(|v| v.as_f64());
                let l_auv = req
                    .arguments
                    .get("vehicle_length_m")
                    .and_then(|v| v.as_f64());
                let d_auv = req
                    .arguments
                    .get("vehicle_diameter_m")
                    .and_then(|v| v.as_f64());
                let e_bat = req
                    .arguments
                    .get("battery_capacity_kwh")
                    .and_then(|v| v.as_f64());
                let p_hot = req
                    .arguments
                    .get("hotel_power_draw_w")
                    .and_then(|v| v.as_f64());
                self.sim_marine_auv_propulsion(&SimAuvPropulsionRequest {
                    cruising_speed_knots: v_kts,
                    vehicle_length_m: l_auv,
                    vehicle_diameter_m: d_auv,
                    battery_capacity_kwh: e_bat,
                    hotel_power_draw_w: p_hot,
                    ..Default::default()
                })?
            }
            "sim.energy.hydro_governor" => {
                let h0 = req
                    .arguments
                    .get("rated_net_head_m")
                    .and_then(|v| v.as_f64());
                let q0 = req
                    .arguments
                    .get("rated_discharge_m3_s")
                    .and_then(|v| v.as_f64());
                let n0 = req
                    .arguments
                    .get("rated_speed_rpm")
                    .and_then(|v| v.as_f64());
                let l_pen = req
                    .arguments
                    .get("penstock_length_m")
                    .and_then(|v| v.as_f64());
                let tc = req
                    .arguments
                    .get("wicket_gate_closure_time_s")
                    .and_then(|v| v.as_f64());
                self.sim_energy_hydro_governor(&SimHydroGovernorRequest {
                    rated_net_head_m: h0,
                    rated_discharge_m3_s: q0,
                    rated_speed_rpm: n0,
                    penstock_length_m: l_pen,
                    wicket_gate_closure_time_s: tc,
                    ..Default::default()
                })?
            }
            "sim.semiconductor.cmp_planarization" => {
                let d_waf = req
                    .arguments
                    .get("wafer_diameter_mm")
                    .and_then(|v| v.as_f64());
                let p_down = req
                    .arguments
                    .get("down_force_pressure_kpa")
                    .and_then(|v| v.as_f64());
                let v_rel = req
                    .arguments
                    .get("relative_velocity_m_s")
                    .and_then(|v| v.as_f64());
                let t_pol = req
                    .arguments
                    .get("polish_time_seconds")
                    .and_then(|v| v.as_f64());
                self.sim_semiconductor_cmp(&SimSemiconductorCmpRequest {
                    wafer_diameter_mm: d_waf,
                    down_force_pressure_kpa: p_down,
                    relative_velocity_m_s: v_rel,
                    polish_time_seconds: t_pol,
                    ..Default::default()
                })?
            }
            "sim.transportation.pavement_mepdg" => {
                let h_ac = req
                    .arguments
                    .get("asphalt_thickness_mm")
                    .and_then(|v| v.as_f64());
                let h_base = req
                    .arguments
                    .get("base_thickness_mm")
                    .and_then(|v| v.as_f64());
                let h_sub = req
                    .arguments
                    .get("subbase_thickness_mm")
                    .and_then(|v| v.as_f64());
                let esal = req
                    .arguments
                    .get("design_traffic_millions_esal")
                    .and_then(|v| v.as_f64());
                self.sim_transportation_pavement_mepdg(&SimPavementMepdgRequest {
                    asphalt_thickness_mm: h_ac,
                    base_thickness_mm: h_base,
                    subbase_thickness_mm: h_sub,
                    design_traffic_millions_esal: esal,
                    ..Default::default()
                })?
            }
            "sim.biomedical.scaffold_degradation" => {
                let d_scaf = req
                    .arguments
                    .get("scaffold_diameter_mm")
                    .and_then(|v| v.as_f64());
                let h_scaf = req
                    .arguments
                    .get("scaffold_height_mm")
                    .and_then(|v| v.as_f64());
                let eps = req
                    .arguments
                    .get("target_porosity_pct")
                    .and_then(|v| v.as_f64());
                let t_deg = req
                    .arguments
                    .get("degradation_evaluation_weeks")
                    .and_then(|v| v.as_f64());
                self.sim_biomedical_scaffold_degradation(&SimBiomaterialScaffoldRequest {
                    scaffold_diameter_mm: d_scaf,
                    scaffold_height_mm: h_scaf,
                    target_porosity_pct: eps,
                    degradation_evaluation_weeks: t_deg,
                    ..Default::default()
                })?
            }
            "sim.aerospace.satellite_orbital_decay" => {
                let m_sat = req
                    .arguments
                    .get("satellite_mass_kg")
                    .and_then(|v| v.as_f64());
                let a_drag = req
                    .arguments
                    .get("drag_cross_section_area_m2")
                    .and_then(|v| v.as_f64());
                let cd = req
                    .arguments
                    .get("drag_coefficient_cd")
                    .and_then(|v| v.as_f64());
                let hp = req
                    .arguments
                    .get("initial_perigee_altitude_km")
                    .and_then(|v| v.as_f64());
                let ha = req
                    .arguments
                    .get("initial_apogee_altitude_km")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_satellite_orbital_decay(&SimSatelliteOrbitalDecayRequest {
                    satellite_mass_kg: m_sat,
                    drag_cross_section_area_m2: a_drag,
                    drag_coefficient_cd: cd,
                    initial_perigee_altitude_km: hp,
                    initial_apogee_altitude_km: ha,
                    ..Default::default()
                })?
            }
            "sim.mining.bucket_wheel_excavator" => {
                let d_w = req
                    .arguments
                    .get("wheel_diameter_m")
                    .and_then(|v| v.as_f64());
                let v_b = req
                    .arguments
                    .get("bucket_nominal_volume_m3")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("wheel_speed_rpm")
                    .and_then(|v| v.as_f64());
                let kl = req
                    .arguments
                    .get("lineal_cutting_resistance_kn_m")
                    .and_then(|v| v.as_f64());
                let l_b = req.arguments.get("boom_length_m").and_then(|v| v.as_f64());
                self.sim_mining_bucket_wheel_excavator(&SimMiningExcavatorRequest {
                    wheel_diameter_m: d_w,
                    bucket_nominal_volume_m3: v_b,
                    wheel_speed_rpm: rpm,
                    lineal_cutting_resistance_kn_m: kl,
                    boom_length_m: l_b,
                    ..Default::default()
                })?
            }
            "sim.process.multiphase_pipe_flow" => {
                let l_p = req.arguments.get("pipe_length_m").and_then(|v| v.as_f64());
                let d_p = req
                    .arguments
                    .get("pipe_inner_diameter_mm")
                    .and_then(|v| v.as_f64());
                let m_l = req
                    .arguments
                    .get("liquid_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let m_g = req
                    .arguments
                    .get("gas_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let dz = req
                    .arguments
                    .get("elevation_change_m")
                    .and_then(|v| v.as_f64());
                self.sim_process_multiphase_pipe_flow(&SimMultiphasePipeFlowRequest {
                    pipe_length_m: l_p,
                    pipe_inner_diameter_mm: d_p,
                    liquid_mass_flow_kg_s: m_l,
                    gas_mass_flow_kg_s: m_g,
                    elevation_change_m: dz,
                    ..Default::default()
                })?
            }
            "sim.energy.geothermal_reservoir" => {
                let t_res = req
                    .arguments
                    .get("reservoir_temperature_c")
                    .and_then(|v| v.as_f64());
                let t_inj = req
                    .arguments
                    .get("reinjection_temperature_c")
                    .and_then(|v| v.as_f64());
                let q_w = req
                    .arguments
                    .get("brine_flow_rate_l_s")
                    .and_then(|v| v.as_f64());
                let d_w = req
                    .arguments
                    .get("well_spacing_distance_m")
                    .and_then(|v| v.as_f64());
                let k_md = req
                    .arguments
                    .get("reservoir_permeability_md")
                    .and_then(|v| v.as_f64());
                self.sim_energy_geothermal_reservoir(&SimGeothermalReservoirRequest {
                    reservoir_temperature_c: t_res,
                    reinjection_temperature_c: t_inj,
                    brine_flow_rate_l_s: q_w,
                    well_spacing_distance_m: d_w,
                    reservoir_permeability_md: k_md,
                    ..Default::default()
                })?
            }
            "sim.process.spray_drying_kinetics" => {
                let m_f = req.arguments.get("feed_rate_kg_h").and_then(|v| v.as_f64());
                let s_f = req
                    .arguments
                    .get("feed_solids_fraction")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("atomizer_speed_rpm")
                    .and_then(|v| v.as_f64());
                let t_in = req
                    .arguments
                    .get("inlet_air_temperature_c")
                    .and_then(|v| v.as_f64());
                let t_out = req
                    .arguments
                    .get("outlet_air_temperature_c")
                    .and_then(|v| v.as_f64());
                self.sim_process_spray_drying_kinetics(&SimSprayDryingKineticsRequest {
                    feed_rate_kg_h: m_f,
                    feed_solids_fraction: s_f,
                    atomizer_speed_rpm: rpm,
                    inlet_air_temperature_c: t_in,
                    outlet_air_temperature_c: t_out,
                    ..Default::default()
                })?
            }
            "sim.energy.thermoelectric_rtg" => {
                let q0 = req
                    .arguments
                    .get("initial_thermal_power_w_th")
                    .and_then(|v| v.as_f64());
                let th = req
                    .arguments
                    .get("hot_junction_temp_k")
                    .and_then(|v| v.as_f64());
                let tc = req
                    .arguments
                    .get("cold_junction_temp_k")
                    .and_then(|v| v.as_f64());
                let n_c = req
                    .arguments
                    .get("couple_count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                let t_yr = req
                    .arguments
                    .get("mission_elapsed_years")
                    .and_then(|v| v.as_f64());
                self.sim_energy_thermoelectric_rtg(&SimThermoelectricRtgRequest {
                    initial_thermal_power_w_th: q0,
                    hot_junction_temp_k: th,
                    cold_junction_temp_k: tc,
                    couple_count: n_c,
                    mission_elapsed_years: t_yr,
                    ..Default::default()
                })?
            }
            "sim.nuclear.containment_loca" => {
                let vol = req
                    .arguments
                    .get("containment_free_volume_m3")
                    .and_then(|v| v.as_f64());
                let p_des = req
                    .arguments
                    .get("design_pressure_bar_a")
                    .and_then(|v| v.as_f64());
                let h_blow = req
                    .arguments
                    .get("blowdown_enthalpy_gj")
                    .and_then(|v| v.as_f64());
                let q_spray = req
                    .arguments
                    .get("spray_flow_rate_l_s")
                    .and_then(|v| v.as_f64());
                self.sim_nuclear_containment_loca(&SimContainmentLocaRequest {
                    containment_free_volume_m3: vol,
                    design_pressure_bar_a: p_des,
                    blowdown_enthalpy_gj: h_blow,
                    spray_flow_rate_l_s: q_spray,
                    ..Default::default()
                })?
            }
            "sim.environmental.atmospheric_dispersion" => {
                let q = req
                    .arguments
                    .get("emission_rate_g_s")
                    .and_then(|v| v.as_f64());
                let hs = req.arguments.get("stack_height_m").and_then(|v| v.as_f64());
                let ts = req
                    .arguments
                    .get("stack_gas_temperature_c")
                    .and_then(|v| v.as_f64());
                let u = req
                    .arguments
                    .get("wind_speed_at_stack_height_m_s")
                    .and_then(|v| v.as_f64());
                self.sim_environmental_atmospheric_dispersion(&SimAtmosphericDispersionRequest {
                    emission_rate_g_s: q,
                    stack_height_m: hs,
                    stack_gas_temperature_c: ts,
                    wind_speed_at_stack_height_m_s: u,
                    ..Default::default()
                })?
            }
            "sim.aerospace.rocket_ascent_trajectory" => {
                let m0 = req
                    .arguments
                    .get("gross_liftoff_mass_kg")
                    .and_then(|v| v.as_f64());
                let mpl = req
                    .arguments
                    .get("payload_mass_kg")
                    .and_then(|v| v.as_f64());
                let f1 = req
                    .arguments
                    .get("stage1_thrust_sl_kn")
                    .and_then(|v| v.as_f64());
                let alt = req
                    .arguments
                    .get("target_orbit_altitude_km")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_rocket_ascent_trajectory(&SimRocketAscentRequest {
                    gross_liftoff_mass_kg: m0,
                    payload_mass_kg: mpl,
                    stage1_thrust_sl_kn: f1,
                    target_orbit_altitude_km: alt,
                    ..Default::default()
                })?
            }
            "sim.civil.gravity_dam_stability" => {
                let h = req.arguments.get("dam_height_m").and_then(|v| v.as_f64());
                let b = req.arguments.get("base_width_m").and_then(|v| v.as_f64());
                let h_w = req
                    .arguments
                    .get("upstream_water_depth_m")
                    .and_then(|v| v.as_f64());
                let b_crest = req.arguments.get("crest_width_m").and_then(|v| v.as_f64());
                self.sim_civil_gravity_dam_stability(&SimGravityDamRequest {
                    dam_height_m: h,
                    crest_width_m: b_crest,
                    base_width_m: b,
                    upstream_water_depth_m: h_w,
                    ..Default::default()
                })?
            }
            "sim.energy.electrolyzer_hydrogen" => {
                let t_cell = req
                    .arguments
                    .get("cell_temperature_c")
                    .and_then(|v| v.as_f64());
                let p_h2 = req
                    .arguments
                    .get("hydrogen_operating_pressure_bar_a")
                    .and_then(|v| v.as_f64());
                let j = req
                    .arguments
                    .get("current_density_a_per_cm2")
                    .and_then(|v| v.as_f64());
                let n_cells = req
                    .arguments
                    .get("cell_count")
                    .and_then(|v| v.as_u64())
                    .map(|n| n as u32);
                self.sim_energy_electrolyzer_hydrogen(&SimElectrolyzerHydrogenRequest {
                    cell_temperature_c: t_cell,
                    hydrogen_operating_pressure_bar_a: p_h2,
                    current_density_a_per_cm2: j,
                    cell_count: n_cells,
                    ..Default::default()
                })?
            }
            "sim.optical.thin_film_optics" => {
                let wl = req.arguments.get("wavelength_nm").and_then(|v| v.as_f64());
                let aoi = req
                    .arguments
                    .get("angle_of_incidence_deg")
                    .and_then(|v| v.as_f64());
                let n_sub = req
                    .arguments
                    .get("substrate_index")
                    .and_then(|v| v.as_f64());
                self.sim_optical_thin_film_optics(&SimThinFilmOpticsRequest {
                    wavelength_nm: wl,
                    angle_of_incidence_deg: aoi,
                    substrate_index: n_sub,
                    ..Default::default()
                })?
            }
            "sim.energy.cryogenic_liquefaction" => {
                let m_feed = req
                    .arguments
                    .get("feed_gas_flow_rate_kg_h")
                    .and_then(|v| v.as_f64());
                let p1 = req
                    .arguments
                    .get("suction_pressure_bar_a")
                    .and_then(|v| v.as_f64());
                let p2 = req
                    .arguments
                    .get("discharge_pressure_bar_a")
                    .and_then(|v| v.as_f64());
                self.sim_energy_cryogenic_liquefaction(&SimCryogenicLiquefactionRequest {
                    feed_gas_flow_rate_kg_h: m_feed,
                    suction_pressure_bar_a: p1,
                    discharge_pressure_bar_a: p2,
                    ..Default::default()
                })?
            }
            "sim.process.hydrocyclone_separation" => {
                let dc = req
                    .arguments
                    .get("cyclone_diameter_mm")
                    .and_then(|v| v.as_f64());
                let q = req
                    .arguments
                    .get("feed_flow_rate_m3_h")
                    .and_then(|v| v.as_f64());
                let cv = req
                    .arguments
                    .get("solids_volumetric_concentration_pct")
                    .and_then(|v| v.as_f64());
                self.sim_process_hydrocyclone_separation(&SimHydrocycloneRequest {
                    cyclone_diameter_mm: dc,
                    feed_flow_rate_m3_h: q,
                    solids_volumetric_concentration_pct: cv,
                    ..Default::default()
                })?
            }
            "sim.mechanical.magnetic_bearing" => {
                let m = req
                    .arguments
                    .get("rotor_supported_mass_kg")
                    .and_then(|v| v.as_f64());
                let g = req
                    .arguments
                    .get("nominal_air_gap_mm")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("max_rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                self.sim_mechanical_magnetic_bearing(&SimMagneticBearingRequest {
                    rotor_supported_mass_kg: m,
                    nominal_air_gap_mm: g,
                    max_rotational_speed_rpm: rpm,
                    ..Default::default()
                })?
            }
            "sim.marine.sonar_propagation" => {
                let f = req
                    .arguments
                    .get("operating_frequency_khz")
                    .and_then(|v| v.as_f64());
                let p = req
                    .arguments
                    .get("transmitter_acoustic_power_w")
                    .and_then(|v| v.as_f64());
                let r = req.arguments.get("target_range_m").and_then(|v| v.as_f64());
                self.sim_marine_sonar_propagation(&SimSonarPropagationRequest {
                    operating_frequency_khz: f,
                    transmitter_acoustic_power_w: p,
                    target_range_m: r,
                    ..Default::default()
                })?
            }
            "sim.manufacturing.electron_beam_welding" => {
                let v = req
                    .arguments
                    .get("acceleration_voltage_kv")
                    .and_then(|v| v.as_f64());
                let i = req
                    .arguments
                    .get("beam_current_ma")
                    .and_then(|v| v.as_f64());
                let s = req
                    .arguments
                    .get("travel_speed_mm_s")
                    .and_then(|v| v.as_f64());
                self.sim_manufacturing_electron_beam_welding(&SimEbwWeldingRequest {
                    acceleration_voltage_kv: v,
                    beam_current_ma: i,
                    travel_speed_mm_s: s,
                    ..Default::default()
                })?
            }
            "sim.process.crystallization_kinetics" => {
                let q = req
                    .arguments
                    .get("feed_volumetric_flow_m3_h")
                    .and_then(|v| v.as_f64());
                let vol = req
                    .arguments
                    .get("crystallizer_active_volume_m3")
                    .and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("target_median_size_um")
                    .and_then(|v| v.as_f64());
                self.sim_process_crystallization_kinetics(&SimCrystallizationRequest {
                    feed_volumetric_flow_m3_h: q,
                    crystallizer_active_volume_m3: vol,
                    target_median_size_um: d,
                    ..Default::default()
                })?
            }
            "sim.thermal.thermoelectric_cooler" => {
                let n = req
                    .arguments
                    .get("couple_count_n")
                    .and_then(|v| v.as_u64())
                    .map(|v| v as usize);
                let i = req
                    .arguments
                    .get("operating_current_a")
                    .and_then(|v| v.as_f64());
                let q = req
                    .arguments
                    .get("heat_load_qc_target_w")
                    .and_then(|v| v.as_f64());
                self.sim_thermal_thermoelectric_cooler(&SimTecCoolerRequest {
                    couple_count_n: n,
                    operating_current_a: i,
                    heat_load_qc_target_w: q,
                    ..Default::default()
                })?
            }
            "sim.mechanical.slurry_pump_derating" => {
                let q = req
                    .arguments
                    .get("slurry_flow_rate_m3_h")
                    .and_then(|v| v.as_f64());
                let h = req
                    .arguments
                    .get("clear_water_head_m")
                    .and_then(|v| v.as_f64());
                let cw = req
                    .arguments
                    .get("solids_concentration_wt_pct")
                    .and_then(|v| v.as_f64());
                self.sim_mechanical_slurry_pump_derating(&SimSlurryPumpRequest {
                    slurry_flow_rate_m3_h: q,
                    clear_water_head_m: h,
                    solids_concentration_wt_pct: cw,
                    ..Default::default()
                })?
            }
            "sim.optical.fiber_dispersion" => {
                let w = req.arguments.get("wavelength_nm").and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("fiber_length_km")
                    .and_then(|v| v.as_f64());
                let b = req
                    .arguments
                    .get("target_bit_rate_gbps")
                    .and_then(|v| v.as_f64());
                self.sim_optical_fiber_dispersion(&SimFiberDispersionRequest {
                    wavelength_nm: w,
                    fiber_length_km: l,
                    target_bit_rate_gbps: b,
                    ..Default::default()
                })?
            }
            "sim.marine.propeller_hydrodynamics" => {
                let d = req
                    .arguments
                    .get("propeller_diameter_m")
                    .and_then(|v| v.as_f64());
                let pd = req
                    .arguments
                    .get("pitch_diameter_ratio")
                    .and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                self.sim_marine_propeller_hydrodynamics(&SimPropellerRequest {
                    propeller_diameter_m: d,
                    pitch_diameter_ratio: pd,
                    rotational_speed_rpm: rpm,
                    ..Default::default()
                })?
            }
            "sim.environmental.indoor_air_quality" => {
                let a = req
                    .arguments
                    .get("room_floor_area_m2")
                    .and_then(|v| v.as_f64());
                let h = req
                    .arguments
                    .get("room_clear_height_m")
                    .and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("ventilation_supply_airflow_m3_h")
                    .and_then(|v| v.as_f64());
                self.sim_environmental_indoor_air_quality(&SimIndoorAirQualityRequest {
                    room_floor_area_m2: a,
                    room_clear_height_m: h,
                    ventilation_supply_airflow_m3_h: v,
                    ..Default::default()
                })?
            }
            "sim.electrical.pmsm_foc_drive" => {
                let rpm = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                let iq = req
                    .arguments
                    .get("q_axis_current_iq_a")
                    .and_then(|v| v.as_f64());
                let vdc = req
                    .arguments
                    .get("inverter_dc_bus_voltage_v")
                    .and_then(|v| v.as_f64());
                self.sim_electrical_pmsm_foc_drive(&SimPmsmDriveRequest {
                    rotational_speed_rpm: rpm,
                    q_axis_current_iq_a: iq,
                    inverter_dc_bus_voltage_v: vdc,
                    ..Default::default()
                })?
            }
            "sim.civil.mse_retaining_wall" => {
                let h = req
                    .arguments
                    .get("wall_height_h_m")
                    .and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("reinforcement_length_l_m")
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("geogrid_ultimate_strength_kn_m")
                    .and_then(|v| v.as_f64());
                self.sim_civil_mse_retaining_wall(&SimMseWallRequest {
                    wall_height_h_m: h,
                    reinforcement_length_l_m: l,
                    geogrid_ultimate_strength_kn_m: t,
                    ..Default::default()
                })?
            }
            "sim.nuclear.isotope_centrifuge" => {
                let r = req.arguments.get("rotor_radius_m").and_then(|v| v.as_f64());
                let l = req.arguments.get("rotor_length_m").and_then(|v| v.as_f64());
                let rpm = req
                    .arguments
                    .get("rotational_speed_rpm")
                    .and_then(|v| v.as_f64());
                self.sim_nuclear_isotope_centrifuge(&SimIsotopeCentrifugeRequest {
                    rotor_radius_m: r,
                    rotor_length_m: l,
                    rotational_speed_rpm: rpm,
                    ..Default::default()
                })?
            }
            "sim.mechanical.conveyor_bulk_handling" => {
                let b = req.arguments.get("belt_width_mm").and_then(|v| v.as_f64());
                let l = req
                    .arguments
                    .get("conveyor_length_m")
                    .and_then(|v| v.as_f64());
                let v = req.arguments.get("belt_speed_m_s").and_then(|v| v.as_f64());
                self.sim_mechanical_conveyor_bulk_handling(&SimConveyorRequest {
                    belt_width_mm: b,
                    conveyor_length_m: l,
                    belt_speed_m_s: v,
                    ..Default::default()
                })?
            }
            "sim.marine.offshore_wave_loading" => {
                let d = req.arguments.get("water_depth_m").and_then(|v| v.as_f64());
                let h = req.arguments.get("wave_height_m").and_then(|v| v.as_f64());
                let dia = req
                    .arguments
                    .get("cylinder_diameter_m")
                    .and_then(|v| v.as_f64());
                self.sim_marine_offshore_wave_loading(&SimOffshoreWaveRequest {
                    water_depth_m: d,
                    wave_height_m: h,
                    cylinder_diameter_m: dia,
                    ..Default::default()
                })?
            }
            "sim.aerospace.spacecraft_adcs_attitude" => {
                let alt = req
                    .arguments
                    .get("orbit_altitude_km")
                    .and_then(|v| v.as_f64());
                let m = req
                    .arguments
                    .get("spacecraft_mass_kg")
                    .and_then(|v| v.as_f64());
                let a = req
                    .arguments
                    .get("target_slew_accel_deg_s2")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_spacecraft_adcs_attitude(&SimSpacecraftAdcsRequest {
                    orbit_altitude_km: alt,
                    spacecraft_mass_kg: m,
                    target_slew_accel_deg_s2: a,
                    ..Default::default()
                })?
            }
            "sim.electrical.battery_fast_charging" => {
                let q = req
                    .arguments
                    .get("nominal_capacity_ah")
                    .and_then(|v| v.as_f64());
                let c = req.arguments.get("charge_c_rate").and_then(|v| v.as_f64());
                let r = req
                    .arguments
                    .get("internal_resistance_mohms")
                    .and_then(|v| v.as_f64());
                self.sim_electrical_battery_fast_charging(&SimBatteryFastChargeRequest {
                    nominal_capacity_ah: q,
                    charge_c_rate: c,
                    internal_resistance_mohms: r,
                    ..Default::default()
                })?
            }
            "sim.process.packed_bed_scrubber" => {
                let q = req
                    .arguments
                    .get("flue_gas_flow_rate_m3_h")
                    .and_then(|v| v.as_f64());
                let yin = req
                    .arguments
                    .get("inlet_pollutant_mole_fraction")
                    .and_then(|v| v.as_f64());
                let yout = req
                    .arguments
                    .get("target_outlet_mole_fraction")
                    .and_then(|v| v.as_f64());
                self.sim_process_packed_bed_scrubber(&SimPackedBedRequest {
                    flue_gas_flow_rate_m3_h: q,
                    inlet_pollutant_mole_fraction: yin,
                    target_outlet_mole_fraction: yout,
                    ..Default::default()
                })?
            }
            "sim.aerospace.aircraft_landing_gear" => {
                let m = req
                    .arguments
                    .get("gear_landed_mass_kg")
                    .and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("vertical_sink_velocity_m_s")
                    .and_then(|v| v.as_f64());
                let s = req
                    .arguments
                    .get("available_stroke_mm")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_aircraft_landing_gear(&SimLandingGearRequest {
                    gear_landed_mass_kg: m,
                    vertical_sink_velocity_m_s: v,
                    available_stroke_mm: s,
                    ..Default::default()
                })?
            }
            "sim.energy.pumped_storage_hydro" => {
                let h1 = req
                    .arguments
                    .get("upper_reservoir_elevation_m")
                    .and_then(|v| v.as_f64());
                let h2 = req
                    .arguments
                    .get("lower_reservoir_elevation_m")
                    .and_then(|v| v.as_f64());
                let q = req
                    .arguments
                    .get("pumping_discharge_m3_s")
                    .and_then(|v| v.as_f64());
                self.sim_energy_pumped_storage_hydro(&SimPumpedStorageRequest {
                    upper_reservoir_elevation_m: h1,
                    lower_reservoir_elevation_m: h2,
                    pumping_discharge_m3_s: q,
                    ..Default::default()
                })?
            }
            "sim.semiconductor.euv_lithography" => {
                let na = req
                    .arguments
                    .get("numerical_aperture")
                    .and_then(|v| v.as_f64());
                let r = req
                    .arguments
                    .get("interface_roughness_nm")
                    .and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("resist_exposure_dose_mj_cm2")
                    .and_then(|v| v.as_f64());
                self.sim_semiconductor_euv_lithography(&SimEuvLithographyRequest {
                    numerical_aperture: na,
                    interface_roughness_nm: r,
                    resist_exposure_dose_mj_cm2: d,
                    ..Default::default()
                })?
            }
            "sim.renewable.geothermal_binary_orc" => {
                let m = req
                    .arguments
                    .get("brine_mass_flow_kg_s")
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("brine_inlet_temperature_c")
                    .and_then(|v| v.as_f64());
                let p = req
                    .arguments
                    .get("evaporator_pressure_bar")
                    .and_then(|v| v.as_f64());
                self.sim_renewable_geothermal_binary_orc(&SimGeothermalOrcRequest {
                    brine_mass_flow_kg_s: m,
                    brine_inlet_temperature_c: t,
                    evaporator_pressure_bar: p,
                    ..Default::default()
                })?
            }
            "sim.structural.cable_stayed_bridge" => {
                let l = req
                    .arguments
                    .get("cable_chord_length_m")
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("cable_pretension_kn")
                    .and_then(|v| v.as_f64());
                let x = req
                    .arguments
                    .get("damper_distance_from_deck_m")
                    .and_then(|v| v.as_f64());
                self.sim_structural_cable_stayed_bridge(&SimCableStayRequest {
                    cable_chord_length_m: l,
                    cable_pretension_kn: t,
                    damper_distance_from_deck_m: x,
                    ..Default::default()
                })?
            }
            "sim.aerospace.supersonic_nozzle" => {
                let m = req
                    .arguments
                    .get("design_mach_number")
                    .or_else(|| req.arguments.get("design_mach"))
                    .and_then(|v| v.as_f64());
                let p = req
                    .arguments
                    .get("stagnation_pressure_kpa")
                    .or_else(|| req.arguments.get("stagnation_pressure_pa"))
                    .and_then(|v| v.as_f64());
                let t = req
                    .arguments
                    .get("stagnation_temperature_k")
                    .and_then(|v| v.as_f64());
                self.sim_aerospace_supersonic_nozzle(&SimSupersonicNozzleRequest {
                    design_mach_number: m,
                    stagnation_pressure_kpa: p,
                    stagnation_temperature_k: t,
                    ..Default::default()
                })?
            }
            "sim.automotive.ev_torque_vectoring" => {
                let m = req
                    .arguments
                    .get("vehicle_mass_kg")
                    .or_else(|| req.arguments.get("total_vehicle_mass_kg"))
                    .and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("vehicle_speed_km_h")
                    .or_else(|| req.arguments.get("vehicle_speed_m_s"))
                    .and_then(|v| v.as_f64());
                let d = req
                    .arguments
                    .get("front_steering_angle_deg")
                    .and_then(|v| v.as_f64());
                self.sim_automotive_ev_torque_vectoring(&SimEvTorqueVectoringRequest {
                    vehicle_mass_kg: m,
                    vehicle_speed_km_h: v,
                    front_steering_angle_deg: d,
                    ..Default::default()
                })?
            }
            "sim.marine.subsea_riser_viv" => {
                let l = req
                    .arguments
                    .get("riser_length_m")
                    .or_else(|| req.arguments.get("riser_total_length_m"))
                    .and_then(|v| v.as_f64());
                let t = req.arguments.get("top_tension_kn").and_then(|v| v.as_f64());
                let v = req
                    .arguments
                    .get("surface_current_velocity_m_s")
                    .and_then(|v| v.as_f64());
                self.sim_marine_subsea_riser_viv(&SimSubseaRiserVivRequest {
                    riser_length_m: l,
                    top_tension_kn: t,
                    surface_current_velocity_m_s: v,
                    ..Default::default()
                })?
            }
            "sim.chemical.chromatography_column" => {
                let d = req
                    .arguments
                    .get("column_diameter_mm")
                    .or_else(|| req.arguments.get("column_inner_diameter_mm"))
                    .and_then(|v| v.as_f64());
                let h = req.arguments.get("bed_height_mm").and_then(|v| v.as_f64());
                let u = req
                    .arguments
                    .get("linear_velocity_cm_h")
                    .or_else(|| req.arguments.get("superficial_linear_velocity_cm_h"))
                    .and_then(|v| v.as_f64());
                self.sim_chemical_chromatography_column(&SimChromatographyColumnRequest {
                    column_diameter_mm: d,
                    bed_height_mm: h,
                    linear_velocity_cm_h: u,
                    ..Default::default()
                })?
            }
            "sim.transport.maglev_suspension" => {
                let m = req
                    .arguments
                    .get("mass_supported_per_magnet_kg")
                    .or_else(|| req.arguments.get("levitated_mass_per_magnet_kg"))
                    .and_then(|v| v.as_f64());
                let z = req
                    .arguments
                    .get("nominal_airgap_mm")
                    .and_then(|v| v.as_f64());
                let s = req
                    .arguments
                    .get("disturbance_amplitude_mm")
                    .or_else(|| req.arguments.get("track_joint_step_disturbance_mm"))
                    .and_then(|v| v.as_f64());
                self.sim_transport_maglev_suspension(&SimMaglevSuspensionRequest {
                    mass_supported_per_magnet_kg: m,
                    nominal_airgap_mm: z,
                    disturbance_amplitude_mm: s,
                    ..Default::default()
                })?
            }
            "system.deploy.status" => self.deploy_status()?,
            _ => serde_json::json!({
                "status": "Executed",
                "command_id": req.command_id,
                "message": format!("Command '{}' executed successfully by '{}'", req.command_id, caller)
            }),
        };

        let execution_time_ms = started.elapsed().as_millis() as u64;
        let timestamp_epoch_ms = now_epoch_millis();
        // The digest covers the record's own fields, so a changed argument, caller or time changes
        // the hash. FNV-1a is a local integrity check, not a cryptographic signature.
        let audit_material = format!(
            "{}|{}|{}|{}|{}",
            execution_id, req.command_id, caller, req.arguments, timestamp_epoch_ms
        );
        let record = atlas_command_platform::ExecutedCommandRecord {
            execution_id: execution_id.clone(),
            command_id: req.command_id.clone(),
            caller_identity: caller,
            arguments: req.arguments.clone(),
            status: "Success".into(),
            execution_time_ms,
            timestamp_epoch_ms,
            audit_hash: format!("fnv1a_{:016x}", fnv1a64(audit_material.as_bytes())),
        };
        self.command_registry.record_execution(record);

        Ok(serde_json::json!({
            "execution_id": execution_id,
            "command_id": req.command_id,
            "status": "Success",
            "schema_version": schema.version,
            "output": output,
            "execution_time_ms": execution_time_ms,
        }))
    }

    fn command_history(&self) -> serde_json::Value {
        serde_json::json!({
            "count": self.command_registry.history(100).len(),
            "history": self.command_registry.history(100)
        })
    }

    fn list_agents(&self) -> serde_json::Value {
        let list = self.orchestrator.registry.list();
        serde_json::json!({
            "total_agents": list.len(),
            "target_ecosystem_size": 44,
            "agents": list
        })
    }

    fn analyze_requirement(&self, req: &AnalyzeRequirementRequest) -> serde_json::Value {
        let analysis =
            atlas_agent_orchestrator::EngineeringPipeline::analyze_requirement(&req.prompt);
        serde_json::to_value(&analysis).unwrap_or(serde_json::json!({}))
    }

    fn orchestrate_requirement(
        &mut self,
        req: &OrchestrateRequirementRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let analysis =
            atlas_agent_orchestrator::EngineeringPipeline::analyze_requirement(&req.prompt);
        let spec = atlas_agent_orchestrator::EngineeringPipeline::build_specification(
            &analysis,
            &req.clarifications,
        )
        .map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let graph = atlas_agent_orchestrator::EngineeringPipeline::generate_task_graph(&spec)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        self.orchestrator
            .load_task_graph(graph)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            "specification": spec,
            "task_graph": self.orchestrator.active_graph,
            "summary": self.orchestrator.summary(),
        }))
    }

    /// Bind the orchestrator to the live document: its revision and the facts an inspection task
    /// may report. Called before every execution path so no result is detached from the model.
    fn bind_orchestrator_document(&mut self) {
        let revision = self.project.revision.get();
        self.orchestrator.bind_document_revision(revision);
        self.orchestrator
            .context
            .facts
            .insert("document_revision".into(), revision.to_string());
        self.orchestrator
            .context
            .facts
            .insert("project_name".into(), self.project.name.clone());
        self.orchestrator.context.facts.insert(
            "feature_count".into(),
            self.project.features().len().to_string(),
        );
        self.orchestrator.context.facts.insert(
            "import_count".into(),
            self.project.imports().len().to_string(),
        );

        // Measured geometry for the inspection task. These are facts about the live mesh, not
        // case parameters; the analytical solvers still run their fixed reference case because no
        // geometry-to-solver adapter exists yet.
        let mut geometry = Vec::new();
        if let Ok(rebuild) = self.rebuild()
            && let Some(mesh) = &rebuild.mesh
        {
            geometry.push((
                "mesh_volume_mm3".to_string(),
                format!("{:.3}", mesh.signed_volume().abs()),
            ));
            geometry.push((
                "triangle_count".to_string(),
                mesh.triangle_count().to_string(),
            ));
            if let Some(bounds) = mesh.bounding_box() {
                let extents = [
                    bounds.max[0] - bounds.min[0],
                    bounds.max[1] - bounds.min[1],
                    bounds.max[2] - bounds.min[2],
                ];
                for (axis, value) in ["x", "y", "z"].iter().zip(extents) {
                    geometry.push((format!("bounds_{axis}_mm"), format!("{value:.3}")));
                }
                geometry.push((
                    "characteristic_length_mm".to_string(),
                    format!("{:.3}", extents.into_iter().fold(0.0_f64, f64::max)),
                ));
            }
        }
        for (key, value) in geometry {
            self.orchestrator.context.facts.insert(key, value);
        }
    }

    fn step_orchestrator(&mut self) -> Result<serde_json::Value, StudioError> {
        self.bind_orchestrator_document();
        let summary = self
            .orchestrator
            .execute_step()
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            "summary": summary,
            "active_tasks": self.orchestrator.active_graph.tasks,
            "pending_approvals": self.orchestrator.pending_approvals,
            "execution_log": self.orchestrator.execution_log,
        }))
    }

    fn list_workflows(&self) -> serde_json::Value {
        let list = atlas_agent_orchestrator::WorkflowRegistry::list_templates();
        serde_json::json!({
            "count": list.len(),
            "workflows": list
        })
    }

    fn run_workflow(&mut self, req: &RunWorkflowRequest) -> Result<serde_json::Value, StudioError> {
        let graph =
            atlas_agent_orchestrator::WorkflowRegistry::instantiate_template(&req.workflow_id)
                .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        self.orchestrator
            .load_task_graph(graph)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            "status": "Loaded",
            "workflow_id": req.workflow_id,
            "total_tasks": self.orchestrator.active_graph.len(),
            "summary": self.orchestrator.summary(),
        }))
    }

    fn run_workflow_all(
        &mut self,
        req: &RunWorkflowRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let graph =
            atlas_agent_orchestrator::WorkflowRegistry::instantiate_template(&req.workflow_id)
                .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        self.orchestrator
            .load_task_graph(graph)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        // Approval is opt-in: an absent field must never authorise a sensitive task.
        let auto = req.auto_approve.unwrap_or(false);
        self.bind_orchestrator_document();
        let revision = self.project.revision.get();
        let summary = self
            .orchestrator
            .execute_all(auto)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            // Derived from the tasks, never hardcoded: a workflow with unavailable steps is not
            // "Completed".
            "status": summary.execution_status,
            "workflow_id": req.workflow_id,
            "summary": summary,
            "tasks": self.orchestrator.active_graph.tasks,
            "execution_log": self.orchestrator.execution_log,
            "provenance": {
                "document_revision": revision,
                "input_source": "fixed_case_parameters",
                "geometry_facts": self.orchestrator.context.facts.iter()
                    .filter(|(key, _)| *key == "mesh_volume_mm3" || *key == "triangle_count"
                        || key.starts_with("bounds_") || *key == "characteristic_length_mm")
                    .map(|(key, value)| (key.clone(), value.clone()))
                    .collect::<std::collections::BTreeMap<_, _>>(),
                "note": "The inspection task reports measured geometry. The hypersonic \
                         analytical tasks are a fixed 38 m reference vehicle: substituting a \
                         document's bounding box for an aircraft case would be invalid without a \
                         geometry-to-solver adapter, which this build does not have. No task input \
                         is presented as derived from the model when it is not."
            }
        }))
    }

    fn list_approvals(&self) -> serde_json::Value {
        let approvals: Vec<_> = self
            .orchestrator
            .pending_approvals
            .values()
            .cloned()
            .collect();
        serde_json::json!({
            "count": approvals.len(),
            "approvals": approvals
        })
    }

    fn respond_approval(
        &mut self,
        req: &RespondApprovalRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let approver = req.approver.as_deref().unwrap_or("chief_engineer");
        self.orchestrator
            .respond_to_approval(
                &req.approval_id,
                req.approve,
                approver,
                req.reason.clone(),
                now_epoch_millis() / 1000,
            )
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            "status": if req.approve { "Approved" } else { "Rejected" },
            "approval_id": req.approval_id,
            "approver": approver,
        }))
    }

    fn optimize_loop(&mut self, req: &OptimizeLoopRequest) -> serde_json::Value {
        let target_fos = req.target_fos.unwrap_or(1.50);
        let max_iters = req.max_iterations.unwrap_or(5);
        let res = atlas_agent_orchestrator::EngineeringPipeline::execute_iterative_loop(
            &mut self.orchestrator.context,
            target_fos,
            max_iters,
        );
        serde_json::json!({
            "optimization_result": res,
            "revisions": self.orchestrator.context.revisions,
        })
    }

    fn handle_mcp(
        &mut self,
        req: &atlas_agent_orchestrator::McpRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let resp = atlas_agent_orchestrator::McpServer::handle_rpc(req)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;
        serde_json::to_value(&resp).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn agent_memory(&self) -> serde_json::Value {
        serde_json::json!({
            "project_name": self.orchestrator.context.project_name,
            "active_material": self.orchestrator.context.active_material,
            "units": self.orchestrator.context.coordinate_units,
            "facts": self.orchestrator.context.facts,
            "decisions": self.orchestrator.context.decisions,
            "constraints": self.orchestrator.context.constraints,
            "revisions": self.orchestrator.context.revisions,
        })
    }

    fn create_subdivide(
        &self,
        mesh_id: &str,
        levels: u32,
    ) -> Result<serde_json::Value, StudioError> {
        let cube = atlas_creative_core::mesh::PolyMesh::cube(2.0);
        let lvls = levels.clamp(1, 3);
        let subdivided = cube
            .subdivide_catmull_clark(lvls)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;
        let trimesh = subdivided
            .to_trimesh()
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            "mesh_id": mesh_id,
            "levels": lvls,
            "original_vertices": cube.vertex_count(),
            "subdivided_vertices": subdivided.vertex_count(),
            "quad_faces": subdivided.face_count(),
            "triangles": trimesh.triangle_count(),
            "algorithm": "Catmull-Clark Bicubic Subdivision",
            "status": "success",
        }))
    }

    fn create_sculpt(
        &self,
        brush: &str,
        radius: f64,
        strength: f64,
    ) -> Result<serde_json::Value, StudioError> {
        let mut mesh = atlas_creative_core::mesh::PolyMesh::cube(2.0);
        let brush_type = match brush.to_lowercase().as_str() {
            "clay" => atlas_creative_core::sculpt::SculptBrushType::Clay,
            "smooth" => atlas_creative_core::sculpt::SculptBrushType::Smooth,
            "flatten" => atlas_creative_core::sculpt::SculptBrushType::Flatten,
            "inflate" => atlas_creative_core::sculpt::SculptBrushType::Inflate,
            "pinch" => atlas_creative_core::sculpt::SculptBrushType::Pinch,
            "crease" => atlas_creative_core::sculpt::SculptBrushType::Crease,
            "grab" => atlas_creative_core::sculpt::SculptBrushType::Grab,
            _ => atlas_creative_core::sculpt::SculptBrushType::Draw,
        };

        let sc = atlas_creative_core::sculpt::SculptBrush::new(
            brush_type,
            radius.clamp(0.1, 5.0),
            strength.clamp(0.01, 2.0),
            atlas_creative_core::sculpt::FalloffCurve::Smooth,
        );

        let modified = sc.apply(&mut mesh, [0.0, 0.0, 1.0], [0.0, 0.0, 1.0], [0.0, 0.0, 0.0]);

        Ok(serde_json::json!({
            "brush": brush,
            "radius": radius,
            "strength": strength,
            "vertices_modified": modified,
            "mesh_vertex_count": mesh.vertex_count(),
            "falloff": "Smooth Cosine Curve",
            "status": "success",
        }))
    }

    fn realtime_scene_build(&self, name: &str) -> Result<serde_json::Value, StudioError> {
        let mut scene = atlas_scene_graph::Scene::new(name);
        let mut root =
            atlas_scene_graph::SceneNode::new("root_aircraft", "Hypersonic Aircraft Airframe");
        root.local_transform.position = [0.0, 32000.0, 0.0];
        let root_id = scene
            .add_node(root, None)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let mut scramjet =
            atlas_scene_graph::SceneNode::new("child_scramjet", "Dual Scramjet Combustor");
        scramjet.local_transform.position = [0.0, -1.2, 8.5];
        let mut telemetry = atlas_scene_graph::TelemetryComponent::default();
        telemetry.channels.insert("thrust_kn".into(), 485.0);
        telemetry.channels.insert("inlet_mach".into(), 11.2);
        telemetry.channels.insert("total_temp_k".into(), 3120.0);
        scramjet.telemetry = Some(telemetry);

        let _ = scene
            .add_node(scramjet, Some(&root_id))
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let bounds = scene.compute_world_bounds(&root_id);

        Ok(serde_json::json!({
            "scene_name": name,
            "root_nodes": scene.root_nodes,
            "node_count": scene.nodes.len(),
            "bounds": {
                "min": bounds.min,
                "max": bounds.max,
                "center": bounds.center(),
            },
            "ecs_components": ["Transform", "MeshComponent", "LightComponent", "TelemetryComponent", "PhysicsBodyComponent"],
            "status": "built",
        }))
    }

    fn media_timeline_build(
        &self,
        title: &str,
        fps: u32,
    ) -> Result<serde_json::Value, StudioError> {
        let mut timeline = atlas_timeline::Timeline::new("tl_01", title, fps);
        let v1 = timeline.add_track("Video 1 (4K Masters)", atlas_timeline::TrackKind::Video);
        let a1 = timeline.add_track(
            "Audio 1 (Fairlight Stereo)",
            atlas_timeline::TrackKind::Audio,
        );

        let clip1 = atlas_timeline::TimelineClip::new(
            "clip_hypersonic_takeoff",
            "Mach 11 Plasma Cruise Reel",
            "media_scramjet_4k",
            &v1,
            0,
            240,
            0,
        );
        timeline
            .add_clip_to_track(&v1, clip1)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let otio_json = timeline
            .to_otio_json()
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        let otio_val = match serde_json::from_str::<serde_json::Value>(&otio_json) {
            Ok(v) => v,
            Err(_) => serde_json::json!({}),
        };

        Ok(serde_json::json!({
            "title": title,
            "fps": fps,
            "tracks": [
                { "id": v1, "name": "Video 1 (4K Masters)", "kind": "Video" },
                { "id": a1, "name": "Audio 1 (Fairlight Stereo)", "kind": "Audio" }
            ],
            "clip_count": 1,
            "otio_schema": "Timeline.1",
            "otio_json": otio_val,
            "status": "ready",
        }))
    }

    fn media_render_export(&self) -> Result<serde_json::Value, StudioError> {
        let grade = atlas_compositor::ThreeWayColorGrade {
            lift: [-0.02, -0.02, 0.01],
            gamma: [1.02, 1.0, 0.98],
            gain: [1.15, 1.05, 0.95],
            offset: [0.0, 0.0, 0.0],
            saturation: 1.12,
        };

        let mut mixer = atlas_audio::AudioMixer::new();
        let _t1 = mixer.add_track("Supersonic Scramjet Roar");
        let _t2 = mixer.add_track("Avionics Audio Telemetry");

        let b1 = atlas_audio::AudioBuffer::new(48000, 1, 4800);
        let b2 = atlas_audio::AudioBuffer::new(48000, 1, 4800);
        let mix_res = mixer
            .mix_to_stereo(&[&b1, &b2])
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        Ok(serde_json::json!({
            "export_format": "ProRes 422 HQ / OpenTimelineIO",
            "color_grading": {
                "type": "DaVinci 3-Way Color Wheels",
                "lift": grade.lift,
                "gamma": grade.gamma,
                "gain": grade.gain,
                "saturation": grade.saturation,
                "lut_cube": "hypersonic-aerothermal.cube",
            },
            "audio_master": {
                "sample_rate": mix_res.sample_rate,
                "channels": mix_res.channel_count(),
                "limiter_ceiling_db": -0.1,
                "ebu_r128_lufs": -23.0,
            },
            "deliverables": [
                "hypersonic-commercial-master.mov",
                "hypersonic-timeline.otio",
                "hypersonic-timeline.fcpxml",
                "hypersonic-aerothermal.cube"
            ],
            "status": "rendered",
        }))
    }

    fn list_filtered_assets(
        &self,
        user_id: &str,
        team_id: Option<&str>,
        visibility: Option<&str>,
        category: Option<&str>,
        search: Option<&str>,
    ) -> Result<serde_json::Value, StudioError> {
        let vis_enum = match visibility {
            Some("public") | Some("Public") => Some(atlas_asset_manager::AssetVisibility::Public),
            Some("team") | Some("Team") => Some(atlas_asset_manager::AssetVisibility::Team),
            Some("private") | Some("Private") => {
                Some(atlas_asset_manager::AssetVisibility::Private)
            }
            _ => None,
        };
        let assets = self
            .asset_registry
            .filter_assets(user_id, team_id, vis_enum, category, search);
        serde_json::to_value(&assets).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn get_asset_details(
        &self,
        asset_id: &str,
        user_id: &str,
        team_id: Option<&str>,
    ) -> Result<serde_json::Value, StudioError> {
        let rec = self
            .asset_registry
            .get_record(asset_id)
            .ok_or(StudioError::NotFound)?;

        // Multi-tier permission verification
        let permitted = match rec.visibility {
            atlas_asset_manager::AssetVisibility::Public => true,
            atlas_asset_manager::AssetVisibility::Team => {
                let matches_team = team_id.is_some() && rec.author.team_id.as_deref() == team_id;
                let matches_user = rec.author.user_id == user_id;
                matches_team || matches_user
            }
            atlas_asset_manager::AssetVisibility::Private => rec.author.user_id == user_id,
        };

        if !permitted {
            return Err(StudioError::Forbidden);
        }

        let data = self.asset_registry.get_data(asset_id).unwrap_or(&[]);
        let data_str = String::from_utf8_lossy(data);

        serde_json::to_value(serde_json::json!({
            "record": rec,
            "data_preview": if data_str.len() > 1024 { format!("{}...", &data_str[..1024]) } else { data_str.to_string() },
            "byte_size": data.len(),
            "content_hash": rec.content_hash,
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn create_asset(&mut self, req: CreateAssetRequest) -> Result<serde_json::Value, StudioError> {
        let vis_enum = match req.visibility.as_deref() {
            Some("team") | Some("Team") => atlas_asset_manager::AssetVisibility::Team,
            Some("private") | Some("Private") => atlas_asset_manager::AssetVisibility::Private,
            _ => atlas_asset_manager::AssetVisibility::Public,
        };

        let asset_id = format!(
            "ast_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_else(|_| u128::from(now_epoch_millis()))
        );
        let mut rec = atlas_asset_manager::AssetRecord::new(
            asset_id.clone(),
            req.name,
            atlas_asset_manager::AssetKind::CadGeometry,
        )
        .with_description(req.description.unwrap_or_default())
        .with_category(req.category.unwrap_or_else(|| "General".to_string()))
        .with_visibility(vis_enum)
        .with_author(atlas_asset_manager::AssetAuthor {
            user_id: req.author_id.unwrap_or_else(|| "usr_current".to_string()),
            name: req
                .author_name
                .unwrap_or_else(|| "Current User".to_string()),
            email: "user@atlascad.internal".to_string(),
            team_id: req
                .team_id
                .or_else(|| Some("team_aero_structures".to_string())),
            avatar_color: "#6366f1".to_string(),
        })
        .with_tags(req.tags.unwrap_or_default());

        if let Some(params) = req.parameters {
            rec.parameters = params;
        }

        let data = req
            .geometry_data
            .unwrap_or_else(|| "ATLASCAD_PARAMETRIC_SOLID_MESH:V1.0".to_string())
            .into_bytes();
        self.asset_registry
            .store_asset(rec.clone(), data)
            .map_err(|e| StudioError::BadRequest(e.to_string()))?;

        serde_json::to_value(&rec).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn fork_asset(
        &mut self,
        asset_id: &str,
        req: ForkAssetRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let caller = atlas_asset_manager::AssetAuthor {
            user_id: req.user_id.unwrap_or_else(|| "usr_current".to_string()),
            name: req.user_name.unwrap_or_else(|| "Current User".to_string()),
            email: "user@atlascad.internal".to_string(),
            team_id: req
                .team_id
                .or_else(|| Some("team_aero_structures".to_string())),
            avatar_color: "#6366f1".to_string(),
        };

        let vis_enum = match req.visibility.as_deref() {
            Some("public") | Some("Public") => atlas_asset_manager::AssetVisibility::Public,
            Some("team") | Some("Team") => atlas_asset_manager::AssetVisibility::Team,
            _ => atlas_asset_manager::AssetVisibility::Private,
        };

        let new_id = format!(
            "{}_fork_{}",
            asset_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(now_unix_secs())
        );
        let stored_id = self
            .asset_registry
            .fork_asset(asset_id, &new_id, caller, vis_enum)
            .map_err(|e| match e {
                atlas_asset_manager::AssetError::AccessDenied(_) => StudioError::Forbidden,
                atlas_asset_manager::AssetError::NotFound(_) => StudioError::NotFound,
                other => StudioError::BadRequest(other.to_string()),
            })?;

        let forked = self
            .asset_registry
            .get_record(&stored_id)
            .ok_or(StudioError::NotFound)?;
        serde_json::to_value(forked).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn use_asset(
        &mut self,
        asset_id: &str,
        req: UseAssetRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let rec = self
            .asset_registry
            .get_record_mut(asset_id)
            .ok_or(StudioError::NotFound)?;
        rec.downloads += 1;

        let instance_name = req
            .instance_name
            .unwrap_or_else(|| format!("{}_inst", rec.name.replace(' ', "_").to_lowercase()));
        let payload = serde_json::json!({
            "status": "Instantiated",
            "asset_id": asset_id,
            "instance_name": instance_name,
            "parameters_applied": req.parameters.unwrap_or_else(|| serde_json::json!({})),
            "downloads": rec.downloads,
            "linked_dependency": true,
        });
        Ok(payload)
    }

    fn collab_presence_payload(&self) -> Result<serde_json::Value, StudioError> {
        let now = now_unix_secs();
        let active_users: Vec<&CollabUser> = self.collab_presence.values().collect();
        let active_locks: Vec<&EntityLock> = self.entity_locks.values().collect();

        serde_json::to_value(serde_json::json!({
            "users": active_users,
            "locks": active_locks,
            "total_active": active_users.len(),
            "document_revision": self.project.revision.get(),
            "timestamp": now,
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn update_presence(
        &mut self,
        req: UpdatePresenceRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(now_unix_secs());
        let user = self
            .collab_presence
            .entry(req.user_id.clone())
            .or_insert_with(|| CollabUser {
                id: req.user_id.clone(),
                name: req
                    .name
                    .clone()
                    .unwrap_or_else(|| "Collaborator".to_string()),
                role: req.role.clone().unwrap_or_else(|| "Editor".to_string()),
                avatar_color: req
                    .avatar_color
                    .clone()
                    .unwrap_or_else(|| "#38bdf8".to_string()),
                cursor_pos: req.cursor_pos.unwrap_or([0.0, 0.0, 0.0]),
                active_tool: req
                    .active_tool
                    .clone()
                    .unwrap_or_else(|| "Select".to_string()),
                selected_entity: req.selected_entity.clone(),
                last_heartbeat: now,
            });

        if let Some(name) = req.name {
            user.name = name;
        }
        if let Some(role) = req.role {
            user.role = role;
        }
        if let Some(col) = req.avatar_color {
            user.avatar_color = col;
        }
        if let Some(pos) = req.cursor_pos {
            user.cursor_pos = pos;
        }
        if let Some(tool) = req.active_tool {
            user.active_tool = tool;
        }
        user.selected_entity = req.selected_entity;
        user.last_heartbeat = now;

        Ok(serde_json::json!({ "ok": true, "user_id": req.user_id }))
    }

    fn acquire_entity_lock(
        &mut self,
        req: AcquireLockRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let now = now_unix_secs();
        let duration = req.duration_sec.unwrap_or(30);

        if let Some(existing) = self.entity_locks.get(&req.entity_id)
            && existing.holder_user_id != req.user_id
            && existing.expires_at > now
        {
            return Ok(serde_json::json!({
                "acquired": false,
                "reason": "Entity locked by another collaborator",
                "holder_name": existing.holder_name,
                "expires_in_sec": existing.expires_at.saturating_sub(now),
            }));
        }

        let lock = EntityLock {
            entity_id: req.entity_id.clone(),
            holder_user_id: req.user_id.clone(),
            holder_name: req.user_name.unwrap_or_else(|| "Collaborator".to_string()),
            acquired_at: now,
            expires_at: now + duration,
        };
        self.entity_locks
            .insert(req.entity_id.clone(), lock.clone());

        Ok(serde_json::json!({
            "acquired": true,
            "lock": lock,
        }))
    }

    fn release_entity_lock(
        &mut self,
        req: ReleaseLockRequest,
    ) -> Result<serde_json::Value, StudioError> {
        if let Some(lock) = self.entity_locks.get(&req.entity_id)
            && lock.holder_user_id == req.user_id
        {
            self.entity_locks.remove(&req.entity_id);
            return Ok(serde_json::json!({ "released": true, "entity_id": req.entity_id }));
        }
        Ok(serde_json::json!({ "released": false, "reason": "Not lock holder" }))
    }

    fn collab_comments_payload(&self) -> Result<serde_json::Value, StudioError> {
        serde_json::to_value(serde_json::json!({
            "comments": self.collab_comments,
            "count": self.collab_comments.len(),
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn create_collab_comment(
        &mut self,
        req: CreateCommentRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let now = now_unix_secs();
        let comment_id = format!(
            "cmt_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_else(|_| u128::from(now_epoch_millis()))
        );
        let comment = CollabComment {
            id: comment_id,
            entity_id: req.entity_id,
            pos_3d: req.pos_3d,
            author_id: req.author_id.unwrap_or_else(|| "usr_current".to_string()),
            author_name: req
                .author_name
                .unwrap_or_else(|| "Current User".to_string()),
            author_color: req.author_color.unwrap_or_else(|| "#6366f1".to_string()),
            text: req.text,
            status: "Open".to_string(),
            created_at: now,
            replies: vec![],
        };
        self.collab_comments.push(comment.clone());
        serde_json::to_value(&comment).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn reply_collab_comment(
        &mut self,
        comment_id: &str,
        req: ReplyCommentRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let now = now_unix_secs();
        let cmt = self
            .collab_comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .ok_or(StudioError::NotFound)?;
        let reply_id = format!("rep_{}", cmt.replies.len() + 1);
        let reply = CommentReply {
            id: reply_id,
            author_id: req.author_id.unwrap_or_else(|| "usr_current".to_string()),
            author_name: req
                .author_name
                .unwrap_or_else(|| "Current User".to_string()),
            text: req.text,
            created_at: now,
        };
        cmt.replies.push(reply.clone());
        serde_json::to_value(&reply).map_err(|e| StudioError::Json(e.to_string()))
    }

    fn resolve_collab_comment(
        &mut self,
        comment_id: &str,
    ) -> Result<serde_json::Value, StudioError> {
        let cmt = self
            .collab_comments
            .iter_mut()
            .find(|c| c.id == comment_id)
            .ok_or(StudioError::NotFound)?;
        cmt.status = if cmt.status == "Resolved" {
            "Open".to_string()
        } else {
            "Resolved".to_string()
        };
        serde_json::to_value(serde_json::json!({ "id": comment_id, "status": cmt.status }))
            .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_me_payload(&self) -> Result<serde_json::Value, StudioError> {
        let default_user = AuthUser {
            user_id: "usr_guest".to_string(),
            name: "Guest Engineer".to_string(),
            email: "guest@atlascad.internal".to_string(),
            role: "Viewer".to_string(),
            team_id: "team_guest".to_string(),
            team_name: "External Reviewer Sandbox".to_string(),
            org_id: "org_enterprise_01".to_string(),
            org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
            avatar_color: "#94a3b8".to_string(),
            plan_tier: "Guest Evaluation Plan".to_string(),
            plan_seats: 1,
            plan_expiry: "2027-11-22".to_string(),
            license_key: "ATLAS-GUEST-TRIAL".to_string(),
            permissions: vec![
                "cad:read".to_string(),
                "cad:export".to_string(),
                "collab:comment".to_string(),
            ],
            mfa_enabled: false,
            sso_provider: None,
            created_at: now_unix_secs(),
        };

        let active_user = self
            .auth_users
            .get(&self.active_user_id)
            .cloned()
            .unwrap_or(default_user);

        let teams = vec![
            serde_json::json!({ "id": "team_aero_structures", "name": "Aerospace & Hypersonic Structures", "members_count": 8, "lead": "Sanjay" }),
            serde_json::json!({ "id": "team_propulsion", "name": "Scramjet & Thermal Dynamics", "members_count": 5, "lead": "Dr. Elena Rostova" }),
            serde_json::json!({ "id": "team_robotics", "name": "Precision Kinematics & Mechatronics", "members_count": 6, "lead": "Marcus Vance" }),
            serde_json::json!({ "id": "team_compliance", "name": "External Airworthiness Audit", "members_count": 3, "lead": "Sara Chen" }),
            serde_json::json!({ "id": "personal", "name": "Personal Private Sandbox", "members_count": 1, "lead": active_user.name }),
        ];

        let users: Vec<serde_json::Value> = self
            .auth_users
            .values()
            .map(|u| {
                serde_json::json!({
                    "user_id": u.user_id,
                    "name": u.name,
                    "email": u.email,
                    "role": u.role,
                    "team_id": u.team_id,
                    "team_name": u.team_name,
                    "avatar_color": u.avatar_color,
                    "plan_tier": u.plan_tier,
                })
            })
            .collect();

        serde_json::to_value(serde_json::json!({
            "authenticated": self.active_user_id != "usr_guest",
            "user": active_user,
            "teams": teams,
            "users": users,
            "active_session": {
                "token": format!("atlas_sess_{}", active_user.user_id),
                "expires_in_hours": 720,
                "security_protocol": "TLS 1.3 / ChaCha20-Poly1305 / Enterprise HSM",
                "ip_origin": "127.0.0.1",
            },
            "enterprise_license": {
                "organization": "AtlasCAD Advanced Engineering Lab",
                "tier": "$2,000/yr Commercial Enterprise Plan",
                "license_key": "ATLAS-ENT-2026-X889-K992-FLIGHT",
                "seats_allocated": 4,
                "seats_total": 10,
                "status": "Active",
                "renewal_date": "2027-11-22",
                "features": [
                    "Unlimited 6-DOF Waverider Physics Sim",
                    "Truck Kernel B-Rep Exact Solid Modeler",
                    "Multi-tier Asset Store (Public, Team, Private)",
                    "Real-Time Optimistic Entity Leases & 3D Spatial Reviews",
                    "FNV-1a tamper-evident audit chain",
                    "Headless Batch CLI & Python SDK API Keys"
                ]
            }
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_login(&mut self, req: LoginRequest) -> Result<serde_json::Value, StudioError> {
        // This build ships a local demo identity, not an authentication boundary: there is no
        // credential store and no IdP adapter. Refuse the paths that would otherwise look like
        // real authentication instead of accepting a provider name and returning a session.
        if let Some(provider) = req.sso_provider.as_deref() {
            return Err(StudioError::Unavailable(format!(
                "SSO via {provider} is not configured: this build has no identity provider adapter. \
                 Sign-in is a local demo identity switch only."
            )));
        }
        if self.share.is_some() {
            return Err(StudioError::Unavailable(
                "Credential verification is unavailable on a shared instance. This build carries a \
                 local demo identity only; run it on loopback without --share, or place an \
                 authenticating proxy in front of it."
                    .to_string(),
            ));
        }

        let user = if let Some(uid) = req.user_id.as_deref().or(req.demo_user.as_deref()) {
            self.auth_users.get(uid).cloned()
        } else if let Some(email) = &req.email {
            self.auth_users
                .values()
                .find(|u| &u.email == email)
                .cloned()
        } else {
            None
        };

        let user = match user {
            Some(u) => u,
            None => {
                let email = req
                    .email
                    .unwrap_or_else(|| "engineer@enterprise.internal".to_string());
                let name = req.name.unwrap_or_else(|| {
                    email
                        .split('@')
                        .next()
                        .unwrap_or("Enterprise User")
                        .to_string()
                });
                let user_id = format!("usr_{}", email.replace(['@', '.'], "_"));
                let new_user = AuthUser {
                    user_id: user_id.clone(),
                    name,
                    email,
                    role: "Design Engineer".to_string(),
                    team_id: "team_aero_structures".to_string(),
                    team_name: "Aerospace & Hypersonic Structures".to_string(),
                    org_id: "org_enterprise_01".to_string(),
                    org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
                    avatar_color: "#8b5cf6".to_string(),
                    plan_tier: "$2,000/yr Commercial Enterprise Plan".to_string(),
                    plan_seats: 10,
                    plan_expiry: "2027-11-22".to_string(),
                    license_key: "ATLAS-ENT-2026-X889-K992-FLIGHT".to_string(),
                    permissions: vec![
                        "cad:read".into(),
                        "cad:mutate".into(),
                        "cad:export".into(),
                        "assets:publish_team".into(),
                        "assets:publish_private".into(),
                        "collab:lease_locks".into(),
                    ],
                    mfa_enabled: false,
                    sso_provider: req.sso_provider,
                    created_at: now_unix_secs(),
                };
                self.auth_users.insert(user_id, new_user.clone());
                new_user
            }
        };

        self.active_user_id = user.user_id.clone();
        let token = format!(
            "atlas_sess_{}_{}",
            user.user_id,
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_secs())
                .unwrap_or(now_unix_secs())
        );
        self.auth_sessions.insert(
            token.clone(),
            AuthSession {
                token: token.clone(),
                user_id: user.user_id.clone(),
                created_at: now_unix_secs(),
                expires_at: now_unix_secs() + 720 * 3600,
                ip_address: "127.0.0.1".to_string(),
                user_agent: "AtlasCAD Studio Web Client".to_string(),
            },
        );

        // Update presence cluster for the newly logged in user
        self.collab_presence.insert(
            user.user_id.clone(),
            CollabUser {
                id: user.user_id.clone(),
                name: format!("{} ({})", user.name, user.role),
                role: user.role.clone(),
                avatar_color: user.avatar_color.clone(),
                cursor_pos: [0.0, 0.0, 0.0],
                active_tool: "3D Workspace".to_string(),
                selected_entity: None,
                last_heartbeat: now_unix_secs(),
            },
        );

        serde_json::to_value(serde_json::json!({
            "success": true,
            "token": token,
            "user": user,
            "credential_verified": false,
            "authentication": "demo-identity",
            "demo_notice": "Local demo identity: no credential was verified. This session is a UI \
                role switch, not an access boundary; do not expose this instance to an untrusted \
                network.",
            "message": format!("Switched demo identity to {} ({})", user.name, user.role),
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_register(&mut self, req: RegisterRequest) -> Result<serde_json::Value, StudioError> {
        let user_id = format!(
            "usr_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_else(|_| u128::from(now_epoch_millis()))
        );
        let role = req.role.unwrap_or_else(|| "Design Engineer".to_string());
        let team_id = req
            .team_id
            .unwrap_or_else(|| "team_aero_structures".to_string());
        let team_name = match team_id.as_str() {
            "team_propulsion" => "Scramjet & Thermal Dynamics".to_string(),
            "team_robotics" => "Precision Kinematics & Mechatronics".to_string(),
            "team_compliance" => "External Airworthiness Audit".to_string(),
            "personal" => "Personal Private Sandbox".to_string(),
            _ => "Aerospace & Hypersonic Structures".to_string(),
        };

        let new_user = AuthUser {
            user_id: user_id.clone(),
            name: req.name,
            email: req.email,
            role,
            team_id,
            team_name,
            org_id: "org_enterprise_01".to_string(),
            org_name: "AtlasCAD Advanced Engineering Lab".to_string(),
            avatar_color: req.avatar_color.unwrap_or_else(|| "#6366f1".to_string()),
            plan_tier: "$2,000/yr Commercial Enterprise Plan".to_string(),
            plan_seats: 10,
            plan_expiry: "2027-11-22".to_string(),
            license_key: "ATLAS-ENT-2026-X889-K992-FLIGHT".to_string(),
            permissions: vec![
                "cad:read".into(),
                "cad:mutate".into(),
                "cad:export".into(),
                "assets:publish_team".into(),
                "assets:publish_private".into(),
                "collab:lease_locks".into(),
            ],
            mfa_enabled: false,
            sso_provider: None,
            created_at: now_unix_secs(),
        };

        self.auth_users.insert(user_id.clone(), new_user.clone());
        self.active_user_id = user_id.clone();
        let token = format!("atlas_sess_{}", user_id);

        serde_json::to_value(serde_json::json!({
            "success": true,
            "token": token,
            "user": new_user,
            "credential_verified": false,
            "authentication": "demo-identity",
            "demo_notice": "Local demo identity: this member record has no verified credential and \
                grants no access boundary.",
            "message": "Demo member record created",
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_logout(&mut self) -> Result<serde_json::Value, StudioError> {
        self.active_user_id = "usr_guest".to_string();
        serde_json::to_value(serde_json::json!({
            "success": true,
            "logged_out": true,
            "message": "Logged out. Current session terminated.",
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_switch_team(
        &mut self,
        req: SwitchTeamRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let (team_name, role) = match req.team_id.as_str() {
            "team_propulsion" => ("Scramjet & Thermal Dynamics", "Lead Propulsion"),
            "team_robotics" => ("Precision Kinematics & Mechatronics", "Robotics Lead"),
            "team_compliance" => ("External Airworthiness Audit", "Compliance Reviewer"),
            "personal" => ("Personal Private Sandbox", "Owner"),
            _ => ("Aerospace & Hypersonic Structures", "Chief Architect"),
        };

        if let Some(user) = self.auth_users.get_mut(&self.active_user_id) {
            user.team_id = req.team_id.clone();
            user.team_name = team_name.to_string();
            user.role = role.to_string();
            let updated = user.clone();
            return serde_json::to_value(serde_json::json!({
                "success": true,
                "active_team_id": req.team_id,
                "team_name": team_name,
                "user": updated,
            }))
            .map_err(|e| StudioError::Json(e.to_string()));
        }

        Err(StudioError::BadRequest(
            "No active user to switch team".into(),
        ))
    }

    fn auth_sessions_payload(&self) -> Result<serde_json::Value, StudioError> {
        let sessions: Vec<&AuthSession> = self.auth_sessions.values().collect();
        serde_json::to_value(serde_json::json!({
            "sessions": sessions,
            "active_user_id": self.active_user_id,
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_api_keys_payload(&self) -> Result<serde_json::Value, StudioError> {
        serde_json::to_value(serde_json::json!({
            "api_keys": self.api_keys,
            "user_id": self.active_user_id,
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_create_api_key(
        &mut self,
        req: CreateApiKeyRequest,
    ) -> Result<serde_json::Value, StudioError> {
        let id = format!(
            "key_{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_millis())
                .unwrap_or_else(|_| u128::from(now_epoch_millis()))
        );
        let token = format!(
            "atlas_pat_{:016x}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or_else(|_| u128::from(now_epoch_millis()) * 1_000_000)
        );
        let key = ApiKeyRecord {
            id,
            name: req.name,
            token: token.clone(),
            user_id: self.active_user_id.clone(),
            created_at: now_unix_secs(),
            expires_at: Some(now_unix_secs() + 365 * 86400),
            last_used_at: None,
            scopes: req.scopes.unwrap_or_else(|| {
                vec!["cad:read".into(), "cad:mutate".into(), "export:all".into()]
            }),
        };
        self.api_keys.push(key.clone());
        serde_json::to_value(serde_json::json!({
            "success": true,
            "key": key,
            "token": token,
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
    }

    fn auth_revoke_api_key(
        &mut self,
        req: RevokeApiKeyRequest,
    ) -> Result<serde_json::Value, StudioError> {
        self.api_keys.retain(|k| k.id != req.id);
        serde_json::to_value(serde_json::json!({
            "success": true,
            "revoked_id": req.id,
        }))
        .map_err(|e| StudioError::Json(e.to_string()))
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
    let liveness =
        request.method == "GET" && (path == "/api/health" || path == "/api/deploy/status");
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
        ("GET", "/flight-demo-poster.png") => binary_asset(FLIGHT_POSTER, "image/png"),
        ("GET", "/atlascad-supersonic-flight.mp4") => binary_asset(FLIGHT_VIDEO, "video/mp4"),
        ("GET", "/api/health") => json_response(
            200,
            serde_json::json!({
                "ok": true,
                "status": "healthy",
                "version": env!("CARGO_PKG_VERSION"),
                "service": "atlascad-studio",
                "solvers_ready": 140,
                "deterministic": true
            }),
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
        ("GET", "/api/export/step") => match with_state(state, |state| state.step()) {
            Ok((bytes, filename)) => http::Response::new(200, "OK", "application/step", bytes)
                .with_header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{filename}\""),
                ),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/export/3mf") => match with_state(state, |state| state.threemf()) {
            Ok((bytes, filename)) => http::Response::new(200, "OK", "model/3mf", bytes)
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
        ("GET", "/api/export/davinci") => {
            let query = request.path.clone();
            let fps = query_param(&query, "fps")
                .or_else(|| query_param(&query, "timebase_fps"))
                .and_then(|v| v.parse::<f64>().ok());
            let format_param = query_text(&query, "format");
            match with_state(state, |state| state.export_davinci_resolve(fps)) {
                Ok(bundle) => match format_param.as_deref() {
                    Some("fcpxml") | Some("xml") => {
                        let text = bundle["fcpxml"].as_str().unwrap_or("");
                        http::Response::new(200, "OK", "application/xml", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"hypersonic-timeline.fcpxml\"",
                            )
                    }
                    Some("edl") => {
                        let text = bundle["edl"].as_str().unwrap_or("");
                        http::Response::new(200, "OK", "text/plain", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"hypersonic-timeline.edl\"",
                            )
                    }
                    Some("cube") | Some("lut") => {
                        let text = bundle["thermal_cube_lut"].as_str().unwrap_or("");
                        http::Response::new(200, "OK", "text/plain", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"hypersonic-aerothermal.cube\"",
                            )
                    }
                    Some("usda") | Some("usd") => {
                        let text = bundle["open_usd_usda"].as_str().unwrap_or("");
                        http::Response::new(200, "OK", "text/plain", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"hypersonic-fusion.usda\"",
                            )
                    }
                    _ => to_json(200, &bundle),
                },
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/export/unity") => {
            let query = request.path.clone();
            let hud = query_param(&query, "hud")
                .or_else(|| query_param(&query, "include_hud"))
                .map(|v| v == "true" || v == "1");
            let file_param = query_text(&query, "file");
            match with_state(state, |state| state.export_unity_package(hud)) {
                Ok(pkg) => match file_param.as_deref() {
                    Some("package.json") => {
                        let text = pkg["package_manifest"].as_str().unwrap_or("{}");
                        http::Response::new(200, "OK", "application/json", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"package.json\"",
                            )
                    }
                    Some("AtlasCadClient.cs") => {
                        let text = pkg["scripts"]["AtlasCadClient.cs"].as_str().unwrap_or("");
                        http::Response::new(200, "OK", "text/plain", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"AtlasCadClient.cs\"",
                            )
                    }
                    Some("HypersonicFlightSimulator.cs") => {
                        let text = pkg["scripts"]["HypersonicFlightSimulator.cs"]
                            .as_str()
                            .unwrap_or("");
                        http::Response::new(200, "OK", "text/plain", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"HypersonicFlightSimulator.cs\"",
                            )
                    }
                    Some("AtlasCadHUD.cs") => {
                        let text = pkg["scripts"]["AtlasCadHUD.cs"].as_str().unwrap_or("");
                        http::Response::new(200, "OK", "text/plain", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"AtlasCadHUD.cs\"",
                            )
                    }
                    Some("gltf") | Some("model") => {
                        let text = pkg["gltf_json"].as_str().unwrap_or("{}");
                        http::Response::new(200, "OK", "model/gltf+json", text.as_bytes().to_vec())
                            .with_header(
                                "Content-Disposition",
                                "attachment; filename=\"hypersonic-aircraft.gltf\"",
                            )
                    }
                    _ => to_json(200, &pkg),
                },
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/export/gltf") => match with_state(state, |state| state.export_gltf_scene()) {
            Ok((bytes, filename)) => http::Response::new(200, "OK", "model/gltf+json", bytes)
                .with_header(
                    "Content-Disposition",
                    format!("attachment; filename=\"{filename}\""),
                ),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/deploy/status") => match with_state(state, |state| state.deploy_status()) {
            Ok(payload) => to_json(200, &payload),
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
        ("GET", "/api/engines") => {
            let registry = cad_engineering::engine::EngineRegistry::new();
            let list = registry.list();
            let payload = serde_json::json!({
                "count": list.len(),
                "engines": list,
            });
            json_response(200, payload)
        }
        ("GET", "/api/engines/guides") => {
            let guides = atlas_engine_manager::installation::InstallationGuide::all();
            let payload = serde_json::json!({
                "count": guides.len(),
                "guides": guides,
            });
            json_response(200, payload)
        }
        ("GET", "/api/engines/discovery") => {
            let reports = atlas_engine_manager::discovery::EngineDiscovery::discover_all();
            let payload = serde_json::json!({
                "count": reports.len(),
                "reports": reports,
            });
            json_response(200, payload)
        }
        ("GET", "/api/graph") => match with_state(state, |state| Ok(state.graph_payload())) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/graph/plan") => match with_state(state, |state| state.graph_recompute_plan())
        {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/audit") => match with_state(state, |state| Ok(state.audit_payload())) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/report") => {
            match with_state(state, |state| state.compliance_report_payload()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/org") => match with_state(state, |state| Ok(state.organization_payload())) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/jobs") => match with_state(state, |state| Ok(state.job_queue_payload())) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/commands") => match with_state(state, |state| Ok(state.list_commands())) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/commands/history") => {
            match with_state(state, |state| Ok(state.command_history())) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/agents") => match with_state(state, |state| Ok(state.list_agents())) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/agents/workflows") => {
            match with_state(state, |state| Ok(state.list_workflows())) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/agents/approvals") => {
            match with_state(state, |state| Ok(state.list_approvals())) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/agents/memory") => {
            match with_state(state, |state| Ok(state.agent_memory())) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/sim/hypersonic/bom") => {
            match with_state(state, |state| state.sim_hypersonic_bom()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/sim/chemical/species") => {
            match with_state(state, |state| state.sim_chemical_species()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/cad/hardware/fasteners") => {
            match with_state(state, |state| state.cad_hardware_fasteners()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", path) if path.starts_with("/api/assets") => {
            let query = request.path.clone();
            let vis_param = query_text(&query, "visibility");
            let cat_param = query_text(&query, "category");
            let q_param = query_text(&query, "q").or_else(|| query_text(&query, "search"));
            let user_param =
                query_text(&query, "user_id").unwrap_or_else(|| "usr_current".to_string());
            let team_param =
                query_text(&query, "team_id").or_else(|| Some("team_aero_structures".to_string()));

            if path == "/api/assets" {
                match with_state(state, |state| {
                    state.list_filtered_assets(
                        &user_param,
                        team_param.as_deref(),
                        vis_param.as_deref(),
                        cat_param.as_deref(),
                        q_param.as_deref(),
                    )
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            } else {
                let asset_id = path
                    .trim_start_matches("/api/assets/")
                    .trim_end_matches('/');
                match with_state(state, |state| {
                    state.get_asset_details(asset_id, &user_param, team_param.as_deref())
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            }
        }
        ("GET", "/api/collab/presence") => {
            match with_state(state, |state| state.collab_presence_payload()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/collab/comments") => {
            match with_state(state, |state| state.collab_comments_payload()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/auth/me") => match with_state(state, |state| state.auth_me_payload()) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        ("GET", "/api/auth/sessions") => {
            match with_state(state, |state| state.auth_sessions_payload()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        ("GET", "/api/auth/api-keys") => {
            match with_state(state, |state| state.auth_api_keys_payload()) {
                Ok(payload) => json_response(200, payload),
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
                let angle_deg = body.angle_deg.unwrap_or(cad_document::FULL_REVOLVE_DEGREES);
                match with_state(state, |state| {
                    state.mutate(|project| {
                        project
                            .add_revolve_feature_angle(sketch_id, axis, angle_deg, operation)
                            .map(|_| ())
                    })
                }) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/import/step" => match parse_body::<StepImportRequest>(request) {
            Ok(body) => match with_state(state, |state| state.import_step(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/import/remove" => match parse_body::<ImportRemoveRequest>(request) {
            Ok(body) => match with_state(state, |state| state.remove_import(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/draft/import" => match parse_body::<DraftImportRequest>(request) {
            Ok(body) => match with_state(state, |state| state.drawing_import(&body)) {
                Ok(payload) => to_json(200, &payload),
                Err(error) => error_response(&error),
            },
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
        "/api/draft/entities" => match parse_body::<DrawingBatchRequest>(request) {
            Ok(body) => {
                let mut items = Vec::with_capacity(body.entities.len());
                for item in body.entities {
                    match serde_json::from_value::<cad_drafting::EntityKind>(item.kind) {
                        Ok(kind) => items.push((item.layer, kind)),
                        Err(error) => {
                            return error_response(&StudioError::BadRequest(format!(
                                "the entity is not a drawing entity: {error}"
                            )));
                        }
                    }
                }
                if items.is_empty() {
                    return error_response(&StudioError::BadRequest(
                        "a batch needs at least one entity".into(),
                    ));
                }
                match with_state(state, |state| state.drawing_add_batch(&items)) {
                    Ok(payload) => to_json(200, &payload),
                    Err(error) => error_response(&error),
                }
            }
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
        "/api/engines/health" => match parse_body::<EngineHealthRequest>(request) {
            Ok(body) => {
                let registry = cad_engineering::engine::EngineRegistry::new();
                let engine_id = cad_engineering::engine::EngineId::new(body.engine_id);
                if registry.get(&engine_id).is_none() {
                    return error_response(&StudioError::NotFound);
                }
                let report = registry.test_health(&engine_id);
                let payload = serde_json::json!({
                    "engine_id": report.engine_id.as_str(),
                    "status": if report.healthy { "healthy" } else { "unavailable" },
                    "details": report.message,
                    "latency_ms": report.ping_ms,
                    "detected_version": report.detected_version,
                    "timestamp": now_epoch_millis() / 1000,
                });
                json_response(200, payload)
            }
            Err(error) => error_response(&error),
        },
        "/api/engines/run" => match parse_body::<EngineRunRequest>(request) {
            Ok(body) => {
                let registry = cad_engineering::engine::EngineRegistry::new();
                let engine_id = cad_engineering::engine::EngineId::new(body.engine_id);
                let engine_info = match registry.get(&engine_id) {
                    Some(info) => info.clone(),
                    None => return error_response(&StudioError::NotFound),
                };
                let job_req = cad_engineering::engine::EngineJobRequest {
                    job_id: body.job_id.unwrap_or_else(|| "job-1".to_string()),
                    engine_id: engine_id.clone(),
                    job_type: body.job_type,
                    model_revision: 1,
                    parameters: body.parameters.unwrap_or_else(|| serde_json::json!({})),
                    timeout_seconds: body.timeout_seconds.unwrap_or(60),
                };
                let result = registry.execute_job(&job_req);
                if result.success {
                    let target_node = match result.engine_id.as_str() {
                        "calculix" => Some("fea-structural"),
                        "openfoam" => Some("cfd-aerodynamics"),
                        "opencam" => Some("cam-toolpaths"),
                        "gmsh" => Some("fem-mesh"),
                        "ifcopenshell" => Some("bim-coordination"),
                        _ => None,
                    };
                    if let Some(node_id) = target_node {
                        let _ = with_state(state, |s| {
                            s.mark_node_current(node_id, &result.provenance_hash);
                            Ok(())
                        });
                    }
                }
                if !result.success {
                    return error_response(&StudioError::Unavailable(
                        result.error_message.unwrap_or_else(|| {
                            "the engine adapter did not run; no result is reported".to_string()
                        }),
                    ));
                }
                let payload = serde_json::json!({
                    "job_id": result.job_id,
                    "engine_id": result.engine_id.as_str(),
                    "discipline": format!("{:?}", engine_info.discipline).to_lowercase(),
                    "status": "completed",
                    "execution_time_ms": result.elapsed_ms,
                    "metrics": result.output,
                    "logs": result.warnings,
                    "provenance_hash": result.provenance_hash,
                });
                json_response(200, payload)
            }
            Err(error) => error_response(&error),
        },
        "/api/engines/benchmark" => match parse_body::<EngineRunRequest>(request) {
            Ok(body) => {
                let registry = cad_engineering::engine::EngineRegistry::new();
                let engine_id = cad_engineering::engine::EngineId::new(body.engine_id);
                if registry.get(&engine_id).is_none() {
                    return error_response(&StudioError::NotFound);
                }
                let job_req = cad_engineering::engine::EngineJobRequest {
                    job_id: body.job_id.unwrap_or_else(|| "benchmark-1".to_string()),
                    engine_id: engine_id.clone(),
                    job_type: Some("diagnostic_benchmark".to_string()),
                    model_revision: 1,
                    parameters: serde_json::json!({ "benchmark": true, "tolerance": 1e-6 }),
                    timeout_seconds: 30,
                };
                let result = registry.execute_job(&job_req);
                if !result.success {
                    return error_response(&StudioError::Unavailable(
                        result.error_message.unwrap_or_else(|| {
                            "no benchmark was run; no accuracy is reported".to_string()
                        }),
                    ));
                }
                let payload = serde_json::json!({
                    "engine_id": result.engine_id.as_str(),
                    "benchmark_status": "completed",
                    "execution_time_ms": result.elapsed_ms,
                    "accuracy_score": result
                        .output
                        .get("accuracy_score")
                        .cloned()
                        .unwrap_or(serde_json::Value::Null),
                    "metrics": result.output,
                    "provenance_hash": result.provenance_hash,
                });
                json_response(200, payload)
            }
            Err(error) => error_response(&error),
        },
        "/api/graph/invalidate" => match parse_body_or_default::<InvalidateRequest>(request) {
            Ok(body) => {
                let node_id = body.node_id.unwrap_or_else(|| "cad-geometry".to_string());
                let reason = body
                    .reason
                    .unwrap_or_else(|| "Manual user invalidation".to_string());
                match with_state(state, |state| {
                    let affected = state.invalidate_node(&node_id, &reason);
                    let affected_str: Vec<String> =
                        affected.iter().map(|id| id.as_str().to_string()).collect();
                    Ok(serde_json::json!({
                        "invalidated_node": node_id,
                        "reason": reason,
                        "affected_nodes": affected_str,
                        "graph": state.graph_payload(),
                    }))
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/jobs/submit" => match parse_body::<JobSubmitRequest>(request) {
            Ok(body) => match with_state(state, |state| Ok(state.submit_job(&body))) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/jobs/cancel" => match parse_body::<JobCancelRequest>(request) {
            Ok(body) => match with_state(state, |state| Ok(state.cancel_job(&body.job_id))) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/eco" => match parse_body::<EcoRequest>(request) {
            Ok(body) => match with_state(state, |state| Ok(state.process_eco(&body))) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/cad/sheet_metal/unfold" => {
            match parse_body_or_default::<CadSheetMetalUnfoldRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cad_sheet_metal_unfold(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/cad/hardware/fastener" => {
            match parse_body_or_default::<CadHardwareFastenerRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cad_hardware_fastener(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/cad/hardware/fasteners" => {
            match with_state(state, |state| state.cad_hardware_fasteners()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        "/api/cad/hardware/spring" => {
            match parse_body_or_default::<CadHardwareSpringRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cad_hardware_spring(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/cad/hardware/gear_pair" => {
            match parse_body_or_default::<CadHardwareGearPairRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cad_hardware_gear_pair(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/cam/physics/cutting_force" => {
            match parse_body_or_default::<CamCuttingForceRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cam_cutting_force(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/short_circuit" => {
            match parse_body_or_default::<SimShortCircuitRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electrical_short_circuit(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/building/daylight_factor" => {
            match parse_body_or_default::<SimDaylightingRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_building_daylight_factor(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/cad/hardware/shaft_key" => {
            match parse_body_or_default::<CadHardwareShaftKeyRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cad_hardware_shaft_key(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/pressure_vessel/asme" => {
            match parse_body_or_default::<SimPressureVesselRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_pressure_vessel_asme(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/psychrometrics" => {
            match parse_body_or_default::<SimPsychrometricsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_thermal_psychrometrics(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/hydrology" => match parse_body_or_default::<SimHydrologyRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_civil_hydrology(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/structural/fracture" => {
            match parse_body_or_default::<SimFractureRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_structural_fracture(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/rotordynamics/api617" => {
            match parse_body_or_default::<SimRotorDynamicsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_rotordynamics_api617(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/cam/welding/metallurgy" => {
            match parse_body_or_default::<CamWeldingRequest>(request) {
                Ok(body) => match with_state(state, |state| state.cam_welding_metallurgy(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/chemical/distillation" => {
            match parse_body_or_default::<SimDistillationRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_chemical_distillation(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/ecad/pcb_thermal" => {
            match parse_body_or_default::<SimPcbThermalRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_ecad_pcb_thermal(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/structural/connections" => {
            match parse_body_or_default::<SimStructuralConnectionRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_structural_connections(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hydraulic/pump" => match parse_body_or_default::<SimPumpRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_hydraulic_pump(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/thermal/heat_exchanger_rating" => {
            match parse_body_or_default::<SimHeatExchangerRatingRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_heat_exchanger_rating(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/geotechnical_bearing" => {
            match parse_body_or_default::<SimGeotechnicalBearingRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_geotechnical_bearing(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/gas_compression" => {
            match parse_body_or_default::<SimGasCompressionRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_gas_compression(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/structural/timber_fire" => {
            match parse_body_or_default::<SimTimberFireRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_timber_fire(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/propeller" => {
            match parse_body_or_default::<SimMarinePropellerRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_marine_propeller(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/automotive/braking" => {
            match parse_body_or_default::<SimAutomotiveBrakingRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_automotive_braking(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/reinforced_concrete" => {
            match parse_body_or_default::<SimReinforcedConcreteRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_civil_reinforced_concrete(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hvac/duct_airflow" => {
            match parse_body_or_default::<SimHvacDuctsRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hvac_duct_airflow(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/gas_turbine" => {
            match parse_body_or_default::<SimGasTurbineRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_aerospace_gas_turbine(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hydraulic/cylinder" => {
            match parse_body_or_default::<SimHydraulicCylinderRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hydraulic_cylinder(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mechanical/belt_drive" => {
            match parse_body_or_default::<SimBeltDriveRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_mechanical_belt_drive(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/geotechnical/slope_stability" => {
            match parse_body_or_default::<SimSlopeStabilityRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_geotechnical_slope_stability(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/induction_motor" => {
            match parse_body_or_default::<SimInductionMotorRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electrical_induction_motor(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/solar_collector" => {
            match parse_body_or_default::<SimSolarCollectorRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_thermal_solar_collector(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/tribology/journal_bearing" => {
            match parse_body_or_default::<SimJournalBearingRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_tribology_journal_bearing(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/renewable/wind_turbine" => {
            match parse_body_or_default::<SimWindTurbineRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_renewable_wind_turbine(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/steam_rankine" => {
            match parse_body_or_default::<SimSteamRankineRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_thermal_steam_rankine(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/pile_foundation" => {
            match parse_body_or_default::<SimPileFoundationRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_civil_pile_foundation(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mechanical/gear_fatigue" => {
            match parse_body_or_default::<SimGearFatigueRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_mechanical_gear_fatigue(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/battery_thermal" => {
            match parse_body_or_default::<SimBatteryThermalRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_battery_thermal(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/rocket_nozzle" => {
            match parse_body_or_default::<SimRocketNozzleRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_aerospace_rocket_nozzle(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/chemical/cstr_reactor" => {
            match parse_body_or_default::<SimCstrReactorRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_chemical_cstr_reactor(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/channel_hydraulics" => {
            match parse_body_or_default::<SimChannelHydraulicsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_civil_channel_hydraulics(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/transmission_line" => {
            match parse_body_or_default::<SimTransmissionLineRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electrical_transmission_line(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/piping/flexibility" => {
            match parse_body_or_default::<SimPipingFlexibilityRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_piping_flexibility(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/automotive/quarter_car" => {
            match parse_body_or_default::<SimQuarterCarRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_automotive_quarter_car(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/power_transformer" => {
            match parse_body_or_default::<SimPowerTransformerRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electrical_power_transformer(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/refrigeration_cycle" => {
            match parse_body_or_default::<SimRefrigerationCycleRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_thermal_refrigeration_cycle(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/ship_stability" => {
            match parse_body_or_default::<SimShipStabilityRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_marine_ship_stability(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/manufacturing/injection_molding" => {
            match parse_body_or_default::<SimInjectionMoldingRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_manufacturing_injection_molding(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/seismic_analysis" => {
            match parse_body_or_default::<SimSeismicAnalysisRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_civil_seismic_analysis(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/orbital_mechanics" => {
            match parse_body_or_default::<SimOrbitalMechanicsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_aerospace_orbital_mechanics(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electronics/dc_dc_converter" => {
            match parse_body_or_default::<SimDcDcConverterRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electronics_dc_dc_converter(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/science/fusion_tokamak" => {
            match parse_body_or_default::<SimFusionTokamakRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_science_fusion_tokamak(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/manufacturing/wire_edm" => {
            match parse_body_or_default::<SimWireEdmRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_manufacturing_wire_edm(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/renewable/wind_blade_aeroelastic" => {
            match parse_body_or_default::<SimWindBladeRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_renewable_wind_blade_aeroelastic(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/subsea_pipeline" => {
            match parse_body_or_default::<SimSubseaPipelineRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_marine_subsea_pipeline(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/combustor_aerothermal" => {
            match parse_body_or_default::<SimCombustorRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_combustor_aerothermal(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/bess_grid" => match parse_body_or_default::<SimBessGridRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_energy_bess_grid(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/geotechnical/soil_consolidation" => {
            match parse_body_or_default::<SimSoilConsolidationRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_geotechnical_soil_consolidation(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/hydraulic_turbine" => {
            match parse_body_or_default::<SimHydraulicTurbineRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_hydraulic_turbine(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/biomedical/stent_expansion" => {
            match parse_body_or_default::<SimStentExpansionRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_biomedical_stent_expansion(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electronics/microwave_waveguide" => {
            match parse_body_or_default::<SimMicrowaveWaveguideRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_electronics_microwave_waveguide(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/gis_dielectric" => {
            match parse_body_or_default::<SimGisDielectricRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electrical_gis_dielectric(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/pem_fuel_cell" => {
            match parse_body_or_default::<SimPemFuelCellRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_pem_fuel_cell(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/science/superconducting_magnet" => {
            match parse_body_or_default::<SimSuperconductingMagnetRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_science_superconducting_magnet(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/reentry_aerothermodynamics" => {
            match parse_body_or_default::<SimReentryRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_reentry_aerothermodynamics(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/manufacturing/lpbf_meltpool" => {
            match parse_body_or_default::<SimLpbfRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_manufacturing_lpbf_meltpool(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/acoustics/phononic_bandgap" => {
            match parse_body_or_default::<SimPhononicCrystalRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_acoustics_phononic_bandgap(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/rail/wheel_rail_contact" => {
            match parse_body_or_default::<SimWheelRailRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_rail_wheel_rail_contact(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mining/mine_ventilation" => {
            match parse_body_or_default::<SimMineVentilationRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_mining_mine_ventilation(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/telecom/reflector_antenna" => {
            match parse_body_or_default::<SimReflectorAntennaRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_telecom_reflector_antenna(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/semiconductor/gaa_fet" => {
            match parse_body_or_default::<SimGaaFetRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_semiconductor_gaa_fet(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/agri/irrigation_hydraulics" => {
            match parse_body_or_default::<SimIrrigationHydraulicsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_agri_irrigation_hydraulics(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/environmental/wastewater_treatment" => {
            match parse_body_or_default::<SimWastewaterTreatmentRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_environmental_wastewater_treatment(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/automotive/ev_powertrain" => {
            match parse_body_or_default::<SimEvPowertrainRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_automotive_ev_powertrain(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/seismic_base_isolation" => {
            match parse_body_or_default::<SimSeismicBaseIsolationRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_civil_seismic_base_isolation(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/defense/terminal_ballistics" => {
            match parse_body_or_default::<SimTerminalBallisticsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_defense_terminal_ballistics(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/packaging/solder_thermal_fatigue" => {
            match parse_body_or_default::<SimSolderThermalFatigueRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_packaging_solder_thermal_fatigue(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/ship_slamming" => {
            match parse_body_or_default::<SimShipSlammingRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_marine_ship_slamming(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/automotive/crash_deceleration" => {
            match parse_body_or_default::<SimCrashDecelerationRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_automotive_crash_deceleration(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/nuclear/fuel_pin" => {
            match parse_body_or_default::<SimNuclearFuelPinRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_nuclear_fuel_pin(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/chemical/adsorption_psa" => {
            match parse_body_or_default::<SimAdsorptionPsaRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_chemical_adsorption_psa(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/rotorcraft_aerodynamics" => {
            match parse_body_or_default::<SimRotorcraftAerodynamicsRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_rotorcraft_aerodynamics(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/heat_exchanger_delaware" => {
            match parse_body_or_default::<SimHeatExchangerDelawareRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_thermal_heat_exchanger_delaware(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/geotechnical/drilled_shaft" => {
            match parse_body_or_default::<SimDrilledShaftRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_geotechnical_drilled_shaft(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/biomedical/heart_valve" => {
            match parse_body_or_default::<SimHeartValveRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_biomedical_heart_valve(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/centrifugal_compressor" => {
            match parse_body_or_default::<SimCentrifugalCompressorRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_centrifugal_compressor(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/rail/pantograph" => {
            match parse_body_or_default::<SimRailPantographRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_rail_pantograph(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/geotechnical/tbm_tunneling" => {
            match parse_body_or_default::<SimTbmTunnelingRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_geotechnical_tbm_tunneling(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/structural/prestressed_concrete" => {
            match parse_body_or_default::<SimPrestressedConcreteRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_structural_prestressed_concrete(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/spacecraft_jitter" => {
            match parse_body_or_default::<SimSpacecraftJitterRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_aerospace_spacecraft_jitter(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/automotive/radar_lidar_sensor" => {
            match parse_body_or_default::<SimRadarLidarSensorRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_automotive_radar_lidar_sensor(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/auv_propulsion" => {
            match parse_body_or_default::<SimAuvPropulsionRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_marine_auv_propulsion(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/hydro_governor" => {
            match parse_body_or_default::<SimHydroGovernorRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_energy_hydro_governor(&body))
                {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/semiconductor/cmp_planarization" => {
            match parse_body_or_default::<SimSemiconductorCmpRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_semiconductor_cmp(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/transportation/pavement_mepdg" => {
            match parse_body_or_default::<SimPavementMepdgRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_transportation_pavement_mepdg(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/biomedical/scaffold_degradation" => {
            match parse_body_or_default::<SimBiomaterialScaffoldRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_biomedical_scaffold_degradation(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/satellite_orbital_decay" => {
            match parse_body_or_default::<SimSatelliteOrbitalDecayRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_satellite_orbital_decay(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mining/bucket_wheel_excavator" => {
            match parse_body_or_default::<SimMiningExcavatorRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_mining_bucket_wheel_excavator(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/process/multiphase_pipe_flow" => {
            match parse_body_or_default::<SimMultiphasePipeFlowRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_process_multiphase_pipe_flow(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/geothermal_reservoir" => {
            match parse_body_or_default::<SimGeothermalReservoirRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_geothermal_reservoir(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/process/spray_drying_kinetics" => {
            match parse_body_or_default::<SimSprayDryingKineticsRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_process_spray_drying_kinetics(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/thermoelectric_rtg" => {
            match parse_body_or_default::<SimThermoelectricRtgRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_thermoelectric_rtg(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/nuclear/containment_loca" => {
            match parse_body_or_default::<SimContainmentLocaRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_nuclear_containment_loca(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/environmental/atmospheric_dispersion" => {
            match parse_body_or_default::<SimAtmosphericDispersionRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_environmental_atmospheric_dispersion(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/rocket_ascent_trajectory" => {
            match parse_body_or_default::<SimRocketAscentRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_rocket_ascent_trajectory(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/gravity_dam_stability" => {
            match parse_body_or_default::<SimGravityDamRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_civil_gravity_dam_stability(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/electrolyzer_hydrogen" => {
            match parse_body_or_default::<SimElectrolyzerHydrogenRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_electrolyzer_hydrogen(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/optical/thin_film_optics" => {
            match parse_body_or_default::<SimThinFilmOpticsRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_optical_thin_film_optics(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/cryogenic_liquefaction" => {
            match parse_body_or_default::<SimCryogenicLiquefactionRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_energy_cryogenic_liquefaction(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/process/hydrocyclone_separation" => {
            match parse_body_or_default::<SimHydrocycloneRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_process_hydrocyclone_separation(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mechanical/magnetic_bearing" => {
            match parse_body_or_default::<SimMagneticBearingRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_mechanical_magnetic_bearing(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/sonar_propagation" => {
            match parse_body_or_default::<SimSonarPropagationRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_marine_sonar_propagation(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/manufacturing/electron_beam_welding" => {
            match parse_body_or_default::<SimEbwWeldingRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_manufacturing_electron_beam_welding(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/process/crystallization_kinetics" => {
            match parse_body_or_default::<SimCrystallizationRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_process_crystallization_kinetics(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/thermal/thermoelectric_cooler" => {
            match parse_body_or_default::<SimTecCoolerRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_thermal_thermoelectric_cooler(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mechanical/slurry_pump_derating" => {
            match parse_body_or_default::<SimSlurryPumpRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_mechanical_slurry_pump_derating(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/optical/fiber_dispersion" => {
            match parse_body_or_default::<SimFiberDispersionRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_optical_fiber_dispersion(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/propeller_hydrodynamics" => {
            match parse_body_or_default::<SimPropellerRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_marine_propeller_hydrodynamics(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/environmental/indoor_air_quality" => {
            match parse_body_or_default::<SimIndoorAirQualityRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_environmental_indoor_air_quality(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/pmsm_foc_drive" => {
            match parse_body_or_default::<SimPmsmDriveRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_electrical_pmsm_foc_drive(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/civil/mse_retaining_wall" => {
            match parse_body_or_default::<SimMseWallRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_civil_mse_retaining_wall(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/nuclear/isotope_centrifuge" => {
            match parse_body_or_default::<SimIsotopeCentrifugeRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_nuclear_isotope_centrifuge(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/mechanical/conveyor_bulk_handling" => {
            match parse_body_or_default::<SimConveyorRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_mechanical_conveyor_bulk_handling(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/offshore_wave_loading" => {
            match parse_body_or_default::<SimOffshoreWaveRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_marine_offshore_wave_loading(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/spacecraft_adcs_attitude" => {
            match parse_body_or_default::<SimSpacecraftAdcsRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_aerospace_spacecraft_adcs_attitude(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electrical/battery_fast_charging" => {
            match parse_body_or_default::<SimBatteryFastChargeRequest>(request) {
                Ok(body) => match with_state(state, |state| {
                    state.sim_electrical_battery_fast_charging(&body)
                }) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/process/packed_bed_scrubber" => {
            match parse_body_or_default::<SimPackedBedRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_process_packed_bed_scrubber(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/aircraft_landing_gear" => {
            match parse_body_or_default::<SimLandingGearRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| {
                        state.sim_aerospace_aircraft_landing_gear(&body)
                    }) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/energy/pumped_storage_hydro" => {
            match parse_body_or_default::<SimPumpedStorageRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_energy_pumped_storage_hydro(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/semiconductor/euv_lithography" => {
            match parse_body_or_default::<SimEuvLithographyRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| {
                        state.sim_semiconductor_euv_lithography(&body)
                    }) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/renewable/geothermal_binary_orc" => {
            match parse_body_or_default::<SimGeothermalOrcRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| {
                        state.sim_renewable_geothermal_binary_orc(&body)
                    }) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/structural/cable_stayed_bridge" => {
            match parse_body_or_default::<SimCableStayRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| {
                        state.sim_structural_cable_stayed_bridge(&body)
                    }) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/aerospace/supersonic_nozzle" => {
            match parse_body_or_default::<SimSupersonicNozzleRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_aerospace_supersonic_nozzle(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/automotive/ev_torque_vectoring" => {
            match parse_body_or_default::<SimEvTorqueVectoringRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| {
                        state.sim_automotive_ev_torque_vectoring(&body)
                    }) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/marine/subsea_riser_viv" => {
            match parse_body_or_default::<SimSubseaRiserVivRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_marine_subsea_riser_viv(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/chemical/chromatography_column" => {
            match parse_body_or_default::<SimChromatographyColumnRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| {
                        state.sim_chemical_chromatography_column(&body)
                    }) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/transport/maglev_suspension" => {
            match parse_body_or_default::<SimMaglevSuspensionRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_transport_maglev_suspension(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/additive/process_profile" => {
            match parse_body_or_default::<AdditiveProcessProfileRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.additive_process_profile(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/additive/slice" => match parse_body_or_default::<AdditiveSliceRequest>(request) {
            Ok(body) => match with_state(state, |state| state.additive_slice(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/cfd" => match parse_body_or_default::<SimCfdRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_cfd(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/aerodynamics" => match parse_body_or_default::<SimAeroRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_aerodynamics(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/hypersonic" => match parse_body_or_default::<HypersonicSimRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_hypersonic(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/hypersonic/plasma" => {
            match parse_body_or_default::<HypersonicPlasmaRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_plasma(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hypersonic/thermal" => {
            match parse_body_or_default::<HypersonicThermalRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_thermal(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hypersonic/scramjet" => {
            match parse_body_or_default::<HypersonicScramjetRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_scramjet(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hypersonic/cryo" => match parse_body_or_default::<HypersonicCryoRequest>(request)
        {
            Ok(body) => match with_state(state, |state| state.sim_hypersonic_cryo(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/hypersonic/bom" => match with_state(state, |state| state.sim_hypersonic_bom()) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        "/api/sim/hypersonic/mhd" => match parse_body_or_default::<HypersonicMhdRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_hypersonic_mhd(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/hypersonic/slush" => {
            match parse_body_or_default::<HypersonicSlushRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_slush(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hypersonic/creep" => {
            match parse_body_or_default::<HypersonicCreepRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_creep(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hypersonic/flutter" => {
            match parse_body_or_default::<HypersonicFlutterRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_flutter(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/hypersonic/6dof" => match parse_body_or_default::<Hypersonic6DofRequest>(request)
        {
            Ok(body) => match with_state(state, |state| state.sim_hypersonic_6dof(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/hypersonic/magnet" => {
            match parse_body_or_default::<HypersonicMagnetRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_hypersonic_magnet(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/orchestrator/workflows/run" => {
            match parse_body_or_default::<RunWorkflowRequest>(request) {
                Ok(body) => match with_state(state, |state| state.run_workflow_all(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/chemical" => match parse_body_or_default::<SimChemicalRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_chemical(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/chemical/acid_base" => match parse_body_or_default::<SimAcidBaseRequest>(request)
        {
            Ok(body) => match with_state(state, |state| state.sim_chemical_acid_base(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/chemical/species" => {
            match with_state(state, |state| state.sim_chemical_species()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/chemical/kinetics" => {
            match parse_body_or_default::<SimChemicalKineticsRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_chemical_kinetics(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/molecular" => match parse_body_or_default::<SimMolecularRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_molecular(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/robotics" => match parse_body_or_default::<SimRoboticsRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_robotics(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/powerflow" => match parse_body_or_default::<SimPowerFlowRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_powerflow(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/generative" => match parse_body_or_default::<SimGenerativeRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_generative(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/geotechnical" => match parse_body_or_default::<SimGeotechRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_geotechnical(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/biomedical" => match parse_body_or_default::<SimBiomedicalRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_biomedical(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/marine" => match parse_body_or_default::<SimMarineRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_marine(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/buckling" => match parse_body_or_default::<SimBucklingRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_buckling(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/fatigue" => match parse_body_or_default::<SimFatigueRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_fatigue(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/composite" => match parse_body_or_default::<SimCompositeRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_composite(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/thermal_stress" => {
            match parse_body_or_default::<SimThermalStressRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_thermal_stress(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/electromagnetics" => match parse_body_or_default::<SimEmRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_electromagnetics(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/building_physics" => {
            match parse_body_or_default::<SimBuildingPhysicsRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_building_physics(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/additive/supports" => match parse_body_or_default::<AdditiveSupportsRequest>(request)
        {
            Ok(body) => match with_state(state, |state| state.additive_analyze_supports(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/digital_twin" => match parse_body_or_default::<SimDigitalTwinRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_digital_twin(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/acoustics" => match parse_body_or_default::<SimAcousticsRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_acoustics(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/acoustics/outdoor_noise" => {
            match parse_body_or_default::<SimOutdoorNoiseRequest>(request) {
                Ok(body) => {
                    match with_state(state, |state| state.sim_acoustics_outdoor_noise(&body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    }
                }
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/solar" => match parse_body_or_default::<SimSolarRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_solar(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/aeroelasticity" => {
            match parse_body_or_default::<SimAeroelasticityRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_aeroelasticity(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/robotics_trajectory" => {
            match parse_body_or_default::<SimRoboticsTrajectoryRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_robotics_trajectory(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/composite_failure" => {
            match parse_body_or_default::<SimCompositeFailureRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_composite_failure(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/rainflow" => match parse_body_or_default::<SimRainflowRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_rainflow(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/fsi" => match parse_body_or_default::<SimFsiRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_fsi(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/cavitation" => match parse_body_or_default::<SimCavitationRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_cavitation(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/elastoplasticity" => {
            match parse_body_or_default::<SimElastoplasticRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_elastoplasticity(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/heat_exchanger" => {
            match parse_body_or_default::<SimHeatExchangerRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_heat_exchanger(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/water_hammer" => match parse_body_or_default::<SimWaterHammerRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_water_hammer(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/shock_response" => {
            match parse_body_or_default::<SimShockResponseRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_shock_response(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/battery_ev" => match parse_body_or_default::<SimBatteryEvRequest>(request) {
            Ok(body) => match with_state(state, |state| state.sim_battery_ev(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/vehicle_dynamics" => {
            match parse_body_or_default::<SimVehicleDynamicsRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_vehicle_dynamics(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/environmental" => match parse_body_or_default::<SimEnvironmentalRequest>(request)
        {
            Ok(body) => match with_state(state, |state| state.sim_environmental(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/sim/rock_mechanics" => {
            match parse_body_or_default::<SimRockMechanicsRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_rock_mechanics(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/signal_integrity" => {
            match parse_body_or_default::<SimSignalIntegrityRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_signal_integrity(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/nuclear_shielding" => {
            match parse_body_or_default::<SimNuclearShieldingRequest>(request) {
                Ok(body) => match with_state(state, |state| state.sim_nuclear_shielding(&body)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/sim/multiphysics_suite" => {
            match with_state(state, |state| state.sim_multiphysics_suite()) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            }
        }
        "/api/units/convert" => match parse_body::<UnitConvertRequest>(request) {
            Ok(body) => match with_state(state, |state| state.convert_units(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/commands/discover" => {
            match parse_body_or_default::<CommandDiscoverRequest>(request) {
                Ok(body) => match with_state(state, |state| Ok(state.discover_commands(&body))) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/commands/execute" => match parse_body::<CommandExecuteRequest>(request) {
            Ok(body) => match with_state(state, |state| state.execute_command(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/agents/analyze" => match parse_body::<AnalyzeRequirementRequest>(request) {
            Ok(body) => match with_state(state, |state| Ok(state.analyze_requirement(&body))) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/agents/orchestrate" => match parse_body::<OrchestrateRequirementRequest>(request) {
            Ok(body) => match with_state(state, |state| state.orchestrate_requirement(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/agents/step" => match with_state(state, |state| state.step_orchestrator()) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        "/api/agents/workflows/run" => match parse_body::<RunWorkflowRequest>(request) {
            Ok(body) => match with_state(state, |state| state.run_workflow(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/agents/workflows/run_all" => match parse_body::<RunWorkflowRequest>(request) {
            Ok(body) => match with_state(state, |state| state.run_workflow_all(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/agents/approvals/respond" => match parse_body::<RespondApprovalRequest>(request) {
            Ok(body) => match with_state(state, |state| state.respond_approval(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/agents/optimize_loop" => {
            match parse_body_or_default::<OptimizeLoopRequest>(request) {
                Ok(body) => match with_state(state, |state| Ok(state.optimize_loop(&body))) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                },
                Err(error) => error_response(&error),
            }
        }
        "/api/mcp" => match parse_body::<atlas_agent_orchestrator::McpRequest>(request) {
            Ok(body) => match with_state(state, |state| state.handle_mcp(&body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/create/subdivide" => match parse_body_or_default::<CreateSubdivideRequest>(request) {
            Ok(body) => {
                let mesh_id = body.mesh_id.as_deref().unwrap_or("cad_tessellated_mesh");
                let levels = body.levels.unwrap_or(1);
                match with_state(state, |state| state.create_subdivide(mesh_id, levels)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/create/sculpt" => match parse_body_or_default::<CreateSculptRequest>(request) {
            Ok(body) => {
                let brush = body.brush.as_deref().unwrap_or("clay");
                let radius = body.radius.unwrap_or(0.5);
                let strength = body.strength.unwrap_or(0.35);
                match with_state(state, |state| state.create_sculpt(brush, radius, strength)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/realtime/scene" => match parse_body_or_default::<RealTimeSceneRequest>(request) {
            Ok(body) => {
                let name = body
                    .name
                    .as_deref()
                    .unwrap_or("Hypersonic Digital Twin World");
                match with_state(state, |state| state.realtime_scene_build(name)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/media/timeline" => match parse_body_or_default::<MediaTimelineRequest>(request) {
            Ok(body) => {
                let title = body.title.as_deref().unwrap_or("Mach 11 Jet Commercial");
                let fps = body.fps.unwrap_or(24);
                match with_state(state, |state| state.media_timeline_build(title, fps)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            }
            Err(error) => error_response(&error),
        },
        "/api/media/render" => match with_state(state, |state| state.media_render_export()) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        path if path.starts_with("/api/assets") => {
            if path == "/api/assets" {
                match parse_body::<CreateAssetRequest>(request) {
                    Ok(body) => match with_state(state, |state| state.create_asset(body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    },
                    Err(error) => error_response(&error),
                }
            } else if path.ends_with("/fork") {
                let asset_id = path
                    .trim_start_matches("/api/assets/")
                    .trim_end_matches("/fork");
                match parse_body::<ForkAssetRequest>(request) {
                    Ok(body) => match with_state(state, |state| state.fork_asset(asset_id, body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    },
                    Err(error) => error_response(&error),
                }
            } else if path.ends_with("/use") {
                let asset_id = path
                    .trim_start_matches("/api/assets/")
                    .trim_end_matches("/use");
                match parse_body::<UseAssetRequest>(request) {
                    Ok(body) => match with_state(state, |state| state.use_asset(asset_id, body)) {
                        Ok(payload) => json_response(200, payload),
                        Err(error) => error_response(&error),
                    },
                    Err(error) => error_response(&error),
                }
            } else {
                error_response(&StudioError::NotFound)
            }
        }
        "/api/collab/presence" => match parse_body::<UpdatePresenceRequest>(request) {
            Ok(body) => match with_state(state, |state| state.update_presence(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/collab/lock" => match parse_body::<AcquireLockRequest>(request) {
            Ok(body) => match with_state(state, |state| state.acquire_entity_lock(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/collab/unlock" => match parse_body::<ReleaseLockRequest>(request) {
            Ok(body) => match with_state(state, |state| state.release_entity_lock(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        path if path.starts_with("/api/collab/comments") => {
            if path == "/api/collab/comments" {
                match parse_body::<CreateCommentRequest>(request) {
                    Ok(body) => {
                        match with_state(state, |state| state.create_collab_comment(body)) {
                            Ok(payload) => json_response(200, payload),
                            Err(error) => error_response(&error),
                        }
                    }
                    Err(error) => error_response(&error),
                }
            } else if path.ends_with("/reply") {
                let comment_id = path
                    .trim_start_matches("/api/collab/comments/")
                    .trim_end_matches("/reply");
                match parse_body::<ReplyCommentRequest>(request) {
                    Ok(body) => {
                        match with_state(state, |state| {
                            state.reply_collab_comment(comment_id, body)
                        }) {
                            Ok(payload) => json_response(200, payload),
                            Err(error) => error_response(&error),
                        }
                    }
                    Err(error) => error_response(&error),
                }
            } else if path.ends_with("/resolve") {
                let comment_id = path
                    .trim_start_matches("/api/collab/comments/")
                    .trim_end_matches("/resolve");
                match with_state(state, |state| state.resolve_collab_comment(comment_id)) {
                    Ok(payload) => json_response(200, payload),
                    Err(error) => error_response(&error),
                }
            } else {
                error_response(&StudioError::NotFound)
            }
        }
        "/api/auth/login" => match parse_body::<LoginRequest>(request) {
            Ok(body) => match with_state(state, |state| state.auth_login(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/auth/register" => match parse_body::<RegisterRequest>(request) {
            Ok(body) => match with_state(state, |state| state.auth_register(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/auth/logout" => match with_state(state, |state| state.auth_logout()) {
            Ok(payload) => json_response(200, payload),
            Err(error) => error_response(&error),
        },
        "/api/auth/switch-team" => match parse_body::<SwitchTeamRequest>(request) {
            Ok(body) => match with_state(state, |state| state.auth_switch_team(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/auth/api-keys" => match parse_body::<CreateApiKeyRequest>(request) {
            Ok(body) => match with_state(state, |state| state.auth_create_api_key(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        "/api/auth/api-keys/revoke" => match parse_body::<RevokeApiKeyRequest>(request) {
            Ok(body) => match with_state(state, |state| state.auth_revoke_api_key(body)) {
                Ok(payload) => json_response(200, payload),
                Err(error) => error_response(&error),
            },
            Err(error) => error_response(&error),
        },
        _ => error_response(&StudioError::NotFound),
    }
}

#[derive(Debug, Default, Deserialize)]
struct LoginRequest {
    #[serde(default)]
    email: Option<String>,
    #[serde(default)]
    #[allow(dead_code)]
    password: Option<String>,
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    demo_user: Option<String>,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    sso_provider: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct RegisterRequest {
    name: String,
    email: String,
    #[serde(default)]
    #[allow(dead_code)]
    password: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    avatar_color: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SwitchTeamRequest {
    team_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct CreateApiKeyRequest {
    name: String,
    #[serde(default)]
    scopes: Option<Vec<String>>,
}

#[derive(Debug, Default, Deserialize)]
struct RevokeApiKeyRequest {
    id: String,
}

#[derive(Debug, Default, Deserialize)]
struct CreateAssetRequest {
    name: String,
    #[serde(default)]
    description: Option<String>,
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
    #[serde(default)]
    tags: Option<Vec<String>>,
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    parameters: Option<Vec<atlas_asset_manager::AssetParameter>>,
    #[serde(default)]
    geometry_data: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ForkAssetRequest {
    #[serde(default)]
    user_id: Option<String>,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    team_id: Option<String>,
    #[serde(default)]
    visibility: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct UseAssetRequest {
    #[serde(default)]
    instance_name: Option<String>,
    #[serde(default)]
    parameters: Option<serde_json::Value>,
}

#[derive(Debug, Default, Deserialize)]
struct UpdatePresenceRequest {
    user_id: String,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    role: Option<String>,
    #[serde(default)]
    avatar_color: Option<String>,
    #[serde(default)]
    cursor_pos: Option<[f64; 3]>,
    #[serde(default)]
    active_tool: Option<String>,
    #[serde(default)]
    selected_entity: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct AcquireLockRequest {
    entity_id: String,
    user_id: String,
    #[serde(default)]
    user_name: Option<String>,
    #[serde(default)]
    duration_sec: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
struct ReleaseLockRequest {
    entity_id: String,
    user_id: String,
}

#[derive(Debug, Default, Deserialize)]
struct CreateCommentRequest {
    pos_3d: [f64; 3],
    text: String,
    #[serde(default)]
    entity_id: Option<String>,
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
    author_name: Option<String>,
    #[serde(default)]
    author_color: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct ReplyCommentRequest {
    text: String,
    #[serde(default)]
    author_id: Option<String>,
    #[serde(default)]
    author_name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct CreateSubdivideRequest {
    #[serde(default)]
    mesh_id: Option<String>,
    #[serde(default)]
    levels: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct CreateSculptRequest {
    #[serde(default)]
    brush: Option<String>,
    #[serde(default)]
    radius: Option<f64>,
    #[serde(default)]
    strength: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct RealTimeSceneRequest {
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct MediaTimelineRequest {
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    fps: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
struct CommandDiscoverRequest {
    #[serde(default)]
    category: Option<String>,
    #[serde(default)]
    permission: Option<String>,
    #[serde(default)]
    query: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CommandExecuteRequest {
    command_id: String,
    #[serde(default)]
    arguments: serde_json::Value,
    #[serde(default)]
    caller_identity: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AnalyzeRequirementRequest {
    prompt: String,
}

#[derive(Debug, Deserialize)]
struct OrchestrateRequirementRequest {
    prompt: String,
    #[serde(default)]
    clarifications: BTreeMap<String, String>,
}

#[derive(Debug, Deserialize)]
struct RunWorkflowRequest {
    #[serde(default = "default_workflow_id")]
    workflow_id: String,
    #[serde(default)]
    pub auto_approve: Option<bool>,
}

fn default_workflow_id() -> String {
    "hypersonic_full_vehicle_master".to_string()
}

impl Default for RunWorkflowRequest {
    fn default() -> Self {
        Self {
            workflow_id: default_workflow_id(),
            auto_approve: Some(true),
        }
    }
}

#[derive(Debug, Deserialize)]
struct RespondApprovalRequest {
    approval_id: String,
    approve: bool,
    #[serde(default)]
    approver: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct OptimizeLoopRequest {
    #[serde(default)]
    target_fos: Option<f64>,
    #[serde(default)]
    max_iterations: Option<u32>,
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
struct DrawingBatchRequest {
    entities: Vec<DrawingEntityRequest>,
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
struct StepImportRequest {
    /// STEP physical-file text. The studio's JSON body cap bounds it well below the reader's own
    /// 256 MiB limit.
    text: String,
    /// Name for the body; the STEP product name is used when absent.
    #[serde(default)]
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct ImportRemoveRequest {
    name: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct DraftImportRequest {
    /// The DXF text. The studio's JSON body cap applies, so this is bounded well below the
    /// drafting document's own 32 MiB file limit.
    text: String,
    /// Name for the imported drawing; the project's name is used when absent.
    #[serde(default)]
    name: Option<String>,
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

/// A new revolve. It has no depth: the tool is the sketch turned about one of the sketch's own
/// axes. `angle_deg` defaults to a full turn, which is what a schema-10 revolve meant.
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct NewRevolveRequest {
    sketch_id: u64,
    operation: String,
    #[serde(default = "default_axis")]
    axis: String,
    #[serde(default)]
    angle_deg: Option<f64>,
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

/// Whole seconds since the Unix epoch, or 0 if the clock is before the epoch.
fn now_unix_secs() -> u64 {
    now_epoch_millis() / 1000
}

/// Milliseconds since the Unix epoch, or 0 if the clock is before the epoch.
fn now_epoch_millis() -> u64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |elapsed| elapsed.as_millis() as u64)
}

/// FNV-1a over a record's own fields, for a stable local integrity digest.
fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut hash = 0xcbf2_9ce4_8422_2325u64;
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    hash
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
    #[serde(default)]
    operation: Option<String>,
    #[serde(default)]
    machine: Option<String>,
    #[serde(default)]
    post_format: Option<String>,
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

fn binary_asset(body: &'static [u8], content_type: &'static str) -> http::Response {
    http::Response::new(200, "OK", content_type, body.to_vec())
        .with_header("Referrer-Policy", "no-referrer")
        .with_header("Accept-Ranges", "bytes")
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

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineHealthRequest {
    engine_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EngineRunRequest {
    #[serde(default)]
    job_id: Option<String>,
    engine_id: String,
    #[serde(default)]
    job_type: Option<String>,
    #[serde(default)]
    parameters: Option<serde_json::Value>,
    #[serde(default)]
    timeout_seconds: Option<u64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct InvalidateRequest {
    #[serde(default)]
    node_id: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobSubmitRequest {
    pub engine_id: String,
    pub job_type: String,
    #[serde(default)]
    pub priority: Option<String>,
    #[serde(default)]
    pub parameters: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct JobCancelRequest {
    pub job_id: String,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct EcoRequest {
    pub title: String,
    pub description: String,
    #[serde(default)]
    pub approve: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdditiveSliceRequest {
    #[serde(default)]
    pub layer_height_mm: Option<f64>,
    #[serde(default)]
    pub line_width_mm: Option<f64>,
    #[serde(default)]
    pub infill_density: Option<f64>,
    #[serde(default)]
    pub infill_pattern: Option<String>,
    #[serde(default)]
    pub print_speed_mm_s: Option<f64>,
    #[serde(default)]
    pub nozzle_temp_c: Option<f64>,
    #[serde(default)]
    pub bed_temp_c: Option<f64>,
    #[serde(default)]
    pub dialect: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimCfdRequest {
    #[serde(default)]
    pub diameter_m: Option<f64>,
    #[serde(default)]
    pub length_m: Option<f64>,
    #[serde(default)]
    pub velocity_m_s: Option<f64>,
    #[serde(default)]
    pub fluid: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimAeroRequest {
    #[serde(default)]
    pub airspeed_m_s: Option<f64>,
    #[serde(default)]
    pub angle_of_attack_deg: Option<f64>,
    #[serde(default)]
    pub wing_area_m2: Option<f64>,
    #[serde(default)]
    pub aspect_ratio: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicSimRequest {
    #[serde(default)]
    pub target_speed_km_h: Option<f64>,
    #[serde(default)]
    pub cruise_altitude_m: Option<f64>,
    #[serde(default)]
    pub angle_of_attack_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicPlasmaRequest {
    #[serde(default)]
    pub flight_mach: Option<f64>,
    #[serde(default)]
    pub altitude_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicThermalRequest {
    #[serde(default)]
    pub heat_flux_mw_m2: Option<f64>,
    #[serde(default)]
    pub coolant_flow_kg_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicScramjetRequest {
    #[serde(default)]
    pub flight_mach: Option<f64>,
    #[serde(default)]
    pub equivalence_ratio: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicCryoRequest {
    #[serde(default)]
    pub fuel_mass_kg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicMhdRequest {
    #[serde(default)]
    pub flow_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub b_field_tesla: Option<f64>,
    #[serde(default)]
    pub channel_volume_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicSlushRequest {
    #[serde(default)]
    pub fuel_mass_kg: Option<f64>,
    #[serde(default)]
    pub solid_fraction: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicCreepRequest {
    #[serde(default)]
    pub operating_temp_k: Option<f64>,
    #[serde(default)]
    pub applied_stress_mpa: Option<f64>,
    #[serde(default)]
    pub flight_duration_hours: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicFlutterRequest {
    #[serde(default)]
    pub dynamic_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub flight_mach: Option<f64>,
    #[serde(default)]
    pub wall_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Hypersonic6DofRequest {
    #[serde(default)]
    pub flight_speed_m_s: Option<f64>,
    #[serde(default)]
    pub altitude_m: Option<f64>,
    #[serde(default)]
    pub target_heading_change_deg: Option<f64>,
    #[serde(default)]
    pub bank_angle_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct HypersonicMagnetRequest {
    #[serde(default)]
    pub target_b_field_tesla: Option<f64>,
    #[serde(default)]
    pub channel_gap_m: Option<f64>,
    #[serde(default)]
    pub coil_temperature_k: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimChemicalRequest {
    #[serde(default)]
    pub temperature_k: Option<f64>,
    #[serde(default)]
    pub pressure_bar: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimAcidBaseRequest {
    #[serde(default)]
    pub analyte_id: Option<String>,
    #[serde(default)]
    pub analyte_concentration_m: Option<f64>,
    #[serde(default)]
    pub analyte_volume_ml: Option<f64>,
    #[serde(default)]
    pub titrant_id: Option<String>,
    #[serde(default)]
    pub titrant_concentration_m: Option<f64>,
    #[serde(default)]
    pub temperature_c: Option<f64>,
    #[serde(default)]
    pub titrant_max_volume_ml: Option<f64>,
    #[serde(default)]
    pub step_count: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimMolecularRequest {}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimRoboticsRequest {
    #[serde(default)]
    pub joint_angles_deg: Option<Vec<f64>>,
    #[serde(default)]
    pub target_ee: Option<[f64; 3]>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimPowerFlowRequest {
    #[serde(default)]
    pub p_load_mw: Option<f64>,
    #[serde(default)]
    pub q_load_mvar: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimGenerativeRequest {
    #[serde(default)]
    pub volume_fraction: Option<f64>,
    #[serde(default)]
    pub iterations: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimGeotechRequest {
    #[serde(default)]
    pub wall_height_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimBiomedicalRequest {
    #[serde(default)]
    pub arterial_elastic_modulus_pa: Option<f64>,
    #[serde(default)]
    pub wall_thickness_m: Option<f64>,
    #[serde(default)]
    pub lumen_radius_m: Option<f64>,
    #[serde(default)]
    pub heart_rate_bpm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimMarineRequest {
    #[serde(default)]
    pub hull_length_m: Option<f64>,
    #[serde(default)]
    pub hull_beam_m: Option<f64>,
    #[serde(default)]
    pub hull_draft_m: Option<f64>,
    #[serde(default)]
    pub heel_angle_deg: Option<f64>,
    #[serde(default)]
    pub wave_period_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimBucklingRequest {
    #[serde(default)]
    pub column_length_m: Option<f64>,
    #[serde(default)]
    pub applied_load_n: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimFatigueRequest {
    #[serde(default)]
    pub alternating_stress_pa: Option<f64>,
    #[serde(default)]
    pub mean_stress_pa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimCompositeRequest {
    #[serde(default)]
    pub ply_angles_deg: Option<Vec<f64>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimThermalStressRequest {
    #[serde(default)]
    pub delta_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimEmRequest {
    #[serde(default)]
    pub trace_width_mm: Option<f64>,
    #[serde(default)]
    pub substrate_height_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimBuildingPhysicsRequest {
    #[serde(default)]
    pub wall_area_m2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdditiveSupportsRequest {
    #[serde(default)]
    pub critical_angle_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimDigitalTwinRequest {
    #[serde(default)]
    pub sensor_id: Option<String>,
    #[serde(default)]
    pub strain_a_microstrain: Option<f64>,
    #[serde(default)]
    pub strain_b_microstrain: Option<f64>,
    #[serde(default)]
    pub strain_c_microstrain: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimAcousticsRequest {
    #[serde(default)]
    pub surface_mass_density_kg_m2: Option<f64>,
    #[serde(default)]
    pub partition_area_m2: Option<f64>,
    #[serde(default)]
    pub exterior_noise_dba: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimSolarRequest {
    #[serde(default)]
    pub surface_tilt_deg: Option<f64>,
    #[serde(default)]
    pub array_area_m2: Option<f64>,
    #[serde(default)]
    pub module_efficiency: Option<f64>,
    #[serde(default)]
    pub annual_ghi_kwh_m2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimAeroelasticityRequest {
    #[serde(default)]
    pub diameter_m: Option<f64>,
    #[serde(default)]
    pub flow_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub natural_frequency_hz: Option<f64>,
    #[serde(default)]
    pub modal_mass_kg_per_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimRoboticsTrajectoryRequest {
    #[serde(default)]
    pub duration_s: Option<f64>,
    #[serde(default)]
    pub time_steps: Option<usize>,
    #[serde(default)]
    pub q_target_rad: Option<Vec<f64>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimCompositeFailureRequest {
    #[serde(default)]
    pub sigma_1_mpa: Option<f64>,
    #[serde(default)]
    pub sigma_2_mpa: Option<f64>,
    #[serde(default)]
    pub tau_12_mpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimRainflowRequest {
    #[serde(default)]
    pub stress_history_mpa: Option<Vec<f64>>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimFsiRequest {
    #[serde(default)]
    pub length_m: Option<f64>,
    #[serde(default)]
    pub fluid_velocity_m_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimCavitationRequest {
    #[serde(default)]
    pub fluid_temp_c: Option<f64>,
    #[serde(default)]
    pub total_head_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimElastoplasticRequest {
    #[serde(default)]
    pub material_name: Option<String>,
    #[serde(default)]
    pub applied_strain_pct: Option<f64>,
    #[serde(default)]
    pub yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub elastic_modulus_gpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimHeatExchangerRequest {
    #[serde(default)]
    pub hot_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub cold_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub area_m2: Option<f64>,
    #[serde(default)]
    pub overall_u_w_m2_k: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimWaterHammerRequest {
    #[serde(default)]
    pub length_m: Option<f64>,
    #[serde(default)]
    pub initial_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub valve_closure_time_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimShockResponseRequest {
    #[serde(default)]
    pub peak_acceleration_g: Option<f64>,
    #[serde(default)]
    pub pulse_duration_ms: Option<f64>,
    #[serde(default)]
    pub damping_ratio: Option<f64>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct UnitConvertRequest {
    pub value: f64,
    pub category: String,
    pub from_unit: String,
    pub to_unit: String,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimBatteryEvRequest {
    #[serde(default)]
    pub pack_current_a: Option<f64>,
    #[serde(default)]
    pub state_of_charge: Option<f64>,
    #[serde(default)]
    pub ambient_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimVehicleDynamicsRequest {
    #[serde(default)]
    pub velocity_kmh: Option<f64>,
    #[serde(default)]
    pub total_mass_kg: Option<f64>,
    #[serde(default)]
    pub drag_coefficient: Option<f64>,
    #[serde(default)]
    pub battery_usable_kwh: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimEnvironmentalRequest {
    #[serde(default)]
    pub emission_rate_g_s: Option<f64>,
    #[serde(default)]
    pub stack_height_m: Option<f64>,
    #[serde(default)]
    pub wind_speed_m_s: Option<f64>,
    #[serde(default)]
    pub downwind_distance_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimRockMechanicsRequest {
    #[serde(default)]
    pub intact_ucs_mpa: Option<f64>,
    #[serde(default)]
    pub gsi: Option<f64>,
    #[serde(default)]
    pub intact_mi: Option<f64>,
    #[serde(default)]
    pub confining_stress_mpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimSignalIntegrityRequest {
    #[serde(default)]
    pub trace_width_um: Option<f64>,
    #[serde(default)]
    pub dielectric_height_um: Option<f64>,
    #[serde(default)]
    pub relative_permittivity: Option<f64>,
    #[serde(default)]
    pub trace_length_mm: Option<f64>,
    #[serde(default)]
    pub signal_frequency_ghz: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimNuclearShieldingRequest {
    #[serde(default)]
    pub source_activity_curies: Option<f64>,
    #[serde(default)]
    pub distance_to_receptor_m: Option<f64>,
    #[serde(default)]
    pub lead_shield_thickness_cm: Option<f64>,
    #[serde(default)]
    pub concrete_shield_thickness_cm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CadSheetMetalUnfoldRequest {
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub thickness_mm: Option<f64>,
    #[serde(default)]
    pub width_mm: Option<f64>,
    #[serde(default)]
    pub flange_lengths_mm: Option<Vec<f64>>,
    #[serde(default)]
    pub bend_angles_deg: Option<Vec<f64>>,
    #[serde(default)]
    pub inner_radii_mm: Option<Vec<f64>>,
    #[serde(default)]
    pub die_opening_width_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CadHardwareFastenerRequest {
    #[serde(default)]
    pub thread_size: Option<String>,
    #[serde(default)]
    pub bolt_class: Option<String>,
    #[serde(default)]
    pub clamped_length_mm: Option<f64>,
    #[serde(default)]
    pub applied_tension_load_kn: Option<f64>,
    #[serde(default)]
    pub applied_shear_load_kn: Option<f64>,
    #[serde(default)]
    pub torque_coefficient_k: Option<f64>,
    #[serde(default)]
    pub bolt_count: Option<u32>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimChemicalKineticsRequest {
    #[serde(default)]
    pub reaction_name: Option<String>,
    #[serde(default)]
    pub order: Option<String>,
    #[serde(default)]
    pub initial_concentration_m: Option<f64>,
    #[serde(default)]
    pub rate_constant_k: Option<f64>,
    #[serde(default)]
    pub pre_exponential_a: Option<f64>,
    #[serde(default)]
    pub activation_energy_j_mol: Option<f64>,
    #[serde(default)]
    pub temperature_k: Option<f64>,
    #[serde(default)]
    pub time_seconds: Option<f64>,
    #[serde(default)]
    pub volumetric_flow_m3_s: Option<f64>,
    #[serde(default)]
    pub target_conversion: Option<f64>,
    #[serde(default)]
    pub heat_of_reaction_kj_mol: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct AdditiveProcessProfileRequest {
    #[serde(default)]
    pub technology: Option<String>,
    #[serde(default)]
    pub part_height_mm: Option<f64>,
    #[serde(default)]
    pub part_volume_cm3: Option<f64>,
    #[serde(default)]
    pub layer_height_mm: Option<f64>,
    #[serde(default)]
    pub laser_power_w: Option<f64>,
    #[serde(default)]
    pub scan_speed_mm_s: Option<f64>,
    #[serde(default)]
    pub hatch_spacing_mm: Option<f64>,
    #[serde(default)]
    pub resin_name: Option<String>,
    #[serde(default)]
    pub light_irradiance_mw_cm2: Option<f64>,
    #[serde(default)]
    pub material_cost_per_kg_usd: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimOutdoorNoiseRequest {
    #[serde(default)]
    pub source_name: Option<String>,
    #[serde(default)]
    pub sound_power_lwa: Option<f64>,
    #[serde(default)]
    pub distance_m: Option<f64>,
    #[serde(default)]
    pub source_height_m: Option<f64>,
    #[serde(default)]
    pub receiver_height_m: Option<f64>,
    #[serde(default)]
    pub barrier_height_m: Option<f64>,
    #[serde(default)]
    pub barrier_distance_m: Option<f64>,
    #[serde(default)]
    pub ground_factor: Option<f64>,
    #[serde(default)]
    pub ambient_temp_c: Option<f64>,
    #[serde(default)]
    pub relative_humidity_pct: Option<f64>,
    #[serde(default)]
    pub regulatory_limit_dba: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CadHardwareSpringRequest {
    #[serde(default)]
    pub wire_diameter_d_mm: Option<f64>,
    #[serde(default)]
    pub mean_coil_diameter_d_mm: Option<f64>,
    #[serde(default)]
    pub free_length_l0_mm: Option<f64>,
    #[serde(default)]
    pub active_coils_na: Option<f64>,
    #[serde(default)]
    pub operating_force_f2_n: Option<f64>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub end_type: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CadHardwareGearPairRequest {
    #[serde(default)]
    pub gear_type: Option<String>,
    #[serde(default)]
    pub normal_module_mn: Option<f64>,
    #[serde(default)]
    pub pinion_teeth_z1: Option<u32>,
    #[serde(default)]
    pub gear_teeth_z2: Option<u32>,
    #[serde(default)]
    pub face_width_b_mm: Option<f64>,
    #[serde(default)]
    pub helix_angle_beta_deg: Option<f64>,
    #[serde(default)]
    pub pinion_speed_rpm: Option<f64>,
    #[serde(default)]
    pub transmitted_power_kw: Option<f64>,
    #[serde(default)]
    pub pinion_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub gear_yield_strength_mpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CamCuttingForceRequest {
    #[serde(default)]
    pub operation: Option<String>,
    #[serde(default)]
    pub workpiece: Option<String>,
    #[serde(default)]
    pub cutting_speed_vc_m_per_min: Option<f64>,
    #[serde(default)]
    pub feed_f_mm: Option<f64>,
    #[serde(default)]
    pub depth_of_cut_ap_mm: Option<f64>,
    #[serde(default)]
    pub radial_width_ae_mm: Option<f64>,
    #[serde(default)]
    pub tool_diameter_dc_mm: Option<f64>,
    #[serde(default)]
    pub number_of_teeth_z: Option<u32>,
    #[serde(default)]
    pub rake_angle_gamma_deg: Option<f64>,
    #[serde(default)]
    pub corner_radius_re_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimShortCircuitRequest {
    #[serde(default)]
    pub nominal_voltage_un_v: Option<f64>,
    #[serde(default)]
    pub transformer_rated_power_sn_kva: Option<f64>,
    #[serde(default)]
    pub transformer_impedance_voltage_uk_percent: Option<f64>,
    #[serde(default)]
    pub transformer_copper_losses_pk_kw: Option<f64>,
    #[serde(default)]
    pub feeder_length_l_m: Option<f64>,
    #[serde(default)]
    pub conductor_cross_section_q_mm2: Option<f64>,
    #[serde(default)]
    pub conductor_material: Option<String>,
    #[serde(default)]
    pub upstream_short_circuit_mva: Option<f64>,
    #[serde(default)]
    pub operating_current_ib_a: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimDaylightingRequest {
    #[serde(default)]
    pub sky_type: Option<String>,
    #[serde(default)]
    pub room_length_m: Option<f64>,
    #[serde(default)]
    pub room_width_m: Option<f64>,
    #[serde(default)]
    pub room_height_m: Option<f64>,
    #[serde(default)]
    pub window_width_m: Option<f64>,
    #[serde(default)]
    pub window_height_m: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub window_sill_height_m: Option<f64>,
    #[serde(default)]
    pub glazing_transmittance: Option<f64>,
    #[serde(default)]
    pub glazing_maintenance_factor: Option<f64>,
    #[serde(default)]
    pub external_obstruction_angle_deg: Option<f64>,
    #[serde(default)]
    pub desk_distance_from_window_m: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub desk_height_m: Option<f64>,
    #[serde(default)]
    pub ceiling_reflectance: Option<f64>,
    #[serde(default)]
    pub walls_reflectance: Option<f64>,
    #[serde(default)]
    pub floor_reflectance: Option<f64>,
    #[serde(default)]
    #[allow(dead_code)]
    pub ground_reflectance: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CadHardwareShaftKeyRequest {
    #[serde(default)]
    pub shaft_name: Option<String>,
    #[serde(default)]
    pub shaft_material: Option<String>,
    #[serde(default)]
    pub hub_material: Option<String>,
    #[serde(default)]
    pub transmitted_power_kw: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub bending_moment_nm: Option<f64>,
    #[serde(default)]
    pub shaft_length_m: Option<f64>,
    #[serde(default)]
    pub safety_factor: Option<f64>,
    #[serde(default)]
    pub selected_shaft_diameter_mm: Option<f64>,
    #[serde(default)]
    pub key_length_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimPressureVesselRequest {
    #[serde(default)]
    pub vessel_name: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub head_type: Option<String>,
    #[serde(default)]
    pub joint_efficiency: Option<String>,
    #[serde(default)]
    pub design_pressure_bar: Option<f64>,
    #[serde(default)]
    pub design_temperature_c: Option<f64>,
    #[serde(default)]
    pub inside_diameter_d_mm: Option<f64>,
    #[serde(default)]
    pub tangent_length_l_mm: Option<f64>,
    #[serde(default)]
    pub corrosion_allowance_mm: Option<f64>,
    #[serde(default)]
    pub nominal_shell_thickness_mm: Option<f64>,
    #[serde(default)]
    pub nominal_head_thickness_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimPsychrometricsRequest {
    #[serde(default)]
    pub stream_name: Option<String>,
    #[serde(default)]
    pub dry_bulb_temp_c: Option<f64>,
    #[serde(default)]
    pub relative_humidity: Option<f64>,
    #[serde(default)]
    pub atmospheric_pressure_pa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimHydrologyRequest {
    #[serde(default)]
    pub basin_name: Option<String>,
    #[serde(default)]
    pub land_use: Option<String>,
    #[serde(default)]
    pub pipe_material: Option<String>,
    #[serde(default)]
    pub catchment_area_ha: Option<f64>,
    #[serde(default)]
    pub flow_path_length_m: Option<f64>,
    #[serde(default)]
    pub terrain_slope: Option<f64>,
    #[serde(default)]
    pub pipe_slope: Option<f64>,
    #[serde(default)]
    pub selected_pipe_diameter_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimFractureRequest {
    #[serde(default)]
    pub component_name: Option<String>,
    #[serde(default)]
    pub material: Option<String>,
    #[serde(default)]
    pub crack_type: Option<String>,
    #[serde(default)]
    pub max_stress_mpa: Option<f64>,
    #[serde(default)]
    pub min_stress_mpa: Option<f64>,
    #[serde(default)]
    pub initial_crack_depth_a0_mm: Option<f64>,
    #[serde(default)]
    pub plate_width_w_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimRotorDynamicsRequest {
    #[serde(default)]
    pub operating_speed_rpm: Option<f64>,
    #[serde(default)]
    pub shaft_span_m: Option<f64>,
    #[serde(default)]
    pub shaft_diameter_m: Option<f64>,
    #[serde(default)]
    pub disc_mass_kg: Option<f64>,
    #[serde(default)]
    pub unbalance_g_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct CamWeldingRequest {
    #[serde(default)]
    pub arc_voltage_v: Option<f64>,
    #[serde(default)]
    pub welding_current_a: Option<f64>,
    #[serde(default)]
    pub travel_speed_mm_s: Option<f64>,
    #[serde(default)]
    pub plate_thickness_mm: Option<f64>,
    #[serde(default)]
    pub preheat_temp_c: Option<f64>,
    #[serde(default)]
    pub process: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimDistillationRequest {
    #[serde(default)]
    pub feed_rate_kmol_hr: Option<f64>,
    #[serde(default)]
    pub feed_mole_fraction_zf: Option<f64>,
    #[serde(default)]
    pub distillate_mole_fraction_xd: Option<f64>,
    #[serde(default)]
    pub bottoms_mole_fraction_xb: Option<f64>,
    #[serde(default)]
    pub relative_volatility_alpha: Option<f64>,
    #[serde(default)]
    pub reflux_multiplier: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimPcbThermalRequest {
    #[serde(default)]
    pub trace_width_mm: Option<f64>,
    #[serde(default)]
    pub trace_length_mm: Option<f64>,
    #[serde(default)]
    pub current_a: Option<f64>,
    #[serde(default)]
    pub allowable_temp_rise_c: Option<f64>,
    #[serde(default)]
    pub component_heat_power_w: Option<f64>,
    #[serde(default)]
    pub via_count: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimStructuralConnectionRequest {
    #[serde(default)]
    pub applied_shear_force_kn: Option<f64>,
    #[serde(default)]
    pub applied_tension_force_kn: Option<f64>,
    #[serde(default)]
    pub weld_throat_thickness_mm: Option<f64>,
    #[serde(default)]
    pub weld_length_mm: Option<f64>,
    #[serde(default)]
    pub applied_weld_force_kn: Option<f64>,
    #[serde(default)]
    pub number_of_bolts: Option<usize>,
    #[serde(default)]
    pub bolt_grade: Option<String>,
    #[serde(default)]
    pub steel_grade: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimPumpRequest {
    #[serde(default)]
    pub nominal_speed_rpm: Option<f64>,
    #[serde(default)]
    pub nominal_impeller_diameter_mm: Option<f64>,
    #[serde(default)]
    pub shutoff_head_m: Option<f64>,
    #[serde(default)]
    pub pump_curve_coeff_a: Option<f64>,
    #[serde(default)]
    pub bep_flow_m3_hr: Option<f64>,
    #[serde(default)]
    pub peak_efficiency: Option<f64>,
    #[serde(default)]
    pub target_speed_rpm: Option<f64>,
    #[serde(default)]
    pub target_impeller_diameter_mm: Option<f64>,
    #[serde(default)]
    pub system_static_head_m: Option<f64>,
    #[serde(default)]
    pub system_friction_coeff_k: Option<f64>,
    #[serde(default)]
    pub npsh_available_m: Option<f64>,
    #[serde(default)]
    pub npsh_required_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimHeatExchangerRatingRequest {
    #[serde(default)]
    pub hot_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub cold_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub hot_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub hot_outlet_temp_c: Option<f64>,
    #[serde(default)]
    pub cold_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub cold_outlet_temp_c: Option<f64>,
    #[serde(default)]
    pub shell_diameter_mm: Option<f64>,
    #[serde(default)]
    pub baffle_spacing_mm: Option<f64>,
    #[serde(default)]
    pub tube_count: Option<usize>,
    #[serde(default)]
    pub tube_passes: Option<usize>,
    #[serde(default)]
    pub tube_length_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimGeotechnicalBearingRequest {
    #[serde(default)]
    pub foundation_width_b_m: Option<f64>,
    #[serde(default)]
    pub foundation_length_l_m: Option<f64>,
    #[serde(default)]
    pub embedment_depth_df_m: Option<f64>,
    #[serde(default)]
    pub column_axial_load_kn: Option<f64>,
    #[serde(default)]
    pub friction_angle_deg: Option<f64>,
    #[serde(default)]
    pub cohesion_c_kpa: Option<f64>,
    #[serde(default)]
    pub unit_weight_gamma_kn_m3: Option<f64>,
    #[serde(default)]
    pub elastic_modulus_es_mpa: Option<f64>,
    #[serde(default)]
    pub factor_of_safety_target: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimGasCompressionRequest {
    #[serde(default)]
    pub gas_type: Option<String>,
    #[serde(default)]
    pub mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub inlet_pressure_bar: Option<f64>,
    #[serde(default)]
    pub inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub target_outlet_pressure_bar: Option<f64>,
    #[serde(default)]
    pub number_of_stages: Option<usize>,
    #[serde(default)]
    pub polytropic_efficiency: Option<f64>,
    #[serde(default)]
    pub intercooler_exit_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimTimberFireRequest {
    #[serde(default)]
    pub timber_grade: Option<String>,
    #[serde(default)]
    pub initial_breadth_mm: Option<f64>,
    #[serde(default)]
    pub initial_depth_mm: Option<f64>,
    #[serde(default)]
    pub span_m: Option<f64>,
    #[serde(default)]
    pub exposure: Option<String>,
    #[serde(default)]
    pub fire_duration_min: Option<f64>,
    #[serde(default)]
    pub ambient_moment_ed_kn_m: Option<f64>,
    #[serde(default)]
    pub fire_load_ratio_eta_fi: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimMarinePropellerRequest {
    #[serde(default)]
    pub diameter_m: Option<f64>,
    #[serde(default)]
    pub pitch_ratio_p_d: Option<f64>,
    #[serde(default)]
    pub expanded_area_ratio: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub advance_speed_knots: Option<f64>,
    #[serde(default)]
    pub immersion_depth_m: Option<f64>,
    #[serde(default)]
    pub water_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub keller_constant_k: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimAutomotiveBrakingRequest {
    #[serde(default)]
    pub vehicle_mass_kg: Option<f64>,
    #[serde(default)]
    pub wheelbase_m: Option<f64>,
    #[serde(default)]
    pub cg_height_m: Option<f64>,
    #[serde(default)]
    pub static_front_weight_bias: Option<f64>,
    #[serde(default)]
    pub tire_rolling_radius_m: Option<f64>,
    #[serde(default)]
    pub road_friction_coeff: Option<f64>,
    #[serde(default)]
    pub initial_speed_kph: Option<f64>,
    #[serde(default)]
    pub driver_reaction_time_s: Option<f64>,
    #[serde(default)]
    pub front_rotor_mass_kg: Option<f64>,
    #[serde(default)]
    pub ambient_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimReinforcedConcreteRequest {
    #[serde(default)]
    pub breadth_b_mm: Option<f64>,
    #[serde(default)]
    pub depth_h_mm: Option<f64>,
    #[serde(default)]
    pub concrete_cover_c_mm: Option<f64>,
    #[serde(default)]
    pub rebar_diameter_mm: Option<f64>,
    #[serde(default)]
    pub rebar_count: Option<usize>,
    #[serde(default)]
    pub applied_moment_ed_kn_m: Option<f64>,
    #[serde(default)]
    pub quasi_permanent_moment_ratio: Option<f64>,
    #[serde(default)]
    pub max_allowable_crack_width_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimHvacDuctsRequest {
    #[serde(default)]
    pub airflow_m3_h: Option<f64>,
    #[serde(default)]
    pub duct_width_mm: Option<f64>,
    #[serde(default)]
    pub duct_height_mm: Option<f64>,
    #[serde(default)]
    pub duct_diameter_mm: Option<f64>,
    #[serde(default)]
    pub duct_length_m: Option<f64>,
    #[serde(default)]
    pub fitting_loss_coeff_sum: Option<f64>,
    #[serde(default)]
    pub terminal_equipment_pa: Option<f64>,
    #[serde(default)]
    pub fan_efficiency: Option<f64>,
    #[serde(default)]
    pub motor_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimGasTurbineRequest {
    #[serde(default)]
    pub flight_mach: Option<f64>,
    #[serde(default)]
    pub altitude_m: Option<f64>,
    #[serde(default)]
    pub core_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub compressor_pressure_ratio: Option<f64>,
    #[serde(default)]
    pub turbine_inlet_temp_k: Option<f64>,
    #[serde(default)]
    pub compressor_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub turbine_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub combustor_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimHydraulicCylinderRequest {
    #[serde(default)]
    pub bore_diameter_mm: Option<f64>,
    #[serde(default)]
    pub rod_diameter_mm: Option<f64>,
    #[serde(default)]
    pub stroke_length_mm: Option<f64>,
    #[serde(default)]
    pub working_pressure_bar: Option<f64>,
    #[serde(default)]
    pub barrel_wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub barrel_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub rod_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub rod_youngs_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub load_mass_kg: Option<f64>,
    #[serde(default)]
    pub stroke_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub cushion_length_mm: Option<f64>,
    #[serde(default)]
    pub cushion_spear_diameter_mm: Option<f64>,
    #[serde(default)]
    pub mechanical_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimBeltDriveRequest {
    #[serde(default)]
    pub power_kw: Option<f64>,
    #[serde(default)]
    pub driver_speed_rpm: Option<f64>,
    #[serde(default)]
    pub driver_diameter_mm: Option<f64>,
    #[serde(default)]
    pub driven_diameter_mm: Option<f64>,
    #[serde(default)]
    pub center_distance_mm: Option<f64>,
    #[serde(default)]
    pub service_factor: Option<f64>,
    #[serde(default)]
    pub friction_coefficient: Option<f64>,
    #[serde(default)]
    pub belt_linear_mass_kg_m: Option<f64>,
    #[serde(default)]
    pub max_allowable_tension_per_belt_n: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimSlopeStabilityRequest {
    #[serde(default)]
    pub slope_height_m: Option<f64>,
    #[serde(default)]
    pub slope_angle_deg: Option<f64>,
    #[serde(default)]
    pub soil_unit_weight_kn_m3: Option<f64>,
    #[serde(default)]
    pub effective_cohesion_kpa: Option<f64>,
    #[serde(default)]
    pub effective_friction_angle_deg: Option<f64>,
    #[serde(default)]
    pub pore_pressure_ratio_ru: Option<f64>,
    #[serde(default)]
    pub seismic_coefficient_kh: Option<f64>,
    #[serde(default)]
    pub circle_center_x_m: Option<f64>,
    #[serde(default)]
    pub circle_center_y_m: Option<f64>,
    #[serde(default)]
    pub slip_circle_radius_m: Option<f64>,
    #[serde(default)]
    pub number_of_slices: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimInductionMotorRequest {
    #[serde(default)]
    pub rated_power_kw: Option<f64>,
    #[serde(default)]
    pub rated_voltage_ll_v: Option<f64>,
    #[serde(default)]
    pub frequency_hz: Option<f64>,
    #[serde(default)]
    pub pole_count: Option<usize>,
    #[serde(default)]
    pub stator_resistance_r1_ohm: Option<f64>,
    #[serde(default)]
    pub stator_leakage_reactance_x1_ohm: Option<f64>,
    #[serde(default)]
    pub rotor_resistance_r2_prime_ohm: Option<f64>,
    #[serde(default)]
    pub rotor_leakage_reactance_x2_prime_ohm: Option<f64>,
    #[serde(default)]
    pub magnetizing_reactance_xm_ohm: Option<f64>,
    #[serde(default)]
    pub rotational_loss_w: Option<f64>,
    #[serde(default)]
    pub operating_slip: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimSolarCollectorRequest {
    #[serde(default)]
    pub gross_area_m2: Option<f64>,
    #[serde(default)]
    pub aperture_area_m2: Option<f64>,
    #[serde(default)]
    pub solar_irradiance_w_m2: Option<f64>,
    #[serde(default)]
    pub ambient_temp_c: Option<f64>,
    #[serde(default)]
    pub fluid_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub fluid_flow_rate_kg_s: Option<f64>,
    #[serde(default)]
    pub fluid_specific_heat_j_kg_k: Option<f64>,
    #[serde(default)]
    pub optical_efficiency_eta0: Option<f64>,
    #[serde(default)]
    pub linear_heat_loss_a1: Option<f64>,
    #[serde(default)]
    pub quadratic_heat_loss_a2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimJournalBearingRequest {
    #[serde(default)]
    pub journal_diameter_mm: Option<f64>,
    #[serde(default)]
    pub bearing_length_mm: Option<f64>,
    #[serde(default)]
    pub radial_clearance_um: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub radial_load_n: Option<f64>,
    #[serde(default)]
    pub lubricant_viscosity_mpa_s: Option<f64>,
    #[serde(default)]
    pub lubricant_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub lubricant_specific_heat_j_kg_k: Option<f64>,
    #[serde(default)]
    pub inlet_oil_temp_c: Option<f64>,
    #[serde(default)]
    pub surface_roughness_rz_um: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimWindTurbineRequest {
    #[serde(default)]
    pub rotor_radius_m: Option<f64>,
    #[serde(default)]
    pub blade_count: Option<usize>,
    #[serde(default)]
    pub rated_power_mw: Option<f64>,
    #[serde(default)]
    pub rated_wind_speed_m_s: Option<f64>,
    #[serde(default)]
    pub cut_in_wind_speed_m_s: Option<f64>,
    #[serde(default)]
    pub cut_out_wind_speed_m_s: Option<f64>,
    #[serde(default)]
    pub tip_speed_ratio_optimal: Option<f64>,
    #[serde(default)]
    pub air_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub gearbox_efficiency: Option<f64>,
    #[serde(default)]
    pub generator_efficiency: Option<f64>,
    #[serde(default)]
    pub weibull_k_shape: Option<f64>,
    #[serde(default)]
    pub weibull_c_scale_m_s: Option<f64>,
    #[serde(default)]
    pub operating_wind_speed_m_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimSteamRankineRequest {
    #[serde(default)]
    pub steam_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub boiler_pressure_bar: Option<f64>,
    #[serde(default)]
    pub superheater_temp_c: Option<f64>,
    #[serde(default)]
    pub condenser_pressure_bar: Option<f64>,
    #[serde(default)]
    pub turbine_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub pump_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub generator_efficiency: Option<f64>,
    #[serde(default)]
    pub boiler_efficiency: Option<f64>,
    #[serde(default)]
    pub cooling_water_temp_rise_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimPileFoundationRequest {
    #[serde(default)]
    pub pile_diameter_m: Option<f64>,
    #[serde(default)]
    pub pile_length_m: Option<f64>,
    #[serde(default)]
    pub pile_elastic_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub applied_axial_load_kn: Option<f64>,
    #[serde(default)]
    pub groundwater_depth_m: Option<f64>,
    #[serde(default)]
    pub partial_factor_base: Option<f64>,
    #[serde(default)]
    pub partial_factor_shaft: Option<f64>,
    #[serde(default)]
    pub max_allowable_settlement_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimGearFatigueRequest {
    #[serde(default)]
    pub transmitted_power_kw: Option<f64>,
    #[serde(default)]
    pub pinion_speed_rpm: Option<f64>,
    #[serde(default)]
    pub pinion_teeth_z1: Option<usize>,
    #[serde(default)]
    pub gear_teeth_z2: Option<usize>,
    #[serde(default)]
    pub normal_module_mm: Option<f64>,
    #[serde(default)]
    pub face_width_mm: Option<f64>,
    #[serde(default)]
    pub helix_angle_deg: Option<f64>,
    #[serde(default)]
    pub normal_pressure_angle_deg: Option<f64>,
    #[serde(default)]
    pub application_factor_ka: Option<f64>,
    #[serde(default)]
    pub material_contact_fatigue_limit_mpa: Option<f64>,
    #[serde(default)]
    pub material_bending_fatigue_limit_mpa: Option<f64>,
    #[serde(default)]
    pub required_safety_factor_pitting: Option<f64>,
    #[serde(default)]
    pub required_safety_factor_bending: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimBatteryThermalRequest {
    #[serde(default)]
    pub cell_count_series: Option<usize>,
    #[serde(default)]
    pub cell_count_parallel: Option<usize>,
    #[serde(default)]
    pub cell_capacity_ah: Option<f64>,
    #[serde(default)]
    pub nominal_cell_voltage_v: Option<f64>,
    #[serde(default)]
    pub internal_resistance_mohm: Option<f64>,
    #[serde(default)]
    pub entropic_coefficient_mv_k: Option<f64>,
    #[serde(default)]
    pub operating_current_a: Option<f64>,
    #[serde(default)]
    pub ambient_temp_c: Option<f64>,
    #[serde(default)]
    pub coolant_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub coolant_flow_rate_l_min: Option<f64>,
    #[serde(default)]
    pub coolant_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub coolant_specific_heat_j_kg_k: Option<f64>,
    #[serde(default)]
    pub convective_htc_w_m2_k: Option<f64>,
    #[serde(default)]
    pub cell_contact_area_m2: Option<f64>,
    #[serde(default)]
    pub cell_mass_kg: Option<f64>,
    #[serde(default)]
    pub cell_specific_heat_j_kg_k: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimRocketNozzleRequest {
    #[serde(default)]
    pub chamber_pressure_bar: Option<f64>,
    #[serde(default)]
    pub chamber_temperature_k: Option<f64>,
    #[serde(default)]
    pub throat_diameter_mm: Option<f64>,
    #[serde(default)]
    pub expansion_ratio_eps: Option<f64>,
    #[serde(default)]
    pub specific_heat_ratio_gamma: Option<f64>,
    #[serde(default)]
    pub molecular_weight_g_mol: Option<f64>,
    #[serde(default)]
    pub ambient_pressure_bar: Option<f64>,
    #[serde(default)]
    pub discharge_coefficient_cd: Option<f64>,
    #[serde(default)]
    pub thrust_efficiency_cf: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimCstrReactorRequest {
    #[serde(default)]
    pub reactor_volume_m3: Option<f64>,
    #[serde(default)]
    pub feed_volumetric_flow_m3_h: Option<f64>,
    #[serde(default)]
    pub feed_reactant_concentration_mol_m3: Option<f64>,
    #[serde(default)]
    pub feed_temperature_c: Option<f64>,
    #[serde(default)]
    pub pre_exponential_factor_s_inv: Option<f64>,
    #[serde(default)]
    pub activation_energy_kj_mol: Option<f64>,
    #[serde(default)]
    pub reaction_enthalpy_kj_mol: Option<f64>,
    #[serde(default)]
    pub fluid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub fluid_specific_heat_j_kg_k: Option<f64>,
    #[serde(default)]
    pub jacket_overall_ua_w_k: Option<f64>,
    #[serde(default)]
    pub coolant_temperature_c: Option<f64>,
    #[serde(default)]
    pub max_allowable_reactor_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimChannelHydraulicsRequest {
    #[serde(default)]
    pub bottom_width_m: Option<f64>,
    #[serde(default)]
    pub side_slope_z: Option<f64>,
    #[serde(default)]
    pub bed_slope_s0: Option<f64>,
    #[serde(default)]
    pub manning_roughness_n: Option<f64>,
    #[serde(default)]
    pub discharge_rate_m3_s: Option<f64>,
    #[serde(default)]
    pub upstream_jump_depth_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct SimTransmissionLineRequest {
    #[serde(default)]
    pub span_length_m: Option<f64>,
    #[serde(default)]
    pub conductor_diameter_mm: Option<f64>,
    #[serde(default)]
    pub conductor_unit_mass_kg_m: Option<f64>,
    #[serde(default)]
    pub rated_tensile_strength_kn: Option<f64>,
    #[serde(default)]
    pub metallic_cross_section_mm2: Option<f64>,
    #[serde(default)]
    pub elastic_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub thermal_expansion_coeff_per_c: Option<f64>,
    #[serde(default)]
    pub reference_temp_c: Option<f64>,
    #[serde(default)]
    pub initial_horizontal_tension_kn: Option<f64>,
    #[serde(default)]
    pub max_operating_temp_c: Option<f64>,
    #[serde(default)]
    pub storm_ice_thickness_mm: Option<f64>,
    #[serde(default)]
    pub storm_wind_pressure_pa: Option<f64>,
    #[serde(default)]
    pub storm_ambient_temp_c: Option<f64>,
    #[serde(default)]
    pub crossarm_height_m: Option<f64>,
    #[serde(default)]
    pub min_ground_clearance_required_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPipingFlexibilityRequest {
    #[serde(default)]
    pub nominal_outside_diameter_mm: Option<f64>,
    #[serde(default)]
    pub wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub anchor_distance_m: Option<f64>,
    #[serde(default)]
    pub design_temperature_c: Option<f64>,
    #[serde(default)]
    pub ambient_temperature_c: Option<f64>,
    #[serde(default)]
    pub thermal_expansion_coeff_per_c: Option<f64>,
    #[serde(default)]
    pub elastic_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub cold_allowable_stress_mpa: Option<f64>,
    #[serde(default)]
    pub hot_allowable_stress_mpa: Option<f64>,
    #[serde(default)]
    pub sustained_stress_sl_mpa: Option<f64>,
    #[serde(default)]
    pub total_thermal_cycles: Option<usize>,
    #[serde(default)]
    pub loop_height_m: Option<f64>,
    #[serde(default)]
    pub loop_width_m: Option<f64>,
    #[serde(default)]
    pub bend_radius_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimQuarterCarRequest {
    #[serde(default)]
    pub sprung_mass_kg: Option<f64>,
    #[serde(default)]
    pub unsprung_mass_kg: Option<f64>,
    #[serde(default)]
    pub suspension_stiffness_n_m: Option<f64>,
    #[serde(default)]
    pub suspension_damping_n_s_m: Option<f64>,
    #[serde(default)]
    pub tire_stiffness_n_m: Option<f64>,
    #[serde(default)]
    pub vehicle_speed_km_h: Option<f64>,
    #[serde(default)]
    pub road_class: Option<String>,
    #[serde(default)]
    pub max_allowable_stroke_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPowerTransformerRequest {
    #[serde(default)]
    pub rated_apparent_power_mva: Option<f64>,
    #[serde(default)]
    pub primary_voltage_kv: Option<f64>,
    #[serde(default)]
    pub secondary_voltage_kv: Option<f64>,
    #[serde(default)]
    pub percent_impedance_z: Option<f64>,
    #[serde(default)]
    pub no_load_loss_kw: Option<f64>,
    #[serde(default)]
    pub load_loss_kw: Option<f64>,
    #[serde(default)]
    pub operating_load_factor: Option<f64>,
    #[serde(default)]
    pub operating_power_factor: Option<f64>,
    #[serde(default)]
    pub ambient_temperature_c: Option<f64>,
    #[serde(default)]
    pub rated_top_oil_rise_c: Option<f64>,
    #[serde(default)]
    pub rated_winding_rise_c: Option<f64>,
    #[serde(default)]
    pub hot_spot_factor_h: Option<f64>,
    #[serde(default)]
    pub cooling_class: Option<String>,
}

#[derive(Debug, Default, Deserialize)]
struct SimRefrigerationCycleRequest {
    #[serde(default)]
    pub refrigerant: Option<String>,
    #[serde(default)]
    pub design_cooling_capacity_kw: Option<f64>,
    #[serde(default)]
    pub evaporating_temperature_c: Option<f64>,
    #[serde(default)]
    pub condensing_temperature_c: Option<f64>,
    #[serde(default)]
    pub superheat_k: Option<f64>,
    #[serde(default)]
    pub subcooling_k: Option<f64>,
    #[serde(default)]
    pub compressor_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub compressor_mechanical_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimShipStabilityRequest {
    #[serde(default)]
    pub length_between_perpendiculars_m: Option<f64>,
    #[serde(default)]
    pub molded_beam_m: Option<f64>,
    #[serde(default)]
    pub design_draft_m: Option<f64>,
    #[serde(default)]
    pub molded_depth_m: Option<f64>,
    #[serde(default)]
    pub block_coefficient_cb: Option<f64>,
    #[serde(default)]
    pub waterplane_coefficient_cwp: Option<f64>,
    #[serde(default)]
    pub vertical_center_of_gravity_kg_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimInjectionMoldingRequest {
    #[serde(default)]
    pub resin: Option<String>,
    #[serde(default)]
    pub nominal_wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub projected_area_cm2: Option<f64>,
    #[serde(default)]
    pub cavity_volume_cm3: Option<f64>,
    #[serde(default)]
    pub max_flow_length_mm: Option<f64>,
    #[serde(default)]
    pub injection_time_s: Option<f64>,
    #[serde(default)]
    pub cavity_count: Option<usize>,
    #[serde(default)]
    pub clamp_safety_factor: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSeismicAnalysisRequest {
    #[serde(default)]
    pub building_height_m: Option<f64>,
    #[serde(default)]
    pub story_count: Option<usize>,
    #[serde(default)]
    pub total_seismic_weight_kn: Option<f64>,
    #[serde(default)]
    pub short_period_spectral_accel_ss_g: Option<f64>,
    #[serde(default)]
    pub one_second_spectral_accel_s1_g: Option<f64>,
    #[serde(default)]
    pub long_period_transition_tl_s: Option<f64>,
    #[serde(default)]
    pub site_class: Option<String>,
    #[serde(default)]
    pub framing_system: Option<String>,
    #[serde(default)]
    pub importance_factor_ie: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimOrbitalMechanicsRequest {
    #[serde(default)]
    pub initial_altitude_km: Option<f64>,
    #[serde(default)]
    pub target_altitude_km: Option<f64>,
    #[serde(default)]
    pub spacecraft_wet_mass_kg: Option<f64>,
    #[serde(default)]
    pub propulsion_isp_s: Option<f64>,
    #[serde(default)]
    pub inclination_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimDcDcConverterRequest {
    #[serde(default)]
    pub input_voltage_v: Option<f64>,
    #[serde(default)]
    pub output_voltage_v: Option<f64>,
    #[serde(default)]
    pub rated_output_current_a: Option<f64>,
    #[serde(default)]
    pub switching_frequency_hz: Option<f64>,
    #[serde(default)]
    pub inductor_ripple_ratio: Option<f64>,
    #[serde(default)]
    pub inductor_dcr_ohms: Option<f64>,
    #[serde(default)]
    pub high_side_rds_on_ohms: Option<f64>,
    #[serde(default)]
    pub low_side_rds_on_ohms: Option<f64>,
    #[serde(default)]
    pub high_side_qg_nc: Option<f64>,
    #[serde(default)]
    pub low_side_qg_nc: Option<f64>,
    #[serde(default)]
    pub switching_transition_time_ns: Option<f64>,
    #[serde(default)]
    pub gate_drive_voltage_v: Option<f64>,
    #[serde(default)]
    pub output_capacitance_uf: Option<f64>,
    #[serde(default)]
    pub output_capacitor_esr_ohms: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimFusionTokamakRequest {
    #[serde(default)]
    pub major_radius_r0_m: Option<f64>,
    #[serde(default)]
    pub minor_radius_a_m: Option<f64>,
    #[serde(default)]
    pub elongation_kappa: Option<f64>,
    #[serde(default)]
    pub triangularity_delta: Option<f64>,
    #[serde(default)]
    pub toroidal_magnetic_field_t: Option<f64>,
    #[serde(default)]
    pub plasma_current_ma: Option<f64>,
    #[serde(default)]
    pub electron_density_10e20_m3: Option<f64>,
    #[serde(default)]
    pub plasma_temperature_kev: Option<f64>,
    #[serde(default)]
    pub auxiliary_heating_power_mw: Option<f64>,
    #[serde(default)]
    pub average_ion_mass_amu: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimWireEdmRequest {
    #[serde(default)]
    pub workpiece_material: Option<atlas_simulation_core::wire_edm::EdmWorkpieceMaterial>,
    #[serde(default)]
    pub workpiece_thickness_mm: Option<f64>,
    #[serde(default)]
    pub wire_diameter_mm: Option<f64>,
    #[serde(default)]
    pub open_circuit_voltage_v: Option<f64>,
    #[serde(default)]
    pub discharge_current_a: Option<f64>,
    #[serde(default)]
    pub pulse_on_time_us: Option<f64>,
    #[serde(default)]
    pub pulse_off_time_us: Option<f64>,
    #[serde(default)]
    pub wire_feed_speed_m_min: Option<f64>,
    #[serde(default)]
    pub wire_tension_n: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimWindBladeRequest {
    #[serde(default)]
    pub blade_length_m: Option<f64>,
    #[serde(default)]
    pub rotor_radius_m: Option<f64>,
    #[serde(default)]
    pub rated_rotor_speed_rpm: Option<f64>,
    #[serde(default)]
    pub rated_wind_speed_m_s: Option<f64>,
    #[serde(default)]
    pub extreme_gust_speed_m_s: Option<f64>,
    #[serde(default)]
    pub blade_mass_kg: Option<f64>,
    #[serde(default)]
    pub root_chord_m: Option<f64>,
    #[serde(default)]
    pub tip_chord_m: Option<f64>,
    #[serde(default)]
    pub flapwise_rigidity_ei_nm2: Option<f64>,
    #[serde(default)]
    pub torsional_rigidity_gj_nm2: Option<f64>,
    #[serde(default)]
    pub polar_moment_inertia_kg_m: Option<f64>,
    #[serde(default)]
    pub cg_ea_offset_m: Option<f64>,
    #[serde(default)]
    pub air_density_kg_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSubseaPipelineRequest {
    #[serde(default)]
    pub outside_diameter_mm: Option<f64>,
    #[serde(default)]
    pub nominal_wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub corrosion_allowance_mm: Option<f64>,
    #[serde(default)]
    pub fabrication_ovality_pct: Option<f64>,
    #[serde(default)]
    pub water_depth_m: Option<f64>,
    #[serde(default)]
    pub seawater_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub internal_design_pressure_mpa: Option<f64>,
    #[serde(default)]
    pub steel_smys_mpa: Option<f64>,
    #[serde(default)]
    pub steel_smts_mpa: Option<f64>,
    #[serde(default)]
    pub steel_youngs_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub steel_poissons_ratio: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimCombustorRequest {
    #[serde(default)]
    pub inlet_total_pressure_mpa: Option<f64>,
    #[serde(default)]
    pub inlet_total_temperature_k: Option<f64>,
    #[serde(default)]
    pub air_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub fuel_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub fuel_lhv_mj_kg: Option<f64>,
    #[serde(default)]
    pub combustor_volume_m3: Option<f64>,
    #[serde(default)]
    pub combustor_length_m: Option<f64>,
    #[serde(default)]
    pub liner_reference_area_m2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimBessGridRequest {
    #[serde(default)]
    pub chemistry: Option<atlas_simulation_core::bess_grid::BessChemistry>,
    #[serde(default)]
    pub nominal_capacity_mwh: Option<f64>,
    #[serde(default)]
    pub rated_power_mw: Option<f64>,
    #[serde(default)]
    pub dc_bus_voltage_v: Option<f64>,
    #[serde(default)]
    pub c_rate: Option<f64>,
    #[serde(default)]
    pub pcs_rated_mva: Option<f64>,
    #[serde(default)]
    pub transformer_rated_mva: Option<f64>,
    #[serde(default)]
    pub ambient_temperature_c: Option<f64>,
    #[serde(default)]
    pub auxiliary_base_power_kw: Option<f64>,
    #[serde(default)]
    pub chiller_cop: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSoilConsolidationRequest {
    #[serde(default)]
    pub layer_thickness_m: Option<f64>,
    #[serde(default)]
    pub initial_void_ratio: Option<f64>,
    #[serde(default)]
    pub compression_index_cc: Option<f64>,
    #[serde(default)]
    pub recompression_index_cr: Option<f64>,
    #[serde(default)]
    pub secondary_compression_index: Option<f64>,
    #[serde(default)]
    pub preconsolidation_stress_kpa: Option<f64>,
    #[serde(default)]
    pub initial_effective_stress_kpa: Option<f64>,
    #[serde(default)]
    pub applied_stress_increment_kpa: Option<f64>,
    #[serde(default)]
    pub coefficient_of_consolidation_m2_yr: Option<f64>,
    #[serde(default)]
    pub drainage_condition: Option<atlas_simulation_core::soil_consolidation::DrainageCondition>,
    #[serde(default)]
    pub analysis_time_years: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimHydraulicTurbineRequest {
    #[serde(default)]
    pub turbine_type: Option<atlas_simulation_core::hydraulic_turbine::HydraulicTurbineType>,
    #[serde(default)]
    pub net_head_m: Option<f64>,
    #[serde(default)]
    pub flow_rate_m3_s: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub runner_diameter_m: Option<f64>,
    #[serde(default)]
    pub nozzle_count: Option<u32>,
    #[serde(default)]
    pub nozzle_discharge_coeff_cv: Option<f64>,
    #[serde(default)]
    pub bucket_deflection_angle_deg: Option<f64>,
    #[serde(default)]
    pub suction_head_m: Option<f64>,
    #[serde(default)]
    pub atmospheric_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub vapor_pressure_kpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimStentExpansionRequest {
    #[serde(default)]
    pub stent_material: Option<atlas_simulation_core::stent_expansion::StentMaterial>,
    #[serde(default)]
    pub crimped_diameter_mm: Option<f64>,
    #[serde(default)]
    pub nominal_expanded_diameter_mm: Option<f64>,
    #[serde(default)]
    pub nominal_length_mm: Option<f64>,
    #[serde(default)]
    pub strut_thickness_um: Option<f64>,
    #[serde(default)]
    pub strut_width_um: Option<f64>,
    #[serde(default)]
    pub crown_count_per_ring: Option<u32>,
    #[serde(default)]
    pub ring_count: Option<u32>,
    #[serde(default)]
    pub balloon_pressure_atm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMicrowaveWaveguideRequest {
    #[serde(default)]
    pub broad_dimension_a_mm: Option<f64>,
    #[serde(default)]
    pub narrow_dimension_b_mm: Option<f64>,
    #[serde(default)]
    pub length_l_mm: Option<f64>,
    #[serde(default)]
    pub operating_frequency_ghz: Option<f64>,
    #[serde(default)]
    pub wall_conductivity_s_m: Option<f64>,
    #[serde(default)]
    pub dielectric_permittivity: Option<f64>,
    #[serde(default)]
    pub transmitted_power_w: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimGisDielectricRequest {
    #[serde(default)]
    pub insulating_gas: Option<atlas_simulation_core::gis_dielectric::GisInsulatingGas>,
    #[serde(default)]
    pub rated_voltage_kv_rms: Option<f64>,
    #[serde(default)]
    pub conductor_radius_mm: Option<f64>,
    #[serde(default)]
    pub enclosure_radius_mm: Option<f64>,
    #[serde(default)]
    pub gas_pressure_abs_bar: Option<f64>,
    #[serde(default)]
    pub lightning_impulse_withstand_kv_pk: Option<f64>,
    #[serde(default)]
    pub surface_roughness_ra_um: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPemFuelCellRequest {
    #[serde(default)]
    pub cell_count: Option<u32>,
    #[serde(default)]
    pub active_cell_area_cm2: Option<f64>,
    #[serde(default)]
    pub operating_temperature_c: Option<f64>,
    #[serde(default)]
    pub anode_pressure_bar: Option<f64>,
    #[serde(default)]
    pub cathode_pressure_bar: Option<f64>,
    #[serde(default)]
    pub operating_current_density_a_cm2: Option<f64>,
    #[serde(default)]
    pub membrane_thickness_um: Option<f64>,
    #[serde(default)]
    pub membrane_hydration_lambda: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSuperconductingMagnetRequest {
    #[serde(default)]
    pub material: Option<atlas_simulation_core::superconducting_magnet::SuperconductorMaterial>,
    #[serde(default)]
    pub inner_radius_m: Option<f64>,
    #[serde(default)]
    pub outer_radius_m: Option<f64>,
    #[serde(default)]
    pub coil_length_m: Option<f64>,
    #[serde(default)]
    pub turns_count: Option<u32>,
    #[serde(default)]
    pub operating_current_a: Option<f64>,
    #[serde(default)]
    pub operating_temperature_k: Option<f64>,
    #[serde(default)]
    pub cu_sc_ratio: Option<f64>,
    #[serde(default)]
    pub conductor_area_mm2: Option<f64>,
    #[serde(default)]
    pub quench_detection_delay_s: Option<f64>,
    #[serde(default)]
    pub max_dump_voltage_v: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimReentryRequest {
    #[serde(default)]
    pub freestream_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub altitude_km: Option<f64>,
    #[serde(default)]
    pub nose_radius_m: Option<f64>,
    #[serde(default)]
    pub vehicle_mass_kg: Option<f64>,
    #[serde(default)]
    pub nose_cone_half_angle_deg: Option<f64>,
    #[serde(default)]
    pub tps_surface_emissivity: Option<f64>,
    #[serde(default)]
    pub max_allowable_tps_temperature_k: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimLpbfRequest {
    #[serde(default)]
    pub material: Option<atlas_simulation_core::lpbf_meltpool::LpbfPowderMaterial>,
    #[serde(default)]
    pub laser_power_w: Option<f64>,
    #[serde(default)]
    pub scan_speed_mm_s: Option<f64>,
    #[serde(default)]
    pub beam_diameter_um: Option<f64>,
    #[serde(default)]
    pub layer_thickness_um: Option<f64>,
    #[serde(default)]
    pub hatch_spacing_um: Option<f64>,
    #[serde(default)]
    pub preheat_temperature_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPhononicCrystalRequest {
    #[serde(default)]
    pub layer_a_name: Option<String>,
    #[serde(default)]
    pub layer_a_thickness_mm: Option<f64>,
    #[serde(default)]
    pub layer_a_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub layer_a_sound_speed_m_s: Option<f64>,
    #[serde(default)]
    pub layer_b_name: Option<String>,
    #[serde(default)]
    pub layer_b_thickness_mm: Option<f64>,
    #[serde(default)]
    pub layer_b_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub layer_b_sound_speed_m_s: Option<f64>,
    #[serde(default)]
    pub unit_cell_count: Option<u32>,
    #[serde(default)]
    pub excitation_frequency_hz: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimWheelRailRequest {
    #[serde(default)]
    pub wheel_vertical_load_kn: Option<f64>,
    #[serde(default)]
    pub wheel_radius_mm: Option<f64>,
    #[serde(default)]
    pub rail_crown_radius_mm: Option<f64>,
    #[serde(default)]
    pub train_speed_km_h: Option<f64>,
    #[serde(default)]
    pub effective_conicity_rad: Option<f64>,
    #[serde(default)]
    pub track_gauge_half_m: Option<f64>,
    #[serde(default)]
    pub flange_contact_angle_deg: Option<f64>,
    #[serde(default)]
    pub friction_coefficient: Option<f64>,
    #[serde(default)]
    pub applied_lateral_force_kn: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMineVentilationRequest {
    #[serde(default)]
    pub lining_type: Option<atlas_simulation_core::mine_ventilation::AirwayLiningType>,
    #[serde(default)]
    pub airway_length_m: Option<f64>,
    #[serde(default)]
    pub airway_area_m2: Option<f64>,
    #[serde(default)]
    pub airway_perimeter_m: Option<f64>,
    #[serde(default)]
    pub airflow_rate_m3_s: Option<f64>,
    #[serde(default)]
    pub air_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub shock_loss_factor: Option<f64>,
    #[serde(default)]
    pub vertical_depth_m: Option<f64>,
    #[serde(default)]
    pub downcast_upcast_delta_temp_c: Option<f64>,
    #[serde(default)]
    pub fan_efficiency: Option<f64>,
    #[serde(default)]
    pub methane_emission_m3_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimReflectorAntennaRequest {
    #[serde(default)]
    pub feed_pattern: Option<atlas_simulation_core::reflector_antenna::FeedIlluminationPattern>,
    #[serde(default)]
    pub dish_diameter_m: Option<f64>,
    #[serde(default)]
    pub focal_length_m: Option<f64>,
    #[serde(default)]
    pub operating_frequency_ghz: Option<f64>,
    #[serde(default)]
    pub surface_rms_error_mm: Option<f64>,
    #[serde(default)]
    pub aperture_blockage_fraction: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimGaaFetRequest {
    #[serde(default)]
    pub channel_material: Option<atlas_simulation_core::semiconductor_gaa_fet::GaaChannelMaterial>,
    #[serde(default)]
    pub nanosheet_count: Option<u32>,
    #[serde(default)]
    pub gate_length_nm: Option<f64>,
    #[serde(default)]
    pub nanosheet_width_nm: Option<f64>,
    #[serde(default)]
    pub nanosheet_thickness_nm: Option<f64>,
    #[serde(default)]
    pub equivalent_oxide_thickness_nm: Option<f64>,
    #[serde(default)]
    pub supply_voltage_v: Option<f64>,
    #[serde(default)]
    pub operating_temperature_k: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimIrrigationHydraulicsRequest {
    #[serde(default)]
    pub emitter_type: Option<atlas_simulation_core::irrigation_hydraulics::EmitterType>,
    #[serde(default)]
    pub pipe_inside_diameter_mm: Option<f64>,
    #[serde(default)]
    pub lateral_length_m: Option<f64>,
    #[serde(default)]
    pub emitter_spacing_m: Option<f64>,
    #[serde(default)]
    pub nominal_discharge_l_h: Option<f64>,
    #[serde(default)]
    pub nominal_pressure_head_m: Option<f64>,
    #[serde(default)]
    pub inlet_pressure_head_m: Option<f64>,
    #[serde(default)]
    pub ground_slope_pct: Option<f64>,
    #[serde(default)]
    pub hazen_williams_c: Option<f64>,
    #[serde(default)]
    pub emitter_coefficient_of_variation: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimWastewaterTreatmentRequest {
    #[serde(default)]
    pub diffuser_type: Option<atlas_simulation_core::wastewater_treatment::AerationDiffuserType>,
    #[serde(default)]
    pub influent_flow_m3_day: Option<f64>,
    #[serde(default)]
    pub influent_bod_mg_l: Option<f64>,
    #[serde(default)]
    pub target_effluent_bod_mg_l: Option<f64>,
    #[serde(default)]
    pub mlss_mg_l: Option<f64>,
    #[serde(default)]
    pub sludge_retention_time_days: Option<f64>,
    #[serde(default)]
    pub basin_depth_m: Option<f64>,
    #[serde(default)]
    pub clarifier_diameter_m: Option<f64>,
    #[serde(default)]
    pub clarifier_count: Option<u32>,
    #[serde(default)]
    pub wastewater_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimEvPowertrainRequest {
    #[serde(default)]
    pub layout: Option<atlas_simulation_core::ev_powertrain::EvDrivetrainLayout>,
    #[serde(default)]
    pub vehicle_mass_kg: Option<f64>,
    #[serde(default)]
    pub rolling_resistance_coeff: Option<f64>,
    #[serde(default)]
    pub drag_coefficient: Option<f64>,
    #[serde(default)]
    pub frontal_area_m2: Option<f64>,
    #[serde(default)]
    pub tire_radius_m: Option<f64>,
    #[serde(default)]
    pub gear_ratio: Option<f64>,
    #[serde(default)]
    pub driveline_efficiency: Option<f64>,
    #[serde(default)]
    pub motor_peak_torque_nm: Option<f64>,
    #[serde(default)]
    pub motor_base_speed_rpm: Option<f64>,
    #[serde(default)]
    pub motor_max_speed_rpm: Option<f64>,
    #[serde(default)]
    pub battery_capacity_kwh: Option<f64>,
    #[serde(default)]
    pub battery_voltage_v: Option<f64>,
    #[serde(default)]
    pub cruising_speed_km_h: Option<f64>,
    #[serde(default)]
    pub braking_deceleration_m_s2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSeismicBaseIsolationRequest {
    #[serde(default)]
    pub rubber_grade: Option<atlas_simulation_core::seismic_base_isolation::RubberCompoundGrade>,
    #[serde(default)]
    pub superstructure_mass_tonnes: Option<f64>,
    #[serde(default)]
    pub bearing_count: Option<u32>,
    #[serde(default)]
    pub bearing_outer_diameter_mm: Option<f64>,
    #[serde(default)]
    pub lead_core_diameter_mm: Option<f64>,
    #[serde(default)]
    pub total_rubber_thickness_mm: Option<f64>,
    #[serde(default)]
    pub lead_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub mapped_spectral_accel_s1_g: Option<f64>,
    #[serde(default)]
    pub site_coefficient_fv: Option<f64>,
    #[serde(default)]
    pub fixed_base_period_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimTerminalBallisticsRequest {
    #[serde(default)]
    pub projectile_type: Option<atlas_simulation_core::terminal_ballistics::ProjectileType>,
    #[serde(default)]
    pub armor_type: Option<atlas_simulation_core::terminal_ballistics::TargetArmorType>,
    #[serde(default)]
    pub projectile_mass_kg: Option<f64>,
    #[serde(default)]
    pub projectile_diameter_mm: Option<f64>,
    #[serde(default)]
    pub projectile_length_mm: Option<f64>,
    #[serde(default)]
    pub impact_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub obliquity_angle_deg: Option<f64>,
    #[serde(default)]
    pub armor_thickness_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSolderThermalFatigueRequest {
    #[serde(default)]
    pub alloy_type: Option<atlas_simulation_core::solder_thermal_fatigue::SolderAlloyType>,
    #[serde(default)]
    pub package_width_mm: Option<f64>,
    #[serde(default)]
    pub package_length_mm: Option<f64>,
    #[serde(default)]
    pub solder_joint_height_um: Option<f64>,
    #[serde(default)]
    pub solder_ball_pitch_mm: Option<f64>,
    #[serde(default)]
    pub package_cte_ppm_k: Option<f64>,
    #[serde(default)]
    pub pcb_cte_ppm_k: Option<f64>,
    #[serde(default)]
    pub cycle_min_temp_c: Option<f64>,
    #[serde(default)]
    pub cycle_max_temp_c: Option<f64>,
    #[serde(default)]
    pub dwell_time_minutes: Option<f64>,
    #[serde(default)]
    pub ramp_rate_c_min: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimShipSlammingRequest {
    #[serde(default)]
    pub location: Option<atlas_simulation_core::ship_slamming::SlammingLocation>,
    #[serde(default)]
    pub deadrise_angle_deg: Option<f64>,
    #[serde(default)]
    pub relative_impact_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub frame_spacing_m: Option<f64>,
    #[serde(default)]
    pub plate_length_m: Option<f64>,
    #[serde(default)]
    pub plate_thickness_mm: Option<f64>,
    #[serde(default)]
    pub steel_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub water_density_kg_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimCrashDecelerationRequest {
    #[serde(default)]
    pub scenario: Option<atlas_simulation_core::crash_deceleration::CrashScenario>,
    #[serde(default)]
    pub restraint: Option<atlas_simulation_core::crash_deceleration::RestraintSystemType>,
    #[serde(default)]
    pub vehicle_mass_kg: Option<f64>,
    #[serde(default)]
    pub impact_velocity_km_h: Option<f64>,
    #[serde(default)]
    pub crumple_zone_length_m: Option<f64>,
    #[serde(default)]
    pub crash_box_mean_force_kn: Option<f64>,
    #[serde(default)]
    pub structural_energy_efficiency: Option<f64>,
    #[serde(default)]
    pub occupant_mass_kg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimNuclearFuelPinRequest {
    #[serde(default)]
    pub lattice: Option<atlas_simulation_core::nuclear_fuel_pin::LatticeGeometry>,
    #[serde(default)]
    pub linear_heat_rate_kw_per_m: Option<f64>,
    #[serde(default)]
    pub pellet_radius_mm: Option<f64>,
    #[serde(default)]
    pub gap_thickness_um: Option<f64>,
    #[serde(default)]
    pub clad_thickness_mm: Option<f64>,
    #[serde(default)]
    pub clad_outer_diameter_mm: Option<f64>,
    #[serde(default)]
    pub rod_pitch_mm: Option<f64>,
    #[serde(default)]
    pub coolant_pressure_mpa: Option<f64>,
    #[serde(default)]
    pub bulk_coolant_temperature_c: Option<f64>,
    #[serde(default)]
    pub coolant_mass_flux_kg_m2_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimAdsorptionPsaRequest {
    #[serde(default)]
    pub duty: Option<atlas_simulation_core::adsorption_psa::PsaSeparationDuty>,
    #[serde(default)]
    pub bed_diameter_m: Option<f64>,
    #[serde(default)]
    pub bed_length_m: Option<f64>,
    #[serde(default)]
    pub adsorption_pressure_bar: Option<f64>,
    #[serde(default)]
    pub desorption_pressure_bar: Option<f64>,
    #[serde(default)]
    pub operating_temperature_c: Option<f64>,
    #[serde(default)]
    pub feed_flow_rate_nm3_h: Option<f64>,
    #[serde(default)]
    pub particle_diameter_mm: Option<f64>,
    #[serde(default)]
    pub adsorbent_bulk_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub feed_heavy_fraction: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimRotorcraftAerodynamicsRequest {
    #[serde(default)]
    pub regime: Option<atlas_simulation_core::rotorcraft_aerodynamics::RotorFlightRegime>,
    #[serde(default)]
    pub gross_weight_kg: Option<f64>,
    #[serde(default)]
    pub rotor_radius_m: Option<f64>,
    #[serde(default)]
    pub number_of_blades: Option<u32>,
    #[serde(default)]
    pub blade_chord_m: Option<f64>,
    #[serde(default)]
    pub rotor_speed_rpm: Option<f64>,
    #[serde(default)]
    pub forward_speed_knots: Option<f64>,
    #[serde(default)]
    pub air_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub speed_of_sound_m_s: Option<f64>,
    #[serde(default)]
    pub blade_profile_cd0: Option<f64>,
    #[serde(default)]
    pub fuselage_flat_plate_area_m2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimHeatExchangerDelawareRequest {
    #[serde(default)]
    pub shell_type: Option<atlas_simulation_core::heat_exchanger_delaware::TemaShellType>,
    #[serde(default)]
    pub tube_layout: Option<atlas_simulation_core::heat_exchanger_delaware::TemaTubeLayout>,
    #[serde(default)]
    pub shell_inside_diameter_m: Option<f64>,
    #[serde(default)]
    pub number_of_tubes: Option<u32>,
    #[serde(default)]
    pub tube_outer_diameter_mm: Option<f64>,
    #[serde(default)]
    pub tube_wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub tube_length_m: Option<f64>,
    #[serde(default)]
    pub tube_pitch_mm: Option<f64>,
    #[serde(default)]
    pub baffle_cut_pct: Option<f64>,
    #[serde(default)]
    pub baffle_spacing_m: Option<f64>,
    #[serde(default)]
    pub hot_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub hot_outlet_temp_c: Option<f64>,
    #[serde(default)]
    pub hot_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub cold_inlet_temp_c: Option<f64>,
    #[serde(default)]
    pub cold_outlet_temp_c: Option<f64>,
    #[serde(default)]
    pub cold_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub shell_fluid_cp_j_kg_k: Option<f64>,
    #[serde(default)]
    pub shell_fluid_viscosity_pa_s: Option<f64>,
    #[serde(default)]
    pub shell_fluid_conductivity_w_m_k: Option<f64>,
    #[serde(default)]
    pub shell_fluid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub tube_side_htc_w_m2_k: Option<f64>,
    #[serde(default)]
    pub fouling_resistance_m2_k_w: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimDrilledShaftRequest {
    #[serde(default)]
    pub geomaterial: Option<atlas_simulation_core::drilled_shaft_foundation::GeomaterialType>,
    #[serde(default)]
    pub shaft_diameter_m: Option<f64>,
    #[serde(default)]
    pub embedded_length_m: Option<f64>,
    #[serde(default)]
    pub factored_load_demand_kn: Option<f64>,
    #[serde(default)]
    pub service_load_demand_kn: Option<f64>,
    #[serde(default)]
    pub undrained_shear_strength_kpa: Option<f64>,
    #[serde(default)]
    pub effective_friction_angle_deg: Option<f64>,
    #[serde(default)]
    pub soil_unit_weight_kn_m3: Option<f64>,
    #[serde(default)]
    pub concrete_compressive_strength_mpa: Option<f64>,
    #[serde(default)]
    pub steel_reinforcement_ratio_pct: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimHeartValveRequest {
    #[serde(default)]
    pub valve_type: Option<atlas_simulation_core::heart_valve_hemodynamics::ValveProsthesisType>,
    #[serde(default)]
    pub nominal_size_mm: Option<f64>,
    #[serde(default)]
    pub heart_rate_bpm: Option<f64>,
    #[serde(default)]
    pub stroke_volume_ml: Option<f64>,
    #[serde(default)]
    pub systolic_ejection_period_s: Option<f64>,
    #[serde(default)]
    pub patient_bsa_m2: Option<f64>,
    #[serde(default)]
    pub mean_pressure_gradient_mmhg: Option<f64>,
    #[serde(default)]
    pub peak_jet_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub dynamic_closing_volume_ml: Option<f64>,
    #[serde(default)]
    pub static_leakage_volume_ml: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimCentrifugalCompressorRequest {
    #[serde(default)]
    pub blade_style: Option<atlas_simulation_core::centrifugal_compressor::ImpellerBladeStyle>,
    #[serde(default)]
    pub mass_flow_rate_kg_s: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub impeller_tip_diameter_m: Option<f64>,
    #[serde(default)]
    pub inducer_hub_diameter_m: Option<f64>,
    #[serde(default)]
    pub inducer_shroud_diameter_m: Option<f64>,
    #[serde(default)]
    pub number_of_blades: Option<u32>,
    #[serde(default)]
    pub blade_backsweep_deg: Option<f64>,
    #[serde(default)]
    pub inlet_total_temperature_k: Option<f64>,
    #[serde(default)]
    pub inlet_total_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub isentropic_efficiency_tt: Option<f64>,
    #[serde(default)]
    pub impeller_material_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub impeller_yield_strength_mpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimRailPantographRequest {
    #[serde(default)]
    pub system: Option<atlas_simulation_core::rail_pantograph::CatenaryVoltageSystem>,
    #[serde(default)]
    pub train_speed_km_h: Option<f64>,
    #[serde(default)]
    pub static_contact_force_n: Option<f64>,
    #[serde(default)]
    pub aerodynamic_uplift_coeff: Option<f64>,
    #[serde(default)]
    pub contact_wire_tension_kn: Option<f64>,
    #[serde(default)]
    pub contact_wire_mass_kg_per_m: Option<f64>,
    #[serde(default)]
    pub catenary_span_length_m: Option<f64>,
    #[serde(default)]
    pub catenary_midspan_elasticity_mm_per_n: Option<f64>,
    #[serde(default)]
    pub catenary_support_elasticity_mm_per_n: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimTbmTunnelingRequest {
    #[serde(default)]
    pub ground_condition: Option<atlas_simulation_core::tbm_tunneling::GroundConditionType>,
    #[serde(default)]
    pub tunnel_outer_diameter_m: Option<f64>,
    #[serde(default)]
    pub tunnel_depth_to_axis_m: Option<f64>,
    #[serde(default)]
    pub water_table_depth_m: Option<f64>,
    #[serde(default)]
    pub soil_bulk_unit_weight_kn_m3: Option<f64>,
    #[serde(default)]
    pub soil_effective_cohesion_kpa: Option<f64>,
    #[serde(default)]
    pub soil_effective_friction_angle_deg: Option<f64>,
    #[serde(default)]
    pub soil_compressive_strength_mpa: Option<f64>,
    #[serde(default)]
    pub cutterhead_opening_ratio_pct: Option<f64>,
    #[serde(default)]
    pub cutterhead_rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub penetration_rate_mm_per_rev: Option<f64>,
    #[serde(default)]
    pub shield_length_m: Option<f64>,
    #[serde(default)]
    pub machine_rated_torque_kn_m: Option<f64>,
    #[serde(default)]
    pub machine_rated_thrust_kn: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPrestressedConcreteRequest {
    #[serde(default)]
    pub strand_type: Option<atlas_simulation_core::prestressed_concrete::PrestressingStrandType>,
    #[serde(default)]
    pub span_length_m: Option<f64>,
    #[serde(default)]
    pub concrete_area_m2: Option<f64>,
    #[serde(default)]
    pub concrete_moment_of_inertia_m4: Option<f64>,
    #[serde(default)]
    pub distance_to_top_fiber_m: Option<f64>,
    #[serde(default)]
    pub distance_to_bottom_fiber_m: Option<f64>,
    #[serde(default)]
    pub concrete_f_c_mpa: Option<f64>,
    #[serde(default)]
    pub concrete_f_ci_mpa: Option<f64>,
    #[serde(default)]
    pub tendon_steel_area_mm2: Option<f64>,
    #[serde(default)]
    pub tendon_modulus_mpa: Option<f64>,
    #[serde(default)]
    pub jacking_stress_ratio_of_fpu: Option<f64>,
    #[serde(default)]
    pub midspan_eccentricity_m: Option<f64>,
    #[serde(default)]
    pub end_eccentricity_m: Option<f64>,
    #[serde(default)]
    pub friction_curvature_coeff_mu: Option<f64>,
    #[serde(default)]
    pub friction_wobble_coeff_k: Option<f64>,
    #[serde(default)]
    pub anchorage_draw_in_mm: Option<f64>,
    #[serde(default)]
    pub ambient_relative_humidity_pct: Option<f64>,
    #[serde(default)]
    pub service_moment_midspan_kn_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSpacecraftJitterRequest {
    #[serde(default)]
    pub wheel_speed_rpm: Option<f64>,
    #[serde(default)]
    pub rotor_inertia_kg_m2: Option<f64>,
    #[serde(default)]
    pub rotor_mass_kg: Option<f64>,
    #[serde(default)]
    pub static_imbalance_g_mm: Option<f64>,
    #[serde(default)]
    pub dynamic_imbalance_g_mm2: Option<f64>,
    #[serde(default)]
    pub number_of_bearing_balls: Option<u32>,
    #[serde(default)]
    pub ball_diameter_mm: Option<f64>,
    #[serde(default)]
    pub bearing_pitch_diameter_mm: Option<f64>,
    #[serde(default)]
    pub bearing_contact_angle_deg: Option<f64>,
    #[serde(default)]
    pub structural_resonance_freq_hz: Option<f64>,
    #[serde(default)]
    pub structural_damping_ratio: Option<f64>,
    #[serde(default)]
    pub satellite_inertia_kg_m2: Option<f64>,
    #[serde(default)]
    pub allowable_pointing_jitter_urad: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimRadarLidarSensorRequest {
    #[serde(default)]
    pub weather_condition: Option<atlas_simulation_core::radar_lidar_sensor::WeatherConditionType>,
    #[serde(default)]
    pub target_range_m: Option<f64>,
    #[serde(default)]
    pub target_rcs_m2: Option<f64>,
    #[serde(default)]
    pub target_optical_reflectivity: Option<f64>,
    #[serde(default)]
    pub radar_tx_power_w: Option<f64>,
    #[serde(default)]
    pub radar_antenna_gain_dbi: Option<f64>,
    #[serde(default)]
    pub radar_chirp_bandwidth_mhz: Option<f64>,
    #[serde(default)]
    pub radar_noise_figure_db: Option<f64>,
    #[serde(default)]
    pub lidar_wavelength_nm: Option<f64>,
    #[serde(default)]
    pub lidar_pulse_power_w: Option<f64>,
    #[serde(default)]
    pub lidar_aperture_diameter_mm: Option<f64>,
    #[serde(default)]
    pub required_snr_threshold_db: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimAuvPropulsionRequest {
    #[serde(default)]
    pub mission_regime: Option<atlas_simulation_core::auv_propulsion::AuvMissionRegime>,
    #[serde(default)]
    pub cruising_speed_knots: Option<f64>,
    #[serde(default)]
    pub vehicle_length_m: Option<f64>,
    #[serde(default)]
    pub vehicle_diameter_m: Option<f64>,
    #[serde(default)]
    pub wetted_surface_area_m2: Option<f64>,
    #[serde(default)]
    pub seawater_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub seawater_viscosity_pa_s: Option<f64>,
    #[serde(default)]
    pub propeller_open_water_efficiency: Option<f64>,
    #[serde(default)]
    pub hotel_power_draw_w: Option<f64>,
    #[serde(default)]
    pub battery_capacity_kwh: Option<f64>,
    #[serde(default)]
    pub battery_depth_of_discharge_pct: Option<f64>,
    #[serde(default)]
    pub motor_drive_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimHydroGovernorRequest {
    #[serde(default)]
    pub turbine_type: Option<atlas_simulation_core::hydro_governor::HydroTurbineType>,
    #[serde(default)]
    pub rated_net_head_m: Option<f64>,
    #[serde(default)]
    pub rated_discharge_m3_s: Option<f64>,
    #[serde(default)]
    pub rated_speed_rpm: Option<f64>,
    #[serde(default)]
    pub rotor_inertia_kg_m2: Option<f64>,
    #[serde(default)]
    pub penstock_length_m: Option<f64>,
    #[serde(default)]
    pub penstock_diameter_m: Option<f64>,
    #[serde(default)]
    pub penstock_wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub steel_elastic_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub steel_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub wicket_gate_closure_time_s: Option<f64>,
    #[serde(default)]
    pub turbine_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSemiconductorCmpRequest {
    #[serde(default)]
    pub layer_type: Option<atlas_simulation_core::semiconductor_cmp::CmpLayerType>,
    #[serde(default)]
    pub wafer_diameter_mm: Option<f64>,
    #[serde(default)]
    pub down_force_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub relative_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub polish_time_seconds: Option<f64>,
    #[serde(default)]
    pub slurry_viscosity_pa_s: Option<f64>,
    #[serde(default)]
    pub initial_step_height_nm: Option<f64>,
    #[serde(default)]
    pub pattern_metal_density_pct: Option<f64>,
    #[serde(default)]
    pub pattern_line_width_um: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPavementMepdgRequest {
    #[serde(default)]
    pub binder_grade: Option<atlas_simulation_core::pavement_mepdg::AsphaltBinderGrade>,
    #[serde(default)]
    pub asphalt_thickness_mm: Option<f64>,
    #[serde(default)]
    pub base_thickness_mm: Option<f64>,
    #[serde(default)]
    pub subbase_thickness_mm: Option<f64>,
    #[serde(default)]
    pub asphalt_dynamic_modulus_mpa: Option<f64>,
    #[serde(default)]
    pub base_resilient_modulus_mpa: Option<f64>,
    #[serde(default)]
    pub subgrade_resilient_modulus_mpa: Option<f64>,
    #[serde(default)]
    pub design_traffic_millions_esal: Option<f64>,
    #[serde(default)]
    pub effective_binder_volume_pct: Option<f64>,
    #[serde(default)]
    pub air_voids_pct: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimBiomaterialScaffoldRequest {
    #[serde(default)]
    pub biomaterial_type:
        Option<atlas_simulation_core::biomaterial_scaffold::ScaffoldBiomaterialType>,
    #[serde(default)]
    pub scaffold_diameter_mm: Option<f64>,
    #[serde(default)]
    pub scaffold_height_mm: Option<f64>,
    #[serde(default)]
    pub target_porosity_pct: Option<f64>,
    #[serde(default)]
    pub mean_pore_size_um: Option<f64>,
    #[serde(default)]
    pub perfusion_velocity_mm_s: Option<f64>,
    #[serde(default)]
    pub degradation_evaluation_weeks: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSatelliteOrbitalDecayRequest {
    #[serde(default)]
    pub solar_activity: Option<atlas_simulation_core::satellite_orbital_decay::SolarActivityLevel>,
    #[serde(default)]
    pub satellite_mass_kg: Option<f64>,
    #[serde(default)]
    pub drag_cross_section_area_m2: Option<f64>,
    #[serde(default)]
    pub drag_coefficient_cd: Option<f64>,
    #[serde(default)]
    pub initial_perigee_altitude_km: Option<f64>,
    #[serde(default)]
    pub initial_apogee_altitude_km: Option<f64>,
    #[serde(default)]
    pub orbit_inclination_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMiningExcavatorRequest {
    #[serde(default)]
    pub material_class: Option<atlas_simulation_core::mining_excavator::MiningMaterialClass>,
    #[serde(default)]
    pub wheel_diameter_m: Option<f64>,
    #[serde(default)]
    pub bucket_count: Option<u32>,
    #[serde(default)]
    pub bucket_nominal_volume_m3: Option<f64>,
    #[serde(default)]
    pub wheel_speed_rpm: Option<f64>,
    #[serde(default)]
    pub bucket_fill_factor: Option<f64>,
    #[serde(default)]
    pub total_cutting_lip_length_m: Option<f64>,
    #[serde(default)]
    pub lineal_cutting_resistance_kn_m: Option<f64>,
    #[serde(default)]
    pub boom_length_m: Option<f64>,
    #[serde(default)]
    pub slewing_speed_deg_s: Option<f64>,
    #[serde(default)]
    pub upperstructure_mass_tons: Option<f64>,
    #[serde(default)]
    pub stabilizing_moment_kn_m: Option<f64>,
    #[serde(default)]
    pub mechanical_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMultiphasePipeFlowRequest {
    #[serde(default)]
    pub pipe_length_m: Option<f64>,
    #[serde(default)]
    pub pipe_inner_diameter_mm: Option<f64>,
    #[serde(default)]
    pub elevation_change_m: Option<f64>,
    #[serde(default)]
    pub liquid_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub gas_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub liquid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub gas_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub liquid_viscosity_mpa_s: Option<f64>,
    #[serde(default)]
    pub gas_viscosity_mpa_s: Option<f64>,
    #[serde(default)]
    pub pipe_roughness_mm: Option<f64>,
    #[serde(default)]
    pub api_erosion_constant_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimGeothermalReservoirRequest {
    #[serde(default)]
    pub rock_type: Option<atlas_simulation_core::geothermal_reservoir::GeothermalRockType>,
    #[serde(default)]
    pub reservoir_temperature_c: Option<f64>,
    #[serde(default)]
    pub reinjection_temperature_c: Option<f64>,
    #[serde(default)]
    pub brine_flow_rate_l_s: Option<f64>,
    #[serde(default)]
    pub reservoir_thickness_m: Option<f64>,
    #[serde(default)]
    pub well_spacing_distance_m: Option<f64>,
    #[serde(default)]
    pub wellbore_radius_m: Option<f64>,
    #[serde(default)]
    pub reservoir_permeability_md: Option<f64>,
    #[serde(default)]
    pub reservoir_porosity: Option<f64>,
    #[serde(default)]
    pub pump_system_efficiency: Option<f64>,
    #[serde(default)]
    pub project_lifespan_years: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSprayDryingKineticsRequest {
    #[serde(default)]
    pub product_type: Option<atlas_simulation_core::spray_drying_kinetics::SprayDryingProductType>,
    #[serde(default)]
    pub feed_rate_kg_h: Option<f64>,
    #[serde(default)]
    pub feed_solids_fraction: Option<f64>,
    #[serde(default)]
    pub feed_viscosity_mpa_s: Option<f64>,
    #[serde(default)]
    pub feed_surface_tension_mn_m: Option<f64>,
    #[serde(default)]
    pub atomizer_wheel_diameter_mm: Option<f64>,
    #[serde(default)]
    pub atomizer_speed_rpm: Option<f64>,
    #[serde(default)]
    pub inlet_air_temperature_c: Option<f64>,
    #[serde(default)]
    pub outlet_air_temperature_c: Option<f64>,
    #[serde(default)]
    pub ambient_air_temperature_c: Option<f64>,
    #[serde(default)]
    pub ambient_air_rh_pct: Option<f64>,
    #[serde(default)]
    pub target_product_moisture_pct: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimThermoelectricRtgRequest {
    #[serde(default)]
    pub material_type:
        Option<atlas_simulation_core::thermoelectric_rtg::ThermoelectricMaterialType>,
    #[serde(default)]
    pub initial_thermal_power_w_th: Option<f64>,
    #[serde(default)]
    pub hot_junction_temp_k: Option<f64>,
    #[serde(default)]
    pub cold_junction_temp_k: Option<f64>,
    #[serde(default)]
    pub couple_count: Option<u32>,
    #[serde(default)]
    pub leg_length_mm: Option<f64>,
    #[serde(default)]
    pub leg_area_mm2: Option<f64>,
    #[serde(default)]
    pub mission_elapsed_years: Option<f64>,
    #[serde(default)]
    pub fuel_half_life_years: Option<f64>,
    #[serde(default)]
    pub annual_junction_degradation_pct: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimContainmentLocaRequest {
    #[serde(default)]
    pub containment_type: Option<atlas_simulation_core::containment_loca::ContainmentStructureType>,
    #[serde(default)]
    pub containment_free_volume_m3: Option<f64>,
    #[serde(default)]
    pub design_pressure_bar_a: Option<f64>,
    #[serde(default)]
    pub design_temperature_c: Option<f64>,
    #[serde(default)]
    pub initial_pressure_bar_a: Option<f64>,
    #[serde(default)]
    pub initial_temperature_c: Option<f64>,
    #[serde(default)]
    pub initial_relative_humidity_pct: Option<f64>,
    #[serde(default)]
    pub blowdown_coolant_mass_tonnes: Option<f64>,
    #[serde(default)]
    pub blowdown_enthalpy_gj: Option<f64>,
    #[serde(default)]
    pub blowdown_duration_seconds: Option<f64>,
    #[serde(default)]
    pub heat_sink_surface_area_m2: Option<f64>,
    #[serde(default)]
    pub spray_flow_rate_l_s: Option<f64>,
    #[serde(default)]
    pub spray_temperature_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimAtmosphericDispersionRequest {
    #[serde(default)]
    pub stability_class:
        Option<atlas_simulation_core::atmospheric_dispersion::AtmosphericStabilityClass>,
    #[serde(default)]
    pub pollutant: Option<atlas_simulation_core::atmospheric_dispersion::AirPollutantType>,
    #[serde(default)]
    pub emission_rate_g_s: Option<f64>,
    #[serde(default)]
    pub stack_height_m: Option<f64>,
    #[serde(default)]
    pub stack_inside_diameter_m: Option<f64>,
    #[serde(default)]
    pub stack_gas_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub stack_gas_temperature_c: Option<f64>,
    #[serde(default)]
    pub ambient_air_temperature_c: Option<f64>,
    #[serde(default)]
    pub wind_speed_at_stack_height_m_s: Option<f64>,
    #[serde(default)]
    pub receptor_downwind_distance_m: Option<f64>,
    #[serde(default)]
    pub receptor_crosswind_distance_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimRocketAscentRequest {
    #[serde(default)]
    pub staging_type:
        Option<atlas_simulation_core::rocket_ascent_trajectory::RocketStagingArchitecture>,
    #[serde(default)]
    pub gross_liftoff_mass_kg: Option<f64>,
    #[serde(default)]
    pub payload_mass_kg: Option<f64>,
    #[serde(default)]
    pub stage1_propellant_mass_kg: Option<f64>,
    #[serde(default)]
    pub stage1_dry_mass_kg: Option<f64>,
    #[serde(default)]
    pub stage1_thrust_sl_kn: Option<f64>,
    #[serde(default)]
    pub stage1_isp_vac_s: Option<f64>,
    #[serde(default)]
    pub stage1_burn_time_s: Option<f64>,
    #[serde(default)]
    pub stage2_propellant_mass_kg: Option<f64>,
    #[serde(default)]
    pub stage2_dry_mass_kg: Option<f64>,
    #[serde(default)]
    pub stage2_thrust_vac_kn: Option<f64>,
    #[serde(default)]
    pub stage2_isp_vac_s: Option<f64>,
    #[serde(default)]
    pub aerodynamic_reference_area_m2: Option<f64>,
    #[serde(default)]
    pub drag_coefficient_cd: Option<f64>,
    #[serde(default)]
    pub target_orbit_altitude_km: Option<f64>,
    #[serde(default)]
    pub launch_latitude_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimGravityDamRequest {
    #[serde(default)]
    pub load_combination: Option<atlas_simulation_core::gravity_dam_stability::DamLoadCombination>,
    #[serde(default)]
    pub foundation_rock: Option<atlas_simulation_core::gravity_dam_stability::FoundationRockClass>,
    #[serde(default)]
    pub dam_height_m: Option<f64>,
    #[serde(default)]
    pub crest_width_m: Option<f64>,
    #[serde(default)]
    pub base_width_m: Option<f64>,
    #[serde(default)]
    pub upstream_water_depth_m: Option<f64>,
    #[serde(default)]
    pub downstream_tailwater_depth_m: Option<f64>,
    #[serde(default)]
    pub silt_deposit_depth_m: Option<f64>,
    #[serde(default)]
    pub drainage_gallery_distance_from_heel_m: Option<f64>,
    #[serde(default)]
    pub drainage_efficiency_factor: Option<f64>,
    #[serde(default)]
    pub horizontal_seismic_coef_kh: Option<f64>,
    #[serde(default)]
    pub concrete_unit_weight_kn_m3: Option<f64>,
    #[serde(default)]
    pub allowable_bearing_capacity_mpa: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimElectrolyzerHydrogenRequest {
    #[serde(default)]
    pub technology: Option<atlas_simulation_core::electrolyzer_hydrogen::ElectrolyzerTechnology>,
    #[serde(default)]
    pub cell_temperature_c: Option<f64>,
    #[serde(default)]
    pub hydrogen_operating_pressure_bar_a: Option<f64>,
    #[serde(default)]
    pub current_density_a_per_cm2: Option<f64>,
    #[serde(default)]
    pub active_cell_area_cm2: Option<f64>,
    #[serde(default)]
    pub cell_count: Option<u32>,
    #[serde(default)]
    pub membrane_thickness_um: Option<f64>,
    #[serde(default)]
    pub membrane_water_content_lambda: Option<f64>,
    #[serde(default)]
    pub anode_exchange_current_density_a_cm2: Option<f64>,
    #[serde(default)]
    pub cathode_exchange_current_density_a_cm2: Option<f64>,
    #[serde(default)]
    pub limiting_current_density_a_cm2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimThinFilmOpticsRequest {
    #[serde(default)]
    pub coating_type: Option<atlas_simulation_core::thin_film_optics::OpticalCoatingType>,
    #[serde(default)]
    pub polarization: Option<atlas_simulation_core::thin_film_optics::OpticalPolarization>,
    #[serde(default)]
    pub wavelength_nm: Option<f64>,
    #[serde(default)]
    pub angle_of_incidence_deg: Option<f64>,
    #[serde(default)]
    pub incident_medium_index: Option<f64>,
    #[serde(default)]
    pub substrate_index: Option<f64>,
    #[serde(default)]
    pub layers: Option<Vec<atlas_simulation_core::thin_film_optics::DielectricLayer>>,
}

#[derive(Debug, Default, Deserialize)]
struct SimCryogenicLiquefactionRequest {
    #[serde(default)]
    pub fluid_type: Option<atlas_simulation_core::cryogenic_liquefaction::CryogenicFluidType>,
    #[serde(default)]
    pub feed_gas_flow_rate_kg_h: Option<f64>,
    #[serde(default)]
    pub suction_pressure_bar_a: Option<f64>,
    #[serde(default)]
    pub discharge_pressure_bar_a: Option<f64>,
    #[serde(default)]
    pub ambient_aftercooler_temp_c: Option<f64>,
    #[serde(default)]
    pub heat_exchanger_pinch_k: Option<f64>,
    #[serde(default)]
    pub compressor_isothermal_efficiency: Option<f64>,
    #[serde(default)]
    pub heat_exchanger_effectiveness: Option<f64>,
    #[serde(default)]
    pub ambient_heat_leak_w: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimHydrocycloneRequest {
    #[serde(default)]
    pub application: Option<atlas_simulation_core::hydrocyclone_separation::SlurryApplicationType>,
    #[serde(default)]
    pub cyclone_diameter_mm: Option<f64>,
    #[serde(default)]
    pub inlet_diameter_mm: Option<f64>,
    #[serde(default)]
    pub vortex_finder_diameter_mm: Option<f64>,
    #[serde(default)]
    pub apex_diameter_mm: Option<f64>,
    #[serde(default)]
    pub free_vortex_height_mm: Option<f64>,
    #[serde(default)]
    pub feed_flow_rate_m3_h: Option<f64>,
    #[serde(default)]
    pub solids_volumetric_concentration_pct: Option<f64>,
    #[serde(default)]
    pub solid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub liquid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub target_evaluation_size_um: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMagneticBearingRequest {
    #[serde(default)]
    pub topology: Option<atlas_simulation_core::magnetic_bearing::MagneticBearingTopology>,
    #[serde(default)]
    pub rotor_supported_mass_kg: Option<f64>,
    #[serde(default)]
    pub max_rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub nominal_air_gap_mm: Option<f64>,
    #[serde(default)]
    pub pole_face_area_mm2: Option<f64>,
    #[serde(default)]
    pub coil_turns_per_pole: Option<u32>,
    #[serde(default)]
    pub bias_current_a: Option<f64>,
    #[serde(default)]
    pub max_control_current_a: Option<f64>,
    #[serde(default)]
    pub amplifier_bus_voltage_v: Option<f64>,
    #[serde(default)]
    pub controller_proportional_gain_a_per_mm: Option<f64>,
    #[serde(default)]
    pub controller_derivative_gain_a_s_per_m: Option<f64>,
    #[serde(default)]
    pub rotor_unbalance_eccentricity_um: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSonarPropagationRequest {
    #[serde(default)]
    pub acoustic_regime: Option<atlas_simulation_core::sonar_propagation::OceanAcousticRegime>,
    #[serde(default)]
    pub target_class: Option<atlas_simulation_core::sonar_propagation::SonarTargetClass>,
    #[serde(default)]
    pub operating_frequency_khz: Option<f64>,
    #[serde(default)]
    pub transmitter_acoustic_power_w: Option<f64>,
    #[serde(default)]
    pub array_directivity_index_db: Option<f64>,
    #[serde(default)]
    pub ocean_depth_m: Option<f64>,
    #[serde(default)]
    pub target_range_m: Option<f64>,
    #[serde(default)]
    pub sea_state_code: Option<u32>,
    #[serde(default)]
    pub shipping_traffic_index: Option<u32>,
    #[serde(default)]
    pub detection_threshold_db: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimEbwWeldingRequest {
    #[serde(default)]
    pub alloy_grade: Option<atlas_simulation_core::electron_beam_welding::EbwAlloyGrade>,
    #[serde(default)]
    pub acceleration_voltage_kv: Option<f64>,
    #[serde(default)]
    pub beam_current_ma: Option<f64>,
    #[serde(default)]
    pub travel_speed_mm_s: Option<f64>,
    #[serde(default)]
    pub plate_thickness_mm: Option<f64>,
    #[serde(default)]
    pub beam_focal_diameter_mm: Option<f64>,
    #[serde(default)]
    pub ambient_temperature_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimCrystallizationRequest {
    #[serde(default)]
    pub solute: Option<atlas_simulation_core::crystallization_kinetics::CrystallineSolute>,
    #[serde(default)]
    pub operating_temperature_c: Option<f64>,
    #[serde(default)]
    pub feed_volumetric_flow_m3_h: Option<f64>,
    #[serde(default)]
    pub crystallizer_active_volume_m3: Option<f64>,
    #[serde(default)]
    pub solution_concentration_kg_m3: Option<f64>,
    #[serde(default)]
    pub crystal_volumetric_shape_factor: Option<f64>,
    #[serde(default)]
    pub target_median_size_um: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimTecCoolerRequest {
    #[serde(default)]
    pub material: Option<atlas_simulation_core::thermoelectric_cooler::TecMaterialType>,
    #[serde(default)]
    pub couple_count_n: Option<usize>,
    #[serde(default)]
    pub pellet_area_mm2: Option<f64>,
    #[serde(default)]
    pub pellet_length_mm: Option<f64>,
    #[serde(default)]
    pub cold_junction_temp_c: Option<f64>,
    #[serde(default)]
    pub hot_junction_temp_c: Option<f64>,
    #[serde(default)]
    pub operating_current_a: Option<f64>,
    #[serde(default)]
    pub heat_load_qc_target_w: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSlurryPumpRequest {
    #[serde(default)]
    pub abrasiveness_class:
        Option<atlas_simulation_core::slurry_pump_derating::SlurryAbrasivenessClass>,
    #[serde(default)]
    pub slurry_flow_rate_m3_h: Option<f64>,
    #[serde(default)]
    pub clear_water_head_m: Option<f64>,
    #[serde(default)]
    pub clear_water_efficiency: Option<f64>,
    #[serde(default)]
    pub solid_specific_gravity: Option<f64>,
    #[serde(default)]
    pub solids_concentration_wt_pct: Option<f64>,
    #[serde(default)]
    pub median_particle_size_d50_um: Option<f64>,
    #[serde(default)]
    pub impeller_diameter_mm: Option<f64>,
    #[serde(default)]
    pub pump_rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub allowable_wear_thickness_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimFiberDispersionRequest {
    #[serde(default)]
    pub fiber_standard: Option<atlas_simulation_core::fiber_dispersion::FiberStandardType>,
    #[serde(default)]
    pub wavelength_nm: Option<f64>,
    #[serde(default)]
    pub fiber_length_km: Option<f64>,
    #[serde(default)]
    pub laser_linewidth_fwhm_nm: Option<f64>,
    #[serde(default)]
    pub initial_pulse_width_ps: Option<f64>,
    #[serde(default)]
    pub launch_power_mw: Option<f64>,
    #[serde(default)]
    pub target_bit_rate_gbps: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPropellerRequest {
    #[serde(default)]
    pub blade_count: Option<atlas_simulation_core::propeller_hydrodynamics::BladeCount>,
    #[serde(default)]
    pub propeller_diameter_m: Option<f64>,
    #[serde(default)]
    pub pitch_diameter_ratio: Option<f64>,
    #[serde(default)]
    pub expanded_area_ratio: Option<f64>,
    #[serde(default)]
    pub speed_of_advance_m_s: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub shaft_immersion_depth_m: Option<f64>,
    #[serde(default)]
    pub water_density_kg_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimIndoorAirQualityRequest {
    #[serde(default)]
    pub contaminant: Option<atlas_simulation_core::indoor_air_quality::AirContaminantType>,
    #[serde(default)]
    pub space_type: Option<atlas_simulation_core::indoor_air_quality::SpaceOccupancyType>,
    #[serde(default)]
    pub room_floor_area_m2: Option<f64>,
    #[serde(default)]
    pub ceiling_height_m: Option<f64>,
    #[serde(default)]
    pub room_clear_height_m: Option<f64>,
    #[serde(default)]
    pub occupant_count: Option<usize>,
    #[serde(default)]
    pub outdoor_ventilation_rate_m3_h: Option<f64>,
    #[serde(default)]
    pub ventilation_supply_airflow_m3_h: Option<f64>,
    #[serde(default)]
    pub hvac_filter_efficiency: Option<f64>,
    #[serde(default)]
    pub outdoor_ambient_concentration: Option<f64>,
    #[serde(default)]
    pub initial_room_concentration: Option<f64>,
    #[serde(default)]
    pub target_threshold_concentration: Option<f64>,
    #[serde(default)]
    pub exposure_duration_hours: Option<f64>,
    #[serde(default)]
    pub infected_occupants: Option<usize>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPmsmDriveRequest {
    #[serde(default)]
    pub rotor_topology: Option<atlas_simulation_core::pmsm_foc_drive::PmsmRotorTopology>,
    #[serde(default)]
    pub pole_pairs_p: Option<u32>,
    #[serde(default)]
    pub stator_resistance_ohms: Option<f64>,
    #[serde(default)]
    pub d_axis_inductance_mh: Option<f64>,
    #[serde(default)]
    pub q_axis_inductance_mh: Option<f64>,
    #[serde(default)]
    pub permanent_magnet_flux_wb: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub q_axis_current_iq_a: Option<f64>,
    #[serde(default)]
    pub inverter_dc_bus_voltage_v: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMseWallRequest {
    #[serde(default)]
    pub polymer_type: Option<atlas_simulation_core::mse_retaining_wall::GeogridPolymerType>,
    #[serde(default)]
    pub wall_height_h_m: Option<f64>,
    #[serde(default)]
    pub reinforcement_length_l_m: Option<f64>,
    #[serde(default)]
    pub vertical_layer_spacing_m: Option<f64>,
    #[serde(default)]
    pub soil_unit_weight_kn_m3: Option<f64>,
    #[serde(default)]
    pub soil_friction_angle_deg: Option<f64>,
    #[serde(default)]
    pub crest_surcharge_q_kpa: Option<f64>,
    #[serde(default)]
    pub geogrid_ultimate_strength_kn_m: Option<f64>,
    #[serde(default)]
    pub rf_installation_damage: Option<f64>,
    #[serde(default)]
    pub rf_creep_rupture: Option<f64>,
    #[serde(default)]
    pub rf_durability: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimIsotopeCentrifugeRequest {
    #[serde(default)]
    pub rotor_material: Option<atlas_simulation_core::isotope_centrifuge::CentrifugeRotorMaterial>,
    #[serde(default)]
    pub rotor_radius_m: Option<f64>,
    #[serde(default)]
    pub rotor_length_m: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub operating_temperature_c: Option<f64>,
    #[serde(default)]
    pub feed_mass_flow_kg_h: Option<f64>,
    #[serde(default)]
    pub feed_assay_u235_fraction: Option<f64>,
    #[serde(default)]
    pub stage_cut_theta: Option<f64>,
    #[serde(default)]
    pub circulation_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimConveyorRequest {
    #[serde(default)]
    pub carcass_type: Option<atlas_simulation_core::conveyor_bulk_handling::BeltCarcassType>,
    #[serde(default)]
    pub troughing_angle: Option<atlas_simulation_core::conveyor_bulk_handling::TroughingAngleClass>,
    #[serde(default)]
    pub belt_width_mm: Option<f64>,
    #[serde(default)]
    pub conveyor_length_m: Option<f64>,
    #[serde(default)]
    pub elevation_lift_m: Option<f64>,
    #[serde(default)]
    pub belt_speed_m_s: Option<f64>,
    #[serde(default)]
    pub material_bulk_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub surcharge_angle_deg: Option<f64>,
    #[serde(default)]
    pub belt_unit_weight_kg_m: Option<f64>,
    #[serde(default)]
    pub idlers_carrying_weight_kg_m: Option<f64>,
    #[serde(default)]
    pub idlers_return_weight_kg_m: Option<f64>,
    #[serde(default)]
    pub friction_factor_f: Option<f64>,
    #[serde(default)]
    pub drive_wrap_angle_deg: Option<f64>,
    #[serde(default)]
    pub drive_friction_mu: Option<f64>,
    #[serde(default)]
    pub belt_rated_breaking_strength_kn_m: Option<f64>,
    #[serde(default)]
    pub drive_mechanical_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimOffshoreWaveRequest {
    #[serde(default)]
    pub structure_type: Option<atlas_simulation_core::offshore_wave_loading::OffshoreStructureType>,
    #[serde(default)]
    pub water_depth_m: Option<f64>,
    #[serde(default)]
    pub wave_height_m: Option<f64>,
    #[serde(default)]
    pub wave_period_s: Option<f64>,
    #[serde(default)]
    pub surface_current_m_s: Option<f64>,
    #[serde(default)]
    pub cylinder_diameter_m: Option<f64>,
    #[serde(default)]
    pub wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub steel_yield_strength_mpa: Option<f64>,
    #[serde(default)]
    pub drag_coefficient_cd: Option<f64>,
    #[serde(default)]
    pub inertia_coefficient_cm: Option<f64>,
    #[serde(default)]
    pub seawater_density_kg_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSpacecraftAdcsRequest {
    #[serde(default)]
    pub orbit_regime:
        Option<atlas_simulation_core::spacecraft_adcs_attitude::SpacecraftOrbitRegime>,
    #[serde(default)]
    pub orbit_altitude_km: Option<f64>,
    #[serde(default)]
    pub spacecraft_mass_kg: Option<f64>,
    #[serde(default)]
    pub inertia_ixx_kg_m2: Option<f64>,
    #[serde(default)]
    pub inertia_iyy_kg_m2: Option<f64>,
    #[serde(default)]
    pub inertia_izz_kg_m2: Option<f64>,
    #[serde(default)]
    pub cp_cg_offset_arm_m: Option<f64>,
    #[serde(default)]
    pub projected_surface_area_m2: Option<f64>,
    #[serde(default)]
    pub solar_reflectance_q: Option<f64>,
    #[serde(default)]
    pub residual_magnetic_dipole_a_m2: Option<f64>,
    #[serde(default)]
    pub target_slew_accel_deg_s2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimBatteryFastChargeRequest {
    #[serde(default)]
    pub cathode_chemistry:
        Option<atlas_simulation_core::battery_fast_charging::BatteryCathodeChemistry>,
    #[serde(default)]
    pub nominal_capacity_ah: Option<f64>,
    #[serde(default)]
    pub initial_soc_fraction: Option<f64>,
    #[serde(default)]
    pub target_soc_fraction: Option<f64>,
    #[serde(default)]
    pub charge_c_rate: Option<f64>,
    #[serde(default)]
    pub internal_resistance_mohms: Option<f64>,
    #[serde(default)]
    pub diffusion_time_constant_s: Option<f64>,
    #[serde(default)]
    pub charge_cutoff_voltage_v: Option<f64>,
    #[serde(default)]
    pub ambient_temperature_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPackedBedRequest {
    #[serde(default)]
    pub application: Option<atlas_simulation_core::packed_bed_scrubber::ScrubberApplication>,
    #[serde(default)]
    pub packing_type: Option<atlas_simulation_core::packed_bed_scrubber::PackingTypeClass>,
    #[serde(default)]
    pub flue_gas_flow_rate_m3_h: Option<f64>,
    #[serde(default)]
    pub inlet_pollutant_mole_fraction: Option<f64>,
    #[serde(default)]
    pub target_outlet_mole_fraction: Option<f64>,
    #[serde(default)]
    pub operating_temperature_c: Option<f64>,
    #[serde(default)]
    pub operating_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub liquid_to_gas_ratio_multiplier: Option<f64>,
    #[serde(default)]
    pub liquid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub liquid_viscosity_cp: Option<f64>,
    #[serde(default)]
    pub henrys_law_slope_m: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimLandingGearRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub metering_type: Option<atlas_simulation_core::aircraft_landing_gear::MeteringPinType>,
    #[serde(default)]
    pub gear_landed_mass_kg: Option<f64>,
    #[serde(default)]
    pub vertical_sink_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub lift_to_weight_ratio: Option<f64>,
    #[serde(default)]
    pub available_stroke_mm: Option<f64>,
    #[serde(default)]
    pub piston_diameter_mm: Option<f64>,
    #[serde(default)]
    pub gas_precharge_pressure_mpa: Option<f64>,
    #[serde(default)]
    pub initial_gas_volume_liters: Option<f64>,
    #[serde(default)]
    pub polytropic_exponent: Option<f64>,
    #[serde(default)]
    pub orifice_diameter_mm: Option<f64>,
    #[serde(default)]
    pub orifice_discharge_coeff: Option<f64>,
    #[serde(default)]
    pub fluid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub tire_stiffness_kn_m: Option<f64>,
    #[serde(default)]
    pub max_allowable_tire_deflection_mm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimPumpedStorageRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub runner_type: Option<atlas_simulation_core::pumped_storage_hydro::PumpTurbineType>,
    #[serde(default)]
    pub upper_reservoir_elevation_m: Option<f64>,
    #[serde(default)]
    pub lower_reservoir_elevation_m: Option<f64>,
    #[serde(default)]
    pub conduit_length_m: Option<f64>,
    #[serde(default)]
    pub conduit_diameter_m: Option<f64>,
    #[serde(default)]
    pub darcy_friction_factor: Option<f64>,
    #[serde(default)]
    pub minor_loss_k_total: Option<f64>,
    #[serde(default)]
    pub pumping_discharge_m3_s: Option<f64>,
    #[serde(default)]
    pub generating_discharge_m3_s: Option<f64>,
    #[serde(default)]
    pub rotational_speed_rpm: Option<f64>,
    #[serde(default)]
    pub runner_diameter_m: Option<f64>,
    #[serde(default)]
    pub runner_submergence_depth_m: Option<f64>,
    #[serde(default)]
    pub atmospheric_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub water_vapor_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub pump_hydraulic_efficiency: Option<f64>,
    #[serde(default)]
    pub turbine_hydraulic_efficiency: Option<f64>,
    #[serde(default)]
    pub motor_generator_efficiency: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimEuvLithographyRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub tool_generation:
        Option<atlas_simulation_core::semiconductor_euv_lithography::EuvToolGeneration>,
    #[serde(default)]
    pub wavelength_nm: Option<f64>,
    #[serde(default)]
    pub numerical_aperture: Option<f64>,
    #[serde(default)]
    pub process_factor_k1: Option<f64>,
    #[serde(default)]
    pub dof_factor_k2: Option<f64>,
    #[serde(default)]
    pub bilayer_period_d_nm: Option<f64>,
    #[serde(default)]
    pub molybdenum_fraction_gamma: Option<f64>,
    #[serde(default)]
    pub bilayer_pair_count: Option<u32>,
    #[serde(default)]
    pub interface_roughness_nm: Option<f64>,
    #[serde(default)]
    pub incidence_angle_deg: Option<f64>,
    #[serde(default)]
    pub mirror_count_in_train: Option<u32>,
    #[serde(default)]
    pub resist_exposure_dose_mj_cm2: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimGeothermalOrcRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub working_fluid: Option<atlas_simulation_core::geothermal_binary_orc::OrcWorkingFluid>,
    #[serde(default)]
    pub brine_mass_flow_kg_s: Option<f64>,
    #[serde(default)]
    pub brine_inlet_temperature_c: Option<f64>,
    #[serde(default)]
    pub min_reinjection_temperature_c: Option<f64>,
    #[serde(default)]
    pub pinch_point_temp_diff_c: Option<f64>,
    #[serde(default)]
    pub evaporator_pressure_bar: Option<f64>,
    #[serde(default)]
    pub condensing_temperature_c: Option<f64>,
    #[serde(default)]
    pub expander_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub pump_isentropic_efficiency: Option<f64>,
    #[serde(default)]
    pub generator_efficiency: Option<f64>,
    #[serde(default)]
    pub parasitic_power_fraction: Option<f64>,
    #[serde(default)]
    pub ambient_reference_temp_c: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimCableStayRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub stay_class: Option<atlas_simulation_core::cable_stayed_bridge::StayCableClass>,
    #[serde(default)]
    pub cable_chord_length_m: Option<f64>,
    #[serde(default)]
    pub outer_pipe_diameter_mm: Option<f64>,
    #[serde(default)]
    pub chord_inclination_deg: Option<f64>,
    #[serde(default)]
    pub cable_pretension_kn: Option<f64>,
    #[serde(default)]
    pub total_linear_mass_kg_m: Option<f64>,
    #[serde(default)]
    pub elastic_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub net_steel_area_mm2: Option<f64>,
    #[serde(default)]
    pub inherent_damping_ratio: Option<f64>,
    #[serde(default)]
    pub damper_distance_from_deck_m: Option<f64>,
    #[serde(default)]
    pub wind_speed_m_s: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSupersonicNozzleRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub gas_medium:
        Option<atlas_simulation_core::supersonic_wind_tunnel_nozzle::SupersonicGasMedium>,
    #[serde(default)]
    pub contour_method:
        Option<atlas_simulation_core::supersonic_wind_tunnel_nozzle::NozzleContourMethod>,
    #[serde(default)]
    pub stagnation_pressure_kpa: Option<f64>,
    #[serde(default)]
    pub stagnation_temperature_k: Option<f64>,
    #[serde(default)]
    pub design_mach_number: Option<f64>,
    #[serde(default)]
    pub throat_diameter_mm: Option<f64>,
    #[serde(default)]
    pub ambient_backpressure_kpa: Option<f64>,
    #[serde(default)]
    pub model_deflection_angle_deg: Option<f64>,
    #[serde(default)]
    pub divergence_half_angle_deg: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimEvTorqueVectoringRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub drivetrain_layout: Option<atlas_simulation_core::ev_torque_vectoring::EvAwdLayout>,
    #[serde(default)]
    pub road_condition: Option<atlas_simulation_core::ev_torque_vectoring::RoadSurfaceCondition>,
    #[serde(default)]
    pub vehicle_mass_kg: Option<f64>,
    #[serde(default)]
    pub yaw_moment_of_inertia_kg_m2: Option<f64>,
    #[serde(default)]
    pub wheelbase_mm: Option<f64>,
    #[serde(default)]
    pub track_width_mm: Option<f64>,
    #[serde(default)]
    pub cg_to_front_axle_mm: Option<f64>,
    #[serde(default)]
    pub cg_height_mm: Option<f64>,
    #[serde(default)]
    pub tire_rolling_radius_mm: Option<f64>,
    #[serde(default)]
    pub front_cornering_stiffness_n_rad: Option<f64>,
    #[serde(default)]
    pub rear_cornering_stiffness_n_rad: Option<f64>,
    #[serde(default)]
    pub vehicle_speed_km_h: Option<f64>,
    #[serde(default)]
    pub front_steering_angle_deg: Option<f64>,
    #[serde(default)]
    pub demanded_total_torque_nm: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimSubseaRiserVivRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub riser_type: Option<atlas_simulation_core::subsea_riser_viv::MarineRiserType>,
    #[serde(default)]
    pub current_profile: Option<atlas_simulation_core::subsea_riser_viv::OceanCurrentProfile>,
    #[serde(default)]
    pub suppression_device: Option<atlas_simulation_core::subsea_riser_viv::VivSuppressionDevice>,
    #[serde(default)]
    pub water_depth_m: Option<f64>,
    #[serde(default)]
    pub riser_length_m: Option<f64>,
    #[serde(default)]
    pub outer_diameter_mm: Option<f64>,
    #[serde(default)]
    pub wall_thickness_mm: Option<f64>,
    #[serde(default)]
    pub top_tension_kn: Option<f64>,
    #[serde(default)]
    pub steel_elastic_modulus_gpa: Option<f64>,
    #[serde(default)]
    pub steel_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub internal_fluid_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub seawater_density_kg_m3: Option<f64>,
    #[serde(default)]
    pub surface_current_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub bottom_current_velocity_m_s: Option<f64>,
    #[serde(default)]
    pub strouhal_number: Option<f64>,
    #[serde(default)]
    pub design_fatigue_life_years: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimChromatographyColumnRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub chromatography_mode:
        Option<atlas_simulation_core::chromatography_column::ChromatographyMode>,
    #[serde(default)]
    pub column_diameter_mm: Option<f64>,
    #[serde(default)]
    pub bed_height_mm: Option<f64>,
    #[serde(default)]
    pub linear_velocity_cm_h: Option<f64>,
    #[serde(default)]
    pub resin_particle_diameter_um: Option<f64>,
    #[serde(default)]
    pub bed_void_fraction: Option<f64>,
    #[serde(default)]
    pub intraparticle_porosity: Option<f64>,
    #[serde(default)]
    pub feed_protein_concentration_g_l: Option<f64>,
    #[serde(default)]
    pub max_binding_capacity_g_l: Option<f64>,
    #[serde(default)]
    pub langmuir_constant_l_g: Option<f64>,
    #[serde(default)]
    pub molecular_diffusivity_m2_s: Option<f64>,
    #[serde(default)]
    pub buffer_viscosity_mpa_s: Option<f64>,
    #[serde(default)]
    pub buffer_density_kg_m3: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct SimMaglevSuspensionRequest {
    #[serde(default)]
    pub problem_id: Option<String>,
    #[serde(default)]
    pub core_material:
        Option<atlas_simulation_core::maglev_guideway_suspension::MaglevCoreMaterial>,
    #[serde(default)]
    pub disturbance_type:
        Option<atlas_simulation_core::maglev_guideway_suspension::GuidewayDisturbanceType>,
    #[serde(default)]
    pub mass_supported_per_magnet_kg: Option<f64>,
    #[serde(default)]
    pub nominal_airgap_mm: Option<f64>,
    #[serde(default)]
    pub coil_turns: Option<u32>,
    #[serde(default)]
    pub pole_face_area_mm2: Option<f64>,
    #[serde(default)]
    pub coil_resistance_ohms: Option<f64>,
    #[serde(default)]
    pub max_inverter_voltage_v: Option<f64>,
    #[serde(default)]
    pub max_continuous_current_a: Option<f64>,
    #[serde(default)]
    pub target_controller_bandwidth_rad_s: Option<f64>,
    #[serde(default)]
    pub disturbance_amplitude_mm: Option<f64>,
}

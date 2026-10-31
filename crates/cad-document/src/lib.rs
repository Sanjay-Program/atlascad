//! A local parametric CAD project that can be saved, reopened, edited, and rebuilt.
//!
//! This is the smallest honest product workflow the platform can complete end to end on this
//! hardware, without a GUI:
//!
//! 1. Create a project with named parameters.
//! 2. Create a sketch: rectangle, circle, or regular polygon driven by parameters, or a polyline
//!    drawn point by point.
//! 3. Append an ordered history of extrusions. The first starts the body; every later feature
//!    either unions a new extrusion into it
//!    ([`Project::add_join_extrude`]) or is refused, because this document carries **one body**.
//! 4. Change a parameter or the sketch profile and rebuild every dependent feature.
//! 5. Undo and redo.
//! 6. Save and reopen the project file, including migration from schemas 1, 2 and 3.
//! 7. Evaluate through a [`cad_contract::Kernel`] and inspect volume.
//!
//! # Rebuilding and failure
//!
//! [`Project::build`] rebuilds the history in order and **reports** a kernel failure instead of
//! hiding it: the failing feature keeps its ID and message, evaluation stops there, and the mesh of
//! the last feature that did rebuild is returned. [`Project::evaluate`] is the strict wrapper used
//! by callers that want an error. Join is only attempted when the kernel advertises
//! `BooleanUnion`; nothing here falls back to an approximate mesh.
//!
//! # Approximations that are stated, not hidden
//!
//! The kernel contract takes a polygon profile, so a circle is tessellated into a deterministic
//! polygon whose segment count follows the document's linear tolerance. A circle's volume is
//! therefore the polygon's volume, slightly below `πr²h`; the error shrinks with the tolerance and
//! is reported by [`Project::sketch_segment_count`]. No operation here claims an exact analytic
//! cylinder.
//!
//! What this is **not**: a general sketch constraint solver, arcs and splines, assemblies, drawings,
//! fillets, a working boolean difference, or multiple bodies. Those are not advertised here.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cad_contract::document::{DocumentError, DocumentHeader, Revision};
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::kernel::{Kernel, SolidId, validate_profile};
use cad_contract::mesh::Mesh;
use cad_contract::tol::Tolerance;
use cad_contract::units::Unit;
use cad_engineering_model::{
    ArtifactId, ArtifactRegistry, DerivedArtifact, DocumentRef, StalenessReport,
};
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Payload schema of a `.zcad` project file. Independent of [`DocumentHeader::schema`].
///
/// * Schema 1 stored a rectangle sketch as flat `width_param`/`height_param` fields.
/// * Schema 2 stores an explicit [`SketchProfile`]; schema-1 files are migrated on load.
/// * Schema 3 gives every extrude an explicit [`ExtrudeOperation`], so the ordered feature list is
///   a real history (first feature starts the body, later features join it) instead of a list whose
///   last entry was the only one evaluated.
/// * Schema 4 adds [`SketchProfile::Polyline`], a closed profile drawn point by point. The addition
///   is additive for readers of schema 4 but not for a schema-3 reader, which refuses an unknown
///   profile kind; the version is bumped so an older build says "newer schema" rather than
///   "invalid profile".
/// * Schema 5 replaces a feature's flat `depth_param` with an explicit [`FeatureKind`], so a feature
///   is an extrude **or** a revolve instead of an extrude with an optional variant. A schema-4 file
///   is migrated on load by wrapping each feature's `depth_param` in `{"type":"extrude", …}`.
///   Schemas 1 to 4 are migrated on load.
/// * Schema 6 adds [`SketchProfile::Sector`], an arc-based closed profile. A schema-5 build refuses
///   the new profile kind, which would read as "invalid profile" rather than "newer schema", so the
///   version is bumped; a schema-5 file is migrated to 6 on load with no field changes.
/// * Schema 7 adds named revisions: a bounded list of named, timestamped copies of the editable
///   history. A schema-6 reader would fail on the unknown `revisions` field as an invalid document,
///   so the version is bumped; a schema-6 file is migrated to 7 on load with no field changes and
///   starts with no revisions.
/// * Schema 8 gives a sketch a plane height, so a part is not limited to the z = 0 plane, and gives
///   an extrude a direction. Both fields are defaulted, so a schema-7 file is already valid; the
///   version is bumped because a schema-7 reader would refuse the new sketch field as an invalid
///   document rather than report a newer schema.
/// * Schema 9 embeds the derived-artifact registry, so a drawing's or toolpath's source revision
///   travels with the design instead of living in a sidecar that can be lost or copied separately.
///   The field is defaulted, so a schema-8 file opens with an empty registry; the version is bumped
///   so a schema-8 build says "newer schema" instead of discarding the records.
pub const PROJECT_SCHEMA: u32 = 9;

/// Smallest regular polygon the sketch model accepts.
pub const MIN_POLYGON_SIDES: u32 = 3;
/// Largest regular polygon the sketch model accepts. Above this a polygon is a worse circle.
pub const MAX_POLYGON_SIDES: u32 = 256;
/// Smallest closed polyline the sketch model accepts: a triangle.
pub const MIN_POLYLINE_POINTS: usize = 3;
/// Largest closed polyline the sketch model accepts. A drawn profile is bounded so a hostile or
/// accidental file cannot ask the kernel to build a million-facet prism.
pub const MAX_POLYLINE_POINTS: usize = 256;

/// How far, in millimetres, a cut's tool is extended past a face it shares with the body.
///
/// This is the difference between an exact cut and a cut with a defect. Every feature starts on the
/// z = 0 sketch plane, so a cut whose depth equals the body's thickness has *both* of its faces
/// coincident with the body's. Coincidence is the case the boolean kernel handles worst — truck
/// returns no result at all for coplanar operands and only succeeds after moving one of them, and
/// that movement leaves a membrane of the same size inside the body (see
/// `docs/kernel-comparison.md` §4.1 and §4.2).
///
/// Extending the tool instead is exact: the extra material lies outside the body, so it changes
/// nothing, while the faces the tool does share are moved off the body and the operation is
/// ordinary. 0.01 mm is ten times the boolean tolerance (`DEFAULT_BOOLEAN_TOLERANCE_MM`), which is
/// what makes the coincidence reliably broken, and it is far below the tolerance of any
/// manufacturing process this application targets. A through-cut's volume is therefore exact; a
/// pocket whose end is inside the body is not extended at all, so it is exact too.
pub const CUT_OVERSHOOT_MM: f64 = 1e-2;

/// How far, in millimetres, a join tool is moved into the body it joins when a face of each would
/// otherwise be coincident.
///
/// A union whose operands share a face is the kernel's known weak case (see
/// `docs/kernel-comparison.md` §4): the corpus records tangent contact as a failure. Placing a boss
/// on a body's top or bottom face produces exactly that, which is why a sketch plane above z = 0
/// would otherwise be unusable for the one workflow it exists to enable.
///
/// The adjustment moves the coincident face **into** the body instead of out of it, so the extra
/// material is already present and the union's volume is unchanged. It is deliberately smaller than
/// the 0.05 mm document tolerance and ten times the boolean tolerance, so it breaks the coincidence
/// reliably without moving the visible face.
pub const JOIN_OVERLAP_MM: f64 = 1e-2;

const MIN_CIRCLE_SEGMENTS: usize = 24;
const MAX_CIRCLE_SEGMENTS: usize = 256;
/// Fewest facets an arc may be approximated by: two, so a sector is never a single chord.
const MIN_ARC_SEGMENTS: usize = 2;
/// Maximum accepted serialized project size, checked before parsing untrusted input.
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;

/// Named revisions kept per project. Each one is a full copy of the history, so the bound is about
/// file size: the oldest is dropped when a new revision would exceed it.
pub const MAX_REVISIONS: usize = 32;
/// Longest revision name, in characters, that the document accepts.
pub const MAX_REVISION_NAME_CHARS: usize = 80;

const MAX_UNDO: usize = 64;
static TEMP_FILE_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Errors a project operation can produce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProjectError {
    #[error("document: {0}")]
    Document(#[from] DocumentError),
    #[error("kernel: {0}")]
    Kernel(String),
    #[error("{0}")]
    Invalid(String),
    #[error("unknown parameter `{0}`")]
    UnknownParameter(String),
    #[error("unknown sketch {0}")]
    UnknownSketch(u64),
    #[error("unknown revision {0}")]
    UnknownRevision(u64),
    #[error("feature {feature} failed: {message}")]
    FeatureFailed { feature: u64, message: String },
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("project schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("I/O at {path}: {message}")]
    Io { path: String, message: String },
    #[error("engineering model: {0}")]
    Engineering(#[from] cad_engineering_model::ModelError),
}

impl From<cad_contract::error::KernelError> for ProjectError {
    fn from(error: cad_contract::error::KernelError) -> Self {
        ProjectError::Kernel(error.to_string())
    }
}

impl From<cad_contract::ids::IdError> for ProjectError {
    fn from(error: cad_contract::ids::IdError) -> Self {
        ProjectError::Invalid(error.to_string())
    }
}

/// A named scalar that features read when they rebuild.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parameter {
    pub id: String,
    pub value: f64,
}

/// The closed profile a sketch draws, in the XY plane, in document units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum SketchProfile {
    /// Axis-aligned rectangle from `origin`, sized by two parameters.
    Rectangle {
        width_param: String,
        height_param: String,
    },
    /// Circle centred on the sketch origin; `diameter_param` is the diameter.
    Circle { diameter_param: String },
    /// Regular polygon centred on the sketch origin, circumscribed by `diameter_param`.
    RegularPolygon { diameter_param: String, sides: u32 },
    /// A closed profile given point by point, relative to the sketch origin, in document units.
    ///
    /// The loop is implicitly closed (the last point joins the first). Points are validated on
    /// creation and on load: between [`MIN_POLYLINE_POINTS`] and [`MAX_POLYLINE_POINTS`] finite
    /// points, no repeated consecutive point, and a non-zero enclosed area. Winding is normalised
    /// to counter-clockwise when the profile is resolved, so the kernel always receives an
    /// outward-oriented solid.
    Polyline { points: Vec<[f64; 2]> },
    /// A pie slice: the region bounded by one arc and the two radii to its centre.
    ///
    /// This is the arc-based profile. Unlike [`SketchProfile::Circle`], which approximates the whole
    /// circumference, a sector is a single arc from `start_degrees` sweeping `sweep_degrees`
    /// counter-clockwise, plus the two radii that close it at the sketch origin. The arc is
    /// tessellated with the same chord tolerance as a circle and scaled to the swept fraction of
    /// the turn, so a small sector does not inherit a full circle's facet count.
    ///
    /// `sweep_degrees` must be strictly between 0 and 360: a zero sweep encloses no area, and a full
    /// turn is not a sector (the two radii coincide) — use [`SketchProfile::Circle`] for a disc.
    Sector {
        /// Diameter of the circle the sector is cut from.
        diameter_param: String,
        /// Where the arc starts, in degrees counter-clockwise from +X; used modulo 360.
        start_degrees: f64,
        /// How far the arc sweeps counter-clockwise, in degrees, exclusive of 0 and 360.
        sweep_degrees: f64,
    },
}

impl SketchProfile {
    /// Human-readable profile name used in reports and the UI.
    pub const fn kind(&self) -> &'static str {
        match self {
            SketchProfile::Rectangle { .. } => "rectangle",
            SketchProfile::Circle { .. } => "circle",
            SketchProfile::RegularPolygon { .. } => "regular_polygon",
            SketchProfile::Polyline { .. } => "polyline",
            SketchProfile::Sector { .. } => "sector",
        }
    }
}

/// A closed profile in a horizontal plane, positioned by `origin` and `plane_z`, in document units.
///
/// A sketch plane is parallel to the world XY plane at height `plane_z`. One plane height is enough
/// to place a boss on a top face or to start a pocket below one; a tilted or arbitrary plane is not
/// part of the document, and a feature therefore never claims to build on one.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sketch {
    pub id: EntityId,
    pub origin: [f64; 2],
    /// Height of the sketch plane above z = 0, in document units. Defaults to 0 for a schema-7
    /// file, which could only sketch on the world plane.
    #[serde(default)]
    pub plane_z: f64,
    pub profile: SketchProfile,
}

/// Which way an extrusion is swept from its sketch plane.
///
/// The depth parameter is a length, and the parameter map holds positive lengths, so the direction
/// is a separate, explicit input rather than a sign the document would have to infer from a
/// negative parameter. `Down` is what makes a pocket on a top face expressible: the sketch is
/// placed on that face and the tool is swept into the body.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeDirection {
    /// Sweep towards +Z from the sketch plane.
    #[default]
    Up,
    /// Sweep towards -Z from the sketch plane.
    Down,
}

impl ExtrudeDirection {
    /// Label used in reports and the model tree.
    pub const fn label(self) -> &'static str {
        match self {
            ExtrudeDirection::Up => "up",
            ExtrudeDirection::Down => "down",
        }
    }

    /// `+1` for up, `-1` for down: the sign applied to a resolved depth.
    pub const fn sign(self) -> f64 {
        match self {
            ExtrudeDirection::Up => 1.0,
            ExtrudeDirection::Down => -1.0,
        }
    }
}

/// What an extrude does to the body built by the earlier features.
///
/// The operation is explicit in the persisted feature so the ordered history is readable without
/// rebuilding it. The first feature of a project must be `new_body`; every later feature is either
/// a `join` (union into the running body) or a `cut` (difference of the extrusion out of the
/// running body). A project carries **one body**, so a second `new_body` is refused and a `join` or
/// `cut` before the first body exists is refused.
///
/// The kernel advertises the boolean each operation needs; a cut is refused with a stated reason when
/// the kernel does not advertise `BooleanDifference`, in the same way a join is refused when it does
/// not advertise `BooleanUnion`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExtrudeOperation {
    /// Start the body. Only the first feature of a project may do this.
    #[default]
    NewBody,
    /// Union this extrusion into the body built by the earlier features.
    Join,
    /// Remove this extrusion from the body built by the earlier features.
    Cut,
}

impl ExtrudeOperation {
    /// Label used in reports and the model tree.
    pub const fn label(self) -> &'static str {
        match self {
            ExtrudeOperation::NewBody => "new body",
            ExtrudeOperation::Join => "join",
            ExtrudeOperation::Cut => "cut",
        }
    }
}

/// The axis a revolve turns about, in the sketch's own XY plane through its origin.
///
/// Only axes through the sketch origin are offered because that is what a sketch can express: the
/// profile is drawn relative to the origin, so "the local Y axis" is a named, checkable line rather
/// than an arbitrary one the document would have to store separately.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RevolveAxis {
    /// The sketch's local Y axis through its origin.
    #[default]
    Vertical,
    /// The sketch's local X axis through its origin.
    Horizontal,
}

impl RevolveAxis {
    /// Label used in reports and the model tree.
    pub const fn label(self) -> &'static str {
        match self {
            RevolveAxis::Vertical => "vertical axis",
            RevolveAxis::Horizontal => "horizontal axis",
        }
    }

    /// A point on the axis and its direction, in the sketch's own plane.
    pub const fn line(self) -> ([f64; 2], [f64; 2]) {
        match self {
            RevolveAxis::Vertical => ([0.0, 0.0], [0.0, 1.0]),
            RevolveAxis::Horizontal => ([0.0, 0.0], [1.0, 0.0]),
        }
    }
}

/// What a feature does with its sketch.
///
/// A feature is one of these, not an extrude with optional extras: a revolve has no depth, and an
/// extrude has no axis, so the persisted shape says which inputs are required.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub enum FeatureKind {
    /// Sweep the profile by `depth_param`, in document units, in `direction` from the sketch plane.
    Extrude {
        depth_param: String,
        /// Defaults to `up` for a schema-7 file, which could only extrude towards +Z.
        #[serde(default)]
        direction: ExtrudeDirection,
    },
    /// Sweep the profile a full turn about `axis`, producing a solid of revolution.
    ///
    /// A full turn is the only mode: a partial sweep is an open shell, which is not a body, and the
    /// document carries bodies only. See [`RevolveAxis`] for which line is used.
    Revolve { axis: RevolveAxis },
}

impl FeatureKind {
    /// Label used in reports and the model tree.
    pub const fn label(&self) -> &'static str {
        match self {
            FeatureKind::Extrude { .. } => "extrude",
            FeatureKind::Revolve { .. } => "revolve",
        }
    }
}

/// One step of the ordered history: a sketch consumed by an [`FeatureKind`], and what that does to
/// the body built by the earlier steps.
///
/// Features are stored in build order and may only reference a sketch created before them; a
/// feature ID is allocated after the sketch ID it consumes, so `sketch_id < id` is the persisted
/// proof that the reference points backwards.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: EntityId,
    pub sketch_id: EntityId,
    /// Defaults to `new_body` when absent so a schema-2 payload still deserialises.
    #[serde(default)]
    pub operation: ExtrudeOperation,
    pub kind: FeatureKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    parameters: Vec<Parameter>,
    sketches: Vec<Sketch>,
    features: Vec<Feature>,
    next_entity: u64,
}

/// A named, timestamped copy of the editable history.
///
/// A revision stores the same four fields an undo snapshot does — parameters, sketches, features and
/// the next entity id — and deliberately *not* the undo/redo stacks, the recovery state or the other
/// revisions, so restoring one cannot resurrect a deleted revision or a stale history.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct NamedRevision {
    id: u64,
    name: String,
    created_at_unix: u64,
    /// The document revision this copy was taken at, for display in the list.
    revision: u64,
    snapshot: Snapshot,
}

/// What the UI needs to list a named revision without carrying its geometry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct RevisionSummary {
    pub id: u64,
    pub name: String,
    pub created_at_unix: u64,
    /// The document revision the copy was taken at.
    pub revision: u64,
    pub sketches: usize,
    pub features: usize,
}

/// The persistent project. Kernel handles are never stored here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Project {
    pub schema: u32,
    pub header: DocumentHeader,
    pub revision: Revision,
    pub name: String,
    parameters: Vec<Parameter>,
    sketches: Vec<Sketch>,
    features: Vec<Feature>,
    next_entity: u64,
    #[serde(default)]
    undo: Vec<Snapshot>,
    #[serde(default)]
    redo: Vec<Snapshot>,
    #[serde(default)]
    revisions: Vec<NamedRevision>,
    #[serde(default = "first_revision_id")]
    next_revision: u64,
}

/// First revision id. Named revisions have their own id space, separate from entity ids, because a
/// revision is not geometry and must not shift sketch/feature numbering.
const fn first_revision_id() -> u64 {
    1
}

/// Geometry produced by evaluating the feature tree against a kernel.
#[derive(Debug, Clone)]
pub struct Evaluated {
    pub mesh: Mesh,
    pub volume_mm3: f64,
    pub triangles: usize,
    pub solid: SolidId,
}

/// Whether one feature rebuilt, and the measurement of the body after it.
#[derive(Debug, Clone, PartialEq)]
pub enum FeatureOutcome {
    Built { volume_mm3: f64, triangles: usize },
    Failed { message: String },
}

/// The rebuild result of one feature in history order.
#[derive(Debug, Clone, PartialEq)]
pub struct FeatureReport {
    pub id: EntityId,
    pub outcome: FeatureOutcome,
}

/// The first feature that could not rebuild, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeatureFailure {
    pub id: EntityId,
    pub message: String,
}

/// The result of rebuilding the ordered history.
///
/// A rebuild that fails part-way is still a result, not an error: the caller gets one report per
/// attempted feature, the mesh of the last feature that succeeded, and the failure that stopped it.
/// The solid handle is owned by the kernel that produced it and must be released by the caller.
#[derive(Debug, Clone, Default)]
pub struct Build {
    pub reports: Vec<FeatureReport>,
    pub mesh: Option<Mesh>,
    pub solid: Option<SolidId>,
    pub failed: Option<FeatureFailure>,
}

impl Build {
    /// True when every feature in the history rebuilt.
    pub fn is_complete(&self) -> bool {
        self.failed.is_none()
    }

    /// The outcome recorded for `id`, if it was attempted.
    pub fn feature(&self, id: EntityId) -> Option<&FeatureOutcome> {
        self.reports
            .iter()
            .find(|report| report.id == id)
            .map(|report| &report.outcome)
    }
}

/// Most cached prefixes kept alive at once.
///
/// Each entry holds one B-rep body and its tessellation, so the bound is about memory, not time: a
/// long history would otherwise keep every intermediate body alive. When the limit is exceeded the
/// oldest entries are released and their prefix simply rebuilds next time — a cache miss, never a
/// wrong answer. The newest entries are the ones an edit is most likely to reuse.
const MAX_CACHE_ENTRIES: usize = 32;

/// One reusable rebuild result: the body after a prefix of the history.
#[derive(Debug)]
struct CacheEntry {
    /// Hash of every feature recipe from the start of the history through this one.
    prefix: u64,
    /// The body after this feature. Owned by the cache; released by [`BuildCache::clear`].
    solid: SolidId,
    mesh: Mesh,
    volume_mm3: f64,
    triangles: usize,
}

/// Reusable per-feature rebuild results, tied to the kernel that produced them.
///
/// A full rebuild re-runs every extrude, every boolean and every tessellation in the history. That
/// is what `GET /api/state` does today, and it grows with history length even when only the last
/// parameter changed. This cache stores the body after each prefix of the ordered history, keyed by
/// a hash of the *resolved* inputs of that prefix: the millimetre profile points (which already
/// fold in parameters, units, sketch origin and circle segmentation), the millimetre depth, the
/// operation, the document tolerance and the kernel identity.
///
/// # Why this is safe
///
/// * A prefix hash changes as soon as any input of any feature in that prefix changes, so editing
///   an *earlier* feature cannot produce a hit on a prefix that contains it.
/// * The key is the resolved geometry rather than the parameter values, so a rename or an unrelated
///   parameter edit does not invalidate anything, and an edit that happens to restore an earlier
///   value hits the same entry.
/// * A cache hit returns the stored solid, mesh, volume and triangle count together, so the reported
///   measurement can never disagree with the geometry the cache holds.
/// * The kernel identity (name, version, capabilities) is part of the seed. Handing the cache to a
///   different kernel clears it instead of returning handles that kernel never issued.
/// * A feature that fails is never cached, so a failing history reports the same failure every time.
///
/// # Ownership
///
/// Entries own their solids. A caller building through [`Project::build_with_cache`] must **not**
/// release [`Build::solid`], because the cache may hand that handle out again; release the cache
/// instead with [`BuildCache::clear`] when the kernel is about to go away. `Project::build` keeps the
/// uncached contract, where the caller releases the returned solid.
#[derive(Debug, Default)]
pub struct BuildCache {
    /// Identity of the kernel the entries belong to; `None` when the cache is empty.
    identity: Option<u64>,
    entries: Vec<CacheEntry>,
    hits: u64,
    misses: u64,
}

impl BuildCache {
    /// An empty cache.
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of cached prefixes.
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    /// True when nothing is cached.
    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// How many feature rebuilds were served from the cache.
    pub fn hits(&self) -> u64 {
        self.hits
    }

    /// How many feature rebuilds had to run the kernel.
    pub fn misses(&self) -> u64 {
        self.misses
    }

    /// Drop every entry and release its solid. Must be called with the kernel that built the cache.
    pub fn clear<K: Kernel>(&mut self, kernel: &mut K) {
        for entry in self.entries.drain(..) {
            kernel.release(entry.solid);
        }
        self.identity = None;
        self.hits = 0;
        self.misses = 0;
    }
}

/// FNV-1a offset basis, used to seed the prefix hash.
const HASH_SEED: u64 = 0xcbf2_9ce4_8422_2325;
const HASH_PRIME: u64 = 0x0000_0100_0000_01b3;

fn hash_bytes(mut hash: u64, bytes: &[u8]) -> u64 {
    for byte in bytes {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(HASH_PRIME);
    }
    hash
}

fn hash_u64(hash: u64, value: u64) -> u64 {
    hash_bytes(hash, &value.to_le_bytes())
}

/// A stable hash of what the kernel is and what it can do.
///
/// Two kernels with the same name, version and capability set are assumed to build the same solid
/// from the same input; a different one clears the cache rather than risk adopting a handle it never
/// issued.
fn kernel_identity<K: Kernel>(kernel: &K) -> u64 {
    let info = kernel.info();
    let mut hash = hash_bytes(HASH_SEED, info.name.as_bytes());
    hash = hash_bytes(hash, info.version.as_bytes());
    for capability in info.capabilities.iter() {
        hash = hash_bytes(hash, capability.label().as_bytes());
    }
    hash
}

/// Extend a prefix hash with one feature's resolved inputs.
///
/// The feature ID is hashed first, so two features that resolve to identical geometry at different
/// positions in the history get different prefixes. Without it, a join and a repeated join of the
/// same profile would share a key and the second would wrongly reuse the first's body.
fn prefix_hash(
    prefix: u64,
    feature: &Feature,
    profile: &[[f64; 2]],
    action: &ResolvedAction,
    tolerance_mm: f64,
) -> u64 {
    let mut hash = hash_u64(prefix, feature.id.get());
    hash = hash_bytes(
        hash,
        &[match feature.operation {
            ExtrudeOperation::NewBody => 0,
            ExtrudeOperation::Join => 1,
            ExtrudeOperation::Cut => 2,
        }],
    );
    hash = hash_bytes(
        hash,
        &[match action {
            ResolvedAction::Extrude { .. } => 0,
            ResolvedAction::Revolve { .. } => 1,
        }],
    );
    hash = hash_u64(hash, profile.len() as u64);
    for point in profile {
        hash = hash_u64(hash, point[0].to_bits());
        hash = hash_u64(hash, point[1].to_bits());
    }
    match action {
        ResolvedAction::Extrude { depth_mm, plane_mm } => {
            hash = hash_u64(hash, depth_mm.to_bits());
            hash = hash_u64(hash, plane_mm.to_bits());
        }
        ResolvedAction::Revolve {
            axis_origin_mm,
            axis_direction_mm,
            plane_mm,
        } => {
            hash = hash_u64(hash, axis_origin_mm[0].to_bits());
            hash = hash_u64(hash, axis_origin_mm[1].to_bits());
            hash = hash_u64(hash, axis_direction_mm[0].to_bits());
            hash = hash_u64(hash, axis_direction_mm[1].to_bits());
            hash = hash_u64(hash, plane_mm.to_bits());
        }
    }
    hash_u64(hash, tolerance_mm.to_bits())
}

/// What a feature asks the kernel to do, resolved to millimetre inputs.
///
/// Private: it is the resolved form of [`FeatureKind`], not part of the persisted document.
#[derive(Debug, Clone, PartialEq)]
enum ResolvedAction {
    Extrude {
        /// Signed depth: negative when the feature sweeps down from its sketch plane.
        depth_mm: f64,
        /// Height of the sketch plane above z = 0.
        plane_mm: f64,
    },
    Revolve {
        axis_origin_mm: [f64; 2],
        axis_direction_mm: [f64; 2],
        /// Height of the sketch plane above z = 0.
        plane_mm: f64,
    },
}

/// The body one feature produced, before the caller decides who owns it.
struct RebuiltFeature {
    solid: SolidId,
    mesh: Mesh,
    volume_mm3: f64,
    triangles: usize,
}

impl Project {
    /// A new millimetre project with width/height/depth parameters and no geometry yet.
    pub fn new(name: impl Into<String>, document: DocumentId, unit: Unit) -> Self {
        Project {
            schema: PROJECT_SCHEMA,
            header: DocumentHeader::new(document, unit),
            revision: Revision::INITIAL,
            name: name.into(),
            parameters: vec![
                Parameter {
                    id: "width".into(),
                    value: 10.0,
                },
                Parameter {
                    id: "height".into(),
                    value: 10.0,
                },
                Parameter {
                    id: "depth".into(),
                    value: 5.0,
                },
                Parameter {
                    id: "diameter".into(),
                    value: 10.0,
                },
            ],
            sketches: Vec::new(),
            features: Vec::new(),
            next_entity: 1,
            undo: Vec::new(),
            redo: Vec::new(),
            revisions: Vec::new(),
            next_revision: first_revision_id(),
        }
    }

    /// A prismatic part: rectangle sketch plus extrude, ready to evaluate.
    pub fn box_part(
        name: impl Into<String>,
        document: DocumentId,
        unit: Unit,
        size: [f64; 3],
    ) -> Result<Self, ProjectError> {
        let mut project = Project::new(name, document, unit);
        project.set_parameter("width", size[0])?;
        project.set_parameter("height", size[1])?;
        project.set_parameter("depth", size[2])?;
        project.add_rectangle_sketch("width", "height", [0.0, 0.0])?;
        let sketch_id = project
            .sketches
            .last()
            .ok_or_else(|| ProjectError::Invalid("sketch was not created".into()))?
            .id;
        project.add_extrude(sketch_id, "depth")?;
        Ok(project)
    }

    pub fn parameters(&self) -> &[Parameter] {
        &self.parameters
    }

    pub fn sketches(&self) -> &[Sketch] {
        &self.sketches
    }

    pub fn features(&self) -> &[Feature] {
        &self.features
    }

    /// Named revisions in the order they were recorded, oldest first.
    pub fn revisions(&self) -> Vec<RevisionSummary> {
        self.revisions
            .iter()
            .map(|revision| RevisionSummary {
                id: revision.id,
                name: revision.name.clone(),
                created_at_unix: revision.created_at_unix,
                revision: revision.revision,
                sketches: revision.snapshot.sketches.len(),
                features: revision.snapshot.features.len(),
            })
            .collect()
    }

    /// Record the current history under `name` and return the revision's id.
    ///
    /// The copy holds parameters, sketches and features — the editable history — so restoring it
    /// later reproduces the model exactly, including which features had built. Undo/redo is *not*
    /// copied: a revision is a saved state, not a history. Recording a revision is not itself an
    /// undo step, because it changes no geometry; it does bump the document revision, so the file is
    /// marked unsaved and the copy is not lost by closing the window.
    ///
    /// When [`MAX_REVISIONS`] copies exist the oldest is dropped. That is a stated retention rule,
    /// not silent data loss: [`Project::revisions`] always shows what is kept.
    pub fn add_revision(&mut self, name: &str) -> Result<u64, ProjectError> {
        let name = name.trim();
        if name.is_empty() {
            return Err(ProjectError::Invalid(
                "a revision needs a name; use something you will recognise later".into(),
            ));
        }
        if name.chars().count() > MAX_REVISION_NAME_CHARS {
            return Err(ProjectError::Invalid(format!(
                "a revision name is limited to {MAX_REVISION_NAME_CHARS} characters"
            )));
        }
        if self
            .revisions
            .iter()
            .any(|revision| revision.name.eq_ignore_ascii_case(name))
        {
            return Err(ProjectError::Invalid(format!(
                "a revision named `{name}` already exists; choose another name or restore it"
            )));
        }
        if self.revisions.len() >= MAX_REVISIONS {
            self.revisions.remove(0);
        }
        let id = self.next_revision;
        self.next_revision = self
            .next_revision
            .checked_add(1)
            .ok_or_else(|| ProjectError::Invalid("revision id space exhausted".into()))?;
        self.revisions.push(NamedRevision {
            id,
            name: name.to_string(),
            created_at_unix: unix_now(),
            revision: self.revision.get(),
            snapshot: self.snapshot(),
        });
        self.bump_revision()?;
        Ok(id)
    }

    /// Replace the current history with the copy named by `id`.
    ///
    /// The current history is pushed onto the undo stack first, so a restore is always reversible
    /// with one undo — including when the user restores the wrong revision by accident. The revision
    /// list itself is untouched by a restore.
    pub fn restore_revision(&mut self, id: u64) -> Result<(), ProjectError> {
        let snapshot = self
            .revisions
            .iter()
            .find(|revision| revision.id == id)
            .map(|revision| revision.snapshot.clone())
            .ok_or(ProjectError::UnknownRevision(id))?;
        self.push_undo();
        self.restore(snapshot);
        self.bump_revision()
    }

    /// Delete a named revision. The geometry is unaffected.
    pub fn remove_revision(&mut self, id: u64) -> Result<(), ProjectError> {
        let before = self.revisions.len();
        self.revisions.retain(|revision| revision.id != id);
        if self.revisions.len() == before {
            return Err(ProjectError::UnknownRevision(id));
        }
        self.bump_revision()
    }

    /// Number of undo snapshots available.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Number of redo snapshots available.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    pub fn parameter(&self, id: &str) -> Result<f64, ProjectError> {
        self.parameters
            .iter()
            .find(|parameter| parameter.id == id)
            .map(|parameter| parameter.value)
            .ok_or_else(|| ProjectError::UnknownParameter(id.to_string()))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            parameters: self.parameters.clone(),
            sketches: self.sketches.clone(),
            features: self.features.clone(),
            next_entity: self.next_entity,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.parameters = snapshot.parameters;
        self.sketches = snapshot.sketches;
        self.features = snapshot.features;
        self.next_entity = snapshot.next_entity;
    }

    fn bump_revision(&mut self) -> Result<(), ProjectError> {
        self.revision = self
            .revision
            .next()
            .ok_or_else(|| ProjectError::Invalid("revision counter exhausted".into()))?;
        Ok(())
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }

    fn allocate_entity(&mut self) -> Result<EntityId, ProjectError> {
        let id = EntityId::new(self.next_entity)?;
        self.next_entity = self
            .next_entity
            .checked_add(1)
            .ok_or_else(|| ProjectError::Invalid("entity id space exhausted".into()))?;
        Ok(id)
    }

    /// Change a parameter. Invalid numbers are refused and the previous value is kept.
    pub fn set_parameter(&mut self, id: &str, value: f64) -> Result<(), ProjectError> {
        if !value.is_finite() || value <= 0.0 {
            return Err(ProjectError::Invalid(format!(
                "parameter `{id}` must be a finite positive number, got {value}"
            )));
        }
        let _ = self.parameter(id)?;
        self.push_undo();
        for parameter in &mut self.parameters {
            if parameter.id == id {
                parameter.value = value;
            }
        }
        self.bump_revision()
    }

    /// Add a rectangle sketch driven by two existing parameters.
    pub fn add_rectangle_sketch(
        &mut self,
        width_param: &str,
        height_param: &str,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.add_sketch(
            SketchProfile::Rectangle {
                width_param: width_param.to_string(),
                height_param: height_param.to_string(),
            },
            origin,
        )
    }

    /// Add a circle sketch driven by an existing diameter parameter.
    pub fn add_circle_sketch(
        &mut self,
        diameter_param: &str,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.add_sketch(
            SketchProfile::Circle {
                diameter_param: diameter_param.to_string(),
            },
            origin,
        )
    }

    /// Add a regular polygon driven by an existing diameter parameter.
    pub fn add_regular_polygon_sketch(
        &mut self,
        diameter_param: &str,
        sides: u32,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.add_sketch(
            SketchProfile::RegularPolygon {
                diameter_param: diameter_param.to_string(),
                sides,
            },
            origin,
        )
    }

    /// Add a closed polyline profile drawn point by point, relative to `origin`.
    ///
    /// The points are validated before the sketch is added, so an invalid loop never becomes part
    /// of the document. Winding does not matter: [`Project::sketch_profile_mm`] normalises it.
    pub fn add_polyline_sketch(
        &mut self,
        points: Vec<[f64; 2]>,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.add_sketch(SketchProfile::Polyline { points }, origin)
    }

    /// Add an arc-based sector driven by an existing diameter parameter.
    ///
    /// `start_degrees` and `sweep_degrees` are literal angles rather than parameters because the
    /// parameter map holds positive lengths, and a start angle of 0 is meaningful. The sweep is
    /// validated before the sketch is added.
    pub fn add_sector_sketch(
        &mut self,
        diameter_param: &str,
        start_degrees: f64,
        sweep_degrees: f64,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.add_sketch(
            SketchProfile::Sector {
                diameter_param: diameter_param.to_string(),
                start_degrees,
                sweep_degrees,
            },
            origin,
        )
    }

    /// Add a sketch with an arbitrary validated profile on the z = 0 plane.
    pub fn add_sketch(
        &mut self,
        profile: SketchProfile,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.add_sketch_on_plane(profile, origin, 0.0)
    }

    /// Add a sketch on a plane at `plane_z` document units above z = 0.
    ///
    /// The plane height is validated before the sketch is added, so a non-finite height never
    /// becomes part of the document.
    pub fn add_sketch_on_plane(
        &mut self,
        profile: SketchProfile,
        origin: [f64; 2],
        plane_z: f64,
    ) -> Result<EntityId, ProjectError> {
        self.validate_profile_references(&profile)?;
        if !origin[0].is_finite() || !origin[1].is_finite() {
            return Err(ProjectError::Invalid("sketch origin must be finite".into()));
        }
        validate_plane_z(plane_z)?;
        self.push_undo();
        let id = self.allocate_entity()?;
        self.sketches.push(Sketch {
            id,
            origin,
            plane_z,
            profile,
        });
        self.bump_revision()?;
        Ok(id)
    }

    pub fn sketch(&self, id: EntityId) -> Option<&Sketch> {
        self.sketches.iter().find(|sketch| sketch.id == id)
    }

    /// Replace a sketch's profile. Feature references to the sketch stay valid; the geometry they
    /// rebuild from changes. The old profile is recoverable with [`Project::undo`].
    pub fn set_sketch_profile(
        &mut self,
        sketch_id: EntityId,
        profile: SketchProfile,
    ) -> Result<(), ProjectError> {
        if !self.sketches.iter().any(|sketch| sketch.id == sketch_id) {
            return Err(ProjectError::UnknownSketch(sketch_id.get()));
        }
        self.validate_profile_references(&profile)?;
        self.push_undo();
        let sketch = self
            .sketches
            .iter_mut()
            .find(|sketch| sketch.id == sketch_id)
            .ok_or(ProjectError::UnknownSketch(sketch_id.get()))?;
        sketch.profile = profile;
        self.bump_revision()
    }

    /// Move a sketch in the XY plane. Geometry is rebuilt from the new origin.
    pub fn set_sketch_origin(
        &mut self,
        sketch_id: EntityId,
        origin: [f64; 2],
    ) -> Result<(), ProjectError> {
        let plane_z = self
            .sketch(sketch_id)
            .map(|sketch| sketch.plane_z)
            .ok_or(ProjectError::UnknownSketch(sketch_id.get()))?;
        self.set_sketch_placement(sketch_id, origin, plane_z)
    }

    /// Move a sketch to a new plane height, keeping its XY origin. Geometry is rebuilt from it.
    pub fn set_sketch_plane(
        &mut self,
        sketch_id: EntityId,
        plane_z: f64,
    ) -> Result<(), ProjectError> {
        let origin = self
            .sketch(sketch_id)
            .map(|sketch| sketch.origin)
            .ok_or(ProjectError::UnknownSketch(sketch_id.get()))?;
        self.set_sketch_placement(sketch_id, origin, plane_z)
    }

    /// Place a sketch in one step: an XY origin and a plane height, as one undoable edit.
    ///
    /// One transaction rather than two, so a single "place sketch" action is one undo away and a
    /// half-applied move cannot exist.
    pub fn set_sketch_placement(
        &mut self,
        sketch_id: EntityId,
        origin: [f64; 2],
        plane_z: f64,
    ) -> Result<(), ProjectError> {
        if !origin[0].is_finite() || !origin[1].is_finite() {
            return Err(ProjectError::Invalid("sketch origin must be finite".into()));
        }
        validate_plane_z(plane_z)?;
        if !self.sketches.iter().any(|sketch| sketch.id == sketch_id) {
            return Err(ProjectError::UnknownSketch(sketch_id.get()));
        }
        self.push_undo();
        let sketch = self
            .sketches
            .iter_mut()
            .find(|sketch| sketch.id == sketch_id)
            .ok_or(ProjectError::UnknownSketch(sketch_id.get()))?;
        sketch.origin = origin;
        sketch.plane_z = plane_z;
        self.bump_revision()
    }

    fn validate_profile_references(&self, profile: &SketchProfile) -> Result<(), ProjectError> {
        match profile {
            SketchProfile::Rectangle {
                width_param,
                height_param,
            } => {
                let _ = self.parameter(width_param)?;
                let _ = self.parameter(height_param)?;
            }
            SketchProfile::Circle { diameter_param } => {
                let _ = self.parameter(diameter_param)?;
            }
            SketchProfile::RegularPolygon {
                diameter_param,
                sides,
            } => {
                let _ = self.parameter(diameter_param)?;
                validate_sides(*sides)?;
            }
            SketchProfile::Polyline { points } => validate_polyline(points)?,
            SketchProfile::Sector {
                diameter_param,
                start_degrees,
                sweep_degrees,
            } => {
                let _ = self.parameter(diameter_param)?;
                validate_sector(*start_degrees, *sweep_degrees)?;
            }
        }
        Ok(())
    }

    /// Add an extrude of an existing sketch as the project's body.
    ///
    /// This is the schema-2 behaviour kept under its original name: it may only be called while the
    /// project has no features, because a second body is not supported.
    pub fn add_extrude(
        &mut self,
        sketch_id: EntityId,
        depth_param: &str,
    ) -> Result<EntityId, ProjectError> {
        self.add_extrude_feature(sketch_id, depth_param, ExtrudeOperation::NewBody)
    }

    /// Extrude an existing sketch and union it into the body built so far.
    pub fn add_join_extrude(
        &mut self,
        sketch_id: EntityId,
        depth_param: &str,
    ) -> Result<EntityId, ProjectError> {
        self.add_extrude_feature(sketch_id, depth_param, ExtrudeOperation::Join)
    }

    /// Extrude an existing sketch and remove it from the body built so far.
    pub fn add_cut_extrude(
        &mut self,
        sketch_id: EntityId,
        depth_param: &str,
    ) -> Result<EntityId, ProjectError> {
        self.add_extrude_feature(sketch_id, depth_param, ExtrudeOperation::Cut)
    }

    /// Append an extrude to the history.
    ///
    /// The new feature may only consume a sketch that already exists, which is also why the ID it
    /// receives is larger than that sketch's ID. The first feature must start the body; every later
    /// feature is either a join (union into the body) or a cut (difference of the extrusion out of it),
    /// so a document can never describe two bodies or a forward reference.
    pub fn add_extrude_feature(
        &mut self,
        sketch_id: EntityId,
        depth_param: &str,
        operation: ExtrudeOperation,
    ) -> Result<EntityId, ProjectError> {
        self.add_extrude_feature_in_direction(
            sketch_id,
            depth_param,
            operation,
            ExtrudeDirection::Up,
        )
    }

    /// Append an extrude with an explicit sweep direction.
    ///
    /// Everything `add_extrude_feature` documents applies; the only difference is which way the
    /// tool is swept from the sketch plane. `Down` is how a pocket is cut into a top face.
    pub fn add_extrude_feature_in_direction(
        &mut self,
        sketch_id: EntityId,
        depth_param: &str,
        operation: ExtrudeOperation,
        direction: ExtrudeDirection,
    ) -> Result<EntityId, ProjectError> {
        let _ = self.parameter(depth_param)?;
        self.append_feature(
            sketch_id,
            operation,
            FeatureKind::Extrude {
                depth_param: depth_param.to_string(),
                direction,
            },
        )
    }

    /// Revolve an existing sketch a full turn about one of its own axes as the project's body.
    pub fn add_revolve(
        &mut self,
        sketch_id: EntityId,
        axis: RevolveAxis,
    ) -> Result<EntityId, ProjectError> {
        self.add_revolve_feature(sketch_id, axis, ExtrudeOperation::NewBody)
    }

    /// Append a revolve with an explicit operation, as the API and UI do.
    pub fn add_revolve_feature(
        &mut self,
        sketch_id: EntityId,
        axis: RevolveAxis,
        operation: ExtrudeOperation,
    ) -> Result<EntityId, ProjectError> {
        self.append_feature(sketch_id, operation, FeatureKind::Revolve { axis })
    }

    /// Revolve an existing sketch and union it into the body built so far.
    pub fn add_join_revolve(
        &mut self,
        sketch_id: EntityId,
        axis: RevolveAxis,
    ) -> Result<EntityId, ProjectError> {
        self.add_revolve_feature(sketch_id, axis, ExtrudeOperation::Join)
    }

    /// Revolve an existing sketch and remove it from the body built so far.
    pub fn add_cut_revolve(
        &mut self,
        sketch_id: EntityId,
        axis: RevolveAxis,
    ) -> Result<EntityId, ProjectError> {
        self.add_revolve_feature(sketch_id, axis, ExtrudeOperation::Cut)
    }

    /// Shared validation and append for every feature kind.
    fn append_feature(
        &mut self,
        sketch_id: EntityId,
        operation: ExtrudeOperation,
        kind: FeatureKind,
    ) -> Result<EntityId, ProjectError> {
        if !self.sketches.iter().any(|sketch| sketch.id == sketch_id) {
            return Err(ProjectError::UnknownSketch(sketch_id.get()));
        }
        match operation {
            ExtrudeOperation::NewBody if !self.features.is_empty() => {
                return Err(ProjectError::Invalid(
                    "this project already has a body; add this feature as a join instead".into(),
                ));
            }
            ExtrudeOperation::Join if self.features.is_empty() => {
                return Err(ProjectError::Invalid(
                    "a join needs an existing body; the first feature must create one".into(),
                ));
            }
            ExtrudeOperation::Cut if self.features.is_empty() => {
                return Err(ProjectError::Invalid(
                    "a cut needs an existing body; the first feature must create one".into(),
                ));
            }
            _ => {}
        }
        self.push_undo();
        let id = self.allocate_entity()?;
        self.features.push(Feature {
            id,
            sketch_id,
            operation,
            kind,
        });
        self.bump_revision()?;
        Ok(id)
    }

    pub fn undo(&mut self) -> Result<(), ProjectError> {
        let previous = self.undo.pop().ok_or(ProjectError::NothingToUndo)?;
        self.redo.push(self.snapshot());
        self.restore(previous);
        self.bump_revision()
    }

    pub fn redo(&mut self) -> Result<(), ProjectError> {
        let next = self.redo.pop().ok_or(ProjectError::NothingToRedo)?;
        self.undo.push(self.snapshot());
        self.restore(next);
        self.bump_revision()
    }

    /// Number of polygon segments this sketch evaluates to.
    ///
    /// A rectangle is four, a regular polygon is its side count, a polyline is its point count, a
    /// sector is its arc's facet count plus the two radii, and a circle follows the document's
    /// linear tolerance: `n = ceil(2π / (2·acos(1 − tol / r)))`, bounded to 24..=256.
    pub fn sketch_segment_count(&self, sketch: &Sketch) -> Result<usize, ProjectError> {
        match &sketch.profile {
            SketchProfile::Rectangle { .. } => Ok(4),
            SketchProfile::RegularPolygon { sides, .. } => Ok(*sides as usize),
            SketchProfile::Polyline { points } => Ok(points.len()),
            SketchProfile::Sector {
                diameter_param,
                start_degrees,
                sweep_degrees,
            } => {
                validate_sector(*start_degrees, *sweep_degrees)?;
                let diameter = self
                    .header
                    .units
                    .to_mm(self.parameter(diameter_param)?)
                    .abs();
                Ok(arc_segments(
                    diameter * 0.5,
                    self.header.tolerance.linear_mm(),
                    *sweep_degrees,
                ) + 2)
            }
            SketchProfile::Circle { diameter_param } => {
                let diameter = self
                    .header
                    .units
                    .to_mm(self.parameter(diameter_param)?)
                    .abs();
                Ok(circle_segments(
                    diameter * 0.5,
                    self.header.tolerance.linear_mm(),
                ))
            }
        }
    }

    /// Closed profile in millimetres, counter-clockwise, in the sketch's own XY plane.
    pub fn sketch_profile_mm(&self, sketch: &Sketch) -> Result<Vec<[f64; 2]>, ProjectError> {
        let unit = self.header.units;
        let ox = unit.to_mm(sketch.origin[0]);
        let oy = unit.to_mm(sketch.origin[1]);
        let profile = match &sketch.profile {
            SketchProfile::Rectangle {
                width_param,
                height_param,
            } => {
                let width = unit.to_mm(self.parameter(width_param)?);
                let height = unit.to_mm(self.parameter(height_param)?);
                vec![
                    [ox, oy],
                    [ox + width, oy],
                    [ox + width, oy + height],
                    [ox, oy + height],
                ]
            }
            SketchProfile::Circle { diameter_param } => {
                let radius = unit.to_mm(self.parameter(diameter_param)?) * 0.5;
                let segments = circle_segments(radius, self.header.tolerance.linear_mm());
                (0..segments)
                    .map(|index| {
                        let angle = std::f64::consts::TAU * index as f64 / segments as f64;
                        [ox + radius * angle.cos(), oy + radius * angle.sin()]
                    })
                    .collect()
            }
            SketchProfile::RegularPolygon {
                diameter_param,
                sides,
            } => {
                validate_sides(*sides)?;
                let radius = unit.to_mm(self.parameter(diameter_param)?) * 0.5;
                // Start half a segment off the top so a square sits square to the axes.
                let start = -std::f64::consts::FRAC_PI_2 + std::f64::consts::PI / f64::from(*sides);
                (0..*sides)
                    .map(|index| {
                        let angle =
                            start + std::f64::consts::TAU * f64::from(index) / f64::from(*sides);
                        [ox + radius * angle.cos(), oy + radius * angle.sin()]
                    })
                    .collect()
            }
            SketchProfile::Polyline { points } => {
                validate_polyline(points)?;
                let mut profile: Vec<[f64; 2]> = points
                    .iter()
                    .map(|point| [ox + unit.to_mm(point[0]), oy + unit.to_mm(point[1])])
                    .collect();
                // A drawn loop may be clockwise; the kernel must receive a counter-clockwise
                // profile so the solid is outward-oriented and its volume positive.
                if signed_area(&profile) < 0.0 {
                    profile.reverse();
                }
                profile
            }
            SketchProfile::Sector {
                diameter_param,
                start_degrees,
                sweep_degrees,
            } => {
                validate_sector(*start_degrees, *sweep_degrees)?;
                let radius = unit.to_mm(self.parameter(diameter_param)?).abs() * 0.5;
                let segments =
                    arc_segments(radius, self.header.tolerance.linear_mm(), *sweep_degrees);
                // Centre first, then the arc counter-clockwise: centre -> arc start -> arc end ->
                // centre is a positively wound loop, so no reversal is needed.
                let start = start_degrees.rem_euclid(360.0).to_radians();
                let sweep = sweep_degrees.to_radians();
                let mut profile = Vec::with_capacity(segments + 2);
                profile.push([ox, oy]);
                for index in 0..=segments {
                    let angle = start + sweep * index as f64 / segments as f64;
                    profile.push([ox + radius * angle.cos(), oy + radius * angle.sin()]);
                }
                profile
            }
        };
        validate_profile(&profile).map_err(|error| ProjectError::Invalid(error.to_string()))?;
        Ok(profile)
    }

    /// Rebuild every feature in order, stopping at the first failure.
    ///
    /// A kernel failure is reported in the result rather than returned as an error, because a
    /// document that cannot rebuild still belongs to the user: the caller needs the failing
    /// feature's ID and message, and the mesh of the last feature that did rebuild. Only a
    /// structurally invalid document is an error here.
    ///
    /// Solids superseded by a join are released only after the replacement has tessellated, so a
    /// failure at any step leaves the previous body and its mesh intact.
    pub fn build<K: Kernel>(&self, kernel: &mut K) -> Result<Build, ProjectError> {
        self.validate()?;
        let mut build = Build::default();
        let tolerance_mm = self.header.tolerance.linear_mm();
        let mut body: Option<SolidId> = None;

        for feature in &self.features {
            let outcome = match self.resolve_feature(feature) {
                Ok((profile, action)) => self.rebuild_feature(
                    feature,
                    &profile,
                    &action,
                    kernel,
                    tolerance_mm,
                    body,
                    true,
                ),
                Err(message) => Err(message),
            };
            match outcome {
                Ok(rebuilt) => {
                    build.reports.push(FeatureReport {
                        id: feature.id,
                        outcome: FeatureOutcome::Built {
                            volume_mm3: rebuilt.volume_mm3,
                            triangles: rebuilt.triangles,
                        },
                    });
                    body = Some(rebuilt.solid);
                    build.mesh = Some(rebuilt.mesh);
                }
                Err(message) => {
                    build.reports.push(FeatureReport {
                        id: feature.id,
                        outcome: FeatureOutcome::Failed {
                            message: message.clone(),
                        },
                    });
                    build.failed = Some(FeatureFailure {
                        id: feature.id,
                        message,
                    });
                    break;
                }
            }
        }

        build.solid = body;
        Ok(build)
    }

    /// Rebuild every feature in order, reusing [`BuildCache`] entries for prefixes already built.
    ///
    /// The observable result is identical to [`Project::build`] — same reports, same mesh, same
    /// failure — but features whose inputs are unchanged are not sent to the kernel again. The
    /// returned [`Build::solid`] is owned by `cache`, so the caller must not release it; call
    /// [`BuildCache::clear`] instead when the kernel is discarded.
    pub fn build_with_cache<K: Kernel>(
        &self,
        kernel: &mut K,
        cache: &mut BuildCache,
    ) -> Result<Build, ProjectError> {
        self.validate()?;
        let identity = kernel_identity(kernel);
        if cache.identity != Some(identity) {
            // Different kernel: its handles mean nothing here, so start clean rather than risk
            // adopting a handle that kernel never issued.
            cache.clear(kernel);
            cache.identity = Some(identity);
        } else if cache
            .entries
            .iter()
            .any(|entry| kernel.contains(entry.solid) == Some(false))
        {
            // Same kernel type, different instance: the identity matches but the handles do not
            // exist here. Clearing is cheap; adopting them would fail deep inside a later feature.
            cache.clear(kernel);
            cache.identity = Some(identity);
        }
        let tolerance_mm = self.header.tolerance.linear_mm();
        let mut build = Build::default();
        let mut body: Option<SolidId> = None;
        let mut prefix = HASH_SEED;
        let mut chain: Vec<u64> = Vec::with_capacity(self.features.len());

        for feature in &self.features {
            let (profile, action) = match self.resolve_feature(feature) {
                Ok(resolved) => resolved,
                Err(message) => {
                    build.reports.push(FeatureReport {
                        id: feature.id,
                        outcome: FeatureOutcome::Failed {
                            message: message.clone(),
                        },
                    });
                    build.failed = Some(FeatureFailure {
                        id: feature.id,
                        message,
                    });
                    break;
                }
            };
            prefix = prefix_hash(prefix, feature, &profile, &action, tolerance_mm);
            chain.push(prefix);

            if let Some(entry) = cache.entries.iter().find(|entry| entry.prefix == prefix) {
                cache.hits = cache.hits.saturating_add(1);
                body = Some(entry.solid);
                build.mesh = Some(entry.mesh.clone());
                build.reports.push(FeatureReport {
                    id: feature.id,
                    outcome: FeatureOutcome::Built {
                        volume_mm3: entry.volume_mm3,
                        triangles: entry.triangles,
                    },
                });
                continue;
            }

            cache.misses = cache.misses.saturating_add(1);
            // Keep the superseded body: an earlier prefix still refers to it, and the cache owns
            // every handle it hands out.
            match self.rebuild_feature(
                feature,
                &profile,
                &action,
                kernel,
                tolerance_mm,
                body,
                false,
            ) {
                Ok(rebuilt) => {
                    cache.entries.push(CacheEntry {
                        prefix,
                        solid: rebuilt.solid,
                        mesh: rebuilt.mesh.clone(),
                        volume_mm3: rebuilt.volume_mm3,
                        triangles: rebuilt.triangles,
                    });
                    build.reports.push(FeatureReport {
                        id: feature.id,
                        outcome: FeatureOutcome::Built {
                            volume_mm3: rebuilt.volume_mm3,
                            triangles: rebuilt.triangles,
                        },
                    });
                    body = Some(rebuilt.solid);
                    build.mesh = Some(rebuilt.mesh);
                }
                Err(message) => {
                    build.reports.push(FeatureReport {
                        id: feature.id,
                        outcome: FeatureOutcome::Failed {
                            message: message.clone(),
                        },
                    });
                    build.failed = Some(FeatureFailure {
                        id: feature.id,
                        message,
                    });
                    break;
                }
            }
        }

        // Keep the prefixes of the history that just rebuilt — newest last, because the newest
        // prefix holds the body — and then spend any capacity left on entries from earlier
        // revisions, which is what makes undo cheap: undoing to a state that was already built
        // finds its prefixes still cached. Only when the cap is exceeded is a solid released.
        //
        // The current chain is kept explicitly in chain order, never by `truncate` alone, so the
        // final prefix (and therefore `build.solid`) can never be the entry that is dropped.
        let first_kept = chain.len().saturating_sub(MAX_CACHE_ENTRIES);
        let mut kept: Vec<CacheEntry> = Vec::new();
        for prefix in chain.into_iter().skip(first_kept) {
            if let Some(position) = cache
                .entries
                .iter()
                .position(|entry| entry.prefix == prefix)
            {
                kept.push(cache.entries.swap_remove(position));
            }
        }
        // `cache.entries` now holds only prefixes from other revisions. Within the remaining
        // capacity they all survive; beyond it, release them.
        for entry in cache.entries.drain(..) {
            if kept.len() < MAX_CACHE_ENTRIES {
                kept.push(entry);
            } else {
                kernel.release(entry.solid);
            }
        }
        cache.entries = kept;

        build.solid = body;
        Ok(build)
    }

    /// The resolved inputs of one feature: its profile in millimetres and what to do with it.
    ///
    /// Resolving is separate from building so the cache can hash exactly what the kernel will be
    /// given. The error strings are the ones a failing feature reports, so an uncached and a cached
    /// rebuild fail with the same message.
    fn resolve_feature(
        &self,
        feature: &Feature,
    ) -> Result<(Vec<[f64; 2]>, ResolvedAction), String> {
        let sketch = self
            .sketch(feature.sketch_id)
            .ok_or_else(|| format!("sketch {} does not exist", feature.sketch_id))?;
        let profile = self
            .sketch_profile_mm(sketch)
            .map_err(|error| format!("sketch {} is not a usable profile: {error}", sketch.id))?;
        let unit = self.header.units;
        // The profile is in millimetres and already offset by the sketch origin, so the axis must be
        // in the same frame: the sketch's local axis line placed at the origin.
        let ox = unit.to_mm(sketch.origin[0]);
        let oy = unit.to_mm(sketch.origin[1]);
        // The sketch plane, in millimetres. The profile itself is resolved in the plane's own 2D
        // coordinates, so the height is applied to the tool afterwards rather than to the points.
        let plane_mm = unit.to_mm(sketch.plane_z);
        let action = match &feature.kind {
            FeatureKind::Extrude {
                depth_param,
                direction,
            } => ResolvedAction::Extrude {
                depth_mm: unit.to_mm(
                    self.parameter(depth_param)
                        .map_err(|error| error.to_string())?,
                ) * direction.sign(),
                plane_mm,
            },
            FeatureKind::Revolve { axis } => {
                let (origin, direction) = axis.line();
                ResolvedAction::Revolve {
                    axis_origin_mm: [ox + unit.to_mm(origin[0]), oy + unit.to_mm(origin[1])],
                    axis_direction_mm: direction,
                    plane_mm,
                }
            }
        };
        Ok((profile, action))
    }

    /// Rebuild one feature from already-resolved inputs, returning the body it produced.
    ///
    /// `release_superseded` is the difference between the two build paths: an uncached rebuild owns
    /// the previous body and frees it, while a cached one keeps it because a cache entry still
    /// refers to it.
    #[allow(clippy::too_many_arguments)]
    fn rebuild_feature<K: Kernel>(
        &self,
        feature: &Feature,
        profile: &[[f64; 2]],
        action: &ResolvedAction,
        kernel: &mut K,
        tolerance_mm: f64,
        body: Option<SolidId>,
        release_superseded: bool,
    ) -> Result<RebuiltFeature, String> {
        let (swept, plane_mm) = match action {
            ResolvedAction::Extrude { depth_mm, plane_mm } => (
                kernel
                    .extrude(profile, *depth_mm)
                    .map_err(|error| format!("extrude failed: {error}"))?,
                *plane_mm,
            ),
            ResolvedAction::Revolve {
                axis_origin_mm,
                axis_direction_mm,
                plane_mm,
            } => {
                if !kernel.supports(cad_contract::capability::Capability::Revolve) {
                    return Err(
                        "the kernel does not advertise revolve, so this feature cannot be built"
                            .into(),
                    );
                }
                let turned = kernel
                    .revolve(
                        profile,
                        *axis_origin_mm,
                        *axis_direction_mm,
                        std::f64::consts::TAU,
                    )
                    .map_err(|error| format!("revolve failed: {error}"))?;
                (turned, *plane_mm)
            }
        };
        // Both kinds are built on the world plane and then moved, because every kernel sweep starts
        // at z = 0 and the contract has no plane argument.
        let swept = place_on_plane(kernel, swept, plane_mm)?;

        let (next, superseded): (SolidId, Option<SolidId>) = match (feature.operation, body) {
            (ExtrudeOperation::NewBody, None) => (swept, None),
            (ExtrudeOperation::NewBody, Some(_)) => {
                kernel.release(swept);
                return Err(
                    "a second new body is not supported; later features must join the first".into(),
                );
            }
            (ExtrudeOperation::Join, None) => {
                kernel.release(swept);
                return Err("a join needs a body from an earlier feature".into());
            }
            (ExtrudeOperation::Join, Some(running)) => {
                if !kernel.supports(cad_contract::capability::Capability::BooleanUnion) {
                    kernel.release(swept);
                    return Err(
                        "the kernel does not advertise boolean union, so this join cannot be built"
                            .into(),
                    );
                }
                // A boss that sits on a face of the body shares that face with it, which is the
                // coplanar case the kernel handles worst. The tool is moved into the body by
                // JOIN_OVERLAP_MM instead, where the extra material is already present. A revolved
                // tool is used as drawn; if it is coplanar the failure is reported, not hidden.
                let tool = match action {
                    ResolvedAction::Extrude { depth_mm, plane_mm } => {
                        match overlapping_join_tool(kernel, profile, *depth_mm, *plane_mm, running)
                        {
                            Some(tool) => {
                                kernel.release(swept);
                                tool
                            }
                            None => swept,
                        }
                    }
                    ResolvedAction::Revolve { .. } => swept,
                };
                match kernel.boolean(cad_contract::kernel::BooleanOp::Union, running, tool) {
                    Ok(joined) => {
                        kernel.release(tool);
                        (joined, Some(running))
                    }
                    Err(error) => {
                        kernel.release(tool);
                        return Err(format!("join failed: {error}"));
                    }
                }
            }
            (ExtrudeOperation::Cut, None) => {
                kernel.release(swept);
                return Err("a cut needs a body from an earlier feature".into());
            }
            (ExtrudeOperation::Cut, Some(running)) => {
                if !kernel.supports(cad_contract::capability::Capability::BooleanDifference) {
                    kernel.release(swept);
                    return Err(
                        "the kernel does not advertise boolean difference, so this cut cannot be built"
                            .into(),
                    );
                }
                // An *extruded* cut tool is made to overshoot the body, so no face of the tool is
                // coincident with a face of the body. Coincident faces are the boolean case the
                // kernel handles worst, and they are the common case: every extrude starts on the
                // z = 0 sketch plane, so a cut whose depth equals the body's thickness would
                // otherwise share both of its faces with the body. See CUT_OVERSHOOT_MM.
                //
                // A revolved tool has no such overshoot: its extent is the sketch, so it is used as
                // drawn and may hit the kernel's coplanar limitation. That is reported by the
                // feature if it happens rather than hidden.
                let tool = match action {
                    ResolvedAction::Extrude { depth_mm, plane_mm } => {
                        match overshooting_tool(kernel, profile, *depth_mm, *plane_mm, running) {
                            Some(tool) => {
                                kernel.release(swept);
                                tool
                            }
                            // No coincidence to break, or the kernel cannot move a solid: cut with
                            // the tool exactly as drawn.
                            None => swept,
                        }
                    }
                    ResolvedAction::Revolve { .. } => swept,
                };
                match kernel.boolean(cad_contract::kernel::BooleanOp::Difference, running, tool) {
                    Ok(cut) => {
                        kernel.release(tool);
                        (cut, Some(running))
                    }
                    Err(error) => {
                        kernel.release(tool);
                        return Err(format!("cut failed: {error}"));
                    }
                }
            }
        };

        let rebuilt = match kernel.tessellate(next, tolerance_mm) {
            Ok(rebuilt) => rebuilt,
            Err(error) => {
                kernel.release(next);
                return Err(format!("tessellation failed: {error}"));
            }
        };
        if let Err(error) = rebuilt.validate() {
            kernel.release(next);
            return Err(format!("the kernel produced an invalid mesh: {error}"));
        }
        if let Some(previous) = superseded
            && release_superseded
        {
            kernel.release(previous);
        }
        Ok(RebuiltFeature {
            solid: next,
            volume_mm3: rebuilt.signed_volume(),
            triangles: rebuilt.triangle_count(),
            mesh: rebuilt,
        })
    }

    /// Strict rebuild: every feature must succeed and the project must contain geometry.
    ///
    /// Prefer [`Project::build`] when the caller has to show a failed feature to the user.
    pub fn evaluate<K: Kernel>(&self, kernel: &mut K) -> Result<Evaluated, ProjectError> {
        if self.features.is_empty() {
            return Err(ProjectError::Invalid(
                "project has no features to evaluate".into(),
            ));
        }
        let build = self.build(kernel)?;
        if let Some(failure) = build.failed {
            if let Some(solid) = build.solid {
                kernel.release(solid);
            }
            return Err(ProjectError::FeatureFailed {
                feature: failure.id.get(),
                message: failure.message,
            });
        }
        let mesh = build
            .mesh
            .ok_or_else(|| ProjectError::Invalid("rebuild produced no mesh".into()))?;
        let solid = build
            .solid
            .ok_or_else(|| ProjectError::Invalid("rebuild produced no solid".into()))?;
        Ok(Evaluated {
            volume_mm3: mesh.signed_volume(),
            triangles: mesh.triangle_count(),
            mesh,
            solid,
        })
    }

    pub fn to_json(&self) -> Result<String, ProjectError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| ProjectError::Invalid(format!("cannot serialise project: {error}")))
    }

    pub fn from_json(bytes: &[u8]) -> Result<Self, ProjectError> {
        validate_project_size(bytes.len() as u64)?;
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| ProjectError::Invalid(format!("not valid JSON: {error}")))?;
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| ProjectError::Invalid("missing `schema`".into()))?;
        let schema = u32::try_from(schema).unwrap_or(u32::MAX);
        let value = match schema {
            1 => migrate_v7_to_v8(migrate_v6_to_v7(migrate_v5_to_v6(migrate_v4_to_v5(
                migrate_v3_to_v4(migrate_v2_to_v3(migrate_v1_to_v2(value)?)?)?,
            )?)?)?)?,
            2 => migrate_v7_to_v8(migrate_v6_to_v7(migrate_v5_to_v6(migrate_v4_to_v5(
                migrate_v3_to_v4(migrate_v2_to_v3(value)?)?,
            )?)?)?)?,
            3 => migrate_v7_to_v8(migrate_v6_to_v7(migrate_v5_to_v6(migrate_v4_to_v5(
                migrate_v3_to_v4(value)?,
            )?)?)?)?,
            4 => migrate_v7_to_v8(migrate_v6_to_v7(migrate_v5_to_v6(migrate_v4_to_v5(
                value,
            )?)?)?)?,
            5 => migrate_v7_to_v8(migrate_v6_to_v7(migrate_v5_to_v6(value)?)?)?,
            6 => migrate_v7_to_v8(migrate_v6_to_v7(value)?)?,
            7 => migrate_v7_to_v8(value)?,
            PROJECT_SCHEMA => value,
            _ => {
                return Err(ProjectError::UnsupportedSchema {
                    found: schema,
                    supported: PROJECT_SCHEMA,
                });
            }
        };
        let project: Project = serde_json::from_value(value)
            .map_err(|error| ProjectError::Invalid(format!("project JSON: {error}")))?;
        project.validate()?;
        Ok(project)
    }

    /// Check serialized document invariants before the project is saved or evaluated.
    pub fn validate(&self) -> Result<(), ProjectError> {
        if self.schema != PROJECT_SCHEMA {
            return Err(ProjectError::UnsupportedSchema {
                found: self.schema,
                supported: PROJECT_SCHEMA,
            });
        }
        if self.header.schema != cad_contract::document::CURRENT_SCHEMA {
            return Err(ProjectError::Invalid(
                "embedded document header is not at the current schema".into(),
            ));
        }
        if Revision::from_raw(self.revision.get()).is_none() {
            return Err(ProjectError::Invalid("revision must be positive".into()));
        }
        let _ = Tolerance::new(
            self.header.tolerance.linear_mm(),
            self.header.tolerance.angular_rad(),
        )
        .map_err(|error| ProjectError::Invalid(error.to_string()))?;
        if self.name.trim().is_empty() {
            return Err(ProjectError::Invalid(
                "project name must not be empty".into(),
            ));
        }
        if self.undo.len() > MAX_UNDO || self.redo.len() > MAX_UNDO {
            return Err(ProjectError::Invalid(format!(
                "project history exceeds the {MAX_UNDO}-snapshot limit"
            )));
        }
        validate_contents(
            &self.parameters,
            &self.sketches,
            &self.features,
            self.next_entity,
        )?;
        for snapshot in self.undo.iter().chain(self.redo.iter()) {
            validate_contents(
                &snapshot.parameters,
                &snapshot.sketches,
                &snapshot.features,
                snapshot.next_entity,
            )?;
        }
        if self.revisions.len() > MAX_REVISIONS {
            return Err(ProjectError::Invalid(format!(
                "project holds more than {MAX_REVISIONS} named revisions"
            )));
        }
        if self.next_revision == 0 {
            return Err(ProjectError::Invalid(
                "next revision id must be positive".into(),
            ));
        }
        let mut revision_names = std::collections::BTreeSet::new();
        for revision in &self.revisions {
            if revision.id == 0 || revision.id >= self.next_revision {
                return Err(ProjectError::Invalid(format!(
                    "named revision {} is outside the allocated revision ids",
                    revision.id
                )));
            }
            if revision.name.trim().is_empty()
                || revision.name.chars().count() > MAX_REVISION_NAME_CHARS
            {
                return Err(ProjectError::Invalid(format!(
                    "named revision {} has an empty or over-long name",
                    revision.id
                )));
            }
            if !revision_names.insert(revision.name.to_ascii_lowercase()) {
                return Err(ProjectError::Invalid(format!(
                    "revision name `{}` is used twice",
                    revision.name
                )));
            }
            if revision.revision == 0 {
                return Err(ProjectError::Invalid(format!(
                    "named revision {} records a zero document revision",
                    revision.id
                )));
            }
            // A revision can be restored, so its copy has to satisfy exactly the rules the live
            // history does. A file that smuggled an invalid one in must not open.
            validate_contents(
                &revision.snapshot.parameters,
                &revision.snapshot.sketches,
                &revision.snapshot.features,
                revision.snapshot.next_entity,
            )?;
        }
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        self.validate()?;
        let json = self.to_json()?;
        // Keep the previous file as `<path>.backup` before replacing it, so a file that is later
        // truncated by a full disk or edited by hand can still be recovered without a snapshot.
        // Best effort on purpose: the atomic replace below is what protects this save, and a
        // read-only or full directory must not turn a successful save into a failed one.
        let _ = refresh_backup(path);
        atomic_replace(path, json.as_bytes())
    }

    /// Write the unsaved-edit snapshot that makes a crash recoverable.
    ///
    /// The snapshot is a separate file (`<path>.recovery`), so an autosave never writes the project
    /// itself: saving stays an explicit user action. A snapshot that cannot be parsed or validated
    /// is ignored by [`open_with_recovery`] rather than replacing the project.
    ///
    /// Returns the snapshot's timestamp, in seconds since the Unix epoch, so a caller can report
    /// when the recovered edit was made.
    pub fn save_recovery(&self, path: &Path) -> Result<u64, ProjectError> {
        self.validate()?;
        let saved_at_unix = unix_now();
        let file = RecoveryFile {
            schema: RECOVERY_SCHEMA,
            saved_at_unix,
            // Which version of the project file this snapshot was taken from. Comparing this with
            // the file's current mtime on open is exact: no clock skew and no same-second race
            // between a save and the next edit.
            base_mtime_unix: last_write_unix(path),
            project: self.clone(),
        };
        let bytes = serde_json::to_vec(&file).map_err(|error| {
            ProjectError::Invalid(format!("cannot serialise recovery snapshot: {error}"))
        })?;
        validate_project_size(bytes.len() as u64)?;
        atomic_replace(&recovery_path(path), &bytes)?;
        Ok(saved_at_unix)
    }

    pub fn load(path: &Path) -> Result<Self, ProjectError> {
        let metadata = std::fs::metadata(path).map_err(|source| ProjectError::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        })?;
        validate_project_size(metadata.len())?;
        let file = std::fs::File::open(path).map_err(|source| ProjectError::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        })?;
        let mut bytes = Vec::new();
        file.take(MAX_PROJECT_BYTES + 1)
            .read_to_end(&mut bytes)
            .map_err(|source| ProjectError::Io {
                path: path.display().to_string(),
                message: source.to_string(),
            })?;
        validate_project_size(bytes.len() as u64)?;
        Self::from_json(&bytes)
    }
}

fn validate_contents(
    parameters: &[Parameter],
    sketches: &[Sketch],
    features: &[Feature],
    next_entity: u64,
) -> Result<(), ProjectError> {
    let mut parameter_ids = std::collections::BTreeSet::new();
    for parameter in parameters {
        if parameter.id.trim().is_empty() || !parameter_ids.insert(parameter.id.as_str()) {
            return Err(ProjectError::Invalid(format!(
                "parameter id `{}` is empty or duplicated",
                parameter.id
            )));
        }
        if !parameter.value.is_finite() || parameter.value <= 0.0 {
            return Err(ProjectError::Invalid(format!(
                "parameter `{}` must be a finite positive number",
                parameter.id
            )));
        }
    }

    let mut entity_ids = std::collections::BTreeSet::new();
    let mut max_id = 0;
    for sketch in sketches {
        if !entity_ids.insert(sketch.id) {
            return Err(ProjectError::Invalid(format!(
                "entity id {} is duplicated",
                sketch.id
            )));
        }
        max_id = max_id.max(sketch.id.get());
        match &sketch.profile {
            SketchProfile::Rectangle {
                width_param,
                height_param,
            } => {
                if !parameter_ids.contains(width_param.as_str())
                    || !parameter_ids.contains(height_param.as_str())
                {
                    return Err(ProjectError::Invalid(format!(
                        "sketch {} references a missing parameter",
                        sketch.id
                    )));
                }
            }
            SketchProfile::Circle { diameter_param } => {
                if !parameter_ids.contains(diameter_param.as_str()) {
                    return Err(ProjectError::Invalid(format!(
                        "sketch {} references a missing parameter",
                        sketch.id
                    )));
                }
            }
            SketchProfile::RegularPolygon {
                diameter_param,
                sides,
            } => {
                if !parameter_ids.contains(diameter_param.as_str()) {
                    return Err(ProjectError::Invalid(format!(
                        "sketch {} references a missing parameter",
                        sketch.id
                    )));
                }
                validate_sides(*sides)?;
            }
            SketchProfile::Polyline { points } => validate_polyline(points)?,
            SketchProfile::Sector {
                diameter_param,
                start_degrees,
                sweep_degrees,
            } => {
                if !parameter_ids.contains(diameter_param.as_str()) {
                    return Err(ProjectError::Invalid(format!(
                        "sketch {} references a missing parameter",
                        sketch.id
                    )));
                }
                validate_sector(*start_degrees, *sweep_degrees)?;
            }
        }
        if sketch
            .origin
            .iter()
            .any(|coordinate| !coordinate.is_finite())
        {
            return Err(ProjectError::Invalid(format!(
                "sketch {} origin must be finite",
                sketch.id
            )));
        }
        if !sketch.plane_z.is_finite() {
            return Err(ProjectError::Invalid(format!(
                "sketch {} plane height must be finite",
                sketch.id
            )));
        }
    }
    for (index, feature) in features.iter().enumerate() {
        if !entity_ids.insert(feature.id) {
            return Err(ProjectError::Invalid(format!(
                "entity id {} is duplicated",
                feature.id
            )));
        }
        max_id = max_id.max(feature.id.get());
        // The list is the build order, so a document that is not sorted has no defined history.
        if index > 0 && feature.id.get() <= features[index - 1].id.get() {
            return Err(ProjectError::Invalid(format!(
                "feature {} is not stored after feature {}; the feature list must be in build order",
                feature.id,
                features[index - 1].id
            )));
        }
        if index == 0 {
            if feature.operation != ExtrudeOperation::NewBody {
                return Err(ProjectError::Invalid(format!(
                    "feature {} must start a new body; a join or cut needs an earlier body",
                    feature.id
                )));
            }
        } else if feature.operation != ExtrudeOperation::Join
            && feature.operation != ExtrudeOperation::Cut
        {
            return Err(ProjectError::Invalid(format!(
                "feature {} starts a second body, which this document does not support; use a join or a cut",
                feature.id
            )));
        }
        let sketch = sketches
            .iter()
            .find(|sketch| sketch.id == feature.sketch_id)
            .ok_or(ProjectError::UnknownSketch(feature.sketch_id.get()))?;
        // Entity ids are allocated in creation order, so this is the persisted proof that the
        // feature consumes geometry that existed before it. It is what makes a dependency cycle
        // unrepresentable instead of merely improbable.
        if sketch.id.get() >= feature.id.get() {
            return Err(ProjectError::Invalid(format!(
                "feature {} references sketch {}, which was created after it; a feature may only use \
                 earlier geometry",
                feature.id, sketch.id
            )));
        }
        if let FeatureKind::Extrude { depth_param, .. } = &feature.kind
            && !parameter_ids.contains(depth_param.as_str())
        {
            return Err(ProjectError::UnknownParameter(depth_param.clone()));
        }
    }
    if next_entity <= max_id {
        return Err(ProjectError::Invalid(
            "next entity id must be greater than every stored entity id".into(),
        ));
    }
    Ok(())
}

/// Check a drawn loop: bounded point count, finite, no repeated consecutive point, non-zero area.
///
/// The structural rules are exactly the contract's profile rules, applied here so a bad loop is
/// refused when it is created (and when a file is loaded) rather than only when it is extruded.
fn validate_polyline(points: &[[f64; 2]]) -> Result<(), ProjectError> {
    if points.len() < MIN_POLYLINE_POINTS || points.len() > MAX_POLYLINE_POINTS {
        return Err(ProjectError::Invalid(format!(
            "a polyline needs between {MIN_POLYLINE_POINTS} and {MAX_POLYLINE_POINTS} points, got {}",
            points.len()
        )));
    }
    validate_profile(points).map_err(|error| ProjectError::Invalid(error.to_string()))
}

/// Twice the signed area of a closed loop (shoelace); positive means counter-clockwise.
fn signed_area(points: &[[f64; 2]]) -> f64 {
    let mut twice = 0.0;
    for (index, point) in points.iter().enumerate() {
        let next = points[(index + 1) % points.len()];
        twice += point[0] * next[1] - next[0] * point[1];
    }
    twice * 0.5
}

/// Check a sector's literal angles.
///
/// The sweep is the one input that can describe something the document cannot build: 0° encloses
/// no area, and 360° makes the two radii coincide, which is a disc rather than a sector. Both are
/// refused with a message that names the alternative instead of failing inside `validate_profile`.
fn validate_sector(start_degrees: f64, sweep_degrees: f64) -> Result<(), ProjectError> {
    if !start_degrees.is_finite() {
        return Err(ProjectError::Invalid(
            "a sector start angle must be a finite number of degrees".into(),
        ));
    }
    if !sweep_degrees.is_finite() || sweep_degrees <= 0.0 || sweep_degrees >= 360.0 {
        return Err(ProjectError::Invalid(format!(
            "a sector sweep must be between 0 and 360 degrees, exclusive; got {sweep_degrees}. A \
             full turn is a circle profile"
        )));
    }
    Ok(())
}

fn validate_sides(sides: u32) -> Result<(), ProjectError> {
    if (MIN_POLYGON_SIDES..=MAX_POLYGON_SIDES).contains(&sides) {
        Ok(())
    } else {
        Err(ProjectError::Invalid(format!(
            "polygon sides must be between {MIN_POLYGON_SIDES} and {MAX_POLYGON_SIDES}, got {sides}"
        )))
    }
}

/// Segment count for a circle at a given chord tolerance, bounded so a pathological tolerance can
/// neither collapse the circle nor ask for a million facets.
fn circle_segments(radius_mm: f64, tolerance_mm: f64) -> usize {
    if !radius_mm.is_finite() || radius_mm <= 0.0 {
        return MIN_CIRCLE_SEGMENTS;
    }
    let tolerance = if tolerance_mm.is_finite() && tolerance_mm > 0.0 {
        tolerance_mm
    } else {
        1e-6
    };
    // Sagitta of a chord: tol = r * (1 - cos(step / 2)).
    let ratio = (1.0 - tolerance / radius_mm).clamp(-1.0, 1.0);
    let step = 2.0 * ratio.acos();
    let count = if step > 0.0 {
        (std::f64::consts::TAU / step).ceil()
    } else {
        MAX_CIRCLE_SEGMENTS as f64
    };
    (count as usize).clamp(MIN_CIRCLE_SEGMENTS, MAX_CIRCLE_SEGMENTS)
}

/// Facets used to approximate a sector's arc.
///
/// The circle's tolerance-bounded count is scaled to the swept fraction of the turn, so the chord
/// tolerance of a quarter sector matches a circle of the same radius rather than being four times
/// coarser. At least two facets are always used: one chord per radius would repeat a point.
fn arc_segments(radius_mm: f64, tolerance_mm: f64, sweep_degrees: f64) -> usize {
    let full = circle_segments(radius_mm, tolerance_mm) as f64;
    let scaled = (full * sweep_degrees / 360.0).ceil();
    if !scaled.is_finite() {
        return MIN_ARC_SEGMENTS;
    }
    (scaled as usize).clamp(MIN_ARC_SEGMENTS, MAX_CIRCLE_SEGMENTS)
}

/// Convert a schema-1 payload in place: flat rectangle fields become an explicit
/// [`SketchProfile::Rectangle`], and the standard `diameter` parameter is added when absent so an
/// upgraded project can be switched to a round profile without a manual repair step.
fn migrate_v1_to_v2(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    let sketches = value
        .get_mut("sketches")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| ProjectError::Invalid("schema 1 project has no sketches array".into()))?;
    for sketch in sketches.iter_mut() {
        let object = sketch
            .as_object_mut()
            .ok_or_else(|| ProjectError::Invalid("schema 1 sketch is not an object".into()))?;
        let width = object
            .remove("width_param")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| ProjectError::Invalid("schema 1 sketch has no width_param".into()))?;
        let height = object
            .remove("height_param")
            .and_then(|value| value.as_str().map(str::to_owned))
            .ok_or_else(|| ProjectError::Invalid("schema 1 sketch has no height_param".into()))?;
        object.insert(
            "profile".into(),
            serde_json::json!({
                "kind": "rectangle",
                "width_param": width,
                "height_param": height,
            }),
        );
    }

    if let Some(parameters) = value
        .get_mut("parameters")
        .and_then(serde_json::Value::as_array_mut)
    {
        let has_diameter = parameters.iter().any(|parameter| {
            parameter.get("id").and_then(serde_json::Value::as_str) == Some("diameter")
        });
        if !has_diameter {
            parameters.push(serde_json::json!({ "id": "diameter", "value": 10.0 }));
        }
    }

    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(2));
    Ok(value)
}

/// Convert a schema-2 payload in place: every feature gains an explicit operation.
///
/// Schema 2 had no operation, and its evaluation silently used only the last feature. A file with
/// one feature therefore means exactly what it did before (`new_body`). A file that somehow held
/// several features is interpreted as the ordered history the earlier writer never evaluated: the
/// first starts the body and the rest join it. No file is rewritten on disk by loading it.
fn migrate_v2_to_v3(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    let features = value
        .get_mut("features")
        .and_then(serde_json::Value::as_array_mut)
        .ok_or_else(|| ProjectError::Invalid("schema 2 project has no features array".into()))?;
    for (index, feature) in features.iter_mut().enumerate() {
        let object = feature
            .as_object_mut()
            .ok_or_else(|| ProjectError::Invalid("schema 2 feature is not an object".into()))?;
        object
            .entry("operation")
            .or_insert_with(|| serde_json::json!(if index == 0 { "new_body" } else { "join" }));
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(3));
    Ok(value)
}

/// Convert a schema-3 payload in place: stamp the version.
///
/// Schema 4 only adds [`SketchProfile::Polyline`]. A schema-3 payload contains no polyline, so no
/// field rewriting is required; the bump exists because a schema-3 reader would refuse the new
/// profile kind and should instead report "newer schema". No file is rewritten on disk by loading
/// it.
fn migrate_v3_to_v4(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(4));
    Ok(value)
}

/// Convert a schema-4 payload in place: a feature's flat `depth_param` becomes an explicit
/// [`FeatureKind::Extrude`].
///
/// Schema 4 could only extrude, so every migrated feature is an extrude and its meaning is
/// unchanged. A feature that already carries a `kind` is left alone, which keeps the migration
/// idempotent. No file is rewritten on disk by loading it.
fn migrate_v4_to_v5(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    if let Some(features) = value
        .get_mut("features")
        .and_then(serde_json::Value::as_array_mut)
    {
        for feature in features.iter_mut() {
            let object = feature
                .as_object_mut()
                .ok_or_else(|| ProjectError::Invalid("schema 4 feature is not an object".into()))?;
            if object.contains_key("kind") {
                continue;
            }
            let depth = object
                .remove("depth_param")
                .and_then(|value| value.as_str().map(str::to_owned))
                .ok_or_else(|| {
                    ProjectError::Invalid("schema 4 feature has no depth_param".into())
                })?;
            object.insert(
                "kind".into(),
                serde_json::json!({ "type": "extrude", "depth_param": depth }),
            );
        }
    }
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(PROJECT_SCHEMA));
    Ok(value)
}

/// Convert a schema-5 payload in place: only the version changes.
///
/// Schema 6 adds [`SketchProfile::Sector`] and no field is renamed or removed, so a schema-5 file is
/// already a valid schema-6 file. The bump exists so a schema-5 build says "newer schema" for a
/// project containing a sector instead of failing to read the profile. No file is rewritten by
/// loading it.
fn migrate_v5_to_v6(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(PROJECT_SCHEMA));
    Ok(value)
}

/// Convert a schema-6 payload in place: only the version changes.
///
/// Schema 7 adds the optional `revisions` list and its `next_revision` counter, both defaulted, so a
/// schema-6 file is already valid. The bump exists so a schema-6 build says "newer schema" for a
/// project that carries revisions instead of failing on an unknown field. No file is rewritten by
/// loading it.
fn migrate_v6_to_v7(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(PROJECT_SCHEMA));
    Ok(value)
}

/// Convert a schema-7 payload in place: only the version changes.
///
/// Schema 8 adds a sketch's `plane_z` and an extrude's `direction`, both defaulted, so a schema-7
/// file is already a valid schema-8 file: every sketch it holds is on the world plane and every
/// extrude sweeps up, which is all schema 7 could express. The bump exists so a schema-7 build says
/// "newer schema" for a project that uses a raised plane instead of failing on an unknown field. No
/// file is rewritten by loading it.
fn migrate_v7_to_v8(mut value: serde_json::Value) -> Result<serde_json::Value, ProjectError> {
    let object = value
        .as_object_mut()
        .ok_or_else(|| ProjectError::Invalid("project JSON is not an object".into()))?;
    object.insert("schema".into(), serde_json::json!(PROJECT_SCHEMA));
    Ok(value)
}

/// A cut tool that reaches past every face of `body` that `depth_mm` would otherwise share.
///
/// `None` means "no coincidence to break" — either the tool's far end stops inside the body, or the
/// kernel cannot move a solid, in which case the caller keeps the tool exactly as drawn and the cut
/// may carry the kernel's coplanar defect rather than failing outright. That fallback is deliberate:
/// a cut that is right to 1.1e-4 mm is more useful than no cut, and the alternative would be to
/// refuse cutting on any kernel that cannot translate.
fn overshooting_tool<K: Kernel>(
    kernel: &mut K,
    profile: &[[f64; 2]],
    depth_mm: f64,
    plane_mm: f64,
    body: SolidId,
) -> Option<SolidId> {
    if !kernel.supports(cad_contract::capability::Capability::Transform) {
        return None;
    }
    let bounds = kernel.bounding_box(body).ok()?;
    // The tool as drawn spans `[lo, hi]` on z: from the sketch plane, which the profile is drawn on,
    // in the direction the feature sweeps. `depth_mm` carries the sign, so `min`/`max` handle both.
    let mut lo = plane_mm + depth_mm.min(0.0);
    let mut hi = plane_mm + depth_mm.max(0.0);
    let coincidence = CUT_OVERSHOOT_MM * 0.5;
    let mut extended = false;
    if (lo - bounds.min[2]).abs() <= coincidence {
        lo -= CUT_OVERSHOOT_MM;
        extended = true;
    }
    if (hi - bounds.max[2]).abs() <= coincidence {
        hi += CUT_OVERSHOOT_MM;
        extended = true;
    }
    if !extended {
        return None;
    }
    let height = hi - lo;
    if !height.is_finite() || height <= 0.0 {
        return None;
    }
    let tool = kernel.extrude(profile, height).ok()?;
    match kernel.translate(tool, [0.0, 0.0, lo]) {
        Ok(moved) => {
            kernel.release(tool);
            Some(moved)
        }
        Err(_) => {
            kernel.release(tool);
            None
        }
    }
}

/// A join tool moved so that it does not share a face with the body it joins.
///
/// A boss placed on a body's top face shares that face, which is exactly the coplanar case the
/// corpus records as a failure. The tool is therefore adjusted by [`JOIN_OVERLAP_MM`] in the one
/// direction that cannot change the union:
///
/// * a tool that sits **on** the body's top face has its bottom moved down into the body;
/// * a tool that sits **under** the body's bottom face has its top moved up into it;
/// * a tool that reaches a face from the inside has that face pulled back, which removes only
///   material that already lies inside the body.
///
/// Every one of those is only volume-neutral while the tool lies **inside the body's footprint**: a
/// tool that overhangs the body has material outside it, and moving its face there would add or drop
/// that sliver. The adjustment is therefore skipped unless the profile's bounding box is inside the
/// body's, which leaves an overhanging join exactly as it was before this helper existed — the
/// kernel then either unions it or reports why it could not.
///
/// `None` means there was no coincidence to break, the tool overhangs the body, or the kernel cannot
/// move a solid; the caller then joins the tool exactly as drawn and any coplanar defect is the
/// feature's reported result rather than something hidden here.
fn overlapping_join_tool<K: Kernel>(
    kernel: &mut K,
    profile: &[[f64; 2]],
    depth_mm: f64,
    plane_mm: f64,
    body: SolidId,
) -> Option<SolidId> {
    if !kernel.supports(cad_contract::capability::Capability::Transform) {
        return None;
    }
    let bounds = kernel.bounding_box(body).ok()?;
    let mut lo = plane_mm + depth_mm.min(0.0);
    let mut hi = plane_mm + depth_mm.max(0.0);
    let coincident_xy = profile.iter().all(|point| {
        point[0] >= bounds.min[0] - 1e-9
            && point[0] <= bounds.max[0] + 1e-9
            && point[1] >= bounds.min[1] - 1e-9
            && point[1] <= bounds.max[1] + 1e-9
    });
    if !coincident_xy {
        return None;
    }
    let coincidence = JOIN_OVERLAP_MM * 0.5;
    let mut adjusted = false;
    if (hi - bounds.max[2]).abs() <= coincidence {
        hi -= JOIN_OVERLAP_MM;
        adjusted = true;
    }
    if (lo - bounds.max[2]).abs() <= coincidence {
        lo -= JOIN_OVERLAP_MM;
        adjusted = true;
    }
    if (lo - bounds.min[2]).abs() <= coincidence {
        lo += JOIN_OVERLAP_MM;
        adjusted = true;
    }
    if (hi - bounds.min[2]).abs() <= coincidence {
        hi += JOIN_OVERLAP_MM;
        adjusted = true;
    }
    if !adjusted {
        return None;
    }
    let height = hi - lo;
    if !height.is_finite() || height <= 0.0 {
        return None;
    }
    let tool = kernel.extrude(profile, height).ok()?;
    match kernel.translate(tool, [0.0, 0.0, lo]) {
        Ok(moved) => {
            kernel.release(tool);
            Some(moved)
        }
        Err(_) => {
            kernel.release(tool);
            None
        }
    }
}

/// Move a tool from the world plane to its sketch's plane height.
///
/// A kernel sweep always starts at z = 0, so a sketch above that plane is built there and then
/// moved. A kernel that cannot translate cannot place a sketch off the world plane and says so
/// instead of silently building the feature at z = 0.
fn place_on_plane<K: Kernel>(
    kernel: &mut K,
    solid: SolidId,
    plane_mm: f64,
) -> Result<SolidId, String> {
    if plane_mm == 0.0 {
        return Ok(solid);
    }
    if !kernel.supports(cad_contract::capability::Capability::Transform) {
        kernel.release(solid);
        return Err(
            "the kernel does not advertise transform, so a sketch above the z = 0 plane cannot be \
             placed"
                .into(),
        );
    }
    match kernel.translate(solid, [0.0, 0.0, plane_mm]) {
        Ok(moved) => {
            kernel.release(solid);
            Ok(moved)
        }
        Err(error) => {
            kernel.release(solid);
            Err(format!("placing the sketch plane failed: {error}"))
        }
    }
}

/// A sketch plane height must be a finite number of document units.
fn validate_plane_z(plane_z: f64) -> Result<(), ProjectError> {
    if !plane_z.is_finite() {
        return Err(ProjectError::Invalid(
            "sketch plane height must be a finite number of document units".into(),
        ));
    }
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// Unsaved-edit recovery snapshots
// ---------------------------------------------------------------------------------------------

/// Payload schema of an unsaved-edit recovery snapshot.
///
/// Independent of [`PROJECT_SCHEMA`]: a snapshot wraps a project, so the project inside still
/// carries its own schema and migrates the same way on load.
///
/// * Schema 1 records the project, when it was written, and nothing else.
pub const RECOVERY_SCHEMA: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct RecoveryFile {
    schema: u32,
    saved_at_unix: u64,
    /// mtime of the project file when the snapshot was written; `None` if it did not exist.
    base_mtime_unix: Option<u64>,
    project: Project,
}

/// A snapshot that parsed and validated, with the file version it was taken from.
struct LoadedRecovery {
    project: Project,
    info: RecoveryInfo,
    base_mtime_unix: Option<u64>,
}

/// Which copy a session was recovered from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecoverySource {
    /// The unsaved-edit snapshot next to the project, written after the last edit.
    Snapshot,
    /// The previous good project file, kept as `<path>.backup` by the last successful save.
    Backup,
}

/// What the application should say about a project it recovered instead of the file on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecoveryInfo {
    /// When the copy was written, in seconds since the Unix epoch.
    pub saved_at_unix: u64,
    /// The project file could not be read, so this copy is the only one of the document.
    pub file_unreadable: bool,
    /// Whether this came from an unsaved-edit snapshot or the previous good file.
    pub source: RecoverySource,
}

/// Where the previous good copy of `path` lives: next to the project, so it moves with it.
pub fn backup_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".backup");
    PathBuf::from(name)
}

/// Replace `<path>.backup` with the current contents of `path`, atomically if possible.
///
/// Returns `Ok(false)` when there is no file to copy yet. A backup is written through a temporary
/// file and renamed, so a crash during the copy cannot leave a half-written backup in its place.
fn refresh_backup(path: &Path) -> Result<bool, ProjectError> {
    if !path.is_file() {
        return Ok(false);
    }
    let backup = backup_path(path);
    let parent = backup
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = backup
        .file_name()
        .ok_or_else(|| ProjectError::Invalid("backup path has no file name".into()))?;
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let mut temp = PathBuf::from(parent);
    temp.push(format!(
        ".{}.tmp-{}-{sequence}",
        name.to_string_lossy(),
        std::process::id()
    ));
    std::fs::copy(path, &temp).map_err(|source| ProjectError::Io {
        path: temp.display().to_string(),
        message: source.to_string(),
    })?;
    let synced = std::fs::File::open(&temp).and_then(|file| file.sync_all());
    if let Err(source) = synced {
        let _ = std::fs::remove_file(&temp);
        return Err(ProjectError::Io {
            path: temp.display().to_string(),
            message: source.to_string(),
        });
    }
    if let Err(source) = std::fs::rename(&temp, &backup) {
        let _ = std::fs::remove_file(&temp);
        return Err(ProjectError::Io {
            path: backup.display().to_string(),
            message: source.to_string(),
        });
    }
    Ok(true)
}

/// Where the unsaved-edit snapshot for `path` lives: next to the project, so it moves with it.
pub fn recovery_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".recovery");
    PathBuf::from(name)
}

/// Delete the snapshot for `path`, if there is one. A missing snapshot is not an error.
///
/// Called after a successful save, and when the user discards a recovered state.
pub fn clear_recovery(path: &Path) -> Result<(), ProjectError> {
    let snapshot = recovery_path(path);
    match std::fs::remove_file(&snapshot) {
        Ok(()) => Ok(()),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(ProjectError::Io {
            path: snapshot.display().to_string(),
            message: error.to_string(),
        }),
    }
}

/// Read the snapshot for `path`, if one exists and validates.
///
/// A missing, oversized, malformed or invalid snapshot is `None`, never an error: a stale file left
/// by an old build or a half-written one must not stop the application from opening the project.
fn load_recovery(path: &Path) -> Result<Option<LoadedRecovery>, ProjectError> {
    let snapshot = recovery_path(path);
    let metadata = match std::fs::metadata(&snapshot) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => {
            return Err(ProjectError::Io {
                path: snapshot.display().to_string(),
                message: error.to_string(),
            });
        }
    };
    if metadata.len() > MAX_PROJECT_BYTES {
        return Ok(None);
    }
    let bytes = match std::fs::read(&snapshot) {
        Ok(bytes) => bytes,
        Err(_) => return Ok(None),
    };
    let file: RecoveryFile = match serde_json::from_slice(&bytes) {
        Ok(file) => file,
        Err(_) => return Ok(None),
    };
    if file.schema != RECOVERY_SCHEMA {
        return Ok(None);
    }
    let info = RecoveryInfo {
        saved_at_unix: file.saved_at_unix,
        file_unreadable: false,
        source: RecoverySource::Snapshot,
    };
    Ok(Some(LoadedRecovery {
        project: file.project,
        info,
        base_mtime_unix: file.base_mtime_unix,
    }))
}

/// Load `path`, preferring an unsaved-edit snapshot taken from the same version of the file.
///
/// Three outcomes:
///
/// * the file loads and was written after the snapshot: the file wins, and the superseded snapshot
///   is deleted;
/// * the file loads and is still the version the snapshot was taken from: the snapshot wins, with
///   [`RecoveryInfo::file_unreadable`] `false`;
/// * the file cannot be read and a snapshot exists: the snapshot wins, with
///   [`RecoveryInfo::file_unreadable`] `true`, because the snapshot is then the only copy.
///
/// The caller must still treat the recovered project as **unsaved**: saving is what promotes it to
/// the project file, and [`clear_recovery`] is what happens after.
pub fn open_with_recovery(path: &Path) -> Result<(Project, Option<RecoveryInfo>), ProjectError> {
    let written_at = last_write_unix(path);
    let recovery = load_recovery(path)?;
    match Project::load(path) {
        Ok(project) => match recovery {
            Some(recovered) if recovered.base_mtime_unix == written_at => {
                Ok((recovered.project, Some(recovered.info)))
            }
            Some(_) => {
                // The project file was written after the snapshot was taken, so the snapshot only
                // describes history. Remove it rather than offering it again next start.
                let _ = clear_recovery(path);
                Ok((project, None))
            }
            None => Ok((project, None)),
        },
        Err(error) => match recovery {
            Some(recovered) => Ok((
                recovered.project,
                Some(RecoveryInfo {
                    file_unreadable: true,
                    ..recovered.info
                }),
            )),
            None => {
                // No unsaved-edit snapshot. The previous good file is still worth opening: a
                // truncated or hand-damaged project is exactly the case a backup exists for.
                let backup = backup_path(path);
                match Project::load(&backup) {
                    Ok(project) => Ok((
                        project,
                        Some(RecoveryInfo {
                            saved_at_unix: last_write_unix(&backup).unwrap_or(0),
                            file_unreadable: true,
                            source: RecoverySource::Backup,
                        }),
                    )),
                    Err(_) => Err(error),
                }
            }
        },
    }
}

fn last_write_unix(path: &Path) -> Option<u64> {
    std::fs::metadata(path)
        .ok()?
        .modified()
        .ok()?
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|elapsed| elapsed.as_secs())
}

fn unix_now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|elapsed| elapsed.as_secs())
        .unwrap_or(0)
}

fn validate_project_size(size: u64) -> Result<(), ProjectError> {
    if size > MAX_PROJECT_BYTES {
        return Err(ProjectError::Invalid(format!(
            "project is larger than the {MAX_PROJECT_BYTES}-byte limit"
        )));
    }
    Ok(())
}

/// Write next to the destination and rename only after the full file reaches stable storage.
/// A failed write leaves an existing project untouched.
fn atomic_replace(path: &Path, bytes: &[u8]) -> Result<(), ProjectError> {
    let parent = path
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    let name = path
        .file_name()
        .ok_or_else(|| ProjectError::Invalid("project path has no file name".into()))?;
    let mut temp_path = PathBuf::from(parent);
    let sequence = TEMP_FILE_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let temp_name = format!(
        ".{}.tmp-{}-{sequence}",
        name.to_string_lossy(),
        std::process::id()
    );
    temp_path.push(temp_name);

    let write_result = (|| {
        let mut file = OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temp_path)
            .map_err(|source| ProjectError::Io {
                path: temp_path.display().to_string(),
                message: source.to_string(),
            })?;
        file.write_all(bytes).map_err(|source| ProjectError::Io {
            path: temp_path.display().to_string(),
            message: source.to_string(),
        })?;
        file.sync_all().map_err(|source| ProjectError::Io {
            path: temp_path.display().to_string(),
            message: source.to_string(),
        })?;
        drop(file);
        std::fs::rename(&temp_path, path).map_err(|source| ProjectError::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        })?;
        #[cfg(unix)]
        std::fs::File::open(parent)
            .and_then(|directory| directory.sync_all())
            .map_err(|source| ProjectError::Io {
                path: parent.display().to_string(),
                message: source.to_string(),
            })?;
        Ok(())
    })();

    if write_result.is_err() {
        let _ = std::fs::remove_file(&temp_path);
    }
    write_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use cad_contract::capability::{Capability, CapabilitySet, KernelInfo};
    use cad_contract::error::KernelError;
    use cad_contract::kernel::{BooleanOp, SolidId};
    use cad_contract::mesh::Mesh;
    use std::collections::BTreeMap;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicU64, Ordering};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    static COUNTER: AtomicU64 = AtomicU64::new(0);

    struct TempDir(PathBuf);

    impl TempDir {
        fn new(tag: &str) -> std::io::Result<Self> {
            let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
            let mut path = std::env::temp_dir();
            path.push(format!(
                "cad-document-{tag}-{}-{unique}",
                std::process::id()
            ));
            std::fs::create_dir_all(&path)?;
            Ok(TempDir(path))
        }
    }

    impl Drop for TempDir {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    /// A kernel that extrudes a rectangle into a triangularized box with analytic volume.
    struct BoxKernel {
        next: u64,
        solids: BTreeMap<u64, [f64; 3]>,
    }

    impl BoxKernel {
        fn new() -> Self {
            BoxKernel {
                next: 1,
                solids: BTreeMap::new(),
            }
        }

        fn box_mesh(size: [f64; 3]) -> Mesh {
            let [x, y, z] = size;
            let positions = vec![
                [0.0, 0.0, 0.0],
                [x, 0.0, 0.0],
                [x, y, 0.0],
                [0.0, y, 0.0],
                [0.0, 0.0, z],
                [x, 0.0, z],
                [x, y, z],
                [0.0, y, z],
            ];
            let triangles = vec![
                [0, 2, 1],
                [0, 3, 2],
                [4, 5, 6],
                [4, 6, 7],
                [0, 1, 5],
                [0, 5, 4],
                [1, 2, 6],
                [1, 6, 5],
                [2, 3, 7],
                [2, 7, 6],
                [3, 0, 4],
                [3, 4, 7],
            ];
            Mesh {
                positions,
                triangles,
                normals: Vec::new(),
            }
        }
    }

    impl Kernel for BoxKernel {
        fn info(&self) -> KernelInfo {
            KernelInfo::new(
                "box-kernel",
                "test",
                CapabilitySet::from([Capability::Extrude, Capability::Tessellate]),
            )
        }

        fn extrude(
            &mut self,
            profile: &[[f64; 2]],
            height_mm: f64,
        ) -> cad_contract::Result<SolidId> {
            validate_profile(profile)?;
            if !height_mm.is_finite() || height_mm <= 0.0 {
                return Err(KernelError::Invalid("height must be positive".into()));
            }
            let width = profile[1][0] - profile[0][0];
            let height = profile[2][1] - profile[1][1];
            let id = self.next;
            self.next += 1;
            self.solids.insert(id, [width, height, height_mm]);
            Ok(SolidId::from_raw(id))
        }

        fn boolean(
            &mut self,
            _op: BooleanOp,
            _a: SolidId,
            _b: SolidId,
        ) -> cad_contract::Result<SolidId> {
            Err(KernelError::Unsupported {
                capability: Capability::BooleanUnion,
            })
        }

        fn tessellate(&self, solid: SolidId, _tol: f64) -> cad_contract::Result<Mesh> {
            let size = self
                .solids
                .get(&solid.get())
                .ok_or(KernelError::UnknownSolid(solid))?;
            Ok(Self::box_mesh(*size))
        }

        fn release(&mut self, solid: SolidId) {
            self.solids.remove(&solid.get());
        }
    }

    fn box_project() -> Result<Project, ProjectError> {
        Project::box_part(
            "bracket",
            DocumentId::new(1),
            Unit::Millimeter,
            [10.0, 10.0, 5.0],
        )
    }

    #[test]
    fn a_box_project_has_analytic_volume() -> TestResult {
        let project = box_project()?;
        let mut kernel = BoxKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        assert!(
            (evaluated.volume_mm3 - 500.0).abs() < 1e-9,
            "volume was {}",
            evaluated.volume_mm3
        );
        assert_eq!(evaluated.triangles, 12);
        Ok(())
    }

    #[test]
    fn editing_a_parameter_rebuilds_dependent_geometry() -> TestResult {
        let mut project = box_project()?;
        project.set_parameter("depth", 8.0)?;
        let mut kernel = BoxKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        assert!((evaluated.volume_mm3 - 800.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn undo_restores_the_previous_parameter() -> TestResult {
        let mut project = box_project()?;
        project.set_parameter("width", 20.0)?;
        assert!((project.parameter("width")? - 20.0).abs() < 1e-12);
        project.undo()?;
        assert!((project.parameter("width")? - 10.0).abs() < 1e-12);
        project.redo()?;
        assert!((project.parameter("width")? - 20.0).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn save_and_reopen_preserves_parameters_and_features() -> TestResult {
        let dir = TempDir::new("roundtrip")?;
        let project = box_project()?;
        let path = dir.0.join("bracket.zcad");
        project.save(&path)?;
        let loaded = Project::load(&path)?;
        assert_eq!(loaded.name, "bracket");
        assert_eq!(loaded.sketches().len(), 1);
        assert_eq!(loaded.features().len(), 1);
        assert!((loaded.parameter("depth")? - 5.0).abs() < 1e-12);
        let mut kernel = BoxKernel::new();
        let evaluated = loaded.evaluate(&mut kernel)?;
        assert!((evaluated.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn save_atomically_replaces_an_existing_project() -> TestResult {
        let dir = TempDir::new("atomic-save")?;
        let path = dir.0.join("part.zcad");
        let original = box_project()?;
        original.save(&path)?;
        let mut edited = Project::load(&path)?;
        edited.set_parameter("depth", 8.0)?;
        edited.save(&path)?;

        let reopened = Project::load(&path)?;
        assert!((reopened.parameter("depth")? - 8.0).abs() < 1e-12);
        let saved_bytes = std::fs::read(&path)?;
        let mut invalid = reopened.clone();
        invalid.name.clear();
        assert!(invalid.save(&path).is_err());
        assert_eq!(std::fs::read(&path)?, saved_bytes);
        let temp_files = std::fs::read_dir(&dir.0)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains(".tmp-"))
            .count();
        assert_eq!(
            temp_files, 0,
            "successful saves must remove temporary files"
        );
        Ok(())
    }

    #[test]
    fn undo_and_redo_history_survive_serialization() -> TestResult {
        let mut project = box_project()?;
        project.set_parameter("width", 20.0)?;
        let bytes = project.to_json()?;
        let mut reopened = Project::from_json(bytes.as_bytes())?;

        reopened.undo()?;
        assert!((reopened.parameter("width")? - 10.0).abs() < 1e-12);
        reopened.redo()?;
        assert!((reopened.parameter("width")? - 20.0).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn schema_one_projects_without_history_fields_still_load() -> TestResult {
        let project = box_project()?;
        let mut value =
            serde_json::to_value(project).map_err(|error| format!("serialize fixture: {error}"))?;
        if let Some(object) = value.as_object_mut() {
            object.remove("undo");
            object.remove("redo");
        }
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize legacy fixture: {error}"))?;
        let loaded = Project::from_json(&bytes)?;
        assert!(matches!(
            loaded.clone().undo(),
            Err(ProjectError::NothingToUndo)
        ));
        Ok(())
    }

    #[test]
    fn invalid_persisted_history_is_rejected() -> TestResult {
        let mut project = box_project()?;
        project.set_parameter("width", 12.0)?;
        let mut value =
            serde_json::to_value(project).map_err(|error| format!("serialize fixture: {error}"))?;
        value["undo"][0]["parameters"][0]["value"] = serde_json::json!(-1.0);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        assert!(matches!(
            Project::from_json(&bytes),
            Err(ProjectError::Invalid(_))
        ));
        Ok(())
    }

    #[test]
    fn loading_rejects_dangling_feature_references() -> TestResult {
        let project = box_project()?;
        let mut value =
            serde_json::to_value(project).map_err(|error| format!("serialize fixture: {error}"))?;
        value["features"][0]["sketch_id"] = serde_json::json!(999);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        assert!(matches!(
            Project::from_json(&bytes),
            Err(ProjectError::UnknownSketch(999))
        ));
        Ok(())
    }

    #[test]
    fn loading_rejects_non_positive_persisted_parameters() -> TestResult {
        let project = box_project()?;
        let mut value =
            serde_json::to_value(project).map_err(|error| format!("serialize fixture: {error}"))?;
        value["parameters"][0]["value"] = serde_json::json!(-10.0);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        assert!(matches!(
            Project::from_json(&bytes),
            Err(ProjectError::Invalid(_))
        ));
        Ok(())
    }

    #[test]
    fn loading_rejects_project_payloads_over_the_size_limit() {
        assert!(matches!(
            validate_project_size(MAX_PROJECT_BYTES + 1),
            Err(ProjectError::Invalid(message)) if message.contains("limit")
        ));
    }

    #[test]
    fn load_refuses_an_oversized_file_before_reading_it() -> TestResult {
        let dir = TempDir::new("oversized")?;
        let path = dir.0.join("large.zcad");
        let file = std::fs::File::create(&path)?;
        file.set_len(MAX_PROJECT_BYTES + 1)?;
        assert!(matches!(
            Project::load(&path),
            Err(ProjectError::Invalid(message)) if message.contains("limit")
        ));
        Ok(())
    }

    #[test]
    fn a_negative_parameter_is_rejected_and_does_not_mutate() -> TestResult {
        let mut project = box_project()?;
        match project.set_parameter("width", -1.0) {
            Err(ProjectError::Invalid(message)) => {
                assert!(message.contains("positive"), "{message}");
            }
            other => return Err(format!("unexpected: {other:?}").into()),
        }
        assert!((project.parameter("width")? - 10.0).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn a_newer_project_schema_is_refused() -> TestResult {
        let json = br#"{
            "schema": 99,
            "header": {
                "schema": 2,
                "document": 1,
                "units": "millimeter",
                "tolerance": { "linear_mm": 0.05, "angular_rad": 0.005 }
            },
            "revision": 1,
            "name": "future",
            "parameters": [],
            "sketches": [],
            "features": [],
            "next_entity": 1
        }"#;
        match Project::from_json(json) {
            Err(ProjectError::UnsupportedSchema { found: 99, .. }) => Ok(()),
            other => Err(format!("unexpected: {other:?}").into()),
        }
    }

    #[test]
    fn unknown_json_fields_are_refused() -> TestResult {
        let json = br#"{
            "schema": 1,
            "header": {
                "schema": 2,
                "document": 1,
                "units": "millimeter",
                "tolerance": { "linear_mm": 0.05, "angular_rad": 0.005 }
            },
            "revision": 1,
            "name": "extra",
            "parameters": [],
            "sketches": [],
            "features": [],
            "next_entity": 1,
            "secret": true
        }"#;
        assert!(Project::from_json(json).is_err());
        Ok(())
    }

    #[test]
    fn unknown_nested_header_and_tolerance_fields_are_refused() -> TestResult {
        let project = box_project()?;
        let mut value =
            serde_json::to_value(project).map_err(|error| format!("serialize fixture: {error}"))?;
        value["header"]["unexpected"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        assert!(Project::from_json(&bytes).is_err());

        let project = box_project()?;
        let mut value =
            serde_json::to_value(project).map_err(|error| format!("serialize fixture: {error}"))?;
        value["header"]["tolerance"]["unexpected"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        assert!(Project::from_json(&bytes).is_err());
        Ok(())
    }

    #[test]
    fn inch_parameters_are_converted_before_evaluation() -> TestResult {
        let project =
            Project::box_part("inch-box", DocumentId::new(2), Unit::Inch, [1.0, 1.0, 1.0])?;
        let mut kernel = BoxKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        let expected = 25.4f64.powi(3);
        assert!(
            (evaluated.volume_mm3 - expected).abs() < 1e-6,
            "got {}",
            evaluated.volume_mm3
        );
        Ok(())
    }

    #[test]
    fn undo_with_empty_history_is_an_error() -> TestResult {
        let mut project = Project::new("empty", DocumentId::new(3), Unit::Millimeter);
        assert!(matches!(project.undo(), Err(ProjectError::NothingToUndo)));
        Ok(())
    }

    fn polygon_area(points: &[[f64; 2]]) -> f64 {
        let mut twice = 0.0;
        for index in 0..points.len() {
            let next = (index + 1) % points.len();
            twice += points[index][0] * points[next][1] - points[next][0] * points[index][1];
        }
        (twice * 0.5).abs()
    }

    /// A kernel that builds the actual prism for any convex profile, so volume is measured from
    /// geometry instead of being echoed back.
    struct PrismKernel {
        next: u64,
        solids: BTreeMap<u64, (Vec<[f64; 2]>, f64)>,
    }

    impl PrismKernel {
        fn new() -> Self {
            PrismKernel {
                next: 1,
                solids: BTreeMap::new(),
            }
        }

        fn prism_mesh(profile: &[[f64; 2]], height: f64) -> Mesh {
            let n = profile.len();
            let mut positions: Vec<[f64; 3]> = Vec::with_capacity(n * 2);
            for point in profile {
                positions.push([point[0], point[1], 0.0]);
            }
            for point in profile {
                positions.push([point[0], point[1], height]);
            }
            let mut triangles: Vec<[u32; 3]> = Vec::new();
            for index in 1..n - 1 {
                let i = index as u32;
                triangles.push([0, i + 1, i]);
                triangles.push([n as u32, n as u32 + i, n as u32 + i + 1]);
            }
            for index in 0..n {
                let a = index as u32;
                let b = ((index + 1) % n) as u32;
                let top_a = n as u32 + a;
                let top_b = n as u32 + b;
                triangles.push([a, b, top_b]);
                triangles.push([a, top_b, top_a]);
            }
            Mesh {
                positions,
                triangles,
                normals: Vec::new(),
            }
        }
    }

    impl Kernel for PrismKernel {
        fn info(&self) -> KernelInfo {
            KernelInfo::new(
                "prism-kernel",
                "test",
                CapabilitySet::from([Capability::Extrude, Capability::Tessellate]),
            )
        }

        fn extrude(
            &mut self,
            profile: &[[f64; 2]],
            height_mm: f64,
        ) -> cad_contract::Result<SolidId> {
            validate_profile(profile)?;
            let id = self.next;
            self.next += 1;
            self.solids.insert(id, (profile.to_vec(), height_mm));
            Ok(SolidId::from_raw(id))
        }

        fn boolean(
            &mut self,
            _op: BooleanOp,
            _a: SolidId,
            _b: SolidId,
        ) -> cad_contract::Result<SolidId> {
            Err(KernelError::Unsupported {
                capability: Capability::BooleanUnion,
            })
        }

        fn tessellate(&self, solid: SolidId, _tol: f64) -> cad_contract::Result<Mesh> {
            let (profile, height) = self
                .solids
                .get(&solid.get())
                .ok_or(KernelError::UnknownSolid(solid))?;
            Ok(Self::prism_mesh(profile, *height))
        }

        fn release(&mut self, solid: SolidId) {
            self.solids.remove(&solid.get());
        }
    }

    #[test]
    fn a_circle_sketch_becomes_a_tolerance_bounded_polygon() -> TestResult {
        let mut project = Project::new("disc", DocumentId::new(4), Unit::Millimeter);
        project.set_parameter("diameter", 20.0)?;
        let sketch = project.add_circle_sketch("diameter", [0.0, 0.0])?;
        let sketch = project
            .sketch(sketch)
            .ok_or("the circle sketch must exist")?;
        let profile = project.sketch_profile_mm(sketch)?;
        let segments = project.sketch_segment_count(sketch)?;
        assert_eq!(profile.len(), segments);
        assert!((24..=256).contains(&segments));
        for point in &profile {
            let radius = (point[0] * point[0] + point[1] * point[1]).sqrt();
            assert!((radius - 10.0).abs() < 1e-9, "radius was {radius}");
        }
        let analytic =
            0.5 * segments as f64 * 100.0 * (std::f64::consts::TAU / segments as f64).sin();
        assert!(
            (polygon_area(&profile) - analytic).abs() < 1e-9,
            "polygon area {} vs analytic {analytic}",
            polygon_area(&profile)
        );
        assert!(polygon_area(&profile) < std::f64::consts::PI * 100.0);
        Ok(())
    }

    #[test]
    fn a_circular_part_rebuilds_with_a_measured_volume() -> TestResult {
        let mut project = Project::new("disc", DocumentId::new(5), Unit::Millimeter);
        project.set_parameter("diameter", 20.0)?;
        project.set_parameter("depth", 5.0)?;
        project.add_circle_sketch("diameter", [0.0, 0.0])?;
        let sketch_id = project.sketches()[0].id;
        project.add_extrude(sketch_id, "depth")?;

        let mut kernel = PrismKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        let exact = std::f64::consts::PI * 100.0 * 5.0;
        let error = (evaluated.volume_mm3 - exact).abs() / exact;
        assert!(
            error < 0.01,
            "a 20 mm circle at 0.05 mm tolerance should be within 1% of exact, was {error}"
        );
        assert!(evaluated.triangles >= 3);
        Ok(())
    }

    #[test]
    fn a_regular_polygon_has_the_requested_side_count_and_orientation() -> TestResult {
        let mut project = Project::new("nut", DocumentId::new(6), Unit::Millimeter);
        project.set_parameter("diameter", 10.0)?;
        let id = project.add_regular_polygon_sketch("diameter", 6, [2.0, 3.0])?;
        let sketch = project.sketch(id).ok_or("the polygon sketch must exist")?;
        let profile = project.sketch_profile_mm(sketch)?;
        assert_eq!(profile.len(), 6);
        let expected = 0.5 * 6.0 * 25.0 * (std::f64::consts::PI / 3.0).sin();
        assert!((polygon_area(&profile) - expected).abs() < 1e-9);
        for point in &profile {
            let radius = ((point[0] - 2.0).powi(2) + (point[1] - 3.0).powi(2)).sqrt();
            assert!((radius - 5.0).abs() < 1e-9);
        }
        Ok(())
    }

    #[test]
    fn an_invalid_polygon_side_count_is_refused() -> TestResult {
        let mut project = Project::new("bad", DocumentId::new(7), Unit::Millimeter);
        assert!(matches!(
            project.add_regular_polygon_sketch("diameter", 2, [0.0, 0.0]),
            Err(ProjectError::Invalid(_))
        ));
        assert!(matches!(
            project.add_regular_polygon_sketch("diameter", 257, [0.0, 0.0]),
            Err(ProjectError::Invalid(_))
        ));
        Ok(())
    }

    #[test]
    fn changing_the_profile_rebuilds_and_undo_restores_the_rectangle() -> TestResult {
        let mut project = box_project()?;
        let sketch_id = project.sketches()[0].id;
        assert_eq!(project.sketch_segment_count(&project.sketches()[0])?, 4);
        project.set_sketch_profile(
            sketch_id,
            SketchProfile::Circle {
                diameter_param: "diameter".into(),
            },
        )?;
        assert_eq!(project.sketches()[0].profile.kind(), "circle");
        let segments = project.sketch_segment_count(&project.sketches()[0])?;
        assert!(segments > 4);
        project.undo()?;
        assert_eq!(project.sketches()[0].profile.kind(), "rectangle");
        Ok(())
    }

    #[test]
    fn moving_a_sketch_moves_its_geometry() -> TestResult {
        let mut project = box_project()?;
        let sketch_id = project.sketches()[0].id;
        project.set_sketch_origin(sketch_id, [5.0, -2.0])?;
        let sketch = project.sketch(sketch_id).ok_or("sketch must exist")?;
        let profile = project.sketch_profile_mm(sketch)?;
        assert!((profile[0][0] - 5.0).abs() < 1e-12);
        assert!((profile[0][1] + 2.0).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn a_schema_one_project_migrates_to_an_explicit_rectangle_profile() -> TestResult {
        let schema_one = br#"{
            "schema": 1,
            "header": {
                "schema": 2,
                "document": 1,
                "units": "millimeter",
                "tolerance": { "linear_mm": 0.05, "angular_rad": 0.005 }
            },
            "revision": 1,
            "name": "legacy bracket",
            "parameters": [
                { "id": "width", "value": 10.0 },
                { "id": "height", "value": 10.0 },
                { "id": "depth", "value": 5.0 }
            ],
            "sketches": [
                { "id": 1, "width_param": "width", "height_param": "height", "origin": [0.0, 0.0] }
            ],
            "features": [ { "id": 2, "sketch_id": 1, "depth_param": "depth" } ],
            "next_entity": 3
        }"#;
        let project = Project::from_json(schema_one)?;
        assert_eq!(project.schema, PROJECT_SCHEMA);
        assert_eq!(
            project.sketches()[0].profile,
            SketchProfile::Rectangle {
                width_param: "width".into(),
                height_param: "height".into()
            }
        );
        assert!((project.parameter("diameter")? - 10.0).abs() < 1e-12);
        let mut kernel = BoxKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        assert!((evaluated.volume_mm3 - 500.0).abs() < 1e-9);

        assert_eq!(project.features()[0].operation, ExtrudeOperation::NewBody);
        let json = project.to_json()?;
        assert!(
            json.contains(&format!("\"schema\": {PROJECT_SCHEMA}")),
            "{json}"
        );
        assert!(json.contains("\"kind\": \"rectangle\""), "{json}");
        assert!(json.contains("\"operation\": \"new_body\""), "{json}");
        Ok(())
    }

    #[test]
    fn a_schema_two_payload_with_flat_rectangle_fields_is_refused() -> TestResult {
        let json = br#"{
            "schema": 2,
            "header": {
                "schema": 2,
                "document": 1,
                "units": "millimeter",
                "tolerance": { "linear_mm": 0.05, "angular_rad": 0.005 }
            },
            "revision": 1,
            "name": "malformed",
            "parameters": [ { "id": "width", "value": 10.0 } ],
            "sketches": [ { "id": 1, "width_param": "width", "height_param": "width", "origin": [0.0, 0.0] } ],
            "features": [],
            "next_entity": 2
        }"#;
        assert!(Project::from_json(json).is_err());
        Ok(())
    }

    /// A test double for the *document's* join semantics: it records every call and models a union
    /// as a volume sum. The real union is measured against the truck adapter in
    /// `crates/cad-studio/tests/http_api.rs`; this kernel proves the history asks for the right
    /// operations, in the right order, with per-feature reports and a named failure.
    struct JoinKernel {
        next: u64,
        volumes: BTreeMap<u64, f64>,
        calls: Vec<String>,
        fail_boolean: bool,
    }

    impl JoinKernel {
        fn new() -> Self {
            JoinKernel {
                next: 1,
                volumes: BTreeMap::new(),
                calls: Vec::new(),
                fail_boolean: false,
            }
        }

        fn failing() -> Self {
            let mut kernel = Self::new();
            kernel.fail_boolean = true;
            kernel
        }

        fn unions(&self) -> usize {
            self.calls
                .iter()
                .filter(|call| call.starts_with("union"))
                .count()
        }
    }

    impl Kernel for JoinKernel {
        fn info(&self) -> KernelInfo {
            KernelInfo::new(
                "join-kernel",
                "test",
                CapabilitySet::from([
                    Capability::Extrude,
                    Capability::Tessellate,
                    Capability::BooleanUnion,
                    // A sketch can sit on a raised plane, which the document expresses as a
                    // translation of the tool after the sweep.
                    Capability::Transform,
                ]),
            )
        }

        fn extrude(
            &mut self,
            profile: &[[f64; 2]],
            height_mm: f64,
        ) -> cad_contract::Result<SolidId> {
            validate_profile(profile)?;
            if !height_mm.is_finite() || height_mm <= 0.0 {
                return Err(KernelError::Invalid("height must be positive".into()));
            }
            let id = self.next;
            self.next += 1;
            self.calls.push(format!("extrude {id}"));
            self.volumes.insert(id, polygon_area(profile) * height_mm);
            Ok(SolidId::from_raw(id))
        }

        fn boolean(
            &mut self,
            op: BooleanOp,
            a: SolidId,
            b: SolidId,
        ) -> cad_contract::Result<SolidId> {
            if op != BooleanOp::Union {
                return Err(KernelError::Unsupported {
                    capability: op.capability(),
                });
            }
            let left = *self
                .volumes
                .get(&a.get())
                .ok_or(KernelError::UnknownSolid(a))?;
            let right = *self
                .volumes
                .get(&b.get())
                .ok_or(KernelError::UnknownSolid(b))?;
            self.calls.push(format!("union {} {}", a.get(), b.get()));
            if self.fail_boolean {
                return Err(KernelError::Internal("injected boolean failure".into()));
            }
            let id = self.next;
            self.next += 1;
            self.volumes.insert(id, left + right);
            Ok(SolidId::from_raw(id))
        }

        fn tessellate(&self, solid: SolidId, _tol: f64) -> cad_contract::Result<Mesh> {
            let volume = *self
                .volumes
                .get(&solid.get())
                .ok_or(KernelError::UnknownSolid(solid))?;
            Ok(BoxKernel::box_mesh([1.0, 1.0, volume]))
        }

        fn translate(
            &mut self,
            solid: SolidId,
            delta_mm: [f64; 3],
        ) -> cad_contract::Result<SolidId> {
            let volume = *self
                .volumes
                .get(&solid.get())
                .ok_or(KernelError::UnknownSolid(solid))?;
            self.calls
                .push(format!("translate {} {}", solid.get(), delta_mm[2]));
            let id = self.next;
            self.next += 1;
            self.volumes.insert(id, volume);
            Ok(SolidId::from_raw(id))
        }

        fn release(&mut self, solid: SolidId) {
            self.calls.push(format!("release {}", solid.get()));
            self.volumes.remove(&solid.get());
        }

        fn contains(&self, solid: SolidId) -> Option<bool> {
            Some(self.volumes.contains_key(&solid.get()))
        }
    }

    /// A base project plus one joinable second sketch and join feature.
    fn join_project() -> Result<(Project, EntityId, EntityId), ProjectError> {
        let mut project = box_project()?;
        let second = project.add_rectangle_sketch("width", "height", [5.0, 5.0])?;
        let join = project.add_join_extrude(second, "depth")?;
        Ok((project, second, join))
    }

    #[test]
    fn a_two_feature_history_builds_both_extrudes_and_one_union() -> TestResult {
        let (project, _, join) = join_project()?;
        let mut kernel = JoinKernel::new();
        let build = project.build(&mut kernel)?;

        assert!(build.is_complete());
        assert_eq!(build.reports.len(), 2);
        assert_eq!(kernel.unions(), 1, "calls: {:?}", kernel.calls);
        // Two 10 x 10 x 5 boxes: the stub models a disjoint-equal union as 500 + 500.
        match build.feature(join) {
            Some(FeatureOutcome::Built { volume_mm3, .. }) => {
                assert!((volume_mm3 - 1000.0).abs() < 1e-9, "got {volume_mm3}");
            }
            other => return Err(format!("join feature was not built: {other:?}").into()),
        }
        let mesh = build.mesh.ok_or("a completed build must keep its mesh")?;
        assert!((mesh.signed_volume() - 1000.0).abs() < 1e-9);
        kernel.release(build.solid.ok_or("a completed build must keep its solid")?);
        Ok(())
    }

    #[test]
    fn a_shared_parameter_rebuilds_every_dependent_feature() -> TestResult {
        let (mut project, _, _) = join_project()?;
        project.set_parameter("width", 20.0)?;
        let mut kernel = JoinKernel::new();
        let build = project.build(&mut kernel)?;

        assert!(build.is_complete());
        // Each feature is now 20 x 10 x 5, and the second report is the running total.
        assert_eq!(build.reports.len(), 2);
        let first = &build.reports[0].outcome;
        let second = &build.reports[1].outcome;
        assert!(
            matches!(first, FeatureOutcome::Built { volume_mm3, .. } if (volume_mm3 - 1000.0).abs() < 1e-9),
            "first feature: {first:?}"
        );
        assert!(
            matches!(second, FeatureOutcome::Built { volume_mm3, .. } if (volume_mm3 - 2000.0).abs() < 1e-9),
            "second feature: {second:?}"
        );
        kernel.release(build.solid.ok_or("solid")?);
        Ok(())
    }

    /// The M11g sketch plane and extrude direction: the document must place the tool at the height
    /// its sketch declares, sweep it the way the feature says, and keep both in the file.
    fn raised_project(plane_z: f64, direction: ExtrudeDirection) -> Result<Project, ProjectError> {
        let mut project = Project::new("raised", DocumentId::new(1), Unit::Millimeter);
        let sketch = project.add_sketch_on_plane(
            SketchProfile::Rectangle {
                width_param: "width".into(),
                height_param: "height".into(),
            },
            [2.0, 3.0],
            plane_z,
        )?;
        project.add_extrude_feature_in_direction(
            sketch,
            "depth",
            ExtrudeOperation::NewBody,
            direction,
        )?;
        Ok(project)
    }

    #[test]
    fn a_raised_sketch_plane_is_applied_as_a_translation_of_the_tool() -> TestResult {
        let project = raised_project(5.0, ExtrudeDirection::Up)?;
        let mut kernel = JoinKernel::new();
        let build = project.build(&mut kernel)?;
        assert!(build.is_complete(), "{:?}", build.failed);
        assert!(
            kernel.calls.iter().any(|call| call == "translate 1 5"),
            "calls: {:?}",
            kernel.calls
        );
        // The profile is still resolved in its own plane: the height is not baked into the points.
        assert_eq!(
            project.sketch_profile_mm(&project.sketches()[0])?,
            vec![[2.0, 3.0], [12.0, 3.0], [12.0, 13.0], [2.0, 13.0]]
        );
        Ok(())
    }

    #[test]
    fn an_extrude_direction_resolves_to_a_signed_depth() -> TestResult {
        for (direction, expected) in [(ExtrudeDirection::Up, 5.0), (ExtrudeDirection::Down, -5.0)] {
            let project = raised_project(2.5, direction)?;
            let feature = project.features()[0].clone();
            let (_, action) = project
                .resolve_feature(&feature)
                .map_err(|message| -> Box<dyn std::error::Error> { message.into() })?;
            match action {
                ResolvedAction::Extrude { depth_mm, plane_mm } => {
                    assert!(
                        (depth_mm - expected).abs() < 1e-12,
                        "{direction:?} resolved to {depth_mm}"
                    );
                    assert!((plane_mm - 2.5).abs() < 1e-12, "plane {plane_mm}");
                }
                other => return Err(format!("expected an extrude, got {other:?}").into()),
            }
        }
        Ok(())
    }

    #[test]
    fn an_inch_sketch_plane_is_converted_before_it_is_applied() -> TestResult {
        let mut project = Project::new("inch-plane", DocumentId::new(1), Unit::Inch);
        let sketch = project.add_sketch_on_plane(
            SketchProfile::Rectangle {
                width_param: "width".into(),
                height_param: "height".into(),
            },
            [0.0, 0.0],
            0.5,
        )?;
        project.add_extrude_feature(sketch, "depth", ExtrudeOperation::NewBody)?;
        let mut kernel = JoinKernel::new();
        assert!(project.build(&mut kernel)?.is_complete());
        assert!(
            kernel.calls.iter().any(|call| call == "translate 1 12.7"),
            "0.5 in must reach the kernel as 12.7 mm; calls: {:?}",
            kernel.calls
        );
        Ok(())
    }

    #[test]
    fn a_kernel_without_transform_refuses_a_raised_sketch_plane() -> TestResult {
        let project = raised_project(5.0, ExtrudeDirection::Up)?;
        // BoxKernel advertises extrude and tessellation only: it cannot place a tool above z = 0.
        let mut kernel = BoxKernel::new();
        let build = project.build(&mut kernel)?;
        let failure = build.failed.ok_or("a raised plane must fail here")?;
        assert!(
            failure.message.contains("transform"),
            "message: {}",
            failure.message
        );
        assert!(build.mesh.is_none());
        Ok(())
    }

    #[test]
    fn moving_a_sketch_to_another_plane_invalidates_the_cache() -> TestResult {
        let mut project = box_project()?;
        let id = project.sketches()[0].id;
        let mut kernel = JoinKernel::new();
        let mut cache = BuildCache::new();
        project.build_with_cache(&mut kernel, &mut cache)?;
        let extrudes = |kernel: &JoinKernel| {
            kernel
                .calls
                .iter()
                .filter(|call| call.starts_with("extrude"))
                .count()
        };
        let before = extrudes(&kernel);

        project.set_sketch_plane(id, 4.0)?;
        project.build_with_cache(&mut kernel, &mut cache)?;
        assert_eq!(
            extrudes(&kernel),
            before + 1,
            "the plane is part of the resolved inputs; calls: {:?}",
            kernel.calls
        );
        assert!(
            kernel.calls.iter().any(|call| call.ends_with(" 4")),
            "the tool must move to the new plane: {:?}",
            kernel.calls
        );
        Ok(())
    }

    #[test]
    fn placing_a_sketch_on_a_plane_is_one_undo_step() -> TestResult {
        let mut project = box_project()?;
        let id = project.sketches()[0].id;
        let depth = project.undo_depth();
        project.set_sketch_placement(id, [2.0, 3.0], 7.0)?;
        assert_eq!(project.undo_depth(), depth + 1);
        let sketch = project.sketch(id).ok_or("sketch")?;
        assert_eq!(sketch.origin, [2.0, 3.0]);
        assert!((sketch.plane_z - 7.0).abs() < 1e-12);

        project.undo()?;
        let sketch = project.sketch(id).ok_or("sketch")?;
        assert_eq!(sketch.origin, [0.0, 0.0]);
        assert!(sketch.plane_z.abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn a_non_finite_plane_is_refused_and_changes_nothing() -> TestResult {
        let mut project = box_project()?;
        let id = project.sketches()[0].id;
        let profile = SketchProfile::Rectangle {
            width_param: "width".into(),
            height_param: "height".into(),
        };
        assert!(matches!(
            project.add_sketch_on_plane(profile.clone(), [0.0, 0.0], f64::NAN),
            Err(ProjectError::Invalid(message)) if message.contains("plane")
        ));
        assert!(matches!(
            project.set_sketch_plane(id, f64::INFINITY),
            Err(ProjectError::Invalid(message)) if message.contains("plane")
        ));
        assert!(project.sketch(id).ok_or("sketch")?.plane_z.abs() < 1e-12);
        assert_eq!(project.sketches().len(), 1);
        Ok(())
    }

    #[test]
    fn a_schema_seven_project_loads_on_the_world_plane_with_upward_extrudes() -> TestResult {
        let project = box_project()?;
        let mut value =
            serde_json::to_value(&project).map_err(|error| format!("serialize: {error}"))?;
        value["schema"] = serde_json::json!(7);
        // What schema 7 actually wrote: no plane height on a sketch and no direction on an extrude.
        for sketch in value["sketches"].as_array_mut().ok_or("sketches")? {
            sketch.as_object_mut().ok_or("sketch")?.remove("plane_z");
        }
        for feature in value["features"].as_array_mut().ok_or("features")? {
            let kind = feature
                .get_mut("kind")
                .and_then(serde_json::Value::as_object_mut)
                .ok_or("kind")?;
            kind.remove("direction");
        }
        let bytes = serde_json::to_vec(&value).map_err(|error| format!("serialize: {error}"))?;

        let loaded = Project::from_json(&bytes)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert!(loaded.sketches()[0].plane_z.abs() < 1e-12);
        assert_eq!(
            loaded.features()[0].kind,
            FeatureKind::Extrude {
                depth_param: "depth".into(),
                direction: ExtrudeDirection::Up,
            }
        );
        let mut kernel = BoxKernel::new();
        assert!((loaded.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_failing_join_names_its_feature_and_keeps_the_last_good_mesh() -> TestResult {
        let (project, _, join) = join_project()?;
        let mut kernel = JoinKernel::failing();
        let build = project.build(&mut kernel)?;

        assert!(!build.is_complete());
        assert_eq!(build.reports.len(), 2, "evaluation stops at the failure");
        assert!(matches!(
            build.reports[0].outcome,
            FeatureOutcome::Built { .. }
        ));
        match &build.reports[1].outcome {
            FeatureOutcome::Failed { message } => {
                assert!(message.contains("join failed"), "{message}");
                assert!(message.contains("injected"), "{message}");
            }
            other => return Err(format!("expected a failed join, got {other:?}").into()),
        }
        assert_eq!(build.failed.as_ref().map(|failure| failure.id), Some(join));
        let mesh = build.mesh.ok_or("the first feature's mesh must be kept")?;
        assert!(
            (mesh.signed_volume() - 500.0).abs() < 1e-9,
            "kept mesh volume was {}",
            mesh.signed_volume()
        );
        // The document itself is untouched: the user still owns both features and can fix them.
        assert_eq!(project.features().len(), 2);
        kernel.release(build.solid.ok_or("solid")?);

        // The strict wrapper still refuses, and names the feature rather than the document.
        let mut strict = JoinKernel::failing();
        match project.evaluate(&mut strict) {
            Err(ProjectError::FeatureFailed { feature, message }) => {
                assert_eq!(feature, join.get());
                assert!(message.contains("join failed"));
            }
            other => return Err(format!("expected FeatureFailed, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn a_join_is_refused_when_the_kernel_cannot_union() -> TestResult {
        let (project, _, join) = join_project()?;
        let mut kernel = BoxKernel::new();
        let build = project.build(&mut kernel)?;
        assert_eq!(build.failed.as_ref().map(|failure| failure.id), Some(join));
        match &build.reports[1].outcome {
            FeatureOutcome::Failed { message } => assert!(
                message.contains("does not advertise boolean union"),
                "{message}"
            ),
            other => return Err(format!("expected a refused join, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn a_second_body_or_a_join_without_a_body_is_refused() -> TestResult {
        let mut project = box_project()?;
        let second = project.add_rectangle_sketch("width", "height", [5.0, 5.0])?;
        match project.add_extrude(second, "depth") {
            Err(ProjectError::Invalid(message)) => assert!(message.contains("join"), "{message}"),
            other => return Err(format!("unexpected: {other:?}").into()),
        }

        let mut empty = Project::new("empty", DocumentId::new(9), Unit::Millimeter);
        let sketch = empty.add_rectangle_sketch("width", "height", [0.0, 0.0])?;
        match empty.add_join_extrude(sketch, "depth") {
            Err(ProjectError::Invalid(message)) => assert!(message.contains("body"), "{message}"),
            other => return Err(format!("unexpected: {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn loading_refuses_a_feature_that_uses_a_sketch_created_after_it() -> TestResult {
        let (project, second, _) = join_project()?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        // Feature 1 (id 2) may not consume the sketch created later (id 3).
        value["features"][0]["sketch_id"] = serde_json::json!(second.get());
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        match Project::from_json(&bytes) {
            Err(ProjectError::Invalid(message)) => {
                assert!(message.contains("created after it"), "{message}");
            }
            other => return Err(format!("unexpected: {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn schema_two_features_gain_an_explicit_operation_on_load() -> TestResult {
        let (project, _, _) = join_project()?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        value["schema"] = serde_json::json!(2);
        for feature in value["features"].as_array_mut().ok_or("features array")? {
            feature
                .as_object_mut()
                .ok_or("feature object")?
                .remove("operation");
        }
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize legacy fixture: {error}"))?;
        let loaded = Project::from_json(&bytes)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert_eq!(loaded.features().len(), 2);
        assert_eq!(loaded.features()[0].operation, ExtrudeOperation::NewBody);
        assert_eq!(loaded.features()[1].operation, ExtrudeOperation::Join);
        Ok(())
    }

    #[test]
    fn a_two_feature_project_round_trips_through_save_and_reopen() -> TestResult {
        let dir = TempDir::new("feature-graph")?;
        let (project, _, _) = join_project()?;
        let path = dir.0.join("joined.zcad");
        project.save(&path)?;
        let loaded = Project::load(&path)?;

        assert_eq!(loaded.features(), project.features());
        assert_eq!(loaded.sketches(), project.sketches());
        assert_eq!(loaded.to_json()?, project.to_json()?);

        let mut before = JoinKernel::new();
        let mut after = JoinKernel::new();
        let original = project.build(&mut before)?;
        let reopened = loaded.build(&mut after)?;
        assert!(reopened.is_complete());
        assert_eq!(
            original.mesh.map(|mesh| mesh.signed_volume()),
            reopened.mesh.map(|mesh| mesh.signed_volume())
        );
        Ok(())
    }

    #[test]
    fn undo_removes_the_joined_feature_and_its_geometry_together() -> TestResult {
        let (mut project, _, _) = join_project()?;
        let mut kernel = JoinKernel::new();
        assert!(
            (project
                .build(&mut kernel)?
                .mesh
                .ok_or("mesh")?
                .signed_volume()
                - 1000.0)
                .abs()
                < 1e-9
        );

        project.undo()?;
        assert_eq!(project.features().len(), 1);
        let mut kernel = JoinKernel::new();
        assert!(
            (project
                .build(&mut kernel)?
                .mesh
                .ok_or("mesh")?
                .signed_volume()
                - 500.0)
                .abs()
                < 1e-9
        );

        project.redo()?;
        assert_eq!(project.features().len(), 2);
        Ok(())
    }

    #[test]
    fn a_persisted_polygon_with_too_few_sides_is_refused() -> TestResult {
        let mut project = Project::new("poly", DocumentId::new(8), Unit::Millimeter);
        let id = project.add_regular_polygon_sketch("diameter", 6, [0.0, 0.0])?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        let index = project
            .sketches()
            .iter()
            .position(|sketch| sketch.id == id)
            .ok_or("sketch must exist")?;
        value["sketches"][index]["profile"]["sides"] = serde_json::json!(2);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize corrupt fixture: {error}"))?;
        assert!(matches!(
            Project::from_json(&bytes),
            Err(ProjectError::Invalid(message)) if message.contains("sides")
        ));
        Ok(())
    }

    /// An L-shaped closed loop: 30 x 30 outline with a 20 x 20 corner removed.
    /// Area = 30 * 10 + 10 * 20 = 500 mm².
    const L_PROFILE: [[f64; 2]; 6] = [
        [0.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [10.0, 10.0],
        [10.0, 30.0],
        [0.0, 30.0],
    ];

    /// A convex pentagon with analytic area 450 mm² (30 x 20 with its top corners cut).
    ///
    /// Convex on purpose: the in-memory [`PrismKernel`] triangulates a cap as a fan, which is a
    /// correct mesh for a convex loop. The concave L-shape is measured against the real truck
    /// kernel in the corpus and the studio HTTP tests instead.
    const CONVEX_PROFILE: [[f64; 2]; 5] = [
        [0.0, 0.0],
        [30.0, 0.0],
        [30.0, 10.0],
        [15.0, 20.0],
        [0.0, 10.0],
    ];

    fn polyline_project() -> Result<(Project, EntityId), ProjectError> {
        let mut project = Project::new("drawn", DocumentId::new(11), Unit::Millimeter);
        let id = project.add_polyline_sketch(CONVEX_PROFILE.to_vec(), [0.0, 0.0])?;
        project.add_extrude(id, "depth")?;
        Ok((project, id))
    }

    #[test]
    fn a_polyline_sketch_extrudes_to_its_analytic_volume() -> TestResult {
        let (project, id) = polyline_project()?;
        let sketch = project.sketch(id).ok_or("sketch must exist")?;
        assert_eq!(sketch.profile.kind(), "polyline");
        assert_eq!(project.sketch_segment_count(sketch)?, 5);

        let profile = project.sketch_profile_mm(sketch)?;
        assert_eq!(profile.len(), 5);
        // The drawn loop is already counter-clockwise, so it must be passed through unchanged.
        assert!((signed_area(&profile) - 450.0).abs() < 1e-9);
        for (drawn, resolved) in CONVEX_PROFILE.iter().zip(profile.iter()) {
            assert!((drawn[0] - resolved[0]).abs() < 1e-12);
            assert!((drawn[1] - resolved[1]).abs() < 1e-12);
        }

        let mut kernel = PrismKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        // 450 mm² x 5 mm = 2250 mm³, the analytic volume of the pentagonal prism.
        assert!(
            (evaluated.volume_mm3 - 2250.0).abs() < 1e-9,
            "volume was {}",
            evaluated.volume_mm3
        );
        Ok(())
    }

    #[test]
    fn a_concave_polyline_resolves_its_analytic_area() -> TestResult {
        let mut project = Project::new("l-bracket", DocumentId::new(15), Unit::Millimeter);
        let id = project.add_polyline_sketch(L_PROFILE.to_vec(), [0.0, 0.0])?;
        let sketch = project.sketch(id).ok_or("sketch must exist")?;
        assert_eq!(project.sketch_segment_count(sketch)?, 6);
        let profile = project.sketch_profile_mm(sketch)?;
        // 30 x 10 arm plus 10 x 20 upright: 500 mm², the analytic area of the L profile.
        assert!(
            (signed_area(&profile) - 500.0).abs() < 1e-9,
            "area was {}",
            signed_area(&profile)
        );
        Ok(())
    }

    /// A quarter sector of a 20 mm diameter circle: π·10²/4 = 78.5398 mm².
    const SECTOR_ANALYTIC_AREA_MM2: f64 = std::f64::consts::PI * 100.0 / 4.0;

    fn sector_project() -> Result<(Project, EntityId), ProjectError> {
        let mut project = Project::new("pie", DocumentId::new(21), Unit::Millimeter);
        project.set_parameter("diameter", 20.0)?;
        let id = project.add_sector_sketch("diameter", 0.0, 90.0, [0.0, 0.0])?;
        project.add_extrude(id, "depth")?;
        Ok((project, id))
    }

    #[test]
    fn a_sector_resolves_an_arc_from_its_centre_with_its_analytic_area() -> TestResult {
        let (project, id) = sector_project()?;
        let sketch = project.sketch(id).ok_or("sketch must exist")?;
        assert_eq!(sketch.profile.kind(), "sector");

        let segments = project.sketch_segment_count(sketch)?;
        let profile = project.sketch_profile_mm(sketch)?;
        // The profile is the centre plus the arc, so one more point than facets and radii.
        assert_eq!(profile.len(), segments);
        assert!((4..=MAX_CIRCLE_SEGMENTS + 2).contains(&segments));

        // The loop starts at the sector's centre, which is the sketch origin.
        assert!((profile[0][0]).abs() < 1e-12 && (profile[0][1]).abs() < 1e-12);
        // Every other point lies on the circle the sector is cut from, and the arc runs exactly
        // from 0° to 90° counter-clockwise.
        for point in &profile[1..] {
            let radius = (point[0] * point[0] + point[1] * point[1]).sqrt();
            assert!((radius - 10.0).abs() < 1e-9, "radius was {radius}");
            assert!(
                point[0] >= -1e-9 && point[1] >= -1e-9,
                "point left the quadrant"
            );
        }
        let first = profile[1];
        let last = profile[segments - 1];
        assert!((first[0] - 10.0).abs() < 1e-9 && first[1].abs() < 1e-9);
        assert!(last[0].abs() < 1e-9 && (last[1] - 10.0).abs() < 1e-9);

        // The facet count is the circle's count scaled to a quarter turn, and the polygon area is
        // the shoelace area of exactly that many facets.
        let arc_facets = segments - 2;
        let analytic_polygon = 0.5
            * arc_facets as f64
            * 100.0
            * (std::f64::consts::FRAC_PI_2 / arc_facets as f64).sin();
        assert!(
            (signed_area(&profile) - analytic_polygon).abs() < 1e-9,
            "shoelace area {} vs analytic polygon {analytic_polygon}",
            signed_area(&profile)
        );
        // An inscribed polygon under-estimates the disc sector, but only slightly.
        let area = signed_area(&profile);
        assert!(area < SECTOR_ANALYTIC_AREA_MM2);
        assert!(
            (area - SECTOR_ANALYTIC_AREA_MM2).abs() / SECTOR_ANALYTIC_AREA_MM2 < 0.01,
            "area {area} is not within 1% of {SECTOR_ANALYTIC_AREA_MM2}"
        );

        let mut kernel = PrismKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        // A prism has volume = cap area x depth exactly.
        assert!(
            (evaluated.volume_mm3 - area * 5.0).abs() < 1e-9,
            "volume was {} for area {area}",
            evaluated.volume_mm3
        );
        Ok(())
    }

    #[test]
    fn a_sectors_sweep_must_be_between_zero_and_a_full_turn() -> TestResult {
        let mut project = Project::new("bad-pie", DocumentId::new(22), Unit::Millimeter);
        assert!(
            project
                .add_sector_sketch("diameter", 0.0, 0.0, [0.0, 0.0])
                .is_err_and(|error| error.to_string().contains("sweep")),
            "a zero sweep encloses no area and must be refused"
        );
        assert!(
            project
                .add_sector_sketch("diameter", 0.0, 360.0, [0.0, 0.0])
                .is_err_and(|error| error.to_string().contains("circle")),
            "a full turn is a circle, not a sector, and must say so"
        );
        assert!(
            project
                .add_sector_sketch("diameter", 0.0, f64::NAN, [0.0, 0.0])
                .is_err()
        );
        assert!(
            project
                .add_sector_sketch("diameter", f64::INFINITY, 90.0, [0.0, 0.0])
                .is_err()
        );
        assert!(matches!(
            project.add_sector_sketch("missing", 0.0, 90.0, [0.0, 0.0]),
            Err(ProjectError::UnknownParameter(_))
        ));
        // Nothing was added by a refused profile.
        assert!(project.sketches().is_empty());
        Ok(())
    }

    #[test]
    fn a_named_revision_restores_the_history_it_captured() -> TestResult {
        let mut project = box_project()?;
        project.set_parameter("depth", 12.0)?;
        let thick = project.add_revision("thick")?;
        project.set_parameter("depth", 4.0)?;

        let mut kernel = BoxKernel::new();
        assert!((project.evaluate(&mut kernel)?.volume_mm3 - 400.0).abs() < 1e-9);

        // The revision's own summary says what it holds, without exposing the copy.
        let summaries = project.revisions();
        assert_eq!(summaries.len(), 1);
        assert_eq!(summaries[0].id, thick);
        assert_eq!(summaries[0].name, "thick");
        assert_eq!(summaries[0].features, 1);
        assert!(summaries[0].created_at_unix > 0);

        project.restore_revision(thick)?;
        let mut kernel = BoxKernel::new();
        assert!(
            (project.evaluate(&mut kernel)?.volume_mm3 - 1200.0).abs() < 1e-9,
            "restoring must bring back the 12 mm depth"
        );
        // A restore is one undo away from where it started, so a mis-click is recoverable.
        project.undo()?;
        let mut kernel = BoxKernel::new();
        assert!((project.evaluate(&mut kernel)?.volume_mm3 - 400.0).abs() < 1e-9);
        // Undo restores history, not the revision list: the copy is still there.
        assert_eq!(project.revisions().len(), 1);
        Ok(())
    }

    #[test]
    fn revisions_are_named_and_bounded_and_survive_save_and_reopen() -> TestResult {
        let dir = TempDir::new("revisions")?;
        let mut project = box_project()?;
        assert!(project.add_revision("  ").is_err());
        let long = "x".repeat(MAX_REVISION_NAME_CHARS + 1);
        assert!(project.add_revision(&long).is_err());
        project.add_revision("first")?;
        assert!(
            project
                .add_revision("FIRST")
                .is_err_and(|error| error.to_string().contains("already exists")),
            "a name has to be unambiguous, so it is compared case-insensitively"
        );
        project.set_parameter("depth", 9.0)?;
        project.add_revision("second")?;

        let path = dir.0.join("named.zcad");
        project.save(&path)?;
        let loaded = Project::load(&path)?;
        assert_eq!(loaded.revisions(), project.revisions());
        assert_eq!(loaded.schema, PROJECT_SCHEMA);

        // The copy is usable after the round trip, not only the label.
        let second = loaded.revisions()[1].id;
        let mut loaded = loaded;
        loaded.restore_revision(second)?;
        let mut kernel = BoxKernel::new();
        assert!((loaded.evaluate(&mut kernel)?.volume_mm3 - 900.0).abs() < 1e-9);
        assert!(loaded.restore_revision(999).is_err());
        assert!(loaded.remove_revision(second).is_ok());
        assert_eq!(loaded.revisions().len(), 1);
        assert!(loaded.remove_revision(second).is_err());
        Ok(())
    }

    #[test]
    fn only_the_newest_revisions_are_kept_at_the_bound() -> TestResult {
        let mut project = box_project()?;
        for index in 0..=MAX_REVISIONS {
            project.add_revision(&format!("rev-{index}"))?;
        }
        let summaries = project.revisions();
        assert_eq!(summaries.len(), MAX_REVISIONS);
        assert_eq!(summaries[0].name, "rev-1");
        assert_eq!(
            summaries.last().map(|revision| revision.name.as_str()),
            Some(format!("rev-{MAX_REVISIONS}").as_str())
        );
        // Ids stay monotonic; dropping the oldest never reuses one.
        assert!(summaries.windows(2).all(|pair| pair[0].id < pair[1].id));
        Ok(())
    }

    #[test]
    fn a_schema_six_project_migrates_to_schema_seven() -> TestResult {
        let project = box_project()?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        let object = value.as_object_mut().ok_or("project object")?;
        object.insert("schema".into(), serde_json::json!(6));
        object.remove("revisions");
        object.remove("next_revision");
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize legacy fixture: {error}"))?;

        let loaded = Project::from_json(&bytes)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert!(loaded.revisions().is_empty());
        let mut kernel = BoxKernel::new();
        assert!((loaded.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_corrupt_project_opens_its_previous_good_copy() -> TestResult {
        let dir = TempDir::new("backup")?;
        let path = dir.0.join("bracket.zcad");
        let mut project = box_project()?;
        project.save(&path)?;
        // The next save keeps the 5 mm version as the backup and writes the 8 mm one.
        project.set_parameter("depth", 8.0)?;
        project.save(&path)?;
        assert!(backup_path(&path).is_file());

        // Truncate the project the way a full disk or a kill mid-write would.
        std::fs::write(&path, b"{\"schema\": 7, \"name\":")?;
        let (recovered, info) = open_with_recovery(&path)?;
        let info = info.ok_or("the backup must be reported as a recovery")?;
        assert!(info.file_unreadable);
        assert_eq!(info.source, RecoverySource::Backup);
        assert!(info.saved_at_unix > 0);
        assert!((recovered.parameter("depth")? - 5.0).abs() < 1e-12);
        let mut kernel = BoxKernel::new();
        assert!((recovered.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);

        // Saving the recovered document repairs the file, and the repaired file opens directly.
        recovered.save(&path)?;
        assert!((Project::load(&path)?.parameter("depth")? - 5.0).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn a_sector_survives_save_and_reopen() -> TestResult {
        let dir = TempDir::new("sector")?;
        let (project, id) = sector_project()?;
        let path = dir.0.join("pie.zcad");
        project.save(&path)?;

        let loaded = Project::load(&path)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert_eq!(loaded.sketches(), project.sketches());
        assert_eq!(loaded.features(), project.features());

        let mut before = PrismKernel::new();
        let mut after = PrismKernel::new();
        let original = project.evaluate(&mut before)?.volume_mm3;
        let reopened = loaded.evaluate(&mut after)?.volume_mm3;
        assert!((original - reopened).abs() < 1e-9);
        assert_eq!(
            loaded
                .sketch(id)
                .ok_or("the sector must still exist")?
                .profile
                .kind(),
            "sector"
        );
        Ok(())
    }

    #[test]
    fn a_schema_five_project_migrates_to_schema_six() -> TestResult {
        let (project, _) = sector_project()?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        // Schema 5 differs only in the version: the sector itself is not a schema-6 shape, the
        // version exists so an older build refuses the file instead of misreading the profile.
        value["schema"] = serde_json::json!(5);
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize legacy fixture: {error}"))?;

        let loaded = Project::from_json(&bytes)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        let mut kernel = PrismKernel::new();
        assert!(loaded.evaluate(&mut kernel)?.volume_mm3 > 0.0);
        Ok(())
    }

    #[test]
    fn a_clockwise_polyline_is_normalised_to_counter_clockwise() -> TestResult {
        let mut project = Project::new("clockwise", DocumentId::new(12), Unit::Millimeter);
        let mut reversed = CONVEX_PROFILE.to_vec();
        reversed.reverse();
        let id = project.add_polyline_sketch(reversed, [0.0, 0.0])?;
        project.add_extrude(id, "depth")?;
        let sketch = project.sketch(id).ok_or("sketch must exist")?;
        let profile = project.sketch_profile_mm(sketch)?;
        assert!(
            signed_area(&profile) > 0.0,
            "the kernel must receive a counter-clockwise loop, area was {}",
            signed_area(&profile)
        );

        let mut kernel = PrismKernel::new();
        let evaluated = project.evaluate(&mut kernel)?;
        assert!((evaluated.volume_mm3 - 2250.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn moving_a_polyline_sketch_offsets_every_point() -> TestResult {
        let mut project = Project::new("drawn-move", DocumentId::new(16), Unit::Millimeter);
        let id = project.add_polyline_sketch(L_PROFILE.to_vec(), [0.0, 0.0])?;
        project.set_sketch_origin(id, [5.0, -2.0])?;
        let sketch = project.sketch(id).ok_or("sketch must exist")?;
        let profile = project.sketch_profile_mm(sketch)?;
        for (drawn, resolved) in L_PROFILE.iter().zip(profile.iter()) {
            assert!((resolved[0] - (drawn[0] + 5.0)).abs() < 1e-12);
            assert!((resolved[1] - (drawn[1] - 2.0)).abs() < 1e-12);
        }
        Ok(())
    }

    #[test]
    fn an_invalid_polyline_is_refused() -> TestResult {
        let mut project = Project::new("bad-loop", DocumentId::new(13), Unit::Millimeter);

        // Two points cannot enclose an area.
        assert!(matches!(
            project.add_polyline_sketch(vec![[0.0, 0.0], [10.0, 0.0]], [0.0, 0.0]),
            Err(ProjectError::Invalid(message)) if message.contains("between 3 and")
        ));
        // A repeated consecutive point is a zero-length segment.
        assert!(matches!(
            project.add_polyline_sketch(
                vec![[0.0, 0.0], [10.0, 0.0], [10.0, 0.0], [0.0, 10.0]],
                [0.0, 0.0]
            ),
            Err(ProjectError::Invalid(message)) if message.contains("repeated consecutive")
        ));
        // Collinear points enclose nothing.
        assert!(matches!(
            project.add_polyline_sketch(
                vec![[0.0, 0.0], [5.0, 0.0], [10.0, 0.0]],
                [0.0, 0.0]
            ),
            Err(ProjectError::Invalid(message)) if message.contains("no area")
        ));
        // A non-finite coordinate is refused before it reaches the kernel.
        assert!(matches!(
            project.add_polyline_sketch(
                vec![[0.0, 0.0], [f64::INFINITY, 0.0], [0.0, 10.0]],
                [0.0, 0.0]
            ),
            Err(ProjectError::Invalid(message)) if message.contains("finite")
        ));
        // The loop is bounded so a hostile file cannot ask for a million-facet prism.
        let too_many: Vec<[f64; 2]> = (0..=MAX_POLYLINE_POINTS).map(|i| [i as f64, 0.0]).collect();
        assert!(matches!(
            project.add_polyline_sketch(too_many, [0.0, 0.0]),
            Err(ProjectError::Invalid(message)) if message.contains("between 3 and")
        ));
        // None of the refused loops became a sketch.
        assert!(project.sketches().is_empty());
        Ok(())
    }

    #[test]
    fn a_polyline_survives_save_and_reopen() -> TestResult {
        let dir = TempDir::new("polyline")?;
        let (project, id) = polyline_project()?;
        let path = dir.0.join("drawn.zcad");
        project.save(&path)?;
        let loaded = Project::load(&path)?;

        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert_eq!(
            loaded.sketch(id).map(|sketch| sketch.profile.clone()),
            project.sketch(id).map(|sketch| sketch.profile.clone())
        );
        assert_eq!(loaded.to_json()?, project.to_json()?);

        let mut before = PrismKernel::new();
        let mut after = PrismKernel::new();
        let original = project.build(&mut before)?;
        let reopened = loaded.build(&mut after)?;
        assert!((original.mesh.ok_or("mesh")?.signed_volume() - 2250.0).abs() < 1e-9);
        assert!((reopened.mesh.ok_or("mesh")?.signed_volume() - 2250.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_schema_three_project_migrates_to_schema_four() -> TestResult {
        let project = box_project()?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        value["schema"] = serde_json::json!(3);
        let bytes =
            serde_json::to_vec(&value).map_err(|error| format!("serialize legacy: {error}"))?;
        let loaded = Project::from_json(&bytes)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert_eq!(loaded.sketches(), project.sketches());
        assert_eq!(loaded.features(), project.features());
        let mut kernel = BoxKernel::new();
        assert!((loaded.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn undo_removes_a_drawn_sketch() -> TestResult {
        let mut project = Project::new("undo-drawn", DocumentId::new(14), Unit::Millimeter);
        project.add_polyline_sketch(L_PROFILE.to_vec(), [0.0, 0.0])?;
        assert_eq!(project.sketches().len(), 1);
        project.undo()?;
        assert!(project.sketches().is_empty());
        project.redo()?;
        assert_eq!(project.sketches().len(), 1);
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // M11d: revolve
    // -----------------------------------------------------------------------------------------

    /// A kernel that records the revolve requests it receives and models one as a fixed volume, so
    /// the document's plumbing (axis resolution, operation gating, cache, migration) is tested
    /// without pretending to measure a real solid of revolution. The analytic revolve lives in the
    /// truck adapter's tests and in the studio's HTTP test, which use that adapter.
    struct RevolveKernel {
        next: u64,
        volumes: BTreeMap<u64, f64>,
        calls: Vec<([f64; 2], [f64; 2], f64)>,
    }

    impl RevolveKernel {
        fn new() -> Self {
            RevolveKernel {
                next: 1,
                volumes: BTreeMap::new(),
                calls: Vec::new(),
            }
        }
    }

    impl Kernel for RevolveKernel {
        fn info(&self) -> KernelInfo {
            KernelInfo::new(
                "revolve-kernel",
                "test",
                CapabilitySet::from([
                    Capability::Extrude,
                    Capability::Revolve,
                    Capability::Tessellate,
                    Capability::BooleanUnion,
                ]),
            )
        }

        fn extrude(
            &mut self,
            profile: &[[f64; 2]],
            height_mm: f64,
        ) -> cad_contract::Result<SolidId> {
            validate_profile(profile)?;
            let id = self.next;
            self.next += 1;
            self.volumes.insert(id, polygon_area(profile) * height_mm);
            Ok(SolidId::from_raw(id))
        }

        fn revolve(
            &mut self,
            profile: &[[f64; 2]],
            axis_origin_mm: [f64; 2],
            axis_direction_mm: [f64; 2],
            angle_rad: f64,
        ) -> cad_contract::Result<SolidId> {
            validate_profile(profile)?;
            self.calls
                .push((axis_origin_mm, axis_direction_mm, angle_rad));
            let id = self.next;
            self.next += 1;
            // A stub volume: the point of this kernel is that the call arrived with the right
            // inputs, not that it can measure a turned solid.
            self.volumes.insert(id, 123.0);
            Ok(SolidId::from_raw(id))
        }

        fn boolean(
            &mut self,
            op: BooleanOp,
            a: SolidId,
            b: SolidId,
        ) -> cad_contract::Result<SolidId> {
            if op != BooleanOp::Union {
                return Err(KernelError::Unsupported {
                    capability: op.capability(),
                });
            }
            let left = *self
                .volumes
                .get(&a.get())
                .ok_or(KernelError::UnknownSolid(a))?;
            let right = *self
                .volumes
                .get(&b.get())
                .ok_or(KernelError::UnknownSolid(b))?;
            let id = self.next;
            self.next += 1;
            self.volumes.insert(id, left + right);
            Ok(SolidId::from_raw(id))
        }

        fn tessellate(&self, solid: SolidId, _tol: f64) -> cad_contract::Result<Mesh> {
            let volume = *self
                .volumes
                .get(&solid.get())
                .ok_or(KernelError::UnknownSolid(solid))?;
            Ok(BoxKernel::box_mesh([1.0, 1.0, volume]))
        }

        fn release(&mut self, solid: SolidId) {
            self.volumes.remove(&solid.get());
        }

        fn contains(&self, solid: SolidId) -> Option<bool> {
            Some(self.volumes.contains_key(&solid.get()))
        }
    }

    fn revolve_project() -> Result<(Project, EntityId), ProjectError> {
        let mut project = Project::new("lathe", DocumentId::new(20), Unit::Millimeter);
        let sketch = project.add_polyline_sketch(
            vec![[2.0, 0.0], [4.0, 0.0], [4.0, 3.0], [2.0, 3.0]],
            [5.0, 7.0],
        )?;
        Ok((project, sketch))
    }

    #[test]
    fn a_revolve_resolves_the_axis_through_the_sketch_origin() -> TestResult {
        let (mut project, sketch) = revolve_project()?;
        let feature = project.add_revolve(sketch, RevolveAxis::Horizontal)?;
        let mut kernel = RevolveKernel::new();
        let build = project.build(&mut kernel)?;

        assert!(build.is_complete());
        // The profile is in millimetres and offset by the sketch origin (5, 7), so the axis line is
        // the sketch's local X axis placed at that origin, and the sweep is a full turn.
        assert_eq!(
            kernel.calls,
            vec![([5.0, 7.0], [1.0, 0.0], std::f64::consts::TAU)]
        );
        match build.feature(feature) {
            Some(FeatureOutcome::Built { volume_mm3, .. }) => {
                assert!((volume_mm3 - 123.0).abs() < 1e-9);
            }
            other => return Err(format!("revolve was not built: {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn a_revolve_is_refused_when_the_kernel_cannot_revolve() -> TestResult {
        let (mut project, sketch) = revolve_project()?;
        let feature = project.add_revolve(sketch, RevolveAxis::Vertical)?;
        let mut kernel = BoxKernel::new();
        let build = project.build(&mut kernel)?;

        assert_eq!(
            build.failed.as_ref().map(|failure| failure.id),
            Some(feature)
        );
        match &build.reports[0].outcome {
            FeatureOutcome::Failed { message } => {
                assert!(message.contains("does not advertise revolve"), "{message}");
            }
            other => return Err(format!("expected a refused revolve, got {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn a_revolve_join_uses_the_union_and_reports_the_running_volume() -> TestResult {
        let mut project = box_project()?;
        let sketch = project.add_polyline_sketch(
            vec![[2.0, 0.0], [4.0, 0.0], [4.0, 3.0], [2.0, 3.0]],
            [0.0, 0.0],
        )?;
        let feature = project.add_join_revolve(sketch, RevolveAxis::Vertical)?;
        let mut kernel = RevolveKernel::new();
        let build = project.build(&mut kernel)?;

        assert!(build.is_complete());
        assert_eq!(kernel.calls.len(), 1);
        match build.feature(feature) {
            Some(FeatureOutcome::Built { volume_mm3, .. }) => {
                // 500 mm³ from the box plus the stub revolve's 123 mm³.
                assert!((volume_mm3 - 623.0).abs() < 1e-9, "got {volume_mm3}");
            }
            other => return Err(format!("join revolve was not built: {other:?}").into()),
        }
        Ok(())
    }

    #[test]
    fn schema_four_features_migrate_to_an_explicit_extrude_kind() -> TestResult {
        let project = box_project()?;
        let mut value = serde_json::to_value(&project)
            .map_err(|error| format!("serialize fixture: {error}"))?;
        value["schema"] = serde_json::json!(4);
        for feature in value["features"].as_array_mut().ok_or("features array")? {
            let object = feature.as_object_mut().ok_or("feature object")?;
            let depth = object
                .remove("kind")
                .and_then(|kind| kind.get("depth_param").cloned())
                .ok_or("kind.depth_param")?;
            object.insert("depth_param".into(), depth);
        }
        let bytes = serde_json::to_vec(&value)
            .map_err(|error| format!("serialize legacy fixture: {error}"))?;

        let loaded = Project::from_json(&bytes)?;
        assert_eq!(loaded.schema, PROJECT_SCHEMA);
        assert_eq!(
            loaded.features()[0].kind,
            FeatureKind::Extrude {
                depth_param: "depth".into(),
                direction: ExtrudeDirection::Up,
            }
        );
        let mut kernel = BoxKernel::new();
        assert!((loaded.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_revolve_survives_save_and_reopen_with_its_axis() -> TestResult {
        let dir = TempDir::new("revolve")?;
        let (mut project, sketch) = revolve_project()?;
        project.add_revolve(sketch, RevolveAxis::Horizontal)?;
        let path = dir.0.join("lathe.zcad");
        project.save(&path)?;

        let loaded = Project::load(&path)?;
        assert_eq!(loaded.features(), project.features());
        assert_eq!(loaded.to_json()?, project.to_json()?);
        let mut before = RevolveKernel::new();
        let mut after = RevolveKernel::new();
        assert!(project.build(&mut before)?.is_complete());
        assert!(loaded.build(&mut after)?.is_complete());
        assert_eq!(before.calls, after.calls);
        Ok(())
    }

    #[test]
    fn an_unchanged_revolve_history_is_served_from_the_cache() -> TestResult {
        let (mut project, sketch) = revolve_project()?;
        project.add_revolve(sketch, RevolveAxis::Vertical)?;
        let mut kernel = RevolveKernel::new();
        let mut cache = BuildCache::new();
        project.build_with_cache(&mut kernel, &mut cache)?;
        assert_eq!(kernel.calls.len(), 1);

        let second = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(second.is_complete());
        assert_eq!(
            kernel.calls.len(),
            1,
            "the cached rebuild must not revolve again"
        );
        assert_eq!(cache.hits(), 1);
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // M11c: per-feature rebuild cache
    // -----------------------------------------------------------------------------------------

    /// A box plus a join whose depth is driven by `diameter`. The two features therefore read
    /// disjoint parameters, which is what lets a test edit the *last* feature's input without
    /// also changing the first.
    fn isolated_last_feature_project() -> Result<Project, ProjectError> {
        let mut project = box_project()?;
        let second = project.add_rectangle_sketch("width", "height", [5.0, 5.0])?;
        project.add_extrude_feature(second, "diameter", ExtrudeOperation::Join)?;
        Ok(project)
    }

    fn extrudes(kernel: &JoinKernel) -> usize {
        kernel
            .calls
            .iter()
            .filter(|call| call.starts_with("extrude"))
            .count()
    }

    #[test]
    fn a_cached_rebuild_of_an_unchanged_history_does_no_kernel_work() -> TestResult {
        let project = isolated_last_feature_project()?;
        let mut kernel = JoinKernel::new();
        let mut cache = BuildCache::new();

        let first = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(first.is_complete());
        assert_eq!(extrudes(&kernel), 2);
        assert_eq!(kernel.unions(), 1);
        assert_eq!(cache.len(), 2, "one entry per feature prefix");
        assert_eq!((cache.hits(), cache.misses()), (0, 2));

        kernel.calls.clear();
        let second = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(second.is_complete());
        assert!(
            kernel.calls.is_empty(),
            "a rebuild of unchanged inputs must not call the kernel: {:?}",
            kernel.calls
        );
        assert_eq!((cache.hits(), cache.misses()), (2, 2));
        assert_eq!(
            first.mesh.map(|mesh| mesh.signed_volume()),
            second.mesh.map(|mesh| mesh.signed_volume())
        );
        Ok(())
    }

    #[test]
    fn editing_the_last_features_parameter_reuses_the_earlier_prefix() -> TestResult {
        let mut project = isolated_last_feature_project()?;
        let mut kernel = JoinKernel::new();
        let mut cache = BuildCache::new();
        let first = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!((first.mesh.ok_or("mesh")?.signed_volume() - 1500.0).abs() < 1e-9);

        kernel.calls.clear();
        project.set_parameter("diameter", 4.0)?;
        let second = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(second.is_complete());

        // Exactly one extrude and one union: the earlier feature came from the cache, so its
        // extrude (solid 1) was not re-run and `union 1 2` was not repeated. Temporary tools are
        // still released — that is the cut/join path freeing its own operand, not a cached body.
        let geometry_calls: Vec<&String> = kernel
            .calls
            .iter()
            .filter(|call| call.starts_with("extrude") || call.starts_with("union"))
            .collect();
        assert_eq!(
            geometry_calls,
            vec!["extrude 4", "union 1 4"],
            "only the edited feature may rebuild"
        );
        assert_eq!(
            cache.misses(),
            3,
            "two on the first build plus the edited feature"
        );
        assert_eq!(cache.hits(), 1);

        // The reused feature reports its cached measurement; the edited one is measured afresh.
        assert!(matches!(
            second.reports[0].outcome,
            FeatureOutcome::Built { volume_mm3, .. } if (volume_mm3 - 500.0).abs() < 1e-9
        ));
        assert!(matches!(
            second.reports[1].outcome,
            FeatureOutcome::Built { volume_mm3, .. } if (volume_mm3 - 900.0).abs() < 1e-9
        ));
        let volume = second.mesh.ok_or("mesh")?.signed_volume();
        assert!((volume - 900.0).abs() < 1e-9, "volume was {volume}");
        Ok(())
    }

    #[test]
    fn editing_an_earlier_feature_invalidates_every_later_prefix() -> TestResult {
        let mut project = isolated_last_feature_project()?;
        let mut kernel = JoinKernel::new();
        let mut cache = BuildCache::new();
        project.build_with_cache(&mut kernel, &mut cache)?;

        // `width` sizes both sketches, so both features' resolved profiles change.
        kernel.calls.clear();
        project.set_parameter("width", 20.0)?;
        let build = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(build.is_complete());
        assert_eq!(extrudes(&kernel), 2, "every dependent feature must rebuild");
        assert_eq!(kernel.unions(), 1);
        let volume = build.mesh.ok_or("mesh")?.signed_volume();
        // 20 x 10 x 5 plus 20 x 10 x 10 = 1000 + 2000.
        assert!((volume - 3000.0).abs() < 1e-9, "volume was {volume}");

        // Undo restores the previous inputs, whose prefixes are still cached, so the rebuild is
        // free again. That is why entries from earlier revisions survive the prune.
        project.undo()?;
        kernel.calls.clear();
        let restored = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(kernel.calls.is_empty(), "calls: {:?}", kernel.calls);
        assert!((restored.mesh.ok_or("mesh")?.signed_volume() - 1500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_cached_rebuild_still_reports_a_failing_feature() -> TestResult {
        let project = isolated_last_feature_project()?;
        let mut kernel = JoinKernel::failing();
        let mut cache = BuildCache::new();

        let first = project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(!first.is_complete());
        let message = first
            .failed
            .as_ref()
            .map(|failure| failure.message.clone())
            .ok_or("a failed feature must be reported")?;
        assert!(message.contains("injected"), "{message}");
        assert_eq!(
            cache.len(),
            1,
            "only the feature that did rebuild is cacheable"
        );

        // The failure is reproduced rather than cached away, and the good prefix is still reused.
        let second = project.build_with_cache(&mut kernel, &mut cache)?;
        assert_eq!(second.failed.map(|failure| failure.message), Some(message));
        assert_eq!(cache.hits(), 1);
        Ok(())
    }

    #[test]
    fn a_cache_is_discarded_when_the_kernel_instance_changes() -> TestResult {
        let project = isolated_last_feature_project()?;
        let mut first_kernel = JoinKernel::new();
        let mut cache = BuildCache::new();
        project.build_with_cache(&mut first_kernel, &mut cache)?;
        assert_eq!(cache.len(), 2);

        // Same kernel type, different instance: the identity matches but the handles do not exist.
        let mut second_kernel = JoinKernel::new();
        let build = project.build_with_cache(&mut second_kernel, &mut cache)?;
        assert!(build.is_complete());
        assert_eq!(
            extrudes(&second_kernel),
            2,
            "a fresh kernel must rebuild rather than adopt stale handles"
        );
        Ok(())
    }

    #[test]
    fn clearing_the_cache_releases_every_handle_it_owns() -> TestResult {
        let project = isolated_last_feature_project()?;
        let mut kernel = JoinKernel::new();
        let mut cache = BuildCache::new();
        project.build_with_cache(&mut kernel, &mut cache)?;
        assert!(!kernel.volumes.is_empty());
        cache.clear(&mut kernel);
        assert!(cache.is_empty());
        assert!(
            kernel.volumes.is_empty(),
            "every cached solid must be released"
        );
        Ok(())
    }

    #[test]
    fn a_cached_build_and_an_uncached_build_agree() -> TestResult {
        let mut project = isolated_last_feature_project()?;
        project.set_parameter("width", 14.0)?;
        project.set_parameter("diameter", 6.0)?;

        let mut cold = JoinKernel::new();
        let uncached = project.build(&mut cold)?;

        let mut warm_kernel = JoinKernel::new();
        let mut cache = BuildCache::new();
        // Two builds so the second is served entirely from the cache.
        project.build_with_cache(&mut warm_kernel, &mut cache)?;
        let cached = project.build_with_cache(&mut warm_kernel, &mut cache)?;

        assert_eq!(uncached.reports, cached.reports);
        assert_eq!(
            uncached.mesh.map(|mesh| mesh.signed_volume()),
            cached.mesh.map(|mesh| mesh.signed_volume())
        );
        Ok(())
    }

    // -----------------------------------------------------------------------------------------
    // Unsaved-edit recovery
    // -----------------------------------------------------------------------------------------

    #[test]
    fn an_unsaved_edit_is_offered_from_its_snapshot_and_the_file_is_untouched() -> TestResult {
        let dir = TempDir::new("recovery-offer")?;
        let path = dir.0.join("part.zcad");
        let mut project = box_project()?;
        project.save(&path)?;

        project.set_parameter("depth", 8.0)?;
        let saved_at = project.save_recovery(&path)?;

        let (recovered, info) = open_with_recovery(&path)?;
        let info = info.ok_or("a snapshot from the same file version must be offered")?;
        assert_eq!(info.saved_at_unix, saved_at);
        assert!(!info.file_unreadable);
        let mut kernel = BoxKernel::new();
        assert!(
            (recovered.evaluate(&mut kernel)?.volume_mm3 - 800.0).abs() < 1e-9,
            "the recovered project must be the edited one"
        );

        // The snapshot never writes the project file: that stays the last explicit save.
        let on_disk = Project::load(&path)?;
        let mut kernel = BoxKernel::new();
        assert!((on_disk.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn a_snapshot_superseded_by_a_save_is_deleted_rather_than_offered() -> TestResult {
        let dir = TempDir::new("recovery-stale")?;
        let path = dir.0.join("part.zcad");
        let mut project = box_project()?;
        project.save(&path)?;

        // An unsaved edit leaves a snapshot of an 800 mm3 body behind...
        project.set_parameter("depth", 8.0)?;
        project.save_recovery(&path)?;

        // ...then the user saves a different edit (600 mm3), which makes the snapshot history.
        // The pause is what makes the file's mtime advance past the snapshot's recorded base; the
        // comparison is by file version, not by clock, so the rule itself does not need it.
        std::thread::sleep(std::time::Duration::from_millis(1100));
        project.set_parameter("depth", 6.0)?;
        project.save(&path)?;

        let (loaded, info) = open_with_recovery(&path)?;
        assert!(info.is_none(), "a stale snapshot must not be offered");
        assert!(
            !recovery_path(&path).exists(),
            "a stale snapshot is removed so it is not offered again"
        );
        let mut kernel = BoxKernel::new();
        assert!(
            (loaded.evaluate(&mut kernel)?.volume_mm3 - 600.0).abs() < 1e-9,
            "the saved file must win over the older snapshot"
        );
        Ok(())
    }

    #[test]
    fn a_corrupt_snapshot_is_ignored_instead_of_blocking_the_project() -> TestResult {
        let dir = TempDir::new("recovery-corrupt")?;
        let path = dir.0.join("part.zcad");
        let project = box_project()?;
        project.save(&path)?;
        std::fs::write(recovery_path(&path), b"{ not a snapshot")?;

        let (loaded, info) = open_with_recovery(&path)?;
        assert!(info.is_none());
        let mut kernel = BoxKernel::new();
        assert!((loaded.evaluate(&mut kernel)?.volume_mm3 - 500.0).abs() < 1e-9);
        Ok(())
    }

    #[test]
    fn an_unreadable_project_falls_back_to_its_snapshot() -> TestResult {
        let dir = TempDir::new("recovery-unreadable")?;
        let path = dir.0.join("part.zcad");
        let mut project = box_project()?;
        project.save(&path)?;
        project.set_parameter("depth", 8.0)?;
        project.save_recovery(&path)?;

        // A crash during the write leaves a truncated file behind.
        std::fs::write(&path, b"{\"schema\":5,\"name\":\"brac")?;

        let (recovered, info) = open_with_recovery(&path)?;
        let info = info.ok_or("the snapshot is the only readable copy")?;
        assert!(info.file_unreadable);
        let mut kernel = BoxKernel::new();
        assert!((recovered.evaluate(&mut kernel)?.volume_mm3 - 800.0).abs() < 1e-9);
        Ok(())
    }
}

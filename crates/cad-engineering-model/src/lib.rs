//! Phase A of the unified engineering model.
//!
//! This crate is the shared identity and dependency layer that a CAD, CAM, CAE or ECAD workspace
//! uses to answer one question honestly: **which derived result belongs to which design revision?**
//!
//! It does not create geometry, meshes, toolpaths or schematics. It records the facts those
//! workspaces must share:
//!
//! * a [`DocumentRef`] is a stable document identity plus a revision and an optional content
//!   fingerprint. A revision alone is enough for the normal save/load path; the fingerprint catches
//!   the case where a file is replaced or restored at the same revision number.
//! * [`Material`] and [`MaterialLibrary`] hold validated, attributable property values. The built-in
//!   library is explicitly a set of typical handbook values, not a certified material database.
//! * [`DerivedArtifact`] pins a drawing, simulation, manufacturing setup, toolpath, PCB layout or
//!   report to the revision it was generated from, the design entities it read, and the artifacts it
//!   depends on.
//! * [`ArtifactRegistry::assess`] reports, for every artifact, whether it is current, needs a
//!   recompute, needs review, or is blocked by an upstream artifact. The propagation is transitive
//!   and computed over an acyclic dependency graph.
//!
//! # What is deliberately not here
//!
//! The registry stores a result **summary and warnings**, not the solver output, toolpath or mesh.
//! Those payloads belong to their producing workspace; duplicating them here would create a second
//! source of truth. The registry is the index that says whether the payload may be trusted for the
//! current revision.
//!
//! Staleness is not silently resolved. A source change marks an artifact as requiring review or
//! recomputation and keeps the previous result reference intact. [`ArtifactRegistry::review`]
//! records an explicit human acknowledgment at a revision; it does not rewrite the result as if it
//! had been recomputed, and [`ArtifactStatus::is_current`] stays `false` for it.
//!
//! # Format
//!
//! The registry is versioned JSON (`schema: 1`). Unknown fields, a newer schema, a duplicate ID, an
//! unknown upstream reference, a cycle and an oversized file are errors, matching the project
//! document's fail-closed rules.
//!
//! # Example
//!
//! ```
//! use cad_contract::document::Revision;
//! use cad_contract::ids::DocumentId;
//! use cad_engineering_model::{
//!     ArtifactId, ArtifactKind, ArtifactRegistry, DerivedArtifact, DocumentRef,
//! };
//!
//! let document = DocumentId::new(7);
//! let mut registry = ArtifactRegistry::new(document);
//! let design_at_4 = DocumentRef::new(document, Revision::from_raw(4).unwrap(), None);
//! let drawing = DerivedArtifact::new(
//!     ArtifactId::new(1).unwrap(),
//!     ArtifactKind::Drawing,
//!     "shop drawing",
//!     design_at_4,
//! );
//! registry.add(drawing)?;
//!
//! // The design has moved on to revision 5.
//! let current = DocumentRef::new(document, Revision::from_raw(5).unwrap(), None);
//! let report = registry.assess(current)?;
//! assert!(!report.artifacts[0].status.is_current());
//! assert!(report.requires_action);
//! # Ok::<(), cad_engineering_model::ModelError>(())
//! ```
//!
//! The example uses `unwrap` deliberately: a doctest is a test, and `Revision::from_raw` only fails
//! for zero.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};

use cad_contract::document::Revision;
use cad_contract::ids::{DocumentId, EntityId};
use serde::{Deserialize, Serialize};

/// The registry schema this build writes.
pub const MODEL_SCHEMA: u32 = 1;

/// Largest registry file accepted before parsing, in bytes.
pub const MAX_MODEL_BYTES: u64 = 8 * 1024 * 1024;

/// Most derived artifacts one registry may hold.
pub const MAX_ARTIFACTS: usize = 4096;

/// Longest artifact label, in characters.
pub const MAX_LABEL_CHARS: usize = 200;

/// Most direct upstream dependencies one artifact may declare.
pub const MAX_UPSTREAM: usize = 64;

/// Longest result summary, in characters.
pub const MAX_SUMMARY_CHARS: usize = 2000;

/// Most warnings one artifact result may record.
pub const MAX_WARNINGS: usize = 64;

/// Longest single warning, in characters.
pub const MAX_WARNING_CHARS: usize = 500;

/// Most materials one library may hold.
pub const MAX_MATERIALS: usize = 256;

/// Why the engineering model rejected an operation or a file.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ModelError {
    #[error("{0}")]
    Invalid(String),
    #[error("engineering-model schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("engineering model is {bytes} bytes, over the {limit}-byte limit")]
    TooLarge { bytes: u64, limit: u64 },
    #[error("artifact {0} is recorded twice")]
    DuplicateArtifact(u64),
    #[error("artifact {0} is not in the registry")]
    UnknownArtifact(u64),
    #[error("artifact {0} names an upstream artifact that is not recorded")]
    UnknownUpstream(u64),
    #[error("artifact {0} participates in a dependency cycle")]
    Cycle(u64),
    #[error(
        "registry belongs to document {expected}, but the artifact or reader names document {found}"
    )]
    WrongDocument { expected: u64, found: u64 },
    #[error("not valid JSON: {0}")]
    Json(String),
    #[error("I/O at {path}: {message}")]
    Io { path: String, message: String },
}

/// A content fingerprint for a design revision.
///
/// A revision number is the primary identity: a document that has been edited must bump it. The
/// fingerprint is the secondary check for the cases a number cannot catch by itself — a project file
/// restored from a backup, copied over, or hand-edited with its revision left alone. It is a 64-bit
/// FNV-1a hash, cheap and deterministic, and is not a cryptographic integrity guarantee.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Fingerprint(u64);

impl Fingerprint {
    /// Hash content deterministically with FNV-1a.
    pub fn of_bytes(bytes: &[u8]) -> Self {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        for byte in bytes {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
        Fingerprint(hash)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// A stable document identity plus the revision a value was derived from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentRef {
    /// Stable document identity, independent of the file name.
    pub document: DocumentId,
    /// The revision the value was generated from.
    pub revision: Revision,
    /// Optional content hash of the document at that revision.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fingerprint: Option<Fingerprint>,
}

impl DocumentRef {
    pub const fn new(
        document: DocumentId,
        revision: Revision,
        fingerprint: Option<Fingerprint>,
    ) -> Self {
        DocumentRef {
            document,
            revision,
            fingerprint,
        }
    }

    fn validate(&self) -> Result<(), ModelError> {
        if Revision::from_raw(self.revision.get()).is_none() {
            return Err(ModelError::Invalid(
                "a document reference needs a positive revision".into(),
            ));
        }
        Ok(())
    }
}

/// What kind of engineering artifact a record describes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArtifactKind {
    Drawing,
    Simulation,
    ManufacturingSetup,
    Toolpath,
    Inspection,
    PcbLayout,
    Documentation,
    Other,
}

impl ArtifactKind {
    /// The stable lowercase name used in JSON and in the CLI.
    pub const fn label(self) -> &'static str {
        match self {
            ArtifactKind::Drawing => "drawing",
            ArtifactKind::Simulation => "simulation",
            ArtifactKind::ManufacturingSetup => "manufacturing_setup",
            ArtifactKind::Toolpath => "toolpath",
            ArtifactKind::Inspection => "inspection",
            ArtifactKind::PcbLayout => "pcb_layout",
            ArtifactKind::Documentation => "documentation",
            ArtifactKind::Other => "other",
        }
    }

    /// Parse the name written by [`ArtifactKind::label`].
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim().to_ascii_lowercase();
        let kind = match value.as_str() {
            "drawing" => ArtifactKind::Drawing,
            "simulation" => ArtifactKind::Simulation,
            "manufacturing_setup" | "manufacturing" => ArtifactKind::ManufacturingSetup,
            "toolpath" => ArtifactKind::Toolpath,
            "inspection" => ArtifactKind::Inspection,
            "pcb_layout" | "pcb" => ArtifactKind::PcbLayout,
            "documentation" | "document" => ArtifactKind::Documentation,
            "other" => ArtifactKind::Other,
            _ => return None,
        };
        Some(kind)
    }
}

/// A material category. The category is metadata, not a solver model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialCategory {
    Metal,
    Polymer,
    Ceramic,
    Composite,
    Electronic,
    Other,
}

/// A material definition with attributable, validated property values.
///
/// Properties are in SI-adjacent engineering units: density in kg/m³, moduli and strengths in MPa,
/// conductivity in W/(m·K), and service temperature in °C. A property may be absent when the library
/// does not have a defensible value; absent is not zero.
///
/// The built-in values are typical handbook figures for preliminary engineering. They are **not** a
/// certified material specification: a real analysis must use the supplier's data or a qualified
/// material database.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Material {
    pub id: String,
    pub name: String,
    pub category: MaterialCategory,
    pub density_kg_m3: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub youngs_modulus_mpa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub poisson_ratio: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yield_strength_mpa: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub thermal_conductivity_w_m_k: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub max_service_temperature_c: Option<f64>,
    /// Where the values came from, and how far they may be relied on.
    pub source: String,
}

impl Material {
    /// Check every property for a physically meaningful value and every string for a safe size.
    pub fn validate(&self) -> Result<(), ModelError> {
        let id = self.id.trim();
        if id.is_empty() || id.chars().count() > 64 {
            return Err(ModelError::Invalid(
                "a material id must be 1..=64 characters".into(),
            ));
        }
        if !id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_'))
        {
            return Err(ModelError::Invalid(format!(
                "material id `{id}` may contain only ASCII letters, digits, `.`, `-` and `_`"
            )));
        }
        if self.name.trim().is_empty() || self.name.chars().count() > 120 {
            return Err(ModelError::Invalid(format!(
                "material `{id}` needs a name of 1..=120 characters"
            )));
        }
        if self.source.trim().is_empty() || self.source.chars().count() > 300 {
            return Err(ModelError::Invalid(format!(
                "material `{id}` needs a source of 1..=300 characters"
            )));
        }
        require_finite_positive(self.density_kg_m3, "density", id)?;
        if self.density_kg_m3 > 30_000.0 {
            return Err(ModelError::Invalid(format!(
                "material `{id}` has a density above 30 000 kg/m³; check the unit"
            )));
        }
        if let Some(value) = self.youngs_modulus_mpa {
            require_finite_positive(value, "Young's modulus", id)?;
        }
        if let Some(value) = self.poisson_ratio
            && (!value.is_finite() || !(-1.0..0.5).contains(&value))
        {
            return Err(ModelError::Invalid(format!(
                "material `{id}` has Poisson ratio {value}; it must be finite and in (-1, 0.5)"
            )));
        }
        if let Some(value) = self.yield_strength_mpa {
            require_finite_positive(value, "yield strength", id)?;
        }
        if let Some(value) = self.thermal_conductivity_w_m_k
            && (!value.is_finite() || value < 0.0)
        {
            return Err(ModelError::Invalid(format!(
                "material `{id}` has a thermal conductivity that is not a finite non-negative value"
            )));
        }
        if let Some(value) = self.max_service_temperature_c
            && !value.is_finite()
        {
            return Err(ModelError::Invalid(format!(
                "material `{id}` has a non-finite service temperature"
            )));
        }
        Ok(())
    }
}

fn require_finite_positive(value: f64, property: &str, id: &str) -> Result<(), ModelError> {
    if !value.is_finite() || value <= 0.0 {
        return Err(ModelError::Invalid(format!(
            "material `{id}` has a {property} that is not a finite positive value"
        )));
    }
    Ok(())
}

/// A validated collection of materials, addressable by stable id.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct MaterialLibrary {
    materials: Vec<Material>,
}

impl MaterialLibrary {
    /// Build a library after validating every entry and rejecting duplicate ids.
    pub fn new(materials: Vec<Material>) -> Result<Self, ModelError> {
        let library = MaterialLibrary { materials };
        library.validate()?;
        Ok(library)
    }

    /// The small built-in library used by the application and the CLI.
    ///
    /// Every number is a typical room-temperature handbook value. The source string on each entry
    /// says so; do not read this as a certified material database.
    pub fn standard() -> Self {
        const SOURCE: &str = "Typical handbook value for preliminary engineering; not a certified \
                              material specification and not a substitute for supplier data";
        MaterialLibrary {
            materials: vec![
                Material {
                    id: "steel.1018".into(),
                    name: "AISI 1018 cold-drawn steel".into(),
                    category: MaterialCategory::Metal,
                    density_kg_m3: 7870.0,
                    youngs_modulus_mpa: Some(205_000.0),
                    poisson_ratio: Some(0.29),
                    yield_strength_mpa: Some(370.0),
                    thermal_conductivity_w_m_k: Some(51.9),
                    max_service_temperature_c: Some(540.0),
                    source: SOURCE.into(),
                },
                Material {
                    id: "aluminum.6061-t6".into(),
                    name: "6061-T6 aluminium".into(),
                    category: MaterialCategory::Metal,
                    density_kg_m3: 2700.0,
                    youngs_modulus_mpa: Some(68_900.0),
                    poisson_ratio: Some(0.33),
                    yield_strength_mpa: Some(276.0),
                    thermal_conductivity_w_m_k: Some(167.0),
                    max_service_temperature_c: Some(150.0),
                    source: SOURCE.into(),
                },
                Material {
                    id: "titanium.ti-6al-4v".into(),
                    name: "Ti-6Al-4V titanium alloy".into(),
                    category: MaterialCategory::Metal,
                    density_kg_m3: 4430.0,
                    youngs_modulus_mpa: Some(113_800.0),
                    poisson_ratio: Some(0.342),
                    yield_strength_mpa: Some(880.0),
                    thermal_conductivity_w_m_k: Some(6.7),
                    max_service_temperature_c: Some(400.0),
                    source: SOURCE.into(),
                },
                Material {
                    id: "copper.c11000".into(),
                    name: "C11000 electrolytic copper".into(),
                    category: MaterialCategory::Metal,
                    density_kg_m3: 8960.0,
                    youngs_modulus_mpa: Some(117_000.0),
                    poisson_ratio: Some(0.34),
                    yield_strength_mpa: Some(70.0),
                    thermal_conductivity_w_m_k: Some(401.0),
                    max_service_temperature_c: Some(200.0),
                    source: SOURCE.into(),
                },
                Material {
                    id: "abs.general".into(),
                    name: "General-purpose ABS".into(),
                    category: MaterialCategory::Polymer,
                    density_kg_m3: 1040.0,
                    youngs_modulus_mpa: Some(2300.0),
                    poisson_ratio: Some(0.35),
                    yield_strength_mpa: Some(40.0),
                    thermal_conductivity_w_m_k: Some(0.18),
                    max_service_temperature_c: Some(80.0),
                    source: SOURCE.into(),
                },
                Material {
                    id: "pla.general".into(),
                    name: "General-purpose PLA".into(),
                    category: MaterialCategory::Polymer,
                    density_kg_m3: 1240.0,
                    youngs_modulus_mpa: Some(3500.0),
                    poisson_ratio: Some(0.36),
                    yield_strength_mpa: Some(60.0),
                    thermal_conductivity_w_m_k: Some(0.13),
                    max_service_temperature_c: Some(60.0),
                    source: SOURCE.into(),
                },
                Material {
                    id: "fr4.standard".into(),
                    name: "FR-4 woven-glass epoxy laminate".into(),
                    category: MaterialCategory::Electronic,
                    density_kg_m3: 1850.0,
                    youngs_modulus_mpa: Some(24_000.0),
                    poisson_ratio: Some(0.15),
                    yield_strength_mpa: None,
                    thermal_conductivity_w_m_k: Some(0.3),
                    max_service_temperature_c: Some(130.0),
                    source: SOURCE.into(),
                },
            ],
        }
    }

    pub fn validate(&self) -> Result<(), ModelError> {
        if self.materials.len() > MAX_MATERIALS {
            return Err(ModelError::Invalid(format!(
                "a material library is limited to {MAX_MATERIALS} entries"
            )));
        }
        let mut ids = BTreeSet::new();
        for material in &self.materials {
            material.validate()?;
            if !ids.insert(material.id.to_ascii_lowercase()) {
                return Err(ModelError::Invalid(format!(
                    "material id `{}` is used twice",
                    material.id
                )));
            }
        }
        Ok(())
    }

    pub fn get(&self, id: &str) -> Option<&Material> {
        self.materials
            .iter()
            .find(|material| material.id.eq_ignore_ascii_case(id.trim()))
    }

    pub fn materials(&self) -> &[Material] {
        &self.materials
    }

    pub fn len(&self) -> usize {
        self.materials.len()
    }

    pub fn is_empty(&self) -> bool {
        self.materials.is_empty()
    }
}

/// A result summary attached to a derived artifact.
///
/// This is metadata, not the result itself. A solver's field data, a toolpath's moves and a
/// drawing's sheet stay in their producing workspace; this record says which input revision they
/// belong to and what the producer reported when it made them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactResult {
    #[serde(default)]
    pub summary: String,
    #[serde(default)]
    pub warnings: Vec<String>,
    /// Unix seconds at which the producer wrote the result.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub produced_at_unix: Option<u64>,
}

impl ArtifactResult {
    fn validate(&self) -> Result<(), ModelError> {
        if self.summary.chars().count() > MAX_SUMMARY_CHARS {
            return Err(ModelError::Invalid(format!(
                "a result summary is limited to {MAX_SUMMARY_CHARS} characters"
            )));
        }
        if self.warnings.len() > MAX_WARNINGS {
            return Err(ModelError::Invalid(format!(
                "an artifact result is limited to {MAX_WARNINGS} warnings"
            )));
        }
        if self
            .warnings
            .iter()
            .any(|warning| warning.chars().count() > MAX_WARNING_CHARS)
        {
            return Err(ModelError::Invalid(format!(
                "a warning is limited to {MAX_WARNING_CHARS} characters"
            )));
        }
        Ok(())
    }
}

/// A result derived from a specific design revision.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DerivedArtifact {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub label: String,
    /// The design revision this artifact was produced from.
    pub source: DocumentRef,
    /// Design entities the producer read. Optional because not every producer records them yet.
    #[serde(default)]
    pub source_entities: Vec<EntityId>,
    /// Artifacts whose results this artifact was derived from.
    #[serde(default)]
    pub upstream: Vec<ArtifactId>,
    #[serde(default)]
    pub result: ArtifactResult,
    /// The revision at which a human explicitly accepted this artifact despite staleness.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reviewed_revision: Option<Revision>,
}

impl DerivedArtifact {
    /// A record with no summary, warnings, entities or upstream dependencies.
    pub fn new(
        id: ArtifactId,
        kind: ArtifactKind,
        label: impl Into<String>,
        source: DocumentRef,
    ) -> Self {
        DerivedArtifact {
            id,
            kind,
            label: label.into(),
            source,
            source_entities: Vec::new(),
            upstream: Vec::new(),
            result: ArtifactResult::default(),
            reviewed_revision: None,
        }
    }

    fn validate(&self) -> Result<(), ModelError> {
        if self.id.get() == 0 {
            return Err(ModelError::Invalid(
                "an artifact id of 0 is not valid".into(),
            ));
        }
        let label = self.label.trim();
        if label.is_empty() || label.chars().count() > MAX_LABEL_CHARS {
            return Err(ModelError::Invalid(format!(
                "artifact {} needs a label of 1..={MAX_LABEL_CHARS} characters",
                self.id.get()
            )));
        }
        self.source.validate()?;
        if self.upstream.len() > MAX_UPSTREAM {
            return Err(ModelError::Invalid(format!(
                "artifact {} declares more than {MAX_UPSTREAM} upstream artifacts",
                self.id.get()
            )));
        }
        let mut seen = BTreeSet::new();
        for upstream in &self.upstream {
            if upstream.get() == self.id.get() {
                return Err(ModelError::Cycle(self.id.get()));
            }
            if !seen.insert(upstream.get()) {
                return Err(ModelError::Invalid(format!(
                    "artifact {} names upstream artifact {} twice",
                    self.id.get(),
                    upstream.get()
                )));
            }
        }
        self.result.validate()?;
        if let Some(revision) = self.reviewed_revision {
            if Revision::from_raw(revision.get()).is_none() {
                return Err(ModelError::Invalid(format!(
                    "artifact {} records a zero review revision",
                    self.id.get()
                )));
            }
            if revision < self.source.revision {
                return Err(ModelError::Invalid(format!(
                    "artifact {} was reviewed at revision {} but derived from a later revision {}",
                    self.id.get(),
                    revision.get(),
                    self.source.revision.get()
                )));
            }
        }
        Ok(())
    }
}

/// A stable artifact identity. Zero is not valid, so a defaulted field cannot alias a real record.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ArtifactId(u64);

impl ArtifactId {
    pub const fn new(raw: u64) -> Option<Self> {
        if raw == 0 {
            None
        } else {
            Some(ArtifactId(raw))
        }
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

/// The relationship between one artifact and the document's current revision.
///
/// A reviewed artifact is **not** current: its result still came from an older revision. It is
/// releasable because a person acknowledged that fact, which is different from a recomputed result
/// and is reported as such.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(tag = "state", rename_all = "snake_case")]
pub enum ArtifactStatus {
    /// Derived from the current revision.
    Current,
    /// The document content changed without a revision change; the result cannot be trusted.
    RequiresRecompute {
        source_revision: u64,
        current_revision: u64,
    },
    /// Derived from an older revision and not yet acknowledged.
    RequiresReview {
        source_revision: u64,
        current_revision: u64,
    },
    /// Derived from an older revision and explicitly acknowledged at the current revision.
    AcceptedAfterReview {
        source_revision: u64,
        current_revision: u64,
        reviewed_revision: u64,
    },
    /// The artifact itself is current, but an upstream artifact is not current or releasable.
    BlockedByUpstream {
        upstream: u64,
        source_revision: u64,
        current_revision: u64,
    },
    /// The artifact records a revision newer than the document supplied; the pair is inconsistent.
    FutureRevision {
        source_revision: u64,
        current_revision: u64,
    },
}

impl ArtifactStatus {
    /// True only when the artifact was derived from the current revision.
    pub const fn is_current(self) -> bool {
        matches!(self, ArtifactStatus::Current)
    }

    /// True when the artifact may be used downstream without further validation.
    ///
    /// A reviewed artifact qualifies: the user has seen that it is stale and accepted it. A current
    /// or reviewed artifact whose upstream is blocked does not qualify.
    pub const fn is_releasable(self) -> bool {
        matches!(
            self,
            ArtifactStatus::Current | ArtifactStatus::AcceptedAfterReview { .. }
        )
    }

    /// True when the artifact needs a recompute or a review before release.
    pub const fn requires_action(self) -> bool {
        !self.is_releasable()
    }

    /// A stable lowercase name for reports.
    pub const fn label(self) -> &'static str {
        match self {
            ArtifactStatus::Current => "current",
            ArtifactStatus::RequiresRecompute { .. } => "requires_recompute",
            ArtifactStatus::RequiresReview { .. } => "requires_review",
            ArtifactStatus::AcceptedAfterReview { .. } => "accepted_after_review",
            ArtifactStatus::BlockedByUpstream { .. } => "blocked_by_upstream",
            ArtifactStatus::FutureRevision { .. } => "future_revision",
        }
    }
}

/// One artifact's assessment against a supplied current revision.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ArtifactAssessment {
    pub id: ArtifactId,
    pub kind: ArtifactKind,
    pub label: String,
    pub source: DocumentRef,
    pub status: ArtifactStatus,
    /// True when this artifact may be released with the supplied revision.
    pub releasable: bool,
}

/// The result of assessing a whole registry.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct StalenessReport {
    pub document: DocumentId,
    pub revision: Revision,
    pub artifacts: Vec<ArtifactAssessment>,
    /// True when any artifact needs a recompute, a review, or is blocked.
    pub requires_action: bool,
}

/// The persisted index of derived artifacts for one document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ArtifactRegistry {
    pub schema: u32,
    pub document: DocumentId,
    artifacts: Vec<DerivedArtifact>,
}

impl ArtifactRegistry {
    /// An empty registry for `document` at the current schema.
    pub fn new(document: DocumentId) -> Self {
        ArtifactRegistry {
            schema: MODEL_SCHEMA,
            document,
            artifacts: Vec::new(),
        }
    }

    /// Add an artifact. The whole registry is revalidated, so a bad reference cannot be stored.
    pub fn add(&mut self, artifact: DerivedArtifact) -> Result<(), ModelError> {
        if self
            .artifacts
            .iter()
            .any(|existing| existing.id == artifact.id)
        {
            return Err(ModelError::DuplicateArtifact(artifact.id.get()));
        }
        self.artifacts.push(artifact);
        if let Err(error) = self.validate() {
            self.artifacts.pop();
            return Err(error);
        }
        Ok(())
    }

    /// Remove an artifact. An artifact that other artifacts depend on cannot be removed.
    pub fn remove(&mut self, id: ArtifactId) -> Result<(), ModelError> {
        let referenced = self
            .artifacts
            .iter()
            .find(|artifact| artifact.id != id && artifact.upstream.contains(&id));
        if let Some(dependent) = referenced {
            return Err(ModelError::Invalid(format!(
                "artifact {} still depends on artifact {}",
                dependent.id.get(),
                id.get()
            )));
        }
        let before = self.artifacts.len();
        self.artifacts.retain(|artifact| artifact.id != id);
        if self.artifacts.len() == before {
            return Err(ModelError::UnknownArtifact(id.get()));
        }
        Ok(())
    }

    /// Record that a human accepted the artifact's staleness at `revision`.
    ///
    /// This does not alter the artifact's source revision or claim that the result was recomputed.
    /// Reviewing an older revision does not acknowledge a newer one: the status compares the review
    /// revision with the current document revision.
    pub fn review(&mut self, id: ArtifactId, revision: Revision) -> Result<(), ModelError> {
        if Revision::from_raw(revision.get()).is_none() {
            return Err(ModelError::Invalid(
                "review revision must be positive".into(),
            ));
        }
        let artifact = self
            .artifacts
            .iter_mut()
            .find(|artifact| artifact.id == id)
            .ok_or(ModelError::UnknownArtifact(id.get()))?;
        if revision < artifact.source.revision {
            return Err(ModelError::Invalid(format!(
                "artifact {} was derived from revision {}; a review cannot predate it",
                id.get(),
                artifact.source.revision.get()
            )));
        }
        artifact.reviewed_revision = Some(match artifact.reviewed_revision {
            Some(existing) if existing > revision => existing,
            _ => revision,
        });
        self.validate()
    }

    pub fn artifacts(&self) -> &[DerivedArtifact] {
        &self.artifacts
    }

    pub fn get(&self, id: ArtifactId) -> Option<&DerivedArtifact> {
        self.artifacts.iter().find(|artifact| artifact.id == id)
    }

    /// Validate the schema, identities, references and the acyclic dependency graph.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema != MODEL_SCHEMA {
            return Err(ModelError::UnsupportedSchema {
                found: self.schema,
                supported: MODEL_SCHEMA,
            });
        }
        if self.artifacts.len() > MAX_ARTIFACTS {
            return Err(ModelError::Invalid(format!(
                "a registry is limited to {MAX_ARTIFACTS} artifacts"
            )));
        }
        let mut ids = BTreeSet::new();
        for artifact in &self.artifacts {
            artifact.validate()?;
            if artifact.source.document != self.document {
                return Err(ModelError::WrongDocument {
                    expected: self.document.get(),
                    found: artifact.source.document.get(),
                });
            }
            if !ids.insert(artifact.id.get()) {
                return Err(ModelError::DuplicateArtifact(artifact.id.get()));
            }
        }
        for artifact in &self.artifacts {
            for upstream in &artifact.upstream {
                if !ids.contains(&upstream.get()) {
                    return Err(ModelError::UnknownUpstream(upstream.get()));
                }
            }
        }
        // Kahn's algorithm: an iterative cycle check that cannot overflow the stack on a large
        // registry. `topological_order` repeats the work with indices, which keeps assessment simple.
        let _ = self.topological_order()?;
        Ok(())
    }

    /// Topological order of artifact indices, with upstream artifacts before their dependents.
    fn topological_order(&self) -> Result<Vec<usize>, ModelError> {
        let index: BTreeMap<u64, usize> = self
            .artifacts
            .iter()
            .enumerate()
            .map(|(position, artifact)| (artifact.id.get(), position))
            .collect();
        if index.len() != self.artifacts.len() {
            // Duplicates are reported by `validate` before this function matters; find one here so
            // the error is still specific if called directly.
            let mut seen = BTreeSet::new();
            for artifact in &self.artifacts {
                if !seen.insert(artifact.id.get()) {
                    return Err(ModelError::DuplicateArtifact(artifact.id.get()));
                }
            }
        }
        let mut indegree = vec![0_usize; self.artifacts.len()];
        let mut dependents: Vec<Vec<usize>> = vec![Vec::new(); self.artifacts.len()];
        for (position, artifact) in self.artifacts.iter().enumerate() {
            for upstream in &artifact.upstream {
                let parent = index
                    .get(&upstream.get())
                    .copied()
                    .ok_or(ModelError::UnknownUpstream(upstream.get()))?;
                if parent == position {
                    return Err(ModelError::Cycle(artifact.id.get()));
                }
                indegree[position] += 1;
                dependents[parent].push(position);
            }
        }
        let mut queue: VecDeque<usize> = indegree
            .iter()
            .enumerate()
            .filter(|(_, degree)| **degree == 0)
            .map(|(position, _)| position)
            .collect();
        let mut order = Vec::with_capacity(self.artifacts.len());
        while let Some(position) = queue.pop_front() {
            order.push(position);
            for &dependent in &dependents[position] {
                indegree[dependent] -= 1;
                if indegree[dependent] == 0 {
                    queue.push_back(dependent);
                }
            }
        }
        if order.len() != self.artifacts.len() {
            let stuck = indegree
                .iter()
                .position(|degree| *degree > 0)
                .and_then(|position| self.artifacts.get(position))
                .map(|artifact| artifact.id.get())
                .unwrap_or(0);
            return Err(ModelError::Cycle(stuck));
        }
        Ok(order)
    }

    /// Assess every artifact against `current`.
    ///
    /// The report keeps the previous result references; it changes no artifact, so nothing is ever
    /// silently promoted to current. An artifact whose source is current but whose upstream needs
    /// action is reported as [`ArtifactStatus::BlockedByUpstream`].
    pub fn assess(&self, current: DocumentRef) -> Result<StalenessReport, ModelError> {
        self.validate()?;
        if current.document != self.document {
            return Err(ModelError::WrongDocument {
                expected: self.document.get(),
                found: current.document.get(),
            });
        }
        if Revision::from_raw(current.revision.get()).is_none() {
            return Err(ModelError::Invalid(
                "the current document revision must be positive".into(),
            ));
        }
        let order = self.topological_order()?;
        let mut statuses: BTreeMap<u64, ArtifactStatus> = BTreeMap::new();
        for position in order {
            let Some(artifact) = self.artifacts.get(position) else {
                continue;
            };
            let mut status = source_status(artifact, current);
            if status.is_releasable()
                && let Some(upstream) = artifact.upstream.iter().find(|upstream| {
                    statuses
                        .get(&upstream.get())
                        .is_some_and(|status| status.requires_action())
                })
            {
                status = ArtifactStatus::BlockedByUpstream {
                    upstream: upstream.get(),
                    source_revision: artifact.source.revision.get(),
                    current_revision: current.revision.get(),
                };
            }
            statuses.insert(artifact.id.get(), status);
        }
        let artifacts = self
            .artifacts
            .iter()
            .map(|artifact| {
                let status = statuses.get(&artifact.id.get()).copied().unwrap_or(
                    ArtifactStatus::RequiresReview {
                        source_revision: artifact.source.revision.get(),
                        current_revision: current.revision.get(),
                    },
                );
                ArtifactAssessment {
                    id: artifact.id,
                    kind: artifact.kind,
                    label: artifact.label.clone(),
                    source: artifact.source,
                    status,
                    releasable: status.is_releasable(),
                }
            })
            .collect::<Vec<_>>();
        let requires_action = artifacts.iter().any(|artifact| !artifact.releasable);
        Ok(StalenessReport {
            document: self.document,
            revision: current.revision,
            artifacts,
            requires_action,
        })
    }

    /// Serialise after validating, so a registry that cannot be read back is never written.
    pub fn to_json(&self) -> Result<String, ModelError> {
        self.validate()?;
        serde_json::to_string_pretty(self).map_err(|error| ModelError::Json(error.to_string()))
    }

    /// Read a registry from JSON, refusing unknown fields, newer schemas and oversized input.
    pub fn from_json(bytes: &[u8]) -> Result<Self, ModelError> {
        let length = u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        if length > MAX_MODEL_BYTES {
            return Err(ModelError::TooLarge {
                bytes: length,
                limit: MAX_MODEL_BYTES,
            });
        }
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|error| ModelError::Json(error.to_string()))?;
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| ModelError::Invalid("missing `schema`".into()))?;
        let schema = u32::try_from(schema).unwrap_or(u32::MAX);
        if schema > MODEL_SCHEMA {
            return Err(ModelError::UnsupportedSchema {
                found: schema,
                supported: MODEL_SCHEMA,
            });
        }
        let registry: ArtifactRegistry =
            serde_json::from_value(value).map_err(|error| ModelError::Json(error.to_string()))?;
        registry.validate()?;
        Ok(registry)
    }

    /// Write atomically: a failed write leaves the previous registry untouched.
    pub fn save(&self, path: &Path) -> Result<(), ModelError> {
        let json = self.to_json()?;
        static TEMP: AtomicU64 = AtomicU64::new(0);
        let serial = TEMP.fetch_add(1, Ordering::Relaxed);
        let file_name = path
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("artifacts.json");
        let temporary =
            path.with_file_name(format!(".{file_name}.tmp-{}-{serial}", std::process::id()));
        if let Err(error) = std::fs::write(&temporary, json.as_bytes()) {
            let _ = std::fs::remove_file(&temporary);
            return Err(io_error(path, &error));
        }
        if let Err(error) = std::fs::rename(&temporary, path) {
            let _ = std::fs::remove_file(&temporary);
            return Err(io_error(path, &error));
        }
        Ok(())
    }

    /// Read a registry file, checking its size before parsing.
    pub fn load(path: &Path) -> Result<Self, ModelError> {
        let metadata = std::fs::metadata(path).map_err(|error| io_error(path, &error))?;
        if metadata.len() > MAX_MODEL_BYTES {
            return Err(ModelError::TooLarge {
                bytes: metadata.len(),
                limit: MAX_MODEL_BYTES,
            });
        }
        let bytes = std::fs::read(path).map_err(|error| io_error(path, &error))?;
        Self::from_json(&bytes)
    }
}

fn source_status(artifact: &DerivedArtifact, current: DocumentRef) -> ArtifactStatus {
    let source_revision = artifact.source.revision.get();
    let current_revision = current.revision.get();
    if source_revision > current_revision {
        return ArtifactStatus::FutureRevision {
            source_revision,
            current_revision,
        };
    }
    if source_revision == current_revision {
        // The revision number is the same, but the content is not: a restored or hand-edited file
        // can keep its revision. Only compare when both sides actually state a fingerprint.
        if let (Some(expected), Some(actual)) = (artifact.source.fingerprint, current.fingerprint)
            && expected != actual
        {
            return ArtifactStatus::RequiresRecompute {
                source_revision,
                current_revision,
            };
        }
        return ArtifactStatus::Current;
    }
    let reviewed_at = artifact.reviewed_revision.map(|revision| revision.get());
    match reviewed_at {
        Some(reviewed) if reviewed >= current_revision => ArtifactStatus::AcceptedAfterReview {
            source_revision,
            current_revision,
            reviewed_revision: reviewed,
        },
        _ => ArtifactStatus::RequiresReview {
            source_revision,
            current_revision,
        },
    }
}

fn io_error(path: &Path, error: &std::io::Error) -> ModelError {
    ModelError::Io {
        path: path.display().to_string(),
        message: error.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ArtifactId, ArtifactKind, ArtifactRegistry, ArtifactResult, ArtifactStatus,
        DerivedArtifact, DocumentRef, Fingerprint, MAX_MODEL_BYTES, MODEL_SCHEMA, Material,
        MaterialCategory, MaterialLibrary, ModelError,
    };
    use cad_contract::document::Revision;
    use cad_contract::ids::{DocumentId, EntityId};

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn document() -> DocumentId {
        DocumentId::new(7)
    }

    fn at(revision: u64) -> Result<DocumentRef, Box<dyn std::error::Error>> {
        let revision = Revision::from_raw(revision).ok_or("revision must be positive")?;
        Ok(DocumentRef::new(document(), revision, None))
    }

    fn artifact(
        id: u64,
        kind: ArtifactKind,
        label: &str,
        source: DocumentRef,
    ) -> Result<DerivedArtifact, Box<dyn std::error::Error>> {
        let id = ArtifactId::new(id).ok_or("artifact id must be positive")?;
        Ok(DerivedArtifact::new(id, kind, label, source))
    }

    #[test]
    fn standard_material_library_is_valid_and_unique() -> TestResult {
        let library = MaterialLibrary::standard();
        library.validate()?;
        assert_eq!(library.len(), 7);
        let steel = library.get("STEEL.1018").ok_or("steel must be present")?;
        assert_eq!(steel.category, MaterialCategory::Metal);
        assert!((steel.density_kg_m3 - 7870.0).abs() < f64::EPSILON);
        assert!(library.get("unobtainium").is_none());
        // Duplicate ids are refused regardless of case.
        let mut duplicated = library.materials().to_vec();
        let mut copy = steel.clone();
        copy.id = "Steel.1018".into();
        duplicated.push(copy);
        assert!(MaterialLibrary::new(duplicated).is_err());
        Ok(())
    }

    #[test]
    fn a_material_with_an_impossible_property_is_refused() -> TestResult {
        let library = MaterialLibrary::standard();
        let mut material = library
            .get("aluminum.6061-t6")
            .cloned()
            .ok_or("aluminium must be present")?;
        material.poisson_ratio = Some(0.9);
        assert!(material.validate().is_err());
        material.poisson_ratio = Some(0.33);
        material.density_kg_m3 = 0.0;
        assert!(material.validate().is_err());
        material.density_kg_m3 = f64::NAN;
        assert!(material.validate().is_err());
        material.density_kg_m3 = 2700.0;
        material.youngs_modulus_mpa = Some(-1.0);
        assert!(material.validate().is_err());
        material.youngs_modulus_mpa = Some(68_900.0);
        material.source.clear();
        assert!(material.validate().is_err());
        // A new record with a zero id would alias a defaulted field.
        let zero = Material {
            id: String::new(),
            ..material
        };
        assert!(zero.validate().is_err());
        Ok(())
    }

    #[test]
    fn duplicate_artifact_ids_and_unknown_upstream_are_refused() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        let source = at(1)?;
        registry.add(artifact(1, ArtifactKind::Drawing, "drawing", source)?)?;
        assert!(matches!(
            registry.add(artifact(1, ArtifactKind::Simulation, "study", source)?),
            Err(ModelError::DuplicateArtifact(1))
        ));
        let mut dependent = artifact(2, ArtifactKind::Toolpath, "toolpath", source)?;
        dependent.upstream.push(ArtifactId::new(99).ok_or("id")?);
        assert!(matches!(
            registry.add(dependent),
            Err(ModelError::UnknownUpstream(99))
        ));
        assert_eq!(registry.artifacts().len(), 1);
        Ok(())
    }

    #[test]
    fn a_dependency_cycle_is_refused_on_load() -> TestResult {
        // A cycle cannot be created through `add` because upstream artifacts must already exist, so
        // this is exactly the hand-edited-file case the validator has to catch.
        let json = format!(
            r#"{{
                "schema": {MODEL_SCHEMA},
                "document": 7,
                "artifacts": [
                    {{"id": 1, "kind": "drawing", "label": "a",
                      "source": {{"document": 7, "revision": 1}}, "upstream": [2]}},
                    {{"id": 2, "kind": "simulation", "label": "b",
                      "source": {{"document": 7, "revision": 1}}, "upstream": [1]}}
                ]
            }}"#
        );
        assert!(matches!(
            ArtifactRegistry::from_json(json.as_bytes()),
            Err(ModelError::Cycle(_))
        ));
        let self_cycle = format!(
            r#"{{
                "schema": {MODEL_SCHEMA},
                "document": 7,
                "artifacts": [
                    {{"id": 3, "kind": "drawing", "label": "a",
                      "source": {{"document": 7, "revision": 1}}, "upstream": [3]}}
                ]
            }}"#
        );
        assert!(matches!(
            ArtifactRegistry::from_json(self_cycle.as_bytes()),
            Err(ModelError::Cycle(3))
        ));
        Ok(())
    }

    #[test]
    fn unchanged_revision_is_current_and_a_change_requires_review() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(1, ArtifactKind::Drawing, "shop drawing", at(3)?)?)?;
        let report = registry.assess(at(3)?)?;
        assert_eq!(report.artifacts[0].status, ArtifactStatus::Current);
        assert!(!report.requires_action);

        let report = registry.assess(at(4)?)?;
        assert_eq!(
            report.artifacts[0].status,
            ArtifactStatus::RequiresReview {
                source_revision: 3,
                current_revision: 4
            }
        );
        assert!(report.requires_action);
        assert!(!report.artifacts[0].releasable);
        Ok(())
    }

    #[test]
    fn review_acknowledges_but_does_not_rewrite_the_source_revision() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(
            1,
            ArtifactKind::Simulation,
            "linear static",
            at(2)?,
        )?)?;
        registry.review(
            ArtifactId::new(1).ok_or("id")?,
            Revision::from_raw(4).ok_or("rev")?,
        )?;
        let report = registry.assess(at(4)?)?;
        assert_eq!(
            report.artifacts[0].status,
            ArtifactStatus::AcceptedAfterReview {
                source_revision: 2,
                current_revision: 4,
                reviewed_revision: 4
            }
        );
        assert!(report.artifacts[0].releasable);
        assert!(!report.requires_action);
        assert!(!report.artifacts[0].status.is_current());
        // The record still says the result came from revision 2.
        let stored = registry
            .get(ArtifactId::new(1).ok_or("id")?)
            .ok_or("artifact")?;
        assert_eq!(stored.source.revision.get(), 2);
        // Reviewing again at an older revision does not lower the acknowledgment.
        registry.review(
            ArtifactId::new(1).ok_or("id")?,
            Revision::from_raw(3).ok_or("rev")?,
        )?;
        let stored = registry
            .get(ArtifactId::new(1).ok_or("id")?)
            .ok_or("artifact")?;
        assert_eq!(
            stored.reviewed_revision.map(Revision::get),
            Some(4),
            "the latest review wins"
        );
        Ok(())
    }

    #[test]
    fn staleness_propagates_transitively_through_upstream_artifacts() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(10, ArtifactKind::Drawing, "drawing", at(1)?)?)?;
        let mut setup = artifact(11, ArtifactKind::ManufacturingSetup, "setup", at(1)?)?;
        setup.upstream.push(ArtifactId::new(10).ok_or("id")?);
        registry.add(setup)?;
        let mut toolpath = artifact(12, ArtifactKind::Toolpath, "roughing", at(1)?)?;
        toolpath.upstream.push(ArtifactId::new(11).ok_or("id")?);
        registry.add(toolpath)?;

        let current = registry.assess(at(2)?)?;
        assert_eq!(current.artifacts.len(), 3);
        assert_eq!(
            current.artifacts[0].status,
            ArtifactStatus::RequiresReview {
                source_revision: 1,
                current_revision: 2
            }
        );
        assert_eq!(
            current.artifacts[1].status,
            ArtifactStatus::RequiresReview {
                source_revision: 1,
                current_revision: 2
            }
        );
        assert_eq!(
            current.artifacts[2].status,
            ArtifactStatus::RequiresReview {
                source_revision: 1,
                current_revision: 2
            }
        );

        // Reviewing the oldest artifact alone does not unblock a dependent whose own source is stale.
        registry.review(
            ArtifactId::new(10).ok_or("id")?,
            Revision::from_raw(2).ok_or("rev")?,
        )?;
        let report = registry.assess(at(2)?)?;
        assert_eq!(report.artifacts[0].status.label(), "accepted_after_review");
        assert_eq!(report.artifacts[1].status.label(), "requires_review");
        assert!(report.requires_action);

        // A current artifact with a stale upstream is blocked by it rather than reported current.
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(20, ArtifactKind::Simulation, "modal", at(1)?)?)?;
        let mut drawing = artifact(21, ArtifactKind::Drawing, "drawing", at(2)?)?;
        drawing.upstream.push(ArtifactId::new(20).ok_or("id")?);
        registry.add(drawing)?;
        let report = registry.assess(at(2)?)?;
        assert_eq!(
            report.artifacts[1].status,
            ArtifactStatus::BlockedByUpstream {
                upstream: 20,
                source_revision: 2,
                current_revision: 2
            }
        );
        assert!(!report.artifacts[1].releasable);
        assert!(report.requires_action);
        Ok(())
    }

    #[test]
    fn a_content_fingerprint_mismatch_requires_recompute_at_the_same_revision() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        let mut source = at(5)?;
        source.fingerprint = Some(Fingerprint::of_bytes(b"first content"));
        registry.add(artifact(1, ArtifactKind::Simulation, "study", source)?)?;
        let mut current = at(5)?;
        current.fingerprint = Some(Fingerprint::of_bytes(b"first content"));
        assert_eq!(
            registry.assess(current)?.artifacts[0].status,
            ArtifactStatus::Current
        );

        current.fingerprint = Some(Fingerprint::of_bytes(b"different content"));
        let report = registry.assess(current)?;
        assert_eq!(
            report.artifacts[0].status,
            ArtifactStatus::RequiresRecompute {
                source_revision: 5,
                current_revision: 5
            }
        );
        assert!(report.requires_action);
        // Without a fingerprint on either side the revision number is the only evidence available.
        let report = registry.assess(at(5)?)?;
        assert_eq!(report.artifacts[0].status, ArtifactStatus::Current);
        Ok(())
    }

    #[test]
    fn future_revisions_and_wrong_documents_are_refused() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(1, ArtifactKind::Drawing, "drawing", at(9)?)?)?;
        let report = registry.assess(at(3)?)?;
        assert_eq!(
            report.artifacts[0].status,
            ArtifactStatus::FutureRevision {
                source_revision: 9,
                current_revision: 3
            }
        );
        assert!(report.requires_action);

        let other = DocumentRef::new(
            DocumentId::new(8),
            Revision::from_raw(3).ok_or("rev")?,
            None,
        );
        assert!(matches!(
            registry.assess(other),
            Err(ModelError::WrongDocument { .. })
        ));
        // An artifact cannot be added against a different document in the first place.
        let mut registry = ArtifactRegistry::new(document());
        let wrong = DocumentRef::new(
            DocumentId::new(8),
            Revision::from_raw(1).ok_or("rev")?,
            None,
        );
        assert!(matches!(
            registry.add(artifact(1, ArtifactKind::Drawing, "drawing", wrong)?),
            Err(ModelError::WrongDocument { .. })
        ));
        Ok(())
    }

    #[test]
    fn review_cannot_predate_the_source_revision_or_name_an_unknown_artifact() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(1, ArtifactKind::Drawing, "drawing", at(4)?)?)?;
        assert!(
            registry
                .review(
                    ArtifactId::new(1).ok_or("id")?,
                    Revision::from_raw(3).ok_or("rev")?
                )
                .is_err()
        );
        assert!(matches!(
            registry.review(
                ArtifactId::new(2).ok_or("id")?,
                Revision::from_raw(4).ok_or("rev")?
            ),
            Err(ModelError::UnknownArtifact(2))
        ));
        Ok(())
    }

    #[test]
    fn an_artifact_cannot_be_removed_while_a_dependent_uses_it() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(1, ArtifactKind::Drawing, "drawing", at(1)?)?)?;
        let mut toolpath = artifact(2, ArtifactKind::Toolpath, "path", at(1)?)?;
        toolpath.upstream.push(ArtifactId::new(1).ok_or("id")?);
        registry.add(toolpath)?;
        assert!(registry.remove(ArtifactId::new(1).ok_or("id")?).is_err());
        registry.remove(ArtifactId::new(2).ok_or("id")?)?;
        registry.remove(ArtifactId::new(1).ok_or("id")?)?;
        assert!(registry.artifacts().is_empty());
        Ok(())
    }

    #[test]
    fn registry_round_trips_and_refuses_unknown_fields_newer_schemas_and_oversize() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        let mut item = artifact(1, ArtifactKind::Simulation, "linear static", at(2)?)?;
        item.source_entities.push(EntityId::new(5)?);
        item.result = ArtifactResult {
            summary: "max von Mises 120 MPa".into(),
            warnings: vec!["linear material model".into()],
            produced_at_unix: Some(1_700_000_000),
        };
        registry.add(item)?;
        let json = registry.to_json()?;
        let back = ArtifactRegistry::from_json(json.as_bytes())?;
        assert_eq!(back, registry);
        assert!(json.contains("\"schema\": 1"));

        let unknown = json.replace("\"schema\": 1", "\"schema\": 1, \"extra\": true");
        assert!(ArtifactRegistry::from_json(unknown.as_bytes()).is_err());

        let newer = json.replace("\"schema\": 1", "\"schema\": 2");
        assert!(matches!(
            ArtifactRegistry::from_json(newer.as_bytes()),
            Err(ModelError::UnsupportedSchema { found: 2, .. })
        ));

        // The size limit is checked before parsing, so a huge file is not deserialised.
        let mut huge = vec![b' '; usize::try_from(MAX_MODEL_BYTES)? + 1];
        huge[0] = b'{';
        assert!(matches!(
            ArtifactRegistry::from_json(&huge),
            Err(ModelError::TooLarge { .. })
        ));

        // A zero revision is not a document revision.
        let zero = json.replace("\"revision\": 2", "\"revision\": 0");
        assert!(ArtifactRegistry::from_json(zero.as_bytes()).is_err());
        Ok(())
    }

    #[test]
    fn save_and_load_are_atomic_and_preserve_the_registry() -> TestResult {
        let mut registry = ArtifactRegistry::new(document());
        registry.add(artifact(1, ArtifactKind::Drawing, "drawing", at(1)?)?)?;
        let dir = std::env::temp_dir().join(format!(
            "atlas-engineering-model-{}-{}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .map(|duration| duration.as_nanos())
                .unwrap_or(0)
        ));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("artifacts.json");
        registry.save(&path)?;
        let loaded = ArtifactRegistry::load(&path)?;
        assert_eq!(loaded, registry);
        // The temporary file is renamed, not left behind.
        let leftovers: Vec<_> = std::fs::read_dir(&dir)?
            .filter_map(Result::ok)
            .filter(|entry| entry.file_name().to_string_lossy().contains("tmp"))
            .collect();
        assert!(leftovers.is_empty());
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn invalid_artifact_fields_are_rejected() -> TestResult {
        let source = at(1)?;
        let mut empty = artifact(1, ArtifactKind::Drawing, "   ", source)?;
        assert!(empty.validate().is_err());
        empty.label = "drawing".into();
        empty.upstream.push(ArtifactId::new(1).ok_or("id")?);
        assert!(matches!(empty.validate(), Err(ModelError::Cycle(1))));
        empty.upstream.clear();
        empty.result.warnings = vec!["x".repeat(501)];
        assert!(empty.validate().is_err());
        empty.result.warnings.clear();
        Ok(())
    }
}

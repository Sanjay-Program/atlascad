//! A local parametric CAD project that can be saved, reopened, edited, and rebuilt.
//!
//! This is the smallest honest product workflow the platform can complete end to end on this
//! hardware, without a GUI:
//!
//! 1. Create a project with named parameters.
//! 2. Create a sketch: rectangle, circle, or regular polygon, each driven by parameters.
//! 3. Create an extrude feature driven by `depth`.
//! 4. Change a parameter or the sketch profile and recompute.
//! 5. Undo and redo.
//! 6. Save and reopen the project file, including migration from schema 1.
//! 7. Evaluate through a [`cad_contract::Kernel`] and inspect volume.
//!
//! # Approximations that are stated, not hidden
//!
//! The kernel contract takes a polygon profile, so a circle is tessellated into a deterministic
//! polygon whose segment count follows the document's linear tolerance. A circle's volume is
//! therefore the polygon's volume, slightly below `πr²h`; the error shrinks with the tolerance and
//! is reported by [`Project::sketch_segment_count`]. No operation here claims an exact analytic
//! cylinder.
//!
//! What this is **not**: a general sketch constraint solver, assemblies, drawings, fillets, or
//! boolean difference. Those are not advertised here.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cad_contract::document::{DocumentError, DocumentHeader, Revision};
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::kernel::{Kernel, SolidId, validate_profile};
use cad_contract::mesh::Mesh;
use cad_contract::tol::Tolerance;
use cad_contract::units::Unit;
use serde::{Deserialize, Serialize};
use std::fs::OpenOptions;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

/// Payload schema of a `.zcad` project file. Independent of [`DocumentHeader::schema`].
///
/// * Schema 1 stored a rectangle sketch as flat `width_param`/`height_param` fields.
/// * Schema 2 stores an explicit [`SketchProfile`]; schema-1 files are migrated on load.
pub const PROJECT_SCHEMA: u32 = 2;

/// Smallest regular polygon the sketch model accepts.
pub const MIN_POLYGON_SIDES: u32 = 3;
/// Largest regular polygon the sketch model accepts. Above this a polygon is a worse circle.
pub const MAX_POLYGON_SIDES: u32 = 256;
const MIN_CIRCLE_SEGMENTS: usize = 24;
const MAX_CIRCLE_SEGMENTS: usize = 256;
/// Maximum accepted serialized project size, checked before parsing untrusted input.
pub const MAX_PROJECT_BYTES: u64 = 64 * 1024 * 1024;

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
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("project schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("I/O at {path}: {message}")]
    Io { path: String, message: String },
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
}

impl SketchProfile {
    /// Human-readable profile name used in reports and the UI.
    pub const fn kind(&self) -> &'static str {
        match self {
            SketchProfile::Rectangle { .. } => "rectangle",
            SketchProfile::Circle { .. } => "circle",
            SketchProfile::RegularPolygon { .. } => "regular_polygon",
        }
    }
}

/// A closed profile in the XY plane, positioned by `origin`, in document units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sketch {
    pub id: EntityId,
    pub origin: [f64; 2],
    pub profile: SketchProfile,
}

/// An extrude of a sketch along +Z by a depth parameter, in document units.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Feature {
    pub id: EntityId,
    pub sketch_id: EntityId,
    pub depth_param: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    parameters: Vec<Parameter>,
    sketches: Vec<Sketch>,
    features: Vec<Feature>,
    next_entity: u64,
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
}

/// Geometry produced by evaluating the feature tree against a kernel.
#[derive(Debug, Clone)]
pub struct Evaluated {
    pub mesh: Mesh,
    pub volume_mm3: f64,
    pub triangles: usize,
    pub solid: SolidId,
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

    /// Add a sketch with an arbitrary validated profile.
    pub fn add_sketch(
        &mut self,
        profile: SketchProfile,
        origin: [f64; 2],
    ) -> Result<EntityId, ProjectError> {
        self.validate_profile_references(&profile)?;
        if !origin[0].is_finite() || !origin[1].is_finite() {
            return Err(ProjectError::Invalid("sketch origin must be finite".into()));
        }
        self.push_undo();
        let id = self.allocate_entity()?;
        self.sketches.push(Sketch {
            id,
            origin,
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
        if !origin[0].is_finite() || !origin[1].is_finite() {
            return Err(ProjectError::Invalid("sketch origin must be finite".into()));
        }
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
        }
        Ok(())
    }

    /// Add an extrude of an existing sketch.
    pub fn add_extrude(
        &mut self,
        sketch_id: EntityId,
        depth_param: &str,
    ) -> Result<EntityId, ProjectError> {
        if !self.sketches.iter().any(|sketch| sketch.id == sketch_id) {
            return Err(ProjectError::UnknownSketch(sketch_id.get()));
        }
        let _ = self.parameter(depth_param)?;
        self.push_undo();
        let id = self.allocate_entity()?;
        self.features.push(Feature {
            id,
            sketch_id,
            depth_param: depth_param.to_string(),
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
    /// A rectangle is four, a regular polygon is its side count, and a circle follows the document's
    /// linear tolerance: `n = ceil(2π / (2·acos(1 − tol / r)))`, bounded to 24..=256.
    pub fn sketch_segment_count(&self, sketch: &Sketch) -> Result<usize, ProjectError> {
        match &sketch.profile {
            SketchProfile::Rectangle { .. } => Ok(4),
            SketchProfile::RegularPolygon { sides, .. } => Ok(*sides as usize),
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
        };
        validate_profile(&profile).map_err(|error| ProjectError::Invalid(error.to_string()))?;
        Ok(profile)
    }

    /// Rebuild every feature through `kernel`. Only a single extrude is supported today.
    pub fn evaluate<K: Kernel>(&self, kernel: &mut K) -> Result<Evaluated, ProjectError> {
        self.validate()?;
        let feature = self
            .features
            .last()
            .ok_or_else(|| ProjectError::Invalid("project has no features to evaluate".into()))?;
        let sketch = self
            .sketches
            .iter()
            .find(|candidate| candidate.id == feature.sketch_id)
            .ok_or(ProjectError::UnknownSketch(feature.sketch_id.get()))?;
        let profile = self.sketch_profile_mm(sketch)?;
        let depth_mm = self
            .header
            .units
            .to_mm(self.parameter(&feature.depth_param)?);
        let solid = kernel.extrude(&profile, depth_mm)?;
        let mesh = kernel.tessellate(solid, self.header.tolerance.linear_mm())?;
        mesh.validate()
            .map_err(|error| ProjectError::Invalid(error.to_string()))?;
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
        let value = if schema == 1 {
            migrate_v1_to_v2(value)?
        } else if schema == PROJECT_SCHEMA {
            value
        } else {
            return Err(ProjectError::UnsupportedSchema {
                found: schema,
                supported: PROJECT_SCHEMA,
            });
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
        Ok(())
    }

    pub fn save(&self, path: &Path) -> Result<(), ProjectError> {
        self.validate()?;
        let json = self.to_json()?;
        atomic_replace(path, json.as_bytes())
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
    }
    for feature in features {
        if !entity_ids.insert(feature.id) {
            return Err(ProjectError::Invalid(format!(
                "entity id {} is duplicated",
                feature.id
            )));
        }
        max_id = max_id.max(feature.id.get());
        if !sketches.iter().any(|sketch| sketch.id == feature.sketch_id) {
            return Err(ProjectError::UnknownSketch(feature.sketch_id.get()));
        }
        if !parameter_ids.contains(feature.depth_param.as_str()) {
            return Err(ProjectError::UnknownParameter(feature.depth_param.clone()));
        }
    }
    if next_entity <= max_id {
        return Err(ProjectError::Invalid(
            "next entity id must be greater than every stored entity id".into(),
        ));
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
    object.insert("schema".into(), serde_json::json!(PROJECT_SCHEMA));
    Ok(value)
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

        let json = project.to_json()?;
        assert!(json.contains("\"schema\": 2"), "{json}");
        assert!(json.contains("\"kind\": \"rectangle\""), "{json}");
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
}

//! A 2D drafting document: the layer/entity model a drawing is made of, with versioned
//! persistence and DXF R12 output.
//!
//! This is the first slice of the drafting workspace. It is deliberately a *document*, not a
//! drawing surface: geometry, layers, undo and a tested exchange format come before any UI, because
//! a drawing that cannot be saved and opened elsewhere is not a drawing.
//!
//! # What is here
//!
//! 1. Layers with an ACI colour, visibility and a lock. A locked layer refuses edits; a hidden layer
//!    still holds its geometry and exports as an *off* DXF layer (negative colour), which is how a
//!    DXF reader is told the layer is off rather than losing it.
//! 2. Entities: point, line, circle, arc and polyline (closed or open). Every entity is validated
//!    before it enters the document: finite coordinates, positive radius, at least two distinct
//!    polyline points.
//! 3. Bounded undo/redo over layers and geometry together.
//! 4. Versioned JSON persistence that refuses unknown fields and newer schemas, with a size cap
//!    checked before parsing.
//! 5. **DXF R12 (AC1009) output**: header, LTYPE and LAYER tables, entities, EOF. A polyline is
//!    written as its segments because R12 predates `LWPOLYLINE`; that is a stated reduction, not a
//!    loss of geometry.
//!
//! # What is not here, and is not claimed
//!
//! Dimensions, text, hatch, blocks, Xrefs, layouts/sheets, PDF plotting, line types beyond
//! `CONTINUOUS`, DXF **input**, DWG, and any interactive editor. Those are the next slices; none is
//! advertised by this crate.
//!
//! # Units
//!
//! Coordinates are stored in the document's own unit (the header's [`Unit`]), and the DXF writer
//! records that unit in `$INSUNITS` (4 = mm, 5 = cm, 6 = m, 1 = in, 2 = ft). Nothing here converts
//! silently: the numbers written are the numbers stored.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cad_contract::document::DocumentHeader;
use cad_contract::ids::{DocumentId, EntityId};
use cad_contract::units::Unit;
use serde::{Deserialize, Serialize};
use std::path::Path;

mod dxf;
mod pdf;
mod sheet;

pub use dxf::{DxfImport, ImportReport, MAX_DXF_BYTES, from_dxf};
pub use pdf::{PDF_VERSION, helvetica_width_pt, mm_to_pt, to_pdf};
pub use sheet::{
    MAX_SHEET_TEXT, Orientation, PaperSize, Plot, PlotError, PlotLine, PlotText, STANDARD_SCALES,
    Sheet, TextAlign, plotted_entity_count, scale_label,
};

/// Payload schema of a `.zdraft` drawing file.
///
/// * Schema 1 is the first schema: layers, entities, undo history and the next entity id.
pub const DRAFT_SCHEMA: u32 = 1;

/// Largest accepted serialized drawing, checked before parsing untrusted input.
pub const MAX_DRAFT_BYTES: u64 = 32 * 1024 * 1024;

/// Most snapshots kept for undo. A drawing edits cheaply, so the bound is purely about memory.
pub const MAX_UNDO: usize = 64;

/// Most points a single polyline may hold, so a hostile file cannot ask for an unbounded loop.
pub const MAX_POLYLINE_POINTS: usize = 4096;

/// Longest text accepted in one text or dimension entity, in characters.
pub const MAX_TEXT_CHARS: usize = 512;

/// Longest layer name accepted, in characters.
pub const MAX_LAYER_NAME_CHARS: usize = 255;

/// Layer name every new drawing starts with. `0` is the conventional default in DXF.
pub const DEFAULT_LAYER: &str = "0";

/// Errors a drafting operation can produce.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DraftError {
    #[error("document: {0}")]
    Document(#[from] cad_contract::document::DocumentError),
    #[error("{0}")]
    Invalid(String),
    #[error("unknown layer `{0}`")]
    UnknownLayer(String),
    #[error("unknown entity {0}")]
    UnknownEntity(u64),
    #[error("nothing to undo")]
    NothingToUndo,
    #[error("nothing to redo")]
    NothingToRedo,
    #[error("drawing schema {found} is newer than supported schema {supported}")]
    UnsupportedSchema { found: u32, supported: u32 },
    #[error("I/O at {path}: {message}")]
    Io { path: String, message: String },
}

/// One drawing layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Layer {
    pub name: String,
    /// AutoCAD Color Index, 1–255.
    pub color: u8,
    /// A hidden layer keeps its geometry and is written to DXF as an off layer.
    pub visible: bool,
    /// A locked layer refuses new entities and removals; it still exports.
    pub locked: bool,
    /// Plotted line weight in millimetres. R12 has no line-weight table, so it is document data
    /// only, kept for a later DXF version rather than silently dropped on export.
    pub line_weight_mm: f64,
}

impl Layer {
    fn new(name: &str, color: u8) -> Self {
        Layer {
            name: name.to_string(),
            color,
            visible: true,
            locked: false,
            line_weight_mm: 0.25,
        }
    }
}

/// A drawing entity's geometry, in document units, in the XY plane.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum EntityKind {
    Point {
        at: [f64; 2],
    },
    Line {
        start: [f64; 2],
        end: [f64; 2],
    },
    Circle {
        center: [f64; 2],
        radius: f64,
    },
    /// An arc of `radius` about `center`, from `start_degrees` counter-clockwise to `end_degrees`.
    Arc {
        center: [f64; 2],
        radius: f64,
        start_degrees: f64,
        end_degrees: f64,
    },
    /// A polyline through `points`. A closed loop repeats no point: the last point joins the first.
    Polyline {
        points: Vec<[f64; 2]>,
        closed: bool,
    },
    /// One line of text with its baseline start at `at`; `height` is the cap height in document
    /// units. A drawing without text is not a drawing, and a title block is text.
    Text {
        at: [f64; 2],
        height: f64,
        value: String,
    },
    /// A linear dimension between two points, measured in document units.
    ///
    /// `offset` moves the dimension line along the left normal of `start -> end`, so a positive
    /// offset puts it on the side you get by turning left from `start` to `end`. `text` overrides the
    /// measured value; `None` means the measurement is drawn.
    Dimension {
        start: [f64; 2],
        end: [f64; 2],
        offset: f64,
        text: Option<String>,
    },
}

impl EntityKind {
    /// Stable label used by reports and the DXF writer.
    pub const fn label(&self) -> &'static str {
        match self {
            EntityKind::Point { .. } => "point",
            EntityKind::Line { .. } => "line",
            EntityKind::Circle { .. } => "circle",
            EntityKind::Arc { .. } => "arc",
            EntityKind::Polyline { .. } => "polyline",
            EntityKind::Text { .. } => "text",
            EntityKind::Dimension { .. } => "dimension",
        }
    }

    fn validate(&self) -> Result<(), DraftError> {
        match self {
            EntityKind::Point { at } => validate_point(at),
            EntityKind::Line { start, end } => {
                validate_point(start)?;
                validate_point(end)?;
                if start == end {
                    return Err(DraftError::Invalid(
                        "a line needs two distinct endpoints".into(),
                    ));
                }
                Ok(())
            }
            EntityKind::Circle { center, radius } => {
                validate_point(center)?;
                validate_radius(*radius)?;
                Ok(())
            }
            EntityKind::Arc {
                center,
                radius,
                start_degrees,
                end_degrees,
            } => {
                validate_point(center)?;
                validate_radius(*radius)?;
                if !start_degrees.is_finite() || !end_degrees.is_finite() {
                    return Err(DraftError::Invalid(
                        "arc angles must be finite numbers of degrees".into(),
                    ));
                }
                if (end_degrees - start_degrees).rem_euclid(360.0) == 0.0 {
                    return Err(DraftError::Invalid(
                        "an arc needs a non-zero sweep; use a circle for a full turn".into(),
                    ));
                }
                Ok(())
            }
            EntityKind::Polyline { points, closed: _ } => {
                if points.len() < 2 {
                    return Err(DraftError::Invalid(format!(
                        "a polyline needs at least 2 points, got {}",
                        points.len()
                    )));
                }
                if points.len() > MAX_POLYLINE_POINTS {
                    return Err(DraftError::Invalid(format!(
                        "a polyline accepts at most {MAX_POLYLINE_POINTS} points, got {}",
                        points.len()
                    )));
                }
                for point in points {
                    validate_point(point)?;
                }
                for pair in points.windows(2) {
                    if pair[0] == pair[1] {
                        return Err(DraftError::Invalid(
                            "a polyline must not repeat a consecutive point".into(),
                        ));
                    }
                }
                Ok(())
            }
            EntityKind::Text { at, height, value } => {
                validate_point(at)?;
                if !height.is_finite() || *height <= 0.0 {
                    return Err(DraftError::Invalid(
                        "a text height must be a finite positive number".into(),
                    ));
                }
                if value.trim().is_empty() {
                    return Err(DraftError::Invalid(
                        "a text entity needs at least one visible character".into(),
                    ));
                }
                if value.chars().count() > MAX_TEXT_CHARS {
                    return Err(DraftError::Invalid(format!(
                        "a text accepts at most {MAX_TEXT_CHARS} characters"
                    )));
                }
                if value.contains(['\n', '\r']) {
                    return Err(DraftError::Invalid(
                        "a text entity is one line; use another entity for the next".into(),
                    ));
                }
                Ok(())
            }
            EntityKind::Dimension {
                start,
                end,
                offset,
                text,
            } => {
                validate_point(start)?;
                validate_point(end)?;
                if start == end {
                    return Err(DraftError::Invalid(
                        "a dimension needs two distinct points to measure between".into(),
                    ));
                }
                if !offset.is_finite() {
                    return Err(DraftError::Invalid(
                        "a dimension offset must be finite".into(),
                    ));
                }
                if let Some(text) = text
                    && text.chars().count() > MAX_TEXT_CHARS
                {
                    return Err(DraftError::Invalid(format!(
                        "a dimension label accepts at most {MAX_TEXT_CHARS} characters"
                    )));
                }
                Ok(())
            }
        }
    }
}

fn validate_point(point: &[f64; 2]) -> Result<(), DraftError> {
    if !point[0].is_finite() || !point[1].is_finite() {
        return Err(DraftError::Invalid(
            "coordinates must be finite numbers".into(),
        ));
    }
    Ok(())
}

fn validate_radius(radius: f64) -> Result<(), DraftError> {
    if !radius.is_finite() || radius <= 0.0 {
        return Err(DraftError::Invalid(
            "a radius must be a finite positive number".into(),
        ));
    }
    Ok(())
}

/// One entity on a layer.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Entity {
    pub id: EntityId,
    pub layer: String,
    pub kind: EntityKind,
}

/// The editable state an undo snapshot captures: layers, geometry and the id counter.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Snapshot {
    layers: Vec<Layer>,
    entities: Vec<Entity>,
    next_entity: u64,
}

/// A 2D drawing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Draft {
    pub schema: u32,
    pub header: DocumentHeader,
    pub name: String,
    layers: Vec<Layer>,
    entities: Vec<Entity>,
    next_entity: u64,
    #[serde(default)]
    undo: Vec<Snapshot>,
    #[serde(default)]
    redo: Vec<Snapshot>,
}

/// Two-dimensional bounds: `[min_x, min_y, max_x, max_y]`.
pub type Bounds = [f64; 4];

impl Draft {
    /// An empty drawing in `unit` with the conventional `0` layer.
    pub fn new(name: impl Into<String>, document: DocumentId, unit: Unit) -> Self {
        Draft {
            schema: DRAFT_SCHEMA,
            header: DocumentHeader::new(document, unit),
            name: name.into(),
            layers: vec![Layer::new(DEFAULT_LAYER, 7)],
            entities: Vec::new(),
            next_entity: 1,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    /// The layers, in creation order.
    pub fn layers(&self) -> &[Layer] {
        &self.layers
    }

    /// The entities, in creation order.
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    /// Look a layer up by name.
    pub fn layer(&self, name: &str) -> Option<&Layer> {
        self.layers.iter().find(|layer| layer.name == name)
    }

    /// Add a layer with an ACI colour. Names are trimmed, non-empty, bounded and unique
    /// case-insensitively, so `outline` and `OUTLINE` cannot both exist.
    pub fn add_layer(&mut self, name: &str, color: u8) -> Result<(), DraftError> {
        let name = validate_layer_name(name)?;
        if !(1..=255).contains(&color) {
            return Err(DraftError::Invalid(
                "a layer colour must be an ACI index from 1 to 255".into(),
            ));
        }
        if self
            .layers
            .iter()
            .any(|layer| layer.name.eq_ignore_ascii_case(name))
        {
            return Err(DraftError::Invalid(format!(
                "layer `{name}` already exists"
            )));
        }
        self.push_undo();
        self.layers.push(Layer::new(name, color));
        Ok(())
    }

    /// Change a layer's colour.
    pub fn set_layer_color(&mut self, name: &str, color: u8) -> Result<(), DraftError> {
        if !(1..=255).contains(&color) {
            return Err(DraftError::Invalid(
                "a layer colour must be an ACI index from 1 to 255".into(),
            ));
        }
        self.push_undo();
        let layer = self.layer_mut(name)?;
        layer.color = color;
        Ok(())
    }

    /// Show or hide a layer. Hiding keeps the geometry; the DXF writer records the layer as off.
    pub fn set_layer_visible(&mut self, name: &str, visible: bool) -> Result<(), DraftError> {
        self.push_undo();
        let layer = self.layer_mut(name)?;
        layer.visible = visible;
        Ok(())
    }

    /// Lock or unlock a layer. A lock refuses geometry edits on that layer only.
    pub fn set_layer_locked(&mut self, name: &str, locked: bool) -> Result<(), DraftError> {
        self.push_undo();
        let layer = self.layer_mut(name)?;
        layer.locked = locked;
        Ok(())
    }

    /// Set a layer's plotted line weight, in millimetres.
    pub fn set_layer_line_weight(&mut self, name: &str, weight_mm: f64) -> Result<(), DraftError> {
        if !weight_mm.is_finite() || weight_mm < 0.0 {
            return Err(DraftError::Invalid(
                "a line weight must be a finite, non-negative number of millimetres".into(),
            ));
        }
        self.push_undo();
        let layer = self.layer_mut(name)?;
        layer.line_weight_mm = weight_mm;
        Ok(())
    }

    /// Add a validated entity to an existing, unlocked layer.
    pub fn add_entity(&mut self, layer: &str, kind: EntityKind) -> Result<EntityId, DraftError> {
        kind.validate()?;
        let target = self
            .layer(layer)
            .ok_or_else(|| DraftError::UnknownLayer(layer.to_string()))?;
        if target.locked {
            return Err(DraftError::Invalid(format!(
                "layer `{layer}` is locked, so it accepts no geometry"
            )));
        }
        let id = EntityId::new(self.next_entity)
            .map_err(|error| DraftError::Invalid(error.to_string()))?;
        let next = self
            .next_entity
            .checked_add(1)
            .ok_or_else(|| DraftError::Invalid("entity id space exhausted".into()))?;
        self.push_undo();
        self.next_entity = next;
        self.entities.push(Entity {
            id,
            layer: layer.to_string(),
            kind,
        });
        Ok(id)
    }

    /// Remove an entity. The layer must exist and be unlocked.
    pub fn remove_entity(&mut self, id: EntityId) -> Result<(), DraftError> {
        let position = self
            .entities
            .iter()
            .position(|entity| entity.id == id)
            .ok_or(DraftError::UnknownEntity(id.get()))?;
        let layer_name = self.entities[position].layer.clone();
        if self.layer(&layer_name).is_some_and(|layer| layer.locked) {
            return Err(DraftError::Invalid(format!(
                "layer `{layer_name}` is locked, so its geometry cannot be removed"
            )));
        }
        self.push_undo();
        self.entities.remove(position);
        Ok(())
    }

    /// Drop the undo and redo history.
    ///
    /// Used when a drawing arrives whole rather than by editing — an import, for instance. Undoing
    /// back through the records of a file that was never edited here is not a history anyone wants.
    pub fn clear_history(&mut self) {
        self.undo.clear();
        self.redo.clear();
    }

    /// Undo the last change.
    pub fn undo(&mut self) -> Result<(), DraftError> {
        let previous = self.undo.pop().ok_or(DraftError::NothingToUndo)?;
        self.redo.push(self.snapshot());
        self.restore(previous);
        Ok(())
    }

    /// Redo the last undone change.
    pub fn redo(&mut self) -> Result<(), DraftError> {
        let next = self.redo.pop().ok_or(DraftError::NothingToRedo)?;
        self.undo.push(self.snapshot());
        self.restore(next);
        Ok(())
    }

    /// Snapshots available to undo.
    pub fn undo_depth(&self) -> usize {
        self.undo.len()
    }

    /// Snapshots available to redo.
    pub fn redo_depth(&self) -> usize {
        self.redo.len()
    }

    /// Bounds of every entity, or `None` for an empty drawing. A hidden layer still counts: the
    /// geometry exists and a drawing that changes extents when a layer is switched off would be a
    /// surprise in a plot.
    pub fn bounds(&self) -> Option<Bounds> {
        let mut bounds: Option<Bounds> = None;
        for entity in &self.entities {
            extend(&mut bounds, entity_bounds(&entity.kind));
        }
        bounds
    }

    /// Total drawn length of every entity, in document units. A point contributes nothing.
    pub fn total_length(&self) -> f64 {
        self.entities
            .iter()
            .map(|entity| entity_length(&entity.kind))
            .sum()
    }

    /// Number of entities on `layer`.
    pub fn entities_on(&self, layer: &str) -> usize {
        self.entities
            .iter()
            .filter(|entity| entity.layer == layer)
            .count()
    }

    pub fn to_json(&self) -> Result<String, DraftError> {
        serde_json::to_string_pretty(self)
            .map_err(|error| DraftError::Invalid(format!("cannot serialise drawing: {error}")))
    }

    /// Parse and validate a drawing. Unknown fields and newer schemas are refused rather than
    /// partially read, so a file is never silently downgraded.
    pub fn from_json(bytes: &[u8]) -> Result<Self, DraftError> {
        if bytes.len() as u64 > MAX_DRAFT_BYTES {
            return Err(DraftError::Invalid(format!(
                "drawing exceeds the {MAX_DRAFT_BYTES}-byte limit"
            )));
        }
        let value: serde_json::Value = serde_json::from_slice(bytes)
            .map_err(|error| DraftError::Invalid(format!("not valid JSON: {error}")))?;
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .ok_or_else(|| DraftError::Invalid("missing `schema`".into()))?;
        let schema = u32::try_from(schema).unwrap_or(u32::MAX);
        if schema != DRAFT_SCHEMA {
            return Err(DraftError::UnsupportedSchema {
                found: schema,
                supported: DRAFT_SCHEMA,
            });
        }
        let draft: Draft = serde_json::from_value(value)
            .map_err(|error| DraftError::Invalid(format!("drawing JSON: {error}")))?;
        draft.validate()?;
        Ok(draft)
    }

    /// Check the document invariants a loaded or edited drawing must satisfy.
    pub fn validate(&self) -> Result<(), DraftError> {
        if self.schema != DRAFT_SCHEMA {
            return Err(DraftError::UnsupportedSchema {
                found: self.schema,
                supported: DRAFT_SCHEMA,
            });
        }
        if self.layers.is_empty() {
            return Err(DraftError::Invalid(
                "a drawing needs at least one layer".into(),
            ));
        }
        if self.name.trim().is_empty() {
            return Err(DraftError::Invalid("drawing name must not be empty".into()));
        }
        if self.undo.len() > MAX_UNDO || self.redo.len() > MAX_UNDO {
            return Err(DraftError::Invalid(format!(
                "drawing history exceeds the {MAX_UNDO}-snapshot limit"
            )));
        }
        validate_contents(&self.layers, &self.entities, self.next_entity)?;
        for snapshot in self.undo.iter().chain(self.redo.iter()) {
            validate_contents(&snapshot.layers, &snapshot.entities, snapshot.next_entity)?;
        }
        Ok(())
    }

    /// Save atomically: write a temporary file beside the target, then rename it into place, so a
    /// crash cannot leave a half-written drawing under the real name.
    pub fn save(&self, path: &Path) -> Result<(), DraftError> {
        self.validate()?;
        let json = self.to_json()?;
        let temporary = path.with_extension("zdraft.tmp");
        std::fs::write(&temporary, json.as_bytes()).map_err(|source| DraftError::Io {
            path: temporary.display().to_string(),
            message: source.to_string(),
        })?;
        std::fs::rename(&temporary, path).map_err(|source| DraftError::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        })?;
        Ok(())
    }

    /// Load a drawing, refusing anything larger than the cap before reading it.
    pub fn load(path: &Path) -> Result<Self, DraftError> {
        let metadata = std::fs::metadata(path).map_err(|source| DraftError::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        })?;
        if metadata.len() > MAX_DRAFT_BYTES {
            return Err(DraftError::Invalid(format!(
                "drawing exceeds the {MAX_DRAFT_BYTES}-byte limit"
            )));
        }
        let bytes = std::fs::read(path).map_err(|source| DraftError::Io {
            path: path.display().to_string(),
            message: source.to_string(),
        })?;
        Self::from_json(&bytes)
    }

    /// DXF R12 (AC1009) text: header, `LTYPE` and `LAYER` tables, entities, EOF.
    ///
    /// A polyline is written as its segments, because R12 has no `LWPOLYLINE`. A locked or hidden
    /// layer still exports; a hidden layer is written with a **negative** colour, which is how DXF
    /// records an off layer, so the information survives the round trip through a reader.
    pub fn to_dxf(&self) -> String {
        let mut out = String::new();
        let bounds = self.bounds();

        // HEADER
        pair(&mut out, 0, "SECTION");
        pair(&mut out, 2, "HEADER");
        pair(&mut out, 9, "$ACADVER");
        pair(&mut out, 1, "AC1009");
        pair(&mut out, 9, "$INSUNITS");
        pair(&mut out, 70, &insunits(self.header.units).to_string());
        if let Some([min_x, min_y, max_x, max_y]) = bounds {
            pair(&mut out, 9, "$EXTMIN");
            point(&mut out, 10, [min_x, min_y]);
            pair(&mut out, 9, "$EXTMAX");
            point(&mut out, 10, [max_x, max_y]);
        }
        pair(&mut out, 0, "ENDSEC");

        // TABLES: a single continuous line type, then the layers.
        pair(&mut out, 0, "SECTION");
        pair(&mut out, 2, "TABLES");

        pair(&mut out, 0, "TABLE");
        pair(&mut out, 2, "LTYPE");
        pair(&mut out, 70, "1");
        pair(&mut out, 0, "LTYPE");
        pair(&mut out, 2, "CONTINUOUS");
        pair(&mut out, 70, "0");
        pair(&mut out, 3, "Solid line");
        pair(&mut out, 72, "65");
        pair(&mut out, 73, "0");
        pair(&mut out, 40, "0.0");
        pair(&mut out, 0, "ENDTAB");

        pair(&mut out, 0, "TABLE");
        pair(&mut out, 2, "LAYER");
        pair(&mut out, 70, &self.layers.len().to_string());
        for layer in &self.layers {
            pair(&mut out, 0, "LAYER");
            pair(&mut out, 2, &layer.name);
            pair(&mut out, 70, "0");
            let color = if layer.visible {
                i64::from(layer.color)
            } else {
                -i64::from(layer.color)
            };
            pair(&mut out, 62, &color.to_string());
            pair(&mut out, 6, "CONTINUOUS");
        }
        pair(&mut out, 0, "ENDTAB");
        pair(&mut out, 0, "ENDSEC");

        // ENTITIES
        pair(&mut out, 0, "SECTION");
        pair(&mut out, 2, "ENTITIES");
        for entity in &self.entities {
            write_entity(&mut out, entity);
        }
        pair(&mut out, 0, "ENDSEC");
        pair(&mut out, 0, "EOF");
        out
    }

    fn layer_mut(&mut self, name: &str) -> Result<&mut Layer, DraftError> {
        let position = self
            .layers
            .iter()
            .position(|layer| layer.name == name)
            .ok_or_else(|| DraftError::UnknownLayer(name.to_string()))?;
        self.layers
            .get_mut(position)
            .ok_or_else(|| DraftError::UnknownLayer(name.to_string()))
    }

    fn snapshot(&self) -> Snapshot {
        Snapshot {
            layers: self.layers.clone(),
            entities: self.entities.clone(),
            next_entity: self.next_entity,
        }
    }

    fn restore(&mut self, snapshot: Snapshot) {
        self.layers = snapshot.layers;
        self.entities = snapshot.entities;
        self.next_entity = snapshot.next_entity;
    }

    fn push_undo(&mut self) {
        self.undo.push(self.snapshot());
        if self.undo.len() > MAX_UNDO {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
}

fn validate_layer_name(name: &str) -> Result<&str, DraftError> {
    let trimmed = name.trim();
    if trimmed.is_empty() {
        return Err(DraftError::Invalid("a layer name must not be empty".into()));
    }
    if trimmed.chars().count() > MAX_LAYER_NAME_CHARS {
        return Err(DraftError::Invalid(format!(
            "a layer name accepts at most {MAX_LAYER_NAME_CHARS} characters"
        )));
    }
    if trimmed.contains(['\n', '\r']) {
        return Err(DraftError::Invalid(
            "a layer name must not contain a line break".into(),
        ));
    }
    Ok(trimmed)
}

fn validate_contents(
    layers: &[Layer],
    entities: &[Entity],
    next_entity: u64,
) -> Result<(), DraftError> {
    if layers.is_empty() {
        return Err(DraftError::Invalid(
            "a drawing needs at least one layer".into(),
        ));
    }
    let mut names: Vec<String> = Vec::with_capacity(layers.len());
    for layer in layers {
        let name = validate_layer_name(&layer.name)?;
        if !(1..=255).contains(&layer.color) {
            return Err(DraftError::Invalid(format!(
                "layer `{name}` has an invalid colour"
            )));
        }
        if !layer.line_weight_mm.is_finite() || layer.line_weight_mm < 0.0 {
            return Err(DraftError::Invalid(format!(
                "layer `{name}` has an invalid line weight"
            )));
        }
        if names
            .iter()
            .any(|existing| existing.eq_ignore_ascii_case(name))
        {
            return Err(DraftError::Invalid(format!(
                "layer `{name}` is defined twice"
            )));
        }
        names.push(name.to_string());
    }

    let mut ids: Vec<u64> = Vec::with_capacity(entities.len());
    let mut max_id = 0;
    for entity in entities {
        let id = entity.id.get();
        if ids.contains(&id) {
            return Err(DraftError::Invalid(format!("entity id {id} is duplicated")));
        }
        ids.push(id);
        max_id = max_id.max(id);
        if !names
            .iter()
            .any(|name| name.eq_ignore_ascii_case(&entity.layer))
        {
            return Err(DraftError::UnknownLayer(entity.layer.clone()));
        }
        entity.kind.validate()?;
    }
    if next_entity <= max_id {
        return Err(DraftError::Invalid(
            "next entity id must be greater than every stored entity id".into(),
        ));
    }
    Ok(())
}

fn extend(bounds: &mut Option<Bounds>, other: Bounds) {
    *bounds = Some(match *bounds {
        None => other,
        Some(current) => [
            current[0].min(other[0]),
            current[1].min(other[1]),
            current[2].max(other[2]),
            current[3].max(other[3]),
        ],
    });
}

/// Bounds of one entity. An arc includes its endpoints and every cardinal point inside its sweep,
/// because a quarter arc from 10° to 80° reaches no cardinal point but one from 10° to 100° reaches
/// 90° and its top is part of the bounding box.
fn entity_bounds(kind: &EntityKind) -> Bounds {
    match kind {
        EntityKind::Point { at } => [at[0], at[1], at[0], at[1]],
        EntityKind::Line { start, end } => [
            start[0].min(end[0]),
            start[1].min(end[1]),
            start[0].max(end[0]),
            start[1].max(end[1]),
        ],
        EntityKind::Circle { center, radius } => [
            center[0] - radius,
            center[1] - radius,
            center[0] + radius,
            center[1] + radius,
        ],
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => {
            let sweep = (end_degrees - start_degrees).rem_euclid(360.0);
            let mut bounds: Option<Bounds> = None;
            for offset in [0.0, sweep] {
                let angle = (start_degrees + offset).to_radians();
                let point = [
                    center[0] + radius * angle.cos(),
                    center[1] + radius * angle.sin(),
                ];
                extend(&mut bounds, [point[0], point[1], point[0], point[1]]);
            }
            for cardinal in [0.0, 90.0, 180.0, 270.0] {
                if (cardinal - start_degrees).rem_euclid(360.0) <= sweep {
                    let angle = cardinal.to_radians();
                    let point = [
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ];
                    extend(&mut bounds, [point[0], point[1], point[0], point[1]]);
                }
            }
            bounds.unwrap_or([center[0], center[1], center[0], center[1]])
        }
        EntityKind::Text { at, height, value } => {
            // The width of a string is the font's business; a drawing needs an extent, so the box is
            // estimated at 0.6 of the cap height per character. It is an estimate and the docs say so.
            let width = 0.6 * height * value.chars().count() as f64;
            [at[0], at[1], at[0] + width, at[1] + height]
        }
        EntityKind::Dimension {
            start, end, offset, ..
        } => {
            let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
            let length = dx.hypot(dy);
            let (nx, ny) = if length > 0.0 {
                (-dy / length * offset, dx / length * offset)
            } else {
                (0.0, 0.0)
            };
            let corners = [
                *start,
                *end,
                [start[0] + nx, start[1] + ny],
                [end[0] + nx, end[1] + ny],
            ];
            let mut bounds: Option<Bounds> = None;
            for point in corners {
                extend(&mut bounds, [point[0], point[1], point[0], point[1]]);
            }
            bounds.unwrap_or([start[0], start[1], end[0], end[1]])
        }
        EntityKind::Polyline { points, .. } => {
            let mut bounds: Option<Bounds> = None;
            for point in points {
                extend(&mut bounds, [point[0], point[1], point[0], point[1]]);
            }
            bounds.unwrap_or([0.0, 0.0, 0.0, 0.0])
        }
    }
}

/// Drawn length of one entity, in document units.
fn entity_length(kind: &EntityKind) -> f64 {
    match kind {
        EntityKind::Point { .. } => 0.0,
        EntityKind::Line { start, end } => distance(start, end),
        EntityKind::Circle { radius, .. } => std::f64::consts::TAU * radius,
        EntityKind::Arc {
            radius,
            start_degrees,
            end_degrees,
            ..
        } => {
            let sweep = (end_degrees - start_degrees).rem_euclid(360.0).to_radians();
            radius * sweep
        }
        EntityKind::Polyline { points, closed } => {
            let mut length: f64 = points
                .windows(2)
                .map(|pair| distance(&pair[0], &pair[1]))
                .sum();
            if *closed && let (Some(first), Some(last)) = (points.first(), points.last()) {
                length += distance(first, last);
            }
            length
        }
        // Annotation has no drawn length: a dimension is a measurement of geometry that is already
        // counted, so adding it here would double-count the part.
        EntityKind::Text { .. } | EntityKind::Dimension { .. } => 0.0,
    }
}

fn distance(a: &[f64; 2], b: &[f64; 2]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    dx.hypot(dy)
}

fn insunits(unit: Unit) -> i64 {
    match unit {
        Unit::Inch => 1,
        Unit::Foot => 2,
        Unit::Millimeter => 4,
        Unit::Centimeter => 5,
        Unit::Meter => 6,
    }
}

/// One DXF group: a code on its own line, then the value on the next.
fn pair(out: &mut String, code: i32, value: &str) {
    out.push_str(&code.to_string());
    out.push('\n');
    out.push_str(value);
    out.push('\n');
}

fn point(out: &mut String, code: i32, at: [f64; 2]) {
    pair(out, code, &number(at[0]));
    pair(out, code + 10, &number(at[1]));
    pair(out, code + 20, "0");
}

/// A DXF coordinate. Six decimals is more than a millimetre drawing needs, and trailing zeros are
/// trimmed so an integer coordinate reads `10` rather than `10.000000`.
fn number(value: f64) -> String {
    let value = if value.abs() < 5e-7 { 0.0 } else { value };
    let mut text = format!("{value:.6}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() || text == "-0" {
        "0".to_string()
    } else {
        text
    }
}

fn write_entity(out: &mut String, entity: &Entity) {
    let layer = entity.layer.as_str();
    match &entity.kind {
        EntityKind::Point { at } => {
            pair(out, 0, "POINT");
            pair(out, 8, layer);
            point(out, 10, *at);
        }
        EntityKind::Line { start, end } => {
            pair(out, 0, "LINE");
            pair(out, 8, layer);
            point(out, 10, *start);
            point(out, 11, *end);
        }
        EntityKind::Circle { center, radius } => {
            pair(out, 0, "CIRCLE");
            pair(out, 8, layer);
            point(out, 10, *center);
            pair(out, 40, &number(*radius));
        }
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => {
            pair(out, 0, "ARC");
            pair(out, 8, layer);
            point(out, 10, *center);
            pair(out, 40, &number(*radius));
            pair(out, 50, &number(start_degrees.rem_euclid(360.0)));
            pair(out, 51, &number(end_degrees.rem_euclid(360.0)));
        }
        EntityKind::Text { at, height, value } => {
            pair(out, 0, "TEXT");
            pair(out, 8, layer);
            point(out, 10, *at);
            pair(out, 40, &number(*height));
            pair(out, 1, value);
        }
        EntityKind::Dimension {
            start,
            end,
            offset,
            text,
        } => write_dimension(out, layer, *start, *end, *offset, text.as_deref()),
        EntityKind::Polyline { points, closed } => {
            for pair_points in points.windows(2) {
                pair(out, 0, "LINE");
                pair(out, 8, layer);
                point(out, 10, pair_points[0]);
                point(out, 11, pair_points[1]);
            }
            // A closed loop's last point joins its first; R12 has no polyline entity here, so that
            // closing segment is a segment like any other and must not be lost.
            if *closed
                && points.len() > 2
                && let (Some(first), Some(last)) = (points.first(), points.last())
            {
                pair(out, 0, "LINE");
                pair(out, 8, layer);
                point(out, 10, *last);
                point(out, 11, *first);
            }
        }
    }
}

/// A linear dimension as DXF R12 accepts it without a dimension block.
///
/// R12's `DIMENSION` entity points at a block containing its own geometry, which an exploded form
/// avoids entirely: the extension lines, the dimension line, the ticks and the value are written as
/// ordinary `LINE` and `TEXT` entities. That reads correctly in every program, at the cost of being
/// editable only as loose lines. The reduction is stated rather than hidden.
fn write_dimension(
    out: &mut String,
    layer: &str,
    start: [f64; 2],
    end: [f64; 2],
    offset: f64,
    text: Option<&str>,
) {
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return;
    }
    let (nx, ny) = (-dy / length, dx / length);
    let a = [start[0] + nx * offset, start[1] + ny * offset];
    let b = [end[0] + nx * offset, end[1] + ny * offset];
    let mut line = |from: [f64; 2], to: [f64; 2]| {
        pair(out, 0, "LINE");
        pair(out, 8, layer);
        point(out, 10, from);
        point(out, 11, to);
    };
    line(start, a);
    line(end, b);
    line(a, b);

    // Ticks at 2% of the measured length, so they stay visible on a 2 mm feature and a 2 m one.
    let tick = (length * 0.02).max(0.4);
    for point in [a, b] {
        line(
            [point[0] - tick, point[1] - tick],
            [point[0] + tick, point[1] + tick],
        );
    }

    let height = (length * 0.06).clamp(0.8, 5.0);
    let value = text.map_or_else(|| number(length), ToString::to_string);
    let mid = [(a[0] + b[0]) / 2.0, (a[1] + b[1]) / 2.0 + height * 0.4];
    pair(out, 0, "TEXT");
    pair(out, 8, layer);
    point(out, 10, mid);
    pair(out, 40, &number(height));
    pair(out, 1, &value);
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn drawing() -> Draft {
        Draft::new("plate", DocumentId::new(1), Unit::Millimeter)
    }

    #[test]
    fn a_new_drawing_has_the_default_layer_and_no_geometry() {
        let draft = drawing();
        assert_eq!(draft.layers().len(), 1);
        assert_eq!(draft.layers()[0].name, DEFAULT_LAYER);
        assert!(draft.entities().is_empty());
        assert_eq!(draft.bounds(), None);
        assert_eq!(draft.total_length(), 0.0);
        assert!(draft.validate().is_ok());
    }

    #[test]
    fn an_entity_needs_an_existing_layer() -> TestResult {
        let mut draft = drawing();
        match draft.add_entity(
            "OUTLINE",
            EntityKind::Line {
                start: [0.0, 0.0],
                end: [10.0, 0.0],
            },
        ) {
            Err(DraftError::UnknownLayer(name)) => assert_eq!(name, "OUTLINE"),
            other => return Err(format!("unexpected: {other:?}").into()),
        }
        assert!(draft.entities().is_empty());
        assert_eq!(draft.undo_depth(), 0, "a refused add is not an undo step");
        Ok(())
    }

    #[test]
    fn invalid_geometry_is_refused_before_it_enters_the_document() -> TestResult {
        let mut draft = drawing();
        let cases = [
            EntityKind::Line {
                start: [0.0, 0.0],
                end: [0.0, 0.0],
            },
            EntityKind::Line {
                start: [f64::NAN, 0.0],
                end: [1.0, 0.0],
            },
            EntityKind::Circle {
                center: [0.0, 0.0],
                radius: 0.0,
            },
            EntityKind::Arc {
                center: [0.0, 0.0],
                radius: 5.0,
                start_degrees: 30.0,
                end_degrees: 30.0,
            },
            EntityKind::Polyline {
                points: vec![[0.0, 0.0]],
                closed: false,
            },
            EntityKind::Polyline {
                points: vec![[0.0, 0.0], [0.0, 0.0]],
                closed: false,
            },
        ];
        for case in cases {
            assert!(
                draft.add_entity(DEFAULT_LAYER, case.clone()).is_err(),
                "{case:?} was accepted"
            );
        }
        assert!(draft.entities().is_empty());
        Ok(())
    }

    #[test]
    fn layer_names_are_unique_case_insensitively_and_bounded() -> TestResult {
        let mut draft = drawing();
        draft.add_layer("Outline", 1)?;
        assert!(draft.add_layer("outline", 2).is_err());
        assert!(draft.add_layer("   ", 1).is_err());
        assert!(draft.add_layer("x", 0).is_err());
        let long = "a".repeat(MAX_LAYER_NAME_CHARS + 1);
        assert!(draft.add_layer(&long, 1).is_err());
        assert_eq!(draft.layers().len(), 2);
        Ok(())
    }

    #[test]
    fn a_locked_layer_refuses_geometry_and_a_hidden_layer_accepts_it() -> TestResult {
        let mut draft = drawing();
        draft.add_layer("LOCKED", 1)?;
        draft.add_layer("HIDDEN", 2)?;
        draft.set_layer_locked("LOCKED", true)?;
        draft.set_layer_visible("HIDDEN", false)?;

        let line = EntityKind::Line {
            start: [0.0, 0.0],
            end: [10.0, 0.0],
        };
        assert!(draft.add_entity("LOCKED", line.clone()).is_err());
        let id = draft.add_entity("HIDDEN", line)?;
        assert!(draft.remove_entity(id).is_ok());
        let id = draft.add_entity(DEFAULT_LAYER, EntityKind::Point { at: [1.0, 1.0] })?;
        draft.set_layer_locked(DEFAULT_LAYER, true)?;
        assert!(draft.remove_entity(id).is_err());
        assert_eq!(draft.entities().len(), 1);
        Ok(())
    }

    #[test]
    fn undo_and_redo_restore_layers_and_geometry_together() -> TestResult {
        let mut draft = drawing();
        draft.add_layer("WALLS", 3)?;
        let line = draft.add_entity(
            "WALLS",
            EntityKind::Line {
                start: [0.0, 0.0],
                end: [4.0, 0.0],
            },
        )?;
        assert_eq!(draft.entities().len(), 1);
        draft.remove_entity(line)?;
        assert!(draft.entities().is_empty());

        draft.undo()?;
        assert_eq!(draft.entities().len(), 1);
        draft.undo()?;
        assert!(draft.entities().is_empty());
        draft.undo()?;
        assert_eq!(draft.layers().len(), 1);

        draft.redo()?;
        assert_eq!(draft.layers().len(), 2);
        draft.redo()?;
        assert_eq!(draft.entities().len(), 1);
        // The third undo emptied the drawing, so the third redo brings that state back.
        draft.redo()?;
        assert!(draft.entities().is_empty());
        assert_eq!(draft.layers().len(), 2);
        assert!(matches!(draft.redo(), Err(DraftError::NothingToRedo)));
        Ok(())
    }

    #[test]
    fn bounds_and_lengths_are_analytic() -> TestResult {
        let mut draft = drawing();
        // A 10 x 5 closed rectangle has perimeter 30 and bounds 0,0..10,5.
        draft.add_entity(
            DEFAULT_LAYER,
            EntityKind::Polyline {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]],
                closed: true,
            },
        )?;
        // A 90 degree arc of radius 2 at (0,0) from 0 to 90 adds pi/2 * 2 = 3.14159 to the length
        // and reaches the top of the circle at (0,2).
        draft.add_entity(
            DEFAULT_LAYER,
            EntityKind::Arc {
                center: [0.0, 0.0],
                radius: 2.0,
                start_degrees: 0.0,
                end_degrees: 90.0,
            },
        )?;
        let bounds = draft.bounds().ok_or("bounds")?;
        assert!((bounds[0] - 0.0).abs() < 1e-12, "min x {bounds:?}");
        assert!((bounds[1] - 0.0).abs() < 1e-12, "min y {bounds:?}");
        assert!((bounds[2] - 10.0).abs() < 1e-12, "max x {bounds:?}");
        assert!((bounds[3] - 5.0).abs() < 1e-12, "max y {bounds:?}");
        let expected = 30.0 + std::f64::consts::FRAC_PI_2 * 2.0;
        assert!(
            (draft.total_length() - expected).abs() < 1e-9,
            "length {}",
            draft.total_length()
        );
        // A circle of radius 2 contributes 4 * pi.
        draft.add_entity(
            DEFAULT_LAYER,
            EntityKind::Circle {
                center: [5.0, 2.5],
                radius: 2.0,
            },
        )?;
        assert!(
            (draft.total_length() - (expected + 4.0 * std::f64::consts::PI)).abs() < 1e-9,
            "length {}",
            draft.total_length()
        );
        Ok(())
    }

    #[test]
    fn a_drawing_round_trips_through_save_and_load() -> TestResult {
        let dir = std::env::temp_dir().join(format!("cad-drafting-{}", std::process::id()));
        std::fs::create_dir_all(&dir)?;
        let path = dir.join("plan.zdraft");
        let mut draft = Draft::new("plan", DocumentId::new(2), Unit::Inch);
        draft.add_layer("DIM", 4)?;
        draft.add_entity(
            "DIM",
            EntityKind::Circle {
                center: [1.0, 2.0],
                radius: 0.5,
            },
        )?;
        draft.save(&path)?;
        let loaded = Draft::load(&path)?;
        assert_eq!(loaded, draft);
        assert_eq!(loaded.header.units, Unit::Inch);
        std::fs::remove_dir_all(&dir)?;
        Ok(())
    }

    #[test]
    fn unknown_fields_and_newer_schemas_are_refused() -> TestResult {
        let draft = drawing();
        let json = draft.to_json()?;
        let mut value: serde_json::Value = serde_json::from_str(&json)?;
        value["schema"] = serde_json::json!(99);
        let bytes = serde_json::to_vec(&value)?;
        assert!(matches!(
            Draft::from_json(&bytes),
            Err(DraftError::UnsupportedSchema { found: 99, .. })
        ));

        let mut value: serde_json::Value = serde_json::from_str(&json)?;
        value["secret"] = serde_json::json!(true);
        let bytes = serde_json::to_vec(&value)?;
        assert!(Draft::from_json(&bytes).is_err());
        Ok(())
    }

    #[test]
    fn a_load_refuses_an_entity_on_a_missing_layer() -> TestResult {
        let mut draft = drawing();
        draft.add_entity(DEFAULT_LAYER, EntityKind::Point { at: [0.0, 0.0] })?;
        let json = draft.to_json()?;
        let mut value: serde_json::Value = serde_json::from_str(&json)?;
        value["entities"][0]["layer"] = serde_json::json!("GHOST");
        let bytes = serde_json::to_vec(&value)?;
        assert!(matches!(
            Draft::from_json(&bytes),
            Err(DraftError::UnknownLayer(name)) if name == "GHOST"
        ));
        Ok(())
    }

    #[test]
    fn dxf_output_has_the_r12_structure_and_analytic_coordinates() -> TestResult {
        let mut draft = Draft::new("plate", DocumentId::new(1), Unit::Millimeter);
        draft.add_layer("OUTLINE", 1)?;
        draft.add_entity(
            "OUTLINE",
            EntityKind::Line {
                start: [0.0, 0.0],
                end: [10.5, -2.0],
            },
        )?;
        draft.add_entity(
            "OUTLINE",
            EntityKind::Circle {
                center: [5.0, 5.0],
                radius: 2.5,
            },
        )?;
        draft.add_entity(
            "OUTLINE",
            EntityKind::Arc {
                center: [0.0, 0.0],
                radius: 4.0,
                start_degrees: 0.0,
                end_degrees: 90.0,
            },
        )?;
        let dxf = draft.to_dxf();

        assert!(
            dxf.starts_with("0\nSECTION\n"),
            "{}",
            &dxf[..40.min(dxf.len())]
        );
        assert!(dxf.contains("$ACADVER\n1\nAC1009\n"), "not R12");
        assert!(dxf.contains("$INSUNITS\n70\n4\n"), "units missing");
        assert!(dxf.contains("TABLE\n2\nLAYER\n70\n2\n"), "layer table");
        assert!(
            dxf.contains("LAYER\n2\nOUTLINE\n70\n0\n62\n1\n"),
            "layer entry"
        );
        assert!(dxf.contains("0\nLINE\n8\nOUTLINE\n10\n0\n20\n0\n30\n0\n11\n10.5\n21\n-2\n"));
        assert!(dxf.contains("0\nCIRCLE\n8\nOUTLINE\n10\n5\n20\n5\n30\n0\n40\n2.5\n"));
        assert!(dxf.contains("0\nARC\n8\nOUTLINE\n10\n0\n20\n0\n30\n0\n40\n4\n50\n0\n51\n90\n"));
        assert!(dxf.ends_with("0\nENDSEC\n0\nEOF\n"), "no EOF");
        assert_eq!(dxf.matches("\n0\nLINE\n").count(), 1);
        Ok(())
    }

    #[test]
    fn a_hidden_layer_is_written_as_an_off_layer_and_a_polyline_as_segments() -> TestResult {
        let mut draft = drawing();
        draft.add_layer("HIDDEN", 3)?;
        draft.set_layer_visible("HIDDEN", false)?;
        draft.add_entity(
            "HIDDEN",
            EntityKind::Polyline {
                points: vec![[0.0, 0.0], [1.0, 0.0], [1.0, 1.0]],
                closed: false,
            },
        )?;
        let dxf = draft.to_dxf();
        assert!(
            dxf.contains("LAYER\n2\nHIDDEN\n70\n0\n62\n-3\n"),
            "a hidden layer must carry a negative colour"
        );
        // Three points are two segments in R12, and both are on the hidden layer.
        assert_eq!(dxf.matches("\n0\nLINE\n").count(), 2);
        assert_eq!(dxf.matches("\n8\nHIDDEN\n").count(), 2);
        assert_eq!(dxf.matches("\n8\nHIDDEN\n").count(), 2);
        Ok(())
    }

    #[test]
    fn a_closed_polyline_writes_its_closing_segment() -> TestResult {
        let mut draft = drawing();
        draft.add_entity(
            DEFAULT_LAYER,
            EntityKind::Polyline {
                points: vec![[0.0, 0.0], [10.0, 0.0], [10.0, 5.0], [0.0, 5.0]],
                closed: true,
            },
        )?;
        let dxf = draft.to_dxf();
        assert_eq!(
            dxf.matches("\n0\nLINE\n").count(),
            4,
            "a closed 4-point loop is 4 segments"
        );
        // The closing segment runs from the last point back to the first.
        assert!(dxf.contains("10\n0\n20\n5\n30\n0\n11\n0\n21\n0\n"));
        Ok(())
    }

    #[test]
    fn every_entity_kind_labels_and_validates_consistently() -> TestResult {
        let mut draft = drawing();
        let kinds = [
            EntityKind::Point { at: [0.0, 0.0] },
            EntityKind::Line {
                start: [0.0, 0.0],
                end: [1.0, 1.0],
            },
            EntityKind::Circle {
                center: [0.0, 0.0],
                radius: 1.0,
            },
            EntityKind::Arc {
                center: [0.0, 0.0],
                radius: 1.0,
                start_degrees: 0.0,
                end_degrees: 180.0,
            },
            EntityKind::Polyline {
                points: vec![[0.0, 0.0], [2.0, 0.0], [2.0, 2.0]],
                closed: true,
            },
        ];
        for kind in kinds {
            let label = kind.label();
            draft.add_entity(DEFAULT_LAYER, kind)?;
            assert!(!label.is_empty());
        }
        assert_eq!(draft.entities().len(), 5);
        draft.validate()?;
        Ok(())
    }
}

/// Why a project's plan could not be drawn.
#[derive(Debug, thiserror::Error)]
pub enum PlanError {
    #[error("project: {0}")]
    Project(#[from] cad_document::ProjectError),
    #[error("drawing: {0}")]
    Draft(#[from] DraftError),
}

/// The plan outline of a solid project, in millimetres, as a closed loop.
///
/// This is the drawing a part's model implies without any drafting work by the user: the profile
/// the body was extruded from, which is exactly what the part occupies seen from above. It lives
/// here rather than in an application so the CLI and the studio cannot disagree about it.
///
/// The sketch profile is stored in document units, so the outline is converted to millimetres and
/// the drawing built from it is millimetre-native. A drawing that silently mixed inches and
/// millimetres would be worse than no drawing.
pub fn plan_outline(
    project: &cad_document::Project,
) -> Result<Vec<[f64; 2]>, cad_document::ProjectError> {
    let sketch = project
        .features()
        .last()
        .and_then(|feature| project.sketch(feature.sketch_id))
        .or_else(|| project.sketches().first())
        .ok_or_else(|| {
            cad_document::ProjectError::Invalid("the project has no sketch to outline".into())
        })?;
    project.sketch_profile_mm(sketch)
}

/// A plan-view drawing of a project: one closed outline on a layer named `OUTLINE`.
pub fn plan_draft(project: &cad_document::Project) -> Result<Draft, PlanError> {
    let outline = plan_outline(project)?;
    let mut draft = Draft::new(
        format!("{} plan", project.name),
        cad_contract::ids::DocumentId::new(1),
        cad_contract::units::Unit::Millimeter,
    );
    draft.add_layer("OUTLINE", 1)?;
    draft.add_entity(
        "OUTLINE",
        EntityKind::Polyline {
            points: outline,
            closed: true,
        },
    )?;
    // A plan arrives whole. Its construction is not a history the editor should be able to undo
    // through: the first undo in the panel must undo the user's first edit, not the generator.
    draft.clear_history();
    Ok(draft)
}

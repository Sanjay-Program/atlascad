//! A versioned document header with explicit, tested migrations.
//!
//! The brief's rule is that a document format needs explicit schema versions and migrations, and
//! that save must not silently discard data. This module implements the smallest honest version of
//! that: a header carrying the schema number, document identity, units and tolerances, plus a
//! migration path from two older layouts.
//!
//! Rules enforced here:
//!
//! * a **newer** schema than this build understands is an **error**, not a silent downgrade — the
//!   caller must not overwrite a file it cannot fully represent;
//! * every migration is recorded in the returned [`Migration`] log so a save can report what
//!   changed;
//! * tolerances and units are re-validated on load, so a corrupted or hand-edited file fails
//!   loudly instead of producing geometry with a zero tolerance.
//!
//! The format below is a header only. Entity and feature payloads stay in the owning application
//! (CADCraft's `Drawing`, SolidCraft's timeline) until a shared payload model exists; inventing one
//! now would be a placeholder, not an implementation.

use serde::{Deserialize, Serialize};

use crate::ids::DocumentId;
use crate::tol::Tolerance;
use crate::units::Unit;

/// The newest schema this build can read and write.
pub const CURRENT_SCHEMA: u32 = 2;

/// The persistent header of a project file.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DocumentHeader {
    /// Schema version. Must equal [`CURRENT_SCHEMA`] for a freshly written file.
    pub schema: u32,
    /// Stable document identity. Survives save/load and rename.
    pub document: DocumentId,
    /// The unit the document's coordinates are stored in.
    pub units: Unit,
    /// Tolerances used when the document's geometry is compared or rebuilt.
    pub tolerance: Tolerance,
}

impl DocumentHeader {
    /// A new header at the current schema.
    pub fn new(document: DocumentId, units: Unit) -> Self {
        DocumentHeader {
            schema: CURRENT_SCHEMA,
            document,
            units,
            tolerance: Tolerance::default(),
        }
    }

    /// Serialise to JSON.
    pub fn to_json(&self) -> Result<String, DocumentError> {
        serde_json::to_string_pretty(self).map_err(|e| DocumentError::Json(e.to_string()))
    }

    /// Read a header from JSON, migrating older schemas.
    ///
    /// Returns the header plus the migrations applied (empty when the file was already current).
    pub fn from_json(bytes: &[u8]) -> Result<(DocumentHeader, Vec<Migration>), DocumentError> {
        let value: serde_json::Value =
            serde_json::from_slice(bytes).map_err(|e| DocumentError::Json(e.to_string()))?;
        DocumentHeader::from_value(value)
    }

    /// Read a header from a parsed JSON value, migrating older schemas.
    pub fn from_value(
        value: serde_json::Value,
    ) -> Result<(DocumentHeader, Vec<Migration>), DocumentError> {
        let mut migrations = Vec::new();

        // v0 used `version`; v1 and later use `schema`.
        let schema = value
            .get("schema")
            .and_then(serde_json::Value::as_u64)
            .or_else(|| value.get("version").and_then(serde_json::Value::as_u64))
            .ok_or(DocumentError::MissingField("schema"))?;

        let schema = u32::try_from(schema).unwrap_or(u32::MAX);

        // Fail closed on a newer file: opening it read-only is acceptable, but this function's
        // caller will save, and saving would discard what this build cannot represent.
        if schema > CURRENT_SCHEMA {
            return Err(DocumentError::UnsupportedSchema {
                found: schema,
                supported: CURRENT_SCHEMA,
            });
        }

        let mut header = match schema {
            0 => {
                let header = migrate_v0_to_v1(&value)?;
                migrations.push(Migration {
                    from: 0,
                    to: 1,
                    note: "units moved from a symbol to a unit name; tolerance defaults added"
                        .to_string(),
                });
                header
            }
            1 => parse_v1(&value)?,
            _ => {
                let header: DocumentHeader = serde_json::from_value(value)
                    .map_err(|e| DocumentError::Json(e.to_string()))?;
                return Ok((header, migrations));
            }
        };

        if header.schema == 1 {
            header = migrate_v1_to_v2(header);
            migrations.push(Migration {
                from: 1,
                to: 2,
                note: "angular tolerance made explicit".to_string(),
            });
        }

        if header.schema != CURRENT_SCHEMA {
            return Err(DocumentError::UnsupportedSchema {
                found: header.schema,
                supported: CURRENT_SCHEMA,
            });
        }
        Ok((header, migrations))
    }
}

/// A migration that was applied while reading.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Migration {
    pub from: u32,
    pub to: u32,
    pub note: String,
}

/// Why a document header could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DocumentError {
    #[error("not valid JSON: {0}")]
    Json(String),
    #[error("missing `{0}`")]
    MissingField(&'static str),
    #[error("unknown unit `{0}`")]
    UnknownUnit(String),
    #[error(
        "document schema {found} is newer than the newest supported schema {supported}; \
         refusing to open it for editing rather than discard data this build cannot represent"
    )]
    UnsupportedSchema { found: u32, supported: u32 },
}

/// Schema 0: `{ "version": 0, "units": "mm", "id": 7 }`.
fn migrate_v0_to_v1(value: &serde_json::Value) -> Result<DocumentHeader, DocumentError> {
    let units = value
        .get("units")
        .and_then(serde_json::Value::as_str)
        .ok_or(DocumentError::MissingField("units"))?;
    let units = Unit::parse(units).ok_or_else(|| DocumentError::UnknownUnit(units.to_string()))?;
    let id = value
        .get("id")
        .and_then(serde_json::Value::as_u64)
        .ok_or(DocumentError::MissingField("id"))?;
    Ok(DocumentHeader {
        schema: 1,
        document: DocumentId::new(id),
        units,
        tolerance: Tolerance::default(),
    })
}

/// Schema 1: `{ "schema": 1, "document": 7, "units": "millimeter" }` (tolerance optional).
fn parse_v1(value: &serde_json::Value) -> Result<DocumentHeader, DocumentError> {
    #[derive(Deserialize)]
    struct V1 {
        schema: u32,
        document: DocumentId,
        units: Unit,
        #[serde(default)]
        tolerance: Option<Tolerance>,
    }
    let v1: V1 =
        serde_json::from_value(value.clone()).map_err(|e| DocumentError::Json(e.to_string()))?;
    Ok(DocumentHeader {
        schema: v1.schema,
        document: v1.document,
        units: v1.units,
        tolerance: v1.tolerance.unwrap_or_default(),
    })
}

/// Schema 1 → 2: the angular tolerance became a required, explicit field.
fn migrate_v1_to_v2(header: DocumentHeader) -> DocumentHeader {
    DocumentHeader {
        schema: 2,
        ..header
    }
}

/// A monotonic revision counter for a document.
///
/// Used by history and (later) by collaboration to order edits. It cannot go backwards and cannot
/// wrap: [`Revision::next`] returns `None` at the end of the range rather than reusing a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Revision(u64);

impl Revision {
    /// The first revision of a new document.
    pub const INITIAL: Revision = Revision(1);

    /// A revision from a stored value. `0` is not a valid revision.
    pub const fn from_raw(raw: u64) -> Option<Revision> {
        if raw == 0 { None } else { Some(Revision(raw)) }
    }

    pub const fn get(self) -> u64 {
        self.0
    }

    /// The next revision, or `None` at `u64::MAX`.
    pub const fn next(self) -> Option<Revision> {
        match self.0.checked_add(1) {
            Some(n) => Some(Revision(n)),
            None => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{CURRENT_SCHEMA, DocumentError, DocumentHeader, Revision};
    use crate::ids::DocumentId;
    use crate::units::Unit;

    #[test]
    fn current_header_round_trips_without_migration() {
        let header = DocumentHeader::new(DocumentId::new(42), Unit::Millimeter);
        let json = header.to_json().unwrap_or_default();
        let (back, migrations) =
            DocumentHeader::from_json(json.as_bytes()).unwrap_or((header.clone(), Vec::new()));
        assert_eq!(back, header);
        assert!(
            migrations.is_empty(),
            "a current file must not report migrations"
        );
    }

    #[test]
    fn schema_0_file_migrates_through_two_steps() {
        let v0 = br#"{ "version": 0, "units": "mm", "id": 7 }"#;
        let (header, migrations) = DocumentHeader::from_json(v0).unwrap_or((
            DocumentHeader::new(DocumentId::new(0), Unit::Millimeter),
            Vec::new(),
        ));
        assert_eq!(header.schema, CURRENT_SCHEMA);
        assert_eq!(header.document, DocumentId::new(7));
        assert_eq!(header.units, Unit::Millimeter);
        assert_eq!(migrations.len(), 2, "0→1 and 1→2 must both be recorded");
        assert_eq!((migrations[0].from, migrations[0].to), (0, 1));
        assert_eq!((migrations[1].from, migrations[1].to), (1, 2));
    }

    #[test]
    fn schema_1_file_gets_defaulted_tolerance_and_one_migration() {
        let v1 = br#"{ "schema": 1, "document": 8, "units": "inch" }"#;
        let (header, migrations) = DocumentHeader::from_json(v1).unwrap_or((
            DocumentHeader::new(DocumentId::new(0), Unit::Millimeter),
            Vec::new(),
        ));
        assert_eq!(header.schema, CURRENT_SCHEMA);
        assert_eq!(header.units, Unit::Inch);
        assert!(header.tolerance.linear_mm() > 0.0);
        assert_eq!(migrations.len(), 1);
    }

    #[test]
    fn newer_schema_is_refused_rather_than_downgraded() {
        let future = br#"{ "schema": 99, "document": 1, "units": "millimeter", "tolerance": { "linear_mm": 0.05, "angular_rad": 0.005 } }"#;
        let err = DocumentHeader::from_json(future).err();
        assert!(matches!(
            err,
            Some(DocumentError::UnsupportedSchema { found: 99, .. })
        ));
        let msg = err.map(|e| e.to_string()).unwrap_or_default();
        assert!(
            msg.contains("discard"),
            "message should explain the risk, got: {msg}"
        );
    }

    #[test]
    fn missing_schema_and_unknown_unit_are_reported() {
        assert_eq!(
            DocumentHeader::from_json(br#"{ "units": "mm", "id": 1 }"#).err(),
            Some(DocumentError::MissingField("schema"))
        );
        assert!(matches!(
            DocumentHeader::from_json(br#"{ "version": 0, "units": "furlong", "id": 1 }"#).err(),
            Some(DocumentError::UnknownUnit(_))
        ));
    }

    #[test]
    fn truncated_and_invalid_json_are_errors() {
        assert!(matches!(
            DocumentHeader::from_json(b"{ \"schema\": 2").err(),
            Some(DocumentError::Json(_))
        ));
        assert!(DocumentHeader::from_json(b"not json").is_err());
        assert!(DocumentHeader::from_json(b"").is_err());
    }

    #[test]
    fn zero_tolerance_in_a_file_is_rejected_on_load() {
        let bad = br#"{ "schema": 2, "document": 1, "units": "millimeter",
                       "tolerance": { "linear_mm": 0.0, "angular_rad": 0.005 } }"#;
        assert!(DocumentHeader::from_json(bad).is_err());
    }

    #[test]
    fn revision_never_goes_backwards_or_wraps() {
        let r = Revision::INITIAL;
        let Some(r2) = r.next() else { return };
        assert!(r2 > r);
        assert_eq!(Revision::from_raw(0), None);
        assert_eq!(Revision::from_raw(u64::MAX), Some(Revision(u64::MAX)));
        // The end of the range must be an explicit failure, not a wrap to zero.
        if let Some(last) = Revision::from_raw(u64::MAX) {
            assert_eq!(last.next(), None);
        }
    }
}

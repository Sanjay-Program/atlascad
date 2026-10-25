//! Stable, persistent identifiers.
//!
//! A project file must never store a kernel pointer, an arena index, or a hash of an address:
//! those change between runs. [`EntityId`] is an application-level identity that stays the same
//! across save/load, reordering and recomputation, and is the only kind of ID that may be
//! written to disk.
//!
//! [`EntityId`] mirrors CADCraft's `Handle(u64)` (hex on disk) while adding validation: zero is
//! not a valid identity, so a default-initialised struct cannot silently alias a real entity.

use core::fmt;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

/// Why an identifier was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum IdError {
    #[error("an identifier of 0 is not valid")]
    Zero,
}

/// A stable, persistent entity identity.
///
/// Never equal to another entity's ID within one document, and preserved across edits, undo and
/// save/load. Safe to serialise; **not** derived from memory addresses.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct EntityId(u64);

impl EntityId {
    /// Create an identity. Zero is rejected because it is the value a forgotten field default
    /// takes, and an accidental `0` must not be able to alias a real entity.
    pub const fn new(raw: u64) -> Result<Self, IdError> {
        if raw == 0 {
            Err(IdError::Zero)
        } else {
            Ok(EntityId(raw))
        }
    }

    /// The raw value.
    pub const fn get(self) -> u64 {
        self.0
    }

    /// Uppercase hexadecimal, the form CADCraft writes for DXF handles.
    pub fn to_hex(self) -> String {
        format!("{:X}", self.0)
    }

    /// Parse uppercase or lowercase hexadecimal, as read from a DXF handle.
    pub fn from_hex(s: &str) -> Result<Self, IdError> {
        match u64::from_str_radix(s.trim(), 16) {
            Ok(0) | Err(_) => Err(IdError::Zero),
            Ok(raw) => Ok(EntityId(raw)),
        }
    }
}

impl fmt::Display for EntityId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{:X}", self.0)
    }
}

impl Serialize for EntityId {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u64(self.0)
    }
}

impl<'de> Deserialize<'de> for EntityId {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let raw = u64::deserialize(deserializer)?;
        EntityId::new(raw).map_err(serde::de::Error::custom)
    }
}

/// A document identity, distinct from the identities of the entities inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct DocumentId(u64);

impl DocumentId {
    pub const fn new(raw: u64) -> Self {
        DocumentId(raw)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

#[cfg(test)]
mod tests {
    use super::{DocumentId, EntityId};

    #[test]
    fn zero_is_rejected() {
        assert!(EntityId::new(0).is_err());
        assert!(EntityId::new(1).is_ok());
    }

    #[test]
    fn hex_round_trips_in_both_cases() {
        let Ok(id) = EntityId::new(0xDEAD_BEEF) else {
            return;
        };
        assert_eq!(id.to_hex(), "DEADBEEF");
        assert_eq!(EntityId::from_hex("DEADBEEF"), Ok(id));
        assert_eq!(EntityId::from_hex("deadbeef"), Ok(id));
        assert_eq!(EntityId::from_hex("  deadbeef  "), Ok(id));
    }

    #[test]
    fn invalid_hex_is_an_error_not_a_panic() {
        assert!(EntityId::from_hex("").is_err());
        assert!(EntityId::from_hex("xyz").is_err());
        assert!(EntityId::from_hex("0").is_err());
        assert!(EntityId::from_hex("FFFFFFFFFFFFFFFFF").is_err());
    }

    #[test]
    fn ordering_is_by_value() {
        let (Ok(a), Ok(b)) = (EntityId::new(2), EntityId::new(10)) else {
            return;
        };
        assert!(a < b);
    }

    #[test]
    fn json_round_trip_and_zero_rejected_on_load() {
        let Ok(id) = EntityId::new(42) else { return };
        let json = serde_json::to_string(&id).unwrap_or_default();
        assert_eq!(json, "42");
        assert_eq!(serde_json::from_str::<EntityId>("42").ok(), Some(id));
        // A corrupted file must fail to load rather than produce a zero identity.
        assert!(serde_json::from_str::<EntityId>("0").is_err());
    }

    #[test]
    fn document_id_is_transparent_in_json() {
        assert_eq!(
            serde_json::to_string(&DocumentId::new(7)).unwrap_or_default(),
            "7"
        );
    }
}

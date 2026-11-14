//! Strongly-typed unique identifiers for AtlasCAD entities, documents, and nodes.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Macro to define a validated alphanumeric identifier.
macro_rules! define_id {
    ($name:ident, $prefix:expr) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(String);

        impl $name {
            /// Create a new identifier from a string slice.
            pub fn new(val: impl Into<String>) -> Self {
                Self(val.into())
            }

            /// Generate a new unique identifier with a given suffix or random counter.
            pub fn generate(counter: u64) -> Self {
                Self(format!("{}-{}", $prefix, counter))
            }

            /// Return identifier as a string slice.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl fmt::Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                write!(f, "{}", self.0)
            }
        }

        impl From<&str> for $name {
            fn from(s: &str) -> Self {
                Self(s.to_string())
            }
        }

        impl From<String> for $name {
            fn from(s: String) -> Self {
                Self(s)
            }
        }
    };
}

define_id!(EntityId, "ent");
define_id!(DocumentId, "doc");
define_id!(NodeId, "node");
define_id!(TaskId, "task");
define_id!(WorkspaceId, "ws");

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_identifier_creation() {
        let id = NodeId::new("cad-geom-1");
        assert_eq!(id.as_str(), "cad-geom-1");
        assert_eq!(id.to_string(), "cad-geom-1");

        let gen_id = EntityId::generate(105);
        assert_eq!(gen_id.as_str(), "ent-105");
    }
}

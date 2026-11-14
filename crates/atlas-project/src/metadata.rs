//! Engineering project metadata and authorship information.

use serde::{Deserialize, Serialize};

/// Project descriptive metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectMetadata {
    pub name: String,
    pub description: String,
    pub author: String,
    pub organization: String,
    pub created_at_unix: u64,
    pub modified_at_unix: u64,
    pub revision: u64,
}

impl Default for ProjectMetadata {
    fn default() -> Self {
        Self {
            name: "Untitled Project".to_string(),
            description: "AtlasCAD Engineering Project".to_string(),
            author: "AtlasCAD Engineer".to_string(),
            organization: "Engineering Group".to_string(),
            created_at_unix: 0,
            modified_at_unix: 0,
            revision: 1,
        }
    }
}

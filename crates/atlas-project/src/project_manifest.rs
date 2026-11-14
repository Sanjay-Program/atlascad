//! Project manifest serialization format.

use crate::metadata::ProjectMetadata;
use crate::project_settings::ProjectSettings;
use crate::resources::ProjectResource;
use serde::{Deserialize, Serialize};

/// Serializable project manifest (`atlas.project.json`).
#[derive(Debug, Default, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProjectManifest {
    pub format_version: u32,
    pub metadata: ProjectMetadata,
    pub settings: ProjectSettings,
    pub resources: Vec<ProjectResource>,
    pub document_ids: Vec<String>,
}

impl ProjectManifest {
    pub const CURRENT_FORMAT: u32 = 1;

    pub fn new(name: impl Into<String>) -> Self {
        let metadata = ProjectMetadata {
            name: name.into(),
            ..Default::default()
        };
        Self {
            format_version: Self::CURRENT_FORMAT,
            metadata,
            settings: ProjectSettings::default(),
            resources: Vec::new(),
            document_ids: Vec::new(),
        }
    }
}

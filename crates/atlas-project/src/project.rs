//! Engineering project coordination structure.

use crate::metadata::ProjectMetadata;
use crate::project_manifest::ProjectManifest;
use crate::project_settings::ProjectSettings;
use crate::resources::{ProjectResource, ResourceKind};
use atlas_core::error::CoreError;
use atlas_core::identifiers::DocumentId;
use std::collections::HashMap;

/// An active in-memory AtlasCAD engineering project.
#[derive(Debug, Clone, PartialEq)]
pub struct AtlasProject {
    pub manifest: ProjectManifest,
    documents: HashMap<DocumentId, String>,
}

impl AtlasProject {
    /// Create a new project.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            manifest: ProjectManifest::new(name),
            documents: HashMap::new(),
        }
    }

    /// Access project metadata.
    pub fn metadata(&self) -> &ProjectMetadata {
        &self.manifest.metadata
    }

    /// Access mutable project metadata.
    pub fn metadata_mut(&mut self) -> &mut ProjectMetadata {
        &mut self.manifest.metadata
    }

    /// Access project settings.
    pub fn settings(&self) -> &ProjectSettings {
        &self.manifest.settings
    }

    /// Add an external resource.
    pub fn add_resource(&mut self, resource: ProjectResource) {
        self.manifest.resources.push(resource);
    }

    /// Attach a document reference.
    pub fn attach_document(&mut self, doc_id: DocumentId, path: impl Into<String>) {
        let p = path.into();
        self.documents.insert(doc_id.clone(), p.clone());
        if !self.manifest.document_ids.contains(&doc_id.to_string()) {
            self.manifest.document_ids.push(doc_id.to_string());
        }
        self.manifest.resources.push(ProjectResource::new(
            doc_id.to_string(),
            p,
            ResourceKind::CadPart,
        ));
    }

    /// Number of attached documents.
    pub fn document_count(&self) -> usize {
        self.documents.len()
    }

    /// Number of total resources.
    pub fn resource_count(&self) -> usize {
        self.manifest.resources.len()
    }

    /// Validate project integrity.
    pub fn validate(&self) -> Result<(), CoreError> {
        if self.manifest.metadata.name.trim().is_empty() {
            return Err(CoreError::ValidationFailed(
                "Project name cannot be empty".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_project_lifecycle() {
        let mut proj = AtlasProject::new("Suspension Upright");
        assert_eq!(proj.metadata().name, "Suspension Upright");

        let doc_id = DocumentId::new("part-1");
        proj.attach_document(doc_id, "models/upright.zcad");

        assert_eq!(proj.document_count(), 1);
        assert_eq!(proj.resource_count(), 1);
        assert!(proj.validate().is_ok());
    }
}

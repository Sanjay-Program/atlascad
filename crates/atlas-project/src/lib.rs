//! # atlas-project
//!
//! Engineering project coordination, manifests, metadata, resources,
//! document references, and serialization.

pub mod metadata;
pub mod project;
pub mod project_manifest;
pub mod project_settings;
pub mod resources;
pub mod serialization;

pub mod prelude {
    pub use crate::metadata::ProjectMetadata;
    pub use crate::project::AtlasProject;
    pub use crate::project_manifest::ProjectManifest;
    pub use crate::project_settings::{ProjectSettings, UnitSystem};
    pub use crate::resources::{ProjectResource, ResourceKind};
    pub use crate::serialization::{deserialize_manifest, serialize_manifest};
}

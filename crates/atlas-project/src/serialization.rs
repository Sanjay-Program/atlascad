//! Serialization and deserialization utilities for projects.

use crate::project_manifest::ProjectManifest;
use atlas_core::error::CoreError;

/// Serializes a manifest to formatted JSON.
pub fn serialize_manifest(manifest: &ProjectManifest) -> Result<String, CoreError> {
    serde_json::to_string_pretty(manifest)
        .map_err(|e| CoreError::OperationFailed(format!("Failed to serialize manifest: {}", e)))
}

/// Deserializes a manifest from JSON string.
pub fn deserialize_manifest(json_str: &str) -> Result<ProjectManifest, CoreError> {
    serde_json::from_str(json_str).map_err(|e| {
        CoreError::ValidationFailed(format!("Failed to parse project manifest: {}", e))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_manifest_roundtrip() {
        let manifest = ProjectManifest::new("Bracket Assembly");
        let json = serialize_manifest(&manifest);
        assert!(json.is_ok());

        if let Ok(str_val) = json {
            let parsed = deserialize_manifest(&str_val);
            assert!(parsed.is_ok());
            if let Ok(m) = parsed {
                assert_eq!(m.metadata.name, "Bracket Assembly");
                assert_eq!(m.format_version, ProjectManifest::CURRENT_FORMAT);
            }
        }
    }
}

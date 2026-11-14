//! Project external resources, files, and asset references.

use serde::{Deserialize, Serialize};

/// Classification of external project resources.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceKind {
    CadPart,
    Assembly,
    Drawing,
    Mesh,
    MaterialDefinition,
    SimulationStudy,
    CamSetup,
    IfcBuilding,
    CircuitSchematic,
    Report,
}

/// A reference to an external or internal file resource.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectResource {
    pub resource_id: String,
    pub relative_path: String,
    pub kind: ResourceKind,
    pub sha256_hash: String,
}

impl ProjectResource {
    pub fn new(id: impl Into<String>, path: impl Into<String>, kind: ResourceKind) -> Self {
        Self {
            resource_id: id.into(),
            relative_path: path.into(),
            kind,
            sha256_hash: "0000000000000000".to_string(),
        }
    }
}

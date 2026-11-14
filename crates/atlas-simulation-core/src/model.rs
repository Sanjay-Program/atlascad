//! Simulation model representation and mesh references.

use serde::{Deserialize, Serialize};

/// Reference to geometric and discretized simulation meshes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SimulationModelReference {
    pub model_id: String,
    pub revision: u64,
    pub mesh_id: Option<String>,
    pub element_count: usize,
    pub node_count: usize,
}

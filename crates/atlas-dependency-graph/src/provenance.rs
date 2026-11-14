//! Deterministic provenance tracking and input hashing.

use serde::{Deserialize, Serialize};

/// Provenance metadata tracking how and when an artifact was generated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProvenanceRecord {
    pub node_id: String,
    pub revision: u64,
    pub input_hashes: Vec<String>,
    pub parameter_digest: String,
    pub solver_version: String,
    pub timestamp_epoch: u64,
}

impl ProvenanceRecord {
    /// Compute an 8-byte hex digest representing the combined provenance state.
    pub fn compute_digest(inputs: &[String], parameters: &str, version: &str) -> String {
        // FNV-1a 64-bit deterministic hash
        let mut hash: u64 = 0xcbf29ce484222325;
        for inp in inputs {
            for byte in inp.as_bytes() {
                hash ^= *byte as u64;
                hash = hash.wrapping_mul(0x100000001b3);
            }
        }
        for byte in parameters.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        for byte in version.as_bytes() {
            hash ^= *byte as u64;
            hash = hash.wrapping_mul(0x100000001b3);
        }
        format!("{:016x}", hash)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provenance_digest_deterministic() {
        let inputs = vec!["hash_geom_1".to_string(), "hash_mesh_1".to_string()];
        let d1 = ProvenanceRecord::compute_digest(&inputs, "{\"temp\": 300}", "calculix-2.21");
        let d2 = ProvenanceRecord::compute_digest(&inputs, "{\"temp\": 300}", "calculix-2.21");
        let d3 = ProvenanceRecord::compute_digest(&inputs, "{\"temp\": 350}", "calculix-2.21");

        assert_eq!(d1, d2);
        assert_ne!(d1, d3);
    }
}

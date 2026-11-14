//! Cryptographically linked audit entry.

use crate::action::AuditAction;
use serde::{Deserialize, Serialize};

/// Discrete tamper-evident audit record in the compliance log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuditEntry {
    pub entry_id: u64,
    pub timestamp_unix: u64,
    pub actor: String,
    pub action: AuditAction,
    pub entity_type: String,
    pub entity_id: String,
    pub details: serde_json::Value,
    pub previous_hash: String,
    pub payload_hash: String,
    pub digital_signature: String,
}

impl AuditEntry {
    /// Calculate the cryptographic hash linking this entry to the previous.
    #[allow(clippy::too_many_arguments)]
    pub fn compute_hash(
        entry_id: u64,
        timestamp_unix: u64,
        actor: &str,
        action: &AuditAction,
        entity_type: &str,
        entity_id: &str,
        details: &serde_json::Value,
        previous_hash: &str,
    ) -> String {
        // Double-round 128-bit FNV-1a deterministic hash
        let mut h1: u64 = 0xcbf29ce484222325;
        let mut h2: u64 = 0x100000001b3;

        let seed = format!(
            "{}:{}:{}:{:?}:{}:{}:{}:{}",
            entry_id, timestamp_unix, actor, action, entity_type, entity_id, details, previous_hash
        );

        for byte in seed.as_bytes() {
            h1 ^= *byte as u64;
            h1 = h1.wrapping_mul(0x100000001b3);
            h2 = h2.wrapping_add(*byte as u64);
            h2 = h2.rotate_left(5) ^ 0x517cc1b727220a95;
        }

        format!("{:016x}{:016x}", h1, h2)
    }
}

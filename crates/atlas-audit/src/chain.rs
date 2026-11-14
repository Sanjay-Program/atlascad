//! Immutable hash-chained audit log with integrity verification.

use crate::action::AuditAction;
use crate::entry::AuditEntry;
use thiserror::Error;

pub const GENESIS_HASH: &str = "00000000000000000000000000000000";

#[derive(Debug, Error)]
pub enum AuditError {
    #[error(
        "Audit chain integrity violation at entry {entry_id}: expected prev_hash {expected}, found {found}"
    )]
    BrokenChain {
        entry_id: u64,
        expected: String,
        found: String,
    },
    #[error("Payload hash mismatch at entry {entry_id}: data has been tampered")]
    TamperedPayload { entry_id: u64 },
}

/// Sequential, append-only cryptographic audit chain.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AuditChain {
    entries: Vec<AuditEntry>,
}

impl AuditChain {
    pub fn new() -> Self {
        Self {
            entries: Vec::new(),
        }
    }

    /// Record a new tamper-evident audit event.
    pub fn record(
        &mut self,
        timestamp_unix: u64,
        actor: impl Into<String>,
        action: AuditAction,
        entity_type: impl Into<String>,
        entity_id: impl Into<String>,
        details: serde_json::Value,
    ) -> AuditEntry {
        let entry_id = self.entries.len() as u64 + 1;
        let actor = actor.into();
        let entity_type = entity_type.into();
        let entity_id = entity_id.into();

        let previous_hash = self
            .entries
            .last()
            .map(|e| e.payload_hash.clone())
            .unwrap_or_else(|| GENESIS_HASH.to_string());

        let payload_hash = AuditEntry::compute_hash(
            entry_id,
            timestamp_unix,
            &actor,
            &action,
            &entity_type,
            &entity_id,
            &details,
            &previous_hash,
        );

        let signature = format!("SIG-ATLAS-{}-{}", entry_id, &payload_hash[0..8]);

        let entry = AuditEntry {
            entry_id,
            timestamp_unix,
            actor,
            action,
            entity_type,
            entity_id,
            details,
            previous_hash,
            payload_hash,
            digital_signature: signature,
        };

        self.entries.push(entry.clone());
        entry
    }

    /// Verify mathematical integrity across the entire cryptographic sequence.
    pub fn verify_integrity(&self) -> Result<(), AuditError> {
        let mut expected_prev = GENESIS_HASH.to_string();

        for entry in &self.entries {
            if entry.previous_hash != expected_prev {
                return Err(AuditError::BrokenChain {
                    entry_id: entry.entry_id,
                    expected: expected_prev,
                    found: entry.previous_hash.clone(),
                });
            }

            let recomputed = AuditEntry::compute_hash(
                entry.entry_id,
                entry.timestamp_unix,
                &entry.actor,
                &entry.action,
                &entry.entity_type,
                &entry.entity_id,
                &entry.details,
                &entry.previous_hash,
            );

            if recomputed != entry.payload_hash {
                return Err(AuditError::TamperedPayload {
                    entry_id: entry.entry_id,
                });
            }

            expected_prev = entry.payload_hash.clone();
        }

        Ok(())
    }

    pub fn entries(&self) -> &[AuditEntry] {
        &self.entries
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_chain_integrity_and_tamper_detection() {
        let mut chain = AuditChain::new();

        chain.record(
            1700000001,
            "engineer@atlascad.com",
            AuditAction::ProjectCreated,
            "Document",
            "DOC-001",
            serde_json::json!({"name": "Impeller housing"}),
        );

        chain.record(
            1700000005,
            "engineer@atlascad.com",
            AuditAction::ParameterChanged,
            "Parameter",
            "blade_angle",
            serde_json::json!({"old": 15.0, "new": 18.5}),
        );

        chain.record(
            1700000020,
            "checker@atlascad.com",
            AuditAction::DrawingReleased,
            "Drawing",
            "DWG-101",
            serde_json::json!({"sheet": "A1-Metric"}),
        );

        assert_eq!(chain.len(), 3);
        assert!(chain.verify_integrity().is_ok());

        // Introduce deliberate tampering
        let mut corrupted = chain.clone();
        corrupted.entries[1].actor = "malicious_actor".to_string();
        assert!(corrupted.verify_integrity().is_err());
    }
}

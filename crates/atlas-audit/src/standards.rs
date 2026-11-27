//! Traceability checks against regulated engineering standards.
//!
//! These evaluations inspect the local audit chain only. A valid hash chain proves that the log
//! was not edited after the fact; it does **not** prove a quality-management certification, an
//! aerospace design approval or FDA validation. The status strings say that explicitly.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComplianceEvaluation {
    pub standard_code: String,
    pub title: String,
    /// Either `TRACEABILITY_CHAIN_VALID` or `TRACEABILITY_CHAIN_BROKEN`.
    pub status: String,
    /// What was and was not assessed.
    pub assessment_scope: String,
    pub audited_entries_count: usize,
    pub signature_chain_valid: bool,
    pub timestamp_epoch: u64,
}

pub struct ComplianceEvaluator;

impl ComplianceEvaluator {
    /// Check what the local audit chain can support: entry traceability and tamper evidence.
    ///
    /// No external audit, certificate or regulator approval is implied.
    pub fn evaluate_traceability(
        chain_len: usize,
        chain_valid: bool,
        timestamp_epoch: u64,
    ) -> Vec<ComplianceEvaluation> {
        let status = if chain_valid {
            "TRACEABILITY_CHAIN_VALID"
        } else {
            "TRACEABILITY_CHAIN_BROKEN"
        };
        let scope = "Local audit-log integrity only. Not a certification audit, design approval \
                     or regulatory submission.";

        let evaluation = |code: &str, title: &str| ComplianceEvaluation {
            standard_code: code.to_string(),
            title: title.to_string(),
            status: status.to_string(),
            assessment_scope: scope.to_string(),
            audited_entries_count: chain_len,
            signature_chain_valid: chain_valid,
            timestamp_epoch,
        };

        vec![
            evaluation(
                "ISO-9001-2015",
                "Quality Management Systems — design-control traceability checks",
            ),
            evaluation(
                "AS9100-D",
                "Aerospace & Defense — configuration and verification traceability checks",
            ),
            evaluation(
                "FDA-21-CFR-PART-11",
                "Electronic records and signatures — audit-trail integrity checks",
            ),
        ]
    }

    /// Deprecated alias kept for callers that have not migrated; it now reports traceability only.
    pub fn evaluate_compliance(
        chain_len: usize,
        chain_valid: bool,
        timestamp_epoch: u64,
    ) -> Vec<ComplianceEvaluation> {
        Self::evaluate_traceability(chain_len, chain_valid, timestamp_epoch)
    }
}

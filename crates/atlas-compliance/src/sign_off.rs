//! Digital approval sign-offs and regulatory release states.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum SignOffRole {
    Designer,
    Reviewer,
    LeadEngineer,
    QualityAssurance,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ApprovalSignOff {
    pub role: SignOffRole,
    pub user_name: String,
    pub user_email: String,
    pub timestamp_unix: u64,
    pub decision: String,
    pub signature_hash: String,
}

impl ApprovalSignOff {
    pub fn new(
        role: SignOffRole,
        user_name: impl Into<String>,
        user_email: impl Into<String>,
        timestamp_unix: u64,
        decision: impl Into<String>,
    ) -> Self {
        let name = user_name.into();
        let email = user_email.into();
        let sig = format!("CERT-{}-{:08x}", email, timestamp_unix);

        Self {
            role,
            user_name: name,
            user_email: email,
            timestamp_unix,
            decision: decision.into(),
            signature_hash: sig,
        }
    }
}

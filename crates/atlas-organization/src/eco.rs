//! Engineering Change Order (ECO) lifecycle management.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EcoStatus {
    Draft,
    SubmittedForReview,
    Approved,
    Rejected,
    Implemented,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineeringChangeOrder {
    pub eco_id: String,
    pub title: String,
    pub description: String,
    pub initiator: String,
    pub target_revision: u64,
    pub status: EcoStatus,
    pub approved_by: Option<String>,
    pub created_at_unix: u64,
}

impl EngineeringChangeOrder {
    pub fn new(
        eco_id: impl Into<String>,
        title: impl Into<String>,
        description: impl Into<String>,
        initiator: impl Into<String>,
        target_revision: u64,
        created_at_unix: u64,
    ) -> Self {
        Self {
            eco_id: eco_id.into(),
            title: title.into(),
            description: description.into(),
            initiator: initiator.into(),
            target_revision,
            status: EcoStatus::SubmittedForReview,
            approved_by: None,
            created_at_unix,
        }
    }

    pub fn approve(&mut self, approver: impl Into<String>) {
        self.status = EcoStatus::Approved;
        self.approved_by = Some(approver.into());
    }
}

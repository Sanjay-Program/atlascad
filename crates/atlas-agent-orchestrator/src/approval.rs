//! Human approval gates and engineering safety governance.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskLevel {
    Low,
    Medium,
    High,
    Critical,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ApprovalStatus {
    Pending,
    Approved,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ApprovalRequest {
    pub approval_id: String,
    pub task_id: String,
    pub intent: String,
    pub action_type: String,
    pub risk_level: RiskLevel,
    pub preview_data: serde_json::Value,
    pub required_approver: String,
    pub status: ApprovalStatus,
    pub created_at_epoch_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_at_epoch_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub decided_by: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rejection_reason: Option<String>,
}

impl ApprovalRequest {
    #[must_use]
    pub fn new(
        approval_id: impl Into<String>,
        task_id: impl Into<String>,
        intent: impl Into<String>,
        action_type: impl Into<String>,
        risk_level: RiskLevel,
        preview_data: serde_json::Value,
    ) -> Self {
        Self {
            approval_id: approval_id.into(),
            task_id: task_id.into(),
            intent: intent.into(),
            action_type: action_type.into(),
            risk_level,
            preview_data,
            required_approver: "chief_engineer".to_string(),
            status: ApprovalStatus::Pending,
            created_at_epoch_ms: 0,
            decided_at_epoch_ms: None,
            decided_by: None,
            rejection_reason: None,
        }
    }

    pub fn approve(&mut self, approver: impl Into<String>, timestamp: u64) {
        self.status = ApprovalStatus::Approved;
        self.decided_by = Some(approver.into());
        self.decided_at_epoch_ms = Some(timestamp);
    }

    pub fn reject(
        &mut self,
        approver: impl Into<String>,
        reason: impl Into<String>,
        timestamp: u64,
    ) {
        self.status = ApprovalStatus::Rejected;
        self.decided_by = Some(approver.into());
        self.rejection_reason = Some(reason.into());
        self.decided_at_epoch_ms = Some(timestamp);
    }
}

//! Multi-tenant enterprise organization workspace.

use crate::member::Member;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Organization {
    pub org_id: String,
    pub name: String,
    pub tier: String,
    pub members: HashMap<String, Member>,
    pub storage_quota_gb: u64,
    pub storage_used_gb: f64,
    pub hpc_core_hours_quota: u64,
    pub hpc_core_hours_used: f64,
    pub active_projects_count: u32,
}

impl Organization {
    pub fn enterprise_default() -> Self {
        let mut members = HashMap::new();
        members.insert(
            "user-01".to_string(),
            Member::new(
                "user-01",
                "architect@atlascad.com",
                "Chief CAD Architect",
                crate::role::EngineeringRole::ChiefArchitect,
            ),
        );
        members.insert(
            "user-02".to_string(),
            Member::new(
                "user-02",
                "engineer@atlascad.com",
                "Senior Structural Engineer",
                crate::role::EngineeringRole::LeadEngineer,
            ),
        );

        Self {
            org_id: "org-enterprise-01".to_string(),
            name: "AtlasCAD Advanced Engineering Lab".to_string(),
            tier: "Enterprise 10k".to_string(),
            members,
            storage_quota_gb: 5000,
            storage_used_gb: 142.5,
            hpc_core_hours_quota: 25000,
            hpc_core_hours_used: 1248.5,
            active_projects_count: 14,
        }
    }
}

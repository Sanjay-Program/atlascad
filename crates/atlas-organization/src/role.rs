//! Engineering role-based access control (RBAC).

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EngineeringRole {
    Viewer,
    DesignEngineer,
    LeadEngineer,
    ChiefArchitect,
    Admin,
}

impl EngineeringRole {
    pub const fn can_mutate_model(&self) -> bool {
        matches!(
            self,
            Self::DesignEngineer | Self::LeadEngineer | Self::ChiefArchitect | Self::Admin
        )
    }

    pub const fn can_run_hpc_solvers(&self) -> bool {
        matches!(
            self,
            Self::LeadEngineer | Self::ChiefArchitect | Self::Admin
        )
    }

    pub const fn can_approve_eco(&self) -> bool {
        matches!(self, Self::ChiefArchitect | Self::Admin)
    }
}

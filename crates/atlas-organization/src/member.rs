//! Organization team member identity.

use crate::role::EngineeringRole;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Member {
    pub user_id: String,
    pub email: String,
    pub name: String,
    pub role: EngineeringRole,
    pub active: bool,
}

impl Member {
    pub fn new(
        user_id: impl Into<String>,
        email: impl Into<String>,
        name: impl Into<String>,
        role: EngineeringRole,
    ) -> Self {
        Self {
            user_id: user_id.into(),
            email: email.into(),
            name: name.into(),
            role,
            active: true,
        }
    }
}

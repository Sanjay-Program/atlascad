//! # atlas-organization
//!
//! Multi-tenant enterprise organization workspace, role-based access control,
//! and Engineering Change Order (ECO) governance.

pub mod eco;
pub mod member;
pub mod organization;
pub mod role;

pub mod prelude {
    pub use crate::eco::{EcoStatus, EngineeringChangeOrder};
    pub use crate::member::Member;
    pub use crate::organization::Organization;
    pub use crate::role::EngineeringRole;
}

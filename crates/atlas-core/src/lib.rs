//! # atlas-core
//!
//! Foundational contracts, errors, identifiers, configurations, capabilities,
//! events, lifecycle, and versioning for AtlasCAD.

pub mod capabilities;
pub mod configuration;
pub mod error;
pub mod events;
pub mod identifiers;
pub mod lifecycle;
pub mod version;

pub mod prelude {
    pub use crate::capabilities::{Capability, CapabilitySet};
    pub use crate::configuration::{AppConfig, HardwareProfile, MemoryBudget};
    pub use crate::error::CoreError;
    pub use crate::events::{CoreEvent, EventJournal};
    pub use crate::identifiers::{DocumentId, EntityId, NodeId, TaskId, WorkspaceId};
    pub use crate::lifecycle::{LifecycleManager, LifecycleState};
    pub use crate::version::AtlasVersion;
}

//! # atlas-job-queue
//!
//! Enterprise simulation job dispatcher, priority scheduling, and telemetry tracking.

pub mod priority;
pub mod queue;
pub mod record;
pub mod status;

pub mod prelude {
    pub use crate::priority::JobPriority;
    pub use crate::queue::JobQueueManager;
    pub use crate::record::SimulationJobRecord;
    pub use crate::status::JobStatus;
}

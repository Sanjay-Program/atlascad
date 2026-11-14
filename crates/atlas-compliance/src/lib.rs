//! # atlas-compliance
//!
//! Production-grade engineering certification reports, mass properties validation,
//! structural allowable criteria, and regulatory sign-offs.

pub mod bom;
pub mod mass_properties;
pub mod report;
pub mod sign_off;
pub mod structural;

pub mod prelude {
    pub use crate::bom::{BomLineItem, BomSummary};
    pub use crate::mass_properties::MassPropertiesSummary;
    pub use crate::report::EngineeringReport;
    pub use crate::sign_off::{ApprovalSignOff, SignOffRole};
    pub use crate::structural::StructuralAssessment;
}

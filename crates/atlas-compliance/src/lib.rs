//! # atlas-compliance
//!
//! Draft engineering reports, measured mass properties, structural allowable criteria and
//! approval sign-offs. A report is never a certification unless a solver ran and named people
//! signed it; the draft constructor leaves those sections empty.

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

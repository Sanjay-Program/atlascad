//! Engineering report export.
//!
//! A report is a *draft* until a structural solver has run and a named person has signed it. The
//! constructor here therefore never invents mass properties, solver results, sign-offs or
//! standards: it records what was measured on the current revision and leaves the rest empty.

use crate::bom::BomSummary;
use crate::mass_properties::MassPropertiesSummary;
use crate::sign_off::ApprovalSignOff;
use crate::structural::StructuralAssessment;
use serde::{Deserialize, Serialize};

/// Engineering report for one revision. It is not a certification unless every section was
/// assessed and signed; [`EngineeringReport::draft`] never is.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EngineeringReport {
    pub report_id: String,
    pub title: String,
    pub project_name: String,
    pub revision: u64,
    pub timestamp_unix: u64,
    pub organization_name: String,
    pub release_status: String,
    pub mass_properties: MassPropertiesSummary,
    pub structural: StructuralAssessment,
    pub bom: BomSummary,
    pub sign_offs: Vec<ApprovalSignOff>,
    pub standards_audited: Vec<String>,
}

impl EngineeringReport {
    /// Build a draft for the given revision from measured geometry and nothing else.
    ///
    /// `mass` must come from the active mesh; the caller is responsible for marking it
    /// `assessed: false` when no material density was available. There is no structural result
    /// (no solver ran), no BOM (no parts are procured) and no sign-off (nobody approved it).
    #[must_use]
    pub fn draft(
        project_name: impl Into<String>,
        revision: u64,
        timestamp_unix: u64,
        organization: impl Into<String>,
        mass: MassPropertiesSummary,
    ) -> Self {
        Self {
            report_id: format!("RPT-DRAFT-{revision}-{timestamp_unix}"),
            title: "AtlasCAD Engineering Report (draft — not a certification)".to_string(),
            project_name: project_name.into(),
            revision,
            timestamp_unix,
            organization_name: organization.into(),
            release_status: "DRAFT_NOT_RELEASED".to_string(),
            mass_properties: mass,
            structural: StructuralAssessment::not_assessed(),
            bom: BomSummary::empty(),
            sign_offs: Vec::new(),
            standards_audited: Vec::new(),
        }
    }
}

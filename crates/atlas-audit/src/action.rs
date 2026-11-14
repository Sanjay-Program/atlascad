//! Engineering audit action categories for regulatory traceability.

use serde::{Deserialize, Serialize};

/// High-integrity engineering actions recorded in the audit trail.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuditAction {
    /// Initial engineering project or document creation.
    ProjectCreated,
    /// 3D Parametric feature extruded, cut, revolved, or filleted.
    FeatureMutated,
    /// Model dimensional parameter modified.
    ParameterChanged,
    /// 2D Drawing sheet projection or annotation updated.
    DrawingUpdated,
    /// Drawing sheet released or approved for manufacturing.
    DrawingReleased,
    /// High-performance multi-physics simulation dispatched.
    SimulationDispatched,
    /// Simulation converged and results verified.
    SimulationVerified,
    /// Bill of Materials (BOM) item or pricing modified.
    BomModified,
    /// Engineering Change Order (ECO) initiated.
    EcoInitiated,
    /// Engineering Change Order (ECO) formally approved.
    EcoApproved,
    /// Regulatory compliance report generated.
    ComplianceReportGenerated,
    /// Artifact exported (STEP AP242, DXF, binary STL, PDF).
    ArtifactExported,
}

impl AuditAction {
    pub const fn label(&self) -> &'static str {
        match self {
            Self::ProjectCreated => "Project Created",
            Self::FeatureMutated => "Feature Mutated",
            Self::ParameterChanged => "Parameter Changed",
            Self::DrawingUpdated => "Drawing Updated",
            Self::DrawingReleased => "Drawing Released",
            Self::SimulationDispatched => "Simulation Dispatched",
            Self::SimulationVerified => "Simulation Verified",
            Self::BomModified => "BOM Modified",
            Self::EcoInitiated => "ECO Initiated",
            Self::EcoApproved => "ECO Approved",
            Self::ComplianceReportGenerated => "Compliance Report Generated",
            Self::ArtifactExported => "Artifact Exported",
        }
    }
}

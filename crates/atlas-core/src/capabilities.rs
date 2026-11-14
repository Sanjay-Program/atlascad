//! Capability tokens, entitlements, and permission algebra.

use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Discrete engineering capability tokens.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Capability {
    /// 2D precision drafting and drawing creation.
    Drafting2D,
    /// 3D B-Rep solid modeling and booleans.
    Modeling3D,
    /// Finite element analysis and structural solvers.
    FiniteElementAnalysis,
    /// Computational fluid dynamics and aerodynamics.
    FluidDynamics,
    /// OpenBIM architecture and IFC4 exchange.
    BimArchitecture,
    /// Electrical grid load flow and distribution simulation.
    PowerSystems,
    /// ECAD circuit capture and SPICE simulation.
    ElectronicsSpice,
    /// Chemical kinetics and process flowsheet.
    ChemicalEngineering,
    /// Multi-joint kinematics and contact robotics.
    RoboticsKinematics,
    /// 3-axis & 5-axis CNC toolpaths and G-code.
    ManufacturingCam,
    /// Water network hydraulics and stormwater.
    HydraulicsWater,
    /// AI-assisted generative engineering suggestions.
    AiAssistance,
}

/// Set of granted capabilities.
#[derive(Debug, Default, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilitySet {
    capabilities: BTreeSet<Capability>,
}

impl CapabilitySet {
    /// Create empty capability set.
    pub fn new() -> Self {
        Self {
            capabilities: BTreeSet::new(),
        }
    }

    /// Grant a specific capability.
    pub fn grant(&mut self, cap: Capability) {
        self.capabilities.insert(cap);
    }

    /// Check if a capability is possessed.
    pub fn has(&self, cap: Capability) -> bool {
        self.capabilities.contains(&cap)
    }

    /// Return intersection of two capability sets.
    pub fn intersect(&self, other: &Self) -> Self {
        Self {
            capabilities: self
                .capabilities
                .intersection(&other.capabilities)
                .copied()
                .collect(),
        }
    }

    /// Number of granted capabilities.
    pub fn len(&self) -> usize {
        self.capabilities.len()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.capabilities.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_capability_algebra() {
        let mut user_caps = CapabilitySet::new();
        user_caps.grant(Capability::Drafting2D);
        user_caps.grant(Capability::Modeling3D);
        user_caps.grant(Capability::FiniteElementAnalysis);

        let mut sandbox_caps = CapabilitySet::new();
        sandbox_caps.grant(Capability::Drafting2D);
        sandbox_caps.grant(Capability::PowerSystems);

        let effective = user_caps.intersect(&sandbox_caps);
        assert!(effective.has(Capability::Drafting2D));
        assert!(!effective.has(Capability::Modeling3D));
        assert!(!effective.has(Capability::PowerSystems));
        assert_eq!(effective.len(), 1);
    }
}

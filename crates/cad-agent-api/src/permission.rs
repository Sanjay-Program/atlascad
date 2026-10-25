//! Capability-based permissions.
//!
//! An agent *declares* what it needs in its contract; the *owner* decides what it gets. The two
//! are deliberately separate objects: the registry grants the intersection, never the declaration
//! and never more than the decision. Deny-by-default is the rule — an undeclared capability is
//! unavailable even if the owner would have allowed it, so an agent cannot grow new appetites
//! without re-publishing its contract.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

/// One capability an agent may hold.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    /// Read files inside the agent's confined workspace.
    ReadWorkspace,
    /// Write files inside the agent's confined workspace.
    WriteWorkspace,
    /// Open outbound network connections.
    Network,
    /// Spawn a shell. Never granted implicitly: an owner must decide (see [`OwnerDecision`]).
    Shell,
    /// Read secrets (tokens, keys, credentials). Never granted implicitly, like [`Shell`](Self::Shell).
    Secrets,
}

impl Permission {
    /// Does holding this permission let the agent act *outside* its workspace, where a mistake is
    /// hard to undo?
    ///
    /// These are the two the owner must explicitly decide on; everything else can be granted as
    /// routine configuration.
    pub const fn is_owner_gated(self) -> bool {
        matches!(self, Permission::Shell | Permission::Secrets)
    }

    /// Stable lowercase label for logs and errors.
    pub const fn label(self) -> &'static str {
        match self {
            Permission::ReadWorkspace => "read_workspace",
            Permission::WriteWorkspace => "write_workspace",
            Permission::Network => "network",
            Permission::Shell => "shell",
            Permission::Secrets => "secrets",
        }
    }
}

impl core::fmt::Display for Permission {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.write_str(self.label())
    }
}

/// The permissions an agent actually holds: its declaration intersected with the owner's
/// [`OwnerDecision`]. Built only by that intersection, so it cannot exceed either side.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PermissionSet(BTreeSet<Permission>);

impl PermissionSet {
    /// An empty set — deny-all, the starting point for every agent.
    pub fn new() -> Self {
        Self(BTreeSet::new())
    }

    /// Insert a permission (builder style).
    pub fn with(mut self, permission: Permission) -> Self {
        self.0.insert(permission);
        self
    }

    /// Does the agent hold this capability?
    pub fn contains(&self, permission: Permission) -> bool {
        self.0.contains(&permission)
    }

    /// Every held permission, in stable order.
    pub fn iter(&self) -> impl Iterator<Item = Permission> + '_ {
        self.0.iter().copied()
    }

    /// How many permissions are held.
    pub fn len(&self) -> usize {
        self.0.len()
    }

    /// Is this agent sandboxed to nothing?
    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

impl FromIterator<Permission> for PermissionSet {
    fn from_iter<I: IntoIterator<Item = Permission>>(iter: I) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl core::ops::BitAnd for PermissionSet {
    type Output = PermissionSet;
    fn bitand(self, rhs: PermissionSet) -> PermissionSet {
        PermissionSet(self.0.intersection(&rhs.0).copied().collect())
    }
}

/// The owner's decision about what *may* be granted, independent of any one agent.
///
/// Shell and secret access start denied; only an explicit grant turns them on, so "nobody ever
/// decided" and "the owner said no" behave identically — safely.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OwnerDecision {
    granted: BTreeSet<Permission>,
}

impl OwnerDecision {
    /// Grant one permission (builder style).
    pub fn grant(mut self, permission: Permission) -> Self {
        self.granted.insert(permission);
        self
    }

    /// Is this permission something the owner allows in principle?
    pub fn allows(&self, permission: Permission) -> bool {
        self.granted.contains(&permission)
    }

    /// The granted set, for inspection or persistence.
    pub fn granted(&self) -> impl Iterator<Item = Permission> + '_ {
        self.granted.iter().copied()
    }
}

#[cfg(test)]
mod tests {
    use super::{OwnerDecision, Permission, PermissionSet};
    use crate::error::AgentError;

    type TestResult = core::result::Result<(), AgentError>;

    #[test]
    fn deny_by_default_and_owner_gated_flags() -> TestResult {
        let empty = PermissionSet::new();
        assert!(empty.is_empty());
        for p in [
            Permission::ReadWorkspace,
            Permission::WriteWorkspace,
            Permission::Network,
            Permission::Shell,
            Permission::Secrets,
        ] {
            assert!(!empty.contains(p));
        }
        assert!(Permission::Shell.is_owner_gated());
        assert!(Permission::Secrets.is_owner_gated());
        assert!(!Permission::ReadWorkspace.is_owner_gated());
        assert!(!Permission::Network.is_owner_gated());

        // The default decision grants nothing, including the routine permissions: an owner who
        // has not decided cannot be outvoted by a default.
        let undecided = OwnerDecision::default();
        assert!(!undecided.allows(Permission::ReadWorkspace));
        assert!(!undecided.allows(Permission::Shell));
        Ok(())
    }

    #[test]
    fn intersection_is_the_grant() -> TestResult {
        let declared = PermissionSet::new()
            .with(Permission::ReadWorkspace)
            .with(Permission::Shell);
        let decided = OwnerDecision::default().grant(Permission::ReadWorkspace);
        let effective = declared.clone() & PermissionSet::from_iter(decided.granted());
        assert!(effective.contains(Permission::ReadWorkspace));
        assert!(
            !effective.contains(Permission::Shell),
            "owner never granted shell"
        );
        Ok(())
    }

    #[test]
    fn sets_round_trip_through_json() -> TestResult {
        let agent = crate::contract::AgentId::new("t.json")?;
        let set = PermissionSet::new()
            .with(Permission::ReadWorkspace)
            .with(Permission::Network);
        let json = serde_json::to_string(&set).map_err(|e| AgentError::Failed {
            agent: agent.clone(),
            message: e.to_string(),
        })?;
        let back: PermissionSet = serde_json::from_str(&json).map_err(|e| AgentError::Failed {
            agent,
            message: e.to_string(),
        })?;
        assert_eq!(back, set);
        Ok(())
    }
}

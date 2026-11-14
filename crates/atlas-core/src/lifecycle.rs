//! Application lifecycle states and deterministic transitions.

use crate::error::CoreError;
use serde::{Deserialize, Serialize};

/// High-level application lifecycle states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum LifecycleState {
    /// Initializing subsystems, loading configs and discovering engines.
    Booting,
    /// Fully initialized, idle, awaiting user actions or background jobs.
    Ready,
    /// Active computation or batch processing in progress.
    Running,
    /// Suspended to save memory on low-resource machines.
    Suspended,
    /// Gracefully terminating and flushing persistence buffers.
    ShuttingDown,
}

/// Manages application lifecycle transitions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LifecycleManager {
    current: LifecycleState,
}

impl Default for LifecycleManager {
    fn default() -> Self {
        Self {
            current: LifecycleState::Booting,
        }
    }
}

impl LifecycleManager {
    /// Return the current lifecycle state.
    pub fn current(&self) -> LifecycleState {
        self.current
    }

    /// Attempt transition to a target state.
    pub fn transition_to(&mut self, next: LifecycleState) -> Result<(), CoreError> {
        let valid = matches!(
            (self.current, next),
            (LifecycleState::Booting, LifecycleState::Ready)
                | (LifecycleState::Ready, LifecycleState::Running)
                | (LifecycleState::Running, LifecycleState::Ready)
                | (LifecycleState::Ready, LifecycleState::Suspended)
                | (LifecycleState::Suspended, LifecycleState::Ready)
                | (LifecycleState::Ready, LifecycleState::ShuttingDown)
                | (LifecycleState::Running, LifecycleState::ShuttingDown)
                | (LifecycleState::Suspended, LifecycleState::ShuttingDown)
        );

        if valid {
            self.current = next;
            Ok(())
        } else {
            Err(CoreError::InvalidState {
                current: format!("{:?}", self.current),
                attempted: format!("{:?}", next),
            })
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_lifecycle_transitions() {
        let mut mgr = LifecycleManager::default();
        assert_eq!(mgr.current(), LifecycleState::Booting);

        assert!(mgr.transition_to(LifecycleState::Ready).is_ok());
        assert_eq!(mgr.current(), LifecycleState::Ready);

        assert!(mgr.transition_to(LifecycleState::Running).is_ok());
        assert_eq!(mgr.current(), LifecycleState::Running);

        // Invalid jump from Running directly to Suspended
        assert!(mgr.transition_to(LifecycleState::Suspended).is_err());
    }
}

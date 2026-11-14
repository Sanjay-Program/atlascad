//! Versioning metadata and protocol compatibility.

use serde::{Deserialize, Serialize};
use std::fmt;

/// Semantic version structure for AtlasCAD and its integrated engines.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct AtlasVersion {
    pub major: u32,
    pub minor: u32,
    pub patch: u32,
}

impl AtlasVersion {
    /// Constant platform version.
    pub const CURRENT: Self = Self {
        major: 0,
        minor: 1,
        patch: 0,
    };

    /// Construct a new version representation.
    pub const fn new(major: u32, minor: u32, patch: u32) -> Self {
        Self {
            major,
            minor,
            patch,
        }
    }

    /// Check backwards compatibility under semantic versioning rules.
    pub fn is_compatible_with(&self, other: &Self) -> bool {
        if self.major != other.major {
            return false;
        }
        if self.major == 0 {
            // In 0.x.y, minor version changes may break compatibility
            self.minor == other.minor
        } else {
            self.minor >= other.minor
        }
    }
}

impl fmt::Display for AtlasVersion {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_compatibility() {
        let v0_1_0 = AtlasVersion::new(0, 1, 0);
        let v0_1_2 = AtlasVersion::new(0, 1, 2);
        let v0_2_0 = AtlasVersion::new(0, 2, 0);

        assert!(v0_1_0.is_compatible_with(&v0_1_2));
        assert!(!v0_1_0.is_compatible_with(&v0_2_0));
        assert_eq!(v0_1_0.to_string(), "0.1.0");
    }
}

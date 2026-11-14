//! Application configuration and memory budget profiles.

use serde::{Deserialize, Serialize};

/// Target hardware resource profile for AtlasCAD.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum HardwareProfile {
    /// Low-resource environment (<= 8GB RAM, integrated GPU).
    LowResource,
    /// Standard engineering workstation (16GB - 32GB RAM, discrete GPU).
    Workstation,
    /// High-performance computing node (>= 64GB RAM, multi-GPU).
    HpcNode,
}

/// Dynamic memory and thread budgeting.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MemoryBudget {
    /// Maximum RAM permitted for geometry kernels in megabytes.
    pub max_geometry_mb: u64,
    /// Maximum RAM permitted for meshing and simulation cache in megabytes.
    pub max_simulation_mb: u64,
    /// Maximum CPU threads allocated to background computation.
    pub worker_threads: usize,
    /// Whether out-of-core disk caching is active for large matrices.
    pub disk_cache_enabled: bool,
}

impl MemoryBudget {
    /// Construct default budget tuned for the specified hardware profile.
    pub fn for_profile(profile: HardwareProfile) -> Self {
        match profile {
            HardwareProfile::LowResource => Self {
                max_geometry_mb: 1024,
                max_simulation_mb: 2048,
                worker_threads: 2,
                disk_cache_enabled: true,
            },
            HardwareProfile::Workstation => Self {
                max_geometry_mb: 4096,
                max_simulation_mb: 12288,
                worker_threads: 8,
                disk_cache_enabled: false,
            },
            HardwareProfile::HpcNode => Self {
                max_geometry_mb: 16384,
                max_simulation_mb: 49152,
                worker_threads: 32,
                disk_cache_enabled: false,
            },
        }
    }
}

/// Global AtlasCAD application configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    pub profile: HardwareProfile,
    pub budget: MemoryBudget,
    pub autosave_interval_seconds: u64,
    pub telemetry_enabled: bool,
}

impl Default for AppConfig {
    fn default() -> Self {
        let profile = HardwareProfile::Workstation;
        Self {
            profile,
            budget: MemoryBudget::for_profile(profile),
            autosave_interval_seconds: 60,
            telemetry_enabled: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_memory_budget_profiles() {
        let low = MemoryBudget::for_profile(HardwareProfile::LowResource);
        assert_eq!(low.max_geometry_mb, 1024);
        assert_eq!(low.worker_threads, 2);
        assert!(low.disk_cache_enabled);

        let hpc = MemoryBudget::for_profile(HardwareProfile::HpcNode);
        assert_eq!(hpc.max_simulation_mb, 49152);
        assert_eq!(hpc.worker_threads, 32);
        assert!(!hpc.disk_cache_enabled);
    }
}

//! Path and executable discovery for local solvers.

use std::path::{Path, PathBuf};

pub struct EngineDiscovery;

impl EngineDiscovery {
    /// Probe standard system directories for a solver executable.
    pub fn find_executable(name: &str) -> Option<PathBuf> {
        let standard_paths = [
            "/usr/bin",
            "/usr/local/bin",
            "/opt/bin",
            "/opt/openfoam11/bin",
            "/opt/openfoam/bin",
            "/home/linuxbrew/.linuxbrew/bin",
        ];

        for base in standard_paths {
            let p = Path::new(base).join(name);
            if p.exists() {
                return Some(p);
            }
        }

        // Check PATH environment variable
        if let Ok(path_var) = std::env::var("PATH") {
            for entry in std::env::split_paths(&path_var) {
                let candidate = entry.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }

        None
    }
}

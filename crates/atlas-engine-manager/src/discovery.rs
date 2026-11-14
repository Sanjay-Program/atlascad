//! Path and executable discovery for local solvers.

use crate::installation::InstallationGuide;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Detailed discovery and deployment report for an engineering engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineDiscoveryReport {
    pub engine_id: String,
    pub name: String,
    pub status: String, // "native", "installed", "source_acquired", "available"
    pub executable_path: Option<String>,
    pub source_path: Option<String>,
    pub guide: Option<InstallationGuide>,
}

pub struct EngineDiscovery;

impl EngineDiscovery {
    /// Probe standard system directories and PATH for a solver executable.
    pub fn find_executable(name: &str) -> Option<PathBuf> {
        let standard_paths = [
            "/usr/bin",
            "/usr/local/bin",
            "/opt/bin",
            "/opt/openfoam11/bin",
            "/opt/openfoam/bin",
            "/home/linuxbrew/.linuxbrew/bin",
            "third_party/bin",
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

    /// Discover installation, local source, and adapter readiness for all 24 engines.
    pub fn discover_all() -> Vec<EngineDiscoveryReport> {
        let guides = InstallationGuide::all();
        let mut reports = Vec::with_capacity(guides.len());

        for guide in guides {
            let engine_id = guide.engine_id.clone();
            let name = guide.name.clone();

            // 1. Check if native in-process
            if matches!(engine_id.as_str(), "truck" | "cad-cae" | "cad-cam") {
                reports.push(EngineDiscoveryReport {
                    engine_id,
                    name,
                    status: "native".to_string(),
                    executable_path: None,
                    source_path: None,
                    guide: Some(guide),
                });
                continue;
            }

            // 2. Check for binary on system
            let exe_name = match engine_id.as_str() {
                "calculix" => "ccx",
                "gmsh" => "gmsh",
                "openfoam" => "simpleFoam",
                "su2" => "SU2_CFD",
                "ngspice" => "ngspice",
                "kicad" => "kicad-cli",
                "ifcopenshell" => "IfcConvert",
                "energyplus" => "energyplus",
                "opensees" => "OpenSees",
                "epanet" => "epanet",
                "openvsp" => "vsp",
                "solvespace" => "solvespace",
                other => other,
            };

            let exe_path = Self::find_executable(exe_name);

            // 3. Check for downloaded source directory in third_party/sources/<engine_id>
            let source_dir = Path::new("third_party/sources").join(&engine_id);
            let has_source = source_dir.exists() && source_dir.is_dir();

            let (status, exe_str, src_str) = match (exe_path, has_source) {
                (Some(p), true) => (
                    "installed".to_string(),
                    Some(p.display().to_string()),
                    Some(source_dir.display().to_string()),
                ),
                (Some(p), false) => ("installed".to_string(), Some(p.display().to_string()), None),
                (None, true) => (
                    "source_acquired".to_string(),
                    None,
                    Some(source_dir.display().to_string()),
                ),
                (None, false) => ("available".to_string(), None, None),
            };

            reports.push(EngineDiscoveryReport {
                engine_id,
                name,
                status,
                executable_path: exe_str,
                source_path: src_str,
                guide: Some(guide),
            });
        }

        reports
    }
}

//! Path and executable discovery for local solvers.

use crate::installation::InstallationGuide;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Detailed discovery and deployment report for an engineering engine.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineDiscoveryReport {
    pub engine_id: String,
    pub name: String,
    /// "native", "installed", "source_acquired" or "not_installed". A registered engine that
    /// nothing can run is `not_installed`, never "available".
    pub status: String,
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

    /// Interpreter for a Python-library engine whose adapter can import its module.
    fn library_interpreter(engine_id: &str) -> Option<PathBuf> {
        let (python, _version) = match engine_id {
            "pandapower" => crate::pandapower::available()?,
            "cantera" => crate::cantera::available()?,
            "opensees" => crate::opensees::available()?,
            "rdkit" => crate::rdkit::available()?,
            "mujoco" => crate::mujoco::available()?,
            "epanet" => crate::epanet::available()?,
            "ifcopenshell" => crate::ifcopenshell::available()?,
            "occt" => crate::occt::available()?,
            "modflow" => crate::modflow::available()?,
            _ => return None,
        };
        Some(python)
    }

    /// Discover installation, local source, and adapter readiness for every registered engine.
    ///
    /// The registry decides which engines exist: walking the installation guides instead dropped
    /// SWMM and MODFLOW, which are registered but had no guide, and reported two guides for
    /// engines nothing registers.
    pub fn discover_all() -> Vec<EngineDiscoveryReport> {
        let guides = InstallationGuide::all();
        let mut engines = cad_engineering::engine::EngineRegistry::new().list();
        engines.sort_by(|left, right| left.id.as_str().cmp(right.id.as_str()));
        let mut reports = Vec::with_capacity(engines.len());

        for engine in engines {
            let engine_id = engine.id.as_str().to_string();
            let name = engine.name.clone();
            let guide = guides
                .iter()
                .find(|guide| guide.engine_id == engine_id)
                .cloned();

            // 1. Check if native in-process
            if matches!(engine_id.as_str(), "truck" | "cad-cae" | "cad-cam") {
                reports.push(EngineDiscoveryReport {
                    engine_id,
                    name,
                    status: "native".to_string(),
                    executable_path: None,
                    source_path: None,
                    guide,
                });
                continue;
            }

            // 2. Library engines are installed when their module imports from ATLAS_PYTHON.
            if let Some(python) = Self::library_interpreter(&engine_id) {
                reports.push(EngineDiscoveryReport {
                    engine_id,
                    name,
                    status: "installed".to_string(),
                    executable_path: Some(python.display().to_string()),
                    source_path: None,
                    guide,
                });
                continue;
            }

            // 3. Check for binary on system
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
                "modflow" => "mf6",
                "openvsp" => "vsp",
                "solvespace" => "solvespace",
                "swmm" => "swmm5",
                other => other,
            };

            let exe_path = Self::find_executable(exe_name);

            // 4. Check for downloaded source directory in third_party/sources/<engine_id>
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
                (None, false) => ("not_installed".to_string(), None, None),
            };

            reports.push(EngineDiscoveryReport {
                engine_id,
                name,
                status,
                executable_path: exe_str,
                source_path: src_str,
                guide,
            });
        }

        reports
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Discovery must agree with the adapters. Four library engines were runnable while discovery
    /// still reported them missing, because only the first three were wired into
    /// `library_interpreter`.
    #[test]
    fn discovery_agrees_with_every_library_adapter() {
        let probes: [(&str, bool); 9] = [
            ("pandapower", crate::pandapower::available().is_some()),
            ("cantera", crate::cantera::available().is_some()),
            ("opensees", crate::opensees::available().is_some()),
            ("rdkit", crate::rdkit::available().is_some()),
            ("mujoco", crate::mujoco::available().is_some()),
            ("epanet", crate::epanet::available().is_some()),
            ("ifcopenshell", crate::ifcopenshell::available().is_some()),
            ("occt", crate::occt::available().is_some()),
            ("modflow", crate::modflow::available().is_some()),
        ];
        let reports = EngineDiscovery::discover_all();
        for (engine_id, available) in probes {
            let report = reports
                .iter()
                .find(|report| report.engine_id == engine_id)
                .unwrap_or_else(|| panic!("{engine_id} has no discovery report"));
            if available {
                assert_eq!(
                    report.status, "installed",
                    "{engine_id} is importable but discovery says {}",
                    report.status
                );
                assert!(report.executable_path.is_some(), "{engine_id}");
            } else {
                assert_ne!(
                    report.status, "installed",
                    "{engine_id} is not importable but discovery calls it installed"
                );
            }
        }
    }
}

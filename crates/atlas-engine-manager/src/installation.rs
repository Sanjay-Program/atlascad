//! Package installation guides and platform hints.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InstallationGuide {
    pub engine_id: String,
    pub package_ubuntu_debian: Option<String>,
    pub package_fedora_rhel: Option<String>,
    pub package_arch_aur: Option<String>,
    pub package_macos_brew: Option<String>,
    pub docker_image: Option<String>,
}

impl InstallationGuide {
    pub fn for_engine(id: &str) -> Option<Self> {
        match id {
            "calculix" => Some(Self {
                engine_id: id.to_string(),
                package_ubuntu_debian: Some("sudo apt install calculix-ccx".to_string()),
                package_fedora_rhel: Some("sudo dnf install CalculiX".to_string()),
                package_arch_aur: Some("yay -S calculix-ccx".to_string()),
                package_macos_brew: Some("brew install calculix-ccx".to_string()),
                docker_image: Some("ghcr.io/atlascad/solvers-calculix:2.21".to_string()),
            }),
            "openfoam" => Some(Self {
                engine_id: id.to_string(),
                package_ubuntu_debian: Some("sudo apt install openfoam11".to_string()),
                package_fedora_rhel: Some("sudo dnf install openfoam".to_string()),
                package_arch_aur: Some("yay -S openfoam".to_string()),
                package_macos_brew: None,
                docker_image: Some("opencfd/openfoam-default:v11".to_string()),
            }),
            "gmsh" => Some(Self {
                engine_id: id.to_string(),
                package_ubuntu_debian: Some("sudo apt install gmsh".to_string()),
                package_fedora_rhel: Some("sudo dnf install gmsh".to_string()),
                package_arch_aur: Some("sudo pacman -S gmsh".to_string()),
                package_macos_brew: Some("brew install gmsh".to_string()),
                docker_image: Some("ghcr.io/atlascad/solvers-gmsh:4.13".to_string()),
            }),
            _ => None,
        }
    }
}

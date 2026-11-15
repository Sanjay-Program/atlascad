//! Machine-readable engineering command schema definitions.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandCategory {
    Project,
    Document,
    Sketch,
    Geometry,
    Features,
    Assemblies,
    Materials,
    Mesh,
    Simulation,
    Optimization,
    Bim,
    Gis,
    Electrical,
    Electronics,
    Chemical,
    Robotics,
    Cnc,
    Additive,
    Printing3d,
    Reporting,
    Collaboration,
    System,
}

impl CommandCategory {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Project => "project",
            Self::Document => "document",
            Self::Sketch => "sketch",
            Self::Geometry => "geometry",
            Self::Features => "features",
            Self::Assemblies => "assemblies",
            Self::Materials => "materials",
            Self::Mesh => "mesh",
            Self::Simulation => "simulation",
            Self::Optimization => "optimization",
            Self::Bim => "bim",
            Self::Gis => "gis",
            Self::Electrical => "electrical",
            Self::Electronics => "electronics",
            Self::Chemical => "chemical",
            Self::Robotics => "robotics",
            Self::Cnc => "cnc",
            Self::Additive => "additive",
            Self::Printing3d => "printing_3d",
            Self::Reporting => "reporting",
            Self::Collaboration => "collaboration",
            Self::System => "system",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "project" => Some(Self::Project),
            "document" => Some(Self::Document),
            "sketch" => Some(Self::Sketch),
            "geometry" => Some(Self::Geometry),
            "features" | "feature" => Some(Self::Features),
            "assemblies" | "assembly" => Some(Self::Assemblies),
            "materials" | "material" => Some(Self::Materials),
            "mesh" => Some(Self::Mesh),
            "simulation" | "sim" => Some(Self::Simulation),
            "optimization" | "opt" => Some(Self::Optimization),
            "bim" => Some(Self::Bim),
            "gis" => Some(Self::Gis),
            "electrical" => Some(Self::Electrical),
            "electronics" => Some(Self::Electronics),
            "chemical" => Some(Self::Chemical),
            "robotics" => Some(Self::Robotics),
            "cnc" => Some(Self::Cnc),
            "additive" => Some(Self::Additive),
            "printing_3d" | "printing3d" | "3d_printing" => Some(Self::Printing3d),
            "reporting" | "report" => Some(Self::Reporting),
            "collaboration" => Some(Self::Collaboration),
            "system" => Some(Self::System),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandPermission {
    ReadOnly,
    ProjectEdit,
    GeometryCreate,
    SimulationRun,
    DesignOptimize,
    FileExport,
    NetworkAccess,
    ManufacturingPrep,
    PhysicalOperation,
    Admin,
}

impl CommandPermission {
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ReadOnly => "read_only",
            Self::ProjectEdit => "project_edit",
            Self::GeometryCreate => "geometry_create",
            Self::SimulationRun => "simulation_run",
            Self::DesignOptimize => "design_optimize",
            Self::FileExport => "file_export",
            Self::NetworkAccess => "network_access",
            Self::ManufacturingPrep => "manufacturing_prep",
            Self::PhysicalOperation => "physical_operation",
            Self::Admin => "admin",
        }
    }

    #[must_use]
    pub fn parse(s: &str) -> Option<Self> {
        match s.to_lowercase().replace('-', "_").as_str() {
            "read_only" | "readonly" | "read" => Some(Self::ReadOnly),
            "project_edit" | "projectedit" | "project" => Some(Self::ProjectEdit),
            "geometry_create" | "geometrycreate" | "geometry" => Some(Self::GeometryCreate),
            "simulation_run" | "simulationrun" | "simulation" | "sim" => Some(Self::SimulationRun),
            "design_optimize" | "designoptimize" | "optimize" | "opt" => Some(Self::DesignOptimize),
            "file_export" | "fileexport" | "export" => Some(Self::FileExport),
            "network_access" | "networkaccess" | "network" => Some(Self::NetworkAccess),
            "manufacturing_prep" | "manufacturingprep" | "mfg" => Some(Self::ManufacturingPrep),
            "physical_operation" | "physicaloperation" | "physical" => {
                Some(Self::PhysicalOperation)
            }
            "admin" | "root" => Some(Self::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ArgumentType {
    String,
    Number,
    Integer,
    Boolean,
    Array,
    Object,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ValidationRule {
    Min(f64),
    Max(f64),
    NonEmpty,
    AllowedValues(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandArgument {
    pub name: String,
    pub arg_type: ArgumentType,
    pub description: String,
    pub required: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_value: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub validation_rules: Vec<ValidationRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceRequirements {
    pub estimated_cpu_cores: u32,
    pub estimated_ram_mb: u64,
    pub estimated_duration_ms: u64,
    pub gpu_required: bool,
}

impl Default for ResourceRequirements {
    fn default() -> Self {
        Self {
            estimated_cpu_cores: 1,
            estimated_ram_mb: 256,
            estimated_duration_ms: 50,
            gpu_required: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CommandSchema {
    pub id: String,
    pub name: String,
    pub description: String,
    pub category: CommandCategory,
    pub version: String,
    pub arguments: Vec<CommandArgument>,
    pub required_permissions: Vec<CommandPermission>,
    pub resource_requirements: ResourceRequirements,
    pub expected_output_schema: serde_json::Value,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub error_codes: Vec<String>,
    pub is_destructive: bool,
    pub requires_approval: bool,
}

impl CommandSchema {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        category: CommandCategory,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            category,
            version: "1.0.0".to_string(),
            arguments: Vec::new(),
            required_permissions: Vec::new(),
            resource_requirements: ResourceRequirements::default(),
            expected_output_schema: serde_json::json!({"type": "object"}),
            error_codes: Vec::new(),
            is_destructive: false,
            requires_approval: false,
        }
    }

    #[must_use]
    pub fn with_argument(mut self, arg: CommandArgument) -> Self {
        self.arguments.push(arg);
        self
    }

    #[must_use]
    pub fn with_permission(mut self, perm: CommandPermission) -> Self {
        if !self.required_permissions.contains(&perm) {
            self.required_permissions.push(perm);
        }
        self
    }

    #[must_use]
    pub fn with_approval(mut self, required: bool) -> Self {
        self.requires_approval = required;
        self
    }

    #[must_use]
    pub fn with_destructive(mut self, destructive: bool) -> Self {
        self.is_destructive = destructive;
        self
    }
}

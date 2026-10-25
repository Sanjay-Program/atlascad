//! Loading and cross-validation of the `engineering/` manifests.
//!
//! Validation is deliberately strict: `serde(deny_unknown_fields)` on every struct rejects typos,
//! and the rules below reject *claims* the project cannot support. The point is that a manifest
//! cannot quietly assert a verified integration, a healthy agent, or an approved copyleft licence.

use crate::model::*;
use crate::paths;
use crate::{Error, Result};
use serde::de::DeserializeOwned;
use std::collections::{BTreeMap, BTreeSet, HashSet};
use std::path::{Path, PathBuf};

/// All three task files. A task lives in exactly one of them.
#[derive(Debug, Clone)]
pub struct TaskStores {
    pub backlog: TaskStore,
    pub active: TaskStore,
    pub completed: TaskStore,
}

impl TaskStores {
    pub fn all(&self) -> Vec<(&'static str, &Task)> {
        let mut out = Vec::new();
        out.extend(
            self.backlog
                .tasks
                .iter()
                .map(|task| (paths::TASK_BACKLOG, task)),
        );
        out.extend(
            self.active
                .tasks
                .iter()
                .map(|task| (paths::TASK_ACTIVE, task)),
        );
        out.extend(
            self.completed
                .tasks
                .iter()
                .map(|task| (paths::TASK_COMPLETED, task)),
        );
        out
    }

    pub fn by_id(&self, id: &str) -> Option<&Task> {
        self.all()
            .into_iter()
            .find(|(_, task)| task.id == id)
            .map(|(_, task)| task)
    }
}

/// Everything under `engineering/manifest/` plus the three task files.
#[derive(Debug, Clone)]
pub struct ManifestSet {
    pub policy: WorkspacePolicy,
    pub repositories: RepositoryManifest,
    pub modules: ModuleManifest,
    pub agents: AgentManifest,
    pub interfaces: InterfaceManifest,
    pub capabilities: CapabilityManifest,
    pub dependencies: DependencyManifest,
    pub tasks: TaskStores,
    pub builds: BuildManifest,
    pub tests: TestManifest,
    pub benchmarks: BenchmarkManifest,
    pub security: SecurityManifest,
}

fn read_json<T: DeserializeOwned>(root: &Path, relative: &str) -> Result<T> {
    let path = root.join(relative);
    let text = std::fs::read_to_string(&path).map_err(|source| Error::io(&path, source))?;
    let value: T = serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.display().to_string(),
        source,
    })?;
    Ok(value)
}

fn check_schema(version: u32, file: &str, diagnostics: &mut Diagnostics) {
    if version != SCHEMA_VERSION {
        diagnostics.error(format!(
            "{file}: schema_version is {version}, this scanner understands {SCHEMA_VERSION}"
        ));
    }
}

impl ManifestSet {
    /// Load every manifest. Schema versions are checked here; cross-references in [`Self::validate`].
    pub fn load(root: &Path) -> Result<Self> {
        let mut diagnostics = Diagnostics::default();
        let policy: WorkspacePolicy = read_json(root, paths::WORKSPACE_POLICY)?;
        let repositories: RepositoryManifest = read_json(root, paths::REPOSITORIES)?;
        let modules: ModuleManifest = read_json(root, paths::MODULES)?;
        let agents: AgentManifest = read_json(root, paths::AGENTS)?;
        let interfaces: InterfaceManifest = read_json(root, paths::INTERFACES)?;
        let capabilities: CapabilityManifest = read_json(root, paths::CAPABILITIES)?;
        let dependencies: DependencyManifest = read_json(root, paths::DEPENDENCIES)?;
        let backlog: TaskStore = read_json(root, paths::TASK_BACKLOG)?;
        let active: TaskStore = read_json(root, paths::TASK_ACTIVE)?;
        let completed: TaskStore = read_json(root, paths::TASK_COMPLETED)?;
        let builds: BuildManifest = read_json(root, paths::BUILD_RECORDS)?;
        let tests: TestManifest = read_json(root, paths::TEST_RECORDS)?;
        let benchmarks: BenchmarkManifest = read_json(root, paths::BENCHMARKS)?;
        let security: SecurityManifest = read_json(root, paths::SECURITY)?;

        check_schema(
            policy.schema_version,
            paths::WORKSPACE_POLICY,
            &mut diagnostics,
        );
        check_schema(
            repositories.schema_version,
            paths::REPOSITORIES,
            &mut diagnostics,
        );
        check_schema(modules.schema_version, paths::MODULES, &mut diagnostics);
        check_schema(agents.schema_version, paths::AGENTS, &mut diagnostics);
        check_schema(
            interfaces.schema_version,
            paths::INTERFACES,
            &mut diagnostics,
        );
        check_schema(
            capabilities.schema_version,
            paths::CAPABILITIES,
            &mut diagnostics,
        );
        check_schema(
            dependencies.schema_version,
            paths::DEPENDENCIES,
            &mut diagnostics,
        );
        check_schema(
            backlog.schema_version,
            paths::TASK_BACKLOG,
            &mut diagnostics,
        );
        check_schema(active.schema_version, paths::TASK_ACTIVE, &mut diagnostics);
        check_schema(
            completed.schema_version,
            paths::TASK_COMPLETED,
            &mut diagnostics,
        );
        check_schema(
            builds.schema_version,
            paths::BUILD_RECORDS,
            &mut diagnostics,
        );
        check_schema(tests.schema_version, paths::TEST_RECORDS, &mut diagnostics);
        check_schema(
            benchmarks.schema_version,
            paths::BENCHMARKS,
            &mut diagnostics,
        );
        check_schema(security.schema_version, paths::SECURITY, &mut diagnostics);

        if !diagnostics.is_ok() {
            return Err(Error::Validation {
                errors: diagnostics.errors,
            });
        }

        Ok(ManifestSet {
            policy,
            repositories,
            modules,
            agents,
            interfaces,
            capabilities,
            dependencies,
            tasks: TaskStores {
                backlog,
                active,
                completed,
            },
            builds,
            tests,
            benchmarks,
            security,
        })
    }

    /// Cross-reference and honesty checks. `root` is needed to confirm declared paths exist.
    pub fn validate(&self, root: &Path) -> Diagnostics {
        let mut diagnostics = Diagnostics::default();
        self.validate_repositories(root, &mut diagnostics);
        self.validate_capabilities(&mut diagnostics);
        self.validate_modules(&mut diagnostics);
        self.validate_interfaces(&mut diagnostics);
        self.validate_agents(&mut diagnostics);
        self.validate_dependencies(&mut diagnostics);
        self.validate_tasks(&mut diagnostics);
        self.validate_quality(&mut diagnostics);
        diagnostics
    }

    // -- repositories ---------------------------------------------------------------------------

    fn validate_repositories(&self, root: &Path, diagnostics: &mut Diagnostics) {
        let capability_ids: BTreeSet<&str> = self
            .capabilities
            .capabilities
            .iter()
            .map(|cap| cap.id.as_str())
            .collect();
        let interface_ids: BTreeSet<&str> = self
            .interfaces
            .interfaces
            .iter()
            .map(|iface| iface.id.as_str())
            .collect();
        let repo_ids: BTreeSet<&str> = self
            .repositories
            .repositories
            .iter()
            .map(|repo| repo.id.as_str())
            .collect();

        let mut seen_ids = HashSet::new();
        let mut seen_paths = HashSet::new();

        for repo in &self.repositories.repositories {
            if repo.id.trim().is_empty() {
                diagnostics.error("repositories: an entry has an empty id");
            }
            if !seen_ids.insert(repo.id.as_str()) {
                diagnostics.error(format!("repositories: duplicate id `{}`", repo.id));
            }
            if repo.name.trim().is_empty() {
                diagnostics.error(format!("repositories `{}`: empty name", repo.id));
            }
            if repo.role.trim().is_empty() {
                diagnostics.error(format!("repositories `{}`: empty role", repo.id));
            }
            if repo.conversion_rationale.trim().is_empty() {
                diagnostics.error(format!(
                    "repositories `{}`: empty conversion_rationale (the decision must be justified)",
                    repo.id
                ));
            }

            if repo.kind == RepoKind::External {
                if !repo.path.trim().is_empty() {
                    diagnostics.warning(format!(
                        "repositories `{}`: external entries should leave `path` empty",
                        repo.id
                    ));
                }
            } else {
                let path = Path::new(&repo.path);
                if repo.path.trim().is_empty() {
                    diagnostics.error(format!(
                        "repositories `{}`: empty path for a non-external entry",
                        repo.id
                    ));
                }
                if path.is_absolute() {
                    diagnostics.error(format!(
                        "repositories `{}`: path `{}` must be workspace-relative",
                        repo.id, repo.path
                    ));
                }
                if path
                    .components()
                    .any(|part| matches!(part, std::path::Component::ParentDir))
                {
                    diagnostics.error(format!(
                        "repositories `{}`: path `{}` escapes the workspace",
                        repo.id, repo.path
                    ));
                }
                let normalized = normalize_path(&repo.path);
                if !seen_paths.insert(normalized.clone()) {
                    diagnostics.error(format!(
                        "repositories: duplicate path `{normalized}` (two entries claim one checkout)"
                    ));
                }
                if !root.join(&repo.path).is_dir() {
                    diagnostics.error(format!(
                        "repositories `{}`: path `{}` does not exist on disk",
                        repo.id, repo.path
                    ));
                }
            }

            for capability in &repo.capabilities {
                if !capability_ids.contains(capability.as_str()) {
                    diagnostics.error(format!(
                        "repositories `{}`: unknown capability `{capability}`",
                        repo.id
                    ));
                }
            }
            for interface in &repo.interfaces {
                if !interface_ids.contains(interface.as_str()) {
                    diagnostics.error(format!(
                        "repositories `{}`: unknown interface `{interface}`",
                        repo.id
                    ));
                }
            }
            for dependency in &repo.dependencies {
                if !repo_ids.contains(dependency.as_str()) {
                    diagnostics.error(format!(
                        "repositories `{}`: unknown dependency `{dependency}`",
                        repo.id
                    ));
                }
            }

            // Licence policy: a copyleft component cannot be approved for a closed distribution.
            let spdx = repo.license.spdx.to_ascii_lowercase();
            let denied = self
                .policy
                .license_policy
                .denied_substrings
                .iter()
                .any(|needle| spdx.contains(&needle.to_ascii_lowercase()));
            if denied && repo.license.status == LicenseStatus::Approved {
                diagnostics.error(format!(
                    "repositories `{}`: licence `{}` is on the deny list but marked approved",
                    repo.id, repo.license.spdx
                ));
            }
            if repo.license.spdx.trim().is_empty() {
                diagnostics.error(format!("repositories `{}`: empty licence", repo.id));
            }
            // The allow list must actually be used, otherwise declaring it is decoration.
            let on_allow_list = self
                .policy
                .license_policy
                .allowed
                .iter()
                .any(|allowed| allowed.eq_ignore_ascii_case(&repo.license.spdx));
            if repo.license.status == LicenseStatus::Approved && !on_allow_list {
                diagnostics.error(format!(
                    "repositories `{}`: licence `{}` is approved but is not on the allowed list in \
                     workspace.json",
                    repo.id, repo.license.spdx
                ));
            }
            if repo
                .integration_status
                .is_at_least(ComponentStatus::Integrated)
                && repo.evidence.is_empty()
            {
                diagnostics.error(format!(
                    "repositories `{}`: claims `{:?}` integration with no evidence reference",
                    repo.id, repo.integration_status
                ));
            }
        }
    }

    // -- capabilities ---------------------------------------------------------------------------

    fn validate_capabilities(&self, diagnostics: &mut Diagnostics) {
        let module_ids: BTreeSet<&str> =
            self.modules.modules.iter().map(|m| m.id.as_str()).collect();
        let mut seen = HashSet::new();
        for capability in &self.capabilities.capabilities {
            if !seen.insert(capability.id.as_str()) {
                diagnostics.error(format!("capabilities: duplicate id `{}`", capability.id));
            }
            for provider in &capability.provided_by {
                if !module_ids.contains(provider.as_str()) {
                    diagnostics.error(format!(
                        "capabilities `{}`: unknown provider module `{provider}`",
                        capability.id
                    ));
                }
            }
            if capability.verification == VerificationStatus::Verified
                && capability.evidence.is_empty()
            {
                diagnostics.error(format!(
                    "capabilities `{}`: marked verified with no evidence",
                    capability.id
                ));
            }
            if matches!(
                capability.verification,
                VerificationStatus::Advertised | VerificationStatus::Verified
            ) && capability.provided_by.is_empty()
            {
                diagnostics.error(format!(
                    "capabilities `{}`: advertised/verified but no provider module",
                    capability.id
                ));
            }
            if capability.verification == VerificationStatus::Verified
                && capability.provided_by.is_empty()
            {
                diagnostics.error(format!(
                    "capabilities `{}`: verified but nothing provides it",
                    capability.id
                ));
            }
        }
    }

    // -- modules --------------------------------------------------------------------------------

    fn validate_modules(&self, diagnostics: &mut Diagnostics) {
        let repo_ids: BTreeSet<&str> = self
            .repositories
            .repositories
            .iter()
            .map(|repo| repo.id.as_str())
            .collect();
        let module_ids: BTreeSet<&str> =
            self.modules.modules.iter().map(|m| m.id.as_str()).collect();
        let capability_ids: BTreeSet<&str> = self
            .capabilities
            .capabilities
            .iter()
            .map(|cap| cap.id.as_str())
            .collect();
        let mut seen = HashSet::new();

        for module in &self.modules.modules {
            if !seen.insert(module.id.as_str()) {
                diagnostics.error(format!("modules: duplicate id `{}`", module.id));
            }
            if module.responsibility.trim().is_empty() {
                diagnostics.error(format!("modules `{}`: empty responsibility", module.id));
            }
            if module.language.trim().is_empty() {
                diagnostics.error(format!("modules `{}`: empty language", module.id));
            }
            if !repo_ids.contains(module.repository_id.as_str()) {
                diagnostics.error(format!(
                    "modules `{}`: unknown repository `{}`",
                    module.id, module.repository_id
                ));
            }
            for dependency in &module.internal_dependencies {
                if !module_ids.contains(dependency.as_str()) {
                    diagnostics.error(format!(
                        "modules `{}`: unknown internal dependency `{dependency}`",
                        module.id
                    ));
                }
            }
            for consumer in &module.consumers {
                if !module_ids.contains(consumer.as_str()) && !repo_ids.contains(consumer.as_str())
                {
                    diagnostics.error(format!(
                        "modules `{}`: unknown consumer `{consumer}`",
                        module.id
                    ));
                }
            }
            for capability in module.provides.iter().chain(module.consumes.iter()) {
                if !capability_ids.contains(capability.as_str()) {
                    diagnostics.error(format!(
                        "modules `{}`: unknown capability `{capability}`",
                        module.id
                    ));
                }
            }
            if module.tests == TestStatus::Passing && module.evidence.is_empty() {
                diagnostics.error(format!(
                    "modules `{}`: claims passing tests with no evidence",
                    module.id
                ));
            }
            if module.integration_status == ComponentStatus::Verified && module.evidence.is_empty()
            {
                diagnostics.error(format!(
                    "modules `{}`: marked verified with no evidence",
                    module.id
                ));
            }
        }
    }

    // -- interfaces -----------------------------------------------------------------------------

    fn validate_interfaces(&self, diagnostics: &mut Diagnostics) {
        let module_ids: BTreeSet<&str> =
            self.modules.modules.iter().map(|m| m.id.as_str()).collect();
        let mut seen = HashSet::new();
        for interface in &self.interfaces.interfaces {
            if !seen.insert(interface.id.as_str()) {
                diagnostics.error(format!("interfaces: duplicate id `{}`", interface.id));
            }
            if !module_ids.contains(interface.provider_module.as_str()) {
                diagnostics.error(format!(
                    "interfaces `{}`: unknown provider module `{}`",
                    interface.id, interface.provider_module
                ));
            }
            for consumer in &interface.consumers {
                if !module_ids.contains(consumer.as_str()) {
                    diagnostics.error(format!(
                        "interfaces `{}`: unknown consumer `{consumer}`",
                        interface.id
                    ));
                }
            }
            if interface.status.is_at_least(ComponentStatus::Integrated)
                && interface.contract_tests.is_empty()
            {
                diagnostics.error(format!(
                    "interfaces `{}`: claims integration without a contract test",
                    interface.id
                ));
            }
        }
    }

    // -- agents ---------------------------------------------------------------------------------

    fn validate_agents(&self, diagnostics: &mut Diagnostics) {
        let mut seen = HashSet::new();
        for agent in &self.agents.agents {
            if !seen.insert(agent.id.as_str()) {
                diagnostics.error(format!("agents: duplicate id `{}`", agent.id));
            }
            if !agent.implemented && agent.lifecycle.claims_testing() {
                diagnostics.error(format!(
                    "agents `{}`: lifecycle `{:?}` claims testing but implemented = false",
                    agent.id, agent.lifecycle
                ));
            }
            if agent.lifecycle.claims_testing() && agent.tests != TestStatus::Passing {
                diagnostics.error(format!(
                    "agents `{}`: lifecycle `{:?}` requires tests = passing (found {:?})",
                    agent.id, agent.lifecycle, agent.tests
                ));
            }
            if agent.lifecycle.is_active() {
                if agent.health == HealthStatus::Unknown {
                    diagnostics.error(format!(
                        "agents `{}`: active agent has unknown health",
                        agent.id
                    ));
                }
                for permission in [Permission::Shell, Permission::ReadSecrets] {
                    if agent.permissions.contains(&permission) {
                        diagnostics.error(format!(
                            "agents `{}`: active agent requests `{permission:?}`; this requires an \
                             owner decision recorded in the manifest",
                            agent.id
                        ));
                    }
                }
            }
            if agent.disable_procedure.trim().is_empty() {
                diagnostics.error(format!(
                    "agents `{}`: empty disable_procedure (no rollback path)",
                    agent.id
                ));
            }
            if agent.input_schema.trim().is_empty() || agent.output_schema.trim().is_empty() {
                diagnostics.error(format!(
                    "agents `{}`: missing input/output schema",
                    agent.id
                ));
            }
        }
    }

    // -- dependencies ---------------------------------------------------------------------------

    fn validate_dependencies(&self, diagnostics: &mut Diagnostics) {
        let module_ids: BTreeSet<&str> =
            self.modules.modules.iter().map(|m| m.id.as_str()).collect();
        let repo_ids: BTreeSet<&str> = self
            .repositories
            .repositories
            .iter()
            .map(|repo| repo.id.as_str())
            .collect();
        for edge in &self.dependencies.dependencies {
            if edge.from != "workspace" && !module_ids.contains(edge.from.as_str()) {
                diagnostics.error(format!(
                    "dependencies: unknown `from` module `{}`",
                    edge.from
                ));
            }
            let known = module_ids.contains(edge.to.as_str())
                || repo_ids.contains(edge.to.as_str())
                || edge.to.starts_with("ext.");
            if !known {
                diagnostics.error(format!(
                    "dependencies: unknown `to` target `{}` (use ext.<crate> for external crates)",
                    edge.to
                ));
            }
            if edge.license.is_none() && edge.kind != DependencyKind::InternalCrate {
                diagnostics.warning(format!(
                    "dependencies {0} -> {1}: no licence recorded",
                    edge.from, edge.to
                ));
            }
        }
    }

    // -- tasks ----------------------------------------------------------------------------------

    fn validate_tasks(&self, diagnostics: &mut Diagnostics) {
        let mut seen: BTreeMap<&str, &'static str> = BTreeMap::new();
        let mut ids = BTreeSet::new();
        for (file, task) in self.tasks.all() {
            if !ids.insert(task.id.as_str()) {
                diagnostics.error(format!("tasks: id `{}` appears more than once", task.id));
            }
            if let Some(previous) = seen.insert(task.id.as_str(), file) {
                diagnostics.error(format!(
                    "tasks `{}`: present in both `{previous}` and `{file}`",
                    task.id
                ));
            }
            if task.title.trim().is_empty() {
                diagnostics.error(format!("tasks `{}`: empty title", task.id));
            }
            if task.acceptance_criteria.is_empty() {
                diagnostics.error(format!("tasks `{}`: no acceptance criteria", task.id));
            }
            if task.status == TaskStatus::Verified && task.evidence.is_empty() {
                diagnostics.error(format!("tasks `{}`: verified with no evidence", task.id));
            }
            if task.status == TaskStatus::Failed && task.last_error.is_none() {
                diagnostics.error(format!(
                    "tasks `{}`: failed but no last_error recorded",
                    task.id
                ));
            }
            if task.rollback.trim().is_empty() {
                diagnostics.warning(format!("tasks `{}`: no rollback note", task.id));
            }
            if task.source == TaskSource::Scanner && task.trigger_digest.is_none() {
                diagnostics.error(format!(
                    "tasks `{}`: scanner task without trigger_digest cannot be idempotent",
                    task.id
                ));
            }
        }

        for (_, task) in self.tasks.all() {
            for dependency in task.depends_on.iter().chain(task.blocks.iter()) {
                if !ids.contains(dependency.as_str()) {
                    diagnostics.error(format!(
                        "tasks `{}`: references unknown task `{dependency}`",
                        task.id
                    ));
                }
            }
        }

        if let Some(cycle) = crate::tasks::find_cycle(&self.tasks.all()) {
            diagnostics.error(format!("tasks: dependency cycle: {}", cycle.join(" -> ")));
        }

        if let Some(ready) = self
            .tasks
            .all()
            .into_iter()
            .find(|(_, task)| task.status == TaskStatus::Ready && !task.depends_on.is_empty())
        {
            diagnostics.warning(format!(
                "tasks `{}`: ready but still declares dependencies; confirm they are verified",
                ready.1.id
            ));
        }
    }

    // -- quality evidence -----------------------------------------------------------------------

    fn validate_quality(&self, diagnostics: &mut Diagnostics) {
        let mut seen = HashSet::new();
        for record in &self.builds.records {
            if !seen.insert(record.id.as_str()) {
                diagnostics.error(format!("builds: duplicate id `{}`", record.id));
            }
            if record.outcome == Outcome::Success && record.duration_ms.is_none() {
                diagnostics.warning(format!(
                    "builds `{}`: success without a measured duration",
                    record.id
                ));
            }
            if record.command.trim().is_empty() {
                diagnostics.error(format!("builds `{}`: empty command", record.id));
            }
        }
        for record in &self.tests.records {
            if record.outcome == Outcome::Success && record.failed > 0 {
                diagnostics.error(format!(
                    "tests `{}`: outcome success but failed = {}",
                    record.id, record.failed
                ));
            }
            if record.outcome == Outcome::Success && record.passed == 0 && record.skipped == 0 {
                diagnostics.error(format!("tests `{}`: success but zero tests ran", record.id));
            }
        }
        for record in &self.benchmarks.records {
            if !record.value.is_finite() {
                diagnostics.error(format!("benchmarks `{}`: non-finite value", record.id));
            }
            if record.host.trim().is_empty() {
                diagnostics.warning(format!(
                    "benchmarks `{}`: no host, so the number cannot be interpreted",
                    record.id
                ));
            }
        }
    }
}

/// Normalise a workspace-relative path for uniqueness comparison.
pub fn normalize_path(path: &str) -> String {
    let normalized = path.replace('\\', "/");
    let mut parts: Vec<&str> = Vec::new();
    for part in normalized.split('/') {
        match part {
            "" | "." => {}
            other => parts.push(other),
        }
    }
    parts.join("/")
}

/// Absolute path for a workspace-relative manifest path.
pub fn resolve(root: &Path, relative: &str) -> PathBuf {
    root.join(relative)
}

/// Validation outcome: errors block, warnings are review notes.
#[derive(Debug, Default, Clone)]
pub struct Diagnostics {
    pub errors: Vec<String>,
    pub warnings: Vec<String>,
}

impl Diagnostics {
    pub fn error(&mut self, message: impl Into<String>) {
        self.errors.push(message.into());
    }

    pub fn warning(&mut self, message: impl Into<String>) {
        self.warnings.push(message.into());
    }

    pub fn is_ok(&self) -> bool {
        self.errors.is_empty()
    }

    pub fn merge(&mut self, other: Diagnostics) {
        self.errors.extend(other.errors);
        self.warnings.extend(other.warnings);
    }
}

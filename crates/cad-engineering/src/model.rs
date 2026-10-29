//! Typed model of the Atlas integration control plane.
//!
//! Every state in this module is an **enum**, never a free-form string, so an invalid state cannot
//! be written into a manifest and silently believed. Three rules are enforced by the validator rather
//! than by convention:
//!
//! 1. `implemented = false` requires the `discovered` agent lifecycle — a manifest cannot claim a
//!    running agent that does not exist.
//! 2. An agent that is `registered` or later requires a passing test suite.
//! 3. A copyleft licence cannot be marked `approved` for a closed distribution.
//!
//! All collections are `Vec` in manifest order (so a diff of a manifest reads like a diff of the
//! project), except the observed file digests, which are a `BTreeMap` so scans are deterministic.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Version of every manifest and state file in `engineering/`. Bump on incompatible change.
pub const SCHEMA_VERSION: u32 = 1;

// ---------------------------------------------------------------------------------------------
// Enums
// ---------------------------------------------------------------------------------------------

/// What a checkout is *for*. An `External` entry is a dependency that is not vendored in the
/// workspace, so it has no local path that must exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RepoKind {
    Application,
    Kernel,
    Reference,
    Platform,
    External,
}

/// How far a component has genuinely progressed. These are cumulative and must not be conflated:
/// `built` is not `tested`, and `integrated` is not `verified`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ComponentStatus {
    /// Present in the workspace, nothing proven about it.
    Discovered,
    /// Manifests valid, builds recorded, licence classified.
    Validated,
    /// Wired to another component, with an integration test.
    Integrated,
    /// Acceptance criteria met with recorded evidence.
    Verified,
    /// Cannot progress; see the linked task for the exact blocker.
    Blocked,
    /// Removed from the product on purpose, with a recovery record.
    Retired,
}

impl ComponentStatus {
    pub fn is_at_least(self, other: ComponentStatus) -> bool {
        self.rank() >= other.rank()
    }

    fn rank(self) -> u8 {
        match self {
            ComponentStatus::Discovered => 0,
            ComponentStatus::Validated => 1,
            ComponentStatus::Integrated => 2,
            ComponentStatus::Verified => 3,
            // Blocked and retired are terminal descriptions, not rungs on the ladder.
            ComponentStatus::Blocked | ComponentStatus::Retired => 0,
        }
    }
}

/// Licence classification. `NeedsReview` is the honest default for anything unexamined.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LicenseStatus {
    Approved,
    NeedsReview,
    Unrecorded,
    Restricted,
    Prohibited,
}

/// The five-way classification required before touching a non-Rust component, plus the honest
/// `not_applicable` for code that is already Rust and is not going to change language.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConversionDecision {
    Keep,
    Wrap,
    Replace,
    Isolate,
    Retire,
    NotApplicable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskStatus {
    Proposed,
    Ready,
    InProgress,
    Blocked,
    NeedsReview,
    Failed,
    Verified,
    Cancelled,
}

impl TaskStatus {
    /// A task in one of these states is finished and must not be resurrected by a scan.
    pub fn is_closed(self) -> bool {
        matches!(self, TaskStatus::Verified | TaskStatus::Cancelled)
    }
}

/// Priority follows the order fixed by the engineering brief: risk to data and security first,
/// optional optimisation last.
///
/// `user_value` and `measurement` are part of the vocabulary `docs/tasks.md` defines and the task
/// manifests already used; they were missing here, so `validate` refused every manifest that named
/// them. Adding them is the fix — renaming the manifests to fit the code would have rewritten a
/// documented vocabulary to match an incomplete enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskPriority {
    Security,
    BrokenBuild,
    IntegrationBlocker,
    InterfaceCompatibility,
    RequiredFeature,
    UserValue,
    Measurement,
    Performance,
    Refactor,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    Low,
    Medium,
    High,
    Critical,
}

/// `discovered → validated → registered → ready → running → stopped`, plus terminal states.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentLifecycle {
    Discovered,
    Validated,
    Registered,
    Ready,
    Running,
    Stopped,
    Failed,
    Quarantined,
    Deprecated,
    Removed,
}

impl AgentLifecycle {
    pub fn is_active(self) -> bool {
        matches!(self, AgentLifecycle::Ready | AgentLifecycle::Running)
    }

    /// Registration and everything after it is a claim that the agent was tested.
    pub fn claims_testing(self) -> bool {
        matches!(
            self,
            AgentLifecycle::Registered
                | AgentLifecycle::Ready
                | AgentLifecycle::Running
                | AgentLifecycle::Stopped
        )
    }
}

/// Capability-based access control. An agent gets nothing it does not declare.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Permission {
    ReadWorkspace,
    WriteWorkspace,
    SpawnProcess,
    GeometryKernel,
    Network,
    ReadSecrets,
    Shell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthStatus {
    Unknown,
    Healthy,
    Degraded,
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RegistrationStatus {
    Unregistered,
    Staged,
    Active,
    Suspended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TestStatus {
    None,
    Partial,
    Passing,
    Failing,
    Unknown,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum VerificationStatus {
    /// Implemented nowhere.
    Absent,
    /// The implementation exists and the capability is published, but no test exercises it.
    Advertised,
    /// A recorded test exercises it with a passed outcome.
    Verified,
    /// A test exists and fails.
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Stability {
    Draft,
    Stable,
    Deprecated,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InterfaceKind {
    RustTrait,
    RustCrateApi,
    JsonSchema,
    Cli,
    Mcp,
    Ffi,
    Http,
    Subprocess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DependencyKind {
    InternalCrate,
    PathPatch,
    Registry,
    Git,
    System,
    Kernel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Outcome {
    Success,
    Failure,
    /// Recorded so that a skipped check is visible instead of absent.
    NotRun,
}

/// The kinds of change a scan can observe. Each maps to at most one generated task per occurrence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeKind {
    RepositoryDiscovered,
    RepositoryMissing,
    CommitChanged,
    WorkingTreeDirty,
    FilesModified,
    PackageAdded,
    PackageRemoved,
}

impl ChangeKind {
    pub fn slug(self) -> &'static str {
        match self {
            ChangeKind::RepositoryDiscovered => "repo-discovered",
            ChangeKind::RepositoryMissing => "repo-missing",
            ChangeKind::CommitChanged => "commit-changed",
            ChangeKind::WorkingTreeDirty => "working-tree-dirty",
            ChangeKind::FilesModified => "files-modified",
            ChangeKind::PackageAdded => "package-added",
            ChangeKind::PackageRemoved => "package-removed",
        }
    }

    pub fn priority(self) -> TaskPriority {
        match self {
            ChangeKind::RepositoryMissing => TaskPriority::BrokenBuild,
            ChangeKind::WorkingTreeDirty | ChangeKind::CommitChanged => {
                TaskPriority::IntegrationBlocker
            }
            ChangeKind::RepositoryDiscovered => TaskPriority::IntegrationBlocker,
            ChangeKind::FilesModified | ChangeKind::PackageAdded | ChangeKind::PackageRemoved => {
                TaskPriority::InterfaceCompatibility
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// Workspace policy (`engineering/manifest/workspace.json`)
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct WorkspacePolicy {
    pub schema_version: u32,
    /// Directories, relative to the workspace root, that may contain repositories.
    pub scan_roots: Vec<String>,
    pub ignore: IgnorePolicy,
    pub license_policy: LicensePolicy,
    pub notes: Vec<String>,
}

/// Fingerprint policy. Documented here so a fingerprint can be recomputed by hand.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IgnorePolicy {
    /// Directory names skipped at any depth.
    pub directories: Vec<String>,
    /// File-name suffixes skipped (e.g. `.zip`, `.png` are *not* skipped by default).
    pub suffixes: Vec<String>,
    /// Files larger than this are hashed by size + mtime instead of content.
    pub max_hash_bytes: u64,
    /// Symlinks are never followed: a scan must not escape the workspace.
    pub follow_symlinks: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicensePolicy {
    /// SPDX identifiers that are acceptable in a shipping binary.
    pub allowed: Vec<String>,
    /// Substrings that mark a licence as unsuitable for a closed distribution.
    pub denied_substrings: Vec<String>,
    pub notes: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Repositories
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryManifest {
    pub schema_version: u32,
    pub repositories: Vec<RepositoryEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RepositoryEntry {
    pub id: String,
    pub name: String,
    /// Workspace-relative path. Empty for `external` entries.
    pub path: String,
    pub kind: RepoKind,
    pub origin: Option<String>,
    pub branch: Option<String>,
    pub version: Option<String>,
    pub license: LicenseEntry,
    pub languages: Vec<LanguageShare>,
    pub role: String,
    pub capabilities: Vec<String>,
    pub interfaces: Vec<String>,
    pub dependencies: Vec<String>,
    pub build_commands: Vec<String>,
    pub test_commands: Vec<String>,
    pub integration_status: ComponentStatus,
    pub conversion: ConversionDecision,
    pub conversion_rationale: String,
    pub known_limitations: Vec<String>,
    /// The commit the last recorded build/test evidence applies to.
    pub last_verified_commit: Option<String>,
    pub tasks: Vec<String>,
    /// Documents that support the claims above, e.g. `docs/repository-audit.md §3`.
    pub evidence: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LicenseEntry {
    pub spdx: String,
    pub status: LicenseStatus,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LanguageShare {
    pub language: String,
    pub files: u32,
}

// ---------------------------------------------------------------------------------------------
// Modules
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleManifest {
    pub schema_version: u32,
    pub modules: Vec<ModuleEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ModuleEntry {
    pub id: String,
    pub name: String,
    pub repository_id: String,
    pub language: String,
    pub responsibility: String,
    pub boundaries: Vec<String>,
    pub public_api: Vec<String>,
    pub internal_dependencies: Vec<String>,
    pub external_dependencies: Vec<String>,
    pub consumes: Vec<String>,
    pub provides: Vec<String>,
    pub tests: TestStatus,
    pub loc: Option<u32>,
    pub files: Option<u32>,
    pub performance_baseline: Option<String>,
    pub migration: ConversionDecision,
    pub integration_status: ComponentStatus,
    pub consumers: Vec<String>,
    pub compat: Option<String>,
    pub evidence: Vec<String>,
}

// ---------------------------------------------------------------------------------------------
// Agents
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentManifest {
    pub schema_version: u32,
    pub agents: Vec<AgentEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AgentEntry {
    pub id: String,
    pub name: String,
    pub version: String,
    pub purpose: String,
    /// `none` until an agent runtime exists. A value here is a claim, so keep it exact.
    pub runtime: String,
    pub language: String,
    /// Whether source code implementing this agent exists in the workspace today.
    pub implemented: bool,
    pub lifecycle: AgentLifecycle,
    pub input_schema: String,
    pub output_schema: String,
    pub permissions: Vec<Permission>,
    pub resources: String,
    pub dependencies: Vec<String>,
    pub interfaces: Vec<String>,
    pub health: HealthStatus,
    pub tests: TestStatus,
    pub registration: RegistrationStatus,
    pub compat: String,
    pub owner: String,
    pub last_successful_run: Option<String>,
    pub limitations: Vec<String>,
    pub disable_procedure: String,
}

// ---------------------------------------------------------------------------------------------
// Interfaces and capabilities
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceManifest {
    pub schema_version: u32,
    pub interfaces: Vec<InterfaceEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InterfaceEntry {
    pub id: String,
    pub name: String,
    pub kind: InterfaceKind,
    pub provider_module: String,
    pub version: String,
    pub stability: Stability,
    pub schema: Option<String>,
    pub consumers: Vec<String>,
    pub contract_tests: Vec<String>,
    pub compatibility_policy: String,
    pub status: ComponentStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityManifest {
    pub schema_version: u32,
    pub capabilities: Vec<CapabilityEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CapabilityEntry {
    pub id: String,
    pub name: String,
    pub description: String,
    pub provided_by: Vec<String>,
    pub verification: VerificationStatus,
    pub evidence: Vec<String>,
    pub notes: String,
}

// ---------------------------------------------------------------------------------------------
// Dependencies
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyManifest {
    pub schema_version: u32,
    pub dependencies: Vec<DependencyEdge>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DependencyEdge {
    /// Module id, or `workspace` for root-level dependencies.
    pub from: String,
    /// Module id, `repo.<id>`, or `ext.<crate>` for something outside the workspace.
    pub to: String,
    pub kind: DependencyKind,
    pub version: Option<String>,
    pub license: Option<String>,
    pub risk: Risk,
    pub notes: String,
}

// ---------------------------------------------------------------------------------------------
// Tasks
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TaskStore {
    pub schema_version: u32,
    pub tasks: Vec<Task>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TaskSource {
    /// Written by a human or an engineering session.
    Curated,
    /// Produced by `atlas-eng scan` from an observed change.
    Scanner,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Task {
    pub id: String,
    pub title: String,
    pub rationale: String,
    pub priority: TaskPriority,
    pub risk: Risk,
    pub status: TaskStatus,
    pub acceptance_criteria: Vec<String>,
    pub inputs: Vec<String>,
    pub outputs: Vec<String>,
    pub depends_on: Vec<String>,
    pub blocks: Vec<String>,
    pub expected_changes: Vec<String>,
    pub required_tests: Vec<String>,
    pub attempts: u32,
    pub last_error: Option<String>,
    pub evidence: Vec<String>,
    pub rollback: String,
    pub source: TaskSource,
    /// For scanner tasks: the observed value that triggered the task. Re-running an unchanged scan
    /// reproduces this digest, which is what makes task generation idempotent.
    pub trigger_digest: Option<String>,
    /// Closure notes, in the order of `acceptance_criteria`. A criterion may be recorded as met,
    /// partially met or deviated from; "partially met" is a real outcome and is kept rather than
    /// rounded up to a pass.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub acceptance_results: Vec<String>,
    /// Contract, interface or schema changes the task made, so a consumer can see what moved.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub contract_changes: Vec<String>,
    /// What the task does not do. Recorded next to the evidence so a limit is not read as an omission.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub limitations: Vec<String>,
    /// When the recorded evidence was last reproduced, and on what.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_on: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verified_host: Option<String>,
}

// ---------------------------------------------------------------------------------------------
// Quality evidence
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildManifest {
    pub schema_version: u32,
    pub records: Vec<BuildRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildRecord {
    pub id: String,
    pub target: String,
    pub command: String,
    pub outcome: Outcome,
    pub duration_ms: Option<u64>,
    pub peak_rss_kb: Option<u64>,
    pub recorded_at: String,
    pub host: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestManifest {
    pub schema_version: u32,
    pub records: Vec<TestRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct TestRecord {
    pub id: String,
    pub target: String,
    pub command: String,
    pub outcome: Outcome,
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
    pub duration_ms: Option<u64>,
    pub recorded_at: String,
    pub notes: String,
    /// A workspace-wide summary of runs recorded separately. Summaries are reported but never
    /// added into the per-package totals, or the same test would be counted twice.
    #[serde(default)]
    pub summary: bool,
    /// Records this run replaces without deleting their history. A superseded record stays in the
    /// manifest as evidence of what was true on its own date but is excluded from current totals.
    #[serde(default)]
    pub supersedes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkManifest {
    pub schema_version: u32,
    pub records: Vec<BenchmarkRecord>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BenchmarkRecord {
    pub id: String,
    pub name: String,
    pub metric: String,
    pub value: f64,
    pub unit: String,
    pub host: String,
    pub recorded_at: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityManifest {
    pub schema_version: u32,
    pub scans: Vec<SecurityScan>,
    pub findings: Vec<SecurityFinding>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityScan {
    pub tool: String,
    pub outcome: Outcome,
    pub recorded_at: String,
    pub notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SecurityFinding {
    pub id: String,
    pub title: String,
    pub severity: Risk,
    pub status: TaskStatus,
    pub evidence: Vec<String>,
    pub action: String,
}

// ---------------------------------------------------------------------------------------------
// Observed state (`engineering/state/scan.json`) — written by the scanner, never hand-edited
// ---------------------------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ScanState {
    pub schema_version: u32,
    pub observed_at_unix: u64,
    pub host: HostInfo,
    pub repos: Vec<RepoObservation>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HostInfo {
    pub os: String,
    pub arch: String,
    pub cpus: usize,
    pub scanner_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RepoObservation {
    /// Manifest id when the repository is registered, otherwise a derived stable id.
    pub id: String,
    pub path: String,
    pub declared: bool,
    /// Whether the path was present during this scan.
    pub exists: bool,
    pub git: Option<GitState>,
    pub fingerprint: Option<Fingerprint>,
    /// Relative path → content digest. Used for changed-file detection between scans.
    pub files: BTreeMap<String, String>,
    pub packages: Vec<String>,
    pub notes: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GitState {
    pub commit: Option<String>,
    pub branch: Option<String>,
    pub origin: Option<String>,
    /// Number of paths reported by `git status --porcelain`.
    pub dirty_paths: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Fingerprint {
    pub algorithm: String,
    pub digest: String,
    pub files: u64,
    pub bytes: u64,
    pub content_hashed: u64,
    pub metadata_only: u64,
}

impl ScanState {
    pub fn repo(&self, id: &str) -> Option<&RepoObservation> {
        self.repos.iter().find(|r| r.id == id)
    }
}

/// Persisted result of the last comparison, so a report can be rendered without rescanning.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ChangeSet {
    pub schema_version: u32,
    pub observed_at_unix: u64,
    pub compared_with_previous: bool,
    pub changes: Vec<Change>,
}

/// A change observed between two scans, or between the manifest declaration and reality.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Change {
    pub repo_id: String,
    pub kind: ChangeKind,
    pub summary: String,
    pub details: Vec<String>,
    /// Digest of the new observed value. Makes generated task ids stable.
    pub digest: String,
}

impl Change {
    /// Task id derived from the observed *value*, not from the time of the scan. Repeating an
    /// unchanged scan therefore reproduces the same id (no duplicate task), while a genuinely new
    /// change — a second new commit, a second batch of edits — gets its own task.
    pub fn task_id(&self) -> String {
        let short: String = self.digest.chars().take(8).collect();
        format!(
            "task.scan.{}.{}.{}",
            self.kind.slug(),
            slug(&self.repo_id),
            short
        )
    }
}

/// Lowercase ASCII slug used in generated identifiers. Stable across runs.
pub fn slug(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
        } else if !out.ends_with('-') {
            out.push('-');
        }
    }
    out.trim_matches('-').to_string()
}

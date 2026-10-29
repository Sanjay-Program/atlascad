//! Workspace-relative paths owned by the control plane. One place, so no module invents its own.

/// Manifest directory.
pub const MANIFEST_DIR: &str = "engineering/manifest";
/// Observed state written by the scanner.
pub const STATE_DIR: &str = "engineering/state";
pub const SCAN_STATE: &str = "engineering/state/scan.json";
/// Changes detected by the most recent scan, so `report` never has to rescan.
pub const LAST_CHANGES: &str = "engineering/state/last-changes.json";
/// Append-only digest history, one JSON object per line.
pub const SCAN_HISTORY: &str = "engineering/state/scan-history.jsonl";
pub const REPORTS_DIR: &str = "engineering/reports";
pub const LATEST_REPORT: &str = "engineering/reports/latest-integration-report.md";

pub const WORKSPACE_POLICY: &str = "engineering/manifest/workspace.json";
pub const REPOSITORIES: &str = "engineering/manifest/repositories.json";
pub const MODULES: &str = "engineering/manifest/modules.json";
pub const AGENTS: &str = "engineering/manifest/agents.json";
pub const INTERFACES: &str = "engineering/manifest/interfaces.json";
pub const CAPABILITIES: &str = "engineering/manifest/capabilities.json";
pub const DEPENDENCIES: &str = "engineering/manifest/dependencies.json";

pub const TASK_BACKLOG: &str = "engineering/tasks/backlog.json";
pub const TASK_ACTIVE: &str = "engineering/tasks/active.json";
pub const TASK_COMPLETED: &str = "engineering/tasks/completed.json";

pub const BUILD_RECORDS: &str = "engineering/quality/builds.json";
pub const TEST_RECORDS: &str = "engineering/quality/tests.json";
pub const BENCHMARKS: &str = "engineering/quality/benchmarks.json";
pub const SECURITY: &str = "engineering/quality/security.json";

/// The licence policy the release gate checks every dependency against.
pub const LICENCES: &str = "engineering/policy/licences.json";
/// Generated CycloneDX SBOMs.
pub const SBOM_DIR: &str = "engineering/sbom";
/// The SBOM file name for this version.
pub const SBOM: &str = "engineering/sbom/atlascad.cdx.json";

/// Every manifest the validator must be able to load.
pub const ALL_FILES: &[&str] = &[
    WORKSPACE_POLICY,
    REPOSITORIES,
    MODULES,
    AGENTS,
    INTERFACES,
    CAPABILITIES,
    DEPENDENCIES,
    TASK_BACKLOG,
    TASK_ACTIVE,
    TASK_COMPLETED,
    BUILD_RECORDS,
    TEST_RECORDS,
    BENCHMARKS,
    SECURITY,
];

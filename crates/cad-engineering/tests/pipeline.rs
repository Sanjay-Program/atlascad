//! End-to-end tests for the engineering control plane.
//!
//! These run against real temporary directories, not mocked data, because the properties under
//! test are about the filesystem: a modified file must be detected, a scan must be repeatable, and
//! an ignored directory must not contribute to a fingerprint.
//!
//! Test bodies return `Result` and never `unwrap`, so a failure is a failure rather than a skipped
//! assertion.

use cad_engineering::SCANNER_VERSION;
use cad_engineering::diff;
use cad_engineering::fingerprint::{self, ALGORITHM};
use cad_engineering::manifest::ManifestSet;
use cad_engineering::model::*;
use cad_engineering::paths;
use cad_engineering::report::{self, ReportInput};
use cad_engineering::scan::Scanner;
use cad_engineering::tasks;
use serde_json::json;
use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicU64, Ordering};

type TestResult = Result<(), Box<dyn std::error::Error>>;

static COUNTER: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(tag: &str) -> std::io::Result<Self> {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        let mut path = std::env::temp_dir();
        path.push(format!(
            "cad-engineering-{tag}-{}-{unique}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(TempDir(path))
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn must<T>(value: Option<T>, what: &str) -> Result<T, Box<dyn std::error::Error>> {
    value.ok_or_else(|| format!("expected {what}").into())
}

fn write(path: &Path, contents: &str) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, contents)
}

fn policy() -> WorkspacePolicy {
    WorkspacePolicy {
        schema_version: SCHEMA_VERSION,
        scan_roots: vec!["apps".to_string()],
        ignore: IgnorePolicy {
            directories: vec![
                ".git".to_string(),
                "target".to_string(),
                "node_modules".to_string(),
            ],
            suffixes: vec![".zip".to_string()],
            max_hash_bytes: 8 * 1024 * 1024,
            follow_symlinks: false,
        },
        license_policy: LicensePolicy {
            allowed: vec!["MIT OR Apache-2.0".to_string()],
            denied_substrings: vec!["GPL".to_string()],
            notes: Vec::new(),
        },
        notes: Vec::new(),
    }
}

fn demo_repository(path: &str) -> RepositoryEntry {
    RepositoryEntry {
        id: "repo.demo".to_string(),
        name: "Demo".to_string(),
        path: path.to_string(),
        kind: RepoKind::Application,
        origin: None,
        branch: None,
        version: Some("0.1.0".to_string()),
        license: LicenseEntry {
            spdx: "MIT OR Apache-2.0".to_string(),
            status: LicenseStatus::Approved,
            notes: String::new(),
        },
        languages: vec![LanguageShare {
            language: "rust".to_string(),
            files: 1,
        }],
        role: "test fixture".to_string(),
        capabilities: vec!["cap.demo".to_string()],
        interfaces: vec!["iface.demo".to_string()],
        dependencies: Vec::new(),
        build_commands: vec!["cargo build".to_string()],
        test_commands: vec!["cargo test".to_string()],
        integration_status: ComponentStatus::Discovered,
        conversion: ConversionDecision::Keep,
        conversion_rationale: "fixture".to_string(),
        known_limitations: Vec::new(),
        last_verified_commit: None,
        tasks: Vec::new(),
        evidence: Vec::new(),
    }
}

fn demo_module() -> ModuleEntry {
    ModuleEntry {
        id: "module.demo".to_string(),
        name: "demo".to_string(),
        repository_id: "repo.demo".to_string(),
        language: "rust".to_string(),
        responsibility: "fixture module".to_string(),
        boundaries: Vec::new(),
        public_api: vec!["Demo".to_string()],
        internal_dependencies: Vec::new(),
        external_dependencies: Vec::new(),
        consumes: Vec::new(),
        provides: vec!["cap.demo".to_string()],
        tests: TestStatus::Passing,
        loc: None,
        files: None,
        performance_baseline: None,
        migration: ConversionDecision::NotApplicable,
        integration_status: ComponentStatus::Discovered,
        consumers: Vec::new(),
        compat: None,
        evidence: vec!["fixture test".to_string()],
    }
}

fn sample_task() -> Task {
    Task {
        id: "task.sample".to_string(),
        title: "Sample".to_string(),
        rationale: "fixture".to_string(),
        priority: TaskPriority::IntegrationBlocker,
        risk: Risk::Medium,
        status: TaskStatus::Ready,
        acceptance_criteria: vec!["a criterion".to_string()],
        inputs: Vec::new(),
        outputs: Vec::new(),
        depends_on: Vec::new(),
        blocks: Vec::new(),
        expected_changes: Vec::new(),
        required_tests: Vec::new(),
        attempts: 0,
        last_error: None,
        evidence: Vec::new(),
        rollback: "revert".to_string(),
        source: TaskSource::Curated,
        trigger_digest: None,
        acceptance_results: Vec::new(),
        contract_changes: Vec::new(),
        limitations: Vec::new(),
        verified_on: None,
        verified_host: None,
    }
}

/// Write a complete, consistent manifest set into `root` so validation can be exercised.
fn write_manifests(root: &Path) -> TestResult {
    write(
        &root.join(paths::WORKSPACE_POLICY),
        &serde_json::to_string_pretty(&serde_json::to_value(policy())?)?,
    )?;

    let repository = serde_json::to_value(demo_repository("apps/demo"))?;
    let module = serde_json::to_value(demo_module())?;
    let interface = json!({
        "id": "iface.demo",
        "name": "Demo interface",
        "kind": "rust_trait",
        "provider_module": "module.demo",
        "version": "1",
        "stability": "stable",
        "schema": null,
        "consumers": ["module.demo"],
        "contract_tests": ["demo_contract"],
        "compatibility_policy": "additive only",
        "status": "discovered"
    });
    let capability = json!({
        "id": "cap.demo",
        "name": "Demo capability",
        "description": "fixture",
        "provided_by": ["module.demo"],
        "verification": "advertised",
        "evidence": [],
        "notes": "fixture"
    });

    write_json(
        root,
        paths::REPOSITORIES,
        &json!({"schema_version": SCHEMA_VERSION, "repositories": [repository]}),
    )?;
    write_json(
        root,
        paths::MODULES,
        &json!({"schema_version": SCHEMA_VERSION, "modules": [module]}),
    )?;
    write_json(
        root,
        paths::AGENTS,
        &json!({"schema_version": SCHEMA_VERSION, "agents": []}),
    )?;
    write_json(
        root,
        paths::INTERFACES,
        &json!({"schema_version": SCHEMA_VERSION, "interfaces": [interface]}),
    )?;
    write_json(
        root,
        paths::CAPABILITIES,
        &json!({"schema_version": SCHEMA_VERSION, "capabilities": [capability]}),
    )?;
    write_json(
        root,
        paths::DEPENDENCIES,
        &json!({"schema_version": SCHEMA_VERSION, "dependencies": []}),
    )?;
    write_json(
        root,
        paths::TASK_BACKLOG,
        &json!({"schema_version": SCHEMA_VERSION, "tasks": []}),
    )?;
    write_json(
        root,
        paths::TASK_ACTIVE,
        &json!({"schema_version": SCHEMA_VERSION, "tasks": []}),
    )?;
    write_json(
        root,
        paths::TASK_COMPLETED,
        &json!({"schema_version": SCHEMA_VERSION, "tasks": []}),
    )?;
    write_json(
        root,
        paths::BUILD_RECORDS,
        &json!({"schema_version": SCHEMA_VERSION, "records": []}),
    )?;
    write_json(
        root,
        paths::TEST_RECORDS,
        &json!({"schema_version": SCHEMA_VERSION, "records": []}),
    )?;
    write_json(
        root,
        paths::BENCHMARKS,
        &json!({"schema_version": SCHEMA_VERSION, "records": []}),
    )?;
    write_json(
        root,
        paths::SECURITY,
        &json!({"schema_version": SCHEMA_VERSION, "scans": [], "findings": []}),
    )?;
    Ok(())
}

fn write_json(root: &Path, relative: &str, value: &serde_json::Value) -> TestResult {
    write(
        &root.join(relative),
        &format!("{}\n", serde_json::to_string_pretty(value)?),
    )?;
    Ok(())
}

fn read_json(root: &Path, relative: &str) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    let text = std::fs::read_to_string(root.join(relative))?;
    Ok(serde_json::from_str(&text)?)
}

// ---------------------------------------------------------------------------------------------
// 1. Manifests
// ---------------------------------------------------------------------------------------------

#[test]
fn a_consistent_manifest_set_validates() -> TestResult {
    let dir = TempDir::new("valid")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    let manifests = ManifestSet::load(dir.path())?;
    let diagnostics = manifests.validate(dir.path());
    assert!(
        diagnostics.is_ok(),
        "expected a clean validation, got {:?}",
        diagnostics.errors
    );
    Ok(())
}

#[test]
fn duplicate_repository_ids_are_rejected() -> TestResult {
    let dir = TempDir::new("dup-repo")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    let mut repositories = read_json(dir.path(), paths::REPOSITORIES)?;
    let mut second = repositories["repositories"][0].clone();
    second["path"] = json!("apps/demo");
    repositories["repositories"]
        .as_array_mut()
        .ok_or("repositories is not an array")?
        .push(second);
    write_json(dir.path(), paths::REPOSITORIES, &repositories)?;

    let manifests = ManifestSet::load(dir.path())?;
    let diagnostics = manifests.validate(dir.path());
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|error| error.contains("duplicate id")),
        "expected a duplicate-id error, got {:?}",
        diagnostics.errors
    );
    Ok(())
}

#[test]
fn a_repository_pointing_at_an_unknown_capability_is_rejected() -> TestResult {
    let dir = TempDir::new("unknown-cap")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    let mut repositories = read_json(dir.path(), paths::REPOSITORIES)?;
    repositories["repositories"][0]["capabilities"] = json!(["cap.does-not-exist"]);
    write_json(dir.path(), paths::REPOSITORIES, &repositories)?;

    let manifests = ManifestSet::load(dir.path())?;
    let diagnostics = manifests.validate(dir.path());
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|error| error.contains("unknown capability")),
        "expected an unknown-capability error, got {:?}",
        diagnostics.errors
    );
    Ok(())
}

#[test]
fn copyleft_cannot_be_marked_approved() -> TestResult {
    let dir = TempDir::new("licence")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    let mut repositories = read_json(dir.path(), paths::REPOSITORIES)?;
    repositories["repositories"][0]["license"]["spdx"] = json!("GPL-3.0-only");
    repositories["repositories"][0]["license"]["status"] = json!("approved");
    write_json(dir.path(), paths::REPOSITORIES, &repositories)?;

    let manifests = ManifestSet::load(dir.path())?;
    let diagnostics = manifests.validate(dir.path());
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|error| error.contains("deny list")),
        "expected a licence policy error, got {:?}",
        diagnostics.errors
    );
    Ok(())
}

#[test]
fn an_agent_cannot_claim_registration_without_passing_tests() -> TestResult {
    let dir = TempDir::new("agent")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    write_json(
        dir.path(),
        paths::AGENTS,
        &json!({
            "schema_version": SCHEMA_VERSION,
            "agents": [{
                "id": "agent.fake",
                "name": "Fake",
                "version": "0.1.0",
                "purpose": "prove the guard works",
                "runtime": "none",
                "language": "rust",
                "implemented": false,
                "lifecycle": "ready",
                "input_schema": "none",
                "output_schema": "none",
                "permissions": [],
                "resources": "none",
                "dependencies": [],
                "interfaces": [],
                "health": "healthy",
                "tests": "none",
                "registration": "active",
                "compat": "none",
                "owner": "module.demo",
                "last_successful_run": null,
                "limitations": [],
                "disable_procedure": "none"
            }]
        }),
    )?;

    let manifests = ManifestSet::load(dir.path())?;
    let diagnostics = manifests.validate(dir.path());
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|error| error.contains("implemented = false")),
        "expected the unimplemented-agent guard, got {:?}",
        diagnostics.errors
    );
    Ok(())
}

#[test]
fn a_verified_task_without_evidence_is_rejected() -> TestResult {
    let dir = TempDir::new("verified-task")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    let mut task = serde_json::to_value(sample_task())?;
    task["status"] = json!("verified");
    write_json(
        dir.path(),
        paths::TASK_BACKLOG,
        &json!({"schema_version": SCHEMA_VERSION, "tasks": [task]}),
    )?;

    let manifests = ManifestSet::load(dir.path())?;
    let diagnostics = manifests.validate(dir.path());
    assert!(
        diagnostics
            .errors
            .iter()
            .any(|error| error.contains("verified with no evidence")),
        "expected a verified-without-evidence error, got {:?}",
        diagnostics.errors
    );
    Ok(())
}

#[test]
fn schema_drift_is_a_load_error() -> TestResult {
    let dir = TempDir::new("schema")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;

    let mut repositories = read_json(dir.path(), paths::REPOSITORIES)?;
    repositories["schema_version"] = json!(99);
    write_json(dir.path(), paths::REPOSITORIES, &repositories)?;

    match ManifestSet::load(dir.path()) {
        Err(cad_engineering::Error::Validation { errors }) => {
            assert!(
                errors.iter().any(|error| error.contains("schema_version")),
                "expected a schema_version error, got {errors:?}"
            );
            Ok(())
        }
        Err(other) => Err(format!("unexpected error: {other}").into()),
        Ok(_) => Err("a future schema version must not load silently".into()),
    }
}

// ---------------------------------------------------------------------------------------------
// 2. Fingerprints
// ---------------------------------------------------------------------------------------------

#[test]
fn fingerprint_ignores_build_output_git_and_zip_files() -> TestResult {
    let dir = TempDir::new("fingerprint")?;
    write(&dir.path().join("src/main.rs"), "fn main() {}\n")?;
    write(&dir.path().join("src/lib.rs"), "pub fn one() {}\n")?;
    write(&dir.path().join("target/debug/binary"), "not source")?;
    write(&dir.path().join(".git/config"), "[core]")?;
    write(&dir.path().join("node_modules/pkg/index.js"), "x")?;
    write(&dir.path().join("archive.zip"), "zip")?;

    let files = fingerprint::collect_files(dir.path(), &policy().ignore)?;
    let paths: Vec<&str> = files.iter().map(|file| file.relative.as_str()).collect();
    assert_eq!(paths, vec!["src/lib.rs", "src/main.rs"]);
    Ok(())
}

#[test]
fn fingerprint_changes_when_content_changes_and_not_before() -> TestResult {
    let dir = TempDir::new("fingerprint-change")?;
    write(&dir.path().join("src/main.rs"), "one\n")?;

    let first =
        fingerprint::fingerprint_of(&fingerprint::collect_files(dir.path(), &policy().ignore)?);
    let second =
        fingerprint::fingerprint_of(&fingerprint::collect_files(dir.path(), &policy().ignore)?);
    assert_eq!(
        first, second,
        "an unchanged tree must fingerprint identically"
    );

    write(&dir.path().join("src/main.rs"), "two\n")?;
    let third =
        fingerprint::fingerprint_of(&fingerprint::collect_files(dir.path(), &policy().ignore)?);
    assert_ne!(first.digest, third.digest);
    assert_eq!(third.files, 1);
    assert_eq!(third.content_hashed, 1);
    assert_eq!(third.metadata_only, 0);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 3. Scanning and change detection
// ---------------------------------------------------------------------------------------------

#[test]
fn the_first_scan_is_a_baseline_and_reports_no_changes() -> TestResult {
    let dir = TempDir::new("baseline")?;
    write(
        &dir.path().join("apps/demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\n",
    )?;
    write(&dir.path().join("apps/demo/src/lib.rs"), "pub fn a() {}\n")?;

    let scanner = Scanner::new(dir.path(), policy());
    let state = scanner.scan(&[])?;
    assert_eq!(state.repos.len(), 1);
    assert!(!state.repos[0].declared);
    assert!(state.repos[0].id.starts_with("repo.unregistered."));
    assert!(diff::compare(None, &state).is_empty());
    Ok(())
}

#[test]
fn a_modified_file_is_reported_with_its_path() -> TestResult {
    let dir = TempDir::new("modified")?;
    write(&dir.path().join("apps/demo/src/lib.rs"), "pub fn a() {}\n")?;
    write(
        &dir.path().join("apps/demo/src/other.rs"),
        "pub fn b() {}\n",
    )?;
    let repositories = vec![demo_repository("apps/demo")];

    let scanner = Scanner::new(dir.path(), policy());
    let first = scanner.scan(&repositories)?;
    write(
        &dir.path().join("apps/demo/src/lib.rs"),
        "pub fn a() -> u8 { 1 }\n",
    )?;
    let second = scanner.scan(&repositories)?;

    let changes = diff::compare(Some(&first), &second);
    let modified = must(
        changes
            .iter()
            .find(|change| change.kind == ChangeKind::FilesModified),
        "a FilesModified change",
    )?;
    assert_eq!(modified.repo_id, "repo.demo");
    assert!(
        modified
            .details
            .iter()
            .any(|detail| detail == "~ src/lib.rs"),
        "expected the changed path to be listed, got {:?}",
        modified.details
    );
    assert!(
        !modified
            .details
            .iter()
            .any(|detail| detail.contains("other.rs")),
        "an unchanged file must not be listed"
    );
    Ok(())
}

#[test]
fn changed_files_are_reported_as_added_and_removed() -> TestResult {
    let dir = TempDir::new("added-removed")?;
    write(&dir.path().join("apps/demo/a.rs"), "a\n")?;
    write(&dir.path().join("apps/demo/b.rs"), "b\n")?;
    let repositories = vec![demo_repository("apps/demo")];

    let scanner = Scanner::new(dir.path(), policy());
    let first = scanner.scan(&repositories)?;
    std::fs::remove_file(dir.path().join("apps/demo/b.rs"))?;
    write(&dir.path().join("apps/demo/c.rs"), "c\n")?;
    let second = scanner.scan(&repositories)?;

    let changes = diff::compare(Some(&first), &second);
    let modified = must(
        changes
            .iter()
            .find(|change| change.kind == ChangeKind::FilesModified),
        "a FilesModified change",
    )?;
    assert!(modified.details.iter().any(|detail| detail == "+ c.rs"));
    assert!(modified.details.iter().any(|detail| detail == "- b.rs"));
    Ok(())
}

#[test]
fn a_declared_repository_that_is_missing_is_reported() -> TestResult {
    let dir = TempDir::new("missing")?;
    let mut repository = demo_repository("apps/gone");
    repository.last_verified_commit = Some("a".repeat(40));
    let scanner = Scanner::new(dir.path(), policy());
    let state = scanner.scan(&[repository.clone()])?;

    let changes = diff::compare_declared(&[repository], &state);
    assert!(
        changes
            .iter()
            .any(|change| change.kind == ChangeKind::RepositoryMissing),
        "expected a missing-repository change, got {changes:?}"
    );
    Ok(())
}

#[test]
fn a_head_past_the_verified_commit_is_reported() -> TestResult {
    let dir = TempDir::new("head-drift")?;
    let repo_dir = dir.path().join("apps/demo");
    write(&repo_dir.join("src/lib.rs"), "pub fn a() {}\n")?;
    if !git(&repo_dir, &["init", "--quiet"])? {
        return Ok(()); // git unavailable in this environment: the other tests still apply.
    }
    git(&repo_dir, &["add", "."])?;
    if !git(
        &repo_dir,
        &[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.com",
            "commit",
            "--quiet",
            "-m",
            "fixture",
        ],
    )? {
        return Ok(());
    }

    let mut repository = demo_repository("apps/demo");
    repository.last_verified_commit = Some("0".repeat(40));
    let scanner = Scanner::new(dir.path(), policy());
    let state = scanner.scan(&[repository.clone()])?;

    let observation = must(state.repo("repo.demo"), "the declared observation")?;
    let git_state = must(observation.git.as_ref(), "git state")?;
    assert!(git_state.commit.is_some());
    assert_eq!(git_state.dirty_paths, 0);

    let changes = diff::compare_declared(&[repository], &state);
    assert!(
        changes
            .iter()
            .any(|change| change.kind == ChangeKind::CommitChanged),
        "expected commit drift, got {changes:?}"
    );

    // An untracked file must show up as a dirty working tree on the next scan.
    write(&repo_dir.join("src/untracked.rs"), "pub fn b() {}\n")?;
    let dirty = scanner.scan(&[demo_repository("apps/demo")])?;
    let dirty_changes = diff::compare(Some(&state), &dirty);
    let dirty_state = must(
        dirty_changes
            .iter()
            .find(|change| change.kind == ChangeKind::WorkingTreeDirty),
        "a WorkingTreeDirty change",
    )?;
    assert!(dirty_state.summary.contains("uncommitted"));
    Ok(())
}

#[test]
fn an_unregistered_checkout_is_discovered_without_being_declared() -> TestResult {
    let dir = TempDir::new("discovered")?;
    write(
        &dir.path().join("apps/newcomer/Cargo.toml"),
        "[package]\nname = \"newcomer\"\nversion = \"0.0.1\"\nedition = \"2021\"\n",
    )?;
    write(
        &dir.path().join("apps/newcomer/src/lib.rs"),
        "pub fn hello() {}\n",
    )?;
    let scanner = Scanner::new(dir.path(), policy());
    let state = scanner.scan(&[])?;
    let observation = must(state.repos.first(), "an observation")?;
    assert!(!observation.declared);
    assert!(!observation.packages.is_empty());
    Ok(())
}

#[test]
fn a_pinned_toolchain_does_not_break_package_discovery() -> TestResult {
    // Regression: three real checkouts pin Rust 1.99.0. Running cargo with the repository as the
    // working directory made rustup attempt a toolchain download and time out after 20 s, which
    // silently emptied the package inventory. Cargo must run from the workspace root.
    let dir = TempDir::new("toolchain")?;
    write(
        &dir.path().join("apps/demo/Cargo.toml"),
        "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )?;
    write(&dir.path().join("apps/demo/src/lib.rs"), "pub fn a() {}\n")?;
    write(
        &dir.path().join("apps/demo/rust-toolchain.toml"),
        "[toolchain]\nchannel = \"9.99.0\"\n",
    )?;

    let scanner = Scanner::new(dir.path(), policy());
    let state = scanner.scan(&[])?;
    let observation = must(state.repos.first(), "an observation")?;
    assert!(
        observation
            .notes
            .iter()
            .any(|note| note.contains("pins a toolchain")),
        "the pinned toolchain must be recorded, got {:?}",
        observation.notes
    );
    assert!(
        !observation.packages.is_empty(),
        "package discovery must not depend on the pinned toolchain, notes: {:?}",
        observation.notes
    );
    assert_eq!(observation.packages, vec!["demo@0.1.0"]);
    Ok(())
}

fn git(dir: &Path, args: &[&str]) -> std::io::Result<bool> {
    let status = Command::new("git")
        .arg("-C")
        .arg(dir)
        .args(args)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success())
}

// ---------------------------------------------------------------------------------------------
// 4. Tasks
// ---------------------------------------------------------------------------------------------

fn sample_change(kind: ChangeKind, digest: &str) -> Change {
    Change {
        repo_id: "repo.demo".to_string(),
        kind,
        summary: "fixture change".to_string(),
        details: Vec::new(),
        digest: digest.to_string(),
    }
}

#[test]
fn task_ids_are_stable_for_the_same_observation() -> TestResult {
    let change = sample_change(ChangeKind::FilesModified, "0123456789abcdef");
    let first = tasks::synthesize(std::slice::from_ref(&change));
    let second = tasks::synthesize(std::slice::from_ref(&change));
    assert_eq!(first.len(), 1);
    assert_eq!(second.len(), 1);
    assert_eq!(first[0].id, second[0].id);
    assert_eq!(first[0].trigger_digest, second[0].trigger_digest);
    Ok(())
}

#[test]
fn a_new_observation_yields_a_new_task_id() -> TestResult {
    let a = tasks::synthesize(&[sample_change(ChangeKind::FilesModified, "1111111111111111")]);
    let b = tasks::synthesize(&[sample_change(ChangeKind::FilesModified, "2222222222222222")]);
    assert_ne!(a[0].id, b[0].id, "a different observed value is new work");
    Ok(())
}

#[test]
fn merging_the_same_tasks_twice_inserts_them_once() -> TestResult {
    let change = sample_change(ChangeKind::CommitChanged, "abcdef0123456789");
    let generated = tasks::synthesize(&[change]);
    let mut store = TaskStore {
        schema_version: SCHEMA_VERSION,
        tasks: Vec::new(),
    };
    let known = BTreeSet::new();

    let first = tasks::merge(&mut store, &known, generated.clone());
    assert_eq!(first.inserted, 1);
    assert_eq!(first.duplicates, 0);
    assert_eq!(store.tasks.len(), 1);

    let known: BTreeSet<String> = store.tasks.iter().map(|task| task.id.clone()).collect();
    let second = tasks::merge(&mut store, &known, generated);
    assert_eq!(second.inserted, 0);
    assert_eq!(second.duplicates, 1);
    assert_eq!(store.tasks.len(), 1, "no duplicate task may be created");
    Ok(())
}

#[test]
fn a_scan_that_finds_nothing_new_is_idempotent() -> TestResult {
    let dir = TempDir::new("idempotent")?;
    write(&dir.path().join("apps/demo/src/lib.rs"), "pub fn a() {}\n")?;
    let repositories = vec![demo_repository("apps/demo")];
    let scanner = Scanner::new(dir.path(), policy());

    let first = scanner.scan(&repositories)?;
    let second = scanner.scan(&repositories)?;
    assert!(
        diff::compare(Some(&first), &second).is_empty(),
        "an unchanged workspace must produce no changes"
    );
    // Reproducing the baseline also reproduces identity, not just equality of content.
    let first_fingerprint = must(first.repos[0].fingerprint.as_ref(), "fingerprint")?;
    let second_fingerprint = must(second.repos[0].fingerprint.as_ref(), "fingerprint")?;
    assert_eq!(first_fingerprint.digest, second_fingerprint.digest);
    assert_eq!(first_fingerprint.algorithm, ALGORITHM);
    Ok(())
}

#[test]
fn a_dependency_cycle_is_detected() -> TestResult {
    let mut a = sample_task();
    a.id = "task.a".to_string();
    a.depends_on = vec!["task.b".to_string()];
    let mut b = sample_task();
    b.id = "task.b".to_string();
    b.depends_on = vec!["task.a".to_string()];
    let mut c = sample_task();
    c.id = "task.c".to_string();
    c.depends_on = vec!["task.a".to_string()];

    let all = vec![("backlog", &a), ("backlog", &b), ("backlog", &c)];
    let cycle = must(tasks::find_cycle(&all), "a detected cycle")?;
    assert!(cycle.contains(&"task.a".to_string()));
    assert!(cycle.contains(&"task.b".to_string()));
    assert!(!cycle.contains(&"task.c".to_string()));
    Ok(())
}

#[test]
fn no_cycle_is_reported_for_an_acyclic_graph() -> TestResult {
    let mut a = sample_task();
    a.id = "task.a".to_string();
    let mut b = sample_task();
    b.id = "task.b".to_string();
    b.depends_on = vec!["task.a".to_string()];
    let all = vec![("backlog", &a), ("backlog", &b)];
    assert!(tasks::find_cycle(&all).is_none());
    Ok(())
}

#[test]
fn next_ready_skips_tasks_with_unverified_dependencies() -> TestResult {
    let mut blocked = sample_task();
    blocked.id = "task.blocked".to_string();
    blocked.depends_on = vec!["task.gate".to_string()];
    let mut gate = sample_task();
    gate.id = "task.gate".to_string();
    gate.status = TaskStatus::Proposed;
    let mut free = sample_task();
    free.id = "task.free".to_string();
    free.priority = TaskPriority::Security;

    let all = vec![
        ("backlog", &blocked),
        ("backlog", &gate),
        ("backlog", &free),
    ];
    let ready = tasks::next_ready(&all);
    let ids: Vec<&str> = ready.iter().map(|task| task.id.as_str()).collect();
    assert_eq!(ids, vec!["task.free"]);
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 5. Report
// ---------------------------------------------------------------------------------------------

#[test]
fn the_report_counts_come_from_the_manifests() -> TestResult {
    let dir = TempDir::new("report")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;
    let manifests = ManifestSet::load(dir.path())?;
    let changes = vec![sample_change(ChangeKind::FilesModified, "deadbeefdeadbeef")];
    let rendered = report::render(&ReportInput {
        manifests: &manifests,
        scan: None,
        changes: &changes,
        compared_with_previous: true,
        merge: None,
        generated_at_unix: 1_792_616_135,
    });

    assert!(rendered.contains("| Repositories declared | 1 |"));
    assert!(rendered.contains("| Capabilities — advertised only | 1 |"));
    assert!(rendered.contains("2026-10-21T20:55:35Z"));
    assert!(rendered.contains("FilesModified"));
    assert!(
        rendered.contains("Capability `cap.demo` is advertised"),
        "advertised-but-unverified claims must be listed"
    );
    Ok(())
}

#[test]
fn the_report_states_when_nothing_was_measured() -> TestResult {
    let dir = TempDir::new("report-empty")?;
    std::fs::create_dir_all(dir.path().join("apps/demo"))?;
    write_manifests(dir.path())?;
    let manifests = ManifestSet::load(dir.path())?;
    let rendered = report::render(&ReportInput {
        manifests: &manifests,
        scan: None,
        changes: &[],
        compared_with_previous: false,
        merge: None,
        generated_at_unix: 0,
    });
    assert!(rendered.contains("**Last observation:** none"));
    assert!(rendered.contains("Builds: **0** succeeded"));
    assert!(rendered.contains("Baseline observation"));
    Ok(())
}

// ---------------------------------------------------------------------------------------------
// 6. Scanner metadata
// ---------------------------------------------------------------------------------------------

#[test]
fn the_scanner_version_is_recorded() -> TestResult {
    let dir = TempDir::new("version")?;
    let scanner = Scanner::new(dir.path(), policy());
    let state = scanner.scan(&[])?;
    assert_eq!(state.host.scanner_version, SCANNER_VERSION);
    assert_eq!(state.schema_version, SCHEMA_VERSION);
    Ok(())
}

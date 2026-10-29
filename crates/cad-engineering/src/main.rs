//! `atlas-eng` — the engineering control-plane CLI.
//!
//! ```text
//! atlas-eng validate [--root PATH]   cross-validate every manifest, report errors and warnings
//! atlas-eng scan     [--root PATH] [--write]
//!                                       observe the workspace, diff against the last scan,
//!                                       generate tasks idempotently
//! atlas-eng report   [--root PATH] [--write]
//!                                       render the integration report from recorded state
//! atlas-eng tasks    [--root PATH]   list the next unblocked tasks
//! ```
//!
//! `scan` without `--write` changes nothing on disk (a dry run). `--write` writes only
//! `engineering/state/`, `engineering/reports/` and — only when a genuinely new task exists —
//! `engineering/tasks/backlog.json`. It never touches a source repository.

use cad_engineering::diff;
use cad_engineering::model::*;
use cad_engineering::paths;
use cad_engineering::report::{self, ReportInput};
use cad_engineering::scan::Scanner;
use cad_engineering::supply;
use cad_engineering::tasks;
use cad_engineering::{Error, ManifestSet, Result};
use serde::Serialize;
use serde::de::DeserializeOwned;
use std::collections::BTreeSet;
use std::io::Write as _;
use std::path::{Path, PathBuf};

struct Args {
    command: String,
    root: PathBuf,
    write: bool,
}

fn main() {
    let code = match run() {
        Ok(code) => code,
        Err(error) => {
            eprintln!("atlas-eng: {error}");
            1
        }
    };
    std::process::exit(code);
}

fn run() -> Result<i32> {
    let args = parse_args()?;
    match args.command.as_str() {
        "validate" => cmd_validate(&args.root),
        "scan" => cmd_scan(&args.root, args.write),
        "report" => cmd_report(&args.root, args.write),
        "tasks" => cmd_tasks(&args.root),
        "licences" | "licenses" => cmd_licences(&args.root),
        "sbom" => cmd_sbom(&args.root, args.write),
        "help" | "--help" | "-h" => {
            usage();
            Ok(0)
        }
        other => Err(Error::Message(format!(
            "unknown command `{other}`; run `atlas-eng help`"
        ))),
    }
}

fn parse_args() -> Result<Args> {
    let mut arguments = std::env::args().skip(1);
    let command = arguments.next().unwrap_or_else(|| "help".to_string());
    let mut root = std::env::current_dir().map_err(|source| Error::Io {
        path: ".".to_string(),
        source,
    })?;
    let mut write = false;

    while let Some(argument) = arguments.next() {
        match argument.as_str() {
            "--root" => {
                let value = arguments
                    .next()
                    .ok_or_else(|| Error::Message("--root needs a path argument".to_string()))?;
                root = PathBuf::from(value);
            }
            "--write" => write = true,
            other => {
                return Err(Error::Message(format!(
                    "unknown argument `{other}`; run `atlas-eng help`"
                )));
            }
        }
    }

    Ok(Args {
        command,
        root,
        write,
    })
}

fn usage() {
    println!("atlas-eng {}", env!("CARGO_PKG_VERSION"));
    println!();
    println!("USAGE:");
    println!("  atlas-eng validate [--root PATH]");
    println!("  atlas-eng scan     [--root PATH] [--write]");
    println!("  atlas-eng report   [--root PATH] [--write]");
    println!("  atlas-eng tasks    [--root PATH]");
    println!(
        "  atlas-eng licences [--root PATH]    check every dependency's licence against policy"
    );
    println!("  atlas-eng sbom     [--root PATH] [--write]   emit a CycloneDX SBOM");
    println!();
    println!("`scan` is read-only without --write. Nothing here modifies a source repository.");
}

fn cmd_validate(root: &Path) -> Result<i32> {
    let manifests = ManifestSet::load(root)?;
    let diagnostics = manifests.validate(root);
    for warning in &diagnostics.warnings {
        println!("warning: {warning}");
    }
    for error in &diagnostics.errors {
        println!("error: {error}");
    }
    println!(
        "{} error(s), {} warning(s) across {} manifest file(s)",
        diagnostics.errors.len(),
        diagnostics.warnings.len(),
        paths::ALL_FILES.len()
    );
    Ok(if diagnostics.is_ok() { 0 } else { 1 })
}

fn cmd_scan(root: &Path, write: bool) -> Result<i32> {
    let manifests = ManifestSet::load(root)?;
    let diagnostics = manifests.validate(root);
    if !diagnostics.is_ok() {
        println!(
            "warning: {} manifest error(s); scanning anyway because this command is read-only",
            diagnostics.errors.len()
        );
        for error in diagnostics.errors.iter().take(10) {
            println!("  manifest error: {error}");
        }
    }

    let previous: Option<ScanState> = load_json_optional(root, paths::SCAN_STATE)?;
    let scanner = Scanner::new(root, manifests.policy.clone());
    let state = scanner.scan(&manifests.repositories.repositories)?;

    let mut changes = diff::compare(previous.as_ref(), &state);
    changes.extend(diff::compare_declared(
        &manifests.repositories.repositories,
        &state,
    ));
    let changes = diff::normalize(changes);

    let existing_ids: BTreeSet<String> = manifests
        .tasks
        .all()
        .iter()
        .map(|(_, task)| task.id.clone())
        .collect();
    let mut backlog = manifests.tasks.backlog.clone();
    let merge = tasks::merge(&mut backlog, &existing_ids, tasks::synthesize(&changes));

    let baseline = previous.is_none();
    println!(
        "scanned {} repositories at {}",
        state.repos.len(),
        report::iso8601(state.observed_at_unix)
    );
    println!(
        "changes: {} ({})",
        changes.len(),
        if baseline {
            "baseline observation; no comparison possible"
        } else {
            "compared with the previous scan"
        }
    );
    for change in &changes {
        println!(
            "  [{:?}] {} — {}",
            change.kind, change.repo_id, change.summary
        );
    }
    println!(
        "tasks: {} inserted, {} already known",
        merge.inserted, merge.duplicates
    );

    if !write {
        println!("dry run: nothing written (pass --write to persist state and the report)");
        return Ok(0);
    }

    let mut snapshot = manifests.clone();
    snapshot.tasks.backlog = backlog.clone();

    write_json(root, paths::SCAN_STATE, &state)?;
    write_json(
        root,
        paths::LAST_CHANGES,
        &ChangeSet {
            schema_version: SCHEMA_VERSION,
            observed_at_unix: state.observed_at_unix,
            compared_with_previous: !baseline,
            changes: changes.clone(),
        },
    )?;
    if merge.inserted > 0 {
        write_json(root, paths::TASK_BACKLOG, &backlog)?;
    }
    append_history(root, &state)?;

    let rendered = report::render(&ReportInput {
        manifests: &snapshot,
        scan: Some(&state),
        changes: &changes,
        compared_with_previous: !baseline,
        merge: Some(merge),
        generated_at_unix: state.observed_at_unix,
    });
    write_text(root, paths::LATEST_REPORT, &rendered)?;

    println!(
        "wrote {}, {}, {} and {}",
        paths::SCAN_STATE,
        paths::LAST_CHANGES,
        paths::SCAN_HISTORY,
        paths::LATEST_REPORT
    );
    if merge.inserted == 0 {
        println!("backlog unchanged: no new task was generated");
    }
    Ok(0)
}

fn cmd_report(root: &Path, write: bool) -> Result<i32> {
    let manifests = ManifestSet::load(root)?;
    let scan: Option<ScanState> = load_json_optional(root, paths::SCAN_STATE)?;
    let change_set: Option<ChangeSet> = load_json_optional(root, paths::LAST_CHANGES)?;
    let empty: Vec<Change> = Vec::new();
    let changes = change_set
        .as_ref()
        .map(|set| set.changes.as_slice())
        .unwrap_or(empty.as_slice());
    let rendered = report::render(&ReportInput {
        manifests: &manifests,
        scan: scan.as_ref(),
        changes,
        compared_with_previous: change_set
            .as_ref()
            .map(|set| set.compared_with_previous)
            .unwrap_or(false),
        merge: None,
        generated_at_unix: cad_engineering::scan::now_unix(),
    });
    if write {
        write_text(root, paths::LATEST_REPORT, &rendered)?;
        println!("wrote {}", paths::LATEST_REPORT);
    } else {
        print!("{rendered}");
    }
    Ok(0)
}

/// Check the resolved dependency graph against the release licence policy.
///
/// Exit code 1 when any package is denied or unrecognised, so this can gate a release without
/// `cargo-deny` installed. Every failure is printed with the reason a reviewer needs.
fn cmd_licences(root: &Path) -> Result<i32> {
    let policy_path = root.join(paths::LICENCES);
    let policy = supply::load_policy(&policy_path)?;
    let metadata = supply::metadata(root)?;
    let audit = supply::audit(&metadata, &policy)?;
    let (allowed, denied, unknown) = audit.counts();
    for package in audit.failures() {
        println!(
            "{:?}: {} {} — {}",
            package.verdict, package.name, package.version, package.reason
        );
    }
    println!(
        "{} package(s): {allowed} allowed, {denied} denied, {unknown} unknown; policy {}",
        audit.packages.len(),
        paths::LICENCES
    );
    Ok(if audit.is_ok() { 0 } else { 1 })
}

/// Emit a deterministic CycloneDX SBOM for the resolved graph.
fn cmd_sbom(root: &Path, write: bool) -> Result<i32> {
    let policy = supply::load_policy(&root.join(paths::LICENCES))?;
    let metadata = supply::metadata(root)?;
    let audit = supply::audit(&metadata, &policy)?;
    let sbom = supply::sbom(&metadata, &policy)?;
    let components = sbom
        .get("components")
        .and_then(serde_json::Value::as_array)
        .map(Vec::len)
        .unwrap_or(0);
    println!("CycloneDX 1.5, {components} component(s)");
    if !audit.is_ok() {
        println!(
            "warning: {} package(s) fail the licence policy; the SBOM still records them",
            audit.failures().count()
        );
    }
    if !write {
        println!("dry run: pass --write to write {}", paths::SBOM);
        return Ok(0);
    }
    write_json(root, paths::SBOM, &sbom)?;
    println!("wrote {}", paths::SBOM);
    Ok(0)
}

fn cmd_tasks(root: &Path) -> Result<i32> {
    let manifests = ManifestSet::load(root)?;
    let ready = tasks::next_ready(&manifests.tasks.all());
    if ready.is_empty() {
        println!("no ready tasks");
        return Ok(0);
    }
    for task in ready {
        println!(
            "[{:?}/{:?}] {} — {}",
            task.priority, task.risk, task.id, task.title
        );
    }
    Ok(0)
}

#[derive(Serialize)]
struct HistoryEntry<'a> {
    observed_at_unix: u64,
    scanner: &'a str,
    repos: Vec<HistoryRepo<'a>>,
}

#[derive(Serialize)]
struct HistoryRepo<'a> {
    id: &'a str,
    path: &'a str,
    commit: Option<&'a str>,
    fingerprint: Option<&'a str>,
    dirty_paths: u32,
}

fn append_history(root: &Path, state: &ScanState) -> Result<()> {
    let entry = HistoryEntry {
        observed_at_unix: state.observed_at_unix,
        scanner: cad_engineering::SCANNER_VERSION,
        repos: state
            .repos
            .iter()
            .map(|repo| HistoryRepo {
                id: &repo.id,
                path: &repo.path,
                commit: repo.git.as_ref().and_then(|git| git.commit.as_deref()),
                fingerprint: repo.fingerprint.as_ref().map(|fp| fp.digest.as_str()),
                dirty_paths: repo.git.as_ref().map(|git| git.dirty_paths).unwrap_or(0),
            })
            .collect(),
    };
    let line = serde_json::to_string(&entry)
        .map_err(|error| Error::Message(format!("cannot serialise history entry: {error}")))?;
    let path = ensure_parent(root, paths::SCAN_HISTORY)?;
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&path)
        .map_err(|source| Error::io(&path, source))?;
    writeln!(file, "{line}").map_err(|source| Error::io(&path, source))?;
    Ok(())
}

fn ensure_parent(root: &Path, relative: &str) -> Result<PathBuf> {
    let path = root.join(relative);
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|source| Error::io(parent, source))?;
    }
    Ok(path)
}

fn write_text(root: &Path, relative: &str, text: &str) -> Result<()> {
    let path = ensure_parent(root, relative)?;
    std::fs::write(&path, text).map_err(|source| Error::io(&path, source))
}

fn write_json<T: Serialize>(root: &Path, relative: &str, value: &T) -> Result<()> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| Error::Message(format!("cannot serialise {relative}: {error}")))?;
    write_text(root, relative, &format!("{text}\n"))
}

fn load_json_optional<T: DeserializeOwned>(root: &Path, relative: &str) -> Result<Option<T>> {
    let path = root.join(relative);
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|source| Error::io(&path, source))?;
    let value = serde_json::from_str(&text).map_err(|source| Error::Json {
        path: path.display().to_string(),
        source,
    })?;
    Ok(Some(value))
}

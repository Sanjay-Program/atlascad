//! Change detection.
//!
//! Two comparisons matter, and they are different questions:
//!
//! * **Observed vs previous observation** ([`compare`]) — what changed in the workspace since the
//!   last scan. This is what generates follow-up work.
//! * **Declared vs observed** ([`compare_declared`]) — where a curated manifest is out of date
//!   (a missing checkout, or a HEAD that has moved past the commit the recorded evidence applies
//!   to). This is what stops a manifest from silently describing a repository that no longer
//!   matches.
//!
//! The first scan has no baseline and therefore reports **no** changes; saying "everything changed"
//! on a first observation would generate a task per repository for no information.

use crate::fingerprint;
use crate::model::*;
use std::collections::{BTreeMap, BTreeSet};

/// How many changed paths are listed per change before summarising.
const MAX_LISTED_PATHS: usize = 40;

/// Changes between the previous scan (if any) and the current one, sorted deterministically.
pub fn compare(previous: Option<&ScanState>, current: &ScanState) -> Vec<Change> {
    let Some(previous) = previous else {
        return Vec::new();
    };

    let previous_by_id: BTreeMap<&str, &RepoObservation> = previous
        .repos
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect();
    let current_by_id: BTreeMap<&str, &RepoObservation> = current
        .repos
        .iter()
        .map(|repo| (repo.id.as_str(), repo))
        .collect();

    let mut changes = Vec::new();

    for repo in &current.repos {
        match previous_by_id.get(repo.id.as_str()) {
            None => changes.push(Change {
                repo_id: repo.id.clone(),
                kind: ChangeKind::RepositoryDiscovered,
                summary: format!("`{}` was not observed in the previous scan", repo.path),
                details: vec![format!(
                    "declared in manifests: {}",
                    if repo.declared { "yes" } else { "no" }
                )],
                digest: fingerprint::digest_hex(&["discovered", repo.path.as_str()]),
            }),
            Some(before) => {
                changes.extend(compare_repo(before, repo));
            }
        }
    }

    for repo in &previous.repos {
        if !current_by_id.contains_key(repo.id.as_str()) {
            changes.push(Change {
                repo_id: repo.id.clone(),
                kind: ChangeKind::RepositoryMissing,
                summary: format!("`{}` disappeared from the scan", repo.path),
                details: vec!["it existed during the previous scan".to_string()],
                digest: fingerprint::digest_hex(&["missing", repo.path.as_str()]),
            });
        }
    }

    sort_and_dedupe(changes)
}

fn compare_repo(before: &RepoObservation, after: &RepoObservation) -> Vec<Change> {
    let mut changes = Vec::new();

    if before.exists && !after.exists {
        changes.push(Change {
            repo_id: after.id.clone(),
            kind: ChangeKind::RepositoryMissing,
            summary: format!("`{}` is no longer present on disk", after.path),
            details: Vec::new(),
            digest: fingerprint::digest_hex(&["missing", after.path.as_str()]),
        });
        return changes;
    }

    let before_commit = before.git.as_ref().and_then(|git| git.commit.as_deref());
    let after_commit = after.git.as_ref().and_then(|git| git.commit.as_deref());
    if let (Some(before_commit), Some(after_commit)) = (before_commit, after_commit)
        && before_commit != after_commit
    {
        changes.push(Change {
            repo_id: after.id.clone(),
            kind: ChangeKind::CommitChanged,
            summary: format!("HEAD moved to {}", short(after_commit)),
            details: vec![
                format!("previous: {before_commit}"),
                format!("current:  {after_commit}"),
                "re-validate the modules, public interfaces and tests that this repository owns"
                    .to_string(),
            ],
            digest: after_commit.to_string(),
        });
    }

    let before_files = before.fingerprint.as_ref().map(|fp| fp.digest.clone());
    let after_files = after.fingerprint.as_ref().map(|fp| fp.digest.clone());
    if before_files != after_files
        && let Some(digest) = after_files.clone()
    {
        let (details, modified) = changed_paths(&before.files, &after.files);
        changes.push(Change {
            repo_id: after.id.clone(),
            kind: ChangeKind::FilesModified,
            summary: format!(
                "{} tracked file(s) changed against the previous fingerprint",
                modified
            ),
            details,
            digest,
        });
    }

    let before_dirty = before.git.as_ref().map(|git| git.dirty_paths).unwrap_or(0);
    let after_dirty = after.git.as_ref().map(|git| git.dirty_paths).unwrap_or(0);
    if before_dirty == 0 && after_dirty > 0 {
        changes.push(Change {
            repo_id: after.id.clone(),
            kind: ChangeKind::WorkingTreeDirty,
            summary: format!("{after_dirty} uncommitted path(s) appeared"),
            details: vec![
                "uncommitted work is never discarded, reset or stashed by this tooling".to_string(),
                "review the diff, decide whether it is intended, then commit or hand it back"
                    .to_string(),
            ],
            digest: fingerprint::digest_hex(&[
                after_commit.unwrap_or("no-commit"),
                &after_dirty.to_string(),
                after_files.as_deref().unwrap_or("no-fingerprint"),
            ]),
        });
    }

    let before_packages: BTreeSet<&str> = before.packages.iter().map(String::as_str).collect();
    let after_packages: BTreeSet<&str> = after.packages.iter().map(String::as_str).collect();
    let added: Vec<&str> = after_packages
        .difference(&before_packages)
        .copied()
        .collect();
    let removed: Vec<&str> = before_packages
        .difference(&after_packages)
        .copied()
        .collect();

    if !added.is_empty() {
        changes.push(Change {
            repo_id: after.id.clone(),
            kind: ChangeKind::PackageAdded,
            summary: format!("{} Cargo package(s) added", added.len()),
            details: added.iter().map(|name| format!("+ {name}")).collect(),
            digest: fingerprint::digest_hex(&added),
        });
    }
    if !removed.is_empty() {
        changes.push(Change {
            repo_id: after.id.clone(),
            kind: ChangeKind::PackageRemoved,
            summary: format!("{} Cargo package(s) removed", removed.len()),
            details: removed.iter().map(|name| format!("- {name}")).collect(),
            digest: fingerprint::digest_hex(&removed),
        });
    }

    changes
}

fn changed_paths(
    before: &BTreeMap<String, String>,
    after: &BTreeMap<String, String>,
) -> (Vec<String>, usize) {
    let mut details = Vec::new();
    let mut modified = 0usize;

    for (path, digest) in after {
        match before.get(path) {
            None => {
                modified += 1;
                if details.len() < MAX_LISTED_PATHS {
                    details.push(format!("+ {path}"));
                }
            }
            Some(previous) if previous != digest => {
                modified += 1;
                if details.len() < MAX_LISTED_PATHS {
                    details.push(format!("~ {path}"));
                }
            }
            Some(_) => {}
        }
    }
    for path in before.keys() {
        if !after.contains_key(path) {
            modified += 1;
            if details.len() < MAX_LISTED_PATHS {
                details.push(format!("- {path}"));
            }
        }
    }
    if modified > MAX_LISTED_PATHS {
        details.push(format!(
            "… {} further path(s) not listed",
            modified - MAX_LISTED_PATHS
        ));
    }
    (details, modified)
}

/// Where the curated manifests disagree with what is on disk.
pub fn compare_declared(repositories: &[RepositoryEntry], current: &ScanState) -> Vec<Change> {
    let mut changes = Vec::new();

    for repo in repositories {
        if repo.kind == RepoKind::External {
            continue;
        }
        let observation = current.repo(&repo.id);
        let Some(observation) = observation else {
            changes.push(Change {
                repo_id: repo.id.clone(),
                kind: ChangeKind::RepositoryMissing,
                summary: format!("`{}` was not observed by the scan", repo.path),
                details: vec!["it is declared in repositories.json".to_string()],
                digest: fingerprint::digest_hex(&["not-observed", repo.id.as_str()]),
            });
            continue;
        };

        if !observation.exists {
            changes.push(Change {
                repo_id: repo.id.clone(),
                kind: ChangeKind::RepositoryMissing,
                summary: format!("declared path `{}` does not exist", repo.path),
                details: vec!["the manifest promises a checkout that is not there".to_string()],
                digest: fingerprint::digest_hex(&["absent", repo.path.as_str()]),
            });
            continue;
        }

        let verified = repo.last_verified_commit.as_deref();
        let head = observation
            .git
            .as_ref()
            .and_then(|git| git.commit.as_deref());
        if let (Some(verified), Some(head)) = (verified, head)
            && verified != head
        {
            changes.push(Change {
                repo_id: repo.id.clone(),
                kind: ChangeKind::CommitChanged,
                summary: format!(
                    "HEAD {} is past the commit the recorded evidence covers ({})",
                    short(head),
                    short(verified)
                ),
                details: vec![
                    format!("verified:  {verified}"),
                    format!("checked out: {head}"),
                    "the recorded build/test evidence no longer describes this checkout"
                        .to_string(),
                ],
                digest: head.to_string(),
            });
        }
    }

    sort_and_dedupe(changes)
}

/// Sort and de-duplicate a combined change list. Two sources may observe the same fact (a HEAD
/// that moved past the declared commit is both an observed change and a declaration drift), and
/// the report should show it once.
pub fn normalize(changes: Vec<Change>) -> Vec<Change> {
    sort_and_dedupe(changes)
}

fn sort_and_dedupe(changes: Vec<Change>) -> Vec<Change> {
    let mut seen = BTreeSet::new();
    let mut out: Vec<Change> = changes
        .into_iter()
        .filter(|change| seen.insert((change.repo_id.clone(), change.kind, change.digest.clone())))
        .collect();
    out.sort_by(|a, b| {
        (a.repo_id.as_str(), a.kind, a.digest.as_str()).cmp(&(
            b.repo_id.as_str(),
            b.kind,
            b.digest.as_str(),
        ))
    });
    out
}

fn short(commit: &str) -> &str {
    commit.get(..12).unwrap_or(commit)
}

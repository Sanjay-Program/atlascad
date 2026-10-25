//! The dependency-aware task graph.
//!
//! Scanner tasks are *derived* from observations, so their ids must be derived from the observation
//! too. The id is `task.scan.<kind>.<repo>.<first 8 hex of the observed value>`; an unchanged scan
//! reproduces the same ids, and [`merge`] refuses to insert an id that already exists. The scanner
//! never edits an existing task, so a previous failure or verification record survives.

use crate::model::*;
use std::collections::{BTreeMap, BTreeSet, VecDeque};

/// Result of merging generated tasks into the backlog.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct MergeReport {
    pub inserted: usize,
    pub duplicates: usize,
}

/// Turn observed changes into tasks. Pure: same changes in, same tasks out.
pub fn synthesize(changes: &[Change]) -> Vec<Task> {
    changes.iter().map(synthesize_one).collect()
}

fn synthesize_one(change: &Change) -> Task {
    let (risk, acceptance, tests) = match change.kind {
        ChangeKind::RepositoryMissing => (
            Risk::High,
            vec![
                "the declared path is restored, or the entry is retired with a recorded reason"
                    .to_string(),
                "no other manifest still references the missing repository".to_string(),
            ],
            vec!["run the affected repositories' build commands".to_string()],
        ),
        ChangeKind::WorkingTreeDirty => (
            Risk::Medium,
            vec![
                "the uncommitted diff is reviewed and attributed to a known task".to_string(),
                "it is committed, or handed back to its author; nothing is discarded".to_string(),
                "the scan reports a clean tree afterwards".to_string(),
            ],
            vec![
                "build and test the affected repository before committing".to_string(),
                "check that no generated or build artifact was captured by the fingerprint"
                    .to_string(),
            ],
        ),
        ChangeKind::CommitChanged => (
            Risk::Medium,
            vec![
                "the new commit is inspected for changes to public interfaces".to_string(),
                "affected modules and consumers are identified from the dependency manifest"
                    .to_string(),
                "build and test evidence is re-recorded against the new commit".to_string(),
                "`last_verified_commit` is updated only after those checks pass".to_string(),
            ],
            vec![
                "the repository's own test suite (see test_commands in repositories.json)"
                    .to_string(),
                "contract tests for every interface the repository provides".to_string(),
            ],
        ),
        ChangeKind::RepositoryDiscovered => (
            Risk::Medium,
            vec![
                "the licence is read and recorded before any code is used".to_string(),
                "capabilities and duplicates are compared against existing modules".to_string(),
                "a repository entry is added with a stable id and evidence references".to_string(),
                "no source is copied or merged during onboarding".to_string(),
            ],
            vec![
                "isolated build of the new repository".to_string(),
                "isolated test run of the new repository".to_string(),
            ],
        ),
        ChangeKind::FilesModified | ChangeKind::PackageAdded | ChangeKind::PackageRemoved => (
            Risk::Low,
            vec![
                "the changed paths are classified as source, manifest, test or artefact"
                    .to_string(),
                "an impact analysis names the affected modules and their consumers".to_string(),
                "only the minimum validation set is run, and its result is recorded".to_string(),
                "the manifest fingerprint is refreshed once the change is understood".to_string(),
            ],
            vec![
                "format + lint the owning crate".to_string(),
                "unit tests of the owning crate".to_string(),
                "integration tests of every consumer of a changed interface".to_string(),
            ],
        ),
    };

    Task {
        id: change.task_id(),
        title: format!("{}: {}", change.repo_id, change.summary),
        rationale: format!(
            "Observed by the workspace scanner: {}. Generated tasks are pinned to the observed \
             value, so re-running an unchanged scan does not create another task.",
            change.summary
        ),
        priority: change.kind.priority(),
        risk,
        status: TaskStatus::Ready,
        acceptance_criteria: acceptance,
        inputs: vec![change.repo_id.clone()],
        outputs: vec!["engineering/manifest/repositories.json".to_string()],
        depends_on: Vec::new(),
        blocks: Vec::new(),
        expected_changes: change.details.clone(),
        required_tests: tests,
        attempts: 0,
        last_error: None,
        evidence: Vec::new(),
        rollback: "The scanner changes no source code. Revert the manifest or state edit; no \
                   repository is reset, cleaned or stashed by this tooling."
            .to_string(),
        source: TaskSource::Scanner,
        trigger_digest: Some(change.digest.clone()),
    }
}

/// Insert generated tasks that are not already known. Existing ids are left untouched, whatever
/// their status, so a verified or cancelled task is never reopened by a later scan.
pub fn merge(
    store: &mut TaskStore,
    existing_ids: &BTreeSet<String>,
    incoming: Vec<Task>,
) -> MergeReport {
    let mut known = existing_ids.clone();
    let mut report = MergeReport::default();
    for task in incoming {
        if !known.insert(task.id.clone()) {
            report.duplicates += 1;
            continue;
        }
        store.tasks.push(task);
        report.inserted += 1;
    }
    store.tasks.sort_by(|a, b| a.id.cmp(&b.id));
    report
}

/// Tasks that can be started now: `ready`, with every dependency verified.
pub fn next_ready<'a>(all: &[(&'static str, &'a Task)]) -> Vec<&'a Task> {
    let status: BTreeMap<&str, TaskStatus> = all
        .iter()
        .map(|(_, task)| (task.id.as_str(), task.status))
        .collect();

    let mut ready: Vec<&Task> = all
        .iter()
        .filter(|(_, task)| task.status == TaskStatus::Ready)
        .filter(|(_, task)| {
            task.depends_on
                .iter()
                .all(|dep| status.get(dep.as_str()) == Some(&TaskStatus::Verified))
        })
        .map(|(_, task)| *task)
        .collect();

    ready.sort_by(|a, b| {
        (a.priority, a.risk, a.id.as_str()).cmp(&(b.priority, b.risk, b.id.as_str()))
    });
    ready
}

/// Returns the tasks that actually form a dependency cycle, or `None` for an acyclic graph.
///
/// Kahn's algorithm first removes everything that can be ordered; the remainder contains every
/// cycle plus everything downstream of one. The remainder is then walked along `depends_on` until a
/// node repeats — the repeated segment is the cycle, and a task that merely depends on a cycle is
/// **not** reported as part of it (that would blame innocent work).
pub fn find_cycle(all: &[(&'static str, &Task)]) -> Option<Vec<String>> {
    let ids: BTreeSet<&str> = all.iter().map(|(_, task)| task.id.as_str()).collect();
    let mut indegree: BTreeMap<&str, usize> = ids.iter().map(|id| (*id, 0)).collect();
    let mut dependents: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    let mut depends_on: BTreeMap<&str, Vec<&str>> = BTreeMap::new();

    for (_, task) in all {
        for dependency in &task.depends_on {
            if !ids.contains(dependency.as_str()) {
                continue;
            }
            dependents
                .entry(dependency.as_str())
                .or_default()
                .push(task.id.as_str());
            depends_on
                .entry(task.id.as_str())
                .or_default()
                .push(dependency.as_str());
            if let Some(entry) = indegree.get_mut(task.id.as_str()) {
                *entry += 1;
            }
        }
    }

    let mut queue: VecDeque<&str> = indegree
        .iter()
        .filter(|(_, degree)| **degree == 0)
        .map(|(id, _)| *id)
        .collect();
    let mut visited = 0usize;

    while let Some(id) = queue.pop_front() {
        visited += 1;
        if let Some(next) = dependents.get(id) {
            for target in next {
                if let Some(entry) = indegree.get_mut(target) {
                    *entry = entry.saturating_sub(1);
                    if *entry == 0 {
                        queue.push_back(target);
                    }
                }
            }
        }
    }

    if visited == ids.len() {
        return None;
    }

    let leftover: BTreeSet<&str> = indegree
        .iter()
        .filter(|(_, degree)| **degree > 0)
        .map(|(id, _)| *id)
        .collect();

    let mut start = *leftover.iter().next()?;
    let mut path: Vec<&str> = Vec::new();
    let mut visited_at: BTreeMap<&str, usize> = BTreeMap::new();
    loop {
        if let Some(index) = visited_at.get(start) {
            return Some(path[*index..].iter().map(|id| (*id).to_string()).collect());
        }
        visited_at.insert(start, path.len());
        path.push(start);
        let next = depends_on.get(start).and_then(|dependencies| {
            dependencies
                .iter()
                .copied()
                .find(|dependency| leftover.contains(dependency))
        })?;
        start = next;
    }
}

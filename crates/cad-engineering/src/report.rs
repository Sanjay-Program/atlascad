//! Integration report rendering.
//!
//! The report is generated **from the manifests and the last scan**, never from prose someone
//! typed. Every number below is a count over a real collection, and a metric that could not be
//! measured is printed as `not run` rather than omitted.

use crate::ManifestSet;
use crate::model::*;
use crate::tasks::{self, MergeReport};
use std::collections::BTreeMap;
use std::fmt::Write as _;

pub struct ReportInput<'a> {
    pub manifests: &'a ManifestSet,
    pub scan: Option<&'a ScanState>,
    pub changes: &'a [Change],
    /// False on the very first scan, when no comparison baseline exists.
    pub compared_with_previous: bool,
    pub merge: Option<MergeReport>,
    pub generated_at_unix: u64,
}

pub fn render(input: &ReportInput<'_>) -> String {
    let mut out = String::new();

    let _ = writeln!(out, "# Atlas CAD — Integration Report");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "**Generated:** {} (unix {}) · **Scanner:** {}",
        iso8601(input.generated_at_unix),
        input.generated_at_unix,
        crate::SCANNER_VERSION
    );
    match input.scan {
        Some(scan) => {
            let _ = writeln!(
                out,
                "**Last observation:** {} · **Host:** {}/{} · {} CPUs",
                iso8601(scan.observed_at_unix),
                scan.host.os,
                scan.host.arch,
                scan.host.cpus
            );
        }
        None => {
            let _ = writeln!(
                out,
                "**Last observation:** none — run `atlas-eng scan --write` to create one."
            );
        }
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "> Generated file. Edit the manifests under `engineering/manifest/`, not this report."
    );
    let _ = writeln!(out);

    render_overview(input, &mut out);
    render_repositories(input, &mut out);
    render_licences(input, &mut out);
    render_evidence(input, &mut out);
    render_changes(input, &mut out);
    render_tasks(input, &mut out);
    render_gaps(input, &mut out);
    out
}

fn render_overview(input: &ReportInput<'_>, out: &mut String) {
    let manifests = input.manifests;
    let repos = &manifests.repositories.repositories;
    let count_status = |status: ComponentStatus| {
        repos
            .iter()
            .filter(|repo| repo.integration_status == status)
            .count()
    };

    let mut task_counts: BTreeMap<TaskStatus, usize> = BTreeMap::new();
    for (_, task) in manifests.tasks.all() {
        *task_counts.entry(task.status).or_insert(0) += 1;
    }
    let task_count = |status: TaskStatus| task_counts.get(&status).copied().unwrap_or(0);

    let capability_count = |status: VerificationStatus| {
        manifests
            .capabilities
            .capabilities
            .iter()
            .filter(|capability| capability.verification == status)
            .count()
    };

    let _ = writeln!(out, "## 1. Control plane");
    let _ = writeln!(out);
    let _ = writeln!(out, "| Metric | Count |");
    let _ = writeln!(out, "|---|---:|");
    let _ = writeln!(out, "| Repositories declared | {} |", repos.len());
    let _ = writeln!(
        out,
        "| Repositories observed | {} |",
        input.scan.map(|scan| scan.repos.len()).unwrap_or(0)
    );
    let _ = writeln!(
        out,
        "| — validated | {} |",
        count_status(ComponentStatus::Validated)
    );
    let _ = writeln!(
        out,
        "| — integrated | {} |",
        count_status(ComponentStatus::Integrated)
    );
    let _ = writeln!(
        out,
        "| — verified | {} |",
        count_status(ComponentStatus::Verified)
    );
    let _ = writeln!(
        out,
        "| — blocked | {} |",
        count_status(ComponentStatus::Blocked)
    );
    let _ = writeln!(
        out,
        "| — retired | {} |",
        count_status(ComponentStatus::Retired)
    );
    let _ = writeln!(out, "| Modules | {} |", manifests.modules.modules.len());
    let _ = writeln!(
        out,
        "| Interfaces | {} |",
        manifests.interfaces.interfaces.len()
    );
    let _ = writeln!(
        out,
        "| Capabilities — verified | {} |",
        capability_count(VerificationStatus::Verified)
    );
    let _ = writeln!(
        out,
        "| Capabilities — advertised only | {} |",
        capability_count(VerificationStatus::Advertised)
    );
    let _ = writeln!(
        out,
        "| Capabilities — absent | {} |",
        capability_count(VerificationStatus::Absent)
    );
    let _ = writeln!(
        out,
        "| Agents — implemented | {} |",
        manifests
            .agents
            .agents
            .iter()
            .filter(|a| a.implemented)
            .count()
    );
    let _ = writeln!(
        out,
        "| Agents — active | {} |",
        manifests
            .agents
            .agents
            .iter()
            .filter(|a| a.lifecycle.is_active())
            .count()
    );
    for (status, label) in [
        (TaskStatus::Proposed, "proposed"),
        (TaskStatus::Ready, "ready"),
        (TaskStatus::InProgress, "in progress"),
        (TaskStatus::Blocked, "blocked"),
        (TaskStatus::NeedsReview, "needs review"),
        (TaskStatus::Failed, "failed"),
        (TaskStatus::Verified, "verified"),
        (TaskStatus::Cancelled, "cancelled"),
    ] {
        let _ = writeln!(out, "| Tasks — {label} | {} |", task_count(status));
    }
    let _ = writeln!(out);
}

fn render_repositories(input: &ReportInput<'_>, out: &mut String) {
    let scan = input.scan;
    let _ = writeln!(out, "## 2. Repositories");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "| Id | Kind | Status | Licence | HEAD | Verified commit | Tree |"
    );
    let _ = writeln!(out, "|---|---|---|---|---|---|---|");

    for repo in &input.manifests.repositories.repositories {
        let observation = scan.and_then(|scan| scan.repo(&repo.id));
        let head = observation
            .and_then(|obs| obs.git.as_ref())
            .and_then(|git| git.commit.as_deref())
            .map(short_commit)
            .unwrap_or_else(|| {
                if repo.kind == RepoKind::External {
                    "—".to_string()
                } else if observation.is_some() {
                    "no git".to_string()
                } else {
                    "not scanned".to_string()
                }
            });
        let verified = repo
            .last_verified_commit
            .as_deref()
            .map(short_commit)
            .unwrap_or_else(|| "—".to_string());
        let dirty = observation
            .and_then(|obs| obs.git.as_ref())
            .map(|git| {
                if git.dirty_paths == 0 {
                    "clean".to_string()
                } else {
                    format!("{} dirty", git.dirty_paths)
                }
            })
            .unwrap_or_else(|| "—".to_string());
        let _ = writeln!(
            out,
            "| `{}` | {:?} | {:?} | {} | {} | {} | {} |",
            repo.id, repo.kind, repo.integration_status, repo.license.spdx, head, verified, dirty
        );
    }
    let _ = writeln!(out);
}

fn render_licences(input: &ReportInput<'_>, out: &mut String) {
    let _ = writeln!(out, "## 3. Licence blockers");
    let _ = writeln!(out);
    let flagged: Vec<&RepositoryEntry> = input
        .manifests
        .repositories
        .repositories
        .iter()
        .filter(|repo| repo.license.status != LicenseStatus::Approved)
        .collect();
    if flagged.is_empty() {
        let _ = writeln!(
            out,
            "No repository is outside the approved set. This is a manifest statement, not legal \
             advice; see `docs/license-inventory.md`."
        );
    } else {
        let _ = writeln!(out, "| Id | Licence | Status | Note |");
        let _ = writeln!(out, "|---|---|---|---|");
        for repo in flagged {
            let _ = writeln!(
                out,
                "| `{}` | {} | {:?} | {} |",
                repo.id, repo.license.spdx, repo.license.status, repo.license.notes
            );
        }
    }
    let _ = writeln!(out);
}

/// Records that may be summed without double counting.
///
/// A summary record duplicates runs that are also recorded per package, and a superseding record
/// replaces an older one that stays in the manifest as history. Counting either inflates the total.
fn counted_test_records(records: &[TestRecord]) -> Vec<&TestRecord> {
    let superseded: std::collections::BTreeSet<&str> = records
        .iter()
        .flat_map(|record| record.supersedes.iter().map(String::as_str))
        .collect();
    records
        .iter()
        .filter(|record| !record.summary && !superseded.contains(record.id.as_str()))
        .collect()
}

fn render_evidence(input: &ReportInput<'_>, out: &mut String) {
    let manifests = input.manifests;
    let outcome_count = |records: &[Outcome]| {
        (
            records.iter().filter(|o| **o == Outcome::Success).count(),
            records.iter().filter(|o| **o == Outcome::Failure).count(),
            records.iter().filter(|o| **o == Outcome::NotRun).count(),
        )
    };
    let build_outcomes: Vec<Outcome> = manifests.builds.records.iter().map(|r| r.outcome).collect();
    let (built, build_failed, build_not_run) = outcome_count(&build_outcomes);
    let test_outcomes: Vec<Outcome> = manifests.tests.records.iter().map(|r| r.outcome).collect();
    let (tested, test_failed, test_not_run) = outcome_count(&test_outcomes);
    let counted = counted_test_records(&manifests.tests.records);
    let passed: u32 = counted.iter().map(|r| r.passed).sum();
    let failed: u32 = counted.iter().map(|r| r.failed).sum();
    let skipped: u32 = counted.iter().map(|r| r.skipped).sum();

    let _ = writeln!(out, "## 4. Recorded evidence");
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "* Builds: **{built}** succeeded, **{build_failed}** failed, **{build_not_run}** recorded as not run."
    );
    let _ = writeln!(
        out,
        "* Tests: **{tested}** runs succeeded, **{test_failed}** failed, **{test_not_run}** not run \
         — {passed} tests passed, {failed} failed, {skipped} skipped across the counted per-package \
         records (workspace summaries and superseded records excluded; imported applications and \
         corpus examples are included, so this is not a single-suite root-workspace total)."
    );
    let _ = writeln!(
        out,
        "* Benchmarks: **{}** recorded measurement(s).",
        manifests.benchmarks.records.len()
    );

    let open_findings: Vec<&SecurityFinding> = manifests
        .security
        .findings
        .iter()
        .filter(|finding| !finding.status.is_closed())
        .collect();
    let security_scans = manifests.security.scans.len();
    let security_ran = manifests
        .security
        .scans
        .iter()
        .filter(|scan| scan.outcome == Outcome::Success)
        .count();
    let _ = writeln!(
        out,
        "* Security: **{}** open finding(s); **{}/{}** scans ran successfully.",
        open_findings.len(),
        security_ran,
        security_scans
    );
    for finding in &open_findings {
        let _ = writeln!(
            out,
            "  * {} — {} ({:?}, {:?})",
            finding.id, finding.title, finding.severity, finding.status
        );
    }
    let _ = writeln!(out);
}

fn render_changes(input: &ReportInput<'_>, out: &mut String) {
    let _ = writeln!(out, "## 5. Changes");
    let _ = writeln!(out);
    if !input.compared_with_previous {
        let _ = writeln!(
            out,
            "Baseline observation: there is no previous scan, so nothing is reported as changed. \
             The next scan will compare against this one."
        );
        let _ = writeln!(out);
        return;
    }
    if input.changes.is_empty() {
        let _ = writeln!(
            out,
            "No change detected against the previous observation. Stable ids were reproduced, so \
             no new task was generated."
        );
        let _ = writeln!(out);
        return;
    }
    let _ = writeln!(
        out,
        "{} change(s) detected against the previous observation.",
        input.changes.len()
    );
    let _ = writeln!(out);
    for change in input.changes {
        let _ = writeln!(
            out,
            "* **`{}`** · {:?} · {}",
            change.repo_id, change.kind, change.summary
        );
        for detail in &change.details {
            let _ = writeln!(out, "  * {detail}");
        }
    }
    let _ = writeln!(out);
}

fn render_tasks(input: &ReportInput<'_>, out: &mut String) {
    let _ = writeln!(out, "## 6. Tasks");
    let _ = writeln!(out);
    if let Some(merge) = input.merge {
        let _ = writeln!(
            out,
            "This scan inserted **{}** task(s); **{}** were already known and left untouched.",
            merge.inserted, merge.duplicates
        );
        let _ = writeln!(out);
    }
    let ready = tasks::next_ready(&input.manifests.tasks.all());
    if ready.is_empty() {
        let _ = writeln!(out, "No task is currently ready.");
    } else {
        let _ = writeln!(out, "Next unblocked tasks, in priority order:");
        let _ = writeln!(out);
        let _ = writeln!(out, "| Id | Priority | Risk | Title |");
        let _ = writeln!(out, "|---|---|---|---|");
        for task in ready.iter().take(12) {
            let _ = writeln!(
                out,
                "| `{}` | {:?} | {:?} | {} |",
                task.id, task.priority, task.risk, task.title
            );
        }
    }
    let _ = writeln!(out);
}

fn render_gaps(input: &ReportInput<'_>, out: &mut String) {
    let manifests = input.manifests;
    let _ = writeln!(out, "## 7. Claims that are not yet verified");
    let _ = writeln!(out);

    let mut wrote = false;
    for capability in &manifests.capabilities.capabilities {
        if capability.verification == VerificationStatus::Advertised {
            wrote = true;
            let _ = writeln!(
                out,
                "* Capability `{}` is advertised by {} but no test is recorded against it.",
                capability.id,
                capability.provided_by.join(", ")
            );
        }
    }
    for module in &manifests.modules.modules {
        if matches!(module.tests, TestStatus::None | TestStatus::Unknown) {
            wrote = true;
            let _ = writeln!(
                out,
                "* Module `{}` has no recorded test status ({:?}).",
                module.id, module.tests
            );
        }
    }
    for agent in &manifests.agents.agents {
        if !agent.implemented {
            wrote = true;
            let _ = writeln!(
                out,
                "* Agent `{}` is declared but not implemented; lifecycle is `{:?}`.",
                agent.id, agent.lifecycle
            );
        }
    }
    for scan in &manifests.security.scans {
        if scan.outcome != Outcome::Success {
            wrote = true;
            let _ = writeln!(
                out,
                "* Security scan `{}` is recorded as {:?}: {}",
                scan.tool, scan.outcome, scan.notes
            );
        }
    }
    if !wrote {
        let _ = writeln!(out, "None recorded.");
    }
    let _ = writeln!(out);
    let _ = writeln!(
        out,
        "Counts above are derived from `engineering/manifest/*.json`. A capability is only \
         `verified` when a recorded test exercised it; a build result is not a test result."
    );
}

fn short_commit(commit: &str) -> String {
    commit.chars().take(12).collect()
}

/// ISO-8601 UTC without pulling in a date library (days-to-civil conversion, Hinnant's algorithm).
pub fn iso8601(unix_seconds: u64) -> String {
    let days = (unix_seconds / 86_400) as i64;
    let seconds_of_day = unix_seconds % 86_400;
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        seconds_of_day / 3600,
        (seconds_of_day % 3600) / 60,
        seconds_of_day % 60
    )
}

fn civil_from_days(days_since_epoch: i64) -> (i64, u32, u32) {
    let z = days_since_epoch + 719_468;
    let era = if z >= 0 { z } else { z - 146_096 } / 146_097;
    let day_of_era = z - era * 146_097;
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let year = year_of_era + era * 400;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let month_prime = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * month_prime + 2) / 5 + 1;
    let month = month_prime + if month_prime < 10 { 3 } else { -9 };
    (year + i64::from(month <= 2), month as u32, day as u32)
}

#[cfg(test)]
mod tests {
    use super::{counted_test_records, iso8601};
    use crate::model::{Outcome, TestRecord};

    fn record(id: &str, summary: bool, supersedes: &[&str]) -> TestRecord {
        TestRecord {
            id: id.to_string(),
            target: "repo.test".into(),
            command: "cargo test".into(),
            outcome: Outcome::Success,
            passed: 3,
            failed: 0,
            skipped: 0,
            duration_ms: None,
            recorded_at: "2026-10-25".into(),
            notes: String::new(),
            summary,
            supersedes: supersedes
                .iter()
                .map(|value| (*value).to_string())
                .collect(),
        }
    }

    #[test]
    fn summaries_and_superseded_records_are_not_summed_twice() {
        let records = vec![
            record("old", false, &[]),
            record("new", false, &["old"]),
            record("workspace", true, &[]),
        ];
        let counted = counted_test_records(&records);
        assert_eq!(counted.len(), 1);
        assert_eq!(counted[0].id, "new");
    }

    #[test]
    fn a_superseding_record_does_not_hide_itself() {
        let records = vec![record("only", false, &["absent"])];
        assert_eq!(counted_test_records(&records).len(), 1);
    }

    #[test]
    fn epoch_formats_as_iso_utc() {
        assert_eq!(iso8601(0), "1970-01-01T00:00:00Z");
    }

    #[test]
    fn known_timestamps_round_trip() {
        // 2026-10-21T20:55:35Z, the REPOSITORY_INVENTORY.txt generation time.
        assert_eq!(iso8601(1_792_616_135), "2026-10-21T20:55:35Z");
    }
}

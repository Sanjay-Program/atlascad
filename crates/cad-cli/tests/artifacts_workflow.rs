//! `atlas artifacts` end to end: a derived drawing and a toolpath pinned to a design revision, then
//! the design edited underneath them. The exit code, not only the JSON, is what a release gate uses.

use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "atlas-artifacts-workflow-{}-{serial}",
            std::process::id()
        ));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(args)
        .output()?)
}

fn stdout_json(output: &Output) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn revision(output: &Output) -> Result<u64, Box<dyn std::error::Error>> {
    stdout_json(output)?["revision"]
        .as_u64()
        .ok_or_else(|| "the CLI summary has no revision".into())
}

fn status_of(report: &serde_json::Value, id: u64) -> Result<String, Box<dyn std::error::Error>> {
    let artifacts = report["artifacts"]
        .as_array()
        .ok_or("the report has no artifacts")?;
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact["id"].as_u64() == Some(id))
        .ok_or("the report does not contain the artifact")?;
    Ok(artifact["status"]["state"]
        .as_str()
        .ok_or("the artifact status has no state")?
        .to_string())
}

fn releasable(report: &serde_json::Value, id: u64) -> Result<bool, Box<dyn std::error::Error>> {
    let artifacts = report["artifacts"]
        .as_array()
        .ok_or("the report has no artifacts")?;
    let artifact = artifacts
        .iter()
        .find(|artifact| artifact["id"].as_u64() == Some(id))
        .ok_or("the report does not contain the artifact")?;
    artifact["releasable"]
        .as_bool()
        .ok_or_else(|| "the artifact has no releasable flag".into())
}

#[test]
fn derived_artifacts_are_pinned_to_a_revision_and_a_design_change_blocks_release()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("bracket.zcad");
    let registry = dir.0.join("bracket.artifacts.json");
    let project_arg = project.to_string_lossy().into_owned();
    let registry_arg = registry.to_string_lossy().into_owned();

    let created = run(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--name",
        "bracket",
        "--size",
        "10,10,5",
    ])?;
    assert!(created.status.success());
    let first = revision(&created)?;
    assert!(first >= 1);
    let revision_arg = first.to_string();

    let drawing = run(&[
        "artifacts",
        "add",
        "--registry",
        &registry_arg,
        "--document",
        "1",
        "--revision",
        &revision_arg,
        "--id",
        "1",
        "--kind",
        "drawing",
        "--label",
        "shop drawing",
        "--entity",
        "1,2",
        "--summary",
        "plan view of the bracket",
    ])?;
    assert!(
        drawing.status.success(),
        "{}",
        String::from_utf8_lossy(&drawing.stderr)
    );
    assert_eq!(stdout_json(&drawing)?["artifacts"], 1);

    let toolpath = run(&[
        "artifacts",
        "add",
        "--registry",
        &registry_arg,
        "--document",
        "1",
        "--revision",
        &revision_arg,
        "--id",
        "2",
        "--kind",
        "toolpath",
        "--label",
        "roughing",
        "--upstream",
        "1",
    ])?;
    assert!(
        toolpath.status.success(),
        "{}",
        String::from_utf8_lossy(&toolpath.stderr)
    );

    let current = run(&[
        "artifacts",
        "check",
        "--project",
        &project_arg,
        "--registry",
        &registry_arg,
    ])?;
    assert!(current.status.success());
    let report = stdout_json(&current)?;
    assert_eq!(report["requires_action"], false);
    assert_eq!(status_of(&report, 1)?, "current");
    assert_eq!(status_of(&report, 2)?, "current");
    assert!(releasable(&report, 1)?);
    assert!(releasable(&report, 2)?);

    // Change the model. Every derived artifact now belongs to an older revision.
    let edited = run(&[
        "project",
        "set",
        "--file",
        &project_arg,
        "--param",
        "depth",
        "--value",
        "8",
    ])?;
    assert!(edited.status.success());
    let second = revision(&edited)?;
    assert!(second > first);
    let revision_arg = second.to_string();

    let stale = run(&[
        "artifacts",
        "check",
        "--project",
        &project_arg,
        "--registry",
        &registry_arg,
    ])?;
    assert_eq!(
        stale.status.code(),
        Some(3),
        "a stale derived artifact must fail a release gate, not only in prose"
    );
    let report = stdout_json(&stale)?;
    assert_eq!(report["requires_action"], true);
    assert_eq!(status_of(&report, 1)?, "requires_review");
    assert_eq!(status_of(&report, 2)?, "requires_review");
    assert!(!releasable(&report, 1)?);
    let drawing = report["artifacts"]
        .as_array()
        .ok_or("artifacts")?
        .iter()
        .find(|artifact| artifact["id"].as_u64() == Some(1))
        .ok_or("drawing")?;
    assert_eq!(drawing["source"]["revision"].as_u64(), Some(first));
    assert_eq!(drawing["status"]["current_revision"].as_u64(), Some(second));

    // Reviewing the drawing acknowledges its staleness without pretending it was recomputed.
    let reviewed = run(&[
        "artifacts",
        "review",
        "--registry",
        &registry_arg,
        "--id",
        "1",
        "--revision",
        &revision_arg,
    ])?;
    assert!(reviewed.status.success());
    let partial = run(&[
        "artifacts",
        "check",
        "--project",
        &project_arg,
        "--registry",
        &registry_arg,
    ])?;
    assert_eq!(partial.status.code(), Some(3));
    let report = stdout_json(&partial)?;
    assert_eq!(status_of(&report, 1)?, "accepted_after_review");
    assert!(releasable(&report, 1)?);
    assert_eq!(status_of(&report, 2)?, "requires_review");
    assert!(!releasable(&report, 2)?);

    // Reviewing the toolpath too releases the set; its source revision is still the first one.
    let reviewed = run(&[
        "artifacts",
        "review",
        "--registry",
        &registry_arg,
        "--id",
        "2",
        "--revision",
        &revision_arg,
    ])?;
    assert!(reviewed.status.success());
    let released = run(&[
        "artifacts",
        "check",
        "--project",
        &project_arg,
        "--registry",
        &registry_arg,
    ])?;
    assert!(released.status.success());
    let report = stdout_json(&released)?;
    assert_eq!(report["requires_action"], false);
    assert_eq!(status_of(&report, 2)?, "accepted_after_review");
    for artifact in report["artifacts"].as_array().ok_or("artifacts")? {
        assert_eq!(artifact["source"]["revision"].as_u64(), Some(first));
        assert_eq!(
            artifact["status"]["reviewed_revision"].as_u64(),
            Some(second)
        );
    }

    // A file on disk now holds the acknowledgment, so a later process sees the same state.
    let rechecked = run(&[
        "artifacts",
        "check",
        "--project",
        &project_arg,
        "--registry",
        &registry_arg,
    ])?;
    assert!(rechecked.status.success());
    Ok(())
}

#[test]
fn an_unknown_upstream_artifact_and_a_bad_kind_are_refused()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let registry = dir.0.join("bad.artifacts.json");
    let registry_arg = registry.to_string_lossy().into_owned();

    let unknown = run(&[
        "artifacts",
        "add",
        "--registry",
        &registry_arg,
        "--document",
        "1",
        "--revision",
        "1",
        "--id",
        "2",
        "--kind",
        "toolpath",
        "--label",
        "roughing",
        "--upstream",
        "99",
    ])?;
    assert!(!unknown.status.success());
    assert!(
        String::from_utf8_lossy(&unknown.stderr).contains("99"),
        "the refusal must name the missing upstream"
    );
    assert!(!registry.exists(), "a refused registry must not be written");

    let bad_kind = run(&[
        "artifacts",
        "add",
        "--registry",
        &registry_arg,
        "--document",
        "1",
        "--revision",
        "1",
        "--id",
        "1",
        "--kind",
        "hologram",
        "--label",
        "x",
    ])?;
    assert!(!bad_kind.status.success());
    assert!(
        String::from_utf8_lossy(&bad_kind.stderr).contains("not an artifact kind"),
        "{}",
        String::from_utf8_lossy(&bad_kind.stderr)
    );
    Ok(())
}

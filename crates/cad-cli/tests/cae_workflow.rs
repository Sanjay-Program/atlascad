//! `atlas cae static` end to end: a real study on a real project, recorded with its revision, and a
//! release gate that a review cannot open for a simulation.

use cad_cae::{Axis, FaceLoad, FaceSelection, StaticStudy, Support};
use std::error::Error;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "atlas-cae-workflow-{}-{serial}",
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

fn run(args: &[&str]) -> Result<Output, Box<dyn Error>> {
    Ok(Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(args)
        .output()?)
}

fn stdout_json(output: &Output) -> Result<serde_json::Value, Box<dyn Error>> {
    Ok(serde_json::from_slice(&output.stdout)?)
}

fn succeeded(output: &Output, what: &str) -> Result<serde_json::Value, Box<dyn Error>> {
    assert!(
        output.status.success(),
        "{what} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    stdout_json(output)
}

#[test]
fn a_static_study_is_recorded_and_only_a_recompute_can_release_it() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("cae.zcad");
    let result_file = dir.0.join("study.json");
    let project_arg = project.to_string_lossy().into_owned();
    let result_arg = result_file.to_string_lossy().into_owned();

    let created = run(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--name",
        "cae",
        "--size",
        "10,10,5",
    ])?;
    let created = succeeded(&created, "project new")?;
    let first_revision = created["revision"]
        .as_u64()
        .ok_or("the project summary has no revision")?;

    let study = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--voxel",
        "2.5",
        "--material",
        "steel.1018",
        "--fix",
        "x=0",
        "--load",
        "x=10",
        "--force",
        "0,0,-100",
        "--out",
        &result_arg,
        "--record",
        "1",
        "--label",
        "bracket static",
    ])?;
    let report = succeeded(&study, "cae static")?;
    assert_eq!(report["mesh"]["elements"].as_u64(), Some(32));
    assert_eq!(report["mesh"]["voxel_size_mm"].as_f64(), Some(2.5));
    assert_eq!(report["solver"]["converged"], true);
    assert_eq!(report["quality"], cad_cae::QUALITY_LABEL);
    assert!(
        report["results"]["max_von_mises_mpa"]
            .as_f64()
            .is_some_and(|value| value > 0.0)
    );
    assert!(
        report["results"]["factor_of_safety"]
            .as_f64()
            .is_some_and(|value| value > 0.0)
    );
    // The result file is the same document that was printed, and it carries its assumptions.
    let written = std::fs::read(&result_file)?;
    let written: serde_json::Value = serde_json::from_slice(&written)?;
    assert_eq!(written["study"], "linear static");
    assert!(
        written["warnings"]
            .as_array()
            .is_some_and(|warnings| !warnings.is_empty())
    );

    let current = run(&["artifacts", "check", "--project", &project_arg])?;
    assert_eq!(current.status.code(), Some(0));
    let checked = stdout_json(&current)?;
    assert_eq!(checked["artifacts"][0]["kind"], "simulation");
    assert_eq!(checked["artifacts"][0]["policy"], "recompute");
    assert_eq!(checked["artifacts"][0]["releasable"], true);

    // Change the design. The result now belongs to an older revision.
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
    let edited = succeeded(&edited, "project set")?;
    let second_revision = edited["revision"]
        .as_u64()
        .ok_or("the edited summary has no revision")?;
    assert!(second_revision > first_revision);

    let stale = run(&["artifacts", "check", "--project", &project_arg])?;
    assert_eq!(stale.status.code(), Some(3));
    let stale_report = stdout_json(&stale)?;
    assert_eq!(
        stale_report["artifacts"][0]["status"]["state"],
        "requires_review"
    );
    assert_eq!(stale_report["artifacts"][0]["releasable"], false);

    // A review is an acknowledgment, not a recompute, so it cannot release a simulation.
    let reviewed = run(&[
        "artifacts",
        "review",
        "--project",
        &project_arg,
        "--id",
        "1",
        "--revision",
        &second_revision.to_string(),
    ])?;
    assert!(reviewed.status.success());
    let blocked = run(&["artifacts", "check", "--project", &project_arg])?;
    assert_eq!(
        blocked.status.code(),
        Some(3),
        "review must not release a stale simulation"
    );
    let blocked_report = stdout_json(&blocked)?;
    assert_eq!(
        blocked_report["artifacts"][0]["status"]["state"],
        "accepted_after_review"
    );
    assert_eq!(blocked_report["artifacts"][0]["releasable"], false);

    // Recomputing the study at the new revision replaces the record and opens the gate.
    let recomputed = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--voxel",
        "2.5",
        "--material",
        "steel.1018",
        "--fix",
        "x=0",
        "--load",
        "x=10",
        "--force",
        "0,0,-100",
        "--record",
        "1",
    ])?;
    let recomputed = succeeded(&recomputed, "cae static (recompute)")?;
    assert_eq!(recomputed["mesh"]["elements"].as_u64(), Some(48));
    let cleared = run(&["artifacts", "check", "--project", &project_arg])?;
    assert_eq!(cleared.status.code(), Some(0));
    let cleared_report = stdout_json(&cleared)?;
    assert_eq!(cleared_report["artifacts"][0]["status"]["state"], "current");
    assert_eq!(cleared_report["artifacts"][0]["releasable"], true);
    Ok(())
}

#[test]
fn a_study_without_output_or_a_bad_face_is_refused() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("bad.zcad");
    let project_arg = project.to_string_lossy().into_owned();
    let created = run(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--size",
        "10,10,5",
    ])?;
    assert!(created.status.success());

    // No output and no record would compute a study and silently discard it.
    let silent = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--voxel",
        "2.5",
        "--material",
        "steel.1018",
        "--fix",
        "x=0",
        "--load",
        "x=10",
        "--force",
        "0,0,-1",
    ])
    .map(|output| output.status.code());
    // Bad usage is exit 2: the job input is wrong, not an analysis refusal.
    assert_eq!(silent?, Some(2));

    let bad_axis = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--voxel",
        "2.5",
        "--material",
        "steel.1018",
        "--fix",
        "w=0",
        "--load",
        "x=10",
        "--force",
        "0,0,-1",
        "--record",
        "1",
    ])
    .map(|output| output.status.code());
    assert_eq!(bad_axis?, Some(2));
    Ok(())
}

#[test]
fn a_study_file_is_reproducible_and_carries_its_provenance() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("job.zcad");
    let study_file = dir.0.join("study.json");
    let flags_result = dir.0.join("flags.json");
    let file_result = dir.0.join("file.json");
    let project_arg = project.to_string_lossy().into_owned();
    let study_arg = study_file.to_string_lossy().into_owned();
    let file_arg = file_result.to_string_lossy().into_owned();
    let flags_arg = flags_result.to_string_lossy().into_owned();

    let created = run(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--size",
        "60,10,10",
    ])?;
    let created = succeeded(&created, "project new")?;
    let revision = created["revision"].as_u64().ok_or("no revision")?;

    let mut study = StaticStudy::new("cantilever job", 2.5, "steel.1018");
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 60.0),
        [0.0, 0.0, -100.0],
    )];
    std::fs::write(&study_file, study.to_json()?)?;

    let from_file = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--study",
        &study_arg,
        "--out",
        &file_arg,
    ])?;
    let from_file = succeeded(&from_file, "cae static --study")?;
    let from_flags = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--voxel",
        "2.5",
        "--material",
        "steel.1018",
        "--fix",
        "x=0",
        "--load",
        "x=60",
        "--force",
        "0,0,-100",
        "--out",
        &flags_arg,
    ])?;
    let from_flags = succeeded(&from_flags, "cae static (flags)")?;

    // The file is the same job as the flags, not a second interpretation of them.
    assert_eq!(from_file["definition"], serde_json::json!(study));
    assert_eq!(from_file["engine"], "cad-cae 0.1.0");
    assert_eq!(from_file["source"]["revision"].as_u64(), Some(revision));
    assert_eq!(from_file["source"]["document"].as_u64(), Some(1));
    assert_eq!(from_file["results"], from_flags["results"]);
    assert_eq!(from_file["mesh"], from_flags["mesh"]);

    // The written file is the printed document, and the atomic write left no temporary behind.
    let written: serde_json::Value = serde_json::from_slice(&std::fs::read(&file_result)?)?;
    assert_eq!(written, from_file);
    let leftovers: Vec<String> = std::fs::read_dir(&dir.0)?
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .filter(|name| name.contains(".tmp-"))
        .collect();
    assert!(
        leftovers.is_empty(),
        "the atomic result write left temporary files: {leftovers:?}"
    );
    Ok(())
}

#[test]
fn bad_job_input_is_exit_2_and_a_refused_analysis_is_exit_3() -> Result<(), Box<dyn Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("job.zcad");
    let out = dir.0.join("result.json");
    let project_arg = project.to_string_lossy().into_owned();
    let out_arg = out.to_string_lossy().into_owned();

    let created = run(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--size",
        "60,10,10",
    ])?;
    assert!(created.status.success());

    let mut study = StaticStudy::new("cantilever", 2.5, "steel.1018");
    study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
    study.loads = vec![FaceLoad::new(
        FaceSelection::new(Axis::X, 60.0),
        [0.0, 0.0, -100.0],
    )];

    let write_study = |name: &str, value: &serde_json::Value| -> Result<String, Box<dyn Error>> {
        let path = dir.0.join(name);
        std::fs::write(&path, serde_json::to_vec(value)?)?;
        Ok(path.to_string_lossy().into_owned())
    };

    // Malformed JSON and a newer schema are broken job files.
    let malformed = dir.0.join("malformed.json");
    std::fs::write(&malformed, b"{not json")?;
    let malformed_arg = malformed.to_string_lossy().into_owned();
    let code = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--study",
        &malformed_arg,
        "--out",
        &out_arg,
    ])?
    .status
    .code();
    assert_eq!(code, Some(2));

    let newer = write_study(
        "newer.json",
        &serde_json::json!({ "schema": 2, "study": study }),
    )?;
    let code = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--study",
        &newer,
        "--out",
        &out_arg,
    ])?
    .status
    .code();
    assert_eq!(code, Some(2));

    // A valid file whose study cannot be analysed is a refusal: retrying it cannot help.
    let empty =
        serde_json::json!({ "schema": 1, "study": StaticStudy::new("empty", 2.5, "steel.1018") });
    let empty = write_study("empty.json", &empty)?;
    let output = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--study",
        &empty,
        "--out",
        &out_arg,
    ])?;
    assert_eq!(output.status.code(), Some(3));
    assert!(!out.exists(), "a refused study must not publish a result");

    let mut unknown = study.clone();
    unknown.material = "unobtainium".into();
    let unknown = write_study(
        "unknown.json",
        &serde_json::json!({ "schema": 1, "study": unknown }),
    )?;
    let code = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--study",
        &unknown,
        "--out",
        &out_arg,
    ])?
    .status
    .code();
    assert_eq!(code, Some(3));

    // Mixing a study file with study flags is ambiguous, so it is bad input.
    let value = serde_json::json!({ "schema": 1, "study": study });
    let mixed = write_study("mixed.json", &value)?;
    let code = run(&[
        "cae",
        "static",
        "--file",
        &project_arg,
        "--study",
        &mixed,
        "--voxel",
        "2.5",
        "--out",
        &out_arg,
    ])?
    .status
    .code();
    assert_eq!(code, Some(2));
    assert!(!out.exists());
    Ok(())
}

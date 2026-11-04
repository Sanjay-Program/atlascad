//! The whole CAM job end to end: an STL part in, a program, setup sheet, stock preview and summary
//! out.

use std::process::Command;

type Failure = Box<dyn std::error::Error>;

fn atlas() -> Command {
    Command::new(env!("CARGO_BIN_EXE_atlas"))
}

#[test]
fn a_job_writes_every_artifact_for_a_part() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-job-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let part = directory.join("part.stl");
    let built = atlas()
        .args(["box", "--size", "20,20,12", "--out"])
        .arg(&part)
        .output()?;
    assert!(
        built.status.success(),
        "box failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );

    let output = atlas()
        .args(["cam", "job", "--size", "40,30,20", "--part"])
        .arg(&part)
        .args(["--out-dir"])
        .arg(&directory)
        .args([
            "--tool",
            "6",
            "--stepover",
            "4",
            "--stepdown",
            "2",
            "--stock-to-leave",
            "0.5",
            "--resolution",
            "2",
            "--format",
            "grbl",
            "--finish",
            "6",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "cam job failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let program = std::fs::read_to_string(directory.join("program.nc"))?;
    assert!(program.starts_with("(atlas cam job)\nG21\n"), "{program}");
    assert!(program.ends_with("M5\nG0 Z25\nM30\n"));
    assert!(program.contains("G1"), "the program has cutting moves");

    let sheet = std::fs::read_to_string(directory.join("setup.md"))?;
    assert!(sheet.contains("# Setup sheet — atlas cam job"), "{sheet}");
    assert!(sheet.contains("3-axis milling"));
    assert!(sheet.contains("40.0 x 30.0 x 20.0 mm"));
    assert!(sheet.contains("| 3D roughing | T1 |"));
    assert!(sheet.contains("| T1 | 6 mm flat end mill |"));

    let preview = std::fs::read_to_string(directory.join("stock-preview.txt"))?;
    assert!(preview.contains("removed"), "{preview}");
    assert!(preview.contains("deepest cut"));

    let pdf = std::fs::read(directory.join("setup.pdf"))?;
    assert!(pdf.starts_with(b"%PDF-1.4\n"), "setup.pdf is not a PDF");
    assert!(pdf.ends_with(b"%%EOF\n"));

    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("summary.json"))?)?;
    assert_eq!(summary["format"], "grbl");
    assert_eq!(summary["operations"][0], "3D roughing");
    assert_eq!(summary["operations"][1], "wall finishing");
    assert_eq!(summary["operations"][2], "3D finishing");
    assert!(summary["moves"].as_u64().unwrap_or(0) > 10);
    assert!(summary["wall_moves"].as_u64().unwrap_or(0) > 10);
    assert!(summary["finish_moves"].as_u64().unwrap_or(0) > 10);
    assert!(summary["removed_mm3"].as_f64().unwrap_or(0.0) > 18000.0);
    assert_eq!(summary["files"].as_array().map(Vec::len), Some(5));

    // Both tools are in one program, joined by a Grbl pause for the manual change, and the setup
    // sheet lists the wall pass with the flat tool and the surface pass with the ball nose.
    assert!(
        program.contains("(change tool to T2 then cycle start)\nM0\n"),
        "{program}"
    );
    assert!(
        program.contains("Z15 "),
        "the ball nose cuts 3 mm above the 12 mm top"
    );
    assert!(sheet.contains("| wall finishing | T1 |"), "{sheet}");
    assert!(sheet.contains("| 3D finishing | T2 |"), "{sheet}");
    assert!(sheet.contains("| T2 | 6 mm ball end mill |"), "{sheet}");

    // The same summary is on stdout, so a caller does not have to read the file back.
    let printed: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(printed["removed_mm3"], summary["removed_mm3"]);

    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_job_without_a_part_faces_the_block() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-face-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let output = atlas()
        .args(["cam", "job", "--size", "40,30,20", "--out-dir"])
        .arg(&directory)
        .args(["--stepdown", "2", "--resolution", "2"])
        .output()?;
    assert!(
        output.status.success(),
        "cam job failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let sheet = std::fs::read_to_string(directory.join("setup.md"))?;
    assert!(sheet.contains("| facing | T1 |"), "{sheet}");
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("summary.json"))?)?;
    assert_eq!(summary["operations"][0], "facing");
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_holder_that_enters_stock_refuses_the_job() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-holder-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    // A 19 mm island stays standing while the tool cuts the ring beside it down to the bottom skin,
    // so the holder meets the island long before the flutes do.
    let part = directory.join("island.stl");
    let built = atlas()
        .args(["box", "--size", "20,20,19", "--out"])
        .arg(&part)
        .output()?;
    assert!(
        built.status.success(),
        "box failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    let output = atlas()
        .args(["cam", "job", "--size", "40,30,20", "--part"])
        .arg(&part)
        .args([
            "--stepdown",
            "2",
            "--flute",
            "8",
            "--holder-diameter",
            "20",
            "--holder-length",
            "40",
            "--resolution",
            "2",
            "--out-dir",
        ])
        .arg(&directory)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !directory.join("program.nc").exists(),
        "a holder collision must not write a program"
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_holder_with_long_flutes_is_clear() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-clear-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let output = atlas()
        .args(["cam", "job", "--size", "40,30,20", "--out-dir"])
        .arg(&directory)
        .args([
            "--stepdown",
            "19",
            "--flute",
            "20",
            "--holder-diameter",
            "20",
            "--resolution",
            "2",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(directory.join("summary.json"))?)?;
    assert!(summary["holder"].as_str().unwrap_or("").contains("clear"));
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn an_unreadable_part_is_input_error() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-badpart-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let output = atlas()
        .args([
            "cam",
            "job",
            "--size",
            "40,30,20",
            "--part",
            "does-not-exist.stl",
            "--out-dir",
        ])
        .arg(&directory)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

const JOB_FILE: &str = r#"
{
  "schema": 1,
  "job": {
    "name": "job file bracket",
    "stock_mm": [40, 30, 20],
    "part": "part.stl",
    "format": "grbl",
    "tool": {
      "diameter_mm": 6,
      "stepover_mm": 4,
      "stepdown_mm": 2,
      "flute_length_mm": 20,
      "stock_to_leave_mm": 0.5,
      "holder": { "diameter_mm": 20, "length_mm": 40 }
    },
    "finish": { "diameter_mm": 6 },
    "resolution_mm": 2
  }
}
"#;

fn write_job(directory: &std::path::Path, job: &str) -> Result<std::path::PathBuf, Failure> {
    let path = directory.join("job.json");
    std::fs::write(&path, job)?;
    Ok(path)
}

fn a_part_beside_the_job(directory: &std::path::Path) -> Result<(), Failure> {
    let built = atlas()
        .args(["box", "--size", "20,20,12", "--out"])
        .arg(directory.join("part.stl"))
        .output()?;
    assert!(
        built.status.success(),
        "box failed: {}",
        String::from_utf8_lossy(&built.stderr)
    );
    Ok(())
}

#[test]
fn a_versioned_job_file_carries_every_input() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-file-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    a_part_beside_the_job(&directory)?;
    let job = write_job(&directory, JOB_FILE)?;
    let out = directory.join("out");
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(&out)
        .output()?;
    assert!(
        output.status.success(),
        "cam job failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let program = std::fs::read_to_string(out.join("program.nc"))?;
    assert!(
        program.contains("(change tool to T2 then cycle start)\nM0\n"),
        "{program}"
    );
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("summary.json"))?)?;
    assert_eq!(summary["program"], "job file bracket");
    assert_eq!(summary["format"], "grbl");
    assert_eq!(summary["operations"][0], "3D roughing");
    assert_eq!(summary["operations"][1], "wall finishing");
    assert_eq!(summary["operations"][2], "3D finishing");
    assert!(
        summary["job"].as_str().unwrap_or("").ends_with("job.json"),
        "the summary names the job file it ran: {summary}"
    );
    assert!(out.join("setup.pdf").exists());
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_job_takes_its_cutting_data_from_the_material() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-material-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    a_part_beside_the_job(&directory)?;
    let job = JOB_FILE.replace(
        "\"stock_mm\": [40, 30, 20],",
        "\"stock_mm\": [40, 30, 20],\n    \"material\": \"aluminium\",\n    \"flutes\": 3,",
    );
    assert!(job.contains("\"material\""), "the fixture was edited");
    let path = write_job(&directory, &job)?;
    let out = directory.join("out");
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&path)
        .args(["--out-dir"])
        .arg(&out)
        .output()?;
    assert!(
        output.status.success(),
        "cam job failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let program = std::fs::read_to_string(out.join("program.nc"))?;
    assert!(program.contains("S13262 M3"), "{program}");
    assert!(program.contains("F795"), "{program}");
    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("summary.json"))?)?;
    assert_eq!(summary["material"], "aluminium");
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_job_clears_rest_material_with_a_smaller_tool() -> Result<(), Failure> {
    use cad_cam::Stock;
    use cad_contract::mesh::Mesh;

    fn merge(a: &Mesh, b: &Mesh) -> Mesh {
        let mut positions = a.positions.clone();
        let offset = positions.len() as u32;
        positions.extend_from_slice(&b.positions);
        let mut triangles = a.triangles.clone();
        triangles.extend(b.triangles.iter().map(|triangle| {
            [
                triangle[0] + offset,
                triangle[1] + offset,
                triangle[2] + offset,
            ]
        }));
        Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        }
    }

    let directory = std::env::temp_dir().join(format!("atlas-cam-rest-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let base = Stock {
        min: [0.0, 5.0, 0.0],
        max: [20.0, 15.0, 5.0],
    };
    let left = Stock {
        min: [0.0, 5.0, 5.0],
        max: [8.5, 15.0, 15.0],
    };
    let right = Stock {
        min: [11.5, 5.0, 5.0],
        max: [20.0, 15.0, 15.0],
    };
    let part = merge(&merge(&base.to_mesh(), &left.to_mesh()), &right.to_mesh());
    let part_path = directory.join("channel.stl");
    cad_io::write_stl_binary(&part, &part_path)?;

    let out = directory.join("out");
    let output = atlas()
        .args(["cam", "job", "--size", "20,20,15", "--part"])
        .arg(&part_path)
        .args(["--out-dir"])
        .arg(&out)
        .args([
            "--tool",
            "6",
            "--stepover",
            "4",
            "--stepdown",
            "2",
            "--stock-to-leave",
            "0.5",
            "--resolution",
            "1",
            "--rest",
            "2",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "cam job failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("summary.json"))?)?;
    assert!(
        summary["rest_moves"].as_u64().unwrap_or(0) > 0,
        "the rest pass wrote no moves: {summary}"
    );
    let operations = summary["operations"]
        .as_array()
        .cloned()
        .unwrap_or_default();
    assert!(
        operations.iter().any(|name| name == "rest machining"),
        "the summary does not name the rest pass: {summary}"
    );
    let program = std::fs::read_to_string(out.join("program.nc"))?;
    assert!(
        program.contains("T3"),
        "the rest tool is not in the program"
    );
    let sheet = std::fs::read_to_string(out.join("setup.md"))?;
    assert!(
        sheet.contains("2 mm rest flat end mill"),
        "the setup sheet does not describe the rest tool: {sheet}"
    );

    // The same rest pass is reachable from a versioned job file.
    let job = directory.join("rest-job.json");
    std::fs::write(
        &job,
        r#"{
  "schema": 1,
  "job": {
    "name": "rest channel",
    "stock_mm": [20, 20, 15],
    "part": "channel.stl",
    "tool": { "diameter_mm": 6, "stepover_mm": 4, "stepdown_mm": 2, "stock_to_leave_mm": 0.5 },
    "rest": { "diameter_mm": 2 },
    "resolution_mm": 1
  }
}
"#,
    )?;
    let file_out = directory.join("file-out");
    let file_run = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(&file_out)
        .output()?;
    assert!(
        file_run.status.success(),
        "job-file rest failed: {}",
        String::from_utf8_lossy(&file_run.stderr)
    );
    let file_summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(file_out.join("summary.json"))?)?;
    assert!(
        file_summary["rest_moves"].as_u64().unwrap_or(0) > 0,
        "the job-file rest pass wrote no moves: {file_summary}"
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn rest_machining_needs_a_part() -> Result<(), Failure> {
    let directory =
        std::env::temp_dir().join(format!("atlas-cam-rest-nopart-{}", std::process::id()));
    let output = atlas()
        .args(["cam", "job", "--size", "20,20,15", "--out-dir"])
        .arg(&directory)
        .args(["--rest", "2"])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("--rest"), "stderr: {stderr}");
    Ok(())
}

#[test]
fn an_unknown_job_field_is_refused() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-unknown-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    a_part_beside_the_job(&directory)?;
    let job = write_job(
        &directory,
        &JOB_FILE.replace(
            "\"resolution_mm\": 2",
            "\"resolution_mm\": 2, \"rapid_feed\": 900",
        ),
    )?;
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(directory.join("out"))
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "an unknown field must be refused, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!directory.join("out/program.nc").exists());
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_newer_job_schema_is_refused() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-schema-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    a_part_beside_the_job(&directory)?;
    let job = write_job(
        &directory,
        &JOB_FILE.replace("\"schema\": 1", "\"schema\": 2"),
    )?;
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(directory.join("out"))
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("schema 2"),
        "the refusal names the schema: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_job_file_cannot_be_combined_with_input_flags() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-mixed-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    a_part_beside_the_job(&directory)?;
    let job = write_job(&directory, JOB_FILE)?;
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--tool", "6", "--out-dir"])
        .arg(directory.join("out"))
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("--tool"),
        "the refusal names the conflicting flag: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_job_file_with_a_missing_part_is_bad_input() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-nopart-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let job = write_job(&directory, JOB_FILE)?;
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(directory.join("out"))
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn a_recorded_program_is_gated_by_the_artifact_check() -> Result<(), Failure> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-record-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    a_part_beside_the_job(&directory)?;
    let job = write_job(&directory, JOB_FILE)?;
    let project = directory.join("bracket.zcad");
    let created = atlas()
        .args(["project", "new", "--file"])
        .arg(&project)
        .args(["--name", "bracket", "--size", "40,30,20"])
        .output()?;
    assert!(
        created.status.success(),
        "project new failed: {}",
        String::from_utf8_lossy(&created.stderr)
    );

    let out = directory.join("out");
    let output = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(&out)
        .args(["--project"])
        .arg(&project)
        .args(["--record", "7", "--label", "roughing and finishing"])
        .output()?;
    assert!(
        output.status.success(),
        "cam job failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let check = atlas()
        .args(["artifacts", "check", "--project"])
        .arg(&project)
        .output()?;
    assert_eq!(
        check.status.code(),
        Some(0),
        "stderr: {}",
        String::from_utf8_lossy(&check.stderr)
    );
    let report: serde_json::Value = serde_json::from_slice(&check.stdout)?;
    let artifact = report["artifacts"]
        .as_array()
        .ok_or("the release report has no artifacts")?
        .iter()
        .find(|artifact| artifact["id"].as_u64() == Some(7))
        .ok_or("the recorded toolpath is missing from the report")?;
    assert_eq!(artifact["releasable"], true, "{artifact}");
    assert_eq!(
        artifact["status"]["state"].as_str().unwrap_or(""),
        "current",
        "{artifact}"
    );

    let summary: serde_json::Value =
        serde_json::from_slice(&std::fs::read(out.join("summary.json"))?)?;
    assert!(
        summary["engine"]
            .as_str()
            .unwrap_or("")
            .starts_with("cad-cam "),
        "the summary names the engine that posted it: {summary}"
    );

    // Recording needs a project, and a project on its own is not a request to record.
    let without_project = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(directory.join("out2"))
        .args(["--record", "7"])
        .output()?;
    assert_eq!(
        without_project.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&without_project.stderr)
    );
    let without_record = atlas()
        .args(["cam", "job", "--job"])
        .arg(&job)
        .args(["--out-dir"])
        .arg(directory.join("out3"))
        .args(["--project"])
        .arg(&project)
        .output()?;
    assert_eq!(
        without_record.status.code(),
        Some(2),
        "stderr: {}",
        String::from_utf8_lossy(&without_record.stderr)
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

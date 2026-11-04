//! The CAM command end to end: a block size in, a validated G-code program on disk.

use std::process::Command;

#[test]
fn cam_gcode_writes_a_validated_program() -> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let out = directory.join("facing.nc");
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "cam",
            "gcode",
            "--size",
            "40,30,20",
            "--depth",
            "2",
            "--tool",
            "6",
            "--stepover",
            "4",
            "--format",
            "grbl",
            "--program",
            "fixture facing",
            "--out",
        ])
        .arg(&out)
        .output()?;
    assert!(
        output.status.success(),
        "cam gcode failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let program = std::fs::read_to_string(&out)?;
    assert!(
        program.starts_with("(fixture facing)\nG21\nG90\nG94\nG17\nG54\n"),
        "{program}"
    );
    assert!(program.contains("S12000 M3"));
    assert!(program.contains("G0 X-3 Y-3 Z25"));
    assert!(program.ends_with("M5\nG0 Z25\nM30\n"));

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(summary["format"], "grbl");
    assert_eq!(summary["moves"], 40);
    assert_eq!(summary["tool_mm"], 6.0);

    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

#[test]
fn an_impossible_depth_is_refused_without_writing_a_program()
-> Result<(), Box<dyn std::error::Error>> {
    let directory = std::env::temp_dir().join(format!("atlas-cam-bad-{}", std::process::id()));
    std::fs::create_dir_all(&directory)?;
    let out = directory.join("bad.nc");
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "cam", "gcode", "--size", "40,30,20", "--depth", "25", "--out",
        ])
        .arg(&out)
        .output()?;
    assert_eq!(
        output.status.code(),
        Some(3),
        "stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        !out.exists(),
        "a refused operation must not write a program"
    );
    std::fs::remove_dir_all(&directory)?;
    Ok(())
}

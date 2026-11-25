//! `atlas step-import` across a process boundary.
//!
//! The input is written by the platform's STEP writer and read back by the command, then the STL it
//! writes is read by the platform's STL reader: every assertion is on a file, not on the process
//! agreeing with itself. The malformed case checks the exit code a release pipeline branches on.

use cad_contract::mesh::Mesh;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("atlas-step-cli-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }

    fn path(&self) -> &std::path::Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn run(args: &[&str]) -> std::io::Result<Output> {
    Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(args)
        .output()
}

/// A 10 x 10 x 5 mm cube: 500 mm³, twelve triangles.
fn cube() -> Mesh {
    let positions = vec![
        [0.0, 0.0, 0.0],
        [10.0, 0.0, 0.0],
        [10.0, 10.0, 0.0],
        [0.0, 10.0, 0.0],
        [0.0, 0.0, 5.0],
        [10.0, 0.0, 5.0],
        [10.0, 10.0, 5.0],
        [0.0, 10.0, 5.0],
    ];
    let triangles = vec![
        [0, 2, 1],
        [0, 3, 2],
        [4, 5, 6],
        [4, 6, 7],
        [0, 1, 5],
        [0, 5, 4],
        [1, 2, 6],
        [1, 6, 5],
        [2, 3, 7],
        [2, 7, 6],
        [3, 0, 4],
        [3, 4, 7],
    ];
    Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    }
}

#[test]
fn a_step_file_is_read_and_written_back_as_measured_stl() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new()?;
    let source = dir.path().join("cube.stp");
    let stl = dir.path().join("cube.stl");
    std::fs::write(&source, cad_io::step_bytes(&cube(), "Cube")?)?;

    let output = run(&[
        "step-import",
        "--from",
        source.to_str().ok_or("source path")?,
        "--out",
        stl.to_str().ok_or("stl path")?,
    ])?;
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );

    let summary: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    assert_eq!(summary["name"], "Cube");
    assert_eq!(summary["source_unit"], "mm");
    assert_eq!(summary["closed"], true);
    assert_eq!(summary["triangles"], 12);
    assert_eq!(summary["skipped_faces"], 0);
    let volume = summary["volume_mm3"].as_f64().ok_or("volume")?;
    assert!((volume - 500.0).abs() < 1e-6, "volume was {volume}");
    assert_eq!(summary["bounds"]["max"][0], 10.0);
    assert_eq!(summary["bounds"]["max"][2], 5.0);
    assert_eq!(summary["stl"], stl.to_str().ok_or("stl path")?);

    // The written STL is the same solid, measured independently by the STL reader.
    let written = std::fs::read(&stl)?;
    let mesh = cad_io::read_stl_binary(&written)?;
    assert_eq!(mesh.triangle_count(), 12);
    let stl_volume = mesh.signed_volume().abs();
    assert!(
        (stl_volume - 500.0).abs() < 1e-3,
        "STL volume was {stl_volume}"
    );
    Ok(())
}

#[test]
fn a_file_that_is_not_step_exits_two_and_writes_nothing() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new()?;
    let source = dir.path().join("fake.stp");
    let stl = dir.path().join("should-not-exist.stl");
    std::fs::write(&source, b"this is not a STEP file")?;

    let output = run(&[
        "step-import",
        "--from",
        source.to_str().ok_or("source path")?,
        "--out",
        stl.to_str().ok_or("stl path")?,
    ])?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("signature"), "{stderr}");
    assert!(!stl.exists(), "a refused import wrote an STL");
    Ok(())
}

#[test]
fn a_step_file_without_geometry_exits_two() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let source = dir.path().join("empty.stp");
    std::fs::write(
        &source,
        b"ISO-10303-21;\nHEADER;\nENDSEC;\nDATA;\n#1=APPLICATION_CONTEXT('x');\nENDSEC;\nEND-ISO-10303-21;\n",
    )?;
    let output = run(&["step-import", "--from", source.to_str().ok_or("path")?])?;
    assert_eq!(output.status.code(), Some(2));
    assert!(String::from_utf8_lossy(&output.stderr).contains("no faceted face"));
    Ok(())
}

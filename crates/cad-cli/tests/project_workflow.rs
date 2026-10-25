use cad_cli::read_stl_binary;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("atlas-cad-cli-{}-{serial}", std::process::id()));
        std::fs::create_dir_all(&path)?;
        Ok(Self(path))
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn invoke(args: &[&str]) -> Result<Output, Box<dyn std::error::Error>> {
    let output = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(args)
        .output()?;
    if !output.status.success() {
        return Err(format!(
            "atlas {:?} failed: {}",
            args,
            String::from_utf8_lossy(&output.stderr)
        )
        .into());
    }
    Ok(output)
}

fn reported_volume(output: &Output) -> Result<f64, Box<dyn std::error::Error>> {
    let value: serde_json::Value = serde_json::from_slice(&output.stdout)?;
    value["volume_mm3"]
        .as_f64()
        .ok_or_else(|| "CLI response did not contain volume_mm3".into())
}

#[test]
fn cli_project_create_edit_undo_redo_reopen_and_export_work_across_processes()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("bracket.zcad");
    let stl = dir.0.join("bracket.stl");
    let project_arg = project.to_string_lossy().into_owned();
    let stl_arg = stl.to_string_lossy().into_owned();

    let new = invoke(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--name",
        "bracket",
        "--size",
        "10,10,5",
    ])?;
    assert!((reported_volume(&new)? - 500.0).abs() < 1e-6);

    let edit = invoke(&[
        "project",
        "set",
        "--file",
        &project_arg,
        "--param",
        "depth",
        "--value",
        "8",
    ])?;
    assert!((reported_volume(&edit)? - 800.0).abs() < 1e-6);

    let undo = invoke(&["project", "undo", "--file", &project_arg])?;
    assert!((reported_volume(&undo)? - 500.0).abs() < 1e-6);
    let redo = invoke(&["project", "redo", "--file", &project_arg])?;
    assert!((reported_volume(&redo)? - 800.0).abs() < 1e-6);

    invoke(&[
        "project",
        "export",
        "--file",
        &project_arg,
        "--out",
        &stl_arg,
    ])?;
    let bytes = std::fs::read(&stl)?;
    let mesh = read_stl_binary(&bytes)?;
    mesh.validate()?;
    assert!((mesh.signed_volume() - 800.0).abs() < 1e-6);
    Ok(())
}

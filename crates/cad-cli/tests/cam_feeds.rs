use std::path::PathBuf;
use std::process::Command;

type Failure = Box<dyn std::error::Error>;

fn atlas() -> PathBuf {
    PathBuf::from(env!("CARGO_BIN_EXE_atlas"))
}

#[derive(Debug, serde::Deserialize)]
struct FeedsReport {
    material: String,
    tool_mm: f64,
    flutes: u32,
    surface_speed_m_min: f64,
    spindle_rpm: f64,
    feed_mm_min: f64,
    plunge_feed_mm_min: f64,
    spindle_limited: bool,
    feed_limited: bool,
}

#[test]
fn feeds_come_from_the_material_and_tool() -> Result<(), Failure> {
    let output = Command::new(atlas())
        .args([
            "cam",
            "feeds",
            "--material",
            "aluminium",
            "--tool",
            "6",
            "--flutes",
            "3",
        ])
        .output()?;
    assert!(
        output.status.success(),
        "feeds failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let report: FeedsReport = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report.material, "aluminium");
    assert_eq!(report.tool_mm, 6.0);
    assert_eq!(report.flutes, 3);
    assert_eq!(report.spindle_rpm, 13_262.0);
    assert_eq!(report.feed_mm_min, 795.0);
    assert_eq!(report.plunge_feed_mm_min, 318.0);
    assert!((report.surface_speed_m_min - 250.0).abs() < 0.5);
    assert!(!report.spindle_limited);
    assert!(!report.feed_limited);
    Ok(())
}

#[test]
fn machine_limits_clamp_the_recommendation_visibly() -> Result<(), Failure> {
    let output = Command::new(atlas())
        .args([
            "cam",
            "feeds",
            "--material",
            "aluminium",
            "--tool",
            "6",
            "--flutes",
            "3",
            "--max-rpm",
            "8000",
            "--max-feed",
            "1000",
        ])
        .output()?;
    assert!(output.status.success());
    let report: FeedsReport = serde_json::from_slice(&output.stdout)?;
    assert_eq!(report.spindle_rpm, 8_000.0);
    assert_eq!(report.feed_mm_min, 480.0);
    assert!(report.spindle_limited);
    assert!(!report.feed_limited);
    Ok(())
}

#[test]
fn an_unknown_material_is_a_bad_input() -> Result<(), Failure> {
    let output = Command::new(atlas())
        .args(["cam", "feeds", "--material", "unobtainium", "--tool", "6"])
        .output()?;
    assert_eq!(output.status.code(), Some(2));
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("aluminium"), "stderr: {stderr}");
    assert!(stderr.contains("unobtainium"), "stderr: {stderr}");
    Ok(())
}

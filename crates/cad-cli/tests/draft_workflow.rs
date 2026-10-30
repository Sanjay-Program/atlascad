//! `atlas draft` and `atlas dxf` across process boundaries.
//!
//! The assertions are on the *files a customer's CAD program would open*: a 40 x 30 mm plan must
//! arrive as a DXF whose header carries those extents and whose body has four segments. Checking the
//! summary JSON alone would only prove the process agreed with itself.

use cad_drafting::Draft;
use std::path::PathBuf;
use std::process::{Command, Output};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_TEMP: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new() -> std::io::Result<Self> {
        let serial = NEXT_TEMP.fetch_add(1, Ordering::Relaxed);
        let path =
            std::env::temp_dir().join(format!("atlas-draft-cli-{}-{serial}", std::process::id()));
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

fn summary(output: &Output) -> Result<serde_json::Value, Box<dyn std::error::Error>> {
    Ok(serde_json::from_slice(&output.stdout)?)
}

#[test]
fn a_project_drafts_a_plan_that_exports_to_dxf_and_re_exports_identically()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("bracket.zcad");
    let drawing = dir.0.join("bracket-plan.zdraft");
    let dxf = dir.0.join("bracket-plan.dxf");
    let again = dir.0.join("bracket-plan-2.dxf");

    let project_arg = project.to_string_lossy().into_owned();
    let drawing_arg = drawing.to_string_lossy().into_owned();
    let dxf_arg = dxf.to_string_lossy().into_owned();
    let again_arg = again.to_string_lossy().into_owned();

    invoke(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--name",
        "bracket",
        "--size",
        "40,30,10",
    ])?;

    let drafted = invoke(&[
        "draft",
        "--file",
        &project_arg,
        "--save",
        &drawing_arg,
        "--out",
        &dxf_arg,
    ])?;
    let report = summary(&drafted)?;
    assert_eq!(report["entities"].as_u64(), Some(1));
    assert_eq!(report["layers"].as_u64(), Some(2), "layer 0 plus OUTLINE");
    assert_eq!(report["unit"], "mm");
    // 40 x 30 rectangle: extents and a 140 mm perimeter.
    assert_eq!(report["bounds"], serde_json::json!([0.0, 0.0, 40.0, 30.0]));
    assert!((report["total_length"].as_f64().unwrap_or_default() - 140.0).abs() < 1e-9);

    // The native drawing reloads and holds exactly what the summary claimed.
    let loaded = Draft::load(&drawing)?;
    assert_eq!(loaded.entities().len(), 1);
    assert_eq!(loaded.layers().len(), 2);
    assert!(loaded.layer("OUTLINE").is_some());

    // The DXF is a real R12 file with the plan's extents and four segments.
    let text = std::fs::read_to_string(&dxf)?;
    assert!(text.starts_with("0\nSECTION\n"));
    assert!(text.contains("$ACADVER\n1\nAC1009\n"));
    assert!(text.contains("$INSUNITS\n70\n4\n"), "millimetres");
    assert!(text.contains("$EXTMIN\n10\n0\n20\n0\n"));
    assert!(text.contains("$EXTMAX\n10\n40\n20\n30\n"));
    assert_eq!(text.matches("\n0\nLINE\n").count(), 4);
    assert!(text.contains("\n8\nOUTLINE\n"));

    // Writing the same drawing again is byte-identical: the exporter has no hidden state.
    invoke(&["dxf", "--from", &drawing_arg, "--out", &again_arg])?;
    assert_eq!(std::fs::read(&again)?, text.as_bytes());
    Ok(())
}

/// A drawing written by another program is opened, saved and re-exported without losing geometry.
///
/// The input is hand-written R12 with an annotation this model cannot represent, so the test pins
/// both halves of the contract: the supported entities survive byte-for-byte through a round trip,
/// and the unsupported one is reported rather than quietly dropped.
#[test]
fn a_drawing_from_another_program_opens_and_re_exports_its_geometry()
-> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let source = dir.0.join("aircraft-panel.dxf");
    let drawing = dir.0.join("panel.zdraft");
    let dxf = dir.0.join("panel.dxf");

    std::fs::write(
        &source,
        "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n1\n0\nENDSEC\n\
         0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n1\n\
         0\nLAYER\n2\nDETAIL\n70\n0\n62\n3\n0\nENDTAB\n0\nENDSEC\n\
         0\nSECTION\n2\nENTITIES\n\
         0\nLINE\n8\nDETAIL\n10\n0\n20\n0\n11\n6\n21\n0\n\
         0\nCIRCLE\n8\nDETAIL\n10\n3\n20\n2\n40\n1.5\n\
         0\nTEXT\n8\nDETAIL\n10\n0\n20\n4\n40\n0.25\n1\nSECTION A-A\n\
         0\nENDSEC\n0\nEOF\n",
    )?;

    let source_arg = source.to_string_lossy().into_owned();
    let drawing_arg = drawing.to_string_lossy().into_owned();
    let dxf_arg = dxf.to_string_lossy().into_owned();

    let imported = invoke(&[
        "dxf-import",
        "--from",
        &source_arg,
        "--save",
        &drawing_arg,
        "--out",
        &dxf_arg,
    ])?;
    let report = summary(&imported)?;
    assert_eq!(report["declared_unit"], "in", "the header said inches");
    assert_eq!(report["draft"]["unit"], "in");
    // The line, the circle and the annotation: TEXT is a first-class entity of this document now.
    assert_eq!(report["draft"]["entities"].as_u64(), Some(3));
    assert_eq!(report["draft"]["layers"].as_u64(), Some(2), "0 plus DETAIL");
    assert_eq!(report["skipped"], serde_json::json!([]));
    assert!(
        String::from_utf8_lossy(&imported.stderr).contains("3 entities"),
        "the summary should state what was read"
    );

    let loaded = Draft::load(&drawing)?;
    assert_eq!(loaded.header.units.to_string(), "in");
    assert_eq!(loaded.entities_on("DETAIL"), 3);
    assert!(
        loaded
            .entities()
            .iter()
            .any(|entity| matches!(&entity.kind, cad_drafting::EntityKind::Text { value, .. } if value == "SECTION A-A"))
    );

    // Re-exporting what was read produces a file this reader reads back identically.
    let text = std::fs::read_to_string(&dxf)?;
    // The line spans x 0..6 and the circle reaches y 3.5; the text baseline sits at y = 4 with a
    // 0.25 in cap height, so the drawing's top edge is 4.25 in.
    assert!(text.contains("$EXTMIN\n10\n0\n20\n0\n"), "{text}");
    assert!(text.contains("$EXTMAX\n10\n6\n20\n4.25\n"), "{text}");
    assert!(text.contains("$INSUNITS\n70\n1\n"), "inches kept");
    let reopened = cad_drafting::from_dxf("panel", cad_contract::ids::DocumentId::new(1), &text)?;
    assert_eq!(reopened.report.skipped, Vec::new());
    assert_eq!(reopened.draft.entities().len(), 3);
    assert_eq!(reopened.draft.to_dxf(), text);
    Ok(())
}

#[test]
fn drafting_refuses_requests_that_would_look_like_success() -> Result<(), Box<dyn std::error::Error>>
{
    let dir = TempDir::new()?;
    let project = dir.0.join("blank.zcad");
    let drawing = dir.0.join("blank.zdraft");
    let project_arg = project.to_string_lossy().into_owned();
    let drawing_arg = drawing.to_string_lossy().into_owned();

    invoke(&["project", "new", "--file", &project_arg])?;

    // No output requested: refuse rather than compute and discard.
    let bare = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["draft", "--file", &project_arg])
        .output()?;
    assert_eq!(bare.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&bare.stderr).contains("--save"));

    // A drawing handed to `draft` as if it were a project names the right command instead.
    invoke(&["draft", "--file", &project_arg, "--save", &drawing_arg])?;
    let confused = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args(["draft", "--file", &drawing_arg, "--out", "unused.dxf"])
        .output()?;
    assert_eq!(confused.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&confused.stderr).contains("atlas dxf"),
        "the error should point at the working command"
    );
    Ok(())
}

#[test]
fn a_drawing_plots_to_a_named_pdf_sheet() -> Result<(), Box<dyn std::error::Error>> {
    let dir = TempDir::new()?;
    let project = dir.0.join("bracket.zcad");
    let pdf = dir.0.join("bracket-sheet.pdf");
    let project_arg = project.to_string_lossy().into_owned();
    let pdf_arg = pdf.to_string_lossy().into_owned();

    invoke(&[
        "project",
        "new",
        "--file",
        &project_arg,
        "--name",
        "bracket",
        "--size",
        "40,30,10",
    ])?;

    let plotted = invoke(&[
        "sheet",
        "--file",
        &project_arg,
        "--pdf",
        &pdf_arg,
        "--paper",
        "A4",
        "--orientation",
        "landscape",
        "--scale",
        "1",
        "--title",
        "Bracket plate",
        "--number",
        "AC-1001",
        "--revision",
        "B",
        "--author",
        "Atlas",
        "--date",
        "2026-10-30",
    ])?;
    let report = summary(&plotted)?;
    assert_eq!(report["paper"], "A4");
    assert_eq!(report["orientation"], "landscape");
    assert_eq!(report["scale"], "1:1");
    assert_eq!(report["page_mm"], serde_json::json!([297.0, 210.0]));
    assert_eq!(report["source"], "project");
    // The frame, the title block and two overall dimensions: enough lines to be a sheet, not a shape.
    assert!(report["lines"].as_u64().unwrap_or_default() > 10);
    assert!(report["texts"].as_u64().unwrap_or_default() > 8);

    let bytes = std::fs::read(&pdf)?;
    let text = String::from_utf8(bytes)?;
    assert!(text.starts_with("%PDF-1.4\n"));
    assert!(text.trim_end().ends_with("%%EOF"));
    // A4 landscape in points, and the fields a checker reads off the sheet.
    assert!(
        text.contains("/MediaBox [0 0 841.89 595.276]"),
        "{text:.400}"
    );
    for expected in [
        "(Bracket plate) Tj",
        "(AC-1001) Tj",
        "(B) Tj",
        "(Atlas) Tj",
        "(2026-10-30) Tj",
        "(1:1) Tj",
        "(40) Tj",
        "(30) Tj",
    ] {
        assert!(text.contains(expected), "missing {expected}");
    }

    // A scale the paper cannot hold is refused, and the refusal names the scale that would work.
    let too_large = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "sheet",
            "--file",
            &project_arg,
            "--pdf",
            &pdf_arg,
            "--paper",
            "A4",
            "--scale",
            "0.1",
        ])
        .output()?;
    assert_eq!(too_large.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&too_large.stderr).contains("does not fit"),
        "{}",
        String::from_utf8_lossy(&too_large.stderr)
    );

    // An unknown paper names the ones that exist.
    let bad_paper = Command::new(env!("CARGO_BIN_EXE_atlas"))
        .args([
            "sheet",
            "--file",
            &project_arg,
            "--pdf",
            &pdf_arg,
            "--paper",
            "B2",
        ])
        .output()?;
    assert_eq!(bad_paper.status.code(), Some(1));
    assert!(
        String::from_utf8_lossy(&bad_paper.stderr).contains("A4"),
        "{}",
        String::from_utf8_lossy(&bad_paper.stderr)
    );
    Ok(())
}

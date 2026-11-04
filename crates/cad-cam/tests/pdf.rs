//! The PDF setup sheet: a real PDF file with a valid trailer and deterministic bytes.

use cad_cam::{
    CuttingCondition, Machine, MachineEnvelope, Operation, Setup, SetupSheet, Stock, Tool,
    ToolGeometry, ToolId, ToolpathId, WorkCoordinate, generate, setup_sheet_pdf,
};

type Failure = Box<dyn std::error::Error>;

fn sheet() -> Result<SetupSheet, Failure> {
    let stock = Stock::new([0.0, 0.0, 0.0], [40.0, 30.0, 20.0])?;
    let setup = Setup::new(
        Machine::Milling3Axis,
        MachineEnvelope::default_mill(),
        stock,
        WorkCoordinate::standard(),
        25.0,
    )?;
    let tool = Tool::new(
        ToolId::new(1).ok_or("tool ids start at one")?,
        "6 mm flat end mill",
        ToolGeometry::FlatEndMill {
            diameter_mm: 6.0,
            flute_length_mm: 20.0,
        },
        CuttingCondition::conservative_default(),
        2.0,
    )?;
    let path = generate(
        &Operation::Facing {
            depth_mm: 2.0,
            stepover_mm: 4.0,
        },
        &setup,
        &tool,
        ToolpathId::new(1).ok_or("toolpath ids start at one")?,
    )?;
    let mut sheet = SetupSheet::new("fixture job", &setup);
    sheet.add_operation("facing", &tool, &path);
    Ok(sheet)
}

#[test]
fn the_sheet_is_a_structurally_valid_pdf() -> Result<(), Failure> {
    let pdf = setup_sheet_pdf(&sheet()?);
    assert!(pdf.starts_with(b"%PDF-1.4\n"), "missing PDF header");
    assert!(pdf.ends_with(b"%%EOF\n"), "missing PDF trailer");

    let text = String::from_utf8_lossy(&pdf).into_owned();
    let marker = text
        .rfind("startxref\n")
        .ok_or("the PDF has no startxref")?;
    let offset: usize = text[marker + "startxref\n".len()..]
        .lines()
        .next()
        .ok_or("startxref has no offset")?
        .parse()?;
    assert_eq!(
        &pdf[offset..offset + 4],
        b"xref",
        "startxref must point at the cross-reference table"
    );
    assert!(text.contains("/Type /Catalog"));
    assert!(text.contains("/Type /Page "));
    assert!(text.contains("/BaseFont /Helvetica"));
    assert_eq!(text.matches("/Type /Page ").count(), 1);
    Ok(())
}

#[test]
fn the_sheet_text_names_the_setup_and_the_operation() -> Result<(), Failure> {
    let text = String::from_utf8_lossy(&setup_sheet_pdf(&sheet()?)).into_owned();
    assert!(text.contains("(Setup sheet - fixture job)"), "{text}");
    assert!(text.contains("Machine: 3-axis milling"));
    assert!(text.contains("Stock: 40.0 x 30.0 x 20.0 mm"));
    assert!(text.contains("T1"));
    assert!(text.contains("facing"));
    assert!(text.contains("Operations"));
    assert!(text.contains("Validate the program"));
    Ok(())
}

#[test]
fn the_same_sheet_renders_the_same_bytes() -> Result<(), Failure> {
    let first = setup_sheet_pdf(&sheet()?);
    let second = setup_sheet_pdf(&sheet()?);
    assert_eq!(first, second);
    Ok(())
}

#[test]
fn parenthesis_in_a_name_cannot_break_the_content_stream() -> Result<(), Failure> {
    let mut sheet = sheet()?;
    sheet.title = "job (rev B)".to_string();
    let text = String::from_utf8_lossy(&setup_sheet_pdf(&sheet)).into_owned();
    assert!(text.contains("(Setup sheet - job \\(rev B\\))"), "{text}");
    Ok(())
}

#[test]
fn a_long_sheet_paginates_rather_than_overflowing() -> Result<(), Failure> {
    let mut sheet = sheet()?;
    for index in 0..60 {
        sheet.operations.push(cad_cam::SheetOperation {
            name: format!("operation {index}"),
            tool_id: sheet.tools[0].id,
            moves: 10,
            cutting_mm: 1.0,
            rapid_mm: 1.0,
            estimated_minutes: 0.01,
            depth_mm: Some(1.0),
            bounds: ([0.0; 3], [1.0; 3]),
        });
    }
    let pdf = setup_sheet_pdf(&sheet);
    assert!(pdf.starts_with(b"%PDF-1.4\n"));
    let text = String::from_utf8_lossy(&pdf).into_owned();
    assert!(
        text.matches("/Type /Page ").count() >= 2,
        "a 60-operation sheet must paginate"
    );
    Ok(())
}

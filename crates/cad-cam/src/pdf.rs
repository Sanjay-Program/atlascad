//! A deterministic PDF setup sheet.
//!
//! The document is written directly: one page per 48 lines, Helvetica and Helvetica-Bold standard
//! fonts, uncompressed content streams, and no timestamps. The same sheet always produces the same
//! bytes, so a regression fixture can compare the whole file. The text is plain ASCII
//! (typographic characters are transliterated) because the standard fonts are not embedded.

use crate::sheet::SetupSheet;
use crate::tool_path_visitor::machine_name;

/// Page height and width in PostScript points (A4 portrait).
const PAGE_WIDTH: f64 = 595.0;
const PAGE_HEIGHT: f64 = 842.0;
/// Lines that fit between the title and the footer at the chosen leading.
const LINES_PER_PAGE: usize = 48;

/// Render the sheet as a PDF file.
pub fn setup_sheet_pdf(sheet: &SetupSheet) -> Vec<u8> {
    let body = sheet_lines(sheet);
    let title = format!("Setup sheet - {}", ascii(&sheet.title));
    let pages: Vec<&[String]> = if body.is_empty() {
        vec![&[]]
    } else {
        body.chunks(LINES_PER_PAGE).collect()
    };
    let page_count = pages.len();
    let font_regular = 3 + page_count * 2;
    let font_bold = font_regular + 1;

    let mut objects: Vec<String> = Vec::with_capacity(font_bold);
    objects.push("<</Type /Catalog /Pages 2 0 R>>".to_string());
    let kids = (0..page_count)
        .map(|index| format!("{} 0 R", 3 + index * 2))
        .collect::<Vec<_>>()
        .join(" ");
    objects.push(format!(
        "<</Type /Pages /Kids [{kids}] /Count {page_count}>>"
    ));
    for (index, lines) in pages.iter().enumerate() {
        let content_object = 4 + index * 2;
        objects.push(format!(
            "<</Type /Page /Parent 2 0 R /MediaBox [0 0 {PAGE_WIDTH} {PAGE_HEIGHT}] /Resources <</Font <</F1 {font_regular} 0 R /F2 {font_bold} 0 R>>>> /Contents {content_object} 0 R>>"
        ));
        let content = content_stream(&title, lines, index + 1, page_count);
        objects.push(format!(
            "<</Length {}>>\nstream\n{content}\nendstream",
            content.len()
        ));
    }
    objects.push(
        "<</Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding>>"
            .to_string(),
    );
    objects.push(
        "<</Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding>>"
            .to_string(),
    );

    let mut out = Vec::new();
    out.extend_from_slice(b"%PDF-1.4\n%\xE2\xE3\xCF\xD3\n");
    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", index + 1).as_bytes());
    }
    let xref_offset = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n", objects.len() + 1).as_bytes());
    out.extend_from_slice(b"0000000000 65535 f \n");
    for offset in &offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<</Size {} /Root 1 0 R>>\nstartxref\n{xref_offset}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// The sheet's text, one line per row.
fn sheet_lines(sheet: &SetupSheet) -> Vec<String> {
    let size = sheet.stock.size();
    let mut lines = vec![
        format!("Machine: {}", machine_name(sheet.machine)),
        format!("Stock: {:.1} x {:.1} x {:.1} mm", size[0], size[1], size[2]),
        format!("Stock volume: {:.0} mm3", size[0] * size[1] * size[2]),
        format!("Clearance plane: {:.1} mm", sheet.clearance_mm),
        format!(
            "WCS origin: ({:.1}, {:.1}, {:.1})",
            sheet.wcs_origin[0], sheet.wcs_origin[1], sheet.wcs_origin[2]
        ),
        format!(
            "WCS z axis: ({:.1}, {:.1}, {:.1})",
            sheet.wcs_z_axis[0], sheet.wcs_z_axis[1], sheet.wcs_z_axis[2]
        ),
        String::new(),
        "Tools".to_string(),
    ];
    if sheet.tools.is_empty() {
        lines.push("No tools in this setup.".to_string());
    }
    for tool in &sheet.tools {
        lines.push(format!(
            "T{}  {}  {}  D{:.1} mm  S{:.0} rpm  F{:.0} mm/min  plunge {:.0}  max DOC {:.1} mm",
            tool.id.get(),
            tool.name,
            tool.family,
            tool.diameter_mm,
            tool.spindle_rpm,
            tool.feed_mm_min,
            tool.plunge_feed_mm_min,
            tool.max_depth_of_cut_mm
        ));
    }
    lines.push(String::new());
    lines.push("Operations".to_string());
    if sheet.operations.is_empty() {
        lines.push("No operations in this job.".to_string());
    }
    for operation in &sheet.operations {
        let depth = match operation.depth_mm {
            Some(depth) => format!("{depth:.1} mm"),
            None => "from part".to_string(),
        };
        lines.push(format!(
            "{}  T{}  {} moves  {:.1} mm cut  {:.1} mm rapid  {:.2} min  depth {}",
            operation.name,
            operation.tool_id.get(),
            operation.moves,
            operation.cutting_mm,
            operation.rapid_mm,
            operation.estimated_minutes,
            depth
        ));
    }
    lines.push(String::new());
    lines.push(
        "Generated by AtlasCAD cad-cam. Validate the program before running it on a machine."
            .to_string(),
    );
    lines
}

/// One page of content: the title, the body lines, and a page footer.
fn content_stream(title: &str, lines: &[String], page: usize, pages: usize) -> String {
    let mut content = String::new();
    content.push_str(&format!(
        "BT /F2 16 Tf 50 800 Td ({}) Tj ET\n",
        escape(title)
    ));
    let mut y = 770.0;
    for line in lines {
        if line.is_empty() {
            y -= 8.0;
            continue;
        }
        content.push_str(&format!(
            "BT /F1 10 Tf 50 {y} Td ({}) Tj ET\n",
            escape(line)
        ));
        y -= 14.0;
    }
    content.push_str(&format!(
        "BT /F1 8 Tf 50 35 Td (page {page} of {pages}) Tj ET\n"
    ));
    content
}

/// Escape a string for a PDF literal string.
fn escape(text: &str) -> String {
    let mut out = String::new();
    for character in ascii(text).chars() {
        match character {
            '\\' | '(' | ')' => {
                out.push('\\');
                out.push(character);
            }
            other => out.push(other),
        }
    }
    out
}

/// Transliterate the handful of typographic characters the sheet uses.
fn ascii(text: &str) -> String {
    text.chars()
        .map(|character| match character {
            '—' | '–' => '-',
            '³' => '3',
            '×' => 'x',
            '·' => '-',
            '≥' => '>',
            '≤' => '<',
            other if other.is_ascii() => other,
            _ => '?',
        })
        .collect()
}

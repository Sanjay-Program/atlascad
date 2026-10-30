//! A minimal vector PDF writer for a [`Plot`].
//!
//! PDF 1.4, one page, the two base-14 Helvetica faces, and a content stream of vector paths and
//! text. Nothing is embedded and nothing is compressed, so a plotted sheet is a document a person
//! can open in any PDF reader and a program can inspect without a library.
//!
//! # Why the base-14 fonts
//!
//! Helvetica and Helvetica-Bold are among the fourteen fonts every PDF reader is required to
//! provide, so a sheet needs no font file, no subsetting and no licence question about embedding.
//! The cost is that text is not embedded, which matters for archival PDF/A — stated here rather than
//! discovered later.
//!
//! # Coordinates
//!
//! The [`Plot`] is in paper millimetres with the origin at the bottom-left and y upwards, which is
//! also PDF's convention. The only conversion is millimetres to points (72/25.4), done once in
//! [`mm_to_pt`]. No matrix is used for the page, so a wrong number is a wrong number on the page and
//! not a silently transformed one.

use crate::sheet::{Plot, TextAlign};

/// The PDF version this writer emits.
pub const PDF_VERSION: &str = "1.4";

/// Millimetres to PDF points.
pub fn mm_to_pt(mm: f64) -> f64 {
    mm * 72.0 / 25.4
}

/// Serialise a plot as a complete PDF document.
///
/// `title` is written into the document information dictionary, which is what a reader shows in its
/// window title and what a print queue displays. It is not drawn on the page; the sheet's title block
/// carries the visible title.
pub fn to_pdf(plot: &Plot, title: &str) -> String {
    let content = content_stream(plot);
    let (width, height) = plot.page_mm;

    let mut objects: Vec<String> = Vec::with_capacity(7);
    objects.push("<< /Type /Catalog /Pages 2 0 R >>".to_string());
    objects.push("<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string());
    objects.push(format!(
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 {} {}] /Resources << /Font << /F1 5 0 R /F2 6 0 R >> >> /Contents 4 0 R >>",
        number(mm_to_pt(width)),
        number(mm_to_pt(height))
    ));
    objects.push(format!(
        "<< /Length {} >>\nstream\n{}\nendstream",
        content.len(),
        content
    ));
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica /Encoding /WinAnsiEncoding >>"
            .to_string(),
    );
    objects.push(
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica-Bold /Encoding /WinAnsiEncoding >>"
            .to_string(),
    );
    objects.push(format!(
        "<< /Title ({}) /Producer (Atlas CAD) >>",
        escape_text(title)
    ));

    let mut out = String::with_capacity(content.len() + 2048);
    out.push_str("%PDF-");
    out.push_str(PDF_VERSION);
    out.push('\n');

    let mut offsets = Vec::with_capacity(objects.len());
    for (index, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.push_str(&format!("{} 0 obj\n{}\nendobj\n", index + 1, body));
    }

    let xref_at = out.len();
    out.push_str(&format!("xref\n0 {}\n", objects.len() + 1));
    out.push_str("0000000000 65535 f \n");
    for offset in &offsets {
        out.push_str(&format!("{offset:010} 00000 n \n"));
    }
    out.push_str(&format!(
        "trailer\n<< /Size {} /Root 1 0 R /Info 7 0 R >>\nstartxref\n{}\n%%EOF\n",
        objects.len() + 1,
        xref_at
    ));
    out
}

/// The page content stream: one `w` state per line weight group, then paths and text.
fn content_stream(plot: &Plot) -> String {
    let mut content = String::new();
    content.push_str("q\n");
    let mut width: Option<f64> = None;

    for line in &plot.lines {
        if line.points.len() < 2 {
            continue;
        }
        if width != Some(line.width_mm) {
            content.push_str(&format!("{} w\n", number(mm_to_pt(line.width_mm))));
            width = Some(line.width_mm);
        }
        content.push_str(&format!(
            "{} {} m\n",
            number(mm_to_pt(line.points[0][0])),
            number(mm_to_pt(line.points[0][1]))
        ));
        for point in &line.points[1..] {
            content.push_str(&format!(
                "{} {} l\n",
                number(mm_to_pt(point[0])),
                number(mm_to_pt(point[1]))
            ));
        }
        if line.closed {
            content.push_str("h\n");
        }
        content.push_str("S\n");
    }

    content.push_str("0 J 0 j\n");
    for text in &plot.texts {
        let size = mm_to_pt(text.height_mm);
        let x = mm_to_pt(text.at[0]);
        let y = mm_to_pt(text.at[1]);
        let width_pt = helvetica_width_pt(&text.value, size);
        let x = match text.align {
            TextAlign::Left => x,
            TextAlign::Center => x - width_pt / 2.0,
            TextAlign::Right => x - width_pt,
        };
        let face = if text.bold { "/F2" } else { "/F1" };
        content.push_str(&format!(
            "BT {face} {} Tf 1 0 0 1 {} {} Tm ({}) Tj ET\n",
            number(size),
            number(x),
            number(y),
            escape_text(&text.value)
        ));
    }
    content.push_str("Q\n");
    content
}

/// Escape a string for a PDF literal string: the two delimiters and the escape character.
///
/// Characters outside WinAnsi's byte range are replaced with `?`, and bytes above 127 are written as
/// three-digit octal escapes so the stream stays ASCII. A `?` in a title block is visible and
/// therefore fixable; a byte the font cannot draw would be invisible and look like a missing field.
fn escape_text(value: &str) -> String {
    let mut out = String::with_capacity(value.len() + 8);
    for character in value.chars() {
        match character {
            '\\' => out.push_str("\\\\"),
            '(' => out.push_str("\\("),
            ')' => out.push_str("\\)"),
            ' '..='~' => out.push(character),
            other => {
                let code = other as u32;
                // WinAnsi covers Latin-1, which holds the degree sign, middle dot and multiplication
                // sign a drawing actually uses. Anything above that cannot be drawn by the base-14
                // font and becomes a visible `?` rather than a silent byte the reader ignores.
                if code <= 0xFF {
                    out.push_str(&format!("\\{code:03o}"));
                } else {
                    out.push('?');
                }
            }
        }
    }
    out
}

/// Helvetica advance width, in points, from the standard AFM widths.
///
/// Only used to anchor centred and right-aligned text. An approximation would put a dimension value
/// a little to one side of its line, which reads as sloppy rather than wrong, so the table covers the
/// printable ASCII range and falls back to an average for anything else.
pub fn helvetica_width_pt(value: &str, size_pt: f64) -> f64 {
    let units: u32 = value.chars().map(glyph_width).sum();
    f64::from(units) * size_pt / 1000.0
}

fn glyph_width(character: char) -> u32 {
    match character {
        ' ' => 278,
        '!' => 278,
        '"' => 355,
        '#' => 556,
        '$' => 556,
        '%' => 889,
        '&' => 667,
        '\'' => 191,
        '(' | ')' => 333,
        '*' => 389,
        '+' => 584,
        ',' => 278,
        '-' => 333,
        '.' => 278,
        '/' => 278,
        '0'..='9' => 556,
        ':' | ';' => 278,
        '<' | '=' | '>' => 584,
        '?' => 556,
        '@' => 1015,
        'A' => 667,
        'B' => 667,
        'C' => 722,
        'D' => 722,
        'E' => 667,
        'F' => 611,
        'G' => 778,
        'H' => 722,
        'I' => 278,
        'J' => 500,
        'K' => 667,
        'L' => 556,
        'M' => 833,
        'N' => 722,
        'O' => 778,
        'P' => 667,
        'Q' => 778,
        'R' => 722,
        'S' => 667,
        'T' => 611,
        'U' => 722,
        'V' => 667,
        'W' => 944,
        'X' => 667,
        'Y' => 667,
        'Z' => 611,
        '[' => 278,
        '\\' => 278,
        ']' => 278,
        '^' => 469,
        '_' => 556,
        '`' => 333,
        'a' => 556,
        'b' => 556,
        'c' => 500,
        'd' => 556,
        'e' => 556,
        'f' => 278,
        'g' => 556,
        'h' => 556,
        'i' => 222,
        'j' => 222,
        'k' => 500,
        'l' => 222,
        'm' => 833,
        'n' => 556,
        'o' => 556,
        'p' => 556,
        'q' => 556,
        'r' => 333,
        's' => 500,
        't' => 278,
        'u' => 556,
        'v' => 500,
        'w' => 722,
        'x' => 500,
        'y' => 500,
        'z' => 500,
        '{' | '|' | '}' => 334,
        '~' => 584,
        // WinAnsi punctuation that appears in drawing text.
        '°' => 400,
        '·' => 278,
        '×' => 584,
        _ => 556,
    }
}

/// A number as PDF writes it: up to three decimals, no trailing zeros, never `-0`.
fn number(value: f64) -> String {
    let value = if value.abs() < 5e-4 { 0.0 } else { value };
    let mut text = format!("{value:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    if text.is_empty() || text == "-0" {
        "0".into()
    } else {
        text
    }
}

#[cfg(test)]
mod tests {
    use super::{escape_text, helvetica_width_pt, mm_to_pt, to_pdf};
    use crate::sheet::{Orientation, PaperSize, Plot, PlotLine, PlotText, Sheet, TextAlign};
    use crate::{Draft, EntityKind};
    use cad_contract::ids::DocumentId;
    use cad_contract::units::Unit;

    fn plot() -> Plot {
        let mut draft = Draft::new("plate", DocumentId::new(1), Unit::Millimeter);
        draft
            .add_entity(
                "0",
                EntityKind::Line {
                    start: [0.0, 0.0],
                    end: [40.0, 0.0],
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        Sheet::new("plate (left) & right")
            .plot(&draft)
            .unwrap_or_else(|error| unreachable!("{error}"))
    }

    #[test]
    fn millimetres_convert_to_points_exactly() {
        assert!((mm_to_pt(25.4) - 72.0).abs() < 1e-9);
        assert!((mm_to_pt(210.0) - 595.275_590_551_181_1).abs() < 1e-9);
    }

    #[test]
    fn the_document_has_the_objects_a_reader_requires() {
        let pdf = to_pdf(&plot(), "plate");
        assert!(pdf.starts_with("%PDF-1.4\n"), "{}", &pdf[..20]);
        assert!(pdf.ends_with("%%EOF\n"));
        for object in [
            "/Type /Catalog",
            "/Type /Pages",
            "/Type /Page",
            "/BaseFont /Helvetica",
            "/BaseFont /Helvetica-Bold",
        ] {
            assert!(pdf.contains(object), "missing {object}");
        }
        assert!(
            pdf.contains("/MediaBox [0 0 1190.551 841.89]"),
            "A3 landscape media box"
        );
    }

    #[test]
    fn the_xref_offsets_point_at_their_objects() {
        let pdf = to_pdf(&plot(), "plate");
        let start = pdf
            .rfind("startxref")
            .unwrap_or_else(|| unreachable!("startxref"));
        let tail = &pdf[start + "startxref".len()..];
        let offset: usize = tail
            .trim()
            .lines()
            .next()
            .unwrap_or_else(|| unreachable!("offset"))
            .trim()
            .parse()
            .unwrap_or_else(|_| unreachable!("numeric offset"));
        assert!(
            pdf[offset..].starts_with("xref\n"),
            "startxref must land on xref"
        );
        // Each `n` entry of the table points at `<number> 0 obj`.
        let table = &pdf[offset..];
        let mut lines = table.lines().skip(3);
        for number in 1..=7 {
            let entry = lines
                .next()
                .unwrap_or_else(|| unreachable!("xref entry {number}"));
            let raw = &entry[..10];
            let at: usize = raw.parse().unwrap_or_else(|_| unreachable!("offset parse"));
            assert!(
                pdf[at..].starts_with(&format!("{number} 0 obj")),
                "entry {number} points at {:?}",
                &pdf[at..at + 12.min(pdf.len() - at)]
            );
        }
    }

    #[test]
    fn text_is_escaped_and_anchored_by_its_measured_width() {
        assert_eq!(escape_text("a (b) \\ c"), "a \\(b\\) \\\\ c");
        assert_eq!(escape_text("40 × 30"), "40 \\327 30");
        assert_eq!(escape_text("—"), "?");
        // Helvetica's own widths: a 10 pt capital A is 6.67 pt wide.
        assert!((helvetica_width_pt("A", 10.0) - 6.67).abs() < 1e-9);
        assert!((helvetica_width_pt("", 10.0)).abs() < 1e-9);

        let plot = plot();
        let pdf = to_pdf(&plot, "plate");
        assert!(
            pdf.contains("(plate \\(left\\) & right) Tj"),
            "the title survives escaping"
        );
    }

    #[test]
    fn every_primitive_reaches_the_content_stream() {
        let mut plot = plot();
        let before = plot.clone();
        plot.lines.push(PlotLine {
            points: vec![[10.0, 10.0], [20.0, 20.0]],
            width_mm: 0.5,
            closed: true,
        });
        plot.texts.push(PlotText {
            at: [50.0, 50.0],
            height_mm: 3.0,
            value: "HELLO".into(),
            align: TextAlign::Center,
            bold: true,
        });
        let pdf = to_pdf(&plot, "doc");
        // Every primitive line ends in `S`; text does not, and the page frame, title block and both
        // overall dimensions are lines like any other.
        assert_eq!(pdf.matches("\nS\n").count(), before.lines.len() + 1);
        assert!(pdf.contains("(HELLO) Tj"));
        assert!(
            pdf.contains("/F2 8.504 Tf"),
            "3 mm is 8.5 pt in the bold face"
        );
        assert!(pdf.contains("h\nS\n"), "a closed line is closed with h");
    }

    #[test]
    fn an_empty_page_is_still_a_document() {
        let plot = Plot {
            page_mm: (210.0, 297.0),
            lines: Vec::new(),
            texts: Vec::new(),
            scale: 1.0,
        };
        let pdf = to_pdf(&plot, "empty");
        assert!(pdf.starts_with("%PDF-1.4"));
        assert!(pdf.contains("/MediaBox [0 0 595.276 841.89]"));
        assert!(pdf.ends_with("%%EOF\n"));
    }

    #[test]
    fn a_letter_page_uses_its_own_size() {
        let mut sheet = Sheet::new("letter");
        sheet.paper = PaperSize::Letter;
        sheet.orientation = Orientation::Portrait;
        let mut draft = Draft::new("l", DocumentId::new(1), Unit::Millimeter);
        draft
            .add_entity(
                "0",
                EntityKind::Line {
                    start: [0.0, 0.0],
                    end: [10.0, 10.0],
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let plot = sheet
            .plot(&draft)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let pdf = to_pdf(&plot, "l");
        assert!(
            pdf.contains("/MediaBox [0 0 612 792]"),
            "8.5 x 11 in in points"
        );
    }
}

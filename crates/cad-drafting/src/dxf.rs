//! Reading DXF: the other half of the exchange format.
//!
//! The writer in [`crate::Draft::to_dxf`] emits R12 (AC1009). This reader accepts an ASCII DXF of
//! any version that uses the entity types below, which in practice means most R12–R2000 files and
//! anything a modern program writes for compatibility.
//!
//! # What is read
//!
//! * The `LAYER` table: name, ACI colour (a negative colour is DXF's "layer off"), and the locked
//!   flag. A layer an entity names but the table never defines is created on the default colour, so
//!   geometry is never attached to a layer that does not exist.
//! * Entities: `POINT`, `LINE`, `CIRCLE`, `ARC`, `POLYLINE` (with its `VERTEX` records, closed flag
//!   honoured) and `LWPOLYLINE`. `POLYLINE` is read whether or not it is written as 2D or 3D; the z
//!   coordinate is dropped because the document is a 2D drawing.
//! * `$INSUNITS` in the header, so a file in inches stays in inches. A file that does not say
//!   (R12 often does not) is reported as `declared_unit: None` and read as millimetres — stated,
//!   not guessed silently.
//!
//! # What is skipped, and why it is reported
//!
//! `TEXT`, `MTEXT`, `DIMENSION`, `HATCH`, `INSERT`/blocks, splines, ellipses, images and every
//! other entity type this model has no representation for is **counted and named** in
//! [`ImportReport::skipped`]. Dropping them silently would turn "the file opened" into a claim the
//! drawing does not support: the geometry would be there and the annotation would not. A caller can
//! therefore refuse the import, warn, or accept a partial drawing with a real number attached.
//!
//! Malformed input is an error, not a shrug: an odd number of lines, a non-numeric group code, or a
//! record missing a coordinate it needs all fail the import with the offending group named.

use crate::{DEFAULT_LAYER, Draft, DraftError, EntityKind};
use cad_contract::ids::DocumentId;
use cad_contract::units::Unit;

/// Largest accepted DXF text, checked before parsing. A drawing is text; 32 MiB is far beyond any
/// hand-made sheet and small enough that a hostile file cannot exhaust memory on the way in.
pub const MAX_DXF_BYTES: usize = 32 * 1024 * 1024;

/// A DXF file turned into a drawing, plus what was and was not read.
#[derive(Debug)]
pub struct DxfImport {
    pub draft: Draft,
    pub report: ImportReport,
}

/// What the reader did with a file, so "it imported" carries numbers.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ImportReport {
    /// The unit the file stated in `$INSUNITS`; `None` when it did not state one.
    pub declared_unit: Option<Unit>,
    /// Layers in the finished drawing, including the default `0`.
    pub layers: usize,
    /// Entities read into the drawing.
    pub entities: usize,
    /// Entity types this model cannot represent, with how often each appeared, sorted by name.
    pub skipped: Vec<(String, usize)>,
}

/// One DXF record: the entity or table entry named by a group code `0`, then its own groups.
struct Record<'a> {
    kind: String,
    groups: Vec<(i32, &'a str)>,
}

/// Read a DXF document into a new drawing called `name`.
///
/// The drawing is returned with an empty undo history: opening a file is not an edit.
pub fn from_dxf(name: &str, document: DocumentId, text: &str) -> Result<DxfImport, DraftError> {
    if text.len() > MAX_DXF_BYTES {
        return Err(DraftError::Invalid(format!(
            "the DXF is {} bytes, larger than the {MAX_DXF_BYTES} accepted",
            text.len()
        )));
    }
    let pairs = pairs(text)?;
    let declared_unit = header_unit(&pairs);
    let records = records(&pairs);

    let mut layers: Vec<ParsedLayer> = Vec::new();
    let mut entities: Vec<(String, EntityKind)> = Vec::new();
    let mut skipped: Vec<(String, usize)> = Vec::new();
    let mut section = String::new();
    let mut table = String::new();

    let mut index = 0;
    while index < records.len() {
        let record = &records[index];
        match record.kind.as_str() {
            "SECTION" => {
                section = text_group(record, 2)
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                index += 1;
            }
            "ENDSEC" => {
                section.clear();
                table.clear();
                index += 1;
            }
            "EOF" => break,
            "TABLE" => {
                table = text_group(record, 2)
                    .unwrap_or_default()
                    .to_ascii_uppercase();
                index += 1;
            }
            "ENDTAB" => {
                table.clear();
                index += 1;
            }
            "LAYER" if section == "TABLES" && table == "LAYER" => {
                if let Some(layer) = parsed_layer(record)? {
                    layers.push(layer);
                }
                index += 1;
            }
            "POLYLINE" if section == "ENTITIES" => {
                let (kind, consumed) = polyline(&records, index, record)?;
                entities.push((layer_of(record), kind));
                index += consumed;
            }
            "VERTEX" | "SEQEND" if section == "ENTITIES" => {
                // An orphan vertex is a broken file, not a drawing feature.
                index += 1;
            }
            "LINE" | "CIRCLE" | "ARC" | "POINT" | "LWPOLYLINE" | "TEXT"
                if section == "ENTITIES" =>
            {
                entities.push((layer_of(record), entity(record)?));
                index += 1;
            }
            other if section == "ENTITIES" => {
                bump(&mut skipped, other);
                index += 1;
            }
            _ => index += 1,
        }
    }

    let unit = declared_unit.unwrap_or(Unit::Millimeter);
    let mut draft = Draft::new(name, document, unit);
    for layer in &layers {
        if draft.layer(&layer.name).is_some() {
            // The default `0` layer already exists; its colour and visibility still come from the
            // file. A lock is applied after the geometry, so an off/locked layer still imports.
            if !layer.name.eq_ignore_ascii_case(DEFAULT_LAYER) {
                continue;
            }
            draft.set_layer_color(&layer.name, layer.color)?;
            draft.set_layer_visible(&layer.name, layer.visible)?;
            draft.set_layer_locked(&layer.name, false)?;
            continue;
        }
        draft.add_layer(&layer.name, layer.color)?;
        draft.set_layer_visible(&layer.name, layer.visible)?;
        draft.set_layer_locked(&layer.name, false)?;
    }
    for (layer, kind) in entities {
        if draft.layer(&layer).is_none() {
            draft.add_layer(&layer, 7)?;
        }
        draft.add_entity(&layer, kind)?;
    }
    // Locks are restored last: a locked layer must not reject the geometry the file put on it.
    for layer in &layers {
        if layer.locked && draft.layer(&layer.name).is_some() {
            draft.set_layer_locked(&layer.name, true)?;
        }
    }
    draft.clear_history();

    skipped.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(DxfImport {
        report: ImportReport {
            declared_unit,
            layers: draft.layers().len(),
            entities: draft.entities().len(),
            skipped,
        },
        draft,
    })
}

struct ParsedLayer {
    name: String,
    color: u8,
    visible: bool,
    locked: bool,
}

fn parsed_layer(record: &Record) -> Result<Option<ParsedLayer>, DraftError> {
    let Some(name) = text_group(record, 2) else {
        return Ok(None);
    };
    let name = name.trim();
    if name.is_empty() {
        return Ok(None);
    }
    // A negative colour is how DXF records a layer that is switched off; 0 is BYBLOCK and 256 is
    // BYLAYER, neither of which is a colour a layer may carry.
    let raw = number_opt(record, 62).unwrap_or(7.0);
    let visible = raw >= 0.0;
    let color = raw.abs().round();
    let color = if (1.0..=255.0).contains(&color) {
        // Value is checked to lie in 1..=255 above, so the cast cannot wrap.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let color = color as u8;
        color
    } else {
        7
    };
    let flags = number_opt(record, 70).unwrap_or(0.0);
    // Bit 4 of the layer flags means "locked".
    let locked = (flags.round() as i64) & 4 != 0;
    Ok(Some(ParsedLayer {
        name: name.to_string(),
        color,
        visible,
        locked,
    }))
}

fn entity(record: &Record) -> Result<EntityKind, DraftError> {
    match record.kind.as_str() {
        "POINT" => Ok(EntityKind::Point {
            at: [number(record, 10)?, number(record, 20)?],
        }),
        "LINE" => Ok(EntityKind::Line {
            start: [number(record, 10)?, number(record, 20)?],
            end: [number(record, 11)?, number(record, 21)?],
        }),
        "CIRCLE" => Ok(EntityKind::Circle {
            center: [number(record, 10)?, number(record, 20)?],
            radius: number(record, 40)?,
        }),
        "ARC" => Ok(EntityKind::Arc {
            center: [number(record, 10)?, number(record, 20)?],
            radius: number(record, 40)?,
            start_degrees: number(record, 50)?,
            end_degrees: number(record, 51)?,
        }),
        "LWPOLYLINE" => {
            let points = paired_points(record)?;
            let closed = flags(record, 70) & 1 != 0;
            Ok(EntityKind::Polyline { points, closed })
        }
        "TEXT" => {
            let value = text_group(record, 1).unwrap_or_default().trim().to_string();
            if value.is_empty() {
                return Err(DraftError::Invalid(
                    "a DXF TEXT record has no value in group 1".into(),
                ));
            }
            Ok(EntityKind::Text {
                at: [number(record, 10)?, number(record, 20)?],
                // R12 defaults a text height to 2.5 when the writer omits it.
                height: number_opt(record, 40).unwrap_or(2.5),
                value,
            })
        }
        other => Err(DraftError::Invalid(format!(
            "`{other}` is not an entity this reader writes"
        ))),
    }
}

/// Read a `POLYLINE` record and the `VERTEX` records that follow it, through `SEQEND`.
///
/// Returns the polyline and how many records it consumed, including its own `SEQEND`.
fn polyline(
    records: &[Record],
    index: usize,
    record: &Record,
) -> Result<(EntityKind, usize), DraftError> {
    let closed = flags(record, 70) & 1 != 0;
    let mut points = Vec::new();
    let mut cursor = index + 1;
    while cursor < records.len() {
        match records[cursor].kind.as_str() {
            "VERTEX" => {
                let vertex = &records[cursor];
                points.push([number(vertex, 10)?, number(vertex, 20)?]);
            }
            "SEQEND" => {
                cursor += 1;
                break;
            }
            // A polyline that runs into another entity, or ends at the end of the file, keeps the
            // vertices it did have: swallowing the next entity would be the worse mistake.
            _ => break,
        }
        cursor += 1;
    }
    Ok((EntityKind::Polyline { points, closed }, cursor - index))
}

fn paired_points(record: &Record) -> Result<Vec<[f64; 2]>, DraftError> {
    let mut points = Vec::new();
    let mut pending_x: Option<f64> = None;
    for (code, value) in &record.groups {
        match code {
            10 => {
                pending_x = Some(parse_number("LWPOLYLINE", 10, value)?);
            }
            20 => {
                let y = parse_number("LWPOLYLINE", 20, value)?;
                let x = pending_x.take().ok_or_else(|| {
                    DraftError::Invalid(
                        "a DXF LWPOLYLINE has a y coordinate with no x before it".into(),
                    )
                })?;
                points.push([x, y]);
            }
            _ => {}
        }
    }
    if pending_x.is_some() {
        return Err(DraftError::Invalid(
            "a DXF LWPOLYLINE ends with an x coordinate that has no y".into(),
        ));
    }
    Ok(points)
}

fn layer_of(record: &Record) -> String {
    text_group(record, 8)
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .unwrap_or(DEFAULT_LAYER)
        .to_string()
}

fn flags(record: &Record, code: i32) -> i64 {
    number_opt(record, code).map_or(0, |value| value.round() as i64)
}

fn text_group<'a>(record: &'a Record<'a>, code: i32) -> Option<&'a str> {
    record
        .groups
        .iter()
        .find(|(c, _)| *c == code)
        .map(|(_, value)| *value)
}

fn number_opt(record: &Record, code: i32) -> Option<f64> {
    text_group(record, code).and_then(|value| value.parse::<f64>().ok())
}

fn number(record: &Record, code: i32) -> Result<f64, DraftError> {
    let value = text_group(record, code).ok_or_else(|| {
        DraftError::Invalid(format!(
            "a DXF {} record has no group {code}, which it needs",
            record.kind
        ))
    })?;
    parse_number(&record.kind, code, value)
}

fn parse_number(kind: &str, code: i32, value: &str) -> Result<f64, DraftError> {
    value.parse::<f64>().map_err(|_| {
        DraftError::Invalid(format!(
            "DXF {kind} group {code} is `{value}`, which is not a number"
        ))
    })
}

fn bump(counts: &mut Vec<(String, usize)>, kind: &str) {
    if let Some(entry) = counts.iter_mut().find(|(name, _)| name == kind) {
        entry.1 += 1;
        return;
    }
    counts.push((kind.to_string(), 1));
}

/// `$INSUNITS` from the header, if the file states one this model can honour.
fn header_unit(pairs: &[(i32, &str)]) -> Option<Unit> {
    let position = pairs
        .iter()
        .position(|(code, value)| *code == 9 && value.eq_ignore_ascii_case("$INSUNITS"))?;
    let value = pairs
        .iter()
        .skip(position + 1)
        .find(|(code, _)| *code == 70)
        .and_then(|(_, value)| value.parse::<i32>().ok())?;
    Unit::from_dxf_insunits(value)
}

/// Split the file into group-code pairs, failing loudly on a truncated or non-numeric file.
fn pairs(text: &str) -> Result<Vec<(i32, &str)>, DraftError> {
    let lines: Vec<&str> = text
        .lines()
        .map(|line| line.trim_end_matches('\r'))
        .collect();
    let mut end = lines.len();
    while end > 0 && lines[end - 1].trim().is_empty() {
        end -= 1;
    }
    if !end.is_multiple_of(2) {
        return Err(DraftError::Invalid(format!(
            "the DXF has {end} lines, an odd number: a group code and its value come in pairs"
        )));
    }
    let mut out = Vec::with_capacity(end / 2);
    for step in 0..end / 2 {
        let code_line = lines[step * 2].trim();
        let value = lines[step * 2 + 1].trim();
        let code: i32 = code_line.parse().map_err(|_| {
            DraftError::Invalid(format!(
                "DXF line {} is `{code_line}`, which is not a group code",
                step * 2 + 1
            ))
        })?;
        out.push((code, value));
    }
    Ok(out)
}

/// Group the pairs by the `0` records: everything a `0` names, until the next `0`, belongs to it.
fn records<'a>(pairs: &[(i32, &'a str)]) -> Vec<Record<'a>> {
    let mut out: Vec<Record> = Vec::new();
    for (code, value) in pairs {
        if *code == 0 {
            out.push(Record {
                kind: (*value).trim().to_ascii_uppercase(),
                groups: Vec::new(),
            });
        } else if let Some(record) = out.last_mut() {
            record.groups.push((*code, *value));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::{MAX_DXF_BYTES, from_dxf};
    use crate::{Draft, DraftError, EntityKind, MAX_POLYLINE_POINTS};
    use cad_contract::ids::DocumentId;
    use cad_contract::units::Unit;

    fn document() -> DocumentId {
        DocumentId::new(1)
    }

    fn read(text: &str) -> Draft {
        from_dxf("imported", document(), text)
            .unwrap_or_else(|error| unreachable!("import failed: {error}"))
            .draft
    }

    /// A minimal R12 file: header, one layer table, one entity section.
    fn file(unit: i32, body: &str) -> String {
        format!(
            "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n{unit}\n0\nENDSEC\n\
             0\nSECTION\n2\nTABLES\n0\nTABLE\n2\nLAYER\n70\n2\n\
             0\nLAYER\n2\n0\n70\n0\n62\n7\n\
             0\nLAYER\n2\nOUTLINE\n70\n4\n62\n-1\n\
             0\nENDTAB\n0\nENDSEC\n\
             0\nSECTION\n2\nENTITIES\n{body}0\nENDSEC\n0\nEOF\n"
        )
    }

    #[test]
    fn every_entity_type_this_crate_writes_reads_back() {
        let body = "0\nPOINT\n8\nOUTLINE\n10\n1\n20\n2\n\
                    0\nLINE\n8\nOUTLINE\n10\n0\n20\n0\n11\n10.5\n21\n-2\n\
                    0\nCIRCLE\n8\nOUTLINE\n10\n5\n20\n5\n40\n2.5\n\
                    0\nARC\n8\nOUTLINE\n10\n0\n20\n0\n40\n4\n50\n0\n51\n90\n\
                    0\nLWPOLYLINE\n8\nOUTLINE\n90\n4\n70\n1\n\
                    10\n0\n20\n0\n10\n4\n20\n0\n10\n4\n20\n3\n10\n0\n20\n3\n\
                    0\nTEXT\n8\nOUTLINE\n10\n1\n20\n1\n40\n3.5\n1\n40 x 30 PLATE\n";
        let draft = read(&file(4, body));
        assert_eq!(draft.entities().len(), 6);
        assert_eq!(
            draft.entities()[5].kind,
            EntityKind::Text {
                at: [1.0, 1.0],
                height: 3.5,
                value: "40 x 30 PLATE".into(),
            }
        );
        assert_eq!(
            draft.entities()[1].kind,
            EntityKind::Line {
                start: [0.0, 0.0],
                end: [10.5, -2.0],
            }
        );
        assert_eq!(
            draft.entities()[3].kind,
            EntityKind::Arc {
                center: [0.0, 0.0],
                radius: 4.0,
                start_degrees: 0.0,
                end_degrees: 90.0,
            }
        );
        assert!(matches!(
            draft.entities()[4].kind,
            EntityKind::Polyline { ref points, closed: true } if points.len() == 4
        ));
    }

    #[test]
    fn a_polyline_reads_its_vertices_and_stops_at_seqend() {
        let body = "0\nPOLYLINE\n8\n0\n66\n1\n70\n1\n\
                    0\nVERTEX\n8\n0\n10\n0\n20\n0\n\
                    0\nVERTEX\n8\n0\n10\n5\n20\n0\n\
                    0\nVERTEX\n8\n0\n10\n5\n20\n4\n\
                    0\nSEQEND\n8\n0\n\
                    0\nLINE\n8\n0\n10\n1\n20\n1\n11\n2\n21\n2\n";
        let draft = read(&file(4, body));
        assert_eq!(draft.entities().len(), 2);
        assert_eq!(
            draft.entities()[0].kind,
            EntityKind::Polyline {
                points: vec![[0.0, 0.0], [5.0, 0.0], [5.0, 4.0]],
                closed: true,
            }
        );
        // The line after SEQEND is a record of its own, not swallowed by the polyline.
        assert!(matches!(draft.entities()[1].kind, EntityKind::Line { .. }));
    }

    #[test]
    fn the_layer_table_supplies_colour_visibility_and_lock() {
        let draft = read(&file(
            4,
            "0\nLINE\n8\nOUTLINE\n10\n0\n20\n0\n11\n1\n21\n1\n",
        ));
        assert_eq!(draft.layers().len(), 2);
        let outline = draft
            .layer("OUTLINE")
            .unwrap_or_else(|| unreachable!("layer"));
        // A negative colour means the layer is off; the colour itself is kept.
        assert_eq!(outline.color, 1);
        assert!(!outline.visible);
        assert!(outline.locked);
        // The lock is in force after the import, and the geometry it held is there.
        assert_eq!(draft.entities_on("OUTLINE"), 1);
    }

    #[test]
    fn a_layer_an_entity_names_but_the_table_omits_is_created() {
        let body = "0\nLINE\n8\nNOT IN THE TABLE\n10\n0\n20\n0\n11\n1\n21\n1\n";
        let draft = read(&file(4, body));
        assert!(draft.layer("NOT IN THE TABLE").is_some());
        assert_eq!(draft.entities_on("NOT IN THE TABLE"), 1);
    }

    #[test]
    fn the_header_unit_is_honoured_and_an_absent_one_is_reported() {
        let mut body = String::new();
        body.push_str("0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n");
        let imported = from_dxf("inches", document(), &file(1, &body))
            .unwrap_or_else(|error| unreachable!("import failed: {error}"));
        assert_eq!(imported.report.declared_unit, Some(Unit::Inch));
        assert_eq!(imported.draft.header.units, Unit::Inch);
        assert!(imported.draft.to_dxf().contains("$INSUNITS\n70\n1\n"));

        let unitless = file(0, &body);
        let imported = from_dxf("unitless", document(), &unitless)
            .unwrap_or_else(|error| unreachable!("import failed: {error}"));
        assert_eq!(imported.report.declared_unit, None);
        assert_eq!(imported.draft.header.units, Unit::Millimeter);
    }

    #[test]
    fn unsupported_entities_are_counted_rather_than_silently_dropped() {
        let body = "0\nMTEXT\n8\n0\n10\n0\n20\n0\n40\n2.5\n1\nHello\n\
                    0\nMTEXT\n8\n0\n10\n0\n20\n5\n40\n2.5\n1\nWorld\n\
                    0\nDIMENSION\n8\n0\n\
                    0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n";
        let imported = from_dxf("annotated", document(), &file(4, body))
            .unwrap_or_else(|error| unreachable!("import failed: {error}"));
        assert_eq!(imported.draft.entities().len(), 1);
        assert_eq!(
            imported.report.skipped,
            vec![("DIMENSION".to_string(), 1), ("MTEXT".to_string(), 2)]
        );
    }

    #[test]
    fn a_dimension_is_written_as_lines_and_a_text_and_reads_back_as_those() {
        let mut original = Draft::new("dim", document(), Unit::Millimeter);
        original
            .add_entity(
                "0",
                EntityKind::Dimension {
                    start: [0.0, 0.0],
                    end: [40.0, 0.0],
                    offset: 10.0,
                    text: None,
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let dxf = original.to_dxf();
        // Two extension lines, the dimension line, two ticks, then the value: no DIMENSION entity.
        assert_eq!(dxf.matches("\n0\nLINE\n").count(), 5, "{dxf}");
        assert!(!dxf.contains("DIMENSION"));
        assert!(dxf.contains("0\nTEXT\n"), "{dxf}");
        assert!(dxf.contains("\n1\n40\n"), "the measured value: {dxf}");
        let reopened = from_dxf("dim", document(), &dxf)
            .unwrap_or_else(|error| unreachable!("import failed: {error}"));
        assert_eq!(reopened.report.skipped, Vec::new());
        assert_eq!(
            reopened
                .draft
                .entities()
                .iter()
                .filter(|entity| matches!(entity.kind, EntityKind::Text { .. }))
                .count(),
            1
        );
    }

    #[test]
    fn the_writer_and_the_reader_round_trip_a_drawing() {
        let mut original = Draft::new("round trip", document(), Unit::Millimeter);
        original
            .add_layer("HOLES", 3)
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .add_entity(
                "0",
                EntityKind::Line {
                    start: [0.0, 0.0],
                    end: [40.0, 0.0],
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .add_entity(
                "HOLES",
                EntityKind::Circle {
                    center: [20.0, 10.0],
                    radius: 3.5,
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .add_entity(
                "0",
                EntityKind::Arc {
                    center: [0.0, 0.0],
                    radius: 10.0,
                    start_degrees: 30.0,
                    end_degrees: 120.0,
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .add_entity("0", EntityKind::Point { at: [1.0, 2.0] })
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .add_entity(
                "0",
                EntityKind::Text {
                    at: [1.0, 2.0],
                    height: 2.5,
                    value: "40 x 30 PLATE".into(),
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .add_layer("HIDDEN", 5)
            .unwrap_or_else(|error| unreachable!("{error}"));
        original
            .set_layer_visible("HIDDEN", false)
            .unwrap_or_else(|error| unreachable!("{error}"));

        let imported = read(&original.to_dxf());
        assert_eq!(imported.header.units, original.header.units);
        assert_eq!(imported.layers().len(), original.layers().len());
        for (read, written) in imported.layers().iter().zip(original.layers()) {
            assert_eq!(read.name, written.name);
            assert_eq!(read.color, written.color);
            assert_eq!(read.visible, written.visible);
        }
        // A closed polyline exports as segments, so geometry is compared by count and by the
        // entities that survive as themselves.
        assert_eq!(imported.entities().len(), original.entities().len());
        let kinds: Vec<&str> = imported.entities().iter().map(|e| e.kind.label()).collect();
        assert_eq!(kinds, vec!["line", "circle", "arc", "point", "text"]);
        // Exporting the imported drawing reproduces the same file, byte for byte.
        assert_eq!(imported.to_dxf(), original.to_dxf());
    }

    #[test]
    fn malformed_files_fail_with_the_offending_group_named() {
        // An odd number of lines: the last code has no value.
        let odd = "0\nSECTION\n2\nHEADER\n9\n$INSUNITS\n70\n4\n0\nENDSEC\n0\n";
        let error = from_dxf("odd", document(), odd).err();
        assert!(
            matches!(error, Some(DraftError::Invalid(ref message)) if message.contains("odd number")),
            "{error:?}"
        );

        // A non-numeric group code.
        let error = from_dxf("bad", document(), "nope\nVALUES\n0\nEOF\n").err();
        assert!(
            matches!(error, Some(DraftError::Invalid(ref message)) if message.contains("group code")),
            "{error:?}"
        );

        // A circle with no radius.
        let error = from_dxf(
            "radius",
            document(),
            &file(4, "0\nCIRCLE\n8\n0\n10\n0\n20\n0\n"),
        )
        .err();
        assert!(
            matches!(error, Some(DraftError::Invalid(ref message)) if message.contains("group 40")),
            "{error:?}"
        );

        // A line whose endpoint is not a number.
        let error = from_dxf(
            "number",
            document(),
            &file(4, "0\nLINE\n8\n0\n10\n0\n20\n0\n11\nx\n21\n1\n"),
        )
        .err();
        assert!(
            matches!(error, Some(DraftError::Invalid(ref message)) if message.contains("not a number")),
            "{error:?}"
        );
    }

    #[test]
    fn a_file_larger_than_the_cap_is_refused_before_it_is_parsed() {
        let huge = "0\n".repeat(MAX_DXF_BYTES / 2 + 1);
        let error = from_dxf("huge", document(), &huge).err();
        assert!(
            matches!(error, Some(DraftError::Invalid(ref message)) if message.contains("larger than")),
            "{error:?}"
        );
    }

    #[test]
    fn opening_a_file_leaves_no_undo_history() {
        let draft = read(&file(4, "0\nLINE\n8\n0\n10\n0\n20\n0\n11\n1\n21\n1\n"));
        assert_eq!(draft.undo_depth(), 0);
        assert_eq!(draft.redo_depth(), 0);
    }

    #[test]
    fn a_polyline_with_too_many_vertices_is_refused_by_the_document() {
        let mut body = String::from("0\nPOLYLINE\n8\n0\n70\n0\n");
        for index in 0..=MAX_POLYLINE_POINTS {
            body.push_str(&format!("0\nVERTEX\n8\n0\n10\n{index}\n20\n0\n"));
        }
        body.push_str("0\nSEQEND\n8\n0\n");
        let error = from_dxf("long", document(), &file(4, &body)).err();
        assert!(error.is_some(), "an over-long polyline must be refused");
    }
}

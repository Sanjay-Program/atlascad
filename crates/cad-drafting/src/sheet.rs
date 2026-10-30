//! A drawing sheet: paper, scale, frame, title block and the plot primitives a renderer draws.
//!
//! A [`Draft`] is a collection of geometry. A [`Sheet`] is the page it is printed on: paper size,
//! orientation, scale, frame and title block. [`Sheet::plot`] turns the two into a [`Plot`], which is
//! a flat list of lines and texts in **paper millimetres** — the only units a plotter needs.
//!
//! Keeping the layout separate from the PDF writer is deliberate: the scale arithmetic, the fit
//! search and the title block can all be tested analytically, and the PDF module only has to be
//! correct about one thing (serialisation).
//!
//! # What a plot contains
//!
//! * The frame, at the sheet margin.
//! * The title block, with the drawing's name, number, revision, author, date, scale, unit and paper.
//! * Every entity on a **visible** layer, at the sheet scale, placed in the plot area. A hidden layer
//!   is off, so it is not plotted; the geometry is still in the drawing.
//! * Optional overall dimensions (the drawing's width and height), drawn in paper space so the text
//!   stays legible at any scale.
//!
//! Everything is plotted black with the layer's line weight. Drawings are plotted monochrome on
//! purpose: a colour plot of a mechanical drawing is not what a shop floor wants.

use crate::{Draft, EntityKind, Layer};
use std::fmt;

/// Standard paper sizes, in millimetres, portrait.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PaperSize {
    A4,
    A3,
    A2,
    A1,
    A0,
    Letter,
}

impl PaperSize {
    /// Portrait width and height, in millimetres.
    pub const fn millimeters(self) -> (f64, f64) {
        match self {
            PaperSize::A4 => (210.0, 297.0),
            PaperSize::A3 => (297.0, 420.0),
            PaperSize::A2 => (420.0, 594.0),
            PaperSize::A1 => (594.0, 841.0),
            PaperSize::A0 => (841.0, 1189.0),
            PaperSize::Letter => (215.9, 279.4),
        }
    }

    /// The label printed in the title block and used by the CLI and the API.
    pub const fn label(self) -> &'static str {
        match self {
            PaperSize::A4 => "A4",
            PaperSize::A3 => "A3",
            PaperSize::A2 => "A2",
            PaperSize::A1 => "A1",
            PaperSize::A0 => "A0",
            PaperSize::Letter => "Letter",
        }
    }

    /// Parse a paper name, case-insensitively. `letter`, `us-letter` and `ansi-a` are accepted.
    pub fn parse(name: &str) -> Option<PaperSize> {
        match name.trim().to_ascii_lowercase().as_str() {
            "a4" => Some(PaperSize::A4),
            "a3" => Some(PaperSize::A3),
            "a2" => Some(PaperSize::A2),
            "a1" => Some(PaperSize::A1),
            "a0" => Some(PaperSize::A0),
            "letter" | "us-letter" | "ansi-a" => Some(PaperSize::Letter),
            _ => None,
        }
    }
}

/// Which way the paper is turned.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Orientation {
    Portrait,
    Landscape,
}

impl Orientation {
    pub fn parse(name: &str) -> Option<Orientation> {
        match name.trim().to_ascii_lowercase().as_str() {
            "portrait" => Some(Orientation::Portrait),
            "landscape" => Some(Orientation::Landscape),
            _ => None,
        }
    }

    /// Apply the orientation to a portrait page size.
    pub const fn page(self, portrait: (f64, f64)) -> (f64, f64) {
        match self {
            Orientation::Portrait => portrait,
            Orientation::Landscape => (portrait.1, portrait.0),
        }
    }
}

/// Why a plot cannot be produced.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum PlotError {
    #[error("the drawing is empty, so there is nothing to plot")]
    Empty,
    #[error(
        "the requested scale 1:{requested} does not fit {paper}; the smallest standard scale that does is 1:{needed}"
    )]
    DoesNotFit {
        requested: String,
        needed: String,
        paper: &'static str,
    },
    #[error("{0}")]
    Invalid(String),
}

/// Standard scales as model-millimetres per paper-millimetre: 2:1, 1:1, 1:2, 1:2.5, 1:5, 1:10, …
///
/// A plot picks from these rather than inventing an arbitrary ratio, because a drawing whose scale
/// is "1:3.47" cannot be measured with a rule and would be rejected by any checker.
pub const STANDARD_SCALES: [f64; 13] = [
    0.2, 0.5, 1.0, 2.0, 2.5, 5.0, 10.0, 20.0, 25.0, 50.0, 100.0, 200.0, 500.0,
];

/// Largest accepted field length in the title block, in characters.
pub const MAX_SHEET_TEXT: usize = 120;

/// How a text primitive is anchored horizontally.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TextAlign {
    Left,
    Center,
    Right,
}

/// One polyline in paper space: millimetres, origin at the bottom-left of the page, y up.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotLine {
    pub points: Vec<[f64; 2]>,
    pub width_mm: f64,
    pub closed: bool,
}

/// One text run in paper space. `height_mm` is the font size, not the box.
#[derive(Debug, Clone, PartialEq)]
pub struct PlotText {
    pub at: [f64; 2],
    pub height_mm: f64,
    pub value: String,
    pub align: TextAlign,
    /// `true` for title-block labels, which are set bold so they read as labels.
    pub bold: bool,
}

/// A page ready to be serialised: page size and the primitives on it, in paper millimetres.
#[derive(Debug, Clone, PartialEq)]
pub struct Plot {
    pub page_mm: (f64, f64),
    pub lines: Vec<PlotLine>,
    pub texts: Vec<PlotText>,
    /// The scale actually used, in model units per paper millimetre.
    pub scale: f64,
}

impl Plot {
    /// The scale label a checker reads off the sheet: `1:2`, `2:1`, `1:1`.
    pub fn scale_label(&self) -> String {
        scale_label(self.scale)
    }

    /// Every string drawn on the page, in order. Used by tests to assert the title block.
    pub fn text_values(&self) -> Vec<&str> {
        self.texts.iter().map(|text| text.value.as_str()).collect()
    }
}

/// A printable sheet: paper, scale and the fields of its title block.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Sheet {
    pub paper: PaperSize,
    pub orientation: Orientation,
    /// Model units per paper millimetre. `Some(2.0)` means 1:2. `None` fits a standard scale to the
    /// plot area.
    pub scale: Option<f64>,
    /// Frame margin from the paper edge, in millimetres.
    pub margin_mm: f64,
    pub title: String,
    pub number: String,
    pub revision: String,
    pub author: String,
    pub date: String,
    pub notes: String,
    /// Draw the drawing's overall width and height.
    pub dimensions: bool,
}

impl Sheet {
    /// An A3 landscape sheet at 1:1, titled after the drawing.
    pub fn new(title: impl Into<String>) -> Self {
        Sheet {
            paper: PaperSize::A3,
            orientation: Orientation::Landscape,
            scale: Some(1.0),
            margin_mm: 10.0,
            title: title.into(),
            number: String::new(),
            revision: String::new(),
            author: String::new(),
            date: String::new(),
            notes: String::new(),
            dimensions: true,
        }
    }

    /// Paper width and height after orientation, in millimetres.
    pub fn page_mm(&self) -> (f64, f64) {
        self.orientation.page(self.paper.millimeters())
    }

    /// Height of the title block, in millimetres.
    pub const fn title_block_height_mm() -> f64 {
        28.0
    }

    /// The rectangle the drawing may occupy: `[x, y, width, height]` in paper millimetres, inside
    /// the frame and above the title block, with a little breathing room.
    pub fn plot_area(&self) -> [f64; 4] {
        let (page_w, page_h) = self.page_mm();
        let margin = self.margin_mm;
        let pad = 6.0;
        let left = margin + pad;
        let bottom = margin + Sheet::title_block_height_mm() + pad;
        let width = page_w - 2.0 * (margin + pad);
        let height = page_h - 2.0 * margin - Sheet::title_block_height_mm() - 2.0 * pad;
        [left, bottom, width.max(1.0), height.max(1.0)]
    }

    /// Validate the sheet's fields before anything is plotted.
    pub fn validate(&self) -> Result<(), PlotError> {
        if !self.margin_mm.is_finite() || self.margin_mm < 2.0 || self.margin_mm > 60.0 {
            return Err(PlotError::Invalid(
                "the sheet margin must be between 2 and 60 mm".into(),
            ));
        }
        if let Some(scale) = self.scale
            && (!scale.is_finite() || scale <= 0.0 || scale > 5000.0)
        {
            return Err(PlotError::Invalid(
                "the sheet scale must be a positive number of model units per paper millimetre"
                    .into(),
            ));
        }
        for (label, value) in [
            ("title", &self.title),
            ("number", &self.number),
            ("revision", &self.revision),
            ("author", &self.author),
            ("date", &self.date),
            ("notes", &self.notes),
        ] {
            if value.chars().count() > MAX_SHEET_TEXT {
                return Err(PlotError::Invalid(format!(
                    "the {label} accepts at most {MAX_SHEET_TEXT} characters"
                )));
            }
            if value.contains(['\n', '\r']) {
                return Err(PlotError::Invalid(format!(
                    "the {label} must be a single line"
                )));
            }
        }
        Ok(())
    }

    /// Lay the drawing out on the page.
    ///
    /// The scale is the one requested when it fits; when it does not, the error names the smallest
    /// standard scale that does, so the caller can say "this needs 1:5" instead of "does not fit".
    /// With `scale: None` a standard scale is chosen automatically.
    pub fn plot(&self, draft: &Draft) -> Result<Plot, PlotError> {
        self.validate()?;
        let bounds = draft.bounds().ok_or(PlotError::Empty)?;
        let [min_x, min_y, max_x, max_y] = bounds;
        let width = (max_x - min_x).max(0.0);
        let height = (max_y - min_y).max(0.0);
        let [area_x, area_y, area_w, area_h] = self.plot_area();

        let fits = |scale: f64| width / scale <= area_w && height / scale <= area_h;
        let scale = match self.scale {
            Some(requested) => {
                if !fits(requested) {
                    let needed = STANDARD_SCALES
                        .iter()
                        .copied()
                        .find(|candidate| *candidate >= requested && fits(*candidate))
                        .ok_or_else(|| {
                            PlotError::Invalid(format!(
                                "the drawing is {} x {} {} and does not fit {} at any standard scale",
                                number(width),
                                number(height),
                                draft.header.units.symbol(),
                                self.paper.label()
                            ))
                        })?;
                    return Err(PlotError::DoesNotFit {
                        requested: number(requested),
                        needed: number(needed),
                        paper: self.paper.label(),
                    });
                }
                requested
            }
            // Auto-fit never *enlarges*: a part is plotted at 1:1 unless it has to be reduced, and a
            // user who wants 2:1 or 5:1 asks for it. The range starts at 1.0 so the first fit found
            // is the least reduction that works.
            None => STANDARD_SCALES
                .iter()
                .copied()
                .filter(|candidate| *candidate >= 1.0)
                .find(|candidate| fits(*candidate))
                .ok_or_else(|| {
                    PlotError::Invalid(format!(
                        "the drawing does not fit {} at any standard scale",
                        self.paper.label()
                    ))
                })?,
        };

        // Centre the drawing in the plot area.
        let used_w = width / scale;
        let used_h = height / scale;
        let origin_x = area_x + (area_w - used_w) / 2.0;
        let origin_y = area_y + (area_h - used_h) / 2.0;
        let to_paper = |point: [f64; 2]| {
            [
                origin_x + (point[0] - min_x) / scale,
                origin_y + (point[1] - min_y) / scale,
            ]
        };

        let mut plot = Plot {
            page_mm: self.page_mm(),
            lines: Vec::new(),
            texts: Vec::new(),
            scale,
        };
        self.frame(&mut plot);

        for entity in draft.entities() {
            let layer = draft.layer(&entity.layer);
            if layer.is_some_and(|layer| !layer.visible) {
                continue;
            }
            let width_mm = layer.map_or(0.25, |layer| layer.line_weight_mm);
            if let Some(line) = entity_line(&entity.kind, &to_paper, width_mm) {
                plot.lines.push(line);
            }
            match &entity.kind {
                EntityKind::Text { at, height, value } => plot.texts.push(PlotText {
                    at: to_paper(*at),
                    height_mm: (height / scale).max(1.2),
                    value: value.clone(),
                    align: TextAlign::Left,
                    bold: false,
                }),
                EntityKind::Dimension {
                    start,
                    end,
                    offset,
                    text,
                } => dimension(&mut plot, *start, *end, *offset, text.as_deref(), &to_paper),
                _ => {}
            }
        }

        if self.dimensions {
            // 9 mm of paper between the geometry and its overall dimensions, converted to model
            // units so the offset is the same on an A4 at 1:5 and an A0 at 1:1.
            let overshoot = 9.0 * scale;
            dimension(
                &mut plot,
                [min_x, min_y],
                [max_x, min_y],
                -overshoot,
                None,
                &to_paper,
            );
            dimension(
                &mut plot,
                [min_x, min_y],
                [min_x, max_y],
                overshoot,
                None,
                &to_paper,
            );
        }

        self.title_block(&mut plot, draft, scale);
        Ok(plot)
    }

    /// The page frame.
    fn frame(&self, plot: &mut Plot) {
        let (page_w, page_h) = plot.page_mm;
        let margin = self.margin_mm;
        plot.lines.push(PlotLine {
            points: vec![
                [margin, margin],
                [page_w - margin, margin],
                [page_w - margin, page_h - margin],
                [margin, page_h - margin],
            ],
            width_mm: 0.5,
            closed: true,
        });
    }

    /// The title block: a box across the bottom of the frame, with the fields a checker looks for.
    fn title_block(&self, plot: &mut Plot, draft: &Draft, scale: f64) {
        let (page_w, _page_h) = plot.page_mm;
        let margin = self.margin_mm;
        let height = Sheet::title_block_height_mm();
        let left = margin;
        let right = page_w - margin;
        let bottom = margin;
        let top = margin + height;
        let width = right - left;
        let mid = bottom + 16.0;

        for (points, width_mm) in [
            (
                vec![[left, bottom], [right, bottom], [right, top], [left, top]],
                0.5,
            ),
            (vec![[left, mid], [right, mid]], 0.35),
            (
                vec![[left + width * 0.62, mid], [left + width * 0.62, top]],
                0.35,
            ),
            (
                vec![[left + width * 0.82, mid], [left + width * 0.82, top]],
                0.35,
            ),
            (
                vec![[left + width * 0.55, bottom], [left + width * 0.55, mid]],
                0.35,
            ),
            (
                vec![[left + width * 0.72, bottom], [left + width * 0.72, mid]],
                0.35,
            ),
        ] {
            plot.lines.push(PlotLine {
                points,
                width_mm,
                closed: false,
            });
        }

        let label = 1.8;
        let value = 3.0;
        let mut field = |x: f64, y: f64, text: &str, size: f64, bold: bool| {
            if !text.is_empty() {
                plot.texts.push(PlotText {
                    at: [x + 1.5, y + 1.5],
                    height_mm: size,
                    value: text.to_string(),
                    align: TextAlign::Left,
                    bold,
                });
            }
        };

        field(left, top - 5.0, "TITLE", label, true);
        field(left, top - 11.0, &self.title, value, false);
        field(left + width * 0.62, top - 5.0, "DRAWING No", label, true);
        field(
            left + width * 0.62,
            top - 11.0,
            if self.number.is_empty() {
                &draft.name
            } else {
                &self.number
            },
            value,
            false,
        );
        field(left + width * 0.82, top - 5.0, "REV", label, true);
        field(
            left + width * 0.82,
            top - 11.0,
            &self.revision,
            value,
            false,
        );

        field(left, mid - 5.0, "AUTHOR", label, true);
        field(left, mid - 11.0, &self.author, 2.4, false);
        field(left + width * 0.55, mid - 5.0, "DATE", label, true);
        field(left + width * 0.55, mid - 11.0, &self.date, 2.4, false);
        field(left + width * 0.72, mid - 5.0, "SCALE", label, true);
        field(
            left + width * 0.72,
            mid - 11.0,
            &scale_label(scale),
            2.4,
            false,
        );
        // The remaining strip carries unit, paper and notes, which is what a checker needs to read a
        // plotted sheet correctly: a number without a unit is not a dimension.
        let unit_line = format!(
            "{} · {} {}",
            draft.header.units.symbol(),
            self.paper.label(),
            match self.orientation {
                Orientation::Portrait => "portrait",
                Orientation::Landscape => "landscape",
            }
        );
        plot.texts.push(PlotText {
            at: [right - 1.5, top - 4.0],
            height_mm: label,
            value: unit_line,
            align: TextAlign::Right,
            bold: false,
        });
        if !self.notes.is_empty() {
            plot.texts.push(PlotText {
                at: [right - 1.5, bottom + 3.0],
                height_mm: 2.2,
                value: self.notes.clone(),
                align: TextAlign::Right,
                bold: false,
            });
        }
        let entities = draft.entities().len();
        plot.texts.push(PlotText {
            at: [right - 1.5, top - 10.0],
            height_mm: 2.2,
            value: format!(
                "{entities} entit{}",
                if entities == 1 { "y" } else { "ies" }
            ),
            align: TextAlign::Right,
            bold: false,
        });
    }
}

/// `1:2`, `2:1` or `1:1` for a scale in model units per paper millimetre.
pub fn scale_label(scale: f64) -> String {
    if scale >= 1.0 {
        format!("1:{}", number(scale))
    } else {
        format!("{}:1", number(1.0 / scale))
    }
}

fn number(value: f64) -> String {
    let mut text = format!("{value:.3}");
    while text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// The polyline for one entity, in paper millimetres. Circles and arcs are tessellated finely enough
/// for print (a 0.05 mm chord error at sheet scale).
fn entity_line<F>(kind: &EntityKind, to_paper: &F, width_mm: f64) -> Option<PlotLine>
where
    F: Fn([f64; 2]) -> [f64; 2],
{
    match kind {
        EntityKind::Point { at } => {
            let [x, y] = to_paper(*at);
            Some(PlotLine {
                points: vec![[x - 0.6, y], [x + 0.6, y]],
                width_mm: width_mm.max(0.5),
                closed: false,
            })
        }
        EntityKind::Line { start, end } => Some(PlotLine {
            points: vec![to_paper(*start), to_paper(*end)],
            width_mm,
            closed: false,
        }),
        EntityKind::Polyline { points, closed } => Some(PlotLine {
            points: points.iter().map(|point| to_paper(*point)).collect(),
            width_mm,
            closed: *closed,
        }),
        EntityKind::Circle { center, radius } => {
            let steps = arc_steps(*radius);
            let points = (0..=steps)
                .map(|step| {
                    let angle = std::f64::consts::TAU * (step as f64) / (steps as f64);
                    to_paper([
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ])
                })
                .collect();
            Some(PlotLine {
                points,
                width_mm,
                closed: true,
            })
        }
        EntityKind::Arc {
            center,
            radius,
            start_degrees,
            end_degrees,
        } => {
            let sweep = (end_degrees - start_degrees).rem_euclid(360.0);
            let steps = arc_steps(*radius);
            let points = (0..=steps)
                .map(|step| {
                    let angle =
                        (start_degrees + sweep * (step as f64) / (steps as f64)).to_radians();
                    to_paper([
                        center[0] + radius * angle.cos(),
                        center[1] + radius * angle.sin(),
                    ])
                })
                .collect();
            Some(PlotLine {
                points,
                width_mm,
                closed: false,
            })
        }
        // Text and dimensions carry their own primitives.
        EntityKind::Text { .. } | EntityKind::Dimension { .. } => None,
    }
}

/// Segments for a full circle of `radius` at a 0.05 mm chord tolerance, bounded for sanity.
fn arc_steps(radius: f64) -> usize {
    if radius <= 0.05 {
        return 8;
    }
    let ratio = 1.0 - 0.05 / radius;
    let step = if (-1.0..1.0).contains(&ratio) {
        2.0 * ratio.acos()
    } else {
        std::f64::consts::FRAC_PI_4
    };
    let steps = (std::f64::consts::TAU / step).ceil();
    #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
    let steps = steps.clamp(8.0, 720.0) as usize;
    steps
}

/// One linear dimension in paper space: extension lines, the dimension line with ticks, and the
/// measured text. `offset` is in model units along the left normal of `start -> end`.
#[allow(clippy::too_many_arguments)]
fn dimension<F>(
    plot: &mut Plot,
    start: [f64; 2],
    end: [f64; 2],
    offset: f64,
    text: Option<&str>,
    to_paper: &F,
) where
    F: Fn([f64; 2]) -> [f64; 2],
{
    let (sx, sy) = (start[0], start[1]);
    let (ex, ey) = (end[0], end[1]);
    let (dx, dy) = (ex - sx, ey - sy);
    let length = dx.hypot(dy);
    if length <= f64::EPSILON {
        return;
    }
    let (nx, ny) = (-dy / length, dx / length);
    let a = [sx + nx * offset, sy + ny * offset];
    let b = [ex + nx * offset, ey + ny * offset];
    let paper_a = to_paper(a);
    let paper_b = to_paper(b);

    for (from, to) in [(start, a), (end, b)] {
        plot.lines.push(PlotLine {
            points: vec![to_paper(from), to_paper(to)],
            width_mm: 0.18,
            closed: false,
        });
    }
    plot.lines.push(PlotLine {
        points: vec![paper_a, paper_b],
        width_mm: 0.18,
        closed: false,
    });

    // Ticks are 45-degree slashes, two millimetres long in paper space, which is legible at any scale.
    let tick = 1.0;
    for point in [paper_a, paper_b] {
        plot.lines.push(PlotLine {
            points: vec![
                [point[0] - tick, point[1] - tick],
                [point[0] + tick, point[1] + tick],
            ],
            width_mm: 0.25,
            closed: false,
        });
    }

    let value = text.map_or_else(|| number(length), ToString::to_string);
    let mid = [
        (paper_a[0] + paper_b[0]) / 2.0,
        (paper_a[1] + paper_b[1]) / 2.0,
    ];
    plot.texts.push(PlotText {
        at: [mid[0], mid[1] + 1.0],
        height_mm: 2.5,
        value,
        align: TextAlign::Center,
        bold: false,
    });
}

impl fmt::Display for Plot {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "plot {}x{} mm, {}, {} lines, {} texts",
            number(self.page_mm.0),
            number(self.page_mm.1),
            self.scale_label(),
            self.lines.len(),
            self.texts.len()
        )
    }
}

/// The layer check the plot uses, exposed so the studio can show the same answer.
pub fn plotted_entity_count(draft: &Draft) -> usize {
    draft
        .entities()
        .iter()
        .filter(|entity| {
            draft
                .layer(&entity.layer)
                .is_none_or(|layer: &Layer| layer.visible)
        })
        .count()
}

#[cfg(test)]
mod tests {
    use super::{
        Orientation, PaperSize, PlotError, Sheet, TextAlign, plotted_entity_count, scale_label,
    };
    use crate::{Draft, EntityKind};
    use cad_contract::ids::DocumentId;
    use cad_contract::units::Unit;

    fn plate() -> Draft {
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
        draft
            .add_entity(
                "0",
                EntityKind::Line {
                    start: [40.0, 0.0],
                    end: [40.0, 30.0],
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        draft
    }

    #[test]
    fn a_default_sheet_is_a3_landscape_at_one_to_one() {
        let sheet = Sheet::new("plate");
        assert_eq!(sheet.page_mm(), (420.0, 297.0));
        assert_eq!(scale_label(sheet.scale.unwrap_or_default()), "1:1");
        // Frame margin 10, title block 28 high, 6 mm of padding round the drawing.
        let [x, y, w, h] = sheet.plot_area();
        assert_eq!((x, y), (16.0, 44.0));
        assert_eq!((w, h), (388.0, 237.0));
    }

    #[test]
    fn a_drawing_is_placed_at_the_sheet_scale_and_centred() {
        let sheet = Sheet::new("plate");
        let plot = sheet.plot(&plate()).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(plot.scale, 1.0);
        assert_eq!(plot.scale_label(), "1:1");
        // 40 x 30 in a 388 x 237 area at 1:1: centred at 190, 147.5.
        // The default layer's 0.25 mm weight marks the geometry; ticks happen to share it, so the
        // search takes the first line, which is the first entity.
        let first = plot
            .lines
            .iter()
            .find(|line| line.width_mm == 0.25)
            .unwrap_or_else(|| unreachable!("entity line"));
        assert_eq!(first.points[0], [190.0, 147.5]);
        assert_eq!(first.points[1], [230.0, 147.5]);
    }

    #[test]
    fn a_scale_that_does_not_fit_names_the_standard_scale_that_does() {
        let mut draft = Draft::new("big", DocumentId::new(1), Unit::Millimeter);
        draft
            .add_entity(
                "0",
                EntityKind::Line {
                    start: [0.0, 0.0],
                    end: [4000.0, 1000.0],
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let mut sheet = Sheet::new("big");
        sheet.scale = Some(1.0);
        match sheet.plot(&draft) {
            Err(PlotError::DoesNotFit { needed, .. }) => assert_eq!(needed, "20"),
            other => unreachable!("expected DoesNotFit, got {other:?}"),
        }
        // A third of the sheet is a standard 1:20; the label follows the scale used.
        sheet.scale = None;
        let plot = sheet.plot(&draft).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(plot.scale, 20.0);
        assert_eq!(plot.scale_label(), "1:20");
    }

    #[test]
    fn a_hidden_layer_is_not_plotted_but_still_counts_in_the_drawing() {
        let mut draft = plate();
        // A 0.6 mm weight makes the visible outline separable from ticks and frame lines.
        draft
            .set_layer_line_weight("0", 0.6)
            .unwrap_or_else(|error| unreachable!("{error}"));
        draft
            .add_layer("HIDDEN", 1)
            .unwrap_or_else(|error| unreachable!("{error}"));
        draft
            .add_entity(
                "HIDDEN",
                EntityKind::Line {
                    start: [0.0, 30.0],
                    end: [40.0, 30.0],
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        draft
            .set_layer_visible("HIDDEN", false)
            .unwrap_or_else(|error| unreachable!("{error}"));
        let plot = Sheet::new("plate")
            .plot(&draft)
            .unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(draft.entities().len(), 3);
        assert_eq!(plotted_entity_count(&draft), 2);
        assert_eq!(
            plot.lines
                .iter()
                .filter(|line| line.width_mm == 0.6)
                .count(),
            2
        );
    }

    #[test]
    fn overall_dimensions_measure_the_drawing_and_can_be_switched_off() {
        let sheet = Sheet::new("plate");
        let plot = sheet.plot(&plate()).unwrap_or_else(|e| unreachable!("{e}"));
        let values = plot.text_values();
        assert!(values.contains(&"40"), "width: {values:?}");
        assert!(values.contains(&"30"), "height: {values:?}");

        let mut bare = Sheet::new("plate");
        bare.dimensions = false;
        let plot = bare.plot(&plate()).unwrap_or_else(|e| unreachable!("{e}"));
        assert!(!plot.text_values().contains(&"40"));
    }

    #[test]
    fn the_title_block_carries_the_fields_a_checker_reads() {
        let mut sheet = Sheet::new("Bracket plate");
        sheet.number = "AC-1001".into();
        sheet.revision = "B".into();
        sheet.author = "A. Person".into();
        sheet.date = "2026-10-30".into();
        sheet.notes = "DO NOT SCALE".into();
        let plot = sheet.plot(&plate()).unwrap_or_else(|e| unreachable!("{e}"));
        let values = plot.text_values();
        for expected in [
            "TITLE",
            "Bracket plate",
            "DRAWING No",
            "AC-1001",
            "REV",
            "B",
            "AUTHOR",
            "A. Person",
            "DATE",
            "2026-10-30",
            "SCALE",
            "1:1",
            "DO NOT SCALE",
        ] {
            assert!(
                values.contains(&expected),
                "missing {expected:?} in {values:?}"
            );
        }
        // The unit and paper are on the sheet, so a plotted number is readable as a measurement.
        let unit = values
            .iter()
            .find(|value| value.contains("A3"))
            .unwrap_or_else(|| unreachable!("unit line"));
        assert!(unit.contains("mm"), "{unit}");
    }

    #[test]
    fn text_entities_print_at_the_sheet_scale_and_dimensions_draw_their_own_lines() {
        let mut draft = plate();
        draft
            .add_entity(
                "0",
                EntityKind::Text {
                    at: [0.0, 15.0],
                    height: 5.0,
                    value: "PLATE".into(),
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        draft
            .add_entity(
                "0",
                EntityKind::Dimension {
                    start: [0.0, 0.0],
                    end: [40.0, 0.0],
                    offset: 5.0,
                    text: None,
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let sheet = Sheet::new("plate");
        let plot = sheet.plot(&draft).unwrap_or_else(|e| unreachable!("{e}"));
        let label = plot
            .texts
            .iter()
            .find(|text| text.value == "PLATE")
            .unwrap_or_else(|| unreachable!("text entity"));
        assert_eq!(label.height_mm, 5.0, "5 mm at 1:1");
        assert_eq!(label.align, TextAlign::Left);

        // A dimension adds two extension lines, the dimension line and two ticks to the plot.
        let mut without = plate();
        without
            .add_entity(
                "0",
                EntityKind::Text {
                    at: [0.0, 15.0],
                    height: 5.0,
                    value: "PLATE".into(),
                },
            )
            .unwrap_or_else(|error| unreachable!("{error}"));
        let base = sheet.plot(&without).unwrap_or_else(|e| unreachable!("{e}"));
        assert_eq!(plot.lines.len(), base.lines.len() + 5);
    }

    #[test]
    fn an_empty_drawing_is_refused_and_bad_sheet_fields_are_refused() {
        let empty = Draft::new("empty", DocumentId::new(1), Unit::Millimeter);
        assert_eq!(Sheet::new("empty").plot(&empty), Err(PlotError::Empty));

        let mut sheet = Sheet::new("plate");
        sheet.margin_mm = 0.0;
        assert!(matches!(sheet.validate(), Err(PlotError::Invalid(_))));
        sheet.margin_mm = 10.0;
        sheet.scale = Some(0.0);
        assert!(matches!(sheet.validate(), Err(PlotError::Invalid(_))));
        sheet.scale = Some(1.0);
        sheet.title = "x".repeat(crate::MAX_SHEET_TEXT + 1);
        assert!(matches!(sheet.validate(), Err(PlotError::Invalid(_))));
    }

    #[test]
    fn labels_and_parse_helpers_round_trip() {
        assert_eq!(scale_label(1.0), "1:1");
        assert_eq!(scale_label(2.0), "1:2");
        assert_eq!(scale_label(0.5), "2:1");
        assert_eq!(scale_label(2.5), "1:2.5");
        assert_eq!(PaperSize::parse("a3"), Some(PaperSize::A3));
        assert_eq!(PaperSize::parse("Letter"), Some(PaperSize::Letter));
        assert_eq!(PaperSize::parse("B2"), None);
        assert_eq!(
            Orientation::parse("landscape"),
            Some(Orientation::Landscape)
        );
        assert_eq!(PaperSize::A4.millimeters(), (210.0, 297.0));
        assert_eq!(Orientation::Landscape.page((210.0, 297.0)), (297.0, 210.0));
    }
}

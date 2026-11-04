//! Postprocessors: toolpaths to G-code.
//!
//! Each format emits its own header and footer; the move section is shared and modal, so a repeated
//! `G1` or an unchanged `F` is not restated. Output is deterministic: the same toolpath and format
//! always produce byte-identical text, which is what makes the regression fixtures meaningful.

use std::fmt;

use crate::setup::Setup;
use crate::tool::{Machine, ToolId};
use crate::tool_path::{MoveKind, Toolpath, ToolpathError};

/// The control dialect a program is written for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostFormat {
    /// Grbl 1.1: millimetres, absolute, `M30` end.
    Grbl,
    /// LinuxCNC: `%`-delimited program, `G40 G49 G80` modal reset, `M2` end.
    LinuxCnc,
    /// Mach3: `%`-delimited program, `M30` end.
    Mach3,
}

impl PostFormat {
    /// Parse a format name as typed on a command line.
    pub fn parse(name: &str) -> Result<Self, PostError> {
        match name {
            "grbl" => Ok(PostFormat::Grbl),
            "linuxcnc" | "linux-cnc" => Ok(PostFormat::LinuxCnc),
            "mach3" => Ok(PostFormat::Mach3),
            other => Err(PostError::UnknownFormat(other.to_string())),
        }
    }

    /// The stable name used in reports.
    pub const fn name(self) -> &'static str {
        match self {
            PostFormat::Grbl => "grbl",
            PostFormat::LinuxCnc => "linuxcnc",
            PostFormat::Mach3 => "mach3",
        }
    }

    /// The file extension a program of this format is normally given.
    pub const fn extension(self) -> &'static str {
        match self {
            PostFormat::Grbl => "nc",
            PostFormat::LinuxCnc => "ngc",
            PostFormat::Mach3 => "tap",
        }
    }

    /// Does this format delimit the program with `%` lines?
    const fn percent_delimited(self) -> bool {
        matches!(self, PostFormat::LinuxCnc | PostFormat::Mach3)
    }
}

impl fmt::Display for PostFormat {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A configured postprocessor.
#[derive(Debug, Clone, PartialEq)]
pub struct Postprocessor {
    /// The control dialect.
    pub format: PostFormat,
    /// The program name written into the header comment.
    pub program: String,
    /// Prefix every line with `N10`, `N20`, ... when the control wants them.
    pub include_line_numbers: bool,
}

impl Postprocessor {
    /// A postprocessor for `format`, with the program name in the header.
    pub fn new(format: PostFormat, program: impl Into<String>) -> Self {
        Postprocessor {
            format,
            program: program.into(),
            include_line_numbers: false,
        }
    }

    /// Turn line numbering on or off.
    pub fn with_line_numbers(mut self, enabled: bool) -> Self {
        self.include_line_numbers = enabled;
        self
    }

    /// Emit a program.
    ///
    /// The toolpath is validated against the setup first; a program that would leave the machine
    /// envelope, gouge below the stock or rapid through the stock is refused.
    pub fn emit(&self, toolpath: &Toolpath, setup: &Setup) -> Result<String, PostError> {
        self.emit_program(&[toolpath], setup)
    }

    /// Emit one program from one or more toolpaths, inserting a tool change between tools.
    ///
    /// A program with a single toolpath is byte-identical to [`Postprocessor::emit`]. Every segment
    /// is validated; the spindle stops and the tool returns to the clearance plane before the
    /// change. Grbl has no automatic tool changer, so it gets an `M0` pause with a comment instead
    /// of `T.. M6`.
    pub fn emit_program(
        &self,
        toolpaths: &[&Toolpath],
        setup: &Setup,
    ) -> Result<String, PostError> {
        if toolpaths.is_empty() {
            return Err(PostError::EmptyProgram);
        }
        for toolpath in toolpaths {
            toolpath.validate(setup)?;
        }
        if !setup.machine.has_z() {
            return Err(PostError::UnsupportedMachine(setup.machine));
        }

        let mut out = String::new();
        let mut number = 10u32;
        if self.format.percent_delimited() {
            out.push_str("%\n");
        }

        self.line(&mut out, &mut number, &format!("({})", sanitize(&self.program)));
        self.line(&mut out, &mut number, "G21");
        self.line(&mut out, &mut number, "G90");
        self.line(&mut out, &mut number, "G94");
        self.line(&mut out, &mut number, "G17");
        if self.format == PostFormat::LinuxCnc {
            self.line(&mut out, &mut number, "G40 G49 G80");
        }
        self.line(&mut out, &mut number, "G54");

        let mut last_word: Option<&'static str> = None;
        let mut last_feed: Option<f64> = None;
        let mut last_tool: Option<ToolId> = None;
        let mut last_spindle: Option<f64> = None;
        for (index, toolpath) in toolpaths.iter().enumerate() {
            if index > 0 {
                self.line(&mut out, &mut number, "M5");
                self.line(
                    &mut out,
                    &mut number,
                    &format!("G0 Z{}", number_text(toolpath.clearance_mm)),
                );
                if last_tool != Some(toolpath.tool_id) {
                    self.tool_change(&mut out, &mut number, toolpath.tool_id);
                    last_spindle = None;
                }
                last_word = None;
                last_feed = None;
            }
            let spindle_changed = last_spindle
                .map(|rpm| (rpm - toolpath.spindle_rpm).abs() > 1.0e-9)
                .unwrap_or(true);
            if spindle_changed {
                self.line(
                    &mut out,
                    &mut number,
                    &format!("S{} M3", number_text(toolpath.spindle_rpm)),
                );
                last_spindle = Some(toolpath.spindle_rpm);
            }
            for point in &toolpath.points {
                let mut words: Vec<String> = Vec::new();
                let word = point.kind.word();
                if last_word != Some(word) {
                    words.push(word.to_string());
                    last_word = Some(word);
                }
                words.push(format!("X{}", number_text(point.position[0])));
                words.push(format!("Y{}", number_text(point.position[1])));
                words.push(format!("Z{}", number_text(point.position[2])));
                if point.kind.is_cutting() {
                    let changed = last_feed
                        .map(|feed| (feed - point.feed_mm_min).abs() > 1.0e-9)
                        .unwrap_or(true);
                    if changed {
                        words.push(format!("F{}", number_text(point.feed_mm_min)));
                        last_feed = Some(point.feed_mm_min);
                    }
                }
                self.line(&mut out, &mut number, &words.join(" "));
            }
            last_tool = Some(toolpath.tool_id);
        }

        let clearance = toolpaths
            .last()
            .map(|toolpath| toolpath.clearance_mm)
            .unwrap_or(setup.clearance_mm);
        self.line(&mut out, &mut number, "M5");
        self.line(
            &mut out,
            &mut number,
            &format!("G0 Z{}", number_text(clearance)),
        );
        match self.format {
            PostFormat::LinuxCnc => self.line(&mut out, &mut number, "M2"),
            PostFormat::Grbl | PostFormat::Mach3 => self.line(&mut out, &mut number, "M30"),
        }
        if self.format.percent_delimited() {
            out.push_str("%\n");
        }
        Ok(out)
    }

    /// The words that swap to `tool`. Grbl cannot change tools by itself, so it pauses instead.
    fn tool_change(&self, out: &mut String, number: &mut u32, tool: ToolId) {
        match self.format {
            PostFormat::Grbl => {
                self.line(
                    out,
                    number,
                    &format!("(change tool to T{} then cycle start)", tool.get()),
                );
                self.line(out, number, "M0");
            }
            PostFormat::LinuxCnc | PostFormat::Mach3 => {
                self.line(out, number, &format!("T{} M6", tool.get()));
            }
        }
    }

    fn line(&self, out: &mut String, number: &mut u32, text: &str) {
        if self.include_line_numbers {
            out.push_str(&format!("N{number} {text}\n"));
            *number += 10;
        } else {
            out.push_str(text);
            out.push('\n');
        }
    }
}

/// Why a program was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PostError {
    #[error("the toolpath is not safe to post: {0}")]
    Invalid(#[from] ToolpathError),
    #[error("cannot post a program for {0}")]
    UnsupportedMachine(Machine),
    #[error("unknown postprocessor `{0}`; use grbl, linuxcnc or mach3")]
    UnknownFormat(String),
    #[error("the program name must contain at least one visible character")]
    EmptyProgram,
}

/// Format a coordinate with four decimals and no trailing zeros.
fn number_text(value: f64) -> String {
    let text = format!("{value:.4}");
    let trimmed = text.trim_end_matches('0').trim_end_matches('.');
    match trimmed {
        "" | "-0" => "0".to_string(),
        other => other.to_string(),
    }
}

/// A comment-safe program name: parentheses would end the comment early.
fn sanitize(program: &str) -> String {
    program
        .trim()
        .chars()
        .map(|character| match character {
            '(' | ')' => '_',
            other => other,
        })
        .collect()
}

/// The G word a move kind produces, for tests and reports.
pub fn move_word(kind: MoveKind) -> &'static str {
    kind.word()
}

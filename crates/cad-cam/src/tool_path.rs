//! Toolpaths: ordered moves in work coordinates, and the checks that make them safe to post.
//!
//! A [`Toolpath`] is not trusted because a generator made it. [`Toolpath::validate`] re-derives the
//! facts a machine relies on:
//!
//! * every coordinate and feed is finite, and every cutting move has a positive feed;
//! * consecutive moves are contiguous and short enough to be a single interpolation, with no
//!   zero-length moves;
//! * every move stays inside the machine envelope;
//! * no cutting move goes below the stock floor (a gouge);
//! * no rapid passes through the stock body (a crash).
//!
//! Each failure names the move index, so a caller can point at the line in the program.

use std::fmt;

use crate::setup::Setup;

/// The longest single move accepted, in millimetres.
///
/// A longer jump is treated as a discontinuity rather than a traverse: no 2.5D strategy needs one,
/// and a wrong offset or a missing point shows up as a jump.
pub const MAX_SEGMENT_MM: f64 = 250.0;

/// Distance below which two positions are considered the same point, in millimetres.
pub const POSITION_TOLERANCE_MM: f64 = 1.0e-6;

/// The rate a rapid traverse is assumed to run at, for time estimates only, in mm/min.
pub const RAPID_RATE_MM_MIN: f64 = 5_000.0;

/// An opaque toolpath identifier. Zero is reserved and is never issued.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ToolpathId(u64);

impl ToolpathId {
    /// Issue an id, refusing the reserved zero.
    pub const fn new(raw: u64) -> Option<Self> {
        match raw {
            0 => None,
            value => Some(ToolpathId(value)),
        }
    }

    /// The raw number.
    pub const fn get(self) -> u64 {
        self.0
    }
}

impl fmt::Display for ToolpathId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "toolpath {}", self.0)
    }
}

/// What kind of move a point begins.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MoveKind {
    /// A rapid traverse: the tool is not cutting.
    Rapid,
    /// A cutting move in the xy plane.
    Feed,
    /// A cutting move along the tool axis.
    Plunge,
}

impl MoveKind {
    /// Is the tool in material for this move?
    pub const fn is_cutting(self) -> bool {
        !matches!(self, MoveKind::Rapid)
    }

    /// The G word that starts this move.
    pub const fn word(self) -> &'static str {
        match self {
            MoveKind::Rapid => "G0",
            MoveKind::Feed | MoveKind::Plunge => "G1",
        }
    }

    /// A human-readable name, for reports.
    pub const fn name(self) -> &'static str {
        match self {
            MoveKind::Rapid => "rapid",
            MoveKind::Feed => "feed",
            MoveKind::Plunge => "plunge",
        }
    }
}

/// One move's endpoint and the conditions that apply from it.
///
/// The position is where the move **ends**; the move starts at the previous point. The first point
/// is therefore a positioning move to the start of the cut.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ToolpathPoint {
    /// Endpoint in work coordinates, millimetres.
    pub position: [f64; 3],
    /// What kind of move ends here.
    pub kind: MoveKind,
    /// Feed rate for a cutting move, mm/min.
    pub feed_mm_min: f64,
}

impl ToolpathPoint {
    /// A rapid traverse to `position`.
    pub const fn rapid(position: [f64; 3]) -> Self {
        ToolpathPoint {
            position,
            kind: MoveKind::Rapid,
            feed_mm_min: 0.0,
        }
    }

    /// A cutting move to `position` at `feed_mm_min`.
    pub const fn feed(position: [f64; 3], feed_mm_min: f64) -> Self {
        ToolpathPoint {
            position,
            kind: MoveKind::Feed,
            feed_mm_min,
        }
    }

    /// A plunging cutting move to `position` at `feed_mm_min`.
    pub const fn plunge(position: [f64; 3], feed_mm_min: f64) -> Self {
        ToolpathPoint {
            position,
            kind: MoveKind::Plunge,
            feed_mm_min,
        }
    }
}

/// An ordered toolpath for one tool.
#[derive(Debug, Clone, PartialEq)]
pub struct Toolpath {
    /// The toolpath id.
    pub id: ToolpathId,
    /// The tool that performs it.
    pub tool_id: crate::tool::ToolId,
    /// The moves, in execution order.
    pub points: Vec<ToolpathPoint>,
    /// The spindle speed for the whole toolpath, rpm.
    pub spindle_rpm: f64,
    /// The safe z plane the program retracts to, in work coordinates.
    pub clearance_mm: f64,
}

impl Toolpath {
    /// Assemble a toolpath. Nothing is checked here; [`Toolpath::validate`] is the gate.
    pub fn new(
        id: ToolpathId,
        tool_id: crate::tool::ToolId,
        points: Vec<ToolpathPoint>,
        spindle_rpm: f64,
        clearance_mm: f64,
    ) -> Self {
        Toolpath {
            id,
            tool_id,
            points,
            spindle_rpm,
            clearance_mm,
        }
    }

    /// The length the tool travels while cutting, in millimetres.
    pub fn cutting_length_mm(&self) -> f64 {
        self.points
            .windows(2)
            .filter(|pair| pair[1].kind.is_cutting())
            .map(|pair| distance(pair[0].position, pair[1].position))
            .sum()
    }

    /// The length the tool travels in rapids, in millimetres.
    pub fn rapid_length_mm(&self) -> f64 {
        self.points
            .windows(2)
            .filter(|pair| !pair[1].kind.is_cutting())
            .map(|pair| distance(pair[0].position, pair[1].position))
            .sum()
    }

    /// The bounding box of the toolpath in work coordinates.
    pub fn bounds(&self) -> ([f64; 3], [f64; 3]) {
        let mut min = [f64::INFINITY; 3];
        let mut max = [f64::NEG_INFINITY; 3];
        for point in &self.points {
            for axis in 0..3 {
                min[axis] = min[axis].min(point.position[axis]);
                max[axis] = max[axis].max(point.position[axis]);
            }
        }
        (min, max)
    }

    /// The machining time at the programmed feeds, in minutes.
    ///
    /// Rapids are estimated at [`RAPID_RATE_MM_MIN`]; this is a planning number, not a control
    /// number.
    pub fn estimated_minutes(&self) -> f64 {
        let mut minutes = 0.0;
        for pair in self.points.windows(2) {
            let length = distance(pair[0].position, pair[1].position);
            let rate = if pair[1].kind.is_cutting() {
                pair[1].feed_mm_min
            } else {
                RAPID_RATE_MM_MIN
            };
            if rate.is_finite() && rate > 0.0 {
                minutes += length / rate;
            }
        }
        minutes
    }

    /// Check the toolpath against the setup before any postprocessor touches it.
    pub fn validate(&self, setup: &Setup) -> Result<(), ToolpathError> {
        if self.points.len() < 2 {
            return Err(ToolpathError::TooShort {
                points: self.points.len(),
            });
        }
        if !self.spindle_rpm.is_finite() || self.spindle_rpm <= 0.0 {
            return Err(ToolpathError::InvalidSpindle {
                rpm: self.spindle_rpm,
            });
        }
        if !self.clearance_mm.is_finite() || self.clearance_mm <= setup.stock.max[2] {
            return Err(ToolpathError::InvalidClearance {
                clearance: self.clearance_mm,
                stock_top: setup.stock.max[2],
            });
        }

        let floor = setup.stock.min[2];
        let mut previous: Option<[f64; 3]> = None;
        for (index, point) in self.points.iter().enumerate() {
            for axis in 0..3 {
                if !point.position[axis].is_finite() {
                    return Err(ToolpathError::NonFinite { index, axis });
                }
            }
            if !point.feed_mm_min.is_finite() || point.feed_mm_min < 0.0 {
                return Err(ToolpathError::InvalidFeed {
                    index,
                    feed: point.feed_mm_min,
                });
            }
            if point.kind.is_cutting() && point.feed_mm_min <= 0.0 {
                return Err(ToolpathError::InvalidFeed {
                    index,
                    feed: point.feed_mm_min,
                });
            }

            let machine_point = setup
                .machine_point(point.position)
                .map_err(|error| ToolpathError::InvalidSetup(error.to_string()))?;
            for (axis, &value) in machine_point.iter().enumerate() {
                let min = setup.envelope.min[axis];
                let max = setup.envelope.max[axis];
                if value < min - POSITION_TOLERANCE_MM || value > max + POSITION_TOLERANCE_MM {
                    return Err(ToolpathError::OutsideEnvelope {
                        index,
                        axis,
                        value,
                        min,
                        max,
                    });
                }
            }

            if point.kind.is_cutting() && point.position[2] < floor - POSITION_TOLERANCE_MM {
                return Err(ToolpathError::Gouge {
                    index,
                    z: point.position[2],
                    floor,
                });
            }

            let inside_body = point.position[2] < setup.stock.max[2] - POSITION_TOLERANCE_MM
                && setup.stock.contains_xy(
                    [point.position[0], point.position[1]],
                    -POSITION_TOLERANCE_MM,
                );
            if point.kind == MoveKind::Rapid && inside_body {
                return Err(ToolpathError::Collision {
                    index,
                    position: point.position,
                });
            }

            if let Some(previous) = previous {
                let distance = distance(previous, point.position);
                if distance <= POSITION_TOLERANCE_MM {
                    return Err(ToolpathError::ZeroLengthMove { index });
                }
                if distance > MAX_SEGMENT_MM {
                    return Err(ToolpathError::Discontinuous { index, distance });
                }
            }
            previous = Some(point.position);
        }
        Ok(())
    }
}

/// The straight-line distance between two points.
pub fn distance(a: [f64; 3], b: [f64; 3]) -> f64 {
    let dx = b[0] - a[0];
    let dy = b[1] - a[1];
    let dz = b[2] - a[2];
    (dx * dx + dy * dy + dz * dz).sqrt()
}

/// Why a toolpath was refused.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ToolpathError {
    #[error("a toolpath needs at least two moves, got {points}")]
    TooShort { points: usize },
    #[error("toolpath point {index} has a non-finite axis {axis}")]
    NonFinite { index: usize, axis: usize },
    #[error("toolpath point {index} has an invalid feed of {feed} mm/min")]
    InvalidFeed { index: usize, feed: f64 },
    #[error("the spindle speed {rpm} rpm is not usable")]
    InvalidSpindle { rpm: f64 },
    #[error("the clearance plane {clearance} mm is not above the stock top {stock_top} mm")]
    InvalidClearance { clearance: f64, stock_top: f64 },
    #[error("toolpath point {index} is a zero-length move")]
    ZeroLengthMove { index: usize },
    #[error("toolpath point {index} jumps {distance} mm, more than the {MAX_SEGMENT_MM} mm maximum")]
    Discontinuous { index: usize, distance: f64 },
    #[error(
        "toolpath point {index} leaves the machine envelope: axis {axis} at {value} mm, allowed {min} .. {max}"
    )]
    OutsideEnvelope {
        index: usize,
        axis: usize,
        value: f64,
        min: f64,
        max: f64,
    },
    #[error("toolpath point {index} cuts to z {z} mm, below the stock floor {floor} mm")]
    Gouge { index: usize, z: f64, floor: f64 },
    #[error("toolpath point {index} is a rapid through the stock at {position:?}")]
    Collision { index: usize, position: [f64; 3] },
    #[error("the setup does not accept the toolpath: {0}")]
    InvalidSetup(String),
    #[error("invalid operation parameter: {0}")]
    InvalidParameter(String),
    #[error("there is no material to remove: the part already reaches the stock top")]
    NothingToMachine,
    #[error("this crate drives z-up 3-axis mills only, not {0}")]
    UnsupportedMachine(crate::tool::Machine),
}

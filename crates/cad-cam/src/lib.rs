//! The 2.5D and 3-axis CAM workspace of AtlasCAD.
//!
//! # What this crate does
//!
//! It turns a design into machine motion:
//! 1. A [`Setup`] carries the stock, the machine, the work coordinate system (WCS) and the safe
//!    clearance plane.
//! 2. A [`Tool`] carries geometry (flat end mill, ball end mill, drill, face mill), cutting
//!    conditions and the deepest cut it may take in one pass.
//! 3. An [`Operation`] — facing, contour, pocket, drilling, slotting or 3D roughing — is turned
//!    into a [`Toolpath`] by [`generate`]: ordered moves in work coordinates.
//! 4. [`Toolpath::validate`] re-checks every move against the setup, [`MaterialRemoval`] previews
//!    the stock that a program removes, and [`SetupSheet`] writes the shop-floor document.
//! 5. A [`Postprocessor`] turns the moves into G-code, the only format a control understands.
//!
//! # Design rules
//!
//! * Nothing is persisted. The CAM module is pure geometry → motion; the `.nc` file is the output
//!   of a command, not a stored object. A toolpath record in the engineering-model registry stays
//!   a summary, never the moves.
//! * Every toolpath is checked against the setup it belongs to by [`Toolpath::validate`]: a move
//!   outside the machine envelope is an error, a cutting move below the stock floor is an error,
//!   and a rapid that would pass through the stock body is an error.
//! * Generation and validation are pure. No function here touches a machine, opens a port or
//!   writes a file; the CLI owns I/O. No G-code produced here is sent to a physical machine.
//! * A G-code file is the product of one specific postprocessor and must not be reused as though
//!   it came from another one; [`Postprocessor::emit`] names the format in the program header.
//! * 3D roughing and the stock preview are heightmap models. They see the top surface a vertical
//!   ray can reach, which is the right model for 2.5D/3-axis cutting and cannot represent an
//!   undercut.

pub mod feeds;
pub mod gcode;
pub mod pdf;
pub mod setup;
pub mod sheet;
pub mod simulation;
pub mod strategy;
pub mod surface;
pub mod tool;
pub mod tool_path;
pub mod tool_path_visitor;

pub use feeds::{FeedsError, FeedsRequest, Material, RecommendedFeeds, recommend};
pub use gcode::{PostError, PostFormat, Postprocessor};
pub use pdf::setup_sheet_pdf;
pub use setup::{Setup, SetupError, Stock, WorkCoordinate};
pub use sheet::{SetupSheet, SheetOperation, SheetTool};
pub use simulation::{MaterialRemoval, SimulationError, check_holder_clearance};
pub use strategy::{Operation, generate};
pub use surface::{HeightField, SurfaceError};
pub use tool::{
    CuttingCondition, Holder, Machine, MachineEnvelope, Tool, ToolError, ToolGeometry, ToolId,
    ToolLibrary,
};
pub use tool_path::{MoveKind, Toolpath, ToolpathError, ToolpathId, ToolpathPoint};
pub use tool_path_visitor::{machine_name, show_toolpath, summarize};

/// The crate version, recorded in a program's summary and its derived artifact.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Any failure the CAM pipeline can report, so a caller can propagate one error type.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CamError {
    #[error(transparent)]
    Feeds(#[from] FeedsError),
    #[error(transparent)]
    Tool(#[from] ToolError),
    #[error(transparent)]
    Setup(#[from] SetupError),
    #[error(transparent)]
    Toolpath(#[from] ToolpathError),
    #[error(transparent)]
    Surface(#[from] SurfaceError),
    #[error(transparent)]
    Simulation(#[from] SimulationError),
    #[error(transparent)]
    Post(#[from] PostError),
}

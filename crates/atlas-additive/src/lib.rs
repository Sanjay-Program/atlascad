//! # atlas-additive
//!
//! Multi-technology additive manufacturing and 3D slicing platform for AtlasCAD.
//!
//! Provides geometric slicing, volumetric extrusion kinematics, infill pattern
//! generation, multi-dialect G-code emission, and analytical print estimation.

pub mod estimation;
pub mod gcode;
pub mod infill;
pub mod settings;
pub mod slicer;

pub mod prelude {
    pub use crate::estimation::PrintEstimation;
    pub use crate::gcode::GCodeEmitter;
    pub use crate::infill::{InfillLine, generate_infill};
    pub use crate::settings::{FilamentMaterial, InfillPattern, PrinterDialect, SlicerSettings};
    pub use crate::slicer::{
        MoveType, SlicedLayer, SlicedModel, compute_adaptive_layer_height,
        compute_adaptive_layer_height_from_normal, slice_solid_bounds,
    };
}

//! End-to-end integration of the contract and the truck adapter.
//!
//! Until now `cad-contract` and `cad-kernel-truck` had no consumer other than their own tests: the
//! contract described a kernel, and the adapter proved it worked, but nothing *used* either. This
//! crate closes that gap with the smallest real workflow:
//!
//! ```text
//! unit-aware input -> DocumentHeader (units + tolerance) -> kernel extrude/boolean
//!                  -> tessellate -> validated Mesh -> binary STL on disk
//! ```
//!
//! It exists because neither GUI application can be built on the development machine described in
//! `docs/build-and-test.md`; a headless binary is the only way to exercise the whole path there.
//!
//! # What is deliberately not here
//!
//! No GUI or general-purpose importer is included. The `project` commands delegate to
//! `cad-document` for native project persistence and history; the direct `box`/`union` helpers are
//! single-operation requests.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cad_contract::capability::Capability;
use cad_contract::document::DocumentHeader;
use cad_contract::ids::DocumentId;
use cad_contract::kernel::{BooleanOp, Kernel};
use cad_contract::mesh::Mesh;
use cad_contract::tol::Tolerance;
use cad_contract::units::Unit;
use cad_document::{Evaluated, Project, ProjectError};
use cad_kernel_truck::TruckKernel;
use serde::Serialize;
use std::path::Path;

/// Errors a CLI operation can produce.
#[derive(Debug, thiserror::Error)]
pub enum CliError {
    #[error("kernel: {0}")]
    Kernel(#[from] cad_contract::error::KernelError),
    #[error("document: {0}")]
    Document(#[from] cad_contract::document::DocumentError),
    #[error("mesh: {0}")]
    Mesh(#[from] cad_contract::mesh::MeshError),
    #[error("tolerance: {0}")]
    Tolerance(#[from] cad_contract::tol::ToleranceError),
    #[error("I/O at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid input: {0}")]
    Input(String),
    #[error("project: {0}")]
    Project(#[from] ProjectError),
}

/// An axis-aligned box request. Coordinates are in the document's unit; the kernel always receives
/// millimetres, so the conversion happens once, here.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BoxSpec {
    pub size: [f64; 3],
    pub origin: [f64; 3],
}

impl BoxSpec {
    pub fn new(size: [f64; 3], origin: [f64; 3]) -> Result<Self, CliError> {
        for (axis, value) in ["x", "y", "z"].iter().zip(size) {
            if !value.is_finite() || value <= 0.0 {
                return Err(CliError::Input(format!(
                    "size {axis} must be a finite positive number, got {value}"
                )));
            }
        }
        for (axis, value) in ["x", "y", "z"].iter().zip(origin) {
            if !value.is_finite() {
                return Err(CliError::Input(format!(
                    "origin {axis} must be finite, got {value}"
                )));
            }
        }
        Ok(BoxSpec { size, origin })
    }

    /// The rectangle handed to the kernel, in millimetres, counter-clockwise.
    fn profile_mm(&self, unit: Unit) -> [[f64; 2]; 4] {
        let ox = unit.to_mm(self.origin[0]);
        let oy = unit.to_mm(self.origin[1]);
        let sx = unit.to_mm(self.size[0]);
        let sy = unit.to_mm(self.size[1]);
        [[ox, oy], [ox + sx, oy], [ox + sx, oy + sy], [ox, oy + sy]]
    }

    fn height_mm(&self, unit: Unit) -> f64 {
        unit.to_mm(self.size[2])
    }
}

/// What was built, for machines and for humans.
#[derive(Debug, Clone, Serialize)]
pub struct BuildSummary {
    pub operation: String,
    pub kernel: String,
    pub kernel_version: String,
    pub capabilities: Vec<String>,
    pub document_id: u64,
    pub schema: u32,
    pub units: String,
    pub tolerance_mm: f64,
    pub requested_size_mm: Vec<f64>,
    pub volume_mm3: f64,
    pub surface_area_mm2: f64,
    pub triangles: usize,
    pub vertices: usize,
    pub bounding_box_mm: Option<[f64; 6]>,
}

/// The built geometry plus the header that describes its units, so a caller can persist both.
pub struct Built {
    pub mesh: Mesh,
    pub summary: BuildSummary,
    pub header: DocumentHeader,
}

impl Built {
    pub fn header_json(&self) -> Result<String, CliError> {
        Ok(self.header.to_json()?)
    }
}

fn validate_tolerance(chord_tol_mm: f64) -> Result<Tolerance, CliError> {
    let tolerance = Tolerance::new(chord_tol_mm, cad_contract::tol::DEFAULT_ANGULAR_RAD)?;
    Ok(tolerance)
}

fn capabilities_of(info: &cad_contract::capability::KernelInfo) -> Vec<String> {
    info.capabilities
        .iter()
        .map(|capability| capability.label().to_string())
        .collect()
}

fn summarise(
    operation: &str,
    kernel: &TruckKernel,
    mesh: Mesh,
    tolerance: Tolerance,
    unit: Unit,
    requested_size_mm: Vec<f64>,
) -> BuildSummary {
    let info = kernel.info();
    let capabilities = capabilities_of(&info);
    BuildSummary {
        operation: operation.to_string(),
        kernel: info.name,
        kernel_version: info.version,
        capabilities,
        document_id: 1,
        schema: cad_contract::document::CURRENT_SCHEMA,
        units: unit.symbol().to_string(),
        tolerance_mm: tolerance.linear_mm(),
        requested_size_mm,
        volume_mm3: mesh.signed_volume(),
        surface_area_mm2: mesh.surface_area(),
        triangles: mesh.triangle_count(),
        vertices: mesh.vertex_count(),
        bounding_box_mm: mesh.bounding_box().map(|bb| {
            [
                bb.min[0], bb.min[1], bb.min[2], bb.max[0], bb.max[1], bb.max[2],
            ]
        }),
    }
}

/// A new document header. A CLI invocation is a single-document project; a real application would
/// allocate the id from its document store and persist it.
fn header_for(unit: Unit, tolerance: Tolerance) -> DocumentHeader {
    let mut header = DocumentHeader::new(DocumentId::new(1), unit);
    header.tolerance = tolerance;
    header
}

/// Extrude a box, tessellate it, and report both.
pub fn build_box(spec: BoxSpec, unit: Unit, chord_tol_mm: f64) -> Result<Built, CliError> {
    let tolerance = validate_tolerance(chord_tol_mm)?;
    let mut kernel = TruckKernel::new();
    kernel.require(Capability::Extrude)?;
    kernel.require(Capability::Tessellate)?;

    let profile = spec.profile_mm(unit);
    let solid = kernel.extrude(&profile, spec.height_mm(unit))?;
    let mesh = kernel.tessellate(solid, tolerance.linear_mm())?;
    mesh.validate()?;
    kernel.release(solid);

    let summary = summarise(
        "box",
        &kernel,
        mesh.clone(),
        tolerance,
        unit,
        vec![
            unit.to_mm(spec.size[0]),
            unit.to_mm(spec.size[1]),
            unit.to_mm(spec.size[2]),
        ],
    );
    Ok(Built {
        mesh,
        summary,
        header: header_for(unit, tolerance),
    })
}

/// Extrude two boxes and union them.
pub fn build_union(
    a: BoxSpec,
    b: BoxSpec,
    unit: Unit,
    chord_tol_mm: f64,
) -> Result<Built, CliError> {
    let tolerance = validate_tolerance(chord_tol_mm)?;
    let mut kernel = TruckKernel::new();
    kernel.require(Capability::BooleanUnion)?;

    let solid_a = kernel.extrude(&a.profile_mm(unit), a.height_mm(unit))?;
    let solid_b = kernel.extrude(&b.profile_mm(unit), b.height_mm(unit))?;
    let union = kernel.boolean(BooleanOp::Union, solid_a, solid_b)?;
    let mesh = kernel.tessellate(union, tolerance.linear_mm())?;
    mesh.validate()?;
    kernel.release(solid_a);
    kernel.release(solid_b);
    kernel.release(union);

    let summary = summarise(
        "union",
        &kernel,
        mesh.clone(),
        tolerance,
        unit,
        vec![
            unit.to_mm(a.size[0]),
            unit.to_mm(a.size[1]),
            unit.to_mm(a.size[2]),
            unit.to_mm(b.size[0]),
            unit.to_mm(b.size[1]),
            unit.to_mm(b.size[2]),
            unit.to_mm(b.origin[0] - a.origin[0]),
            unit.to_mm(b.origin[1] - a.origin[1]),
            unit.to_mm(b.origin[2] - a.origin[2]),
        ],
    );
    Ok(Built {
        mesh,
        summary,
        header: header_for(unit, tolerance),
    })
}

/// Evaluate a parametric project through the truck adapter.
pub fn evaluate_project(project: &Project) -> Result<(Evaluated, TruckKernel), CliError> {
    let mut kernel = TruckKernel::new();
    kernel.require(Capability::Extrude)?;
    kernel.require(Capability::Tessellate)?;
    let evaluated = project.evaluate(&mut kernel)?;
    Ok((evaluated, kernel))
}

// ---------------------------------------------------------------------------------------------
// Mesh interchange
// ---------------------------------------------------------------------------------------------
//
// Binary STL lives in `cad-io` because the desktop application writes the same format. These
// wrappers keep the CLI's established error type for callers and tests while there is one
// implementation of the format.

impl From<cad_io::StlError> for CliError {
    fn from(error: cad_io::StlError) -> Self {
        match error {
            cad_io::StlError::Mesh(error) => CliError::Mesh(error),
            cad_io::StlError::Io { path, source } => CliError::Io { path, source },
            cad_io::StlError::Invalid(message) => CliError::Input(message),
        }
    }
}

/// Serialise a mesh as a binary STL.
pub fn stl_bytes(mesh: &Mesh) -> Result<Vec<u8>, CliError> {
    Ok(cad_io::stl_bytes(mesh)?)
}

/// Write a mesh as a binary STL and return the number of bytes written.
pub fn write_stl_binary(mesh: &Mesh, path: &Path) -> Result<usize, CliError> {
    Ok(cad_io::write_stl_binary(mesh, path)?)
}

/// Read back a binary STL as a mesh. The format carries no units; the header next to it does.
pub fn read_stl_binary(bytes: &[u8]) -> Result<Mesh, CliError> {
    Ok(cad_io::read_stl_binary(bytes)?)
}

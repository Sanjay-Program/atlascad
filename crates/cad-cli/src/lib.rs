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
use cad_drafting::{Draft, DraftError, PlanError};
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
    #[error("drafting: {0}")]
    Drafting(#[from] DraftError),
    #[error("plan: {0}")]
    Plan(#[from] PlanError),
    #[error("sheet: {0}")]
    Sheet(#[from] cad_drafting::PlotError),
    #[error("engineering model: {0}")]
    Model(#[from] cad_engineering_model::ModelError),
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

/// A drafting document's summary, printed by `atlas draft` and `atlas dxf`.
#[derive(Debug, Clone, Serialize)]
pub struct DraftSummary {
    pub file: String,
    pub name: String,
    pub unit: String,
    pub layers: usize,
    pub entities: usize,
    /// `[min_x, min_y, max_x, max_y]` in the drawing's unit, or null when the drawing is empty.
    pub bounds: Option<[f64; 4]>,
    /// Sum of every entity's length — a quick check that nothing was silently dropped.
    pub total_length: f64,
    pub dxf: Option<String>,
    pub stl: Option<String>,
}

/// Outline a project's plan view, in millimetres.
///
/// The profile of the feature that built the body is the plan outline of a single-body part: it is
/// what the part occupies looking straight down. The profile is always in document units, so it is
/// converted to millimetres here and the drawing is millimetre-native regardless of the project's
/// units — a drawing that silently mixed inches and millimetres would be worse than no drawing.
pub fn plan_outline(project: &Project) -> Result<Vec<[f64; 2]>, CliError> {
    Ok(cad_drafting::plan_outline(project)?)
}

/// Build a millimetre plan-view drawing of a project: one closed outline on layer `OUTLINE`.
pub fn draft_plan_view(project: &Project) -> Result<Draft, CliError> {
    Ok(cad_drafting::plan_draft(project)?)
}

/// Load a `.zdraft` and report what it holds without changing it.
pub fn draft_summary(path: &Path) -> Result<DraftSummary, CliError> {
    let draft = Draft::load(path)?;
    Ok(summary_of(&draft, path, None))
}

/// Write a drawing as DXF (R12, `AC1009`), returning the summary that includes the output path.
pub fn draft_to_dxf(from: &Path, out: &Path) -> Result<DraftSummary, CliError> {
    let draft = Draft::load(from)?;
    let dxf = draft.to_dxf();
    std::fs::write(out, dxf.as_bytes()).map_err(|source| CliError::Io {
        path: out.display().to_string(),
        source,
    })?;
    Ok(summary_of(&draft, from, Some(out)))
}

/// Save a drawing and, if `out` is given, export it to DXF in the same call.
pub fn write_draft(
    draft: &Draft,
    save: Option<&Path>,
    dxf_out: Option<&Path>,
) -> Result<DraftSummary, CliError> {
    if let Some(path) = save {
        draft.save(path)?;
    }
    if let Some(path) = dxf_out {
        std::fs::write(path, draft.to_dxf().as_bytes()).map_err(|source| CliError::Io {
            path: path.display().to_string(),
            source,
        })?;
    }
    let label = save.or(dxf_out).unwrap_or_else(|| Path::new(""));
    let mut summary = summary_of(draft, label, dxf_out);
    // `summary_of` reports the DXF when asked; report the saved drawing too so a caller that only
    // asked for `--save` still sees where it went.
    if let Some(path) = save {
        summary.file = path.display().to_string();
    }
    Ok(summary)
}

/// What a DXF import read, with the entity types it could not represent named.
#[derive(Debug, Clone, Serialize)]
pub struct DxfImportSummary {
    pub source: String,
    /// The unit the file stated, or `null` when it stated none and millimetres were assumed.
    pub declared_unit: Option<String>,
    pub draft: DraftSummary,
    /// Entity types this reader does not model, with how often each appeared. An import with a
    /// non-empty list is a partial drawing, and says so rather than looking complete.
    pub skipped: Vec<Skipped>, // serialised as `[{ "kind", "count" }]`
}

/// One unread entity type and its count.
#[derive(Debug, Clone, Serialize)]
pub struct Skipped {
    pub kind: String,
    pub count: usize,
}

/// Read a DXF file into a drawing, saving it as `.zdraft` and/or writing a normalised DXF.
///
/// The name of the drawing comes from the file stem, because a DXF R12 has no dependable title.
pub fn import_dxf(
    from: &Path,
    save: Option<&Path>,
    out: Option<&Path>,
) -> Result<DxfImportSummary, CliError> {
    let text = std::fs::read_to_string(from).map_err(|source| CliError::Io {
        path: from.display().to_string(),
        source,
    })?;
    let name = from
        .file_stem()
        .and_then(|stem| stem.to_str())
        .unwrap_or("drawing");
    let imported = cad_drafting::from_dxf(name, DocumentId::new(1), &text)?;
    let summary = write_draft(&imported.draft, save, out)?;
    Ok(DxfImportSummary {
        source: from.display().to_string(),
        declared_unit: imported
            .report
            .declared_unit
            .map(|unit| unit.symbol().to_string()),
        skipped: imported
            .report
            .skipped
            .iter()
            .map(|(kind, count)| Skipped {
                kind: kind.clone(),
                count: *count,
            })
            .collect(),
        draft: summary,
    })
}

/// A plotted sheet's summary, printed by `atlas sheet`.
#[derive(Debug, Clone, Serialize)]
pub struct SheetSummary {
    pub file: String,
    /// `project` when the drawing was generated from a `.zcad`, `drawing` when it was read from a
    /// `.zdraft`.
    pub source: &'static str,
    pub paper: String,
    pub orientation: String,
    pub scale: String,
    /// `[width, height]` in millimetres.
    pub page_mm: [f64; 2],
    pub lines: usize,
    pub texts: usize,
    pub pdf: Option<String>,
}

/// Load a drawing from either document: a `.zdraft` is opened as itself, a project is drafted to
/// its plan first. Deciding by extension keeps `atlas sheet` usable on both without a flag that
/// only repeats the file name.
fn load_drawing(path: &Path) -> Result<(Draft, &'static str), CliError> {
    if path.extension().and_then(|extension| extension.to_str()) == Some("zdraft") {
        Ok((Draft::load(path)?, "drawing"))
    } else {
        let project = Project::load(path)?;
        Ok((draft_plan_view(&project)?, "project"))
    }
}

/// Lay a drawing out on a sheet and write the page as PDF.
pub fn plot_sheet(
    from: &Path,
    sheet: &cad_drafting::Sheet,
    out: &Path,
) -> Result<SheetSummary, CliError> {
    let (draft, source) = load_drawing(from)?;
    // An untitled sheet takes the drawing's name rather than printing a blank TITLE row.
    let mut sheet = sheet.clone();
    if sheet.title.trim().is_empty() {
        sheet.title = draft.name.clone();
    }
    let plot = sheet.plot(&draft)?;
    let pdf = cad_drafting::to_pdf(&plot, &draft.name);
    std::fs::write(out, pdf.as_bytes()).map_err(|source| CliError::Io {
        path: out.display().to_string(),
        source,
    })?;
    Ok(SheetSummary {
        file: from.display().to_string(),
        source,
        paper: sheet.paper.label().to_string(),
        orientation: match sheet.orientation {
            cad_drafting::Orientation::Portrait => "portrait",
            cad_drafting::Orientation::Landscape => "landscape",
        }
        .to_string(),
        scale: plot.scale_label(),
        page_mm: [plot.page_mm.0, plot.page_mm.1],
        lines: plot.lines.len(),
        texts: plot.texts.len(),
        pdf: Some(out.display().to_string()),
    })
}

fn summary_of(draft: &Draft, file: &Path, dxf: Option<&Path>) -> DraftSummary {
    DraftSummary {
        file: file.display().to_string(),
        name: draft.name.clone(),
        unit: draft.header.units.symbol().to_string(),
        layers: draft.layers().len(),
        entities: draft.entities().len(),
        bounds: draft.bounds(),
        total_length: draft.total_length(),
        dxf: dxf.map(|p| p.display().to_string()),
        stl: None,
    }
}

//! `cad-cae` — the first real analysis producer in AtlasCAD.
//!
//! # What this crate actually does
//!
//! It runs a **linear static structural analysis** on a watertight tessellated solid:
//!
//! 1. classify a regular grid of voxels inside the closed triangle mesh ([`crate::voxel`]);
//! 2. build an 8-node hexahedral finite-element mesh from the solid voxels;
//! 3. assemble an isotropic linear-elastic stiffness matrix and solve `K u = f` with a
//!    Jacobi-preconditioned conjugate gradient solver;
//! 4. report displacement, von Mises stress at element centres, reactions, strain energy,
//!    equilibrium residual and solver convergence, together with the assumptions that produced
//!    them.
//!
//! # What it is not
//!
//! It is **not** a certified analysis, a boundary-fitted mesher, a nonlinear solver, a modal or
//! thermal solver, or a substitute for engineering judgement. The output is labelled a preliminary
//! engineering study, and [`StaticResult::warnings`] lists the assumptions. In particular:
//!
//! * the voxel mesh approximates curved and sloped geometry at the voxel size;
//! * loads are lumped equally over the selected face nodes, not applied as a consistent traction;
//! * supports are idealised fixed or component-fixed nodes;
//! * the material is homogeneous, isotropic and linear elastic under small strain, and the library
//!   values are typical handbook figures rather than a qualified specification;
//! * a fully integrated hex8 element is stiff in bending on a coarse mesh, so a bending result
//!   needs the mesh-convergence series the validation suite runs.
//!
//! # Provenance
//!
//! A study is a [`StaticStudy`] value that can be saved and re-run; a result is a
//! [`StaticResult`] whose summary is small enough to be recorded as an
//! [`cad_engineering_model::DerivedArtifact`] of kind `simulation`, pinned to the design revision
//! that produced it. The registry then refuses to release it after the design changes — see the
//! release policy in `cad-engineering-model`.

use cad_contract::document::Revision;
use cad_contract::ids::DocumentId;
use cad_contract::mesh::Mesh;
use cad_engineering_model::{Material, MaterialLibrary};
use serde::{Deserialize, Serialize};

mod fe;
mod voxel;

/// The label every result carries. It is data, not documentation, so a report cannot drop it.
pub const QUALITY_LABEL: &str =
    "preliminary engineering study - linear elastic, not a certified analysis";

/// Version of the JSON study file a job reads. A newer file is refused, not guessed at.
pub const STUDY_SCHEMA: u32 = 1;
/// Largest study file accepted, in bytes. A study is a short definition, not data.
pub const MAX_STUDY_BYTES: usize = 64 * 1024;
/// Upper bound on solid elements in one study.
pub const MAX_STUDY_ELEMENTS: usize = 250_000;
/// Upper bound on finite-element nodes in one study.
pub const MAX_STUDY_NODES: usize = 400_000;
/// Largest admissible difference between the voxel volume and the body volume.
pub const MAX_VOLUME_ERROR: f64 = 0.25;

/// Why a study could not be run.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum CaeError {
    #[error("invalid study: {0}")]
    Invalid(String),
    #[error("material `{0}` is not in the material library")]
    UnknownMaterial(String),
    #[error("material `{0}` has no {1}; a linear static study needs it")]
    MissingProperty(String, &'static str),
    #[error("the mesh is not closed ({0} open edges); a structural analysis needs a solid body")]
    OpenMesh(usize),
    #[error("mesh: {0}")]
    Mesh(String),
    #[error(
        "the voxel mesh is too large: {elements} elements and {nodes} nodes exceed the \
         {max_elements}-element / {max_nodes}-node limit; use a larger voxel size"
    )]
    TooLarge {
        elements: usize,
        nodes: usize,
        max_elements: usize,
        max_nodes: usize,
    },
    #[error("the solver failed: {0}")]
    Solver(String),
    #[error("cannot serialise or write the result: {0}")]
    Io(String),
    #[error("the study file is not a valid study file: {0}")]
    StudyFile(String),
}

/// A global axis. Faces are selected by an axis and a coordinate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Axis {
    X,
    Y,
    Z,
}

impl Axis {
    pub const fn index(self) -> usize {
        match self {
            Axis::X => 0,
            Axis::Y => 1,
            Axis::Z => 2,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Axis::X => "x",
            Axis::Y => "y",
            Axis::Z => "z",
        }
    }

    /// Parse `x`, `y` or `z`, case-insensitively.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "x" => Some(Axis::X),
            "y" => Some(Axis::Y),
            "z" => Some(Axis::Z),
            _ => None,
        }
    }
}

/// A planar face selection: all nodes whose coordinate on `axis` is within `tolerance_mm` of
/// `coordinate_mm`.
///
/// A plane is the smallest selection that is both meaningful to a user and exactly testable. It is
/// not a face id, so a re-tessellated body re-selects the same plane; it also means a load cannot be
/// applied to one curved face of a part. That limitation is deliberate and recorded.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceSelection {
    pub axis: Axis,
    pub coordinate_mm: f64,
    pub tolerance_mm: f64,
}

impl FaceSelection {
    /// A plane at `coordinate_mm` with a 1 µm tolerance.
    pub fn new(axis: Axis, coordinate_mm: f64) -> Self {
        FaceSelection {
            axis,
            coordinate_mm,
            tolerance_mm: 1.0e-3,
        }
    }

    /// A plane with an explicit tolerance.
    pub fn with_tolerance(axis: Axis, coordinate_mm: f64, tolerance_mm: f64) -> Self {
        FaceSelection {
            axis,
            coordinate_mm,
            tolerance_mm,
        }
    }

    fn validate(&self, what: &str) -> Result<(), CaeError> {
        if !self.coordinate_mm.is_finite() {
            return Err(CaeError::Invalid(format!(
                "{what} has a non-finite coordinate"
            )));
        }
        if !self.tolerance_mm.is_finite() || self.tolerance_mm <= 0.0 {
            return Err(CaeError::Invalid(format!(
                "{what} needs a positive, finite tolerance"
            )));
        }
        Ok(())
    }
}

/// A support: a selected plane with per-component fixity.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Support {
    pub face: FaceSelection,
    /// Which translational components are fixed: `[x, y, z]`.
    pub fix: [bool; 3],
}

impl Support {
    /// A fully fixed (encastré) face.
    pub const fn fixed(face: FaceSelection) -> Self {
        Support {
            face,
            fix: [true, true, true],
        }
    }

    /// A roller/symmetry support that fixes one translational component.
    pub fn hold(face: FaceSelection, component: Axis) -> Self {
        let mut fix = [false; 3];
        fix[component.index()] = true;
        Support { face, fix }
    }
}

/// A force distributed equally over the nodes of a selected plane.
///
/// `total_force_n` is the **sum** over the face, not a per-node value. The vector is in newtons in
/// the document's global axes.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FaceLoad {
    pub face: FaceSelection,
    pub total_force_n: [f64; 3],
}

impl FaceLoad {
    pub const fn new(face: FaceSelection, total_force_n: [f64; 3]) -> Self {
        FaceLoad {
            face,
            total_force_n,
        }
    }
}

/// Solver controls. The residual is `||r|| / ||f||` on the constrained system.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SolverSettings {
    pub relative_tolerance: f64,
    pub max_iterations: usize,
}

impl Default for SolverSettings {
    fn default() -> Self {
        SolverSettings {
            relative_tolerance: 1.0e-8,
            max_iterations: 20_000,
        }
    }
}

/// A saved linear static study. This is the input that can be re-run and compared.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StaticStudy {
    pub name: String,
    /// Edge length of one hexahedral element, in millimetres.
    pub voxel_size_mm: f64,
    /// Material id from the shared material library.
    pub material: String,
    pub supports: Vec<Support>,
    pub loads: Vec<FaceLoad>,
    #[serde(default)]
    pub solver: SolverSettings,
}

impl StaticStudy {
    pub fn new(name: impl Into<String>, voxel_size_mm: f64, material: impl Into<String>) -> Self {
        StaticStudy {
            name: name.into(),
            voxel_size_mm,
            material: material.into(),
            supports: Vec::new(),
            loads: Vec::new(),
            solver: SolverSettings::default(),
        }
    }

    /// The versioned JSON form a deployment job reads, with the schema the parser expects.
    pub fn to_json(&self) -> Result<String, CaeError> {
        #[derive(Serialize)]
        struct StudyFile<'a> {
            schema: u32,
            study: &'a StaticStudy,
        }
        serde_json::to_string_pretty(&StudyFile {
            schema: STUDY_SCHEMA,
            study: self,
        })
        .map_err(|error| CaeError::Io(format!("cannot serialise the study: {error}")))
    }

    /// Check the study before it reaches the mesher or the solver.
    pub fn validate(&self) -> Result<(), CaeError> {
        let name = self.name.trim();
        if name.is_empty() || name.chars().count() > 120 {
            return Err(CaeError::Invalid(
                "a study name must be 1..=120 characters".into(),
            ));
        }
        if !self.voxel_size_mm.is_finite() || self.voxel_size_mm <= 0.0 {
            return Err(CaeError::Invalid(format!(
                "voxel size {} is not a positive length",
                self.voxel_size_mm
            )));
        }
        if self.material.trim().is_empty() {
            return Err(CaeError::Invalid("the study names no material".into()));
        }
        if self.supports.is_empty() {
            return Err(CaeError::Invalid(
                "the study has no supports; an unrestrained body has no unique solution".into(),
            ));
        }
        if self.loads.is_empty() {
            return Err(CaeError::Invalid("the study has no loads".into()));
        }
        if self.supports.len() > 64 || self.loads.len() > 64 {
            return Err(CaeError::Invalid(
                "a study is limited to 64 supports and 64 loads".into(),
            ));
        }
        for support in &self.supports {
            support.face.validate("a support")?;
            if !support.fix.iter().any(|fixed| *fixed) {
                return Err(CaeError::Invalid(
                    "a support fixes no component; remove it instead".into(),
                ));
            }
        }
        for load in &self.loads {
            load.face.validate("a load")?;
            if load.total_force_n.iter().any(|value| !value.is_finite()) {
                return Err(CaeError::Invalid(
                    "a load has a non-finite force component".into(),
                ));
            }
            if load.total_force_n.iter().all(|value| *value == 0.0) {
                return Err(CaeError::Invalid(
                    "a load has zero total force; remove it instead".into(),
                ));
            }
        }
        if !self.solver.relative_tolerance.is_finite()
            || self.solver.relative_tolerance <= 0.0
            || self.solver.relative_tolerance > 1.0e-2
        {
            return Err(CaeError::Invalid(
                "the solver relative tolerance must be in (0, 1e-2]".into(),
            ));
        }
        if self.solver.max_iterations == 0 || self.solver.max_iterations > 1_000_000 {
            return Err(CaeError::Invalid(
                "the solver iteration limit must be in 1..=1,000,000".into(),
            ));
        }
        Ok(())
    }
}

/// Parse a versioned study file (see [`StaticStudy::to_json`]).
///
/// Parsing is separated from validation on purpose: a malformed or oversized file is a broken job
/// input ([`CaeError::StudyFile`]), while a well-formed study the model cannot accept is a refusal
/// ([`CaeError`] variants produced by [`StaticStudy::validate`]). A supervisor can retry the first
/// and must not retry the second.
///
/// The file is bounded before it is parsed, the schema is checked before the study is trusted, and
/// unknown fields are refused at every level, so a newer or mistyped file cannot be silently
/// accepted with a field ignored.
pub fn parse_study(bytes: &[u8]) -> Result<StaticStudy, CaeError> {
    if bytes.len() > MAX_STUDY_BYTES {
        return Err(CaeError::StudyFile(format!(
            "{} bytes exceeds the {MAX_STUDY_BYTES}-byte limit",
            bytes.len()
        )));
    }
    #[derive(Deserialize)]
    #[serde(deny_unknown_fields)]
    struct StudyFile {
        schema: u32,
        study: StaticStudy,
    }
    let file: StudyFile =
        serde_json::from_slice(bytes).map_err(|error| CaeError::StudyFile(error.to_string()))?;
    if file.schema != STUDY_SCHEMA {
        return Err(CaeError::StudyFile(format!(
            "schema {} is not supported; this engine reads schema {STUDY_SCHEMA}",
            file.schema
        )));
    }
    file.study.validate()?;
    Ok(file.study)
}

/// The material values a result was computed with, so the result cannot drift from its inputs.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MaterialRef {
    pub id: String,
    pub name: String,
    pub youngs_modulus_mpa: f64,
    pub poisson_ratio: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub yield_strength_mpa: Option<f64>,
    pub density_kg_m3: f64,
    pub source: String,
}

/// The analysis mesh, described well enough to judge whether it is fine enough.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct MeshSummary {
    pub surface_triangles: usize,
    pub voxel_size_mm: f64,
    pub elements: usize,
    pub nodes: usize,
    pub degrees_of_freedom: usize,
    pub constrained_dofs: usize,
    /// Volume of the analysis mesh, mm³.
    pub voxel_volume_mm3: f64,
    /// Volume of the source solid, mm³ (divergence theorem on the tessellation).
    pub source_volume_mm3: f64,
    /// `|voxel - source| / source`.
    pub volume_error_ratio: f64,
}

/// What the solver did, including the evidence that it converged.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SolverReport {
    pub method: String,
    pub iterations: usize,
    pub relative_residual: f64,
    pub tolerance: f64,
    pub converged: bool,
}

/// The engineering numbers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResultSummary {
    pub max_displacement_mm: f64,
    pub max_displacement_at_mm: [f64; 3],
    pub max_von_mises_mpa: f64,
    pub max_von_mises_at_mm: [f64; 3],
    /// Elastic strain energy in N·mm (= mJ).
    pub strain_energy_nmm: f64,
    pub applied_force_n: [f64; 3],
    pub reaction_force_n: [f64; 3],
    /// `|applied + reaction| / |applied|`; a large value means the solve did not balance.
    pub equilibrium_residual_ratio: f64,
    /// Yield strength divided by maximum von Mises stress, when the library has a yield strength.
    /// This is a linear-elastic stress ratio only.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub factor_of_safety: Option<f64>,
}

/// The design revision a result was produced from, so the JSON file alone says which design it
/// describes. The same identity is what the artifact registry pins.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct StudySource {
    pub document: DocumentId,
    pub revision: Revision,
}

/// One completed linear static study.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct StaticResult {
    pub study: String,
    /// Always present; see [`QUALITY_LABEL`].
    pub quality: String,
    /// The engine that produced the result, `cad-cae <version>`. A deployment pins the image; this
    /// field says which engine actually ran.
    pub engine: String,
    /// The study that produced the result, embedded so the result file is self-contained. Without
    /// it a saved result and its inputs can be separated and the numbers cannot be reproduced.
    pub definition: StaticStudy,
    /// The design revision the mesh was evaluated from, when the caller knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<StudySource>,
    pub material: MaterialRef,
    pub mesh: MeshSummary,
    pub solver: SolverReport,
    pub results: ResultSummary,
    pub warnings: Vec<String>,
}

impl StaticResult {
    /// Attach the design revision this result was computed from.
    pub fn with_source(mut self, document: DocumentId, revision: Revision) -> Self {
        self.source = Some(StudySource { document, revision });
        self
    }

    /// A one-paragraph summary suitable for an artifact record (well under the registry's limit).
    pub fn summary_line(&self) -> String {
        format!(
            "linear static: max displacement {:.6} mm, max von Mises {:.3} MPa at {}, \
             factor of safety {}, {} hex8 elements at {:.3} mm, PCG {} iterations, \
             residual {:.1e}. {QUALITY_LABEL}",
            self.results.max_displacement_mm,
            self.results.max_von_mises_mpa,
            format_point(self.results.max_von_mises_at_mm),
            match self.results.factor_of_safety {
                Some(value) => format!("{value:.3} (linear-elastic stress ratio only)"),
                None => "not available (no yield strength in the material library)".to_string(),
            },
            self.mesh.elements,
            self.mesh.voxel_size_mm,
            self.solver.iterations,
            self.solver.relative_residual,
        )
    }

    /// The study definition this result came from is not stored in the result; a caller that wants
    /// to reproduce it must keep the [`StaticStudy`]. This returns the analysis fingerprint instead.
    pub fn mesh_fingerprint(&self) -> String {
        format!(
            "{}x{}x{} hex8 @ {:.6} mm",
            self.mesh.elements,
            self.mesh.nodes,
            self.mesh.degrees_of_freedom,
            self.mesh.voxel_size_mm
        )
    }
}

fn format_point(point: [f64; 3]) -> String {
    format!("({:.3}, {:.3}, {:.3})", point[0], point[1], point[2])
}

fn magnitude(values: [f64; 3]) -> f64 {
    (values[0] * values[0] + values[1] * values[1] + values[2] * values[2]).sqrt()
}

/// Run a linear static study on `mesh` with the study's material.
pub fn run_static(
    mesh: &Mesh,
    study: &StaticStudy,
    library: &MaterialLibrary,
) -> Result<StaticResult, CaeError> {
    study.validate()?;
    let material = library
        .get(&study.material)
        .ok_or_else(|| CaeError::UnknownMaterial(study.material.clone()))?;
    mesh.validate()
        .map_err(|error| CaeError::Mesh(error.to_string()))?;
    if !mesh.is_closed() {
        return Err(CaeError::OpenMesh(mesh.open_edges()));
    }
    let youngs = material
        .youngs_modulus_mpa
        .ok_or_else(|| CaeError::MissingProperty(material.id.clone(), "Young's modulus"))?;
    let poisson = material
        .poisson_ratio
        .ok_or_else(|| CaeError::MissingProperty(material.id.clone(), "Poisson ratio"))?;
    let source_volume = mesh.signed_volume().abs();
    if !source_volume.is_finite() || source_volume <= 0.0 {
        return Err(CaeError::Mesh(
            "the mesh encloses no volume; check its orientation".into(),
        ));
    }

    let grid = voxel::VoxelGrid::build(mesh, study.voxel_size_mm)?;
    let [nx, ny, nz] = grid.counts;
    let node_counts = grid.node_counts();
    let node_total = node_counts[0]
        .checked_mul(node_counts[1])
        .and_then(|value| value.checked_mul(node_counts[2]))
        .ok_or_else(|| CaeError::Invalid("the node grid overflows".into()))?;
    let elements = grid.element_count();
    if elements == 0 {
        return Err(CaeError::Invalid(
            "the voxel mesh contains no solid elements".into(),
        ));
    }
    if elements > MAX_STUDY_ELEMENTS || node_total > MAX_STUDY_NODES {
        return Err(CaeError::TooLarge {
            elements,
            nodes: node_total,
            max_elements: MAX_STUDY_ELEMENTS,
            max_nodes: MAX_STUDY_NODES,
        });
    }
    let ndof = node_total * 3;

    let mut positions = Vec::with_capacity(node_total);
    for k in 0..node_counts[2] {
        for j in 0..node_counts[1] {
            for i in 0..node_counts[0] {
                positions.push([
                    grid.origin[0] + i as f64 * grid.cell,
                    grid.origin[1] + j as f64 * grid.cell,
                    grid.origin[2] + k as f64 * grid.cell,
                ]);
            }
        }
    }
    if positions.len() != node_total {
        return Err(CaeError::Invalid("the node table is inconsistent".into()));
    }

    let node = |i: usize, j: usize, k: usize| -> u32 {
        (i + node_counts[0] * (j + node_counts[1] * k)) as u32
    };
    let mut element_nodes = Vec::with_capacity(elements);
    for k in 0..nz {
        for j in 0..ny {
            for i in 0..nx {
                let index = i + nx * (j + ny * k);
                if !grid.inside.get(index).copied().unwrap_or(false) {
                    continue;
                }
                element_nodes.push([
                    node(i, j, k),
                    node(i + 1, j, k),
                    node(i + 1, j + 1, k),
                    node(i, j + 1, k),
                    node(i, j, k + 1),
                    node(i + 1, j, k + 1),
                    node(i + 1, j + 1, k + 1),
                    node(i, j + 1, k + 1),
                ]);
            }
        }
    }
    if element_nodes.len() != elements {
        return Err(CaeError::Invalid(
            "the element table is inconsistent with the voxel mask".into(),
        ));
    }

    // Refuse a mesh that cannot represent the body before spending a solve on it.
    let voxel_volume_mm3 = elements as f64 * grid.cell.powi(3);
    let volume_error_ratio = (voxel_volume_mm3 - source_volume).abs() / source_volume;
    if volume_error_ratio > MAX_VOLUME_ERROR {
        return Err(CaeError::Invalid(format!(
            "the {} mm voxel mesh has volume {voxel_volume_mm3:.3} mm3 against the body's \
             {source_volume:.3} mm3 ({:.0}% off, limit {:.0}%); use a smaller voxel size",
            grid.cell,
            volume_error_ratio * 100.0,
            MAX_VOLUME_ERROR * 100.0
        )));
    }

    // Nodes beyond the solid (the grid can overhang the body) belong to no element. A load on an
    // orphan node has no stiffness to react against, which makes the system inconsistent, so only
    // nodes attached to at least one element can be selected.
    let mut used = vec![false; node_total];
    for nodes in &element_nodes {
        for &node in nodes.iter() {
            if let Some(entry) = used.get_mut(node as usize) {
                *entry = true;
            }
        }
    }

    let elasticity = fe::Elasticity::isotropic(youngs, poisson);
    let local = fe::hex_stiffness(grid.cell, &elasticity);

    let mut fixed = vec![false; ndof];
    let mut force = vec![0.0; ndof];
    for support in &study.supports {
        let selected = select_nodes(&positions, &used, &support.face).map_err(|message| {
            CaeError::Invalid(format!("support at {}: {message}", describe(&support.face)))
        })?;
        for index in selected {
            for component in 0..3 {
                if support.fix[component] {
                    fixed[index * 3 + component] = true;
                }
            }
        }
    }
    let constrained_dofs = fixed.iter().filter(|value| **value).count();
    if constrained_dofs == 0 {
        return Err(CaeError::Invalid(
            "the supports constrain no degree of freedom".into(),
        ));
    }
    if constrained_dofs >= ndof {
        return Err(CaeError::Invalid(
            "the supports constrain every degree of freedom; nothing can deflect".into(),
        ));
    }

    for load in &study.loads {
        let selected = select_nodes(&positions, &used, &load.face).map_err(|message| {
            CaeError::Invalid(format!("load at {}: {message}", describe(&load.face)))
        })?;
        let share = 1.0 / selected.len() as f64;
        for index in selected {
            for component in 0..3 {
                force[index * 3 + component] += load.total_force_n[component] * share;
            }
        }
    }

    let outcome = fe::solve(&element_nodes, &local, ndof, &fixed, &force, &study.solver)
        .map_err(CaeError::Solver)?;

    let mut max_displacement = 0.0;
    let mut max_displacement_at = [0.0; 3];
    for (index, position) in positions.iter().enumerate() {
        let displacement = [
            outcome.displacement[index * 3],
            outcome.displacement[index * 3 + 1],
            outcome.displacement[index * 3 + 2],
        ];
        let magnitude = magnitude(displacement);
        if magnitude > max_displacement {
            max_displacement = magnitude;
            max_displacement_at = *position;
        }
    }

    let mut max_von_mises = 0.0;
    let mut max_von_mises_at = [0.0; 3];
    let mut ue = [0.0; fe::ELEMENT_DOFS];
    for nodes in &element_nodes {
        let mut centroid = [0.0; 3];
        for local_node in 0..fe::ELEMENT_NODES {
            let global = nodes[local_node] as usize;
            let base = global * 3;
            ue[local_node * 3] = outcome.displacement[base];
            ue[local_node * 3 + 1] = outcome.displacement[base + 1];
            ue[local_node * 3 + 2] = outcome.displacement[base + 2];
            if let Some(position) = positions.get(global) {
                for (axis, value) in centroid.iter_mut().enumerate() {
                    *value += position[axis] / 8.0;
                }
            }
        }
        let stress = fe::centre_stress(&ue, grid.cell, &elasticity);
        let von_mises = fe::von_mises(&stress);
        if von_mises > max_von_mises {
            max_von_mises = von_mises;
            max_von_mises_at = centroid;
        }
    }

    let applied_force_n = [
        force.iter().step_by(3).sum(),
        force.iter().skip(1).step_by(3).sum(),
        force.iter().skip(2).step_by(3).sum(),
    ];
    let mut reaction_force_n = [0.0; 3];
    for (index, value) in outcome.internal_force.iter().enumerate() {
        if fixed[index] {
            // A support reaction is the internal force minus any load applied at that node.
            reaction_force_n[index % 3] += value - force[index];
        }
    }
    let applied_magnitude = magnitude(applied_force_n);
    let equilibrium = magnitude([
        applied_force_n[0] + reaction_force_n[0],
        applied_force_n[1] + reaction_force_n[1],
        applied_force_n[2] + reaction_force_n[2],
    ]) / applied_magnitude.max(f64::MIN_POSITIVE);

    let strain_energy_nmm = 0.5
        * outcome
            .displacement
            .iter()
            .zip(force.iter())
            .map(|(u, f)| u * f)
            .sum::<f64>();

    let mut warnings = vec![
        "The analysis mesh is a voxel (stair-step) hexahedral approximation of the tessellated \
         surface; curved and sloped faces are quantised at the voxel size."
            .to_string(),
        "Loads are lumped equally over the nodes selected on each face, not applied as a \
         consistent traction, so stress and deflection near a load face are approximations."
            .to_string(),
        "Supports are idealised fixed or component-fixed nodes; real joint, weld and contact \
         stiffness is not modelled."
            .to_string(),
        "Linear elastic, small strain, homogeneous isotropic material, no self-weight, contact, \
         plasticity, buckling, fatigue or residual stress."
            .to_string(),
        "Stresses are recovered at element centres only; a peak at a fillet, hole or re-entrant \
         corner needs a finer mesh and a stress-concentration check."
            .to_string(),
        QUALITY_LABEL.to_string(),
    ];
    if volume_error_ratio > 0.05 {
        warnings.push(format!(
            "The voxel mesh volume differs from the body by {:.1}%; refine the voxel size before \
             relying on absolute quantities.",
            volume_error_ratio * 100.0
        ));
    }
    if equilibrium > 1.0e-6 {
        warnings.push(format!(
            "The reaction sum does not balance the applied load (residual {equilibrium:.2e}); \
             treat the result as suspect."
        ));
    }

    let factor_of_safety = match material.yield_strength_mpa {
        Some(yield_strength) if max_von_mises > 0.0 => Some(yield_strength / max_von_mises),
        _ => None,
    };

    Ok(StaticResult {
        study: study.name.clone(),
        quality: QUALITY_LABEL.to_string(),
        engine: format!("{} {}", env!("CARGO_PKG_NAME"), env!("CARGO_PKG_VERSION")),
        definition: study.clone(),
        source: None,
        material: material_ref(material, youngs, poisson),
        mesh: MeshSummary {
            surface_triangles: mesh.triangle_count(),
            voxel_size_mm: grid.cell,
            elements,
            nodes: node_total,
            degrees_of_freedom: ndof,
            constrained_dofs,
            voxel_volume_mm3,
            source_volume_mm3: source_volume,
            volume_error_ratio,
        },
        solver: SolverReport {
            method: "Jacobi-preconditioned conjugate gradient".to_string(),
            iterations: outcome.iterations,
            relative_residual: outcome.relative_residual,
            tolerance: study.solver.relative_tolerance,
            converged: outcome.relative_residual <= study.solver.relative_tolerance,
        },
        results: ResultSummary {
            max_displacement_mm: max_displacement,
            max_displacement_at_mm: max_displacement_at,
            max_von_mises_mpa: max_von_mises,
            max_von_mises_at_mm: max_von_mises_at,
            strain_energy_nmm,
            applied_force_n,
            reaction_force_n,
            equilibrium_residual_ratio: equilibrium,
            factor_of_safety,
        },
        warnings,
    })
}

fn material_ref(material: &Material, youngs_modulus_mpa: f64, poisson_ratio: f64) -> MaterialRef {
    MaterialRef {
        id: material.id.clone(),
        name: material.name.clone(),
        youngs_modulus_mpa,
        poisson_ratio,
        yield_strength_mpa: material.yield_strength_mpa,
        density_kg_m3: material.density_kg_m3,
        source: material.source.clone(),
    }
}

fn describe(face: &FaceSelection) -> String {
    format!(
        "{}={} (tolerance {} mm)",
        face.axis.label(),
        face.coordinate_mm,
        face.tolerance_mm
    )
}

fn select_nodes(
    positions: &[[f64; 3]],
    used: &[bool],
    face: &FaceSelection,
) -> Result<Vec<usize>, String> {
    let axis = face.axis.index();
    let selected: Vec<usize> = positions
        .iter()
        .enumerate()
        .filter(|(index, position)| {
            used.get(*index).copied().unwrap_or(false)
                && position
                    .get(axis)
                    .is_some_and(|value| (*value - face.coordinate_mm).abs() <= face.tolerance_mm)
        })
        .map(|(index, _)| index)
        .collect();
    if selected.is_empty() {
        return Err(format!(
            "no node lies on the plane (model spans {}..{} mm on this axis)",
            positions
                .iter()
                .filter_map(|position| position.get(axis).copied())
                .fold(f64::INFINITY, f64::min),
            positions
                .iter()
                .filter_map(|position| position.get(axis).copied())
                .fold(f64::NEG_INFINITY, f64::max),
        ));
    }
    Ok(selected)
}

#[cfg(test)]
mod tests {
    use super::*;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn cantilever() -> StaticStudy {
        let mut study = StaticStudy::new("job", 2.5, "steel.1018");
        study.supports = vec![Support::fixed(FaceSelection::new(Axis::X, 0.0))];
        study.loads = vec![FaceLoad::new(
            FaceSelection::new(Axis::X, 60.0),
            [0.0, 0.0, -100.0],
        )];
        study
    }

    #[test]
    fn a_study_file_round_trips_through_its_schema() -> TestResult {
        let study = cantilever();
        let parsed = parse_study(study.to_json()?.as_bytes())?;
        assert_eq!(parsed, study);
        Ok(())
    }

    #[test]
    fn a_malformed_or_newer_study_file_is_not_a_study() -> TestResult {
        // Not JSON at all.
        assert!(matches!(parse_study(b"{"), Err(CaeError::StudyFile(_))));
        // An unknown field is refused instead of ignored.
        let study = cantilever();
        let unknown = serde_json::json!({ "schema": STUDY_SCHEMA, "study": study, "extra": true });
        assert!(matches!(
            parse_study(&serde_json::to_vec(&unknown)?),
            Err(CaeError::StudyFile(_))
        ));
        // A newer schema is refused, not guessed at.
        let newer = serde_json::json!({ "schema": STUDY_SCHEMA + 1, "study": study });
        assert!(matches!(
            parse_study(&serde_json::to_vec(&newer)?),
            Err(CaeError::StudyFile(_))
        ));
        // The file is bounded before it is parsed.
        assert!(matches!(
            parse_study(&vec![b' '; MAX_STUDY_BYTES + 1]),
            Err(CaeError::StudyFile(_))
        ));
        Ok(())
    }

    #[test]
    fn a_well_formed_study_the_model_cannot_accept_is_a_refusal_not_a_broken_file() -> TestResult {
        // No supports: the file is valid, the study is not analysable. That distinction is what a
        // supervisor uses to decide between "fix the job" and "fix the study".
        let study = StaticStudy::new("empty", 2.5, "steel.1018");
        let file = serde_json::json!({ "schema": STUDY_SCHEMA, "study": study });
        let bytes = serde_json::to_vec(&file)?;
        assert!(matches!(parse_study(&bytes), Err(CaeError::Invalid(_))));
        Ok(())
    }
}

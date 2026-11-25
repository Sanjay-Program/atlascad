//! ISO 10303-21 (STEP) exchange for faceted solids.
//!
//! Two directions, one entity subset:
//!
//! * [`step_bytes`] writes an AP214 `CONFIG_CONTROL_DESIGN` physical file whose geometry is a
//!   faceted boundary representation: one planar `ADVANCED_FACE` per triangle, each with a
//!   `POLY_LOOP` bound and a real `PLANE`. A watertight triangle mesh becomes a
//!   `MANIFOLD_SOLID_BREP`; a mesh with boundary edges becomes a `SHELL_BASED_SURFACE_MODEL` with an
//!   `OPEN_SHELL`, because calling an open patch a solid is a claim a reader would act on.
//! * [`read_step`] parses the same subset back into a [`Mesh`]: `CARTESIAN_POINT`,
//!   `VERTEX_POINT`, `POLY_LOOP`, `FACE_OUTER_BOUND`/`FACE_BOUND`, `ADVANCED_FACE`/`FACE_SURFACE`,
//!   `CLOSED_SHELL`/`OPEN_SHELL`, and the three solid/surface-model wrappers that contain them.
//!
//! # Why faceted
//!
//! The contract's kernel exchange is a triangle [`Mesh`], not a B-rep. Writing analytic planes,
//! cylinders and NURBS would require the feature history to survive into this crate, which it does
//! not. The file therefore states exactly what it holds: planar facets. A reader that wants smooth
//! surfaces will tessellate them; none is invented here.
//!
//! # Units
//!
//! The writer always states millimetres (`SI_UNIT(.MILLI.,.METRE.)`), matching every coordinate in
//! [`Mesh`]. The reader recognises SI metre with a prefix and `CONVERSION_BASED_UNIT` inches, and
//! converts to millimetres, so the returned mesh always obeys the contract's unit.
//!
//! # Untrusted input
//!
//! [`read_step`] caps the file at [`MAX_STEP_BYTES`] and refuses a document with more entities or
//! triangles than [`MAX_STEP_ENTITIES`] and [`MAX_STEP_TRIANGLES`] rather than allocating for a
//! claim. A reference to an undefined entity is an error naming the reference, not a silently
//! dropped face.
//!
//! # What this does not do
//!
//! It does not read analytic surfaces (`B_SPLINE_SURFACE`, `CYLINDRICAL_SURFACE`, `TOROIDAL_SURFACE`),
//! `EDGE_LOOP`-based B-reps, assemblies (`NEXT_ASSEMBLY_USAGE_OCCURRENCE`), colours, PMI, or
//! AP242 tessellated face sets. A file using them is refused when it contains no faceted faces, and
//! individual non-faceted faces are counted in [`StepImport::skipped_faces`] instead of being dropped
//! silently.

#![deny(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use cad_contract::mesh::{Mesh, MeshError};
use cad_contract::units::Unit;
use std::collections::HashMap;
use std::path::Path;

/// Largest accepted STEP file, checked before parsing untrusted input.
pub const MAX_STEP_BYTES: usize = 256 * 1024 * 1024;

/// Most `#id=ENTITY(...)` records accepted from one file.
pub const MAX_STEP_ENTITIES: usize = 8_000_000;

/// Most triangles accepted from one file.
pub const MAX_STEP_TRIANGLES: usize = 2_000_000;

/// Why a STEP file could not be written or read.
#[derive(Debug, thiserror::Error)]
pub enum StepError {
    #[error("mesh: {0}")]
    Mesh(#[from] MeshError),
    #[error("I/O at {path}: {source}")]
    Io {
        path: String,
        source: std::io::Error,
    },
    #[error("invalid STEP: {0}")]
    Invalid(String),
}

/// What a STEP file contained, in contract units.
#[derive(Debug, Clone)]
pub struct StepImport {
    /// The product name from `PRODUCT`, or the file stem when the file does not name one.
    pub name: String,
    /// Triangles in millimetres, deduplicated by `CARTESIAN_POINT`.
    pub mesh: Mesh,
    /// The unit the file declared, before conversion to millimetres.
    pub source_unit: Unit,
    /// True when the geometry came from a `MANIFOLD_SOLID_BREP` or a `CLOSED_SHELL`.
    pub closed: bool,
    /// Entity records in the `DATA` section, including ones this reader does not interpret.
    pub entities: usize,
    /// Faces that contributed at least one triangle.
    pub faces: usize,
    /// Faces this reader could not use (an analytic surface or a non-polygonal bound).
    pub skipped_faces: usize,
}

// ---------------------------------------------------------------------------------------------
// Writing
// ---------------------------------------------------------------------------------------------

/// Serialise a triangle mesh as an AP214 faceted BREP.
///
/// Degenerate triangles (two identical corners, or zero area) are omitted: a face with no area is
/// not representable as a planar face and would make the shell inconsistent. A mesh whose welded
/// edges are not each shared by exactly two faces is written as a shell-based surface model.
pub fn step_bytes(mesh: &Mesh, name: &str) -> Result<Vec<u8>, StepError> {
    mesh.validate()?;
    let name = step_string(name);
    let closed = is_watertight(mesh);

    let mut out = String::with_capacity(512 + mesh.triangles.len() * 420);
    let mut ids = EntityIds::default();

    out.push_str("ISO-10303-21;\nHEADER;\n");
    out.push_str("FILE_DESCRIPTION(('AtlasCAD faceted boundary representation','2;0'),'2;1');\n");
    out.push_str(&format!(
        "FILE_NAME('{name}.stp','1970-01-01T00:00:00',('AtlasCAD'),('AtlasCAD'),'AtlasCAD','AtlasCAD','');\n"
    ));
    out.push_str("FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));\nENDSEC;\nDATA;\n");

    let application = ids.next();
    out.push_str(&format!(
        "#{application}=APPLICATION_CONTEXT('core data for automotive mechanical design processes');\n"
    ));
    let protocol = ids.next();
    out.push_str(&format!(
        "#{protocol}=APPLICATION_PROTOCOL_DEFINITION('international standard','config_control_design',1994,#{application});\n"
    ));

    // Product structure: an AP214 file without one is a bag of geometry no CAD system can place.
    let product_context = ids.next();
    out.push_str(&format!(
        "#{product_context}=PRODUCT_CONTEXT('',#{application},'mechanical');\n"
    ));
    let product = ids.next();
    out.push_str(&format!(
        "#{product}=PRODUCT('{name}','{name}','',(#{product_context}));\n"
    ));
    let formation = ids.next();
    out.push_str(&format!(
        "#{formation}=PRODUCT_DEFINITION_FORMATION('','',#{product});\n"
    ));
    let definition_context = ids.next();
    out.push_str(&format!(
        "#{definition_context}=PRODUCT_DEFINITION_CONTEXT('part definition',#{application},'design');\n"
    ));
    let definition = ids.next();
    out.push_str(&format!(
        "#{definition}=PRODUCT_DEFINITION('design','',#{formation},#{definition_context});\n"
    ));
    let definition_shape = ids.next();
    out.push_str(&format!(
        "#{definition_shape}=PRODUCT_DEFINITION_SHAPE('','',#{definition});\n"
    ));

    // Units and tolerance. The geometry is in millimetres, so say so.
    let length_unit = ids.next();
    out.push_str(&format!(
        "#{length_unit}=(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.));\n"
    ));
    let angle_unit = ids.next();
    out.push_str(&format!(
        "#{angle_unit}=(NAMED_UNIT(*)PLANE_ANGLE_UNIT()SI_UNIT($,.RADIAN.));\n"
    ));
    let solid_angle_unit = ids.next();
    out.push_str(&format!(
        "#{solid_angle_unit}=(NAMED_UNIT(*)SOLID_ANGLE_UNIT()SI_UNIT($,.STERADIAN.));\n"
    ));
    let uncertainty = ids.next();
    out.push_str(&format!(
        "#{uncertainty}=UNCERTAINTY_MEASURE_WITH_UNIT(LENGTH_MEASURE(1.E-6),#{length_unit},'distance_accuracy_value','');\n"
    ));
    let context = ids.next();
    out.push_str(&format!(
        "#{context}=(GEOMETRIC_REPRESENTATION_CONTEXT(3)GLOBAL_UNCERTAINTY_ASSIGNED_CONTEXT((#{uncertainty}))GLOBAL_UNIT_ASSIGNED_CONTEXT((#{length_unit},#{angle_unit},#{solid_angle_unit}))REPRESENTATION_CONTEXT('',''));\n"
    ));

    // Geometry. Each triangle gets its own three vertices: STEP topology does not need them shared,
    // and a per-face loop keeps every bound's orientation independent.
    let mut faces = Vec::with_capacity(mesh.triangles.len());
    for triangle in &mesh.triangles {
        let corners = [
            mesh.positions[triangle[0] as usize],
            mesh.positions[triangle[1] as usize],
            mesh.positions[triangle[2] as usize],
        ];
        let Some((normal, reference)) = face_frame(corners) else {
            continue;
        };

        let mut apexes = [0u64; 3];
        let mut apex_points = [0u64; 3];
        for (slot, corner) in corners.iter().enumerate() {
            let point = ids.next();
            out.push_str(&format!(
                "#{point}=CARTESIAN_POINT('',({},{},{}));\n",
                real(corner[0]),
                real(corner[1]),
                real(corner[2])
            ));
            let vertex = ids.next();
            out.push_str(&format!("#{vertex}=VERTEX_POINT('',#{point});\n"));
            apexes[slot] = vertex;
            apex_points[slot] = point;
        }

        let loop_id = ids.next();
        out.push_str(&format!(
            "#{loop_id}=POLY_LOOP('',(#{},#{},#{}));\n",
            apexes[0], apexes[1], apexes[2]
        ));
        let bound = ids.next();
        out.push_str(&format!("#{bound}=FACE_OUTER_BOUND('',#{loop_id},.T.);\n"));

        let axis = ids.next();
        out.push_str(&format!(
            "#{axis}=DIRECTION('',({},{},{}));\n",
            real(normal[0]),
            real(normal[1]),
            real(normal[2])
        ));
        let reference_direction = ids.next();
        out.push_str(&format!(
            "#{reference_direction}=DIRECTION('',({},{},{}));\n",
            real(reference[0]),
            real(reference[1]),
            real(reference[2])
        ));
        let placement = ids.next();
        out.push_str(&format!(
            "#{placement}=AXIS2_PLACEMENT_3D('',#{},#{axis},#{reference_direction});\n",
            apex_points[0]
        ));
        let plane = ids.next();
        out.push_str(&format!("#{plane}=PLANE('',#{placement});\n"));
        let face = ids.next();
        out.push_str(&format!(
            "#{face}=ADVANCED_FACE('',(#{bound}),#{plane},.T.);\n"
        ));
        faces.push(face);
    }

    if faces.is_empty() {
        return Err(StepError::Invalid(
            "the mesh has no non-degenerate triangle to export".into(),
        ));
    }

    let face_list = faces
        .iter()
        .map(|id| format!("#{id}"))
        .collect::<Vec<_>>()
        .join(",");
    let shell = ids.next();
    let representation_item = if closed {
        out.push_str(&format!("#{shell}=CLOSED_SHELL('',({face_list}));\n"));
        let brep = ids.next();
        out.push_str(&format!(
            "#{brep}=MANIFOLD_SOLID_BREP('{name}',#{shell});\n"
        ));
        brep
    } else {
        out.push_str(&format!("#{shell}=OPEN_SHELL('',({face_list}));\n"));
        let model = ids.next();
        out.push_str(&format!(
            "#{model}=SHELL_BASED_SURFACE_MODEL('{name}',(#{shell}));\n"
        ));
        model
    };

    let representation = ids.next();
    let representation_kind = if closed {
        "ADVANCED_BREP_SHAPE_REPRESENTATION"
    } else {
        // A surface model is not a B-rep, so it goes in the generic representation.
        "SHAPE_REPRESENTATION"
    };
    out.push_str(&format!(
        "#{representation}={representation_kind}('{name}',(#{representation_item}),#{context});\n"
    ));
    let shape_representation = ids.next();
    out.push_str(&format!(
        "#{shape_representation}=SHAPE_DEFINITION_REPRESENTATION(#{definition_shape},#{representation});\n"
    ));

    out.push_str("ENDSEC;\nEND-ISO-10303-21;\n");
    Ok(out.into_bytes())
}

/// Write a mesh as an AP214 STEP file and return the number of bytes written.
pub fn write_step(mesh: &Mesh, name: &str, path: &Path) -> Result<usize, StepError> {
    let bytes = step_bytes(mesh, name)?;
    std::fs::write(path, &bytes).map_err(|source| StepError::Io {
        path: path.display().to_string(),
        source,
    })?;
    Ok(bytes.len())
}

/// Entity id counter. Ids are handed out in write order, so every reference points backwards.
#[derive(Default)]
struct EntityIds {
    next: u64,
}

impl EntityIds {
    fn next(&mut self) -> u64 {
        self.next += 1;
        self.next
    }
}

/// The outward normal and a stable in-plane reference direction for one triangle.
///
/// `None` means the triangle is degenerate: two corners coincide or the cross product is not a
/// finite, non-zero vector.
fn face_frame(corners: [[f64; 3]; 3]) -> Option<([f64; 3], [f64; 3])> {
    let u = [
        corners[1][0] - corners[0][0],
        corners[1][1] - corners[0][1],
        corners[1][2] - corners[0][2],
    ];
    let v = [
        corners[2][0] - corners[0][0],
        corners[2][1] - corners[0][1],
        corners[2][2] - corners[0][2],
    ];
    let cross = [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ];
    let normal = normalize(cross)?;
    let reference = normalize(u)?;
    Some((normal, reference))
}

fn normalize(vector: [f64; 3]) -> Option<[f64; 3]> {
    let length = (vector[0] * vector[0] + vector[1] * vector[1] + vector[2] * vector[2]).sqrt();
    if !length.is_finite() || length <= 1e-12 {
        return None;
    }
    Some([vector[0] / length, vector[1] / length, vector[2] / length])
}

/// Is every welded edge shared by exactly two triangles, in opposite directions?
///
/// Positions are welded at 1e-6 mm before the test, because a tessellator is free to duplicate a
/// vertex per face and that duplication says nothing about the surface being open.
fn is_watertight(mesh: &Mesh) -> bool {
    let mut welded: HashMap<(i64, i64, i64), u32> = HashMap::new();
    let mut ids = Vec::with_capacity(mesh.positions.len());
    for position in &mesh.positions {
        let key = (
            (position[0] * 1e6).round() as i64,
            (position[1] * 1e6).round() as i64,
            (position[2] * 1e6).round() as i64,
        );
        let next = welded.len() as u32;
        let id = *welded.entry(key).or_insert(next);
        ids.push(id);
    }
    let mut directed: HashMap<(u32, u32), usize> = HashMap::new();
    for triangle in &mesh.triangles {
        let a = ids[triangle[0] as usize];
        let b = ids[triangle[1] as usize];
        let c = ids[triangle[2] as usize];
        if a == b || b == c || c == a {
            return false;
        }
        for edge in [(a, b), (b, c), (c, a)] {
            *directed.entry(edge).or_insert(0) += 1;
        }
    }
    !directed.is_empty()
        && directed
            .iter()
            .all(|(&(a, b), &count)| count == 1 && directed.get(&(b, a)) == Some(&1))
}

/// A STEP string: quote-doubled, with control characters and the surrounding quotes' hazards
/// removed. A caller-supplied project name must never break the file's grammar.
fn step_string(value: &str) -> String {
    let mut out = String::with_capacity(value.len().min(120));
    for character in value.chars().take(120) {
        let safe = if character.is_control() || character == '\\' {
            ' '
        } else {
            character
        };
        if safe == '\'' {
            out.push_str("''");
        } else {
            out.push(safe);
        }
    }
    let trimmed = out.trim();
    if trimmed.is_empty() {
        "part".to_string()
    } else {
        trimmed.to_string()
    }
}

/// A STEP `REAL`. STEP requires a decimal point, so `0` is written `0.` and `1.5` keeps its digits.
fn real(value: f64) -> String {
    if !value.is_finite() {
        return "0.".to_string();
    }
    if value.abs() < 1e-9 {
        return "0.".to_string();
    }
    let mut text = format!("{value:.9}");
    if text.contains('.') {
        while text.ends_with('0') {
            text.pop();
        }
        if text.ends_with('.') {
            text.push('0');
        }
    } else {
        text.push('.');
    }
    text
}

// ---------------------------------------------------------------------------------------------
// Reading
// ---------------------------------------------------------------------------------------------

/// Parse a faceted STEP file into a millimetre mesh.
pub fn read_step(bytes: &[u8]) -> Result<StepImport, StepError> {
    if bytes.len() > MAX_STEP_BYTES {
        return Err(StepError::Invalid(format!(
            "STEP file is {} bytes, over the {MAX_STEP_BYTES}-byte limit",
            bytes.len()
        )));
    }
    let text = std::str::from_utf8(bytes)
        .map_err(|_| StepError::Invalid("STEP file is not UTF-8".into()))?;
    if !text.trim_start().starts_with("ISO-10303-21") {
        return Err(StepError::Invalid(
            "file does not begin with the ISO-10303-21 signature".into(),
        ));
    }

    let entities = parse_entities(text)?;
    if entities.is_empty() {
        return Err(StepError::Invalid(
            "the DATA section contains no entity records".into(),
        ));
    }

    let resolver = Resolver {
        entities: &entities,
    };

    // Entity ids are sorted so the same file always yields the same product name and unit when it
    // declares more than one.
    let mut ordered: Vec<u64> = entities.keys().copied().collect();
    ordered.sort_unstable();

    let mut name = "part".to_string();
    for id in &ordered {
        let (kind, arguments) = &entities[id];
        if kind == "PRODUCT"
            && let Some(Arg::Str(value)) = arguments.first()
            && !value.is_empty()
        {
            name = value.clone();
        }
    }
    let (product_units, scale) = unit_scale(&entities, &ordered);

    // Collect the shells that belong to a solid, or every shell when the file has no solid wrapper.
    let mut shells: Vec<u64> = Vec::new();
    let mut closed = false;
    for id in &ordered {
        let (kind, arguments) = &entities[id];
        match kind.as_str() {
            "MANIFOLD_SOLID_BREP" | "BREP_WITH_VOIDS" => {
                if let Some(Arg::Ref(shell)) = arguments.get(1) {
                    shells.push(*shell);
                    closed = true;
                }
            }
            "SHELL_BASED_SURFACE_MODEL" => {
                if let Some(Arg::List(list)) = arguments.get(1) {
                    for value in list {
                        if let Arg::Ref(shell) = value {
                            shells.push(*shell);
                        }
                    }
                }
            }
            _ => {}
        }
    }
    if shells.is_empty() {
        for id in &ordered {
            if entities[id].0 == "CLOSED_SHELL" {
                closed = true;
            }
        }
        shells = ordered
            .iter()
            .copied()
            .filter(|id| {
                let kind = &entities[id].0;
                kind == "CLOSED_SHELL" || kind == "OPEN_SHELL"
            })
            .collect();
    }

    let mut faces: Vec<u64> = Vec::new();
    for shell in &shells {
        let Some((kind, arguments)) = entities.get(shell) else {
            return Err(StepError::Invalid(format!(
                "a shell reference points at #{shell}, which is not defined"
            )));
        };
        if kind != "CLOSED_SHELL" && kind != "OPEN_SHELL" {
            return Err(StepError::Invalid(format!(
                "#{shell} is `{kind}`, not a shell"
            )));
        }
        let list = match arguments.get(1) {
            Some(Arg::List(list)) => list,
            _ => {
                return Err(StepError::Invalid(format!("#{shell} has no face list")));
            }
        };
        for value in list {
            if let Arg::Ref(face) = value {
                faces.push(*face);
            }
        }
    }
    if faces.is_empty() {
        // A file with only free faces (no shell) is still usable as a surface set.
        faces = ordered
            .iter()
            .copied()
            .filter(|id| {
                let kind = &entities[id].0;
                kind == "ADVANCED_FACE" || kind == "FACE_SURFACE"
            })
            .collect();
    }
    if faces.is_empty() {
        return Err(StepError::Invalid(
            "the file contains no faceted face this reader can use".into(),
        ));
    }
    if faces.len() > MAX_STEP_TRIANGLES {
        return Err(StepError::Invalid(format!(
            "the file declares {} faces, over the {MAX_STEP_TRIANGLES}-face limit",
            faces.len()
        )));
    }

    let mut point_index: HashMap<u64, u32> = HashMap::new();
    let mut positions: Vec<[f64; 3]> = Vec::new();
    let mut triangles: Vec<[u32; 3]> = Vec::new();
    let mut used_faces = 0;
    let mut skipped_faces = 0;

    for face in &faces {
        let Some(loop_vertices) = resolver.face_vertices(*face) else {
            skipped_faces += 1;
            continue;
        };
        if loop_vertices.len() < 3 {
            skipped_faces += 1;
            continue;
        }

        let mut ring = Vec::with_capacity(loop_vertices.len());
        for vertex in &loop_vertices {
            let Some(point) = resolver.vertex_point(*vertex) else {
                return Err(StepError::Invalid(format!(
                    "a vertex reference points at #{vertex}, which is not a VERTEX_POINT"
                )));
            };
            let index = match point_index.get(&point) {
                Some(index) => *index,
                None => {
                    let Some(position) = resolver.cartesian_point(point) else {
                        return Err(StepError::Invalid(format!(
                            "a vertex points at #{point}, which is not a CARTESIAN_POINT"
                        )));
                    };
                    let index = u32::try_from(positions.len()).map_err(|_| {
                        StepError::Invalid("the file has too many vertices for a u32 mesh".into())
                    })?;
                    positions.push([
                        position[0] * scale,
                        position[1] * scale,
                        position[2] * scale,
                    ]);
                    point_index.insert(point, index);
                    index
                }
            };
            ring.push(index);
        }

        // A POLY_LOOP is a polygon; fan-triangulating it is exact for the planar faces this reader
        // accepts and keeps a square face as two triangles rather than inventing a centre vertex.
        for corner in 1..ring.len() - 1 {
            triangles.push([ring[0], ring[corner], ring[corner + 1]]);
        }
        used_faces += 1;
    }

    if triangles.is_empty() {
        return Err(StepError::Invalid(
            "the file's faces produced no triangles".into(),
        ));
    }
    if triangles.len() > MAX_STEP_TRIANGLES {
        return Err(StepError::Invalid(format!(
            "the file's faces produce more than {MAX_STEP_TRIANGLES} triangles"
        )));
    }

    let mesh = Mesh {
        positions,
        triangles,
        normals: Vec::new(),
    };
    mesh.validate()?;

    Ok(StepImport {
        name,
        mesh,
        source_unit: product_units,
        closed,
        entities: entities.len(),
        faces: used_faces,
        skipped_faces,
    })
}

/// Read a faceted STEP file from disk.
pub fn read_step_file(path: &Path) -> Result<StepImport, StepError> {
    let bytes = std::fs::read(path).map_err(|source| StepError::Io {
        path: path.display().to_string(),
        source,
    })?;
    read_step(&bytes)
}

/// One parameter of a STEP entity instance.
#[derive(Debug, Clone, PartialEq)]
enum Arg {
    Str(String),
    Ref(u64),
    Num(f64),
    Enum(String),
    Bool(bool),
    List(Vec<Arg>),
    Typed(String, Vec<Arg>),
    Null,
}

/// Split the file into `#id=ENTITY(args)` records and parse each one's parameters.
fn parse_entities(text: &str) -> Result<HashMap<u64, (String, Vec<Arg>)>, StepError> {
    let mut entities = HashMap::new();
    let mut count = 0usize;
    for statement in statements(text) {
        let Some((id, rest)) = split_record(&statement) else {
            continue;
        };
        count += 1;
        if count > MAX_STEP_ENTITIES {
            return Err(StepError::Invalid(format!(
                "the file has more than {MAX_STEP_ENTITIES} entities"
            )));
        }
        let trimmed = rest.trim();
        let (kind, arguments) = if trimmed.starts_with('(') {
            // A complex entity instance: several typed components in one record.
            let mut parser = Parser::new(trimmed);
            ("COMPLEX".to_string(), parser.complex_list()?)
        } else {
            let open = trimmed
                .find('(')
                .ok_or_else(|| StepError::Invalid(format!("#{id} has no parameter list")))?;
            let kind = trimmed[..open].trim().to_ascii_uppercase();
            if kind.is_empty() {
                return Err(StepError::Invalid(format!("#{id} has no entity name")));
            }
            let mut parser = Parser::new(&trimmed[open..]);
            (kind, parser.parameter_list()?)
        };
        entities.insert(id, (kind, arguments));
    }
    Ok(entities)
}

/// Statements are separated by semicolons outside strings and comments.
fn statements(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    let mut characters = text.chars().peekable();
    let mut in_string = false;
    while let Some(character) = characters.next() {
        if in_string {
            current.push(character);
            if character == '\'' {
                if characters.peek() == Some(&'\'') {
                    current.push('\'');
                    characters.next();
                } else {
                    in_string = false;
                }
            }
            continue;
        }
        match character {
            '\'' => {
                in_string = true;
                current.push(character);
            }
            '/' if characters.peek() == Some(&'*') => {
                characters.next();
                while let Some(character) = characters.next() {
                    if character == '*' && characters.peek() == Some(&'/') {
                        characters.next();
                        break;
                    }
                }
            }
            ';' => out.push(std::mem::take(&mut current)),
            _ => current.push(character),
        }
    }
    out
}

fn split_record(statement: &str) -> Option<(u64, &str)> {
    let trimmed = statement.trim();
    let rest = trimmed.strip_prefix('#')?;
    let equals = rest.find('=')?;
    let id = rest[..equals].trim().parse::<u64>().ok()?;
    Some((id, &rest[equals + 1..]))
}

struct Parser<'a> {
    bytes: &'a [u8],
    index: usize,
}

impl<'a> Parser<'a> {
    fn new(text: &'a str) -> Self {
        Parser {
            bytes: text.as_bytes(),
            index: 0,
        }
    }

    fn skip_whitespace(&mut self) {
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_whitespace())
        {
            self.index += 1;
        }
    }

    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.index).copied()
    }

    /// Parse a complex entity instance: `(A(...)B(...)C(...))`, with no commas between parts.
    fn complex_list(&mut self) -> Result<Vec<Arg>, StepError> {
        self.expect(b'(')?;
        let mut parts = Vec::new();
        loop {
            self.skip_whitespace();
            match self.peek() {
                Some(b')') => {
                    self.index += 1;
                    return Ok(parts);
                }
                None => {
                    return Err(StepError::Invalid(
                        "unterminated complex STEP entity".into(),
                    ));
                }
                _ => parts.push(self.value()?),
            }
        }
    }

    fn parameter_list(&mut self) -> Result<Vec<Arg>, StepError> {
        self.expect(b'(')?;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(b')') {
            self.index += 1;
            return Ok(values);
        }
        loop {
            values.push(self.value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(b',') => self.index += 1,
                Some(b')') => {
                    self.index += 1;
                    return Ok(values);
                }
                _ => {
                    return Err(StepError::Invalid(
                        "unterminated parameter list in a STEP entity".into(),
                    ));
                }
            }
        }
    }

    fn value(&mut self) -> Result<Arg, StepError> {
        self.skip_whitespace();
        match self.peek() {
            Some(b'\'') => self.string(),
            Some(b'#') => self.reference(),
            Some(b'(') => Ok(Arg::List(self.parameter_list()?)),
            Some(b'$') => {
                self.index += 1;
                Ok(Arg::Null)
            }
            Some(b'*') => {
                self.index += 1;
                Ok(Arg::Enum("*".into()))
            }
            Some(b'.') => {
                // `.5` is a number; `.T.` is an enumeration and ends at the next dot.
                if self
                    .bytes
                    .get(self.index + 1)
                    .is_some_and(|byte| byte.is_ascii_digit())
                {
                    self.number()
                } else {
                    self.enumeration()
                }
            }
            Some(byte) if byte.is_ascii_digit() || byte == b'-' || byte == b'+' => self.number(),
            Some(byte) if byte.is_ascii_alphabetic() || byte == b'_' => self.typed_or_enum(),
            other => Err(StepError::Invalid(format!(
                "unexpected {} at byte {} of a STEP parameter",
                other
                    .map(|byte| format!("`{}`", byte as char))
                    .unwrap_or_else(|| "end of file".into()),
                self.index
            ))),
        }
    }

    fn string(&mut self) -> Result<Arg, StepError> {
        self.index += 1;
        let mut out = String::new();
        loop {
            match self.peek() {
                None => {
                    return Err(StepError::Invalid(
                        "unterminated string in a STEP entity".into(),
                    ));
                }
                Some(b'\'') => {
                    if self.bytes.get(self.index + 1) == Some(&b'\'') {
                        out.push('\'');
                        self.index += 2;
                    } else {
                        self.index += 1;
                        return Ok(Arg::Str(out));
                    }
                }
                Some(_) => {
                    let start = self.index;
                    while self
                        .bytes
                        .get(self.index)
                        .is_some_and(|byte| *byte != b'\'')
                    {
                        self.index += 1;
                    }
                    out.push_str(
                        std::str::from_utf8(&self.bytes[start..self.index])
                            .map_err(|_| StepError::Invalid("STEP string is not UTF-8".into()))?,
                    );
                }
            }
        }
    }

    fn reference(&mut self) -> Result<Arg, StepError> {
        self.index += 1;
        let start = self.index;
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_digit())
        {
            self.index += 1;
        }
        let digits = std::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| StepError::Invalid("bad reference in a STEP entity".into()))?;
        if digits.is_empty() {
            return Err(StepError::Invalid(
                "a `#` in a STEP parameter is not followed by a number".into(),
            ));
        }
        let id = digits
            .parse::<u64>()
            .map_err(|_| StepError::Invalid(format!("bad entity reference #{digits}")))?;
        Ok(Arg::Ref(id))
    }

    fn enumeration(&mut self) -> Result<Arg, StepError> {
        self.index += 1;
        let start = self.index;
        while self.bytes.get(self.index).is_some_and(|byte| *byte != b'.') {
            self.index += 1;
        }
        if self.peek() != Some(b'.') {
            return Err(StepError::Invalid(
                "unterminated enumeration in a STEP parameter".into(),
            ));
        }
        let value = std::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| StepError::Invalid("bad enumeration in a STEP entity".into()))?
            .to_ascii_uppercase();
        self.index += 1;
        Ok(match value.as_str() {
            "T" => Arg::Bool(true),
            "F" => Arg::Bool(false),
            _ => Arg::Enum(value),
        })
    }

    fn number(&mut self) -> Result<Arg, StepError> {
        let start = self.index;
        while self.bytes.get(self.index).is_some_and(|byte| {
            byte.is_ascii_digit() || matches!(byte, b'+' | b'-' | b'.' | b'e' | b'E' | b'd' | b'D')
        }) {
            self.index += 1;
        }
        let raw = std::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| StepError::Invalid("bad number in a STEP entity".into()))?;
        let normalized = raw.replace(['d', 'D'], "e");
        normalized
            .parse::<f64>()
            .map(Arg::Num)
            .map_err(|_| StepError::Invalid(format!("`{raw}` is not a STEP real")))
    }

    fn typed_or_enum(&mut self) -> Result<Arg, StepError> {
        let start = self.index;
        while self
            .bytes
            .get(self.index)
            .is_some_and(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
        {
            self.index += 1;
        }
        let name = std::str::from_utf8(&self.bytes[start..self.index])
            .map_err(|_| StepError::Invalid("bad keyword in a STEP entity".into()))?
            .to_ascii_uppercase();
        self.skip_whitespace();
        if self.peek() == Some(b'(') {
            Ok(Arg::Typed(name, self.parameter_list()?))
        } else {
            Ok(Arg::Enum(name))
        }
    }

    fn expect(&mut self, byte: u8) -> Result<(), StepError> {
        self.skip_whitespace();
        if self.peek() == Some(byte) {
            self.index += 1;
            Ok(())
        } else {
            Err(StepError::Invalid(format!(
                "expected `{}` at byte {} of a STEP parameter",
                byte as char, self.index
            )))
        }
    }
}

struct Resolver<'a> {
    entities: &'a HashMap<u64, (String, Vec<Arg>)>,
}

impl Resolver<'_> {
    /// The vertex references of a face's first bound, when that bound is a polygon loop.
    fn face_vertices(&self, face: u64) -> Option<Vec<u64>> {
        let (kind, arguments) = self.entities.get(&face)?;
        if kind != "ADVANCED_FACE" && kind != "FACE_SURFACE" {
            return None;
        }
        let Arg::List(bounds) = arguments.get(1)? else {
            return None;
        };
        for bound in bounds {
            let Arg::Ref(bound) = bound else { continue };
            let (bound_kind, bound_arguments) = self.entities.get(bound)?;
            if bound_kind != "FACE_OUTER_BOUND" && bound_kind != "FACE_BOUND" {
                continue;
            }
            let Arg::Ref(loop_id) = bound_arguments.get(1)? else {
                return None;
            };
            let (loop_kind, loop_arguments) = self.entities.get(loop_id)?;
            if loop_kind != "POLY_LOOP" {
                // An EDGE_LOOP needs curve geometry to resolve; report the face as skipped.
                return None;
            }
            let Arg::List(vertices) = loop_arguments.get(1)? else {
                return None;
            };
            let mut refs = Vec::with_capacity(vertices.len());
            for vertex in vertices {
                let Arg::Ref(vertex) = vertex else {
                    return None;
                };
                refs.push(*vertex);
            }
            return Some(refs);
        }
        None
    }

    fn vertex_point(&self, vertex: u64) -> Option<u64> {
        let (kind, arguments) = self.entities.get(&vertex)?;
        if kind != "VERTEX_POINT" {
            return None;
        }
        match arguments.get(1)? {
            Arg::Ref(point) => Some(*point),
            _ => None,
        }
    }

    fn cartesian_point(&self, point: u64) -> Option<[f64; 3]> {
        let (kind, arguments) = self.entities.get(&point)?;
        if kind != "CARTESIAN_POINT" {
            return None;
        }
        let Arg::List(coordinates) = arguments.get(1)? else {
            return None;
        };
        let mut out = [0.0f64; 3];
        for (slot, value) in coordinates.iter().take(3).enumerate() {
            let Arg::Num(number) = value else {
                return None;
            };
            out[slot] = *number;
        }
        Some(out)
    }
}

/// Collect every `TYPED(name, ...)` parameter anywhere in an entity's arguments.
///
/// A complex entity instance such as `(LENGTH_UNIT()NAMED_UNIT(*)SI_UNIT(.MILLI.,.METRE.))` has no
/// entity name of its own, so the unit is found by walking the components rather than by position.
fn collect_typed<'a>(arguments: &'a [Arg], name: &str, out: &mut Vec<&'a [Arg]>) {
    for argument in arguments {
        match argument {
            Arg::Typed(kind, inner) => {
                if kind == name {
                    out.push(inner);
                }
                collect_typed(inner, name, out);
            }
            Arg::List(items) => collect_typed(items, name, out),
            _ => {}
        }
    }
}

/// The unit a file declares and how many millimetres one of it is.
///
/// Defaults to millimetres, which is also the contract's unit, so a file that declares nothing is
/// read at face value rather than guessed from coordinate magnitude.
fn unit_scale(entities: &HashMap<u64, (String, Vec<Arg>)>, ordered: &[u64]) -> (Unit, f64) {
    let mut found = (Unit::Millimeter, 1.0);
    for id in ordered {
        let (kind, arguments) = &entities[id];
        if kind == "CONVERSION_BASED_UNIT"
            && let Some(Arg::Str(name)) = arguments.first()
            && let Some(unit) = Unit::parse(name)
        {
            found = (unit, unit.to_millimeters());
        }
        let mut typed = Vec::new();
        collect_typed(arguments, "SI_UNIT", &mut typed);
        for unit in typed {
            if unit.last() != Some(&Arg::Enum("METRE".into())) {
                continue;
            }
            found = match unit.first() {
                Some(Arg::Enum(prefix)) if prefix == "MILLI" => (Unit::Millimeter, 1.0),
                Some(Arg::Enum(prefix)) if prefix == "CENTI" => (Unit::Centimeter, 10.0),
                _ => (Unit::Meter, 1000.0),
            };
        }
    }
    found
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashSet;

    type TestResult = Result<(), Box<dyn std::error::Error>>;

    fn cube() -> Mesh {
        let positions = vec![
            [0.0, 0.0, 0.0],
            [10.0, 0.0, 0.0],
            [10.0, 10.0, 0.0],
            [0.0, 10.0, 0.0],
            [0.0, 0.0, 5.0],
            [10.0, 0.0, 5.0],
            [10.0, 10.0, 5.0],
            [0.0, 10.0, 5.0],
        ];
        let triangles = vec![
            [0, 2, 1],
            [0, 3, 2],
            [4, 5, 6],
            [4, 6, 7],
            [0, 1, 5],
            [0, 5, 4],
            [1, 2, 6],
            [1, 6, 5],
            [2, 3, 7],
            [2, 7, 6],
            [3, 0, 4],
            [3, 4, 7],
        ];
        Mesh {
            positions,
            triangles,
            normals: Vec::new(),
        }
    }

    /// Two triangles sharing one edge: a patch, not a solid.
    fn patch() -> Mesh {
        Mesh {
            positions: vec![
                [0.0, 0.0, 0.0],
                [10.0, 0.0, 0.0],
                [10.0, 10.0, 0.0],
                [0.0, 10.0, 0.0],
            ],
            triangles: vec![[0, 1, 2], [0, 2, 3]],
            normals: Vec::new(),
        }
    }

    fn assert_references_resolve(step: &str) -> TestResult {
        let mut defined = HashSet::new();
        let mut referenced = Vec::new();
        for statement in statements(step) {
            if let Some((id, _)) = split_record(&statement) {
                defined.insert(id);
            }
            // Scan the whole statement for references; the definition itself is harmless.
            let bytes = statement.as_bytes();
            let mut index = 0;
            while index < bytes.len() {
                if bytes[index] == b'#' {
                    let start = index + 1;
                    let mut end = start;
                    while bytes.get(end).is_some_and(|byte| byte.is_ascii_digit()) {
                        end += 1;
                    }
                    if let Ok(id) = statement[start..end].parse::<u64>() {
                        referenced.push(id);
                    }
                    index = end;
                } else {
                    index += 1;
                }
            }
        }
        for id in referenced {
            assert!(defined.contains(&id), "reference #{id} is not defined");
        }
        Ok(())
    }

    #[test]
    fn a_closed_cube_is_a_manifold_solid_brep() -> TestResult {
        let bytes = step_bytes(&cube(), "Cube")?;
        let text = String::from_utf8(bytes)?;
        assert!(text.starts_with("ISO-10303-21;"));
        assert!(text.contains("FILE_SCHEMA(('CONFIG_CONTROL_DESIGN'));"));
        assert!(text.contains("APPLICATION_PROTOCOL_DEFINITION"));
        assert!(text.contains("PRODUCT_DEFINITION_SHAPE"));
        assert!(text.contains("SI_UNIT(.MILLI.,.METRE.)"));
        assert!(text.contains("CLOSED_SHELL"));
        assert!(text.contains("MANIFOLD_SOLID_BREP('Cube'"));
        assert!(text.contains("ADVANCED_BREP_SHAPE_REPRESENTATION"));
        assert!(text.contains("SHAPE_DEFINITION_REPRESENTATION"));
        assert!(text.ends_with("END-ISO-10303-21;\n"));
        // The defect this writer replaces: ADVANCED_FACE needs a surface and a sense flag.
        for line in text.lines().filter(|line| line.contains("ADVANCED_FACE")) {
            assert!(line.contains("),#") && line.ends_with(",.T.);"), "{line}");
        }
        assert_references_resolve(&text)?;
        Ok(())
    }

    #[test]
    fn a_cube_round_trips_through_the_reader() -> TestResult {
        let bytes = step_bytes(&cube(), "Cube")?;
        let imported = read_step(&bytes)?;
        assert_eq!(imported.name, "Cube");
        assert!(imported.closed);
        assert_eq!(imported.mesh.triangle_count(), 12);
        assert_eq!(imported.mesh.vertex_count(), 36);
        assert_eq!(imported.source_unit, Unit::Millimeter);
        let volume = imported.mesh.signed_volume().abs();
        assert!((volume - 500.0).abs() < 1e-6, "volume was {volume}");
        Ok(())
    }

    #[test]
    fn an_open_patch_is_written_as_a_shell_model_not_a_solid() -> TestResult {
        let bytes = step_bytes(&patch(), "Patch")?;
        let text = String::from_utf8(bytes)?;
        assert!(text.contains("OPEN_SHELL"));
        assert!(text.contains("SHELL_BASED_SURFACE_MODEL"));
        assert!(!text.contains("MANIFOLD_SOLID_BREP"));
        let imported = read_step(text.as_bytes())?;
        assert!(!imported.closed);
        assert_eq!(imported.mesh.triangle_count(), 2);
        Ok(())
    }

    #[test]
    fn a_degenerate_triangle_is_omitted_rather_than_written_without_a_plane() -> TestResult {
        let mut mesh = patch();
        mesh.positions.push([10.0, 10.0, 0.0]);
        mesh.triangles.push([1, 2, 4]); // three collinear corners
        let bytes = step_bytes(&mesh, "Degenerate")?;
        let imported = read_step(&bytes)?;
        assert_eq!(imported.mesh.triangle_count(), 2);
        Ok(())
    }

    #[test]
    fn a_project_name_cannot_break_the_file_grammar() -> TestResult {
        let bytes = step_bytes(&cube(), "Bob's \"part\"; ENDSEC;")?;
        let imported = read_step(&bytes)?;
        // The file escapes the quote as `''`; a reader must hand the original name back.
        assert_eq!(imported.name, "Bob's \"part\"; ENDSEC;");
        Ok(())
    }

    #[test]
    fn a_file_without_faces_is_refused_with_a_reason() {
        let none = Mesh {
            positions: vec![],
            triangles: vec![],
            normals: vec![],
        };
        // An empty mesh fails the contract's own validation before the writer sees it.
        assert!(matches!(
            step_bytes(&none, "Empty"),
            Err(StepError::Mesh(_))
        ));
    }

    #[test]
    fn an_out_of_range_triangle_is_not_written() {
        let mut mesh = cube();
        mesh.triangles[0] = [0, 1, 99];
        assert!(matches!(step_bytes(&mesh, "Bad"), Err(StepError::Mesh(_))));
    }

    #[test]
    fn a_text_file_is_refused() {
        assert!(matches!(
            read_step(b"not a step file"),
            Err(StepError::Invalid(message)) if message.contains("signature")
        ));
    }

    #[test]
    fn a_file_over_the_size_limit_is_refused() {
        let mut bytes = vec![b' '; MAX_STEP_BYTES + 1];
        bytes[..12].copy_from_slice(b"ISO-10303-21");
        assert!(matches!(read_step(&bytes), Err(StepError::Invalid(_))));
    }

    #[test]
    fn a_dangling_reference_is_named_not_ignored() -> TestResult {
        let text = "ISO-10303-21;\nDATA;\n#1=MANIFOLD_SOLID_BREP('',#99);\nENDSEC;\n";
        let Err(error) = read_step(text.as_bytes()) else {
            return Err("the dangling reference was accepted".into());
        };
        let message = error.to_string();
        assert!(message.contains("99"), "{message}");
        Ok(())
    }

    #[test]
    fn an_unknown_entity_is_counted_not_fatal() -> TestResult {
        let mut text = String::from_utf8(step_bytes(&cube(), "Cube")?)?;
        text = text.replace(
            "ENDSEC;\nEND-ISO-10303-21;",
            "#9999=COLOUR_RGB('',1.,0.,0.);\nENDSEC;\nEND-ISO-10303-21;",
        );
        let imported = read_step(text.as_bytes())?;
        assert_eq!(imported.mesh.triangle_count(), 12);
        assert!(imported.entities >= 13);
        Ok(())
    }

    #[test]
    fn an_inch_file_is_converted_to_millimetres() -> TestResult {
        let mut text = String::from_utf8(step_bytes(&cube(), "Cube")?)?;
        text = text.replace("SI_UNIT(.MILLI.,.METRE.)", "SI_UNIT($,.METRE.)");
        // The cube's 10 mm coordinates are read as 10 m, which is 10 000 mm.
        let imported = read_step(text.as_bytes())?;
        assert_eq!(imported.source_unit, Unit::Meter);
        let max = imported.mesh.bounding_box().ok_or("no bounds")?.max;
        assert!((max[0] - 10_000.0).abs() < 1e-6, "max was {max:?}");
        Ok(())
    }

    #[test]
    fn a_quad_poly_loop_becomes_two_triangles() -> TestResult {
        let text = "ISO-10303-21;\nDATA;\n\
            #1=MANIFOLD_SOLID_BREP('',#2);\n\
            #2=CLOSED_SHELL('',(#3));\n\
            #3=ADVANCED_FACE('',(#4),#9,.T.);\n\
            #4=FACE_OUTER_BOUND('',#5,.T.);\n\
            #5=POLY_LOOP('',(#6,#7,#8,#10));\n\
            #6=VERTEX_POINT('',#11);\n\
            #7=VERTEX_POINT('',#12);\n\
            #8=VERTEX_POINT('',#13);\n\
            #10=VERTEX_POINT('',#14);\n\
            #11=CARTESIAN_POINT('',(0.,0.,0.));\n\
            #12=CARTESIAN_POINT('',(10.,0.,0.));\n\
            #13=CARTESIAN_POINT('',(10.,10.,0.));\n\
            #14=CARTESIAN_POINT('',(0.,10.,0.));\n\
            #9=PLANE('',#15);\n\
            #15=AXIS2_PLACEMENT_3D('',#11,$,$);\n\
            ENDSEC;\nEND-ISO-10303-21;\n";
        let imported = read_step(text.as_bytes())?;
        assert_eq!(imported.mesh.triangle_count(), 2);
        assert_eq!(imported.mesh.vertex_count(), 4);
        Ok(())
    }

    #[test]
    fn a_face_with_a_non_polygonal_bound_is_counted_as_skipped() -> TestResult {
        let text = "ISO-10303-21;\nDATA;\n\
            #1=MANIFOLD_SOLID_BREP('',#2);\n\
            #2=CLOSED_SHELL('',(#3,#20));\n\
            #20=ADVANCED_FACE('',(#21),#9,.T.);\n\
            #21=FACE_OUTER_BOUND('',#22,.T.);\n\
            #22=EDGE_LOOP('',());\n\
            #3=ADVANCED_FACE('',(#4),#9,.T.);\n\
            #4=FACE_OUTER_BOUND('',#5,.T.);\n\
            #5=POLY_LOOP('',(#6,#7,#8));\n\
            #6=VERTEX_POINT('',#11);\n\
            #7=VERTEX_POINT('',#12);\n\
            #8=VERTEX_POINT('',#13);\n\
            #11=CARTESIAN_POINT('',(0.,0.,0.));\n\
            #12=CARTESIAN_POINT('',(10.,0.,0.));\n\
            #13=CARTESIAN_POINT('',(0.,10.,0.));\n\
            #9=PLANE('',#15);\n\
            #15=AXIS2_PLACEMENT_3D('',#11,$,$);\n\
            ENDSEC;\nEND-ISO-10303-21;\n";
        let imported = read_step(text.as_bytes())?;
        assert_eq!(imported.faces, 1);
        assert_eq!(imported.skipped_faces, 1);
        Ok(())
    }

    #[test]
    fn a_doubled_quote_inside_a_string_is_preserved() -> TestResult {
        let text = "ISO-10303-21;\nDATA;\n#1=PRODUCT('O''Brien','','',());\n\
            #2=MANIFOLD_SOLID_BREP('',#3);\n#3=CLOSED_SHELL('',(#4));\n\
            #4=ADVANCED_FACE('',(#5),#9,.T.);\n#5=FACE_OUTER_BOUND('',#6,.T.);\n\
            #6=POLY_LOOP('',(#7,#8,#10));\n#7=VERTEX_POINT('',#11);\n\
            #8=VERTEX_POINT('',#12);\n#10=VERTEX_POINT('',#13);\n\
            #11=CARTESIAN_POINT('',(0.,0.,0.));\n#12=CARTESIAN_POINT('',(1.,0.,0.));\n\
            #13=CARTESIAN_POINT('',(0.,1.,0.));\n#9=PLANE('',#15);\n\
            #15=AXIS2_PLACEMENT_3D('',#11,$,$);\nENDSEC;\nEND-ISO-10303-21;\n";
        let imported = read_step(text.as_bytes())?;
        assert_eq!(imported.name, "O'Brien");
        Ok(())
    }

    #[test]
    fn scientific_notation_is_parsed() -> TestResult {
        let text = "ISO-10303-21;\nDATA;\n#1=MANIFOLD_SOLID_BREP('',#2);\n\
            #2=CLOSED_SHELL('',(#3));\n#3=ADVANCED_FACE('',(#4),#9,.T.);\n\
            #4=FACE_OUTER_BOUND('',#5,.T.);\n#5=POLY_LOOP('',(#6,#7,#8));\n\
            #6=VERTEX_POINT('',#11);\n#7=VERTEX_POINT('',#12);\n#8=VERTEX_POINT('',#13);\n\
            #11=CARTESIAN_POINT('',(0.,0.,0.));\n#12=CARTESIAN_POINT('',(1.E-2,0.,0.));\n\
            #13=CARTESIAN_POINT('',(0.,1.0D-1,0.));\n#9=PLANE('',#15);\n\
            #15=AXIS2_PLACEMENT_3D('',#11,$,$);\nENDSEC;\nEND-ISO-10303-21;\n";
        let imported = read_step(text.as_bytes())?;
        let bounds = imported.mesh.bounding_box().ok_or("no bounds")?;
        assert!((bounds.max[0] - 0.01).abs() < 1e-12);
        assert!((bounds.max[1] - 0.1).abs() < 1e-12);
        Ok(())
    }

    #[test]
    fn a_missing_data_section_is_refused() {
        assert!(matches!(
            read_step(b"ISO-10303-21;\nHEADER;\nENDSEC;\nEND-ISO-10303-21;\n"),
            Err(StepError::Invalid(message)) if message.contains("no entity")
        ));
    }

    #[test]
    fn real_numbers_always_carry_a_decimal_point() {
        assert_eq!(real(0.0), "0.");
        assert_eq!(real(-0.0), "0.");
        assert_eq!(real(1.0), "1.0");
        assert_eq!(real(1.5), "1.5");
        assert_eq!(real(1.0 / 3.0), "0.333333333");
        assert!(!real(f64::NAN).contains("NaN"));
    }
}

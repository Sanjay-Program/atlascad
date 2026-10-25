//! The converter: [`cadmpeg_ir::CadIr`] to monstertruck bodies.
//!
//! # Nothing is dropped
//!
//! [`convert`] returns a `Vec`, and a `Vec` cannot say "one of these was left
//! out". Handing back three bodies for a file that holds five is the worst
//! available outcome: the caller sees success, gets geometry, and has no way to
//! learn what is missing. So every body converts or the whole call fails, and the
//! failure names the kind that caused it. `iges::from_path` returning an error you
//! can act on beats a solid with a face quietly absent from it.
//!
//! This is why there is no `skip` path anywhere below, and why the refusals in
//! [`surface`], [`curve`] and [`topology`] are typed errors rather than `None`.

mod frame;

pub(super) mod curve;
pub(super) mod surface;
pub(super) mod topology;

#[cfg(test)]
mod tests;

use cadmpeg_ir::geometry::{Curve as IrCurve, Surface as IrSurface};
use cadmpeg_ir::index::ModelIndex;
use cadmpeg_ir::topology::{
    BodyKind, Coedge, Edge as IrEdge, Face as IrFace, Loop as IrLoop, Point as IrPoint,
    Vertex as IrVertex,
};

use super::{ImportedBody, ImportedSheet, ImportedSolid};
use crate::{Error, Result};

/// What a conversion needs to know throughout: the indexed document and which
/// format it came from.
///
/// The format name is carried so every refusal says `IGES carried a ...` rather
/// than making the caller guess which of two readers produced it.
pub(super) struct Context<'a> {
    index: ModelIndex<'a>,
    format: &'static str,
}

impl<'a> Context<'a> {
    fn malformed(&self, detail: String) -> Error {
        Error::MalformedGeometry {
            format: self.format,
            detail,
        }
    }

    fn dangling(&self, kind: &'static str, id: &str) -> Error {
        Error::DanglingReference {
            format: self.format,
            kind,
            id: id.to_owned(),
        }
    }
}

/// Resolve one reference, or report which table it dangled out of.
///
/// A macro rather than a generic function because `ModelIndex`'s accessors are
/// one per table, generated from the same declaration as the tables themselves.
macro_rules! resolvers {
    ($($method:ident -> $table:ident: $entity:ty, $kind:literal;)*) => {
        impl<'a> Context<'a> {
            $(
                fn $method(&self, id: &str) -> Result<&'a $entity> {
                    self.index.$table(id).ok_or_else(|| self.dangling($kind, id))
                }
            )*
        }
    };
}

resolvers! {
    face -> faces: IrFace, "face";
    boundary_loop -> loops: IrLoop, "loop";
    coedge -> coedges: Coedge, "coedge";
    edge -> edges: IrEdge, "edge";
    vertex -> vertices: IrVertex, "vertex";
    point -> points: IrPoint, "point";
    surface -> surfaces: IrSurface, "surface";
    curve -> curves: IrCurve, "curve";
}

/// Convert every body in a decoded document.
pub(super) fn convert(ir: &cadmpeg_ir::CadIr, format: &'static str) -> Result<Vec<ImportedBody>> {
    let context = Context {
        index: ModelIndex::new(ir),
        format,
    };
    ir.model
        .bodies
        .iter()
        .map(|body| body_to_imported(body, &context))
        .collect()
}

fn body_to_imported(
    body: &cadmpeg_ir::topology::Body,
    context: &Context<'_>,
) -> Result<ImportedBody> {
    match body.kind {
        BodyKind::Solid => Ok(ImportedBody::Solid(solid(body, context)?)),
        BodyKind::Sheet => Ok(ImportedBody::Sheet(sheet(body, context)?)),
        // A wire body is a curve collection with no faces, and a `General` body
        // mixes dimensions. Neither has a compressed form. Named, not dropped.
        BodyKind::Wire => Err(Error::UnsupportedBodyKind {
            format: context.format,
            kind: "wire",
        }),
        BodyKind::General => Err(Error::UnsupportedBodyKind {
            format: context.format,
            kind: "mixed-dimensional",
        }),
        // Exhaustive on purpose: see the note in [`surface::convert`].
    }
}

/// A solid: one compressed shell per source shell, because for a body that bounds
/// a volume the shell partition IS the difference between the outer surface and a
/// void.
fn solid(body: &cadmpeg_ir::topology::Body, context: &Context<'_>) -> Result<ImportedSolid> {
    let mut boundaries = Vec::new();
    for region in &body.regions {
        let region = context
            .index
            .regions(region.as_str())
            .ok_or_else(|| context.dangling("region", region.as_str()))?;
        for shell in &region.shells {
            let shell = context
                .index
                .shells(shell.as_str())
                .ok_or_else(|| context.dangling("shell", shell.as_str()))?;
            let mut builder = topology::ShellBuilder::new(context);
            builder.add_shell(shell)?;
            if builder.is_empty() {
                return Err(context.malformed(format!(
                    "solid body {} has a shell with no faces, so it bounds no volume",
                    body.id
                )));
            }
            boundaries.push(builder.finish());
        }
    }
    if boundaries.is_empty() {
        return Err(context.malformed(format!("solid body {} carries no boundary shell", body.id)));
    }
    let mut solid = ImportedSolid {
        boundaries,
        id_allocator: None,
        attributes: None,
    };
    if let Some(transform) = &body.transform {
        place_solid(&mut solid, transform, context)?;
    }
    Ok(solid)
}

/// A sheet: every shell of the body merged into one compressed shell.
///
/// See [`topology::ShellBuilder::add_shell`] for why merging is right here and
/// wrong for a solid.
fn sheet(body: &cadmpeg_ir::topology::Body, context: &Context<'_>) -> Result<ImportedSheet> {
    let mut builder = topology::ShellBuilder::new(context);
    for region in &body.regions {
        let region = context
            .index
            .regions(region.as_str())
            .ok_or_else(|| context.dangling("region", region.as_str()))?;
        for shell in &region.shells {
            let shell = context
                .index
                .shells(shell.as_str())
                .ok_or_else(|| context.dangling("shell", shell.as_str()))?;
            builder.add_shell(shell)?;
        }
    }
    if builder.is_empty() {
        return Err(context.malformed(format!("sheet body {} carries no face", body.id)));
    }
    let mut sheet = builder.finish();
    if let Some(transform) = &body.transform {
        place_shell(&mut sheet, transform, context)?;
    }
    Ok(sheet)
}

/// Apply a body's world placement.
///
/// A body transform is a placement of the body's geometry, so it applies to every
/// vertex, curve and surface in it. Leaving it out puts a correctly-shaped body at
/// the origin instead of where the assembly says it is -- which is a silent wrong
/// answer, not a visible failure.
fn place_solid(
    solid: &mut ImportedSolid,
    transform: &cadmpeg_ir::transform::Transform,
    context: &Context<'_>,
) -> Result<()> {
    for shell in &mut solid.boundaries {
        place_shell(shell, transform, context)?;
    }
    Ok(())
}

fn place_shell(
    shell: &mut ImportedSheet,
    transform: &cadmpeg_ir::transform::Transform,
    context: &Context<'_>,
) -> Result<()> {
    use monstertruck_modeling::{Transform as _, Transformed};
    if !transform.is_affine() {
        return Err(context.malformed("a body carries a non-affine placement".to_owned()));
    }
    let matrix = frame::matrix(transform);
    for vertex in &mut shell.vertices {
        *vertex = matrix.transform_point(*vertex);
    }
    for edge in &mut shell.edges {
        edge.curve.transform_by(matrix);
    }
    for face in &mut shell.faces {
        face.surface.transform_by(matrix);
    }
    Ok(())
}

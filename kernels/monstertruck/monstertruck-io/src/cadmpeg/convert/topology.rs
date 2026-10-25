//! Topology: the intermediate representation's entity tables to
//! [`CompressedShell`].
//!
//! Both sides are half-edge B-reps with the same nesting -- body, region, shell,
//! face, loop, coedge, edge, vertex -- so this is translation from string ids to
//! array indices, not reconstruction. What it has to get right is orientation and
//! loop order, because both produce shells that look correct and fail much later.
//!
//! # Orientation composes, and each flip is applied exactly once
//!
//! Three things flip a boundary in the source: a face's sense against its
//! surface, a coedge's sense against its edge, and the direction the edge's own
//! parameter interval runs. They do NOT all belong here:
//!
//! * the edge's interval direction is handled where the curve is built
//!   ([`super::curve`]), so a converted edge curve always runs start vertex to end
//!   vertex;
//! * the coedge's sense becomes [`CompressedEdgeIndex::orientation`];
//! * the face's sense becomes [`CompressedFace::orientation`].
//!
//! Applying any of them twice, or in the wrong layer, is the failure mode. The
//! curve is already oriented by the time this module sees it, so nothing here
//! re-reads the edge's interval.
//!
//! # Loop order
//!
//! `Face::try_new` takes the FIRST wire as the outer boundary. The representation
//! says the first loop is outer "conventionally", and separately carries a
//! [`LoopBoundaryRole`]. Convention is not a guarantee, so where the role is
//! stated it is obeyed and the outer loop is moved to the front; where it is not,
//! the source order stands.
//!
//! [`CompressedShell`]: monstertruck_topology::compress::CompressedShell
//! [`CompressedEdgeIndex::orientation`]: monstertruck_topology::compress::CompressedEdgeIndex
//! [`CompressedFace::orientation`]: monstertruck_topology::compress::CompressedFace

use std::collections::HashMap;

use cadmpeg_ir::topology::{Face as IrFace, LoopBoundaryRole, Sense, Shell as IrShell};
use monstertruck_modeling::{Curve, Point3, Surface};
use monstertruck_topology::compress::{
    CompressedEdge, CompressedEdgeIndex, CompressedFace, CompressedShell,
};

use super::{Context, curve, frame, surface};
use crate::Result;

/// A shell under construction, interning vertices and edges as faces reach them.
///
/// Interning is what makes the output a SHARED-edge shell rather than a pile of
/// independent faces: two faces that use the same source edge get the same index,
/// which is the property the boolean kernel's topology depends on.
pub(super) struct ShellBuilder<'a> {
    context: &'a Context<'a>,
    vertices: Vec<Point3>,
    vertex_index: HashMap<String, usize>,
    edges: Vec<CompressedEdge<Curve>>,
    edge_index: HashMap<String, usize>,
    faces: Vec<CompressedFace<Surface>>,
}

impl<'a> ShellBuilder<'a> {
    pub(super) fn new(context: &'a Context<'a>) -> Self {
        Self {
            context,
            vertices: Vec::new(),
            vertex_index: HashMap::new(),
            edges: Vec::new(),
            edge_index: HashMap::new(),
            faces: Vec::new(),
        }
    }

    /// Add every face of one source shell.
    ///
    /// Several source shells may be added to one builder. That is how a sheet body
    /// with more than one shell is carried: [`super::ImportedSheet`] is a single
    /// compressed shell, whose `faces` array is free to hold disconnected
    /// components, and for a body that bounds no volume the shell partition
    /// carries no meaning to lose. A SOLID's shells are never merged this way --
    /// there each one is a separate boundary and the partition is the difference
    /// between a void and an outer surface.
    pub(super) fn add_shell(&mut self, shell: &IrShell) -> Result<()> {
        for face in &shell.faces {
            let face = self.context.face(face.as_str())?;
            let face = self.build_face(face)?;
            self.faces.push(face);
        }
        Ok(())
    }

    pub(super) fn finish(self) -> CompressedShell<Point3, Curve, Surface> {
        CompressedShell {
            vertices: self.vertices,
            edges: self.edges,
            faces: self.faces,
            vertex_stable_ids: None,
            edge_stable_ids: None,
            face_stable_ids: None,
        }
    }

    /// Whether anything was built at all.
    pub(super) fn is_empty(&self) -> bool { self.faces.is_empty() }

    fn build_face(&mut self, face: &IrFace) -> Result<CompressedFace<Surface>> {
        let carrier = self.context.surface(face.surface.as_str())?;
        let surface = surface::convert(&carrier.geometry, self.context)?;
        let mut boundaries = Vec::with_capacity(face.loops.len());
        let mut outer = None;
        for (position, identifier) in face.loops.iter().enumerate() {
            let ring = self.context.boundary_loop(identifier.as_str())?;
            if ring.boundary_role == LoopBoundaryRole::Outer && outer.is_none() {
                outer = Some(position);
            }
            let mut edges = Vec::with_capacity(ring.coedges.len());
            for identifier in &ring.coedges {
                let coedge = self.context.coedge(identifier.as_str())?;
                if coedge.use_curve.is_some() {
                    // A coedge-local 3D carrier means the two faces meeting at
                    // this edge disagree about where it is. `CompressedEdge` holds
                    // ONE curve, so admitting this would silently keep one face's
                    // version and hand the other a boundary it does not lie on.
                    return Err(crate::Error::UnsupportedCurveKind {
                        format: self.context.format,
                        kind: "coedge-local (tolerant edge)",
                    });
                }
                let index = self.intern_edge(coedge.edge.as_str())?;
                edges.push(CompressedEdgeIndex {
                    index,
                    orientation: coedge.sense == Sense::Forward,
                });
            }
            if edges.is_empty() {
                // A loop with no coedges is a surface singularity carried as
                // `vertex_uses` -- a pole, where a whole boundary collapses to one
                // point. monstertruck's compressed boundary is a ring of EDGES
                // with nowhere to put it.
                return Err(crate::Error::UnsupportedSurfaceKind {
                    format: self.context.format,
                    kind: "singular (pole) face boundary",
                });
            }
            boundaries.push(edges);
        }
        if boundaries.is_empty() {
            return Err(self
                .context
                .malformed(format!("face {} carries no boundary loop", face.id)));
        }
        // Move the stated outer loop to the front, where `Face::try_new` expects
        // it. `swap` and not a stable rotation: the inner loops have no order of
        // their own, so the cheapest correct move is the right one.
        if let Some(position) = outer {
            boundaries.swap(0, position);
        }
        Ok(CompressedFace {
            boundaries,
            orientation: face.sense == Sense::Forward,
            surface,
        })
    }

    /// The index of a source edge in this shell, converting it on first use.
    fn intern_edge(&mut self, identifier: &str) -> Result<usize> {
        if let Some(index) = self.edge_index.get(identifier) {
            return Ok(*index);
        }
        let edge = self.context.edge(identifier)?;
        let front = self.intern_vertex(edge.start.as_str())?;
        let back = self.intern_vertex(edge.end.as_str())?;
        let Some(carrier) = &edge.curve else {
            // An edge with no attributed curve is a tolerant or degenerate edge.
            // There is no geometry to convert and inventing a line between the
            // vertices would put a straight boundary where the file declined to
            // say there was one.
            return Err(crate::Error::UnsupportedCurveKind {
                format: self.context.format,
                kind: "absent (tolerant or degenerate edge)",
            });
        };
        let carrier = self.context.curve(carrier.as_str())?;
        let curve = curve::convert(
            &carrier.geometry,
            edge.param_range,
            (self.vertices[front], self.vertices[back]),
            self.context,
        )?;
        let index = self.edges.len();
        self.edges.push(CompressedEdge {
            vertices: (front, back),
            curve,
        });
        self.edge_index.insert(identifier.to_owned(), index);
        Ok(index)
    }

    /// The index of a source vertex in this shell, resolving its point on first
    /// use.
    fn intern_vertex(&mut self, identifier: &str) -> Result<usize> {
        if let Some(index) = self.vertex_index.get(identifier) {
            return Ok(*index);
        }
        let vertex = self.context.vertex(identifier)?;
        let point = self.context.point(vertex.point.as_str())?;
        let index = self.vertices.len();
        self.vertices.push(frame::point(&point.position));
        self.vertex_index.insert(identifier.to_owned(), index);
        Ok(index)
    }
}

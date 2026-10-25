//! Conversion tests over an IN-MEMORY intermediate representation.
//!
//! No IGES file is read here, deliberately. The decoder is cadmpeg's
//! responsibility and is already measured (see the note on [`crate::cadmpeg`]);
//! what is unverified is this crate's mapping, and building the representation
//! directly is the only way to exercise a refusal arm for a carrier no fixture in
//! the world happens to contain.
//!
//! **These tests do not show that reading an IGES file works.** They show that a
//! decoded document converts. The end-to-end path has no test until the repository
//! carries an IGES fixture of its own -- spec 028 slice 3.

use cadmpeg_ir::geometry::{
    Curve as IrCurve, CurveGeometry, NurbsCurve as IrNurbsCurve, Surface as IrSurface,
    SurfaceGeometry,
};
use cadmpeg_ir::math::{Point3 as IrPoint3, Vector3 as IrVector3};
use cadmpeg_ir::topology::{
    Body, BodyKind, Coedge, Edge as IrEdge, Face as IrFace, Loop as IrLoop, LoopBoundaryRole,
    Point as IrPoint, Region, Sense, Shell as IrShell, Vertex as IrVertex,
};
use monstertruck_modeling::{
    BoundedCurve, EuclideanSpace, InnerSpace, MetricSpace, ParametricCurve, ParametricSurface,
    Point3, Surface, Transform as _,
};
use monstertruck_topology::compress::CompressedEdgeIndex;

use super::*;

// ---------------------------------------------------------------------------
// Building a document by hand.
// ---------------------------------------------------------------------------

/// A minimal document under construction.
///
/// Every helper returns the id it just added, so a test reads as the graph it
/// builds rather than as a wall of string literals.
struct Builder {
    ir: cadmpeg_ir::CadIr,
    counter: usize,
}

impl Default for Builder {
    fn default() -> Self {
        Self {
            ir: empty_document(),
            counter: 0,
        }
    }
}

impl Builder {
    fn id(&mut self, prefix: &str) -> String {
        self.counter += 1;
        format!("{prefix}{}", self.counter)
    }

    fn point(&mut self, x: f64, y: f64, z: f64) -> String {
        let id = self.id("point");
        self.ir.model.points.push(IrPoint {
            id: id.clone().into(),
            position: IrPoint3::new(x, y, z),
            source_object: None,
        });
        id
    }

    fn vertex(&mut self, x: f64, y: f64, z: f64) -> String {
        let point = self.point(x, y, z);
        let id = self.id("vertex");
        self.ir.model.vertices.push(IrVertex {
            id: id.clone().into(),
            point: point.into(),
            tolerance: None,
        });
        id
    }

    fn surface(&mut self, geometry: SurfaceGeometry) -> String {
        let id = self.id("surface");
        self.ir.model.surfaces.push(IrSurface {
            id: id.clone().into(),
            geometry,
            source_object: None,
        });
        id
    }

    fn curve(&mut self, geometry: CurveGeometry) -> String {
        let id = self.id("curve");
        self.ir.model.curves.push(IrCurve {
            id: id.clone().into(),
            geometry,
            source_object: None,
        });
        id
    }

    fn edge(
        &mut self,
        curve: Option<&str>,
        start: &str,
        end: &str,
        range: Option<[f64; 2]>,
    ) -> String {
        let id = self.id("edge");
        self.ir.model.edges.push(IrEdge {
            id: id.clone().into(),
            curve: curve.map(Into::into),
            start: start.into(),
            end: end.into(),
            param_range: range,
            tolerance: None,
        });
        id
    }

    /// A closed ring of coedges over `edges`, all forward, on one loop.
    fn ring(&mut self, face: &str, role: LoopBoundaryRole, edges: &[String]) -> String {
        self.ring_with_senses(
            face,
            role,
            &edges
                .iter()
                .map(|edge| (edge.clone(), Sense::Forward))
                .collect::<Vec<_>>(),
        )
    }

    fn ring_with_senses(
        &mut self,
        face: &str,
        role: LoopBoundaryRole,
        edges: &[(String, Sense)],
    ) -> String {
        let loop_id = self.id("loop");
        // The coedge ids have to be known before the ring can be linked, so they
        // are allocated first and the `next`/`previous` cycle closed afterwards.
        let coedge_ids: Vec<String> = (0..edges.len()).map(|_| self.id("coedge")).collect();
        let count = coedge_ids.len();
        for (position, (edge, sense)) in edges.iter().enumerate() {
            self.ir.model.coedges.push(Coedge {
                id: coedge_ids[position].clone().into(),
                owner_loop: loop_id.clone().into(),
                edge: edge.clone().into(),
                next: coedge_ids[(position + 1) % count].clone().into(),
                previous: coedge_ids[(position + count - 1) % count].clone().into(),
                // Self-referential radial ring: a laminar boundary, which is what
                // a sheet's edges are.
                radial_next: coedge_ids[position].clone().into(),
                sense: *sense,
                pcurves: Vec::new(),
                use_curve: None,
                use_curve_parameter_range: None,
            });
        }
        self.ir.model.loops.push(IrLoop {
            id: loop_id.clone().into(),
            face: face.into(),
            boundary_role: role,
            coedges: coedge_ids.into_iter().map(Into::into).collect(),
            vertex_uses: Vec::new(),
        });
        loop_id
    }

    /// A one-face body of the given kind, with the loops already built against
    /// the face id this returns.
    fn body(&mut self, kind: BodyKind, faces: Vec<IrFace>) -> String {
        let body = self.id("body");
        let region = self.id("region");
        let shell = self.id("shell");
        let face_ids = faces.iter().map(|face| face.id.clone()).collect();
        self.ir.model.faces.extend(faces);
        self.ir.model.shells.push(IrShell {
            id: shell.clone().into(),
            region: region.clone().into(),
            faces: face_ids,
            wire_edges: Vec::new(),
            free_vertices: Vec::new(),
        });
        self.ir.model.regions.push(Region {
            id: region.clone().into(),
            body: body.clone().into(),
            shells: vec![shell.into()],
        });
        self.ir.model.bodies.push(Body {
            id: body.clone().into(),
            kind,
            regions: vec![region.into()],
            transform: None,
            name: None,
            color: None,
            visible: None,
        });
        body
    }

    fn convert(&self) -> Result<Vec<ImportedBody>> { convert(&self.ir, "TEST") }
}

/// The z = 0 plane, x-aligned.
fn xy_plane() -> SurfaceGeometry {
    SurfaceGeometry::Plane {
        origin: IrPoint3::new(0.0, 0.0, 0.0),
        normal: IrVector3::new(0.0, 0.0, 1.0),
        u_axis: IrVector3::new(1.0, 0.0, 0.0),
    }
}

/// An infinite line through `from` towards `to`, parameterised by DISTANCE, so an
/// edge on it spans `[0, |to - from|]`.
fn line_through(from: (f64, f64, f64), to: (f64, f64, f64)) -> (CurveGeometry, f64) {
    let direction = IrVector3::new(to.0 - from.0, to.1 - from.1, to.2 - from.2);
    let length =
        (direction.x * direction.x + direction.y * direction.y + direction.z * direction.z).sqrt();
    (
        CurveGeometry::Line {
            origin: IrPoint3::new(from.0, from.1, from.2),
            direction: IrVector3::new(
                direction.x / length,
                direction.y / length,
                direction.z / length,
            ),
        },
        length,
    )
}

/// A triangular sheet face on the z = 0 plane: the smallest thing that is a real
/// face rather than a fragment.
fn triangle_sheet() -> Builder {
    let mut builder = Builder::default();
    let corners = [(0.0, 0.0), (4.0, 0.0), (0.0, 3.0)];
    let vertices: Vec<String> = corners
        .iter()
        .map(|(x, y)| builder.vertex(*x, *y, 0.0))
        .collect();
    let surface = builder.surface(xy_plane());
    let face_id = builder.id("face");
    let mut edges = Vec::new();
    for index in 0..3 {
        let (from, to) = (corners[index], corners[(index + 1) % 3]);
        let (geometry, length) = line_through((from.0, from.1, 0.0), (to.0, to.1, 0.0));
        let curve = builder.curve(geometry);
        edges.push(builder.edge(
            Some(&curve),
            &vertices[index],
            &vertices[(index + 1) % 3],
            Some([0.0, length]),
        ));
    }
    let ring = builder.ring(&face_id, LoopBoundaryRole::Outer, &edges);
    let shell_placeholder = builder.id("shell-unused");
    builder.body(
        BodyKind::Sheet,
        vec![IrFace {
            id: face_id.into(),
            // Rewritten by `body`; the shell id is not read by the converter,
            // which walks body -> region -> shell -> face rather than back up.
            shell: shell_placeholder.into(),
            surface: surface.into(),
            sense: Sense::Forward,
            loops: vec![ring.into()],
            name: None,
            color: None,
            tolerance: None,
        }],
    );
    builder
}

// ---------------------------------------------------------------------------
// The whole path, for the shapes that are supposed to work.
// ---------------------------------------------------------------------------

/// The end-to-end claim for slices 1 and 2: a decoded sheet face becomes a
/// compressed shell whose vertices, edges and surface are the ones the document
/// described.
#[test]
fn a_planar_sheet_face_becomes_a_compressed_shell() {
    let bodies = triangle_sheet().convert().expect("the triangle converts");
    assert_eq!(bodies.len(), 1);
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        panic!("a sheet body must convert to a sheet, not {:?}", bodies[0]);
    };
    assert_eq!(sheet.faces.len(), 1);
    // Three edges and three vertices, each SHARED: the ring visits vertex 0
    // twice, once as an end and once as a start, and interning is what makes
    // those the same index.
    assert_eq!(sheet.edges.len(), 3);
    assert_eq!(sheet.vertices.len(), 3);
    assert_eq!(sheet.faces[0].boundaries.len(), 1);
    assert_eq!(sheet.faces[0].boundaries[0].len(), 3);
    assert!(sheet.faces[0].orientation);
    // The plane is the source plane: normal +z through the origin.
    let Surface::Plane(plane) = &sheet.faces[0].surface else {
        panic!(
            "a plane must convert to a plane, not {:?}",
            sheet.faces[0].surface
        );
    };
    assert!(plane.normal().z > 0.999, "normal was {:?}", plane.normal());
    assert!(plane.origin().to_vec().magnitude() < 1.0e-12);
}

/// The invariant `Edge::try_new` will check later: an edge's curve runs from its
/// start vertex to its end vertex, at the curve's own parameter bounds.
///
/// This is the one that silently breaks if the trim is applied in the wrong frame
/// or the interval is read as normalised.
#[test]
fn every_converted_edge_curve_runs_from_its_start_vertex_to_its_end_vertex() {
    let bodies = triangle_sheet().convert().expect("the triangle converts");
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        unreachable!()
    };
    for (index, edge) in sheet.edges.iter().enumerate() {
        let (front, back) = (
            sheet.vertices[edge.vertices.0],
            sheet.vertices[edge.vertices.1],
        );
        let (start, end) = edge.curve.range_tuple();
        assert!(
            edge.curve.subs(start).distance(front) < 1.0e-9,
            "edge {index} starts at {:?}, not at its start vertex {front:?}",
            edge.curve.subs(start)
        );
        assert!(
            edge.curve.subs(end).distance(back) < 1.0e-9,
            "edge {index} ends at {:?}, not at its end vertex {back:?}",
            edge.curve.subs(end)
        );
    }
}

/// The strongest oracle available here: monstertruck's OWN extraction accepts the
/// converted shell.
///
/// `Shell::extract` runs `Edge::try_new` on every edge and `Face::try_new` on
/// every face, so it checks the endpoint agreement and the boundary-wire rules
/// that the hand-written assertions above only sample. A converter that produced
/// a plausible-looking shell with a curve running the wrong way, or a ring that
/// does not close, fails here and passes everything else.
#[test]
fn the_converted_shell_is_accepted_by_monstertrucks_own_extraction() {
    use monstertruck_topology::Shell;
    let bodies = triangle_sheet().convert().expect("the triangle converts");
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        unreachable!()
    };
    let shell = Shell::extract(sheet.clone()).expect("the shell extracts");
    assert_eq!(shell.len(), 1, "one face in, one face out");
    let face = &shell[0];
    assert_eq!(face.absolute_boundaries().len(), 1);
    assert_eq!(face.absolute_boundaries()[0].len(), 3);
    // A closed ring: `Wire::is_closed` is the property that fails if the coedge
    // order or an edge's direction was mishandled.
    assert!(
        face.absolute_boundaries()[0].is_closed(),
        "the converted boundary must be a closed wire"
    );
}

/// A reversed coedge must show up as an orientation flag, NOT as a reversed curve:
/// the curve is shared between the faces that meet at the edge, and flipping it
/// would flip it for both.
#[test]
fn a_reversed_coedge_flips_the_edge_use_and_not_the_shared_curve() {
    let mut builder = Builder::default();
    let start = builder.vertex(0.0, 0.0, 0.0);
    let end = builder.vertex(2.0, 0.0, 0.0);
    let (geometry, length) = line_through((0.0, 0.0, 0.0), (2.0, 0.0, 0.0));
    let curve = builder.curve(geometry);
    let edge = builder.edge(Some(&curve), &start, &end, Some([0.0, length]));
    let surface = builder.surface(xy_plane());
    let face_id = builder.id("face");
    // A two-coedge ring over ONE edge: degenerate as a face, but it is the
    // smallest graph that carries both senses of the same edge.
    let ring = builder.ring_with_senses(
        &face_id,
        LoopBoundaryRole::Outer,
        &[(edge.clone(), Sense::Forward), (edge, Sense::Reversed)],
    );
    let placeholder = builder.id("shell-unused");
    builder.body(
        BodyKind::Sheet,
        vec![IrFace {
            id: face_id.into(),
            shell: placeholder.into(),
            surface: surface.into(),
            sense: Sense::Forward,
            loops: vec![ring.into()],
            name: None,
            color: None,
            tolerance: None,
        }],
    );
    let bodies = builder.convert().expect("the two-sense ring converts");
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        unreachable!()
    };
    assert_eq!(sheet.edges.len(), 1, "one source edge must intern once");
    let uses = &sheet.faces[0].boundaries[0];
    assert_eq!(uses.len(), 2);
    assert!(uses[0].orientation);
    assert!(!uses[1].orientation);
    assert_eq!(uses[0].index, uses[1].index);
    // The shared curve still runs start-to-end, untouched by the reversed use.
    let (start_parameter, _) = sheet.edges[0].curve.range_tuple();
    assert!(
        sheet.edges[0]
            .curve
            .subs(start_parameter)
            .distance(Point3::origin())
            < 1.0e-12
    );
}

/// A face whose sense is reversed becomes a face with `orientation == false`,
/// which is where `create_face` inverts it. The surface itself must NOT be
/// pre-inverted, or the two flips cancel.
#[test]
fn a_reversed_face_sense_becomes_the_faces_orientation_flag() {
    let mut builder = triangle_sheet();
    builder.ir.model.faces[0].sense = Sense::Reversed;
    let bodies = builder.convert().expect("the triangle converts");
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        unreachable!()
    };
    assert!(!sheet.faces[0].orientation);
    let Surface::Plane(plane) = &sheet.faces[0].surface else {
        unreachable!()
    };
    assert!(
        plane.normal().z > 0.999,
        "the surface must be untouched; the flip belongs to the face"
    );
}

/// The stated outer loop reaches position zero, where `Face::try_new` reads the
/// outer boundary -- even when the source lists it second.
#[test]
fn a_stated_outer_loop_is_moved_to_the_front() {
    let mut builder = triangle_sheet();
    // Give the face a second loop, marked inner, and list it FIRST.
    let inner = {
        let corners = [(1.0, 1.0), (2.0, 1.0), (1.0, 1.5)];
        let vertices: Vec<String> = corners
            .iter()
            .map(|(x, y)| builder.vertex(*x, *y, 0.0))
            .collect();
        let face_id = builder.ir.model.faces[0].id.to_string();
        let mut edges = Vec::new();
        for index in 0..3 {
            let (from, to) = (corners[index], corners[(index + 1) % 3]);
            let (geometry, length) = line_through((from.0, from.1, 0.0), (to.0, to.1, 0.0));
            let curve = builder.curve(geometry);
            edges.push(builder.edge(
                Some(&curve),
                &vertices[index],
                &vertices[(index + 1) % 3],
                Some([0.0, length]),
            ));
        }
        builder.ring(&face_id, LoopBoundaryRole::Inner, &edges)
    };
    let outer = builder.ir.model.faces[0].loops[0].clone();
    builder.ir.model.faces[0].loops = vec![inner.into(), outer.clone()];
    let bodies = builder.convert().expect("the two-loop face converts");
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        unreachable!()
    };
    assert_eq!(sheet.faces[0].boundaries.len(), 2);
    // Identified by GEOMETRY, not by edge index. Edges are interned in traversal
    // order, so the inner ring -- listed first by the source -- takes indices 0..3
    // and the outer one 3..6. Asserting on those numbers would pin the interning
    // order rather than the loop order, and would read as passing for the wrong
    // reason.
    let touches_origin = |boundary: &[CompressedEdgeIndex]| {
        boundary.iter().any(|use_of| {
            let edge = &sheet.edges[use_of.index];
            sheet.vertices[edge.vertices.0].to_vec().magnitude() < 1.0e-12
                || sheet.vertices[edge.vertices.1].to_vec().magnitude() < 1.0e-12
        })
    };
    // Only the outer triangle has a corner at the origin.
    assert!(
        touches_origin(&sheet.faces[0].boundaries[0]),
        "position zero must hold the loop the source marked outer"
    );
    assert!(
        !touches_origin(&sheet.faces[0].boundaries[1]),
        "the inner loop must not have been left in front"
    );
}

// ---------------------------------------------------------------------------
// Analytic carriers.
// ---------------------------------------------------------------------------

/// A cylinder must keep its analytic form. Landing it on a spline net is the
/// silent loss of exactness this converter exists to avoid.
#[test]
fn a_cylinder_stays_a_surface_of_revolution() {
    let mut builder = Builder::default();
    let surface = builder.surface(SurfaceGeometry::Cylinder {
        origin: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(1.0, 0.0, 0.0),
        radius: 5.0,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let converted = surface::convert(&carrier, &context).expect("a cylinder converts");
    let Surface::RevolutionSurface(revolution) = &converted else {
        panic!("a cylinder must stay analytic, got {converted:?}");
    };
    // On the surface: every point at parameter (u, v) is `radius` from the axis.
    for (u, v) in [(0.0, 0.0), (1.0, 0.5), (2.5, 1.0)] {
        let point = revolution.subs(u, v);
        let radial = (point.x * point.x + point.y * point.y).sqrt();
        assert!(
            (radial - 5.0).abs() < 1.0e-9,
            "point at ({u}, {v}) sits {radial} from the axis, not 5"
        );
    }
    let _ = surface;
}

/// A sphere keeps the analytic variant, which is what preserves the closed-form
/// parameter division -- spec 012 U1.2.
#[test]
fn a_sphere_stays_analytic_and_carries_its_radius() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Sphere {
        center: IrPoint3::new(1.0, -2.0, 3.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(1.0, 0.0, 0.0),
        radius: 7.0,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let converted = surface::convert(&carrier, &context).expect("a sphere converts");
    let Surface::SphericalSurface(sphere) = &converted else {
        panic!("a sphere must stay analytic, got {converted:?}");
    };
    assert_eq!(sphere.entity().radius(), 7.0);
    assert_eq!(sphere.entity().center(), Point3::new(1.0, -2.0, 3.0));
}

/// A torus's axis DOES change its point set, so unlike a sphere's the frame has to
/// survive. Measured on the surface, not on the stored transform.
#[test]
fn a_torus_keeps_the_axis_it_was_given() {
    let mut builder = Builder::default();
    // Axis along +x, so the tube ring lies in the y-z plane.
    builder.surface(SurfaceGeometry::Torus {
        center: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(1.0, 0.0, 0.0),
        ref_direction: IrVector3::new(0.0, 1.0, 0.0),
        major_radius: 10.0,
        minor_radius: 2.0,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let converted = surface::convert(&carrier, &context).expect("a torus converts");
    let Surface::ToroidalSurface(torus) = &converted else {
        panic!("a torus must stay analytic, got {converted:?}");
    };
    // Every point is `minor` away from the ring of radius `major` in the plane
    // PERPENDICULAR to +x, which is the claim that the axis was honoured. Were the
    // axis dropped, the ring would lie in the x-y plane and this would fail.
    for (u, v) in [(0.0, 0.0), (1.0, 2.0), (3.0, 4.5)] {
        let point = torus.subs(u, v);
        let ring_distance = (point.y * point.y + point.z * point.z).sqrt();
        let to_tube_centre = ((ring_distance - 10.0).powi(2) + point.x * point.x).sqrt();
        assert!(
            (to_tube_centre - 2.0).abs() < 1.0e-9,
            "point at ({u}, {v}) sits {to_tube_centre} from the tube centre, not 2"
        );
    }
}

/// A circular edge becomes an EXACT rational arc between its vertices, at the
/// angles the file stated.
#[test]
fn a_circular_edge_becomes_an_exact_arc_between_its_vertices() {
    use std::f64::consts::FRAC_PI_2;
    let geometry = CurveGeometry::Circle {
        center: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(1.0, 0.0, 0.0),
        radius: 3.0,
    };
    let ir = empty_document();
    let context = context_over(&ir);
    // A quarter turn from the reference direction: (3, 0, 0) to (0, 3, 0).
    let converted = curve::convert(
        &geometry,
        Some([0.0, FRAC_PI_2]),
        (Point3::new(3.0, 0.0, 0.0), Point3::new(0.0, 3.0, 0.0)),
        &context,
    )
    .expect("a quarter arc converts");
    let (start, end) = converted.range_tuple();
    assert!(converted.subs(start).distance(Point3::new(3.0, 0.0, 0.0)) < 1.0e-12);
    assert!(converted.subs(end).distance(Point3::new(0.0, 3.0, 0.0)) < 1.0e-12);
    // Exactness, not closeness: a rational quadratic IS a circular arc, so every
    // interior point is on the circle to floating-point noise and not to a
    // chordal tolerance.
    for step in 1..8 {
        let t = start + (end - start) * f64::from(step) / 8.0;
        let point = converted.subs(t);
        let radial = (point.x * point.x + point.y * point.y).sqrt();
        assert!(
            (radial - 3.0).abs() < 1.0e-12,
            "the arc leaves its circle by {} at t = {t}",
            (radial - 3.0).abs()
        );
        assert!(point.z.abs() < 1.0e-12);
    }
}

/// A free-form edge is restricted to the knot interval the file stated, and the
/// restriction keeps the curve's own parameterization.
#[test]
fn a_nurbs_edge_is_restricted_to_its_stated_knot_interval() {
    let geometry = CurveGeometry::Nurbs(IrNurbsCurve {
        degree: 1,
        knots: vec![0.0, 0.0, 1.0, 2.0, 2.0],
        control_points: vec![
            IrPoint3::new(0.0, 0.0, 0.0),
            IrPoint3::new(1.0, 0.0, 0.0),
            IrPoint3::new(2.0, 0.0, 0.0),
        ],
        weights: None,
        periodic: false,
    });
    let ir = empty_document();
    let context = context_over(&ir);
    let full = curve::convert(
        &geometry,
        None,
        (Point3::origin(), Point3::new(2.0, 0.0, 0.0)),
        &context,
    )
    .expect("the polyline spline converts");
    assert_eq!(full.range_tuple(), (0.0, 2.0));
    let restricted = curve::convert(
        &geometry,
        Some([0.5, 1.5]),
        (Point3::new(0.5, 0.0, 0.0), Point3::new(1.5, 0.0, 0.0)),
        &context,
    )
    .expect("the restricted span converts");
    let (start, end) = restricted.range_tuple();
    assert!((start - 0.5).abs() < 1.0e-12, "range started at {start}");
    assert!((end - 1.5).abs() < 1.0e-12, "range ended at {end}");
    assert!(restricted.subs(start).distance(Point3::new(0.5, 0.0, 0.0)) < 1.0e-12);
    assert!(restricted.subs(end).distance(Point3::new(1.5, 0.0, 0.0)) < 1.0e-12);
}

/// An interval that runs BACKWARDS along the carrier gives a curve that still
/// starts at the edge's start vertex. Leaving this to the coedge's sense would
/// double-flip the second use of the same edge.
#[test]
fn a_backwards_interval_gives_a_curve_that_still_starts_at_the_start_vertex() {
    let geometry = CurveGeometry::Nurbs(IrNurbsCurve {
        degree: 1,
        knots: vec![0.0, 0.0, 1.0, 2.0, 2.0],
        control_points: vec![
            IrPoint3::new(0.0, 0.0, 0.0),
            IrPoint3::new(1.0, 0.0, 0.0),
            IrPoint3::new(2.0, 0.0, 0.0),
        ],
        weights: None,
        periodic: false,
    });
    let ir = empty_document();
    let context = context_over(&ir);
    let converted = curve::convert(
        &geometry,
        Some([1.5, 0.5]),
        (Point3::new(1.5, 0.0, 0.0), Point3::new(0.5, 0.0, 0.0)),
        &context,
    )
    .expect("the reversed span converts");
    let (start, end) = converted.range_tuple();
    assert!(
        converted.subs(start).distance(Point3::new(1.5, 0.0, 0.0)) < 1.0e-12,
        "started at {:?}",
        converted.subs(start)
    );
    assert!(converted.subs(end).distance(Point3::new(0.5, 0.0, 0.0)) < 1.0e-12);
}

/// A rational free-form curve keeps its weights: the converted control points are
/// homogeneous, so the curve is the source curve and not its polynomial shadow.
#[test]
fn a_rational_nurbs_edge_keeps_its_weights() {
    use std::f64::consts::FRAC_1_SQRT_2;
    // The standard rational quadratic quarter circle of radius 1.
    let geometry = CurveGeometry::Nurbs(IrNurbsCurve {
        degree: 2,
        knots: vec![0.0, 0.0, 0.0, 1.0, 1.0, 1.0],
        control_points: vec![
            IrPoint3::new(1.0, 0.0, 0.0),
            IrPoint3::new(1.0, 1.0, 0.0),
            IrPoint3::new(0.0, 1.0, 0.0),
        ],
        weights: Some(vec![1.0, FRAC_1_SQRT_2, 1.0]),
        periodic: false,
    });
    let ir = empty_document();
    let context = context_over(&ir);
    let converted = curve::convert(
        &geometry,
        None,
        (Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)),
        &context,
    )
    .expect("the rational quarter circle converts");
    // If the weights were dropped, the midpoint would be the Bezier midpoint
    // (0.75, 0.75) at radius 1.06, not on the unit circle.
    let point = converted.subs(0.5);
    let radial = (point.x * point.x + point.y * point.y).sqrt();
    assert!(
        (radial - 1.0).abs() < 1.0e-12,
        "the midpoint sits at radius {radial}, so the weights were lost"
    );
}

// ---------------------------------------------------------------------------
// Refusals. Every arm below is a path that must FAIL, and be seen to fail.
// ---------------------------------------------------------------------------

/// The invariant the whole converter is built around: a body it cannot represent
/// fails the call by name. It is never dropped from the returned list.
#[test]
fn a_wire_body_is_refused_by_name_and_not_dropped() {
    let mut builder = triangle_sheet();
    // A second body, this one a wire, alongside a sheet that converts fine.
    builder.body(BodyKind::Wire, Vec::new());
    let error = builder
        .convert()
        .expect_err("a wire body must fail the call");
    assert!(
        matches!(error, Error::UnsupportedBodyKind { kind: "wire", .. }),
        "expected a named wire refusal, got {error}"
    );
}

#[test]
fn a_mixed_dimensional_body_is_refused_by_name() {
    let mut builder = Builder::default();
    builder.body(BodyKind::General, Vec::new());
    let error = builder.convert().expect_err("a general body must fail");
    assert!(
        matches!(
            error,
            Error::UnsupportedBodyKind {
                kind: "mixed-dimensional",
                ..
            }
        ),
        "got {error}"
    );
}

/// A spindle torus must not reach the analytic variant: its forward map is exact
/// while `search_parameter` is wrong over roughly a third of the domain, and the
/// inverse is the direction that places face trims. Spec 011 T1.
#[test]
fn a_spindle_torus_is_refused_rather_than_routed_analytically() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Torus {
        center: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(1.0, 0.0, 0.0),
        // Minor exceeds major: the tube swallows the axis and self-intersects.
        major_radius: 1.0,
        minor_radius: 4.0,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let error = surface::convert(&carrier, &context).expect_err("a spindle must be refused");
    assert!(
        matches!(
            error,
            Error::UnsupportedSurfaceKind {
                kind: "degenerate torus",
                ..
            }
        ),
        "got {error}"
    );
}

/// An elliptical cone is not a surface of revolution, so it must not silently
/// become the circular cone that `ratio == 1` would have described.
#[test]
fn an_elliptical_cone_is_refused_rather_than_rounded_to_a_circular_one() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Cone {
        origin: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(1.0, 0.0, 0.0),
        radius: 2.0,
        ratio: 0.5,
        half_angle: 0.3,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let error = surface::convert(&carrier, &context).expect_err("an ellipse must be refused");
    assert!(
        matches!(
            error,
            Error::UnsupportedSurfaceKind {
                kind: "elliptical cone",
                ..
            }
        ),
        "got {error}"
    );
}

/// A tessellation with a chordal bound must not be presented as exact geometry.
#[test]
fn a_source_native_polygonal_surface_is_refused() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Polygonal {
        vertices: vec![
            IrPoint3::new(0.0, 0.0, 0.0),
            IrPoint3::new(1.0, 0.0, 0.0),
            IrPoint3::new(0.0, 1.0, 0.0),
        ],
        triangles: vec![[0, 1, 2]],
        chordal_deflection: 0.01,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let error = surface::convert(&carrier, &context).expect_err("a mesh must be refused");
    assert!(
        matches!(
            error,
            Error::UnsupportedSurfaceKind {
                kind: "source-native polygonal",
                ..
            }
        ),
        "got {error}"
    );
}

/// A reference direction parallel to the axis leaves the azimuth undetermined, so
/// there is no frame to build and no sensible default to pick.
#[test]
fn a_degenerate_frame_is_refused_as_malformed() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Cylinder {
        origin: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(0.0, 0.0, 1.0),
        radius: 1.0,
    });
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let error = surface::convert(&carrier, &context).expect_err("a parallel reference must fail");
    assert!(
        matches!(error, Error::MalformedGeometry { .. }),
        "got {error}"
    );
}

/// A knot vector that does not match its control net is a defect in the file, and
/// must not reach `BsplineSurface::new`, which panics on it.
#[test]
fn a_nurbs_surface_with_a_short_knot_vector_is_refused_and_does_not_panic() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Nurbs(cadmpeg_ir::geometry::NurbsSurface {
        u_degree: 1,
        v_degree: 1,
        // Both need four knots for two poles at degree one; u gets three.
        u_knots: vec![0.0, 0.0, 1.0],
        v_knots: vec![0.0, 0.0, 1.0, 1.0],
        u_count: 2,
        v_count: 2,
        control_points: vec![
            IrPoint3::new(0.0, 0.0, 0.0),
            IrPoint3::new(0.0, 1.0, 0.0),
            IrPoint3::new(1.0, 0.0, 0.0),
            IrPoint3::new(1.0, 1.0, 0.0),
        ],
        weights: None,
        u_periodic: false,
        v_periodic: false,
    }));
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let error = surface::convert(&carrier, &context).expect_err("a short knot vector must fail");
    assert!(
        matches!(error, Error::MalformedGeometry { .. }),
        "got {error}"
    );
}

/// A periodic net would need control points the file did not send, so it is
/// refused rather than built with its seam in the wrong place.
#[test]
fn a_periodic_nurbs_surface_is_refused() {
    let mut builder = Builder::default();
    builder.surface(SurfaceGeometry::Nurbs(cadmpeg_ir::geometry::NurbsSurface {
        u_degree: 1,
        v_degree: 1,
        u_knots: vec![0.0, 1.0, 2.0],
        v_knots: vec![0.0, 0.0, 1.0, 1.0],
        u_count: 2,
        v_count: 2,
        control_points: vec![
            IrPoint3::new(0.0, 0.0, 0.0),
            IrPoint3::new(0.0, 1.0, 0.0),
            IrPoint3::new(1.0, 0.0, 0.0),
            IrPoint3::new(1.0, 1.0, 0.0),
        ],
        weights: None,
        u_periodic: true,
        v_periodic: false,
    }));
    let carrier = builder.ir.model.surfaces[0].geometry.clone();
    let context = context_over(&builder.ir);
    let error = surface::convert(&carrier, &context).expect_err("a periodic net must fail");
    assert!(
        matches!(
            error,
            Error::UnsupportedSurfaceKind {
                kind: "periodic NURBS",
                ..
            }
        ),
        "got {error}"
    );
}

/// An ellipse is exactly representable in principle but has no builder here, so it
/// is refused by name rather than approximated.
#[test]
fn an_elliptical_edge_is_refused_by_name() {
    let geometry = CurveGeometry::Ellipse {
        center: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        major_direction: IrVector3::new(1.0, 0.0, 0.0),
        major_radius: 3.0,
        minor_radius: 2.0,
    };
    let ir = empty_document();
    let context = context_over(&ir);
    let error = curve::convert(
        &geometry,
        Some([0.0, 1.0]),
        (Point3::new(3.0, 0.0, 0.0), Point3::new(0.0, 2.0, 0.0)),
        &context,
    )
    .expect_err("an ellipse must be refused");
    assert!(
        matches!(
            error,
            Error::UnsupportedCurveKind {
                kind: "elliptical",
                ..
            }
        ),
        "got {error}"
    );
}

/// A circle with no stated arc is ambiguous between two arcs, so guessing is
/// refused.
#[test]
fn a_circular_edge_with_no_parameter_range_is_refused() {
    let geometry = CurveGeometry::Circle {
        center: IrPoint3::new(0.0, 0.0, 0.0),
        axis: IrVector3::new(0.0, 0.0, 1.0),
        ref_direction: IrVector3::new(1.0, 0.0, 0.0),
        radius: 1.0,
    };
    let ir = empty_document();
    let context = context_over(&ir);
    let error = curve::convert(
        &geometry,
        None,
        (Point3::new(1.0, 0.0, 0.0), Point3::new(0.0, 1.0, 0.0)),
        &context,
    )
    .expect_err("an untrimmed circle must be refused");
    assert!(
        matches!(error, Error::MalformedGeometry { .. }),
        "got {error}"
    );
}

/// An edge with no curve is a tolerant or degenerate edge. Inventing a straight
/// line between its vertices would put a boundary where the file declined to say
/// there was one.
#[test]
fn an_edge_with_no_curve_is_refused_rather_than_straightened() {
    let mut builder = triangle_sheet();
    builder.ir.model.edges[0].curve = None;
    let error = builder.convert().expect_err("a curveless edge must fail");
    assert!(
        matches!(
            error,
            Error::UnsupportedCurveKind {
                kind: "absent (tolerant or degenerate edge)",
                ..
            }
        ),
        "got {error}"
    );
}

/// A coedge-local carrier means the two faces at an edge disagree about where it
/// is, and `CompressedEdge` holds one curve. Keeping either face's version
/// silently strands the other.
#[test]
fn a_coedge_local_carrier_is_refused() {
    let mut builder = triangle_sheet();
    let curve = builder.ir.model.curves[0].id.clone();
    builder.ir.model.coedges[0].use_curve = Some(curve.to_string().into());
    let error = builder.convert().expect_err("a tolerant coedge must fail");
    assert!(
        matches!(
            error,
            Error::UnsupportedCurveKind {
                kind: "coedge-local (tolerant edge)",
                ..
            }
        ),
        "got {error}"
    );
}

/// A pole loop is a whole boundary collapsed to one point at a surface
/// singularity. A ring of edge indices cannot express it.
#[test]
fn a_singular_pole_boundary_is_refused() {
    let mut builder = triangle_sheet();
    builder.ir.model.loops[0].coedges.clear();
    let error = builder.convert().expect_err("a pole loop must fail");
    assert!(
        matches!(
            error,
            Error::UnsupportedSurfaceKind {
                kind: "singular (pole) face boundary",
                ..
            }
        ),
        "got {error}"
    );
}

/// A reference with no target means the decoded document is not closed. Converting
/// the closed part of it would hand back a body with a hole in it.
#[test]
fn a_dangling_reference_names_the_table_it_dangled_out_of() {
    let mut builder = triangle_sheet();
    builder.ir.model.faces[0].surface = "no-such-surface".into();
    let error = builder.convert().expect_err("a dangling surface must fail");
    assert!(
        matches!(
            error,
            Error::DanglingReference {
                kind: "surface",
                ..
            }
        ),
        "got {error}"
    );
    assert!(
        error.to_string().contains("no-such-surface"),
        "the message must name the id, got {error}"
    );
}

/// A document with no body at all is a fact about the file, reported as such.
#[test]
fn an_empty_document_reports_no_geometry() {
    let ir = empty_document();
    let error = crate::cadmpeg::to_bodies(&ir, "TEST").expect_err("nothing to convert");
    assert!(matches!(error, Error::NoGeometry { .. }), "got {error}");
}

// ---------------------------------------------------------------------------
// Placement.
// ---------------------------------------------------------------------------

/// A body's world placement has to be applied. Skipping it puts a
/// correctly-shaped body at the origin instead of where the assembly says it is,
/// which is a silent wrong answer rather than a visible failure.
#[test]
fn a_body_transform_moves_the_whole_body() {
    let mut builder = triangle_sheet();
    let mut transform = cadmpeg_ir::transform::Transform::identity();
    // Row-major: the translation lives in the last COLUMN, i.e. rows[i][3].
    transform.rows[0][3] = 10.0;
    transform.rows[1][3] = 20.0;
    transform.rows[2][3] = 30.0;
    builder.ir.model.bodies[0].transform = Some(transform);
    let bodies = builder.convert().expect("the placed triangle converts");
    let ImportedBody::Sheet(sheet) = &bodies[0] else {
        unreachable!()
    };
    // The triangle's first corner was the origin.
    assert!(
        sheet.vertices[0].distance(Point3::new(10.0, 20.0, 30.0)) < 1.0e-12,
        "vertex 0 landed at {:?}",
        sheet.vertices[0]
    );
    // And the geometry moved with it, not just the vertices.
    let Surface::Plane(plane) = &sheet.faces[0].surface else {
        unreachable!()
    };
    assert!((plane.origin().z - 30.0).abs() < 1.0e-12);
    let (start, _) = sheet.edges[0].curve.range_tuple();
    assert!(
        sheet.edges[0]
            .curve
            .subs(start)
            .distance(Point3::new(10.0, 20.0, 30.0))
            < 1.0e-12
    );
}

/// A row-major transform read as column-major transposes every placement, which
/// for a rotation inverts it. Pinned with an asymmetric matrix, where the two
/// readings differ.
#[test]
fn a_row_major_transform_is_not_read_transposed() {
    let mut transform = cadmpeg_ir::transform::Transform::identity();
    // A quarter turn about +z: x -> y, y -> -x. Row-major, that is
    // rows[0] = [0, -1, 0, 0] and rows[1] = [1, 0, 0, 0].
    transform.rows[0] = [0.0, -1.0, 0.0, 0.0];
    transform.rows[1] = [1.0, 0.0, 0.0, 0.0];
    let matrix = frame::matrix(&transform);
    let turned = matrix.transform_point(Point3::new(1.0, 0.0, 0.0));
    assert!(
        turned.distance(Point3::new(0.0, 1.0, 0.0)) < 1.0e-12,
        "+x turned to {turned:?}; transposed would give (0, -1, 0)"
    );
}

// ---------------------------------------------------------------------------

/// An empty document.
///
/// `CadIr` has no `Default`: `empty` takes the document's units, because a
/// document without them cannot say what its coordinates mean. Millimetres is
/// what every cadmpeg decoder normalises to.
fn empty_document() -> cadmpeg_ir::CadIr {
    cadmpeg_ir::CadIr::empty(cadmpeg_ir::units::Units::default())
}

/// A conversion context over a document, for the tests that exercise one carrier
/// rather than a whole body.
fn context_over(ir: &cadmpeg_ir::CadIr) -> Context<'_> {
    Context {
        index: ModelIndex::new(ir),
        format: "TEST",
    }
}

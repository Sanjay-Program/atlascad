use super::*;
use std::env;

#[derive(Clone)]
struct EdgeSplitInfo {
    splits: Vec<(f64, usize)>,
    indices: Vec<usize>,
}

fn debug_translated_focus_edge<C>(edge: &Edge<C>) -> bool
where C: ParametricCurve3D<Point = Point3> + BoundedCurve<Point = Point3> {
    let Ok(debug) = env::var("MT_BOOL_DEBUG_PASS_THROUGH") else {
        return false;
    };
    if debug != "translated" && debug != "origin" && debug != "all" {
        return false;
    }
    let curve = &edge.curve;
    let (t0, t1) = curve.range_tuple();
    let midpoint = curve.subs((t0 + t1) * 0.5);
    if debug == "translated" {
        midpoint.x > 0.75
            && midpoint.x < 1.01
            && midpoint.y > 0.65
            && midpoint.y < 0.86
            && midpoint.z > -0.01
            && midpoint.z < 0.30
    } else if debug == "origin" {
        midpoint.x > -0.01
            && midpoint.x < 0.35
            && midpoint.y > 0.15
            && midpoint.y < 0.35
            && midpoint.z > -0.01
            && midpoint.z < 0.05
    } else {
        midpoint.x > 0.75
            && midpoint.x < 1.01
            && midpoint.y > 0.65
            && midpoint.y < 0.86
            && midpoint.z > -0.01
            && midpoint.z < 0.30
            || midpoint.x > -0.01
                && midpoint.x < 0.35
                && midpoint.y > 0.15
                && midpoint.y < 0.35
                && midpoint.z > -0.01
                && midpoint.z < 0.05
    }
}

fn sampled_curve_aabb<C>(edge: &Edge<C>, tol: f64) -> (Point3, Point3)
where C: ParametricCurve3D<Point = Point3>
        + BoundedCurve<Point = Point3>
        + ParameterDivision1D<Point = Point3> {
    let (_, samples) = edge
        .curve
        .parameter_division(edge.curve.range_tuple(), f64::max(tol, 10.0 * TOLERANCE));
    samples.into_iter().fold(
        (
            Point3::new(f64::INFINITY, f64::INFINITY, f64::INFINITY),
            Point3::new(f64::NEG_INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY),
        ),
        |(min, max), point| {
            (
                Point3::new(min.x.min(point.x), min.y.min(point.y), min.z.min(point.z)),
                Point3::new(max.x.max(point.x), max.y.max(point.y), max.z.max(point.z)),
            )
        },
    )
}

fn split_parameters<C>(edge: &Edge<C>, vertices: &[Point3], tol: f64) -> Vec<(f64, usize)>
where C: ParametricCurve3D<Point = Point3>
        + BoundedCurve<Point = Point3>
        + ParameterDivision1D<Point = Point3>
        + SearchNearestParameter<CurveParameter, Point = Point3> {
    let debug_focus = debug_translated_focus_edge(edge);
    let tol2 = tol * tol;
    let (t0, t1) = edge.curve.range_tuple();
    let param_eps = (t1 - t0).abs() * 1.0e-8 + 1.0e-10;
    let (min, max) = sampled_curve_aabb(edge, tol);
    let candidates: Vec<_> = vertices
        .iter()
        .copied()
        .enumerate()
        .filter(|(index, _)| *index != edge.vertices.0 && *index != edge.vertices.1)
        .filter(|(_, point)| {
            point.x >= min.x - tol
                && point.x <= max.x + tol
                && point.y >= min.y - tol
                && point.y <= max.y + tol
                && point.z >= min.z - tol
                && point.z <= max.z + tol
        })
        .filter_map(|(index, point)| {
            let t = edge.curve.search_nearest_parameter(point, None, 100)?;
            let distance2 = edge.curve.subs(t).distance2(point);
            Some((index, point, t, distance2))
        })
        .collect();
    if debug_focus {
        let curve = &edge.curve;
        let midpoint = curve.subs((t0 + t1) * 0.5);
        eprintln!(
            "debug pass-through edge front={:?} back={:?} midpoint={midpoint:?} candidates={:?}",
            vertices[edge.vertices.0], vertices[edge.vertices.1], candidates,
        );
    }
    let mut splits: Vec<_> = candidates
        .into_iter()
        .filter_map(|(index, point, t, distance2)| {
            let on_curve = distance2 <= tol2;
            if debug_focus {
                eprintln!(
                    "debug pass-through candidate index={index} point={point:?} t={t} distance2={distance2} on_curve={on_curve}",
                );
            }
            (on_curve && t > t0 + param_eps && t < t1 - param_eps).then_some((t, index))
        })
        .collect();
    splits.sort_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0));
    splits.dedup_by(|lhs, rhs| {
        let same_param = (lhs.0 - rhs.0).abs() <= param_eps;
        let same_vertex = lhs.1 == rhs.1;
        same_param || same_vertex
    });
    if debug_focus {
        eprintln!("debug pass-through splits={splits:?}");
    }
    splits
}

fn split_edge<C>(
    edge_index: usize,
    edges: &mut Vec<Edge<C>>,
    vertices: &[Point3],
    splits: &[(f64, usize)],
) -> Vec<usize>
where
    C: BoundedCurve<Point = Point3> + Cut + SnapCurveEndpoints,
{
    let edge = &mut edges[edge_index];
    let back = edge.vertices.1;
    let first = splits[0];
    let mut tail = edge.curve.cut(first.0);
    edge.vertices.1 = first.1;
    edge.curve
        .snap_endpoints(vertices[edge.vertices.0], vertices[first.1]);
    let mut previous = first.1;
    let mut indices = vec![edge_index];
    splits.iter().skip(1).for_each(|(t, vertex)| {
        let next_tail = tail.cut(*t);
        tail.snap_endpoints(vertices[previous], vertices[*vertex]);
        edges.push(Edge {
            vertices: (previous, *vertex),
            curve: tail.clone(),
        });
        indices.push(edges.len() - 1);
        tail = next_tail;
        previous = *vertex;
    });
    tail.snap_endpoints(vertices[previous], vertices[back]);
    edges.push(Edge {
        vertices: (previous, back),
        curve: tail,
    });
    indices.push(edges.len() - 1);
    indices
}

fn replace_edge_uses(wire: &mut Wire, replacements: &HashMap<usize, EdgeSplitInfo>) {
    *wire = wire
        .iter()
        .copied()
        .flat_map(|edge| {
            replacements
                .get(&edge.index)
                .map(|split_info| {
                    if edge.orientation {
                        split_info
                            .indices
                            .iter()
                            .copied()
                            .map(|index| EdgeIndex {
                                index,
                                orientation: true,
                            })
                            .collect::<Vec<_>>()
                    } else {
                        split_info
                            .indices
                            .iter()
                            .rev()
                            .copied()
                            .map(|index| EdgeIndex {
                                index,
                                orientation: false,
                            })
                            .collect::<Vec<_>>()
                    }
                })
                .unwrap_or_else(|| vec![edge])
        })
        .collect();
}

fn split_trim_curve<T>(
    trim_curve: T,
    edge_range: (f64, f64),
    orientation: bool,
    splits: &[(f64, usize)],
) -> Vec<T>
where
    T: BoundedCurve + Cut + Clone,
{
    let (edge_t0, edge_t1) = edge_range;
    let edge_len = edge_t1 - edge_t0;
    let (trim_t0, trim_t1) = trim_curve.range_tuple();
    let trim_scale = trim_t1 - trim_t0;
    if trim_scale.so_small() {
        return vec![trim_curve];
    }
    let param_eps = edge_len.abs() * 1.0e-8 + 1.0e-10;
    let trim_eps = trim_scale.abs() * 1.0e-8 + 1.0e-10;
    let mut local_splits: Vec<_> = splits
        .iter()
        .map(|(t, vertex)| {
            let alpha = if edge_len.so_small() {
                0.5
            } else {
                (*t - edge_t0) / edge_len
            };
            let local_alpha = if orientation { alpha } else { 1.0 - alpha };
            (trim_t0 + trim_scale * local_alpha, *vertex)
        })
        .filter(|(t, _)| *t > trim_t0 + trim_eps && *t < trim_t1 - trim_eps)
        .collect();
    local_splits.sort_by(|lhs, rhs| lhs.0.total_cmp(&rhs.0));
    local_splits.dedup_by(|lhs, rhs| {
        let same_param = (lhs.0 - rhs.0).abs() <= param_eps;
        let same_vertex = lhs.1 == rhs.1;
        same_param || same_vertex
    });
    let mut head = trim_curve;
    let mut segments = Vec::with_capacity(local_splits.len() + 1);
    local_splits.into_iter().for_each(|(t, _)| {
        let (head_t0, head_t1) = head.range_tuple();
        if t > head_t0 + trim_eps && t < head_t1 - trim_eps {
            let tail = head.cut(t);
            segments.push(head.clone());
            head = tail;
        }
    });
    segments.push(head);
    segments
}

fn replace_trimmed_edge_uses<T, C>(
    wire: &mut Vec<CompressedEdgeUse<T>>,
    edges: &[Edge<C>],
    replacements: &HashMap<usize, EdgeSplitInfo>,
) where
    T: BoundedCurve + Cut + Clone,
    C: BoundedCurve<Point = Point3>,
{
    *wire = wire
        .iter()
        .cloned()
        .flat_map(|edge_use| {
            replacements
                .get(&edge_use.index)
                .map(|split_info| {
                    let trim_segments = edge_use.trim_curve.clone().map(|trim_curve| {
                        split_trim_curve(
                            trim_curve,
                            edges[edge_use.index].curve.range_tuple(),
                            edge_use.orientation,
                            &split_info.splits,
                        )
                    });
                    let indices: Vec<_> = if edge_use.orientation {
                        split_info.indices.clone()
                    } else {
                        split_info.indices.iter().rev().copied().collect()
                    };
                    let trim_values: Vec<_> = trim_segments
                        .map(|segments| segments.into_iter().map(Some).collect())
                        .unwrap_or_else(|| vec![None; indices.len()]);
                    indices
                        .into_iter()
                        .zip(trim_values)
                        .map(|(index, trim_curve)| CompressedEdgeUse {
                            index,
                            orientation: edge_use.orientation,
                            trim_curve,
                        })
                        .collect::<Vec<_>>()
                })
                .unwrap_or_else(|| vec![edge_use])
        })
        .collect();
}

pub(super) fn split_pass_through_edges<C, S>(shell: &mut Shell<Point3, C, S>, tol: f64)
where C: ParametricCurve3D<Point = Point3>
        + BoundedCurve<Point = Point3>
        + Cut
        + ParameterDivision1D<Point = Point3>
        + SearchNearestParameter<CurveParameter, Point = Point3>
        + SnapCurveEndpoints
        + Clone {
    let len = shell.edges.len();
    let replacements: HashMap<_, _> = (0..len)
        .filter_map(|edge_index| {
            let splits = split_parameters(&shell.edges[edge_index], &shell.vertices, tol);
            (!splits.is_empty()).then(|| {
                (
                    edge_index,
                    EdgeSplitInfo {
                        indices: split_edge(edge_index, &mut shell.edges, &shell.vertices, &splits),
                        splits,
                    },
                )
            })
        })
        .collect();
    if replacements.is_empty() {
        return;
    }
    shell.faces.iter_mut().for_each(|face| {
        face.boundaries
            .iter_mut()
            .for_each(|wire| replace_edge_uses(wire, &replacements));
    });
}

pub(super) fn split_pass_through_edges_trimmed<C, S, T>(
    shell: &mut CompressedTrimmedShell<Point3, C, S, T>,
    tol: f64,
) where
    C: ParametricCurve3D<Point = Point3>
        + BoundedCurve<Point = Point3>
        + Cut
        + ParameterDivision1D<Point = Point3>
        + SearchNearestParameter<CurveParameter, Point = Point3>
        + SnapCurveEndpoints
        + Clone,
    T: BoundedCurve + Cut + Clone,
{
    let len = shell.edges.len();
    let replacements: HashMap<_, _> = (0..len)
        .filter_map(|edge_index| {
            let splits = split_parameters(&shell.edges[edge_index], &shell.vertices, tol);
            (!splits.is_empty()).then(|| {
                (
                    edge_index,
                    EdgeSplitInfo {
                        indices: split_edge(edge_index, &mut shell.edges, &shell.vertices, &splits),
                        splits,
                    },
                )
            })
        })
        .collect();
    if replacements.is_empty() {
        return;
    }
    shell.faces.iter_mut().for_each(|face| {
        face.boundaries
            .iter_mut()
            .for_each(|wire| replace_trimmed_edge_uses(wire, &shell.edges, &replacements));
    });
}

/// Unify coincident duplicate edges after the pass-through split.
///
/// When both sides of a shared span get subdivided (each face's copy split
/// at the same passing-through vertices), the shell holds several edges with
/// the SAME vertex pair and geometry under different indices, and closedness
/// counting still sees every one as half-used (sphere-minus-cube, cell 8:
/// seven duplicate groups after the split). Only a SOUND pairing is unified:
/// each side used exactly once, with the two uses traversing the span in
/// OPPOSITE directions after vertex-order adjustment (adjacent faces across
/// the span). Same-direction coincident edges are legitimate separate
/// topology (e.g. doubled seams at tangency) and must survive -- unifying
/// them deterministically breaks the or() union of offset cubes
/// (`or_outputs_boolean_named_selections`).
pub(super) fn dedup_coincident_pass_through_edges<C>(
    edges: &[Edge<C>],
    use_orientations: &[Vec<(usize, bool)>],
    tol: f64,
) -> HashMap<usize, (usize, bool)>
where
    C: ParametricCurve3D<Point = Point3> + BoundedCurve<Point = Point3>,
{
    let tol2 = tol * tol;
    let midpoint = |edge: &Edge<C>| {
        let (t0, t1) = edge.curve.range_tuple();
        edge.curve.subs((t0 + t1) * 0.5)
    };
    let mut canonical_by_pair: HashMap<(usize, usize), Vec<usize>> = HashMap::default();
    let mut remap: HashMap<usize, (usize, bool)> = HashMap::default();
    for (index, edge) in edges.iter().enumerate() {
        let (a, b) = edge.vertices;
        if a == b {
            continue;
        }
        let key = (a.min(b), a.max(b));
        let mid = midpoint(edge);
        let bucket = canonical_by_pair.entry(key).or_default();
        let matched = bucket.iter().copied().find(|&canonical_index| {
            if midpoint(&edges[canonical_index]).distance2(mid) > tol2 {
                return false;
            }
            // Only unify a SOUND pairing: each side used exactly once, and
            // the two uses traverse the span in OPPOSITE directions after
            // vertex-order adjustment (adjacent faces across the span).
            // Same-direction coincident edges are legitimate separate
            // topology (e.g. doubled seams) and must survive.
            let canonical_uses = use_orientations
                .get(canonical_index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let duplicate_uses = use_orientations
                .get(index)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            let (
                &[(canonical_face, canonical_orientation)],
                &[(duplicate_face, duplicate_orientation)],
            ) = (canonical_uses, duplicate_uses)
            else {
                return false;
            };
            // Unifying two uses within ONE face wire makes that wire
            // non-simple (TrimmedShell rejects it); only pair across faces.
            if canonical_face == duplicate_face {
                return false;
            }
            let reversed = edges[canonical_index].vertices != edges[index].vertices;
            let duplicate_effective = duplicate_orientation ^ reversed;
            duplicate_effective != canonical_orientation
        });
        match matched {
            Some(canonical_index) => {
                let reversed = edges[canonical_index].vertices != edge.vertices;
                if std::env::var("MT_BOOL_DEBUG_PASS_THROUGH_DEDUP").is_ok() {
                    let m = midpoint(edge);
                    eprintln!(
                        "debug pass-through-dedup unify dup={index} -> canon={canonical_index} reversed={reversed} pair={key:?} mid=({:.4},{:.4},{:.4})",
                        m.x, m.y, m.z,
                    );
                }
                remap.insert(index, (canonical_index, reversed));
            }
            None => bucket.push(index),
        }
    }
    remap
}

/// Apply a [`dedup_coincident_pass_through_edges`] remap to trimmed wires.
pub(super) fn remap_trimmed_edge_uses_for_dedup<T>(
    wire: &mut [CompressedEdgeUse<T>],
    remap: &HashMap<usize, (usize, bool)>,
) {
    wire.iter_mut().for_each(|edge_use| {
        if let Some((canonical, reversed)) = remap.get(&edge_use.index) {
            edge_use.index = *canonical;
            if *reversed {
                edge_use.orientation = !edge_use.orientation;
            }
        }
    });
}

/// Split PINCHED compressed faces so the shell becomes extractable.
///
/// `Face::try_new` requires a face's boundary wires to be pairwise
/// VERTEX-DISJOINT (`Wire::disjoint_wires`), and rejects violations with the
/// same `NotSimpleWire` error an intra-wire revisit produces. The
/// pass-through split can imprint one T-junction vertex into several wires
/// of a face (a pinched face), making the whole shell inextractable. Wires
/// in a vertex-sharing component cannot coexist in any truck face, so each
/// becomes its own face on the same surface; mutually disjoint wires
/// (outer + true holes) stay together.
#[doc(hidden)]
pub fn split_pinched_compressed_faces<P, C, S: Clone>(shell: &mut Shell<P, C, S>) {
    fn component_root(components: &mut [usize], index: usize) -> usize {
        let mut root = index;
        while components[root] != root {
            root = components[root];
        }
        let mut cursor = index;
        while components[cursor] != root {
            let next = components[cursor];
            components[cursor] = root;
            cursor = next;
        }
        root
    }
    let faces = std::mem::take(&mut shell.faces);
    let mut new_faces = Vec::with_capacity(faces.len());
    for face in faces {
        if face.boundaries.len() <= 1 {
            new_faces.push(face);
            continue;
        }
        let vertex_sets: Vec<std::collections::BTreeSet<usize>> = face
            .boundaries
            .iter()
            .map(|wire| {
                wire.iter()
                    .flat_map(|edge_use| {
                        let (v0, v1) = shell.edges[edge_use.index].vertices;
                        [v0, v1]
                    })
                    .collect()
            })
            .collect();
        let wire_count = face.boundaries.len();
        let mut components: Vec<usize> = (0..wire_count).collect();
        for i in 0..wire_count {
            for j in (i + 1)..wire_count {
                if !vertex_sets[i].is_disjoint(&vertex_sets[j]) {
                    let (a, b) = (
                        component_root(&mut components, i),
                        component_root(&mut components, j),
                    );
                    if a != b {
                        components[b] = a;
                    }
                }
            }
        }
        let mut component_sizes: HashMap<usize, usize> = HashMap::default();
        for index in 0..wire_count {
            *component_sizes
                .entry(component_root(&mut components, index))
                .or_insert(0) += 1;
        }
        if component_sizes.values().all(|size| *size == 1) {
            new_faces.push(face);
            continue;
        }
        let CompressedFace {
            boundaries,
            orientation,
            surface,
        } = face;
        let mut kept_boundaries = Vec::new();
        for (index, wire) in boundaries.into_iter().enumerate() {
            if component_sizes[&component_root(&mut components, index)] == 1 {
                kept_boundaries.push(wire);
            } else {
                new_faces.push(CompressedFace {
                    boundaries: vec![wire],
                    orientation,
                    surface: surface.clone(),
                });
            }
        }
        if !kept_boundaries.is_empty() {
            new_faces.push(CompressedFace {
                boundaries: kept_boundaries,
                orientation,
                surface,
            });
        }
    }
    shell.faces = new_faces;
}

/// Split compressed face wires that REVISIT a vertex into separate closed
/// sub-loops, dropping zero-area SPIKES (sub-loops whose edge uses cancel
/// pairwise -- the pass-through imprint retracing one edge both ways).
/// `Wire::is_simple` rejects revisiting wires with `NotSimpleWire`; after
/// this pass a face may be PINCHED instead (the split loops share the
/// revisited vertex), which `split_pinched_compressed_faces` then resolves.
#[doc(hidden)]
pub fn split_non_simple_compressed_wires<P, C, S>(shell: &mut Shell<P, C, S>) {
    let edges = &shell.edges;
    let wire_start_vertices = |wire: &Wire| -> Vec<usize> {
        wire.iter()
            .map(|edge_use| {
                let (v0, v1) = edges[edge_use.index].vertices;
                if edge_use.orientation { v0 } else { v1 }
            })
            .collect()
    };
    shell.faces.iter_mut().for_each(|face| {
        let mut wire_index = 0usize;
        while wire_index < face.boundaries.len() {
            let wire = &face.boundaries[wire_index];
            let starts = wire_start_vertices(wire);
            let mut seen: HashMap<usize, usize> = HashMap::default();
            let revisit = starts
                .iter()
                .enumerate()
                .find_map(|(j, &vertex)| seen.insert(vertex, j).map(|i| (i, j)));
            if let Some((i, j)) = revisit {
                let sub_loop: Wire = wire[i..j].to_vec();
                let outer: Wire = wire[..i].iter().chain(wire[j..].iter()).copied().collect();
                if sub_loop.len() >= 2 && !outer.is_empty() {
                    // A sub-loop is a SPIKE/SLIT when its traversals cancel
                    // pairwise: signed balance per UNORDERED vertex pair,
                    // +1 for min->max traversal, -1 for max->min. This
                    // covers both the same-edge-both-ways spike and the
                    // two-distinct-coincident-edges slit (the cell-8
                    // three-cover producer: a face boundary walking up the
                    // pole meridian and straight back).
                    let mut traversal_balance: HashMap<(usize, usize), isize> = HashMap::default();
                    sub_loop.iter().for_each(|edge_use| {
                        let (v0, v1) = edges[edge_use.index].vertices;
                        let (start, end) = if edge_use.orientation {
                            (v0, v1)
                        } else {
                            (v1, v0)
                        };
                        let key = (start.min(end), start.max(end));
                        *traversal_balance.entry(key).or_insert(0) +=
                            if start <= end { 1 } else { -1 };
                    });
                    let is_spike = traversal_balance.values().all(|balance| *balance == 0);
                    face.boundaries[wire_index] = outer;
                    if !is_spike {
                        face.boundaries.push(sub_loop);
                    }
                    continue;
                }
            }
            wire_index += 1;
        }
    });
}

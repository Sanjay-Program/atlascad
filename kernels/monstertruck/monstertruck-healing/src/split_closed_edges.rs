use super::*;

/// Cuts the edge at the middle point of the bounded curve `curve`.
fn cut_edge<P, C>(
    vertices: &mut Vec<P>,
    Edge {
        vertices: (_, v1),
        curve,
    }: &mut Edge<C>,
) -> Edge<C>
where
    C: BoundedCurve<Point = P> + Cut,
{
    let (t0, t1) = curve.range_tuple();
    let t = (t0 + t1) / 2.0;
    vertices.push(curve.subs(t));

    let v2 = *v1;
    *v1 = vertices.len() - 1;
    let v1 = *v1;
    let curve0 = curve.cut(t);
    Edge {
        vertices: (v1, v2),
        curve: curve0,
    }
}

/// Adds cut edges to the vector `edges`.
fn add_edges<P, C>(vertices: &mut Vec<P>, edges: &mut Vec<Edge<C>>) -> HashMap<usize, usize>
where C: BoundedCurve<Point = P> + Cut {
    let len = edges.len();
    let sub_add_edges = move |i: usize| {
        let (v0, v1) = edges[i].vertices;
        if v0 == v1 {
            let new_edge = cut_edge(vertices, &mut edges[i]);
            edges.push(new_edge);
            Some((i, edges.len() - 1))
        } else {
            None
        }
    };
    (0..len).filter_map(sub_add_edges).collect()
}

fn replace_edges(wire: &mut Wire, added: &HashMap<usize, usize>) {
    let insert_one_edge = |edge: EdgeIndex| {
        let Some(new_edge_index) = added.get(&edge.index) else {
            return vec![edge];
        };
        let new_edge = EdgeIndex {
            index: *new_edge_index,
            orientation: edge.orientation,
        };
        match edge.orientation {
            true => vec![edge, new_edge],
            false => vec![new_edge, edge],
        }
    };
    let new_wire = wire.iter().copied().flat_map(insert_one_edge).collect();
    *wire = new_wire;
}

pub(super) fn split_closed_edges<P, C, S>(shell: &mut Shell<P, C, S>)
where C: BoundedCurve<Point = P> + Cut {
    let added = add_edges(&mut shell.vertices, &mut shell.edges);
    let wire_iter = shell.faces.iter_mut().flat_map(|face| &mut face.boundaries);
    wire_iter.for_each(|wire| replace_edges(wire, &added));
}

fn split_trim_curve_midpoint<T>(trim_curve: T) -> (T, T)
where T: BoundedCurve + Cut + Clone {
    let (t0, t1) = trim_curve.range_tuple();
    let tm = (t0 + t1) * 0.5;
    let mut head = trim_curve;
    let tail = head.cut(tm);
    (head, tail)
}

fn replace_trimmed_edges<T>(wire: &mut Vec<CompressedEdgeUse<T>>, added: &HashMap<usize, usize>)
where T: BoundedCurve + Cut + Clone {
    *wire = wire
        .iter()
        .cloned()
        .flat_map(|edge_use| {
            let Some(new_edge_index) = added.get(&edge_use.index).copied() else {
                return vec![edge_use];
            };
            let trim_segments = edge_use
                .trim_curve
                .map(split_trim_curve_midpoint)
                .map(|(front, back)| vec![Some(front), Some(back)])
                .unwrap_or_else(|| vec![None, None]);
            let indices = if edge_use.orientation {
                [edge_use.index, new_edge_index]
            } else {
                [new_edge_index, edge_use.index]
            };
            indices
                .into_iter()
                .zip(trim_segments)
                .map(|(index, trim_curve)| CompressedEdgeUse {
                    index,
                    orientation: edge_use.orientation,
                    trim_curve,
                })
                .collect::<Vec<_>>()
        })
        .collect();
}

pub(super) fn split_closed_edges_trimmed<C, S, T>(
    shell: &mut CompressedTrimmedShell<Point3, C, S, T>,
) where
    C: BoundedCurve<Point = Point3> + Cut,
    T: BoundedCurve + Cut + Clone, {
    let added = add_edges(&mut shell.vertices, &mut shell.edges);
    shell.faces.iter_mut().for_each(|face| {
        face.boundaries
            .iter_mut()
            .for_each(|wire| replace_trimmed_edges(wire, &added));
    });
}

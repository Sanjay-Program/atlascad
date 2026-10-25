use super::*;
use std::collections::BTreeSet;

/// The two vertex-disjoint cap loops a seam split yields from one wire.
type CapLoops<T> = (Vec<CompressedEdgeUse<T>>, Vec<CompressedEdgeUse<T>>);

/// The oriented (front, back) endpoint vertices of a compressed edge-use.
fn use_endpoints<C, T>(
    edge_use: &CompressedEdgeUse<T>,
    edges: &[CompressedEdge<C>],
) -> (usize, usize) {
    let (v0, v1) = edges[edge_use.index].vertices;
    match edge_use.orientation {
        true => (v0, v1),
        false => (v1, v0),
    }
}

/// A run of edge-uses forms a non-empty closed loop (each back meets the next
/// front, and the last back meets the first front).
fn is_closed_loop<C, T>(arc: &[CompressedEdgeUse<T>], edges: &[CompressedEdge<C>]) -> bool {
    !arc.is_empty()
        && arc
            .windows(2)
            .all(|pair| use_endpoints(&pair[0], edges).1 == use_endpoints(&pair[1], edges).0)
        && use_endpoints(arc.last().unwrap(), edges).1 == use_endpoints(&arc[0], edges).0
}

/// All endpoint vertices touched by a run of edge-uses.
fn arc_vertices<C, T>(
    arc: &[CompressedEdgeUse<T>],
    edges: &[CompressedEdge<C>],
) -> BTreeSet<usize> {
    arc.iter()
        .flat_map(|edge_use| {
            let (v0, v1) = edges[edge_use.index].vertices;
            [v0, v1]
        })
        .collect()
}

/// Resolve a residual periodic-surface SEAM face into its disjoint cap loops.
///
/// A cylinder/cone imported from another CAD system can present a single
/// boundary wire that walks its two cap circles joined by a doubled SEAM edge:
/// one compressed edge index used twice in the wire, once with each orientation
/// (the periodic surface's artificial seam, traversed up and back down). After
/// [`split_closed_edges`](super::split_closed_edges) turns each closed cap edge
/// into a pair of half-arcs, the wire reads
/// `[cap_a_arc, cap_a_arc, SEAM, cap_b_arc, cap_b_arc, SEAM^-1]` and REVISITS the
/// two seam-endpoint vertices.
///
/// [`split_closed_faces`](super::split_closed_faces) divides most such faces
/// along an antipodal param-space divisor into two simple sub-faces, but its
/// final `assort_boundary`/`divide_face` step rejects faces whose param-space
/// loops are all clockwise (inward-normal / hole cylinders, whose divided loops
/// have no positive outer loop) or whose seam-crossing half wraps the u-period,
/// leaving the doubled-seam wire intact. `Face::try_new` then refuses it as
/// `NotSimpleWire` (the seam endpoints are revisited), so the whole STEP solid
/// fails to extract at solidify -- BEFORE any boolean.
///
/// Run last in the trimmed heal, this pass drops the redundant seam edge uses
/// and re-forms the wire as the two vertex-disjoint cap loops the seam separated
/// -- the annular (tube) representation of the periodic face -- preserving every
/// surviving edge-use's face-local trim (each cap arc keeps its own trim curve;
/// only the seam uses, which carry no boundary, are dropped). It rewrites a wire
/// only when the split yields two non-empty, closed, VERTEX-DISJOINT loops, so a
/// face it cannot fully resolve (e.g. a cone whose caps meet at the apex, or a
/// genuine self-intersection) is left untouched and still refuses honestly with
/// a typed `NotSimpleWire` rather than being silently corrupted.
///
/// Returns the number of wires rewritten.
pub(super) fn split_seam_faces_trimmed<P, C, S, T: Clone>(
    shell: &mut CompressedTrimmedShell<P, C, S, T>,
) -> usize {
    let CompressedTrimmedShell { edges, faces, .. } = shell;
    let edges = &*edges;
    let mut splits = 0usize;
    for face in faces.iter_mut() {
        let mut rewritten: Vec<Vec<CompressedEdgeUse<T>>> =
            Vec::with_capacity(face.boundaries.len());
        for wire in std::mem::take(&mut face.boundaries) {
            if let Some((arc0, arc1)) = split_seam_wire(&wire, edges) {
                rewritten.push(arc0);
                rewritten.push(arc1);
                splits += 1;
            } else {
                rewritten.push(wire);
            }
        }
        face.boundaries = rewritten;
    }
    splits
}

/// If `wire` contains a seam edge (one edge index used exactly twice with
/// opposite orientation) whose removal leaves two non-empty, closed,
/// vertex-disjoint loops, return those two loops. Otherwise `None`.
fn split_seam_wire<C, T: Clone>(
    wire: &[CompressedEdgeUse<T>],
    edges: &[CompressedEdge<C>],
) -> Option<CapLoops<T>> {
    let mut positions: HashMap<usize, Vec<usize>> = HashMap::default();
    for (index, edge_use) in wire.iter().enumerate() {
        positions.entry(edge_use.index).or_default().push(index);
    }
    let (p0, p1) = positions.values().find_map(|pos| {
        (pos.len() == 2 && wire[pos[0]].orientation != wire[pos[1]].orientation)
            .then(|| (pos[0].min(pos[1]), pos[0].max(pos[1])))
    })?;
    let arc0: Vec<CompressedEdgeUse<T>> = wire[p0 + 1..p1].to_vec();
    let arc1: Vec<CompressedEdgeUse<T>> = wire[p1 + 1..]
        .iter()
        .chain(wire[..p0].iter())
        .cloned()
        .collect();
    if is_closed_loop(&arc0, edges)
        && is_closed_loop(&arc1, edges)
        && arc_vertices(&arc0, edges).is_disjoint(&arc_vertices(&arc1, edges))
    {
        Some((arc0, arc1))
    } else {
        None
    }
}

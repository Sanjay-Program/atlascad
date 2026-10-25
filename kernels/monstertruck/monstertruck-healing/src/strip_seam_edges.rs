//! Split wires that revisit the same edge with opposite orientations.
//!
//! STEP exports of cylindrical and conical faces routinely encode a face's
//! boundary as a single wire that walks up one side of the seam, around the
//! top rim, back down the other side of the seam, and around the bottom rim.
//! The seam edge appears twice in the wire -- once in each orientation --
//! and the wire visits the seam's endpoint vertices twice, so [`Shell::extract`]
//! rejects the wire as not simple.
//!
//! Cutting along the seam edge splits the single wire into two simple wires
//! (the top rim and the bottom rim) that together still form a valid set of
//! boundaries for the same face. This pass walks every wire on every face and
//! performs that split when the seam-edge pattern is present.
//!
//! [`Shell::extract`]: monstertruck_topology::Shell::extract

use super::*;

/// Find a seam edge in `wire`: a pair of indices `(i, j)` with the same
/// underlying edge but opposite orientations. Returns `None` if no such pair
/// exists.
fn find_seam_pair(wire: &[EdgeIndex]) -> Option<(usize, usize)> {
    let mut first: HashMap<usize, (usize, bool)> = HashMap::default();
    for (idx, e) in wire.iter().enumerate() {
        match first.get(&e.index) {
            Some(&(prev_idx, prev_orient)) if prev_orient != e.orientation => {
                return Some((prev_idx, idx));
            }
            _ => {
                first.insert(e.index, (idx, e.orientation));
            }
        }
    }
    None
}

/// If `wire` contains a seam-edge pair, drop both occurrences and split the
/// remaining edges into two new wires. Returns the new boundary list (one or
/// more wires) when a split is performed, or `None` when the wire is already
/// seam-free.
///
/// `wire` is treated as a closed cycle, so the "outer" piece may straddle the
/// wraparound between the two seam-edge positions.
fn split_one_wire(wire: &[EdgeIndex]) -> Option<Vec<Wire>> {
    let (i, j) = find_seam_pair(wire)?;
    debug_assert!(i < j);
    let inner: Wire = wire[i + 1..j].to_vec();
    let mut outer: Wire = Vec::with_capacity(wire.len() - (j - i + 1));
    outer.extend_from_slice(&wire[j + 1..]);
    outer.extend_from_slice(&wire[..i]);
    if inner.is_empty() && outer.is_empty() {
        return None;
    }
    let mut result = Vec::new();
    if !inner.is_empty() {
        result.push(inner);
    }
    if !outer.is_empty() {
        result.push(outer);
    }
    Some(result)
}

/// Repeatedly strip seams from every wire on every face until no further
/// splits are possible. Each pass can introduce new wires that themselves
/// contain seams (rare, but possible on multi-seamed faces such as toroidal
/// patches), so the outer loop iterates until a fixed point.
pub(super) fn strip_seam_edges<P, C, S>(shell: &mut Shell<P, C, S>) {
    // Multi-seamed faces (e.g. toroidal patches) may surface a new seam
    // pair only after an earlier pass strips its outer seam, so iterate
    // until a fixed point. Cap the iteration depth so a buggy split
    // can never spin forever.
    const MAX_ITERS: usize = 32;
    let mut changed = true;
    let mut iters = 0;
    while changed && iters < MAX_ITERS {
        changed = false;
        for face in shell.faces.iter_mut() {
            let mut new_boundaries: Vec<Wire> = Vec::with_capacity(face.boundaries.len());
            for wire in face.boundaries.drain(..) {
                if let Some(split) = split_one_wire(&wire) {
                    new_boundaries.extend(split);
                    changed = true;
                } else {
                    new_boundaries.push(wire);
                }
            }
            face.boundaries = new_boundaries;
        }
        iters += 1;
    }
}

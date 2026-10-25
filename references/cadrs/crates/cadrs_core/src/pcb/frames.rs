//! The frames a derived board lives in (`docs/PCB_IN_PART_STUDIO.md` §11), and the one place
//! that converts between them:
//!
//! - the **layout** (cadrs_eda): nm, y up, the board's origin;
//! - the **board frame**: mm, the layout's x and y, z up through the board with its bottom face
//!   at z = 0 and its top face at z = thickness;
//! - the **Part Studio** (world): the board frame placed by a [`PlaneFrame`] (its origin is the
//!   board frame's origin, `u`/`v` its x and y, the normal its z).
//!
//! A footprint's frame is the board frame moved by [`footprint_motion`]: on the top face turned
//! by its angle; on the bottom face turned and flipped over x, so its model hangs down.

use cadrs_eda::footprint::FootprintPlacement;
use cadrs_eda::layer::Side;
use cadrs_eda::units::{Pt, to_mm};
use cadrs_kernel::Motion;
use cadrs_sketch::PlaneFrame;
use nalgebra::{Matrix3, Rotation3, Vector3};

pub type V3 = [f64; 3];

fn dot(a: V3, b: V3) -> f64 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

/// A layout point in the board frame's plane (mm).
pub fn layout_to_board(p: Pt) -> [f64; 2] {
    [to_mm(p.x), to_mm(p.y)]
}

/// A board-frame point (mm) on the layout (nm).
pub fn board_to_layout(p: [f64; 2]) -> Pt {
    Pt::mm(p[0], p[1])
}

/// A board-frame point in the Part Studio.
pub fn to_world(frame: &PlaneFrame, p: V3) -> V3 {
    let n = frame.normal();
    std::array::from_fn(|i| frame.origin[i] + frame.u[i] * p[0] + frame.v[i] * p[1] + n[i] * p[2])
}

/// A Part Studio point in the board frame.
pub fn to_board(frame: &PlaneFrame, p: V3) -> V3 {
    let d = [p[0] - frame.origin[0], p[1] - frame.origin[1], p[2] - frame.origin[2]];
    dir_to_board(frame, d)
}

/// A Part Studio direction in the board frame.
pub fn dir_to_board(frame: &PlaneFrame, d: V3) -> V3 {
    [dot(d, frame.u), dot(d, frame.v), dot(d, frame.normal())]
}

/// A board-frame direction in the Part Studio.
pub fn dir_to_world(frame: &PlaneFrame, d: V3) -> V3 {
    let n = frame.normal();
    std::array::from_fn(|i| frame.u[i] * d[0] + frame.v[i] * d[1] + n[i] * d[2])
}

/// The board frame's place in the Part Studio as a motion.
pub fn board_motion(frame: &PlaneFrame) -> Motion {
    let col = |a: V3| Vector3::new(a[0], a[1], a[2]);
    Motion { linear: Matrix3::from_columns(&[col(frame.u), col(frame.v), col(frame.normal())]), translation: col(frame.origin) }
}

/// A footprint's frame in the board frame (see the module docs); `thickness` in mm.
pub fn footprint_motion(p: &FootprintPlacement, thickness: f64) -> Motion {
    let rz = Rotation3::from_axis_angle(&Vector3::z_axis(), p.angle.to_radians());
    let [x, y] = layout_to_board(p.at);
    match p.side {
        Side::Top => Motion { linear: *rz.matrix(), translation: Vector3::new(x, y, thickness) },
        Side::Bottom => {
            let over = Rotation3::from_axis_angle(&Vector3::x_axis(), std::f64::consts::PI);
            Motion { linear: (rz * over).matrix().clone_owned(), translation: Vector3::new(x, y, 0.0) }
        }
    }
}

/// A footprint's frame in the Part Studio.
pub fn footprint_world(frame: &PlaneFrame, thickness: f64, p: &FootprintPlacement) -> Motion {
    footprint_motion(p, thickness).then(&board_motion(frame))
}

/// Where a view ray (Part Studio) meets the board face it sees, in the board frame's plane
/// (mm): the top face seen from above, the bottom face from below. `None` along the board.
pub fn ray_to_board(frame: &PlaneFrame, thickness: f64, origin: V3, dir: V3) -> Option<[f64; 2]> {
    let o = to_board(frame, origin);
    let d = dir_to_board(frame, dir);
    if d[2].abs() < 1e-9 {
        return None;
    }
    let z = if d[2] < 0.0 { thickness } else { 0.0 };
    let t = (z - o[2]) / d[2];
    Some([o[0] + d[0] * t, o[1] + d[1] * t])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: V3, b: V3) -> bool {
        (0..3).all(|i| (a[i] - b[i]).abs() < 1e-9)
    }

    fn apply(m: &Motion, p: V3) -> V3 {
        let q = m.linear * Vector3::new(p[0], p[1], p[2]) + m.translation;
        [q.x, q.y, q.z]
    }

    /// A board standing up: x along world y, y along world z, the normal along +x.
    fn standing() -> PlaneFrame {
        PlaneFrame { origin: [10.0, -4.0, 2.0], u: [0.0, 1.0, 0.0], v: [0.0, 0.0, 1.0] }
    }

    #[test]
    fn points_go_round_the_frames_and_back() {
        let f = standing();
        for p in [[0.0, 0.0, 0.0], [3.5, -2.25, 1.6], [-40.0, 12.0, -7.0]] {
            let w = to_world(&f, p);
            assert!(close(to_board(&f, w), p));
            assert!(close(apply(&board_motion(&f), p), w));
        }
        assert!(close(to_world(&f, [1.0, 2.0, 3.0]), [13.0, -3.0, 4.0]));
        assert!(close(dir_to_board(&f, dir_to_world(&f, [0.3, -0.4, 0.5])), [0.3, -0.4, 0.5]));
        let pt = Pt::mm(12.5, -3.25);
        assert_eq!(board_to_layout(layout_to_board(pt)), pt);
    }

    #[test]
    fn footprints_sit_on_their_face_top_and_bottom() {
        let f = standing();
        for (side, z) in [(Side::Top, 1.6), (Side::Bottom, 0.0)] {
            let pl = FootprintPlacement { at: Pt::mm(10.0, 20.0), angle: 90.0, side };
            // A pad at (1, 2) of the footprint: where the layout has it, on its face.
            let pad = pl.apply(Pt::mm(1.0, 2.0));
            let [x, y] = layout_to_board(pad);
            assert!(close(apply(&footprint_motion(&pl, 1.6), [1.0, 2.0, 0.0]), [x, y, z]), "{side:?}");
            assert!(close(apply(&footprint_world(&f, 1.6, &pl), [1.0, 2.0, 0.0]), to_world(&f, [x, y, z])));
            // Up from the footprint is out of its face.
            let up = footprint_motion(&pl, 1.6).linear * Vector3::z();
            assert!((up.z - if side == Side::Top { 1.0 } else { -1.0 }).abs() < 1e-9);
        }
    }

    #[test]
    fn a_ray_meets_the_face_it_sees() {
        let f = standing();
        // From +x (above the top face) straight down the normal.
        let from_above = ray_to_board(&f, 1.6, to_world(&f, [5.0, 6.0, 50.0]), dir_to_world(&f, [0.0, 0.0, -1.0])).unwrap();
        assert!((from_above[0] - 5.0).abs() < 1e-9 && (from_above[1] - 6.0).abs() < 1e-9);
        // Slanted from below: it meets z = 0, not the top.
        let o = to_world(&f, [0.0, 0.0, -10.0]);
        let p = ray_to_board(&f, 1.6, o, dir_to_world(&f, [1.0, 0.0, 1.0])).unwrap();
        assert!((p[0] - 10.0).abs() < 1e-9);
        assert!(ray_to_board(&f, 1.6, o, dir_to_world(&f, [1.0, 0.0, 0.0])).is_none());
    }
}

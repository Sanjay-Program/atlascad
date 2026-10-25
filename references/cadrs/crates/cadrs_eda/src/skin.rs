//! A board face as made, as a picture: the solder mask, copper, pads, holes and silkscreen of
//! [`crate::render::board_surface`] rasterised over the board's box, transparent outside the
//! board (beyond its outline and in its cut-outs), so it can be laid on any face shape as one
//! rectangle. The PCB Studio's 3D view and a board derived into a Part Studio both show it.

use crate::board::Board;
use crate::layer::Side;
use crate::units::{Bounds, Nm};
use resvg::{tiny_skia, usvg};

/// A rasterised board face: RGBA pixels (row 0 at the top, the board's highest y), its size,
/// and the board box it covers.
#[derive(Clone, Debug, PartialEq)]
pub struct Skin {
    pub rgba: Vec<u8>,
    pub width: u32,
    pub height: u32,
    pub bounds: Bounds,
}

impl Skin {
    /// The picture as PNG bytes.
    pub fn png(&self) -> Option<Vec<u8>> {
        let size = tiny_skia::IntSize::from_wh(self.width, self.height)?;
        tiny_skia::Pixmap::from_vec(self.rgba.clone(), size)?.encode_png().ok()
    }
}

/// The face `side` of `board` at `px_per_mm`, at most `max_px` on its longer side. `None`
/// without a closed outline.
pub fn board_skin(board: &Board, side: Side, px_per_mm: f32, max_px: u32) -> Option<Skin> {
    let region = crate::outline::board_region(board);
    let bounds = region.iter().flatten().fold(None, |acc, p| Some(Bounds::union(acc, Bounds::of(*p))))?;
    let (w, h) = (bounds.size().w as f64 / 1e6, bounds.size().h as f64 / 1e6);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = px_per_mm.min(max_px as f32 / w.max(h) as f32);
    let (pw, ph) = (((w as f32 * scale).ceil() as u32).max(1), ((h as f32 * scale).ceil() as u32).max(1));
    let mut d = crate::render::board_surface(board, side);
    d.bounds = Some(bounds);
    let tree = usvg::Tree::from_str(&crate::render::to_svg(&d), &usvg::Options::default()).ok()?;
    let mut pix = tiny_skia::Pixmap::new(pw, ph)?;
    let k = pw as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    // Only the board: its area (outline less cut-outs) as a mask.
    let mm = |v: Nm| v as f64 / 1e6;
    let (x0, y1) = (mm(bounds.min.x), mm(bounds.max.y));
    let to_px = |p: crate::units::Pt| (((mm(p.x) - x0) * pw as f64 / w) as f32, ((y1 - mm(p.y)) * ph as f64 / h) as f32);
    let mut path = tiny_skia::PathBuilder::new();
    for ring in &region {
        for (i, p) in ring.iter().enumerate() {
            let (x, y) = to_px(*p);
            if i == 0 { path.move_to(x, y) } else { path.line_to(x, y) }
        }
        path.close();
    }
    if let Some(path) = path.finish() {
        let mut mask = tiny_skia::Mask::new(pw, ph)?;
        mask.fill_path(&path, tiny_skia::FillRule::EvenOdd, true, tiny_skia::Transform::identity());
        pix.apply_mask(&mask);
    }
    // tiny-skia keeps premultiplied alpha; pictures are straight RGBA.
    let rgba: Vec<u8> = pix
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(Skin { rgba, width: pw, height: ph, bounds })
}

/// A draw list rasterised over `bounds` at `px_per_mm` (at most `max_px` on the longer side),
/// transparent where it draws nothing (its background is left out). With `clip`, only inside
/// that region (the board's area) is kept.
pub fn raster(list: &crate::render::DrawList, bounds: Bounds, px_per_mm: f32, max_px: u32, clip: Option<&crate::poly::Region>) -> Option<Skin> {
    let (w, h) = (bounds.size().w as f64 / 1e6, bounds.size().h as f64 / 1e6);
    if w <= 0.0 || h <= 0.0 {
        return None;
    }
    let scale = px_per_mm.min(max_px as f32 / w.max(h) as f32);
    let (pw, ph) = (((w as f32 * scale).ceil() as u32).max(1), ((h as f32 * scale).ceil() as u32).max(1));
    let mut d = list.clone();
    d.background = [0, 0, 0, 0];
    d.bounds = Some(bounds);
    let tree = usvg::Tree::from_str(&crate::render::to_svg(&d), &usvg::Options::default()).ok()?;
    let mut pix = tiny_skia::Pixmap::new(pw, ph)?;
    let k = pw as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(k, k), &mut pix.as_mut());
    if let Some(region) = clip {
        let mm = |v: Nm| v as f64 / 1e6;
        let (x0, y1) = (mm(bounds.min.x), mm(bounds.max.y));
        let to_px = |p: crate::units::Pt| (((mm(p.x) - x0) * pw as f64 / w) as f32, ((y1 - mm(p.y)) * ph as f64 / h) as f32);
        let mut path = tiny_skia::PathBuilder::new();
        for ring in region {
            for (i, p) in ring.iter().enumerate() {
                let (x, y) = to_px(*p);
                if i == 0 { path.move_to(x, y) } else { path.line_to(x, y) }
            }
            path.close();
        }
        if let Some(path) = path.finish() {
            let mut mask = tiny_skia::Mask::new(pw, ph)?;
            mask.fill_path(&path, tiny_skia::FillRule::EvenOdd, true, tiny_skia::Transform::identity());
            pix.apply_mask(&mask);
        }
    }
    let rgba: Vec<u8> = pix
        .pixels()
        .iter()
        .flat_map(|p| {
            let c = p.demultiply();
            [c.red(), c.green(), c.blue(), c.alpha()]
        })
        .collect();
    Some(Skin { rgba, width: pw, height: ph, bounds })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::units::{Pt, mm};

    #[test]
    fn a_skin_is_the_board_and_clear_around_it() {
        // An L-shaped board: a 20 × 20 square less its top-right 10 × 10 quarter.
        let mut b = Board::default();
        let pts = [(0.0, 0.0), (20.0, 0.0), (20.0, 10.0), (10.0, 10.0), (10.0, 20.0), (0.0, 20.0)];
        for i in 0..pts.len() {
            let (a, c) = (pts[i], pts[(i + 1) % pts.len()]);
            crate::board_edit::add_shape(&mut b, crate::graphics::Geom::Line { a: Pt::mm(a.0, a.1), b: Pt::mm(c.0, c.1) }, crate::layer::Layer::Outline);
        }
        let s = board_skin(&b, Side::Top, 10.0, 4096).unwrap();
        assert_eq!((s.width, s.height), (200, 200));
        assert_eq!(s.bounds.size().w, mm(20.0));
        let alpha = |x: u32, y: u32| s.rgba[((y * s.width + x) * 4 + 3) as usize];
        // Bottom-left quarter (image bottom-left) is board; the top-right quarter (image top
        // right) is clear.
        assert_eq!(alpha(50, 150), 255);
        assert_eq!(alpha(150, 50), 0);
        assert!(s.png().is_some_and(|p| p.starts_with(&[0x89, b'P', b'N', b'G'])));
        // A draw list: clear where it draws nothing.
        let list = crate::render::DrawList { background: [9, 9, 9, 255], areas: vec![crate::render::Area { color: [200, 0, 0, 255], z: 0, tris: vec![[0.0, 0.0], [10e6, 0.0], [0.0, 10e6]] }], ..Default::default() };
        let r = raster(&list, s.bounds, 10.0, 4096, None).unwrap();
        let a = |x: u32, y: u32| r.rgba[((y * r.width + x) * 4 + 3) as usize];
        assert_eq!(a(20, 180), 255);
        assert_eq!(a(180, 20), 0);
        // Capped size.
        let small = board_skin(&b, Side::Bottom, 10.0, 64).unwrap();
        assert_eq!(small.width, 64);
    }
}

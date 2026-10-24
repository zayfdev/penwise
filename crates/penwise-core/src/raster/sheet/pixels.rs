//! Exact pixmap byte operations for sheet layout: quarter-turn rotation, content bbox, crop, blit, fill.
//! Plain byte copies (no tiny-skia pipelines), so layout never perturbs rendered pixels.
//!
//! Owner: slice e.

use super::Rotation;
use crate::raster::RasterError;
use tiny_skia::Pixmap;

/// Opaque white, premultiplied RGBA8 (the sheet background).
pub(super) const WHITE: [u8; 4] = [255, 255, 255, 255];

/// Integer pixel rect.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct PixelRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

/// New pixmap filled with `rgba` (premultiplied).
///
/// # Errors
/// [`RasterError::InvalidSize`] for zero or too-large dimensions.
pub(super) fn filled(width: u32, height: u32, rgba: [u8; 4]) -> Result<Pixmap, RasterError> {
    let mut pm = Pixmap::new(width, height).ok_or(RasterError::InvalidSize { width, height })?;
    for px in pm.data_mut().chunks_exact_mut(4) {
        px.copy_from_slice(&rgba);
    }
    Ok(pm)
}

/// `src` turned by a quarter turn (`Cw`: top row becomes the right column) or copied (`None`).
///
/// # Errors
/// [`RasterError::InvalidSize`] when the turned pixmap cannot be allocated.
pub(super) fn rotate(src: &Pixmap, rotation: Rotation) -> Result<Pixmap, RasterError> {
    let (w, h) = (src.width(), src.height());
    if rotation == Rotation::None {
        return Ok(src.clone());
    }
    let mut dst = filled(h, w, WHITE)?;
    let (sw, dw) = (w as usize, h as usize);
    let (sd, dd) = (src.data(), dst.data_mut());
    for y in 0..h as usize {
        for x in 0..w as usize {
            let (dx, dy) = match rotation {
                Rotation::Cw => (dw - 1 - y, x),
                Rotation::Ccw | Rotation::None => (y, sw - 1 - x),
            };
            let (s, d) = ((y * sw + x) * 4, (dy * dw + dx) * 4);
            if let (Some(from), Some(to)) = (sd.get(s..s + 4), dd.get_mut(d..d + 4)) {
                to.copy_from_slice(from);
            }
        }
    }
    Ok(dst)
}

/// Bounding rect of every pixel that differs from `background`, grown by `pad` and clipped to the pixmap;
/// `None` when every pixel equals `background`.
pub(super) fn content_rect(pm: &Pixmap, background: [u8; 4], pad: u32) -> Option<PixelRect> {
    let (w, h) = (pm.width(), pm.height());
    let (mut x0, mut y0, mut x1, mut y1) = (w, h, 0u32, 0u32);
    for (y, row) in (0u32..).zip(pm.data().chunks_exact(w as usize * 4)) {
        for (x, px) in (0u32..).zip(row.chunks_exact(4)) {
            if px != background {
                (x0, y0) = (x0.min(x), y0.min(y));
                (x1, y1) = (x1.max(x + 1), y1.max(y + 1));
            }
        }
    }
    if x0 >= x1 || y0 >= y1 {
        return None;
    }
    let (x0, y0) = (x0.saturating_sub(pad), y0.saturating_sub(pad));
    let (x1, y1) = (x1.saturating_add(pad).min(w), y1.saturating_add(pad).min(h));
    Some(PixelRect {
        x: x0,
        y: y0,
        width: x1 - x0,
        height: y1 - y0,
    })
}

/// Copy of `rect` (must lie inside `src`).
///
/// # Errors
/// [`RasterError::InvalidSize`] for an empty rect.
pub(super) fn crop(src: &Pixmap, rect: PixelRect) -> Result<Pixmap, RasterError> {
    let mut dst = filled(rect.width, rect.height, WHITE)?;
    blit_from(&mut dst, src, rect);
    Ok(dst)
}

/// Copies all of `src` into `dst` with its top-left corner at `(x, y)`; pixels falling outside `dst` are dropped.
pub(super) fn blit(dst: &mut Pixmap, src: &Pixmap, x: u32, y: u32) {
    let (dw, dh) = (dst.width(), dst.height());
    let width = src.width().min(dw.saturating_sub(x));
    let height = src.height().min(dh.saturating_sub(y));
    let (sw, dws) = (src.width() as usize * 4, dw as usize * 4);
    let n = width as usize * 4;
    let dd = dst.data_mut();
    for row in 0..height as usize {
        let s = row * sw;
        let d = (y as usize + row) * dws + x as usize * 4;
        if let (Some(from), Some(to)) = (src.data().get(s..s + n), dd.get_mut(d..d + n)) {
            to.copy_from_slice(from);
        }
    }
}

/// Copies `rect` of `src` into `dst` at the origin.
fn blit_from(dst: &mut Pixmap, src: &Pixmap, rect: PixelRect) {
    let (sw, dw) = (src.width() as usize * 4, dst.width() as usize * 4);
    let n = (rect.width.min(dst.width())) as usize * 4;
    let rows = rect.height.min(dst.height()) as usize;
    let dd = dst.data_mut();
    for row in 0..rows {
        let s = (rect.y as usize + row) * sw + rect.x as usize * 4;
        let d = row * dw;
        if let (Some(from), Some(to)) = (src.data().get(s..s + n), dd.get_mut(d..d + n)) {
            to.copy_from_slice(from);
        }
    }
}

/// Fills `rect` of `dst` (clipped) with `rgba`.
pub(super) fn fill(dst: &mut Pixmap, rect: PixelRect, rgba: [u8; 4]) {
    let (dw, dh) = (dst.width(), dst.height());
    let x1 = rect.x.saturating_add(rect.width).min(dw) as usize;
    let y1 = rect.y.saturating_add(rect.height).min(dh) as usize;
    let stride = dw as usize * 4;
    let dd = dst.data_mut();
    for row in rect.y as usize..y1 {
        let line = row * stride;
        let span = dd.get_mut(line + rect.x as usize * 4..line + x1 * 4);
        for px in span.into_iter().flat_map(|s| s.chunks_exact_mut(4)) {
            px.copy_from_slice(&rgba);
        }
    }
}

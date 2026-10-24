//! Coverage buffer → pixmap compositing (ink-rendering.md §3.5): plain `f32` source-over / separable multiply on
//! the premultiplied RGBA8 pixmap bytes, independent of tiny-skia's SIMD pipelines.
//!
//! Owner: slice e.

use super::CoverageBuffer;
use crate::brush::BlendMode;
use crate::float::clamp01;
use crate::geometry::StrokeStyle;
use tiny_skia::Pixmap;

/// Composites `color` at `alpha · S` over the buffer's rect of `pixmap` (premultiplied RGBA8 bytes).
pub(super) fn composite(
    pixmap: &mut Pixmap,
    buf: &CoverageBuffer,
    style: &StrokeStyle,
    alpha: f32,
) {
    let stride = pixmap.width() as usize;
    let (ox, oy) = (
        buf.origin_x.unsigned_abs() as usize,
        buf.origin_y.unsigned_abs() as usize,
    );
    let width = buf.width as usize;
    let c = style.color;
    let straight = [c.r, c.g, c.b].map(|v| f32::from(v) / 255.0);
    let multiply = style.blend == BlendMode::Multiply;
    let data = pixmap.data_mut();
    for (i, &s) in buf.data.iter().enumerate() {
        if s <= 0.0 {
            continue;
        }
        let (row, col) = (i / width, i % width);
        let at = ((oy + row) * stride + ox + col) * 4;
        let Some(px) = data.get_mut(at..at + 4) else {
            continue;
        };
        let sa = alpha * clamp01(s);
        let d = [px[0], px[1], px[2], px[3]].map(|v| f32::from(v) / 255.0);
        let out_a = sa + d[3] * (1.0 - sa);
        let out: [f32; 3] = std::array::from_fn(|k| {
            let sc = straight[k] * sa;
            if multiply {
                sc * (1.0 - d[3]) + d[k] * (1.0 - sa) + sc * d[k]
            } else {
                sc + d[k] * (1.0 - sa)
            }
        });
        let a8 = to_u8(out_a);
        for (dst, v) in px.iter_mut().zip(out) {
            *dst = to_u8(v).min(a8);
        }
        px[3] = a8;
    }
}

/// `round(clamp01(v)·255)`.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "value is clamped to [0, 255] before the cast"
)]
fn to_u8(v: f32) -> u8 {
    (clamp01(v) * 255.0 + 0.5).floor() as u8
}

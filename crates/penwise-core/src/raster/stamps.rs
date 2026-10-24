//! Dabs → pixels via a per-stroke coverage buffer (ink-rendering.md §3.4–§3.5, §3.7).
//!
//! Owner: slice e.

use super::{RasterError, Viewport};
use crate::brush::GrainParams;
use crate::geometry::{Dab, StrokeStyle};
use tiny_skia::Pixmap;

/// Single-channel stroke coverage `S ∈ [0, 1]` over the stroke's pixel bbox, reused across strokes.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct CoverageBuffer {
    /// Pixel position of `data[0]` in the target pixmap.
    pub origin_x: i32,
    pub origin_y: i32,
    pub width: u32,
    pub height: u32,
    /// Row-major, `width · height` values.
    pub data: Vec<f32>,
}

impl CoverageBuffer {
    /// Resizes to the given pixel rect and zeroes it (keeps the allocation when large enough).
    ///
    /// # Errors
    /// [`RasterError::InvalidSize`] when `width · height` overflows `u32` or exceeds 2²⁸ pixels.
    pub fn reset(
        &mut self,
        origin_x: i32,
        origin_y: i32,
        width: u32,
        height: u32,
    ) -> Result<(), RasterError> {
        todo!()
    }
}

/// `MyPaint` two-segment falloff (§3.4) at `rr = (dist/r)²`, `0 < h ≤ 1`:
/// `rr ≤ h → 1 + rr·(1 − 1/h)`; `h < rr ≤ 1 → (h/(1 − h))·(1 − rr)`; `rr > 1 → 0`. `h ≥ 1` → hard disc
/// (1 inside, 0 outside).
#[must_use]
pub fn dab_falloff(rr: f32, hardness: f32) -> f32 {
    todo!()
}

/// Minimum 1 px fade (§3.4): when `r·(1 − h) < 1`, re-solve keeping the optical radius
/// `r_opt = r − (1 − h)·r/2`: `h' = (r_opt − 0.5)/(r_opt + 0.5)`, `r' = 1/(1 − h')`; otherwise returns
/// `(r, h)` unchanged. Inputs are pixel radii.
#[must_use]
pub fn min_fade(radius: f32, hardness: f32) -> (f32, f32) {
    todo!()
}

/// Renders one stamp stroke: resets `scratch` to the stroke's pixel bbox (clipped to the pixmap), then for each
/// dab in order and each covered pixel center: rotate the offset by −angle, `rr = (x'/rx)² + (y'/ry)²` (after
/// [`min_fade`] on both radii), `a = dab_falloff(rr, h) · dab.opacity · g` where
/// `g = grain_coverage(paper_height(page_xy, grain.scale), dab.grain_level, grain.softness, strength)`
/// (`stamp::texture`; `strength = dab.grain_strength`; `g = 1` without grain), and `S ← S + a·(1 − S)`. Finally composites
/// `color` at alpha `color.a/255 · style.opacity · S` per pixel with `style.blend` (`SourceOver` / `Multiply`).
///
/// # Errors
/// [`RasterError::InvalidViewport`]; [`RasterError::InvalidSize`] from [`CoverageBuffer::reset`].
pub fn render_stamps(
    pixmap: &mut Pixmap,
    dabs: &[Dab],
    grain: Option<&GrainParams>,
    style: &StrokeStyle,
    viewport: &Viewport,
    scratch: &mut CoverageBuffer,
) -> Result<(), RasterError> {
    todo!()
}

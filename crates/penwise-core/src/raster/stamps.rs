//! Dabs → pixels via a per-stroke coverage buffer (ink-rendering.md §3.4–§3.5, §3.7).
//!
//! Owner: slice e.
//!
//! Each stroke accumulates its dabs into [`CoverageBuffer`] with the union rule `S ← S + a·(1 − S)`, then
//! composites once at the stroke opacity cap (`composite`), so overlapping dabs of one stroke never exceed it.

mod composite;

use super::{RasterError, Viewport, stroke_alpha};
use crate::brush::GrainParams;
use crate::float::{clamp01, max, min};
use crate::geometry::{Dab, StrokeStyle};
use crate::stamp::texture::{grain_coverage, paper_height};
use composite::composite;
use tiny_skia::Pixmap;

/// Largest coverage buffer, pixels (2²⁸).
const MAX_PIXELS: u32 = 1 << 28;
/// Floor for the re-solved hardness of sub-pixel dabs (keeps them visible instead of fading to nothing).
const MIN_FADE_HARDNESS: f32 = 0.1;
/// Floor for dab hardness (`dab_falloff` is 0 everywhere at `h = 0`).
const MIN_HARDNESS: f32 = 1.0e-3;

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
        let len = width
            .checked_mul(height)
            .filter(|&n| n <= MAX_PIXELS)
            .ok_or(RasterError::InvalidSize { width, height })?;
        self.origin_x = origin_x;
        self.origin_y = origin_y;
        self.width = width;
        self.height = height;
        self.data.clear();
        self.data.resize(len as usize, 0.0);
        Ok(())
    }
}

/// `MyPaint` two-segment falloff (§3.4) at `rr = (dist/r)²`, `0 < h ≤ 1`:
/// `rr ≤ h → 1 + rr·(1 − 1/h)`; `h < rr ≤ 1 → (h/(1 − h))·(1 − rr)`; `rr > 1 → 0`. `h ≥ 1` → hard disc
/// (1 inside, 0 outside).
#[must_use]
pub fn dab_falloff(rr: f32, hardness: f32) -> f32 {
    if rr.is_nan() || rr > 1.0 {
        return 0.0;
    }
    if hardness.is_nan() || hardness >= 1.0 {
        return 1.0;
    }
    if hardness <= 0.0 {
        return 0.0;
    }
    let rr = max(rr, 0.0);
    let opa = if rr <= hardness {
        1.0 + rr * (1.0 - 1.0 / hardness)
    } else {
        hardness / (1.0 - hardness) * (1.0 - rr)
    };
    clamp01(opa)
}

/// Minimum 1 px fade (§3.4): when `r·(1 − h) < 1`, re-solve keeping the optical radius
/// `r_opt = r − (1 − h)·r/2`: `h' = (r_opt − 0.5)/(r_opt + 0.5)`, `r' = 1/(1 − h')`, and report `faded = true`;
/// otherwise returns `(r, h, false)` unchanged. Inputs are pixel radii.
///
/// Sub-pixel dabs (`r_opt < 0.61`) would get `h' ≤ 0.1` (≤ 0 vanishes entirely), so `h'` is floored at 0.1.
/// The flag is explicit because `h'` may equal `h` bit for bit (e.g. `h = 0.1` at the floor) while `r` changed.
/// Non-positive or non-finite radii are returned unchanged.
#[must_use]
pub fn min_fade(radius: f32, hardness: f32) -> (f32, f32, bool) {
    let needs_fade = radius > 0.0 && radius.is_finite() && radius * (1.0 - hardness) < 1.0;
    if !needs_fade {
        return (radius, hardness, false);
    }
    let r_opt = radius - (1.0 - hardness) * radius / 2.0;
    let h = max((r_opt - 0.5) / (r_opt + 0.5), MIN_FADE_HARDNESS);
    (1.0 / (1.0 - h), h, true)
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
    viewport.check()?;
    let alpha = stroke_alpha(style);
    let rect = stroke_rect(dabs, viewport, pixmap.width(), pixmap.height());
    let Some([x, y, w, h]) = rect.filter(|_| alpha > 0.0) else {
        return scratch.reset(0, 0, 0, 0);
    };
    scratch.reset(x.cast_signed(), y.cast_signed(), w, h)?;
    for dab in dabs {
        if let Some(pd) = PixelDab::new(dab, viewport) {
            accumulate(scratch, &pd, grain, viewport);
        }
    }
    composite(pixmap, scratch, style, alpha);
    Ok(())
}

/// A dab in pixel space with the minimum fade applied.
struct PixelDab {
    cx: f32,
    cy: f32,
    inv_rx2: f32,
    inv_ry2: f32,
    /// Circumscribing radius (bbox half-extent).
    extent: f32,
    sin: f32,
    cos: f32,
    hardness: f32,
    opacity: f32,
    grain_strength: f32,
    grain_level: f32,
}

impl PixelDab {
    /// `None` for invisible or malformed dabs (non-finite fields, non-positive radii or opacity).
    fn new(dab: &Dab, viewport: &Viewport) -> Option<Self> {
        let fields = [dab.x, dab.y, dab.radius_x, dab.radius_y, dab.angle];
        let opacity = clamp01(dab.opacity);
        let positive = dab.radius_x > 0.0 && dab.radius_y > 0.0 && opacity > 0.0;
        if !positive || fields.iter().any(|v| !v.is_finite()) {
            return None;
        }
        let [cx, cy] = viewport.map(dab.x, dab.y);
        let (rx, ry) = (dab.radius_x * viewport.scale, dab.radius_y * viewport.scale);
        let h = max(clamp01(dab.hardness), MIN_HARDNESS);
        // Fade along the minor axis; the major axis keeps its optical radius `r·(1 + h)/2` at the new hardness but
        // never ends up shorter than the faded minor axis (the 0.1 hardness floor breaks optical-radius parity, so
        // a circular sub-pixel dab would otherwise turn into an ellipse).
        let (r_minor, faded_h, faded) = min_fade(min(rx, ry), h);
        let keep = |r: f32| {
            if faded {
                max(r * (1.0 + h) / (1.0 + faded_h), r_minor)
            } else {
                r
            }
        };
        let (rx, ry) = if rx < ry {
            (r_minor, keep(ry))
        } else {
            (keep(rx), r_minor)
        };
        let (sin, cos) = libm::sincosf(dab.angle);
        let extent = max(rx, ry);
        if !(cx.is_finite() && cy.is_finite() && extent.is_finite()) {
            return None;
        }
        Some(Self {
            cx,
            cy,
            inv_rx2: 1.0 / (rx * rx),
            inv_ry2: 1.0 / (ry * ry),
            extent,
            sin,
            cos,
            hardness: faded_h,
            opacity,
            grain_strength: clamp01(dab.grain_strength),
            grain_level: dab.grain_level,
        })
    }

    /// Integer pixel span `[lo, hi)` per axis covering the dab, clipped to `[0, limit]` (as `f32`).
    fn span(center: f32, extent: f32, limit: f32) -> (f32, f32) {
        let lo = (center - extent).floor().clamp(0.0, limit);
        let hi = (center + extent).ceil().clamp(0.0, limit);
        (lo, hi)
    }
}

/// Union of dab pixel bboxes clipped to the pixmap: `[x, y, width, height]`, `None` when empty.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "spans are clamped to [0, pixmap size] before the casts; sizes are far below 2^24"
)]
fn stroke_rect(dabs: &[Dab], viewport: &Viewport, width: u32, height: u32) -> Option<[u32; 4]> {
    let (wf, hf) = (width as f32, height as f32);
    let (mut x0, mut y0, mut x1, mut y1) = (wf, hf, 0.0f32, 0.0f32);
    for pd in dabs.iter().filter_map(|d| PixelDab::new(d, viewport)) {
        let (lx, hx) = PixelDab::span(pd.cx, pd.extent, wf);
        let (ly, hy) = PixelDab::span(pd.cy, pd.extent, hf);
        if lx < hx && ly < hy {
            (x0, y0, x1, y1) = (min(x0, lx), min(y0, ly), max(x1, hx), max(y1, hy));
        }
    }
    (x0 < x1 && y0 < y1).then_some([x0 as u32, y0 as u32, (x1 - x0) as u32, (y1 - y0) as u32])
}

/// Adds one dab to the coverage buffer (union rule).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "spans are clamped to the buffer before the casts; pixel coordinates are far below 2^24"
)]
fn accumulate(buf: &mut CoverageBuffer, pd: &PixelDab, grain: Option<&GrainParams>, vp: &Viewport) {
    let (ox, oy) = (buf.origin_x as f32, buf.origin_y as f32);
    let (lx, hx) = PixelDab::span(pd.cx - ox, pd.extent, buf.width as f32);
    let (ly, hy) = PixelDab::span(pd.cy - oy, pd.extent, buf.height as f32);
    let grain = grain.filter(|_| pd.grain_strength > 0.0);
    let stride = buf.width as usize;
    for row in (ly as usize)..(hy as usize) {
        let py = oy + row as f32 + 0.5;
        let fy = py - pd.cy;
        for col in (lx as usize)..(hx as usize) {
            let px = ox + col as f32 + 0.5;
            let fx = px - pd.cx;
            let xr = fx * pd.cos + fy * pd.sin;
            let yr = fy * pd.cos - fx * pd.sin;
            let rr = xr * xr * pd.inv_rx2 + yr * yr * pd.inv_ry2;
            let falloff = dab_falloff(rr, pd.hardness);
            if falloff <= 0.0 {
                continue;
            }
            let mut a = falloff * pd.opacity;
            if let Some(g) = grain {
                let (qx, qy) = (px / vp.scale + vp.origin_x, py / vp.scale + vp.origin_y);
                let height = paper_height(qx, qy, g.scale);
                a *= grain_coverage(height, pd.grain_level, g.softness, pd.grain_strength);
            }
            if let Some(s) = buf.data.get_mut(row * stride + col) {
                *s += clamp01(a) * (1.0 - *s);
            }
        }
    }
}

#[cfg(test)]
mod tests;

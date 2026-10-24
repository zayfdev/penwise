//! Geometry → pixels with `tiny-skia` (feature `raster`; server/native only, never wasm).
//!
//! Owner: slice e.
//!
//! Pixels only need perceptual equality (goldens: decoded-pixel tolerance, `png::PixelTolerance`), but stay as
//! deterministic as practical (core-determinism.md §3): feed tiny-skia only flattened polygons (no curves),
//! keep the default linear color space, use only `SourceOver`/`Multiply`, build transforms with `libm::sincosf`
//! (never `Transform::from_rotate`).

pub mod outline;
pub mod png;
pub mod sheet;
pub mod stamps;

/// Re-export so callers use the exact `tiny-skia` version core was built against.
pub use tiny_skia;

pub use crate::bounds::Bounds;
use crate::brush::BlendMode;
use crate::float::clamp01;
use crate::geometry::{StrokeGeometry, StrokeStyle};
use ::tiny_skia::Pixmap;
pub use stamps::CoverageBuffer;

/// Page → pixel mapping: `px = (page_x − origin_x)·scale`, `py = (page_y − origin_y)·scale`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Viewport {
    pub origin_x: f32,
    pub origin_y: f32,
    /// Pixels per page px (`> 0`).
    pub scale: f32,
}

impl Viewport {
    /// Identity mapping (page px = pixels).
    pub const IDENTITY: Self = Self {
        origin_x: 0.0,
        origin_y: 0.0,
        scale: 1.0,
    };

    /// `Ok` when the mapping is usable (finite origin, finite positive scale).
    pub(crate) fn check(&self) -> Result<(), RasterError> {
        if self.scale.is_finite()
            && self.scale > 0.0
            && self.origin_x.is_finite()
            && self.origin_y.is_finite()
        {
            Ok(())
        } else {
            Err(RasterError::InvalidViewport)
        }
    }

    /// Page px → pixel coordinates.
    pub(crate) fn map(&self, x: f32, y: f32) -> [f32; 2] {
        [
            (x - self.origin_x) * self.scale,
            (y - self.origin_y) * self.scale,
        ]
    }

    /// Page-space rectangle covered by a `width × height` pixmap.
    #[allow(
        clippy::cast_precision_loss,
        reason = "pixmap dimensions are far below 2^24"
    )]
    fn page_rect(&self, width: u32, height: u32) -> Bounds {
        Bounds {
            min_x: self.origin_x,
            min_y: self.origin_y,
            max_x: self.origin_x + width as f32 / self.scale,
            max_y: self.origin_y + height as f32 / self.scale,
        }
    }
}

/// Raster failures.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum RasterError {
    /// Zero or too-large pixmap / coverage buffer dimensions.
    #[error("invalid raster size {width}x{height}")]
    InvalidSize { width: u32, height: u32 },
    /// Viewport scale not finite and positive.
    #[error("invalid viewport")]
    InvalidViewport,
    /// Two images to compare differ in size.
    #[error("image size mismatch: {a_width}x{a_height} vs {b_width}x{b_height}")]
    SizeMismatch {
        a_width: u32,
        a_height: u32,
        b_width: u32,
        b_height: u32,
    },
    /// PNG encoding failed.
    #[error("PNG encode failed")]
    PngEncode,
    /// PNG bytes could not be decoded.
    #[error("PNG decode failed")]
    PngDecode,
}

/// Renders strokes of **one layer** onto `pixmap` in two bands (ink-rendering.md §2.4): first every
/// `BlendMode::Multiply` stroke (under-ink band), then every other stroke, each band in slice order (stable).
/// Outline strokes go through [`outline::fill_outline`]; stamp strokes through [`stamps::render_stamps`]
/// using `scratch` as the per-stroke coverage buffer. Strokes whose bounds miss the pixmap are skipped; empty
/// geometry draws nothing.
///
/// # Errors
/// [`RasterError::InvalidViewport`] for a non-finite or non-positive scale; errors from the per-family
/// renderers.
pub fn render_strokes(
    strokes: &[StrokeGeometry],
    viewport: &Viewport,
    pixmap: &mut Pixmap,
    scratch: &mut CoverageBuffer,
) -> Result<(), RasterError> {
    viewport.check()?;
    // Slack of 1 pixel + 1 page px covers anti-aliasing and the 1 px minimum dab fade.
    let page = viewport
        .page_rect(pixmap.width(), pixmap.height())
        .inflate(1.0 + 1.0 / viewport.scale);
    for multiply_band in [true, false] {
        for stroke in strokes {
            let is_multiply = stroke.style().blend == BlendMode::Multiply;
            if is_multiply == multiply_band && stroke.bounds().intersects(&page) {
                render_one(stroke, viewport, pixmap, scratch)?;
            }
        }
    }
    Ok(())
}

fn render_one(
    stroke: &StrokeGeometry,
    viewport: &Viewport,
    pixmap: &mut Pixmap,
    scratch: &mut CoverageBuffer,
) -> Result<(), RasterError> {
    match stroke {
        StrokeGeometry::Outline { style, outline } => {
            outline::fill_outline(pixmap, outline, style, viewport)
        }
        StrokeGeometry::Stamps { style, grain, dabs } => {
            stamps::render_stamps(pixmap, dabs, grain.as_ref(), style, viewport, scratch)
        }
    }
}

/// Effective straight alpha `color.a/255 · opacity`, in `[0, 1]`.
pub(crate) fn stroke_alpha(style: &StrokeStyle) -> f32 {
    clamp01(f32::from(style.color.a) / 255.0 * clamp01(style.opacity))
}

#[cfg(test)]
mod tests;

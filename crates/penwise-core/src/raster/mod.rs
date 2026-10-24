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
pub mod stamps;

/// Re-export so callers use the exact `tiny-skia` version core was built against.
pub use tiny_skia;

use crate::geometry::StrokeGeometry;
use ::tiny_skia::Pixmap;
use stamps::CoverageBuffer;

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
    todo!()
}

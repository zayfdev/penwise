//! PNG encode/decode and the golden pixel comparator (core-determinism.md §3). Compare decoded pixels, never
//! PNG bytes (they depend on `png`/deflate versions).
//!
//! Owner: slice e.

use super::RasterError;
use tiny_skia::Pixmap;

/// Golden tolerance: pass when the max channel delta ≤ `max_channel_delta` **and** the fraction of pixels with
/// any channel differing is < `max_differing_fraction`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PixelTolerance {
    pub max_channel_delta: u8,
    pub max_differing_fraction: f32,
}

impl PixelTolerance {
    /// Server golden default: delta ≤ 2, < 0.1 % of pixels differ.
    pub const GOLDEN: Self = Self {
        max_channel_delta: 2,
        max_differing_fraction: 0.001,
    };
}

/// Result of comparing two same-size images (demultiplied RGBA8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PixelDiff {
    /// Largest per-channel absolute difference.
    pub max_delta: u8,
    /// Pixels where any channel differs.
    pub differing: u64,
    /// `width · height`.
    pub total: u64,
}

impl PixelDiff {
    /// `max_delta ≤ tol.max_channel_delta && differing / total < tol.max_differing_fraction` (computed in
    /// `f64`; `total == 0` passes).
    #[must_use]
    pub fn passes(&self, tol: PixelTolerance) -> bool {
        todo!()
    }
}

/// Encodes `pixmap` as PNG (tiny-skia demultiplies internally).
///
/// # Errors
/// [`RasterError::PngEncode`].
pub fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, RasterError> {
    todo!()
}

/// Decodes PNG bytes into a premultiplied pixmap.
///
/// # Errors
/// [`RasterError::PngDecode`].
pub fn decode_png(bytes: &[u8]) -> Result<Pixmap, RasterError> {
    todo!()
}

/// Compares demultiplied RGBA8 of `a` and `b` channel by channel.
///
/// # Errors
/// [`RasterError::SizeMismatch`] when dimensions differ.
pub fn compare_pixels(a: &Pixmap, b: &Pixmap) -> Result<PixelDiff, RasterError> {
    todo!()
}

/// Diff visualization for failure artifacts, same size as the inputs, opaque: per pixel with channel delta
/// `d = max over RGBA`: `d == 0` → white, `0 < d ≤ tol.max_channel_delta` → gray `255 − 40·d` (saturating),
/// `d > tol.max_channel_delta` → red `(255, 0, 0)`.
///
/// # Errors
/// [`RasterError::SizeMismatch`]; [`RasterError::InvalidSize`] if the pixmap cannot be allocated.
pub fn diff_image(a: &Pixmap, b: &Pixmap, tol: PixelTolerance) -> Result<Pixmap, RasterError> {
    todo!()
}

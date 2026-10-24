//! PNG encode/decode and the golden pixel comparator (core-determinism.md §3). Compare decoded pixels, never
//! PNG bytes (they depend on `png`/deflate versions).
//!
//! Owner: slice e.

use super::RasterError;
use tiny_skia::{Pixmap, PremultipliedColorU8};

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
    /// `max_delta ≤ tol.max_channel_delta && differing / total < tol.max_differing_fraction`; `total == 0` passes.
    /// The ratio is rounded to `f32` before comparing, the threshold's own precision: widening the `f32` threshold
    /// instead would turn `0.001` into `0.00100000005` and let exactly 0.1 % pass.
    #[must_use]
    pub fn passes(&self, tol: PixelTolerance) -> bool {
        if self.total == 0 {
            return true;
        }
        #[allow(
            clippy::cast_precision_loss,
            clippy::cast_possible_truncation,
            reason = "pixel counts are far below 2^52; f32 is the threshold's precision"
        )]
        let fraction = (self.differing as f64 / self.total as f64) as f32;
        self.max_delta <= tol.max_channel_delta && fraction < tol.max_differing_fraction
    }
}

/// Encodes `pixmap` as PNG (tiny-skia demultiplies internally).
///
/// # Errors
/// [`RasterError::PngEncode`].
pub fn encode_png(pixmap: &Pixmap) -> Result<Vec<u8>, RasterError> {
    pixmap.encode_png().map_err(|_| RasterError::PngEncode)
}

/// Decodes PNG bytes into a premultiplied pixmap.
///
/// # Errors
/// [`RasterError::PngDecode`].
pub fn decode_png(bytes: &[u8]) -> Result<Pixmap, RasterError> {
    Pixmap::decode_png(bytes).map_err(|_| RasterError::PngDecode)
}

/// Compares demultiplied RGBA8 of `a` and `b` channel by channel.
///
/// # Errors
/// [`RasterError::SizeMismatch`] when dimensions differ.
pub fn compare_pixels(a: &Pixmap, b: &Pixmap) -> Result<PixelDiff, RasterError> {
    same_size(a, b)?;
    let mut diff = PixelDiff {
        max_delta: 0,
        differing: 0,
        total: u64::from(a.width()) * u64::from(a.height()),
    };
    for (pa, pb) in a.pixels().iter().zip(b.pixels()) {
        let d = pixel_delta(*pa, *pb);
        if d > 0 {
            diff.differing += 1;
            diff.max_delta = diff.max_delta.max(d);
        }
    }
    Ok(diff)
}

/// Diff visualization for failure artifacts, same size as the inputs, opaque: per pixel with channel delta
/// `d = max over RGBA`: `d == 0` → white, `0 < d ≤ tol.max_channel_delta` → gray `255 − 40·d` (saturating),
/// `d > tol.max_channel_delta` → red `(255, 0, 0)`.
///
/// # Errors
/// [`RasterError::SizeMismatch`]; [`RasterError::InvalidSize`] if the pixmap cannot be allocated.
pub fn diff_image(a: &Pixmap, b: &Pixmap, tol: PixelTolerance) -> Result<Pixmap, RasterError> {
    same_size(a, b)?;
    let (width, height) = (a.width(), a.height());
    let mut out = Pixmap::new(width, height).ok_or(RasterError::InvalidSize { width, height })?;
    let dst = out.data_mut().chunks_exact_mut(4);
    for ((pa, pb), px) in a.pixels().iter().zip(b.pixels()).zip(dst) {
        let d = pixel_delta(*pa, *pb);
        let rgb = if d == 0 {
            [255, 255, 255]
        } else if d <= tol.max_channel_delta {
            let gray = u8::try_from(255u32.saturating_sub(40 * u32::from(d))).unwrap_or(0);
            [gray; 3]
        } else {
            [255, 0, 0]
        };
        px.copy_from_slice(&[rgb[0], rgb[1], rgb[2], 255]);
    }
    Ok(out)
}

fn same_size(a: &Pixmap, b: &Pixmap) -> Result<(), RasterError> {
    if a.width() == b.width() && a.height() == b.height() {
        Ok(())
    } else {
        Err(RasterError::SizeMismatch {
            a_width: a.width(),
            a_height: a.height(),
            b_width: b.width(),
            b_height: b.height(),
        })
    }
}

/// Largest absolute channel difference of the demultiplied RGBA8 values.
fn pixel_delta(a: PremultipliedColorU8, b: PremultipliedColorU8) -> u8 {
    let (a, b) = (a.demultiply(), b.demultiply());
    [
        a.red().abs_diff(b.red()),
        a.green().abs_diff(b.green()),
        a.blue().abs_diff(b.blue()),
        a.alpha().abs_diff(b.alpha()),
    ]
    .into_iter()
    .max()
    .unwrap_or(0)
}

#[cfg(test)]
mod tests;

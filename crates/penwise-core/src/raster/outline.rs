//! Outline polygons → pixels.
//!
//! Owner: slice e.

use super::{RasterError, Viewport};
use crate::geometry::{Outline, StrokeStyle};
use tiny_skia::Pixmap;

/// Fills `outline` (every contour as a closed subpath of one path, `FillRule::Winding`, anti-aliased) with
/// `style.color` at alpha `color.a/255 · style.opacity`, blend `SourceOver` for `Normal` and `Multiply` for
/// `Multiply`. Points are mapped through `viewport` before path building (lines only, no curves). Outlines with
/// fewer than 3 points in a contour skip that contour; an outline that yields no path draws nothing (`Ok`).
///
/// # Errors
/// [`RasterError::InvalidViewport`] for a non-finite or non-positive scale.
pub fn fill_outline(
    pixmap: &mut Pixmap,
    outline: &Outline,
    style: &StrokeStyle,
    viewport: &Viewport,
) -> Result<(), RasterError> {
    todo!()
}

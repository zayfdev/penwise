//! Binary scanline rasterizer: polygons (nonzero) and capsules onto a bit grid, cell-centre sampling.
//!
//! Owner: slice c (slice-internal). Independent of `raster/` (tiny-skia): only `+ − × ÷ sqrt`, so walls are
//! bit-identical on every target.

use super::grid::Window;

/// Sets every cell whose centre is inside the nonzero union of `contours` (page px; each implicitly closed).
pub(crate) fn fill_contours(window: &Window, contours: &[&[[f32; 2]]], bits: &mut [u64]) {
    todo!()
}

/// Sets every cell whose centre is within `radius` of segment `a`–`b` (a capsule); `radius` is raised to
/// `FILL_CELL / 2` so thin lines still form a closed wall one cell wide.
pub(crate) fn fill_capsule(
    window: &Window,
    a: [f32; 2],
    b: [f32; 2],
    radius: f32,
    bits: &mut [u64],
) {
    todo!()
}

/// Clears every cell whose centre is within `radius` of segment `a`–`b` (erase masks punched out of walls).
pub(crate) fn clear_capsule(
    window: &Window,
    a: [f32; 2],
    b: [f32; 2],
    radius: f32,
    bits: &mut [u64],
) {
    todo!()
}

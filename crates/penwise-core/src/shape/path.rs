//! Shape path construction and Bézier flattening.
//!
//! Owner: slice d.
//!
//! Every builder appends to a [`ShapePath`] in page px; all take the box `size = [w, h]` in local px and the
//! item transform. Determinism: only `+ − × ÷ sqrt`; flattening subdivides by a fixed rule (below), never by
//! floating step counts that could differ across targets.

use super::ShapePath;
use crate::item::{Affine, ArrowHead};

/// Bézier circle constant `4/3·(√2 − 1)`.
pub const KAPPA: f32 = 0.552_284_8;

/// `cos 28°`, `sin 28°`: arrowhead barb rotation (no trig at build time).
pub const ARROW_COS: f32 = 0.882_947_6;
/// See [`ARROW_COS`].
pub const ARROW_SIN: f32 = 0.469_471_6;

/// Appends the flattening of cubic `p0 p1 p2 p3` (page px) to `out`, excluding `p0` (the caller emitted it):
/// recursive-free subdivision with an explicit stack, splitting at `t = 0.5` until the control-polygon
/// flatness `max(dist(p1, chord), dist(p2, chord)) ≤ FLATTEN_TOLERANCE` or depth 10.
pub fn flatten_cubic(
    p0: [f32; 2],
    p1: [f32; 2],
    p2: [f32; 2],
    p3: [f32; 2],
    out: &mut Vec<[f32; 2]>,
) {
    todo!()
}

/// Closed rectangle: corners `(−w/2, −h/2)`, `(w/2, −h/2)`, `(w/2, h/2)`, `(−w/2, h/2)` mapped.
pub fn rect(size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    todo!()
}

/// Closed rounded rectangle, `radius` clamped to `[0, min(w, h) / 2]` (local px). For similarity transforms
/// the corners are quarter-arc Béziers built in page space from the mapped corners (round under rotation);
/// otherwise the local construction (4 lines + 4 quarter Béziers) is mapped (corners become elliptical under
/// shear, as the user dragged). Radius 0 → [`rect`].
pub fn rounded_rect(size: [f32; 2], radius: f32, transform: &Affine, out: &mut ShapePath) {
    todo!()
}

/// Closed ellipse inscribed in the box: 4 cubic Béziers with [`KAPPA`], control points mapped through the
/// transform (exact), then [`flatten_cubic`]. Starts at the local right-most point, clockwise on screen.
pub fn ellipse(size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    todo!()
}

/// Open segment `(−w/2, 0) → (w/2, 0)` mapped.
pub fn line(size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    todo!()
}

/// [`line`] plus heads. Head length `clamp(4 · stroke_width, 8, 0.4 · shaft_length)` page px measured on the
/// mapped shaft; barbs = the unit shaft direction rotated by ±28° ([`ARROW_COS`], [`ARROW_SIN`]) scaled by
/// the head length, from the tip backwards. A zero-length shaft gets no heads.
pub fn arrow(
    size: [f32; 2],
    head: ArrowHead,
    stroke_width: f32,
    transform: &Affine,
    out: &mut ShapePath,
) {
    todo!()
}

/// Closed polygon: unit-box `points` scaled by `size`, then mapped.
pub fn polygon(points: &[[f32; 2]], size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    todo!()
}

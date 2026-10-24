//! Shared 2D primitives for hit testing, lasso, erasers, fill and handles: distances, winding numbers,
//! areas, arc-length resampling.
//!
//! Owner: slice a. Slices b, c, d call these instead of writing their own.
//!
//! Determinism: only `+ − × ÷ sqrt` (`f64` accumulators where noted), no `libm` needed; ties resolved
//! explicitly so results never depend on iteration order.

use crate::item::Polygon;

/// Squared distance from `p` to segment `a`–`b`, and the clamped projection parameter `t ∈ [0, 1]`
/// (`t = 0` for a degenerate segment).
#[must_use]
pub fn point_segment_dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    todo!()
}

/// Distance between segments `a0–a1` and `b0–b1`: `0` when they intersect (orientation test with the
/// collinear/touching cases counted as intersecting), else the minimum of the four point–segment distances.
#[must_use]
pub fn segment_segment_dist(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> f32 {
    todo!()
}

/// Distance from `p` to an open polyline (a single point is a degenerate segment; empty → `f32::INFINITY`).
#[must_use]
pub fn point_polyline_dist(p: [f32; 2], line: &[[f32; 2]]) -> f32 {
    todo!()
}

/// Winding number of the closed `ring` (implicitly closed) around `p`, half-open edge rule
/// (an upward edge includes its start vertex, a downward edge its end vertex), so points on shared edges are
/// counted exactly once across adjacent rings.
#[must_use]
pub fn winding_number(p: [f32; 2], ring: &[[f32; 2]]) -> i32 {
    todo!()
}

/// Nonzero point-in-polygon over the outer ring and holes of every polygon (sum of winding numbers ≠ 0).
#[must_use]
pub fn polygons_contain(polygons: &[Polygon], p: [f32; 2]) -> bool {
    todo!()
}

/// Nonzero point-in-ring for a single implicitly closed ring (lasso polygons).
#[must_use]
pub fn ring_contains(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    todo!()
}

/// Shoelace sum `Σ (x_i·y_{i+1} − x_{i+1}·y_i) / 2` of an implicitly closed ring, accumulated in `f64`.
#[must_use]
pub fn signed_area(ring: &[[f32; 2]]) -> f32 {
    todo!()
}

/// Distance from `p` to the boundary of an implicitly closed ring.
#[must_use]
pub fn point_ring_dist(p: [f32; 2], ring: &[[f32; 2]]) -> f32 {
    todo!()
}

/// Appends points every `step` of arc length along `line` (the first vertex, then interior points, then the
/// last vertex), using an `f64` running arc length. `closed` adds the closing segment. `step ≤ 0` or
/// non-finite → only the vertices.
pub fn resample(line: &[[f32; 2]], closed: bool, step: f32, out: &mut Vec<[f32; 2]>) {
    todo!()
}

/// Arc length of `line` (`closed` adds the closing segment), `f64` accumulator.
#[must_use]
pub fn arc_length(line: &[[f32; 2]], closed: bool) -> f32 {
    todo!()
}

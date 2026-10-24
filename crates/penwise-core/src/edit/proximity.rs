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
#[allow(
    clippy::cast_possible_truncation,
    reason = "f64 working precision, one rounding to f32"
)]
pub fn point_segment_dist2(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> (f32, f32) {
    let (px, py) = wide(p);
    let (ax, ay) = wide(a);
    let (bx, by) = wide(b);
    let (dx, dy) = (bx - ax, by - ay);
    let len2 = dx * dx + dy * dy;
    let t = if len2 > 0.0 {
        ((px - ax) * dx + (py - ay) * dy) / len2
    } else {
        0.0
    };
    let t = t.clamp(0.0, 1.0);
    let (qx, qy) = (ax + dx * t - px, ay + dy * t - py);
    ((qx * qx + qy * qy) as f32, t as f32)
}

/// Distance between segments `a0–a1` and `b0–b1`: `0` when they intersect (orientation test with the
/// collinear/touching cases counted as intersecting), else the minimum of the four point–segment distances.
#[must_use]
pub fn segment_segment_dist(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> f32 {
    if segments_intersect(a0, a1, b0, b1) {
        return 0.0;
    }
    let d2 = [
        point_segment_dist2(a0, b0, b1).0,
        point_segment_dist2(a1, b0, b1).0,
        point_segment_dist2(b0, a0, a1).0,
        point_segment_dist2(b1, a0, a1).0,
    ];
    d2.iter()
        .fold(f32::INFINITY, |m, &d| crate::float::min(m, d))
        .sqrt()
}

/// Distance from `p` to an open polyline (a single point is a degenerate segment; empty → `f32::INFINITY`).
#[must_use]
pub fn point_polyline_dist(p: [f32; 2], line: &[[f32; 2]]) -> f32 {
    match line {
        [] => f32::INFINITY,
        [only] => point_segment_dist2(p, *only, *only).0.sqrt(),
        _ => line
            .windows(2)
            .fold(f32::INFINITY, |m, w| {
                crate::float::min(m, point_segment_dist2(p, w[0], w[1]).0)
            })
            .sqrt(),
    }
}

/// Winding number of the closed `ring` (implicitly closed) around `p`, half-open edge rule
/// (an upward edge includes its start vertex, a downward edge its end vertex), so points on shared edges are
/// counted exactly once across adjacent rings.
#[must_use]
pub fn winding_number(p: [f32; 2], ring: &[[f32; 2]]) -> i32 {
    let mut wn = 0i32;
    for (a, b) in closed_edges(ring) {
        if a[1] <= p[1] {
            if b[1] > p[1] && orient(a, b, p) > 0.0 {
                wn += 1;
            }
        } else if b[1] <= p[1] && orient(a, b, p) < 0.0 {
            wn -= 1;
        }
    }
    wn
}

/// Nonzero point-in-polygon over the outer ring and holes of every polygon (sum of winding numbers ≠ 0).
#[must_use]
pub fn polygons_contain(polygons: &[Polygon], p: [f32; 2]) -> bool {
    let total: i32 = polygons
        .iter()
        .map(|poly| {
            winding_number(p, &poly.outer)
                + poly.holes.iter().map(|h| winding_number(p, h)).sum::<i32>()
        })
        .sum();
    total != 0
}

/// Nonzero point-in-ring for a single implicitly closed ring (lasso polygons).
#[must_use]
pub fn ring_contains(ring: &[[f32; 2]], p: [f32; 2]) -> bool {
    winding_number(p, ring) != 0
}

/// Shoelace sum `Σ (x_i·y_{i+1} − x_{i+1}·y_i) / 2` of an implicitly closed ring, accumulated in `f64`.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "f64 accumulator, one rounding to f32"
)]
pub fn signed_area(ring: &[[f32; 2]]) -> f32 {
    let sum: f64 = closed_edges(ring)
        .map(|(a, b)| {
            let (ax, ay) = wide(a);
            let (bx, by) = wide(b);
            ax * by - bx * ay
        })
        .sum();
    (sum / 2.0) as f32
}

/// Distance from `p` to the boundary of an implicitly closed ring.
#[must_use]
pub fn point_ring_dist(p: [f32; 2], ring: &[[f32; 2]]) -> f32 {
    match ring {
        [] => f32::INFINITY,
        [only] => point_segment_dist2(p, *only, *only).0.sqrt(),
        _ => closed_edges(ring)
            .fold(f32::INFINITY, |m, (a, b)| {
                crate::float::min(m, point_segment_dist2(p, a, b).0)
            })
            .sqrt(),
    }
}

/// Upper bound on the interpolated points one [`resample`] call emits (the step grows to
/// `arc length / MAX_RESAMPLE_POINTS` for longer lines, so absurdly large paths stay bounded).
pub const MAX_RESAMPLE_POINTS: u32 = 1 << 18;

/// Appends points every `step` of arc length along `line` (the first vertex, then interior points, then the
/// last vertex), using an `f64` running arc length. `closed` adds the closing segment. `step ≤ 0` or
/// non-finite → only the vertices. The effective step is `max(step, arc length / MAX_RESAMPLE_POINTS)`, so at
/// most [`MAX_RESAMPLE_POINTS`] + 2 points are appended.
///
/// A closed walk ends where it started, so for `closed` the final point is not repeated (the output is an
/// implicitly closed ring again); with an invalid `step` every input vertex is appended.
#[allow(
    clippy::cast_possible_truncation,
    reason = "f64 working precision, one rounding to f32"
)]
pub fn resample(line: &[[f32; 2]], closed: bool, step: f32, out: &mut Vec<[f32; 2]>) {
    let Some(&first) = line.first() else { return };
    if !(step.is_finite() && step > 0.0) {
        out.extend_from_slice(line);
        return;
    }
    out.push(first);
    let total: f64 = edges(line, closed)
        .map(|(a, b)| edge_len(a, b))
        .filter(|len| len.is_finite())
        .sum();
    let floor = total / f64::from(MAX_RESAMPLE_POINTS);
    let step = if floor > f64::from(step) {
        floor
    } else {
        f64::from(step)
    };
    let mut walked = 0.0f64; // arc length at the start of the current edge
    let mut k = 1.0f64; // index of the next emitted point (at arc length k·step; no drift)
    for (a, b) in edges(line, closed) {
        let (ax, ay) = wide(a);
        let (bx, by) = wide(b);
        let len = edge_len(a, b);
        if !len.is_finite() {
            continue;
        }
        while k * step < walked + len {
            let t = (k * step - walked) / len;
            out.push([(ax + (bx - ax) * t) as f32, (ay + (by - ay) * t) as f32]);
            k += 1.0;
        }
        walked += len;
    }
    if !closed
        && line.len() > 1
        && let Some(&last) = line.last()
    {
        out.push(last);
    }
}

/// Arc length of `line` (`closed` adds the closing segment), `f64` accumulator.
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "f64 accumulator, one rounding to f32"
)]
pub fn arc_length(line: &[[f32; 2]], closed: bool) -> f32 {
    let sum: f64 = edges(line, closed).map(|(a, b)| edge_len(a, b)).sum();
    sum as f32
}

fn wide(p: [f32; 2]) -> (f64, f64) {
    (f64::from(p[0]), f64::from(p[1]))
}

/// Edge length in `f64`.
fn edge_len(a: [f32; 2], b: [f32; 2]) -> f64 {
    let (ax, ay) = wide(a);
    let (bx, by) = wide(b);
    ((bx - ax) * (bx - ax) + (by - ay) * (by - ay)).sqrt()
}

/// Cross product `(b − a) × (p − a)` in `f64`: `> 0` when `p` is left of `a→b` in y-up maths.
fn orient(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> f64 {
    let (ax, ay) = wide(a);
    let (bx, by) = wide(b);
    let (px, py) = wide(p);
    (bx - ax) * (py - ay) - (px - ax) * (by - ay)
}

fn on_box(a: [f32; 2], b: [f32; 2], p: [f32; 2]) -> bool {
    let (lo_x, hi_x) = if a[0] <= b[0] {
        (a[0], b[0])
    } else {
        (b[0], a[0])
    };
    let (lo_y, hi_y) = if a[1] <= b[1] {
        (a[1], b[1])
    } else {
        (b[1], a[1])
    };
    p[0] >= lo_x && p[0] <= hi_x && p[1] >= lo_y && p[1] <= hi_y
}

fn segments_intersect(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> bool {
    let d1 = orient(b0, b1, a0);
    let d2 = orient(b0, b1, a1);
    let d3 = orient(a0, a1, b0);
    let d4 = orient(a0, a1, b1);
    if ((d1 > 0.0 && d2 < 0.0) || (d1 < 0.0 && d2 > 0.0))
        && ((d3 > 0.0 && d4 < 0.0) || (d3 < 0.0 && d4 > 0.0))
    {
        return true;
    }
    (d1 == 0.0 && on_box(b0, b1, a0))
        || (d2 == 0.0 && on_box(b0, b1, a1))
        || (d3 == 0.0 && on_box(a0, a1, b0))
        || (d4 == 0.0 && on_box(a0, a1, b1))
}

/// Consecutive vertex pairs, plus `last → first` when `closed` and there are ≥ 2 vertices.
fn edges(line: &[[f32; 2]], closed: bool) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    let closing = match (closed, line.first(), line.last()) {
        (true, Some(&first), Some(&last)) if line.len() > 1 => Some((last, first)),
        _ => None,
    };
    line.windows(2).map(|w| (w[0], w[1])).chain(closing)
}

fn closed_edges(ring: &[[f32; 2]]) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    edges(ring, true)
}

#[cfg(test)]
mod tests;

//! Owner: design-harness M3 (core). Arrow detector for the mark classifier: a straight shaft with a short V
//! (one or two barbs) at one end.
//!
//! See [`super::classify`] for the vocabulary, confidence and decision order.

use super::classify::{MIN_SHAFT_LEN_PX, MarkShape, Poly, dist, dot, simplify_indices, sub};
use crate::float::{max, min};

/// A straight run with a short V (one or two barbs) at one end: an arrow.
pub(super) fn arrow(polys: &[Poly]) -> Option<MarkShape> {
    let mut best: Option<(f32, MarkShape)> = None;
    for shaft in shafts(polys) {
        for at_start in [false, true] {
            let Some(score) = head_score(polys, &shaft, at_start) else {
                continue;
            };
            let (from, to) = if at_start {
                (shaft.end, shaft.start)
            } else {
                (shaft.start, shaft.end)
            };
            if best.as_ref().is_none_or(|(seen, _)| score > *seen) {
                best = Some((score, MarkShape::arrow(score, from, to)));
            }
        }
    }
    best.map(|(_, shape)| shape)
}

/// A straight sub-polyline long enough to be an arrow shaft.
struct Shaft {
    poly: usize,
    i0: usize,
    i1: usize,
    start: [f32; 2],
    end: [f32; 2],
    len: f32,
}

fn shafts(polys: &[Poly]) -> Vec<Shaft> {
    let mut out = Vec::new();
    for (pi, p) in polys.iter().enumerate() {
        let eps = max(0.015 * p.diag(), 1.0);
        let indices = simplify_indices(&p.pts, eps);
        for pair in indices.windows(2) {
            let (i0, i1) = (pair[0], pair[1]);
            let start = p.pts[i0];
            let end = p.pts[i1];
            let len = dist(start, end);
            if len >= MIN_SHAFT_LEN_PX {
                out.push(Shaft {
                    poly: pi,
                    i0,
                    i1,
                    start,
                    end,
                    len,
                });
            }
        }
    }
    out
}

/// Confidence of a V-head at one end of `shaft`, or `None` when there is no head.
fn head_score(polys: &[Poly], shaft: &Shaft, at_start: bool) -> Option<f32> {
    let (tip, tail) = if at_start {
        (shaft.start, shaft.end)
    } else {
        (shaft.end, shaft.start)
    };
    let spread = sub(tip, tail);
    let u = [spread[0] / shaft.len, spread[1] / shaft.len];
    let reach = (0.30 * shaft.len).clamp(8.0, 60.0);
    let lateral_min = max(0.18 * reach, 3.0);
    let (mut count, mut max_dist, mut max_lat) = (0_usize, 0.0_f32, 0.0_f32);
    let mut nearest = f32::INFINITY;
    let mut continues = 0_usize;
    let (mut left, mut right) = (false, false);
    for (pi, p) in polys.iter().enumerate() {
        for (i, point) in p.pts.iter().enumerate() {
            if pi == shaft.poly {
                let beyond = if at_start { i < shaft.i0 } else { i > shaft.i1 };
                if !beyond {
                    continue;
                }
            }
            let d = dist(*point, tip);
            if d < 2.0 || d > 1.4 * reach {
                continue;
            }
            let off = sub(*point, tip);
            let along = dot(off, u);
            // Backward of the shaft (a barb, not a continuation) but not folded back along the shaft: real
            // barbs sit 20°–85° off the backward direction, a hairpin return in a scribble sits near 0°.
            if along > -0.10 * d || along < -0.94 * d {
                continue;
            }
            let side = u[0] * off[1] - u[1] * off[0];
            if side.abs() < lateral_min {
                continue;
            }
            if d > reach {
                // The "barb" keeps going past a head: it is another arm (a V, a triangle side), not a head.
                continues += 1;
                continue;
            }
            count += 1;
            nearest = min(nearest, d);
            max_dist = max(max_dist, d);
            max_lat = max(max_lat, side.abs());
            if side > 0.0 {
                left = true;
            } else {
                right = true;
            }
        }
    }
    if max_lat <= 0.0 {
        return None;
    }
    let ratio = max_dist / shaft.len;
    // A head starts at the shaft tip; a stray stroke that happens to pass within reach is not one.
    if count < 2
        || continues >= 2
        || nearest > max(0.35 * reach, 8.0)
        || max_dist < max(0.30 * reach, 6.0)
        || ratio > 0.60
    {
        return None;
    }
    let mut score = 0.6_f32;
    if left && right {
        score += 0.10;
    }
    if (0.12..=0.40).contains(&ratio) {
        score += 0.08;
    }
    if shaft.len >= 60.0 {
        score += 0.05;
    }
    if max_lat >= 0.30 * shaft.len {
        score += 0.03;
    }
    Some(min(score, 0.95))
}

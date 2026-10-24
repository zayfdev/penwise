//! Polygon validity and canonical form via `i_overlay` (editing-algorithms.md §3.8).
//!
//! Owner: slice c (slice-internal).
//!
//! Integer API only: points snapped to the 1/16 px grid (`i32 = round(v · 16)`) before `i_overlay`
//! (`OverlayRule::Subject` self-union, `FillRule::NonZero`), converted back by `/ 16`. No float overlay API.

use std::cmp::Ordering;

use i_overlay::core::fill_rule::FillRule;
use i_overlay::core::overlay::{Overlay, ShapeType};
use i_overlay::core::overlay_rule::OverlayRule;
use i_overlay::i_float::int::point::IntPoint;

use crate::item::Polygon;

/// Grid steps per page px.
const GRID: f64 = 16.0;
/// Largest grid coordinate magnitude handed to `i_overlay` (well inside its `i32` range of `±2³⁰`).
const LIMIT: i64 = 536_870_912;

type Ring = Vec<IntPoint<i32>>;

/// Canonical, valid polygons from possibly self-crossing rings (outer and hole rings mixed, any orientation;
/// classified by the nonzero union itself). Output: outer rings positive shoelace, holes negative, each ring
/// rotated to start at its minimum `(y, x)` vertex, holes sorted by that vertex, polygons sorted by their
/// outer ring's minimum vertex; rings with < 3 points or zero area dropped. Rings with non-finite points are
/// ignored.
pub(crate) fn normalize(rings: &[Vec<[f32; 2]>]) -> Vec<Polygon> {
    let contours: Vec<Ring> = rings.iter().filter_map(|r| to_grid(r)).collect();
    let capacity = contours.iter().map(Vec::len).sum();
    if capacity == 0 {
        return Vec::new();
    }
    let mut overlay = Overlay::<i32>::new(capacity);
    for contour in &contours {
        overlay.add_contour(contour, ShapeType::Subject);
    }
    let shapes = overlay.overlay(OverlayRule::Subject, FillRule::NonZero);
    let mut polygons: Vec<(Ring, Vec<Ring>)> =
        shapes.into_iter().filter_map(canonical_shape).collect();
    polygons.sort_by(|p, q| cmp_first(&p.0, &q.0));
    polygons
        .into_iter()
        .map(|(outer, holes)| Polygon {
            outer: to_page(&outer),
            holes: holes.iter().map(|h| to_page(h)).collect(),
        })
        .collect()
}

/// Ring snapped to the grid with consecutive duplicates (and a repeated closing point) removed; `None` for
/// non-finite input or fewer than 3 distinct points.
#[allow(
    clippy::cast_possible_truncation,
    reason = "values are rounded and clamped to ±2^29 first"
)]
fn to_grid(ring: &[[f32; 2]]) -> Option<Ring> {
    let snap = |v: f32| {
        let s = (f64::from(v) * GRID).round();
        // Saturating cast, then integer clamp (equal to clamping the rounded float; NaN maps to 0 either way).
        (s as i64).clamp(-LIMIT, LIMIT) as i32
    };
    let mut out: Ring = Vec::with_capacity(ring.len());
    for p in ring {
        if !(p[0].is_finite() && p[1].is_finite()) {
            return None;
        }
        let q = IntPoint::new(snap(p[0]), snap(p[1]));
        if out.last() != Some(&q) {
            out.push(q);
        }
    }
    while out.len() > 1 && out.first() == out.last() {
        out.pop();
    }
    (out.len() >= 3).then_some(out)
}

/// First contour = outer, the rest holes. Forces orientation (outer positive, holes negative), drops
/// degenerate rings, rotates each ring to its minimum `(y, x)` vertex and sorts holes. `None` when the outer
/// ring is degenerate.
fn canonical_shape(shape: Vec<Ring>) -> Option<(Ring, Vec<Ring>)> {
    let mut contours = shape.into_iter();
    let outer = orient(contours.next()?, true)?;
    let mut holes: Vec<Ring> = contours.filter_map(|h| orient(h, false)).collect();
    holes.sort_by(|a, b| cmp_first(a, b));
    Some((outer, holes))
}

/// Ring with the requested shoelace sign, rotated to its minimum vertex; `None` when degenerate.
fn orient(mut ring: Ring, positive: bool) -> Option<Ring> {
    let area = shoelace(&ring);
    if ring.len() < 3 || area == 0 {
        return None;
    }
    if (area > 0) != positive {
        ring.reverse();
    }
    let start = ring
        .iter()
        .enumerate()
        .min_by(|(_, p), (_, q)| cmp_point(**p, **q))
        .map_or(0, |(i, _)| i);
    ring.rotate_left(start);
    Some(ring)
}

/// Exact shoelace sum `Σ (x_i·y_{i+1} − x_{i+1}·y_i)` in `i128`.
fn shoelace(ring: &[IntPoint<i32>]) -> i128 {
    let Some(&last) = ring.last() else {
        return 0;
    };
    let mut prev = last;
    let mut sum = 0i128;
    for &p in ring {
        sum += i128::from(prev.x) * i128::from(p.y) - i128::from(p.x) * i128::from(prev.y);
        prev = p;
    }
    sum
}

/// `(y, x)` order.
fn cmp_point(p: IntPoint<i32>, q: IntPoint<i32>) -> Ordering {
    p.y.cmp(&q.y).then(p.x.cmp(&q.x))
}

/// Order of rings by their first (minimum) vertex.
fn cmp_first(a: &[IntPoint<i32>], b: &[IntPoint<i32>]) -> Ordering {
    match (a.first(), b.first()) {
        (Some(p), Some(q)) => cmp_point(*p, *q),
        (p, q) => p.is_some().cmp(&q.is_some()),
    }
}

/// Grid ring back to page px (exact for `|v| < 2²⁰` px).
#[allow(
    clippy::cast_possible_truncation,
    reason = "one rounding from the exact f64 quotient"
)]
fn to_page(ring: &[IntPoint<i32>]) -> Vec<[f32; 2]> {
    ring.iter()
        .map(|p| {
            [
                (f64::from(p.x) / GRID) as f32,
                (f64::from(p.y) / GRID) as f32,
            ]
        })
        .collect()
}

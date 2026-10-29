//! Owner: design-harness M3 (core). Shape detectors for the mark classifier: closed loops (circle or box),
//! two-stroke crosses and boxes, wavy lines, and horizontal text lines.
//!
//! See [`super::classify`] for the vocabulary, confidence and decision order.

use super::classify::{
    CLOSE_RATIO, MarkKind, MarkShape, Poly, cosine, cross2, dist, dot, sharp_corners, shoelace,
    simplify_indices, sub,
};
use crate::float::{max, min};

/// A single closed stroke: circle (smooth loop) or box (sharp-cornered rectangle).
#[allow(
    clippy::cast_precision_loss,
    reason = "the corner count is at most the point count, far below f32's exact-integer range"
)]
pub(super) fn closed_shape(p: &Poly) -> Option<MarkShape> {
    let gap = dist(p.first(), p.last());
    let limit = max(CLOSE_RATIO * p.diag(), 14.0);
    if gap > limit {
        return None;
    }
    let bbox_area = p.bbox.width() * p.bbox.height();
    if bbox_area <= 1.0 {
        return None;
    }
    let area = shoelace(&p.pts).abs();
    let area_ratio = area / bbox_area;
    let perimeter = p.len + gap;
    let circularity = 4.0 * core::f32::consts::PI * area / (perimeter * perimeter);
    let eps = max(0.03 * p.diag(), 1.5);
    let indices = simplify_indices(&p.pts, eps);
    let verts: Vec<[f32; 2]> = indices.iter().map(|&i| p.pts[i]).collect();
    let corners = sharp_corners(&verts, 0.12 * p.diag());
    let closure_q = (1.0 - gap / limit).clamp(0.0, 1.0);
    if corners <= 1 && circularity >= 0.50 && area_ratio >= 0.35 {
        let confidence =
            0.5 + 0.25 * min(circularity, 1.0) + 0.2 * closure_q + 0.05 * min(area_ratio, 1.0);
        return Some(MarkShape::guessed(MarkKind::Circle, confidence));
    }
    let aspect =
        max(p.bbox.width(), p.bbox.height()) / max(min(p.bbox.width(), p.bbox.height()), 1.0);
    if (2..=6).contains(&corners) && area_ratio >= 0.35 && circularity >= 0.40 && aspect <= 8.0 {
        let confidence = 0.5 + 0.08 * corners as f32 + 0.2 * closure_q;
        return Some(MarkShape::guessed(MarkKind::BoxNew, confidence));
    }
    None
}

/// Two straight strokes crossing inside both: an X.
pub(super) fn two_stroke_cross(a: &Poly, b: &Poly) -> Option<MarkShape> {
    if a.len < 12.0 || b.len < 12.0 || a.straightness() < 0.85 || b.straightness() < 0.85 {
        return None;
    }
    let (a0, a1) = (a.first(), a.last());
    let (b0, b1) = (b.first(), b.last());
    let (_, t, u) = segment_intersection(a0, a1, b0, b1)?;
    if !(0.05..=0.95).contains(&t) || !(0.05..=0.95).contains(&u) {
        return None;
    }
    let (ra, rb) = (sub(a1, a0), sub(b1, b0));
    let cos = (dot(ra, rb) / (a.len * b.len)).abs();
    if cos > 0.906 {
        return None;
    }
    let mut confidence = 0.6;
    if cos < 0.5 {
        confidence += 0.12;
    }
    if (0.25..=0.75).contains(&t) && (0.25..=0.75).contains(&u) {
        confidence += 0.1;
    }
    if a.len / b.len < 2.0 && b.len / a.len < 2.0 {
        confidence += 0.05;
    }
    Some(MarkShape::guessed(MarkKind::Cross, confidence))
}

/// Intersection of segments `a0..a1` and `b0..b1`: point and the two parameters, both `0..=1` at a crossing.
fn segment_intersection(
    a0: [f32; 2],
    a1: [f32; 2],
    b0: [f32; 2],
    b1: [f32; 2],
) -> Option<([f32; 2], f32, f32)> {
    let r = sub(a1, a0);
    let s = sub(b1, b0);
    let denom = cross2(r, s);
    if denom.abs() < 1e-3 {
        return None;
    }
    let q = sub(b0, a0);
    let t = cross2(q, s) / denom;
    let u = cross2(q, r) / denom;
    Some(([a0[0] + r[0] * t, a0[1] + r[1] * t], t, u))
}

/// Two L-shaped strokes whose ends meet: a rectangle drawn in two goes.
pub(super) fn two_stroke_box(a: &Poly, b: &Poly) -> Option<MarkShape> {
    let union = a.bbox.union(&b.bbox);
    let diag = libm::hypotf(union.width(), union.height());
    let eps = max(0.04 * diag, 1.5);
    let verts_a = corners_of(a, eps)?;
    let verts_b = corners_of(b, eps)?;
    let limit = max(0.25 * diag, 12.0);
    let close = |x: [f32; 2], y: [f32; 2]| dist(x, y) <= limit;
    let (a0, a2) = (verts_a[0], verts_a[2]);
    let (b0, b2) = (verts_b[0], verts_b[2]);
    let paired = (close(a0, b0) && close(a2, b2)) || (close(a0, b2) && close(a2, b0));
    if !paired {
        return None;
    }
    let aspect = max(union.width(), union.height()) / max(min(union.width(), union.height()), 1.0);
    if aspect > 8.0 {
        return None;
    }
    Some(MarkShape::guessed(MarkKind::BoxNew, 0.62))
}

/// `[start, corner, end]` of a stroke with exactly one corner of roughly 72°–120° and two long arms.
fn corners_of(p: &Poly, eps: f32) -> Option<[[f32; 2]; 3]> {
    let indices = simplify_indices(&p.pts, eps);
    if indices.len() != 3 {
        return None;
    }
    let verts = [p.pts[indices[0]], p.pts[indices[1]], p.pts[indices[2]]];
    let (arm_in, arm_out) = (dist(verts[0], verts[1]), dist(verts[1], verts[2]));
    let cosine_here = cosine(verts[0], verts[1], verts[2]);
    if arm_in < 8.0 || arm_out < 8.0 || !(-0.5..=0.5).contains(&cosine_here) {
        return None;
    }
    Some(verts)
}

/// A horizontal sine with at least one full oscillation. Returns the confidence.
#[allow(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "bin counts are small positive integers; exact in f32 and usize"
)]
pub(super) fn wavy(p: &Poly) -> Option<f32> {
    let (w, h) = (p.bbox.width(), p.bbox.height());
    if w < 30.0 || h > 0.45 * w || p.len > 2.5 * w {
        return None;
    }
    let bins = ((w / 6.0).round() as usize).clamp(8, 48);
    let mut sums = vec![(0.0_f32, 0_u32); bins];
    for point in &p.pts {
        let i = (((point[0] - p.bbox.min_x) / w) * bins as f32) as usize;
        let entry = &mut sums[i.min(bins - 1)];
        entry.0 += point[1];
        entry.1 += 1;
    }
    let series: Vec<[f32; 2]> = sums
        .iter()
        .enumerate()
        .filter(|(_, (_, n))| *n > 0)
        .map(|(i, (sum, n))| {
            let x = p.bbox.min_x + (i as f32 + 0.5) * w / bins as f32;
            [x, sum / *n as f32]
        })
        .collect();
    if series.len() < 6 {
        return None;
    }
    let n = series.len() as f32;
    let (sx, sy) = series
        .iter()
        .fold((0.0, 0.0), |(sx, sy), point| (sx + point[0], sy + point[1]));
    let (sxx, sxy) = series.iter().fold((0.0, 0.0), |(sxx, sxy), point| {
        (sxx + point[0] * point[0], sxy + point[0] * point[1])
    });
    let denom = n * sxx - sx * sx;
    if denom.abs() < 1e-6 {
        return None;
    }
    let slope = (n * sxy - sx * sy) / denom;
    let intercept = (sy - slope * sx) / n;
    let mut sign = 0_i8;
    let mut changes = 0_u32;
    let mut amplitude = 0.0_f32;
    for point in &series {
        let residual = point[1] - (intercept + slope * point[0]);
        amplitude = max(amplitude, residual.abs());
        let now = if residual > 0.0 { 1 } else { -1 };
        if sign != 0 && now != sign {
            changes += 1;
        }
        sign = now;
    }
    let mut dy_sign = 0_i8;
    let mut bends = 0_u32;
    for pair in series.windows(2) {
        let dy = pair[1][1] - pair[0][1];
        if dy.abs() < 0.2 {
            continue;
        }
        let now = if dy > 0.0 { 1 } else { -1 };
        if dy_sign != 0 && now != dy_sign {
            bends += 1;
        }
        dy_sign = now;
    }
    // A sine turns at every crest and trough; an arrow's shaft is flat and its barb bends once.
    if changes < 2 || bends < 2 || amplitude < 2.0 || amplitude > 0.30 * w {
        return None;
    }
    Some(min(
        0.55 + 0.05 * f32::from(u8::try_from(changes.min(6)).unwrap_or(6)),
        0.85,
    ))
}

/// A near-horizontal straight line: strike through text or underline below it.
pub(super) fn horizontal_line(p: &Poly, element_rect: Option<[f32; 4]>) -> Option<MarkShape> {
    let w = p.bbox.width();
    if w < 24.0 || p.bbox.height() > max(0.12 * w, 8.0) || p.straightness() < 0.90 {
        return None;
    }
    let middle = (p.bbox.min_y + p.bbox.max_y) * 0.5;
    let (guess, confidence) = match element_rect {
        Some([_, top, _, height]) if height > 1.0 => {
            let rel = (middle - top) / height;
            if rel > 0.90 {
                (MarkKind::Underline, 0.65)
            } else if rel >= 0.15 {
                (MarkKind::Strike, 0.70)
            } else {
                (MarkKind::Strike, 0.50)
            }
        }
        _ => (MarkKind::Strike, 0.45),
    };
    Some(MarkShape::guessed(guess, confidence))
}

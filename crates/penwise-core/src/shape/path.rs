//! Shape path construction and Bézier flattening.
//!
//! Owner: slice d.
//!
//! Every builder appends to a [`ShapePath`] in page px; all take the box `size = [w, h]` in local px and the
//! item transform. Determinism: only `+ − × ÷ sqrt`; flattening subdivides by a fixed rule (below), never by
//! floating step counts that could differ across targets.

use super::{FLATTEN_TOLERANCE, ShapePath};
use crate::edit::proximity::point_segment_dist2;
use crate::float::{max, min};
use crate::item::{Affine, ArrowHead};

/// Bézier circle constant `4/3·(√2 − 1)`.
pub const KAPPA: f32 = 0.552_284_8;

/// `cos 28°`, `sin 28°`: arrowhead barb rotation (no trig at build time).
pub const ARROW_COS: f32 = 0.882_947_6;
/// See [`ARROW_COS`].
pub const ARROW_SIN: f32 = 0.469_471_6;

/// Subdivision depth cap of [`flatten_cubic`] (at most 2¹⁰ segments per cubic).
const MAX_FLATTEN_DEPTH: u8 = 10;

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
    let tol2 = FLATTEN_TOLERANCE * FLATTEN_TOLERANCE;
    // Depth-first, first half on top, so segments are emitted in curve order.
    let mut stack: Vec<([[f32; 2]; 4], u8)> = vec![([p0, p1, p2, p3], 0)];
    while let Some((c, depth)) = stack.pop() {
        let flat = max(
            point_segment_dist2(c[1], c[0], c[3]).0,
            point_segment_dist2(c[2], c[0], c[3]).0,
        ) <= tol2;
        // NaN flatness (non-finite input) also stops here instead of subdividing forever.
        if flat || depth >= MAX_FLATTEN_DEPTH || !c.iter().flatten().all(|v| v.is_finite()) {
            out.push(c[3]);
            continue;
        }
        let (first, second) = split_half(&c);
        stack.push((second, depth + 1));
        stack.push((first, depth + 1));
    }
}

/// De Casteljau split at `t = 0.5`.
fn split_half(c: &[[f32; 2]; 4]) -> ([[f32; 2]; 4], [[f32; 2]; 4]) {
    let m01 = mid(c[0], c[1]);
    let m12 = mid(c[1], c[2]);
    let m23 = mid(c[2], c[3]);
    let a = mid(m01, m12);
    let b = mid(m12, m23);
    let m = mid(a, b);
    ([c[0], m01, a, m], [m, b, m23, c[3]])
}

fn mid(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [(a[0] + b[0]) * 0.5, (a[1] + b[1]) * 0.5]
}

/// Point `a + (b − a)·t`.
fn towards(a: [f32; 2], b: [f32; 2], t: f32) -> [f32; 2] {
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t]
}

/// Closes the current run (`points[last run end ..]`) and records whether runs are closed.
fn end_run(out: &mut ShapePath, closed: bool) {
    out.run_ends
        .push(u32::try_from(out.points.len()).unwrap_or(u32::MAX));
    out.closed = closed;
}

/// Local box corners `TL, TR, BR, BL` mapped to page space.
fn mapped_corners(size: [f32; 2], transform: &Affine) -> [[f32; 2]; 4] {
    let (hx, hy) = (size[0] * 0.5, size[1] * 0.5);
    [[-hx, -hy], [hx, -hy], [hx, hy], [-hx, hy]].map(|p| transform.apply(p))
}

/// Closed rectangle: corners `(−w/2, −h/2)`, `(w/2, −h/2)`, `(w/2, h/2)`, `(−w/2, h/2)` mapped.
pub fn rect(size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    out.points.extend(mapped_corners(size, transform));
    end_run(out, true);
}

/// Closed rounded rectangle, `radius` clamped to `[0, min(w, h) / 2]` (local px). For similarity transforms
/// the corners are quarter-arc Béziers built in page space from the mapped corners (round under rotation);
/// otherwise the local construction (4 lines + 4 quarter Béziers) is mapped (corners become elliptical under
/// shear, as the user dragged). Radius 0 → [`rect`].
///
/// Both cases share one construction on the mapped corners: each arc starts and ends at the fraction
/// `radius / edge length` along its two edges and its control points sit at [`KAPPA`] of the way to the
/// corner. Affine maps preserve those ratios, so this is exactly the mapped local construction, and under a
/// similarity the page radius is `radius · scale`, i.e. the page-space quarter arc.
pub fn rounded_rect(size: [f32; 2], radius: f32, transform: &Affine, out: &mut ShapePath) {
    let r = min(max(radius, 0.0), min(size[0], size[1]) * 0.5);
    if r.is_nan() || r <= 0.0 {
        rect(size, transform, out);
        return;
    }
    let corners = mapped_corners(size, transform);
    let (along_w, along_h) = (r / size[0], r / size[1]);
    let start = out.points.len();
    for i in 0..4 {
        let corner = corners[i];
        let prev = corners[(i + 3) % 4];
        let next = corners[(i + 1) % 4];
        // Corners TL, TR, BR, BL: the edge to `next` is horizontal (length w) for even i.
        let (t_prev, t_next) = if i % 2 == 0 {
            (along_h, along_w)
        } else {
            (along_w, along_h)
        };
        let a = towards(corner, prev, t_prev);
        let b = towards(corner, next, t_next);
        if out.points.len() == start || !out.points.last().is_some_and(|&p| same_point(p, a)) {
            out.points.push(a);
        }
        flatten_cubic(
            a,
            towards(a, corner, KAPPA),
            towards(b, corner, KAPPA),
            b,
            &mut out.points,
        );
    }
    drop_closing_duplicate(out, start);
    end_run(out, true);
}

/// Removes the last point of the current run when it repeats the run's first point (closed runs are
/// implicit).
fn drop_closing_duplicate(out: &mut ShapePath, start: usize) {
    if out.points.len() > start + 1
        && let (Some(&last), Some(&first)) = (out.points.last(), out.points.get(start))
        && same_point(last, first)
    {
        out.points.pop();
    }
}

/// Bitwise point equality (dedupe of exactly repeated construction points).
fn same_point(a: [f32; 2], b: [f32; 2]) -> bool {
    a[0].to_bits() == b[0].to_bits() && a[1].to_bits() == b[1].to_bits()
}

/// Closed ellipse inscribed in the box: 4 cubic Béziers with [`KAPPA`], control points mapped through the
/// transform (exact), then [`flatten_cubic`]. Starts at the local right-most point, clockwise on screen.
pub fn ellipse(size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    let (rx, ry) = (size[0] * 0.5, size[1] * 0.5);
    let (kx, ky) = (rx * KAPPA, ry * KAPPA);
    // Quadrants right → bottom → left → top (clockwise with y down), each as (p1, p2, p3).
    let quadrants = [
        [[rx, ky], [kx, ry], [0.0, ry]],
        [[-kx, ry], [-rx, ky], [-rx, 0.0]],
        [[-rx, -ky], [-kx, -ry], [0.0, -ry]],
        [[kx, -ry], [rx, -ky], [rx, 0.0]],
    ];
    let start = out.points.len();
    let mut p0 = transform.apply([rx, 0.0]);
    out.points.push(p0);
    for q in quadrants {
        let [p1, p2, p3] = q.map(|p| transform.apply(p));
        flatten_cubic(p0, p1, p2, p3, &mut out.points);
        p0 = p3;
    }
    drop_closing_duplicate(out, start);
    end_run(out, true);
}

/// Open segment `(−w/2, 0) → (w/2, 0)` mapped.
pub fn line(size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    let hx = size[0] * 0.5;
    out.points.push(transform.apply([-hx, 0.0]));
    out.points.push(transform.apply([hx, 0.0]));
    end_run(out, false);
}

/// [`line`] plus heads. Head length `clamp(4 · stroke_width, 8, 0.4 · shaft_length)` page px measured on the
/// mapped shaft; barbs = the unit shaft direction rotated by ±28° ([`ARROW_COS`], [`ARROW_SIN`]) scaled by
/// the head length, from the tip backwards. A zero-length shaft gets no heads.
///
/// The upper bound wins when the shaft is shorter than 20 px (`0.4 · length < 8`), so heads never exceed
/// 40 % of the shaft. Heads are emitted end head first, then (for [`ArrowHead::Both`]) the start head.
pub fn arrow(
    size: [f32; 2],
    head: ArrowHead,
    stroke_width: f32,
    transform: &Affine,
    out: &mut ShapePath,
) {
    line(size, transform, out);
    let &[.., start, end] = out.points.as_slice() else {
        return;
    };
    let (dx, dy) = (end[0] - start[0], end[1] - start[1]);
    let length = (dx * dx + dy * dy).sqrt();
    if !(length > 0.0 && length.is_finite()) {
        return;
    }
    let head_length = min(max(4.0 * stroke_width, 8.0), 0.4 * length);
    let unit = [dx / length, dy / length];
    push_head(end, [-unit[0], -unit[1]], head_length, out);
    if head == ArrowHead::Both {
        push_head(start, unit, head_length, out);
    }
}

/// One open head run `left barb → tip → right barb`; `back` is the unit vector from the tip into the shaft.
fn push_head(tip: [f32; 2], back: [f32; 2], length: f32, out: &mut ShapePath) {
    let rotated = |s: f32| {
        [
            tip[0] + (back[0] * ARROW_COS - back[1] * s) * length,
            tip[1] + (back[0] * s + back[1] * ARROW_COS) * length,
        ]
    };
    out.points.push(rotated(-ARROW_SIN));
    out.points.push(tip);
    out.points.push(rotated(ARROW_SIN));
    end_run(out, false);
}

/// Closed polygon: unit-box `points` scaled by `size`, then mapped.
pub fn polygon(points: &[[f32; 2]], size: [f32; 2], transform: &Affine, out: &mut ShapePath) {
    out.points.extend(
        points
            .iter()
            .map(|p| transform.apply([p[0] * size[0], p[1] * size[1]])),
    );
    end_run(out, true);
}

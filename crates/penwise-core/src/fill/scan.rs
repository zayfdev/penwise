//! Binary scanline rasterizer: polygons (nonzero) and capsules onto a bit grid, cell-centre sampling.
//!
//! Owner: slice c (slice-internal). Independent of `raster/` (tiny-skia): only `+ − × ÷ sqrt` in `f64`, so
//! walls are bit-identical on every target.

use super::FILL_CELL;
use super::grid::{Window, clear_run, global, set_run};

/// One edge crossing a row's centre line.
#[derive(Debug, Clone, Copy)]
struct Crossing {
    row: u32,
    x: f64,
    /// `+1` for an edge going down (y increasing), `−1` going up.
    dir: i8,
}

/// Sets every cell whose centre is inside the nonzero union of `contours` (page px; each implicitly closed).
pub(crate) fn fill_contours(window: &Window, contours: &[&[[f32; 2]]], bits: &mut [u64]) {
    let (xs, ys) = axes(window);
    let mut crossings = Vec::new();
    for contour in contours {
        if contour.len() < 3 {
            continue;
        }
        let mut prev = global(contour[contour.len() - 1]);
        for &p in *contour {
            let cur = global(p);
            add_edge(prev, cur, ys, &mut crossings);
            prev = cur;
        }
    }
    crossings.sort_by(|a, b| {
        a.row
            .cmp(&b.row)
            .then(a.x.total_cmp(&b.x))
            .then(a.dir.cmp(&b.dir))
    });
    let mut winding = 0i32;
    let mut start = 0.0;
    let mut row = u32::MAX;
    for c in &crossings {
        if c.row != row {
            row = c.row;
            winding = 0;
        }
        let before = winding;
        winding += i32::from(c.dir);
        if before == 0 && winding != 0 {
            start = c.x;
        } else if before != 0 && winding == 0 {
            let (x0, x1) = (xs.centre_from(start), xs.centre_from(c.x));
            if x0 < x1 {
                set_run(bits, window.width, row, x0, x1);
            }
        }
    }
}

/// Records the rows whose centre line lies in the edge's half-open span `[y_lo, y_hi)`. Shared vertices
/// produce identical bounds for both edges, so every row sees an even crossing count.
#[allow(
    clippy::float_cmp,
    reason = "horizontal edges never cross a row centre: exact test"
)]
fn add_edge(a: [f64; 2], b: [f64; 2], rows: Axis, out: &mut Vec<Crossing>) {
    if !(a.iter().chain(&b).all(|v| v.is_finite())) || a[1] == b[1] {
        return;
    }
    let (dir, lo, hi) = if a[1] < b[1] { (1, a, b) } else { (-1, b, a) };
    let slope = (hi[0] - lo[0]) / (hi[1] - lo[1]);
    for row in rows.centre_from(lo[1])..rows.centre_from(hi[1]) {
        out.push(Crossing {
            row,
            x: lo[0] + (rows.centre(row) - lo[1]) * slope,
            dir,
        });
    }
}

/// One window axis on the global cell grid. All arithmetic is in global cell units (`p / FILL_CELL`), so a
/// cell's walls do not depend on where the window happens to start.
#[derive(Debug, Clone, Copy)]
struct Axis {
    /// Global index of the window's first cell (exact in `f64`: `|origin| ≤ 2²⁴ + 4096`).
    origin: f64,
    n: u32,
}

#[allow(
    clippy::cast_precision_loss,
    reason = "window origins are bounded by the 2^24 px tap limit, exact in f64"
)]
fn axes(window: &Window) -> (Axis, Axis) {
    let axis = |origin: i64, n: u32| Axis {
        origin: origin as f64,
        n,
    };
    (
        axis(window.origin[0], window.width),
        axis(window.origin[1], window.height),
    )
}

impl Axis {
    /// Global coordinate of the centre of window cell `i`.
    fn centre(self, i: u32) -> f64 {
        self.origin + f64::from(i) + 0.5
    }

    /// First window cell whose centre is `≥ v` (global units), clamped to `0..=n`.
    fn centre_from(self, v: f64) -> u32 {
        self.clamp((v - 0.5).ceil())
    }

    /// One past the last window cell whose centre is `≤ v`, clamped to `0..=n`.
    fn centre_through(self, v: f64) -> u32 {
        self.clamp((v - 0.5).floor() + 1.0)
    }

    /// Global integral cell index `g` as a window index clamped to `0..=n` (NaN → 0).
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "the local index is integral and checked to lie in 0..n"
    )]
    fn clamp(self, g: f64) -> u32 {
        let local = g - self.origin;
        if local.is_nan() || local <= 0.0 {
            0
        } else if local >= f64::from(self.n) {
            self.n
        } else {
            local as u32
        }
    }
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
    let r = if radius.is_nan() || radius < FILL_CELL * 0.5 {
        FILL_CELL * 0.5
    } else {
        radius
    };
    capsule_runs(window, [a, b], r, bits, true);
}

/// Clears every cell whose centre is within `radius` of segment `a`–`b` (erase masks punched out of walls).
pub(crate) fn clear_capsule(
    window: &Window,
    a: [f32; 2],
    b: [f32; 2],
    radius: f32,
    bits: &mut [u64],
) {
    if radius.is_nan() || radius <= 0.0 {
        return;
    }
    capsule_runs(window, [a, b], radius, bits, false);
}

/// Sets (`set`) or clears the cells whose centres lie in the closed capsule, one run per row.
fn capsule_runs(window: &Window, seg: [[f32; 2]; 2], radius: f32, bits: &mut [u64], set: bool) {
    let (xs, ys) = axes(window);
    let (a, b) = (global(seg[0]), global(seg[1]));
    let r = f64::from(radius) / f64::from(FILL_CELL);
    if !(a.iter().chain(&b).all(|v| v.is_finite()) && r.is_finite()) {
        return;
    }
    let y_lo = min64(a[1], b[1]) - r;
    let y_hi = max64(a[1], b[1]) + r;
    for row in ys.centre_from(y_lo)..ys.centre_through(y_hi) {
        let Some((xl, xr)) = capsule_row(a, b, r, ys.centre(row)) else {
            continue;
        };
        let (x0, x1) = (xs.centre_from(xl), xs.centre_through(xr));
        if x0 < x1 {
            if set {
                set_run(bits, window.width, row, x0, x1);
            } else {
                clear_run(bits, window.width, row, x0, x1);
            }
        }
    }
}

/// The closed x-interval of the capsule (`a`–`b`, radius `r`) on the line at height `y`: the hull of the two
/// end-disc chords and the band between them (the capsule is convex, so the slice is one interval).
fn capsule_row(a: [f64; 2], b: [f64; 2], r: f64, y: f64) -> Option<(f64, f64)> {
    let mut lo = f64::INFINITY;
    let mut hi = f64::NEG_INFINITY;
    for p in [a, b] {
        let dy = y - p[1];
        let s2 = r * r - dy * dy;
        if s2 >= 0.0 {
            let s = s2.sqrt();
            lo = min64(lo, p[0] - s);
            hi = max64(hi, p[0] + s);
        }
    }
    let d = [b[0] - a[0], b[1] - a[1]];
    let len2 = d[0] * d[0] + d[1] * d[1];
    if len2 > 0.0 {
        // With u = (x − a.x, y − a.y): |cross(d, u)| ≤ r·|d| and 0 ≤ dot(d, u) ≤ |d|², both linear in x.
        let dy = y - a[1];
        let reach = r * len2.sqrt();
        let band = linear_range(-d[1], d[0] * dy, -reach, reach).zip(linear_range(
            d[0],
            d[1] * dy,
            0.0,
            len2,
        ));
        if let Some(((c0, c1), (t0, t1))) = band {
            let (l, u) = (max64(c0, t0), min64(c1, t1));
            if l <= u {
                lo = min64(lo, a[0] + l);
                hi = max64(hi, a[0] + u);
            }
        }
    }
    (lo <= hi).then_some((lo, hi))
}

/// `{x : lo ≤ k·x + c ≤ hi}` as a closed interval (possibly unbounded), `None` when empty.
#[allow(
    clippy::float_cmp,
    reason = "an exactly zero slope term is the degenerate case"
)]
fn linear_range(k: f64, c: f64, lo: f64, hi: f64) -> Option<(f64, f64)> {
    if k == 0.0 {
        return (lo <= c && c <= hi).then_some((f64::NEG_INFINITY, f64::INFINITY));
    }
    let (p, q) = ((lo - c) / k, (hi - c) / k);
    Some(if k > 0.0 { (p, q) } else { (q, p) })
}

/// Smaller of two `f64`s (`a` on ties; std `min` is banned for its sign-of-zero ambiguity).
pub(crate) fn min64(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// Larger of two `f64`s (`a` on ties).
pub(crate) fn max64(a: f64, b: f64) -> f64 {
    if b > a { b } else { a }
}

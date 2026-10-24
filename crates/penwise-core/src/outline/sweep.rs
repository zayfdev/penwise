//! Flat-nib sweep (calligraphy, highlighter; ink-rendering.md §2.1).
//!
//! Owner: slice b.
//!
//! A flat nib is a rectangle; moving it from one centerline point to the next sweeps exactly the convex hull
//! of the two rectangles. The stroke is the union of those per-segment hulls. A left/right ribbon through two
//! nib corners cannot express this: whenever the motion crosses the nib axis the corners swap sides, the
//! ribbon twists into a bow-tie, and overlapping lobes of opposite winding cancel under the nonzero rule
//! (holes, spikes and shards).
//!
//! The hulls still travel through the single-contour `left ++ end_cap ++ reverse(right) ++ start_cap` layout:
//! segment `k` appends its hull loop `h₀ … h_{m−1} h₀` to `left` and its anchor `h₀` to `right`. The bridges
//! `h₀ᵏ → h₀ᵏ⁺¹` walked forward in `left` are walked back exactly by `reverse(right)`, so they cancel, and
//! every hull has the same (positive shoelace) orientation: the contour's winding number is the number of
//! hulls covering a point, never zero inside the sweep. Caps are empty: the end rectangles are the flat caps.
//!
//! Decimation ([`Sweep`]): input arrives at up to ~500 Hz, a fraction of a pixel apart, and one hull per sample
//! costs ~10 vertices. A run of samples is swept as the single hull of its first and last rectangle when every
//! centre in between stays within `tol` of that chord (`tol` < the inscribed radius of both end rectangles) and
//! every rectangle in between keeps its shape within [`MAX_SHAPE_DRIFT`]. The hull of `A ∪ B` contains every
//! interpolated rectangle `(1 − λ)·A + λ·B`, which contains the disc of radius `min(reach_A, reach_B)` around
//! the interpolated centre, so the decimated sweep still covers the whole centreline; on straight runs with a
//! constant nib it is exactly the undecimated sweep. Runs are also capped at `minDist` (`size·smoothing`), so
//! `smoothing = 0` disables decimation.
//!
//! Run state is O(1) (sleeve-fitting): the directions from the run's first centre that stay within `tol` of
//! every skipped centre form a cone, intersected point by point. A point that breaks the run makes the
//! previous (skipped) point the end of the run; that hull is appended one point late. Decisions depend only on
//! the points walked so far, so the stable prefix never changes; the last point is never skipped.
//!
//! Slice-internal: every item is `pub(super)` and may be reshaped by slice b.

use super::walker::Sides;
use crate::float::{max, min};

/// Largest distance of a skipped centre from its run's chord, px (visual tolerance).
const MAX_DEVIATION: f32 = 0.25;
/// `tol ≤ REACH_SHARE · reach` of both run ends. Below `1/√2`: a skipped centre may sit `tol` past the chord's
/// end as well as `tol` beside it, `√2·tol` from the segment.
const REACH_SHARE: f32 = 0.6;
/// Largest corner drift of a skipped rectangle relative to the run's first one (width or angle change), px.
const MAX_SHAPE_DRIFT: f32 = 0.5;

/// Nib rectangle corners ([`super::nib::nib_corners`]).
pub(super) type NibQuad = [[f32; 2]; 4];

/// One walked nib rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Nib {
    pub center: [f32; 2],
    pub quad: NibQuad,
    /// Inscribed radius: `min(width, thickness) / 2`.
    pub reach: f32,
}

/// Decimated flat-nib sweep state (see the module docs).
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct Sweep {
    run: Option<Run>,
}

/// Points since the last emitted rectangle.
#[derive(Debug, Clone, Copy, PartialEq)]
struct Run {
    /// Last emitted rectangle (the run's start).
    kept: Nib,
    /// Last skipped rectangle: the run's end if the next point breaks the run.
    pending: Option<Nib>,
    /// Chord tolerance, px.
    tol: f32,
    /// Largest distance of a skipped centre from `kept.center`.
    far: f32,
    /// `(lo, hi)`: admissible chord directions, counter-clockwise from `lo` to `hi` (narrower than π).
    cone: Option<([f32; 2], [f32; 2])>,
}

impl Sweep {
    #[must_use]
    pub(super) const fn new() -> Self {
        Self { run: None }
    }

    /// Walks `nib`; `last` (the stroke's last point) is never skipped. `min_dist_sq` caps a run's squared length.
    pub(super) fn push(&mut self, nib: Nib, last: bool, min_dist_sq: f32, out: &mut Sides<'_>) {
        let Some(run) = &mut self.run else {
            self.run = Some(Run::start(nib));
            return;
        };
        if run.admits(&nib, min_dist_sq) {
            if !last {
                run.skip(&nib);
                return;
            }
        } else if let Some(end) = run.pending {
            push_segment(&run.kept.quad, &end.quad, out);
            *run = Run::start(end);
            if !last && run.admits(&nib, min_dist_sq) {
                run.skip(&nib);
                return;
            }
        }
        push_segment(&run.kept.quad, &nib.quad, out);
        *run = Run::start(nib);
    }
}

impl Run {
    fn start(kept: Nib) -> Self {
        Self {
            kept,
            pending: None,
            tol: min(MAX_DEVIATION, REACH_SHARE * kept.reach),
            far: 0.0,
            cone: None,
        }
    }

    /// Whether `nib` can end the run: within `minDist`, not closer than the farthest skipped centre (less
    /// `tol`), inside the cone, wide enough for `tol`, and of the run's shape.
    fn admits(&self, nib: &Nib, min_dist_sq: f32) -> bool {
        let v = sub(nib.center, self.kept.center);
        let r_sq = dot(v, v);
        if r_sq > min_dist_sq || r_sq.sqrt() + self.tol < self.far {
            return false;
        }
        if REACH_SHARE * nib.reach < self.tol || !self.same_shape(nib) {
            return false;
        }
        self.cone
            .is_none_or(|(lo, hi)| perp_dot(lo, v) >= 0.0 && perp_dot(v, hi) >= 0.0)
    }

    /// Every corner of `nib` within [`MAX_SHAPE_DRIFT`] of the matching corner of `kept`, relative to centres.
    fn same_shape(&self, nib: &Nib) -> bool {
        let limit = MAX_SHAPE_DRIFT * MAX_SHAPE_DRIFT;
        nib.quad.iter().zip(&self.kept.quad).all(|(a, b)| {
            let drift = sub(sub(*a, nib.center), sub(*b, self.kept.center));
            dot(drift, drift) <= limit
        })
    }

    /// Skips admitted `nib`: later chords must pass within `tol` of its centre.
    fn skip(&mut self, nib: &Nib) {
        self.pending = Some(*nib);
        let v = sub(nib.center, self.kept.center);
        let r = dot(v, v).sqrt();
        self.far = max(self.far, r);
        if r <= self.tol {
            // Within `tol` of the run's start: every chord passes close enough.
            return;
        }
        // Directions within `asin(tol / r)` of `v`.
        let sin = self.tol / r;
        let cos = (1.0 - sin * sin).sqrt();
        let lo = [v[0] * cos + v[1] * sin, v[1] * cos - v[0] * sin];
        let hi = [v[0] * cos - v[1] * sin, v[1] * cos + v[0] * sin];
        self.cone = Some(match self.cone {
            None => (lo, hi),
            Some((lo0, hi0)) => (
                if perp_dot(lo0, lo) > 0.0 { lo } else { lo0 },
                if perp_dot(hi, hi0) > 0.0 { hi } else { hi0 },
            ),
        });
    }
}

/// Appends the hull loop of the segment sweeping nib `a` to nib `b` (see the module docs). Degenerate hulls
/// (fewer than 3 vertices: zero area) append nothing.
pub(super) fn push_segment(a: &NibQuad, b: &NibQuad, out: &mut Sides<'_>) {
    let mut pts = [[0.0_f32; 2]; 8];
    pts[..4].copy_from_slice(a);
    pts[4..].copy_from_slice(b);
    let mut hull = [[0.0_f32; 2]; 16];
    let m = convex_hull(&mut pts, &mut hull);
    if m < 3 {
        return;
    }
    let anchor = hull[0];
    out.left.extend_from_slice(&hull[..m]);
    out.left.push(anchor);
    out.right.push(anchor);
}

/// Andrew's monotone chain: sorts `pts` (x then y, total order) and writes their convex hull to `hull` with
/// positive shoelace area (counter-clockwise in x-right/y-up axes), dropping duplicate and collinear points.
/// Returns the vertex count.
pub(super) fn convex_hull(pts: &mut [[f32; 2]; 8], hull: &mut [[f32; 2]; 16]) -> usize {
    pts.sort_unstable_by(|a, b| a[0].total_cmp(&b[0]).then(a[1].total_cmp(&b[1])));
    let mut k = 0;
    for &p in pts.iter() {
        while k >= 2 && cross(hull[k - 2], hull[k - 1], p) <= 0.0 {
            k -= 1;
        }
        hull[k] = p;
        k += 1;
    }
    let lower = k + 1;
    for &p in pts.iter().rev().skip(1) {
        while k >= lower && cross(hull[k - 2], hull[k - 1], p) <= 0.0 {
            k -= 1;
        }
        hull[k] = p;
        k += 1;
    }
    // The chain ends where it started.
    k - 1
}

/// `(a − o) × (b − o)`.
fn cross(o: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
    (a[0] - o[0]) * (b[1] - o[1]) - (a[1] - o[1]) * (b[0] - o[0])
}

/// `a × b`: positive when `b` is counter-clockwise of `a`.
fn perp_dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

fn sub(a: [f32; 2], b: [f32; 2]) -> [f32; 2] {
    [a[0] - b[0], a[1] - b[1]]
}

#[cfg(test)]
mod tests;

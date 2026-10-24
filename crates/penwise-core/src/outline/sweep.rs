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
//! Slice-internal: every item is `pub(super)` and may be reshaped by slice b.

use super::walker::Sides;

/// Nib rectangle corners ([`super::nib::nib_corners`]).
pub(super) type NibQuad = [[f32; 2]; 4];

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

#[cfg(test)]
mod tests;

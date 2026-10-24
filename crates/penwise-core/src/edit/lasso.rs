//! Lasso selection (editing-algorithms.md §4).
//!
//! Owner: slice a.

use super::hit::{page_masks, runs};
use super::proximity::{arc_length, point_polyline_dist, resample, ring_contains};
use crate::bounds::Bounds;
use crate::float::max;
use crate::item::{ItemGeometry, ItemId, LayerSet, Scene};

/// Lasso tuning (constants re-tuned at the M4 feel check).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct LassoParams {
    /// Fraction of an item's resampled points that must lie inside (`0..=1`).
    pub threshold: f32,
    /// Arc-length resample step, page px.
    pub step: f32,
    /// Items with total arc length below this (page px) are selected iff their centroid is inside.
    pub tiny_length: f32,
}

impl LassoParams {
    /// Research defaults: 75%, 2 px step, 4 px tiny length.
    pub const DEFAULT: Self = Self {
        threshold: 0.75,
        step: 2.0,
        tiny_length: 4.0,
    };
}

/// Items selected by the lasso `polygon` (page px; auto-closed back to its first point; nonzero rule via
/// [`crate::edit::proximity::ring_contains`]), sorted by `(z, id)` ascending.
///
/// 1. Prefilter: [`Scene::query`] on the lasso's bounds, `layers` only; erase items never selected.
/// 2. Per candidate, sample points:
///    - stroke: each visible centreline run resampled every `step` (arc length, so slow pen segments do not
///      bias the count), dropping points inside the item's masks;
///    - shape: flattened page path resampled, plus every path vertex, dropping masked points;
///    - fill: outer rings resampled plus their vertices;
///    - text: the 4 corners and the centre.
/// 3. Select when `inside / total ≥ threshold`. Items whose total arc length is `< tiny_length` (dots) are
///    selected iff the mean of their samples is inside. An item with zero unmasked samples is never selected.
///
/// Fewer than 3 lasso points → empty.
#[must_use]
pub fn lasso(
    scene: &Scene,
    polygon: &[[f32; 2]],
    layers: LayerSet,
    params: &LassoParams,
) -> Vec<ItemId> {
    if polygon.len() < 3 {
        return Vec::new();
    }
    let area = polygon.iter().fold(Bounds::EMPTY, |mut b, p| {
        b.include_point(p[0], p[1]);
        b
    });
    let mut points = Vec::new();
    let mut scratch = Vec::new();
    scene
        .query(&area, layers)
        .into_iter()
        .filter(|&id| {
            points.clear();
            let length = item_samples(scene, id, params.step, &mut points, &mut scratch);
            selected(polygon, &points, length, params)
        })
        .collect()
}

/// Writes the unmasked sample points of a live item into `out`; returns the item's total arc length.
fn item_samples(
    scene: &Scene,
    id: ItemId,
    step: f32,
    out: &mut Vec<[f32; 2]>,
    scratch: &mut Vec<[f32; 2]>,
) -> f32 {
    let mut length = 0.0f32;
    match scene.geometry(id) {
        Some(ItemGeometry::Stroke { centreline, .. }) => {
            for (start, end) in runs(&centreline.run_ends, centreline.points.len()) {
                scratch.clear();
                scratch.extend(centreline.points[start..end].iter().map(|c| [c.x, c.y]));
                length += arc_length(scratch, false);
                resample(scratch, false, step, out);
            }
            drop_masked(scene, id, out);
        }
        Some(ItemGeometry::Shape(shape)) => {
            let path = &shape.path;
            for (start, end) in runs(&path.run_ends, path.points.len()) {
                length += push_ring_or_line(&path.points[start..end], path.closed, step, out);
            }
            drop_masked(scene, id, out);
        }
        Some(ItemGeometry::Fill { polygons, .. }) => {
            for poly in polygons {
                length += push_ring_or_line(&poly.outer, true, step, out);
            }
        }
        Some(ItemGeometry::Text { corners, .. }) => {
            length = arc_length(corners, true);
            out.extend_from_slice(corners);
            let mid = |i: usize| f32::midpoint(corners[0][i], corners[2][i]);
            out.push([mid(0), mid(1)]);
        }
        Some(ItemGeometry::Erase { .. }) | None => {}
    }
    length
}

/// Removes the points of `out` inside `id`'s masks. Each mask is culled by its bounds (path box inflated by
/// the radius plus a rounding pad) before the exact distance test, so points far from every mask cost O(K)
/// box checks instead of O(K·path) distances.
fn drop_masked(scene: &Scene, id: ItemId, out: &mut Vec<[f32; 2]>) {
    let masks: Vec<_> = page_masks(scene, id)
        .into_iter()
        .map(|(path, radius)| (mask_bounds(path, radius), path, radius))
        .collect();
    if masks.is_empty() {
        return;
    }
    let all = masks
        .iter()
        .fold(Bounds::EMPTY, |acc, (b, _, _)| acc.union(b));
    out.retain(|&p| {
        !(all.contains(p[0], p[1])
            && masks.iter().any(|(b, path, radius)| {
                b.contains(p[0], p[1]) && point_polyline_dist(p, path) <= *radius
            }))
    });
}

/// Conservative box of the points within `radius` of `path`: the path box inflated by `radius` plus a pad
/// covering `f32` rounding in [`point_polyline_dist`], so the cull never rejects a point the exact test
/// would accept.
fn mask_bounds(path: &[[f32; 2]], radius: f32) -> Bounds {
    let b = path.iter().fold(Bounds::EMPTY, |mut b, p| {
        b.include_point(p[0], p[1]);
        b
    });
    let magnitude = [b.min_x, b.min_y, b.max_x, b.max_y, radius]
        .iter()
        .fold(0.0f32, |m, v| max(m, v.abs()));
    b.inflate(radius + 1.0e-3 * (1.0 + magnitude))
}

/// Resampled points of `run` plus the vertices `resample` did not already emit (it emits the first vertex,
/// and the last one of open runs). Returns the run's arc length.
fn push_ring_or_line(run: &[[f32; 2]], closed: bool, step: f32, out: &mut Vec<[f32; 2]>) -> f32 {
    resample(run, closed, step, out);
    let interior_end = if closed {
        run.len()
    } else {
        run.len().saturating_sub(1)
    };
    if interior_end > 1 {
        out.extend_from_slice(&run[1..interior_end]);
    }
    arc_length(run, closed)
}

/// The selection rule of [`lasso`] step 3.
fn selected(polygon: &[[f32; 2]], points: &[[f32; 2]], length: f32, params: &LassoParams) -> bool {
    if points.is_empty() {
        return false;
    }
    if length < params.tiny_length {
        let (sx, sy) = points.iter().fold((0.0f64, 0.0f64), |(sx, sy), p| {
            (sx + f64::from(p[0]), sy + f64::from(p[1]))
        });
        let n = f64::from(u32::try_from(points.len()).unwrap_or(u32::MAX));
        #[allow(
            clippy::cast_possible_truncation,
            reason = "mean of f32 points, one rounding"
        )]
        let centroid = [(sx / n) as f32, (sy / n) as f32];
        return ring_contains(polygon, centroid);
    }
    let inside = points
        .iter()
        .filter(|&&p| ring_contains(polygon, p))
        .count();
    let inside = f64::from(u32::try_from(inside).unwrap_or(u32::MAX));
    let total = f64::from(u32::try_from(points.len()).unwrap_or(u32::MAX));
    inside >= f64::from(params.threshold) * total
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cull box never rejects a point the exact mask test accepts (grid around a diagonal mask,
    /// including points exactly at the radius); a NaN radius masks nothing either way.
    #[test]
    #[allow(clippy::cast_precision_loss, reason = "small grid indices")]
    fn mask_bounds_are_conservative() {
        let path = [[10.0, 10.0], [40.0, 25.0], [40.0, 25.0]];
        let radius = 6.0;
        let b = mask_bounds(&path, radius);
        let mut inside = 0;
        for i in 0..=240 {
            for j in 0..=240 {
                let p = [i as f32 * 0.25, j as f32 * 0.25];
                if point_polyline_dist(p, &path) <= radius {
                    inside += 1;
                    assert!(b.contains(p[0], p[1]), "{p:?} masked but culled");
                }
            }
        }
        assert!(inside > 0);
        assert!(!b.contains(100.0, 100.0));
        assert!(!mask_bounds(&path, f32::NAN).contains(20.0, 20.0));
    }
}

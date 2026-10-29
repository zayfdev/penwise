//! Tap hit testing (editing-algorithms.md §4).
//!
//! Owner: slice a.
//!
//! `tol` is in page units: the caller converts its screen-space tolerance (e.g. 8 dp) with the current zoom.
//! Core does no I/O and knows no zoom.

use super::proximity::{
    point_polyline_dist, point_ring_dist, point_segment_dist2, polygons_contain, ring_contains,
};
use crate::bounds::Bounds;
use crate::float::lerp;
use crate::geometry::Outline;
use crate::item::{Centreline, ItemGeometry, ItemId, ItemKind, LayerSet, Scene};
use crate::shape::ShapePath;

/// Every live, non-erase item on `layers` hit at `p` with tolerance `tol ≥ 0`, sorted by `(z, id)` ascending.
/// Candidates come from [`Scene::query`] on `p` inflated by `tol` plus the item's max half-width; then
/// [`item_hit`] decides. Empty when nothing is hit.
///
/// The cached item bounds already include the ink's half-width, so the query box is `p` inflated by `tol`.
/// A negative or NaN `tol` counts as `0`.
#[must_use]
pub fn hit_test(scene: &Scene, p: [f32; 2], tol: f32, layers: LayerSet) -> Vec<ItemId> {
    let tol = sanitize(tol);
    let area = Bounds::from_point(p[0], p[1]).inflate(tol);
    scene
        .query(&area, layers)
        .into_iter()
        .filter(|&id| item_hit(scene, id, p, tol))
        .collect()
}

/// Exact test for one live item (false for unknown, deleted or erase items):
/// - stroke: some centreline run segment `i` with `dist(p, seg_i) ≤ lerp(h_i, h_{i+1}, t) + tol` (tapered
///   capsule; a 1-point run is a disc of its half-width);
/// - shape: `dist(p, flattened page path) ≤ brush_size_max / 2 + tol`, or inside its fill polygon when
///   filled (nonzero);
/// - fill: nonzero point-in-polygon over its page polygons (holes excluded), or within `tol` of a ring;
/// - text, image: inside the box quad or within `tol` of it.
///
/// Masks: for strokes and shapes, a point is not a hit when it lies inside any of the item's masks
/// (`dist(p, mask.path) ≤ mask.radius`, page space), editing-algorithms.md §2.
#[must_use]
pub fn item_hit(scene: &Scene, id: ItemId, p: [f32; 2], tol: f32) -> bool {
    let tol = sanitize(tol);
    let (Some(item), Some(geometry)) = (scene.get(id), scene.geometry(id)) else {
        return false;
    };
    match (&item.kind, geometry) {
        (ItemKind::Stroke(_), ItemGeometry::Stroke { centreline, .. }) => {
            centreline_hit(centreline, p, tol) && !masked(scene, id, p)
        }
        (ItemKind::Shape(shape), ItemGeometry::Shape(g)) => {
            let reach = shape.brush.size_max / 2.0 + tol;
            let on_path = path_dist(&g.path, p) <= reach;
            let inside = g
                .fill
                .as_ref()
                .is_some_and(|(_, outline)| outline_contains(outline, p));
            (on_path || inside) && !masked(scene, id, p)
        }
        (ItemKind::Fill(_), ItemGeometry::Fill { polygons, .. }) => {
            polygons_contain(polygons, p)
                || polygons.iter().any(|poly| {
                    point_ring_dist(p, &poly.outer) <= tol
                        || poly.holes.iter().any(|h| point_ring_dist(p, h) <= tol)
                })
        }
        (ItemKind::Text(_), ItemGeometry::Text { corners, .. })
        | (ItemKind::Image(_), ItemGeometry::Image { corners })
        | (ItemKind::Frame(_), ItemGeometry::Frame { corners }) => {
            ring_contains(corners, p) || point_ring_dist(p, corners) <= tol
        }
        _ => false,
    }
}

/// The visually topmost of `hits` (largest [`crate::item::RenderKey`], i.e. band then `(z, id)`), or `None`
/// for an empty slice. Tap-select uses `topmost(scene, &hit_test(..))`.
#[must_use]
pub fn topmost(scene: &Scene, hits: &[ItemId]) -> Option<ItemId> {
    hits.iter()
        .filter_map(|&id| scene.render_key(id))
        .max()
        .map(|k| k.id)
}

/// True when `p` lies inside one of `target`'s live masks (page space).
pub(crate) fn masked(scene: &Scene, target: ItemId, p: [f32; 2]) -> bool {
    in_masks(&page_masks(scene, target), p)
}

/// Page-space `(path, radius)` of `target`'s live, non-empty masks, ascending mask id.
pub(crate) fn page_masks(scene: &Scene, target: ItemId) -> Vec<(&[[f32; 2]], f32)> {
    scene
        .masks(target)
        .into_iter()
        .filter_map(|m| match scene.geometry(m) {
            Some(ItemGeometry::Erase { path, radius }) if !path.is_empty() => {
                Some((path.as_slice(), *radius))
            }
            _ => None,
        })
        .collect()
}

/// True when `p` is within some mask's radius of its path.
pub(crate) fn in_masks(masks: &[(&[[f32; 2]], f32)], p: [f32; 2]) -> bool {
    masks
        .iter()
        .any(|(path, radius)| point_polyline_dist(p, path) <= *radius)
}

fn sanitize(tol: f32) -> f32 {
    if tol > 0.0 { tol } else { 0.0 }
}

/// Tapered-capsule test over every run of `line`.
fn centreline_hit(line: &Centreline, p: [f32; 2], tol: f32) -> bool {
    runs(&line.run_ends, line.points.len()).any(|(start, end)| {
        let run = &line.points[start..end];
        match run {
            [] => false,
            [dot] => {
                point_segment_dist2(p, [dot.x, dot.y], [dot.x, dot.y])
                    .0
                    .sqrt()
                    <= dot.half_width + tol
            }
            _ => run.windows(2).any(|w| {
                let (d2, t) = point_segment_dist2(p, [w[0].x, w[0].y], [w[1].x, w[1].y]);
                d2.sqrt() <= lerp(w[0].half_width, w[1].half_width, t) + tol
            }),
        }
    })
}

/// Distance from `p` to a flattened shape path (closed runs include their closing edge).
pub(crate) fn path_dist(path: &ShapePath, p: [f32; 2]) -> f32 {
    runs(&path.run_ends, path.points.len()).fold(f32::INFINITY, |best, (start, end)| {
        let run = &path.points[start..end];
        let d = if path.closed {
            point_ring_dist(p, run)
        } else {
            point_polyline_dist(p, run)
        };
        crate::float::min(best, d)
    })
}

/// Nonzero containment over every contour of an outline.
fn outline_contains(outline: &Outline, p: [f32; 2]) -> bool {
    let total: i32 = runs(&outline.contour_ends, outline.points.len())
        .map(|(start, end)| super::proximity::winding_number(p, &outline.points[start..end]))
        .sum();
    total != 0
}

/// `(start, end)` index pairs of the runs described by exclusive `ends`, clamped to `len` so malformed
/// run tables never index out of bounds.
pub(crate) fn runs(ends: &[u32], len: usize) -> impl Iterator<Item = (usize, usize)> + '_ {
    let mut start = 0usize;
    ends.iter().map(move |&end| {
        let end = usize::try_from(end).map_or(len, |e| e.min(len));
        let run = (start.min(end), end);
        start = end;
        run
    })
}

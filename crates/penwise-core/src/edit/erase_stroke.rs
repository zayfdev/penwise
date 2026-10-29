//! Stroke eraser: removes whole items the eraser path touches (PLAN "Eraser").
//!
//! Owner: slice b.

mod index;
#[cfg(test)]
mod tests;

use super::Edit;
use super::proximity::{
    point_polyline_dist, point_segment_dist2, ring_contains, segment_segment_dist,
};
use crate::bounds::Bounds;
use crate::float::max;
use crate::item::{CentrePoint, Centreline, ItemGeometry, ItemId, ItemKind, LayerSet, Scene};
use crate::shape::ShapeGeometry;
pub(crate) use index::{EraserIndex, query_box};

/// `Delete` for every live item on `layers` the eraser touches, sorted by `(z, id)`. The eraser is the capsule
/// chain of radius `radius > 0` along `path` (page px; one point = a disc). Touch rules:
/// - stroke: some eraser segment and some visible centreline segment satisfy
///   `segment_segment_dist ≤ radius + h` (`h` = the larger half-width of the two centreline ends);
///   points inside the stroke's masks do not count;
/// - shape: eraser within `radius + brush_size_max / 2` of the flattened path (masks respected), or any eraser
///   point inside its fill;
/// - fill: eraser within `radius` of a polygon **ring** only (scribbling over a fill's interior to erase ink
///   on top must not delete the fill);
/// - text, image: eraser within `radius` of the box quad or inside it;
/// - erase items are never deleted directly (they are inert while their target is dead).
///
/// May be called per path chunk during a gesture (overlap the previous chunk by one point); apply the edits
/// between calls, and group the whole gesture as one undo step. Empty path or `radius ≤ 0` → empty.
#[must_use]
pub fn erase_strokes(scene: &Scene, path: &[[f32; 2]], radius: f32, layers: LayerSet) -> Vec<Edit> {
    let Some(area) = eraser_bounds(path, radius) else {
        return Vec::new();
    };
    let index = EraserIndex::new(path);
    scene
        .query(&area, layers)
        .into_iter()
        .filter(|&id| path_touches_in(scene, id, path, radius, &index))
        .map(Edit::Delete)
        .collect()
}

/// The touch rule above for one live item (false for unknown, dead or erase items).
#[must_use]
pub fn path_touches(scene: &Scene, id: ItemId, path: &[[f32; 2]], radius: f32) -> bool {
    if eraser_bounds(path, radius).is_none() {
        return false;
    }
    path_touches_in(scene, id, path, radius, &EraserIndex::new(path))
}

/// [`path_touches`] with a prebuilt index of `path` (`path` already validated by [`eraser_bounds`]).
pub(crate) fn path_touches_in(
    scene: &Scene,
    id: ItemId,
    path: &[[f32; 2]],
    radius: f32,
    index: &EraserIndex,
) -> bool {
    let (Some(item), Some(geometry)) = (scene.get(id), scene.geometry(id)) else {
        return false;
    };
    let masks = page_masks(scene, id);
    match (&item.kind, geometry) {
        (ItemKind::Stroke(_), ItemGeometry::Stroke { centreline, .. }) => {
            stroke_touches(centreline, index, radius, &masks)
        }
        (ItemKind::Shape(shape), ItemGeometry::Shape(g)) => {
            shape_touches(g, radius + shape.brush.size_max * 0.5, path, index, &masks)
        }
        (ItemKind::Fill(_), ItemGeometry::Fill { polygons, .. }) => polygons.iter().any(|poly| {
            std::iter::once(&poly.outer)
                .chain(&poly.holes)
                .any(|ring| line_touches(ring, true, index, radius, &[]))
        }),
        (ItemKind::Text(_), ItemGeometry::Text { corners, .. })
        | (ItemKind::Image(_), ItemGeometry::Image { corners }) => {
            line_touches(corners, true, index, radius, &[])
                || path.iter().any(|&p| ring_contains(corners, p))
        }
        // Frames are live pages under the ink (erasing over one erases the ink on it, never the page) and
        // marks are groups of strokes.
        _ => false,
    }
}

/// A page-space mask: `(path, radius)`.
pub(crate) type PageMask<'a> = (&'a [[f32; 2]], f32);

/// Page bounds of the eraser capsule chain; `None` for an empty path, a non-finite point or an invalid radius.
pub(crate) fn eraser_bounds(path: &[[f32; 2]], radius: f32) -> Option<Bounds> {
    if path.is_empty() || !radius.is_finite() || radius <= 0.0 {
        return None;
    }
    let mut b = Bounds::EMPTY;
    for &[x, y] in path {
        if !x.is_finite() || !y.is_finite() {
            return None;
        }
        b.include_point(x, y);
    }
    Some(b.inflate(radius))
}

/// Live masks of `target` in page space (cached erase geometry), ascending id.
pub(crate) fn page_masks(scene: &Scene, target: ItemId) -> Vec<PageMask<'_>> {
    scene
        .masks(target)
        .into_iter()
        .filter_map(|m| match scene.geometry(m) {
            Some(ItemGeometry::Erase { path, radius }) => Some((path.as_slice(), *radius)),
            _ => None,
        })
        .collect()
}

/// True when `p` lies inside some mask (`dist(p, mask) ≤ mask.radius`).
pub(crate) fn masked(masks: &[PageMask<'_>], p: [f32; 2]) -> bool {
    masks
        .iter()
        .any(|&(path, r)| point_polyline_dist(p, path) <= r)
}

/// Segments of a polyline: one degenerate segment for a single point; `closed` adds the closing edge.
pub(crate) fn segments(
    line: &[[f32; 2]],
    closed: bool,
) -> impl Iterator<Item = ([f32; 2], [f32; 2])> + '_ {
    let single = match line {
        [only] => Some((*only, *only)),
        _ => None,
    };
    let closing = match (closed && line.len() > 2, line.first(), line.last()) {
        (true, Some(&first), Some(&last)) => Some((last, first)),
        _ => None,
    };
    single
        .into_iter()
        .chain(line.windows(2).map(|w| (w[0], w[1])))
        .chain(closing)
}

/// Point of segment `a0–a1` closest to segment `b0–b1` (their crossing point when they cross).
pub(crate) fn closest_on(a0: [f32; 2], a1: [f32; 2], b0: [f32; 2], b1: [f32; 2]) -> [f32; 2] {
    let da = [a1[0] - a0[0], a1[1] - a0[1]];
    let db = [b1[0] - b0[0], b1[1] - b0[1]];
    let d = cross(da, db);
    if d.abs() > 0.0 {
        let w = [b0[0] - a0[0], b0[1] - a0[1]];
        let t = cross(w, db) / d;
        let s = cross(w, da) / d;
        if (0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&s) {
            return [a0[0] + da[0] * t, a0[1] + da[1] * t];
        }
    }
    let on_a = |p: [f32; 2]| {
        let (_, t) = point_segment_dist2(p, a0, a1);
        [a0[0] + da[0] * t, a0[1] + da[1] * t]
    };
    let candidates = [a0, a1, on_a(b0), on_a(b1)];
    let mut best = a0;
    let mut best_d2 = f32::INFINITY;
    for c in candidates {
        let (d2, _) = point_segment_dist2(c, b0, b1);
        if d2 < best_d2 {
            best = c;
            best_d2 = d2;
        }
    }
    best
}

fn cross(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[1] - a[1] * b[0]
}

/// Some segment of `line` is within `tol` of some eraser segment at an unmasked point.
fn line_touches(
    line: &[[f32; 2]],
    closed: bool,
    index: &EraserIndex,
    tol: f32,
    masks: &[PageMask<'_>],
) -> bool {
    segments(line, closed).any(|(a0, a1)| pair_touches(a0, a1, index, tol, masks))
}

/// Segment `a0–a1` is within `tol` of some eraser segment at an unmasked point. Only eraser segments whose
/// box comes within `tol` of the pair's box are tested (none when the pair misses the eraser bounds).
fn pair_touches(
    a0: [f32; 2],
    a1: [f32; 2],
    index: &EraserIndex,
    tol: f32,
    masks: &[PageMask<'_>],
) -> bool {
    index.any(&query_box(a0, a1, tol), |e0, e1| {
        segment_segment_dist(e0, e1, a0, a1) <= tol
            && (masks.is_empty() || !masked(masks, closest_on(a0, a1, e0, e1)))
    })
}

/// Stroke rule: tapered-capsule contact with any visible centreline segment.
fn stroke_touches(
    centreline: &Centreline,
    index: &EraserIndex,
    radius: f32,
    masks: &[PageMask<'_>],
) -> bool {
    let mut start = 0usize;
    for &end in &centreline.run_ends {
        let end = usize::try_from(end).unwrap_or(usize::MAX);
        let run = centreline.points.get(start..end).unwrap_or(&[]);
        start = end;
        if run_touches(run, index, radius, masks) {
            return true;
        }
    }
    false
}

fn run_touches(
    run: &[CentrePoint],
    index: &EraserIndex,
    radius: f32,
    masks: &[PageMask<'_>],
) -> bool {
    let single = match run {
        [only] => Some((*only, *only)),
        _ => None,
    };
    single
        .into_iter()
        .chain(run.windows(2).map(|w| (w[0], w[1])))
        .any(|(c0, c1)| {
            let tol = radius + max(c0.half_width, c1.half_width);
            pair_touches([c0.x, c0.y], [c1.x, c1.y], index, tol, masks)
        })
}

/// Shape rule: near the flattened path, or an eraser point inside the filled closed run.
fn shape_touches(
    g: &ShapeGeometry,
    tol: f32,
    path: &[[f32; 2]],
    index: &EraserIndex,
    masks: &[PageMask<'_>],
) -> bool {
    let mut start = 0usize;
    for &end in &g.path.run_ends {
        let end = usize::try_from(end).unwrap_or(usize::MAX);
        let run = g.path.points.get(start..end).unwrap_or(&[]);
        start = end;
        if line_touches(run, g.path.closed, index, tol, masks) {
            return true;
        }
        if g.fill.is_some()
            && g.path.closed
            && path
                .iter()
                .any(|&p| ring_contains(run, p) && !masked(masks, p))
        {
            return true;
        }
    }
    false
}

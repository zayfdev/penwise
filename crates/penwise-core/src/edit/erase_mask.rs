//! Precise erase of stamp strokes and shapes: erase-mask items (editing-algorithms.md §2).
//!
//! Owner: slice b.

use super::erase_stroke::{PageMask, page_masks, path_touches, segments};
use super::proximity::{point_polyline_dist, resample, ring_contains};
use crate::bounds::Bounds;
use crate::float::max;
use crate::geometry::StrokeGeometry;
use crate::item::{EraseItem, ItemGeometry, ItemId, ItemKind, Scene};
use crate::sample::POSITION_STEPS_PER_PX;

/// The mask the eraser `(path, radius)` (page px) leaves on live target `target`, or `None` when the eraser
/// does not touch the target's visible (unmasked) geometry. The path keeps only the eraser points whose
/// capsule segments intersect the target's bounds inflated by `radius` (plus one neighbour on each side), is
/// mapped into the target's local frame with the inverse transform and snapped to the 1/16 px grid; the
/// radius becomes `radius / width_scale`. `None` as well when the target transform is not invertible.
#[must_use]
pub fn mask_for(
    scene: &Scene,
    target: ItemId,
    path: &[[f32; 2]],
    radius: f32,
) -> Option<EraseItem> {
    let item = scene.get(target)?;
    if !matches!(item.kind, ItemKind::Stroke(_) | ItemKind::Shape(_)) {
        return None;
    }
    if !path_touches(scene, target, path, radius) {
        return None;
    }
    let inverse = item.transform.invert()?;
    let scale = item.transform.width_scale();
    let local_radius = radius / scale;
    if !local_radius.is_finite() || local_radius <= 0.0 {
        return None;
    }
    let area = scene.bounds(target)?.inflate(radius);
    let local = kept_points(path, &area)
        .into_iter()
        .map(|p| snap(inverse.apply(p)))
        .collect::<Vec<_>>();
    if local.is_empty() || local.iter().any(|p| !p[0].is_finite() || !p[1].is_finite()) {
        return None;
    }
    Some(EraseItem {
        target,
        path: local,
        radius: local_radius,
    })
}

/// GC test: true when every visible part of `target` lies under the union of its live masks plus `extra`:
/// - stamp stroke: every dab centre satisfies `dist(dab, mask) + dab.radius_x ≤ mask.radius` for some mask;
/// - shape: every point of the flattened page path, resampled at 1 px, lies within some
///   `mask.radius − brush_size_max / 2`, and the fill (if any) is fully covered by the same test on its
///   polygon resampled at 1 px plus a 1 px interior grid.
///
/// When true, the precise eraser emits `Delete(target)` instead of adding `extra` (fully erased items must not
/// linger in hit testing).
#[must_use]
pub fn fully_masked(scene: &Scene, target: ItemId, extra: &EraseItem) -> bool {
    let (Some(item), Some(geometry)) = (scene.get(target), scene.geometry(target)) else {
        return false;
    };
    let (extra_path, extra_radius) = extra.page_mask(&item.transform);
    let mut masks = page_masks(scene, target);
    masks.push((extra_path.as_slice(), extra_radius));
    match (&item.kind, geometry) {
        (
            ItemKind::Stroke(_),
            ItemGeometry::Stroke {
                geometry: StrokeGeometry::Stamps { dabs, .. },
                ..
            },
        ) => dabs.iter().all(|d| within(&masks, [d.x, d.y], d.radius_x)),
        (ItemKind::Shape(shape), ItemGeometry::Shape(g)) => {
            let margin = shape.brush.size_max * 0.5;
            let mut points = Vec::new();
            let mut start = 0usize;
            for &end in &g.path.run_ends {
                let end = usize::try_from(end).unwrap_or(usize::MAX);
                let run = g.path.points.get(start..end).unwrap_or(&[]);
                start = end;
                points.clear();
                resample(run, g.path.closed, 1.0, &mut points);
                if !points.iter().all(|&p| within(&masks, p, margin)) {
                    return false;
                }
                if g.fill.is_some() && g.path.closed && !interior_covered(run, &masks, margin) {
                    return false;
                }
            }
            true
        }
        _ => false,
    }
}

/// Some mask contains the disc of radius `margin` at `p`: `dist(p, mask) + margin ≤ mask.radius`.
fn within(masks: &[PageMask<'_>], p: [f32; 2], margin: f32) -> bool {
    masks
        .iter()
        .any(|&(path, r)| point_polyline_dist(p, path) + margin <= r)
}

/// Every point of the 1 px grid inside `ring` passes [`within`].
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_precision_loss,
    clippy::cast_sign_loss,
    reason = "grid counts are bounded by the finite ring bounds and clamped before the casts"
)]
fn interior_covered(ring: &[[f32; 2]], masks: &[PageMask<'_>], margin: f32) -> bool {
    let mut b = Bounds::EMPTY;
    for &[x, y] in ring {
        b.include_point(x, y);
    }
    if b.is_empty() {
        return true;
    }
    let x0 = b.min_x.floor();
    let y0 = b.min_y.floor();
    let nx = (b.max_x - x0).ceil();
    let ny = (b.max_y - y0).ceil();
    if !(nx.is_finite() && ny.is_finite()) || nx * ny > 16_777_216.0 {
        return false;
    }
    let (nx, ny) = (max(nx, 0.0) as u32, max(ny, 0.0) as u32);
    for j in 0..=ny {
        for i in 0..=nx {
            let p = [x0 + i as f32, y0 + j as f32];
            if ring_contains(ring, p) && !within(masks, p, margin) {
                return false;
            }
        }
    }
    true
}

/// Eraser points whose capsule segments can reach `area` (segment bounds intersect it), plus one neighbour
/// on each side. Gaps between kept stretches are bridged with the original points whenever the straight
/// bridge could itself reach `area`, so the mask never covers more than the eraser did.
fn kept_points(path: &[[f32; 2]], area: &Bounds) -> Vec<[f32; 2]> {
    let n = path.len();
    let mut keep = vec![false; n];
    let reaches = |a: [f32; 2], b: [f32; 2]| {
        let mut s = Bounds::from_point(a[0], a[1]);
        s.include_point(b[0], b[1]);
        s.intersects(area)
    };
    for (i, (a, b)) in segments(path, false).enumerate() {
        if reaches(a, b) {
            let hi = if n == 1 { 0 } else { (i + 2).min(n - 1) };
            if let Some(stretch) = keep.get_mut(i.saturating_sub(1)..=hi) {
                stretch.fill(true);
            }
        }
    }
    let kept: Vec<usize> = (0..n).filter(|&k| keep[k]).collect();
    let mut out = Vec::with_capacity(kept.len());
    for (j, &k) in kept.iter().enumerate() {
        if let Some(&prev) = j.checked_sub(1).and_then(|j| kept.get(j))
            && k > prev + 1
            && reaches(path[prev], path[k])
        {
            out.extend_from_slice(&path[prev + 1..k]);
        }
        out.push(path[k]);
    }
    out
}

/// Snaps a local point to the 1/16 px grid.
fn snap(p: [f32; 2]) -> [f32; 2] {
    let s = POSITION_STEPS_PER_PX;
    [(p[0] * s).round() / s, (p[1] * s).round() / s]
}

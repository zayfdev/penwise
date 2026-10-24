//! Rasterizes the scene's ink walls into a window (editing-algorithms.md §3.3).
//!
//! Owner: slice c (slice-internal).

use super::FILL_CELL;
use super::grid::{Window, reset};
use super::scan::{clear_capsule, fill_capsule, fill_contours};
use crate::brush::BlendMode;
use crate::geometry::{Dab, Outline, StrokeGeometry};
use crate::item::{Centreline, ItemGeometry, ItemKind, LayerSet, Scene};

/// Stamp dabs count as walls when `opacity · hardness` reaches this.
const STAMP_WALL_ALPHA: f32 = 0.25;

/// Clears `bits` (resized to the window) and sets wall cells from live ink-layer items whose bounds meet the
/// window, in `(z, id)` order (order is irrelevant to the union; fixed for reproducibility):
/// - outline strokes: their outline contours ([`super::scan::fill_contours`]) **and** their centreline
///   capsules at `max(h, FILL_CELL / 2)` (guarantees ≥ 1 cell width);
/// - stamp strokes: capsules along consecutive dabs with `opacity · hardness ≥ 0.25`, radius `radius_x`;
/// - shapes: their stroke outline contours plus path capsules at half the brush's minimum width;
/// - an item with masks is drawn into a separate layer, its masks are cleared there with
///   [`super::scan::clear_capsule`], and the layer is merged (a mask only removes its own target's wall).
///
/// Skipped: fills, shape fills, `BlendMode::Multiply` strokes and shapes, erase items, agent/text layers.
/// Fixed-page edges are walls of the flood step (virtual EDT edges), not drawn here.
pub(crate) fn rasterize_walls(scene: &Scene, window: &Window, bits: &mut Vec<u64>) {
    reset(bits, window.width, window.height);
    let mut layer: Vec<u64> = Vec::new();
    for id in scene.query(&window.page_bounds().inflate(FILL_CELL), LayerSet::INK) {
        let (Some(item), Some(geometry)) = (scene.get(id), scene.geometry(id)) else {
            continue;
        };
        let path_radius = match &item.kind {
            ItemKind::Stroke(s) if s.params.blend != BlendMode::Multiply => 0.0,
            ItemKind::Shape(s) if s.brush.blend != BlendMode::Multiply => s.brush.size_min * 0.5,
            _ => continue,
        };
        let masks = scene.masks(id);
        if masks.is_empty() {
            draw(window, geometry, path_radius, bits);
            continue;
        }
        reset(&mut layer, window.width, window.height);
        draw(window, geometry, path_radius, &mut layer);
        for mask in masks {
            if let Some(ItemGeometry::Erase { path, radius }) = scene.geometry(mask) {
                chain(path, false, |a, b| {
                    clear_capsule(window, a, b, *radius, &mut layer);
                });
            }
        }
        for (w, l) in bits.iter_mut().zip(&layer) {
            *w |= l;
        }
    }
}

/// Draws one item's walls. `path_radius` is the shape path's capsule radius (unused for strokes, whose
/// centreline carries per-point half-widths).
fn draw(window: &Window, geometry: &ItemGeometry, path_radius: f32, bits: &mut [u64]) {
    match geometry {
        ItemGeometry::Stroke {
            geometry,
            centreline,
        } => {
            draw_stroke(window, geometry, bits);
            draw_centreline(window, centreline, bits);
        }
        ItemGeometry::Shape(shape) => {
            draw_stroke(window, &shape.stroke, bits);
            let path = &shape.path;
            for run in runs(&path.points, &path.run_ends) {
                chain(run, path.closed, |a, b| {
                    fill_capsule(window, a, b, path_radius, bits);
                });
            }
        }
        _ => {}
    }
}

fn draw_stroke(window: &Window, geometry: &StrokeGeometry, bits: &mut [u64]) {
    match geometry {
        StrokeGeometry::Outline { outline, .. } => {
            fill_contours(window, &contours(outline), bits);
        }
        StrokeGeometry::Stamps { dabs, .. } => draw_dabs(window, dabs, bits),
    }
}

/// Capsules joining consecutive wall dabs (a lone wall dab is a disc).
fn draw_dabs(window: &Window, dabs: &[Dab], bits: &mut [u64]) {
    let mut prev: Option<&Dab> = None;
    for d in dabs {
        if d.opacity * d.hardness < STAMP_WALL_ALPHA {
            prev = None;
            continue;
        }
        let (from, radius) = prev.map_or(([d.x, d.y], d.radius_x), |p| {
            ([p.x, p.y], crate::float::max(p.radius_x, d.radius_x))
        });
        fill_capsule(window, from, [d.x, d.y], radius, bits);
        prev = Some(d);
    }
}

/// Tapered-capsule chain of each centreline run, at the larger half-width of each segment.
fn draw_centreline(window: &Window, centreline: &Centreline, bits: &mut [u64]) {
    let mut start = 0usize;
    for &end in &centreline.run_ends {
        let end = end as usize;
        if let Some(run) = centreline.points.get(start..end) {
            if let [only] = run {
                fill_capsule(
                    window,
                    [only.x, only.y],
                    [only.x, only.y],
                    only.half_width,
                    bits,
                );
            }
            for pair in run.windows(2) {
                let (p, q) = (pair[0], pair[1]);
                let r = crate::float::max(p.half_width, q.half_width);
                fill_capsule(window, [p.x, p.y], [q.x, q.y], r, bits);
            }
        }
        start = end;
    }
}

/// Contour slices of an outline.
fn contours(outline: &Outline) -> Vec<&[[f32; 2]]> {
    runs(&outline.points, &outline.contour_ends)
}

/// Splits `points` at the exclusive `ends`.
fn runs<'a>(points: &'a [[f32; 2]], ends: &[u32]) -> Vec<&'a [[f32; 2]]> {
    let mut start = 0usize;
    ends.iter()
        .filter_map(|&end| {
            let end = end as usize;
            let run = points.get(start..end);
            start = end;
            run
        })
        .collect()
}

/// Calls `f` for each segment of `points` (a lone point is a zero-length segment; `closed` adds the closing
/// segment).
fn chain(points: &[[f32; 2]], closed: bool, mut f: impl FnMut([f32; 2], [f32; 2])) {
    match points {
        [] => {}
        [only] => f(*only, *only),
        _ => {
            for pair in points.windows(2) {
                f(pair[0], pair[1]);
            }
            if closed && let (Some(&first), Some(&last)) = (points.first(), points.last()) {
                f(last, first);
            }
        }
    }
}

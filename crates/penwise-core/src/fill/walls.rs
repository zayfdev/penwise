//! Rasterizes the scene's ink walls into a window (editing-algorithms.md §3.3).
//!
//! Owner: slice c (slice-internal).

use super::FILL_CELL;
use super::grid::{Window, index, reset};
use super::scan::{clear_capsule, fill_capsule, fill_contours};
use crate::bounds::Bounds;
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
/// - an item with masks is drawn into `layer` (caller scratch, reused), restricted to the window rows its
///   bounds can reach ([`item_rows`]); its masks are cleared there with [`super::scan::clear_capsule`] and
///   only those rows are merged (a mask only removes its own target's wall). Per masked item the work is
///   proportional to its rows, never to the window.
///
/// Skipped: fills, shape fills, `BlendMode::Multiply` strokes and shapes, erase items, agent/text layers.
/// Fixed-page edges are walls of the flood step (virtual EDT edges), not drawn here.
pub(crate) fn rasterize_walls(
    scene: &Scene,
    window: &Window,
    bits: &mut Vec<u64>,
    layer: &mut Vec<u64>,
) {
    reset(bits, window.width, window.height);
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
        let bounds = scene.bounds(id).unwrap_or_else(|| geometry.bounds());
        let margin = crate::float::max(path_radius, FILL_CELL) + FILL_CELL;
        let Some((start, end)) = item_rows(window, &bounds, margin) else {
            continue;
        };
        // Same width and global cell grid, fewer rows: rasterization is window-independent, so the band's
        // cells equal the full window's rows `start..end`.
        let band = Window {
            origin: [window.origin[0], window.origin[1] + i64::from(start)],
            height: end - start,
            ..*window
        };
        reset(layer, band.width, band.height);
        draw(&band, geometry, path_radius, layer);
        for mask in masks {
            if let Some(ItemGeometry::Erase { path, radius }) = scene.geometry(mask) {
                chain(path, false, |a, b| {
                    clear_capsule(&band, a, b, *radius, layer);
                });
            }
        }
        or_at(bits, layer, index(window.width, 0, start));
    }
}

/// Window rows `start..end` (non-empty) whose cell centres may lie within `margin` page px of `bounds`
/// vertically; `None` when no row can. Non-finite bounds widen to the whole window.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    clippy::cast_precision_loss,
    reason = "window origins are bounded by the 2^24 px tap limit; local rows are clamped to 0..=height"
)]
fn item_rows(window: &Window, bounds: &Bounds, margin: f32) -> Option<(u32, u32)> {
    let cell = f64::from(FILL_CELL);
    let origin = window.origin[1] as f64;
    let n = window.height;
    let local = |g: f64, nan: u32| {
        let v = g - origin;
        if v.is_nan() {
            nan
        } else if v <= 0.0 {
            0
        } else if v >= f64::from(n) {
            n
        } else {
            v as u32
        }
    };
    let m = f64::from(margin);
    let start = local(((f64::from(bounds.min_y) - m) / cell).floor(), 0);
    let end = local(((f64::from(bounds.max_y) + m) / cell).ceil() + 1.0, n);
    (start < end).then_some((start, end))
}

/// ORs the bit grid `layer` into `bits` starting at flat bit index `offset` (rows are not word-aligned).
fn or_at(bits: &mut [u64], layer: &[u64], offset: usize) {
    let (base, shift) = (offset >> 6, offset & 63);
    for (k, &word) in layer.iter().enumerate() {
        if word == 0 {
            continue;
        }
        if let Some(w) = bits.get_mut(base + k) {
            *w |= word << shift;
        }
        if shift > 0
            && let Some(w) = bits.get_mut(base + k + 1)
        {
            *w |= word >> (64 - shift);
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

#[cfg(test)]
mod tests;

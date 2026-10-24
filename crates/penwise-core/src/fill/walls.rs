//! Rasterizes the scene's ink walls into a window (editing-algorithms.md §3.3).
//!
//! Owner: slice c (slice-internal).

use super::grid::Window;
use crate::item::Scene;

/// Clears `bits` (resized to the window) and sets wall cells from live ink-layer items whose bounds meet the
/// window, in `(z, id)` order (order is irrelevant to the union; fixed for reproducibility):
/// - outline strokes: their outline contours ([`super::scan::fill_contours`]) **and** their centreline
///   capsules at `max(h, FILL_CELL / 2)` (guarantees ≥ 1 cell width);
/// - stamp strokes: capsules along consecutive dabs with `opacity · hardness ≥ 0.25`, radius `radius_x`;
/// - shapes: their stroke outline contours plus path capsules at half the brush width;
/// - then each item's masks are cleared with [`super::scan::clear_capsule`] before the next item is drawn
///   (a mask only removes its own target's wall).
///
/// Skipped: fills, shape fills, `BlendMode::Multiply` strokes, erase items, agent/text layers. Fixed-page
/// edges are added as walls by the flood step, not here.
pub(crate) fn rasterize_walls(scene: &Scene, window: &Window, bits: &mut Vec<u64>) {
    todo!()
}

//! Precise erase of stamp strokes and shapes: erase-mask items (editing-algorithms.md §2).
//!
//! Owner: slice b.

use crate::item::{EraseItem, ItemId, Scene};

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
    todo!()
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
    todo!()
}

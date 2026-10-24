//! Move / scale / rotate selections about an anchor; angle snapping (editing-algorithms.md §5).
//!
//! Owner: slice a.
//!
//! Live drags are previews only (the app renders items with a preview transform); one set of absolute
//! `Update { transform }` edits is emitted at gesture end. Erase items are never transformed directly: masks
//! live in their target's local frame and follow it.

use super::Edit;
use crate::bounds::Bounds;
use crate::item::{Affine, ItemId, Scene};

/// Snap step for rotation: 15°.
pub const SNAP_STEP: f32 = core::f32::consts::PI / 12.0;
/// A drag angle within 3° of a multiple of [`SNAP_STEP`] snaps to it.
pub const SNAP_TOLERANCE: f32 = core::f32::consts::PI / 60.0;

/// For each live, non-erase id in `ids` (input order irrelevant; output sorted by id, duplicates and dead ids
/// skipped): `Update { id, transform: Some(item.transform.then(delta)) }`. Items whose new transform is not
/// finite or not invertible are skipped. Identity `delta` → empty.
#[must_use]
pub fn transform_items(scene: &Scene, ids: &[ItemId], delta: &Affine) -> Vec<Edit> {
    todo!()
}

/// [`transform_items`] with `Affine::translate(dx, dy)`.
#[must_use]
pub fn move_items(scene: &Scene, ids: &[ItemId], dx: f32, dy: f32) -> Vec<Edit> {
    todo!()
}

/// [`transform_items`] with `Affine::scale(sx, sy).about(anchor)` (anchor in page px stays fixed; negative
/// factors mirror). Ink strokes get thicker/thinner by `sqrt(|sx·sy|)`; shapes keep their brush width.
#[must_use]
pub fn scale_items(scene: &Scene, ids: &[ItemId], anchor: [f32; 2], sx: f32, sy: f32) -> Vec<Edit> {
    todo!()
}

/// [`transform_items`] with `Affine::rotate(angle).about(pivot)`.
#[must_use]
pub fn rotate_items(scene: &Scene, ids: &[ItemId], pivot: [f32; 2], angle: f32) -> Vec<Edit> {
    todo!()
}

/// Rotation of a drag around `pivot` from pointer `from` to pointer `to`:
/// `atan2(to − pivot) − atan2(from − pivot)` via `libm::atan2f`, wrapped to `(−π, π]`; `0` when either
/// pointer coincides with the pivot. With `snap`, passed through [`snap_angle`].
#[must_use]
pub fn drag_angle(pivot: [f32; 2], from: [f32; 2], to: [f32; 2], snap: bool) -> f32 {
    todo!()
}

/// `angle` rounded to the nearest multiple of [`SNAP_STEP`] when within [`SNAP_TOLERANCE`] of it, else
/// unchanged. Non-finite → `0`.
#[must_use]
pub fn snap_angle(angle: f32) -> f32 {
    todo!()
}

/// Union of the cached page bounds of the live, non-erase items in `ids` ([`Bounds::EMPTY`] if none).
#[must_use]
pub fn selection_bounds(scene: &Scene, ids: &[ItemId]) -> Bounds {
    todo!()
}

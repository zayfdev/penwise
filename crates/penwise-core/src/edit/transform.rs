//! Move / scale / rotate selections about an anchor; angle snapping (editing-algorithms.md §5).
//!
//! Owner: slice a.
//!
//! Live drags are previews only (the app renders items with a preview transform); one set of absolute
//! `Update { transform }` edits is emitted at gesture end. Erase items are never transformed directly: masks
//! live in their target's local frame and follow it.

use super::{Edit, ItemPatch};
use crate::bounds::Bounds;
use crate::item::{Affine, Item, ItemId, ItemKind, Scene};

/// Snap step for rotation: 15°.
pub const SNAP_STEP: f32 = core::f32::consts::PI / 12.0;
/// A drag angle within 3° of a multiple of [`SNAP_STEP`] snaps to it.
pub const SNAP_TOLERANCE: f32 = core::f32::consts::PI / 60.0;

/// For each live, non-erase id in `ids` (input order irrelevant; output sorted by id, duplicates and dead ids
/// skipped): `Update { id, transform: Some(item.transform.then(delta)) }`. Items whose new transform is not
/// finite or not invertible are skipped. Identity `delta` → empty.
#[must_use]
pub fn transform_items(scene: &Scene, ids: &[ItemId], delta: &Affine) -> Vec<Edit> {
    if *delta == Affine::IDENTITY {
        return Vec::new();
    }
    selection(scene, ids)
        .filter_map(|item| {
            let transform = item.transform.then(delta);
            (transform.is_finite() && transform.invert().is_some()).then(|| Edit::Update {
                id: item.id,
                patch: ItemPatch {
                    transform: Some(transform),
                    ..ItemPatch::default()
                },
            })
        })
        .collect()
}

/// [`transform_items`] with `Affine::translate(dx, dy)`.
#[must_use]
pub fn move_items(scene: &Scene, ids: &[ItemId], dx: f32, dy: f32) -> Vec<Edit> {
    transform_items(scene, ids, &Affine::translate(dx, dy))
}

/// [`transform_items`] with `Affine::scale(sx, sy).about(anchor)` (anchor in page px stays fixed; negative
/// factors mirror). Ink strokes get thicker/thinner by `sqrt(|sx·sy|)`; shapes keep their brush width.
#[must_use]
pub fn scale_items(scene: &Scene, ids: &[ItemId], anchor: [f32; 2], sx: f32, sy: f32) -> Vec<Edit> {
    transform_items(scene, ids, &Affine::scale(sx, sy).about(anchor))
}

/// [`transform_items`] with `Affine::rotate(angle).about(pivot)`.
#[must_use]
pub fn rotate_items(scene: &Scene, ids: &[ItemId], pivot: [f32; 2], angle: f32) -> Vec<Edit> {
    transform_items(scene, ids, &Affine::rotate(angle).about(pivot))
}

/// Rotation of a drag around `pivot` from pointer `from` to pointer `to`:
/// `atan2(to − pivot) − atan2(from − pivot)` via `libm::atan2f`, wrapped to `(−π, π]`; `0` when either
/// pointer coincides with the pivot. With `snap`, passed through [`snap_angle`].
#[must_use]
pub fn drag_angle(pivot: [f32; 2], from: [f32; 2], to: [f32; 2], snap: bool) -> f32 {
    let (fx, fy) = (from[0] - pivot[0], from[1] - pivot[1]);
    let (tx, ty) = (to[0] - pivot[0], to[1] - pivot[1]);
    if (fx == 0.0 && fy == 0.0) || (tx == 0.0 && ty == 0.0) {
        return 0.0;
    }
    let pi = core::f32::consts::PI;
    let mut angle = libm::atan2f(ty, tx) - libm::atan2f(fy, fx);
    if angle > pi {
        angle -= 2.0 * pi;
    } else if angle <= -pi {
        angle += 2.0 * pi;
    }
    if snap { snap_angle(angle) } else { angle }
}

/// `angle` rounded to the nearest multiple of [`SNAP_STEP`] when within [`SNAP_TOLERANCE`] of it, else
/// unchanged. Non-finite → `0`.
#[must_use]
pub fn snap_angle(angle: f32) -> f32 {
    if !angle.is_finite() {
        return 0.0;
    }
    // `round` is IEEE-exact (half away from zero), so this is identical on every target.
    let snapped = (angle / SNAP_STEP).round() * SNAP_STEP;
    if (angle - snapped).abs() <= SNAP_TOLERANCE {
        snapped
    } else {
        angle
    }
}

/// Union of the cached page bounds of the live, non-erase items in `ids` ([`Bounds::EMPTY`] if none).
#[must_use]
pub fn selection_bounds(scene: &Scene, ids: &[ItemId]) -> Bounds {
    selection(scene, ids)
        .filter_map(|item| scene.bounds(item.id))
        .fold(Bounds::EMPTY, |acc, b| acc.union(&b))
}

/// Live, non-erase items of `ids`, ascending id, each once.
pub(crate) fn selection<'a>(
    scene: &'a Scene,
    ids: &[ItemId],
) -> impl Iterator<Item = &'a Item> + 'a {
    let mut sorted = ids.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    sorted
        .into_iter()
        .filter_map(|id| scene.get(id))
        .filter(|item| !matches!(item.kind, ItemKind::Erase(_)))
}

#[cfg(test)]
mod tests;

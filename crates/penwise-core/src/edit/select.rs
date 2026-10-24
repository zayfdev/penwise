//! Selection actions besides transforms: delete, duplicate, recolor, promote layer (PLAN "Select").
//!
//! Owner: slice a.

use super::Edit;
use crate::brush::Color;
use crate::item::{IdSource, ItemId, Layer, Scene};

/// `Delete(id)` for each live, non-erase id (sorted by id, duplicates/dead skipped). Masks of deleted items
/// stay (they are inert while the target is dead and come back with it on undo).
#[must_use]
pub fn delete_items(scene: &Scene, ids: &[ItemId]) -> Vec<Edit> {
    todo!()
}

/// Copies of the live, non-erase items in `ids`, offset by `(dx, dy)` page px and stacked on top:
/// - items processed in `(z, id)` order; each copy takes `ids_src.next_id()` in that order, z values
///   `scene.next_z() + k·Z_STEP` (k = 0, 1, …) preserving relative order, transform `then(translate)`;
/// - each copy's masks are copied too (fresh ids drawn right after their target's, `target` retargeted);
/// - a trimmed stroke piece becomes its own family root: `trim.origin` = the copy's id and `trim.visible`
///   = `[the piece's current effective visible range(s)]` (so it no longer intersects with the original's
///   family). `own` indexes the copy's own range.
///
/// Returns the `Add` edits in creation order.
#[must_use]
pub fn duplicate_items(
    scene: &Scene,
    ids: &[ItemId],
    dx: f32,
    dy: f32,
    ids_src: &mut dyn IdSource,
) -> Vec<Edit> {
    todo!()
}

/// Recolor: per live item (sorted by id) an absolute `Update` of the field holding its color — stroke /
/// shape `Brush` (whole params with only `color` replaced), fill / text `Paint` (opacity kept). Erase items
/// skipped. Items already of that color are skipped.
#[must_use]
pub fn recolor_items(scene: &Scene, ids: &[ItemId], color: Color) -> Vec<Edit> {
    todo!()
}

/// Moves items to `layer` (e.g. promote agent ink to the ink layer): `Update { layer }` per live item whose
/// layer differs, plus the same update for each of its live masks (masks share their target's layer).
#[must_use]
pub fn set_layer(scene: &Scene, ids: &[ItemId], layer: Layer) -> Vec<Edit> {
    todo!()
}

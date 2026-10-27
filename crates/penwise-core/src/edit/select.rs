//! Selection actions besides transforms: delete, duplicate, recolor, promote layer (PLAN "Select").
//!
//! Owner: slice a.

use super::transform::selection;
use super::{Edit, ItemPatch};
use crate::brush::{BrushParams, Color};
use crate::item::{Affine, IdSource, Item, ItemId, ItemKind, Layer, Paint, Scene, Trim, Z_STEP};

/// `Delete(id)` for each live, non-erase id (sorted by id, duplicates/dead skipped). Masks of deleted items
/// stay (they are inert while the target is dead and come back with it on undo).
#[must_use]
pub fn delete_items(scene: &Scene, ids: &[ItemId]) -> Vec<Edit> {
    selection(scene, ids)
        .map(|item| Edit::Delete(item.id))
        .collect()
}

/// Copies of the live, non-erase items in `ids`, offset by `(dx, dy)` page px and stacked on top:
/// - items processed in `(z, id)` order; each copy takes `ids_src.next_id()` in that order, z values
///   `scene.next_z() + k·Z_STEP` (k = 0, 1, …) preserving relative order, transform `then(translate)`;
///   copies whose z would overflow `i64` (or all, when `next_z` is `None`) are not created;
/// - each copy's masks are copied too (fresh ids drawn right after their target's, `target` retargeted);
/// - a trimmed stroke piece becomes its own family root: `trim.origin` = the copy's id and `trim.visible`
///   = `[the piece's current effective visible range(s)]` (so it no longer intersects with the original's
///   family). `own` indexes the copy's own range.
///
/// Returns the `Add` edits in creation order.
///
/// A piece renders one range, so when concurrent splits left a stroke with several effective ranges, the
/// copy is one piece per range (same z, all rooted at the first; each piece's id is drawn before its mask
/// copies, which follow it). The same applies to an untrimmed stroke whose live family hides part of it.
/// Strokes with nothing visible are not copied.
#[must_use]
pub fn duplicate_items(
    scene: &Scene,
    ids: &[ItemId],
    dx: f32,
    dy: f32,
    ids_src: &mut dyn IdSource,
) -> Vec<Edit> {
    let mut items: Vec<&Item> = selection(scene, ids).collect();
    items.sort_by_key(|item| (item.z, item.id));
    let Some(base) = scene.next_z() else {
        return Vec::new();
    };
    let offset = Affine::translate(dx, dy);
    let mut edits = Vec::new();
    for (k, item) in items.into_iter().enumerate() {
        let z = i64::try_from(k)
            .ok()
            .and_then(|k| k.checked_mul(Z_STEP))
            .and_then(|step| base.checked_add(step));
        let Some(z) = z else {
            break;
        };
        let mut copy = item.clone();
        copy.z = z;
        copy.transform = item.transform.then(&offset);
        let pieces = if let ItemKind::Stroke(stroke) = &item.kind {
            let visible = scene.visible_ranges(item.id);
            let whole = stroke.trim.is_none() && visible == [stroke.own_range()];
            (!whole).then(|| {
                let trim = stroke.trim.as_ref();
                (
                    visible,
                    trim.map_or(0, |t| t.sample_offset),
                    trim.and_then(|t| t.warm.clone()),
                )
            })
        } else {
            None
        };
        if let Some((visible, sample_offset, warm)) = pieces {
            let mut root = None;
            for own in 0..visible.len() {
                let mut piece = copy.clone();
                piece.id = ids_src.next_id();
                let target = piece.id;
                let origin = *root.get_or_insert(target);
                if let ItemKind::Stroke(s) = &mut piece.kind {
                    s.trim = Some(Trim {
                        origin,
                        sample_offset,
                        visible: visible.clone(),
                        own: u32::try_from(own).unwrap_or(u32::MAX),
                        warm: warm.clone(),
                    });
                }
                let (layer, z) = (piece.layer, piece.z);
                edits.push(Edit::Add(piece));
                copy_masks(scene, item.id, (target, layer, z), ids_src, &mut edits);
            }
        } else {
            copy.id = ids_src.next_id();
            let target = copy.id;
            let (layer, z) = (copy.layer, copy.z);
            edits.push(Edit::Add(copy));
            copy_masks(scene, item.id, (target, layer, z), ids_src, &mut edits);
        }
    }
    edits
}

/// `Add`s for copies of `original`'s live masks, retargeted to `target` with its layer and z.
fn copy_masks(
    scene: &Scene,
    original: ItemId,
    (target, layer, z): (ItemId, Layer, i64),
    ids_src: &mut dyn IdSource,
    edits: &mut Vec<Edit>,
) {
    for mask in scene.masks(original) {
        let Some(item) = scene.get(mask) else {
            continue;
        };
        let mut copy = item.clone();
        copy.id = ids_src.next_id();
        copy.layer = layer;
        copy.z = z;
        if let ItemKind::Erase(e) = &mut copy.kind {
            e.target = target;
        }
        edits.push(Edit::Add(copy));
    }
}

/// Recolor: per live item (sorted by id) an absolute `Update` of the field holding its color — stroke /
/// shape `Brush` (whole params with only `color` replaced), fill / text `Paint` (opacity kept). Erase items
/// skipped. Items already of that color are skipped.
#[must_use]
pub fn recolor_items(scene: &Scene, ids: &[ItemId], color: Color) -> Vec<Edit> {
    selection(scene, ids)
        .filter_map(|item| {
            let mut patch = ItemPatch::default();
            match &item.kind {
                ItemKind::Stroke(s) if s.params.color != color => {
                    patch.brush = Some(BrushParams { color, ..s.params });
                }
                ItemKind::Shape(s) if s.brush.color != color => {
                    patch.brush = Some(BrushParams { color, ..s.brush });
                }
                ItemKind::Fill(f) if f.paint.color != color => {
                    patch.paint = Some(Paint { color, ..f.paint });
                }
                ItemKind::Text(t) if t.paint.color != color => {
                    patch.paint = Some(Paint { color, ..t.paint });
                }
                _ => return None,
            }
            Some(Edit::Update { id: item.id, patch })
        })
        .collect()
}

/// Moves items to `layer` (e.g. promote agent ink to the ink layer): `Update { layer }` per live item whose
/// layer differs, plus the same update for each of its live masks (masks share their target's layer).
#[must_use]
pub fn set_layer(scene: &Scene, ids: &[ItemId], layer: Layer) -> Vec<Edit> {
    let update = |id| Edit::Update {
        id,
        patch: ItemPatch {
            layer: Some(layer),
            ..ItemPatch::default()
        },
    };
    let mut edits = Vec::new();
    for item in selection(scene, ids) {
        if item.layer != layer {
            edits.push(update(item.id));
        }
        for mask in scene.masks(item.id) {
            if scene.get(mask).is_some_and(|m| m.layer != layer) {
                edits.push(update(mask));
            }
        }
    }
    edits
}

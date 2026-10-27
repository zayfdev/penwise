//! Precise eraser entry point (PLAN "Eraser": splits outline strokes, masks stamp strokes; fills are cut).
//!
//! Owner: slice b.

use super::Edit;
use super::erase_fill::fill_cut_edits;
use super::erase_mask::{fully_masked, mask_for};
use super::erase_split::{split_edits, uncovered_in};
use super::erase_stroke::{EraserIndex, eraser_bounds, path_touches_in};
use crate::brush::BrushFamily;
use crate::item::{
    Affine, IdSource, Item, ItemGeometry, ItemId, ItemKind, LayerSet, Scene, StrokeItem,
};

/// Edits erasing exactly the area under the eraser capsule chain `(path, radius)` (page px) from live items on
/// `layers`. Candidates: [`Scene::query`] on the path bounds inflated by `radius` (+ max half-width), processed
/// in `(z, id)` order; ids drawn from `ids` in that order:
/// - outline-family stroke → [`super::erase_split::uncovered_ranges`] over
///   [`Scene::visible_ranges`], then [`super::erase_split::split_edits`] (`Delete` + piece `Add`s);
/// - stamp-family stroke or shape → [`super::erase_mask::mask_for`]; if [`super::erase_mask::fully_masked`]
///   → `Delete(target)`, else `Add(Item { kind: Erase, layer/z of the target, transform identity })`;
/// - fill → [`super::erase_fill::fill_cut_edits`];
/// - text and erase items → untouched.
///
/// One call = one eraser path (pen-down → up, or one chunk of it; apply between chunk calls). Every edit of a
/// gesture belongs to one undo step (undo restores originals and deletes pieces/masks). Empty path or
/// `radius ≤ 0` → empty.
#[must_use]
pub fn erase_precise(
    scene: &Scene,
    path: &[[f32; 2]],
    radius: f32,
    layers: LayerSet,
    ids: &mut dyn IdSource,
) -> Vec<Edit> {
    let Some(area) = eraser_bounds(path, radius) else {
        return Vec::new();
    };
    let index = EraserIndex::new(path);
    let mut edits = Vec::new();
    for id in scene.query(&area, layers) {
        let Some(item) = scene.get(id) else {
            continue;
        };
        match &item.kind {
            ItemKind::Stroke(stroke) if matches!(stroke.params.family, BrushFamily::Outline(_)) => {
                edits.extend(split_stroke(scene, item, stroke, path, radius, &index, ids));
            }
            ItemKind::Stroke(_) | ItemKind::Shape(_) => {
                edits.extend(mask_edit(scene, item, path, radius, ids));
            }
            ItemKind::Fill(_) => {
                if let Some(ItemGeometry::Fill { polygons, .. }) = scene.geometry(id) {
                    edits.extend(fill_cut_edits(item, polygons, path, radius));
                }
            }
            ItemKind::Text(_) | ItemKind::Image(_) | ItemKind::Erase(_) => {}
        }
    }
    edits
}

/// Outline stroke: `Delete` + piece `Add`s when the eraser covers some visible ink.
fn split_stroke(
    scene: &Scene,
    item: &Item,
    stroke: &StrokeItem,
    path: &[[f32; 2]],
    radius: f32,
    index: &EraserIndex,
    ids: &mut dyn IdSource,
) -> Vec<Edit> {
    // Cheap exact prefilter on the cached visible centreline (outline strokes carry no masks): no capsule
    // contact means no covered point.
    if !path_touches_in(scene, item.id, path, radius, index) {
        return Vec::new();
    }
    let visible = scene.visible_ranges(item.id);
    match uncovered_in(stroke, &item.transform, &visible, index, radius) {
        Some(kept) => split_edits(item, stroke, &kept, ids),
        None => Vec::new(),
    }
}

/// Stamp stroke or shape: one mask item, or `Delete` when the mask completes the erasure.
fn mask_edit(
    scene: &Scene,
    item: &Item,
    path: &[[f32; 2]],
    radius: f32,
    ids: &mut dyn IdSource,
) -> Option<Edit> {
    let target: ItemId = item.id;
    let mask = mask_for(scene, target, path, radius)?;
    if fully_masked(scene, target, &mask) {
        return Some(Edit::Delete(target));
    }
    Some(Edit::Add(Item {
        id: ids.next_id(),
        layer: item.layer,
        z: item.z,
        transform: Affine::IDENTITY,
        kind: ItemKind::Erase(mask),
    }))
}

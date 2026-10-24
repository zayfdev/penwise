//! Precise eraser entry point (PLAN "Eraser": splits outline strokes, masks stamp strokes; fills are cut).
//!
//! Owner: slice b.

use super::Edit;
use crate::item::{IdSource, LayerSet, Scene};

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
    todo!()
}

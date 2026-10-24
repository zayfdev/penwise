//! Tap hit testing (editing-algorithms.md §4).
//!
//! Owner: slice a.
//!
//! `tol` is in page units: the caller converts its screen-space tolerance (e.g. 8 dp) with the current zoom.
//! Core does no I/O and knows no zoom.

use crate::item::{ItemId, LayerSet, Scene};

/// Every live, non-erase item on `layers` hit at `p` with tolerance `tol ≥ 0`, sorted by `(z, id)` ascending.
/// Candidates come from [`Scene::query`] on `p` inflated by `tol` plus the item's max half-width; then
/// [`item_hit`] decides. Empty when nothing is hit.
#[must_use]
pub fn hit_test(scene: &Scene, p: [f32; 2], tol: f32, layers: LayerSet) -> Vec<ItemId> {
    todo!()
}

/// Exact test for one live item (false for unknown, deleted or erase items):
/// - stroke: some centreline run segment `i` with `dist(p, seg_i) ≤ lerp(h_i, h_{i+1}, t) + tol` (tapered
///   capsule; a 1-point run is a disc of its half-width);
/// - shape: `dist(p, flattened page path) ≤ brush_size_max / 2 + tol`, or inside its fill polygon when
///   filled (nonzero);
/// - fill: nonzero point-in-polygon over its page polygons (holes excluded), or within `tol` of a ring;
/// - text: inside the box quad or within `tol` of it.
///
/// Masks: for strokes and shapes, a point is not a hit when it lies inside any of the item's masks
/// (`dist(p, mask.path) ≤ mask.radius`, page space), editing-algorithms.md §2.
#[must_use]
pub fn item_hit(scene: &Scene, id: ItemId, p: [f32; 2], tol: f32) -> bool {
    todo!()
}

/// The visually topmost of `hits` (largest [`crate::item::RenderKey`], i.e. band then `(z, id)`), or `None`
/// for an empty slice. Tap-select uses `topmost(scene, &hit_test(..))`.
#[must_use]
pub fn topmost(scene: &Scene, hits: &[ItemId]) -> Option<ItemId> {
    todo!()
}

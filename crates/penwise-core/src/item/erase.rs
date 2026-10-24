//! Precise-eraser masks for stamp strokes and shapes (editing-algorithms.md §2).
//!
//! Owner: slice b.
//!
//! One erase item per (gesture, target). A mask acts only on its target (destination-out inside the target's
//! layer), never on "everything below", so ink drawn later over the area stays and the mask follows its target
//! through transforms. Fills are never masked: precise erase cuts their polygons instead (decided).

use super::affine::Affine;
use super::id::ItemId;

/// Capsule-chain mask in the **target's local frame** (pre-transform): circles of `radius` along `path`,
/// joined. Immutable after `Add`; undo of a mask gesture deletes the erase items. The erase item's own
/// `Item::transform` is identity and ignored; its `layer`/`z` copy the target's at creation.
#[derive(Debug, Clone, PartialEq)]
pub struct EraseItem {
    pub target: ItemId,
    /// Local-frame centreline on the 1/16 px grid (codec stores grid deltas), ≥ 1 point.
    pub path: Vec<[f32; 2]>,
    /// Local-frame radius `> 0`.
    pub radius: f32,
}

impl EraseItem {
    /// Mask in page space under the target's transform: `path` mapped point by point, radius ×
    /// [`Affine::width_scale`] (a non-uniform target scale renders the mask with that averaged radius).
    /// Returns `(page_path, page_radius)`.
    #[must_use]
    pub fn page_mask(&self, target_transform: &Affine) -> (Vec<[f32; 2]>, f32) {
        todo!()
    }
}

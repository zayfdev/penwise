//! Stroke eraser: removes whole items the eraser path touches (PLAN "Eraser").
//!
//! Owner: slice b.

use super::Edit;
use crate::item::{ItemId, LayerSet, Scene};

/// `Delete` for every live item on `layers` the eraser touches, sorted by `(z, id)`. The eraser is the capsule
/// chain of radius `radius > 0` along `path` (page px; one point = a disc). Touch rules:
/// - stroke: some eraser segment and some visible centreline segment satisfy
///   `segment_segment_dist ≤ radius + h` (`h` = the larger half-width of the two centreline ends);
///   points inside the stroke's masks do not count;
/// - shape: eraser within `radius + brush_size_max / 2` of the flattened path (masks respected), or any eraser
///   point inside its fill;
/// - fill: eraser within `radius` of a polygon **ring** only (scribbling over a fill's interior to erase ink
///   on top must not delete the fill);
/// - text: eraser within `radius` of the box quad or inside it;
/// - erase items are never deleted directly (they are inert while their target is dead).
///
/// May be called per path chunk during a gesture (overlap the previous chunk by one point); apply the edits
/// between calls, and group the whole gesture as one undo step. Empty path or `radius ≤ 0` → empty.
#[must_use]
pub fn erase_strokes(scene: &Scene, path: &[[f32; 2]], radius: f32, layers: LayerSet) -> Vec<Edit> {
    todo!()
}

/// The touch rule above for one live item (false for unknown, dead or erase items).
#[must_use]
pub fn path_touches(scene: &Scene, id: ItemId, path: &[[f32; 2]], radius: f32) -> bool {
    todo!()
}

//! Shape creation drag, resize about the opposite handle, endpoint drags, and the resulting edits
//! (editing-algorithms.md §5 "Resize about an anchor").
//!
//! Owner: slice d.
//!
//! All results carry absolute values and are emitted once at gesture end (`Update { shape_size, transform }`);
//! live drags preview with the same functions.

use super::handles::{Handle, HandleFrame};
use crate::edit::Edit;
use crate::item::{Affine, ItemId, Scene};

/// Frame of a new shape dragged from `start` to `end` (page px, axis-aligned): centre at the midpoint, size
/// `|end − start|`. With `constrain`, both sides become `max(|dx|, |dy|)` keeping the drag direction's signs
/// (square/circle). Linear kinds: the frame spans `start → end` (rotation from `libm::atan2f`, `size =
/// [length, 0]`), and `constrain` snaps the angle with [`crate::edit::transform::snap_angle`] at 15° steps
/// (always, not only within the tolerance).
#[must_use]
pub fn create_frame(start: [f32; 2], end: [f32; 2], linear: bool, constrain: bool) -> HandleFrame {
    todo!()
}

/// Resize `frame` by dragging resize handle `handle` to page point `pointer`, about the opposite handle:
/// 1. `q = T⁻¹ · pointer` (local); anchor = opposite corner / edge midpoint in local coords;
/// 2. `s_x = (q.x − anchor.x) / (handle.x − anchor.x)`, `s_y` likewise; edge handles keep the other axis at 1;
/// 3. `constrain`: `s_x = s_y = sign · max(|s_x|, |s_y|)` (edge handles scale both axes);
/// 4. the new frame keeps the anchor fixed in page space: new `size = [|s_x|·w, |s_y|·h]`, the transform gains
///    the anchor-preserving translation and a mirror for negative factors (`Tr(anchor)·S(sign)·Tr(−anchor)`
///    composed in local space).
///
/// Non-resize handles (`Rotate`, `Body`, `Start`, `End`) and non-invertible frames return `frame` unchanged.
/// Zero-extent axes stay 0 (the factor is ignored for them).
#[must_use]
pub fn resize_frame(
    frame: &HandleFrame,
    handle: Handle,
    pointer: [f32; 2],
    constrain: bool,
) -> HandleFrame {
    todo!()
}

/// Line/arrow endpoint drag: the moved endpoint follows `pointer`, the other stays; with `constrain`, the
/// segment angle snaps to 15° steps. Non-endpoint handles return `frame` unchanged.
#[must_use]
pub fn drag_endpoint(
    frame: &HandleFrame,
    handle: Handle,
    pointer: [f32; 2],
    constrain: bool,
) -> HandleFrame {
    todo!()
}

/// The affine `D` with `D ∘ old.box = new.box`: `new.transform · S(new.size / old.size) · old.transform⁻¹`
/// (axes with zero old extent use factor 1). Group resize applies it to every selected item with
/// [`crate::edit::transform::transform_items`]. Identity when `old` is not invertible.
#[must_use]
pub fn frame_delta(old: &HandleFrame, new: &HandleFrame) -> Affine {
    todo!()
}

/// Edit committing a single shape's new frame: `Update { id, shape_size: Some(new.size), transform:
/// Some(new.transform) }` with unchanged fields omitted (empty `Vec` when nothing changed or `id` is not a
/// live shape).
#[must_use]
pub fn resize_edits(scene: &Scene, id: ItemId, new: &HandleFrame) -> Vec<Edit> {
    todo!()
}

//! What one bridge report changes for a frame's marks: the ink layer repaints only when a mark can have moved.

use super::{FrameLayoutUpdate, FrameLayouts};
use crate::item::{FrameExtent, FramePreset, ItemId, ItemKind, Scene};

/// The outcome of [`merge_layout`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayoutChange {
    /// The stored layout differs from before.
    pub changed: bool,
    /// A mark may have moved, appeared, vanished or changed its look: renderers repaint.
    pub moves_marks: bool,
}

/// Merges `update` into `frame`'s layout ([`FrameLayouts::merge`]) and says whether its marks can have moved.
///
/// The page scroll alone moves none of a full-length frame's marks: they sit at document points (`view_origin` is
/// `[0, 0]`) and only its live window follows the scroll. A viewport frame's marks scroll with the page, and any
/// other field (preset, document size, viewport, state, a rect) can move or restyle marks of either extent.
pub fn merge_layout(
    scene: &Scene,
    layouts: &mut FrameLayouts,
    frame: ItemId,
    fallback: FramePreset,
    update: &FrameLayoutUpdate,
) -> LayoutChange {
    let before = layouts.get(frame).cloned();
    let changed = layouts.merge(frame, fallback, update);
    let full = matches!(
        scene.get(frame).map(|item| &item.kind),
        Some(ItemKind::Frame(item)) if item.extent == FrameExtent::Full
    );
    let scroll_only = match (before, layouts.get(frame)) {
        (Some(before), Some(after)) => {
            before.preset == after.preset
                && same(before.doc, after.doc)
                && same(before.viewport, after.viewport)
                && before.state == after.state
                && before.rects == after.rects
        }
        _ => false,
    };
    LayoutChange {
        changed,
        moves_marks: changed && !(full && scroll_only),
    }
}

/// The stored value is unchanged, bit for bit (it was replaced, not recomputed).
fn same(a: [f32; 2], b: [f32; 2]) -> bool {
    a.map(f32::to_bits) == b.map(f32::to_bits)
}

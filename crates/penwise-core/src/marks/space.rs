//! Frame space conversions: where a frame's live window sits on the board, and board ↔ document points.
//!
//! A frame-local coordinate is a **document** coordinate. The *view origin* is the document point shown at the
//! box's top-left: `[0, 0]` for a full-length frame, the page scroll for a viewport frame. A document point `p`
//! sits at `pos + p − view_origin` on the board. The live view (one WebView/iframe, always `css_w × css_h`) sits
//! at `pos + scroll − view_origin`, which is the page's actual scroll and never the camera's wish, so the
//! content inside it is at its true board position.

use super::{FrameLayout, FrameLayouts, FrameRect, FrameState};
use crate::item::{ItemId, Scene};

/// The live window of `frame` in board coords: `pos + scroll − view_origin`, sized `css_w × css_h`.
#[must_use]
pub fn live_window(frame: &FrameState, layout: &FrameLayout) -> FrameRect {
    let view = frame.view_origin(layout.scroll);
    FrameRect::new(
        frame.pos[0] + layout.scroll[0] - view[0],
        frame.pos[1] + layout.scroll[1] - view[1],
        frame.size[0],
        frame.size[1],
    )
}

/// The document point under board point `board`: `board − pos + view_origin`. Used for hit requests and the
/// server's crops.
#[must_use]
pub fn doc_point(frame: &FrameState, layout: &FrameLayout, board: [f32; 2]) -> [f32; 2] {
    let view = frame.view_origin(layout.scroll);
    [
        board[0] - frame.pos[0] + view[0],
        board[1] - frame.pos[1] + view[1],
    ]
}

/// The board point of document point `doc` (inverse of [`doc_point`]): `pos + doc − view_origin`.
#[must_use]
pub fn board_point(frame: &FrameState, layout: &FrameLayout, doc: [f32; 2]) -> [f32; 2] {
    let view = frame.view_origin(layout.scroll);
    [
        frame.pos[0] + doc[0] - view[0],
        frame.pos[1] + doc[1] - view[1],
    ]
}

/// The box `[w, h]` of frame `frame` of `scene` under the bridge `layouts`: `FrameItem::box_size` with the layout's
/// document height, the same function [`Placement`](super::Placement) clips with. `None` when `frame` is not a
/// live frame item.
#[must_use]
pub fn frame_box_size(scene: &Scene, layouts: &FrameLayouts, frame: ItemId) -> Option<[f32; 2]> {
    FrameState::of(scene, layouts, frame).map(|state| state.box_size())
}

/// [`live_window`] for frame `frame` of `scene` under the bridge `layouts` (no layout yet = scroll `[0, 0]`);
/// `None` when `frame` is not a live frame item.
#[must_use]
pub fn frame_live_window(
    scene: &Scene,
    layouts: &FrameLayouts,
    frame: ItemId,
) -> Option<FrameRect> {
    let (state, layout) = state_and_layout(scene, layouts, frame)?;
    Some(live_window(&state, &layout))
}

/// [`doc_point`] for frame `frame` of `scene`; `None` when `frame` is not a live frame item.
#[must_use]
pub fn frame_doc_point(
    scene: &Scene,
    layouts: &FrameLayouts,
    frame: ItemId,
    board: [f32; 2],
) -> Option<[f32; 2]> {
    let (state, layout) = state_and_layout(scene, layouts, frame)?;
    Some(doc_point(&state, &layout, board))
}

/// [`board_point`] for frame `frame` of `scene`; `None` when `frame` is not a live frame item.
#[must_use]
pub fn frame_board_point(
    scene: &Scene,
    layouts: &FrameLayouts,
    frame: ItemId,
    doc: [f32; 2],
) -> Option<[f32; 2]> {
    let (state, layout) = state_and_layout(scene, layouts, frame)?;
    Some(board_point(&state, &layout, doc))
}

fn state_and_layout(
    scene: &Scene,
    layouts: &FrameLayouts,
    frame: ItemId,
) -> Option<(FrameState, FrameLayout)> {
    let state = FrameState::of(scene, layouts, frame)?;
    let layout = state
        .layout
        .clone()
        .unwrap_or_else(|| FrameLayout::new(state.preset));
    Some((state, layout))
}

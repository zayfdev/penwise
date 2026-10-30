//! Frame space for the clients: the live window and board ↔ document points (design harness v2), straight from
//! `penwise_core::marks` under the frame's layout. Unknown frames answer `None`.

use penwise_core::item::ItemId;
use penwise_core::marks::{FrameRect, frame_board_point, frame_doc_point, frame_live_window};

use super::SceneHandle;

impl SceneHandle {
    /// Where frame `frame`'s live view sits on the board, `[x, y, w, h]` (board coords).
    #[must_use]
    pub fn frame_live_window(&self, frame: ItemId) -> Option<[f32; 4]> {
        let state = self.marks.borrow();
        frame_live_window(&self.scene, &state.layouts, frame).map(FrameRect::array)
    }

    /// The document point of frame `frame` under board point `board`.
    #[must_use]
    pub fn frame_doc_point(&self, frame: ItemId, board: [f32; 2]) -> Option<[f32; 2]> {
        let state = self.marks.borrow();
        frame_doc_point(&self.scene, &state.layouts, frame, board)
    }

    /// The board point of document point `doc` of frame `frame` (inverse of [`SceneHandle::frame_doc_point`]).
    #[must_use]
    pub fn frame_board_point(&self, frame: ItemId, doc: [f32; 2]) -> Option<[f32; 2]> {
        let state = self.marks.borrow();
        frame_board_point(&self.scene, &state.layouts, frame, doc)
    }
}

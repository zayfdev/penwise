//! Frame space for the clients: the live window and board ↔ document points (design harness v2), straight from
//! `penwise_core::marks` under the frame's layout. Unknown frames answer `None`.

use penwise_core::Bounds;
use penwise_core::item::ItemId;
use penwise_core::item::LayerSet;
use penwise_core::marks::{
    FrameRect, MarkView, frame_board_point, frame_doc_point, frame_live_window,
};

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

    /// `[min_x, min_y, max_x, max_y]` of everything rendered on `layers` (frames at their boxes under the current
    /// layouts, pictures, ink): what a `Focus` for the page's content fits. `None` for an empty page. The same
    /// rule as the viewer's `content_bounds`.
    #[must_use]
    pub fn content_bounds(&self, layers: LayerSet) -> Option<[f32; 4]> {
        let state = self.marks.borrow();
        let view = MarkView::new(&self.scene, &state.layouts, state.show_done);
        let presentation = view.presentation();
        let scene = &presentation.scene;
        let bounds = scene
            .render_order(layers)
            .into_iter()
            .filter_map(|id| scene.bounds(id))
            .fold(Bounds::EMPTY, |acc, b| acc.union(&b));
        (!bounds.is_empty()).then_some([bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y])
    }
}

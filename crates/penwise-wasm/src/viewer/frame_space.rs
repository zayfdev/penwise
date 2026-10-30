//! Frame space in the browser (design harness v2): the live window and board ↔ document points, straight from
//! `penwise_core::marks` under the frame's layout. Each answers an empty array for an unknown frame.

use penwise_core::marks::{frame_board_point, frame_doc_point, frame_live_window};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use super::Viewer;
use super::frames::parse_item;

#[wasm_bindgen]
impl Viewer {
    /// `[x, y, w, h]` of frame `frame`'s live window in board coords (`pos + scroll − view origin`, sized
    /// `css_w × css_h`); empty when `frame` is not a frame.
    ///
    /// # Errors
    /// `JsError` for a malformed frame id.
    pub fn frame_live_window(&self, frame: &str) -> Result<Vec<f32>, JsError> {
        let id = parse_item(frame)?;
        let Some((page, _)) = self.frame(id) else {
            return Ok(Vec::new());
        };
        let Some(replica) = self.replica.page(page) else {
            return Ok(Vec::new());
        };
        Ok(frame_live_window(replica.scene(), &self.layouts, id)
            .map_or_else(Vec::new, |rect| rect.array().to_vec()))
    }

    /// `[x, y]` document point of frame `frame` under board point `(x, y)`; empty when `frame` is not a frame.
    ///
    /// # Errors
    /// `JsError` for a malformed frame id.
    pub fn frame_doc_point(&self, frame: &str, x: f32, y: f32) -> Result<Vec<f32>, JsError> {
        let id = parse_item(frame)?;
        let Some((page, _)) = self.frame(id) else {
            return Ok(Vec::new());
        };
        let Some(replica) = self.replica.page(page) else {
            return Ok(Vec::new());
        };
        Ok(frame_doc_point(replica.scene(), &self.layouts, id, [x, y])
            .map_or_else(Vec::new, |p| p.to_vec()))
    }

    /// `[x, y]` board point of document point `(x, y)` of frame `frame`; empty when `frame` is not a frame.
    ///
    /// # Errors
    /// `JsError` for a malformed frame id.
    pub fn frame_board_point(&self, frame: &str, x: f32, y: f32) -> Result<Vec<f32>, JsError> {
        let id = parse_item(frame)?;
        let Some((page, _)) = self.frame(id) else {
            return Ok(Vec::new());
        };
        let Some(replica) = self.replica.page(page) else {
            return Ok(Vec::new());
        };
        Ok(
            frame_board_point(replica.scene(), &self.layouts, id, [x, y])
                .map_or_else(Vec::new, |p| p.to_vec()),
        )
    }
}

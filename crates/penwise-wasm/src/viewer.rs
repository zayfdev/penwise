//! `Viewer`: the browser's replica (proto [`Replica`] + [`LiveStrokes`]) and geometry export.
//!
//! Owner: slice d.
//!
//! `receive` dispatch (binding glue only; rules live in proto):
//! - `OpsSince` → `Replica::apply_seq` per op (errors counted in `apply_errors`); every applied item `Add`
//!   also calls `LiveStrokes::commit(item id)`.
//! - `StrokeDelta`/`StrokeCancel` → `LiveStrokes::apply_delta`/`cancel` (errors ignored).
//! - `Resync` → `LiveStrokes::clear`. Other kinds only fill the [`Received`] summary.

use penwise_proto::{LiveStrokes, Replica};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use crate::draw_list::DrawList;
use crate::received::{NotebookEntry, PageEntry, Received};

/// Browser-side state for every page.
#[wasm_bindgen]
#[derive(Debug)]
pub struct Viewer {
    replica: Replica,
    live: LiveStrokes,
}

#[wasm_bindgen]
impl Viewer {
    /// Empty viewer (`head_seq = 0`).
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    /// Decodes one server frame (`decode_server`) and applies it (module dispatch).
    ///
    /// # Errors
    /// `JsError` with the `ProtoError` text when the frame does not decode (caller logs and drops it).
    pub fn receive(&mut self, frame: &[u8]) -> Result<Received, JsError> {
        todo!()
    }

    /// Highest applied seq: the `last_seen_seq` for the next `Hello`.
    #[must_use]
    pub fn head_seq(&self) -> u64 {
        todo!()
    }

    /// Committed geometry of `page` on `layers` (core `LayerSet::bits`; 0b111 = all).
    ///
    /// # Errors
    /// `JsError` for a malformed page id. Unknown page → empty list.
    pub fn render_page_paths(&self, page: &str, layers: u8) -> Result<DrawList, JsError> {
        todo!()
    }

    /// Live (in-progress) strokes on `page`, ascending stroke id.
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn render_live_paths(&self, page: &str) -> Result<DrawList, JsError> {
        todo!()
    }

    /// Union of live item bounds on `layers` as `[min_x, min_y, max_x, max_y]`; `None` when empty or unknown
    /// (infinite-page fit).
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn content_bounds(&self, page: &str, layers: u8) -> Result<Option<Vec<f32>>, JsError> {
        todo!()
    }

    /// Live notebooks from the local catalog, ascending id.
    #[must_use]
    pub fn notebooks(&self) -> Vec<NotebookEntry> {
        todo!()
    }

    /// Live pages of `notebook` from the local catalog, ascending `(position, id)`.
    ///
    /// # Errors
    /// `JsError` for a malformed notebook id.
    pub fn pages(&self, notebook: &str) -> Result<Vec<PageEntry>, JsError> {
        todo!()
    }

    /// One page from the local catalog (`None` if unknown).
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn page(&self, page: &str) -> Result<Option<PageEntry>, JsError> {
        todo!()
    }

    /// Drops every live stroke (socket closed).
    pub fn clear_live(&mut self) {
        todo!()
    }
}

impl Default for Viewer {
    fn default() -> Self {
        Self::new()
    }
}

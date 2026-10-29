//! `Viewer`: the browser's replica (proto [`Replica`] + [`LiveStrokes`]) and geometry export.
//!
//! Binding glue only: the dispatch rules live in [`dispatch`], sync rules in proto. Methods taking JS strings
//! parse ids first (`JsError` on a malformed id) and then call a typed method that cannot fail.

use std::str::FromStr;

use penwise_core::item::LayerSet;
use penwise_core::marks::{FrameLayouts, MarkView};
use penwise_proto::{LiveStrokes, NotebookId, PageId, Replica, Scope};
use wasm_bindgen::prelude::{JsError, wasm_bindgen};

use crate::draw_list::DrawList;
use crate::error::WasmError;
use crate::received::{NotebookEntry, PageEntry, Received};

mod author;
mod dispatch;
mod edits;
mod frames;
mod marks;
mod pen;
mod persist;
mod session;

use author::Author;

/// Browser-side state for every page.
#[wasm_bindgen]
#[derive(Debug, Default)]
pub struct Viewer {
    replica: Replica,
    live: LiveStrokes,
    /// Scope of the snapshot in progress (`None` between snapshots).
    snapshot_scope: Option<Scope>,
    /// Device identity, clock and outbox (`None` until [`Viewer::identify`]); a pure viewer never needs it.
    author: Option<Author>,
    /// Client-local frame layouts from the bridge (never synced).
    layouts: FrameLayouts,
    /// The local show-done view flag.
    show_done: bool,
}

#[wasm_bindgen]
impl Viewer {
    /// Empty viewer (`head_seq = 0`).
    #[wasm_bindgen(constructor)]
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Decodes one server frame (`decode_server`) and applies it (see [`dispatch`]).
    ///
    /// # Errors
    /// `JsError` with the `ProtoError` text when the frame does not decode (caller logs and drops it).
    pub fn receive(&mut self, frame: &[u8]) -> Result<Received, JsError> {
        Ok(self.apply_frame(frame)?)
    }

    /// Highest applied seq: the `last_seen_seq` for the next `Hello`.
    #[must_use]
    pub fn head_seq(&self) -> u64 {
        self.replica.head_seq()
    }

    /// Committed geometry of `page` on `layers` (core `LayerSet::bits`; 0b111 = all).
    ///
    /// # Errors
    /// `JsError` for a malformed page id. Unknown page → empty list.
    pub fn render_page_paths(&self, page: &str, layers: u8) -> Result<DrawList, JsError> {
        Ok(self.draw_page(parse_id(page)?, LayerSet { bits: layers }))
    }

    /// Live (in-progress) strokes on `page`, ascending stroke id.
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn render_live_paths(&self, page: &str) -> Result<DrawList, JsError> {
        Ok(self.draw_live(parse_id(page)?))
    }

    /// Union of item bounds on `layers` as `[min_x, min_y, max_x, max_y]`; `None` when empty or unknown
    /// (infinite-page fit).
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn content_bounds(&self, page: &str, layers: u8) -> Result<Option<Vec<f32>>, JsError> {
        Ok(self.bounds_of(parse_id(page)?, LayerSet { bits: layers }))
    }

    /// Live notebooks from the local catalog, ascending `(position, id)`.
    #[must_use]
    pub fn notebooks(&self) -> Vec<NotebookEntry> {
        let catalog = self.replica.catalog();
        catalog
            .notebooks()
            .iter()
            .map(|n| NotebookEntry::from_notebook(n, catalog.pages(n.id).len()))
            .collect()
    }

    /// Live pages of `notebook` from the local catalog, ascending `(position, id)`.
    ///
    /// # Errors
    /// `JsError` for a malformed notebook id.
    pub fn pages(&self, notebook: &str) -> Result<Vec<PageEntry>, JsError> {
        let notebook: NotebookId = parse_id(notebook)?;
        Ok(self
            .replica
            .catalog()
            .pages(notebook)
            .iter()
            .map(PageEntry::from_page)
            .collect())
    }

    /// One live page from the local catalog (`None` if unknown or deleted).
    ///
    /// # Errors
    /// `JsError` for a malformed page id.
    pub fn page(&self, page: &str) -> Result<Option<PageEntry>, JsError> {
        let page: PageId = parse_id(page)?;
        Ok(self
            .replica
            .catalog()
            .page(page)
            .filter(|p| !p.deleted)
            .map(|p| PageEntry::from_page(&p)))
    }

    /// Drops every live stroke (socket closed).
    pub fn clear_live(&mut self) {
        self.live.clear();
    }
}

impl Viewer {
    /// Committed geometry of `page` on `layers` in the effective presentation (marks placed, hidden strokes
    /// gone, dimmed strokes at 30% opacity); unknown page → empty list.
    #[must_use]
    pub fn draw_page(&self, page: PageId, layers: LayerSet) -> DrawList {
        let Some(replica) = self.replica.page(page) else {
            return DrawList::default();
        };
        let view = MarkView::new(replica.scene(), &self.layouts, self.show_done);
        if view.has_marks() {
            let presentation = view.presentation();
            DrawList::from_scene_dim(&presentation.scene, layers, &presentation.dim)
        } else {
            DrawList::from_scene(replica.scene(), layers)
        }
    }

    /// Live strokes of `page`, ascending stroke id.
    #[must_use]
    pub fn draw_live(&self, page: PageId) -> DrawList {
        let mut list = DrawList::default();
        for (_, stroke) in self.live.on_page(page) {
            list.push_live(&stroke.geometry());
        }
        list
    }

    /// `[min_x, min_y, max_x, max_y]` of the rendered items of `page` on `layers`.
    #[must_use]
    pub fn bounds_of(&self, page: PageId, layers: LayerSet) -> Option<Vec<f32>> {
        let replica = self.replica.page(page)?;
        let view = MarkView::new(replica.scene(), &self.layouts, self.show_done);
        let presentation = view.presentation();
        let scene = &presentation.scene;
        let bounds = scene
            .render_order(layers)
            .into_iter()
            .filter_map(|id| scene.bounds(id))
            .fold(penwise_core::Bounds::EMPTY, |acc, b| acc.union(&b));
        (!bounds.is_empty()).then(|| vec![bounds.min_x, bounds.min_y, bounds.max_x, bounds.max_y])
    }
}

/// Parses a 26-char id text.
///
/// # Errors
/// [`WasmError::Id`] for anything else.
pub fn parse_id<T: FromStr<Err = penwise_proto::IdParseError>>(text: &str) -> Result<T, WasmError> {
    Ok(text.parse()?)
}

/// JS clocks are `f64` ms; NaN/negative → 0, fraction dropped.
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "clamped non-negative; saturating cast"
)]
pub(crate) fn whole_ms(now_ms: f64) -> u64 {
    if now_ms.is_finite() && now_ms > 0.0 {
        now_ms as u64
    } else {
        0
    }
}

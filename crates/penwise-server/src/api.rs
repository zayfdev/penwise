//! Token-gated HTTP API: page PNG and item listing (the e2e "stroke arrived at the server" check, PLAN
//! "Verification harness") plus an unauthenticated health probe.
//!
//! Owner: slice a.
//!
//! Routes (all except `/api/health` behind `pairing::token::require_bearer`; responses `Cache-Control:
//! no-store`):
//! - `GET /api/health` → `200 {"ok":true,"protocol":<PROTOCOL_VERSION>}` (no server id, no counts).
//! - `GET /api/pages/{page}/png?scale=<f32>&layers=<u8 bits>&background=<bool>` → `image/png` via
//!   `render::render_page_png` (`PageReplica::from_items(storage.page_items(page))`), 404 unknown page, 400 bad
//!   query (`RenderError::InvalidOptions`), 413 `TooLarge`.
//! - `GET /api/pages/{page}/items` → JSON [`ItemsResponse`].
//!
//! `{page}` is the 26-char id text form. Storage and render calls run in `spawn_blocking`.

use axum::Router;
use serde::{Deserialize, Serialize};

use crate::app::App;

/// Query of the PNG route (every field optional; defaults = `RenderOptions::whole_page`).
#[derive(Debug, Clone, Copy, PartialEq, Deserialize)]
pub struct PngQuery {
    pub scale: Option<f32>,
    /// `LayerSet::bits`.
    pub layers: Option<u8>,
    pub background: Option<bool>,
}

/// One item in [`ItemsResponse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemJson {
    /// Id text form.
    pub id: String,
    /// `stroke` | `erase` | `shape` | `fill` | `text`.
    pub kind: &'static str,
    pub deleted: bool,
    pub updated_seq: u64,
}

/// `GET /api/pages/{page}/items` body.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ItemsResponse {
    pub page: String,
    pub head_seq: u64,
    /// Ascending id, tombstones included.
    pub items: Vec<ItemJson>,
}

/// The `/api` routes (bearer layer applied here).
pub fn routes(app: &App) -> Router<App> {
    todo!()
}

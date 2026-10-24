//! JS-facing summary of one received server frame (what the UI must react to).
//!
//! Owner: slice d.

use penwise_proto::{NotebookInfo, Page, PageInfo, RejectReason};
use wasm_bindgen::prelude::wasm_bindgen;

/// Which `ServerMsg` arrived (JS enum; codes = `ServerMsg` variant order, frozen).
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Welcome = 0,
    Reject = 1,
    Paired = 2,
    Ack = 3,
    OpsSince = 4,
    StrokeDelta = 5,
    StrokeCancel = 6,
    SwitchPage = 7,
    NotebookList = 8,
    PageList = 9,
    Resync = 10,
    Ping = 11,
    Pong = 12,
}

/// Notebook row for the UI.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NotebookEntry {
    pub id: String,
    pub title: String,
    /// Live pages (`0` when built from the local catalog without a server snapshot).
    pub page_count: u32,
}

/// Page row for the UI.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq)]
pub struct PageEntry {
    pub id: String,
    pub notebook: String,
    /// Sort key (exact up to 2⁵³).
    pub position: f64,
    /// `false` = infinite.
    pub fixed: bool,
    /// Page px (`0` for infinite).
    pub width: f32,
    pub height: f32,
    /// proto `Background::code` (0 blank, 1 lined, 2 grid, 3 dot).
    pub background: u8,
}

/// Summary of one frame after the viewer applied it. Fields not relevant to `kind` are empty/zero.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq)]
pub struct Received {
    pub kind: MessageKind,
    /// `Welcome`/`OpsSince`: server head; else the replica's head after applying.
    pub head_seq: u64,
    /// `OpsSince`: another catch-up batch follows.
    pub more: bool,
    /// `Welcome.current_page`, `SwitchPage.page`.
    pub page: Option<String>,
    /// `SwitchPage.by`, live-ink producer.
    pub device: Option<String>,
    /// Pages whose committed scene changed (re-render).
    pub changed_pages: Vec<String>,
    /// Pages whose live preview changed (re-render the live layer only).
    pub live_pages: Vec<String>,
    /// Notebook/page lists changed (refresh `ui/pages.ts`).
    pub catalog_changed: bool,
    /// `Paired.token` (store it; never log it).
    pub token: Option<String>,
    /// `Welcome.server` / `Paired.server`.
    pub server: Option<String>,
    /// `Reject` reason: `bad_token` | `unsupported_protocol` | `bad_pairing_code` | `malformed` |
    /// `rate_limited`.
    pub reject: Option<String>,
    /// `Ping`/`Pong` nonce (answer a `Ping` with `encode_pong(nonce)`).
    pub nonce: u64,
    /// `PageList.notebook`.
    pub notebook: Option<String>,
    /// `NotebookList` snapshot.
    pub notebooks: Vec<NotebookEntry>,
    /// `PageList` snapshot.
    pub pages: Vec<PageEntry>,
    /// Ops in this frame the replica refused (logged to the console by the caller; never fatal).
    pub apply_errors: u32,
}

impl NotebookEntry {
    /// From a server snapshot row.
    #[must_use]
    pub fn from_info(info: &NotebookInfo) -> Self {
        todo!()
    }
}

impl PageEntry {
    /// From a server snapshot row.
    #[must_use]
    pub fn from_info(info: &PageInfo) -> Self {
        todo!()
    }

    /// From the local catalog.
    #[must_use]
    pub fn from_page(page: &Page) -> Self {
        todo!()
    }
}

impl Received {
    /// Empty summary of `kind`.
    #[must_use]
    pub fn empty(kind: MessageKind) -> Self {
        todo!()
    }
}

/// Stable snake_case name of a reject reason (the `Received::reject` strings).
#[must_use]
pub fn reject_name(reason: RejectReason) -> &'static str {
    todo!()
}

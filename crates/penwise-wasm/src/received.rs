//! JS-facing summary of one received server frame (what the UI must react to) and the catalog rows the
//! viewer exposes.

use penwise_proto::{ErrorCode, Notebook, Page, PageKind, RejectReason, ServerMsg};
use wasm_bindgen::prelude::wasm_bindgen;

/// Which `ServerMsg` arrived (JS enum; codes = `ServerMsg` variant order, frozen).
#[wasm_bindgen]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MessageKind {
    Welcome = 0,
    Reject = 1,
    Ack = 2,
    OpsSince = 3,
    SnapshotBegin = 4,
    SnapshotChunk = 5,
    SnapshotEnd = 6,
    StrokeDelta = 7,
    StrokeCancel = 8,
    SwitchPage = 9,
    Resync = 10,
    Error = 11,
    Ping = 12,
    Pong = 13,
}

/// Notebook row for the UI.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq)]
pub struct NotebookEntry {
    pub id: String,
    pub title: String,
    /// Sort key (exact up to 2⁵³).
    pub position: f64,
    /// Live pages in the local catalog.
    pub page_count: u32,
}

/// Page row for the UI.
#[wasm_bindgen(getter_with_clone)]
#[derive(Debug, Clone, PartialEq)]
pub struct PageEntry {
    pub id: String,
    pub notebook: String,
    /// May be empty (untitled).
    pub title: String,
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
    /// `SwitchPage.by`, `StrokeDelta`/`StrokeCancel` producer.
    pub device: Option<String>,
    /// Pages whose committed scene changed (re-render).
    pub changed_pages: Vec<String>,
    /// Pages whose live preview changed (re-render the live layer only).
    pub live_pages: Vec<String>,
    /// `SnapshotBegin`: `"all"`, `"catalog"` or the page id the snapshot replaces.
    pub scope: Option<String>,
    /// Notebook/page lists changed (refresh the page list).
    pub catalog_changed: bool,
    /// `Welcome.server`.
    pub server: Option<String>,
    /// `Reject` reason: `bad_token` | `unsupported_protocol` | `malformed` | `rate_limited`.
    pub reject: Option<String>,
    /// `Error` code: `malformed` | `too_large` | `internal` | `shutting_down`.
    pub error: Option<String>,
    /// `Error.detail` (server text, for the console).
    pub detail: Option<String>,
    /// `Ping`/`Pong` nonce (answer a `Ping` with `encode_pong(nonce)`).
    pub nonce: u64,
    /// `Ack`: this client's ops the server logged / refused (`PushOps` gate re-opened).
    pub acked: u32,
    pub rejected: u32,
    /// `Ack` refused ops: the replica was rebuilt without them and is empty (head 0); reconnect to refill it.
    pub resync: bool,
    /// Ops/snapshot rows in this frame the replica refused (logged to the console by the caller; never fatal).
    pub apply_errors: u32,
}

impl MessageKind {
    /// The kind of `msg`.
    #[must_use]
    pub fn of(msg: &ServerMsg) -> Self {
        match msg {
            ServerMsg::Welcome { .. } => Self::Welcome,
            ServerMsg::Reject { .. } => Self::Reject,
            ServerMsg::Ack { .. } => Self::Ack,
            ServerMsg::OpsSince { .. } => Self::OpsSince,
            ServerMsg::SnapshotBegin { .. } => Self::SnapshotBegin,
            ServerMsg::SnapshotChunk(_) => Self::SnapshotChunk,
            ServerMsg::SnapshotEnd { .. } => Self::SnapshotEnd,
            ServerMsg::StrokeDelta { .. } => Self::StrokeDelta,
            ServerMsg::StrokeCancel { .. } => Self::StrokeCancel,
            ServerMsg::SwitchPage { .. } => Self::SwitchPage,
            ServerMsg::Resync { .. } => Self::Resync,
            ServerMsg::Error { .. } => Self::Error,
            ServerMsg::Ping { .. } => Self::Ping,
            ServerMsg::Pong { .. } => Self::Pong,
        }
    }
}

impl NotebookEntry {
    /// From the local catalog; `page_count` = its live pages.
    #[must_use]
    pub fn from_notebook(notebook: &Notebook, page_count: usize) -> Self {
        Self {
            id: notebook.id.to_string(),
            title: notebook.title.clone(),
            position: position_f64(notebook.position),
            page_count: u32::try_from(page_count).unwrap_or(u32::MAX),
        }
    }
}

impl PageEntry {
    /// From the local catalog.
    #[must_use]
    pub fn from_page(page: &Page) -> Self {
        let (fixed, width, height) = match page.kind {
            PageKind::Fixed { width, height } => (true, width, height),
            PageKind::Infinite => (false, 0.0, 0.0),
        };
        Self {
            id: page.id.to_string(),
            notebook: page.notebook.to_string(),
            title: page.title.clone(),
            position: position_f64(page.position),
            fixed,
            width,
            height,
            background: page.background.code(),
        }
    }
}

impl Received {
    /// Empty summary of `kind`.
    #[must_use]
    pub fn empty(kind: MessageKind) -> Self {
        Self {
            kind,
            head_seq: 0,
            more: false,
            page: None,
            device: None,
            changed_pages: Vec::new(),
            live_pages: Vec::new(),
            catalog_changed: false,
            scope: None,
            server: None,
            reject: None,
            error: None,
            detail: None,
            nonce: 0,
            acked: 0,
            rejected: 0,
            resync: false,
            apply_errors: 0,
        }
    }
}

/// Stable `snake_case` name of a reject reason (the `Received::reject` strings).
#[must_use]
pub fn reject_name(reason: RejectReason) -> &'static str {
    match reason {
        RejectReason::BadToken => "bad_token",
        RejectReason::UnsupportedProtocol { .. } => "unsupported_protocol",
        RejectReason::Malformed => "malformed",
        RejectReason::RateLimited => "rate_limited",
    }
}

/// Stable `snake_case` name of a connection error code (the `Received::error` strings).
#[must_use]
pub fn error_name(code: ErrorCode) -> &'static str {
    match code {
        ErrorCode::Malformed => "malformed",
        ErrorCode::TooLarge => "too_large",
        ErrorCode::Internal => "internal",
        ErrorCode::ShuttingDown => "shutting_down",
    }
}

/// Sort keys cross JS as `f64` (exact up to 2⁵³; positions are `POSITION_STEP` multiples far below that).
#[allow(
    clippy::cast_precision_loss,
    reason = "documented: exact up to 2^53, positions stay far below"
)]
fn position_f64(position: i64) -> f64 {
    position as f64
}

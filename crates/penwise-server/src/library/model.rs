//! Pages' data shapes: strokes, page kinds, backgrounds, the library index and the frames that carry them.

use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

/// Default sheet width, in page px: the tablet's landscape screen, so ink maps 1:1.
pub const PAGE_WIDTH: f32 = 2560.0;
/// Default sheet height, in page px: the tablet's landscape screen, so ink maps 1:1.
pub const PAGE_HEIGHT: f32 = 1600.0;
/// Ruling / grid / dot pitch of every non-blank background (7 mm at 340 dpi).
pub const BACKGROUND_SPACING_PX: f32 = 93.7;
/// Marks a stroke as assistant-authored; cleared as a set.
pub const AGENT_LAYER: &str = "agent";
/// Longest page or notebook title, in bytes.
pub const MAX_TITLE_BYTES: usize = 256;

/// One finished stroke as the tablet sends it. The seed is a decimal string because JS numbers cannot hold a
/// 64-bit value exactly; the server treats it as opaque.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StrokeMsg {
    /// `ink` for the user's own strokes, `agent` for marks an assistant added (cleared as a set).
    #[serde(default = "default_layer")]
    pub layer: String,
    pub preset: u32,
    pub argb: i32,
    pub scale: f32,
    pub seed: String,
    /// Five floats per sample: x, y, pressure, tilt, azimuth.
    pub samples: Vec<f32>,
    /// Microseconds since the stroke started, one per sample.
    pub times: Vec<i32>,
}

fn default_layer() -> String {
    "ink".to_owned()
}

/// Page extent. Immutable after creation.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum PageKind {
    /// `[0, width] × [0, height]` in page px.
    Fixed { width: f32, height: f32 },
    /// Unbounded; the PNG uses content bounds.
    Infinite,
}

impl PageKind {
    /// The default sheet: landscape, matching how the tablet is held, so ink maps 1:1.
    #[must_use]
    pub fn landscape() -> Self {
        Self::Fixed {
            width: PAGE_WIDTH,
            height: PAGE_HEIGHT,
        }
    }
}

/// Paper pattern drawn under every layer.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Background {
    /// Plain paper.
    #[default]
    Blank,
    /// Horizontal rules every [`BACKGROUND_SPACING_PX`].
    Ruled,
    /// Square grid.
    Grid,
    /// Dot grid.
    Dots,
}

/// One page's settings (its strokes live in `pages/<id>.json`).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageMeta {
    /// `pg_` plus 16 hex characters.
    pub id: String,
    pub title: String,
    pub kind: PageKind,
    pub background: Background,
}

/// A named group of pages.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Notebook {
    /// `nb_` plus 16 hex characters.
    pub id: String,
    pub title: String,
    pub pages: Vec<PageMeta>,
}

/// The whole library index, persisted as `meta.json`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Meta {
    /// Schema version; currently 1.
    pub version: u32,
    /// Page agents address when they do not name one.
    pub active: String,
    pub notebooks: Vec<Notebook>,
}

impl Meta {
    /// The page with `id`, anywhere in the library.
    #[must_use]
    pub fn page(&self, id: &str) -> Option<&PageMeta> {
        self.notebooks
            .iter()
            .flat_map(|notebook| &notebook.pages)
            .find(|page| page.id == id)
    }

    /// The notebook holding page `id`.
    #[must_use]
    pub fn notebook_of(&self, id: &str) -> Option<&Notebook> {
        self.notebooks
            .iter()
            .find(|notebook| notebook.pages.iter().any(|page| page.id == id))
    }

    pub(crate) fn page_mut(&mut self, id: &str) -> Option<&mut PageMeta> {
        self.notebooks
            .iter_mut()
            .flat_map(|notebook| &mut notebook.pages)
            .find(|page| page.id == id)
    }

    pub(crate) fn notebook_mut(&mut self, id: &str) -> Option<&mut Notebook> {
        self.notebooks.iter_mut().find(|notebook| notebook.id == id)
    }

    /// Every page, in reading order.
    pub(crate) fn all_pages(&self) -> impl Iterator<Item = &PageMeta> {
        self.notebooks.iter().flat_map(|notebook| &notebook.pages)
    }
}

/// A fan-out frame: `page` is the page it belongs to, `None` for library-wide frames.
#[derive(Clone, Debug)]
pub struct Frame {
    /// Page the frame is about, when it is page-scoped.
    pub page: Option<String>,
    /// The JSON text sent to clients.
    pub text: String,
}

/// Request body of `POST /pages`.
#[derive(Debug, Default, Deserialize)]
pub struct NewPage {
    /// Client-chosen page id (offline-first tablets create pages before they reach the server); ignored when it
    /// already exists.
    pub id: Option<String>,
    /// Defaults to `Page N`.
    pub title: Option<String>,
    /// Defaults to A4 portrait.
    pub kind: Option<PageKind>,
    /// Defaults to blank.
    pub background: Option<Background>,
    /// Target notebook; defaults to the active page's notebook, else the first.
    pub notebook: Option<String>,
}

/// Register update for `PATCH /pages/<id>`.
#[derive(Debug, Default, Deserialize)]
pub struct PagePatch {
    /// New title.
    pub title: Option<String>,
    /// New paper pattern.
    pub background: Option<Background>,
}

/// Minimum replayable payload for a stroke: one sample of geometry.
pub(crate) fn op_frame(page: &str, op: &serde_json::Value) -> String {
    serde_json::json!({ "t": "op", "page": page, "op": op }).to_string()
}

pub(crate) fn remove_op(seeds: &[String]) -> serde_json::Value {
    serde_json::json!({ "k": "remove", "seeds": seeds })
}

pub(crate) fn replace_op(seeds: &[String], strokes: &[StrokeMsg]) -> serde_json::Value {
    serde_json::json!({ "k": "replace", "seeds": seeds, "strokes": strokes })
}

pub(crate) fn stroke_frame(page: &str, stroke: &StrokeMsg) -> String {
    serde_json::json!({ "t": "stroke", "page": page, "stroke": stroke }).to_string()
}

pub(crate) fn page_frame(page: &str, strokes: &[StrokeMsg]) -> String {
    serde_json::json!({ "t": "page", "page": page, "strokes": strokes }).to_string()
}

pub(crate) fn meta_frame_of(meta: &Meta) -> String {
    serde_json::json!({ "t": "meta", "meta": meta }).to_string()
}

pub(crate) fn active_frame(page: &str) -> String {
    serde_json::json!({ "t": "active", "page": page }).to_string()
}

/// Random id: prefix, `_`, 16 lowercase hex characters.
#[allow(
    clippy::cast_possible_truncation,
    reason = "the low 64 bits of the clock are plenty to keep ids apart"
)]
pub(crate) fn new_id(prefix: &str) -> String {
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |since| since.as_nanos());
    // Mix the clock with a per-process counter so two pages created in the same nanosecond differ.
    let bump = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mixed = nanos as u64 ^ bump.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{prefix}_{mixed:016x}")
}

/// `pg_` plus 16 lowercase hex characters, the id shape both the server and the tablet generate.
pub(crate) fn is_page_id(id: &str) -> bool {
    id.len() == 19
        && id.starts_with("pg_")
        && id[3..]
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase())
}

pub(crate) fn check_title(title: &str) -> Result<(), String> {
    if title.trim().is_empty() {
        return Err("title is empty".to_owned());
    }
    if title.len() > MAX_TITLE_BYTES {
        return Err(format!("title longer than {MAX_TITLE_BYTES} bytes"));
    }
    Ok(())
}

pub(crate) fn title_or(title: Option<String>, fallback: String) -> Result<String, String> {
    match title {
        Some(title) => {
            check_title(&title)?;
            Ok(title)
        }
        None => Ok(fallback),
    }
}

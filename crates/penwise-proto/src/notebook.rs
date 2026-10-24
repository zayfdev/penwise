//! Notebook and page model (PLAN "Data model": Notebook, Page) and the changes their ops carry.
//!
//! Owner: slice a.
//!
//! Registers (LWW by [`crate::Hlc`], see [`crate::lww`]): notebook `title`, `deleted`; page `position`,
//! `background`, `deleted`. Immutable after `Create`: a page's `notebook` and `kind`. `Create` writes every
//! register at its own stamp. Nothing is hard-deleted: `deleted` is a tombstone register (restorable).

use serde::{Deserialize, Serialize};

use crate::ids::{NotebookId, PageId};

/// Gap between page positions handed out for "append": `last position + POSITION_STEP`, so a later insert can
/// take a midpoint without renumbering (same scheme as core `Z_STEP`). Equal positions sort by [`PageId`].
pub const POSITION_STEP: i64 = 1 << 16;
/// Longest notebook title in UTF-8 bytes (longer titles are rejected, not truncated).
pub const MAX_TITLE_BYTES: usize = 256;
/// Largest fixed page side, page px (≈ 3 m at 340 dpi; bounds render buffers and fill windows).
pub const MAX_PAGE_SIDE_PX: f32 = 40_000.0;
/// Ruling / grid / dot pitch of every non-blank background, page px (7 mm at 340 dpi).
pub const BACKGROUND_SPACING_PX: f32 = 93.7;

/// Page extent (wire form of core `item::PageKind`, which carries no serde). Immutable after creation.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum PageKind {
    /// `[0, width] × [0, height]` page px; both finite, `> 0`, `≤ MAX_PAGE_SIDE_PX`.
    Fixed { width: f32, height: f32 },
    /// Unbounded canvas; renderers default to content bounds.
    Infinite,
}

/// Paper pattern drawn under every layer. Codes are frozen (SQLite `pages.background`; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Background {
    /// Code 0.
    Blank,
    /// Horizontal rules every `BACKGROUND_SPACING_PX`. Code 1.
    Lined,
    /// Square grid. Code 2.
    Grid,
    /// Dot grid. Code 3.
    Dot,
}

/// Materialized notebook (winning register values).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Notebook {
    pub id: NotebookId,
    pub title: String,
    pub deleted: bool,
}

/// Materialized page (winning register values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    /// Immutable.
    pub notebook: NotebookId,
    /// Order inside the notebook: `(position, id)` ascending.
    pub position: i64,
    /// Immutable.
    pub kind: PageKind,
    pub background: Background,
    pub deleted: bool,
}

/// Payload of a notebook op.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum NotebookChange {
    /// New notebook (writes `title` and `deleted = false`).
    Create { title: String },
    /// Overwrite the set registers with absolute values.
    Update(NotebookPatch),
}

/// Absolute new values (`None` = untouched).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookPatch {
    pub title: Option<String>,
    pub deleted: Option<bool>,
}

/// Payload of a page op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PageChange {
    /// New page (writes every register; `deleted = false`).
    Create {
        notebook: NotebookId,
        position: i64,
        kind: PageKind,
        background: Background,
    },
    /// Overwrite the set registers with absolute values.
    Update(PagePatch),
}

/// Absolute new values (`None` = untouched).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagePatch {
    pub position: Option<i64>,
    pub background: Option<Background>,
    pub deleted: Option<bool>,
}

impl PageKind {
    /// Core scene extent for [`penwise_core::item::Scene::new`].
    #[must_use]
    pub fn to_scene_kind(self) -> penwise_core::item::PageKind {
        todo!()
    }

    /// True for `Infinite`, or `Fixed` with both sides finite, `> 0` and `≤ MAX_PAGE_SIDE_PX`.
    #[must_use]
    pub fn is_valid(self) -> bool {
        todo!()
    }
}

impl Background {
    /// Frozen code (see variant docs).
    #[must_use]
    pub fn code(self) -> u8 {
        todo!()
    }

    /// Inverse of [`Background::code`]; `None` for unknown codes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        todo!()
    }
}

impl NotebookChange {
    /// True when every title present is non-empty after trimming and `≤ MAX_TITLE_BYTES`, and an `Update`
    /// sets at least one register.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        todo!()
    }
}

impl PageChange {
    /// True when a `Create` kind is valid ([`PageKind::is_valid`]) and an `Update` sets at least one register.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        todo!()
    }
}

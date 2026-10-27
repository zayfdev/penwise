//! Notebook and page model (PLAN "Data model": Notebook, Page) and the changes their ops carry.
//!
//! Registers (LWW by [`crate::Hlc`], see [`crate::lww`]): notebook `title`, `position`, `deleted`; page
//! `title`, `position`, `background`, `deleted`. Immutable after `Create`: a page's `notebook` and `kind`. `Create` writes every
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

/// Paper pattern drawn under every layer. Codes are frozen (`SQLite` `pages.background`; append only).
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
    /// Order in the notebook list: `(position, id)` ascending.
    pub position: i64,
    pub deleted: bool,
}

/// Materialized page (winning register values).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Page {
    pub id: PageId,
    /// Immutable.
    pub notebook: NotebookId,
    /// May be empty (untitled page); `≤ MAX_TITLE_BYTES`.
    pub title: String,
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
    /// New notebook (writes every register; `deleted = false`).
    Create { title: String, position: i64 },
    /// Overwrite the set registers with absolute values.
    Update(NotebookPatch),
}

/// Absolute new values (`None` = untouched).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookPatch {
    pub title: Option<String>,
    pub position: Option<i64>,
    pub deleted: Option<bool>,
}

/// Payload of a page op.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PageChange {
    /// New page (writes every register; `deleted = false`).
    Create {
        notebook: NotebookId,
        title: String,
        position: i64,
        kind: PageKind,
        background: Background,
    },
    /// Overwrite the set registers with absolute values.
    Update(PagePatch),
}

/// Absolute new values (`None` = untouched).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct PagePatch {
    pub title: Option<String>,
    pub position: Option<i64>,
    pub background: Option<Background>,
    pub deleted: Option<bool>,
}

impl PageKind {
    /// Core scene extent for [`penwise_core::item::Scene::new`].
    #[must_use]
    pub fn to_scene_kind(self) -> penwise_core::item::PageKind {
        match self {
            Self::Fixed { width, height } => penwise_core::item::PageKind::Fixed { width, height },
            Self::Infinite => penwise_core::item::PageKind::Infinite,
        }
    }

    /// Inverse of [`PageKind::to_scene_kind`].
    #[must_use]
    pub fn from_scene_kind(kind: penwise_core::item::PageKind) -> Self {
        match kind {
            penwise_core::item::PageKind::Fixed { width, height } => Self::Fixed { width, height },
            penwise_core::item::PageKind::Infinite => Self::Infinite,
        }
    }

    /// True for `Infinite`, or `Fixed` with both sides finite, `> 0` and `≤ MAX_PAGE_SIDE_PX`.
    #[must_use]
    pub fn is_valid(self) -> bool {
        match self {
            Self::Infinite => true,
            Self::Fixed { width, height } => [width, height]
                .into_iter()
                .all(|side| side.is_finite() && side > 0.0 && side <= MAX_PAGE_SIDE_PX),
        }
    }
}

impl Background {
    /// Frozen code (see variant docs).
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Blank => 0,
            Self::Lined => 1,
            Self::Grid => 2,
            Self::Dot => 3,
        }
    }

    /// Inverse of [`Background::code`]; `None` for unknown codes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Blank),
            1 => Some(Self::Lined),
            2 => Some(Self::Grid),
            3 => Some(Self::Dot),
            _ => None,
        }
    }
}

/// Notebook titles are never blank.
fn notebook_title_ok(title: &str) -> bool {
    !title.trim().is_empty() && title.len() <= MAX_TITLE_BYTES
}

/// Page titles may be empty (untitled page).
fn page_title_ok(title: &str) -> bool {
    title.len() <= MAX_TITLE_BYTES
}

impl NotebookChange {
    /// True when every title present is non-empty after trimming and `≤ MAX_TITLE_BYTES`, and an `Update`
    /// sets at least one register.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Create { title, .. } => notebook_title_ok(title),
            Self::Update(patch) => {
                (patch.title.is_some() || patch.position.is_some() || patch.deleted.is_some())
                    && patch.title.as_deref().is_none_or(notebook_title_ok)
            }
        }
    }
}

impl PageChange {
    /// True when a `Create` has a valid kind ([`PageKind::is_valid`]) and title (`≤ MAX_TITLE_BYTES`), and an
    /// `Update` sets at least one register with a valid title if present.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        match self {
            Self::Create { title, kind, .. } => kind.is_valid() && page_title_ok(title),
            Self::Update(patch) => {
                (patch.title.is_some()
                    || patch.position.is_some()
                    || patch.background.is_some()
                    || patch.deleted.is_some())
                    && patch.title.as_deref().is_none_or(page_title_ok)
            }
        }
    }
}

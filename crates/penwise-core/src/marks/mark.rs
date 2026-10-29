//! Mark items: a group of strokes drawn on one frame in one go, with an LWW status.

use super::MAX_STATE_LEN;
use crate::edit::{Edit, EditError, ItemPatch};
use crate::item::{FramePreset, Item, ItemId, ItemKind, Scene};

/// Most strokes one mark may group (bounds hostile remote lists).
pub const MAX_MARK_STROKES: usize = 4096;

/// Lifecycle of a mark. Codes are frozen (item codec and edit field values; append only).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum MarkStatus {
    /// Not looked at yet.
    New,
    /// The agent has read it.
    Seen,
    /// Handled (the edit landed); hidden unless show-done is on.
    Done,
    /// Done but questioned again (hidden when show-done is off, so it shows like `new`).
    Reopened,
}

impl MarkStatus {
    /// Every status, in code order.
    pub const ALL: [Self; 4] = [Self::New, Self::Seen, Self::Done, Self::Reopened];

    /// Frozen byte tag: `New = 0, Seen = 1, Done = 2, Reopened = 3`.
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::New => 0,
            Self::Seen => 1,
            Self::Done => 2,
            Self::Reopened => 3,
        }
    }

    /// Inverse of [`MarkStatus::code`]; `None` for unknown bytes.
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::New),
            1 => Some(Self::Seen),
            2 => Some(Self::Done),
            3 => Some(Self::Reopened),
            _ => None,
        }
    }

    /// The lowercase name the clients and JSON use (`"new"`, `"seen"`, `"done"`, `"reopened"`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::New => "new",
            Self::Seen => "seen",
            Self::Done => "done",
            Self::Reopened => "reopened",
        }
    }

    /// Inverse of [`MarkStatus::as_str`]; `None` for any other text.
    #[must_use]
    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|s| s.as_str() == text)
    }

    /// True when the status hides its mark unless show-done is on.
    #[must_use]
    pub fn is_done(self) -> bool {
        self == Self::Done
    }
}

/// A mark: the strokes drawn together on one frame at one preset, in the page state they were drawn in.
#[derive(Debug, Clone, PartialEq)]
pub struct MarkItem {
    /// The frame item the strokes belong to.
    pub frame: ItemId,
    /// The preset the group was drawn at (marks only show at their preset).
    pub preset: FramePreset,
    /// LWW status ([`crate::edit::Field::MarkStatus`]).
    pub status: MarkStatus,
    /// The page state the group was drawn in (empty = default).
    pub state: String,
    /// Member strokes, in addition order.
    pub strokes: Vec<ItemId>,
}

impl MarkItem {
    /// State at most [`MAX_STATE_LEN`] bytes and at most [`MAX_MARK_STROKES`] members.
    ///
    /// # Errors
    /// [`EditError::InvalidValue`] naming `"mark_state"` or `"mark_strokes"`.
    pub fn check(&self) -> Result<(), EditError> {
        if self.state.len() > MAX_STATE_LEN {
            return Err(EditError::InvalidValue {
                field: "mark_state",
            });
        }
        if self.strokes.len() > MAX_MARK_STROKES {
            return Err(EditError::InvalidValue {
                field: "mark_strokes",
            });
        }
        Ok(())
    }

    /// The mark item to add: ink layer, identity transform, `status`.
    #[must_use]
    pub fn item(self, id: ItemId, z: i64) -> Item {
        Item {
            id,
            layer: crate::item::Layer::Ink,
            z,
            transform: crate::item::Affine::IDENTITY,
            kind: ItemKind::Mark(self),
        }
    }
}

/// The mark item `id`, if `id` is a live mark.
#[must_use]
pub fn mark_of(scene: &Scene, id: ItemId) -> Option<&MarkItem> {
    match &scene.get(id)?.kind {
        ItemKind::Mark(mark) => Some(mark),
        _ => None,
    }
}

/// Edits giving mark `id` `status` (one `Field::MarkStatus` update, so it syncs by LWW and undoes like any other
/// field). Empty for an unknown/non-mark id or when the status is unchanged.
///
/// # Errors
/// [`EditError::UnknownItem`] only if the scene rejects the update.
pub fn set_mark_status(
    scene: &Scene,
    id: ItemId,
    status: MarkStatus,
) -> Result<Vec<Edit>, EditError> {
    let Some(mark) = mark_of(scene, id) else {
        return Ok(Vec::new());
    };
    if mark.status == status {
        return Ok(Vec::new());
    }
    Ok(vec![Edit::Update {
        id,
        patch: ItemPatch {
            mark_status: Some(status),
            ..ItemPatch::default()
        },
    }])
}

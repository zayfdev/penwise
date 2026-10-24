//! Local undo/redo over a multi-writer, append-only edit log (editing-algorithms.md §6; decided).
//!
//! Owner: slice e.
//!
//! - One stack per (device, page), in memory, never synced. Only **local** edits are recorded (through
//!   [`UndoStack::apply_local`]); remote and agent edits go straight to [`Scene::apply`] and are never undone
//!   from here.
//! - Undo/redo **emit new edits** (the caller applies them with `Scene::apply_all` and sends them like any
//!   other edit; it must not record them with `apply_local`). The log is never rewritten.
//! - One pen-down → pen-up gesture = one undo step: stroke, erase gesture (all splits/masks/cuts), transform
//!   drag, fill tap, lasso + delete. No time-based merging.
//! - Staleness: a recorded field is undone only while the scene still holds **bitwise** the value this device
//!   wrote ([`ItemPatch::field_eq`]); a field someone else changed since is skipped (their edit wins). Non-
//!   `Deleted` fields of items that are currently tombstoned are skipped too. Value equality stands in for
//!   per-field op stamps: core has no op metadata, and an equal value written by another device is
//!   indistinguishable after LWW anyway.

use super::{Edit, EditError, ItemPatch};
use crate::item::{ItemId, Scene};

/// Default number of undo steps kept (oldest dropped first).
pub const DEFAULT_DEPTH: usize = 200;

/// Net effect of one action on one item: per written field, the value before the action's first write and
/// after its last write. An `Add` records `before.deleted = Some(true)`, `after.deleted = Some(false)`.
/// Fields whose net before and after are bitwise equal are dropped at record time.
#[derive(Debug, Clone, PartialEq)]
struct ItemWrite {
    id: ItemId,
    before: ItemPatch,
    after: ItemPatch,
}

/// One undo step: net writes, one per item, sorted by id.
#[derive(Debug, Clone, Default, PartialEq)]
struct Action {
    writes: Vec<ItemWrite>,
}

/// Per-(device, page) undo/redo stack.
#[derive(Debug, Clone)]
pub struct UndoStack {
    undo: Vec<Action>,
    redo: Vec<Action>,
    /// Gesture being recorded (between `begin_gesture` and `end_gesture`).
    open: Option<Action>,
    depth: usize,
}

impl UndoStack {
    /// Empty stack keeping at most `depth` steps (`0` is treated as 1).
    #[must_use]
    pub fn new(depth: usize) -> Self {
        todo!()
    }

    /// Opens a gesture: every `apply_local` until [`UndoStack::end_gesture`] joins one step. Opening while a
    /// gesture is open first ends the open one.
    pub fn begin_gesture(&mut self) {
        todo!()
    }

    /// Closes the open gesture: a non-empty net action is pushed (dropping the oldest beyond `depth`) and the
    /// redo stack is cleared; an empty one is discarded. No-op without an open gesture.
    pub fn end_gesture(&mut self) {
        todo!()
    }

    /// Applies local `edits` to `scene` in order and records them. For each edit, the pre-edit values of the
    /// fields it writes are captured with [`Scene::invert`] before applying, then merged into the action's net
    /// writes. Inside an open gesture the edits join it; otherwise this call is one complete step (as if
    /// wrapped in `begin_gesture`/`end_gesture`).
    ///
    /// # Errors
    /// The first [`Scene::apply`] error. Edits before it stay applied and recorded; the failing edit and later
    /// ones are neither applied nor recorded.
    pub fn apply_local(&mut self, scene: &mut Scene, edits: &[Edit]) -> Result<(), EditError> {
        todo!()
    }

    /// Edits undoing the newest step that still has a non-stale field (ends an open gesture first).
    /// Per item (id order): fields still holding our `after` value are set back to `before`; a restored
    /// `deleted = true` on an item we added is emitted as `Delete(id)`, every other change as one
    /// `Update { id, patch }`. Steps where every field is stale are dropped and the next one is tried, so the
    /// user never sees a no-op undo. The emitted change is pushed on the redo stack with the **current** values
    /// as its targets (Figma rule). Empty when nothing is undoable.
    #[must_use]
    pub fn undo(&mut self, scene: &Scene) -> Vec<Edit> {
        todo!()
    }

    /// Mirror of [`UndoStack::undo`] over the redo stack (same staleness rules, pushes back onto undo). Any
    /// new `apply_local` step clears redo.
    #[must_use]
    pub fn redo(&mut self, scene: &Scene) -> Vec<Edit> {
        todo!()
    }

    /// True when an undo step (possibly stale) is stored or a gesture is open.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        todo!()
    }

    /// True when a redo step (possibly stale) is stored.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        todo!()
    }

    /// Drops all steps and any open gesture (e.g. page closed).
    pub fn clear(&mut self) {
        todo!()
    }
}

impl Default for UndoStack {
    /// `UndoStack::new(DEFAULT_DEPTH)`.
    fn default() -> Self {
        todo!()
    }
}

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

use super::{Edit, EditError, Field, ItemPatch};
use crate::item::{Item, ItemId, Scene};

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
        Self {
            undo: Vec::new(),
            redo: Vec::new(),
            open: None,
            depth: depth.max(1),
        }
    }

    /// Opens a gesture: every `apply_local` until [`UndoStack::end_gesture`] joins one step. Opening while a
    /// gesture is open first ends the open one.
    pub fn begin_gesture(&mut self) {
        self.end_gesture();
        self.open = Some(Action::default());
    }

    /// Closes the open gesture: a non-empty net action is pushed (dropping the oldest beyond `depth`) and the
    /// redo stack is cleared; an empty one is discarded. No-op without an open gesture.
    pub fn end_gesture(&mut self) {
        let Some(mut action) = self.open.take() else {
            return;
        };
        action.prune();
        if !action.writes.is_empty() {
            push_capped(&mut self.undo, action, self.depth);
            self.redo.clear();
        }
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
        let standalone = self.open.is_none();
        let action = self.open.get_or_insert_with(Action::default);
        let result = action.record(scene, edits);
        if standalone {
            self.end_gesture();
        }
        result
    }

    /// Edits undoing the newest step that still has a non-stale field (ends an open gesture first).
    /// Per item (id order): fields still holding our `after` value are set back to `before`; a restored
    /// `deleted = true` on an item we added is emitted as `Delete(id)`, every other change as one
    /// `Update { id, patch }`. Steps where every field is stale are dropped and the next one is tried, so the
    /// user never sees a no-op undo. The emitted change is pushed on the redo stack with the **current** values
    /// as its targets (Figma rule). Empty when nothing is undoable.
    #[must_use]
    pub fn undo(&mut self, scene: &Scene) -> Vec<Edit> {
        self.end_gesture();
        step(&mut self.undo, &mut self.redo, self.depth, scene)
    }

    /// Mirror of [`UndoStack::undo`] over the redo stack (same staleness rules, pushes back onto undo). Any
    /// new `apply_local` step clears redo.
    #[must_use]
    pub fn redo(&mut self, scene: &Scene) -> Vec<Edit> {
        self.end_gesture();
        step(&mut self.redo, &mut self.undo, self.depth, scene)
    }

    /// True when an undo step (possibly stale) is stored or a gesture is open.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.open.is_some()
    }

    /// True when a redo step (possibly stale) is stored.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }

    /// Drops all steps and any open gesture (e.g. page closed).
    pub fn clear(&mut self) {
        self.undo.clear();
        self.redo.clear();
        self.open = None;
    }
}

impl Default for UndoStack {
    /// `UndoStack::new(DEFAULT_DEPTH)`.
    fn default() -> Self {
        Self::new(DEFAULT_DEPTH)
    }
}

impl Action {
    /// Applies `edits` one by one, capturing each edit's pre-values with [`Scene::invert`] and its post-values
    /// from the scene, and folds them into the net writes. Stops at the first error.
    fn record(&mut self, scene: &mut Scene, edits: &[Edit]) -> Result<(), EditError> {
        for edit in edits {
            let before = match scene.invert(edit)? {
                Edit::Delete(_) => ItemPatch {
                    deleted: Some(true),
                    ..ItemPatch::default()
                },
                Edit::Update { patch, .. } => patch,
                Edit::Add(_) => ItemPatch::default(),
            };
            scene.apply(edit)?;
            let id = edit.target();
            if let Some((item, deleted)) = scene.get_any(id) {
                let after = ItemPatch::capture(item, deleted, &before.fields());
                self.merge(id, before, after);
            }
        }
        Ok(())
    }

    /// Folds one edit's `before`/`after` into the write for `id`: the earliest `before` and latest `after`
    /// of each field win.
    fn merge(&mut self, id: ItemId, before: ItemPatch, after: ItemPatch) {
        match self.writes.binary_search_by(|w| w.id.cmp(&id)) {
            Ok(index) => {
                if let Some(write) = self.writes.get_mut(index) {
                    let mut earliest = before;
                    earliest.merge(&write.before);
                    write.before = earliest;
                    write.after.merge(&after);
                }
            }
            Err(index) => self.writes.insert(index, ItemWrite { id, before, after }),
        }
    }

    /// Drops fields whose net before and after are bitwise equal, then writes left empty.
    fn prune(&mut self) {
        for write in &mut self.writes {
            let changed: Vec<Field> = write
                .before
                .fields()
                .into_iter()
                .filter(|&f| !write.before.field_eq(&write.after, f))
                .collect();
            write.before = write.before.restrict(&changed);
            write.after = write.after.restrict(&changed);
        }
        self.writes.retain(|w| !w.before.is_empty());
    }

    /// Edits setting every non-stale field back to `before`, and the action that reverses them (targets =
    /// the values just restored, `before` = the current values).
    fn revert(&self, scene: &Scene) -> (Vec<Edit>, Action) {
        let mut edits = Vec::new();
        let mut reverse = Action::default();
        for write in &self.writes {
            let Some((item, deleted)) = scene.get_any(write.id) else {
                continue;
            };
            let fields = write.live_fields(item, deleted);
            if fields.is_empty() {
                continue;
            }
            let target = write.before.restrict(&fields);
            let current = ItemPatch::capture(item, deleted, &fields);
            edits.push(to_edit(write.id, target.clone()));
            reverse.writes.push(ItemWrite {
                id: write.id,
                before: current,
                after: target,
            });
        }
        (edits, reverse)
    }
}

impl ItemWrite {
    /// Fields of this write that the scene still holds bitwise as our `after` value. Non-`Deleted` fields are
    /// dropped when the item is (or, after restoring our `Deleted` value, will be) tombstoned.
    fn live_fields(&self, item: &Item, deleted: bool) -> Vec<Field> {
        let written = self.after.fields();
        let current = ItemPatch::capture(item, deleted, &written);
        let mut ours: Vec<Field> = written
            .into_iter()
            .filter(|&f| current.field_eq(&self.after, f))
            .collect();
        let dead_after = if ours.contains(&Field::Deleted) {
            self.before.deleted.unwrap_or(deleted)
        } else {
            deleted
        };
        if dead_after {
            ours.retain(|&f| f == Field::Deleted);
        }
        ours
    }
}

/// Pops `from` until a step yields edits; pushes its reverse onto `to`. Fully stale steps are dropped.
fn step(from: &mut Vec<Action>, to: &mut Vec<Action>, depth: usize, scene: &Scene) -> Vec<Edit> {
    while let Some(action) = from.pop() {
        let (edits, reverse) = action.revert(scene);
        if !edits.is_empty() {
            push_capped(to, reverse, depth);
            return edits;
        }
    }
    Vec::new()
}

/// Pushes `action`, dropping the oldest steps beyond `depth`.
fn push_capped(stack: &mut Vec<Action>, action: Action, depth: usize) {
    stack.push(action);
    let excess = stack.len().saturating_sub(depth);
    stack.drain(..excess);
}

/// `Delete(id)` for a patch that only tombstones, `Update` otherwise.
fn to_edit(id: ItemId, patch: ItemPatch) -> Edit {
    let tombstone = ItemPatch {
        deleted: Some(true),
        ..ItemPatch::default()
    };
    if patch == tombstone {
        Edit::Delete(id)
    } else {
        Edit::Update { id, patch }
    }
}

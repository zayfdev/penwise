//! `SceneHandle`: one page's [`Scene`] plus its local [`UndoStack`] and the tools that turn pen gestures into
//! core edits. Every tool applies its edits to the scene, records them as undo steps and **returns** them;
//! the caller hands the returned edits to [`crate::sync_core::SyncCore::commit_edits`], which stamps and queues
//! them (design §1.2-1.4). Undo and redo return their edits without recording them.
//!
//! The scene is a mirror of the replica's page: the replica (inside `SyncCore`) is the LWW authority, so remote
//! changes reach the scene through [`SceneHandle::refresh`] with the [`crate::sync_core::Change`] list that
//! `SyncCore::receive` reports (or [`SceneHandle::reload`] after a snapshot). Local edits carry stamps newer
//! than everything the device has seen, so they always win and the mirror never diverges.

mod dirty;
mod edits_blob;
mod frame_space;
mod frames;
mod marks;
mod mirror;
mod tools;

pub use edits_blob::{decode_edits, encode_edits};
pub use tools::{
    SAMPLE_FLOATS, ShapeSpec, StrokeSpec, handle_from_code, pairs, shape_brush, shape_kind,
};

use std::cell::RefCell;

use penwise_core::edit::{Edit, UndoStack, undo::DEFAULT_DEPTH};
use penwise_core::item::{ItemId, Scene};
use penwise_proto::outbox::check_edit_size;
use penwise_proto::{IdMint, PageId};

use crate::error::SyncError;
use crate::sync_core::SyncCore;
use dirty::Dirty;

/// A page's scene, undo stack and id stream.
#[derive(Debug)]
pub struct SceneHandle {
    page: PageId,
    scene: Scene,
    undo: UndoStack,
    ids: IdMint,
    dirty: Dirty,
    /// Client-local mark state (frame layouts, show-done, cached presentation).
    marks: RefCell<marks::MarksState>,
}

impl SceneHandle {
    /// Opens `page` from the replica of `core`; `entropy` seeds the id stream for items minted by tools.
    ///
    /// # Errors
    /// `UnknownPage` when the replica has no such page.
    pub fn open(core: &SyncCore, page: PageId, entropy: u64) -> Result<Self, SyncError> {
        let replica = core.replica().page(page).ok_or(SyncError::UnknownPage)?;
        Ok(Self {
            page,
            scene: replica.scene().clone(),
            undo: UndoStack::new(DEFAULT_DEPTH),
            ids: IdMint::new(core.device(), entropy),
            dirty: Dirty::everything(),
            marks: RefCell::new(marks::MarksState::default()),
        })
    }

    /// The page this handle edits.
    #[must_use]
    pub fn page(&self) -> PageId {
        self.page
    }

    /// The mirrored scene (hit tests, geometry, render order).
    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// A fresh item id (stamp the same id on live deltas and on the committed `Add`).
    pub fn new_item_id(&mut self, now_ms: u64) -> ItemId {
        use penwise_core::item::IdSource;
        self.ids.set_now(now_ms);
        self.ids.next_id()
    }

    /// Opens an undo gesture (pen down); everything until [`SceneHandle::end_gesture`] is one undo step.
    pub fn begin_gesture(&mut self) {
        self.undo.begin_gesture();
    }

    /// Closes the open gesture (pen up).
    pub fn end_gesture(&mut self) {
        self.undo.end_gesture();
    }

    /// True when an undo step is available.
    #[must_use]
    pub fn can_undo(&self) -> bool {
        self.undo.can_undo()
    }

    /// True when a redo step is available.
    #[must_use]
    pub fn can_redo(&self) -> bool {
        self.undo.can_redo()
    }

    /// Edits undoing the newest still-valid step, already applied to the scene (empty: nothing to undo).
    ///
    /// # Errors
    /// The scene refused an edit (a bug: undo edits are derived from the scene itself).
    pub fn undo(&mut self) -> Result<Vec<Edit>, SyncError> {
        let edits = self.undo.undo(&self.scene);
        self.dirty.mark_edits(&self.scene, &edits);
        self.scene.apply_all(&edits)?;
        self.marks.get_mut().invalidate();
        Ok(edits)
    }

    /// Mirror of [`SceneHandle::undo`].
    ///
    /// # Errors
    /// As [`SceneHandle::undo`].
    pub fn redo(&mut self) -> Result<Vec<Edit>, SyncError> {
        let edits = self.undo.redo(&self.scene);
        self.dirty.mark_edits(&self.scene, &edits);
        self.scene.apply_all(&edits)?;
        self.marks.get_mut().invalidate();
        Ok(edits)
    }

    /// Commits `edits` (just applied to this scene by a tool, undo or redo) to `core` for this page. When the
    /// commit fails the scene is reloaded from the replica, so it never keeps an edit the replica refused.
    ///
    /// # Errors
    /// The commit's error (`OpTooLarge`, validation, replica).
    pub fn commit_into(
        &mut self,
        core: &mut SyncCore,
        edits: &[Edit],
        now_ms: u64,
    ) -> Result<(), SyncError> {
        if edits.is_empty() {
            return Ok(());
        }
        if let Err(error) = core.commit_edits(self.page, edits, now_ms) {
            let _ = self.reload(core);
            return Err(error);
        }
        Ok(())
    }

    /// Takes the ids whose render data changed since the last call (see [`dirty`]): byte 0 is `1` when the
    /// whole scene changed, then 16-byte ids.
    pub fn take_dirty(&mut self) -> Vec<u8> {
        self.dirty.take()
    }

    /// Applies `edits` as local recorded edits; Adds go first and Deletes last so a mid-gesture disconnect can
    /// only duplicate ink, never lose it (design §1.3).
    fn local(&mut self, mut edits: Vec<Edit>) -> Result<Vec<Edit>, SyncError> {
        for edit in &edits {
            check_edit_size(edit).map_err(SyncError::OpTooLarge)?;
        }
        edits.sort_by_key(|e| matches!(e, Edit::Delete(_)));
        self.dirty.mark_edits(&self.scene, &edits);
        self.undo.apply_local(&mut self.scene, &edits)?;
        self.marks.get_mut().invalidate();
        Ok(edits)
    }
}

#[cfg(test)]
mod tests;

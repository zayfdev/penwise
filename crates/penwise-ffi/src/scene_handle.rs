//! `SceneHandle`: one page's [`Scene`] plus its local [`UndoStack`] and the tools that turn pen gestures into
//! core edits. Every tool applies its edits to the scene, records them as undo steps and **returns** them;
//! the caller hands the returned edits to [`crate::sync_core::SyncCore::commit_edits`], which stamps and queues
//! them (design §1.2-1.4). Undo and redo return their edits without recording them.
//!
//! The scene is a mirror of the replica's page: the replica (inside `SyncCore`) is the LWW authority, so remote
//! changes reach the scene through [`SceneHandle::refresh`] with the [`crate::sync_core::Change`] list that
//! `SyncCore::receive` reports (or [`SceneHandle::reload`] after a snapshot). Local edits carry stamps newer
//! than everything the device has seen, so they always win and the mirror never diverges.

mod edits_blob;
mod mirror;
mod tools;

pub use edits_blob::{decode_edits, encode_edits};
pub use tools::{SAMPLE_FLOATS, ShapeSpec, StrokeSpec, handle_from_code, pairs, shape_kind};

use penwise_core::edit::{Edit, UndoStack, undo::DEFAULT_DEPTH};
use penwise_core::item::{ItemId, LayerSet, Scene};
use penwise_proto::PageId;

use crate::error::SyncError;
use crate::ids::IdMint;
use crate::sync_core::SyncCore;

/// A page's scene, undo stack and id stream.
#[derive(Debug)]
pub struct SceneHandle {
    page: PageId,
    scene: Scene,
    undo: UndoStack,
    ids: IdMint,
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
        self.scene.apply_all(&edits)?;
        Ok(edits)
    }

    /// Mirror of [`SceneHandle::undo`].
    ///
    /// # Errors
    /// As [`SceneHandle::undo`].
    pub fn redo(&mut self) -> Result<Vec<Edit>, SyncError> {
        let edits = self.undo.redo(&self.scene);
        self.scene.apply_all(&edits)?;
        Ok(edits)
    }

    /// Live item ids on `layers` in paint order (bottom → top).
    #[must_use]
    pub fn render_order(&self, layers: LayerSet) -> Vec<ItemId> {
        self.scene.render_order(layers)
    }

    /// Applies `edits` as local recorded edits; Adds go first and Deletes last so a mid-gesture disconnect can
    /// only duplicate ink, never lose it (design §1.3).
    fn local(&mut self, mut edits: Vec<Edit>) -> Result<Vec<Edit>, SyncError> {
        edits.sort_by_key(|e| matches!(e, Edit::Delete(_)));
        self.undo.apply_local(&mut self.scene, &edits)?;
        Ok(edits)
    }
}

#[cfg(test)]
mod tests;

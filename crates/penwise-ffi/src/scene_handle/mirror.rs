//! Keeping the scene equal to the replica's page after remote changes.

use penwise_core::edit::{Edit, Field, ItemPatch};
use penwise_core::item::{ItemId, Scene};

use super::SceneHandle;
use crate::error::SyncError;
use crate::sync_core::{Change, SyncCore};

impl SceneHandle {
    /// Replaces the scene with a copy of the replica's page (after a snapshot, or when a mirror step failed).
    /// A page the replica no longer has (the device moved to another server) becomes an empty scene.
    ///
    /// # Errors
    /// None today; the `Result` keeps the signature stable for callers that propagate errors.
    pub fn reload(&mut self, core: &SyncCore) -> Result<(), SyncError> {
        self.dirty.mark_all();
        self.scene = match core.replica().page(self.page) {
            Some(replica) => replica.scene().clone(),
            None => Scene::new(self.scene.kind()),
        };
        Ok(())
    }

    /// Mirrors the changes of one `SyncCore::receive` that concern this page. A failed per-item mirror falls
    /// back to a full reload.
    ///
    /// # Errors
    /// `UnknownPage` when a reload is needed and the page vanished.
    pub fn refresh(&mut self, core: &SyncCore, changes: &[Change]) -> Result<(), SyncError> {
        let page = self.page;
        for change in changes.iter().filter(|c| c.page == page) {
            match change.item {
                None => self.reload(core)?,
                Some(id) => {
                    if self.mirror_item(core, id).is_err() {
                        self.reload(core)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// Makes item `id` of the scene equal the replica's winning state (`Add`, or one absolute patch).
    fn mirror_item(&mut self, core: &SyncCore, id: ItemId) -> Result<(), SyncError> {
        let Some(replica) = core.replica().page(self.page) else {
            return Err(SyncError::UnknownPage);
        };
        let Some(winner) = replica.item(id) else {
            return Ok(());
        };
        let Some((current, deleted)) = self.scene.get_any(id) else {
            let add = Edit::Add(winner.item.clone());
            self.dirty
                .mark_edits(&self.scene, std::slice::from_ref(&add));
            self.scene.apply(&add)?;
            if winner.deleted {
                self.scene.apply(&Edit::Delete(id))?;
            }
            return Ok(());
        };
        if *current == winner.item && deleted == winner.deleted {
            return Ok(());
        }
        let patch = ItemPatch::capture(&winner.item, winner.deleted, &Field::ALL);
        let update = Edit::Update { id, patch };
        self.dirty
            .mark_edits(&self.scene, std::slice::from_ref(&update));
        self.scene.apply(&update)?;
        Ok(())
    }
}

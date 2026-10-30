//! The stills' memory: the latest [`Still`] metadata of every frame (small, never evicted by size) and the
//! version counters that keep versions rising.

use std::collections::HashMap;
use std::sync::Arc;

use penwise_core::item::ItemId;

use super::model::Still;

/// Latest still per frame, and the version counters that outlive a deleted-and-restored frame's still.
#[derive(Debug, Default)]
pub(crate) struct StillCache {
    entries: HashMap<ItemId, Arc<Still>>,
    versions: HashMap<ItemId, u64>,
}

impl StillCache {
    pub(crate) fn get(&self, frame: ItemId) -> Option<Arc<Still>> {
        self.entries.get(&frame).cloned()
    }

    /// The version a new still of `frame` gets: above every earlier one and at least `floor` (a clock, so a
    /// restarted server does not reuse a version a client still remembers).
    pub(crate) fn next_version(&mut self, frame: ItemId, floor: u64) -> u64 {
        let last = self.versions.get(&frame).copied().unwrap_or(0);
        let next = last.saturating_add(1).max(floor);
        self.versions.insert(frame, next);
        next
    }

    pub(crate) fn put(&mut self, frame: ItemId, still: Arc<Still>) {
        self.entries.insert(frame, still);
    }

    /// Drops the frame's still (the version counter stays: a restored frame must not reuse a version).
    pub(crate) fn forget(&mut self, frame: ItemId) {
        self.entries.remove(&frame);
    }

    /// The cached frames of `project`.
    pub(crate) fn frames_of(&self, project: &str) -> Vec<ItemId> {
        self.entries
            .iter()
            .filter(|(_, still)| still.project == project)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Every cached frame.
    pub(crate) fn frames(&self) -> Vec<ItemId> {
        self.entries.keys().copied().collect()
    }
}

//! Which items' render data changed since the renderer last looked, so the tablet repaints only those.
//!
//! A change to an erase item changes its *target's* render data (the mask is a part of the target), so the
//! target is marked too. Marking happens before the edits are applied, while the erase item's old target is
//! still readable.

use std::collections::BTreeSet;

use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, ItemKind, Scene};

/// Render changes since the last [`Dirty::take`].
#[derive(Debug)]
pub struct Dirty {
    all: bool,
    ids: BTreeSet<ItemId>,
}

impl Dirty {
    /// Everything is dirty: the renderer has not seen this scene yet.
    pub fn everything() -> Self {
        Self {
            all: true,
            ids: BTreeSet::new(),
        }
    }

    /// The scene was replaced wholesale.
    pub fn mark_all(&mut self) {
        self.all = true;
        self.ids.clear();
    }

    /// Marks what `edits` will change in `scene` (call before applying them).
    pub fn mark_edits(&mut self, scene: &Scene, edits: &[Edit]) {
        if self.all {
            return;
        }
        for edit in edits {
            let id = edit.target();
            self.ids.insert(id);
            if let Edit::Add(item) = edit {
                self.mark_target(&item.kind);
            }
            if let Some((item, _)) = scene.get_any(id) {
                self.mark_target(&item.kind);
            }
        }
    }

    fn mark_target(&mut self, kind: &ItemKind) {
        if let ItemKind::Erase(erase) = kind {
            self.ids.insert(erase.target);
        }
    }

    /// Takes the pending changes: byte 0 is `1` when everything changed, then 16-byte ids.
    pub fn take(&mut self) -> Vec<u8> {
        let mut out = vec![u8::from(self.all)];
        if !self.all {
            out.extend(self.ids.iter().flat_map(|id| id.0));
        }
        self.all = false;
        self.ids.clear();
        out
    }
}

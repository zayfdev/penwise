//! One page of a replica: LWW item states and the core scene they produce.

use std::collections::BTreeMap;

use penwise_core::edit::Edit;
use penwise_core::item::{ItemId, Scene};

use crate::error::ReplicaError;
use crate::lww::{LwwError, MaterializedItem};
use crate::notebook::PageKind;
use crate::op::Op;

/// One page: winning item states and the core scene they produce.
#[derive(Debug, Clone)]
pub struct PageReplica {
    scene: Scene,
    items: BTreeMap<ItemId, MaterializedItem>,
}

impl PageReplica {
    /// Empty page of `kind`.
    #[must_use]
    pub fn new(kind: PageKind) -> Self {
        Self {
            scene: Scene::new(kind.to_scene_kind()),
            items: BTreeMap::new(),
        }
    }

    /// Page from stored states (server render, tablet page open): `Scene::from_items` over
    /// `(item, deleted)` pairs.
    ///
    /// # Errors
    /// [`ReplicaError::Edit`] from `Scene::from_items` (duplicate id, invalid item).
    pub fn from_items(
        kind: PageKind,
        items: impl IntoIterator<Item = MaterializedItem>,
    ) -> Result<Self, ReplicaError> {
        let items: Vec<MaterializedItem> = items.into_iter().collect();
        let scene = Scene::from_items(
            kind.to_scene_kind(),
            items.iter().map(|m| (m.item.clone(), m.deleted)),
        )?;
        Ok(Self {
            scene,
            items: items.into_iter().map(|m| (m.item.id, m)).collect(),
        })
    }

    /// Applies one item op: `Add` → [`MaterializedItem::from_add`] + `Scene::apply(Add)`; `Delete`/`Update`
    /// → [`MaterializedItem::merge`] and `Scene::apply` of the winning sub-edit. Returns true when the scene
    /// changed. Re-applying the same `Add` (same id, same stamp) is `Ok(false)`.
    ///
    /// # Errors
    /// `Duplicate` (Add of an existing id with another stamp), `UnknownItem`, `Edit` (scene rejected the
    /// edit; the replica is unchanged).
    pub fn apply(&mut self, op: &Op) -> Result<bool, ReplicaError> {
        match &op.edit {
            Edit::Add(item) => {
                if let Some(existing) = self.items.get(&item.id) {
                    return if existing.stamps.created == op.hlc {
                        Ok(false)
                    } else {
                        Err(ReplicaError::Duplicate)
                    };
                }
                self.scene.apply(&op.edit)?;
                self.items
                    .insert(item.id, MaterializedItem::from_add(item.clone(), op.hlc));
                Ok(true)
            }
            Edit::Delete(id) | Edit::Update { id, .. } => {
                let state = self.items.get_mut(id).ok_or(ReplicaError::UnknownItem)?;
                match state.merge(&op.edit, op.hlc).map_err(lww_error)? {
                    Some(winning) => {
                        self.scene.apply(&winning)?;
                        Ok(true)
                    }
                    None => Ok(false),
                }
            }
        }
    }

    /// Folds a stored row into the page: inserts an unknown item, or merges the registers of a known one
    /// ([`MaterializedItem::merge_state`]). Returns true when the scene changed.
    ///
    /// # Errors
    /// `Duplicate` (known id with another creation stamp or kind), `Edit` (invalid item).
    pub fn merge_state(&mut self, state: MaterializedItem) -> Result<bool, ReplicaError> {
        let id = state.item.id;
        let Some(existing) = self.items.get_mut(&id) else {
            self.scene.apply(&Edit::Add(state.item.clone()))?;
            if state.deleted {
                self.scene.apply(&Edit::Delete(id))?;
            }
            self.items.insert(id, state);
            return Ok(true);
        };
        match existing.merge_state(&state).map_err(lww_error)? {
            Some(winning) => {
                self.scene.apply(&winning)?;
                Ok(true)
            }
            None => Ok(false),
        }
    }

    /// Drops item `id` (state and scene), e.g. to replace it by the server's version. The scene is rebuilt
    /// from the remaining items.
    ///
    /// # Errors
    /// `Edit` when the remaining items no longer form a scene (never expected).
    pub(crate) fn forget(&mut self, id: ItemId) -> Result<(), ReplicaError> {
        if self.items.remove(&id).is_none() {
            return Ok(());
        }
        self.scene = Scene::from_items(
            self.scene.kind(),
            self.items.values().map(|m| (m.item.clone(), m.deleted)),
        )?;
        Ok(())
    }

    /// The page scene (render order, geometry, hit tests).
    #[must_use]
    pub fn scene(&self) -> &Scene {
        &self.scene
    }

    /// Winning state of an item (tombstones included).
    #[must_use]
    pub fn item(&self, id: ItemId) -> Option<&MaterializedItem> {
        self.items.get(&id)
    }

    /// Every item's winning state (tombstones included), ascending id.
    pub fn items(&self) -> impl Iterator<Item = &MaterializedItem> {
        self.items.values()
    }
}

/// Replicas are equal when their winning item states are (the scene is derived from them).
impl PartialEq for PageReplica {
    fn eq(&self, other: &Self) -> bool {
        self.items == other.items
    }
}

/// `Edit` errors stay `Edit` (the replica API reports scene rejections uniformly); the rest are LWW misuse.
fn lww_error(error: LwwError) -> ReplicaError {
    match error {
        LwwError::Edit(e) => ReplicaError::Edit(e),
        other => ReplicaError::Lww(other),
    }
}

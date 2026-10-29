//! In-memory page scene: items, tombstones, derived geometry cache, spatial index, edit application.
//!
//! Owner: slice a.
//!
//! The scene applies edits in the order given; it does not resolve concurrency. `penwise-proto` (M3)
//! materializes LWW state from the op log and feeds the scene edits that already reflect the winning values
//! (all patches carry absolute values, so re-applying the winner is always correct).
//!
//! **Determinism.** Everything observable (query results, iteration, geometry) is independent of insertion
//! order and container internals: maps are `BTreeMap`/`BTreeSet`, and rstar hits are sorted by `(z, id)`
//! before they leave the scene.

mod derive;

use std::collections::{BTreeMap, BTreeSet};

use super::geometry::{ItemGeometry, RenderKey};
use super::id::ItemId;
use super::index::SpatialIndex;
use super::layer::LayerSet;
use super::{Item, ItemKind, Z_STEP};
use crate::bounds::Bounds;
use crate::edit::{Edit, EditError, Field, ItemPatch};

use derive::Slot;

/// Page extent (PLAN "Data model": page kind).
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum PageKind {
    /// `[0, width] × [0, height]` page px; the fill bucket treats the page edge as a wall.
    Fixed {
        width: f32,
        height: f32,
    },
    Infinite,
}

/// One page's items.
///
/// Derived-data invalidation on every applied edit (slice a keeps these exact):
/// - the edited item's geometry, bounds and index entry;
/// - for a stroke family (same origin, [`crate::item::StrokeItem::origin`]): every live member, because the
///   family intersection ([`Scene::visible_ranges`]) changes when a member is added, deleted or restored;
/// - for a target's transform or liveness change: the target's erase items (their page masks move).
#[derive(Debug, Clone)]
pub struct Scene {
    kind: PageKind,
    slots: BTreeMap<ItemId, Slot>,
    /// Live non-erase items in paint order.
    order: BTreeSet<RenderKey>,
    index: SpatialIndex,
    /// Stroke origin → live family members (including the origin itself when live).
    families: BTreeMap<ItemId, BTreeSet<ItemId>>,
    /// Target → live erase items.
    masks: BTreeMap<ItemId, BTreeSet<ItemId>>,
}

impl Scene {
    /// Empty scene.
    #[must_use]
    pub fn new(kind: PageKind) -> Self {
        Self {
            kind,
            slots: BTreeMap::new(),
            order: BTreeSet::new(),
            index: SpatialIndex::default(),
            families: BTreeMap::new(),
            masks: BTreeMap::new(),
        }
    }

    /// Bulk load (page open): `(item, deleted)` pairs in any order. Builds the index with `RTree::bulk_load`
    /// and derives all geometry once; the result equals applying `Add` for every item and then `Delete` for
    /// the tombstoned ones.
    ///
    /// # Errors
    /// `DuplicateItem` for a repeated id; `InvalidValue` from [`Item::validate`].
    pub fn from_items(
        kind: PageKind,
        items: impl IntoIterator<Item = (Item, bool)>,
    ) -> Result<Self, EditError> {
        let mut scene = Self::new(kind);
        for (item, deleted) in items {
            if scene.slots.contains_key(&item.id) {
                return Err(EditError::DuplicateItem(item.id));
            }
            item.validate()?;
            scene.slots.insert(item.id, Slot::new(item, deleted));
        }
        scene.derive_all();
        Ok(scene)
    }

    /// Page extent.
    #[must_use]
    pub fn kind(&self) -> PageKind {
        self.kind
    }

    /// Live item by id (`None` for unknown or deleted).
    #[must_use]
    pub fn get(&self, id: ItemId) -> Option<&Item> {
        self.live(id).map(|s| &s.item)
    }

    /// Item including tombstones: `(item, deleted)`.
    #[must_use]
    pub fn get_any(&self, id: ItemId) -> Option<(&Item, bool)> {
        self.slots.get(&id).map(|s| (&s.item, s.deleted))
    }

    /// Live item ids, ascending (deterministic: ULID order).
    #[must_use]
    pub fn live_ids(&self) -> Vec<ItemId> {
        self.slots
            .iter()
            .filter(|(_, slot)| !slot.deleted)
            .map(|(id, _)| *id)
            .collect()
    }

    /// Number of live items (erase items included).
    #[must_use]
    pub fn len(&self) -> usize {
        self.slots.values().filter(|s| !s.deleted).count()
    }

    /// True when no item is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.slots.values().all(|s| s.deleted)
    }

    /// Cached page geometry of a live item.
    #[must_use]
    pub fn geometry(&self, id: ItemId) -> Option<&ItemGeometry> {
        self.live(id).and_then(|s| s.geometry.as_ref())
    }

    /// Cached page bounds of a live item.
    #[must_use]
    pub fn bounds(&self, id: ItemId) -> Option<Bounds> {
        self.live(id).map(|s| s.bounds)
    }

    /// Paint key of a live non-erase item.
    #[must_use]
    pub fn render_key(&self, id: ItemId) -> Option<RenderKey> {
        self.live(id).and_then(|s| s.item.paint_key())
    }

    /// Live non-erase items on `layers` in paint order `(band, z, id)` ascending. Renderers draw each item
    /// followed by its [`Scene::masks`] (destination-out on the item's own layer).
    #[must_use]
    pub fn render_order(&self, layers: LayerSet) -> Vec<ItemId> {
        self.order
            .iter()
            .filter(|k| {
                self.get(k.id)
                    .is_some_and(|item| layers.contains(item.layer))
            })
            .map(|k| k.id)
            .collect()
    }

    /// Live non-erase items on `layers` whose bounds intersect `area` (closed boxes), sorted by `(z, id)`
    /// ascending. rstar prefilter, then exact `Bounds::intersects` on the cached bounds.
    #[must_use]
    pub fn query(&self, area: &Bounds, layers: LayerSet) -> Vec<ItemId> {
        let mut hits = Vec::new();
        self.index.query(area, &mut hits);
        let mut keyed: Vec<(i64, ItemId)> = hits
            .into_iter()
            .filter_map(|id| self.live(id).map(|s| (id, s)))
            .filter(|(_, s)| {
                !matches!(s.item.kind, ItemKind::Erase(_))
                    && layers.contains(s.item.layer)
                    && s.bounds.intersects(area)
            })
            .map(|(id, s)| (s.item.z, id))
            .collect();
        keyed.sort_unstable();
        keyed.dedup();
        keyed.into_iter().map(|(_, id)| id).collect()
    }

    /// Largest z among live items, `0` for an empty scene.
    #[must_use]
    pub fn top_z(&self) -> i64 {
        self.slots
            .values()
            .filter(|s| !s.deleted)
            .map(|s| s.item.z)
            .max()
            .unwrap_or(0)
    }

    /// z for a new topmost item: `top_z() + Z_STEP`, or `None` when that overflows `i64` (the z space above
    /// the top item is exhausted; callers must not create an item that would tie with it).
    #[must_use]
    pub fn next_z(&self) -> Option<i64> {
        self.top_z().checked_add(Z_STEP)
    }

    /// Applies one edit atomically (validate first; on error the scene is unchanged).
    /// - `Add(item)`: [`Item::validate`]; inserts live.
    /// - `Delete(id)`: tombstones a live item; deleting a tombstone is a no-op `Ok`.
    /// - `Update { id, patch }`: writes every set field (on tombstones too: they keep LWW state);
    ///   `deleted: Some(false)` restores a tombstone, `Some(true)` deletes. Then re-validates the item.
    ///
    /// # Errors
    /// `DuplicateItem` (Add of an existing id, live or tombstoned), `UnknownItem` (Delete/Update of an
    /// unknown id), `FieldNotApplicable` (patch field the item's kind does not have), `InvalidValue`.
    pub fn apply(&mut self, edit: &Edit) -> Result<(), EditError> {
        match edit {
            Edit::Add(item) => {
                if self.slots.contains_key(&item.id) {
                    return Err(EditError::DuplicateItem(item.id));
                }
                item.validate()?;
                self.slots.insert(item.id, Slot::new(item.clone(), true));
                self.replace(item.id, item.clone(), false, true);
            }
            Edit::Delete(id) => {
                let slot = self.slots.get(id).ok_or(EditError::UnknownItem(*id))?;
                if !slot.deleted {
                    let item = slot.item.clone();
                    self.replace(*id, item, true, false);
                }
            }
            Edit::Update { id, patch } => {
                let slot = self.slots.get(id).ok_or(EditError::UnknownItem(*id))?;
                let (item, deleted) = patch.applied(&slot.item, slot.deleted)?;
                let dirty = patch
                    .fields()
                    .iter()
                    .any(|f| !matches!(f, Field::Deleted | Field::Layer | Field::Z));
                self.replace(*id, item, deleted, dirty);
            }
        }
        Ok(())
    }

    /// Applies `edits` in order, stopping at the first error (earlier edits stay applied; the error carries
    /// no index — callers needing all-or-nothing validate on a clone).
    ///
    /// # Errors
    /// The first [`Scene::apply`] error.
    pub fn apply_all(&mut self, edits: &[Edit]) -> Result<(), EditError> {
        edits.iter().try_for_each(|e| self.apply(e))
    }

    /// Edit that undoes `edit` if applied right after it, computed from the current (pre-`edit`) state:
    /// - `Add(item)` → `Delete(item.id)`;
    /// - `Delete(id)` → `Update { id, deleted: Some(current_deleted) }` (restores a live item; a no-op delete
    ///   of a tombstone inverts to a no-op);
    /// - `Update { id, patch }` → `Update { id, patch' }` where `patch'` holds the **current absolute value**
    ///   of exactly the fields `patch` sets ([`crate::edit::ItemPatch::capture`]).
    ///
    /// # Errors
    /// The errors `apply(edit)` would return, checked without mutating.
    pub fn invert(&self, edit: &Edit) -> Result<Edit, EditError> {
        match edit {
            Edit::Add(item) => {
                if self.slots.contains_key(&item.id) {
                    return Err(EditError::DuplicateItem(item.id));
                }
                item.validate()?;
                Ok(Edit::Delete(item.id))
            }
            Edit::Delete(id) => {
                let slot = self.slots.get(id).ok_or(EditError::UnknownItem(*id))?;
                let patch = ItemPatch {
                    deleted: Some(slot.deleted),
                    ..ItemPatch::default()
                };
                Ok(Edit::Update { id: *id, patch })
            }
            Edit::Update { id, patch } => {
                let slot = self.slots.get(id).ok_or(EditError::UnknownItem(*id))?;
                patch.applied(&slot.item, slot.deleted)?;
                let patch = ItemPatch::capture(&slot.item, slot.deleted, &patch.fields());
                Ok(Edit::Update { id: *id, patch })
            }
        }
    }

    fn live(&self, id: ItemId) -> Option<&Slot> {
        self.slots.get(&id).filter(|s| !s.deleted)
    }
}

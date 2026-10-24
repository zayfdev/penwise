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

use std::collections::{BTreeMap, BTreeSet};

use super::geometry::{DeriveContext, ItemGeometry, RenderKey};
use super::id::ItemId;
use super::index::SpatialIndex;
use super::layer::LayerSet;
use super::stroke::{SampleRange, intersect_ranges};
use super::{Item, ItemKind, Z_STEP};
use crate::bounds::Bounds;
use crate::edit::{Edit, EditError, Field, ItemPatch};

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

/// Per-item state.
#[derive(Debug, Clone)]
struct Slot {
    item: Item,
    /// Tombstone: deleted items stay (undo/LWW need them) but leave the index, order and families.
    deleted: bool,
    /// Cached derived geometry; `None` while deleted.
    geometry: Option<ItemGeometry>,
    /// `geometry`'s bounds ([`Bounds::EMPTY`] while deleted); exactly what the index holds for non-erase items.
    bounds: Bounds,
}

impl Slot {
    /// Slot without derived data (the scene links and derives it next).
    fn new(item: Item, deleted: bool) -> Self {
        let (geometry, bounds) = (None, Bounds::EMPTY);
        Self {
            item,
            deleted,
            geometry,
            bounds,
        }
    }
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
        let live: Vec<ItemId> = scene
            .slots
            .iter()
            .filter(|(_, s)| !s.deleted)
            .map(|(id, _)| *id)
            .collect();
        for &id in &live {
            scene.link(id);
        }
        let mut entries = Vec::with_capacity(live.len());
        for &id in &live {
            let geometry = scene.derive(id);
            let Some(slot) = scene.slots.get_mut(&id) else {
                continue;
            };
            slot.bounds = geometry
                .as_ref()
                .map_or(Bounds::EMPTY, ItemGeometry::bounds);
            slot.geometry = geometry;
            if !matches!(slot.item.kind, ItemKind::Erase(_)) {
                entries.push((id, slot.bounds));
            }
        }
        scene.index = SpatialIndex::bulk(&entries);
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

    /// z for a new topmost item: `top_z() + Z_STEP` (saturating).
    #[must_use]
    pub fn next_z(&self) -> i64 {
        self.top_z().saturating_add(Z_STEP)
    }

    /// Effective visible ranges of a live stroke: `own(X) ∩ ⋂ family_visible(Y)` over every other live member
    /// `Y` of its family ([`crate::item::intersect_ranges`], members folded in id order). Empty for non-strokes
    /// and dead items.
    #[must_use]
    pub fn visible_ranges(&self, id: ItemId) -> Vec<SampleRange> {
        let Some(ItemKind::Stroke(stroke)) = self.get(id).map(|item| &item.kind) else {
            return Vec::new();
        };
        let mut visible = vec![stroke.own_range()];
        let family = self.families.get(&stroke.origin(id));
        for &other in family.into_iter().flatten().filter(|&&m| m != id) {
            if let Some(ItemKind::Stroke(s)) = self.get(other).map(|item| &item.kind) {
                visible = intersect_ranges(&visible, &s.family_visible());
            }
        }
        visible
    }

    /// Live erase items targeting `target`, ascending id.
    #[must_use]
    pub fn masks(&self, target: ItemId) -> Vec<ItemId> {
        members(&self.masks, target)
    }

    /// Live members of the stroke family rooted at `origin`, ascending id.
    #[must_use]
    pub fn family(&self, origin: ItemId) -> Vec<ItemId> {
        members(&self.families, origin)
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

    /// Swaps in a validated new state for an existing slot and keeps every derived structure exact
    /// (see the invalidation list on [`Scene`]). `dirty`: the item's own geometry inputs changed.
    fn replace(&mut self, id: ItemId, item: Item, deleted: bool, dirty: bool) {
        let Some(slot) = self.slots.get(&id) else {
            return;
        };
        let was_live = !slot.deleted;
        let moved = !slot.item.transform.bit_eq(&item.transform);
        if was_live {
            self.unlink(id);
        }
        if let Some(slot) = self.slots.get_mut(&id) {
            slot.item = item;
            slot.deleted = deleted;
        }
        let now_live = !deleted;
        if now_live {
            self.link(id);
            if dirty || !was_live {
                self.rederive(id);
            }
        } else {
            self.set_geometry(id, None);
        }
        let mut dependents = Vec::new();
        if was_live != now_live || moved {
            dependents.extend(self.masks(id));
        }
        if was_live != now_live
            && let Some(ItemKind::Stroke(s)) = self.slots.get(&id).map(|s| &s.item.kind)
        {
            dependents.extend(self.family(s.origin(id)).into_iter().filter(|&m| m != id));
        }
        for dependent in dependents {
            self.rederive(dependent);
        }
    }

    /// Registers a live slot in the paint order, its family and its target's mask set.
    fn link(&mut self, id: ItemId) {
        let Some(slot) = self.slots.get(&id) else {
            return;
        };
        match &slot.item.kind {
            ItemKind::Erase(e) => {
                self.masks.entry(e.target).or_default().insert(id);
            }
            ItemKind::Stroke(s) => {
                self.families.entry(s.origin(id)).or_default().insert(id);
            }
            ItemKind::Shape(_) | ItemKind::Fill(_) | ItemKind::Text(_) => {}
        }
        if let Some(key) = slot.item.paint_key() {
            self.order.insert(key);
        }
    }

    /// Reverse of [`Scene::link`] (called before the slot's item changes).
    fn unlink(&mut self, id: ItemId) {
        let Some(slot) = self.slots.get(&id) else {
            return;
        };
        match &slot.item.kind {
            ItemKind::Erase(e) => remove_member(&mut self.masks, e.target, id),
            ItemKind::Stroke(s) => remove_member(&mut self.families, s.origin(id), id),
            ItemKind::Shape(_) | ItemKind::Fill(_) | ItemKind::Text(_) => {}
        }
        if let Some(key) = slot.item.paint_key() {
            self.order.remove(&key);
        }
    }

    /// Geometry of a live slot from its current item and scene context.
    fn derive(&self, id: ItemId) -> Option<ItemGeometry> {
        let slot = self.live(id)?;
        let visible = self.visible_ranges(id);
        let target_transform = if let ItemKind::Erase(e) = &slot.item.kind {
            self.get(e.target).map(|t| t.transform)
        } else {
            None
        };
        let ctx = DeriveContext {
            visible: &visible,
            target_transform: target_transform.as_ref(),
        };
        Some(slot.item.derive_geometry(&ctx))
    }

    fn rederive(&mut self, id: ItemId) {
        let geometry = self.derive(id);
        self.set_geometry(id, geometry);
    }

    /// Stores `geometry` and moves the slot's index entry to the new bounds (non-erase items only).
    fn set_geometry(&mut self, id: ItemId, geometry: Option<ItemGeometry>) {
        let Some(slot) = self.slots.get_mut(&id) else {
            return;
        };
        let bounds = geometry
            .as_ref()
            .map_or(Bounds::EMPTY, ItemGeometry::bounds);
        if !matches!(slot.item.kind, ItemKind::Erase(_)) {
            self.index.remove(id, &slot.bounds);
            self.index.insert(id, &bounds);
        }
        slot.geometry = geometry;
        slot.bounds = bounds;
    }
}

fn remove_member(map: &mut BTreeMap<ItemId, BTreeSet<ItemId>>, group: ItemId, id: ItemId) {
    if let Some(set) = map.get_mut(&group)
        && set.remove(&id)
        && set.is_empty()
    {
        map.remove(&group);
    }
}

fn members(map: &BTreeMap<ItemId, BTreeSet<ItemId>>, group: ItemId) -> Vec<ItemId> {
    map.get(&group)
        .map(|m| m.iter().copied().collect())
        .unwrap_or_default()
}

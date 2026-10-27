//! Scene bookkeeping: per-item [`Slot`]s, family/mask/order links and derived geometry + index sync.
//!
//! Owner: slice a.

use std::collections::{BTreeMap, BTreeSet};

use super::Scene;
use crate::bounds::Bounds;
use crate::item::geometry::{DeriveContext, ItemGeometry};
use crate::item::id::ItemId;
use crate::item::index::SpatialIndex;
use crate::item::stroke::{SampleRange, intersect_ranges};
use crate::item::{Item, ItemKind};

/// Per-item state.
#[derive(Debug, Clone)]
pub(super) struct Slot {
    pub(super) item: Item,
    /// Tombstone: deleted items stay (undo/LWW need them) but leave the index, order and families.
    pub(super) deleted: bool,
    /// Cached derived geometry; `None` while deleted.
    pub(super) geometry: Option<ItemGeometry>,
    /// `geometry`'s bounds ([`Bounds::EMPTY`] while deleted); exactly what the index holds for non-erase items.
    pub(super) bounds: Bounds,
}

impl Slot {
    /// Slot without derived data (the scene links and derives it next).
    pub(super) fn new(item: Item, deleted: bool) -> Self {
        let (geometry, bounds) = (None, Bounds::EMPTY);
        Self {
            item,
            deleted,
            geometry,
            bounds,
        }
    }
}

impl Scene {
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

    /// Bulk-load tail of [`Scene::from_items`]: links every live slot, derives its geometry once and builds
    /// the index with `RTree::bulk_load`.
    pub(super) fn derive_all(&mut self) {
        let live: Vec<ItemId> = self
            .slots
            .iter()
            .filter(|(_, s)| !s.deleted)
            .map(|(id, _)| *id)
            .collect();
        for &id in &live {
            self.link(id);
        }
        let mut entries = Vec::with_capacity(live.len());
        for &id in &live {
            let geometry = self.derive(id);
            let Some(slot) = self.slots.get_mut(&id) else {
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
        self.index = SpatialIndex::bulk(&entries);
    }

    /// Swaps in a validated new state for an existing slot and keeps every derived structure exact
    /// (see the invalidation list on [`Scene`]). `dirty`: the item's own geometry inputs changed.
    pub(super) fn replace(&mut self, id: ItemId, item: Item, deleted: bool, dirty: bool) {
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
    pub(super) fn link(&mut self, id: ItemId) {
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
            ItemKind::Shape(_) | ItemKind::Fill(_) | ItemKind::Text(_) | ItemKind::Image(_) => {}
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
            ItemKind::Shape(_) | ItemKind::Fill(_) | ItemKind::Text(_) | ItemKind::Image(_) => {}
        }
        if let Some(key) = slot.item.paint_key() {
            self.order.remove(&key);
        }
    }

    /// Geometry of a live slot from its current item and scene context.
    pub(super) fn derive(&self, id: ItemId) -> Option<ItemGeometry> {
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

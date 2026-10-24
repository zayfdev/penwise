//! Per-page R*-tree over live item bounds (`rstar` 0.13, editing-algorithms.md §4).
//!
//! Owner: slice a. Crate-internal: only [`crate::item::Scene`] touches it.
//!
//! rstar's tree shape depends on insertion order, so query hits are unordered; callers sort them by
//! `(z, id)` (the scene does) before anything observable happens.

use rstar::{AABB, RTree, RTreeObject};

use super::id::ItemId;
use crate::bounds::Bounds;

/// One indexed item: its page bounds and id.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(crate) struct IndexEntry {
    pub(crate) min: [f32; 2],
    pub(crate) max: [f32; 2],
    pub(crate) id: ItemId,
}

impl RTreeObject for IndexEntry {
    type Envelope = AABB<[f32; 2]>;

    fn envelope(&self) -> Self::Envelope {
        AABB::from_corners(self.min, self.max)
    }
}

impl IndexEntry {
    fn new(id: ItemId, b: &Bounds) -> Self {
        Self {
            min: [b.min_x, b.min_y],
            max: [b.max_x, b.max_y],
            id,
        }
    }
}

/// R*-tree of live, non-erase items with non-empty bounds.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpatialIndex {
    tree: RTree<IndexEntry>,
}

impl SpatialIndex {
    /// Bulk-loads `entries` (`RTree::bulk_load`); empty bounds are skipped.
    pub(crate) fn bulk(entries: &[(ItemId, Bounds)]) -> Self {
        let entries = entries
            .iter()
            .filter(|(_, b)| !b.is_empty())
            .map(|(id, b)| IndexEntry::new(*id, b))
            .collect();
        Self {
            tree: RTree::bulk_load(entries),
        }
    }

    /// Inserts `id` with `bounds` (no-op for empty bounds).
    pub(crate) fn insert(&mut self, id: ItemId, bounds: &Bounds) {
        if !bounds.is_empty() {
            self.tree.insert(IndexEntry::new(id, bounds));
        }
    }

    /// Removes the entry `(id, bounds)` (exact match of the bounds it was inserted with; empty bounds were
    /// never inserted, so they are a no-op).
    pub(crate) fn remove(&mut self, id: ItemId, bounds: &Bounds) {
        if !bounds.is_empty() {
            self.tree.remove(&IndexEntry::new(id, bounds));
        }
    }

    /// Ids whose envelope intersects `area` (`locate_in_envelope_intersecting`), **unsorted**, appended to
    /// `out`.
    pub(crate) fn query(&self, area: &Bounds, out: &mut Vec<ItemId>) {
        if area.is_empty() {
            return;
        }
        let envelope = AABB::from_corners([area.min_x, area.min_y], [area.max_x, area.max_y]);
        out.extend(
            self.tree
                .locate_in_envelope_intersecting(envelope)
                .map(|e| e.id),
        );
    }
}

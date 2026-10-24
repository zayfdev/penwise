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
        todo!()
    }
}

/// R*-tree of live, non-erase items with non-empty bounds.
#[derive(Debug, Clone, Default)]
pub(crate) struct SpatialIndex {
    tree: RTree<IndexEntry>,
}

impl SpatialIndex {
    /// Bulk-loads `entries` (`RTree::bulk_load`); empty bounds are skipped.
    pub(crate) fn bulk(entries: Vec<(ItemId, Bounds)>) -> Self {
        todo!()
    }

    /// Inserts `id` with `bounds` (no-op for empty bounds).
    pub(crate) fn insert(&mut self, id: ItemId, bounds: &Bounds) {
        todo!()
    }

    /// Removes the entry `(id, bounds)` (exact match of the bounds it was inserted with).
    pub(crate) fn remove(&mut self, id: ItemId, bounds: &Bounds) {
        todo!()
    }

    /// Ids whose envelope intersects `area` (`locate_in_envelope_intersecting`), **unsorted**, appended to
    /// `out`.
    pub(crate) fn query(&self, area: &Bounds, out: &mut Vec<ItemId>) {
        todo!()
    }
}

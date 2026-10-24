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

use super::Item;
use super::geometry::{ItemGeometry, RenderKey};
use super::id::ItemId;
use super::index::SpatialIndex;
use super::layer::LayerSet;
use super::stroke::SampleRange;
use crate::bounds::Bounds;
use crate::edit::{Edit, EditError};

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
        todo!()
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
        todo!()
    }

    /// Page extent.
    #[must_use]
    pub fn kind(&self) -> PageKind {
        todo!()
    }

    /// Live item by id (`None` for unknown or deleted).
    #[must_use]
    pub fn get(&self, id: ItemId) -> Option<&Item> {
        todo!()
    }

    /// Item including tombstones: `(item, deleted)`.
    #[must_use]
    pub fn get_any(&self, id: ItemId) -> Option<(&Item, bool)> {
        todo!()
    }

    /// Number of live items (erase items included).
    #[must_use]
    pub fn len(&self) -> usize {
        todo!()
    }

    /// True when no item is live.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        todo!()
    }

    /// Cached page geometry of a live item.
    #[must_use]
    pub fn geometry(&self, id: ItemId) -> Option<&ItemGeometry> {
        todo!()
    }

    /// Cached page bounds of a live item.
    #[must_use]
    pub fn bounds(&self, id: ItemId) -> Option<Bounds> {
        todo!()
    }

    /// Paint key of a live non-erase item.
    #[must_use]
    pub fn render_key(&self, id: ItemId) -> Option<RenderKey> {
        todo!()
    }

    /// Live non-erase items on `layers` in paint order `(band, z, id)` ascending. Renderers draw each item
    /// followed by its [`Scene::masks`] (destination-out on the item's own layer).
    #[must_use]
    pub fn render_order(&self, layers: LayerSet) -> Vec<ItemId> {
        todo!()
    }

    /// Live non-erase items on `layers` whose bounds intersect `area` (closed boxes), sorted by `(z, id)`
    /// ascending. rstar prefilter, then exact `Bounds::intersects` on the cached bounds.
    #[must_use]
    pub fn query(&self, area: &Bounds, layers: LayerSet) -> Vec<ItemId> {
        todo!()
    }

    /// Largest z among live items, `0` for an empty scene.
    #[must_use]
    pub fn top_z(&self) -> i64 {
        todo!()
    }

    /// z for a new topmost item: `top_z() + Z_STEP` (saturating).
    #[must_use]
    pub fn next_z(&self) -> i64 {
        todo!()
    }

    /// Effective visible ranges of a live stroke: `own(X) ∩ ⋂ family_visible(Y)` over every other live member
    /// `Y` of its family ([`crate::item::intersect_ranges`], members folded in id order). Empty for non-strokes
    /// and dead items.
    #[must_use]
    pub fn visible_ranges(&self, id: ItemId) -> Vec<SampleRange> {
        todo!()
    }

    /// Live erase items targeting `target`, ascending id.
    #[must_use]
    pub fn masks(&self, target: ItemId) -> Vec<ItemId> {
        todo!()
    }

    /// Live members of the stroke family rooted at `origin`, ascending id.
    #[must_use]
    pub fn family(&self, origin: ItemId) -> Vec<ItemId> {
        todo!()
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
        todo!()
    }

    /// Applies `edits` in order, stopping at the first error (earlier edits stay applied; the error carries
    /// no index — callers needing all-or-nothing validate on a clone).
    ///
    /// # Errors
    /// The first [`Scene::apply`] error.
    pub fn apply_all(&mut self, edits: &[Edit]) -> Result<(), EditError> {
        todo!()
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
        todo!()
    }
}

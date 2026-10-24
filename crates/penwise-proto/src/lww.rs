//! Last-writer-wins registers (PLAN "Sync protocol": LWW per item field).
//!
//! Owner: slice a.
//!
//! **Rule.** Each (target, field) pair is one register holding `(value, stamp)`. A write `(v, h)` replaces
//! it iff `h > stamp` ([`crate::Hlc`] total order: `wall_ms`, then `counter`, then device bytes). Equal stamps
//! mean the same write re-applied (op ids are unique, and one device never reuses a stamp): no change.
//! Registers per target:
//! - item: one per core [`Field`] (13 in M2: `Deleted`, `Layer`, `Z`, `Transform`, `Brush`, `ShapeFill`,
//!   `Paint`, `Polygons`, `ShapeKind`, `ShapeSize`, `Text`, `TextBox`, `FontSize`); `Edit::Add` writes all of
//!   them at the op's stamp (fields the kind lacks included, harmlessly); `Edit::Delete` writes
//!   `Deleted = true`; `Edit::Update` writes exactly the patch's set fields. Immutable data (`id`, kind,
//!   samples, seed, trim, erase path, text sources) comes from the `Add` only.
//! - notebook: `title`, `deleted`; page: `position`, `background`, `deleted` ([`crate::notebook`]).
//!
//! Because every write is absolute and the comparison is a total order, applying any set of ops in any
//! order, with duplicates, yields the same registers (convergence; tested with shuffled/duplicated
//! delivery per AGENTS.md "Testing").

use penwise_core::edit::{Edit, Field};
use penwise_core::item::Item;
use serde::{Deserialize, Serialize};

use crate::hlc::Hlc;

/// Item-level LWW misuse (a bug in the caller, never a remote-data condition).
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum LwwError {
    /// `merge` was given an `Edit::Add` (adds create a [`MaterializedItem`] instead).
    #[error("merge of an Add")]
    AddNotMergeable,
    /// The edit targets another item.
    #[error("edit targets a different item")]
    WrongTarget,
}

/// One register.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Register<T> {
    pub value: T,
    pub stamp: Hlc,
}

impl<T> Register<T> {
    /// Register created at `stamp`.
    #[must_use]
    pub fn new(value: T, stamp: Hlc) -> Self {
        todo!()
    }

    /// Applies the write `(value, stamp)`; returns true iff it won (`stamp > self.stamp`) and replaced the value.
    pub fn set(&mut self, value: T, stamp: Hlc) -> bool {
        todo!()
    }
}

/// Per-field stamps of one item. `fields[Field::code]`; a missing entry (field added to core after this
/// blob was written) reads as `created`. Stored as postcard in SQLite `items.stamps`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FieldStamps {
    /// Stamp of the item's `Add`.
    pub created: Hlc,
    pub fields: Vec<Hlc>,
}

impl FieldStamps {
    /// Every field stamped `created` (`fields.len() == Field::ALL.len()`).
    #[must_use]
    pub fn new(created: Hlc) -> Self {
        todo!()
    }

    /// Stamp of `field` (`created` when absent).
    #[must_use]
    pub fn get(&self, field: Field) -> Hlc {
        todo!()
    }

    /// Sets the stamp of `field` (extending `fields` with `created` as needed).
    pub fn set(&mut self, field: Field, stamp: Hlc) {
        todo!()
    }
}

/// An item's winning state: the core item with winning field values, its tombstone and per-field stamps.
#[derive(Debug, Clone, PartialEq)]
pub struct MaterializedItem {
    pub item: Item,
    pub deleted: bool,
    pub stamps: FieldStamps,
}

impl MaterializedItem {
    /// State right after `Edit::Add(item)` stamped `created`: live, every field stamped `created`.
    #[must_use]
    pub fn from_add(item: Item, created: Hlc) -> Self {
        todo!()
    }

    /// Merges a `Delete`/`Update` of this item stamped `stamp`: keeps the fields whose register it wins
    /// (`stamp > stamps.get(field)`; `Delete` = field `Deleted` with value `true`), updates `item`, `deleted`
    /// and `stamps` for them, and returns the **winning sub-edit** for the page scene:
    /// `Some(Edit::Update { id, patch: patch.restrict(winners) })` (a `Delete` that wins returns
    /// `Some(Edit::Delete(id))`), `None` when no field won. Feeding every returned edit to
    /// `Scene::apply` keeps the scene equal to the materialized state.
    ///
    /// # Errors
    /// [`LwwError::AddNotMergeable`] for an `Add`; [`LwwError::WrongTarget`] for another item's edit. The
    /// state is unchanged on error.
    pub fn merge(&mut self, edit: &Edit, stamp: Hlc) -> Result<Option<Edit>, LwwError> {
        todo!()
    }
}

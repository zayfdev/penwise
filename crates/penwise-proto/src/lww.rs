//! Last-writer-wins registers (PLAN "Sync protocol": LWW per item field).
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
//! - notebook: `title`, `position`, `deleted`; page: `title`, `position`, `background`, `deleted`
//!   ([`crate::notebook`]).
//!
//! **Duplicate `Add`.** An `Add` of an id that exists is a no-op when its stamp equals the item's `created`
//! stamp (the same op delivered twice, e.g. the author's own echo), and a `Duplicate` error otherwise (the
//! server rejects a second `Add` of one id under a different op id, so replicas never see it).
//!
//! Because every write is absolute and the comparison is a total order, applying any set of ops in any
//! order, with duplicates, yields the same registers (convergence; tested with shuffled/duplicated
//! delivery per AGENTS.md "Testing").

use penwise_core::edit::{Edit, EditError, Field, ItemPatch};
use penwise_core::item::Item;
use serde::{Deserialize, Serialize};

use crate::error::ProtoError;
use crate::hlc::Hlc;

/// Leading byte of the stored [`FieldStamps`] blob (`items.stamps`).
pub const STAMPS_VERSION: u8 = 1;

/// Item-level LWW misuse (a bug in the caller, never a remote-data condition).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum LwwError {
    /// `merge` was given an `Edit::Add` (adds create a [`MaterializedItem`] instead).
    #[error("merge of an Add")]
    AddNotMergeable,
    /// The edit targets another item.
    #[error("edit targets a different item")]
    WrongTarget,
    /// The patch sets a field the item's kind lacks, or the winning values break an item invariant. Checked
    /// over the whole patch (not only the winners) so the verdict never depends on delivery order.
    #[error(transparent)]
    Edit(#[from] EditError),
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
        Self { value, stamp }
    }

    /// Applies the write `(value, stamp)`; returns true iff it won (`stamp > self.stamp`) and replaced the value.
    pub fn set(&mut self, value: T, stamp: Hlc) -> bool {
        if stamp > self.stamp {
            self.value = value;
            self.stamp = stamp;
            true
        } else {
            false
        }
    }
}

/// Per-field stamps of one item. `fields[Field::code]`; a missing entry (field added to core after this
/// blob was written) reads as `created`. Stored as postcard in `SQLite` `items.stamps`.
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
        Self {
            created,
            fields: vec![created; Field::ALL.len()],
        }
    }

    /// Stamp of `field` (`created` when absent).
    #[must_use]
    pub fn get(&self, field: Field) -> Hlc {
        self.fields
            .get(usize::from(field.code()))
            .copied()
            .unwrap_or(self.created)
    }

    /// Sets the stamp of `field` (extending `fields` with `created` as needed).
    pub fn set(&mut self, field: Field, stamp: Hlc) {
        let index = usize::from(field.code());
        if self.fields.len() <= index {
            self.fields.resize(index + 1, self.created);
        }
        if let Some(slot) = self.fields.get_mut(index) {
            *slot = stamp;
        }
    }

    /// The `fields` a write stamped `stamp` wins (`stamp > get(field)`), in the order given.
    #[must_use]
    pub fn winners(&self, fields: &[Field], stamp: Hlc) -> Vec<Field> {
        fields
            .iter()
            .copied()
            .filter(|&field| stamp > self.get(field))
            .collect()
    }

    /// `[STAMPS_VERSION]` + postcard (`SQLite` `items.stamps`, `ItemState::stamps`).
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut out = vec![STAMPS_VERSION];
        // Serializing plain integers into a `Vec` cannot fail.
        if let Ok(bytes) = postcard::to_allocvec(self) {
            out.extend_from_slice(&bytes);
        }
        out
    }

    /// Inverse of [`FieldStamps::encode`].
    ///
    /// # Errors
    /// `Empty`, `UnsupportedVersion`, `Decode`, `TrailingBytes`, or `Invalid` for more stamps than core has
    /// fields.
    pub fn decode(bytes: &[u8]) -> Result<Self, ProtoError> {
        let (&version, body) = bytes.split_first().ok_or(ProtoError::Empty)?;
        if version != STAMPS_VERSION {
            return Err(ProtoError::UnsupportedVersion(version));
        }
        let (stamps, rest) = postcard::take_from_bytes::<Self>(body).map_err(ProtoError::Decode)?;
        if !rest.is_empty() {
            return Err(ProtoError::TrailingBytes { count: rest.len() });
        }
        if stamps.fields.len() > Field::ALL.len() {
            return Err(ProtoError::Invalid("field stamps"));
        }
        Ok(stamps)
    }
}

/// The LWW primitive on a bare stamp set (the server keeps `body ⊕ patch` instead of a [`MaterializedItem`]):
/// every field of `patch` whose register `hlc` wins is stamped `hlc`; returns the winning sub-patch.
pub fn merge_patch(stamps: &mut FieldStamps, patch: &ItemPatch, hlc: Hlc) -> ItemPatch {
    let winners = stamps.winners(&patch.fields(), hlc);
    for &field in &winners {
        stamps.set(field, hlc);
    }
    patch.restrict(&winners)
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
        Self {
            item,
            deleted: false,
            stamps: FieldStamps::new(created),
        }
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
        let (target, patch) = match edit {
            Edit::Add(_) => return Err(LwwError::AddNotMergeable),
            Edit::Delete(id) => (
                *id,
                ItemPatch {
                    deleted: Some(true),
                    ..ItemPatch::default()
                },
            ),
            Edit::Update { id, patch } => (*id, patch.clone()),
        };
        if target != self.item.id {
            return Err(LwwError::WrongTarget);
        }
        let tag = self.item.kind.tag();
        if let Some(field) = patch.fields().into_iter().find(|f| !f.applies_to(tag)) {
            return Err(EditError::FieldNotApplicable { id: target, field }.into());
        }
        let winners = self.stamps.winners(&patch.fields(), stamp);
        if winners.is_empty() {
            return Ok(None);
        }
        let winning = patch.restrict(&winners);
        let (item, deleted) = winning.applied(&self.item, self.deleted)?;
        self.item = item;
        self.deleted = deleted;
        for &field in &winners {
            self.stamps.set(field, stamp);
        }
        Ok(Some(match edit {
            Edit::Delete(id) => Edit::Delete(*id),
            _ => Edit::Update {
                id: target,
                patch: winning,
            },
        }))
    }

    /// Folds another materialization of the **same** item (same id, kind and `created` stamp: a snapshot row)
    /// into this one, register by register. Returns the winning sub-edit for the scene (as
    /// [`MaterializedItem::merge`]); `None` when nothing changed.
    ///
    /// # Errors
    /// [`LwwError::WrongTarget`] for another item, kind or creation stamp; `Edit` when the winning values
    /// break an invariant. The state is unchanged on error.
    pub fn merge_state(&mut self, other: &MaterializedItem) -> Result<Option<Edit>, LwwError> {
        if other.item.id != self.item.id
            || other.item.kind.tag() != self.item.kind.tag()
            || other.stamps.created != self.stamps.created
        {
            return Err(LwwError::WrongTarget);
        }
        let tag = self.item.kind.tag();
        let winners: Vec<Field> = Field::ALL
            .into_iter()
            .filter(|f| f.applies_to(tag) && other.stamps.get(*f) > self.stamps.get(*f))
            .collect();
        if winners.is_empty() {
            return Ok(None);
        }
        let winning = ItemPatch::capture(&other.item, other.deleted, &winners);
        let (item, deleted) = winning.applied(&self.item, self.deleted)?;
        self.item = item;
        self.deleted = deleted;
        for &field in &winners {
            self.stamps.set(field, other.stamps.get(field));
        }
        Ok(Some(Edit::Update {
            id: self.item.id,
            patch: winning,
        }))
    }
}

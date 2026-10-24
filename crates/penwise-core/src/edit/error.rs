//! Edit application errors.
//!
//! Owner: slice a.

use super::patch::Field;
use crate::item::ItemId;

/// Why the scene rejected an edit (the scene is unchanged when this is returned).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EditError {
    /// `Delete`/`Update` of an id the scene has never seen (not even as a tombstone).
    #[error("unknown item {0:?}")]
    UnknownItem(ItemId),
    /// `Add` of an id that already exists (live or tombstoned).
    #[error("duplicate item {0:?}")]
    DuplicateItem(ItemId),
    /// The patch sets a field the item's kind does not have (e.g. `FontSize` on a stroke).
    #[error("field {field:?} does not apply to item {id:?}")]
    FieldNotApplicable { id: ItemId, field: Field },
    /// A value breaks an item invariant ([`crate::item::Item::validate`]); `field` names it.
    #[error("invalid {field}")]
    InvalidValue { field: &'static str },
}

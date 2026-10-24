//! Materialized items: the `items` table (winning state per item, tombstones included).
//!
//! Owner: slice a.
//!
//! Row encoding: `data` = core `codec::item::encode_item` (PWIT v1) of `MaterializedItem::item`;
//! `stamps` = `STAMPS_VERSION` byte + postcard `FieldStamps`; `deleted`, `kind`, `page` duplicated as columns
//! for queries. Written only inside `append_ops`' transaction.

use penwise_proto::{ItemRef, MaterializedItem, Op, OpRejection, PageId};
use rusqlite::Transaction;

use super::{Storage, StorageError};

/// Leading version byte of `items.stamps`.
pub const STAMPS_VERSION: u8 = 1;

/// Light row for listings (`/api/pages/{page}/items`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ItemSummary {
    pub id: ItemRef,
    /// core `ItemKindTag::code`.
    pub kind: u8,
    pub deleted: bool,
    pub updated_seq: u64,
}

impl Storage {
    /// Every item of `page` (tombstones included), ascending id: input for `PageReplica::from_items`.
    ///
    /// # Errors
    /// [`StorageError::Corrupt`] for undecodable `data`/`stamps`; SQLite errors.
    pub fn page_items(&self, page: PageId) -> Result<Vec<MaterializedItem>, StorageError> {
        todo!()
    }

    /// Summaries of every item of `page`, ascending id.
    ///
    /// # Errors
    /// SQLite errors.
    pub fn item_summaries(&self, page: PageId) -> Result<Vec<ItemSummary>, StorageError> {
        todo!()
    }
}

/// Materializes one item op inside the append transaction (after the op row got `seq`): `Add` inserts a row
/// (`added_seq = updated_seq = seq`); `Delete`/`Update` loads the row, `MaterializedItem::merge`s and writes
/// it back when a field won (`updated_seq = seq`). Returns `Err(rejection)` without writing for an unknown
/// item, a duplicate `Add`, or a page mismatch (checked before the op row is inserted by calling this with
/// `seq = None` first: dry-run mode).
///
/// # Errors
/// [`StorageError`] for SQLite/corrupt rows.
pub(crate) fn materialize_item_op(
    tx: &Transaction<'_>,
    op: &Op,
    seq: Option<u64>,
) -> Result<Result<(), OpRejection>, StorageError> {
    todo!()
}

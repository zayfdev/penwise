//! Materialized notebooks and pages (`notebooks`/`pages` tables) and their list snapshots.
//!
//! Owner: slice a.
//!
//! Register merge = proto LWW rule in SQL: `UPDATE pages SET position = ?1, position_hlc = ?2 WHERE id = ?3
//! AND position_hlc < ?2` (24-byte stamps compare by memcmp = `Hlc` order), one statement per present field.
//! `Create` inserts every register at the op stamp.

use penwise_proto::{
    NotebookId, NotebookInfo, NotebookOp, OpRejection, Page, PageId, PageInfo, PageOp,
};
use rusqlite::Transaction;

use super::{Storage, StorageError};

impl Storage {
    /// Live notebooks, ascending id, with live page counts and `updated_ms`.
    ///
    /// # Errors
    /// SQLite errors.
    pub fn notebook_list(&self) -> Result<Vec<NotebookInfo>, StorageError> {
        todo!()
    }

    /// Live pages of `notebook`, ascending `(position, id)`, with live item counts.
    ///
    /// # Errors
    /// [`StorageError::Corrupt`] for an unknown background code; SQLite errors.
    pub fn page_list(&self, notebook: NotebookId) -> Result<Vec<PageInfo>, StorageError> {
        todo!()
    }

    /// One materialized page (tombstoned included); `None` if never created.
    ///
    /// # Errors
    /// As [`Storage::page_list`].
    pub fn page(&self, id: PageId) -> Result<Option<Page>, StorageError> {
        todo!()
    }
}

/// Checks (and, when `apply`, materializes) a notebook op inside the append transaction.
/// Rejections: `Duplicate` (create of an existing id), `UnknownNotebook` (update of an unknown id).
///
/// # Errors
/// [`StorageError`] for SQLite failures.
pub(crate) fn materialize_notebook_op(
    tx: &Transaction<'_>,
    op: &NotebookOp,
    apply: bool,
) -> Result<Result<(), OpRejection>, StorageError> {
    todo!()
}

/// Checks (and, when `apply`, materializes) a page op inside the append transaction.
/// Rejections: `Duplicate`, `UnknownNotebook` (create in an unknown notebook), `UnknownPage` (update).
///
/// # Errors
/// [`StorageError`] for SQLite failures.
pub(crate) fn materialize_page_op(
    tx: &Transaction<'_>,
    op: &PageOp,
    apply: bool,
) -> Result<Result<(), OpRejection>, StorageError> {
    todo!()
}

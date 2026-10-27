//! Notebook and page ops: `Create` inserts every register at the op's stamp, `Update` overwrites each present
//! register iff its stamp wins (same rule as `penwise_proto::Catalog`).

use penwise_proto::{
    Background, NotebookChange, NotebookOp, OpRejection, PageChange, PageKind, PageOp,
};
use rusqlite::{OptionalExtension, Transaction, params};

use super::{Done, Fail, log_floor};
use crate::storage::rows::{parse_hlc, to_i64};

/// What a register write did.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Write {
    /// The stamp won and the register was overwritten.
    Won,
    /// A newer stamp is stored.
    Lost,
    /// The stored stamp is this very op's: a retry of an op whose log row was compacted away.
    Same,
}

/// The verdict for an `Update` that wrote `writes`: a retry (all registers already carry this op's stamp) is
/// recognized instead of logged again.
fn update_done(tx: &Transaction<'_>, writes: &[Write]) -> Result<Done, Fail> {
    if !writes.is_empty() && writes.iter().all(|w| *w == Write::Same) {
        Ok(Done::Existing(log_floor(tx)?))
    } else {
        Ok(Done::Applied)
    }
}

/// Writes `value` into `column` when `stamp` beats the stored `column_hlc`.
fn lww_column(
    tx: &Transaction<'_>,
    table: &str,
    id: &[u8; 16],
    column: &str,
    value: impl rusqlite::ToSql,
    stamp: penwise_proto::Hlc,
    seq: u64,
) -> Result<Write, Fail> {
    // `table` and `column` are string literals from this module, never caller data.
    let stored: Vec<u8> = tx.query_row(
        &format!("SELECT {column}_hlc FROM {table} WHERE id = ?1"),
        [&id[..]],
        |r| r.get(0),
    )?;
    let stored = parse_hlc(&stored)?;
    match stamp.cmp(&stored) {
        std::cmp::Ordering::Greater => {
            tx.execute(
                &format!(
                    "UPDATE {table} SET {column} = ?2, {column}_hlc = ?3, updated_seq = ?4 WHERE id = ?1"
                ),
                params![&id[..], value, stamp.to_bytes().as_slice(), to_i64(seq)],
            )?;
            Ok(Write::Won)
        }
        std::cmp::Ordering::Equal => Ok(Write::Same),
        std::cmp::Ordering::Less => Ok(Write::Lost),
    }
}

pub(super) fn apply_notebook(
    tx: &Transaction<'_>,
    op: &NotebookOp,
    seq: u64,
) -> Result<Done, Fail> {
    let created = created_hlc(tx, "notebooks", &op.notebook.0)?;
    let exists = created.is_some();
    let id = op.notebook.0;
    let at = op.hlc.to_bytes();
    match &op.change {
        NotebookChange::Create { title, position } => {
            if exists {
                return duplicate_create(tx, created, op.hlc);
            }
            tx.prepare_cached(
                "INSERT INTO notebooks (id, created_hlc, title, title_hlc, position, position_hlc,
                     deleted, deleted_hlc, updated_seq)
                 VALUES (?1, ?2, ?3, ?2, ?4, ?2, 0, ?2, ?5)",
            )?
            .execute(params![
                &id[..],
                at.as_slice(),
                title,
                position,
                to_i64(seq)
            ])?;
        }
        NotebookChange::Update(patch) => {
            if !exists {
                return Err(OpRejection::UnknownNotebook.into());
            }
            let mut writes = Vec::new();
            if let Some(title) = &patch.title {
                writes.push(lww_column(
                    tx,
                    "notebooks",
                    &id,
                    "title",
                    title,
                    op.hlc,
                    seq,
                )?);
            }
            if let Some(position) = patch.position {
                writes.push(lww_column(
                    tx,
                    "notebooks",
                    &id,
                    "position",
                    position,
                    op.hlc,
                    seq,
                )?);
            }
            if let Some(deleted) = patch.deleted {
                writes.push(lww_column(
                    tx,
                    "notebooks",
                    &id,
                    "deleted",
                    i64::from(deleted),
                    op.hlc,
                    seq,
                )?);
            }
            return update_done(tx, &writes);
        }
    }
    Ok(Done::Applied)
}

/// The `created_hlc` of row `id` in `table` (`notebooks` / `pages`), `None` when absent.
fn created_hlc(
    tx: &Transaction<'_>,
    table: &str,
    id: &[u8; 16],
) -> Result<Option<penwise_proto::Hlc>, Fail> {
    // `table` is a string literal from this module, never caller data.
    let stored: Option<Vec<u8>> = tx
        .query_row(
            &format!("SELECT created_hlc FROM {table} WHERE id = ?1"),
            [&id[..]],
            |r| r.get(0),
        )
        .optional()?;
    Ok(stored.map(|bytes| parse_hlc(&bytes)).transpose()?)
}

/// A `Create` of an existing row: the same creation stamp is a retry whose log row was compacted away
/// (recognized, nothing to log); any other stamp is a real duplicate.
fn duplicate_create(
    tx: &Transaction<'_>,
    created: Option<penwise_proto::Hlc>,
    stamp: penwise_proto::Hlc,
) -> Result<Done, Fail> {
    if created == Some(stamp) {
        Ok(Done::Existing(log_floor(tx)?))
    } else {
        Err(OpRejection::Duplicate.into())
    }
}

fn kind_columns(kind: PageKind) -> (i64, Option<f64>, Option<f64>) {
    match kind {
        PageKind::Fixed { width, height } => (0, Some(f64::from(width)), Some(f64::from(height))),
        PageKind::Infinite => (1, None, None),
    }
}

pub(super) fn apply_page(tx: &Transaction<'_>, op: &PageOp, seq: u64) -> Result<Done, Fail> {
    let created = created_hlc(tx, "pages", &op.page.0)?;
    let exists = created.is_some();
    let id = op.page.0;
    match &op.change {
        PageChange::Create {
            notebook,
            title,
            position,
            kind,
            background,
        } => {
            if exists {
                return duplicate_create(tx, created, op.hlc);
            }
            let known = tx
                .prepare_cached("SELECT 1 FROM notebooks WHERE id = ?1")?
                .exists([&notebook.0[..]])?;
            if !known {
                return Err(OpRejection::UnknownNotebook.into());
            }
            let (code, width, height) = kind_columns(*kind);
            tx.prepare_cached(
                "INSERT INTO pages (id, notebook, kind_code, width, height, created_hlc, title, title_hlc,
                     position, position_hlc, background, background_hlc, deleted, deleted_hlc, updated_seq)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?6, ?8, ?6, ?9, ?6, 0, ?6, ?10)",
            )?
            .execute(params![
                &id[..],
                &notebook.0[..],
                code,
                width,
                height,
                op.hlc.to_bytes().as_slice(),
                title,
                position,
                i64::from(background.code()),
                to_i64(seq),
            ])?;
        }
        PageChange::Update(patch) => {
            if !exists {
                return Err(OpRejection::UnknownPage.into());
            }
            let mut writes = Vec::new();
            if let Some(title) = &patch.title {
                writes.push(lww_column(tx, "pages", &id, "title", title, op.hlc, seq)?);
            }
            if let Some(position) = patch.position {
                writes.push(lww_column(
                    tx, "pages", &id, "position", position, op.hlc, seq,
                )?);
            }
            if let Some(background) = patch.background {
                let code = i64::from(Background::code(background));
                writes.push(lww_column(
                    tx,
                    "pages",
                    &id,
                    "background",
                    code,
                    op.hlc,
                    seq,
                )?);
            }
            if let Some(deleted) = patch.deleted {
                writes.push(lww_column(
                    tx,
                    "pages",
                    &id,
                    "deleted",
                    i64::from(deleted),
                    op.hlc,
                    seq,
                )?);
            }
            return update_done(tx, &writes);
        }
    }
    Ok(Done::Applied)
}

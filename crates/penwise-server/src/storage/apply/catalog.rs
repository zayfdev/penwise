//! Notebook and page ops: `Create` inserts every register at the op's stamp, `Update` overwrites each present
//! register iff its stamp wins (same rule as `penwise_proto::Catalog`).

use penwise_proto::{
    Background, NotebookChange, NotebookOp, OpRejection, PageChange, PageKind, PageOp,
};
use rusqlite::{Transaction, params};

use super::Fail;
use crate::storage::rows::{parse_hlc, to_i64};

/// Writes `value` into `column` when `stamp` beats the stored `column_hlc`.
fn lww_column(
    tx: &Transaction<'_>,
    table: &str,
    id: &[u8; 16],
    column: &str,
    value: impl rusqlite::ToSql,
    stamp: penwise_proto::Hlc,
    seq: u64,
) -> Result<(), Fail> {
    // `table` and `column` are string literals from this module, never caller data.
    let stored: Vec<u8> = tx.query_row(
        &format!("SELECT {column}_hlc FROM {table} WHERE id = ?1"),
        [&id[..]],
        |r| r.get(0),
    )?;
    if stamp > parse_hlc(&stored)? {
        tx.execute(
            &format!(
                "UPDATE {table} SET {column} = ?2, {column}_hlc = ?3, updated_seq = ?4 WHERE id = ?1"
            ),
            params![&id[..], value, stamp.to_bytes().as_slice(), to_i64(seq)],
        )?;
    }
    Ok(())
}

pub(super) fn apply_notebook(tx: &Transaction<'_>, op: &NotebookOp, seq: u64) -> Result<(), Fail> {
    let exists = tx
        .prepare_cached("SELECT 1 FROM notebooks WHERE id = ?1")?
        .exists([&op.notebook.0[..]])?;
    let id = op.notebook.0;
    let at = op.hlc.to_bytes();
    match &op.change {
        NotebookChange::Create { title, position } => {
            if exists {
                return Err(OpRejection::Duplicate.into());
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
            if let Some(title) = &patch.title {
                lww_column(tx, "notebooks", &id, "title", title, op.hlc, seq)?;
            }
            if let Some(position) = patch.position {
                lww_column(tx, "notebooks", &id, "position", position, op.hlc, seq)?;
            }
            if let Some(deleted) = patch.deleted {
                lww_column(
                    tx,
                    "notebooks",
                    &id,
                    "deleted",
                    i64::from(deleted),
                    op.hlc,
                    seq,
                )?;
            }
        }
    }
    Ok(())
}

fn kind_columns(kind: PageKind) -> (i64, Option<f64>, Option<f64>) {
    match kind {
        PageKind::Fixed { width, height } => (0, Some(f64::from(width)), Some(f64::from(height))),
        PageKind::Infinite => (1, None, None),
    }
}

pub(super) fn apply_page(tx: &Transaction<'_>, op: &PageOp, seq: u64) -> Result<(), Fail> {
    let exists = tx
        .prepare_cached("SELECT 1 FROM pages WHERE id = ?1")?
        .exists([&op.page.0[..]])?;
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
                return Err(OpRejection::Duplicate.into());
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
            if let Some(title) = &patch.title {
                lww_column(tx, "pages", &id, "title", title, op.hlc, seq)?;
            }
            if let Some(position) = patch.position {
                lww_column(tx, "pages", &id, "position", position, op.hlc, seq)?;
            }
            if let Some(background) = patch.background {
                let code = i64::from(Background::code(background));
                lww_column(tx, "pages", &id, "background", code, op.hlc, seq)?;
            }
            if let Some(deleted) = patch.deleted {
                lww_column(tx, "pages", &id, "deleted", i64::from(deleted), op.hlc, seq)?;
            }
        }
    }
    Ok(())
}

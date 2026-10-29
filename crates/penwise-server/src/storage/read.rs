//! Read-only pool and the read queries (each runs in one read transaction).

use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};

use penwise_core::item::{ItemId, Layer};
use penwise_proto::snapshot::ItemState;
use penwise_proto::{Catalog, NotebookId, Page, PageId, PageReplica, SeqOp, decode_op};
use rusqlite::{Connection, OptionalExtension};

use super::error::StorageError;
use super::rows::{corrupt, meta_get, meta_get_u64, to_i64, to_u64};
use super::snapshot::{for_item_batches, notebook_states, page_states};
use super::{AliasTarget, OpFilter, OpsPage, PageView, Status, schema};

/// Idle read-only connections; a read takes one (opening a new one when none is idle) and returns it.
#[derive(Debug)]
pub(crate) struct ReadPool {
    path: PathBuf,
    idle: Mutex<Vec<Connection>>,
}

impl ReadPool {
    pub(crate) fn new(path: PathBuf) -> Self {
        Self {
            path,
            idle: Mutex::new(Vec::new()),
        }
    }

    pub(crate) fn with<T>(
        &self,
        f: impl FnOnce(&Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        let pooled = self
            .idle
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .pop();
        let conn = match pooled {
            Some(conn) => conn,
            None => schema::open_reader(&self.path)?,
        };
        let result = f(&conn);
        let mut idle = self.idle.lock().unwrap_or_else(PoisonError::into_inner);
        if idle.len() < 8 {
            idle.push(conn);
        }
        result
    }
}

/// `(head_seq, log_floor)` inside the caller's transaction.
pub(crate) fn head_and_floor(conn: &Connection) -> Result<(u64, u64), StorageError> {
    let head: Option<i64> = conn
        .query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'ops'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    let floor = meta_get_u64(conn, "log_floor")?.unwrap_or(0);
    Ok((head.map_or(0, to_u64), floor))
}

pub(crate) fn status(conn: &Connection) -> Result<Status, StorageError> {
    let tx = conn.unchecked_transaction()?;
    let (head_seq, log_floor) = head_and_floor(&tx)?;
    Ok(Status {
        head_seq,
        log_floor,
    })
}

pub(crate) fn ops_since(
    conn: &Connection,
    after_seq: u64,
    filter: OpFilter,
    limit: usize,
) -> Result<OpsPage, StorageError> {
    let tx = conn.unchecked_transaction()?;
    let (head_seq, floor) = head_and_floor(&tx)?;
    if after_seq < floor {
        return Err(StorageError::BelowFloor { floor });
    }
    let limit = limit.max(1);
    let (sql, page): (&str, Option<Vec<u8>>) = match filter {
        OpFilter::All => (
            "SELECT seq, body FROM ops WHERE seq > ?1 AND (?2 IS NULL OR 1) ORDER BY seq LIMIT ?3",
            None,
        ),
        OpFilter::CatalogAnd(page) => (
            "SELECT seq, body FROM ops WHERE seq > ?1 AND (kind <> 0 OR page = ?2) ORDER BY seq LIMIT ?3",
            page.map(|p| p.0.to_vec()),
        ),
    };
    let mut stmt = tx.prepare(sql)?;
    let mut rows = stmt.query(rusqlite::params![
        to_i64(after_seq),
        page,
        i64::try_from(limit).unwrap_or(i64::MAX)
    ])?;
    let mut ops = Vec::new();
    while let Some(row) = rows.next()? {
        let seq: i64 = row.get(0)?;
        let body: Vec<u8> = row.get(1)?;
        let op = decode_op(&body).map_err(|e| corrupt("stored op", &e))?;
        ops.push(SeqOp {
            seq: to_u64(seq),
            op,
        });
    }
    let (upto_seq, more) = match ops.last() {
        Some(last) if ops.len() == limit && last.seq < head_seq => (last.seq, true),
        _ => (head_seq, false),
    };
    Ok(OpsPage {
        ops,
        upto_seq,
        head_seq,
        more,
    })
}

pub(crate) fn page_view(conn: &Connection, page: PageId) -> Result<Option<PageView>, StorageError> {
    let tx = conn.unchecked_transaction()?;
    let Some(state) = page_states(&tx, Some(page))?.into_iter().next() else {
        return Ok(None);
    };
    let mut items = Vec::new();
    for_item_batches(&tx, Some(page), |batch: Vec<ItemState>| {
        for row in batch {
            items.push(row.to_item().map_err(|e| corrupt("item row", &e))?);
        }
        Ok(())
    })?;
    let updated_seq = tx.query_row(
        "SELECT MAX((SELECT updated_seq FROM pages WHERE id = ?1),
                    COALESCE((SELECT MAX(updated_seq) FROM items WHERE page = ?1), 0))",
        [&page.0[..]],
        |r| r.get::<_, i64>(0),
    )?;
    let replica = PageReplica::from_items(state.kind, items)
        .map_err(|e| StorageError::Corrupt(format!("page items: {e}")))?;
    Ok(Some(PageView {
        page: Page {
            id: state.id,
            notebook: state.notebook,
            title: state.title,
            position: state.position,
            kind: state.kind,
            background: state.background,
            deleted: state.deleted,
        },
        replica,
        updated_seq: to_u64(updated_seq),
    }))
}

pub(crate) fn catalog(conn: &Connection) -> Result<Catalog, StorageError> {
    let tx = conn.unchecked_transaction()?;
    let mut catalog = Catalog::new();
    for notebook in notebook_states(&tx)? {
        catalog
            .merge_notebook_state(&notebook)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
    }
    for page in page_states(&tx, None)? {
        catalog
            .merge_page_state(&page)
            .map_err(|e| StorageError::Corrupt(e.to_string()))?;
    }
    Ok(catalog)
}

pub(crate) fn live_item_ids(
    conn: &Connection,
    page: PageId,
    layer: Option<Layer>,
) -> Result<Vec<ItemId>, StorageError> {
    let mut stmt = conn.prepare(
        "SELECT id FROM items WHERE page = ?1 AND deleted = 0 AND (?2 IS NULL OR layer = ?2) ORDER BY z, id",
    )?;
    let rows = stmt.query_map(
        rusqlite::params![&page.0[..], layer.map(|l| i64::from(l.code()))],
        |r| r.get::<_, [u8; 16]>(0),
    )?;
    Ok(rows
        .collect::<Result<Vec<_>, _>>()?
        .into_iter()
        .map(ItemId)
        .collect())
}

/// The page holding `id` (live or deleted). Item ids are page-local but globally unique, so one row answers an
/// item-addressed route.
pub(crate) fn page_of_item(conn: &Connection, id: ItemId) -> Result<Option<PageId>, StorageError> {
    let row: Option<[u8; 16]> = conn
        .query_row("SELECT page FROM items WHERE id = ?1", [&id.0[..]], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(row.and_then(|bytes| PageId::from_slice(&bytes)))
}

pub(crate) fn resolve_alias(
    conn: &Connection,
    legacy: &str,
) -> Result<Option<AliasTarget>, StorageError> {
    let row: Option<(i64, Vec<u8>)> = conn
        .query_row(
            "SELECT kind, id FROM id_alias WHERE legacy = ?1",
            [legacy],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?;
    Ok(row.and_then(|(kind, id)| match kind {
        0 => NotebookId::from_slice(&id).map(AliasTarget::Notebook),
        1 => PageId::from_slice(&id).map(AliasTarget::Page),
        _ => None,
    }))
}

pub(crate) fn current_page(conn: &Connection) -> Result<Option<PageId>, StorageError> {
    Ok(meta_get(conn, "current_page")?.and_then(|b| PageId::from_slice(&b)))
}

/// The op-log seq `agent` last read marks up to (`None` before its first call).
pub(crate) fn agent_checkpoint(
    conn: &Connection,
    agent: &str,
) -> Result<Option<u64>, StorageError> {
    let seq: Option<i64> = conn
        .query_row(
            "SELECT seq FROM agent_checkpoints WHERE agent = ?1",
            [agent],
            |r| r.get(0),
        )
        .optional()?;
    Ok(seq.map(to_u64))
}

/// `(id, updated_seq)` of every item row of `page` (tombstones included; callers intersect with the live scene).
pub(crate) fn item_seqs(
    conn: &Connection,
    page: PageId,
) -> Result<Vec<(ItemId, u64)>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, updated_seq FROM items WHERE page = ?1")?;
    let rows = stmt.query_map([&page.0[..]], |r| {
        Ok((r.get::<_, [u8; 16]>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, seq) = row?;
        out.push((ItemId(id), to_u64(seq)));
    }
    Ok(out)
}

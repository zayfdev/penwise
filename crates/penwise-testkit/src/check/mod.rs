//! Convergence helpers: comparable fingerprints of replicas and of the server's `SQLite` tables.
//!
//! The server DB is read directly (read-only, schema v4 of the design): the log (`ops`) folded through a proto
//! [`Replica`] is the reference; the materialized tables must equal it ("`items` = `Replica` fold").

use std::collections::BTreeMap;
use std::path::Path;

use penwise_core::item::ItemId;
use penwise_proto::{AnyOp, PageId, Replica, SeqOp, decode_op};
use rusqlite::{Connection, OpenFlags};

mod tables;

pub use tables::load_tables;

/// Comparable state: catalog registers and every item with its stamps. Ignores the replica cursor.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct Fingerprint {
    /// Notebook and page registers, `Debug`-rendered, sorted.
    pub catalog: Vec<String>,
    /// `(page, item)` → `Debug` of item + deleted flag + stamps.
    pub items: BTreeMap<(PageId, ItemId), String>,
}

impl Fingerprint {
    /// Live (not deleted) item count on `page`.
    #[must_use]
    pub fn live_on(&self, page: PageId) -> usize {
        self.items
            .iter()
            .filter(|((p, _), v)| *p == page && !v.contains("deleted: true"))
            .count()
    }
}

/// Fingerprint of a replica.
#[must_use]
pub fn fingerprint(replica: &Replica) -> Fingerprint {
    let catalog = replica.catalog();
    let mut out = Fingerprint::default();
    let mut pages = Vec::new();
    let mut notebooks = catalog.notebooks();
    notebooks.extend(catalog.deleted_notebooks());
    for nb in &notebooks {
        out.catalog.push(format!("{nb:?}"));
        let mut list = catalog.pages(nb.id);
        list.extend(catalog.deleted_pages(nb.id));
        for page in list {
            out.catalog.push(format!("{page:?}"));
            pages.push(page.id);
        }
    }
    out.catalog.sort();
    for page in pages {
        let Some(replica_page) = replica.page(page) else {
            continue;
        };
        for m in replica_page.items() {
            out.items.insert(
                (page, m.item.id),
                format!("{:?}|deleted: {}|{:?}", m.item, m.deleted, m.stamps),
            );
        }
    }
    out
}

/// Opens the server DB read-only.
///
/// # Errors
/// The `SQLite` error text.
pub fn open_ro(path: &Path) -> Result<Connection, String> {
    Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY).map_err(|e| e.to_string())
}

/// Highest `seq` in the log (0 when empty).
///
/// # Errors
/// The `SQLite` error text.
pub fn db_head(path: &Path) -> Result<u64, String> {
    let conn = open_ro(path)?;
    let head: i64 = conn
        .query_row("SELECT COALESCE(MAX(seq), 0) FROM ops", [], |r| r.get(0))
        .map_err(|e| e.to_string())?;
    Ok(u64::try_from(head).unwrap_or(0))
}

/// All `seq` values of the log, ascending.
///
/// # Errors
/// The `SQLite` error text.
pub fn db_seqs(path: &Path) -> Result<Vec<u64>, String> {
    let conn = open_ro(path)?;
    let mut stmt = conn
        .prepare("SELECT seq FROM ops ORDER BY seq")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| r.get::<_, i64>(0))
        .map_err(|e| e.to_string())?;
    rows.map(|r| {
        r.map(|v| u64::try_from(v).unwrap_or(0))
            .map_err(|e| e.to_string())
    })
    .collect()
}

/// Folds the `ops` table (in `seq` order) through a fresh replica: the reference state.
///
/// # Errors
/// `SQLite` or decode error text.
pub fn fold_ops(path: &Path) -> Result<Replica, String> {
    let conn = open_ro(path)?;
    let mut stmt = conn
        .prepare("SELECT seq, body FROM ops ORDER BY seq")
        .map_err(|e| e.to_string())?;
    let rows = stmt
        .query_map([], |r| Ok((r.get::<_, i64>(0)?, r.get::<_, Vec<u8>>(1)?)))
        .map_err(|e| e.to_string())?;
    let mut replica = Replica::new();
    for row in rows {
        let (seq, body) = row.map_err(|e| e.to_string())?;
        let op: AnyOp = decode_op(&body).map_err(|e| e.to_string())?;
        let seq = u64::try_from(seq).map_err(|e| e.to_string())?;
        replica
            .apply_seq(&SeqOp { seq, op })
            .map_err(|e| format!("seq {seq}: {e}"))?;
    }
    Ok(replica)
}

/// `server_meta.log_floor` (0 when the log was never compacted).
///
/// # Errors
/// The `SQLite` error text.
pub fn log_floor(path: &Path) -> Result<u64, String> {
    let conn = open_ro(path)?;
    let bytes: Option<Vec<u8>> = conn
        .query_row(
            "SELECT value FROM server_meta WHERE key = 'log_floor'",
            [],
            |r| r.get(0),
        )
        .ok();
    Ok(bytes
        .and_then(|b| <[u8; 8]>::try_from(b.as_slice()).ok())
        .map_or(0, u64::from_le_bytes))
}

/// Asserts the materialized tables equal the log fold; returns the fingerprint.
///
/// # Errors
/// Describes the first divergence (counts and first differing key) or any read error.
pub fn tables_equal_fold(path: &Path) -> Result<Fingerprint, String> {
    if log_floor(path)? > 0 {
        // A compacted log no longer replays from zero: the tables are the only full state.
        return Ok(fingerprint(&load_tables(path)?));
    }
    let folded = fingerprint(&fold_ops(path)?);
    let tables = fingerprint(&load_tables(path)?);
    if folded == tables {
        return Ok(folded);
    }
    let first = folded
        .items
        .iter()
        .find(|(k, v)| tables.items.get(k) != Some(v))
        .map(|(k, v)| format!("{k:?}: fold={v} table={:?}", tables.items.get(k)));
    Err(format!(
        "tables != fold: items {} vs {}, catalog {} vs {}; first diff {first:?}",
        folded.items.len(),
        tables.items.len(),
        folded.catalog.len(),
        tables.catalog.len()
    ))
}

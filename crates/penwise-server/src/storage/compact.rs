//! Retention (owner decision 2026-10-27): the log and item tombstones live [`RETENTION_DAYS`] days; a device
//! unseen for [`DEVICE_IDLE_DAYS`] stops holding the floor. Compaction raises `log_floor`; a client that has
//! seen less than the floor gets a snapshot instead of a diff.

use penwise_core::edit::Field;
use penwise_proto::{FieldStamps, Hlc};
use rusqlite::{Connection, OptionalExtension, TransactionBehavior, params};

use super::error::StorageError;
use super::read::head_and_floor;
use super::rows::{meta_put_u64, to_i64};
use super::{DEVICE_IDLE_DAYS, RETENTION_DAYS};

const DAY_MS: u64 = 86_400_000;

/// What one compaction did.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CompactReport {
    /// `log_floor` after the run.
    pub log_floor: u64,
    pub ops_deleted: u64,
    pub items_purged: u64,
}

/// Rows deleted per transaction: the writer thread is shared with every push and ack, so one run never holds
/// it for long ([`Storage::compact`](super::Storage::compact) calls [`step`] until nothing is left).
pub const BATCH_ROWS: usize = 5_000;
/// Free pages returned to the OS per batch when the database uses `auto_vacuum = INCREMENTAL` (a no-op for the
/// default `NONE`; changing that on an existing file needs a full `VACUUM`, which this server never runs).
const VACUUM_PAGES: u32 = 1_000;

/// One bounded slice of retention.
#[derive(Debug, Clone, Copy)]
pub struct Step {
    /// What this slice did (`log_floor` is the floor after it).
    pub report: CompactReport,
    /// More rows are waiting: call again (other writer commands run in between).
    pub more: bool,
}

/// Runs one retention slice in one transaction: raises `log_floor` (together with the first deletes, so a
/// reader never sees a log with holes below the floor) and deletes at most [`BATCH_ROWS`] old ops and purges at
/// most [`BATCH_ROWS`] tombstoned items.
///
/// Devices of `kind = 1` (browsers) never hold the floor back: they keep no cursor and refetch a snapshot.
///
/// # Errors
/// `Sqlite`, `Corrupt`.
pub fn step(conn: &mut Connection, now_ms: u64) -> Result<Step, StorageError> {
    let cutoff = to_i64(now_ms.saturating_sub(RETENTION_DAYS * DAY_MS));
    let idle_cutoff = to_i64(now_ms.saturating_sub(DEVICE_IDLE_DAYS * DAY_MS));
    let tx = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
    let (head, old_floor) = head_and_floor(&tx)?;
    let retained: i64 = tx.query_row(
        "SELECT COALESCE(MAX(seq), 0) FROM ops WHERE received_ms < ?1",
        [cutoff],
        |r| r.get(0),
    )?;
    let min_ack: Option<i64> = tx
        .query_row(
            "SELECT MIN(acked_seq) FROM devices WHERE last_seen_ms > ?1 AND kind != 1",
            [idle_cutoff],
            |r| r.get(0),
        )
        .optional()?
        .flatten();
    let floor = u64::try_from(retained)
        .unwrap_or(0)
        .min(min_ack.map_or(head, |a| u64::try_from(a).unwrap_or(0)))
        .min(head)
        .max(old_floor);
    let leftover: bool = tx.query_row(
        "SELECT EXISTS (SELECT 1 FROM ops WHERE seq <= ?1)",
        [to_i64(floor)],
        |r| r.get(0),
    )?;
    if floor == old_floor && !leftover {
        return Ok(Step {
            report: CompactReport {
                log_floor: floor,
                ops_deleted: 0,
                items_purged: 0,
            },
            more: false,
        });
    }
    meta_put_u64(&tx, "log_floor", floor)?;
    let ops_deleted = tx.execute(
        "DELETE FROM ops WHERE seq IN (SELECT seq FROM ops WHERE seq <= ?1 ORDER BY seq LIMIT ?2)",
        params![to_i64(floor), i64::try_from(BATCH_ROWS).unwrap_or(i64::MAX)],
    )?;
    let purge = purgeable_items(&tx, floor, u64::try_from(cutoff).unwrap_or(0))?;
    for (id, created) in &purge {
        // Remember the id forever: a late `Add` of a purged item must not bring it back.
        tx.execute(
            "INSERT OR IGNORE INTO purged_items (id, created_hlc, purged_ms) VALUES (?1, ?2, ?3)",
            params![&id[..], created.to_bytes().as_slice(), to_i64(now_ms)],
        )?;
        tx.execute("DELETE FROM image_labels WHERE item = ?1", [&id[..]])?;
        tx.execute("DELETE FROM items WHERE id = ?1", [&id[..]])?;
    }
    tx.commit()?;
    if ops_deleted > 0 || !purge.is_empty() {
        // Returns freed pages when the file is `auto_vacuum = INCREMENTAL`; otherwise nothing happens.
        conn.execute_batch(&format!("PRAGMA incremental_vacuum({VACUUM_PAGES})"))?;
    }
    Ok(Step {
        report: CompactReport {
            log_floor: floor,
            ops_deleted: ops_deleted as u64,
            items_purged: purge.len() as u64,
        },
        more: ops_deleted >= BATCH_ROWS || purge.len() >= BATCH_ROWS,
    })
}

/// Tombstoned items whose tombstone is older than the cutoff and whose last write is at or below `floor`.
fn purgeable_items(
    tx: &rusqlite::Transaction<'_>,
    floor: u64,
    cutoff_ms: u64,
) -> Result<Vec<([u8; 16], Hlc)>, StorageError> {
    let mut stmt =
        tx.prepare("SELECT id, stamps FROM items WHERE deleted = 1 AND updated_seq <= ?1")?;
    let rows = stmt.query_map([to_i64(floor)], |r| {
        Ok((r.get::<_, [u8; 16]>(0)?, r.get::<_, Vec<u8>>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, stamps) = row?;
        let stamps = FieldStamps::decode(&stamps)
            .map_err(|e| StorageError::Corrupt(format!("item stamps: {e}")))?;
        if stamps.get(Field::Deleted).wall_ms < cutoff_ms {
            out.push((id, stamps.created));
            if out.len() >= BATCH_ROWS {
                break;
            }
        }
    }
    Ok(out)
}

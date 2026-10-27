//! Retention (owner decision 2026-10-27): the log and item tombstones live [`RETENTION_DAYS`] days; a device
//! unseen for [`DEVICE_IDLE_DAYS`] stops holding the floor. Compaction raises `log_floor`; a client that has
//! seen less than the floor gets a snapshot instead of a diff.

use penwise_core::edit::Field;
use penwise_proto::FieldStamps;
use rusqlite::{Connection, OptionalExtension, TransactionBehavior};

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

/// Runs retention in one transaction.
///
/// # Errors
/// `Sqlite`, `Corrupt`.
pub fn run(conn: &mut Connection, now_ms: u64) -> Result<CompactReport, StorageError> {
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
            "SELECT MIN(acked_seq) FROM devices WHERE last_seen_ms > ?1",
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
    if floor == old_floor {
        return Ok(CompactReport {
            log_floor: floor,
            ops_deleted: 0,
            items_purged: 0,
        });
    }
    let ops_deleted = tx.execute("DELETE FROM ops WHERE seq <= ?1", [to_i64(floor)])?;
    let purge = purgeable_items(&tx, floor, u64::try_from(cutoff).unwrap_or(0))?;
    for id in &purge {
        tx.execute("DELETE FROM image_labels WHERE item = ?1", [&id[..]])?;
        tx.execute("DELETE FROM items WHERE id = ?1", [&id[..]])?;
    }
    meta_put_u64(&tx, "log_floor", floor)?;
    tx.commit()?;
    Ok(CompactReport {
        log_floor: floor,
        ops_deleted: ops_deleted as u64,
        items_purged: purge.len() as u64,
    })
}

/// Tombstoned items whose tombstone is older than the cutoff and whose last write is at or below `floor`.
fn purgeable_items(
    tx: &rusqlite::Transaction<'_>,
    floor: u64,
    cutoff_ms: u64,
) -> Result<Vec<[u8; 16]>, StorageError> {
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
            out.push(id);
        }
    }
    Ok(out)
}

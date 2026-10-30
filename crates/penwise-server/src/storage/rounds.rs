//! Design-harness rounds (spec v2 §4): one request from the owner, the marks drawn since the last one was sent.
//!
//! A round is a server-local row, never synced: `open → sent → resolved`. Marks belong to a round by the op-log
//! seq that added their `Mark` item, so rounds need no core or codec change: round *n* holds the marks added
//! after round *n−1*'s `sent_seq`, up to round *n*'s own `sent_seq`; the open round holds everything after the
//! last `sent_seq`. All writes run on the writer thread ([`apply`]); reads use the reader pool.

use rusqlite::{Connection, OptionalExtension, Row, params};

use super::error::StorageError;
use super::read::head_and_floor;
use super::rows::{to_i64, to_u64};

/// Where a round is in its life. Codes are stored in `rounds.status`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RoundStatus {
    /// Collecting marks; not yet sent.
    Open,
    /// Closed by Send; waiting for the agent.
    Sent,
    /// The agent finished it: snapshot placed, frame moved on.
    Resolved,
}

impl RoundStatus {
    fn code(self) -> i64 {
        match self {
            Self::Open => 0,
            Self::Sent => 1,
            Self::Resolved => 2,
        }
    }

    fn from_code(code: i64) -> Result<Self, StorageError> {
        match code {
            0 => Ok(Self::Open),
            1 => Ok(Self::Sent),
            2 => Ok(Self::Resolved),
            other => Err(StorageError::Corrupt(format!("round status {other}"))),
        }
    }

    /// The lowercase name the API and the `look` text use.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Open => "open",
            Self::Sent => "sent",
            Self::Resolved => "resolved",
        }
    }
}

/// One `rounds` row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Round {
    pub project: String,
    pub n: u32,
    pub status: RoundStatus,
    /// Op-log head when Send closed the round.
    pub sent_seq: Option<u64>,
    pub resolved_seq: Option<u64>,
    pub sent_ms: Option<u64>,
    pub resolved_ms: Option<u64>,
    /// When the agent last read the round with `look`.
    pub looked_ms: Option<u64>,
}

/// A change to the `rounds` table, run by the writer thread.
#[derive(Debug, Clone)]
pub(crate) enum RoundOp {
    /// The project's open round, created when there is none.
    EnsureOpen { project: String },
    /// Compare-and-set Send: closes round `n` as `sent` at the current log head, but only while `n` is still the
    /// project's open round (created when there is none). Anything else is a conflict (`None`).
    Send { project: String, n: u32 },
    /// Closes round `n` as resolved; an open round is sent in the same step.
    Resolve { project: String, n: u32 },
    /// Records that the agent read round `n`.
    Looked { project: String, n: u32 },
}

const COLUMNS: &str = "project, n, status, sent_seq, resolved_seq, sent_ms, resolved_ms, looked_ms";

fn round_of(row: &Row<'_>) -> Result<Round, StorageError> {
    let opt = |index: usize| -> Result<Option<u64>, rusqlite::Error> {
        Ok(row.get::<_, Option<i64>>(index)?.map(to_u64))
    };
    Ok(Round {
        project: row.get(0)?,
        n: u32::try_from(row.get::<_, i64>(1)?).unwrap_or(u32::MAX),
        status: RoundStatus::from_code(row.get(2)?)?,
        sent_seq: opt(3)?,
        resolved_seq: opt(4)?,
        sent_ms: opt(5)?,
        resolved_ms: opt(6)?,
        looked_ms: opt(7)?,
    })
}

/// Every round of `project`, oldest first.
pub(crate) fn list(conn: &Connection, project: &str) -> Result<Vec<Round>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM rounds WHERE project = ?1 ORDER BY n"
    ))?;
    let mut rows = stmt.query([project])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        out.push(round_of(row)?);
    }
    Ok(out)
}

fn get(conn: &Connection, project: &str, n: u32) -> Result<Option<Round>, StorageError> {
    let mut stmt = conn.prepare(&format!(
        "SELECT {COLUMNS} FROM rounds WHERE project = ?1 AND n = ?2"
    ))?;
    let mut rows = stmt.query(params![project, i64::from(n)])?;
    rows.next()?.map(round_of).transpose()
}

/// The open round, inserting row `max(n) + 1` when the project has none.
fn ensure_open(conn: &Connection, project: &str) -> Result<Round, StorageError> {
    let open: Option<i64> = conn
        .query_row(
            "SELECT n FROM rounds WHERE project = ?1 AND status = 0",
            [project],
            |r| r.get(0),
        )
        .optional()?;
    let n = if let Some(n) = open {
        n
    } else {
        let next: i64 = conn.query_row(
            "SELECT COALESCE(MAX(n), 0) + 1 FROM rounds WHERE project = ?1",
            [project],
            |r| r.get(0),
        )?;
        conn.execute(
            "INSERT INTO rounds (project, n, status) VALUES (?1, ?2, 0)",
            params![project, next],
        )?;
        next
    };
    get(conn, project, u32::try_from(n).unwrap_or(u32::MAX))?
        .ok_or_else(|| StorageError::Corrupt("round vanished".to_owned()))
}

/// Runs one [`RoundOp`] in a transaction; `None` when `Resolve`/`Looked` name a round that does not exist or
/// `Send` names a round that is no longer open.
pub(crate) fn apply(
    conn: &mut Connection,
    op: &RoundOp,
    now_ms: u64,
) -> Result<Option<Round>, StorageError> {
    let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
    let (head, _) = head_and_floor(&tx)?;
    let round = match op {
        RoundOp::EnsureOpen { project } => Some(ensure_open(&tx, project)?),
        RoundOp::Send { project, n } => {
            let open = ensure_open(&tx, project)?;
            if open.n == *n {
                send(&tx, &open, head, now_ms)?;
                get(&tx, project, open.n)?
            } else {
                None
            }
        }
        RoundOp::Resolve { project, n } => match get(&tx, project, *n)? {
            None => None,
            Some(found) => {
                if found.status == RoundStatus::Open {
                    send(&tx, &found, head, now_ms)?;
                }
                tx.execute(
                    "UPDATE rounds SET status = ?3, resolved_seq = COALESCE(resolved_seq, ?4),
                         resolved_ms = COALESCE(resolved_ms, ?5)
                     WHERE project = ?1 AND n = ?2",
                    params![
                        project,
                        i64::from(*n),
                        RoundStatus::Resolved.code(),
                        to_i64(head),
                        to_i64(now_ms)
                    ],
                )?;
                get(&tx, project, *n)?
            }
        },
        RoundOp::Looked { project, n } => {
            tx.execute(
                "UPDATE rounds SET looked_ms = ?3 WHERE project = ?1 AND n = ?2",
                params![project, i64::from(*n), to_i64(now_ms)],
            )?;
            get(&tx, project, *n)?
        }
    };
    tx.commit()?;
    Ok(round)
}

fn send(conn: &Connection, round: &Round, head: u64, now_ms: u64) -> Result<(), StorageError> {
    conn.execute(
        "UPDATE rounds SET status = ?3, sent_seq = ?4, sent_ms = ?5 WHERE project = ?1 AND n = ?2",
        params![
            round.project,
            i64::from(round.n),
            RoundStatus::Sent.code(),
            to_i64(head),
            to_i64(now_ms)
        ],
    )?;
    Ok(())
}

/// A frame or mark item with the seqs of the ops that added and last changed it.
#[derive(Debug, Clone)]
pub struct LedgerRow {
    pub item: penwise_core::item::Item,
    pub added_seq: u64,
    pub updated_seq: u64,
}

/// The live frame and mark items of `page` and nothing else: what the round ledger needs, without reading the
/// strokes and pictures that make up most of a page.
pub(crate) fn frames_and_marks(
    conn: &Connection,
    page: penwise_proto::PageId,
) -> Result<Vec<LedgerRow>, StorageError> {
    use penwise_core::item::ItemKindTag;
    let mut stmt = conn.prepare(
        "SELECT body, patch, stamps, added_seq, updated_seq FROM items
         WHERE page = ?1 AND deleted = 0 AND kind IN (?2, ?3)",
    )?;
    let mut rows = stmt.query(params![
        &page.0[..],
        i64::from(ItemKindTag::Frame.code()),
        i64::from(ItemKindTag::Mark.code())
    ])?;
    let mut out = Vec::new();
    while let Some(row) = rows.next()? {
        let state = penwise_proto::snapshot::ItemState {
            page,
            body: row.get(0)?,
            patch: row.get(1)?,
            stamps: row.get(2)?,
            deleted: false,
        };
        let item = state
            .to_item()
            .map_err(|error| StorageError::Corrupt(format!("item row: {error}")))?
            .item;
        out.push(LedgerRow {
            item,
            added_seq: to_u64(row.get(3)?),
            updated_seq: to_u64(row.get(4)?),
        });
    }
    Ok(out)
}

/// `(id, added_seq)` of every item row of `page`: the seq of the op that added it (marks pick their round by it).
pub(crate) fn added_seqs(
    conn: &Connection,
    page: penwise_proto::PageId,
) -> Result<Vec<(penwise_core::item::ItemId, u64)>, StorageError> {
    let mut stmt = conn.prepare("SELECT id, added_seq FROM items WHERE page = ?1")?;
    let rows = stmt.query_map([&page.0[..]], |r| {
        Ok((r.get::<_, [u8; 16]>(0)?, r.get::<_, i64>(1)?))
    })?;
    let mut out = Vec::new();
    for row in rows {
        let (id, seq) = row?;
        out.push((penwise_core::item::ItemId(id), to_u64(seq)));
    }
    Ok(out)
}

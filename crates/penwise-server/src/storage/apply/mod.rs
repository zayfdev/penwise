//! Appending a frame of ops: validation order of design §2.4, the log row, and LWW materialization.
//!
//! Everything here runs inside the writer's transaction. A frame is all-or-nothing: if any op is refused the
//! caller rolls the transaction back.

mod catalog;
mod items;

use penwise_proto::hlc::MAX_SKEW_MS;
use penwise_proto::outbox::MAX_OP_BYTES;
use penwise_proto::{AnyOp, DeviceId, Hlc, OpRejection, SeqOp, encode_op};
use rusqlite::{OptionalExtension, Transaction, params};

use super::error::{AppendError, Rejected, StorageError};
use super::rows::{to_i64, to_u64};
use super::{Appended, FaultPlan};

/// Why one op could not be applied.
#[derive(Debug)]
pub(crate) enum Fail {
    Rejected(OpRejection),
    Db(StorageError),
}

impl From<rusqlite::Error> for Fail {
    fn from(error: rusqlite::Error) -> Self {
        Self::Db(error.into())
    }
}

impl From<StorageError> for Fail {
    fn from(error: StorageError) -> Self {
        Self::Db(error)
    }
}

impl From<OpRejection> for Fail {
    fn from(reason: OpRejection) -> Self {
        Self::Rejected(reason)
    }
}

/// The log floor: an upper bound for the seq of any op whose log row is gone (compacted).
pub(super) fn log_floor(tx: &Transaction<'_>) -> Result<u64, StorageError> {
    Ok(super::read::head_and_floor(tx)?.1)
}

/// What materializing one op did.
pub(crate) enum Done {
    /// Applied; the op needs its log row.
    Applied,
    /// A retry of an op that was already applied (same creation stamp / same register stamps) whose log row
    /// was compacted away: the original seq (an upper bound, the log floor, when it is no longer known).
    Existing(u64),
}

/// Result of a committed-to-be frame.
pub(crate) struct FrameResult {
    pub appended: Vec<Appended>,
    /// Newly logged ops, ascending seq (broadcast after commit).
    pub new_ops: Vec<SeqOp>,
    pub max_hlc: Option<Hlc>,
}

/// The session's view of the frame.
pub(crate) struct FrameEnv<'a> {
    pub device: DeviceId,
    pub now_ms: u64,
    pub fault: Option<&'a FaultPlan>,
}

/// Next seq to hand out: the `AUTOINCREMENT` high-water mark + 1 (survives compaction).
pub(crate) fn next_seq(tx: &Transaction<'_>) -> Result<u64, StorageError> {
    let last: Option<i64> = tx
        .query_row(
            "SELECT seq FROM sqlite_sequence WHERE name = 'ops'",
            [],
            |r| r.get(0),
        )
        .optional()?;
    Ok(to_u64(last.unwrap_or(0)) + 1)
}

fn stateless(env: &FrameEnv<'_>, op: &AnyOp) -> Result<(), OpRejection> {
    if op.device() != env.device || op.hlc().device != env.device {
        return Err(OpRejection::NotOwnDevice);
    }
    if op.hlc().wall_ms > env.now_ms.saturating_add(MAX_SKEW_MS) {
        return Err(OpRejection::ClockSkew);
    }
    op.validate()?;
    // The same cap the clients enforce at authoring: every logged op fits an `OpsSince` frame with room to spare.
    match encode_op(op) {
        Ok(body) if body.len() <= MAX_OP_BYTES => Ok(()),
        _ => Err(OpRejection::Invalid),
    }
}

fn existing_seq(tx: &Transaction<'_>, op: &AnyOp) -> Result<Option<u64>, rusqlite::Error> {
    tx.query_row(
        "SELECT seq FROM ops WHERE op_id = ?1",
        [&op.op_id().0[..]],
        |r| r.get::<_, i64>(0),
    )
    .optional()
    .map(|seq| seq.map(to_u64))
}

/// Applies `ops` in order. `Ok` only when every op was accepted.
///
/// # Errors
/// `Rejected` listing every refused op (the caller rolls back), or `Storage`.
pub(crate) fn append_frame(
    tx: &Transaction<'_>,
    env: &FrameEnv<'_>,
    ops: &[AnyOp],
) -> Result<FrameResult, AppendError> {
    let batch = next_seq(tx)?;
    let mut seq = batch;
    let mut result = FrameResult {
        appended: Vec::with_capacity(ops.len()),
        new_ops: Vec::new(),
        max_hlc: None,
    };
    let mut rejected = Vec::new();
    let mut inserted = 0_usize;
    for op in ops {
        match apply_checked(tx, env, op, seq, batch) {
            Ok(appended) if appended.duplicate => result.appended.push(appended),
            Ok(appended) => {
                result.appended.push(appended);
                result.new_ops.push(SeqOp {
                    seq,
                    op: op.clone(),
                });
                result.max_hlc = result.max_hlc.max(Some(op.hlc()));
                seq += 1;
                inserted += 1;
                if env
                    .fault
                    .is_some_and(|f| f.after_ops == inserted && f.fire(inserted))
                {
                    return Err(StorageError::Fault.into());
                }
            }
            Err(Fail::Rejected(reason)) => rejected.push(Rejected {
                op_id: op.op_id(),
                reason,
            }),
            Err(Fail::Db(error)) => return Err(error.into()),
        }
    }
    if !rejected.is_empty() {
        return Err(AppendError::Rejected(rejected));
    }
    if env.fault.is_some_and(|f| f.fire(inserted)) {
        return Err(StorageError::Fault.into());
    }
    Ok(result)
}

/// Checks one op (stateless, dedupe) and, if new, materializes and logs it as `seq`.
fn apply_checked(
    tx: &Transaction<'_>,
    env: &FrameEnv<'_>,
    op: &AnyOp,
    seq: u64,
    batch: u64,
) -> Result<Appended, Fail> {
    stateless(env, op)?;
    let duplicate = |original: u64| Appended {
        op_id: op.op_id(),
        seq: original,
        duplicate: true,
    };
    if let Some(original) = existing_seq(tx, op)? {
        return Ok(duplicate(original));
    }
    match materialize(tx, op, seq)? {
        Done::Existing(original) => Ok(duplicate(original)),
        Done::Applied => {
            log_op(tx, op, seq, batch, env.now_ms)?;
            Ok(Appended {
                op_id: op.op_id(),
                seq,
                duplicate: false,
            })
        }
    }
}

/// References + LWW merge of one op into the materialized rows (no stateless checks, no log row).
pub(crate) fn materialize(tx: &Transaction<'_>, op: &AnyOp, seq: u64) -> Result<Done, Fail> {
    match op {
        AnyOp::Item(op) => items::apply(tx, op, seq),
        AnyOp::Page(op) => catalog::apply_page(tx, op, seq),
        AnyOp::Notebook(op) => catalog::apply_notebook(tx, op, seq),
    }
}

/// Inserts the `ops` row for `op` at `seq`.
pub(crate) fn log_op(
    tx: &Transaction<'_>,
    op: &AnyOp,
    seq: u64,
    batch: u64,
    received_ms: u64,
) -> Result<(), Fail> {
    let body = encode_op(op).map_err(|e| StorageError::Corrupt(format!("encode op: {e}")))?;
    let notebook: Option<[u8; 16]> = match op {
        AnyOp::Notebook(n) => Some(n.notebook.0),
        AnyOp::Page(p) => match &p.change {
            penwise_proto::PageChange::Create { notebook, .. } => Some(notebook.0),
            penwise_proto::PageChange::Update(_) => None,
        },
        AnyOp::Item(_) => None,
    };
    tx.prepare_cached(
        "INSERT INTO ops (seq, op_id, batch, device, hlc, kind, notebook, page, item, body, received_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11)",
    )?
    .execute(params![
        to_i64(seq),
        &op.op_id().0[..],
        to_i64(batch),
        &op.device().0[..],
        op.hlc().to_bytes().as_slice(),
        i64::from(op.kind_code()),
        notebook.as_ref().map(|b| &b[..]),
        op.page().map(|p| p.0).as_ref().map(|b| &b[..]),
        op.item().map(|i| i.0).as_ref().map(|b| &b[..]),
        body,
        to_i64(received_ms),
    ])?;
    Ok(())
}

//! The append-only op log and the per-op validation + materialization transaction.
//!
//! Owner: slice a.
//!
//! `append_ops` runs **one transaction per call** (one `PushOps` frame). For each op, in push order:
//! 1. `op_id` already logged → [`AppendOutcome::Duplicate`] with the original seq (nothing else happens;
//!    idempotent outbox replay, also when the duplicate's body differs).
//! 2. `op.device()` (and `hlc.device`) ≠ `device` → `Rejected(NotOwnDevice)`.
//! 3. `hlc.wall_ms > now_ms + MAX_SKEW_MS` → `Rejected(ClockSkew)`.
//! 4. [`AnyOp::validate`] → `Rejected(Invalid)` / `Rejected(NotOwnDevice)`.
//! 5. Stateful create-before-reference checks against the materialized tables: `UnknownNotebook`,
//!    `UnknownPage`, `UnknownItem`, `Duplicate` (create of an existing id), `WrongPage` (item op whose page
//!    differs from the item row's page).
//! 6. `INSERT INTO ops` (seq assigned), then materialize: notebook/page ops → `pages.rs` register merge;
//!    item ops → `items.rs` (`MaterializedItem::from_add` / `merge`); bump `updated_ms` of the page and its
//!    notebook to `max(updated_ms, hlc.wall_ms)`.
//!
//! Earlier ops of the same batch are visible to later ones (a page create then its strokes in one frame).
//! A SQLite error rolls back the whole call; rejections never do.

use penwise_proto::{AnyOp, DeviceId, Hlc, OpRejection, SeqOp};

use super::{Storage, StorageError};

/// Result for one pushed op.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AppendOutcome {
    /// Newly logged at `seq`.
    Logged { seq: u64 },
    /// `op_id` was already logged at `seq`.
    Duplicate { seq: u64 },
    /// Refused; not logged.
    Rejected(OpRejection),
}

impl Storage {
    /// Validates, logs and materializes `ops` from the authenticated `device` (module steps 1–6). Returns one
    /// outcome per op, in order.
    ///
    /// # Errors
    /// [`StorageError`] (transaction rolled back, nothing logged).
    pub fn append_ops(
        &self,
        device: DeviceId,
        ops: &[AnyOp],
        now_ms: u64,
    ) -> Result<Vec<AppendOutcome>, StorageError> {
        todo!()
    }

    /// Logged ops with `seq > from_seq`, ascending, at most `limit`.
    ///
    /// # Errors
    /// [`StorageError::Corrupt`] for an undecodable body; SQLite errors.
    pub fn ops_since(&self, from_seq: u64, limit: usize) -> Result<Vec<SeqOp>, StorageError> {
        todo!()
    }

    /// Largest logged seq (`0` for an empty log).
    ///
    /// # Errors
    /// SQLite errors.
    pub fn head_seq(&self) -> Result<u64, StorageError> {
        todo!()
    }

    /// Largest logged stamp (`MAX(hlc)`), to resume a server-side `HlcClock` (server-authored ops, M5).
    ///
    /// # Errors
    /// [`StorageError::Corrupt`]; SQLite errors.
    pub fn max_hlc(&self) -> Result<Option<Hlc>, StorageError> {
        todo!()
    }
}

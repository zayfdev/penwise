//! Client-side outbox: ops created locally and not yet acknowledged (design §1.6, §5.1).
//!
//! Pure and durable-friendly. Entries hold each op's stored envelope bytes ([`crate::encode_op`]), so a host
//! can keep them as `SQLite` rows ([`Outbox::pending_entries`] / [`Outbox::from_parts`]) or as one blob
//! ([`Outbox::to_bytes`] / [`Outbox::from_bytes`]). The lifecycle of an op:
//!
//! 1. `insert` when the op is created (before it is applied locally or sent), creation order = push order;
//! 2. `next_batch` → `ClientMsg::PushOps` (ops stay pending until acknowledged; a re-push after a reconnect or
//!    restart is safe because the server dedupes by `op_id`);
//! 3. `ack` when the `Ack` lists the op as logged (or an `OpsSince` echo of it arrives), or `reject` when the
//!    `Ack` lists it as refused: the op moves to the **dead letter** list, which is kept (shown in the UI,
//!    exportable) and never dropped by the outbox itself.
//!
//! There is no in-flight state: after a restart every pending op is simply pushed again.

use std::collections::{BTreeSet, VecDeque};

use serde::{Deserialize, Serialize};

use crate::envelope::{MAX_FRAME_BYTES, decode_op, encode_op};
use crate::error::ProtoError;
use crate::ids::OpId;
use crate::message::{MAX_OPS_PER_FRAME, OpRejected};
use crate::op::{AnyOp, OpRejection};

/// Leading byte of [`Outbox::to_bytes`].
pub const OUTBOX_VERSION: u8 = 1;
/// Soft byte budget of one `PushOps` batch (leaves room under `MAX_FRAME_BYTES` for the frame overhead).
pub const MAX_BATCH_BYTES: usize = MAX_FRAME_BYTES - (MAX_FRAME_BYTES >> 3);

/// Largest stored envelope of one op the outbox accepts (a quarter of a frame, so a logged op can never
/// push an `OpsSince` or `PushOps` frame over `MAX_FRAME_BYTES`). Authoring refuses larger ops.
pub const MAX_OP_BYTES: usize = MAX_FRAME_BYTES / 4;

/// Checks that an op carrying `edit` stays under [`MAX_OP_BYTES`] (envelope overhead included, with margin).
/// Run before an edit is applied to a scene, so a refused edit never reaches the screen.
///
/// # Errors
/// The size the op would have, when it is too large.
pub fn check_edit_size(edit: &penwise_core::edit::Edit) -> Result<(), usize> {
    let mut blob = Vec::new();
    penwise_core::codec::edit::encode_edit(edit, &mut blob);
    let size = blob.len() + 1024;
    if size <= MAX_OP_BYTES {
        Ok(())
    } else {
        Err(size)
    }
}

/// One pending op: its id and stored envelope bytes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct Entry {
    op_id: OpId,
    bytes: Vec<u8>,
}

/// An op the server refused (or that cannot be decoded), kept for the user.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DeadOp {
    pub op_id: OpId,
    /// Stored envelope bytes of the op ([`crate::decode_op`]).
    pub bytes: Vec<u8>,
    pub reason: OpRejection,
}

/// Serialized form.
#[derive(Serialize, Deserialize)]
struct Stored {
    pending: Vec<Entry>,
    dead: Vec<DeadOp>,
}

/// Pending ops in creation order plus the dead-letter list.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outbox {
    pending: VecDeque<Entry>,
    dead: Vec<DeadOp>,
    ids: BTreeSet<OpId>,
}

impl Outbox {
    /// Empty outbox.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Queues `op` at the end. Returns false (and queues nothing) when an op with the same id is already
    /// pending or dead-lettered.
    ///
    /// # Errors
    /// `Encode` / `TooLarge` when the op cannot be framed or its envelope exceeds [`MAX_OP_BYTES`].
    pub fn insert(&mut self, op: &AnyOp) -> Result<bool, ProtoError> {
        let op_id = op.op_id();
        if self.ids.contains(&op_id) {
            return Ok(false);
        }
        let bytes = encode_op(op)?;
        if bytes.len() > MAX_OP_BYTES {
            return Err(ProtoError::TooLarge { len: bytes.len() });
        }
        self.ids.insert(op_id);
        self.pending.push_back(Entry { op_id, bytes });
        Ok(true)
    }

    /// Removes one pending op without acknowledging it (it was queued but its local apply failed).
    pub fn forget_pending(&mut self, op_id: OpId) -> bool {
        let Some(at) = self.pending.iter().position(|e| e.op_id == op_id) else {
            return false;
        };
        self.pending.remove(at);
        self.ids.remove(&op_id);
        true
    }

    /// The next `PushOps` batch: pending ops in creation order, at most `MAX_OPS_PER_FRAME` and about
    /// `MAX_BATCH_BYTES` (always at least one op). The ops stay pending. An entry that no longer decodes
    /// (a corrupted row) is moved to the dead letters as `Invalid` instead of blocking the queue.
    pub fn next_batch(&mut self) -> Vec<AnyOp> {
        let mut batch = Vec::new();
        let mut bytes = 0_usize;
        let mut undecodable = Vec::new();
        for entry in &self.pending {
            if batch.len() >= MAX_OPS_PER_FRAME {
                break;
            }
            if !batch.is_empty() && bytes + entry.bytes.len() > MAX_BATCH_BYTES {
                break;
            }
            match decode_op(&entry.bytes) {
                Ok(op) => {
                    bytes += entry.bytes.len();
                    batch.push(op);
                }
                Err(_) => undecodable.push(entry.op_id),
            }
        }
        for op_id in undecodable {
            self.move_to_dead(op_id, OpRejection::Invalid);
        }
        batch
    }

    /// Every pending op, decoded, in creation order (to re-apply to a replica rebuilt from a snapshot).
    /// Entries that do not decode are skipped; [`Outbox::next_batch`] quarantines them.
    #[must_use]
    pub fn pending_ops(&self) -> Vec<AnyOp> {
        self.pending
            .iter()
            .filter_map(|e| decode_op(&e.bytes).ok())
            .collect()
    }

    /// Replaces every pending op by `ops` (in order; dead letters stay). Pending entries that no longer decode
    /// are quarantined as `Invalid` first. Ops that do not fit ([`Outbox::insert`] fails) are skipped; returns
    /// how many.
    pub fn replace_pending(&mut self, ops: &[AnyOp]) -> usize {
        let undecodable: Vec<OpId> = self
            .pending
            .iter()
            .filter(|e| decode_op(&e.bytes).is_err())
            .map(|e| e.op_id)
            .collect();
        for op_id in undecodable {
            self.move_to_dead(op_id, OpRejection::Invalid);
        }
        for entry in self.pending.drain(..) {
            if !self.dead.iter().any(|d| d.op_id == entry.op_id) {
                self.ids.remove(&entry.op_id);
            }
        }
        ops.iter()
            .filter(|op| !matches!(self.insert(op), Ok(true)))
            .count()
    }

    /// Pending `(op id, envelope bytes)` in creation order (one `SQLite` row each).
    pub fn pending_entries(&self) -> impl Iterator<Item = (OpId, &[u8])> {
        self.pending.iter().map(|e| (e.op_id, e.bytes.as_slice()))
    }

    /// Number of pending ops.
    #[must_use]
    pub fn len(&self) -> usize {
        self.pending.len()
    }

    /// True when nothing is pending (dead letters do not count).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.pending.is_empty()
    }

    /// True when `op_id` is pending.
    #[must_use]
    pub fn contains(&self, op_id: OpId) -> bool {
        self.pending.iter().any(|e| e.op_id == op_id)
    }

    /// Drops the pending ops the server logged. Ids that are not pending (a repeated `Ack`) are ignored.
    /// Returns how many were removed.
    pub fn ack(&mut self, op_ids: &[OpId]) -> usize {
        let acked: BTreeSet<OpId> = op_ids.iter().copied().collect();
        let before = self.pending.len();
        self.pending.retain(|e| !acked.contains(&e.op_id));
        for op_id in &acked {
            if !self.dead.iter().any(|d| d.op_id == *op_id) {
                self.ids.remove(op_id);
            }
        }
        before - self.pending.len()
    }

    /// Moves the pending ops the server refused to the dead letters with their reason. Ids that are not
    /// pending are ignored. Returns how many moved.
    pub fn reject(&mut self, rejected: &[OpRejected]) -> usize {
        rejected
            .iter()
            .filter(|r| self.move_to_dead(r.op_id, r.reason))
            .count()
    }

    /// Dead-lettered ops, oldest first.
    #[must_use]
    pub fn dead(&self) -> &[DeadOp] {
        &self.dead
    }

    /// Removes one dead letter (after the user exported or dismissed it).
    pub fn remove_dead(&mut self, op_id: OpId) -> Option<DeadOp> {
        let at = self.dead.iter().position(|d| d.op_id == op_id)?;
        self.ids.remove(&op_id);
        Some(self.dead.remove(at))
    }

    /// Rebuilds an outbox from stored rows (pending in creation order).
    ///
    /// # Errors
    /// `Invalid` when an op id appears twice.
    pub fn from_parts(
        pending: impl IntoIterator<Item = (OpId, Vec<u8>)>,
        dead: Vec<DeadOp>,
    ) -> Result<Self, ProtoError> {
        let mut outbox = Self::new();
        for (op_id, bytes) in pending {
            if !outbox.ids.insert(op_id) {
                return Err(ProtoError::Invalid("duplicate op id in outbox"));
            }
            outbox.pending.push_back(Entry { op_id, bytes });
        }
        for letter in &dead {
            if !outbox.ids.insert(letter.op_id) {
                return Err(ProtoError::Invalid("duplicate op id in outbox"));
            }
        }
        outbox.dead = dead;
        Ok(outbox)
    }

    /// `[OUTBOX_VERSION]` + postcard of every pending entry and dead letter.
    ///
    /// # Errors
    /// `Encode`.
    pub fn to_bytes(&self) -> Result<Vec<u8>, ProtoError> {
        let stored = Stored {
            pending: self.pending.iter().cloned().collect(),
            dead: self.dead.clone(),
        };
        postcard::to_extend(&stored, vec![OUTBOX_VERSION]).map_err(ProtoError::Encode)
    }

    /// Inverse of [`Outbox::to_bytes`].
    ///
    /// # Errors
    /// `Empty`, `UnsupportedVersion`, `Decode`, `TrailingBytes`, `Invalid` (duplicate op id).
    pub fn from_bytes(bytes: &[u8]) -> Result<Self, ProtoError> {
        let (&version, body) = bytes.split_first().ok_or(ProtoError::Empty)?;
        if version != OUTBOX_VERSION {
            return Err(ProtoError::UnsupportedVersion(version));
        }
        let (stored, rest) =
            postcard::take_from_bytes::<Stored>(body).map_err(ProtoError::Decode)?;
        if !rest.is_empty() {
            return Err(ProtoError::TrailingBytes { count: rest.len() });
        }
        Self::from_parts(
            stored.pending.into_iter().map(|e| (e.op_id, e.bytes)),
            stored.dead,
        )
    }

    fn move_to_dead(&mut self, op_id: OpId, reason: OpRejection) -> bool {
        let Some(at) = self.pending.iter().position(|e| e.op_id == op_id) else {
            return false;
        };
        if let Some(entry) = self.pending.remove(at) {
            self.dead.push(DeadOp {
                op_id,
                bytes: entry.bytes,
                reason,
            });
        }
        true
    }
}

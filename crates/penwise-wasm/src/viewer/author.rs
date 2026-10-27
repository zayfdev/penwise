//! The browser's own authoring state: device identity, calibrated clock, id stream, undo stacks and the
//! outbox of local ops not yet acknowledged. Same session rules as the tablet's `SyncCore`
//! (`penwise-ffi`): pushes only after `Welcome`, never during a snapshot, one `PushOps` in flight, the
//! outbox re-applied on top of every snapshot.

use std::collections::BTreeMap;

use penwise_core::brush::BrushParams;
use penwise_core::edit::UndoStack;
use penwise_core::item::ItemId;
use penwise_core::sample::Sample;
use penwise_proto::message::OpRejected;
use penwise_proto::{
    ClientMsg, ClockOffset, DeadOp, DeviceId, Hlc, HlcClock, HlcError, IdMint, OpId, Outbox,
    PageId, decode_op, encode_client,
};

use crate::error::WasmError;

/// Leading byte of [`Author::state_bytes`].
pub const STATE_VERSION: u8 = 1;

/// A pen stroke being drawn (preview deltas sent, `Add` not yet committed).
#[derive(Debug)]
pub struct PenStroke {
    pub page: PageId,
    pub id: ItemId,
    pub params: BrushParams,
    /// Encoded `params` (every delta repeats it).
    pub brush: Vec<u8>,
    pub seed: u64,
    /// Quantized samples so far; the committed item stores exactly these.
    pub samples: Vec<Sample>,
}

/// Device identity and local-edit bookkeeping of one browser tab.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent session-state flags, each toggled by its own event"
)]
#[derive(Debug)]
pub struct Author {
    pub(super) device: DeviceId,
    hlc: HlcClock,
    offset: ClockOffset,
    /// Newest remote stamp seen; folded into the clock at the next local stamp.
    newest_remote: Option<Hlc>,
    pub(super) mint: IdMint,
    outbox: Outbox,
    /// Largest "remote stamp ahead of this clock" seen at a stamp, until reported ([`Author::take_clock_skew`]).
    skew_ms: u64,
    /// Bumped whenever the outbox changes (the host persists [`Author::state_bytes`] when it moves).
    revision: u64,
    /// Restored from storage and the pending ops are not yet shown in the (rebuilt) replica.
    restored: bool,
    pub(super) undo: BTreeMap<PageId, UndoStack>,
    hello_at: u64,
    welcomed: bool,
    inflight: bool,
    snapshotting: bool,
    pub(super) pen: Option<PenStroke>,
}

impl Author {
    /// Fresh state for `device`; `entropy` seeds the id stream.
    #[must_use]
    pub fn new(device: DeviceId, entropy: u64) -> Self {
        Self {
            device,
            hlc: HlcClock::new(device),
            offset: ClockOffset::ZERO,
            newest_remote: None,
            mint: IdMint::new(device, entropy),
            outbox: Outbox::new(),
            skew_ms: 0,
            revision: 0,
            restored: false,
            undo: BTreeMap::new(),
            hello_at: 0,
            welcomed: false,
            inflight: false,
            snapshotting: false,
            pen: None,
        }
    }

    /// A new connection starts (`Hello` is built at `now_ms`; the `Welcome` calibrates against it).
    pub fn begin_session(&mut self, now_ms: u64) {
        self.hello_at = now_ms;
        self.welcomed = false;
        self.inflight = false;
        self.snapshotting = false;
    }

    /// `Welcome`: calibrate the clock and allow pushes.
    pub fn on_welcome(&mut self, server_now_ms: u64) {
        self.offset = ClockOffset::from_welcome(server_now_ms, self.hello_at);
        self.welcomed = true;
        self.inflight = false;
    }

    /// `Ack`: drop acknowledged ops, dead-letter rejected ones, re-open the push gate.
    /// Returns how many ops moved to the dead letters (their effects are still in the replica).
    pub fn on_ack(&mut self, acked: &[OpId], rejected: &[OpRejected]) -> usize {
        let refused = self.outbox.reject(rejected);
        if self.outbox.ack(acked) + refused > 0 {
            self.revision += 1;
        }
        self.inflight = false;
        refused
    }

    /// A snapshot starts (pushes pause) / ends.
    pub fn set_snapshotting(&mut self, snapshotting: bool) {
        self.snapshotting = snapshotting;
        if snapshotting {
            self.inflight = false;
        }
    }

    /// The socket closed: nothing is in flight (the outbox is kept).
    pub fn go_offline(&mut self) {
        self.welcomed = false;
        self.inflight = false;
        self.snapshotting = false;
    }

    /// Remember a stamp from the server log so local stamps exceed it.
    pub fn observe(&mut self, remote: Hlc) {
        if self.newest_remote.is_none_or(|seen| remote > seen) {
            self.newest_remote = Some(remote);
        }
    }

    /// Stamp for a local op at the calibrated clock.
    pub fn stamp(&mut self, now_ms: u64) -> Hlc {
        let corrected = self.offset.adjust(now_ms);
        if let Some(remote) = self.newest_remote.take()
            && let Err(HlcError::ClockSkew { ahead_ms }) =
                self.hlc.observe_logged(remote, corrected)
        {
            self.skew_ms = self.skew_ms.max(ahead_ms);
        }
        self.hlc.send(corrected)
    }

    /// How far (ms) a stamp from the server log was ahead of this device's clock at the last local stamp;
    /// 0 when none. Reading resets it. The clock adopted that stamp, so new ops may be refused as `ClockSkew`.
    pub fn take_clock_skew(&mut self) -> u64 {
        std::mem::take(&mut self.skew_ms)
    }

    /// Ops queued and not yet acknowledged.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.outbox.len()
    }

    /// Queues a stamped op.
    pub fn queue(&mut self, op: &penwise_proto::AnyOp) -> Result<(), WasmError> {
        if self.outbox.insert(op)? {
            self.revision += 1;
        }
        Ok(())
    }

    /// True once after [`Author::from_state`]: the caller shows the pending ops in the replica.
    pub fn take_restored(&mut self) -> bool {
        std::mem::take(&mut self.restored)
    }

    /// Counter that moves whenever [`Author::state_bytes`] would change.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// Ops the server refused, oldest first.
    #[must_use]
    pub fn dead(&self) -> &[DeadOp] {
        self.outbox.dead()
    }

    /// Drops one dead letter (dismissed or exported by the user).
    pub fn remove_dead(&mut self, op_id: OpId) -> Option<DeadOp> {
        let letter = self.outbox.remove_dead(op_id);
        if letter.is_some() {
            self.revision += 1;
        }
        letter
    }

    /// What survives a page reload: `[STATE_VERSION]` + device id + [`Outbox::to_bytes`] (pending ops and
    /// dead letters). Their stamps also restore the clock floor, so ops made after a reload still sort after
    /// the ones made before it.
    ///
    /// # Errors
    /// Encode failure (never expected).
    pub fn state_bytes(&self) -> Result<Vec<u8>, WasmError> {
        let mut out = vec![STATE_VERSION];
        out.extend_from_slice(&self.device.to_bytes());
        out.extend_from_slice(&self.outbox.to_bytes()?);
        Ok(out)
    }

    /// Inverse of [`Author::state_bytes`]; `entropy` reseeds the id stream.
    ///
    /// # Errors
    /// [`WasmError::BadArgument`] for a blob of another version or length, [`WasmError::Proto`] for a damaged
    /// outbox.
    pub fn from_state(bytes: &[u8], entropy: u64) -> Result<Self, WasmError> {
        let (&version, rest) = bytes
            .split_first()
            .ok_or(WasmError::BadArgument("empty state"))?;
        if version != STATE_VERSION || rest.len() < 16 {
            return Err(WasmError::BadArgument("unsupported state"));
        }
        let (device, outbox) = rest.split_at(16);
        let device = DeviceId::from_slice(device).ok_or(WasmError::BadArgument("state device"))?;
        let mut author = Self::new(device, entropy);
        author.outbox = Outbox::from_bytes(outbox)?;
        let stamps = author
            .outbox
            .pending_ops()
            .into_iter()
            .chain(
                author
                    .outbox
                    .dead()
                    .iter()
                    .filter_map(|d| decode_op(&d.bytes).ok()),
            )
            .map(|op| op.hlc())
            .collect::<Vec<_>>();
        for hlc in stamps {
            author.observe(hlc);
        }
        author.restored = true;
        Ok(author)
    }

    /// Queued ops in order (re-applied on top of a snapshot).
    #[must_use]
    pub fn pending_ops(&self) -> Vec<penwise_proto::AnyOp> {
        self.outbox.pending_ops()
    }

    /// The next `PushOps` frame when the session may send one.
    ///
    /// # Errors
    /// Encode failure (never expected).
    pub fn poll_push(&mut self) -> Result<Option<Vec<u8>>, WasmError> {
        if !self.welcomed || self.snapshotting || self.inflight || self.outbox.is_empty() {
            return Ok(None);
        }
        let dead = self.outbox.dead().len();
        let ops = self.outbox.next_batch();
        if self.outbox.dead().len() != dead {
            self.revision += 1;
        }
        if ops.is_empty() {
            return Ok(None);
        }
        let frame = encode_client(&ClientMsg::PushOps { ops })?;
        self.inflight = true;
        Ok(Some(frame))
    }
}

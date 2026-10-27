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
    ClientMsg, ClockOffset, DeviceId, Hlc, HlcClock, OpId, Outbox, PageId, encode_client,
};

use crate::error::WasmError;
use crate::mint::IdMint;

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
#[derive(Debug)]
pub struct Author {
    pub(super) device: DeviceId,
    hlc: HlcClock,
    offset: ClockOffset,
    /// Newest remote stamp seen; folded into the clock at the next local stamp.
    newest_remote: Option<Hlc>,
    pub(super) mint: IdMint,
    outbox: Outbox,
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
    pub fn on_ack(&mut self, acked: &[OpId], rejected: &[OpRejected]) {
        self.outbox.ack(acked);
        self.outbox.reject(rejected);
        self.inflight = false;
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
        if let Some(remote) = self.newest_remote.take() {
            let _ = self.hlc.recv(remote, corrected);
        }
        self.hlc.send(corrected)
    }

    /// Ops queued and not yet acknowledged.
    #[must_use]
    pub fn pending(&self) -> usize {
        self.outbox.len()
    }

    /// Queues a stamped op.
    pub fn queue(&mut self, op: &penwise_proto::AnyOp) -> Result<(), WasmError> {
        self.outbox.insert(op)?;
        Ok(())
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
        let ops = self.outbox.next_batch();
        if ops.is_empty() {
            return Ok(None);
        }
        let frame = encode_client(&ClientMsg::PushOps { ops })?;
        self.inflight = true;
        Ok(Some(frame))
    }
}

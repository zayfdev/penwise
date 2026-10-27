//! `SyncCore`: everything the tablet's sync client needs that is not I/O: the device's [`HlcClock`] with its
//! server-calibrated [`ClockOffset`], the durable [`Outbox`], the local [`Replica`], the pinned server id and the
//! `/sync` frame codec. Bytes in, bytes out; Kotlin only moves WebSocket frames between the socket and this type.
//!
//! Session flow (design §4.3): `hello` → socket → `receive(Welcome …)` → `receive(OpsSince | Snapshot* …)` →
//! `poll_push` after every `receive` and every local commit. A `receive` that reports `reconnect` means the
//! server identity changed: the replica was reset (outbox kept), close the socket and `hello` again.
//!
//! Local edits made by [`crate::scene_handle::SceneHandle`] enter through [`SyncCore::commit_edits`], which stamps
//! them (`HlcClock::send` on the calibrated clock), applies them to the replica (the LWW authority) and queues
//! them in the outbox.

mod authoring;
mod catalog_blob;
mod legacy;
mod live;
mod outcome;
mod session;
mod state;

pub use authoring::NewPage;
pub use legacy::{ImportReport, parse_page_file};
pub use live::LiveChunk;
pub use outcome::{Change, Outcome, decode_changes};

use penwise_proto::{ClockOffset, DeviceId, Hlc, HlcClock, Outbox, PageId, Replica, ServerId};
use std::collections::BTreeSet;

use crate::ids::IdMint;

/// The tablet's sync state.
#[derive(Debug)]
pub struct SyncCore {
    device: DeviceId,
    hlc: HlcClock,
    offset: ClockOffset,
    replica: Replica,
    outbox: Outbox,
    pinned: Option<ServerId>,
    ids: IdMint,
    /// Local clock reading when the last `Hello` was built (calibration reference for `Welcome`).
    hello_at: u64,
    /// `Welcome` seen on this connection (pushes are allowed).
    welcomed: bool,
    /// A `PushOps` frame is out and its `Ack` has not arrived.
    inflight: bool,
    snapshotting: bool,
    /// Pages a running snapshot may have changed (reported as whole-page reloads at its end).
    snapshot_pages: BTreeSet<PageId>,
}

impl SyncCore {
    /// Fresh state for `device`; `entropy` seeds the id stream (Kotlin: `SecureRandom.nextLong()`).
    #[must_use]
    pub fn new(device: DeviceId, entropy: u64) -> Self {
        Self::assemble(
            device,
            HlcClock::new(device),
            entropy,
            Replica::new(),
            Outbox::new(),
        )
    }

    fn assemble(
        device: DeviceId,
        hlc: HlcClock,
        entropy: u64,
        replica: Replica,
        outbox: Outbox,
    ) -> Self {
        Self {
            device,
            hlc,
            offset: ClockOffset::ZERO,
            replica,
            outbox,
            pinned: None,
            ids: IdMint::new(device, entropy),
            hello_at: 0,
            welcomed: false,
            inflight: false,
            snapshotting: false,
            snapshot_pages: BTreeSet::new(),
        }
    }

    /// This device's id.
    #[must_use]
    pub fn device(&self) -> DeviceId {
        self.device
    }

    /// The local replica (read-only: every write goes through `receive` or a commit).
    #[must_use]
    pub fn replica(&self) -> &Replica {
        &self.replica
    }

    /// The durable queue of local ops not yet acknowledged.
    #[must_use]
    pub fn outbox(&self) -> &Outbox {
        &self.outbox
    }

    /// Highest applied server `seq` (the next `Hello`'s `last_seen_seq`).
    #[must_use]
    pub fn head_seq(&self) -> u64 {
        self.replica.head_seq()
    }

    /// True when the connection is settled: welcomed, no snapshot running, nothing queued or in flight.
    #[must_use]
    pub fn is_settled(&self) -> bool {
        self.welcomed && !self.snapshotting && !self.inflight && self.outbox.is_empty()
    }

    /// The socket closed: nothing is in flight any more (the outbox is kept).
    pub fn go_offline(&mut self) {
        self.welcomed = false;
        self.inflight = false;
        self.snapshotting = false;
    }

    /// Clock-corrected wall time for stamping and `recv`.
    fn local_now(&self, now_ms: u64) -> u64 {
        self.offset.adjust(now_ms)
    }

    fn stamp(&mut self, now_ms: u64) -> Hlc {
        let corrected = self.local_now(now_ms);
        self.hlc.send(corrected)
    }
}

#[cfg(test)]
mod tests;

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

mod apply;
mod authoring;
mod catalog_blob;
mod dead;
mod export;
mod hold;
mod legacy;
mod live;
mod outcome;
mod refusal;
mod session;
mod state;

pub use authoring::NewPage;
pub use dead::{DeadInfo, encode_dead_letters};
pub use export::StateSnapshot;
pub use legacy::{ImportReport, parse_page_file};
pub use live::LiveChunk;
pub use outcome::{Change, Outcome, decode_changes};

use penwise_proto::{
    ClockOffset, DeviceId, Hlc, HlcClock, IdMint, Outbox, PageId, Replica, ServerId,
};
use std::collections::BTreeSet;

/// The tablet's sync state.
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent connection-state flags"
)]
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
    /// `Welcome.protocol_version` of this connection (`0` before the `Welcome`).
    server_protocol: u16,
    /// A `PushOps` frame is out and its `Ack` has not arrived.
    inflight: bool,
    snapshotting: bool,
    /// Scope of the snapshot in progress (decides whether its end moves the cursor).
    snapshot_scope: penwise_proto::Scope,
    /// Pages a running snapshot may have changed (reported as whole-page reloads at its end).
    snapshot_pages: BTreeSet<PageId>,
    /// `Welcome.head_seq` of this connection.
    welcome_head: u64,
    /// The replica reached the `Welcome` head (or a snapshot ended) on this connection.
    caught_up: bool,
    /// A different server answered and the user has not decided yet: nothing from it is applied or pushed.
    held: Option<ServerId>,
    /// Newest stamp seen in anything the server sent (floor when the clock is rebased).
    remote_high: Hlc,
    /// A `ClockSkew` refusal was already retried and no clean `Ack` came since (a second one is dead-lettered).
    restamped: bool,
    /// Bumped by every change of the persistable state ([`SyncCore::revision`]).
    revision: u64,
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
            server_protocol: 0,
            inflight: false,
            snapshotting: false,
            snapshot_pages: BTreeSet::new(),
            snapshot_scope: penwise_proto::Scope::All,
            welcome_head: 0,
            caught_up: false,
            held: None,
            remote_high: Hlc::zero(device),
            restamped: false,
            revision: 0,
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

    /// True once the replica holds everything the server had when this connection began: its head reached
    /// `Welcome.head_seq`, or a snapshot ended. False before the `Welcome`, while a snapshot runs, while a
    /// server change is held and after the socket closed. A legacy import before this point can disagree
    /// with the server's migrated copy.
    #[must_use]
    pub fn is_caught_up(&self) -> bool {
        self.welcomed && self.caught_up && !self.snapshotting && self.held.is_none()
    }

    /// Changes whenever the persistable state ([`SyncCore::export_state`]) changed: persist when it differs
    /// from the value at the last persist.
    #[must_use]
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The socket closed: nothing is in flight any more (the outbox is kept).
    pub fn go_offline(&mut self) {
        self.welcomed = false;
        self.inflight = false;
        self.snapshotting = false;
        self.caught_up = false;
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

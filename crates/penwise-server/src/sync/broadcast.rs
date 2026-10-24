//! Fan-out of logged ops and live traffic to every session (server-stack.md §2 "Fan-out").
//!
//! Owner: slice b.
//!
//! One global `tokio::sync::broadcast` channel. Each message is encoded **once** (`encode_server`) and shared
//! as `Bytes` (refcounted, no per-socket re-encode). Sessions filter by [`Outbound::origin`]/[`Outbound::echo`].
//! A receiver that falls `BROADCAST_CAPACITY` messages behind gets `RecvError::Lagged`: its session sends
//! `ServerMsg::Resync { from_seq: sent_seq }`, drops live state and catches up from storage. Live
//! `StrokeDelta`s lost that way are acceptable (ephemeral by contract).
//!
//! **Ordering.** Ops are broadcast after their transaction commits, one frame per `PushOps`
//! (`OpsSince { from_seq: first.seq − 1, ops, head_seq, more: false }`). Two pushes may broadcast out of seq
//! order; sessions handle it with `sent_seq` (session.rs): drop ops with `seq ≤ sent_seq`, fill a gap
//! (`first.seq > sent_seq + 1`) from storage first.

use axum::body::Bytes;
use penwise_proto::{DeviceId, PageId, ProtoError, SeqOp, StrokeCancel, StrokeDelta};
use tokio::sync::broadcast;

/// Channel capacity (messages, not bytes).
pub const BROADCAST_CAPACITY: usize = 1024;

/// One encoded server frame on the channel.
#[derive(Debug, Clone)]
pub struct Outbound {
    /// Device whose action produced it.
    pub origin: DeviceId,
    /// Whether sessions of `origin` also receive it (ops: yes, so every client sees its seq; live ink and
    /// page switches: no).
    pub echo: bool,
    /// For op frames: the logged seq range `first..=last`; `None` for live traffic.
    pub seqs: Option<(u64, u64)>,
    pub frame: Bytes,
}

/// Sender side (clone-free: lives in the hub).
#[derive(Debug)]
pub struct Broadcast {
    tx: broadcast::Sender<Outbound>,
}

impl Broadcast {
    /// Channel with `capacity` slots.
    #[must_use]
    pub fn new(capacity: usize) -> Self {
        todo!()
    }

    /// New receiver (call **before** catch-up so nothing logged in between is missed).
    #[must_use]
    pub fn subscribe(&self) -> broadcast::Receiver<Outbound> {
        todo!()
    }

    /// Broadcasts newly logged ops (non-empty, ascending seq) as one `OpsSince` frame, `echo = true`.
    /// No receivers is not an error.
    ///
    /// # Errors
    /// [`ProtoError`] when the frame cannot be encoded (larger than `MAX_FRAME_BYTES`).
    pub fn ops(&self, origin: DeviceId, ops: Vec<SeqOp>, head_seq: u64) -> Result<(), ProtoError> {
        todo!()
    }

    /// Relays live ink from `origin` (`echo = false`).
    ///
    /// # Errors
    /// As [`Broadcast::ops`].
    pub fn stroke_delta(&self, origin: DeviceId, delta: StrokeDelta) -> Result<(), ProtoError> {
        todo!()
    }

    /// Relays a live-stroke cancel from `origin` (`echo = false`).
    ///
    /// # Errors
    /// As [`Broadcast::ops`].
    pub fn stroke_cancel(&self, origin: DeviceId, cancel: StrokeCancel) -> Result<(), ProtoError> {
        todo!()
    }

    /// Relays `SwitchPage { page, by: origin }` (`echo = false`).
    ///
    /// # Errors
    /// As [`Broadcast::ops`].
    pub fn switch_page(&self, origin: DeviceId, page: PageId) -> Result<(), ProtoError> {
        todo!()
    }
}

//! One WebSocket session: handshake, catch-up, push/ack, live relay, keep-alive, resync.
//!
//! Owner: slice b.
//!
//! Lifecycle (protocol in `penwise_proto::message`):
//! 1. **Handshake** within `HANDSHAKE_TIMEOUT_MS`; non-binary or undecodable first frame → `Reject(Malformed)`.
//!    - `Pair` → `Pairing::redeem` → `Paired` (then wait for `Hello` on the same socket, new timeout) |
//!      `Reject(BadPairingCode)`.
//!    - `Hello` → protocol in `MIN_PROTOCOL_VERSION..=PROTOCOL_VERSION` else `Reject(UnsupportedProtocol)`;
//!      `Pairing::authenticate(device, token)` else `Reject(BadToken)`. Failed handshakes count against the
//!      peer IP in `pairing::limiter` → `Reject(RateLimited)` when over.
//!    - Every `Reject` is followed by a close frame (code 4001) and the socket ends.
//! 2. **Welcome**, register with the hub, `subscribe()` to the broadcast, then catch up: `ops_since` batches of
//!    `MAX_OPS_PER_FRAME` from `last_seen_seq` until `head_seq`, `more` set on all but the last; `sent_seq`
//!    tracks the highest seq sent.
//! 3. **Loop** (`tokio::select!` over socket frames, broadcast receiver, ping timer, hub close signal):
//!    - `PushOps` (`1..=MAX_OPS_PER_FRAME` ops else drop + log) → `storage.append_ops(device, ops, now)` in
//!      `spawn_blocking` → `Ack` to this socket → broadcast the newly logged ops.
//!    - `StrokeDelta`/`StrokeCancel`: forwarded only if the delta's page is known (cheap cached check) and
//!      `samples.len() ∈ 1..=MAX_DELTA_SAMPLES`; the session remembers its open live strokes and, on exit,
//!      broadcasts a `StrokeCancel` for each still open (no commit seen).
//!    - `SwitchPage` → `hub.switch_page`. `ListNotebooks`/`ListPages` → storage snapshot reply.
//!    - `Resync { from_seq }` → `Resync` reply + catch-up from `from_seq`.
//!    - `Ping` → `Pong`; any frame resets the idle timer; `PING_INTERVAL_MS` silence → `Ping`;
//!      `IDLE_TIMEOUT_MS` silence → close.
//!    - Broadcast `Outbound`: skip own non-echo frames; op frames apply the `sent_seq` rule from
//!      `broadcast.rs`; `Lagged` → `Resync { from_seq: sent_seq }` + catch-up.
//!    - Undecodable frames after the handshake are logged and dropped.
//! 4. **Exit**: cancel open live strokes, `hub.unregister`.

use std::net::SocketAddr;

use axum::extract::ws::WebSocket;
use penwise_proto::{ClientKind, DeviceId};
use tokio::sync::watch;

use crate::app::App;

/// WebSocket close code sent after `Reject`.
pub const CLOSE_REJECTED: u16 = 4001;
/// Close code when the hub closes the session (revocation).
pub const CLOSE_REVOKED: u16 = 4003;

/// Hub-side handle of a session (close on revocation/shutdown).
#[derive(Debug)]
pub struct SessionHandle {
    pub device: DeviceId,
    close: watch::Sender<Option<u16>>,
}

/// Session-side close signal: becomes `Some(code)` when the hub wants the session closed.
pub type CloseSignal = watch::Receiver<Option<u16>>;

/// Result of a successful handshake.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Authenticated {
    pub device: DeviceId,
    pub client: ClientKind,
    pub protocol_version: u16,
    pub last_seen_seq: u64,
}

/// Why a session ended (logged at `info`, `warn` for protocol errors).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SessionEnd {
    /// Peer closed or the socket errored.
    PeerClosed,
    /// Handshake refused (reason sent to the peer).
    Rejected,
    /// No frame for `IDLE_TIMEOUT_MS` (or no handshake within `HANDSHAKE_TIMEOUT_MS`).
    Timeout,
    /// Hub requested close (revocation, shutdown).
    Closed,
    /// Storage failure while serving the session.
    Storage,
}

impl SessionHandle {
    /// Handle + the session's close signal.
    #[must_use]
    pub fn new(device: DeviceId) -> (Self, CloseSignal) {
        todo!()
    }

    /// Asks the session to close with `code`.
    pub fn close(&self, code: u16) {
        todo!()
    }
}

/// Serves one upgraded socket from `peer` to completion (module lifecycle). Never panics; every error ends
/// the session with a logged [`SessionEnd`].
pub async fn run(socket: WebSocket, peer: SocketAddr, app: App) -> SessionEnd {
    todo!()
}

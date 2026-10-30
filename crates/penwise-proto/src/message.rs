//! Client ↔ server WebSocket messages (one binary frame each, [`crate::envelope`]).
//!
//! **Session.** The first client frame is `Hello` within `HANDSHAKE_TIMEOUT_MS`, else the server closes.
//! `Hello` → `Welcome` | `Reject`+close. After `Welcome` the server either streams `OpsSince` batches
//! (`more == false` on the last) or, when the client is behind the log floor / on another server / ahead of the
//! head, a `SnapshotBegin` · `SnapshotChunk`… · `SnapshotEnd` sequence; then live traffic (broadcast ops as
//! `OpsSince`, `StrokeDelta`, `StrokeCancel`, `SwitchPage`). Pairing is HTTP, not part of this protocol.
//!
//! **Versioning.** Both enums are append-only (postcard discriminants) from the first release (S1 froze the
//! v1 set; `tests/fixtures/v1/` pins every variant's bytes). After `Welcome` both sides use only variants that
//! exist in `min(client protocol_version, server protocol_version)`; a frame that fails to decode is dropped
//! and logged, never fatal (except during the handshake).

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, ItemRef, OpId, PageId, ServerId};
use crate::live::{StrokeCancel, StrokeDelta};
use crate::op::{AnyOp, OpRejection, SeqOp};
use crate::snapshot::{Scope, SnapshotChunk};

/// Close the socket when no `Hello` arrived within this time.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 5_000;
/// Either side sends `Ping` after this much silence.
pub const PING_INTERVAL_MS: u64 = 15_000;
/// Either side closes after this much silence (no frame at all).
pub const IDLE_TIMEOUT_MS: u64 = 45_000;
/// Most ops in one `PushOps` or `OpsSince` frame.
pub const MAX_OPS_PER_FRAME: usize = 512;
/// Most item rows in one `SnapshotChunk`.
pub const MAX_SNAPSHOT_ITEMS: usize = 512;

/// Long-lived per-device bearer token: 32 random bytes as base64url without padding (43 chars). Only its
/// SHA-256 is stored server-side. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthToken(pub String);

/// What kind of client a device is (stored in `devices.kind`; decides the session [`Scope`]: tablets and
/// agents get everything, browsers the catalog plus the watched page). Codes frozen, append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClientKind {
    /// Code 0.
    Tablet,
    /// Code 1.
    Browser,
    /// Code 2 (MCP agents).
    Agent,
}

/// Why a handshake was refused (sent once, then the server closes the socket). Append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// Unknown, revoked, or other-device token (deliberately indistinguishable).
    BadToken,
    /// `protocol_version` outside the server's supported range.
    UnsupportedProtocol { min: u16, max: u16 },
    /// First frame was not `Hello`, or did not decode.
    Malformed,
    /// Too many failed handshakes from this peer recently.
    RateLimited,
}

/// Connection-level failure after the handshake (the server closes right after). Append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ErrorCode {
    /// A frame did not decode or broke a session rule.
    Malformed,
    /// A frame exceeded `MAX_FRAME_BYTES`.
    TooLarge,
    /// Server-side failure; retry later.
    Internal,
    /// The server is stopping.
    ShuttingDown,
}

/// One refused op of a `PushOps`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct OpRejected {
    pub op_id: OpId,
    pub reason: OpRejection,
}

/// Client → server. Variant order frozen; append only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ClientMsg {
    /// Authenticate and start syncing after `last_seen_seq` (`0` = from the beginning).
    Hello {
        protocol_version: u16,
        device: DeviceId,
        token: AuthToken,
        last_seen_seq: u64,
        client: ClientKind,
    },
    /// Upload new ops (the outbox), `1..=MAX_OPS_PER_FRAME`, in creation order. Answered by one `Ack`.
    PushOps {
        ops: Vec<AnyOp>,
    },
    /// Live ink chunk.
    StrokeDelta(StrokeDelta),
    /// Live stroke abandoned.
    StrokeCancel(StrokeCancel),
    /// "I am now showing `page`": relayed to every other session (followers switch too).
    SwitchPage {
        page: PageId,
    },
    /// Which page's live deltas (and, for browser scope, item ops / snapshot) this session wants; `None` = none.
    Watch {
        page: Option<PageId>,
    },
    /// Client detected a gap (or lost local state): resend everything after `from_seq`.
    Resync {
        from_seq: u64,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    /// "Show `item` (a frame) on `page`": relayed to every other session as [`ServerMsg::Focus`]. The receiver
    /// switches to `page` if needed, then fits the frame to the screen; `item: None` fits the page's content.
    /// Needs envelope [`crate::envelope::FRAME_VERSION`] 2 and protocol 2 (appended last: older variant indices
    /// unchanged); send it only when the `Welcome` reported `protocol_version >= 2`.
    Focus {
        page: PageId,
        item: Option<ItemRef>,
    },
}

/// Server → client. Variant order frozen; append only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Handshake accepted. `protocol_version` = the server's; `log_floor` = oldest `seq` still in the log (a
    /// client with `last_seen_seq < log_floor` gets a snapshot); `server_now_ms` calibrates the client's
    /// [`crate::hlc::ClockOffset`]; `current_page` = last `SwitchPage` target.
    Welcome {
        protocol_version: u16,
        server: ServerId,
        device: DeviceId,
        head_seq: u64,
        log_floor: u64,
        server_now_ms: u64,
        current_page: Option<PageId>,
    },
    /// Handshake refused; the server closes right after.
    Reject {
        reason: RejectReason,
    },
    /// Answer to one `PushOps`: `op_ids[i]` is logged at `seqs[i]` (an already-logged op id reports its
    /// original seq), in push order; refused ops are listed in `rejected` and never logged. Every pushed op
    /// appears in exactly one of the two lists.
    Ack {
        op_ids: Vec<OpId>,
        seqs: Vec<u64>,
        rejected: Vec<OpRejected>,
    },
    /// Logged ops `from_seq < seq ≤ upto_seq` of the session scope in ascending seq (≤ `MAX_OPS_PER_FRAME`).
    /// For tablet scope `ops` are contiguous and `upto_seq == ops.last().seq`; a filtered (browser) batch uses
    /// `upto_seq` to advance the cursor across the gap. `more` = another batch of the same catch-up follows.
    /// Live broadcast of a newly logged op is a one-op `OpsSince` with `more == false` (also sent back to its
    /// author, which treats it as already applied).
    OpsSince {
        from_seq: u64,
        upto_seq: u64,
        ops: Vec<SeqOp>,
        head_seq: u64,
        more: bool,
    },
    /// Start of a snapshot of `scope` taken at `head_seq`.
    SnapshotBegin {
        head_seq: u64,
        scope: Scope,
    },
    /// One batch of snapshot rows (`≤ MAX_SNAPSHOT_ITEMS` items).
    SnapshotChunk(SnapshotChunk),
    /// End of the snapshot; live `OpsSince` traffic continues after `head_seq`.
    SnapshotEnd {
        head_seq: u64,
    },
    /// Live ink from `device` (never echoed to its producer).
    StrokeDelta {
        device: DeviceId,
        delta: StrokeDelta,
    },
    /// Live stroke from `device` abandoned.
    StrokeCancel {
        device: DeviceId,
        cancel: StrokeCancel,
    },
    /// `by` switched to `page`.
    SwitchPage {
        page: PageId,
        by: DeviceId,
    },
    /// The server dropped broadcasts for this session (lag) or honoured a client `Resync`: discard live
    /// strokes; `OpsSince` batches from `from_seq` follow.
    Resync {
        from_seq: u64,
    },
    /// Connection-level failure; the server closes right after.
    Error {
        code: ErrorCode,
        detail: String,
    },
    Ping {
        nonce: u64,
    },
    Pong {
        nonce: u64,
    },
    /// `by` asked every device to show `item` on `page` (see [`ClientMsg::Focus`]). Envelope version 2; the server
    /// sends it only to sessions that offered protocol 2.
    Focus {
        page: PageId,
        item: Option<ItemRef>,
        by: DeviceId,
    },
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthToken(<redacted>)")
    }
}

impl ClientKind {
    /// Frozen code (see variant docs).
    #[must_use]
    pub fn code(self) -> u8 {
        match self {
            Self::Tablet => 0,
            Self::Browser => 1,
            Self::Agent => 2,
        }
    }

    /// Inverse of [`ClientKind::code`].
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        match code {
            0 => Some(Self::Tablet),
            1 => Some(Self::Browser),
            2 => Some(Self::Agent),
            _ => None,
        }
    }
}

impl ClientMsg {
    /// Frame-shape limits the decoder does not enforce: `PushOps` holds `1..=MAX_OPS_PER_FRAME` ops, a
    /// `StrokeDelta` `1..=MAX_DELTA_SAMPLES` samples. Sessions reject a message failing this.
    #[must_use]
    pub fn within_limits(&self) -> bool {
        match self {
            Self::PushOps { ops } => (1..=MAX_OPS_PER_FRAME).contains(&ops.len()),
            Self::StrokeDelta(delta) => {
                (1..=crate::live::MAX_DELTA_SAMPLES).contains(&delta.samples.len())
            }
            _ => true,
        }
    }
}

//! Client ↔ server WebSocket messages (one binary frame each, [`crate::envelope`]).
//!
//! Owner: slice b (`AuthToken`/`PairingCode` Debug impls: contract).
//!
//! **Session.** The first client frame is `Hello` (or `Pair`) within `HANDSHAKE_TIMEOUT_MS`, else the server
//! closes. `Hello` → `Welcome` | `Reject`+close. After `Welcome` the server streams `OpsSince` batches from
//! `last_seen_seq + 1` (`more == false` on the last) and then live traffic (broadcast ops as single-op
//! `OpsSince`, `StrokeDelta`, `StrokeCancel`, `SwitchPage`). `Pair` → `Paired` (the client stores the token
//! and continues with `Hello` on the same socket) | `Reject`+close.
//!
//! **Versioning.** Both enums are append-only (postcard discriminants). After `Welcome` both sides use only
//! variants that exist in `min(client protocol_version, server protocol_version)`; a frame that fails to
//! decode is dropped and logged, never fatal (except during the handshake).

use core::fmt;

use serde::{Deserialize, Serialize};

use crate::ids::{DeviceId, NotebookId, OpId, PageId, ServerId};
use crate::live::{StrokeCancel, StrokeDelta};
use crate::notebook::{Background, PageKind};
use crate::op::{AnyOp, OpRejection, SeqOp};

/// Close the socket when no `Hello`/`Pair` arrived within this time.
pub const HANDSHAKE_TIMEOUT_MS: u64 = 5_000;
/// Either side sends `Ping` after this much silence.
pub const PING_INTERVAL_MS: u64 = 15_000;
/// Either side closes after this much silence (no frame at all).
pub const IDLE_TIMEOUT_MS: u64 = 45_000;
/// Most ops in one `PushOps` or `OpsSince` frame.
pub const MAX_OPS_PER_FRAME: usize = 512;

/// Long-lived per-device bearer token: 32 random bytes as base64url without padding (43 chars). Only its
/// SHA-256 is stored server-side. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthToken(pub String);

/// Short-lived single-use pairing code: 16 random bytes as base64url without padding (22 chars). Lives only in
/// server memory. `Debug` never prints it.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PairingCode(pub String);

/// What kind of client a device is (stored in `devices.kind`). Codes frozen, append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ClientKind {
    /// Code 0.
    Tablet,
    /// Code 1.
    Browser,
    /// Code 2 (MCP agents, M5).
    Agent,
}

/// Why a handshake was refused (sent once, then the server closes the socket). Append-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    /// Unknown, revoked, or other-device token (deliberately indistinguishable).
    BadToken,
    /// `protocol_version` outside the server's supported range.
    UnsupportedProtocol { min: u16, max: u16 },
    /// Pairing code unknown, used, or expired (deliberately indistinguishable).
    BadPairingCode,
    /// First frame was not `Hello`/`Pair`, or did not decode.
    Malformed,
    /// Too many failed handshakes from this peer recently.
    RateLimited,
}

/// Notebook summary for list snapshots.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NotebookInfo {
    pub id: NotebookId,
    pub title: String,
    /// Live pages.
    pub page_count: u32,
    /// Largest `hlc.wall_ms` of any op on the notebook or its pages.
    pub updated_ms: u64,
}

/// Page summary for list snapshots.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PageInfo {
    pub id: PageId,
    pub notebook: NotebookId,
    pub position: i64,
    pub kind: PageKind,
    pub background: Background,
    /// Live items.
    pub item_count: u32,
    /// Largest `hlc.wall_ms` of any op on the page.
    pub updated_ms: u64,
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
    /// Authenticate and start syncing from `last_seen_seq + 1` (`0` = from the beginning).
    Hello {
        protocol_version: u16,
        device: DeviceId,
        token: AuthToken,
        last_seen_seq: u64,
        client: ClientKind,
    },
    /// Exchange a pairing code for a device token. `device` is chosen by the client (fresh ULID); `name` is
    /// a human label (`≤ 64` UTF-8 bytes) for the device list.
    Pair {
        protocol_version: u16,
        code: PairingCode,
        device: DeviceId,
        name: String,
        client: ClientKind,
    },
    /// Upload new ops (the outbox), `1..=MAX_OPS_PER_FRAME`, in creation order. Answered by one `Ack`.
    PushOps { ops: Vec<AnyOp> },
    /// Live ink chunk.
    StrokeDelta(StrokeDelta),
    /// Live stroke abandoned.
    StrokeCancel(StrokeCancel),
    /// "I am now showing `page`": relayed to every other session (followers switch too).
    SwitchPage { page: PageId },
    /// Request a `NotebookList`.
    ListNotebooks,
    /// Request a `PageList` for `notebook`.
    ListPages { notebook: NotebookId },
    /// Client detected a gap (or lost local state): resend everything after `from_seq`.
    Resync { from_seq: u64 },
    Ping { nonce: u64 },
    Pong { nonce: u64 },
}

/// Server → client. Variant order frozen; append only.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum ServerMsg {
    /// Handshake accepted. `protocol_version` = the server's; `current_page` = last `SwitchPage` target.
    Welcome {
        protocol_version: u16,
        server: ServerId,
        device: DeviceId,
        head_seq: u64,
        current_page: Option<PageId>,
    },
    /// Handshake refused; the server closes right after.
    Reject { reason: RejectReason },
    /// Pairing accepted: store `token` (and pin `server`); continue with `Hello`.
    Paired { token: AuthToken, server: ServerId },
    /// Answer to one `PushOps`: `op_ids[i]` is logged at `seqs[i]` (an already-logged op id reports its
    /// original seq), in push order; refused ops are listed in `rejected` and never logged. Every pushed op
    /// appears in exactly one of the two lists.
    Ack {
        op_ids: Vec<OpId>,
        seqs: Vec<u64>,
        rejected: Vec<OpRejected>,
    },
    /// Logged ops `from_seq < seq ≤ …` in ascending seq (≤ `MAX_OPS_PER_FRAME`). `more` = another batch of
    /// the same catch-up follows. Live broadcast of a newly logged op is a one-op `OpsSince` with
    /// `more == false` (also sent back to its author, which treats it as already applied).
    OpsSince {
        from_seq: u64,
        ops: Vec<SeqOp>,
        head_seq: u64,
        more: bool,
    },
    /// Live ink from `device` (never echoed to its producer).
    StrokeDelta { device: DeviceId, delta: StrokeDelta },
    /// Live stroke from `device` abandoned.
    StrokeCancel { device: DeviceId, cancel: StrokeCancel },
    /// `by` switched to `page`.
    SwitchPage { page: PageId, by: DeviceId },
    /// Live notebooks, ascending id.
    NotebookList { notebooks: Vec<NotebookInfo> },
    /// Live pages of `notebook`, ascending `(position, id)`.
    PageList {
        notebook: NotebookId,
        pages: Vec<PageInfo>,
    },
    /// The server dropped broadcasts for this session (lag) or honoured a client `Resync`: discard live
    /// strokes; `OpsSince` batches from `from_seq` follow.
    Resync { from_seq: u64 },
    Ping { nonce: u64 },
    Pong { nonce: u64 },
}

impl fmt::Debug for AuthToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("AuthToken(<redacted>)")
    }
}

impl fmt::Debug for PairingCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("PairingCode(<redacted>)")
    }
}

impl ClientKind {
    /// Frozen code (see variant docs).
    #[must_use]
    pub fn code(self) -> u8 {
        todo!()
    }

    /// Inverse of [`ClientKind::code`].
    #[must_use]
    pub fn from_code(code: u8) -> Option<Self> {
        todo!()
    }
}

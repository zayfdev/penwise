//! What one received frame changed, and its flat binary form for Kotlin.
//!
//! Layout (little-endian): `u8 version = 1`, `u8 kind` (index of the `ServerMsg` variant: `Welcome` 0,
//! `Reject` 1, `Ack` 2, `OpsSince` 3, `SnapshotBegin` 4, `SnapshotChunk` 5, `SnapshotEnd` 6, `StrokeDelta` 7,
//! `StrokeCancel` 8, `SwitchPage` 9, `Resync` 10, `Error` 11, `Ping` 12, `Pong` 13), `u8 flags` (bit0 catalog changed, bit1 reconnect,
//! bit2 snapshot finished, bit3 `page` present, bit4 server changed: a different server answered and the replica is held until `resolve_server_change`), `u8 code` (Reject: 0 bad token, 1 unsupported protocol,
//! 2 malformed, 3 rate limited; Error: 0 malformed, 1 too large, 2 internal, 3 shutting down; else 0),
//! `u64 head_seq`, `u32 applied`, `u32 apply_errors`, `u32 acked`, `u32 rejected`, `[16] page` when bit3,
//! `u32 n` then `n × (u32 len, frame)` replies to send, `u32 m` then `m × ([16] page, [16] item)` changes
//! (all-zero item = reload the whole page).

use penwise_core::item::ItemId;
use penwise_proto::{ErrorCode, PageId, RejectReason, ServerMsg};

use crate::bytes::{Reader, Writer};
use crate::error::SyncError;

/// Blob layout version.
pub const OUTCOME_VERSION: u8 = 1;

/// One scene change a [`crate::scene_handle::SceneHandle`] must mirror.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Change {
    pub page: PageId,
    /// `None`: reload the whole page.
    pub item: Option<ItemId>,
}

/// Summary of one `receive`.
#[allow(
    clippy::struct_excessive_bools,
    reason = "one field per flag bit of the wire blob"
)]
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Outcome {
    pub kind: u8,
    pub code: u8,
    pub head_seq: u64,
    pub catalog_changed: bool,
    pub reconnect: bool,
    pub snapshot_done: bool,
    pub server_changed: bool,
    pub applied: u32,
    pub apply_errors: u32,
    pub acked: u32,
    pub rejected: u32,
    pub page: Option<PageId>,
    pub replies: Vec<Vec<u8>>,
    pub changes: Vec<Change>,
}

/// Wire index of a server message (the `kind` byte).
#[must_use]
pub fn kind_code(msg: &ServerMsg) -> u8 {
    match msg {
        ServerMsg::Welcome { .. } => 0,
        ServerMsg::Reject { .. } => 1,
        ServerMsg::Ack { .. } => 2,
        ServerMsg::OpsSince { .. } => 3,
        ServerMsg::SnapshotBegin { .. } => 4,
        ServerMsg::SnapshotChunk(_) => 5,
        ServerMsg::SnapshotEnd { .. } => 6,
        ServerMsg::StrokeDelta { .. } => 7,
        ServerMsg::StrokeCancel { .. } => 8,
        ServerMsg::SwitchPage { .. } => 9,
        ServerMsg::Resync { .. } => 10,
        ServerMsg::Error { .. } => 11,
        ServerMsg::Ping { .. } => 12,
        ServerMsg::Pong { .. } => 13,
    }
}

/// The `code` byte of a handshake rejection.
#[must_use]
pub fn reject_code(reason: RejectReason) -> u8 {
    match reason {
        RejectReason::BadToken => 0,
        RejectReason::UnsupportedProtocol { .. } => 1,
        RejectReason::Malformed => 2,
        RejectReason::RateLimited => 3,
    }
}

/// The `code` byte of a connection-level error.
#[must_use]
pub fn error_code(code: ErrorCode) -> u8 {
    match code {
        ErrorCode::Malformed => 0,
        ErrorCode::TooLarge => 1,
        ErrorCode::Internal => 2,
        ErrorCode::ShuttingDown => 3,
    }
}

impl Outcome {
    /// Records a change once (`None` item supersedes per-item entries of the same page).
    pub fn note(&mut self, change: Change) {
        if self
            .changes
            .iter()
            .any(|c| c.page == change.page && c.item.is_none())
        {
            return;
        }
        if change.item.is_none() {
            self.changes.retain(|c| c.page != change.page);
        }
        if !self.changes.contains(&change) {
            self.changes.push(change);
        }
    }

    /// The flat blob documented in the module header.
    #[must_use]
    pub fn encode(&self) -> Vec<u8> {
        let mut w = Writer::new();
        w.u8(OUTCOME_VERSION);
        w.u8(self.kind);
        let flags = u8::from(self.catalog_changed)
            | u8::from(self.reconnect) << 1
            | u8::from(self.snapshot_done) << 2
            | u8::from(self.page.is_some()) << 3
            | u8::from(self.server_changed) << 4;
        w.u8(flags);
        w.u8(self.code);
        w.u64(self.head_seq);
        w.u32(self.applied);
        w.u32(self.apply_errors);
        w.u32(self.acked);
        w.u32(self.rejected);
        if let Some(page) = self.page {
            w.raw(&page.to_bytes());
        }
        w.count(self.replies.len());
        for reply in &self.replies {
            w.blob(reply);
        }
        w.count(self.changes.len());
        for change in &self.changes {
            w.raw(&change.page.to_bytes());
            w.raw(&change.item.map_or([0; 16], |id| id.0));
        }
        w.finish()
    }
}

/// The change list of an [`Outcome::encode`] blob (what `SceneHandle::refresh` needs).
///
/// # Errors
/// A truncated or foreign blob.
pub fn decode_changes(blob: &[u8]) -> Result<Vec<Change>, SyncError> {
    let mut r = Reader::new(blob);
    let head = r.take(4)?;
    if head[0] != OUTCOME_VERSION {
        return Err(SyncError::BadArgument("outcome version"));
    }
    // head_seq + applied + apply_errors + acked + rejected.
    r.take(8 + 4 * 4)?;
    if head[2] & 0b1000 != 0 {
        r.take(16)?;
    }
    for _ in 0..r.u32()? {
        r.blob()?;
    }
    let mut changes = Vec::new();
    for _ in 0..r.u32()? {
        let page = PageId::from_slice(r.take(16)?).ok_or(SyncError::BadArgument("page id"))?;
        let item = r.take(16)?;
        changes.push(Change {
            page,
            item: (item != [0; 16]).then(|| ItemId(item.try_into().unwrap_or([0; 16]))),
        });
    }
    Ok(changes)
}

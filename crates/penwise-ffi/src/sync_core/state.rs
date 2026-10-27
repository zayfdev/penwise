//! Persisting a [`SyncCore`] across process restarts: one versioned blob holding the clock, calibration,
//! server pin, outbox (pending + dead letters) and the whole replica as snapshot chunks.
//!
//! The tablet's `SQLite` store (design T2) may instead persist rows itself; this blob is the simple, complete
//! option and the format the headless tests use for crash/restart.

use penwise_proto::snapshot::ItemState;
use penwise_proto::{
    ClockOffset, DeviceId, Hlc, HlcClock, Outbox, Replica, ServerId, SnapshotChunk,
    message::MAX_SNAPSHOT_ITEMS,
};
use serde::{Deserialize, Serialize};

use super::SyncCore;
use crate::error::SyncError;

/// Blob layout version.
pub const STATE_VERSION: u8 = 1;

#[derive(Serialize, Deserialize)]
struct Stored {
    device: [u8; 16],
    hlc: Vec<u8>,
    offset_ms: i64,
    pinned: Option<[u8; 16]>,
    head_seq: u64,
    outbox: Vec<u8>,
    chunks: Vec<SnapshotChunk>,
}

impl SyncCore {
    /// The persistable state (see module docs).
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn export_state(&self) -> Result<Vec<u8>, SyncError> {
        let stored = Stored {
            device: self.device.to_bytes(),
            hlc: self.hlc.last().to_bytes().to_vec(),
            offset_ms: self.offset.millis(),
            pinned: self.pinned.map(ServerId::to_bytes),
            head_seq: self.replica.head_seq(),
            outbox: self.outbox.to_bytes()?,
            chunks: self.replica_chunks(),
        };
        postcard::to_extend(&stored, vec![STATE_VERSION])
            .map_err(|_| SyncError::State("state does not encode"))
    }

    /// Rebuilds a core from [`SyncCore::export_state`]; `entropy` reseeds the id stream.
    ///
    /// # Errors
    /// `State` for an unknown version, a corrupt blob or an inconsistent replica.
    pub fn restore(bytes: &[u8], entropy: u64) -> Result<Self, SyncError> {
        let (&version, body) = bytes.split_first().ok_or(SyncError::State("empty state"))?;
        if version != STATE_VERSION {
            return Err(SyncError::State("unsupported state version"));
        }
        let stored: Stored =
            postcard::from_bytes(body).map_err(|_| SyncError::State("corrupt state"))?;
        let device =
            DeviceId::from_slice(&stored.device).ok_or(SyncError::State("bad device id"))?;
        let last = Hlc::from_bytes(&stored.hlc).ok_or(SyncError::State("bad clock"))?;
        let mut replica = Replica::new();
        for chunk in &stored.chunks {
            replica.apply_snapshot_chunk(chunk)?;
        }
        replica.advance_to(stored.head_seq);
        let outbox = Outbox::from_bytes(&stored.outbox)?;
        let mut core = Self::assemble(device, HlcClock::resume(last), entropy, replica, outbox);
        core.offset = ClockOffset::from_millis(stored.offset_ms);
        core.pinned = stored
            .pinned
            .map(|b| ServerId::from_slice(&b).ok_or(SyncError::State("bad server id")))
            .transpose()?;
        Ok(core)
    }

    /// The replica as snapshot chunks of at most `MAX_SNAPSHOT_ITEMS` item rows.
    fn replica_chunks(&self) -> Vec<SnapshotChunk> {
        let catalog = self.replica.catalog();
        let mut chunks = vec![SnapshotChunk {
            notebooks: catalog.notebook_states(),
            pages: catalog.page_states(),
            items: Vec::new(),
        }];
        let mut rows: Vec<ItemState> = Vec::new();
        for page in self.page_ids() {
            if let Some(replica) = self.replica.page(page) {
                rows.extend(replica.items().map(|m| ItemState::from_item(page, m)));
            }
        }
        for batch in rows.chunks(MAX_SNAPSHOT_ITEMS) {
            chunks.push(SnapshotChunk {
                notebooks: Vec::new(),
                pages: Vec::new(),
                items: batch.to_vec(),
            });
        }
        chunks
    }
}

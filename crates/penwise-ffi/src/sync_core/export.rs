//! State export split in two halves so the expensive one runs without the host's lock: [`SyncCore::snapshot_state`]
//! copies the state (the replica's item rows and the outbox bytes, without the scenes; the pen and render threads wait for
//! this only), [`StateSnapshot::encode`] turns the copy into the persistable blob (item encoding, postcard).

use penwise_proto::ServerId;

use super::SyncCore;
use super::state::{Rows, Stored, chunks_of, copy_rows};
use crate::error::SyncError;

/// A consistent copy of everything [`SyncCore::export_state`] persists; independent of the core.
#[derive(Debug)]
pub struct StateSnapshot {
    device: [u8; 16],
    hlc: Vec<u8>,
    offset_ms: i64,
    pinned: Option<[u8; 16]>,
    head_seq: u64,
    outbox: Vec<u8>,
    rows: Rows,
}

impl SyncCore {
    /// Copies the persistable state (cheap compared to encoding it; call under the host's lock).
    ///
    /// # Errors
    /// Outbox encoding failure (never expected).
    pub fn snapshot_state(&self) -> Result<StateSnapshot, SyncError> {
        Ok(StateSnapshot {
            device: self.device.to_bytes(),
            hlc: self.hlc.last().to_bytes().to_vec(),
            offset_ms: self.offset.millis(),
            pinned: self.pinned.map(ServerId::to_bytes),
            head_seq: self.replica.head_seq(),
            outbox: self.outbox.to_bytes()?,
            rows: copy_rows(&self.replica),
        })
    }
}

impl StateSnapshot {
    /// The blob [`SyncCore::restore`] reads (same bytes as `export_state` at the moment of the snapshot).
    ///
    /// # Errors
    /// Encoding failure (never expected).
    pub fn encode(self) -> Result<Vec<u8>, SyncError> {
        Stored {
            device: self.device,
            hlc: self.hlc,
            offset_ms: self.offset_ms,
            pinned: self.pinned,
            head_seq: self.head_seq,
            outbox: self.outbox,
            chunks: chunks_of(&self.rows),
        }
        .encode()
    }
}

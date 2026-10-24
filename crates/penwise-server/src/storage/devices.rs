//! Server identity, paired devices and token hashes (`server_meta`, `devices`, `tokens`).
//!
//! Owner: slice c.
//!
//! Tokens are stored only as `SHA-256(token text)` (`pairing::token::hash_secret`). Lookup is by that hash
//! (primary key); the caller re-compares the fetched hash with `pairing::token::secrets_match` (constant
//! time) before trusting it.

use penwise_proto::{ClientKind, DeviceId, ServerId};

use super::{Storage, StorageError};

/// One paired device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DeviceRecord {
    pub id: DeviceId,
    /// `1..=64` UTF-8 bytes.
    pub name: String,
    pub kind: ClientKind,
    pub created_ms: u64,
    pub last_seen_ms: Option<u64>,
}

impl Storage {
    /// Returns the stored server id, inserting `candidate` first if none exists (first start).
    ///
    /// # Errors
    /// [`StorageError::Corrupt`] for a malformed stored id; SQLite errors.
    pub fn ensure_server_id(&self, candidate: ServerId) -> Result<ServerId, StorageError> {
        todo!()
    }

    /// Pairs a device in one transaction: upserts `device` (an existing id keeps `created_ms`, takes the new
    /// `name`/`kind`: re-pairing) and inserts the token hash.
    ///
    /// # Errors
    /// SQLite errors (including a hash collision on the primary key, i.e. a reused token).
    pub fn pair_device(
        &self,
        device: &DeviceRecord,
        token_hash: &[u8; 32],
        now_ms: u64,
    ) -> Result<(), StorageError> {
        todo!()
    }

    /// Device and stored hash for a non-revoked token hash; `None` when unknown or revoked.
    ///
    /// # Errors
    /// SQLite errors.
    pub fn token_device(
        &self,
        token_hash: &[u8; 32],
    ) -> Result<Option<(DeviceId, [u8; 32])>, StorageError> {
        todo!()
    }

    /// Records a successful authentication (`tokens.last_used_ms`, `devices.last_seen_ms`).
    ///
    /// # Errors
    /// SQLite errors.
    pub fn touch_token(&self, token_hash: &[u8; 32], now_ms: u64) -> Result<(), StorageError> {
        todo!()
    }

    /// Every paired device, ascending id.
    ///
    /// # Errors
    /// SQLite errors; [`StorageError::Corrupt`] for an unknown kind code.
    pub fn devices(&self) -> Result<Vec<DeviceRecord>, StorageError> {
        todo!()
    }

    /// Revokes every token of `device`; returns how many were live. Open sessions of the device are closed
    /// by the caller (`SyncHub::disconnect_device`).
    ///
    /// # Errors
    /// SQLite errors.
    pub fn revoke_device(&self, device: DeviceId, now_ms: u64) -> Result<usize, StorageError> {
        todo!()
    }
}

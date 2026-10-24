//! SQLite storage: schema migrations, the append-only op log, materialized notebooks/pages/items, devices and
//! tokens (PLAN "Code layout": `storage/`; server-stack.md §3).
//!
//! Owner: slice a (`mod.rs`, `migrations.rs`, `ops.rs`, `items.rs`, `pages.rs`, `migrations/`); slice c
//! (`devices.rs`).
//!
//! One `rusqlite::Connection` behind a `std::sync::Mutex`: a single writer, short transactions, readers take
//! the same lock (single-user server; revisit with a read pool only if profiling says so). Every method is
//! **blocking**: async callers wrap it in `tokio::task::spawn_blocking`. Schema: `migrations/v1.sql`.
//!
//! Pragmas on open (outside migrations): `journal_mode = WAL`, `synchronous = NORMAL`, `foreign_keys = ON`,
//! `busy_timeout = 5000`.

mod devices;
mod items;
pub mod migrations;
mod ops;
mod pages;

use std::path::Path;
use std::sync::{Arc, Mutex};

use rusqlite::Connection;

pub use devices::DeviceRecord;
pub use items::{ItemSummary, STAMPS_VERSION};
pub use ops::AppendOutcome;

/// Stored-state failure.
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error(transparent)]
    Sqlite(#[from] rusqlite::Error),
    #[error(transparent)]
    Migration(#[from] rusqlite_migration::Error),
    /// A stored blob (op body, item data, stamps) failed to decode: corruption or a schema bug.
    #[error("corrupt {what}")]
    Corrupt { what: &'static str },
    /// The connection mutex was poisoned by a panic in another thread.
    #[error("storage lock poisoned")]
    Poisoned,
}

/// Handle to the database (clone = same connection).
#[derive(Debug, Clone)]
pub struct Storage {
    conn: Arc<Mutex<Connection>>,
}

impl Storage {
    /// Opens (creating if missing) the database file, sets the pragmas and migrates to the latest schema.
    ///
    /// # Errors
    /// [`StorageError::Sqlite`], [`StorageError::Migration`].
    pub fn open(path: &Path) -> Result<Self, StorageError> {
        todo!()
    }

    /// In-memory database (tests): same pragmas (WAL is a no-op) and migrations.
    ///
    /// # Errors
    /// As [`Storage::open`].
    pub fn open_in_memory() -> Result<Self, StorageError> {
        todo!()
    }

    /// Runs `f` with the locked connection.
    ///
    /// # Errors
    /// [`StorageError::Poisoned`], or `f`'s error.
    pub(crate) fn with_conn<T>(
        &self,
        f: impl FnOnce(&mut Connection) -> Result<T, StorageError>,
    ) -> Result<T, StorageError> {
        todo!()
    }
}

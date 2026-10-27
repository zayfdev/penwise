//! Storage errors.

use penwise_proto::{OpId, OpRejection};

/// A failure of the storage layer itself (never a verdict on client data).
#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("io: {0}")]
    Io(std::io::Error),
    /// The database is a v1–3 library: run `penwise-server migrate` first.
    #[error("database is schema v{version}; run `penwise-server migrate`")]
    NeedsMigration { version: i64 },
    #[error("database is schema v{version}, newer than this build")]
    FutureSchema { version: i64 },
    /// A stored row does not decode (corruption or a bug; never skipped silently).
    #[error("corrupt row: {0}")]
    Corrupt(String),
    /// A v3 → v4 migration step failed or its verification did not hold.
    #[error("migration: {0}")]
    Migration(String),
    /// The writer thread is gone (server shutting down).
    #[error("storage closed")]
    Closed,
    /// Fault injection ([`super::FaultPlan`]) aborted the frame.
    #[error("injected fault")]
    Fault,
    /// The requested `after_seq` is below the retained log: the caller needs a snapshot.
    #[error("log starts after seq {floor}")]
    BelowFloor { floor: u64 },
}

/// One refused op of a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Rejected {
    pub op_id: OpId,
    pub reason: OpRejection,
}

/// Why `append_ops` logged nothing.
#[derive(Debug, thiserror::Error)]
pub enum AppendError {
    /// At least one op failed validation: the whole frame was rolled back. Lists every refused op.
    #[error("{} op(s) rejected", .0.len())]
    Rejected(Vec<Rejected>),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

impl From<rusqlite::Error> for AppendError {
    fn from(error: rusqlite::Error) -> Self {
        Self::Storage(error.into())
    }
}

/// Why a restore produced no ops.
#[derive(Debug, thiserror::Error)]
pub enum RestoreError {
    #[error("unknown page")]
    UnknownPage,
    /// `to_seq` predates the retained log.
    #[error("log starts after seq {floor}")]
    BelowFloor { floor: u64 },
    #[error(transparent)]
    Append(#[from] AppendError),
    #[error(transparent)]
    Storage(#[from] StorageError),
}

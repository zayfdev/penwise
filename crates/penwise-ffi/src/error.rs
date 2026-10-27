//! Errors of the JNI-free layer ([`crate::sync_core::SyncCore`], [`crate::scene_handle::SceneHandle`]).
//!
//! The JNI layer maps every variant to a thrown `RuntimeException` (see [`crate::stroke::FfiError`]).

use penwise_core::CodecError;
use penwise_core::edit::EditError;
use penwise_proto::legacy::LegacyError;
use penwise_proto::{ProtoError, ReplicaError};

/// Failure of a bytes-in/bytes-out operation.
#[derive(Debug, thiserror::Error)]
pub enum SyncError {
    /// An argument the caller must not send (wrong id length, non-finite number, unknown code).
    #[error("invalid argument: {0}")]
    BadArgument(&'static str),
    /// Frame, op or state-blob coding.
    #[error("proto: {0}")]
    Proto(#[from] ProtoError),
    /// The local replica refused an op.
    #[error("replica: {0}")]
    Replica(#[from] ReplicaError),
    /// The scene refused an edit.
    #[error("edit: {0}")]
    Edit(#[from] EditError),
    /// An item/edit blob failed its codec.
    #[error("codec: {0}")]
    Codec(#[from] CodecError),
    /// A legacy tablet stroke did not convert.
    #[error("legacy: {0}")]
    Legacy(#[from] LegacyError),
    /// An op would exceed the size limit of one logged op (nothing was changed).
    #[error("op too large ({0} bytes)")]
    OpTooLarge(usize),
    /// The page is not in the local replica (sync the catalog first, or create it).
    #[error("unknown page")]
    UnknownPage,
    /// A persisted state blob is unusable.
    #[error("state: {0}")]
    State(&'static str),
}

//! Protocol errors: frame/blob coding ([`ProtoError`]) and replica application ([`ReplicaError`]).
//!
//! Owner: slice a.

use penwise_core::edit::EditError;

use crate::lww::LwwError;

/// Frame or stored-op coding failure. Decoding never panics on any input.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ProtoError {
    /// Zero-length frame or blob.
    #[error("empty frame")]
    Empty,
    /// Leading version byte this build does not know.
    #[error("unsupported frame version {0}")]
    UnsupportedVersion(u8),
    /// Frame longer than `MAX_FRAME_BYTES` (checked before decoding and after encoding).
    #[error("frame of {len} bytes exceeds the limit")]
    TooLarge { len: usize },
    /// Bytes left after the message.
    #[error("{count} trailing bytes")]
    TrailingBytes { count: usize },
    /// postcard rejected the bytes (truncation, unknown variant, bad UTF-8, embedded edit blob error).
    #[error("decode: {0}")]
    Decode(postcard::Error),
    /// postcard could not encode (never expected for well-formed values).
    #[error("encode: {0}")]
    Encode(postcard::Error),
}

/// An op a replica cannot apply. For a server-ordered log these indicate a server bug or a corrupted log;
/// replicas log them and continue.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum ReplicaError {
    #[error("unknown page")]
    UnknownPage,
    #[error("unknown notebook")]
    UnknownNotebook,
    #[error("unknown item")]
    UnknownItem,
    /// Create of an existing id with a different stamp.
    #[error("duplicate id")]
    Duplicate,
    #[error(transparent)]
    Lww(#[from] LwwError),
    /// The core scene rejected the edit.
    #[error(transparent)]
    Edit(#[from] EditError),
}

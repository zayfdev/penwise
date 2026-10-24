//! Typed decode/encode errors for every codec in core. Decoding never panics: malformed input is an error.
//!
//! Owner: slice d.

/// Codec failure. Offsets are byte positions in the input (diagnostics only; never serialized).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CodecError {
    /// Input ended inside a field.
    #[error("unexpected end of input at byte {offset}")]
    UnexpectedEof { offset: usize },
    /// Wrong 4-byte magic.
    #[error("bad magic {found:?}")]
    BadMagic { found: [u8; 4] },
    /// Layout version this build does not know.
    #[error("unsupported version {0}")]
    UnsupportedVersion(u16),
    /// Varint longer than its type allows (> 5 bytes for 32-bit, > 10 for 64-bit) or overflowing it.
    #[error("varint overflow at byte {offset}")]
    VarintOverflow { offset: usize },
    /// Bytes left after the trailer.
    #[error("{count} trailing bytes after the checksum")]
    TrailingBytes { count: usize },
    /// FNV-1a trailer does not match the content.
    #[error("checksum mismatch: stored {stored:#018x}, computed {computed:#018x}")]
    ChecksumMismatch { stored: u64, computed: u64 },
    /// A tag, enum code, UTF-8 string, non-finite float or structural count is invalid.
    #[error("invalid {field}")]
    InvalidValue { field: &'static str },
    /// A numeric value lies outside its documented range (e.g. pressure grid > 4096).
    #[error("{field} out of range")]
    OutOfRange { field: &'static str },
    /// `.pwrec` source this decoder cannot map to samples.
    #[error("unsupported recording source {0}")]
    UnsupportedSource(u8),
}

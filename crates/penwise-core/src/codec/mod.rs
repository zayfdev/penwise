//! Binary formats: varint/zigzag primitives, the versioned stroke blob and `.pwrec` fixtures.
//!
//! Owner: slice d.
//!
//! Hand-written, zero-dependency, deterministic: no serde/postcard, no `usize` or native-endian bytes in output,
//! every blob versioned and ending in an FNV-1a 64 trailer ([`crate::hash`]).

pub mod edit;
pub mod error;
pub mod item;
mod params_fields;
pub mod pwrec;
pub mod stroke;
pub mod varint;

pub use error::CodecError;
pub use pwrec::{Recording, decode_pwrec, encode_pwrec};
pub use stroke::{StrokeRecord, decode_stroke, encode_stroke};

use crate::hash::fnv1a64;
use varint::Reader;

/// Appends `magic` and `version` (LE).
fn write_header(out: &mut Vec<u8>, magic: [u8; 4], version: u16) {
    out.extend_from_slice(&magic);
    out.extend_from_slice(&version.to_le_bytes());
}

/// Reads and checks `magic` then `version`.
fn read_header(r: &mut Reader<'_>, magic: [u8; 4], version: u16) -> Result<(), CodecError> {
    let mut found = [0_u8; 4];
    found.copy_from_slice(r.bytes(4)?);
    if found != magic {
        return Err(CodecError::BadMagic { found });
    }
    let v = r.u16_le()?;
    if v != version {
        return Err(CodecError::UnsupportedVersion(v));
    }
    Ok(())
}

/// Appends the FNV-1a 64 trailer of `out[start..]` (the blob written since `start`).
fn write_trailer(out: &mut Vec<u8>, start: usize) {
    let hash = fnv1a64(out.get(start..).unwrap_or(&[]));
    out.extend_from_slice(&hash.to_le_bytes());
}

/// Shared tail of every strict decoder. `body` is the decoded value and the offset where its bytes end.
///
/// - Success: the 8-byte trailer must follow at `end`, match `fnv1a64(bytes[..end])`, and be the last bytes.
/// - Field-validation failure (`InvalidValue`, `OutOfRange`, `UnsupportedSource`): the checksum takes
///   precedence, checked against the file's last 8 bytes, so corruption reports `ChecksumMismatch`.
/// - Structural failures (`UnexpectedEof`, `VarintOverflow`, …) are returned as-is.
fn finish_decode<T>(bytes: &[u8], body: Result<(T, usize), CodecError>) -> Result<T, CodecError> {
    match body {
        Ok((value, end)) => {
            let content = bytes.get(..end).unwrap_or(&[]);
            let mut tail = Reader::new(bytes.get(end..).unwrap_or(&[]));
            let stored = tail
                .u64_le()
                .map_err(|_| CodecError::UnexpectedEof { offset: end })?;
            check_sum(stored, content)?;
            match tail.remaining() {
                0 => Ok(value),
                count => Err(CodecError::TrailingBytes { count }),
            }
        }
        Err(
            e @ (CodecError::InvalidValue { .. }
            | CodecError::OutOfRange { .. }
            | CodecError::UnsupportedSource(_)),
        ) => {
            if let Some(split) = bytes.len().checked_sub(8) {
                let (content, trailer) = bytes.split_at(split);
                let mut stored = [0_u8; 8];
                stored.copy_from_slice(trailer);
                check_sum(u64::from_le_bytes(stored), content)?;
            }
            Err(e)
        }
        Err(e) => Err(e),
    }
}

/// `ChecksumMismatch` unless `stored == fnv1a64(content)`.
fn check_sum(stored: u64, content: &[u8]) -> Result<(), CodecError> {
    let computed = fnv1a64(content);
    if stored == computed {
        Ok(())
    } else {
        Err(CodecError::ChecksumMismatch { stored, computed })
    }
}

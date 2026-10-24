//! Binary formats: varint/zigzag primitives, the versioned stroke blob and `.pwrec` fixtures.
//!
//! Owner: slice d.
//!
//! Hand-written, zero-dependency, deterministic: no serde/postcard, no `usize` or native-endian bytes in output,
//! every blob versioned and ending in an FNV-1a 64 trailer ([`crate::hash`]).

pub mod error;
pub mod pwrec;
pub mod stroke;
pub mod varint;

pub use error::CodecError;
pub use pwrec::{Recording, decode_pwrec, encode_pwrec};
pub use stroke::{StrokeRecord, decode_stroke, encode_stroke};

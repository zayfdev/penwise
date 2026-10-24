//! LEB128 varints, zigzag, and a bounds-checked byte reader shared by the stroke and `.pwrec` codecs.
//!
//! Owner: slice d.
//!
//! `uv` = unsigned LEB128 (7 bits per byte, low group first, high bit = continuation). `sv` = zigzag then `uv`.
//! Encoders are canonical (minimal length); decoders reject over-long encodings that exceed the type width
//! (more than 5 bytes for 32-bit, 10 for 64-bit, or set bits beyond the width) with `VarintOverflow`.

use super::error::CodecError;

/// Zigzag: `(v << 1) ^ (v >> 31)` as `u32` (0→0, −1→1, 1→2, …).
#[must_use]
pub const fn zigzag32(v: i32) -> u32 {
    todo!()
}

/// Inverse of [`zigzag32`].
#[must_use]
pub const fn unzigzag32(v: u32) -> i32 {
    todo!()
}

/// Appends `v` as minimal LEB128 (1–5 bytes).
pub fn write_uv32(out: &mut Vec<u8>, v: u32) {
    todo!()
}

/// Appends `v` as minimal LEB128 (1–10 bytes).
pub fn write_uv64(out: &mut Vec<u8>, v: u64) {
    todo!()
}

/// Appends `zigzag32(v)` as LEB128.
pub fn write_sv32(out: &mut Vec<u8>, v: i32) {
    todo!()
}

/// Bounds-checked little-endian reader over a byte slice. Every read either advances past the value or
/// returns an error and leaves the position unspecified (decoders abort on the first error).
#[derive(Debug, Clone)]
pub struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl<'a> Reader<'a> {
    /// Reader at offset 0.
    #[must_use]
    pub fn new(buf: &'a [u8]) -> Self {
        Self { buf, pos: 0 }
    }

    /// Current byte offset.
    #[must_use]
    pub fn position(&self) -> usize {
        todo!()
    }

    /// Bytes not yet consumed.
    #[must_use]
    pub fn remaining(&self) -> usize {
        todo!()
    }

    /// Next `n` bytes.
    ///
    /// # Errors
    /// `UnexpectedEof` if fewer than `n` remain.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        todo!()
    }

    /// One byte.
    ///
    /// # Errors
    /// `UnexpectedEof`.
    pub fn u8(&mut self) -> Result<u8, CodecError> {
        todo!()
    }

    /// Little-endian `u16`.
    ///
    /// # Errors
    /// `UnexpectedEof`.
    pub fn u16_le(&mut self) -> Result<u16, CodecError> {
        todo!()
    }

    /// Little-endian `u64`.
    ///
    /// # Errors
    /// `UnexpectedEof`.
    pub fn u64_le(&mut self) -> Result<u64, CodecError> {
        todo!()
    }

    /// `f32` from little-endian bits; rejects NaN/±inf.
    ///
    /// # Errors
    /// `UnexpectedEof`; `InvalidValue { field }` for non-finite values.
    pub fn f32_le(&mut self, field: &'static str) -> Result<f32, CodecError> {
        todo!()
    }

    /// LEB128 `u32`.
    ///
    /// # Errors
    /// `UnexpectedEof`; `VarintOverflow` past 5 bytes or 32 bits.
    pub fn uv32(&mut self) -> Result<u32, CodecError> {
        todo!()
    }

    /// LEB128 `u64`.
    ///
    /// # Errors
    /// `UnexpectedEof`; `VarintOverflow` past 10 bytes or 64 bits.
    pub fn uv64(&mut self) -> Result<u64, CodecError> {
        todo!()
    }

    /// Zigzag LEB128 `i32`.
    ///
    /// # Errors
    /// As [`Reader::uv32`].
    pub fn sv32(&mut self) -> Result<i32, CodecError> {
        todo!()
    }
}

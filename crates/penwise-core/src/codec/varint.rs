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
    ((v << 1) ^ (v >> 31)).cast_unsigned()
}

/// Inverse of [`zigzag32`].
#[must_use]
pub const fn unzigzag32(v: u32) -> i32 {
    (v >> 1).cast_signed() ^ (v & 1).cast_signed().wrapping_neg()
}

/// Appends `v` as minimal LEB128 (1–5 bytes).
pub fn write_uv32(out: &mut Vec<u8>, v: u32) {
    write_uv64(out, u64::from(v));
}

/// Appends `v` as minimal LEB128 (1–10 bytes).
pub fn write_uv64(out: &mut Vec<u8>, v: u64) {
    let mut v = v;
    while v >= 0x80 {
        out.push((v.to_le_bytes()[0] & 0x7F) | 0x80);
        v >>= 7;
    }
    out.push(v.to_le_bytes()[0]);
}

/// Appends `zigzag32(v)` as LEB128.
pub fn write_sv32(out: &mut Vec<u8>, v: i32) {
    write_uv32(out, zigzag32(v));
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
        self.pos
    }

    /// Bytes not yet consumed.
    #[must_use]
    pub fn remaining(&self) -> usize {
        self.buf.len().saturating_sub(self.pos)
    }

    /// Next `n` bytes.
    ///
    /// # Errors
    /// `UnexpectedEof` if fewer than `n` remain.
    pub fn bytes(&mut self, n: usize) -> Result<&'a [u8], CodecError> {
        let start = self.pos;
        let out = start
            .checked_add(n)
            .and_then(|end| self.buf.get(start..end))
            .ok_or(CodecError::UnexpectedEof { offset: start })?;
        self.pos = start + n;
        Ok(out)
    }

    /// One byte.
    ///
    /// # Errors
    /// `UnexpectedEof`.
    pub fn u8(&mut self) -> Result<u8, CodecError> {
        let [b] = self.array::<1>()?;
        Ok(b)
    }

    /// Little-endian `u16`.
    ///
    /// # Errors
    /// `UnexpectedEof`.
    pub fn u16_le(&mut self) -> Result<u16, CodecError> {
        Ok(u16::from_le_bytes(self.array()?))
    }

    /// Little-endian `u64`.
    ///
    /// # Errors
    /// `UnexpectedEof`.
    pub fn u64_le(&mut self) -> Result<u64, CodecError> {
        Ok(u64::from_le_bytes(self.array()?))
    }

    /// `f32` from little-endian bits; rejects NaN/±inf.
    ///
    /// # Errors
    /// `UnexpectedEof`; `InvalidValue { field }` for non-finite values.
    pub fn f32_le(&mut self, field: &'static str) -> Result<f32, CodecError> {
        let v = f32::from_bits(u32::from_le_bytes(self.array()?));
        if v.is_finite() {
            Ok(v)
        } else {
            Err(CodecError::InvalidValue { field })
        }
    }

    /// LEB128 `u32`.
    ///
    /// # Errors
    /// `UnexpectedEof`; `VarintOverflow` past 5 bytes or 32 bits.
    pub fn uv32(&mut self) -> Result<u32, CodecError> {
        let start = self.pos;
        let v = self.leb128(5)?;
        u32::try_from(v).map_err(|_| CodecError::VarintOverflow { offset: start })
    }

    /// LEB128 `u64`.
    ///
    /// # Errors
    /// `UnexpectedEof`; `VarintOverflow` past 10 bytes or 64 bits.
    pub fn uv64(&mut self) -> Result<u64, CodecError> {
        self.leb128(10)
    }

    /// Zigzag LEB128 `i32`.
    ///
    /// # Errors
    /// As [`Reader::uv32`].
    pub fn sv32(&mut self) -> Result<i32, CodecError> {
        Ok(unzigzag32(self.uv32()?))
    }

    /// Next `N` bytes as an array.
    fn array<const N: usize>(&mut self) -> Result<[u8; N], CodecError> {
        let mut out = [0_u8; N];
        out.copy_from_slice(self.bytes(N)?);
        Ok(out)
    }

    /// LEB128 of at most `max_bytes` bytes into a `u64`; bits shifted past 64 are an overflow (callers narrow
    /// further). Over-long encodings beyond `max_bytes` are `VarintOverflow`.
    fn leb128(&mut self, max_bytes: u32) -> Result<u64, CodecError> {
        let start = self.pos;
        let mut value = 0_u64;
        for i in 0..max_bytes {
            let byte = self.u8()?;
            let group = u64::from(byte & 0x7F);
            let shift = 7 * i;
            let shifted = group << shift;
            if shifted >> shift != group {
                return Err(CodecError::VarintOverflow { offset: start });
            }
            value |= shifted;
            if byte & 0x80 == 0 {
                return Ok(value);
            }
        }
        Err(CodecError::VarintOverflow { offset: start })
    }
}

#[cfg(test)]
mod tests;

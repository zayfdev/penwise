//! Tiny little-endian writer/reader for the documented blob layouts (outcome, catalog, edit lists).

use crate::error::SyncError;

/// Append-only little-endian writer.
#[derive(Debug, Default)]
pub struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    /// Empty writer.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn u8(&mut self, v: u8) {
        self.buf.push(v);
    }

    pub fn u32(&mut self, v: u32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn u64(&mut self, v: u64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn i64(&mut self, v: i64) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn f32(&mut self, v: f32) {
        self.buf.extend_from_slice(&v.to_le_bytes());
    }

    pub fn raw(&mut self, bytes: &[u8]) {
        self.buf.extend_from_slice(bytes);
    }

    /// `u32` length prefix, then the bytes (clamped to `u32::MAX`, unreachable for in-memory blobs).
    pub fn blob(&mut self, bytes: &[u8]) {
        self.u32(u32::try_from(bytes.len()).unwrap_or(u32::MAX));
        self.raw(bytes);
    }

    /// `u32` element count.
    pub fn count(&mut self, n: usize) {
        self.u32(u32::try_from(n).unwrap_or(u32::MAX));
    }

    #[must_use]
    pub fn finish(self) -> Vec<u8> {
        self.buf
    }
}

/// Bounds-checked little-endian reader.
#[derive(Debug)]
pub struct Reader<'a> {
    rest: &'a [u8],
}

impl<'a> Reader<'a> {
    #[must_use]
    pub fn new(bytes: &'a [u8]) -> Self {
        Self { rest: bytes }
    }

    pub fn take(&mut self, n: usize) -> Result<&'a [u8], SyncError> {
        if self.rest.len() < n {
            return Err(SyncError::BadArgument("truncated blob"));
        }
        let (head, tail) = self.rest.split_at(n);
        self.rest = tail;
        Ok(head)
    }

    pub fn u32(&mut self) -> Result<u32, SyncError> {
        let b: [u8; 4] = self
            .take(4)?
            .try_into()
            .map_err(|_| SyncError::BadArgument("truncated blob"))?;
        Ok(u32::from_le_bytes(b))
    }

    /// Length-prefixed bytes written by [`Writer::blob`].
    pub fn blob(&mut self) -> Result<&'a [u8], SyncError> {
        let n = usize::try_from(self.u32()?).map_err(|_| SyncError::BadArgument("blob size"))?;
        self.take(n)
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.rest.is_empty()
    }
}

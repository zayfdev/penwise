//! FNV-1a 64-bit hash: golden summaries and `.pwrec`/stroke-blob trailers (core-determinism.md §5).
//!
//! Owner: contract.
//!
//! Zero-dependency and identical on every target. Not collision resistant; inputs are not adversarial.
//! Test vectors: `""` → `0xcbf2_9ce4_8422_2325`, `"a"` → `0xaf63_dc4c_8601_ec8c`.

/// FNV-1a 64 offset basis.
pub const FNV_OFFSET: u64 = 0xcbf2_9ce4_8422_2325;
/// FNV-1a 64 prime.
pub const FNV_PRIME: u64 = 0x0000_0100_0000_01b3;

/// Incremental FNV-1a 64 hasher; `write` calls in any chunking give the same result as one call.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Fnv1a64 {
    state: u64,
}

impl Default for Fnv1a64 {
    fn default() -> Self {
        Self::new()
    }
}

impl Fnv1a64 {
    /// Fresh hasher at the offset basis.
    #[must_use]
    pub const fn new() -> Self {
        Self { state: FNV_OFFSET }
    }

    /// Feeds `bytes` (xor then multiply, per byte).
    pub fn write(&mut self, bytes: &[u8]) {
        for &b in bytes {
            self.state ^= u64::from(b);
            self.state = self.state.wrapping_mul(FNV_PRIME);
        }
    }

    /// Current hash value; the hasher stays usable.
    #[must_use]
    pub const fn finish(self) -> u64 {
        self.state
    }
}

/// One-shot FNV-1a 64 of `bytes`.
#[must_use]
pub fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h = Fnv1a64::new();
    h.write(bytes);
    h.finish()
}

#[cfg(test)]
mod tests;

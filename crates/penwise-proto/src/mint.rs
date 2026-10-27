//! Id minting for clients: ULIDs from the caller's wall clock plus a seeded `splitmix64` stream.
//!
//! Proto is pure (no RNG, no clock), so entropy comes from the host (Android `SecureRandom`, browser
//! `crypto.getRandomValues`, tests: any number) and the time from each caller's `now_ms`. The stream is also
//! seeded from the (unique) device id, so two devices never collide in practice: 80 random bits per id.

use penwise_core::item::{IdSource, ItemId};

use crate::ids::{DeviceId, ID_RANDOM_MASK, MAX_ID_TIMESTAMP_MS, NotebookId, OpId, PageId};

const GAMMA: u64 = 0x9E37_79B9_7F4A_7C15;

/// Seeded id source.
#[derive(Debug, Clone)]
pub struct IdMint {
    state: u64,
    now_ms: u64,
}

impl IdMint {
    /// Stream seeded from `entropy` mixed with the device id.
    #[must_use]
    pub fn new(device: DeviceId, entropy: u64) -> Self {
        let mut seed = entropy;
        for chunk in device.to_bytes().chunks(8) {
            let mut word = [0_u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            seed = seed.rotate_left(17) ^ u64::from_le_bytes(word);
        }
        Self::from_seed(seed)
    }

    /// Stream seeded from `seed` alone (tests: reproducible ids).
    #[must_use]
    pub fn from_seed(seed: u64) -> Self {
        Self {
            state: seed,
            now_ms: 0,
        }
    }

    /// Wall clock stamped into the ids minted until the next call.
    pub fn set_now(&mut self, now_ms: u64) {
        self.now_ms = now_ms.min(MAX_ID_TIMESTAMP_MS);
    }

    /// Next raw 64 bits of the stream (seeds, tie-break values).
    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(GAMMA);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn parts(&mut self) -> (u64, u128) {
        let random = (u128::from(self.next_u64()) << 64) | u128::from(self.next_u64());
        (self.now_ms, random & ID_RANDOM_MASK)
    }

    /// Fresh op id.
    pub fn op(&mut self) -> OpId {
        let (ts, random) = self.parts();
        OpId::from_parts(ts, random)
    }

    /// Fresh notebook id.
    pub fn notebook(&mut self) -> NotebookId {
        let (ts, random) = self.parts();
        NotebookId::from_parts(ts, random)
    }

    /// Fresh page id.
    pub fn page(&mut self) -> PageId {
        let (ts, random) = self.parts();
        PageId::from_parts(ts, random)
    }

    /// Fresh device id.
    pub fn device(&mut self) -> DeviceId {
        let (ts, random) = self.parts();
        DeviceId::from_parts(ts, random)
    }
}

impl IdSource for IdMint {
    fn next_id(&mut self) -> ItemId {
        let (ts, random) = self.parts();
        ItemId(OpId::from_parts(ts, random).to_bytes())
    }
}

//! Deterministic id generation for tests: unique per seed, no RNG dependency.

use penwise_core::item::{IdSource, ItemId};
use penwise_proto::{DeviceId, NotebookId, OpId, PageId};

/// splitmix64-based ULID source. Two generators with different seeds never collide in practice.
#[derive(Debug, Clone)]
pub struct IdGen {
    state: u64,
    counter: u64,
}

impl IdGen {
    /// Generator seeded from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            state: seed,
            counter: 0,
        }
    }

    /// Generator seeded from a device id (so devices get disjoint id streams).
    #[must_use]
    pub fn for_device(device: DeviceId) -> Self {
        let mut seed = 0_u64;
        for chunk in device.to_bytes().chunks(8) {
            let mut word = [0_u8; 8];
            word[..chunk.len()].copy_from_slice(chunk);
            seed = seed.rotate_left(17) ^ u64::from_le_bytes(word);
        }
        Self::new(seed)
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }

    fn parts(&mut self) -> (u64, u128) {
        self.counter += 1;
        let random = (u128::from(self.next_u64()) << 16) ^ u128::from(self.next_u64());
        (1_700_000_000_000 + self.counter, random & ((1 << 80) - 1))
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

    /// Fresh random-looking `u64` (seeds).
    pub fn seed(&mut self) -> u64 {
        self.next_u64()
    }
}

impl IdSource for IdGen {
    fn next_id(&mut self) -> ItemId {
        let (ts, random) = self.parts();
        ItemId(OpId::from_parts(ts, random).to_bytes())
    }
}

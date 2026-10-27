//! Deterministic id generation for tests: unique per seed, no RNG dependency. A thin wrapper over
//! [`penwise_proto::IdMint`] that advances a fake clock by 1 ms per id.

use penwise_core::item::{IdSource, ItemId};
use penwise_proto::{DeviceId, IdMint, NotebookId, OpId, PageId};

const BASE_MS: u64 = 1_700_000_000_000;

/// Seeded id source. Two generators with different seeds never collide in practice.
#[derive(Debug, Clone)]
pub struct IdGen {
    mint: IdMint,
    counter: u64,
}

impl IdGen {
    /// Generator seeded from `seed`.
    #[must_use]
    pub fn new(seed: u64) -> Self {
        Self {
            mint: IdMint::from_seed(seed),
            counter: 0,
        }
    }

    /// Generator seeded from a device id (so devices get disjoint id streams).
    #[must_use]
    pub fn for_device(device: DeviceId) -> Self {
        Self {
            mint: IdMint::new(device, 0),
            counter: 0,
        }
    }

    fn tick(&mut self) -> &mut IdMint {
        self.counter += 1;
        self.mint.set_now(BASE_MS + self.counter);
        &mut self.mint
    }

    /// Fresh op id.
    pub fn op(&mut self) -> OpId {
        self.tick().op()
    }

    /// Fresh notebook id.
    pub fn notebook(&mut self) -> NotebookId {
        self.tick().notebook()
    }

    /// Fresh page id.
    pub fn page(&mut self) -> PageId {
        self.tick().page()
    }

    /// Fresh device id.
    pub fn device(&mut self) -> DeviceId {
        self.tick().device()
    }

    /// Fresh random-looking `u64` (seeds).
    pub fn seed(&mut self) -> u64 {
        self.mint.next_u64()
    }
}

impl IdSource for IdGen {
    fn next_id(&mut self) -> ItemId {
        self.tick().next_id()
    }
}

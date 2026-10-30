//! Per-frame bookkeeping: who may capture, when a frame was last asked for, whether it is stale, and the
//! negative cache of failed captures.
//!
//! Capture traffic is bounded by construction. A frame has one gate, so one capture runs at a time. Every
//! request records when it *asked*; a request that finds a capture that *started after it asked* simply takes
//! that capture's result, so any number of waiting requests and triggers cost at most one more capture (one
//! running + one pending). A capture that failed is remembered for [`FAILURE_TTL`]: asking again answers the
//! same error at once instead of opening another page that will fail the same way.

use std::sync::Arc;
use std::sync::PoisonError;
use std::time::{Duration, Instant};

use penwise_core::item::ItemId;

use super::Inner;
use super::model::Still;

/// A frame counts as being looked at for this long after its last poll; only such frames are re-shot eagerly.
pub(super) const POLLED_RECENTLY: Duration = Duration::from_secs(10);
/// How long a failed capture is answered from memory.
pub(super) const FAILURE_TTL: Duration = Duration::from_secs(20);

/// What a frame's captures have been up to.
#[derive(Debug, Default)]
pub(super) struct Slot {
    /// One capture at a time.
    pub(super) gate: Arc<tokio::sync::Mutex<()>>,
    /// Debounce generation: only the newest trigger fires.
    pub(super) generation: u64,
    /// The last time a client asked for the still.
    pub(super) polled: Option<Instant>,
    /// Something changed while nobody was looking: the next poll captures before it answers.
    pub(super) stale: bool,
    /// When the latest capture began.
    pub(super) started: Option<Instant>,
    pub(super) failed: Option<(Instant, String)>,
}

impl Inner {
    pub(super) fn with_slot<R>(&self, frame: ItemId, work: impl FnOnce(&mut Slot) -> R) -> R {
        let mut slots = self.slots.lock().unwrap_or_else(PoisonError::into_inner);
        work(slots.entry(frame).or_default())
    }

    pub(super) fn gate(&self, frame: ItemId) -> Arc<tokio::sync::Mutex<()>> {
        self.with_slot(frame, |slot| Arc::clone(&slot.gate))
    }

    /// Records a poll; `true` when the frame went stale since its last capture.
    pub(super) fn note_poll(&self, frame: ItemId) -> bool {
        self.with_slot(frame, |slot| {
            slot.polled = Some(Instant::now());
            slot.stale
        })
    }

    pub(super) fn polled_recently(&self, frame: ItemId) -> bool {
        self.with_slot(frame, |slot| {
            slot.polled
                .is_some_and(|polled| polled.elapsed() < POLLED_RECENTLY)
        })
    }

    /// The next debounce generation of `frame`.
    pub(super) fn next_generation(&self, frame: ItemId) -> u64 {
        self.with_slot(frame, |slot| {
            slot.generation += 1;
            slot.generation
        })
    }

    /// Takes the generation if it is still the newest (the trigger that owns it may fire).
    pub(super) fn owns_generation(&self, frame: ItemId, generation: u64) -> bool {
        self.with_slot(frame, |slot| slot.generation == generation)
    }

    pub(super) fn mark_stale(&self, frame: ItemId) {
        self.with_slot(frame, |slot| slot.stale = true);
    }

    pub(super) fn clear_failure(&self, frame: ItemId) {
        self.with_slot(frame, |slot| slot.failed = None);
    }

    /// The cached still when a capture started after `asked` (so it already covers what the asker wanted).
    pub(super) fn covered(&self, frame: ItemId, asked: Instant) -> Option<Arc<Still>> {
        let newer = self.with_slot(frame, |slot| {
            slot.started.is_some_and(|began| began > asked)
        });
        if newer { self.cache().get(frame) } else { None }
    }

    /// The message of a capture failure still inside [`FAILURE_TTL`], and the seconds left of it.
    pub(super) fn recent_failure(&self, frame: ItemId) -> Option<(String, u64)> {
        self.with_slot(frame, |slot| {
            let (at, message) = slot.failed.as_ref()?;
            let left = FAILURE_TTL.checked_sub(at.elapsed())?;
            Some((message.clone(), left.as_secs().max(1)))
        })
    }

    pub(super) fn begin(&self, frame: ItemId) {
        self.with_slot(frame, |slot| {
            slot.started = Some(Instant::now());
            slot.stale = false;
        });
    }

    pub(super) fn fail(&self, frame: ItemId, message: String) {
        self.with_slot(frame, |slot| slot.failed = Some((Instant::now(), message)));
    }

    /// Drops everything about a deleted frame: its slot (gate included) and its still.
    pub(super) fn forget(&self, frame: ItemId) {
        self.slots
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .remove(&frame);
        self.flips().remove(&frame);
        self.cache().forget(frame);
    }
}

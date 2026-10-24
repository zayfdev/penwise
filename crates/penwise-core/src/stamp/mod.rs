//! Stamp brushes: deterministic dab placement along the path (ink-rendering.md §3).
//!
//! Owner: slice c.
//!
//! Dabs are geometry (bit-identical across targets); dab masks, coverage buffers and grain sampling are raster
//! (`raster::stamps`). Randomness comes only from [`prng`], keyed by `(seed, dab_index, channel)`, so batch and
//! streaming give identical dabs regardless of push chunking. Paper grain is the procedural [`texture`].
//!
//! Only [`StampEngine`]'s pub API, [`prng`] and [`texture`] are cross-slice contract; `spacing`/`dab` items are
//! `pub(super)` and slice c may reshape them.

mod dab;
pub mod prng;
mod spacing;
pub mod texture;

use crate::brush::{BrushParams, StampKind};
use crate::dynamics::InkPoint;
use crate::geometry::Dab;

/// Safety cap on dabs (spray: particles) per stroke; emission stops silently beyond it.
pub const MAX_DABS_PER_STROKE: u32 = 200_000;

/// Incremental stamp engine for one stroke.
///
/// Stable vs tentative (§4.3): dabs on segments between committed points are final and appended to the stable
/// list in order; dabs on the segment from the last committed point to the pending tail are tentative and
/// rebuilt by [`StampEngine::update_tail`] from a copy of the spacing state (phase and dab index are not
/// advanced by tentative work).
///
/// Slice c may add private fields.
#[derive(Debug, Clone, PartialEq)]
pub struct StampEngine {
    kind: StampKind,
    params: BrushParams,
    seed: u64,
    /// Last committed point (segment start for the next commit).
    prev: Option<InkPoint>,
    spacing: spacing::SpacingState,
    stable: Vec<Dab>,
    tentative: Vec<Dab>,
}

impl StampEngine {
    /// New engine for `kind` with `params` (copied) and the stroke `seed`.
    #[must_use]
    pub fn new(params: &BrushParams, kind: StampKind, seed: u64) -> Self {
        todo!()
    }

    /// Commits final points: the first point ever emits the first dab at arc position 0; each following point
    /// walks the straight segment from the previous point with the spacing accumulator
    /// `dφ = Δs / max(spacing·D, 0.5 px) + Δt·dabs_per_second` (Δt clamped to ≤ 50 ms, from sample timestamps
    /// only), emitting a dab (or a spray burst) at every integer crossing of φ. Inputs lerp linearly along the
    /// segment. Exact-duplicate positions still advance the temporal term. Stops emitting at
    /// [`MAX_DABS_PER_STROKE`]. Chunk-invariant: `extend(a ++ b)` ≡ `extend(a); extend(b)`.
    pub fn extend(&mut self, committed: &[InkPoint]) {
        todo!()
    }

    /// Rebuilds the tentative dabs for the segment from the last committed point to `tail` (cleared when
    /// `None`) without changing committed state.
    pub fn update_tail(&mut self, tail: Option<&InkPoint>) {
        todo!()
    }

    /// Final dabs so far, in emission order. Never shrinks or changes.
    #[must_use]
    pub fn stable(&self) -> &[Dab] {
        todo!()
    }

    /// Current tentative dabs (replaced on every `update_tail`).
    #[must_use]
    pub fn tentative(&self) -> &[Dab] {
        todo!()
    }

    /// Commits `last` (if any) exactly as `extend(&[last])` would, and returns all dabs in emission order.
    /// No points ever → empty `Vec`; one point → exactly the `s = 0` dab (or burst).
    #[must_use]
    pub fn finish(self, last: Option<&InkPoint>) -> Vec<Dab> {
        todo!()
    }
}

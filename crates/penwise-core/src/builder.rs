//! Incremental stroke building: quantized samples in, stable + tentative geometry out (ink-rendering.md §4.3).
//!
//! Owner: slice a.
//!
//! Pipeline per sample: [`Sample::quantize`] → [`Smoother::filter`] → [`Dynamics::apply`] → engine
//! ([`OutlineEngine`] or [`StampEngine`]).
//!
//! **Hold-back rule:** the newest filtered point is kept as the *pending tail*. Every earlier point is
//! *committed* to the engine with `extend`; the tail is shown through `update_tail` (tentative). On
//! [`StrokeBuilder::finish`] the tail's position is replaced by its raw (quantized, unfiltered) position —
//! perfect-freehand's `last` behaviour, so strokes end where the pen left — and committed via the engine's
//! `finish`.
//!
//! **Acceptance property (chunk invariance):** for any sample sequence `S` and any split of `S` into chunks,
//! `new + push(chunk)… + finish` returns geometry whose [`StrokeGeometry::canonical_bytes`] are identical to
//! `new + push(S) + finish`. Engines must therefore keep all cross-push state (filters, arc length, dab phase,
//! dab index, outline prefixes) in fields, never recompute from chunk boundaries.

use crate::brush::BrushParams;
use crate::dynamics::{Dynamics, InkPoint};
use crate::geometry::{GeometryView, StrokeGeometry};
use crate::outline::OutlineEngine;
use crate::sample::Sample;
use crate::smoothing::Smoother;
use crate::stamp::StampEngine;

/// Family engine behind the builder.
#[derive(Debug, Clone)]
enum Engine {
    Outline(OutlineEngine),
    Stamp(StampEngine),
}

/// Builds one stroke incrementally. Allocation happens only when internal buffers grow; `push` reuses them.
#[derive(Debug, Clone)]
pub struct StrokeBuilder {
    params: BrushParams,
    seed: u64,
    dynamics: Dynamics,
    smoother: Smoother,
    engine: Engine,
    /// Newest filtered point (not yet committed) and its raw quantized position.
    tail: Option<(InkPoint, [f32; 2])>,
    /// Scratch for points committed in one `push` (reused).
    committed: Vec<InkPoint>,
}

impl StrokeBuilder {
    /// Starts a stroke with `params` (copied) and the per-stroke PRNG `seed` (stored with the stroke; stamp
    /// jitter/scatter are pure functions of `(seed, dab_index, channel)`). Selects [`OutlineEngine`] for
    /// `BrushFamily::Outline` and [`StampEngine`] for `BrushFamily::Stamp`.
    #[must_use]
    pub fn new(params: &BrushParams, seed: u64) -> Self {
        todo!()
    }

    /// Appends samples in order. Each sample is quantized ([`Sample::quantize`]), filtered and run through the
    /// dynamics; all but the newest resulting point are committed to the engine (one `extend` call per push),
    /// then the engine's tentative part is recomputed from the new tail (`update_tail`). Empty `samples` is a
    /// no-op. Timestamps are taken as given (non-increasing `t_us` is handled by the filter's Δt guard).
    pub fn push(&mut self, samples: &[Sample]) {
        todo!()
    }

    /// Current live geometry: committed (stable) parts plus the tentative tail. Before the first sample it is an
    /// empty view of the right family. Never allocates.
    #[must_use]
    pub fn geometry(&self) -> GeometryView<'_> {
        todo!()
    }

    /// Ends the stroke: the pending tail is committed with its raw position (see module docs) and the engine
    /// finalizes caps/tapers. A stroke with exactly one sample yields a dot (outline) or a single dab at
    /// `s = 0` (stamp); a stroke with no samples yields empty geometry (no points / no dabs) with the brush
    /// style. Style = `{ params.color, params.opacity, params.blend }`.
    #[must_use]
    pub fn finish(self) -> StrokeGeometry {
        todo!()
    }

    /// Batch convenience: `new(params, seed)`, one `push(samples)`, `finish()`.
    #[must_use]
    pub fn build(params: &BrushParams, seed: u64, samples: &[Sample]) -> StrokeGeometry {
        let mut builder = Self::new(params, seed);
        builder.push(samples);
        builder.finish()
    }

    /// The brush this stroke is built with.
    #[must_use]
    pub fn params(&self) -> &BrushParams {
        &self.params
    }

    /// The stroke's PRNG seed.
    #[must_use]
    pub fn seed(&self) -> u64 {
        self.seed
    }
}

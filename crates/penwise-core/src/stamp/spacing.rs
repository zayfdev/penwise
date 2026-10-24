//! Dab placement along arc length and time (ink-rendering.md §3.2).
//!
//! Owner: slice c.
//!
//! Slice-internal (`pub(super)`); slice c may reshape these items.

use crate::dynamics::InkPoint;

/// Smallest spatial step between dabs, px (stops tiny dabs exploding the dab count).
pub(super) const MIN_STEP_PX: f32 = 0.5;
/// Largest Δt counted by the temporal term per segment, µs (a lost frame can't dump a blob).
pub(super) const MAX_GAP_US: u32 = 50_000;

/// Accumulator carried across segments and `push` calls.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SpacingState {
    /// Fractional phase toward the next dab, `0 ≤ phase < 1`.
    pub phase: f32,
    /// Index of the next dab to emit (keys the PRNG).
    pub next_index: u64,
}

impl SpacingState {
    pub(super) const START: Self = Self {
        phase: 0.0,
        next_index: 0,
    };
}

/// Spacing configuration.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct SpacingRule {
    /// Fraction of the reference diameter.
    pub spacing: f32,
    /// Temporal emission rate (0 = off).
    pub dabs_per_second: f32,
}

/// Where and with which inputs to stamp one dab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DabSite {
    /// Input point lerped to the dab position (x, y, pressures, speed, tilt, t lerped; azimuth via
    /// shortest-arc lerp).
    pub point: InkPoint,
    /// Dab index (PRNG key).
    pub index: u64,
}

/// Walks segment `a → b`. Spatial phase uses `Δs / max(spacing·D, MIN_STEP_PX)` where `D` lerps between
/// `diameters[0]` (at `a`) and `diameters[1]` (at `b`) and is evaluated at the current dab (reference diameter:
/// minor axis for elliptical dabs); temporal phase adds `min(Δt, MAX_GAP_US)·1e-6·dabs_per_second`, spread
/// along the segment proportionally to its parameter. For each integer crossing of the phase, calls
/// `emit(site)`; stops early when `emit` returns `false`. Updates `state` (phase, next index). A zero-length,
/// zero-time segment emits nothing.
pub(super) fn walk_segment(
    state: &mut SpacingState,
    a: &InkPoint,
    b: &InkPoint,
    rule: &SpacingRule,
    diameters: [f32; 2],
    emit: &mut dyn FnMut(DabSite) -> bool,
) {
    todo!()
}

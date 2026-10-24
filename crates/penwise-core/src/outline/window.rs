//! Initial-pressure window (ink-rendering.md §1.4 deviation of perfect-freehand's first-10-points fold).
//!
//! Owner: slice b.
//!
//! Points with index `< 10` or arc length `< 2·size` form the window (a prefix: both bounds are monotone).
//! The upstream fold `acc ← (acc + p_k)/2` (starting at `p_0`) runs over it, and every window point uses
//! `max(p_k, acc)`: lifts the near-zero first contact samples with real pressure (`simulatePressure = false`).
//! The fold is final once a committed point lies past the window; until then the window prefix is tentative.

use super::freehand::{INITIAL_PRESSURE_POINTS, StrokePoint};
use crate::float::max;

/// Fold state of the window.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct PressureWindow {
    pub acc: f32,
    /// A point past the window was added: `acc` is final.
    pub closed: bool,
    /// `2·size`, px of arc.
    arc: f64,
}

impl PressureWindow {
    /// Empty window for a brush of mean diameter `size`.
    #[must_use]
    pub(super) fn new(size: f32) -> Self {
        Self {
            acc: 0.0,
            closed: false,
            arc: f64::from(2.0 * size),
        }
    }

    /// Point `index` lies inside the window.
    #[must_use]
    pub(super) fn contains(&self, index: usize, p: &StrokePoint) -> bool {
        index < INITIAL_PRESSURE_POINTS || p.running_length < self.arc
    }

    /// Feeds centerline point `index` (in order, starting at 0).
    pub(super) fn add(&mut self, index: usize, p: &StrokePoint) {
        if self.closed {
            return;
        }
        if !self.contains(index, p) {
            self.closed = true;
        } else if index == 0 {
            self.acc = p.pressure;
        } else {
            self.acc = (self.acc + p.pressure) * 0.5;
        }
    }

    /// Pressure used for point `index`: `max(p, acc)` inside the window, raw `p` after it.
    #[must_use]
    pub(super) fn pressure(&self, index: usize, p: &StrokePoint) -> f32 {
        if self.contains(index, p) {
            max(p.pressure, self.acc)
        } else {
            p.pressure
        }
    }
}

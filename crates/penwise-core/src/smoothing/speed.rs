//! Windowed pen-speed estimate (ink-rendering.md §4.2), independent of the One Euro filter.

use super::MIN_DT_S;

/// Time window of the pen-speed estimate, µs (ink-rendering.md §4.2).
pub const SPEED_WINDOW_US: u32 = 10_000;

/// Samples kept for the speed window (covers 10 ms up to 1.5 kHz; faster input shortens the window).
const SPEED_HISTORY: usize = 16;

/// Pen speed from quantized sample positions, independent of the One Euro filter (whose derivative
/// `(x − x̂_prev)/Δt` mixes filtered and raw positions and overestimates speed rate-dependently).
///
/// `speed = Σ|p_k − p_{k−1}| / (t_newest − t_oldest)` over the newest samples whose timestamp lies within
/// [`SPEED_WINDOW_US`] of the newest one. The window always spans at least the last segment, and extends back
/// further while the elapsed time is still 0 (repeated timestamps). Timestamps are made monotone
/// (`t = max(t, t_prev)`); elapsed time is at least [`MIN_DT_S`]. Only `+ − × ÷ sqrt`, so bit-identical.
#[derive(Debug, Clone, PartialEq)]
pub(super) struct SpeedEstimator {
    /// Ring buffer of `(x, y, monotone t_us)`; `head` is the next write slot.
    ring: [(f32, f32, u32); SPEED_HISTORY],
    len: usize,
    head: usize,
}

impl SpeedEstimator {
    pub(super) const fn new() -> Self {
        Self {
            ring: [(0.0, 0.0, 0); SPEED_HISTORY],
            len: 0,
            head: 0,
        }
    }

    /// `k`-th newest stored sample (`k = 0` is the newest); `k < len`.
    fn back(&self, k: usize) -> (f32, f32, u32) {
        self.ring[(self.head + SPEED_HISTORY - 1 - k) % SPEED_HISTORY]
    }

    /// Records one quantized sample and returns the windowed speed, px/s (0 for the first sample).
    #[allow(
        clippy::cast_possible_truncation,
        reason = "elapsed seconds fit f32; computed in f64 for an exact µs → s product"
    )]
    pub(super) fn push(&mut self, x: f32, y: f32, t_us: u32) -> f32 {
        let t = if self.len == 0 {
            t_us
        } else {
            u32::max(t_us, self.back(0).2)
        };
        self.ring[self.head] = (x, y, t);
        self.head = (self.head + 1) % SPEED_HISTORY;
        self.len = usize::min(self.len + 1, SPEED_HISTORY);
        if self.len < 2 {
            return 0.0;
        }
        let mut arc = 0.0_f32;
        let mut elapsed_us = 0_u32;
        let mut newer = (x, y);
        for k in 1..self.len {
            let (px, py, pt) = self.back(k);
            let age = t - pt;
            if k > 1 && age > SPEED_WINDOW_US && elapsed_us > 0 {
                break;
            }
            let (dx, dy) = (newer.0 - px, newer.1 - py);
            arc += (dx * dx + dy * dy).sqrt();
            elapsed_us = age;
            newer = (px, py);
        }
        arc / crate::float::max((f64::from(elapsed_us) * 1e-6) as f32, MIN_DT_S)
    }
}

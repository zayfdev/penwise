//! Dab placement along arc length and time (ink-rendering.md §3.2).
//!
//! Owner: slice c.
//!
//! Slice-internal (`pub(super)`); slice c may reshape these items.

use core::f32::consts::{PI, TAU};

use crate::dynamics::InkPoint;
use crate::float::{lerp, max, min};

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

impl SpacingRule {
    /// Spatial step `max(spacing·D, MIN_STEP_PX)` for reference diameter `d` (NaN → `MIN_STEP_PX`).
    fn step(self, d: f32) -> f32 {
        max(MIN_STEP_PX, self.spacing * d)
    }

    /// Effective spacing as a fraction of `d` (`step / d`), the exponent of the flow compensation; falls back to
    /// the raw `spacing` for a degenerate `d`.
    pub(super) fn fraction(self, d: f32) -> f32 {
        let f = self.step(d) / d;
        if d > 0.0 && f.is_finite() {
            f
        } else {
            self.spacing
        }
    }
}

/// Where and with which inputs to stamp one dab.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DabSite {
    /// Input point lerped to the dab position (x, y, pressures, speed, tilt, t lerped; azimuth via
    /// shortest-arc lerp).
    pub point: InkPoint,
    /// Dab index (PRNG key).
    pub index: u64,
    /// Effective spacing fraction at this dab ([`SpacingRule::fraction`] of the reference diameter), used to
    /// normalize flow so accumulated alpha does not depend on spacing.
    pub spacing: f32,
}

impl DabSite {
    /// Site for `point` with reference diameter `d`.
    pub(super) fn new(point: InkPoint, index: u64, rule: SpacingRule, d: f32) -> Self {
        Self {
            point,
            index,
            spacing: rule.fraction(d),
        }
    }
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
    rule: SpacingRule,
    diameters: [f32; 2],
    emit: &mut dyn FnMut(DabSite) -> bool,
) {
    let dx = b.x - a.x;
    let dy = b.y - a.y;
    let len = (dx * dx + dy * dy).sqrt();
    let temporal = temporal_phase(a.t_us, b.t_us, rule.dabs_per_second);
    // Segment parameter of the last dab on this segment (or its start).
    let mut u = 0.0f32;
    loop {
        let d = lerp(diameters[0], diameters[1], u);
        // Phase gained per unit of segment parameter, with D frozen at the current dab.
        let rate = len / rule.step(d) + temporal;
        let advances = rate.is_finite() && rate > 0.0;
        if !advances {
            return;
        }
        let need = 1.0 - state.phase;
        let available = rate * (1.0 - u);
        if need > available {
            state.phase += available;
            return;
        }
        u = min(u + need / rate, 1.0);
        state.phase = 0.0;
        let d_site = lerp(diameters[0], diameters[1], u);
        let site = DabSite::new(lerp_point(a, b, u), state.next_index, rule, d_site);
        state.next_index = state.next_index.wrapping_add(1);
        if !emit(site) {
            return;
        }
    }
}

/// Temporal phase of one segment: `min(Δt, MAX_GAP_US)·1e-6·dabs_per_second` (0 when off or time runs
/// backwards).
#[allow(clippy::cast_precision_loss, reason = "Δt ≤ 50 000 µs, exact in f32")]
fn temporal_phase(t0: u32, t1: u32, dabs_per_second: f32) -> f32 {
    let on = dabs_per_second.is_finite() && dabs_per_second > 0.0;
    if !on {
        return 0.0;
    }
    let dt = t1.saturating_sub(t0).min(MAX_GAP_US);
    dt as f32 * 1e-6 * dabs_per_second
}

/// Input point at parameter `u` of `a → b`.
fn lerp_point(a: &InkPoint, b: &InkPoint, u: f32) -> InkPoint {
    InkPoint {
        x: lerp(a.x, b.x, u),
        y: lerp(a.y, b.y, u),
        t_us: lerp_time(a.t_us, b.t_us, u),
        pressure: lerp(a.pressure, b.pressure, u),
        size_pressure: lerp(a.size_pressure, b.size_pressure, u),
        alpha_factor: lerp(a.alpha_factor, b.alpha_factor, u),
        speed: lerp(a.speed, b.speed, u),
        tilt: lerp(a.tilt, b.tilt, u),
        azimuth: lerp_angle(a.azimuth, b.azimuth, u),
    }
}

/// Timestamp at `u` (holds `t0` when time does not advance).
#[allow(
    clippy::cast_possible_truncation,
    clippy::cast_sign_loss,
    reason = "0 ≤ offset ≤ t1 − t0 fits u32; saturating cast"
)]
fn lerp_time(t0: u32, t1: u32, u: f32) -> u32 {
    if t1 <= t0 {
        return t0;
    }
    let offset = f64::from(t1 - t0) * f64::from(u);
    t0.saturating_add(offset.round() as u32)
}

/// Shortest-arc angle lerp, result in `(−π, π]`.
fn lerp_angle(a: f32, b: f32, u: f32) -> f32 {
    let mut delta = b - a;
    if delta > PI {
        delta -= TAU;
    } else if delta < -PI {
        delta += TAU;
    }
    let mut r = a + delta * u;
    if r > PI {
        r -= TAU;
    } else if r <= -PI {
        r += TAU;
    }
    r
}

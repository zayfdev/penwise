//! perfect-freehand port: centerline points and offset/cap geometry (ink-rendering.md §1, with §1.4 deviations).
//!
//! Owner: slice b.
//!
//! Derived from perfect-freehand v1.2.3 (<https://github.com/steveruizok/perfect-freehand>), MIT License:
//!
//! Copyright (c) 2021 Stephen Ruiz Ltd
//!
//! Permission is hereby granted, free of charge, to any person obtaining a copy of this software and
//! associated documentation files (the "Software"), to deal in the Software without restriction, including
//! without limitation the rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell
//! copies of the Software, and to permit persons to whom the Software is furnished to do so, subject to the
//! following conditions:
//!
//! The above copyright notice and this permission notice shall be included in all copies or substantial
//! portions of the Software.
//!
//! THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT
//! LIMITED TO THE WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO
//! EVENT SHALL THE AUTHORS OR COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER
//! IN AN ACTION OF CONTRACT, TORT OR OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE
//! USE OR OTHER DEALINGS IN THE SOFTWARE.
//!
//! Penwise deviations (§1.4): `streamline = 0` (One Euro smoothing upstream), `simulatePressure = false`,
//! min-length gate `min(size, 1 px)`, initial pressure averaged over `max(10 points, 2·size px of arc)`,
//! numeric tapers only, `sqrt(dx² + dy²)` instead of `hypot`, `libm::sincosf` for rotations.
//! Streaming also drops upstream's 2-point → 5-point interpolation (the point count is unknown while drawing)
//! and measures the min-length gate as the straight distance from the first point. Ported from later upstream
//! (`isVeryShort`): a round-tip stroke of 2+ points shorter than [`END_NOISE_THRESHOLD`] renders the single-point
//! dot at its first point instead of a cap built from its last point.
//! Slice-internal: every item is `pub(super)` and may be reshaped by slice b.

use crate::brush::{BrushParams, CapStyle};
use crate::float::{clamp01, min};

/// Upstream `FIXED_PI = π + 0.0001` (kept for parity with upstream fixtures).
pub(super) const FIXED_PI: f32 = core::f32::consts::PI + 0.000_1;
pub(super) const START_CAP_SEGMENTS: u16 = 13;
pub(super) const END_CAP_SEGMENTS: u16 = 29;
pub(super) const CORNER_CAP_SEGMENTS: u16 = 13;
/// px of arc at the end skipped as noise (unless last point).
pub(super) const END_NOISE_THRESHOLD: f32 = 3.0;
pub(super) const MIN_RADIUS: f32 = 0.01;
/// Penwise min-length gate cap (px).
pub(super) const MIN_LENGTH_GATE: f32 = 1.0;
/// Minimum initial-pressure window, points.
pub(super) const INITIAL_PRESSURE_POINTS: usize = 10;

/// perfect-freehand options derived from [`BrushParams`].
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct FreehandOptions {
    /// `(size_min + size_max) / 2` (diameter).
    pub size: f32,
    /// `(size_max − size_min) / (size_max + size_min)`; 0 when the sum is 0.
    pub thinning: f32,
    /// `outline.smoothing`.
    pub smoothing: f32,
    pub taper_start: f32,
    pub taper_end: f32,
    pub cap_start: CapStyle,
    pub cap_end: CapStyle,
    /// `min(size, MIN_LENGTH_GATE)`.
    pub min_length: f32,
}

/// One centerline point (upstream `StrokePoint`). `vector` points **backwards** (current → previous), unit.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct StrokePoint {
    pub point: [f32; 2],
    /// `InkPoint::size_pressure`.
    pub pressure: f32,
    pub vector: [f32; 2],
    /// Distance to the previous kept point.
    pub distance: f32,
    /// Cumulative arc length, `f64` accumulator (converted with `as f32` where used).
    pub running_length: f64,
}

impl FreehandOptions {
    /// Maps brush params per §1.3: `size = (Dmin+Dmax)/2`, `thinning = (Dmax−Dmin)/(Dmax+Dmin)`, tapers/caps/
    /// smoothing from `params.outline`, `min_length = min(size, 1)`. Non-finite or negative lengths become 0.
    #[must_use]
    pub(super) fn from_params(params: &BrushParams) -> Self {
        let d_min = non_negative(params.size_min);
        let d_max = non_negative(params.size_max);
        let sum = d_min + d_max;
        let size = sum * 0.5;
        let thinning = if sum > 0.0 {
            (d_max - d_min) / sum
        } else {
            0.0
        };
        let o = &params.outline;
        Self {
            size,
            thinning,
            smoothing: non_negative(o.smoothing),
            taper_start: non_negative(o.taper_start),
            taper_end: non_negative(o.taper_end),
            cap_start: o.cap_start,
            cap_end: o.cap_end,
            min_length: min(size, MIN_LENGTH_GATE),
        }
    }

    /// Radius at pressure `p`: `size · (0.5 − thinning·(0.5 − p))` (identity easing; linear in p), or `size/2`
    /// when `thinning == 0`. `p` is clamped to `[0, 1]`; not clamped to `MIN_RADIUS` here (tapers do that).
    #[must_use]
    pub(super) fn radius(&self, p: f32) -> f32 {
        if self.thinning == 0.0 {
            return self.size * 0.5;
        }
        self.size * (0.5 - self.thinning * (0.5 - clamp01(p)))
    }

    /// Taper multiplier at arc length `l` with `remaining` px of arc to the stroke end: upstream easings
    /// `t(2 − t)` (start) and `(t − 1)³ + 1` (end); 1 outside both taper zones.
    #[must_use]
    pub(super) fn taper(&self, l: f32, remaining: f32) -> f32 {
        let ts = if l < self.taper_start {
            let t = l / self.taper_start;
            t * (2.0 - t)
        } else {
            1.0
        };
        let te = if remaining < self.taper_end {
            let t = remaining / self.taper_end - 1.0;
            t * t * t + 1.0
        } else {
            1.0
        };
        min(ts, te)
    }

    /// `minDist² = (size · smoothing)²`: outline points closer than this to the previous one are dropped.
    #[must_use]
    pub(super) fn min_dist_sq(&self) -> f32 {
        let d = self.size * self.smoothing;
        d * d
    }
}

impl StrokePoint {
    /// The first point of a stroke (`vector` is fixed up to the second point's vector by the reader).
    #[must_use]
    pub(super) fn first(point: [f32; 2], pressure: f32) -> Self {
        Self {
            point,
            pressure: clamp01(pressure),
            vector: [0.0, 0.0],
            distance: 0.0,
            running_length: 0.0,
        }
    }

    /// The point after `self` at `point`, or `None` when it coincides with `self` (upstream exact-equality
    /// dedupe; a zero distance is treated the same).
    #[must_use]
    pub(super) fn next(&self, point: [f32; 2], pressure: f32) -> Option<Self> {
        let dx = self.point[0] - point[0];
        let dy = self.point[1] - point[1];
        let distance = (dx * dx + dy * dy).sqrt();
        // Exact equality ⇔ zero distance for finite grid points; NaN/inf never become stroke points.
        if distance <= 0.0 || !distance.is_finite() {
            return None;
        }
        Some(Self {
            point,
            pressure: clamp01(pressure),
            vector: [dx / distance, dy / distance],
            distance,
            running_length: self.running_length + f64::from(distance),
        })
    }
}

/// Arc-length accumulator → `f32` (the documented `as f32` conversion point).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "arc lengths are accumulated in f64 and used as f32 (core-determinism.md)"
)]
pub(super) fn arc_f32(x: f64) -> f32 {
    x as f32
}

/// `per(A) = (A.y, −A.x)`.
#[must_use]
pub(super) fn per(a: [f32; 2]) -> [f32; 2] {
    [a[1], -a[0]]
}

/// Rotates `a` about `c` by `r` radians (`libm::sincosf`).
#[must_use]
pub(super) fn rot_around(a: [f32; 2], c: [f32; 2], r: f32) -> [f32; 2] {
    let (s, co) = libm::sincosf(r);
    let px = a[0] - c[0];
    let py = a[1] - c[1];
    let nx = px * co - py * s;
    let ny = px * s + py * co;
    [nx + c[0], ny + c[1]]
}

/// Appends the round start cap: `right0` rotated about `first` by `FIXED_PI·t`, `t = k/13, k = 1..=13`.
pub(super) fn round_start_cap(first: [f32; 2], right0: [f32; 2], out: &mut Vec<[f32; 2]>) {
    let n = f32::from(START_CAP_SEGMENTS);
    for k in 1..=START_CAP_SEGMENTS {
        out.push(rot_around(right0, first, FIXED_PI * (f32::from(k) / n)));
    }
}

/// Appends the round end cap: `s = last + dir·r`, `dir = per(−v_last)`, points
/// `rot_around(s, last, 3·FIXED_PI·t)` for `t = k/29, k = 1..29` (exclusive of 1; 1.5 turns covers sharp end turns).
pub(super) fn round_end_cap(last: [f32; 2], v_last: [f32; 2], r: f32, out: &mut Vec<[f32; 2]>) {
    let dir = per([-v_last[0], -v_last[1]]);
    let s = [last[0] + dir[0] * r, last[1] + dir[1] * r];
    let n = f32::from(END_CAP_SEGMENTS);
    for k in 1..END_CAP_SEGMENTS {
        out.push(rot_around(s, last, FIXED_PI * 3.0 * (f32::from(k) / n)));
    }
}

/// Appends a flat cap at `center` across offset `o` (= `left − right` half vector): upstream flat start cap
/// `center − 0.5c, center − 0.51c, center + 0.51c, center + 0.5c` with `c = 2·o`.
pub(super) fn flat_cap(center: [f32; 2], o: [f32; 2], out: &mut Vec<[f32; 2]>) {
    let corners = [o[0] * 2.0, o[1] * 2.0];
    let inner = [corners[0] * 0.5, corners[1] * 0.5];
    let outer = [corners[0] * 0.51, corners[1] * 0.51];
    out.push([center[0] - inner[0], center[1] - inner[1]]);
    out.push([center[0] - outer[0], center[1] - outer[1]]);
    out.push([center[0] + outer[0], center[1] + outer[1]]);
    out.push([center[0] + inner[0], center[1] + inner[1]]);
}

/// Appends a single-point dot: 13 points `rot_around([center.x + r, center.y], center, 2·FIXED_PI·k/13)` for
/// `k = 0..13`; `r` is clamped to `≥ MIN_RADIUS`.
pub(super) fn dot(center: [f32; 2], r: f32, out: &mut Vec<[f32; 2]>) {
    let r = if r > MIN_RADIUS { r } else { MIN_RADIUS };
    let start = [center[0] + r, center[1]];
    let n = f32::from(START_CAP_SEGMENTS);
    for k in 0..START_CAP_SEGMENTS {
        out.push(rot_around(
            start,
            center,
            FIXED_PI * 2.0 * (f32::from(k) / n),
        ));
    }
}

/// `x` if finite and positive, else 0.
#[must_use]
pub(super) fn non_negative(x: f32) -> f32 {
    if x.is_finite() && x > 0.0 { x } else { 0.0 }
}

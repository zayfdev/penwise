//! Nib models: elliptic fountain tipping (§2.2) and flat calligraphy/highlighter nibs (§2.1).
//!
//! Owner: slice b.
//!
//! Slice-internal: every item is `pub(super)` and may be reshaped by slice b. Transcendentals via `libm`.

use core::f32::consts::{FRAC_PI_2, PI, TAU};

use super::freehand::per;
use crate::brush::NibParams;
use crate::float::lerp;

/// Direction smoother: EMA on the forward unit vector by arc length, `w = 1 − expf(−Δs/r)`, renormalized
/// (stops nib-width flicker on jitter). Exact-zero results keep the previous direction.
#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct DirectionSmoother {
    dir: [f32; 2],
    initialized: bool,
}

impl DirectionSmoother {
    #[must_use]
    pub(super) fn new() -> Self {
        Self {
            dir: [1.0, 0.0],
            initialized: false,
        }
    }

    /// Feeds forward unit direction `d` after `ds` px of arc with current radius `r`; returns the smoothed unit
    /// direction. First call returns `d`. `r ≤ 0` → `w = 1`.
    pub(super) fn update(&mut self, d: [f32; 2], ds: f32, r: f32) -> [f32; 2] {
        if !self.initialized {
            self.initialized = true;
            if d[0] != 0.0 || d[1] != 0.0 {
                self.dir = d;
            }
            return self.dir;
        }
        let w = if r > 0.0 {
            1.0 - libm::expf(-ds / r)
        } else {
            1.0
        };
        let x = lerp(self.dir[0], d[0], w);
        let y = lerp(self.dir[1], d[1], w);
        let len = (x * x + y * y).sqrt();
        if len > 0.0 && len.is_finite() {
            self.dir = [x / len, y / len];
        }
        self.dir
    }
}

/// Fountain radius multiplier for forward direction `d` (unit):
/// `n = per(d)`, `u = (cos φ, sin φ)`, `u⊥ = per(u)`, `k = sqrt((u·n)² + a²·(u⊥·n)²)` ∈ `[a, 1]`;
/// returns `k / ((1 + a)/2)` (average width ≈ pressure width). `a = nib.aspect`, `φ = nib.angle`.
#[must_use]
pub(super) fn fountain_scale(d: [f32; 2], nib: &NibParams) -> f32 {
    let aspect = nib.aspect;
    let norm = (1.0 + aspect) * 0.5;
    if norm <= 0.0 || !norm.is_finite() {
        return 1.0;
    }
    let normal = per(d);
    let (sin, cos) = libm::sincosf(nib.angle);
    let axis = [cos, sin];
    let un = dot(axis, normal);
    let upn = dot(per(axis), normal);
    let support = (un * un + aspect * aspect * (upn * upn)).sqrt();
    support / norm
}

/// Flat nib width from curved pressure: `W = lerp(size_min, size_max, c)` (highlighter: `size_min == size_max`).
#[must_use]
pub(super) fn nib_width(size_min: f32, size_max: f32, c: f32) -> f32 {
    lerp(size_min, size_max, c)
}

/// Effective nib angle with fixation: `φ_eff = lerp_angle(θ + π/2, φ, fixation)`, `θ = atan2(d.y, d.x)`,
/// shortest-arc angle interpolation; `fixation == 1` returns `φ` exactly (no trig).
#[must_use]
pub(super) fn effective_angle(d: [f32; 2], nib: &NibParams) -> f32 {
    let f = nib.fixation;
    if f >= 1.0 || f.is_nan() {
        return nib.angle;
    }
    let free = libm::atan2f(d[1], d[0]) + FRAC_PI_2;
    let mut diff = (nib.angle - free) % TAU;
    if diff > PI {
        diff -= TAU;
    } else if diff <= -PI {
        diff += TAU;
    }
    free + diff * f
}

/// Corners of the flat nib rectangle at `p` (§2.1): `p ± (W/2)·u ± (t/2)·u⊥`, `u = (cos φ, sin φ)`,
/// `u⊥ = per(u)`, in the order `(+,+), (−,+), (−,−), (+,−)`.
///
/// The flat-nib sweep is the union of the convex hulls of consecutive rectangles (`sweep.rs`), so its width
/// across the motion normal `n` is the rectangle's support `W·|u·n| + t·|u⊥·n|`
/// (= `W·|sin(θ − φ)| + t·|cos(θ − φ)|`) whichever way the stroke crosses the nib.
#[must_use]
pub(super) fn nib_corners(p: [f32; 2], width: f32, thickness: f32, phi: f32) -> [[f32; 2]; 4] {
    let (sn, cs) = libm::sincosf(phi);
    let u = [cs, sn];
    let up = per(u);
    let hw = width * 0.5;
    let ht = thickness * 0.5;
    [(hw, ht), (-hw, ht), (-hw, -ht), (hw, -ht)]
        .map(|(a, b)| [p[0] + a * u[0] + b * up[0], p[1] + a * u[1] + b * up[1]])
}

/// Single-point flat-nib dot: the nib rectangle at the nib angle `φ` (4 points, [`nib_corners`] order).
pub(super) fn nib_rect(p: [f32; 2], width: f32, nib: &NibParams, out: &mut Vec<[f32; 2]>) {
    out.extend_from_slice(&nib_corners(p, width, nib.thickness, nib.angle));
}

fn dot(a: [f32; 2], b: [f32; 2]) -> f32 {
    a[0] * b[0] + a[1] * b[1]
}

#[cfg(test)]
mod tests;

//! Nib models: elliptic fountain tipping (§2.2) and flat calligraphy/highlighter nibs (§2.1).
//!
//! Owner: slice b.
//!
//! Slice-internal: every item is `pub(super)` and may be reshaped by slice b. Transcendentals via `libm`.

use crate::brush::NibParams;

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
        todo!()
    }
}

/// Fountain radius multiplier for forward direction `d` (unit):
/// `n = per(d)`, `u = (cos φ, sin φ)`, `u⊥ = per(u)`, `k = sqrt((u·n)² + a²·(u⊥·n)²)` ∈ `[a, 1]`;
/// returns `k / ((1 + a)/2)` (average width ≈ pressure width). `a = nib.aspect`, `φ = nib.angle`.
#[must_use]
pub(super) fn fountain_scale(d: [f32; 2], nib: &NibParams) -> f32 {
    todo!()
}

/// Flat nib width from curved pressure: `W = lerp(size_min, size_max, c)` (highlighter: `size_min == size_max`).
#[must_use]
pub(super) fn nib_width(size_min: f32, size_max: f32, c: f32) -> f32 {
    todo!()
}

/// Effective nib angle with fixation: `φ_eff = lerp_angle(θ + π/2, φ, fixation)`, `θ = atan2(d.y, d.x)`,
/// shortest-arc angle interpolation; `fixation == 1` returns `φ` exactly (no trig).
#[must_use]
pub(super) fn effective_angle(d: [f32; 2], nib: &NibParams) -> f32 {
    todo!()
}

/// Left/right ribbon points for a flat nib at `p` (§2.1): `u = (cos φ_eff, sin φ_eff)`, `u⊥ = per(u)`,
/// `s = sign(d·u⊥)` (+1 when 0); `left = p + (W/2)·u + s·(t/2)·u⊥`, `right = p − (W/2)·u − s·(t/2)·u⊥`,
/// `t = nib.thickness`. Returns `(left, right)`.
#[must_use]
pub(super) fn flat_offsets(
    p: [f32; 2],
    d: [f32; 2],
    width: f32,
    phi_eff: f32,
    nib: &NibParams,
) -> ([f32; 2], [f32; 2]) {
    todo!()
}

//! Pressure curve, velocity and tilt response (ink-rendering.md §3.3, §3.8, §4.2, §5).
//!
//! Owner: slice a.
//!
//! Turns a [`FilteredSample`] into an [`InkPoint`]: the per-point input both brush families consume.
//! Transcendentals via `libm` only (`powf`).

use crate::float::{clamp01, smoothstep};
use crate::smoothing::FilteredSample;

/// Pressure curve `c(p)`: `x = clamp((p − floor)/(saturation − floor), 0, 1)`, `c = x^gamma`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct PressureCurve {
    /// Pressure at or below which `c = 0` (default 0.02 ≈ raw 80; kills near-zero contact samples).
    pub floor: f32,
    /// Pressure at or above which `c = 1` (default 0.90 ≈ raw 3700).
    pub saturation: f32,
    /// Exponent (`> 0`); default 0.75.
    pub gamma: f32,
}

impl PressureCurve {
    /// Research default: floor 0.02, saturation 0.90, γ 0.75.
    pub const DEFAULT: Self = Self {
        floor: 0.02,
        saturation: 0.90,
        gamma: 0.75,
    };
    /// Pressure ignored: `c(p) = 1` for every p (highlighter).
    pub const CONSTANT: Self = Self {
        floor: 0.0,
        saturation: 0.0,
        gamma: 1.0,
    };

    /// `c(p)` in `0..=1`. Exact rules:
    /// - NaN `p` → treat as 0.
    /// - `saturation ≤ floor` → step: `c = 1` if `p ≥ floor`, else 0 (so [`Self::CONSTANT`] gives 1 for every
    ///   `p ≥ 0`).
    /// - otherwise `x = clamp01((p − floor)/(saturation − floor))`; `x == 0` → 0, `x == 1` → 1 (no `powf` call),
    ///   else `libm::powf(x, gamma)`.
    #[must_use]
    #[allow(clippy::float_cmp, reason = "exact endpoints skip powf by contract")]
    pub fn eval(&self, p: f32) -> f32 {
        let p = if p.is_nan() { 0.0 } else { p };
        if self.saturation <= self.floor {
            return if p >= self.floor { 1.0 } else { 0.0 };
        }
        let x = clamp01((p - self.floor) / (self.saturation - self.floor));
        if x == 0.0 {
            0.0
        } else if x == 1.0 {
            1.0
        } else {
            libm::powf(x, self.gamma)
        }
    }
}

/// Speed-dependent thinning/lightening: `factor = 1 − k·smoothstep(v0, v1, speed)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VelocityResponse {
    /// Speed (px/s) where the effect starts (default 300).
    pub v0: f32,
    /// Speed (px/s) of full effect (default 3000).
    pub v1: f32,
    /// `k_v` on size/width (outline brushes: ballpoint 0.1, fountain 0.25).
    pub size: f32,
    /// `k_v` on dab alpha (stamp brushes: pencil 0.15).
    pub opacity: f32,
}

impl VelocityResponse {
    /// No velocity response.
    pub const NONE: Self = Self {
        v0: 300.0,
        v1: 3000.0,
        size: 0.0,
        opacity: 0.0,
    };

    /// Size multiplier `1 − size·smoothstep(v0, v1, speed)` ([`crate::float::smoothstep`]).
    #[must_use]
    pub fn size_factor(&self, speed: f32) -> f32 {
        1.0 - self.size * smoothstep(self.v0, self.v1, speed)
    }

    /// Alpha multiplier `1 − opacity·smoothstep(v0, v1, speed)`.
    #[must_use]
    pub fn opacity_factor(&self, speed: f32) -> f32 {
        1.0 - self.opacity * smoothstep(self.v0, self.v1, speed)
    }
}

/// Tilt side-shading response (stamp brushes, §3.8). `s = smoothstep(theta0, theta1, tilt)`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct TiltResponse {
    /// Tilt (rad) where side shading starts (default 45°).
    pub theta0: f32,
    /// Tilt (rad) of full side shading (default 65°).
    pub theta1: f32,
    /// Max dab aspect `A_max` (≥ 1; 1 = no elongation).
    pub max_aspect: f32,
    /// Max radius scale `K_max` (≥ 1; 1 = no growth).
    pub max_radius_scale: f32,
    /// Dab hardness at full side contact.
    pub side_hardness: f32,
    /// Alpha multiplier at full side contact (default 0.5).
    pub side_alpha: f32,
}

impl TiltResponse {
    /// No tilt response (`max_aspect = max_radius_scale = 1`, side values neutral).
    pub const NONE: Self = Self {
        theta0: 45.0 * DEG,
        theta1: 65.0 * DEG,
        max_aspect: 1.0,
        max_radius_scale: 1.0,
        side_hardness: 1.0,
        side_alpha: 1.0,
    };

    /// Side-contact amount `s = smoothstep(theta0, theta1, tilt)` in `0..=1`.
    #[must_use]
    pub fn side(&self, tilt: f32) -> f32 {
        smoothstep(self.theta0, self.theta1, tilt)
    }

    /// True when this response never changes a dab (`max_aspect == 1 && max_radius_scale == 1` and
    /// `side_alpha == 1`); lets the stamp engine skip the tilt path.
    #[must_use]
    #[allow(clippy::float_cmp, reason = "neutral values are exact constants")]
    pub fn is_none(&self) -> bool {
        self.max_aspect == 1.0 && self.max_radius_scale == 1.0 && self.side_alpha == 1.0
    }
}

/// Degrees → radians factor (`π/180` as `f32`).
pub const DEG: f32 = core::f32::consts::PI / 180.0;

/// All dynamics of a brush.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Dynamics {
    pub pressure: PressureCurve,
    pub velocity: VelocityResponse,
    pub tilt: TiltResponse,
}

/// One point after smoothing and dynamics: the unit both brush families consume.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct InkPoint {
    /// Page px.
    pub x: f32,
    pub y: f32,
    /// Microseconds since stroke start.
    pub t_us: u32,
    /// Curved pressure `c(p)` in `0..=1` (no velocity applied).
    pub pressure: f32,
    /// `c(p) · velocity.size_factor(speed)`: drives size/width.
    pub size_pressure: f32,
    /// `velocity.opacity_factor(speed)`: multiplies stamp dab alpha (1 for outline brushes).
    pub alpha_factor: f32,
    /// Pen speed, px/s ([`FilteredSample::speed`]: windowed, from quantized positions).
    pub speed: f32,
    /// Radians, as [`crate::Sample::tilt`].
    pub tilt: f32,
    /// Radians, as [`crate::Sample::azimuth`].
    pub azimuth: f32,
}

impl Dynamics {
    /// Applies the curve and velocity response to one filtered sample:
    /// `pressure = self.pressure.eval(f.pressure)`, `size_pressure = pressure · velocity.size_factor(f.speed)`,
    /// `alpha_factor = velocity.opacity_factor(f.speed)`; position/time/tilt/azimuth/speed copied from `f`
    /// (filtered `x, y`, not `raw_x, raw_y`).
    #[must_use]
    pub fn apply(&self, f: &FilteredSample) -> InkPoint {
        let pressure = self.pressure.eval(f.pressure);
        InkPoint {
            x: f.x,
            y: f.y,
            t_us: f.t_us,
            pressure,
            size_pressure: pressure * self.velocity.size_factor(f.speed),
            alpha_factor: self.velocity.opacity_factor(f.speed),
            speed: f.speed,
            tilt: f.tilt,
            azimuth: f.azimuth,
        }
    }
}

#[cfg(test)]
mod tests;

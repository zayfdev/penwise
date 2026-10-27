//! Range checks for untrusted [`BrushParams`] (decoded from sync ops or stored blobs).

use super::{BrushParams, GrainParams, OutlineParams, SprayParams, StampParams};
use crate::dynamics::Dynamics;
use crate::sample::POSITION_LIMIT_PX;
use crate::smoothing::{OneEuroParams, SmoothingParams};

/// Largest accepted dimensionless gain/exponent/ratio (aspect, scale, gamma, grain scale, ...).
const GAIN_LIMIT: f32 = 64.0;
/// Largest accepted `dabs_per_second` / `particles_max`: bounds the dab count a hostile stroke can request.
const DABS_PER_SECOND_LIMIT: f32 = 1_000.0;
const PARTICLES_LIMIT: u32 = 1_024;

impl BrushParams {
    /// True when every float is finite and in its documented range, so geometry built from these params is
    /// finite and bounded: `opacity ∈ 0..=1`; `0 < size_min ≤ size_max ≤ 2²⁰`; `size_gamma ∈ (0, 64]`; the
    /// pressure curve, velocity and tilt responses, One Euro filters, outline and stamp params inside the
    /// ranges noted on their fields (fractions `0..=1`, lengths `0..=2²⁰`, exponents `(0, 64]`, tilt angles in
    /// `0..=π`). Never panics.
    #[must_use]
    pub fn is_valid(&self) -> bool {
        unit(self.opacity)
            && positive(self.size_min, POSITION_LIMIT_PX)
            && positive(self.size_max, POSITION_LIMIT_PX)
            && self.size_min <= self.size_max
            && positive(self.size_gamma, GAIN_LIMIT)
            && dynamics_ok(&self.dynamics)
            && smoothing_ok(&self.smoothing)
            && outline_ok(&self.outline)
            && stamp_ok(&self.stamp)
    }
}

/// Finite and within `0..=1`.
fn unit(v: f32) -> bool {
    (0.0..=1.0).contains(&v)
}

/// Finite and within `(0, max]`.
fn positive(v: f32, max: f32) -> bool {
    v > 0.0 && v <= max
}

/// Finite and within `0..=max`.
fn bounded(v: f32, max: f32) -> bool {
    (0.0..=max).contains(&v)
}

fn dynamics_ok(d: &Dynamics) -> bool {
    let (p, v, t) = (&d.pressure, &d.velocity, &d.tilt);
    unit(p.floor)
        && unit(p.saturation)
        && positive(p.gamma, GAIN_LIMIT)
        && bounded(v.v0, 1.0e6)
        && bounded(v.v1, 1.0e6)
        && unit(v.size)
        && unit(v.opacity)
        && bounded(t.theta0, core::f32::consts::PI)
        && bounded(t.theta1, core::f32::consts::PI)
        && (1.0..=GAIN_LIMIT).contains(&t.max_aspect)
        && (1.0..=GAIN_LIMIT).contains(&t.max_radius_scale)
        && unit(t.side_hardness)
        && unit(t.side_alpha)
}

fn one_euro_ok(p: &OneEuroParams) -> bool {
    positive(p.min_cutoff_hz, POSITION_LIMIT_PX)
        && bounded(p.beta, POSITION_LIMIT_PX)
        && positive(p.d_cutoff_hz, POSITION_LIMIT_PX)
}

fn smoothing_ok(s: &SmoothingParams) -> bool {
    one_euro_ok(&s.position) && one_euro_ok(&s.pressure)
}

fn outline_ok(o: &OutlineParams) -> bool {
    unit(o.smoothing)
        && bounded(o.taper_start, POSITION_LIMIT_PX)
        && bounded(o.taper_end, POSITION_LIMIT_PX)
        && o.nib.angle.abs() <= 1_000.0
        && positive(o.nib.aspect, GAIN_LIMIT)
        && bounded(o.nib.thickness, POSITION_LIMIT_PX)
        && unit(o.nib.fixation)
}

fn stamp_ok(s: &StampParams) -> bool {
    bounded(s.flow, GAIN_LIMIT)
        && unit(s.flow_pressure)
        && s.hardness > 0.0
        && s.hardness <= 1.0
        && positive(s.spacing, GAIN_LIMIT)
        && bounded(s.scatter, GAIN_LIMIT)
        && unit(s.jitter.size)
        && unit(s.jitter.opacity)
        && unit(s.jitter.angle)
        && bounded(s.dabs_per_second, DABS_PER_SECOND_LIMIT)
        && s.grain.as_ref().is_none_or(grain_ok)
        && s.spray.as_ref().is_none_or(spray_ok)
}

fn grain_ok(g: &GrainParams) -> bool {
    positive(g.scale, GAIN_LIMIT) && unit(g.strength) && unit(g.softness)
}

fn spray_ok(s: &SprayParams) -> bool {
    (1..=PARTICLES_LIMIT).contains(&s.particles_max)
        && positive(s.particle_size_min, POSITION_LIMIT_PX)
        && positive(s.particle_size_max, POSITION_LIMIT_PX)
        && s.particle_size_min <= s.particle_size_max
}

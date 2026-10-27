//! One Euro input filtering (ink-rendering.md §4.1): time-based, rate-independent smoothing of position and
//! pressure. Replaces perfect-freehand's per-sample `streamline`.
//!
//! Owner: slice a.
//!
//! Uses only `+ − × ÷` and `sqrt` (IEEE-exact), so it is bit-identical on every target. The filter state is
//! advanced one quantized sample at a time; feeding the same samples in any `push` chunking yields the same
//! outputs.

mod speed;
mod state;

pub use speed::SPEED_WINDOW_US;
use speed::SpeedEstimator;
pub use state::{HISTORY_LEN, HistoryPoint, PositionState, PressureState, SmootherState};

use crate::sample::Sample;

/// One Euro parameters for one signal.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct OneEuroParams {
    /// Cutoff at rest, Hz (`> 0`).
    pub min_cutoff_hz: f32,
    /// Speed coefficient, 1/px for positions (Hz per px/s: cutoff rises by `beta · speed`). 0 = plain EMA.
    pub beta: f32,
    /// Cutoff of the derivative filter, Hz (`> 0`).
    pub d_cutoff_hz: f32,
}

impl OneEuroParams {
    /// Position default: `min_cutoff 4 Hz, β 0.01 /px, d_cutoff 1 Hz`.
    pub const POSITION: Self = Self {
        min_cutoff_hz: 4.0,
        beta: 0.01,
        d_cutoff_hz: 1.0,
    };
    /// Pressure default: `min_cutoff 8 Hz, β 0` (EMA; kills 1-LSB flicker).
    pub const PRESSURE: Self = Self {
        min_cutoff_hz: 8.0,
        beta: 0.0,
        d_cutoff_hz: 1.0,
    };
}

/// Smoothing configuration of a brush.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct SmoothingParams {
    pub position: OneEuroParams,
    pub pressure: OneEuroParams,
}

impl SmoothingParams {
    /// Research defaults ([`OneEuroParams::POSITION`], [`OneEuroParams::PRESSURE`]).
    pub const DEFAULT: Self = Self {
        position: OneEuroParams::POSITION,
        pressure: OneEuroParams::PRESSURE,
    };
}

/// Smallest Δt used by the filters, seconds (0.5 ms).
pub const MIN_DT_S: f32 = 0.000_5;

/// One Euro filter state for a 2-D point; x and y share one cutoff driven by the speed `|(dx̂, dŷ)|`.
#[derive(Debug, Clone, PartialEq)]
pub struct OneEuro {
    params: OneEuroParams,
    /// `None` until the first sample.
    prev: Option<OneEuroState>,
}

/// Internal per-filter memory (slice a may add private fields).
#[derive(Debug, Clone, Copy, PartialEq)]
struct OneEuroState {
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    t_us: u32,
    dt_s: f32,
}

impl OneEuro {
    /// Fresh filter.
    #[must_use]
    pub fn new(params: OneEuroParams) -> Self {
        Self { params, prev: None }
    }

    /// Filter resuming from `state`.
    fn resume(params: OneEuroParams, s: PositionState) -> Self {
        let prev = OneEuroState {
            x: s.x,
            y: s.y,
            dx: s.dx,
            dy: s.dy,
            t_us: s.t_us,
            dt_s: s.dt_s,
        };
        Self {
            params,
            prev: Some(prev),
        }
    }

    fn state(&self) -> Option<PositionState> {
        self.prev.map(|p| PositionState {
            x: p.x,
            y: p.y,
            dx: p.dx,
            dy: p.dy,
            t_us: p.t_us,
            dt_s: p.dt_s,
        })
    }

    /// Filters one point; returns `(x̂, ŷ, speed_px_per_s)`.
    ///
    /// - First call: returns the input unchanged with speed 0 and seeds the state (`dt = MIN_DT_S`).
    /// - `Δt = (t_us − prev.t_us)·1e-6` s computed in `u32` wrapping-free form (`t_us < prev` counts as `Δt ≤ 0`);
    ///   `Δt ≤ 0` reuses the previous Δt; then `Δt = max(Δt, MIN_DT_S)` (via [`crate::float::max`]).
    /// - `α(fc) = 1 / (1 + τ/Δt)`, `τ = 1/(2π·fc)` (π as an `f32` constant, no transcendentals).
    /// - `d = (x − x̂_prev)/Δt`; `d̂ = d̂_prev + α(d_cutoff)·(d − d̂_prev)` per axis.
    /// - `speed = sqrt(d̂x² + d̂y²)`; `fc = min_cutoff + β·speed`; `x̂ = x̂_prev + α(fc)·(x − x̂_prev)` per axis.
    pub fn filter(&mut self, x: f32, y: f32, t_us: u32) -> (f32, f32, f32) {
        let Some(prev) = self.prev else {
            self.prev = Some(OneEuroState {
                x,
                y,
                dx: 0.0,
                dy: 0.0,
                t_us,
                dt_s: MIN_DT_S,
            });
            return (x, y, 0.0);
        };
        let dt = next_dt(prev.t_us, prev.dt_s, t_us);
        let a_d = alpha(self.params.d_cutoff_hz, dt);
        let dx = prev.dx + a_d * ((x - prev.x) / dt - prev.dx);
        let dy = prev.dy + a_d * ((y - prev.y) / dt - prev.dy);
        let speed = (dx * dx + dy * dy).sqrt();
        let a = alpha(self.params.min_cutoff_hz + self.params.beta * speed, dt);
        let fx = prev.x + a * (x - prev.x);
        let fy = prev.y + a * (y - prev.y);
        self.prev = Some(OneEuroState {
            x: fx,
            y: fy,
            dx,
            dy,
            t_us,
            dt_s: dt,
        });
        (fx, fy, speed)
    }
}

/// Δt in seconds from sample timestamps: `t ≤ prev_t` reuses `prev_dt`; result is at least [`MIN_DT_S`].
#[allow(
    clippy::cast_possible_truncation,
    reason = "Δt in seconds fits f32; computed in f64 for an exact µs → s product"
)]
fn next_dt(prev_t_us: u32, prev_dt_s: f32, t_us: u32) -> f32 {
    let dt = if t_us > prev_t_us {
        (f64::from(t_us - prev_t_us) * 1e-6) as f32
    } else {
        prev_dt_s
    };
    crate::float::max(dt, MIN_DT_S)
}

/// EMA weight `1 / (1 + τ/Δt)`, `τ = 1/(2π·fc)`. `fc ≤ 0` (or NaN) means `τ = ∞`: weight 0 (hold), never NaN.
fn alpha(cutoff_hz: f32, dt_s: f32) -> f32 {
    if cutoff_hz.is_nan() || cutoff_hz <= 0.0 {
        return 0.0;
    }
    let tau = 1.0 / (2.0 * core::f32::consts::PI * cutoff_hz);
    1.0 / (1.0 + tau / dt_s)
}

/// One Euro filter for a scalar signal (pressure); same equations as [`OneEuro`] with one axis and the
/// derivative magnitude `|d̂|` as speed.
#[derive(Debug, Clone, PartialEq)]
pub struct OneEuroScalar {
    params: OneEuroParams,
    prev: Option<(f32, f32, u32, f32)>,
}

impl OneEuroScalar {
    /// Fresh filter.
    #[must_use]
    pub fn new(params: OneEuroParams) -> Self {
        Self { params, prev: None }
    }

    fn resume(params: OneEuroParams, s: PressureState) -> Self {
        Self {
            params,
            prev: Some((s.value, s.derivative, s.t_us, s.dt_s)),
        }
    }

    fn state(&self) -> Option<PressureState> {
        self.prev
            .map(|(value, derivative, t_us, dt_s)| PressureState {
                value,
                derivative,
                t_us,
                dt_s,
            })
    }

    /// Filters one value; first call returns `v` unchanged. Δt rules as in [`OneEuro::filter`].
    pub fn filter(&mut self, v: f32, t_us: u32) -> f32 {
        let Some((prev_v, prev_d, prev_t, prev_dt)) = self.prev else {
            self.prev = Some((v, 0.0, t_us, MIN_DT_S));
            return v;
        };
        let dt = next_dt(prev_t, prev_dt, t_us);
        let d = prev_d + alpha(self.params.d_cutoff_hz, dt) * ((v - prev_v) / dt - prev_d);
        let a = alpha(self.params.min_cutoff_hz + self.params.beta * d.abs(), dt);
        let fv = prev_v + a * (v - prev_v);
        self.prev = Some((fv, d, t_us, dt));
        fv
    }
}

/// A quantized sample after smoothing.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FilteredSample {
    /// Filtered position, page px.
    pub x: f32,
    pub y: f32,
    /// Filtered pressure, clamped to `0..=1`.
    pub pressure: f32,
    /// Tilt and azimuth pass through unfiltered (already quantized).
    pub tilt: f32,
    pub azimuth: f32,
    pub t_us: u32,
    /// Pen speed, px/s, from the quantized positions over the last [`SPEED_WINDOW_US`] (not the One Euro
    /// derivative).
    pub speed: f32,
    /// The unfiltered (quantized) position, used for the pen-up `last` point.
    pub raw_x: f32,
    pub raw_y: f32,
}

/// Position + pressure smoother and pen-speed estimator for one stroke.
#[derive(Debug, Clone, PartialEq)]
pub struct Smoother {
    position: OneEuro,
    pressure: OneEuroScalar,
    speed: SpeedEstimator,
}

impl Smoother {
    /// Fresh smoother for one stroke.
    #[must_use]
    pub fn new(params: &SmoothingParams) -> Self {
        Self {
            position: OneEuro::new(params.position),
            pressure: OneEuroScalar::new(params.pressure),
            speed: SpeedEstimator::new(),
        }
    }

    /// Smoother that continues from `state` as if it had filtered every earlier sample itself.
    #[must_use]
    pub fn resume(params: &SmoothingParams, state: &SmootherState) -> Self {
        Self {
            position: OneEuro::resume(params.position, state.position),
            pressure: OneEuroScalar::resume(params.pressure, state.pressure),
            speed: SpeedEstimator::from_history(&state.history),
        }
    }

    /// Memory after the samples filtered so far; `None` before the first one.
    #[must_use]
    pub fn state(&self) -> Option<SmootherState> {
        Some(SmootherState {
            position: self.position.state()?,
            pressure: self.pressure.state()?,
            history: self.speed.history(),
        })
    }

    /// Filters one already-quantized sample (position via [`OneEuro`], pressure via [`OneEuroScalar`] then
    /// clamped to `0..=1`; tilt/azimuth/t copied; `raw_x/raw_y` = input position). `speed` is the windowed
    /// pen speed of the quantized positions (see [`SPEED_WINDOW_US`]); the One Euro derivative only drives
    /// the filter's own adaptive cutoff.
    pub fn filter(&mut self, s: &Sample) -> FilteredSample {
        let (x, y, _) = self.position.filter(s.x, s.y, s.t_us);
        let pressure = crate::float::clamp01(self.pressure.filter(s.pressure, s.t_us));
        FilteredSample {
            x,
            y,
            pressure,
            tilt: s.tilt,
            azimuth: s.azimuth,
            t_us: s.t_us,
            speed: self.speed.push(s.x, s.y, s.t_us),
            raw_x: s.x,
            raw_y: s.y,
        }
    }
}

#[cfg(test)]
mod tests;

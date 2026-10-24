//! Pen samples in page px, the quantization grid, and raw digitizer → [`Sample`] mapping.
//!
//! Owner: slice a (`DigitizerRanges::map`); the grid and `quantize` are contract (frozen: they define the
//! stored stroke format).
//!
//! **Invariant (quantize-before-build):** [`crate::StrokeBuilder`] quantizes every sample it receives with
//! [`Sample::quantize`] before any other processing, and the stroke codec stores exactly that grid. `quantize`
//! is idempotent, so a stroke drawn live and the same stroke decoded from storage or a `.pwrec` replay feed the
//! builder bit-identical inputs and therefore produce bit-identical geometry.

use core::f32::consts::{FRAC_PI_2, PI};

/// Position grid: 1/16 px.
pub const POSITION_STEPS_PER_PX: f32 = 16.0;
/// Positions are clamped to `±POSITION_LIMIT_PX` (2²⁰ px) so grid values stay exact in `f32` (|k| ≤ 2²⁴).
pub const POSITION_LIMIT_PX: f32 = 1_048_576.0;
/// Pressure grid: 1/4096 (grid values `0..=4096`, so both 0.0 and 1.0 are representable).
pub const PRESSURE_STEPS: f32 = 4096.0;
/// Angle grid: π/2048 rad (≈0.088°) for both tilt and azimuth.
pub const ANGLE_STEP: f32 = PI / 2048.0;
/// Tilt grid values span `0..=TILT_GRID_MAX` (0 … π/2).
pub const TILT_GRID_MAX: u16 = 1024;
/// Azimuth grid values span `-AZIMUTH_GRID_HALF+1 ..= AZIMUTH_GRID_HALF` (−π exclusive … π inclusive).
pub const AZIMUTH_GRID_HALF: i16 = 2048;

/// One pen sample in page px (y down), the only input the ink engine accepts.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Sample {
    /// Page px.
    pub x: f32,
    /// Page px, y down.
    pub y: f32,
    /// Normalized pressure `0..=1`.
    pub pressure: f32,
    /// Radians from perpendicular to the screen, `0..=π/2` (Android `AXIS_TILT`).
    pub tilt: f32,
    /// Radians, `(−π, π]`, direction the barrel leans, clockwise from screen-up: 0 = up, π/2 = right,
    /// −π/2 = left, π = down (Android `AXIS_ORIENTATION`). 0 when the pen is upright.
    pub azimuth: f32,
    /// Microseconds since the first sample of the stroke.
    pub t_us: u32,
}

/// A sample on the integer quantization grid: exactly what the stroke codec stores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GridSample {
    /// x in 1/16 px, `|x| ≤ 2²⁴`.
    pub x: i32,
    /// y in 1/16 px, `|y| ≤ 2²⁴`.
    pub y: i32,
    /// Pressure in 1/4096, `0..=4096`.
    pub pressure: u16,
    /// Tilt in [`ANGLE_STEP`]s, `0..=1024`.
    pub tilt: u16,
    /// Azimuth in [`ANGLE_STEP`]s, `-2047..=2048`.
    pub azimuth: i16,
    /// Microseconds since stroke start.
    pub t_us: u32,
}

impl Sample {
    /// Snaps the sample onto the quantization grid (`self.to_grid().to_sample()`). Idempotent.
    #[must_use]
    pub fn quantize(self) -> Sample {
        self.to_grid().to_sample()
    }

    /// Grid representation. NaN components map to 0; positions clamp to ±[`POSITION_LIMIT_PX`]; pressure
    /// clamps to `0..=1`; tilt clamps to `0..=π/2`; azimuth wraps into `(−π, π]`. Rounding is half away from
    /// zero (`f32::round`, IEEE-exact).
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        clippy::cast_sign_loss,
        reason = "values are clamped/rounded into the target range first"
    )]
    pub fn to_grid(self) -> GridSample {
        let pos = |v: f32| {
            let v = if v.is_nan() {
                0.0
            } else {
                v.clamp(-POSITION_LIMIT_PX, POSITION_LIMIT_PX)
            };
            (v * POSITION_STEPS_PER_PX).round() as i32
        };
        let unit = |v: f32| if v.is_nan() { 0.0 } else { v.clamp(0.0, 1.0) };
        let tilt = if self.tilt.is_nan() {
            0.0
        } else {
            self.tilt.clamp(0.0, FRAC_PI_2)
        };
        let az = if self.azimuth.is_nan() {
            0.0
        } else {
            self.azimuth.clamp(-1.0e6, 1.0e6)
        };
        let az_steps = (az / ANGLE_STEP).round() as i64;
        let half = i64::from(AZIMUTH_GRID_HALF);
        let az_wrapped = (az_steps + half - 1).rem_euclid(2 * half) - (half - 1);
        GridSample {
            x: pos(self.x),
            y: pos(self.y),
            pressure: (unit(self.pressure) * PRESSURE_STEPS).round() as u16,
            tilt: ((tilt / ANGLE_STEP).round() as u16).min(TILT_GRID_MAX),
            azimuth: az_wrapped as i16,
            t_us: self.t_us,
        }
    }

    /// Unit vector in page axes (y down) pointing where the barrel leans: `(sin azimuth, −cos azimuth)`.
    /// Stamp tilt shading uses this as the research doc's lean direction α (`α = atan2(v.y, v.x)`).
    #[must_use]
    pub fn lean_vector(&self) -> [f32; 2] {
        let (s, c) = libm::sincosf(self.azimuth);
        [s, -c]
    }
}

impl GridSample {
    /// Back to page units. Exact for positions and pressure (power-of-two steps); angles are `k · ANGLE_STEP`.
    #[must_use]
    #[allow(clippy::cast_precision_loss, reason = "|grid| ≤ 2^24, exact in f32")]
    pub fn to_sample(self) -> Sample {
        Sample {
            x: self.x as f32 / POSITION_STEPS_PER_PX,
            y: self.y as f32 / POSITION_STEPS_PER_PX,
            pressure: f32::from(self.pressure) / PRESSURE_STEPS,
            tilt: f32::from(self.tilt) * ANGLE_STEP,
            azimuth: f32::from(self.azimuth) * ANGLE_STEP,
            t_us: self.t_us,
        }
    }
}

/// One raw digitizer frame (device units), e.g. a `.pwrec` source-0 sample or a `getevent` frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RawSample {
    pub x: i32,
    pub y: i32,
    pub pressure: i32,
    /// Device tilt X, assumed degrees (`sec_e-pen` reports ±63) [UNVERIFIED against `AXIS_TILT`].
    pub tilt_x_deg: i32,
    /// Device tilt Y, assumed degrees.
    pub tilt_y_deg: i32,
    /// Microseconds since stroke start.
    pub t_us: u32,
}

/// Inclusive device range of one axis.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AxisRange {
    pub min: i32,
    pub max: i32,
}

/// Rational device-units → px factor: `px = (raw − min) · num / den`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PxScale {
    pub num: u32,
    pub den: u32,
}

/// Device ranges and px mapping of a digitizer (the `.pwrec` header carries the same data).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DigitizerRanges {
    pub x: AxisRange,
    pub y: AxisRange,
    pub pressure: AxisRange,
    pub tilt_x: AxisRange,
    pub tilt_y: AxisRange,
    pub x_scale: PxScale,
    pub y_scale: PxScale,
}

impl DigitizerRanges {
    /// Galaxy Tab S9 `SM-X710` `sec_e-pen` (portrait-native axes, 1600×2560 px).
    pub const SM_X710: Self = Self {
        x: AxisRange { min: 0, max: 14752 },
        y: AxisRange { min: 0, max: 23603 },
        pressure: AxisRange { min: 0, max: 4095 },
        tilt_x: AxisRange { min: -63, max: 63 },
        tilt_y: AxisRange { min: -63, max: 63 },
        x_scale: PxScale {
            num: 1600,
            den: 14753,
        },
        y_scale: PxScale {
            num: 2560,
            den: 23604,
        },
    };

    /// Maps a raw frame to an **unquantized** [`Sample`] (the builder/codec quantize). All arithmetic in `f64`,
    /// transcendentals via `libm` (f64 variants), converted to `f32` at the end, so it is deterministic:
    /// - `x = (raw.x − x.min) · x_scale.num / x_scale.den` (same for y); out-of-range positions are not clamped.
    ///   A zero `den` yields 0.
    /// - `pressure = clamp((raw.pressure − pressure.min) / (pressure.max − pressure.min), 0, 1)`; 0 if `max ≤ min`.
    /// - Tilt: clamp each tilt to ±89°, `tx = tan(tilt_x°)`, `ty = tan(tilt_y°)`;
    ///   `tilt = atan(sqrt(tx² + ty²))` clamped to `0..=π/2`;
    ///   `azimuth = atan2(tx, −ty)` (lean direction, clockwise from screen-up, y down), and exactly `0.0` when
    ///   `tx == 0 && ty == 0` (upright; avoids `atan2(0, −0) = π`). Device +X/+Y are taken as page +x/+y.
    /// - `t_us` is copied.
    #[must_use]
    #[allow(
        clippy::cast_possible_truncation,
        reason = "f64 → f32 at the end of the mapping is the documented output precision"
    )]
    pub fn map(&self, raw: &RawSample) -> Sample {
        let pressure_span = f64::from(self.pressure.max) - f64::from(self.pressure.min);
        let pressure = if pressure_span > 0.0 {
            ((f64::from(raw.pressure) - f64::from(self.pressure.min)) / pressure_span)
                .clamp(0.0, 1.0)
        } else {
            0.0
        };
        let (tilt, azimuth) = tilt_azimuth(raw.tilt_x_deg, raw.tilt_y_deg);
        Sample {
            x: axis_px(raw.x, self.x, self.x_scale) as f32 + 0.0,
            y: axis_px(raw.y, self.y, self.y_scale) as f32 + 0.0,
            pressure: pressure as f32 + 0.0,
            tilt: tilt as f32 + 0.0,
            azimuth: azimuth as f32 + 0.0,
            t_us: raw.t_us,
        }
    }
}

/// `(raw − min) · num / den` in `f64`; 0 for a zero `den`.
fn axis_px(raw: i32, range: AxisRange, scale: PxScale) -> f64 {
    if scale.den == 0 {
        return 0.0;
    }
    (f64::from(raw) - f64::from(range.min)) * f64::from(scale.num) / f64::from(scale.den)
}

/// Device tilt X/Y degrees → (`AXIS_TILT`, `AXIS_ORIENTATION`) radians, `f64` via `libm`.
fn tilt_azimuth(tilt_x_deg: i32, tilt_y_deg: i32) -> (f64, f64) {
    const MAX_DEG: f64 = 89.0;
    let to_rad = |deg: i32| f64::from(deg).clamp(-MAX_DEG, MAX_DEG) * core::f64::consts::PI / 180.0;
    let tx = libm::tan(to_rad(tilt_x_deg));
    let ty = libm::tan(to_rad(tilt_y_deg));
    if tx == 0.0 && ty == 0.0 {
        return (0.0, 0.0);
    }
    let tilt = libm::atan((tx * tx + ty * ty).sqrt()).clamp(0.0, core::f64::consts::FRAC_PI_2);
    (tilt, libm::atan2(tx, -ty))
}

#[cfg(test)]
mod tests;

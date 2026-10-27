//! Browser pointer input → core [`Sample`]s, through the same [`DigitizerRanges::map`] as the tablet's raw pen
//! frames (`sample.rs`): tilt X/Y plane angles in degrees become `AXIS_TILT`/`AXIS_ORIENTATION` radians there,
//! never in JS. Positions are page px already and pass through; pressure is the 12-bit device scale.

use penwise_core::sample::{AxisRange, DigitizerRanges, PxScale, RawSample, Sample};
use wasm_bindgen::prelude::wasm_bindgen;

/// Floats per input event in [`pointer_samples`]: `x, y, pressure (0..1), tiltX°, tiltY°`.
pub const POINTER_STRIDE: usize = 5;
/// Floats per output sample (the `Stroke`/`stroke_push` layout): `x, y, pressure, tilt, azimuth`.
pub const SAMPLE_STRIDE: usize = 5;

const PRESSURE_MAX: i32 = 4095;
const PRESSURE_MAX_F: f32 = 4095.0;

/// Identity position scale, 12-bit pressure, ±63° tilt like the `sec_e-pen` digitizer (tilt is clamped to ±89°
/// by the mapping itself).
const RANGES: DigitizerRanges = DigitizerRanges {
    x: AxisRange { min: 0, max: 1 },
    y: AxisRange { min: 0, max: 1 },
    pressure: AxisRange {
        min: 0,
        max: PRESSURE_MAX,
    },
    tilt_x: AxisRange { min: -90, max: 90 },
    tilt_y: AxisRange { min: -90, max: 90 },
    x_scale: PxScale { num: 1, den: 1 },
    y_scale: PxScale { num: 1, den: 1 },
};

/// One browser pointer event as a [`Sample`] with `t_us` as given. `pressure` is `PointerEvent.pressure`
/// (`0..=1`), tilts are `tiltX`/`tiltY` degrees (fractions round to whole degrees like the digitizer's).
#[must_use]
#[allow(
    clippy::cast_possible_truncation,
    reason = "pressure and tilt are clamped/rounded into i32 range first"
)]
pub fn pointer_sample(
    x: f32,
    y: f32,
    pressure: f32,
    tilt_x_deg: f32,
    tilt_y_deg: f32,
    t_us: u32,
) -> Sample {
    let unit = if pressure.is_nan() {
        0.0
    } else {
        pressure.clamp(0.0, 1.0)
    };
    let degrees = |v: f32| {
        if v.is_nan() {
            0
        } else {
            v.clamp(-90.0, 90.0).round() as i32
        }
    };
    let mapped = RANGES.map(&RawSample {
        x: 0,
        y: 0,
        pressure: (unit * PRESSURE_MAX_F).round() as i32,
        tilt_x_deg: degrees(tilt_x_deg),
        tilt_y_deg: degrees(tilt_y_deg),
        t_us,
    });
    Sample { x, y, ..mapped }
}

/// Converts `events` (`POINTER_STRIDE` floats each) to samples (`SAMPLE_STRIDE` floats each, ready for
/// `Viewer.stroke_push`). Timestamps stay with the caller (`stroke_push` takes them separately).
#[wasm_bindgen]
#[must_use]
pub fn pointer_samples(events: &[f32]) -> Vec<f32> {
    let mut out = Vec::with_capacity(events.len() / POINTER_STRIDE * SAMPLE_STRIDE);
    for e in events.chunks_exact(POINTER_STRIDE) {
        let s = pointer_sample(e[0], e[1], e[2], e[3], e[4], 0);
        out.extend_from_slice(&[s.x, s.y, s.pressure, s.tilt, s.azimuth]);
    }
    out
}

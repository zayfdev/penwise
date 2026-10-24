#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::{OneEuro, OneEuroParams, OneEuroScalar, Smoother, SmoothingParams};
use crate::sample::Sample;

/// 240 Hz sample period in µs (Tab S9 digitizer rate, rounded).
const P240: u32 = 4_167;

fn t_at(i: u32, period_us: u32) -> u32 {
    i * period_us
}

#[test]
fn first_sample_passes_through() {
    let mut f = OneEuro::new(OneEuroParams::POSITION);
    assert_eq!(f.filter(12.5, -3.25, 777), (12.5, -3.25, 0.0));
    let mut p = OneEuroScalar::new(OneEuroParams::PRESSURE);
    assert_eq!(p.filter(0.375, 777), 0.375);
}

#[test]
fn constant_input_converges() {
    let mut f = OneEuro::new(OneEuroParams::POSITION);
    let mut p = OneEuroScalar::new(OneEuroParams::PRESSURE);
    f.filter(0.0, 0.0, 0);
    p.filter(0.0, 0);
    let (mut x, mut y, mut speed, mut v) = (0.0, 0.0, 0.0, 0.0);
    for i in 1..=480 {
        (x, y, speed) = f.filter(100.0, 50.0, t_at(i, P240));
        v = p.filter(0.6, t_at(i, P240));
    }
    assert!(
        (x - 100.0).abs() < 1e-2 && (y - 50.0).abs() < 1e-2,
        "({x}, {y})"
    );
    assert!(speed < 1.0, "speed {speed}");
    assert!((v - 0.6).abs() < 1e-3, "pressure {v}");
}

#[test]
fn slow_jitter_is_attenuated() {
    let mut f = OneEuro::new(OneEuroParams::POSITION);
    let mut worst: f32 = 0.0;
    for i in 0..480 {
        let jitter = if i % 2 == 0 { 0.5 } else { -0.5 };
        let (x, y, _) = f.filter(100.0 + jitter, 200.0 - jitter, t_at(i, P240));
        if i >= 240 {
            worst = crate::float::max(
                worst,
                crate::float::max((x - 100.0).abs(), (y - 200.0).abs()),
            );
        }
    }
    // ±0.5 px input jitter at rest comes out at well under a fifth of its amplitude.
    assert!(worst < 0.1, "residual jitter {worst}");
}

#[test]
fn repeated_and_backwards_timestamps_stay_finite() {
    let mut f = OneEuro::new(OneEuroParams::POSITION);
    let mut p = OneEuroScalar::new(OneEuroParams::PRESSURE);
    let inputs = [
        (0.0, 0.0, 1000),
        (10.0, 5.0, 1000),
        (20.0, 9.0, 1000),
        (30.0, 1.0, 400),
        (40.0, 2.0, 0),
    ];
    let mut last_x = 0.0;
    for (i, (x, y, t)) in inputs.into_iter().enumerate() {
        let (fx, fy, speed) = f.filter(x, y, t);
        let fp = p.filter(if i % 2 == 0 { 0.2 } else { 0.9 }, t);
        assert!(
            fx.is_finite() && fy.is_finite() && speed.is_finite() && fp.is_finite(),
            "sample {i}"
        );
        assert!(
            fx >= last_x,
            "filter must move toward the input, sample {i}"
        );
        last_x = fx;
    }
    assert!(
        last_x > 0.0,
        "Δt guard still lets the filter follow the input"
    );
}

#[test]
fn zero_cutoff_never_produces_nan() {
    let params = OneEuroParams {
        min_cutoff_hz: 0.0,
        beta: 0.0,
        d_cutoff_hz: 0.0,
    };
    let mut f = OneEuro::new(params);
    f.filter(1.0, 1.0, 0);
    let (x, y, s) = f.filter(5.0, 5.0, 0);
    assert!(x.is_finite() && y.is_finite() && s.is_finite());
}

/// Slow line `x = 200 px/s · t` filtered at 250 Hz and at 125 Hz: the time-based filter lags by nearly the same
/// amount at both rates (EMA lag ≈ `v·τ`, independent of Δt), unlike a per-sample EMA whose lag doubles.
#[test]
fn output_is_rate_independent() {
    let run = |period_us: u32, until_us: u32| {
        let mut f = OneEuro::new(OneEuroParams::POSITION);
        let mut out = (0.0, 0.0, 0.0);
        let mut i = 0;
        while t_at(i, period_us) <= until_us {
            let t = t_at(i, period_us);
            #[allow(clippy::cast_precision_loss, reason = "t < 2^24 µs")]
            let x = 200.0 * (t as f32) * 1e-6;
            out = f.filter(x, 0.0, t);
            i += 1;
        }
        out
    };
    let fast = run(4_000, 1_000_000);
    let slow = run(8_000, 1_000_000);
    assert!(fast.2.is_finite() && slow.2.is_finite() && fast.2 > 0.0 && slow.2 > 0.0);
    assert!(
        (fast.0 - slow.0).abs() < 1.0,
        "250 Hz {} vs 125 Hz {}",
        fast.0,
        slow.0
    );
    assert!(
        fast.0 < 200.0 && fast.0 > 195.0,
        "lag stays a few px: {}",
        fast.0
    );
}

#[test]
fn smoother_clamps_pressure_and_keeps_raw_position() {
    let mut s = Smoother::new(&SmoothingParams::DEFAULT);
    let first = Sample {
        x: 10.0,
        y: 20.0,
        pressure: 0.5,
        tilt: 0.25,
        azimuth: -1.0,
        t_us: 0,
    };
    let out = s.filter(&first);
    assert_eq!(
        (out.x, out.y, out.pressure, out.speed),
        (10.0, 20.0, 0.5, 0.0)
    );
    let second = Sample {
        x: 30.0,
        pressure: 1.0,
        t_us: P240,
        ..first
    };
    let out = s.filter(&second);
    assert_eq!((out.raw_x, out.raw_y), (30.0, 20.0));
    assert!(out.x > 10.0 && out.x < 30.0);
    assert!(out.pressure > 0.5 && out.pressure <= 1.0);
    assert_eq!((out.tilt, out.azimuth, out.t_us), (0.25, -1.0, P240));
}

#[test]
fn smoother_speed_is_finite_for_repeated_and_backwards_timestamps() {
    let mut s = Smoother::new(&SmoothingParams::DEFAULT);
    let inputs = [
        (0.0, 0.0, 1000),
        (10.0, 5.0, 1000),
        (20.0, 9.0, 1000),
        (30.0, 1.0, 400),
        (40.0, 2.0, 0),
        (40.0, 2.0, 0),
        (50.0, 2.0, 30_000),
        (60.0, 2.0, 20_000),
    ];
    for (i, (x, y, t_us)) in inputs.into_iter().enumerate() {
        let out = s.filter(&Sample {
            x,
            y,
            pressure: 0.5,
            tilt: 0.0,
            azimuth: 0.0,
            t_us,
        });
        assert!(
            out.speed.is_finite() && out.speed >= 0.0,
            "sample {i}: {}",
            out.speed
        );
    }
}

/// After a gap longer than the window, the speed falls back to the last segment.
#[test]
fn smoother_speed_falls_back_to_last_segment() {
    let mut s = Smoother::new(&SmoothingParams::DEFAULT);
    let at = |x: f32, t_us: u32| Sample {
        x,
        y: 0.0,
        pressure: 0.5,
        tilt: 0.0,
        azimuth: 0.0,
        t_us,
    };
    assert_eq!(s.filter(&at(0.0, 0)).speed, 0.0);
    let out = s.filter(&at(10.0, 50_000));
    assert!((out.speed - 200.0).abs() < 1e-3, "speed {}", out.speed);
}

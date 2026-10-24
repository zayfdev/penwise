#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::{DEG, Dynamics, PressureCurve, TiltResponse, VelocityResponse};
use crate::sample::Sample;
use crate::smoothing::{FilteredSample, Smoother, SmoothingParams};

fn close(a: f32, b: f32, tol: f32) -> bool {
    (a - b).abs() <= tol
}

/// `c(p)` for the default curve: `x = (p − 0.02)/0.88`, `c = x^0.75`.
/// (ink-rendering.md §5 quotes ≈0.43/0.60/0.88 for these inputs; the formula itself gives the values below.)
#[test]
fn default_curve_table() {
    let c = PressureCurve::DEFAULT;
    let table = [
        (0.32, 0.446_148),
        (0.49, 0.624_757),
        (0.50, 0.634_701),
        (0.80, 0.913_501),
    ];
    for (p, want) in table {
        assert!(
            close(c.eval(p), want, 1e-4),
            "c({p}) = {} want {want}",
            c.eval(p)
        );
    }
}

#[test]
fn curve_floor_saturation_and_endpoints() {
    let c = PressureCurve::DEFAULT;
    assert_eq!(c.eval(0.0), 0.0);
    assert_eq!(c.eval(0.02), 0.0);
    assert_eq!(c.eval(-1.0), 0.0);
    assert_eq!(c.eval(f32::NAN), 0.0);
    assert_eq!(c.eval(0.90), 1.0);
    assert_eq!(c.eval(1.0), 1.0);
    assert_eq!(c.eval(f32::INFINITY), 1.0);
    // Monotone between floor and saturation.
    let mut last = 0.0;
    for i in 0..=100u8 {
        let v = c.eval(f32::from(i) / 100.0);
        assert!(v >= last && (0.0..=1.0).contains(&v));
        last = v;
    }
}

#[test]
fn constant_curve_ignores_pressure() {
    let c = PressureCurve::CONSTANT;
    for p in [0.0, 0.001, 0.5, 1.0] {
        assert_eq!(c.eval(p), 1.0);
    }
    assert_eq!(c.eval(f32::NAN), 1.0);
}

#[test]
fn velocity_thinning() {
    let v = VelocityResponse {
        size: 0.25,
        opacity: 0.15,
        ..VelocityResponse::NONE
    };
    assert_eq!(v.size_factor(0.0), 1.0);
    assert_eq!(v.size_factor(300.0), 1.0);
    assert_eq!(v.size_factor(3000.0), 0.75);
    assert_eq!(v.size_factor(1.0e9), 0.75);
    assert!(close(v.opacity_factor(5000.0), 0.85, 1e-6));
    let mid = v.size_factor(1650.0);
    assert!(close(mid, 0.875, 1e-6), "midpoint {mid}");
    assert_eq!(VelocityResponse::NONE.size_factor(10_000.0), 1.0);
    assert_eq!(VelocityResponse::NONE.opacity_factor(10_000.0), 1.0);
}

#[test]
fn tilt_side_contact() {
    let t = TiltResponse {
        max_aspect: 3.0,
        ..TiltResponse::NONE
    };
    assert_eq!(t.side(0.0), 0.0);
    assert_eq!(t.side(45.0 * DEG), 0.0);
    assert_eq!(t.side(65.0 * DEG), 1.0);
    assert_eq!(t.side(core::f32::consts::FRAC_PI_2), 1.0);
    assert!(close(t.side(55.0 * DEG), 0.5, 1e-5));
    assert!(TiltResponse::NONE.is_none());
    assert!(!t.is_none());
    let alpha_only = TiltResponse {
        side_alpha: 0.5,
        ..TiltResponse::NONE
    };
    assert!(!alpha_only.is_none());
}

#[test]
fn apply_combines_curve_and_velocity() {
    let d = Dynamics {
        pressure: PressureCurve::DEFAULT,
        velocity: VelocityResponse {
            size: 0.1,
            opacity: 0.2,
            ..VelocityResponse::NONE
        },
        tilt: TiltResponse::NONE,
    };
    let f = FilteredSample {
        x: 1.5,
        y: 2.5,
        pressure: 0.90,
        tilt: 0.3,
        azimuth: -0.7,
        t_us: 42,
        speed: 4000.0,
        raw_x: 9.0,
        raw_y: 9.0,
    };
    let p = d.apply(&f);
    assert_eq!((p.x, p.y, p.t_us), (1.5, 2.5, 42));
    assert_eq!((p.tilt, p.azimuth, p.speed), (0.3, -0.7, 4000.0));
    assert_eq!(p.pressure, 1.0);
    assert!(close(p.size_pressure, 0.9, 1e-6));
    assert!(close(p.alpha_factor, 0.8, 1e-6));
}

/// Steady straight diagonal line at `speed_px_s`, quantized samples at `rate_hz` (rounded µs timestamps), run
/// through [`Smoother`] and [`Dynamics::apply`]; returns the dynamics speed of every sample with
/// `50 ms ≤ t ≤ 150 ms` (mid-stroke of a 200 ms stroke).
#[allow(
    clippy::cast_precision_loss,
    reason = "timestamps < 2^24 µs are exact in f32"
)]
fn mid_stroke_speeds(speed_px_s: f32, rate_hz: u32) -> Vec<f32> {
    let d = Dynamics {
        pressure: PressureCurve::DEFAULT,
        velocity: VelocityResponse::NONE,
        tilt: TiltResponse::NONE,
    };
    let mut smoother = Smoother::new(&SmoothingParams::DEFAULT);
    let mut out = Vec::new();
    let mut i = 0_u32;
    loop {
        let t_us = (i * 1_000_000 + rate_hz / 2) / rate_hz;
        if t_us > 200_000 {
            break;
        }
        let s = speed_px_s * t_us as f32 * 1e-6;
        let sample = Sample {
            x: 100.0 + 0.6 * s,
            y: 100.0 + 0.8 * s,
            pressure: 0.5,
            tilt: 0.0,
            azimuth: 0.0,
            t_us,
        }
        .quantize();
        let p = d.apply(&smoother.filter(&sample));
        if (50_000..=150_000).contains(&t_us) {
            out.push(p.speed);
        }
        i += 1;
    }
    out
}

#[test]
fn dynamics_speed_matches_true_speed_at_any_rate() {
    for speed in [200.0_f32, 1500.0] {
        for rate in [125, 250, 482] {
            let speeds = mid_stroke_speeds(speed, rate);
            assert!(!speeds.is_empty());
            for v in speeds {
                assert!(
                    close(v, speed, 0.1 * speed),
                    "{speed} px/s at {rate} Hz read as {v}"
                );
            }
        }
    }
}

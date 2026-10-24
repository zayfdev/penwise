#![allow(
    clippy::float_cmp,
    reason = "exact outputs are the contract under test"
)]

use super::{canon, clamp01, lerp, max, min, smoothstep};

const POS_ZERO: u32 = 0x0000_0000;
const NEG_ZERO: u32 = 0x8000_0000;

#[test]
fn min_max_pick_the_ordered_operand() {
    let pairs = [
        (1.0, 2.0),
        (-3.5, 0.25),
        (-1.0, -1.0e-30),
        (f32::NEG_INFINITY, 7.0),
    ];
    for (lo, hi) in pairs {
        assert_eq!(min(lo, hi), lo);
        assert_eq!(min(hi, lo), lo);
        assert_eq!(max(lo, hi), hi);
        assert_eq!(max(hi, lo), hi);
    }
}

#[test]
fn min_max_return_the_first_operand_on_signed_zero_ties() {
    assert_eq!(min(0.0, -0.0).to_bits(), POS_ZERO);
    assert_eq!(min(-0.0, 0.0).to_bits(), NEG_ZERO);
    assert_eq!(max(0.0, -0.0).to_bits(), POS_ZERO);
    assert_eq!(max(-0.0, 0.0).to_bits(), NEG_ZERO);
}

#[test]
fn min_max_ignore_a_nan_second_operand() {
    assert_eq!(min(2.0, f32::NAN), 2.0);
    assert_eq!(max(2.0, f32::NAN), 2.0);
}

#[test]
fn clamp01_clamps_and_maps_nan_to_zero() {
    let cases = [
        (-1.0, 0.0),
        (0.0, 0.0),
        (0.25, 0.25),
        (1.0, 1.0),
        (3.0, 1.0),
        (f32::INFINITY, 1.0),
        (f32::NEG_INFINITY, 0.0),
    ];
    for (x, want) in cases {
        assert_eq!(clamp01(x), want, "{x}");
    }
    assert_eq!(clamp01(f32::NAN).to_bits(), POS_ZERO);
    assert_eq!(clamp01(-0.0).to_bits(), POS_ZERO);
}

#[test]
fn lerp_hits_endpoints_and_midpoint() {
    assert_eq!(lerp(2.0, 6.0, 0.0), 2.0);
    assert_eq!(lerp(2.0, 6.0, 1.0), 6.0);
    assert_eq!(lerp(2.0, 6.0, 0.5), 4.0);
    assert_eq!(lerp(2.0, 6.0, 1.5), 8.0);
    assert_eq!(lerp(6.0, 2.0, 0.25), 5.0);
}

#[test]
fn smoothstep_is_a_clamped_hermite_ramp() {
    assert_eq!(smoothstep(1.0, 3.0, 0.0), 0.0);
    assert_eq!(smoothstep(1.0, 3.0, 1.0), 0.0);
    assert_eq!(smoothstep(1.0, 3.0, 2.0), 0.5);
    assert_eq!(smoothstep(1.0, 3.0, 3.0), 1.0);
    assert_eq!(smoothstep(1.0, 3.0, 9.0), 1.0);
    // t = 0.25 → 0.0625 · 2.5.
    assert_eq!(smoothstep(0.0, 4.0, 1.0), 0.156_25);
    assert_eq!(smoothstep(0.0, 1.0, f32::NAN), 0.0);
}

#[test]
fn smoothstep_degenerates_to_a_step_when_edges_cross() {
    assert_eq!(smoothstep(2.0, 2.0, 1.999), 0.0);
    assert_eq!(smoothstep(2.0, 2.0, 2.0), 1.0);
    assert_eq!(smoothstep(2.0, 1.0, 1.5), 0.0);
    assert_eq!(smoothstep(2.0, 1.0, 5.0), 1.0);
    assert_eq!(smoothstep(2.0, 1.0, f32::NAN), 0.0);
}

#[test]
fn canon_normalizes_zero_and_rejects_non_finite() {
    assert_eq!(canon(-0.0).map(f32::to_bits), Some(POS_ZERO));
    assert_eq!(canon(0.0).map(f32::to_bits), Some(POS_ZERO));
    assert_eq!(canon(-1.5), Some(-1.5));
    assert_eq!(canon(f32::MAX), Some(f32::MAX));
    assert_eq!(
        canon(f32::MIN_POSITIVE / 2.0),
        Some(f32::MIN_POSITIVE / 2.0)
    );
    assert_eq!(canon(f32::NAN), None);
    assert_eq!(canon(f32::INFINITY), None);
    assert_eq!(canon(f32::NEG_INFINITY), None);
}
